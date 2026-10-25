//! The visibility of an axis and the angle of its tick labels: what a hidden axis leaves out and the space it gives to
//! the plot area, and where rotated tick labels are anchored and how much space the layout reserves for them.

use ironlab_ir::{Limits, NodeId, Text, TickLabels, Ticks, View3d};
use ironlab_scene::display::{Point, Rect, Transform};
use ironlab_scene::{Scene, SceneWarningKind};
use ironlab_text::FontId;
use kurbo::Shape;

use crate::common::{Fx, compile_figure, engine};
use crate::probe::{
    Leaf, assert_close, axes_hit, axis_maps, disjoint, from_source, glyph_runs, inside, leaves,
    parse_number, runs_with_text, text_bbox, x_tick_labels, y_tick_labels,
};

/// The base font size of a default figure, in points.
fn fs() -> f64 {
    Fx::new().fig.font_size_pt
}

// ---------------------------------------------------------------------------------------------------------------------
// Hidden axes
// ---------------------------------------------------------------------------------------------------------------------

/// One 2D axes whose every decoration can be told apart: x runs over [0, 4·10⁵] with explicit ticks at 1, 2 and
/// 3 ·10⁵, so its labels carry the only common exponent of the figure; y runs over [−1, 1] with explicit ticks at
/// −0.5, 0 and 0.5; both axes are labelled and gridded. Explicit ticks and manual limits keep the ticks the same
/// whatever the size of the plot area, so a figure with a hidden axis can be compared with one without.
fn decorated_2d(box_: bool) -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.5e5, 3.5e5], &[-0.8, 0.8], None, |_| {});
    let axes = fx.ax(ax);
    axes.box_ = box_;
    axes.x.limits = Limits::Manual { min: 0.0, max: 4e5 };
    axes.x.ticks = Ticks::Values(vec![1e5, 2e5, 3e5]);
    axes.x.label = Some(Text::plain("Chord"));
    axes.x.grid = true;
    axes.y.limits = Limits::Manual {
        min: -1.0,
        max: 1.0,
    };
    axes.y.ticks = Ticks::Values(vec![-0.5, 0.0, 0.5]);
    axes.y.label = Some(Text::plain("Lift"));
    axes.y.grid = true;
    (fx, ax)
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

/// Counts the horizontal segments lying along the line y = `edge` and spanning more than half the plot width: the
/// spine of an x axis, along the bottom of the plot area (or the top, when the box is on).
fn horizontal_spines(scene: &Scene, ax: NodeId, plot: Rect, edge: f64) -> usize {
    segments(scene, ax)
        .into_iter()
        .filter(|&s| horizontal(s) && (s.0.y - edge).abs() < 1e-6 && length(s) > 0.5 * plot.width)
        .count()
}

/// Counts the vertical segments lying along the line x = `edge` and spanning more than half the plot height: the
/// spine of a y axis, along the left of the plot area (or the right, when the box is on).
fn vertical_spines(scene: &Scene, ax: NodeId, plot: Rect, edge: f64) -> usize {
    segments(scene, ax)
        .into_iter()
        .filter(|&s| vertical(s) && (s.0.x - edge).abs() < 1e-6 && length(s) > 0.5 * plot.height)
        .count()
}

/// Counts the x tick marks: short vertical segments with an end on the bottom or the top edge of the plot area.
fn x_tick_marks(scene: &Scene, ax: NodeId, plot: Rect) -> usize {
    segments(scene, ax)
        .into_iter()
        .filter(|&s| vertical(s) && length(s) > 0.0 && length(s) < fs())
        .filter(|(a, b)| {
            [a.y, b.y]
                .iter()
                .any(|y| (y - plot.bottom()).abs() < 1e-6 || (y - plot.y).abs() < 1e-6)
        })
        .count()
}

/// Counts the y tick marks: short horizontal segments with an end on the left or the right edge of the plot area.
fn y_tick_marks(scene: &Scene, ax: NodeId, plot: Rect) -> usize {
    segments(scene, ax)
        .into_iter()
        .filter(|&s| horizontal(s) && length(s) > 0.0 && length(s) < fs())
        .filter(|(a, b)| {
            [a.x, b.x]
                .iter()
                .any(|x| (x - plot.x).abs() < 1e-6 || (x - plot.right()).abs() < 1e-6)
        })
        .count()
}

/// Counts the x grid lines: vertical segments strictly inside the plot area spanning its height.
fn x_grid_lines(scene: &Scene, ax: NodeId, plot: Rect) -> usize {
    segments(scene, ax)
        .into_iter()
        .filter(|&s| vertical(s) && length(s) > 0.9 * plot.height)
        .filter(|(a, _)| a.x > plot.x + 1e-6 && a.x < plot.right() - 1e-6)
        .count()
}

/// Counts the y grid lines: horizontal segments strictly inside the plot area spanning its width.
fn y_grid_lines(scene: &Scene, ax: NodeId, plot: Rect) -> usize {
    segments(scene, ax)
        .into_iter()
        .filter(|&s| horizontal(s) && length(s) > 0.9 * plot.width)
        .filter(|(a, _)| a.y > plot.y + 1e-6 && a.y < plot.bottom() - 1e-6)
        .count()
}

/// Counts the glyph runs set in the math font, which in [`decorated_2d`] are those of the common exponent of x.
fn math_runs(scene: &Scene) -> usize {
    glyph_runs(&leaves(scene))
        .into_iter()
        .filter(|(_, g)| g.font == FontId::Math)
        .count()
}

// Why (#74): a figure whose x axis is shared with a plot below it, or whose x axis means nothing (a strip of images),
// hides the axis entirely. A hidden x axis must draw nothing along the bottom (nor along the top, where the box would
// repeat it): no spine, no tick mark, no tick label, no common exponent and no label. Anything left over would be a
// stray line or number that the author asked to remove.
#[test]
fn a_hidden_x_axis_draws_no_spine_ticks_labels_exponent_or_label() {
    for box_ in [false, true] {
        let (mut fx, ax) = decorated_2d(box_);
        let shown = compile_figure(&fx.fig.clone());
        fx.ax(ax).x.visible = false;
        let hidden = compile_figure(&fx.build());
        let (sp, hp) = (
            axes_hit(&shown, ax).plot_rect,
            axes_hit(&hidden, ax).plot_rect,
        );

        // The fixture draws every decoration when the axis is shown, so that their absence below means something.
        assert_eq!(
            horizontal_spines(&shown, ax, sp, sp.bottom()),
            1,
            "box {box_}"
        );
        assert!(x_tick_marks(&shown, ax, sp) >= 3, "box {box_}");
        assert_eq!(x_tick_labels(&leaves(&shown), sp).len(), 3, "box {box_}");
        assert!(
            math_runs(&shown) > 0,
            "box {box_}: the x labels carry an exponent"
        );
        assert!(!runs_with_text(&leaves(&shown), "Chord").is_empty());

        assert_eq!(
            horizontal_spines(&hidden, ax, hp, hp.bottom()),
            0,
            "box {box_}: bottom spine"
        );
        assert_eq!(
            horizontal_spines(&hidden, ax, hp, hp.y),
            0,
            "box {box_}: top spine"
        );
        assert_eq!(x_tick_marks(&hidden, ax, hp), 0, "box {box_}: x tick marks");
        assert!(
            x_tick_labels(&leaves(&hidden), hp).is_empty(),
            "box {box_}: x tick labels"
        );
        assert_eq!(math_runs(&hidden), 0, "box {box_}: the x exponent");
        assert!(
            runs_with_text(&leaves(&hidden), "Chord").is_empty(),
            "box {box_}: x label"
        );
    }
}

// Why (#74): hiding an axis hides the x axis and nothing else. The y axis keeps its spine (and, with the box on, the
// right edge that repeats it), its ticks, labels and label, and the grid lines of the hidden axis still follow `grid`,
// because a grid is a property of the data region rather than of the drawn axis.
#[test]
fn a_hidden_x_axis_keeps_its_grid_lines_and_leaves_the_y_axis_as_it_was() {
    for box_ in [false, true] {
        let (mut fx, ax) = decorated_2d(box_);
        let shown = compile_figure(&fx.fig.clone());
        fx.ax(ax).x.visible = false;
        let hidden = compile_figure(&fx.build());
        let (sp, hp) = (
            axes_hit(&shown, ax).plot_rect,
            axes_hit(&hidden, ax).plot_rect,
        );

        assert_eq!(x_grid_lines(&hidden, ax, hp), 3, "box {box_}: x grid lines");
        assert_eq!(y_grid_lines(&hidden, ax, hp), 3, "box {box_}: y grid lines");
        assert_eq!(
            vertical_spines(&hidden, ax, hp, hp.x),
            1,
            "box {box_}: left spine"
        );
        assert_eq!(
            vertical_spines(&hidden, ax, hp, hp.right()),
            usize::from(box_),
            "box {box_}: right edge"
        );
        assert_eq!(
            y_tick_marks(&hidden, ax, hp),
            y_tick_marks(&shown, ax, sp),
            "box {box_}: y tick marks"
        );
        let y_labels = |scene: &Scene, plot| {
            let mut v: Vec<f64> = y_tick_labels(&leaves(scene), plot)
                .iter()
                .map(|l| l.value)
                .collect();
            v.sort_by(f64::total_cmp);
            v
        };
        assert_eq!(y_labels(&hidden, hp), vec![-0.5, 0.0, 0.5], "box {box_}");
        assert!(
            !runs_with_text(&leaves(&hidden), "Lift").is_empty(),
            "box {box_}: y label"
        );
        // The limits still place the data: the hidden axis maps the same range across the plot area.
        let (x, _) = axis_maps(&hidden, ax);
        assert_eq!((x.min, x.max), (0.0, 4e5));
        assert_close(x.to_figure(0.0), hp.x, 1e-6);
        assert_close(x.to_figure(4e5), hp.right(), 1e-6);
    }
}

// Why (#74): no space is reserved for what a hidden axis does not draw, so the plot area grows down into the rows of
// tick labels, exponent and label it would have taken, exactly as it grows into the space of hidden tick labels. A
// hidden axis that left a blank band would waste the space hiding it was meant to recover.
#[test]
fn a_hidden_x_axis_gives_its_space_to_the_plot_area() {
    let (mut fx, ax) = decorated_2d(true);
    let shown = compile_figure(&fx.fig.clone());
    fx.ax(ax).x.visible = false;
    let hidden = compile_figure(&fx.build());
    let (sp, hp) = (
        axes_hit(&shown, ax).plot_rect,
        axes_hit(&hidden, ax).plot_rect,
    );
    // The tick labels and the axis label take at least two lines of text.
    assert!(
        hp.height > sp.height + 2.0 * fs(),
        "{hp:?} is taller than {sp:?}"
    );
    assert!(
        hp.bottom() > sp.bottom() + 2.0 * fs(),
        "{hp:?} grows below {sp:?}"
    );
    // It grows downwards only: the y axis still needs its space on the left.
    assert_close(hp.x, sp.x, 1e-6);
}

// Why: hiding the y axis is the same operation along the other dimension; the left spine (and the right edge with
// the box on), the y ticks, labels and label disappear, the plot area grows to the left, and the x axis and the y grid
// are unchanged.
#[test]
fn a_hidden_y_axis_draws_nothing_beside_the_plot_and_gives_its_space_to_the_plot_area() {
    for box_ in [false, true] {
        let (mut fx, ax) = decorated_2d(box_);
        let shown = compile_figure(&fx.fig.clone());
        fx.ax(ax).y.visible = false;
        let hidden = compile_figure(&fx.build());
        let (sp, hp) = (
            axes_hit(&shown, ax).plot_rect,
            axes_hit(&hidden, ax).plot_rect,
        );

        assert_eq!(vertical_spines(&shown, ax, sp, sp.x), 1, "box {box_}");
        assert!(y_tick_marks(&shown, ax, sp) >= 3, "box {box_}");
        assert_eq!(y_tick_labels(&leaves(&shown), sp).len(), 3, "box {box_}");

        assert_eq!(
            vertical_spines(&hidden, ax, hp, hp.x),
            0,
            "box {box_}: left spine"
        );
        assert_eq!(
            vertical_spines(&hidden, ax, hp, hp.right()),
            0,
            "box {box_}: right edge"
        );
        assert_eq!(y_tick_marks(&hidden, ax, hp), 0, "box {box_}: y tick marks");
        assert!(
            y_tick_labels(&leaves(&hidden), hp).is_empty(),
            "box {box_}: y tick labels"
        );
        assert!(
            runs_with_text(&leaves(&hidden), "Lift").is_empty(),
            "box {box_}: y label"
        );

        assert!(
            hp.x < sp.x - fs(),
            "box {box_}: {hp:?} grows left of {sp:?}"
        );
        assert!(
            hp.width > sp.width + fs(),
            "box {box_}: {hp:?} is wider than {sp:?}"
        );

        assert_eq!(y_grid_lines(&hidden, ax, hp), 3, "box {box_}: y grid lines");
        assert_eq!(
            horizontal_spines(&hidden, ax, hp, hp.bottom()),
            1,
            "box {box_}: bottom spine"
        );
        assert_eq!(
            horizontal_spines(&hidden, ax, hp, hp.y),
            usize::from(box_),
            "box {box_}: top edge"
        );
        assert_eq!(x_tick_labels(&leaves(&hidden), hp).len(), 3, "box {box_}");
        assert!(math_runs(&hidden) > 0, "box {box_}: the x exponent stays");
        assert!(
            !runs_with_text(&leaves(&hidden), "Chord").is_empty(),
            "box {box_}: x label"
        );
    }
}

// Why: with both axes hidden only the data and the grids remain (a strip of images, a schematic); the box, which is
// made of the spines of the two axes, must disappear with them rather than be drawn because it is "on".
#[test]
fn hiding_both_axes_removes_the_box() {
    let (mut fx, ax) = decorated_2d(true);
    fx.ax(ax).x.visible = false;
    fx.ax(ax).y.visible = false;
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    assert_eq!(horizontal_spines(&scene, ax, plot, plot.bottom()), 0);
    assert_eq!(horizontal_spines(&scene, ax, plot, plot.y), 0);
    assert_eq!(vertical_spines(&scene, ax, plot, plot.x), 0);
    assert_eq!(vertical_spines(&scene, ax, plot, plot.right()), 0);
    assert_eq!(x_grid_lines(&scene, ax, plot), 3);
    assert_eq!(y_grid_lines(&scene, ax, plot), 3);
    assert!(glyph_runs(&leaves(&scene)).is_empty(), "no text is drawn");
}

// ---------------------------------------------------------------------------------------------------------------------
// Rotated tick labels
// ---------------------------------------------------------------------------------------------------------------------

/// Long category names, the case for rotated labels (#153).
const NAMES: [&str; 4] = [
    "first category",
    "second category",
    "third category",
    "fourth category",
];

/// One 2D axes naming the four [`NAMES`] at x = 0, 1, 2 and 3, with x limits [−0.5, 3.5] and the x label "Case",
/// whose x tick labels are turned by `angle` degrees.
fn x_categories(angle: f64) -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(
        ax,
        &[0.0, 1.0, 2.0, 3.0],
        &[0.0, 1.0, 0.0, 1.0],
        None,
        |_| {},
    );
    let axes = fx.ax(ax);
    axes.x.limits = Limits::Manual {
        min: -0.5,
        max: 3.5,
    };
    axes.x.ticks = Ticks::Values(vec![0.0, 1.0, 2.0, 3.0]);
    axes.x.tick_labels = TickLabels::Text(NAMES.iter().map(|n| Text::plain(*n)).collect());
    axes.x.label = Some(Text::plain("Case"));
    axes.x.tick_label_angle = angle;
    (fx, ax)
}

/// One 2D axes naming the four [`NAMES`] at y = 0, 1, 2 and 3, with y limits [−0.5, 3.5] and the y label "Case",
/// whose y tick labels are turned by `angle` degrees.
fn y_categories(angle: f64) -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(
        ax,
        &[0.0, 1.0, 0.0, 1.0],
        &[0.0, 1.0, 2.0, 3.0],
        None,
        |_| {},
    );
    let axes = fx.ax(ax);
    axes.y.limits = Limits::Manual {
        min: -0.5,
        max: 3.5,
    };
    axes.y.ticks = Ticks::Values(vec![0.0, 1.0, 2.0, 3.0]);
    axes.y.tick_labels = TickLabels::Text(NAMES.iter().map(|n| Text::plain(*n)).collect());
    axes.y.label = Some(Text::plain("Case"));
    axes.y.tick_label_angle = angle;
    (fx, ax)
}

/// Returns the one glyph run that draws `name`.
#[track_caller]
fn run(scene: &Scene, name: &str) -> Leaf {
    let all = leaves(scene);
    let runs = runs_with_text(&all, name);
    assert_eq!(runs.len(), 1, "{name} is drawn as one run");
    runs[0].clone()
}

/// Returns the unit vector along which a run reads, in figure space (y downwards).
fn reading_direction(leaf: &Leaf) -> Point {
    let t: Transform = leaf.transform;
    let (o, a) = (t.apply(Point::new(0.0, 0.0)), t.apply(Point::new(1.0, 0.0)));
    let (dx, dy) = (a.x - o.x, a.y - o.y);
    let n = dx.hypot(dy);
    Point::new(dx / n, dy / n)
}

/// Returns the unit vector along which text turned `degrees` anticlockwise from horizontal reads on the page, whose
/// y axis points downwards.
fn turned(degrees: f64) -> Point {
    let (s, c) = degrees.to_radians().sin_cos();
    Point::new(c, -s)
}

#[track_caller]
fn assert_reads_along(leaf: &Leaf, degrees: f64) {
    let (d, e) = (reading_direction(leaf), turned(degrees));
    assert!(
        (d.x - e.x).abs() < 1e-6 && (d.y - e.y).abs() < 1e-6,
        "a label turned {degrees}° must read along {e:?}, not {d:?}"
    );
}

/// Returns the two ends of a run along its reading direction, in figure space: the points midway across the ink at
/// its first and at its last extent along the line of text. These are what "the end of the label nearest its tick"
/// refers to, whatever the transform that turns the run.
fn ends(leaf: &Leaf) -> (Point, Point) {
    let run = leaf.glyphs().expect("a glyph run");
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for g in &run.glyphs {
        let Some(outline) = engine().glyph_outline(run.font, g.id) else {
            continue;
        };
        let b = outline.bounding_box();
        x0 = x0.min(g.x + run.size_pt * b.x0);
        x1 = x1.max(g.x + run.size_pt * b.x1);
        y0 = y0.min(g.y + run.size_pt * b.y0);
        y1 = y1.max(g.y + run.size_pt * b.y1);
    }
    let middle = (y0 + y1) / 2.0;
    (
        leaf.transform.apply(Point::new(x0, middle)),
        leaf.transform.apply(Point::new(x1, middle)),
    )
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// The usual distance of a tick label from its axis is under a line of text; an anchored end must lie within this
/// distance of the edge of the plot area, so that a label is read with its tick and not left floating.
fn near() -> f64 {
    1.5 * fs()
}

/// How far the anchored end may lie from the line through its tick across the axis: the label is centred across its
/// thickness on the tick, and the ink of a run is not exactly centred in the box the layout measures.
fn across() -> f64 {
    0.3 * fs()
}

// Why (#153): matplotlib's `rotation=45, ha="right"` is the look authors reach for under a crowded categorical axis:
// each name rises to the right and ends at its tick, so the eye follows the name up to the tick it names. A label
// turned about its centre instead would sit between two ticks and be read against the wrong one. For a positive angle
// on the bottom axis the end of the text is the end nearest the tick; it lies just below the tick, the text runs off
// to the lower left, and none of it enters the plot area.
#[test]
fn a_positive_angle_anchors_the_end_of_each_x_label_below_its_tick() {
    let (fx, ax) = x_categories(45.0);
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, _) = axis_maps(&scene, ax);
    for (k, name) in NAMES.iter().enumerate() {
        let leaf = run(&scene, name);
        assert_reads_along(&leaf, 45.0);
        let tick = Point::new(x.to_figure(k as f64), plot.bottom());
        let (start, end) = ends(&leaf);
        assert!(
            distance(end, tick) < distance(start, tick),
            "{name}: the end of the text, {end:?}, is nearer the tick {tick:?} than its start, {start:?}"
        );
        assert_close(end.x, tick.x, across());
        assert!(
            end.y > plot.bottom() && end.y < plot.bottom() + near(),
            "{name}: {end:?}"
        );
        let b = leaf.bbox().unwrap();
        assert!(
            b.y >= plot.bottom() - 1e-6,
            "{name} at {b:?} stays below the plot"
        );
        assert!(
            b.right() < tick.x + across() + fs(),
            "{name} at {b:?} runs off to the left"
        );
    }
}

// Why (#153): a negative angle reads downwards to the right, so the end nearest the tick is the start of the text:
// the anchoring mirrors that of a positive angle, and the labels hang to the lower right of their ticks.
#[test]
fn a_negative_angle_anchors_the_start_of_each_x_label_below_its_tick() {
    let (fx, ax) = x_categories(-45.0);
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, _) = axis_maps(&scene, ax);
    for (k, name) in NAMES.iter().enumerate() {
        let leaf = run(&scene, name);
        assert_reads_along(&leaf, -45.0);
        let tick = Point::new(x.to_figure(k as f64), plot.bottom());
        let (start, end) = ends(&leaf);
        assert!(
            distance(start, tick) < distance(end, tick),
            "{name}: the start of the text, {start:?}, is nearer the tick {tick:?} than its end, {end:?}"
        );
        assert_close(start.x, tick.x, across());
        assert!(
            start.y > plot.bottom() && start.y < plot.bottom() + near(),
            "{name}: {start:?}"
        );
        let b = leaf.bbox().unwrap();
        assert!(
            b.y >= plot.bottom() - 1e-6,
            "{name} at {b:?} stays below the plot"
        );
        assert!(
            b.x > tick.x - across() - fs(),
            "{name} at {b:?} runs off to the right"
        );
    }
}

// Why (#153): at ±90° the labels stand vertically, the most compact way to fit many long names under one axis. A
// vertical label must stand centred on its tick, reading upwards (90°) or downwards (−90°), with the end nearest the
// tick at the top, so that a column of names lines up with the column of ticks.
#[test]
fn vertical_x_labels_are_centred_on_their_ticks_with_the_near_end_at_the_top() {
    for angle in [90.0, -90.0] {
        let (fx, ax) = x_categories(angle);
        let scene = compile_figure(&fx.build());
        let plot = axes_hit(&scene, ax).plot_rect;
        let (x, _) = axis_maps(&scene, ax);
        for (k, name) in NAMES.iter().enumerate() {
            let leaf = run(&scene, name);
            assert_reads_along(&leaf, angle);
            if angle > 0.0 {
                assert!(leaf.reads_upwards(), "{name} reads upwards at {angle}°");
            }
            let tick_x = x.to_figure(k as f64);
            let b = leaf.bbox().unwrap();
            assert_close(b.x + b.width / 2.0, tick_x, across());
            assert!(b.height > b.width, "{name} at {b:?} stands vertically");
            assert!(
                b.y >= plot.bottom() - 1e-6,
                "{name} at {b:?} stays below the plot"
            );
            assert!(
                b.y < plot.bottom() + near(),
                "{name} at {b:?} starts just below the plot"
            );
            let (start, end) = ends(&leaf);
            let top = if angle > 0.0 { end } else { start };
            assert!(
                top.y < start.y.max(end.y),
                "{name}: the near end is at the top"
            );
        }
    }
}

// Why (#153): a y axis on the left of the plot reads into its ticks from the left whatever the angle between −90° and
// 90°, so the end of the text is always the end nearest the tick: a label rising (positive) or falling (negative) to
// the right ends at its tick, beside it rather than above or below it, and stays clear of both the plot and the y
// axis label.
#[test]
fn a_turned_y_label_ends_beside_its_tick_for_either_sign_of_the_angle() {
    for angle in [30.0, -30.0, 60.0] {
        let (fx, ax) = y_categories(angle);
        let scene = compile_figure(&fx.build());
        let plot = axes_hit(&scene, ax).plot_rect;
        let (_, y) = axis_maps(&scene, ax);
        let mut leftmost = f64::INFINITY;
        for (k, name) in NAMES.iter().enumerate() {
            let leaf = run(&scene, name);
            assert_reads_along(&leaf, angle);
            let tick = Point::new(plot.x, y.to_figure(k as f64));
            let (start, end) = ends(&leaf);
            assert!(
                distance(end, tick) < distance(start, tick),
                "{angle}°, {name}: the end of the text is nearest the tick"
            );
            assert_close(end.y, tick.y, across());
            assert!(
                end.x < plot.x && end.x > plot.x - near(),
                "{angle}°, {name}: {end:?}"
            );
            // A turned label never comes closer to the plot than the usual label gap: at a steep angle the corner of
            // the end of the label would swing towards the plot by half the thickness times the sine of the angle,
            // so the label is moved away from the plot until its nearest point keeps the gap. No part of it,
            // descenders included, may therefore reach the plot edge.
            let b = leaf.bbox().unwrap();
            assert!(
                b.right() <= plot.x + 1e-6,
                "{angle}°, {name} at {b:?} stays left of the plot"
            );
            leftmost = leftmost.min(b.x);
        }
        let label = text_bbox(&leaves(&scene), "Case").expect("the y label is drawn");
        assert!(
            label.right() <= leftmost,
            "{angle}°: the y label at {label:?} lies left of the tick labels, which reach {leftmost}"
        );
    }
}

/// Returns the plot area of the axes of [`x_categories`] turned by `angle`, and the two ends of each name.
fn x_category_geometry(angle: f64) -> (Rect, Vec<(Point, Point)>) {
    let (fx, ax) = x_categories(angle);
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    (plot, NAMES.iter().map(|n| ends(&run(&scene, n))).collect())
}

// Why: an angle is a direction, so angles that differ by whole turns draw the same labels. An author who writes 405
// or −315 means 45, and one who writes −180 or 540 means 180 (an angle is normalised to (−180, 180]); a compiler that
// fed the raw angle to a sign test or to the choice of anchoring end would anchor such labels by the wrong end. The
// comparison allows for rounding in the sine and cosine, since the binding requirement is the drawing, not the bits.
#[test]
fn angles_that_differ_by_whole_turns_draw_the_same_labels() {
    let cases: [(f64, &[f64]); 4] = [
        (0.0, &[360.0, -360.0, 720.0]),
        (45.0, &[405.0, -315.0]),
        (90.0, &[450.0, -270.0]),
        (180.0, &[-180.0, 540.0, -540.0]),
    ];
    for (base, equivalents) in cases {
        let (plot, expected) = x_category_geometry(base);
        for &angle in equivalents {
            let (p, actual) = x_category_geometry(angle);
            for (v, w) in [(p.y, plot.y), (p.height, plot.height)] {
                assert_close(v, w, 1e-6);
            }
            for (name, ((s, e), (es, ee))) in NAMES.iter().zip(actual.iter().zip(&expected)) {
                let (d, n) = (turned(base), distance(*s, *e));
                assert!(
                    ((e.x - s.x) / n - d.x).abs() < 1e-6 && ((e.y - s.y) / n - d.y).abs() < 1e-6,
                    "{angle}°: {name} reads along {d:?}"
                );
                assert!(
                    distance(*s, *es) < 1e-6 && distance(*e, *ee) < 1e-6,
                    "{angle}° draws {name} as {base}° does"
                );
            }
        }
    }
}

// Why: at 180° the labels are level again, only upside down, so no end of the text is nearer its tick than the other.
// They are placed as level labels are: centred on their ticks below an x axis and beside a y axis, taking the same
// space as level labels, rather than anchored by one end and so hanging to one side of the tick they name.
#[test]
fn upside_down_labels_are_centred_on_their_ticks() {
    let (fx, ax) = x_categories(180.0);
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, _) = axis_maps(&scene, ax);
    for (k, name) in NAMES.iter().enumerate() {
        let leaf = run(&scene, name);
        assert_reads_along(&leaf, 180.0);
        let b = leaf.bbox().unwrap();
        assert_close(b.x + b.width / 2.0, x.to_figure(k as f64), across());
        assert!(
            b.y >= plot.bottom() - 1e-6 && b.y < plot.bottom() + near(),
            "{name} at {b:?} lies just below the plot"
        );
    }
    let (level_fx, level_ax) = x_categories(0.0);
    let level = axes_hit(&compile_figure(&level_fx.build()), level_ax).plot_rect;
    assert_close(plot.height, level.height, 0.01);

    let (fx, ax) = y_categories(180.0);
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (_, y) = axis_maps(&scene, ax);
    for (k, name) in NAMES.iter().enumerate() {
        let leaf = run(&scene, name);
        assert_reads_along(&leaf, 180.0);
        let b = leaf.bbox().unwrap();
        assert_close(b.y + b.height / 2.0, y.to_figure(k as f64), across());
        assert!(
            b.right() <= plot.x + 1e-6 && b.right() > plot.x - near(),
            "{name} at {b:?} lies just left of the plot"
        );
    }
}

// Why (#74): the case matrix of #74 is the composition both properties were made for. A matrix axes names its columns
// with long case names set vertically, and its rows with level category names; beside it a margin axes shares the
// rows and hides its x axis, because its columns are counts rather than values along an axis. The hidden axis must
// give its space to its plot area while the matrix reserves the full length of its vertical names, the level y names
// must stay exactly as level labels are drawn, and no text of either axes may cover the other or leave the page.
#[test]
fn a_case_matrix_with_vertical_column_names_beside_a_margin_without_an_x_axis() {
    let rows = ["xpanel", "xbl", "xsolve"];
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let matrix = fx.axes2d(0, 0);
    fx.line(matrix, &[0.0, 3.0], &[0.0, 2.0], None, |_| {});
    let margin = fx.axes2d(0, 1);
    fx.line(margin, &[0.0, 1.0], &[0.0, 2.0], None, |_| {});
    for (ax, x_hidden) in [(matrix, false), (margin, true)] {
        let axes = fx.ax(ax);
        axes.y.limits = Limits::Manual {
            min: -0.5,
            max: 2.5,
        };
        axes.y.ticks = Ticks::Values(vec![0.0, 1.0, 2.0]);
        axes.y.tick_labels = TickLabels::Text(rows.iter().map(|n| Text::plain(*n)).collect());
        axes.y.reverse = true;
        axes.x.visible = !x_hidden;
    }
    let axes = fx.ax(matrix);
    axes.x.limits = Limits::Manual {
        min: -0.5,
        max: 3.5,
    };
    axes.x.ticks = Ticks::Values(vec![0.0, 1.0, 2.0, 3.0]);
    axes.x.tick_labels = TickLabels::Text(NAMES.iter().map(|n| Text::plain(*n)).collect());
    axes.x.tick_label_angle = 90.0;
    let scene = compile_figure(&fx.build());
    let all = leaves(&scene);
    let (matrix_plot, margin_plot) = (
        axes_hit(&scene, matrix).plot_rect,
        axes_hit(&scene, margin).plot_rect,
    );

    // The matrix names its columns vertically, below its plot.
    for name in NAMES {
        let leaf = run(&scene, name);
        assert_reads_along(&leaf, 90.0);
        assert!(leaf.bbox().unwrap().y >= matrix_plot.bottom() - 1e-6);
    }
    // Both axes name their rows with level labels, drawn as level labels always are.
    for name in rows {
        let runs = runs_with_text(&all, name);
        assert_eq!(runs.len(), 2, "{name} names a row of each axes");
        for leaf in runs {
            assert_eq!(leaf.transform, Transform::IDENTITY, "{name} is level");
        }
    }
    // The margin draws nothing along its bottom. Its plot area still spans exactly the rows of the matrix, because
    // the margins of a row of axes are aligned: the space below the matrix that its vertical names need is reserved
    // below the margin too, so that every row of counts lies beside the row of the matrix it describes.
    assert_eq!(
        horizontal_spines(&scene, margin, margin_plot, margin_plot.bottom()),
        0
    );
    assert_eq!(x_tick_marks(&scene, margin, margin_plot), 0);
    assert_close(margin_plot.y, matrix_plot.y, 1e-6);
    assert_close(margin_plot.bottom(), matrix_plot.bottom(), 1e-6);

    // No text covers the other axes or leaves the page.
    let page = Rect::new(
        0.0,
        0.0,
        scene.display_list.width_pt,
        scene.display_list.height_pt,
    );
    let text = |id| -> Vec<Rect> {
        glyph_runs(&from_source(&all, id))
            .into_iter()
            .filter_map(|(l, _)| l.bbox())
            .collect()
    };
    let (matrix_text, margin_text) = (text(matrix), text(margin));
    assert!(!matrix_text.is_empty() && !margin_text.is_empty());
    for t in &matrix_text {
        assert!(inside(*t, page, 0.5), "{t:?} lies within {page:?}");
        assert!(disjoint(*t, margin_plot), "{t:?} enters the margin");
        for u in &margin_text {
            assert!(disjoint(*t, *u), "{t:?} covers margin text at {u:?}");
        }
    }
    for u in &margin_text {
        assert!(inside(*u, page, 0.5), "{u:?} lies within {page:?}");
        assert!(disjoint(*u, matrix_plot), "{u:?} enters the matrix");
    }
    assert!(
        !scene
            .warnings
            .iter()
            .any(|w| matches!(w.kind, SceneWarningKind::TextOutsidePage { .. })),
        "{:?}",
        scene.warnings
    );
}

// Why (#153): the layout must reserve the space the rotated labels take, which for a long name grows with the angle
// from one line of text at 0° to the whole length of the name at 90°. A layout that reserved the level height would
// push the rotated labels over the axis label or off the figure.
#[test]
fn the_space_below_the_plot_grows_with_the_angle_of_long_x_labels() {
    let heights: Vec<f64> = [0.0, 30.0, 60.0, 90.0]
        .iter()
        .map(|angle| {
            let (fx, ax) = x_categories(*angle);
            axes_hit(&compile_figure(&fx.build()), ax).plot_rect.height
        })
        .collect();
    for pair in heights.windows(2) {
        assert!(
            pair[1] < pair[0] - 1.0,
            "plot heights {heights:?} fall as the angle grows"
        );
    }
}

// Why (#153, #74): the axis label is set below the tick labels, and rotated labels reach further down than level
// ones; the label must move down with them so that it never overlaps a tick label, whatever the angle.
#[test]
fn rotated_x_labels_never_overlap_the_x_axis_label() {
    for angle in [30.0, 45.0, 60.0, 90.0, -45.0, -90.0] {
        let (fx, _) = x_categories(angle);
        let scene = compile_figure(&fx.build());
        let leaves = leaves(&scene);
        let label = text_bbox(&leaves, "Case").expect("the x label is drawn");
        for name in NAMES {
            let b = text_bbox(&leaves, name).unwrap();
            assert!(
                disjoint(b, label),
                "{angle}°: {name} at {b:?} overlaps the label at {label:?}"
            );
            assert!(
                label.y >= b.bottom(),
                "{angle}°: the label lies below {name}"
            );
        }
    }
}

// Why (#74): in the case matrix of #74 matplotlib clipped the longest rotated label at the edge of the page. Rotated
// labels at the ends of an axis overhang the plot area sideways (a positive angle to the left of the first tick, a
// negative one to the right of the last, a y label turned downwards above the top tick), and the layout must reserve
// that overhang, so that every label is drawn whole within the page and no text-outside-page warning is raised.
#[test]
fn rotated_labels_at_the_ends_of_an_axis_stay_within_the_page() {
    let cases: [(&str, f64, f64); 4] = [
        ("x at 45°", 45.0, 0.0),
        ("x at −45°", -45.0, 0.0),
        ("x at 90°", 90.0, 0.0),
        ("y at −30°", 0.0, -30.0),
    ];
    for (case, x_angle, y_angle) in cases {
        let mut fx = Fx::new();
        fx.fig.size.width_mm = 90.0;
        fx.fig.size.height_mm = 70.0;
        let ax = fx.axes2d(0, 0);
        fx.line(ax, &[0.0, 3.0], &[0.0, 3.0], None, |_| {});
        let axes = fx.ax(ax);
        // The first and last ticks lie on the edges of the plot area, where the overhang is largest.
        for axis in [&mut axes.x, &mut axes.y] {
            axis.limits = Limits::Manual { min: 0.0, max: 3.0 };
            axis.ticks = Ticks::Values(vec![0.0, 1.0, 2.0, 3.0]);
            axis.tick_labels = TickLabels::Text(NAMES.iter().map(|n| Text::plain(*n)).collect());
        }
        axes.x.tick_label_angle = x_angle;
        axes.y.tick_label_angle = y_angle;
        let scene = compile_figure(&fx.build());
        let page = Rect::new(
            0.0,
            0.0,
            scene.display_list.width_pt,
            scene.display_list.height_pt,
        );
        for name in NAMES {
            for leaf in runs_with_text(&leaves(&scene), name) {
                let b = leaf.bbox().unwrap();
                assert!(
                    inside(b, page, 0.5),
                    "{case}: {name} at {b:?} lies within {page:?}"
                );
            }
        }
        let outside: Vec<_> = scene
            .warnings
            .iter()
            .filter(|w| matches!(w.kind, SceneWarningKind::TextOutsidePage { .. }))
            .collect();
        assert!(outside.is_empty(), "{case}: {outside:?}");
    }
}

// Why (#153): rotated labels must not overlap a neighbouring axes either. The first label of the right-hand axes,
// turned 45° about its end at a tick on the left edge of its plot, reaches far to the left; the layout must make room
// so that it neither enters the plot area of the left-hand axes nor covers any of its text.
#[test]
fn rotated_labels_do_not_reach_into_a_neighbouring_axes() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    fx.line(left, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let right = fx.axes2d(0, 1);
    fx.line(right, &[0.0, 3.0], &[0.0, 1.0], None, |_| {});
    let axes = fx.ax(right);
    axes.x.limits = Limits::Manual { min: 0.0, max: 3.0 };
    axes.x.ticks = Ticks::Values(vec![0.0, 1.0, 2.0, 3.0]);
    axes.x.tick_labels = TickLabels::Text(NAMES.iter().map(|n| Text::plain(*n)).collect());
    axes.x.tick_label_angle = 45.0;
    let scene = compile_figure(&fx.build());
    let all = leaves(&scene);
    let left_plot = axes_hit(&scene, left).plot_rect;
    let left_text: Vec<Rect> = glyph_runs(&from_source(&all, left))
        .into_iter()
        .filter_map(|(l, _)| l.bbox())
        .collect();
    assert!(!left_text.is_empty(), "the left axes is labelled");
    for name in NAMES {
        let b = text_bbox(&all, name).unwrap();
        assert!(
            disjoint(b, left_plot),
            "{name} at {b:?} enters the left plot {left_plot:?}"
        );
        for t in &left_text {
            assert!(
                disjoint(b, *t),
                "{name} at {b:?} covers text of the left axes at {t:?}"
            );
        }
    }
}

// Why (#153): the angle applies to every tick label of the axis, the automatic numeric labels as well as category
// names, but not to the common exponent `×10^k`, which qualifies the whole axis rather than one tick and is read as a
// level annotation beside it.
#[test]
fn automatic_labels_are_rotated_and_the_common_exponent_is_not() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[1e5, 3e5], &[0.0, 1.0], None, |_| {});
    fx.ax(ax).x.tick_label_angle = 45.0;
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let all = leaves(&scene);
    let numbers: Vec<&Leaf> = glyph_runs(&all)
        .into_iter()
        .filter(|(l, g)| {
            g.font != FontId::Math
                && parse_number(&g.text).is_some()
                && l.bbox().is_some_and(|b| b.y >= plot.bottom() - 1e-6)
        })
        .map(|(l, _)| l)
        .collect();
    assert!(numbers.len() >= 2, "x tick labels are drawn");
    for leaf in numbers {
        assert_reads_along(leaf, 45.0);
    }
    let exponent: Vec<&Leaf> = glyph_runs(&all)
        .into_iter()
        .filter(|(_, g)| g.font == FontId::Math)
        .map(|(l, _)| l)
        .collect();
    assert!(!exponent.is_empty(), "the x labels carry a common exponent");
    for leaf in exponent {
        assert_reads_along(leaf, 0.0);
    }
}

// Why (#153): an angle has no effect when the labels are hidden; in particular the layout must not reserve the
// rotated extent of labels that are not drawn.
#[test]
fn an_angle_on_hidden_tick_labels_changes_nothing() {
    let (mut fx, ax) = x_categories(0.0);
    fx.ax(ax).x.tick_labels = TickLabels::Hidden;
    let level = compile_figure(&fx.fig.clone());
    fx.ax(ax).x.tick_label_angle = 60.0;
    let turned = compile_figure(&fx.build());
    assert_eq!(turned.display_list, level.display_list);
    assert_eq!(turned.hit_map, level.hit_map);
}

// Why: every figure written before the angle existed has an angle of 0 and must draw exactly as it did, so level
// labels are emitted as before, unwrapped by any rotation. An angle of −0, which compares equal to 0 but carries a
// negative sign, must not be mistaken for a negative angle and anchor the labels by their start.
#[test]
fn an_angle_of_zero_draws_level_labels_exactly_as_before() {
    let (mut fx, ax) = x_categories(0.0);
    let level = compile_figure(&fx.fig.clone());
    for name in NAMES {
        assert_eq!(
            run(&level, name).transform,
            Transform::IDENTITY,
            "{name} is not wrapped in a rotation"
        );
    }
    fx.ax(ax).x.tick_label_angle = -0.0;
    fx.ax(ax).y.tick_label_angle = -0.0;
    let negative_zero = compile_figure(&fx.build());
    assert_eq!(negative_zero.display_list, level.display_list);
    assert_eq!(negative_zero.hit_map, level.hit_map);
}

// ---------------------------------------------------------------------------------------------------------------------
// Three-dimensional axes
// ---------------------------------------------------------------------------------------------------------------------

/// A 3D axes whose tick labels are told apart by their text, as in the 3D tests: x labels have decimals, y labels
/// are hundreds and z labels are negative; the axes are labelled "East", "North" and "Height".
fn decorated_3d() -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
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
    axes.x.label = Some(Text::plain("East"));
    axes.y.label = Some(Text::plain("North"));
    axes.z.label = Some(Text::plain("Height"));
    (fx, ax)
}

/// Returns the values of the numeric tick labels of a 3D axes that satisfy `of_axis`.
fn labels_3d(scene: &Scene, of_axis: impl Fn(&str, f64) -> bool) -> Vec<f64> {
    glyph_runs(&leaves(scene))
        .into_iter()
        .filter_map(|(_, g)| Some((g.text.clone(), parse_number(&g.text)?)))
        .filter(|(t, v)| of_axis(t, *v))
        .map(|(_, v)| v)
        .collect()
}

fn x_3d(t: &str, v: f64) -> bool {
    t.contains('.') && (0.05..=0.95).contains(&v)
}

fn y_3d(t: &str, v: f64) -> bool {
    !t.contains('.') && (110.0..=890.0).contains(&v)
}

fn z_3d(t: &str, v: f64) -> bool {
    t.starts_with('\u{2212}') && (-95.0..=-15.0).contains(&v)
}

// Why: a hidden axis of a 3D axes draws no ticks, tick labels or label, while the box, whose edges belong to the box
// rather than to one axis, is drawn as for a visible axis; the other two axes are unaffected.
#[test]
fn a_hidden_3d_axis_draws_no_ticks_labels_or_label_and_keeps_the_box() {
    let (mut fx, ax) = decorated_3d();
    let shown = compile_figure(&fx.fig.clone());
    fx.ax(ax).x.visible = false;
    let hidden = compile_figure(&fx.build());

    assert!(
        !labels_3d(&shown, x_3d).is_empty(),
        "the shown x axis is labelled"
    );
    assert!(labels_3d(&hidden, x_3d).is_empty(), "no x tick labels");
    assert!(
        runs_with_text(&leaves(&hidden), "East").is_empty(),
        "no x label"
    );
    assert!(!labels_3d(&hidden, y_3d).is_empty(), "y tick labels stay");
    assert!(!labels_3d(&hidden, z_3d).is_empty(), "z tick labels stay");
    assert!(!runs_with_text(&leaves(&hidden), "North").is_empty());
    assert!(!runs_with_text(&leaves(&hidden), "Height").is_empty());

    // Box edges are long; tick marks are shorter than a line of text.
    let count = |scene: &Scene, long: bool| {
        segments(scene, ax)
            .into_iter()
            .filter(|&s| (length(s) > 5.0 * fs()) == long && length(s) > 0.0)
            .count()
    };
    assert_eq!(
        count(&hidden, true),
        count(&shown, true),
        "the box edges are all drawn"
    );
    assert!(
        count(&hidden, false) < count(&shown, false),
        "the x tick marks are gone"
    );
}

// Why: a 3D axes draws its tick labels along projected edges, where a rotation in the page has no agreed meaning, so
// it ignores the angle (validation warns of it) and draws exactly what it draws without one.
#[test]
fn a_3d_axes_ignores_the_tick_label_angle() {
    let (mut fx, ax) = decorated_3d();
    let level = compile_figure(&fx.fig.clone());
    let axes = fx.ax(ax);
    axes.x.tick_label_angle = 45.0;
    axes.y.tick_label_angle = 90.0;
    axes.z.tick_label_angle = -30.0;
    let turned = compile_figure(&fx.build());
    assert_eq!(turned.display_list, level.display_list);
    assert_eq!(turned.hit_map, level.hit_map);
}
