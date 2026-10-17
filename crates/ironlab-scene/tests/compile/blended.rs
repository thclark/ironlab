//! Blended coordinates and unclipped artists: reference lines that span the axes, and markers along an edge of the
//! axes at data positions, as Matplotlib's `axhline`, `axvline` and blended transforms draw them.
//!
//! WHY: a comparison sheet judges its data against reference lines (a tolerance, the ends of an interval) and marks
//! where a series is not finite with ticks along the top of the axes. Both must stay where they belong whatever the
//! limits, the scale or the pan and zoom of the axes, must not drag the automatic limits towards their fractions, and
//! the ticks must be drawn even where they overhang the plot area.

use ironlab_ir::{Coords, Limits, MarkerShape, NodeId, Scale};
use ironlab_scene::Scene;
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::AxisMap;

use crate::common::{Fx, compile_figure};
use crate::probe::{assert_close, axes_hit, axis_maps, from_source, leaves};

const TOL: f64 = 1e-6;

/// Returns the plot rectangle of an axes.
fn plot(scene: &Scene, ax: NodeId) -> Rect {
    axes_hit(scene, ax).plot_rect
}

/// Returns where the points of an artist were drawn, in source order, from the hit map.
fn drawn(scene: &Scene, artist: NodeId) -> Vec<(usize, Point)> {
    scene
        .hit_map
        .artists
        .iter()
        .find(|hit| hit.artist == artist)
        .map(|hit| {
            hit.samples
                .iter()
                .map(|s| (s.source_index, s.position))
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the figure-space y of a fraction of the plot height, 0 at the bottom edge and 1 at the top.
fn fraction_y(plot: Rect, f: f64) -> f64 {
    plot.bottom() - f * plot.height
}

/// Returns the figure-space x of a fraction of the plot width, 0 at the left edge and 1 at the right.
fn fraction_x(plot: Rect, f: f64) -> f64 {
    plot.x + f * plot.width
}

/// Asserts that a clip is the x extent of the plot area, unbounded by the plot area along y: the clip of an artist
/// whose y values are fractions of the plot area and whose clipping is released.
#[track_caller]
fn assert_released_along_y_only(clip: Option<Rect>, plot: Rect) {
    let clip = clip.expect("an artist placed in data along x is clipped along x");
    assert_close(clip.x, plot.x, TOL);
    assert_close(clip.right(), plot.right(), TOL);
    assert!(
        clip.y < plot.y && clip.bottom() > plot.bottom(),
        "the clip {clip:?} extends beyond the plot area {plot:?} along y"
    );
}

/// Adds a horizontal reference line at `y`, as the facade's `hline` builds it.
fn hline(fx: &mut Fx, ax: NodeId, y: f64) -> NodeId {
    fx.line(ax, &[0.0, 1.0], &[y, y], None, |l| {
        l.x_coords = Coords::AxesFraction;
    })
}

/// Adds markers at data `x` along the top of the axes, at 0.985 of its height, as a comparison sheet marks the
/// values that are not finite.
fn edge_ticks(fx: &mut Fx, ax: NodeId, x: &[f64], clip: bool) -> NodeId {
    fx.line(ax, x, &vec![0.985; x.len()], None, |l| {
        l.y_coords = Coords::AxesFraction;
        l.clip = clip;
        l.line.dash = ironlab_ir::DashStyle::None;
        l.marker.shape = MarkerShape::VLine;
        l.marker.size_pt = 6.0;
    })
}

// A reference line spans the plot area from edge to edge at its data value, on a linear and on a logarithmic axis,
// so that it never stops short of the edges as a line between fixed data coordinates does when the reader zooms out.
#[test]
fn a_reference_line_spans_the_plot_area_at_its_data_value() {
    for (x_scale, y_scale) in [(Scale::Linear, Scale::Linear), (Scale::Log, Scale::Log)] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        fx.ax(ax).x.scale = x_scale;
        fx.ax(ax).y.scale = y_scale;
        fx.line(ax, &[1.0, 10.0, 100.0], &[0.01, 0.1, 10.0], None, |_| {});
        let reference = hline(&mut fx, ax, 1.0);
        let scene = compile_figure(&fx.build());
        let plot = plot(&scene, ax);
        let (_, y) = axis_maps(&scene, ax);
        let points = drawn(&scene, reference);
        assert_eq!(
            points.len(),
            2,
            "{x_scale:?}/{y_scale:?}: both ends are drawn"
        );
        assert_close(points[0].1.x, plot.x, TOL);
        assert_close(points[1].1.x, plot.right(), TOL);
        for (_, p) in &points {
            assert_close(p.y, y.to_figure(1.0), TOL);
        }
    }
}

// Pan and zoom in the viewers change the limits of the axes and recompile, so a reference line must follow its data
// value through any limits while still spanning the plot area; a line in data coordinates would leave the view.
#[test]
fn a_vertical_reference_line_follows_its_data_value_through_any_limits() {
    for (min, max) in [(-10.0, 10.0), (-0.5, 0.25), (-100.0, -50.0)] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        fx.ax(ax).x.limits = Limits::Manual { min, max };
        let reference = fx.line(ax, &[0.0, 0.0], &[0.0, 1.0], None, |l| {
            l.y_coords = Coords::AxesFraction;
        });
        let scene = compile_figure(&fx.build());
        let plot = plot(&scene, ax);
        let (x, _) = axis_maps(&scene, ax);
        let points = drawn(&scene, reference);
        let expected_x = x.to_figure(0.0);
        // The drawn points may lie outside the plot area when 0 is out of view; they are still placed.
        assert_eq!(points.len(), 2);
        assert_close(points[0].1.y, plot.bottom(), TOL);
        assert_close(points[1].1.y, plot.y, TOL);
        assert_close(points[0].1.x, expected_x, TOL);
        assert_close(points[1].1.x, expected_x, TOL);
    }
}

// A fraction of the plot area is not a value along the axis, so a dimension in axes-fraction coordinates must not
// pull the automatic limits towards 0..1: data in 100..200 keeps limits around 100..200.
#[test]
fn a_dimension_in_axes_fraction_takes_no_part_in_the_automatic_limits() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[100.0, 200.0], None, |_| {});
    edge_ticks(&mut fx, ax, &[0.5], true);
    let scene = compile_figure(&fx.build());
    let (_, y) = axis_maps(&scene, ax);
    assert!(
        y.min >= 100.0 - TOL,
        "y limits {}..{} reach down to the fraction",
        y.min,
        y.max
    );
    assert!(y.max <= 200.0 + TOL);
}

// The data dimension of a blended artist still takes part in the automatic limits, as Matplotlib's axhline does, so
// that a tolerance line above the data is in view without the author fixing the limits.
#[test]
fn the_data_dimension_of_a_reference_line_takes_part_in_the_automatic_limits() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[10.0, 15.0, 20.0], &[0.1, 0.2, 0.3], None, |_| {});
    hline(&mut fx, ax, 5.0);
    let scene = compile_figure(&fx.build());
    let (x, y) = axis_maps(&scene, ax);
    assert!(
        y.max >= 5.0,
        "the y limits {}..{} include the line",
        y.min,
        y.max
    );
    // The fractions 0 and 1 of the x span are not data, so the x limits still fit the data in 10..20 rather than
    // being stretched down to 0 by the reference line; they are the same as without it.
    let mut alone = Fx::new();
    let ax_alone = alone.axes2d(0, 0);
    alone.line(
        ax_alone,
        &[10.0, 15.0, 20.0],
        &[0.1, 0.2, 0.3],
        None,
        |_| {},
    );
    let (x_alone, _) = axis_maps(&compile_figure(&alone.build()), ax_alone);
    assert_eq!((x.min, x.max), (x_alone.min, x_alone.max));
}

// Fractions on a logarithmic axis are placed linearly in the plot area: 0.985 is 98.5 % of the way up whatever the
// decades shown, and a fraction of 0 is not a non-positive value that the log axis must refuse.
#[test]
fn fractions_are_placed_linearly_on_a_logarithmic_axis() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.ax(ax).y.scale = Scale::Log;
    fx.line(ax, &[0.0, 1.0, 2.0], &[1e-6, 1e-2, 1e3], None, |_| {});
    let ticks = edge_ticks(&mut fx, ax, &[0.0, 1.0, 2.0], false);
    let bottom = fx.line(ax, &[0.0, 1.0], &[0.0, 0.0], None, |l| {
        l.y_coords = Coords::AxesFraction;
    });
    let scene = compile_figure(&fx.build());
    assert!(
        scene
            .warnings
            .iter()
            .all(|w| w.node != Some(bottom) && w.node != Some(ticks)),
        "no warning about fractions on the log axis: {:?}",
        scene.warnings
    );
    let plot = plot(&scene, ax);
    let (x, _) = axis_maps(&scene, ax);
    let tick_points = drawn(&scene, ticks);
    assert_eq!(tick_points.len(), 3, "every tick is placed");
    for (i, p) in tick_points {
        assert_close(p.y, fraction_y(plot, 0.985), TOL);
        assert_close(p.x, x.to_figure(i as f64), TOL);
    }
    let bottom_points = drawn(&scene, bottom);
    assert_eq!(
        bottom_points.len(),
        2,
        "a fraction of 0 is placed on a log axis"
    );
    for (_, p) in bottom_points {
        assert_close(p.y, plot.bottom(), TOL);
    }
}

// The pathological panel of a comparison sheet: every value of the series is NaN, so only the ticks along the top
// say anything. They must be drawn even though the axis has no data to scale to, on a logarithmic axis too.
#[test]
fn edge_ticks_are_drawn_when_every_value_of_the_series_is_nan() {
    for scale in [Scale::Linear, Scale::Log] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        fx.ax(ax).y.scale = scale;
        let alpha = [-2.0, 0.0, 2.0, 4.0];
        fx.line(ax, &alpha, &[f64::NAN; 4], None, |_| {});
        let ticks = edge_ticks(&mut fx, ax, &alpha, false);
        let scene = compile_figure(&fx.build());
        let plot = plot(&scene, ax);
        // With no finite data the axis falls back to its default range, which the ticks do not disturb.
        let (_, y) = axis_maps(&scene, ax);
        let expected = match scale {
            Scale::Log => (1.0, 10.0),
            Scale::Linear => (0.0, 1.0),
        };
        assert_eq!((y.min, y.max), expected, "{scale:?}: the default range");
        assert!(
            scene.warnings.iter().all(|w| w.node != Some(ticks)),
            "{scale:?}: nothing is wrong with the ticks: {:?}",
            scene.warnings
        );
        let points = drawn(&scene, ticks);
        assert_eq!(points.len(), 4, "{scale:?}: one tick per value");
        for (_, p) in points {
            assert_close(p.y, fraction_y(plot, 0.985), TOL);
            assert!(p.x >= plot.x - TOL && p.x <= plot.right() + TOL);
        }
    }
}

// An artist that is not clipped along its fraction dimension is drawn in a clip that extends beyond the plot area in
// that dimension only, so that a tick on the top edge is drawn whole; a clipped artist stays inside the plot area. The
// order in which they were added is the order in which they are painted.
#[test]
fn an_unclipped_artist_is_painted_outside_the_clip_of_the_plot_area_in_artist_order() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let first = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let unclipped = edge_ticks(&mut fx, ax, &[0.5], false);
    let last = fx.line(ax, &[0.0, 1.0], &[1.0, 0.0], None, |_| {});
    let scene = compile_figure(&fx.build());
    let plot = plot(&scene, ax);
    let all = leaves(&scene);
    let position = |id: NodeId| {
        all.iter()
            .position(|l| l.source == Some(id))
            .unwrap_or_else(|| panic!("{id} is drawn"))
    };
    for id in [first, last] {
        for leaf in from_source(&all, id) {
            assert_eq!(leaf.clip, Some(plot), "{id} is clipped to the plot area");
        }
    }
    let ticks = from_source(&all, unclipped);
    assert!(!ticks.is_empty());
    for leaf in ticks {
        assert_released_along_y_only(leaf.clip, plot);
    }
    assert!(position(first) < position(unclipped) && position(unclipped) < position(last));
}

// A tick placed exactly on the top edge of the axes overhangs it by half its length; with clipping on, the overhang
// is cut by the plot area, which is what `clip` is for.
#[test]
fn a_clipped_artist_on_the_edge_is_clipped_to_the_plot_area() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let ticks = fx.line(ax, &[0.5], &[1.0], None, |l| {
        l.y_coords = Coords::AxesFraction;
        l.marker.shape = MarkerShape::VLine;
    });
    let scene = compile_figure(&fx.build());
    let plot = plot(&scene, ax);
    let leaves = from_source(&leaves(&scene), ticks);
    assert!(!leaves.is_empty());
    assert!(leaves.iter().all(|l| l.clip == Some(plot)));
    let points = drawn(&scene, ticks);
    assert_close(points[0].1.y, plot.y, TOL);
}

// Both dimensions in axes-fraction coordinates place a point in the plot area alone, as an annotation in axes
// coordinates does, whatever the limits.
#[test]
fn both_dimensions_in_axes_fraction_place_a_point_in_the_plot_area() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.ax(ax).x.limits = Limits::Manual {
        min: 1000.0,
        max: 2000.0,
    };
    let point = fx.scatter(ax, &[0.25], &[0.75], None, |s, _| {
        s.x_coords = Coords::AxesFraction;
        s.y_coords = Coords::AxesFraction;
    });
    let scene = compile_figure(&fx.build());
    let plot = plot(&scene, ax);
    let points = drawn(&scene, point);
    assert_eq!(points.len(), 1);
    assert_close(points[0].1.x, fraction_x(plot, 0.25), TOL);
    assert_close(points[0].1.y, fraction_y(plot, 0.75), TOL);
}

// The hit map's axis maps are what the viewers turn a pointer into data with; a blended artist must not change them,
// or panning an axes holding a reference line would pan by the wrong amount.
#[test]
fn a_blended_artist_leaves_the_axis_maps_of_its_axes_unchanged() {
    let build = |with_reference: bool| {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        fx.line(ax, &[10.0, 11.0, 12.0], &[3.0, 4.0, 5.0], None, |_| {});
        if with_reference {
            fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |l| {
                l.x_coords = Coords::AxesFraction;
                l.y_coords = Coords::AxesFraction;
            });
        }
        let figure = fx.build();
        let maps: (AxisMap, AxisMap) = axis_maps(&compile_figure(&figure), ax);
        maps
    };
    assert_eq!(build(false), build(true));
}

// A reference line at a data value on a logarithmic axis lands where the logarithm puts it: 10 is half way between 1
// and 100, computed here from the plot area alone rather than through the compiler's own axis map.
#[test]
fn a_reference_line_on_a_single_logarithmic_axis_lands_at_the_logarithmic_position() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.ax(ax).x.scale = Scale::Log;
    fx.ax(ax).x.limits = Limits::Manual {
        min: 1.0,
        max: 100.0,
    };
    fx.ax(ax).y.scale = Scale::Log;
    fx.ax(ax).y.limits = Limits::Manual {
        min: 0.1,
        max: 1000.0,
    };
    let vertical = fx.line(ax, &[10.0, 10.0], &[0.0, 1.0], None, |l| {
        l.y_coords = Coords::AxesFraction;
    });
    let horizontal = hline(&mut fx, ax, 10.0);
    let scene = compile_figure(&fx.build());
    let plot = plot(&scene, ax);
    let v = drawn(&scene, vertical);
    assert_eq!(v.len(), 2);
    for (_, p) in v {
        assert_close(p.x, fraction_x(plot, 0.5), TOL);
    }
    let h = drawn(&scene, horizontal);
    assert_eq!(h.len(), 2);
    for (_, p) in h {
        // 10 is two decades above 0.1 of the four shown.
        assert_close(p.y, fraction_y(plot, 0.5), TOL);
    }
}

// The automatic limits of linked axes cover the data of every member, so a reference line in one member reaches the
// others as data would, and the fractions of the ticks in that member reach none of them.
#[test]
fn linked_axes_take_the_data_value_of_a_reference_line_and_not_the_fractions_of_edge_ticks() {
    let mut fx = Fx::new();
    fx.fig.layout = ironlab_ir::TileLayout::new(1, 2);
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    fx.line(left, &[0.0, 1.0], &[100.0, 200.0], None, |_| {});
    hline(&mut fx, right, 500.0);
    edge_ticks(&mut fx, right, &[0.5], false);
    fx.fig.links.push(ironlab_ir::AxisLink {
        dimension: ironlab_ir::Dimension::Y,
        axes: vec![left, right],
    });
    let scene = compile_figure(&fx.build());
    for ax in [left, right] {
        let (_, y) = axis_maps(&scene, ax);
        assert!(
            y.min >= 100.0 - TOL,
            "{ax}: y from {} reaches towards the fractions",
            y.min
        );
        assert!(
            y.max >= 500.0,
            "{ax}: y to {} misses the reference line",
            y.max
        );
    }
}

// Markers that are not clipped are drawn wherever they are placed, beyond the plot area too, even in a series dense
// enough to be thinned to the view: thinning drops only what cannot be seen, and an unclipped marker above the plot
// area can be. Clipped, the same markers are all outside the clip and are dropped.
#[test]
fn unclipped_markers_beyond_the_plot_area_survive_the_thinning_of_a_dense_series() {
    let n = 20_000;
    let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
    for clip in [false, true] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let ticks = fx.line(ax, &x, &vec![1.02; n], None, |l| {
            l.y_coords = Coords::AxesFraction;
            l.clip = clip;
            l.line.dash = ironlab_ir::DashStyle::None;
            l.marker.shape = MarkerShape::VLine;
        });
        let scene = compile_figure(&fx.build());
        let plot = plot(&scene, ax);
        let points = drawn(&scene, ticks);
        if clip {
            assert!(
                points.is_empty(),
                "clipped markers above the plot area are dropped"
            );
        } else {
            assert!(
                !points.is_empty(),
                "unclipped markers above the plot area are drawn"
            );
            assert!(points.iter().all(|(_, p)| p.y < plot.y));
            let all = leaves(&scene);
            let drawn_leaves = from_source(&all, ticks);
            assert!(!drawn_leaves.is_empty());
            for leaf in drawn_leaves {
                assert_released_along_y_only(leaf.clip, plot);
            }
        }
    }
}

// The owner's report: after panning in x, edge ticks whose data x lies outside the x limits slid out of the plot area
// and onto the neighbouring axes. Releasing the clip lets a tick overhang the top or bottom edge, but along its data
// dimension the tick is clipped to the plot area exactly as a clipped artist is, so a tick out of the x range is not
// drawn and a tick in range is.
#[test]
fn edge_ticks_outside_the_x_limits_are_clipped_to_the_x_extent_of_the_plot_area() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.ax(ax).x.limits = Limits::Manual {
        min: 0.0,
        max: 10.0,
    };
    fx.line(ax, &[0.0, 10.0], &[0.0, 1.0], None, |_| {});
    let ticks = edge_ticks(&mut fx, ax, &[-5.0, 5.0, 15.0], false);
    let scene = compile_figure(&fx.build());
    let plot = plot(&scene, ax);
    let leaves = from_source(&leaves(&scene), ticks);
    assert!(!leaves.is_empty());
    for leaf in &leaves {
        assert_released_along_y_only(leaf.clip, plot);
    }
    // Of the three ticks, only the one at x = 5 lies within the clip.
    let points = drawn(&scene, ticks);
    let visible: Vec<usize> = points
        .iter()
        .filter(|(_, p)| p.x >= plot.x && p.x <= plot.right())
        .map(|(i, _)| *i)
        .collect();
    assert_eq!(visible, vec![1]);
}

// An artist placed in data along both dimensions has no edge of the axes to sit on, so releasing its clip has no
// effect: it is clipped to the plot area as any other artist is, and never draws over the neighbouring axes.
#[test]
fn an_artist_in_data_coordinates_is_clipped_whatever_its_clip_flag() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.ax(ax).x.limits = Limits::Manual { min: 0.0, max: 1.0 };
    let line = fx.line(ax, &[-1.0, 2.0], &[0.0, 1.0], None, |l| l.clip = false);
    let scene = compile_figure(&fx.build());
    let plot = plot(&scene, ax);
    let leaves = from_source(&leaves(&scene), line);
    assert!(!leaves.is_empty());
    assert!(leaves.iter().all(|l| l.clip == Some(plot)));
}

// A reversed axis maps its data from its upper limit, but a fraction of the plot area is a physical position: 0 is
// always the left or bottom edge. On axes reversed in both directions, an edge tick at a data x is placed through the
// reversed x map, and its fraction of the height still counts from the bottom; a reference line at a data y is
// placed through the reversed y map and still spans from the left edge to the right.
#[test]
fn blended_coordinates_compose_with_reversed_axes() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.ax(ax).x.limits = Limits::Manual {
        min: 0.0,
        max: 10.0,
    };
    fx.ax(ax).x.reverse = true;
    fx.ax(ax).y.limits = Limits::Manual { min: 0.0, max: 4.0 };
    fx.ax(ax).y.reverse = true;
    let ticks = fx.line(ax, &[2.0], &[0.1], None, |l| {
        l.y_coords = Coords::AxesFraction;
        l.marker.shape = MarkerShape::VLine;
    });
    let reference = hline(&mut fx, ax, 1.0);
    let scene = compile_figure(&fx.build());
    let plot = plot(&scene, ax);

    let tick = drawn(&scene, ticks);
    assert_eq!(tick.len(), 1);
    // x = 2 of 0..10 drawn from the right: a fifth of the width in from the right edge.
    assert_close(tick[0].1.x, plot.right() - 0.2 * plot.width, TOL);
    // A tenth of the height up from the bottom edge, whatever the direction of y.
    assert_close(tick[0].1.y, fraction_y(plot, 0.1), TOL);

    let line = drawn(&scene, reference);
    assert_eq!(line.len(), 2);
    // The fraction 0 is the left edge and 1 the right edge, although x runs from the right.
    assert_close(line[0].1.x, plot.x, TOL);
    assert_close(line[1].1.x, plot.right(), TOL);
    // y = 1 of 0..4 drawn from the top: a quarter of the height down from the top edge.
    for (_, p) in line {
        assert_close(p.y, plot.y + 0.25 * plot.height, TOL);
    }
}
