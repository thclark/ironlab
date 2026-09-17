//! Ticks and the measured text of axes decorations: tick labels, common exponents, axis labels and titles.

use ironlab_ir::{Axes, NodeId};

use crate::maths::ticks;

use super::Ctx;
use super::limits::{Range, axes_axis, is_3d, x_length_estimate};
use super::text::{self, TextBlock, measure_str};
use crate::display::Rect;

/// Tick labels are this fraction of the base font size.
pub(super) const TICK_SCALE: f64 = 0.9;
/// Axes titles are this multiple of the base font size.
pub(super) const TITLE_SCALE: f64 = 1.1;

/// The ticks of one axis with their measured labels.
pub(crate) struct AxisTicks {
    /// Major tick positions inside the limits, in data units.
    pub major: Vec<f64>,
    /// Minor tick positions inside the limits, in data units (drawn on log axes only).
    pub minor: Vec<f64>,
    /// One label per major tick.
    pub labels: Vec<TextBlock>,
    /// The common exponent label `×10^k`, when the labels show mantissas.
    pub exponent: Option<TextBlock>,
}

impl AxisTicks {
    /// The widest label, in points.
    pub fn max_width(&self) -> f64 {
        self.labels.iter().map(TextBlock::width).fold(0.0, f64::max)
    }

    /// The largest extent of any label above its baseline.
    pub fn max_height(&self) -> f64 {
        self.labels
            .iter()
            .map(TextBlock::height)
            .fold(0.0, f64::max)
    }

    /// The largest extent of any label below its baseline.
    pub fn max_depth(&self) -> f64 {
        self.labels.iter().map(TextBlock::depth).fold(0.0, f64::max)
    }
}

/// The measured decorations of one axes.
pub(crate) struct Decor {
    /// Ticks of the x, y and z axes; the z ticks of a 2D axes are empty.
    pub ticks: [AxisTicks; 3],
    /// Labels of the x, y and z axes.
    pub labels: [Option<TextBlock>; 3],
    pub title: Option<TextBlock>,
}

/// Computes the ticks of every axis of an axes and measures all of its decoration text.
pub(super) fn measure(
    ctx: &mut Ctx,
    axes: &Axes,
    ranges: &[Range; 3],
    targets: &[usize; 3],
) -> Decor {
    let fs = ctx.font_size;
    let dims = if is_3d(axes) { 3 } else { 2 };
    let ticks = std::array::from_fn(|dim| {
        if dim < dims {
            axis_ticks(ctx, axes.id, ranges[dim], targets[dim])
        } else {
            AxisTicks {
                major: Vec::new(),
                minor: Vec::new(),
                labels: Vec::new(),
                exponent: None,
            }
        }
    });
    let labels = std::array::from_fn(|dim| {
        let label = axes_axis(axes, dim).label.as_ref()?;
        (dim < dims).then(|| text::measure(ctx, label, fs, axes.id))
    });
    let title = axes
        .title
        .as_ref()
        .map(|t| text::measure(ctx, t, TITLE_SCALE * fs, axes.id));
    Decor {
        ticks,
        labels,
        title,
    }
}

/// The smallest clear gap between neighbouring x tick labels of a 2D axes, in font sizes.
const X_LABEL_GAP: f64 = 1.5;

/// Returns the x tick target of a 2D axes reduced, when necessary, so that its tick labels fit along the axis.
///
/// The labels fit when the distance between neighbouring major ticks along the estimated axis length is at least the
/// widest label plus [`X_LABEL_GAP`] font sizes. When they do not fit, the target becomes the number of such
/// distances that the axis length holds, and always decreases, so that repeatedly fitting and recomputing the limits
/// terminates. A logarithmic x axis, a 3D axes and a target of two are returned unchanged.
pub(super) fn fit_x_target(
    ctx: &mut Ctx,
    axes: &Axes,
    range: Range,
    target: usize,
    outer: Rect,
) -> usize {
    if is_3d(axes) || range.log || target <= 2 {
        return target;
    }
    let ticks = axis_ticks(ctx, axes.id, range, target);
    let [a, b, ..] = ticks.major[..] else {
        return target;
    };
    let length = x_length_estimate(outer);
    let spacing = length * (b - a) / (range.max - range.min);
    let needed = ticks.max_width() + X_LABEL_GAP * ctx.font_size;
    if !(spacing.is_finite() && needed.is_finite()) || spacing >= needed {
        return target;
    }
    let fit = (length / needed).floor();
    let fit = if fit.is_finite() && fit >= 2.0 {
        fit as usize
    } else {
        2
    };
    fit.min(target - 1).max(2)
}

/// Computes the ticks of one axis and typesets their labels.
fn axis_ticks(ctx: &mut Ctx, owner: NodeId, range: Range, target: usize) -> AxisTicks {
    let size = TICK_SCALE * ctx.font_size;
    let inside = |v: &f64| {
        let tol = 1e-9 * (range.max - range.min).abs();
        v.is_finite() && *v >= range.min - tol && *v <= range.max + tol
    };
    if range.log {
        let t = ticks::log_ticks(range.min, range.max, target);
        let major: Vec<f64> = t.major.into_iter().filter(inside).collect();
        let labels = major
            .iter()
            .map(|v| {
                let label = ticks::format_log(*v);
                let math = label.starts_with('$');
                measure_str(ctx, &label, math, size, owner)
            })
            .collect();
        return AxisTicks {
            major,
            minor: t.minor.into_iter().filter(inside).collect(),
            labels,
            exponent: None,
        };
    }
    let t = ticks::linear_ticks(range.min, range.max, target);
    let major: Vec<f64> = t.major.into_iter().filter(inside).collect();
    let k = ticks::common_exponent(&major);
    let factor = 10f64.powi(k);
    let labels = major
        .iter()
        .map(|v| {
            let label = ticks::format_linear(v / factor, t.step / factor);
            measure_str(ctx, &label, false, size, owner)
        })
        .collect();
    let exponent =
        (k != 0).then(|| measure_str(ctx, &format!("$\\times 10^{{{k}}}$"), true, size, owner));
    AxisTicks {
        major,
        minor: Vec::new(),
        labels,
        exponent,
    }
}
