//! Datatips on cells, tested without a GPU or a window.
//!
//! These tests compile the figure, because what they are about is the agreement between where the scene compiler drew
//! a cell and what the viewer reports for a pointer over it: the cell under the pointer, its row and column in the
//! user's array, its edges, its value in the words of its scale, and the names of its row and its column when the
//! axes are labelled with text, which is how an occupancy matrix names its things.
//!
//! The API these tests assume, beyond what `bar_interaction.rs` exercises:
//!
//! ```text
//! // crates/ironlab-canvas/src/interaction.rs
//! pub enum Tip { …, Cell(CellDatatip) }
//!
//! /// The cell under the pointer, as the viewer reports it.
//! pub struct CellDatatip {
//!     pub axes: NodeId,
//!     pub artist: NodeId,
//!     /// The label of the cells, when they have one.
//!     pub label: Option<String>,
//!     /// The row of the cell in the artist's array of values.
//!     pub row: usize,
//!     /// The column of the cell in the artist's array of values.
//!     pub column: usize,
//!     /// Where a callout is anchored, in figure space: the centre of the drawn cell.
//!     pub position: Point,
//!     /// The rectangle of the cell as drawn, in figure space, the gap excluded, which the canvas outlines.
//!     pub rect: Rect,
//!     /// The edges of the cell's column, edges `column` and `column + 1`, in the order of the data.
//!     pub column_edges: (f64, f64),
//!     /// The edges of the cell's row, edges `row` and `row + 1`, in the order of the data.
//!     pub row_edges: (f64, f64),
//!     /// What the artist's array holds for the cell, in the words of its scale, as a pixel of a scalar image
//!     /// reports it: `Index` through an indexed scale, `Category` (with its label) through a categorical one, and
//!     /// `Value` otherwise.
//!     pub value: PixelValue,
//!     /// The text tick label of the y axis at the centre of the cell's row, when the axis is labelled with text and
//!     /// a tick falls exactly there: the name of the row.
//!     pub row_name: Option<String>,
//!     /// The text tick label of the x axis at the centre of the cell's column, likewise: the name of the column.
//!     pub column_name: Option<String>,
//!     /// The count and density of the bin the cell draws, when the cells record the binning of a two-dimensional
//!     /// histogram; see `bar_interaction.rs` for `BinReading`.
//!     pub bin: Option<BinReading>,
//! }
//!
//! // crates/ironlab-canvas/src/figure_canvas.rs, re-exported from the crate root
//! pub fn cell_datatip_text(tip: &CellDatatip) -> String;
//! ```
//!
//! A cell is recorded in the hit map only when it was drawn, so a cell that its scale leaves transparent, such as the
//! NaN padding of a ragged row, has no datatip. A drawn point within reach wins over the cell beneath it, as it wins
//! over a bar or a pixel.

mod common;

use common::*;
use ironlab_canvas::interaction::{CellDatatip, PixelValue, Tip};
use ironlab_canvas::{DATATIP_RADIUS_POINTS, FigureState, cell_datatip_text};

use ironlab_ir::{
    Artist, Axes, CategoricalColorScale, CellEdges, CellGap, Cells, Color, ColorCategory,
    ColorScale, ColorScaleKind, ColorScaleLevels, ColorSpec, ContinuousColorScale,
    DiscreteColorScale, Figure, IndexedColorScale, Limits, NdArray, NodeId, Series, Text,
    TickLabels, Ticks,
};
use ironlab_scene::display::Point;
use ironlab_scene::hit::{AxesHitKind, HitMap};

// ---------------------------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------------------------

/// The node identifier of the colour scale of every fixture.
const SCALE: NodeId = NodeId(90);

/// A colour scale (node 90) of the given kind.
fn scale(kind: ColorScaleKind) -> ColorScale {
    ColorScale {
        id: SCALE,
        kind,
        ..ColorScale::default()
    }
}

/// A continuous scale between 0 and 1.
fn continuous() -> ColorScale {
    scale(ColorScaleKind::Continuous(ContinuousColorScale {
        limits: Limits::Manual { min: 0.0, max: 1.0 },
        ..ContinuousColorScale::default()
    }))
}

/// A categorical scale of the three statuses of a branch.
fn statuses() -> ColorScale {
    scale(ColorScaleKind::Categorical(CategoricalColorScale {
        categories: [
            (Color::rgb(0.0, 0.6, 0.3), "taken"),
            (Color::rgb(0.8, 0.2, 0.1), "not taken"),
            (Color::rgb(0.5, 0.5, 0.5), "unreachable"),
        ]
        .into_iter()
        .map(|(color, label)| ColorCategory {
            color,
            label: Text::plain(label),
        })
        .collect(),
        ..CategoricalColorScale::default()
    }))
}

/// A figure of one 2D axes (node 2) with automatic limits holding cells (node 3) of `values` (array 100) mapped
/// through `scale`, edited by `edit`, which may add arrays to the figure and artists to the axes.
fn figure_of(
    values: NdArray,
    scale: ColorScale,
    edit: impl FnOnce(&mut Cells, &mut Axes, &mut Vec<ironlab_ir::DataArray>),
) -> Figure {
    let mut data = vec![data_array(array_id(0), values)];
    let mut cells = Cells {
        id: NodeId(3),
        values: array_id(0),
        color_scale: Some(SCALE),
        ..Cells::default()
    };
    let mut axes = Axes {
        id: NodeId(2),
        ..Axes::default()
    };
    edit(&mut cells, &mut axes, &mut data);
    axes.artists.insert(0, Artist::Cells(cells));
    Figure {
        id: NodeId(1),
        data,
        axes: vec![axes],
        color_scales: vec![scale],
        ..Figure::new()
    }
}

/// A matrix of shape `[ny, nx]`.
fn matrix(ny: usize, nx: usize, values: Vec<f64>) -> NdArray {
    NdArray::from_shape(vec![ny, nx], values).expect("the shape matches the values")
}

/// The state of a freshly opened `figure` and the hit map of its compilation.
fn compiled(figure: Figure) -> (FigureState, HitMap) {
    let state = FigureState::new(figure);
    let hit = ironlab_scene::compile(state.figure(), &TEXT).hit_map;
    (state, hit)
}

/// The figure position of the data point `(x, y)` in the first axes of `hit`.
fn at_data(hit: &HitMap, x: f64, y: f64) -> Point {
    match &hit.axes[0].kind {
        AxesHitKind::TwoD { x: xm, y: ym } => Point::new(xm.to_figure(x), ym.to_figure(y)),
        AxesHitKind::ThreeD => panic!("the axes is three-dimensional"),
    }
}

/// The figure rectangle of the data rectangle `[x0, x1] × [y0, y1]`, as `(left, top, right, bottom)` in figure
/// space, whichever way the axes run.
fn page_box(hit: &HitMap, [x0, x1]: [f64; 2], [y0, y1]: [f64; 2]) -> [f64; 4] {
    let (a, b) = (at_data(hit, x0, y0), at_data(hit, x1, y1));
    [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)]
}

fn inside(p: Point, [left, top, right, bottom]: [f64; 4]) -> bool {
    let tol = 1e-6;
    p.x >= left - tol && p.x <= right + tol && p.y >= top - tol && p.y <= bottom + tol
}

/// Returns the cell datatip at the data point `(x, y)`, or a panic naming what lies there instead.
#[track_caller]
fn cell_tip(state: &FigureState, hit: &HitMap, x: f64, y: f64) -> CellDatatip {
    match state.tip_at(hit, at_data(hit, x, y)) {
        Some(Tip::Cell(tip)) => tip,
        other => panic!("a cell lies under ({x}, {y}), not {other:?}"),
    }
}

/// Labels the ticks of an axis at 0, 1, …, n − 1 with the given names, as `xcategories` and `ycategories` do.
fn name_ticks(axis: &mut ironlab_ir::Axis, names: &[&str]) {
    axis.ticks = Ticks::Values((0..names.len()).map(|k| k as f64).collect());
    axis.tick_labels = TickLabels::Text(names.iter().map(|&n| Text::plain(n)).collect());
}

// ---------------------------------------------------------------------------------------------------------------
// What a cell reports
// ---------------------------------------------------------------------------------------------------------------

// Why: a datatip is how a reader learns which cell is which and what it holds, so the cell under the pointer must be
// named by its row and column in the user's own array, with the edges it spans and the value the array holds, not
// anything read back from the page. The edges are uneven here, so a cell found by dividing the plot evenly would be
// the wrong one. The callout is anchored on the drawn cell, so that the canvas marks the cell that was read.
#[test]
fn the_cell_under_the_pointer_is_named_with_its_row_column_edges_and_value() {
    let (state, hit) = compiled(figure_of(
        matrix(2, 3, vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6]),
        continuous(),
        |cells, _, data| {
            cells.label = Some(Text::plain("Coverage"));
            data.push(data_array(
                array_id(1),
                NdArray::vector(vec![0.0, 1.0, 3.0, 7.0]),
            ));
            data.push(data_array(
                array_id(2),
                NdArray::vector(vec![0.0, 2.0, 3.0]),
            ));
            cells.columns = CellEdges::Edges(array_id(1));
            cells.rows = CellEdges::Edges(array_id(2));
        },
    ));
    let tip = cell_tip(&state, &hit, 2.0, 2.5);
    assert_eq!(tip.axes, NodeId(2));
    assert_eq!(tip.artist, NodeId(3));
    assert_eq!(tip.label.as_deref(), Some("Coverage"));
    assert_eq!(
        (tip.row, tip.column),
        (1, 1),
        "the cell between x 1 and 3 and y 2 and 3"
    );
    assert_eq!(tip.column_edges, (1.0, 3.0));
    assert_eq!(tip.row_edges, (2.0, 3.0));
    assert_eq!(
        tip.value,
        PixelValue::Value(0.5),
        "row 1, column 1 of a 2 × 3 array"
    );
    assert_eq!(
        (tip.row_name, tip.column_name),
        (None, None),
        "the axes are not labelled with text"
    );
    let cell = page_box(&hit, [1.0, 3.0], [2.0, 3.0]);
    assert!(
        inside(tip.position, cell),
        "the callout is anchored on the cell: {:?}",
        tip.position
    );
    let r = tip.rect;
    for corner in [Point::new(r.x, r.y), Point::new(r.right(), r.bottom())] {
        assert!(inside(corner, cell), "the outline lies on the cell: {r:?}");
    }
}

// Why: a cell reports what its array holds in the words of its scale, exactly as a pixel of a scalar image does, so
// that the two read alike: a status of an occupancy matrix names its category, an index names its entry, and a
// measurement is a value. Eight-bit statuses, the compact way to store them, are widened to the numbers they denote.
#[test]
fn a_cell_reports_its_value_in_the_words_of_its_scale() {
    let (state, hit) = compiled(figure_of(
        matrix(1, 3, vec![0.0, 1.0, 2.0]),
        statuses(),
        |_, _, _| {},
    ));
    assert_eq!(
        cell_tip(&state, &hit, 1.0, 0.0).value,
        PixelValue::Category {
            index: 1.0,
            label: Some("not taken".to_owned())
        }
    );

    let bytes = NdArray::from_shape_u8(vec![1, 3], vec![0, 1, 2]).expect("shape");
    let (state, hit) = compiled(figure_of(bytes, statuses(), |_, _, _| {}));
    assert_eq!(
        cell_tip(&state, &hit, 2.0, 0.0).value,
        PixelValue::Category {
            index: 2.0,
            label: Some("unreachable".to_owned())
        },
        "an 8-bit status is widened and named"
    );

    let palette = scale(ColorScaleKind::Indexed(IndexedColorScale {
        colors: vec![Color::rgb(1.0, 0.0, 0.0), Color::rgb(0.0, 0.0, 1.0)],
        ..IndexedColorScale::default()
    }));
    let (state, hit) = compiled(figure_of(
        matrix(1, 2, vec![1.0, 0.0]),
        palette,
        |_, _, _| {},
    ));
    assert_eq!(
        cell_tip(&state, &hit, 0.0, 0.0).value,
        PixelValue::Index(1.0)
    );

    let bands = scale(ColorScaleKind::Discrete(DiscreteColorScale {
        levels: ColorScaleLevels::Explicit {
            values: vec![0.0, 0.5, 1.0],
        },
        ..DiscreteColorScale::default()
    }));
    let (state, hit) = compiled(figure_of(
        matrix(1, 2, vec![0.25, 0.75]),
        bands,
        |_, _, _| {},
    ));
    assert_eq!(
        cell_tip(&state, &hit, 1.0, 0.0).value,
        PixelValue::Value(0.75),
        "a banded value is reported as stored, not as its band"
    );
}

// Why: an occupancy matrix places its things at 0, 1, 2, … and names them with text tick labels, so the indices mean
// nothing to the reader; hovering a cell must say which thing it is, by the name of its row (the subroutine) and of
// its column (the case), as well as its status. The names are the tick labels at the centre of the cell's row and
// column, on the y and the x axis, whichever way the axes run; a cell whose centre falls on no named tick has no name.
#[test]
fn a_cell_carries_the_names_of_its_row_and_column_from_the_text_ticks_of_both_axes() {
    let (state, hit) = compiled(figure_of(
        matrix(2, 3, vec![0.0, 1.0, 2.0, 2.0, 1.0, 0.0]),
        statuses(),
        |_, axes, _| {
            name_ticks(&mut axes.x, &["case_a", "case_b", "case_c"]);
            name_ticks(&mut axes.y, &["psilin", "ggcalc"]);
            axes.y.reverse = true;
        },
    ));
    let tip = cell_tip(&state, &hit, 2.0, 1.0);
    assert_eq!((tip.row, tip.column), (1, 2));
    assert_eq!(tip.row_name.as_deref(), Some("ggcalc"));
    assert_eq!(tip.column_name.as_deref(), Some("case_c"));
    assert_eq!(
        tip.value,
        PixelValue::Category {
            index: 0.0,
            label: Some("taken".to_owned())
        }
    );

    // The name is found by where the cell lies, not by its index: on edges 0, 1 and 3 the centres are 0.5, which no
    // tick names, and 2, which the tick "two" names, although the cell is column 1.
    let (state, hit) = compiled(figure_of(
        matrix(1, 2, vec![0.0, 1.0]),
        statuses(),
        |cells, axes, data| {
            data.push(data_array(
                array_id(1),
                NdArray::vector(vec![0.0, 1.0, 3.0]),
            ));
            cells.columns = CellEdges::Edges(array_id(1));
            name_ticks(&mut axes.x, &["zero", "one", "two"]);
        },
    ));
    let first = cell_tip(&state, &hit, 0.5, 0.0);
    assert_eq!(
        (first.column, first.column_name),
        (0, None),
        "no tick falls at 0.5"
    );
    let second = cell_tip(&state, &hit, 2.0, 0.0);
    assert_eq!(second.column, 1);
    assert_eq!(
        second.column_name.as_deref(),
        Some("two"),
        "the tick at the centre 2"
    );
    assert_eq!(
        second.row_name, None,
        "the y axis is not labelled with text"
    );
}

// Why: the gap is what sets cells apart, and a reader pointing into it is pointing between two things; a tip there
// would name a cell the reader is not looking at. The NaN padding of a ragged row is not a thing at all, so it is not
// drawn and reads nothing either, rather than reporting a missing value for a branch that does not exist.
#[test]
fn nothing_is_read_in_the_gap_between_cells_or_over_the_padding_of_a_ragged_row() {
    let nan = f64::NAN;
    let (state, hit) = compiled(figure_of(
        matrix(2, 3, vec![0.0, 1.0, 2.0, 1.0, nan, nan]),
        statuses(),
        |cells, _, _| {
            cells.gap = CellGap { x: 0.4, y: 0.4 };
            // Outlined, so that a hit recorded for every cell of the grid, rather than for every cell drawn, would
            // show here: the padding has neither face nor outline.
            cells.edge.color = ColorSpec::Rgba {
                color: Color::rgb(0.0, 0.0, 0.0),
            };
        },
    ));
    // Unit cells inset by 0.2 on each side span 0.3 either side of their centres, so 0.5 lies in a gap.
    assert!(
        state.tip_at(&hit, at_data(&hit, 0.5, 0.0)).is_none(),
        "between columns 0 and 1"
    );
    assert!(
        state.tip_at(&hit, at_data(&hit, 0.0, 0.5)).is_none(),
        "between rows 0 and 1"
    );
    assert!(
        state.tip_at(&hit, at_data(&hit, 1.0, 1.0)).is_none(),
        "over the padding"
    );
    let before = cell_tip(&state, &hit, 0.0, 1.0);
    assert_eq!(
        (before.row, before.column),
        (1, 0),
        "the cell before the padding is read"
    );
}

// Why: hiding cells by their legend entry removes them from the page; a datatip that still read them would report
// data the reader has hidden.
#[test]
fn hidden_cells_are_not_read() {
    let (state, hit) = compiled(figure_of(
        matrix(1, 2, vec![0.0, 1.0]),
        statuses(),
        |cells, axes, _| {
            cells.visible = false;
            axes.x.limits = Limits::Manual {
                min: -0.5,
                max: 1.5,
            };
            axes.y.limits = Limits::Manual {
                min: -0.5,
                max: 0.5,
            };
        },
    ));
    assert!(state.tip_at(&hit, at_data(&hit, 0.0, 0.0)).is_none());
}

// Why: the dot overlay of the case matrix of #74 marks some cells with points drawn over them; a reader pointing at
// a dot means the dot, which is small and painted on top, as a point wins over the bar or pixel beneath it. Away from
// any dot the cell is what is there.
#[test]
fn a_point_within_reach_is_preferred_over_the_cell_beneath_it() {
    let (state, hit) = compiled(figure_of(
        matrix(1, 3, vec![0.0, 1.0, 2.0]),
        statuses(),
        |_, axes, data| {
            data.push(data_array(array_id(1), NdArray::vector(vec![0.0, 1.0])));
            data.push(data_array(array_id(2), NdArray::vector(vec![0.0, 0.0])));
            axes.artists.push(Artist::Series(Series {
                id: NodeId(4),
                x: array_id(1),
                y: array_id(2),
                ..Series::default()
            }));
        },
    ));
    match state.tip_at(&hit, at_data(&hit, 1.0, 0.0)) {
        Some(Tip::Point(tip)) => assert_eq!(tip.artist, NodeId(4)),
        other => panic!("the dot is preferred over the cell beneath it, not {other:?}"),
    }
    let away = at_data(&hit, 2.0, 0.0);
    assert!(
        hit.sample_at(away, DATATIP_RADIUS_POINTS).is_none(),
        "precondition: no point within reach"
    );
    assert!(matches!(state.tip_at(&hit, away), Some(Tip::Cell(_))));
}

// ---------------------------------------------------------------------------------------------------------------
// Two-dimensional histograms: the count and density of a bin
// ---------------------------------------------------------------------------------------------------------------

// Why: a two-dimensional histogram normalised to probabilities, percentages or densities no longer shows the number
// of samples in a cell, nor the density that makes cells of unequal areas comparable. Its binning record holds what
// is needed to recover both from the value drawn: the normalisation, the total, and the area of the cell, its width
// times its height. The columns here are of widths 1, 2 and 3 and the single row of height 2, so that an area taken
// from the width alone, or from the wrong cell, would give other numbers. Cells that record no binning are a heatmap
// of values, which has no bins to count.
#[test]
fn a_cell_of_a_binned_two_dimensional_histogram_reports_the_count_and_density_of_its_bin() {
    use ironlab_canvas::interaction::BinReading;
    use ironlab_ir::{Binning2d, Bins, HistNorm};
    let columns = [0.0, 1.0, 3.0, 6.0];
    let counts = [4.0, 6.0, 2.0];
    let total = 12.0;
    let area = |k: usize| (columns[k + 1] - columns[k]) * 2.0;
    let automatic = || scale(ColorScaleKind::Continuous(ContinuousColorScale::default()));
    for norm in [
        HistNorm::Count,
        HistNorm::Probability,
        HistNorm::Percentage,
        HistNorm::CountDensity,
        HistNorm::Density,
    ] {
        let values: Vec<f64> = (0..3)
            .map(|k| match norm {
                HistNorm::Count => counts[k],
                HistNorm::Probability => counts[k] / total,
                HistNorm::Percentage => 100.0 * counts[k] / total,
                HistNorm::CountDensity => counts[k] / area(k),
                _ => counts[k] / (total * area(k)),
            })
            .collect();
        let (state, hit) = compiled(figure_of(
            matrix(1, 3, values),
            automatic(),
            |cells, _, data| {
                data.push(data_array(array_id(1), NdArray::vector(columns.to_vec())));
                data.push(data_array(array_id(2), NdArray::vector(vec![0.0, 2.0])));
                cells.columns = CellEdges::Edges(array_id(1));
                cells.rows = CellEdges::Edges(array_id(2));
                cells.binning = Some(Binning2d {
                    x_bins: Bins::Edges(columns.to_vec()),
                    y_bins: Bins::Edges(vec![0.0, 2.0]),
                    x_range: None,
                    y_range: None,
                    normalization: norm,
                    min_count: None,
                    total,
                    excluded_non_finite: 0,
                    excluded_out_of_range: 0,
                });
            },
        ));
        for k in 0..3 {
            let tip = cell_tip(&state, &hit, (columns[k] + columns[k + 1]) / 2.0, 1.0);
            assert_eq!(tip.column, k, "{norm:?}");
            let BinReading { count, density } = tip
                .bin
                .unwrap_or_else(|| panic!("{norm:?}: cell {k} gives no reading of its bin"));
            assert_close(
                count,
                counts[k],
                1e-9,
                &format!("{norm:?}, cell {k}: count"),
            );
            assert_close(
                density,
                counts[k] / (total * area(k)),
                1e-9,
                &format!("{norm:?}, cell {k}: density"),
            );
        }
    }

    let (state, hit) = compiled(figure_of(
        matrix(1, 1, vec![0.25]),
        continuous(),
        |_, _, _| {},
    ));
    assert_eq!(cell_tip(&state, &hit, 0.0, 0.0).bin, None);
}

// ---------------------------------------------------------------------------------------------------------------
// The text of the callout
// ---------------------------------------------------------------------------------------------------------------

// Why: the text is all a reader gets from hovering a cell, so it must say which thing the cell is and what state it
// is in: the label of the cells first, as a series' label comes first, then the names of the row and the column when
// the axes give them, then the value in the words a pixel of a scalar image uses for it (`category = … (index …)`,
// `index = …`, `value = …`), so that the two callouts read alike, and the row and column in the user's array.
#[test]
fn the_callout_text_names_the_cells_their_row_and_column_and_the_category() {
    let (state, hit) = compiled(figure_of(
        matrix(2, 2, vec![0.0, 1.0, 2.0, 1.0]),
        statuses(),
        |cells, axes, _| {
            cells.label = Some(Text::plain("Branches"));
            name_ticks(&mut axes.x, &["case_a", "case_b"]);
            name_ticks(&mut axes.y, &["psilin", "ggcalc"]);
        },
    ));
    let tip = cell_tip(&state, &hit, 1.0, 1.0);
    let text = cell_datatip_text(&tip);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("Branches"),
        "the label comes first: {text:?}"
    );
    assert!(text.contains("ggcalc"), "the text names the row: {text:?}");
    assert!(
        text.contains("case_b"),
        "the text names the column: {text:?}"
    );
    assert!(
        lines.contains(&"category = not taken (index 1)"),
        "the status reads as a categorical pixel's does: {text:?}"
    );
    assert!(
        text.contains("row 1, column 1"),
        "and the place in the array: {text:?}"
    );

    let (state, hit) = compiled(figure_of(
        matrix(1, 1, vec![0.25]),
        continuous(),
        |_, _, _| {},
    ));
    let text = cell_datatip_text(&cell_tip(&state, &hit, 0.0, 0.0));
    assert!(
        text.lines().any(|line| line == "value = 0.25"),
        "a measurement reads as a pixel's value does: {text:?}"
    );
}
