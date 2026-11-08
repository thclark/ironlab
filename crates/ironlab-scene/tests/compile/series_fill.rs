//! The fill of a series down to a base (#123, #124): what is filled, where it lies in paint order and in its clip,
//! how it looks, and how its legend sample and legend entry represent it.
//!
//! A series is an artist that draws its data directly, so its fill obeys the governing rule of the polar axes: the
//! filled region of each unbroken run of points is the polygon of the run's mapped points followed by the mapped
//! points (x_last, base) and (x_first, base), every edge a straight chord on the page, in Cartesian and polar axes
//! alike. The tests here compute those vertices from the plot rectangle (Cartesian, with manual limits) or from the
//! centre and rim of the polar circle (taken from the plot rectangle, in which the circle is centred and whose shorter
//! side is its diameter), never from the compiler's own mapping.

use ironlab_ir::{
    Colour, ColourSpec, Coords, DashStyle, Limits, MarkerShape, NodeId, PolarView, Projection,
    Scale, Series,
};
use ironlab_scene::display::{PathSegment, Point, Rect, Rgba};
use ironlab_scene::hit::LegendOwner;
use ironlab_scene::{Scene, SceneWarningKind};

use crate::common::{COLOUR_ORDER, Fx, cell, compile_figure, rgb8, rgb8_close, rgba, text};
use crate::probe::{
    Leaf, assert_close, axes_hit, axis_maps, centre, from_source, leaves, panel_entries,
    points_close, runs_with_text, text_bbox,
};

/// Agreement expected of a vertex placed on the page from data, in points.
const TOL: f64 = 1e-6;

const RED: Colour = Colour::rgb(1.0, 0.0, 0.0);
const BLUE: Colour = Colour::rgb(0.0, 0.0, 1.0);

// ---------------------------------------------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------------------------------------------

/// Returns the leaves of `id` whose paths are filled, in paint order.
fn fill_leaves(scene: &Scene, id: NodeId) -> Vec<Leaf> {
    from_source(&leaves(scene), id)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.fill.is_some()))
        .collect()
}

/// Returns the polygons `id` filled, one per subpath, in figure space, without a closing vertex that repeats the
/// first.
fn fill_polygons(scene: &Scene, id: NodeId) -> Vec<Vec<Point>> {
    fill_leaves(scene, id)
        .iter()
        .flat_map(Leaf::subpaths)
        .collect()
}

/// Returns the colour of the fill of `id`.
fn fill_colour(scene: &Scene, id: NodeId) -> Rgba {
    fill_leaves(scene, id)
        .first()
        .and_then(|l| l.path()?.fill)
        .map(|f| f.colour)
        .expect("the series is filled")
}

/// Returns the colour of the stroke of the line of `id`.
fn line_colour(scene: &Scene, id: NodeId) -> Rgba {
    from_source(&leaves(scene), id)
        .iter()
        .find_map(|l| l.path().and_then(|p| p.stroke.as_ref()).map(|s| s.colour))
        .expect("the line is stroked")
}

/// Asserts that every filled path of `id` is built from straight segments only: no curve segment, so that no edge
/// of the fill follows an arc between two data points.
#[track_caller]
fn assert_straight(scene: &Scene, id: NodeId) {
    for leaf in fill_leaves(scene, id) {
        let path = leaf.path().expect("a fill leaf is a path");
        assert!(
            path.segments.iter().all(|s| matches!(
                s,
                PathSegment::MoveTo(_) | PathSegment::LineTo(_) | PathSegment::Close
            )),
            "a series fill has straight edges only: {:?}",
            path.segments
        );
    }
}

/// Removes vertices that repeat the vertex before them, so that a polygon whose two base points coincide (both at the
/// centre of a polar plot) compares equal whether or not the compiler emits the repeated point.
fn without_repeats(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    for &p in points {
        if out.last().is_none_or(|&q| !points_close(p, q, TOL)) {
            out.push(p);
        }
    }
    out
}

/// Asserts that a filled polygon has exactly the vertices `expected`, in order, starting anywhere and in either
/// direction (the non-zero rule fills a polygon alike whichever way round it runs).
#[track_caller]
fn assert_polygon(actual: &[Point], expected: &[Point], what: &str) {
    let actual = without_repeats(actual);
    let expected = without_repeats(expected);
    let n = expected.len();
    let matches = |candidate: &[Point]| {
        (0..n)
            .any(|shift| (0..n).all(|k| points_close(candidate[(k + shift) % n], expected[k], TOL)))
    };
    let reversed: Vec<Point> = actual.iter().rev().copied().collect();
    assert!(
        actual.len() == n && (matches(&actual) || matches(&reversed)),
        "{what}: filled {actual:?}, expected {expected:?}"
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

/// The page point of the data point (x, y) in Cartesian axes with manual limits `[x0, x1]` and `[y0, y1]` drawn in
/// the plot rectangle `plot`: x runs left to right and y bottom to top.
fn cartesian(plot: Rect, (x0, x1): (f64, f64), (y0, y1): (f64, f64), x: f64, y: f64) -> Point {
    Point::new(
        plot.x + (x - x0) / (x1 - x0) * plot.width,
        plot.bottom() - (y - y0) / (y1 - y0) * plot.height,
    )
}

/// The centre and rim radius of the full polar circle of an axes, from its plot rectangle.
fn circle(scene: &Scene, ax: NodeId) -> (Point, f64) {
    let plot = axes_hit(scene, ax).plot_rect;
    (centre(plot), plot.width.min(plot.height) / 2.0)
}

/// The page point at `deg` degrees anticlockwise from east and `rho` points from `centre`; figure space grows
/// downwards, so north has the smaller y.
fn at(centre: Point, deg: f64, rho: f64) -> Point {
    let phi = deg.to_radians();
    Point::new(centre.x + rho * phi.cos(), centre.y - rho * phi.sin())
}

/// Adds Cartesian axes with manual limits `x` and `y` in the one tile of a figure.
fn cartesian_axes(fx: &mut Fx, x: (f64, f64), y: (f64, f64)) -> NodeId {
    let ax = fx.axes2d(0, 0);
    let axes = fx.ax(ax);
    axes.x.limits = Limits::Manual { min: x.0, max: x.1 };
    axes.y.limits = Limits::Manual { min: y.0, max: y.1 };
    ax
}

/// Adds polar axes with the default view and radial limits `r` in the one tile of a figure.
fn polar_axes(fx: &mut Fx, r: (f64, f64)) -> NodeId {
    let ax = fx.axes_in(
        cell(0, 0, 1, 1),
        Projection::Polar {
            polar: PolarView::default(),
        },
    );
    fx.ax(ax).y.limits = Limits::Manual { min: r.0, max: r.1 };
    ax
}

/// Gives a series a fill of the given colour down to `base`.
fn fill_to(base: f64, colour: ColourSpec) -> impl FnOnce(&mut Series) {
    move |s| {
        s.base = Some(base);
        s.fill = colour;
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The filled polygon
// ---------------------------------------------------------------------------------------------------------------

// Why: the area under a line is read as the area between the data and the base, so the polygon must be exactly the
// line's own points closed down to the base at its two ends: any extra vertex would invent data between the points,
// and closing it anywhere but straight down at the first and last x would add or remove area. Here the run
// (0, 2), (1, 5), (3, 1), (4, 3) with a base of −1 is filled as those four points followed by (4, −1) and (0, −1).
#[test]
fn in_cartesian_axes_the_fill_is_the_run_closed_straight_down_to_the_base_at_its_ends() {
    let (xl, yl) = ((0.0, 4.0), (-2.0, 6.0));
    let mut fx = Fx::new();
    let ax = cartesian_axes(&mut fx, xl, yl);
    let x = [0.0, 1.0, 3.0, 4.0];
    let y = [2.0, 5.0, 1.0, 3.0];
    let id = fx.line(ax, &x, &y, None, fill_to(-1.0, ColourSpec::Auto));
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let plot = axes_hit(&scene, ax).plot_rect;
    let p = |x, y| cartesian(plot, xl, yl, x, y);
    let polygons = fill_polygons(&scene, id);
    assert_eq!(polygons.len(), 1, "one run, one polygon: {polygons:?}");
    assert_polygon(
        &polygons[0],
        &[
            p(0.0, 2.0),
            p(1.0, 5.0),
            p(3.0, 1.0),
            p(4.0, 3.0),
            p(4.0, -1.0),
            p(0.0, -1.0),
        ],
        "the run closed to a base of −1",
    );
    assert_straight(&scene, id);
}

// Why: a radar chart is a series in polar axes filled down to the centre; the governing rule says a series joins its
// points with straight chords, so the filled outline must be the polygon of the mapped points, closed to the base at
// the centre where both base points coincide. Following the axes between the points would make a radar polygon bulge
// into arcs the data does not contain.
#[test]
fn in_polar_axes_a_base_at_the_lower_radial_limit_closes_the_fill_at_the_centre_with_straight_chords()
 {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, (0.0, 2.0));
    let id = fx.line(
        ax,
        &[0.0, 90.0, 200.0],
        &[1.0, 2.0, 1.5],
        None,
        fill_to(0.0, ColourSpec::Auto),
    );
    let scene = compile_figure(&fx.build());
    let (c, rim) = circle(&scene, ax);
    let polygons = fill_polygons(&scene, id);
    assert_eq!(polygons.len(), 1, "{polygons:?}");
    assert_polygon(
        &polygons[0],
        &[
            at(c, 0.0, rim / 2.0),
            at(c, 90.0, rim),
            at(c, 200.0, 0.75 * rim),
            c,
            c,
        ],
        "the three points closed at the centre",
    );
    assert_straight(&scene, id);
}

// Why: with a base above the lower radial limit the two base points lie on the circle of the base, at the first and
// last angles; the fill is a direct artist, so they are joined by one straight chord, not by an arc along that circle.
// The polygon therefore has exactly the three data vertices and the two base vertices, and no others.
#[test]
fn in_polar_axes_a_base_above_the_centre_is_joined_by_one_straight_chord() {
    let mut fx = Fx::new();
    let ax = polar_axes(&mut fx, (0.0, 2.0));
    let id = fx.line(
        ax,
        &[0.0, 90.0, 200.0],
        &[1.0, 2.0, 1.5],
        None,
        fill_to(0.5, ColourSpec::Auto),
    );
    let scene = compile_figure(&fx.build());
    let (c, rim) = circle(&scene, ax);
    let polygons = fill_polygons(&scene, id);
    assert_eq!(polygons.len(), 1, "{polygons:?}");
    assert_polygon(
        &polygons[0],
        &[
            at(c, 0.0, rim / 2.0),
            at(c, 90.0, rim),
            at(c, 200.0, 0.75 * rim),
            at(c, 200.0, rim / 4.0),
            at(c, 0.0, rim / 4.0),
        ],
        "the three points closed to a base of 0.5 by a chord",
    );
    assert_straight(&scene, id);
}

// Why: a missing value breaks the line, and the fill must break with it: filling across the gap would show an area
// where the data has none. Each unbroken run is closed to the base on its own, as each piece of broken stairs is.
#[test]
fn each_run_of_a_line_broken_by_a_missing_value_is_filled_as_its_own_polygon() {
    let (xl, yl) = ((0.0, 4.0), (0.0, 4.0));
    let mut fx = Fx::new();
    let ax = cartesian_axes(&mut fx, xl, yl);
    let id = fx.line(
        ax,
        &[0.0, 1.0, 2.0, 3.0, 4.0],
        &[1.0, 2.0, f64::NAN, 3.0, 2.0],
        None,
        fill_to(0.5, ColourSpec::Auto),
    );
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let p = |x, y| cartesian(plot, xl, yl, x, y);
    let mut polygons = fill_polygons(&scene, id);
    polygons.sort_by(|a, b| {
        let left = |s: &Vec<Point>| s.iter().map(|q| q.x).fold(f64::INFINITY, f64::min);
        left(a).total_cmp(&left(b))
    });
    assert_eq!(polygons.len(), 2, "two runs, two polygons: {polygons:?}");
    assert_polygon(
        &polygons[0],
        &[p(0.0, 1.0), p(1.0, 2.0), p(1.0, 0.5), p(0.0, 0.5)],
        "the run before the gap",
    );
    assert_polygon(
        &polygons[1],
        &[p(3.0, 3.0), p(4.0, 2.0), p(4.0, 0.5), p(3.0, 0.5)],
        "the run after the gap",
    );
}

// Why: the fill must cover exactly what the line draws, so when a dense line is thinned to the resolution of the plot
// the fill is built from the same thinned points; a fill built from all the points would not match the drawn outline
// and would cost what the thinning saves. Whatever points the line keeps, the fill is those points closed to the base.
#[test]
fn a_decimated_line_is_filled_through_the_points_it_draws() {
    let n = 100_000;
    let (x, y): (Vec<f64>, Vec<f64>) = (0..n)
        .map(|i| (i as f64, 2.0 + (i as f64 / 250.0).sin()))
        .unzip();
    let mut fx = Fx::new();
    let ax = cartesian_axes(&mut fx, (0.0, (n - 1) as f64), (0.0, 4.0));
    let id = fx.line(ax, &x, &y, None, fill_to(0.0, ColourSpec::Auto));
    let scene = compile_figure(&fx.build());
    let line: Vec<Point> = from_source(&leaves(&scene), id)
        .iter()
        .filter(|l| {
            l.path()
                .is_some_and(|p| p.stroke.is_some() && p.fill.is_none())
        })
        .flat_map(Leaf::subpaths)
        .flatten()
        .collect();
    let line = without_repeats(&line);
    assert!(
        line.len() < n / 10,
        "the line is thinned: {} points",
        line.len()
    );
    let polygons = fill_polygons(&scene, id);
    assert_eq!(polygons.len(), 1);
    let (_, ymap) = axis_maps(&scene, ax);
    let base_y = ymap.to_figure(0.0);
    let first = line[0];
    let last = *line.last().unwrap();
    let mut expected = line.clone();
    expected.push(Point::new(last.x, base_y));
    expected.push(Point::new(first.x, base_y));
    assert_polygon(
        &polygons[0],
        &expected,
        "the thinned line closed to the base",
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Paint order, clip and look
// ---------------------------------------------------------------------------------------------------------------

// Why: the fill is the background of its own series: drawn over the line it would hide the line where they meet,
// and over the markers it would tint them. So it is painted before the line and the markers of the same series, and
// inside the same clip, so that it is cut off where the line is.
#[test]
fn the_fill_is_painted_beneath_the_line_and_markers_in_the_same_clip() {
    let mut fx = Fx::new();
    let ax = cartesian_axes(&mut fx, (0.0, 2.0), (0.0, 4.0));
    let id = fx.line(ax, &[0.0, 1.0, 2.0], &[1.0, 3.0, 2.0], None, |s| {
        s.base = Some(0.0);
        s.fill = ColourSpec::Auto;
        s.marker.shape = MarkerShape::Circle;
    });
    let scene = compile_figure(&fx.build());
    let drawn = from_source(&leaves(&scene), id);
    let fill = drawn
        .iter()
        .position(|l| l.path().is_some_and(|p| p.fill.is_some()))
        .expect("a fill is drawn");
    let line = drawn
        .iter()
        .position(|l| {
            l.path()
                .is_some_and(|p| p.stroke.is_some() && p.fill.is_none())
        })
        .expect("a line is drawn");
    let markers = drawn
        .iter()
        .position(|l| l.markers().is_some())
        .expect("markers are drawn");
    assert!(
        fill < line && fill < markers,
        "the fill ({fill}) is painted before the line ({line}) and markers ({markers})"
    );
    assert_eq!(
        drawn[fill].clip, drawn[line].clip,
        "the fill and the line share a clip"
    );
}

// Why: an automatic fill takes the colour of its series, so that the filled area and its outline read as one series
// without the author choosing a colour twice; the line keeps that colour too.
#[test]
fn an_automatic_fill_takes_the_colour_of_the_series() {
    let mut fx = Fx::new();
    let ax = cartesian_axes(&mut fx, (0.0, 2.0), (0.0, 4.0));
    let id = fx.line(
        ax,
        &[0.0, 1.0, 2.0],
        &[1.0, 3.0, 2.0],
        None,
        fill_to(0.0, ColourSpec::Auto),
    );
    let scene = compile_figure(&fx.build());
    assert_colour(
        fill_colour(&scene, id),
        COLOUR_ORDER[0],
        "the automatic fill",
    );
    assert_colour(line_colour(&scene, id), COLOUR_ORDER[0], "the line");
}

// Why: a radar chart overlays several filled outlines, which stay readable only when each fill is translucent; the
// fill opacity multiplies the alpha of the fill colour (0.8 × 0.5 = 0.4) and leaves the line opaque.
#[test]
fn the_fill_opacity_multiplies_the_alpha_of_the_fill_and_not_of_the_line() {
    let mut fx = Fx::new();
    let ax = cartesian_axes(&mut fx, (0.0, 2.0), (0.0, 4.0));
    let id = fx.line(ax, &[0.0, 1.0, 2.0], &[1.0, 3.0, 2.0], None, |s| {
        s.base = Some(0.0);
        s.fill = ColourSpec::Rgba {
            colour: Colour::rgba(0.0, 0.0, 1.0, 0.8),
        };
        s.fill_opacity = 0.5;
        s.line.colour = ColourSpec::Rgba { colour: RED };
    });
    let scene = compile_figure(&fx.build());
    assert_close(f64::from(fill_colour(&scene, id).a), 0.4, 1e-6);
    assert_close(f64::from(line_colour(&scene, id).a), 1.0, 1e-6);
}

// Why: a base alone, without a fill colour, is a property waiting for a fill; it must draw nothing more than the line,
// or choosing a base would paint an area the author never asked for.
#[test]
fn a_base_without_a_fill_draws_no_fill() {
    let mut fx = Fx::new();
    let ax = cartesian_axes(&mut fx, (0.0, 2.0), (0.0, 4.0));
    let id = fx.line(ax, &[0.0, 1.0, 2.0], &[1.0, 3.0, 2.0], None, |s| {
        s.base = Some(0.0)
    });
    let scene = compile_figure(&fx.build());
    assert!(fill_polygons(&scene, id).is_empty());
}

// Why: a logarithmic y axis has no place for a base of 0, yet an area under a curve spanning decades is commonly
// drawn on one; as for bars and stairs, the fill then drops to the low edge of the plot area, so that it stays a
// closed area under the line.
#[test]
fn on_a_logarithmic_y_axis_a_base_that_cannot_be_placed_drops_to_the_low_edge() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.ax(ax).y.scale = Scale::Log;
    let id = fx.line(
        ax,
        &[0.0, 1.0, 2.0],
        &[20.0, 300.0, 50.0],
        None,
        fill_to(0.0, ColourSpec::Auto),
    );
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (xmap, ymap) = axis_maps(&scene, ax);
    let polygons = fill_polygons(&scene, id);
    assert_eq!(polygons.len(), 1, "{polygons:?}");
    let expected = [
        Point::new(xmap.to_figure(0.0), ymap.to_figure(20.0)),
        Point::new(xmap.to_figure(1.0), ymap.to_figure(300.0)),
        Point::new(xmap.to_figure(2.0), ymap.to_figure(50.0)),
        Point::new(xmap.to_figure(2.0), plot.bottom()),
        Point::new(xmap.to_figure(0.0), plot.bottom()),
    ];
    assert_polygon(&polygons[0], &expected, "the fill down to the low edge");
}

// Why: the base is a value of y, so it is read in the coordinates y is read in: with y given as fractions of the plot
// area, a base of 0 is the bottom edge of the plot whatever the limits of the y axis, and a base of 0.5 its middle.
#[test]
fn the_base_is_read_in_the_coordinates_of_y() {
    let mut fx = Fx::new();
    let ax = cartesian_axes(&mut fx, (0.0, 2.0), (10.0, 20.0));
    let id = fx.line(ax, &[0.0, 1.0, 2.0], &[0.25, 0.75, 0.5], None, |s| {
        s.y_coords = Coords::AxesFraction;
        s.base = Some(0.0);
        s.fill = ColourSpec::Auto;
    });
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let p = |x: f64, f: f64| {
        Point::new(
            plot.x + x / 2.0 * plot.width,
            plot.bottom() - f * plot.height,
        )
    };
    let polygons = fill_polygons(&scene, id);
    assert_eq!(polygons.len(), 1, "{polygons:?}");
    assert_polygon(
        &polygons[0],
        &[
            p(0.0, 0.25),
            p(1.0, 0.75),
            p(2.0, 0.5),
            p(2.0, 0.0),
            p(0.0, 0.0),
        ],
        "the fill down to the bottom of the plot area",
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Legend
// ---------------------------------------------------------------------------------------------------------------

/// An axes above a legend panel that lists the axes' entries, holding what `add` adds. Returns the compiled scene and
/// the panel.
fn keyed(add: impl FnOnce(&mut Fx, NodeId)) -> (Scene, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    let ax = fx.axes2d(0, 0);
    let panel = fx.panel(cell(1, 0, 1, 1));
    fx.legend_in(ax, panel);
    add(&mut fx, ax);
    (compile_figure(&fx.build()), panel)
}

/// Returns the items of the legend `source` drawn for the sample beside the label `label`: those that end left of the
/// label, start no further left than four label heights from it, and whose vertical centre lies within the height of
/// the label.
fn sample_beside(scene: &Scene, source: NodeId, label: &str) -> Vec<Leaf> {
    let all = leaves(scene);
    assert_eq!(
        runs_with_text(&all, label).len(),
        1,
        "the label {label:?} is drawn once"
    );
    let label = text_bbox(&all, label).expect("the label has a box");
    from_source(&all, source)
        .into_iter()
        .filter(|leaf| leaf.glyphs().is_none())
        .filter(|leaf| {
            leaf.visible_bbox().is_some_and(|b| {
                let cy = b.y + b.height / 2.0;
                b.right() <= label.x + 1e-6
                    && b.x >= label.x - 4.0 * label.height.max(9.0)
                    && cy >= label.y
                    && cy <= label.bottom()
            })
        })
        .collect()
}

// Why: a key is useful only if it looks like what it names: a filled series is an area under a line, so its sample is
// a swatch of the fill with the line sample over it, as filled stairs are keyed; keyed by a line alone it could not be
// told from the same series unfilled. An unfilled series keeps its plain line sample, with nothing filled.
#[test]
fn a_filled_series_is_keyed_by_a_swatch_of_its_fill_behind_its_line() {
    let (scene, panel) = keyed(|fx, ax| {
        fx.line(ax, &[0.0, 1.0], &[1.0, 2.0], None, |s| {
            s.label = text("Filled");
            s.base = Some(0.0);
            s.fill = ColourSpec::Rgba { colour: BLUE };
            s.line.colour = ColourSpec::Rgba { colour: RED };
        });
        fx.line(ax, &[0.0, 1.0], &[2.0, 3.0], None, |s| {
            s.label = text("Open");
            s.line.colour = ColourSpec::Rgba { colour: RED };
        });
    });
    let sample = sample_beside(&scene, panel, "Filled");
    let swatch = sample
        .iter()
        .position(|l| {
            l.path()
                .and_then(|p| p.fill)
                .is_some_and(|f| rgb8(f.colour) == rgb8(rgba(BLUE)))
        })
        .unwrap_or_else(|| panic!("a swatch in the fill colour: {sample:?}"));
    let line = sample
        .iter()
        .position(|l| {
            l.path()
                .and_then(|p| p.stroke.as_ref())
                .is_some_and(|s| rgb8(s.colour) == rgb8(rgba(RED)))
        })
        .unwrap_or_else(|| panic!("a line in the line colour: {sample:?}"));
    assert!(
        swatch < line,
        "the swatch is painted behind the line: {sample:?}"
    );

    let open = sample_beside(&scene, panel, "Open");
    assert!(
        open.iter()
            .all(|l| l.path().is_none_or(|p| p.fill.is_none())),
        "an unfilled series fills nothing in its sample: {open:?}"
    );
}

// Why: entries of the same name merge only when they look alike (ADR 0018), because the reader takes one entry for
// one series; a filled series and an unfilled one of the same name and line look different in the plot and in their
// samples, so the fill is part of the look: both are listed and the clash of names is reported.
#[test]
fn entries_of_the_same_name_that_differ_only_in_their_fill_are_not_merged() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    let panel = fx.panel(cell(1, 0, 1, 2));
    let red = |s: &mut Series| s.line.colour = ColourSpec::Rgba { colour: RED };
    let a = fx.named_line(left, "Sensor A", red);
    let b = fx.named_line(right, "Sensor A", |s| {
        red(s);
        s.base = Some(0.0);
        s.fill = ColourSpec::Auto;
    });
    fx.legend_in(left, panel);
    fx.legend_in(right, panel);
    let scene = compile_figure(&fx.build());
    let targets: Vec<Vec<(NodeId, NodeId)>> = panel_entries(&scene, panel)
        .into_iter()
        .map(|e| e.targets)
        .collect();
    assert_eq!(targets, vec![vec![(left, a)], vec![(right, b)]]);
    assert!(
        scene.warnings.iter().any(|w| matches!(
            &w.kind,
            SceneWarningKind::LegendNameClash { legend, name }
                if *legend == LegendOwner::Panel(panel) && name == "Sensor A"
        )),
        "the clash of names is reported: {:?}",
        scene.warnings
    );

    // The same fill on both merges them, as any other identical look does.
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    let panel = fx.panel(cell(1, 0, 1, 2));
    let filled = |s: &mut Series| {
        s.line.colour = ColourSpec::Rgba { colour: RED };
        s.line.dash = DashStyle::Solid;
        s.base = Some(0.0);
        s.fill = ColourSpec::Rgba { colour: BLUE };
    };
    let a = fx.named_line(left, "Sensor A", filled);
    let b = fx.named_line(right, "Sensor A", filled);
    fx.legend_in(left, panel);
    fx.legend_in(right, panel);
    let scene = compile_figure(&fx.build());
    let targets: Vec<Vec<(NodeId, NodeId)>> = panel_entries(&scene, panel)
        .into_iter()
        .map(|e| e.targets)
        .collect();
    assert_eq!(targets, vec![vec![(left, a), (right, b)]]);
}
