//! Data shared by the gallery figures.
//!
//! The gridded figures plot the same scalar field, derived from a few iterations of the quadratic map that defines
//! a Julia set. The field is smooth near the origin and grows rapidly towards the corners of the domain, so it has
//! enough structure to exercise contouring, colour mapping and three-dimensional views without being as familiar as
//! MATLAB's `peaks`. The image figures use the complex iterate itself, a quantised form of the field and a colour
//! conversion for domain colouring.

use ironlab::prelude::*;

/// The real part of the constant `c` of the Julia map `z ↦ z² + c`.
pub const JULIA_C_RE: f64 = -0.8;

/// The imaginary part of the constant `c` of the Julia map `z ↦ z² + c`.
pub const JULIA_C_IM: f64 = 0.156;

/// The number of times the Julia map is applied to each point.
pub const JULIA_ITERATIONS: usize = 3;

/// The lower bound of both coordinates of the domain sampled by [`julia_grid`].
pub const DOMAIN_MIN: f64 = -1.5;

/// The upper bound of both coordinates of the domain sampled by [`julia_grid`].
pub const DOMAIN_MAX: f64 = 1.5;

/// Returns the real and imaginary parts of `z₃`, where `z₀ = x + iy` and `zₙ₊₁ = zₙ² + c` with `c = −0.8 + 0.156i`.
///
/// The domain colouring figure maps the argument of `z₃` to the hue of a pixel and its magnitude to the lightness.
#[must_use]
pub fn julia_iterate(x: f64, y: f64) -> (f64, f64) {
    let (mut re, mut im) = (x, y);
    for _ in 0..JULIA_ITERATIONS {
        // (re + i·im)² = re² − im² + 2i·re·im
        (re, im) = (re * re - im * im + JULIA_C_RE, 2.0 * re * im + JULIA_C_IM);
    }
    (re, im)
}

/// Returns `ln(1 + |z₃|)`, the field of the magnitude of the iterate returned by [`julia_iterate`].
///
/// The logarithm compresses the rapid growth of the iterates away from the origin, so that the field stays within
/// a range that a colormap can show.
#[must_use]
pub fn julia_field(x: f64, y: f64) -> f64 {
    let (re, im) = julia_iterate(x, y);
    re.hypot(im).ln_1p()
}

/// Samples [`julia_field`] on an `n` by `n` grid spanning `[−1.5, 1.5]` in both x and y.
///
/// Returns the x coordinates (one per column), the y coordinates (one per row) and the matrix of field values, whose
/// value in row `j` and column `i` belongs to the point `(x[i], y[j])`.
#[must_use]
pub fn julia_grid(n: usize) -> (Vec<f64>, Vec<f64>, Matrix) {
    let x = linspace(DOMAIN_MIN, DOMAIN_MAX, n);
    let y = linspace(DOMAIN_MIN, DOMAIN_MAX, n);
    let z = Matrix::from_fn(y.len(), x.len(), |row, col| julia_field(x[col], y[row]));
    (x, y, z)
}

/// Quantises a field into `count` classes of equal width between its smallest and largest finite values, and returns
/// the class of every value as the index of a colormap entry.
///
/// Class `k` of `count` has the index `k · 255 / (count − 1)`, rounded down, so that the classes are spread over the
/// whole of a 256-entry colormap rather than bunched at its start; the largest value belongs to the last class. A
/// `count` of 0 or 1 puts every value in class 0, as does a field whose finite values are all equal, and a value that
/// is not finite is in class 0.
#[must_use]
pub fn quantise(z: &Matrix, count: usize) -> ByteMatrix {
    let finite = z.values().iter().copied().filter(|v| v.is_finite());
    let (min, max) = finite.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    });
    let last = count.saturating_sub(1);
    ByteMatrix::from_fn(z.rows(), z.cols(), |row, col| {
        let value = z[(row, col)];
        if last == 0 || !value.is_finite() || max <= min {
            return 0;
        }
        let class = (((value - min) / (max - min) * count as f64).floor() as usize).min(last);
        (class * 255 / last) as u8
    })
}

/// Returns the partial derivatives `(∂z/∂x, ∂z/∂y)` of a field sampled on a rectilinear grid, as MATLAB's
/// `gradient` does.
///
/// Interior values use central differences, and values on the edges of the grid use one-sided differences. The
/// coordinates need not be evenly spaced. A dimension with a single sample has a derivative of zero.
#[must_use]
pub fn gradient(x: &[f64], y: &[f64], z: &Matrix) -> (Matrix, Matrix) {
    let (rows, cols) = (z.rows(), z.cols());
    let dzdx = Matrix::from_fn(rows, cols, |row, col| {
        if cols < 2 {
            return 0.0;
        }
        let (lo, hi) = (col.saturating_sub(1), (col + 1).min(cols - 1));
        (z[(row, hi)] - z[(row, lo)]) / (x[hi] - x[lo])
    });
    let dzdy = Matrix::from_fn(rows, cols, |row, col| {
        if rows < 2 {
            return 0.0;
        }
        let (lo, hi) = (row.saturating_sub(1), (row + 1).min(rows - 1));
        (z[(hi, col)] - z[(lo, col)]) / (y[hi] - y[lo])
    });
    (dzdx, dzdy)
}

/// Returns the components `(nx, ny, nz)` of the upward unit normal of the surface of heights `z` at every grid node.
///
/// The normal of the surface `z = f(x, y)` is parallel to `(−∂f/∂x, −∂f/∂y, 1)`; the derivatives are estimated
/// with [`gradient`].
#[must_use]
pub fn surface_normals(x: &[f64], y: &[f64], z: &Matrix) -> (Matrix, Matrix, Matrix) {
    let (dzdx, dzdy) = gradient(x, y, z);
    let length = dzdx.zip_map(&dzdy, |p, q| (p * p + q * q + 1.0).sqrt());
    let nx = dzdx.zip_map(&length, |p, l| -p / l);
    let ny = dzdy.zip_map(&length, |q, l| -q / l);
    let nz = length.map(|l| 1.0 / l);
    (nx, ny, nz)
}

/// Returns the points of a sunflower (Vogel) spiral of `n` points filling a disc of the given radius.
///
/// The points are spread evenly over the disc without any randomness, so figures built from them are reproducible.
#[must_use]
pub fn sunflower(n: usize, radius: f64) -> (Vec<f64>, Vec<f64>) {
    let golden_angle = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    (0..n)
        .map(|i| {
            let r = radius * ((i as f64 + 0.5) / n as f64).sqrt();
            let theta = i as f64 * golden_angle;
            (r * theta.cos(), r * theta.sin())
        })
        .unzip()
}

/// Returns the opaque colour with the given hue, saturation and lightness (HSL).
///
/// The hue is measured in turns, so that 0, ⅓ and ⅔ are red, green and blue, and a value outside one turn is wrapped
/// onto the colour wheel, as the argument of a complex number divided by 2π needs. The saturation and the lightness
/// run from 0 to 1: a lightness of 0 is black, ½ is the pure hue at full saturation and 1 is white.
#[must_use]
pub fn hsl_to_rgb(hue_turns: f64, saturation: f64, lightness: f64) -> Color {
    // The chroma is the spread between the largest and smallest components; the hue picks which sextant of the
    // wheel supplies the middle component, and the offset lifts every component to the requested lightness.
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sextant = hue_turns.rem_euclid(1.0) * 6.0;
    let middle = chroma * (1.0 - (sextant % 2.0 - 1.0).abs());
    let (r, g, b) = match sextant as usize {
        0 => (chroma, middle, 0.0),
        1 => (middle, chroma, 0.0),
        2 => (0.0, chroma, middle),
        3 => (0.0, middle, chroma),
        4 => (middle, 0.0, chroma),
        _ => (chroma, 0.0, middle),
    };
    let offset = lightness - chroma / 2.0;
    Color::rgb(
        (r + offset) as f32,
        (g + offset) as f32,
        (b + offset) as f32,
    )
}

/// The source text of this module, shown on the gallery's data helpers page.
pub const FIELDS_SOURCE: &str = include_str!("fields.rs");
