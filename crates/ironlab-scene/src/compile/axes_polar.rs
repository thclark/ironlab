//! Polar axes: margins, the fit of the circle or sector, grid, frame, tick labels, axis labels, title, artists and
//! legend.
//!
//! In a polar axes x carries the angle and y the radius. The x limits are the sector, `x.reverse` turns the angle
//! clockwise, and the radius runs from its lower limit at the centre (or at the edge of the hole) to its upper limit at
//! the rim, the other way round when `y.reverse` is set. The sector, between its edge angles and including the centre,
//! is scaled uniformly to fit the plot rectangle and centred in it, so a polar axes is always drawn round.

use std::f64::consts::TAU;

use ironlab_ir::{NodeId, PolarView};

use crate::display::{Item, ItemKind, Point, Rect};
use crate::hit::{AxesHit, AxesHitKind, AxisMap, HitMap};

use super::Ctx;
use super::artists::{AxesInput, Placement, Space, coalesce_markers, draw_artists, group_dense};
use super::axes2d::{clip_groups, frame_stroke, title_left};
use super::decor::{AxisTicks, Decor};
use super::layout::{Margins, page_padding};
use super::limits::Range;
use super::paths::{self, PathBuilder};
use super::plane::{Plane, Warp, polar_point, unit_sector_box};
use super::style::{self, GRID, INK};
use super::text::TextBlock;
use super::{colourbar, legend};

/// Distances used to arrange the decorations of a polar axes, in points.
struct Spacing {
    /// Between the rim and the angular tick labels.
    tick_pad: f64,
    /// Between the outermost radial tick label and the common exponent beyond it.
    radial_pad: f64,
    /// Between the angular tick labels and an axis label.
    label_gap: f64,
    /// Between the angular tick labels and the title.
    title_gap: f64,
    /// Around the outside of the axes.
    outer: f64,
}

impl Spacing {
    fn new(ctx: &Ctx) -> Self {
        let fs = ctx.font_size;
        Self {
            tick_pad: 0.4 * fs,
            radial_pad: 0.2 * fs,
            label_gap: 0.4 * fs,
            title_gap: 0.5 * fs,
            outer: page_padding(ctx),
        }
    }
}

/// Returns the room the angular tick labels take beyond the rim, across a vertical edge and across a horizontal edge
/// of the plot rectangle: nothing when there are none, and otherwise the pad and the largest width or height.
fn label_band(ticks: &AxisTicks, s: &Spacing) -> (f64, f64) {
    if ticks.labels.is_empty() {
        return (0.0, 0.0);
    }
    (
        s.tick_pad + ticks.max_width(),
        s.tick_pad + ticks.max_height() + ticks.max_depth(),
    )
}

/// Computes the space a polar axes needs around its plot rectangle.
///
/// The angular tick labels may lie on any side of the circle, so the room they take is reserved on all four sides;
/// the radial tick labels lie inside the circle and take none. The y (radius) label lies to the left, the x (angle)
/// label below and the title above, as in a 2D axes.
pub(super) fn margins(ctx: &Ctx, decor: &Decor) -> Margins {
    let s = Spacing::new(ctx);
    let (across, along) = label_band(&decor.ticks[0], &s);
    let label = |dim: usize| {
        decor.labels[dim]
            .as_ref()
            .map_or(0.0, |l| l.total_height() + s.label_gap)
    };
    let title = decor
        .title
        .as_ref()
        .map_or(0.0, |t| t.total_height() + s.title_gap);
    Margins {
        left: s.outer + across + label(1),
        right: s.outer + across,
        top: s.outer + along + title,
        bottom: s.outer + along + label(0),
    }
}

/// The geometry of a polar axes on the page.
struct Geometry {
    plane: Plane,
    centre: Point,
    rim: f64,
    hole: f64,
    /// The smaller and the larger page angle of the sector, in radians anticlockwise from east.
    angles: (f64, f64),
    /// Whether the sector is a full turn.
    full: bool,
    /// The bounding box of the sector on the page, centre included.
    fitted: Rect,
}

impl Geometry {
    /// Returns whether a page angle lies strictly inside the sector, by more than a rounding error; every angle does
    /// in a full turn.
    fn interior(&self, phi: f64) -> bool {
        if self.full {
            return true;
        }
        let (a0, a1) = self.angles;
        let t = (phi - a0).rem_euclid(TAU);
        t > 1e-9 && t < a1 - a0 - 1e-9
    }
}

/// Fits the sector of a polar axes into its plot rectangle and builds its maps.
fn geometry(polar: PolarView, x: Range, y: Range, plot: Rect) -> Geometry {
    let turn = polar.angle_units.turn();
    let turn = if turn.is_finite() && turn > 0.0 {
        turn
    } else {
        360.0
    };
    let zero = if polar.zero_deg.is_finite() {
        polar.zero_deg.to_radians()
    } else {
        0.0
    };
    let sign = if x.reverse { -1.0 } else { 1.0 };
    let page_angle = |theta: f64| zero + sign * theta * TAU / turn;
    let start = page_angle(x.min);
    let mut end = page_angle(x.max);
    // Limits spanning more than a turn, which validation refuses, are drawn as one turn.
    if (end - start).abs() > TAU {
        end = start + sign * TAU;
    }
    let full = (end - start).abs() >= TAU - 1e-9;
    let angles = (start.min(end), start.max(end));
    let (bx0, bx1, by0, by1) = unit_sector_box(angles.0, angles.1);
    let rim = (plot.width / (bx1 - bx0)).min(plot.height / (by1 - by0));
    let rim = if rim.is_finite() && rim > 0.0 {
        rim
    } else {
        0.0
    };
    let middle = Point::new(plot.x + plot.width / 2.0, plot.y + plot.height / 2.0);
    let centre = Point::new(
        middle.x - rim * (bx0 + bx1) / 2.0,
        middle.y + rim * (by0 + by1) / 2.0,
    );
    let fraction = if polar.hole.is_finite() {
        polar.hole.clamp(0.0, 0.99)
    } else {
        0.0
    };
    let hole = fraction * rim;
    let (r_start, r_end) = if y.reverse { (rim, hole) } else { (hole, rim) };
    let plane = Plane {
        x: AxisMap {
            min: x.min,
            max: x.max,
            log: false,
            start,
            end,
        },
        y: AxisMap {
            min: y.min,
            max: y.max,
            log: y.log,
            start: r_start,
            end: r_end,
        },
        warp: Warp::Polar { centre },
    };
    let fitted = Rect::new(
        centre.x + rim * bx0,
        centre.y - rim * by1,
        rim * (bx1 - bx0),
        rim * (by1 - by0),
    );
    Geometry {
        plane,
        centre,
        rim,
        hole,
        angles,
        full,
        fitted,
    }
}

/// Draws a polar axes and records its hit geometry.
pub(super) fn emit(
    ctx: &mut Ctx,
    input: &AxesInput,
    polar: PolarView,
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    let axes = input.axes;
    let plot = input.plot;
    let [xr, yr, _] = *input.ranges;
    let g = geometry(polar, xr, yr, plot);
    let space = Space::TwoD(g.plane);

    draw_grid(input, &g, out);

    let primaries = style::primaries(ctx.figure, axes);
    let drawn = draw_artists(ctx, input, &primaries, &space);
    hits.artists.extend(drawn.hits);
    hits.images.extend(drawn.images);
    hits.bars.extend(drawn.bars);
    hits.steps.extend(drawn.steps);
    hits.cells.extend(drawn.cells);
    let data: Vec<Item> = drawn.prims.into_iter().map(|(_, item)| item).collect();
    let mut data_points = Vec::new();
    paths::for_each_vertex(&data, &mut |p| data_points.push(p));
    let data = group_dense(coalesce_markers(data), &drawn.dense);
    let released: Vec<(NodeId, Rect)> = input
        .prepared
        .iter()
        .map(|p| {
            (
                p.artist.id(),
                Placement::of(p.artist).clip_rect(plot, input.page),
            )
        })
        .filter(|(_, clip)| *clip != plot)
        .collect();
    let groups = clip_groups(axes.id, plot, data, &released);
    out.extend(sector_clipped(axes.id, &g, plot, groups));

    draw_frame(input, &g, out);
    draw_labels(ctx, input, &g, out);
    colourbar::draw_inside(ctx, axes, plot, input.scales, out, hits);
    legend::draw_inside(ctx, input, &primaries, &data_points, out, hits);

    hits.axes.push(AxesHit {
        id: axes.id,
        plot_rect: plot,
        kind: AxesHitKind::Polar {
            centre: g.centre,
            x: g.plane.x,
            y: g.plane.y,
        },
    });
}

/// Wraps every maximal run of the groups clipped to the plot rectangle in one sector clip of the plot region, so that
/// the artists are clipped to the disc or sector rather than to the rectangle around it. A group whose clip is released
/// beyond the plot area, for a series placed in axes fractions without clipping, is left outside it.
fn sector_clipped(axes: NodeId, g: &Geometry, plot: Rect, groups: Vec<Item>) -> Vec<Item> {
    let Some(sector) = g.plane.sector().filter(|s| s.is_valid()) else {
        return groups;
    };
    let mut out = Vec::new();
    let mut run: Vec<Item> = Vec::new();
    let flush = |out: &mut Vec<Item>, run: &mut Vec<Item>| {
        if !run.is_empty() {
            out.push(Item {
                source: Some(axes),
                kind: ItemKind::SectorClip {
                    sector,
                    items: std::mem::take(run),
                },
            });
        }
    };
    for group in groups {
        let clipped_to_plot =
            matches!(group.kind, ItemKind::Group { clip: Some(c), .. } if c == plot);
        if clipped_to_plot {
            run.push(group);
        } else {
            flush(&mut out, &mut run);
            out.push(group);
        }
    }
    flush(&mut out, &mut run);
    out
}

/// Returns the vertices of the circle of radius `rho` about the centre, followed closely, without its first vertex
/// repeated.
fn circle(g: &Geometry, rho: f64) -> Vec<Point> {
    let a0 = g.angles.0;
    let mut points = g
        .plane
        .outline(&[Point::new(a0, rho), Point::new(a0 + TAU, rho)], false);
    points.pop();
    points
}

/// Returns the vertices of the arc of radius `rho` across the sector, followed closely.
fn arc(g: &Geometry, rho: f64) -> Vec<Point> {
    let (a0, a1) = g.angles;
    g.plane
        .outline(&[Point::new(a0, rho), Point::new(a1, rho)], false)
}

/// Draws the grid, under the artists: a spoke from the edge of the hole to the rim at every interior angular tick when the x grid is on,
/// and an arc across the sector at every radial tick strictly between the hole and the rim when the y grid is on, with
/// the stroke of the grid of a 2D axes.
fn draw_grid(input: &AxesInput, g: &Geometry, out: &mut Vec<Item>) {
    let axes = input.axes;
    let mut b = PathBuilder::new();
    if axes.x.grid {
        for tick in &input.decor.ticks[0].major {
            let phi = g.plane.x.to_figure(*tick);
            if phi.is_finite() && g.interior(phi) {
                b.polyline(
                    &[
                        polar_point(g.centre, phi, g.hole),
                        polar_point(g.centre, phi, g.rim),
                    ],
                    false,
                );
            }
        }
    }
    if axes.y.grid {
        for tick in &input.decor.ticks[1].major {
            let rho = g.plane.y.to_figure(*tick);
            if !(rho.is_finite() && rho > g.hole + 1e-6 && rho < g.rim - 1e-6) {
                continue;
            }
            if g.full {
                b.polyline(&circle(g, rho), true);
            } else {
                b.polyline(&arc(g, rho), false);
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

/// Draws the frame when the box is on: the rim, the edge of the hole when there is one, and the two radial edges of a
/// partial sector, with the stroke of the box of a 2D axes.
fn draw_frame(input: &AxesInput, g: &Geometry, out: &mut Vec<Item>) {
    if !input.axes.box_ || g.rim <= 0.0 {
        return;
    }
    let mut b = PathBuilder::new();
    if g.full {
        b.polyline(&circle(g, g.rim), true);
        if g.hole > 0.0 {
            b.polyline(&circle(g, g.hole), true);
        }
    } else {
        let (a0, a1) = g.angles;
        let corners = [
            Point::new(a0, g.rim),
            Point::new(a1, g.rim),
            Point::new(a1, g.hole),
            Point::new(a0, g.hole),
        ];
        b.polyline(&g.plane.outline(&corners, true), true);
    }
    out.extend(paths::item(
        input.axes.id,
        b.finish(),
        None,
        Some(frame_stroke()),
    ));
}

/// Returns the box of `label` with its centre at `centre`.
fn centred_box(label: &TextBlock, centre: Point) -> Rect {
    label.bounds(label.origin_for_centre(centre))
}

/// Returns whether two rectangles overlap.
fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// Returns the centres of the angular tick labels, outside the rim on the ray of each tick, each placed so that the
/// nearest point of its box to the centre lies `pad` beyond the rim; and which of them are drawn: all of them, or every
/// second, third and so on when neighbouring labels would overlap round the rim.
fn angular_label_centres(g: &Geometry, ticks: &AxisTicks, pad: f64) -> (Vec<Point>, Vec<bool>) {
    let centres: Vec<Point> = ticks
        .major
        .iter()
        .zip(&ticks.labels)
        .map(|(v, label)| {
            let phi = g.plane.x.to_figure(*v);
            let (c, s) = (phi.cos(), phi.sin());
            let d =
                g.rim + pad + label.width() / 2.0 * c.abs() + label.total_height() / 2.0 * s.abs();
            polar_point(g.centre, phi, d)
        })
        .collect();
    let boxes: Vec<Rect> = ticks
        .labels
        .iter()
        .zip(&centres)
        .map(|(label, c)| centred_box(label, *c))
        .collect();
    let n = boxes.len();
    for stride in 1..=n.max(1) {
        let kept: Vec<usize> = (0..n).filter(|k| k % stride == 0).collect();
        let clash = kept.windows(2).any(|w| overlap(boxes[w[0]], boxes[w[1]]))
            || (g.full
                && kept.len() > 2
                && overlap(
                    boxes[kept[0]],
                    boxes[*kept.last().expect("kept is not empty")],
                ));
        if !clash {
            return (centres, (0..n).map(|k| k % stride == 0).collect());
        }
    }
    (centres, (0..n).map(|k| k == 0).collect())
}

/// Returns the page angle of the spoke along which the radial tick labels are drawn: the spoke the view names, or the
/// one halfway between the first two angular ticks, or, without two ticks, the one a sixteenth of the sector from its
/// lower limit.
fn radial_spoke(polar: PolarView, g: &Geometry, x: &AxisTicks) -> f64 {
    let theta = match (polar.radial_axis_angle, x.major.as_slice()) {
        (Some(theta), _) if theta.is_finite() => theta,
        (_, [a, b, ..]) => (a + b) / 2.0,
        _ => g.plane.x.min + (g.plane.x.max - g.plane.x.min) / 16.0,
    };
    g.plane.x.to_figure(theta)
}

/// Draws the angular tick labels round the rim, the radial tick labels along their spoke, the axis labels and the
/// title.
fn draw_labels(ctx: &mut Ctx, input: &AxesInput, g: &Geometry, out: &mut Vec<Item>) {
    let s = Spacing::new(ctx);
    let axes = input.axes;
    let id = axes.id;
    let decor = input.decor;
    let [xt, yt, _] = &decor.ticks;
    let polar = match axes.projection {
        ironlab_ir::Projection::Polar { polar } => polar,
        _ => PolarView::default(),
    };

    let (centres, drawn) = angular_label_centres(g, xt, s.tick_pad);
    let mut taken: Vec<Rect> = Vec::new();
    for ((label, centre), shown) in xt.labels.iter().zip(&centres).zip(&drawn) {
        if *shown {
            label.draw_on_page(ctx, label.origin_for_centre(*centre), INK, id, out);
            taken.push(centred_box(label, *centre));
        }
    }

    // The radial labels are centred on their spoke, so that the figure writer chooses where they lie by choosing the
    // spoke, for example in a gap between bars, as matplotlib's `set_rlabel_position` does.
    let psi = radial_spoke(polar, g, xt);
    let (ux, uy) = (psi.cos(), -psi.sin());
    let place =
        |_label: &TextBlock, rho: f64| Point::new(g.centre.x + rho * ux, g.centre.y + rho * uy);
    let mut outermost: Option<f64> = None;
    for (v, label) in yt.major.iter().zip(&yt.labels) {
        let rho = g.plane.y.to_figure(*v);
        if !rho.is_finite() || rho < 1e-6 {
            continue;
        }
        // A radial label that would cover an angular label, as one at the rim may, is left out.
        let centre = place(label, rho);
        let bounds = centred_box(label, centre);
        if taken.iter().any(|t| overlap(*t, bounds)) {
            continue;
        }
        label.draw_on_page(ctx, label.origin_for_centre(centre), INK, id, out);
        if outermost.is_none_or(|r| rho > r) {
            outermost = Some(rho);
        }
    }
    if let (Some(e), Some(rho)) = (&yt.exponent, outermost) {
        let along = rho + e.total_height().max(e.width()) * 0.75 + s.radial_pad;
        let centre = place(e, along);
        e.draw_on_page(ctx, e.origin_for_centre(centre), INK, id, out);
    }

    let (across, along) = label_band(xt, &s);
    let f = g.fitted;
    if let Some(label) = &decor.labels[0] {
        let origin = Point::new(
            f.x + (f.width - label.width()) / 2.0,
            f.bottom() + along + s.label_gap + label.height(),
        );
        label.draw_on_page(ctx, origin, INK, id, out);
    }
    if let Some(label) = &decor.labels[1] {
        let right = f.x - across - s.label_gap;
        let origin = Point::new(
            right - label.depth(),
            f.y + (f.height + label.width()) / 2.0,
        );
        label.draw_upwards_on_page(ctx, origin, INK, id, out);
    }
    if let Some(title) = &decor.title {
        let bottom = f.y - along - s.title_gap;
        let origin = Point::new(
            title_left(axes.title_align, f, title.width()),
            bottom - title.depth(),
        );
        title.draw_on_page(ctx, origin, INK, id, out);
    }
}
