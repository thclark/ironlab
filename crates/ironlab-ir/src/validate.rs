//! Structural validation of figures.

use std::collections::{BTreeMap, BTreeSet};

use crate::artist::{
    Artist, Contour, ContourPlacement, Grid, ImagePlacement, ImagePlane, Levels, OutOfRange,
    PixelRange, ScatterColor, ScatterSize,
};
use crate::axes::{Axes, Axis, Limits, Projection, Scale};
use crate::data::{NdArray, NdArrayElement, Values};
use crate::figure::{Figure, Parameter};
use crate::ids::{DataId, NodeId};
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
    /// cannot plot an array of 8-bit values. The three image kinds accept either
    /// element type.
    ElementTypeMismatch,
    /// An artist or placement that needs the z axis (z or w data, contours at their
    /// level, an image on a wall) is used in a two-dimensional axes. A surface is not
    /// such an artist: in a two-dimensional axes it is seen from directly above.
    ThreeDArtistInTwoDAxes,
    /// An axis link refers to an identifier that is not an axes of the figure.
    DanglingLink,
    /// Two nodes share an identifier.
    DuplicateNodeId,
    /// An axes cell lies outside the figure's tile layout or has a zero span.
    CellOutOfLayout,
    /// The figure's width, height or base font size is not a finite positive number.
    InvalidSize,
    /// Manual limits (of a coordinate axis or of the colour limits) are not finite,
    /// are not strictly increasing, or are not positive on a logarithmic axis.
    InvalidLimits,
    /// Contour levels are empty, not finite or not strictly increasing, or an
    /// automatic level count is zero.
    InvalidLevels,
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
    /// that an artist reported for an error is not reported twice.
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
    /// plane is not finite, or the centres of the first and last pixels along an axis
    /// coincide although the image has more than one pixel along that axis.
    InvalidImagePlacement,
    /// A pixel of a colour-indexed or colour-mapped image falls in a category whose
    /// out-of-range policy is strict: an index outside the colormap, a value outside
    /// manual colour limits, or an index or value that is not finite.
    PixelOutOfRange,
    /// An image lies in a plane of which an axis is logarithmic, on which a raster of
    /// flat pixels cannot be placed, or its plane is offset to a non-positive
    /// coordinate along a logarithmic third axis, where the plane cannot be placed, so
    /// the image is not drawn.
    ImageOnLogAxis,
}

impl Figure {
    /// Checks the figure for structural problems.
    ///
    /// Errors report unknown data identifiers, arrays whose values do not match their
    /// shape, arrays of 8-bit values referenced by an artist other than an image (every
    /// other artist requires 64-bit floating-point values), inconsistent array lengths
    /// and shapes within an artist (for an image, pixels that are not an array of shape
    /// `[ny, nx, 3]` or `[ny, nx, 4]`, or indices or values that are not two-dimensional),
    /// 3D artists in 2D axes (an image on the xz or yz plane among them), links to
    /// identifiers that are not axes, duplicate node identifiers, cells outside the tile
    /// layout, a non-positive figure size or font size, invalid manual limits, invalid
    /// contour levels, parameters with an empty name or a non-finite number, labels that
    /// are empty or repeated, an image
    /// placement whose pixel centres or plane offset are not finite or whose first and
    /// last centres coincide along an axis of more than one pixel, and pixels of a
    /// colour-indexed or colour-mapped image that fall in a category whose out-of-range
    /// policy is strict (for a colour-mapped image, a value lies below or above the range
    /// only when the colour limits are manual and valid, because automatic limits are
    /// the range of the data). Warnings report finite non-positive data plotted along
    /// logarithmic axes, an image whose plane has a logarithmic axis, which is not
    /// drawn, and an artist whose data gives it nothing to draw, which is left out (a
    /// line, scatter or quiver of no points, an image with no rows or no columns, and a
    /// contour or surface whose field has no rows or no columns or a single row or
    /// column, which has no cell between its nodes); data that is not plotted along an
    /// axis (such as quiver components, colour data or the pixels of an image) never
    /// produces the first of these. An array of 8-bit values that no artist refers to
    /// is not an error.
    ///
    /// The z limits and z scale of a 2D axes are ignored, as they are when drawing, and
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

    /// Checks the cell and limits of an axes.
    fn check_axes(&mut self, axes: &Axes) {
        let (cell, layout) = (axes.cell, self.figure.layout);
        let outside = cell.row_span == 0
            || cell.col_span == 0
            || u64::from(cell.row) + u64::from(cell.row_span) > u64::from(layout.rows)
            || u64::from(cell.col) + u64::from(cell.col_span) > u64::from(layout.cols);
        if outside {
            self.error(
                Some(axes.id),
                IssueKind::CellOutOfLayout,
                format!(
                    "the cell {cell:?} does not lie within the {} × {} tile layout",
                    layout.rows, layout.cols
                ),
            );
        }

        let three_d = matches!(axes.projection, Projection::ThreeD { .. });
        let mut limits = vec![
            ("x limits", axes.x.limits, axes.x.scale),
            ("y limits", axes.y.limits, axes.y.scale),
        ];
        if three_d {
            limits.push(("z limits", axes.z.limits, axes.z.scale));
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

        if let Artist::Contour(contour) = artist {
            self.check_levels(contour);
        }

        let references_are_valid =
            self.check_references(id, &usage.references, usage.accepts_bytes);
        match ImageView::of(artist) {
            Some(image) => self.check_image(axes, three_d, id, &image, references_are_valid),
            None if references_are_valid && self.check_shapes(id, artist) => {
                self.check_something_to_draw(id, artist);
            }
            None => {}
        }

        for (dimension, data) in usage.positions {
            if dimension == Dimension::Z && !three_d {
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
                if let Some(c) = surface.c
                    && data[&c].shape != data[&surface.z].shape
                {
                    self.error(
                        Some(node),
                        IssueKind::ShapeMismatch,
                        format!(
                            "the colour data has shape {:?}, but the field has shape {:?}",
                            data[&c].shape, data[&surface.z].shape
                        ),
                    );
                    return false;
                }
                grid_agrees
            }
            // The shape of an image is checked by `check_image`, which also needs the
            // number of pixels along each axis of the image.
            Artist::Image(_) | Artist::IndexedImage(_) | Artist::MappedImage(_) => true,
        }
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
    /// and, when no error was reported against it, whether it has any pixel to draw.
    fn check_image(
        &mut self,
        axes: &Axes,
        three_d: bool,
        node: NodeId,
        image: &ImageView,
        data_is_valid: bool,
    ) {
        let errors_before = self.report.errors.len();
        let pixels = if data_is_valid {
            self.check_image_shape(node, image)
        } else {
            None
        };
        self.check_image_placement(node, image.placement, pixels);
        self.check_image_plane_scales(axes, three_d, node, image.placement.plane);
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

    /// Warns of an image whose plane has a logarithmic axis, on which a raster of flat
    /// pixels cannot be placed, or whose plane is offset to a non-positive coordinate
    /// along a logarithmic third axis, where the plane cannot be placed, so that the
    /// image is not drawn; the z axis of a 2D axes is ignored, as it is when drawing.
    fn check_image_plane_scales(
        &mut self,
        axes: &Axes,
        three_d: bool,
        node: NodeId,
        plane: ImagePlane,
    ) {
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
                self.warning(
                    Some(node),
                    IssueKind::ImageOnLogAxis,
                    format!(
                        "the plane of the image is offset to {offset} along the logarithmic {} \
                         axis, where it cannot be placed, so the image is not drawn",
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
        self.warning(
            Some(node),
            IssueKind::ImageOnLogAxis,
            format!(
                "the image lies in the {} plane, whose {} {noun} {verb} logarithmic, and a \
                 raster of flat pixels cannot be placed on a logarithmic axis, so the image \
                 is not drawn",
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
                placement: &image.placement,
                policies: None,
            },
            Artist::IndexedImage(image) => Self {
                data: image.indices,
                what: "indices",
                channels: false,
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
}

impl ArtistUsage {
    /// The usage of an image, which refers to one array of either element type, plots
    /// nothing as a position along an axis, and needs a three-dimensional axes when it
    /// lies on a wall.
    fn image(data: DataId, placement: &ImagePlacement) -> Self {
        Self {
            references: vec![data],
            positions: Vec::new(),
            three_d: !matches!(placement.plane, ImagePlane::Xy { .. }),
            accepts_bytes: true,
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
                }
            }
            Artist::Surface(surface) => {
                let mut positions = grid_positions(surface.grid);
                positions.push((Dimension::Z, surface.z));
                let mut references: Vec<DataId> = positions.iter().map(|&(_, id)| id).collect();
                references.extend(surface.c);
                // A surface is valid in either projection. In a two-dimensional axes it is
                // seen from directly above, so its field positions nothing there; the
                // caller passes over positions along z in a two-dimensional axes.
                Self {
                    references,
                    positions,
                    three_d: false,
                    accepts_bytes: false,
                }
            }
            Artist::Image(image) => Self::image(image.pixels, &image.placement),
            Artist::IndexedImage(image) => Self::image(image.indices, &image.placement),
            Artist::MappedImage(image) => Self::image(image.values, &image.placement),
        }
    }
}
