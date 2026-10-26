//! Datatips on bars and stairs, tested without a GPU or a window.
//!
//! These tests compile the figure, because what they are about is the agreement between where the scene compiler drew
//! a bar or a step and what the viewer reports for a pointer over it: the bar under the pointer, its index in the
//! user's arrays, its position, length and base, the segment it occupies from its base to its end, the name of its
//! category when the category axis is labelled with text, and, for the bars or steps of a histogram that records its
//! binning, the count of the bin and its density.
//!
//! The API these tests assume, beyond what `interaction.rs` exercises:
//!
//! ```text
//! // crates/ironlab-canvas/src/interaction.rs
//! pub enum Tip { Point(Datatip), Pixel(PixelDatatip), Colorbar(ColorbarTip), Bar(BarDatatip), Step(StepDatatip),
//!     Cell(CellDatatip) }
//!
//! /// The bar under the pointer, as the viewer reports it.
//! pub struct BarDatatip {
//!     pub axes: NodeId,
//!     pub artist: NodeId,
//!     /// The label of the bar series, when it has one.
//!     pub label: Option<String>,
//!     /// The index of the bar in the artist's own arrays.
//!     pub index: usize,
//!     /// Where a callout is anchored, in figure space: a point of the drawn bar.
//!     pub position: Point,
//!     /// The rectangle of the bar as drawn, in figure space.
//!     pub rect: Rect,
//!     /// The position of the bar along the category axis, as its data holds it, without its offset.
//!     pub at: f64,
//!     /// The value of the bar, as its data holds it: the length of the bar from its base.
//!     pub value: f64,
//!     /// The base the bar starts from: the constant base, or the bar's own base.
//!     pub base: f64,
//!     /// The start and end of the bar along the value axis: `(base, base + value)`.
//!     pub segment: (f64, f64),
//!     /// The text tick label of the category axis at the bar's position, when the axis is labelled with text and a
//!     /// tick falls exactly there: the name of the bar's category.
//!     pub tick_label: Option<String>,
//!     /// The count and density of the bin the bar draws, when the series records the binning of a histogram.
//!     pub bin: Option<BinReading>,
//! }
//!
//! /// The step of stairs under the pointer; it gains `bin` as a bar does.
//! pub struct StepDatatip { …, pub edges: (f64, f64), pub value: f64, pub bin: Option<BinReading> }
//!
//! /// The cell under the pointer; it gains `bin` as a bar does, its area the cell's width times its height.
//! pub struct CellDatatip { …, pub bin: Option<BinReading> }
//!
//! /// What a bin of a histogram holds, recovered from the value drawn, the extent of the bin, and the normalisation
//! /// and total of the binning record; a cumulative value is first differenced with that of the bin before it.
//! pub struct BinReading {
//!     /// The number of samples in the bin.
//!     pub count: f64,
//!     /// The probability density of the bin: its count divided by the total and by its width (or area).
//!     pub density: f64,
//! }
//! ```

mod common;

use common::*;
use ironlab_canvas::interaction::{BinReading, Tip};
use ironlab_canvas::{DATATIP_RADIUS_POINTS, FigureState};

use ironlab_ir::{
    Artist, Bar, BarOffset, BarWidth, Binning, Bins, Figure, HistNorm, Limits, NdArray, NodeId,
    Orientation, PerBar, Series, Stairs, StairsData, StepAt, Text, TickLabels, Ticks,
};
use ironlab_scene::display::Point;
use ironlab_scene::hit::{AxesHitKind, HitMap};

// ---------------------------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------------------------

/// A figure of one 2D axes (node 2) with manual limits x ∈ [0, 10] and y ∈ [−2, 5], holding `artists` over `data`,
/// whose arrays take the identifiers 100, 101, … in the order given.
fn figure_of(data: Vec<Vec<f64>>, artists: Vec<Artist>) -> Figure {
    let mut axes = axes_2d(2);
    axes.y.limits = manual(-2.0, 5.0);
    axes.artists = artists;
    Figure {
        id: NodeId(1),
        data: data
            .into_iter()
            .enumerate()
            .map(|(k, values)| data_array(array_id(k as u64), NdArray::vector(values)))
            .collect(),
        axes: vec![axes],
        ..Figure::new()
    }
}

/// The state of a freshly opened `figure` and the hit map of its compilation.
fn compiled(figure: Figure) -> (FigureState, HitMap) {
    let state = FigureState::new(figure);
    let hit = ironlab_scene::compile(state.figure(), &TEXT).hit_map;
    (state, hit)
}

/// A bar series (node `id`) at the positions in array `positions` of the lengths in array `values`.
fn bar(id: u64, positions: u64, values: u64) -> Bar {
    Bar {
        id: NodeId(id),
        positions: array_id(positions),
        values: array_id(values),
        ..Bar::default()
    }
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

// ---------------------------------------------------------------------------------------------------------------
// Bars
// ---------------------------------------------------------------------------------------------------------------

// Why: a datatip is how a reader gets the number behind a bar, so the bar under the pointer must be named with its
// index in the user's own arrays and with the position, length and base the data holds, not values read back from
// the page, together with the segment it spans from its base to its end; a negative bar hangs below the base and
// must be read there too. The callout is anchored on the bar itself, so that the canvas marks the bar that was read,
// and a bar of a plain chart carries no reading of a bin.
#[test]
fn the_bar_under_the_pointer_is_named_with_its_index_position_value_and_base() {
    let (state, hit) = compiled(figure_of(
        vec![vec![2.0, 5.0, 8.0], vec![3.0, -1.0, 2.0]],
        vec![Artist::Bar(Bar {
            label: Some(Text::plain("Revenue")),
            ..bar(3, 0, 1)
        })],
    ));
    // The pitch is 3, so the bar at 5 spans 3.8 to 6.2 and runs from 0 down to −1.
    match state.tip_at(&hit, at_data(&hit, 5.5, -0.5)) {
        Some(Tip::Bar(tip)) => {
            assert_eq!(tip.axes, NodeId(2));
            assert_eq!(tip.artist, NodeId(3));
            assert_eq!(tip.label.as_deref(), Some("Revenue"));
            assert_eq!(tip.index, 1);
            assert_eq!(tip.at, 5.0);
            assert_eq!(tip.value, -1.0);
            assert_eq!(tip.base, 0.0);
            assert_eq!(tip.segment, (0.0, -1.0), "from the base to its end");
            assert_eq!(tip.tick_label, None, "the axis is not labelled with text");
            assert_eq!(tip.bin, None, "a plain bar chart records no binning");
            assert!(
                inside(tip.position, page_box(&hit, [3.8, 6.2], [-1.0, 0.0])),
                "the callout is anchored on the bar: {:?}",
                tip.position
            );
        }
        other => panic!("the bar at 5 lies under the pointer, not {other:?}"),
    }
}

// Why: a reader pointing between bars, or above the end of a bar, is not pointing at a bar; a tip there would name a
// bar the reader is not looking at, and the gap between bars is exactly where a reader points to compare two.
#[test]
fn nothing_is_read_in_the_gap_between_bars_or_beyond_the_end_of_a_bar() {
    let (state, hit) = compiled(figure_of(
        vec![vec![2.0, 5.0, 8.0], vec![3.0, 1.0, 2.0]],
        vec![Artist::Bar(bar(3, 0, 1))],
    ));
    // The bars at 2 and 5 span [1.175, 2.825] and [4.175, 5.825], so 3.5 lies in the gap between them.
    assert!(
        state.tip_at(&hit, at_data(&hit, 3.5, 0.5)).is_none(),
        "in the gap"
    );
    assert!(
        state.tip_at(&hit, at_data(&hit, 5.0, 4.0)).is_none(),
        "above a bar of value 1"
    );
}

// Why: hiding a series by its legend entry removes its bars from the page; a datatip that still read them would
// report data the reader has hidden.
#[test]
fn a_hidden_bar_series_is_not_read() {
    let (state, hit) = compiled(figure_of(
        vec![vec![5.0], vec![3.0]],
        vec![Artist::Bar(Bar {
            visible: false,
            ..bar(3, 0, 1)
        })],
    ));
    assert!(state.tip_at(&hit, at_data(&hit, 5.0, 1.0)).is_none());
}

// Why: a value is a length from the base, so a bar that rests on another, as a part of a stack does, or floats on a
// base of its own, ends somewhere other than its value; the datatip must give the bar's own value, its base and the
// segment from base to end that the reader sees, or the reader would have to add the base to the value to find
// where the bar ends, or subtract it to find the number the data holds.
#[test]
fn a_bar_on_a_base_reports_its_own_value_its_base_and_the_segment_it_spans() {
    let (state, hit) = compiled(figure_of(
        vec![
            vec![2.0, 5.0],
            vec![1.0, 1.5],
            vec![2.0, 3.0],
            vec![1.0, 1.5],
        ],
        vec![
            Artist::Bar(bar(3, 0, 1)),
            Artist::Bar(Bar {
                base: PerBar::Data(array_id(3)),
                ..bar(4, 0, 2)
            }),
        ],
    ));
    match state.tip_at(&hit, at_data(&hit, 2.0, 2.0)) {
        Some(Tip::Bar(tip)) => {
            assert_eq!(
                (tip.artist, tip.index),
                (NodeId(4), 0),
                "the upper part at 2"
            );
            assert_eq!(tip.value, 2.0, "its own value, not its end");
            assert_eq!(tip.base, 1.0, "its own base");
            assert_eq!(tip.segment, (1.0, 3.0), "on top of the lower part");
        }
        other => panic!("the upper part lies under the pointer, not {other:?}"),
    }
    match state.tip_at(&hit, at_data(&hit, 5.0, 0.5)) {
        Some(Tip::Bar(tip)) => {
            assert_eq!(
                (tip.artist, tip.index),
                (NodeId(3), 1),
                "the lower part at 5"
            );
            assert_eq!(tip.value, 1.5);
            assert_eq!(tip.segment, (0.0, 1.5));
        }
        other => panic!("the lower part lies under the pointer, not {other:?}"),
    }
}

// Why: a categorical bar chart places its bars at 0, 1, 2, … and names them with text tick labels, so the number the
// position holds means nothing to the reader; the datatip must carry the name of the category, which is the tick
// label at the bar's position on its category axis: x for upright bars and y for bars lying flat. A bar between
// ticks belongs to no named category, and a bar drawn beside its position by an offset still belongs to the category
// of its position.
#[test]
fn a_bar_on_a_labelled_tick_of_its_category_axis_carries_the_name_of_its_category() {
    let mut figure = figure_of(
        vec![
            vec![2.0, 5.0, 8.0],
            vec![3.0, 1.0, 2.0],
            vec![6.5],
            vec![4.0],
        ],
        vec![Artist::Bar(bar(3, 0, 1)), Artist::Bar(bar(4, 2, 3))],
    );
    let axes = &mut figure.axes[0];
    axes.x.ticks = Ticks::Values(vec![2.0, 5.0, 8.0]);
    axes.x.tick_labels = TickLabels::Text(vec![
        Text::plain("North"),
        Text::plain("South"),
        Text::plain("East"),
    ]);
    let (state, hit) = compiled(figure);
    match state.tip_at(&hit, at_data(&hit, 5.0, 0.5)) {
        Some(Tip::Bar(tip)) => assert_eq!(tip.tick_label.as_deref(), Some("South")),
        other => panic!("the bar at 5 lies under the pointer, not {other:?}"),
    }
    match state.tip_at(&hit, at_data(&hit, 6.5, 3.5)) {
        Some(Tip::Bar(tip)) => {
            assert_eq!(tip.artist, NodeId(4));
            assert_eq!(tip.tick_label, None, "no tick falls at 6.5");
        }
        other => panic!("the bar at 6.5 lies under the pointer, not {other:?}"),
    }

    // A series of a grouped chart is drawn beside its position, offset by a width, yet it belongs to the category
    // of its position, so it is named by the tick at its position, not by the place it is drawn at.
    let mut figure = figure_of(
        vec![vec![2.0, 5.0, 8.0], vec![3.0, 1.0, 2.0]],
        vec![Artist::Bar(Bar {
            width: BarWidth::Pitch(0.4),
            offset: BarOffset::Widths(1.0),
            ..bar(3, 0, 1)
        })],
    );
    let axes = &mut figure.axes[0];
    axes.x.ticks = Ticks::Values(vec![2.0, 5.0, 8.0]);
    axes.x.tick_labels = TickLabels::Text(vec![
        Text::plain("North"),
        Text::plain("South"),
        Text::plain("East"),
    ]);
    let (state, hit) = compiled(figure);
    // The pitch is 3, so the bar at 5 is 1.2 wide and centred at 6.2.
    match state.tip_at(&hit, at_data(&hit, 6.2, 0.5)) {
        Some(Tip::Bar(tip)) => {
            assert_eq!((tip.index, tip.at), (1, 5.0), "the bar of position 5");
            assert_eq!(
                tip.tick_label.as_deref(),
                Some("South"),
                "named by the tick at its position"
            );
        }
        other => panic!("the offset bar at 5 lies under the pointer, not {other:?}"),
    }

    let mut figure = figure_of(
        vec![vec![1.0, 3.0], vec![4.0, 2.0]],
        vec![Artist::Bar(Bar {
            orientation: Orientation::Horizontal,
            ..bar(3, 0, 1)
        })],
    );
    let axes = &mut figure.axes[0];
    axes.y.ticks = Ticks::Values(vec![1.0, 3.0]);
    axes.y.tick_labels = TickLabels::Text(vec![Text::plain("Wing"), Text::plain("Fuselage")]);
    let (state, hit) = compiled(figure);
    match state.tip_at(&hit, at_data(&hit, 1.0, 3.0)) {
        Some(Tip::Bar(tip)) => {
            assert_eq!(tip.index, 1);
            assert_eq!(
                tip.tick_label.as_deref(),
                Some("Fuselage"),
                "from the y axis"
            );
        }
        other => panic!("the flat bar at 3 lies under the pointer, not {other:?}"),
    }
}

// Why: a line drawn over bars is small and painted on top, so a reader pointing at one of its points means the
// point, as a point wins over the pixel beneath it; away from any point the bar is what is there.
#[test]
fn a_point_within_reach_is_preferred_over_the_bar_beneath_it() {
    let (state, hit) = compiled(figure_of(
        vec![vec![5.0], vec![4.0], vec![5.0, 9.0], vec![2.0, 2.0]],
        vec![
            Artist::Bar(bar(3, 0, 1)),
            Artist::Series(Series {
                id: NodeId(4),
                x: array_id(2),
                y: array_id(3),
                ..Series::default()
            }),
        ],
    ));
    let on_point = at_data(&hit, 5.0, 2.0);
    match state.tip_at(&hit, on_point) {
        Some(Tip::Point(tip)) => assert_eq!(tip.artist, NodeId(4)),
        other => panic!("the point is preferred over the bar beneath it, not {other:?}"),
    }
    let away = at_data(&hit, 5.0, 0.5);
    assert!(
        hit.sample_at(away, DATATIP_RADIUS_POINTS).is_none(),
        "precondition: no point within reach"
    );
    assert!(matches!(state.tip_at(&hit, away), Some(Tip::Bar(_))));
}

// ---------------------------------------------------------------------------------------------------------------
// Histograms: the count and density of a bin
// ---------------------------------------------------------------------------------------------------------------

/// The bins of every histogram here: [0, 1], [1, 3] and [3, 6], of widths 1, 2 and 3.
const EDGES: [f64; 4] = [0.0, 1.0, 3.0, 6.0];
/// The counts of the three bins.
const COUNTS: [f64; 3] = [4.0, 6.0, 2.0];
/// The number of samples counted.
const TOTAL: f64 = 12.0;

/// The value that a histogram of [`COUNTS`] in the bins of [`EDGES`] draws for each bin under `norm`, written out
/// from the definitions of the normalisations rather than taken from the code under test.
fn normalised(norm: HistNorm) -> Vec<f64> {
    let widths: Vec<f64> = EDGES.windows(2).map(|w| w[1] - w[0]).collect();
    let mut cumulative = 0.0;
    COUNTS
        .iter()
        .zip(&widths)
        .map(|(&count, &width)| {
            cumulative += count;
            match norm {
                HistNorm::Count => count,
                HistNorm::Probability => count / TOTAL,
                HistNorm::Percentage => 100.0 * count / TOTAL,
                HistNorm::CountDensity => count / width,
                HistNorm::Density => count / (TOTAL * width),
                HistNorm::CumulativeCount => cumulative,
                HistNorm::Cdf => cumulative / TOTAL,
            }
        })
        .collect()
}

/// The reading each bin must give: its count, and its count over the total and its width.
fn expected_reading(k: usize) -> BinReading {
    let width = EDGES[k + 1] - EDGES[k];
    BinReading {
        count: COUNTS[k],
        density: COUNTS[k] / (TOTAL * width),
    }
}

/// The binning record of the histogram under `norm`, whose resolved edges, [`EDGES`], are the array `edges`.
fn record(norm: HistNorm, edges: NodeId) -> Binning {
    Binning {
        bins: Bins::Edges(EDGES.to_vec()),
        edges,
        range: None,
        normalization: norm,
        total: TOTAL,
        excluded_non_finite: 0,
        excluded_out_of_range: 0,
    }
}

const EVERY_NORM: [HistNorm; 7] = [
    HistNorm::Count,
    HistNorm::Probability,
    HistNorm::Percentage,
    HistNorm::CountDensity,
    HistNorm::Density,
    HistNorm::CumulativeCount,
    HistNorm::Cdf,
];

#[track_caller]
fn assert_reading(found: Option<BinReading>, expected: BinReading, what: &str) {
    let found = found.unwrap_or_else(|| panic!("{what}: no reading of the bin"));
    assert_close(found.count, expected.count, 1e-9, &format!("{what}: count"));
    assert_close(
        found.density,
        expected.density,
        1e-9,
        &format!("{what}: density"),
    );
}

// Why: a histogram normalised to probabilities, percentages or densities, or drawn cumulatively, no longer shows the
// number of samples in a bin, which is what a reader most often wants to know, nor the density that makes bins of
// unequal widths comparable. Its binning record holds what is needed to recover both from the value drawn: the
// normalisation, the total, and the width of the bin between the edges the record holds; a cumulative value is first
// differenced with that of the bin before it. Every normalisation must give back the same counts and densities, on
// bins of unequal widths so that a width taken from the wrong bin or from the pitch would be caught, and the bin of
// the middle of three so that a cumulative value that is not differenced would be caught too.
#[test]
fn a_bar_of_a_binned_histogram_reports_the_count_and_density_of_its_bin_for_every_normalisation() {
    for norm in EVERY_NORM {
        let values = normalised(norm);
        let mut figure = figure_of(
            vec![
                vec![0.5, 2.0, 4.5],
                values.clone(),
                vec![1.0, 2.0, 3.0],
                EDGES.to_vec(),
            ],
            vec![Artist::Bar(Bar {
                width: BarWidth::Data(array_id(2)),
                binning: Some(record(norm, array_id(3))),
                ..bar(3, 0, 1)
            })],
        );
        figure.axes[0].y.limits = Limits::Auto;
        let (state, hit) = compiled(figure);
        for k in 0..3 {
            let centre = (EDGES[k] + EDGES[k + 1]) / 2.0;
            match state.tip_at(&hit, at_data(&hit, centre, values[k] / 2.0)) {
                Some(Tip::Bar(tip)) => {
                    assert_eq!(tip.index, k, "{norm:?}");
                    assert_reading(tip.bin, expected_reading(k), &format!("{norm:?}, bin {k}"));
                }
                other => panic!("{norm:?}: bin {k} lies under the pointer, not {other:?}"),
            }
        }
    }
}

// Why: stairs are the other way a histogram is drawn, as an outline over its bin edges, and a reader pointing at a
// step wants the same numbers as for a bar: the count of the bin and its density, recovered from the step's value,
// the width between its two edges and the binning record, for every normalisation alike.
#[test]
fn a_step_of_binned_stairs_reports_the_count_and_density_of_its_bin_for_every_normalisation() {
    for norm in EVERY_NORM {
        let values = normalised(norm);
        let mut figure = figure_of(
            vec![EDGES.to_vec(), values.clone()],
            vec![Artist::Stairs(Stairs {
                id: NodeId(3),
                data: StairsData::Edges {
                    edges: array_id(0),
                    values: array_id(1),
                },
                binning: Some(record(norm, array_id(0))),
                ..Stairs::default()
            })],
        );
        figure.axes[0].y.limits = Limits::Auto;
        let (state, hit) = compiled(figure);
        for k in 0..3 {
            let centre = (EDGES[k] + EDGES[k + 1]) / 2.0;
            match state.tip_at(&hit, at_data(&hit, centre, values[k])) {
                Some(Tip::Step(tip)) => {
                    assert_eq!(tip.index, k, "{norm:?}");
                    assert_reading(tip.bin, expected_reading(k), &format!("{norm:?}, step {k}"));
                }
                other => panic!("{norm:?}: step {k} lies under the pointer, not {other:?}"),
            }
        }
    }
    // Stairs that record no binning are a step plot of values, which have no bins to count.
    let mut figure = figure_of(
        vec![EDGES.to_vec(), COUNTS.to_vec()],
        vec![stairs(StairsData::Edges {
            edges: array_id(0),
            values: array_id(1),
        })],
    );
    figure.axes[0].y.limits = Limits::Auto;
    let (state, hit) = compiled(figure);
    match state.tip_at(&hit, at_data(&hit, 2.0, COUNTS[1])) {
        Some(Tip::Step(tip)) => assert_eq!(tip.bin, None),
        other => panic!("the second step lies under the pointer, not {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Stairs
// ---------------------------------------------------------------------------------------------------------------

/// Stairs (node 3) of the given data, labelled "Counts".
fn stairs(data: StairsData) -> Artist {
    Artist::Stairs(Stairs {
        id: NodeId(3),
        label: Some(Text::plain("Counts")),
        data,
        ..Stairs::default()
    })
}

// Why: the step of a histogram outline is read as a bin and its count, so the datatip names the two edges of the bin
// under the pointer and the value held across it, as the data holds them; the reader points at the level of the
// step, which is where it was drawn.
#[test]
fn the_step_under_the_pointer_is_named_with_its_edges_and_value() {
    let (state, hit) = compiled(figure_of(
        vec![vec![0.0, 1.0, 3.0, 4.0], vec![2.0, 4.5, 1.0]],
        vec![stairs(StairsData::Edges {
            edges: array_id(0),
            values: array_id(1),
        })],
    ));
    match state.tip_at(&hit, at_data(&hit, 2.0, 4.5)) {
        Some(Tip::Step(tip)) => {
            assert_eq!(tip.axes, NodeId(2));
            assert_eq!(tip.artist, NodeId(3));
            assert_eq!(tip.label.as_deref(), Some("Counts"));
            assert_eq!(tip.index, 1);
            assert_eq!(tip.edges, (1.0, 3.0));
            assert_eq!(tip.value, 4.5);
        }
        other => panic!("the second step lies under the pointer, not {other:?}"),
    }
    // Far from any step of the open outline, nothing is read.
    let well_below = at_data(&hit, 2.0, -1.5);
    assert!(
        (well_below.y - at_data(&hit, 2.0, 1.0).y).abs() > 2.0 * DATATIP_RADIUS_POINTS,
        "precondition: the pointer is well away from every level"
    );
    assert!(state.tip_at(&hit, well_below).is_none());
}

// Why: stairs through samples step where `step` says, so the bin a sample's value holds across depends on it; with
// `post` the value at a sample holds up to the next sample, and the datatip must report that interval, not the one
// before it.
#[test]
fn a_step_of_stairs_through_samples_reports_the_interval_its_value_holds_across() {
    let (state, hit) = compiled(figure_of(
        vec![vec![0.0, 1.0, 2.0, 4.0], vec![1.0, 3.0, 2.0, 4.0]],
        vec![stairs(StairsData::Samples {
            x: array_id(0),
            values: array_id(1),
            step: StepAt::Post,
        })],
    ));
    match state.tip_at(&hit, at_data(&hit, 1.5, 3.0)) {
        Some(Tip::Step(tip)) => {
            assert_eq!(tip.index, 1);
            assert_eq!(tip.edges, (1.0, 2.0));
            assert_eq!(tip.value, 3.0);
        }
        other => panic!("the step of the second sample lies under the pointer, not {other:?}"),
    }
}
