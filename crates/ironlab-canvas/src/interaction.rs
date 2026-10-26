//! Pointer interaction as typed edits recorded in a view overlay.
//!
//! This module is pure logic with no GPU or windowing dependency, so every gesture can be unit tested. All positions
//! are in figure space (points, origin at the top-left corner of the figure, y down); the canvas converts screen
//! positions with [`crate::ScreenTransform::invert`] before calling in. Geometry comes from the [`HitMap`] of the most
//! recent compilation of the displayed figure.
//!
//! # The source, the overlay and the displayed figure
//!
//! As decided in ADR 0008, [`FigureState`] holds the figure as its owner defines it (the **source**, which for the
//! viewer is the figure as opened or last saved), an [`Overlay`] of the sets the user has made, and the **displayed
//! figure**, which is the composition of the two. A gesture never mutates the source: it builds a [`Transaction`] of
//! sets and records it in the overlay with [`FigureState::record`], after which the displayed figure is composed
//! again. Composition is the only expensive step, so it runs when the overlay changes rather than on every frame.
//!
//! Limits are set through [`ironlab_ir::command::set_limits`], so that the transaction carries the limits of every
//! axes of the link group and a client that applies it reaches the same figure. Three-dimensional views and artist
//! visibility are set literally, one property per edit.
//!
//! An entry that the figure cannot show (for example limits that are not increasing) is dropped when the displayed
//! figure is composed. Such an entry is removed from the overlay and its reason is reported by
//! [`FigureState::problems`], which the toolbar shows alongside the warnings of the scene compiler.
//!
//! The property editor commits through [`FigureState::try_record`], which refuses such a change before it is
//! recorded, so that the figure is left as it was and no undo step is spent; [`FigureState::begin_edit_step`] and
//! [`FigureState::end_edit_step`] make a drag of a numeric field one step, as a drag on the canvas is.
//!
//! # Semantics
//!
//! - **Wheel zoom** (2D) rescales the x and y limits of the axes under the pointer about the data point under the
//!   pointer, in the axis' scale space (linear values, or base-10 logarithms for log axes), so that this data point
//!   stays under the pointer. A factor greater than one zooms in. In 3D it multiplies `projection.view3d.zoom` by the
//!   factor.
//! - **Pan** (2D) shifts the limits so that the data point grabbed at the start of the drag stays under the pointer.
//!   Every update is computed from the axis maps captured at the start of the drag rather than incrementally, so
//!   repeated updates never accumulate rounding drift. In 3D, pan moves `projection.view3d.pan_x` and
//!   `projection.view3d.pan_y` by the pointer displacement as fractions of the plot rectangle: `pan_x` increases to
//!   the right and `pan_y` increases downwards (figure-space y), so the projected box follows the pointer.
//! - **Rotate** (3D) follows MATLAB's `rotate3d`, in which the object follows the pointer: dragging right by `dx`
//!   points decreases the azimuth by `dx ·` [`ROTATE_DEGREES_PER_POINT`], and dragging down (positive figure-space
//!   `dy`) increases the elevation by `dy ·` [`ROTATE_DEGREES_PER_POINT`]. The elevation is clamped to `[-90, 90]`;
//!   the azimuth is not wrapped. A Rotate-tool drag on a 2D axes does nothing.
//! - **Box zoom** (Zoom tool, 2D) records a rubber band from the drag start to the pointer, clamped to the plot
//!   rectangle, and on release sets the x and y limits to the data range the band covers. A band narrower or shorter
//!   than [`MIN_BOX_ZOOM_POINTS`] is ignored, so that an accidental click does not zoom to a sliver.
//! - **Double click** removes the view entries (limits and three-dimensional view) of the axes under the pointer from
//!   the overlay, so that it shows the source again, and sets the axes linked with it to the limits restored. On a
//!   legend entry, the second click of a double-click is a click like the first, so a double-click toggles the artist
//!   twice, leaves its visibility as it was and does not restore any limits.
//! - **Click** on a legend entry sets `visible` on its artist and selects it; a click elsewhere inside an axes
//!   selects that axes and changes nothing. The selection is what the property editor shows, and no gesture
//!   changes it.
//! - **Refit** removes the view entries of every axes and the limits of every colour scale, but keeps visibility,
//!   because hiding a plot is a choice about content rather than about the view.
//! - **Datatips** change nothing: [`FigureState::tip_at`] names what the colourbar under the pointer reads, or else
//!   the drawn point nearest the pointer within [`DATATIP_RADIUS_POINTS`] or, when there is none, the pixel of the
//!   image under it, each read from the user's own arrays.
//! - **Colourbars** are on top: a bar inside a plot area is drawn over the artists, and the box behind it hides
//!   them, so the colourbars are asked first by every gesture and by the datatip. Hovering a bar reads the value under
//!   the pointer on a continuous bar, the band on a discrete bar, the entry on an indexed bar, the category of a
//!   square and the label of a key ([`ColorbarTip`]). The wheel over a continuous bar, or over a discrete bar of
//!   automatic levels, zooms the scale's limits about the value under the pointer, a drag along it pans them with
//!   every tool, and a double-click restores them from the source, each by the same rules and the same arithmetic as
//!   the gesture on an axis, through one set of `kind.continuous.limits` or `kind.discrete.levels.limits`. A gesture
//!   on any other bar, on a bar item of a legend, on a key or on the box behind an inside bar changes nothing. A click
//!   on a bar selects its scale.
//!
//! Each gesture is one step of the undo history: a whole drag is one step, as is a wheel notch, a legend click, a
//! double-click and a reset. A 2D gesture on an axes whose limits are [`Limits::Auto`] starts from the limits the
//! compiler resolved (the hit map's [`AxisMap`]) and records [`Limits::Manual`] limits, leaving the source automatic.

use ironlab_ir::overlay::Overlay;
use ironlab_ir::{
    Artist, AspectAdjust, Axes, Axis, CategoricalColorScale, ColorScaleClass, ColorScaleDomain,
    ColorScaleKey, ColorScaleKind, ColorScaleLevels, ColorScaleNorm, ColorScaleSlot,
    ColorbarOrientation, Coords, Dimension, DiscreteColorScale, Edit, EditError, Figure,
    IndexedColorScale, Limits, NdArray, NodeId, PixelRange, Projection, PropertyPath, Scale,
    TickLabels, Ticks, Transaction, Value, View3d, command,
};
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::{AxesHitKind, AxisMap, ColorbarHit, ColorbarOwner, HitMap};
use ironlab_scene::maths::aspect;

use crate::problems::{Origin, Problem};

/// Degrees of azimuth or elevation per figure point of pointer travel in the Rotate tool.
pub const ROTATE_DEGREES_PER_POINT: f64 = 0.5;

/// The smallest width and height, in figure points, of a rubber band that the Zoom tool applies.
pub const MIN_BOX_ZOOM_POINTS: f64 = 3.0;

/// How far, in figure points, the pointer may be from a drawn data point and still read it.
pub const DATATIP_RADIUS_POINTS: f64 = 6.0;

/// The data point under the pointer, as the viewer reports it.
///
/// `index` is the index the point has in the artist's own data arrays, before the scene compiler thinned the series
/// to the current view, and `x`, `y` and `z` are the values held at that index. A datatip therefore names the
/// measurement the user recorded, whatever decimation drew. A dimension whose values the artist places as fractions
/// of the plot area rather than as data, such as the span of a reference line or the height of markers along an edge
/// of the axes, has no data value to report, so its coordinate is `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct Datatip {
    /// The axes the point was drawn in.
    pub axes: NodeId,
    /// The artist the point belongs to.
    pub artist: NodeId,
    /// The label of the artist, when it has one.
    pub label: Option<String>,
    /// The index of the point in the artist's own data arrays.
    pub index: usize,
    /// Where the point was drawn, in figure space, which is where a callout is anchored.
    pub position: Point,
    /// The x value, or `None` when the artist places its x values as fractions of the plot area.
    pub x: Option<f64>,
    /// The y value, or `None` when the artist places its y values as fractions of the plot area.
    pub y: Option<f64>,
    /// The z value, for an artist that has one.
    pub z: Option<f64>,
    /// The colour value, for a series with colour data: the value of the point, or, when the data holds one value
    /// per segment, the value of the segment that leaves the point, which the last point of the series has none of.
    pub color: Option<f64>,
}

/// The pixel of an image under the pointer, as the viewer reports it.
///
/// `row` and `column` index the artist's own array, so they name the pixel the user supplied whichever way the
/// ranges of the placement run. `x` and `y` are the data coordinates of the centre of that pixel along the first
/// and second axes of the image's plane, computed from the placement rather than read from the pointer, so that
/// every position within one pixel reads as the one value the pixel holds; every image with a datatip lies in the
/// xy plane of a two-dimensional axes, so they are its x and y. `value` is what the array stores for the pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct PixelDatatip {
    /// The axes the image was drawn in.
    pub axes: NodeId,
    /// The image the pixel belongs to.
    pub artist: NodeId,
    /// The label of the image, when it has one.
    pub label: Option<String>,
    /// The row of the pixel in the artist's array.
    pub row: usize,
    /// The column of the pixel in the artist's array.
    pub column: usize,
    /// The data coordinate of the centre of the pixel along the first axis of the image's plane.
    pub x: f64,
    /// The data coordinate of the centre of the pixel along the second axis of the image's plane.
    pub y: f64,
    /// Where the centre of the pixel was drawn, in figure space, which is where a callout is anchored.
    pub position: Point,
    /// What the artist's array holds for the pixel.
    pub value: PixelValue,
}

/// What the array of an image holds for one pixel, in the units it stores: an 8-bit value is widened to the number
/// it denotes and a floating-point value is read as it is, whatever colour the pixel was painted.
#[derive(Clone, Debug, PartialEq)]
pub enum PixelValue {
    /// The red, green and blue components of a pixel of a true-colour image, and its alpha component when the
    /// array has one: bytes from 0 to 255 or fractions from 0 to 1, as stored.
    Components(Vec<f64>),
    /// The index of a pixel of a scalar image mapped through an indexed colour scale, as stored: a floating-point
    /// index is not truncated, because 2.9 names no entry while 2 does.
    Index(f64),
    /// The index of a pixel of a scalar image mapped through a categorical colour scale, as stored, with the label
    /// of the category it names, or `None` when it names none (it is fractional, negative or past the last
    /// category).
    Category {
        /// The index, as stored.
        index: f64,
        /// The label of the category the index names, as its source text.
        label: Option<String>,
    },
    /// The value of a pixel of a scalar image mapped through a continuous or discrete colour scale, or through no
    /// usable scale.
    Value(f64),
}

/// The bar under the pointer, as the viewer reports it.
///
/// `index` is the index of the bar in the arrays of its series, and the position, value and base are those its data
/// holds, so that a datatip names the numbers the user plotted rather than anything read back from the page.
#[derive(Clone, Debug, PartialEq)]
pub struct BarDatatip {
    /// The axes the bar was drawn in.
    pub axes: NodeId,
    /// The bar series the bar belongs to.
    pub artist: NodeId,
    /// The label of the bar series, when it has one.
    pub label: Option<String>,
    /// The index of the bar in the artist's own arrays.
    pub index: usize,
    /// Where a callout is anchored, in figure space: the centre of the drawn bar.
    pub position: Point,
    /// The rectangle of the bar as drawn, in figure space, which the canvas outlines.
    pub rect: Rect,
    /// The position of the bar along the category axis, as its data holds it.
    pub at: f64,
    /// The value of the bar, as its data holds it.
    pub value: f64,
    /// The base the bar rises from: the constant base, or the bar's own base.
    pub base: f64,
    /// For a part of a stack, the bottom and top of the segment the bar occupies along the value axis.
    pub segment: Option<(f64, f64)>,
    /// The text tick label of the category axis at the bar's position, when the axis is labelled with text and a
    /// tick falls exactly there: the name of the bar's category.
    pub tick_label: Option<String>,
}

/// The cell under the pointer, as the viewer reports it.
///
/// `row` and `column` index the artist's own array of values, and the edges and the value are those its data holds,
/// so that a datatip names what the user plotted rather than anything read back from the page. The value is reported
/// in the words of the scale of the cells, as a pixel of a scalar image reports it, so that a status of an occupancy
/// matrix names its category.
#[derive(Clone, Debug, PartialEq)]
pub struct CellDatatip {
    /// The axes the cells were drawn in.
    pub axes: NodeId,
    /// The cells the cell belongs to.
    pub artist: NodeId,
    /// The label of the cells, when they have one.
    pub label: Option<String>,
    /// The row of the cell in the artist's array of values.
    pub row: usize,
    /// The column of the cell in the artist's array of values.
    pub column: usize,
    /// Where a callout is anchored, in figure space: the centre of the drawn cell.
    pub position: Point,
    /// The rectangle of the cell as drawn, in figure space, the gap excluded, which the canvas outlines.
    pub rect: Rect,
    /// The edges of the cell's column, edges `column` and `column + 1`, in the order of the data.
    pub column_edges: (f64, f64),
    /// The edges of the cell's row, edges `row` and `row + 1`, in the order of the data.
    pub row_edges: (f64, f64),
    /// What the artist's array holds for the cell, in the words of its scale: `Index` through an indexed scale,
    /// `Category` (with its label) through a categorical one, and `Value` otherwise.
    pub value: PixelValue,
    /// The text tick label of the y axis at the centre of the cell's row, when the axis is labelled with text and a
    /// tick falls exactly there: the name of the row.
    pub row_name: Option<String>,
    /// The text tick label of the x axis at the centre of the cell's column, likewise: the name of the column.
    pub column_name: Option<String>,
}

/// The step of stairs under the pointer, as the viewer reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct StepDatatip {
    /// The axes the stairs were drawn in.
    pub axes: NodeId,
    /// The stairs the step belongs to.
    pub artist: NodeId,
    /// The label of the stairs, when they have one.
    pub label: Option<String>,
    /// The index of the step's value in the artist's array of values.
    pub index: usize,
    /// Where a callout is anchored, in figure space: the point of the drawn step nearest the pointer.
    pub position: Point,
    /// The two edges of the step, in the order of the data.
    pub edges: (f64, f64),
    /// The value held across the step.
    pub value: f64,
}

/// What lies under the pointer: a colourbar, a drawn data point, or, where none is within reach, a step of stairs, a
/// bar, a cell or a pixel of an image.
#[derive(Clone, Debug, PartialEq)]
pub enum Tip {
    /// A point of a series.
    Point(Datatip),
    /// A pixel of an image.
    Pixel(PixelDatatip),
    /// A colourbar: its bar, one of its squares or one of its keys.
    Colorbar(ColorbarTip),
    /// A bar of a bar series.
    Bar(BarDatatip),
    /// A step of stairs.
    Step(StepDatatip),
    /// A cell of cells.
    Cell(CellDatatip),
}

/// What a colourbar says under the pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorbarTip {
    /// What holds the bar: a panel, an axes, or a legend whose bar item it is.
    pub owner: ColorbarOwner,
    /// The colour scale the bar shows.
    pub scale: NodeId,
    /// The source text of the scale's title, when it has one.
    pub title: Option<String>,
    /// Where a callout is anchored, in figure space: the point on the centre line of a continuous bar level with the
    /// pointer, or the centre of the piece read.
    pub position: Point,
    /// The piece read, in figure space — a band, an entry's block, a category's square or a key's square — or `None`
    /// on a continuous bar, which has no pieces.
    pub piece: Option<Rect>,
    /// What the bar reads there.
    pub reading: ColorbarReading,
}

/// What a colourbar reads under the pointer, in the terms of the kind of its scale.
#[derive(Clone, Debug, PartialEq)]
pub enum ColorbarReading {
    /// The value under the pointer on a continuous bar.
    Value(f64),
    /// The band under the pointer on a discrete bar, from its lower edge to its upper edge; a value on an edge
    /// belongs to the upper band, as the scale classifies it.
    Band { low: f64, high: f64 },
    /// The entry under the pointer on an indexed bar, with the label of the tick at that entry, when the scale labels
    /// its ticks explicitly and one of them falls there.
    Entry { index: usize, label: Option<String> },
    /// The category of the square under the pointer, with its label's source text.
    Category { index: usize, label: String },
    /// The out-of-range key under the pointer, with the label it is drawn with.
    Key { key: ColorScaleKey, label: String },
}

/// What of a colourbar lies under the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BarPart {
    /// The bar of a continuous, discrete or indexed scale.
    Bar,
    /// The square of the category with this index.
    Category(usize),
    /// The out-of-range key with this position among the bar's keys.
    Key(usize),
    /// The box behind an inside bar, beside the bar and its keys, which hides what lies beneath.
    Backdrop,
}

/// The limits of a colour scale that a gesture on one of its bars moves.
#[derive(Clone, Debug, PartialEq)]
struct ScaleTarget {
    /// The colour scale.
    scale: NodeId,
    /// The path of its limits: `kind.continuous.limits` or `kind.discrete.levels.limits`.
    path: PropertyPath,
    /// How its values lie along the bar, with the limits the gesture starts from.
    map: AxisMap,
    /// Whether the bar stands, so that its values run along figure-space y rather than x.
    vertical: bool,
}

/// The gesture that a primary-button drag performs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Drag pans 2D limits or the 3D view.
    #[default]
    Pan,
    /// Drag draws a rubber band that zooms 2D axes to its extent.
    Zoom,
    /// Drag rotates 3D axes.
    Rotate,
}

/// The state captured at the start of a drag.
#[derive(Clone, Debug, PartialEq)]
struct Drag {
    /// The axes, or the colour scale of the bar, under the pointer when the drag started.
    axes: NodeId,
    /// The plot rectangle of that axes at the start of the drag.
    plot_rect: Rect,
    /// The pointer position at the start of the drag.
    start: Point,
    /// The most recent pointer position.
    last: Point,
    /// The geometry captured at the start of the drag.
    kind: DragKind,
}

#[derive(Clone, Debug, PartialEq)]
enum DragKind {
    /// A 2D axes, with its x and y axis maps at the start of the drag and whether it keeps a fixed aspect.
    TwoD {
        x: AxisMap,
        y: AxisMap,
        fixed_aspect: bool,
    },
    /// A 3D axes, with its view at the start of the drag.
    ThreeD { view: View3d },
    /// A colourbar, with the limits of its scale at the start of the drag.
    Scale(ScaleTarget),
}

/// The interactive state of one figure in the viewer: its source, the user's overlay, and their composition.
#[derive(Clone, Debug)]
pub struct FigureState {
    /// The figure as its owner defines it: as opened, or as last saved. Gestures never change it.
    source: Figure,
    /// The sets the user has made, with their undo history.
    overlay: Overlay,
    /// The source with the overlay applied: what is drawn, hit-tested, exported and saved. Recomposed when the
    /// overlay changes rather than on every frame, because composing clones the source.
    composed: Figure,
    /// What has gone wrong with the user's own changes: entries that composition
    /// discarded and changes that the IR refused. Cleared whenever the overlay changes,
    /// because a problem raised by a change that is no longer there is stale.
    problems: Vec<Problem>,
    /// The active drag tool.
    pub tool: Tool,
    drag: Option<Drag>,
    /// The node the property editor shows, which a click on the canvas and a click in the
    /// object tree both set.
    selected: Option<NodeId>,
}

impl FigureState {
    /// Creates the state for a freshly opened figure, with the Pan tool active and an empty overlay.
    #[must_use]
    pub fn new(figure: Figure) -> Self {
        let id = figure.id;
        Self {
            composed: figure.clone(),
            source: figure,
            overlay: Overlay::new(),
            problems: Vec::new(),
            tool: Tool::Pan,
            drag: None,
            selected: Some(id),
        }
    }

    /// Returns the displayed figure: the source with the overlay applied, which is what is drawn, exported and saved.
    #[must_use]
    pub fn figure(&self) -> &Figure {
        &self.composed
    }

    /// Returns the source figure, which no gesture changes.
    #[must_use]
    pub fn source(&self) -> &Figure {
        &self.source
    }

    /// Returns the overlay of the user's changes.
    #[must_use]
    pub fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    /// Returns the node that the property editor shows, which is the figure itself until
    /// something else is selected.
    #[must_use]
    pub fn selection(&self) -> Option<NodeId> {
        self.selected
    }

    /// Selects a node, or nothing. A node that is not in the displayed figure selects
    /// nothing, so that the property editor never shows a node that has gone.
    pub fn select(&mut self, node: Option<NodeId>) {
        self.selected = node.filter(|node| self.composed.node_kind(*node).is_some());
    }

    /// Opens an undo step, so that everything recorded until [`FigureState::end_edit_step`]
    /// is undone in one step.
    ///
    /// The property editor holds a step open while a numeric field is dragged or a text
    /// field is being typed into, so that the change ends as one step rather than one per
    /// frame, exactly as a drag on the canvas does.
    pub fn begin_edit_step(&mut self) {
        self.overlay.begin_step();
    }

    /// Closes the step opened by [`FigureState::begin_edit_step`]. A step that changed
    /// nothing adds nothing to the history.
    pub fn end_edit_step(&mut self) {
        self.overlay.end_step();
    }

    /// Records a transaction that the displayed figure must accept, which is what the
    /// property editor commits.
    ///
    /// The transaction is first applied to a copy of the displayed figure. When the IR
    /// refuses it (limits that are not increasing, a projection that an artist cannot be
    /// drawn in), nothing is recorded: the figure is left exactly as it was, no undo step
    /// is spent, and the reason is added to [`FigureState::problems`], which the toolbar
    /// shows. Otherwise the transaction is recorded as [`FigureState::record`] does.
    ///
    /// Returns whether the displayed figure changed.
    pub fn try_record(&mut self, transaction: &Transaction) -> bool {
        let mut trial = self.composed.clone();
        if let Err(error) = trial.apply(transaction) {
            self.report(refusal(transaction, &error));
            return false;
        }
        self.record(transaction)
    }

    /// Removes the overlay entry for exactly one property of one node, as the property
    /// editor's revert control does, and recomposes the displayed figure so that the
    /// source's own value is shown again.
    ///
    /// The revert is its own undo step. Returns whether there was an entry to remove.
    pub fn revert(&mut self, node: NodeId, path: &PropertyPath) -> bool {
        if !self.overlay.revert(node, path) {
            return false;
        }
        self.recompose();
        true
    }

    /// Returns the problems raised by the user's own changes, in the order they arose:
    /// the entries that composition discarded and the changes that the IR refused.
    ///
    /// They are kept until the overlay next changes, because a problem raised by a change
    /// that has since been undone, reverted or replaced no longer describes the figure.
    /// The toolbar shows them alongside the warnings of the scene compiler.
    #[must_use]
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// Returns the number of changes the user has made, which is the number of entries in
    /// the overlay: what "Revert all changes" would discard.
    #[must_use]
    pub fn change_count(&self) -> usize {
        self.overlay.entries().len()
    }

    /// Discards every change the user has made — limits, three-dimensional views,
    /// visibility and every property edited — so that the figure its owner defines is
    /// shown again, and returns whether there was one to discard.
    ///
    /// The source is not touched, so this removes every change made to the figure rather
    /// than making one. It is a clean slate rather than a step of the history: the undo
    /// and redo history goes with the changes, so nothing can be undone straight after
    /// it, and every problem raised by those changes goes too.
    ///
    /// This is wider than [`FigureState::reset_view`], which discards only the limits and
    /// three-dimensional views, keeps the visibility of plots and every property edited,
    /// and is itself a step of the history.
    pub fn revert_all(&mut self) -> bool {
        self.end_drag();
        let discarded = !self.overlay.entries().is_empty();
        self.overlay.clear();
        self.problems.clear();
        if discarded {
            self.recompose();
        }
        discarded
    }

    /// Records a transaction of sets made by the user in the overlay and recomposes the displayed figure.
    ///
    /// A set whose value the displayed figure already has is left out, so that a gesture that changes nothing records
    /// nothing. A transaction that holds any edit other than a set is refused, because the overlay holds only sets.
    /// Unless a drag is in progress, the transaction is one step of the undo history. Returns whether the overlay
    /// changed, which is what tells the canvas to recompile the scene.
    pub fn record(&mut self, transaction: &Transaction) -> bool {
        let edits: Vec<Edit> = transaction
            .edits
            .iter()
            .filter(|edit| self.changes_the_figure(edit))
            .cloned()
            .collect();
        if edits.is_empty() {
            return false;
        }
        let before = self.overlay.entries().to_vec();
        if self.overlay.record(&Transaction { edits }).is_err() {
            return false;
        }
        self.recompose();
        self.overlay.entries() != before
    }

    /// Restores the overlay to its state before the most recent gesture. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        self.undone(Overlay::undo)
    }

    /// Re-applies the most recently undone gesture. Returns whether there was one.
    pub fn redo(&mut self) -> bool {
        self.undone(Overlay::redo)
    }

    /// Returns whether there is a gesture to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.overlay.can_undo()
    }

    /// Returns whether there is a gesture to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.overlay.can_redo()
    }

    /// Makes the displayed figure the source and empties the overlay, as saving does.
    ///
    /// The viewer owns the source, so the figure it has just written is the figure its owner now defines; the changes
    /// written can no longer be undone, and resetting the view restores the figure as saved.
    pub fn fold_overlay(&mut self) {
        self.source = self.composed.clone();
        self.overlay = Overlay::new();
        self.problems.clear();
        self.drag = None;
    }

    /// Zooms the axes under `at` by `zoom_factor` (greater than one zooms in). Returns whether the figure changed.
    pub fn scroll(&mut self, hit: &HitMap, at: Point, zoom_factor: f64) -> bool {
        if !(zoom_factor.is_finite() && zoom_factor > 0.0) || zoom_factor == 1.0 {
            return false;
        }
        if let Some(target) = self.bar_gesture(hit, at) {
            let Some(target) = target else {
                return false;
            };
            let coord = if target.vertical { at.y } else { at.x };
            let limits = zoomed(&target.map, coord, zoom_factor);
            return self.step(|state| state.set_scale_limits(&target, limits));
        }
        let Some(axes_hit) = hit.axes_at(at) else {
            return false;
        };
        let id = axes_hit.id;
        let Some(axes) = self.composed.axes(id) else {
            return false;
        };
        match (&axes_hit.kind, axes.projection) {
            (AxesHitKind::TwoD { x, y }, Projection::TwoD) => {
                let (x, y) = current_maps(axes, x, y);
                let x_limits = zoomed(&x, at.x, zoom_factor);
                let y_limits = zoomed(&y, at.y, zoom_factor);
                self.step(|state| {
                    let changed_x = state.set_scaled_limits(id, Dimension::X, &x, x_limits);
                    let changed_y = state.set_scaled_limits(id, Dimension::Y, &y, y_limits);
                    changed_x || changed_y
                })
            }
            (AxesHitKind::ThreeD, Projection::ThreeD { view3d }) => {
                let zoom = view3d.zoom * zoom_factor;
                if !(zoom.is_finite() && zoom > 0.0) {
                    return false;
                }
                self.set_view(id, &[("zoom", zoom)])
            }
            _ => false,
        }
    }

    /// Starts a drag at `at`, capturing the axes under the pointer and its axis maps or view.
    ///
    /// Everything the drag records is one step of the undo history. A drag that starts outside every axes is ignored
    /// by the subsequent updates.
    pub fn drag_start(&mut self, hit: &HitMap, at: Point) {
        self.end_drag();
        if let Some(target) = self.bar_gesture(hit, at) {
            let Some(target) = target else {
                return;
            };
            self.drag = Some(Drag {
                axes: target.scale,
                plot_rect: Rect::new(at.x, at.y, 0.0, 0.0),
                start: at,
                last: at,
                kind: DragKind::Scale(target),
            });
            self.overlay.begin_step();
            return;
        }
        let Some(axes_hit) = hit.axes_at(at) else {
            return;
        };
        let Some(axes) = self.composed.axes(axes_hit.id) else {
            return;
        };
        let kind = match (&axes_hit.kind, axes.projection) {
            (AxesHitKind::TwoD { x, y }, Projection::TwoD) => {
                let (x, y) = current_maps(axes, x, y);
                DragKind::TwoD {
                    x,
                    y,
                    fixed_aspect: fixed_ratio(axes).is_some(),
                }
            }
            (AxesHitKind::ThreeD, Projection::ThreeD { view3d }) => {
                DragKind::ThreeD { view: view3d }
            }
            _ => return,
        };
        self.drag = Some(Drag {
            axes: axes_hit.id,
            plot_rect: axes_hit.plot_rect,
            start: at,
            last: at,
            kind,
        });
        self.overlay.begin_step();
    }

    /// Updates the drag with the pointer at `at`. Returns whether the figure changed.
    pub fn drag_update(&mut self, at: Point) -> bool {
        if !is_finite(at) {
            return false;
        }
        let Some(drag) = self.drag.as_mut() else {
            return false;
        };
        drag.last = at;
        let drag = drag.clone();
        let (dx, dy) = (at.x - drag.start.x, at.y - drag.start.y);
        match (self.tool, &drag.kind) {
            // A bar has one dimension and no camera, so a drag along it pans its scale whatever the tool.
            (_, DragKind::Scale(target)) => {
                let (from, to) = if target.vertical {
                    (drag.start.y, at.y)
                } else {
                    (drag.start.x, at.x)
                };
                if from == to {
                    // Returning to the start of the drag restores the limits captured at its start.
                    return self.restore_scale(target);
                }
                self.set_scale_limits(target, panned(&target.map, from, to))
            }
            (Tool::Pan, DragKind::TwoD { x, y, .. }) => {
                let changed_x = dx != 0.0
                    && self.set_scaled_limits(
                        drag.axes,
                        Dimension::X,
                        x,
                        panned(x, drag.start.x, at.x),
                    );
                let changed_y = dy != 0.0
                    && self.set_scaled_limits(
                        drag.axes,
                        Dimension::Y,
                        y,
                        panned(y, drag.start.y, at.y),
                    );
                // Returning to the start of the drag restores the limits captured at its start.
                let restored_x = dx == 0.0 && self.restore_captured(drag.axes, Dimension::X, x);
                let restored_y = dy == 0.0 && self.restore_captured(drag.axes, Dimension::Y, y);
                changed_x || changed_y || restored_x || restored_y
            }
            (Tool::Pan, DragKind::ThreeD { view }) => {
                if self.view(drag.axes).is_none() {
                    return false;
                }
                let pan_x = view.pan_x + dx / drag.plot_rect.width;
                let pan_y = view.pan_y + dy / drag.plot_rect.height;
                if !(pan_x.is_finite() && pan_y.is_finite()) {
                    return false;
                }
                self.set_view(drag.axes, &[("pan_x", pan_x), ("pan_y", pan_y)])
            }
            (Tool::Rotate, DragKind::ThreeD { view }) => {
                if self.view(drag.axes).is_none() {
                    return false;
                }
                let azimuth_deg = view.azimuth_deg - dx * ROTATE_DEGREES_PER_POINT;
                let elevation_deg =
                    (view.elevation_deg + dy * ROTATE_DEGREES_PER_POINT).clamp(-90.0, 90.0);
                self.set_view(
                    drag.axes,
                    &[
                        ("azimuth_deg", azimuth_deg),
                        ("elevation_deg", elevation_deg),
                    ],
                )
            }
            // The Zoom tool only moves its rubber band while dragging, a 3D axes has no band, and a 2D axes has
            // nothing to rotate.
            (Tool::Zoom, _) | (Tool::Rotate, DragKind::TwoD { .. }) => false,
        }
    }

    /// Ends the drag with the pointer at `at`, applying a box zoom when the Zoom tool is active. Returns whether the
    /// figure changed.
    pub fn drag_end(&mut self, at: Point) -> bool {
        if self.drag.is_none() {
            return false;
        }
        let mut changed = false;
        let on_bar = matches!(
            self.drag,
            Some(Drag {
                kind: DragKind::Scale(_),
                ..
            })
        );
        if self.tool == Tool::Zoom && !on_bar {
            if is_finite(at)
                && let Some(drag) = self.drag.as_mut()
            {
                drag.last = at;
            }
            if let (
                Some(drawn),
                Some(band),
                Some(Drag {
                    axes,
                    kind: DragKind::TwoD { x, y, .. },
                    ..
                }),
            ) = (self.drawn_band(), self.rubber_band(), self.drag.clone())
                && drawn.width >= MIN_BOX_ZOOM_POINTS
                && drawn.height >= MIN_BOX_ZOOM_POINTS
            {
                let span = |map: &AxisMap, p: f64, q: f64| {
                    let (u, v) = (to_scale(map, map.to_data(p)), to_scale(map, map.to_data(q)));
                    (u.min(v), u.max(v))
                };
                let x_limits = span(&x, band.x, band.right());
                let y_limits = span(&y, band.y, band.bottom());
                let changed_x = self.set_scaled_limits(axes, Dimension::X, &x, x_limits);
                let changed_y = self.set_scaled_limits(axes, Dimension::Y, &y, y_limits);
                changed = changed_x || changed_y;
            }
        } else {
            changed = self.drag_update(at);
        }
        self.end_drag();
        changed
    }

    /// Returns the rubber band of an ongoing Zoom-tool drag, in figure space: the rectangle from the start of the
    /// drag to the pointer, clamped to the plot rectangle, and, on an axes with a fixed aspect, with its shorter side
    /// expanded about its centre to the shape of the plot rectangle, so that the band is exactly the region the zoom
    /// will show.
    #[must_use]
    pub fn rubber_band(&self) -> Option<Rect> {
        let band = self.drawn_band()?;
        let drag = self.drag.as_ref()?;
        match drag.kind {
            DragKind::TwoD {
                fixed_aspect: true, ..
            } => Some(to_shape_of(band, drag.plot_rect)),
            _ => Some(band),
        }
    }

    /// Returns the rectangle the pointer has drawn in an ongoing Zoom-tool drag, from the start of the drag to the
    /// pointer and clamped to the plot rectangle, before any expansion for a fixed aspect.
    fn drawn_band(&self) -> Option<Rect> {
        if self.tool != Tool::Zoom {
            return None;
        }
        let drag = self.drag.as_ref()?;
        if !matches!(drag.kind, DragKind::TwoD { .. }) {
            return None;
        }
        let r = drag.plot_rect;
        let clamp = |p: Point| Point::new(p.x.clamp(r.x, r.right()), p.y.clamp(r.y, r.bottom()));
        let (a, b) = (clamp(drag.start), clamp(drag.last));
        Some(Rect::new(
            a.x.min(b.x),
            a.y.min(b.y),
            (a.x - b.x).abs(),
            (a.y - b.y).abs(),
        ))
    }

    /// Removes the view entries of the axes under `at` from the overlay, so that it shows the source again, and sets
    /// the axes linked with it to the limits restored.
    ///
    /// Over a legend entry it toggles the entry's artist instead, as [`FigureState::click`] does. Returns whether the
    /// figure changed.
    pub fn double_click(&mut self, hit: &HitMap, at: Point) -> bool {
        if let Some(target) = self.bar_gesture(hit, at) {
            let Some(target) = target else {
                return false;
            };
            self.select(Some(target.scale));
            return self.step(|state| {
                let before = state.overlay.entries().to_vec();
                state.overlay.reset_view(target.scale);
                let changed = state.overlay.entries() != before;
                if changed {
                    state.recompose();
                }
                changed
            });
        }
        if hit.legend_entry_at(at).is_some() {
            return self.click(hit, at);
        }
        let Some(axes_hit) = hit.axes_at(at) else {
            return false;
        };
        let id = axes_hit.id;
        if self.composed.axes(id).is_none() {
            return false;
        }
        self.select(Some(id));
        self.step(|state| {
            let before = state.overlay.entries().to_vec();
            state.overlay.reset_view(id);
            let mut changed = state.overlay.entries() != before;
            if changed {
                state.recompose();
            }
            // The entries of the axes linked with this one are not removed, so they are set to the limits restored;
            // a set of limits that an axes already shows records nothing.
            for dimension in [Dimension::X, Dimension::Y, Dimension::Z] {
                let axes = state.composed.axes(id).expect("the axes is in the figure");
                let limits = axis(axes, dimension).limits;
                if let Ok(transaction) = command::set_limits(&state.composed, id, dimension, limits)
                {
                    changed |= state.record(&transaction);
                }
            }
            changed
        })
    }

    /// Sets `visible` on every artist the legend entry under `at` stands for, to the opposite of what is displayed.
    /// Returns whether the figure changed.
    ///
    /// An entry is displayed as shown while any of its artists is visible, so a click on an entry hides all of its
    /// artists unless every one of them is already hidden, in which case it shows them all. The first of the
    /// artists is selected.
    pub fn click(&mut self, hit: &HitMap, at: Point) -> bool {
        if let Some((bar, part)) = colorbar_under(hit, at) {
            if part != BarPart::Backdrop {
                self.select(Some(bar.scale));
            }
            return false;
        }
        let Some(entry) = hit.legend_entry_at(at) else {
            if let Some(axes) = hit.axes_at(at) {
                self.select(Some(axes.id));
            }
            return false;
        };
        let artists: Vec<(NodeId, bool)> = entry
            .targets
            .iter()
            .filter_map(|&(_, id)| {
                let (_, artist) = self.composed.artist(id)?;
                Some((id, artist.visible()))
            })
            .collect();
        let Some(&(first, _)) = artists.first() else {
            return false;
        };
        let shown = artists.iter().any(|&(_, visible)| visible);
        self.select(Some(first));
        self.record(&Transaction {
            edits: artists
                .iter()
                .map(|&(id, _)| Edit::Set {
                    node: id,
                    path: path(&["visible"]),
                    value: Value::Bool(!shown),
                })
                .collect(),
        })
    }

    /// Returns the drawn data point nearest the pointer, within [`DATATIP_RADIUS_POINTS`].
    ///
    /// The point comes from the hit map of the most recent compilation, which records the index every drawn point
    /// has in the artist's own arrays even when the series was decimated to fit the view. The values are then read
    /// from those arrays at that index, so a datatip reports the measurement the user recorded rather than anything
    /// the scene compiler derived from it.
    #[must_use]
    pub fn datatip(&self, hit: &HitMap, at: Point) -> Option<Datatip> {
        let (drawn, sample) = hit.sample_at(at, DATATIP_RADIUS_POINTS)?;
        let (_, artist) = self.composed.artist(drawn.artist)?;
        let Artist::Series(series) = artist else {
            return None;
        };
        let (x, y, z, coords) = (
            series.x,
            series.y,
            series.z,
            [series.x_coords, series.y_coords],
        );
        let index = sample.source_index;
        let value = |id: NodeId| self.composed.data_array(id)?.array.get(index);
        // A fraction of the plot area is where the point was drawn, not a measurement, so it is not reported as data.
        let data = |id: NodeId, coords: Coords| match coords {
            Coords::Data => value(id).map(Some),
            Coords::AxesFraction => Some(None),
        };
        Some(Datatip {
            axes: drawn.axes,
            artist: drawn.artist,
            label: artist.label().map(|text| text.content.clone()),
            index,
            position: sample.position,
            x: data(x, coords[0])?,
            y: data(y, coords[1])?,
            z: z.and_then(value),
            color: series.color_data.and_then(value),
        })
    }

    /// Returns the pixel of the image drawn last under the pointer, or `None` where no image lies.
    ///
    /// The pixel comes from the hit map of the most recent compilation, which records the placement of every image
    /// drawn in a two-dimensional axes and finds the row and column of the artist's own array under a point through
    /// its inverse; an image on the floor or a wall of a three-dimensional axes has no entry, so it has no datatip.
    /// The value is read from that array at that row and column, in the units the array stores it in and whatever
    /// colour the pixel was painted: a pixel that a policy left transparent still reports what it holds. The
    /// coordinates are those of the centre of the pixel as the placement puts it — the index along an axis without
    /// a range, `first + i · pitch` along an axis with one, and `first` alone for a single pixel, whose `last`
    /// places nothing — rather than the pointer's, so that every position within one pixel reads as the one value
    /// the pixel holds; `position` is that centre in figure space.
    #[must_use]
    pub fn pixel_datatip(&self, hit: &HitMap, at: Point) -> Option<PixelDatatip> {
        let (drawn, row, column) = hit.pixel_at(at)?;
        let (_, artist) = self.composed.artist(drawn.artist)?;
        let (data, placement, kind) = match artist {
            Artist::Image(image) => (image.pixels, image.placement, PixelKind::Components),
            Artist::ScalarImage(image) => (
                image.values,
                image.placement,
                self.mapped_kind(image.color_scale),
            ),
            _ => return None,
        };
        let value = pixel_value(&self.composed.data_array(data)?.array, kind, row, column)?;
        let centre = Point::new(column as f64 + 0.5, row as f64 + 0.5);
        Some(PixelDatatip {
            axes: drawn.axes,
            artist: drawn.artist,
            label: artist.label().map(|text| text.content.clone()),
            row,
            column,
            x: pixel_centre(placement.columns, drawn.columns, column),
            y: pixel_centre(placement.rows, drawn.rows, row),
            position: drawn.to_pixel.inverse()?.apply(centre),
            value,
        })
    }

    /// Returns the kind of value that an array mapped through the colour scale `scale` holds: indices through an
    /// indexed scale, the indices of categories through a categorical one, and values otherwise.
    fn mapped_kind(&self, scale: Option<NodeId>) -> PixelKind<'_> {
        match scale
            .and_then(|scale| self.composed.color_scale(scale))
            .map(|scale| &scale.kind)
        {
            Some(ColorScaleKind::Indexed(_)) => PixelKind::Index,
            Some(ColorScaleKind::Categorical(scale)) => PixelKind::Category(scale),
            _ => PixelKind::Value,
        }
    }

    /// Returns the cell drawn last under the pointer, or `None` where no cell lies, which includes the gap between
    /// cells and a cell that its scale leaves transparent, because neither is drawn.
    ///
    /// The cell comes from the hit map of the most recent compilation, which records each cell drawn with its row and
    /// column in the artist's array and its edges. The value is read from the array in the words of the scale of the
    /// cells, as [`FigureState::pixel_datatip`] reads the value of a pixel. When the y axis has explicit ticks
    /// labelled with text and one of them falls exactly at the centre of the cell's row, midway between its edges,
    /// the tip carries that label as the name of the row; the name of the column is found on the x axis in the same
    /// way.
    #[must_use]
    pub fn cell_datatip(&self, hit: &HitMap, at: Point) -> Option<CellDatatip> {
        let drawn = hit.cell_at(at)?;
        let (axes, artist) = self.composed.artist(drawn.artist)?;
        let Artist::Cells(cells) = artist else {
            return None;
        };
        let kind = self.mapped_kind(cells.color_scale);
        let array = &self.composed.data_array(cells.values)?.array;
        let value = pixel_value(array, kind, drawn.row, drawn.column)?;
        let centre = |(a, b): (f64, f64)| (a + b) / 2.0;
        let r = drawn.rect;
        Some(CellDatatip {
            axes: drawn.axes,
            artist: drawn.artist,
            label: artist.label().map(|text| text.content.clone()),
            row: drawn.row,
            column: drawn.column,
            position: Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0),
            rect: r,
            column_edges: drawn.column_edges,
            row_edges: drawn.row_edges,
            value,
            row_name: category_name(&axes.y, centre(drawn.row_edges)),
            column_name: category_name(&axes.x, centre(drawn.column_edges)),
        })
    }

    /// Returns what lies under the pointer: the drawn data point nearest it within [`DATATIP_RADIUS_POINTS`], as
    /// [`FigureState::datatip`] finds it, or, when there is none, the step of stairs within the same reach, as
    /// [`FigureState::step_datatip`] finds it, the bar under the pointer, as [`FigureState::bar_datatip`] finds it,
    /// the cell under it, as [`FigureState::cell_datatip`] finds it, or the pixel of the image under it, as
    /// [`FigureState::pixel_datatip`] finds it, in that order.
    ///
    /// A point wins over the bar, the cell or the pixel beneath it because it is small, so that the bar, the cell or
    /// the pixel is read everywhere the pointer is not within reach of a point, and because a series drawn over bars,
    /// cells or an image is usually painted on top of them, so that a pointer within reach of one of its points means
    /// the point rather than what it covers; the dots marking some cells of a matrix are read as dots. A step is a
    /// thin line, read within reach as a point is, so it wins over a bar it is drawn over, as the outline of a
    /// histogram over its bars is; a bar wins over a cell and a cell over an image, which are areas that bars and
    /// cells are usually drawn over.
    ///
    /// A colourbar is asked first, because a bar inside a plot area is drawn over the artists: over a bar, one of
    /// its squares or one of its keys the tip is what the bar reads, as [`FigureState::colorbar_tip`] finds it, and
    /// over the box behind an inside bar there is no tip, because the box hides what lies beneath it.
    #[must_use]
    pub fn tip_at(&self, hit: &HitMap, at: Point) -> Option<Tip> {
        if colorbar_under(hit, at).is_some() {
            return self.colorbar_tip(hit, at).map(Tip::Colorbar);
        }
        self.datatip(hit, at)
            .map(Tip::Point)
            .or_else(|| self.step_datatip(hit, at).map(Tip::Step))
            .or_else(|| self.bar_datatip(hit, at).map(Tip::Bar))
            .or_else(|| self.cell_datatip(hit, at).map(Tip::Cell))
            .or_else(|| self.pixel_datatip(hit, at).map(Tip::Pixel))
    }

    /// Returns the bar drawn last under the pointer, or `None` where no bar lies.
    ///
    /// The bar comes from the hit map of the most recent compilation, which records each bar drawn with its index in
    /// the arrays of its series, its position, value and base, and for a part of a stack the segment it occupies.
    /// When the category axis of the bar (x for upright bars, y for bars lying flat) has explicit ticks labelled
    /// with text and one of them falls exactly at the bar's position, the tip carries that label, the name of the
    /// bar's category.
    #[must_use]
    pub fn bar_datatip(&self, hit: &HitMap, at: Point) -> Option<BarDatatip> {
        let drawn = hit.bar_at(at)?;
        let (axes, artist) = self.composed.artist(drawn.artist)?;
        let Artist::Bar(bar) = artist else {
            return None;
        };
        let tick_label = category_name(
            axis(axes, bar.orientation.category_dimension()),
            drawn.position,
        );
        let r = drawn.rect;
        Some(BarDatatip {
            axes: drawn.axes,
            artist: drawn.artist,
            label: artist.label().map(|text| text.content.clone()),
            index: drawn.index,
            position: Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0),
            rect: r,
            at: drawn.position,
            value: drawn.value,
            base: drawn.base,
            segment: drawn.segment,
            tick_label,
        })
    }

    /// Returns the step of stairs whose drawn run lies nearest the pointer within [`DATATIP_RADIUS_POINTS`], or
    /// `None` where none lies within reach.
    ///
    /// The step comes from the hit map of the most recent compilation, which records each step drawn with the index
    /// of its value, the two edges it holds across and the value; for stairs through samples, the edges are those
    /// that `step` places, so that the tip reports the interval across which the sample's value holds.
    #[must_use]
    pub fn step_datatip(&self, hit: &HitMap, at: Point) -> Option<StepDatatip> {
        let drawn = hit.step_at(at, DATATIP_RADIUS_POINTS)?;
        let (_, artist) = self.composed.artist(drawn.artist)?;
        Some(StepDatatip {
            axes: drawn.axes,
            artist: drawn.artist,
            label: artist.label().map(|text| text.content.clone()),
            index: drawn.index,
            position: drawn.nearest(at),
            edges: drawn.edges,
            value: drawn.value,
        })
    }

    /// Returns what the colourbar under the pointer says, or `None` where no bar, square or key lies.
    ///
    /// The bar is the one painted last under the pointer. A continuous bar reads the value under the pointer, by
    /// inverting the mapping the bar was drawn with ([`ColorbarHit::map`]); a discrete bar reads the band under it,
    /// from the edges the bar drew, a value on an edge belonging to the upper band; an indexed bar reads the entry
    /// whose block is under it, with the label of an explicitly labelled tick at that entry; a square reads its
    /// category and a key the label it is drawn with. The title, the tick labels and the gaps between pieces read
    /// nothing, and neither does the box behind an inside bar.
    #[must_use]
    pub fn colorbar_tip(&self, hit: &HitMap, at: Point) -> Option<ColorbarTip> {
        let (bar, part) = colorbar_under(hit, at)?;
        let scale = self.composed.color_scale(bar.scale)?;
        let centre = |r: Rect| Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0);
        let (position, piece, reading) = match part {
            BarPart::Backdrop => return None,
            BarPart::Key(i) => {
                let key = &bar.keys[i];
                let reading = ColorbarReading::Key {
                    key: key.key,
                    label: key.label.clone(),
                };
                (centre(key.rect), Some(key.rect), reading)
            }
            BarPart::Category(i) => {
                let ColorScaleKind::Categorical(categorical) = &scale.kind else {
                    return None;
                };
                let label = categorical.categories.get(i)?.label.content.clone();
                let square = bar.entries[i];
                let reading = ColorbarReading::Category { index: i, label };
                (centre(square), Some(square), reading)
            }
            BarPart::Bar => {
                let map = bar.map?;
                let vertical = bar.orientation == ColorbarOrientation::Vertical;
                let value = map.to_data(if vertical { at.y } else { at.x });
                match &scale.kind {
                    ColorScaleKind::Continuous(_) => {
                        let r = bar.bar;
                        let position = if vertical {
                            Point::new(r.x + r.width / 2.0, at.y)
                        } else {
                            Point::new(at.x, r.y + r.height / 2.0)
                        };
                        (position, None, ColorbarReading::Value(value))
                    }
                    ColorScaleKind::Discrete(_) => {
                        let (low, high) = band_of(&bar.edges, value)?;
                        let piece = span_of(bar, &map, low, high);
                        (
                            centre(piece),
                            Some(piece),
                            ColorbarReading::Band { low, high },
                        )
                    }
                    ColorScaleKind::Indexed(indexed) => {
                        let count = indexed.colors.len();
                        if count == 0 || !value.is_finite() {
                            return None;
                        }
                        // Saturating: a value beyond either end of the bar names the entry at that end.
                        let index = (value.round().max(0.0) as usize).min(count - 1);
                        let piece = span_of(bar, &map, index as f64 - 0.5, index as f64 + 0.5);
                        let reading = ColorbarReading::Entry {
                            index,
                            label: tick_label_at(indexed, index),
                        };
                        (centre(piece), Some(piece), reading)
                    }
                    ColorScaleKind::Categorical(_) => return None,
                }
            }
        };
        Some(ColorbarTip {
            owner: bar.owner,
            scale: bar.scale,
            title: scale.title.as_ref().map(|title| title.content.clone()),
            position,
            piece,
            reading,
        })
    }

    /// Returns `None` when no colourbar lies under the pointer, so that the gesture falls to what lies beneath, and
    /// otherwise whether the bar there takes the gesture: the limits it moves when the pointer is on the bar of a
    /// continuous scale, or of a discrete scale of automatic levels, held by a panel or an axes, and `None` on any
    /// other part of a colourbar, which the gesture leaves alone.
    ///
    /// Manual limits are read from the displayed figure rather than from the hit map, which may have been compiled
    /// before the latest edit, so that two quick gestures compose; automatic limits are the ones the bar was drawn
    /// with, which the compiler resolved.
    fn bar_gesture(&self, hit: &HitMap, at: Point) -> Option<Option<ScaleTarget>> {
        let (bar, part) = colorbar_under(hit, at)?;
        if part != BarPart::Bar || matches!(bar.owner, ColorbarOwner::Legend(_)) {
            return Some(None);
        }
        let (Some(drawn), Some(scale)) = (bar.map, self.composed.color_scale(bar.scale)) else {
            return Some(None);
        };
        let (path, limits, norm) = match &scale.kind {
            ColorScaleKind::Continuous(continuous) => (
                path(&["kind", "continuous", "limits"]),
                continuous.limits,
                continuous.norm,
            ),
            ColorScaleKind::Discrete(DiscreteColorScale {
                levels: ColorScaleLevels::Auto { limits, .. },
                norm,
                ..
            }) => (
                path(&["kind", "discrete", "levels", "limits"]),
                *limits,
                *norm,
            ),
            _ => return Some(None),
        };
        let (min, max) = match limits {
            Limits::Manual { min, max } => (min, max),
            Limits::Auto => (drawn.min, drawn.max),
        };
        Some(Some(ScaleTarget {
            scale: bar.scale,
            path,
            map: AxisMap {
                min,
                max,
                log: norm == ColorScaleNorm::Log,
                ..drawn
            },
            vertical: bar.orientation == ColorbarOrientation::Vertical,
        }))
    }

    /// Sets the manual limits of a scale given in the scale space of its map (base-10 logarithms under a logarithmic
    /// norm). Returns whether the figure changed; limits that are not finite, not increasing or, under a logarithmic
    /// norm, not positive are refused before anything is recorded.
    fn set_scale_limits(&mut self, target: &ScaleTarget, (a, b): (f64, f64)) -> bool {
        let (min, max) = (from_scale(&target.map, a), from_scale(&target.map, b));
        let usable =
            min.is_finite() && max.is_finite() && min < max && (!target.map.log || min > 0.0);
        usable
            && self.record(&Transaction {
                edits: vec![Edit::Set {
                    node: target.scale,
                    path: target.path.clone(),
                    value: Value::Limits(Limits::Manual { min, max }),
                }],
            })
    }

    /// Restores the limits of a scale captured at the start of a drag, unless the figure still shows them (including
    /// when they are automatic and unchanged). Returns whether the figure changed.
    fn restore_scale(&mut self, target: &ScaleTarget) -> bool {
        match self.composed.get(target.scale, &target.path) {
            Ok(Value::Limits(Limits::Manual { .. })) => {
                let (min, max) = scale_limits(&target.map);
                self.set_scale_limits(target, (min, max))
            }
            _ => false,
        }
    }

    /// Removes the view entries of every axes from the overlay, so that the figure shows the limits and
    /// three-dimensional views of the source again, and keeps the visibility of every artist.
    ///
    /// Returns whether the figure changed.
    pub fn reset_view(&mut self) -> bool {
        self.end_drag();
        let before = self.overlay.entries().to_vec();
        self.overlay.reset_all_views();
        if self.overlay.entries() == before {
            return false;
        }
        self.recompose();
        true
    }

    /// Returns whether the displayed figure has at least one 3D axes.
    #[must_use]
    pub fn has_3d(&self) -> bool {
        self.composed
            .axes
            .iter()
            .any(|axes| matches!(axes.projection, Projection::ThreeD { .. }))
    }

    /// Composes the displayed figure from the source and the overlay, discarding the entries that it cannot apply and
    /// keeping their reasons as problems.
    fn recompose(&mut self) {
        let composition = self.overlay.compose(&self.source);
        // The overlay has changed, so every problem raised by the changes it used to hold
        // describes a change that is no longer being made.
        self.problems.clear();
        for dropped in &composition.dropped {
            let problem = Problem {
                origin: Origin::Discarded,
                node: Some(dropped.entry.node),
                path: Some(dropped.entry.path.clone()),
                detail: reason(&dropped.reason),
            };
            self.report(problem);
        }
        self.overlay.discard(&composition.dropped);
        self.composed = composition.figure;
        let selected = self.selected;
        self.select(selected);
    }

    /// Adds a problem, unless it is already reported.
    fn report(&mut self, problem: Problem) {
        if !self.problems.contains(&problem) {
            self.problems.push(problem);
        }
    }

    /// Runs a gesture whose changes are one step of the undo history, unless a drag is already open, in which case
    /// they belong to the drag.
    fn step(&mut self, gesture: impl FnOnce(&mut Self) -> bool) -> bool {
        let open = self.drag.is_some();
        if !open {
            self.overlay.begin_step();
        }
        let changed = gesture(self);
        if !open {
            self.overlay.end_step();
        }
        changed
    }

    /// Ends the drag, closing its undo step.
    fn end_drag(&mut self) {
        self.drag = None;
        self.overlay.end_step();
    }

    /// Moves through the undo history and recomposes the displayed figure. Returns whether there was a step.
    fn undone(&mut self, step: impl FnOnce(&mut Overlay) -> bool) -> bool {
        self.end_drag();
        if !step(&mut self.overlay) {
            return false;
        }
        self.recompose();
        true
    }

    /// Returns whether an edit changes the displayed figure, so that a set of the value a property already has is
    /// left out of the overlay.
    fn changes_the_figure(&self, edit: &Edit) -> bool {
        match edit {
            Edit::Set { node, path, value } => match self.composed.get(*node, path) {
                Ok(current) => current != *value,
                Err(_) => true,
            },
            _ => true,
        }
    }

    /// Returns the 3D view of an axes of the displayed figure, or `None` when it is not a 3D axes.
    fn view(&self, id: NodeId) -> Option<View3d> {
        match self.composed.axes(id)?.projection {
            Projection::ThreeD { view3d } => Some(view3d),
            Projection::TwoD => None,
        }
    }

    /// Sets properties of the 3D view of an axes, each by name, such as `zoom` or `azimuth_deg`. Returns whether the
    /// figure changed.
    fn set_view(&mut self, id: NodeId, properties: &[(&str, f64)]) -> bool {
        if self.view(id).is_none() {
            return false;
        }
        let edits = properties
            .iter()
            .map(|&(property, value)| Edit::Set {
                node: id,
                path: path(&["projection", "view3d", property]),
                value: Value::Double(value),
            })
            .collect();
        self.record(&Transaction { edits })
    }

    /// Sets manual limits given in the scale space of `map` (base-10 logarithms on a log axis). Returns whether the
    /// figure changed; invalid limits (for example after overflow) are ignored.
    fn set_scaled_limits(
        &mut self,
        id: NodeId,
        dimension: Dimension,
        map: &AxisMap,
        (a, b): (f64, f64),
    ) -> bool {
        let (min, max) = (from_scale(map, a), from_scale(map, b));
        self.set_manual(id, dimension, min, max)
    }

    /// Sets manual limits on an axes and on the axes linked with it. Returns whether the figure changed.
    fn set_manual(&mut self, id: NodeId, dimension: Dimension, min: f64, max: f64) -> bool {
        let limits = Limits::Manual { min, max };
        match command::set_limits(&self.composed, id, dimension, limits) {
            Ok(transaction) => self.record(&transaction),
            Err(_) => false,
        }
    }

    /// Restores the limits captured at the start of a drag, unless the figure still shows them (including when they
    /// are automatic and unchanged). Returns whether the figure changed.
    fn restore_captured(&mut self, id: NodeId, dimension: Dimension, map: &AxisMap) -> bool {
        let Some(axes) = self.composed.axes(id) else {
            return false;
        };
        match axis(axes, dimension).limits {
            Limits::Auto => false,
            Limits::Manual { min, max } if min == map.min && max == map.max => false,
            Limits::Manual { .. } => self.set_manual(id, dimension, map.min, map.max),
        }
    }
}

/// Why an overlay entry could not be applied, in a sentence fit for the problems indicator.
///
/// The validation errors that an entry would introduce are given by their messages, because the error itself formats
/// them as the debug form of the whole report.
fn reason(error: &EditError) -> String {
    match error {
        EditError::Invalid(issues) => issues
            .iter()
            .map(|issue| issue.message.clone())
            .collect::<Vec<String>>()
            .join("; "),
        other => other.to_string(),
    }
}

/// The problem raised by a transaction that the figure refused, naming the property it concerned.
///
/// The error of an edit names its own path; an error of validation names none, so the path of the edit the error
/// concerns, or of the first set of the transaction, is used instead.
fn refusal(transaction: &Transaction, error: &EditError) -> Problem {
    let named = transaction.edits.first().and_then(|edit| match edit {
        Edit::Set { node, path, .. } => Some((*node, path.clone())),
        _ => None,
    });
    Problem {
        origin: Origin::Refused,
        node: named.as_ref().map(|(node, _)| *node),
        path: named.map(|(_, path)| path),
        detail: reason(error),
    }
}

/// The property path of the given segments, which are the field names of the wire schema.
fn path(segments: &[&str]) -> PropertyPath {
    PropertyPath::new(segments.iter().copied()).expect("the segments name a property")
}

/// The kind of image a pixel, or of artist a cell, is read from, which decides the shape of its array and the form
/// of its value.
#[derive(Clone, Copy)]
enum PixelKind<'a> {
    /// A true-colour image, whose array has a third dimension of components.
    Components,
    /// A scalar image through an indexed colour scale, whose two-dimensional array holds indices.
    Index,
    /// A scalar image through a categorical colour scale, whose two-dimensional array holds the indices of its
    /// categories.
    Category(&'a CategoricalColorScale),
    /// A scalar image through a continuous or discrete colour scale, whose two-dimensional array holds values.
    Value,
}

/// Reads what `array` stores for the pixel in `row` and `column` of an image of `kind`, or `None` when the array is
/// not of the shape the kind requires or has no such pixel.
///
/// An 8-bit value is widened to the number it denotes and a floating-point value is read as it is, so the result is
/// in the units the array stores.
fn pixel_value(array: &NdArray, kind: PixelKind, row: usize, column: usize) -> Option<PixelValue> {
    let (ny, nx, channels) = match (kind, array.shape.as_slice()) {
        (PixelKind::Components, &[ny, nx, channels]) => (ny, nx, channels),
        (PixelKind::Index | PixelKind::Category(_) | PixelKind::Value, &[ny, nx]) => (ny, nx, 1),
        _ => return None,
    };
    if row >= ny || column >= nx {
        return None;
    }
    let start = (row * nx + column) * channels;
    Some(match kind {
        PixelKind::Components => PixelValue::Components(
            (start..start + channels)
                .map(|k| array.get(k))
                .collect::<Option<Vec<f64>>>()?,
        ),
        PixelKind::Index => PixelValue::Index(array.get(start)?),
        PixelKind::Category(scale) => {
            let index = array.get(start)?;
            let entries = ColorScaleDomain::Entries {
                count: scale.categories.len(),
            };
            let label = match entries.classify(index) {
                ColorScaleClass::Inside(ColorScaleSlot::Entry(entry)) => scale
                    .categories
                    .get(entry)
                    .map(|category| category.label.content.clone()),
                _ => None,
            };
            PixelValue::Category { index, label }
        }
        PixelKind::Value => PixelValue::Value(array.get(start)?),
    })
}

/// Returns the data coordinate of the centre of pixel `i` along one axis of an image of `n` pixels whose first and
/// last centres `range` gives, or which lie at 0, 1, …, n − 1 when it is absent.
///
/// This is where the scene compiler puts the pixels: the pitch between centres is `(last − first) / (n − 1)`, so
/// a range that runs backwards gives a negative pitch and pixel 0 stays at `first`; a single pixel has no pitch to
/// derive, so it lies at `first` and its `last` is ignored.
fn pixel_centre(range: Option<PixelRange>, n: usize, i: usize) -> f64 {
    match range {
        Some(PixelRange { first, last }) if n > 1 => {
            first + i as f64 * ((last - first) / (n - 1) as f64)
        }
        Some(PixelRange { first, .. }) => first,
        None => i as f64,
    }
}

fn is_finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn axis(axes: &Axes, dimension: Dimension) -> &Axis {
    match dimension {
        Dimension::X => &axes.x,
        Dimension::Y => &axes.y,
        Dimension::Z => &axes.z,
    }
}

/// Returns the x and y axis maps of a 2D axes to navigate from, as [`current_map`] gives them.
///
/// An axes whose fixed aspect is kept by widening its limits draws wider limits than the manual limits it stores, so
/// the manual limits are widened here exactly as the scene compiler widens them for the plot rectangle of the hit map;
/// a gesture then starts from the limits the reader sees, and a gesture from limits already widened finds nothing to
/// widen.
fn current_maps(axes: &Axes, x: &AxisMap, y: &AxisMap) -> (AxisMap, AxisMap) {
    let (mut x, mut y) = (current_map(&axes.x, x), current_map(&axes.y, y));
    if let Some(ratio) = fixed_ratio(axes)
        && axes.aspect_adjust == AspectAdjust::Limits
    {
        let ((x0, x1), (y0, y1)) = aspect::widen(
            scale_limits(&x),
            scale_limits(&y),
            (x.end - x.start).abs(),
            (y.end - y.start).abs(),
            ratio,
        );
        let unscaled = |map: &AxisMap, a: f64, b: f64| (from_scale(map, a), from_scale(map, b));
        let ((xmin, xmax), (ymin, ymax)) = (unscaled(&x, x0, x1), unscaled(&y, y0, y1));
        if xmin.is_finite() && xmax.is_finite() && xmin < xmax {
            (x.min, x.max) = (xmin, xmax);
        }
        if ymin.is_finite() && ymax.is_finite() && ymin < ymax {
            (y.min, y.max) = (ymin, ymax);
        }
    }
    (x, y)
}

/// Returns the ratio of a 2D axes whose aspect is fixed and can be kept, or `None`.
fn fixed_ratio(axes: &Axes) -> Option<f64> {
    let [ratio, _] = axes.aspect.ratios()?;
    (axes.projection == Projection::TwoD && ratio.is_finite() && ratio > 0.0).then_some(ratio)
}

/// Returns `band` with its shorter side, relative to the shape of `plot`, expanded about its centre so that the band
/// has the shape of `plot`.
///
/// A plot drawn at a fixed aspect and zoomed to a band of its own shape keeps that aspect without changing the shape
/// of its plot area or widening its limits, and expanding rather than shrinking keeps everything the reader selected.
fn to_shape_of(band: Rect, plot: Rect) -> Rect {
    if !(band.width > 0.0 && band.height > 0.0 && plot.width > 0.0 && plot.height > 0.0) {
        return band;
    }
    let shape = plot.height / plot.width;
    let (width, height) = if band.height / band.width < shape {
        (band.width, band.width * shape)
    } else {
        (band.height / shape, band.height)
    };
    Rect::new(
        band.x + (band.width - width) / 2.0,
        band.y + (band.height - height) / 2.0,
        width,
        height,
    )
}

/// Returns the axis map to navigate from: the figure-space extent always comes from the hit map, but manual limits
/// and the scale come from the figure itself, so that a hit map compiled before the latest edit cannot undo it.
fn current_map(axis: &Axis, hit: &AxisMap) -> AxisMap {
    let (min, max) = match axis.limits {
        Limits::Manual { min, max } => (min, max),
        Limits::Auto => (hit.min, hit.max),
    };
    AxisMap {
        min,
        max,
        log: axis.scale == Scale::Log,
        start: hit.start,
        end: hit.end,
    }
}

fn to_scale(map: &AxisMap, value: f64) -> f64 {
    if map.log { value.log10() } else { value }
}

fn from_scale(map: &AxisMap, value: f64) -> f64 {
    if map.log { 10f64.powf(value) } else { value }
}

/// The limits of `map` in its scale space.
fn scale_limits(map: &AxisMap) -> (f64, f64) {
    (to_scale(map, map.min), to_scale(map, map.max))
}

/// Returns the limits, in the scale space of `map`, of a zoom by `factor` (greater than one zooms in) about the value
/// at figure-space coordinate `coord`, which stays where it is.
fn zoomed(map: &AxisMap, coord: f64, factor: f64) -> (f64, f64) {
    let (a, b) = scale_limits(map);
    let t = (coord - map.start) / (map.end - map.start);
    let c = a + t * (b - a);
    (c - (c - a) / factor, c + (b - c) / factor)
}

/// Returns the limits, in the scale space of `map`, of a pan that moves the value grabbed at figure-space coordinate
/// `from` to `to`.
fn panned(map: &AxisMap, from: f64, to: f64) -> (f64, f64) {
    let (a, b) = scale_limits(map);
    let per_point = (b - a) / (map.end - map.start);
    let shift = (from - to) * per_point;
    (a + shift, b + shift)
}

/// Returns the colourbar painted last under `p`, with the part of it there, or `None` where no colourbar lies.
///
/// The squares of a categorical scale are its geometry, so the gaps between them belong to nothing; an inside bar's
/// box belongs to its bar, so that what lies beneath stays hidden.
fn colorbar_under(hit: &HitMap, p: Point) -> Option<(&ColorbarHit, BarPart)> {
    hit.colorbars.iter().rev().find_map(|bar| {
        let part = if let Some(i) = bar.keys.iter().position(|key| key.rect.contains(p)) {
            BarPart::Key(i)
        } else if let Some(i) = bar.entries.iter().position(|square| square.contains(p)) {
            BarPart::Category(i)
        } else if bar.map.is_some() && bar.bar.contains(p) {
            BarPart::Bar
        } else if bar.backdrop.is_some_and(|backdrop| backdrop.contains(p)) {
            BarPart::Backdrop
        } else {
            return None;
        };
        Some((bar, part))
    })
}

/// Returns the band of `edges` that holds `value`: the band from the last edge at or below it, so that a value on an
/// edge belongs to the upper band, as the scale classifies it, with a value at or beyond the last edge in the last
/// band and one below the first in the first. A tolerance of a millionth of a millionth of the span absorbs the
/// rounding of a pointer placed exactly on an edge.
fn band_of(edges: &[f64], value: f64) -> Option<(f64, f64)> {
    let (first, last) = (*edges.first()?, *edges.last()?);
    if edges.len() < 2 || !value.is_finite() {
        return None;
    }
    let tolerance = 1e-12 * (last - first).abs();
    let band = edges[..edges.len() - 1]
        .iter()
        .rposition(|edge| *edge <= value + tolerance)
        .unwrap_or(0);
    Some((edges[band], edges[band + 1]))
}

/// Returns the rectangle of a bar from `low` to `high` along it, across its whole thickness.
fn span_of(bar: &ColorbarHit, map: &AxisMap, low: f64, high: f64) -> Rect {
    let (a, b) = (map.to_figure(low), map.to_figure(high));
    let r = bar.bar;
    if bar.orientation == ColorbarOrientation::Vertical {
        Rect::new(r.x, a.min(b), r.width, (a - b).abs())
    } else {
        Rect::new(a.min(b), r.y, (a - b).abs(), r.height)
    }
}

/// Returns the text tick label of an axis at `position`, when the axis has explicit ticks labelled with text and one
/// of them falls exactly there.
fn category_name(axis: &Axis, position: f64) -> Option<String> {
    let (Ticks::Values(values), TickLabels::Text(labels)) = (&axis.ticks, &axis.tick_labels) else {
        return None;
    };
    let at = values.iter().position(|value| *value == position)?;
    labels.get(at).map(|label| label.content.clone())
}

/// Returns the source text of the label of the tick an indexed scale places at entry `index`, when its ticks are
/// explicit values with explicit labels and one of them is that entry.
fn tick_label_at(scale: &IndexedColorScale, index: usize) -> Option<String> {
    let (Ticks::Values(values), TickLabels::Text(labels)) = (&scale.ticks, &scale.tick_labels)
    else {
        return None;
    };
    let at = values.iter().position(|value| *value == index as f64)?;
    labels.get(at).map(|label| label.content.clone())
}
