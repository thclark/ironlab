//! Images: the pixel edges of a placement, the raster of samples resolved under the out-of-range policies of an
//! artist, and the mean colour of a true-colour image for its legend sample.

use std::sync::Arc;

use ironlab_ir::{Color, OutOfRange, PixelRange, Values};

use crate::display::{ImageItem, Rgba};
use crate::maths::colormap::Lut;
use crate::maths::image::{Lookup, lookup_index, lookup_value, quantise};

use super::data::{ImageData, ImageKind, Policies};
use super::style::{self, ColourScale};

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

/// A pixel that a strict policy of the artist refuses, which stops the image from being drawn.
pub(super) struct Refused {
    pub row: usize,
    pub column: usize,
    /// The name of the category and of its policy: `below`, `above` or `non_finite`.
    pub category: &'static str,
    /// What is wrong with the pixel, completing "the pixel in row j and column i …".
    pub condition: &'static str,
}

impl Refused {
    /// Returns the warning that reports the refusal.
    pub fn message(&self) -> String {
        format!(
            "The artist is not drawn because the pixel in row {} and column {} {}, which its strict {} policy \
             refuses.",
            self.row, self.column, self.condition, self.category
        )
    }
}

/// What is wrong with a pixel of each category, completing "the pixel in row j and column i …".
struct Conditions {
    below: &'static str,
    above: &'static str,
    non_finite: &'static str,
}

const VALUE_CONDITIONS: Conditions = Conditions {
    below: "lies below the lower colour limit",
    above: "lies above the upper colour limit",
    non_finite: "is not finite",
};

const INDEX_CONDITIONS: Conditions = Conditions {
    below: "has an index below 0",
    above: "has an index above 255",
    non_finite: "has an index that is not finite",
};

/// A pixel that draws nothing.
const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];

/// An opaque pixel of a colormap entry.
fn opaque(rgb: [u8; 3]) -> [u8; 4] {
    [rgb[0], rgb[1], rgb[2], 255]
}

/// Resolves every pixel of an image to eight-bit colour under the colour scale of its axes and the policies of the
/// artist, in row order from row 0 with the pixel in row `j` and column `i` at index `(j · nx + i) · channels`.
///
/// A true-colour image clamps floating-point components into `[0, 1]` and quantises them, copies 8-bit components,
/// and draws a pixel with a non-finite component transparent. A colour-mapped image looks each value up through the
/// colour limits and a colour-indexed image looks each index up in the colormap directly, a value being widened
/// from 8 bits where the array holds bytes; a pixel that either lookup cannot colour takes what the policy of its
/// category says, and a strict policy refuses the whole image at its first such pixel.
pub(super) fn resolve(image: &ImageData, scale: &ColourScale) -> Result<Raster, Refused> {
    let mut pixels: Vec<[u8; 4]> = Vec::with_capacity(image.nx * image.ny);
    match image.kind {
        ImageKind::TrueColour => match &image.array.values {
            Values::F64(values) => {
                pixels.extend(values.chunks_exact(image.components).map(float_pixel));
            }
            Values::U8(values) => {
                pixels.extend(values.chunks_exact(image.components).map(byte_pixel));
            }
        },
        ImageKind::Indexed(policies) => match &image.array.values {
            Values::U8(indices) => {
                pixels.extend(indices.iter().map(|&i| opaque(scale.lut[usize::from(i)])));
            }
            Values::F64(indices) => {
                for (k, &index) in indices.iter().enumerate() {
                    let lookup = lookup_index(scale.lut, index);
                    pixels.push(decide(
                        lookup,
                        &policies,
                        scale.lut,
                        k,
                        image.nx,
                        &INDEX_CONDITIONS,
                    )?);
                }
            }
        },
        ImageKind::Mapped(policies) => {
            let mut push = |k: usize, value: f64| -> Result<(), Refused> {
                let lookup = lookup_value(scale.lut, value, scale.min, scale.max);
                pixels.push(decide(
                    lookup,
                    &policies,
                    scale.lut,
                    k,
                    image.nx,
                    &VALUE_CONDITIONS,
                )?);
                Ok(())
            };
            match &image.array.values {
                Values::F64(values) => {
                    for (k, &value) in values.iter().enumerate() {
                        push(k, value)?;
                    }
                }
                Values::U8(values) => {
                    for (k, &value) in values.iter().enumerate() {
                        push(k, f64::from(value))?;
                    }
                }
            }
        }
    }
    Ok(pack(pixels))
}

/// Decides the colour of the pixel at row-major index `k` of an image `nx` columns wide from its lookup: an entry
/// is drawn opaque, and a pixel in one of the three categories takes what the policy of that category says.
///
/// A clamp takes the first entry below and the last entry above, which are the ends of the fixed 256-entry
/// table: a stopgap for the colormap redesign, under which a colormap of another length must supply its own ends.
/// At the non-finite category a clamp has no nearest end and draws nothing.
fn decide(
    lookup: Lookup,
    policies: &Policies,
    lut: &Lut,
    k: usize,
    nx: usize,
    conditions: &Conditions,
) -> Result<[u8; 4], Refused> {
    let (policy, category, condition, clamp) = match lookup {
        Lookup::Entry(rgb) => return Ok(opaque(rgb)),
        Lookup::Below => (policies.below, "below", conditions.below, Some(lut[0])),
        Lookup::Above => (policies.above, "above", conditions.above, Some(lut[255])),
        Lookup::NonFinite => (
            policies.non_finite,
            "non_finite",
            conditions.non_finite,
            None,
        ),
    };
    match policy {
        OutOfRange::Strict => Err(Refused {
            row: k / nx,
            column: k % nx,
            category,
            condition,
        }),
        OutOfRange::Transparent => Ok(TRANSPARENT),
        OutOfRange::Clamp => Ok(clamp.map_or(TRANSPARENT, opaque)),
        OutOfRange::Rgba { color } => Ok(fixed(color)),
    }
}

/// Quantises the fixed colour of a policy, alpha included, after clamping its channels as every IR colour is.
fn fixed(color: Color) -> [u8; 4] {
    let c = style::ir_colour(color);
    [c.r, c.g, c.b, c.a].map(|channel| quantise(f64::from(channel)))
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
