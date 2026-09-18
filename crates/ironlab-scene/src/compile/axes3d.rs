//! Three-dimensional axes: the projected box, back-plane grid lines, tick labels on the outer edges, axis labels,
//! depth-sorted artists and the legend.

use ironlab_ir::View3d;

use crate::display::{Item, ItemKind, Point, Rect};
use crate::hit::{AxesHit, AxesHitKind, HitMap};
use crate::maths::camera::{Plane, UNIT_BOX_CORNERS, back_planes, depth_order};

use super::Ctx;
use super::artists::{AxesInput, Projector, Space, draw_artists, group_dense};
use super::decor::{AxisTicks, Decor};
use super::layout::{Margins, page_padding};
use super::legend;
use super::paths::{self, PathBuilder};
use super::style::{self, GRID, INK};
use super::text::TextBlock;

/// Computes the space a 3D axes reserves around the rectangle into which its box is fitted.
///
/// The box is fitted into the plot rectangle for every view, so tick labels and axis labels, which lie outside the
/// projected box, need room around it: the widest tick label and the rotated z label on the left, and a row of tick
/// labels and an axis label below.
pub(super) fn margins(ctx: &Ctx, decor: &Decor) -> Margins {
    let fs = ctx.font_size;
    let outer = page_padding(ctx);
    let widest = decor
        .ticks
        .iter()
        .map(AxisTicks::max_width)
        .fold(0.0, f64::max);
    let row = decor
        .ticks
        .iter()
        .map(|t| t.max_height() + t.max_depth())
        .fold(0.0, f64::max);
    let label_height = |dim: usize| {
        decor.labels[dim]
            .as_ref()
            .map_or(0.0, TextBlock::total_height)
    };
    let title = decor
        .title
        .as_ref()
        .map_or(0.0, |t| t.total_height() + 0.5 * fs);
    Margins {
        left: outer + widest + label_height(2) + 1.2 * fs,
        right: outer + widest + 0.5 * fs,
        top: outer + title + 0.5 * fs,
        bottom: outer + row + label_height(0).max(label_height(1)) + 1.2 * fs,
    }
}

/// Returns the axis and coordinate of a face of the unit box.
fn plane_position(plane: Plane) -> (usize, f64) {
    match plane {
        Plane::XMin => (0, -0.5),
        Plane::XMax => (0, 0.5),
        Plane::YMin => (1, -0.5),
        Plane::YMax => (1, 0.5),
        Plane::ZMin => (2, -0.5),
        Plane::ZMax => (2, 0.5),
    }
}

/// Returns the twelve edges of the unit box as pairs of corners.
fn box_edges() -> Vec<([f64; 3], [f64; 3])> {
    let mut edges = Vec::new();
    for (i, a) in UNIT_BOX_CORNERS.iter().enumerate() {
        for b in &UNIT_BOX_CORNERS[i + 1..] {
            if (0..3).filter(|&k| a[k] != b[k]).count() == 1 {
                edges.push((*a, *b));
            }
        }
    }
    edges
}

/// Draws a 3D axes and records its hit geometry.
pub(super) fn emit(
    ctx: &mut Ctx,
    input: &AxesInput,
    view: View3d,
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    let axes = input.axes;
    let projector = Projector::new(view, input.ranges, input.plot);
    let back: Vec<(usize, f64)> = back_planes(&projector.camera)
        .into_iter()
        .map(plane_position)
        .collect();
    let on_back_plane = |a: &[f64; 3], b: &[f64; 3]| {
        back.iter()
            .any(|&(k, value)| a[k] == value && b[k] == value)
    };

    let mut content = Vec::new();
    draw_grid(input, &projector, &back, &mut content);
    let mut back_edges = PathBuilder::new();
    let mut front_edges = PathBuilder::new();
    for (a, b) in box_edges() {
        let (Some((p, _)), Some((q, _))) = (projector.project(a), projector.project(b)) else {
            continue;
        };
        let target = if on_back_plane(&a, &b) {
            &mut back_edges
        } else {
            &mut front_edges
        };
        target.polyline(&[p, q], false);
    }
    content.extend(paths::item(
        axes.id,
        back_edges.finish(),
        None,
        Some(paths::solid(INK, 0.5)),
    ));

    let primaries = style::primaries(axes);
    let drawn = draw_artists(input, &primaries, &Space::ThreeD(&projector));
    hits.artists.extend(drawn.hits);
    let mut prims = drawn.prims;
    depth_order(&mut prims);
    let sorted: Vec<Item> = prims.into_iter().map(|(_, item)| item).collect();
    content.extend(group_dense(sorted, &drawn.dense));

    if axes.box_ {
        content.extend(paths::item(
            axes.id,
            front_edges.finish(),
            None,
            Some(paths::solid(INK, 0.5)),
        ));
    }
    out.push(Item {
        source: Some(axes.id),
        kind: ItemKind::Group {
            clip: Some(input.outer),
            transform: None,
            items: content,
        },
    });

    draw_labels(ctx, input, &projector, out);
    legend::draw(ctx, input, &primaries, &[], out, hits);
    hits.axes.push(AxesHit {
        id: axes.id,
        plot_rect: input.plot,
        kind: AxesHitKind::ThreeD,
    });
}

/// Returns the normalised position of a data value along axis `dim`, or `None` when it cannot be placed.
fn normalised_along(
    projector: &Projector,
    input: &AxesInput,
    dim: usize,
    value: f64,
) -> Option<f64> {
    let mut p = input.ranges.map(|r| r.min);
    p[dim] = value;
    let u = projector.normalise(p)[dim];
    u.is_finite().then_some(u)
}

/// Draws the grid lines of each axis with its grid enabled on the two back planes that contain its direction.
fn draw_grid(input: &AxesInput, projector: &Projector, back: &[(usize, f64)], out: &mut Vec<Item>) {
    let axes = input.axes;
    let grids = [axes.x.grid, axes.y.grid, axes.z.grid];
    let mut b = PathBuilder::new();
    for dim in (0..3).filter(|d| grids[*d]) {
        for tick in &input.decor.ticks[dim].major {
            let Some(u) = normalised_along(projector, input, dim, *tick) else {
                continue;
            };
            if u.abs() > 0.5 + 1e-9 || (u.abs() - 0.5).abs() < 1e-9 {
                continue;
            }
            for &(plane_axis, plane_value) in back.iter().filter(|(k, _)| *k != dim) {
                let span = 3 - dim - plane_axis;
                let mut start = [0.0; 3];
                start[dim] = u;
                start[plane_axis] = plane_value;
                start[span] = -0.5;
                let mut end = start;
                end[span] = 0.5;
                if let (Some((p, _)), Some((q, _))) =
                    (projector.project(start), projector.project(end))
                {
                    b.polyline(&[p, q], false);
                }
            }
        }
    }
    out.extend(paths::item(
        axes.id,
        b.finish(),
        None,
        Some(paths::solid(GRID, 0.5)),
    ));
}

/// Normalises a figure-space vector, or returns `fallback` when it has no usable length.
fn unit(v: Point, fallback: Point) -> Point {
    let len = v.x.hypot(v.y);
    if len.is_finite() && len > 1e-9 {
        Point::new(v.x / len, v.y / len)
    } else {
        fallback
    }
}

/// The extent of a text box of `width` × `height` along the unit direction `d`, measured from its centre.
fn half_extent(d: Point, width: f64, height: f64) -> f64 {
    (d.x.abs() * width + d.y.abs() * height) / 2.0
}

/// The edge of the box that carries an axis's tick labels, with the screen direction pointing away from the box.
struct LabelEdge {
    /// The normalised coordinates of the edge; the coordinate along the axis is ignored.
    at: [f64; 3],
    /// The figure-space midpoint of the edge.
    mid: Point,
    /// The unit figure-space direction from the edge away from the box.
    outward: Point,
    /// The unit figure-space direction in which the axis values increase along the edge.
    along: Point,
}

/// Chooses the edge that carries the tick labels of axis `dim`.
///
/// For x and y, it is the edge parallel to the axis that is lowest on the page; for z, it is the vertical edge
/// furthest to the left. Ties are broken in favour of the edge nearer the viewer.
fn label_edge(projector: &Projector, dim: usize) -> Option<LabelEdge> {
    let others: Vec<usize> = (0..3).filter(|k| *k != dim).collect();
    let mut best: Option<(LabelEdge, f64, f64)> = None;
    for (sa, sb) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
        let mut at = [0.0; 3];
        at[others[0]] = sa;
        at[others[1]] = sb;
        let (mid, depth) = projector.project(at)?;
        let (mut lo, mut hi) = (at, at);
        lo[dim] = -0.5;
        hi[dim] = 0.5;
        let (p_lo, _) = projector.project(lo)?;
        let (p_hi, _) = projector.project(hi)?;
        let (centre, _) = projector.project([0.0; 3])?;
        let along = unit(
            Point::new(p_hi.x - p_lo.x, p_hi.y - p_lo.y),
            Point::new(1.0, 0.0),
        );
        let mut away = Point::new(mid.x - centre.x, mid.y - centre.y);
        let dot = away.x * along.x + away.y * along.y;
        away = Point::new(away.x - dot * along.x, away.y - dot * along.y);
        let fallback = if dim == 2 {
            Point::new(-1.0, 0.0)
        } else {
            Point::new(0.0, 1.0)
        };
        let outward = unit(away, fallback);
        let score = if dim == 2 { -mid.x } else { mid.y };
        let better = match &best {
            None => true,
            Some((_, s, d)) => score > s + 1e-6 || ((score - s).abs() <= 1e-6 && depth > *d),
        };
        if better {
            best = Some((
                LabelEdge {
                    at,
                    mid,
                    outward,
                    along,
                },
                score,
                depth,
            ));
        }
    }
    best.map(|(edge, _, _)| edge)
}

/// The length of a 3D tick mark, in font sizes.
const TICK_LENGTH: f64 = 0.35;
/// The smallest clear gap between two tick labels of a 3D axes, in font sizes.
const LABEL_CLEARANCE: f64 = 0.3;

/// Returns whether two rectangles come closer than `clearance` along both axes.
fn crowds(a: Rect, b: Rect, clearance: f64) -> bool {
    a.x < b.right() + clearance
        && b.x < a.right() + clearance
        && a.y < b.bottom() + clearance
        && b.y < a.bottom() + clearance
}

/// Draws tick marks, tick labels, common exponent labels, axis labels and the title of a 3D axes.
///
/// Each labelled edge carries a tick mark at every major tick, pointing away from the box, and each label lies beyond
/// its tick mark. Labels are placed axis by axis in x, y, z order, and a label that would come closer than
/// [`LABEL_CLEARANCE`] font sizes to a label already placed (of any axis) is left out, so where rows of labels meet at
/// a corner of the box the earlier axis keeps its label.
fn draw_labels(ctx: &Ctx, input: &AxesInput, projector: &Projector, out: &mut Vec<Item>) {
    let fs = ctx.font_size;
    let tick_length = TICK_LENGTH * fs;
    let pad = tick_length + 0.3 * fs;
    let gap = 0.4 * fs;
    let clearance = LABEL_CLEARANCE * fs;
    let (id, decor) = (input.axes.id, input.decor);
    let mut marks = PathBuilder::new();
    let mut placed: Vec<Rect> = Vec::new();
    for dim in 0..3 {
        let Some(edge) = label_edge(projector, dim) else {
            continue;
        };
        let ticks = &decor.ticks[dim];
        let d = edge.outward;
        let first_box = placed.len();
        let mut last_centre = None;
        for (value, label) in ticks.major.iter().zip(&ticks.labels) {
            let Some(u) = normalised_along(projector, input, dim, *value) else {
                continue;
            };
            let mut at = edge.at;
            at[dim] = u;
            let Some((q, _)) = projector.project(at) else {
                continue;
            };
            marks.polyline(
                &[
                    q,
                    Point::new(q.x + d.x * tick_length, q.y + d.y * tick_length),
                ],
                false,
            );
            let offset = pad + half_extent(d, label.width(), label.total_height());
            let centre = Point::new(q.x + d.x * offset, q.y + d.y * offset);
            let origin = label.origin_for_centre(centre);
            last_centre = Some((centre, label));
            let bounds = label.bounds(origin);
            if placed.iter().any(|p| crowds(*p, bounds, clearance)) {
                continue;
            }
            label.draw(origin, INK, id, out);
            placed.push(bounds);
        }
        let boxes = &placed[first_box..];
        if let (Some(e), Some((centre, last))) = (&ticks.exponent, last_centre) {
            let a = edge.along;
            let shift = half_extent(a, last.width(), last.total_height())
                + gap
                + half_extent(a, e.width(), e.total_height());
            let c = Point::new(centre.x + a.x * shift, centre.y + a.y * shift);
            e.draw(e.origin_for_centre(c), INK, id, out);
        }
        let Some(label) = &decor.labels[dim] else {
            continue;
        };
        if dim == 2 {
            let right = boxes.iter().map(|b| b.x).fold(edge.mid.x - pad, f64::min) - gap;
            let origin = Point::new(right - label.depth(), edge.mid.y + label.width() / 2.0);
            label.draw_upwards(origin, INK, id, out);
        } else {
            let tick_extent = ticks
                .labels
                .iter()
                .map(|l| 2.0 * half_extent(d, l.width(), l.total_height()))
                .fold(0.0, f64::max);
            let offset =
                pad + tick_extent + gap + half_extent(d, label.width(), label.total_height());
            let centre = Point::new(edge.mid.x + d.x * offset, edge.mid.y + d.y * offset);
            label.draw(label.origin_for_centre(centre), INK, id, out);
        }
    }
    out.extend(paths::item(
        id,
        marks.finish(),
        None,
        Some(paths::solid(INK, 0.5)),
    ));
    if let Some(title) = &decor.title {
        let plot = input.plot;
        let origin = Point::new(
            plot.x + (plot.width - title.width()) / 2.0,
            plot.y - 0.5 * fs - title.depth(),
        );
        title.draw(origin, INK, id, out);
    }
}
