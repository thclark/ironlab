//! Two-dimensional axes: margins, grid, box, ticks, labels, title, artists and legend.

use ironlab_ir::{Axes, AxisSide, NodeId, TitleAlign};

use crate::display::{Item, ItemKind, Point, Rect, Stroke};
use crate::hit::{AxesHit, AxesHitKind, AxisMap, HitMap};

use super::Ctx;
use super::artists::{AxesInput, Placement, Space, coalesce_markers, draw_artists, group_dense};
use super::decor::{AxisTicks, Decor};
use super::layout::{Margins, page_padding};
use super::paths::{self, PathBuilder};
use super::plane::Plane;
use super::style::{self, GRID, INK};
use super::text::TextBlock;
use super::{colorbar, legend, limits::Range};

/// The width of the axes box and tick marks, in points.
const FRAME_WIDTH: f64 = 0.5;

/// Returns the stroke of the axes box and tick marks.
pub(super) fn frame_stroke() -> Stroke {
    paths::solid(INK, FRAME_WIDTH)
}

/// Returns the outline of `rect` drawn as the box of an axes is drawn, with no fill.
///
/// Why: a legend panel draws its border with this function, so that the border and the boxes of the axes beside it
/// are the same lines, laid on the same edges of the aligned layout.
pub(super) fn frame(source: NodeId, rect: Rect) -> Option<Item> {
    let mut b = PathBuilder::new();
    b.rect(rect);
    paths::item(source, b.finish(), None, Some(frame_stroke()))
}

/// Distances used to arrange the decorations of a 2D axes, in points.
struct Spacing {
    /// Between the plot edge and the tick labels.
    tick_pad: f64,
    /// Between the tick labels and the axis label.
    label_gap: f64,
    /// Between the plot top (or the exponent label above it) and the title.
    title_gap: f64,
    /// Between the plot edge and a common exponent label.
    exponent_gap: f64,
    /// Around the outside of the axes.
    outer: f64,
}

impl Spacing {
    fn new(ctx: &Ctx) -> Self {
        let fs = ctx.font_size;
        Self {
            tick_pad: 0.4 * fs,
            label_gap: 0.4 * fs,
            title_gap: 0.5 * fs,
            exponent_gap: 0.2 * fs,
            outer: page_padding(ctx),
        }
    }
}

/// The height of the row of x tick labels.
fn x_row_height(ticks: &AxisTicks) -> f64 {
    ticks.max_height() + ticks.max_depth()
}

/// The side of its line on which an axis of a 2D axes sets its tick labels and label.
///
/// An axis on the low side or through the middle sets them below its line (x) or left of it (y), and an axis on the
/// high side above it (x) or right of it (y), so that the labels of an axis on an edge lie outside the plot area.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    /// Below the line, where an x axis on the low side or through the middle sets its labels.
    Bottom,
    /// Above the line, where an x axis on the high side sets its labels.
    Top,
    /// Left of the line, where a y axis on the low side or through the middle sets its labels.
    Left,
    /// Right of the line, where a y axis on the high side sets its labels.
    Right,
}

impl Side {
    /// The side on which an x axis on `side` sets its labels.
    fn of_x(side: AxisSide) -> Self {
        match side {
            AxisSide::Low | AxisSide::Centre => Side::Bottom,
            AxisSide::High => Side::Top,
        }
    }

    /// The side on which a y axis on `side` sets its labels.
    fn of_y(side: AxisSide) -> Self {
        match side {
            AxisSide::Low | AxisSide::Centre => Side::Left,
            AxisSide::High => Side::Right,
        }
    }

    /// The unit page direction across the axis, away from its line towards its labels.
    fn outward(self) -> Point {
        match self {
            Side::Bottom => Point::new(0.0, 1.0),
            Side::Top => Point::new(0.0, -1.0),
            Side::Left => Point::new(-1.0, 0.0),
            Side::Right => Point::new(1.0, 0.0),
        }
    }

    /// The unit page direction along the axis.
    fn along(self) -> Point {
        match self {
            Side::Bottom | Side::Top => Point::new(1.0, 0.0),
            Side::Left | Side::Right => Point::new(0.0, 1.0),
        }
    }
}

/// Returns the page y of the line of an x axis on `side`: the bottom edge of the plot area, its middle or its top
/// edge. The side is a side of the page, so a reversed y axis does not move it.
fn x_line(side: AxisSide, plot: Rect) -> f64 {
    match side {
        AxisSide::Low => plot.bottom(),
        AxisSide::Centre => plot.y + plot.height / 2.0,
        AxisSide::High => plot.y,
    }
}

/// Returns the page x of the line of a y axis on `side`: the left edge of the plot area, its middle or its right edge.
fn y_line(side: AxisSide, plot: Rect) -> f64 {
    match side {
        AxisSide::Low => plot.x,
        AxisSide::Centre => plot.x + plot.width / 2.0,
        AxisSide::High => plot.right(),
    }
}

fn dot(a: Point, b: Point) -> f64 {
    a.x * b.x + a.y * b.y
}

/// Where a tick label turned by an angle other than 0 lies, as offsets from its tick on the edge of the plot area.
struct Turned {
    /// The offset of the baseline origin of the label from its tick.
    origin: Point,
    /// The offset of the box that the turned label covers from its tick.
    bounds: Rect,
}

/// Places a tick label turned `angle` degrees anticlockwise (normalised, and not 0) on `side`, `pad` away from the
/// plot area.
///
/// The label is turned about a point of its own box, its anchor, which is placed on the line through the tick across
/// the axis:
///
/// - For an angle of 180 the text is level but upside down, so neither end is nearer the tick and the anchor is the
///   centre of the box: the label is centred on its tick, as a level label is.
/// - For any other angle the anchor is the middle of the text's thickness at the end of the text that, once turned,
///   lies nearer the plot area, so that the label points at its tick. On the bottom axis that is the end of the text
///   for a positive angle and its start for a negative one; on the left axis it is the end of the text for angles
///   from −90 to 90 and its start beyond them. Where both ends are equally near (±90 on the left axis), the end of
///   the text is taken.
///
/// Across the axis the label is then moved away from the plot area just far enough that the nearest point of its
/// turned box lies `pad` from the edge, so that a turned label never comes closer to the plot than a level one.
fn turned_label(label: &TextBlock, angle: f64, side: Side, pad: f64) -> Turned {
    let (n, t) = (side.outward(), side.along());
    let turn = |p: Point| TextBlock::turned_point(angle, p);
    let middle = (label.depth() - label.height()) / 2.0;
    let anchor = if angle == 180.0 {
        Point::new(label.width() / 2.0, middle)
    } else {
        let (start, end) = (Point::new(0.0, middle), Point::new(label.width(), middle));
        if dot(n, turn(end)) <= dot(n, turn(start)) + 1e-9 {
            end
        } else {
            start
        }
    };
    let corners = label.corners().map(turn);
    let nearest = corners
        .iter()
        .map(|c| dot(n, *c))
        .fold(f64::INFINITY, f64::min);
    let (shift_along, shift_out) = (-dot(t, turn(anchor)), pad - nearest);
    let origin = Point::new(
        shift_along * t.x + shift_out * n.x,
        shift_along * t.y + shift_out * n.y,
    );
    let b = label.turned_bounds(angle);
    Turned {
        origin,
        bounds: Rect::new(origin.x + b.x, origin.y + b.y, b.width, b.height),
    }
}

/// How far the turned tick labels of an axis reach from their ticks, in points, each distance at least 0.
struct Reach {
    /// Away from the plot area, the gap from the plot included.
    out: f64,
    /// Along the axis towards the start of the page (left for the x axis, up for the y axis).
    before: f64,
    /// Along the axis towards the end of the page (right for the x axis, down for the y axis).
    after: f64,
}

/// Returns how far the tick labels of `ticks` reach when turned `angle` degrees on `side`, or `None` when they are
/// level (an angle of 0) or there are none. Any label may lie at either end of the axis, so each distance is the
/// largest over the labels.
fn turned_reach(ticks: &AxisTicks, angle: f64, side: Side, pad: f64) -> Option<Reach> {
    if angle == 0.0 || ticks.labels.is_empty() {
        return None;
    }
    let mut reach = Reach {
        out: 0.0,
        before: 0.0,
        after: 0.0,
    };
    for label in &ticks.labels {
        let b = turned_label(label, angle, side, pad).bounds;
        let (out, before, after) = match side {
            Side::Bottom => (b.bottom(), -b.x, b.right()),
            Side::Top => (-b.y, -b.x, b.right()),
            Side::Left => (-b.x, -b.y, b.bottom()),
            Side::Right => (b.right(), -b.y, b.bottom()),
        };
        reach.out = reach.out.max(out);
        reach.before = reach.before.max(before);
        reach.after = reach.after.max(after);
    }
    Some(reach)
}

/// The distance across an x axis from its line to the far edge of its tick labels, or 0 without tick labels.
fn x_tick_band(ticks: &AxisTicks, reach: Option<&Reach>, s: &Spacing) -> f64 {
    match reach {
        Some(r) => r.out,
        None if ticks.labels.is_empty() => 0.0,
        None => s.tick_pad + x_row_height(ticks),
    }
}

/// The distance across an x axis from its line to the far edge of its tick labels and label, or of its common
/// exponent label beside them, whichever lies further.
fn x_band(axes: &Axes, decor: &Decor, s: &Spacing) -> f64 {
    let x = &decor.ticks[0];
    let reach = turned_reach(x, decor.angles[0], Side::of_x(axes.x.side), s.tick_pad);
    let xlabel = decor.labels[0]
        .as_ref()
        .map_or(0.0, |l| l.total_height() + s.label_gap);
    let exponent_row = x
        .exponent
        .as_ref()
        .map_or(0.0, |e| e.total_height() + s.exponent_gap);
    x_tick_band(x, reach.as_ref(), s) + xlabel.max(exponent_row)
}

/// How far the y tick labels reach along the axis beyond the first tick (up the page) and the last (down the page).
fn y_overhang(axes: &Axes, decor: &Decor, s: &Spacing) -> (f64, f64) {
    let y = &decor.ticks[1];
    match turned_reach(y, decor.angles[1], Side::of_y(axes.y.side), s.tick_pad) {
        Some(reach) => (reach.before, reach.after),
        None => {
            let half = (y.max_height() + y.max_depth()) / 2.0;
            (half, half)
        }
    }
}

/// The height of the band above the plot that holds the y exponent label and the overhang of the top y tick label,
/// and, for an x axis on the high side, its tick labels and label, beneath the y exponent label.
fn top_band(axes: &Axes, decor: &Decor, s: &Spacing) -> f64 {
    let y = &decor.ticks[1];
    let (overhang, _) = y_overhang(axes, decor, s);
    let exponent = y
        .exponent
        .as_ref()
        .map_or(0.0, |e| e.total_height() + s.exponent_gap);
    match axes.x.side {
        AxisSide::High => overhang.max(x_band(axes, decor, s) + exponent),
        AxisSide::Low | AxisSide::Centre => overhang.max(exponent),
    }
}

/// Computes the space a 2D axes needs around its plot rectangle.
///
/// Level tick labels take the space they always have. Turned tick labels take the extent of their turned boxes
/// across the axis, and along it the overhang of any label beyond its tick, since any of them may lie at an end of
/// the axis; turned y labels may also overhang below the plot area.
///
/// The band of an axis is reserved on the side it is drawn on: below (x) or left of (y) the plot area for the low
/// side, above or right of it for the high side, and nowhere for an axis through the middle, whose labels lie inside
/// the plot area. The overhang of the end labels along an axis is reserved whatever its side.
pub(super) fn margins(ctx: &Ctx, axes: &Axes, decor: &Decor) -> Margins {
    let s = Spacing::new(ctx);
    let [x, y, _] = &decor.ticks;
    let x_reach = turned_reach(x, decor.angles[0], Side::of_x(axes.x.side), s.tick_pad);
    let y_reach = turned_reach(y, decor.angles[1], Side::of_y(axes.y.side), s.tick_pad);
    let (first_x_half, last_x_half) = match &x_reach {
        Some(r) => (r.before, r.after),
        None => (
            x.labels.first().map_or(0.0, |l| l.width() / 2.0),
            x.labels.last().map_or(0.0, |l| l.width() / 2.0),
        ),
    };
    let ylabel = decor.labels[1]
        .as_ref()
        .map_or(0.0, |l| l.total_height() + s.label_gap);
    let xlabel = decor.labels[0]
        .as_ref()
        .map_or(0.0, |l| l.total_height() + s.label_gap);
    let exponent_row = x
        .exponent
        .as_ref()
        .map_or(0.0, |e| e.total_height() + s.exponent_gap);
    let title = decor
        .title
        .as_ref()
        .map_or(0.0, |t| t.total_height() + s.title_gap);
    let y_labels = match &y_reach {
        Some(r) => r.out,
        None if y.labels.is_empty() => 0.0,
        None => s.tick_pad + y.max_width(),
    };
    let x_labels = match &x_reach {
        Some(r) => r.out,
        None if x.labels.is_empty() => 0.0,
        None => s.tick_pad + x_row_height(x),
    };
    let right_exponent = x
        .exponent
        .as_ref()
        .map_or(0.0, |e| s.exponent_gap + e.width());
    let below = match axes.x.side {
        AxisSide::Low => {
            let below = x_labels + xlabel.max(exponent_row);
            y_reach.as_ref().map_or(below, |r| below.max(r.after))
        }
        AxisSide::Centre | AxisSide::High => y_overhang(axes, decor, &s).1,
    };
    let left = match axes.y.side {
        AxisSide::Low => (y_labels + ylabel).max(first_x_half),
        AxisSide::Centre | AxisSide::High => first_x_half,
    };
    let right = match axes.y.side {
        AxisSide::High => (y_labels + ylabel).max(last_x_half.max(right_exponent)),
        AxisSide::Low | AxisSide::Centre => last_x_half.max(right_exponent),
    };
    Margins {
        left: s.outer + left,
        right: s.outer + right,
        top: s.outer + top_band(axes, decor, &s) + title,
        bottom: s.outer + below,
    }
}

/// Draws a 2D axes and records its hit geometry.
pub(super) fn emit(ctx: &mut Ctx, input: &AxesInput, out: &mut Vec<Item>, hits: &mut HitMap) {
    let axes = input.axes;
    let plot = input.plot;
    let [xr, yr, _] = *input.ranges;
    let x = axis_map(xr, plot.x, plot.right());
    let y = axis_map(yr, plot.bottom(), plot.y);
    let space = Space::TwoD(Plane::cartesian(x, y));
    let decor = input.decor;

    draw_grid(input, &x, &y, out);

    let primaries = style::primaries(ctx.figure, axes);
    let drawn = draw_artists(ctx, input, &primaries, &space);
    hits.artists.extend(drawn.hits);
    hits.images.extend(drawn.images);
    hits.bars.extend(drawn.bars);
    hits.steps.extend(drawn.steps);
    hits.cells.extend(drawn.cells);
    let data: Vec<Item> = drawn.prims.into_iter().map(|(_, item)| item).collect();
    // The vertices of the drawn data, the corners of images included, which the `best` legend location avoids.
    let mut data_points = Vec::new();
    paths::for_each_vertex(&data, &mut |p| data_points.push(p));
    let data = group_dense(coalesce_markers(data), &drawn.dense);
    // The clip of every artist whose clip differs from the plot area: one that releases it along a fraction dimension.
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
    out.extend(clip_groups(axes.id, plot, data, &released));

    draw_box_and_ticks(ctx, input, &x, &y, out);
    draw_labels(ctx, input, &x, &y, decor, out);
    colorbar::draw_inside(ctx, axes, plot, input.scales, out, hits);
    legend::draw_inside(ctx, input, &primaries, &data_points, out, hits);

    hits.axes.push(AxesHit {
        id: axes.id,
        plot_rect: plot,
        kind: AxesHitKind::TwoD { x, y },
    });
}

/// Wraps the drawn items of an axes in groups, in paint order: each maximal run of items that share a clip in one
/// group with that clip. An artist listed in `released` takes the clip listed with it, which extends beyond the plot
/// area along the dimensions it places as fractions of the plot area; every other artist is clipped to the plot area.
/// Grouping by runs keeps what covers what.
pub(super) fn clip_groups(
    axes: NodeId,
    plot: Rect,
    items: Vec<Item>,
    released: &[(NodeId, Rect)],
) -> Vec<Item> {
    let clip_of = |item: &Item| {
        item.source
            .and_then(|id| released.iter().find(|(artist, _)| *artist == id))
            .map_or(plot, |(_, clip)| *clip)
    };
    let mut groups: Vec<Item> = Vec::new();
    let mut run: Vec<Item> = Vec::new();
    let mut run_clip = plot;
    let flush = |groups: &mut Vec<Item>, run: &mut Vec<Item>, clip: Rect| {
        if !run.is_empty() {
            groups.push(Item {
                source: Some(axes),
                kind: ItemKind::Group {
                    clip: Some(clip),
                    transform: None,
                    items: std::mem::take(run),
                },
            });
        }
    };
    for item in items {
        let clip = clip_of(&item);
        if clip != run_clip {
            flush(&mut groups, &mut run, run_clip);
            run_clip = clip;
        }
        run.push(item);
    }
    flush(&mut groups, &mut run, run_clip);
    groups
}

/// Returns the map of an axis whose usual direction runs from `start` (the left or the bottom of the plot) to `end`;
/// a reversed axis runs the other way, so its limits swap ends.
fn axis_map(range: Range, start: f64, end: f64) -> AxisMap {
    let (start, end) = if range.reverse {
        (end, start)
    } else {
        (start, end)
    };
    AxisMap {
        min: range.min,
        max: range.max,
        log: range.log,
        start,
        end,
    }
}

/// Draws grid lines at the interior major ticks of each axis whose grid is enabled.
fn draw_grid(input: &AxesInput, x: &AxisMap, y: &AxisMap, out: &mut Vec<Item>) {
    let (axes, plot) = (input.axes, input.plot);
    let mut b = PathBuilder::new();
    let interior = |v: f64, lo: f64, hi: f64| v > lo.min(hi) + 1e-6 && v < lo.max(hi) - 1e-6;
    if axes.x.grid {
        for tick in &input.decor.ticks[0].major {
            let fx = x.to_figure(*tick);
            if fx.is_finite() && interior(fx, plot.x, plot.right()) {
                b.polyline(
                    &[Point::new(fx, plot.y), Point::new(fx, plot.bottom())],
                    false,
                );
            }
        }
    }
    if axes.y.grid {
        for tick in &input.decor.ticks[1].major {
            let fy = y.to_figure(*tick);
            if fy.is_finite() && interior(fy, plot.y, plot.bottom()) {
                b.polyline(
                    &[Point::new(plot.x, fy), Point::new(plot.right(), fy)],
                    false,
                );
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

/// Draws the axes box (or the line of each axis) and inward tick marks.
///
/// The box consists of the spines of the two axes: the bottom and top edges belong to the x axis, and the left and
/// right edges to the y axis. With the box on, the whole frame is drawn and carries tick marks on every edge, and an
/// axis through the middle adds its own line, with tick marks, across the plot area. With the box off, each axis draws
/// only its own line, on its side, with its tick marks. A hidden axis draws neither its spines nor its tick marks, so
/// hiding both axes removes the box. While both axes are shown on the low side the box is drawn as one rectangle (or
/// one polyline without the box), exactly as before axes could be hidden or moved.
///
/// Tick marks point into the plot area from an edge, and towards the high side from a line through the middle, whose
/// labels lie on its low side.
fn draw_box_and_ticks(ctx: &Ctx, input: &AxesInput, x: &AxisMap, y: &AxisMap, out: &mut Vec<Item>) {
    let (axes, plot) = (input.axes, input.plot);
    let (l, r, t, bt) = (plot.x, plot.right(), plot.y, plot.bottom());
    let (x_shown, y_shown) = (axes.x.visible, axes.y.visible);
    let (x_side, y_side) = (axes.x.side, axes.y.side);
    let (x_at, y_at) = (x_line(x_side, plot), y_line(y_side, plot));
    let both_low = x_side == AxisSide::Low && y_side == AxisSide::Low;
    if x_shown && y_shown && axes.box_ {
        out.extend(frame(axes.id, plot));
    } else if x_shown && y_shown && both_low {
        let mut b = PathBuilder::new();
        b.polyline(
            &[Point::new(l, t), Point::new(l, bt), Point::new(r, bt)],
            false,
        );
        out.extend(paths::item(axes.id, b.finish(), None, Some(frame_stroke())));
    } else {
        let mut b = PathBuilder::new();
        if x_shown {
            if axes.box_ {
                b.polyline(&[Point::new(l, bt), Point::new(r, bt)], false);
                b.polyline(&[Point::new(l, t), Point::new(r, t)], false);
            } else {
                b.polyline(&[Point::new(l, x_at), Point::new(r, x_at)], false);
            }
        }
        if y_shown {
            if axes.box_ {
                b.polyline(&[Point::new(l, t), Point::new(l, bt)], false);
                b.polyline(&[Point::new(r, t), Point::new(r, bt)], false);
            } else {
                b.polyline(&[Point::new(y_at, t), Point::new(y_at, bt)], false);
            }
        }
        out.extend(paths::item(axes.id, b.finish(), None, Some(frame_stroke())));
    }
    if axes.box_ {
        // The frame holds the edges; an axis through the middle adds its own line.
        let mut b = PathBuilder::new();
        if x_shown && x_side == AxisSide::Centre {
            b.polyline(&[Point::new(l, x_at), Point::new(r, x_at)], false);
        }
        if y_shown && y_side == AxisSide::Centre {
            b.polyline(&[Point::new(y_at, t), Point::new(y_at, bt)], false);
        }
        out.extend(paths::item(axes.id, b.finish(), None, Some(frame_stroke())));
    }

    let length =
        (0.012 * plot.width.max(plot.height)).clamp(0.25 * ctx.font_size, 0.5 * ctx.font_size);
    let mut b = PathBuilder::new();
    let [xt, yt, _] = &input.decor.ticks;
    let mut vertical = |v: f64, len: f64| {
        let fx = x.to_figure(v);
        if !fx.is_finite() {
            return;
        }
        if axes.box_ {
            b.polyline(&[Point::new(fx, bt), Point::new(fx, bt - len)], false);
            b.polyline(&[Point::new(fx, t), Point::new(fx, t + len)], false);
            if x_side == AxisSide::Centre {
                b.polyline(&[Point::new(fx, x_at), Point::new(fx, x_at - len)], false);
            }
        } else if x_side == AxisSide::High {
            b.polyline(&[Point::new(fx, t), Point::new(fx, t + len)], false);
        } else {
            b.polyline(&[Point::new(fx, x_at), Point::new(fx, x_at - len)], false);
        }
    };
    if x_shown {
        xt.major.iter().for_each(|v| vertical(*v, length));
        xt.minor.iter().for_each(|v| vertical(*v, 0.6 * length));
    }
    let mut horizontal = |v: f64, len: f64| {
        let fy = y.to_figure(v);
        if !fy.is_finite() {
            return;
        }
        if axes.box_ {
            b.polyline(&[Point::new(l, fy), Point::new(l + len, fy)], false);
            b.polyline(&[Point::new(r, fy), Point::new(r - len, fy)], false);
            if y_side == AxisSide::Centre {
                b.polyline(&[Point::new(y_at, fy), Point::new(y_at + len, fy)], false);
            }
        } else if y_side == AxisSide::High {
            b.polyline(&[Point::new(r, fy), Point::new(r - len, fy)], false);
        } else {
            b.polyline(&[Point::new(y_at, fy), Point::new(y_at + len, fy)], false);
        }
    };
    if y_shown {
        yt.major.iter().for_each(|v| horizontal(*v, length));
        yt.minor.iter().for_each(|v| horizontal(*v, 0.6 * length));
    }
    out.extend(paths::item(axes.id, b.finish(), None, Some(frame_stroke())));
}

/// Returns the left edge of a title of the given width aligned with the plot area as `align` says.
pub(super) fn title_left(align: TitleAlign, plot: Rect, width: f64) -> f64 {
    match align {
        TitleAlign::Center => plot.x + (plot.width - width) / 2.0,
        TitleAlign::Left => plot.x,
        TitleAlign::Right => plot.right() - width,
    }
}

/// Draws tick labels, common exponent labels, axis labels and the title.
///
/// The labels of each axis are set beside its line, on the side that [`Side::of_x`] or [`Side::of_y`] gives: tick
/// labels next to the line, the axis label beyond them, and the common exponent label beyond the end of the axis (x)
/// or above it (y). The label of an axis on an edge is centred along the plot; that of an axis through the middle is
/// set at its high end (the right of x, the top of y), away from where the two axes cross.
fn draw_labels(
    ctx: &mut Ctx,
    input: &AxesInput,
    x: &AxisMap,
    y: &AxisMap,
    decor: &Decor,
    out: &mut Vec<Item>,
) {
    let s = Spacing::new(ctx);
    let (axes, id, plot) = (input.axes, input.axes.id, input.plot);
    let [xt, yt, _] = &decor.ticks;
    let (x_side, y_side) = (Side::of_x(axes.x.side), Side::of_y(axes.y.side));
    let (x_at, y_at) = (x_line(axes.x.side, plot), y_line(axes.y.side, plot));

    // Where both axes run through the middle they cross inside the plot area, and the tick label of either axis at the
    // crossing would be struck through by the line of the other, so neither is drawn.
    let crossed = axes.x.side == AxisSide::Centre && axes.y.side == AxisSide::Centre;
    let at_crossing = |position: f64, line: f64| crossed && (position - line).abs() < 0.5;

    // Level x tick labels share a baseline beside the line; turned ones are each placed by `turned_label`.
    let [x_angle, y_angle] = decor.angles;
    let x_reach = turned_reach(xt, x_angle, x_side, s.tick_pad);
    if x_reach.is_some() {
        for (v, label) in xt.major.iter().zip(&xt.labels) {
            let fx = x.to_figure(*v);
            if at_crossing(fx, y_at) {
                continue;
            }
            let o = turned_label(label, x_angle, x_side, s.tick_pad).origin;
            let origin = Point::new(fx + o.x, x_at + o.y);
            label.draw_turned_on_page(ctx, origin, x_angle, INK, id, out);
        }
    } else {
        let x_baseline = if x_side == Side::Top {
            x_at - s.tick_pad - xt.max_depth()
        } else {
            x_at + s.tick_pad + xt.max_height()
        };
        for (v, label) in xt.major.iter().zip(&xt.labels) {
            let fx = x.to_figure(*v);
            if at_crossing(fx, y_at) {
                continue;
            }
            label.draw_on_page(
                ctx,
                Point::new(fx - label.width() / 2.0, x_baseline),
                INK,
                id,
                out,
            );
        }
    }
    // The page y of the far edge of the x tick labels from the line.
    let beyond_ticks = if x_side == Side::Top {
        x_at - x_tick_band(xt, x_reach.as_ref(), &s)
    } else {
        match &x_reach {
            Some(reach) => x_at + reach.out,
            None if xt.labels.is_empty() => x_at,
            None => x_at + s.tick_pad + x_row_height(xt),
        }
    };
    if let Some(e) = &xt.exponent {
        let baseline = if x_side == Side::Top {
            beyond_ticks - s.exponent_gap - e.depth()
        } else {
            beyond_ticks + s.exponent_gap + e.height()
        };
        let origin = Point::new(plot.right() + s.exponent_gap, baseline);
        e.draw_on_page(ctx, origin, INK, id, out);
    }

    // Level y tick labels are aligned to the line beside it and centred on their ticks (right-aligned to its left,
    // left-aligned to its right); turned ones are each placed by `turned_label`.
    let y_right = y_at - s.tick_pad;
    let y_left = y_at + s.tick_pad;
    let y_reach = turned_reach(yt, y_angle, y_side, s.tick_pad);
    for (v, label) in yt.major.iter().zip(&yt.labels) {
        let fy = y.to_figure(*v);
        if at_crossing(fy, x_at) {
            continue;
        }
        if y_reach.is_some() {
            let o = turned_label(label, y_angle, y_side, s.tick_pad).origin;
            let origin = Point::new(y_at + o.x, fy + o.y);
            label.draw_turned_on_page(ctx, origin, y_angle, INK, id, out);
        } else if y_side == Side::Right {
            let origin = label.origin_for_centre(Point::new(y_left + label.width() / 2.0, fy));
            label.draw_on_page(ctx, origin, INK, id, out);
        } else {
            let origin = label.origin_for_centre(Point::new(y_right - label.width() / 2.0, fy));
            label.draw_on_page(ctx, origin, INK, id, out);
        }
    }
    if let Some(e) = &yt.exponent {
        // Above the plot area, or above the labels of an x axis on the high side, starting at the line of the y axis
        // (ending there for a y axis on the high side).
        let top = match axes.x.side {
            AxisSide::High => plot.y - x_band(axes, decor, &s),
            AxisSide::Low | AxisSide::Centre => plot.y,
        };
        let left = match axes.y.side {
            AxisSide::High => y_at - e.width(),
            AxisSide::Low | AxisSide::Centre => y_at,
        };
        let origin = Point::new(left, top - s.exponent_gap - e.depth());
        e.draw_on_page(ctx, origin, INK, id, out);
    }

    if let Some(label) = &decor.labels[0] {
        let baseline = if x_side == Side::Top {
            beyond_ticks - s.label_gap - label.depth()
        } else {
            beyond_ticks + s.label_gap + label.height()
        };
        // An axis through the middle crosses the other axis there, so its label is set at its high end, clear of the
        // crossing; an axis on an edge centres its label along the plot.
        let left = match axes.x.side {
            AxisSide::Centre => plot.right() - label.width(),
            AxisSide::Low | AxisSide::High => plot.x + (plot.width - label.width()) / 2.0,
        };
        let origin = Point::new(left, baseline);
        label.draw_on_page(ctx, origin, INK, id, out);
    }
    if let Some(label) = &decor.labels[1] {
        let baseline_x = if y_side == Side::Right {
            let left = match &y_reach {
                Some(reach) => y_at + reach.out + s.label_gap,
                None if yt.labels.is_empty() => y_at + s.label_gap,
                None => y_left + yt.max_width() + s.label_gap,
            };
            left + label.height()
        } else {
            let right = match &y_reach {
                Some(reach) => y_at - reach.out - s.label_gap,
                None if yt.labels.is_empty() => y_at - s.label_gap,
                None => y_right - yt.max_width() - s.label_gap,
            };
            right - label.depth()
        };
        let bottom = match axes.y.side {
            AxisSide::Centre => plot.y + label.width(),
            AxisSide::Low | AxisSide::High => plot.y + (plot.height + label.width()) / 2.0,
        };
        let origin = Point::new(baseline_x, bottom);
        label.draw_upwards_on_page(ctx, origin, INK, id, out);
    }
    if let Some(title) = &decor.title {
        let bottom = plot.y - top_band(axes, decor, &s) - s.title_gap;
        let origin = Point::new(
            title_left(axes.title_align, plot, title.width()),
            bottom - title.depth(),
        );
        title.draw_on_page(ctx, origin, INK, id, out);
    }
}
