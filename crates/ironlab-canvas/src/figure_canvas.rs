//! One figure on a canvas, for any host: the logic of showing it that does not depend on what draws the interface.
//!
//! [`FigureCanvas`] owns what showing a figure needs whatever toolkit the window is built with: the [`FigureState`]
//! the gestures edit, the compilation of the displayed figure that every gesture is hit-tested against, the fit of
//! the page into the area the host gives it, the draw list kept for that fit, and what the host draws around the
//! figure: the cursor, the rubber band of a Zoom-tool drag and the callout of the data under the pointer. A host
//! forwards the gestures its toolkit has decided (a hover, a wheel notch, the start, moves and end of a drag, a click
//! and a double click), all in screen points, and the canvas converts each through the fit into a figure-space call
//! on the state, recompiling the scene after every change so that the next gesture and the same frame's drawing see
//! the figure as it now is. The host then draws in this order: its surround and the page
//! [`background`](FigureCanvas::background) inside the [`fit`](FigureCanvas::fit), the
//! [`draw_list`](FigureCanvas::draw_list) through the pipelines of [`crate::gpu`], and the chrome reported by
//! [`cursor`](FigureCanvas::cursor), [`callout`](FigureCanvas::callout) and
//! [`rubber_band`](FigureCanvas::rubber_band).
//!
//! The figure pane of the egui viewer, `ironlab-viewer`, is the egui host: it forwards the gestures egui decides
//! from its response and draws the reported chrome with egui's painter. A host without egui, such as a browser page,
//! has only raw pointer events; [`Pointer`] turns those into the same [`Gesture`]s by egui's rules and thresholds, so
//! that both hosts tell a press, a drag and a click apart in the same way and the figure behaves alike in each.

use std::sync::Arc;

use emath::{Pos2, Rect, vec2};
use ironlab_ir::Figure;
use ironlab_scene::Scene;
use ironlab_scene::display::Point;
use ironlab_scene::hit::ImageHit;
use ironlab_text::TextEngine;

use crate::canvas::{MAX_TILE_SIDE, Resolution, ScreenTransform, premultiplied, tessellate};
use crate::gpu::DrawList;
use crate::interaction::{Datatip, FigureState, PixelDatatip, PixelValue, Tip, Tool};
use crate::problems::Problem;

/// The rate at which a wheel scroll zooms: a scroll of `d` points zooms by `exp(d · rate)`, so that one notch of a
/// typical mouse wheel (50 points) zooms by about 20 %.
pub const WHEEL_ZOOM_RATE: f64 = 0.0036;

/// The smallest gap, in screen points, between the figure and the edges of its canvas.
pub const CANVAS_MARGIN: f32 = 22.0;

/// The factor by which the scale of a figure on screen may change before its draw list is rebuilt for the new
/// scale: within it, curves flattened for the old scale stay within a tenth of a pixel of true, and a hairline
/// stays within a quarter of a pixel of one pixel wide.
pub const REBUILD_RATIO: f32 = 1.25;

/// The radius, in screen points, of the ring drawn around the data point under the pointer, and around the centre
/// of a pixel too small on screen to outline.
pub const DATATIP_RING_POINTS: f32 = 4.0;

/// The zoom factor of one frame's wheel input: a scroll of `scroll_points` (positive away from the user) zooms by
/// `exp(scroll_points · WHEEL_ZOOM_RATE)`, and a pinch, given as the ratio it scaled by, multiplies that.
#[must_use]
pub fn wheel_factor(scroll_points: f32, pinch: f32) -> f64 {
    (f64::from(scroll_points) * WHEEL_ZOOM_RATE).exp() * f64::from(pinch)
}

/// Formats a data value for a datatip, with enough digits to tell neighbouring points apart and without the noise
/// that printing a binary fraction in full would add.
fn datatip_value(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    if value != 0.0 && !(1e-4..1e6).contains(&value.abs()) {
        return format!("{value:.4e}");
    }
    let text = format!("{value:.6}");
    match text.split_once('.') {
        Some(_) => text.trim_end_matches('0').trim_end_matches('.').to_owned(),
        None => text,
    }
}

/// The text of the callout that names the data point under the pointer, one item per line: the name of the series
/// when it has one, its coordinates, and the index the point has in the artist's own data arrays, which is what the
/// user would use to find the same point in the data they plotted.
#[must_use]
pub fn datatip_text(tip: &Datatip) -> String {
    let mut lines = Vec::new();
    if let Some(name) = &tip.name {
        lines.push(name.clone());
    }
    lines.push(format!("x = {}", datatip_value(tip.x)));
    lines.push(format!("y = {}", datatip_value(tip.y)));
    if let Some(z) = tip.z {
        lines.push(format!("z = {}", datatip_value(z)));
    }
    lines.push(format!("index {}", tip.index));
    lines.join("\n")
}

/// The text of the callout that names the pixel under the pointer, one item per line: the name of the image when
/// it has one, the row and column of the pixel in the artist's own array, the coordinates of the pixel's centre,
/// and what the array holds there, every number formatted as the point datatip formats its coordinates, so that
/// the two callouts read as one.
///
/// The last line takes the words of the image's kind: `value = …` for a colour-mapped image, `index = …` for a
/// colour-indexed one, and for a true-colour image `rgb = …` or `rgba = …` listing the components in that order,
/// separated by commas.
#[must_use]
pub fn pixel_datatip_text(tip: &PixelDatatip) -> String {
    let mut lines = Vec::new();
    if let Some(name) = &tip.name {
        lines.push(name.clone());
    }
    lines.push(format!("row {}, column {}", tip.row, tip.column));
    lines.push(format!("x = {}", datatip_value(tip.x)));
    lines.push(format!("y = {}", datatip_value(tip.y)));
    lines.push(match &tip.value {
        PixelValue::Value(value) => format!("value = {}", datatip_value(*value)),
        PixelValue::Index(index) => format!("index = {}", datatip_value(*index)),
        PixelValue::Components(components) => {
            let label = match components.len() {
                3 => "rgb",
                4 => "rgba",
                _ => "components",
            };
            let listed: Vec<String> = components.iter().copied().map(datatip_value).collect();
            format!("{label} = {}", listed.join(", "))
        }
    });
    lines.join("\n")
}

/// The corners of the pixel in `row` and `column` of a drawn image, on screen and in the order they are joined, or
/// `None` when the pixel is smaller on screen than the ring drawn around a point, so that it is ringed instead and
/// the mark is never too small to see, or when its placement cannot be inverted.
fn pixel_outline(
    image: &ImageHit,
    row: usize,
    column: usize,
    to_screen: ScreenTransform,
) -> Option<[Pos2; 4]> {
    let to_figure = image.to_pixel.inverse()?;
    let (c, r) = (column as f64, row as f64);
    let corners = [(c, r), (c + 1.0, r), (c + 1.0, r + 1.0), (c, r + 1.0)]
        .map(|(x, y)| to_screen.apply(to_figure.apply(Point::new(x, y))));
    let bounds = Rect::from_points(&corners);
    let diameter = 2.0 * DATATIP_RING_POINTS;
    (bounds.width() >= diameter && bounds.height() >= diameter).then_some(corners)
}

/// The cursor a host shows over the canvas, which is the only sign of the active tool before the user drags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    /// An open hand: the Pan tool at rest.
    Grab,
    /// A closed hand: the Pan tool while dragging.
    Grabbing,
    /// The Zoom tool.
    Crosshair,
    /// The Rotate tool.
    Move,
}

/// The mark drawn on the canvas at the data a callout names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Marker {
    /// A ring of `radius` screen points about `centre`: around a drawn point, or around the centre of a pixel too
    /// small on screen to outline.
    Ring { centre: Pos2, radius: f32 },
    /// The four corners of a pixel on screen, in the order they are joined, so that the reader sees the extent that
    /// was read.
    Outline([Pos2; 4]),
}

/// What a host shows for the data under the pointer: a mark on the canvas and the text of a tooltip.
#[derive(Clone, Debug, PartialEq)]
pub struct Callout {
    pub marker: Marker,
    pub text: String,
}

/// Where the figure lies in the host's area: the mapping from figure points to screen points, and the rectangle
/// the page covers on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub to_screen: ScreenTransform,
    pub page: Rect,
}

/// The draw list of a figure and the scale it was built for.
struct Built {
    list: Arc<DrawList>,
    scale: f32,
}

/// One figure on a canvas: its interaction state, its compilation, its fit into the host's area and the draw list
/// kept for that fit.
///
/// The scene is compiled when it is first needed and recompiled after every change to the figure, whether made by a
/// gesture here or through [`state_mut`](Self::state_mut), so that each gesture is hit-tested against the geometry
/// that is on screen. The draw list is in figure points and is rebuilt only when the scene changes or the figure's
/// scale on screen has moved beyond [`REBUILD_RATIO`] from the scale it was flattened for; a pan, a resize within
/// that band or a frame in which nothing moved hands the host the same list, which its painter then draws from the
/// buffers it already holds.
pub struct FigureCanvas {
    state: FigureState,
    /// The compilation of the displayed figure, or `None` when it must be recompiled.
    scene: Option<Scene>,
    /// The size of the page in points, as the scene was last compiled; it survives an invalidation so that a drag
    /// can be converted through the fit between a change and the recompilation that follows it.
    page_pt: Option<[f32; 2]>,
    /// The draw list of `scene`, or `None` when it must be rebuilt.
    built: Option<Built>,
    /// The area of the host the figure is fitted into, in screen points.
    area: Rect,
    /// The largest texture side the host's device allows, which bounds the tiles of an image.
    max_tile_side: u32,
    /// Where the pointer is over the canvas, or `None` when it is not over it.
    pointer: Option<Pos2>,
    /// Whether a drag is in progress.
    dragging: bool,
}

impl FigureCanvas {
    /// A canvas showing `figure`, with no area yet: nothing fits until [`resize`](Self::resize) gives it one.
    #[must_use]
    pub fn new(figure: Figure) -> Self {
        Self {
            state: FigureState::new(figure),
            scene: None,
            page_pt: None,
            built: None,
            area: Rect::ZERO,
            max_tile_side: MAX_TILE_SIDE,
            pointer: None,
            dragging: false,
        }
    }

    /// The interaction state of the figure.
    #[must_use]
    pub fn state(&self) -> &FigureState {
        &self.state
    }

    /// The interaction state of the figure, mutably.
    ///
    /// The scene is recompiled before it is next used, so that any change made through this reference is shown.
    pub fn state_mut(&mut self) -> &mut FigureState {
        self.invalidate();
        &mut self.state
    }

    /// Applies `edit` to the interaction state and, when it reports that it changed the figure, marks the scene
    /// out of date. This is for a caller that knows whether it changed the figure, such as a toolbar or a property
    /// editor, so that a frame in which it changed nothing keeps the scene and the draw list.
    pub fn edit(&mut self, edit: impl FnOnce(&mut FigureState) -> bool) {
        if edit(&mut self.state) {
            self.invalidate();
        }
    }

    /// Marks the scene and the draw list as out of date after a change to the figure.
    pub fn invalidate(&mut self) {
        self.scene = None;
        self.built = None;
    }

    /// Compiles the scene if it is out of date and returns it.
    pub fn scene(&mut self, text: &TextEngine) -> &Scene {
        if self.scene.is_none() {
            self.built = None;
        }
        let scene = self
            .scene
            .get_or_insert_with(|| ironlab_scene::compile(self.state.figure(), text));
        self.page_pt = Some([
            scene.display_list.width_pt as f32,
            scene.display_list.height_pt as f32,
        ]);
        scene
    }

    /// Everything wrong with the figure as shown: the warnings of the scene compiler, in its order, then the
    /// problems the user's own changes raised, so that the reader sees what the figure cannot draw before what
    /// their edits could not do.
    pub fn problems(&mut self, text: &TextEngine) -> Vec<Problem> {
        let mut problems: Vec<Problem> = self
            .scene(text)
            .warnings
            .iter()
            .map(Problem::from_scene)
            .collect();
        problems.extend(self.state.problems().iter().cloned());
        problems
    }

    /// Gives the canvas the host's `area`, in screen points, and the largest texture side the host's device allows.
    pub fn resize(&mut self, area: Rect, max_tile_side: u32) {
        self.area = area;
        self.max_tile_side = max_tile_side;
    }

    /// Where the figure lies in the area: the page scaled uniformly to fit inside [`CANVAS_MARGIN`] and centred, or
    /// `None` when the page has no size or the area has no room for it inside the margin, in which case there is
    /// nothing to draw and nothing to read input against.
    pub fn fit(&mut self, text: &TextEngine) -> Option<Fit> {
        self.scene(text);
        self.fit_of_page()
    }

    /// The fit of the page as the scene was last compiled, which is the current fit whenever the host has compiled
    /// the scene since the last change, as it does every frame before it forwards a gesture.
    fn fit_of_page(&self) -> Option<Fit> {
        let [width_pt, height_pt] = self.page_pt?;
        let inner = self.area.shrink(CANVAS_MARGIN);
        if !(width_pt > 0.0 && height_pt > 0.0 && inner.width() > 0.0 && inner.height() > 0.0) {
            return None;
        }
        let scale = (inner.width() / width_pt).min(inner.height() / height_pt);
        let size = vec2(width_pt * scale, height_pt * scale);
        let origin = self.area.center() - size / 2.0;
        Some(Fit {
            to_screen: ScreenTransform { scale, origin },
            page: Rect::from_min_size(origin, size),
        })
    }

    /// The colour the page is filled with before the draw list is painted over it, as premultiplied sRGB bytes, or
    /// `None` when the background is fully transparent, so that the host leaves its surround showing through.
    pub fn background(&mut self, text: &TextEngine) -> Option<[u8; 4]> {
        premultiplied(self.scene(text).display_list.background).filter(|colour| colour[3] > 0)
    }

    /// Tells the canvas where the pointer is over it, or that it is not over it.
    pub fn hover(&mut self, pointer: Option<Pos2>) {
        self.pointer = pointer;
    }

    /// Zooms the axes under `pos` by `factor` (see [`wheel_factor`]) about the data under the pointer. Returns
    /// whether the figure changed; a factor of one, or a position over no axes, changes nothing.
    pub fn wheel(&mut self, pos: Pos2, factor: f64, text: &TextEngine) -> bool {
        if factor == 1.0 {
            return false;
        }
        let Some(fit) = self.fit(text) else {
            return false;
        };
        let hit = &self.scene.as_ref().expect("compiled by the fit").hit_map;
        let changed = self.state.scroll(hit, fit.to_screen.invert(pos), factor);
        if changed {
            self.invalidate();
        }
        changed
    }

    /// Begins a drag at `pos` with the active tool.
    pub fn drag_start(&mut self, pos: Pos2, text: &TextEngine) {
        let Some(fit) = self.fit(text) else {
            return;
        };
        let hit = &self.scene.as_ref().expect("compiled by the fit").hit_map;
        self.state.drag_start(hit, fit.to_screen.invert(pos));
        self.dragging = true;
    }

    /// Moves the drag in progress to `pos`. Returns whether the figure changed, which a Pan or Rotate drag does and
    /// a Zoom-tool drag, which moves its band, does not.
    pub fn drag_move(&mut self, pos: Pos2) -> bool {
        let Some(fit) = self.fit_of_page() else {
            return false;
        };
        let changed = self.state.drag_update(fit.to_screen.invert(pos));
        if changed {
            self.invalidate();
        }
        changed
    }

    /// Ends the drag in progress at `pos`, or with no position when the host lost the pointer before the button was
    /// released, which closes the drag where its last move left it. Returns whether the figure changed.
    pub fn drag_end(&mut self, pos: Option<Pos2>) -> bool {
        let at = pos
            .and_then(|pos| self.fit_of_page().map(|fit| fit.to_screen.invert(pos)))
            .unwrap_or(Point::new(f64::NAN, f64::NAN));
        self.dragging = false;
        let changed = self.state.drag_end(at);
        if changed {
            self.invalidate();
        }
        changed
    }

    /// Clicks at `pos`, which toggles a legend entry or selects an axes. Returns whether the figure changed.
    pub fn click(&mut self, pos: Pos2, text: &TextEngine) -> bool {
        let Some(fit) = self.fit(text) else {
            return false;
        };
        let hit = &self.scene.as_ref().expect("compiled by the fit").hit_map;
        let changed = self.state.click(hit, fit.to_screen.invert(pos));
        if changed {
            self.invalidate();
        }
        changed
    }

    /// Double-clicks at `pos`, which restores the view of the axes there. Returns whether the figure changed.
    pub fn double_click(&mut self, pos: Pos2, text: &TextEngine) -> bool {
        let Some(fit) = self.fit(text) else {
            return false;
        };
        let hit = &self.scene.as_ref().expect("compiled by the fit").hit_map;
        let changed = self.state.double_click(hit, fit.to_screen.invert(pos));
        if changed {
            self.invalidate();
        }
        changed
    }

    /// Sets the tool a drag uses.
    pub fn set_tool(&mut self, tool: Tool) {
        self.state.tool = tool;
    }

    /// Undoes the last change. Returns whether the figure changed.
    pub fn undo(&mut self) -> bool {
        let changed = self.state.undo();
        if changed {
            self.invalidate();
        }
        changed
    }

    /// Redoes the last undone change. Returns whether the figure changed.
    pub fn redo(&mut self) -> bool {
        let changed = self.state.redo();
        if changed {
            self.invalidate();
        }
        changed
    }

    /// Restores the view of every axes, as the toolbar's Refit does. Returns whether the figure changed.
    pub fn reset_view(&mut self) -> bool {
        let changed = self.state.reset_view();
        if changed {
            self.invalidate();
        }
        changed
    }

    /// Whether there is a change to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.state.can_undo()
    }

    /// Whether there is an undone change to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.state.can_redo()
    }

    /// Whether the displayed figure has a three-dimensional axes, which is what the Rotate tool turns.
    #[must_use]
    pub fn has_3d(&self) -> bool {
        self.state.has_3d()
    }

    /// Whether a drag is in progress.
    #[must_use]
    pub fn dragging(&self) -> bool {
        self.dragging
    }

    /// The cursor to show over the canvas: a hand for the Pan tool, closed while dragging, a crosshair for the Zoom
    /// tool and a move cursor for the Rotate tool.
    #[must_use]
    pub fn cursor(&self) -> Cursor {
        match self.state.tool {
            Tool::Pan if self.dragging => Cursor::Grabbing,
            Tool::Pan => Cursor::Grab,
            Tool::Zoom => Cursor::Crosshair,
            Tool::Rotate => Cursor::Move,
        }
    }

    /// The rubber band of a Zoom-tool drag in progress, in screen points, or `None` when there is none.
    pub fn rubber_band(&mut self, text: &TextEngine) -> Option<Rect> {
        let band = self.state.rubber_band()?;
        let fit = self.fit(text)?;
        Some(Rect::from_min_max(
            fit.to_screen.apply(Point::new(band.x, band.y)),
            fit.to_screen.apply(Point::new(band.right(), band.bottom())),
        ))
    }

    /// What lies under the pointer, a drawn data point or, where none is within reach, the pixel of an image, as a
    /// mark and a text, or `None` while dragging, when the pointer is not over the canvas, or where the figure draws
    /// nothing.
    ///
    /// Both come from the hit map of the current compilation, so a point names the index and the values of the
    /// user's own data even where the series was thinned to fit the view, and a pixel names the row and column of
    /// the user's own array. A point is ringed. A pixel is outlined, so that the reader sees the extent that was
    /// read, or ringed at its centre when it is smaller on screen than the ring, so that the mark is never too small
    /// to see.
    pub fn callout(&mut self, text: &TextEngine) -> Option<Callout> {
        if self.dragging {
            return None;
        }
        let pointer = self.pointer?;
        let fit = self.fit(text)?;
        let to_screen = fit.to_screen;
        let hit = &self.scene.as_ref().expect("compiled by the fit").hit_map;
        let at = to_screen.invert(pointer);
        let ring = |position: Point| Marker::Ring {
            centre: to_screen.apply(position),
            radius: DATATIP_RING_POINTS,
        };
        Some(match self.state.tip_at(hit, at)? {
            Tip::Point(tip) => Callout {
                marker: ring(tip.position),
                text: datatip_text(&tip),
            },
            Tip::Pixel(tip) => {
                let outline = hit
                    .pixel_at(at)
                    .and_then(|(image, row, column)| pixel_outline(image, row, column, to_screen));
                Callout {
                    marker: outline.map_or_else(|| ring(tip.position), Marker::Outline),
                    text: pixel_datatip_text(&tip),
                }
            }
        })
    }

    /// The draw list of the figure with the mapping to draw it by, or `None` when there is no fit.
    ///
    /// The list is rebuilt when there is none, when the figure changed, or when the scale of the fit has moved
    /// beyond [`REBUILD_RATIO`] either way from the scale the list was built at; otherwise it is the same `Arc` as
    /// last time, so that the host's painter draws it from the buffers it already holds.
    pub fn draw_list(&mut self, text: &TextEngine) -> Option<(Arc<DrawList>, ScreenTransform)> {
        let fit = self.fit(text)?;
        let scale = fit.to_screen.scale;
        let rebuild = self.built.as_ref().is_none_or(|built| {
            scale > built.scale * REBUILD_RATIO || scale < built.scale / REBUILD_RATIO
        });
        if rebuild {
            let scene = self.scene.as_ref().expect("compiled by the fit");
            let list = tessellate(
                &scene.display_list,
                text,
                Resolution {
                    scale,
                    max_tile_side: self.max_tile_side.min(MAX_TILE_SIDE),
                },
            );
            self.built = Some(Built {
                list: Arc::new(list),
                scale,
            });
        }
        let built = self.built.as_ref().expect("built above");
        Some((Arc::clone(&built.list), fit.to_screen))
    }

    /// The draw list last built, or `None` before the figure has been drawn or since it changed.
    #[must_use]
    pub fn built_list(&self) -> Option<Arc<DrawList>> {
        self.built.as_ref().map(|built| Arc::clone(&built.list))
    }
}

/// A gesture decided from raw pointer events, in screen points, as the egui host receives it from egui.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// A drag began, at the position the button was pressed.
    DragStart(Pos2),
    /// The drag in progress moved to this position.
    DragMove(Pos2),
    /// The drag in progress ended here, or with no position when the pointer was lost before the button was
    /// released.
    DragEnd(Option<Pos2>),
    /// A click, at the position the button was released.
    Click(Pos2),
    /// A second click soon after and close to the first, which is reported in place of a click.
    DoubleClick(Pos2),
}

/// A press of the primary button that has not yet been released.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Press {
    /// Where the button was pressed.
    origin: Pos2,
    /// When, in seconds.
    time: f64,
    /// Whether the pointer has moved further from the origin than a click allows.
    too_far: bool,
}

/// A click that registered, for deciding whether the next is a double click.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Click {
    pos: Pos2,
    time: f64,
}

/// Decides gestures from raw primary-button events, as egui decides them for the egui host.
///
/// A host without egui feeds it every press ([`down`](Self::down)), move ([`moved`](Self::moved)) and release
/// ([`up`](Self::up)) of the primary button, with the time of each in seconds, and forwards the [`Gesture`]s it
/// returns to a [`FigureCanvas`]. Its rules and thresholds are egui 0.36.2's, from `InputOptions::default` and
/// `PointerState::begin_pass` in `src/input_state/mod.rs` of that crate, with the strict comparisons egui uses:
///
/// - A press becomes a drag when the pointer moves further than [`MAX_CLICK_DISTANCE`](Self::MAX_CLICK_DISTANCE)
///   from where it was pressed, or when it has been held for longer than
///   [`MAX_CLICK_DURATION`](Self::MAX_CLICK_DURATION) when it next moves or is released. The drag starts at the
///   press origin, every move after that is a drag move, and the release ends the drag and is not a click.
/// - A release that did neither is a click, at the release position. It is a double click instead when it comes
///   within [`MAX_DOUBLE_CLICK_DELAY`](Self::MAX_DOUBLE_CLICK_DELAY) of the previous click and within
///   [`MAX_CLICK_DISTANCE`](Self::MAX_CLICK_DISTANCE) of it; a third such click is a plain click again, as egui
///   counts it a triple click, which the egui host acts on as a click. Only a release that registered as a click
///   records a click time, so a click soon after a drag is a plain click.
/// - A move with no button down, and a release with no press before it, decide nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pointer {
    press: Option<Press>,
    dragging: bool,
    last_click: Option<Click>,
    last_last_click_time: Option<f64>,
}

impl Pointer {
    /// egui's `InputOptions::max_click_dist`: a press that moves further than this from its origin, in screen
    /// points, is a drag.
    pub const MAX_CLICK_DISTANCE: f32 = 6.0;

    /// egui's `InputOptions::max_click_duration`, in seconds: a press held longer than this is a drag.
    pub const MAX_CLICK_DURATION: f64 = 0.8;

    /// egui's `InputOptions::max_double_click_delay`, in seconds: a click this soon after the last is a double
    /// click.
    pub const MAX_DOUBLE_CLICK_DELAY: f64 = 0.3;

    /// The primary button was pressed at `pos` at `time` seconds. A press decides nothing by itself; a press while
    /// a drag is in progress, whose release was lost, ends that drag with no position.
    pub fn down(&mut self, pos: Pos2, time: f64) -> Vec<Gesture> {
        let ended = if self.dragging {
            vec![Gesture::DragEnd(None)]
        } else {
            Vec::new()
        };
        self.dragging = false;
        self.press = Some(Press {
            origin: pos,
            time,
            too_far: false,
        });
        ended
    }

    /// The pointer moved to `pos` at `time` seconds.
    pub fn moved(&mut self, pos: Pos2, time: f64) -> Vec<Gesture> {
        let Some(press) = &mut self.press else {
            return Vec::new();
        };
        press.too_far |= press.origin.distance(pos) > Self::MAX_CLICK_DISTANCE;
        if self.dragging {
            return vec![Gesture::DragMove(pos)];
        }
        if press.too_far || time - press.time > Self::MAX_CLICK_DURATION {
            self.dragging = true;
            return vec![Gesture::DragStart(press.origin), Gesture::DragMove(pos)];
        }
        Vec::new()
    }

    /// The primary button was released at `pos`, or with no position when the pointer was lost, at `time` seconds.
    pub fn up(&mut self, pos: Option<Pos2>, time: f64) -> Vec<Gesture> {
        let Some(press) = self.press.take() else {
            return Vec::new();
        };
        if self.dragging {
            self.dragging = false;
            return vec![Gesture::DragEnd(pos)];
        }
        let too_far = press.too_far
            || pos.is_some_and(|pos| press.origin.distance(pos) > Self::MAX_CLICK_DISTANCE);
        if too_far || time - press.time > Self::MAX_CLICK_DURATION {
            return vec![Gesture::DragStart(press.origin), Gesture::DragEnd(pos)];
        }
        let Some(pos) = pos else {
            return Vec::new();
        };
        let within_distance = self.last_click.is_none_or(|last| {
            last.pos.distance_sq(pos) < Self::MAX_CLICK_DISTANCE * Self::MAX_CLICK_DISTANCE
        });
        let double = within_distance
            && self
                .last_click
                .is_some_and(|last| time - last.time < Self::MAX_DOUBLE_CLICK_DELAY);
        let triple = within_distance
            && self
                .last_last_click_time
                .is_some_and(|last| time - last < 2.0 * Self::MAX_DOUBLE_CLICK_DELAY);
        self.last_last_click_time = self.last_click.map(|last| last.time);
        self.last_click = Some(Click { pos, time });
        vec![if double && !triple {
            Gesture::DoubleClick(pos)
        } else {
            Gesture::Click(pos)
        }]
    }
}
