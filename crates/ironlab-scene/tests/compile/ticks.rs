//! Tick placement and tick labels: automatic ticks on short axes, regular steps, explicit values, explicit labels and
//! hidden labels, and the space the layout reserves for them.

use ironlab_ir::{Limits, NodeId, Scale, Text, TickLabels, Ticks};
use ironlab_scene::Scene;
use ironlab_scene::display::Rect;
use ironlab_text::FontId;

use crate::common::{Fx, cell, compile_figure};
use crate::probe::{
    NumericLabel, assert_close, axes_hit, axis_maps, from_source, glyph_runs, leaves, text_bbox,
    x_tick_labels, y_tick_labels,
};

/// One 2D axes holding a line from (0, 0) to (1, 1), with manual limits x ∈ [−0.2, 1.2] and y ∈ [−0.4, 0.4].
fn unit_axes() -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let axes = fx.ax(ax);
    axes.x.limits = Limits::Manual {
        min: -0.2,
        max: 1.2,
    };
    axes.y.limits = Limits::Manual {
        min: -0.4,
        max: 0.4,
    };
    (fx, ax)
}

fn values(labels: &[NumericLabel]) -> Vec<f64> {
    let mut values: Vec<f64> = labels.iter().map(|l| l.value).collect();
    values.sort_by(f64::total_cmp);
    values
}

fn texts(labels: &[NumericLabel]) -> Vec<String> {
    let mut labels = labels.to_vec();
    labels.sort_by(|a, b| a.value.total_cmp(&b.value));
    labels.into_iter().map(|l| l.text).collect()
}

/// Returns the y coordinate of the centre of the bounding box of the glyph runs whose text is exactly `text`.
fn centre_y(scene: &Scene, text: &str) -> f64 {
    let b = text_bbox(&leaves(scene), text).unwrap_or_else(|| panic!("{text:?} is drawn"));
    b.y + b.height / 2.0
}

// Why (#77): a row of a stacked sheet is short, and its automatic ticks were chosen for so few intervals that a C_D
// row from −0.15 to 0.46 got a single label, 0.0, from which no value can be read. A y axis must always carry at
// least three labelled ticks when its range holds them at a round step.
#[test]
fn a_short_y_axis_with_manual_limits_has_at_least_three_labelled_ticks() {
    let mut fx = Fx::new();
    fx.fig.size.height_mm = 160.0;
    fx.fig.layout.rows = 8;
    let ax = fx.axes2d(0, 0);
    for row in 1..8 {
        let other = fx.axes2d(row, 0);
        fx.line(other, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    }
    fx.line(ax, &[0.0, 1.0], &[-0.15, 0.46], None, |_| {});
    fx.ax(ax).y.limits = Limits::Manual {
        min: -0.15,
        max: 0.46,
    };
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    // The axes below share the column, so only the labels this axes draws are its own.
    let labels = y_tick_labels(&from_source(&leaves(&scene), ax), plot);
    assert!(labels.len() >= 3, "{:?}", texts(&labels));
    for v in values(&labels) {
        assert!((-0.15..=0.46).contains(&v), "{v} lies within the limits");
    }
    assert_labels_apart(&labels);
}

/// Asserts that no two labels overlap, so that more ticks are never bought with unreadable labels.
#[track_caller]
fn assert_labels_apart(labels: &[NumericLabel]) {
    for (i, a) in labels.iter().enumerate() {
        for b in &labels[i + 1..] {
            assert!(
                crate::probe::disjoint(a.bbox, b.bbox),
                "{} at {:?} overlaps {} at {:?}",
                a.text,
                a.bbox,
                b.text,
                b.bbox
            );
        }
    }
}

// Why (#77): the same holds for an x axis with manual limits narrower than one automatic step, except that its labels
// must also fit along the axis, so at least two are guaranteed: two labelled ticks are the fewest from which a value
// can be read by interpolation.
#[test]
fn a_narrow_x_axis_with_manual_limits_has_at_least_two_labelled_ticks() {
    let mut fx = Fx::new();
    fx.fig.size.width_mm = 40.0;
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    fx.ax(ax).x.limits = Limits::Manual {
        min: -0.15,
        max: 0.46,
    };
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let labels = x_tick_labels(&leaves(&scene), plot);
    assert!(labels.len() >= 2, "{:?}", texts(&labels));
    assert_labels_apart(&labels);
}

// Why (#76): the panels of a family sheet share one regular grid, x from −0.2 to 1.2 by 0.2, which the automatic
// locator does not choose; a regular step must give a tick at every multiple of the step within the limits, labelled
// with as many decimals as the step needs. The limits 1.2 and 0.4 are multiples of the step only up to rounding
// (6 × 0.2 is 1.2000000000000002), and must still carry their ticks.
#[test]
fn a_regular_step_ticks_every_multiple_within_the_limits() {
    let (mut fx, ax) = unit_axes();
    fx.ax(ax).x.ticks = Ticks::Step {
        step: 0.2,
        origin: 0.0,
    };
    fx.ax(ax).y.ticks = Ticks::Step {
        step: 0.2,
        origin: 0.0,
    };
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let leaves = leaves(&scene);
    assert_eq!(
        texts(&x_tick_labels(&leaves, plot)),
        ["−0.2", "0.0", "0.2", "0.4", "0.6", "0.8", "1.0", "1.2"]
    );
    assert_eq!(
        texts(&y_tick_labels(&leaves, plot)),
        ["−0.4", "−0.2", "0.0", "0.2", "0.4"]
    );
}

// Why: the origin anchors the step, so ticks offset from zero (every 0.1 from 0.05) are possible; their labels need
// the decimals of the origin as well as those of the step, or 0.05 would be printed as 0.1.
#[test]
fn a_regular_step_from_an_origin_is_labelled_with_the_decimals_both_need() {
    let (mut fx, ax) = unit_axes();
    fx.ax(ax).y.ticks = Ticks::Step {
        step: 0.1,
        origin: 0.05,
    };
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    assert_eq!(
        texts(&y_tick_labels(&leaves(&scene), plot)),
        [
            "−0.35", "−0.25", "−0.15", "−0.05", "0.05", "0.15", "0.25", "0.35"
        ]
    );
}

// Why: explicit tick values are drawn where the author asked, whatever the automatic locator would choose, and values
// outside the limits are left out rather than drawn over the decorations. Their labels share the fewest decimals that
// show every value, so that the labels of one axis line up as automatic labels do.
#[test]
fn explicit_tick_values_are_drawn_where_they_lie_within_the_limits() {
    let (mut fx, ax) = unit_axes();
    fx.ax(ax).x.ticks = Ticks::Values(vec![1.1, 0.0, 0.25, 5.0, -3.0]);
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, _) = axis_maps(&scene, ax);
    let labels = x_tick_labels(&leaves(&scene), plot);
    assert_eq!(texts(&labels), ["0.00", "0.25", "1.10"]);
    for label in &labels {
        assert_close(
            label.bbox.x + label.bbox.width / 2.0,
            x.to_figure(label.value),
            0.5,
        );
    }
}

// Why: grid lines are drawn at the major ticks, so explicit ticks must also place the grid; a grid at the automatic
// ticks under explicit labels would contradict the labels.
#[test]
fn grid_lines_follow_explicit_ticks() {
    let (mut fx, ax) = unit_axes();
    fx.ax(ax).x.ticks = Ticks::Values(vec![0.1, 0.7]);
    fx.ax(ax).x.grid = true;
    let scene = compile_figure(&fx.build());
    let (x, _) = axis_maps(&scene, ax);
    let plot = axes_hit(&scene, ax).plot_rect;
    let leaves = leaves(&scene);
    // The long vertical lines inside the plot, which leaves out the edges of the box and the short tick marks.
    let mut grid: Vec<f64> = from_source(&leaves, ax)
        .iter()
        .flat_map(|l| l.line_segments())
        .filter(|(a, b)| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() > 0.5 * plot.height)
        .map(|(a, _)| a.x)
        .filter(|gx| *gx > plot.x + 1e-6 && *gx < plot.right() - 1e-6)
        .collect();
    grid.sort_by(f64::total_cmp);
    grid.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    assert_eq!(grid.len(), 2, "{grid:?}");
    assert_close(grid[0], x.to_figure(0.1), 1e-6);
    assert_close(grid[1], x.to_figure(0.7), 1e-6);
}

// Why (#79): a categorical strip chart names its rows by item; explicit labels are drawn at their tick values, so the
// name of row k sits beside the row of data at y = k, and on a reversed y axis the first item is at the top.
#[test]
fn explicit_tick_labels_are_drawn_at_their_values_and_read_top_down_on_a_reversed_axis() {
    let names = ["first case", "second case", "third case"];
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.scatter(ax, &[0.2, 0.5, 0.8], &[0.0, 1.0, 2.0], None, |_, _| {});
    let axes = fx.ax(ax);
    axes.y.limits = Limits::Manual {
        min: -0.5,
        max: 2.5,
    };
    axes.y.reverse = true;
    axes.y.ticks = Ticks::Values(vec![0.0, 1.0, 2.0]);
    axes.y.tick_labels = TickLabels::Text(names.iter().map(|n| Text::plain(*n)).collect());
    let scene = compile_figure(&fx.build());
    let (_, y) = axis_maps(&scene, ax);
    let plot = axes_hit(&scene, ax).plot_rect;
    let mut previous = f64::NEG_INFINITY;
    for (k, name) in names.iter().enumerate() {
        let centre = centre_y(&scene, name);
        let height = text_bbox(&leaves(&scene), name).unwrap().height;
        assert_close(centre, y.to_figure(k as f64), 0.5 * height);
        assert!(centre > previous, "{name} is below the name before it");
        previous = centre;
        let b = text_bbox(&leaves(&scene), name).unwrap();
        assert!(b.right() <= plot.x, "{name} lies left of the plot");
    }
}

// Why (#77, #79): hidden labels are for an axes whose neighbour already names the ticks, so the ticks stay (they show
// the shared grid) but no label is drawn, and no space is reserved for labels, so the plot grows into it.
#[test]
fn hidden_tick_labels_keep_the_ticks_and_give_their_space_to_the_plot() {
    let (fx, ax) = unit_axes();
    let shown = compile_figure(&fx.fig.clone());
    let mut hidden = fx.build();
    hidden.axes[0].x.tick_labels = TickLabels::Hidden;
    hidden.axes[0].y.tick_labels = TickLabels::Hidden;
    let hidden = compile_figure(&hidden);
    let (sp, hp) = (
        axes_hit(&shown, ax).plot_rect,
        axes_hit(&hidden, ax).plot_rect,
    );
    let leaves_hidden = leaves(&hidden);
    assert!(x_tick_labels(&leaves_hidden, hp).is_empty());
    assert!(y_tick_labels(&leaves_hidden, hp).is_empty());
    assert!(hp.x < sp.x - 5.0, "{hp:?} grows left of {sp:?}");
    assert!(hp.bottom() > sp.bottom() + 5.0, "{hp:?} grows below {sp:?}");
    // The tick marks are still drawn: short segments from the left edge of the plot inwards, which leaves out the
    // top and bottom edges of the box.
    let marks = from_source(&leaves_hidden, ax)
        .iter()
        .flat_map(|l| l.line_segments())
        .filter(|(a, b)| {
            (a.y - b.y).abs() < 1e-9 && (a.x - hp.x).abs() < 1e-6 && b.x > a.x && b.x - a.x < 10.0
        })
        .count();
    assert!(marks >= 3, "{marks} y tick marks");
}

// Why: hidden labels leave no gap for labels that are not drawn, so an axis label of an axis with hidden tick labels
// moves in to sit beside the plot itself.
#[test]
fn an_axis_label_moves_in_beside_the_plot_when_the_tick_labels_are_hidden() {
    let (mut fx, ax) = unit_axes();
    fx.ax(ax).y.label = Some(Text::plain("thickness"));
    let shown = compile_figure(&fx.fig.clone());
    fx.ax(ax).y.tick_labels = TickLabels::Hidden;
    let hidden = compile_figure(&fx.build());
    let gap = |scene: &Scene| {
        let label = text_bbox(&leaves(scene), "thickness").expect("the label is drawn");
        axes_hit(scene, ax).plot_rect.x - label.right()
    };
    assert!(gap(&hidden) > 0.0, "the label lies left of the plot");
    assert!(
        gap(&hidden) < gap(&shown) - 5.0,
        "{} is closer than {}",
        gap(&hidden),
        gap(&shown)
    );
}

// Why: an axis with hidden labels shows no common exponent either, because the exponent qualifies labels that are not
// there, and a lone `×10^k` would read as a stray label.
#[test]
fn hidden_tick_labels_draw_no_common_exponent() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[1e5, 3e5], None, |_| {});
    let shown = compile_figure(&fx.fig.clone());
    fx.ax(ax).y.tick_labels = TickLabels::Hidden;
    let hidden = compile_figure(&fx.build());
    let math = |scene: &Scene| {
        glyph_runs(&leaves(scene))
            .into_iter()
            .filter(|(_, g)| g.font == FontId::Math)
            .count()
    };
    assert!(math(&shown) > 0, "the shown labels carry an exponent");
    assert_eq!(math(&hidden), 0);
}

// Why: an empty list of explicit values asks for an axis without ticks: no tick, label or grid line is drawn, and no
// space is reserved for labels, exactly as for hidden labels.
#[test]
fn an_empty_list_of_tick_values_draws_no_ticks_and_reserves_no_space() {
    let (mut fx, ax) = unit_axes();
    fx.ax(ax).y.grid = true;
    let shown = compile_figure(&fx.fig.clone());
    fx.ax(ax).y.ticks = Ticks::Values(vec![]);
    let empty = compile_figure(&fx.build());
    let plot = axes_hit(&empty, ax).plot_rect;
    let leaves = leaves(&empty);
    assert!(y_tick_labels(&leaves, plot).is_empty());
    let horizontals = from_source(&leaves, ax)
        .iter()
        .flat_map(|l| l.line_segments())
        .filter(|(a, b)| {
            (a.y - b.y).abs() < 1e-9 && a.y > plot.y + 1e-6 && a.y < plot.bottom() - 1e-6
        })
        .count();
    assert_eq!(horizontals, 0, "no tick mark or grid line inside the plot");
    assert!(plot.x < axes_hit(&shown, ax).plot_rect.x - 5.0);
}

// Why: explicit labels are paired with explicit values by position, whatever the order of the values, so that a
// program can list its categories in any order and still find each name beside its own row.
#[test]
fn explicit_tick_labels_follow_their_values_in_any_order() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.scatter(ax, &[0.5, 0.5, 0.5], &[0.0, 1.0, 2.0], None, |_, _| {});
    let axes = fx.ax(ax);
    axes.y.limits = Limits::Manual {
        min: -0.5,
        max: 2.5,
    };
    axes.y.ticks = Ticks::Values(vec![2.0, 0.0, 1.0]);
    axes.y.tick_labels = TickLabels::Text(vec![
        Text::plain("row two"),
        Text::plain("row zero"),
        Text::plain("row one"),
    ]);
    let scene = compile_figure(&fx.build());
    let (_, y) = axis_maps(&scene, ax);
    for (value, name) in [(0.0, "row zero"), (1.0, "row one"), (2.0, "row two")] {
        let height = text_bbox(&leaves(&scene), name).unwrap().height;
        assert_close(centre_y(&scene, name), y.to_figure(value), 0.5 * height);
    }
}

// Why: ticks never change the limits, so a regular step on an axis with automatic limits must leave those limits
// exactly as automatic ticks give them; otherwise choosing a grid would also rescale the data.
#[test]
fn a_regular_step_leaves_automatic_limits_unchanged() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 7.3], &[0.0, 1.0], None, |_| {});
    let plain = compile_figure(&fx.fig.clone());
    fx.ax(ax).x.ticks = Ticks::Step {
        step: 0.3,
        origin: 0.0,
    };
    let stepped = compile_figure(&fx.build());
    let (px, _) = axis_maps(&plain, ax);
    let (sx, _) = axis_maps(&stepped, ax);
    assert_eq!((px.min, px.max), (sx.min, sx.max));
}

// Why: a regular step is a step in data units on any scale, so on a logarithmic axis its ticks are equally spaced in
// value and therefore crowd towards the top of each decade, as the author asked.
#[test]
fn a_regular_step_on_a_log_axis_is_a_step_in_data_units() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[1.0, 2.0], &[1.0, 10.0], None, |_| {});
    let axes = fx.ax(ax);
    axes.y.scale = Scale::Log;
    axes.y.limits = Limits::Manual {
        min: 1.0,
        max: 10.0,
    };
    axes.y.ticks = Ticks::Step {
        step: 2.0,
        origin: 0.0,
    };
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (_, y) = axis_maps(&scene, ax);
    let labels = y_tick_labels(&leaves(&scene), plot);
    assert_eq!(texts(&labels), ["2", "4", "6", "8"]);
    for label in &labels {
        let centre = label.bbox.y + label.bbox.height / 2.0;
        assert_close(centre, y.to_figure(label.value), 0.5 * label.bbox.height);
    }
}

// Why: explicit ticks on a log axis are placed in log space and labelled as decades where they are decades, as the
// automatic labels of a log axis are, so a reader sees one notation on every log axis.
#[test]
fn explicit_ticks_on_a_log_axis_are_placed_logarithmically_and_labelled_as_decades() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[1.0, 2.0], &[1.0, 1000.0], None, |_| {});
    let axes = fx.ax(ax);
    axes.y.scale = Scale::Log;
    axes.y.limits = Limits::Manual {
        min: 1.0,
        max: 1000.0,
    };
    axes.y.ticks = Ticks::Values(vec![1.0, 30.0, 1000.0]);
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (_, y) = axis_maps(&scene, ax);
    let leaves = leaves(&scene);
    let labels = y_tick_labels(&leaves, plot);
    assert_eq!(texts(&labels), ["30"], "only 30 is not a decade");
    let centre = labels[0].bbox.y + labels[0].bbox.height / 2.0;
    assert_close(centre, y.to_figure(30.0), 0.5 * labels[0].bbox.height);
    // The decades 1 and 1000 are typeset as powers of ten: two bases "10" with the exponents 0 and 3.
    let math: Vec<String> = glyph_runs(&leaves)
        .into_iter()
        .filter(|(l, g)| g.font == FontId::Math && l.bbox().is_some_and(|b| b.right() <= plot.x))
        .map(|(_, g)| g.text.clone())
        .collect();
    assert_eq!(math.iter().filter(|t| *t == "10").count(), 2, "{math:?}");
    assert!(
        math.iter().any(|t| t == "0") && math.iter().any(|t| t == "3"),
        "{math:?}"
    );
}

/// A row of two axes over one tile row: the left one names three rows with labels of very different widths on a
/// reversed y axis, the right one shows the same rows with hidden labels, as the two panels of the yFoil
/// transition-position strip chart do.
fn strip_chart_pair() -> (Fx, NodeId, NodeId, Vec<&'static str>) {
    let names = vec![
        "naca_0012_re_3e6_alpha_4_transition_free",
        "short",
        "naca_4412_re_1e6",
    ];
    let mut fx = Fx::new();
    fx.fig.size.width_mm = 120.0;
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    for (ax, labels) in [
        (
            left,
            TickLabels::Text(names.iter().map(|n| Text::plain(*n)).collect()),
        ),
        (right, TickLabels::Hidden),
    ] {
        fx.scatter(ax, &[0.1, 0.5, 0.9], &[0.0, 1.0, 2.0], None, |_, _| {});
        let axes = fx.ax(ax);
        axes.x.limits = Limits::Manual { min: 0.0, max: 1.0 };
        axes.y.limits = Limits::Manual {
            min: -0.5,
            max: 2.5,
        };
        axes.y.reverse = true;
        axes.y.ticks = Ticks::Values(vec![0.0, 1.0, 2.0]);
        axes.y.tick_labels = labels;
    }
    (fx, left, right, names)
}

// Why (#79): the two panels of a strip chart show the same quantity side by side, and a reader compares positions
// across them, so their plot areas must be the same physical width even though only the left one carries the wide
// row names; and the left margin must be sized to the longest name, so that no name is clipped at the page edge.
#[test]
fn panels_side_by_side_have_plot_areas_of_equal_width_whatever_their_tick_labels() {
    let (fx, left, right, names) = strip_chart_pair();
    let figure = fx.build();
    let scene = compile_figure(&figure);
    let (l, r) = (
        axes_hit(&scene, left).plot_rect,
        axes_hit(&scene, right).plot_rect,
    );
    assert_close(l.width, r.width, 1e-6);
    assert_close(l.y, r.y, 1e-6);
    assert_close(l.height, r.height, 1e-6);
    assert!(l.right() < r.x, "the plots do not overlap");
    // Nothing is drawn closer to the page edge than the page padding, 0.4 font sizes.
    let padding = 0.4 * figure.font_size_pt;
    let leaves = leaves(&scene);
    for name in names {
        let b = text_bbox(&leaves, name).unwrap_or_else(|| panic!("{name} is drawn"));
        assert!(
            b.x >= padding - 0.5,
            "{name} at {b:?} is not clipped at the page edge"
        );
        assert!(b.right() <= l.x, "{name} lies left of the plot");
    }
}

// Why (#77): a stack of rows sharing x shows x tick labels on the bottom row only, and the rows must still be the same
// height, so that the same y range reads the same in every row; the labels of the bottom row are extra space below
// it, not a smaller plot.
#[test]
fn stacked_rows_have_plot_areas_of_equal_height_when_only_the_bottom_shows_x_labels() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 3;
    let rows: Vec<NodeId> = (0..3).map(|r| fx.axes2d(r, 0)).collect();
    for (k, ax) in rows.iter().enumerate() {
        fx.line(*ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
        if k < 2 {
            fx.ax(*ax).x.tick_labels = TickLabels::Hidden;
        } else {
            fx.ax(*ax).x.label = Some(Text::plain("chordwise position"));
        }
    }
    let scene = compile_figure(&fx.build());
    let plots: Vec<Rect> = rows
        .iter()
        .map(|ax| axes_hit(&scene, *ax).plot_rect)
        .collect();
    for p in &plots[1..] {
        assert_close(p.height, plots[0].height, 1e-6);
    }
}

// Why: column weights express how wide the plots are relative to one another, so with weights 2 : 1 the first plot is
// exactly twice as wide as the second even when one column carries wide tick labels and the other none.
#[test]
fn column_weights_share_plot_widths_in_proportion_whatever_the_margins() {
    let (mut fx, left, right, _) = strip_chart_pair();
    fx.fig.layout.col_weights = vec![2.0, 1.0];
    let scene = compile_figure(&fx.build());
    let (l, r) = (
        axes_hit(&scene, left).plot_rect,
        axes_hit(&scene, right).plot_rect,
    );
    assert_close(l.width, 2.0 * r.width, 1e-6);
}

// Why: the layout shares space between plot areas only after the margins are known, so a legend panel in its own
// column must still line up with the plot areas of the rows it spans and take its share of the width by its weight.
#[test]
fn a_panel_in_a_weighted_column_lines_up_with_the_plots_beside_it() {
    let (mut fx, left, right, _) = strip_chart_pair();
    fx.fig.layout.cols = 3;
    fx.fig.layout.col_weights = vec![1.0, 1.0, 0.5];
    let panel = fx.panel(cell(0, 2, 1, 1));
    fx.ax(left).artists.clear();
    fx.named_line(left, "series", |_| {});
    fx.legend_in(left, panel);
    let scene = compile_figure(&fx.build());
    let (l, r) = (
        axes_hit(&scene, left).plot_rect,
        axes_hit(&scene, right).plot_rect,
    );
    assert_close(l.width, r.width, 1e-6);
    let p = crate::probe::panel_rect(&scene, panel);
    assert_close(p.y, r.y, 1e-6);
    assert_close(p.bottom(), r.bottom(), 1e-6);
    assert!(p.x > r.right(), "the panel lies right of the plots");
    // The panel's column has no axes, so it has no margins: its share of the plot space, half that of a plot, is the
    // panel inset by the page padding on either side.
    let padding = 0.4 * fx_font_size();
    assert_close(p.width + 2.0 * padding, 0.5 * r.width, 1e-6);
}

/// The base font size of a default figure, in points.
fn fx_font_size() -> f64 {
    Fx::new().fig.font_size_pt
}

// Why: a step far finer than the axis can show would ask for millions of ticks; the compiler must not hang or exhaust
// memory on a figure that validation accepts, and must say what it did instead.
#[test]
fn a_tick_step_too_fine_for_the_axis_falls_back_to_automatic_ticks_with_a_warning() {
    let (mut fx, ax) = unit_axes();
    fx.ax(ax).x.ticks = Ticks::Step {
        step: 1e-9,
        origin: 0.0,
    };
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let labels = x_tick_labels(&leaves(&scene), plot);
    assert!(
        (2..=12).contains(&labels.len()),
        "automatic ticks are drawn instead: {:?}",
        texts(&labels)
    );
    assert!(
        scene
            .warnings
            .iter()
            .any(|w| w.node == Some(ax) && w.message.contains("tick")),
        "{:?}",
        scene.warnings
    );
}
