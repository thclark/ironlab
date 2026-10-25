//! Datatips on bars and stairs, tested without a GPU or a window.
//!
//! These tests compile the figure, because what they are about is the agreement between where the scene compiler drew
//! a bar or a step and what the viewer reports for a pointer over it: the bar under the pointer, its index in the
//! user's arrays, its position, value and base, the segment a stacked bar occupies, and the name of its category
//! when the category axis is labelled with text.
//!
//! The API these tests assume, beyond what `interaction.rs` exercises:
//!
//! ```text
//! // crates/ironlab-canvas/src/interaction.rs
//! pub enum Tip { Point(Datatip), Pixel(PixelDatatip), Colorbar(ColorbarTip), Bar(BarDatatip), Step(StepDatatip) }
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
//!     /// The position of the bar along the category axis, as its data holds it.
//!     pub at: f64,
//!     /// The value of the bar, as its data holds it.
//!     pub value: f64,
//!     /// The base the bar rises from: the constant base, or the bar's own base.
//!     pub base: f64,
//!     /// For a member of a stack, the bottom and top of the segment the bar occupies, along the value axis.
//!     pub segment: Option<(f64, f64)>,
//!     /// The text tick label of the category axis at the bar's position, when the axis is labelled with text and a
//!     /// tick falls exactly there: the name of the bar's category.
//!     pub tick_label: Option<String>,
//! }
//!
//! /// The step of stairs under the pointer.
//! pub struct StepDatatip {
//!     pub axes: NodeId,
//!     pub artist: NodeId,
//!     pub label: Option<String>,
//!     /// The index of the step's value in the artist's array of values.
//!     pub index: usize,
//!     /// Where a callout is anchored, in figure space: a point of the drawn step.
//!     pub position: Point,
//!     /// The two edges of the step, in the order of the data.
//!     pub edges: (f64, f64),
//!     /// The value held across the step.
//!     pub value: f64,
//! }
//! ```

mod common;

use common::*;
use ironlab_canvas::interaction::Tip;
use ironlab_canvas::{DATATIP_RADIUS_POINTS, FigureState};

use ironlab_ir::{
    Artist, Bar, BarGroup, BarLayout, Figure, NdArray, NodeId, Orientation, Series, Stairs,
    StairsData, StepAt, Text, TickLabels, Ticks,
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

/// A bar series (node `id`) at the positions in array `positions` rising to the values in array `values`.
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
// index in the user's own arrays and with the position, value and base the data holds, not values read back from
// the page; a negative bar hangs below the base and must be read there too. The callout is anchored on the bar
// itself, so that the canvas marks the bar that was read.
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
            assert_eq!(
                tip.segment, None,
                "a bar on its own is not a segment of a stack"
            );
            assert_eq!(tip.tick_label, None, "the axis is not labelled with text");
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
    // The bars at 2 and 5 span [0.8, 3.2] and [3.8, 6.2], so 3.5 lies in the gap between them.
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

// Why: in a stack the value of a part is not where it ends on the axis, so the datatip must give the part's own
// value and, separately, the bottom and top of the segment it occupies, which is what the reader sees; otherwise the
// reader would have to subtract the parts below to get the number the data holds.
#[test]
fn a_stacked_bar_reports_its_own_value_and_the_segment_it_occupies() {
    let stacked = |id, values| {
        Artist::Bar(Bar {
            group: Some(BarGroup {
                name: "parts".to_owned(),
                layout: BarLayout::Stacked,
            }),
            ..bar(id, 0, values)
        })
    };
    let (state, hit) = compiled(figure_of(
        vec![vec![2.0, 5.0], vec![1.0, 1.5], vec![2.0, 3.0]],
        vec![stacked(3, 1), stacked(4, 2)],
    ));
    match state.tip_at(&hit, at_data(&hit, 2.0, 2.0)) {
        Some(Tip::Bar(tip)) => {
            assert_eq!(
                (tip.artist, tip.index),
                (NodeId(4), 0),
                "the upper part at 2"
            );
            assert_eq!(tip.value, 2.0, "its own value, not its top");
            assert_eq!(tip.segment, Some((1.0, 3.0)), "on top of the lower part");
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
            assert_eq!(tip.segment, Some((0.0, 1.5)));
        }
        other => panic!("the lower part lies under the pointer, not {other:?}"),
    }
}

// Why: a categorical bar chart places its bars at 0, 1, 2, … and names them with text tick labels, so the number the
// position holds means nothing to the reader; the datatip must carry the name of the category, which is the tick
// label at the bar's position on its category axis: x for upright bars and y for bars lying flat. A bar between
// ticks belongs to no named category.
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
