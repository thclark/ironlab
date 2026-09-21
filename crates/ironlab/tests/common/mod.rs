//! Helpers that look up the IR produced by the facade, so that tests assert on the
//! figure description rather than on rendering.

#![allow(dead_code)]

use ironlab::ir::{self, Artist, Axes, DataId, IssueKind, NdArray, NodeId, ValidationReport};
use ironlab::{ByteMatrix, Color, Figure, ImagePlane, Matrix, OutOfRange, Pixels};

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

pub fn image(fig: &Figure, id: NodeId) -> &ir::Image {
    match artist(fig, id) {
        Artist::Image(a) => a,
        other => panic!("expected an image, found {other:?}"),
    }
}

pub fn indexed_image(fig: &Figure, id: NodeId) -> &ir::IndexedImage {
    match artist(fig, id) {
        Artist::IndexedImage(a) => a,
        other => panic!("expected an indexed image, found {other:?}"),
    }
}

pub fn mapped_image(fig: &Figure, id: NodeId) -> &ir::MappedImage {
    match artist(fig, id) {
        Artist::MappedImage(a) => a,
        other => panic!("expected a mapped image, found {other:?}"),
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
pub fn small_grid() -> (Vec<f64>, Vec<f64>, Matrix) {
    let x = vec![0.0, 1.0, 2.0, 3.0];
    let y = vec![10.0, 20.0, 30.0];
    let z = Matrix::from_fn(3, 4, |row, col| (row * 10 + col) as f64);
    (x, y, z)
}

/// A valid figure holding one image of each kind, built through the facade: a 2 by 2
/// true-colour image of RGBA bytes with mirrored rows and a display name; a 2 by 3
/// colour-indexed image of bytes with explicit column centres and strict and clamped
/// policies; and a 3 by 2 colour-mapped image of floats holding a NaN, on the xz wall
/// of a three-dimensional axes at an offset, with a fixed colour for its non-finite
/// pixels and manual colour limits.
pub fn image_figure() -> Figure {
    let pixels = Pixels::from_rgba8(
        2,
        2,
        &[255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 1, 2, 3, 4],
    )
    .unwrap();
    let indices = ByteMatrix::from_fn(2, 3, |row, col| (row * 3 + col) as u8);
    let values = Matrix::from_fn(3, 2, |row, col| {
        if (row, col) == (1, 1) {
            f64::NAN
        } else {
            (row * 2 + col) as f64 / 5.0
        }
    });
    let mut fig = Figure::new().tiles(1, 3).title("Images");
    fig.axes(0, 0)
        .image(&pixels)
        .pixel_columns(-1.0, 1.0)
        .pixel_rows(1.0, -1.0)
        .display_name("photo");
    fig.axes(0, 1)
        .indexed_image(&indices)
        .pixel_columns(0.5, 2.5)
        .below(OutOfRange::Strict)
        .above(OutOfRange::Clamp);
    fig.axes(0, 2)
        .mapped_image(&values)
        .plane(ImagePlane::Xz { y: Some(0.5) })
        // A multiple of 1/255, so that the eight-bit colour of the JSON form reloads exactly.
        .non_finite(Color::rgba(0.0, 0.0, 0.0, 128.0 / 255.0));
    fig.axes(0, 2).clim(0.0, 1.0);
    fig
}
