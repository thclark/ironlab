//! The aspect of a 2D axes: the ratio of the drawn length of one data unit of y to that of one data unit of x, kept
//! by shrinking the plot area or by widening the limits of one axis.

use ironlab_ir::{
    Aspect, AspectAdjust, AxisLink, Dimension, FigureSize, Limits, NodeId, Scale, View3d,
};
use ironlab_scene::Scene;
use ironlab_scene::display::Rect;
use ironlab_scene::hit::AxisMap;

use crate::common::{Fx, compile_figure, linspace};
use crate::probe::{
    assert_close, axes_hit, axes_legend_entries, axis_maps, bbox_of, centre, from_source, inside,
    leaves, y_tick_labels,
};

/// The relative tolerance within which a drawn ratio must equal the requested one.
const RATIO_TOLERANCE: f64 = 1e-9;

/// Returns a value in the scale space of an axis: the value on a linear axis, its base-10 logarithm on a log axis.
fn scaled(map: &AxisMap, value: f64) -> f64 {
    if map.log { value.log10() } else { value }
}

/// Returns the drawn length, in points, of one data unit of an axis (one decade on a log axis).
fn unit_length(map: &AxisMap) -> f64 {
    (map.end - map.start).abs() / (scaled(map, map.max) - scaled(map, map.min))
}

/// Returns the drawn length of one unit of y divided by that of one unit of x.
fn drawn_ratio(scene: &Scene, ax: NodeId) -> f64 {
    let (x, y) = axis_maps(scene, ax);
    unit_length(&y) / unit_length(&x)
}

#[track_caller]
fn assert_ratio(scene: &Scene, ax: NodeId, expected: f64) {
    let ratio = drawn_ratio(scene, ax);
    assert!(
        (ratio / expected - 1.0).abs() <= RATIO_TOLERANCE,
        "axes {ax}: drawn ratio {ratio}, requested {expected}"
    );
}

/// Asserts that the scene raised no warning about the aspect, which must be kept silently whenever it can be kept.
#[track_caller]
fn assert_no_aspect_warning(scene: &Scene) {
    let about: Vec<&String> = scene
        .warnings
        .iter()
        .map(|w| &w.message)
        .filter(|m| m.contains("aspect"))
        .collect();
    assert!(about.is_empty(), "{about:?}");
}

/// Returns tracks of the given weights.
fn weights(values: &[f64]) -> Vec<ironlab_ir::TrackSize> {
    values
        .iter()
        .map(|w| ironlab_ir::TrackSize::Weight(*w))
        .collect()
}

fn limits(map: &AxisMap) -> (f64, f64) {
    (map.min, map.max)
}

/// A unit circle centred on the origin, drawn as a line of 129 points.
fn circle() -> (Vec<f64>, Vec<f64>) {
    let t = linspace(0.0, std::f64::consts::TAU, 129);
    (
        t.iter().map(|t| t.cos()).collect(),
        t.iter().map(|t| t.sin()).collect(),
    )
}

/// One 2D axes on the default page (160 mm × 100 mm, wider than it is tall) holding a unit circle.
fn circle_axes(aspect: Aspect, adjust: AspectAdjust) -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let (x, y) = circle();
    fx.line(ax, &x, &y, None, |_| {});
    fx.ax(ax).aspect = aspect;
    fx.ax(ax).aspect_adjust = adjust;
    (fx, ax)
}

/// Returns the scene of the same figure with an automatic aspect, against which an adjustment is measured.
fn unadjusted(fx: &Fx) -> Scene {
    let mut figure = fx.fig.clone();
    for axes in &mut figure.axes {
        axes.aspect = Aspect::Auto;
    }
    compile_figure(&figure)
}

#[track_caller]
fn assert_rect_close(a: Rect, b: Rect, tol: f64) {
    for (p, q) in [
        (a.x, b.x),
        (a.y, b.y),
        (a.width, b.width),
        (a.height, b.height),
    ] {
        assert_close(p, q, tol);
    }
}

// -------------------------------------------------------------------------------------------------------------------
// Box: the plot area shrinks and the limits stay
// -------------------------------------------------------------------------------------------------------------------

// Why: a section drawn at equal aspect must keep its shape, so that a 12 % thick aerofoil looks 12 % thick; the
// circle is the simplest shape whose distortion is unmistakable. Its drawn path, not only the published axis maps,
// must be round, and the tick labels must stay beside the shrunk plot area rather than the space it was given.
#[test]
fn equal_aspect_kept_by_the_box_draws_a_circle_round() {
    let (mut fx, ax) = circle_axes(Aspect::Equal, AspectAdjust::Box);
    let line = fx.ax(ax).artists[0].id();
    let scene = compile_figure(&fx.fig);
    assert_ratio(&scene, ax, 1.0);
    assert_no_aspect_warning(&scene);
    let leaves = leaves(&scene);
    let drawn = bbox_of(
        from_source(&leaves, line)
            .iter()
            .flat_map(|leaf| leaf.endpoints()),
    )
    .expect("the circle is drawn");
    assert_close(drawn.width / drawn.height, 1.0, 1e-9);
    let plot = axes_hit(&scene, ax).plot_rect;
    let labels = y_tick_labels(&leaves, plot);
    assert!(!labels.is_empty());
    for label in labels {
        assert!(
            label.bbox.right() <= plot.x && label.bbox.right() > plot.x - 10.0,
            "the y tick label {label:?} sits beside the plot {plot:?}"
        );
    }
}

// Why: `Box` promises to keep the limits the author or the autoscaling chose and to give up plot area instead; on a
// page wider than the plot needs, only the width shrinks, and the plot stays centred in the space it was given, so
// that its neighbours and its title keep their places.
#[test]
fn the_box_shrinks_only_the_long_side_and_stays_centred_with_its_limits_unchanged() {
    let (fx, ax) = circle_axes(Aspect::Equal, AspectAdjust::Box);
    let scene = compile_figure(&fx.fig);
    let free = unadjusted(&fx);
    let (plot, space) = (
        axes_hit(&scene, ax).plot_rect,
        axes_hit(&free, ax).plot_rect,
    );
    assert!(
        plot.width < space.width - 1.0,
        "the plot {plot:?} is narrower than its space {space:?}"
    );
    assert_close(plot.height, space.height, 1e-9);
    assert_close(centre(plot).x, centre(space).x, 1e-9);
    assert_close(centre(plot).y, centre(space).y, 1e-9);
    let ((x, y), (fx_, fy)) = (axis_maps(&scene, ax), axis_maps(&free, ax));
    assert_eq!(limits(&x), limits(&fx_));
    assert_eq!(limits(&y), limits(&fy));
}

// Why: on a page taller than the data needs, it is the height that must give way, not the width.
#[test]
fn on_a_tall_page_the_box_shrinks_its_height() {
    let (mut fx, ax) = circle_axes(Aspect::Equal, AspectAdjust::Box);
    fx.fig.size = FigureSize {
        width_mm: 60.0,
        height_mm: 150.0,
    };
    let scene = compile_figure(&fx.fig);
    let free = unadjusted(&fx);
    let (plot, space) = (
        axes_hit(&scene, ax).plot_rect,
        axes_hit(&free, ax).plot_rect,
    );
    assert_ratio(&scene, ax, 1.0);
    assert_close(plot.width, space.width, 1e-9);
    assert!(plot.height < space.height - 1.0);
    assert_close(centre(plot).y, centre(space).y, 1e-9);
}

// Why: MATLAB's `daspect([1 r 1])` fixes a ratio other than one, for example to exaggerate a thin boundary layer;
// the ratio is the drawn length of a unit of y over that of a unit of x, never its inverse.
#[test]
fn a_ratio_draws_a_unit_of_y_that_many_times_as_long_as_a_unit_of_x() {
    for ratio in [0.25, 2.0, 7.5] {
        let (fx, ax) = circle_axes(Aspect::Ratio(ratio), AspectAdjust::Box);
        assert_ratio(&compile_figure(&fx.fig), ax, ratio);
        let (fx, ax) = circle_axes(Aspect::Ratio(ratio), AspectAdjust::Limits);
        assert_ratio(&compile_figure(&fx.fig), ax, ratio);
    }
}

// Why: with `Box`, manual limits are a statement of exactly what to show, so they must be drawn exactly as given.
#[test]
fn the_box_keeps_manual_limits_exactly() {
    let (mut fx, ax) = circle_axes(Aspect::Equal, AspectAdjust::Box);
    fx.ax(ax).x.limits = Limits::Manual {
        min: -0.2,
        max: 1.2,
    };
    fx.ax(ax).y.limits = Limits::Manual {
        min: -0.4,
        max: 0.4,
    };
    let scene = compile_figure(&fx.fig);
    let (x, y) = axis_maps(&scene, ax);
    assert_eq!(limits(&x), (-0.2, 1.2));
    assert_eq!(limits(&y), (-0.4, 0.4));
    assert_ratio(&scene, ax, 1.0);
}

// -------------------------------------------------------------------------------------------------------------------
// Limits: the plot area stays and the short axis widens about its centre
// -------------------------------------------------------------------------------------------------------------------

// Why: `Limits` keeps the plot area the layout gives (so that a grid of panels stays aligned) and shows more data
// along the short axis instead, widening it symmetrically so that the data stays centred.
#[test]
fn limits_widen_the_short_axis_about_its_centre_and_keep_the_other() {
    let (fx, ax) = circle_axes(Aspect::Equal, AspectAdjust::Limits);
    let scene = compile_figure(&fx.fig);
    let free = unadjusted(&fx);
    assert_ratio(&scene, ax, 1.0);
    let ((x, y), (free_x, free_y)) = (axis_maps(&scene, ax), axis_maps(&free, ax));
    // The page is wide, so x is the axis that must show more.
    assert_eq!(limits(&y), limits(&free_y), "the y limits are kept");
    assert!(x.max - x.min > free_x.max - free_x.min + 1e-6, "x widens");
    assert_close((x.min + x.max) / 2.0, (free_x.min + free_x.max) / 2.0, 1e-9);
}

// Why: the plot area of `Limits` is the one the layout gives, not a shrunk one; only margins that depend on the
// widened tick labels may move it, by far less than a `Box` adjustment would.
#[test]
fn limits_keep_the_plot_area_the_layout_gives() {
    let (fx, ax) = circle_axes(Aspect::Equal, AspectAdjust::Limits);
    let scene = compile_figure(&fx.fig);
    let free = unadjusted(&fx);
    let (plot, space) = (
        axes_hit(&scene, ax).plot_rect,
        axes_hit(&free, ax).plot_rect,
    );
    assert_close(plot.height, space.height, 2.0);
    assert_close(plot.width, space.width, 4.0);
}

// Why: manual limits under `Limits` are the least range to show: the dimension that is long enough keeps exactly
// the limits given, and only the other grows, about the centre of its manual limits.
#[test]
fn limits_widen_manual_limits_only_in_the_dimension_that_must_grow() {
    let (mut fx, ax) = circle_axes(Aspect::Equal, AspectAdjust::Limits);
    fx.ax(ax).x.limits = Limits::Manual {
        min: -0.2,
        max: 1.2,
    };
    fx.ax(ax).y.limits = Limits::Manual {
        min: -0.2,
        max: 0.2,
    };
    let scene = compile_figure(&fx.fig);
    let (x, y) = axis_maps(&scene, ax);
    assert_ratio(&scene, ax, 1.0);
    // A span of 1.4 by 0.4 on a plot about 1.6 times as wide as it is high: y must more than double, and x keeps
    // its limits.
    assert_eq!(limits(&x), (-0.2, 1.2));
    assert!(y.max - y.min > 0.8);
    assert_close((y.min + y.max) / 2.0, 0.0, 1e-12);
}

// Why: on a tall page it is y that is too short, and widening it changes the y tick labels and so the width of the
// plot area the widening was computed for; the layout must settle on limits drawn exactly at the aspect.
#[test]
fn limits_on_a_tall_page_widen_y_and_settle_on_the_exact_aspect() {
    let (mut fx, ax) = circle_axes(Aspect::Equal, AspectAdjust::Limits);
    fx.fig.size = FigureSize {
        width_mm: 60.0,
        height_mm: 150.0,
    };
    let scene = compile_figure(&fx.fig);
    let free = unadjusted(&fx);
    assert_ratio(&scene, ax, 1.0);
    assert_no_aspect_warning(&scene);
    let ((x, y), (free_x, free_y)) = (axis_maps(&scene, ax), axis_maps(&free, ax));
    assert_eq!(limits(&x), limits(&free_x), "x is kept");
    assert!(y.max - y.min > free_y.max - free_y.min + 1e-6, "y widens");
}

// Why: hidden tick labels take no space, so an axes with an aspect and hidden labels must still be drawn at the
// aspect, with the plot area the hidden labels leave.
#[test]
fn the_aspect_holds_with_hidden_tick_labels() {
    for adjust in [AspectAdjust::Box, AspectAdjust::Limits] {
        let (mut fx, ax) = circle_axes(Aspect::Ratio(0.5), adjust);
        fx.ax(ax).x.tick_labels = ironlab_ir::TickLabels::Hidden;
        fx.ax(ax).y.tick_labels = ironlab_ir::TickLabels::Hidden;
        assert_ratio(&compile_figure(&fx.fig), ax, 0.5);
    }
}

// Why: a legend inside the plot area belongs to the plot area that is drawn, so a box shrunk for the aspect must hold
// its legend, not leave it floating in the space the box gave up.
#[test]
fn a_legend_inside_a_shrunk_box_lies_inside_it() {
    let (mut fx, ax) = circle_axes(Aspect::Equal, AspectAdjust::Box);
    if let ironlab_ir::Artist::Line(line) = &mut fx.ax(ax).artists[0] {
        line.display_name = Some(ironlab_ir::Text::plain("section"));
    }
    fx.ax(ax).legend = Some(ironlab_ir::Legend::Inside(
        ironlab_ir::InsideLegend::default(),
    ));
    let scene = compile_figure(&fx.fig);
    let plot = axes_hit(&scene, ax).plot_rect;
    let entries = axes_legend_entries(&scene, ax);
    assert!(!entries.is_empty());
    for entry in entries {
        assert!(
            inside(entry.rect, plot, 1e-6),
            "{:?} in {plot:?}",
            entry.rect
        );
    }
}

// -------------------------------------------------------------------------------------------------------------------
// Direction, scale and layout
// -------------------------------------------------------------------------------------------------------------------

// Why: a reversed axis changes the direction in which values are drawn, not their spacing, so the aspect must hold on
// reversed axes exactly as on normal ones, and the limits must still run lower bound first.
#[test]
fn the_aspect_holds_on_reversed_axes() {
    for adjust in [AspectAdjust::Box, AspectAdjust::Limits] {
        let (mut fx, ax) = circle_axes(Aspect::Ratio(1.5), adjust);
        fx.ax(ax).x.reverse = true;
        fx.ax(ax).y.reverse = true;
        let scene = compile_figure(&fx.fig);
        assert_ratio(&scene, ax, 1.5);
        let (x, y) = axis_maps(&scene, ax);
        assert!(x.min < x.max && y.min < y.max);
        // Widening is centred whichever way the axis runs.
        let (free_x, free_y) = axis_maps(&unadjusted(&fx), ax);
        assert_close(x.min + x.max, free_x.min + free_x.max, 1e-9);
        assert_close(y.min + y.max, free_y.min + free_y.max, 1e-9);
        assert!(x.start > x.end, "x stays reversed");
        assert!(
            y.start < y.end,
            "y stays reversed (its upper limit at the top of the page)"
        );
    }
}

// Why: on a logarithmic axis the unit is one decade, so equal aspect on log–log axes draws a decade of x and a decade
// of y at the same length; widening a log axis must stay symmetric in decades, so that the data stays centred on the
// page.
#[test]
fn on_log_axes_a_decade_is_the_unit() {
    for adjust in [AspectAdjust::Box, AspectAdjust::Limits] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let x = [1.0, 10.0, 100.0];
        let y = [1.0, 100.0, 10_000.0];
        fx.line(ax, &x, &y, None, |_| {});
        fx.ax(ax).x.scale = Scale::Log;
        fx.ax(ax).y.scale = Scale::Log;
        fx.ax(ax).aspect = Aspect::Equal;
        fx.ax(ax).aspect_adjust = adjust;
        let scene = compile_figure(&fx.fig);
        assert_ratio(&scene, ax, 1.0);
        let free = unadjusted(&fx);
        let ((x, _), (free_x, _)) = (axis_maps(&scene, ax), axis_maps(&free, ax));
        if adjust == AspectAdjust::Limits {
            assert_close(
                (x.min.log10() + x.max.log10()) / 2.0,
                (free_x.min.log10() + free_x.max.log10()) / 2.0,
                1e-9,
            );
        }
    }
}

// Why: on log–linear axes the ratio relates a decade of the log axis to a unit of the linear one, which is how an
// author reads `Ratio(r)` there; the drawn ratio must use the same units.
#[test]
fn on_log_linear_axes_a_ratio_relates_a_decade_to_a_unit() {
    for adjust in [AspectAdjust::Box, AspectAdjust::Limits] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        fx.line(ax, &[1.0, 1000.0], &[0.0, 2.0], None, |_| {});
        fx.ax(ax).x.scale = Scale::Log;
        fx.ax(ax).aspect = Aspect::Ratio(0.75);
        fx.ax(ax).aspect_adjust = adjust;
        assert_ratio(&compile_figure(&fx.fig), ax, 0.75);
    }
}

// Why: a figure mixing panels of different weights must keep the aspect in each, each by its own plot area.
#[test]
fn each_tile_of_unequal_weight_keeps_its_aspect() {
    for adjust in [AspectAdjust::Box, AspectAdjust::Limits] {
        let mut fx = Fx::new();
        fx.fig.layout = ironlab_ir::TileLayout {
            rows: 2,
            cols: 2,
            row_weights: weights(&[1.0, 2.5]),
            col_weights: weights(&[3.0, 1.0]),
        };
        let (x, y) = circle();
        let mut ids = Vec::new();
        for (row, col) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            let ax = fx.axes2d(row, col);
            fx.line(ax, &x, &y, None, |_| {});
            fx.ax(ax).aspect = Aspect::Equal;
            fx.ax(ax).aspect_adjust = adjust;
            ids.push(ax);
        }
        let scene = compile_figure(&fx.fig);
        for ax in ids {
            assert_ratio(&scene, ax, 1.0);
        }
    }
}

// Why: the property applies to 2D axes; a 3D axes has no plot area of data units to shape. Validation refuses the
// combination, and the compiler, which never fails, must still draw the figure and say why the aspect was ignored.
#[test]
fn a_fixed_aspect_on_a_3d_axes_is_ignored_with_a_warning() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], Some(&[0.0, 1.0]), |_| {});
    fx.ax(ax).aspect = Aspect::Equal;
    let with = compile_figure(&fx.fig);
    let mut without = fx.fig.clone();
    without.axes[0].aspect = Aspect::Auto;
    let without = compile_figure(&without);
    assert_eq!(with.display_list, without.display_list);
    assert!(
        with.warnings
            .iter()
            .any(|w| w.node == Some(ax) && w.message.contains("aspect")),
        "{:?}",
        with.warnings
    );
}

// Why: a ratio that is not a finite positive number cannot be drawn; the compiler draws the axes as if the aspect
// were automatic and names the axes in a warning rather than failing or drawing a degenerate plot.
#[test]
fn an_unusable_ratio_is_drawn_as_automatic_with_a_warning() {
    for ratio in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let (fx, ax) = circle_axes(Aspect::Ratio(ratio), AspectAdjust::Box);
        let scene = compile_figure(&fx.fig);
        let free = unadjusted(&fx);
        assert_eq!(scene.display_list, free.display_list, "ratio {ratio}");
        assert!(
            scene
                .warnings
                .iter()
                .any(|w| w.node == Some(ax) && w.message.contains("aspect")),
            "ratio {ratio}: {:?}",
            scene.warnings
        );
    }
}

// -------------------------------------------------------------------------------------------------------------------
// Linked axes
// -------------------------------------------------------------------------------------------------------------------

/// Three panels side by side, each overlaying sections around the origin, with x and y linked over all of them, as
/// on an aerofoil family sheet; the middle panel's data is wider, so that the shared limits are set by one panel.
fn family_sheet(adjust: AspectAdjust, weights: Vec<f64>) -> (Fx, Vec<NodeId>) {
    let mut fx = Fx::new();
    fx.fig.size = FigureSize {
        width_mm: 180.0,
        height_mm: 70.0,
    };
    fx.fig.layout = ironlab_ir::TileLayout {
        rows: 1,
        cols: 3,
        row_weights: Vec::new(),
        col_weights: self::weights(&weights),
    };
    let (x, y) = circle();
    let mut ids = Vec::new();
    for col in 0..3 {
        let ax = fx.axes2d(0, col);
        let stretch = if col == 1 { 1.6 } else { 1.0 };
        let xs: Vec<f64> = x.iter().map(|v| stretch * v).collect();
        fx.line(ax, &xs, &y, None, |_| {});
        fx.ax(ax).aspect = Aspect::Equal;
        fx.ax(ax).aspect_adjust = adjust;
        ids.push(ax);
    }
    for dimension in [Dimension::X, Dimension::Y] {
        fx.fig.links.push(AxisLink {
            dimension,
            axes: ids.clone(),
        });
    }
    (fx, ids)
}

/// Returns the figure with every axis's limits set to the limits the scene drew, as a viewer does when a gesture
/// writes the limits it navigated from.
fn with_drawn_limits(fx: &Fx, scene: &Scene) -> ironlab_ir::Figure {
    let mut figure = fx.fig.clone();
    for axes in &mut figure.axes {
        let (x, y) = axis_maps(scene, axes.id);
        axes.x.limits = Limits::Manual {
            min: x.min,
            max: x.max,
        };
        axes.y.limits = Limits::Manual {
            min: y.min,
            max: y.max,
        };
    }
    figure
}

// Why: the family sheet links x and y over all panels at equal aspect; linked axes must show one range, and each
// panel must still be drawn at its aspect.
#[test]
fn linked_panels_share_their_limits_and_each_keeps_its_aspect() {
    for adjust in [AspectAdjust::Box, AspectAdjust::Limits] {
        let (fx, ids) = family_sheet(adjust, Vec::new());
        let scene = compile_figure(&fx.fig);
        assert_no_aspect_warning(&scene);
        let (x0, y0) = axis_maps(&scene, ids[0]);
        for &ax in &ids {
            assert_ratio(&scene, ax, 1.0);
            let (x, y) = axis_maps(&scene, ax);
            assert_eq!(limits(&x), limits(&x0), "{adjust:?}: x is shared");
            assert_eq!(limits(&y), limits(&y0), "{adjust:?}: y is shared");
        }
    }
}

// Why: a viewer writes the limits it shows back as manual limits at every gesture, so the drawn limits must be a
// fixed point of the layout: drawing them again must draw them unchanged, or the panels would creep or oscillate
// with every pan and zoom. Panels of unequal width linked in x and y at `Limits` are the conflict that falls back to
// the box, which must be a fixed point too.
#[test]
fn drawn_limits_of_linked_panels_are_a_fixed_point() {
    for adjust in [AspectAdjust::Box, AspectAdjust::Limits] {
        for weights in [Vec::new(), vec![1.0, 2.0, 1.5]] {
            let (fx, ids) = family_sheet(adjust, weights.clone());
            let mut scene = compile_figure(&fx.fig);
            for _ in 0..4 {
                let again = compile_figure(&with_drawn_limits(&fx, &scene));
                for &ax in &ids {
                    let ((x, y), (ax_, ay)) = (axis_maps(&scene, ax), axis_maps(&again, ax));
                    for (a, b) in [
                        (x.min, ax_.min),
                        (x.max, ax_.max),
                        (y.min, ay.min),
                        (y.max, ay.max),
                    ] {
                        assert_close(b, a, 1e-9 * (1.0 + a.abs()));
                    }
                    assert_rect_close(
                        axes_hit(&again, ax).plot_rect,
                        axes_hit(&scene, ax).plot_rect,
                        1e-6,
                    );
                    assert_ratio(&again, ax, 1.0);
                }
                scene = again;
            }
        }
    }
}

// Why: panels of unequal width linked in x only can each keep the aspect by widening its own y, so no conflict
// arises: the shared x stays shared, each y widens for its own plot area, and nothing is reported.
#[test]
fn panels_linked_in_x_only_widen_their_own_y_without_a_warning() {
    let (mut fx, ids) = family_sheet(AspectAdjust::Limits, vec![1.0, 2.0, 1.5]);
    fx.fig.links.retain(|link| link.dimension == Dimension::X);
    let scene = compile_figure(&fx.fig);
    assert_no_aspect_warning(&scene);
    let (x0, _) = axis_maps(&scene, ids[0]);
    for &ax in &ids {
        assert_ratio(&scene, ax, 1.0);
        assert_eq!(limits(&axis_maps(&scene, ax).0), limits(&x0));
    }
}

// Why: panels of different shapes whose x and y are both linked cannot all keep an equal aspect by widening shared
// limits (one panel would need a wider x, another a wider y, without end); the compiler must still keep every
// aspect, by shrinking the plot areas instead, finish rather than widen without bound, and say so.
#[test]
fn linked_panels_whose_shapes_conflict_keep_their_aspect_by_the_box_with_a_warning() {
    let (mut fx, ids) = family_sheet(AspectAdjust::Limits, vec![1.0, 3.0, 1.0]);
    fx.fig.size = FigureSize {
        width_mm: 200.0,
        height_mm: 60.0,
    };
    let scene = compile_figure(&fx.fig);
    let free = unadjusted(&fx);
    for &ax in &ids {
        assert_ratio(&scene, ax, 1.0);
        let ((x, y), (free_x, free_y)) = (axis_maps(&scene, ax), axis_maps(&free, ax));
        // The limits are at most those the autoscaling gave, not widened without bound.
        assert_eq!(limits(&x), limits(&free_x));
        assert_eq!(limits(&y), limits(&free_y));
    }
    let warned: Vec<NodeId> = scene
        .warnings
        .iter()
        .filter(|w| w.message.contains("aspect"))
        .filter_map(|w| w.node)
        .collect();
    assert_eq!(warned, ids, "each linked panel is named once");
}
