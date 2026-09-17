//! Tick positions, automatic axis limits and tick label formatting.
//!
//! # Linear ticks
//!
//! Major tick steps are "nice numbers": a mantissa from {1, 2, 5} multiplied by an integer power
//! of ten. The step 2.5 is deliberately excluded, because it introduces an extra decimal place
//! in every label (0.25, 0.50, 0.75) where the neighbouring steps 0.2 and 0.5 need only one.
//! The chosen step is the smallest nice step for which the range spans at most `target`
//! intervals, so a range never carries more than `target + 1` major ticks. The comparison allows
//! a relative tolerance of 1e-9, so that rounding in the computed span (for example
//! `0.07 / 0.01 = 7.000000000000001`) never forces a coarser step.
//!
//! Every tick value is computed from an integer multiple of the step's mantissa and a single
//! exact multiplication or division by a power of ten, so that the value is the closest `f64` to
//! the decimal number it represents (0.3, never 0.30000000000000004), and a tick at zero is
//! always positive zero.
//!
//! # Logarithmic ticks
//!
//! Major ticks sit at decades. When more decades are visible than `max(target, 2) + 1`, only every
//! `n`-th decade is labelled, where `n` is the smallest stride that satisfies the target and the
//! labelled exponents are the multiples of `n` (so the decade 10⁰ stays labelled while panning).
//! When fewer than two decades fall inside the range, the linear algorithm supplies the majors.
//!
//! # Labels
//!
//! Labels use U+2212 MINUS SIGN for negative numbers, as publication typography requires.
//! Labels of exact decades on logarithmic axes are LaTeX math strings (`$10^{n}$`), because the
//! text engine typesets `$…$` segments as math.

/// Positions of the major and minor ticks on one axis, in data units, in ascending order.
#[derive(Debug, Clone, PartialEq)]
pub struct Ticks {
    /// Labelled tick positions, strictly increasing, all inside the requested range up to the
    /// tolerance each function documents.
    pub major: Vec<f64>,
    /// Unlabelled tick positions, strictly increasing, all inside the requested range up to the
    /// same tolerance, and never coinciding with a major tick.
    pub minor: Vec<f64>,
    /// The spacing between consecutive major ticks.
    ///
    /// For [`linear_ticks`] this is the nice step in data units. For [`log_ticks`] it is the
    /// decade stride in log10 units (1.0 when every decade is labelled, 2.0 when every other
    /// decade is labelled, and so on) when the majors are decades, and the linear step in data
    /// units when the sub-decade fallback supplied the majors.
    pub step: f64,
}

/// Chooses nice major and minor ticks for a linear axis spanning `[min, max]`.
///
/// Majors are the multiples of the nice step (see the module documentation) that lie inside
/// `[min, max]`, where the bounds are widened by a relative tolerance of 1e-9 of the step so
/// that a bound which is a multiple of the step up to rounding still carries a tick. Minors
/// subdivide each major interval into five parts for the mantissas 1 and 5 and into four parts
/// for the mantissa 2 (so minors of a 0.2 step fall on multiples of 0.05), and are restricted
/// to `[min, max]`.
///
/// Arguments are normalised as follows:
/// - `min > max` is treated as `[max, min]`.
/// - `target` below 1 is treated as 1.
/// - If either bound is not finite, the range `[0, 1]` is used.
/// - A degenerate range (`min == max`) is first expanded exactly as [`nice_limits`] expands it.
/// - If the range is so narrow relative to its magnitude that consecutive multiples of the step
///   are not distinct `f64` values, the returned majors are still strictly increasing (repeated
///   values are dropped), which may leave fewer ticks than usual.
pub fn linear_ticks(min: f64, max: f64, target: usize) -> Ticks {
    let (mut lo, mut hi) = normalise_linear(min, max);
    if lo == hi {
        (lo, hi) = widen_degenerate(lo);
    }
    let Some(step) = NiceStep::choose(hi - lo, target) else {
        return fallback_ticks(lo, hi);
    };
    let tol = STEP_TOLERANCE * step.value();
    let major = multiples_in(step, lo - tol, hi + tol, |_| true);

    let (minor_step, subdivisions) = step.minor();
    let minor = multiples_in(minor_step, lo - tol, hi + tol, |q| q % subdivisions != 0)
        .into_iter()
        .filter(|v| major.binary_search_by(|m| m.total_cmp(v)).is_err())
        .collect();

    Ticks {
        major,
        minor,
        step: step.value(),
    }
}

/// Computes automatic axis limits by rounding `[min, max]` outward to major tick multiples.
///
/// This reproduces MATLAB's automatic limits: the axis box ends on labelled ticks. The returned
/// limits are the first and last major ticks of `linear_ticks(lo, hi, target)` for the returned
/// `(lo, hi)`. When rounding outward widens the range enough to change the step, the widened
/// range is rounded outward again with the new step, until the step no longer changes (the
/// process terminates because the step never decreases while the range grows). A bound that lies
/// on a multiple of the step, to within the relative tolerance of 1e-9 of the step used by
/// [`linear_ticks`], is not widened; it is replaced by that exact multiple, so rounding residue
/// such as `-1e-16` gives the limit `0.0` (positive zero), not `-0.2`.
///
/// Arguments are normalised as follows:
/// - `min > max` is treated as `[max, min]`.
/// - `target` below 2 is treated as 2, because a range that straddles zero can never start and
///   end on multiples of a step that it spans only once, so the rounding would not terminate.
/// - If either bound is not finite, the limits are `(0.0, 1.0)`.
/// - A degenerate range at zero (`min == max == 0`) becomes `(-1.0, 1.0)`.
/// - A degenerate range at `v != 0` is widened to `[v - 0.1·|v|, v + 0.1·|v|]` before outward
///   rounding, so the value sits near the middle of the axis.
pub fn nice_limits(min: f64, max: f64, target: usize) -> (f64, f64) {
    if !(min.is_finite() && max.is_finite()) {
        return (0.0, 1.0);
    }
    let (mut lo, mut hi) = normalise_linear(min, max);
    if lo == hi {
        (lo, hi) = widen_degenerate(lo);
    }
    let target = target.max(2);
    let Some(mut step) = NiceStep::choose(hi - lo, target) else {
        return (lo, hi);
    };
    // The step grows by a factor of at least two whenever it changes, so a handful of rounds
    // reaches the fixed point; the bound only guards against pathological inputs.
    for _ in 0..64 {
        let tol = STEP_TOLERANCE * step.value();
        let (new_lo, new_hi) = (
            step.multiple(step.floor_index(lo + tol)),
            step.multiple(step.ceil_index(hi - tol)),
        );
        if !(new_lo.is_finite() && new_hi.is_finite()) {
            // Rounding outward would overflow; the range is already at the edge of f64.
            return (lo, hi);
        }
        match NiceStep::choose(new_hi - new_lo, target) {
            Some(next) if next != step => {
                (lo, hi, step) = (new_lo, new_hi, next);
            }
            _ => return (new_lo, new_hi),
        }
    }
    (lo, hi)
}

/// Chooses major and minor ticks for a logarithmic axis spanning `[min, max]`.
///
/// Majors are the decades `10^n` inside the range whose exponent `n` is a multiple of the
/// stride, where the stride is the smallest positive integer that leaves at most
/// `max(target, 2) + 1` majors. (Allowing at least three majors guarantees that the thinned
/// decades still include at least two labels.) Minors depend on the stride:
/// - stride 1 and a span of at most six decades: the values `k·10^n` for `k` in 2..=9 inside the
///   range;
/// - stride 1 and a span of more than six decades: none, because they would be too dense;
/// - stride greater than 1: the unlabelled decades inside the range.
///
/// If fewer than two decades lie inside the range, the majors and minors are those of
/// `linear_ticks(min, max, max(target, 5))`, and `step` is the linear step. These ticks are all
/// positive, because the range is, and a target of at least five guarantees at least two majors
/// (the chosen step is less than half the span).
///
/// A decade counts as inside the range when its exponent is within 1e-9 of `log10` of the range,
/// so a bound that is a decade up to rounding still carries its label.
///
/// Every decade value is exactly the `f64` nearest to its decimal literal (for example `0.01`),
/// as is every minor `k·10^n`.
///
/// Arguments are normalised as follows: `min > max` is treated as `[max, min]`; if either bound
/// is not finite or not strictly positive, the range `[1, 10]` is used.
pub fn log_ticks(min: f64, max: f64, target: usize) -> Ticks {
    let (lo, hi) = normalise_log(min, max);
    let (log_lo, log_hi) = (lo.log10(), hi.log10());
    let first = (log_lo - DECADE_TOLERANCE).ceil() as i64;
    let last = (log_hi + DECADE_TOLERANCE).floor() as i64;
    if last - first + 1 < 2 {
        return linear_ticks(lo, hi, target.max(5));
    }

    let limit = target.max(2) as i64 + 1;
    let count_multiples =
        |stride: i64| last.div_euclid(stride) - (first + stride - 1).div_euclid(stride) + 1;
    let stride = (1..).find(|s| count_multiples(*s) <= limit).unwrap_or(1);

    let decades = first..=last;
    let major: Vec<f64> = decades
        .clone()
        .filter(|n| n.rem_euclid(stride) == 0)
        .map(|n| decimal(1, n))
        .collect();

    let minor = if stride > 1 {
        decades
            .filter(|n| n.rem_euclid(stride) != 0)
            .map(|n| decimal(1, n))
            .collect()
    } else if log_hi - log_lo <= 6.0 + DECADE_TOLERANCE {
        let inside = |v: &f64| {
            let l = v.log10();
            l >= log_lo - DECADE_TOLERANCE && l <= log_hi + DECADE_TOLERANCE
        };
        (log_lo.floor() as i64..=log_hi.floor() as i64)
            .flat_map(|n| (2..=9).map(move |k| decimal(k, n)))
            .filter(inside)
            .collect()
    } else {
        Vec::new()
    };

    Ticks {
        major,
        minor,
        step: stride as f64,
    }
}

/// Computes automatic logarithmic axis limits by rounding `[min, max]` outward to decades.
///
/// Bounds within 1e-9 of a decade in `log10` space are replaced by that exact decade rather than
/// widened. A degenerate range at an exact decade
/// `10^n` becomes `(10^(n-1), 10^(n+1))`. `min > max` is treated as `[max, min]`. If either bound
/// is not finite or not strictly positive, the limits are `(1.0, 10.0)`.
pub fn nice_log_limits(min: f64, max: f64) -> (f64, f64) {
    let (lo, hi) = normalise_log(min, max);
    let snap = |l: f64, outward: fn(f64) -> f64| {
        let nearest = l.round();
        if (l - nearest).abs() <= DECADE_TOLERANCE {
            nearest as i64
        } else {
            outward(l) as i64
        }
    };
    let (mut first, mut last) = (snap(lo.log10(), f64::floor), snap(hi.log10(), f64::ceil));
    if first == last {
        (first, last) = (first - 1, last + 1);
    }
    (decimal(1, first), decimal(1, last))
}

/// Formats a linear tick value with exactly as many decimal places as `step` needs.
///
/// The number of decimals is the smallest `d` in 0..=15 for which `step · 10^d` is an integer to
/// within a relative tolerance of 1e-9, so all labels on an axis share the same number of
/// decimals (0.0, 0.5, 1.0). Negative values use U+2212 MINUS SIGN. A value that rounds to zero
/// at that precision is printed without a sign. If `step` is not finite or not positive, the
/// value is printed with the shortest representation that round-trips.
///
/// No exponent is ever used: callers that need `×10^k` notation divide values and step by
/// `10^k`, using [`common_exponent`], before formatting.
pub fn format_linear(value: f64, step: f64) -> String {
    let text = if step.is_finite() && step > 0.0 {
        let decimals = (0..=15)
            .find(|&d| {
                let scaled = step * 10f64.powi(d);
                (scaled - scaled.round()).abs() <= STEP_TOLERANCE * scaled.abs()
            })
            .unwrap_or(15) as usize;
        format!("{value:.decimals$}")
    } else {
        format!("{value}")
    };
    typeset_sign(text)
}

/// Returns the power of ten to factor out of an axis's tick labels, or 0 when none is needed.
///
/// Let `m` be the largest absolute value among the finite ticks. When `m ≥ 1e4` or
/// `0 < m < 1e-3`, the result is `floor(log10(m))`, so the largest label shows a mantissa in
/// [1, 10) and the axis shows `×10^k`. Otherwise, including for an empty slice or all-zero ticks,
/// the result is 0.
pub fn common_exponent(ticks: &[f64]) -> i32 {
    let largest = ticks
        .iter()
        .filter(|v| v.is_finite())
        .map(|v| v.abs())
        .fold(0.0, f64::max);
    if !(largest >= 1e4 || (largest > 0.0 && largest < 1e-3)) {
        return 0;
    }
    // log10 may round across a power of ten; correct the estimate against exact decades.
    let mut exponent = largest.log10().floor() as i64;
    if decimal(1, exponent) > largest {
        exponent -= 1;
    } else if decimal(1, exponent + 1) <= largest {
        exponent += 1;
    }
    exponent as i32
}

/// Formats a logarithmic tick value.
///
/// An exact decade `10^n` (to within 1e-9 in log10 space) is returned as the LaTeX math string
/// `$10^{n}$`, with an ASCII hyphen-minus for negative exponents because LaTeX typesets it as a
/// minus sign. Any other finite value is returned as a plain number: rounded to 12 significant
/// digits, printed without an exponent, with U+2212 MINUS SIGN if negative. A non-finite value
/// gives an empty string.
pub fn format_log(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    if value > 0.0 {
        let l = value.log10();
        let n = l.round();
        if (l - n).abs() < DECADE_TOLERANCE {
            return format!("$10^{{{}}}$", n as i64);
        }
    }
    let rounded: f64 = format!("{value:.11e}")
        .parse()
        .expect("a formatted finite float parses");
    typeset_sign(format!("{rounded}"))
}

/// Relative tolerance, as a fraction of the step, used when comparing spans and bounds with
/// multiples of a linear step.
const STEP_TOLERANCE: f64 = 1e-9;

/// Tolerance in `log10` units used when deciding whether a value is a decade or lies in range.
const DECADE_TOLERANCE: f64 = 1e-9;

/// The powers of ten that are exactly representable as `f64`.
const EXACT_POWERS_OF_TEN: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

/// Returns the `f64` nearest to the decimal number `digits · 10^exponent`.
///
/// When both the integer and the power of ten are exactly representable, one IEEE multiplication
/// or division is correctly rounded and gives the nearest value. Otherwise the decimal literal is
/// parsed, which is also correctly rounded.
fn decimal(digits: i128, exponent: i64) -> f64 {
    if digits == 0 {
        return 0.0;
    }
    let power = exponent.unsigned_abs() as usize;
    if digits.unsigned_abs() <= 1 << 53 && power < EXACT_POWERS_OF_TEN.len() {
        let (d, p) = (digits as f64, EXACT_POWERS_OF_TEN[power]);
        if exponent >= 0 { d * p } else { d / p }
    } else {
        format!("{digits}e{exponent}")
            .parse()
            .expect("a decimal literal parses")
    }
}

/// A nice step: `mantissa · 10^exponent` with the mantissa 1, 2 or 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NiceStep {
    mantissa: i128,
    exponent: i64,
}

impl NiceStep {
    /// Returns the smallest nice step for which `span` covers at most `target` steps, up to the
    /// relative tolerance, or `None` if the span is not finite and positive.
    fn choose(span: f64, target: usize) -> Option<Self> {
        if !(span.is_finite() && span > 0.0) {
            return None;
        }
        let target = target.max(1) as f64;
        let limit = target * (1.0 + STEP_TOLERANCE);
        // log10 may be off by one near a power of ten, so start one decade lower; the first
        // candidate at or above span / target always satisfies the limit.
        let start = (span / target).log10().floor() as i64 - 1;
        (start..=start + 3)
            .flat_map(|exponent| [1, 2, 5].map(|mantissa| NiceStep { mantissa, exponent }))
            .find(|step| {
                let value = step.value();
                value.is_finite() && value > 0.0 && span / value <= limit
            })
    }

    fn value(self) -> f64 {
        self.multiple(1)
    }

    /// The nearest `f64` to `k` times the step.
    fn multiple(self, k: i128) -> f64 {
        decimal(k.saturating_mul(self.mantissa), self.exponent)
    }

    /// The minor step and the number of minor intervals per major interval.
    fn minor(self) -> (NiceStep, i128) {
        let (mantissa, exponent, subdivisions) = match self.mantissa {
            1 => (2, self.exponent - 1, 5),
            2 => (5, self.exponent - 1, 4),
            _ => (1, self.exponent, 5),
        };
        (NiceStep { mantissa, exponent }, subdivisions)
    }

    /// The largest `k` whose multiple is at most `x`, with `x` clamped to the finite range.
    fn floor_index(self, x: f64) -> i128 {
        let x = x.clamp(f64::MIN, f64::MAX);
        let mut k = float_to_index((x / self.value()).floor());
        // The float estimate is off by at most a few indices, and by one per repeated value
        // where consecutive multiples round to the same f64; the bound only guarantees
        // termination.
        for _ in 0..INDEX_CORRECTIONS {
            if self.multiple(k) > x {
                k -= 1;
            } else if self.multiple(k + 1) <= x {
                k += 1;
            } else {
                break;
            }
        }
        k
    }

    /// The smallest `k` whose multiple is at least `x`, with `x` clamped to the finite range.
    fn ceil_index(self, x: f64) -> i128 {
        let x = x.clamp(f64::MIN, f64::MAX);
        let mut k = float_to_index((x / self.value()).ceil());
        for _ in 0..INDEX_CORRECTIONS {
            if self.multiple(k) < x {
                k += 1;
            } else if self.multiple(k - 1) >= x {
                k -= 1;
            } else {
                break;
            }
        }
        k
    }
}

/// The maximum number of single-index corrections to a float estimate of a multiple's index.
const INDEX_CORRECTIONS: usize = 64;

/// Converts an integral float to an index, saturating far outside any usable range.
fn float_to_index(x: f64) -> i128 {
    x.clamp(-1e30, 1e30) as i128
}

/// Returns the strictly increasing multiples of `step` inside `[lo, hi]` whose index passes
/// `keep`. Multiples that round to the same `f64` as an earlier one are dropped.
fn multiples_in(step: NiceStep, lo: f64, hi: f64, keep: impl Fn(i128) -> bool) -> Vec<f64> {
    let (first, last) = (step.ceil_index(lo), step.floor_index(hi));
    let mut values: Vec<f64> = Vec::new();
    for k in first..=last {
        let v = step.multiple(k);
        if keep(k) && v >= lo && v <= hi && values.last().is_none_or(|prev| v > *prev) {
            values.push(v);
        }
    }
    values
}

/// Orders the bounds and replaces a non-finite range with `[0, 1]`.
fn normalise_linear(min: f64, max: f64) -> (f64, f64) {
    if !(min.is_finite() && max.is_finite()) {
        (0.0, 1.0)
    } else {
        (min.min(max), min.max(max))
    }
}

/// Orders the bounds and replaces a non-finite or non-positive range with `[1, 10]`.
fn normalise_log(min: f64, max: f64) -> (f64, f64) {
    if !(min.is_finite() && max.is_finite() && min > 0.0 && max > 0.0) {
        (1.0, 10.0)
    } else {
        (min.min(max), min.max(max))
    }
}

/// Expands the degenerate range `[v, v]` as documented on [`nice_limits`].
fn widen_degenerate(v: f64) -> (f64, f64) {
    if v == 0.0 {
        return (-1.0, 1.0);
    }
    let half_width = 0.1 * v.abs();
    let (lo, hi) = (
        (v - half_width).max(f64::MIN),
        (v + half_width).min(f64::MAX),
    );
    if lo < hi {
        (lo, hi)
    } else {
        // A subnormal value has no representable 10 % neighbourhood.
        (v - 1.0, v + 1.0)
    }
}

/// Ticks for a range whose span overflows, where no nice step exists: the two bounds alone.
fn fallback_ticks(lo: f64, hi: f64) -> Ticks {
    Ticks {
        major: vec![lo, hi],
        minor: Vec::new(),
        step: hi - lo,
    }
}

/// Removes the sign from a label that shows zero and typesets any remaining minus sign.
fn typeset_sign(text: String) -> String {
    let unsigned = text.strip_prefix('-').unwrap_or(&text);
    if unsigned.parse::<f64>().is_ok_and(|v| v == 0.0) {
        unsigned.to_owned()
    } else {
        text.replacen('-', "\u{2212}", 1)
    }
}
