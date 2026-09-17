//! Helpers that look up the IR produced by the facade, so that tests assert on the
//! figure description rather than on rendering.

#![allow(dead_code)]

use ironlab::Figure;
use ironlab::ir::{self, Artist, Axes, DataId, IssueKind, NdArray, NodeId, ValidationReport};

/// Returns the axes with the given identifier, panicking if it does not exist.
pub fn axes(fig: &Figure, id: NodeId) -> &Axes {
    fig.ir()
        .axes(id)
        .unwrap_or_else(|| panic!("{id} is not an axes"))
}

/// Returns the artist with the given identifier, panicking if it does not exist.
pub fn artist(fig: &Figure, id: NodeId) -> &Artist {
    fig.ir()
        .artist(id)
        .map(|(_, artist)| artist)
        .unwrap_or_else(|| panic!("{id} is not an artist"))
}

/// Returns the axes that contains the given artist.
pub fn parent(fig: &Figure, id: NodeId) -> &Axes {
    fig.ir()
        .artist(id)
        .map(|(axes, _)| axes)
        .unwrap_or_else(|| panic!("{id} is not an artist"))
}

pub fn line(fig: &Figure, id: NodeId) -> &ir::Line {
    match artist(fig, id) {
        Artist::Line(a) => a,
        other => panic!("expected a line, found {other:?}"),
    }
}

pub fn scatter(fig: &Figure, id: NodeId) -> &ir::Scatter {
    match artist(fig, id) {
        Artist::Scatter(a) => a,
        other => panic!("expected a scatter, found {other:?}"),
    }
}

pub fn contour(fig: &Figure, id: NodeId) -> &ir::Contour {
    match artist(fig, id) {
        Artist::Contour(a) => a,
        other => panic!("expected a contour, found {other:?}"),
    }
}

pub fn quiver(fig: &Figure, id: NodeId) -> &ir::Quiver {
    match artist(fig, id) {
        Artist::Quiver(a) => a,
        other => panic!("expected a quiver, found {other:?}"),
    }
}

pub fn surface(fig: &Figure, id: NodeId) -> &ir::Surface {
    match artist(fig, id) {
        Artist::Surface(a) => a,
        other => panic!("expected a surface, found {other:?}"),
    }
}

/// Returns the array with the given data identifier, panicking if it does not exist.
pub fn data(fig: &Figure, id: DataId) -> &NdArray {
    fig.ir()
        .data
        .get(&id)
        .unwrap_or_else(|| panic!("{id} is not in the data table"))
}

/// Returns true when the axes is three-dimensional.
pub fn is_3d(axes: &Axes) -> bool {
    matches!(axes.projection, ir::Projection::ThreeD { .. })
}

/// Returns true when the report has an error of the given kind.
pub fn has_error(report: &ValidationReport, kind: IssueKind) -> bool {
    report.errors.iter().any(|issue| issue.kind == kind)
}

/// Returns true when the report has an error of the given kind at the given node.
pub fn has_error_at(report: &ValidationReport, kind: IssueKind, node: NodeId) -> bool {
    report
        .errors
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

/// A 3 by 4 field (3 rows of y, 4 columns of x) with distinct values, and matching
/// coordinate vectors.
pub fn small_grid() -> (Vec<f64>, Vec<f64>, ironlab::Matrix) {
    let x = vec![0.0, 1.0, 2.0, 3.0];
    let y = vec![10.0, 20.0, 30.0];
    let z = ironlab::Matrix::from_fn(3, 4, |row, col| (row * 10 + col) as f64);
    (x, y, z)
}
