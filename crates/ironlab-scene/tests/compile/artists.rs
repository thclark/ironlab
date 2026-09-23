//! Two-dimensional artists: lines, markers, scatter, contour, filled contour and quiver.

use ironlab_ir::{Color, ColorSpec, DashStyle, Levels, Limits, MarkerShape, NodeId, ScatterColor};
use ironlab_scene::Scene;
use ironlab_scene::display::{FillRule, Point, Rgba};
use ironlab_scene::maths::colormap::{VIRIDIS, sample};

use crate::common::{
    COLOUR_ORDER, Fx, compile_figure, linspace, nearest_lut_index, rgb8, rgb8_close,
};
use crate::probe::{
    Leaf, assert_close, axis_maps, from_source, leaves, marker_instances, signed_area,
};

fn stroked(leaves: &[Leaf], id: NodeId) -> Vec<Leaf> {
    from_source(leaves, id)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.stroke.is_some()))
        .collect()
}

fn in_viridis(color: Rgba) -> bool {
    VIRIDIS.iter().any(|c| rgb8_close(*c, rgb8(color)))
}

/// Asserts that each data point has exactly one marker instance of the artist centred on it, whatever order the
/// markers are painted in.
#[track_caller]
fn assert_one_marker_per_point(scene: &Scene, ax: NodeId, artist: NodeId, points: &[(f64, f64)]) {
    let (xmap, ymap) = axis_maps(scene, ax);
    let markers = marker_instances(&leaves(scene), artist);
    assert_eq!(markers.len(), points.len(), "one marker per point");
    let centres: Vec<(f64, f64)> = markers
        .iter()
        .map(|m| (xmap.to_data(m.position.x), ymap.to_data(m.position.y)))
        .collect();
    for (px, py) in points {
        let here = centres
            .iter()
            .filter(|(cx, cy)| (cx - px).abs() < 1e-6 && (cy - py).abs() < 1e-6)
            .count();
        assert_eq!(
            here, 1,
            "markers centred on ({px}, {py}); centres {centres:?}"
        );
    }
}

/// Returns the colour of the first stroke of `id`, as 8-bit sRGB: the stroke of its first stroked path, or, for
/// an artist drawn as markers alone, the edge of its first marker.
fn stroke_rgb8(scene: &Scene, id: NodeId) -> [u8; 3] {
    let leaves = leaves(scene);
    let colour = stroked(&leaves, id)
        .first()
        .map(|l| l.path().unwrap().stroke.as_ref().unwrap().color)
        .or_else(|| marker_instances(&leaves, id).first().and_then(|m| m.edge))
        .unwrap_or_else(|| panic!("artist {id} is stroked"));
    rgb8(colour)
}

// Why: successive series with automatic colour must be told apart. The colour order is the
// colour-blind-safe Okabe–Ito palette without black (which would read as an axis or annotation),
// starting at orange, and it repeats after its seventh colour, as MATLAB cycles its colour order.
#[test]
fn automatic_line_colours_follow_the_okabe_ito_order_and_cycle() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let ids: Vec<NodeId> = (0..8)
        .map(|k| fx.line(ax, &[0.0, 1.0], &[k as f64, k as f64 + 1.0], None, |_| {}))
        .collect();
    let scene = compile_figure(&fx.build());
    for (k, id) in ids.iter().enumerate() {
        let colour = stroke_rgb8(&scene, *id);
        assert!(
            rgb8_close(colour, COLOUR_ORDER[k % 7]),
            "line {k} is {colour:?}, expected {:?}",
            COLOUR_ORDER[k % 7]
        );
    }
}

// Why: clicking a legend entry hides an artist. If the hidden artist gave up its place in the colour
// order, every later series would change colour and no longer match what the reader saw a moment
// ago, so a hidden artist keeps its colour.
#[test]
fn hidden_artist_keeps_its_place_in_the_colour_order() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |l| l.visible = false);
    let second = fx.line(ax, &[0.0, 1.0], &[1.0, 2.0], None, |_| {});
    let third = fx.line(ax, &[0.0, 1.0], &[2.0, 3.0], None, |_| {});
    let scene = compile_figure(&fx.build());
    assert!(rgb8_close(stroke_rgb8(&scene, second), COLOUR_ORDER[1]));
    assert!(rgb8_close(stroke_rgb8(&scene, third), COLOUR_ORDER[2]));
}

// Why: only series that ask for an automatic colour advance the colour order. An explicitly coloured
// line or a colormapped contour must not use up a colour, lines, scatters and quivers share one
// sequence, and each axes starts its own sequence at the first colour, as in MATLAB.
#[test]
fn only_automatic_line_scatter_and_quiver_colours_advance_the_order() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let ax = fx.axes2d(0, 0);
    let red = Color::rgb(1.0, 0.0, 0.0);
    let explicit = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |l| {
        l.line.color = ColorSpec::Rgba { color: red }
    });
    let g = linspace(0.0, 1.0, 5);
    fx.contour(ax, &g, &g, |x, y| x + y, |_| {});
    let scatter = fx.scatter(ax, &[0.2, 0.8], &[0.5, 0.5], None, |_, _| {});
    let quiver = fx.quiver(ax, &[0.5], &[0.5], None, &[0.1], &[0.1], None);
    let line = fx.line(ax, &[0.0, 1.0], &[1.0, 0.0], None, |_| {});
    let other_axes = fx.axes2d(0, 1);
    let first_in_other = fx.line(other_axes, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let scene = compile_figure(&fx.build());
    assert_eq!(stroke_rgb8(&scene, explicit), [255, 0, 0]);
    let checks = [
        ("scatter", scatter, 0),
        ("quiver", quiver, 1),
        ("line", line, 2),
        ("first line of the second axes", first_in_other, 0),
    ];
    for (name, id, k) in checks {
        let colour = stroke_rgb8(&scene, id);
        assert!(
            rgb8_close(colour, COLOUR_ORDER[k]),
            "{name} is {colour:?}, expected {:?}",
            COLOUR_ORDER[k]
        );
    }
}

// Why: in 2D, a later artist is drawn over an earlier one, as in MATLAB, so users control which
// series stays readable where they overlap by the order in which they plot them.
#[test]
fn artists_are_painted_in_the_order_the_axes_lists_them() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let first = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let second = fx.line(ax, &[0.0, 1.0], &[1.0, 0.0], None, |_| {});
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let positions = |id| -> Vec<usize> {
        (0..leaves.len())
            .filter(|i| leaves[*i].source == Some(id))
            .collect()
    };
    let (a, b) = (positions(first), positions(second));
    assert!(!a.is_empty() && !b.is_empty());
    assert!(
        a.iter().max() < b.iter().min(),
        "first line at {a:?} is painted before second line at {b:?}"
    );
}

// Why: the dash style must reach the display list as a dash array, since both backends dash natively
// from it; a solid line must not carry one.
#[test]
fn dash_style_becomes_a_dash_array() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let solid = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let dashed = fx.line(ax, &[0.0, 1.0], &[1.0, 0.0], None, |l| {
        l.line.dash = DashStyle::Dashed
    });
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let dash = |id| {
        stroked(&leaves, id)[0]
            .path()
            .unwrap()
            .stroke
            .clone()
            .unwrap()
            .dash
    };
    assert!(dash(solid).is_empty());
    let d = dash(dashed);
    assert!(!d.is_empty() && d.iter().sum::<f64>() > 0.0, "{d:?}");
}

// Why: `DashStyle::None` with a marker is MATLAB's markers-only plot; it must draw one marker at each
// finite point and no connecting polyline.
#[test]
fn markers_only_line_draws_one_marker_per_finite_point() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let line = fx.line(
        ax,
        &[0.0, 1.0, 2.0, 3.0, 4.0],
        &[0.0, 1.0, f64::NAN, 3.0, 4.0],
        None,
        |l| {
            l.line.dash = DashStyle::None;
            l.marker.shape = MarkerShape::Circle;
            l.marker.size_pt = 6.0;
        },
    );
    let scene = compile_figure(&fx.build());
    let items = from_source(&leaves(&scene), line);
    assert!(
        !items.is_empty() && items.iter().all(|item| item.markers().is_some()),
        "markers and no polyline: {} of the line's {} items are paths",
        items.iter().filter(|item| item.path().is_some()).count(),
        items.len()
    );
    let finite = [(0.0, 0.0), (1.0, 1.0), (3.0, 3.0), (4.0, 4.0)];
    assert_one_marker_per_point(&scene, ax, line, &finite);
}

// Why: NaN marks missing data, so the polyline must break there instead of bridging the gap.
#[test]
fn nan_breaks_the_polyline_into_subpaths() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let x = linspace(0.0, 6.0, 7);
    let line = fx.line(
        ax,
        &x,
        &[0.0, 1.0, 2.0, f64::NAN, 4.0, 5.0, 6.0],
        None,
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    let strokes = stroked(&leaves(&scene), line);
    let moves: usize = strokes.iter().map(Leaf::move_to_count).sum();
    assert_eq!(moves, 2);
    let runs: Vec<usize> = strokes
        .iter()
        .flat_map(|l| l.subpaths())
        .map(|s| s.len())
        .collect();
    assert_eq!(
        runs,
        vec![3, 3],
        "the line breaks exactly at the missing point"
    );
}

// Why: a scatter is one marker per point, centred on that point; merging, dropping or misplacing
// markers misrepresents the data.
#[test]
fn scatter_draws_one_marker_per_point() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let x = linspace(0.0, 1.0, 7);
    let y: Vec<f64> = x.iter().map(|v| 1.0 - v * v).collect();
    let scatter = fx.scatter(ax, &x, &y, None, |_, _| {});
    let scene = compile_figure(&fx.build());
    let points: Vec<(f64, f64)> = x.iter().copied().zip(y.iter().copied()).collect();
    assert_one_marker_per_point(&scene, ax, scatter, &points);
}

// Why: scatter colour data must be mapped through the axes colormap and automatic colour limits, so
// the lowest value takes the first colour and the highest the last.
#[test]
fn scatter_colour_data_is_colormapped() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scatter = fx.scatter(ax, &[0.0, 1.0, 2.0], &[0.0, 0.0, 0.0], None, |s, fx| {
        s.color = ScatterColor::Data {
            data: fx.vector(&[10.0, 15.0, 20.0]),
        };
        s.marker.face = ColorSpec::Auto;
    });
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    let markers = marker_instances(&leaves(&scene), scatter);
    assert_eq!(markers.len(), 3);
    for m in markers {
        let data_x = xmap.to_data(m.position.x).round();
        let t = data_x / 2.0;
        let expected = sample(&VIRIDIS, t).unwrap();
        let fill = m.face.expect("filled marker");
        assert!(
            rgb8_close(rgb8(fill), expected),
            "marker at x = {data_x}: {fill:?} vs {expected:?}"
        );
    }
}

// Why: each isoline must lie on its level and take that level's colormap colour, with higher levels
// further along the colormap, so a reader can tell high ground from low ground.
#[test]
fn contour_lines_lie_on_their_levels_with_colormapped_colours() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    // Ten nodes per axis keep every level off the grid nodes.
    let g = linspace(-1.0, 1.0, 10);
    let levels = [-0.5, 0.0, 0.5];
    let contour = fx.contour(
        ax,
        &g,
        &g,
        |x, _| x,
        |c| {
            c.levels = Levels::Explicit {
                values: levels.to_vec(),
            }
        },
    );
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    let mut colour_of_level: [Option<Rgba>; 3] = [None; 3];
    for leaf in stroked(&leaves(&scene), contour) {
        let xs: Vec<f64> = leaf
            .subpaths()
            .into_iter()
            .flatten()
            .map(|p| xmap.to_data(p.x))
            .collect();
        let k = levels
            .iter()
            .position(|l| (xs[0] - l).abs() < 1e-6)
            .expect("path lies on a level");
        assert!(
            xs.iter().all(|x| (x - levels[k]).abs() < 1e-6),
            "a path holds one level"
        );
        let colour = leaf.path().unwrap().stroke.as_ref().unwrap().color;
        assert!(in_viridis(colour), "{colour:?} comes from the colormap");
        colour_of_level[k] = Some(colour);
    }
    let [lo, mid, hi] =
        colour_of_level.map(|c| nearest_lut_index(&VIRIDIS, c.expect("every level is drawn")));
    assert!(
        lo < mid && mid < hi,
        "colormap indices increase with level: {lo}, {mid}, {hi}"
    );
}

// Why: filled contours paint one nonzero-rule path per band. The pieces of a band share one winding,
// so they union without holes; each band holds only data in its own level range and is coloured
// further along the colormap than the band below it; and together the bands cover the data domain
// exactly, without gaps or overlaps.
#[test]
fn filled_contour_bands_tile_the_data_domain() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let g = linspace(-1.0, 1.0, 21);
    let field = |x: f64, y: f64| x * x + y * y;
    let contour = fx.contour(ax, &g, &g, field, |c| {
        c.fill = true;
        c.levels = Levels::Explicit {
            values: vec![0.5, 1.0, 1.5],
        };
    });
    let scene = compile_figure(&fx.build());
    let (xmap, ymap) = axis_maps(&scene, ax);
    let fills: Vec<Leaf> = from_source(&leaves(&scene), contour)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.fill.is_some()))
        .collect();
    assert_eq!(fills.len(), 4, "one path per band for levels 0.5, 1, 1.5");
    // The bands are [−∞, 0.5), [0.5, 1), [1, 1.5) and [1.5, ∞). The vertex mean of a piece lies inside
    // the piece, where the field differs from its linear interpolant by far less than the tolerance.
    let bands = [
        (f64::NEG_INFINITY, 0.5),
        (0.5, 1.0),
        (1.0, 1.5),
        (1.5, f64::INFINITY),
    ];
    let tol = 0.05;
    let mut total = 0.0;
    let mut band_colour_index = [None; 4];
    for leaf in &fills {
        let fill = leaf.path().unwrap().fill.unwrap();
        assert_eq!(fill.rule, FillRule::NonZero);
        let pieces = leaf.subpaths();
        let areas: Vec<f64> = pieces.iter().map(|s| signed_area(s)).collect();
        let positive = areas.iter().filter(|a| **a > 1e-9).count();
        let negative = areas.iter().filter(|a| **a < -1e-9).count();
        assert!(
            positive == 0 || negative == 0,
            "all pieces of a band share one winding"
        );
        total += areas.iter().sum::<f64>().abs();
        let values: Vec<f64> = pieces
            .iter()
            .map(|piece| {
                let n = piece.len() as f64;
                let x = piece.iter().map(|p| xmap.to_data(p.x)).sum::<f64>() / n;
                let y = piece.iter().map(|p| ymap.to_data(p.y)).sum::<f64>() / n;
                field(x, y)
            })
            .collect();
        let band = bands
            .iter()
            .position(|(lo, hi)| values.iter().all(|v| *v >= lo - tol && *v <= hi + tol))
            .unwrap_or_else(|| panic!("band path holds values of one band: {values:?}"));
        assert!(
            band_colour_index[band].is_none(),
            "band {band} is painted once"
        );
        band_colour_index[band] = Some(nearest_lut_index(&VIRIDIS, fill.color));
    }
    let indices = band_colour_index.map(|i| i.expect("every band is painted"));
    assert!(
        indices.windows(2).all(|w| w[0] < w[1]),
        "colormap indices increase from the lowest band to the highest: {indices:?}"
    );
    let domain = (xmap.to_figure(1.0) - xmap.to_figure(-1.0)).abs()
        * (ymap.to_figure(1.0) - ymap.to_figure(-1.0)).abs();
    assert_close(total / domain, 1.0, 1e-6);
}

// Why: a quiver draws exactly one arrow per vector with finite components, based at its position and
// pointing along the vector; missing vectors are skipped rather than drawn at the origin, and
// automatic scaling keeps each arrow shorter than the spacing between bases so arrows do not
// collide.
#[test]
fn quiver_draws_one_arrow_per_finite_vector() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for j in 0..3 {
        for i in 0..3 {
            x.push(i as f64);
            y.push(j as f64);
        }
    }
    let mut u = vec![1.0; 9];
    u[4] = f64::NAN;
    let v = vec![0.5; 9];
    let quiver = fx.quiver(ax, &x, &y, None, &u, &v, None);
    fx.ax(ax).x.limits = Limits::Manual {
        min: -1.0,
        max: 3.0,
    };
    fx.ax(ax).y.limits = Limits::Manual {
        min: -1.0,
        max: 3.0,
    };
    let scene = compile_figure(&fx.build());
    let (xmap, ymap) = axis_maps(&scene, ax);
    let arrows = from_source(&leaves(&scene), quiver);
    assert_eq!(arrows.len(), 8, "one arrow item per finite vector");
    let figure_vertices: Vec<Vec<Point>> = arrows
        .iter()
        .map(|a| a.subpaths().into_iter().flatten().collect())
        .collect();
    for (i, (bx, by)) in x.iter().zip(&y).enumerate() {
        let base = Point::new(xmap.to_figure(*bx), ymap.to_figure(*by));
        let based_here: Vec<&Vec<Point>> = figure_vertices
            .iter()
            .filter(|vs| {
                vs.iter()
                    .any(|p| (p.x - base.x).abs() < 1e-6 && (p.y - base.y).abs() < 1e-6)
            })
            .collect();
        if i == 4 {
            assert!(based_here.is_empty(), "no arrow for the missing vector");
            continue;
        }
        assert_eq!(based_here.len(), 1, "one arrow based at ({bx}, {by})");
        // The tip is the vertex farthest from the base: the barbs end a head length back from it.
        let tip = based_here[0]
            .iter()
            .max_by(|p, q| {
                let d = |r: &Point| (r.x - base.x).hypot(r.y - base.y);
                d(p).total_cmp(&d(q))
            })
            .unwrap();
        let (dx, dy) = (xmap.to_data(tip.x) - bx, ymap.to_data(tip.y) - by);
        let length = dx.hypot(dy);
        assert!(
            length > 0.0 && length < 1.0,
            "arrow of length {length} is visible and shorter than the base spacing of 1"
        );
        assert_close(dx / length, 1.0 / 1.25f64.sqrt(), 1e-6);
        assert_close(dy / length, 0.5 / 1.25f64.sqrt(), 1e-6);
    }
}
