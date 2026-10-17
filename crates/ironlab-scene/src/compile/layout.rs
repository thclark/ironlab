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
        block.draw(origin, INK, figure.id, out);
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

/// The sizes, in points, of the fitted rows and columns of the tile layout, from the top and from the left; a
/// weighted row or column has none.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Fits {
    rows: Vec<Option<f64>>,
    cols: Vec<Option<f64>>,
}

impl Fits {
    /// Returns a size of zero for every fitted row and column of the layout, which is the size of a fitted track
    /// that holds no legend panel.
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

    /// Returns whether every size differs from that of `other` by less than a millionth of a point, so that laying
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

/// The dimension of a legend panel's content that a fitted track takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Along {
    /// The height, which a fitted row takes.
    Rows,
    /// The width, which a fitted column takes.
    Cols,
}

/// Returns the rectangle of the tile cells an axes or a legend panel occupies.
///
/// The fitted rows take their sizes from `fits`, and the weighted rows share the rest of the height of `area` in
/// proportion to their weights; the columns share its width in the same way. A cell block spanning several rows or
/// columns takes the sum of their sizes.
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
/// len]`, where `fitted` holds the size of every fitted track and `None` for every weighted one.
///
/// The fitted tracks take their sizes, scaled down together when they need more than `len`, and the weighted tracks
/// share the rest in proportion to their weights. A weighted track without a weight, and a track whose weight is not
/// finite and positive, has a weight of one: compilation never fails, and validation reports such weights. Equal
/// weights therefore divide the interval exactly as no weights do.
fn span(
    weights: &[TrackSize],
    fitted: &[Option<f64>],
    first: u32,
    end: u32,
    start: f64,
    len: f64,
) -> (f64, f64) {
    let count = fitted.len() as u32;
    let fit = |k: u32| fitted[k as usize];
    let given = |k: u32| match weights.get(k as usize) {
        Some(&TrackSize::Weight(w)) if w.is_finite() && w > 0.0 => w,
        _ => 1.0,
    };
    let weighted: Vec<u32> = (0..count).filter(|&k| fit(k).is_none()).collect();
    // Equal weights are replaced by ones, so that the boundaries match the unweighted division bit for bit.
    let uniform = weighted.iter().all(|&k| given(k) == given(weighted[0]));
    let weight = |k: u32| match fit(k) {
        Some(_) => 0.0,
        None if uniform => 1.0,
        None => given(k),
    };
    let needed: f64 = (0..count).filter_map(fit).sum();
    // Fitted tracks that need more than the interval share it in proportion to their sizes, leaving the weighted
    // tracks nothing; the legends they hold then report that they overflow.
    let shrink = if needed > len { len / needed } else { 1.0 };
    let size = |k: u32| fit(k).map_or(0.0, |s| s * shrink);
    let total_weight: f64 = (0..count).map(weight).sum();
    let unit = if total_weight > 0.0 {
        (len - needed * shrink).max(0.0) / total_weight
    } else {
        0.0
    };
    let fitted_in = |from: u32, to: u32| (from..to).map(size).sum::<f64>();
    let weight_in = |from: u32, to: u32| (from..to).map(weight).sum::<f64>();
    (
        start + fitted_in(0, first) + weight_in(0, first) * unit,
        fitted_in(first, end) + weight_in(first, end) * unit,
    )
}

/// Computes the size of every fitted row and column from the content of the legend panels that lie in it alone.
///
/// `content(p, along, cross)` returns the height (for [`Along::Rows`]) or width (for [`Along::Cols`]) of the area
/// that legend panel `p`, in the order the figure lists them, needs for its entries when the other dimension of its
/// area is `cross`, which is infinite when that dimension is fitted too. A fitted track is as large as the largest
/// such area of its panels with the margins that align each with the plot areas around it (the page padding on a
/// side whose row or column holds no axes) added, and zero when no panel lies in it alone. `margins` are the margins
/// of the axes, or `None` before they are known, when every side takes the page padding. The cross dimension of a
/// panel is measured with the sizes of `previous`: the columns are fitted to the rows of `previous`, the rows to
/// those columns, and the columns again to those rows. A panel whose row is fitted as well as its column is given
/// an unbounded height when its column is fitted, and its row is then fitted to the width that results.
pub(super) fn fit_tracks(
    ctx: &Ctx,
    grid_area: Rect,
    margins: Option<&[Margins]>,
    previous: &Fits,
    content: impl Fn(usize, Along, f64) -> f64,
) -> Fits {
    let figure = ctx.figure;
    let layout = &figure.layout;
    let aligned = match margins {
        Some(margins) => AlignedMargins::new(figure, margins),
        None => AlignedMargins::none(figure),
    };
    let pad = page_padding(ctx);
    let side = |slot: Option<f64>| slot.unwrap_or(pad);
    let mut fits = previous.clone();
    let fit_cols = |fits: &mut Fits| {
        let mut sizes = Fits::new(layout).cols;
        for (p, panel) in figure.panel_legends.iter().enumerate() {
            let (r, c, re, ce) = span_indices(layout, panel.cell);
            if c != ce || sizes[c].is_none() {
                continue;
            }
            let cross = if r == re && fits.rows[r].is_some() {
                f64::INFINITY
            } else {
                let outer = cell_rect(layout, fits, panel.cell, grid_area);
                aligned.inset(layout, panel.cell, outer, pad).height
            };
            let need =
                content(p, Along::Cols, cross) + side(aligned.left[c]) + side(aligned.right[c]);
            sizes[c] = sizes[c].map(|s| s.max(need));
        }
        fits.cols = sizes;
    };
    let fit_rows = |fits: &mut Fits| {
        let mut sizes = Fits::new(layout).rows;
        for (p, panel) in figure.panel_legends.iter().enumerate() {
            let (r, _, re, _) = span_indices(layout, panel.cell);
            if r != re || sizes[r].is_none() {
                continue;
            }
            // The width is known even when the columns are fitted too, because they are fitted first.
            let outer = cell_rect(layout, fits, panel.cell, grid_area);
            let cross = aligned.inset(layout, panel.cell, outer, pad).width;
            let need =
                content(p, Along::Rows, cross) + side(aligned.top[r]) + side(aligned.bottom[r]);
            sizes[r] = sizes[r].map(|s| s.max(need));
        }
        fits.rows = sizes;
    };
    fit_cols(&mut fits);
    fit_rows(&mut fits);
    fit_cols(&mut fits);
    fits
}

/// The margins of the plot areas, aligned across the tile layout: the largest left margin of the axes starting in
/// each column, the largest right margin of those ending in each column, and likewise the top and bottom margins by
/// row. A row or column in which no axes starts or ends has no margin on that side.
struct AlignedMargins {
    left: Vec<Option<f64>>,
    right: Vec<Option<f64>>,
    top: Vec<Option<f64>>,
    bottom: Vec<Option<f64>>,
}

impl AlignedMargins {
    /// Returns aligned margins in which no row or column has a margin, as before the margins of the axes are known.
    fn none(figure: &Figure) -> Self {
        Self::new(figure, &[])
    }

    fn new(figure: &Figure, margins: &[Margins]) -> Self {
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
        aligned
    }

    /// Insets the rectangle of `cell` by the aligned margins of the rows and columns it starts and ends in, using
    /// `default` on a side whose row or column has no margin.
    fn inset(&self, layout: &TileLayout, cell: Cell, outer: Rect, default: f64) -> Rect {
        let (r, c, re, ce) = span_indices(layout, cell);
        let side = |slot: Option<f64>| slot.unwrap_or(default);
        let (x, width) = inset(
            outer.x,
            outer.width,
            side(self.left[c]),
            side(self.right[ce]),
        );
        let (y, height) = inset(
            outer.y,
            outer.height,
            side(self.top[r]),
            side(self.bottom[re]),
        );
        Rect::new(x, y, width, height)
    }
}

/// Returns the first and last row and column of a cell block, clamped into the layout, as indices.
fn span_indices(layout: &TileLayout, cell: Cell) -> (usize, usize, usize, usize) {
    let (r, c, re, ce) = clamp_cell(layout, cell);
    (r as usize, c as usize, re as usize - 1, ce as usize - 1)
}

/// Computes the plot rectangle of every axes.
///
/// Margins are aligned across the layout: every axes that starts in a column uses the largest left margin of the
/// axes starting there, and likewise for right margins by end column and for top and bottom margins by row, so the
/// plot areas of neighbouring tiles line up.
pub(super) fn plot_rects(figure: &Figure, outer: &[Rect], margins: &[Margins]) -> Vec<Rect> {
    let aligned = AlignedMargins::new(figure, margins);
    figure
        .axes
        .iter()
        .zip(outer)
        .map(|(axes, o)| aligned.inset(&figure.layout, axes.cell, *o, 0.0))
        .collect()
}

/// Computes the area of every legend panel of the figure, in the order the figure lists them.
///
/// A panel is placed in its cells as an axes is, then inset by the aligned margins of the rows and columns it
/// occupies, so that its edges line up with the plot areas of the axes beside, above or below it. A side whose row
/// or column holds no axes is inset by the page padding instead. A panel contributes no margins of its own.
pub(super) fn panel_rects(
    ctx: &Ctx,
    fits: &Fits,
    grid_area: Rect,
    margins: &[Margins],
) -> Vec<Rect> {
    let figure = ctx.figure;
    let aligned = AlignedMargins::new(figure, margins);
    let pad = page_padding(ctx);
    figure
        .panel_legends
        .iter()
        .map(|panel| {
            let outer = cell_rect(&figure.layout, fits, panel.cell, grid_area);
            aligned.inset(&figure.layout, panel.cell, outer, pad)
        })
        .collect()
}

/// Returns whether a legend panel lies in fitted rows alone, so that its height follows its entries rather than the
/// other way round.
pub(super) fn in_fitted_rows(fits: &Fits, layout: &TileLayout, cell: Cell) -> bool {
    let (r, _, re, _) = span_indices(layout, cell);
    (r..=re).all(|k| fits.rows[k].is_some())
}

/// Insets the interval `[start, start + len]` by `lo` and `hi`, keeping at least a minimum fraction of its length.
fn inset(start: f64, len: f64, lo: f64, hi: f64) -> (f64, f64) {
    let len = len.max(1.0);
    let keep = MIN_PLOT_FRACTION * len;
    let available = len - lo - hi;
    if available >= keep {
        (start + lo, available)
    } else {
        let squeeze = (len - keep) / (lo + hi).max(f64::MIN_POSITIVE);
        (start + lo * squeeze, keep)
    }
}
