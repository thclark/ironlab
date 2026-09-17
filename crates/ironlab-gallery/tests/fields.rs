//! Tests of the shared data helpers.
//!
//! WHY: every gridded gallery figure is drawn from these helpers, and the data helpers page documents them. A wrong
//! field would make the figures misrepresent the formula stated in the documentation, and a wrong gradient or normal
//! would draw arrows that do not mean what their captions say.

use ironlab::Matrix;
use ironlab_gallery::fields::{
    DOMAIN_MAX, DOMAIN_MIN, FIELDS_SOURCE, gradient, julia_field, julia_grid, sunflower,
    surface_normals,
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
