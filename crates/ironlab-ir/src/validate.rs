//! Structural validation of figures.

use std::collections::BTreeMap;

use crate::artist::{Artist, Contour, ContourPlacement, Grid, Levels, ScatterColor, ScatterSize};
use crate::axes::{Axes, Axis, Limits, Projection, Scale};
use crate::data::NdArray;
use crate::figure::Figure;
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
    /// A three-dimensional artist or placement is used in a two-dimensional axes.
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
}

impl Figure {
    /// Checks the figure for structural problems.
    ///
    /// Errors report unknown data identifiers, arrays whose values do not match their
    /// shape, inconsistent array lengths and shapes within an artist, 3D artists in 2D
    /// axes, links to identifiers that are not axes, duplicate node identifiers,
    /// cells outside the tile layout, a non-positive figure size or font size, invalid
    /// manual limits and invalid contour levels. Warnings report finite non-positive
    /// data plotted along logarithmic axes; data that is not plotted along an axis
    /// (such as quiver components or colour data) never produces this warning.
    ///
    /// The z limits and z scale of a 2D axes are ignored, as they are when drawing.
    /// A line, scatter or quiver without z data in a 3D axes is not an error: it is
    /// drawn in the plane z = 0. Shape checks are skipped for an artist that refers
    /// to unknown or invalid arrays, so that one problem is not reported repeatedly.
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

    /// Checks the figure-level properties, the data table, node identifiers and links.
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

        let references_are_valid = self.check_references(id, &usage.references);
        if references_are_valid {
            self.check_shapes(id, artist);
        }

        for (dimension, data) in usage.positions {
            if dimension == Dimension::Z && !three_d {
                continue;
            }
            let axis: &Axis = match dimension {
                Dimension::X => &axes.x,
                Dimension::Y => &axes.y,
                Dimension::Z => &axes.z,
            };
            if axis.scale != Scale::Log {
                continue;
            }
            let Some(array) = self.figure.data.get(&data) else {
                continue;
            };
            if array.values.iter().any(|&v| v.is_finite() && v <= 0.0) {
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

    /// Reports each distinct unknown data reference of an artist, and returns whether
    /// every reference is to a known array whose values match its shape.
    fn check_references(&mut self, node: NodeId, references: &[DataId]) -> bool {
        let mut valid = true;
        let mut reported: Vec<DataId> = Vec::new();
        for &data in references {
            match self.figure.data.get(&data) {
                Some(array) => valid &= is_consistent(array),
                None => {
                    valid = false;
                    if !reported.contains(&data) {
                        reported.push(data);
                        self.error(
                            Some(node),
                            IssueKind::UnknownData,
                            format!("{data} is not in the data table"),
                        );
                    }
                }
            }
        }
        valid
    }

    /// Checks the lengths and shapes of an artist's arrays, all of which exist.
    fn check_shapes(&mut self, node: NodeId, artist: &Artist) {
        let data = &self.figure.data;
        match artist {
            Artist::Line(line) => {
                let counts = [Some(line.x), Some(line.y), line.z];
                self.check_equal_counts(node, "the coordinate arrays", &counts);
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
                self.check_equal_counts(node, "the coordinate, size and colour arrays", &counts);
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
                self.check_equal_counts(node, "the position and component arrays", &counts);
            }
            Artist::Contour(contour) => self.check_grid(node, contour.grid, contour.z),
            Artist::Surface(surface) => {
                self.check_grid(node, surface.grid, surface.z);
                if let Some(c) = surface.c
                    && data[&c].shape != data[&surface.z].shape
                {
                    self.error(
                        Some(node),
                        IssueKind::ShapeMismatch,
                        format!(
                            "the colour data has shape {:?}, but the heights have shape {:?}",
                            data[&c].shape, data[&surface.z].shape
                        ),
                    );
                }
            }
        }
    }

    /// Reports a shape mismatch when the present arrays differ in element count.
    fn check_equal_counts(&mut self, node: NodeId, what: &str, arrays: &[Option<DataId>]) {
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
        }
    }

    /// Checks that a field is two-dimensional and that its grid matches it.
    fn check_grid(&mut self, node: NodeId, grid: Grid, field: DataId) {
        let data = &self.figure.data;
        let shape = &data[&field].shape;
        let &[ny, nx] = shape.as_slice() else {
            self.error(
                Some(node),
                IssueKind::ShapeMismatch,
                format!("the field has shape {shape:?}, but it must be two-dimensional"),
            );
            return;
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
        }
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

/// How an artist uses its data and the dimensions of its axes.
struct ArtistUsage {
    /// Every data identifier the artist refers to.
    references: Vec<DataId>,
    /// The arrays whose values are plotted as positions along a dimension.
    positions: Vec<(Dimension, DataId)>,
    /// Whether the artist can only be drawn in a three-dimensional axes.
    three_d: bool,
}

impl ArtistUsage {
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
                }
            }
            Artist::Surface(surface) => {
                let mut positions = grid_positions(surface.grid);
                positions.push((Dimension::Z, surface.z));
                let mut references: Vec<DataId> = positions.iter().map(|&(_, id)| id).collect();
                references.extend(surface.c);
                Self {
                    references,
                    positions,
                    three_d: true,
                }
            }
        }
    }
}
