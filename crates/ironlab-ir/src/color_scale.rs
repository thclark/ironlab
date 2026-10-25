//! Colour scales: figure nodes that turn the numbers of colour-mapped artists into
//! colours, and the classification of a value against a scale that validation and the
//! scene compiler share.
//!
//! A colour scale is a node of the figure, in [`Figure::color_scales`](crate::Figure),
//! that artists name by its identifier. The axes hold no colormap or limits, and nothing
//! is inherited: every artist that maps values to colours names its scale explicitly,
//! and any artist that takes a scale accepts a scale of any kind. A scale's kind fixes
//! both how values become colours and how the scale is drawn:
//!
//! - a [continuous](ContinuousColorScale) scale interpolates a colormap between its
//!   limits and is drawn as a smooth bar;
//! - a [discrete](DiscreteColorScale) scale gives every value the single colour of the
//!   band it falls in and is drawn as a bar of bands;
//! - an [indexed](IndexedColorScale) scale is a palette, of which a whole number `i`
//!   takes entry `i`, drawn as a bar of adjoining blocks;
//! - a [categorical](CategoricalColorScale) scale maps a whole number `i` to category
//!   `i`, drawn as separate outlined squares, each with its label.
//!
//! Values that a scale cannot place fall into categories, each with a policy of its own
//! ([`ColorScaleOutOfRange`]): `below` and `above` the range of a continuous or discrete
//! scale, `outside` the entries of an indexed or categorical scale, and `non_finite` on
//! every kind. [`ColorScaleDomain::classify`] decides which category a value falls into,
//! and [`ColorScale::category`] returns the category's policy, so that validation and
//! drawing apply the same rule.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artist::Artist;
use crate::axes::{ColormapName, Limits, TickLabels, Ticks};
use crate::data::Values;
use crate::figure::Figure;
use crate::ids::NodeId;
use crate::style::{Color, ColorSpec};
use crate::text::Text;

/// A colour scale: a node of the figure that turns numbers into colours, which
/// colour-mapped artists name by its identifier.
///
/// In JSON the kind is an object whose `type` names the kind and whose `value` holds it,
/// such as `{"type": "continuous", "value": {"colormap": ..., "limits": ...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColorScale {
    /// The node identifier of the scale, unique within the figure.
    pub id: NodeId,
    /// The title drawn with the scale: above a colourbar that shows it, and as the label
    /// of a bar item that shows it in a legend.
    pub title: Option<Text>,
    /// How values become colours and how the scale is drawn.
    pub kind: ColorScaleKind,
    /// What is drawn for a value that is NaN or infinite, on every kind of scale. The
    /// policy is transparent by default, because a non-finite value is the author's way
    /// of saying that nothing is drawn there.
    pub non_finite: ColorScaleOutOfRange,
}

impl Default for ColorScale {
    /// A continuous scale with the defaults of its kind, no title, and non-finite values
    /// drawn as nothing without a key.
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            title: None,
            kind: ColorScaleKind::default(),
            non_finite: ColorScaleOutOfRange::new(OutOfRange::Transparent),
        }
    }
}

/// One category of values that a colour scale cannot place within its range: the values
/// below or above the range of a continuous or discrete scale, the values outside the
/// entries of an indexed or categorical scale, or the values that are not finite.
///
/// The type has no single default, because the default policy differs by category:
/// clamping below and above, strict outside the entries, and transparent for non-finite
/// values. [`ColorScaleOutOfRange::new`] builds a category of a given policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColorScaleOutOfRange {
    /// What is drawn for a value of the category.
    pub policy: OutOfRange,
    /// Whether a colourbar that shows the scale draws a key square for the category,
    /// separate from the bar. Keys are off by default.
    pub show_key: bool,
    /// The label of the key square, or `None` for the default label: `> upper` above
    /// the range, `< lower` below it, `NaN / Inf` for non-finite values and
    /// `Out of range` outside the entries, with the limits formatted as the ticks are.
    pub label: Option<Text>,
}

impl ColorScaleOutOfRange {
    /// Returns a category of the given policy, without a key and with the default label.
    #[must_use]
    pub fn new(policy: OutOfRange) -> Self {
        Self {
            policy,
            show_key: false,
            label: None,
        }
    }
}

/// What is drawn for a value that a colour scale cannot place within its range: a value
/// below or above the range, outside the entries, or not finite.
///
/// Each category of such values holds a policy of its own (see [`ColorScaleOutOfRange`]),
/// so that, for example, non-finite values may be tolerated while values outside the
/// range are refused, or the reverse.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutOfRange {
    /// Such a value is a validation error, so the figure is refused until the data is
    /// corrected.
    Strict,
    /// Nothing is drawn for the value.
    #[default]
    Transparent,
    /// The value takes the nearest end colour of the scale: its first colour below the
    /// range and its last colour above it. An index outside the entries of an indexed or
    /// categorical scale is truncated toward zero and then clamped to the first or last
    /// entry. A non-finite value has no nearest end, so at the `non_finite` category a
    /// clamp draws nothing.
    Clamp,
    /// The value is drawn in a fixed colour.
    Rgba {
        /// The colour in which the value is drawn.
        color: Color,
    },
}

impl From<Color> for OutOfRange {
    /// A colour is the fixed-colour policy: such a value is drawn in that colour.
    fn from(color: Color) -> Self {
        OutOfRange::Rgba { color }
    }
}

/// How a colour scale turns values into colours, which also fixes how it is drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ColorScaleKind {
    /// A colormap interpolated between limits, drawn as a smooth bar.
    Continuous(ContinuousColorScale),
    /// Bands between edges, each in one colour of a colormap, drawn as a bar of bands.
    Discrete(DiscreteColorScale),
    /// A palette of which a whole number `i` takes entry `i`, drawn as a bar of adjoining
    /// blocks.
    Indexed(IndexedColorScale),
    /// Labelled categories of which a whole number `i` takes category `i`, drawn as
    /// separate outlined squares.
    Categorical(CategoricalColorScale),
}

impl Default for ColorScaleKind {
    /// A continuous scale with the defaults of its kind.
    fn default() -> Self {
        ColorScaleKind::Continuous(ContinuousColorScale::default())
    }
}

/// A continuous colour scale: a colormap interpolated between two limits, in linear or
/// logarithmic space.
///
/// A value exactly at a limit lies inside the range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContinuousColorScale {
    /// The palette that the scale interpolates.
    pub colormap: Colormap,
    /// The values mapped to the first and last colours of the colormap. Automatic limits
    /// are the exact range of the finite values (the positive finite values under a
    /// logarithmic norm) of every artist that maps values through the scale, in every
    /// axes, hidden artists included. Manual limits must be finite and strictly
    /// increasing, and positive under a logarithmic norm.
    pub limits: Limits,
    /// Whether values are interpolated in linear or logarithmic space.
    pub norm: ColorScaleNorm,
    /// What is drawn for a value below the lower limit; clamped by default. Under a
    /// logarithmic norm a value that is not positive lies below the range.
    pub below: ColorScaleOutOfRange,
    /// What is drawn for a value above the upper limit; clamped by default.
    pub above: ColorScaleOutOfRange,
    /// Where the ticks of a colourbar that shows the scale are placed.
    pub ticks: Ticks,
    /// What is written at the ticks of a colourbar that shows the scale.
    pub tick_labels: TickLabels,
}

impl Default for ContinuousColorScale {
    /// Viridis between automatic limits in linear space, clamping values beyond them,
    /// with automatic ticks and tick labels.
    fn default() -> Self {
        Self {
            colormap: Colormap::default(),
            limits: Limits::Auto,
            norm: ColorScaleNorm::Linear,
            below: ColorScaleOutOfRange::new(OutOfRange::Clamp),
            above: ColorScaleOutOfRange::new(OutOfRange::Clamp),
            ticks: Ticks::Auto,
            tick_labels: TickLabels::Auto,
        }
    }
}

/// A discrete colour scale: bands between edges, each drawn in one colour of a colormap.
///
/// With k bands, band i takes the colour of the colormap at i / (k − 1), so both ends of
/// the colormap are used; a single band takes the colour at the start. A value exactly on
/// an edge between two bands belongs to the upper band, and a value exactly at the first
/// or last edge lies inside the range. Every artist that names the scale is banded, a
/// smooth surface included, so that the bar that shows the scale is truthful.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DiscreteColorScale {
    /// The palette from which the bands take their colours.
    pub colormap: Colormap,
    /// The edges of the bands.
    pub levels: ColorScaleLevels,
    /// Whether band edges are chosen, and the colormap sampled, in linear or logarithmic
    /// space.
    pub norm: ColorScaleNorm,
    /// What is drawn for a value below the first edge; clamped by default.
    pub below: ColorScaleOutOfRange,
    /// What is drawn for a value above the last edge; clamped by default.
    pub above: ColorScaleOutOfRange,
    /// Where the ticks of a colourbar that shows the scale are placed; automatic ticks
    /// are one per band edge.
    pub ticks: Ticks,
    /// What is written at the ticks of a colourbar that shows the scale.
    pub tick_labels: TickLabels,
}

impl Default for DiscreteColorScale {
    /// Viridis in ten automatic bands between automatic limits in linear space, clamping
    /// values beyond them, with automatic ticks and tick labels.
    fn default() -> Self {
        Self {
            colormap: Colormap::default(),
            levels: ColorScaleLevels::default(),
            norm: ColorScaleNorm::Linear,
            below: ColorScaleOutOfRange::new(OutOfRange::Clamp),
            above: ColorScaleOutOfRange::new(OutOfRange::Clamp),
            ticks: Ticks::Auto,
            tick_labels: TickLabels::Auto,
        }
    }
}

/// The edges of the bands of a discrete colour scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ColorScaleLevels {
    /// Edges at nice values from the lower limit to the upper limit, which are the first
    /// and last edges.
    Auto {
        /// The approximate number of bands; at least one.
        count: u32,
        /// The range that the edges span. Automatic limits are the range of the values
        /// mapped through the scale, as for a continuous scale.
        limits: Limits,
    },
    /// Edges given explicitly.
    Explicit {
        /// The edges: at least two, finite, strictly increasing, and positive under a
        /// logarithmic norm.
        values: Vec<f64>,
    },
}

impl Default for ColorScaleLevels {
    /// Ten bands between automatic limits.
    fn default() -> Self {
        ColorScaleLevels::Auto {
            count: 10,
            limits: Limits::Auto,
        }
    }
}

/// An indexed colour scale: a palette of which a whole number `i` from 0 to the number of
/// entries less one takes entry `i`, such as the 256-entry palette of an indexed image.
///
/// Any other finite value, a fractional one included, lies outside the entries and
/// follows `outside`. An array of 8-bit values holds whole numbers only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndexedColorScale {
    /// The entries of the palette; at least one.
    pub colors: Vec<Color>,
    /// What is drawn for a value that names no entry; strict by default, so that such a
    /// value is a validation error.
    pub outside: ColorScaleOutOfRange,
    /// Where the ticks of a colourbar that shows the scale are placed, at the centres of
    /// the entries; automatic ticks fall on whole numbers only.
    pub ticks: Ticks,
    /// What is written at the ticks of a colourbar that shows the scale.
    pub tick_labels: TickLabels,
}

impl Default for IndexedColorScale {
    /// No entries, strict outside them, with automatic ticks and tick labels.
    fn default() -> Self {
        Self {
            colors: Vec::new(),
            outside: ColorScaleOutOfRange::new(OutOfRange::Strict),
            ticks: Ticks::Auto,
            tick_labels: TickLabels::Auto,
        }
    }
}

/// A categorical colour scale: labelled categories of which a whole number `i` from 0 to
/// the number of categories less one takes category `i`, drawn as separate outlined
/// squares, each with its label.
///
/// Any other finite value, a fractional one included, lies outside the categories and
/// follows `outside`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CategoricalColorScale {
    /// The categories; at least one.
    pub categories: Vec<ColorCategory>,
    /// What is drawn for a value that names no category; strict by default, so that such
    /// a value is a validation error.
    pub outside: ColorScaleOutOfRange,
}

impl Default for CategoricalColorScale {
    /// No categories, strict outside them.
    fn default() -> Self {
        Self {
            categories: Vec::new(),
            outside: ColorScaleOutOfRange::new(OutOfRange::Strict),
        }
    }
}

/// One category of a categorical colour scale: its colour and its label, which every key
/// that shows the scale draws beside the colour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColorCategory {
    /// The colour of the category.
    pub color: Color,
    /// The label drawn beside the colour; required, so that every category can be told
    /// apart in a key.
    pub label: Text,
}

/// The palette that a continuous or discrete colour scale interpolates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Colormap {
    /// A bundled colormap.
    Named {
        /// The name of the colormap.
        name: ColormapName,
        /// Whether the colormap runs from its last colour to its first.
        reverse: bool,
    },
    /// A list of colours, interpolated linearly in sRGB from the first, at the start of
    /// the scale, to the last, at its end.
    Listed {
        /// The colours: from 2 to 1024.
        colors: Vec<Color>,
    },
}

impl Default for Colormap {
    /// Viridis, not reversed.
    fn default() -> Self {
        Colormap::Named {
            name: ColormapName::Viridis,
            reverse: false,
        }
    }
}

impl From<ColormapName> for Colormap {
    /// A name is the bundled colormap of that name, not reversed.
    fn from(name: ColormapName) -> Self {
        Colormap::Named {
            name,
            reverse: false,
        }
    }
}

/// The space in which a continuous or discrete colour scale places values between its
/// limits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorScaleNorm {
    /// Positions are proportional to values.
    #[default]
    Linear,
    /// Positions are proportional to the base-10 logarithm of values; a value that is
    /// not positive lies below the range.
    Log,
}

/// The fewest colours of a listed colormap, which interpolation between colours needs.
pub const LISTED_COLORMAP_MIN: usize = 2;

/// The most colours of a listed colormap.
pub const LISTED_COLORMAP_MAX: usize = 1024;

impl ColorScaleNorm {
    /// Returns the automatic limits of a scale of this norm over the given values: the
    /// least and greatest finite values, or, under a logarithmic norm, the least and
    /// greatest positive finite values. Returns `None` when there is no such value.
    ///
    /// The limits may be equal, when every such value is the same.
    pub fn automatic_limits(self, values: impl IntoIterator<Item = f64>) -> Option<(f64, f64)> {
        values
            .into_iter()
            .filter(|v| v.is_finite() && (self == ColorScaleNorm::Linear || *v > 0.0))
            .fold(None, |range, v| match range {
                None => Some((v, v)),
                Some((min, max)) => Some((f64::min(min, v), f64::max(max, v))),
            })
    }
}

impl ColorScaleKind {
    /// Returns the norm of the scale; an indexed or categorical scale, which has none,
    /// places its entries linearly.
    #[must_use]
    pub fn norm(&self) -> ColorScaleNorm {
        match self {
            ColorScaleKind::Continuous(scale) => scale.norm,
            ColorScaleKind::Discrete(scale) => scale.norm,
            ColorScaleKind::Indexed(_) | ColorScaleKind::Categorical(_) => ColorScaleNorm::Linear,
        }
    }

    /// Returns the number of entries of an indexed or categorical scale, or `None` for a
    /// continuous or discrete scale.
    #[must_use]
    pub fn entry_count(&self) -> Option<usize> {
        match self {
            ColorScaleKind::Indexed(scale) => Some(scale.colors.len()),
            ColorScaleKind::Categorical(scale) => Some(scale.categories.len()),
            ColorScaleKind::Continuous(_) | ColorScaleKind::Discrete(_) => None,
        }
    }

    /// Returns the limits of a continuous scale, or the limits of the automatic band
    /// edges of a discrete scale, or `None` for a scale whose range is not set by limits
    /// (a discrete scale with explicit edges, and an indexed or categorical scale).
    #[must_use]
    pub fn limits(&self) -> Option<Limits> {
        match self {
            ColorScaleKind::Continuous(scale) => Some(scale.limits),
            ColorScaleKind::Discrete(DiscreteColorScale {
                levels: ColorScaleLevels::Auto { limits, .. },
                ..
            }) => Some(*limits),
            ColorScaleKind::Discrete(_)
            | ColorScaleKind::Indexed(_)
            | ColorScaleKind::Categorical(_) => None,
        }
    }
}

impl ColorScale {
    /// Returns whether the range of the scale depends on the values mapped through it,
    /// which it does when its limits, or the limits of its automatic band edges, are
    /// automatic.
    #[must_use]
    pub fn has_automatic_limits(&self) -> bool {
        self.kind.limits() == Some(Limits::Auto)
    }

    /// Returns the range of the scale, from the first value inside it to the last, with
    /// automatic limits taken as `automatic` (as [`Figure::automatic_color_limits`]
    /// returns them); `None` for an indexed or categorical scale, whose range is its
    /// entries, and for a scale with automatic limits when `automatic` is `None`.
    ///
    /// The range of a continuous scale is its limits, and that of a discrete scale is
    /// the limits of its automatic band edges or its first and last explicit edges.
    ///
    /// [`Figure::automatic_color_limits`]: crate::Figure::automatic_color_limits
    #[must_use]
    pub fn range(&self, automatic: Option<(f64, f64)>) -> Option<(f64, f64)> {
        match &self.kind {
            ColorScaleKind::Discrete(DiscreteColorScale {
                levels: ColorScaleLevels::Explicit { values },
                ..
            }) => Some((*values.first()?, *values.last()?)),
            kind => match kind.limits()? {
                Limits::Auto => automatic,
                Limits::Manual { min, max } => Some((min, max)),
            },
        }
    }

    /// Returns the domain of the scale where it does not depend on the values mapped
    /// through it: the limits of a continuous scale with manual limits, the explicit
    /// edges of a discrete scale, and the entries of an indexed or categorical scale.
    /// Returns `None` for a scale with automatic limits, and for a discrete scale with
    /// automatic edges, whose edges the scene compiler chooses.
    #[must_use]
    pub fn fixed_domain(&self) -> Option<ColorScaleDomain> {
        match &self.kind {
            ColorScaleKind::Continuous(ContinuousColorScale {
                limits: Limits::Manual { min, max },
                norm,
                ..
            }) => Some(ColorScaleDomain::Continuous {
                min: *min,
                max: *max,
                norm: *norm,
            }),
            ColorScaleKind::Discrete(DiscreteColorScale {
                levels: ColorScaleLevels::Explicit { values },
                norm,
                ..
            }) => Some(ColorScaleDomain::Discrete {
                edges: values.clone(),
                norm: *norm,
            }),
            ColorScaleKind::Indexed(_) | ColorScaleKind::Categorical(_) => {
                Some(ColorScaleDomain::Entries {
                    count: self.kind.entry_count().unwrap_or_default(),
                })
            }
            ColorScaleKind::Continuous(_) | ColorScaleKind::Discrete(_) => None,
        }
    }

    /// Returns the out-of-range category whose policy applies to a value of the given
    /// class, or `None` for a value inside the scale.
    ///
    /// A non-finite value takes `non_finite` on every kind. A value below or above takes
    /// `below` or `above` on a continuous or discrete scale, and a value outside the
    /// entries takes `outside` on an indexed or categorical scale. A class that the kind
    /// never produces (below or above on an index-mapped scale, outside on a continuous or
    /// discrete one) takes the nearest equivalent, so that every class has a category.
    #[must_use]
    pub fn category(&self, class: ColorScaleClass) -> Option<&ColorScaleOutOfRange> {
        let (below, above) = match &self.kind {
            ColorScaleKind::Continuous(scale) => (&scale.below, &scale.above),
            ColorScaleKind::Discrete(scale) => (&scale.below, &scale.above),
            ColorScaleKind::Indexed(scale) => (&scale.outside, &scale.outside),
            ColorScaleKind::Categorical(scale) => (&scale.outside, &scale.outside),
        };
        match class {
            ColorScaleClass::Inside(_) => None,
            ColorScaleClass::NonFinite => Some(&self.non_finite),
            ColorScaleClass::Below | ColorScaleClass::Outside => Some(below),
            ColorScaleClass::Above => Some(above),
        }
    }

    /// Returns the out-of-range category that a key of the given kind shows, or `None`
    /// when the kind of the scale has no such category: below or above on an indexed or
    /// categorical scale, and outside on a continuous or discrete one.
    ///
    /// Unlike [`ColorScale::category`], which gives every class of value a category, this
    /// never stands one category in for another, so that a colourbar draws each key of
    /// its scale once.
    #[must_use]
    pub fn key(&self, key: ColorScaleKey) -> Option<&ColorScaleOutOfRange> {
        match (&self.kind, key) {
            (_, ColorScaleKey::NonFinite) => Some(&self.non_finite),
            (ColorScaleKind::Continuous(scale), ColorScaleKey::Below) => Some(&scale.below),
            (ColorScaleKind::Continuous(scale), ColorScaleKey::Above) => Some(&scale.above),
            (ColorScaleKind::Discrete(scale), ColorScaleKey::Below) => Some(&scale.below),
            (ColorScaleKind::Discrete(scale), ColorScaleKey::Above) => Some(&scale.above),
            (ColorScaleKind::Indexed(scale), ColorScaleKey::Outside) => Some(&scale.outside),
            (ColorScaleKind::Categorical(scale), ColorScaleKey::Outside) => Some(&scale.outside),
            _ => None,
        }
    }
}

/// The out-of-range categories of a colour scale that a colourbar may show as key squares,
/// separate from the bar; [`ColorScale::key`] returns the category behind each.
///
/// The type is not part of the data model and is never serialised: it names a category in
/// the scene compiler, the facade and the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorScaleKey {
    /// The values below the range of a continuous or discrete scale.
    Below,
    /// The values above the range of a continuous or discrete scale.
    Above,
    /// The values that name no entry of an indexed or categorical scale.
    Outside,
    /// The values that are NaN or infinite, on every kind of scale.
    NonFinite,
}

/// The domain of a colour scale with its limits and band edges resolved to numbers, which
/// classifies values.
///
/// [`ColorScale::fixed_domain`] returns the domain of a scale that does not depend on
/// data. The scene compiler builds the domain of a scale with automatic limits or
/// automatic band edges from the values mapped through it.
#[derive(Debug, Clone, PartialEq)]
pub enum ColorScaleDomain {
    /// A continuous range from `min` to `max`, both inside it.
    Continuous {
        /// The lower limit.
        min: f64,
        /// The upper limit.
        max: f64,
        /// The space in which values are placed between the limits.
        norm: ColorScaleNorm,
    },
    /// Bands between strictly increasing edges.
    Discrete {
        /// The edges of the bands, of which there is one fewer than edges.
        edges: Vec<f64>,
        /// The space in which the edges lie.
        norm: ColorScaleNorm,
    },
    /// The entries 0 to `count − 1` of an indexed or categorical scale.
    Entries {
        /// The number of entries.
        count: usize,
    },
}

/// Where a value falls on a colour scale, which decides how it is coloured: inside the
/// scale, at a slot of it, or in one of the categories of values that the scale cannot
/// place, each of which follows the policy that [`ColorScale::category`] returns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorScaleClass {
    /// The value lies inside the scale, at the given slot.
    Inside(ColorScaleSlot),
    /// The value lies below the range of a continuous or discrete scale: less than its
    /// lower limit or first edge, or, under a logarithmic norm, not positive.
    Below,
    /// The value lies above the range of a continuous or discrete scale: greater than its
    /// upper limit or last edge.
    Above,
    /// The value names no entry of an indexed or categorical scale: it is negative, not
    /// less than the number of entries, or not a whole number.
    Outside,
    /// The value is NaN or infinite.
    NonFinite,
}

/// Where a value inside a colour scale lies, from which its colour follows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorScaleSlot {
    /// The position along a continuous scale, from 0 at the lower limit to 1 at the
    /// upper limit, measured in the space of the norm. A scale whose limits are equal,
    /// or do not span a finite positive distance in that space, places every value
    /// inside it at the middle, 0.5.
    Position(f64),
    /// The band of a discrete scale, counted from 0 at the first edge.
    Band(usize),
    /// The entry of an indexed or categorical scale.
    Entry(usize),
}

impl ColorScaleDomain {
    /// Classifies a value against the domain.
    ///
    /// A non-finite value is [non-finite](ColorScaleClass::NonFinite) on every domain.
    /// On a continuous or discrete domain, a value that is not positive under a
    /// logarithmic norm lies below the range, as does a value less than the lower limit or
    /// first edge, and a value greater than the upper limit or last edge lies above it.
    /// A value exactly at a limit, or at the first or last edge, lies inside, and a value
    /// exactly on an edge between two bands belongs to the upper band. On the entries of
    /// an indexed or categorical scale, a whole number from 0 to `count − 1` (negative
    /// zero included) takes that entry, and every other finite value lies
    /// [outside](ColorScaleClass::Outside).
    #[must_use]
    pub fn classify(&self, value: f64) -> ColorScaleClass {
        if !value.is_finite() {
            return ColorScaleClass::NonFinite;
        }
        match self {
            ColorScaleDomain::Continuous { min, max, norm } => {
                if (*norm == ColorScaleNorm::Log && value <= 0.0) || value < *min {
                    return ColorScaleClass::Below;
                }
                if value > *max {
                    return ColorScaleClass::Above;
                }
                let place = |v: f64| match norm {
                    ColorScaleNorm::Linear => v,
                    ColorScaleNorm::Log => v.log10(),
                };
                let span = place(*max) - place(*min);
                let position = if span.is_finite() && span > 0.0 {
                    ((place(value) - place(*min)) / span).clamp(0.0, 1.0)
                } else {
                    0.5
                };
                ColorScaleClass::Inside(ColorScaleSlot::Position(position))
            }
            ColorScaleDomain::Discrete { edges, norm } => {
                let (Some(&first), Some(&last)) = (edges.first(), edges.last()) else {
                    return ColorScaleClass::Below;
                };
                if (*norm == ColorScaleNorm::Log && value <= 0.0) || value < first {
                    return ColorScaleClass::Below;
                }
                if value > last {
                    return ColorScaleClass::Above;
                }
                // The number of edges at or below the value, less one, is the band whose
                // lower edge the value reaches; the last edge closes the last band.
                let reached = edges.partition_point(|&edge| edge <= value);
                let bands = edges.len().saturating_sub(1).max(1);
                ColorScaleClass::Inside(ColorScaleSlot::Band(
                    reached.saturating_sub(1).min(bands - 1),
                ))
            }
            ColorScaleDomain::Entries { count } => {
                if value.fract() == 0.0 && value >= 0.0 && value < *count as f64 {
                    // The value is a whole number within the entries, so the conversion
                    // is exact.
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "the value is a whole number from 0 to the number of entries"
                    )]
                    let entry = value as usize;
                    ColorScaleClass::Inside(ColorScaleSlot::Entry(entry))
                } else {
                    ColorScaleClass::Outside
                }
            }
        }
    }
}

/// How an artist maps numbers to colours: the colour scale it names, if any, and the array
/// of the numbers it maps through that scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorMapping {
    /// The colour scale that the artist names, or `None` when it names none, which makes
    /// the figure invalid.
    pub color_scale: Option<NodeId>,
    /// The data array of the numbers that the artist maps: the values of a scalar image or
    /// of cells,
    /// the field of a contour, the colour data of a surface (or its field when it has
    /// none), and the colour data of a series or of bars.
    pub values: NodeId,
}

impl Artist {
    /// Returns the colour scale that the artist names, whether or not it maps any numbers
    /// through it, or `None` for an artist that names none or cannot name one.
    #[must_use]
    pub fn color_scale(&self) -> Option<NodeId> {
        match self {
            Artist::ScalarImage(a) => a.color_scale,
            Artist::Contour(a) => a.color_scale,
            Artist::Surface(a) => a.color_scale,
            Artist::Series(a) => a.color_scale,
            Artist::Cells(a) => a.color_scale,
            Artist::Bar(a) => a.color_scale,
            Artist::Quiver(_) | Artist::Image(_) | Artist::Stairs(_) => None,
        }
    }

    /// Returns how the artist maps numbers to colours, or `None` when it maps none.
    ///
    /// A scalar image and cells always map their values. A contour maps its field when its
    /// bands are filled or its isolines are colour-mapped, a surface maps its colour data
    /// (or its field) when its faces or its edges are colour-mapped, and a series and bars
    /// map their colour data whenever they have some (a series per point or per segment,
    /// bars per bar), whatever their colours.
    /// On a contour's isolines and a surface's faces and edges, an automatic colour is
    /// colour-mapped as an explicit `Colormapped` colour is, because neither artist has a
    /// colour of its own for `Auto` to stand for. A series or bars without colour data, a
    /// quiver and a true-colour image map nothing, even when a colour is colormapped: such
    /// a colour has no number to map and is drawn in the middle colour of viridis (or, on
    /// markers and bars, as an automatic colour). Stairs map nothing either: they take
    /// fixed colours or the colour order, and a colormapped colour on them is invalid.
    ///
    /// This is the one rule by which validation, automatic limits and the scene compiler
    /// decide whether an artist is colour-mapped.
    #[must_use]
    pub fn color_mapping(&self) -> Option<ColorMapping> {
        let mapped = |spec: ColorSpec| matches!(spec, ColorSpec::Auto | ColorSpec::Colormapped);
        let (color_scale, values) = match self {
            Artist::ScalarImage(a) => (a.color_scale, a.values),
            Artist::Contour(a) if a.fill || mapped(a.line.color) => (a.color_scale, a.z),
            Artist::Surface(a) if mapped(a.face) || mapped(a.edge) => {
                (a.color_scale, a.c.unwrap_or(a.z))
            }
            Artist::Series(a) => (a.color_scale, a.color_data?),
            Artist::Bar(a) => (a.color_scale, a.color_data?),
            Artist::Cells(a) => (a.color_scale, a.values),
            Artist::Contour(_)
            | Artist::Surface(_)
            | Artist::Quiver(_)
            | Artist::Image(_)
            | Artist::Stairs(_) => {
                return None;
            }
        };
        Some(ColorMapping {
            color_scale,
            values,
        })
    }
}

impl Figure {
    /// Returns the data arrays of the numbers mapped through a colour scale, one for each
    /// artist that maps numbers through it, in tree order; hidden artists are included,
    /// because a hidden artist still contributes to the automatic limits of its scale.
    #[must_use]
    pub fn color_scale_values(&self, scale: NodeId) -> Vec<NodeId> {
        self.axes
            .iter()
            .flat_map(|axes| &axes.artists)
            .filter_map(Artist::color_mapping)
            .filter(|mapping| mapping.color_scale == Some(scale))
            .map(|mapping| mapping.values)
            .collect()
    }

    /// Returns the automatic limits of a colour scale: the least and greatest finite
    /// numbers (positive finite numbers, under a logarithmic norm) that any artist maps
    /// through it, in every axes, hidden artists included, as
    /// [`ColorScaleNorm::automatic_limits`] finds them. An 8-bit value counts as the
    /// whole number it denotes. Returns `None` when the figure has no colour scale of the
    /// identifier, or when no artist maps such a number through it.
    ///
    /// The limits are those of the scale whether or not its limits are automatic, so a
    /// caller resolves manual limits first, as [`ColorScale::range`] does.
    #[must_use]
    pub fn automatic_color_limits(&self, scale: NodeId) -> Option<(f64, f64)> {
        let norm = self.color_scale(scale)?.kind.norm();
        let arrays: Vec<&Values> = self
            .color_scale_values(scale)
            .into_iter()
            .filter_map(|id| self.data_array(id))
            .map(|data| &data.array.values)
            .collect();
        norm.automatic_limits(arrays.into_iter().flat_map(
            |values| -> Box<dyn Iterator<Item = f64>> {
                match values {
                    Values::F64(values) => Box::new(values.iter().copied()),
                    Values::U8(values) => Box::new(values.iter().map(|&v| f64::from(v))),
                }
            },
        ))
    }
}
