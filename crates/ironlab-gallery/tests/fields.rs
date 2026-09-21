//! Tests of the shared data helpers.
//!
//! WHY: every gridded gallery figure is drawn from these helpers, and the data helpers page documents them. A wrong
//! field would make the figures misrepresent the formula stated in the documentation, and a wrong gradient or normal
//! would draw arrows that do not mean what their captions say.

use ironlab::{ByteMatrix, Color, Matrix};
use ironlab_gallery::fields::{
    DOMAIN_MAX, DOMAIN_MIN, FIELDS_SOURCE, blob, gradient, hsl_to_rgb, julia_field, julia_grid,
    julia_iterate, quantise, sunflower, surface_normals,
};

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// WHY: the field must match the documented formula ln(1 + |z₃|) with z₀ = x + iy, zₙ₊₁ = zₙ² + c and
/// c = −0.8 + 0.156i. The expected values were computed independently with Python's complex type:
///
/// ```python
/// import math
/// def f(x, y):
///     z = complex(x, y)
///     for _ in range(3):
///         z = z * z + complex(-0.8, 0.156)
///     return math.log1p(abs(z))
/// ```
#[test]
fn julia_field_matches_independently_computed_values() {
    for (x, y, expected) in [
        (0.0, 0.0, 0.586_596_549_475_858_3),
        (0.5, -0.25, 0.524_730_216_890_159_8),
        (1.5, 1.5, 6.273_940_916_134_951_5),
    ] {
        let actual = julia_field(x, y);
        assert!(
            close(actual, expected, 1e-12),
            "julia_field({x}, {y}) = {actual}, expected {expected}"
        );
    }
}

/// WHY: the domain colouring figure colours each pixel by the argument and the magnitude of z₃ itself, not of the
/// field, so the iterate must be the complex number whose magnitude the field compresses. The expected parts were
/// computed with the Python loop above, without the final `log1p(abs(z))`.
#[test]
fn julia_iterate_returns_the_parts_of_the_third_iterate() {
    for (x, y, re, im) in [
        (0.0, 0.0, -0.774_781_199_104_000_1, 0.190_507_699_2),
        (0.5, -0.25, -0.685_444_196_939_937_5, -0.079_184_528_425),
        (-1.0, 0.75, 4.047_473_107_435_064, -5.439_321_178_800_001),
    ] {
        let (actual_re, actual_im) = julia_iterate(x, y);
        assert!(
            close(actual_re, re, 1e-12) && close(actual_im, im, 1e-12),
            "julia_iterate({x}, {y}) = ({actual_re}, {actual_im}), expected ({re}, {im})"
        );
    }
    for (x, y) in [(0.3, 0.7), (-1.2, 0.4), (1.5, 1.5)] {
        let (re, im) = julia_iterate(x, y);
        assert!(
            close(re.hypot(im).ln_1p(), julia_field(x, y), 1e-12),
            "the field at ({x}, {y}) is not ln(1 + |z₃|) of the iterate"
        );
    }
}

/// WHY: the domain colouring maps the argument of z₃ to the hue and its magnitude to the lightness, so the
/// conversion must place the primary hues where the argument lands them, wrap a hue outside one turn onto the
/// wheel (the argument of a complex number spans one turn, but ends negative) and run from black through the pure
/// hue to white as the lightness rises, or the picture would misrepresent the arguments and magnitudes it encodes.
#[test]
fn hsl_to_rgb_places_the_primaries_wraps_the_hue_and_spans_black_to_white() {
    let rgb = |c: Color| (c.r, c.g, c.b);
    assert_eq!(
        rgb(hsl_to_rgb(0.0, 1.0, 0.5)),
        (1.0, 0.0, 0.0),
        "hue 0 is red"
    );
    assert_eq!(
        rgb(hsl_to_rgb(1.0 / 3.0, 1.0, 0.5)),
        (0.0, 1.0, 0.0),
        "one third of a turn is green"
    );
    assert_eq!(
        rgb(hsl_to_rgb(2.0 / 3.0, 1.0, 0.5)),
        (0.0, 0.0, 1.0),
        "two thirds of a turn is blue"
    );
    assert_eq!(
        rgb(hsl_to_rgb(1.0 / 6.0, 1.0, 0.5)),
        (1.0, 1.0, 0.0),
        "a sixth of a turn is yellow"
    );
    assert_eq!(
        rgb(hsl_to_rgb(-1.0 / 3.0, 1.0, 0.5)),
        rgb(hsl_to_rgb(2.0 / 3.0, 1.0, 0.5)),
        "a negative hue wraps onto the wheel"
    );
    assert_eq!(
        rgb(hsl_to_rgb(1.25, 1.0, 0.5)),
        rgb(hsl_to_rgb(0.25, 1.0, 0.5)),
        "a hue beyond one turn wraps onto the wheel"
    );
    assert_eq!(
        rgb(hsl_to_rgb(0.4, 1.0, 0.0)),
        (0.0, 0.0, 0.0),
        "lightness 0 is black"
    );
    assert_eq!(
        rgb(hsl_to_rgb(0.4, 1.0, 1.0)),
        (1.0, 1.0, 1.0),
        "lightness 1 is white"
    );
    assert_eq!(
        rgb(hsl_to_rgb(0.4, 0.0, 0.25)),
        (0.25, 0.25, 0.25),
        "saturation 0 is a grey of the lightness"
    );
    let pale_red = hsl_to_rgb(0.0, 1.0, 0.75);
    assert_eq!(pale_red.r, 1.0);
    assert!(
        close(f64::from(pale_red.g), 0.5, 1e-6) && close(f64::from(pale_red.b), 0.5, 1e-6),
        "lightness above one half tints towards white: {pale_red:?}"
    );
    for hue in [0.0, 0.1, 0.5, 0.9] {
        let c = hsl_to_rgb(hue, 1.0, 0.5);
        assert_eq!(c.a, 1.0, "the colour is opaque");
        assert!(
            [c.r, c.g, c.b].iter().all(|v| (0.0..=1.0).contains(v)),
            "components stay within 0 to 1: {c:?}"
        );
    }
}

/// WHY: the indexed image entry stores the classes of the field as bytes that index the colormap directly, so the
/// quantiser must spread the classes over the whole table (class k of n at k · 255 / (n − 1)), keep them monotone in
/// the value, and put the extremes of the field in the first and last classes; a quantiser that bunched the classes
/// at one end would draw an almost uniform image.
#[test]
fn quantise_spreads_the_classes_over_the_colormap_in_order() {
    let z = Matrix::from_fn(4, 4, |row, col| (row * 4 + col) as f64 / 15.0);
    let classes = quantise(&z, 8);
    assert_eq!((classes.rows(), classes.cols()), (4, 4));
    let expected_indices = [0, 36, 72, 109, 145, 182, 218, 255];
    for index in classes.values() {
        assert!(
            expected_indices.contains(index),
            "{index} is not the index of one of eight classes"
        );
    }
    assert_eq!(
        classes[(0, 0)],
        0,
        "the smallest value is in the first class"
    );
    assert_eq!(
        classes[(3, 3)],
        255,
        "the largest value is in the last class"
    );
    assert!(
        classes.values().windows(2).all(|pair| pair[0] <= pair[1]),
        "classes are monotone in the value: {:?}",
        classes.values()
    );
    let distinct: std::collections::BTreeSet<u8> = classes.values().iter().copied().collect();
    assert_eq!(
        distinct.len(),
        8,
        "sixteen evenly spaced values fill eight classes"
    );

    // Two classes are the two ends of the colormap; a single class is the first entry.
    assert_eq!(
        quantise(&z, 2).values(),
        &[0; 8].into_iter().chain([255; 8]).collect::<Vec<u8>>()[..]
    );
    assert_eq!(quantise(&z, 1), ByteMatrix::zeros(4, 4));

    // The gallery field is what is actually quantised, so its classes must reach both ends of the colormap.
    let (_, _, field) = julia_grid(41);
    let field_classes = quantise(&field, 8);
    assert_eq!(field_classes.values().iter().min(), Some(&0));
    assert_eq!(field_classes.values().iter().max(), Some(&255));
}

/// WHY: z ↦ z² + c maps z and −z to the same value, so the field has point symmetry about the origin. This checks
/// the iteration order independently of the reference values.
#[test]
fn julia_field_is_symmetric_under_negation() {
    for (x, y) in [(0.3, 0.7), (-1.2, 0.4), (1.1, -1.4)] {
        assert!(close(julia_field(x, y), julia_field(-x, -y), 1e-12));
    }
}

/// WHY: non-finite values would break contouring and colour mapping, and a constant field would draw empty contour
/// plots, so the grid must be finite and vary.
#[test]
fn julia_grid_is_finite_and_not_trivial() {
    let (x, y, z) = julia_grid(61);
    assert_eq!((x.len(), y.len()), (61, 61));
    assert_eq!((z.rows(), z.cols()), (61, 61));
    assert!(
        z.values().iter().all(|v| v.is_finite()),
        "the field has non-finite values"
    );
    let min = z.values().iter().copied().fold(f64::INFINITY, f64::min);
    let max = z.values().iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(min < max, "the field is constant ({min})");
    assert!(
        max - min > 1.0,
        "the field varies too little to show structure: {min}..{max}"
    );
}

/// WHY: the documented domain is [−1.5, 1.5]², and rows must follow y and columns x, as in MATLAB.
#[test]
fn julia_grid_spans_the_domain_with_rows_along_y() {
    let (x, y, z) = julia_grid(31);
    assert_eq!(
        (x[0], x[30], y[0], y[30]),
        (DOMAIN_MIN, DOMAIN_MAX, DOMAIN_MIN, DOMAIN_MAX)
    );
    assert!(close(z[(3, 17)], julia_field(x[17], y[3]), 0.0));
    assert!(close(z[(22, 5)], julia_field(x[5], y[22]), 0.0));
}

/// WHY: the image planes entry draws cross-sections of this blob, and its page states the formula, so the blob must
/// be the Gaussian exp(−r² / (2σ²)) of unit peak at the centre of the unit cube with σ = ¼: its peak is 1 at the
/// centre; it depends on the distance from the centre alone, so it is unchanged by reflecting any coordinate about
/// the centre and by permuting the coordinates; and at a known distance it takes the documented value, exp(−½) a
/// quarter of a unit away, exp(−2) half a unit away at the centre of a face and exp(−6) at a corner of the cube.
#[test]
fn blob_is_a_unit_gaussian_centred_in_the_unit_cube() {
    assert_eq!(blob(0.5, 0.5, 0.5), 1.0, "the peak is 1 at the centre");

    let quarter = (-0.5_f64).exp();
    for (x, y, z) in [(0.75, 0.5, 0.5), (0.5, 0.25, 0.5), (0.5, 0.5, 0.75)] {
        let actual = blob(x, y, z);
        assert!(
            close(actual, quarter, 1e-12),
            "blob({x}, {y}, {z}) = {actual}, expected {quarter} a quarter of a unit from the centre"
        );
    }
    let step = 0.25 / 3.0_f64.sqrt();
    let diagonal = blob(0.5 + step, 0.5 - step, 0.5 + step);
    assert!(
        close(diagonal, quarter, 1e-12),
        "a quarter of a unit along a diagonal gives {diagonal}, expected {quarter}"
    );
    let half = (-2.0_f64).exp();
    for (x, y, z) in [(0.5, 0.5, 0.0), (1.0, 0.5, 0.5), (0.5, 0.0, 0.5)] {
        let actual = blob(x, y, z);
        assert!(
            close(actual, half, 1e-12),
            "blob({x}, {y}, {z}) = {actual}, expected {half} at the centre of a face"
        );
    }
    let corner = blob(0.0, 1.0, 0.0);
    assert!(
        close(corner, (-6.0_f64).exp(), 1e-12),
        "a corner of the cube gives {corner}, expected exp(−6)"
    );

    for (x, y, z) in [(0.1, 0.7, 0.4), (0.9, 0.2, 0.55)] {
        let value = blob(x, y, z);
        for (rx, ry, rz) in [(1.0 - x, y, z), (x, 1.0 - y, z), (x, y, 1.0 - z)] {
            assert!(
                close(blob(rx, ry, rz), value, 1e-12),
                "reflecting ({x}, {y}, {z}) about the centre to ({rx}, {ry}, {rz}) changes the blob"
            );
        }
        for (px, py, pz) in [(y, z, x), (z, x, y), (y, x, z)] {
            assert!(
                close(blob(px, py, pz), value, 1e-12),
                "permuting ({x}, {y}, {z}) to ({px}, {py}, {pz}) changes the blob"
            );
        }
    }
}

/// WHY: the quiver entry draws this gradient. For a quadratic field, central differences are exact in the interior
/// and one-sided differences are exact for a linear field at the edges, which pins down both branches, including
/// uneven spacing.
#[test]
fn gradient_is_exact_for_simple_fields() {
    let x = vec![0.0, 0.5, 1.5, 3.0];
    let y = vec![-1.0, 0.0, 2.0];
    let linear = Matrix::from_fn(y.len(), x.len(), |r, c| 2.0 * x[c] - 3.0 * y[r] + 1.0);
    let (dzdx, dzdy) = gradient(&x, &y, &linear);
    for r in 0..y.len() {
        for c in 0..x.len() {
            assert!(
                close(dzdx[(r, c)], 2.0, 1e-12),
                "∂z/∂x at ({r}, {c}) is {}",
                dzdx[(r, c)]
            );
            assert!(
                close(dzdy[(r, c)], -3.0, 1e-12),
                "∂z/∂y at ({r}, {c}) is {}",
                dzdy[(r, c)]
            );
        }
    }

    let even = vec![-1.0, 0.0, 1.0, 2.0];
    let quadratic = Matrix::from_fn(even.len(), even.len(), |r, c| {
        even[c] * even[c] + even[c] * even[r]
    });
    let (dzdx, dzdy) = gradient(&even, &even, &quadratic);
    for r in 1..3 {
        for c in 1..3 {
            let (x, y) = (even[c], even[r]);
            assert!(close(dzdx[(r, c)], 2.0 * x + y, 1e-12));
            assert!(close(dzdy[(r, c)], x, 1e-12));
        }
    }
}

/// WHY: the quiver3 entry draws these normals; they must be unit vectors perpendicular to the surface and point
/// upwards. For a plane z = ax + by the normal is (−a, −b, 1)/√(a² + b² + 1) everywhere.
#[test]
fn surface_normals_of_a_plane_are_its_unit_normal() {
    let x = vec![0.0, 1.0, 2.0, 3.0];
    let y = vec![0.0, 0.5, 1.0];
    let (a, b) = (0.5, -2.0);
    let z = Matrix::from_fn(y.len(), x.len(), |r, c| a * x[c] + b * y[r]);
    let (nx, ny, nz) = surface_normals(&x, &y, &z);
    let length = (a * a + b * b + 1.0_f64).sqrt();
    for r in 0..y.len() {
        for c in 0..x.len() {
            assert!(close(nx[(r, c)], -a / length, 1e-12));
            assert!(close(ny[(r, c)], -b / length, 1e-12));
            assert!(close(nz[(r, c)], 1.0 / length, 1e-12));
        }
    }
}

/// WHY: the scatter entries rely on the spiral filling the disc of the requested radius with the requested count.
#[test]
fn sunflower_fills_a_disc() {
    let (x, y) = sunflower(500, 1.5);
    assert_eq!((x.len(), y.len()), (500, 500));
    let radii: Vec<f64> = x.iter().zip(&y).map(|(x, y)| x.hypot(*y)).collect();
    assert!(
        radii.iter().all(|&r| r <= 1.5),
        "a point lies outside the disc"
    );
    assert!(
        radii.iter().any(|&r| r > 1.4),
        "the spiral does not reach the edge of the disc"
    );
    assert!(
        radii.iter().any(|&r| r < 0.1),
        "the spiral does not reach the centre of the disc"
    );
}

/// WHY: the data helpers page shows this constant, so it must be the module's actual source.
#[test]
fn fields_source_is_the_file_on_disk() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fields.rs");
    assert_eq!(
        FIELDS_SOURCE,
        std::fs::read_to_string(path).expect("fields.rs is readable")
    );
}
