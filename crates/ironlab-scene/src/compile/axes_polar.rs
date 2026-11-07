//! Polar axes: margins, the fit of the circle or sector, grid, frame, tick labels, axis labels, title, artists and
//! legend.
//!
//! In a polar axes x carries the angle and y the radius. The x limits are the sector, `x.reverse` turns the angle
//! clockwise, and the radius runs from its lower limit at the centre (or at the edge of the hole) to its upper limit at
//! the rim, the other way round when `y.reverse` is set. The sector, between its edge angles and including the centre,
//! is scaled uniformly to fit the plot rectangle and centred in it, so a polar axes is always drawn round.

use std::f64::consts::TAU;

use ironlab_ir::{NodeId, PolarView, RadialAxis, RadialAxisEdge, RadialAxisSpokeSide};

use crate::display::{Item, ItemKind, Point, Rect};
use crate::hit::{AxesHit, AxesHitKind, AxisMap, HitMap};

use super::Ctx;
use super::artists::{AxesInput, Placement, Space, coalesce_markers, draw_artists, group_dense};
use super::axes2d::{clip_groups, frame_stroke, tick_length, title_left};
use super::decor::{AxisTicks, Decor};
use super::layout::{Margins, page_padding};
use super::limits::Range;
use super::paths::{self, PathBuilder};
use super::plane::{Plane, Warp, polar_point, unit_sector_box};
use super::style::{self, GRID, INK};
use super::text::TextBlock;
use super::{colorbar, legend};

/// Distances used to arrange the decorations of a polar axes, in points.
struct Spacing {
    /// Between the rim and the angular tick labels.
    tick_pad: f64,
    /// Between the outermost radial tick label, or the end of a radial axis on an edge, and the common exponent
    /// beyond it.
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
/// radial tick labels on a spoke lie inside the circle and take none. The x (angle) label lies below and the title
/// above, as in a 2D axes. The y (radius) label lies to the left when the radial tick labels are centred on a spoke, and
/// along the spoke when the radial axis is drawn along it, which takes room beyond the angular tick labels only where
/// the spoke lies on an edge of the box that bounds the sector, with its labels on the outside. A radial axis on an
/// edge takes the room of a Cartesian axis on that side, beyond the angular tick labels, and carries the y label with
/// it; on the bottom edge it lies between the angular tick labels and the x label.
pub(super) fn margins(ctx: &Ctx, decor: &Decor, polar: PolarView, x: Range) -> Margins {
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
    let mut m = Margins {
        left: s.outer + across,
        right: s.outer + across,
        top: s.outer + along + title,
        bottom: s.outer + along + label(0),
    };
    match polar.radial_axis {
        RadialAxis::Spoke { .. } => m.left += label(1),
        RadialAxis::AlongSpoke { .. } => {
            let [left, right, top, bottom] =
                spoke_overhang(ctx, decor, polar, x, longest_tick(ctx));
            m.left = m.left.max(s.outer + left);
            m.right = m.right.max(s.outer + right);
            m.top = m.top.max(s.outer + top + title);
            m.bottom = m.bottom.max(s.outer + bottom + label(0));
        }
        RadialAxis::Edge { edge } => {
            let yt = &decor.ticks[1];
            let longest = longest_tick(ctx);
            let bands = RulerBands::new(&s, yt, decor.labels[1].as_ref(), edge, longest);
            let ruler = s.label_gap + bands.total();
            let exponent = yt.exponent.as_ref();
            match edge {
                RadialAxisEdge::Bottom => m.bottom += ruler,
                RadialAxisEdge::Top => m.top += ruler,
                RadialAxisEdge::Left => m.left += ruler,
                RadialAxisEdge::Right => m.right += ruler,
            }
            // The tick label at the rim may overhang the end of the ruler by half its length along it, and the common
            // exponent lies beyond that end.
            match edge {
                RadialAxisEdge::Bottom | RadialAxisEdge::Top => {
                    let over = yt.labels.last().map_or(0.0, |l| l.width() / 2.0);
                    let beyond = exponent.map_or(0.0, |e| s.radial_pad + e.width());
                    m.right = m.right.max(s.outer + over.max(beyond));
                }
                RadialAxisEdge::Left | RadialAxisEdge::Right => {
                    let over = yt.labels.last().map_or(0.0, |l| l.total_height() / 2.0);
                    let beyond = exponent.map_or(0.0, |e| s.radial_pad + e.total_height());
                    m.top = m.top.max(s.outer + over.max(beyond) + title);
                }
            }
        }
    }
    m
}

/// Returns the longest that a tick mark of a radial axis can be, which the margins reserve because the plot rectangle,
/// on which the length depends, is not yet known.
fn longest_tick(ctx: &Ctx) -> f64 {
    0.5 * ctx.font_size
}

/// The depths across a radial axis on an edge of the parts that lie beyond its line, in points: the tick marks, the
/// tick labels with their pad, and the axis label with its gap. Each is 0 when the part is absent.
struct RulerBands {
    marks: f64,
    labels: f64,
    label: f64,
}

impl RulerBands {
    fn new(
        s: &Spacing,
        ticks: &AxisTicks,
        label: Option<&TextBlock>,
        edge: RadialAxisEdge,
        marks: f64,
    ) -> Self {
        let upright = matches!(edge, RadialAxisEdge::Left | RadialAxisEdge::Right);
        let labels = if ticks.labels.is_empty() {
            0.0
        } else if upright {
            s.tick_pad + ticks.max_width()
        } else {
            s.tick_pad + ticks.max_height() + ticks.max_depth()
        };
        Self {
            marks,
            labels,
            label: label.map_or(0.0, |l| s.label_gap + l.total_height()),
        }
    }

    /// The depth of the whole axis beyond its line.
    fn total(&self) -> f64 {
        self.marks + self.labels + self.label
    }
}

/// Returns the map from an angle in the data units of x to its page angle, in radians anticlockwise from east, for a
/// polar axes whose x axis is reversed when `reverse` is set.
fn page_angle(polar: PolarView, reverse: bool) -> impl Fn(f64) -> f64 {
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
    let sign = if reverse { -1.0 } else { 1.0 };
    move |theta: f64| zero + sign * theta * TAU / turn
}

/// Returns the page angles of the edges of the sector of a polar axes whose angular limits are `x`: the angle of the
/// lower limit and the angle of the upper limit, no more than a turn apart.
fn sector_angles(polar: PolarView, x: Range) -> (f64, f64) {
    let page_angle = page_angle(polar, x.reverse);
    let start = page_angle(x.min);
    let mut end = page_angle(x.max);
    // Limits spanning more than a turn, which validation refuses, are drawn as one turn.
    if (end - start).abs() > TAU {
        end = start + (end - start).signum() * TAU;
    }
    (start, end)
}

/// Returns the angle, in the data units of x, of the spoke on which the radial axis lies: the spoke the view names, or
/// the one halfway between the first two angular ticks, or, without two ticks, the one a sixteenth of the sector from
/// its lower limit.
fn spoke_angle(polar: PolarView, (min, max): (f64, f64), ticks: &AxisTicks) -> f64 {
    match (polar.radial_axis.angle(), ticks.major.as_slice()) {
        (Some(theta), _) if theta.is_finite() => theta,
        (_, [a, b, ..]) => (a + b) / 2.0,
        _ => min + (max - min) / 16.0,
    }
}

/// Returns the room, beyond the angular tick labels, that a radial axis along a spoke needs on each side of the plot,
/// as `[left, right, top, bottom]`, in points, when its tick marks are `length` long.
///
/// The axis needs room only where its spoke lies along an edge of the box that bounds the sector with its labels on
/// the outside, as along the diameter of a half disc or a straight edge of a quarter; elsewhere its labels lie over the
/// disc. The room is the depth of its tick marks, tick labels and axis label across the spoke.
fn spoke_overhang(ctx: &Ctx, decor: &Decor, polar: PolarView, x: Range, length: f64) -> [f64; 4] {
    let RadialAxis::AlongSpoke { side, .. } = polar.radial_axis else {
        return [0.0; 4];
    };
    let s = Spacing::new(ctx);
    let (start, end) = sector_angles(polar, x);
    let (bx0, bx1, by0, by1) = unit_sector_box(start.min(end), start.max(end));
    let psi = page_angle(polar, x.reverse)(spoke_angle(polar, (x.min, x.max), &decor.ticks[0]));
    // The direction of the spoke and the normal on its labelled side, with y upwards.
    let u = (psi.cos(), psi.sin());
    let n = match side {
        RadialAxisSpokeSide::Anticlockwise => (-psi.sin(), psi.cos()),
        RadialAxisSpokeSide::Clockwise => (psi.sin(), -psi.cos()),
    };
    let yt = &decor.ticks[1];
    let label = decor.labels[1]
        .as_ref()
        .map_or(0.0, |l| s.label_gap + l.total_height());
    // Each edge of the box: its outward direction, how far it lies from the centre in units of the rim, and the extent
    // of the tick labels across it.
    let edges = [
        ((-1.0, 0.0), -bx0, yt.max_width()),
        ((1.0, 0.0), bx1, yt.max_width()),
        ((0.0, 1.0), by1, yt.max_height() + yt.max_depth()),
        ((0.0, -1.0), -by0, yt.max_height() + yt.max_depth()),
    ];
    edges.map(|(e, b, extent): ((f64, f64), f64, f64)| {
        let along_edge = (u.0 * e.0 + u.1 * e.1).abs() < 1e-9 && b.abs() < 1e-9;
        let outwards = n.0 * e.0 + n.1 * e.1 > 0.5;
        if !(along_edge && outwards) {
            return 0.0;
        }
        let labels = if yt.labels.is_empty() {
            0.0
        } else {
            s.tick_pad + extent
        };
        length + labels + label
    })
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
    let (start, end) = sector_angles(polar, x);
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
    draw_labels(ctx, input, polar, &g, out);
    colorbar::draw_inside(ctx, axes, plot, input.scales, out, hits);
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

/// Returns the page angle of the spoke on which the radial axis lies, as [`spoke_angle`] chooses it.
fn radial_spoke(polar: PolarView, g: &Geometry, x: &AxisTicks) -> f64 {
    g.plane
        .x
        .to_figure(spoke_angle(polar, (g.plane.x.min, g.plane.x.max), x))
}

/// Returns the radius of each tick of `values` that lies between the edge of the hole and the rim, by more than a
/// rounding error, in points from the centre.
fn ruled_radii<'a>(g: &'a Geometry, values: &'a [f64]) -> impl Iterator<Item = f64> + 'a {
    values
        .iter()
        .map(|v| g.plane.y.to_figure(*v))
        .filter(|rho| rho.is_finite() && *rho >= g.hole - 1e-6 && *rho <= g.rim + 1e-6)
}

/// Returns how far a level box of the given width and height reaches across a line from its centre, in the direction
/// of the unit normal `n`.
fn half_across(n: Point, width: f64, height: f64) -> f64 {
    width / 2.0 * n.x.abs() + height / 2.0 * n.y.abs()
}

/// Draws the angular tick labels round the rim, the radial axis where the view puts it, the axis labels and the title.
fn draw_labels(
    ctx: &mut Ctx,
    input: &AxesInput,
    polar: PolarView,
    g: &Geometry,
    out: &mut Vec<Item>,
) {
    let s = Spacing::new(ctx);
    let axes = input.axes;
    let id = axes.id;
    let decor = input.decor;
    let xt = &decor.ticks[0];

    let (centres, drawn) = angular_label_centres(g, xt, s.tick_pad);
    let mut taken: Vec<Rect> = Vec::new();
    for ((label, centre), shown) in xt.labels.iter().zip(&centres).zip(&drawn) {
        if *shown {
            label.draw_on_page(ctx, label.origin_for_centre(*centre), INK, id, out);
            taken.push(centred_box(label, *centre));
        }
    }

    let (across, along) = label_band(xt, &s);
    let f = g.fitted;
    // How far below the fitted sector the x label starts, and how far above it the title ends.
    let mut below = along;
    let mut above = along;
    match polar.radial_axis {
        RadialAxis::Spoke { .. } => {
            draw_spoke_labels(ctx, input, polar, g, &taken, out);
            if let Some(label) = &decor.labels[1] {
                let right = f.x - across - s.label_gap;
                let origin = Point::new(
                    right - label.depth(),
                    f.y + (f.height + label.width()) / 2.0,
                );
                label.draw_upwards_on_page(ctx, origin, INK, id, out);
            }
        }
        RadialAxis::AlongSpoke { side, .. } => {
            draw_along_spoke(ctx, input, polar, side, g, &taken, out);
            let [_, _, top, bottom] =
                spoke_overhang(ctx, decor, polar, input.ranges[0], longest_tick(ctx));
            below = below.max(bottom);
            above = above.max(top);
        }
        RadialAxis::Edge { edge } => {
            let depth = draw_edge(ctx, input, edge, g, (across, along), out);
            match edge {
                RadialAxisEdge::Bottom => below = depth,
                RadialAxisEdge::Top => above = depth,
                RadialAxisEdge::Left | RadialAxisEdge::Right => {}
            }
        }
    }

    if let Some(label) = &decor.labels[0] {
        let origin = Point::new(
            f.x + (f.width - label.width()) / 2.0,
            f.bottom() + below + s.label_gap + label.height(),
        );
        label.draw_on_page(ctx, origin, INK, id, out);
    }
    if let Some(title) = &decor.title {
        let bottom = f.y - above - s.title_gap;
        let origin = Point::new(
            title_left(axes.title_align, f, title.width()),
            bottom - title.depth(),
        );
        title.draw_on_page(ctx, origin, INK, id, out);
    }
}

/// Draws the radial tick labels centred on their spoke, with the common exponent beyond the outermost of them.
///
/// The labels are centred on the spoke, so that the figure writer chooses where they lie by choosing the spoke, for
/// example in a gap between bars, as matplotlib's `set_rlabel_position` does.
fn draw_spoke_labels(
    ctx: &mut Ctx,
    input: &AxesInput,
    polar: PolarView,
    g: &Geometry,
    taken: &[Rect],
    out: &mut Vec<Item>,
) {
    let s = Spacing::new(ctx);
    let id = input.axes.id;
    let [xt, yt, _] = &input.decor.ticks;
    let psi = radial_spoke(polar, g, xt);
    let (ux, uy) = (psi.cos(), -psi.sin());
    let place = |rho: f64| Point::new(g.centre.x + rho * ux, g.centre.y + rho * uy);
    let mut outermost: Option<f64> = None;
    for (v, label) in yt.major.iter().zip(&yt.labels) {
        let rho = g.plane.y.to_figure(*v);
        if !rho.is_finite() || rho < 1e-6 {
            continue;
        }
        // A radial label that would cover an angular label, as one at the rim may, is left out.
        let centre = place(rho);
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
        let centre = place(along);
        e.draw_on_page(ctx, e.origin_for_centre(centre), INK, id, out);
    }
}

/// Draws a radial axis along its spoke: the axis line from the edge of the hole to the rim, a tick mark across it at
/// every radial tick pointing to `side`, the tick labels beyond the tick marks on that side, the common exponent beyond
/// the outermost label, and the y label beyond the tick labels, parallel to the spoke and turned so that it is never
/// upside down.
fn draw_along_spoke(
    ctx: &mut Ctx,
    input: &AxesInput,
    polar: PolarView,
    side: RadialAxisSpokeSide,
    g: &Geometry,
    taken: &[Rect],
    out: &mut Vec<Item>,
) {
    let s = Spacing::new(ctx);
    let id = input.axes.id;
    let [xt, yt, _] = &input.decor.ticks;
    let psi = radial_spoke(polar, g, xt);
    let u = Point::new(psi.cos(), -psi.sin());
    // A quarter turn of the page direction (cos ψ, −sin ψ) anticlockwise on the page is (−sin ψ, −cos ψ).
    let n = match side {
        RadialAxisSpokeSide::Anticlockwise => Point::new(-psi.sin(), -psi.cos()),
        RadialAxisSpokeSide::Clockwise => Point::new(psi.sin(), psi.cos()),
    };
    let at = |rho: f64, off: f64| {
        Point::new(
            g.centre.x + rho * u.x + off * n.x,
            g.centre.y + rho * u.y + off * n.y,
        )
    };
    let length = tick_length(ctx, input.plot);

    let mut b = PathBuilder::new();
    b.polyline(&[at(g.hole, 0.0), at(g.rim, 0.0)], false);
    for rho in ruled_radii(g, &yt.major) {
        b.polyline(&[at(rho, 0.0), at(rho, length)], false);
    }
    for rho in ruled_radii(g, &yt.minor) {
        b.polyline(&[at(rho, 0.0), at(rho, 0.6 * length)], false);
    }
    out.extend(paths::item(id, b.finish(), None, Some(frame_stroke())));

    let near = length + s.tick_pad;
    let mut outermost: Option<f64> = None;
    let mut farthest = near;
    for (v, label) in yt.major.iter().zip(&yt.labels) {
        let rho = g.plane.y.to_figure(*v);
        if !(rho.is_finite() && rho >= g.hole - 1e-6 && rho <= g.rim + 1e-6) {
            continue;
        }
        let half = half_across(n, label.width(), label.total_height());
        let centre = at(rho, near + half);
        // A radial label that would cover an angular label, as one at the rim may, is left out.
        let bounds = centred_box(label, centre);
        if taken.iter().any(|t| overlap(*t, bounds)) {
            continue;
        }
        label.draw_on_page(ctx, label.origin_for_centre(centre), INK, id, out);
        farthest = farthest.max(near + 2.0 * half);
        if outermost.is_none_or(|r| rho > r) {
            outermost = Some(rho);
        }
    }
    if let (Some(e), Some(rho)) = (&yt.exponent, outermost) {
        let along = rho + e.total_height().max(e.width()) * 0.75 + s.radial_pad;
        let centre = at(along, near + half_across(n, e.width(), e.total_height()));
        e.draw_on_page(ctx, e.origin_for_centre(centre), INK, id, out);
    }

    if let Some(label) = &input.decor.labels[1] {
        let mut degrees = psi.to_degrees().rem_euclid(360.0);
        if degrees > 90.0 && degrees <= 270.0 {
            degrees -= 180.0;
        } else if degrees > 270.0 {
            degrees -= 360.0;
        }
        let offset = farthest + s.label_gap + label.total_height() / 2.0;
        let centre = at((g.hole + g.rim) / 2.0, offset);
        let local = Point::new(label.width() / 2.0, (label.depth() - label.height()) / 2.0);
        let turned = TextBlock::turned_point(degrees, local);
        let origin = Point::new(centre.x - turned.x, centre.y - turned.y);
        label.draw_turned_on_page(ctx, origin, degrees, INK, id, out);
    }
}

/// Draws a radial axis on an edge of the plot, outside the disc and its angular tick labels, as a Cartesian axis on that
/// edge is drawn: the axis line from level with the edge of the hole to level with the rim, tick marks pointing away
/// from the disc, level tick labels beyond them, the common exponent beyond the end at the rim, and the y label beyond
/// the tick labels, level along a horizontal axis and reading upwards along a vertical one.
///
/// Returns how far beyond the fitted sector the axis reaches on its edge, so that the x label or the title can be set
/// beyond it.
fn draw_edge(
    ctx: &mut Ctx,
    input: &AxesInput,
    edge: RadialAxisEdge,
    g: &Geometry,
    (across, along): (f64, f64),
    out: &mut Vec<Item>,
) -> f64 {
    let s = Spacing::new(ctx);
    let id = input.axes.id;
    let yt = &input.decor.ticks[1];
    let length = tick_length(ctx, input.plot);
    let bands = RulerBands::new(&s, yt, input.decor.labels[1].as_ref(), edge, length);
    let f = g.fitted;
    // The start of the line, level with the edge of the hole; the direction in which the line runs out from it; and
    // the direction away from the disc.
    let (start, run, outward, band) = match edge {
        RadialAxisEdge::Bottom => (
            Point::new(g.centre.x + g.hole, f.bottom() + along + s.label_gap),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            along,
        ),
        RadialAxisEdge::Top => (
            Point::new(g.centre.x + g.hole, f.y - along - s.label_gap),
            Point::new(1.0, 0.0),
            Point::new(0.0, -1.0),
            along,
        ),
        RadialAxisEdge::Left => (
            Point::new(f.x - across - s.label_gap, g.centre.y - g.hole),
            Point::new(0.0, -1.0),
            Point::new(-1.0, 0.0),
            across,
        ),
        RadialAxisEdge::Right => (
            Point::new(f.right() + across + s.label_gap, g.centre.y - g.hole),
            Point::new(0.0, -1.0),
            Point::new(1.0, 0.0),
            across,
        ),
    };
    let at = |rho: f64, off: f64| {
        let d = rho - g.hole;
        Point::new(
            start.x + d * run.x + off * outward.x,
            start.y + d * run.y + off * outward.y,
        )
    };

    let mut b = PathBuilder::new();
    b.polyline(&[at(g.hole, 0.0), at(g.rim, 0.0)], false);
    for rho in ruled_radii(g, &yt.major) {
        b.polyline(&[at(rho, 0.0), at(rho, length)], false);
    }
    for rho in ruled_radii(g, &yt.minor) {
        b.polyline(&[at(rho, 0.0), at(rho, 0.6 * length)], false);
    }
    out.extend(paths::item(id, b.finish(), None, Some(frame_stroke())));

    // Level tick labels: a horizontal axis sets them on one baseline, as the x axis of a 2D axes does, and a vertical
    // axis aligns their edges nearer the line, as the y axis does.
    let near = length + s.tick_pad;
    for (v, label) in yt.major.iter().zip(&yt.labels) {
        let rho = g.plane.y.to_figure(*v);
        if !(rho.is_finite() && rho >= g.hole - 1e-6 && rho <= g.rim + 1e-6) {
            continue;
        }
        let tick = at(rho, near);
        let origin = match edge {
            RadialAxisEdge::Bottom => {
                Point::new(tick.x - label.width() / 2.0, tick.y + yt.max_height())
            }
            RadialAxisEdge::Top => {
                Point::new(tick.x - label.width() / 2.0, tick.y - yt.max_depth())
            }
            RadialAxisEdge::Left => {
                label.origin_for_centre(Point::new(tick.x - label.width() / 2.0, tick.y))
            }
            RadialAxisEdge::Right => {
                label.origin_for_centre(Point::new(tick.x + label.width() / 2.0, tick.y))
            }
        };
        label.draw_on_page(ctx, origin, INK, id, out);
    }
    if let Some(e) = &yt.exponent {
        let end = at(g.rim, 0.0);
        let centre = match edge {
            RadialAxisEdge::Bottom | RadialAxisEdge::Top => {
                let row = at(g.rim, near + bands.labels.max(e.total_height()) / 2.0);
                Point::new(end.x + s.radial_pad + e.width() / 2.0, row.y)
            }
            RadialAxisEdge::Left | RadialAxisEdge::Right => Point::new(
                at(g.rim, near + e.width() / 2.0).x,
                end.y - s.radial_pad - e.total_height() / 2.0,
            ),
        };
        e.draw_on_page(ctx, e.origin_for_centre(centre), INK, id, out);
    }

    if let Some(label) = &input.decor.labels[1] {
        let middle = at(
            (g.hole + g.rim) / 2.0,
            bands.marks + bands.labels + s.label_gap,
        );
        match edge {
            RadialAxisEdge::Bottom => {
                let origin = Point::new(middle.x - label.width() / 2.0, middle.y + label.height());
                label.draw_on_page(ctx, origin, INK, id, out);
            }
            RadialAxisEdge::Top => {
                let origin = Point::new(middle.x - label.width() / 2.0, middle.y - label.depth());
                label.draw_on_page(ctx, origin, INK, id, out);
            }
            RadialAxisEdge::Left => {
                let origin = Point::new(middle.x - label.depth(), middle.y + label.width() / 2.0);
                label.draw_upwards_on_page(ctx, origin, INK, id, out);
            }
            RadialAxisEdge::Right => {
                let origin = Point::new(middle.x + label.height(), middle.y + label.width() / 2.0);
                label.draw_upwards_on_page(ctx, origin, INK, id, out);
            }
        }
    }
    band + s.label_gap + bands.total()
}
