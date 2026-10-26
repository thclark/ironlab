//! Cells: a grid of rectangles between column and row edges, each coloured by its value through a colour scale.
//!
//! Each cell is drawn as one closed rectangle, filled with its colour and outlined by its edge, so that the cells of
//! a matrix are counted, read and exported one by one. The four edges of a cell are mapped onto the page through the
//! axes, and the gap is then taken out of its extent on the page, half on each side, so that cells on a logarithmic
//! or reversed axis are inset symmetrically, as bars are.

use ironlab_ir::{Cells, NdArray, Values};

use crate::display::Rgba;
use crate::hit::{AxisMap, BinReading, CellHit};

use super::artists::{Draw, Space};
use super::data::CellsData;
use super::paths::{self, PathBuilder};
use super::rects::{CLAMP_MARGIN, clamped_rect, expand, inset, intersection, unit_gap};
use super::style::{self, Paint, ResolvedScale};

/// Returns the drawn span on the page of each column or row of cells bounded by `edges`, as its two ends in either
/// order, or `None` for one bounded by an edge that a logarithmic axis cannot place.
fn spans(map: &AxisMap, edges: &[f64], gap: f64) -> Vec<Option<(f64, f64)>> {
    edges
        .windows(2)
        .map(|pair| {
            if map.log && (pair[0] <= 0.0 || pair[1] <= 0.0) {
                return None;
            }
            let (start, span) = inset(map.to_figure(pair[0]), map.to_figure(pair[1]), gap);
            let end = start + span;
            (start.is_finite() && end.is_finite()).then_some((start, end))
        })
        .collect()
}

/// Returns the alpha of cell `k` from the alpha array of the cells: a floating-point alpha clamped to `[0, 1]`, 0 for
/// one that is not finite, and an 8-bit alpha divided by 255, as the alpha of a scalar image is read.
fn alpha_at(alpha: Option<&NdArray>, k: usize) -> f64 {
    match alpha.map(|alpha| &alpha.values) {
        None => 1.0,
        Some(Values::F64(values)) if values[k].is_finite() => values[k].clamp(0.0, 1.0),
        Some(Values::F64(_)) => 0.0,
        Some(Values::U8(values)) => f64::from(values[k]) / 255.0,
    }
}

/// Returns the value of cell `k` as a number: a floating-point value as it is, and an 8-bit value as the whole number
/// it denotes.
fn value_at(values: &NdArray, k: usize) -> f64 {
    match &values.values {
        Values::F64(values) => values[k],
        Values::U8(values) => f64::from(values[k]),
    }
}

/// Returns the reading of the bin that cell `(j, i)` draws when the cells record the binning of a two-dimensional
/// histogram: its count and density, recovered from its value, its area and the record. A cumulative value is first
/// differenced with the cells before it along both axes, by inclusion and exclusion, so that the count is that of
/// the cell alone.
fn bin_of(cells: &Cells, data: &CellsData, j: usize, i: usize) -> Option<BinReading> {
    let binning = cells.binning.as_ref()?;
    let cumulative = |j: Option<usize>, i: Option<usize>| match (j, i) {
        (Some(j), Some(i)) => {
            let v = value_at(data.values, j * data.nx + i);
            if v.is_finite() { v } else { 0.0 }
        }
        _ => 0.0,
    };
    let (before_j, before_i) = (j.checked_sub(1), i.checked_sub(1));
    let before = cumulative(Some(j), before_i) + cumulative(before_j, Some(i))
        - cumulative(before_j, before_i);
    // The area of the bin between the record's edges, which the drawn cell may not span when the cells are set apart
    // by a gap.
    let (x_edges, y_edges) = data.bin_edges?;
    let area = (x_edges.get(i + 1)? - x_edges.get(i)?) * (y_edges.get(j + 1)? - y_edges.get(j)?);
    BinReading::recover(
        binning.normalization,
        binning.total,
        value_at(data.values, j * data.nx + i),
        before,
        area,
    )
}

/// Draws every cell as a closed rectangle, records each in the hit map, and counts the cells drawn as dense content,
/// as the faces of a surface are counted.
///
/// The colour of a cell is the colour of its value through `scale`, the colour scale of the cells, which follows the
/// scale's out-of-range policies; an 8-bit value is mapped as the whole number it denotes. A cell that the scale
/// leaves transparent, such as the NaN padding of a ragged row, or whose alpha is 0, is not a thing on the page: it
/// draws neither face nor outline and has no hit. Otherwise the face takes the colour faded by the cell's alpha and
/// the face opacity, and the outline takes the colour of its edge (the cell's own colour when the edge is automatic
/// or colormapped) faded by the cell's alpha and the edge opacity. A cell bounded by an edge that a logarithmic axis
/// cannot place, and a cell wholly outside the plot area, is not drawn.
pub(super) fn draw_cells(
    draw: &mut Draw,
    cells: &Cells,
    data: &CellsData,
    scale: Option<&ResolvedScale>,
) {
    let Space::TwoD { x, y } = draw.space else {
        return;
    };
    let columns = spans(&x, &data.columns, unit_gap(cells.gap.x));
    let rows = spans(&y, &data.rows, unit_gap(cells.gap.y));
    let map = |value: f64| scale.and_then(|scale| scale.colour(value));
    // The 256 values a byte can hold are mapped once each.
    let table: Option<Vec<Option<Rgba>>> = match &data.values.values {
        Values::U8(_) => Some((0..=u8::MAX).map(|v| map(f64::from(v))).collect()),
        Values::F64(_) => None,
    };
    let colour_at = |k: usize| match &data.values.values {
        Values::F64(values) => map(values[k]),
        Values::U8(values) => table.as_ref().and_then(|t| t[usize::from(values[k])]),
    };
    let width = style::width_or(cells.edge.width_pt, 0.5);
    let dash = style::dash_array(cells.edge.dash, width);
    let face_opacity = style::unit_opacity(cells.face_opacity);
    let edge_opacity = style::unit_opacity(cells.edge_opacity);
    let edge_paint = style::resolve(cells.edge.color, Paint::Colormapped);
    let bounds = expand(draw.clip, CLAMP_MARGIN);
    let mut drawn: u64 = 0;
    for (j, row) in rows.iter().enumerate() {
        let Some((y0, y1)) = *row else { continue };
        for (i, column) in columns.iter().enumerate() {
            let Some((x0, x1)) = *column else { continue };
            let k = j * data.nx + i;
            let Some(colour) = colour_at(k).filter(|c| c.a > 0.0) else {
                continue;
            };
            let alpha = alpha_at(data.alpha, k);
            if alpha <= 0.0 {
                continue;
            }
            let rect = clamped_rect(x0, x1, y0, y1, bounds);
            let Some(seen) = intersection(rect, draw.clip) else {
                continue;
            };
            let face_factor = alpha * face_opacity;
            let fill = (face_factor > 0.0)
                .then(|| paths::fill(colour.with_alpha_factor(face_factor as f32)));
            let edge_colour = match edge_paint {
                Paint::Fixed(fixed) => fixed,
                Paint::Colormapped => Some(colour),
            };
            let edge_factor = alpha * edge_opacity;
            let stroke = edge_colour.filter(|_| edge_factor > 0.0).and_then(|c| {
                let dash = dash.clone()?;
                Some(paths::stroke(
                    c.with_alpha_factor(edge_factor as f32),
                    width,
                    dash,
                ))
            });
            let mut path = PathBuilder::new();
            path.rect(rect);
            if draw.push(0.0, paths::item(cells.id, path.finish(), fill, stroke)) {
                drawn += 1;
                draw.cells.push(CellHit {
                    axes: draw.axes,
                    artist: cells.id,
                    row: j,
                    column: i,
                    rect: seen,
                    column_edges: (data.columns[i], data.columns[i + 1]),
                    row_edges: (data.rows[j], data.rows[j + 1]),
                    bin: bin_of(cells, data, j, i),
                });
            }
        }
    }
    if drawn > 0 {
        *draw.dense.entry(cells.id).or_default() += drawn;
    }
}
