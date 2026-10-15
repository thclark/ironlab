//! Legends: collecting the entries of an axes, merging the entries a legend panel gathers from several axes,
//! arranging entries in columns, and drawing them inside a plot area or a legend panel.
//!
//! Both kinds of legend run the same four steps. [`collect`] lists the named artists of one axes, each with the
//! [`LegendSample`] that describes exactly what its sample looks like. A legend panel merges the entries of every
//! axes that names it, so that a series drawn in several axes is listed once ([`merge`]). The entries are then
//! measured and arranged in columns ([`arrange`]), and the block of entries is placed and drawn ([`render`]).

use ironlab_ir::{
    Artist, ColorSpec, Legend, LegendEntryColumns, LegendEntryLayout, LegendEntryOrder,
    LegendLocation, LineStyle, MarkerStyle, NodeId, PanelLegend, ScatterSize, Text,
};

use crate::display::{Item, ItemKind, Point, Rect, Rgba, Stroke};
use crate::hit::{HitMap, LegendHit, LegendOwner};
use crate::maths::quiver;

use super::artists::AxesInput;
use super::data::ArtistData;
use super::decor::TICK_SCALE;
use super::image;
use super::paths::{self, PathBuilder};
use super::style::{self, INK, Paint};
use super::text::{TextBlock, measure};
use super::{Ctx, SceneWarning, SceneWarningKind};

/// The alpha factor applied to every colour of a hidden artist's entry.
const HIDDEN_ALPHA: f32 = 0.35;

/// What the sample of a legend entry draws, with every colour resolved.
///
/// Two samples are equal exactly when they draw the same picture, so that a legend panel merges entries of the same
/// name only when the reader cannot tell them apart. Every value is finite, because non-finite sizes and widths are
/// replaced before they are stored, so the derived comparison is exact. Visibility is not part of the sample: it
/// greys an entry but does not change what the entry stands for.
#[derive(Clone, Debug, PartialEq)]
enum LegendSample {
    /// A horizontal line across the sample, with a marker at its centre (a line or contour isolines).
    Line {
        stroke: Option<Stroke>,
        marker: Option<MarkerSample>,
    },
    /// A single marker at the centre of the sample (a scatter).
    Marker(Option<MarkerSample>),
    /// A horizontal arrow across the sample (a quiver).
    Arrow(Option<Stroke>),
    /// A filled rectangle across the sample, with an optional outline (contour bands, surfaces and images).
    Patch {
        fill: Option<Rgba>,
        edge: Option<Rgba>,
    },
}

/// The marker of a legend sample.
#[derive(Clone, Debug, PartialEq)]
struct MarkerSample {
    /// The style of the marker, whose size is the size drawn before it is limited to the height of the row.
    style: MarkerStyle,
    /// The colour that a marker face or edge set to `Auto` or `Colormapped` takes.
    auto: Option<Rgba>,
    /// The width of the marker edge, in points.
    edge_width: f64,
}

/// One entry of a legend: a display name, its sample, and the artists it stands for.
pub(super) struct Entry<'a> {
    name: &'a Text,
    sample: LegendSample,
    /// The `(axes, artist)` pairs of the artists the entry stands for, in figure order.
    targets: Vec<(NodeId, NodeId)>,
    /// Whether any of the artists the entry stands for is visible; an entry is greyed when none is.
    visible: bool,
}

/// Metrics of a legend in points, derived from the font size.
struct Metrics {
    pad: f64,
    sample_width: f64,
    gap: f64,
    row_gap: f64,
    column_gap: f64,
    inset: f64,
    min_row: f64,
}

impl Metrics {
    fn new(fs: f64) -> Self {
        Self {
            pad: 0.45 * fs,
            sample_width: 2.2 * fs,
            gap: 0.5 * fs,
            row_gap: 0.25 * fs,
            column_gap: 1.0 * fs,
            inset: 0.7 * fs,
            min_row: 0.9 * fs,
        }
    }

    /// Returns the width of a column of entries whose widest label is `label_width` wide.
    fn column_width(&self, label_width: f64) -> f64 {
        self.sample_width + self.gap + label_width
    }
}

/// The arrangement of the entries of a legend in rows and columns.
struct Grid {
    /// The row and column of each entry, in the order of the entries.
    cells: Vec<(usize, usize)>,
    /// The width of the widest label in each column.
    label_widths: Vec<f64>,
    /// The height of each row, the gap below it included.
    row_heights: Vec<f64>,
    /// The size of the block of entries, its padding included.
    width: f64,
    height: f64,
}

/// Lists the entries of an axes: one for every artist with a display name and usable data, in artist order, hidden
/// artists included.
pub(super) fn collect<'a>(input: &AxesInput<'a>, primaries: &[Paint]) -> Vec<Entry<'a>> {
    let axes = input.axes;
    input
        .prepared
        .iter()
        .zip(primaries)
        .filter_map(|(p, primary)| {
            let data = p.data.as_ref()?;
            let name = p.artist.display_name()?;
            let colour = primary.single(&input.colours);
            Some(Entry {
                name,
                sample: sample(p.artist, data, colour, input.colours.middle()),
                targets: vec![(axes.id, p.artist.id())],
                visible: p.artist.visible(),
            })
        })
        .collect()
}

/// Draws the legend inside the plot area of an axes, if it has one with at least one entry, and records the entries
/// in the hit map.
///
/// `data_points` are figure-space vertices of the drawn data, used to choose the `Best` location.
pub(super) fn draw_inside(
    ctx: &mut Ctx,
    input: &AxesInput,
    primaries: &[Paint],
    data_points: &[Point],
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    let axes = input.axes;
    // The entries of an axes whose legend names a legend panel are drawn with the panel.
    let Some(Legend::Inside(legend)) = axes.legend else {
        return;
    };
    let entries = collect(input, primaries);
    let m = Metrics::new(ctx.font_size);
    let plot = input.plot;
    let available = Rect::new(
        plot.x + m.inset,
        plot.y + m.inset,
        (plot.width - 2.0 * m.inset).max(0.0),
        (plot.height - 2.0 * m.inset).max(0.0),
    );
    render(
        ctx,
        Block {
            owner: LegendOwner::Axes(axes.id),
            source: axes.id,
            layout: legend.layout,
            boxed: legend.boxed,
            available,
        },
        &entries,
        |width, height| place(plot, width, height, legend.location, m.inset, data_points),
        out,
        hits,
    );
}

/// Draws a legend panel in `area`, listing the entries gathered from every axes that names it in figure order, and
/// records the entries in the hit map. A panel with no entries draws nothing.
pub(super) fn draw_panel(
    ctx: &mut Ctx,
    panel: &PanelLegend,
    area: Rect,
    entries: Vec<Entry>,
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    let owner = LegendOwner::Panel(panel.id);
    let entries = merge(ctx, owner, entries);
    // A panel holds no data to avoid, so the best location is the centre.
    let align = match panel.align {
        LegendLocation::Best => LegendLocation::Center,
        align => align,
    };
    render(
        ctx,
        Block {
            owner,
            source: panel.id,
            layout: panel.layout,
            boxed: panel.boxed,
            available: area,
        },
        &entries,
        |width, height| place(area, width, height, align, 0.0, &[]),
        out,
        hits,
    );
}

/// Merges entries with the same display name and the same sample into one entry that stands for all of their
/// artists, keeping the order in which each merged entry first appears.
///
/// Entries that share a display name but not a sample are all listed, and one [`SceneWarningKind::LegendNameClash`]
/// is reported for each such name, because the reader cannot tell which entry stands for which series.
fn merge<'a>(ctx: &mut Ctx, owner: LegendOwner, entries: Vec<Entry<'a>>) -> Vec<Entry<'a>> {
    let mut merged: Vec<Entry<'a>> = Vec::new();
    for entry in entries {
        match merged
            .iter_mut()
            .find(|m| m.name == entry.name && m.sample == entry.sample)
        {
            Some(m) => {
                m.targets.extend(entry.targets);
                m.visible |= entry.visible;
            }
            None => merged.push(entry),
        }
    }
    let mut clashes: Vec<&str> = Vec::new();
    for (k, entry) in merged.iter().enumerate() {
        let name = entry.name.content.as_str();
        if !clashes.contains(&name) && merged[..k].iter().any(|e| e.name.content == name) {
            clashes.push(name);
        }
    }
    let node = match owner {
        LegendOwner::Axes(id) | LegendOwner::Panel(id) => id,
    };
    for name in clashes {
        ctx.warnings.push(SceneWarning {
            node: Some(node),
            message: format!(
                "Several legend entries are named \"{name}\" but are drawn differently, so the reader cannot tell \
                 them apart."
            ),
            kind: SceneWarningKind::LegendNameClash {
                legend: owner,
                name: name.to_owned(),
            },
        });
    }
    merged
}

/// What a block of legend entries belongs to and where it may be drawn.
struct Block {
    /// The legend that draws the block.
    owner: LegendOwner,
    /// The node every item of the block names as its source.
    source: NodeId,
    layout: LegendEntryLayout,
    boxed: bool,
    /// The area the block must fit: the plot area less the legend inset, or the area of a legend panel.
    available: Rect,
}

/// Measures, arranges and draws a block of entries, and records them in the hit map.
///
/// `place` returns the rectangle of a block of the given width and height. When the block does not fit the
/// available area even in as many columns as fit its width, a [`SceneWarningKind::LegendOverflow`] is reported, the
/// block is drawn from the top-left corner of the area inside a group clipped to it, and the entries that do not lie
/// wholly inside the area are left out.
fn render(
    ctx: &mut Ctx,
    block: Block,
    entries: &[Entry],
    place: impl FnOnce(f64, f64) -> Rect,
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    if entries.is_empty() {
        return;
    }
    let fs = ctx.font_size;
    let m = Metrics::new(fs);
    // A merged entry is labelled by its first artist, which therefore names the display name in warnings.
    let labels: Vec<TextBlock> = entries
        .iter()
        .map(|e| measure(ctx, e.name, TICK_SCALE * fs, e.targets[0].1))
        .collect();
    let area = block.available;
    let grid = arrange(&labels, block.layout, area.width, area.height, &m);
    let overflow = grid.width > area.width || grid.height > area.height;
    let rect = if overflow {
        let node = block.source;
        ctx.warnings.push(SceneWarning {
            node: Some(node),
            message: format!(
                "The legend entries do not fit the {:.1} pt × {:.1} pt available to them, so some are not drawn.",
                area.width, area.height
            ),
            kind: SceneWarningKind::LegendOverflow { legend: block.owner },
        });
        Rect::new(area.x, area.y, grid.width, grid.height)
    } else {
        place(grid.width, grid.height)
    };

    let mut items = Vec::new();
    if block.boxed {
        let mut b = PathBuilder::new();
        b.rect(if overflow {
            Rect::new(
                rect.x,
                rect.y,
                rect.width.min(area.width),
                rect.height.min(area.height),
            )
        } else {
            rect
        });
        items.extend(paths::item(
            block.source,
            b.finish(),
            Some(paths::fill(Rgba::WHITE)),
            Some(paths::solid(INK, 0.5)),
        ));
    }

    let column_x: Vec<f64> = grid
        .label_widths
        .iter()
        .scan(rect.x + m.pad, |x, w| {
            let start = *x;
            *x += m.column_width(*w) + m.column_gap;
            Some(start)
        })
        .collect();
    let row_y: Vec<f64> = grid
        .row_heights
        .iter()
        .scan(rect.y + m.pad, |y, h| {
            let start = *y;
            *y += h;
            Some(start)
        })
        .collect();
    for ((entry, label), &(row, col)) in entries.iter().zip(&labels).zip(&grid.cells) {
        let (x, y, h) = (column_x[col], row_y[row], grid.row_heights[row]);
        let hit = Rect::new(
            x - m.pad / 2.0,
            y,
            m.column_width(grid.label_widths[col]) + m.pad,
            h,
        );
        if overflow && !contains(area, hit) {
            continue;
        }
        let alpha = if entry.visible { 1.0 } else { HIDDEN_ALPHA };
        let cell = Rect::new(x, y, m.sample_width, h);
        draw_sample(block.source, &entry.sample, cell, alpha, &mut items);
        let origin = Point::new(
            cell.right() + m.gap,
            y + (h - label.total_height()) / 2.0 + label.height(),
        );
        label.draw(
            origin,
            INK.with_alpha_factor(alpha),
            block.source,
            &mut items,
        );
        hits.legend_entries.push(LegendHit {
            legend: block.owner,
            targets: entry.targets.clone(),
            rect: hit,
        });
    }

    if overflow {
        out.push(Item {
            source: Some(block.source),
            kind: ItemKind::Group {
                clip: Some(area),
                transform: None,
                items,
            },
        });
    } else {
        out.extend(items);
    }
}

/// Returns whether `inner` lies inside `outer`.
fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

/// Arranges measured entries in columns for a block that should fit `max_width × max_height`.
///
/// A fixed count of columns is used as given, but never more columns than entries. Automatic columns are the fewest
/// for which the block fits the height; when no count fits the height, they are as many as fit the width, and at
/// least one. The caller compares the block with the available size to detect an overflow.
fn arrange(
    labels: &[TextBlock],
    layout: LegendEntryLayout,
    max_width: f64,
    max_height: f64,
    m: &Metrics,
) -> Grid {
    let n = labels.len();
    let heights: Vec<f64> = labels
        .iter()
        .map(|l| l.total_height().max(m.min_row) + m.row_gap)
        .collect();
    let widths: Vec<f64> = labels.iter().map(TextBlock::width).collect();
    let grid = |columns: usize| grid(&heights, &widths, columns, layout.order, m);
    match layout.columns {
        LegendEntryColumns::Count(columns) => grid((columns.max(1) as usize).min(n)),
        LegendEntryColumns::Auto => {
            let mut chosen = grid(1);
            for columns in 2..=n {
                if chosen.height <= max_height {
                    break;
                }
                let wider = grid(columns);
                if wider.width > max_width {
                    break;
                }
                chosen = wider;
            }
            chosen
        }
    }
}

/// Places entries of the given row heights and label widths in at most `columns` columns, in the given order.
///
/// Every column but the last holds the same number of rows. The rows are as tall as their tallest entry and the
/// columns as wide as their widest, so that entries line up both across and down.
fn grid(
    heights: &[f64],
    widths: &[f64],
    columns: usize,
    order: LegendEntryOrder,
    m: &Metrics,
) -> Grid {
    let n = heights.len();
    let rows = n.div_ceil(columns.max(1));
    let cells: Vec<(usize, usize)> = (0..n)
        .map(|k| match order {
            LegendEntryOrder::ColumnMajor => (k % rows, k / rows),
            LegendEntryOrder::RowMajor => (k / columns, k % columns),
        })
        .collect();
    let used = cells.iter().map(|&(_, c)| c + 1).max().unwrap_or(0);
    let mut label_widths = vec![0.0f64; used];
    let mut row_heights = vec![0.0f64; rows];
    for (k, &(row, col)) in cells.iter().enumerate() {
        label_widths[col] = label_widths[col].max(widths[k]);
        row_heights[row] = row_heights[row].max(heights[k]);
    }
    let width = 2.0 * m.pad
        + label_widths.iter().map(|w| m.column_width(*w)).sum::<f64>()
        + used.saturating_sub(1) as f64 * m.column_gap;
    let height = 2.0 * m.pad + row_heights.iter().sum::<f64>();
    Grid {
        cells,
        label_widths,
        row_heights,
        width,
        height,
    }
}

/// Chooses the rectangle of a block of entries inside `area`, kept `inset` from its edges.
fn place(
    area: Rect,
    width: f64,
    height: f64,
    location: LegendLocation,
    inset: f64,
    points: &[Point],
) -> Rect {
    let left = area.x + inset;
    let right = area.right() - inset - width;
    let top = area.y + inset;
    let bottom = area.bottom() - inset - height;
    let cx = area.x + (area.width - width) / 2.0;
    let cy = area.y + (area.height - height) / 2.0;
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
        LegendLocation::Center => at(cx, cy),
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

/// Returns the sample of an artist, given its resolved primary colour and the middle colour of its axes' colormap.
fn sample(artist: &Artist, data: &ArtistData, colour: Option<Rgba>, middle: Rgba) -> LegendSample {
    match artist {
        Artist::Line(line) => LegendSample::Line {
            stroke: sample_stroke(&line.line, colour),
            marker: marker_sample(
                line.marker,
                line.marker.size_pt,
                colour,
                style::width_or(line.line.width_pt, 0.75).clamp(0.5, 1.5),
            ),
        },
        Artist::Scatter(scatter) => {
            let size = match scatter.size {
                ScatterSize::Scalar { value } => value,
                ScatterSize::Data { .. } => scatter.marker.size_pt,
            };
            let size = if size.is_finite() && size > 0.0 {
                size
            } else {
                4.0
            };
            LegendSample::Marker(marker_sample(scatter.marker, size, colour, 0.5))
        }
        Artist::Quiver(q) => {
            LegendSample::Arrow(sample_stroke(&q.line, colour).map(|stroke| Stroke {
                dash: Vec::new(),
                ..stroke
            }))
        }
        Artist::Contour(c) => {
            let explicit = match c.line.color {
                ColorSpec::Rgba { color } => Some(style::ir_colour(color)),
                _ => None,
            };
            if c.fill {
                LegendSample::Patch {
                    fill: Some(middle),
                    edge: explicit,
                }
            } else {
                let colour = match c.line.color {
                    ColorSpec::None => None,
                    ColorSpec::Rgba { color } => Some(style::ir_colour(color)),
                    ColorSpec::Auto | ColorSpec::Colormapped => Some(middle),
                };
                LegendSample::Line {
                    stroke: sample_stroke(&c.line, colour),
                    marker: None,
                }
            }
        }
        Artist::Surface(s) => {
            let paint = |spec| match style::resolve(spec, Paint::Colormapped) {
                Paint::Fixed(c) => c,
                Paint::Colormapped => Some(middle),
            };
            LegendSample::Patch {
                fill: paint(s.face),
                edge: paint(s.edge),
            }
        }
        // The mean colour of the raster, so that the patch resembles it; an image with no pixel to average, every
        // pixel being transparent, has no patch.
        Artist::Image(_) => LegendSample::Patch {
            fill: match data {
                ArtistData::Image(image) => image::mean_colour(image),
                _ => None,
            },
            edge: None,
        },
        // The middle colour of the colormap, as for other colormapped artists without a single value: a stopgap
        // for the colormap redesign, until a colourbar (issue #8) shows the scale itself.
        Artist::IndexedImage(_) | Artist::MappedImage(_) => LegendSample::Patch {
            fill: Some(middle),
            edge: None,
        },
    }
}

/// Returns the marker of a sample, or `None` when the marker has no shape or no usable size.
fn marker_sample(
    style: MarkerStyle,
    size: f64,
    auto: Option<Rgba>,
    edge_width: f64,
) -> Option<MarkerSample> {
    style::marker_outline(style.shape)?;
    (size.is_finite() && size > 0.0).then_some(MarkerSample {
        style: MarkerStyle {
            size_pt: size,
            ..style
        },
        auto,
        edge_width,
    })
}

/// Draws a sample inside `cell`, the sample column of its row.
fn draw_sample(source: NodeId, sample: &LegendSample, cell: Rect, alpha: f32, out: &mut Vec<Item>) {
    let fade = |c: Rgba| c.with_alpha_factor(alpha);
    let fade_stroke = |s: &Stroke| Stroke {
        color: fade(s.color),
        ..s.clone()
    };
    let cy = cell.y + cell.height / 2.0;
    let centre = Point::new(cell.x + cell.width / 2.0, cy);
    let marker = |m: &MarkerSample, out: &mut Vec<Item>| {
        let Some(outline) = style::marker_outline(m.style.shape) else {
            return;
        };
        if let Some(instance) = style::marker_instance(
            &m.style,
            &outline,
            style::MarkerPlace {
                position: centre,
                size: m.style.size_pt.min(0.8 * cell.height),
                depth: 0.0,
                source_index: 0,
            },
            m.auto,
            alpha,
        ) {
            out.push(style::marker_item(source, &outline, m.edge_width, instance));
        }
    };
    match sample {
        LegendSample::Line { stroke, marker: m } => {
            if let Some(stroke) = stroke {
                let mut b = PathBuilder::new();
                b.polyline(
                    &[Point::new(cell.x, cy), Point::new(cell.right(), cy)],
                    false,
                );
                out.extend(paths::item(
                    source,
                    b.finish(),
                    None,
                    Some(fade_stroke(stroke)),
                ));
            }
            if let Some(m) = m {
                marker(m, out);
            }
        }
        LegendSample::Marker(m) => {
            if let Some(m) = m {
                marker(m, out);
            }
        }
        LegendSample::Arrow(stroke) => {
            let Some(stroke) = stroke else {
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
                Some(fade_stroke(stroke)),
            ));
        }
        LegendSample::Patch { fill, edge } => {
            let h = 0.6 * cell.height;
            let mut b = PathBuilder::new();
            b.rect(Rect::new(cell.x, cy - h / 2.0, cell.width, h));
            let fill = fill.map(|c| paths::fill(fade(c)));
            let stroke = edge.map(|c| paths::solid(fade(c), 0.5));
            out.extend(paths::item(source, b.finish(), fill, stroke));
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
