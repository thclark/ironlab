//! Thinning of large point series to the resolution the current view can show.
//!
//! A series of a million points drawn into a plot a few hundred points wide cannot show a million
//! distinct positions, so most of the geometry is invisible: it costs time on screen and bytes in
//! the exported vector file without changing the picture. Decimation removes that geometry before
//! it reaches the display list, once, so that the screen and the export agree exactly.
//!
//! Every function here works on [`Sample`]s, which carry the index each point has in the artist's
//! own data arrays. A decimated series therefore cannot be separated from the index map that
//! explains where its points came from, and picking a drawn point always yields the index the
//! user's data has rather than a position in the thinned series.
//!
//! Two rules are used, chosen for the two shapes data takes:
//!
//! - [`largest_triangle_three_buckets`] thins a polyline. It keeps the points that carry the shape
//!   of the curve, including its peaks and troughs, which uniform subsampling loses.
//! - [`bin`] thins a set of markers. Markers are discrete symbols, each far wider than the spacing
//!   of a dense series, so keeping one per square of the plot preserves what the eye can resolve.

use std::collections::HashMap;

use crate::display::Point;

/// A drawn point of a series together with the index it has in the artist's own data arrays.
///
/// The position and the source index are one value, so no step of drawing can return positions
/// without the index map that goes with them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// The index of the point in the artist's data arrays, before any decimation.
    pub source_index: usize,
    /// The position of the point in figure space, in points.
    pub position: Point,
    /// The depth at which the point is painted in a 3D axes; zero in a 2D axes.
    pub depth: f64,
}

/// The number of drawn points per point of plot width that decimation aims to keep.
///
/// One point is 1/72 inch, so four samples per point resolves detail 1/288 inch apart: finer than
/// any display shows and finer than print resolves, while bounding the drawn geometry by the size
/// of the plot rather than by the size of the data.
pub const SAMPLES_PER_POINT: f64 = 4.0;

/// The smallest number of points that decimation ever reduces a series to.
///
/// A plot rectangle can be very small, in a dense grid of subplots or in a figure a few
/// millimetres wide, and a curve thinned to a handful of points would visibly change shape. This
/// floor keeps enough points for the curve to stay recognisable whatever the plot measures.
pub const MIN_TARGET_POINTS: usize = 256;

/// Returns the number of points a plot rectangle `width_pt` points wide is decimated to.
///
/// A non-finite or non-positive width, which a degenerate layout can produce, gives the floor.
pub fn target_points(width_pt: f64) -> usize {
    if !(width_pt.is_finite() && width_pt > 0.0) {
        return MIN_TARGET_POINTS;
    }
    ((width_pt * SAMPLES_PER_POINT) as usize).max(MIN_TARGET_POINTS)
}

/// Returns twice the area of the triangle `a`, `b`, `c`.
fn double_area(a: Point, b: Point, c: Point) -> f64 {
    ((a.x - c.x) * (b.y - a.y) - (a.x - b.x) * (c.y - a.y)).abs()
}

/// Returns the mean position of a non-empty slice of samples.
fn mean_position(samples: &[Sample]) -> Point {
    let n = samples.len() as f64;
    Point::new(
        samples.iter().map(|s| s.position.x).sum::<f64>() / n,
        samples.iter().map(|s| s.position.y).sum::<f64>() / n,
    )
}

/// Thins a polyline to at most `target` points by the largest-triangle-three-buckets rule.
///
/// The first and last points are always kept, so a decimated curve starts and ends exactly where
/// the data does. The points between them are divided into `target - 2` consecutive buckets of
/// equal size, and one point is kept from each: the one forming the largest triangle with the
/// point kept from the previous bucket and the mean of the next bucket. That measure is largest at
/// the extremes and the corners of the curve, so peaks, troughs and discontinuities survive, which
/// is what distinguishes the rule from taking every *n*th point.
///
/// Areas are measured in figure space, so the result depends on the view: the same data thinned
/// for a different zoom, pan or 3D camera keeps different points.
///
/// A series no longer than the target, or a target below three, is returned unchanged.
pub fn largest_triangle_three_buckets(run: &[Sample], target: usize) -> Vec<Sample> {
    if target < 3 || run.len() <= target {
        return run.to_vec();
    }
    let buckets = target - 2;
    // Every bucket holds at least one point, because the points between the first and the last
    // outnumber the buckets whenever the run is longer than the target.
    let inner = run.len() - 2;
    let edge = |b: usize| 1 + b * inner / buckets;

    let mut kept = Vec::with_capacity(target);
    kept.push(run[0]);
    let mut previous = run[0];
    for b in 0..buckets {
        let next = if b + 1 < buckets {
            mean_position(&run[edge(b + 1)..edge(b + 2)])
        } else {
            run[run.len() - 1].position
        };
        let chosen = run[edge(b)..edge(b + 1)]
            .iter()
            .copied()
            .max_by(|p, q| {
                double_area(previous.position, p.position, next).total_cmp(&double_area(
                    previous.position,
                    q.position,
                    next,
                ))
            })
            .expect("every bucket holds at least one point");
        kept.push(chosen);
        previous = chosen;
    }
    kept.push(run[run.len() - 1]);
    kept
}

/// Thins a set of markers to at most one per `bin_pt` square of figure space.
///
/// The sample kept in a square is the one that would be painted last, and so lies on top of the
/// others: the deepest in a 3D axes, where geometry is painted back to front, and otherwise the
/// last in source order. The kept samples are returned in source order, so the order in which the
/// survivors are painted is the order they would have had undecimated.
///
/// A non-finite or non-positive bin size returns the samples unchanged.
pub fn bin(samples: &[Sample], bin_pt: f64) -> Vec<Sample> {
    if !(bin_pt.is_finite() && bin_pt > 0.0) {
        return samples.to_vec();
    }
    let mut squares: HashMap<(i64, i64), Sample> = HashMap::new();
    for sample in samples {
        let key = (
            (sample.position.x / bin_pt).floor() as i64,
            (sample.position.y / bin_pt).floor() as i64,
        );
        let occupant = squares.entry(key).or_insert(*sample);
        let later = occupant
            .depth
            .total_cmp(&sample.depth)
            .then(occupant.source_index.cmp(&sample.source_index));
        if later.is_lt() {
            *occupant = *sample;
        }
    }
    let mut kept: Vec<Sample> = squares.into_values().collect();
    kept.sort_by_key(|s| s.source_index);
    kept
}
