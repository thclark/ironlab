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
fn clamp_cell(layout: TileLayout, cell: Cell) -> (u32, u32, u32, u32) {
    let rows = layout.rows.max(1);
    let cols = layout.cols.max(1);
    let row = cell.row.min(rows - 1);
    let col = cell.col.min(cols - 1);
    let row_end = row.saturating_add(cell.row_span.max(1)).min(rows);
    let col_end = col.saturating_add(cell.col_span.max(1)).min(cols);
    (row, col, row_end, col_end)
}

/// Returns the rectangle of the tile cells an axes occupies.
pub(super) fn cell_rect(layout: TileLayout, cell: Cell, area: Rect) -> Rect {
    let (row, col, row_end, col_end) = clamp_cell(layout, cell);
    let cw = area.width / f64::from(layout.cols.max(1));
    let rh = area.height / f64::from(layout.rows.max(1));
    Rect::new(
        area.x + f64::from(col) * cw,
        area.y + f64::from(row) * rh,
        f64::from(col_end - col) * cw,
        f64::from(row_end - row) * rh,
    )
}

/// Computes the plot rectangle of every axes.
///
/// Margins are aligned across the layout: every axes that starts in a column uses the largest left margin of the
/// axes starting there, and likewise for right margins by end column and for top and bottom margins by row, so the
/// plot areas of neighbouring tiles line up.
pub(super) fn plot_rects(figure: &Figure, outer: &[Rect], margins: &[Margins]) -> Vec<Rect> {
    let rows = figure.layout.rows.max(1) as usize;
    let cols = figure.layout.cols.max(1) as usize;
    let mut left = vec![0.0f64; cols];
    let mut right = vec![0.0f64; cols];
    let mut top = vec![0.0f64; rows];
    let mut bottom = vec![0.0f64; rows];
    let spans: Vec<(usize, usize, usize, usize)> = figure
        .axes
        .iter()
        .map(|axes| {
            let (r, c, re, ce) = clamp_cell(figure.layout, axes.cell);
            (r as usize, c as usize, re as usize - 1, ce as usize - 1)
        })
        .collect();
    for (&(r, c, re, ce), m) in spans.iter().zip(margins) {
        left[c] = left[c].max(m.left);
        right[ce] = right[ce].max(m.right);
        top[r] = top[r].max(m.top);
        bottom[re] = bottom[re].max(m.bottom);
    }
    spans
        .iter()
        .zip(outer)
        .map(|(&(r, c, re, ce), o)| {
            let (x, width) = inset(o.x, o.width, left[c], right[ce]);
            let (y, height) = inset(o.y, o.height, top[r], bottom[re]);
            Rect::new(x, y, width, height)
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
