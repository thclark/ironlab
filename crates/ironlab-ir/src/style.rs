//! Colours, line styles and marker styles.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::IrError;

/// An sRGB colour with straight (non-premultiplied) alpha.
///
/// Each component lies in the range 0 to 1. In JSON a colour is the string
/// `#rrggbb` when it is opaque, or `#rrggbbaa` otherwise, so the stored precision
/// is eight bits per component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// The red component, from 0 to 1.
    pub r: f32,
    /// The green component, from 0 to 1.
    pub g: f32,
    /// The blue component, from 0 to 1.
    pub b: f32,
    /// The opacity, from 0 (transparent) to 1 (opaque).
    pub a: f32,
}

impl Color {
    /// Opaque black.
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    /// Opaque white.
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);

    /// Creates an opaque colour from red, green and blue components between 0 and 1.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Creates a colour from red, green, blue and alpha components between 0 and 1.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Parses a colour from `#rrggbb` or `#rrggbbaa`, accepting upper- or lower-case
    /// hexadecimal digits.
    ///
    /// Each two-digit byte `n` becomes the component `n as f32 / 255.0`, so that a
    /// parsed colour compares equal to the same colour constructed from `n / 255`.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::InvalidColor`] when the string has any other form.
    pub fn from_hex(hex: &str) -> Result<Self, IrError> {
        let invalid = || IrError::InvalidColor(hex.to_owned());
        let digits = hex.strip_prefix('#').ok_or_else(invalid)?.as_bytes();
        if !matches!(digits.len(), 6 | 8) || !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err(invalid());
        }
        let component = |i: usize| {
            let byte = (hex_value(digits[i]) << 4) | hex_value(digits[i + 1]);
            f32::from(byte) / 255.0
        };
        let a = if digits.len() == 8 { component(6) } else { 1.0 };
        Ok(Self::rgba(component(0), component(2), component(4), a))
    }

    /// Formats the colour as lower-case `#rrggbb` when it is opaque, or as
    /// `#rrggbbaa` otherwise, rounding each component to the nearest of 256 levels.
    pub fn to_hex(&self) -> String {
        let [r, g, b, a] = [self.r, self.g, self.b, self.a].map(level);
        if a == u8::MAX {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }
}

/// Returns the value of an ASCII hexadecimal digit.
fn hex_value(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        _ => digit - b'A' + 10,
    }
}

/// Rounds a colour component to the nearest of 256 levels, clamping it to the range
/// 0 to 1 first; a NaN component becomes level 0.
fn level(component: f32) -> u8 {
    // The clamped, rounded value lies in 0..=255 (or is NaN, which casts to 0).
    (component.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(deserializer)?;
        Color::from_hex(&hex).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Color {
    fn schema_name() -> Cow<'static, str> {
        "Color".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$",
            "description": "An sRGB colour with straight alpha, written as #rrggbb when opaque or #rrggbbaa otherwise."
        })
    }
}

/// How a colour is chosen for a stroke, fill or marker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ColorSpec {
    /// The renderer chooses the colour: for series this is the next colour of the
    /// axes colour order (Okabe–Ito), and inside a scatter marker it is the scatter
    /// colour.
    #[default]
    Auto,
    /// A fixed colour.
    Rgba {
        /// The colour to use.
        color: Color,
    },
    /// Nothing is drawn.
    None,
    /// The colour is taken from the axes colormap, indexed by the data value
    /// scaled into the axes colour limits.
    Colormapped,
}

/// The style of a stroked line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LineStyle {
    /// The line colour.
    pub color: ColorSpec,
    /// The line width in points.
    pub width_pt: f64,
    /// The dash pattern of the line.
    pub dash: DashStyle,
}

impl Default for LineStyle {
    fn default() -> Self {
        Self {
            color: ColorSpec::Auto,
            width_pt: 0.75,
            dash: DashStyle::Solid,
        }
    }
}

/// The dash pattern of a stroked line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DashStyle {
    /// A continuous line.
    #[default]
    Solid,
    /// A line of dashes.
    Dashed,
    /// A line of dots.
    Dotted,
    /// A line of alternating dashes and dots.
    DashDot,
    /// No line is drawn.
    None,
}

/// The style of the markers drawn at data points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MarkerStyle {
    /// The marker shape.
    pub shape: MarkerShape,
    /// The marker size in points, measured as the width of the marker.
    pub size_pt: f64,
    /// The colour of the marker interior.
    pub face: ColorSpec,
    /// The colour of the marker outline.
    pub edge: ColorSpec,
}

impl Default for MarkerStyle {
    fn default() -> Self {
        Self {
            shape: MarkerShape::None,
            size_pt: 4.0,
            face: ColorSpec::None,
            edge: ColorSpec::Auto,
        }
    }
}

/// The shape of a marker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkerShape {
    /// No marker is drawn.
    #[default]
    None,
    /// A circle.
    Circle,
    /// An axis-aligned square.
    Square,
    /// A square rotated by 45 degrees.
    Diamond,
    /// A triangle pointing upwards.
    TriangleUp,
    /// A triangle pointing downwards.
    TriangleDown,
    /// A plus sign.
    Plus,
    /// A diagonal cross.
    Cross,
    /// A small filled dot.
    Point,
}
