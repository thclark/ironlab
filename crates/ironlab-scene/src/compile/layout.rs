//! Page size, the figure title and the division of the page into tiles and plot rectangles.

use ironlab_ir::{Cell, Figure, TileLayout, TrackSize};

use crate::display::{Item, Point, Rect};

use super::Ctx;
use super::style::INK;
use super::text::measure;

/// Points per millimetre.
const PT_PER_MM: f64 = 72.0 / 25.4;
/// The figure title is this multiple of the base font size.
const FIGURE_TITLE_SCALE: f64 = 1.2;
/// The default base font size, used when the figure's is not a positive number.
const DEFAULT_FONT_SIZE: f64 = 9.0;
/// The smallest fraction of a tile's width or height that its plot rectangle keeps when decorations crowd it.
const MIN_PLOT_FRACTION: f64 = 0.2;

/// The space reserved around a plot rectangle for its decorations, in points.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Margins {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

/// Returns the figure's base font size, or the default when it is not a positive number.
pub(super) fn font_size(figure: &Figure) -> f64 {
    if figure.font_size_pt.is_finite() && figure.font_size_pt > 0.0 {
        figure.font_size_pt
    } else {
        DEFAULT_FONT_SIZE
    }
}

/// Returns the page size in points, replacing an unusable figure size with the default and warning about it.
pub(super) fn page_size(ctx: &mut Ctx) -> (f64, f64) {
    let figure = ctx.figure;
    if !(figure.font_size_pt.is_finite() && figure.font_size_pt > 0.0) {
        ctx.warn(
            Some(figure.id),
            format!(
                "The font size {} pt is not a positive number, so {DEFAULT_FONT_SIZE} pt is used.",
                figure.font_size_pt
            ),
        );
    }
    let size = figure.size;
    let usable = |v: f64| v.is_finite() && v > 0.0 && v * PT_PER_MM < 1e6;
    if usable(size.width_mm) && usable(size.height_mm) {
        (size.width_mm * PT_PER_MM, size.height_mm * PT_PER_MM)
    } else {
        let default = ironlab_ir::FigureSize::default();
        ctx.warn(
            Some(figure.id),
            format!(
                "The figure size {} mm × {} mm is not usable, so {} mm × {} mm is used.",
                size.width_mm, size.height_mm, default.width_mm, default.height_mm
            ),
        );
        (default.width_mm * PT_PER_MM, default.height_mm * PT_PER_MM)
    }
}

/// Returns the padding kept between the page edge and any content.
pub(super) fn page_padding(ctx: &Ctx) -> f64 {
    0.4 * ctx.font_size
}

/// Draws the figure title centred at the top of the page and returns the area left for the tiles.
pub(super) fn figure_title(ctx: &mut Ctx, page: Rect, out: &mut Vec<Item>) -> Rect {
    let pad = page_padding(ctx);
    let mut top = page.y + pad;
    let figure = ctx.figure;
    if let Some(title) = &figure.title {
        let block = measure(ctx, title, FIGURE_TITLE_SCALE * ctx.font_size, figure.id);
        let origin = Point::new(
            page.x + (page.width - block.width()) / 2.0,
            top + block.height(),
        );
        block.draw_on_page(ctx, origin, INK, figure.id, out);
        top += block.total_height() + 0.3 * ctx.font_size;
    }
    Rect::new(
        page.x + pad,
        top,
        (page.width - 2.0 * pad).max(1.0),
        (page.bottom() - pad - top).max(1.0),
    )
}

/// Clamps a cell into the tile layout, returning `(row, col, row_end, col_end)` with exclusive ends.
pub(super) fn clamp_cell(layout: &TileLayout, cell: Cell) -> (u32, u32, u32, u32) {
    let rows = layout.rows.max(1);
    let cols = layout.cols.max(1);
    let row = cell.row.min(rows - 1);
    let col = cell.col.min(cols - 1);
    let row_end = row.saturating_add(cell.row_span.max(1)).min(rows);
    let col_end = col.saturating_add(cell.col_span.max(1)).min(cols);
    (row, col, row_end, col_end)
}

/// Returns an estimate of the rectangle of the tile cells an axes occupies, from which its tick targets are chosen
/// before its margins are measured.
///
/// The fitted rows and columns take the shares in `fits`, and the weighted rows share the rest of the height of
/// `area`, and the weighted columns the rest of its width, in proportion to their weights; a cell block spanning
/// several rows or columns takes the sum of their sizes. [`place`] then places the axes for good, once the margins
/// are known.
pub(super) fn cell_rect(layout: &TileLayout, fits: &Fits, cell: Cell, area: Rect) -> Rect {
    let (row, col, row_end, col_end) = clamp_cell(layout, cell);
    let (x, width) = span(
        &layout.col_weights,
        &fits.cols,
        col,
        col_end,
        area.x,
        area.width,
    );
    let (y, height) = span(
        &layout.row_weights,
        &fits.rows,
        row,
        row_end,
        area.y,
        area.height,
    );
    Rect::new(x, y, width, height)
}

/// Returns the start and length of the tracks `[first, end)` of the tracks that share the interval `[start, start +
/// len]`, where `fitted` holds the share of every fitted track and `None` for every weighted one.
///
/// The fitted tracks take their shares, scaled down together when they need more than `len`, and the weighted
/// tracks share the rest in proportion to their weights. A weighted track without a weight, and a track whose weight
/// is not finite and positive, has a weight of one: compilation never fails, and validation reports such weights.
/// Equal weights therefore divide the interval exactly as no weights do.
fn span(
    weights: &[TrackSize],
    fitted: &[Option<f64>],
    first: u32,
    end: u32,
    start: f64,
    len: f64,
) -> (f64, f64) {
    let shares = Shares::new(weights, fitted, len);
    let sum = |from: u32, to: u32| {
        (from as usize..to as usize)
            .map(|k| shares.of(k))
            .sum::<f64>()
    };
    (start + sum(0, first), sum(first, end))
}

/// The shares of a length that the rows or the columns of a tile layout take, after the margins have been taken out.
struct Shares {
    /// The share of each fitted track, scaled down together when they need more than the length, or `None` for a
    /// weighted track.
    fitted: Vec<Option<f64>>,
    /// The weight of each track; zero for a fitted track.
    weights: Vec<f64>,
    /// The length of one unit of weight.
    unit: f64,
}

impl Shares {
    /// Divides `len` among tracks of the given sizes, where `fitted` holds the share that each fitted track needs.
    ///
    /// Equal weights are replaced by ones, so that equal weights of any size divide the length exactly as no weights
    /// do; a weighted track without a weight, or whose weight is not finite and positive, has a weight of one.
    fn new(sizes: &[TrackSize], fitted: &[Option<f64>], len: f64) -> Self {
        let given = |k: usize| match sizes.get(k) {
            Some(&TrackSize::Weight(w)) if w.is_finite() && w > 0.0 => w,
            _ => 1.0,
        };
        let weighted: Vec<usize> = (0..fitted.len()).filter(|&k| fitted[k].is_none()).collect();
        let uniform = weighted.iter().all(|&k| given(k) == given(weighted[0]));
        let weights: Vec<f64> = (0..fitted.len())
            .map(|k| match fitted[k] {
                Some(_) => 0.0,
                None if uniform => 1.0,
                None => given(k),
            })
            .collect();
        let needed: f64 = fitted.iter().flatten().sum();
        // Fitted tracks that need more than the length share it in proportion to their needs, leaving the weighted
        // tracks nothing; the legends they hold then report that they overflow.
        let shrink = if needed > len { len / needed } else { 1.0 };
        let fitted: Vec<Option<f64>> = fitted.iter().map(|f| f.map(|s| s * shrink)).collect();
        let total_weight: f64 = weights.iter().sum();
        let unit = if total_weight > 0.0 {
            (len - needed * shrink).max(0.0) / total_weight
        } else {
            0.0
        };
        Self {
            fitted,
            weights,
            unit,
        }
    }

    /// Returns the share of track `k`.
    fn of(&self, k: usize) -> f64 {
        self.fitted[k].unwrap_or(self.weights[k] * self.unit)
    }
}

/// The shares, in points, that the fitted rows and columns of the tile layout take of the space the margins leave,
/// from the top and from the left; a weighted row or column has none.
///
/// The share of a fitted track holds the area of the panels in it, and the page padding on a side of the track
/// at which no axes starts or ends, because a panel is inset by the padding there; the aligned margins of the axes
/// are taken out of the page before the shares, as for every track.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Fits {
    rows: Vec<Option<f64>>,
    cols: Vec<Option<f64>>,
}

impl Fits {
    /// Returns a share of zero for every fitted row and column of the layout, which is the share of a fitted track
    /// that holds no panel.
    pub(super) fn new(layout: &TileLayout) -> Self {
        let fitted = |sizes: &[TrackSize], count: u32| -> Vec<Option<f64>> {
            (0..count.max(1) as usize)
                .map(|k| (sizes.get(k) == Some(&TrackSize::Fit)).then_some(0.0))
                .collect()
        };
        Self {
            rows: fitted(&layout.row_weights, layout.rows),
            cols: fitted(&layout.col_weights, layout.cols),
        }
    }

    /// Returns whether the layout has any fitted row or column.
    pub(super) fn any(&self) -> bool {
        self.rows.iter().chain(&self.cols).any(Option::is_some)
    }

    /// Returns whether every share differs from that of `other` by less than a millionth of a point, so that laying
    /// the figure out again would move nothing visibly.
    pub(super) fn settled(&self, other: &Fits) -> bool {
        let close = |a: &[Option<f64>], b: &[Option<f64>]| {
            a.iter().zip(b).all(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-6,
                (a, b) => a == b,
            })
        };
        close(&self.rows, &other.rows) && close(&self.cols, &other.cols)
    }
}

/// The dimension of a panel's content that a fitted track takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Along {
    /// The height, which a fitted row takes.
    Rows,
    /// The width, which a fitted column takes.
    Cols,
}

/// Computes the share of every fitted row and column from the content of the panels that lie in it alone.
///
/// `content(p, along, cross)` returns the height (for [`Along::Rows`]) or width (for [`Along::Cols`]) of the area
/// that panel `p`, in the order the figure lists them, needs for its entries when the other dimension of its
/// area is `cross`, which is infinite when that dimension is fitted too. A fitted track's share is the largest such
/// area of its panels, with the page padding added on a side at which no axes starts or ends, and zero when no panel
/// lies in it alone. `margins` are the margins of the axes, or `None` before they are known. The cross dimension of a
/// panel is measured with the shares of `previous`: the columns are fitted to the rows of `previous`, the rows to
/// those columns, and the columns again to those rows. A panel whose row is fitted as well as its column is given an
/// unbounded height when its column is fitted, and its row is then fitted to the width that results. `panel_tops`
/// holds the margin each panel needs above its area, as [`place`] takes it.
pub(super) fn fit_tracks(
    ctx: &Ctx,
    grid_area: Rect,
    margins: Option<&[Margins]>,
    panel_tops: &[f64],
    previous: &Fits,
    content: impl Fn(&Ctx, usize, Along, f64) -> f64,
) -> Fits {
    let figure = ctx.figure;
    let layout = &figure.layout;
    let aligned = AlignedMargins::new(figure, margins.unwrap_or(&[]), panel_tops);
    let pad = page_padding(ctx);
    let padding = |lo: Option<f64>, hi: Option<f64>| {
        f64::from(u8::from(lo.is_none())) * pad + f64::from(u8::from(hi.is_none())) * pad
    };
    let mut fits = previous.clone();
    let fit_cols = |fits: &mut Fits| {
        let mut shares = Fits::new(layout).cols;
        let rows = aligned.rows(layout, fits, grid_area);
        for (p, panel) in figure.panels.iter().enumerate() {
            let (r, c, re, ce) = span_indices(layout, panel.cell);
            if c != ce || shares[c].is_none() {
                continue;
            }
            let cross = if r == re && fits.rows[r].is_some() {
                f64::INFINITY
            } else {
                rows.inset(r, re, pad).1
            };
            let need =
                content(ctx, p, Along::Cols, cross) + padding(aligned.left[c], aligned.right[c]);
            shares[c] = shares[c].map(|s| s.max(need));
        }
        fits.cols = shares;
    };
    let fit_rows = |fits: &mut Fits| {
        let mut shares = Fits::new(layout).rows;
        // The width is known even when the columns are fitted too, because they are fitted first.
        let columns = aligned.columns(layout, fits, grid_area);
        for (p, panel) in figure.panels.iter().enumerate() {
            let (r, c, re, ce) = span_indices(layout, panel.cell);
            if r != re || shares[r].is_none() {
                continue;
            }
            let cross = columns.inset(c, ce, pad).1;
            let need =
                content(ctx, p, Along::Rows, cross) + padding(aligned.top[r], aligned.bottom[r]);
            shares[r] = shares[r].map(|s| s.max(need));
        }
        fits.rows = shares;
    };
    fit_cols(&mut fits);
    fit_rows(&mut fits);
    fit_cols(&mut fits);
    fits
}

/// Returns whether a panel lies in fitted rows alone, so that its height follows its entries rather than the
/// other way round.
pub(super) fn in_fitted_rows(fits: &Fits, layout: &TileLayout, cell: Cell) -> bool {
    let (r, _, re, _) = span_indices(layout, cell);
    (r..=re).all(|k| fits.rows[k].is_some())
}

/// The margins of the plot areas, aligned across the tile layout: the largest left margin of the axes starting in
/// each column, the largest right margin of those ending in each column, and likewise the top and bottom margins by
/// row. A row or column in which no axes starts or ends has no margin on that side. A panel that needs a margin above
/// its area, for the heading of a colourbar that stands along its rows, widens the top margin of its first row in
/// the same way, so that the margin is the largest that any cell of the row needs rather than a sum.
struct AlignedMargins {
    left: Vec<Option<f64>>,
    right: Vec<Option<f64>>,
    top: Vec<Option<f64>>,
    bottom: Vec<Option<f64>>,
}

impl AlignedMargins {
    fn new(figure: &Figure, margins: &[Margins], panel_tops: &[f64]) -> Self {
        let rows = figure.layout.rows.max(1) as usize;
        let cols = figure.layout.cols.max(1) as usize;
        let mut aligned = Self {
            left: vec![None; cols],
            right: vec![None; cols],
            top: vec![None; rows],
            bottom: vec![None; rows],
        };
        let widen = |slot: &mut Option<f64>, m: f64| *slot = Some(slot.map_or(m, |s| s.max(m)));
        for (axes, m) in figure.axes.iter().zip(margins) {
            let (r, c, re, ce) = span_indices(&figure.layout, axes.cell);
            widen(&mut aligned.left[c], m.left);
            widen(&mut aligned.right[ce], m.right);
            widen(&mut aligned.top[r], m.top);
            widen(&mut aligned.bottom[re], m.bottom);
        }
        for (panel, &top) in figure.panels.iter().zip(panel_tops) {
            if top > 0.0 {
                let (r, ..) = span_indices(&figure.layout, panel.cell);
                widen(&mut aligned.top[r], top);
            }
        }
        aligned
    }

    /// Divides the height of `area` into the rows of the layout.
    fn rows(&self, layout: &TileLayout, fits: &Fits, area: Rect) -> Tracks {
        Tracks::new(
            &layout.row_weights,
            &fits.rows,
            &self.top,
            &self.bottom,
            area.y,
            area.height,
        )
    }

    /// Divides the width of `area` into the columns of the layout.
    fn columns(&self, layout: &TileLayout, fits: &Fits, area: Rect) -> Tracks {
        Tracks::new(
            &layout.col_weights,
            &fits.cols,
            &self.left,
            &self.right,
            area.x,
            area.width,
        )
    }
}

/// Returns the first and last row and column of a cell block, clamped into the layout, as indices.
fn span_indices(layout: &TileLayout, cell: Cell) -> (usize, usize, usize, usize) {
    let (r, c, re, ce) = clamp_cell(layout, cell);
    (r as usize, c as usize, re as usize - 1, ce as usize - 1)
}

/// The division of one direction of the grid area (its width into columns, or its height into rows) into tracks.
///
/// Each track holds the aligned margins of its row or column on either side of its share of the plot space. The
/// fitted tracks take their shares first, and the weighted tracks share the rest in proportion to their weights,
/// after every margin has been taken out of the length, so that two plot areas of equal weight are of equal size
/// whatever their margins and whatever the fitted tracks beside them.
struct Tracks {
    /// The start of each track, and the end of the last.
    bounds: Vec<f64>,
    /// The aligned margin at the start of each track, as given and scaled by `squeeze`; `None` where no axes starts.
    lo: Vec<Option<f64>>,
    /// The aligned margin at the end of each track, as given and scaled by `squeeze`; `None` where no axes ends.
    hi: Vec<Option<f64>>,
}

impl Tracks {
    /// Divides `[start, start + len]` into tracks of the given sizes and aligned margins, where `fitted` holds the
    /// share of every fitted track (and so the number of tracks).
    ///
    /// The fitted tracks take their shares of what the margins leave, as [`Shares`] describes, and the weighted
    /// tracks the rest. When the margins leave less than [`MIN_PLOT_FRACTION`] of the length to the plots, every
    /// margin is scaled down by the same factor so that the plots keep that fraction.
    fn new(
        sizes: &[TrackSize],
        fitted: &[Option<f64>],
        lo: &[Option<f64>],
        hi: &[Option<f64>],
        start: f64,
        len: f64,
    ) -> Self {
        let count = fitted.len();

        let len = len.max(1.0);
        let margin = |slot: Option<f64>| slot.filter(|m| m.is_finite() && *m > 0.0).unwrap_or(0.0);
        let margins: f64 = lo.iter().chain(hi).map(|m| margin(*m)).sum();
        let keep = MIN_PLOT_FRACTION * len;
        let squeeze = if len - margins >= keep {
            1.0
        } else {
            (len - keep) / margins.max(f64::MIN_POSITIVE)
        };
        let plots = len - squeeze * margins;
        let scaled = |slot: Option<f64>| slot.map(|m| squeeze * margin(Some(m)));
        let lo: Vec<Option<f64>> = lo.iter().map(|m| scaled(*m)).collect();
        let hi: Vec<Option<f64>> = hi.iter().map(|m| scaled(*m)).collect();

        let shares = Shares::new(sizes, fitted, plots);
        let mut bounds = Vec::with_capacity(count + 1);
        let mut at = start;
        bounds.push(at);
        for k in 0..count {
            at += lo[k].unwrap_or(0.0) + shares.of(k) + hi[k].unwrap_or(0.0);
            bounds.push(at);
        }
        Self { bounds, lo, hi }
    }

    /// Returns the start and length of the tracks `first..=last`, inset at either end by the aligned margin there, or
    /// by `default` where the track has none.
    fn inset(&self, first: usize, last: usize, default: f64) -> (f64, f64) {
        let lo = self.lo[first].unwrap_or(default);
        let hi = self.hi[last].unwrap_or(default);
        let (start, end) = (self.bounds[first] + lo, self.bounds[last + 1] - hi);
        (start, (end - start).max(1.0))
    }

    /// Returns the start and length of the tracks `first..=last`.
    fn span(&self, first: usize, last: usize) -> (f64, f64) {
        (
            self.bounds[first],
            self.bounds[last + 1] - self.bounds[first],
        )
    }
}

/// Where every axes and panel of a figure is placed, once the margins of the axes are known.
pub(super) struct Placement {
    /// The rectangle of the cells of each axes, in the order the figure lists them.
    pub outer: Vec<Rect>,
    /// The plot rectangle of each axes, in the order the figure lists them.
    pub plots: Vec<Rect>,
    /// The area of each panel, in the order the figure lists them.
    pub panels: Vec<Rect>,
}

/// Places every axes and panel of the figure in the grid area.
///
/// Margins are aligned across the layout: every axes that starts in a column uses the largest left margin of the
/// axes starting there, and likewise for right margins by end column and for top and bottom margins by row, so the
/// plot areas of neighbouring tiles line up. The rows then share the height of the grid area, and the columns its
/// width, so that every plot area takes its share of what the margins leave in proportion to the weights of its rows
/// and columns; plot areas of equal weight are therefore of equal size, however wide the tick labels and axis labels
/// beside them are. A cell block spanning several rows or columns takes their shares and the margins between them.
///
/// The fitted rows and columns take the shares in `fits` first, and the plot areas of the weighted ones share what is
/// left, so that plot areas of equal weight stay of equal size beside a fitted track.
///
/// A panel is placed in its cells as an axes is, then inset by the aligned margins of the rows and columns it
/// occupies, so that its edges line up with the plot areas of the axes beside, above or below it. A side whose row or
/// column holds no axes is inset by the page padding instead. A panel contributes no margins of its own, except the
/// margin above its area that `panel_tops` gives it, in the order the figure lists the panels, which widens the top
/// margin of its first row as an axes title does.
pub(super) fn place(
    ctx: &Ctx,
    fits: &Fits,
    grid_area: Rect,
    margins: &[Margins],
    panel_tops: &[f64],
) -> Placement {
    let figure = ctx.figure;
    let layout = &figure.layout;
    let aligned = AlignedMargins::new(figure, margins, panel_tops);
    let columns = aligned.columns(layout, fits, grid_area);
    let rows = aligned.rows(layout, fits, grid_area);
    let rect = |cell: Cell, inset: Option<f64>| {
        let (r, c, re, ce) = span_indices(layout, cell);
        let ((x, width), (y, height)) = match inset {
            Some(default) => (columns.inset(c, ce, default), rows.inset(r, re, default)),
            None => (columns.span(c, ce), rows.span(r, re)),
        };
        Rect::new(x, y, width, height)
    };
    let pad = page_padding(ctx);
    Placement {
        outer: figure.axes.iter().map(|a| rect(a.cell, None)).collect(),
        plots: figure
            .axes
            .iter()
            .map(|a| rect(a.cell, Some(0.0)))
            .collect(),
        panels: figure
            .panels
            .iter()
            .map(|p| rect(p.cell, Some(pad)))
            .collect(),
    }
}
