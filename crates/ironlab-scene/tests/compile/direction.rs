//! Reversed axes: an axis drawn from its upper limit to its lower limit, in 2D and 3D, with every other behaviour of
//! the axis unchanged.

use ironlab_ir::{AxisLink, Dimension, Limits, NodeId, Scale, View3d};
use ironlab_scene::display::Point;

use crate::common::{Fx, compile_figure, linspace, xy};
use crate::probe::{
    assert_close, axes_hit, axis_maps, from_source, leaves, marker_instances, x_tick_labels,
    y_tick_labels,
};

/// One 2D axes holding a line over x ∈ [0, 10] and y ∈ [−0.93, 0.97], with automatic limits.
fn sine_axes() -> (Fx, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let x = linspace(0.0, 10.0, 101);
    let y: Vec<f64> = x.iter().map(|v| 0.02 + 0.95 * v.sin()).collect();
    let line = fx.line(ax, &x, &y, None, |_| {});
    (fx, ax, line)
}

// Why: a reversed y axis is what a pressure distribution needs (suction, negative C_p, upwards), so the upper limit
// must sit at the bottom of the plot and the lower limit at the top, and the hit map must publish exactly that
// mapping, because the viewer inverts drawing through it for pan, zoom and datatips.
#[test]
fn a_reversed_y_axis_puts_its_upper_limit_at_the_bottom_of_the_plot() {
    let (mut fx, ax, _) = sine_axes();
    fx.ax(ax).y.reverse = true;
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, y) = axis_maps(&scene, ax);
    assert_eq!((y.min, y.max), (-1.0, 1.0), "the limits keep min < max");
    assert_close(y.to_figure(1.0), plot.bottom(), 1e-9);
    assert_close(y.to_figure(-1.0), plot.y, 1e-9);
    // The x axis is not reversed.
    assert_close(x.to_figure(0.0), plot.x, 1e-9);
    assert_close(x.to_figure(10.0), plot.right(), 1e-9);
}

// Why: a reversed x axis (retention time, depth along a traverse) runs from right to left, so the upper limit is at
// the left edge of the plot.
#[test]
fn a_reversed_x_axis_puts_its_upper_limit_at_the_left_of_the_plot() {
    let (mut fx, ax, _) = sine_axes();
    fx.ax(ax).x.reverse = true;
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, _) = axis_maps(&scene, ax);
    assert_close(x.to_figure(10.0), plot.x, 1e-9);
    assert_close(x.to_figure(0.0), plot.right(), 1e-9);
}

// Why: reversing an axis changes where values are drawn, never which values are shown, so automatic limits must be
// exactly those of the same axis unreversed; otherwise flipping the direction would also rescale the data.
#[test]
fn reversing_an_axis_leaves_its_automatic_limits_unchanged() {
    let (fx, ax, _) = sine_axes();
    let plain = compile_figure(&fx.fig.clone());
    let mut reversed = fx.build();
    reversed.axes[0].x.reverse = true;
    reversed.axes[0].y.reverse = true;
    let reversed = compile_figure(&reversed);
    let (px, py) = axis_maps(&plain, ax);
    let (rx, ry) = axis_maps(&reversed, ax);
    assert_eq!((px.min, px.max), (rx.min, rx.max));
    assert_eq!((py.min, py.max), (ry.min, ry.max));
}

// Why: the drawn line and the published axis maps must agree on a reversed axis too, otherwise pan, zoom and datatips
// point at the wrong data; a data point with a larger y must be drawn lower on the page.
#[test]
fn line_vertices_on_reversed_axes_map_back_to_their_data_through_the_hit_map() {
    let (mut fx, ax, line) = sine_axes();
    fx.ax(ax).x.reverse = true;
    fx.ax(ax).y.reverse = true;
    let scene = compile_figure(&fx.build());
    let (x, y) = axis_maps(&scene, ax);
    let leaves = leaves(&scene);
    let vertices: Vec<Point> = from_source(&leaves, line)
        .iter()
        .filter(|l| l.path().is_some_and(|p| p.stroke.is_some()))
        .flat_map(|l| l.subpaths().into_iter().flatten())
        .collect();
    let xs = linspace(0.0, 10.0, 101);
    assert_eq!(vertices.len(), xs.len());
    for (p, xv) in vertices.iter().zip(&xs) {
        assert_close(x.to_data(p.x), *xv, 1e-6);
        assert_close(y.to_data(p.y), 0.02 + 0.95 * xv.sin(), 1e-6);
    }
    // Larger x is further left and larger y further down.
    assert!(vertices[0].x > vertices[100].x);
    let peak = vertices[16]; // x = 1.6, near the maximum of the sine
    let trough = vertices[47]; // x = 4.7, near its minimum
    assert!(peak.y > trough.y, "{peak:?} is below {trough:?}");
}

// Why: the tick labels must travel with their ticks, so that a reader reads the value at a height from the label
// beside it; on a reversed y axis the labels therefore increase downwards, and on a reversed x axis leftwards.
#[test]
fn tick_labels_of_a_reversed_axis_run_from_its_upper_limit() {
    let (mut fx, ax, _) = sine_axes();
    fx.ax(ax).x.reverse = true;
    fx.ax(ax).y.reverse = true;
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, y) = axis_maps(&scene, ax);
    let leaves = leaves(&scene);
    let ys = y_tick_labels(&leaves, plot);
    assert!(ys.len() >= 3, "{ys:?}");
    for label in &ys {
        let centre = label.bbox.y + label.bbox.height / 2.0;
        assert_close(centre, y.to_figure(label.value), 0.5 * label.bbox.height);
    }
    let xs = x_tick_labels(&leaves, plot);
    assert!(xs.len() >= 3, "{xs:?}");
    for label in &xs {
        let centre = label.bbox.x + label.bbox.width / 2.0;
        assert_close(centre, x.to_figure(label.value), 0.5);
    }
    let top = ys
        .iter()
        .min_by(|a, b| a.bbox.y.total_cmp(&b.bbox.y))
        .unwrap();
    assert_eq!(top.value, -1.0, "the lowest value is labelled at the top");
    let left = xs
        .iter()
        .min_by(|a, b| a.bbox.x.total_cmp(&b.bbox.x))
        .unwrap();
    assert_eq!(
        left.value, 10.0,
        "the highest value is labelled at the left"
    );
}

// Why: a reversed axis must behave as a normal one does on a logarithmic scale as well, decades being equally
// spaced from the upper limit downwards (magnitude axes in astronomy are an example).
#[test]
fn a_reversed_log_axis_places_its_decades_from_the_upper_limit() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[1.0, 2.0, 3.0], &[1e-2, 1.0, 1e2], None, |_| {});
    fx.ax(ax).y.scale = Scale::Log;
    fx.ax(ax).y.reverse = true;
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (_, y) = axis_maps(&scene, ax);
    assert!(y.log);
    assert_eq!((y.min, y.max), (1e-2, 1e2));
    assert_close(y.to_figure(1e2), plot.bottom(), 1e-9);
    assert_close(y.to_figure(1e-2), plot.y, 1e-9);
    assert_close(y.to_figure(1.0), plot.y + plot.height / 2.0, 1e-9);
}

// Why: an image on reversed axes must be mirrored with the axes, so that each pixel still covers its data coordinates
// and the pixel under the pointer is the one the viewer reports.
#[test]
fn an_image_on_reversed_axes_is_mirrored_with_them() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let values: Vec<f64> = (0..6).map(f64::from).collect();
    let image = fx.imagesc(ax, vec![2, 3], values, xy(None, None), |_| {});
    fx.ax(ax).x.reverse = true;
    fx.ax(ax).y.reverse = true;
    let scene = compile_figure(&fx.build());
    let (x, y) = axis_maps(&scene, ax);
    // Pixel (row 1, column 2) is centred on the data point (2, 1).
    let at = Point::new(x.to_figure(2.0), y.to_figure(1.0));
    let (hit, row, column) = scene
        .hit_map
        .pixel_at(at)
        .expect("the point lies on the image");
    assert_eq!(hit.artist, image);
    assert_eq!((row, column), (1, 2));
    // Pixel (row 0, column 0) is centred on the data point (0, 0), which on reversed axes is at the right and the top.
    let at = Point::new(x.to_figure(0.0), y.to_figure(0.0));
    let (_, row, column) = scene
        .hit_map
        .pixel_at(at)
        .expect("the point lies on the image");
    assert_eq!((row, column), (0, 0));
}

// Why: axis links share limits only, so a reversed axis may be linked to a normal one: both must show the same range,
// each in its own direction, which is what a pressure plot (reversed C_p) linked in y to an unreversed companion would
// need, and a link must never silently copy the direction from one axes to the other.
#[test]
fn a_reversed_axis_linked_to_a_normal_one_shares_its_limits_and_keeps_its_direction() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    fx.line(left, &[0.0, 1.0], &[-3.0, 1.0], None, |_| {});
    fx.line(right, &[0.0, 1.0], &[0.0, 7.0], None, |_| {});
    fx.ax(left).y.reverse = true;
    fx.fig.links.push(AxisLink {
        dimension: Dimension::Y,
        axes: vec![left, right],
    });
    let scene = compile_figure(&fx.build());
    let (_, ly) = axis_maps(&scene, left);
    let (_, ry) = axis_maps(&scene, right);
    assert_eq!((ly.min, ly.max), (ry.min, ry.max), "the link shares limits");
    assert!(ly.min <= -3.0 && ly.max >= 7.0);
    let (lp, rp) = (
        axes_hit(&scene, left).plot_rect,
        axes_hit(&scene, right).plot_rect,
    );
    assert_close(ly.to_figure(ly.max), lp.bottom(), 1e-9);
    assert_close(ry.to_figure(ry.max), rp.y, 1e-9);
}

// Why: a reversed axis in a 3D axes must place every artist as if the data along that axis were mirrored about the
// middle of its limits, so that the box, the ticks and the data stay consistent whatever the view.
#[test]
fn a_reversed_axis_of_a_three_dimensional_axes_mirrors_the_data_along_it() {
    let view = View3d::default();
    let build = |reverse: [bool; 3], point: [f64; 3]| {
        let mut fx = Fx::new();
        let ax = fx.axes3d(0, 0, view);
        let scatter = fx.scatter(ax, &[point[0]], &[point[1]], Some(&[point[2]]), |_, _| {});
        let axes = fx.ax(ax);
        for (axis, reverse) in [&mut axes.x, &mut axes.y, &mut axes.z]
            .into_iter()
            .zip(reverse)
        {
            axis.limits = Limits::Manual {
                min: 0.0,
                max: 10.0,
            };
            axis.reverse = reverse;
        }
        let scene = compile_figure(&fx.build());
        marker_instances(&leaves(&scene), scatter)[0].position
    };
    for dim in 0..3 {
        let mut reverse = [false; 3];
        reverse[dim] = true;
        let mut point = [2.0, 3.0, 4.0];
        let reversed = build(reverse, point);
        point[dim] = 10.0 - point[dim];
        let mirrored = build([false; 3], point);
        assert_close(reversed.x, mirrored.x, 1e-6);
        assert_close(reversed.y, mirrored.y, 1e-6);
    }
}

// Why: grid lines and tick marks belong to tick values, not to positions on the page, so on a reversed axis each must
// be drawn where its value is drawn; grid lines left in the unreversed positions would contradict the labels.
#[test]
fn grid_lines_of_a_reversed_axis_follow_its_ticks() {
    let (mut fx, ax, _) = sine_axes();
    let axes = fx.ax(ax);
    axes.y.reverse = true;
    axes.y.grid = true;
    axes.y.limits = Limits::Manual {
        min: -1.0,
        max: 0.5,
    };
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (_, y) = axis_maps(&scene, ax);
    let leaves = leaves(&scene);
    let mut grid: Vec<f64> = from_source(&leaves, ax)
        .iter()
        .flat_map(|l| l.line_segments())
        .filter(|(a, b)| (a.y - b.y).abs() < 1e-9 && (a.x - b.x).abs() > 0.5 * plot.width)
        .map(|(a, _)| a.y)
        .filter(|gy| *gy > plot.y + 1e-6 && *gy < plot.bottom() - 1e-6)
        .collect();
    grid.sort_by(f64::total_cmp);
    grid.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let labels = y_tick_labels(&leaves, plot);
    let interior: Vec<f64> = labels
        .iter()
        .map(|l| y.to_figure(l.value))
        .filter(|gy| *gy > plot.y + 1e-6 && *gy < plot.bottom() - 1e-6)
        .collect();
    assert!(!interior.is_empty());
    assert_eq!(grid.len(), interior.len(), "{grid:?} against {interior:?}");
    for gy in interior {
        assert!(
            grid.iter().any(|g| (g - gy).abs() < 1e-6),
            "a grid line at {gy}"
        );
    }
}

// Why: a quiver arrow points along its vector in data space, so on a reversed x axis an arrow with a positive x
// component points to the left of the page; an arrow drawn in page space would point the wrong way.
#[test]
fn a_quiver_arrow_on_a_reversed_axis_points_along_its_vector_in_data_space() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let quiver = fx.quiver(ax, &[5.0], &[5.0], None, &[2.0], &[0.0], None);
    let axes = fx.ax(ax);
    axes.x.limits = Limits::Manual {
        min: 0.0,
        max: 10.0,
    };
    axes.y.limits = Limits::Manual {
        min: 0.0,
        max: 10.0,
    };
    axes.x.reverse = true;
    let scene = compile_figure(&fx.build());
    let (x, _) = axis_maps(&scene, ax);
    let leaves = leaves(&scene);
    let points: Vec<Point> = from_source(&leaves, quiver)
        .iter()
        .flat_map(|l| l.subpaths().into_iter().flatten())
        .collect();
    let leftmost = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let rightmost = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let tail = x.to_figure(5.0);
    assert_close(rightmost, tail, 1.0);
    assert!(leftmost < tail - 5.0, "the arrow points left: {points:?}");
}

// Why: the tick labels of a 3D axes are placed through the same mirrored box as its data, so on a reversed axis the
// label of a value is drawn where the label of the mirrored value is drawn on the axis unreversed.
#[test]
fn tick_labels_of_a_reversed_three_dimensional_axis_are_mirrored_with_its_data() {
    let view = View3d::default();
    let label_centre = |reverse: bool, text: &str| {
        let mut fx = Fx::new();
        let ax = fx.axes3d(0, 0, view);
        fx.scatter(ax, &[1.0], &[1.0], Some(&[1.0]), |_, _| {});
        let axes = fx.ax(ax);
        for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
            axis.limits = Limits::Manual {
                min: 0.0,
                max: 10.0,
            };
            axis.ticks = ironlab_ir::Ticks::Values(vec![2.0, 8.0]);
        }
        axes.x.reverse = reverse;
        // Only x is labelled, so that no label of another axis can be mistaken for one of x.
        axes.y.tick_labels = ironlab_ir::TickLabels::Hidden;
        axes.z.tick_labels = ironlab_ir::TickLabels::Hidden;
        let scene = compile_figure(&fx.build());
        let b = crate::probe::text_bbox(&leaves(&scene), text)
            .unwrap_or_else(|| panic!("{text} is drawn"));
        Point::new(b.x + b.width / 2.0, b.y + b.height / 2.0)
    };
    let reversed = label_centre(true, "2");
    let mirrored = label_centre(false, "8");
    assert_close(reversed.x, mirrored.x, 1.0);
    assert_close(reversed.y, mirrored.y, 1.0);
}
