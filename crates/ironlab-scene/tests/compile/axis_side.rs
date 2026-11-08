//! The side of the plot on which an axis is drawn in Cartesian axes: where the line, tick marks, tick labels and label
//! of an axis on the low side, through the middle or on the high side lie, and where the layout reserves their space.
//!
//! Two-dimensional expectations are read from the plot rectangle published in the hit map. Three-dimensional
//! expectations are rebuilt from the public camera and the documented fit, as in [`crate::depth`], at the default
//! view, where the projected box is known without reading the compiler's own mapping. The side of the z axis of
//! cylindrical axes is tested with the other decorations of curved axes in [`crate::curved3d`].

use ironlab_ir::{AxisSide, Limits, NodeId, Text, Ticks, View3d};
use ironlab_scene::Scene;
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::maths::camera::{Camera, fit_to_rect};

use crate::common::{Fx, compile_figure};
use crate::probe::{Leaf, axes_hit, from_source, glyph_runs, leaves, parse_number, text_bbox};

/// Agreement expected of a position on the page, in points.
const TOL: f64 = 1e-6;

/// The base font size of a default figure, in points.
fn fs() -> f64 {
    Fx::new().fig.font_size_pt
}

// ---------------------------------------------------------------------------------------------------------------------
// Two-dimensional fixtures and probes
// ---------------------------------------------------------------------------------------------------------------------

const X_TICKS: [f64; 3] = [10.0, 20.0, 30.0];
const Y_TICKS: [f64; 3] = [-0.5, 0.0, 0.5];

/// One 2D axes whose decorations can be told apart by their text: x runs over [0, 40] with ticks at 10, 20 and 30
/// and the label "Chord"; y runs over [−1, 1] with ticks at −0.5, 0 and 0.5 and the label "Lift"; the axes is titled
/// "Polar curve". Manual limits and explicit ticks keep the ticks the same whatever the size of the plot area, so
/// that axes on different sides can be compared.
fn sided_2d(box_: bool, x: AxisSide, y: AxisSide) -> (Scene, NodeId) {
    sided_2d_with(box_, x, y, |_| {})
}

/// The axes of [`sided_2d`], adjusted further by `edit` before it is compiled.
fn sided_2d_with(
    box_: bool,
    x: AxisSide,
    y: AxisSide,
    edit: impl FnOnce(&mut ironlab_ir::Axes),
) -> (Scene, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[5.0, 35.0], &[-0.8, 0.8], None, |_| {});
    let axes = fx.ax(ax);
    axes.box_ = box_;
    axes.title = Some(Text::plain("Polar curve"));
    axes.x.limits = Limits::Manual {
        min: 0.0,
        max: 40.0,
    };
    axes.x.ticks = Ticks::Values(X_TICKS.to_vec());
    axes.x.label = Some(Text::plain("Chord"));
    axes.x.side = x;
    axes.y.limits = Limits::Manual {
        min: -1.0,
        max: 1.0,
    };
    axes.y.ticks = Ticks::Values(Y_TICKS.to_vec());
    axes.y.label = Some(Text::plain("Lift"));
    axes.y.side = y;
    edit(axes);
    (compile_figure(&fx.build()), ax)
}

fn plot_of(scene: &Scene, ax: NodeId) -> Rect {
    axes_hit(scene, ax).plot_rect
}

/// The bounding boxes of the numeric glyph runs whose value is one of `values`: the tick labels of one axis of
/// [`sided_2d`], whose two axes label different values.
fn label_boxes(scene: &Scene, values: &[f64]) -> Vec<Rect> {
    glyph_runs(&leaves(scene))
        .into_iter()
        .filter(|(_, g)| {
            parse_number(&g.text).is_some_and(|v| values.iter().any(|t| (t - v).abs() < 1e-9))
        })
        .filter_map(|(l, _)| l.bbox())
        .collect()
}

/// The straight segments an axes draws itself (box, spines, tick marks and grid lines), in figure space.
fn segments(scene: &Scene, ax: NodeId) -> Vec<(Point, Point)> {
    from_source(&leaves(scene), ax)
        .iter()
        .filter(|l| l.path().is_some())
        .flat_map(Leaf::line_segments)
        .collect()
}

fn length((a, b): (Point, Point)) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

fn horizontal((a, b): (Point, Point)) -> bool {
    (a.y - b.y).abs() < 1e-9
}

fn vertical((a, b): (Point, Point)) -> bool {
    (a.x - b.x).abs() < 1e-9
}

/// Counts the vertical segments along the line x = `at` spanning more than half the plot height: the line of a y
/// axis.
fn vertical_lines_at(scene: &Scene, ax: NodeId, plot: Rect, at: f64) -> usize {
    segments(scene, ax)
        .into_iter()
        .filter(|&s| vertical(s) && (s.0.x - at).abs() < TOL && length(s) > 0.5 * plot.height)
        .count()
}

/// Counts the horizontal segments along the line y = `at` spanning more than half the plot width: the line of an x
/// axis.
fn horizontal_lines_at(scene: &Scene, ax: NodeId, plot: Rect, at: f64) -> usize {
    segments(scene, ax)
        .into_iter()
        .filter(|&s| horizontal(s) && (s.0.y - at).abs() < TOL && length(s) > 0.5 * plot.width)
        .count()
}

/// Counts the y tick marks on the vertical line x = `at`: short horizontal segments that end on or cross it.
fn y_marks_at(scene: &Scene, ax: NodeId, at: f64) -> usize {
    segments(scene, ax)
        .into_iter()
        .filter(|&s| horizontal(s) && length(s) > 0.0 && length(s) < fs())
        .filter(|(a, b)| a.x.min(b.x) <= at + TOL && a.x.max(b.x) >= at - TOL)
        .count()
}

/// Counts the x tick marks on the horizontal line y = `at`: short vertical segments that end on or cross it.
fn x_marks_at(scene: &Scene, ax: NodeId, at: f64) -> usize {
    segments(scene, ax)
        .into_iter()
        .filter(|&s| vertical(s) && length(s) > 0.0 && length(s) < fs())
        .filter(|(a, b)| a.y.min(b.y) <= at + TOL && a.y.max(b.y) >= at - TOL)
        .count()
}

// ---------------------------------------------------------------------------------------------------------------------
// Two-dimensional axes
// ---------------------------------------------------------------------------------------------------------------------

// Why: Low is the default, and every chart drawn before the side existed must be drawn as it was: the y axis along
// the left of the plot with its labels to the left of it, the x axis along the bottom with its labels below it.
// Setting Low explicitly must give exactly that placement, so that a figure saved with an explicit Low reopens
// unchanged.
#[test]
fn a_low_axis_keeps_todays_placement() {
    let (scene, ax) = sided_2d(false, AxisSide::Low, AxisSide::Low);
    let plot = plot_of(&scene, ax);
    assert_eq!(vertical_lines_at(&scene, ax, plot, plot.x), 1, "left spine");
    assert_eq!(
        vertical_lines_at(&scene, ax, plot, plot.right()),
        0,
        "no right spine"
    );
    assert_eq!(
        horizontal_lines_at(&scene, ax, plot, plot.bottom()),
        1,
        "bottom spine"
    );
    assert_eq!(
        horizontal_lines_at(&scene, ax, plot, plot.y),
        0,
        "no top spine"
    );
    assert!(y_marks_at(&scene, ax, plot.x) >= 3, "y marks on the left");
    assert_eq!(
        y_marks_at(&scene, ax, plot.right()),
        0,
        "no y marks on the right"
    );
    assert!(
        x_marks_at(&scene, ax, plot.bottom()) >= 3,
        "x marks at the bottom"
    );
    assert_eq!(x_marks_at(&scene, ax, plot.y), 0, "no x marks at the top");
    let y_labels = label_boxes(&scene, &Y_TICKS);
    assert_eq!(y_labels.len(), 3, "{y_labels:?}");
    assert!(
        y_labels.iter().all(|b| b.right() <= plot.x + TOL),
        "{y_labels:?}"
    );
    let x_labels = label_boxes(&scene, &X_TICKS);
    assert_eq!(x_labels.len(), 3, "{x_labels:?}");
    assert!(
        x_labels.iter().all(|b| b.y >= plot.bottom() - TOL),
        "{x_labels:?}"
    );
}

// Why: a y axis on the right is how a plot shares its x axis with a neighbour on its left, or reads a second
// quantity against the right edge. Everything the axis draws must move there together: the line (even without the
// box), the tick marks, the tick labels outside the plot and the label outside the tick labels. A tick mark left on
// the left edge, or a label inside the plot, would contradict the axis it belongs to.
#[test]
fn a_high_y_axis_draws_its_ticks_labels_and_label_on_the_right() {
    let (scene, ax) = sided_2d(false, AxisSide::Low, AxisSide::High);
    let plot = plot_of(&scene, ax);
    assert_eq!(
        vertical_lines_at(&scene, ax, plot, plot.right()),
        1,
        "right spine"
    );
    assert_eq!(
        vertical_lines_at(&scene, ax, plot, plot.x),
        0,
        "no left spine"
    );
    assert!(
        y_marks_at(&scene, ax, plot.right()) >= 3,
        "y marks on the right"
    );
    assert_eq!(y_marks_at(&scene, ax, plot.x), 0, "no y marks on the left");
    let labels = label_boxes(&scene, &Y_TICKS);
    assert_eq!(labels.len(), 3, "{labels:?}");
    assert!(
        labels.iter().all(|b| b.x >= plot.right() - TOL),
        "y tick labels right of {plot:?}: {labels:?}"
    );
    let outermost = labels
        .iter()
        .map(|b| b.right())
        .fold(f64::NEG_INFINITY, f64::max);
    let label = text_bbox(&leaves(&scene), "Lift").expect("the y label is drawn");
    assert!(
        label.x >= outermost - TOL,
        "the y label {label:?} lies outside the tick labels ending at {outermost}"
    );
}

// Why: the layout reserves the space of an axis on the side it is drawn on. A y axis on the right needs the band of
// tick labels and label on the right, so the plot area gives up width there, and no longer needs it on the left, so
// it grows into that space. A layout that kept the left margin would leave a blank band and push the labels off the
// page.
#[test]
fn a_high_y_axis_moves_its_margin_to_the_right() {
    let (low, ax) = sided_2d(true, AxisSide::Low, AxisSide::Low);
    let (high, hx) = sided_2d(true, AxisSide::Low, AxisSide::High);
    let (lp, hp) = (plot_of(&low, ax), plot_of(&high, hx));
    assert!(
        hp.right() < lp.right() - fs(),
        "{hp:?} leaves room on the right that {lp:?} does not"
    );
    assert!(hp.x < lp.x - fs(), "{hp:?} grows left of {lp:?}");
}

// Why: an x axis along the top is how a plot shares its y axis with a plot beneath it, or how a matrix names its
// columns above them. Its line, tick marks, tick labels and label must all move above the plot, and the title, which
// heads the axes, must sit above all of them rather than on top of the labels.
#[test]
fn a_high_x_axis_sets_its_labels_above_the_plot_and_below_the_title() {
    let (low, lx) = sided_2d(false, AxisSide::Low, AxisSide::Low);
    let (scene, ax) = sided_2d(false, AxisSide::High, AxisSide::Low);
    let plot = plot_of(&scene, ax);
    assert_eq!(
        horizontal_lines_at(&scene, ax, plot, plot.y),
        1,
        "top spine"
    );
    assert_eq!(
        horizontal_lines_at(&scene, ax, plot, plot.bottom()),
        0,
        "no bottom spine"
    );
    assert!(x_marks_at(&scene, ax, plot.y) >= 3, "x marks at the top");
    assert_eq!(
        x_marks_at(&scene, ax, plot.bottom()),
        0,
        "no x marks at the bottom"
    );
    let labels = label_boxes(&scene, &X_TICKS);
    assert_eq!(labels.len(), 3, "{labels:?}");
    assert!(
        labels.iter().all(|b| b.bottom() <= plot.y + TOL),
        "x tick labels above {plot:?}: {labels:?}"
    );
    let highest = labels.iter().map(|b| b.y).fold(f64::INFINITY, f64::min);
    let all = leaves(&scene);
    let label = text_bbox(&all, "Chord").expect("the x label is drawn");
    assert!(
        label.bottom() <= highest + TOL,
        "the x label {label:?} lies above the tick labels starting at {highest}"
    );
    let title = text_bbox(&all, "Polar curve").expect("the title is drawn");
    assert!(
        title.bottom() <= label.y + TOL,
        "the title {title:?} lies above the x label {label:?}"
    );
    let lp = plot_of(&low, lx);
    assert!(
        plot.y > lp.y + fs(),
        "{plot:?} gives room above that {lp:?} does not"
    );
    assert!(
        plot.bottom() > lp.bottom() + fs(),
        "{plot:?} grows below {lp:?}"
    );
}

// Why: a y axis through the middle of the plot is the textbook look of a function plotted about its origin. The
// line runs up the middle of the plot area with the tick marks on it, and the tick labels sit on its low side (to
// its left), inside the plot area, since the axis claims no margin of its own; the left edge of the plot, without
// the box, then carries nothing.
#[test]
fn a_centre_y_axis_runs_up_the_middle_with_its_labels_on_the_low_side() {
    let (scene, ax) = sided_2d(false, AxisSide::Low, AxisSide::Centre);
    let plot = plot_of(&scene, ax);
    let mid = plot.x + plot.width / 2.0;
    assert_eq!(
        vertical_lines_at(&scene, ax, plot, mid),
        1,
        "a line up the middle of {plot:?}"
    );
    assert_eq!(
        vertical_lines_at(&scene, ax, plot, plot.x),
        0,
        "no left spine"
    );
    assert!(
        y_marks_at(&scene, ax, mid) >= 3,
        "y marks on the middle line"
    );
    assert_eq!(
        y_marks_at(&scene, ax, plot.x),
        0,
        "no y marks on the left edge"
    );
    let labels = label_boxes(&scene, &Y_TICKS);
    assert_eq!(labels.len(), 3, "{labels:?}");
    for b in &labels {
        assert!(
            b.right() <= mid + TOL,
            "{b:?} lies left of the line at {mid}"
        );
        assert!(b.x >= plot.x - TOL, "{b:?} lies inside {plot:?}");
    }
}

// Why: an axis through the middle of the plot draws inside the plot area, so the layout must reserve no margin for
// it; the plot area grows to the left into the space a y axis on the left would have taken.
#[test]
fn a_centre_y_axis_reserves_no_margin() {
    let (low, ax) = sided_2d(false, AxisSide::Low, AxisSide::Low);
    let (centre, cx) = sided_2d(false, AxisSide::Low, AxisSide::Centre);
    let (lp, cp) = (plot_of(&low, ax), plot_of(&centre, cx));
    assert!(cp.x < lp.x - fs(), "{cp:?} grows left of {lp:?}");
    assert!(cp.width > lp.width + fs(), "{cp:?} is wider than {lp:?}");
}

// Why: the x axis through the middle is the same layout as the y axis through the middle, turned: the line runs across
// the middle of the plot area (its middle, not the value zero of y, which here lies on it only by coincidence of the
// symmetric limits), the tick marks lie on it, and the tick labels sit on its low side, below it, inside the plot
// area. The bottom edge, without the box, carries nothing.
#[test]
fn a_centre_x_axis_runs_across_the_middle_with_its_labels_below() {
    let (scene, ax) = sided_2d(false, AxisSide::Centre, AxisSide::Low);
    let plot = plot_of(&scene, ax);
    let mid = plot.y + plot.height / 2.0;
    assert_eq!(
        horizontal_lines_at(&scene, ax, plot, mid),
        1,
        "a line across the middle of {plot:?}"
    );
    assert_eq!(
        horizontal_lines_at(&scene, ax, plot, plot.bottom()),
        0,
        "no bottom spine"
    );
    assert!(
        x_marks_at(&scene, ax, mid) >= 3,
        "x marks on the middle line"
    );
    assert_eq!(
        x_marks_at(&scene, ax, plot.bottom()),
        0,
        "no x marks on the bottom edge"
    );
    let labels = label_boxes(&scene, &X_TICKS);
    assert_eq!(labels.len(), 3, "{labels:?}");
    for b in &labels {
        assert!(b.y >= mid - TOL, "{b:?} lies below the line at {mid}");
        assert!(
            b.bottom() <= plot.bottom() + TOL,
            "{b:?} lies inside {plot:?}"
        );
    }
}

// Why: the side is a side of the page, not an end of the data range. The y axis is placed along x, so with x reversed
// the largest x lies on the left; an implementation that put a high y axis at the largest x would put it on the left.
// Likewise a high x axis must stay at the top when y is reversed. With both axes reversed and both on the high side,
// the y axis is still on the right of the plot and the x axis still along its top, each labelled outside.
#[test]
fn a_high_axis_stays_on_the_high_side_of_the_page_when_the_axes_are_reversed() {
    let (scene, ax) = sided_2d_with(false, AxisSide::High, AxisSide::High, |axes| {
        axes.x.reverse = true;
        axes.y.reverse = true;
    });
    let plot = plot_of(&scene, ax);
    assert_eq!(
        vertical_lines_at(&scene, ax, plot, plot.right()),
        1,
        "right spine"
    );
    assert_eq!(
        vertical_lines_at(&scene, ax, plot, plot.x),
        0,
        "no left spine"
    );
    assert_eq!(
        horizontal_lines_at(&scene, ax, plot, plot.y),
        1,
        "top spine"
    );
    assert_eq!(
        horizontal_lines_at(&scene, ax, plot, plot.bottom()),
        0,
        "no bottom spine"
    );
    let y_labels = label_boxes(&scene, &Y_TICKS);
    assert_eq!(y_labels.len(), 3, "{y_labels:?}");
    assert!(
        y_labels.iter().all(|b| b.x >= plot.right() - TOL),
        "y tick labels right of {plot:?}: {y_labels:?}"
    );
    let x_labels = label_boxes(&scene, &X_TICKS);
    assert_eq!(x_labels.len(), 3, "{x_labels:?}");
    assert!(
        x_labels.iter().all(|b| b.bottom() <= plot.y + TOL),
        "x tick labels above {plot:?}: {x_labels:?}"
    );
}

// Why: the box is a frame round the plot, independent of where the axes are read. With the box on, a y axis through
// the middle keeps the whole frame, both left and right edges, and adds its own line up the middle with the tick
// marks on it; its tick labels sit beside that line, not beside the left edge of the frame. A y axis on the high side
// with the box on is labelled on the right only: the box mirrors tick marks, never tick labels.
#[test]
fn with_the_box_on_the_frame_stays_and_the_labels_follow_the_side() {
    let (scene, ax) = sided_2d(true, AxisSide::Low, AxisSide::Centre);
    let plot = plot_of(&scene, ax);
    let mid = plot.x + plot.width / 2.0;
    assert_eq!(vertical_lines_at(&scene, ax, plot, plot.x), 1, "left edge");
    assert_eq!(
        vertical_lines_at(&scene, ax, plot, plot.right()),
        1,
        "right edge"
    );
    assert_eq!(
        vertical_lines_at(&scene, ax, plot, mid),
        1,
        "a line up the middle of {plot:?}"
    );
    assert!(
        y_marks_at(&scene, ax, mid) >= 3,
        "y marks on the middle line"
    );
    let labels = label_boxes(&scene, &Y_TICKS);
    assert_eq!(labels.len(), 3, "{labels:?}");
    for b in &labels {
        assert!(
            b.right() <= mid + TOL,
            "{b:?} lies left of the line at {mid}"
        );
        assert!(
            mid - b.right() < b.x - plot.x,
            "{b:?} lies nearer the middle line at {mid} than the left edge at {}",
            plot.x
        );
    }

    let (scene, ax) = sided_2d(true, AxisSide::Low, AxisSide::High);
    let plot = plot_of(&scene, ax);
    let labels = label_boxes(&scene, &Y_TICKS);
    assert_eq!(labels.len(), 3, "{labels:?}");
    assert!(
        labels.iter().all(|b| b.x >= plot.right() - TOL),
        "y tick labels right of {plot:?}: {labels:?}"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Three-dimensional axes
// ---------------------------------------------------------------------------------------------------------------------

/// The views at which the side of a 3D axis is checked: the default view, and two views turned so far round that a
/// different box edge is lowest, highest, leftmost and rightmost on the page. The side is chosen afresh for every view,
/// so an implementation that fixed the edge once (for example by its data coordinates) would put the axis on the wrong
/// side of the page in at least one of them. No azimuth is a multiple of 90°, where two parallel edges would lie
/// equally high on the page.
fn views_3d() -> [View3d; 3] {
    [
        View3d::default(),
        View3d {
            azimuth_deg: 150.0,
            elevation_deg: 25.0,
            ..View3d::default()
        },
        View3d {
            azimuth_deg: -120.0,
            elevation_deg: 40.0,
            ..View3d::default()
        },
    ]
}

/// A 3D axes at `view` whose tick labels are told apart by their text, as in `three_d.rs`: x labels have decimals
/// within (0.05, 0.95), y labels are hundreds and z labels are negative. `edit` sets the sides.
fn sided_3d(view: View3d, edit: impl FnOnce(&mut ironlab_ir::Axes)) -> (Scene, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, view);
    fx.line(
        ax,
        &[0.15, 0.85],
        &[150.0, 850.0],
        Some(&[-85.0, -15.0]),
        |_| {},
    );
    let axes = fx.ax(ax);
    axes.x.limits = Limits::Manual {
        min: 0.05,
        max: 0.95,
    };
    axes.y.limits = Limits::Manual {
        min: 110.0,
        max: 890.0,
    };
    axes.z.limits = Limits::Manual {
        min: -95.0,
        max: -15.0,
    };
    edit(axes);
    (compile_figure(&fx.build()), ax)
}

/// The tick label boxes of one axis of [`sided_3d`]: 0 for x, 1 for y, 2 for z.
fn tick_labels_3d(scene: &Scene, dim: usize) -> Vec<Rect> {
    glyph_runs(&leaves(scene))
        .into_iter()
        .filter(|(_, g)| {
            let Some(v) = parse_number(&g.text) else {
                return false;
            };
            match dim {
                0 => g.text.contains('.') && (0.05..=0.95).contains(&v),
                1 => (110.0..=890.0).contains(&v),
                _ => (-95.0..=-15.0).contains(&v),
            }
        })
        .filter_map(|(l, _)| l.bbox())
        .collect()
}

/// The figure-space position of a point of the normalised box (each coordinate in [−0.5, 0.5]) of a 3D axes at
/// `view`, which must have no zoom or pan, rebuilt from the public camera and the documented fit.
fn project_3d(scene: &Scene, ax: NodeId, view: View3d) -> impl Fn([f64; 3]) -> Point {
    let plot = plot_of(scene, ax);
    let (scale, offset) = fit_to_rect(plot.width, plot.height);
    let origin = Point::new(plot.x + offset[0], plot.y + offset[1]);
    let camera = Camera {
        azimuth_deg: view.azimuth_deg,
        elevation_deg: view.elevation_deg,
    };
    move |n: [f64; 3]| {
        let p = camera.project(n);
        Point::new(
            origin.x + scale * p.screen[0],
            origin.y - scale * p.screen[1],
        )
    }
}

/// The eight projected corners of the box.
fn corners(project: &impl Fn([f64; 3]) -> Point) -> Vec<Point> {
    let mut out = Vec::new();
    for x in [-0.5, 0.5] {
        for y in [-0.5, 0.5] {
            for z in [-0.5, 0.5] {
                out.push(project([x, y, z]));
            }
        }
    }
    out
}

/// The four projected edges of the box parallel to axis `dim`.
fn edges_along(project: &impl Fn([f64; 3]) -> Point, dim: usize) -> Vec<(Point, Point)> {
    let others: Vec<usize> = (0..3).filter(|k| *k != dim).collect();
    let mut out = Vec::new();
    for a in [-0.5, 0.5] {
        for b in [-0.5, 0.5] {
            let mut lo = [0.0; 3];
            lo[others[0]] = a;
            lo[others[1]] = b;
            let mut hi = lo;
            lo[dim] = -0.5;
            hi[dim] = 0.5;
            out.push((project(lo), project(hi)));
        }
    }
    out
}

fn distance_to_segment(p: Point, (a, b): (Point, Point)) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

fn centre_of(r: Rect) -> Point {
    Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
}

// Why: the z axis of a 3D box is read from a vertical edge. On the high side it is the vertical edge furthest right,
// which is where a reader expects it when the data is seen from the other side, or when the left of the page is
// taken by a neighbouring plot; its labels then lie to the right of the whole box. On the low side (the default) they
// lie to the left of the whole box, as they always have. Which edge that is changes as the camera turns, and the axis
// must follow, so that it stays on the side of the page asked for.
#[test]
fn a_high_z_axis_labels_the_rightmost_vertical_edge_in_every_view() {
    for view in views_3d() {
        let (high, ax) = sided_3d(view, |axes| axes.z.side = AxisSide::High);
        let right = corners(&project_3d(&high, ax, view))
            .iter()
            .map(|p| p.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let labels = tick_labels_3d(&high, 2);
        assert!(!labels.is_empty(), "{view:?}: z tick labels are drawn");
        for b in &labels {
            assert!(
                b.x >= right - TOL,
                "{view:?}: {b:?} lies right of the box, which ends at {right}"
            );
        }

        let (low, lx) = sided_3d(view, |_| {});
        let left = corners(&project_3d(&low, lx, view))
            .iter()
            .map(|p| p.x)
            .fold(f64::INFINITY, f64::min);
        let labels = tick_labels_3d(&low, 2);
        assert!(!labels.is_empty(), "{view:?}: z tick labels are drawn");
        for b in &labels {
            assert!(
                b.right() <= left + TOL,
                "{view:?}: {b:?} lies left of the box, which starts at {left}"
            );
        }
    }
}

// Why: the x and y axes of a 3D box are read from one of the four box edges parallel to each. On the high side that
// is the edge highest on the page, so that the labels clear data piled up at the front of the box; on the low side
// (the default) it is the edge lowest on the page. Each label must lie by its own edge, nearer it than any other
// parallel edge, or the reader would take a value off the wrong edge. The choice is made again in every view, as the
// camera turns a different edge to the top of the page.
#[test]
fn a_high_x_or_y_axis_labels_the_edge_highest_on_the_page_in_every_view() {
    for view in views_3d() {
        for dim in [0, 1] {
            for side in [AxisSide::Low, AxisSide::High] {
                let (scene, ax) = sided_3d(view, |axes| {
                    if dim == 0 {
                        axes.x.side = side;
                    } else {
                        axes.y.side = side;
                    }
                });
                let project = project_3d(&scene, ax, view);
                let edges = edges_along(&project, dim);
                let mid_y = |(a, b): (Point, Point)| (a.y + b.y) / 2.0;
                let chosen = *edges
                    .iter()
                    .min_by(|a, b| {
                        let (ya, yb) = (mid_y(**a), mid_y(**b));
                        if side == AxisSide::High {
                            ya.total_cmp(&yb)
                        } else {
                            yb.total_cmp(&ya)
                        }
                    })
                    .unwrap();
                let box_centre = project([0.0; 3]);
                let labels = tick_labels_3d(&scene, dim);
                assert!(
                    !labels.is_empty(),
                    "{view:?}, axis {dim} {side:?}: tick labels are drawn"
                );
                for b in &labels {
                    let c = centre_of(*b);
                    let near = distance_to_segment(c, chosen);
                    for other in &edges {
                        assert!(
                            near <= distance_to_segment(c, *other) + TOL,
                            "{view:?}, axis {dim} {side:?}: the label at {b:?} lies nearer {other:?} than its edge {chosen:?}"
                        );
                    }
                    if side == AxisSide::High {
                        assert!(
                            c.y < box_centre.y,
                            "{view:?}, axis {dim}: {b:?} lies above the box centre"
                        );
                    } else {
                        assert!(
                            c.y > box_centre.y,
                            "{view:?}, axis {dim}: {b:?} lies below the box centre"
                        );
                    }
                }
            }
        }
    }
}

// Why: a z axis through the middle of a 3D box stands on the centre of the floor, where a reader takes heights off a
// pole planted in the data; its labels sit on its low side, to the left of the pole but inside the outline of the box
// rather than beyond its leftmost edge.
#[test]
fn a_centre_z_axis_stands_at_the_centre_of_the_floor() {
    let view = View3d::default();
    let (scene, ax) = sided_3d(view, |axes| axes.z.side = AxisSide::Centre);
    let project = project_3d(&scene, ax, view);
    let pole = project([0.0, 0.0, -0.5]).x;
    let left = corners(&project)
        .iter()
        .map(|p| p.x)
        .fold(f64::INFINITY, f64::min);
    let labels = tick_labels_3d(&scene, 2);
    assert!(!labels.is_empty(), "z tick labels are drawn");
    for b in &labels {
        assert!(
            b.right() <= pole + TOL,
            "{b:?} lies left of the pole at {pole}"
        );
        assert!(
            b.right() > left,
            "{b:?} ends inside the box, which starts at {left}"
        );
    }
}
