//! Axis limits, tick targets and colour limits.

use ironlab_ir::{Artist, Axes, Axis, ColorSpec, Limits, Projection, Scale, ScatterColor, Values};

use crate::display::Rect;
use crate::maths::ticks;

use super::Ctx;
use super::data::{ArtistData, ImageKind, Prepared, values_along};
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

/// The fraction of a tile's width that a 2D x axis is estimated to span, before decorations are measured.
const X_LENGTH_FRACTION: f64 = 0.85;
/// The fraction of a tile's height that a 2D y axis is estimated to span, before decorations are measured.
const Y_LENGTH_FRACTION: f64 = 0.8;
/// The smallest distance between neighbouring major ticks of a 2D x axis, in font sizes, before the labels are fitted.
const X_TICK_SPACING: f64 = 3.0;
/// The smallest distance between neighbouring major ticks of a 2D y axis, in font sizes.
///
/// Nice steps grow by factors of two to two and a half, so the resulting distance usually lies between this spacing
/// and two and a half times it, which averages about MATLAB's one label per three font sizes.
const Y_TICK_SPACING: f64 = 2.5;
/// The largest number of major tick intervals on a 2D axis.
const MAX_INTERVALS_2D: usize = 10;

/// Returns the estimated length in points of the x axis of a 2D axes in the tile `outer`.
pub(super) fn x_length_estimate(outer: Rect) -> f64 {
    X_LENGTH_FRACTION * outer.width
}

/// Chooses the largest number of major tick intervals for each axis of an axes, from the size of its tile.
///
/// The target depends only on the tile, not on measured decorations, so that it is known before limits are
/// computed. A 2D x axis allows one interval per [`X_TICK_SPACING`] font sizes of its estimated length and a 2D y
/// axis one per [`Y_TICK_SPACING`] font sizes, up to ten intervals, which gives MATLAB's density of labels; the x
/// target is then thinned by [`super::decor::fit_x_target`] until the x tick labels fit. A 3D axis allows one interval per six font sizes of the shorter tile side.
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
            count(
                x_length_estimate(outer),
                X_TICK_SPACING * fs,
                MAX_INTERVALS_2D,
            ),
            count(
                Y_LENGTH_FRACTION * outer.height,
                Y_TICK_SPACING * fs,
                MAX_INTERVALS_2D,
            ),
            5,
        ]
    }
}

/// The finite extents of the values that artists place along one axis.
#[derive(Clone, Copy, Debug, Default)]
struct Extents {
    /// The extent of gridded data (contours, surfaces and the pixel edges of images) along x or y, which takes
    /// tight limits.
    tight: Option<(f64, f64)>,
    /// The extent of all other data, which is rounded outward to major ticks.
    loose: Option<(f64, f64)>,
}

/// Returns the smallest interval covering two optional intervals.
fn union(a: Option<(f64, f64)>, b: Option<(f64, f64)>) -> Option<(f64, f64)> {
    match (a, b) {
        (Some((a0, a1)), Some((b0, b1))) => Some((a0.min(b0), a1.max(b1))),
        (a, None) => a,
        (None, b) => b,
    }
}

impl Extents {
    fn union(self, other: Extents) -> Extents {
        Extents {
            tight: union(self.tight, other.tight),
            loose: union(self.loose, other.loose),
        }
    }
}

/// Returns whether an artist's data along `dim` takes tight limits: the x and y values of contours and surfaces,
/// and the pixel edges of an image along the x or y axis of its plane (never its offset along the third axis).
fn is_tight(data: &ArtistData, dim: usize) -> bool {
    dim < 2
        && match data {
            ArtistData::Contour(_) | ArtistData::Surface { .. } => true,
            ArtistData::Image(image) => image.plane_dims().contains(&dim),
            _ => false,
        }
}

/// Returns the finite extents of the values an axes' artists place along `dim`, dropping non-positive values when
/// `log` is true.
fn extent(axes: &Axes, prepared: &[Prepared], dim: usize, log: bool) -> Extents {
    let three_d = is_3d(axes);
    let mut out = Extents::default();
    for p in prepared {
        if let Some(data) = &p.data {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            values_along(p.artist, data, three_d, dim, &mut |v| {
                if v.is_finite() && (!log || v > 0.0) {
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            });
            let found = (lo <= hi).then_some((lo, hi));
            let this = if is_tight(data, dim) {
                Extents {
                    tight: found,
                    loose: None,
                }
            } else {
                Extents {
                    tight: None,
                    loose: found,
                }
            };
            out = out.union(this);
        }
    }
    out
}

/// Computes the limits of every axis of every axes.
///
/// Manual limits are used as given when they are finite, increasing and (on a log axis) positive; otherwise a
/// warning names the axes and automatic limits are used. Automatic limits cover the data of every axes linked with
/// the axes along that dimension, rounded outward to major ticks with the smallest tick target in the group, except
/// that the x and y extents of gridded data are used exactly (see [`auto_range`]).
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
///
/// The data of the group is rounded outward to major ticks. When the group holds gridded data along the axis (the x
/// or y values of a contour or surface, or the pixel edges of an image along an axis of its plane), each end of the
/// range that no other data reaches beyond the grid is the exact end of the grid instead, unless the grid has no
/// extent along the axis.
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
    let mut extents = Extents::default();
    let mut target = usize::MAX;
    for member in figure.linked_axes(axes.id, dimension) {
        let Some(index) = figure.axes.iter().position(|a| a.id == member) else {
            continue;
        };
        target = target.min(targets[index][dim]);
        extents = extents.union(extent(&figure.axes[index], &prepared[index], dim, log));
    }
    let target = if target == usize::MAX { 5 } else { target };
    let fallback = if log {
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
    let Some((lo, hi)) = union(extents.tight, extents.loose) else {
        return fallback;
    };
    let (nice_min, nice_max) = if log {
        ticks::nice_log_limits(lo, hi)
    } else {
        ticks::nice_limits(lo, hi, target)
    };
    // A side of the range reached only by gridded data ends exactly at the grid, as MATLAB's contour and surf do;
    // a side that other data reaches beyond the grid is rounded outward to a major tick.
    let (min, max) = match extents.tight {
        Some((tight_lo, tight_hi)) if tight_lo < tight_hi => {
            let (loose_lo, loose_hi) = extents.loose.unwrap_or((tight_lo, tight_hi));
            (
                if loose_lo < tight_lo {
                    nice_min
                } else {
                    tight_lo
                },
                if loose_hi > tight_hi {
                    nice_max
                } else {
                    tight_hi
                },
            )
        }
        _ => (nice_min, nice_max),
    };
    if min.is_finite() && max.is_finite() && min < max {
        Range { min, max, log }
    } else {
        fallback
    }
}

/// Visits the values through which an artist is coloured by the colormap, which contribute to automatic colour
/// limits: the colour data of a scatter, the field of a colormapped contour or surface (its colour array when it
/// has one), and every value of a colour-mapped image, widened from 8 bits where its array holds bytes. The indices
/// of a colour-indexed image and the components of a true-colour image are not colour data and are not visited.
fn colour_values(artist: &Artist, data: &ArtistData, visit: &mut dyn FnMut(f64)) {
    let mapped = |spec: ColorSpec| matches!(spec, ColorSpec::Auto | ColorSpec::Colormapped);
    let values: &[f64] = match (artist, data) {
        (Artist::Scatter(s), ArtistData::Scatter { colours, .. }) => match (s.color, colours) {
            (ScatterColor::Data { .. }, Some(colours)) => colours,
            _ => return,
        },
        (Artist::Contour(c), ArtistData::Contour(grid)) if c.fill || mapped(c.line.color) => grid.z,
        (Artist::Surface(s), ArtistData::Surface { grid, colours })
            if mapped(s.face) || mapped(s.edge) =>
        {
            colours.unwrap_or(grid.z)
        }
        (_, ArtistData::Image(image)) if matches!(image.kind, ImageKind::Mapped(_)) => {
            match &image.array.values {
                Values::F64(values) => values,
                Values::U8(values) => {
                    values.iter().for_each(|&v| visit(f64::from(v)));
                    return;
                }
            }
        }
        _ => return,
    };
    values.iter().for_each(|&v| visit(v));
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
        colour_values(p.artist, data, &mut |v| {
            if v.is_finite() {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        });
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
