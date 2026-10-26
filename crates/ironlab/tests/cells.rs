//! Cells in the facade (#74): `cells`, `occupancy` and the `CellsMut` handle that configures the `Cells` artist.
//!
//! These tests assert the figure description the facade stores, not its rendering, which the scene tests own. They
//! assume this API, exported in the prelude:
//!
//! - `AxesMut::cells(values: impl Into<DataSource>) -> CellsMut`, taking a `Matrix`, a `ByteMatrix` or the
//!   identifier of stored data as `scalar_image` does, and naming the default colour scale of the axes as
//!   `scalar_image` does, creating it when the axes has none.
//! - `CellsMut` setters, named as those of `SurfaceMut` and `ScalarImageMut` are: `label`, `column_edges(data)`,
//!   `row_edges(data)`, `unit_columns()`, `unit_rows()`, `gap(x, y)`, `edge_color`, `edge_width`, `alpha(data)`,
//!   `face_opacity`, `edge_opacity`, `color_scale(id)`, and the getter `id()`. Data that a setter replaces is
//!   released from the figure when nothing else uses it and it has no name, as for every other artist.
//! - `AxesMut::occupancy<R: AsRef<[f64]>>(rows: impl IntoIterator<Item = R>) -> CellsMut`, which pads ragged rows
//!   with NaN to the longest row, stores them as cells on unit edges with a gap of 0.2 along x and y, and hides the
//!   x axis. It sets no categories of its own; `ycategories` and `ycategory_groups` (see `categories.rs`) name the
//!   rows.

mod common;

use common::{artist, axes, data, has_error, has_warning};
use ironlab::IssueKind;
use ironlab::ir::{Artist, CellEdges, CellGap, Cells, ColorScaleKind, NdArray};
use ironlab::prelude::*;

const GREEN: Color = Color::rgb(0.0, 0.6, 0.3);
const RED: Color = Color::rgb(0.8, 0.2, 0.1);
const GREY: Color = Color::rgb(0.6, 0.6, 0.6);

/// Returns the cells with the given identifier.
#[track_caller]
fn cells(fig: &Figure, id: NodeId) -> &Cells {
    match artist(fig, id) {
        Artist::Cells(c) => c,
        other => panic!("expected cells, found {other:?}"),
    }
}

/// The values of an array of floating-point numbers, with each NaN written as `None` so that arrays holding NaN
/// can be compared.
fn values_of(array: &NdArray) -> Vec<Option<f64>> {
    array
        .as_f64()
        .expect("the values are floating-point")
        .iter()
        .map(|v| (!v.is_nan()).then_some(*v))
        .collect()
}

/// A categorical scale of the statuses of a branch.
fn statuses(fig: &mut Figure) -> NodeId {
    fig.categorical_scale([(GREEN, "taken"), (RED, "not taken"), (GREY, "unreachable")])
        .id()
}

// WHY: `cells(values)` must draw a heatmap without further calls, as MATLAB's `heatmap` and Matplotlib's `pcolormesh`
// do: seamless unoutlined cells centred on the indices of the values, opaque, mapped through the default colour scale
// of the axes, which it creates as `scalar_image` does, so that a colourbar of the axes describes them. Cells always
// map their values, so a figure whose cells named no scale would be invalid.
#[test]
fn cells_stores_the_values_with_the_defaults_of_a_heatmap_and_the_default_scale() {
    let mut fig = Figure::new();
    let z = Matrix::from_rows(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
    let id = fig.axes(0, 0).cells(&z).id();
    let default = fig.axes(0, 0).color_scale().id();
    let c = cells(&fig, id);
    let stored = data(&fig, c.values);
    assert_eq!(stored.shape, vec![2, 3]);
    assert_eq!(stored.as_f64(), Some(z.values()));
    assert_eq!((c.columns, c.rows), (CellEdges::Unit, CellEdges::Unit));
    assert_eq!(c.gap, CellGap { x: 0.0, y: 0.0 });
    assert_eq!(c.edge.color, ColorSpec::None, "no outline");
    assert_eq!(c.alpha, None);
    assert_eq!((c.face_opacity, c.edge_opacity), (1.0, 1.0));
    assert_eq!(
        c.color_scale,
        Some(default),
        "the default scale of the axes"
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: an occupancy matrix stores a status per cell as a whole number naming a category, compactly as bytes, mapped
// through a categorical scale whose key names the statuses; naming that scale replaces the default, which is deleted
// because nothing else names it, so that the figure keeps no stray scale that a colourbar or key would show.
#[test]
fn byte_statuses_through_a_categorical_scale_replace_the_default_scale() {
    let mut fig = Figure::new();
    let status = statuses(&mut fig);
    let codes = ByteMatrix::from_rows(&[[0, 1, 2], [2, 1, 0]]);
    let id = fig.axes(0, 0).cells(&codes).color_scale(status).id();
    let c = cells(&fig, id);
    assert_eq!(
        data(&fig, c.values).as_u8(),
        Some(&[0u8, 1, 2, 2, 1, 0][..])
    );
    assert_eq!(c.color_scale, Some(status));
    assert_eq!(
        fig.ir().color_scales.len(),
        1,
        "the default scale is deleted"
    );
    assert!(matches!(
        fig.ir().color_scales[0].kind,
        ColorScaleKind::Categorical(_)
    ));
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: each setter is the facade's name for one property of the cells artist, so each must write exactly that
// property: explicit edges, how a two-dimensional histogram of unequal bins is stored, as data the edges name; the
// gap along x and y independently; an outline colour, which `None` hides and `Colormapped` takes from each cell; and
// the alpha array and the two opacities that fade the cells.
#[test]
fn cells_setters_write_the_properties_they_name() {
    let mut fig = Figure::new();
    let z = Matrix::from_rows(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
    let alpha = Matrix::from_rows(&[[1.0, 0.5, 1.0], [0.25, 1.0, 1.0]]);
    let id = fig
        .axes(0, 0)
        .cells(&z)
        .label("Counts")
        .column_edges([0.0, 1.0, 3.0, 7.0])
        .row_edges([10.0, 5.0, 0.0])
        .gap(0.1, 0.25)
        .edge_color(Color::WHITE)
        .edge_width(1.5)
        .alpha(&alpha)
        .face_opacity(0.75)
        .edge_opacity(0.5)
        .id();
    let c = cells(&fig, id).clone();
    assert_eq!(c.label, Some(Text::new("Counts")));
    let CellEdges::Edges(columns) = c.columns else {
        panic!("explicit column edges are data, not {:?}", c.columns);
    };
    assert_eq!(
        data(&fig, columns),
        &NdArray::vector(vec![0.0, 1.0, 3.0, 7.0])
    );
    let CellEdges::Edges(rows) = c.rows else {
        panic!("explicit row edges are data, not {:?}", c.rows);
    };
    assert_eq!(data(&fig, rows), &NdArray::vector(vec![10.0, 5.0, 0.0]));
    assert_eq!(c.gap, CellGap { x: 0.1, y: 0.25 });
    assert_eq!(
        c.edge.color,
        ColorSpec::Rgba {
            color: Color::WHITE
        }
    );
    assert_eq!(c.edge.width_pt, 1.5);
    let stored_alpha = c.alpha.expect("the cells have an alpha");
    assert_eq!(data(&fig, stored_alpha).as_f64(), Some(alpha.values()));
    assert_eq!((c.face_opacity, c.edge_opacity), (0.75, 0.5));
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());

    let id = fig
        .axes(0, 0)
        .cells(&z)
        .edge_color(ColorSpec::Colormapped)
        .edge_color(None)
        .id();
    assert_eq!(
        cells(&fig, id).edge.color,
        ColorSpec::None,
        "`None` hides the outline"
    );
}

// WHY: returning to unit edges, or giving new edges or a new alpha, leaves the old arrays unused; an array the
// figure no longer refers to would bloat every saved file and appear in the browser as data of nothing, so it is
// released, as replaced data is released by every other setter.
#[test]
fn replaced_edges_and_alpha_are_released() {
    let mut fig = Figure::new();
    let z = Matrix::from_rows(&[[1.0, 2.0], [3.0, 4.0]]);
    let id = fig
        .axes(0, 0)
        .cells(&z)
        .column_edges([0.0, 1.0, 2.0])
        .column_edges([0.0, 2.0, 4.0])
        .row_edges([0.0, 1.0, 2.0])
        .unit_rows()
        .alpha(Matrix::from_rows(&[[1.0, 1.0], [0.5, 0.5]]))
        .alpha(Matrix::from_rows(&[[0.25, 0.25], [1.0, 1.0]]))
        .id();
    let c = cells(&fig, id).clone();
    assert_eq!(c.rows, CellEdges::Unit, "`unit_rows` returns to unit edges");
    let mut kept: Vec<NodeId> = vec![c.values, c.alpha.expect("the second alpha")];
    if let CellEdges::Edges(columns) = c.columns {
        assert_eq!(data(&fig, columns), &NdArray::vector(vec![0.0, 2.0, 4.0]));
        kept.push(columns);
    } else {
        panic!("the second column edges are kept, not {:?}", c.columns);
    }
    kept.sort();
    let mut stored: Vec<NodeId> = fig.ir().data.iter().map(|d| d.id).collect();
    stored.sort();
    assert_eq!(stored, kept, "only the arrays the cells use are kept");
}

// WHY: the facade knows which arrays an artist uses through `data_ids`, which must therefore list the edges and the
// alpha of cells: two artists may share one array, and an array that other cells still use must survive when one of
// them stops using it, or the figure would refer to data it no longer holds.
#[test]
fn edges_still_used_by_other_cells_are_kept() {
    let mut fig = Figure::new();
    let z = Matrix::from_rows(&[[1.0, 2.0], [3.0, 4.0]]);
    let first = fig.axes(0, 0).cells(&z).column_edges([0.0, 1.0, 3.0]).id();
    let CellEdges::Edges(shared) = cells(&fig, first).columns else {
        panic!("explicit edges");
    };
    fig.axes(0, 0).cells(&z).column_edges(shared).unit_columns();
    assert!(
        fig.ir().data_array(shared).is_some(),
        "the first cells still use the edges"
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: an occupancy matrix has ragged rows (a subroutine has as many cells as it has branches), which a matrix cannot
// hold; `occupancy` pads each row with NaN to the longest, which the default non-finite policy draws as nothing, and
// stores the result as cells on unit edges, so that cell (j, i) is the i-th thing of row j. The gap of 0.2 along
// both axes sets every cell apart as one thing, and the x axis, whose positions number the things of a row and mean
// nothing to the reader, is hidden; the y axis is left to name the rows.
#[test]
fn occupancy_pads_ragged_rows_with_nan_on_unit_edges_with_a_gap_and_hides_the_x_axis() {
    let mut fig = Figure::new();
    let status = statuses(&mut fig);
    let rows = [vec![0.0, 1.0, 2.0], vec![1.0], vec![2.0, 2.0]];
    let id = fig.axes(0, 0).occupancy(&rows).color_scale(status).id();
    let c = cells(&fig, id);
    let stored = data(&fig, c.values);
    assert_eq!(
        stored.shape,
        vec![3, 3],
        "three rows padded to the longest, of three"
    );
    assert_eq!(
        values_of(stored),
        vec![
            Some(0.0),
            Some(1.0),
            Some(2.0),
            Some(1.0),
            None,
            None,
            Some(2.0),
            Some(2.0),
            None
        ],
        "each row is padded with NaN at its end"
    );
    assert_eq!((c.columns, c.rows), (CellEdges::Unit, CellEdges::Unit));
    assert_eq!(c.gap, CellGap { x: 0.2, y: 0.2 });
    assert_eq!(c.color_scale, Some(status));
    let ax = axes(&fig, common::parent(&fig, id).id);
    assert!(!ax.x.visible, "the x axis is hidden");
    assert!(ax.y.visible, "the y axis is left to name the rows");
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: an occupancy matrix of nothing, such as a study with no subroutines yet, is valid (ADR 0012): it is stored as
// cells with no rows, which validation warns of as having nothing to draw, rather than refused or panicking.
#[test]
fn occupancy_of_no_rows_is_valid_and_warns_that_there_is_nothing_to_draw() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).occupancy(Vec::<Vec<f64>>::new()).id();
    assert_eq!(data(&fig, cells(&fig, id).values).shape[0], 0, "no rows");
    let report = fig.validate();
    assert!(!has_error(&report, IssueKind::ShapeMismatch), "{report:?}");
    assert!(report.is_valid(), "{report:?}");
    assert!(has_warning(&report, IssueKind::NothingToDraw), "{report:?}");
}

// WHY: the handles must be nameable by users who write helpers that configure them, so `CellsMut` and `CategoryRows`
// are in the prelude, as every other handle is.
#[test]
fn the_handles_are_in_the_prelude() {
    fn configure(cells: &mut CellsMut<'_>) {
        cells.gap(0.1, 0.1);
    }
    fn count(rows: &CategoryRows) -> usize {
        rows.len()
    }
    let mut fig = Figure::new();
    let z = Matrix::from_rows(&[[1.0]]);
    configure(&mut fig.axes(0, 0).cells(&z));
    let rows = fig.axes(0, 0).ycategory_groups([("a.f", vec!["x"])]);
    assert_eq!(count(&rows), 2);
}
