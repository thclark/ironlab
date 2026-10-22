//! Panels: preparing, measuring and drawing the content of every panel of the figure.
//!
//! Everything a panel shares with every other panel, whatever it holds, is written once: `layout` fits the fitted
//! rows and columns to the panels in them and places each panel in its cells, inset by the aligned margins of the plot
//! areas beside it, and this module prepares the content of each panel, records the area of each panel in the hit map
//! and places a block of content within the area by the panel's alignment. A kind of content supplies only its size
//! along a track ([`Content::size`]) and its drawing ([`Content::draw`]), each one arm of a match, together with the
//! arm of [`prepare`] that measures it before the layout.

use ironlab_ir::{Legend, NodeId, Panel, PanelContent, PanelLocation};

use crate::display::{Item, Rect};
use crate::hit::{HitMap, PanelHit};

use super::Ctx;
use super::layout::{self, Along, Fits};
use super::legend::{self, Candidate, PanelListing};

/// The content of a panel, prepared and measured before the layout because a fitted row or column takes its size
/// from it.
pub(super) enum Content<'a> {
    /// A legend, with its entries chosen and measured.
    Legend(PanelListing<'a>),
}

/// Prepares the content of every panel of the figure, in the order the figure lists them.
///
/// `candidates` holds the legend candidates of every axes of the figure, in the order the figure lists them. When
/// two panels share an identifier, only the first is named by the axes that name that identifier; the later ones are
/// prepared as if no axes named them.
pub(super) fn prepare<'a>(
    ctx: &mut Ctx<'a>,
    candidates: &[Vec<Candidate<'a>>],
) -> Vec<Content<'a>> {
    let figure = ctx.figure;
    // Why: a panel gathers what the axes naming it contribute in the reading order of their cells, by the row and
    // then the column of their top-left tiles, so that the order follows the page rather than the order of the axes
    // in the figure, which breaks ties.
    let mut order: Vec<usize> = (0..figure.axes.len()).collect();
    order.sort_by_key(|&i| {
        let (row, col, _, _) = layout::clamp_cell(&figure.layout, figure.axes[i].cell);
        (row, col)
    });
    let mut seen: Vec<NodeId> = Vec::new();
    figure
        .panels
        .iter()
        .map(|panel| {
            let first = !seen.contains(&panel.id);
            seen.push(panel.id);
            let naming = order
                .iter()
                .copied()
                .filter(|&i| first && figure.axes[i].legend == Some(Legend::Panel(panel.id)));
            match &panel.content {
                PanelContent::Legend(content) => {
                    let gathered: Vec<&Candidate> = naming.flat_map(|i| &candidates[i]).collect();
                    Content::Legend(PanelListing::new(
                        ctx, panel.id, content, &gathered, candidates,
                    ))
                }
            }
        })
        .collect()
}

impl Content<'_> {
    /// Returns the height (along [`Along::Rows`]) or width (along [`Along::Cols`]) of the area that the content
    /// needs when the other dimension of the area is `cross`, which is infinite when that dimension is fitted too.
    pub(super) fn size(&self, font_size: f64, along: Along, cross: f64) -> f64 {
        match self {
            Content::Legend(legend) => legend.size(font_size, along, cross),
        }
    }

    /// Draws the content of `panel` in its `area`. `fitted_rows` says whether the panel lies in fitted rows alone, so
    /// that the height of its area follows its content rather than the other way round.
    fn draw(
        &self,
        ctx: &mut Ctx,
        panel: &Panel,
        area: Rect,
        fitted_rows: bool,
        out: &mut Vec<Item>,
        hits: &mut HitMap,
    ) {
        match self {
            Content::Legend(content) => {
                let fill = if fitted_rows {
                    legend::Fill::Width
                } else {
                    legend::Fill::Height
                };
                legend::draw_panel(ctx, panel, content, area, fill, out, hits);
            }
        }
    }
}

/// Draws every panel of the figure in its area, after every axes, and records the area of each in the hit map.
///
/// `areas` and `contents` hold the area and the prepared content of each panel, in the order the figure lists them.
pub(super) fn draw(
    ctx: &mut Ctx,
    fits: &Fits,
    areas: Vec<Rect>,
    contents: &[Content],
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    let figure = ctx.figure;
    for ((panel, area), content) in figure.panels.iter().zip(areas).zip(contents) {
        hits.panels.push(PanelHit {
            id: panel.id,
            rect: area,
        });
        let fitted_rows = layout::in_fitted_rows(fits, &figure.layout, panel.cell);
        content.draw(ctx, panel, area, fitted_rows, out, hits);
    }
}

/// Returns the rectangle of a block of content of the given width and height, placed in `area` at `align` and kept
/// `inset` from the edges of the area at which it is placed.
pub(super) fn place(area: Rect, width: f64, height: f64, align: PanelLocation, inset: f64) -> Rect {
    let left = area.x + inset;
    let right = area.right() - inset - width;
    let top = area.y + inset;
    let bottom = area.bottom() - inset - height;
    let cx = area.x + (area.width - width) / 2.0;
    let cy = area.y + (area.height - height) / 2.0;
    let (x, y) = match align {
        PanelLocation::NorthEast => (right, top),
        PanelLocation::NorthWest => (left, top),
        PanelLocation::SouthEast => (right, bottom),
        PanelLocation::SouthWest => (left, bottom),
        PanelLocation::North => (cx, top),
        PanelLocation::South => (cx, bottom),
        PanelLocation::East => (right, cy),
        PanelLocation::West => (left, cy),
        PanelLocation::Center => (cx, cy),
    };
    Rect::new(x, y, width, height)
}
