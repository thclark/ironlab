//! Resolution of the pixels of images into eight-bit colour.
//!
//! The numbers of a scalar image are mapped through its colour scale by the scene compiler, which classifies them
//! against the scale with the figure model's own classification; this module quantises the resulting colours and the
//! components of true-colour images into eight bits.

/// Quantises a colour component from 0 to 1 into eight bits by rounding, after clamping a component that strays
/// outside the range; a non-finite component gives 0, so callers that draw such a pixel transparent must test for
/// it first.
pub fn quantise(component: f64) -> u8 {
    // The clamp bounds the product by 255, and a NaN survives the clamp only to become 0 in the saturating cast.
    (component.clamp(0.0, 1.0) * 255.0).round() as u8
}
