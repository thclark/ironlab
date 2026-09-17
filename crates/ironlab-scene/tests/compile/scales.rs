//! Axis limits, scales, tick labels, links and grid lines.

use ironlab_ir::{AxisLink, Dimension, Limits, NodeId, Scale};
use ironlab_scene::display::Point;
use ironlab_text::FontId;

use crate::common::{Fx, compile_figure, linspace};
use crate::probe::{
    Leaf, assert_close, axes_hit, axis_maps, from_source, glyph_runs, leaves, x_tick_labels,
    y_tick_labels,
};

/// One 2D axes holding a line over x ∈ [0, 10] and y ∈ [−0.93, 0.97].
fn sine_axes() -> (Fx, NodeId, NodeId, Vec<f64>, Vec<f64>) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let x = linspace(0.0, 10.0, 101);
    let y: Vec<f64> = x.iter().map(|v| 0.02 + 0.95 * v.sin()).collect();
    let line = fx.line(ax, &x, &y, None, |_| {});
    (fx, ax, line, x, y)
}

/// Returns every vertex of the stroked paths of `id`, in figure space.
fn stroke_vertices(leaves: &[Leaf], id: NodeId) -> Vec<Point> {
    from_source(leaves, id)
        .iter()
        .filter(|l| l.path().is_some_and(|p| p.stroke.is_some()))
        .flat_map(|l| l.subpaths().into_iter().flatten())
        .collect()
}

// Why: automatic limits round outward to nice tick values, as MATLAB does, and the hit map must
// publish that mapping with min at the left/bottom edge so pointer interaction inverts drawing.
#[test]
fn auto_limits_snap_to_nice_values_and_map_to_plot_edges() {
    let (fx, ax, ..) = sine_axes();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, y) = axis_maps(&scene, ax);
    assert_eq!((x.min, x.max), (0.0, 10.0));
    assert_eq!((y.min, y.max), (-1.0, 1.0));
    assert!(!x.log && !y.log);
    assert_close(x.start, plot.x, 1e-9);
    assert_close(x.end, plot.right(), 1e-9);
    assert_close(y.start, plot.bottom(), 1e-9);
    assert_close(y.end, plot.y, 1e-9);
}

// Why: the drawn line and the published axis maps must agree, otherwise pan, zoom and future
// datatips point at the wrong data.
#[test]
fn line_vertices_map_back_to_their_data_through_the_hit_map() {
    let (fx, ax, line, xs, ys) = sine_axes();
    let scene = compile_figure(&fx.build());
    let (x, y) = axis_maps(&scene, ax);
    let vertices = stroke_vertices(&leaves(&scene), line);
    assert_eq!(vertices.len(), xs.len(), "one vertex per data point");
    for (p, (dx, dy)) in vertices.iter().zip(xs.iter().zip(&ys)) {
        assert_close(x.to_data(p.x), *dx, 1e-6);
        assert_close(y.to_data(p.y), *dy, 1e-6);
    }
}

// Why: manual limits are the user's (or the viewer's pan/zoom) decision and must be used verbatim,
// with data outside them clipped to the plot area rather than drawn over decorations.
#[test]
fn manual_limits_are_respected_and_data_is_clipped_to_plot_rect() {
    let (mut fx, ax, line, ..) = sine_axes();
    fx.ax(ax).x.limits = Limits::Manual { min: 2.0, max: 4.0 };
    fx.ax(ax).y.limits = Limits::Manual {
        min: -0.5,
        max: 0.5,
    };
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, y) = axis_maps(&scene, ax);
    assert_eq!((x.min, x.max), (2.0, 4.0));
    assert_eq!((y.min, y.max), (-0.5, 0.5));
    let leaves = leaves(&scene);
    let strokes = from_source(&leaves, line);
    assert!(!strokes.is_empty());
    for leaf in strokes {
        let clip = leaf.clip.expect("line is drawn inside a clipped group");
        assert_close(clip.x, plot.x, 1e-6);
        assert_close(clip.y, plot.y, 1e-6);
        assert_close(clip.width, plot.width, 1e-6);
        assert_close(clip.height, plot.height, 1e-6);
    }
}

// Why: tick labels must be an evenly spaced nice sequence that reaches both automatic limits,
// printed without spurious decimals.
#[test]
fn x_tick_labels_are_a_nice_sequence_spanning_the_limits() {
    let (fx, ax, ..) = sine_axes();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let mut labels = x_tick_labels(&leaves(&scene), plot);
    labels.sort_by(|a, b| a.bbox.x.total_cmp(&b.bbox.x));
    let texts: Vec<&str> = labels.iter().map(|l| l.text.as_str()).collect();
    assert!(labels.len() >= 3, "{texts:?}");
    assert_eq!(texts.first(), Some(&"0"));
    assert_eq!(texts.last(), Some(&"10"));
    assert!(
        texts.iter().all(|t| !t.contains('.')),
        "integer ticks: {texts:?}"
    );
    let step = labels[1].value - labels[0].value;
    assert!([1.0, 2.0, 5.0].contains(&step), "nice step, got {step}");
    for pair in labels.windows(2) {
        assert_close(pair[1].value - pair[0].value, step, 1e-12);
    }
}

// Why: publication typography uses U+2212 MINUS SIGN, never a hyphen, for negative tick labels. The
// number of decimals depends on the tick step, so both "−1" and "−1.0" are acceptable.
#[test]
fn negative_tick_labels_use_the_minus_sign() {
    let (fx, ax, ..) = sine_axes();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let labels = y_tick_labels(&leaves(&scene), plot);
    let texts: Vec<&str> = labels.iter().map(|l| l.text.as_str()).collect();
    assert!(
        labels
            .iter()
            .any(|l| l.value == -1.0 && l.text.starts_with('\u{2212}')),
        "{texts:?}"
    );
    assert!(texts.iter().all(|t| !t.contains('-')), "{texts:?}");
    assert!(
        labels
            .iter()
            .filter(|l| l.value < 0.0)
            .all(|l| l.text.starts_with('\u{2212}')),
        "{texts:?}"
    );
}

// Why: a value is read off an axis by interpolating between its labelled ticks, so a y axis needs
// MATLAB's density of labels: in a default figure, data spanning [−1, 1] is labelled every 0.5 or
// finer, not only at −1, 0 and 1.
#[test]
fn y_axis_has_matlab_like_tick_density() {
    let (fx, ax, ..) = sine_axes();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let labels = y_tick_labels(&leaves(&scene), plot);
    let texts: Vec<&str> = labels.iter().map(|l| l.text.as_str()).collect();
    assert!(labels.len() >= 5, "{texts:?}");
}

// Why: manual limits that are not multiples of a coarse step (the scatter gallery figure uses
// [−1.6, 1.6]) must still show several labelled ticks; a step chosen too coarse leaves a single
// label at zero, and the axis cannot be read at all.
#[test]
fn manual_limits_off_the_tick_grid_still_show_several_labels() {
    let mut fx = Fx::new();
    fx.fig.size.width_mm = 120.0;
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[-1.5, 1.5], &[-1.5, 1.5], None, |_| {});
    let limits = Limits::Manual {
        min: -1.6,
        max: 1.6,
    };
    fx.ax(ax).x.limits = limits;
    fx.ax(ax).y.limits = limits;
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let leaves = leaves(&scene);
    for (name, labels) in [
        ("x", x_tick_labels(&leaves, plot)),
        ("y", y_tick_labels(&leaves, plot)),
    ] {
        let texts: Vec<&str> = labels.iter().map(|l| l.text.as_str()).collect();
        assert!(labels.len() >= 5, "{name} labels: {texts:?}");
    }
}

/// Returns the x tick labels of a single axes holding a line over `x`, in a figure `width_mm` wide,
/// sorted from left to right.
fn x_labels_at_width(x: [f64; 2], width_mm: f64) -> (Vec<crate::probe::NumericLabel>, f64) {
    let mut fx = Fx::new();
    fx.fig.size.width_mm = width_mm;
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &x, &[0.0, 1.0], None, |_| {});
    let font_size = fx.fig.font_size_pt;
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let mut labels = x_tick_labels(&leaves(&scene), plot);
    labels.sort_by(|a, b| a.bbox.x.total_cmp(&b.bbox.x));
    (labels, font_size)
}

// Why: the number of x ticks depends on how wide the labels are, as in MATLAB: wide labels in a
// narrow axes must be thinned so that neighbouring labels keep a clear gap and never run together,
// while a wide axes with the same data shows more ticks.
#[test]
fn x_tick_density_follows_label_width_and_axis_width() {
    let (narrow, font_size) = x_labels_at_width([1000.5, 1009.5], 60.0);
    let (wide, _) = x_labels_at_width([1000.5, 1009.5], 240.0);
    let texts = |l: &[crate::probe::NumericLabel]| -> Vec<String> {
        l.iter().map(|l| l.text.clone()).collect()
    };
    assert!(narrow.len() >= 2, "{:?}", texts(&narrow));
    for pair in narrow.windows(2) {
        let gap = pair[1].bbox.x - pair[0].bbox.right();
        assert!(
            gap >= font_size,
            "labels {:?} are {gap} pt apart",
            texts(&narrow)
        );
    }
    assert!(
        wide.len() > narrow.len(),
        "wide {:?}, narrow {:?}",
        texts(&wide),
        texts(&narrow)
    );
}

/// A loglog axes with a line through four decades on both axes.
fn loglog_axes() -> (Fx, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let v = [1.0, 10.0, 100.0, 1000.0];
    let line = fx.line(ax, &v, &v, None, |_| {});
    fx.ax(ax).x.scale = Scale::Log;
    fx.ax(ax).y.scale = Scale::Log;
    (fx, ax, line)
}

// Why: decade labels on log axes are typeset as math (10 with a raised exponent), and exact-decade
// data keeps its decades as the automatic limits.
#[test]
fn log_axis_labels_are_math_decades() {
    let (fx, ax, _) = loglog_axes();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, _) = axis_maps(&scene, ax);
    assert!(x.log);
    assert_eq!((x.min, x.max), (1.0, 1000.0));
    let leaves = leaves(&scene);
    assert!(
        x_tick_labels(&leaves, plot).is_empty(),
        "no plain-number decade labels"
    );
    let below: Vec<_> = glyph_runs(&leaves)
        .into_iter()
        .filter(|(l, g)| {
            g.font == ironlab_text::FontId::Math && l.bbox().is_some_and(|b| b.y >= plot.bottom())
        })
        .map(|(_, g)| g)
        .collect();
    let base_size = below.iter().map(|g| g.size_pt).fold(0.0, f64::max);
    let bases = below
        .iter()
        .filter(|g| g.size_pt == base_size && g.text == "10");
    assert!(bases.count() >= 2, "at least two '10' bases below the plot");
    let mut exponents: Vec<i32> = below
        .iter()
        .filter(|g| g.size_pt < base_size)
        .filter_map(|g| g.text.replace('\u{2212}', "-").parse().ok())
        .collect();
    exponents.sort();
    assert!(
        exponents.len() >= 2,
        "exponents are smaller runs: {exponents:?}"
    );
    assert!(exponents.contains(&0), "10^0 is labelled: {exponents:?}");
    let stride = exponents[1] - exponents[0];
    assert!(
        exponents.windows(2).all(|w| w[1] - w[0] == stride),
        "{exponents:?}"
    );
}

// Why: a log axis must place successive decades at equal distances, and the published log axis maps
// must invert that placement; this checks the data mapping, not merely the label text.
#[test]
fn log_axis_places_decades_evenly() {
    let (fx, ax, line) = loglog_axes();
    let scene = compile_figure(&fx.build());
    let (xmap, ymap) = axis_maps(&scene, ax);
    let vertices = stroke_vertices(&leaves(&scene), line);
    assert_eq!(vertices.len(), 4);
    for (p, v) in vertices.iter().zip([1.0, 10.0, 100.0, 1000.0]) {
        assert_close(xmap.to_data(p.x) / v, 1.0, 1e-9);
        assert_close(ymap.to_data(p.y) / v, 1.0, 1e-9);
    }
    let dx = vertices[1].x - vertices[0].x;
    let dy = vertices[1].y - vertices[0].y;
    assert!(dx > 0.0 && dy < 0.0, "increasing data goes right and up");
    for pair in vertices.windows(2) {
        assert_close(pair[1].x - pair[0].x, dx, 1e-6);
        assert_close(pair[1].y - pair[0].y, dy, 1e-6);
    }
}

// Why: non-positive values have no position on a log axis; they must be dropped rather than
// producing NaN geometry, and the user must be told which artist lost data.
#[test]
fn non_positive_data_on_log_axis_is_dropped_with_a_warning() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let line = fx.line(
        ax,
        &[-1.0, 0.0, 1.0, 10.0, 100.0],
        &[1.0, 2.0, 3.0, 4.0, 5.0],
        None,
        |_| {},
    );
    fx.ax(ax).x.scale = Scale::Log;
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    assert_eq!(
        (xmap.min, xmap.max),
        (1.0, 100.0),
        "dropped values do not widen the limits"
    );
    let vertices = stroke_vertices(&leaves(&scene), line);
    assert_eq!(vertices.len(), 3, "only the positive points are drawn");
    assert!(vertices.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    assert!(
        scene.warnings.iter().any(|w| w.node == Some(line)),
        "a warning names the line: {:?}",
        scene.warnings
    );
}

// Why: linked axes must show the same range, so automatic limits are computed over the union of the
// data of the whole link group (not taken from any one member), only along the linked dimension,
// while an unlinked axes keeps its own.
#[test]
fn linked_auto_limits_cover_the_union_of_group_data() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 3;
    let a = fx.axes2d(0, 0);
    let b = fx.axes2d(0, 1);
    let c = fx.axes2d(0, 2);
    // With any tick target above two, a alone would take x limits narrower than [−10, 10] (such as
    // [−10, 5]) and b alone would take [0, 10].
    fx.line(a, &[-10.0, 1.0], &[0.0, 1.0], None, |_| {});
    fx.line(b, &[0.0, 10.0], &[0.0, 10.0], None, |_| {});
    fx.line(c, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    fx.fig.links.push(AxisLink {
        dimension: Dimension::X,
        axes: vec![a, b],
    });
    let scene = compile_figure(&fx.build());
    let (ax_a, ay_a) = axis_maps(&scene, a);
    let (ax_b, ay_b) = axis_maps(&scene, b);
    let (ax_c, _) = axis_maps(&scene, c);
    assert_eq!((ax_a.min, ax_a.max), (-10.0, 10.0));
    assert_eq!((ax_b.min, ax_b.max), (-10.0, 10.0));
    assert_eq!((ay_a.min, ay_a.max), (0.0, 1.0), "y is not linked");
    assert_eq!((ay_b.min, ay_b.max), (0.0, 10.0), "y is not linked");
    assert_eq!(
        (ax_c.min, ax_c.max),
        (0.0, 1.0),
        "unlinked axes is unaffected"
    );
}

// Why: hiding a series from its legend entry must not make the axes jump to new limits, so a hidden
// artist's data still counts towards automatic limits.
#[test]
fn hidden_artist_data_still_determines_automatic_limits() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    fx.line(ax, &[0.0, 10.0], &[0.0, 1.0], None, |l| l.visible = false);
    let scene = compile_figure(&fx.build());
    let (x, _) = axis_maps(&scene, ax);
    assert_eq!((x.min, x.max), (0.0, 10.0));
}

// Why: labels such as 300000 are hard to compare and crowd the axis. As in MATLAB, when the ticks need
// a power of ten, the labels show mantissas and one ×10^k label at the top of a y axis states the
// factor; an axis whose ticks do not need one shows no such label.
#[test]
fn large_y_ticks_show_mantissas_and_a_common_exponent_above_the_axis() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 10.0], &[0.0, 3e5], None, |_| {});
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, y) = axis_maps(&scene, ax);
    assert_eq!((y.min, y.max), (0.0, 3e5));
    let leaves = leaves(&scene);

    let mut labels = y_tick_labels(&leaves, plot);
    assert!(labels.len() >= 2, "y tick labels are drawn");
    labels.sort_by(|a, b| b.bbox.y.total_cmp(&a.bbox.y));
    let values: Vec<f64> = labels.iter().map(|l| l.value).collect();
    assert_eq!(values.first(), Some(&0.0), "{values:?}");
    assert_eq!(
        values.last(),
        Some(&3.0),
        "the top label is a mantissa: {values:?}"
    );
    assert!(
        values.windows(2).all(|w| w[0] < w[1]),
        "mantissas increase up the axis: {values:?}"
    );
    let font_size = 9.0;
    for label in &labels {
        let tick = y.to_figure(label.value * 1e5);
        let middle = label.bbox.y + label.bbox.height / 2.0;
        assert!(
            (middle - tick).abs() < font_size,
            "label {} at {middle} beside its tick at {tick}",
            label.text
        );
    }

    let exponent_runs: Vec<_> = glyph_runs(&leaves)
        .into_iter()
        .filter(|(l, g)| {
            g.font == FontId::Math && l.bbox().is_some_and(|b| b.bottom() <= plot.y + 1e-6)
        })
        .collect();
    let texts: String = exponent_runs.iter().map(|(_, g)| g.text.as_str()).collect();
    assert!(
        texts.contains('\u{00D7}'),
        "a multiplication sign lies above the plot: {texts:?}"
    );
    let base = exponent_runs
        .iter()
        .find(|(_, g)| g.text.contains("10"))
        .unwrap_or_else(|| panic!("a base 10 lies above the plot: {texts:?}"));
    assert!(
        exponent_runs
            .iter()
            .any(|(_, g)| g.text == "5" && g.size_pt < base.1.size_pt),
        "the exponent 5 is a smaller math run: {texts:?}"
    );
    let label_box = exponent_runs
        .iter()
        .filter_map(|(l, _)| l.bbox())
        .reduce(crate::probe::union)
        .unwrap();
    assert!(
        label_box.x < plot.x + plot.width / 2.0,
        "the exponent label is at the y axis end of the plot: {label_box:?} {plot:?}"
    );

    let xlabels = x_tick_labels(&leaves, plot);
    assert_eq!(x.max, 10.0);
    assert!(
        xlabels.iter().any(|l| l.value == 10.0),
        "x ticks need no exponent"
    );
    assert_eq!(
        glyph_runs(&leaves)
            .iter()
            .filter(|(_, g)| g.text.contains('\u{00D7}'))
            .count(),
        1,
        "only one exponent label is drawn"
    );
}

/// Returns the data x of every vertical segment that spans the full height of the plot rect.
fn full_height_verticals(leaves: &[Leaf], scene: &ironlab_scene::Scene, ax: NodeId) -> Vec<f64> {
    let plot = axes_hit(scene, ax).plot_rect;
    let (x, _) = axis_maps(scene, ax);
    leaves
        .iter()
        .flat_map(|l| l.line_segments())
        .filter(|(p, q)| {
            (p.x - q.x).abs() < 1e-6
                && p.y.min(q.y) <= plot.y + 0.5
                && p.y.max(q.y) >= plot.bottom() - 0.5
                && p.x > plot.x + 0.5
                && p.x < plot.right() - 0.5
        })
        .map(|(p, _)| x.to_data(p.x))
        .collect()
}

// Why: grid lines exist to line up with the labelled ticks, so each interior x tick needs a
// full-height line at its value, and none may appear when the grid is off.
#[test]
fn grid_lines_are_drawn_at_major_ticks_only_when_enabled() {
    let (mut fx, ax, ..) = sine_axes();
    let off = compile_figure(&fx.fig);
    assert!(full_height_verticals(&leaves(&off), &off, ax).is_empty());

    fx.ax(ax).x.grid = true;
    fx.ax(ax).y.grid = true;
    let on = compile_figure(&fx.build());
    let plot = axes_hit(&on, ax).plot_rect;
    let leaves = leaves(&on);
    let grid_x = full_height_verticals(&leaves, &on, ax);
    let (x, _) = axis_maps(&on, ax);
    let interior: Vec<f64> = x_tick_labels(&leaves, plot)
        .iter()
        .map(|l| l.value)
        .filter(|v| *v > x.min && *v < x.max)
        .collect();
    assert!(!interior.is_empty());
    for v in &interior {
        assert!(
            grid_x.iter().any(|g| (g - v).abs() < 1e-6),
            "grid line at tick {v}, grid at {grid_x:?}"
        );
    }
    for g in &grid_x {
        assert!(
            interior.iter().any(|v| (g - v).abs() < 1e-6),
            "grid line at {g} has no tick"
        );
    }
}
