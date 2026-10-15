//! Page size, the figure title and the division of the page into tiles and plot rectangles.

use ironlab_ir::{Cell, Figure, TileLayout};

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

/// Returns the rectangle of the tile cells an axes or a legend panel occupies.
///
/// The rows share the height of `area`, and the columns its width, in proportion to their weights; a cell block
/// spanning several rows or columns takes the sum of their weights.
pub(super) fn cell_rect(layout: &TileLayout, cell: Cell, area: Rect) -> Rect {
    let (row, col, row_end, col_end) = clamp_cell(layout, cell);
    let (x, width) = span(
        &layout.col_weights,
        layout.cols,
        col,
        col_end,
        area.x,
        area.width,
    );
    let (y, height) = span(
        &layout.row_weights,
        layout.rows,
        row,
        row_end,
        area.y,
        area.height,
    );
    Rect::new(x, y, width, height)
}

/// Returns the start and length of the tracks `[first, end)` of `count` weighted tracks that share the interval
/// `[start, start + len]`.
///
/// A track without a weight, and a track whose weight is not finite and positive, has a weight of one: compilation
/// never fails, and validation reports such weights. Equal weights therefore divide the interval exactly as no
/// weights do.
fn span(weights: &[f64], count: u32, first: u32, end: u32, start: f64, len: f64) -> (f64, f64) {
    let given = |k: u32| match weights.get(k as usize) {
        Some(&w) if w.is_finite() && w > 0.0 => w,
        _ => 1.0,
    };
    let count = count.max(1);
    // Equal weights are replaced by ones, so that the boundaries match the unweighted division bit for bit.
    let uniform = (0..count).all(|k| given(k) == given(0));
    let weight = |k: u32| if uniform { 1.0 } else { given(k) };
    let sum = |from: u32, to: u32| (from..to).map(weight).sum::<f64>();
    let unit = len / sum(0, count);
    (start + sum(0, first) * unit, sum(first, end) * unit)
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
pub(super) fn panel_rects(ctx: &Ctx, grid_area: Rect, margins: &[Margins]) -> Vec<Rect> {
    let figure = ctx.figure;
    let aligned = AlignedMargins::new(figure, margins);
    let pad = page_padding(ctx);
    figure
        .panel_legends
        .iter()
        .map(|panel| {
            let outer = cell_rect(&figure.layout, panel.cell, grid_area);
            aligned.inset(&figure.layout, panel.cell, outer, pad)
        })
        .collect()
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
