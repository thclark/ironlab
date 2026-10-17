//! The fixed aspect of 2D axes: widening limits and fitting plot areas once the margins are known.

use ironlab_ir::{Aspect, AspectAdjust, Dimension};

use crate::display::Rect;
use crate::maths::aspect::{about_centre, fit_box, widen};

use super::Ctx;
use super::limits::{Range, is_3d};

/// The relative change in a span below which widening is considered to have stopped.
const SETTLED: f64 = 1e-9;

/// The fixed aspect an axes asks for and can be given: its ratio and how it is kept.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Request {
    pub ratio: f64,
    pub adjust: AspectAdjust,
}

/// Returns the fixed aspect of every axes, in the order the figure lists them, or `None` for an axes whose aspect is
/// automatic or cannot be kept.
///
/// A fixed aspect on a 3D axes, and a ratio that is not a finite positive number, which validation reports as
/// errors, are ignored with a warning naming the axes, so that the figure is still drawn.
pub(super) fn requests(ctx: &mut Ctx) -> Vec<Option<Request>> {
    let figure = ctx.figure;
    figure
        .axes
        .iter()
        .map(|axes| {
            let ratio = axes.aspect.ratio()?;
            if is_3d(axes) {
                ctx.warn(
                    Some(axes.id),
                    "The aspect applies to two-dimensional axes only, so the fixed aspect of this three-dimensional \
                     axes is ignored.",
                );
                return None;
            }
            if !(ratio.is_finite() && ratio > 0.0) {
                ctx.warn(
                    Some(axes.id),
                    format!(
                        "The aspect ratio {ratio} is not a finite positive number, so the aspect is drawn as automatic."
                    ),
                );
                return None;
            }
            debug_assert!(axes.aspect != Aspect::Auto);
            Some(Request {
                ratio,
                adjust: axes.aspect_adjust,
            })
        })
        .collect()
}

/// Returns the span of a range in scale space.
fn span(range: &Range) -> f64 {
    let (lo, hi) = scaled(range);
    hi - lo
}

/// Returns the bounds of a range in scale space: the values on a linear axis, their base-10 logarithms on a log axis.
fn scaled(range: &Range) -> (f64, f64) {
    if range.log {
        (range.min.log10(), range.max.log10())
    } else {
        (range.min, range.max)
    }
}

/// Returns `range` with its scale-space span set to `span`, about the centre of its scale-space bounds.
fn with_span(range: Range, span: f64) -> Range {
    let (lo, hi) = about_centre(scaled(&range), span);
    let (min, max) = if range.log {
        (10f64.powf(lo), 10f64.powf(hi))
    } else {
        (lo, hi)
    };
    if min.is_finite() && max.is_finite() && min < max {
        Range { min, max, ..range }
    } else {
        range
    }
}

/// Widens the limits of every axes whose aspect is kept by `Limits`, so that its plot area draws the ratio asked for.
///
/// Only the axis that is too short for the plot area widens, symmetrically about its centre in scale space, and a
/// link group widens as one: the span every member of a group shows is the largest span any member needs. Widening
/// repeats until no span grows; because spans only grow, this settles unless two axes linked in both x and y ask for
/// different shapes, in which case widening would never end. Then no limits are widened, so that [`fit_plots`] keeps
/// the aspect of every axes by its plot area instead, and [`warn_unsettled`] names the axes. The limits of every axis
/// of every axes are returned, widened where the aspect requires it, with whether widening settled.
pub(super) fn widen_limits(
    ctx: &Ctx,
    base: &[[Range; 3]],
    plots: &[Rect],
    requests: &[Option<Request>],
) -> (Vec<[Range; 3]>, bool) {
    let figure = ctx.figure;
    let count = figure.axes.len();
    let limited: Vec<usize> = (0..count)
        .filter(|&i| matches!(requests[i], Some(r) if r.adjust == AspectAdjust::Limits))
        .collect();
    if limited.is_empty() {
        return (base.to_vec(), true);
    }
    // The members of the link group of each axes along x and y, as indices into the figure's axes.
    let group = |i: usize, dimension: Dimension| -> Vec<usize> {
        figure
            .linked_axes(figure.axes[i].id, dimension)
            .into_iter()
            .filter_map(|id| figure.axes.iter().position(|a| a.id == id))
            .collect()
    };
    let groups: Vec<[Vec<usize>; 2]> = (0..count)
        .map(|i| [group(i, Dimension::X), group(i, Dimension::Y)])
        .collect();
    let mut spans: Vec<[f64; 2]> = base.iter().map(|r| [span(&r[0]), span(&r[1])]).collect();

    let passes = 2 * count + 2;
    let mut settled = false;
    for _ in 0..passes {
        let mut grew = false;
        for &i in &limited {
            let Some(request) = requests[i] else { continue };
            let plot = plots[i];
            let [sx, sy] = spans[i];
            let ((x0, x1), (y0, y1)) =
                widen((0.0, sx), (0.0, sy), plot.width, plot.height, request.ratio);
            for (dim, needed) in [(0, x1 - x0), (1, y1 - y0)] {
                if needed > spans[i][dim] * (1.0 + SETTLED) {
                    for &j in &groups[i][dim] {
                        if needed > spans[j][dim] * (1.0 + SETTLED) {
                            spans[j][dim] = needed;
                            grew = true;
                        }
                    }
                }
            }
        }
        if !grew {
            settled = true;
            break;
        }
    }

    if !settled {
        return (base.to_vec(), false);
    }

    let ranges = base
        .iter()
        .zip(&spans)
        .map(|(r, [sx, sy])| {
            let mut out = *r;
            if *sx > span(&r[0]) {
                out[0] = with_span(r[0], *sx);
            }
            if *sy > span(&r[1]) {
                out[1] = with_span(r[1], *sy);
            }
            out
        })
        .collect();
    (ranges, true)
}

/// Names, in a warning each, the axes whose aspect is kept by `Limits` when widening did not settle, so that their
/// aspect is kept by their plot areas instead.
pub(super) fn warn_unsettled(ctx: &mut Ctx, requests: &[Option<Request>]) {
    let figure = ctx.figure;
    for (axes, request) in figure.axes.iter().zip(requests) {
        if matches!(request, Some(r) if r.adjust == AspectAdjust::Limits) {
            ctx.warn(
                Some(axes.id),
                "Axes linked in both x and y ask for different shapes, so the aspect cannot be kept by widening \
                 their shared limits and is kept by shrinking the plot area instead.",
            );
        }
    }
}

/// Fits the plot area of every axes with a fixed aspect to the ratio of its limits, shrinking one side and centring
/// the area in the space the layout gave it.
///
/// An axes kept by `Limits` whose limits were widened already has the ratio, to within rounding, so its area changes
/// by no more than that rounding; fitting it as well makes its ratio exact.
pub(super) fn fit_plots(plots: &mut [Rect], ranges: &[[Range; 3]], requests: &[Option<Request>]) {
    for ((plot, range), request) in plots.iter_mut().zip(ranges).zip(requests) {
        if let Some(request) = request {
            *plot = fit_box(*plot, span(&range[0]), span(&range[1]), request.ratio);
        }
    }
}
