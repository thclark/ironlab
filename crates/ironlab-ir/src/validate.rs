//! Structural validation of figures.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::artist::{
    Artist, Bar, BarBase, BarSlot, CellEdges, Cells, Contour, ContourPlacement, Coords, Grid,
    Image, ImagePlacement, ImagePlane, Levels, PixelRange, Quiver, ScalarImage, Series, Stairs,
    StairsData, Surface,
};
use crate::axes::{Aspect, Axes, Axis, Limits, Projection, Scale, TickLabels, Ticks};
use crate::color_scale::{
    ColorMapping, ColorScale, ColorScaleClass, ColorScaleDomain, ColorScaleKind, ColorScaleLevels,
    ColorScaleNorm, Colormap, LISTED_COLORMAP_MAX, LISTED_COLORMAP_MIN, OutOfRange,
};
use crate::data::{DataArray, NdArray, NdArrayElement, Values};
use crate::figure::{Figure, Parameter};
use crate::ids::NodeId;
use crate::layout::{Cell, TrackSize};
use crate::legend::{
    Legend, LegendEntries, LegendEntryColumns, LegendEntryLayout, LegendItemContent,
};
use crate::link::Dimension;
use crate::nodes::{NodeKind, visit};
use crate::panel::{Panel, PanelContent};
use crate::style::ColorSpec;

/// The outcome of validating a figure.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    /// Problems that prevent the figure from being drawn as described.
    pub errors: Vec<ValidationIssue>,
    /// Problems that are tolerated, for example by omitting some data from drawing.
    pub warnings: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// Returns true when the report contains no errors.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// A single problem found by validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// The node at which the problem was found, if the problem belongs to a node.
    pub node: Option<NodeId>,
    /// The category of the problem.
    pub kind: IssueKind,
    /// A human-readable description of the problem.
    pub message: String,
}

/// The category of a validation problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssueKind {
    /// An artist refers to its data by an identifier that is not a node of the figure.
    UnknownData,
    /// A reference names a node of the figure that is not of the kind it requires: an
    /// artist's data that is not a data array, an artist's colour scale that is not a
    /// colour scale, the scale of a colourbar that is not a colour scale, the panel of a
    /// legend that is not a panel holding a legend, an item of an explicit list of legend
    /// entries that names an artist by a node that is not an artist, or such an item that
    /// names a colour scale by a node that is not a colour scale. It is reported against the node that holds the reference, and a reference
    /// that names no node at all is reported as [`IssueKind::UnknownData`],
    /// [`IssueKind::UnknownColorScale`], [`IssueKind::DanglingPanelLegend`] or
    /// [`IssueKind::DanglingLegendItem`] instead.
    WrongNodeKind,
    /// An artist, a colourbar or a legend item names its colour scale by an identifier
    /// that is not a node of the figure. It is reported against the node that holds the
    /// reference: the artist, the panel of a panel colourbar, the axes of an inside
    /// colourbar, or the owner of a legend that lists the scale (the panel of a legend
    /// panel, or the axes of an inside legend).
    UnknownColorScale,
    /// An artist maps numbers to colours but names no colour scale: a scalar image, cells, a
    /// contour whose bands are filled or whose isolines are colormapped or automatic, a
    /// surface whose faces or edges are colormapped or automatic, or a series with colour
    /// data, whatever its colours. The axes hold no colormap to fall back on, so the
    /// artist cannot be coloured. It is reported against the artist. A colormapped quiver,
    /// or a colormapped colour of a series without colour data, maps no number and needs
    /// no scale.
    MissingColorScale,
    /// A colour scale cannot map any value: an indexed scale with no entries, or a
    /// categorical scale with no categories. It is reported against the scale, whether or
    /// not an artist names it, because a colourbar may show it alone.
    InvalidColorScale,
    /// The listed colormap of a continuous or discrete colour scale holds fewer than 2 or
    /// more than 1024 colours. It is reported against the scale.
    InvalidColormap,
    /// A data array's number of values differs from the product of its shape. It is
    /// reported against the data array.
    InvalidArray,
    /// A data array that no artist refers to. This is a warning, not an error, because an
    /// array may be inserted before the artist that will use it, but an array that stays
    /// unused costs space in every saved file. An artist that is hidden still refers to
    /// its data.
    UnusedData,
    /// The arrays referenced by an artist have inconsistent lengths or shapes. The colour
    /// data of a series is such a mismatch unless it holds one value per point, or one
    /// value per segment (one fewer than the points) for a series that draws a line and
    /// has no colormapped marker face or edge, since a marker colour needs a value at its
    /// point. The arrays of bars and stairs must be one-dimensional, with one entry per bar
    /// (the per-bar colours included, when there are any), one more edge than values, or
    /// one sample per value; the values of cells must be two-dimensional, with one more
    /// column edge than columns, one more row edge than rows, and alpha of their shape.
    ShapeMismatch,
    /// An artist refers to an array whose element type it cannot use: every artist
    /// other than an image or cells requires 64-bit floating-point values, so such an
    /// artist cannot plot an array of 8-bit values, and the alpha of a surface must hold
    /// 64-bit floating-point values too. The two image kinds accept either element
    /// type, for their alpha as for their pixels or values, and so do the values and the
    /// alpha of cells, whose edges must hold 64-bit floating-point values.
    ElementTypeMismatch,
    /// An artist or placement that needs the z axis (z or w data, contours at their
    /// level, an image on a wall) is used in a two-dimensional axes. A surface is not
    /// such an artist: in a two-dimensional axes it is seen from directly above.
    ThreeDArtistInTwoDAxes,
    /// A series places a dimension in axes-fraction coordinates in a
    /// three-dimensional axes, which has no plot area along its dimensions to take a
    /// fraction of.
    AxesFractionInThreeDAxes,
    /// A series releases its clip (`clip` is false) although it places no
    /// dimension in axes-fraction coordinates, or lies in a three-dimensional axes, so
    /// that the release has no effect: clipping is released only along a dimension
    /// placed as fractions of the plot area of a two-dimensional axes. This is a
    /// warning, not an error, because the artist is drawn, clipped to the plot area.
    IneffectiveClip,
    /// An axis link refers to an identifier that is not an axes of the figure.
    DanglingLink,
    /// Two nodes share an identifier.
    DuplicateNodeId,
    /// The cell of an axes or of a panel lies outside the figure's tile layout or has a
    /// zero span.
    CellOutOfLayout,
    /// The cells of two nodes placed in the tile layout, axes or panels, share at least one
    /// tile, so that the later is drawn over the earlier. This is a warning, not an error,
    /// because the figure can still be drawn. Each overlapping pair is reported once,
    /// against the later of the two nodes in the order axes first, then panels, each in
    /// the order the figure lists them, and the message names both nodes.
    /// A cell outside the tile layout, which is reported as [`IssueKind::CellOutOfLayout`],
    /// is not compared with any other.
    OverlappingCells,
    /// The legend of an axes names a panel by an identifier that is not a node of the
    /// figure.
    DanglingPanelLegend,
    /// The tile layout has more row sizes than rows or more column sizes than columns,
    /// or a weight that is not a finite positive number.
    InvalidLayoutWeights,
    /// An axes lies in fitted rows alone, or in fitted columns alone. A fitted row or
    /// column takes its size from the panels in it, and an axes has no content
    /// size of its own, so the axes would have no height or no width. An axes that spans
    /// a fitted row or column together with a weighted one is valid.
    AxesInFittedTrack,
    /// An item of an explicit list of legend entries names an identifier that is not a
    /// node of the figure, or, in a legend inside a plot area, an artist of another axes.
    DanglingLegendItem,
    /// An explicit list of legend entries holds the same item more than once: two items
    /// that name the same artist, or two items with the same label and sample. This is a
    /// warning, not an error, because the legend can still be drawn: every item is
    /// listed where it stands.
    DuplicateLegendItem,
    /// An item of an explicit list of legend entries names an artist that has no label,
    /// so the item shows its sample without a label. This is a warning, not an error,
    /// because the legend can still be drawn and a label can be given later, in code or
    /// in the viewer.
    UnnamedLegendItem,
    /// A legend, inside a plot area or in a panel, has a fixed number of columns of
    /// zero.
    InvalidLegendColumns,
    /// A colour-scale item of an explicit list of legend entries asks for a bar of zero
    /// rows. It is reported against the legend's owner (the panel, or the axes of a legend
    /// inside its plot area) whatever the kind of the scale or the row of the item, because
    /// a later change of either would make the count count.
    InvalidLegendRows,
    /// The figure's width, height or base font size is not a finite positive number.
    InvalidSize,
    /// Manual limits are not finite, are not strictly increasing, or are not positive on
    /// a logarithmic axis or under the logarithmic norm of a colour scale. The limits of
    /// a coordinate axis are reported against the axes, and the limits of a colour scale
    /// (or of its automatic band edges) against the scale.
    InvalidLimits,
    /// Contour levels are empty, not finite or not strictly increasing, or an automatic
    /// level count is zero, reported against the contour; or the band edges of a discrete
    /// colour scale are fewer than two, not finite, not strictly increasing or, under a
    /// logarithmic norm, not positive, or their automatic count is zero, reported against
    /// the scale.
    InvalidLevels,
    /// The ticks of a coordinate axis or of a colour scale cannot be placed: a regular
    /// step that is not a finite positive number or an origin that is not finite, an
    /// explicit tick value that is not finite, or an explicit tick value that is not
    /// positive on a logarithmic axis or scale. Only the axes that are drawn are checked:
    /// x and y, and z in a three-dimensional axes.
    InvalidTicks,
    /// The explicit tick labels of a coordinate axis or of a colour scale do not name its
    /// ticks one by one: the ticks are not explicit values, or the number of labels
    /// differs from the number of values.
    InvalidTickLabels,
    /// The aspect of an axes cannot be kept: a ratio of y or of z that is not a finite
    /// positive number. The ratio of z is checked on a two-dimensional axes too, which
    /// ignores it, so that a figure stays valid when it is made three-dimensional.
    InvalidAspect,
    /// The length of a colourbar is not a fraction of what it lies along: a finite number
    /// greater than 0 and at most 1. The length of an inside colourbar, a fraction of the
    /// edge it lies along, is reported against the axes; the stated length of a panel
    /// colourbar, a fraction of the span of the plot boxes beside it, against the panel.
    InvalidColorbarLength,
    /// Data plotted along a logarithmic axis contains finite values less than or
    /// equal to zero, which are not drawn. It is also reported for a base of bars that is
    /// not positive on a logarithmic value axis, in which case the bars are drawn from the
    /// low edge of the plot area instead; for a base of stairs that is not positive on a
    /// logarithmic value axis, in which case the outline drops to the low edge instead; and
    /// for edges of cells that are not positive on a logarithmic axis, including unit
    /// edges, which start at −0.5, in which case the cells they bound are not drawn.
    NonPositiveOnLogAxis,
    /// An artist whose data gives it nothing to draw, so that it is left out of the
    /// drawing: a series or quiver of no points; bars or stairs of no values; an image of
    /// any kind, or cells, with no rows or no columns; or a contour or surface whose field
    /// has no rows or no columns, or a single row or a single column (values, but no cell
    /// between nodes to draw them in).
    /// The message states which case applies and the shape of the array. This is a
    /// warning, not an error, because empty and singleton data are valid: a figure built
    /// before its data arrives, or streamed from nothing, passes through these states. It
    /// is reported only when the artist's references and shapes are otherwise valid, so
    /// that an artist reported for an error is not reported twice. It is also reported
    /// against a panel holding a legend that has no entries to list, because no axes
    /// names it in its legend or no axes that names it has an artist with a label.
    NothingToDraw,
    /// A figure parameter has an empty name, or is a number that is not finite (which
    /// JSON cannot represent).
    InvalidParameter,
    /// A figure tag is empty, or occurs more than once.
    ///
    /// An empty tag describes nothing and cannot be chosen in the viewer, and a tag
    /// repeated is a tag that says no more than it did the first time; both are mistakes
    /// in the code that built the figure rather than choices it could have meant, so both
    /// are errors.
    InvalidTag,
    /// The placement of an image cannot be drawn: a pixel centre or the offset of its
    /// plane is not finite, the centres of the first and last pixels along an axis
    /// coincide although the image has more than one pixel along that axis, or, in a
    /// three-dimensional axes, the plane is offset to a coordinate that is not positive
    /// along a logarithmic third axis, which has no place for it.
    InvalidImagePlacement,
    /// A number that an artist maps through its colour scale falls in a category of the
    /// scale whose policy is strict: below or above the range of a continuous or discrete
    /// scale, outside the entries of an indexed or categorical scale (a fractional index
    /// included), or not finite. It is reported against the artist. Only the numbers of
    /// the artist's data are checked (the values of a scalar image, the field of a
    /// contour, the colour data of a surface or series), never numbers derived from them
    /// such as the mean of a surface face's corners or the automatic levels of a contour.
    /// No value lies beyond automatic limits, which are the range of the data, except a
    /// value that is not positive under a logarithmic norm, which lies below.
    ValueOutOfRange,
    /// An image lies in a plane of which an axis is logarithmic. An image is a raster of
    /// evenly spaced pixels, which has no placement on a logarithmic axis, where equal
    /// steps in the data are unequal distances on the page, so the figure is refused
    /// rather than drawn without the image. Values sampled along a logarithmic axis are
    /// drawn as a [`Surface`](crate::Surface) instead, whose every node is placed
    /// through the logarithmic scale; the message says how. Only the axes of the plane
    /// count: the third axis, along which the plane is offset, may be logarithmic, and
    /// the z axis of a two-dimensional axes is ignored.
    ImageOnLogAxis,
    /// The opacity of an artist (of an image, of the faces or edges of a surface, of the
    /// faces of bars, of the fill of stairs, or of cells) is not a finite number from 0
    /// to 1.
    InvalidOpacity,
    /// The gap of bars, or the gap of cells along x or y, is not a finite number from 0
    /// up to, but not including, 1. A gap of 1 or more leaves nothing of the bar or cell
    /// to draw, and a negative gap overlaps its neighbours.
    InvalidGap,
    /// The edges of cells, or the edges or samples of stairs, are not finite and strictly
    /// monotonic (increasing or decreasing), so that some cell or step has no width, lies
    /// over its neighbour, or has no place.
    InvalidEdges,
    /// The fixed width of the slot of bars, or a width given per bar, is not a finite
    /// positive number.
    InvalidBarSlot,
    /// A colour of bars (their face or edge) or of stairs (their line or fill) is
    /// colormapped, but bars and stairs have no colour scale to look the colour up in.
    ColormapNotSupported,
    /// Stairs have a fill other than none but no base, so there is nothing to fill down
    /// to.
    StairsFillWithoutBase,
    /// Bars of one axes that share a group name differ from the first member of the group,
    /// in artist order, in orientation, slot, gap, base or layout. The group is drawn with
    /// the properties of its first member, so this is a warning, not an error. It is
    /// reported against the member that differs, and the message names the first member
    /// and each property that differs.
    InconsistentBarGroup,
    /// Bars, stairs or cells are placed in a three-dimensional axes; they are drawn only
    /// in the plane of a two-dimensional axes.
    TwoDArtistInThreeDAxes,
    /// The tick label angle of an axis that is drawn is not finite. It is reported against
    /// the axes; the z axis of a two-dimensional axes is ignored.
    InvalidTickLabelAngle,
    /// An axis of a three-dimensional axes has a tick label angle other than 0, which a
    /// three-dimensional axes ignores because its tick labels lie along projected edges.
    /// This is a warning, not an error, because the figure is drawn with level labels. It is
    /// reported against the axes.
    TickLabelAngleIgnored,
}

impl Figure {
    /// Checks the figure for structural problems.
    ///
    /// Errors report data references that name no node, references that name a node of
    /// the wrong kind (an artist's data that is not a data array, an artist's colour scale
    /// that is not a colour scale, a legend's panel that is not a panel holding a legend,
    /// and a legend item that is not an artist), colour scale references that name no
    /// node, colour-mapped artists that name no colour scale, data arrays whose values do
    /// not match their shape, arrays of 8-bit values referenced by an artist other than an
    /// image (every other artist requires 64-bit floating-point values, the alpha of a
    /// surface among them), inconsistent array lengths and shapes within an artist (for an
    /// image, pixels that are not an array of shape `[ny, nx, 3]` or `[ny, nx, 4]`, or
    /// values of a scalar image that are not two-dimensional; for a surface, colour data
    /// or alpha whose shape differs from the field's; for a scalar image, alpha whose shape
    /// differs from the values'), an opacity that is not a finite number from 0 to 1, 3D
    /// artists in 2D axes (an image on the xz or yz plane among them), series
    /// in 3D axes with a dimension in axes-fraction coordinates, links to identifiers that
    /// are not axes, duplicate node identifiers (data arrays and colour scales included),
    /// cells of axes and panels outside the tile layout, layout sizes that are too many or
    /// weights that are not finite and positive, axes that lie in fitted rows alone or in
    /// fitted columns alone, legends that name no node as their panel, items of an
    /// explicit list of legend entries that name no node (or, inside a plot area, an
    /// artist of another axes), legends with a fixed number of zero columns, a
    /// non-positive figure size or font size, invalid manual limits of an axis or a colour
    /// scale, ticks of an axis or a colour scale that cannot be placed (a regular step
    /// that is not finite and positive or an origin that is not finite, and explicit tick
    /// values that are not finite or, on a logarithmic axis or scale, not positive),
    /// explicit tick labels that do not name explicit tick values one by one, invalid
    /// contour levels and band edges, a listed colormap of fewer than 2 or more than 1024
    /// colours, an indexed or categorical colour scale with no entries, parameters with an
    /// empty name or a non-finite number, tags that are empty or repeated, an image
    /// placement whose pixel centres or plane offset are not finite or whose first and
    /// last centres coincide along an axis of more than one pixel, an image whose plane
    /// has a logarithmic axis (which must be drawn as a surface instead) or, in a 3D
    /// axes, is offset to a coordinate that is not positive along a logarithmic third
    /// axis, and numbers that an artist maps through a colour scale which fall in a
    /// category of the scale whose policy is strict (a value lies below or above a range
    /// with automatic limits only when it is not positive under a logarithmic norm,
    /// because automatic limits are the range of the data). Warnings report finite
    /// non-positive data plotted along logarithmic axes, a series whose released
    /// clip has no effect because it places no dimension in axes-fraction coordinates of
    /// a 2D axes, and an artist whose data gives it nothing to draw, which is left out (a
    /// series or quiver of no points, an image with no rows or no columns, and a
    /// contour or surface whose field has no rows or no columns or a single row or
    /// column, which has no cell between its nodes), a panel holding a legend with no
    /// entries to list, which no axes names or whose axes have no artist with a label, or
    /// whose explicit list has no items, an item of an explicit list that repeats an
    /// earlier item, an item that names an artist without a label, a data array that no
    /// artist refers to, and two nodes of the tile layout, axes or panels, whose cells
    /// share a tile, once per pair against the later node (axes before panels, each in
    /// the order the figure lists them), comparing only cells that lie within the layout;
    /// data that is not plotted along an axis (such as quiver components, colour data, the
    /// pixels of an image, or the values of a dimension in axes-fraction coordinates)
    /// never produces the first of these. An array of 8-bit values that no artist refers
    /// to is not an error; it is reported only as unused. A colour scale that no artist
    /// names is neither an error nor a warning, because a colourbar may show it alone.
    ///
    /// The z limits, z scale and z ticks of a 2D axes are ignored, as they are when
    /// drawing, and so is the offset of an image in the xy plane of a 2D axes. A
    /// series or quiver without z data in a 3D axes is not an error: it is drawn in the
    /// plane z = 0. Shape checks are skipped for an artist that refers to unknown or
    /// invalid arrays, or (other than an image) to 8-bit arrays, so that one problem is
    /// not reported repeatedly; the coincidence of an image's pixel centres is checked
    /// only when its array is valid and of the right shape, because it depends on the
    /// pixels, whereas a non-finite centre or offset is always reported. The numbers an
    /// artist maps are checked against its colour scale only when its data references
    /// are valid and its scale reference names a valid colour scale. An artist is
    /// reported as having nothing to draw only when it is reported for no error, so that
    /// the error, which is what must be fixed, stands alone.
    ///
    /// Bars, stairs and cells are errors in a three-dimensional axes. Their own errors are
    /// a gap outside `[0, 1)`, a slot width that is not finite and positive, edges or
    /// samples that are not finite and strictly monotonic, a colormapped colour on bars or
    /// stairs, and a fill on stairs without a base; their warnings are a base or an edge
    /// that is not positive on a logarithmic axis, and a member of a group of bars that
    /// differs from the first member. The tick label angle of an axis that is drawn must
    /// be finite, and an angle other than 0 in a three-dimensional axes is ignored with a
    /// warning.
    pub fn validate(&self) -> ValidationReport {
        let mut kinds = HashMap::new();
        let mut counts: BTreeMap<NodeId, usize> = BTreeMap::new();
        visit(self, None, &mut |node, _| {
            kinds.entry(node.id()).or_insert(node.kind());
            *counts.entry(node.id()).or_default() += 1;
        });
        let mut arrays = HashMap::new();
        for data in &self.data {
            arrays.entry(data.id).or_insert(&data.array);
        }
        let mut validator = Validator {
            figure: self,
            kinds: &kinds,
            arrays: &arrays,
            automatic_limits: HashMap::new(),
            report: ValidationReport::default(),
        };
        self.check_nodes(&mut validator);
        validator.check_duplicate_ids(&counts);
        validator.check_overlapping_cells();
        validator.check_unused_data();
        validator.report
    }
}

/// The checks of one kind of node.
///
/// The declaration of the kinds of node generates the walk that calls the check of every
/// node of a figure, in tree order, with the node's parent, so a kind without a check
/// does not compile. The checks that span several nodes (duplicate identifiers,
/// overlapping cells and unused data) are made once the walk is done.
pub(crate) trait Check {
    /// The node of which nodes of this kind are children: the figure for a top-level
    /// kind, an axes for an artist, and nothing for the figure itself.
    type Parent;

    /// Reports the problems of the node.
    fn check(&self, validator: &mut Validator<'_>, parent: &Self::Parent);
}

/// How validation sees one kind of artist.
///
/// The declaration of the kinds of node generates [`Artist::usage`], which dispatches to
/// the usage of each kind, so a kind of artist without a usage does not compile. Every
/// artist is then checked by the same code, [`Validator::check_artist`], from its usage.
pub(crate) trait Usage {
    /// Returns how the artist uses its data and the dimensions of its axes.
    fn usage(&self) -> ArtistUsage<'_>;
}

impl Check for Figure {
    type Parent = ();

    fn check(&self, validator: &mut Validator<'_>, _: &()) {
        validator.check_figure();
        validator.check_layout_weights();
    }
}

impl Check for DataArray {
    type Parent = Figure;

    fn check(&self, validator: &mut Validator<'_>, _: &Figure) {
        if !is_consistent(&self.array) {
            validator.error(
                Some(self.id),
                IssueKind::InvalidArray,
                format!(
                    "{} has {} values, which does not match its shape {:?}",
                    self.id,
                    self.array.len(),
                    self.array.shape
                ),
            );
        }
    }
}

impl Check for ColorScale {
    type Parent = Figure;

    fn check(&self, validator: &mut Validator<'_>, _: &Figure) {
        for (kind, message) in color_scale_problems(self) {
            validator.error(Some(self.id), kind, message);
        }
    }
}

impl Check for Axes {
    type Parent = Figure;

    fn check(&self, validator: &mut Validator<'_>, _: &Figure) {
        validator.check_axes(self);
    }
}

impl Check for Panel {
    type Parent = Figure;

    fn check(&self, validator: &mut Validator<'_>, _: &Figure) {
        validator.check_cell(self.id, self.cell);
        match &self.content {
            PanelContent::Legend(legend) => {
                validator.check_legend_entry_layout(self.id, legend.layout);
                validator.check_legend_items(self.id, None, &legend.entries);
                validator.check_panel_has_entries(self.id, &legend.entries);
            }
            PanelContent::Colorbar(colorbar) => {
                validator.check_color_scale_reference(self.id, Some(colorbar.scale), None);
                if let Some(length) = colorbar.length.filter(|&l| !is_fraction(l)) {
                    validator.error(
                        Some(self.id),
                        IssueKind::InvalidColorbarLength,
                        format!(
                            "the length {length} of the colourbar is not a fraction of the span                              beside it greater than 0 and at most 1"
                        ),
                    );
                }
            }
        }
    }
}

/// Returns whether a length is a fraction of what a colourbar lies along: a finite number
/// greater than 0 and at most 1.
fn is_fraction(length: f64) -> bool {
    length.is_finite() && length > 0.0 && length <= 1.0
}

impl Check for Artist {
    type Parent = Axes;

    fn check(&self, validator: &mut Validator<'_>, axes: &Axes) {
        let usage = ArtistUsage {
            color_scale: self.color_scale(),
            color: self.color_mapping(),
            ..self.usage()
        };
        validator.check_artist(axes, self.id(), usage);
    }
}

/// Accumulates the issues found in a figure.
pub(crate) struct Validator<'a> {
    figure: &'a Figure,
    /// The kind of every node, by identifier; a shared identifier takes the kind of the
    /// first node in tree order, as a lookup by identifier finds it.
    kinds: &'a HashMap<NodeId, NodeKind>,
    /// The array of every data array, by identifier; a shared identifier takes the first.
    arrays: &'a HashMap<NodeId, &'a NdArray>,
    /// The automatic limits of every colour scale whose limits have been needed, by
    /// identifier, computed once for every artist that names the scale.
    automatic_limits: HashMap<NodeId, Option<(f64, f64)>>,
    report: ValidationReport,
}

/// Returns whether two cells, each spanning at least one row and one column, share a tile.
fn share_a_tile(a: Cell, b: Cell) -> bool {
    let overlap = |start_a: u32, span_a: u32, start_b: u32, span_b: u32| {
        u64::from(start_a) < u64::from(start_b) + u64::from(span_b)
            && u64::from(start_b) < u64::from(start_a) + u64::from(span_a)
    };
    overlap(a.row, a.row_span, b.row, b.row_span) && overlap(a.col, a.col_span, b.col, b.col_span)
}

/// Returns whether an array's number of values equals the product of its shape.
fn is_consistent(array: &NdArray) -> bool {
    array
        .shape
        .iter()
        .try_fold(1usize, |product, &len| product.checked_mul(len))
        == Some(array.len())
}

impl<'a> Validator<'a> {
    fn error(&mut self, node: Option<NodeId>, kind: IssueKind, message: String) {
        self.report.errors.push(ValidationIssue {
            node,
            kind,
            message,
        });
    }

    fn warning(&mut self, node: Option<NodeId>, kind: IssueKind, message: String) {
        self.report.warnings.push(ValidationIssue {
            node,
            kind,
            message,
        });
    }

    /// Checks the figure-level properties, the parameters, the tags and the links.
    fn check_figure(&mut self) {
        let figure = self.figure;
        let sizes = [
            ("width", figure.size.width_mm, "mm"),
            ("height", figure.size.height_mm, "mm"),
            ("base font size", figure.font_size_pt, "pt"),
        ];
        for (name, value, unit) in sizes {
            if !(value.is_finite() && value > 0.0) {
                self.error(
                    Some(figure.id),
                    IssueKind::InvalidSize,
                    format!("the figure {name} of {value} {unit} is not a finite positive number"),
                );
            }
        }

        for (name, parameter) in &figure.parameters {
            if name.is_empty() {
                self.error(
                    Some(figure.id),
                    IssueKind::InvalidParameter,
                    "a parameter has an empty name".to_owned(),
                );
            }
            if let Parameter::Number(value) = parameter
                && !value.is_finite()
            {
                self.error(
                    Some(figure.id),
                    IssueKind::InvalidParameter,
                    format!("the parameter {name:?} is the number {value}, which is not finite"),
                );
            }
        }

        // Tags are compared exactly, so tags differing only in case are distinct and
        // neither is a repeat of the other.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for (index, tag) in figure.tags.iter().enumerate() {
            if tag.is_empty() {
                // An empty tag has no text to name it by, so it is named by where it is: a
                // figure carrying twenty tags would otherwise report a fault the reader
                // cannot find.
                self.error(
                    Some(figure.id),
                    IssueKind::InvalidTag,
                    format!("the tag in position {} is empty", index + 1),
                );
            } else if !seen.insert(tag.as_str()) {
                self.error(
                    Some(figure.id),
                    IssueKind::InvalidTag,
                    format!("the tag {tag:?} occurs more than once"),
                );
            }
        }

        for link in &figure.links {
            for &id in &link.axes {
                if figure.axes(id).is_none() {
                    self.error(
                        None,
                        IssueKind::DanglingLink,
                        format!(
                            "a link along {:?} refers to {id}, which is not an axes",
                            link.dimension
                        ),
                    );
                }
            }
        }
    }

    /// Reports every identifier that `counts`, the number of nodes of each identifier,
    /// gives to more than one node.
    fn check_duplicate_ids(&mut self, counts: &BTreeMap<NodeId, usize>) {
        for (&id, &count) in counts {
            if count > 1 {
                self.error(
                    Some(id),
                    IssueKind::DuplicateNodeId,
                    format!("{count} nodes share the identifier of {id}"),
                );
            }
        }
    }

    /// Checks that the tile layout has no more weights than rows and columns, and that
    /// every weight is a finite positive number.
    fn check_layout_weights(&mut self) {
        let figure = self.figure;
        let layout = &figure.layout;
        let lists = [
            ("row", &layout.row_weights, layout.rows),
            ("column", &layout.col_weights, layout.cols),
        ];
        for (name, weights, count) in lists {
            if weights.len() > count as usize {
                self.error(
                    Some(figure.id),
                    IssueKind::InvalidLayoutWeights,
                    format!(
                        "the tile layout has {} {name} sizes but only {count} {name}s",
                        weights.len()
                    ),
                );
            }
            for (index, size) in weights.iter().enumerate() {
                let TrackSize::Weight(weight) = *size else {
                    continue;
                };
                if !(weight.is_finite() && weight > 0.0) {
                    self.error(
                        Some(figure.id),
                        IssueKind::InvalidLayoutWeights,
                        format!(
                            "the weight {weight} of {name} {index} is not a finite positive number"
                        ),
                    );
                }
            }
        }
    }

    /// Returns whether a cell lies within the tile layout and spans at least one row and one
    /// column.
    fn in_layout(&self, cell: Cell) -> bool {
        let layout = &self.figure.layout;
        cell.row_span > 0
            && cell.col_span > 0
            && u64::from(cell.row) + u64::from(cell.row_span) <= u64::from(layout.rows)
            && u64::from(cell.col) + u64::from(cell.col_span) <= u64::from(layout.cols)
    }

    /// Checks that a cell of an axes or a panel lies within the tile layout.
    fn check_cell(&mut self, node: NodeId, cell: Cell) {
        if !self.in_layout(cell) {
            let layout = &self.figure.layout;
            let message = format!(
                "the cell {cell:?} does not lie within the {} × {} tile layout",
                layout.rows, layout.cols
            );
            self.error(Some(node), IssueKind::CellOutOfLayout, message);
        }
    }

    /// Warns of every pair of nodes in the tile layout whose cells share a tile, against the
    /// later node of the pair, taking the axes first and then the panels. Cells
    /// outside the layout are reported by [`Self::check_cell`] and are not compared.
    fn check_overlapping_cells(&mut self) {
        let figure = self.figure;
        let placed: Vec<(NodeId, Cell)> = figure
            .axes
            .iter()
            .map(|axes| (axes.id, axes.cell))
            .chain(figure.panels.iter().map(|panel| (panel.id, panel.cell)))
            .filter(|&(_, cell)| self.in_layout(cell))
            .collect();
        for (j, &(later, b)) in placed.iter().enumerate() {
            for &(earlier, a) in &placed[..j] {
                if share_a_tile(a, b) {
                    self.warning(
                        Some(later),
                        IssueKind::OverlappingCells,
                        format!(
                            "the cell {b:?} of {later} shares a tile with the cell {a:?} of \
                             {earlier}, so {later} is drawn over {earlier}"
                        ),
                    );
                }
            }
        }
    }

    /// Checks that the layout of the entries of a legend has at least one column.
    fn check_legend_entry_layout(&mut self, node: NodeId, layout: LegendEntryLayout) {
        if layout.columns == LegendEntryColumns::Count(0) {
            self.error(
                Some(node),
                IssueKind::InvalidLegendColumns,
                "the legend has a fixed number of columns of zero".to_owned(),
            );
        }
    }

    /// Reports an axes whose cell lies in fitted rows alone or in fitted columns alone. A
    /// cell outside the layout is reported by [`Self::check_cell`] and is not checked.
    fn check_fitted_tracks(&mut self, axes: &Axes) {
        if !self.in_layout(axes.cell) {
            return;
        }
        let layout = &self.figure.layout;
        let fitted = |sizes: &[TrackSize], first: u32, span: u32| {
            (first..first + span).all(|k| sizes.get(k as usize) == Some(&TrackSize::Fit))
        };
        let cell = axes.cell;
        for (fitted, tracks, extent) in [
            (
                fitted(&layout.row_weights, cell.row, cell.row_span),
                "rows",
                "height",
            ),
            (
                fitted(&layout.col_weights, cell.col, cell.col_span),
                "columns",
                "width",
            ),
        ] {
            if fitted {
                self.error(
                    Some(axes.id),
                    IssueKind::AxesInFittedTrack,
                    format!(
                        "the axes lies in fitted {tracks} alone, which take their size from \
                         panels, so the axes would have no {extent}"
                    ),
                );
            }
        }
    }

    /// Checks the items of an explicit list of legend entries: that each artist they name
    /// is an artist of the figure (of the axes `own`, for a legend inside its plot area)
    /// with a label, that each colour scale they name is a colour scale of the figure,
    /// and that no item is listed twice.
    fn check_legend_items(&mut self, legend: NodeId, own: Option<&Axes>, entries: &LegendEntries) {
        let LegendEntries::Explicit(items) = entries else {
            return;
        };
        let figure = self.figure;
        for (index, item) in items.iter().enumerate() {
            // Two items of one scale show it twice, whatever rows each asks for.
            let same = |earlier: &crate::LegendItem| match (&earlier.content, &item.content) {
                (
                    LegendItemContent::ColorScale { scale: a, .. },
                    LegendItemContent::ColorScale { scale: b, .. },
                ) => a == b,
                (a, b) => a == b,
            };
            if items[..index].iter().any(same) {
                self.warning(
                    Some(legend),
                    IssueKind::DuplicateLegendItem,
                    format!(
                        "legend item {index} repeats an earlier item, so the legend lists it twice"
                    ),
                );
            }
            let artist = match &item.content {
                LegendItemContent::Artist(artist) => artist,
                LegendItemContent::ColorScale { scale, rows } => {
                    // A scale is a node of the figure, so any legend may list any scale;
                    // a scale draws its own labels, so its item is never unnamed.
                    self.check_color_scale_reference(legend, Some(*scale), None);
                    if *rows == 0 {
                        self.error(
                            Some(legend),
                            IssueKind::InvalidLegendRows,
                            format!(
                                "legend item {index} asks for a bar of no rows, which has no                                  length to show its scale along"
                            ),
                        );
                    }
                    continue;
                }
                LegendItemContent::Custom { .. } => continue,
            };
            let found = match own {
                Some(axes) => axes.artists.iter().find(|a| a.id() == *artist),
                None => figure.artist(*artist).map(|(_, a)| a),
            };
            let not_an_artist = self.kinds.contains_key(artist) && figure.artist(*artist).is_none();
            match found {
                None if not_an_artist => self.error(
                    Some(legend),
                    IssueKind::WrongNodeKind,
                    format!("legend item {index} names {artist}, which is not an artist"),
                ),
                None => {
                    let whose = if own.is_some() {
                        "an artist of the axes"
                    } else {
                        "an artist of the figure"
                    };
                    self.error(
                        Some(legend),
                        IssueKind::DanglingLegendItem,
                        format!("legend item {index} names {artist}, which is not {whose}"),
                    );
                }
                Some(found) if found.label().is_none() => self.warning(
                    Some(legend),
                    IssueKind::UnnamedLegendItem,
                    format!(
                        "legend item {index} names {artist}, which has no label, so the item \
                         has no label"
                    ),
                ),
                Some(_) => {}
            }
        }
    }

    /// Checks the cell, legend, limits and ticks of an axes.
    fn check_axes(&mut self, axes: &Axes) {
        self.check_cell(axes.id, axes.cell);
        self.check_fitted_tracks(axes);

        match &axes.legend {
            Some(Legend::Inside(inside)) => {
                self.check_legend_entry_layout(axes.id, inside.layout);
                self.check_legend_items(axes.id, Some(axes), &inside.entries);
            }
            Some(Legend::Panel(panel)) => match self.figure.panel(*panel) {
                Some(Panel {
                    content: PanelContent::Legend(_),
                    ..
                }) => {}
                Some(_) => self.error(
                    Some(axes.id),
                    IssueKind::WrongNodeKind,
                    format!(
                        "the legend lists its entries in {panel}, which is a panel holding no \
                         legend"
                    ),
                ),
                None if self.kinds.contains_key(panel) => self.error(
                    Some(axes.id),
                    IssueKind::WrongNodeKind,
                    format!(
                        "the legend lists its entries in {panel}, which is not a panel holding a \
                         legend"
                    ),
                ),
                None => self.error(
                    Some(axes.id),
                    IssueKind::DanglingPanelLegend,
                    format!("the legend lists its entries in {panel}, which is not in the figure"),
                ),
            },
            None => {}
        }

        if let Some(colorbar) = axes.colorbar {
            self.check_color_scale_reference(axes.id, Some(colorbar.scale), None);
            if !is_fraction(colorbar.length) {
                self.error(
                    Some(axes.id),
                    IssueKind::InvalidColorbarLength,
                    format!(
                        "the length {} of the inside colourbar is not a fraction of its edge \
                         greater than 0 and at most 1",
                        colorbar.length
                    ),
                );
            }
        }

        let three_d = matches!(axes.projection, Projection::ThreeD { .. });
        if let Aspect::Ratio { y, z } = axes.aspect {
            for (name, ratio) in [("y", y), ("z", z)] {
                if !(ratio.is_finite() && ratio > 0.0) {
                    self.error(
                        Some(axes.id),
                        IssueKind::InvalidAspect,
                        format!(
                            "the aspect ratio of {name} {ratio} is not a finite positive number"
                        ),
                    );
                }
            }
        }
        let mut limits = vec![
            ("x limits", axes.x.limits, axes.x.scale),
            ("y limits", axes.y.limits, axes.y.scale),
        ];
        if three_d {
            limits.push(("z limits", axes.z.limits, axes.z.scale));
        }
        let mut axes_drawn = vec![("x", &axes.x), ("y", &axes.y)];
        if three_d {
            axes_drawn.push(("z", &axes.z));
        }
        for &(name, axis) in &axes_drawn {
            let angle = axis.tick_label_angle;
            if !angle.is_finite() {
                self.error(
                    Some(axes.id),
                    IssueKind::InvalidTickLabelAngle,
                    format!("the tick label angle {angle} of the {name} axis is not finite"),
                );
            } else if three_d && angle != 0.0 {
                self.warning(
                    Some(axes.id),
                    IssueKind::TickLabelAngleIgnored,
                    format!(
                        "the tick label angle {angle} of the {name} axis is ignored, because a \
                         three-dimensional axes draws its tick labels along projected edges"
                    ),
                );
            }
        }
        self.check_bar_groups(axes);
        for (name, axis) in axes_drawn {
            let problems = tick_problems(
                &format!("{name} axis"),
                &axis.ticks,
                &axis.tick_labels,
                axis.scale == Scale::Log,
            );
            for (kind, message) in problems {
                self.error(Some(axes.id), kind, message);
            }
        }
        for (name, limits, scale) in limits {
            if let Some(message) = limits_problem(name, limits, scale == Scale::Log) {
                self.error(Some(axes.id), IssueKind::InvalidLimits, message);
            }
        }
    }

    /// Checks one artist of an axes, as its usage describes it.
    fn check_artist(&mut self, axes: &Axes, id: NodeId, usage: ArtistUsage<'_>) {
        let three_d = matches!(axes.projection, Projection::ThreeD { .. });

        if !three_d && usage.three_d {
            self.error(
                Some(id),
                IssueKind::ThreeDArtistInTwoDAxes,
                "a three-dimensional artist is placed in a two-dimensional axes".to_owned(),
            );
        }

        if three_d && usage.fraction.iter().any(|&fraction| fraction) {
            self.error(
                Some(id),
                IssueKind::AxesFractionInThreeDAxes,
                "a dimension in axes-fraction coordinates is placed in a three-dimensional axes, \
                 which accepts data coordinates only"
                    .to_owned(),
            );
        }

        if usage.clip_released && (three_d || !usage.fraction.iter().any(|&fraction| fraction)) {
            self.warning(
                Some(id),
                IssueKind::IneffectiveClip,
                "the clip is released, but clipping is released only along a dimension in \
                 axes-fraction coordinates of a two-dimensional axes, so the artist is still \
                 clipped to the plot area"
                    .to_owned(),
            );
        }

        if let Some(levels) = usage.levels {
            self.check_levels(id, levels);
        }
        // An artist reported for an error of its own (a missing colour scale or an invalid
        // opacity among them) is not also reported as having nothing to draw, so that the
        // error stands alone.
        let errors_before = self.report.errors.len();
        let scale = self.check_color_scale_reference(id, usage.color_scale, usage.color);
        self.check_opacities(id, &usage.opacities);
        if three_d && usage.two_d_only {
            self.error(
                Some(id),
                IssueKind::TwoDArtistInThreeDAxes,
                "bars, stairs and cells are drawn only in a two-dimensional axes, but this \
                 artist is placed in a three-dimensional axes"
                    .to_owned(),
            );
        }
        for (kind, message) in &usage.problems {
            self.error(Some(id), *kind, message.clone());
        }

        let references_are_valid = self.check_references(id, &usage.references, &usage.bytes);
        if references_are_valid && let (Some(scale), Some(mapping)) = (scale, usage.color) {
            self.check_mapped_values(id, scale, mapping.values);
        }
        match &usage.shape {
            Shape::Image(image) => self.check_image(
                axes,
                three_d,
                id,
                image,
                references_are_valid,
                errors_before,
            ),
            shape
                if references_are_valid
                    && self.check_shapes(id, shape)
                    && self.report.errors.len() == errors_before =>
            {
                self.check_something_to_draw(id, shape);
            }
            _ => {}
        }
        if !three_d && references_are_valid {
            self.check_log_extents(axes, id, &usage.shape);
        }

        for &(dimension, data) in &usage.positions {
            if dimension == Dimension::Z && !three_d {
                continue;
            }
            // A fraction of the plot area is not placed along the axis, so its scale does not apply to it.
            if usage.is_fraction(dimension) {
                continue;
            }
            let axis = axis_of(axes, dimension);
            if axis.scale != Scale::Log {
                continue;
            }
            let Some(values) = self.arrays.get(&data).and_then(|array| array.as_f64()) else {
                continue;
            };
            if values.iter().any(|&v| v.is_finite() && v <= 0.0) {
                self.warning(
                    Some(id),
                    IssueKind::NonPositiveOnLogAxis,
                    format!(
                        "{data} contains values less than or equal to zero along the \
                         logarithmic {dimension:?} axis, which are not drawn"
                    ),
                );
            }
        }
    }

    /// Reports each opacity of an artist that is not a finite number from 0 to 1.
    fn check_opacities(&mut self, node: NodeId, opacities: &[(&str, f64)]) {
        for &(what, opacity) in opacities {
            if !(0.0..=1.0).contains(&opacity) {
                self.error(
                    Some(node),
                    IssueKind::InvalidOpacity,
                    format!("the {what} is {opacity}, but it must be a number from 0 to 1"),
                );
            }
        }
    }

    /// Reports each distinct data reference of an artist that names no node or a node
    /// that is not a data array and, unless it is among the references in `bytes` that
    /// accept 8-bit values, each distinct reference to an array of them, and returns
    /// whether every reference is to a data array of an element type the artist can use
    /// whose values match its shape.
    fn check_references(&mut self, node: NodeId, references: &[NodeId], bytes: &[NodeId]) -> bool {
        let mut valid = true;
        let mut reported: Vec<NodeId> = Vec::new();
        for &data in references {
            let problem = match self.arrays.get(&data) {
                Some(array) if !bytes.contains(&data) && array.element() != NdArrayElement::F64 => {
                    Some((
                        IssueKind::ElementTypeMismatch,
                        format!(
                            "{data} holds {} values, but the artist requires f64 values",
                            array.element()
                        ),
                    ))
                }
                Some(array) => {
                    valid &= is_consistent(array);
                    None
                }
                None => Some(match self.kinds.get(&data) {
                    Some(kind) => (
                        IssueKind::WrongNodeKind,
                        format!("{data} is a node of kind {kind:?}, not a data array"),
                    ),
                    None => (
                        IssueKind::UnknownData,
                        format!("{data} is not a node of the figure"),
                    ),
                }),
            };
            if let Some((kind, message)) = problem {
                valid = false;
                if !reported.contains(&data) {
                    reported.push(data);
                    self.error(Some(node), kind, message);
                }
            }
        }
        valid
    }

    /// Checks the lengths and shapes of the arrays of an artist other than an image, all
    /// of which exist, and returns whether they agree.
    fn check_shapes(&mut self, node: NodeId, shape: &Shape<'_>) -> bool {
        let data = self.arrays;
        match shape {
            Shape::Points {
                what,
                arrays,
                x,
                colors,
                ..
            } => {
                self.check_equal_counts(node, what, arrays)
                    && colors.is_none_or(|colors| self.check_series_colors(node, *x, colors))
            }
            Shape::Grid {
                grid,
                field,
                like_field,
            } => {
                let grid_agrees = self.check_grid(node, *grid, *field);
                let mut agree = true;
                for &(what, array) in like_field {
                    if let Some(array) = array
                        && data[&array].shape != data[field].shape
                    {
                        self.error(
                            Some(node),
                            IssueKind::ShapeMismatch,
                            format!(
                                "{what} has shape {:?}, but the field has shape {:?}",
                                data[&array].shape, data[field].shape
                            ),
                        );
                        agree = false;
                    }
                }
                grid_agrees && agree
            }
            Shape::Bars(bar) => self.check_bar_shapes(node, bar),
            Shape::Stairs(stairs) => self.check_stairs_shapes(node, stairs),
            Shape::Cells(cells) => self.check_cells_shapes(node, cells),
            // The shape of an image is checked by `check_image`, which also needs the
            // number of pixels along each axis of the image.
            Shape::Image(_) => true,
        }
    }

    /// Checks that the arrays of bars are one-dimensional with one entry per bar, as the
    /// per-bar colours are when there are any, and that every width given per bar is a
    /// finite positive number, and returns whether all of them hold.
    fn check_bar_shapes(&mut self, node: NodeId, bar: &Bar) -> bool {
        let data = self.arrays;
        let mut arrays = vec![("positions", bar.positions), ("values", bar.values)];
        if let BarBase::Data(base) = bar.base {
            arrays.push(("bases", base));
        }
        if let BarSlot::Data(widths) = bar.slot {
            arrays.push(("widths", widths));
        }
        if !self.check_vectors(node, &arrays) {
            return false;
        }
        let n = data[&bar.positions].len();
        let mut agree = true;
        for &(what, array) in &arrays[1..] {
            let len = data[&array].len();
            if len != n {
                self.error(
                    Some(node),
                    IssueKind::ShapeMismatch,
                    format!("the {what} hold {len} values, but the positions hold {n}"),
                );
                agree = false;
            }
        }
        if !bar.colors.is_empty() && bar.colors.len() != n {
            self.error(
                Some(node),
                IssueKind::ShapeMismatch,
                format!(
                    "the bars have {} colours, but there are {n} bars, each of which needs one",
                    bar.colors.len()
                ),
            );
            agree = false;
        }
        if !agree {
            return false;
        }
        if let BarSlot::Data(widths) = bar.slot
            && let Some(values) = data[&widths].as_f64()
            && let Some(bad) = values.iter().find(|&&w| !(w.is_finite() && w > 0.0))
        {
            self.error(
                Some(node),
                IssueKind::InvalidBarSlot,
                format!("the width {bad} in {widths} is not a finite positive number"),
            );
            return false;
        }
        true
    }

    /// Checks that the arrays of stairs are one-dimensional, with one more edge than values
    /// or one sample per value, and that the edges or samples are finite and strictly
    /// monotonic, and returns whether all of them hold.
    fn check_stairs_shapes(&mut self, node: NodeId, stairs: &Stairs) -> bool {
        let data = self.arrays;
        let (what, positions, extra) = match stairs.data {
            StairsData::Edges { edges, .. } => ("edges", edges, 1),
            StairsData::Samples { x, .. } => ("samples", x, 0),
        };
        let values = stairs.data.values();
        if !self.check_vectors(node, &[(what, positions), ("values", values)]) {
            return false;
        }
        let (count, n) = (data[&positions].len(), data[&values].len());
        if count != n + extra {
            let needed = if extra == 1 {
                "one more edge than values"
            } else {
                "one sample per value"
            };
            self.error(
                Some(node),
                IssueKind::ShapeMismatch,
                format!("the stairs have {count} {what} for {n} values, but they need {needed}"),
            );
            return false;
        }
        self.check_monotonic(node, &format!("the {what} of the stairs"), positions)
    }

    /// Checks that the values of cells are two-dimensional, that explicit edges are
    /// one-dimensional with one more edge than cells and are finite and strictly
    /// monotonic, and that the alpha has the shape of the values, and returns whether all
    /// of them hold.
    fn check_cells_shapes(&mut self, node: NodeId, cells: &Cells) -> bool {
        let data = self.arrays;
        let shape = &data[&cells.values].shape;
        let &[ny, nx] = shape.as_slice() else {
            self.error(
                Some(node),
                IssueKind::ShapeMismatch,
                format!("the values have shape {shape:?}, but they must be two-dimensional"),
            );
            return false;
        };
        let mut agree = true;
        if let Some(alpha) = cells.alpha
            && data[&alpha].shape != *shape
        {
            self.error(
                Some(node),
                IssueKind::ShapeMismatch,
                format!(
                    "the alpha has shape {:?}, but the values have shape {shape:?}",
                    data[&alpha].shape
                ),
            );
            agree = false;
        }
        for (what, edges, count) in [("column", cells.columns, nx), ("row", cells.rows, ny)] {
            let CellEdges::Edges(edges) = edges else {
                continue;
            };
            let name = format!("{what} edges");
            if !self.check_vectors(node, &[(name.as_str(), edges)]) {
                agree = false;
                continue;
            }
            let found = data[&edges].len();
            if found != count + 1 {
                self.error(
                    Some(node),
                    IssueKind::ShapeMismatch,
                    format!(
                        "there are {found} {what} edges, but the values of shape {shape:?} \
                         need {}, one more than the {what}s",
                        count + 1
                    ),
                );
                agree = false;
                continue;
            }
            agree &= self.check_monotonic(node, &format!("the {what} edges of the cells"), edges);
        }
        agree
    }

    /// Reports each named array that is not one-dimensional, and returns whether all of
    /// them are.
    fn check_vectors(&mut self, node: NodeId, arrays: &[(&str, NodeId)]) -> bool {
        let mut all = true;
        for &(what, array) in arrays {
            let shape = &self.arrays[&array].shape;
            if shape.len() != 1 {
                self.error(
                    Some(node),
                    IssueKind::ShapeMismatch,
                    format!("the {what} have shape {shape:?}, but they must be one-dimensional"),
                );
                all = false;
            }
        }
        all
    }

    /// Reports edges or samples, described by `what`, that are not finite and strictly
    /// monotonic, and returns whether they are.
    fn check_monotonic(&mut self, node: NodeId, what: &str, array: NodeId) -> bool {
        let Some(values) = self.arrays[&array].as_f64() else {
            return true;
        };
        let problem = if let Some(bad) = values.iter().find(|v| !v.is_finite()) {
            format!("{what} include {bad}, which is not finite")
        } else if !(values.windows(2).all(|pair| pair[0] < pair[1])
            || values.windows(2).all(|pair| pair[0] > pair[1]))
        {
            format!("{what} are not strictly increasing or strictly decreasing")
        } else {
            return true;
        };
        self.error(Some(node), IssueKind::InvalidEdges, problem);
        false
    }

    /// Warns of the bases of bars and stairs that are not positive on a logarithmic value
    /// axis, and of the edges of cells that are not positive on a logarithmic axis; the
    /// references of the artist are valid.
    fn check_log_extents(&mut self, axes: &Axes, node: NodeId, shape: &Shape<'_>) {
        match shape {
            Shape::Bars(bar) => {
                let dimension = bar.orientation.value_dimension();
                if axis_of(axes, dimension).scale != Scale::Log {
                    return;
                }
                let not_positive = match bar.base {
                    BarBase::Constant(base) => base <= 0.0,
                    BarBase::Data(base) => self.arrays[&base]
                        .as_f64()
                        .is_some_and(|values| values.iter().any(|&v| v <= 0.0)),
                };
                if not_positive {
                    self.warning(
                        Some(node),
                        IssueKind::NonPositiveOnLogAxis,
                        format!(
                            "the base of the bars is not positive along the logarithmic {} \
                             axis, which has no place for it, so the bars are drawn from the \
                             low edge of the plot area",
                            dimension_name(dimension)
                        ),
                    );
                }
            }
            Shape::Stairs(stairs) => {
                let dimension = stairs.orientation.value_dimension();
                if let Some(base) = stairs.base
                    && base <= 0.0
                    && axis_of(axes, dimension).scale == Scale::Log
                {
                    self.warning(
                        Some(node),
                        IssueKind::NonPositiveOnLogAxis,
                        format!(
                            "the base {base} of the stairs is not positive along the \
                             logarithmic {} axis, which has no place for it, so the outline \
                             drops to the low edge of the plot area",
                            dimension_name(dimension)
                        ),
                    );
                }
            }
            Shape::Cells(cells) => {
                for (dimension, what, edges) in [
                    (Dimension::X, "column", cells.columns),
                    (Dimension::Y, "row", cells.rows),
                ] {
                    if axis_of(axes, dimension).scale != Scale::Log {
                        continue;
                    }
                    let message = match edges {
                        CellEdges::Unit => format!(
                            "the unit {what} edges start at −0.5, which is not positive along \
                             the logarithmic {} axis, so the first {what} is not drawn",
                            dimension_name(dimension)
                        ),
                        CellEdges::Edges(edges)
                            if self.arrays[&edges]
                                .as_f64()
                                .is_some_and(|values| values.iter().any(|&v| v <= 0.0)) =>
                        {
                            format!(
                                "the {what} edges in {edges} include values less than or equal \
                                 to zero along the logarithmic {} axis, so the {what}s they \
                                 bound are not drawn",
                                dimension_name(dimension)
                            )
                        }
                        CellEdges::Edges(_) => continue,
                    };
                    self.warning(Some(node), IssueKind::NonPositiveOnLogAxis, message);
                }
            }
            Shape::Points { .. } | Shape::Grid { .. } | Shape::Image(_) => {}
        }
    }

    /// Warns of every bar series whose group differs from the first member of the group of
    /// the same name in the axes, in artist order, naming the first member and each
    /// property that differs.
    fn check_bar_groups(&mut self, axes: &Axes) {
        let mut first: BTreeMap<&str, &Bar> = BTreeMap::new();
        for artist in &axes.artists {
            let Artist::Bar(bar) = artist else {
                continue;
            };
            let Some(group) = &bar.group else {
                continue;
            };
            let Some(leader) = first.get(group.name.as_str()).copied() else {
                first.insert(&group.name, bar);
                continue;
            };
            let layout = leader.group.as_ref().map(|g| g.layout);
            let differ: Vec<&str> = [
                ("orientation", bar.orientation != leader.orientation),
                ("slot", bar.slot != leader.slot),
                ("gap", bar.gap.to_bits() != leader.gap.to_bits()),
                ("base", bar.base != leader.base),
                ("layout", Some(group.layout) != layout),
            ]
            .into_iter()
            .filter_map(|(name, differs)| differs.then_some(name))
            .collect();
            if differ.is_empty() {
                continue;
            }
            let properties = differ.join(" and ");
            self.warning(
                Some(bar.id),
                IssueKind::InconsistentBarGroup,
                format!(
                    "the bars of {} differ in their {properties} from {}, the first member of \
                     the group {:?}, so they are drawn with the {properties} of {}",
                    bar.id, leader.id, group.name, leader.id
                ),
            );
        }
    }

    /// Warns of a panel holding a legend that has no entries to list, because no axes
    /// names it or no axes that names it has an artist with a label.
    fn check_panel_has_entries(&mut self, panel: NodeId, entries: &LegendEntries) {
        if let LegendEntries::Explicit(items) = entries {
            if items.is_empty() {
                self.warning(
                    Some(panel),
                    IssueKind::NothingToDraw,
                    "the legend of the panel has an explicit list of no items, so the panel is empty"
                        .to_owned(),
                );
            }
            return;
        }
        let mut axes = self
            .figure
            .axes
            .iter()
            .filter(|axes| axes.legend == Some(Legend::Panel(panel)))
            .peekable();
        let message = if axes.peek().is_none() {
            "no axes lists its entries in the panel, so the panel is empty"
        } else if axes.any(|axes| axes.artists.iter().any(|artist| artist.label().is_some())) {
            return;
        } else {
            "no artist of the axes that list their entries in the panel has a label, so the \
             panel is empty"
        };
        self.warning(Some(panel), IssueKind::NothingToDraw, message.to_string());
    }

    /// Warns of an artist other than an image whose arrays, which exist and agree in
    /// shape, give it nothing to draw: a series or quiver of no points, or a
    /// contour or surface whose field has no rows or no columns or a single row or a
    /// single column, between whose nodes there is no cell.
    fn check_something_to_draw(&mut self, node: NodeId, shape: &Shape<'_>) {
        let data = self.arrays;
        let message = match shape {
            Shape::Grid { field, .. } => {
                let shape = &data[field].shape;
                let &[ny, nx] = shape.as_slice() else {
                    return;
                };
                let Some(case) = nodes_without_cells(ny, nx) else {
                    return;
                };
                format!(
                    "the field has shape {shape:?}: it {case}, and a contour or surface draws \
                     the cells between the nodes of its grid, of which there is none, so the \
                     artist is left out of the drawing"
                )
            }
            Shape::Points { points, x, .. } => {
                let shape = &data[x].shape;
                if shape.iter().product::<usize>() > 0 {
                    return;
                }
                format!(
                    "{points} have shape {shape:?}, so the artist has no points and is left out \
                     of the drawing"
                )
            }
            Shape::Bars(bar) => {
                let shape = &data[&bar.values].shape;
                if shape.iter().product::<usize>() > 0 {
                    return;
                }
                format!(
                    "the values have shape {shape:?}, so the series has no bars and is left out \
                     of the drawing"
                )
            }
            Shape::Stairs(stairs) => {
                let shape = &data[&stairs.data.values()].shape;
                if shape.iter().product::<usize>() > 0 {
                    return;
                }
                format!(
                    "the values have shape {shape:?}, so the stairs have no steps and are left \
                     out of the drawing"
                )
            }
            Shape::Cells(cells) => {
                let shape = &data[&cells.values].shape;
                let case = match shape.as_slice() {
                    [0, 0] => "no rows and no columns",
                    [0, _] => "no rows",
                    [_, 0] => "no columns",
                    _ => return,
                };
                format!(
                    "the values have shape {shape:?}, so the cells have {case} and are left out \
                     of the drawing"
                )
            }
            Shape::Image(_) => return,
        };
        self.warning(Some(node), IssueKind::NothingToDraw, message);
    }

    /// Checks an image: the shape of its array when the array is valid, its placement,
    /// the scales of the axes of its plane, and, when no error was reported against it
    /// since `errors_before` errors were reported, whether it has any pixel to draw.
    fn check_image(
        &mut self,
        axes: &Axes,
        three_d: bool,
        node: NodeId,
        image: &ImageView,
        data_is_valid: bool,
        errors_before: usize,
    ) {
        let pixels = if data_is_valid {
            self.check_image_shape(node, image)
                .filter(|&pixels| self.check_image_alpha_shape(node, image, pixels))
        } else {
            None
        };
        self.check_image_placement(node, image.placement, pixels);
        self.check_image_plane_scales(axes, three_d, node, image);
        let Some([ny, nx]) = pixels else {
            return;
        };
        if self.report.errors.len() > errors_before {
            return;
        }
        let case = match (ny, nx) {
            (0, 0) => "no rows and no columns",
            (0, _) => "no rows",
            (_, 0) => "no columns",
            _ => return,
        };
        let shape = &self.arrays[&image.data].shape;
        self.warning(
            Some(node),
            IssueKind::NothingToDraw,
            format!(
                "the {} have shape {shape:?}, so the image has {case} of pixels and is left \
                 out of the drawing",
                image.what
            ),
        );
    }

    /// Checks that the array of an image has the shape its kind requires, and returns
    /// the number of rows and columns of pixels when it has.
    fn check_image_shape(&mut self, node: NodeId, image: &ImageView) -> Option<[usize; 2]> {
        let shape = &self.arrays[&image.data].shape;
        let message = match (image.channels, shape.as_slice()) {
            (true, &[ny, nx, 3 | 4]) | (false, &[ny, nx]) => return Some([ny, nx]),
            (true, _) => format!(
                "the pixels have shape {shape:?}, but they must be a three-dimensional array \
                 whose last dimension holds the 3 or 4 colour components of a pixel"
            ),
            (false, _) => format!(
                "the {} have shape {shape:?}, but they must be two-dimensional",
                image.what
            ),
        };
        self.error(Some(node), IssueKind::ShapeMismatch, message);
        None
    }

    /// Checks that the alpha of a colour-indexed or colour-mapped image, if it has one,
    /// is an array of `pixels` rows and columns, as its indices or values are, and
    /// returns whether it is.
    fn check_image_alpha_shape(
        &mut self,
        node: NodeId,
        image: &ImageView,
        pixels: [usize; 2],
    ) -> bool {
        let Some(alpha) = image.alpha else {
            return true;
        };
        let shape = &self.arrays[&alpha].shape;
        if shape.as_slice() == pixels {
            return true;
        }
        let message = format!(
            "the alpha has shape {shape:?}, but the {} have shape {pixels:?}",
            image.what
        );
        self.error(Some(node), IssueKind::ShapeMismatch, message);
        false
    }

    /// Checks the placement of an image: its pixel centres and plane offset must be
    /// finite, and the centres of its first and last pixels along an axis may coincide
    /// only when it has one pixel along that axis, which `pixels` gives as the rows and
    /// columns when they are known.
    fn check_image_placement(
        &mut self,
        node: NodeId,
        placement: &ImagePlacement,
        pixels: Option<[usize; 2]>,
    ) {
        if let Some(offset) = placement.plane.offset()
            && !offset.is_finite()
        {
            self.error(
                Some(node),
                IssueKind::InvalidImagePlacement,
                format!("the offset {offset} of the plane of the image is not finite"),
            );
        }
        let counts = pixels.map_or([None, None], |[ny, nx]| [Some(nx), Some(ny)]);
        let ranges = [("columns", placement.columns), ("rows", placement.rows)];
        for ((what, range), count) in ranges.into_iter().zip(counts) {
            let Some(PixelRange { first, last }) = range else {
                continue;
            };
            if !(first.is_finite() && last.is_finite()) {
                self.error(
                    Some(node),
                    IssueKind::InvalidImagePlacement,
                    format!(
                        "the centres of the first and last {what} [{first}, {last}] are not finite"
                    ),
                );
            } else if first == last
                && let Some(count) = count
                && count > 1
            {
                self.error(
                    Some(node),
                    IssueKind::InvalidImagePlacement,
                    format!(
                        "the centres of the first and last {what} coincide at {first}, but the \
                         image has {count} {what}"
                    ),
                );
            }
        }
    }

    /// Reports an image whose plane cannot be placed on the logarithmic axes of the axes;
    /// the z axis of a 2D axes is ignored, as it is when drawing.
    ///
    /// An image whose plane has a logarithmic axis is an [`IssueKind::ImageOnLogAxis`]
    /// error, because a raster of evenly spaced pixels has no placement on such an axis;
    /// its message tells the user how to draw the same values as a surface, whose every
    /// node is placed through the logarithmic scale. Otherwise, a plane offset to a
    /// non-positive coordinate along a logarithmic third axis is an
    /// [`IssueKind::InvalidImagePlacement`] error, because the plane itself could be
    /// drawn but its offset has no place on that axis.
    fn check_image_plane_scales(
        &mut self,
        axes: &Axes,
        three_d: bool,
        node: NodeId,
        image: &ImageView,
    ) {
        let plane = image.placement.plane;
        let [columns, rows] = plane.axes();
        let logarithmic: Vec<&str> = [columns, rows]
            .into_iter()
            .filter(|&dimension| three_d || dimension != Dimension::Z)
            .filter(|&dimension| axis_of(axes, dimension).scale == Scale::Log)
            .map(dimension_name)
            .collect();
        if logarithmic.is_empty() {
            let third = [Dimension::X, Dimension::Y, Dimension::Z]
                .into_iter()
                .find(|&dimension| dimension != columns && dimension != rows)
                .expect("a plane leaves one dimension of three");
            if let Some(offset) = plane.offset()
                && three_d
                && offset <= 0.0
                && axis_of(axes, third).scale == Scale::Log
            {
                self.error(
                    Some(node),
                    IssueKind::InvalidImagePlacement,
                    format!(
                        "the plane of the image ({node}) is offset to {offset} along the {} \
                         axis, which is logarithmic and so has no place for a coordinate that \
                         is not positive; give the plane a positive offset, or no offset to \
                         place it at the lower limit of the axis",
                        dimension_name(third)
                    ),
                );
            }
            return;
        }
        let (noun, verb) = if logarithmic.len() == 1 {
            ("axis", "is")
        } else {
            ("axes", "are")
        };
        let surface = match (three_d, plane) {
            (false, _) => "a rectilinear grid of the pixel-centre coordinates along x and y, and \
                 the values as its field `z`, which is the colour data of its nodes: in a \
                 two-dimensional axes a surface is seen from directly above as a pseudocolour \
                 plot, and every node of its grid is placed through the logarithmic scale"
                .to_owned(),
            (true, ImagePlane::Xy { .. }) => "a rectilinear grid of the pixel-centre coordinates \
                 along x and y, a field `z` that holds the height of the plane at every node, and \
                 the values as its colour data `c`; every node of a surface is placed through \
                 the logarithmic scale"
                .to_owned(),
            (true, ImagePlane::Xz { .. } | ImagePlane::Yz { .. }) => {
                let (along, across) = if matches!(plane, ImagePlane::Xz { .. }) {
                    ("x", "y")
                } else {
                    ("y", "x")
                };
                format!(
                    "a curvilinear grid whose {along} coordinates are the pixel-centre \
                     coordinates along {along} and whose {across} coordinates all equal the \
                     offset of the plane, a field `z` of the pixel-centre coordinates along z, \
                     and the values as its colour data `c`; every node of a surface is placed \
                     through the logarithmic scale"
                )
            }
        };
        let true_colour = if image.channels {
            "; a surface is coloured through the colormap, so the colours of a true-colour \
             image must be given as the values from which they were made"
        } else {
            ""
        };
        self.error(
            Some(node),
            IssueKind::ImageOnLogAxis,
            format!(
                "the image ({node}) lies in the {} plane, whose {} {noun} {verb} logarithmic, \
                 but an image is a raster of evenly spaced pixels, which has no placement on a \
                 logarithmic axis, where equal steps in the data are unequal distances on the \
                 page; draw the values as a surface instead, with {surface}{true_colour}",
                plane_name(plane),
                logarithmic.join(" and ")
            ),
        );
    }

    /// Checks the colour scale reference of an artist: that a reference names a colour
    /// scale of the figure, and that an artist that maps numbers to colours names one.
    /// Returns the scale when the reference names a colour scale that is itself valid,
    /// against which the numbers the artist maps can be checked.
    fn check_color_scale_reference(
        &mut self,
        node: NodeId,
        reference: Option<NodeId>,
        mapping: Option<ColorMapping>,
    ) -> Option<&'a ColorScale> {
        let figure: &'a Figure = self.figure;
        let Some(scale) = reference else {
            if mapping.is_some() {
                self.error(
                    Some(node),
                    IssueKind::MissingColorScale,
                    "the artist maps numbers to colours but names no colour scale, and the \
                     axes hold no colormap to fall back on"
                        .to_owned(),
                );
            }
            return None;
        };
        match figure.color_scale(scale) {
            Some(found) => color_scale_problems(found).is_empty().then_some(found),
            None => {
                let (kind, message) = match self.kinds.get(&scale) {
                    Some(kind) => (
                        IssueKind::WrongNodeKind,
                        format!("{scale} is a node of kind {kind:?}, not a colour scale"),
                    ),
                    None => (
                        IssueKind::UnknownColorScale,
                        format!("the colour scale {scale} is not a node of the figure"),
                    ),
                };
                self.error(Some(node), kind, message);
                None
            }
        }
    }

    /// Reports the numbers of an array that an artist maps through a valid colour scale
    /// and that fall in a category of the scale whose policy is strict, one error for
    /// each such category; the array is a valid data array.
    fn check_mapped_values(&mut self, node: NodeId, scale: &ColorScale, values: NodeId) {
        let domain = match scale.fixed_domain() {
            Some(domain) => domain,
            None => {
                let automatic = *self
                    .automatic_limits
                    .entry(scale.id)
                    .or_insert_with(|| self.figure.automatic_color_limits(scale.id));
                // Without a value to range over, no finite value lies beyond automatic
                // limits, except a value that is not positive under a logarithmic norm,
                // which the domain places below whatever its limits.
                let (min, max) = scale
                    .range(automatic)
                    .unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
                ColorScaleDomain::Continuous {
                    min,
                    max,
                    norm: scale.kind.norm(),
                }
            }
        };
        let Some(array) = self.arrays.get(&values) else {
            return;
        };
        // The number of values in each category: below, above, outside and non-finite.
        let mut counts = [0usize; 4];
        for_each_value(array, |value| match domain.classify(value) {
            ColorScaleClass::Inside(_) => {}
            ColorScaleClass::Below => counts[0] += 1,
            ColorScaleClass::Above => counts[1] += 1,
            ColorScaleClass::Outside => counts[2] += 1,
            ColorScaleClass::NonFinite => counts[3] += 1,
        });
        let classes = [
            (
                ColorScaleClass::Below,
                "below",
                "below the range of the scale",
            ),
            (
                ColorScaleClass::Above,
                "above",
                "above the range of the scale",
            ),
            (
                ColorScaleClass::Outside,
                "outside",
                "not whole numbers that name an entry of the scale",
            ),
            (ColorScaleClass::NonFinite, "non_finite", "not finite"),
        ];
        for ((class, name, condition), count) in classes.into_iter().zip(counts) {
            let strict = scale
                .category(class)
                .is_some_and(|category| category.policy == OutOfRange::Strict);
            if strict && count > 0 {
                self.error(
                    Some(node),
                    IssueKind::ValueOutOfRange,
                    format!(
                        "the {name} policy of the colour scale {} is strict, but {count} of the \
                         values in {values} are {condition}",
                        scale.id
                    ),
                );
            }
        }
    }

    /// Checks the length of the colour data of a series of as many points as `x` has
    /// values, and returns whether it is valid: one value per point, or one value per
    /// segment (one fewer than the points) when the series draws a line and has no
    /// colormapped marker face or edge, which would need a value per point.
    fn check_series_colors(&mut self, node: NodeId, x: NodeId, colors: SeriesColors) -> bool {
        let points = self.arrays[&x].len();
        let values = self.arrays[&colors.data].len();
        let per_segment = values + 1 == points;
        if values == points || (per_segment && colors.draws_line && !colors.mapped_markers) {
            return true;
        }
        let message = if per_segment && !colors.draws_line {
            format!(
                "the colour data holds {values} values, one per segment of {points} points, but \
                 the series draws no line, so it has no segments to colour"
            )
        } else if per_segment {
            format!(
                "the colour data holds {values} values, one per segment of {points} points, but \
                 a colormapped marker face or edge needs one value per point"
            )
        } else {
            format!(
                "the colour data holds {values} values, but a series of {points} points needs \
                 {points} values, one per point, or {} values, one per segment",
                points.saturating_sub(1)
            )
        };
        self.error(Some(node), IssueKind::ShapeMismatch, message);
        false
    }

    /// Reports a shape mismatch when the present arrays differ in element count, and
    /// returns whether they agree.
    fn check_equal_counts(&mut self, node: NodeId, what: &str, arrays: &[Option<NodeId>]) -> bool {
        let counts: Vec<usize> = arrays
            .iter()
            .flatten()
            .map(|id| self.arrays[id].len())
            .collect();
        if counts.windows(2).any(|pair| pair[0] != pair[1]) {
            self.error(
                Some(node),
                IssueKind::ShapeMismatch,
                format!("{what} have different numbers of elements {counts:?}"),
            );
            return false;
        }
        true
    }

    /// Checks that a field is two-dimensional and that its grid matches it, and returns
    /// whether both hold.
    fn check_grid(&mut self, node: NodeId, grid: Grid, field: NodeId) -> bool {
        let data = self.arrays;
        let shape = &data[&field].shape;
        let &[ny, nx] = shape.as_slice() else {
            self.error(
                Some(node),
                IssueKind::ShapeMismatch,
                format!("the field has shape {shape:?}, but it must be two-dimensional"),
            );
            return false;
        };
        let problem = match grid {
            Grid::Rectilinear { x, y } => {
                let (x_len, y_len) = (data[&x].len(), data[&y].len());
                (x_len != nx || y_len != ny).then(|| {
                    format!(
                        "the grid vectors have {x_len} x and {y_len} y values, but the \
                         field of shape {shape:?} needs {nx} x and {ny} y values"
                    )
                })
            }
            Grid::Curvilinear { x, y } => {
                let (x_shape, y_shape) = (&data[&x].shape, &data[&y].shape);
                (x_shape != shape || y_shape != shape).then(|| {
                    format!(
                        "the grid coordinates have shapes {x_shape:?} and {y_shape:?}, but \
                         the field has shape {shape:?}"
                    )
                })
            }
        };
        if let Some(message) = problem {
            self.error(Some(node), IssueKind::ShapeMismatch, message);
            return false;
        }
        true
    }

    /// Warns of every data array that no artist refers to.
    fn check_unused_data(&mut self) {
        let figure = self.figure;
        let used: BTreeSet<NodeId> = figure
            .axes
            .iter()
            .flat_map(|axes| &axes.artists)
            .flat_map(|artist| artist.usage().references)
            .collect();
        for data in &figure.data {
            if !used.contains(&data.id) {
                self.warning(
                    Some(data.id),
                    IssueKind::UnusedData,
                    format!(
                        "no artist refers to {}, which is stored but never drawn",
                        data.id
                    ),
                );
            }
        }
    }

    /// Checks the levels of a contour.
    fn check_levels(&mut self, node: NodeId, levels: &Levels) {
        let problem = match levels {
            Levels::Auto { count: 0 } => Some("the automatic level count is zero"),
            Levels::Auto { .. } => None,
            Levels::Explicit { values } if values.is_empty() => Some("there are no levels"),
            Levels::Explicit { values } if !values.iter().all(|v| v.is_finite()) => {
                Some("a level is not finite")
            }
            Levels::Explicit { values } if values.windows(2).any(|pair| pair[0] >= pair[1]) => {
                Some("the levels are not strictly increasing")
            }
            Levels::Explicit { .. } => None,
        };
        if let Some(message) = problem {
            self.error(Some(node), IssueKind::InvalidLevels, message.to_owned());
        }
    }
}

/// Returns the coordinate axis of an axes along a dimension.
fn axis_of(axes: &Axes, dimension: Dimension) -> &Axis {
    match dimension {
        Dimension::X => &axes.x,
        Dimension::Y => &axes.y,
        Dimension::Z => &axes.z,
    }
}

/// Describes a grid of `ny` rows and `nx` columns of nodes that has no cell between its
/// nodes, as messages write it, or returns `None` for a grid with a cell.
fn nodes_without_cells(ny: usize, nx: usize) -> Option<&'static str> {
    Some(match (ny, nx) {
        (0, 0) => "has no rows and no columns",
        (0, _) => "has no rows",
        (_, 0) => "has no columns",
        (1, 1) => "has a single node",
        (1, _) => "has a single row of nodes",
        (_, 1) => "has a single column of nodes",
        _ => return None,
    })
}

/// Returns the name of a dimension as messages write it.
fn dimension_name(dimension: Dimension) -> &'static str {
    match dimension {
        Dimension::X => "x",
        Dimension::Y => "y",
        Dimension::Z => "z",
    }
}

/// Returns the name of the plane of an image as the schema writes it.
fn plane_name(plane: ImagePlane) -> &'static str {
    match plane {
        ImagePlane::Xy { .. } => "xy",
        ImagePlane::Xz { .. } => "xz",
        ImagePlane::Yz { .. } => "yz",
    }
}

/// Calls `visit` with every value of an array as a floating-point number, an 8-bit value
/// widened to the number it denotes.
fn for_each_value(array: &NdArray, mut visit: impl FnMut(f64)) {
    match &array.values {
        Values::F64(values) => values.iter().for_each(|&value| visit(value)),
        Values::U8(values) => values.iter().for_each(|&value| visit(f64::from(value))),
    }
}

/// One of the two image kinds, seen through what validation needs of it.
pub(crate) struct ImageView<'a> {
    /// The pixels or values of the image.
    data: NodeId,
    /// The name of the data field, for messages.
    what: &'static str,
    /// Whether the array holds the colour components of every pixel along a third
    /// dimension, as the pixels of a true-colour image do.
    channels: bool,
    /// The alpha of every pixel of a scalar image, if it has one.
    alpha: Option<NodeId>,
    /// Where the pixels lie in the axes.
    placement: &'a ImagePlacement,
}

/// How an artist uses its data and the dimensions of its axes, and the checks of its own
/// that its kind adds.
pub(crate) struct ArtistUsage<'a> {
    /// The identifier of every data array the artist refers to.
    references: Vec<NodeId>,
    /// The arrays whose values are plotted as positions along a dimension.
    positions: Vec<(Dimension, NodeId)>,
    /// Whether the artist can only be drawn in a three-dimensional axes, because it needs
    /// the z axis to be placed. A surface does not: seen from directly above, it is the
    /// pseudocolour plot of a two-dimensional axes.
    three_d: bool,
    /// The references that may name arrays of 8-bit values, as the data and alpha of an
    /// image and the values and alpha of cells may; every other reference requires 64-bit
    /// floating-point values.
    bytes: Vec<NodeId>,
    /// Whether the artist can only be drawn in a two-dimensional axes, as bars, stairs and
    /// cells can.
    two_d_only: bool,
    /// The errors of the artist that its properties alone decide, each with its kind and
    /// message.
    problems: Vec<(IssueKind, String)>,
    /// Whether the x and y values of a series are fractions of the plot area
    /// rather than data.
    fraction: [bool; 2],
    /// Whether the artist releases its clip, as only a series can.
    clip_released: bool,
    /// Each opacity of the artist, with its name for messages.
    opacities: Vec<(&'static str, f64)>,
    /// The levels of a contour, when it has levels of its own.
    levels: Option<&'a Levels>,
    /// The colour scale that the artist names, if any.
    color_scale: Option<NodeId>,
    /// How the artist maps numbers to colours, when it maps any.
    color: Option<ColorMapping>,
    /// How the arrays of the artist must agree in shape.
    shape: Shape<'a>,
}

/// The colour data of a series, with what decides which lengths of it are valid.
#[derive(Clone, Copy)]
struct SeriesColors {
    /// The colour data.
    data: NodeId,
    /// Whether the series draws a line, without which there are no segments to colour.
    draws_line: bool,
    /// Whether the series draws markers with a colormapped face or edge, which need one
    /// value per point.
    mapped_markers: bool,
}

/// How the arrays of an artist must agree in shape, and what gives it something to draw.
enum Shape<'a> {
    /// Arrays of one value per point, which must have the same number of elements; the
    /// artist has nothing to draw when the array of x positions is empty.
    Points {
        /// The arrays, named as a mismatch of their numbers of elements describes them.
        what: &'static str,
        /// The arrays, of which those present are compared.
        arrays: Vec<Option<NodeId>>,
        /// The arrays of positions, named as an artist with no points describes them.
        points: &'static str,
        /// The array of x positions.
        x: NodeId,
        /// The colour data of a series, whose length is checked against the number of
        /// points by its own rule.
        colors: Option<SeriesColors>,
    },
    /// A field on a grid, which must be two-dimensional and match its grid; the artist
    /// has nothing to draw when the grid has no cell between its nodes.
    Grid {
        /// The grid on which the field is sampled.
        grid: Grid,
        /// The field.
        field: NodeId,
        /// The arrays that must have the shape of the field, each named for messages.
        like_field: Vec<(&'static str, Option<NodeId>)>,
    },
    /// An image, which [`Validator::check_image`] checks.
    Image(ImageView<'a>),
    /// Bars, whose arrays hold one entry per bar; they have nothing to draw when they
    /// have no values.
    Bars(&'a Bar),
    /// Stairs, whose edges or samples must fit their values; they have nothing to draw
    /// when they have no values.
    Stairs(&'a Stairs),
    /// Cells, whose values are two-dimensional and whose edges fit them; they have
    /// nothing to draw when they have no rows or no columns.
    Cells(&'a Cells),
}

impl ArtistUsage<'_> {
    /// Returns whether the values along `dimension` are fractions of the plot area.
    fn is_fraction(&self, dimension: Dimension) -> bool {
        match dimension {
            Dimension::X => self.fraction[0],
            Dimension::Y => self.fraction[1],
            Dimension::Z => false,
        }
    }
}

impl<'a> ArtistUsage<'a> {
    /// The usage of an artist that uses its data and its axes as `references`,
    /// `positions`, `three_d` and `shape` say, accepts only 64-bit floating-point values,
    /// places every dimension in data coordinates, keeps its clip, and has no opacity and
    /// no levels; the kinds of artist that differ say so. The colour scale and the
    /// mapping of the artist are filled in from the artist itself, for every kind alike.
    fn new(
        references: Vec<NodeId>,
        positions: Vec<(Dimension, NodeId)>,
        three_d: bool,
        shape: Shape<'a>,
    ) -> Self {
        Self {
            references,
            positions,
            three_d,
            bytes: Vec::new(),
            two_d_only: false,
            problems: Vec::new(),
            fraction: [false; 2],
            clip_released: false,
            opacities: Vec::new(),
            levels: None,
            color_scale: None,
            color: None,
            shape,
        }
    }

    /// The usage of an image, which refers to its pixels or values and to its
    /// alpha, if it has one, each of either element type, plots nothing as a position
    /// along an axis, and needs a three-dimensional axes when it lies on a wall.
    fn image(image: ImageView<'a>, opacity: f64) -> Self {
        let references: Vec<NodeId> = [Some(image.data), image.alpha]
            .into_iter()
            .flatten()
            .collect();
        Self {
            bytes: references.clone(),
            opacities: vec![("opacity", opacity)],
            ..Self::new(
                references,
                Vec::new(),
                !matches!(image.placement.plane, ImagePlane::Xy { .. }),
                Shape::Image(image),
            )
        }
    }
}

/// Returns the arrays of a grid plotted as positions along x and y.
fn grid_positions(grid: Grid) -> Vec<(Dimension, NodeId)> {
    match grid {
        Grid::Rectilinear { x, y } | Grid::Curvilinear { x, y } => {
            vec![(Dimension::X, x), (Dimension::Y, y)]
        }
    }
}

/// Returns the arrays of x, y and, when present, z positions of a series or quiver.
fn point_positions(x: NodeId, y: NodeId, z: Option<NodeId>) -> Vec<(Dimension, NodeId)> {
    let mut positions = vec![(Dimension::X, x), (Dimension::Y, y)];
    positions.extend(z.map(|z| (Dimension::Z, z)));
    positions
}

/// Returns the identifiers of the arrays of positions.
fn ids(positions: &[(Dimension, NodeId)]) -> Vec<NodeId> {
    positions.iter().map(|&(_, id)| id).collect()
}

impl Usage for Series {
    fn usage(&self) -> ArtistUsage<'_> {
        let positions = point_positions(self.x, self.y, self.z);
        let mut references = ids(&positions);
        references.extend(self.size_data);
        references.extend(self.color_data);
        let colors = self.color_data.map(|data| SeriesColors {
            data,
            draws_line: self.draws_line(),
            mapped_markers: self.draws_markers()
                && [self.marker.face, self.marker.edge].contains(&ColorSpec::Colormapped),
        });
        let shape = Shape::Points {
            what: "the coordinate and size arrays",
            arrays: vec![Some(self.x), Some(self.y), self.z, self.size_data],
            points: "the coordinate arrays",
            x: self.x,
            colors,
        };
        ArtistUsage {
            fraction: fractions(self.x_coords, self.y_coords),
            clip_released: !self.clip,
            ..ArtistUsage::new(references, positions, self.z.is_some(), shape)
        }
    }
}

impl Usage for Quiver {
    fn usage(&self) -> ArtistUsage<'_> {
        let positions = point_positions(self.x, self.y, self.z);
        let mut references = ids(&positions);
        references.extend([self.u, self.v]);
        references.extend(self.w);
        let shape = Shape::Points {
            what: "the position and component arrays",
            arrays: vec![
                Some(self.x),
                Some(self.y),
                self.z,
                Some(self.u),
                Some(self.v),
                self.w,
            ],
            points: "the position arrays",
            x: self.x,
            colors: None,
        };
        let three_d = self.z.is_some() || self.w.is_some();
        ArtistUsage::new(references, positions, three_d, shape)
    }
}

impl Usage for Contour {
    fn usage(&self) -> ArtistUsage<'_> {
        let at_level = self.placement == ContourPlacement::AtLevel;
        let mut positions = grid_positions(self.grid);
        let mut references = ids(&positions);
        references.push(self.z);
        if at_level {
            positions.push((Dimension::Z, self.z));
        }
        let shape = Shape::Grid {
            grid: self.grid,
            field: self.z,
            like_field: Vec::new(),
        };
        ArtistUsage {
            opacities: vec![
                ("line opacity", self.line_opacity),
                ("fill opacity", self.fill_opacity),
            ],
            levels: self.levels.as_ref(),
            ..ArtistUsage::new(references, positions, at_level, shape)
        }
    }
}

impl Usage for Surface {
    fn usage(&self) -> ArtistUsage<'_> {
        let mut positions = grid_positions(self.grid);
        positions.push((Dimension::Z, self.z));
        let mut references = ids(&positions);
        references.extend(self.c);
        references.extend(self.face_alpha);
        references.extend(self.edge_alpha);
        let shape = Shape::Grid {
            grid: self.grid,
            field: self.z,
            like_field: vec![
                ("the colour data", self.c),
                ("the face alpha", self.face_alpha),
                ("the edge alpha", self.edge_alpha),
            ],
        };
        // A surface is valid in either projection. In a two-dimensional axes it is seen
        // from directly above, so its field positions nothing there; the check of an
        // artist passes over positions along z in a two-dimensional axes.
        ArtistUsage {
            opacities: vec![
                ("face opacity", self.face_opacity),
                ("edge opacity", self.edge_opacity),
            ],
            ..ArtistUsage::new(references, positions, false, shape)
        }
    }
}

impl Usage for Image {
    fn usage(&self) -> ArtistUsage<'_> {
        let image = ImageView {
            data: self.pixels,
            what: "pixels",
            channels: true,
            alpha: None,
            placement: &self.placement,
        };
        ArtistUsage::image(image, self.opacity)
    }
}

impl Usage for ScalarImage {
    fn usage(&self) -> ArtistUsage<'_> {
        let image = ImageView {
            data: self.values,
            what: "values",
            channels: false,
            alpha: self.alpha,
            placement: &self.placement,
        };
        ArtistUsage::image(image, self.opacity)
    }
}

impl Usage for Bar {
    fn usage(&self) -> ArtistUsage<'_> {
        let positions = vec![
            (self.orientation.category_dimension(), self.positions),
            (self.orientation.value_dimension(), self.values),
        ];
        let mut references = ids(&positions);
        if let BarBase::Data(base) = self.base {
            references.push(base);
        }
        if let BarSlot::Data(widths) = self.slot {
            references.push(widths);
        }
        let mut problems = gap_problems("gap of the bars", self.gap);
        if let BarSlot::Width(width) = self.slot
            && !(width.is_finite() && width > 0.0)
        {
            problems.push((
                IssueKind::InvalidBarSlot,
                format!(
                    "the width {width} of the slot of the bars is not a finite positive number"
                ),
            ));
        }
        problems.extend(colormap_problems(
            "bars",
            &[("face", self.face), ("edge", self.edge.color)],
        ));
        ArtistUsage {
            two_d_only: true,
            problems,
            opacities: vec![("face opacity", self.face_opacity)],
            ..ArtistUsage::new(references, positions, false, Shape::Bars(self))
        }
    }
}

impl Usage for Stairs {
    fn usage(&self) -> ArtistUsage<'_> {
        let positions = vec![
            (self.orientation.category_dimension(), self.data.positions()),
            (self.orientation.value_dimension(), self.data.values()),
        ];
        let references = ids(&positions);
        let mut problems =
            colormap_problems("stairs", &[("line", self.line.color), ("fill", self.fill)]);
        if self.fill != ColorSpec::None && self.base.is_none() {
            problems.push((
                IssueKind::StairsFillWithoutBase,
                "the stairs are filled but have no base, so there is nothing to fill down to"
                    .to_owned(),
            ));
        }
        ArtistUsage {
            two_d_only: true,
            problems,
            opacities: vec![("fill opacity", self.fill_opacity)],
            ..ArtistUsage::new(references, positions, false, Shape::Stairs(self))
        }
    }
}

impl Usage for Cells {
    fn usage(&self) -> ArtistUsage<'_> {
        let bytes: Vec<NodeId> = [Some(self.values), self.alpha]
            .into_iter()
            .flatten()
            .collect();
        let mut references = bytes.clone();
        for edges in [self.columns, self.rows] {
            if let CellEdges::Edges(edges) = edges {
                references.push(edges);
            }
        }
        let mut problems = gap_problems("gap of the cells along x", self.gap.x);
        problems.extend(gap_problems("gap of the cells along y", self.gap.y));
        // The edges of cells are checked on a logarithmic axis by a check of their own,
        // which knows that unit edges start at −0.5, so they are not listed as positions.
        ArtistUsage {
            bytes,
            two_d_only: true,
            problems,
            opacities: vec![("opacity", self.opacity)],
            ..ArtistUsage::new(references, Vec::new(), false, Shape::Cells(self))
        }
    }
}

/// Returns the problem of a gap, named `what` in the message, that is not a finite number
/// from 0 up to, but not including, 1.
fn gap_problems(what: &str, gap: f64) -> Vec<(IssueKind, String)> {
    if (0.0..1.0).contains(&gap) {
        Vec::new()
    } else {
        vec![(
            IssueKind::InvalidGap,
            format!(
                "the {what} is {gap}, but it must be a number from 0 up to, but not including, 1"
            ),
        )]
    }
}

/// Returns a problem for each named colour of `artist`, which has no colour scale, that is
/// colormapped.
fn colormap_problems(artist: &str, colors: &[(&str, ColorSpec)]) -> Vec<(IssueKind, String)> {
    colors
        .iter()
        .filter(|&&(_, color)| color == ColorSpec::Colormapped)
        .map(|&(what, _)| {
            (
                IssueKind::ColormapNotSupported,
                format!(
                    "the {what} colour of the {artist} is colormapped, but {artist} have no \
                     colour scale to look it up in"
                ),
            )
        })
        .collect()
}

/// Returns whether the x and y values of a series are fractions of the plot area.
fn fractions(x: Coords, y: Coords) -> [bool; 2] {
    [x == Coords::AxesFraction, y == Coords::AxesFraction]
}

/// Returns the problems of a colour scale itself, each with its kind and message, which
/// are reported against the scale and make it unusable for checking the numbers mapped
/// through it.
fn color_scale_problems(scale: &ColorScale) -> Vec<(IssueKind, String)> {
    let mut problems = Vec::new();
    let log = scale.kind.norm() == ColorScaleNorm::Log;
    let mut colormap = |colormap: &Colormap| {
        if let Colormap::Listed { colors } = colormap
            && !(LISTED_COLORMAP_MIN..=LISTED_COLORMAP_MAX).contains(&colors.len())
        {
            problems.push((
                IssueKind::InvalidColormap,
                format!(
                    "the listed colormap holds {} colours, but it must hold from \
                     {LISTED_COLORMAP_MIN} to {LISTED_COLORMAP_MAX}",
                    colors.len()
                ),
            ));
        }
    };
    match &scale.kind {
        ColorScaleKind::Continuous(continuous) => colormap(&continuous.colormap),
        ColorScaleKind::Discrete(discrete) => colormap(&discrete.colormap),
        ColorScaleKind::Indexed(_) | ColorScaleKind::Categorical(_) => {}
    }
    if let Some(limits) = scale.kind.limits()
        && let Some(message) = limits_problem("limits", limits, log)
    {
        problems.push((IssueKind::InvalidLimits, message));
    }
    let band_edges = match &scale.kind {
        ColorScaleKind::Discrete(discrete) => match &discrete.levels {
            ColorScaleLevels::Auto { count: 0, .. } => {
                Some("the automatic number of bands is zero".to_owned())
            }
            ColorScaleLevels::Auto { .. } => None,
            ColorScaleLevels::Explicit { values } if values.len() < 2 => Some(format!(
                "there are {} band edges, but a band needs two",
                values.len()
            )),
            ColorScaleLevels::Explicit { values } if !values.iter().all(|v| v.is_finite()) => {
                Some("a band edge is not finite".to_owned())
            }
            ColorScaleLevels::Explicit { values }
                if values.windows(2).any(|pair| pair[0] >= pair[1]) =>
            {
                Some("the band edges are not strictly increasing".to_owned())
            }
            ColorScaleLevels::Explicit { values } if log && values[0] <= 0.0 => Some(format!(
                "the first band edge {} is not positive under a logarithmic norm",
                values[0]
            )),
            ColorScaleLevels::Explicit { .. } => None,
        },
        _ => None,
    };
    problems.extend(band_edges.map(|message| (IssueKind::InvalidLevels, message)));
    let entries = match &scale.kind {
        ColorScaleKind::Indexed(indexed) if indexed.colors.is_empty() => {
            Some("the indexed colour scale has no entries, so it maps no value to a colour")
        }
        ColorScaleKind::Categorical(categorical) if categorical.categories.is_empty() => {
            Some("the categorical colour scale has no categories, so it maps no value to a colour")
        }
        _ => None,
    };
    problems.extend(entries.map(|message| (IssueKind::InvalidColorScale, message.to_owned())));
    let ticks = match &scale.kind {
        ColorScaleKind::Continuous(continuous) => {
            Some((&continuous.ticks, &continuous.tick_labels))
        }
        ColorScaleKind::Discrete(discrete) => Some((&discrete.ticks, &discrete.tick_labels)),
        ColorScaleKind::Indexed(indexed) => Some((&indexed.ticks, &indexed.tick_labels)),
        ColorScaleKind::Categorical(_) => None,
    };
    if let Some((ticks, labels)) = ticks {
        problems.extend(tick_problems("colour scale", ticks, labels, log));
    }
    problems
}

/// Returns why manual limits, named `name` in the message, cannot be used, or `None` when
/// they can or are automatic: they must be finite and strictly increasing, and positive
/// when `log` is set.
fn limits_problem(name: &str, limits: Limits, log: bool) -> Option<String> {
    let Limits::Manual { min, max } = limits else {
        return None;
    };
    if !(min.is_finite() && max.is_finite() && min < max) {
        Some(format!(
            "the {name} [{min}, {max}] are not finite and strictly increasing"
        ))
    } else if log && min <= 0.0 {
        Some(format!(
            "the {name} [{min}, {max}] are not positive on a logarithmic scale"
        ))
    } else {
        None
    }
}

/// Returns the problems of the ticks of an axis or a colour scale, named `name` in the
/// messages: ticks that cannot be placed, and explicit tick labels that do not name
/// explicit tick values one by one. `log` says whether the ticks lie on a logarithmic
/// axis or scale, where every explicit value must be positive.
fn tick_problems(
    name: &str,
    ticks: &Ticks,
    tick_labels: &TickLabels,
    log: bool,
) -> Vec<(IssueKind, String)> {
    let mut problems = Vec::new();
    let placement = match ticks {
        Ticks::Auto => None,
        Ticks::Step { step, origin } => {
            if !(step.is_finite() && *step > 0.0) {
                Some(format!(
                    "the {name} tick step {step} is not a finite positive number"
                ))
            } else if !origin.is_finite() {
                Some(format!("the {name} tick origin {origin} is not finite"))
            } else {
                None
            }
        }
        Ticks::Values(values) => {
            let not_finite = values.iter().find(|v| !v.is_finite());
            let not_positive = values.iter().find(|v| log && **v <= 0.0);
            match (not_finite, not_positive) {
                (Some(value), _) => Some(format!("the {name} tick value {value} is not finite")),
                (None, Some(value)) => Some(format!(
                    "the {name} tick value {value} is not positive on a logarithmic scale"
                )),
                (None, None) => None,
            }
        }
    };
    problems.extend(placement.map(|message| (IssueKind::InvalidTicks, message)));
    if let TickLabels::Text(texts) = tick_labels {
        let labels = match ticks {
            Ticks::Values(values) if values.len() == texts.len() => None,
            Ticks::Values(values) => Some(format!(
                "the {name} has {} tick labels for {} tick values",
                texts.len(),
                values.len()
            )),
            Ticks::Auto | Ticks::Step { .. } => Some(format!(
                "the {name} has explicit tick labels but its ticks are not explicit values"
            )),
        };
        problems.extend(labels.map(|message| (IssueKind::InvalidTickLabels, message)));
    }
    problems
}
