//! Binning of samples into histograms, in one and two dimensions.
//!
//! The functions in this module are pure: they take a sample and a specification of the
//! bins, and return the bin edges, the normalised value of each bin and a summary of the
//! samples that were left out. They follow the semantics of `numpy.histogram_bin_edges`,
//! `numpy.histogram` and `numpy.histogram2d`, with MATLAB's names for the normalisations
//! and MATLAB's `'integers'` rule:
//!
//! - The range of the bins is the range of the finite samples, unless an explicit range is
//!   given. A range of zero width, `[v, v]`, is widened to `[v - 0.5, v + 0.5]`.
//! - Every bin is half open, `[a, b)`, except the last, which is closed, `[a, b]`, so that
//!   the largest sample is counted.
//! - Samples that are not finite (or whose weight is not finite) are excluded, as are
//!   samples outside an explicit range or outside explicit edges. The
//!   [`HistogramSummary`] reports how many were excluded for each reason.
//! - The total used by the normalisations is the number of included samples, or the sum
//!   of their weights when weights are given.
//!
//! The bin specification, the rules and the normalisations are the model's own types,
//! [`Bins`], [`BinRule`] and [`HistNorm`], so that the record of how a histogram was
//! binned, which the figure stores with the binned values (ADR 0029), holds exactly what
//! the author gave these functions. They mean the following here:
//!
//! - [`Bins::Count`] gives that number of bins of equal width across the range, computed
//!   as `numpy.linspace` computes them. [`Bins::Width`] gives bins of that width from the
//!   minimum of the range, the last edge being the first one at or beyond the maximum, so
//!   that the last bin may extend beyond the data. [`Bins::Edges`] gives the bins between
//!   the edges, which must be finite and strictly increasing, and excludes the samples
//!   outside the first and last edges.
//! - Every rule except [`BinRule::Integers`] estimates a bin width from the samples inside
//!   the range and divides the range into the smallest number of equal bins no wider than
//!   that width; a width of zero, as for a sample whose values are all equal, gives one
//!   bin. [`BinRule::Auto`] is numpy's `'auto'`: the smaller of the Sturges width and the
//!   Freedman–Diaconis width, the latter first raised to at least half the square-root
//!   width. [`BinRule::Sturges`] divides the extent by `log2(n) + 1`, [`BinRule::Scott`]
//!   takes `(24 √π / n)^(1/3)` times the standard deviation, [`BinRule::FreedmanDiaconis`]
//!   takes twice the interquartile range divided by the cube root of `n`, and
//!   [`BinRule::Sqrt`] divides the extent by `√n`. [`BinRule::Integers`] is MATLAB's
//!   `'integers'`: one bin of unit width centred on each integer that the range spans.
//! - The count of a bin is the number of included samples in it, or the sum of their
//!   weights. [`HistNorm::Probability`] divides it by the total and
//!   [`HistNorm::Percentage`] multiplies that by 100; [`HistNorm::CountDensity`] divides it
//!   by the width of the bin, and [`HistNorm::Density`] divides the probability by the
//!   width, so that the area of the histogram is one (numpy's `density=True`, MATLAB's
//!   `'pdf'`). [`HistNorm::CumulativeCount`] adds the counts of the bins before, and
//!   [`HistNorm::Cdf`] divides that by the total, ending at one. In two dimensions the
//!   width of a bin is its area, and the cumulative normalisations accumulate over both
//!   dimensions.
//!
//! IronLAB departs from numpy in three places, each to keep a figure valid rather than
//! raise an error. A bin rule applied to a sample with no finite samples (inside the
//! range, if one is given) gives no bins, where numpy gives one bin on `[0, 1]`. A bin rule
//! applied to a weighted sample estimates the bins from the unweighted sample, where numpy
//! refuses. Non-finite samples are excluded and reported, where numpy raises an error.
//!
//! ```
//! use ironlab::binning::{Bins, HistNorm, histogram};
//!
//! let sample = [1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0];
//! let h = histogram(&sample, None, &Bins::Count(4), None, HistNorm::Count)?;
//! assert_eq!(h.edges, [1.0, 1.75, 2.5, 3.25, 4.0]);
//! assert_eq!(h.values, [1.0, 2.0, 3.0, 4.0]);
//! # Ok::<(), ironlab::binning::BinningError>(())
//! ```

use crate::Matrix;

/// The largest number of bins that a specification may produce. A bin rule applied to a
/// sample with extreme outliers, or a very small bin width, could otherwise exhaust the
/// memory of the machine.
pub const MAX_BINS: usize = 1 << 24;

/// An explicit range of values, `(min, max)`, that the bins of one dimension of a
/// histogram span. Samples outside it are excluded.
pub type BinRange = (f64, f64);

pub use ironlab_ir::{BinRule, Bins, HistNorm};

/// How a one-dimensional histogram is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistStyle {
    /// A bar for each bin, spanning the bin, with neighbouring bars touching.
    #[default]
    Bar,
    /// The outline of the bins as a stepped line, without a fill.
    Step,
    /// The outline of the bins as a stepped line, filled down to zero.
    FilledStep,
}

/// How many samples a histogram includes, and how many it excludes for each reason.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HistogramSummary {
    /// The number of included samples, or the sum of their weights when weights are
    /// given. This is the total that the normalisations divide by.
    pub total: f64,
    /// The number of samples excluded because a value (or its weight) is NaN or infinite.
    pub excluded_non_finite: usize,
    /// The number of finite samples excluded because they lie outside the explicit range
    /// or outside the explicit edges.
    pub excluded_out_of_range: usize,
}

/// A one-dimensional histogram: the bin edges and the normalised value of each bin.
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    /// The edges of the bins, in increasing order: one more than the number of bins, or
    /// none when a bin rule found no samples to estimate from.
    pub edges: Vec<f64>,
    /// The value of each bin, normalised as requested.
    pub values: Vec<f64>,
    /// The number of samples included and excluded.
    pub summary: HistogramSummary,
}

/// A two-dimensional histogram: the bin edges in each dimension and the normalised value
/// of each cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram2d {
    /// The edges of the bins of the x values, in increasing order.
    pub x_edges: Vec<f64>,
    /// The edges of the bins of the y values, in increasing order.
    pub y_edges: Vec<f64>,
    /// The value of each cell, normalised as requested, with one row for each y bin and
    /// one column for each x bin: the value in row `j` and column `i` belongs to the cell
    /// between `x_edges[i]` and `x_edges[i + 1]`, and between `y_edges[j]` and
    /// `y_edges[j + 1]`. A cell whose count is below the minimum count is NaN.
    pub values: Matrix,
    /// The number of samples included and excluded.
    pub summary: HistogramSummary,
}

/// A reason why a sample cannot be binned as specified.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BinningError {
    /// The number of bins is zero.
    #[error("a histogram needs at least one bin")]
    ZeroBins,
    /// The bin width is not a finite, positive number.
    #[error("the bin width {0} is not a finite, positive number")]
    InvalidWidth(f64),
    /// The explicit edges are fewer than two, are not all finite, or do not strictly
    /// increase.
    #[error("bin edges must be at least two finite numbers that strictly increase")]
    InvalidEdges,
    /// The explicit range is not finite, or its minimum exceeds its maximum.
    #[error("the range [{0}, {1}] is not finite or its minimum exceeds its maximum")]
    InvalidRange(f64, f64),
    /// The weights, or the y values of a two-dimensional histogram, do not have one entry
    /// for each sample.
    #[error("expected {expected} values, one for each sample, but found {actual}")]
    LengthMismatch {
        /// The number of samples.
        expected: usize,
        /// The number of weights or y values given.
        actual: usize,
    },
    /// The bins are too many for the range: either the edges cannot be told apart in
    /// floating-point arithmetic, or there are more than [`MAX_BINS`].
    #[error("cannot create {0} bins of finite, distinct width in the range")]
    TooManyBins(usize),
}

/// Returns the edges of the bins that [`histogram`] uses for the sample, which is the
/// equivalent of `numpy.histogram_bin_edges`.
///
/// Non-finite samples are ignored. The result is empty when a bin rule finds no finite
/// samples (inside the range, if one is given) to estimate from.
///
/// # Errors
///
/// Returns an error when the bins or the range are invalid, as described by
/// [`BinningError`].
pub fn bin_edges(
    sample: &[f64],
    bins: &Bins,
    range: Option<(f64, f64)>,
) -> Result<Vec<f64>, BinningError> {
    validate(bins, range)?;
    let finite: Vec<f64> = sample.iter().copied().filter(|v| v.is_finite()).collect();
    edges_for(&finite, bins, range)
}

/// Bins a sample, optionally weighted, and normalises the bins, which is the equivalent
/// of `numpy.histogram` (and of MATLAB's `histcounts`).
///
/// # Errors
///
/// Returns an error when the weights do not have one entry for each sample, or when the
/// bins or the range are invalid, as described by [`BinningError`].
///
/// ```
/// use ironlab::binning::{Bins, HistNorm, histogram};
///
/// let h = histogram(&[0.0, 0.5, 1.0, f64::NAN], None, &Bins::Count(2), None, HistNorm::Probability)?;
/// assert_eq!(h.values, [1.0 / 3.0, 2.0 / 3.0]);
/// assert_eq!(h.summary.excluded_non_finite, 1);
/// # Ok::<(), ironlab::binning::BinningError>(())
/// ```
pub fn histogram(
    sample: &[f64],
    weights: Option<&[f64]>,
    bins: &Bins,
    range: Option<(f64, f64)>,
    norm: HistNorm,
) -> Result<Histogram, BinningError> {
    check_length(sample.len(), weights.map(<[f64]>::len))?;
    validate(bins, range)?;

    let mut summary = HistogramSummary::default();
    let mut values = Vec::with_capacity(sample.len());
    let mut kept_weights = Vec::with_capacity(sample.len());
    for (i, &v) in sample.iter().enumerate() {
        let w = weights.map_or(1.0, |w| w[i]);
        if v.is_finite() && w.is_finite() {
            values.push(v);
            kept_weights.push(w);
        } else {
            summary.excluded_non_finite += 1;
        }
    }

    let edges = edges_for(&values, bins, range)?;
    let bounds = Bounds::new(&edges, range);
    let mut counts = vec![0.0; edges.len().saturating_sub(1)];
    for (&v, &w) in values.iter().zip(&kept_weights) {
        match bounds.index(&edges, v) {
            Some(i) => {
                counts[i] += w;
                summary.total += w;
            }
            None => summary.excluded_out_of_range += 1,
        }
    }

    let widths: Vec<f64> = edges.windows(2).map(|e| e[1] - e[0]).collect();
    let values = normalise(&counts, &widths, summary.total, norm, cumulative_1d);
    Ok(Histogram {
        edges,
        values,
        summary,
    })
}

/// Bins pairs of samples, optionally weighted, into cells and normalises the cells, which
/// is the equivalent of `numpy.histogram2d` (and of MATLAB's `histcounts2`).
///
/// The bins and the range are given for the x values and the y values in turn, and each
/// is applied as [`histogram`] applies it to one dimension. A pair is excluded when either
/// of its values, or its weight, is not finite, or when either value lies outside its
/// range or edges. Cells whose count (before normalisation) is below `min_count` are NaN,
/// as with matplotlib's `cmin`, so that a colour scale draws them as missing.
///
/// # Errors
///
/// Returns an error when the y values or the weights do not have one entry for each x
/// value, or when the bins or a range are invalid, as described by [`BinningError`].
///
/// ```
/// use ironlab::binning::{Bins, HistNorm, histogram2d};
///
/// let x = [0.0, 1.0, 1.0];
/// let y = [0.0, 0.0, 1.0];
/// let bins = Bins::Count(2);
/// let h = histogram2d(&x, &y, None, (&bins, &bins), (None, None), HistNorm::Count, None)?;
/// assert_eq!(h.values.row(0), &[1.0, 1.0]);
/// assert_eq!(h.values.row(1), &[0.0, 1.0]);
/// # Ok::<(), ironlab::binning::BinningError>(())
/// ```
pub fn histogram2d(
    x: &[f64],
    y: &[f64],
    weights: Option<&[f64]>,
    bins: (&Bins, &Bins),
    range: (Option<BinRange>, Option<BinRange>),
    norm: HistNorm,
    min_count: Option<f64>,
) -> Result<Histogram2d, BinningError> {
    check_length(x.len(), Some(y.len()))?;
    check_length(x.len(), weights.map(<[f64]>::len))?;
    validate(bins.0, range.0)?;
    validate(bins.1, range.1)?;

    let mut summary = HistogramSummary::default();
    let mut xs = Vec::with_capacity(x.len());
    let mut ys = Vec::with_capacity(x.len());
    let mut kept_weights = Vec::with_capacity(x.len());
    for (i, (&xv, &yv)) in x.iter().zip(y).enumerate() {
        let w = weights.map_or(1.0, |w| w[i]);
        if xv.is_finite() && yv.is_finite() && w.is_finite() {
            xs.push(xv);
            ys.push(yv);
            kept_weights.push(w);
        } else {
            summary.excluded_non_finite += 1;
        }
    }

    let x_edges = edges_for(&xs, bins.0, range.0)?;
    let y_edges = edges_for(&ys, bins.1, range.1)?;
    let x_bounds = Bounds::new(&x_edges, range.0);
    let y_bounds = Bounds::new(&y_edges, range.1);
    let nx = x_edges.len().saturating_sub(1);
    let ny = y_edges.len().saturating_sub(1);
    let mut counts = vec![0.0; nx * ny];
    for ((&xv, &yv), &w) in xs.iter().zip(&ys).zip(&kept_weights) {
        match (x_bounds.index(&x_edges, xv), y_bounds.index(&y_edges, yv)) {
            (Some(i), Some(j)) => {
                counts[j * nx + i] += w;
                summary.total += w;
            }
            _ => summary.excluded_out_of_range += 1,
        }
    }

    let x_widths: Vec<f64> = x_edges.windows(2).map(|e| e[1] - e[0]).collect();
    let y_widths: Vec<f64> = y_edges.windows(2).map(|e| e[1] - e[0]).collect();
    let areas: Vec<f64> = y_widths
        .iter()
        .flat_map(|wy| x_widths.iter().map(move |wx| wx * wy))
        .collect();
    let accumulate = |c: Vec<f64>| cumulative_2d(c, nx, ny);
    let mut values = normalise(&counts, &areas, summary.total, norm, accumulate);
    if let Some(min_count) = min_count {
        for (value, &count) in values.iter_mut().zip(&counts) {
            if count < min_count {
                *value = f64::NAN;
            }
        }
    }

    let values = Matrix::from_vec(ny, nx, values).expect("one value for each cell");
    Ok(Histogram2d {
        x_edges,
        y_edges,
        values,
        summary,
    })
}

/// Checks that a second array, when given, has one entry for each sample.
fn check_length(expected: usize, actual: Option<usize>) -> Result<(), BinningError> {
    match actual {
        Some(actual) if actual != expected => {
            Err(BinningError::LengthMismatch { expected, actual })
        }
        _ => Ok(()),
    }
}

/// Checks the bins and the range independently of the sample.
fn validate(bins: &Bins, range: Option<(f64, f64)>) -> Result<(), BinningError> {
    match bins {
        Bins::Count(0) => return Err(BinningError::ZeroBins),
        Bins::Width(w) if !(w.is_finite() && *w > 0.0) => {
            return Err(BinningError::InvalidWidth(*w));
        }
        Bins::Edges(edges)
            if edges.len() < 2
                || !edges.iter().all(|e| e.is_finite())
                || edges.windows(2).any(|e| e[0] >= e[1]) =>
        {
            return Err(BinningError::InvalidEdges);
        }
        _ => {}
    }
    if let Some((lo, hi)) = range
        && !(lo.is_finite() && hi.is_finite() && lo <= hi)
    {
        return Err(BinningError::InvalidRange(lo, hi));
    }
    Ok(())
}

/// Returns the outer edges of the bins: the explicit range, or the range of the finite
/// samples, or `[0, 1]` when there are none, widened by a half on each side when its
/// width is zero (numpy's `_get_outer_edges`).
fn outer_edges(values: &[f64], range: Option<(f64, f64)>) -> (f64, f64) {
    let (lo, hi) = range.unwrap_or_else(|| min_max(values).unwrap_or((0.0, 1.0)));
    if lo == hi {
        (lo - 0.5, hi + 0.5)
    } else {
        (lo, hi)
    }
}

fn min_max(values: &[f64]) -> Option<(f64, f64)> {
    values.iter().fold(None, |acc, &v| match acc {
        None => Some((v, v)),
        Some((lo, hi)) => Some((lo.min(v), hi.max(v))),
    })
}

/// Computes the edges for finite samples; the bins and range have been validated.
fn edges_for(
    values: &[f64],
    bins: &Bins,
    range: Option<(f64, f64)>,
) -> Result<Vec<f64>, BinningError> {
    match bins {
        Bins::Count(n) => {
            let (lo, hi) = outer_edges(values, range);
            equal_edges(lo, hi, *n as usize)
        }
        Bins::Width(width) => {
            let (lo, hi) = outer_edges(values, range);
            width_edges(lo, hi, *width)
        }
        Bins::Edges(edges) => Ok(edges.clone()),
        Bins::Rule(rule) => rule_edges(values, *rule, range),
    }
}

/// Returns `n` equal bins from `lo` to `hi`, computed as `numpy.linspace` computes them.
fn equal_edges(lo: f64, hi: f64, n: usize) -> Result<Vec<f64>, BinningError> {
    if n > MAX_BINS {
        return Err(BinningError::TooManyBins(n));
    }
    let step = (hi - lo) / n as f64;
    let mut edges: Vec<f64> = (0..=n).map(|k| k as f64 * step + lo).collect();
    edges[n] = hi;
    distinct(edges, n)
}

/// Returns bins of the given width from `lo`, the last edge being the first one at or
/// beyond `hi`.
fn width_edges(lo: f64, hi: f64, width: f64) -> Result<Vec<f64>, BinningError> {
    let estimate = ((hi - lo) / width).ceil().max(1.0);
    if estimate > MAX_BINS as f64 {
        return Err(BinningError::TooManyBins(MAX_BINS.saturating_add(1)));
    }
    let edge = |k: usize| lo + k as f64 * width;
    let mut n = estimate as usize;
    // Correct the estimate for rounding, so that the last edge is the first one that
    // reaches hi.
    while n > 1 && edge(n - 1) >= hi {
        n -= 1;
    }
    while edge(n) < hi {
        n += 1;
    }
    distinct((0..=n).map(edge).collect(), n)
}

/// Returns the edges when they strictly increase, and an error otherwise.
fn distinct(edges: Vec<f64>, n: usize) -> Result<Vec<f64>, BinningError> {
    if edges.windows(2).all(|e| e[0] < e[1]) {
        Ok(edges)
    } else {
        Err(BinningError::TooManyBins(n))
    }
}

/// Returns the edges chosen by a rule, or none when there are no samples to estimate
/// from.
fn rule_edges(
    values: &[f64],
    rule: BinRule,
    range: Option<(f64, f64)>,
) -> Result<Vec<f64>, BinningError> {
    let (lo, hi) = outer_edges(values, range);
    let mut kept: Vec<f64> = values
        .iter()
        .copied()
        .filter(|v| range.is_none() || (lo..=hi).contains(v))
        .collect();
    if kept.is_empty() {
        return Ok(Vec::new());
    }

    if rule == BinRule::Integers {
        let (min, max) = range.unwrap_or_else(|| min_max(&kept).expect("a sample"));
        let first = (min + 0.5).floor() - 0.5;
        let last = (max + 0.5).floor() + 0.5;
        let n = (last - first).round();
        if n > MAX_BINS as f64 {
            return Err(BinningError::TooManyBins(MAX_BINS.saturating_add(1)));
        }
        return distinct(
            (0..=n as usize).map(|k| first + k as f64).collect(),
            n as usize,
        );
    }

    kept.sort_by(f64::total_cmp);
    let width = match rule {
        BinRule::Auto => {
            let fd = freedman_diaconis_width(&kept);
            let sqrt = sqrt_width(&kept);
            fd.max(sqrt / 2.0).min(sturges_width(&kept))
        }
        BinRule::Sturges => sturges_width(&kept),
        BinRule::Scott => scott_width(&kept),
        BinRule::FreedmanDiaconis => freedman_diaconis_width(&kept),
        BinRule::Sqrt => sqrt_width(&kept),
        BinRule::Integers => unreachable!("handled above"),
    };
    let n = if width > 0.0 {
        let n = ((hi - lo) / width).ceil();
        if n > MAX_BINS as f64 {
            return Err(BinningError::TooManyBins(MAX_BINS.saturating_add(1)));
        }
        n as usize
    } else {
        1
    };
    equal_edges(lo, hi, n)
}

/// The extent of a sorted, non-empty sample.
fn extent(sorted: &[f64]) -> f64 {
    sorted[sorted.len() - 1] - sorted[0]
}

fn sturges_width(sorted: &[f64]) -> f64 {
    extent(sorted) / ((sorted.len() as f64).log2() + 1.0)
}

fn sqrt_width(sorted: &[f64]) -> f64 {
    extent(sorted) / (sorted.len() as f64).sqrt()
}

fn scott_width(sorted: &[f64]) -> f64 {
    let n = sorted.len() as f64;
    let mean = sorted.iter().sum::<f64>() / n;
    let variance = sorted.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    (24.0 * std::f64::consts::PI.sqrt() / n).powf(1.0 / 3.0) * variance.sqrt()
}

fn freedman_diaconis_width(sorted: &[f64]) -> f64 {
    let iqr = percentile(sorted, 0.75) - percentile(sorted, 0.25);
    2.0 * iqr * (sorted.len() as f64).powf(-1.0 / 3.0)
}

/// The quantile `q` of a sorted, non-empty sample, interpolated linearly as by
/// `numpy.percentile` (including its choice of formula for the interpolation).
fn percentile(sorted: &[f64], q: f64) -> f64 {
    let position = q * (sorted.len() - 1) as f64;
    let below = position.floor();
    let t = position - below;
    let a = sorted[below as usize];
    let b = sorted[(below as usize + 1).min(sorted.len() - 1)];
    let difference = b - a;
    if t >= 0.5 {
        b - difference * (1.0 - t)
    } else {
        a + difference * t
    }
}

/// The interval of values that a dimension of a histogram includes: the explicit range
/// (widened when its width is zero) and the extent of the edges.
struct Bounds {
    lo: f64,
    hi: f64,
}

impl Bounds {
    fn new(edges: &[f64], range: Option<(f64, f64)>) -> Self {
        let (mut lo, mut hi) = match (edges.first(), edges.last()) {
            (Some(&first), Some(&last)) => (first, last),
            _ => (f64::INFINITY, f64::NEG_INFINITY),
        };
        if let Some(range) = range {
            let (range_lo, range_hi) = outer_edges(&[], Some(range));
            lo = lo.max(range_lo);
            hi = hi.min(range_hi);
        }
        Self { lo, hi }
    }

    /// Returns the bin that contains a finite value, or `None` when the value is
    /// excluded. Bins are half open except the last, which is closed.
    fn index(&self, edges: &[f64], value: f64) -> Option<usize> {
        if !(self.lo..=self.hi).contains(&value) {
            return None;
        }
        let bins = edges.len() - 1;
        Some(
            edges
                .partition_point(|&e| e <= value)
                .saturating_sub(1)
                .min(bins - 1),
        )
    }
}

/// Normalises the counts of the bins of one dimension, whose widths are given, by `total`,
/// which is the total of the included samples unless several histograms are normalised by
/// the total they pool, as the members of a stack are.
pub(crate) fn normalise_1d(counts: &[f64], widths: &[f64], total: f64, norm: HistNorm) -> Vec<f64> {
    normalise(counts, widths, total, norm, cumulative_1d)
}

/// Normalises the counts of the bins, whose widths (or areas) are given, by the total of
/// the included samples. The function `accumulate` turns counts into cumulative counts.
fn normalise(
    counts: &[f64],
    widths: &[f64],
    total: f64,
    norm: HistNorm,
    accumulate: impl Fn(Vec<f64>) -> Vec<f64>,
) -> Vec<f64> {
    let of_total = |c: f64| if total == 0.0 { 0.0 } else { c / total };
    match norm {
        HistNorm::Count => counts.to_vec(),
        HistNorm::Probability => counts.iter().map(|&c| of_total(c)).collect(),
        HistNorm::Percentage => counts.iter().map(|&c| 100.0 * of_total(c)).collect(),
        HistNorm::CountDensity => counts.iter().zip(widths).map(|(c, w)| c / w).collect(),
        HistNorm::Density => counts
            .iter()
            .zip(widths)
            .map(|(&c, w)| of_total(c) / w)
            .collect(),
        HistNorm::CumulativeCount => accumulate(counts.to_vec()),
        HistNorm::Cdf => accumulate(counts.to_vec())
            .into_iter()
            .map(of_total)
            .collect(),
    }
}

/// Accumulates the counts of a one-dimensional histogram.
fn cumulative_1d(mut counts: Vec<f64>) -> Vec<f64> {
    let mut sum = 0.0;
    for c in &mut counts {
        sum += *c;
        *c = sum;
    }
    counts
}

/// Accumulates row-major counts of `ny` rows and `nx` columns over both dimensions.
fn cumulative_2d(mut counts: Vec<f64>, nx: usize, ny: usize) -> Vec<f64> {
    for row in counts.chunks_mut(nx.max(1)) {
        let accumulated = cumulative_1d(row.to_vec());
        row.copy_from_slice(&accumulated);
    }
    for j in 1..ny {
        for i in 0..nx {
            counts[j * nx + i] += counts[(j - 1) * nx + i];
        }
    }
    counts
}
