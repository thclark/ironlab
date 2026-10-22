//! Series (#19, #44): the one artist for points, drawn as a line, as markers, or as both, and coloured by data along
//! its line as well as at its markers.
//!
//! Expected colours are worked out here from a listed colormap of two or three colours between manual limits of 0 and
//! 1, which the scale interpolates linearly in sRGB, so that the colour of a value `v` through red–blue is
//! `(1 − v, 0, v)` without reference to the compiler's own mapping. These tests pin the following rules, which the
//! plan leaves to the implementation:
//!
//! - **Gradient pieces.** A colour-mapped line with one colour value per point strokes each segment as a run of
//!   pieces, each a straight stroke of one colour, laid end to end from the segment's first point to its second, with
//!   neither gap nor overlap. The colour of a piece is the colour that the straight sRGB interpolation between the
//!   colours of the segment's two points takes somewhere along that piece. The interpolation is of colours, not of
//!   values, so that a gradient between two entries of an indexed or categorical scale does not pass through the
//!   entries between them, and so that the later gradient stroke of #5, an axial shading, draws the same picture.
//! - **Number of pieces.** The number of pieces of a segment depends only on the segment's length on the page, in
//!   points, so that a figure is reproducible and a larger figure is subdivided more finely. A piece is never longer
//!   than [`MAX_PIECE_PT`] unless the segment has [`MAX_PIECES`] pieces, never shorter than [`MIN_PIECE_PT`] unless the
//!   segment is one piece, and no segment has more than [`MAX_PIECES`] pieces. The three values are decided; the
//!   compiler does not export them, so they are stated once, here.
//! - **Dashes.** A dashed colour-mapped line keeps the dash pattern of the same line in one colour: each piece is
//!   stroked with the same dash array, and with a dash offset equal (modulo the pattern) to its distance from the
//!   start of its run, so that the pattern runs on across pieces.
//! - **One value per segment.** Each segment is stroked in the one colour of its value, and the markers take their own
//!   colours. An `Auto` marker face or edge resolves as it would on a series without colour data: to the series' own
//!   colour from the axes colour order, the colour its line would have had were it not colour-mapped. Such a series
//!   uses up that colour of the order, so the next automatic series does not share it.
//! - **Markers.** A marker face or edge that is `Colormapped` takes the colour of its point's value. One that is
//!   `Auto` follows the colour of the series, its line colour, so with a colour-mapped line and one value per point it
//!   takes its point's colour too. A fixed colour stays fixed.
//! - **Marker edge width.** Unchanged from the line and scatter that a series replaces: a series that draws a line
//!   strokes its marker edges at its line width clamped to 0.5–1.5 pt, and a series that draws no line (a dash or a
//!   line colour of `None`) strokes them at 0.5 pt, whatever its line width.
//! - **Thinning.** A thinned line joins kept points. With one value per point each joining segment is a gradient
//!   between the colours of its two kept points; with one value per segment it takes the colour of the first source
//!   segment it replaces.
//! - **Unchanged drawing.** A two-dimensional series without colour data is stroked as one path, exactly as the line
//!   it replaces, so that every figure that does not use the new features draws as before.
//! - **Non-finite colour values.** With the default non-finite category of a scale (transparent), a point whose colour
//!   value is not finite draws no marker, and the two segments that meet it are not drawn.
//! - **Depth in 3D.** In a three-dimensional axes each segment is an item of its own, carrying the depths of its two
//!   ends, and each is placed in the painter's order by its own depth, so that a line passing through a surface is
//!   drawn behind it and in front of it where it should be.
//! - **Colour order.** A series whose line colour is automatic takes the next colour of the axes colour order; a
//!   series coloured by data takes none, unless it has one value per segment and an automatic marker face or edge,
//!   which are drawn in the colour it takes.

use ironlab_ir::{
    Color, ColorScaleKind, ColorSpec, Colormap, ContinuousColorScale, DashStyle, Figure,
    FigureSize, Legend, Limits, MarkerShape, NodeId, View3d,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{Depth, Point, Rect, Rgba};
use ironlab_scene::maths::camera::{Camera, fit_to_rect, normalise_box};
use ironlab_scene::maths::decimate::Sample;

use crate::common::{COLOUR_ORDER, Fx, compile_figure, linspace, rgb8, rgb8_close, rgba, text};
use crate::probe::{
    Leaf, assert_close, axes_hit, axes_legend_entries, axis_maps, entry_artist, from_source,
    inside, leaves, marker_instances, points_close,
};

/// The longest a gradient piece may be, in points, unless its segment has [`MAX_PIECES`] pieces: fine enough that a
/// piece is a few device pixels at print resolution, so that the seams between pieces cannot be seen.
const MAX_PIECE_PT: f64 = 1.0;
/// The shortest a gradient piece may be, in points, unless its segment is one piece.
const MIN_PIECE_PT: f64 = 0.25;
/// The most pieces one segment is cut into: no colormap holds more distinguishable colours than this along one
/// segment, and the bound keeps a very long segment from costing thousands of paths.
const MAX_PIECES: usize = 256;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const GREY: Color = Color::rgb(0.5, 0.5, 0.5);

/// The slack allowed in one channel of a colour: two levels of 255, for the rounding of a colormap to a table.
const TOL: f32 = 2.0 / 255.0;

/// Adds a continuous scale whose listed colormap runs linearly through `colors` between manual limits of 0 and 1.
fn listed(fx: &mut Fx, colors: Vec<Color>) -> NodeId {
    fx.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Listed { colors },
        limits: Limits::Manual { min: 0.0, max: 1.0 },
        ..ContinuousColorScale::default()
    }))
}

/// The colour of `v` through the red–blue scale of [`listed`].
fn red_blue(v: f64) -> Rgba {
    lerp(rgba(RED), rgba(BLUE), v)
}

/// The straight sRGB interpolation between two colours.
fn lerp(a: Rgba, b: Rgba, t: f64) -> Rgba {
    let t = t as f32;
    Rgba::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// Returns whether two colours agree within [`TOL`] in every channel.
fn same_colour(a: Rgba, b: Rgba) -> bool {
    [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)]
        .iter()
        .all(|(x, y)| (x - y).abs() <= TOL)
}

/// Returns whether `c` is a colour that the interpolation from `from` to `to` takes for some parameter in
/// `[t0, t1]`, channel by channel within [`TOL`].
fn interpolated_within(c: Rgba, from: Rgba, to: Rgba, t0: f64, t1: f64) -> bool {
    let (p, q) = (lerp(from, to, t0), lerp(from, to, t1));
    [(c.r, p.r, q.r), (c.g, p.g, q.g), (c.b, p.b, q.b)]
        .iter()
        .all(|&(x, a, b)| x >= a.min(b) - TOL && x <= a.max(b) + TOL)
}

/// Gives a 2D axes manual limits along x and y.
fn limits(fx: &mut Fx, ax: NodeId, x: (f64, f64), y: (f64, f64)) {
    let axes = fx.ax(ax);
    axes.x.limits = Limits::Manual { min: x.0, max: x.1 };
    axes.y.limits = Limits::Manual { min: y.0, max: y.1 };
}

/// One straight stroke of a series, as the display list holds it.
#[derive(Clone, Debug)]
struct Piece {
    from: Point,
    to: Point,
    colour: Rgba,
}

/// Every straight stroke that a node drew, in paint order, in figure space.
fn pieces(leaves: &[Leaf], id: NodeId) -> Vec<Piece> {
    let mut out = Vec::new();
    for leaf in leaves {
        if leaf.source != Some(id) {
            continue;
        }
        let Some(path) = leaf.path() else { continue };
        let (Some(stroke), None) = (&path.stroke, &path.fill) else {
            continue;
        };
        for (from, to) in leaf.line_segments() {
            out.push(Piece {
                from,
                to,
                colour: stroke.color,
            });
        }
    }
    out
}

/// The parameter at which `p` lies on the segment from `a` to `b`, or `None` when it lies off the segment.
fn along(p: Point, a: Point, b: Point) -> Option<f64> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length2 = dx * dx + dy * dy;
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / length2;
    let off = ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / length2.sqrt();
    (off <= 1e-6 && (-1e-9..=1.0 + 1e-9).contains(&t)).then_some(t)
}

/// A piece as [`pieces_on`] gives it: the parameters of its two ends along its segment, and its colour.
type Span = (f64, f64, Rgba);

/// The pieces lying on the segment from `a` to `b`, each as the parameters of its two ends, in increasing order, and
/// its colour, sorted along the segment.
fn pieces_on(pieces: &[Piece], a: Point, b: Point) -> Vec<(f64, f64, Rgba)> {
    let mut out: Vec<(f64, f64, Rgba)> = pieces
        .iter()
        .filter_map(|piece| {
            let (t0, t1) = (along(piece.from, a, b)?, along(piece.to, a, b)?);
            Some((t0.min(t1), t0.max(t1), piece.colour))
        })
        .collect();
    out.sort_by(|p, q| p.0.total_cmp(&q.0));
    out
}

/// Asserts that pieces cover their segment from end to end, each starting where the previous one ended.
#[track_caller]
fn assert_covers(on: &[(f64, f64, Rgba)], what: &str) {
    assert!(!on.is_empty(), "{what}: the segment is drawn");
    assert_close(on[0].0, 0.0, 1e-9);
    assert_close(on[on.len() - 1].1, 1.0, 1e-9);
    for pair in on.windows(2) {
        assert!(
            (pair[1].0 - pair[0].1).abs() <= 1e-9,
            "{what}: a piece starts where the previous one ended, not at {} after {}",
            pair[1].0,
            pair[0].1
        );
    }
}

/// The drawn points of an artist, from the hit map.
fn samples_of(scene: &Scene, id: NodeId) -> &[Sample] {
    &scene
        .hit_map
        .artists
        .iter()
        .find(|a| a.artist == id)
        .unwrap_or_else(|| panic!("hit map has no drawn points for artist {id}"))
        .samples
}

/// Maps a data point of a 2D axes to figure space.
fn on_page(scene: &Scene, ax: NodeId) -> impl Fn(f64, f64) -> Point {
    let (xmap, ymap) = axis_maps(scene, ax);
    move |x, y| Point::new(xmap.to_figure(x), ymap.to_figure(y))
}

// ---------------------------------------------------------------------------------
// Gradients along a line
// ---------------------------------------------------------------------------------

/// The x coordinates of the gradient fixture: the second segment is twice as long as the first.
const GRADIENT_X: [f64; 3] = [0.0, 1.0, 3.0];
/// The colour values of the gradient fixture: red, then blue, then red again.
const GRADIENT_C: [f64; 3] = [0.0, 1.0, 0.0];

/// A figure `width_mm` wide holding one axes of x from 0 to 12 and y from 0 to 1, with a series of three points along
/// y = 0.5 at [`GRADIENT_X`], coloured [`GRADIENT_C`] through red–blue along a colour-mapped solid line, without
/// markers. Returns the figure, the axes and the series.
fn gradient_figure(
    width_mm: f64,
    edit: impl FnOnce(&mut ironlab_ir::Series),
) -> (Figure, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.size = FigureSize {
        width_mm,
        height_mm: 100.0,
    };
    let ax = fx.axes2d(0, 0);
    limits(&mut fx, ax, (0.0, 12.0), (0.0, 1.0));
    let scale = listed(&mut fx, vec![RED, BLUE]);
    let id = fx.series(ax, &GRADIENT_X, &[0.5; 3], None, |s, fx| {
        s.color_data = Some(fx.vector(&GRADIENT_C));
        s.color_scale = Some(scale);
        s.line.color = ColorSpec::Colormapped;
        edit(s);
    });
    (fx.build(), ax, id)
}

/// The pieces of each segment of the gradient fixture compiled at `width_mm`, as [`pieces_on`] gives them, with the
/// page length of each segment.
fn gradient_segments(width_mm: f64) -> Vec<(Vec<Span>, f64)> {
    let (fig, ax, id) = gradient_figure(width_mm, |_| {});
    let scene = compile_figure(&fig);
    let page = on_page(&scene, ax);
    let drawn = pieces(&leaves(&scene), id);
    (0..2)
        .map(|k| {
            let (a, b) = (page(GRADIENT_X[k], 0.5), page(GRADIENT_X[k + 1], 0.5));
            (pieces_on(&drawn, a, b), (b.x - a.x).hypot(b.y - a.y))
        })
        .collect()
}

// Why: with one colour value per point each segment is a gradient between its two points' colours (#44), so its pieces must cover it end to end and run from the first point's colour to the second's.
#[test]
fn a_segment_between_two_coloured_points_is_a_gradient_of_pieces_from_one_colour_to_the_other() {
    for (k, (on, _)) in gradient_segments(160.0).iter().enumerate() {
        let what = format!("segment {k}");
        assert_covers(on, &what);
        assert!(on.len() > 1, "{what}: a gradient is more than one piece");
        let (from, to) = (red_blue(GRADIENT_C[k]), red_blue(GRADIENT_C[k + 1]));
        for &(t0, t1, colour) in on {
            assert!(
                interpolated_within(colour, from, to, t0, t1),
                "{what}: the piece over [{t0}, {t1}] is {colour:?}, which the gradient from {from:?} to {to:?} does \
                 not take there"
            );
        }
    }
}

// Why: a gradient between two colours of a scale is drawn between those colours, not through the colours that lie between their values, so that two entries of an indexed or categorical scale blend directly and the later axial-shading stroke of #5 draws the same picture.
#[test]
fn a_gradient_interpolates_between_the_two_colours_rather_than_through_the_values_between_them() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    limits(&mut fx, ax, (0.0, 1.0), (0.0, 1.0));
    let scale = listed(&mut fx, vec![RED, GREEN, BLUE]);
    let id = fx.series(ax, &[0.0, 1.0], &[0.5, 0.5], None, |s, fx| {
        s.color_data = Some(fx.vector(&[0.0, 1.0]));
        s.color_scale = Some(scale);
        s.line.color = ColorSpec::Colormapped;
    });
    let scene = compile_figure(&fx.build());
    let page = on_page(&scene, ax);
    let on = pieces_on(&pieces(&leaves(&scene), id), page(0.0, 0.5), page(1.0, 0.5));
    assert_covers(&on, "the segment");
    for &(t0, t1, colour) in &on {
        assert!(
            colour.g <= TOL,
            "the piece over [{t0}, {t1}] is {colour:?}: the gradient from red to blue passes through purple, not \
             through the green of the value 0.5"
        );
    }
    let middle = on
        .iter()
        .find(|(t0, t1, _)| *t0 <= 0.5 && 0.5 <= *t1)
        .expect("a piece covers the middle of the segment");
    assert!(
        interpolated_within(middle.2, rgba(RED), rgba(BLUE), middle.0, middle.1),
        "the middle piece is purple, not {:?}",
        middle.2
    );
}

// Why: the number of pieces derives only from the figure, so the same figure must compile to exactly the same display list every time, which is what makes a figure reproducible (#44).
#[test]
fn the_same_coloured_series_compiles_to_the_same_display_list_twice() {
    let (fig, _, _) = gradient_figure(160.0, |s| s.marker.shape = MarkerShape::Circle);
    let first = compile_figure(&fig);
    let second = compile_figure(&fig.clone());
    assert_eq!(first.display_list, second.display_list);
    assert_eq!(first, second);
}

// Why: the pieces must be fine enough wherever the segment is long on the page, so a segment that is longer on the page, whether from its data or from a larger figure, must be cut into more pieces.
#[test]
fn a_segment_longer_on_the_page_is_cut_into_more_pieces() {
    let small = gradient_segments(160.0);
    let large = gradient_segments(320.0);
    assert!(
        small[1].1 > small[0].1,
        "the fixture's second segment is the longer"
    );
    assert!(
        small[1].0.len() > small[0].0.len(),
        "the longer segment of one figure has more pieces: {} against {}",
        small[1].0.len(),
        small[0].0.len()
    );
    for k in 0..2 {
        assert!(
            large[k].1 > small[k].1,
            "segment {k} is longer on the larger page"
        );
        assert!(
            large[k].0.len() > small[k].0.len(),
            "segment {k} has more pieces in the larger figure: {} against {}",
            large[k].0.len(),
            small[k].0.len()
        );
    }
}

// Why: subdivision must be fine enough to hide its seams yet bounded in cost, so a piece is never longer than the bound unless capped, never shorter than the floor unless alone, and never one of more than the cap.
#[test]
fn the_pieces_of_a_segment_are_bounded_in_length_and_in_number() {
    // Twenty-one figure widths give forty-two segment lengths, so that a rule cutting pieces of a fixed length and
    // leaving a remainder shorter than the floor at the end of a segment is all but certain to be caught.
    for width_mm in (100..=300).step_by(10) {
        for (k, (on, length)) in gradient_segments(f64::from(width_mm)).iter().enumerate() {
            let what = format!("segment {k} of the figure {width_mm} mm wide");
            assert_covers(on, &what);
            assert!(on.len() <= MAX_PIECES, "{what}: {} pieces", on.len());
            for &(t0, t1, _) in on {
                let piece = (t1 - t0) * length;
                assert!(
                    piece <= MAX_PIECE_PT + 1e-9 || on.len() == MAX_PIECES,
                    "{what}: a piece of {piece} pt is longer than {MAX_PIECE_PT} pt"
                );
                assert!(
                    piece >= MIN_PIECE_PT - 1e-9 || on.len() == 1,
                    "{what}: a piece of {piece} pt is shorter than {MIN_PIECE_PT} pt"
                );
            }
        }
    }

    // A segment a fraction of a point long is one piece.
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    limits(&mut fx, ax, (0.0, 1.0), (0.0, 1.0));
    let scale = listed(&mut fx, vec![RED, BLUE]);
    let short = fx.series(ax, &[0.5, 0.5 + 1e-4], &[0.5, 0.5], None, |s, fx| {
        s.color_data = Some(fx.vector(&[0.0, 1.0]));
        s.color_scale = Some(scale);
        s.line.color = ColorSpec::Colormapped;
    });
    let scene = compile_figure(&fx.build());
    let page = on_page(&scene, ax);
    let (a, b) = (page(0.5, 0.5), page(0.5 + 1e-4, 0.5));
    assert!((b.x - a.x) < MIN_PIECE_PT, "the fixture segment is short");
    let on = pieces_on(&pieces(&leaves(&scene), short), a, b);
    assert_covers(&on, "the short segment");
    assert_eq!(on.len(), 1, "a segment shorter than the floor is one piece");

    // A segment thousands of points long is cut into no more than the cap.
    let (fig, ax, long) = {
        let mut fx = Fx::new();
        fx.fig.size = FigureSize {
            width_mm: 2_000.0,
            height_mm: 100.0,
        };
        let ax = fx.axes2d(0, 0);
        limits(&mut fx, ax, (0.0, 1.0), (0.0, 1.0));
        let scale = listed(&mut fx, vec![RED, BLUE]);
        let id = fx.series(ax, &[0.0, 1.0], &[0.5, 0.5], None, |s, fx| {
            s.color_data = Some(fx.vector(&[0.0, 1.0]));
            s.color_scale = Some(scale);
            s.line.color = ColorSpec::Colormapped;
        });
        (fx.build(), ax, id)
    };
    let scene = compile_figure(&fig);
    let page = on_page(&scene, ax);
    let (a, b) = (page(0.0, 0.5), page(1.0, 0.5));
    assert!(
        (b.x - a.x) > MAX_PIECE_PT * MAX_PIECES as f64,
        "the fixture segment is longer than the cap can cut finely"
    );
    let on = pieces_on(&pieces(&leaves(&scene), long), a, b);
    assert_covers(&on, "the long segment");
    // Pieces of at most a point would need more than the cap, so the segment takes exactly the cap.
    assert_eq!(on.len(), MAX_PIECES, "the long segment is cut into the cap");
}

// Why: a dashed line must look dashed in the same rhythm whether or not it is coloured by data, so each piece must carry the dash array of the uncoloured line and an offset that continues the pattern from the start of its run.
#[test]
fn a_dashed_gradient_continues_its_dash_pattern_across_pieces() {
    let (fig, ax, id) = gradient_figure(160.0, |s| s.line.dash = DashStyle::Dashed);
    let (plain, _, plain_id) = gradient_figure(160.0, |s| {
        s.line.dash = DashStyle::Dashed;
        s.line.color = ColorSpec::Rgba { color: GREY };
    });
    let plain_scene = compile_figure(&plain);
    let plain_leaves = leaves(&plain_scene);
    let dash = from_source(&plain_leaves, plain_id)[0]
        .path()
        .and_then(|p| p.stroke.as_ref())
        .expect("the uncoloured line is stroked")
        .dash
        .clone();
    assert!(!dash.is_empty(), "a dashed line has a dash array");
    let period: f64 = dash.iter().sum();

    let scene = compile_figure(&fig);
    let page = on_page(&scene, ax);
    let points: Vec<Point> = GRADIENT_X.iter().map(|&x| page(x, 0.5)).collect();
    let all = leaves(&scene);
    let mut checked = 0;
    for leaf in from_source(&all, id) {
        let Some(stroke) = leaf.path().and_then(|p| p.stroke.as_ref()) else {
            continue;
        };
        assert_eq!(
            stroke.dash, dash,
            "every piece has the dash array of the uncoloured line"
        );
        let start = leaf.subpaths()[0][0];
        // The distance from the start of the run to the start of this piece, along the polyline.
        let mut distance = None;
        let mut before = 0.0;
        for k in 0..2 {
            let length = (points[k + 1].x - points[k].x).hypot(points[k + 1].y - points[k].y);
            if let Some(t) = along(start, points[k], points[k + 1]) {
                distance = Some(before + t * length);
                break;
            }
            before += length;
        }
        let distance = distance.expect("every piece starts on the line");
        let r = (stroke.dash_offset - distance).rem_euclid(period);
        assert!(
            r <= 1e-6 || period - r <= 1e-6,
            "a piece starting {distance} pt along the run has the dash offset {}, which does not continue the \
             pattern of period {period}",
            stroke.dash_offset
        );
        checked += 1;
    }
    assert!(checked > 2, "the dashed gradient is drawn in pieces");
}

// ---------------------------------------------------------------------------------
// One value per segment, and the colours of markers
// ---------------------------------------------------------------------------------

// Why: with one colour value per segment each segment takes the one colour of its value and no point has a value, so the markers keep their own fixed colours (#44).
#[test]
fn one_value_per_segment_colours_each_segment_in_one_colour_and_leaves_the_markers_fixed() {
    let x = [0.0, 1.0, 2.0, 3.0];
    let y = [0.0, 1.0, 0.0, 1.0];
    let c = [0.0, 1.0, 0.5];
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    limits(&mut fx, ax, (0.0, 3.0), (0.0, 1.0));
    let scale = listed(&mut fx, vec![RED, BLUE]);
    let id = fx.series(ax, &x, &y, None, |s, fx| {
        s.color_data = Some(fx.vector(&c));
        s.color_scale = Some(scale);
        s.line.color = ColorSpec::Colormapped;
        s.marker.shape = MarkerShape::Circle;
        s.marker.edge = ColorSpec::Rgba {
            color: Color::BLACK,
        };
    });
    let scene = compile_figure(&fx.build());
    let page = on_page(&scene, ax);
    let all = leaves(&scene);
    let drawn = pieces(&all, id);
    for k in 0..3 {
        let on = pieces_on(&drawn, page(x[k], y[k]), page(x[k + 1], y[k + 1]));
        assert_covers(&on, &format!("segment {k}"));
        for &(_, _, colour) in &on {
            assert!(
                same_colour(colour, red_blue(c[k])),
                "segment {k} is the colour of its value {}, not {colour:?}",
                c[k]
            );
        }
    }
    let markers = marker_instances(&all, id);
    assert_eq!(markers.len(), 4, "one marker per point");
    for m in &markers {
        assert_eq!(
            m.edge,
            Some(rgba(Color::BLACK)),
            "marker {}",
            m.source_index
        );
        assert_eq!(m.face, None, "marker {}", m.source_index);
    }
}

// Why: a colour applies wherever the style asks for it (#44): a colormapped marker part takes its point's colour, an automatic one follows the colour-mapped line to its point's colour, and a fixed one stays fixed, so a series can have a coloured line with black markers or the reverse.
#[test]
fn each_marker_part_takes_its_points_colour_where_it_is_colormapped_or_follows_a_colormapped_line()
{
    let x = [0.0, 1.0, 2.0];
    let c = [0.0, 0.5, 1.0];
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    limits(&mut fx, ax, (0.0, 2.0), (0.0, 3.0));
    let scale = listed(&mut fx, vec![RED, BLUE]);
    let add = |fx: &mut Fx, y: f64, line: ColorSpec, face: ColorSpec, edge: ColorSpec| {
        fx.series(ax, &x, &[y; 3], None, |s, fx| {
            s.color_data = Some(fx.vector(&c));
            s.color_scale = Some(scale);
            s.line.color = line;
            s.marker.shape = MarkerShape::Circle;
            s.marker.face = face;
            s.marker.edge = edge;
        })
    };
    let black = ColorSpec::Rgba {
        color: Color::BLACK,
    };
    let grey = ColorSpec::Rgba { color: GREY };
    let following = add(
        &mut fx,
        0.5,
        ColorSpec::Colormapped,
        ColorSpec::Colormapped,
        ColorSpec::Auto,
    );
    let black_edged = add(
        &mut fx,
        1.5,
        ColorSpec::Colormapped,
        ColorSpec::Colormapped,
        black,
    );
    let grey_line = add(&mut fx, 2.5, grey, ColorSpec::Colormapped, ColorSpec::Auto);
    let scene = compile_figure(&fx.build());
    let all = leaves(&scene);

    let check =
        |id: NodeId, face: &dyn Fn(usize) -> Rgba, edge: &dyn Fn(usize) -> Rgba, what: &str| {
            let markers = marker_instances(&all, id);
            assert_eq!(markers.len(), 3, "{what}: one marker per point");
            for m in &markers {
                let i = m.source_index;
                let (f, e) = (m.face.expect("a face"), m.edge.expect("an edge"));
                assert!(
                    same_colour(f, face(i)),
                    "{what}: the face of marker {i} is {f:?}"
                );
                assert!(
                    same_colour(e, edge(i)),
                    "{what}: the edge of marker {i} is {e:?}"
                );
            }
        };
    let mapped = |i: usize| red_blue(c[i]);
    check(
        following,
        &mapped,
        &mapped,
        "an automatic edge under a colour-mapped line",
    );
    check(
        black_edged,
        &mapped,
        &|_| rgba(Color::BLACK),
        "a fixed black edge",
    );
    check(
        grey_line,
        &mapped,
        &|_| rgba(GREY),
        "an automatic edge under a grey line",
    );

    let grey_strokes = pieces(&all, grey_line);
    assert!(!grey_strokes.is_empty(), "the grey line is drawn");
    assert!(
        grey_strokes
            .iter()
            .all(|p| same_colour(p.colour, rgba(GREY))),
        "a line of a fixed colour is not coloured by the data of its markers"
    );
    let mapped_strokes = pieces(&all, black_edged);
    assert!(
        mapped_strokes
            .iter()
            .any(|p| !same_colour(p.colour, rgba(Color::BLACK))),
        "black markers leave the colour-mapped line coloured"
    );
}

// Why: a non-finite colour value means "draw nothing here", so the marker at such a point and the two segments that meet it must be left out while the rest of the series is drawn.
#[test]
fn a_non_finite_colour_value_drops_its_marker_and_the_segments_that_meet_it() {
    let x = [0.0, 1.0, 2.0, 3.0, 4.0];
    let c = [0.0, 0.25, f64::NAN, 0.75, 1.0];
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    limits(&mut fx, ax, (0.0, 4.0), (0.0, 1.0));
    let scale = listed(&mut fx, vec![RED, BLUE]);
    let id = fx.series(ax, &x, &[0.5; 5], None, |s, fx| {
        s.color_data = Some(fx.vector(&c));
        s.color_scale = Some(scale);
        s.line.color = ColorSpec::Colormapped;
        s.marker.shape = MarkerShape::Circle;
    });
    let scene = compile_figure(&fx.build());
    let page = on_page(&scene, ax);
    let all = leaves(&scene);
    let drawn = pieces(&all, id);
    for (k, expected) in [(0, true), (1, false), (2, false), (3, true)] {
        let on = pieces_on(&drawn, page(x[k], 0.5), page(x[k + 1], 0.5));
        // A piece of a neighbouring segment touches this one only at its end, so a segment counts as drawn when a
        // piece lies inside it.
        let inside_it = on.iter().any(|(t0, t1, _)| t1 - t0 > 1e-9);
        assert_eq!(inside_it, expected, "segment {k} drawn");
    }
    let indices: Vec<usize> = marker_instances(&all, id)
        .iter()
        .map(|m| m.source_index)
        .collect();
    assert_eq!(
        indices,
        vec![0, 1, 3, 4],
        "no marker is drawn at the point without a colour"
    );
}

// Why: size data gives each marker its own diameter in points, while the marker size stays the size of the legend key, so the two must each be read where they belong (#19).
#[test]
fn size_data_sizes_each_marker_and_the_marker_size_sizes_the_legend_key() {
    let sizes = [2.0, 12.0, 7.0];
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    limits(&mut fx, ax, (-1.0, 3.0), (0.0, 1.0));
    let id = fx.series(ax, &[0.0, 1.0, 2.0], &[0.5; 3], None, |s, fx| {
        s.line.dash = DashStyle::None;
        s.marker.shape = MarkerShape::Circle;
        s.marker.size_pt = 5.0;
        s.size_data = Some(fx.vector(&sizes));
        s.label = text("Sized");
    });
    fx.ax(ax).legend = Some(Legend::default());
    let scene = compile_figure(&fx.build());
    let all = leaves(&scene);
    let markers = marker_instances(&all, id);
    assert_eq!(markers.len(), 3);
    for m in &markers {
        assert_eq!(
            m.size_pt, sizes[m.source_index],
            "marker {}",
            m.source_index
        );
    }
    let entries = axes_legend_entries(&scene, ax);
    assert_eq!(entries.len(), 1);
    let key: Vec<f64> = leaves_in(&all, entries[0].rect)
        .iter()
        .flat_map(Leaf::instances)
        .map(|m| m.size_pt)
        .collect();
    assert_eq!(key, vec![5.0], "the key is one marker of the marker size");
}

// Why: with one colour value per segment no value belongs to a point, so an automatic marker face or edge resolves as it would on a series without colour data: to the series' own colour from the colour order, the colour its line would have had were it not colour-mapped. The series then uses up that colour, or the next automatic series would share the colour of its markers.
#[test]
fn automatic_markers_of_a_series_coloured_per_segment_take_its_colour_from_the_colour_order() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    limits(&mut fx, ax, (0.0, 2.0), (0.0, 3.0));
    let scale = listed(&mut fx, vec![RED, BLUE]);
    let before = fx.series(ax, &[0.0, 2.0], &[0.5, 0.5], None, |_, _| {});
    let x = [0.0, 1.0, 2.0];
    let c = [0.0, 1.0];
    let segmented = fx.series(ax, &x, &[1.5; 3], None, |s, fx| {
        s.color_data = Some(fx.vector(&c));
        s.color_scale = Some(scale);
        s.line.color = ColorSpec::Colormapped;
        s.marker.shape = MarkerShape::Circle;
        s.marker.face = ColorSpec::Auto;
        s.marker.edge = ColorSpec::Auto;
    });
    let after = fx.series(ax, &[0.0, 2.0], &[2.5, 2.5], None, |_, _| {});
    let scene = compile_figure(&fx.build());
    let page = on_page(&scene, ax);
    let all = leaves(&scene);

    let markers = marker_instances(&all, segmented);
    assert_eq!(markers.len(), 3, "one marker per point");
    for m in &markers {
        for (part, colour) in [("face", m.face), ("edge", m.edge)] {
            let colour =
                rgb8(colour.unwrap_or_else(|| panic!("marker {} has a {part}", m.source_index)));
            assert!(
                rgb8_close(colour, COLOUR_ORDER[1]),
                "the automatic {part} of marker {} is the series' colour {:?} from the order, not {colour:?}",
                m.source_index,
                COLOUR_ORDER[1]
            );
        }
    }
    let drawn = pieces(&all, segmented);
    for k in 0..2 {
        let on = pieces_on(&drawn, page(x[k], 1.5), page(x[k + 1], 1.5));
        assert_covers(&on, &format!("segment {k}"));
        assert!(
            on.iter()
                .all(|&(_, _, colour)| same_colour(colour, red_blue(c[k]))),
            "segment {k} keeps the colour of its value"
        );
    }
    let stroke_of = |id: NodeId| {
        let drawn = pieces(&all, id);
        assert!(!drawn.is_empty(), "series {id} draws its line");
        rgb8(drawn[0].colour)
    };
    assert!(
        rgb8_close(stroke_of(before), COLOUR_ORDER[0]),
        "the series before takes the first colour"
    );
    assert!(
        rgb8_close(stroke_of(after), COLOUR_ORDER[2]),
        "the series after takes the third colour, the second being the coloured series' own"
    );
}

// Why: the series must stroke marker edges exactly as the line and the scatter it replaces did, so that existing figures draw as before: following the width of a line it draws, between half a point and one and a half, and half a point when it draws no line, whatever width its unused line style holds.
#[test]
fn marker_edges_follow_the_line_width_only_when_the_series_draws_a_line() {
    let cases: [(&str, DashStyle, Option<ColorSpec>, f64, f64); 6] = [
        ("a hairline", DashStyle::Solid, None, 0.2, 0.5),
        ("a line within the bounds", DashStyle::Solid, None, 1.0, 1.0),
        ("a thick line", DashStyle::Solid, None, 3.0, 1.5),
        (
            "a thick colour-mapped line",
            DashStyle::Solid,
            Some(ColorSpec::Colormapped),
            3.0,
            1.5,
        ),
        ("a dash of none", DashStyle::None, None, 3.0, 0.5),
        (
            "a line colour of none",
            DashStyle::Solid,
            Some(ColorSpec::None),
            3.0,
            0.5,
        ),
    ];
    for (what, dash, line_colour, width_pt, expected) in cases {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let scale = listed(&mut fx, vec![RED, BLUE]);
        let id = fx.series(ax, &[0.0, 1.0], &[0.0, 1.0], None, |s, fx| {
            s.line.dash = dash;
            s.line.width_pt = width_pt;
            s.marker.shape = MarkerShape::Circle;
            s.marker.edge = ColorSpec::Rgba { color: GREY };
            if let Some(colour) = line_colour {
                s.line.color = colour;
                s.color_data = Some(fx.vector(&[0.0, 1.0]));
                s.color_scale = Some(scale);
            }
        });
        let scene = compile_figure(&fx.build());
        let widths: Vec<f64> = from_source(&leaves(&scene), id)
            .iter()
            .filter_map(|l| l.markers().map(|m| m.edge_width))
            .collect();
        assert!(!widths.is_empty(), "{what}: the markers are drawn");
        assert!(
            widths.iter().all(|w| *w == expected),
            "{what} of width {width_pt} pt strokes its marker edges at {expected} pt, not {widths:?}"
        );
    }
}

// Why: a series without colour data must draw exactly as the line it replaces, so that every figure that does not use colour data renders as before: in 2D its line is one stroked path with a subpath per run, joined at its corners, not a stroke per segment.
#[test]
fn a_two_dimensional_series_without_colour_data_is_stroked_as_one_path() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let id = fx.series(
        ax,
        &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        &[0.0, 1.0, 0.0, f64::NAN, 1.0, 0.0, 1.0],
        None,
        |s, _| s.marker.shape = MarkerShape::Circle,
    );
    let scene = compile_figure(&fx.build());
    let strokes: Vec<Leaf> = from_source(&leaves(&scene), id)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.stroke.is_some()))
        .collect();
    assert_eq!(strokes.len(), 1, "the line is one stroked path");
    let runs: Vec<usize> = strokes[0].subpaths().iter().map(Vec::len).collect();
    assert_eq!(runs, vec![3, 3], "one subpath through each run");
}

// Why: empty and single-point data are valid (ADR 0012), and a series of n points has n − 1 segments, so a series of no points or of one point coloured by data must compile without failing, drawing no line and, for one point, its one marker in its colour.
#[test]
fn an_empty_or_single_point_series_coloured_by_data_draws_no_line_and_at_most_its_marker() {
    for (x, c) in [(vec![], vec![]), (vec![0.5], vec![0.25])] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        limits(&mut fx, ax, (0.0, 1.0), (0.0, 1.0));
        let scale = listed(&mut fx, vec![RED, BLUE]);
        let id = fx.series(ax, &x, &x, None, |s, fx| {
            s.color_data = Some(fx.vector(&c));
            s.color_scale = Some(scale);
            s.line.color = ColorSpec::Colormapped;
            s.marker.shape = MarkerShape::Circle;
            s.marker.face = ColorSpec::Colormapped;
        });
        let scene = compile_figure(&fx.build());
        let all = leaves(&scene);
        let what = format!("{} points", x.len());
        assert!(pieces(&all, id).is_empty(), "{what}: no line is drawn");
        let markers = marker_instances(&all, id);
        assert_eq!(markers.len(), x.len(), "{what}: one marker per point");
        for m in &markers {
            let face = m.face.expect("the marker is filled");
            assert!(
                same_colour(face, red_blue(c[m.source_index])),
                "{what}: the marker is the colour of its value, not {face:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------
// Depth in three dimensions
// ---------------------------------------------------------------------------------

const LO: [f64; 3] = [-1.0; 3];
const HI: [f64; 3] = [1.0; 3];

/// Sets manual limits of [−1, 1] on every axis of a 3D axes, so that positions and depths can be rebuilt from the
/// camera.
fn manual_unit_limits(fx: &mut Fx, ax: NodeId) {
    let axes = fx.ax(ax);
    for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
        axis.limits = Limits::Manual {
            min: -1.0,
            max: 1.0,
        };
    }
}

/// The projection of a 3D axes at the default view with limits [`LO`] to [`HI`], rebuilt from the public camera and
/// the documented fit, giving the figure-space position and the depth of a data point.
fn projection(scene: &Scene, ax: NodeId) -> impl Fn([f64; 3]) -> (Point, f64) {
    let plot = axes_hit(scene, ax).plot_rect;
    let (scale, offset) = fit_to_rect(plot.width, plot.height);
    let origin = Point::new(plot.x + offset[0], plot.y + offset[1]);
    move |p: [f64; 3]| {
        let projected = Camera::default().project(normalise_box(p, LO, HI, [false; 3]));
        (
            Point::new(
                origin.x + scale * projected.screen[0],
                origin.y - scale * projected.screen[1],
            ),
            projected.depth,
        )
    }
}

/// The depths of a leaf that carries one per vertex, or a failure naming the leaf.
#[track_caller]
fn vertex_depths(leaf: &Leaf) -> Vec<f64> {
    match leaf.depth() {
        Some(Depth::Vertices(depths)) => depths.clone(),
        other => panic!("a segment carries one depth per vertex, not {other:?}"),
    }
}

// The depths that each segment item of a three-dimensional line carries, and the points it joins, are pinned in
// `depth.rs`, beside the other depths of a depth group.

// Why: a line passing through a surface must be hidden where it lies behind the surface and shown where it lies in front, in the painter's order of the PDF export as under a depth test, which only per-segment ordering can give.
#[test]
fn a_line_through_a_surface_is_painted_behind_and_in_front_of_it_segment_by_segment() {
    let grid = linspace(-1.0, 1.0, 3);
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let heights = [-1.0, -0.5, 0.5, 1.0];
    let line = fx.series(ax, &[0.5; 4], &[0.5; 4], Some(&heights), |_, _| {});
    let surf = fx.surface(ax, &grid, &grid, |_, _| 0.0, |s| s.edge = ColorSpec::None);
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    let all = leaves(&scene);
    let project = projection(&scene, ax);

    // The fill of the face under the line, whose corners are (0, 0), (1, 0), (1, 1) and (0, 1) at height 0.
    let corners = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let projected: Vec<Point> = corners.iter().map(|c| project(*c).0).collect();
    let face_at = (0..all.len())
        .find(|&k| {
            let leaf = &all[k];
            leaf.source == Some(surf) && leaf.path().is_some_and(|p| p.fill.is_some()) && {
                let ends: Vec<Point> = leaf
                    .endpoints()
                    .iter()
                    .take(4)
                    .map(|p| leaf.transform.apply(*p))
                    .collect();
                ends.len() == 4
                    && projected
                        .iter()
                        .all(|c| ends.iter().any(|e| points_close(*c, *e, 1e-6)))
            }
        })
        .expect("the face under the line is drawn as a fill");
    let face_depth = corners.iter().map(|c| project(*c).1).sum::<f64>() / 4.0;

    let segment_at = |lo: f64, hi: f64| {
        let (a, b) = (project([0.5, 0.5, lo]).0, project([0.5, 0.5, hi]).0);
        (0..all.len())
            .find(|&k| {
                let leaf = &all[k];
                let ends: Vec<Point> = leaf
                    .endpoints()
                    .iter()
                    .map(|p| leaf.transform.apply(*p))
                    .collect();
                leaf.source == Some(line)
                    && ends.len() == 2
                    && ((points_close(ends[0], a, 1e-6) && points_close(ends[1], b, 1e-6))
                        || (points_close(ends[0], b, 1e-6) && points_close(ends[1], a, 1e-6)))
            })
            .unwrap_or_else(|| {
                panic!("the segment from z = {lo} to z = {hi} is an item of its own")
            })
    };
    let (below, above) = (segment_at(-1.0, -0.5), segment_at(0.5, 1.0));
    let mean = |lo: f64, hi: f64| (project([0.5, 0.5, lo]).1 + project([0.5, 0.5, hi]).1) / 2.0;
    let (behind, before) = if mean(-1.0, -0.5) < mean(0.5, 1.0) {
        (below, above)
    } else {
        (above, below)
    };
    assert!(
        mean(-1.0, -0.5).min(mean(0.5, 1.0)) < face_depth
            && face_depth < mean(-1.0, -0.5).max(mean(0.5, 1.0)),
        "the fixture line passes through the face: one segment lies behind it and one in front"
    );
    assert!(
        behind < face_at,
        "the segment behind the face is painted before it ({behind} against {face_at})"
    );
    assert!(
        before > face_at,
        "the segment in front of the face is painted after it ({before} against {face_at})"
    );
}

// Why: a gradient in 3D is still depth-tested along its length, so every piece must carry the depths of its own two ends, interpolated along the segment, with no step between neighbouring pieces.
#[test]
fn the_pieces_of_a_three_dimensional_gradient_carry_depths_interpolated_along_their_segment() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let scale = listed(&mut fx, vec![RED, BLUE]);
    let (a, b) = ([-0.9, -0.9, -0.9], [0.9, 0.9, 0.9]);
    let id = fx.series(
        ax,
        &[a[0], b[0]],
        &[a[1], b[1]],
        Some(&[a[2], b[2]]),
        |s, fx| {
            s.color_data = Some(fx.vector(&[0.0, 1.0]));
            s.color_scale = Some(scale);
            s.line.color = ColorSpec::Colormapped;
        },
    );
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    let project = projection(&scene, ax);
    let ((pa, da), (pb, db)) = (project(a), project(b));
    let mut spans: Vec<(f64, f64, f64, f64)> = Vec::new();
    for leaf in from_source(&leaves(&scene), id) {
        if leaf.path().and_then(|p| p.stroke.as_ref()).is_none() {
            continue;
        }
        let ends: Vec<Point> = leaf
            .endpoints()
            .iter()
            .map(|p| leaf.transform.apply(*p))
            .collect();
        assert_eq!(ends.len(), 2, "a piece is one straight stroke");
        let depths = vertex_depths(&leaf);
        let t: Vec<f64> = ends
            .iter()
            .map(|e| along(*e, pa, pb).expect("a piece lies on its segment"))
            .collect();
        for (ti, di) in t.iter().zip(&depths) {
            assert_close(*di, da + (db - da) * ti, 1e-9);
        }
        let (lo, hi) = if t[0] <= t[1] { (0, 1) } else { (1, 0) };
        spans.push((t[lo], t[hi], depths[lo], depths[hi]));
    }
    assert!(spans.len() > 1, "the gradient is drawn in pieces");
    spans.sort_by(|p, q| p.0.total_cmp(&q.0));
    for pair in spans.windows(2) {
        assert_close(pair[1].0, pair[0].1, 1e-9);
        assert_close(pair[1].2, pair[0].3, 1e-9);
    }
}

// ---------------------------------------------------------------------------------
// Decimation with colour data
// ---------------------------------------------------------------------------------

/// A dense sine of `n` points whose x values are the indices, so a source index reads directly off the x coordinate.
fn dense_sine(n: usize) -> (Vec<f64>, Vec<f64>) {
    (0..n).map(|i| (i as f64, (i as f64 / 250.0).sin())).unzip()
}

/// Colour values in [0, 1) stepping by the golden ratio modulo one. Any two values fewer than 55 indices apart differ
/// by more than a hundredth, so that a colour read at the wrong index of a thinned series is caught, where values
/// stepping by a hundredth would differ from their neighbours' by little more than the tolerance.
fn scrambled(count: usize) -> Vec<f64> {
    let golden = (1.0 + 5.0_f64.sqrt()) / 2.0;
    (0..count).map(|i| (i as f64 * golden).fract()).collect()
}

/// Returns the two consecutive drawn points, in source order, between whose x positions `x` lies.
fn bracket(samples: &[Sample], x: f64) -> (Sample, Sample) {
    let k = samples.partition_point(|s| s.position.x <= x);
    assert!(
        k > 0 && k < samples.len(),
        "a piece at x = {x} lies between two drawn points"
    );
    (samples[k - 1], samples[k])
}

// Why: a dense coloured line is still thinned to what the view can show (ADR 0009), and after thinning each gradient must join two points that were kept, in the colours of their own indices, or the colours would belong to points that are not drawn.
#[test]
fn a_decimated_gradient_joins_the_kept_points_in_the_colours_of_their_own_indices() {
    let n = 40_000;
    let (x, y) = dense_sine(n);
    let c = scrambled(n);
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scale = listed(&mut fx, vec![RED, BLUE]);
    let id = fx.series(ax, &x, &y, None, |s, fx| {
        s.color_data = Some(fx.vector(&c));
        s.color_scale = Some(scale);
        s.line.color = ColorSpec::Colormapped;
    });
    let scene = compile_figure(&fx.build());
    let samples = samples_of(&scene, id).to_vec();
    assert!(samples.len() < n, "the line is thinned");
    let (xmap, ymap) = axis_maps(&scene, ax);
    for s in &samples {
        let i = s.source_index;
        assert_close(s.position.x, xmap.to_figure(x[i]), 1e-9);
        assert_close(s.position.y, ymap.to_figure(y[i]), 1e-9);
    }
    let drawn = pieces(&leaves(&scene), id);
    assert!(!drawn.is_empty());
    for piece in &drawn {
        let mid = (piece.from.x + piece.to.x) / 2.0;
        let (a, b) = bracket(&samples, mid);
        let t0 = along(piece.from, a.position, b.position).unwrap_or_else(|| {
            panic!("the piece at x = {mid} lies on the segment between two kept points")
        });
        let t1 = along(piece.to, a.position, b.position)
            .expect("both ends of a piece lie on its segment");
        let (from, to) = (red_blue(c[a.source_index]), red_blue(c[b.source_index]));
        assert!(
            interpolated_within(piece.colour, from, to, t0.min(t1), t0.max(t1)),
            "the piece at x = {mid} is {:?}, between the colours of points {} and {}",
            piece.colour,
            a.source_index,
            b.source_index
        );
    }
}

// Why: with one value per segment a thinned segment stands for several source segments, so its one colour must be that of one of them, and the rule must be fixed so that a figure is reproducible: it is the colour of the first source segment it replaces, the one that leaves the kept point it starts from.
#[test]
fn a_decimated_line_coloured_per_segment_takes_the_colour_of_the_first_segment_it_replaces() {
    let n = 40_000;
    let (x, y) = dense_sine(n);
    // The loop below checks that the fixture tells apart the colours of the segments of every span it meets.
    let c = scrambled(n - 1);
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scale = listed(&mut fx, vec![RED, BLUE]);
    let id = fx.series(ax, &x, &y, None, |s, fx| {
        s.color_data = Some(fx.vector(&c));
        s.color_scale = Some(scale);
        s.line.color = ColorSpec::Colormapped;
    });
    let scene = compile_figure(&fx.build());
    let samples = samples_of(&scene, id).to_vec();
    assert!(samples.len() < n, "the line is thinned");
    let drawn = pieces(&leaves(&scene), id);
    assert!(!drawn.is_empty(), "the line is drawn");
    let mut spanning = 0;
    for piece in &drawn {
        let mid = (piece.from.x + piece.to.x) / 2.0;
        let (a, b) = bracket(&samples, mid);
        if b.source_index - a.source_index > 1 {
            spanning += 1;
        }
        let first = a.source_index;
        assert!(
            (first + 1..b.source_index).all(|k| !same_colour(red_blue(c[k]), red_blue(c[first]))),
            "the fixture tells the first segment from point {first} to point {} from the others it replaces",
            b.source_index
        );
        assert!(
            same_colour(piece.colour, red_blue(c[first])),
            "the segment from point {} to point {} is {:?}, not the colour {:?} of the first segment it replaces",
            a.source_index,
            b.source_index,
            piece.colour,
            red_blue(c[first])
        );
    }
    assert!(
        spanning > 0,
        "some drawn segments replace several source segments"
    );
}

// Why: markers of a thinned series read their colour from a third array by index, so every kept marker must wear the colour of the value at its own index, whether the series also draws a line or not.
#[test]
fn decimated_markers_keep_the_colours_of_their_own_indices_with_or_without_a_line() {
    let n = 40_000;
    let (x, y) = dense_sine(n);
    let c = scrambled(n);
    for dash in [DashStyle::None, DashStyle::Solid] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let scale = listed(&mut fx, vec![RED, BLUE]);
        let id = fx.series(ax, &x, &y, None, |s, fx| {
            s.color_data = Some(fx.vector(&c));
            s.color_scale = Some(scale);
            s.line.color = ColorSpec::Colormapped;
            s.line.dash = dash;
            s.marker.shape = MarkerShape::Circle;
            s.marker.face = ColorSpec::Colormapped;
        });
        let scene = compile_figure(&fx.build());
        let markers = marker_instances(&leaves(&scene), id);
        assert!(
            !markers.is_empty() && markers.len() < n,
            "{dash:?}: the markers are thinned"
        );
        let (xmap, ymap) = axis_maps(&scene, ax);
        for m in &markers {
            let i = m.source_index;
            assert_close(m.position.x, xmap.to_figure(x[i]), 1e-9);
            assert_close(m.position.y, ymap.to_figure(y[i]), 1e-9);
            for (part, colour) in [("face", m.face), ("edge", m.edge)] {
                let colour = colour.unwrap_or_else(|| panic!("{dash:?}: marker {i} has a {part}"));
                assert!(
                    same_colour(colour, red_blue(c[i])),
                    "{dash:?}: the {part} of marker {i} is {colour:?}, not the colour of its value {}",
                    c[i]
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------
// Legend samples and the colour order
// ---------------------------------------------------------------------------------

/// Returns the leaves drawn wholly inside a rectangle.
fn leaves_in(leaves: &[Leaf], rect: Rect) -> Vec<Leaf> {
    leaves
        .iter()
        .filter(|l| l.bbox().is_some_and(|b| inside(b, rect, 0.5)))
        .cloned()
        .collect()
}

// Why: the legend key draws whatever the series draws (#19): a line sample for a line, a marker for markers, and both for both.
#[test]
fn the_legend_sample_of_a_series_draws_its_line_its_marker_or_both() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let add = |fx: &mut Fx, name: &str, dash: DashStyle, shape: MarkerShape| {
        fx.series(ax, &[0.0, 1.0], &[0.0, 1.0], None, |s, _| {
            s.label = text(name);
            s.line.dash = dash;
            s.marker.shape = shape;
        })
    };
    let line = add(&mut fx, "Line", DashStyle::Solid, MarkerShape::None);
    let markers = add(&mut fx, "Markers", DashStyle::None, MarkerShape::Circle);
    let both = add(&mut fx, "Both", DashStyle::Solid, MarkerShape::Square);
    fx.ax(ax).legend = Some(Legend::default());
    let scene = compile_figure(&fx.build());
    let all = leaves(&scene);
    for entry in axes_legend_entries(&scene, ax) {
        let id = entry_artist(&entry);
        let inside_entry = leaves_in(&all, entry.rect);
        let stroked = inside_entry.iter().any(|l| {
            l.path()
                .is_some_and(|p| p.stroke.is_some() && p.fill.is_none())
        });
        let marked = inside_entry.iter().any(|l| l.markers().is_some());
        let expected = if id == line {
            (true, false)
        } else if id == markers {
            (false, true)
        } else {
            assert_eq!(id, both);
            (true, true)
        };
        assert_eq!(
            (stroked, marked),
            expected,
            "the sample of {id} (line, marker)"
        );
    }
}

// Why: until stage 5 draws a sweep, a colour-mapped series keeps the stage-2 stopgap: its sample takes the middle colour of its own scale.
#[test]
fn the_legend_sample_of_a_colour_mapped_series_takes_the_middle_colour_of_its_scale() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scale = listed(&mut fx, vec![RED, BLUE]);
    fx.series(ax, &[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0], None, |s, fx| {
        s.label = text("Mapped");
        s.color_data = Some(fx.vector(&[0.0, 0.5, 1.0]));
        s.color_scale = Some(scale);
        s.line.color = ColorSpec::Colormapped;
        s.marker.shape = MarkerShape::Circle;
    });
    fx.ax(ax).legend = Some(Legend::default());
    let scene = compile_figure(&fx.build());
    let all = leaves(&scene);
    let entries = axes_legend_entries(&scene, ax);
    assert_eq!(entries.len(), 1);
    let colours: Vec<Rgba> = leaves_in(&all, entries[0].rect)
        .iter()
        .filter(|l| l.glyphs().is_none())
        .flat_map(Leaf::colors)
        .collect();
    assert!(!colours.is_empty(), "the sample is drawn");
    let middle = rgb8(red_blue(0.5));
    for colour in colours {
        let got = rgb8(colour);
        assert!(
            got.iter().zip(middle).all(|(a, b)| a.abs_diff(b) <= 2),
            "the sample is the middle colour {middle:?} of the scale, not {got:?}"
        );
    }
}

// Why: a series whose colour is automatic takes the next colour of the axes colour order whether it draws a line or markers, and a series coloured by data takes none, so the colours of the other series do not depend on it.
#[test]
fn automatic_series_take_the_colour_order_and_a_series_coloured_by_data_takes_none() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scale = listed(&mut fx, vec![RED, BLUE]);
    let first = fx.series(ax, &[0.0, 1.0], &[0.0, 0.2], None, |_, _| {});
    let second = fx.series(ax, &[0.0, 1.0], &[0.3, 0.5], None, |s, _| {
        s.line.dash = DashStyle::None;
        s.marker.shape = MarkerShape::Circle;
    });
    fx.series(ax, &[0.0, 1.0], &[0.6, 0.8], None, |s, fx| {
        s.color_data = Some(fx.vector(&[0.0, 1.0]));
        s.color_scale = Some(scale);
        s.line.color = ColorSpec::Colormapped;
    });
    let fourth = fx.series(ax, &[0.0, 1.0], &[0.9, 1.0], None, |_, _| {});
    let scene = compile_figure(&fx.build());
    let all = leaves(&scene);
    let stroke_of = |id: NodeId| {
        let drawn = pieces(&all, id);
        assert!(!drawn.is_empty(), "series {id} draws its line");
        rgb8(drawn[0].colour)
    };
    assert!(
        rgb8_close(stroke_of(first), COLOUR_ORDER[0]),
        "the first series"
    );
    let edges: Vec<[u8; 3]> = marker_instances(&all, second)
        .iter()
        .map(|m| rgb8(m.edge.expect("an automatic edge is drawn")))
        .collect();
    assert!(!edges.is_empty());
    assert!(
        edges.iter().all(|e| rgb8_close(*e, COLOUR_ORDER[1])),
        "the markers of the second series take the second colour: {edges:?}"
    );
    assert!(
        rgb8_close(stroke_of(fourth), COLOUR_ORDER[2]),
        "the series after the colour-mapped one takes the third colour"
    );
}
