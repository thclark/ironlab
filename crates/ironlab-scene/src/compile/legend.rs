//! Legends: entry layout, placement inside the plot area, samples and greyed entries of hidden artists.

use ironlab_ir::{Artist, ColorSpec, LegendLocation, LineStyle, NodeId, ScatterSize};

use crate::display::{Item, Point, Rect, Rgba, Stroke};
use crate::hit::{HitMap, LegendHit};
use crate::maths::quiver;

use super::Ctx;
use super::artists::AxesInput;
use super::decor::TICK_SCALE;
use super::paths::{self, PathBuilder};
use super::style::{self, ColourScale, INK, Paint};
use super::text::{TextBlock, measure};

/// The alpha factor applied to every colour of a hidden artist's entry.
const HIDDEN_ALPHA: f32 = 0.35;

/// One measured legend entry.
struct Entry<'a> {
    artist: &'a Artist,
    primary: Paint,
    label: TextBlock,
}

/// Metrics of a legend in points, derived from the font size.
struct Metrics {
    pad: f64,
    sample_width: f64,
    gap: f64,
    row_gap: f64,
    inset: f64,
}

impl Metrics {
    fn new(fs: f64) -> Self {
        Self {
            pad: 0.45 * fs,
            sample_width: 2.2 * fs,
            gap: 0.5 * fs,
            row_gap: 0.25 * fs,
            inset: 0.7 * fs,
        }
    }
}

/// Draws the legend of an axes, if it has one with at least one entry, and records the entries in the hit map.
///
/// `data_points` are figure-space vertices of the drawn data, used to choose the `Best` location.
pub(super) fn draw(
    ctx: &mut Ctx,
    input: &AxesInput,
    primaries: &[Paint],
    data_points: &[Point],
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    let axes = input.axes;
    let Some(legend) = axes.legend else { return };
    let fs = ctx.font_size;
    let m = Metrics::new(fs);
    let entries: Vec<Entry> = input
        .prepared
        .iter()
        .zip(primaries)
        .filter(|(p, _)| p.data.is_some())
        .filter_map(|(p, primary)| {
            let name = p.artist.display_name()?;
            Some(Entry {
                artist: p.artist,
                primary: *primary,
                label: measure(ctx, name, TICK_SCALE * fs, p.artist.id()),
            })
        })
        .collect();
    if entries.is_empty() {
        return;
    }

    let row_height = |e: &Entry| e.label.total_height().max(0.9 * fs) + m.row_gap;
    let label_width = entries.iter().map(|e| e.label.width()).fold(0.0, f64::max);
    let width = 2.0 * m.pad + m.sample_width + m.gap + label_width;
    let height = 2.0 * m.pad + entries.iter().map(row_height).sum::<f64>();
    let rect = place(
        input.plot,
        width,
        height,
        legend.location,
        m.inset,
        data_points,
    );

    if legend.boxed {
        let mut b = PathBuilder::new();
        b.rect(rect);
        out.extend(paths::item(
            axes.id,
            b.finish(),
            Some(paths::fill(Rgba::WHITE)),
            Some(paths::solid(INK, 0.5)),
        ));
    }

    let mut y = rect.y + m.pad;
    for entry in &entries {
        let h = row_height(entry);
        let alpha = if entry.artist.visible() {
            1.0
        } else {
            HIDDEN_ALPHA
        };
        let sample = Rect::new(rect.x + m.pad, y, m.sample_width, h);
        draw_sample(axes.id, entry, sample, &input.colours, alpha, out);
        let label_x = sample.right() + m.gap;
        let origin = Point::new(
            label_x,
            y + (h - entry.label.total_height()) / 2.0 + entry.label.height(),
        );
        entry
            .label
            .draw(origin, INK.with_alpha_factor(alpha), axes.id, out);
        let left = rect.x + m.pad / 2.0;
        hits.legend_entries.push(LegendHit {
            axes: axes.id,
            artist: entry.artist.id(),
            rect: Rect::new(left, y, rect.right() - m.pad / 2.0 - left, h),
        });
        y += h;
    }
}

/// Chooses the legend rectangle inside the plot rectangle.
fn place(
    plot: Rect,
    width: f64,
    height: f64,
    location: LegendLocation,
    inset: f64,
    points: &[Point],
) -> Rect {
    let left = plot.x + inset;
    let right = plot.right() - inset - width;
    let top = plot.y + inset;
    let bottom = plot.bottom() - inset - height;
    let cx = plot.x + (plot.width - width) / 2.0;
    let cy = plot.y + (plot.height - height) / 2.0;
    let at = |x: f64, y: f64| Rect::new(x, y, width, height);
    match location {
        LegendLocation::NorthEast => at(right, top),
        LegendLocation::NorthWest => at(left, top),
        LegendLocation::SouthEast => at(right, bottom),
        LegendLocation::SouthWest => at(left, bottom),
        LegendLocation::North => at(cx, top),
        LegendLocation::South => at(cx, bottom),
        LegendLocation::East => at(right, cy),
        LegendLocation::West => at(left, cy),
        LegendLocation::Best => {
            let candidates = [
                at(right, top),
                at(left, top),
                at(right, bottom),
                at(left, bottom),
            ];
            let covered = |r: &Rect| points.iter().filter(|p| r.contains(**p)).count();
            candidates
                .iter()
                .enumerate()
                .min_by_key(|(k, r)| (covered(r), *k))
                .map(|(_, r)| *r)
                .unwrap_or(candidates[0])
        }
    }
}

/// Draws the sample of one entry inside `cell`, the sample column of its row.
fn draw_sample(
    source: NodeId,
    entry: &Entry,
    cell: Rect,
    scale: &ColourScale,
    alpha: f32,
    out: &mut Vec<Item>,
) {
    let fade = |c: Rgba| c.with_alpha_factor(alpha);
    let cy = cell.y + cell.height / 2.0;
    let centre = Point::new(cell.x + cell.width / 2.0, cy);
    let marker_room = 0.8 * cell.height;
    let line_sample = |style: &LineStyle, colour: Option<Rgba>, out: &mut Vec<Item>| {
        let Some(stroke) = sample_stroke(style, colour.map(fade)) else {
            return;
        };
        let mut b = PathBuilder::new();
        b.polyline(
            &[Point::new(cell.x, cy), Point::new(cell.right(), cy)],
            false,
        );
        out.extend(paths::item(source, b.finish(), None, Some(stroke)));
    };
    let patch = |fill: Option<Rgba>, edge: Option<Rgba>, out: &mut Vec<Item>| {
        let h = 0.6 * cell.height;
        let mut b = PathBuilder::new();
        b.rect(Rect::new(cell.x, cy - h / 2.0, cell.width, h));
        let fill = fill.map(|c| paths::fill(fade(c)));
        let stroke = edge.map(|c| paths::solid(fade(c), 0.5));
        out.extend(paths::item(source, b.finish(), fill, stroke));
    };
    match entry.artist {
        Artist::Line(line) => {
            let colour = entry.primary.single(scale);
            line_sample(&line.line, colour, out);
            let size = line.marker.size_pt.min(marker_room);
            let width = style::width_or(line.line.width_pt, 0.75).clamp(0.5, 1.5);
            out.extend(style::marker_item(
                source,
                &line.marker,
                centre,
                size,
                colour,
                width,
                alpha,
            ));
        }
        Artist::Scatter(scatter) => {
            let colour = entry.primary.single(scale);
            let size = match scatter.size {
                ScatterSize::Scalar { value } => value,
                ScatterSize::Data { .. } => scatter.marker.size_pt,
            };
            let size = if size.is_finite() && size > 0.0 {
                size
            } else {
                4.0
            };
            out.extend(style::marker_item(
                source,
                &scatter.marker,
                centre,
                size.min(marker_room),
                colour,
                0.5,
                alpha,
            ));
        }
        Artist::Quiver(q) => {
            let Some(stroke) = sample_stroke(&q.line, entry.primary.single(scale).map(fade)) else {
                return;
            };
            let a = quiver::arrow([cell.x, cy, 0.0], [cell.width, 0.0, 0.0], 1.0, 0.3);
            let p = |v: [f64; 3]| Point::new(v[0], v[1]);
            let mut b = PathBuilder::new();
            b.polyline(&[p(a.shaft[0]), p(a.shaft[1])], false);
            b.polyline(&[p(a.head[0]), p(a.head[1]), p(a.head[2])], false);
            out.extend(paths::item(
                source,
                b.finish(),
                None,
                Some(Stroke {
                    dash: Vec::new(),
                    ..stroke
                }),
            ));
        }
        Artist::Contour(c) => {
            let explicit = match c.line.color {
                ColorSpec::Rgba { color } => Some(style::ir_colour(color)),
                _ => None,
            };
            if c.fill {
                patch(Some(scale.middle()), explicit, out);
            } else {
                let colour = match c.line.color {
                    ColorSpec::None => None,
                    ColorSpec::Rgba { color } => Some(style::ir_colour(color)),
                    ColorSpec::Auto | ColorSpec::Colormapped => Some(scale.middle()),
                };
                line_sample(&c.line, colour, out);
            }
        }
        Artist::Surface(s) => {
            let paint = |spec| style::resolve(spec, Paint::Colormapped).single(scale);
            patch(paint(s.face), paint(s.edge), out);
        }
    }
}

/// Returns the stroke of a legend line sample, with its width limited so the sample stays inside its row.
fn sample_stroke(style: &LineStyle, colour: Option<Rgba>) -> Option<Stroke> {
    let colour = colour?;
    let width = style::width_or(style.width_pt, 0.75).min(3.0);
    let dash = style::dash_array(style.dash, width)?;
    Some(paths::stroke(colour, width, dash))
}
