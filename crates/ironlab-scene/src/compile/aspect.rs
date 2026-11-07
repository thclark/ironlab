//! The fixed aspect of axes: widening limits, fitting the plot areas of 2D axes once the margins are known, and
//! shaping the box of 3D axes.

use ironlab_ir::{AspectAdjust, Axes, Dimension};

use crate::display::Rect;
use crate::maths::aspect::{about_centre, fit_box, widen};

use super::Ctx;
use super::limits::{Range, is_3d};

/// The relative change in a span below which widening is considered to have stopped.
const SETTLED: f64 = 1e-9;

/// The fixed aspect an axes asks for and can be given: the ratios of the length of a unit along each axis to that of
/// a unit of x, how it is kept, and whether the axes is 3D.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Request {
    /// The ratios of x, y and z, the first always 1.
    pub ratios: [f64; 3],
    pub adjust: AspectAdjust,
    pub three_d: bool,
}

impl Request {
    /// The number of axes whose units the aspect relates: 2 in a 2D axes and 3 in a 3D one.
    fn dims(&self) -> usize {
        if self.three_d { 3 } else { 2 }
    }
}

/// Returns whether an axes draws its data at a fixed shape whatever its aspect: a polar axes is always round and a
/// spherical one always a sphere, so validation warns that their aspect is ignored.
fn ignores_aspect(axes: &Axes) -> bool {
    matches!(
        axes.projection,
        ironlab_ir::Projection::Polar { .. } | ironlab_ir::Projection::Spherical { .. }
    )
}

/// Returns the fixed aspect of an axes, or `None` when its aspect is automatic or its ratios are not finite and
/// positive.
fn request_of(axes: &Axes) -> Option<Request> {
    if ignores_aspect(axes) {
        return None;
    }
    let [y, z] = axes.aspect.ratios()?;
    let usable = |r: f64| r.is_finite() && r > 0.0;
    (usable(y) && usable(z)).then_some(Request {
        ratios: [1.0, y, z],
        adjust: axes.aspect_adjust,
        three_d: is_3d(axes),
    })
}

/// Returns the fixed aspect of every axes, in the order the figure lists them, or `None` for an axes whose aspect is
/// automatic or cannot be kept.
///
/// A ratio of y or of z that is not a finite positive number, which validation reports as an error, is drawn as an
/// automatic aspect with a warning naming the axes, so that the figure is still drawn.
pub(super) fn requests(ctx: &mut Ctx) -> Vec<Option<Request>> {
    let figure = ctx.figure;
    figure
        .axes
        .iter()
        .map(|axes| {
            if ignores_aspect(axes) {
                return None;
            }
            let [y, z] = axes.aspect.ratios()?;
            let request = request_of(axes);
            if request.is_none() {
                ctx.warn(
                    Some(axes.id),
                    format!(
                        "The aspect ratios {y} of y and {z} of z are not both finite positive numbers, so the aspect \
                         is drawn as automatic."
                    ),
                );
            }
            request
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

/// Returns the scale-space spans an axes kept by `Limits` needs, given the spans it shows: in 2D, those that give
/// its plot area the ratio of y; in 3D, those whose spans scaled by the ratios are all equal to the largest of them,
/// so that its cube draws them at the aspect.
fn needed(request: &Request, spans: [f64; 3], plot: Rect) -> [f64; 3] {
    if request.three_d {
        let longest = (0..3)
            .map(|k| spans[k] * request.ratios[k])
            .fold(0.0, f64::max);
        std::array::from_fn(|k| longest / request.ratios[k])
    } else {
        let ((x0, x1), (y0, y1)) = widen(
            (0.0, spans[0]),
            (0.0, spans[1]),
            plot.width,
            plot.height,
            request.ratios[1],
        );
        [x1 - x0, y1 - y0, spans[2]]
    }
}

/// Widens the limits of every axes whose aspect is kept by `Limits`, so that its plot area, or the cube of a 3D axes,
/// draws the ratios asked for.
///
/// Only the axes that are too short widen, symmetrically about their centres in scale space, and a link group
/// widens as one: the span every member of a group shows is the largest span any member needs. Widening repeats
/// until no span grows; because spans only grow, this settles unless linked axes ask for different shapes, in which
/// case widening would never end. Then no limits are widened, so that [`fit_plots`] and [`box_sides`] keep the aspect
/// of every axes by its plot area or its box instead, and [`warn_unsettled`] names the axes. The limits of every
/// axis of every axes are returned, widened where the aspect requires it, with whether widening settled.
pub(super) fn widen_limits(
    ctx: &Ctx,
    base: &[[Range; 3]],
    plots: &[Rect],
    requests: &[Option<Request>],
) -> (Vec<[Range; 3]>, bool) {
    let figure = ctx.figure;
    let count = figure.axes.len();
    // The aspect of a cylinder relates its height to its radius, and its angle cannot widen, so it is always kept by
    // the shape of its box.
    let limited: Vec<usize> = (0..count)
        .filter(|&i| matches!(requests[i], Some(r) if r.adjust == AspectAdjust::Limits))
        .filter(|&i| !is_cylindrical(&figure.axes[i]))
        .collect();
    if limited.is_empty() {
        return (base.to_vec(), true);
    }
    // The members of the link group of each axes along x, y and z, as indices into the figure's axes.
    let group = |i: usize, dimension: Dimension| -> Vec<usize> {
        figure
            .linked_axes(figure.axes[i].id, dimension)
            .into_iter()
            .filter_map(|id| figure.axes.iter().position(|a| a.id == id))
            .collect()
    };
    let groups: Vec<[Vec<usize>; 3]> = (0..count)
        .map(|i| {
            [
                group(i, Dimension::X),
                group(i, Dimension::Y),
                group(i, Dimension::Z),
            ]
        })
        .collect();
    let mut spans: Vec<[f64; 3]> = base.iter().map(|r| r.map(|r| span(&r))).collect();

    let passes = 3 * count + 2;
    let mut settled = false;
    for _ in 0..passes {
        let mut grew = false;
        for &i in &limited {
            let Some(request) = requests[i] else { continue };
            let wanted = needed(&request, spans[i], plots[i]);
            for (dim, &needed) in wanted.iter().enumerate().take(request.dims()) {
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
        .map(|(r, s)| {
            std::array::from_fn(|k| {
                if s[k] > span(&r[k]) {
                    with_span(r[k], s[k])
                } else {
                    r[k]
                }
            })
        })
        .collect();
    (ranges, true)
}

/// Names, in a warning each, the axes whose aspect is kept by `Limits` when widening did not settle, so that their
/// aspect is kept by their plot areas or boxes instead.
pub(super) fn warn_unsettled(ctx: &mut Ctx, requests: &[Option<Request>]) {
    let figure = ctx.figure;
    for (axes, request) in figure.axes.iter().zip(requests) {
        if matches!(request, Some(r) if r.adjust == AspectAdjust::Limits) && !is_cylindrical(axes) {
            ctx.warn(
                Some(axes.id),
                "Linked axes ask for different shapes, so the aspect cannot be kept by widening their shared limits \
                 and is kept by shaping the plot area or the box instead.",
            );
        }
    }
}

/// Fits the plot area of every 2D axes with a fixed aspect to the ratio of its limits, shrinking one side and
/// centring the area in the space the layout gave it.
///
/// An axes kept by `Limits` whose limits were widened already has the ratio, to within rounding, so its area changes
/// by no more than that rounding; fitting it as well makes its ratio exact.
pub(super) fn fit_plots(plots: &mut [Rect], ranges: &[[Range; 3]], requests: &[Option<Request>]) {
    for ((plot, range), request) in plots.iter_mut().zip(ranges).zip(requests) {
        if let Some(request) = request.filter(|r| !r.three_d) {
            *plot = fit_box(*plot, span(&range[0]), span(&range[1]), request.ratios[1]);
        }
    }
}

/// Returns whether an axes is cylindrical, whose aspect relates its height to its radius.
fn is_cylindrical(axes: &Axes) -> bool {
    matches!(axes.projection, ironlab_ir::Projection::Cylindrical { .. })
}

/// Returns the relative lengths of the sides of the box of cylindrical axes with a fixed aspect.
///
/// The disc keeps its unit diameter, across which the radial span is drawn from the edge of the hole to the rim, so a
/// unit of radius is `0.5·(1 − hole)/span_r` of the diameter. A unit of height is drawn `z/y` times as long as a unit
/// of radius, so the box takes the sides `[1, 1, h]` with `h = (z/y)·span_z·0.5·(1 − hole)/span_r`, scaled so that the
/// longest side is 1. Spans are in scale space; spans or ratios that are not finite and positive give a cube.
fn cylinder_sides(hole: f64, ratios: [f64; 3], ranges: &[Range; 3]) -> [f64; 3] {
    let hole = if hole.is_finite() {
        hole.clamp(0.0, 0.99)
    } else {
        0.0
    };
    let h = ratios[2] / ratios[1] * span(&ranges[2]) * 0.5 * (1.0 - hole) / span(&ranges[1]);
    if !(h.is_finite() && h > 0.0) {
        return [1.0; 3];
    }
    let longest = h.max(1.0);
    [1.0 / longest, 1.0 / longest, h / longest]
}

/// Returns the relative lengths of the sides of the box of a 3D axes along x, y and z, the longest being 1.
///
/// An automatic aspect gives a cube; cylindrical axes take the sides of [`cylinder_sides`]. A fixed aspect gives sides proportional to the scale-space spans of the limits
/// scaled by the ratios, so that the box draws one unit along each axis at the aspect. Limits widened for `Limits`
/// already have equal scaled spans, so they give a cube to within rounding; limits that could not be widened give the
/// box that keeps the aspect instead.
pub(super) fn box_sides(axes: &Axes, ranges: &[Range; 3]) -> [f64; 3] {
    let Some(request) = request_of(axes) else {
        return [1.0; 3];
    };
    if let ironlab_ir::Projection::Cylindrical { polar, .. } = axes.projection {
        return cylinder_sides(polar.hole, request.ratios, ranges);
    }
    let scaled: [f64; 3] = std::array::from_fn(|k| span(&ranges[k]) * request.ratios[k]);
    let longest = scaled.iter().copied().fold(0.0, f64::max);
    if !(longest.is_finite() && longest > 0.0) || scaled.iter().any(|s| s.is_nan() || *s <= 0.0) {
        return [1.0; 3];
    }
    scaled.map(|s| s / longest)
}
