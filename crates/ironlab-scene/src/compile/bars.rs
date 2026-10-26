//! Bars and stairs: the layout of the bars of a group, and the drawing of bars and stairs in a two-dimensional axes.
//!
//! The bars of an axes are laid out once, before limits are computed, because the automatic limits cover the slots
//! and the stacked totals that the layout decides. The layout is in data units: each bar is given its slot along the
//! category axis, the share of the drawn part of the slot it takes, and its two ends along the value axis. The gap is
//! applied only when the bar is drawn, on the page, so that bars on a logarithmic category axis remain symmetric.

use std::collections::HashMap;

use ironlab_ir::{Artist, Bar, BarBase, BarLayout, BarSlot, ColorSpec, Stairs};

use crate::display::{Point, Rect, Rgba};
use crate::hit::{AxisMap, BarHit, StepHit};

use super::artists::{Draw, Space};
use super::data::{ArtistData, BarPiece, BarsData, Prepared, StairsGeometry};
use super::paths::{self, PathBuilder};
use super::rects::{CLAMP_MARGIN, clamped_rect, expand, faded, inset, intersection, unit_gap};
use super::style::{self, Paint};

/// Returns the key under which a position is matched with the positions of other members of a group: its bits,
/// with −0 taken as 0, so that positions are matched exactly by value.
fn key(position: f64) -> u64 {
    (position + 0.0).to_bits()
}

/// Returns the smallest distance between neighbouring distinct finite positions, taken in order of value, or 1 when
/// there are fewer than two distinct finite positions.
fn pitch<'p>(positions: impl Iterator<Item = &'p [f64]>) -> f64 {
    let mut finite: Vec<f64> = positions
        .flat_map(|p| p.iter().copied())
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

/// One member of a group of bars, as the layout reads it.
struct Member<'p> {
    /// The index of the member among the prepared artists of its axes.
    at: usize,
    bar: &'p Bar,
    /// For each position the member holds, the index of its first bar there.
    first: HashMap<u64, usize>,
}

/// Lays out every bar series of an axes, writing the bars each draws into its data.
///
/// Bar series whose groups share a name form one group, in artist order; a series without a group, or whose data
/// cannot be used, is laid out on its own. The orientation, slot, gap, base and layout of a group are those of its
/// first member, and every member is drawn by them, whatever its own. Along the category axis the slot of a bar
/// is centred on its position. Its width is the pitch, which for a group is taken over the positions of all its
/// members, a fixed width, or, for widths given per bar, the width of the first member, in artist order, that has a
/// bar at that position and gives widths per bar; a base given per bar is chosen in the same way.
///
/// In a grouped group every member takes an equal share of the drawn part of each slot, in artist order, whether or
/// not it has a bar there and whether or not it is visible, so that hiding a member, or a member missing a position,
/// leaves its share empty and moves no other bar. In a stacked group the visible members at each position build on
/// one another in artist order, positive values upwards from the base and negative values downwards from it, each
/// value being the length of its part; a hidden member contributes nothing, so that the parts above it close up and
/// the column shows the total of the parts that remain, and a part without a finite value contributes nothing
/// either. Outside a stack a bar runs from its base to its value.
pub(super) fn lay_out(prepared: &mut [Prepared]) {
    let mut groups: Vec<(Option<&str>, Vec<usize>)> = Vec::new();
    for (at, p) in prepared.iter().enumerate() {
        let (Artist::Bar(bar), Some(ArtistData::Bars(_))) = (p.artist, &p.data) else {
            continue;
        };
        let name = bar.group.as_ref().map(|group| group.name.as_str());
        match groups
            .iter_mut()
            .find(|(existing, _)| name.is_some() && *existing == name)
        {
            Some((_, members)) => members.push(at),
            None => groups.push((name, vec![at])),
        }
    }
    for (_, indices) in groups {
        let pieces = lay_out_group(prepared, &indices);
        let leader = match prepared[indices[0]].artist {
            Artist::Bar(bar) => bar,
            _ => unreachable!("a group holds bar series only"),
        };
        let category = super::data::dimension_index(leader.orientation.category_dimension());
        for (at, pieces) in indices.into_iter().zip(pieces) {
            if let Some(ArtistData::Bars(data)) = &mut prepared[at].data {
                data.pieces = pieces;
                data.category = category;
                data.gap = leader.gap;
            }
        }
    }
}

/// Returns the bars of each member of one group, in the order of `indices`.
fn lay_out_group(prepared: &[Prepared], indices: &[usize]) -> Vec<Vec<BarPiece>> {
    let arrays = |at: usize| match &prepared[at].data {
        Some(ArtistData::Bars(data)) => data.arrays,
        _ => unreachable!("a group holds bar series with usable data only"),
    };
    let members: Vec<Member> = indices
        .iter()
        .map(|&at| {
            let bar = match prepared[at].artist {
                Artist::Bar(bar) => bar,
                _ => unreachable!("a group holds bar series only"),
            };
            let mut first = HashMap::new();
            for (k, &p) in arrays(at).positions.iter().enumerate() {
                if p.is_finite() {
                    first.entry(key(p)).or_insert(k);
                }
            }
            Member { at, bar, first }
        })
        .collect();
    let leader = members[0].bar;
    let layout = leader
        .group
        .as_ref()
        .map_or(BarLayout::Grouped, |group| group.layout);
    let pitch = pitch(members.iter().map(|m| arrays(m.at).positions));
    // The width of the slot and the base at a position: the first member's kind of each, and, for one given per bar,
    // the value of the first member that has a bar at the position and gives one per bar.
    let per_bar = |p: f64, widths: bool| {
        members.iter().find_map(|m| {
            let k = *m.first.get(&key(p))?;
            let data = arrays(m.at);
            let values = if widths { data.widths } else { data.bases };
            values.map(|values| values[k])
        })
    };
    let width_at = |p: f64| match leader.slot {
        BarSlot::Pitch => pitch,
        BarSlot::Width(width) => width,
        BarSlot::Data(_) => per_bar(p, true).unwrap_or(pitch),
    };
    let base_at = |p: f64| match leader.base {
        BarBase::Constant(base) => base,
        BarBase::Data(_) => per_bar(p, false).unwrap_or(0.0),
    };
    let n = members.len() as f64;
    // The running top of the positive parts and bottom of the negative parts of each column of a stack.
    let mut columns: HashMap<u64, (f64, f64)> = HashMap::new();
    members
        .iter()
        .enumerate()
        .map(|(j, member)| {
            let data = arrays(member.at);
            let stacked = layout == BarLayout::Stacked;
            if stacked && !member.bar.visible {
                return Vec::new();
            }
            let mut pieces = Vec::new();
            for (index, (&position, &value)) in data.positions.iter().zip(data.values).enumerate() {
                if !(position.is_finite() && value.is_finite()) {
                    continue;
                }
                let width = width_at(position);
                let base = base_at(position);
                if !(width.is_finite() && width > 0.0 && base.is_finite()) {
                    continue;
                }
                let slot = [position - width / 2.0, position + width / 2.0];
                let (share, ends) = if stacked {
                    let (top, bottom) = columns.entry(key(position)).or_insert((base, base));
                    let ends = if value >= 0.0 {
                        let start = *top;
                        *top += value;
                        [start, *top]
                    } else {
                        let start = *bottom;
                        *bottom += value;
                        [start, *bottom]
                    };
                    ([0.0, 1.0], ends)
                } else {
                    let j = j as f64;
                    ([j / n, (j + 1.0) / n], [base, value])
                };
                pieces.push(BarPiece {
                    index,
                    position,
                    slot,
                    share,
                    base,
                    ends,
                    stacked,
                });
            }
            pieces
        })
        .collect()
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
/// The slot of a bar is mapped onto the page through the category axis, and the drawn part of it is the slot less
/// the fraction `gap` of its width on the page, half on each side, of which the bar takes its share. Its ends are
/// mapped through the value axis, an end that a logarithmic axis cannot place lying at the low edge of the plot.
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
    let gap = unit_gap(data.gap);
    let bounds = expand(draw.clip, CLAMP_MARGIN);
    let mut drawn: u64 = 0;
    for piece in &data.pieces {
        if category.log && piece.slot[0] <= 0.0 {
            continue;
        }
        let (low, high) = (
            category.to_figure(piece.slot[0]),
            category.to_figure(piece.slot[1]),
        );
        let (start, drawn_span) = inset(low, high, gap);
        let a = start + drawn_span * piece.share[0];
        let b = start + drawn_span * piece.share[1];
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
            let value = data.arrays.values[piece.index];
            draw.bars.push(BarHit {
                axes: draw.axes,
                artist: bar.id,
                index: piece.index,
                rect,
                position: piece.position,
                value,
                base: piece.base,
                segment: piece.stacked.then(|| {
                    let [s, e] = piece.ends;
                    (s.min(e), s.max(e))
                }),
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
