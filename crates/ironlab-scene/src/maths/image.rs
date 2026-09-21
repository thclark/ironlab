//! Resolution of the pixels of images into eight-bit colour.
//!
//! A colour-mapped image looks each value up through the colour limits, a colour-indexed image looks each index up
//! in the colormap directly, and a true-colour image quantises each component. The lookups classify a pixel the
//! artist cannot colour on its own into one of three categories (below, above and non-finite), which the artist's
//! out-of-range policies then decide; those policies belong to the figure model and are applied by the scene
//! compiler, so this module knows only the categories.

use super::colormap::{self, Lut};

/// The result of looking up one pixel of a colour-mapped or colour-indexed image in the colormap: the entry it
/// takes, or the category of pixel that the artist's out-of-range policies decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lookup {
    /// The pixel takes this entry of the colormap.
    Entry([u8; 3]),
    /// The value lies below the lower colour limit, or the index is below 0.
    Below,
    /// The value lies above the upper colour limit, or the index is above 255.
    Above,
    /// The value or index is NaN or infinite.
    NonFinite,
}

/// Looks up a value of a colour-mapped image through the colour limits `cmin` to `cmax`.
///
/// A finite value is normalised by [`colormap::normalise`]; one that normalises inside the unit interval takes the
/// entry [`colormap::sample`] gives, so that an image agrees with a surface or scatter coloured beside it, and one
/// that normalises below 0 or above 1 falls in the category on that side.
///
/// This classification is a stopgap for the colormap redesign: the colour scale of the compiler clamps every value
/// into the colormap, so the categories are told apart here over `normalise` until the scale exposes them itself.
pub fn lookup_value(lut: &Lut, value: f64, cmin: f64, cmax: f64) -> Lookup {
    if !value.is_finite() {
        return Lookup::NonFinite;
    }
    let t = colormap::normalise(value, cmin, cmax);
    if t < 0.0 {
        Lookup::Below
    } else if t > 1.0 {
        Lookup::Above
    } else {
        colormap::sample(lut, t).map_or(Lookup::NonFinite, Lookup::Entry)
    }
}

/// Looks up a floating-point index of a colour-indexed image in the colormap.
///
/// The index is truncated toward zero, so an index from −1 exclusive to 256 exclusive names an entry, and −0.5 is
/// entry 0 rather than below the colormap. The colour limits play no part.
///
/// The domain of 0 to 255 is that of the fixed 256-entry tables, a stopgap for the colormap redesign: a colormap of
/// another length must supply its own domain.
pub fn lookup_index(lut: &Lut, index: f64) -> Lookup {
    if !index.is_finite() {
        return Lookup::NonFinite;
    }
    let truncated = index.trunc();
    if truncated < 0.0 {
        Lookup::Below
    } else if truncated > 255.0 {
        Lookup::Above
    } else {
        // The bounds make the cast exact.
        Lookup::Entry(lut[truncated as usize])
    }
}

/// Quantises a colour component from 0 to 1 into eight bits by rounding, after clamping a component that strays
/// outside the range; a non-finite component gives 0, so callers that draw such a pixel transparent must test for
/// it first.
pub fn quantise(component: f64) -> u8 {
    // The clamp bounds the product by 255, and a NaN survives the clamp only to become 0 in the saturating cast.
    (component.clamp(0.0, 1.0) * 255.0).round() as u8
}
