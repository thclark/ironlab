//! Colours, the automatic colour order, colour scales, dash patterns and marker shapes.
//!
//! Every artist maps its numbers to colours through one implementation here: [`ColourScales::resolve`] resolves each
//! colour scale of the figure once per compilation, and [`ResolvedScale::colour`] maps a value through it.

use std::collections::BTreeMap;
use std::sync::Arc;

use ironlab_ir::{
    Artist, Axes, Color, ColorScale, ColorScaleClass, ColorScaleDomain, ColorScaleKey,
    ColorScaleKind, ColorScaleLevels, ColorScaleNorm, ColorScaleSlot, ColorSpec, Colormap,
    ColormapName, DashStyle, Figure, LISTED_COLORMAP_MAX, LISTED_COLORMAP_MIN, Limits, MarkerShape,
    MarkerStyle, NodeId, OutOfRange, Series,
};

use crate::display::{Item, ItemKind, MarkerInstance, MarkersItem, PathSegment, Point, Rgba};
use crate::maths::colormap::{self, Lut};
use crate::maths::{contour, ticks};

use super::Ctx;
use super::paths::PathBuilder;

/// The automatic colour order: the Okabe–Ito palette without black, starting at orange.
pub(super) const COLOUR_ORDER: [[u8; 3]; 7] = [
    [0xE6, 0x9F, 0x00],
    [0x56, 0xB4, 0xE9],
    [0x00, 0x9E, 0x73],
    [0xF0, 0xE4, 0x42],
    [0x00, 0x72, 0xB2],
    [0xD5, 0x5E, 0x00],
    [0xCC, 0x79, 0xA7],
];

/// The colour of axes lines, tick marks and text.
pub(super) const INK: Rgba = Rgba::new(0.15, 0.15, 0.15, 1.0);

/// The colour of grid lines.
pub(super) const GRID: Rgba = Rgba::new(0.87, 0.87, 0.87, 1.0);

/// Converts an IR colour to a display colour, clamping each channel into `[0, 1]` and replacing NaN with 0.
pub(super) fn ir_colour(c: Color) -> Rgba {
    let channel = |v: f32| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
    Rgba::new(channel(c.r), channel(c.g), channel(c.b), channel(c.a))
}

/// Returns the lookup table of a bundled colormap.
pub(super) fn lut(name: ColormapName) -> &'static Lut {
    match name {
        ColormapName::Viridis => &colormap::VIRIDIS,
        ColormapName::Cividis => &colormap::CIVIDIS,
        ColormapName::Magma => &colormap::MAGMA,
        ColormapName::Inferno => &colormap::INFERNO,
        ColormapName::Plasma => &colormap::PLASMA,
        ColormapName::Coolwarm => &colormap::COOLWARM,
        ColormapName::Gray => &colormap::GRAY,
    }
}

/// Returns the middle colour of viridis, which a colormapped colour takes where it has no number to map: the colour
/// of a colormapped quiver, and a colormapped colour of a series without colour data.
pub(super) fn viridis_middle() -> Rgba {
    Rgba::from_u8(colormap::VIRIDIS[128])
}

/// The number of bands of a discrete scale whose automatic band edges cannot be used, and of a contour that takes
/// automatic levels.
const AUTOMATIC_BANDS: u32 = 10;

/// The colours a resolved colour scale maps to: a colormap sampled at a position from 0 to 1, or the entries of an
/// indexed or categorical scale.
enum Palette<'a> {
    /// A bundled colormap, whose 256 entries share the unit interval equally, run backwards when it is reversed.
    Named { lut: &'static Lut, reverse: bool },
    /// A listed colormap of at least two colours, interpolated linearly in sRGB.
    Listed(&'a [Color]),
    /// The entries of an indexed or categorical scale.
    Entries(Vec<Rgba>),
}

impl Palette<'_> {
    /// Returns the colour of a colormap at the position `t` from 0 to 1, or the entry `t` selects among the entries
    /// of an indexed or categorical scale, which only [`ResolvedScale::middle`] asks for.
    fn at(&self, t: f64) -> Option<Rgba> {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        match self {
            Palette::Named { lut, reverse } => {
                // The position bounds the product by 256, so the cast is exact after the floor.
                let index = ((t * 256.0).floor() as usize).min(255);
                let index = if *reverse { 255 - index } else { index };
                Some(Rgba::from_u8(lut[index]))
            }
            Palette::Listed(colors) => {
                let last = colors.len().checked_sub(1)?;
                if last == 0 {
                    return Some(ir_colour(colors[0]));
                }
                let x = t * last as f64;
                let i = (x.floor() as usize).min(last - 1);
                let f = (x - i as f64) as f32;
                let (a, b) = (ir_colour(colors[i]), ir_colour(colors[i + 1]));
                let mix = |p: f32, q: f32| p + (q - p) * f;
                Some(Rgba::new(
                    mix(a.r, b.r),
                    mix(a.g, b.g),
                    mix(a.b, b.b),
                    mix(a.a, b.a),
                ))
            }
            Palette::Entries(entries) => {
                let last = entries.len().checked_sub(1)?;
                entries
                    .get(((t * last as f64).round() as usize).min(last))
                    .copied()
            }
        }
    }
}

/// A colour scale of the figure resolved for one compilation: its domain, with automatic limits and automatic band
/// edges resolved to numbers, and the colours it maps to.
///
/// Every artist maps its numbers to colours through [`ResolvedScale::colour`], so that one rule colours them all: a
/// value inside the scale takes the colour of its slot, and any other value follows the policy of its category of
/// the scale (see [`ColorScale::category`]).
pub(crate) struct ResolvedScale<'a> {
    /// The colour scale of the figure.
    pub scale: &'a ColorScale,
    /// The domain that classifies values, with its limits and band edges resolved.
    pub domain: ColorScaleDomain,
    palette: Palette<'a>,
}

impl ResolvedScale<'_> {
    /// Returns whether the scale is indexed or categorical, so that it maps whole numbers to entries.
    pub fn is_index_mapped(&self) -> bool {
        matches!(self.domain, ColorScaleDomain::Entries { .. })
    }

    /// Returns the colour of a slot of the scale: a continuous position, a band of a discrete scale (band `i` of `k`
    /// takes the colormap colour at `i / (k − 1)`, and a single band the colour at the start), or an entry.
    pub fn slot_colour(&self, slot: ColorScaleSlot) -> Option<Rgba> {
        match slot {
            ColorScaleSlot::Position(t) => self.palette.at(t),
            ColorScaleSlot::Band(band) => {
                let bands = self.bands();
                let t = if bands > 1 {
                    band as f64 / (bands - 1) as f64
                } else {
                    0.0
                };
                self.palette.at(t)
            }
            ColorScaleSlot::Entry(entry) => match &self.palette {
                Palette::Entries(entries) => entries.get(entry).copied(),
                _ => None,
            },
        }
    }

    /// Returns the number of bands of a discrete scale, and 1 for any other kind.
    fn bands(&self) -> usize {
        match &self.domain {
            ColorScaleDomain::Discrete { edges, .. } => edges.len().saturating_sub(1).max(1),
            _ => 1,
        }
    }

    /// Returns the slot of the first colour of the scale, which a value below its range clamps to.
    fn first_slot(&self) -> ColorScaleSlot {
        match self.domain {
            ColorScaleDomain::Continuous { .. } => ColorScaleSlot::Position(0.0),
            ColorScaleDomain::Discrete { .. } => ColorScaleSlot::Band(0),
            ColorScaleDomain::Entries { .. } => ColorScaleSlot::Entry(0),
        }
    }

    /// Returns the slot of the last colour of the scale, which a value above its range clamps to.
    fn last_slot(&self) -> ColorScaleSlot {
        match self.domain {
            ColorScaleDomain::Continuous { .. } => ColorScaleSlot::Position(1.0),
            ColorScaleDomain::Discrete { .. } => ColorScaleSlot::Band(self.bands() - 1),
            ColorScaleDomain::Entries { count } => ColorScaleSlot::Entry(count.saturating_sub(1)),
        }
    }

    /// Returns the colour of `value`, or `None` when nothing is drawn for it.
    ///
    /// A value inside the scale takes the colour of its slot. A value the scale cannot place follows the policy of
    /// its category: nothing under a transparent policy, the fixed colour of a fixed-colour policy, and under a clamp
    /// the first colour below the range, the last colour above it, and, outside the entries of an indexed or
    /// categorical scale, the entry of the value truncated toward zero and clamped to the first or last entry; a
    /// non-finite value has no nearest end, so a clamp draws nothing for it. A strict policy draws nothing either: an
    /// artist whose own numbers a strict policy refuses is skipped before it is drawn (see
    /// [`ResolvedScale::strict_category`]), so a value that reaches a strict policy here is one derived from those
    /// numbers, such as the mean of the corners of a surface face.
    pub fn colour(&self, value: f64) -> Option<Rgba> {
        let class = self.domain.classify(value);
        let policy = match self.scale.category(class) {
            None => {
                return match class {
                    ColorScaleClass::Inside(slot) => self.slot_colour(slot),
                    _ => None,
                };
            }
            Some(category) => category.policy,
        };
        match policy {
            OutOfRange::Strict | OutOfRange::Transparent => None,
            OutOfRange::Rgba { color } => Some(ir_colour(color)),
            OutOfRange::Clamp => match class {
                ColorScaleClass::Below => self.slot_colour(self.first_slot()),
                ColorScaleClass::Above => self.slot_colour(self.last_slot()),
                ColorScaleClass::Outside => {
                    let ColorScaleDomain::Entries { count } = self.domain else {
                        return None;
                    };
                    let last = count.checked_sub(1)?;
                    // The value is finite, and the clamp bounds it before the cast, which saturates anyway.
                    let entry = value.trunc().clamp(0.0, last as f64) as usize;
                    self.slot_colour(ColorScaleSlot::Entry(entry))
                }
                ColorScaleClass::NonFinite | ColorScaleClass::Inside(_) => None,
            },
        }
    }

    /// Returns the colour that the key of an out-of-range category of the scale shows, which is the colour painted for
    /// a value of that category: the fixed colour of a fixed-colour policy, and under a clamp the first colour below
    /// the range and the last above it. Returns `None` when the key is left empty: under a transparent or strict
    /// policy, for a non-finite value, which has no nearest end, for a clamp outside the entries, which takes
    /// either end, and for a category the kind of the scale does not have.
    pub fn key_colour(&self, key: ColorScaleKey) -> Option<Rgba> {
        match (self.scale.key(key)?.policy, key) {
            (OutOfRange::Rgba { color }, _) => Some(ir_colour(color)),
            (OutOfRange::Clamp, ColorScaleKey::Below) => self.slot_colour(self.first_slot()),
            (OutOfRange::Clamp, ColorScaleKey::Above) => self.slot_colour(self.last_slot()),
            _ => None,
        }
    }

    /// Returns the name of the category of `value` when its policy is strict, so that an artist mapping the value is
    /// refused: `below`, `above`, `outside` or `non_finite`. Returns `None` for a value inside the scale or in a
    /// lenient category.
    pub fn strict_category(&self, value: f64) -> Option<(&'static str, &'static str)> {
        let class = self.domain.classify(value);
        let category = self.scale.category(class)?;
        if category.policy != OutOfRange::Strict {
            return None;
        }
        Some(match class {
            ColorScaleClass::Below => ("below", "lies below the range of the colour scale"),
            ColorScaleClass::Above => ("above", "lies above the range of the colour scale"),
            ColorScaleClass::Outside => ("outside", "names no entry of the colour scale"),
            ColorScaleClass::NonFinite | ColorScaleClass::Inside(_) => {
                ("non_finite", "is not finite")
            }
        })
    }

    /// Returns the levels of a contour that takes them from the scale: the band edges of a discrete scale, and, for
    /// an indexed or categorical scale of `n` entries, the half-integers 0.5, 1.5, …, n − 1.5 between entries, so
    /// that each band holds one whole number. Returns `None` for a continuous scale, whose contours take automatic
    /// levels.
    pub fn contour_levels(&self) -> Option<Vec<f64>> {
        match &self.domain {
            ColorScaleDomain::Discrete { edges, .. } => Some(edges.clone()),
            ColorScaleDomain::Entries { count } => {
                Some((1..*count).map(|entry| entry as f64 - 0.5).collect())
            }
            ColorScaleDomain::Continuous { .. } => None,
        }
    }
}

/// The colour scales of a figure, each resolved once per compilation.
pub(crate) struct ColourScales<'a> {
    scales: BTreeMap<NodeId, ResolvedScale<'a>>,
}

impl<'a> ColourScales<'a> {
    /// Resolves every colour scale of the figure: the automatic limits of a scale range over the numbers that every
    /// artist naming it maps, in every axes, hidden artists included (see [`Figure::automatic_color_limits`]).
    ///
    /// A scale that cannot be used as it stands is reported with a warning naming it and drawn with a usable
    /// replacement: manual limits that are not finite and increasing, or not positive under a logarithmic norm, are
    /// replaced by automatic limits; explicit band edges that are not at least two finite, strictly increasing
    /// values (positive under a logarithmic norm) by ten automatic bands; and a listed colormap of fewer than 2 or
    /// more than 1024 colours by viridis. Automatic limits over no value are `[0, 1]`, or `[1, 10]` under a
    /// logarithmic norm.
    pub fn resolve(ctx: &mut Ctx<'a>) -> Self {
        let figure: &'a Figure = ctx.figure;
        let scales = figure
            .color_scales
            .iter()
            .map(|scale| (scale.id, resolve_scale(ctx, figure, scale)))
            .collect();
        Self { scales }
    }

    /// Returns the resolved scale that a reference names, or why it names none: the reference is absent, or names a
    /// node that is not a colour scale of the figure.
    pub fn named(&self, reference: Option<NodeId>) -> Result<&ResolvedScale<'a>, String> {
        let id = reference
            .ok_or_else(|| "it maps numbers to colours but names no colour scale".to_owned())?;
        self.scales.get(&id).ok_or_else(|| {
            format!("it names {id} as its colour scale, which is not a colour scale of the figure")
        })
    }

    /// Returns the resolved colour scale of the given identifier, or `None` when the figure has no colour scale of
    /// that identifier.
    pub fn get(&self, id: NodeId) -> Option<&ResolvedScale<'a>> {
        self.scales.get(&id)
    }

    /// Returns the scale through which an artist colours what it colour-maps, or `None` when it names no usable
    /// scale or maps no number to a colour, by the rule of [`Artist::color_mapping`].
    pub fn of(&self, artist: &Artist) -> Option<&ResolvedScale<'a>> {
        artist
            .color_mapping()
            .and_then(|mapping| mapping.color_scale)
            .and_then(|id| self.get(id))
    }
}

/// Resolves one colour scale, warning against it where it cannot be used as it stands.
fn resolve_scale<'a>(ctx: &mut Ctx, figure: &Figure, scale: &'a ColorScale) -> ResolvedScale<'a> {
    let norm = scale.kind.norm();
    let mut warn = |message: String| ctx.warn(Some(scale.id), message);
    let automatic = || {
        figure
            .automatic_color_limits(scale.id)
            .unwrap_or(match norm {
                ColorScaleNorm::Linear => (0.0, 1.0),
                ColorScaleNorm::Log => (1.0, 10.0),
            })
    };
    let limits = |limits: Limits, warn: &mut dyn FnMut(String)| match limits {
        Limits::Manual { min, max } if limits_usable(min, max, norm) => (min, max),
        Limits::Manual { min, max } => {
            warn(format!(
                "The manual limits [{min}, {max}] of the colour scale are not usable{}, so automatic limits are \
                 used.",
                if norm == ColorScaleNorm::Log {
                    " under its logarithmic norm"
                } else {
                    ""
                }
            ));
            automatic()
        }
        Limits::Auto => automatic(),
    };
    let colormap = |colormap: &'a Colormap, warn: &mut dyn FnMut(String)| match colormap {
        Colormap::Named { name, reverse } => Palette::Named {
            lut: lut(*name),
            reverse: *reverse,
        },
        Colormap::Listed { colors }
            if (LISTED_COLORMAP_MIN..=LISTED_COLORMAP_MAX).contains(&colors.len()) =>
        {
            Palette::Listed(colors)
        }
        Colormap::Listed { colors } => {
            warn(format!(
                "The listed colormap of the colour scale holds {} colours, but it must hold from \
                 {LISTED_COLORMAP_MIN} to {LISTED_COLORMAP_MAX}, so viridis is used.",
                colors.len()
            ));
            Palette::Named {
                lut: &colormap::VIRIDIS,
                reverse: false,
            }
        }
    };
    let (domain, palette) = match &scale.kind {
        ColorScaleKind::Continuous(continuous) => {
            let (min, max) = limits(continuous.limits, &mut warn);
            (
                ColorScaleDomain::Continuous { min, max, norm },
                colormap(&continuous.colormap, &mut warn),
            )
        }
        ColorScaleKind::Discrete(discrete) => {
            let edges = match &discrete.levels {
                ColorScaleLevels::Auto { count, limits: l } => {
                    let (min, max) = limits(*l, &mut warn);
                    automatic_edges(min, max, (*count).max(1), norm)
                }
                ColorScaleLevels::Explicit { values } if edges_usable(values, norm) => {
                    values.clone()
                }
                ColorScaleLevels::Explicit { values } => {
                    warn(format!(
                        "The band edges {values:?} of the colour scale are not at least two finite, strictly \
                         increasing values{}, so {AUTOMATIC_BANDS} automatic bands are used.",
                        if norm == ColorScaleNorm::Log {
                            " that are positive under its logarithmic norm"
                        } else {
                            ""
                        }
                    ));
                    let (min, max) = automatic();
                    automatic_edges(min, max, AUTOMATIC_BANDS, norm)
                }
            };
            (
                ColorScaleDomain::Discrete { edges, norm },
                colormap(&discrete.colormap, &mut warn),
            )
        }
        ColorScaleKind::Indexed(indexed) => (
            ColorScaleDomain::Entries {
                count: indexed.colors.len(),
            },
            Palette::Entries(indexed.colors.iter().copied().map(ir_colour).collect()),
        ),
        ColorScaleKind::Categorical(categorical) => (
            ColorScaleDomain::Entries {
                count: categorical.categories.len(),
            },
            Palette::Entries(
                categorical
                    .categories
                    .iter()
                    .map(|category| ir_colour(category.color))
                    .collect(),
            ),
        ),
    };
    ResolvedScale {
        scale,
        domain,
        palette,
    }
}

/// Returns whether manual limits can be used: finite and strictly increasing, and positive under a logarithmic norm.
fn limits_usable(min: f64, max: f64, norm: ColorScaleNorm) -> bool {
    min.is_finite() && max.is_finite() && min < max && (norm == ColorScaleNorm::Linear || min > 0.0)
}

/// Returns whether explicit band edges can be used: at least two finite, strictly increasing values, positive under
/// a logarithmic norm.
fn edges_usable(edges: &[f64], norm: ColorScaleNorm) -> bool {
    edges.len() >= 2
        && edges.iter().all(|edge| edge.is_finite())
        && edges.windows(2).all(|pair| pair[0] < pair[1])
        && (norm == ColorScaleNorm::Linear || edges[0] > 0.0)
}

/// Returns the automatic band edges of a discrete scale from `min` to `max`: the two limits, which are the first and
/// last edges, and between them the nice values that split the range into about `count` bands, chosen in the space
/// of the norm. Equal limits give a single band of no width.
fn automatic_edges(min: f64, max: f64, count: u32, norm: ColorScaleNorm) -> Vec<f64> {
    // The limits are finite, so they are either equal or increasing.
    if min >= max {
        return vec![min, max];
    }
    let target = count as usize;
    let inner: Vec<f64> = match norm {
        ColorScaleNorm::Linear => contour::auto_levels(min, max, target),
        ColorScaleNorm::Log => ticks::log_ticks(min, max, target)
            .major
            .into_iter()
            .filter(|edge| *edge > min && *edge < max)
            .collect(),
    };
    std::iter::once(min)
        .chain(inner)
        .chain(std::iter::once(max))
        .collect()
}

/// The resolved primary colour of an artist.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Paint {
    /// A fixed colour, or no colour at all.
    Fixed(Option<Rgba>),
    /// A colour taken from a colour scale.
    Colormapped,
}

impl Paint {
    /// Returns the colour, taking `middle` for a colormapped paint, which has no number to map.
    pub fn single(self, middle: Rgba) -> Option<Rgba> {
        match self {
            Paint::Fixed(c) => c,
            Paint::Colormapped => Some(middle),
        }
    }
}

/// Resolves a colour specification whose `Auto` means `auto`.
pub(super) fn resolve(spec: ColorSpec, auto: Paint) -> Paint {
    match spec {
        ColorSpec::Auto => auto,
        ColorSpec::Rgba { color } => Paint::Fixed(Some(ir_colour(color))),
        ColorSpec::None => Paint::Fixed(None),
        ColorSpec::Colormapped => Paint::Colormapped,
    }
}

/// Returns whether a series takes a colour from the axes colour order although its line is colour-mapped: it does
/// when its colour data holds one value per segment and it draws markers with an automatic face or edge, because no
/// value belongs to a point and those markers are drawn in the colour the series would have had.
fn takes_order_for_markers(figure: &Figure, series: &Series) -> bool {
    let length = |id| figure.data_array(id).map(|data| data.array.len());
    let per_segment = series.color_data.is_some_and(
        |colours| matches!((length(colours), length(series.x)), (Some(v), Some(n)) if v + 1 == n),
    );
    per_segment
        && series.draws_markers()
        && [series.marker.face, series.marker.edge].contains(&ColorSpec::Auto)
}

/// Resolves the primary colour of every artist of an axes, assigning automatic colours in artist order.
///
/// The primary colour of a series is the colour of its line, which its automatic marker colours follow. A series
/// whose line is colour-mapped takes no colour from the order, unless its colour data holds one value per segment
/// and it draws automatic marker colours, which then take the next colour of the order as the series' own (see
/// [`takes_order_for_markers`]). Contour, surface and image artists have no primary colour and receive
/// `Paint::Colormapped`; none of them takes a colour from the colour order.
pub(super) fn primaries(figure: &Figure, axes: &Axes) -> Vec<Paint> {
    let mut next = 0usize;
    let mut auto = || {
        let colour = Rgba::from_u8(COLOUR_ORDER[next % COLOUR_ORDER.len()]);
        next += 1;
        Paint::Fixed(Some(colour))
    };
    axes.artists
        .iter()
        .map(|artist| {
            let spec = match artist {
                Artist::Series(series)
                    if series.line.color == ColorSpec::Colormapped
                        && takes_order_for_markers(figure, series) =>
                {
                    ColorSpec::Auto
                }
                Artist::Series(series) => series.line.color,
                Artist::Quiver(quiver) => quiver.line.color,
                // Bars, stairs and cells are not drawn yet, so they take no colour from the order.
                Artist::Contour(_)
                | Artist::Surface(_)
                | Artist::Image(_)
                | Artist::ScalarImage(_)
                | Artist::Bar(_)
                | Artist::Stairs(_)
                | Artist::Cells(_) => ColorSpec::Colormapped,
            };
            match spec {
                ColorSpec::Auto => auto(),
                other => resolve(other, Paint::Fixed(None)),
            }
        })
        .collect()
}

/// Returns the dash array for a dash style and line width, or `None` when no line is drawn.
pub(super) fn dash_array(style: DashStyle, width: f64) -> Option<Vec<f64>> {
    let u = width.max(0.5);
    match style {
        DashStyle::Solid => Some(Vec::new()),
        DashStyle::Dashed => Some(vec![5.0 * u, 3.0 * u]),
        DashStyle::Dotted => Some(vec![u, 2.0 * u]),
        DashStyle::DashDot => Some(vec![5.0 * u, 2.0 * u, u, 2.0 * u]),
        DashStyle::DashDotDot => Some(vec![5.0 * u, 2.0 * u, u, 2.0 * u, u, 2.0 * u]),
        DashStyle::LongDash => Some(vec![10.0 * u, 3.0 * u]),
        DashStyle::None => None,
    }
}

/// Returns `value` when it is finite and not negative, and `default` otherwise.
pub(super) fn width_or(value: f64, default: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        default
    }
}

/// The outline of a marker of width 1 centred on the origin, shared by every instance of the marker.
pub(super) struct MarkerOutline {
    pub segments: Arc<[PathSegment]>,
    /// Whether the outline encloses an area that a face colour fills.
    pub closed: bool,
    /// Whether the marker is a solid dot filled with its edge colour.
    pub dot: bool,
}

/// Builds the outline of a marker shape in unit space, or `None` for `MarkerShape::None`.
pub(super) fn marker_outline(shape: MarkerShape) -> Option<MarkerOutline> {
    let h = 0.5;
    let mut b = PathBuilder::new();
    let at = |dx: f64, dy: f64| Point::new(dx, dy);
    let (closed, dot) = match shape {
        MarkerShape::None => return None,
        MarkerShape::Circle => {
            b.circle(Point::new(0.0, 0.0), h);
            (true, false)
        }
        MarkerShape::Point => {
            b.circle(Point::new(0.0, 0.0), 1.0 / 6.0);
            (true, true)
        }
        MarkerShape::Square => {
            let s = 0.9 * h;
            b.polyline(&[at(-s, -s), at(s, -s), at(s, s), at(-s, s)], true);
            (true, false)
        }
        MarkerShape::Diamond => {
            let s = 1.15 * h;
            b.polyline(&[at(0.0, -s), at(s, 0.0), at(0.0, s), at(-s, 0.0)], true);
            (true, false)
        }
        MarkerShape::TriangleUp | MarkerShape::TriangleDown => {
            let r = 1.15 * h;
            let sign = if shape == MarkerShape::TriangleUp {
                1.0
            } else {
                -1.0
            };
            let half_base = r * 3f64.sqrt() / 2.0;
            b.polyline(
                &[
                    at(0.0, -sign * r),
                    at(half_base, sign * r / 2.0),
                    at(-half_base, sign * r / 2.0),
                ],
                true,
            );
            (true, false)
        }
        MarkerShape::Plus => {
            b.polyline(&[at(-h, 0.0), at(h, 0.0)], false);
            b.polyline(&[at(0.0, -h), at(0.0, h)], false);
            (false, false)
        }
        MarkerShape::VLine => {
            b.polyline(&[at(0.0, -h), at(0.0, h)], false);
            (false, false)
        }
        MarkerShape::HLine => {
            b.polyline(&[at(-h, 0.0), at(h, 0.0)], false);
            (false, false)
        }
        MarkerShape::Cross => {
            let s = h * std::f64::consts::FRAC_1_SQRT_2;
            b.polyline(&[at(-s, -s), at(s, s)], false);
            b.polyline(&[at(-s, s), at(s, -s)], false);
            (false, false)
        }
    };
    Some(MarkerOutline {
        segments: Arc::from(b.finish()),
        closed,
        dot,
    })
}

/// Where one marker goes: its centre, width, depth and the index of its data point.
#[derive(Clone, Copy)]
pub(super) struct MarkerPlace {
    pub position: Point,
    pub size: f64,
    pub depth: f64,
    pub source_index: usize,
}

/// Builds one instance of a marker of `outline` at `place`.
///
/// A face or edge set to `Auto` (or `Colormapped`) takes `auto`, the resolved colour of the artist or data point.
/// Every colour is multiplied by `alpha`. An open marker (plus, cross or a vertical or horizontal tick) is stroked with its edge colour, or with
/// its face colour when it has no edge colour, and a point marker is filled with its edge colour. Returns `None`
/// for an unusable size or a marker with neither a face nor an edge to draw.
pub(super) fn marker_instance(
    style: &MarkerStyle,
    outline: &MarkerOutline,
    place: MarkerPlace,
    auto: Option<Rgba>,
    alpha: f32,
) -> Option<MarkerInstance> {
    let resolve = |spec: ColorSpec| match spec {
        ColorSpec::Auto | ColorSpec::Colormapped => auto,
        ColorSpec::Rgba { color } => Some(ir_colour(color)),
        ColorSpec::None => None,
    };
    marker_instance_coloured(
        outline,
        place,
        resolve(style.face),
        resolve(style.edge),
        alpha,
    )
}

/// Builds one instance of a marker of `outline` at `place` whose face and edge colours are already resolved, each
/// multiplied by `alpha`, as [`marker_instance`] describes.
pub(super) fn marker_instance_coloured(
    outline: &MarkerOutline,
    place: MarkerPlace,
    face: Option<Rgba>,
    edge: Option<Rgba>,
    alpha: f32,
) -> Option<MarkerInstance> {
    let MarkerPlace {
        position,
        size,
        depth,
        source_index,
    } = place;
    if !(size.is_finite() && size > 0.0) {
        return None;
    }
    let fade = |c: Option<Rgba>| c.map(|c| c.with_alpha_factor(alpha));
    let (face, edge) = (fade(face), fade(edge));
    let (face, edge) = if outline.dot {
        (edge.or(face), None)
    } else if outline.closed {
        (face, edge)
    } else {
        (None, edge.or(face))
    };
    if face.is_none() && edge.is_none() {
        return None;
    }
    Some(MarkerInstance {
        position,
        depth,
        size_pt: size,
        face,
        edge,
        source_index,
    })
}

/// Wraps one marker instance in an item of its own, which [`super::artists::coalesce_markers`] merges with its
/// neighbours of the same artist and outline.
pub(super) fn marker_item(
    source: NodeId,
    outline: &MarkerOutline,
    edge_width: f64,
    instance: MarkerInstance,
) -> Item {
    Item {
        source: Some(source),
        kind: ItemKind::Markers(MarkersItem {
            outline: Arc::clone(&outline.segments),
            edge_width,
            instances: vec![instance],
        }),
    }
}

/// Returns an opacity held to `[0, 1]`, with a NaN, which validation refuses, taken as transparent as a non-finite
/// alpha is.
pub(super) fn unit_opacity(opacity: f64) -> f64 {
    if opacity.is_nan() {
        0.0
    } else {
        opacity.clamp(0.0, 1.0)
    }
}
