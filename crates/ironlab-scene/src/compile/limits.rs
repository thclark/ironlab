//! Axis limits, tick targets and colour limits.

use ironlab_ir::{Artist, Axes, Axis, ColorSpec, Limits, Projection, Scale, ScatterColor};

use crate::display::Rect;
use crate::maths::ticks;

use super::Ctx;
use super::data::{ArtistData, Prepared, values_along};
use super::style::{ColourScale, lut};

/// The limits of one data axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Range {
    pub min: f64,
    pub max: f64,
    pub log: bool,
}

/// Returns the coordinate axes of an axes in x, y, z order.
pub(super) fn axes_axis(axes: &Axes, dim: usize) -> &Axis {
    match dim {
        0 => &axes.x,
        1 => &axes.y,
        _ => &axes.z,
    }
}

/// Returns whether an axes is three-dimensional.
pub(super) fn is_3d(axes: &Axes) -> bool {
    matches!(axes.projection, Projection::ThreeD { .. })
}

/// Chooses the largest number of major tick intervals for each axis of an axes, from the size of its tile.
///
/// The target depends only on the tile, not on measured decorations, so that it is known before limits are
/// computed. A horizontal axis allows one interval per five font sizes of width and a vertical axis one per six and
/// a half font sizes of height, so that vertical axes favour coarse steps whose labels need no decimals; a 3D axis allows one per six font sizes of the shorter tile side.
pub(super) fn tick_targets(ctx: &Ctx, axes: &Axes, outer: Rect) -> [usize; 3] {
    let fs = ctx.font_size;
    let count = |len: f64, spacing: f64, max: usize| -> usize {
        let n = (len / spacing).floor();
        if n.is_finite() && n > 0.0 {
            (n as usize).clamp(3, max)
        } else {
            3
        }
    };
    if is_3d(axes) {
        let n = count(outer.width.min(outer.height), 6.0 * fs, 6);
        [n; 3]
    } else {
        [
            count(0.85 * outer.width, 5.0 * fs, 10),
            count(0.8 * outer.height, 6.5 * fs, 10),
            5,
        ]
    }
}

/// Returns the finite extent of the values an axes' artists place along `dim`, dropping non-positive values when
/// `log` is true.
fn extent(axes: &Axes, prepared: &[Prepared], dim: usize, log: bool) -> Option<(f64, f64)> {
    let three_d = is_3d(axes);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for p in prepared {
        if let Some(data) = &p.data {
            values_along(p.artist, data, three_d, dim, &mut |v| {
                if v.is_finite() && (!log || v > 0.0) {
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            });
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// Computes the limits of every axis of every axes.
///
/// Manual limits are used as given when they are finite, increasing and (on a log axis) positive; otherwise a
/// warning names the axes and automatic limits are used. Automatic limits cover the data of every axes linked with
/// the axes along that dimension, rounded outward to major ticks with the smallest tick target in the group.
pub(super) fn axis_ranges(
    ctx: &mut Ctx,
    prepared: &[Vec<Prepared>],
    targets: &[[usize; 3]],
) -> Vec<[Range; 3]> {
    let figure = ctx.figure;
    let mut out = Vec::with_capacity(figure.axes.len());
    for axes in &figure.axes {
        let ranges = std::array::from_fn(|dim| {
            let axis = axes_axis(axes, dim);
            let log = axis.scale == Scale::Log;
            if let Limits::Manual { min, max } = axis.limits {
                if min.is_finite() && max.is_finite() && min < max && (!log || min > 0.0) {
                    return Range { min, max, log };
                }
                if dim < 2 || is_3d(axes) {
                    ctx.warn(
                        Some(axes.id),
                        format!(
                            "The manual {} limits [{min}, {max}] are not usable, so automatic limits are used.",
                            ["x", "y", "z"][dim]
                        ),
                    );
                }
            }
            auto_range(figure, axes, prepared, targets, dim, log)
        });
        out.push(ranges);
    }
    out
}

/// Computes automatic limits for one axis over its link group.
fn auto_range(
    figure: &ironlab_ir::Figure,
    axes: &Axes,
    prepared: &[Vec<Prepared>],
    targets: &[[usize; 3]],
    dim: usize,
    log: bool,
) -> Range {
    let dimension = [
        ironlab_ir::Dimension::X,
        ironlab_ir::Dimension::Y,
        ironlab_ir::Dimension::Z,
    ][dim];
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut target = usize::MAX;
    for member in figure.linked_axes(axes.id, dimension) {
        let Some(index) = figure.axes.iter().position(|a| a.id == member) else {
            continue;
        };
        target = target.min(targets[index][dim]);
        if let Some((a, b)) = extent(&figure.axes[index], &prepared[index], dim, log) {
            lo = lo.min(a);
            hi = hi.max(b);
        }
    }
    let target = if target == usize::MAX { 5 } else { target };
    if lo > hi {
        return if log {
            Range {
                min: 1.0,
                max: 10.0,
                log,
            }
        } else {
            Range {
                min: 0.0,
                max: 1.0,
                log,
            }
        };
    }
    let (min, max) = if log {
        ticks::nice_log_limits(lo, hi)
    } else {
        ticks::nice_limits(lo, hi, target)
    };
    if min.is_finite() && max.is_finite() && min < max {
        Range { min, max, log }
    } else if log {
        Range {
            min: 1.0,
            max: 10.0,
            log,
        }
    } else {
        Range {
            min: 0.0,
            max: 1.0,
            log,
        }
    }
}

/// Returns whether an artist is coloured through the colormap and so contributes to automatic colour limits.
fn colour_values<'a>(artist: &Artist, data: &ArtistData<'a>) -> Option<&'a [f64]> {
    let mapped = |spec: ColorSpec| matches!(spec, ColorSpec::Auto | ColorSpec::Colormapped);
    match (artist, data) {
        (Artist::Scatter(s), ArtistData::Scatter { colours, .. }) => match s.color {
            ScatterColor::Data { .. } => *colours,
            ScatterColor::Spec { .. } => None,
        },
        (Artist::Contour(c), ArtistData::Contour(grid)) => {
            (c.fill || mapped(c.line.color)).then_some(grid.z)
        }
        (Artist::Surface(s), ArtistData::Surface { grid, colours }) => {
            (mapped(s.face) || mapped(s.edge)).then_some(colours.unwrap_or(grid.z))
        }
        _ => None,
    }
}

/// Computes the colour scale of an axes: its colormap and its manual or automatic colour limits.
pub(super) fn colour_scale(ctx: &mut Ctx, axes: &Axes, prepared: &[Prepared]) -> ColourScale {
    let lut = lut(axes.colormap);
    if let Limits::Manual { min, max } = axes.clim {
        if min.is_finite() && max.is_finite() && min < max {
            return ColourScale { lut, min, max };
        }
        ctx.warn(
            Some(axes.id),
            format!("The manual colour limits [{min}, {max}] are not usable, so automatic limits are used."),
        );
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for p in prepared {
        let Some(data) = &p.data else { continue };
        for v in colour_values(p.artist, data).unwrap_or(&[]) {
            if v.is_finite() {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
        }
    }
    if lo > hi {
        (lo, hi) = (0.0, 1.0);
    }
    ColourScale {
        lut,
        min: lo,
        max: hi,
    }
}
