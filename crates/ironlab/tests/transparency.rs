//! Transparency: the alpha arrays and opacities of surfaces and images, and the opacities
//! of contours.

mod common;

use common::{contour, data, has_error_at, image, scalar_image, small_grid, surface};
use ironlab::ir::{IssueKind, NdArray, NdArrayElement};
use ironlab::prelude::*;

// WHY: the facade is how most figures are written, so each setter must reach the IR
// field that the renderer reads, the faces and the edges of a surface and the isolines and
// the bands of a contour independently, and
// an opacity left alone must stay at the opaque default rather than at a zero that would
// hide the artist.
#[test]
fn opacity_setters_reach_their_ir_fields_and_default_to_opaque() {
    let (x, y, z) = small_grid();
    let pixels = Pixels::from_rgb8(1, 1, &[1, 2, 3]).expect("one pixel");
    let floats = Matrix::from_fn(2, 2, |row, col| (row + col) as f64);
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let untouched = ax.surf(&x, &y, &z).id();
    let faded = ax
        .surf(&x, &y, &z)
        .face_opacity(0.25)
        .edge_opacity(0.75)
        .id();
    let true_colour = ax.image(&pixels).opacity(0.5).id();
    let indexed = ax.scalar_image(&floats).opacity(0.125).id();
    let mapped = ax.scalar_image(&floats).opacity(0.0).id();
    let plain = ax.scalar_image(&floats).id();
    let plain_contour = ax.contourf(&x, &y, &z).id();
    let faded_contour = ax
        .contourf(&x, &y, &z)
        .line_opacity(0.0)
        .fill_opacity(0.5)
        .id();

    let s = surface(&fig, untouched);
    assert_eq!((s.face_opacity, s.edge_opacity), (1.0, 1.0));
    let s = surface(&fig, faded);
    assert_eq!((s.face_opacity, s.edge_opacity), (0.25, 0.75));
    assert_eq!(image(&fig, true_colour).opacity, 0.5);
    assert_eq!(scalar_image(&fig, indexed).opacity, 0.125);
    assert_eq!(scalar_image(&fig, mapped).opacity, 0.0);
    assert_eq!(scalar_image(&fig, plain).opacity, 1.0);
    let c = contour(&fig, plain_contour);
    assert_eq!((c.line_opacity, c.fill_opacity), (1.0, 1.0));
    let c = contour(&fig, faded_contour);
    assert_eq!((c.line_opacity, c.fill_opacity), (0.0, 0.5));
}

// WHY: alpha data is stored in the figure's data table like any other array, so the
// setter must store the matrix with its shape and element type (floats from a matrix,
// bytes from a byte matrix, as image arrays are), point the artist at it, and, when the
// alpha is set again, drop the array it replaced rather than leave it in every saved file.
#[test]
fn alpha_setters_store_their_arrays_and_release_the_ones_they_replace() {
    let (x, y, z) = small_grid();
    let floats = Matrix::from_fn(z.rows(), z.cols(), |row, col| (row + col) as f64 / 10.0);
    let bytes = ByteMatrix::from_fn(2, 2, |row, col| (row * 2 + col) as u8 * 60);
    let values = Matrix::from_fn(2, 2, |row, col| (row + col) as f64);
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let s = ax
        .surf(&x, &y, &z)
        .face_alpha(&floats)
        .edge_alpha(&floats)
        .id();
    let m = ax.scalar_image(&values).alpha(&bytes).id();
    let i = ax.scalar_image(&values).alpha(&values).id();

    let surface_ir = surface(&fig, s).clone();
    let face = surface_ir.face_alpha.expect("the face alpha is set");
    let edge = surface_ir.edge_alpha.expect("the edge alpha is set");
    assert_eq!(data(&fig, face).shape, vec![z.rows(), z.cols()]);
    assert_eq!(data(&fig, edge), data(&fig, face));
    let alpha = scalar_image(&fig, m).alpha.expect("the alpha is set");
    assert_eq!(data(&fig, alpha).element(), NdArrayElement::U8);
    assert_eq!(
        data(&fig, alpha),
        &NdArray::from_shape_u8(vec![2, 2], vec![0, 60, 120, 180]).unwrap()
    );
    let alpha = scalar_image(&fig, i).alpha.expect("the alpha is set");
    assert_eq!(data(&fig, alpha).element(), NdArrayElement::F64);

    let mut again = Figure::new();
    let id = again
        .axes(0, 0)
        .surf(&x, &y, &z)
        .face_alpha(&floats)
        .face_alpha(&floats)
        .id();
    let kept = surface(&again, id)
        .face_alpha
        .expect("the face alpha is set");
    assert_eq!(
        again.ir().data.len(),
        4,
        "the two grid vectors, the field and the second alpha, without the first: {:?}",
        again
            .ir()
            .data
            .iter()
            .map(|data| data.id)
            .collect::<Vec<_>>()
    );
    assert!(again.ir().data_array(kept).is_some());
}

// WHY: an opacity is checked by validation rather than by the setter, so that a figure
// can be built in any order; the error must name the artist the handle returned, so that
// the user learns which artist to fix, and so must a wrong-shaped alpha.
#[test]
fn an_invalid_opacity_or_alpha_is_reported_at_the_handles_id() {
    let (x, y, z) = small_grid();
    let values = Matrix::from_fn(2, 2, |row, col| (row + col) as f64);
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let s = ax.surf(&x, &y, &z).edge_opacity(1.5).id();
    let c = ax.contourf(&x, &y, &z).fill_opacity(-0.5).id();
    let m = ax
        .scalar_image(&values)
        .alpha(Matrix::from_fn(1, 2, |_, _| 1.0))
        .id();
    let report = fig.validate();
    for id in [s, c] {
        assert!(
            has_error_at(&report, IssueKind::InvalidOpacity, id),
            "{report:?}"
        );
    }
    assert!(
        has_error_at(&report, IssueKind::ShapeMismatch, m),
        "{report:?}"
    );
}
