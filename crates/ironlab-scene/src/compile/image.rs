//! Images: the pixel edges of a placement, the raster of samples resolved through the colour scale of a scalar
//! image, the alpha and the opacity of an artist, and the mean colour of a true-colour image for its legend sample.

use std::sync::Arc;

use ironlab_ir::{PixelRange, Values};

use crate::display::{ImageItem, Rgba};
use crate::maths::image::quantise;

use super::data::{ImageData, ImageKind};
use super::style::{self, ResolvedScale};

/// Returns the edges of the pixels along one axis of an image of `n` pixels whose first and last centres `range`
/// gives, or which lie at 0 and n − 1 when it is absent.
///
/// The pitch between centres is `(last − first) / (n − 1)`, and the image covers half a pitch beyond each of the
/// two centres, so a range that runs backwards gives a first edge beyond its last and mirrors the image. A single
/// pixel has no pitch to derive, so it is one data unit wide about its first centre and its last centre is
/// ignored.
pub(super) fn edges(range: Option<PixelRange>, n: usize) -> (f64, f64) {
    let (first, last) = match range {
        Some(PixelRange { first, last }) if n > 1 => (first, last),
        Some(PixelRange { first, .. }) => (first, first),
        None => (0.0, n.saturating_sub(1) as f64),
    };
    let pitch = if n > 1 {
        (last - first) / (n - 1) as f64
    } else {
        1.0
    };
    (first - pitch / 2.0, last + pitch / 2.0)
}

/// The samples of an image resolved to eight-bit colour, ready for an [`ImageItem`].
pub(super) struct Raster {
    /// 3 when every pixel is opaque, 4 otherwise.
    pub channels: u8,
    /// `nx · ny · channels` bytes in row order from row 0.
    pub samples: Arc<[u8]>,
}

/// A pixel that draws nothing.
const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];

/// Quantises a colour, alpha included, into the four channels of a pixel, or returns a transparent pixel for no
/// colour.
fn pixel(colour: Option<Rgba>) -> [u8; 4] {
    colour.map_or(TRANSPARENT, |c| {
        [c.r, c.g, c.b, c.a].map(|channel| quantise(f64::from(channel)))
    })
}

/// Resolves every pixel of an image to eight-bit colour, in row order from row 0 with the pixel in row `j` and
/// column `i` at index `(j · nx + i) · channels`.
///
/// A true-colour image clamps floating-point components into `[0, 1]` and quantises them, copies 8-bit components,
/// and draws a pixel with a non-finite component transparent. A scalar image maps each number through its colour
/// scale (see [`ResolvedScale::colour`]), a value being widened from 8 bits where the array holds bytes, and draws a
/// pixel transparent where the scale draws nothing; the 256 values a byte can hold are mapped once each. A scalar
/// image without a scale, which the data stage never lets through, draws nothing. The alpha of every pixel is then
/// multiplied by its value in the alpha array of the artist and by the opacity of the artist, as [`apply_alpha`]
/// describes.
pub(super) fn resolve(image: &ImageData, scale: Option<&ResolvedScale>) -> Raster {
    let mut pixels: Vec<[u8; 4]> = Vec::with_capacity(image.nx * image.ny);
    let map = |value: f64| pixel(scale.and_then(|scale| scale.colour(value)));
    match (image.kind, &image.array.values) {
        (ImageKind::TrueColour, Values::F64(values)) => {
            pixels.extend(values.chunks_exact(image.components).map(float_pixel));
        }
        (ImageKind::TrueColour, Values::U8(values)) => {
            pixels.extend(values.chunks_exact(image.components).map(byte_pixel));
        }
        (ImageKind::Scalar, Values::F64(values)) => {
            pixels.extend(values.iter().map(|&value| map(value)));
        }
        (ImageKind::Scalar, Values::U8(values)) => {
            let table: Vec<[u8; 4]> = (0..=u8::MAX).map(|v| map(f64::from(v))).collect();
            pixels.extend(values.iter().map(|&v| table[usize::from(v)]));
        }
    }
    apply_alpha(&mut pixels, image);
    pack(pixels)
}

/// Multiplies the alpha of every pixel, resolved from its colour, by its value in the alpha array of the image and
/// by the opacity of the image, and quantises the product.
///
/// A floating-point alpha is clamped to `[0, 1]` and a non-finite one is 0, and an 8-bit alpha is divided by 255,
/// as the components of a true-colour image are read. The product applies to every pixel alike, including one
/// drawn in the fixed colour of an out-of-range policy, whose own alpha it multiplies. A pixel whose product is 0
/// is made wholly transparent. An image without an alpha array and with an opacity of 1 is left exactly as its
/// colours resolved, so that its samples are those it had before alpha and opacity existed.
fn apply_alpha(pixels: &mut [[u8; 4]], image: &ImageData) {
    let opacity = style::unit_opacity(image.opacity);
    if image.alpha.is_none() && opacity == 1.0 {
        return;
    }
    let alpha_at = |k: usize| match image.alpha.map(|alpha| &alpha.values) {
        None => 1.0,
        Some(Values::F64(values)) if values[k].is_finite() => values[k].clamp(0.0, 1.0),
        Some(Values::F64(_)) => 0.0,
        Some(Values::U8(values)) => f64::from(values[k]) / 255.0,
    };
    for (k, pixel) in pixels.iter_mut().enumerate() {
        let alpha = quantise(f64::from(pixel[3]) / 255.0 * alpha_at(k) * opacity);
        *pixel = if alpha == 0 {
            TRANSPARENT
        } else {
            [pixel[0], pixel[1], pixel[2], alpha]
        };
    }
}

/// Resolves a pixel of three or four floating-point components: transparent when any component is not finite, and
/// otherwise each component clamped and quantised, with an absent alpha read as opaque.
fn float_pixel(components: &[f64]) -> [u8; 4] {
    if !components.iter().all(|c| c.is_finite()) {
        return TRANSPARENT;
    }
    let alpha = components.get(3).map_or(255, |&a| quantise(a));
    [
        quantise(components[0]),
        quantise(components[1]),
        quantise(components[2]),
        alpha,
    ]
}

/// Resolves a pixel of three or four 8-bit components, copied as supplied, with an absent alpha read as opaque.
fn byte_pixel(components: &[u8]) -> [u8; 4] {
    [
        components[0],
        components[1],
        components[2],
        components.get(3).copied().unwrap_or(255),
    ]
}

/// Packs pixels into samples of three channels when every pixel is opaque and of four otherwise, so that a backend
/// uploads an opaque image without blending and spends an alpha channel only where one pixel needs it.
fn pack(pixels: Vec<[u8; 4]>) -> Raster {
    if pixels.iter().all(|p| p[3] == 255) {
        Raster {
            channels: ImageItem::RGB,
            samples: pixels.iter().flat_map(|p| p[..3].iter().copied()).collect(),
        }
    } else {
        Raster {
            channels: ImageItem::RGBA,
            samples: pixels.iter().flatten().copied().collect(),
        }
    }
}

/// Returns the mean colour of the pixels of a true-colour image whose components are all finite, each component
/// quantised as it is drawn, or `None` when no pixel qualifies. The colour is opaque: the alpha of the pixels
/// plays no part.
pub(super) fn mean_colour(image: &ImageData) -> Option<Rgba> {
    let mut sums = [0u64; 3];
    let mut count = 0u64;
    let mut add = |pixel: [u8; 4]| {
        for (sum, component) in sums.iter_mut().zip(pixel) {
            *sum += u64::from(component);
        }
        count += 1;
    };
    match &image.array.values {
        Values::F64(values) => values
            .chunks_exact(image.components)
            .filter(|components| components.iter().all(|c| c.is_finite()))
            .map(float_pixel)
            .for_each(&mut add),
        Values::U8(values) => values
            .chunks_exact(image.components)
            .map(byte_pixel)
            .for_each(&mut add),
    }
    (count > 0).then(|| {
        // The mean of bytes is a byte, so the rounding cast is exact.
        Rgba::from_u8(sums.map(|sum| (sum as f64 / count as f64).round() as u8))
    })
}
