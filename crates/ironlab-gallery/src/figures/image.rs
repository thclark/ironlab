use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Image";
pub const DESCRIPTION: &str = "A domain colouring of the third Julia iterate z₃, drawn as true-colour pixels whose \
     hue follows the argument of z₃ and whose lightness follows its magnitude. The alpha channel of the pixels fades \
     to transparent outside a disc, so the axes background shows through the corners of the image.";

/// The number of pixels along each side of the image.
const PIXELS: usize = 301;

/// The radius within which the pixels are opaque.
const OPAQUE_RADIUS: f64 = 1.2;

/// The radius beyond which the pixels are fully transparent.
const TRANSPARENT_RADIUS: f64 = 1.5;

pub fn figure() -> Figure {
    // The rows of an image count downwards from its top, so row 0 samples the top of the domain and the rows are
    // placed with a range that runs from the top of the domain to its bottom.
    let pitch = (DOMAIN_MAX - DOMAIN_MIN) / (PIXELS - 1) as f64;
    let pixels = Pixels::rgba_from_fn(PIXELS, PIXELS, |row, col| {
        let x = DOMAIN_MIN + col as f64 * pitch;
        let y = DOMAIN_MAX - row as f64 * pitch;
        let (re, im) = julia_iterate(x, y);
        // The hue is the argument of z₃ as a fraction of a turn, and the lightness rises from black at a zero of z₃
        // towards white as ln(1 + |z₃|) grows.
        let hue = im.atan2(re) / std::f64::consts::TAU;
        let magnitude = re.hypot(im).ln_1p();
        let lightness = magnitude / (1.0 + magnitude);
        let fade = (TRANSPARENT_RADIUS - x.hypot(y)) / (TRANSPARENT_RADIUS - OPAQUE_RADIUS);
        Color {
            a: fade.clamp(0.0, 1.0) as f32,
            ..hsl_to_rgb(hue, 1.0, lightness)
        }
    });

    let mut fig = Figure::new()
        .size_mm(120.0, 100.0)
        .title(r"Domain colouring of $z_3$");
    let mut ax = fig.axes(0, 0);
    ax.image(&pixels)
        .pixel_columns(DOMAIN_MIN, DOMAIN_MAX)
        .pixel_rows(DOMAIN_MAX, DOMAIN_MIN);
    ax.xlabel("$x$").ylabel("$y$");
    fig
}
