//! Two-dimensional axes: margins, grid, box, ticks, labels, title, artists and legend.

use ironlab_ir::{NodeId, TitleAlign};

use crate::display::{Item, ItemKind, Point, Rect, Stroke};
use crate::hit::{AxesHit, AxesHitKind, AxisMap, HitMap};

use super::Ctx;
use super::artists::{AxesInput, Placement, Space, coalesce_markers, draw_artists, group_dense};
use super::decor::{AxisTicks, Decor};
use super::layout::{Margins, page_padding};
use super::paths::{self, PathBuilder};
use super::style::{self, GRID, INK};
use super::{legend, limits::Range};

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

/// The height of the band above the plot that holds the y exponent label and the overhang of the top y tick label.
fn top_band(decor: &Decor, s: &Spacing) -> f64 {
    let y = &decor.ticks[1];
    let overhang = (y.max_height() + y.max_depth()) / 2.0;
    let exponent = y
        .exponent
        .as_ref()
        .map_or(0.0, |e| e.total_height() + s.exponent_gap);
    overhang.max(exponent)
}

/// Computes the space a 2D axes needs around its plot rectangle.
pub(super) fn margins(ctx: &Ctx, decor: &Decor) -> Margins {
    let s = Spacing::new(ctx);
    let [x, y, _] = &decor.ticks;
    let first_x_half = x.labels.first().map_or(0.0, |l| l.width() / 2.0);
    let last_x_half = x.labels.last().map_or(0.0, |l| l.width() / 2.0);
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
    let y_labels = if y.labels.is_empty() {
        0.0
    } else {
        s.tick_pad + y.max_width()
    };
    let x_labels = if x.labels.is_empty() {
        0.0
    } else {
        s.tick_pad + x_row_height(x)
    };
    let right_exponent = x
        .exponent
        .as_ref()
        .map_or(0.0, |e| s.exponent_gap + e.width());
    Margins {
        left: s.outer + (y_labels + ylabel).max(first_x_half),
        right: s.outer + last_x_half.max(right_exponent),
        top: s.outer + top_band(decor, &s) + title,
        bottom: s.outer + x_labels + xlabel.max(exponent_row),
    }
}

/// Draws a 2D axes and records its hit geometry.
pub(super) fn emit(ctx: &mut Ctx, input: &AxesInput, out: &mut Vec<Item>, hits: &mut HitMap) {
    let axes = input.axes;
    let plot = input.plot;
    let [xr, yr, _] = *input.ranges;
    let x = axis_map(xr, plot.x, plot.right());
    let y = axis_map(yr, plot.bottom(), plot.y);
    let space = Space::TwoD { x, y };
    let decor = input.decor;

    draw_grid(input, &x, &y, out);

    let primaries = style::primaries(axes);
    let drawn = draw_artists(ctx, input, &primaries, &space);
    hits.artists.extend(drawn.hits);
    hits.images.extend(drawn.images);
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
fn clip_groups(
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

/// Draws the axes box (or its left and bottom edges) and inward tick marks.
fn draw_box_and_ticks(ctx: &Ctx, input: &AxesInput, x: &AxisMap, y: &AxisMap, out: &mut Vec<Item>) {
    let (axes, plot) = (input.axes, input.plot);
    let (l, r, t, bt) = (plot.x, plot.right(), plot.y, plot.bottom());
    if axes.box_ {
        out.extend(frame(axes.id, plot));
    } else {
        let mut b = PathBuilder::new();
        b.polyline(
            &[Point::new(l, t), Point::new(l, bt), Point::new(r, bt)],
            false,
        );
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
        b.polyline(&[Point::new(fx, bt), Point::new(fx, bt - len)], false);
        if axes.box_ {
            b.polyline(&[Point::new(fx, t), Point::new(fx, t + len)], false);
        }
    };
    xt.major.iter().for_each(|v| vertical(*v, length));
    xt.minor.iter().for_each(|v| vertical(*v, 0.6 * length));
    let mut horizontal = |v: f64, len: f64| {
        let fy = y.to_figure(v);
        if !fy.is_finite() {
            return;
        }
        b.polyline(&[Point::new(l, fy), Point::new(l + len, fy)], false);
        if axes.box_ {
            b.polyline(&[Point::new(r, fy), Point::new(r - len, fy)], false);
        }
    };
    yt.major.iter().for_each(|v| horizontal(*v, length));
    yt.minor.iter().for_each(|v| horizontal(*v, 0.6 * length));
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
fn draw_labels(
    ctx: &mut Ctx,
    input: &AxesInput,
    x: &AxisMap,
    y: &AxisMap,
    decor: &Decor,
    out: &mut Vec<Item>,
) {
    let s = Spacing::new(ctx);
    let (id, plot) = (input.axes.id, input.plot);
    let [xt, yt, _] = &decor.ticks;

    // x tick labels share a baseline below the plot.
    let x_baseline = plot.bottom() + s.tick_pad + xt.max_height();
    for (v, label) in xt.major.iter().zip(&xt.labels) {
        let fx = x.to_figure(*v);
        label.draw_on_page(
            ctx,
            Point::new(fx - label.width() / 2.0, x_baseline),
            INK,
            id,
            out,
        );
    }
    let below_ticks = if xt.labels.is_empty() {
        plot.bottom()
    } else {
        plot.bottom() + s.tick_pad + x_row_height(xt)
    };
    if let Some(e) = &xt.exponent {
        let origin = Point::new(
            plot.right() + s.exponent_gap,
            below_ticks + s.exponent_gap + e.height(),
        );
        e.draw_on_page(ctx, origin, INK, id, out);
    }

    // y tick labels are right-aligned beside the plot and centred on their ticks.
    let y_right = plot.x - s.tick_pad;
    for (v, label) in yt.major.iter().zip(&yt.labels) {
        let fy = y.to_figure(*v);
        let origin = label.origin_for_centre(Point::new(y_right - label.width() / 2.0, fy));
        label.draw_on_page(ctx, origin, INK, id, out);
    }
    if let Some(e) = &yt.exponent {
        let origin = Point::new(plot.x, plot.y - s.exponent_gap - e.depth());
        e.draw_on_page(ctx, origin, INK, id, out);
    }

    if let Some(label) = &decor.labels[0] {
        let origin = Point::new(
            plot.x + (plot.width - label.width()) / 2.0,
            below_ticks + s.label_gap + label.height(),
        );
        label.draw_on_page(ctx, origin, INK, id, out);
    }
    if let Some(label) = &decor.labels[1] {
        let right = if yt.labels.is_empty() {
            plot.x - s.label_gap
        } else {
            y_right - yt.max_width() - s.label_gap
        };
        let origin = Point::new(
            right - label.depth(),
            plot.y + (plot.height + label.width()) / 2.0,
        );
        label.draw_upwards_on_page(ctx, origin, INK, id, out);
    }
    if let Some(title) = &decor.title {
        let bottom = plot.y - top_band(decor, &s) - s.title_gap;
        let origin = Point::new(
            title_left(input.axes.title_align, plot, title.width()),
            bottom - title.depth(),
        );
        title.draw_on_page(ctx, origin, INK, id, out);
    }
}
