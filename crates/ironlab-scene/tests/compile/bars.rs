//! Bars (MATLAB's and Matplotlib's `bar` and `barh`): where each rectangle is drawn from its position, offset, width,
//! base and length, what colours the bars take, and the automatic limits they set. Bars are a primitive: each series
//! draws exactly the bars it describes, and grouping and stacking are written into ordinary series by the facade, so
//! the series here that stand side by side or on one another carry the widths, offsets and bases the helper writes.
//!
//! A bar is read by where its ends are and how wide it is, so every geometric test maps the drawn rectangle back
//! through the axis maps of its axes, as `surfaces_2d` does for faces, and compares it in data units with what the
//! figure says. The arithmetic of the expected rectangles is written out in each test rather than taken from the
//! compiler, so that the expectation does not depend on the code under test.

use ironlab_ir::{
    BarOffset, BarWidth, Color, ColorScaleKind, ColorSpec, ContinuousColorScale, IndexedColorScale,
    Limits, NodeId, Orientation, PerBar, Scale,
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

// ---------------------------------------------------------------------------------------------------------------
// One series: position, width, base and orientation
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

// Why: a value is the length of its bar, measured from the base, so a bar must start exactly at the base and end
// exactly at the base plus its value, downwards for a negative value; and it must sit centred on its position, four
// fifths of the pitch wide by default, so that the reader can tell which position it belongs to and neighbouring
// bars stand apart. With positions 1, 2, 3 the pitch is 1, so each bar spans its position ± 0.4.
#[test]
fn each_bar_is_centred_on_its_position_at_its_width_and_runs_from_the_base_for_its_length() {
    let (mut fx, ax) = axes();
    let values = [3.0, -1.0, 2.0];
    let bar = fx.bar(ax, &[1.0, 2.0, 3.0], &values, |_, _| {});
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    assert_eq!(bars.len(), 3);
    for (k, value) in values.into_iter().enumerate() {
        let p = (k + 1) as f64;
        let d = at(&bars, p);
        assert_span(
            d.x,
            [p - 0.4, p + 0.4],
            &format!("bar {k} across its width"),
        );
        assert_span(
            d.y,
            sorted2(0.0, value),
            &format!("bar {k} from the base for its length"),
        );
    }
}

// Why: the pitch decides how wide every bar of a width given as a fraction of it is, so it must be the smallest
// distance between neighbouring distinct finite positions taken in order of value. Positions given out of order,
// repeated or with a missing (NaN) entry are ordinary in real data: taken in array order the pitch here would be 3
// (from 3 to 0), with a repeat it would be 0, and with the NaN it would be NaN, and the bars would be drawn three
// times too wide, invisible or not at all. A single position has no neighbour, so its pitch is 1.
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

// Why: a width is how wide a bar is drawn, whichever form it takes: a fraction of the pitch (the whole pitch for
// bars that touch), a fixed width in data units, or one width per bar, which is how a histogram of unequal bins is
// drawn, where each bar must cover exactly its bin so that neighbouring bins touch and the area of each bar is its
// count times its width. A histogram whose bars overlapped or left slivers would misstate it.
#[test]
fn a_fraction_of_the_pitch_a_fixed_width_and_per_bar_widths_set_the_width_of_each_bar() {
    let (mut fx, ax) = axes();
    let touching = fx.bar(ax, &[0.0, 2.0, 4.0], &[1.0, 2.0, 3.0], |b, _| {
        b.width = BarWidth::Pitch(1.0);
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, touching);
    assert_span(at(&bars, 0.0).x, [-1.0, 1.0], "the whole pitch of 2");
    assert_span(at(&bars, 2.0).x, [1.0, 3.0], "touching its neighbours");

    let (mut fx, ax) = axes();
    let fixed = fx.bar(ax, &[0.0, 3.0], &[1.0, 2.0], |b, _| {
        b.width = BarWidth::Constant(0.5);
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, fixed);
    assert_span(at(&bars, 0.0).x, [-0.25, 0.25], "a fixed width of 0.5");
    assert_span(at(&bars, 3.0).x, [2.75, 3.25], "a fixed width of 0.5");

    // Bins [0, 1], [1, 3] and [3, 6], as a histogram of unequal bins stores them: centres and widths.
    let (mut fx, ax) = axes();
    let histogram = fx.bar(ax, &[0.5, 2.0, 4.5], &[4.0, 2.0, 1.0], |b, fx| {
        b.width = BarWidth::Data(fx.vector(&[1.0, 2.0, 3.0]));
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, histogram);
    assert_span(at(&bars, 0.5).x, [0.0, 1.0], "the first bin");
    assert_span(at(&bars, 2.0).x, [1.0, 3.0], "the second bin");
    assert_span(at(&bars, 4.5).x, [3.0, 6.0], "the third bin");
}

// Why: a value is a length from the base, as in Matplotlib and Plotly, so a base given per bar draws floating bars,
// such as ranges or the steps of a waterfall, from each base for each length, and a constant base other than 0
// makes bars rise from a reference level, with a negative length hanging down from it. Reading the value as the end
// instead would draw the bar at 1 here from 2 to 3 rather than from 2 to 5.
#[test]
fn bars_run_from_their_base_for_their_length_whether_the_base_is_constant_or_given_per_bar() {
    let (mut fx, ax) = axes();
    let floating = fx.bar(ax, &[0.0, 1.0, 2.0], &[4.0, 3.0, 2.0], |b, fx| {
        b.base = PerBar::Data(fx.vector(&[1.0, 2.0, -1.0]));
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, floating);
    assert_span(at(&bars, 0.0).y, [1.0, 5.0], "from 1 for 4");
    assert_span(at(&bars, 1.0).y, [2.0, 5.0], "from 2 for 3");
    assert_span(at(&bars, 2.0).y, [-1.0, 1.0], "from −1 for 2");

    let (mut fx, ax) = axes();
    let reference = fx.bar(ax, &[0.0, 1.0], &[5.0, -1.5], |b, _| {
        b.base = PerBar::Constant(2.0);
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, reference);
    assert_span(at(&bars, 0.0).y, [2.0, 7.0], "above the reference level");
    assert_span(
        at(&bars, 1.0).y,
        [0.5, 2.0],
        "a negative length hangs below the reference level",
    );
}

// Why: `barh` is how long category names are shown, and it is the same chart turned on its side: the positions and
// widths lie along y and the lengths along x, so the geometry of a horizontal bar is that of an upright one with the
// axes exchanged, and a negative length extends to the left of the base.
#[test]
fn horizontal_bars_place_positions_along_y_and_values_along_x() {
    let (mut fx, ax) = axes();
    let bar = fx.bar(ax, &[0.0, 1.0], &[3.0, -2.0], |b, _| {
        b.orientation = Orientation::Horizontal;
        b.base = PerBar::Constant(1.0);
        b.offset = BarOffset::Absolute(0.1);
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    assert_eq!(bars.len(), 2);
    let on = |y: f64| {
        bars.iter()
            .find(|d| d.y[0] < y && y < d.y[1])
            .unwrap_or_else(|| panic!("a bar lies across y = {y}: {bars:#?}"))
    };
    assert_span(
        on(0.1).y,
        [-0.3, 0.5],
        "the width lies along y, offset along y",
    );
    assert_span(
        on(0.1).x,
        [1.0, 4.0],
        "the length lies along x from the base",
    );
    assert_span(
        on(1.1).y,
        [0.7, 1.5],
        "the width lies along y, offset along y",
    );
    assert_span(
        on(1.1).x,
        [-1.0, 1.0],
        "a negative length extends to the left",
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Offsets
// ---------------------------------------------------------------------------------------------------------------

// Why: an offset in widths places a series beside its position by a multiple of its own width, which is how the
// columns of a grouped chart stay side by side: a column offset by one width touches the column at its position
// whatever the width is. The offset must therefore follow the width, of the pitch, fixed or per bar, rather than be
// fixed in data units when the series is created; a bar of a per-bar width is offset by its own width.
#[test]
fn an_offset_in_widths_moves_each_bar_by_its_own_width_and_follows_a_change_of_width() {
    for (fraction, width) in [(0.4, 0.4), (0.2, 0.2)] {
        let (mut fx, ax) = axes();
        let bar = fx.bar(ax, &[0.0, 1.0, 2.0], &[1.0, 2.0, 3.0], |b, _| {
            b.width = BarWidth::Pitch(fraction);
            b.offset = BarOffset::Widths(1.0);
        });
        let scene = compile_figure(&fx.build());
        let bars = drawn(&scene, ax, bar);
        for p in [0.0, 1.0, 2.0] {
            let centre = p + width;
            assert_span(
                at(&bars, centre).x,
                [centre - width / 2.0, centre + width / 2.0],
                &format!("a width of {width}: the bar at {p} lies one width to the right"),
            );
        }
    }

    let (mut fx, ax) = axes();
    let bar = fx.bar(ax, &[0.0, 4.0], &[1.0, 2.0], |b, fx| {
        b.width = BarWidth::Data(fx.vector(&[1.0, 2.0]));
        b.offset = BarOffset::Widths(-0.5);
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    assert_span(
        at(&bars, -0.5).x,
        [-1.0, 0.0],
        "half of its own width of 1 to the left",
    );
    assert_span(
        at(&bars, 3.0).x,
        [2.0, 4.0],
        "half of its own width of 2 to the left",
    );
}

// Why: an absolute offset places a series a fixed distance along the category axis from its positions, such as a
// reference series drawn just beside the data, so its bars must be centred at the position plus that distance
// whatever their width, and a change of width must leave their centres where they were.
#[test]
fn an_absolute_offset_moves_every_bar_by_a_distance_in_data_units_whatever_its_width() {
    for width in [BarWidth::Pitch(0.8), BarWidth::Constant(0.2)] {
        let (mut fx, ax) = axes();
        let bar = fx.bar(ax, &[0.0, 1.0], &[1.0, 2.0], |b, _| {
            b.width = width;
            b.offset = BarOffset::Absolute(-0.25);
        });
        let scene = compile_figure(&fx.build());
        let bars = drawn(&scene, ax, bar);
        let w = match width {
            BarWidth::Constant(w) => w,
            _ => 0.8,
        };
        for p in [0.0, 1.0] {
            let centre = p - 0.25;
            assert_span(
                at(&bars, centre).x,
                [centre - w / 2.0, centre + w / 2.0],
                &format!("{width:?}: the bar at {p} is centred 0.25 to its left"),
            );
        }
    }
}

// Why: grouped bars are how several series are compared at each position, and with bars as a primitive they are
// ordinary series of a narrower width offset by whole widths, as the facade's helper writes them. Drawn that way,
// the columns of a cluster must stand side by side in order and touch, while the gap of the cluster separates one
// position from the next; this is the drawing the helper relies on. Three columns of a third of 0.8 at offsets −1,
// 0 and 1 widths split p ± 0.4 into thirds and leave 0.2 between clusters.
#[test]
fn series_offset_by_whole_widths_stand_side_by_side_and_touch() {
    let (mut fx, ax) = axes();
    let positions = [0.0, 1.0, 2.0, 3.0];
    let w = 0.8 / 3.0;
    let columns = [0, 1, 2].map(|k| {
        fx.bar(ax, &positions, &[1.0 + k as f64; 4], |b, _| {
            b.width = BarWidth::Pitch(w);
            b.offset = BarOffset::Widths(k as f64 - 1.0);
        })
    });
    let scene = compile_figure(&fx.build());
    let mut clusters = Vec::new();
    for p in positions {
        let mut previous_right = None;
        for (k, &column) in columns.iter().enumerate() {
            let bars = drawn(&scene, ax, column);
            let lo = p - 0.4 + k as f64 * w;
            let d = at(&bars, lo + w / 2.0);
            assert_span(d.x, [lo, lo + w], &format!("column {k} at {p}"));
            assert_span(
                d.y,
                [0.0, 1.0 + k as f64],
                &format!("column {k} at {p} keeps its own value"),
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

// Why: toggling a legend entry hides one series of a grouped chart; because each series draws only its own bars at
// its own offset, the others must not move, so that the reader can compare the views before and after.
#[test]
fn a_hidden_series_draws_nothing_and_the_series_beside_it_do_not_move() {
    let build = |hide: bool| {
        let (mut fx, ax) = axes();
        let columns = [0, 1, 2].map(|k| {
            fx.bar(ax, &[0.0, 1.0], &[1.0, 2.0], |b, _| {
                b.width = BarWidth::Pitch(0.8 / 3.0);
                b.offset = BarOffset::Widths(k as f64 - 1.0);
                b.visible = !(hide && k == 1);
            })
        });
        (compile_figure(&fx.build()), ax, columns)
    };
    let (shown, ax, columns) = build(false);
    let (hidden, _, _) = build(true);
    assert!(
        drawn(&hidden, ax, columns[1]).is_empty(),
        "the hidden series draws nothing"
    );
    for k in [0, 2] {
        let before = drawn(&shown, ax, columns[k]);
        let after = drawn(&hidden, ax, columns[k]);
        assert_eq!(after.len(), before.len(), "series {k} draws the same bars");
        for (b, a) in before.iter().zip(&after) {
            assert_span(a.x, b.x, &format!("series {k} stays where it was"));
        }
    }
}

// Why: a reversed axis is common for bars, such as a ranking read from the top down with `barh`, or depth plotted
// downwards. A bar must still span exactly its width and run from its base for its length: computed on the page
// with the assumption that the page coordinate grows with the data, an offset would move the bar the wrong way, or
// the rectangle would have a negative extent and draw nothing. The bars are mapped back to data, so these are the
// same spans as on unreversed axes, and the page order shows that the axes really are reversed.
#[test]
fn on_reversed_axes_each_bar_still_spans_its_width_at_its_offset_from_its_base_for_its_length() {
    let (mut fx, ax) = axes();
    fx.ax(ax).x.reverse = true;
    fx.ax(ax).y.reverse = true;
    let values = [3.0, -1.0, 2.0];
    let bar = fx.bar(ax, &[0.0, 1.0, 2.0], &values, |b, _| {
        b.width = BarWidth::Pitch(0.4);
        b.offset = BarOffset::Widths(0.5);
    });
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
        let d = at(&bars, p + 0.2);
        assert_span(
            d.x,
            [p, p + 0.4],
            &format!("bar {k} half a width to the right"),
        );
        assert_span(
            d.y,
            sorted2(0.0, value),
            &format!("bar {k} from the base for its length"),
        );
    }
    assert!(
        at(&bars, 0.2).page.x > at(&bars, 1.2).page.x,
        "the first bar is drawn on the right"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Logarithmic axes
// ---------------------------------------------------------------------------------------------------------------

// Why: a width and an offset are lengths in data units along the category axis, so on a logarithmic category axis a
// bar spans exactly from its centre less half its width to its centre plus half its width in data, each edge placed
// through the logarithmic scale, as the edges of a bin are; a histogram of logarithmic bins drawn with per-bar
// widths then covers each bin exactly.
#[test]
fn on_a_logarithmic_category_axis_each_bar_spans_its_width_in_data_units() {
    let (mut fx, ax) = axes();
    fx.ax(ax).x.scale = Scale::Log;
    // Bins [1, 10] and [10, 100], stored as centres and widths.
    let bar = fx.bar(ax, &[5.5, 55.0], &[1.0, 2.0], |b, fx| {
        b.width = BarWidth::Data(fx.vector(&[9.0, 90.0]));
    });
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    let bars = drawn(&scene, ax, bar);
    for (p, [lo, hi]) in [(5.5, [1.0, 10.0]), (55.0, [10.0, 100.0])] {
        let d = at(&bars, p);
        assert_close(d.page.x, xmap.to_figure(lo).min(xmap.to_figure(hi)), 1e-6);
        assert_close(
            d.page.right(),
            xmap.to_figure(lo).max(xmap.to_figure(hi)),
            1e-6,
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

// Why: on a logarithmic value axis what must be positive is the end of a bar, its base plus its length: a negative
// length hanging from a positive base to a positive end is an ordinary bar and is drawn between the two, whereas a
// bar whose end is not positive has nowhere to end and is not drawn, while the other bars of the series are.
#[test]
fn on_a_logarithmic_value_axis_a_bar_is_drawn_to_a_positive_end_and_left_out_when_its_end_is_not_positive()
 {
    let (mut fx, ax) = axes();
    fx.ax(ax).y.scale = Scale::Log;
    let bar = fx.bar(ax, &[0.0, 1.0, 2.0], &[-60.0, -100.0, 400.0], |b, _| {
        b.base = PerBar::Constant(100.0);
    });
    let scene = compile_figure(&fx.build());
    let bars = drawn(&scene, ax, bar);
    assert_eq!(bars.len(), 2, "the bar ending at 0 is left out: {bars:#?}");
    assert!(
        !covers(&bars, 1.0),
        "nothing is drawn for the bar ending at 0"
    );
    let hanging = at(&bars, 0.0);
    assert_close(hanging.y[0], 40.0, 1e-9 * 40.0);
    assert_close(hanging.y[1], 100.0, 1e-9 * 100.0);
    let rising = at(&bars, 2.0);
    assert_close(rising.y[0], 100.0, 1e-9 * 100.0);
    assert_close(rising.y[1], 500.0, 1e-9 * 500.0);
}

// ---------------------------------------------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------------------------------------------

// Why: bars of a width given as a fraction of the pitch fill slots one pitch wide, as the cells of a grid fill
// theirs, so the automatic limits along the category axis end exactly half a pitch beyond the outermost positions,
// rather than at the next tick value, which would leave an arbitrary empty margin at one end and not the other; with
// positions 1 to 3 they are 0.5 and 3.5, so the space beside the outer bars matches the space between bars. Bars that
// are offset beyond that half pitch, and bars of a fixed width or of widths per bar, which have no slot, end the
// limits exactly at their drawn extent, so that no bar is cut off and no space is invented.
#[test]
fn automatic_category_limits_end_half_a_pitch_beyond_the_positions_or_at_the_drawn_extent() {
    let (mut fx, ax) = axes();
    fx.bar(ax, &[1.0, 2.0, 3.0], &[3.0, 1.0, 2.0], |_, _| {});
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    assert_close(xmap.min, 0.5, TOL);
    assert_close(xmap.max, 3.5, TOL);

    // Offset by a whole width to the right, the bar at 3 spans 3.4 to 4.2, beyond the half pitch at 3.5, while on the
    // left the half pitch at 0.5 lies beyond the bar at 1, which starts at 1.4.
    let (mut fx, ax) = axes();
    fx.bar(ax, &[1.0, 2.0, 3.0], &[3.0, 1.0, 2.0], |b, _| {
        b.offset = BarOffset::Widths(1.0);
    });
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    assert_close(xmap.min, 0.5, TOL);
    assert_close(xmap.max, 4.2, TOL);

    let (mut fx, ax) = axes();
    fx.bar(ax, &[0.0, 3.0], &[1.0, 2.0], |b, _| {
        b.width = BarWidth::Constant(0.5);
    });
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    assert_close(xmap.min, -0.25, TOL);
    assert_close(xmap.max, 3.25, TOL);

    // A histogram of bins [0, 1], [1, 3] and [3, 6] ends at its outer bin edges.
    let (mut fx, ax) = axes();
    fx.bar(ax, &[0.5, 2.0, 4.5], &[4.0, 2.0, 1.0], |b, fx| {
        b.width = BarWidth::Data(fx.vector(&[1.0, 2.0, 3.0]));
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

// Why: a bar is read against its base, so the automatic value limits include the bases as well as the ends of the
// bars, the end being the base plus the length: bars rising from 0 must show 0, or they would appear to start at the
// lowest value and their lengths would misstate the ratios between them; and floating bars and stacked parts, whose
// ends lie beyond their lengths, must be shown whole.
#[test]
fn automatic_value_limits_include_the_bases_and_the_ends_of_the_bars() {
    let (mut fx, ax) = axes();
    fx.bar(ax, &[0.0, 1.0, 2.0], &[30.0, 50.0, 40.0], |_, _| {});
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert_close(ymap.min, 0.0, 1e-12);
    assert!(ymap.max >= 50.0);

    let (mut fx, ax) = axes();
    fx.bar(ax, &[0.0, 1.0], &[2.0, 5.0], |b, _| {
        b.base = PerBar::Constant(10.0)
    });
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert!(
        ymap.min <= 10.0 && ymap.max >= 15.0,
        "the base 10 and the end 10 + 5: {ymap:?}"
    );

    // Parts stacked as the helper stacks them: the second series rests on the first, positive and negative apart.
    let (mut fx, ax) = axes();
    fx.bar(ax, &[0.0, 1.0], &[4.0, -3.0], |_, _| {});
    fx.bar(ax, &[0.0, 1.0], &[6.0, -2.0], |b, fx| {
        b.base = PerBar::Data(fx.vector(&[4.0, -3.0]));
    });
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert!(ymap.max >= 10.0, "the end 4 + 6 is shown: {ymap:?}");
    assert!(ymap.min <= -5.0, "the end −3 − 2 is shown: {ymap:?}");
}

// Why: a bar chart whose tallest bar ends on a round tick value would otherwise have its axis end exactly there, so
// that the bar touches the box and reads as cut off; the margin adds a fraction of the span of the bars beyond their
// ends, before the limits are rounded to ticks, so the limit moves past the tallest bar. The base side takes no
// margin, so that bars rising from 0 keep 0 at the edge of the axis; a margin of 0 gives the limits that the bars
// alone give, ending at the round value; and bars of negative lengths take the margin below, where their ends are.
#[test]
fn the_margin_moves_the_value_limit_beyond_the_ends_of_the_bars_but_not_beyond_the_base() {
    let limits = |values: &[f64], edit: &dyn Fn(&mut ironlab_ir::Bar)| {
        let (mut fx, ax) = axes();
        fx.bar(ax, &[0.0, 1.0, 2.0], values, |b, _| edit(b));
        let scene = compile_figure(&fx.build());
        let (xmap, ymap) = axis_maps(&scene, ax);
        (xmap, ymap)
    };

    let (_, ymap) = limits(&[1.0, 5.0, 3.0], &|_| {});
    assert!(
        ymap.max > 5.0 + 1e-9,
        "the tallest bar ends at the round value 5, and the limit lies beyond it: {ymap:?}"
    );
    assert_close(ymap.min, 0.0, 1e-12);

    let (_, ymap) = limits(&[1.0, 5.0, 3.0], &|b| b.margin = 0.0);
    assert_close(ymap.max, 5.0, 1e-12);
    assert_close(ymap.min, 0.0, 1e-12);

    let (_, ymap) = limits(&[-1.0, -5.0, -3.0], &|_| {});
    assert!(
        ymap.min < -5.0 - 1e-9,
        "the margin lies below bars of negative lengths: {ymap:?}"
    );
    assert_close(ymap.max, 0.0, 1e-12);

    // Floating bars rising from a base of 10 take no margin below it: their span is 10 to 15, so a margin of 0.5 adds
    // 2.5 above the ends and nothing below the base, which already lies on a tick. A margin applied on the base side
    // as well would move the lower limit to 7.5 and round it down further.
    let (_, ymap) = limits(&[2.0, 5.0, 3.0], &|b| {
        b.base = PerBar::Constant(10.0);
        b.margin = 0.5;
    });
    assert_close(ymap.min, 10.0, 1e-12);
    assert!(ymap.max >= 17.5, "{ymap:?}");

    // Bars on both sides of the base take the margin on both sides, a fraction of the whole span of 6.
    let (_, ymap) = limits(&[-2.0, 4.0, 1.0], &|b| b.margin = 0.5);
    assert!(ymap.max >= 4.0 + 3.0, "{ymap:?}");
    assert!(ymap.min <= -2.0 - 3.0, "{ymap:?}");

    // Horizontal bars take the margin along x, and none along their category axis y.
    let (xmap, ymap) = limits(&[1.0, 5.0, 3.0], &|b| {
        b.orientation = Orientation::Horizontal
    });
    assert!(xmap.max > 5.0 + 1e-9, "{xmap:?}");
    assert_close(xmap.min, 0.0, 1e-12);
    assert_close(ymap.min, -0.5, TOL);
    assert_close(ymap.max, 2.5, TOL);
}

// Why: manual limits are what the author asked for, so the margin, which only places automatic limits, must leave
// them exactly as given.
#[test]
fn manual_value_limits_ignore_the_margin() {
    let (mut fx, ax) = axes();
    fx.ax(ax).y.limits = Limits::Manual { min: 0.0, max: 5.0 };
    fx.bar(ax, &[0.0, 1.0], &[2.0, 5.0], |b, _| b.margin = 0.5);
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert_close(ymap.min, 0.0, 1e-12);
    assert_close(ymap.max, 5.0, 1e-12);
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
