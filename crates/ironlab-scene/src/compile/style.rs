//! Colours, the automatic colour order, colormaps, dash patterns and marker shapes.

use ironlab_ir::{
    Artist, Axes, Color, ColorSpec, ColormapName, DashStyle, MarkerShape, MarkerStyle, NodeId,
    ScatterColor,
};

use crate::display::{Item, PathSegment, Point, Rgba};
use crate::maths::colormap::{self, Lut};

use super::paths::{self, PathBuilder};

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

/// Returns the lookup table of a colormap.
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

/// Maps data values to colours through an axes' colormap and colour limits.
#[derive(Clone, Copy)]
pub(crate) struct ColourScale {
    pub lut: &'static Lut,
    pub min: f64,
    pub max: f64,
}

impl ColourScale {
    /// Returns the colour of `value`, or `None` for NaN.
    pub fn colour(&self, value: f64) -> Option<Rgba> {
        colormap::sample(self.lut, colormap::normalise(value, self.min, self.max))
            .map(Rgba::from_u8)
    }

    /// Returns the middle colour of the colormap, used where a colormapped colour has no data value.
    pub fn middle(&self) -> Rgba {
        Rgba::from_u8(self.lut[128])
    }
}

/// The resolved primary colour of an artist.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Paint {
    /// A fixed colour, or no colour at all.
    Fixed(Option<Rgba>),
    /// A colour taken from the colormap.
    Colormapped,
}

impl Paint {
    /// Returns the colour, using the colormap's middle colour for a colormapped paint.
    pub fn single(self, scale: &ColourScale) -> Option<Rgba> {
        match self {
            Paint::Fixed(c) => c,
            Paint::Colormapped => Some(scale.middle()),
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

/// Resolves the primary colour of every artist of an axes, assigning automatic colours in artist order.
///
/// Contour and surface artists have no primary colour and receive `Paint::Colormapped`.
pub(super) fn primaries(axes: &Axes) -> Vec<Paint> {
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
                Artist::Line(line) => line.line.color,
                Artist::Quiver(quiver) => quiver.line.color,
                Artist::Scatter(scatter) => match scatter.color {
                    ScatterColor::Spec { spec } => spec,
                    ScatterColor::Data { .. } => ColorSpec::Colormapped,
                },
                Artist::Contour(_) | Artist::Surface(_) => ColorSpec::Colormapped,
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

/// The outline of a marker centred on a point.
pub(super) struct MarkerOutline {
    pub segments: Vec<PathSegment>,
    /// Whether the outline encloses an area that a face colour fills.
    pub closed: bool,
    /// Whether the marker is a solid dot filled with its edge colour.
    pub dot: bool,
}

/// Builds the outline of a marker of width `size` centred on `c`, or `None` for `MarkerShape::None` or an
/// unusable size.
pub(super) fn marker_outline(shape: MarkerShape, c: Point, size: f64) -> Option<MarkerOutline> {
    if !(size.is_finite() && size > 0.0) {
        return None;
    }
    let h = size / 2.0;
    let mut b = PathBuilder::new();
    let at = |dx: f64, dy: f64| Point::new(c.x + dx, c.y + dy);
    let (closed, dot) = match shape {
        MarkerShape::None => return None,
        MarkerShape::Circle => {
            b.circle(c, h);
            (true, false)
        }
        MarkerShape::Point => {
            b.circle(c, size / 6.0);
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
        MarkerShape::Cross => {
            let s = h * std::f64::consts::FRAC_1_SQRT_2;
            b.polyline(&[at(-s, -s), at(s, s)], false);
            b.polyline(&[at(-s, s), at(s, -s)], false);
            (false, false)
        }
    };
    Some(MarkerOutline {
        segments: b.finish(),
        closed,
        dot,
    })
}

/// Builds one marker item of width `size` centred on `centre`.
///
/// A face or edge set to `Auto` (or `Colormapped`) takes `auto`, the resolved colour of the artist or data point.
/// Every colour is multiplied by `alpha`. An open marker (plus or cross) is stroked with its edge colour, or with
/// its face colour when it has no edge colour, and a point marker is filled with its edge colour.
pub(super) fn marker_item(
    source: NodeId,
    style: &MarkerStyle,
    centre: Point,
    size: f64,
    auto: Option<Rgba>,
    width: f64,
    alpha: f32,
) -> Option<Item> {
    let resolve = |spec: ColorSpec| {
        match spec {
            ColorSpec::Auto | ColorSpec::Colormapped => auto,
            ColorSpec::Rgba { color } => Some(ir_colour(color)),
            ColorSpec::None => None,
        }
        .map(|c| c.with_alpha_factor(alpha))
    };
    let (face, edge) = (resolve(style.face), resolve(style.edge));
    let outline = marker_outline(style.shape, centre, size)?;
    if outline.dot {
        return paths::item(
            source,
            outline.segments,
            Some(paths::fill(edge.or(face)?)),
            None,
        );
    }
    let fill = if outline.closed {
        face.map(paths::fill)
    } else {
        None
    };
    let edge = if outline.closed { edge } else { edge.or(face) };
    let stroke = edge.map(|c| paths::stroke(c, width, Vec::new()));
    paths::item(source, outline.segments, fill, stroke)
}
