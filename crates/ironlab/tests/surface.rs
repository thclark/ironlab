//! Surface plots: surf and mesh.

mod common;

use common::{data, has_error_at, is_3d, parent, small_grid, surface};
use ironlab::ir::{ColorSpec, Grid, IssueKind, NdArray};
use ironlab::prelude::*;

// WHY: surf's look is defined by colormapped faces with black edges (MATLAB `shading
// faceted`); these defaults are what the gallery and users rely on.
#[test]
fn surf_defaults_to_colormapped_faces_and_black_edges() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).surf(&x, &y, &z).id();
    let s = surface(&fig, id);
    assert_eq!(s.face, ColorSpec::Colormapped);
    assert_eq!(
        s.edge,
        ColorSpec::Rgba {
            color: Color::BLACK
        }
    );
    assert_eq!(s.c, None);
    let stored = data(&fig, s.z);
    assert_eq!(stored.shape, vec![3, 4]);
    assert_eq!(stored.as_f64(), Some(z.values()));
    assert!(matches!(s.grid, Grid::Rectilinear { .. }));
}

// WHY: mesh differs from surf only in colours: faces take the figure background (so
// they occlude hidden edges) and edges are colormapped. This must use the figure's
// actual background, which is white by default.
#[test]
fn mesh_has_background_faces_and_colormapped_edges() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).mesh(&x, &y, &z).id();
    let s = surface(&fig, id);
    assert_eq!(
        s.face,
        ColorSpec::Rgba {
            color: Color::WHITE
        }
    );
    assert_eq!(s.edge, ColorSpec::Colormapped);
}

// WHY: see mesh_has_background_faces_and_colormapped_edges; a non-white background
// must be honoured, or meshes on dark figures show white faces.
#[test]
fn mesh_faces_follow_a_non_default_background() {
    let (x, y, z) = small_grid();
    let dark = Color::rgb(0.1, 0.1, 0.1);
    let mut fig = Figure::new();
    fig.ir_mut().background = dark;
    let id = fig.axes(0, 0).mesh(&x, &y, &z).id();
    assert_eq!(surface(&fig, id).face, ColorSpec::Rgba { color: dark });
}

// WHY: surf and mesh are inherently 3D; like MATLAB they must convert a 2D axes rather
// than leave the user with a ThreeDArtistInTwoDAxes validation error.
#[test]
fn surf_and_mesh_promote_the_axes_to_3d() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new().tiles(1, 2);
    let s = fig.axes(0, 0).surf(&x, &y, &z).id();
    let m = fig.axes(0, 1).mesh(&x, &y, &z).id();
    assert!(is_3d(parent(&fig, s)));
    assert!(is_3d(parent(&fig, m)));
    assert!(fig.validate().is_valid());
}

// WHY: the colour setters accept `None` to hide faces or edges (MATLAB 'none'), which
// is the common way to draw a smooth-looking surface.
#[test]
fn surface_setters_map_to_ir_fields() {
    let (x, y, z) = small_grid();
    let c = z.map(|v| -v);
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .surf(&x, &y, &z)
        .edge_color(None)
        .face_color(Color::rgb(0.5, 0.5, 0.5))
        .edge_width(1.25)
        .color_data(&c)
        .display_name("field")
        .id();
    let s = surface(&fig, id);
    assert_eq!(s.edge, ColorSpec::None);
    assert_eq!(
        s.face,
        ColorSpec::Rgba {
            color: Color::rgb(0.5, 0.5, 0.5)
        }
    );
    assert_eq!(s.edge_width_pt, 1.25);
    let stored = data(&fig, s.c.expect("color_data stores c"));
    assert_eq!(
        stored,
        &NdArray::from_shape(vec![3, 4], c.into_values()).unwrap()
    );
    assert_eq!(s.display_name, Some(Text::new("field")));
}

// WHY: a height matrix that does not match the grid vectors must be reported, not
// panic, for surfaces as well as contours.
#[test]
fn surface_grid_mismatch_is_reported_by_validate() {
    let (x, y, _) = small_grid();
    let wrong = Matrix::from_fn(4, 3, |_, _| 0.0);
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).surf(&x, &y, &wrong).id();
    assert!(has_error_at(&fig.validate(), IssueKind::ShapeMismatch, id));
}
