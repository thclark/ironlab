//! Bars and stairs: the geometry of the bars of a series, and the drawing of bars and stairs in a two-dimensional
//! axes.
//!
//! A bar series draws exactly the bars it describes (ADR 0026): each bar is centred along the category axis at its
//! position plus the offset of the series, is as wide as its width, and runs along the value axis from its base to
//! its base plus its value. Grouped and stacked bars are ordinary series whose widths, offsets and bases the facade
//! writes, so nothing here relates one series to another. The geometry is in data units and is computed once, when
//! the data of the series is resolved, because the automatic limits cover it.

use ironlab_ir::{Bar, BarOffset, BarWidth, ColorSpec, PerBar, Stairs};

use crate::display::{Point, Rect, Rgba};
use crate::hit::{AxisMap, BarHit, BinReading, StepHit};

use super::artists::{Draw, Space};
use super::data::{BarArrays, BarPiece, BarsData, StairsGeometry};
use super::paths::{self, PathBuilder};
use super::rects::{CLAMP_MARGIN, clamped_rect, expand, faded, intersection};
use super::style::{self, Paint};

/// Returns the smallest distance between neighbouring distinct finite positions, taken in order of value, or 1 when
/// there are fewer than two distinct finite positions.
fn pitch(positions: &[f64]) -> f64 {
    let mut finite: Vec<f64> = positions
        .iter()
        .copied()
        .filter(|p| p.is_finite())
        .collect();
    finite.sort_by(f64::total_cmp);
    finite.dedup();
    finite
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .fold(None, |least: Option<f64>, d| {
            Some(least.map_or(d, |l| l.min(d)))
        })
        .unwrap_or(1.0)
}

/// Resolves the bars of a series from its arrays: the pitch of its positions and, for every bar that can be drawn,
/// its extent along the category axis and its ends along the value axis.
///
/// The width of a bar is the fraction of the pitch, the fixed width, or the bar's own width; its offset is a multiple
/// of that width or a distance in data units; and it runs from its base, the constant base or its own, to its base
/// plus its value. A bar whose position, value, base, width or offset is not finite, or whose width is not positive,
/// is not drawn.
pub(super) fn bars_data<'a>(bar: &Bar, arrays: BarArrays<'a>) -> BarsData<'a> {
    let pitch = pitch(arrays.positions);
    let mut pieces = Vec::new();
    for (index, (&position, &value)) in arrays.positions.iter().zip(arrays.values).enumerate() {
        let width = match bar.width {
            BarWidth::Pitch(fraction) => fraction * pitch,
            BarWidth::Constant(width) => width,
            BarWidth::Data(_) => arrays.widths.map_or(f64::NAN, |widths| widths[index]),
        };
        let base = match bar.base {
            PerBar::Constant(base) => base,
            PerBar::Data(_) => arrays.bases.map_or(f64::NAN, |bases| bases[index]),
        };
        let centre = position
            + match bar.offset {
                BarOffset::Widths(widths) => widths * width,
                BarOffset::Absolute(offset) => offset,
            };
        let end = base + value;
        if !([position, value, base, width, centre, end]
            .iter()
            .all(|v| v.is_finite())
            && width > 0.0)
        {
            continue;
        }
        pieces.push(BarPiece {
            index,
            position,
            span: [centre - width / 2.0, centre + width / 2.0],
            base,
            ends: [base, end],
        });
    }
    BarsData {
        arrays,
        category: super::data::dimension_index(bar.orientation.category_dimension()),
        pitch,
        slotted: matches!(bar.width, BarWidth::Pitch(_)),
        pieces,
    }
}

/// Visits the values that a bar series places along `dim`, from which its automatic limits are found.
///
/// Along the category axis these are the drawn extent of every bar and, for bars whose width is a fraction of the
/// pitch, the slot one pitch wide centred on every finite position, so that the outer bars keep half a slot beside
/// them. Along the value axis they are the bases and ends of the bars, and, beyond each end that lies away from its
/// base, the fraction `margin` of the span of the series along the value axis (from the lowest to the highest of
/// its bases and ends), so that the longest bar does not touch the box while the side of the base takes no margin.
pub(super) fn visit_extent(bar: &Bar, data: &BarsData, dim: usize, visit: &mut dyn FnMut(f64)) {
    if dim == data.category {
        if data.slotted {
            for &position in data.arrays.positions {
                if position.is_finite() {
                    visit(position - data.pitch / 2.0);
                    visit(position + data.pitch / 2.0);
                }
            }
        }
        for piece in &data.pieces {
            piece.span.into_iter().for_each(&mut *visit);
        }
        return;
    }
    let (low, high) = data
        .pieces
        .iter()
        .flat_map(|piece| piece.ends)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        });
    let margin = if bar.margin.is_finite() && bar.margin > 0.0 && low < high {
        bar.margin * (high - low)
    } else {
        0.0
    };
    for piece in &data.pieces {
        let [base, end] = piece.ends;
        visit(base);
        visit(end);
        if end > base {
            visit(end + margin);
        } else if end < base {
            visit(end - margin);
        }
    }
}

/// Returns the bin among the resolved bin edges `edges` that holds `position`, as the binning counted samples: every
/// bin is half open except the last, which is closed; `None` when the position lies outside the edges.
fn bin_holding(edges: &[f64], position: f64) -> Option<usize> {
    let (first, last) = (*edges.first()?, *edges.last()?);
    if !(first..=last).contains(&position) {
        return None;
    }
    let bins = edges.len() - 1;
    Some(
        edges
            .partition_point(|&e| e <= position)
            .saturating_sub(1)
            .min(bins - 1),
    )
}

/// Returns the width of bin `bin` between the resolved bin edges `edges`, or `None` when there is no such bin.
fn bin_width(edges: &[f64], bin: usize) -> Option<f64> {
    Some(edges.get(bin + 1)? - edges.get(bin)?)
}

/// Returns the reading of the value `values[index]` of a histogram binned as `binning` records, drawn for a bin of
/// width `size`: the width of the bin between the record's edges, never the width drawn, which for a member of a
/// grouped histogram is only its share of the bin.
fn bin_of(
    binning: Option<&ironlab_ir::Binning>,
    values: &[f64],
    index: usize,
    size: Option<f64>,
) -> Option<BinReading> {
    let (binning, size) = (binning?, size?);
    let before = index
        .checked_sub(1)
        .and_then(|k| values.get(k).copied())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0);
    BinReading::recover(
        binning.normalization,
        binning.total,
        values[index],
        before,
        size,
    )
}

/// Returns the two axis maps of a two-dimensional space, the category axis first.
fn maps(space: &Space, category: usize) -> Option<(AxisMap, AxisMap)> {
    match *space {
        Space::TwoD { x, y } if category == 0 => Some((x, y)),
        Space::TwoD { x, y } => Some((y, x)),
        Space::ThreeD(_) => None,
    }
}

/// Maps a value onto the page along a value axis: a value that a logarithmic axis cannot place, such as a base of 0,
/// is placed at the low edge of the plot area, so that a bar or a step drawn from it still reads its other end.
fn value_on_page(map: &AxisMap, value: f64) -> f64 {
    if map.log && value <= 0.0 {
        map.to_figure(map.min)
    } else {
        map.to_figure(value)
    }
}

/// Returns the colour of a face or outline of a bar: `series` for an automatic colour, and for a colormapped one
/// without colour data or a usable scale; the colour of the bar's value through the scale for a colormapped one with
/// both.
fn bar_colour(spec: ColorSpec, series: Option<Rgba>, mapped: Option<Option<Rgba>>) -> Option<Rgba> {
    match style::resolve(spec, Paint::Fixed(series)) {
        Paint::Fixed(colour) => colour,
        Paint::Colormapped => mapped.unwrap_or(series),
    }
}

/// Returns the colour of the series of a bar series or of stairs from its resolved primary colour.
fn series_colour(primary: Paint) -> Option<Rgba> {
    primary.single(style::viridis_middle())
}

/// Draws every bar of a bar series as a closed rectangle, filled with its face and outlined by its edge, records
/// each in the hit map, and counts the bars drawn as dense content, as the faces of a surface are counted.
///
/// The extent of a bar along the category axis is mapped onto the page through the category axis, so that a bar on a
/// logarithmic category axis covers exactly its extent in data units; a bar whose low edge such an axis cannot place
/// is not drawn. Its ends are mapped through the value axis: a base that a logarithmic axis cannot place lies at the
/// low edge of the plot, and a bar whose end such an axis cannot place is not drawn.
pub(super) fn draw_bars(draw: &mut Draw, bar: &Bar, data: &BarsData, primary: Paint) {
    let Some((category, value)) = maps(&draw.space, data.category) else {
        return;
    };
    let series = series_colour(primary);
    let scale = bar
        .color_data
        .and(bar.color_scale)
        .and_then(|id| draw.scales.get(id));
    let width = style::width_or(bar.edge.width_pt, 0.5);
    let dash = style::dash_array(bar.edge.dash, width);
    let bounds = expand(draw.clip, CLAMP_MARGIN);
    let mut drawn: u64 = 0;
    for piece in &data.pieces {
        if category.log && piece.span[0] <= 0.0 {
            continue;
        }
        if value.log && piece.ends[1] <= 0.0 {
            continue;
        }
        let (a, b) = (
            category.to_figure(piece.span[0]),
            category.to_figure(piece.span[1]),
        );
        let (c, d) = (
            value_on_page(&value, piece.ends[0]),
            value_on_page(&value, piece.ends[1]),
        );
        if ![a, b, c, d].iter().all(|v| v.is_finite()) {
            continue;
        }
        let (x0, x1, y0, y1) = if data.category == 0 {
            (a, b, c, d)
        } else {
            (c, d, a, b)
        };
        let rect = clamped_rect(x0, x1, y0, y1, bounds);
        let mapped = data.arrays.colours.map(|colours| {
            scale.map_or(Some(style::viridis_middle()), |scale| {
                scale.colour(colours[piece.index])
            })
        });
        let fill = faded(bar_colour(bar.face, series, mapped), bar.face_opacity).map(paths::fill);
        let stroke = faded(bar_colour(bar.edge.color, series, mapped), bar.edge_opacity)
            .and_then(|colour| Some(paths::stroke(colour, width, dash.clone()?)));
        let mut path = PathBuilder::new();
        path.rect(rect);
        if draw.push(0.0, paths::item(bar.id, path.finish(), fill, stroke)) {
            drawn += 1;
            // Only the part of a bar inside the plot area is seen, so only that part is read.
            let Some(rect) = intersection(rect, draw.clip) else {
                continue;
            };
            let [start, end] = piece.ends;
            draw.bars.push(BarHit {
                axes: draw.axes,
                artist: bar.id,
                index: piece.index,
                rect,
                position: piece.position,
                value: data.arrays.values[piece.index],
                base: piece.base,
                segment: (start, end),
                // The bar's bin is the one that holds its position, which lies at the centre of the bin for every
                // member of a group, whatever share of the bin the member draws.
                bin: bin_of(
                    bar.binning.as_ref(),
                    data.arrays.values,
                    piece.index,
                    data.arrays
                        .bin_edges
                        .and_then(|edges| bin_width(edges, bin_holding(edges, piece.position)?)),
                ),
            });
        }
    }
    if drawn > 0 {
        *draw.dense.entry(bar.id).or_default() += drawn;
    }
}

/// Draws stairs: the stepped outline through their values, broken where a value is not finite, and, with a base,
/// dropped to the base at both ends of each unbroken piece and filled down to it when they have a fill. No line is
/// drawn along the base itself, as Matplotlib's `stairs` draws none along its baseline, but the fill closes along
/// it. Each step drawn whose level lies inside the plot area is recorded in the hit map, with the part of its run
/// inside the plot area, because only that part is seen.
pub(super) fn draw_stairs(draw: &mut Draw, stairs: &Stairs, data: &StairsGeometry, primary: Paint) {
    let Some((category, value)) = maps(&draw.space, data.category) else {
        return;
    };
    let series = series_colour(primary);
    let colour = |spec: ColorSpec| match style::resolve(spec, Paint::Fixed(series)) {
        Paint::Fixed(colour) => colour,
        // A colormapped line or fill is invalid, because stairs have no colour data; it is drawn as an automatic one.
        Paint::Colormapped => series,
    };
    let bounds = expand(draw.clip, CLAMP_MARGIN);
    let page = |along: f64, level: f64| {
        let (c, v) = (category.to_figure(along), value_on_page(&value, level));
        let (x, y) = if data.category == 0 { (c, v) } else { (v, c) };
        Point::new(
            x.clamp(bounds.x, bounds.right()),
            y.clamp(bounds.y, bounds.bottom()),
        )
    };
    let placeable = |k: usize| {
        let (e0, e1) = (data.edges[k], data.edges[k + 1]);
        data.values[k].is_finite() && (!category.log || (e0 > 0.0 && e1 > 0.0))
    };
    let base = stairs.base.filter(|b| b.is_finite());
    let mut outline = PathBuilder::new();
    let mut fill_region = PathBuilder::new();
    let mut k = 0;
    while k < data.values.len() {
        if !placeable(k) {
            k += 1;
            continue;
        }
        let mut points = Vec::new();
        if let Some(base) = base {
            points.push(page(data.edges[k], base));
        }
        while k < data.values.len() && placeable(k) {
            let (e0, e1, v) = (data.edges[k], data.edges[k + 1], data.values[k]);
            let (from, to) = (page(e0, v), page(e1, v));
            points.push(from);
            points.push(to);
            if let Some((from, to)) = run_inside(from, to, draw.clip) {
                draw.steps.push(StepHit {
                    axes: draw.axes,
                    artist: stairs.id,
                    index: k,
                    edges: (e0, e1),
                    value: v,
                    from,
                    to,
                    bin: bin_of(
                        stairs.binning.as_ref(),
                        data.values,
                        k,
                        data.bin_edges.and_then(|edges| bin_width(edges, k)),
                    ),
                });
            }
            k += 1;
        }
        if let Some(base) = base {
            points.push(page(data.edges[k], base));
            fill_region.polyline(&points, true);
        }
        outline.polyline(&points, false);
    }
    if base.is_some() {
        let fill = faded(colour(stairs.fill), stairs.fill_opacity).map(paths::fill);
        draw.push(
            0.0,
            paths::item(stairs.id, fill_region.finish(), fill, None),
        );
    }
    let width = style::width_or(stairs.line.width_pt, 0.75);
    let stroke = faded(colour(stairs.line.color), stairs.line_opacity).and_then(|colour| {
        style::dash_array(stairs.line.dash, width).map(|dash| paths::stroke(colour, width, dash))
    });
    draw.push(0.0, paths::item(stairs.id, outline.finish(), None, stroke));
}

/// Returns the part inside `clip` of the run of a step from `from` to `to`, which is horizontal or vertical, or
/// `None` when no part of it lies inside.
fn run_inside(from: Point, to: Point, clip: Rect) -> Option<(Point, Point)> {
    let inside = |v: f64, low: f64, high: f64| v >= low && v <= high;
    let clamp = |p: Point| {
        Point::new(
            p.x.clamp(clip.x, clip.right()),
            p.y.clamp(clip.y, clip.bottom()),
        )
    };
    let (lo_x, hi_x) = (from.x.min(to.x), from.x.max(to.x));
    let (lo_y, hi_y) = (from.y.min(to.y), from.y.max(to.y));
    let meets = hi_x >= clip.x && lo_x <= clip.right() && hi_y >= clip.y && lo_y <= clip.bottom();
    let level_inside = if from.y == to.y {
        inside(from.y, clip.y, clip.bottom())
    } else {
        inside(from.x, clip.x, clip.right())
    };
    (meets && level_inside).then(|| (clamp(from), clamp(to)))
}
