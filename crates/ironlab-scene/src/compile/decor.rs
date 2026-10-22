//! Ticks and the measured text of axes decorations: tick labels, common exponents, axis labels and titles.

use ironlab_ir::{Axes, Axis, NodeId, TickLabels, Ticks};

use crate::maths::ticks;

use super::Ctx;
use super::limits::{Range, axes_axis, is_3d, x_length_estimate};
use super::text::{self, TextBlock, measure_str};
use crate::display::Rect;

/// Tick labels are this fraction of the base font size.
pub(super) const TICK_SCALE: f64 = 0.9;
/// The largest number of ticks a regular step may place on one axis; a finer step falls back to automatic ticks.
const MAX_STEPPED_TICKS: usize = 500;
/// The fewest labelled ticks that automatic ticks give the x axis of a 2D axes whose limits hold them at a round step.
/// Two are the fewest from which a value can be read, and the labels of an x axis must also fit along it. A colourbar
/// takes as few, for the same reason.
pub(super) const MIN_X_TICKS: usize = 2;
/// The fewest labelled ticks that automatic ticks give any other axis whose limits hold them at a round step.
const MIN_TICKS: usize = 3;
/// How many more intervals than its target an axis may be given to reach its fewest ticks.
const MAX_EXTRA_INTERVALS: usize = 20;
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
            let axis = axes_axis(axes, dim);
            let fewest = if dims == 2 && dim == 0 {
                MIN_X_TICKS
            } else {
                MIN_TICKS
            };
            let name = ["x", "y", "z"][dim];
            axis_ticks(ctx, axes.id, axis, name, ranges[dim], targets[dim], fewest)
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
/// terminates. A logarithmic x axis, a 3D axes, an x axis whose ticks are not automatic or whose labels are hidden,
/// and a target of two are returned unchanged.
pub(super) fn fit_x_target(
    ctx: &mut Ctx,
    axes: &Axes,
    range: Range,
    target: usize,
    outer: Rect,
) -> usize {
    let automatic = axes.x.ticks == Ticks::Auto && axes.x.tick_labels != TickLabels::Hidden;
    if is_3d(axes) || range.log || target <= 2 || !automatic {
        return target;
    }
    let ticks = axis_ticks(ctx, axes.id, &axes.x, "x", range, target, MIN_X_TICKS);
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
///
/// The ticks are those the axis asks for: automatic ticks, every multiple of a regular step from its origin, or the
/// explicit values, each kept only where it lies within the limits. Automatic ticks on a linear axis are given more
/// intervals than `target` when that is what it takes for at least `fewest` of them to lie within the limits, which
/// matters only for manual limits narrower than the round step the target gives. A regular step that would place more
/// than [`MAX_STEPPED_TICKS`] ticks falls back to automatic ticks with a warning naming `owner`, which calls the axis
/// `name`. A colourbar draws its ticks through an axis built from its scale, so that its ticks and an axis's agree.
///
/// The labels are what the axis asks for: none when they are hidden, the explicit texts beside the values they name,
/// or the formatted values. Explicit texts that do not name explicit values one by one, which validation reports, are
/// replaced by the formatted values, so that the figure is still drawn.
pub(super) fn axis_ticks(
    ctx: &mut Ctx,
    owner: NodeId,
    axis: &Axis,
    name: &str,
    range: Range,
    target: usize,
    fewest: usize,
) -> AxisTicks {
    let size = TICK_SCALE * ctx.font_size;
    let inside = |v: &f64| {
        let tol = 1e-9 * (range.max - range.min).abs();
        v.is_finite() && *v >= range.min - tol && *v <= range.max + tol && (!range.log || *v > 0.0)
    };
    let explicit = match &axis.ticks {
        Ticks::Auto => None,
        Ticks::Step { step, origin } => {
            match ticks::stepped_ticks(range.min, range.max, *step, *origin, MAX_STEPPED_TICKS) {
                Some(values) => Some(values.into_iter().map(|v| (v, None)).collect::<Vec<_>>()),
                None => {
                    ctx.warn(
                        Some(owner),
                        format!(
                            "The {name} tick step {step} from {origin} is too fine for the limits [{}, {}] or \
                             cannot be placed, so automatic ticks are used.",
                            range.min,
                            range.max
                        ),
                    );
                    None
                }
            }
        }
        Ticks::Values(values) => {
            let texts = match &axis.tick_labels {
                TickLabels::Text(texts) if texts.len() == values.len() => Some(texts),
                _ => None,
            };
            Some(
                values
                    .iter()
                    .enumerate()
                    .map(|(k, v)| (*v, texts.map(|t| &t[k])))
                    .collect(),
            )
        }
    };
    let hidden = axis.tick_labels == TickLabels::Hidden;
    let Some(explicit) = explicit else {
        let mut auto = automatic_ticks(ctx, owner, range, target, fewest);
        if hidden {
            auto.labels.clear();
            auto.exponent = None;
        }
        return auto;
    };
    let (major, texts): (Vec<f64>, Vec<_>) =
        explicit.into_iter().filter(|(v, _)| inside(v)).unzip();
    if hidden {
        return AxisTicks {
            major,
            minor: Vec::new(),
            labels: Vec::new(),
            exponent: None,
        };
    }
    if let Some(texts) = texts.iter().copied().collect::<Option<Vec<_>>>() {
        let labels = texts
            .into_iter()
            .map(|t| text::measure(ctx, t, size, owner))
            .collect();
        return AxisTicks {
            major,
            minor: Vec::new(),
            labels,
            exponent: None,
        };
    }
    if range.log {
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
            minor: Vec::new(),
            labels,
            exponent: None,
        };
    }
    let k = ticks::common_exponent(&major);
    let factor = 10f64.powi(k);
    let scaled: Vec<f64> = major.iter().map(|v| v / factor).collect();
    let decimals = ticks::decimals_for(&scaled);
    let labels = scaled
        .iter()
        .map(|v| measure_str(ctx, &ticks::format_fixed(*v, decimals), false, size, owner))
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

/// Computes automatic ticks for one axis and typesets their labels, giving a linear axis more intervals than `target`
/// where that is needed for at least `fewest` ticks to lie within its limits.
fn automatic_ticks(
    ctx: &mut Ctx,
    owner: NodeId,
    range: Range,
    target: usize,
    fewest: usize,
) -> AxisTicks {
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
    let linear = |target: usize| {
        let t = ticks::linear_ticks(range.min, range.max, target);
        let major: Vec<f64> = t.major.iter().copied().filter(inside).collect();
        (t, major)
    };
    let (mut t, mut major) = linear(target);
    // Each added interval can only make the step finer, and a step of a fifth of the span places at least four ticks,
    // so a few more intervals suffice; the bound only guarantees that the search ends.
    for more in target + 1..=target + MAX_EXTRA_INTERVALS {
        if major.len() >= fewest {
            break;
        }
        (t, major) = linear(more);
    }
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
