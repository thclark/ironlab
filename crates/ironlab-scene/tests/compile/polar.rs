//! Artists in polar axes: where data points land on the page, and how the two kinds of artist are drawn there.
//!
//! In polar axes x is the angle and y the radius. Artists that draw data directly (series, contour, surface, quiver)
//! map each data point and join consecutive points with straight chords on the page; artists that draw shapes
//! representing data (bars, stairs, cells, image pixels) build their outlines from constant-coordinate edges, which
//! follow the axes, so a bar top is an arc and a cell is an annular sector.
//!
//! Every expectation here is computed from the definition of polar axes: the centre and the rim of the plot region,
//! the zero direction, the direction of increasing angle, the hole and the radial limits. Where a full circle is
//! drawn, the centre and the rim are taken from the plot rectangle (the circle is centred in it and its diameter is
//! the shorter side); for a partial sector they are taken from the centre and the radial axis map that the hit map
//! publishes. Nothing is read back from the compiler's own mapping code.

use std::f64::consts::{PI, TAU};

use ironlab_ir::{
    AngleUnits, BarWidth, CellEdges, CellGap, Color, ColorScaleKind, IndexedColorScale, Limits,
    NodeId, Orientation, PerBar, PolarView, Projection, Scale, View2d,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{Item, ItemKind, Point, Rect, Sector};
use ironlab_scene::hit::{AxesHitKind, AxisMap, HitArea};

use crate::common::{Fx, cell, compile_figure, range, rgb8, rgba, text, xy};
use crate::probe::{
    Leaf, assert_close, axes_hit, axis_maps, centre, from_source, inside, leaves, points_close,
};

/// Agreement expected of a point placed on the page from data, in points.
const TOL: f64 = 1e-6;

/// The largest distance, in points, that a followed outline may stray from the true arc between two of its vertices:
/// the sagitta of each chord.
const CHORD: f64 = 0.05;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

// ---------------------------------------------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------------------------------------------

/// The geometry of a polar plot region on the page: its centre, the radius of its rim and the radius of the edge of
/// its hole (0 without one).
#[derive(Clone, Copy, Debug)]
struct Frame {
    centre: Point,
    rim: f64,
    inner: f64,
}

impl Frame {
    /// The page point at `deg` degrees anticlockwise from east and `rho` points from the centre. Figure space grows
    /// downwards, so a point to the north has a smaller y than the centre.
    fn at(&self, deg: f64, rho: f64) -> Point {
        let phi = deg.to_radians();
        Point::new(
            self.centre.x + rho * phi.cos(),
            self.centre.y - rho * phi.sin(),
        )
    }

    /// The page angle in degrees anticlockwise from east, in `[0, 360)`, and the distance from the centre of `p`.
    fn polar_of(&self, p: Point) -> (f64, f64) {
        let (dx, dy) = (p.x - self.centre.x, self.centre.y - p.y);
        (dy.atan2(dx).to_degrees().rem_euclid(360.0), dx.hypot(dy))
    }

    /// The page radius of a value of a linear radial axis whose limits `lo` and `hi` lie at the hole and the rim.
    fn radius(&self, lo: f64, hi: f64, r: f64) -> f64 {
        self.inner + (r - lo) / (hi - lo) * (self.rim - self.inner)
    }
}

/// Adds polar axes with the given view in the one tile of the figure.
fn polar_axes(fx: &mut Fx, polar: PolarView) -> NodeId {
    fx.axes_in(
        cell(0, 0, 1, 1),
        Projection::Polar {
            polar,
            view: ironlab_ir::View2d::default(),
        },
    )
}

/// Sets manual limits on the angular (x) and radial (y) axes.
fn limits(fx: &mut Fx, ax: NodeId, theta: Option<(f64, f64)>, r: Option<(f64, f64)>) {
    let axes = fx.ax(ax);
    if let Some((min, max)) = theta {
        axes.x.limits = Limits::Manual { min, max };
    }
    if let Some((min, max)) = r {
        axes.y.limits = Limits::Manual { min, max };
    }
}

/// Returns the centre and the two axis maps that the hit map publishes for polar axes.
#[track_caller]
fn polar_hit(scene: &Scene, ax: NodeId) -> (Point, AxisMap, AxisMap) {
    match axes_hit(scene, ax).kind {
        AxesHitKind::Polar { centre, x, y } => (centre, x, y),
        ref other => panic!("axes {ax} is polar in the hit map, found {other:?}"),
    }
}

/// The frame of a full circle, from the plot rectangle: the circle is centred in it and fills its shorter side. The
/// hit map must agree, because a front end maps the pointer through it.
#[track_caller]
fn full_circle(scene: &Scene, ax: NodeId, hole: f64) -> Frame {
    let plot = axes_hit(scene, ax).plot_rect;
    let frame = Frame {
        centre: centre(plot),
        rim: plot.width.min(plot.height) / 2.0,
        inner: hole * plot.width.min(plot.height) / 2.0,
    };
    let (published, _, y) = polar_hit(scene, ax);
    assert!(
        points_close(published, frame.centre, TOL),
        "the hit map publishes the centre of the circle: {published:?} against {:?}",
        frame.centre
    );
    assert_close(y.start.max(y.end), frame.rim, TOL);
    assert_close(y.start.min(y.end), frame.inner, TOL);
    frame
}

/// The frame of any polar plot region, from the centre and the radial axis map that the hit map publishes.
fn published_frame(scene: &Scene, ax: NodeId) -> Frame {
    let (centre, _, y) = polar_hit(scene, ax);
    Frame {
        centre,
        rim: y.start.max(y.end),
        inner: y.start.min(y.end),
    }
}

#[track_caller]
fn assert_at(actual: Point, expected: Point, what: &str) {
    assert!(
        points_close(actual, expected, TOL),
        "{what}: drawn at {actual:?}, expected {expected:?}"
    );
}

/// Returns the vertices of the one marker instance of each point a scatter drew, in paint order.
fn marker_positions(scene: &Scene, id: NodeId) -> Vec<Point> {
    from_source(&leaves(scene), id)
        .iter()
        .flat_map(Leaf::instances)
        .map(|m| m.position)
        .collect()
}

/// Returns the closed outlines (one per subpath) that `id` filled, in figure space, without the repeated closing
/// vertex. Outlines that are only stroked over the same polygon are not repeated.
fn filled_outlines(scene: &Scene, id: NodeId) -> Vec<Vec<Point>> {
    from_source(&leaves(scene), id)
        .iter()
        .filter(|l| l.path().is_some_and(|p| p.fill.is_some()))
        .flat_map(Leaf::subpaths)
        .collect()
}

/// Returns the outlines that `id` filled with `color`.
fn filled_in(scene: &Scene, id: NodeId, color: Color) -> Vec<Vec<Point>> {
    from_source(&leaves(scene), id)
        .iter()
        .filter(|l| {
            l.path()
                .and_then(|p| p.fill)
                .is_some_and(|f| rgb8(f.color) == rgb8(rgba(color)))
        })
        .flat_map(Leaf::subpaths)
        .collect()
}

/// Returns the subpaths that `id` stroked, in figure space.
fn stroked(scene: &Scene, id: NodeId) -> Vec<Vec<Point>> {
    from_source(&leaves(scene), id)
        .iter()
        .filter(|l| l.path().is_some_and(|p| p.stroke.is_some()))
        .flat_map(Leaf::subpaths)
        .collect()
}

/// An annular sector on the page: the page angles (degrees anticlockwise from east, `a0 < a1`) and the radii
/// (points, `r0 ≤ r1`) between which it lies.
#[derive(Clone, Copy, Debug)]
struct Annulus {
    a0: f64,
    a1: f64,
    r0: f64,
    r1: f64,
}

impl Annulus {
    /// The angular distance from `a` to `deg`, measured anticlockwise, in `[0, 360)`.
    fn after(a: f64, deg: f64) -> f64 {
        (deg - a).rem_euclid(360.0)
    }

    fn within_angles(&self, deg: f64, slack: f64) -> bool {
        Self::after(self.a0 - slack, deg) <= self.a1 - self.a0 + 2.0 * slack
    }

    /// Returns which part of the boundary of the sector a vertex lies on, or `None` when it lies on none of them.
    /// A vertex at the centre lies on both radial edges of a sector whose inner radius is 0.
    fn edge_of(&self, frame: &Frame, p: Point) -> Option<&'static str> {
        let (deg, rho) = frame.polar_of(p);
        let tol_deg = 1e-6;
        if rho < TOL && self.r0 < TOL {
            return Some("centre");
        }
        let radial = |a: f64| {
            let q = frame.at(a, rho);
            points_close(p, q, TOL) && rho >= self.r0 - TOL && rho <= self.r1 + TOL
        };
        if (rho - self.r1).abs() < TOL && self.within_angles(deg, tol_deg) {
            Some("outer arc")
        } else if (rho - self.r0).abs() < TOL && self.within_angles(deg, tol_deg) {
            Some("inner arc")
        } else if radial(self.a0) {
            Some("first radial edge")
        } else if radial(self.a1) {
            Some("second radial edge")
        } else {
            None
        }
    }
}

/// Asserts that `outline` is the boundary of `sector`: every vertex lies on an arc or a radial edge of it, both arcs
/// reach both radial edges, and every chord along an arc strays from the arc by less than [`CHORD`].
#[track_caller]
fn assert_annular_sector(frame: &Frame, outline: &[Point], sector: Annulus, what: &str) {
    for &p in outline {
        assert!(
            sector.edge_of(frame, p).is_some(),
            "{what}: the vertex {p:?} (at {:?} in degrees and points) lies on no edge of {sector:?}",
            frame.polar_of(p)
        );
    }
    for (radius, name) in [(sector.r1, "outer arc"), (sector.r0, "inner arc")] {
        if radius < TOL {
            continue;
        }
        let mut angles: Vec<f64> = outline
            .iter()
            .map(|&p| frame.polar_of(p))
            .filter(|&(_, rho)| (rho - radius).abs() < TOL)
            .map(|(deg, _)| {
                // A vertex on the first radial edge may be measured a rounding error before it, a whole turn on.
                let a = Annulus::after(sector.a0, deg);
                if 360.0 - a < 1e-6 { a - 360.0 } else { a }
            })
            .collect();
        angles.sort_by(f64::total_cmp);
        let span = sector.a1 - sector.a0;
        assert!(
            angles.first().is_some_and(|&a| a.abs() < 1e-6)
                && angles.last().is_some_and(|&a| (a - span).abs() < 1e-6),
            "{what}: the {name} runs from one radial edge to the other: {angles:?} over {span}°"
        );
        for pair in angles.windows(2) {
            let step = (pair[1] - pair[0]).to_radians();
            let sagitta = radius * (1.0 - (step / 2.0).cos());
            assert!(
                sagitta <= CHORD + 1e-9,
                "{what}: the {name} is followed closely, but a chord of {}° at radius {radius} strays {sagitta} pt",
                pair[1] - pair[0]
            );
        }
    }
}

/// Returns, for every leaf drawn by `id`, the sector clip that most closely encloses it (`None` for a leaf in none).
fn enclosing_sectors(scene: &Scene, id: NodeId) -> Vec<Option<Sector>> {
    fn walk(items: &[Item], id: NodeId, sector: Option<&Sector>, out: &mut Vec<Option<Sector>>) {
        for item in items {
            match &item.kind {
                ItemKind::SectorClip { sector, items } => walk(items, id, Some(sector), out),
                ItemKind::Group { items, .. }
                | ItemKind::Dense { items, .. }
                | ItemKind::Depth { items } => walk(items, id, sector, out),
                _ if item.source == Some(id) => out.push(sector.cloned()),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&scene.display_list.items, id, None, &mut out);
    out
}

/// Asserts that every leaf `id` drew lies inside one sector clip of the plot region `expected`, its start angle
/// compared modulo a turn and ignored for a full turn, which has no edge.
#[track_caller]
fn assert_clipped_to(scene: &Scene, id: NodeId, expected: Sector) {
    let sectors = enclosing_sectors(scene, id);
    assert!(!sectors.is_empty(), "artist {id} draws something");
    for sector in sectors {
        let sector = sector.unwrap_or_else(|| {
            panic!("every leaf of artist {id} lies inside a sector clip of the plot region")
        });
        assert!(
            points_close(sector.centre, expected.centre, TOL),
            "{sector:?} against {expected:?}"
        );
        assert_close(sector.inner, expected.inner, TOL);
        assert_close(sector.outer, expected.outer, TOL);
        assert_close(sector.sweep, expected.sweep, 1e-9);
        if expected.sweep < TAU - 1e-9 {
            let difference = (sector.start - expected.start).rem_euclid(TAU);
            assert!(
                difference < 1e-9 || TAU - difference < 1e-9,
                "the sector starts at {} rad, expected {} rad",
                sector.start,
                expected.start
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Where data points land
// ---------------------------------------------------------------------------------------------------------------

/// Draws markers at `points` (angle, radius) in polar axes with radial limits 0 to 2 and the given view, edited by
/// `edit`, and returns the scene, the axes and the scatter.
fn markers_at(
    polar: PolarView,
    points: &[(f64, f64)],
    edit: impl FnOnce(&mut Fx, NodeId),
) -> (Scene, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, polar);
    let (theta, r): (Vec<f64>, Vec<f64>) = points.iter().copied().unzip();
    let id = fx.scatter(ax, &theta, &r, None, |_, _| {});
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    edit(&mut fx, ax);
    (compile_figure(&fx.build()), ax, id)
}

// Why: the reader reads a polar plot by the direction and the distance of each point from the centre. With the
// default view, angle 0 points east and angles increase anticlockwise, as in mathematics; the radial lower limit is at
// the centre and the upper limit at the rim. Points at the four compass directions and at half and full radius pin
// the map down: a swapped sine, a page y that grows the wrong way, or a radius measured from the wrong limit would
// each move them visibly.
#[test]
fn by_default_angle_zero_points_east_angles_increase_anticlockwise_and_the_radius_runs_from_the_centre()
 {
    let points = [
        (0.0, 1.0),
        (90.0, 2.0),
        (180.0, 1.0),
        (270.0, 0.5),
        (45.0, 2.0),
    ];
    let (scene, ax, id) = markers_at(PolarView::default(), &points, |_, _| {});
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let frame = full_circle(&scene, ax, 0.0);
    let drawn = marker_positions(&scene, id);
    assert_eq!(drawn.len(), points.len());
    for (&(theta, r), &p) in points.iter().zip(&drawn) {
        let want = frame.at(theta, frame.radius(0.0, 2.0, r));
        assert_at(p, want, &format!("the point ({theta}°, {r})"));
    }
    // The datatips report the same places, because the reader points at what is drawn.
    let samples = &scene
        .hit_map
        .artists
        .iter()
        .find(|a| a.artist == id)
        .expect("the series has an entry in the hit map")
        .samples;
    for sample in samples {
        let (theta, r) = points[sample.source_index];
        assert_at(
            sample.position,
            frame.at(theta, frame.radius(0.0, 2.0, r)),
            "the datatip position",
        );
    }
}

// Why: compass plots (wind, antenna patterns, bearings) put angle 0 at the north and measure clockwise, and the
// figure states this through `zero_deg` and `x.reverse` rather than through transformed data. Angle 0 must then point
// up the page and angle 90 to the east; with only `zero_deg` set, 90 points west. Each property is tested alone and
// together, so that one cannot be mistaken for the other.
#[test]
fn zero_deg_turns_angle_zero_and_a_reversed_angular_axis_runs_clockwise() {
    let points = [(0.0, 2.0), (90.0, 2.0), (30.0, 1.0)];
    for (zero_deg, clockwise, page) in [
        (90.0, false, [90.0, 180.0, 120.0]),
        (0.0, true, [0.0, 270.0, 330.0]),
        (90.0, true, [90.0, 0.0, 60.0]),
        (-45.0, false, [315.0, 45.0, 345.0]),
    ] {
        let polar = PolarView {
            zero_deg,
            ..PolarView::default()
        };
        let (scene, ax, id) = markers_at(polar, &points, |fx, ax| fx.ax(ax).x.reverse = clockwise);
        let frame = full_circle(&scene, ax, 0.0);
        for ((&(theta, r), &p), deg) in points.iter().zip(&marker_positions(&scene, id)).zip(page) {
            assert_at(
                p,
                frame.at(deg, frame.radius(0.0, 2.0, r)),
                &format!("({theta}°, {r}) with zero at {zero_deg}° and clockwise {clockwise}"),
            );
        }
    }
}

// Why: the units of the angle are what the data is written in, and a full turn is 360 degrees, 2π radians or n
// categories. A quarter turn in each unit must point the same way, or radian data would wrap 57 times and category
// data would bunch at the zero direction.
#[test]
fn a_quarter_turn_points_north_in_degrees_radians_and_per_turn_units() {
    for (units, quarter) in [
        (AngleUnits::Degrees, 90.0),
        (AngleUnits::Radians, PI / 2.0),
        (AngleUnits::PerTurn(4.0), 1.0),
        (AngleUnits::PerTurn(24.0), 6.0),
    ] {
        let polar = PolarView {
            angle_units: units,
            ..PolarView::default()
        };
        let (scene, ax, id) = markers_at(polar, &[(quarter, 2.0)], |_, _| {});
        let frame = full_circle(&scene, ax, 0.0);
        assert_at(
            marker_positions(&scene, id)[0],
            frame.at(90.0, frame.rim),
            &format!("a quarter turn ({quarter}) in {units:?}"),
        );
    }
}

// Why: a hole moves the radial lower limit off the centre to a circle whose radius is the given fraction of the rim,
// so that data near the lower limit is not crushed into a point. The lower limit must land on that circle, the upper
// limit on the rim, and the values between linearly between them.
#[test]
fn with_a_hole_the_radial_lower_limit_lies_on_the_edge_of_the_hole() {
    let polar = PolarView {
        hole: 0.25,
        ..PolarView::default()
    };
    let points = [(0.0, 0.0), (90.0, 1.0), (180.0, 2.0)];
    let (scene, ax, id) = markers_at(polar, &points, |_, _| {});
    let frame = full_circle(&scene, ax, 0.25);
    let drawn = marker_positions(&scene, id);
    for (k, rho) in [0.25, 0.625, 1.0].into_iter().enumerate() {
        let (theta, r) = points[k];
        assert_at(
            drawn[k],
            frame.at(theta, rho * frame.rim),
            &format!("({theta}°, {r}) with a hole of a quarter"),
        );
    }
}

// Why: a reversed radial axis (MATLAB's `RDir`) puts the upper limit at the centre, as an elevation plot of a sky
// view does (90° at the centre, 0° at the horizon). The limits swap places and the values between follow.
#[test]
fn a_reversed_radial_axis_puts_the_upper_limit_at_the_centre() {
    let points = [(0.0, 2.0), (90.0, 1.5), (180.0, 0.0)];
    let (scene, ax, id) = markers_at(PolarView::default(), &points, |fx, ax| {
        fx.ax(ax).y.reverse = true
    });
    let frame = full_circle(&scene, ax, 0.0);
    let drawn = marker_positions(&scene, id);
    for (k, rho) in [0.0, 0.25, 1.0].into_iter().enumerate() {
        assert_at(
            drawn[k],
            frame.at(points[k].0, rho * frame.rim),
            &format!("{:?} on a reversed radial axis", points[k]),
        );
    }
}

// Why: a logarithmic radius is how an antenna gain or a dynamic range spanning decades is shown. Its lower limit lies
// at the centre (there is no zero on a logarithmic axis to put there) and each decade takes an equal step of radius.
#[test]
fn on_a_logarithmic_radial_axis_each_decade_takes_an_equal_step_from_the_lower_limit_at_the_centre()
{
    let points = [(0.0, 1.0), (90.0, 10.0), (180.0, 100.0), (270.0, 1000.0)];
    let (scene, ax, id) = markers_at(PolarView::default(), &points, |fx, ax| {
        fx.ax(ax).y.scale = Scale::Log;
        limits(fx, ax, None, Some((1.0, 1000.0)));
    });
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let frame = full_circle(&scene, ax, 0.0);
    let drawn = marker_positions(&scene, id);
    for (k, &(theta, r)) in points.iter().enumerate() {
        assert_at(
            drawn[k],
            frame.at(theta, k as f64 / 3.0 * frame.rim),
            &format!("({theta}°, {r}) on a logarithmic radius"),
        );
    }
}

// Why: a hole and a logarithmic radius are often combined (a gain pattern over decades with its low end kept off the
// centre); the lower limit then lies on the edge of the hole and the decades take equal steps from there to the rim.
#[test]
fn on_a_logarithmic_radius_with_a_hole_the_decades_step_equally_from_the_edge_of_the_hole() {
    let polar = PolarView {
        hole: 0.25,
        ..PolarView::default()
    };
    let points = [(0.0, 1.0), (90.0, 10.0), (180.0, 100.0), (270.0, 1000.0)];
    let (scene, ax, id) = markers_at(polar, &points, |fx, ax| {
        fx.ax(ax).y.scale = Scale::Log;
        limits(fx, ax, None, Some((1.0, 1000.0)));
    });
    let frame = full_circle(&scene, ax, 0.25);
    let drawn = marker_positions(&scene, id);
    assert_eq!(drawn.len(), points.len());
    for (k, &(theta, r)) in points.iter().enumerate() {
        let rho = frame.inner + k as f64 / 3.0 * (frame.rim - frame.inner);
        assert_at(
            drawn[k],
            frame.at(theta, rho),
            &format!("({theta}°, {r}) on a logarithmic radius with a hole"),
        );
    }
}

// Why: a sweep about the zero direction (−60° to 60°, a sonar or radar display) crosses angle 0, so its limits have
// opposite signs. The data keeps its true directions on both sides of 0, the sector clip covers exactly the 120°
// between the edges (described anticlockwise from its smaller page angle, whatever the direction of the axis), and
// the inverse map reports a pointer at −30° as −30, within the limits, not as 330. Both the default view and a compass
// view (zero at north, clockwise) are checked, because each puts the wrap of page angles in a different place.
#[test]
fn a_sector_crossing_angle_zero_keeps_its_directions_its_clip_and_its_signed_angles() {
    for (zero_deg, clockwise, start_deg) in [(0.0, false, -60.0f64), (90.0, true, 30.0)] {
        let polar = PolarView {
            zero_deg,
            ..PolarView::default()
        };
        let points = [(-30.0, 1.0), (0.0, 2.0), (45.0, 1.5)];
        let (scene, ax, id) = markers_at(polar, &points, |fx, ax| {
            fx.ax(ax).x.reverse = clockwise;
            limits(fx, ax, Some((-60.0, 60.0)), None);
        });
        let frame = published_frame(&scene, ax);
        let sign = if clockwise { -1.0 } else { 1.0 };
        let page = |theta: f64| zero_deg + sign * theta;
        let drawn = marker_positions(&scene, id);
        assert_eq!(drawn.len(), points.len());
        for (&(theta, r), &p) in points.iter().zip(&drawn) {
            assert_at(
                p,
                frame.at(page(theta), frame.radius(0.0, 2.0, r)),
                &format!("({theta}°, {r}) with zero at {zero_deg}° and clockwise {clockwise}"),
            );
        }
        assert_clipped_to(
            &scene,
            id,
            Sector {
                centre: frame.centre,
                inner: 0.0,
                outer: frame.rim,
                start: start_deg.to_radians(),
                sweep: (120.0f64).to_radians(),
            },
        );
        let kind = &axes_hit(&scene, ax).kind;
        let back = kind
            .to_data(frame.at(page(-30.0), frame.radius(0.0, 2.0, 1.0)))
            .expect("a point inside the sector has data coordinates");
        assert_close(back[0], -30.0, 1e-9);
        assert_close(back[1], 1.0, 1e-9);
        assert_eq!(
            kind.to_data(frame.at(page(90.0), frame.radius(0.0, 2.0, 1.0))),
            None,
            "a point beyond the sector's edge has none"
        );
    }
}

// Why: manual angular limits draw a partial sector, which is how a radar sweep or a half-plane pattern is shown. The
// limits of the sector are the angles of its two radial edges, so the data keeps its true directions: a point at 45°
// in a sector from 0° to 90° still points north-east. The sector is fitted to the plot rectangle, so its centre and
// rim are those the hit map publishes; the published geometry must also put the limits on the sector's edges.
#[test]
fn a_partial_sector_keeps_each_angle_in_its_true_direction() {
    for (zero_deg, clockwise) in [(0.0, false), (90.0, true)] {
        let polar = PolarView {
            zero_deg,
            ..PolarView::default()
        };
        let points = [(0.0, 2.0), (45.0, 1.0), (90.0, 2.0)];
        let (scene, ax, id) = markers_at(polar, &points, |fx, ax| {
            fx.ax(ax).x.reverse = clockwise;
            limits(fx, ax, Some((0.0, 90.0)), None);
        });
        let frame = published_frame(&scene, ax);
        assert_close(frame.inner, 0.0, TOL);
        let page = |theta: f64| {
            if clockwise {
                zero_deg - theta
            } else {
                zero_deg + theta
            }
        };
        for (&(theta, r), &p) in points.iter().zip(&marker_positions(&scene, id)) {
            assert_at(
                p,
                frame.at(page(theta), frame.radius(0.0, 2.0, r)),
                &format!("({theta}°, {r}) with zero at {zero_deg}° and clockwise {clockwise}"),
            );
        }
        let (_, x, _) = polar_hit(&scene, ax);
        assert_close((x.end - x.start).abs(), PI / 2.0, 1e-9);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Artists that draw data directly
// ---------------------------------------------------------------------------------------------------------------

// Why: a series draws exactly the points it is given and joins them with straight lines, in polar axes as in
// Cartesian ones; if the user wants a circle, they supply its points. A series from (0°, 1) to (90°, 1) is therefore
// one straight chord from east to north, not a quarter arc. Interpolating in data space would draw a curve the data
// does not contain and make a radar chart's polygon bulge.
#[test]
fn a_series_joins_its_points_with_one_straight_chord_and_adds_no_vertices() {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let line = fx.line(ax, &[0.0, 90.0], &[1.0, 1.0], None, |_| {});
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    let scene = compile_figure(&fx.build());
    let frame = full_circle(&scene, ax, 0.0);
    let drawn = from_source(&leaves(&scene), line);
    let segments: Vec<_> = drawn.iter().flat_map(Leaf::line_segments).collect();
    assert_eq!(
        segments.len(),
        1,
        "two points are joined by one straight segment: {segments:?}"
    );
    let (a, b) = segments[0];
    assert_at(a, frame.at(0.0, frame.rim / 2.0), "the first point");
    assert_at(b, frame.at(90.0, frame.rim / 2.0), "the second point");
}

// Why: values beyond the radial limits are clipped by the plot region, as values beyond the limits of Cartesian axes
// are clipped by the plot rectangle: the chord to a point beyond the rim is drawn to that point's true place and cut
// off at the rim, so the part inside keeps its true direction. The cut is a clip of the circular region (a sector
// clip of the display list), not a clamping of the data, which would bend the chord.
#[test]
fn a_series_beyond_the_rim_is_drawn_to_its_true_place_inside_a_sector_clip_of_the_plot_region() {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let line = fx.line(ax, &[0.0, 90.0], &[1.0, 3.0], None, |_| {});
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    let scene = compile_figure(&fx.build());
    let frame = full_circle(&scene, ax, 0.0);
    let ends: Vec<Point> = from_source(&leaves(&scene), line)
        .iter()
        .flat_map(Leaf::subpaths)
        .flatten()
        .collect();
    assert_eq!(ends.len(), 2, "{ends:?}");
    assert_at(
        ends[1],
        frame.at(90.0, 1.5 * frame.rim),
        "the point beyond the rim",
    );
    assert_clipped_to(
        &scene,
        line,
        Sector {
            centre: frame.centre,
            inner: 0.0,
            outer: frame.rim,
            start: 0.0,
            sweep: TAU,
        },
    );
}

// Why: the sector clip is the plot region itself, so in a partial sector with a hole it must have the sector's edges
// and the hole's radius; a clip of the whole disc would let a series spill across the radial edges or into the hole.
// A sector from 0° to 90° measured clockwise from the north is the north-east quadrant: on the page it starts at 0
// (east) and sweeps a quarter turn anticlockwise to the north.
#[test]
fn in_a_partial_sector_with_a_hole_the_sector_clip_has_the_edges_of_the_sector_and_of_the_hole() {
    let mut fx = Fx::new();
    let ax = polar_axes(
        &mut fx,
        PolarView {
            zero_deg: 90.0,
            hole: 0.5,
            ..PolarView::default()
        },
    );
    fx.ax(ax).x.reverse = true;
    let line = fx.line(ax, &[10.0, 80.0], &[1.0, 1.0], None, |_| {});
    limits(&mut fx, ax, Some((0.0, 90.0)), Some((0.0, 2.0)));
    let scene = compile_figure(&fx.build());
    let frame = published_frame(&scene, ax);
    assert_close(frame.inner, 0.5 * frame.rim, TOL);
    assert_clipped_to(
        &scene,
        line,
        Sector {
            centre: frame.centre,
            inner: frame.inner,
            outer: frame.rim,
            start: 0.0,
            sweep: PI / 2.0,
        },
    );
}

// Why: a value below the radial lower limit lies beyond the centre (or the edge of the hole) and has no place in the
// plot region. Placing its negative page radius naively would reflect it through the centre, showing it on the
// opposite side as if it were a real value there: a point at (180°, −1) with the lower limit 0 would appear at
// (0°, 1); clamping it to the centre would show it as a value of 0; and with a hole it would land inside the hole,
// where the sector clip hides it but a datatip would still find it. Such a point cannot be placed, so no marker is
// drawn for it, while its neighbours are drawn where they belong.
#[test]
fn a_marker_below_the_radial_lower_limit_is_not_drawn_and_never_reflected_through_the_centre() {
    for hole in [0.0, 0.25] {
        let polar = PolarView {
            hole,
            ..PolarView::default()
        };
        let (scene, ax, id) = markers_at(polar, &[(180.0, -1.0), (90.0, 1.0)], |_, _| {});
        let frame = full_circle(&scene, ax, hole);
        let drawn = marker_positions(&scene, id);
        assert_eq!(
            drawn.len(),
            1,
            "only the point within the radial limits is drawn, with a hole of {hole}: {drawn:?}"
        );
        assert_at(
            drawn[0],
            frame.at(90.0, frame.radius(0.0, 2.0, 1.0)),
            "the point within the limits",
        );
    }
}

// Why: a line through a point that cannot be placed must break there, exactly as it breaks at a non-finite value,
// rather than join its neighbours with a chord that skips the point (which would draw a segment the data does not
// contain) or bend through the centre. The line from 0° to 120° with its middle point below the lower limit is
// therefore two separate segments, 0° to 30° and 90° to 120°.
#[test]
fn a_line_breaks_at_a_point_below_the_radial_lower_limit() {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let line = fx.line(
        ax,
        &[0.0, 30.0, 60.0, 90.0, 120.0],
        &[1.0, 1.0, -1.0, 1.0, 1.0],
        None,
        |_| {},
    );
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    let scene = compile_figure(&fx.build());
    let frame = full_circle(&scene, ax, 0.0);
    let segments: Vec<_> = from_source(&leaves(&scene), line)
        .iter()
        .flat_map(Leaf::line_segments)
        .collect();
    assert_eq!(segments.len(), 2, "{segments:?}");
    let half = frame.rim / 2.0;
    for (k, (from, to)) in [(0.0, 30.0), (90.0, 120.0)].into_iter().enumerate() {
        assert_at(segments[k].0, frame.at(from, half), "the start of a piece");
        assert_at(segments[k].1, frame.at(to, half), "the end of a piece");
    }
}

// Why: a contour is drawn from its data directly: marching squares finds where each level crosses the edges of the
// grid's cells, and those crossings are joined by straight chords on the page. A field equal to the radius, sampled
// every quarter turn, crosses the level 1.5 at the four grid angles only, so its contour is a square with its corners
// on the circle of radius 1.5, not that circle; a contour that followed the axes would invent values between the grid
// angles that the data does not contain.
#[test]
fn a_contour_joins_its_grid_crossings_with_straight_chords() {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let contour = fx.contour(
        ax,
        &[0.0, 90.0, 180.0, 270.0, 360.0],
        &[1.0, 2.0],
        |_, r| r,
        |c| {
            c.levels = Some(ironlab_ir::Levels::Explicit { values: vec![1.5] });
        },
    );
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    let scene = compile_figure(&fx.build());
    let frame = full_circle(&scene, ax, 0.0);
    let segments: Vec<_> = from_source(&leaves(&scene), contour)
        .iter()
        .flat_map(Leaf::line_segments)
        .collect();
    assert_eq!(segments.len(), 4, "one chord per grid cell: {segments:?}");
    let rho = frame.radius(0.0, 2.0, 1.5);
    for p in segments.iter().flat_map(|&(a, b)| [a, b]) {
        let (deg, r) = frame.polar_of(p);
        assert_close(r, rho, TOL);
        let off_grid = (deg / 90.0 - (deg / 90.0).round()).abs() * 90.0;
        assert!(
            off_grid < 1e-6,
            "every vertex is a crossing on a grid angle, found one at {deg}°"
        );
    }
}

// Why: a surface seen from above is a pseudocolour plot whose faces are the quadrilaterals of their four mapped grid
// nodes, in polar axes as in Cartesian ones; `surf(θ, r, z)` over an angle–radius grid is drawn with as many nodes
// as the user supplies. Each face therefore has exactly its four corners as vertices, and its outer edge is a chord.
#[test]
fn a_surface_face_is_the_quadrilateral_of_its_four_mapped_nodes() {
    let theta = [0.0, 90.0, 180.0];
    let r = [1.0, 2.0];
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let surface = fx.surface(ax, &theta, &r, |t, r| t + r, |_| {});
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    let scene = compile_figure(&fx.build());
    let frame = full_circle(&scene, ax, 0.0);
    let faces = filled_outlines(&scene, surface);
    assert_eq!(faces.len(), 2, "one face per grid cell: {faces:?}");
    for i in 0..2 {
        let corners = [
            frame.at(theta[i], frame.radius(0.0, 2.0, r[0])),
            frame.at(theta[i + 1], frame.radius(0.0, 2.0, r[0])),
            frame.at(theta[i + 1], frame.radius(0.0, 2.0, r[1])),
            frame.at(theta[i], frame.radius(0.0, 2.0, r[1])),
        ];
        let face = faces
            .iter()
            .find(|f| {
                corners
                    .iter()
                    .all(|&c| f.iter().any(|&p| points_close(p, c, TOL)))
            })
            .unwrap_or_else(|| panic!("a face has the corners of cell {i}: {faces:?}"));
        assert_eq!(face.len(), 4, "a face has only its four corners: {face:?}");
    }
}

// Why: a quiver arrow runs from its base to the tip at base + (u, v) in data units, u an increment of angle and v of
// radius, and its shaft is the straight line between the two mapped points. An arrow from (0°, 1) with u = 90° and
// v = 0 points from the east to the north along the chord, not round the circle of radius 1.
#[test]
fn a_quiver_shaft_is_the_straight_chord_from_its_base_to_its_tip() {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let quiver = fx.quiver(ax, &[0.0], &[1.0], None, &[90.0], &[0.0], None);
    if let Some(ironlab_ir::Artist::Quiver(q)) = fx.ax(ax).artists.last_mut() {
        q.scale = ironlab_ir::QuiverScale::Off;
    }
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    let scene = compile_figure(&fx.build());
    let frame = full_circle(&scene, ax, 0.0);
    let subpaths: Vec<Vec<Point>> = from_source(&leaves(&scene), quiver)
        .iter()
        .flat_map(Leaf::subpaths)
        .collect();
    let shaft = subpaths
        .iter()
        .find(|s| {
            s.iter()
                .any(|&p| points_close(p, frame.at(0.0, frame.rim / 2.0), TOL))
        })
        .unwrap_or_else(|| panic!("a subpath starts at the base: {subpaths:?}"));
    assert_eq!(
        shaft.len(),
        2,
        "the shaft is one straight segment: {shaft:?}"
    );
    assert_at(shaft[0], frame.at(0.0, frame.rim / 2.0), "the base");
    assert_at(shaft[1], frame.at(90.0, frame.rim / 2.0), "the tip");
}

// ---------------------------------------------------------------------------------------------------------------
// Artists that draw representing shapes: bars
// ---------------------------------------------------------------------------------------------------------------

/// A polar axes with radial limits 0 to 2 holding one upright bar at `position` of the width `width` (in degrees)
/// from `base` to `base + value`, edited by `edit`.
fn one_bar(
    position: f64,
    width: f64,
    base: f64,
    value: f64,
    edit: impl FnOnce(&mut Fx, NodeId),
) -> (Scene, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let bar = fx.bar(ax, &[position], &[value], |b, _| {
        b.width = BarWidth::Constant(width);
        b.base = PerBar::Constant(base);
    });
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    edit(&mut fx, ax);
    (compile_figure(&fx.build()), ax, bar)
}

// Why: a bar represents the region from its base to its end across its width, so in polar axes it is a wedge of an
// annulus: its top is an arc at the radius of its end, its bottom an arc at the radius of its base, and its sides are
// radial. This is the wind-rose wedge. A bar drawn as the quadrilateral of its four corners would read as a shorter
// bar in its middle than at its corners.
#[test]
fn an_upright_bar_is_a_wedge_whose_top_is_an_arc_at_the_radius_of_its_end_and_whose_sides_are_radial()
 {
    let (scene, ax, bar) = one_bar(90.0, 30.0, 0.5, 1.0, |_, _| {});
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let frame = full_circle(&scene, ax, 0.0);
    let outlines = filled_outlines(&scene, bar);
    assert_eq!(
        outlines.len(),
        1,
        "a bar is one closed outline: {outlines:?}"
    );
    assert_annular_sector(
        &frame,
        &outlines[0],
        Annulus {
            a0: 75.0,
            a1: 105.0,
            r0: frame.rim / 4.0,
            r1: frame.rim * 0.75,
        },
        "the bar from 0.5 to 1.5 across 75° to 105°",
    );
}

// Why: a bar from a base of 0 starts at the centre, so the wedge narrows to a point there: both radial sides meet at
// the centre and there is no inner arc. This is the classic polar bar chart.
#[test]
fn a_bar_from_the_lower_radial_limit_is_a_wedge_from_the_centre() {
    let (scene, ax, bar) = one_bar(45.0, 90.0, 0.0, 2.0, |_, _| {});
    let frame = full_circle(&scene, ax, 0.0);
    let outline = &filled_outlines(&scene, bar)[0];
    assert_annular_sector(
        &frame,
        outline,
        Annulus {
            a0: 0.0,
            a1: 90.0,
            r0: 0.0,
            r1: frame.rim,
        },
        "the bar from the centre to the rim across the first quadrant",
    );
    assert!(
        outline.iter().any(|&p| points_close(p, frame.centre, TOL)),
        "the wedge reaches the centre: {outline:?}"
    );
}

// Why: a horizontal bar places its position along the radius and its value along the angle, which is the radial bar
// chart: each bar is a band of an annulus starting at angle 0 and sweeping through its value, its two arcs at the
// radii of its two long edges.
#[test]
fn a_horizontal_bar_is_an_arc_band_sweeping_through_its_value() {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let bar = fx.bar(ax, &[1.0], &[135.0], |b, _| {
        b.orientation = Orientation::Horizontal;
        b.width = BarWidth::Constant(0.5);
    });
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let frame = full_circle(&scene, ax, 0.0);
    let outlines = filled_outlines(&scene, bar);
    assert_eq!(outlines.len(), 1);
    assert_annular_sector(
        &frame,
        &outlines[0],
        Annulus {
            a0: 0.0,
            a1: 135.0,
            r0: frame.radius(0.0, 2.0, 0.75),
            r1: frame.radius(0.0, 2.0, 1.25),
        },
        "the radial bar at 1 from 0° to 135°",
    );
}

// Why: a bar longer than the radial limits must stop at the rim, as a Cartesian bar stops at the edge of the plot
// area: its top becomes the rim's arc rather than an arc beyond it that the clip would cut. A bar wider than a partial
// sector is likewise cut at the sector's radial edge. Clamping the shape in data space keeps its outline the outline
// of what is visible, which is what its hit area must be.
#[test]
fn a_bar_beyond_the_radial_limit_or_the_sector_is_clamped_to_the_plot_region() {
    let (scene, ax, bar) = one_bar(90.0, 30.0, 0.0, 3.0, |_, _| {});
    let frame = full_circle(&scene, ax, 0.0);
    assert_annular_sector(
        &frame,
        &filled_outlines(&scene, bar)[0],
        Annulus {
            a0: 75.0,
            a1: 105.0,
            r0: 0.0,
            r1: frame.rim,
        },
        "the bar to 3 clamped at the rim of limits 0 to 2",
    );

    let (scene, ax, bar) = one_bar(90.0, 30.0, 0.0, 1.0, |fx, ax| {
        limits(fx, ax, Some((0.0, 90.0)), None);
    });
    let frame = published_frame(&scene, ax);
    assert_annular_sector(
        &frame,
        &filled_outlines(&scene, bar)[0],
        Annulus {
            a0: 75.0,
            a1: 90.0,
            r0: 0.0,
            r1: frame.rim / 2.0,
        },
        "the bar across 75° to 105° clamped at the edge of a sector ending at 90°",
    );
}

// Why: a datatip over a bar must answer wherever the reader sees the bar and nowhere else. In polar axes the bar is a
// wedge, so its hit area is the drawn polygon: a point in the middle of the wedge finds the bar, and points just
// beyond its top arc or beside its radial sides, which lie inside its bounding box, do not.
#[test]
fn the_hit_area_of_a_bar_in_polar_axes_is_the_wedge_as_drawn() {
    let (scene, ax, bar) = one_bar(90.0, 60.0, 0.5, 1.0, |_, _| {});
    let frame = full_circle(&scene, ax, 0.0);
    let hits: Vec<_> = scene
        .hit_map
        .bars
        .iter()
        .filter(|b| b.artist == bar)
        .collect();
    assert_eq!(hits.len(), 1);
    let HitArea::Polygon(polygon) = &hits[0].area else {
        panic!(
            "a bar in polar axes has a polygon for its hit area: {:?}",
            hits[0].area
        );
    };
    let drawn = &filled_outlines(&scene, bar)[0];
    assert_eq!(
        polygon.len(),
        drawn.len(),
        "the hit polygon is the drawn outline"
    );
    for (a, b) in polygon.iter().zip(drawn) {
        assert_at(*a, *b, "a vertex of the hit polygon");
    }
    let inside = frame.at(90.0, frame.radius(0.0, 2.0, 1.0));
    assert_eq!(scene.hit_map.bar_at(inside).map(|b| b.artist), Some(bar));
    // Just beyond the top arc near one side, and just beside a radial side at mid radius: both lie inside the
    // bounding box of the wedge.
    for outside in [
        frame.at(65.0, frame.radius(0.0, 2.0, 1.5) + 0.5),
        frame.at(121.0, frame.radius(0.0, 2.0, 1.4)),
    ] {
        assert!(
            scene.hit_map.bar_at(outside).is_none(),
            "{outside:?} lies outside the wedge"
        );
    }
}

// Why: Cartesian bars must keep exactly the hit rectangles they have today, because a front end draws the outline of
// the bar under the pointer from it and the gallery's behaviour must not change. The rectangle is the one the axis
// maps give for the bar's width and its base and end.
#[test]
fn a_cartesian_bar_keeps_its_rectangle_as_its_hit_area() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let bar = fx.bar(ax, &[1.0, 2.0], &[3.0, 4.0], |b, _| {
        b.width = BarWidth::Constant(0.5);
        b.base = PerBar::Constant(1.0);
    });
    let scene = compile_figure(&fx.build());
    let (xmap, ymap) = axis_maps(&scene, ax);
    for (k, (position, value)) in [(1.0, 3.0), (2.0, 4.0)].into_iter().enumerate() {
        let hit = &scene
            .hit_map
            .bars
            .iter()
            .filter(|b| b.artist == bar)
            .collect::<Vec<_>>()[k];
        let HitArea::Rect(rect) = hit.area else {
            panic!(
                "a Cartesian bar keeps a rectangle for its hit area: {:?}",
                hit.area
            );
        };
        let (x0, x1) = (
            xmap.to_figure(position - 0.25),
            xmap.to_figure(position + 0.25),
        );
        let (y0, y1) = (ymap.to_figure(1.0), ymap.to_figure(1.0 + value));
        let want = Rect::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
        for (a, b) in [
            (rect.x, want.x),
            (rect.y, want.y),
            (rect.width, want.width),
            (rect.height, want.height),
        ] {
            assert_close(a, b, 1e-9);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Stairs
// ---------------------------------------------------------------------------------------------------------------

// Why: stairs hold each value across its bin, so in polar axes each run is an arc at the radius of its value between
// the angles of its edges, and each riser is radial. This is the outline of a polar histogram: a run drawn as a chord
// would dip inside its value in the middle of the bin, under-reporting it exactly where the eye reads it.
#[test]
fn each_run_of_stairs_lies_on_the_circle_of_its_value_and_its_hit_follows_the_run() {
    let edges = [0.0, 90.0, 180.0, 270.0];
    let values = [1.0, 2.0, 0.5];
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let stairs = fx.stairs(ax, &edges, &values, |_| {});
    limits(&mut fx, ax, None, Some((0.0, 4.0)));
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let frame = full_circle(&scene, ax, 0.0);
    let outline: Vec<Point> = stroked(&scene, stairs).into_iter().flatten().collect();
    for (k, &value) in values.iter().enumerate() {
        let rho = frame.radius(0.0, 4.0, value);
        let run = Annulus {
            a0: edges[k],
            a1: edges[k + 1],
            r0: rho,
            r1: rho,
        };
        // Every vertex strictly inside the bin's angles lies on the run's circle.
        let within: Vec<Point> = outline
            .iter()
            .copied()
            .filter(|&p| {
                let (deg, _) = frame.polar_of(p);
                deg > edges[k] + 1e-6 && deg < edges[k + 1] - 1e-6
            })
            .collect();
        assert!(
            !within.is_empty(),
            "the run of bin {k} is followed by vertices inside it"
        );
        for p in within {
            assert_close(frame.polar_of(p).1, rho, TOL);
        }

        let hit = scene
            .hit_map
            .steps
            .iter()
            .find(|s| s.artist == stairs && s.index == k)
            .unwrap_or_else(|| panic!("step {k} has a hit"));
        assert!(
            hit.run.len() > 2,
            "a run in polar axes is a polyline along its arc: {:?}",
            hit.run
        );
        assert_annular_sector(&frame, &hit.run, run, &format!("the hit run of step {k}"));
        assert!(
            points_close(hit.run[0], frame.at(edges[k], rho), TOL)
                || points_close(hit.run[0], frame.at(edges[k + 1], rho), TOL),
            "the run starts at an edge of its bin"
        );
        // A pointer on the middle of the arc is on the run, not a chord's sagitta away from it.
        let middle = frame.at((edges[k] + edges[k + 1]) / 2.0, rho);
        assert!(points_close(hit.nearest(middle), middle, 1e-3));
    }
}

// Why: Cartesian stairs keep runs of two points, the two ends of the run on the page, so that nothing a front end
// does with them changes outside polar axes.
#[test]
fn a_cartesian_step_hit_is_the_two_ends_of_its_run() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let stairs = fx.stairs(ax, &[0.0, 1.0, 3.0], &[2.0, 5.0], |_| {});
    let scene = compile_figure(&fx.build());
    let (xmap, ymap) = axis_maps(&scene, ax);
    for (k, (a, b, value)) in [(0.0, 1.0, 2.0), (1.0, 3.0, 5.0)].into_iter().enumerate() {
        let hit = scene
            .hit_map
            .steps
            .iter()
            .find(|s| s.artist == stairs && s.index == k)
            .unwrap();
        assert_eq!(hit.run.len(), 2, "{:?}", hit.run);
        assert_at(
            hit.run[0],
            Point::new(xmap.to_figure(a), ymap.to_figure(value)),
            "run start",
        );
        assert_at(
            hit.run[1],
            Point::new(xmap.to_figure(b), ymap.to_figure(value)),
            "run end",
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Cells
// ---------------------------------------------------------------------------------------------------------------

/// Two cells side by side in angle (0° to 90° red, 90° to 180° blue) between radii 1 and 2, in polar axes with
/// radial limits 0 to 4, with the gap `gap`.
fn two_cells(gap: CellGap) -> (Scene, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let scale = fx.color_scale(ColorScaleKind::Indexed(IndexedColorScale {
        colors: vec![RED, BLUE],
        ..IndexedColorScale::default()
    }));
    let cells = fx.cells(ax, vec![1, 2], vec![0.0, 1.0], Some(scale), |c, fx| {
        c.columns = CellEdges::Edges(fx.vector(&[0.0, 90.0, 180.0]));
        c.rows = CellEdges::Edges(fx.vector(&[1.0, 2.0]));
        c.gap = gap;
    });
    limits(&mut fx, ax, None, Some((0.0, 4.0)));
    (compile_figure(&fx.build()), ax, cells)
}

// Why: a cell represents the region between its column edges and its row edges, which in polar axes is an annular
// sector; a heatmap over angle and range (a radar display) is read cell by cell, and a quadrilateral cell would leave
// slivers between rings. Seamless neighbours share their radial edge exactly.
#[test]
fn each_cell_is_an_annular_sector_between_its_edges() {
    let (scene, ax, cells) = two_cells(CellGap::default());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let frame = full_circle(&scene, ax, 0.0);
    for (color, a0, a1) in [(RED, 0.0, 90.0), (BLUE, 90.0, 180.0)] {
        let outlines = filled_in(&scene, cells, color);
        assert_eq!(outlines.len(), 1, "one outline per cell");
        assert_annular_sector(
            &frame,
            &outlines[0],
            Annulus {
                a0,
                a1,
                r0: frame.rim / 4.0,
                r1: frame.rim / 2.0,
            },
            &format!("the cell from {a0}° to {a1}°"),
        );
    }
}

// Why: the gap is a fraction of each cell's extent, applied in the coordinates the axes draws, which in polar axes are
// the angle and the radius: a gap of 0.2 across leaves 9° on each side of a 90° cell, and a gap of 0.5 down leaves a
// quarter of the radial span at each end. Applied to the page bounding box it would leave unequal margins at the two
// arcs.
#[test]
fn the_gap_of_a_cell_is_applied_to_its_angle_and_its_radius() {
    let (scene, ax, cells) = two_cells(CellGap { x: 0.2, y: 0.5 });
    let frame = full_circle(&scene, ax, 0.0);
    let outline = &filled_in(&scene, cells, RED)[0];
    assert_annular_sector(
        &frame,
        outline,
        Annulus {
            a0: 9.0,
            a1: 81.0,
            r0: frame.radius(0.0, 4.0, 1.25),
            r1: frame.radius(0.0, 4.0, 1.75),
        },
        "the cell from 0° to 90° and 1 to 2 with gaps of 0.2 and 0.5",
    );
    let hit = scene
        .hit_map
        .cells
        .iter()
        .find(|c| c.artist == cells && c.column == 0)
        .expect("the red cell has a hit");
    assert!(
        matches!(hit.area, HitArea::Polygon(_)),
        "a cell in polar axes has a polygon for its hit area: {:?}",
        hit.area
    );
    let middle = frame.at(45.0, frame.radius(0.0, 4.0, 1.5));
    assert_eq!(
        scene.hit_map.cell_at(middle).map(|c| (c.artist, c.column)),
        Some((cells, 0))
    );
    // In the gap beside the cell at its middle radius, and in the gap beyond its outer arc.
    for gap in [
        frame.at(85.0, frame.radius(0.0, 4.0, 1.5)),
        frame.at(45.0, frame.radius(0.0, 4.0, 1.9)),
    ] {
        assert!(
            scene.hit_map.cell_at(gap).is_none(),
            "{gap:?} lies in the gap"
        );
    }
}

// Why: Cartesian cells keep exactly the hit rectangles they have today: the part of the cell inside its gap.
#[test]
fn a_cartesian_cell_keeps_its_rectangle_as_its_hit_area() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scale = fx.color_scale(ColorScaleKind::Indexed(IndexedColorScale {
        colors: vec![RED, BLUE],
        ..IndexedColorScale::default()
    }));
    let cells = fx.cells(ax, vec![1, 2], vec![0.0, 1.0], Some(scale), |c, _| {
        c.gap = CellGap { x: 0.2, y: 0.5 };
    });
    let scene = compile_figure(&fx.build());
    let (xmap, ymap) = axis_maps(&scene, ax);
    for column in 0..2 {
        let hit = scene
            .hit_map
            .cells
            .iter()
            .find(|c| c.artist == cells && c.column == column)
            .unwrap();
        let HitArea::Rect(rect) = hit.area else {
            panic!(
                "a Cartesian cell keeps a rectangle for its hit area: {:?}",
                hit.area
            );
        };
        let x = column as f64;
        let (x0, x1) = (xmap.to_figure(x - 0.4), xmap.to_figure(x + 0.4));
        let (y0, y1) = (ymap.to_figure(-0.25), ymap.to_figure(0.25));
        for (a, b) in [
            (rect.x, x0.min(x1)),
            (rect.y, y0.min(y1)),
            (rect.width, (x1 - x0).abs()),
            (rect.height, (y1 - y0).abs()),
        ] {
            assert_close(a, b, 1e-9);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------------------------------------------

// Why: each pixel of an image represents the region between its edges, so in polar axes it is an annular sector, as
// a cell is; a sonar or radar sweep is an image over angle and range. The image is drawn through a lattice whose
// nodes are placed by the axes: every pixel edge is a node, so every pixel lies exactly between its own edges, and the
// lattice is refined along the angle so that its chords follow the arcs. The datatip inverts the same map, so the
// pixel under the pointer is the pixel drawn there.
#[test]
fn an_image_in_polar_axes_is_a_lattice_whose_nodes_lie_at_their_angles_and_radii() {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    // Three columns centred on 15°, 45° and 75° (edges 0°, 30°, 60°, 90°); two rows centred on 1.5 and 2.5 (edges 1, 2
    // and 3).
    let image = fx.imagesc(
        ax,
        vec![2, 3],
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
        xy(range(15.0, 75.0), range(1.5, 2.5)),
        |_| {},
    );
    limits(&mut fx, ax, None, Some((0.0, 4.0)));
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let frame = full_circle(&scene, ax, 0.0);

    let leaf = from_source(&leaves(&scene), image)
        .into_iter()
        .find(|l| l.image().is_some())
        .expect("the image is drawn as an image item");
    let item = leaf.image().unwrap();
    let ironlab_scene::display::ImageGeometry::Mesh(mesh) = &item.geometry else {
        panic!(
            "an image in polar axes is drawn through a mesh: {:?}",
            item.geometry
        );
    };
    for edge in [0.0, 1.0, 2.0, 3.0] {
        assert!(
            mesh.u.contains(&edge),
            "a node at column edge {edge}: {:?}",
            mesh.u
        );
    }
    for edge in [0.0, 1.0, 2.0] {
        assert!(
            mesh.v.contains(&edge),
            "a node at row edge {edge}: {:?}",
            mesh.v
        );
    }
    assert_eq!(mesh.points.len(), mesh.u.len() * mesh.v.len());
    for (j, &v) in mesh.v.iter().enumerate() {
        for (i, &u) in mesh.u.iter().enumerate() {
            let drawn = leaf.transform.apply(mesh.points[j * mesh.u.len() + i]);
            let want = frame.at(30.0 * u, frame.radius(0.0, 4.0, 1.0 + v));
            assert_at(drawn, want, &format!("the node at pixel-space ({u}, {v})"));
        }
    }
    let outer = frame.radius(0.0, 4.0, 3.0);
    for pair in mesh.u.windows(2) {
        let step = (30.0 * (pair[1] - pair[0])).to_radians();
        let sagitta = outer * (1.0 - (step / 2.0).cos());
        assert!(
            sagitta <= CHORD + 1e-9,
            "the lattice follows the outer arc: a step of {}° strays {sagitta} pt",
            30.0 * (pair[1] - pair[0])
        );
    }

    for ((theta, r), expected) in [
        ((15.0, 1.2), Some((0, 0))),
        ((44.0, 2.9), Some((1, 1))),
        ((89.0, 1.1), Some((0, 2))),
        ((45.0, 0.5), None),
        ((120.0, 2.0), None),
    ] {
        let found = scene
            .hit_map
            .pixel_at(frame.at(theta, frame.radius(0.0, 4.0, r)))
            .map(|(hit, row, column)| (hit.artist, row, column));
        assert_eq!(
            found,
            expected.map(|(row, column)| (image, row, column)),
            "the pixel under ({theta}°, {r})"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Automatic limits
// ---------------------------------------------------------------------------------------------------------------

// Why: the angle of polar axes is periodic, so its automatic limits are the full turn whatever the data: a pattern
// measured over a few degrees must still be drawn in its true direction on a whole circle, not stretched round the
// whole circle as a Cartesian axis would stretch its data range. A partial sector is asked for with manual limits.
#[test]
fn automatic_angular_limits_are_the_full_turn_whatever_the_data() {
    for (units, turn, data) in [
        (AngleUnits::Degrees, 360.0, [10.0, 50.0]),
        (AngleUnits::Radians, TAU, [0.1, 0.5]),
        (AngleUnits::PerTurn(12.0), 12.0, [2.0, 3.0]),
    ] {
        let mut fx = Fx::new();
        let ax = polar_axes(
            &mut fx,
            PolarView {
                angle_units: units,
                ..PolarView::default()
            },
        );
        fx.line(ax, &data, &[1.0, 2.0], None, |_| {});
        let scene = compile_figure(&fx.build());
        let (_, x, _) = polar_hit(&scene, ax);
        assert_eq!((x.min, x.max), (0.0, turn), "{units:?}");
        assert_close((x.end - x.start).abs(), TAU, 1e-12);
    }
}

// Why: a radius is a distance, so for non-negative data on a linear radial axis the automatic lower limit is 0 at the
// centre; a lower limit at the data minimum would put the smallest value at the centre, where it reads as zero.
// Data with negative values, such as a pattern in decibels, keeps its ordinary nice limits, and a logarithmic radius,
// which has no zero, keeps a positive lower limit.
#[test]
fn the_automatic_radial_lower_limit_is_zero_only_for_non_negative_linear_data() {
    let lower = |r: [f64; 2], log: bool| {
        let mut fx = Fx::new();
        let ax = polar_axes(&mut fx, PolarView::default());
        fx.line(ax, &[0.0, 90.0], &r, None, |_| {});
        if log {
            fx.ax(ax).y.scale = Scale::Log;
        }
        let scene = compile_figure(&fx.build());
        let (_, _, y) = polar_hit(&scene, ax);
        (y.min, y.max)
    };
    let (min, max) = lower([2.0, 5.0], false);
    assert_eq!(min, 0.0, "non-negative data starts at the centre");
    assert!(max >= 5.0);
    let (min, _) = lower([-30.0, -3.0], false);
    assert!(
        min <= -30.0,
        "negative data keeps its own lower limit, found {min}"
    );
    let (min, _) = lower([3.0, 300.0], true);
    assert!(
        min > 0.0 && min <= 3.0,
        "a logarithmic radius keeps a positive lower limit, found {min}"
    );
}

// Why: a horizontal reference line (`hline`) places its x values as fractions of the sector, so it names the circle of
// its radius across the whole sector, as a Cartesian `hline` crosses the whole plot area. Joining its two ends with a
// chord would draw nothing at all on a full turn, where fractions 0 and 1 are the same point.
#[test]
fn a_line_across_the_sector_in_axes_fractions_follows_its_circle() {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, PolarView::default());
    let line = fx.line(ax, &[0.0, 1.0], &[1.0, 1.0], None, |l| {
        l.x_coords = ironlab_ir::Coords::AxesFraction;
    });
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    let scene = compile_figure(&fx.build());
    let frame = full_circle(&scene, ax, 0.0);
    let vertices: Vec<Point> = stroked(&scene, line).into_iter().flatten().collect();
    assert!(vertices.len() > 8, "the circle is followed: {vertices:?}");
    let mut angles: Vec<f64> = vertices
        .iter()
        .map(|&p| {
            let (deg, rho) = frame.polar_of(p);
            assert_close(rho, frame.rim / 2.0, TOL);
            deg
        })
        .collect();
    angles.sort_by(f64::total_cmp);
    for pair in angles.windows(2) {
        let sagitta = frame.rim / 2.0 * (1.0 - ((pair[1] - pair[0]).to_radians() / 2.0).cos());
        assert!(
            sagitta <= CHORD + 1e-9,
            "a chord of {}° strays {sagitta} pt",
            pair[1] - pair[0]
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Pan
// ---------------------------------------------------------------------------------------------------------------

/// A polar axes with a grid on both axes, the box, a line and markers, an angle label, a radius label and a title,
/// panned by `view`: everything a pan may move or must leave alone. Returns the scene, the axes and the line.
fn panned_polar(view: View2d) -> (Scene, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes_in(
        cell(0, 0, 1, 1),
        Projection::Polar {
            polar: PolarView::default(),
            view,
        },
    );
    let line = fx.line(ax, &[0.0, 90.0, 200.0], &[1.0, 2.0, 1.5], None, |_| {});
    fx.scatter(ax, &[45.0, 300.0], &[0.5, 1.8], None, |_, _| {});
    limits(&mut fx, ax, None, Some((0.0, 2.0)));
    let axes = fx.ax(ax);
    axes.x.grid = true;
    axes.y.grid = true;
    axes.x.label = text("Bearing");
    axes.y.label = text("Range");
    axes.title = text("Pattern");
    (compile_figure(&fx.build()), ax, line)
}

/// Returns `p` moved by `(dx, dy)`.
fn moved(p: Point, (dx, dy): (f64, f64)) -> Point {
    Point::new(p.x + dx, p.y + dy)
}

// Why: panning moves the drawn polar axes across its plot area as a camera offset, so that a reader who zoomed the
// radius can bring a part of the disc back into view. The whole axes must move together: the centre the hit map
// publishes (through which the pointer is read), the grid, the frame, the tick labels, every artist and the sector
// that clips them, each by exactly the pan's fraction of the plot area's width and height. The plot area itself, and
// so the margins, must not change, and the axis labels and the title, which annotate the cell rather than the disc,
// stay where they were; a pan that also moved the layout would make the axes jump as it is dragged.
#[test]
fn a_pan_moves_the_centre_grid_frame_tick_labels_and_artists_by_the_fraction_of_the_plot_area() {
    let (home, ax, line) = panned_polar(View2d::default());
    let pan = View2d {
        pan_x: 0.1,
        pan_y: -0.05,
    };
    let (shifted, _, _) = panned_polar(pan);

    let plot = axes_hit(&home, ax).plot_rect;
    assert_eq!(
        axes_hit(&shifted, ax).plot_rect,
        plot,
        "the plot area does not change"
    );
    let offset = (pan.pan_x * plot.width, pan.pan_y * plot.height);
    let (home_frame, panned_frame) = (published_frame(&home, ax), published_frame(&shifted, ax));
    assert_at(
        panned_frame.centre,
        moved(home_frame.centre, offset),
        "the published centre",
    );
    assert_close(panned_frame.rim, home_frame.rim, TOL);

    let (before, after) = (leaves(&home), leaves(&shifted));
    assert_eq!(before.len(), after.len(), "the same items are drawn");
    let fixed = ["Bearing", "Range", "Pattern"];
    for (b, a) in before.iter().zip(&after) {
        let (Some(bb), Some(ab)) = (b.bbox(), a.bbox()) else {
            continue;
        };
        let annotation = a.glyphs().is_some_and(|g| fixed.contains(&g.text.as_str()));
        let expected = if annotation {
            Point::new(bb.x, bb.y)
        } else {
            moved(Point::new(bb.x, bb.y), offset)
        };
        assert!(
            points_close(Point::new(ab.x, ab.y), expected, TOL)
                && (ab.width - bb.width).abs() < TOL
                && (ab.height - bb.height).abs() < TOL,
            "{:?} of {:?}: {bb:?} becomes {ab:?}, expected at {expected:?}",
            a.glyphs().map(|g| g.text.clone()),
            a.source,
        );
    }

    assert_clipped_to(
        &shifted,
        line,
        Sector {
            centre: panned_frame.centre,
            inner: 0.0,
            outer: panned_frame.rim,
            start: 0.0,
            sweep: TAU,
        },
    );
}

// Why: a panned axes may be moved partly out of its plot area, and what is moved out must not paint over the
// neighbouring tiles, the colourbars or the margins of other axes, just as a panned 3D axes is clipped. Every artist
// stays clipped to the plot area, and everything the pan moves is clipped to the axes' own cell.
#[test]
fn a_panned_polar_axes_stays_clipped_to_its_plot_area_and_its_cell() {
    let (scene, ax, line) = panned_polar(View2d {
        pan_x: 0.4,
        pan_y: 0.3,
    });
    let plot = axes_hit(&scene, ax).plot_rect;
    let page = Rect::new(
        0.0,
        0.0,
        scene.display_list.width_pt,
        scene.display_list.height_pt,
    );
    for leaf in from_source(&leaves(&scene), line) {
        let clip = leaf.clip.expect("every leaf of the line is clipped");
        assert!(
            inside(clip, plot, TOL),
            "the line is clipped within the plot area: {clip:?} against {plot:?}"
        );
    }
    for leaf in from_source(&leaves(&scene), ax) {
        let clip = leaf.clip.expect("every leaf the axes draws is clipped");
        assert!(
            inside(plot, clip, TOL) && inside(clip, page, TOL),
            "the axes is clipped to its cell, which holds the plot area: {clip:?}"
        );
    }
}

// Why: a pan that is not finite cannot place the axes; as in 3D axes, it is drawn as no pan rather than failing or
// drawing nothing, so a figure carrying one from a faulty writer still shows its data where the reader expects it.
#[test]
fn a_pan_that_is_not_finite_draws_the_axes_unpanned() {
    let (home, _, _) = panned_polar(View2d::default());
    let (odd, _, _) = panned_polar(View2d {
        pan_x: f64::NAN,
        pan_y: f64::INFINITY,
    });
    assert_eq!(odd.display_list, home.display_list);
    assert_eq!(odd.hit_map, home.hit_map);
}
