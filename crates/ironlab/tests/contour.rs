//! Contour plots (contour, contourf, contour3) and the grid conversion shared by
//! gridded plots.

mod common;

use common::{contour, data, has_error_at, is_3d, parent, small_grid};
use ironlab::ir::{ColorSpec, ContourPlacement, Grid, IssueKind, Levels, NdArray};
use ironlab::prelude::*;

// WHY: the field must be stored with shape [ny, nx] in row-major order; a transposed
// or column-major copy would draw a mirrored plot that looks plausible.
#[test]
fn contour_stores_the_field_with_shape_rows_by_cols() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).contour(&x, &y, &z).id();
    let c = contour(&fig, id);
    let stored = data(&fig, c.z);
    assert_eq!(stored.shape, vec![3, 4]);
    assert_eq!(stored.as_f64(), Some(z.values()));
}

// WHY: coordinate vectors must produce a rectilinear grid (x per column, y per row),
// which is both the compact storage and what the contouring code fast-paths.
#[test]
fn vector_coordinates_make_a_rectilinear_grid() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).contour(&x, &y, &z).id();
    match contour(&fig, id).grid {
        Grid::Rectilinear { x: gx, y: gy } => {
            assert_eq!(data(&fig, gx), &NdArray::vector(x));
            assert_eq!(data(&fig, gy), &NdArray::vector(y));
        }
        other => panic!("expected a rectilinear grid, found {other:?}"),
    }
}

// WHY: coordinate matrices (for example from meshgrid, or a genuinely curved grid)
// must produce a curvilinear grid with the node coordinates stored as [ny, nx].
#[test]
fn matrix_coordinates_make_a_curvilinear_grid() {
    let (x, y, z) = small_grid();
    let (xx, yy) = meshgrid(&x, &y);
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).contour(&xx, &yy, &z).id();
    match contour(&fig, id).grid {
        Grid::Curvilinear { x: gx, y: gy } => {
            assert_eq!(data(&fig, gx).shape, vec![3, 4]);
            assert_eq!(data(&fig, gx).as_f64(), Some(xx.values()));
            assert_eq!(data(&fig, gy).as_f64(), Some(yy.values()));
        }
        other => panic!("expected a curvilinear grid, found {other:?}"),
    }
    assert!(fig.validate().is_valid());
}

// WHY: mixing a vector with a matrix is resolved (as documented on GridCoords) by
// repeating the vector to the field's shape, so the result is a valid curvilinear grid
// rather than an error.
#[test]
fn a_vector_mixed_with_a_matrix_is_repeated_to_the_field_shape() {
    let (x, y, z) = small_grid();
    let (_, yy) = meshgrid(&x, &y);
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).contour(&x, &yy, &z).id();
    match contour(&fig, id).grid {
        Grid::Curvilinear { x: gx, .. } => {
            let (xx, _) = meshgrid(&x, &y);
            assert_eq!(data(&fig, gx).shape, vec![3, 4]);
            assert_eq!(data(&fig, gx).as_f64(), Some(xx.values()));
        }
        other => panic!("expected a curvilinear grid, found {other:?}"),
    }
    assert!(fig.validate().is_valid());
}

// WHY: on a square field a vector repeated along the wrong dimension still has the
// right shape, so neither validation nor the non-square tests above detect it; the
// plot would silently be mirrored about the diagonal. Each mixed form is checked node
// by node: an x vector must vary along columns and a y vector along rows.
#[test]
fn mixed_coordinates_on_a_square_field_are_not_transposed() {
    let x = vec![0.0, 1.0, 2.0];
    let y = vec![10.0, 20.0, 30.0];
    let z = Matrix::from_fn(3, 3, |row, col| (row * 10 + col) as f64);
    let (xx, yy) = meshgrid(&x, &y);

    let mut fig = Figure::new().tiles(1, 2);
    let x_vector = fig.axes(0, 0).contour(&x, &yy, &z).id();
    let y_vector = fig.axes(0, 1).contour(&xx, &y, &z).id();

    let node_coordinates = |id| match contour(&fig, id).grid {
        Grid::Curvilinear { x: gx, y: gy } => (data(&fig, gx).clone(), data(&fig, gy).clone()),
        other => panic!("expected a curvilinear grid, found {other:?}"),
    };
    for id in [x_vector, y_vector] {
        let (gx, gy) = node_coordinates(id);
        assert_eq!(gx.shape, vec![3, 3]);
        assert_eq!(gx.as_f64(), Some(xx.values()), "x coordinates of {id}");
        assert_eq!(gy.as_f64(), Some(yy.values()), "y coordinates of {id}");
    }
}

// WHY: a grid vector whose length does not match the field is a common user error (x
// and y swapped); it must not panic in the builder but must be reported against the
// artist.
#[test]
fn grid_shape_mismatch_is_reported_by_validate() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new();
    // x and y swapped: 3 x values for 4 columns, 4 y values for 3 rows.
    let id = fig.axes(0, 0).contour(&y, &x, &z).id();
    assert!(has_error_at(&fig.validate(), IssueKind::ShapeMismatch, id));
}

// WHY: contour, contourf and contour3 differ only in fill and placement; these two
// fields decide whether isolines or bands are drawn and where, so each function must
// set exactly its combination.
#[test]
fn contour_variants_set_fill_and_placement() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new().tiles(1, 3);
    let lines = fig.axes(0, 0).contour(&x, &y, &z).id();
    let filled = fig.axes(0, 1).contourf(&x, &y, &z).id();
    let raised = fig.axes(0, 2).contour3(&x, &y, &z).id();

    let c = contour(&fig, lines);
    assert!(!c.fill);
    assert_eq!(c.placement, ContourPlacement::Plane { z: None });

    let c = contour(&fig, filled);
    assert!(c.fill);
    assert_eq!(c.placement, ContourPlacement::Plane { z: None });

    let c = contour(&fig, raised);
    assert!(!c.fill);
    assert_eq!(c.placement, ContourPlacement::AtLevel);
}

// WHY: contour and contourf are 2D plots and must leave the axes 2D, whereas contour3
// places isolines at their heights and so must promote the axes to 3D.
#[test]
fn only_contour3_promotes_the_axes_to_3d() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new().tiles(1, 3);
    let lines = fig.axes(0, 0).contour(&x, &y, &z).id();
    let filled = fig.axes(0, 1).contourf(&x, &y, &z).id();
    let raised = fig.axes(0, 2).contour3(&x, &y, &z).id();
    assert!(!is_3d(parent(&fig, lines)));
    assert!(!is_3d(parent(&fig, filled)));
    assert!(is_3d(parent(&fig, raised)));
    assert!(fig.validate().is_valid());
}

// WHY: the default must be ten automatic, colormapped levels (the IR default), and the
// two level setters must select automatic count and explicit values respectively.
#[test]
fn levels_and_level_values_select_automatic_or_explicit_levels() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let default = ax.contour(&x, &y, &z).id();
    let counted = ax.contour(&x, &y, &z).levels(4).id();
    let explicit = ax.contour(&x, &y, &z).level_values([1.0, 5.0, 25.0]).id();
    let overridden = ax
        .contour(&x, &y, &z)
        .level_values([1.0, 2.0])
        .levels(7)
        .id();

    assert_eq!(contour(&fig, default).levels, Levels::Auto { count: 10 });
    assert_eq!(contour(&fig, default).line.color, ColorSpec::Colormapped);
    assert_eq!(contour(&fig, counted).levels, Levels::Auto { count: 4 });
    assert_eq!(
        contour(&fig, explicit).levels,
        Levels::Explicit {
            values: vec![1.0, 5.0, 25.0]
        }
    );
    assert_eq!(contour(&fig, overridden).levels, Levels::Auto { count: 7 });
}

// WHY: invalid explicit levels are user input errors; they must reach the IR unchanged
// so that validation, not a builder panic, reports them.
#[test]
fn decreasing_level_values_are_reported_by_validate() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .contour(&x, &y, &z)
        .level_values([3.0, 1.0])
        .id();
    assert!(has_error_at(&fig.validate(), IssueKind::InvalidLevels, id));
}

// WHY: contour line style setters map to the isoline style used by the renderer.
#[test]
fn contour_setters_map_to_ir_fields() {
    let (x, y, z) = small_grid();
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .contour(&x, &y, &z)
        .line_width(0.3)
        .color(Color::BLACK)
        .display_name("$\\psi$")
        .id();
    let c = contour(&fig, id);
    assert_eq!(c.line.width_pt, 0.3);
    assert_eq!(
        c.line.color,
        ColorSpec::Rgba {
            color: Color::BLACK
        }
    );
    assert_eq!(c.display_name, Some(Text::new("$\\psi$")));
}
