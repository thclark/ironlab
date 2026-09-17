//! Figure-level builders and the placement of axes in the tile layout.

mod common;

use common::{axes, has_error, is_3d};
use ironlab::ir::{self, IssueKind, Projection, View3d};
use ironlab::prelude::*;

// WHY: a new figure must be exactly the IR's MATLAB-like default, so that the facade
// adds no hidden state and a figure built with no calls matches the documented
// defaults of the file format.
#[test]
fn new_figure_is_the_ir_default() {
    let fig = Figure::new();
    assert_eq!(fig.ir(), &ir::Figure::default());
    assert!(fig.ir().axes.is_empty());
}

// WHY: the figure-level builders are the only way to set page size, title, font size
// and layout from user code; each must land in its IR field, or exported PDFs have
// the wrong physical size or layout.
#[test]
fn figure_builders_set_ir_fields() {
    let fig = Figure::new()
        .size_mm(120.0, 45.5)
        .title("Overview of $\\alpha$")
        .font_size_pt(7.5)
        .tiles(2, 3);
    let ir = fig.ir();
    assert_eq!(ir.size.width_mm, 120.0);
    assert_eq!(ir.size.height_mm, 45.5);
    assert_eq!(ir.font_size_pt, 7.5);
    assert_eq!((ir.layout.rows, ir.layout.cols), (2, 3));
    assert_eq!(ir.title, Some(Text::new("Overview of $\\alpha$")));
}

// WHY: users (and gallery pages) re-borrow axes by cell, MATLAB `subplot` style; a
// second call for the same cell must not create a duplicate axes that would draw on
// top of the first.
#[test]
fn axes_at_the_same_cell_is_idempotent() {
    let mut fig = Figure::new().tiles(2, 2);
    let first = fig.axes(1, 0).id();
    let again = fig.axes(1, 0).id();
    assert_eq!(first, again);
    assert_eq!(fig.ir().axes.len(), 1);

    let other = fig.axes(0, 1).id();
    assert_ne!(other, first);
    assert_eq!(fig.ir().axes.len(), 2);
}

// WHY: a created axes must record its cell and be a default 2D axes, because the scene
// compiler lays out axes solely from `cell` and `projection`.
#[test]
fn axes_creates_a_default_2d_axes_in_its_cell() {
    let mut fig = Figure::new().tiles(2, 3);
    let id = fig.axes(1, 2).id();
    let a = axes(&fig, id);
    assert_eq!(
        a.cell,
        ir::Cell {
            row: 1,
            col: 2,
            row_span: 1,
            col_span: 1
        }
    );
    assert_eq!(a.projection, Projection::TwoD);
    assert!(a.artists.is_empty());
}

// WHY: node identifiers must stay unique as axes are added, because links, legend
// toggles and saved interaction state refer to nodes by identifier.
#[test]
fn created_axes_have_distinct_ids_from_the_figure() {
    let mut fig = Figure::new().tiles(2, 2);
    let ids = [
        fig.axes(0, 0).id(),
        fig.axes(0, 1).id(),
        fig.axes(1, 0).id(),
        fig.axes(1, 1).id(),
    ];
    let mut all: Vec<NodeId> = ids.to_vec();
    all.push(fig.ir().id);
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 5);
}

// WHY: the builder must not panic on an axes outside the layout (the brief: problems
// surface through validation), yet the user must still be told about it.
#[test]
fn axes_outside_the_layout_is_reported_by_validate_not_a_panic() {
    let mut fig = Figure::new().tiles(1, 2);
    fig.axes(0, 0).plot([0.0, 1.0], [0.0, 1.0]);
    fig.axes(3, 0).plot([0.0, 1.0], [0.0, 1.0]);
    let report = fig.validate();
    assert!(has_error(&report, IssueKind::CellOutOfLayout));
}

// WHY: spanning axes are how gallery figures build mixed layouts; the spans must be
// stored so the layout engine gives the axes the combined area.
#[test]
fn axes_span_sets_spans() {
    let mut fig = Figure::new().tiles(3, 3);
    let id = fig.axes_span(0, 1, 2, 2).id();
    let cell = axes(&fig, id).cell;
    assert_eq!(
        (cell.row, cell.col, cell.row_span, cell.col_span),
        (0, 1, 2, 2)
    );
    assert!(fig.validate().is_valid());
}

// WHY: once an axes spans several tiles, asking for any covered tile should return
// that axes rather than silently creating an overlapping one.
#[test]
fn axes_in_a_covered_tile_returns_the_spanning_axes() {
    let mut fig = Figure::new().tiles(2, 2);
    let wide = fig.axes_span(0, 0, 1, 2).id();
    assert_eq!(fig.axes(0, 1).id(), wide);
    assert_eq!(fig.ir().axes.len(), 1);
}

// WHY: calling axes_span again on the same top-left tile must update the existing
// axes (keeping its plots), not add a second axes.
#[test]
fn axes_span_on_an_existing_axes_changes_its_span() {
    let mut fig = Figure::new().tiles(2, 2);
    let id = fig.axes(0, 0).id();
    fig.axes(0, 0).plot([0.0, 1.0], [1.0, 2.0]);
    assert_eq!(fig.axes_span(0, 0, 2, 1).id(), id);
    let a = axes(&fig, id);
    assert_eq!((a.cell.row_span, a.cell.col_span), (2, 1));
    assert_eq!(a.artists.len(), 1);
    assert_eq!(fig.ir().axes.len(), 1);
}

// WHY: axes3 must produce MATLAB's default 3D camera, since gallery 3D figures rely on
// it without calling view().
#[test]
fn axes3_creates_a_3d_axes_with_the_default_view() {
    let mut fig = Figure::new();
    let id = fig.axes3(0, 0).id();
    assert_eq!(
        axes(&fig, id).projection,
        Projection::ThreeD {
            view3d: View3d::default()
        }
    );
}

// WHY: axes3 on an existing 2D axes converts it in place, keeping its plots and
// properties, so a user can decide on 3D after plotting.
#[test]
fn axes3_converts_an_existing_2d_axes_in_place() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).id();
    fig.axes(0, 0).plot([0.0, 1.0], [0.0, 1.0]);
    fig.axes(0, 0).title("kept");
    assert_eq!(fig.axes3(0, 0).id(), id);
    let a = axes(&fig, id);
    assert!(is_3d(a));
    assert_eq!(a.artists.len(), 1);
    assert_eq!(a.title, Some(Text::new("kept")));
    assert_eq!(fig.ir().axes.len(), 1);
}

// WHY: axes3 on an axes that is already 3D must not reset a view the user has set.
#[test]
fn axes3_keeps_an_existing_view() {
    let mut fig = Figure::new();
    fig.axes3(0, 0).view(45.0, 10.0);
    let id = fig.axes3(0, 0).id();
    match axes(&fig, id).projection {
        Projection::ThreeD { view3d } => {
            assert_eq!((view3d.azimuth_deg, view3d.elevation_deg), (45.0, 10.0));
        }
        Projection::TwoD => panic!("expected a 3D axes"),
    }
}

// WHY: axes() is a lookup as well as a constructor; it must not demote a 3D axes back
// to 2D when the user re-borrows it to add labels.
#[test]
fn axes_does_not_demote_a_3d_axes() {
    let mut fig = Figure::new();
    let id = fig.axes3(0, 0).id();
    fig.axes(0, 0).zlabel("$z$");
    assert!(is_3d(axes(&fig, id)));
}

// WHY: view() sets the camera stored in the IR, keeping zoom and pan, which the viewer
// changes independently.
#[test]
fn view_sets_azimuth_and_elevation_and_keeps_zoom_and_pan() {
    let mut fig = Figure::new();
    let id = fig.axes3(0, 0).id();
    if let Some(a) = fig.ir_mut().axes_mut(id) {
        a.projection = Projection::ThreeD {
            view3d: View3d {
                zoom: 2.0,
                pan_x: 0.1,
                pan_y: -0.2,
                ..View3d::default()
            },
        };
    }
    fig.axes(0, 0).view(120.0, 15.0);
    assert_eq!(
        axes(&fig, id).projection,
        Projection::ThreeD {
            view3d: View3d {
                azimuth_deg: 120.0,
                elevation_deg: 15.0,
                zoom: 2.0,
                pan_x: 0.1,
                pan_y: -0.2,
            }
        }
    );
}

// WHY: view() on a 2D axes follows MATLAB, where setting a 3D view makes the axes 3D.
#[test]
fn view_promotes_a_2d_axes_to_3d() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).id();
    fig.axes(0, 0).view(0.0, 90.0);
    assert!(is_3d(axes(&fig, id)));
}

// WHY: from_ir/into_ir are the escape hatch between the facade and the IR (for loading
// files and hand-built figures); they must be lossless, and the builder must continue
// allocating identifiers that do not collide with the wrapped figure's.
#[test]
fn from_ir_and_into_ir_are_lossless_and_allocation_continues() {
    let mut original = Figure::new().tiles(1, 2);
    let existing = original.axes(0, 0).id();
    let ir = original.clone().into_ir();

    let mut wrapped = Figure::from_ir(ir.clone());
    assert_eq!(wrapped.ir(), &ir);
    let new = wrapped.axes(0, 1).id();
    assert_ne!(new, existing);
    assert_ne!(new, wrapped.ir().id);
}

// WHY: parameters are set from ordinary Rust values, and the kind stored in the IR
// decides how figures sort and filter, so each Rust type must map to the matching kind.
// Values of type i32 (the type of most integer variables, and of an integer literal that
// nothing else constrains) and of type i64 must both be accepted as integers.
#[test]
fn parameter_stores_each_rust_value_as_the_matching_kind() {
    let cells: i32 = 4096;
    let count: i64 = 1 << 40;
    let fig = Figure::new()
        .parameter("converged", true)
        .parameter("cells", cells)
        .parameter("samples", count)
        .parameter("reynolds_number", 1e5)
        .parameter("ratio", 3.0)
        .parameter("solver", "k–ω SST")
        .parameter("case", String::from("baseline"));
    let expected = [
        ("case", Parameter::String("baseline".to_owned())),
        ("cells", Parameter::Integer(4096)),
        ("converged", Parameter::Bool(true)),
        ("ratio", Parameter::Number(3.0)),
        ("reynolds_number", Parameter::Number(1e5)),
        ("samples", Parameter::Integer(1 << 40)),
        ("solver", Parameter::String("k–ω SST".to_owned())),
    ]
    .map(|(name, value)| (name.to_owned(), value));
    assert_eq!(
        fig.parameters().clone().into_iter().collect::<Vec<_>>(),
        expected
    );
}

// WHY: a parameter is identified by its name, so setting a name again (for example when a
// script refines a value) must replace the value, including its kind, rather than keep
// two entries or the first value.
#[test]
fn setting_a_parameter_again_replaces_its_value_and_kind() {
    let fig = Figure::new()
        .parameter("mesh", "coarse")
        .parameter("mesh", 3);
    assert_eq!(fig.parameters().len(), 1);
    assert_eq!(fig.parameters()["mesh"], Parameter::Integer(3));
}
