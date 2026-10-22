//! Helpers that look up the IR produced by the facade, so that tests assert on the
//! figure description rather than on rendering.

#![allow(dead_code)]

use ironlab::ir::{self, Artist, Axes, IssueKind, NdArray, NodeId, ValidationReport};
use ironlab::{
    ByteMatrix, Color, Colormap, ColormapSample, Figure, ImagePlane, Matrix, OutOfRange, Pixels,
};

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

pub fn series(fig: &Figure, id: NodeId) -> &ir::Series {
    match artist(fig, id) {
        Artist::Series(a) => a,
        other => panic!("expected a series, found {other:?}"),
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

pub fn scalar_image(fig: &Figure, id: NodeId) -> &ir::ScalarImage {
    match artist(fig, id) {
        Artist::ScalarImage(a) => a,
        other => panic!("expected a scalar image, found {other:?}"),
    }
}

/// Returns the array of the data array with the given identifier, panicking if it does
/// not exist.
pub fn data(fig: &Figure, id: NodeId) -> &NdArray {
    &fig.ir()
        .data_array(id)
        .unwrap_or_else(|| panic!("{id} is not a data array"))
        .array
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
/// true-colour image of RGBA bytes with mirrored rows and a label; a 2 by 3 scalar image
/// of bytes with explicit column centres, through an indexed scale of six entries whose
/// out-of-range policy is a clamp; and a 3 by 2 scalar image of floats holding a NaN, on
/// the xz wall of a three-dimensional axes at an offset, through the axes' default scale
/// with manual limits and a fixed colour for non-finite values.
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
        .label("photo");
    let palette = fig
        .indexed_scale(Colormap::Viridis.sample(6))
        .outside(OutOfRange::Clamp)
        .id();
    fig.axes(0, 1)
        .scalar_image(&indices)
        .pixel_columns(0.5, 2.5)
        .color_scale(palette);
    fig.axes(0, 2)
        .scalar_image(&values)
        .plane(ImagePlane::Xz { y: Some(0.5) });
    fig.axes(0, 2)
        .color_scale()
        .limits(0.0, 1.0)
        // A multiple of 1/255, so that the eight-bit colour of the JSON form reloads exactly.
        .non_finite(Color::rgba(0.0, 0.0, 0.0, 128.0 / 255.0));
    fig
}
