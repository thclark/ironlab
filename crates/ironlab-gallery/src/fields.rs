//! Data shared by the gallery figures.
//!
//! The gridded figures plot the same scalar field, derived from a few iterations of the quadratic map that defines
//! a Julia set. The field is smooth near the origin and grows rapidly towards the corners of the domain, so it has
//! enough structure to exercise contouring, colour mapping and three-dimensional views without being as familiar as
//! MATLAB's `peaks`. The image figures use the complex iterate itself, a quantised form of the field and a colour
//! conversion for domain colouring, the cylinder flow figure plots the speed of a potential flow on a polar mesh, and
//! the correlation peak figure samples a synthetic cross-correlation volume, a field of three variables with one
//! dominant peak at the centre of the unit cube and weaker peaks around it, on planes of the cube.

use std::ops::Range;

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

/// The dominant peak of the field returned by [`correlation_field`]: a Gaussian of unit peak at the centre of the
/// unit cube with standard deviation 0.1, so that it falls to 0.61 a tenth of a unit from the centre and to 0.04 a
/// quarter of a unit away, the closest that a noise peak may lie.
pub const CENTRAL_PEAK: Peak = Peak {
    centre: [0.5, 0.5, 0.5],
    sigma: 0.1,
    amplitude: 1.0,
};

/// The number of noise peaks returned by [`noise_peaks`].
pub const NOISE_PEAK_COUNT: usize = 16;

/// The seed of the generator that draws the noise peaks. Any seed gives a valid field; this one was chosen by
/// looking at the figure, so that the floor shows several bright peaks spread across it.
pub const NOISE_SEED: u64 = 170;

/// The range from which the standard deviation of a noise peak is drawn.
pub const NOISE_SIGMA: Range<f64> = 0.04..0.12;

/// The range from which the amplitude of a noise peak is drawn: from 0.6 to 1.4 times a quarter of the amplitude of
/// [`CENTRAL_PEAK`].
pub const NOISE_AMPLITUDE: Range<f64> = 0.15..0.35;

/// The least distance of the centre of a noise peak from the centre of [`CENTRAL_PEAK`].
pub const NOISE_CLEARANCE: f64 = 0.25;

/// The range of heights above the floor of the cube from which the `z` of every third noise peak is drawn.
pub const FLOOR_PEAK_HEIGHT: Range<f64> = 0.0..0.08;

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

/// One Gaussian peak of the field returned by [`correlation_field`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Peak {
    /// The point at which the peak takes its amplitude.
    pub centre: [f64; 3],
    /// The standard deviation `σ` of the peak, which sets its width.
    pub sigma: f64,
    /// The value of the peak at its centre.
    pub amplitude: f64,
}

impl Peak {
    /// Returns `amplitude · exp(−r² / (2σ²))`, the value of the peak at the point `(x, y, z)`, where `r` is the
    /// distance of the point from the centre of the peak.
    #[must_use]
    pub fn at(&self, x: f64, y: f64, z: f64) -> f64 {
        let r = distance([x, y, z], self.centre);
        self.amplitude * (-r * r / (2.0 * self.sigma * self.sigma)).exp()
    }
}

/// Returns the distance between the points `a` and `b`.
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(&b)
        .map(|(p, q)| (p - q).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// SplitMix64, the pseudo-random generator that draws the noise peaks.
///
/// Each step adds an odd constant to the state, which therefore takes every value once before repeating, and mixes
/// a copy of the state with two rounds of a shift, an exclusive or and a multiplication, which spread a change in
/// any bit of the state over every bit of the output. The generator is used because its outputs are well spread
/// and its whole definition is these few lines, so a reader can reproduce the peaks from the seed alone.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Returns the next output of the generator.
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a number drawn uniformly from `[0, 1)`: the top 53 bits of the next output, which are as many as the
    /// mantissa of an `f64` holds, divided by 2⁵³.
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Returns a number drawn uniformly from `range`.
    fn draw(&mut self, range: Range<f64>) -> f64 {
        range.start + (range.end - range.start) * self.unit()
    }
}

/// Returns the noise peaks of [`correlation_field`]: [`NOISE_PEAK_COUNT`] Gaussians, weaker and mostly narrower than
/// [`CENTRAL_PEAK`], drawn by [`noise_peaks_from`] with the seed [`NOISE_SEED`], so that they are the same on every
/// run.
#[must_use]
pub fn noise_peaks() -> Vec<Peak> {
    noise_peaks_from(NOISE_SEED)
}

/// Returns the [`NOISE_PEAK_COUNT`] noise peaks that the SplitMix64 generator above draws from the given seed.
///
/// For each peak the generator draws, in this order, the three coordinates of the centre uniformly over the unit
/// cube, then the standard deviation uniformly from [`NOISE_SIGMA`], then the amplitude uniformly from
/// [`NOISE_AMPLITUDE`]. Every third peak, counting from the first, has its `z` drawn from [`FLOOR_PEAK_HEIGHT`]
/// instead, so that the floor of the cube shows several peaks. A centre within [`NOISE_CLEARANCE`] of the dominant
/// peak is discarded and all three coordinates are drawn again, so that the dominant peak stays clean.
#[must_use]
pub fn noise_peaks_from(seed: u64) -> Vec<Peak> {
    let mut generator = SplitMix64 { state: seed };
    (0..NOISE_PEAK_COUNT)
        .map(|index| {
            let centre = loop {
                let x = generator.unit();
                let y = generator.unit();
                let z = if index % 3 == 0 {
                    generator.draw(FLOOR_PEAK_HEIGHT)
                } else {
                    generator.unit()
                };
                if distance([x, y, z], CENTRAL_PEAK.centre) >= NOISE_CLEARANCE {
                    break [x, y, z];
                }
            };
            let sigma = generator.draw(NOISE_SIGMA);
            let amplitude = generator.draw(NOISE_AMPLITUDE);
            Peak {
                centre,
                sigma,
                amplitude,
            }
        })
        .collect()
}

/// Returns the value at `(x, y, z)` of a synthetic cross-correlation volume: the sum of [`CENTRAL_PEAK`] and the
/// [`noise_peaks`].
///
/// A cross-correlation volume from three-dimensional particle image velocimetry has one dominant peak, at the
/// displacement of the particles between two exposures, among weaker peaks from chance alignments of other
/// particles. Every peak is positive, so the field is never negative, and at the centre of a peak it is at least
/// that peak's amplitude.
#[must_use]
pub fn correlation_field(x: f64, y: f64, z: f64) -> f64 {
    CENTRAL_PEAK.at(x, y, z)
        + noise_peaks()
            .iter()
            .map(|peak| peak.at(x, y, z))
            .sum::<f64>()
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

/// Returns the speed, at the radius `r` and the angle `theta` in radians, of the potential flow of an ideal fluid
/// past a circular cylinder of unit radius centred on the origin, as a multiple of the speed of the free stream.
///
/// The free stream flows along x, from which `theta` is measured. The speed `q` satisfies
/// `q² = 1 − 2 cos(2θ) / r² + 1 / r⁴`. It is zero at the two stagnation points, where the x axis meets the cylinder,
/// twice the free-stream speed at the top and bottom of the cylinder, and tends to the free-stream speed far from
/// the cylinder. A point inside the cylinder holds no fluid, so its speed is NaN.
#[must_use]
pub fn cylinder_flow_speed(r: f64, theta: f64) -> f64 {
    if r < 1.0 {
        return f64::NAN;
    }
    let r2 = r * r;
    // Rounding can leave the square of the speed at a stagnation point negative by a few parts in 10¹⁶.
    (1.0 - 2.0 * (2.0 * theta).cos() / r2 + 1.0 / (r2 * r2))
        .max(0.0)
        .sqrt()
}

/// Returns a polar mesh around the unit cylinder, as the matrices of the x and y coordinates of its nodes, with the
/// [`cylinder_flow_speed`] at every node.
///
/// Row `j` of each matrix is the ring of radius `outer_radius^(j / (rings − 1))`, so the rings run from the surface
/// of the cylinder to `outer_radius` and their spacing grows in proportion to their radius, which keeps the cells
/// nearly square and makes them smallest where the speed changes fastest. Column `i` is the spoke at the angle
/// `2π i / (spokes − 1)`, so the first and last spokes coincide and the mesh closes round the cylinder.
#[must_use]
pub fn cylinder_flow_mesh(
    rings: usize,
    spokes: usize,
    outer_radius: f64,
) -> (Matrix, Matrix, Matrix) {
    let radius = |row: usize| outer_radius.powf(row as f64 / (rings - 1).max(1) as f64);
    let angle = |col: usize| std::f64::consts::TAU * col as f64 / (spokes - 1).max(1) as f64;
    let x = Matrix::from_fn(rings, spokes, |row, col| radius(row) * angle(col).cos());
    let y = Matrix::from_fn(rings, spokes, |row, col| radius(row) * angle(col).sin());
    let speed = Matrix::from_fn(rings, spokes, |row, col| {
        cylinder_flow_speed(radius(row), angle(col))
    });
    (x, y, speed)
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
