//! Legends: listing the entries of a legend, merging the entries a legend panel gathers from several axes, arranging
//! entries in columns, and drawing them inside a plot area or a legend panel.
//!
//! Both kinds of legend run the same four steps. [`candidates`] describes every artist of one axes that a legend can
//! list, each with the [`SampleLook`] that describes exactly what its sample looks like. The entries of a legend are
//! then chosen: the named artists of its axes, merged by name and look in a legend panel ([`panel_entries`]), or the
//! items of an explicit list, in their order, a colour scale among them becoming the entries of its categories or a
//! bar item ([`scale_item`]). The entries are measured ([`Listing`]) and arranged in columns, with the items on a row
//! of their own between groups of columns ([`arrange`]), and the block of entries is placed and drawn ([`render`]).

use std::borrow::Cow;
use std::collections::HashSet;

use ironlab_ir::{
    Artist, Color, ColorScaleKind, ColorSpec, DashStyle, Interpreter, Legend, LegendEntries,
    LegendEntryColumns, LegendEntryLayout, LegendEntryOrder, LegendItem, LegendItemContent,
    LegendLocation, LegendSample, LineStyle, MarkerStyle, NodeId, Panel, PanelLegend,
    PanelLocation, ScalarImage, Series, Text, TitleAlign,
};

use crate::display::{Item, ItemKind, Point, Rect, Rgba, Stroke};
use crate::hit::{ColorbarKeyHit, HitMap, LegendHit, LegendOwner};
use crate::maths::quiver;

use super::artists::AxesInput;
use super::axes2d;
use super::colorbar::{self, LegendBar, SWATCH_OUTLINE};
use super::data::{ArtistData, Prepared, SeriesColours};
use super::decor::TICK_SCALE;
use super::image;
use super::layout::Along;
use super::panel;
use super::paths::{self, PathBuilder};
use super::style::{self, ColourScales, INK, Paint, ResolvedScale};
use super::text::{TextBlock, measure, measure_str};
use super::{Ctx, SceneWarning, SceneWarningKind};

/// The alpha factor applied to every colour of a hidden artist's entry.
const HIDDEN_ALPHA: f32 = 0.35;

/// The amount, in points, by which a block of entries may exceed the area available to it before it overflows.
///
/// A fitted row or column is sized to hold its block exactly, and the area of the panel is then recomputed from the
/// sizes of the tracks, which can lose a few units in the last place; the tolerance keeps that rounding from being
/// reported as an overflow.
const FIT_TOLERANCE: f64 = 1e-6;

/// The label of an item that names an artist without a label.
static NO_NAME: Text = Text {
    content: String::new(),
    interpreter: Interpreter::None,
};

/// The most entry rows along which a standing bar item runs, whatever it asks for: far more than any page holds, so
/// that an absurd count lays out a bar too long for the legend, which then overflows, rather than exhausting memory.
const MAX_BAR_ROWS: u32 = 1000;

/// The number of segments in which a line sample sweeps a scale of many colours, enough that no step shows.
const LINE_SWEEP_SEGMENTS: usize = 32;

/// The number of markers across the sample of a series that draws only markers coloured by data.
const SWEEP_MARKERS: usize = 3;

/// A colour that paints nothing, held by the stroke of a line sample whose colours a sweep gives.
const NO_COLOUR: Rgba = Rgba::new(0.0, 0.0, 0.0, 0.0);

/// What the sample of a legend entry draws, with every colour resolved.
///
/// Two samples are equal exactly when they draw the same picture, so that a legend panel merges entries of the same
/// name only when the reader cannot tell them apart. Every value is finite, because non-finite sizes and widths are
/// replaced before they are stored, so the derived comparison is exact. Visibility is not part of the sample: it
/// greys an entry but does not change what the entry stands for.
#[derive(Clone, Debug, PartialEq)]
enum SampleLook {
    /// A horizontal line across the sample, with a marker at its centre (a series that draws a line, or contour
    /// isolines). A line that colour data colours draws a sweep of its scale, in segments from the low end at the
    /// left, and its stroke then gives only its width and dashes.
    Line {
        stroke: Option<Stroke>,
        sweep: Option<Sweep>,
        marker: Option<MarkerSample>,
    },
    /// A single marker at the centre of the sample (a series that draws no line), or a row of markers across it
    /// when the marker follows a sweep.
    Marker(Option<MarkerSample>),
    /// A horizontal arrow across the sample (a quiver).
    Arrow(Option<Stroke>),
    /// A filled rectangle across the sample, with an optional outline (a true-colour image, and a colour-mapped
    /// artist without a usable scale).
    Patch {
        fill: Option<Rgba>,
        edge: Option<Rgba>,
    },
    /// A sweep of a colour scale across the sample, with an optional outline (contour bands, colour-mapped surfaces
    /// and scalar images).
    Sweep { sweep: Sweep, edge: Option<Rgba> },
    /// A square of one colour, or empty, with a thin outline, smaller than its row: a custom swatch, a category of a
    /// categorical scale or one of its keys, and a surface whose faces are one colour, outlined in its edge colour.
    Swatch {
        fill: Option<Rgba>,
        outline: Option<Rgba>,
    },
}

/// The colours of a sweep of a colour scale, from its low end at the left to its high end at the right, each over
/// an equal share of the sample, with the opacity of the artist taken in (see [`colorbar::sweep`]). Two sweeps are
/// equal exactly when they show the same colours, so that a legend panel merges two plots of the same name exactly
/// when they sweep the same colours.
#[derive(Clone, Debug, PartialEq)]
struct Sweep(Vec<Option<Rgba>>);

impl Sweep {
    /// Returns the sweep of `scale`, faded by `opacity`.
    fn of(scale: &ResolvedScale, opacity: f64) -> Self {
        let factor = style::unit_opacity(opacity) as f32;
        Self(
            colorbar::sweep(scale)
                .into_iter()
                .map(|c| c.map(|c| c.with_alpha_factor(factor)))
                .collect(),
        )
    }

    /// Returns the colour at the fraction `f` across the sweep.
    fn at(&self, f: f64) -> Option<Rgba> {
        let n = self.0.len();
        let k = (f * n as f64).floor().max(0.0) as usize;
        self.0.get(k.min(n.checked_sub(1)?)).copied().flatten()
    }

    /// Returns the colours of the sweep, each faded by `alpha`.
    fn faded(&self, alpha: f32) -> Vec<Option<Rgba>> {
        self.0
            .iter()
            .map(|c| c.map(|c| c.with_alpha_factor(alpha)))
            .collect()
    }
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
    /// The sweep that a marker following colour data takes its `Auto` and `Colormapped` colours from, at its place
    /// across the sample, instead of `auto`.
    sweep: Option<Sweep>,
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
    name: Cow<'a, Text>,
    sample: SampleLook,
    /// The `(axes, artist)` pairs of the artists the entry stands for, in the order the legend lists them. An item
    /// that stands for no artist has none, and is neither greyed nor a click target.
    targets: Vec<(NodeId, NodeId)>,
    /// Whether any of the artists the entry stands for is visible; an entry is greyed when none is.
    visible: bool,
    /// Whether the entry spans every column, on a row of its own.
    full_row: bool,
    /// Whether the entry is a category of a categorical scale, or one of its keys, which an explicit list merges
    /// with any other entry of the same label and look.
    category: bool,
}

impl<'a> Entry<'a> {
    /// Returns the entry of a single artist.
    fn of(candidate: &Candidate<'a>, full_row: bool) -> Self {
        Self {
            name: Cow::Borrowed(candidate.name.unwrap_or(&NO_NAME)),
            sample: candidate.sample.clone(),
            targets: vec![(candidate.axes, candidate.artist)],
            visible: candidate.visible,
            full_row,
            category: false,
        }
    }

    /// Returns the entry of an item that stands for no artist.
    fn custom(label: &'a Text, sample: &LegendSample, full_row: bool) -> Self {
        Self {
            name: Cow::Borrowed(label),
            sample: custom_sample(sample),
            targets: Vec::new(),
            visible: true,
            full_row,
            category: false,
        }
    }
}

/// What a legend lists, in order, before it is measured: an entry, or a bar item of a colour scale.
// On 32-bit targets, such as WebAssembly, the variants differ by less than the lint's threshold.
#[cfg_attr(
    target_pointer_width = "64",
    expect(
        clippy::large_enum_variant,
        reason = "a legend lists a few members, each moved once into its listing"
    )
)]
pub(super) enum Chosen<'a> {
    Entry(Entry<'a>),
    Bar {
        scale: &'a ResolvedScale<'a>,
        full_row: bool,
        /// The number of entry rows along which a standing bar runs.
        rows: usize,
    },
}

/// One measured member of a legend: an entry with its label, or a bar item.
enum Listed<'a> {
    Entry { entry: Entry<'a>, label: TextBlock },
    Bar(LegendBar<'a>),
}

impl Listed<'_> {
    /// Returns whether the member spans every column, on a row of its own.
    fn full_row(&self) -> bool {
        match self {
            Listed::Entry { entry, .. } => entry.full_row,
            Listed::Bar(bar) => bar.lying(),
        }
    }
}

/// The entries of a legend with their labels measured, ready to be arranged.
pub(super) struct Listing<'a> {
    items: Vec<Listed<'a>>,
    /// The height of the row of an entry with a one-line label, its gap included, which each row of a standing bar
    /// item takes.
    pitch: f64,
    /// The space that the row of an entry with a one-line label leaves above and below its label, half the gap
    /// between the labels of neighbouring rows. A lying bar item leaves it above and below itself, so that it is as
    /// far from the labels of the entries around it as the labels of two entry rows are from each other.
    head: f64,
}

impl<'a> Listing<'a> {
    /// Measures the labels of the entries and the bar items of the legend `owner`, whose items name `source`; a label
    /// that cannot be typeset is reported against the first artist the entry stands for, or against `source` for an
    /// item that stands for no artist.
    pub(super) fn new(
        ctx: &mut Ctx,
        owner: LegendOwner,
        source: NodeId,
        chosen: Vec<Chosen<'a>>,
    ) -> Self {
        let size = TICK_SCALE * ctx.font_size;
        let m = Metrics::new(ctx.font_size);
        // A one-line label is as tall as the font is, whatever it says.
        let line = measure_str(&mut ctx.scratch(), "0", false, size, source).total_height();
        let pitch = line.max(m.min_row) + m.row_gap;
        let head = (pitch - line) / 2.0;
        let items = chosen
            .into_iter()
            .map(|c| match c {
                Chosen::Entry(entry) => {
                    let who = entry.targets.first().map_or(source, |&(_, artist)| artist);
                    let label = measure(ctx, &entry.name, size, who);
                    Listed::Entry { entry, label }
                }
                // The ends of a standing bar are kept half a label inside its rows, so that the tick labels at its
                // ends, centred on them, stay clear of the entries above and below.
                Chosen::Bar {
                    scale,
                    full_row,
                    rows,
                } => Listed::Bar(LegendBar::new(
                    ctx,
                    owner,
                    scale,
                    full_row,
                    rows,
                    pitch,
                    line / 2.0,
                    m.sample_width,
                )),
            })
            .collect();
        Self { items, pitch, head }
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
        if self.items.is_empty() {
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

/// Returns the box of a boxed inside legend drawn at `rect`: a white background with a thin frame, which keeps what is
/// drawn on it readable over the artists beneath. An inside colourbar draws the same box behind itself.
pub(super) fn backdrop(source: NodeId, rect: Rect) -> Option<Item> {
    let mut b = PathBuilder::new();
    b.rect(rect);
    paths::item(
        source,
        b.finish(),
        Some(paths::fill(Rgba::WHITE)),
        Some(paths::solid(INK, 0.5)),
    )
}

/// Returns the padding between the box of an inside legend and its entries, and the inset of that box from the edges
/// of the plot area, for the base font size `fs`; an inside colourbar is boxed and placed with the same distances.
pub(super) fn inside_box_spacing(fs: f64) -> (f64, f64) {
    let m = Metrics::new(fs);
    (m.pad, m.inset)
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
    /// The side of a swatch, smaller than the shortest row so that a margin separates the swatches of neighbouring
    /// rows.
    swatch: f64,
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
            swatch: 0.8 * fs,
        }
    }

    /// Returns the width of a column of entries whose widest label is `label_width` wide.
    fn column_width(&self, label_width: f64) -> f64 {
        self.sample_width + self.gap + label_width
    }
}

/// Returns the inset that a legend panel keeps between its border and its entries, for the base font size
/// `font_size`: the inset of an inside legend when the panel is boxed, and none otherwise.
fn panel_inset(font_size: f64, legend: &PanelLegend) -> f64 {
    // Why: entries against a border look cramped and read as part of the frame, so a boxed panel keeps them the
    // inset an inside legend keeps from the box of its axes; an unboxed panel has no frame to clear, and its block
    // sits flush with the edges it is aligned to.
    if legend.boxed {
        Metrics::new(font_size).inset
    } else {
        0.0
    }
}

/// Where an entry lies in the arrangement of a legend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Place {
    /// The first row of the entry; a standing bar item runs down the rows of its keys and its bar from it.
    row: usize,
    /// The column of the entry, or `None` for an entry that spans every column on a row of its own.
    column: Option<usize>,
}

/// What the arrangement of a legend needs of one of its members.
struct Span {
    /// Whether the member spans every column, on a row of its own.
    full: bool,
    /// The height that the member needs of each row it runs down: one row for an entry, and for a standing bar item
    /// the rows of its bar and of its keys.
    heights: Vec<f64>,
    /// The width of the member's label, or of what stands for it, beyond the sample column; for a member on a row of
    /// its own, its whole width.
    width: f64,
}

/// The arrangement of the entries of a legend in rows and columns.
struct Grid {
    /// The place of each entry, in the order of the entries.
    places: Vec<Place>,
    /// The number of rows that each entry runs down, in the order of the entries.
    spans: Vec<usize>,
    /// The width of the widest label in each column, among the entries that do not span every column; negative for a
    /// column whose bar item is narrower than the sample column and its gap.
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
    scales: &ColourScales,
    primaries: &[Paint],
) -> Vec<Candidate<'a>> {
    prepared
        .iter()
        .zip(primaries)
        .filter_map(|(p, primary)| {
            let data = p.data.as_ref()?;
            // A colour-mapped primary colour without a number to map is drawn in the middle of viridis, as the
            // artist itself is; a colour that colour data varies is keyed by a sweep of the artist's scale.
            let colour = primary.single(style::viridis_middle());
            Some(Candidate {
                axes: axes.id,
                artist: p.artist.id(),
                name: p.artist.label(),
                sample: sample(p.artist, data, colour, scales.of(p.artist)),
                visible: p.artist.visible(),
            })
        })
        .collect()
}

/// Returns the entries of a legend inside the plot area of an axes: its named artists in artist order, or the items
/// of its explicit list in their order, each artist item naming an artist of the axes and each scale item any scale
/// of the figure. Nothing is merged: an item of a legend inside a plot area is listed as it is given, the entries of
/// the categories of a scale item among them.
fn inside_entries<'a>(
    entries: &'a LegendEntries,
    own: &[Candidate<'a>],
    scales: &'a ColourScales<'a>,
) -> Vec<Chosen<'a>> {
    match entries {
        LegendEntries::Auto => own
            .iter()
            .filter(|c| c.name.is_some())
            .map(|c| Chosen::Entry(Entry::of(c, false)))
            .collect(),
        LegendEntries::Explicit(items) => items
            .iter()
            .flat_map(|item| match &item.content {
                LegendItemContent::Artist(id) => own
                    .iter()
                    .find(|c| c.artist == *id)
                    .map(|c| Chosen::Entry(Entry::of(c, item.full_row)))
                    .into_iter()
                    .collect(),
                LegendItemContent::Custom { label, sample } => {
                    vec![Chosen::Entry(Entry::custom(label, sample, item.full_row))]
                }
                LegendItemContent::ColorScale { scale, rows } => {
                    scale_item(scales, *scale, item.full_row, *rows)
                }
            })
            .collect(),
    }
}

/// Returns what a scale item of a legend lists: one entry per category of a categorical scale, labelled with the
/// category's label beside a swatch of its colour, followed by an entry for each out-of-range key the scale shows,
/// in the order of a colourbar's keys; a bar item `rows` entry rows long for any other kind of scale (at least one row,
/// as validation requires); and nothing for an identifier that names no colour scale, which validation reports. A
/// categorical scale with no categories, which validation also reports, lists nothing.
fn scale_item<'a>(
    scales: &'a ColourScales<'a>,
    id: NodeId,
    full_row: bool,
    rows: u32,
) -> Vec<Chosen<'a>> {
    let Some(scale) = scales.get(id) else {
        return Vec::new();
    };
    match &scale.scale.kind {
        ColorScaleKind::Categorical(c) if c.categories.is_empty() => Vec::new(),
        ColorScaleKind::Categorical(_) => colorbar::category_swatches(scale)
            .into_iter()
            .map(|(fill, name)| {
                Chosen::Entry(Entry {
                    name,
                    sample: SampleLook::Swatch {
                        fill,
                        outline: Some(SWATCH_OUTLINE),
                    },
                    targets: Vec::new(),
                    visible: true,
                    full_row,
                    category: true,
                })
            })
            .collect(),
        _ => vec![Chosen::Bar {
            scale,
            full_row,
            rows: rows.clamp(1, MAX_BAR_ROWS) as usize,
        }],
    }
}

/// Merges each entry of a category into the first entry before it of the same label and look, which takes the
/// artists it stands for, and merges any entry of the same label and look into an earlier category entry; an entry
/// keeps the place of the first of those it merges. Two entries of which neither is a category are both listed, as
/// two items of an explicit list that repeat each other are.
fn merge_categories(chosen: Vec<Chosen<'_>>) -> Vec<Chosen<'_>> {
    let mut merged: Vec<Chosen> = Vec::with_capacity(chosen.len());
    for c in chosen {
        let Chosen::Entry(entry) = c else {
            merged.push(c);
            continue;
        };
        let twin = merged.iter_mut().find_map(|m| match m {
            Chosen::Entry(m)
                if (m.category || entry.category)
                    && m.name == entry.name
                    && m.sample == entry.sample =>
            {
                Some(m)
            }
            _ => None,
        });
        match twin {
            Some(m) => {
                m.targets.extend(entry.targets);
                m.visible |= entry.visible;
                m.category |= entry.category;
            }
            None => merged.push(Chosen::Entry(entry)),
        }
    }
    merged
}

/// The legend of a panel with its entries chosen and measured.
pub(super) struct PanelListing<'a> {
    legend: &'a PanelLegend,
    listing: Listing<'a>,
}

impl<'a> PanelListing<'a> {
    /// Chooses and measures the entries of the legend of the panel `panel`, from the candidates `gathered` of the
    /// axes that name the panel, the candidates `figure` of every axes and the colour scales of the figure, as
    /// [`panel_entries`] describes.
    pub(super) fn new(
        ctx: &mut Ctx,
        panel: NodeId,
        legend: &'a PanelLegend,
        gathered: &[&Candidate<'a>],
        figure: &[Vec<Candidate<'a>>],
        scales: &'a ColourScales<'a>,
    ) -> Self {
        let entries = panel_entries(ctx, panel, legend, gathered, figure, scales);
        Self {
            legend,
            listing: Listing::new(ctx, LegendOwner::Panel(panel), panel, entries),
        }
    }

    /// Returns the height (along [`Along::Rows`]) or width (along [`Along::Cols`]) of the area that the legend
    /// needs for its entries and the panel's title `heading` above them, its insets included, when the other
    /// dimension of the area is `cross`.
    pub(super) fn size(
        &self,
        font_size: f64,
        along: Along,
        cross: f64,
        heading: Option<&TextBlock>,
    ) -> f64 {
        let inset = panel_inset(font_size, self.legend);
        let m = Metrics::new(font_size);
        let part = heading_part(&m, heading, self.listing.items.is_empty());
        let (fill, max) = match along {
            Along::Rows => (Fill::Width, (cross - 2.0 * inset, f64::INFINITY)),
            Along::Cols => (Fill::Height, (f64::INFINITY, cross - 2.0 * inset - part)),
        };
        let (w, h) = self.listing.size(font_size, self.legend.layout, fill, max);
        2.0 * inset
            + match along {
                Along::Rows => h + part,
                Along::Cols => w.max(heading_width(&m, heading, self.listing.items.is_empty())),
            }
    }
}

/// Returns the height that the title of a legend panel takes above the block of entries: its line and one gap, the
/// space above it being the padding of the block. Above no entries the title takes only its line.
fn heading_part(m: &Metrics, heading: Option<&TextBlock>, empty: bool) -> f64 {
    heading.map_or(0.0, |h| h.total_height() + if empty { 0.0 } else { m.gap })
}

/// Returns the least width of a block of entries headed by the title of a legend panel: the title with half the
/// padding on either side, the margin by which the entries' click targets are inset in the block, which the title
/// is aligned with. Above no entries it is the title's width alone.
fn heading_width(m: &Metrics, heading: Option<&TextBlock>, empty: bool) -> f64 {
    heading.map_or(0.0, |h| h.width() + if empty { 0.0 } else { m.pad })
}

/// Returns the entries of the legend of a panel.
///
/// `gathered` holds the candidates of the axes that name the panel, in the reading order of their cells, and
/// `figure` the candidates of every axes of the figure. Automatic entries are the named candidates gathered, with
/// those of the same name and look merged into one. An explicit list gives exactly its items, in their order: an
/// artist item takes the artist from any axes of the figure, and also stands for every gathered candidate of the
/// same label and look, so that clicking it acts on the same series in every axes that names the panel; a scale item
/// lists the categories of a categorical scale or a bar item of any other (see [`scale_item`]), and the entries of
/// categories are merged with the entries of the same label and look (see [`merge_categories`]).
fn panel_entries<'a>(
    ctx: &mut Ctx,
    panel: NodeId,
    legend: &'a PanelLegend,
    gathered: &[&Candidate<'a>],
    figure: &[Vec<Candidate<'a>>],
    scales: &'a ColourScales<'a>,
) -> Vec<Chosen<'a>> {
    match &legend.entries {
        LegendEntries::Auto => {
            let entries = gathered
                .iter()
                .filter(|c| c.name.is_some())
                .map(|c| Entry::of(c, false))
                .collect();
            merge(ctx, LegendOwner::Panel(panel), entries)
                .into_iter()
                .map(Chosen::Entry)
                .collect()
        }
        LegendEntries::Explicit(items) => merge_categories(
            items
                .iter()
                .flat_map(|item| explicit_panel_entry(item, gathered, figure, scales))
                .collect(),
        ),
    }
}

/// Returns what one item of the explicit list of a legend panel lists: nothing for an item naming an artist that
/// the figure does not have, or whose data cannot be used.
fn explicit_panel_entry<'a>(
    item: &'a LegendItem,
    gathered: &[&Candidate<'a>],
    figure: &[Vec<Candidate<'a>>],
    scales: &'a ColourScales<'a>,
) -> Vec<Chosen<'a>> {
    match &item.content {
        LegendItemContent::Artist(id) => {
            let Some(named) = figure.iter().flatten().find(|c| c.artist == *id) else {
                return Vec::new();
            };
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
            vec![Chosen::Entry(entry)]
        }
        LegendItemContent::Custom { label, sample } => {
            vec![Chosen::Entry(Entry::custom(label, sample, item.full_row))]
        }
        LegendItemContent::ColorScale { scale, rows } => {
            scale_item(scales, *scale, item.full_row, *rows)
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
    let own = candidates(axes, input.prepared, input.scales, primaries);
    let chosen = inside_entries(&legend.entries, &own, input.scales);
    let listing = Listing::new(ctx, LegendOwner::Axes(axes.id), axes.id, chosen);
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

/// Draws the legend of a panel in the panel's `area`, headed by the panel's title `heading` when it has one,
/// arranging automatic columns as `fill` says, and records the entries that stand for artists in the hit map.
///
/// A boxed legend outlines `area` as an axes outlines its plot area, and keeps its entries and its heading the legend
/// inset inside that border. The heading lies above the block of entries, aligned with the left edge, the centre or
/// the right edge of the entries' click targets as the panel's title alignment says; a block narrower than its
/// heading is aligned with the heading in the same way. A boxed legend with no entries draws only its border and its
/// heading, and an unboxed one only its heading.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_panel(
    ctx: &mut Ctx,
    panel: &Panel,
    content: &PanelListing,
    heading: Option<&TextBlock>,
    area: Rect,
    fill: Fill,
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    let PanelListing { legend, listing } = content;
    // Why: the panel's area is aligned with the plot areas of the axes in its rows and columns, so a border drawn
    // as their box is drawn continues the edges of their boxes.
    if legend.boxed {
        out.extend(axes2d::frame(panel.id, area));
    }
    let inset = panel_inset(ctx.font_size, legend);
    let m = Metrics::new(ctx.font_size);
    let empty = listing.items.is_empty();
    let part = heading_part(&m, heading, empty);
    let least = heading_width(&m, heading, empty);
    let available = Rect::new(
        area.x + inset,
        area.y + inset + part,
        (area.width - 2.0 * inset).max(0.0),
        (area.height - 2.0 * inset - part).max(0.0),
    );
    let aligned = |lo: f64, hi: f64, inner: f64| match panel.title_align {
        TitleAlign::Left => lo,
        TitleAlign::Center => lo + (hi - lo - inner) / 2.0,
        TitleAlign::Right => hi - inner,
    };
    let placed = render(
        ctx,
        Block {
            owner: LegendOwner::Panel(panel.id),
            source: panel.id,
            layout: legend.layout,
            fill,
            boxed: false,
            available,
        },
        listing,
        |width, height| {
            let whole = width.max(least);
            let r = panel::place(area, whole, height + part, panel.align, inset);
            Rect::new(aligned(r.x, r.right(), width), r.y + part, width, height)
        },
        out,
        hits,
    );
    let Some(heading) = heading else {
        return;
    };
    let (left, top) = match placed {
        // The title lies the block's padding below the top of the heading and block together, aligned with the
        // entries' click targets, which are inset half the padding in the block.
        Some(block) => (
            aligned(
                block.x + m.pad / 2.0,
                block.right() - m.pad / 2.0,
                heading.width(),
            ),
            block.y - part + m.pad,
        ),
        None => {
            let r = panel::place(
                area,
                heading.width(),
                heading.total_height(),
                panel.align,
                inset,
            );
            (r.x, r.y)
        }
    };
    let origin = Point::new(left, top + heading.height());
    heading.draw_on_page(ctx, origin, INK, panel.id, out);
}

/// Merges entries with the same label and the same sample into one entry that stands for all of their
/// artists, keeping the order in which each merged entry first appears.
///
/// Entries that share a label but not a sample are all listed, and one [`SceneWarningKind::LegendNameClash`]
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

/// Arranges, draws and records a block of measured entries, and returns the rectangle of the block, or `None` when
/// there are no entries to draw.
///
/// `place` returns the rectangle of a block of the given width and height. When the block does not fit the
/// available area even in as many columns as fit its width, a [`SceneWarningKind::LegendOverflow`] is reported, the
/// block is drawn from the top-left corner of the area inside a group clipped to it, and the entries that do not lie
/// wholly inside the area are left out. Only the entries that stand for artists are recorded in the hit map, and the
/// bar items are recorded as colourbars.
fn render(
    ctx: &mut Ctx,
    block: Block,
    listing: &Listing,
    place: impl FnOnce(f64, f64) -> Rect,
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) -> Option<Rect> {
    if listing.items.is_empty() {
        return None;
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
        items.extend(backdrop(
            block.source,
            if overflow {
                Rect::new(
                    rect.x,
                    rect.y,
                    rect.width.min(area.width),
                    rect.height.min(area.height),
                )
            } else {
                rect
            },
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
    let mut bars = HitMap::default();
    for ((member, place), span) in listing.items.iter().zip(&grid.places).zip(&grid.spans) {
        let y = row_y[place.row];
        let h: f64 = grid.row_heights[place.row..place.row + span].iter().sum();
        let (x, width) = match place.column {
            Some(col) => (column_x[col], m.column_width(grid.label_widths[col])),
            None => (rect.x + m.pad, full_width),
        };
        let hit = Rect::new(x - m.pad / 2.0, y, width + m.pad, h);
        if overflow && !contains(area, hit) {
            continue;
        }
        let (entry, label) = match member {
            Listed::Entry { entry, label } => (entry, label),
            // A lying bar item is drawn below the space that an entry row leaves above its label, as a label is
            // centred in its row.
            Listed::Bar(bar) if bar.lying() => {
                let r = Rect::new(x, y + listing.head, width, bar.height());
                bars.colorbars.push(bar.draw(ctx, r, &mut items));
                continue;
            }
            // A standing bar item runs down the rows of its bar, between the rows of the keys above and below it,
            // each of which is drawn as a swatch entry and recorded among the bar's keys.
            Listed::Bar(bar) => {
                let tops: Vec<f64> = grid.row_heights[place.row..place.row + span]
                    .iter()
                    .scan(y, |at, h| {
                        let top = *at;
                        *at += h;
                        Some(top)
                    })
                    .chain(std::iter::once(y + h))
                    .collect();
                let (keys, above) = bar.keys();
                let (first, last) = (above, above + bar.rows());
                let r = Rect::new(x, tops[first], width, tops[last] - tops[first]);
                let mut hit = bar.draw(ctx, r, &mut items);
                for (k, key) in keys.iter().enumerate() {
                    let row = if k < above { k } else { last + k - above };
                    let cell = Rect::new(x, tops[row], m.sample_width, tops[row + 1] - tops[row]);
                    let sample = SampleLook::Swatch {
                        fill: key.fill,
                        outline: Some(SWATCH_OUTLINE),
                    };
                    draw_sample(block.source, &sample, cell, 1.0, &m, &mut items);
                    draw_label(&key.label, cell, 1.0, &m, block.source, &mut items);
                    hit.keys.push(ColorbarKeyHit {
                        key: key.key,
                        rect: swatch_square(cell, &m),
                        label: key.text.clone(),
                    });
                }
                bars.colorbars.push(hit);
                continue;
            }
        };
        let alpha = if entry.visible { 1.0 } else { HIDDEN_ALPHA };
        let cell = Rect::new(x, y, m.sample_width, h);
        draw_sample(block.source, &entry.sample, cell, alpha, &m, &mut items);
        draw_label(label, cell, alpha, &m, block.source, &mut items);
        // An item that stands for no artist has nothing to hide or show, so it is not a click target.
        if !entry.targets.is_empty() {
            hits.legend_entries.push(LegendHit {
                legend: block.owner,
                targets: entry.targets.clone(),
                rect: hit,
            });
        }
    }
    hits.colorbars.extend(bars.colorbars);

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
    Some(rect)
}

/// Draws the label of an entry beside the sample cell `cell` of its row, centred across the row, faded by `alpha`.
fn draw_label(
    label: &TextBlock,
    cell: Rect,
    alpha: f32,
    m: &Metrics,
    source: NodeId,
    out: &mut Vec<Item>,
) {
    let origin = Point::new(
        cell.right() + m.gap,
        cell.y + (cell.height - label.total_height()) / 2.0 + label.height(),
    );
    label.draw(origin, INK.with_alpha_factor(alpha), source, out);
}

/// Returns the square of a swatch drawn in the sample cell `cell` of a row: as large as the swatch side, the cell's
/// width and its height allow, centred in the cell.
fn swatch_square(cell: Rect, m: &Metrics) -> Rect {
    let side = m.swatch.min(cell.width).min(cell.height);
    Rect::new(
        cell.x + (cell.width - side) / 2.0,
        cell.y + (cell.height - side) / 2.0,
        side,
        side,
    )
}

/// Returns whether `inner` lies inside `outer`.
fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

/// Returns what the arrangement needs of each member of a listing.
///
/// An entry needs one row as tall as its label, and at least the shortest row, with the gap below it. A standing bar
/// item needs the rows it asks for, each as tall as the row of an entry with a one-line label, with a row for each of
/// its keys above and below them, each as an entry row of the key's label, and is as wide beyond the sample column as
/// its bar, ticks, labels and title reach beyond it, and its key labels. A lying bar item needs one row as tall as
/// itself with the space an entry row leaves above its label above and below it, and the width it needs.
fn spans(listing: &Listing, m: &Metrics) -> Vec<Span> {
    let beyond_sample = m.column_width(0.0);
    listing
        .items
        .iter()
        .map(|member| match member {
            Listed::Entry { entry, label } => Span {
                full: entry.full_row,
                heights: vec![label.total_height().max(m.min_row) + m.row_gap],
                width: if entry.full_row {
                    m.column_width(label.width())
                } else {
                    label.width()
                },
            },
            Listed::Bar(bar) if bar.lying() => Span {
                full: true,
                heights: vec![bar.height() + 2.0 * listing.head],
                width: bar.width(),
            },
            Listed::Bar(bar) => {
                let (keys, above) = bar.keys();
                let row =
                    |key: &colorbar::KeyRow| key.label.total_height().max(m.min_row) + m.row_gap;
                let heights = keys[..above]
                    .iter()
                    .map(row)
                    .chain(std::iter::repeat_n(listing.pitch, bar.rows()))
                    .chain(keys[above..].iter().map(row))
                    .collect();
                // The bar's width beyond the sample column may be negative: a column that holds nothing but the bar
                // is then exactly as wide as the bar.
                let labels = keys
                    .iter()
                    .map(|k| k.label.width())
                    .fold(f64::NEG_INFINITY, f64::max);
                Span {
                    full: false,
                    heights,
                    width: (bar.width() - beyond_sample).max(labels),
                }
            }
        })
        .collect()
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
    let spans = spans(listing, m);
    let n = listing
        .items
        .iter()
        .filter(|item| !item.full_row())
        .count()
        .max(1);
    let grid = |columns: usize| grid(&spans, columns, layout.order, m);
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

/// Places the members of a group, which lie between rows of their own, in at most `columns` columns filled column by
/// column, and returns the row and column of each, relative to the group, with the number of rows of the group.
///
/// Every column but the last holds as many rows as the first, which are the fewest that let the members fit the
/// columns: a member that runs down several rows is never split between columns, but starts the next column when it
/// does not fit the rest of the current one.
fn column_major(group: &[Span], columns: usize) -> (Vec<(usize, usize)>, usize) {
    let slots: usize = group.iter().map(|s| s.heights.len()).sum();
    let tallest = group.iter().map(|s| s.heights.len()).max().unwrap_or(1);
    let mut rows = slots.div_ceil(columns).max(tallest);
    loop {
        let mut cells = Vec::with_capacity(group.len());
        let (mut r, mut c) = (0, 0);
        for span in group {
            let n = span.heights.len();
            if r > 0 && r + n > rows {
                c += 1;
                r = 0;
            }
            cells.push((r, c));
            r += n;
        }
        if c < columns {
            return (cells, rows);
        }
        rows += 1;
    }
}

/// Places the members of a group, which lie between rows of their own, in `columns` columns filled row by row, and
/// returns the row and column of each, relative to the group, with the number of rows of the group. Each member
/// takes the next free place in reading order, and a member that runs down several rows the next place below which
/// those rows of its column are free.
fn row_major(group: &[Span], columns: usize) -> (Vec<(usize, usize)>, usize) {
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    let mut cells = Vec::with_capacity(group.len());
    let mut rows = 0;
    let mut p = 0;
    for span in group {
        let n = span.heights.len();
        loop {
            let (r, c) = (p / columns, p % columns);
            p += 1;
            if (0..n).all(|i| !taken.contains(&(r + i, c))) {
                taken.extend((0..n).map(|i| (r + i, c)));
                cells.push((r, c));
                rows = rows.max(r + n);
                break;
            }
        }
    }
    (cells, rows)
}

/// Places members of the given spans in at most `columns` columns, in the given order.
///
/// A member marked `full` takes a row of its own, at its position in the list, spanning every column. The members
/// between such rows form a group that fills the columns on its own (see [`column_major`] and [`row_major`]). The
/// rows are as tall as the tallest need of any member in them and the columns as wide as their widest, across every
/// group, so that entries line up both across and down; a member on a row of its own sets no column's width.
fn grid(spans: &[Span], columns: usize, order: LegendEntryOrder, m: &Metrics) -> Grid {
    let columns = columns.max(1);
    let mut places = Vec::with_capacity(spans.len());
    let mut row = 0;
    let mut k = 0;
    while k < spans.len() {
        if spans[k].full {
            places.push(Place { row, column: None });
            row += spans[k].heights.len();
            k += 1;
            continue;
        }
        let size = spans[k..].iter().take_while(|s| !s.full).count();
        let group = &spans[k..k + size];
        let (cells, rows) = match order {
            LegendEntryOrder::ColumnMajor => column_major(group, columns),
            LegendEntryOrder::RowMajor => row_major(group, columns),
        };
        places.extend(cells.into_iter().map(|(r, c)| Place {
            row: row + r,
            column: Some(c),
        }));
        row += rows;
        k += size;
    }
    let used = places
        .iter()
        .filter_map(|p| p.column.map(|c| c + 1))
        .max()
        .unwrap_or(0);
    // A column is as wide as its widest member needs, which for a bar item alone may be less than an empty label.
    let mut label_widths = vec![f64::NEG_INFINITY; used];
    let mut row_heights = vec![0.0f64; row];
    let mut full_width = 0.0f64;
    for (span, place) in spans.iter().zip(&places) {
        match place.column {
            Some(col) => label_widths[col] = label_widths[col].max(span.width),
            None => full_width = full_width.max(span.width),
        }
        for (i, h) in span.heights.iter().enumerate() {
            row_heights[place.row + i] = row_heights[place.row + i].max(*h);
        }
    }
    let columns_width = label_widths.iter().map(|w| m.column_width(*w)).sum::<f64>()
        + used.saturating_sub(1) as f64 * m.column_gap;
    let width = 2.0 * m.pad + columns_width.max(full_width);
    let height = 2.0 * m.pad + row_heights.iter().sum::<f64>();
    Grid {
        places,
        spans: spans.iter().map(|s| s.heights.len()).collect(),
        label_widths,
        row_heights,
        width,
        height,
    }
}

/// Returns the look of the sample of an item that stands for no artist.
///
/// A line sample is drawn as the sample of a series is: its automatic colour is the ink of the labels, because
/// the item belongs to no axes whose colour order could supply one, and its marker's automatic colours take the line
/// colour, or the ink when the line has no colour. A swatch is a square of its colour with a thin black outline, as
/// the category of a categorical scale is drawn.
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
                sweep: None,
                marker: marker_sample(
                    marker,
                    marker.size_pt,
                    Some(colour.unwrap_or(INK)),
                    style::width_or(line.width_pt, 0.75).clamp(0.5, 1.5),
                    None,
                ),
            }
        }
        LegendSample::Swatch { color } => SampleLook::Swatch {
            fill: Some(style::ir_colour(color)),
            outline: Some(SWATCH_OUTLINE),
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
    let at = |align| panel::place(area, width, height, align, inset);
    match location {
        LegendLocation::NorthEast => at(PanelLocation::NorthEast),
        LegendLocation::NorthWest => at(PanelLocation::NorthWest),
        LegendLocation::SouthEast => at(PanelLocation::SouthEast),
        LegendLocation::SouthWest => at(PanelLocation::SouthWest),
        LegendLocation::North => at(PanelLocation::North),
        LegendLocation::South => at(PanelLocation::South),
        LegendLocation::East => at(PanelLocation::East),
        LegendLocation::West => at(PanelLocation::West),
        LegendLocation::Center => at(PanelLocation::Center),
        LegendLocation::Best => {
            let candidates = [
                at(PanelLocation::NorthEast),
                at(PanelLocation::NorthWest),
                at(PanelLocation::SouthEast),
                at(PanelLocation::SouthWest),
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

/// Returns the sample of an artist, given its resolved primary colour and the colour scale through which it maps
/// numbers to colours, if it names a usable one.
fn sample(
    artist: &Artist,
    data: &ArtistData,
    colour: Option<Rgba>,
    scale: Option<&ResolvedScale>,
) -> SampleLook {
    match artist {
        Artist::Series(series) => series_sample(series, data, colour, scale),
        Artist::Quiver(q) => {
            SampleLook::Arrow(sample_stroke(&q.line, colour).map(|stroke| Stroke {
                dash: Vec::new(),
                ..stroke
            }))
        }
        // The opacities of a contour carry into its sample, as those of a surface do. Bands and colour-mapped
        // isolines sweep the contour's scale; without a usable scale they are drawn in the middle of viridis, as the
        // contour itself is.
        Artist::Contour(c) => {
            let fade = |colour: Rgba, opacity: f64| {
                Some(colour.with_alpha_factor(style::unit_opacity(opacity) as f32))
            };
            let explicit = match c.line.color {
                ColorSpec::Rgba { color } => fade(style::ir_colour(color), c.line_opacity),
                _ => None,
            };
            if c.fill {
                match scale {
                    Some(scale) => SampleLook::Sweep {
                        sweep: Sweep::of(scale, c.fill_opacity),
                        edge: explicit,
                    },
                    None => SampleLook::Patch {
                        fill: fade(style::viridis_middle(), c.fill_opacity),
                        edge: explicit,
                    },
                }
            } else {
                match (c.line.color, scale) {
                    (ColorSpec::None, _) => SampleLook::Line {
                        stroke: None,
                        sweep: None,
                        marker: None,
                    },
                    (ColorSpec::Rgba { .. }, _) => SampleLook::Line {
                        stroke: sample_stroke(&c.line, explicit),
                        sweep: None,
                        marker: None,
                    },
                    (ColorSpec::Auto | ColorSpec::Colormapped, Some(scale)) => SampleLook::Line {
                        stroke: sample_stroke(&c.line, Some(NO_COLOUR)),
                        sweep: Some(Sweep::of(scale, c.line_opacity)),
                        marker: None,
                    },
                    (ColorSpec::Auto | ColorSpec::Colormapped, None) => SampleLook::Line {
                        stroke: sample_stroke(
                            &c.line,
                            fade(style::viridis_middle(), c.line_opacity),
                        ),
                        sweep: None,
                        marker: None,
                    },
                }
            }
        }
        // The opacities, but not the alpha arrays, which vary over the surface, carry into the sample, so that a
        // translucent surface is keyed by a translucent sample. Colour-mapped faces sweep the scale, outlined in a
        // fixed edge colour; faces of one colour are a swatch outlined in the edge colour, so that a surface with
        // thin black edges is keyed as a category of the same colour is; and colour-mapped edges around no faces
        // sweep the scale as a line.
        Artist::Surface(s) => {
            let fade = |colour: Option<Rgba>, opacity: f64| {
                colour.map(|c| c.with_alpha_factor(style::unit_opacity(opacity) as f32))
            };
            let face = style::resolve(s.face, Paint::Colormapped);
            let edge = style::resolve(s.edge, Paint::Colormapped);
            let fixed_edge = match edge {
                Paint::Fixed(c) => fade(c, s.edge_opacity),
                Paint::Colormapped => None,
            };
            match (face, edge, scale) {
                (Paint::Colormapped, _, Some(scale)) => SampleLook::Sweep {
                    sweep: Sweep::of(scale, s.face_opacity),
                    edge: fixed_edge,
                },
                (Paint::Colormapped, _, None) => SampleLook::Swatch {
                    fill: fade(Some(style::viridis_middle()), s.face_opacity),
                    outline: fixed_edge,
                },
                (Paint::Fixed(None), Paint::Colormapped, Some(scale)) => SampleLook::Line {
                    stroke: sample_stroke(
                        &LineStyle {
                            width_pt: s.edge_width_pt,
                            ..LineStyle::default()
                        },
                        Some(NO_COLOUR),
                    ),
                    sweep: Some(Sweep::of(scale, s.edge_opacity)),
                    marker: None,
                },
                (Paint::Fixed(fill), _, _) => SampleLook::Swatch {
                    fill: fade(fill, s.face_opacity),
                    outline: fixed_edge,
                },
            }
        }
        // The mean colour of the raster, so that the patch resembles it; an image with no pixel to average, every
        // pixel being transparent, has no patch.
        // The opacity of an image carries into its patch, but not its alpha, which varies over the image.
        Artist::Image(i) => SampleLook::Patch {
            fill: match data {
                ArtistData::Image(image) => image::mean_colour(image)
                    .map(|c| c.with_alpha_factor(style::unit_opacity(i.opacity) as f32)),
                _ => None,
            },
            edge: None,
        },
        // A sweep of the scale, which takes the image's opacity but not its alpha, which varies over the image.
        Artist::ScalarImage(ScalarImage { opacity, .. }) => match scale {
            Some(scale) => SampleLook::Sweep {
                sweep: Sweep::of(scale, *opacity),
                edge: None,
            },
            None => SampleLook::Patch {
                fill: None,
                edge: None,
            },
        },
        // A swatch filled with the face and outlined by the edge, as a small bar; faces coloured by data sweep the
        // scale they are coloured through, as the faces of a surface do. A colour that colour data varies has no one
        // colour, so a colormapped outline with colour data outlines nothing.
        Artist::Bar(b) => {
            let fade = |colour: Option<Rgba>, opacity: f64| {
                colour.map(|c| c.with_alpha_factor(style::unit_opacity(opacity) as f32))
            };
            let mapped = b.color_data.is_some();
            let single = |spec: ColorSpec| match spec {
                ColorSpec::Colormapped if mapped => None,
                other => match style::resolve(other, Paint::Fixed(colour)) {
                    Paint::Fixed(c) => c,
                    Paint::Colormapped => colour,
                },
            };
            let edge = fade(single(b.edge.color), b.edge_opacity)
                .filter(|_| b.edge.dash != DashStyle::None);
            match (b.face, scale) {
                (ColorSpec::Colormapped, Some(scale)) if mapped => SampleLook::Sweep {
                    sweep: Sweep::of(scale, b.face_opacity),
                    edge,
                },
                _ => SampleLook::Swatch {
                    fill: fade(single(b.face), b.face_opacity),
                    outline: edge,
                },
            }
        }
        // Open stairs are a line; filled stairs are an area outlined by their line, as a filled histogram is.
        Artist::Stairs(s) => {
            let fade = |colour: Option<Rgba>, opacity: f64| {
                colour.map(|c| c.with_alpha_factor(style::unit_opacity(opacity) as f32))
            };
            let resolved = |spec: ColorSpec| match style::resolve(spec, Paint::Fixed(colour)) {
                Paint::Fixed(c) => c,
                Paint::Colormapped => colour,
            };
            let line = fade(resolved(s.line.color), s.line_opacity);
            let fill = fade(resolved(s.fill), s.fill_opacity).filter(|_| s.base.is_some());
            match fill {
                Some(fill) => SampleLook::Patch {
                    fill: Some(fill),
                    edge: line.filter(|_| s.line.dash != DashStyle::None),
                },
                None => SampleLook::Line {
                    stroke: sample_stroke(&s.line, line),
                    sweep: None,
                    marker: None,
                },
            }
        }
        // Cells are not drawn yet, so their sample is an empty patch.
        Artist::Cells(_) => SampleLook::Patch {
            fill: None,
            edge: None,
        },
    }
}

/// Returns the sample of a series: a line sample, with the marker at its centre, when the series draws a line, and
/// a single marker otherwise; the marker is of the marker size, not of any size data, and its edge is as wide as
/// the series strokes its marker edges.
///
/// `colour` is the resolved primary colour of the series and `scale` the scale of its colour data, if it has any.
/// A colour-mapped line with colour data draws a sweep of the scale. A marker colour that follows the colour data
/// point by point (a colour-mapped marker colour, or an automatic one following a colour-mapped line) takes the
/// sweep's colour where the marker sits: at the centre of a line sample, and across the sample in a row of markers
/// when the series draws only markers. Any other automatic or colour-mapped marker colour takes the series' own
/// colour, as the markers themselves do.
fn series_sample(
    series: &Series,
    data: &ArtistData,
    colour: Option<Rgba>,
    scale: Option<&ResolvedScale>,
) -> SampleLook {
    let per_point = matches!(
        data,
        ArtistData::Series {
            colours: Some(SeriesColours::PerPoint(_)),
            ..
        }
    );
    let sweep = scale.map(|scale| Sweep::of(scale, 1.0));
    let mapped_line = series.line.color == ColorSpec::Colormapped;
    let line_sweep = sweep.clone().filter(|_| mapped_line);
    let follows = per_point && sweep.is_some();
    let fixed = |colour: Option<Rgba>| match colour {
        Some(c) => ColorSpec::Rgba {
            color: Color::rgba(c.r, c.g, c.b, c.a),
        },
        None => ColorSpec::None,
    };
    let part = |spec: ColorSpec| match spec {
        ColorSpec::Auto if mapped_line && follows => ColorSpec::Colormapped,
        ColorSpec::Colormapped if follows => ColorSpec::Colormapped,
        ColorSpec::Auto | ColorSpec::Colormapped => fixed(colour),
        other => other,
    };
    let marker = MarkerStyle {
        face: part(series.marker.face),
        edge: part(series.marker.edge),
        ..series.marker
    };
    let marker_sweep = sweep
        .filter(|_| marker.face == ColorSpec::Colormapped || marker.edge == ColorSpec::Colormapped);
    if series.draws_line() {
        let stroke_colour = if line_sweep.is_some() {
            Some(NO_COLOUR)
        } else {
            colour
        };
        SampleLook::Line {
            stroke: sample_stroke(&series.line, stroke_colour),
            sweep: line_sweep,
            marker: marker_sample(
                marker,
                series.marker.size_pt,
                None,
                style::width_or(series.line.width_pt, 0.75).clamp(0.5, 1.5),
                marker_sweep,
            ),
        }
    } else {
        SampleLook::Marker(marker_sample(
            marker,
            series.marker.size_pt,
            None,
            0.5,
            marker_sweep,
        ))
    }
}

/// Returns the marker of a sample, or `None` when the marker has no shape or no usable size.
fn marker_sample(
    style: MarkerStyle,
    size: f64,
    auto: Option<Rgba>,
    edge_width: f64,
    sweep: Option<Sweep>,
) -> Option<MarkerSample> {
    style::marker_outline(style.shape)?;
    (size.is_finite() && size > 0.0).then_some(MarkerSample {
        style: MarkerStyle {
            size_pt: size,
            ..style
        },
        auto,
        edge_width,
        sweep,
    })
}

/// Draws a sample inside `cell`, the sample column of its row.
fn draw_sample(
    source: NodeId,
    sample: &SampleLook,
    cell: Rect,
    alpha: f32,
    m: &Metrics,
    out: &mut Vec<Item>,
) {
    let fade = |c: Rgba| c.with_alpha_factor(alpha);
    let fade_stroke = |s: &Stroke| Stroke {
        color: fade(s.color),
        ..s.clone()
    };
    let cy = cell.y + cell.height / 2.0;
    // A marker at the fraction `f` across the sample, `size` wide at most.
    let marker = |m: &MarkerSample, f: f64, size: f64, out: &mut Vec<Item>| {
        let Some(outline) = style::marker_outline(m.style.shape) else {
            return;
        };
        let auto = match &m.sweep {
            Some(sweep) => sweep.at(f),
            None => m.auto,
        };
        if let Some(instance) = style::marker_instance(
            &m.style,
            &outline,
            style::MarkerPlace {
                position: Point::new(cell.x + f * cell.width, cy),
                size: m.style.size_pt.min(size),
                depth: 0.0,
                source_index: 0,
            },
            auto,
            alpha,
        ) {
            out.push(style::marker_item(source, &outline, m.edge_width, instance));
        }
    };
    let line = |from: f64, to: f64, stroke: Stroke, out: &mut Vec<Item>| {
        let mut b = PathBuilder::new();
        b.polyline(
            &[
                Point::new(cell.x + from * cell.width, cy),
                Point::new(cell.x + to * cell.width, cy),
            ],
            false,
        );
        out.extend(paths::item(source, b.finish(), None, Some(stroke)));
    };
    // The band across the middle of the sample that a patch or a sweep fills.
    let band = {
        let h = 0.6 * cell.height;
        Rect::new(cell.x, cy - h / 2.0, cell.width, h)
    };
    let outline = |r: Rect, edge: &Option<Rgba>, out: &mut Vec<Item>| {
        if let Some(edge) = edge {
            let mut b = PathBuilder::new();
            b.rect(r);
            out.extend(paths::item(
                source,
                b.finish(),
                None,
                Some(paths::solid(fade(*edge), 0.5)),
            ));
        }
    };
    match sample {
        SampleLook::Line {
            stroke,
            sweep,
            marker: m,
        } => {
            match (stroke, sweep) {
                (Some(stroke), Some(sweep)) => {
                    let n = sweep.0.len().clamp(1, LINE_SWEEP_SEGMENTS);
                    for k in 0..n {
                        let (from, to) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
                        if let Some(colour) = sweep.at((from + to) / 2.0) {
                            // The dashes run on across the segments, as along one line.
                            let s = Stroke {
                                color: colour,
                                dash_offset: stroke.dash_offset + from * cell.width,
                                ..stroke.clone()
                            };
                            line(from, to, fade_stroke(&s), out);
                        }
                    }
                }
                (Some(stroke), None) => line(0.0, 1.0, fade_stroke(stroke), out),
                (None, _) => {}
            }
            if let Some(m) = m {
                marker(m, 0.5, 0.8 * cell.height, out);
            }
        }
        SampleLook::Marker(Some(m)) if m.sweep.is_some() => {
            // A row of markers, each in the sweep's colour at its place, no wider than its share of the sample.
            let share = cell.width / SWEEP_MARKERS as f64;
            for k in 0..SWEEP_MARKERS {
                let f = (k as f64 + 0.5) / SWEEP_MARKERS as f64;
                marker(m, f, (0.8 * cell.height).min(0.8 * share), out);
            }
        }
        SampleLook::Marker(m) => {
            if let Some(m) = m {
                marker(m, 0.5, 0.8 * cell.height, out);
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
            let mut b = PathBuilder::new();
            b.rect(band);
            let fill = fill.map(|c| paths::fill(fade(c)));
            let stroke = edge.map(|c| paths::solid(fade(c), 0.5));
            out.extend(paths::item(source, b.finish(), fill, stroke));
        }
        SampleLook::Sweep { sweep, edge } => {
            out.extend(colorbar::colour_row(source, band, &sweep.faded(alpha)));
            outline(band, edge, out);
        }
        SampleLook::Swatch { fill, outline } => {
            let square = swatch_square(cell, m);
            out.extend(colorbar::swatch(
                source,
                square,
                fill.map(fade),
                outline.map(fade),
            ));
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
