//! Colourbars: a colour scale drawn as a bar, in a panel of its own or inside the plot area of an axes.
//!
//! A colourbar is a strip of pieces read from the top of a standing bar, or from the left of a lying one: the bar
//! itself (or the squares of a categorical scale) and the squares of the out-of-range keys its scale shows. Every
//! square is as wide as the bar is thick, and one margin separates each piece from the next. The bar holds no
//! colours, limits or ticks of its own: its colours are those of the resolved scale, and its ticks are those of an
//! axis built from the scale's ticks and tick labels, measured by [`decor::axis_ticks`] as an axis's are. Low values
//! are always at the bottom or the left, and the ends of a bar are flat and never carry an out-of-range colour.
//!
//! A panel colourbar stretches along the area of its panel, which the layout insets by the aligned margins of the
//! rows or columns it spans, and is placed across that area by the panel's alignment. An inside colourbar lies along
//! one edge of the plot area, inset from it by one margin, with its labels toward the interior, and takes no margin
//! of the layout.

use ironlab_ir::{
    Axes, Axis, ColorScaleDomain, ColorScaleKey, ColorScaleKind, ColorScaleNorm, ColorScaleSlot,
    ColorbarOrientation, InsideColorbarSide, NodeId, Panel, PanelColorbar, Scale, Text, Ticks,
};

use crate::display::{ImageItem, Item, ItemKind, Point, Rect, Rgba};
use crate::hit::{ColorbarHit, ColorbarKeyHit, ColorbarOwner, HitMap};
use crate::maths::ticks;

use super::Ctx;
use super::axes2d::frame_stroke;
use super::decor::{self, AxisTicks, MIN_X_TICKS, TICK_SCALE};
use super::image;
use super::layout::{Along, page_padding};
use super::legend;
use super::limits::{self, Range};
use super::panel;
use super::paths::{self, PathBuilder};
use super::style::{ColourScales, INK, ResolvedScale};
use super::text::{TextBlock, measure, measure_str};

/// The number of samples along a continuous bar, one per entry of a bundled colormap, so that no band shows.
const CONTINUOUS_SAMPLES: usize = 256;

/// The outline of a category or key square, black so that a square of any colour, or none, reads as a swatch.
const OUTLINE: Rgba = Rgba::BLACK;

/// The width of the outline of a square, in points.
const OUTLINE_WIDTH: f64 = 0.5;

/// Distances of a colourbar in points, each a multiple of the base font size, so that a bar scales with its text as
/// a legend does.
struct Metrics {
    /// The thickness of the bar, and the side of every square.
    thickness: f64,
    /// The length of a tick mark, outside the bar on its label side.
    tick: f64,
    /// Between the edge of the bar and its labels.
    label_gap: f64,
    /// Between neighbouring pieces of a standing bar: the bar and a key, or two squares.
    gap: f64,
    /// Between the title and what it heads.
    title_gap: f64,
    /// The shortest bar of a colourbar whose length nothing fixes.
    min_length: f64,
}

impl Metrics {
    fn new(fs: f64) -> Self {
        Self {
            thickness: 1.2 * fs,
            tick: 0.3 * fs,
            label_gap: 0.55 * fs,
            gap: 0.5 * fs,
            title_gap: 0.5 * fs,
            min_length: 6.0 * fs,
        }
    }
}

/// One piece of a colourbar along its length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Piece {
    /// The bar of a continuous, discrete or indexed scale.
    Bar,
    /// The square of a category of a categorical scale.
    Category(usize),
    /// The square of an out-of-range key.
    Key(usize),
}

/// An out-of-range key of a colourbar.
struct Key<'a> {
    key: ColorScaleKey,
    /// The colour painted for values of the category, or `None` for an empty square.
    fill: Option<Rgba>,
    /// The author's label, or `None` for the default label.
    label: Option<&'a Text>,
}

/// A colourbar with its scale resolved and the text that does not depend on its length measured.
pub(super) struct Colorbar<'a> {
    owner: ColorbarOwner,
    /// The node that every item of the colourbar names as its source, and that warnings name.
    source: NodeId,
    scale: &'a ResolvedScale<'a>,
    vertical: bool,
    title: Option<TextBlock>,
    /// The colour and the measured label of each category of a categorical scale.
    categories: Vec<(Option<Rgba>, TextBlock)>,
    keys: Vec<Key<'a>>,
    /// The pieces in reading order: from the top of a standing bar, from the left of a lying one.
    pieces: Vec<Piece>,
}

/// What a colourbar measures for a given length: its ticks, and the labels of its keys, whose default labels name the
/// limits as the ticks write them.
struct Measured {
    ticks: Option<(AxisTicks, Range)>,
    key_labels: Vec<TextBlock>,
}

/// Where the pieces of a colourbar are laid: from `start` along its length, with the bar between `cross` and
/// `cross + thickness` across it, and its labels on the side of increasing coordinates when `outward` is positive.
/// `bounds`, when given, is the interval along the colourbar that its labels are kept within: the plot area of an
/// inside colourbar, so that the labels at the ends of a bar as long as the edge stay inside it.
#[derive(Clone, Copy)]
struct Strip {
    start: f64,
    cross: f64,
    outward: f64,
    bounds: Option<(f64, f64)>,
}

impl<'a> Colorbar<'a> {
    /// Prepares the colourbar of `scale` held by `owner`, standing when `vertical`, measuring its title and the labels
    /// of its categories.
    fn new(
        ctx: &mut Ctx,
        owner: ColorbarOwner,
        scale: &'a ResolvedScale<'a>,
        vertical: bool,
    ) -> Self {
        let source = match owner {
            ColorbarOwner::Panel(id) | ColorbarOwner::Axes(id) => id,
        };
        let fs = ctx.font_size;
        let title = scale
            .scale
            .title
            .as_ref()
            .map(|t| measure(ctx, t, fs, source));
        let categories: Vec<(Option<Rgba>, TextBlock)> = match &scale.scale.kind {
            ColorScaleKind::Categorical(c) => c
                .categories
                .iter()
                .enumerate()
                .map(|(i, category)| {
                    (
                        scale.slot_colour(ColorScaleSlot::Entry(i)),
                        measure(ctx, &category.label, TICK_SCALE * fs, source),
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        let shown = |key: ColorScaleKey| {
            let category = scale.scale.key(key).filter(|c| c.show_key)?;
            Some(Key {
                key,
                fill: scale.key_colour(key),
                label: category.label.as_ref(),
            })
        };
        // The order of the keys along the bar: the keys of values beyond the high end lie beyond the high end, and the
        // keys of values beyond the low end, and of no value, beyond the low end, the non-finite key outermost.
        let (before, after): (Vec<ColorScaleKey>, Vec<ColorScaleKey>) =
            match (&scale.scale.kind, vertical) {
                (ColorScaleKind::Categorical(_), _) => (
                    vec![],
                    vec![ColorScaleKey::Outside, ColorScaleKey::NonFinite],
                ),
                (ColorScaleKind::Indexed(_), true) => (
                    vec![],
                    vec![ColorScaleKey::Outside, ColorScaleKey::NonFinite],
                ),
                (ColorScaleKind::Indexed(_), false) => (
                    vec![ColorScaleKey::NonFinite, ColorScaleKey::Outside],
                    vec![],
                ),
                (_, true) => (
                    vec![ColorScaleKey::Above],
                    vec![ColorScaleKey::Below, ColorScaleKey::NonFinite],
                ),
                (_, false) => (
                    vec![ColorScaleKey::NonFinite, ColorScaleKey::Below],
                    vec![ColorScaleKey::Above],
                ),
            };
        let mut keys = Vec::new();
        let mut pieces = Vec::new();
        let mut push_keys = |order: &[ColorScaleKey], pieces: &mut Vec<Piece>| {
            for key in order.iter().filter_map(|&k| shown(k)) {
                pieces.push(Piece::Key(keys.len()));
                keys.push(key);
            }
        };
        push_keys(&before, &mut pieces);
        if categories.is_empty() {
            pieces.push(Piece::Bar);
        } else {
            pieces.extend((0..categories.len()).map(Piece::Category));
        }
        push_keys(&after, &mut pieces);
        Self {
            owner,
            source,
            scale,
            vertical,
            title,
            categories,
            keys,
            pieces,
        }
    }

    /// Returns whether the colourbar has a bar, which stretches to the length it is given, rather than squares alone.
    fn has_bar(&self) -> bool {
        self.pieces.contains(&Piece::Bar)
    }

    /// Returns the number of squares along the colourbar: its keys and the categories of a categorical scale.
    fn squares(&self) -> usize {
        self.pieces.iter().filter(|p| **p != Piece::Bar).count()
    }

    /// Measures the ticks and key labels of the colourbar when it is `length` points long, keys included, with `gap`
    /// between its pieces.
    fn measure(&self, ctx: &mut Ctx, m: &Metrics, length: f64, gap: f64) -> Measured {
        let bar_length = length - self.squares() as f64 * (m.thickness + gap);
        let ticks = self.ticks(ctx, bar_length);
        let size = TICK_SCALE * ctx.font_size;
        let key_labels = self
            .keys
            .iter()
            .map(|key| match key.label {
                Some(text) => measure(ctx, text, size, self.source),
                None => {
                    let text = self.default_key_label(key.key, ticks.as_ref());
                    measure_str(ctx, &text, text.contains('$'), size, self.source)
                }
            })
            .collect();
        Measured { ticks, key_labels }
    }

    /// Computes the ticks of a bar `length` points long, or `None` for a categorical scale, whose squares are labelled
    /// one by one.
    fn ticks(&self, ctx: &mut Ctx, length: f64) -> Option<(AxisTicks, Range)> {
        let (min, max, log, ticks, tick_labels) = match (&self.scale.domain, &self.scale.scale.kind)
        {
            (ColorScaleDomain::Continuous { min, max, norm }, ColorScaleKind::Continuous(c)) => (
                *min,
                *max,
                *norm == ColorScaleNorm::Log,
                c.ticks.clone(),
                &c.tick_labels,
            ),
            (ColorScaleDomain::Discrete { edges, norm }, ColorScaleKind::Discrete(d)) => {
                // Automatic ticks of a discrete scale mark every band edge.
                let ticks = match &d.ticks {
                    Ticks::Auto => Ticks::Values(edges.clone()),
                    ticks => ticks.clone(),
                };
                let (first, last) = (*edges.first()?, *edges.last()?);
                (
                    first,
                    last,
                    *norm == ColorScaleNorm::Log,
                    ticks,
                    &d.tick_labels,
                )
            }
            (ColorScaleDomain::Entries { count }, ColorScaleKind::Indexed(i)) => {
                // Entry k lies at the centre of the k-th of `count` blocks, and automatic ticks name whole entries.
                let ticks = match &i.ticks {
                    Ticks::Auto => Ticks::Values(whole_ticks(
                        *count,
                        limits::bar_tick_target(ctx, length, !self.vertical),
                    )),
                    ticks => ticks.clone(),
                };
                (-0.5, *count as f64 - 0.5, false, ticks, &i.tick_labels)
            }
            _ => return None,
        };
        let (min, max) = usable_span(min, max, log);
        let range = Range {
            min,
            max,
            log,
            reverse: false,
        };
        let axis = Axis {
            scale: if log { Scale::Log } else { Scale::Linear },
            ticks,
            tick_labels: tick_labels.clone(),
            ..Axis::default()
        };
        let target = limits::bar_tick_target(ctx, length, !self.vertical);
        let measured = decor::axis_ticks(
            ctx,
            self.source,
            &axis,
            "colour scale",
            range,
            target,
            MIN_X_TICKS,
        );
        Some((measured, range))
    }

    /// Returns the default label of a key: the range it stands for, with the limit written as the bar's ticks write
    /// it, or the kind of value it stands for.
    fn default_key_label(&self, key: ColorScaleKey, ticks: Option<&(AxisTicks, Range)>) -> String {
        let limit = |value: f64| -> String {
            if let Some((t, range)) = ticks {
                let tol = 1e-9 * (range.max - range.min).abs();
                let at = t.major.iter().position(|v| (v - value).abs() <= tol);
                if let (Some(k), None) = (at, &t.exponent)
                    && let Some(label) = t.labels.get(k)
                {
                    return label.source().to_owned();
                }
                if range.log {
                    return ticks::format_log(value);
                }
            }
            ticks::format_fixed(value, ticks::decimals_for(&[value]))
        };
        let (low, high) = match &self.scale.domain {
            ColorScaleDomain::Continuous { min, max, .. } => (*min, *max),
            ColorScaleDomain::Discrete { edges, .. } => (
                edges.first().copied().unwrap_or(f64::NAN),
                edges.last().copied().unwrap_or(f64::NAN),
            ),
            ColorScaleDomain::Entries { .. } => (f64::NAN, f64::NAN),
        };
        match key {
            ColorScaleKey::Below => format!("< {}", limit(low)),
            ColorScaleKey::Above => format!("> {}", limit(high)),
            ColorScaleKey::Outside => "Out of range".to_owned(),
            ColorScaleKey::NonFinite => "NaN / Inf".to_owned(),
        }
    }

    /// Returns every label beside the colourbar: its tick labels, the labels of its categories and of its keys.
    fn labels<'b>(&'b self, measured: &'b Measured) -> impl Iterator<Item = &'b TextBlock> {
        let ticks = measured.ticks.iter().flat_map(|(t, _)| &t.labels);
        ticks
            .chain(self.categories.iter().map(|(_, label)| label))
            .chain(&measured.key_labels)
    }

    /// Returns the extent across the colourbar of the band of labels beside it: the widest label of a standing
    /// colourbar, and the tallest of a lying one.
    fn band(&self, measured: &Measured) -> f64 {
        if self.vertical {
            self.labels(measured)
                .map(TextBlock::width)
                .fold(0.0, f64::max)
        } else {
            let height = self
                .labels(measured)
                .map(TextBlock::height)
                .fold(0.0, f64::max);
            let depth = self
                .labels(measured)
                .map(TextBlock::depth)
                .fold(0.0, f64::max);
            height + depth
        }
    }

    /// Returns the extent of the colourbar across its length, without its title: the bar, the gap and the labels.
    fn across(&self, m: &Metrics, measured: &Measured) -> f64 {
        m.thickness + m.label_gap + self.band(measured)
    }

    /// Returns the margin between neighbouring pieces. Along a lying colourbar, whose labels lie under the squares
    /// side by side, the margin is wide enough for no two labels to meet, the widest label of a square beside the
    /// widest of any piece, so that it stays one margin throughout. A square's label is centred half a thickness
    /// from its edge, but the label of a tick at the end of a bar is centred on the bar's end, so beside a bar the
    /// margin is half a thickness wider than between two squares. A colourbar `length` points long with a bar never
    /// gives its margins more than leaves the bar a third of that length, even if labels then meet, because a key with
    /// no bar to read shows nothing.
    fn gap(&self, m: &Metrics, measured: &Measured, length: f64) -> f64 {
        if self.vertical || self.squares() == 0 {
            return m.gap;
        }
        let squares = self
            .categories
            .iter()
            .map(|(_, label)| label)
            .chain(&measured.key_labels)
            .map(TextBlock::width)
            .fold(0.0, f64::max);
        let widest = self
            .labels(measured)
            .map(TextBlock::width)
            .fold(squares, f64::max);
        let reach = if self.has_bar() {
            m.thickness / 2.0
        } else {
            m.thickness
        };
        let wanted = m.gap.max((squares + widest) / 2.0 + m.gap - reach);
        let gaps = self.pieces.len().saturating_sub(1) as f64;
        if !self.has_bar() || gaps == 0.0 {
            return wanted;
        }
        let room = (2.0 / 3.0 * length - self.squares() as f64 * m.thickness) / gaps;
        wanted.min(room.max(m.gap))
    }

    /// Measures the colourbar when it is `length` points long and returns the measurement with the margin between its
    /// pieces. The margin depends on the labels and the ticks on the length the margins leave the bar, so the labels
    /// are measured once with the least margin, whose warnings are discarded, and again with the margin found.
    fn measure_fitted(&self, ctx: &mut Ctx, m: &Metrics, length: f64) -> (Measured, f64) {
        let first = self.measure(&mut ctx.scratch(), m, length, m.gap);
        let gap = self.gap(m, &first, length);
        (self.measure(ctx, m, length, gap), gap)
    }

    /// Returns how far the label of the first or the last piece of a lying colourbar reaches beyond its end, whichever
    /// reaches further, each label being centred under its square; a bar's own end labels reach nothing, because they
    /// are kept within the colourbar's bounds. A standing colourbar's labels lie beside it and reach nothing.
    fn end_overhang(&self, m: &Metrics, measured: &Measured) -> f64 {
        if self.vertical {
            return 0.0;
        }
        let reach = |piece: Option<&Piece>| {
            let width = match piece {
                Some(Piece::Key(i)) => measured.key_labels[*i].width(),
                Some(Piece::Category(i)) => self.categories[*i].1.width(),
                Some(Piece::Bar) | None => 0.0,
            };
            ((width - m.thickness) / 2.0).max(0.0)
        };
        reach(self.pieces.first()).max(reach(self.pieces.last()))
    }

    /// Returns the length of the colourbar when nothing fixes it: its squares, with the shortest bar when it has one.
    fn natural_length(&self, m: &Metrics, gap: f64) -> f64 {
        let squares = self.squares() as f64 * m.thickness;
        let gaps = self.pieces.len().saturating_sub(1) as f64 * gap;
        squares + gaps + if self.has_bar() { m.min_length } else { 0.0 }
    }

    /// Returns the extent of the title above what it heads, its gap included, or zero without a title.
    fn title_part(&self, m: &Metrics) -> f64 {
        self.title
            .as_ref()
            .map_or(0.0, |t| t.total_height() + m.title_gap)
    }

    /// Returns the size of the area of a panel holding the colourbar along a fitted track (see `panel::Content`).
    pub(super) fn size(&self, ctx: &Ctx, along: Along, cross: f64) -> f64 {
        let m = Metrics::new(ctx.font_size);
        let mut scratch = ctx.scratch();
        let lengthwise = matches!(
            (self.vertical, along),
            (true, Along::Rows) | (false, Along::Cols)
        );
        if lengthwise {
            let measured = self.measure(&mut scratch, &m, f64::INFINITY, m.gap);
            return self.natural_length(&m, self.gap(&m, &measured, f64::INFINITY));
        }
        let (measured, _) = self.measure_fitted(&mut scratch, &m, cross);
        let across = self.across(&m, &measured);
        if self.vertical {
            across.max(self.title.as_ref().map_or(0.0, TextBlock::width))
        } else {
            across + self.title_part(&m)
        }
    }

    /// Returns the margin the colourbar of a panel needs above the panel's area: a standing bar's title is a heading
    /// in the margin above the first row it spans, as an axes title is above its plot area.
    pub(super) fn top_margin(&self, ctx: &Ctx) -> f64 {
        if !self.vertical || self.title.is_none() {
            return 0.0;
        }
        page_padding(ctx) + self.title_part(&Metrics::new(ctx.font_size))
    }

    /// Draws the colourbar of `panel` in its `area` and records it in the hit map.
    /// Returns whether the colourbar stands, running along the rows of the tile layout rather than its columns.
    pub(super) fn vertical(&self) -> bool {
        self.vertical
    }

    pub(super) fn draw_panel(
        &self,
        ctx: &mut Ctx,
        panel: &Panel,
        area: Rect,
        out: &mut Vec<Item>,
        hits: &mut HitMap,
    ) {
        let m = Metrics::new(ctx.font_size);
        let length = if self.vertical {
            area.height
        } else {
            area.width
        };
        let (measured, gap) = self.measure_fitted(ctx, &m, length);
        let length = if self.has_bar() {
            length
        } else {
            self.natural_length(&m, gap)
        };
        let across = self.across(&m, &measured);
        let title_width = self.title.as_ref().map_or(0.0, TextBlock::width);
        if self.vertical {
            let width = across.max(title_width);
            let block = panel::place(area, width, length, panel.align, 0.0);
            let strip = Strip {
                start: block.y,
                cross: block.x,
                outward: 1.0,
                bounds: None,
            };
            let hit = self.draw_strip(ctx, &m, &measured, strip, length, gap, out);
            // The title is centred over the bar, kept within the block, which is at least as wide as the title.
            let left = (block.x + m.thickness / 2.0 - title_width / 2.0)
                .clamp(block.x, block.x + width - title_width);
            self.draw_title(ctx, Point::new(left, block.y - m.title_gap), false, out);
            hits.colorbars.push(hit);
        } else {
            let title = self.title_part(&m);
            let block = panel::place(area, length, title + across, panel.align, 0.0);
            let strip = Strip {
                start: block.x,
                cross: block.y + title,
                outward: 1.0,
                bounds: None,
            };
            let hit = self.draw_strip(ctx, &m, &measured, strip, length, gap, out);
            let left = block.x + (length - title_width) / 2.0;
            self.draw_title(ctx, Point::new(left, block.y), true, out);
            hits.colorbars.push(hit);
        }
    }

    /// Draws the title with its left edge at `at.x`, its top at `at.y` when `top`, and its bottom there otherwise.
    fn draw_title(&self, ctx: &mut Ctx, at: Point, top: bool, out: &mut Vec<Item>) {
        let Some(title) = &self.title else {
            return;
        };
        let baseline = if top {
            at.y + title.height()
        } else {
            at.y - title.depth()
        };
        title.draw_on_page(ctx, Point::new(at.x, baseline), INK, self.source, out);
    }

    /// Returns the rectangle from `u0` to `u1` along the colourbar and from `c0` to `c1` across it.
    fn rect(&self, strip: Strip, u0: f64, u1: f64, c0: f64, c1: f64) -> Rect {
        if self.vertical {
            Rect::new(c0, strip.start + u0, c1 - c0, u1 - u0)
        } else {
            Rect::new(strip.start + u0, c0, u1 - u0, c1 - c0)
        }
    }

    /// Draws the pieces of the colourbar along `strip`, `length` points long when it has a bar, with `gap` between
    /// pieces, and returns its hit geometry.
    #[expect(
        clippy::too_many_arguments,
        reason = "the geometry of one strip, measured once by the caller"
    )]
    fn draw_strip(
        &self,
        ctx: &mut Ctx,
        m: &Metrics,
        measured: &Measured,
        strip: Strip,
        length: f64,
        gap: f64,
        out: &mut Vec<Item>,
    ) -> ColorbarHit {
        let fixed =
            self.squares() as f64 * m.thickness + self.pieces.len().saturating_sub(1) as f64 * gap;
        let bar_length = (length - fixed).max(0.0);
        let (c0, c1) = (strip.cross, strip.cross + m.thickness);
        // The edge at which the labels start, and the baseline that the labels of a lying colourbar share.
        let edge = if strip.outward > 0.0 {
            c1 + m.label_gap
        } else {
            c0 - m.label_gap
        };
        let labels: Vec<&TextBlock> = self.labels(measured).collect();
        let row_height = labels.iter().map(|l| l.height()).fold(0.0, f64::max);
        let row_depth = labels.iter().map(|l| l.depth()).fold(0.0, f64::max);
        let label_origin = |label: &TextBlock, at: f64| -> Point {
            let half = if self.vertical {
                label.total_height() / 2.0
            } else {
                label.width() / 2.0
            };
            let at = match strip.bounds {
                Some((lo, hi)) if hi - lo >= 2.0 * half => at.clamp(lo + half, hi - half),
                _ => at,
            };
            if self.vertical {
                let left = if strip.outward > 0.0 {
                    edge
                } else {
                    edge - label.width()
                };
                label.origin_for_centre(Point::new(left + label.width() / 2.0, at))
            } else {
                let baseline = if strip.outward > 0.0 {
                    edge + row_height
                } else {
                    edge - row_depth
                };
                Point::new(at - label.width() / 2.0, baseline)
            }
        };
        let centre_along = |r: Rect| {
            if self.vertical {
                r.y + r.height / 2.0
            } else {
                r.x + r.width / 2.0
            }
        };

        let mut hit = ColorbarHit {
            owner: self.owner,
            scale: self.scale.scale.id,
            orientation: if self.vertical {
                ColorbarOrientation::Vertical
            } else {
                ColorbarOrientation::Horizontal
            },
            bar: Rect::new(0.0, 0.0, 0.0, 0.0),
            entries: Vec::new(),
            keys: Vec::new(),
            backdrop: None,
        };
        let mut u = 0.0;
        for piece in &self.pieces {
            let size = if *piece == Piece::Bar {
                bar_length
            } else {
                m.thickness
            };
            let r = self.rect(strip, u, u + size, c0, c1);
            u += size + gap;
            match *piece {
                Piece::Bar => {
                    hit.bar = r;
                    self.draw_bar(m, measured, r, strip.outward, out);
                    if let Some((t, range)) = &measured.ticks {
                        for (v, label) in t.major.iter().zip(&t.labels) {
                            let at = self.along(r, fraction(*range, *v));
                            let origin = label_origin(label, at);
                            label.draw_on_page(ctx, origin, INK, self.source, out);
                        }
                        if let Some(e) = &t.exponent {
                            let origin = if self.vertical {
                                let left = if strip.outward > 0.0 {
                                    edge
                                } else {
                                    edge - e.width()
                                };
                                Point::new(left, r.y - m.title_gap / 2.0 - e.depth())
                            } else {
                                let o = label_origin(e, r.right());
                                Point::new(r.right() + m.label_gap, o.y)
                            };
                            e.draw_on_page(ctx, origin, INK, self.source, out);
                        }
                    }
                }
                Piece::Category(i) => {
                    let (fill, label) = &self.categories[i];
                    out.extend(square(self.source, r, *fill));
                    label.draw_on_page(
                        ctx,
                        label_origin(label, centre_along(r)),
                        INK,
                        self.source,
                        out,
                    );
                    hit.entries.push(r);
                }
                Piece::Key(i) => {
                    let key = &self.keys[i];
                    out.extend(square(self.source, r, key.fill));
                    let label = &measured.key_labels[i];
                    label.draw_on_page(
                        ctx,
                        label_origin(label, centre_along(r)),
                        INK,
                        self.source,
                        out,
                    );
                    hit.keys.push(ColorbarKeyHit {
                        key: key.key,
                        rect: r,
                    });
                }
            }
        }
        if let (Some(first), Some(last)) = (hit.entries.first(), hit.entries.last()) {
            let (x, y) = (first.x.min(last.x), first.y.min(last.y));
            hit.bar = Rect::new(
                x,
                y,
                first.right().max(last.right()) - x,
                first.bottom().max(last.bottom()) - y,
            );
        }
        hit
    }

    /// Returns the coordinate along the colourbar of the fraction `f` of the bar `r` from its low end.
    fn along(&self, r: Rect, f: f64) -> f64 {
        if self.vertical {
            r.bottom() - f * r.height
        } else {
            r.x + f * r.width
        }
    }

    /// Draws the bar `r` of a continuous, discrete or indexed scale, its outline and its tick marks.
    fn draw_bar(
        &self,
        m: &Metrics,
        measured: &Measured,
        r: Rect,
        outward: f64,
        out: &mut Vec<Item>,
    ) {
        match &self.scale.domain {
            ColorScaleDomain::Continuous { .. } => {
                let colours = (0..CONTINUOUS_SAMPLES).map(|k| {
                    let t = (k as f64 + 0.5) / CONTINUOUS_SAMPLES as f64;
                    self.scale.slot_colour(ColorScaleSlot::Position(t))
                });
                out.extend(self.image(r, colours.collect()));
            }
            ColorScaleDomain::Entries { count } => {
                let colours = (0..*count).map(|k| self.scale.slot_colour(ColorScaleSlot::Entry(k)));
                out.extend(self.image(r, colours.collect()));
            }
            ColorScaleDomain::Discrete { edges, norm } => {
                let log = *norm == ColorScaleNorm::Log;
                let (min, max) = usable_span(edges[0], edges[edges.len() - 1], log);
                let range = Range {
                    min,
                    max,
                    log,
                    reverse: false,
                };
                for (band, pair) in edges.windows(2).enumerate() {
                    let (a, b) = (
                        self.along(r, fraction(range, pair[0])),
                        self.along(r, fraction(range, pair[1])),
                    );
                    let band_rect = if self.vertical {
                        Rect::new(r.x, b, r.width, a - b)
                    } else {
                        Rect::new(a, r.y, b - a, r.height)
                    };
                    let mut path = PathBuilder::new();
                    path.rect(band_rect);
                    let fill = self
                        .scale
                        .slot_colour(ColorScaleSlot::Band(band))
                        .map(paths::fill);
                    out.extend(paths::item(self.source, path.finish(), fill, None));
                }
            }
        }
        let mut outline = PathBuilder::new();
        outline.rect(r);
        out.extend(paths::item(
            self.source,
            outline.finish(),
            None,
            Some(frame_stroke()),
        ));

        let Some((t, range)) = &measured.ticks else {
            return;
        };
        let mut marks = PathBuilder::new();
        let base = if outward > 0.0 {
            if self.vertical { r.right() } else { r.bottom() }
        } else if self.vertical {
            r.x
        } else {
            r.y
        };
        let mut mark = |v: f64, len: f64| {
            let at = self.along(r, fraction(*range, v));
            if !at.is_finite() {
                return;
            }
            let tip = base + outward * len;
            let (a, b) = if self.vertical {
                (Point::new(base, at), Point::new(tip, at))
            } else {
                (Point::new(at, base), Point::new(at, tip))
            };
            marks.polyline(&[a, b], false);
        };
        t.major.iter().for_each(|v| mark(*v, m.tick));
        t.minor.iter().for_each(|v| mark(*v, 0.6 * m.tick));
        out.extend(paths::item(
            self.source,
            marks.finish(),
            None,
            Some(frame_stroke()),
        ));
    }

    /// Returns the image item of a bar of the given colours, from the low end, one sample each along the bar.
    fn image(&self, r: Rect, colours: Vec<Option<Rgba>>) -> Option<Item> {
        let n = colours.len();
        let pixels: Vec<[u8; 4]> = if self.vertical {
            // Row 0 is the top of the image, where the high end of a standing bar lies.
            colours.into_iter().rev().map(image::pixel).collect()
        } else {
            colours.into_iter().map(image::pixel).collect()
        };
        let raster = image::pack(pixels);
        let n = u32::try_from(n).ok().filter(|n| *n > 0)?;
        let (width, height) = if self.vertical { (1, n) } else { (n, 1) };
        Some(Item {
            source: Some(self.source),
            kind: ItemKind::Image(ImageItem {
                rect: r,
                width,
                height,
                channels: raster.channels,
                samples: raster.samples,
                depth: None,
            }),
        })
    }
}

/// Returns a square filled with `fill`, or empty without one, outlined in black.
fn square(source: NodeId, r: Rect, fill: Option<Rgba>) -> Option<Item> {
    let mut b = PathBuilder::new();
    b.rect(r);
    paths::item(
        source,
        b.finish(),
        fill.map(paths::fill),
        Some(paths::solid(OUTLINE, OUTLINE_WIDTH)),
    )
}

/// Returns the fraction of `range` from its low end at which `value` lies, in the space of its scale.
fn fraction(range: Range, value: f64) -> f64 {
    let place = |v: f64| if range.log { v.log10() } else { v };
    (place(value) - place(range.min)) / (place(range.max) - place(range.min))
}

/// Returns a span of positive length to draw a bar over: the given limits, or, when they coincide, one unit about them
/// (a decade under a logarithmic norm), so that a scale of one value still draws a bar.
fn usable_span(min: f64, max: f64, log: bool) -> (f64, f64) {
    if max > min {
        (min, max)
    } else if log {
        (min / 10.0_f64.sqrt(), max * 10.0_f64.sqrt())
    } else {
        (min - 0.5, max + 0.5)
    }
}

/// Returns the automatic ticks of an indexed scale of `count` entries: whole numbers at the nice step that `target`
/// intervals give, and every entry when that step is finer than one entry.
fn whole_ticks(count: usize, target: usize) -> Vec<f64> {
    if count <= 1 {
        return vec![0.0];
    }
    let last = (count - 1) as f64;
    let t = ticks::linear_ticks(0.0, last, target);
    // A step that is not finite or is finer than one entry ticks every entry.
    if t.step.is_nan() || t.step < 1.0 {
        return (0..count).map(|k| k as f64).collect();
    }
    let whole: Vec<f64> = t
        .major
        .into_iter()
        .filter(|v| v.fract() == 0.0 && (0.0..=last).contains(v))
        .collect();
    if whole.is_empty() { vec![0.0] } else { whole }
}

/// Prepares the colourbar of a panel, or warns against the panel and returns `None` when it names no colour scale of
/// the figure.
pub(super) fn prepare_panel<'a>(
    ctx: &mut Ctx,
    panel: &Panel,
    colorbar: &PanelColorbar,
    scales: &'a ColourScales<'a>,
) -> Option<Colorbar<'a>> {
    let owner = ColorbarOwner::Panel(panel.id);
    let scale = usable(ctx, panel.id, colorbar.scale, scales)?;
    Some(Colorbar::new(
        ctx,
        owner,
        scale,
        colorbar.orientation == ColorbarOrientation::Vertical,
    ))
}

/// Returns the resolved scale a colourbar names, or warns against its holder and returns `None`.
fn usable<'a>(
    ctx: &mut Ctx,
    holder: NodeId,
    scale: NodeId,
    scales: &'a ColourScales<'a>,
) -> Option<&'a ResolvedScale<'a>> {
    let found = scales.get(scale);
    if found.is_none() {
        ctx.warn(
            Some(holder),
            format!("The colourbar shows {scale}, which is not a colour scale of the figure, so it is not drawn."),
        );
    }
    found
}

/// Draws the colourbar inside the plot area `plot` of an axes, if it has one, over everything drawn before it, and
/// records it in the hit map.
///
/// The colourbar is boxed as a boxed inside legend is, with the same background, frame, padding and inset from the
/// plot area, so that its labels read over any artist. The box lies along its edge of the plot area, inset from it,
/// `length` of the edge long (a categorical key as long as its squares need) and centred along it; the title, the
/// bar, the keys and every label lie inside it, the labels toward the interior of the plot area and the title beyond
/// them (above the top end of a standing bar).
pub(super) fn draw_inside(
    ctx: &mut Ctx,
    axes: &Axes,
    plot: Rect,
    scales: &ColourScales,
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    let Some(colorbar) = axes.colorbar else {
        return;
    };
    let Some(scale) = usable(ctx, axes.id, colorbar.scale, scales) else {
        return;
    };
    let side = colorbar.side;
    let vertical = matches!(side, InsideColorbarSide::East | InsideColorbarSide::West);
    let bar = Colorbar::new(ctx, ColorbarOwner::Axes(axes.id), scale, vertical);
    let m = Metrics::new(ctx.font_size);
    let (pad, inset) = legend::inside_box_spacing(ctx.font_size);
    let edge = if vertical { plot.height } else { plot.width };
    // Validation refuses a length that is not a fraction of the edge, so such a length is drawn as the default.
    let fraction = if colorbar.length.is_finite() && colorbar.length > 0.0 {
        colorbar.length.min(1.0)
    } else {
        0.8
    };
    let title_part = bar.title_part(&m);
    let title_width = bar.title.as_ref().map_or(0.0, TextBlock::width);
    // The length along the edge that the box leaves to the bar and its keys: its padding is taken out at both ends,
    // and a standing bar's title above its top end.
    let heading = if vertical { title_part } else { 0.0 };
    let inner = |length: f64| (length - 2.0 * pad - heading).max(0.0);
    let (measured, gap) = bar.measure_fitted(ctx, &m, inner(fraction * edge));
    // The labels under the end squares of a lying colourbar stay inside the box without crowding their neighbours.
    // The same room is left at both ends, so that the pieces stay centred in the box.
    let overhang = bar.end_overhang(&m, &measured);
    let length = if bar.has_bar() {
        fraction * edge
    } else {
        let natural = bar.natural_length(&m, gap) + 2.0 * pad + heading + 2.0 * overhang;
        if vertical {
            natural
        } else {
            natural.max(title_width + 2.0 * pad)
        }
    };
    let across = bar.across(&m, &measured);
    let depth = 2.0 * pad
        + if vertical {
            across.max(title_width)
        } else {
            across + title_part
        };
    let centre = if vertical {
        plot.y + plot.height / 2.0
    } else {
        plot.x + plot.width / 2.0
    };
    let along_start = centre - length / 2.0;
    let backdrop = match side {
        InsideColorbarSide::South => {
            Rect::new(along_start, plot.bottom() - inset - depth, length, depth)
        }
        InsideColorbarSide::North => Rect::new(along_start, plot.y + inset, length, depth),
        InsideColorbarSide::East => {
            Rect::new(plot.right() - inset - depth, along_start, depth, length)
        }
        InsideColorbarSide::West => Rect::new(plot.x + inset, along_start, depth, length),
    };
    // The content of the box, inside its padding.
    let content = Rect::new(
        backdrop.x + pad,
        backdrop.y + pad,
        (backdrop.width - 2.0 * pad).max(0.0),
        (backdrop.height - 2.0 * pad).max(0.0),
    );
    let t = m.thickness;
    let (cross, outward) = match side {
        InsideColorbarSide::South => (content.bottom() - t, -1.0),
        InsideColorbarSide::North => (content.y, 1.0),
        InsideColorbarSide::East => (content.right() - t, -1.0),
        InsideColorbarSide::West => (content.x, 1.0),
    };
    let (start, bounds) = if vertical {
        let top = content.y + heading;
        (top, (top, content.bottom()))
    } else {
        (content.x + overhang, (content.x, content.right()))
    };
    let strip = Strip {
        start,
        cross,
        outward,
        bounds: Some(bounds),
    };
    out.extend(legend::backdrop(axes.id, backdrop));
    let strip_length = (inner(length) - 2.0 * overhang).max(0.0);
    let mut hit = bar.draw_strip(ctx, &m, &measured, strip, strip_length, gap, out);
    hit.backdrop = Some(backdrop);
    // A title of the title's width centred on `at`, kept within the content of the box across its run.
    let left_of =
        |at: f64, lo: f64, hi: f64| (at - title_width / 2.0).clamp(lo, (hi - title_width).max(lo));
    let band = bar.band(&measured);
    match side {
        // A lying bar's title heads the bar beyond its labels, toward the interior of the plot area.
        InsideColorbarSide::South => {
            let bottom = cross - m.label_gap - band - m.title_gap;
            let left = left_of(centre, content.x, content.right());
            bar.draw_title(ctx, Point::new(left, bottom), false, out);
        }
        InsideColorbarSide::North => {
            let top = cross + t + m.label_gap + band + m.title_gap;
            let left = left_of(centre, content.x, content.right());
            bar.draw_title(ctx, Point::new(left, top), true, out);
        }
        // A standing bar's title heads its top end, over the bar where the box allows.
        InsideColorbarSide::East | InsideColorbarSide::West => {
            let left = left_of(cross + t / 2.0, content.x, content.right());
            bar.draw_title(ctx, Point::new(left, content.y), true, out);
        }
    }
    hits.colorbars.push(hit);
}
