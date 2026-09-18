//! Thinning of large point series: the target point count, the largest-triangle-three-buckets rule
//! for polylines and the binning rule for markers.

use ironlab_scene::display::Point;
use ironlab_scene::maths::decimate::{
    MIN_TARGET_POINTS, SAMPLES_PER_POINT, Sample, bin, largest_triangle_three_buckets,
    target_points,
};
use proptest::prelude::*;

/// Builds a series whose source indices are its positions in `points`, all at depth zero.
fn series(points: &[(f64, f64)]) -> Vec<Sample> {
    points
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| Sample {
            source_index: i,
            position: Point::new(x, y),
            depth: 0.0,
        })
        .collect()
}

/// A series of `n` points along a straight line, one unit apart.
fn straight(n: usize) -> Vec<Sample> {
    series(&(0..n).map(|i| (i as f64, i as f64)).collect::<Vec<_>>())
}

fn indices(samples: &[Sample]) -> Vec<usize> {
    samples.iter().map(|s| s.source_index).collect()
}

// Why: decimation exists to bound the drawn geometry by the size of the plot rather than by the
// size of the data, so the target must follow the plot width. The floor matters because a figure
// can be a few millimetres wide, or a subplot grid can be dense, and a curve thinned to a handful
// of points would visibly change shape.
#[test]
fn the_target_follows_the_plot_width_and_never_drops_below_the_floor() {
    let wide = 400.0;
    assert_eq!(
        target_points(wide),
        (wide * SAMPLES_PER_POINT) as usize,
        "a wide plot is decimated in proportion to its width"
    );
    assert_eq!(target_points(1.0), MIN_TARGET_POINTS);
    assert_eq!(target_points(0.0), MIN_TARGET_POINTS);
    assert_eq!(target_points(f64::NAN), MIN_TARGET_POINTS);
}

// Why: decimation is an optimisation, not a style. A series the view can already show point for
// point must be drawn point for point, so that every ordinary figure compiles to exactly the
// geometry it did before and nothing about a small plot is silently approximated.
#[test]
fn a_series_no_longer_than_the_target_is_returned_unchanged() {
    let run = straight(64);
    assert_eq!(largest_triangle_three_buckets(&run, 64), run);
    assert_eq!(largest_triangle_three_buckets(&run, 1000), run);
}

// Why: the axis limits are computed from the data, so a curve that lost its first or last point
// would no longer reach the ends of the axes it is drawn against, and the reader would see a gap
// that the data does not have.
#[test]
fn the_first_and_last_points_always_survive() {
    let run = straight(5000);
    let kept = largest_triangle_three_buckets(&run, 300);
    assert_eq!(kept.first(), run.first());
    assert_eq!(kept.last(), run.last());
    assert_eq!(kept.len(), 300, "the rule fills its target exactly");
}

// Why: this is the whole reason for choosing largest-triangle-three-buckets over taking every nth
// point. A transient in a signal — the spike a reader is looking for — falls between uniform
// samples and disappears, while the triangle-area measure is largest exactly there.
#[test]
fn an_isolated_spike_survives_where_uniform_sampling_would_lose_it() {
    let mut points: Vec<(f64, f64)> = (0..1000).map(|i| (i as f64, 0.0)).collect();
    points[553].1 = 100.0;
    let kept = largest_triangle_three_buckets(&series(&points), 10);
    assert!(
        indices(&kept).contains(&553),
        "the spike survived; kept {:?}",
        indices(&kept)
    );
    assert!(
        !(0..10).map(|k| k * 100).any(|i| i == 553),
        "uniform sampling at the same rate would step straight over the spike"
    );
}

// Why: a datatip must report the index the user's data has. If thinning reordered a series or
// returned a point under the wrong index, every reported value would be someone else's, which is
// worse than no datatip at all.
#[test]
fn thinning_keeps_each_point_under_its_own_source_index() {
    let points: Vec<(f64, f64)> = (0..4000)
        .map(|i| (i as f64, (i as f64 / 37.0).sin()))
        .collect();
    let run = series(&points);
    let kept = largest_triangle_three_buckets(&run, 500);
    for sample in &kept {
        let (x, y) = points[sample.source_index];
        assert_eq!(
            sample.position,
            Point::new(x, y),
            "sample {} holds the position of source point {}",
            sample.source_index,
            sample.source_index
        );
    }
    assert!(
        indices(&kept).windows(2).all(|w| w[0] < w[1]),
        "the series keeps its order, so the polyline is not scrambled"
    );
}

// Why: markers are painted in order and the last one drawn covers those beneath it. Keeping the
// covering marker rather than a covered one means a decimated scatter shows the same colours and
// sizes on top that the undecimated one would.
#[test]
fn binning_keeps_the_marker_that_would_be_painted_over_the_others() {
    let crowded = series(&[(0.1, 0.1), (0.4, 0.4), (0.9, 0.9)]);
    let kept = bin(&crowded, 1.0);
    assert_eq!(
        indices(&kept),
        vec![2],
        "the last marker of the square wins"
    );
}

// Why: in a 3D axes geometry is painted back to front, so the marker on top is the one nearest the
// viewer rather than the last in the data. Binning by source order there would drop the marker the
// reader can actually see.
#[test]
fn binning_in_three_dimensions_keeps_the_marker_nearest_the_viewer() {
    let crowded = vec![
        Sample {
            source_index: 0,
            position: Point::new(0.1, 0.1),
            depth: 5.0,
        },
        Sample {
            source_index: 1,
            position: Point::new(0.4, 0.4),
            depth: -5.0,
        },
    ];
    let kept = bin(&crowded, 1.0);
    assert_eq!(
        indices(&kept),
        vec![0],
        "the deepest sample is painted last and so lies on top"
    );
}

// Why: binning must only remove markers the reader could not tell apart. Markers spread more than
// one square apart are all distinguishable, so every one of them has to survive, in the order they
// were painted.
#[test]
fn binning_keeps_every_marker_the_view_separates() {
    let spread = series(&[(0.0, 0.0), (2.0, 0.0), (4.0, 0.0), (6.0, 3.0)]);
    let kept = bin(&spread, 1.0);
    assert_eq!(indices(&kept), vec![0, 1, 2, 3]);
}

proptest! {
    // Why: the compiler runs this rule on whatever data a user loads, so it must not panic, lose the
    // ends of the series or scramble its order for any length, any target or any spread of values.
    #[test]
    fn thinning_any_series_gives_an_ordered_subsequence_within_the_target(
        values in prop::collection::vec(-1e6f64..1e6, 2..400),
        target in 0usize..400,
    ) {
        let points: Vec<(f64, f64)> = values.iter().enumerate().map(|(i, v)| (i as f64, *v)).collect();
        let run = series(&points);
        let kept = largest_triangle_three_buckets(&run, target);

        prop_assert!(kept.len() <= run.len());
        prop_assert!(target < 3 || kept.len() <= target);
        prop_assert_eq!(kept.first(), run.first());
        prop_assert_eq!(kept.last(), run.last());
        prop_assert!(indices(&kept).windows(2).all(|w| w[0] < w[1]));
        for sample in &kept {
            prop_assert_eq!(*sample, run[sample.source_index]);
        }
    }
}
