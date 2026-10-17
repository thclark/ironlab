//! Legends: listing the entries of a legend, merging the entries a legend panel gathers from several axes, arranging
//! entries in columns, and drawing them inside a plot area or a legend panel.
//!
//! Both kinds of legend run the same four steps. [`candidates`] describes every artist of one axes that a legend can
//! list, each with the [`SampleLook`] that describes exactly what its sample looks like. The entries of a legend are
//! then chosen: the named artists of its axes, merged by name and look in a legend panel ([`panel_entries`]), or the
//! items of an explicit list, in their order. The entries are measured ([`Listing`]) and arranged in columns, with
//! the items on a row of their own between groups of columns ([`arrange`]), and the block of entries is placed and
//! drawn ([`render`]).

use ironlab_ir::{
    Artist, ColorSpec, Interpreter, Legend, LegendEntries, LegendEntryColumns, LegendEntryLayout,
    LegendEntryOrder, LegendItem, LegendItemContent, LegendLocation, LegendSample, LineStyle,
    MarkerStyle, NodeId, PanelLegend, ScatterSize, Text,
};

use crate::display::{Item, ItemKind, Point, Rect, Rgba, Stroke};
use crate::hit::{HitMap, LegendHit, LegendOwner};
use crate::maths::quiver;

use super::artists::AxesInput;
use super::axes2d;
use super::data::{ArtistData, Prepared};
use super::decor::TICK_SCALE;
use super::image;
use super::paths::{self, PathBuilder};
use super::style::{self, ColourScale, INK, Paint};
use super::text::{TextBlock, measure};
use super::{Ctx, SceneWarning, SceneWarningKind};

/// The alpha factor applied to every colour of a hidden artist's entry.
const HIDDEN_ALPHA: f32 = 0.35;

/// The amount, in points, by which a block of entries may exceed the area available to it before it overflows.
///
/// A fitted row or column is sized to hold its block exactly, and the area of the panel is then recomputed from the
/// sizes of the tracks, which can lose a few units in the last place; the tolerance keeps that rounding from being
/// reported as an overflow.
const FIT_TOLERANCE: f64 = 1e-6;

/// The label of an item that names an artist without a display name.
static NO_NAME: Text = Text {
    content: String::new(),
    interpreter: Interpreter::None,
};

/// What the sample of a legend entry draws, with every colour resolved.
///
/// Two samples are equal exactly when they draw the same picture, so that a legend panel merges entries of the same
/// name only when the reader cannot tell them apart. Every value is finite, because non-finite sizes and widths are
/// replaced before they are stored, so the derived comparison is exact. Visibility is not part of the sample: it
/// greys an entry but does not change what the entry stands for.
#[derive(Clone, Debug, PartialEq)]
enum SampleLook {
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

/// An artist of an axes that a legend can list: one with usable data, named or not.
pub(super) struct Candidate<'a> {
    axes: NodeId,
    artist: NodeId,
    name: Option<&'a Text>,
    sample: SampleLook,
    visible: bool,
}

/// One entry of a legend: a label, its sample, and the artists it stands for.
pub(super) struct Entry<'a> {
    name: &'a Text,
    sample: SampleLook,
    /// The `(axes, artist)` pairs of the artists the entry stands for, in the order the legend lists them. An item
    /// that stands for no artist has none, and is neither greyed nor a click target.
    targets: Vec<(NodeId, NodeId)>,
    /// Whether any of the artists the entry stands for is visible; an entry is greyed when none is.
    visible: bool,
    /// Whether the entry spans every column, on a row of its own.
    full_row: bool,
}

impl<'a> Entry<'a> {
    /// Returns the entry of a single artist.
    fn of(candidate: &Candidate<'a>, full_row: bool) -> Self {
        Self {
            name: candidate.name.unwrap_or(&NO_NAME),
            sample: candidate.sample.clone(),
            targets: vec![(candidate.axes, candidate.artist)],
            visible: candidate.visible,
            full_row,
        }
    }

    /// Returns the entry of an item that stands for no artist.
    fn custom(label: &'a Text, sample: &LegendSample, full_row: bool) -> Self {
        Self {
            name: label,
            sample: custom_sample(sample),
            targets: Vec::new(),
            visible: true,
            full_row,
        }
    }
}

/// The entries of a legend with their labels measured, ready to be arranged.
pub(super) struct Listing<'a> {
    entries: Vec<Entry<'a>>,
    labels: Vec<TextBlock>,
}

impl<'a> Listing<'a> {
    /// Measures the labels of the entries of the legend whose items name `source`; a label that cannot be typeset
    /// is reported against the first artist the entry stands for, or against `source` for an item that stands for
    /// no artist.
    pub(super) fn new(ctx: &mut Ctx, source: NodeId, entries: Vec<Entry<'a>>) -> Self {
        let size = TICK_SCALE * ctx.font_size;
        let labels = entries
            .iter()
            .map(|e| {
                let owner = e.targets.first().map_or(source, |&(_, artist)| artist);
                measure(ctx, e.name, size, owner)
            })
            .collect();
        Self { entries, labels }
    }

    /// Returns the size of the block of entries, padding included, arranged as `mode` chooses within `max_width ×
    /// max_height`.
    /// `font_size` is the base font size of the figure.
    pub(super) fn size(
        &self,
        font_size: f64,
        layout: LegendEntryLayout,
        mode: Fill,
        max: (f64, f64),
    ) -> (f64, f64) {
        if self.entries.is_empty() {
            return (0.0, 0.0);
        }
        let grid = arrange(self, layout, mode, max.0, max.1, &Metrics::new(font_size));
        (grid.width, grid.height)
    }
}

/// How automatic columns are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fill {
    /// The fewest columns in which the block fits the height available, as long as the block fits the width.
    Height,
    /// The fewest rows that the width available allows, in the fewest columns that give that many rows. A legend
    /// panel in fitted rows alone, whose height follows its entries, is arranged in this way.
    Width,
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

/// Returns the inset that a legend panel keeps between its border and its entries, for the base font size
/// `font_size`: the inset of an inside legend when the panel is boxed, and none otherwise.
pub(super) fn panel_inset(font_size: f64, panel: &PanelLegend) -> f64 {
    // Why: entries against a border look cramped and read as part of the frame, so a boxed panel keeps them the
    // inset an inside legend keeps from the box of its axes; an unboxed panel has no frame to clear, and its block
    // sits flush with the edges it is aligned to.
    if panel.boxed {
        Metrics::new(font_size).inset
    } else {
        0.0
    }
}

/// Where an entry lies in the arrangement of a legend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Place {
    row: usize,
    /// The column of the entry, or `None` for an entry that spans every column on a row of its own.
    column: Option<usize>,
}

/// The arrangement of the entries of a legend in rows and columns.
struct Grid {
    /// The place of each entry, in the order of the entries.
    places: Vec<Place>,
    /// The width of the widest label in each column, among the entries that do not span every column.
    label_widths: Vec<f64>,
    /// The height of each row, the gap below it included.
    row_heights: Vec<f64>,
    /// The size of the block of entries, its padding included.
    width: f64,
    height: f64,
}

/// Describes every artist of an axes with usable data, named or not, in artist order, hidden artists included.
pub(super) fn candidates<'a>(
    axes: &'a ironlab_ir::Axes,
    prepared: &[Prepared<'a>],
    colours: &ColourScale,
    primaries: &[Paint],
) -> Vec<Candidate<'a>> {
    prepared
        .iter()
        .zip(primaries)
        .filter_map(|(p, primary)| {
            let data = p.data.as_ref()?;
            let colour = primary.single(colours);
            Some(Candidate {
                axes: axes.id,
                artist: p.artist.id(),
                name: p.artist.display_name(),
                sample: sample(p.artist, data, colour, colours.middle()),
                visible: p.artist.visible(),
            })
        })
        .collect()
}

/// Returns the entries of a legend inside the plot area of an axes: its named artists in artist order, or the items
/// of its explicit list in their order, each artist item naming an artist of the axes. Nothing is merged.
fn inside_entries<'a>(entries: &'a LegendEntries, own: &[Candidate<'a>]) -> Vec<Entry<'a>> {
    match entries {
        LegendEntries::Auto => own
            .iter()
            .filter(|c| c.name.is_some())
            .map(|c| Entry::of(c, false))
            .collect(),
        LegendEntries::Explicit(items) => items
            .iter()
            .filter_map(|item| match &item.content {
                LegendItemContent::Artist(id) => own
                    .iter()
                    .find(|c| c.artist == *id)
                    .map(|c| Entry::of(c, item.full_row)),
                LegendItemContent::Custom { label, sample } => {
                    Some(Entry::custom(label, sample, item.full_row))
                }
            })
            .collect(),
    }
}

/// Returns the entries of a legend panel.
///
/// `gathered` holds the candidates of the axes that name the panel, in the reading order of their cells, and
/// `figure` the candidates of every axes of the figure. Automatic entries are the named candidates gathered, with
/// those of the same name and look merged into one. An explicit list gives exactly its items, in their order: an
/// artist item takes the artist from any axes of the figure, and also stands for every gathered candidate of the
/// same display name and look, so that clicking it acts on the same series in every axes that names the panel.
pub(super) fn panel_entries<'a>(
    ctx: &mut Ctx,
    panel: &'a PanelLegend,
    gathered: &[&Candidate<'a>],
    figure: &[Vec<Candidate<'a>>],
) -> Vec<Entry<'a>> {
    match &panel.entries {
        LegendEntries::Auto => {
            let entries = gathered
                .iter()
                .filter(|c| c.name.is_some())
                .map(|c| Entry::of(c, false))
                .collect();
            merge(ctx, LegendOwner::Panel(panel.id), entries)
        }
        LegendEntries::Explicit(items) => items
            .iter()
            .filter_map(|item| explicit_panel_entry(item, gathered, figure))
            .collect(),
    }
}

/// Returns the entry of one item of the explicit list of a legend panel, or `None` for an item naming an artist
/// that the figure does not have, or whose data cannot be used.
fn explicit_panel_entry<'a>(
    item: &'a LegendItem,
    gathered: &[&Candidate<'a>],
    figure: &[Vec<Candidate<'a>>],
) -> Option<Entry<'a>> {
    match &item.content {
        LegendItemContent::Artist(id) => {
            let named = figure.iter().flatten().find(|c| c.artist == *id)?;
            let mut entry = Entry::of(named, item.full_row);
            if named.name.is_some() {
                for twin in gathered {
                    if twin.artist != named.artist
                        && twin.name == named.name
                        && twin.sample == named.sample
                    {
                        entry.targets.push((twin.axes, twin.artist));
                        entry.visible |= twin.visible;
                    }
                }
            }
            Some(entry)
        }
        LegendItemContent::Custom { label, sample } => {
            Some(Entry::custom(label, sample, item.full_row))
        }
    }
}

/// Draws the legend inside the plot area of an axes, if it has one with at least one entry, and records the entries
/// that stand for artists in the hit map.
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
    let Some(Legend::Inside(legend)) = &axes.legend else {
        return;
    };
    let own = candidates(axes, input.prepared, &input.colours, primaries);
    let listing = Listing::new(ctx, axes.id, inside_entries(&legend.entries, &own));
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
            fill: Fill::Height,
            boxed: legend.boxed,
            available,
        },
        &listing,
        |width, height| place(plot, width, height, legend.location, m.inset, data_points),
        out,
        hits,
    );
}

/// Draws a legend panel in `area` with its listing, arranging automatic columns as `fill` says, and records the
/// entries that stand for artists in the hit map.
///
/// A boxed panel outlines `area` as an axes outlines its plot area, and keeps its entries the legend inset inside
/// that border. A boxed panel with no entries draws only its border, and an unboxed one draws nothing.
pub(super) fn draw_panel(
    ctx: &mut Ctx,
    panel: &PanelLegend,
    area: Rect,
    listing: &Listing,
    fill: Fill,
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    // Why: the panel's area is aligned with the plot areas of the axes in its rows and columns, so a border drawn
    // as their box is drawn continues the edges of their boxes.
    if panel.boxed {
        out.extend(axes2d::frame(panel.id, area));
    }
    // A panel holds no data to avoid, so the best location is the centre.
    let align = match panel.align {
        LegendLocation::Best => LegendLocation::Center,
        align => align,
    };
    let inset = panel_inset(ctx.font_size, panel);
    let available = Rect::new(
        area.x + inset,
        area.y + inset,
        (area.width - 2.0 * inset).max(0.0),
        (area.height - 2.0 * inset).max(0.0),
    );
    render(
        ctx,
        Block {
            owner: LegendOwner::Panel(panel.id),
            source: panel.id,
            layout: panel.layout,
            fill,
            boxed: false,
            available,
        },
        listing,
        |width, height| place(area, width, height, align, inset, &[]),
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
    /// How automatic columns are chosen.
    fill: Fill,
    /// Whether the block is drawn with a background and outline, as an inside legend may be; the border of a panel
    /// is drawn around the panel, not the block.
    boxed: bool,
    /// The area the block must fit: the plot area less the legend inset, or the area of a legend panel less the
    /// legend inset when the panel has a border.
    available: Rect,
}

/// Arranges, draws and records a block of measured entries.
///
/// `place` returns the rectangle of a block of the given width and height. When the block does not fit the
/// available area even in as many columns as fit its width, a [`SceneWarningKind::LegendOverflow`] is reported, the
/// block is drawn from the top-left corner of the area inside a group clipped to it, and the entries that do not lie
/// wholly inside the area are left out. Only the entries that stand for artists are recorded in the hit map.
fn render(
    ctx: &mut Ctx,
    block: Block,
    listing: &Listing,
    place: impl FnOnce(f64, f64) -> Rect,
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    let entries = &listing.entries;
    if entries.is_empty() {
        return;
    }
    let m = Metrics::new(ctx.font_size);
    let area = block.available;
    let grid = arrange(
        listing,
        block.layout,
        block.fill,
        area.width,
        area.height,
        &m,
    );
    let overflow =
        grid.width > area.width + FIT_TOLERANCE || grid.height > area.height + FIT_TOLERANCE;
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
    // An entry on a row of its own spans the block, its padding aside.
    let full_width = grid.width - 2.0 * m.pad;
    for ((entry, label), place) in entries.iter().zip(&listing.labels).zip(&grid.places) {
        let (y, h) = (row_y[place.row], grid.row_heights[place.row]);
        let (x, width) = match place.column {
            Some(col) => (column_x[col], m.column_width(grid.label_widths[col])),
            None => (rect.x + m.pad, full_width),
        };
        let hit = Rect::new(x - m.pad / 2.0, y, width + m.pad, h);
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
        // An item that stands for no artist has nothing to hide or show, so it is not a click target.
        if !entry.targets.is_empty() {
            hits.legend_entries.push(LegendHit {
                legend: block.owner,
                targets: entry.targets.clone(),
                rect: hit,
            });
        }
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
/// A fixed count of columns is used as given, but never more columns than there are entries outside rows of their
/// own. Automatic columns are chosen as `fill` says. With [`Fill::Height`] they are the fewest for which the block
/// fits the height; when no count fits the height, they are as many as fit the width, and at least one. With
/// [`Fill::Width`] they give the fewest rows of any count that fits the width, and are the fewest columns that give
/// that many rows; when no count fits the width, there is one. Rows of their own count towards the height whatever
/// the number of columns. The caller compares the block with the available size to detect an overflow.
fn arrange(
    listing: &Listing,
    layout: LegendEntryLayout,
    fill: Fill,
    max_width: f64,
    max_height: f64,
    m: &Metrics,
) -> Grid {
    let heights: Vec<f64> = listing
        .labels
        .iter()
        .map(|l| l.total_height().max(m.min_row) + m.row_gap)
        .collect();
    let widths: Vec<f64> = listing.labels.iter().map(TextBlock::width).collect();
    let full: Vec<bool> = listing.entries.iter().map(|e| e.full_row).collect();
    let n = full.iter().filter(|f| !**f).count().max(1);
    let grid = |columns: usize| grid(&heights, &widths, &full, columns, layout.order, m);
    match (layout.columns, fill) {
        (LegendEntryColumns::Count(columns), _) => grid((columns.max(1) as usize).min(n)),
        (LegendEntryColumns::Auto, Fill::Height) => {
            let mut chosen = grid(1);
            for columns in 2..=n {
                if chosen.height <= max_height + FIT_TOLERANCE {
                    break;
                }
                let wider = grid(columns);
                if wider.width > max_width + FIT_TOLERANCE {
                    break;
                }
                chosen = wider;
            }
            chosen
        }
        (LegendEntryColumns::Auto, Fill::Width) => {
            let mut chosen = grid(1);
            for columns in 2..=n {
                let wider = grid(columns);
                if wider.width > max_width + FIT_TOLERANCE {
                    break;
                }
                if wider.row_heights.len() < chosen.row_heights.len() {
                    chosen = wider;
                }
            }
            chosen
        }
    }
}

/// Places entries of the given row heights and label widths in at most `columns` columns, in the given order.
///
/// An entry marked in `full` takes a row of its own, at its position in the list, spanning every column. The entries
/// between such rows form a group that fills the columns on its own: every column of a group but the last holds the
/// same number of rows. The rows are as tall as their tallest entry and the columns as wide as their widest, across
/// every group, so that entries line up both across and down; the label of an entry on a row of its own sets no
/// column's width.
fn grid(
    heights: &[f64],
    widths: &[f64],
    full: &[bool],
    columns: usize,
    order: LegendEntryOrder,
    m: &Metrics,
) -> Grid {
    let columns = columns.max(1);
    let mut places = Vec::with_capacity(full.len());
    let mut row = 0;
    let mut k = 0;
    while k < full.len() {
        if full[k] {
            places.push(Place { row, column: None });
            row += 1;
            k += 1;
            continue;
        }
        let size = full[k..].iter().take_while(|f| !**f).count();
        let rows = size.div_ceil(columns);
        for j in 0..size {
            let (r, c) = match order {
                LegendEntryOrder::ColumnMajor => (j % rows, j / rows),
                LegendEntryOrder::RowMajor => (j / columns, j % columns),
            };
            places.push(Place {
                row: row + r,
                column: Some(c),
            });
        }
        row += rows;
        k += size;
    }
    let used = places
        .iter()
        .filter_map(|p| p.column.map(|c| c + 1))
        .max()
        .unwrap_or(0);
    let mut label_widths = vec![0.0f64; used];
    let mut row_heights = vec![0.0f64; row];
    let mut full_width = 0.0f64;
    for (k, place) in places.iter().enumerate() {
        match place.column {
            Some(col) => label_widths[col] = label_widths[col].max(widths[k]),
            None => full_width = full_width.max(m.column_width(widths[k])),
        }
        row_heights[place.row] = row_heights[place.row].max(heights[k]);
    }
    let columns_width = label_widths.iter().map(|w| m.column_width(*w)).sum::<f64>()
        + used.saturating_sub(1) as f64 * m.column_gap;
    let width = 2.0 * m.pad + columns_width.max(full_width);
    let height = 2.0 * m.pad + row_heights.iter().sum::<f64>();
    Grid {
        places,
        label_widths,
        row_heights,
        width,
        height,
    }
}

/// Returns the look of the sample of an item that stands for no artist.
///
/// A line sample is drawn as the sample of a line artist is: its automatic colour is the ink of the labels, because
/// the item belongs to no axes whose colour order could supply one, and its marker's automatic colours take the line
/// colour, or the ink when the line has no colour. A swatch is a patch of its colour, without an outline.
fn custom_sample(sample: &LegendSample) -> SampleLook {
    match *sample {
        LegendSample::Line { line, marker } => {
            let colour = match line.color {
                ColorSpec::None => None,
                ColorSpec::Rgba { color } => Some(style::ir_colour(color)),
                ColorSpec::Auto | ColorSpec::Colormapped => Some(INK),
            };
            SampleLook::Line {
                stroke: sample_stroke(&line, colour),
                marker: marker_sample(
                    marker,
                    marker.size_pt,
                    Some(colour.unwrap_or(INK)),
                    style::width_or(line.width_pt, 0.75).clamp(0.5, 1.5),
                ),
            }
        }
        LegendSample::Swatch { color } => SampleLook::Patch {
            fill: Some(style::ir_colour(color)),
            edge: None,
        },
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
fn sample(artist: &Artist, data: &ArtistData, colour: Option<Rgba>, middle: Rgba) -> SampleLook {
    match artist {
        Artist::Line(line) => SampleLook::Line {
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
            SampleLook::Marker(marker_sample(scatter.marker, size, colour, 0.5))
        }
        Artist::Quiver(q) => {
            SampleLook::Arrow(sample_stroke(&q.line, colour).map(|stroke| Stroke {
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
                SampleLook::Patch {
                    fill: Some(middle),
                    edge: explicit,
                }
            } else {
                let colour = match c.line.color {
                    ColorSpec::None => None,
                    ColorSpec::Rgba { color } => Some(style::ir_colour(color)),
                    ColorSpec::Auto | ColorSpec::Colormapped => Some(middle),
                };
                SampleLook::Line {
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
            SampleLook::Patch {
                fill: paint(s.face),
                edge: paint(s.edge),
            }
        }
        // The mean colour of the raster, so that the patch resembles it; an image with no pixel to average, every
        // pixel being transparent, has no patch.
        Artist::Image(_) => SampleLook::Patch {
            fill: match data {
                ArtistData::Image(image) => image::mean_colour(image),
                _ => None,
            },
            edge: None,
        },
        // The middle colour of the colormap, as for other colormapped artists without a single value: a stopgap
        // for the colormap redesign, until a colourbar (issue #8) shows the scale itself.
        Artist::IndexedImage(_) | Artist::MappedImage(_) => SampleLook::Patch {
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
fn draw_sample(source: NodeId, sample: &SampleLook, cell: Rect, alpha: f32, out: &mut Vec<Item>) {
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
        SampleLook::Line { stroke, marker: m } => {
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
        SampleLook::Marker(m) => {
            if let Some(m) = m {
                marker(m, out);
            }
        }
        SampleLook::Arrow(stroke) => {
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
        SampleLook::Patch { fill, edge } => {
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
