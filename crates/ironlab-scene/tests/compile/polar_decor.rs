//! The decorations, layout and hit geometry of polar axes: the angular tick labels for each kind of angle unit, the fit
//! of the circle or sector into the plot rectangle, the rim, hole and sector edges of the frame, the spokes and arcs of
//! the grid, the placement of the angular and radial tick labels, the sector clip that encloses the artists, and the
//! inverse map that front ends use to read a pointer position as an angle and a radius.
//!
//! Expected positions are computed from the published polar hit geometry (the centre, and the rim and hole radii that
//! the radius map runs between), never from the compiler's internals. The plot rectangle is the hit map's `plot_rect`,
//! which these tests take to be the rectangle the layout gives the axes, within which the sector is fitted.
//!
//! The data-to-page map of artists, clipping and automatic limits are tested in [`crate::polar`].

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use ironlab_ir::{
    AngleUnits, Figure, Limits, NodeId, PolarView, Projection, Text, TickLabels, Ticks,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{Item, ItemKind, Point, Rect, Sector, Stroke};
use ironlab_scene::hit::{AxesHitKind, AxisMap};

use crate::common::{Fx, compile_figure, text};
use crate::probe::{
    Leaf, assert_close, axes_hit, centre, from_source, glyph_runs, leaves, outlines, parse_number,
    runs_with_text,
};

// ---------------------------------------------------------------------------------------------------------------------
// Fixtures and geometry helpers
// ---------------------------------------------------------------------------------------------------------------------

/// A polar view with the given angle units and all else at its defaults (zero to the east, no hole, radial labels
/// at the default spoke).
fn view(angle_units: AngleUnits) -> PolarView {
    PolarView {
        angle_units,
        ..PolarView::default()
    }
}

/// A one-tile figure holding a polar axes with `polar` as its view and a series inside the radial limits 0 to 1,
/// which are fixed so that the radial map is known; `edit` adjusts the axes before the figure is built.
fn polar_figure(polar: PolarView, edit: impl FnOnce(&mut ironlab_ir::Axes)) -> (Figure, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes_in(crate::common::cell(0, 0, 1, 1), Projection::Polar { polar });
    let turn = polar.angle_units.turn();
    fx.line(
        ax,
        &[0.0, 0.25 * turn, 0.5 * turn, 0.75 * turn],
        &[0.5, 0.8, 0.6, 0.9],
        None,
        |_| {},
    );
    let axes = fx.ax(ax);
    axes.y.limits = Limits::Manual { min: 0.0, max: 1.0 };
    edit(axes);
    (fx.build(), ax)
}

/// Compiles a polar figure as [`polar_figure`] builds it.
fn polar_scene(polar: PolarView, edit: impl FnOnce(&mut ironlab_ir::Axes)) -> (Scene, NodeId) {
    let (figure, ax) = polar_figure(polar, edit);
    (compile_figure(&figure), ax)
}

/// The geometry of a polar axes as the hit map publishes it.
struct PolarGeometry {
    centre: Point,
    x: AxisMap,
    y: AxisMap,
    /// The radius of the rim, in points: the larger end of the radius map.
    rim: f64,
    /// The radius of the edge of the hole, in points: the smaller end of the radius map.
    hole: f64,
    plot: Rect,
}

impl PolarGeometry {
    /// Returns the page point at page angle `phi` (radians anticlockwise from east) and radius `rho` (points).
    fn place(&self, phi: f64, rho: f64) -> Point {
        Point::new(
            self.centre.x + rho * phi.cos(),
            self.centre.y - rho * phi.sin(),
        )
    }

    /// Returns the page angle of a page point about the centre, in radians anticlockwise from east.
    fn angle_of(&self, p: Point) -> f64 {
        (-(p.y - self.centre.y)).atan2(p.x - self.centre.x)
    }

    fn radius_of(&self, p: Point) -> f64 {
        (p.x - self.centre.x).hypot(p.y - self.centre.y)
    }
}

/// Returns the published polar geometry of axes `id`, panicking when the axes is not polar.
#[track_caller]
fn polar_geometry(scene: &Scene, id: NodeId) -> PolarGeometry {
    let hit = axes_hit(scene, id);
    match hit.kind {
        AxesHitKind::Polar { centre, x, y } => PolarGeometry {
            centre,
            x,
            y,
            rim: y.start.max(y.end),
            hole: y.start.min(y.end),
            plot: hit.plot_rect,
        },
        ref other => panic!("axes {id} is not polar: {other:?}"),
    }
}

/// Returns the difference between two angles reduced to (−π, π].
fn angle_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(TAU);
    if d > PI { d - TAU } else { d }
}

#[track_caller]
fn assert_angle_close(actual: f64, expected: f64, tol: f64, what: &str) {
    let d = angle_diff(actual, expected);
    assert!(
        d.abs() <= tol,
        "{what}: expected angle {expected} rad (mod 2π) ± {tol}, got {actual} ({d} off)"
    );
}

/// Returns the distance from `p` to the nearest point of `r`.
fn distance_to_rect(p: Point, r: Rect) -> f64 {
    let nx = p.x.clamp(r.x, r.right());
    let ny = p.y.clamp(r.y, r.bottom());
    (p.x - nx).hypot(p.y - ny)
}

/// Returns the bounding box of the glyph run whose text is exactly `label`, which must be drawn exactly once.
#[track_caller]
fn label_box(leaves: &[Leaf], label: &str) -> Rect {
    let runs = runs_with_text(leaves, label);
    assert_eq!(
        runs.len(),
        1,
        "the label {label:?} is drawn exactly once; drawn texts: {:?}",
        texts(leaves)
    );
    runs[0].bbox().expect("a drawn label has ink")
}

/// Returns the texts of every glyph run, in paint order.
fn texts(leaves: &[Leaf]) -> Vec<String> {
    glyph_runs(leaves)
        .into_iter()
        .map(|(_, g)| g.text.clone())
        .collect()
}

/// Returns the texts of the angular tick labels: the runs that end with a degree sign.
fn degree_labels(leaves: &[Leaf]) -> Vec<String> {
    texts(leaves)
        .into_iter()
        .filter(|t| t.ends_with('°'))
        .collect()
}

/// Asserts that an angular tick label lies outside the rim on the ray from the centre at page angle `phi`: its centre is
/// on the ray, within the tolerance of the difference between a label's ink and its layout box, and its nearest point
/// to the centre is beyond the rim by no more than a tick pad and the slack of the ink box.
#[track_caller]
fn assert_label_on_ray(g: &PolarGeometry, label: Rect, phi: f64, what: &str) {
    let c = centre(label);
    let (dx, dy) = (c.x - g.centre.x, c.y - g.centre.y);
    let (ux, uy) = (phi.cos(), -phi.sin());
    let along = dx * ux + dy * uy;
    let across = (dx * uy - dy * ux).abs();
    assert!(
        along > g.rim,
        "{what}: the label lies beyond the rim along its spoke ({along} ≤ rim {})",
        g.rim
    );
    assert!(
        across <= 3.0,
        "{what}: the label is centred on the ray of its tick ({across} pt off it)"
    );
    let nearest = distance_to_rect(g.centre, label);
    assert!(
        nearest >= g.rim + 0.5 && nearest <= g.rim + 15.0,
        "{what}: the nearest point of the label is a pad outside the rim (rim {}, nearest {nearest})",
        g.rim
    );
}

/// Returns the stroke of the box and of the grid of a reference Cartesian axes, which the polar frame and grid share.
fn cartesian_strokes() -> (Stroke, Stroke) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let axes = fx.ax(ax);
    axes.x.grid = true;
    axes.box_ = true;
    let scene = compile_figure(&fx.build());
    let frame = outlines(&scene, ax)
        .first()
        .and_then(|l| l.path().and_then(|p| p.stroke.clone()))
        .expect("a boxed Cartesian axes draws its box");
    let grid = from_source(&leaves(&scene), ax)
        .into_iter()
        .filter_map(|l| l.path().and_then(|p| p.stroke.clone()))
        .find(|s| *s != frame)
        .expect("a gridded Cartesian axes draws grid lines in a stroke other than its box's");
    (frame, grid)
}

/// Returns the subpaths (on-curve points, in figure space) of the axes' own paths drawn with `stroke`.
fn subpaths_with_stroke(scene: &Scene, ax: NodeId, stroke: &Stroke) -> Vec<Vec<Point>> {
    from_source(&leaves(scene), ax)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.stroke.as_ref() == Some(stroke)))
        .flat_map(|l| l.subpaths())
        .collect()
}

/// Returns the straight segments of the axes' own paths drawn with `stroke`, closing segments included.
fn segments_with_stroke(scene: &Scene, ax: NodeId, stroke: &Stroke) -> Vec<(Point, Point)> {
    from_source(&leaves(scene), ax)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.stroke.as_ref() == Some(stroke)))
        .flat_map(|l| l.line_segments())
        .collect()
}

/// A subpath classified by its shape about the centre of a polar axes.
#[derive(Debug, PartialEq)]
enum Shape {
    /// Every on-curve point at one radius (an arc or a circle), with that radius.
    Arc(f64),
    /// Two points on one ray from the centre, with the page angle of the ray and the two radii, inner first.
    Spoke(f64, f64, f64),
    Other,
}

fn classify(g: &PolarGeometry, points: &[Point]) -> Shape {
    let radii: Vec<f64> = points.iter().map(|&p| g.radius_of(p)).collect();
    let (lo, hi) = radii
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &r| {
            (a.min(r), b.max(r))
        });
    if points.len() >= 2 && hi - lo < 0.05 && lo > 0.05 {
        return Shape::Arc(lo);
    }
    if let [a, b] = points {
        let (inner, outer) = if g.radius_of(*a) <= g.radius_of(*b) {
            (*a, *b)
        } else {
            (*b, *a)
        };
        let phi = g.angle_of(outer);
        let on_ray = g.radius_of(inner) < 0.05 || angle_diff(g.angle_of(inner), phi).abs() < 1e-6;
        if on_ray {
            return Shape::Spoke(phi, g.radius_of(inner), g.radius_of(outer));
        }
    }
    Shape::Other
}

/// Returns every sector clip of the display list, in paint order.
fn sector_clips(scene: &Scene) -> Vec<Sector> {
    fn walk(items: &[Item], out: &mut Vec<Sector>) {
        for item in items {
            match &item.kind {
                ItemKind::SectorClip { sector, items } => {
                    out.push(*sector);
                    walk(items, out);
                }
                ItemKind::Group { items, .. }
                | ItemKind::Dense { items, .. }
                | ItemKind::Depth { items } => walk(items, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&scene.display_list.items, &mut out);
    out
}

fn deg(d: f64) -> f64 {
    d.to_radians()
}

// ---------------------------------------------------------------------------------------------------------------------
// Angular ticks and their labels
// ---------------------------------------------------------------------------------------------------------------------

// WHY: a compass-like ring of eight labels every 45° is what every polar plot in degrees shows, and a reader would see
// "360°" printed over "0°" if the tick at the upper limit of a full turn were kept; the degree sign tells the reader
// that the ring is an angle rather than another number axis.
#[test]
fn a_full_turn_in_degrees_is_labelled_every_45_degrees_with_a_degree_sign_and_no_360() {
    let (scene, _) = polar_scene(view(AngleUnits::Degrees), |a| {
        a.y.tick_labels = TickLabels::Hidden;
    });
    let leaves = leaves(&scene);
    let mut labels = degree_labels(&leaves);
    labels.sort_by_key(|t| parse_number(t.trim_end_matches('°')).map(|v| v as i64));
    assert_eq!(
        labels,
        ["0°", "45°", "90°", "135°", "180°", "225°", "270°", "315°"],
        "drawn texts: {:?}",
        texts(&leaves)
    );
}

// WHY: angles in radians are read as fractions of π; a label such as "0.785" would force the reader to do the
// division, and "6.28" over "0" is the radian form of the duplicated full-turn label.
//
// Assumption: the labels are plain text runs with a Unicode π (as the compiler writes the Unicode minus sign in plain
// numeric labels), and a multiple of one is written without the "1", so π/4 rather than 1π/4.
#[test]
fn a_full_turn_in_radians_is_labelled_in_multiples_of_pi() {
    let (scene, _) = polar_scene(view(AngleUnits::Radians), |a| {
        a.y.tick_labels = TickLabels::Hidden;
    });
    let leaves = leaves(&scene);
    let drawn = texts(&leaves);
    let expected = ["0", "π/4", "π/2", "3π/4", "π", "5π/4", "3π/2", "7π/4"];
    for label in expected {
        assert_eq!(
            drawn.iter().filter(|t| t.as_str() == label).count(),
            1,
            "the label {label:?} is drawn once; drawn texts: {drawn:?}"
        );
    }
    assert!(
        !drawn.iter().any(|t| t == "2π"),
        "no 2π label is drawn on top of 0: {drawn:?}"
    );
    assert!(
        !drawn
            .iter()
            .any(|t| parse_number(t).is_some_and(|v| v > 0.0)),
        "no decimal angle such as 0.785 is drawn: {drawn:?}"
    );
}

// WHY: categorical spokes (a radar chart's criteria) are explicit tick values with text labels; each name must sit at
// its own spoke, a fifth of a turn apart, measured clockwise from north as the view asks, or the chart would mislabel
// its axes.
#[test]
fn per_turn_categories_with_text_labels_sit_at_their_own_spokes() {
    let names = ["mass", "cost", "range", "speed", "noise"];
    let polar = PolarView {
        angle_units: AngleUnits::PerTurn(5.0),
        zero_deg: 90.0,
        ..PolarView::default()
    };
    let (scene, ax) = polar_scene(polar, |a| {
        a.x.reverse = true;
        a.x.ticks = Ticks::Values(vec![0.0, 1.0, 2.0, 3.0, 4.0]);
        a.x.tick_labels = TickLabels::Text(names.iter().map(|n| Text::plain(*n)).collect());
        a.y.tick_labels = TickLabels::Hidden;
    });
    let g = polar_geometry(&scene, ax);
    let leaves = leaves(&scene);
    for (k, name) in names.iter().enumerate() {
        let phi = deg(90.0 - 72.0 * k as f64);
        assert_label_on_ray(&g, label_box(&leaves, name), phi, name);
    }
}

// WHY: eight divisions are a convention of a whole turn in degrees or radians only; a partial sector or a turn of other
// units (24 hours) gets the round ticks of any linear axis, and a tick at 24 hours must not be printed over 0.
#[test]
fn partial_sectors_and_other_units_get_ordinary_linear_ticks() {
    // A quarter sector in degrees.
    let (scene, _) = polar_scene(view(AngleUnits::Degrees), |a| {
        a.x.limits = Limits::Manual {
            min: 0.0,
            max: 90.0,
        };
        a.y.tick_labels = TickLabels::Hidden;
    });
    let values: Vec<f64> = degree_labels(&leaves(&scene))
        .iter()
        .filter_map(|t| parse_number(t.trim_end_matches('°')))
        .collect();
    assert_linear_ticks(&values, 0.0, 90.0, "a quarter sector in degrees");

    // A whole turn of 24 hours.
    let (scene, _) = polar_scene(view(AngleUnits::PerTurn(24.0)), |a| {
        a.y.tick_labels = TickLabels::Hidden;
    });
    let drawn = texts(&leaves(&scene));
    let values: Vec<f64> = drawn.iter().filter_map(|t| parse_number(t)).collect();
    assert_linear_ticks(&values, 0.0, 24.0, "a turn of 24 hours");
    assert!(
        !values.contains(&24.0),
        "the tick at a full turn coincides with 0 and is dropped: {drawn:?}"
    );
    assert_ne!(
        sorted(values),
        vec![0.0, 3.0, 6.0, 9.0, 12.0, 15.0, 18.0, 21.0],
        "eight divisions are kept for degrees and radians"
    );
}

// WHY: only a tick that would be printed over the tick at the lower limit is dropped. The upper limit of a partial
// sector is an edge of its own, which the reader must be able to read: half a turn of twelve hours ends at "6", and
// dropping that label as though it were a full turn would leave the sector's far edge unnamed.
#[test]
fn the_label_at_the_upper_limit_of_a_partial_sector_is_kept() {
    let (scene, _) = polar_scene(view(AngleUnits::PerTurn(12.0)), |a| {
        a.x.limits = Limits::Manual { min: 0.0, max: 6.0 };
        a.y.tick_labels = TickLabels::Hidden;
    });
    let drawn = texts(&leaves(&scene));
    let values: Vec<f64> = drawn.iter().filter_map(|t| parse_number(t)).collect();
    assert_linear_ticks(&values, 0.0, 6.0, "half a turn of twelve hours");
    assert!(
        values.contains(&0.0) && values.contains(&6.0),
        "both edges of the sector are labelled: {drawn:?}"
    );
}

// WHY: manual angular limits of exactly one turn ask for the same full circle as automatic limits, so they must draw
// the same figure: no radial edges, no "360°" over "0°", the same eight labels and the same fit. A sector test that
// compared the span against a turn with a strict inequality, or a tick rule that applied only to automatic limits,
// would make the two differ.
#[test]
fn manual_limits_of_one_turn_draw_exactly_what_automatic_limits_draw() {
    let (automatic, _) = polar_scene(view(AngleUnits::Degrees), |a| a.box_ = true);
    let (manual, _) = polar_scene(view(AngleUnits::Degrees), |a| {
        a.box_ = true;
        a.x.limits = Limits::Manual {
            min: 0.0,
            max: 360.0,
        };
    });
    assert_eq!(manual.display_list, automatic.display_list);
}

fn sorted(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    v
}

/// Asserts that tick values are at least three equally spaced multiples of a 1-2-5 step within `[lo, hi]`.
#[track_caller]
fn assert_linear_ticks(values: &[f64], lo: f64, hi: f64, what: &str) {
    let v = sorted(values.to_vec());
    assert!(v.len() >= 3, "{what}: at least three ticks: {v:?}");
    assert!(
        v.iter().all(|t| *t >= lo - 1e-9 && *t <= hi + 1e-9),
        "{what}: ticks within the limits: {v:?}"
    );
    let step = v[1] - v[0];
    assert!(
        v.windows(2).all(|w| (w[1] - w[0] - step).abs() < 1e-9),
        "{what}: equally spaced: {v:?}"
    );
    let mantissa = step / 10f64.powf(step.log10().floor());
    assert!(
        [1.0, 2.0, 5.0, 10.0]
            .iter()
            .any(|m| (mantissa - m).abs() < 1e-9),
        "{what}: the step {step} is a 1-2-5 step, as on any linear axis"
    );
}

// WHY: a hidden angular axis draws no labels, so that a bare dial or a schematic polar diagram can be drawn.
#[test]
fn a_hidden_angular_axis_draws_no_angular_labels() {
    let (scene, _) = polar_scene(view(AngleUnits::Degrees), |a| {
        a.x.visible = false;
    });
    assert!(
        degree_labels(&leaves(&scene)).is_empty(),
        "drawn texts: {:?}",
        texts(&leaves(&scene))
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Fitting the circle or sector into the plot rectangle
// ---------------------------------------------------------------------------------------------------------------------

// WHY: a polar plot is always drawn round; the circle takes the whole of the shorter side of the plot rectangle and is
// centred in it, so it neither overflows into the margins nor wastes space.
#[test]
fn a_full_circle_has_the_shorter_side_of_the_plot_rectangle_as_its_diameter_and_is_centred() {
    let (scene, ax) = polar_scene(view(AngleUnits::Degrees), |_| {});
    let g = polar_geometry(&scene, ax);
    assert!(
        g.plot.width > g.plot.height + 1.0,
        "the default figure is wider than tall, so the fit is decided by the height: {:?}",
        g.plot
    );
    assert_close(2.0 * g.rim, g.plot.width.min(g.plot.height), 1e-6);
    let c = centre(g.plot);
    assert_close(g.centre.x, c.x, 1e-6);
    assert_close(g.centre.y, c.y, 1e-6);
    assert_close(g.hole, 0.0, 1e-9);
}

// WHY: the angular labels take the same room on every side of a circle, so a polar axes without axis labels or a title
// sits in the middle of its tile; a lopsided circle would look misplaced beside the others in a grid of tiles.
#[test]
fn a_polar_axes_without_labels_or_title_is_centred_in_its_tile() {
    let (scene, ax) = polar_scene(view(AngleUnits::Degrees), |_| {});
    let g = polar_geometry(&scene, ax);
    assert_close(g.centre.x, scene.display_list.width_pt / 2.0, 0.5);
    assert_close(g.centre.y, scene.display_list.height_pt / 2.0, 0.5);
}

// WHY: a quarter sector drawn in the space of a whole circle would leave three quarters of the plot empty; the sector's
// own bounding box (between its edge angles and including the centre) is what is fitted.
#[test]
fn a_quarter_sector_fills_the_plot_rectangle_with_its_bounding_box() {
    let (scene, ax) = polar_scene(view(AngleUnits::Degrees), |a| {
        a.x.limits = Limits::Manual {
            min: 0.0,
            max: 90.0,
        };
    });
    let g = polar_geometry(&scene, ax);
    // The quarter from east to north is the square [cx, cx + rim] × [cy − rim, cy] on the page.
    assert_close(g.rim, g.plot.width.min(g.plot.height), 1e-6);
    let c = centre(g.plot);
    assert_close(g.centre.x + g.rim / 2.0, c.x, 1e-6);
    assert_close(g.centre.y - g.rim / 2.0, c.y, 1e-6);
}

// WHY: a radar or sonar sweep (−60° to 60° about north) is wider than tall; its bounding box runs from the centre up to
// the rim at north and across between the two edge spokes, and that box is what is scaled to fit.
#[test]
fn a_sweep_about_north_fits_its_wide_bounding_box() {
    let polar = PolarView {
        zero_deg: 90.0,
        ..PolarView::default()
    };
    let (scene, ax) = polar_scene(polar, |a| {
        a.x.limits = Limits::Manual {
            min: -60.0,
            max: 60.0,
        };
    });
    let g = polar_geometry(&scene, ax);
    // Page angles 30° to 150°: at unit radius the box is [−sin 60°, sin 60°] × [0, 1] (up from the centre).
    let half_width = deg(60.0).sin();
    let expected_rim = (g.plot.width / (2.0 * half_width)).min(g.plot.height);
    assert_close(g.rim, expected_rim, 1e-6);
    let c = centre(g.plot);
    assert_close(g.centre.x, c.x, 1e-6);
    assert_close(g.centre.y - g.rim / 2.0, c.y, 1e-6);
}

// WHY: the hole is a fraction of the rim radius, so the inner edge of an annulus scales with the plot.
#[test]
fn the_hole_radius_is_the_hole_fraction_of_the_rim() {
    let polar = PolarView {
        hole: 0.3,
        ..PolarView::default()
    };
    let (scene, ax) = polar_scene(polar, |_| {});
    let g = polar_geometry(&scene, ax);
    assert_close(g.hole, 0.3 * g.rim, 1e-9);
}

// WHY: the angular labels lie outside the rim, so the margins must reserve room for them on every side; a label cut
// off at the page edge is unreadable.
#[test]
fn angular_labels_stay_on_the_page() {
    let (scene, _) = polar_scene(view(AngleUnits::Degrees), |_| {});
    let page = Rect::new(
        0.0,
        0.0,
        scene.display_list.width_pt,
        scene.display_list.height_pt,
    );
    let leaves = leaves(&scene);
    for label in degree_labels(&leaves) {
        let b = label_box(&leaves, &label);
        assert!(
            b.x >= page.x
                && b.y >= page.y
                && b.right() <= page.right()
                && b.bottom() <= page.bottom(),
            "the label {label:?} at {b:?} lies on the page {page:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The sector clip
// ---------------------------------------------------------------------------------------------------------------------

// WHY: artists are clipped to the polar region, not to the plot rectangle, so a line beyond the rim does not spill into
// the corners of the plot; the clip must be exactly the region the decorations outline.
#[test]
fn the_artists_of_a_polar_axes_are_clipped_to_one_sector_of_its_plot_region() {
    let polar = PolarView {
        hole: 0.25,
        ..PolarView::default()
    };
    let (scene, ax) = polar_scene(polar, |_| {});
    let g = polar_geometry(&scene, ax);
    let clips = sector_clips(&scene);
    assert_eq!(clips.len(), 1, "one sector clip per polar axes: {clips:?}");
    let s = clips[0];
    assert_close(s.centre.x, g.centre.x, 1e-9);
    assert_close(s.centre.y, g.centre.y, 1e-9);
    assert_close(s.outer, g.rim, 1e-9);
    assert_close(s.inner, g.hole, 1e-9);
    assert_close(s.sweep, TAU, 1e-12);
}

// WHY: a clockwise quarter from north (0° at north, 90° at east) occupies the page quadrant from east to north, and
// the clip's start and sweep, which are page angles anticlockwise from east, must describe that quadrant whatever the
// direction of the angular axis.
#[test]
fn the_sector_clip_of_a_clockwise_quarter_from_north_covers_the_north_east_quadrant() {
    let polar = PolarView {
        zero_deg: 90.0,
        ..PolarView::default()
    };
    let (scene, _) = polar_scene(polar, |a| {
        a.x.reverse = true;
        a.x.limits = Limits::Manual {
            min: 0.0,
            max: 90.0,
        };
    });
    let clips = sector_clips(&scene);
    assert_eq!(clips.len(), 1, "{clips:?}");
    assert_angle_close(clips[0].start, 0.0, 1e-9, "start");
    assert_close(clips[0].sweep, FRAC_PI_2, 1e-9);
    assert_close(clips[0].inner, 0.0, 1e-9);
}

// WHY: Cartesian axes keep their rectangular clips and must not gain a sector clip, so that their display lists, and
// so every existing picture, are unchanged.
#[test]
fn a_cartesian_axes_has_no_sector_clip() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let scene = compile_figure(&fx.build());
    assert!(sector_clips(&scene).is_empty());
}

// ---------------------------------------------------------------------------------------------------------------------
// Frame and grid
// ---------------------------------------------------------------------------------------------------------------------

// WHY: the box of a polar axes is its rim (and the edge of its hole), drawn with the stroke of a Cartesian box so the
// two kinds of axes look alike in one figure; a figure drawn without a box has none.
#[test]
fn the_frame_is_the_rim_and_the_hole_edge_when_box_is_on_and_absent_when_off() {
    let (frame, _) = cartesian_strokes();
    let polar = PolarView {
        hole: 0.3,
        ..PolarView::default()
    };
    let (scene, ax) = polar_scene(polar, |a| a.box_ = true);
    let g = polar_geometry(&scene, ax);
    let arcs: Vec<f64> = subpaths_with_stroke(&scene, ax, &frame)
        .iter()
        .filter_map(|s| match classify(&g, s) {
            Shape::Arc(r) => Some(r),
            _ => None,
        })
        .collect();
    assert!(
        arcs.iter().any(|r| (r - g.rim).abs() < 0.05),
        "the rim is drawn with the box stroke: arcs at {arcs:?}, rim {}",
        g.rim
    );
    assert!(
        arcs.iter().any(|r| (r - g.hole).abs() < 0.05),
        "the edge of the hole is drawn with the box stroke: arcs at {arcs:?}, hole {}",
        g.hole
    );
    assert!(
        !segments_with_stroke(&scene, ax, &frame)
            .iter()
            .any(|&(a, b)| (g.radius_of(a) - g.radius_of(b)).abs() > 1.0),
        "a full circle has no radial edges"
    );

    let (scene, ax) = polar_scene(polar, |a| a.box_ = false);
    let g = polar_geometry(&scene, ax);
    let on_rim = subpaths_with_stroke(&scene, ax, &frame)
        .iter()
        .any(|s| s.iter().all(|&p| (g.radius_of(p) - g.rim).abs() < 0.05));
    assert!(!on_rim, "no rim is drawn without the box");
}

// WHY: a partial sector is closed by its two edge spokes, which are part of the frame, from the hole (or the centre) to
// the rim; without them a sector would look like an arc floating over the data.
#[test]
fn a_partial_sector_frame_has_its_two_radial_edges() {
    let (frame, _) = cartesian_strokes();
    let polar = PolarView {
        hole: 0.2,
        ..PolarView::default()
    };
    let (scene, ax) = polar_scene(polar, |a| {
        a.box_ = true;
        a.x.limits = Limits::Manual {
            min: 0.0,
            max: 90.0,
        };
    });
    let g = polar_geometry(&scene, ax);
    let segments = segments_with_stroke(&scene, ax, &frame);
    for phi in [0.0, FRAC_PI_2] {
        let (inner, outer) = (g.place(phi, g.hole), g.place(phi, g.rim));
        let found = segments.iter().any(|&(a, b)| {
            let close = |p: Point, q: Point| (p.x - q.x).hypot(p.y - q.y) < 0.05;
            (close(a, inner) && close(b, outer)) || (close(a, outer) && close(b, inner))
        });
        assert!(
            found,
            "the edge at page angle {phi} runs from the hole to the rim with the box stroke"
        );
    }
}

/// The grid of a polar axes whose radial ticks are at 0.25, 0.5 and 0.75 of the limits 0 to 1 (strictly inside them,
/// so that no arc coincides with the rim or the hole) and whose angular ticks are at the quarter turns.
fn gridded(x_grid: bool, y_grid: bool, hole: f64) -> (Scene, NodeId) {
    let polar = PolarView {
        hole,
        ..PolarView::default()
    };
    polar_scene(polar, |a| {
        a.x.grid = x_grid;
        a.y.grid = y_grid;
        a.x.ticks = Ticks::Values(vec![0.0, 90.0, 180.0, 270.0]);
        a.y.ticks = Ticks::Values(vec![0.25, 0.5, 0.75]);
    })
}

/// Returns the spokes and arcs of the grid of a polar axes.
fn grid_shapes(scene: &Scene, ax: NodeId) -> (Vec<(f64, f64, f64)>, Vec<f64>) {
    let (_, grid) = cartesian_strokes();
    let g = polar_geometry(scene, ax);
    let mut spokes = Vec::new();
    let mut arcs = Vec::new();
    for s in subpaths_with_stroke(scene, ax, &grid) {
        match classify(&g, &s) {
            Shape::Spoke(phi, a, b) => spokes.push((phi, a, b)),
            Shape::Arc(r) => arcs.push(r),
            Shape::Other => panic!("a grid subpath is neither a spoke nor an arc: {s:?}"),
        }
    }
    (spokes, arcs)
}

// WHY: the angular grid is spokes at the angular ticks from the edge of the hole to the rim, and the radial grid is
// circles at the radial ticks; each is drawn only when its own axis asks for a grid, with the stroke of the Cartesian
// grid, so that switching a grid on or off in the inspector has the same meaning in both kinds of axes.
#[test]
fn spokes_follow_the_angular_grid_and_arcs_follow_the_radial_grid() {
    let hole = 0.2;
    let (scene, ax) = gridded(true, true, hole);
    let g = polar_geometry(&scene, ax);
    let (spokes, arcs) = grid_shapes(&scene, ax);

    let mut angles: Vec<f64> = spokes.iter().map(|s| s.0.rem_euclid(TAU)).collect();
    angles.sort_by(f64::total_cmp);
    assert_eq!(angles.len(), 4, "one spoke per angular tick: {spokes:?}");
    for (a, e) in angles.iter().zip([0.0, FRAC_PI_2, PI, 1.5 * PI]) {
        assert_angle_close(*a, e, 1e-9, "spoke angle");
    }
    for &(_, inner, outer) in &spokes {
        assert_close(inner, g.hole, 0.05);
        assert_close(outer, g.rim, 0.05);
    }

    let mut radii = arcs.clone();
    radii.sort_by(f64::total_cmp);
    assert_eq!(radii.len(), 3, "one arc per radial tick: {arcs:?}");
    for (r, v) in radii.iter().zip([0.25, 0.5, 0.75]) {
        assert_close(*r, g.hole + v * (g.rim - g.hole), 0.05);
    }

    let (scene, ax) = gridded(false, true, hole);
    let (spokes, arcs) = grid_shapes(&scene, ax);
    assert!(
        spokes.is_empty(),
        "no spokes without the angular grid: {spokes:?}"
    );
    assert_eq!(arcs.len(), 3);

    let (scene, ax) = gridded(true, false, hole);
    let (spokes, arcs) = grid_shapes(&scene, ax);
    assert_eq!(spokes.len(), 4);
    assert!(arcs.is_empty(), "no arcs without the radial grid: {arcs:?}");
}

// WHY: the edges of a partial sector are drawn by the frame, so the grid draws spokes only at the interior angular
// ticks; a spoke on top of an edge would double its weight.
#[test]
fn a_partial_sector_draws_spokes_only_at_interior_ticks() {
    let (scene, ax) = polar_scene(view(AngleUnits::Degrees), |a| {
        a.x.grid = true;
        a.x.limits = Limits::Manual {
            min: 0.0,
            max: 90.0,
        };
        a.x.ticks = Ticks::Values(vec![0.0, 30.0, 60.0, 90.0]);
    });
    let (spokes, _) = grid_shapes(&scene, ax);
    let mut angles: Vec<f64> = spokes.iter().map(|s| s.0).collect();
    angles.sort_by(f64::total_cmp);
    assert_eq!(angles.len(), 2, "{spokes:?}");
    assert_angle_close(angles[0], deg(30.0), 1e-9, "first interior spoke");
    assert_angle_close(angles[1], deg(60.0), 1e-9, "second interior spoke");
}

// ---------------------------------------------------------------------------------------------------------------------
// Placement of the tick labels
// ---------------------------------------------------------------------------------------------------------------------

// WHY: each angular label names the spoke it sits on, so it must be outside the rim on the ray of its tick: east for 0°
// and north for 90° by default, and north for 0° and east for 90° on a compass (zero at north, clockwise). A label on
// the wrong side would mislabel every direction in the plot.
#[test]
fn angular_labels_sit_outside_the_rim_on_the_ray_of_their_tick() {
    let cases = [
        ("anticlockwise from east", 0.0, false),
        ("clockwise from north", 90.0, true),
    ];
    for (what, zero_deg, clockwise) in cases {
        let polar = PolarView {
            zero_deg,
            ..PolarView::default()
        };
        let (scene, ax) = polar_scene(polar, |a| {
            a.x.reverse = clockwise;
            a.y.tick_labels = TickLabels::Hidden;
        });
        let g = polar_geometry(&scene, ax);
        let leaves = leaves(&scene);
        for k in 0..8 {
            let theta = 45.0 * f64::from(k);
            let sign = if clockwise { -1.0 } else { 1.0 };
            let phi = deg(zero_deg + sign * theta);
            let label = format!("{theta}°");
            assert_label_on_ray(
                &g,
                label_box(&leaves, &label),
                phi,
                &format!("{what}: {label}"),
            );
        }
    }
}

/// Returns the numeric labels drawn, as (value, box), sorted by value.
fn numeric_runs(leaves: &[Leaf]) -> Vec<(f64, Rect)> {
    let mut out: Vec<(f64, Rect)> = glyph_runs(leaves)
        .into_iter()
        .filter_map(|(l, g)| Some((parse_number(&g.text)?, l.bbox()?)))
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Asserts that radial labels lie along the spoke at page angle `phi`, each at the radius of its tick and beside the
/// spoke on its clockwise side.
#[track_caller]
fn assert_radial_labels_along(g: &PolarGeometry, labels: &[(f64, Rect)], phi: f64, what: &str) {
    let (ux, uy) = (phi.cos(), -phi.sin());
    // The clockwise normal of the spoke on the page (y grows downwards).
    let (nx, ny) = (phi.sin(), phi.cos());
    for &(value, b) in labels {
        let c = centre(b);
        let (dx, dy) = (c.x - g.centre.x, c.y - g.centre.y);
        let along = dx * ux + dy * uy;
        let side = dx * nx + dy * ny;
        let rho = g.y.to_figure(value);
        let half_diagonal = b.width.hypot(b.height) / 2.0;
        assert!(
            (along - rho).abs() <= half_diagonal + 4.0,
            "{what}: the label of {value} lies at the radius of its tick ({along} vs {rho})"
        );
        assert!(
            side >= -1.0 && side <= half_diagonal + 8.0,
            "{what}: the label of {value} lies just clockwise of the spoke ({side} pt across it)"
        );
    }
}

// WHY: radial labels read the radius off the spoke they sit beside; the author picks that spoke so that the labels do
// not cover the data, and without a choice they sit between the first two angular ticks, clear of both spokes.
#[test]
fn radial_labels_lie_along_the_chosen_spoke_or_between_the_first_two_angular_ticks() {
    let ticks = |a: &mut ironlab_ir::Axes| {
        a.y.ticks = Ticks::Values(vec![0.5, 1.0]);
        a.x.tick_labels = TickLabels::Hidden;
    };

    let chosen = PolarView {
        radial_axis_angle: Some(90.0),
        ..PolarView::default()
    };
    let (scene, ax) = polar_scene(chosen, ticks);
    let g = polar_geometry(&scene, ax);
    let labels = numeric_runs(&leaves(&scene));
    assert_eq!(
        labels.iter().map(|l| l.0).collect::<Vec<_>>(),
        [0.5, 1.0],
        "one label per radial tick"
    );
    assert_radial_labels_along(&g, &labels, FRAC_PI_2, "at the 90° spoke");

    let (scene, ax) = polar_scene(PolarView::default(), ticks);
    let g = polar_geometry(&scene, ax);
    let labels = numeric_runs(&leaves(&scene));
    assert_radial_labels_along(&g, &labels, deg(22.5), "by default, at 22.5°");
}

// WHY: a reversed radius puts the upper limit at the centre, so its label must move there too; labels that stayed put
// would misread every radius in the plot.
#[test]
fn radial_labels_follow_a_reversed_radius() {
    let (scene, ax) = polar_scene(PolarView::default(), |a| {
        a.y.reverse = true;
        a.y.ticks = Ticks::Values(vec![0.25, 0.75]);
        a.x.tick_labels = TickLabels::Hidden;
    });
    let g = polar_geometry(&scene, ax);
    let labels = numeric_runs(&leaves(&scene));
    assert_eq!(labels.len(), 2, "{labels:?}");
    let distance = |b: Rect| g.radius_of(centre(b));
    assert!(
        distance(labels[1].1) < distance(labels[0].1),
        "0.75 is nearer the centre than 0.25 when the radius is reversed: {labels:?}"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Hit geometry
// ---------------------------------------------------------------------------------------------------------------------

/// The polar views under which the inverse map is checked: the default, a compass with a hole, and a reversed radius.
fn hit_cases() -> Vec<(&'static str, PolarView, bool, bool)> {
    vec![
        ("default", PolarView::default(), false, false),
        (
            "clockwise from north with a hole",
            PolarView {
                zero_deg: 90.0,
                hole: 0.3,
                ..PolarView::default()
            },
            true,
            false,
        ),
        (
            "reversed radius from west",
            PolarView {
                zero_deg: 180.0,
                ..PolarView::default()
            },
            false,
            true,
        ),
    ]
}

// WHY: the published maps are what front ends use to place tips and crosshairs: the angle map gives the page angle of
// a data angle (zero_deg for 0, a quarter turn further in the direction of the axis for 90°) and the radius map runs
// from the hole edge to the rim (or back for a reversed radius); a front end that read them differently from the
// compiler would report the wrong data under the pointer.
#[test]
fn the_published_maps_encode_zero_direction_hole_and_reversal() {
    for (what, polar, clockwise, reversed) in hit_cases() {
        let (scene, ax) = polar_scene(polar, |a| {
            a.x.reverse = clockwise;
            a.y.reverse = reversed;
        });
        let g = polar_geometry(&scene, ax);
        let sign = if clockwise { -1.0 } else { 1.0 };
        assert_angle_close(g.x.to_figure(0.0), deg(polar.zero_deg), 1e-9, what);
        assert_close(
            g.x.to_figure(90.0) - g.x.to_figure(0.0),
            sign * FRAC_PI_2,
            1e-9,
        );
        let (at_min, at_max) = if reversed {
            (g.rim, g.hole)
        } else {
            (g.hole, g.rim)
        };
        assert_close(g.y.to_figure(0.0), at_min, 1e-9);
        assert_close(g.y.to_figure(1.0), at_max, 1e-9);
        assert_close(g.hole, polar.hole * g.rim, 1e-9);
    }
}

// WHY: a front end converts a pointer to (θ, r) and back to place a crosshair; the two conversions must be inverse,
// agree with the published centre and maps, and report the angle within the limits (300°, not −60°).
#[test]
fn to_data_inverts_to_figure_and_agrees_with_the_published_geometry() {
    for (what, polar, clockwise, reversed) in hit_cases() {
        let (scene, ax) = polar_scene(polar, |a| {
            a.x.reverse = clockwise;
            a.y.reverse = reversed;
        });
        let g = polar_geometry(&scene, ax);
        let kind = &axes_hit(&scene, ax).kind;
        for theta in [0.0, 30.0, 135.0, 300.0] {
            for r in [0.1, 0.5, 0.9] {
                let p = kind
                    .to_figure([theta, r])
                    .unwrap_or_else(|| panic!("{what}: ({theta}, {r}) has a place"));
                let expected = g.place(g.x.to_figure(theta), g.y.to_figure(r));
                assert_close(p.x, expected.x, 1e-9);
                assert_close(p.y, expected.y, 1e-9);
                let back = kind
                    .to_data(p)
                    .unwrap_or_else(|| panic!("{what}: {p:?} lies in the polar region"));
                assert_close(back[0], theta, 1e-9);
                assert_close(back[1], r, 1e-9);
            }
        }
    }
}

// WHY: a pointer outside the polar region (beyond the rim, in the hole, or outside a partial sector) points at no data,
// so a tip or crosshair must not report a radius or an angle for it.
//
// Assumption: `to_data` returns `None` for such points, which is what its `Option` result is for.
#[test]
fn to_data_returns_none_outside_the_polar_region() {
    let polar = PolarView {
        hole: 0.3,
        ..PolarView::default()
    };
    let (scene, ax) = polar_scene(polar, |a| {
        a.x.limits = Limits::Manual {
            min: 0.0,
            max: 90.0,
        };
    });
    let g = polar_geometry(&scene, ax);
    let kind = &axes_hit(&scene, ax).kind;
    let inside = g.place(deg(45.0), 0.5 * (g.hole + g.rim));
    assert!(
        kind.to_data(inside).is_some(),
        "the middle of the sector is inside"
    );
    for (what, p) in [
        ("beyond the rim", g.place(deg(45.0), g.rim + 5.0)),
        ("in the hole", g.place(deg(45.0), 0.5 * g.hole)),
        (
            "outside the sector",
            g.place(deg(200.0), 0.5 * (g.hole + g.rim)),
        ),
    ] {
        assert_eq!(kind.to_data(p), None, "{what}: {p:?}");
    }
}

// WHY: the corners of the plot rectangle outside the circle belong to no data, so a wheel or a drag there must not
// zoom or pan the polar axes.
#[test]
fn axes_at_finds_a_polar_axes_only_inside_its_circle() {
    let (scene, ax) = polar_scene(PolarView::default(), |_| {});
    let g = polar_geometry(&scene, ax);
    let inside = g.place(deg(10.0), 0.5 * g.rim);
    assert_eq!(scene.hit_map.axes_at(inside).map(|a| a.id), Some(ax));
    // The plot rectangle is wider than the circle, so a point near its left edge, level with the centre, is in the
    // rectangle but outside the circle.
    let outside = Point::new(g.plot.x + 1.0, g.centre.y);
    assert!(
        g.radius_of(outside) > g.rim,
        "the probe lies outside the circle"
    );
    assert!(
        scene.hit_map.axes_at(outside).is_none(),
        "a point in the plot rectangle outside the circle is in no axes"
    );
}

// WHY: the hit geometry kind is how a front end decides which interaction applies; a Cartesian axes must still publish
// its two linear maps.
#[test]
fn a_cartesian_axes_still_publishes_two_dimensional_maps() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    fx.ax(ax).title = text("Cartesian");
    let scene = compile_figure(&fx.build());
    assert!(matches!(
        axes_hit(&scene, ax).kind,
        AxesHitKind::TwoD { .. }
    ));
}

// WHY: a polar axes paints its grid under its artists, as a Cartesian axes does, so that the light spokes and circles
// never cross a bar, a filled outline or a line, and the two kinds of axes layer their grids the same way.
#[test]
fn the_grid_is_painted_under_the_artists() {
    let (scene, ax) = gridded(true, true, 0.0);
    let (_, grid) = cartesian_strokes();
    let all = leaves(&scene);
    let first_artist = all
        .iter()
        .position(|l| l.source.is_some() && l.source != Some(ax) && l.path().is_some())
        .expect("the series is drawn");
    let last_grid = all
        .iter()
        .rposition(|l| {
            l.source == Some(ax) && l.path().is_some_and(|p| p.stroke.as_ref() == Some(&grid))
        })
        .expect("the grid is drawn");
    assert!(
        last_grid < first_artist,
        "the grid (leaf {last_grid}) is painted before the first artist leaf ({first_artist})"
    );
}

// Why: polar axes draw the angle round the rim and the radius along a spoke, so neither axis has a side of the plot to
// move to; validation warns that the side is ignored, and the drawing must be exactly the drawing of the default side,
// so that a figure switched to polar axes from Cartesian axes with an axis on the right draws as any polar plot.
#[test]
fn a_side_on_polar_axes_changes_nothing_drawn() {
    let (plain, _) = polar_scene(PolarView::default(), |_| {});
    let (sided, _) = polar_scene(PolarView::default(), |axes| {
        axes.x.side = ironlab_ir::AxisSide::High;
        axes.y.side = ironlab_ir::AxisSide::Centre;
    });
    assert_eq!(sided.display_list, plain.display_list);
}
