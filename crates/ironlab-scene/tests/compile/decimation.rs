//! Thinning of dense series to the current view, and the index map back to the source data that
//! the hit map publishes for picking and datatips.

use ironlab_ir::{Limits, MarkerShape, NodeId, ScatterColor, View3d};
use ironlab_scene::Scene;
use ironlab_scene::display::Point;
use ironlab_scene::maths::decimate::{Sample, target_points};

use crate::common::{Fx, compile_figure};
use crate::probe::{Leaf, assert_close, axes_hit, axis_maps, centre, from_source, leaves};

/// `n` points spread deterministically over a rectangle, as a dense scatter fills its plot.
fn spread(n: usize) -> (Vec<f64>, Vec<f64>) {
    (0..n)
        .map(|i| {
            let x = i as f64 / n as f64;
            let y = ((i as u64).wrapping_mul(2_654_435_761) % 10_000) as f64 / 10_000.0;
            (x, y)
        })
        .unzip()
}

/// A dense sine of `n` points whose x values are the indices, so a source index reads directly off
/// the x coordinate of a drawn point.
fn dense_sine(n: usize) -> (Vec<f64>, Vec<f64>) {
    (0..n).map(|i| (i as f64, (i as f64 / 250.0).sin())).unzip()
}

/// Returns the vertices of each subpath of the polyline an artist stroked, in figure space.
///
/// A 2D line emits its polyline as its first item and its markers after it, so the first leaf of
/// the artist is the polyline.
fn polyline_subpaths(scene: &Scene, id: NodeId) -> Vec<Vec<Point>> {
    from_source(&leaves(scene), id)
        .first()
        .map(Leaf::subpaths)
        .expect("the artist drew a polyline")
}

/// Returns the centres of the markers an artist drew, in figure space.
fn marker_centres(scene: &Scene, id: NodeId) -> Vec<Point> {
    from_source(&leaves(scene), id)
        .iter()
        .filter_map(|l| l.bbox())
        .map(centre)
        .collect()
}

/// Returns the drawn points the hit map records for an artist.
fn samples_of(scene: &Scene, id: NodeId) -> &[Sample] {
    &scene
        .hit_map
        .artists
        .iter()
        .find(|a| a.artist == id)
        .unwrap_or_else(|| panic!("hit map has no drawn points for artist {id}"))
        .samples
}

/// Asserts that every drawn point of an artist sits exactly where the source value of its own
/// index maps to, which is what makes the index map trustworthy.
#[track_caller]
fn assert_samples_match_source(scene: &Scene, ax: NodeId, id: NodeId, x: &[f64], y: &[f64]) {
    let (xmap, ymap) = axis_maps(scene, ax);
    for sample in samples_of(scene, id) {
        let i = sample.source_index;
        assert!(i < x.len(), "index {i} lies inside the source arrays");
        assert_close(sample.position.x, xmap.to_figure(x[i]), 1e-9);
        assert_close(sample.position.y, ymap.to_figure(y[i]), 1e-9);
    }
}

// Why: a million-point series drawn into a plot a few hundred points wide costs a million
// coordinates on the canvas and in the exported PDF to show detail no display and no printer can
// resolve. Thinning it to the resolution of the plot is the whole point of the feature, so the
// drawn geometry must be bounded by the size of the plot rather than by the size of the data.
#[test]
fn a_dense_line_is_drawn_with_no_more_vertices_than_the_plot_can_resolve() {
    let (x, y) = dense_sine(100_000);
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let id = fx.line(ax, &x, &y, None, |_| {});
    let scene = compile_figure(&fx.build());

    let drawn: usize = polyline_subpaths(&scene, id).iter().map(Vec::len).sum();
    let target = target_points(axes_hit(&scene, ax).plot_rect.width);
    assert!(
        drawn <= target,
        "{drawn} vertices drawn for a plot that resolves {target}"
    );
    assert!(
        drawn > 2,
        "the curve is still a curve, not a single segment"
    );
}

// Why: the axis limits are computed from the whole data range, so a thinned curve that stopped
// short of its first or last point would leave a gap against the end of the axes that the data
// does not have.
#[test]
fn a_decimated_line_still_begins_and_ends_at_the_data() {
    let (x, y) = dense_sine(60_000);
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let id = fx.line(ax, &x, &y, None, |_| {});
    let scene = compile_figure(&fx.build());

    let (xmap, ymap) = axis_maps(&scene, ax);
    let subpaths = polyline_subpaths(&scene, id);
    let first = subpaths.first().expect("the line is stroked")[0];
    let last = *subpaths
        .last()
        .expect("the line is stroked")
        .last()
        .unwrap();
    assert_close(first.x, xmap.to_figure(x[0]), 1e-9);
    assert_close(first.y, ymap.to_figure(y[0]), 1e-9);
    assert_close(last.x, xmap.to_figure(x[x.len() - 1]), 1e-9);
    assert_close(last.y, ymap.to_figure(y[y.len() - 1]), 1e-9);
}

// Why: the line artist's contract is that a non-finite value breaks the line, which is how a gap
// in a record is shown. Thinning buckets points by position in the series, so a rule applied
// across the whole series would happily draw a triangle over the gap and join the two halves.
// Each run of drawable points is therefore thinned on its own.
#[test]
fn a_non_finite_value_still_breaks_a_decimated_line() {
    let (x, mut y) = dense_sine(40_000);
    y[20_000] = f64::NAN;
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let id = fx.line(ax, &x, &y, None, |_| {});
    let scene = compile_figure(&fx.build());

    let (xmap, _) = axis_maps(&scene, ax);
    let subpaths = polyline_subpaths(&scene, id);
    assert_eq!(subpaths.len(), 2, "the gap splits the line into two runs");
    let end_of_first = xmap.to_data(subpaths[0].last().unwrap().x);
    let start_of_second = xmap.to_data(subpaths[1][0].x);
    assert!(
        end_of_first <= 19_999.0 + 1e-6 && start_of_second >= 20_001.0 - 1e-6,
        "no drawn segment spans the gap: the runs end at {end_of_first} and start at {start_of_second}"
    );
}

// Why: decimation is an optimisation, so a series the plot can already show in full must be drawn
// in full. A figure of a few dozen measurements must not have points quietly removed from it, and
// every one of them must remain pickable.
#[test]
fn a_series_the_plot_can_show_in_full_keeps_every_point() {
    let (x, y) = dense_sine(50);
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let id = fx.line(ax, &x, &y, None, |l| l.marker.shape = MarkerShape::Circle);
    let scene = compile_figure(&fx.build());

    let drawn: usize = polyline_subpaths(&scene, id).iter().map(Vec::len).sum();
    assert_eq!(drawn, 50, "every vertex of the polyline is drawn");
    let indices: Vec<usize> = samples_of(&scene, id)
        .iter()
        .map(|s| s.source_index)
        .collect();
    assert_eq!(indices, (0..50).collect::<Vec<_>>());
}

// Why: this is the guarantee the whole feature rests on. A datatip that reported a position in the
// thinned series would name the wrong measurement, and the user would have no way of telling.
// Every point the hit map publishes must therefore sit exactly where its own source value maps to.
#[test]
fn every_drawn_point_of_a_decimated_line_carries_its_index_in_the_source_data() {
    let (x, y) = dense_sine(80_000);
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let id = fx.line(ax, &x, &y, None, |_| {});
    let scene = compile_figure(&fx.build());

    let samples = samples_of(&scene, id);
    assert!(
        samples.len() < 80_000,
        "the series really was thinned, so the index map is doing work"
    );
    assert!(samples.iter().any(|s| s.source_index > 1000));
    assert_samples_match_source(&scene, ax, id, &x, &y);
}

// Why: picking is the consumer of the index map. A front end asks the hit map what lies under the
// pointer and must be handed the index the user's own arrays use, whatever decimation did, so that
// the value it shows is the value the user recorded.
#[test]
fn picking_a_decimated_series_reports_the_index_of_the_original_array() {
    let (x, y) = dense_sine(80_000);
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let id = fx.line(ax, &x, &y, None, |_| {});
    let scene = compile_figure(&fx.build());

    let wanted = samples_of(&scene, id)[137];
    let (artist, picked) = scene
        .hit_map
        .sample_at(wanted.position, 1.0)
        .expect("a drawn point lies under its own position");
    assert_eq!(artist.artist, id);
    assert_eq!(artist.axes, ax);
    assert_eq!(picked.source_index, wanted.source_index);

    let (xmap, ymap) = axis_maps(&scene, ax);
    assert_close(
        xmap.to_data(picked.position.x),
        x[picked.source_index],
        1e-6,
    );
    assert_close(
        ymap.to_data(picked.position.y),
        y[picked.source_index],
        1e-6,
    );
}

// Why: decimation is keyed on the view, so it must be redone when the view changes rather than
// cached against the data. Zooming in shows detail that was thinned away at the previous zoom, and
// a viewer that kept the earlier selection would show a curve that never sharpens.
#[test]
fn zooming_in_keeps_a_different_set_of_points() {
    let (x, y) = dense_sine(60_000);
    let indices_for = |limits: Limits| {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let id = fx.line(ax, &x, &y, None, |_| {});
        fx.ax(ax).x.limits = limits;
        let scene = compile_figure(&fx.build());
        let kept: Vec<usize> = samples_of(&scene, id)
            .iter()
            .map(|s| s.source_index)
            .collect();
        kept
    };
    let whole = indices_for(Limits::Auto);
    let zoomed = indices_for(Limits::Manual {
        min: 30_000.0,
        max: 31_000.0,
    });

    assert_ne!(whole, zoomed, "the view chose a different set of points");
    let inside = |kept: &[usize]| {
        kept.iter()
            .filter(|i| (30_000..=31_000).contains(*i))
            .count()
    };
    assert!(
        inside(&zoomed) > inside(&whole),
        "the zoomed view spends its points on the window it shows: {} against {}",
        inside(&zoomed),
        inside(&whole)
    );
}

// Why: the view a series is thinned for is the plot rectangle it is drawn into, not the figure. A
// series repeated across a row of subplots must be thinned harder in each of them, or a dense grid
// of small plots would cost as much as one large one per plot.
#[test]
fn a_narrower_plot_rectangle_keeps_fewer_points() {
    let (x, y) = dense_sine(60_000);
    let kept_in = |cols: u32| {
        let mut fx = Fx::new();
        fx.fig.layout.cols = cols;
        let ax = fx.axes2d(0, 0);
        let id = fx.line(ax, &x, &y, None, |_| {});
        let scene = compile_figure(&fx.build());
        (
            axes_hit(&scene, ax).plot_rect.width,
            samples_of(&scene, id).len(),
        )
    };
    let (wide_width, wide) = kept_in(1);
    let (narrow_width, narrow) = kept_in(4);

    assert!(
        narrow_width < wide_width,
        "the four-column plot is narrower"
    );
    assert!(
        narrow < wide,
        "the narrow plot keeps fewer points: {narrow} against {wide}"
    );
}

// Why: a dense scatter overplots itself — markers a few points wide cannot show a hundred thousand
// distinct positions in a plot a few hundred points across — so its cost must be set by the plot it
// is drawn into rather than by the size of the data. Quadrupling the data over the same region must
// therefore leave the picture, and the work of drawing it, essentially unchanged.
#[test]
fn a_dense_scatter_costs_what_the_plot_can_show_rather_than_what_the_data_holds() {
    let markers_for = |n: usize| {
        let (x, y) = spread(n);
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let id = fx.scatter(ax, &x, &y, None, |_, _| {});
        let scene = compile_figure(&fx.build());
        assert_eq!(
            marker_centres(&scene, id).len(),
            samples_of(&scene, id).len(),
            "the hit map lists exactly the markers that were drawn"
        );
        assert_samples_match_source(&scene, ax, id, &x, &y);
        marker_centres(&scene, id).len()
    };
    let hundred_thousand = markers_for(100_000);
    let four_hundred_thousand = markers_for(400_000);

    assert!(
        hundred_thousand < 100_000 / 3,
        "{hundred_thousand} markers drawn for 100000 overlapping points"
    );
    assert!(
        four_hundred_thousand < hundred_thousand * 21 / 20,
        "four times the data drew {four_hundred_thousand} markers against {hundred_thousand}, so the \
         count has saturated at what the plot can show"
    );
}

// Why: binning must only ever remove a marker that another marker covers. Points the plot places
// far apart are all separately visible, so a sparse scatter must keep every one of them however
// many the series holds elsewhere.
#[test]
fn a_scatter_whose_markers_the_view_separates_keeps_all_of_them() {
    let x: Vec<f64> = (0..300).map(|i| i as f64).collect();
    let y: Vec<f64> = (0..300).map(|i| (i % 7) as f64).collect();
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let id = fx.scatter(ax, &x, &y, None, |_, _| {});
    let scene = compile_figure(&fx.build());

    assert_eq!(marker_centres(&scene, id).len(), 300);
}

// Why: a colour-mapped scatter reads its colour from a third array by index. If a decimated marker
// carried a position from one index and a colour from another the picture would be wrong in a way
// no one could see, so the drawn marker and the index it reports must agree.
#[test]
fn a_decimated_colour_mapped_scatter_keeps_its_colours_aligned_with_its_points() {
    let (x, y) = dense_sine(40_000);
    let c: Vec<f64> = (0..40_000).map(|i| (i % 100) as f64).collect();
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let id = fx.scatter(ax, &x, &y, None, |s, fx| {
        s.color = ScatterColor::Data {
            data: fx.vector(&c),
        };
    });
    let scene = compile_figure(&fx.build());

    assert!(samples_of(&scene, id).len() < 40_000);
    assert_samples_match_source(&scene, ax, id, &x, &y);
}

// Why: in a 3D axes the view is the camera rather than the axis limits, and a curve that projects
// to a straight line from one angle is a spiral from another. Thinning must follow the camera, or a
// reader who rotates the box would be shown the detail of the view they turned away from.
#[test]
fn rotating_a_three_dimensional_view_keeps_a_different_set_of_points() {
    let n = 40_000;
    // A curve that wanders quickly in x and slowly in y as it climbs in z, so which of its wiggles
    // carries the shape of the projected curve depends on where the camera stands.
    let x: Vec<f64> = (0..n).map(|i| (i as f64 / 13.0).sin()).collect();
    let y: Vec<f64> = (0..n).map(|i| (i as f64 / 29.0).sin()).collect();
    let t: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let kept_at = |azimuth_deg: f64| {
        let mut fx = Fx::new();
        let ax = fx.axes3d(
            0,
            0,
            View3d {
                azimuth_deg,
                ..View3d::default()
            },
        );
        let id = fx.line(ax, &x, &y, Some(&t), |_| {});
        let scene = compile_figure(&fx.build());
        samples_of(&scene, id)
            .iter()
            .map(|s| s.source_index)
            .collect::<Vec<_>>()
    };

    assert_ne!(
        kept_at(-37.5),
        kept_at(52.5),
        "the camera chose a different set of points"
    );
}
