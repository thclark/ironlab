use ironlab_scene::maths::ticks::{
    Ticks, common_exponent, format_linear, format_log, linear_ticks, log_ticks, nice_limits,
    nice_log_limits,
};
use proptest::prelude::*;

use crate::{assert_close, is_nice_step};

const MINUS: char = '\u{2212}';

/// Smallest number of decimals that represents every multiple of `step` exactly.
fn decimals_for(step: f64) -> usize {
    (0..=15)
        .find(|&d| {
            let scaled = step * 10f64.powi(d as i32);
            (scaled - scaled.round()).abs() <= 1e-9 * scaled.abs().max(1.0)
        })
        .unwrap_or(15)
}

/// The nice step immediately below `step` in the 1-2-5 sequence.
fn next_smaller_nice_step(step: f64) -> f64 {
    let exponent = step.log10().floor();
    let mantissa = (step / 10f64.powf(exponent)).round();
    if mantissa == 5.0 {
        step * 0.4
    } else {
        step * 0.5
    }
}

fn is_positive_zero_or_nonzero(v: f64) -> bool {
    v != 0.0 || v.to_bits() == 0
}

fn exponent_of_decade(v: f64) -> Option<i32> {
    let n = v.log10().round();
    ((v.log10() - n).abs() < 1e-9).then_some(n as i32)
}

// Why: [0, 1] is the most common axis; its ticks must be the familiar 0, 0.2, …, 1 and every
// value must be the exact decimal, or labels and grid lines drift from the numbers they name.
#[test]
fn unit_range_uses_step_of_one_fifth_with_exact_values() {
    let ticks = linear_ticks(0.0, 1.0, 5);
    assert_eq!(ticks.step, 0.2);
    assert_eq!(ticks.major, vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
}

// Why: naive accumulation gives 0.30000000000000004; ticks must be the nearest f64 to 0.3 so
// that labels, gridlines and user-supplied limits compare equal.
#[test]
fn tenths_are_exact_decimals() {
    let ticks = linear_ticks(0.0, 1.0, 10);
    assert_eq!(ticks.step, 0.1);
    assert_eq!(ticks.major[3], 0.3);
    for major in &ticks.major {
        assert!(
            format!("{major}").len() <= 3,
            "{major} is not a short decimal"
        );
    }
}

// Why: minor ticks must subdivide the major interval evenly and never overdraw a major tick,
// otherwise the axis shows doubled or uneven tick marks.
#[test]
fn minor_ticks_subdivide_major_intervals_without_touching_majors() {
    let ticks = linear_ticks(0.0, 1.0, 5);
    assert_eq!(ticks.minor.len(), 15, "a 0.2 step splits into quarters");
    for minor in &ticks.minor {
        assert!((0.0..=1.0).contains(minor));
        assert!(ticks.major.iter().all(|m| (m - minor).abs() > 1e-12));
        let quarters = minor / 0.05;
        assert_close(quarters, quarters.round(), 1e-9);
    }
}

// Why: an arbitrary data range must still produce nice ticks that cover the range without
// skipping a multiple of the step at either end.
#[test]
fn offset_range_is_covered_by_nice_multiples() {
    let coarse = linear_ticks(-3.7, 12.2, 5);
    assert_eq!(coarse.step, 5.0);
    assert_eq!(coarse.major, vec![0.0, 5.0, 10.0]);

    let fine = linear_ticks(-3.7, 12.2, 10);
    assert_eq!(fine.step, 2.0);
    assert_eq!(fine.major, vec![-2.0, 0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0]);
}

// Why: a span that is an exact multiple of a nice step can come out a hair larger in f64
// (0.07 / 0.01 = 7.000000000000001); that rounding must not force a coarser step, or the axis
// loses half its labels for no visible reason.
#[test]
fn exact_multiple_span_is_not_pushed_to_a_coarser_step() {
    let ticks = linear_ticks(0.0, 0.07, 7);
    assert_eq!(ticks.step, 0.01);
    assert_eq!(
        ticks.major,
        vec![0.0, 0.01, 0.02, 0.03, 0.04, 0.05, 0.06, 0.07]
    );
}

// Why: computed data often cross zero by rounding residue only (sin(π) ≈ 1.2e−16). A bound that
// is a tick up to rounding must neither lose its tick nor push the automatic limits out by a whole
// step (MATLAB shows [0, 1] for such data, not [−0.2, 1]), and the resulting zero must be
// positive so it is never labelled "−0".
#[test]
fn rounding_residue_across_zero_snaps_to_ticks() {
    let ticks = linear_ticks(-1e-16, 1.0, 5);
    assert_eq!(ticks.major, vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
    assert!(is_positive_zero_or_nonzero(ticks.major[0]));

    let (lo, hi) = nice_limits(-1e-16, 1.0, 5);
    assert_eq!((lo, hi), (0.0, 1.0));
    assert!(is_positive_zero_or_nonzero(lo));

    assert_eq!(nice_limits(-0.1 - 0.2, 0.3, 6), (-0.3, 0.3));
    assert_eq!(nice_limits(1e-17, 0.1 + 0.2, 3), (0.0, 0.3));
}

// Why: a symmetric range crosses zero; a "−0" label is a typographic error that users notice.
#[test]
fn zero_tick_is_positive_zero() {
    let ticks = linear_ticks(-0.3, 0.3, 6);
    assert_eq!(ticks.major, vec![-0.3, -0.2, -0.1, 0.0, 0.1, 0.2, 0.3]);
    assert!(ticks.major.iter().all(|v| is_positive_zero_or_nonzero(*v)));
    assert_eq!(format_linear(ticks.major[3], ticks.step), "0.0");
}

// Why: a constant data series (a flat line) must still get a usable axis rather than a
// division by zero or an empty tick list.
#[test]
fn degenerate_range_produces_ticks_around_the_value() {
    let ticks = linear_ticks(5.0, 5.0, 5);
    assert!(ticks.major.len() >= 2);
    assert!(ticks.major.iter().all(|v| v.is_finite()));
    assert!(ticks.major[0] < 5.0 && *ticks.major.last().unwrap() > 5.0);
}

// Why: callers derive ranges from user limits that may be given high-to-low; the ticks must not
// depend on argument order.
#[test]
fn reversed_range_is_treated_as_ascending() {
    assert_eq!(linear_ticks(1.0, 0.0, 5), linear_ticks(0.0, 1.0, 5));
    assert_eq!(nice_limits(12.2, -3.7, 5), nice_limits(-3.7, 12.2, 5));
}

// Why: a target of zero is a caller mistake that must not hang or panic.
#[test]
fn target_zero_is_treated_as_one() {
    let ticks = linear_ticks(0.0, 1.0, 0);
    assert_eq!(ticks.major, vec![0.0, 1.0]);
}

// Why: non-finite data (an all-NaN series) must not poison the layout.
#[test]
fn non_finite_bounds_fall_back_to_unit_range() {
    assert_eq!(linear_ticks(f64::NAN, 3.0, 5), linear_ticks(0.0, 1.0, 5));
    assert_eq!(nice_limits(f64::NAN, 3.0, 5), (0.0, 1.0));
    assert_eq!(nice_limits(f64::INFINITY, 2.0, 5), (0.0, 1.0));
}

// Why: physical data span many orders of magnitude; ticks must stay exact and finite and the
// axis must factor out a common power of ten instead of printing long numbers.
#[test]
fn huge_and_tiny_magnitudes_produce_exact_ticks_and_exponents() {
    let tiny = linear_ticks(0.0, 3e-9, 5);
    assert_eq!(tiny.major, vec![0.0, 1e-9, 2e-9, 3e-9]);
    assert_eq!(common_exponent(&tiny.major), -9);

    let huge = linear_ticks(1e12, 5e12, 5);
    assert_eq!(huge.major, vec![1e12, 2e12, 3e12, 4e12, 5e12]);
    assert_eq!(common_exponent(&huge.major), 12);
}

// Why: zooming far in on a large value reaches the limit of f64 resolution; ticks must remain
// strictly increasing (no repeated gridlines) rather than stuttering.
#[test]
fn precision_limited_range_gives_strictly_increasing_ticks() {
    let (lo, hi) = (1e12, 1e12 + 1e-3);
    let ticks = linear_ticks(lo, hi, 5);
    assert!(ticks.major.iter().all(|v| v.is_finite()));
    assert!(ticks.major.windows(2).all(|w| w[0] < w[1]));
    assert!(ticks.major.iter().all(|v| (lo..=hi).contains(v)));
}

// Why: MATLAB-style automatic limits end the axis box on labelled ticks; the examples pin the
// expected outward rounding, including not padding a range that already ends on ticks.
#[test]
fn nice_limits_round_outward_to_ticks() {
    assert_eq!(nice_limits(-3.7, 12.2, 5), (-5.0, 15.0));
    assert_eq!(nice_limits(0.13, 0.87, 5), (0.0, 1.0));
    assert_eq!(nice_limits(0.0, 1.0, 5), (0.0, 1.0));
}

// Why: constant data must yield a non-empty axis; zero cannot be widened proportionally, so it
// has its own documented expansion.
#[test]
fn nice_limits_expand_degenerate_ranges() {
    assert_eq!(nice_limits(0.0, 0.0, 5), (-1.0, 1.0));
    // [4.5, 5.5] rounds out with step 0.2 to [4.4, 5.6], whose span needs step 0.5; rounding the
    // widened range again gives the fixed point [4, 6]. Rounding the original range with 0.5
    // would give [4.5, 5.5], whose own ticks (step 0.2) do not end on the limits.
    assert_eq!(nice_limits(5.0, 5.0, 5), (4.0, 6.0));
    let (lo, hi) = nice_limits(-200.0, -200.0, 5);
    assert!(lo < -200.0 && hi > -200.0);
}

// Why: labels share one precision per axis so columns of numbers align (0.0, 0.5, 1.0), and
// floating-point noise must never appear in a label.
#[test]
fn format_linear_uses_step_precision() {
    assert_eq!(format_linear(0.3, 0.1), "0.3");
    assert_eq!(format_linear(0.1 + 0.2, 0.1), "0.3");
    assert_eq!(format_linear(1.0, 0.2), "1.0");
    assert_eq!(format_linear(1500.0, 500.0), "1500");
    assert_eq!(format_linear(0.25, 0.25), "0.25");
}

// Why: publication typography uses the minus sign U+2212, not the hyphen, and rounding residue
// near zero must not produce a signed zero label.
#[test]
fn format_linear_uses_unicode_minus_and_never_signs_zero() {
    assert_eq!(format_linear(-2.5, 0.5), format!("{MINUS}2.5"));
    assert!(!format_linear(-2.5, 0.5).contains('-'));
    assert_eq!(format_linear(-1e-17, 0.1), "0.0");
    assert_eq!(format_linear(-0.0, 1.0), "0");
}

// Why: the ×10^k factor must appear exactly when labels would otherwise be too long, and must
// make the largest label a single leading digit.
#[test]
fn common_exponent_thresholds() {
    assert_eq!(common_exponent(&[0.0, 0.5, 1.0]), 0);
    assert_eq!(common_exponent(&[0.0, 5000.0]), 0);
    assert_eq!(common_exponent(&[0.0, 5000.0, 10000.0]), 4);
    assert_eq!(common_exponent(&[-2e5, 0.0]), 5);
    assert_eq!(common_exponent(&[0.0005, 0.001]), 0);
    assert_eq!(common_exponent(&[0.0, 0.0002, 0.0004]), -4);
    assert_eq!(common_exponent(&[]), 0);
    assert_eq!(common_exponent(&[0.0]), 0);
    assert_eq!(common_exponent(&[f64::NAN, 3e6]), 6);
}

// Why: log axes are labelled at decades; loglog plots of 1..1000 must show 1, 10, 100, 1000 with
// the familiar 2..9 minor ticks in each decade.
#[test]
fn log_ticks_over_three_decades() {
    let ticks = log_ticks(1.0, 1000.0, 5);
    assert_eq!(ticks.major, vec![1.0, 10.0, 100.0, 1000.0]);
    assert_eq!(ticks.step, 1.0);
    assert_eq!(ticks.minor.len(), 24);
    for expected in [2.0, 3.0, 9.0, 20.0, 30.0, 90.0, 200.0, 900.0] {
        assert!(ticks.minor.contains(&expected), "missing minor {expected}");
    }
    assert!(ticks.minor.iter().all(|m| (1.0..=1000.0).contains(m)));
    assert!(ticks.minor.iter().all(|m| !ticks.major.contains(m)));
}

// Why: negative decades are computed by division, and must still be the exact literals
// (0.01, not 0.010000000000000002) so labels and limits compare equal.
#[test]
fn log_ticks_below_one_are_exact() {
    let ticks = log_ticks(0.001, 1.0, 5);
    assert_eq!(ticks.major, vec![0.001, 0.01, 0.1, 1.0]);
    assert!(ticks.minor.contains(&0.02));
    assert!(ticks.minor.contains(&0.005));
}

// Why: powers of ten above 1e22 are not exactly representable, so repeated multiplication drifts
// away from the literal; decade ticks must still equal the literals (1e23, not
// 1.0000000000000001e23) so that they compare equal to user limits such as 1e30.
#[test]
fn log_ticks_above_1e22_are_exact_literals() {
    let ticks = log_ticks(1e20, 1e30, 10);
    let expected: Vec<f64> = (20..=30)
        .map(|n| format!("1e{n}").parse().unwrap())
        .collect();
    assert_eq!(ticks.major, expected);
    assert_eq!(nice_log_limits(2e22, 5e29), (1e22, 1e30));
}

// Why: log limits are usually exact decades, and pan/zoom arithmetic in log space returns them
// only up to rounding; the end decades must stay labelled (as exact literals) and their minors
// must start inside the range.
#[test]
fn log_ticks_keep_decades_at_the_bounds() {
    let ticks = log_ticks(10.0, 1000.0, 5);
    assert_eq!(ticks.major, vec![10.0, 100.0, 1000.0]);
    assert_eq!(ticks.minor.len(), 16);
    assert_eq!(ticks.minor[0], 20.0);

    let rounded = log_ticks(0.1f64.next_up(), 10f64.next_down(), 5);
    assert_eq!(rounded.major, vec![0.1, 1.0, 10.0]);
    assert_eq!(
        nice_log_limits(0.1f64.next_up(), 100f64.next_up()),
        (0.1, 100.0)
    );
}

// Why: over very wide spans labelling every decade overlaps the labels; thinning must keep
// labels on decades, keep 10^0 when visible, and turn the skipped decades into minor ticks.
#[test]
fn log_ticks_thin_over_twenty_decades() {
    let ticks = log_ticks(1e-10, 1e10, 5);
    assert_eq!(ticks.step, 4.0);
    assert!(ticks.major.len() >= 2 && ticks.major.len() <= 6);
    assert!(ticks.major.contains(&1.0));
    for major in &ticks.major {
        let n = exponent_of_decade(*major).expect("major is a decade");
        assert_eq!(n % 4, 0);
    }
    assert_eq!(ticks.minor.len(), 21 - ticks.major.len());
    assert!(ticks.minor.iter().all(|m| exponent_of_decade(*m).is_some()));
}

// Why: with every decade labelled over many decades, 2..9 minors would be a solid smear.
#[test]
fn log_ticks_drop_sub_decade_minors_beyond_six_decades() {
    let ticks = log_ticks(1.0, 1e7, 10);
    assert_eq!(ticks.step, 1.0);
    assert_eq!(ticks.major.len(), 8);
    assert!(ticks.minor.is_empty());
}

// Why: zooming into less than a decade must still leave labelled ticks, otherwise the axis has
// no scale at all.
#[test]
fn log_ticks_within_one_decade_fall_back_to_linear_ticks() {
    let ticks = log_ticks(2.0, 8.0, 5);
    assert_eq!(ticks.major, vec![2.0, 4.0, 6.0, 8.0]);
    assert_eq!(ticks.step, 2.0);

    let narrow = log_ticks(0.9, 1.1, 5);
    assert!(narrow.major.len() >= 2);
    assert!(
        narrow
            .major
            .iter()
            .all(|v| *v > 0.0 && (0.9..=1.1).contains(v))
    );

    let one_decade_inside = log_ticks(2.0, 50.0, 5);
    assert!(one_decade_inside.major.len() >= 2);
}

// Why: log axes with invalid bounds (from non-positive data) must degrade to a sane default
// instead of producing NaN ticks; argument order must not matter.
#[test]
fn log_ticks_normalise_invalid_and_reversed_ranges() {
    let default = log_ticks(1.0, 10.0, 5);
    assert_eq!(log_ticks(-1.0, 10.0, 5), default);
    assert_eq!(log_ticks(0.0, 0.0, 5), default);
    assert_eq!(log_ticks(1000.0, 1.0, 5), log_ticks(1.0, 1000.0, 5));
}

// Why: automatic log limits end on decades, matching MATLAB, without padding exact decades.
#[test]
fn nice_log_limits_round_outward_to_decades() {
    assert_eq!(nice_log_limits(3.0, 400.0), (1.0, 1000.0));
    assert_eq!(nice_log_limits(10.0, 100.0), (10.0, 100.0));
    assert_eq!(nice_log_limits(0.02, 0.5), (0.01, 1.0));
    assert_eq!(nice_log_limits(10.0, 10.0), (1.0, 100.0));
    assert_eq!(nice_log_limits(3.0, 3.0), (1.0, 10.0));
    assert_eq!(nice_log_limits(-1.0, 5.0), (1.0, 10.0));
    assert_eq!(nice_log_limits(400.0, 3.0), (1.0, 1000.0));
}

// Why: decade labels are typeset as math by the text engine; the LaTeX must be exactly this form
// (braced exponent, hyphen-minus inside math) for negative and zero exponents too.
#[test]
fn format_log_typesets_decades_as_latex() {
    assert_eq!(format_log(100.0), "$10^{2}$");
    assert_eq!(format_log(0.01), "$10^{-2}$");
    assert_eq!(format_log(1.0), "$10^{0}$");
    assert_eq!(format_log(10f64.powi(-3)), "$10^{-3}$");
}

// Why: the sub-decade fallback labels non-decade values, which must be plain short numbers.
#[test]
fn format_log_prints_other_values_plainly() {
    assert_eq!(format_log(20.0), "20");
    assert_eq!(format_log(0.05), "0.05");
    assert_eq!(format_log(f64::NAN), "");
}

fn check_linear_contract(min: f64, span: f64, target: usize) -> Result<Ticks, TestCaseError> {
    let max = min + span;
    let ticks = linear_ticks(min, max, target);
    let step = ticks.step;
    let tol = 1e-9 * step;
    prop_assert!(is_nice_step(step), "step {} is not nice", step);
    prop_assert!(ticks.major.len() <= target + 1);
    prop_assert!(ticks.major.windows(2).all(|w| w[0] < w[1]));
    prop_assert!(
        span / next_smaller_nice_step(step) > target as f64,
        "a smaller nice step would also satisfy the target"
    );
    let first = ticks.major[0];
    let last = *ticks.major.last().unwrap();
    prop_assert!(first >= min - tol && last <= max + tol);
    prop_assert!(first - step < min - tol && last + step > max + tol);
    let decimals = decimals_for(step);
    for major in &ticks.major {
        prop_assert!(is_positive_zero_or_nonzero(*major));
        let multiple = major / step;
        prop_assert!((multiple - multiple.round()).abs() < 1e-6);
        let reparsed: f64 = format!("{major:.decimals$}").parse().unwrap();
        prop_assert_eq!(reparsed, *major, "tick is not the exact decimal");
    }
    Ok(ticks)
}

proptest! {
    // Why: the documented contract (nice, minimal, covering, exact, bounded count) must hold for
    // every ordinary range, not only the hand-picked examples.
    #[test]
    fn linear_ticks_satisfy_contract(
        min in -1e3f64..1e3,
        span in 1e-3f64..1e3,
        target in 5usize..=10,
    ) {
        let ticks = check_linear_contract(min, span, target)?;
        prop_assert!(ticks.major.len() >= 2);
    }

    // Why: the axis box must end on labelled ticks for any data range, which requires the
    // limits to be stable under re-ticking.
    #[test]
    fn nice_limits_end_on_major_ticks(
        min in -1e3f64..1e3,
        span in 1e-3f64..1e3,
        target in 3usize..=10,
    ) {
        let max = min + span;
        let (lo, hi) = nice_limits(min, max, target);
        let ticks = linear_ticks(lo, hi, target);
        let tol = 1e-9 * ticks.step;
        prop_assert!(lo <= min + tol && hi >= max - tol);
        prop_assert!((ticks.major[0] - lo).abs() <= tol);
        prop_assert!((ticks.major.last().unwrap() - hi).abs() <= tol);
    }

    // Why: log majors must always be decades or, in the fallback, positive values within range;
    // a log axis can never show a non-positive tick.
    #[test]
    fn log_ticks_are_positive_and_in_range(
        lo_exp in -12f64..12.0,
        span_exp in 0.01f64..30.0,
        target in 2usize..=10,
    ) {
        let min = 10f64.powf(lo_exp);
        let max = 10f64.powf(lo_exp + span_exp);
        let ticks = log_ticks(min, max, target);
        prop_assert!(ticks.major.len() >= 2);
        prop_assert!(ticks.major.len() <= target.max(5) + 1);
        // Decades within 1e-9 of a bound in log10 space count as inside: 1e-8 relative covers it.
        let (rel_lo, rel_hi) = (min * (1.0 - 1e-8), max * (1.0 + 1e-8));
        for v in ticks.major.iter().chain(&ticks.minor) {
            prop_assert!(*v > 0.0 && *v >= rel_lo && *v <= rel_hi);
        }
    }
}
