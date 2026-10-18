//! Structural validation of figures.

use std::collections::{BTreeMap, BTreeSet};

use crate::artist::{
    Artist, Contour, ContourPlacement, Coords, Grid, ImagePlacement, ImagePlane, Levels,
    OutOfRange, PixelRange, ScatterColor, ScatterSize,
};
use crate::axes::{Aspect, Axes, Axis, Limits, Projection, Scale, TickLabels, Ticks};
use crate::data::{NdArray, NdArrayElement, Values};
use crate::figure::{Figure, Parameter};
use crate::ids::{DataId, NodeId};
use crate::layout::{Cell, TrackSize};
use crate::legend::{
    Legend, LegendEntries, LegendEntryColumns, LegendEntryLayout, LegendItem, LegendItemContent,
};
use crate::link::Dimension;

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
    /// An artist refers to a data identifier that is not in the data table.
    UnknownData,
    /// An array's number of values differs from the product of its shape.
    InvalidArray,
    /// The arrays referenced by an artist have inconsistent lengths or shapes.
    ShapeMismatch,
    /// An artist refers to an array whose element type it cannot use: every artist
    /// other than an image requires 64-bit floating-point values, so such an artist
    /// cannot plot an array of 8-bit values, and the alpha of a surface must hold
    /// 64-bit floating-point values too. The three image kinds accept either element
    /// type, for their alpha as for their pixels, indices or values.
    ElementTypeMismatch,
    /// An artist or placement that needs the z axis (z or w data, contours at their
    /// level, an image on a wall) is used in a two-dimensional axes. A surface is not
    /// such an artist: in a two-dimensional axes it is seen from directly above.
    ThreeDArtistInTwoDAxes,
    /// A line or scatter places a dimension in axes-fraction coordinates in a
    /// three-dimensional axes, which has no plot area along its dimensions to take a
    /// fraction of.
    AxesFractionInThreeDAxes,
    /// A line or scatter releases its clip (`clip` is false) although it places no
    /// dimension in axes-fraction coordinates, or lies in a three-dimensional axes, so
    /// that the release has no effect: clipping is released only along a dimension
    /// placed as fractions of the plot area of a two-dimensional axes. This is a
    /// warning, not an error, because the artist is drawn, clipped to the plot area.
    IneffectiveClip,
    /// An axis link refers to an identifier that is not an axes of the figure.
    DanglingLink,
    /// Two nodes share an identifier.
    DuplicateNodeId,
    /// The cell of an axes or of a legend panel lies outside the figure's tile layout or
    /// has a zero span.
    CellOutOfLayout,
    /// The cells of two nodes placed in the tile layout, axes or legend panels, share at
    /// least one tile, so that the later is drawn over the earlier. This is a warning, not
    /// an error, because the figure can still be drawn. Each overlapping pair is reported
    /// once, against the later of the two nodes in the order axes first, then legend
    /// panels, each in the order the figure lists them, and the message names both nodes.
    /// A cell outside the tile layout, which is reported as [`IssueKind::CellOutOfLayout`],
    /// is not compared with any other.
    OverlappingCells,
    /// The legend of an axes names a legend panel that the figure does not have.
    DanglingPanelLegend,
    /// The tile layout has more row sizes than rows or more column sizes than columns,
    /// or a weight that is not a finite positive number.
    InvalidLayoutWeights,
    /// An axes lies in fitted rows alone, or in fitted columns alone. A fitted row or
    /// column takes its size from the legend panels in it, and an axes has no content
    /// size of its own, so the axes would have no height or no width. An axes that spans
    /// a fitted row or column together with a weighted one is valid.
    AxesInFittedTrack,
    /// An item of an explicit list of legend entries names an artist that the figure
    /// does not have, or, in a legend inside a plot area, an artist of another axes.
    DanglingLegendItem,
    /// An explicit list of legend entries holds the same item more than once: two items
    /// that name the same artist, or two items with the same label and sample. This is a
    /// warning, not an error, because the legend can still be drawn: every item is
    /// listed where it stands.
    DuplicateLegendItem,
    /// An item of an explicit list of legend entries names an artist that has no display
    /// name, so the item shows its sample without a label. This is a warning, not an
    /// error, because the legend can still be drawn and a display name can be given
    /// later, in code or in the viewer.
    UnnamedLegendItem,
    /// A legend, inside a plot area or in a panel, has a fixed number of columns of
    /// zero.
    InvalidLegendColumns,
    /// The figure's width, height or base font size is not a finite positive number.
    InvalidSize,
    /// Manual limits (of a coordinate axis or of the colour limits) are not finite,
    /// are not strictly increasing, or are not positive on a logarithmic axis.
    InvalidLimits,
    /// Contour levels are empty, not finite or not strictly increasing, or an
    /// automatic level count is zero.
    InvalidLevels,
    /// The ticks of a coordinate axis cannot be placed: a regular step that is not a
    /// finite positive number or an origin that is not finite, an explicit tick value
    /// that is not finite, or an explicit tick value that is not positive on a
    /// logarithmic axis. Only the axes that are drawn are checked: x and y, and z in a
    /// three-dimensional axes.
    InvalidTicks,
    /// The explicit tick labels of a coordinate axis do not name its ticks one by one:
    /// the ticks are not explicit values, or the number of labels differs from the
    /// number of values.
    InvalidTickLabels,
    /// The aspect of an axes cannot be kept: a ratio of y or of z that is not a finite
    /// positive number. The ratio of z is checked on a two-dimensional axes too, which
    /// ignores it, so that a figure stays valid when it is made three-dimensional.
    InvalidAspect,
    /// Data plotted along a logarithmic axis contains finite values less than or
    /// equal to zero, which are not drawn.
    NonPositiveOnLogAxis,
    /// An artist whose data gives it nothing to draw, so that it is left out of the
    /// drawing: a line, scatter or quiver of no points; an image of any kind with no rows
    /// or no columns; or a contour or surface whose field has no rows or no columns, or a
    /// single row or a single column (values, but no cell between nodes to draw them in).
    /// The message states which case applies and the shape of the array. This is a
    /// warning, not an error, because empty and singleton data are valid: a figure built
    /// before its data arrives, or streamed from nothing, passes through these states. It
    /// is reported only when the artist's references and shapes are otherwise valid, so
    /// that an artist reported for an error is not reported twice. It is also reported
    /// against a legend panel that has no entries to list, because no axes names it in
    /// its legend or no axes that names it has an artist with a display name.
    NothingToDraw,
    /// A figure parameter has an empty name, or is a number that is not finite (which
    /// JSON cannot represent).
    InvalidParameter,
    /// A figure label is empty, or occurs more than once.
    ///
    /// An empty label describes nothing and cannot be chosen in the viewer, and a label
    /// repeated is a label that says no more than it did the first time; both are
    /// mistakes in the code that built the figure rather than choices it could have
    /// meant, so both are errors.
    InvalidLabel,
    /// The placement of an image cannot be drawn: a pixel centre or the offset of its
    /// plane is not finite, the centres of the first and last pixels along an axis
    /// coincide although the image has more than one pixel along that axis, or, in a
    /// three-dimensional axes, the plane is offset to a coordinate that is not positive
    /// along a logarithmic third axis, which has no place for it.
    InvalidImagePlacement,
    /// A pixel of a colour-indexed or colour-mapped image falls in a category whose
    /// out-of-range policy is strict: an index outside the colormap, a value outside
    /// manual colour limits, or an index or value that is not finite.
    PixelOutOfRange,
    /// An image lies in a plane of which an axis is logarithmic. An image is a raster of
    /// evenly spaced pixels, which has no placement on a logarithmic axis, where equal
    /// steps in the data are unequal distances on the page, so the figure is refused
    /// rather than drawn without the image. Values sampled along a logarithmic axis are
    /// drawn as a [`Surface`](crate::Surface) instead, whose every node is placed
    /// through the logarithmic scale; the message says how. Only the axes of the plane
    /// count: the third axis, along which the plane is offset, may be logarithmic, and
    /// the z axis of a two-dimensional axes is ignored.
    ImageOnLogAxis,
    /// The opacity of an artist (of an image, or of the faces or edges of a surface) is
    /// not a finite number from 0 to 1.
    InvalidOpacity,
}

impl Figure {
    /// Checks the figure for structural problems.
    ///
    /// Errors report unknown data identifiers, arrays whose values do not match their
    /// shape, arrays of 8-bit values referenced by an artist other than an image (every
    /// other artist requires 64-bit floating-point values, the alpha of a surface among
    /// them), inconsistent array lengths and shapes within an artist (for an image,
    /// pixels that are not an array of shape `[ny, nx, 3]` or `[ny, nx, 4]`, or indices
    /// or values that are not two-dimensional; for a surface, colour data or alpha whose
    /// shape differs from the field's; for a colour-indexed or colour-mapped image, alpha
    /// whose shape differs from the indices' or values'), an opacity that is not a finite
    /// number from 0 to 1,
    /// 3D artists in 2D axes (an image on the xz or yz plane among them), lines and
    /// scatters in 3D axes with a dimension in axes-fraction coordinates, links to
    /// identifiers that are not axes, duplicate node identifiers, cells of axes and
    /// legend panels outside the tile layout, layout sizes that are too many or weights
    /// that are not finite and positive, axes that lie in fitted rows alone or in fitted
    /// columns alone, legends that name a legend panel the figure does not have, items of
    /// an explicit list of legend entries that name no artist of the figure (or, inside a
    /// plot area, an artist of another axes),
    /// legends with a fixed number of zero columns, a non-positive figure size or font
    /// size, invalid manual limits, ticks that cannot be placed (a regular step that is
    /// not finite and positive or an origin that is not finite, and explicit tick values
    /// that are not finite or, on a logarithmic axis, not positive), explicit tick labels
    /// that do not name explicit tick values one by one, invalid
    /// contour levels, parameters with an empty name or a non-finite number, labels that
    /// are empty or repeated, an image
    /// placement whose pixel centres or plane offset are not finite or whose first and
    /// last centres coincide along an axis of more than one pixel, an image whose plane
    /// has a logarithmic axis (which must be drawn as a surface instead) or, in a 3D
    /// axes, is offset to a coordinate that is not positive along a logarithmic third
    /// axis, and pixels of a colour-indexed or colour-mapped image that fall in a
    /// category whose out-of-range policy is strict (for a colour-mapped image, a value lies below or above the range
    /// only when the colour limits are manual and valid, because automatic limits are
    /// the range of the data). Warnings report finite non-positive data plotted along
    /// logarithmic axes, a line or scatter whose released clip has no effect because it
    /// places no dimension in axes-fraction coordinates of a 2D axes, and an artist whose data gives it nothing to draw, which is left out (a
    /// line, scatter or quiver of no points, an image with no rows or no columns, and a
    /// contour or surface whose field has no rows or no columns or a single row or
    /// column, which has no cell between its nodes), a legend panel with no entries
    /// to list, which no axes names or whose axes have no artist with a display name, or
    /// whose explicit list has no items, an item of an explicit list that repeats an
    /// earlier item, an item that names an artist without a display name, and
    /// two nodes of the tile layout, axes or legend panels, whose cells share a tile, once
    /// per pair against the later node (axes before legend panels, each in the order the
    /// figure lists them), comparing only cells that lie within the layout;
    /// data that is not plotted along an
    /// axis (such as quiver components, colour data, the pixels of an image, or the
    /// values of a dimension in axes-fraction coordinates) never produces the first of
    /// these. An array of 8-bit values that no artist refers to
    /// is not an error.
    ///
    /// The z limits, z scale and z ticks of a 2D axes are ignored, as they are when drawing, and
    /// so is the offset of an image in the xy plane of a 2D axes. A line, scatter or
    /// quiver without z data in a 3D axes is not an error: it is drawn in the plane
    /// z = 0. Shape checks are skipped for an artist that refers to unknown or invalid
    /// arrays, or (other than an image) to 8-bit arrays, so that one problem is not
    /// reported repeatedly; the coincidence of an image's pixel centres and its strict
    /// policies are checked only when its array is valid and of the right shape, because
    /// both depend on the pixels, whereas a non-finite centre or offset is always
    /// reported. An artist is reported as having nothing to draw only when it is
    /// reported for no error, so that the error, which is what must be fixed, stands
    /// alone.
    pub fn validate(&self) -> ValidationReport {
        let mut validator = Validator {
            figure: self,
            report: ValidationReport::default(),
        };
        validator.check_figure();
        validator.check_layout_weights();
        for panel in &self.panel_legends {
            validator.check_cell(panel.id, panel.cell);
            validator.check_legend_entry_layout(panel.id, panel.layout);
            validator.check_legend_items(panel.id, None, &panel.entries);
            validator.check_panel_has_entries(panel.id, &panel.entries);
        }
        validator.check_overlapping_cells();
        for axes in &self.axes {
            validator.check_axes(axes);
            for artist in &axes.artists {
                validator.check_artist(axes, artist);
            }
        }
        validator.report
    }
}

/// Accumulates the issues found in a figure.
struct Validator<'a> {
    figure: &'a Figure,
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

impl Validator<'_> {
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

    /// Checks the figure-level properties, the parameters, the labels, the data table,
    /// node identifiers and links.
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

        // Labels are compared exactly, so labels differing only in case are distinct and
        // neither is a repeat of the other.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for (index, label) in figure.labels.iter().enumerate() {
            if label.is_empty() {
                // An empty label has no text to name it by, so it is named by where it
                // is: a figure carrying twenty labels would otherwise report a fault the
                // reader cannot find.
                self.error(
                    Some(figure.id),
                    IssueKind::InvalidLabel,
                    format!("the label in position {} is empty", index + 1),
                );
            } else if !seen.insert(label.as_str()) {
                self.error(
                    Some(figure.id),
                    IssueKind::InvalidLabel,
                    format!("the label {label:?} occurs more than once"),
                );
            }
        }

        for (id, array) in &figure.data {
            if !is_consistent(array) {
                self.error(
                    None,
                    IssueKind::InvalidArray,
                    format!(
                        "{id} has {} values, which does not match its shape {:?}",
                        array.len(),
                        array.shape
                    ),
                );
            }
        }

        let mut counts: BTreeMap<NodeId, usize> = BTreeMap::new();
        for id in figure.node_ids() {
            *counts.entry(id).or_default() += 1;
        }
        for (id, count) in counts {
            if count > 1 {
                self.error(
                    Some(id),
                    IssueKind::DuplicateNodeId,
                    format!("{count} nodes share the identifier of {id}"),
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

    /// Checks that a cell of an axes or a legend panel lies within the tile layout.
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
    /// later node of the pair, taking the axes first and then the legend panels. Cells
    /// outside the layout are reported by [`Self::check_cell`] and are not compared.
    fn check_overlapping_cells(&mut self) {
        let figure = self.figure;
        let placed: Vec<(NodeId, Cell)> = figure
            .axes
            .iter()
            .map(|axes| (axes.id, axes.cell))
            .chain(
                figure
                    .panel_legends
                    .iter()
                    .map(|panel| (panel.id, panel.cell)),
            )
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
                         legend panels, so the axes would have no {extent}"
                    ),
                );
            }
        }
    }

    /// Checks the items of an explicit list of legend entries: that each artist they name
    /// is an artist of the figure (of the axes `own`, for a legend inside its plot area)
    /// with a display name, and that no item is listed twice.
    fn check_legend_items(&mut self, legend: NodeId, own: Option<&Axes>, entries: &LegendEntries) {
        let LegendEntries::Explicit(items) = entries else {
            return;
        };
        let figure = self.figure;
        for (index, item) in items.iter().enumerate() {
            if items[..index]
                .iter()
                .any(|earlier| earlier.content == item.content)
            {
                self.warning(
                    Some(legend),
                    IssueKind::DuplicateLegendItem,
                    format!(
                        "legend item {index} repeats an earlier item, so the legend lists it twice"
                    ),
                );
            }
            let LegendItem {
                content: LegendItemContent::Artist(artist),
                ..
            } = item
            else {
                continue;
            };
            let found = match own {
                Some(axes) => axes.artists.iter().find(|a| a.id() == *artist),
                None => figure.artist(*artist).map(|(_, a)| a),
            };
            match found {
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
                Some(found) if found.display_name().is_none() => self.warning(
                    Some(legend),
                    IssueKind::UnnamedLegendItem,
                    format!(
                        "legend item {index} names {artist}, which has no display name, so the \
                         item has no label"
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
            Some(Legend::Panel(panel)) if self.figure.panel_legend(*panel).is_none() => {
                self.error(
                    Some(axes.id),
                    IssueKind::DanglingPanelLegend,
                    format!("the legend lists its entries in {panel}, which is not a legend panel"),
                );
            }
            Some(Legend::Panel(_)) | None => {}
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
        for (name, axis) in axes_drawn {
            self.check_ticks(axes.id, name, axis);
        }
        limits.push(("colour limits", axes.clim, Scale::Linear));
        for (name, limits, scale) in limits {
            let Limits::Manual { min, max } = limits else {
                continue;
            };
            if !(min.is_finite() && max.is_finite() && min < max) {
                self.error(
                    Some(axes.id),
                    IssueKind::InvalidLimits,
                    format!("the {name} [{min}, {max}] are not finite and strictly increasing"),
                );
            } else if scale == Scale::Log && min <= 0.0 {
                self.error(
                    Some(axes.id),
                    IssueKind::InvalidLimits,
                    format!("the {name} [{min}, {max}] are not positive on a logarithmic axis"),
                );
            }
        }
    }

    /// Checks that the ticks of one coordinate axis can be placed and that explicit tick
    /// labels name them one by one.
    fn check_ticks(&mut self, node: NodeId, name: &str, axis: &Axis) {
        let log = axis.scale == Scale::Log;
        let problem = match &axis.ticks {
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
                    (Some(value), _) => {
                        Some(format!("the {name} tick value {value} is not finite"))
                    }
                    (None, Some(value)) => Some(format!(
                        "the {name} tick value {value} is not positive on a logarithmic axis"
                    )),
                    (None, None) => None,
                }
            }
        };
        if let Some(message) = problem {
            self.error(Some(node), IssueKind::InvalidTicks, message);
        }
        if let TickLabels::Text(texts) = &axis.tick_labels {
            let problem = match &axis.ticks {
                Ticks::Values(values) if values.len() == texts.len() => None,
                Ticks::Values(values) => Some(format!(
                    "the {name} axis has {} tick labels for {} tick values",
                    texts.len(),
                    values.len()
                )),
                Ticks::Auto | Ticks::Step { .. } => Some(format!(
                    "the {name} axis has explicit tick labels but its ticks are not explicit values"
                )),
            };
            if let Some(message) = problem {
                self.error(Some(node), IssueKind::InvalidTickLabels, message);
            }
        }
    }

    /// Checks one artist of an axes.
    fn check_artist(&mut self, axes: &Axes, artist: &Artist) {
        let id = artist.id();
        let three_d = matches!(axes.projection, Projection::ThreeD { .. });
        let usage = ArtistUsage::of(artist);

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

        let released = match artist {
            Artist::Line(line) => !line.clip,
            Artist::Scatter(scatter) => !scatter.clip,
            _ => false,
        };
        if released && (three_d || !usage.fraction.iter().any(|&fraction| fraction)) {
            self.warning(
                Some(id),
                IssueKind::IneffectiveClip,
                "the clip is released, but clipping is released only along a dimension in \
                 axes-fraction coordinates of a two-dimensional axes, so the artist is still \
                 clipped to the plot area"
                    .to_owned(),
            );
        }

        if let Artist::Contour(contour) = artist {
            self.check_levels(contour);
        }
        // An artist reported for an invalid opacity is not also reported as having nothing to
        // draw, so that the error stands alone.
        let errors_before = self.report.errors.len();
        self.check_opacities(id, artist);

        let references_are_valid =
            self.check_references(id, &usage.references, usage.accepts_bytes);
        match ImageView::of(artist) {
            Some(image) => self.check_image(
                axes,
                three_d,
                id,
                &image,
                references_are_valid,
                errors_before,
            ),
            None if references_are_valid
                && self.check_shapes(id, artist)
                && self.report.errors.len() == errors_before =>
            {
                self.check_something_to_draw(id, artist);
            }
            None => {}
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
            let Some(values) = self.figure.data.get(&data).and_then(NdArray::as_f64) else {
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
    fn check_opacities(&mut self, node: NodeId, artist: &Artist) {
        let opacities: &[(&str, f64)] = match artist {
            Artist::Surface(surface) => &[
                ("face opacity", surface.face_opacity),
                ("edge opacity", surface.edge_opacity),
            ],
            Artist::Image(image) => &[("opacity", image.opacity)],
            Artist::IndexedImage(image) => &[("opacity", image.opacity)],
            Artist::MappedImage(image) => &[("opacity", image.opacity)],
            Artist::Contour(contour) => &[
                ("line opacity", contour.line_opacity),
                ("fill opacity", contour.fill_opacity),
            ],
            Artist::Line(_) | Artist::Scatter(_) | Artist::Quiver(_) => &[],
        };
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

    /// Reports each distinct unknown data reference of an artist and, unless the artist
    /// accepts 8-bit values, each distinct reference to an array of them, and returns
    /// whether every reference is to a known array of an element type the artist can
    /// use whose values match its shape.
    fn check_references(
        &mut self,
        node: NodeId,
        references: &[DataId],
        accepts_bytes: bool,
    ) -> bool {
        let mut valid = true;
        let mut reported: Vec<DataId> = Vec::new();
        for &data in references {
            let problem = match self.figure.data.get(&data) {
                Some(array) if !accepts_bytes && array.element() != NdArrayElement::F64 => Some((
                    IssueKind::ElementTypeMismatch,
                    format!(
                        "{data} holds {} values, but the artist requires f64 values",
                        array.element()
                    ),
                )),
                Some(array) => {
                    valid &= is_consistent(array);
                    None
                }
                None => Some((
                    IssueKind::UnknownData,
                    format!("{data} is not in the data table"),
                )),
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
    fn check_shapes(&mut self, node: NodeId, artist: &Artist) -> bool {
        let data = &self.figure.data;
        match artist {
            Artist::Line(line) => {
                let counts = [Some(line.x), Some(line.y), line.z];
                self.check_equal_counts(node, "the coordinate arrays", &counts)
            }
            Artist::Scatter(scatter) => {
                let size = match scatter.size {
                    ScatterSize::Data { data } => Some(data),
                    ScatterSize::Scalar { .. } => None,
                };
                let color = match scatter.color {
                    ScatterColor::Data { data } => Some(data),
                    ScatterColor::Spec { .. } => None,
                };
                let counts = [Some(scatter.x), Some(scatter.y), scatter.z, size, color];
                self.check_equal_counts(node, "the coordinate, size and colour arrays", &counts)
            }
            Artist::Quiver(quiver) => {
                let counts = [
                    Some(quiver.x),
                    Some(quiver.y),
                    quiver.z,
                    Some(quiver.u),
                    Some(quiver.v),
                    quiver.w,
                ];
                self.check_equal_counts(node, "the position and component arrays", &counts)
            }
            Artist::Contour(contour) => self.check_grid(node, contour.grid, contour.z),
            Artist::Surface(surface) => {
                let grid_agrees = self.check_grid(node, surface.grid, surface.z);
                let mut agree = true;
                for (what, array) in [
                    ("the colour data", surface.c),
                    ("the face alpha", surface.face_alpha),
                    ("the edge alpha", surface.edge_alpha),
                ] {
                    if let Some(array) = array
                        && data[&array].shape != data[&surface.z].shape
                    {
                        self.error(
                            Some(node),
                            IssueKind::ShapeMismatch,
                            format!(
                                "{what} has shape {:?}, but the field has shape {:?}",
                                data[&array].shape, data[&surface.z].shape
                            ),
                        );
                        agree = false;
                    }
                }
                grid_agrees && agree
            }
            // The shape of an image is checked by `check_image`, which also needs the
            // number of pixels along each axis of the image.
            Artist::Image(_) | Artist::IndexedImage(_) | Artist::MappedImage(_) => true,
        }
    }

    /// Warns of a legend panel that has no entries to list, because no axes names it or
    /// no axes that names it has an artist with a display name.
    fn check_panel_has_entries(&mut self, panel: NodeId, entries: &LegendEntries) {
        if let LegendEntries::Explicit(items) = entries {
            if items.is_empty() {
                self.warning(
                    Some(panel),
                    IssueKind::NothingToDraw,
                    "the legend panel has an explicit list of no items, so the panel is empty"
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
            "no axes lists its entries in the legend panel, so the panel is empty"
        } else if axes.any(|axes| {
            axes.artists
                .iter()
                .any(|artist| artist.display_name().is_some())
        }) {
            return;
        } else {
            "no artist of the axes that list their entries in the legend panel has a display \
             name, so the panel is empty"
        };
        self.warning(Some(panel), IssueKind::NothingToDraw, message.to_string());
    }

    /// Warns of an artist other than an image whose arrays, which exist and agree in
    /// shape, give it nothing to draw: a line, scatter or quiver of no points, or a
    /// contour or surface whose field has no rows or no columns or a single row or a
    /// single column, between whose nodes there is no cell.
    fn check_something_to_draw(&mut self, node: NodeId, artist: &Artist) {
        let data = &self.figure.data;
        let (what, shape) = match artist {
            Artist::Line(line) => ("the coordinate arrays", &data[&line.x].shape),
            Artist::Scatter(scatter) => ("the coordinate arrays", &data[&scatter.x].shape),
            Artist::Quiver(quiver) => ("the position arrays", &data[&quiver.x].shape),
            Artist::Contour(contour) => ("the field", &data[&contour.z].shape),
            Artist::Surface(surface) => ("the field", &data[&surface.z].shape),
            Artist::Image(_) | Artist::IndexedImage(_) | Artist::MappedImage(_) => return,
        };
        let message = match artist {
            Artist::Contour(_) | Artist::Surface(_) => {
                let &[ny, nx] = shape.as_slice() else {
                    return;
                };
                let Some(case) = nodes_without_cells(ny, nx) else {
                    return;
                };
                format!(
                    "{what} has shape {shape:?}: it {case}, and a contour or surface draws the \
                     cells between the nodes of its grid, of which there is none, so the artist \
                     is left out of the drawing"
                )
            }
            _ => {
                if shape.iter().product::<usize>() > 0 {
                    return;
                }
                format!(
                    "{what} have shape {shape:?}, so the artist has no points and is left out \
                     of the drawing"
                )
            }
        };
        self.warning(Some(node), IssueKind::NothingToDraw, message);
    }

    /// Checks an image: the shape of its array when the array is valid, its placement,
    /// the scales of the axes of its plane, the pixels that its strict policies cover,
    /// and, when no error was reported against it since `errors_before` errors were
    /// reported, whether it has any pixel to draw.
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
        if let Some(policies) = &image.policies {
            self.check_strict_policies(axes, node, image, policies);
        }
        if self.report.errors.len() > errors_before {
            return;
        }
        let case = match (ny, nx) {
            (0, 0) => "no rows and no columns",
            (0, _) => "no rows",
            (_, 0) => "no columns",
            _ => return,
        };
        let shape = &self.figure.data[&image.data].shape;
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
        let shape = &self.figure.data[&image.data].shape;
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
        let shape = &self.figure.data[&alpha].shape;
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

    /// Reports the pixels of a colour-indexed or colour-mapped image that fall in a
    /// category whose policy is strict; the array of the image is valid.
    fn check_strict_policies(
        &mut self,
        axes: &Axes,
        node: NodeId,
        image: &ImageView,
        policies: &Policies,
    ) {
        let array = &self.figure.data[&image.data];
        // The range outside which a pixel lies below or above, and how such a pixel is
        // described.
        let (range, below, above) = match policies.range {
            Range::Colormap => (
                Some((0.0, 255.0)),
                "less than 0 after truncation toward zero".to_owned(),
                "greater than 255 after truncation toward zero".to_owned(),
            ),
            Range::ColourLimits => match axes.clim {
                Limits::Manual { min, max } if min.is_finite() && max.is_finite() && min < max => (
                    Some((min, max)),
                    format!("less than the lower colour limit {min}"),
                    format!("greater than the upper colour limit {max}"),
                ),
                _ => (None, String::new(), String::new()),
            },
        };
        let truncate = policies.range == Range::Colormap;
        let mut counts = [0usize; 3];
        for_each_value(array, |value| {
            if !value.is_finite() {
                counts[2] += 1;
                return;
            }
            let Some((min, max)) = range else {
                return;
            };
            let value = if truncate { value.trunc() } else { value };
            if value < min {
                counts[0] += 1;
            } else if value > max {
                counts[1] += 1;
            }
        });
        let categories = [
            ("below", policies.below, below),
            ("above", policies.above, above),
            ("non_finite", policies.non_finite, "not finite".to_owned()),
        ];
        for ((name, policy, condition), count) in categories.into_iter().zip(counts) {
            if policy == OutOfRange::Strict && count > 0 {
                self.error(
                    Some(node),
                    IssueKind::PixelOutOfRange,
                    format!(
                        "the {name} policy is strict, but {count} of the {} in {} are {condition}",
                        image.what, image.data
                    ),
                );
            }
        }
    }

    /// Reports a shape mismatch when the present arrays differ in element count, and
    /// returns whether they agree.
    fn check_equal_counts(&mut self, node: NodeId, what: &str, arrays: &[Option<DataId>]) -> bool {
        let counts: Vec<usize> = arrays
            .iter()
            .flatten()
            .map(|id| self.figure.data[id].len())
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
    fn check_grid(&mut self, node: NodeId, grid: Grid, field: DataId) -> bool {
        let data = &self.figure.data;
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

    /// Checks the contour levels of a contour.
    fn check_levels(&mut self, contour: &Contour) {
        let problem = match &contour.levels {
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
            self.error(
                Some(contour.id),
                IssueKind::InvalidLevels,
                message.to_owned(),
            );
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

/// One of the three image kinds, seen through what validation needs of it.
struct ImageView<'a> {
    /// The pixels, indices or values of the image.
    data: DataId,
    /// The name of the data field, for messages.
    what: &'static str,
    /// Whether the array holds the colour components of every pixel along a third
    /// dimension, as the pixels of a true-colour image do.
    channels: bool,
    /// The alpha of every pixel of a colour-indexed or colour-mapped image, if it has
    /// one.
    alpha: Option<DataId>,
    /// Where the pixels lie in the axes.
    placement: &'a ImagePlacement,
    /// The out-of-range policies of a colour-indexed or colour-mapped image, or `None`
    /// for a true-colour image, which has none.
    policies: Option<Policies>,
}

/// The out-of-range policies of a colour-indexed or colour-mapped image, with the range
/// outside which its pixels fall in the below and above categories.
struct Policies {
    below: OutOfRange,
    above: OutOfRange,
    non_finite: OutOfRange,
    range: Range,
}

/// The range of an image kind, outside which its pixels fall in the below and above
/// categories.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Range {
    /// The entries 0 to 255 of the colormap, in which an index truncated toward zero is
    /// looked up.
    Colormap,
    /// The colour limits of the axes, when they are manual and valid. No value lies
    /// outside automatic limits, which are the range of the data, and invalid manual
    /// limits are reported against the axes rather than against every pixel.
    ColourLimits,
}

impl<'a> ImageView<'a> {
    fn of(artist: &'a Artist) -> Option<Self> {
        Some(match artist {
            Artist::Image(image) => Self {
                data: image.pixels,
                what: "pixels",
                channels: true,
                alpha: None,
                placement: &image.placement,
                policies: None,
            },
            Artist::IndexedImage(image) => Self {
                data: image.indices,
                what: "indices",
                channels: false,
                alpha: image.alpha,
                placement: &image.placement,
                policies: Some(Policies {
                    below: image.below,
                    above: image.above,
                    non_finite: image.non_finite,
                    range: Range::Colormap,
                }),
            },
            Artist::MappedImage(image) => Self {
                data: image.values,
                what: "values",
                channels: false,
                alpha: image.alpha,
                placement: &image.placement,
                policies: Some(Policies {
                    below: image.below,
                    above: image.above,
                    non_finite: image.non_finite,
                    range: Range::ColourLimits,
                }),
            },
            Artist::Line(_)
            | Artist::Scatter(_)
            | Artist::Contour(_)
            | Artist::Quiver(_)
            | Artist::Surface(_) => return None,
        })
    }
}

/// How an artist uses its data and the dimensions of its axes.
struct ArtistUsage {
    /// Every data identifier the artist refers to.
    references: Vec<DataId>,
    /// The arrays whose values are plotted as positions along a dimension.
    positions: Vec<(Dimension, DataId)>,
    /// Whether the artist can only be drawn in a three-dimensional axes, because it needs
    /// the z axis to be placed. A surface does not: seen from directly above, it is the
    /// pseudocolour plot of a two-dimensional axes.
    three_d: bool,
    /// Whether the artist can use arrays of 8-bit values, as the image kinds can.
    accepts_bytes: bool,
    /// Whether the x and y values of a line or scatter are fractions of the plot area
    /// rather than data.
    fraction: [bool; 2],
}

impl ArtistUsage {
    /// The usage of an image, which refers to its pixels, indices or values and to its
    /// alpha, if it has one, each of either element type, plots nothing as a position
    /// along an axis, and needs a three-dimensional axes when it lies on a wall.
    fn image(data: DataId, alpha: Option<DataId>, placement: &ImagePlacement) -> Self {
        Self {
            references: [Some(data), alpha].into_iter().flatten().collect(),
            positions: Vec::new(),
            three_d: !matches!(placement.plane, ImagePlane::Xy { .. }),
            accepts_bytes: true,
            fraction: [false; 2],
        }
    }

    /// Returns whether the values along `dimension` are fractions of the plot area.
    fn is_fraction(&self, dimension: Dimension) -> bool {
        match dimension {
            Dimension::X => self.fraction[0],
            Dimension::Y => self.fraction[1],
            Dimension::Z => false,
        }
    }

    fn of(artist: &Artist) -> Self {
        let grid_positions = |grid: Grid| match grid {
            Grid::Rectilinear { x, y } | Grid::Curvilinear { x, y } => {
                vec![(Dimension::X, x), (Dimension::Y, y)]
            }
        };
        match artist {
            Artist::Line(line) => {
                let mut positions = vec![(Dimension::X, line.x), (Dimension::Y, line.y)];
                positions.extend(line.z.map(|z| (Dimension::Z, z)));
                Self {
                    references: positions.iter().map(|&(_, id)| id).collect(),
                    positions,
                    three_d: line.z.is_some(),
                    accepts_bytes: false,
                    fraction: fractions(line.x_coords, line.y_coords),
                }
            }
            Artist::Scatter(scatter) => {
                let mut positions = vec![(Dimension::X, scatter.x), (Dimension::Y, scatter.y)];
                positions.extend(scatter.z.map(|z| (Dimension::Z, z)));
                let mut references: Vec<DataId> = positions.iter().map(|&(_, id)| id).collect();
                if let ScatterSize::Data { data } = scatter.size {
                    references.push(data);
                }
                if let ScatterColor::Data { data } = scatter.color {
                    references.push(data);
                }
                Self {
                    references,
                    positions,
                    three_d: scatter.z.is_some(),
                    accepts_bytes: false,
                    fraction: fractions(scatter.x_coords, scatter.y_coords),
                }
            }
            Artist::Quiver(quiver) => {
                let mut positions = vec![(Dimension::X, quiver.x), (Dimension::Y, quiver.y)];
                positions.extend(quiver.z.map(|z| (Dimension::Z, z)));
                let mut references: Vec<DataId> = positions.iter().map(|&(_, id)| id).collect();
                references.extend([quiver.u, quiver.v]);
                references.extend(quiver.w);
                Self {
                    references,
                    positions,
                    three_d: quiver.z.is_some() || quiver.w.is_some(),
                    accepts_bytes: false,
                    fraction: [false; 2],
                }
            }
            Artist::Contour(contour) => {
                let at_level = contour.placement == ContourPlacement::AtLevel;
                let mut positions = grid_positions(contour.grid);
                let mut references: Vec<DataId> = positions.iter().map(|&(_, id)| id).collect();
                references.push(contour.z);
                if at_level {
                    positions.push((Dimension::Z, contour.z));
                }
                Self {
                    references,
                    positions,
                    three_d: at_level,
                    accepts_bytes: false,
                    fraction: [false; 2],
                }
            }
            Artist::Surface(surface) => {
                let mut positions = grid_positions(surface.grid);
                positions.push((Dimension::Z, surface.z));
                let mut references: Vec<DataId> = positions.iter().map(|&(_, id)| id).collect();
                references.extend(surface.c);
                references.extend(surface.face_alpha);
                references.extend(surface.edge_alpha);
                // A surface is valid in either projection. In a two-dimensional axes it is
                // seen from directly above, so its field positions nothing there; the
                // caller passes over positions along z in a two-dimensional axes.
                Self {
                    references,
                    positions,
                    three_d: false,
                    accepts_bytes: false,
                    fraction: [false; 2],
                }
            }
            Artist::Image(image) => Self::image(image.pixels, None, &image.placement),
            Artist::IndexedImage(image) => {
                Self::image(image.indices, image.alpha, &image.placement)
            }
            Artist::MappedImage(image) => Self::image(image.values, image.alpha, &image.placement),
        }
    }
}

/// Returns whether the x and y values of a line or scatter are fractions of the plot area.
fn fractions(x: Coords, y: Coords) -> [bool; 2] {
    [x == Coords::AxesFraction, y == Coords::AxesFraction]
}
