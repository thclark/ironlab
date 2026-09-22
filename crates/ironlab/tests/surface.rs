//! Surface plots: surf and mesh in three dimensions, and surface in the axes as it is.

mod common;

use common::{axes, data, has_error_at, is_3d, parent, small_grid, surface};
use ironlab::ir::{ColorSpec, Grid, IssueKind, NdArray, Projection, View3d};
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

// WHY: surf and mesh are the 3D plots of MATLAB; like MATLAB they must convert a 2D
// axes, so that the user sees a surface in perspective rather than the flat
// pseudocolour plot that a surface in a 2D axes is (that is what `surface` is for).
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

// WHY: `surface` is how a pseudocolour plot (MATLAB's `pcolor`) is made: the same
// artist as surf, with the same look, added to the axes as it is. Like `image`, it must
// leave a 2D axes two-dimensional, which is the whole difference from surf, and the
// figure it builds must validate, since a surface is allowed in a 2D axes.
#[test]
fn surface_has_the_defaults_of_surf_and_leaves_a_2d_axes_two_dimensional() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new().tiles(1, 2);
    let flat = fig.axes(0, 0).surface(&x, &y, &z).id();
    let raised = fig.axes(0, 1).surf(&x, &y, &z).id();

    assert!(!is_3d(parent(&fig, flat)));
    assert!(is_3d(parent(&fig, raised)), "surf still converts its axes");

    let (s, reference) = (surface(&fig, flat), surface(&fig, raised));
    assert_eq!(s.face, reference.face);
    assert_eq!(s.edge, reference.edge);
    assert_eq!(s.edge_width_pt, reference.edge_width_pt);
    assert_eq!(s.c, None);
    assert!(matches!(s.grid, Grid::Rectilinear { .. }));
    let stored = data(&fig, s.z);
    assert_eq!(stored.shape, vec![3, 4]);
    assert_eq!(stored.as_f64(), Some(z.values()));

    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: "leaves the axes as it is" cuts both ways. A user who has set the camera of a 3D
// axes and then adds a surface with `surface` must keep that axes three-dimensional and
// its view exactly as set, rather than have it flattened to 2D or reset to the default
// view.
#[test]
fn surface_keeps_an_axes_that_is_already_3d_and_its_view() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new();
    let axes_id = fig.axes3(0, 0).view(45.0, 10.0).id();
    let id = fig.axes(0, 0).surface(&x, &y, &z).id();
    assert_eq!(parent(&fig, id).id, axes_id);
    assert_eq!(
        axes(&fig, axes_id).projection,
        Projection::ThreeD {
            view3d: View3d {
                azimuth_deg: 45.0,
                elevation_deg: 10.0,
                ..View3d::default()
            }
        }
    );
    assert!(fig.validate().is_valid());
}

// WHY: the axes decides how its surfaces are seen, not the call that added them. A
// later surf in the same axes converts it to 3D, as surf always does, and the surface
// added earlier with `surface` must simply be drawn in that 3D axes: one artist kind,
// valid in both projections.
#[test]
fn surf_after_surface_converts_the_shared_axes_to_3d() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new();
    let first = fig.axes(0, 0).surface(&x, &y, &z).id();
    assert!(!is_3d(parent(&fig, first)));
    let second = fig.axes(0, 0).surf(&x, &y, &z).id();
    assert_eq!(parent(&fig, first).id, parent(&fig, second).id);
    assert!(is_3d(parent(&fig, first)));
    assert!(fig.validate().is_valid());
}

// WHY: the usual pseudocolour plot of mapped data has a curvilinear mesh given as
// coordinate matrices, no edges (MATLAB's `shading flat`) and often separate colour
// data. `surface` must store coordinate matrices as a curvilinear grid, and none of the
// settings applied through the handle it returns may convert the axes, so that the
// finished plot is still two-dimensional and validates. What each setter stores is
// pinned by `surface_setters_map_to_ir_fields`.
#[test]
fn surface_with_coordinate_matrices_and_flat_shading_stays_two_dimensional() {
    let (_, _, z) = small_grid();
    let xs = Matrix::from_fn(3, 4, |row, col| col as f64 + 0.5 * row as f64);
    let ys = Matrix::from_fn(3, 4, |row, col| row as f64 + 0.25 * col as f64);
    let c = z.map(|v| -v);
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .surface(xs.clone(), ys.clone(), &z)
        .edge_color(None)
        .color_data(&c)
        .id();
    let s = surface(&fig, id);
    let Grid::Curvilinear { x, y } = s.grid else {
        panic!("coordinate matrices make a curvilinear grid: {:?}", s.grid);
    };
    assert_eq!(data(&fig, x).shape, vec![3, 4]);
    assert_eq!(data(&fig, x).as_f64(), Some(xs.values()));
    assert_eq!(data(&fig, y).as_f64(), Some(ys.values()));
    assert_eq!(s.edge, ColorSpec::None);
    assert_eq!(s.face, ColorSpec::Colormapped);
    assert!(s.c.is_some());
    assert!(!is_3d(parent(&fig, id)));
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: allowing a surface in a 2D axes must not hide a real mistake. A field that does
// not match the grid vectors is reported as that mismatch, on the surface, and as
// nothing else, so the message the user reads is about the shapes and not about the
// projection of the axes.
#[test]
fn surface_grid_mismatch_in_a_2d_axes_is_reported_as_a_shape_mismatch_only() {
    let (x, y, _) = small_grid();
    let wrong = Matrix::from_fn(4, 3, |_, _| 0.0);
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).surface(&x, &y, &wrong).id();
    let report = fig.validate();
    assert!(has_error_at(&report, IssueKind::ShapeMismatch, id));
    assert_eq!(report.errors.len(), 1, "{report:?}");
}

// WHY: a pseudocolour plot of a single row of values is the natural first attempt of a
// user who expects n values to give n cells, as they do for a mapped image. The figure is
// valid, since a streamed field passes through one row, but the surface is not drawn, so
// the report a user or an agent reads must carry a warning that names the surface, whether
// it was added with `surface` or promoted to 3D by `surf`.
#[test]
fn a_field_with_a_single_row_validates_with_a_warning_for_surface_and_surf() {
    let x = linspace(0.0, 3.0, 4);
    let y = vec![0.0];
    let row = Matrix::from_fn(1, 4, |_, col| col as f64);
    for three_d in [false, true] {
        let mut fig = Figure::new();
        let id = if three_d {
            fig.axes(0, 0).surf(&x, &y, &row).id()
        } else {
            fig.axes(0, 0).surface(&x, &y, &row).id()
        };
        let report = fig.validate();
        assert!(report.is_valid(), "three_d = {three_d}: {report:?}");
        assert_eq!(report.warnings.len(), 1, "three_d = {three_d}: {report:?}");
        assert_eq!(report.warnings[0].kind, IssueKind::NothingToDraw);
        assert_eq!(report.warnings[0].node, Some(id));
    }
}
