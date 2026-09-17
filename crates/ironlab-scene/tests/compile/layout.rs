//! Page size, tile layout and placement of titles, labels and tick labels.

use ironlab_ir::{Cell, Color, Legend, Projection};
use ironlab_scene::display::Rect;

use crate::common::{Fx, compile_figure, linspace, rgba, text};
use crate::probe::{
    assert_close, axes_hit, disjoint, glyph_runs, inside, leaves, runs_with_text, text_bbox,
    x_tick_labels, y_tick_labels,
};

const MM_TO_PT: f64 = 72.0 / 25.4;

/// A single 2D axes with a title, both axis labels and a line spanning x ∈ [0, 10],
/// y ∈ [−0.93, 0.97].
fn labelled_figure() -> (Fx, ironlab_ir::NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let x = linspace(0.0, 10.0, 101);
    let y: Vec<f64> = x.iter().map(|v| 0.02 + 0.95 * v.sin()).collect();
    fx.line(ax, &x, &y, None, |_| {});
    let axes = fx.ax(ax);
    axes.title = text("Overview");
    axes.x.label = text("Horizontal");
    axes.y.label = text("Vertical");
    (fx, ax)
}

// Why: the PDF MediaBox and the canvas aspect both come from the display list size, so it must be
// the physical figure size in points, painted with the figure's own background colour.
#[test]
fn page_size_is_figure_size_in_points_with_figure_background() {
    let (mut fx, _) = labelled_figure();
    fx.fig.size.width_mm = 120.0;
    fx.fig.size.height_mm = 80.0;
    fx.fig.background = Color::rgb(0.9, 0.95, 1.0);
    let scene = compile_figure(&fx.build());
    assert_close(scene.display_list.width_pt, 120.0 * MM_TO_PT, 1e-9);
    assert_close(scene.display_list.height_pt, 80.0 * MM_TO_PT, 1e-9);
    assert_eq!(
        scene.display_list.background,
        rgba(Color::rgb(0.9, 0.95, 1.0))
    );
}

// Why: the plot area must leave a margin for decorations on every side, otherwise labels are cut
// off at the page edge.
#[test]
fn plot_rect_lies_strictly_inside_figure() {
    let (fx, ax) = labelled_figure();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let dl = &scene.display_list;
    assert!(plot.width > 0.0 && plot.height > 0.0, "{plot:?}");
    assert!(plot.x > 0.0 && plot.y > 0.0, "{plot:?}");
    assert!(
        plot.right() < dl.width_pt && plot.bottom() < dl.height_pt,
        "{plot:?}"
    );
}

// Why: an axes title belongs above its data region, over the data it names, as in MATLAB, and must
// not cover data.
#[test]
fn axes_title_sits_above_plot_rect() {
    let (fx, ax) = labelled_figure();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let leaves = leaves(&scene);
    let title = text_bbox(&leaves, "Overview").expect("title glyphs are drawn");
    assert!(title.bottom() <= plot.y, "title {title:?} plot {plot:?}");
    let middle = title.x + title.width / 2.0;
    assert!(
        middle > plot.x && middle < plot.right(),
        "title {title:?} is over the plot {plot:?}"
    );
    assert!(
        runs_with_text(&leaves, "Overview")
            .iter()
            .all(|l| l.source == Some(ax)),
        "the axes title names the axes as its source"
    );
}

// Why: the vertical stacking below the plot is plot, then tick labels, then the axis label; any
// other order makes the label ambiguous or overlapping.
#[test]
fn xlabel_sits_below_x_tick_labels_which_sit_below_plot() {
    let (fx, ax) = labelled_figure();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let leaves = leaves(&scene);
    let ticks = x_tick_labels(&leaves, plot);
    assert!(!ticks.is_empty(), "x tick labels are drawn below the plot");
    let lowest_tick = ticks
        .iter()
        .map(|t| t.bbox.bottom())
        .fold(f64::MIN, f64::max);
    let xlabel = text_bbox(&leaves, "Horizontal").expect("xlabel glyphs are drawn");
    assert!(
        xlabel.y >= lowest_tick,
        "xlabel {xlabel:?} below ticks at {lowest_tick}"
    );
}

// Why: a y label reads bottom-to-top beside the axis, which requires a quarter-turn rotation
// group, and it must clear the y tick labels.
#[test]
fn ylabel_is_rotated_and_left_of_y_tick_labels() {
    let (fx, ax) = labelled_figure();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let leaves = leaves(&scene);
    let runs = runs_with_text(&leaves, "Vertical");
    assert!(!runs.is_empty(), "ylabel glyphs are drawn");
    assert!(
        runs.iter().all(|l| l.reads_upwards()),
        "ylabel runs are inside a group rotated to read upwards"
    );
    let ylabel = text_bbox(&leaves, "Vertical").unwrap();
    assert!(
        ylabel.height > ylabel.width,
        "rotated label is taller than wide: {ylabel:?}"
    );
    let ticks = y_tick_labels(&leaves, plot);
    assert!(
        !ticks.is_empty(),
        "y tick labels are drawn left of the plot"
    );
    let leftmost_tick = ticks.iter().map(|t| t.bbox.x).fold(f64::MAX, f64::min);
    assert!(
        ylabel.right() <= leftmost_tick,
        "ylabel {ylabel:?} left of ticks at {leftmost_tick}"
    );
}

// Why: layout subtracts measured decoration extents; overlapping text means a measurement was
// ignored, and every tick label must sit outside the data region.
#[test]
fn title_labels_and_tick_labels_do_not_overlap() {
    let (fx, ax) = labelled_figure();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let leaves = leaves(&scene);
    let xt = x_tick_labels(&leaves, plot);
    let yt = y_tick_labels(&leaves, plot);
    let numeric_runs = glyph_runs(&leaves)
        .iter()
        .filter(|(_, g)| crate::probe::parse_number(&g.text).is_some())
        .count();
    assert_eq!(
        xt.len() + yt.len(),
        numeric_runs,
        "every tick label lies either below or left of the plot rect"
    );
    let mut boxes: Vec<(String, Rect)> = vec![
        ("title".into(), text_bbox(&leaves, "Overview").unwrap()),
        ("xlabel".into(), text_bbox(&leaves, "Horizontal").unwrap()),
        ("ylabel".into(), text_bbox(&leaves, "Vertical").unwrap()),
    ];
    boxes.extend(xt.iter().map(|t| (format!("x tick {}", t.text), t.bbox)));
    boxes.extend(yt.iter().map(|t| (format!("y tick {}", t.text), t.bbox)));
    for (i, (na, a)) in boxes.iter().enumerate() {
        for (nb, b) in &boxes[i + 1..] {
            assert!(disjoint(*a, *b), "{na} {a:?} overlaps {nb} {b:?}");
        }
    }
}

/// A 2×2 tile layout with one line per axes, returned in row-major order.
fn tiles_2x2() -> (Fx, [ironlab_ir::NodeId; 4]) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    let ids = [(0, 0), (0, 1), (1, 0), (1, 1)].map(|(r, c)| {
        let ax = fx.axes2d(r, c);
        fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
        fx.ax(ax).title = text(&format!("Tile{r}{c}"));
        ax
    });
    (fx, ids)
}

// Why: tiles must not overlap, and their reading order must follow the layout's rows and columns.
#[test]
fn tile_plot_rects_are_disjoint_and_follow_rows_and_columns() {
    let (fx, [a, b, c, d]) = tiles_2x2();
    let scene = compile_figure(&fx.build());
    let r = [a, b, c, d].map(|id| axes_hit(&scene, id).plot_rect);
    for i in 0..4 {
        for j in i + 1..4 {
            assert!(
                disjoint(r[i], r[j]),
                "tiles {i} and {j} overlap: {:?} {:?}",
                r[i],
                r[j]
            );
        }
    }
    assert!(
        r[0].right() < r[1].x && r[2].right() < r[3].x,
        "column 0 is left of column 1"
    );
    assert!(
        r[0].bottom() < r[2].y && r[1].bottom() < r[3].y,
        "row 0 is above row 1"
    );
}

// Why: a cell spanning two columns must stretch across both columns of the tiles below it.
#[test]
fn cell_spanning_two_columns_covers_both_columns() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    let wide = fx.axes_in(
        Cell {
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 2,
        },
        Projection::TwoD,
    );
    let left = fx.axes2d(1, 0);
    let right = fx.axes2d(1, 1);
    for ax in [wide, left, right] {
        fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    }
    let scene = compile_figure(&fx.build());
    let [w, l, r] = [wide, left, right].map(|id| axes_hit(&scene, id).plot_rect);
    assert!(
        w.x <= l.x + 1.0,
        "wide {w:?} starts at the left column {l:?}"
    );
    assert!(
        w.right() >= r.right() - 1.0,
        "wide {w:?} ends at the right column {r:?}"
    );
    assert!(
        w.width > l.width + r.width,
        "wide tile spans the gap between columns"
    );
    assert!(w.bottom() < l.y.min(r.y), "wide tile is in the upper row");
}

// Why: the figure title (sgtitle) heads the whole figure, above every axes and its title.
#[test]
fn figure_title_sits_above_all_axes_and_their_titles() {
    let (mut fx, ids) = tiles_2x2();
    fx.fig.title = text("Summary");
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let sg = text_bbox(&leaves, "Summary").expect("figure title glyphs are drawn");
    let sg_runs = runs_with_text(&leaves, "Summary");
    assert!(
        sg_runs
            .iter()
            .all(|l| l.source == Some(ironlab_ir::NodeId(1))),
        "figure title items name the figure as their source"
    );
    for id in ids {
        let plot = axes_hit(&scene, id).plot_rect;
        assert!(sg.bottom() <= plot.y, "sgtitle {sg:?} above plot {plot:?}");
    }
    for title in ["Tile00", "Tile01"] {
        let t = text_bbox(&leaves, title).expect("axes titles are drawn");
        assert!(sg.bottom() <= t.y, "sgtitle {sg:?} above axes title {t:?}");
    }
}

// Why: nothing may spill off the page, where the PDF would silently crop it.
#[test]
fn every_item_lies_within_figure_bounds() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    fx.fig.title = text("Everything");
    let left = fx.axes2d(0, 0);
    let x = linspace(-3.0, 3.0, 31);
    let y: Vec<f64> = x.iter().map(|v| v * v).collect();
    fx.line(left, &x, &y, None, |l| l.display_name = text("Parabola"));
    fx.scatter(left, &x, &y, None, |s, _| {
        s.display_name = text("$y = x^2$")
    });
    let ax = fx.ax(left);
    ax.title = text("Left");
    ax.x.label = text("$x$ position");
    ax.y.label = text("$y$ value");
    ax.legend = Some(Legend::default());
    let right = fx.axes2d(0, 1);
    let g = linspace(-1.0, 1.0, 11);
    fx.contour(right, &g, &g, |a, b| a * b, |c| c.fill = true);
    fx.ax(right).title = text("Right");
    let scene = compile_figure(&fx.build());
    let page = Rect::new(
        0.0,
        0.0,
        scene.display_list.width_pt,
        scene.display_list.height_pt,
    );
    let leaves = leaves(&scene);
    assert!(!leaves.is_empty());
    for leaf in &leaves {
        if let Some(b) = leaf.visible_bbox() {
            assert!(
                inside(b, page, 0.5),
                "item from {:?} at {b:?} leaves the page",
                leaf.source
            );
        }
    }
}
