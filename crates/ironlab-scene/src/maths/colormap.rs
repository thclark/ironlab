//! Colour maps as 256-entry sRGB lookup tables.
//!
//! The tables are generated from matplotlib by `crates/ironlab-scene/scripts/generate_colormaps.py`
//! (see that script for the provenance and licence of each table) and live in the generated
//! `colormap_data` module.

pub use super::colormap_data::{CIVIDIS, COOLWARM, GRAY, INFERNO, MAGMA, PLASMA, VIRIDIS};

/// A colour map: 256 sRGB colours ordered from the lowest to the highest value.
pub type Lut = [[u8; 3]; 256];

/// Looks up the colour for a normalised value `t`.
///
/// `t` is clamped to [0, 1] (so infinities map to the end colours) and the entry index is
/// `min(floor(t · 256), 255)`, which gives every entry an equal share of the unit interval, as
/// matplotlib does. Returns `None` for NaN, which marks a missing value that must not be painted.
pub fn sample(lut: &Lut, t: f64) -> Option<[u8; 3]> {
    if t.is_nan() {
        return None;
    }
    let t = t.clamp(0.0, 1.0);
    // The float-to-integer cast saturates, and t ≤ 1 bounds the product by 256.
    let index = ((t * 256.0).floor() as usize).min(255);
    Some(lut[index])
}

/// Maps `value` linearly so that `cmin` becomes 0 and `cmax` becomes 1.
///
/// The result is not clamped; [`sample`] clamps. NaN propagates. When `cmin == cmax`, every
/// finite value maps to 0.5, so a constant field takes the middle colour. When `cmin > cmax`, the
/// mapping is reversed.
pub fn normalise(value: f64, cmin: f64, cmax: f64) -> f64 {
    if value.is_nan() {
        return f64::NAN;
    }
    if cmin == cmax {
        // Infinite values keep their sign so that `sample` still clamps them to the ends.
        return if value.is_finite() { 0.5 } else { value };
    }
    // Adding positive zero turns a negative zero (from reversed limits) into positive zero.
    (value - cmin) / (cmax - cmin) + 0.0
}
