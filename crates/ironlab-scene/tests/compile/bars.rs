//! Bars (MATLAB's and Matplotlib's `bar` and `barh`): where each rectangle is drawn, how the members of a group share
//! or stack on a position, what colours the bars take, and the automatic limits they set.
//!
//! A bar is read by where its ends are and how wide it is, so every geometric test maps the drawn rectangle back
//! through the axis maps of its axes, as `surfaces_2d` does for faces, and compares it in data units with what the
//! figure says. The arithmetic of the expected rectangles is written out in each test rather than taken from the
//! compiler, so that the expectation does not depend on the code under test.

use ironlab_ir::{
    Artist, BarBase, BarGroup, BarLayout, BarSlot, Color, ColorScaleKind, ColorSpec,
    ContinuousColorScale, IndexedColorScale, Limits, NodeId, Orientation, Scale,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{Item, ItemKind, Rect, Rgba, Stroke};

use crate::common::{COLOUR_ORDER, Fx, compile_figure, rgb8, rgb8_close, rgba};
use crate::probe::{assert_close, axis_maps, from_source, leaves};

/// Agreement expected of a rectangle mapped from data to the page and back.
const TOL: f64 = 1e-9;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

// ---------------------------------------------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------------------------------------------

/// A bar as it was drawn: its rectangle on the page, the same rectangle as data along x and along y (each as
/// `[low, high]`), and how it was painted.
///
/// A bar may be drawn as one path that is both filled and stroked, or as a filled path and a stroked path over the
/// same rectangle; the two are merged into one `Drawn` here, so that the tests do not depend on that choice.
#[derive(Clone, Debug)]
struct Drawn {
    page: Rect,
    x: [f64; 2],
    y: [f64; 2],
    fill: Option<Rgba>,
    stroke: Option<Stroke>,
}

fn sorted2(a: f64, b: f64) -> [f64; 2] {
    if a <= b { [a, b] } else { [b, a] }
}

fn same_rect(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() < 1e-6
        && (a.y - b.y).abs() < 1e-6
        && (a.width - b.width).abs() < 1e-6
        && (a.height - b.height).abs() < 1e-6
}

/// Returns the bars that `bar` drew in the 2D axes `ax`, in paint order. Every path it drew must be one closed
/// axis-aligned rectangle, because that is what a bar is.
fn drawn(scene: &Scene, ax: NodeId, bar: NodeId) -> Vec<Drawn> {
    let (xmap, ymap) = axis_maps(scene, ax);
    let mut out: Vec<Drawn> = Vec::new();
    for leaf in from_source(&leaves(scene), bar) {
        let Some(path) = leaf.path() else {
            panic!("a bar is drawn as paths, found {:?}", leaf.kind);
        };
        let subpaths = leaf.subpaths();
        assert_eq!(
            subpaths.len(),
            1,
            "a bar is one closed outline: {subpaths:?}"
        );
        let corners = &subpaths[0];
        assert_eq!(corners.len(), 4, "a bar is a rectangle: {corners:?}");
        assert!(
            (0..4).all(|k| {
                let (a, b) = (corners[k], corners[(k + 1) % 4]);
                (a.x - b.x).abs() < 1e-9 || (a.y - b.y).abs() < 1e-9
            }),
            "a bar is an axis-aligned rectangle: {corners:?}"
        );
        let page = leaf.bbox().expect("a path has a box");
        let fill = path.fill.map(|f| f.color);
        let stroke = path.stroke.clone();
        if let Some(same) = out.iter_mut().find(|d| same_rect(d.page, page)) {
            same.fill = same.fill.or(fill);
            same.stroke = same.stroke.clone().or(stroke);
        } else {
            out.push(Drawn {
                page,
                x: sorted2(xmap.to_data(page.x), xmap.to_data(page.right())),
                y: sorted2(ymap.to_data(page.y), ymap.to_data(page.bottom())),
                fill,
                stroke,
            });
        }
    }
    out
}

/// Returns the one upright bar whose extent along x contains `x`, or a panic naming the place.
#[track_caller]
fn at(bars: &[Drawn], x: f64) -> &Drawn {
    let found: Vec<&Drawn> = bars.iter().filter(|d| d.x[0] < x && x < d.x[1]).collect();
    assert_eq!(found.len(), 1, "one bar covers x = {x}: {bars:#?}");
    found[0]
}

/// Returns whether any upright bar covers `x`.
fn covers(bars: &[Drawn], x: f64) -> bool {
    bars.iter().any(|d| d.x[0] < x && x < d.x[1])
}

#[track_caller]
fn assert_span(actual: [f64; 2], expected: [f64; 2], what: &str) {
    assert!(
        (actual[0] - expected[0]).abs() <= TOL && (actual[1] - expected[1]).abs() <= TOL,
        "{what}: drawn across {actual:?}, expected {expected:?}"
    );
}

/// Asserts that a colour is the IR colour `expected`, to the rounding of 8-bit colour.
#[track_caller]
fn assert_colour(actual: Option<Rgba>, expected: [u8; 3], what: &str) {
    let actual = actual.unwrap_or_else(|| panic!("{what}: nothing is painted"));
    assert!(
        rgb8_close(rgb8(actual), expected),
        "{what}: painted {:?}, expected {expected:?}",
        rgb8(actual)
    );
}

fn rgb8_of(color: Color) -> [u8; 3] {
    rgb8(rgba(color))
}

/// An axes holding nothing yet.
fn axes() -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    (fx, ax)
}

/// A group of the given name and layout.
fn group(name: &str, layout: BarLayout) -> Option<BarGroup> {
    Some(BarGroup {
        name: name.to_owned(),
        layout,
    })
}

// ---------------------------------------------------------------------------------------------------------------
// One series: slot, gap, base and orientation
// ---------------------------------------------------------------------------------------------------------------

// Why: a bar series was skipped with a warning until the compiler could draw it; a bar chart that compiles to
// nothing, or that carries a warning a front end shows as a problem, would tell the reader the figure is broken.
// One rectangle per bar, and no warning naming the series, is the minimum a bar chart owes its reader.
#[test]
fn a_bar_series_is_drawn_as_one_rectangle_per_bar_without_a_warning() {
    let (mut fx, ax) = axes();
    let bar = fx.bar(ax, &[1.0, 2.0, 3.0], &[3.0, 1.0, 2.0], |_, _| {});
    let scene = compile_figure(&fx.build());
    assert_eq!(drawn(&scene, ax, bar).len(), 3, "one rectangle per bar");
    let about_the_bar: Vec<_> = scene
        .warnings
        .iter()
        .filter(|w| w.node == Some(bar))
        .collect();
    assert!(
        about_the_bar.is_empty(),
        "a valid bar series compiles without a warning: {about_the_bar:?}"
    );
}

// Why: the length of a bar is its value, measured from the base, so a bar must start exactly at the base and end
// exactly at its value, downwards for a value below the base; and it must sit centred on its position across
// `1 − gap` of its slot, so that the reader can tell which position it belongs to and neighbouring bars stand apart.
// With positions 1, 2, 3 the pitch is 1, so each bar spans its position ± 0.4 at the default gap of 0.2.
#[test]
fn each_bar_spans_its_slot_less_the_gap_centred_on_its_position_from_the_base_to_its_value() {
    let (mut fx, ax) = axes();
    let values = [3.0, -1.0, 2.0];
    let bar = fx.bar(ax, &[1.0, 2.0, 3.0], &values, |_, _| {});
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    assert_eq!(bars.len(), 3);
    for (k, value) in values.into_iter().enumerate() {
        let p = (k + 1) as f64;
        let d = at(&bars, p);
        assert_span(d.x, [p - 0.4, p + 0.4], &format!("bar {k} across its slot"));
        assert_span(
            d.y,
            sorted2(0.0, value),
            &format!("bar {k} from the base to its value"),
        );
    }
}

// Why: the pitch decides how wide every bar is, so it must be the smallest distance between neighbouring distinct
// finite positions taken in order of value. Positions given out of order, repeated or with a missing (NaN) entry are
// ordinary in real data: taken in array order the pitch here would be 3 (from 3 to 0), with a repeat it would be 0,
// and with the NaN it would be NaN, and the bars would be drawn three times too wide, invisible or not at all. A
// single position has no neighbour, so its pitch is 1.
#[test]
fn the_pitch_is_the_smallest_distance_between_distinct_finite_positions_in_order_of_value() {
    let (mut fx, ax) = axes();
    let bar = fx.bar(
        ax,
        &[3.0, f64::NAN, 0.0, 1.0, 1.0],
        &[1.0, 1.0, 1.0, 1.0, 1.0],
        |_, _| {},
    );
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    // The two bars at 1 are drawn over one another, and `drawn` merges paths over the same rectangle, so the four
    // bars drawn are three rectangles.
    assert_eq!(
        bars.len(),
        3,
        "the bar at the non-finite position is not drawn: {bars:#?}"
    );
    assert_span(at(&bars, 3.0).x, [2.6, 3.4], "the pitch is 1, from 0 to 1");
    assert_span(at(&bars, 0.0).x, [-0.4, 0.4], "the pitch is 1, from 0 to 1");

    let (mut fx, ax) = axes();
    let single = fx.bar(ax, &[5.0], &[2.0], |_, _| {});
    let scene = compile_figure(&fx.build());
    assert_span(
        drawn(&scene, ax, single)[0].x,
        [4.6, 5.4],
        "a single position has a pitch of 1",
    );
}

// Why: a value that is missing (NaN) or infinite has no length to draw, so its bar is left out, and a gap in the
// row of bars shows the reader that the value is missing; the bars either side must stay exactly where they were,
// or the missing value would shift the bars that are present onto the wrong positions.
#[test]
fn a_bar_without_a_finite_value_is_left_out_and_its_neighbours_keep_their_places() {
    let (mut fx, ax) = axes();
    let bar = fx.bar(
        ax,
        &[0.0, 1.0, 2.0, 3.0],
        &[1.0, f64::NAN, f64::INFINITY, 3.0],
        |_, _| {},
    );
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    assert_eq!(bars.len(), 2, "only the finite values are drawn: {bars:#?}");
    assert!(!covers(&bars, 1.0) && !covers(&bars, 2.0));
    assert_span(
        at(&bars, 0.0).x,
        [-0.4, 0.4],
        "the first bar keeps its place",
    );
    assert_span(at(&bars, 3.0).x, [2.6, 3.4], "the last bar keeps its place");
    assert_span(at(&bars, 3.0).y, [0.0, 3.0], "and its value");
}

// Why: a fixed width draws bars narrower or wider than the pitch, and per-bar widths are how a histogram of unequal
// bins is drawn: with a gap of 0 each bar must cover exactly its bin, so that neighbouring bins touch and the area of
// each bar is its count times its width. A histogram whose bars overlapped or left slivers would misstate it.
#[test]
fn a_fixed_width_and_per_bar_widths_set_the_slot_of_each_bar() {
    let (mut fx, ax) = axes();
    let fixed = fx.bar(ax, &[0.0, 3.0], &[1.0, 2.0], |b, _| {
        b.slot = BarSlot::Width(0.5);
        b.gap = 0.0;
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, fixed);
    assert_span(at(&bars, 0.0).x, [-0.25, 0.25], "a fixed width of 0.5");
    assert_span(at(&bars, 3.0).x, [2.75, 3.25], "a fixed width of 0.5");

    // Bins [0, 1], [1, 3] and [3, 6], as a histogram of unequal bins stores them: centres and widths.
    let (mut fx, ax) = axes();
    let histogram = fx.bar(ax, &[0.5, 2.0, 4.5], &[4.0, 2.0, 1.0], |b, fx| {
        b.slot = BarSlot::Data(fx.vector(&[1.0, 2.0, 3.0]));
        b.gap = 0.0;
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, histogram);
    assert_span(at(&bars, 0.5).x, [0.0, 1.0], "the first bin");
    assert_span(at(&bars, 2.0).x, [1.0, 3.0], "the second bin");
    assert_span(at(&bars, 4.5).x, [3.0, 6.0], "the third bin");
}

// Why: a base given per bar draws floating bars, such as ranges or the steps of a waterfall, which are read by both
// ends; a constant base other than 0 makes bars rise from a reference level, and a value below that level hangs down
// from it. Either way the bar must run between exactly the two numbers the figure holds.
#[test]
fn bars_run_from_their_base_whether_constant_or_given_per_bar() {
    let (mut fx, ax) = axes();
    let floating = fx.bar(ax, &[0.0, 1.0, 2.0], &[4.0, 3.0, 2.0], |b, fx| {
        b.base = BarBase::Data(fx.vector(&[1.0, 2.0, -1.0]));
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, floating);
    assert_span(at(&bars, 0.0).y, [1.0, 4.0], "from its own base");
    assert_span(at(&bars, 1.0).y, [2.0, 3.0], "from its own base");
    assert_span(at(&bars, 2.0).y, [-1.0, 2.0], "from its own base");

    let (mut fx, ax) = axes();
    let reference = fx.bar(ax, &[0.0, 1.0], &[5.0, 1.0], |b, _| {
        b.base = BarBase::Constant(2.0);
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, reference);
    assert_span(at(&bars, 0.0).y, [2.0, 5.0], "above the reference level");
    assert_span(
        at(&bars, 1.0).y,
        [1.0, 2.0],
        "hanging below the reference level",
    );
}

// Why: `barh` is how long category names are shown, and it is the same chart turned on its side: the positions and
// slots lie along y and the values along x, so the geometry of a horizontal bar is that of an upright one with the
// axes exchanged, and a value below the base extends to the left.
#[test]
fn horizontal_bars_place_positions_along_y_and_values_along_x() {
    let (mut fx, ax) = axes();
    let bar = fx.bar(ax, &[0.0, 1.0], &[3.0, -2.0], |b, _| {
        b.orientation = Orientation::Horizontal;
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    assert_eq!(bars.len(), 2);
    let on = |y: f64| {
        bars.iter()
            .find(|d| d.y[0] < y && y < d.y[1])
            .unwrap_or_else(|| panic!("a bar lies across y = {y}: {bars:#?}"))
    };
    assert_span(on(0.0).y, [-0.4, 0.4], "the slot lies along y");
    assert_span(on(0.0).x, [0.0, 3.0], "the value lies along x");
    assert_span(on(1.0).y, [0.6, 1.4], "the slot lies along y");
    assert_span(
        on(1.0).x,
        [-2.0, 0.0],
        "a negative value extends to the left",
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------------------------------------------

/// Three grouped members A, B and C over positions 0 to 3, edited by `edit` with the index of the member, and the
/// drawn bars of each.
fn grouped(edit: impl Fn(usize, &mut ironlab_ir::Bar)) -> (Scene, NodeId, [NodeId; 3]) {
    let (mut fx, ax) = axes();
    let positions = [0.0, 1.0, 2.0, 3.0];
    let members = [0, 1, 2].map(|k| {
        fx.bar(ax, &positions, &[1.0 + k as f64; 4], |b, _| {
            b.group = group("models", BarLayout::Grouped);
            edit(k, b);
        })
    });
    (compile_figure(&fx.build()), ax, members)
}

// Why: grouped bars compare several series at each position, which the reader can only do if the members of a
// cluster stand side by side in artist order and touch, while the gap separates one position's cluster from the
// next. At the default gap of 0.2 and a pitch of 1 the drawn part of each slot is p ± 0.4, which three members split
// into thirds of 0.8 / 3; between clusters 0.2 is left empty.
#[test]
fn grouped_members_split_the_drawn_part_of_each_slot_touch_one_another_and_leave_the_gap_between_clusters()
 {
    let (scene, ax, members) = grouped(|_, _| {});
    let w = 0.8 / 3.0;
    let mut clusters = Vec::new();
    for p in [0.0, 1.0, 2.0, 3.0] {
        let mut previous_right = None;
        for (k, &member) in members.iter().enumerate() {
            let bars = drawn(&scene, ax, member);
            let lo = p - 0.4 + k as f64 * w;
            let d = at(&bars, lo + w / 2.0);
            assert_span(d.x, [lo, lo + w], &format!("member {k} at {p}"));
            assert_span(
                d.y,
                [0.0, 1.0 + k as f64],
                &format!("member {k} at {p} keeps its own value"),
            );
            if let Some(right) = previous_right {
                assert_close(d.x[0], right, TOL);
            }
            previous_right = Some(d.x[1]);
        }
        clusters.push([p - 0.4, previous_right.unwrap()]);
    }
    for pair in clusters.windows(2) {
        assert_close(pair[1][0] - pair[0][1], 0.2, TOL);
    }
}

// Why: the members of a group need not have the same positions, and are matched by position value, not by index:
// a member that lacks a position leaves its own sub-slot empty there, so that every other member stays in the
// sub-slot it has at every other position and the reader finds each series at the same place in every cluster.
// Member B here holds positions 3, 2 and 0, in reverse order and without 1.
#[test]
fn a_grouped_member_is_matched_by_position_and_leaves_its_sub_slot_empty_where_it_has_no_bar() {
    let (mut fx, ax) = axes();
    let a = fx.bar(ax, &[0.0, 1.0, 2.0, 3.0], &[1.0; 4], |b, _| {
        b.group = group("g", BarLayout::Grouped);
    });
    let b = fx.bar(ax, &[3.0, 2.0, 0.0], &[30.0, 20.0, 0.5], |b, _| {
        b.group = group("g", BarLayout::Grouped);
    });
    let c = fx.bar(ax, &[0.0, 1.0, 2.0, 3.0], &[3.0; 4], |b, _| {
        b.group = group("g", BarLayout::Grouped);
    });
    let scene = compile_figure(&fx.build());
    let w = 0.8 / 3.0;
    let middle = |p: f64, k: usize| p - 0.4 + (k as f64 + 0.5) * w;

    let bs = drawn(&scene, ax, b);
    assert_eq!(bs.len(), 3);
    assert!(!covers(&bs, middle(1.0, 1)), "B has no bar at 1");
    assert_span(
        at(&bs, middle(3.0, 1)).y,
        [0.0, 30.0],
        "B's bar at 3 holds its value at 3",
    );
    assert_span(
        at(&bs, middle(0.0, 1)).y,
        [0.0, 0.5],
        "B's bar at 0 holds its value at 0",
    );
    for (k, member) in [(0, a), (2, c)] {
        let bars = drawn(&scene, ax, member);
        let lo = 1.0 - 0.4 + k as f64 * w;
        assert_span(
            at(&bars, middle(1.0, k)).x,
            [lo, lo + w],
            &format!("member {k} keeps its sub-slot at 1"),
        );
    }
}

// Why: toggling a legend entry hides one member of a group; if the remaining members then widened to fill the
// cluster, every bar of the chart would move and the reader could not compare the views before and after. A hidden
// grouped member therefore keeps its sub-slot, empty, and draws nothing.
#[test]
fn a_hidden_grouped_member_keeps_its_place_and_the_others_do_not_move() {
    let (shown, ax, members) = grouped(|_, _| {});
    let (hidden, _, _) = grouped(|k, b| b.visible = k != 1);
    assert!(
        drawn(&hidden, ax, members[1]).is_empty(),
        "the hidden member draws nothing"
    );
    for k in [0, 2] {
        let before = drawn(&shown, ax, members[k]);
        let after = drawn(&hidden, ax, members[k]);
        assert_eq!(before.len(), 4, "member {k} draws a bar at each position");
        assert_eq!(
            after.len(),
            4,
            "member {k} still draws a bar at each position"
        );
        for (b, a) in before.iter().zip(&after) {
            assert_span(a.x, b.x, &format!("member {k} stays where it was"));
        }
    }
}

// Why: a stack shows a total and its parts, so each member must start exactly where the members before it ended at
// the same position, in artist order. Positive and negative values stack separately, upwards and downwards from the
// base (Plotly's relative mode): were a negative part stacked onto the top of a positive one it would hide inside
// it and the column would no longer show the positive total. Member B is given its positions in reverse order, so a
// stack matched by index rather than by position value would put B's parts on the wrong columns.
#[test]
fn stacked_members_build_on_one_another_with_positive_and_negative_parts_stacked_separately() {
    let (mut fx, ax) = axes();
    let a = fx.bar(ax, &[0.0, 1.0, 2.0], &[1.0, -2.0, 3.0], |b, _| {
        b.group = group("s", BarLayout::Stacked);
    });
    let b = fx.bar(ax, &[2.0, 1.0, 0.0], &[-1.0, -1.0, 2.0], |b, _| {
        b.group = group("s", BarLayout::Stacked);
    });
    let c = fx.bar(ax, &[0.0, 1.0, 2.0], &[0.5, 4.0, 1.0], |b, _| {
        b.group = group("s", BarLayout::Stacked);
    });
    let scene = compile_figure(&fx.build());
    let (a, b, c) = (
        drawn(&scene, ax, a),
        drawn(&scene, ax, b),
        drawn(&scene, ax, c),
    );

    // Position 0: all positive, so 0 → 1 → 3 → 3.5.
    assert_span(at(&a, 0.0).y, [0.0, 1.0], "A at 0");
    assert_span(at(&b, 0.0).y, [1.0, 3.0], "B at 0 starts where A ended");
    assert_span(at(&c, 0.0).y, [3.0, 3.5], "C at 0 starts where B ended");
    // Position 1: A and B are negative and stack downwards; C is positive and rises from the base, not from them.
    assert_span(at(&a, 1.0).y, [-2.0, 0.0], "A at 1 hangs from the base");
    assert_span(at(&b, 1.0).y, [-3.0, -2.0], "B at 1 hangs from A");
    assert_span(at(&c, 1.0).y, [0.0, 4.0], "C at 1 rises from the base");
    // Position 2: B is negative between two positive members, so C stacks on A, not on B.
    assert_span(at(&a, 2.0).y, [0.0, 3.0], "A at 2");
    assert_span(at(&b, 2.0).y, [-1.0, 0.0], "B at 2 hangs from the base");
    assert_span(
        at(&c, 2.0).y,
        [3.0, 4.0],
        "C at 2 stacks on A, the last positive part",
    );
    // Stacked members share the whole drawn part of the slot.
    assert_span(
        at(&c, 2.0).x,
        [1.6, 2.4],
        "a stack is as wide as a lone bar",
    );
}

// Why: a stack's members build on a base other than 0 as they do on 0: the first part starts at the base, positive
// parts above it and negative parts below it.
#[test]
fn a_stack_builds_from_its_base() {
    let (mut fx, ax) = axes();
    let members = [[2.0, -1.0], [3.0, -2.0]].map(|values| {
        fx.bar(ax, &[0.0, 1.0], &values, |b, _| {
            b.group = group("s", BarLayout::Stacked);
            b.base = BarBase::Constant(10.0);
        })
    });
    let scene = compile_figure(&fx.build());
    let (a, b) = (drawn(&scene, ax, members[0]), drawn(&scene, ax, members[1]));
    assert_span(at(&a, 0.0).y, [10.0, 12.0], "A at 0 rises from the base");
    assert_span(at(&b, 0.0).y, [12.0, 15.0], "B at 0 rises from A");
    assert_span(at(&a, 1.0).y, [9.0, 10.0], "A at 1 hangs from the base");
    assert_span(at(&b, 1.0).y, [7.0, 9.0], "B at 1 hangs from A");
}

// Why: toggling a legend entry of a stack is how a reader sees the total of the remaining parts, so a hidden member
// contributes nothing and the members above it close up onto those below, unlike a hidden grouped member, which
// keeps its place.
#[test]
fn a_hidden_stacked_member_contributes_nothing_and_the_others_close_up() {
    let (mut fx, ax) = axes();
    let ids = [1.0, 2.0, 4.0].map(|v| {
        fx.bar(ax, &[0.0], &[v], |b, _| {
            b.group = group("s", BarLayout::Stacked);
        })
    });
    if let Some(Artist::Bar(first)) = fx.ax(ax).artists.first_mut() {
        first.visible = false;
    }
    let scene = compile_figure(&fx.build());
    assert!(
        drawn(&scene, ax, ids[0]).is_empty(),
        "the hidden member draws nothing"
    );
    assert_span(
        drawn(&scene, ax, ids[1])[0].y,
        [0.0, 2.0],
        "B closes up onto the base",
    );
    assert_span(
        drawn(&scene, ax, ids[2])[0].y,
        [2.0, 6.0],
        "C closes up onto B",
    );
}

// Why: the orientation, slot, gap, base and layout of a group are those of its first member, because a group with
// members of different widths or gaps has no one cluster to draw. A later member that disagrees is warned of by
// validation and drawn by the first member's properties, so the chart stays a readable group rather than a member
// drawn on its own over the others.
#[test]
fn a_member_that_disagrees_with_the_first_is_drawn_by_the_first_members_properties() {
    let (mut fx, ax) = axes();
    let first = fx.bar(ax, &[0.0, 1.0], &[1.0, 1.0], |b, _| {
        b.group = group("g", BarLayout::Grouped);
    });
    let later = fx.bar(ax, &[0.0, 1.0], &[2.0, 2.0], |b, _| {
        b.group = group("g", BarLayout::Grouped);
        b.gap = 0.6;
        b.slot = BarSlot::Width(5.0);
        b.base = BarBase::Constant(1.0);
    });
    let scene = compile_figure(&fx.build());
    let first = drawn(&scene, ax, first);
    let later = drawn(&scene, ax, later);
    assert_span(
        at(&first, -0.2).x,
        [-0.4, 0.0],
        "the first member's half of the cluster",
    );
    assert_span(
        at(&later, 0.2).x,
        [0.0, 0.4],
        "the later member's half, by the first's slot and gap",
    );
    assert_span(
        at(&later, 0.2).y,
        [0.0, 2.0],
        "from the first member's base",
    );
}

// Why: members of one group commonly come from different calls with different widths, such as a histogram and a
// reference series binned differently, yet a cluster or a column can only be drawn across one slot. Whether grouped
// or stacked, every member takes the slot of the first member, so that the members of a cluster touch without
// overlapping and the parts of a stack lie exactly on one another; a later member drawn at its own width would
// stick out of the cluster or the column. The first member here has a fixed width of 2 and the second a width of
// 0.5, both without a gap, at positions 0 and 4.
#[test]
fn members_of_different_slot_widths_are_drawn_across_the_first_members_slot_whatever_the_layout() {
    for layout in [BarLayout::Grouped, BarLayout::Stacked] {
        let (mut fx, ax) = axes();
        let [first, later] = [2.0, 0.5].map(|width| {
            fx.bar(ax, &[0.0, 4.0], &[1.0, 2.0], |b, _| {
                b.group = group("g", layout);
                b.slot = BarSlot::Width(width);
                b.gap = 0.0;
            })
        });
        let scene = compile_figure(&fx.build());
        let (first, later) = (drawn(&scene, ax, first), drawn(&scene, ax, later));
        assert_eq!((first.len(), later.len()), (2, 2), "{layout:?}");
        for p in [0.0, 4.0] {
            let (a, b) = match layout {
                BarLayout::Grouped => ([p - 1.0, p], [p, p + 1.0]),
                BarLayout::Stacked => ([p - 1.0, p + 1.0], [p - 1.0, p + 1.0]),
            };
            assert_span(
                at(&first, a[0] + 0.25).x,
                a,
                &format!("{layout:?}: the first member at {p}"),
            );
            assert_span(
                at(&later, b[1] - 0.25).x,
                b,
                &format!("{layout:?}: the later member at {p}"),
            );
        }
    }
}

// Why: a part of a stack that is missing (NaN) has no length, so it is not drawn and contributes nothing: the part
// above it starts where the part below it ended. Adding the missing value to the running total would make every
// later part of the column NaN, and the column would lose its top for one missing number.
#[test]
fn a_part_of_a_stack_without_a_finite_value_contributes_nothing() {
    let (mut fx, ax) = axes();
    let ids = [1.0, f64::NAN, 2.0].map(|v| {
        fx.bar(ax, &[0.0], &[v], |b, _| {
            b.group = group("s", BarLayout::Stacked);
        })
    });
    let scene = compile_figure(&fx.build());
    assert!(
        drawn(&scene, ax, ids[1]).is_empty(),
        "the missing part draws nothing"
    );
    assert_span(
        drawn(&scene, ax, ids[0])[0].y,
        [0.0, 1.0],
        "A from the base",
    );
    assert_span(
        drawn(&scene, ax, ids[2])[0].y,
        [1.0, 3.0],
        "C closes up onto A",
    );
}

// Why: a group is formed by name within one axes only, and a series without a group is never part of one. Subplots
// commonly reuse a group name, and a chart may add a lone reference series beside a group; were either drawn into a
// group, bars would shrink into sub-slots that belong to nothing on their own axes.
#[test]
fn groups_are_formed_by_name_within_one_axes_and_never_take_in_an_ungrouped_series() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    let in_left = fx.bar(left, &[0.0, 1.0], &[1.0, 1.0], |b, _| {
        b.group = group("g", BarLayout::Grouped);
    });
    let in_right = fx.bar(right, &[0.0, 1.0], &[1.0, 1.0], |b, _| {
        b.group = group("g", BarLayout::Grouped);
    });
    let lone = fx.bar(right, &[0.0, 1.0], &[2.0, 2.0], |_, _| {});
    let scene = compile_figure(&fx.build());
    for (ax, bar, what) in [
        (left, in_left, "the only member in its axes"),
        (right, in_right, "the only member in its axes"),
        (right, lone, "a series without a group"),
    ] {
        assert_span(at(&drawn(&scene, ax, bar), 0.0).x, [-0.4, 0.4], what);
    }
}

// Why: a reversed axis is common for bars, such as a ranking read from the top down with `barh`, or depth plotted
// downwards. A bar must still span exactly its slot less the gap and run from its base to its value: computed on
// the page with the assumption that the page coordinate grows with the data, the inset of the gap would become an
// outset, or the rectangle would have a negative extent and draw nothing. The bars are mapped back to data, so
// these are the same spans as on unreversed axes, and the page order shows that the axes really are reversed.
#[test]
fn on_reversed_axes_each_bar_still_spans_its_slot_less_the_gap_from_its_base_to_its_value() {
    let (mut fx, ax) = axes();
    fx.ax(ax).x.reverse = true;
    fx.ax(ax).y.reverse = true;
    let values = [3.0, -1.0, 2.0];
    let bar = fx.bar(ax, &[0.0, 1.0, 2.0], &values, |_, _| {});
    let scene = compile_figure(&fx.build());
    let (xmap, ymap) = axis_maps(&scene, ax);
    assert!(
        xmap.to_figure(0.0) > xmap.to_figure(1.0) && ymap.to_figure(0.0) < ymap.to_figure(1.0),
        "precondition: both axes run backwards on the page: {xmap:?} {ymap:?}"
    );
    let bars = drawn(&scene, ax, bar);
    assert_eq!(bars.len(), 3);
    for (k, value) in values.into_iter().enumerate() {
        let p = k as f64;
        let d = at(&bars, p);
        assert_span(d.x, [p - 0.4, p + 0.4], &format!("bar {k} across its slot"));
        assert_span(
            d.y,
            sorted2(0.0, value),
            &format!("bar {k} from the base to its value"),
        );
    }
    assert!(
        at(&bars, 0.0).page.x > at(&bars, 1.0).page.x,
        "the first bar is drawn on the right"
    );
    assert_close(xmap.min.min(xmap.max), -0.5, TOL);
    assert_close(xmap.min.max(xmap.max), 2.5, TOL);
}

// ---------------------------------------------------------------------------------------------------------------
// Logarithmic axes
// ---------------------------------------------------------------------------------------------------------------

// Why: on a logarithmic category axis a gap measured in data units would leave more space on the right of each bar
// than on its left, so the bar would look off-centre in its slot. The gap is a fraction of the slot measured on the
// page: each side is inset by a quarter of the slot's page width at a gap of 0.5, which in data units is not
// symmetric about the position at all.
#[test]
fn on_a_logarithmic_category_axis_the_gap_is_symmetric_on_the_page() {
    let (mut fx, ax) = axes();
    fx.ax(ax).x.scale = Scale::Log;
    let bar = fx.bar(ax, &[10.0, 30.0], &[1.0, 2.0], |b, _| {
        b.slot = BarSlot::Width(10.0);
        b.gap = 0.5;
    });
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    let bars = drawn(&scene, ax, bar);
    for (p, d) in [(10.0, at(&bars, 10.0)), (30.0, at(&bars, 30.0))] {
        let (lo, hi) = (xmap.to_figure(p - 5.0), xmap.to_figure(p + 5.0));
        let quarter = 0.25 * (hi - lo);
        assert_close(d.page.x, lo + quarter, 1e-6);
        assert_close(d.page.right(), hi - quarter, 1e-6);
        let (left, right) = (d.x[0] - (p - 5.0), (p + 5.0) - d.x[1]);
        assert!(
            (left - right).abs() > 0.1,
            "precondition: in data units the gap is not symmetric ({left} against {right})"
        );
    }
}

// Why: a logarithmic value axis has no place for a base of 0, yet bars from 0 are the default and a bar chart of
// quantities spanning decades is common. Each such bar is drawn from the low edge of the plot area, so that its top
// still reads its value, and the axis must not try to reach 0.
#[test]
fn on_a_logarithmic_value_axis_a_base_that_is_not_positive_is_drawn_from_the_low_edge_of_the_plot()
{
    let (mut fx, ax) = axes();
    fx.ax(ax).y.scale = Scale::Log;
    let bar = fx.bar(ax, &[0.0, 1.0], &[20.0, 300.0], |_, _| {});
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert!(ymap.min > 0.0, "the axis does not reach for 0: {ymap:?}");
    let bars = drawn(&scene, ax, bar);
    let low_edge = ymap.to_figure(ymap.min);
    for (p, value) in [(0.0, 20.0), (1.0, 300.0)] {
        let d = at(&bars, p);
        let bottom = if (d.page.y - low_edge).abs() < (d.page.bottom() - low_edge).abs() {
            d.page.y
        } else {
            d.page.bottom()
        };
        assert_close(bottom, low_edge, 1e-6);
        assert_close(d.y[1], value, 1e-9 * value);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------------------------------------------

// Why: bars fill their slots as the cells of a grid fill theirs, so the automatic limits along the category axis end
// exactly at the outer edges of the outermost slots, rather than at the next tick value, which would leave an
// arbitrary empty margin at one end and not the other. With positions 1 to 3 at a pitch of 1 they are 0.5 and 3.5,
// so the half-gap beside the outer bars matches the half-gap beside every other bar.
#[test]
fn automatic_category_limits_end_exactly_at_the_outer_edges_of_the_slots() {
    let (mut fx, ax) = axes();
    fx.bar(ax, &[1.0, 2.0, 3.0], &[3.0, 1.0, 2.0], |_, _| {});
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    assert_close(xmap.min, 0.5, TOL);
    assert_close(xmap.max, 3.5, TOL);

    // A histogram of bins [0, 1], [1, 3] and [3, 6] ends at its outer bin edges.
    let (mut fx, ax) = axes();
    fx.bar(ax, &[0.5, 2.0, 4.5], &[4.0, 2.0, 1.0], |b, fx| {
        b.slot = BarSlot::Data(fx.vector(&[1.0, 2.0, 3.0]));
        b.gap = 0.0;
    });
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    assert_close(xmap.min, 0.0, TOL);
    assert_close(xmap.max, 6.0, TOL);

    // Horizontal bars set the limits of y the same way.
    let (mut fx, ax) = axes();
    fx.bar(ax, &[1.0, 2.0, 3.0], &[3.0, 1.0, 2.0], |b, _| {
        b.orientation = Orientation::Horizontal;
    });
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert_close(ymap.min, 0.5, TOL);
    assert_close(ymap.max, 3.5, TOL);
}

// Why: a bar is read against its base, so the automatic value limits include the base as well as the ends of the
// bars: bars rising from 0 must show 0, or they would appear to start at the lowest value and their lengths would
// misstate the ratios between them. A stack must be shown whole, so the limits cover the totals of its positive and
// negative parts rather than the individual values.
#[test]
fn automatic_value_limits_include_the_base_and_the_totals_of_stacks() {
    let (mut fx, ax) = axes();
    fx.bar(ax, &[0.0, 1.0, 2.0], &[30.0, 50.0, 40.0], |_, _| {});
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert_close(ymap.min, 0.0, 1e-12);
    assert!(ymap.max >= 50.0);

    let (mut fx, ax) = axes();
    fx.bar(ax, &[0.0, 1.0], &[12.0, 15.0], |b, _| {
        b.base = BarBase::Constant(10.0)
    });
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert!(ymap.min <= 10.0 && ymap.max >= 15.0, "{ymap:?}");

    let (mut fx, ax) = axes();
    for values in [[4.0, -3.0], [6.0, -2.0]] {
        fx.bar(ax, &[0.0, 1.0], &values, |b, _| {
            b.group = group("s", BarLayout::Stacked);
        });
    }
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert!(
        ymap.max >= 10.0,
        "the positive total 4 + 6 is shown: {ymap:?}"
    );
    assert!(
        ymap.min <= -5.0,
        "the negative total −3 − 2 is shown: {ymap:?}"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Colours
// ---------------------------------------------------------------------------------------------------------------

// Why: bars take the colour order as series do, so that a bar chart of several series is told apart without the
// author choosing colours, and a line and bars plotted together take different colours. By default each bar is
// outlined by a solid black line 0.5 points wide, so that neighbouring bars of similar colours stay distinct.
#[test]
fn automatic_faces_take_the_colour_order_and_edges_default_to_thin_black() {
    let (mut fx, ax) = axes();
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let first = fx.bar(ax, &[0.0, 1.0], &[1.0, 2.0], |_, _| {});
    let second = fx.bar(ax, &[0.0, 1.0], &[1.0, 2.0], |_, _| {});
    let scene = compile_figure(&fx.build());
    for (bar, k) in [(first, 1), (second, 2)] {
        let bars = drawn(&scene, ax, bar);
        assert_eq!(bars.len(), 2, "both bars of series {k} are drawn");
        for d in bars {
            assert_colour(
                d.fill,
                COLOUR_ORDER[k],
                &format!("the {k}th colour of the order"),
            );
            let stroke = d.stroke.expect("bars are outlined by default");
            assert_colour(Some(stroke.color), [0, 0, 0], "a black outline");
            assert_close(stroke.width, 0.5, 1e-12);
            assert!(stroke.dash.is_empty(), "a solid outline: {:?}", stroke.dash);
        }
    }
}

// Why: a fixed colour paints every bar of the series in it; `none` draws outlines only, as hollow bars are drawn;
// an outline of `none` draws faces only; and an automatic outline takes the series colour, as an automatic marker
// edge follows its series, so that hollow bars still carry the colour of their series.
#[test]
fn fixed_and_none_faces_and_edges_are_drawn_as_given_and_an_automatic_edge_takes_the_series_colour()
{
    let (mut fx, ax) = axes();
    let fixed = fx.bar(ax, &[0.0], &[1.0], |b, _| {
        b.face = ColorSpec::Rgba { color: RED };
        b.edge.color = ColorSpec::None;
    });
    let hollow = fx.bar(ax, &[1.0], &[1.0], |b, _| {
        b.face = ColorSpec::None;
        b.edge.color = ColorSpec::Auto;
    });
    let scene = compile_figure(&fx.build());
    let fixed = &drawn(&scene, ax, fixed)[0];
    assert_colour(fixed.fill, rgb8_of(RED), "a fixed face");
    assert!(fixed.stroke.is_none(), "no outline");
    let hollow = &drawn(&scene, ax, hollow)[0];
    assert!(hollow.fill.is_none(), "no face");
    assert_colour(
        hollow.stroke.as_ref().map(|s| s.color),
        COLOUR_ORDER[0],
        "an automatic edge takes the series colour",
    );
}

// Why: the face and edge opacities fade the bars as a whole, for bars drawn over other data or a reference band;
// each multiplies the alpha of its colour, so a translucent colour at half opacity is fainter still.
#[test]
fn face_and_edge_opacities_multiply_the_alpha_of_their_colours() {
    let (mut fx, ax) = axes();
    let bar = fx.bar(ax, &[0.0], &[1.0], |b, _| {
        b.face = ColorSpec::Rgba {
            color: Color::rgba(1.0, 0.0, 0.0, 0.8),
        };
        b.face_opacity = 0.5;
        b.edge_opacity = 0.25;
    });
    let scene = compile_figure(&fx.build());
    let d = &drawn(&scene, ax, bar)[0];
    assert_close(f64::from(d.fill.expect("filled").a), 0.4, 1e-6);
    assert_close(
        f64::from(d.stroke.as_ref().expect("stroked").color.a),
        0.25,
        1e-6,
    );
}

// Why: colouring individual bars by category, such as highlighting one bar or colouring bars by the model they
// belong to, is done through colour data and an indexed scale: each bar takes the entry its value names, so the bars
// must be painted in exactly the palette entries their data index, whatever their positions.
#[test]
fn a_colormapped_face_gives_each_bar_the_colour_of_its_value_through_the_scale() {
    let (mut fx, ax) = axes();
    let palette = fx.color_scale(ColorScaleKind::Indexed(IndexedColorScale {
        colors: vec![RED, GREEN, BLUE],
        ..IndexedColorScale::default()
    }));
    let bar = fx.bar(ax, &[0.0, 1.0, 2.0, 3.0], &[1.0, 2.0, 3.0, 4.0], |b, fx| {
        b.face = ColorSpec::Colormapped;
        b.color_data = Some(fx.vector(&[0.0, 2.0, 0.0, 1.0]));
        b.color_scale = Some(palette);
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    for (p, colour) in [(0.0, RED), (1.0, BLUE), (2.0, RED), (3.0, GREEN)] {
        assert_colour(
            at(&bars, p).fill,
            rgb8_of(colour),
            &format!("the bar at {p}"),
        );
    }
}

// Why: a colormapped outline lets hollow bars, or bars of one face colour, be told apart by their own colours; each
// outline takes the colour of its bar's value through the scale.
#[test]
fn a_colormapped_edge_gives_each_outline_the_colour_of_its_bars_value() {
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: ironlab_ir::Colormap::Listed {
            colors: vec![RED, BLUE],
        },
        limits: Limits::Manual { min: 0.0, max: 1.0 },
        ..ContinuousColorScale::default()
    }));
    let bar = fx.bar(ax, &[0.0, 1.0], &[1.0, 1.0], |b, fx| {
        b.face = ColorSpec::None;
        b.edge.color = ColorSpec::Colormapped;
        b.color_data = Some(fx.vector(&[0.0, 1.0]));
        b.color_scale = Some(scale);
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    assert_colour(
        at(&bars, 0.0).stroke.as_ref().map(|s| s.color),
        rgb8_of(RED),
        "the low end",
    );
    assert_colour(
        at(&bars, 1.0).stroke.as_ref().map(|s| s.color),
        rgb8_of(BLUE),
        "the high end",
    );
}

// Why: a colormapped face without colour data has no value to map; it is drawn as an automatic face would be, in the
// series colour, as a colormapped marker of a series without colour data is, rather than vanishing.
#[test]
fn a_colormapped_face_without_colour_data_is_drawn_in_the_series_colour() {
    let (mut fx, ax) = axes();
    let bar = fx.bar(ax, &[0.0, 1.0], &[1.0, 2.0], |b, _| {
        b.face = ColorSpec::Colormapped;
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    assert_eq!(bars.len(), 2, "both bars are drawn");
    for d in bars {
        assert_colour(d.fill, COLOUR_ORDER[0], "the series colour");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Robustness and export
// ---------------------------------------------------------------------------------------------------------------

// Why: a bar series of no values is valid (ADR 0012), as an empty result of a query is; it draws nothing and must
// not stop the rest of the axes from being drawn.
#[test]
fn a_bar_series_of_no_values_draws_nothing_and_the_axes_is_still_drawn() {
    let (mut fx, ax) = axes();
    let empty = fx.bar(ax, &[], &[], |_, _| {});
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let scene = compile_figure(&fx.build());
    assert!(from_source(&leaves(&scene), empty).is_empty());
    assert!(
        !from_source(&leaves(&scene), line).is_empty(),
        "the line is still drawn"
    );
}

/// The dense groups of the scene, in paint order, as `(source, cells)`.
fn dense_groups(scene: &Scene) -> Vec<(Option<NodeId>, u64)> {
    fn walk(items: &[Item], out: &mut Vec<(Option<NodeId>, u64)>) {
        for item in items {
            match &item.kind {
                ItemKind::Dense { cells, .. } => out.push((item.source, *cells)),
                ItemKind::Group { items, .. } | ItemKind::Depth { items } => walk(items, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&scene.display_list.items, &mut out);
    out
}

// Why: the PDF exporter decides whether to rasterise content from the number of pieces it would otherwise draw as
// vectors, so a chart of very many bars must be counted as surface faces are; counting bars that were not drawn
// would rasterise geometry that is not there. Of these five bars, the one without a finite value is not drawn.
#[test]
fn bars_are_counted_as_dense_content_by_the_number_drawn() {
    let (mut fx, ax) = axes();
    let bar = fx.bar(
        ax,
        &[0.0, 1.0, 2.0, 3.0, 4.0],
        &[1.0, 2.0, f64::NAN, 4.0, 5.0],
        |_, _| {},
    );
    let scene = compile_figure(&fx.build());
    assert_eq!(dense_groups(&scene), vec![(Some(bar), 4)]);
}
