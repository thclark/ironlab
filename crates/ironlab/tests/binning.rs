//! The binning functions behind histograms: bin rules, numpy edge semantics, exclusions,
//! weights and normalisations, in one and two dimensions.
//!
//! The expected values are literals computed by numpy 2.3 (or by hand where numpy has no
//! equivalent); the comment above each gives the call that produces it.

use ironlab::binning::{
    BinRule, BinningError, Bins, HistNorm, HistStyle, Histogram, Histogram2d, HistogramSummary,
    bin_edges, histogram, histogram2d,
};

const COUNT_SAMPLE: [f64; 10] = [1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0];
const COUNT_WEIGHTS: [f64; 10] = [0.5, 0.5, 0.5, 0.5, 0.5, 2.0, 2.0, 2.0, 2.0, 2.0];
const RULE_SAMPLE: [f64; 12] = [2.1, -1.3, 0.4, 1.9, 5.1, 6.2, 3.3, 0.0, 2.8, 4.4, 1.1, -0.7];

fn assert_close(actual: &[f64], expected: &[f64]) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "length differs: {actual:?} against {expected:?}"
    );
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        let close = (a.is_nan() && e.is_nan()) || (a - e).abs() <= 1e-12 * e.abs().max(1.0);
        assert!(
            close,
            "element {i} differs: {actual:?} against {expected:?}"
        );
    }
}

fn hist(sample: &[f64], bins: Bins) -> Histogram {
    histogram(sample, None, &bins, None, HistNorm::Count).expect("valid binning")
}

fn hist_norm(sample: &[f64], bins: Bins, norm: HistNorm) -> Histogram {
    histogram(sample, None, &bins, None, norm).expect("valid binning")
}

fn summary(
    total: f64,
    excluded_non_finite: usize,
    excluded_out_of_range: usize,
) -> HistogramSummary {
    HistogramSummary {
        total,
        excluded_non_finite,
        excluded_out_of_range,
    }
}

fn widths(edges: &[f64]) -> Vec<f64> {
    edges.windows(2).map(|w| w[1] - w[0]).collect()
}

// ---------------------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------------------

// WHY: a bare `ax.hist(sample)` must bin like numpy's and matplotlib's defaults would lead
// a reader to expect: automatic bins, raw counts, touching bars.
#[test]
fn defaults_are_automatic_bins_counts_and_bars() {
    assert_eq!(Bins::default(), Bins::Rule(BinRule::Auto));
    assert_eq!(HistNorm::default(), HistNorm::Count);
    assert_eq!(HistStyle::default(), HistStyle::Bar);
}

// WHY: the types are part of the everyday plotting vocabulary, so a glob import of the
// prelude must bring them into scope.
#[test]
fn types_are_in_the_prelude() {
    use ironlab::prelude::*;
    let _ = (
        Bins::Count(3),
        BinRule::Sturges,
        HistNorm::Density,
        HistStyle::Step,
    );
    let _ = HistogramSummary::default();
}

// ---------------------------------------------------------------------------------------
// Bin specifications
// ---------------------------------------------------------------------------------------

// WHY: n equal bins must span exactly [min, max]; the maximum lies on the last edge and
// must be counted, because the last bin is closed (a reader counts every sample).
#[test]
fn count_bins_span_the_data_and_close_the_last_bin() {
    let h = hist(&COUNT_SAMPLE, Bins::Count(4));
    // np.histogram([1,2,2,3,3,3,4,4,4,4], bins=4)
    assert_close(&h.edges, &[1.0, 1.75, 2.5, 3.25, 4.0]);
    assert_close(&h.values, &[1.0, 2.0, 3.0, 4.0]);
    assert_eq!(h.summary, summary(10.0, 0, 0));
}

// WHY: a sample on an interior edge belongs to the bin on its right (bins are half open),
// and explicit edges may have unequal widths.
#[test]
fn explicit_edges_are_used_as_given_with_unequal_widths() {
    let sample = [0.0, 0.5, 1.0, 2.5, 3.0, 5.9, 6.0];
    let h = hist(&sample, Bins::Edges(vec![0.0, 1.0, 3.0, 6.0]));
    // np.histogram([0,0.5,1,2.5,3,5.9,6], bins=[0,1,3,6])
    assert_close(&h.edges, &[0.0, 1.0, 3.0, 6.0]);
    assert_close(&h.values, &[2.0, 2.0, 3.0]);
}

// WHY: samples outside explicit edges are not drawn, and the summary must say so, so
// that the reader is not misled about how many samples the bars represent.
#[test]
fn explicit_edges_exclude_samples_outside_them() {
    let h = hist(&[-1.0, 0.0, 7.0], Bins::Edges(vec![0.0, 1.0, 3.0, 6.0]));
    // np.histogram([-1,0,7], bins=[0,1,3,6])
    assert_close(&h.values, &[1.0, 0.0, 0.0]);
    assert_eq!(h.summary, summary(1.0, 0, 2));
}

// WHY: a bin width starts the bins at the range minimum and adds bins until the maximum
// is covered, so the last edge may lie beyond the data.
#[test]
fn width_bins_start_at_the_minimum_and_cover_the_maximum() {
    let h = hist(&[0.0, 1.0], Bins::Width(0.3));
    // Hand-computed: 0.0 + np.arange(5) * 0.3, the fewest bins of width 0.3 that cover 1.0
    assert_close(&h.edges, &[0.0, 0.3, 0.6, 0.8999999999999999, 1.2]);
    assert_close(&h.values, &[1.0, 0.0, 0.0, 1.0]);
}

// WHY: when the width divides the range, the last edge is the maximum and no empty bin is
// appended beyond the data; the maximum falls in the closed last bin.
#[test]
fn width_bins_end_at_the_maximum_when_the_width_divides_the_range() {
    let h = hist(&[0.0, 0.5, 1.0], Bins::Width(0.25));
    // np.histogram([0,0.5,1], bins=np.arange(0, 1.25, 0.25))
    assert_close(&h.edges, &[0.0, 0.25, 0.5, 0.75, 1.0]);
    assert_close(&h.values, &[1.0, 0.0, 1.0, 1.0]);
}

// WHY: bin_edges is the numpy.histogram_bin_edges equivalent, and must agree with the
// edges that histogram uses.
#[test]
fn bin_edges_agree_with_histogram() {
    let bins = Bins::Rule(BinRule::Sqrt);
    let edges = bin_edges(&RULE_SAMPLE, &bins, None).expect("valid binning");
    assert_eq!(edges, hist(&RULE_SAMPLE, bins).edges);
}

// ---------------------------------------------------------------------------------------
// Bin rules
// ---------------------------------------------------------------------------------------

// WHY: a scientist who switches between numpy and IronLAB must get the same bins from the
// same rule name.
#[test]
fn rules_match_numpy_on_a_small_sample() {
    let edges = |rule| hist(&RULE_SAMPLE, Bins::Rule(rule)).edges;
    // np.histogram_bin_edges(RULE_SAMPLE, bins='sturges') (and bins='auto', which agrees)
    let sturges = [-1.3, 0.19999999999999996, 1.7, 3.2, 4.7, 6.2];
    assert_close(&edges(BinRule::Sturges), &sturges);
    assert_close(&edges(BinRule::Auto), &sturges);
    // np.histogram_bin_edges(RULE_SAMPLE, bins='scott')
    assert_close(&edges(BinRule::Scott), &[-1.3, 1.2, 3.7, 6.2]);
    // np.histogram_bin_edges(RULE_SAMPLE, bins='fd')
    assert_close(&edges(BinRule::FreedmanDiaconis), &[-1.3, 1.2, 3.7, 6.2]);
    // np.histogram_bin_edges(RULE_SAMPLE, bins='sqrt')
    assert_close(&edges(BinRule::Sqrt), &[-1.3, 0.575, 2.45, 4.325, 6.2]);
}

// WHY: with outliers, Freedman–Diaconis alone gives a great many tiny bins; numpy's
// "auto" limits the width to at least half the square-root width. This is the case a
// reader of a heavy-tailed histogram relies on not exploding into slivers.
#[test]
fn auto_relaxes_freedman_diaconis_when_outliers_stretch_the_range() {
    let mut sample: Vec<f64> = (0..100).map(|i| f64::from(i) / 99.0).collect();
    sample.extend([10.0, -10.0]);
    // len(np.histogram_bin_edges(list(np.arange(100)/99) + [10.0, -10.0], bins='fd')) - 1
    assert_eq!(
        hist(&sample, Bins::Rule(BinRule::FreedmanDiaconis))
            .values
            .len(),
        92
    );
    // np.histogram_bin_edges(list(np.arange(100)/99) + [10.0, -10.0], bins='sturges')
    assert_close(
        &hist(&sample, Bins::Rule(BinRule::Sturges)).edges,
        &[-10.0, -7.5, -5.0, -2.5, 0.0, 2.5, 5.0, 7.5, 10.0],
    );
    // np.histogram_bin_edges(list(np.arange(100)/99) + [10.0, -10.0], bins='auto')
    let auto = hist(&sample, Bins::Rule(BinRule::Auto)).edges;
    assert_eq!(auto.len(), 22);
    assert_close(&auto[..3], &[-10.0, -9.047619047619047, -8.095238095238095]);
    assert_close(&auto[20..], &[9.047619047619047, 10.0]);
}

// WHY: for a large sample, "auto" takes the Freedman–Diaconis width when it gives more
// bins than Sturges.
#[test]
fn auto_takes_freedman_diaconis_for_a_large_sample() {
    let sample: Vec<f64> = (0..2000).map(|i| f64::from(i) / 1999.0).collect();
    // len(np.histogram_bin_edges(np.arange(2000)/1999, bins=...)) - 1
    assert_eq!(
        hist(&sample, Bins::Rule(BinRule::FreedmanDiaconis))
            .values
            .len(),
        13
    );
    assert_eq!(hist(&sample, Bins::Rule(BinRule::Sturges)).values.len(), 12);
    let auto = hist(&sample, Bins::Rule(BinRule::Auto));
    assert_eq!(auto.values.len(), 13);
    // np.histogram_bin_edges(np.arange(2000)/1999, bins='auto')[:3]
    assert_close(
        &auto.edges[..3],
        &[0.0, 0.07692307692307693, 0.15384615384615385],
    );
    assert_eq!(auto.values.iter().sum::<f64>(), 2000.0);
}

// WHY: when most samples are equal the interquartile range is zero, so Freedman–Diaconis
// gives a single bin; numpy's "auto" then uses half the square-root width.
#[test]
fn auto_with_a_zero_interquartile_range_uses_half_the_square_root_width() {
    let sample = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 5.0];
    // np.histogram_bin_edges(np.array([0,0,0,0,0,0,0,1,5.]), bins='fd')
    assert_close(
        &hist(&sample, Bins::Rule(BinRule::FreedmanDiaconis)).edges,
        &[0.0, 5.0],
    );
    // np.histogram(np.array([0,0,0,0,0,0,0,1,5.]), bins='auto')
    let auto = hist(&sample, Bins::Rule(BinRule::Auto));
    assert_close(
        &auto.edges,
        &[
            0.0,
            0.8333333333333334,
            1.6666666666666667,
            2.5,
            3.3333333333333335,
            4.166666666666667,
            5.0,
        ],
    );
    assert_close(&auto.values, &[7.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
}

// WHY: integer-valued data (counts, dice, categories) needs one bin per integer, centred
// on it, so that no bin straddles two integers (MATLAB's 'integers' rule).
#[test]
fn integers_rule_centres_unit_bins_on_the_integers() {
    let h = hist(&[0.6, 1.0, 2.0, 2.0, 4.4], Bins::Rule(BinRule::Integers));
    // np.histogram([0.6,1,2,2,4.4], bins=np.arange(0.5, 5.5))
    assert_close(&h.edges, &[0.5, 1.5, 2.5, 3.5, 4.5]);
    assert_close(&h.values, &[2.0, 2.0, 0.0, 1.0]);
}

// WHY: a half-integer belongs to the bin on its right (bins are half open), so the
// integers rule must extend the bins to include it rather than drop it or count it twice.
#[test]
fn integers_rule_places_half_integers_in_the_bin_to_their_right() {
    let h = hist(&[-0.5, 4.5], Bins::Rule(BinRule::Integers));
    // np.histogram([-0.5, 4.5], bins=np.arange(-0.5, 6.5))
    assert_close(&h.edges, &[-0.5, 0.5, 1.5, 2.5, 3.5, 4.5, 5.5]);
    assert_close(&h.values, &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
}

// ---------------------------------------------------------------------------------------
// Degenerate samples
// ---------------------------------------------------------------------------------------

// WHY: a single sample has a zero-width range, which numpy widens to [v - 0.5, v + 0.5]
// so that the bins have a finite width.
#[test]
fn a_single_sample_is_binned_in_a_unit_range_around_it() {
    let h = hist(&[3.0], Bins::Count(3));
    // np.histogram([3.0], bins=3)
    assert_close(
        &h.edges,
        &[2.5, 2.8333333333333335, 3.1666666666666665, 3.5],
    );
    assert_close(&h.values, &[0.0, 1.0, 0.0]);
    // np.histogram([3.0], bins='auto')
    let auto = hist(&[3.0], Bins::Rule(BinRule::Auto));
    assert_close(&auto.edges, &[2.5, 3.5]);
    assert_close(&auto.values, &[1.0]);
}

// WHY: every rule gives zero width for equal samples; that must become one bin holding
// every sample, not a division by zero.
#[test]
fn equal_samples_fall_in_one_bin_for_every_rule() {
    for rule in [
        BinRule::Auto,
        BinRule::Sturges,
        BinRule::Scott,
        BinRule::FreedmanDiaconis,
        BinRule::Sqrt,
    ] {
        let h = hist(&[2.0, 2.0, 2.0], Bins::Rule(rule));
        // np.histogram([2.0,2,2], bins='auto')
        assert_close(&h.edges, &[1.5, 2.5]);
        assert_close(&h.values, &[3.0]);
    }
}

// WHY: an empty sample is valid (ADR 0012): a rule has nothing to estimate from, so it
// gives no bins, and the histogram draws nothing rather than failing.
#[test]
fn an_empty_sample_gives_no_bins_for_a_rule() {
    for rule in [
        BinRule::Auto,
        BinRule::Sturges,
        BinRule::Scott,
        BinRule::FreedmanDiaconis,
        BinRule::Sqrt,
        BinRule::Integers,
    ] {
        let h = hist(&[], Bins::Rule(rule));
        assert!(h.edges.is_empty() && h.values.is_empty(), "{rule:?}");
        assert_eq!(h.summary, summary(0.0, 0, 0));
    }
}

// WHY: an empty sample with a bin count still has bins (numpy uses the range [0, 1]),
// and normalising its zero counts must give zeros, not NaN.
#[test]
fn an_empty_sample_with_a_count_gives_empty_bins_on_the_unit_range() {
    let h = hist_norm(&[], Bins::Count(3), HistNorm::Probability);
    // np.histogram([], bins=3)
    assert_close(
        &h.edges,
        &[0.0, 0.3333333333333333, 0.6666666666666666, 1.0],
    );
    assert_close(&h.values, &[0.0, 0.0, 0.0]);
    let cdf = hist_norm(&[], Bins::Count(3), HistNorm::Cdf);
    assert_close(&cdf.values, &[0.0, 0.0, 0.0]);
}

// WHY: NaN and infinite samples cannot be placed in a bin; they must neither stretch the
// range nor be counted, and the summary must report how many were dropped.
#[test]
fn non_finite_samples_are_excluded_and_reported() {
    let sample = [
        1.0,
        2.0,
        f64::NAN,
        3.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        4.0,
    ];
    let h = hist(&sample, Bins::Count(3));
    // np.histogram([1,2,3,4], bins=3)
    assert_close(&h.edges, &[1.0, 2.0, 3.0, 4.0]);
    assert_close(&h.values, &[1.0, 1.0, 2.0]);
    assert_eq!(h.summary, summary(4.0, 3, 0));
}

// WHY: a sample that is entirely non-finite is a sample with nothing to draw, not an
// error.
#[test]
fn an_entirely_non_finite_sample_gives_no_bins() {
    let h = hist(&[f64::NAN, f64::INFINITY], Bins::Rule(BinRule::Auto));
    assert!(h.edges.is_empty() && h.values.is_empty());
    assert_eq!(h.summary, summary(0.0, 2, 0));
}

// ---------------------------------------------------------------------------------------
// Explicit range
// ---------------------------------------------------------------------------------------

// WHY: an explicit range fixes the bins regardless of the data, and samples outside it
// are excluded and reported (numpy's `range`).
#[test]
fn an_explicit_range_sets_the_bins_and_excludes_samples_outside_it() {
    let h = histogram(
        &[-1.0, 0.0, 0.5, 1.0, 2.0],
        None,
        &Bins::Count(2),
        Some((0.0, 1.0)),
        HistNorm::Count,
    )
    .unwrap();
    // np.histogram([-1,0,0.5,1,2], bins=2, range=(0,1))
    assert_close(&h.edges, &[0.0, 0.5, 1.0]);
    assert_close(&h.values, &[1.0, 2.0]);
    assert_eq!(h.summary, summary(3.0, 0, 2));
}

// WHY: numpy estimates a rule's width from the samples inside the range only, and divides
// the range (not the data's extent) into bins.
#[test]
fn a_rule_with_an_explicit_range_estimates_from_the_samples_inside_it() {
    let h = histogram(
        &RULE_SAMPLE,
        None,
        &Bins::Rule(BinRule::Sturges),
        Some((0.0, 4.0)),
        HistNorm::Count,
    )
    .unwrap();
    // np.histogram(RULE_SAMPLE, bins='sturges', range=(0, 4))
    assert_close(&h.edges, &[0.0, 0.8, 1.6, 2.4000000000000004, 3.2, 4.0]);
    assert_close(&h.values, &[2.0, 1.0, 2.0, 1.0, 1.0]);
    assert_eq!(h.summary, summary(7.0, 0, 5));
}

// WHY: a zero-width explicit range is widened in the same way as a zero-width data range.
#[test]
fn a_zero_width_explicit_range_is_widened() {
    let h = histogram(
        &[1.0, 2.0, 3.0],
        None,
        &Bins::Count(2),
        Some((2.0, 2.0)),
        HistNorm::Count,
    )
    .unwrap();
    // np.histogram([1,2,3], bins=2, range=(2,2))
    assert_close(&h.edges, &[1.5, 2.0, 2.5]);
    assert_close(&h.values, &[0.0, 1.0]);
    assert_eq!(h.summary, summary(1.0, 0, 2));
}

// WHY: the last width bin may extend beyond an explicit range, but a sample beyond the
// range is still outside what the author asked for and must not be counted.
#[test]
fn width_bins_exclude_samples_beyond_an_explicit_range() {
    let h = histogram(
        &[0.0, 0.95, 1.1],
        None,
        &Bins::Width(0.3),
        Some((0.0, 1.0)),
        HistNorm::Count,
    )
    .unwrap();
    // Hand-computed: 0.0 + np.arange(5) * 0.3; 1.1 lies outside range=(0, 1)
    assert_close(&h.edges, &[0.0, 0.3, 0.6, 0.8999999999999999, 1.2]);
    assert_close(&h.values, &[1.0, 0.0, 0.0, 1.0]);
    assert_eq!(h.summary, summary(2.0, 0, 1));
}

// ---------------------------------------------------------------------------------------
// Weights
// ---------------------------------------------------------------------------------------

// WHY: with weights, each bin holds the sum of its samples' weights, and the total used
// for normalisation is the sum of the included weights.
#[test]
fn weights_replace_counts_by_sums() {
    let h = histogram(
        &COUNT_SAMPLE,
        Some(&COUNT_WEIGHTS),
        &Bins::Count(4),
        None,
        HistNorm::Count,
    )
    .unwrap();
    // np.histogram([1,2,2,3,3,3,4,4,4,4], bins=4, weights=[0.5]*5+[2]*5)
    assert_close(&h.values, &[0.5, 1.0, 3.0, 8.0]);
    assert_eq!(h.summary, summary(12.5, 0, 0));
}

// WHY: weighted density is what a scientist uses to plot a weighted PDF; it must match
// numpy's weighted density.
#[test]
fn weighted_density_matches_numpy() {
    let h = histogram(
        &COUNT_SAMPLE,
        Some(&COUNT_WEIGHTS),
        &Bins::Count(4),
        None,
        HistNorm::Density,
    )
    .unwrap();
    // np.histogram([1,2,2,3,3,3,4,4,4,4], bins=4, weights=[0.5]*5+[2]*5, density=True)
    assert_close(
        &h.values,
        &[
            0.05333333333333333,
            0.10666666666666666,
            0.32,
            0.8533333333333333,
        ],
    );
}

// WHY: a sample with a non-finite weight cannot contribute a finite amount to a bin; it
// is excluded like a non-finite sample, and does not stretch the range.
#[test]
fn a_sample_with_a_non_finite_weight_is_excluded() {
    let h = histogram(
        &[1.0, 2.0, 3.0],
        Some(&[1.0, f64::NAN, 1.0]),
        &Bins::Count(2),
        None,
        HistNorm::Count,
    )
    .unwrap();
    // np.histogram([1,3], bins=2)
    assert_close(&h.edges, &[1.0, 2.0, 3.0]);
    assert_close(&h.values, &[1.0, 1.0]);
    assert_eq!(h.summary, summary(2.0, 1, 0));
}

// WHY: weights do not change where a rule puts the edges (numpy refuses weighted rules;
// IronLAB estimates from the unweighted sample).
#[test]
fn a_rule_estimates_bins_from_the_unweighted_sample() {
    let unweighted = hist(&RULE_SAMPLE, Bins::Rule(BinRule::Sqrt));
    let weighted = histogram(
        &RULE_SAMPLE,
        Some(&[3.0; 12]),
        &Bins::Rule(BinRule::Sqrt),
        None,
        HistNorm::Count,
    )
    .unwrap();
    assert_eq!(weighted.edges, unweighted.edges);
    let tripled: Vec<f64> = unweighted.values.iter().map(|v| 3.0 * v).collect();
    assert_close(&weighted.values, &tripled);
}

// ---------------------------------------------------------------------------------------
// Normalisations
// ---------------------------------------------------------------------------------------

// WHY: each normalisation is a different reading of the same counts; these are the
// values a reader of each kind of histogram expects for counts [1, 2, 3, 4] in bins of
// width 0.75.
#[test]
fn each_normalisation_matches_its_definition() {
    let values = |norm| hist_norm(&COUNT_SAMPLE, Bins::Count(4), norm).values;
    assert_close(&values(HistNorm::Count), &[1.0, 2.0, 3.0, 4.0]);
    // h / h.sum(), where h = np.histogram([1,2,2,3,3,3,4,4,4,4], bins=4)[0]
    assert_close(&values(HistNorm::Probability), &[0.1, 0.2, 0.3, 0.4]);
    // 100 * h / h.sum()
    assert_close(&values(HistNorm::Percentage), &[10.0, 20.0, 30.0, 40.0]);
    // h / np.diff(edges)
    assert_close(
        &values(HistNorm::CountDensity),
        &[
            1.3333333333333333,
            2.6666666666666665,
            4.0,
            5.333333333333333,
        ],
    );
    // np.histogram([1,2,2,3,3,3,4,4,4,4], bins=4, density=True)[0]
    assert_close(
        &values(HistNorm::Density),
        &[
            0.13333333333333333,
            0.26666666666666666,
            0.4,
            0.5333333333333333,
        ],
    );
    // np.cumsum(h)
    assert_close(&values(HistNorm::CumulativeCount), &[1.0, 3.0, 6.0, 10.0]);
    // np.cumsum(h) / h.sum()
    assert_close(&values(HistNorm::Cdf), &[0.1, 0.3, 0.6, 1.0]);
}

// WHY: a density histogram is overlaid on a probability density function, so its area
// must be one, also with unequal widths, weights and excluded samples.
#[test]
fn density_integrates_to_one() {
    let cases = [
        histogram(
            &[0.0, 0.5, 1.0, 2.5, 3.0, 5.9, 6.0, 9.0, f64::NAN],
            None,
            &Bins::Edges(vec![0.0, 1.0, 3.0, 6.0]),
            None,
            HistNorm::Density,
        ),
        histogram(
            &COUNT_SAMPLE,
            Some(&COUNT_WEIGHTS),
            &Bins::Rule(BinRule::Sturges),
            None,
            HistNorm::Density,
        ),
    ];
    for h in cases {
        let h = h.unwrap();
        let area: f64 = h
            .values
            .iter()
            .zip(widths(&h.edges))
            .map(|(v, w)| v * w)
            .sum();
        assert!((area - 1.0).abs() < 1e-12, "area {area}");
    }
    // np.histogram([0,0.5,1,2.5,3,5.9,6], bins=[0,1,3,6], density=True)[0]
    let h = histogram(
        &[0.0, 0.5, 1.0, 2.5, 3.0, 5.9, 6.0],
        None,
        &Bins::Edges(vec![0.0, 1.0, 3.0, 6.0]),
        None,
        HistNorm::Density,
    )
    .unwrap();
    assert_close(
        &h.values,
        &[0.2857142857142857, 0.14285714285714285, 0.14285714285714285],
    );
}

// WHY: the total is the included samples, so a percentage histogram sums to 100 and a
// CDF ends at 1 even when samples are excluded; the excluded ones are in the summary.
#[test]
fn percentage_sums_to_100_and_cdf_ends_at_1_over_the_included_samples() {
    let sample = [-5.0, 0.1, 0.2, 0.4, 0.9, f64::NAN, 7.0];
    let run = |norm| histogram(&sample, None, &Bins::Count(3), Some((0.0, 1.0)), norm).unwrap();
    let percentage = run(HistNorm::Percentage);
    assert!((percentage.values.iter().sum::<f64>() - 100.0).abs() < 1e-12);
    assert_eq!(percentage.summary, summary(4.0, 1, 2));
    let cdf = run(HistNorm::Cdf);
    assert_eq!(cdf.values.last().copied(), Some(1.0));
    assert!(cdf.values.windows(2).all(|w| w[0] <= w[1]));
}

// ---------------------------------------------------------------------------------------
// Invalid specifications
// ---------------------------------------------------------------------------------------

// WHY: the facade never panics on bad input; it needs a typed error for each
// specification that cannot produce bins, so that it can report the problem.
#[test]
fn invalid_specifications_are_errors() {
    let run = |bins: Bins, range| histogram(&[1.0, 2.0], None, &bins, range, HistNorm::Count);
    assert_eq!(
        run(Bins::Count(0), None).unwrap_err(),
        BinningError::ZeroBins
    );
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            matches!(
                run(Bins::Width(width), None).unwrap_err(),
                BinningError::InvalidWidth(_)
            ),
            "{width}"
        );
    }
    assert_eq!(
        run(Bins::Edges(vec![1.0]), None).unwrap_err(),
        BinningError::InvalidEdges
    );
    assert_eq!(
        run(Bins::Edges(vec![0.0, 2.0, 1.0]), None).unwrap_err(),
        BinningError::InvalidEdges
    );
    assert_eq!(
        run(Bins::Edges(vec![0.0, 1.0, 1.0]), None).unwrap_err(),
        BinningError::InvalidEdges
    );
    assert_eq!(
        run(Bins::Edges(vec![0.0, f64::NAN]), None).unwrap_err(),
        BinningError::InvalidEdges
    );
    assert!(matches!(
        run(Bins::Count(2), Some((1.0, 0.0))).unwrap_err(),
        BinningError::InvalidRange(..)
    ));
    assert!(matches!(
        run(Bins::Count(2), Some((0.0, f64::INFINITY))).unwrap_err(),
        BinningError::InvalidRange(..)
    ));
    assert_eq!(
        histogram(
            &[1.0, 2.0],
            Some(&[1.0]),
            &Bins::Count(2),
            None,
            HistNorm::Count
        )
        .unwrap_err(),
        BinningError::LengthMismatch {
            expected: 2,
            actual: 1
        }
    );
}

// WHY: numpy refuses more bins than the floating-point range can separate; silently
// producing bins of zero width would give infinite densities.
#[test]
fn too_many_bins_for_the_range_is_an_error() {
    let err = histogram(
        &[1.0],
        None,
        &Bins::Count(10),
        Some((1.0, 1.0 + f64::EPSILON)),
        HistNorm::Count,
    )
    .unwrap_err();
    assert_eq!(err, BinningError::TooManyBins(10));
}

// ---------------------------------------------------------------------------------------
// Two dimensions
// ---------------------------------------------------------------------------------------

const X2: [f64; 8] = [0.0, 1.0, 2.0, 3.0, 0.5, 2.5, 3.0, 1.5];
const Y2: [f64; 8] = [0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 2.0, 0.5];
const W2: [f64; 8] = [1.0, 2.0, 1.0, 1.0, 1.0, 1.0, 3.0, 1.0];

fn hist2(bins: (Bins, Bins), norm: HistNorm, min_count: Option<f64>) -> Histogram2d {
    histogram2d(
        &X2,
        &Y2,
        None,
        (&bins.0, &bins.1),
        (None, None),
        norm,
        min_count,
    )
    .expect("valid binning")
}

// WHY: the values are drawn as cells, row j for y bin j and column i for x bin i (as a
// Matrix sampled on a grid); a transposed result would draw the histogram mirrored.
#[test]
fn counts_are_rows_of_y_bins_and_columns_of_x_bins() {
    let h = hist2((Bins::Count(2), Bins::Count(3)), HistNorm::Count, None);
    // H, xe, ye = np.histogram2d(X2, Y2, bins=[2,3]); H.T, xe, ye
    assert_close(&h.x_edges, &[0.0, 1.5, 3.0]);
    assert_close(
        &h.y_edges,
        &[0.0, 0.6666666666666666, 1.3333333333333333, 2.0],
    );
    assert_eq!((h.values.rows(), h.values.cols()), (3, 2));
    assert_close(h.values.values(), &[2.0, 1.0, 0.0, 2.0, 1.0, 2.0]);
    assert_eq!(h.summary, summary(8.0, 0, 0));
}

// WHY: weighted 2D histograms sum weights per cell, as numpy's do.
#[test]
fn two_dimensional_weights_replace_counts_by_sums() {
    let h = histogram2d(
        &X2,
        &Y2,
        Some(&W2),
        (&Bins::Count(2), &Bins::Count(3)),
        (None, None),
        HistNorm::Count,
        None,
    )
    .unwrap();
    // np.histogram2d(X2, Y2, bins=[2,3], weights=W2)[0].T
    assert_close(h.values.values(), &[3.0, 1.0, 0.0, 2.0, 1.0, 4.0]);
    assert_eq!(h.summary.total, 11.0);
}

// WHY: a 2D density divides by the bin area, so that its volume is one.
#[test]
fn two_dimensional_density_divides_by_the_bin_area() {
    let h = hist2((Bins::Count(2), Bins::Count(3)), HistNorm::Density, None);
    // np.histogram2d(X2, Y2, bins=[2,3], density=True)[0].T
    assert_close(
        h.values.values(),
        &[
            0.25,
            0.125,
            0.0,
            0.25,
            0.12499999999999999,
            0.24999999999999997,
        ],
    );
    let wx = widths(&h.x_edges);
    let wy = widths(&h.y_edges);
    let volume: f64 = (0..h.values.rows())
        .flat_map(|j| (0..h.values.cols()).map(move |i| (j, i)))
        .map(|(j, i)| h.values[(j, i)] * wx[i] * wy[j])
        .sum();
    assert!((volume - 1.0).abs() < 1e-12, "volume {volume}");
}

// WHY: explicit edges per dimension are used as given.
#[test]
fn two_dimensional_explicit_edges_are_used_as_given() {
    let h = hist2(
        (
            Bins::Edges(vec![0.0, 1.0, 3.0]),
            Bins::Edges(vec![0.0, 1.0, 2.0]),
        ),
        HistNorm::Count,
        None,
    );
    // np.histogram2d(X2, Y2, bins=[[0,1,3],[0,1,2]])[0].T
    assert_close(h.values.values(), &[1.0, 2.0, 1.0, 4.0]);
}

// WHY: a pair outside either range is outside the histogram, and is reported once.
#[test]
fn two_dimensional_ranges_exclude_pairs_outside_either() {
    let h = histogram2d(
        &X2,
        &Y2,
        None,
        (&Bins::Count(2), &Bins::Count(2)),
        (Some((0.0, 2.0)), Some((0.0, 1.0))),
        HistNorm::Count,
        None,
    )
    .unwrap();
    // np.histogram2d(X2, Y2, bins=[2,2], range=[[0,2],[0,1]])
    assert_close(&h.x_edges, &[0.0, 1.0, 2.0]);
    assert_close(&h.y_edges, &[0.0, 0.5, 1.0]);
    assert_close(h.values.values(), &[1.0, 1.0, 0.0, 2.0]);
    assert_eq!(h.summary, summary(4.0, 0, 4));
}

// WHY: a pair with a non-finite coordinate (or weight) cannot be placed, and must not
// stretch the range of the other coordinate either.
#[test]
fn two_dimensional_non_finite_pairs_are_excluded() {
    let mut x = X2.to_vec();
    let mut y = Y2.to_vec();
    x.extend([f64::NAN, 100.0]);
    y.extend([100.0, f64::INFINITY]);
    let h = histogram2d(
        &x,
        &y,
        None,
        (&Bins::Count(2), &Bins::Count(3)),
        (None, None),
        HistNorm::Count,
        None,
    )
    .unwrap();
    let finite = hist2((Bins::Count(2), Bins::Count(3)), HistNorm::Count, None);
    assert_eq!(h.x_edges, finite.x_edges);
    assert_eq!(h.y_edges, finite.y_edges);
    assert_eq!(h.values, finite.values);
    assert_eq!(h.summary, summary(8.0, 2, 0));
}

// WHY: a 2D histogram hides sparsely populated cells with min_count (matplotlib's cmin);
// the threshold is on the raw count, so it means "fewer than c samples" whatever the
// normalisation.
#[test]
fn min_count_makes_sparse_cells_not_a_number() {
    let counts = hist2((Bins::Count(2), Bins::Count(3)), HistNorm::Count, Some(2.0));
    let nan = f64::NAN;
    assert_close(counts.values.values(), &[2.0, nan, nan, 2.0, nan, 2.0]);
    let density = hist2(
        (Bins::Count(2), Bins::Count(3)),
        HistNorm::Density,
        Some(2.0),
    );
    assert_close(
        density.values.values(),
        &[0.25, nan, nan, 0.25, nan, 0.24999999999999997],
    );
}

// WHY: the cumulative 2D normalisations accumulate over both dimensions (MATLAB's
// histogram2 'cumcount' and 'cdf'), so that the last cell holds everything.
#[test]
fn two_dimensional_cumulative_normalisations_accumulate_over_both_dimensions() {
    let cum = hist2(
        (Bins::Count(2), Bins::Count(3)),
        HistNorm::CumulativeCount,
        None,
    );
    // np.histogram2d(X2, Y2, bins=[2,3])[0].T.cumsum(0).cumsum(1)
    assert_close(cum.values.values(), &[2.0, 3.0, 2.0, 5.0, 3.0, 8.0]);
    let cdf = hist2((Bins::Count(2), Bins::Count(3)), HistNorm::Cdf, None);
    assert_close(cdf.values.values(), &[0.25, 0.375, 0.25, 0.625, 0.375, 1.0]);
    let probability = hist2(
        (Bins::Count(2), Bins::Count(3)),
        HistNorm::Probability,
        None,
    );
    assert!((probability.values.values().iter().sum::<f64>() - 1.0).abs() < 1e-12);
}

// WHY: an empty 2D sample with rules has nothing to draw, which is valid.
#[test]
fn an_empty_two_dimensional_sample_gives_no_cells_for_rules() {
    let h = histogram2d(
        &[],
        &[],
        None,
        (&Bins::default(), &Bins::default()),
        (None, None),
        HistNorm::Count,
        None,
    )
    .unwrap();
    assert!(h.x_edges.is_empty() && h.y_edges.is_empty());
    assert_eq!((h.values.rows(), h.values.cols()), (0, 0));
}

// WHY: x and y of different lengths cannot be paired.
#[test]
fn two_dimensional_samples_of_different_lengths_are_an_error() {
    let err = histogram2d(
        &[1.0, 2.0],
        &[1.0],
        None,
        (&Bins::Count(2), &Bins::Count(2)),
        (None, None),
        HistNorm::Count,
        None,
    )
    .unwrap_err();
    assert_eq!(
        err,
        BinningError::LengthMismatch {
            expected: 2,
            actual: 1
        }
    );
}
