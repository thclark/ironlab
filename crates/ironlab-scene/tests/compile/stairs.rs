//! Stairs (MATLAB's `stairs`, Matplotlib's `stairs` and `step`): the stepped outline through values held between
//! edges or at samples, its base and fill, and the limits it sets.
//!
//! Stairs are read by the level of each step and where it changes, so the tests map the drawn outline back through
//! the axis maps into data and compare its vertices with the outline the figure describes, written out by hand. The
//! vertices are compared after dropping repeated points and points that lie between their neighbours on a straight
//! run, so that the tests pin the shape rather than how many points the compiler happens to emit for it.

use ironlab_ir::{Color, ColorSpec, NodeId, Orientation, Scale, StepAt};
use ironlab_scene::Scene;
use ironlab_scene::display::Rgba;

use crate::common::{COLOUR_ORDER, Fx, compile_figure, rgb8, rgb8_close};
use crate::probe::{assert_close, axis_maps, from_source, leaves, signed_area};

const TOL: f64 = 1e-9;

// ---------------------------------------------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------------------------------------------

/// Removes repeated vertices and vertices that lie strictly between their neighbours on a straight run, which change
/// nothing that is drawn. A vertex at which the outline turns back on itself is kept.
fn simplify(points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let close = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-7 && (a[1] - b[1]).abs() < 1e-7;
    let mut deduped: Vec<[f64; 2]> = Vec::new();
    for p in points {
        if deduped.last().is_none_or(|&q| !close(p, q)) {
            deduped.push(p);
        }
    }
    let mut out: Vec<[f64; 2]> = Vec::new();
    for (k, &p) in deduped.iter().enumerate() {
        if let (Some(&a), Some(&c)) = (out.last(), deduped.get(k + 1)) {
            let (u, v) = ([p[0] - a[0], p[1] - a[1]], [c[0] - p[0], c[1] - p[1]]);
            let cross = u[0] * v[1] - u[1] * v[0];
            let onward = u[0] * v[0] + u[1] * v[1] > 0.0;
            if cross.abs() < 1e-9 && onward {
                continue;
            }
        }
        out.push(p);
    }
    out
}

/// Returns the subpaths of the outline that `stairs` stroked in the 2D axes `ax`, in data coordinates, simplified.
fn outline(scene: &Scene, ax: NodeId, stairs: NodeId) -> Vec<Vec<[f64; 2]>> {
    let (xmap, ymap) = axis_maps(scene, ax);
    from_source(&leaves(scene), stairs)
        .iter()
        .filter(|l| l.path().is_some_and(|p| p.stroke.is_some()))
        .flat_map(|l| l.subpaths())
        .map(|sub| {
            simplify(
                sub.iter()
                    .map(|p| [xmap.to_data(p.x), ymap.to_data(p.y)])
                    .collect(),
            )
        })
        .collect()
}

/// Returns the colour of the stroke of the outline.
fn line_colour(scene: &Scene, stairs: NodeId) -> Rgba {
    from_source(&leaves(scene), stairs)
        .iter()
        .find_map(|l| l.path().and_then(|p| p.stroke.as_ref()).map(|s| s.color))
        .expect("the outline is stroked")
}

/// Returns the filled regions of `stairs` in data coordinates, one polygon per subpath, and their fill colour.
fn fill(scene: &Scene, ax: NodeId, stairs: NodeId) -> (Vec<Vec<[f64; 2]>>, Option<Rgba>) {
    let (xmap, ymap) = axis_maps(scene, ax);
    let filled: Vec<_> = from_source(&leaves(scene), stairs)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.fill.is_some()))
        .collect();
    let colour = filled.first().and_then(|l| l.path()?.fill).map(|f| f.color);
    let regions = filled
        .iter()
        .flat_map(|l| l.subpaths())
        .map(|sub| {
            sub.iter()
                .map(|p| [xmap.to_data(p.x), ymap.to_data(p.y)])
                .collect()
        })
        .collect();
    (regions, colour)
}

/// Returns the total area of polygons in data coordinates.
fn area(regions: &[Vec<[f64; 2]>]) -> f64 {
    regions
        .iter()
        .map(|r| {
            let points: Vec<_> = r
                .iter()
                .map(|p| ironlab_scene::display::Point::new(p[0], p[1]))
                .collect();
            signed_area(&points).abs()
        })
        .sum()
}

/// Asserts that an outline has the vertices `expected`, in either direction.
#[track_caller]
fn assert_outline(actual: &[[f64; 2]], expected: &[[f64; 2]], what: &str) {
    let matches = |candidate: &[[f64; 2]]| {
        candidate.len() == expected.len()
            && candidate
                .iter()
                .zip(expected)
                .all(|(a, e)| (a[0] - e[0]).abs() <= TOL && (a[1] - e[1]).abs() <= TOL)
    };
    let reversed: Vec<_> = actual.iter().rev().copied().collect();
    assert!(
        matches(actual) || matches(&reversed),
        "{what}: drawn as {actual:?}, expected {expected:?}"
    );
}

#[track_caller]
fn assert_colour(actual: Rgba, expected: [u8; 3], what: &str) {
    assert!(
        rgb8_close(rgb8(actual), expected),
        "{what}: painted {:?}, expected {expected:?}",
        rgb8(actual)
    );
}

fn axes() -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    (fx, ax)
}

/// Edges of three uneven bins, so that a step placed by index rather than by edge is caught.
const EDGES: [f64; 4] = [0.0, 1.0, 3.0, 4.0];
const VALUES: [f64; 3] = [2.0, 5.0, 1.0];

// ---------------------------------------------------------------------------------------------------------------
// Outlines
// ---------------------------------------------------------------------------------------------------------------

// Why: stairs between edges are the outline of a histogram: each value is held across its whole bin and the outline
// rises or falls at the shared edge, so the level of each step and the place it changes are the data. Without a base
// the outline is open: it starts at the first value and ends at the last, with nothing drawn down to any axis.
#[test]
fn stairs_between_edges_hold_each_value_across_its_bin_and_step_at_the_shared_edges() {
    let (mut fx, ax) = axes();
    let stairs = fx.stairs(ax, &EDGES, &VALUES, |_| {});
    let scene = compile_figure(&fx.build());
    let about: Vec<_> = scene
        .warnings
        .iter()
        .filter(|w| w.node == Some(stairs))
        .collect();
    assert!(
        about.is_empty(),
        "valid stairs compile without a warning: {about:?}"
    );
    let drawn = outline(&scene, ax, stairs);
    assert_eq!(drawn.len(), 1, "one unbroken outline: {drawn:?}");
    assert_outline(
        &drawn[0],
        &[
            [0.0, 2.0],
            [1.0, 2.0],
            [1.0, 5.0],
            [3.0, 5.0],
            [3.0, 1.0],
            [4.0, 1.0],
        ],
        "open stairs",
    );
}

// Why: a base closes the outline, as a histogram's outline drops to zero at its outer edges; the drop happens at both
// ends and only there, so that the region enclosed is exactly the area under the steps.
#[test]
fn a_base_drops_the_outline_to_it_at_both_ends() {
    for base in [0.0, -1.0] {
        let (mut fx, ax) = axes();
        let stairs = fx.stairs(ax, &EDGES, &VALUES, |s| s.base = Some(base));
        let scene = compile_figure(&fx.build());
        let drawn = outline(&scene, ax, stairs);
        assert_eq!(drawn.len(), 1, "one outline: {drawn:?}");
        assert_outline(
            &drawn[0],
            &[
                [0.0, base],
                [0.0, 2.0],
                [1.0, 2.0],
                [1.0, 5.0],
                [3.0, 5.0],
                [3.0, 1.0],
                [4.0, 1.0],
                [4.0, base],
            ],
            &format!("stairs on a base of {base}"),
        );
    }
}

// Why: stairs through samples must step where the author said: `post` (MATLAB's `stairs`) holds each value from its
// own sample up to the next, `pre` from the previous sample up to its own, and `mid` changes value midway between
// the samples, exactly as Matplotlib's `step` draws them. Drawing one where another was asked moves every change of
// level by a sample, which misdates every event in a time series.
#[test]
fn stairs_through_samples_step_after_before_or_midway_between_the_samples_as_asked() {
    let x = [0.0, 1.0, 2.0, 4.0];
    let values = [1.0, 3.0, 2.0, 5.0];
    let cases: [(StepAt, &[[f64; 2]]); 3] = [
        (
            StepAt::Post,
            &[
                [0.0, 1.0],
                [1.0, 1.0],
                [1.0, 3.0],
                [2.0, 3.0],
                [2.0, 2.0],
                [4.0, 2.0],
                [4.0, 5.0],
            ],
        ),
        (
            StepAt::Pre,
            &[
                [0.0, 1.0],
                [0.0, 3.0],
                [1.0, 3.0],
                [1.0, 2.0],
                [2.0, 2.0],
                [2.0, 5.0],
                [4.0, 5.0],
            ],
        ),
        (
            StepAt::Mid,
            &[
                [0.0, 1.0],
                [0.5, 1.0],
                [0.5, 3.0],
                [1.5, 3.0],
                [1.5, 2.0],
                [3.0, 2.0],
                [3.0, 5.0],
                [4.0, 5.0],
            ],
        ),
    ];
    for (step, expected) in cases {
        let (mut fx, ax) = axes();
        let stairs = fx.stairs_at(ax, &x, &values, step, |_| {});
        let scene = compile_figure(&fx.build());
        let drawn = outline(&scene, ax, stairs);
        assert_eq!(drawn.len(), 1, "{step:?}: one outline: {drawn:?}");
        assert_outline(&drawn[0], expected, &format!("{step:?}"));
    }
}

// Why: edges may decrease as well as increase, as a profile measured from the far end is stored; the steps must
// still hold each value between its own two edges, so that the picture is the same as for the edges in increasing
// order with the values reversed.
#[test]
fn decreasing_edges_hold_each_value_between_its_own_two_edges() {
    let (mut fx, ax) = axes();
    let stairs = fx.stairs(ax, &[4.0, 3.0, 1.0, 0.0], &[1.0, 5.0, 2.0], |_| {});
    let scene = compile_figure(&fx.build());
    let drawn = outline(&scene, ax, stairs);
    assert_eq!(drawn.len(), 1);
    assert_outline(
        &drawn[0],
        &[
            [0.0, 2.0],
            [1.0, 2.0],
            [1.0, 5.0],
            [3.0, 5.0],
            [3.0, 1.0],
            [4.0, 1.0],
        ],
        "decreasing edges",
    );
}

// Why: horizontal stairs show a profile against height or depth, which is read upwards; the edges then lie along y
// and the values along x, so the outline is that of upright stairs with the axes exchanged.
#[test]
fn horizontal_stairs_place_the_edges_along_y_and_the_values_along_x() {
    let (mut fx, ax) = axes();
    let stairs = fx.stairs(ax, &EDGES, &VALUES, |s| {
        s.orientation = Orientation::Horizontal
    });
    let scene = compile_figure(&fx.build());
    let drawn = outline(&scene, ax, stairs);
    assert_eq!(drawn.len(), 1);
    assert_outline(
        &drawn[0],
        &[
            [2.0, 0.0],
            [2.0, 1.0],
            [5.0, 1.0],
            [5.0, 3.0],
            [1.0, 3.0],
            [1.0, 4.0],
        ],
        "horizontal stairs",
    );
}

// Why: a missing value (NaN) has no level to draw, so its bin is left out and the outline is broken there, as a line
// is broken at a missing point; joining across the gap would invent a step that the data does not hold.
#[test]
fn a_value_that_is_not_finite_breaks_the_outline_and_its_bin_is_not_drawn() {
    let (mut fx, ax) = axes();
    let stairs = fx.stairs(
        ax,
        &[0.0, 1.0, 2.0, 3.0, 4.0],
        &[1.0, f64::NAN, 2.0, 3.0],
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    let mut drawn = outline(&scene, ax, stairs);
    drawn.sort_by(|a, b| {
        let lo = |s: &Vec<[f64; 2]>| s.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        lo(a).total_cmp(&lo(b))
    });
    assert_eq!(drawn.len(), 2, "the outline is broken in two: {drawn:?}");
    assert_outline(
        &drawn[0],
        &[[0.0, 1.0], [1.0, 1.0]],
        "the piece before the gap",
    );
    assert_outline(
        &drawn[1],
        &[[2.0, 2.0], [3.0, 2.0], [3.0, 3.0], [4.0, 3.0]],
        "the piece after the gap",
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Fill
// ---------------------------------------------------------------------------------------------------------------

// Why: a filled histogram is read by area, so the fill must be exactly the region between the outline and the base:
// here 2 × 1 + 5 × 2 + 1 × 1 = 13 square units above a base of 0. An automatic fill takes the colour of the series,
// as the line does, so that the filled stairs are keyed and recognised by one colour.
#[test]
fn the_fill_is_the_region_between_the_outline_and_the_base_in_the_series_colour() {
    let (mut fx, ax) = axes();
    let stairs = fx.stairs(ax, &EDGES, &VALUES, |s| {
        s.base = Some(0.0);
        s.fill = ColorSpec::Auto;
    });
    let scene = compile_figure(&fx.build());
    let (regions, colour) = fill(&scene, ax, stairs);
    assert_close(area(&regions), 13.0, 1e-6);
    assert_colour(
        colour.expect("filled"),
        COLOUR_ORDER[0],
        "an automatic fill",
    );
    assert_colour(line_colour(&scene, stairs), COLOUR_ORDER[0], "the line");
}

// Why: a missing value leaves a gap in the fill as it does in the outline; filling across it would show an area, and
// so a count, where the data has none.
#[test]
fn a_value_that_is_not_finite_leaves_a_gap_in_the_fill() {
    let (mut fx, ax) = axes();
    let stairs = fx.stairs(ax, &[0.0, 1.0, 2.0, 3.0], &[1.0, f64::NAN, 2.0], |s| {
        s.base = Some(0.0);
        s.fill = ColorSpec::Rgba {
            color: Color::rgb(0.5, 0.5, 0.5),
        };
    });
    let scene = compile_figure(&fx.build());
    let (regions, _) = fill(&scene, ax, stairs);
    assert_close(area(&regions), 3.0, 1e-6);
    for region in &regions {
        let (lo, hi) = region
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[0]), hi.max(p[0]))
            });
        assert!(
            hi <= 1.0 + TOL || lo >= 2.0 - TOL,
            "no fill spans the missing bin [1, 2]: {region:?}"
        );
    }
}

// Why: a based outline broken by a missing value is two histograms side by side, each of which must read as a
// closed shape down to the base, as matplotlib draws each run of finite values; an outline left hanging at the gap
// would look like a step down to nothing, and a fill closed only at the outer ends would have no edge at the gap.
// So each unbroken piece drops to the base at both of its ends, and the fill under each piece is exactly its area.
#[test]
fn with_a_base_each_piece_of_an_outline_broken_by_a_missing_value_is_closed_to_the_base() {
    let (mut fx, ax) = axes();
    let stairs = fx.stairs(
        ax,
        &[0.0, 1.0, 2.0, 3.0, 4.0],
        &[1.0, f64::NAN, 2.0, 3.0],
        |s| {
            s.base = Some(-1.0);
            s.fill = ColorSpec::Auto;
        },
    );
    let scene = compile_figure(&fx.build());
    let mut drawn = outline(&scene, ax, stairs);
    drawn.sort_by(|a, b| {
        let lo = |s: &Vec<[f64; 2]>| s.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        lo(a).total_cmp(&lo(b))
    });
    assert_eq!(drawn.len(), 2, "the outline is broken in two: {drawn:?}");
    assert_outline(
        &drawn[0],
        &[[0.0, -1.0], [0.0, 1.0], [1.0, 1.0], [1.0, -1.0]],
        "the piece before the gap",
    );
    assert_outline(
        &drawn[1],
        &[
            [2.0, -1.0],
            [2.0, 2.0],
            [3.0, 2.0],
            [3.0, 3.0],
            [4.0, 3.0],
            [4.0, -1.0],
        ],
        "the piece after the gap",
    );
    let (regions, _) = fill(&scene, ax, stairs);
    // 1 × 2 before the gap, and 1 × 3 + 1 × 4 after it, above a base of −1.
    assert_close(area(&regions), 9.0, 1e-6);
}

// Why: the line and fill opacities fade each independently, so that a translucent fill can sit under an opaque
// outline, the usual look of overlaid histograms; each multiplies the alpha of its colour.
#[test]
fn line_and_fill_opacities_multiply_the_alpha_of_their_colours() {
    let (mut fx, ax) = axes();
    let stairs = fx.stairs(ax, &EDGES, &VALUES, |s| {
        s.base = Some(0.0);
        s.fill = ColorSpec::Rgba {
            color: Color::rgba(0.0, 0.0, 1.0, 0.8),
        };
        s.fill_opacity = 0.5;
        s.line_opacity = 0.25;
    });
    let scene = compile_figure(&fx.build());
    let (_, colour) = fill(&scene, ax, stairs);
    assert_close(f64::from(colour.expect("filled").a), 0.4, 1e-6);
    assert_close(f64::from(line_colour(&scene, stairs).a), 0.25, 1e-6);
}

// Why: stairs take the colour order as series do, so that a histogram outline plotted over a line is told apart from
// it without the author choosing a colour.
#[test]
fn an_automatic_line_takes_the_next_colour_of_the_order() {
    let (mut fx, ax) = axes();
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let stairs = fx.stairs(ax, &EDGES, &VALUES, |_| {});
    let scene = compile_figure(&fx.build());
    assert_colour(
        line_colour(&scene, stairs),
        COLOUR_ORDER[1],
        "the second colour",
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Limits and logarithmic axes
// ---------------------------------------------------------------------------------------------------------------

// Why: stairs cover their edges as a grid covers its cells, so the automatic limits along the edges end exactly at
// the first and last edges rather than at the next tick; along the values they include the base, so a histogram on
// a base of 0 shows 0, while open stairs, which draw nothing down to any base, do not pull the axis to 0.
#[test]
fn automatic_limits_end_at_the_outer_edges_and_include_a_base_but_not_an_absent_one() {
    let edges = [0.5, 1.0, 3.0, 4.2];
    let values = [5.0, 6.0, 7.0];

    let (mut fx, ax) = axes();
    fx.stairs(ax, &edges, &values, |s| s.base = Some(0.0));
    let scene = compile_figure(&fx.build());
    let (xmap, ymap) = axis_maps(&scene, ax);
    assert_close(xmap.min, 0.5, TOL);
    assert_close(xmap.max, 4.2, TOL);
    assert_close(ymap.min, 0.0, 1e-12);
    assert!(ymap.max >= 7.0);

    let (mut fx, ax) = axes();
    fx.stairs(ax, &edges, &values, |_| {});
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert!(ymap.min > 0.0, "open stairs do not reach for 0: {ymap:?}");
    assert!(ymap.min <= 5.0 && ymap.max >= 7.0);

    let (mut fx, ax) = axes();
    fx.stairs_at(ax, &[1.0, 2.0, 5.0], &values, StepAt::Mid, |_| {});
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    assert_close(xmap.min, 1.0, TOL);
    assert_close(xmap.max, 5.0, TOL);
}

// Why: a logarithmic value axis has no place for a base of 0, yet a histogram of counts spanning decades is commonly
// drawn on one; the outline then drops to the low edge of the plot area at both ends instead, so that it stays
// closed and its steps still read their values.
#[test]
fn on_a_logarithmic_value_axis_a_base_that_is_not_positive_drops_to_the_low_edge_of_the_plot() {
    let (mut fx, ax) = axes();
    fx.ax(ax).y.scale = Scale::Log;
    let stairs = fx.stairs(ax, &EDGES, &[20.0, 300.0, 50.0], |s| s.base = Some(0.0));
    let scene = compile_figure(&fx.build());
    let (_, ymap) = axis_maps(&scene, ax);
    assert!(ymap.min > 0.0, "the axis does not reach for 0: {ymap:?}");
    let drawn = outline(&scene, ax, stairs);
    assert_eq!(drawn.len(), 1);
    let ends = [drawn[0][0], *drawn[0].last().unwrap()];
    for end in ends {
        assert_close(end[1], ymap.min, 1e-9 * ymap.min);
    }
}

// Why: stairs of no values are valid (ADR 0012) and draw nothing, without stopping the rest of the axes.
#[test]
fn stairs_of_no_values_draw_nothing() {
    let (mut fx, ax) = axes();
    // No values need one edge, which is the shape validation accepts.
    let empty = fx.stairs(ax, &[0.0], &[], |_| {});
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let scene = compile_figure(&fx.build());
    assert!(from_source(&leaves(&scene), empty).is_empty());
    assert!(!from_source(&leaves(&scene), line).is_empty());
}
