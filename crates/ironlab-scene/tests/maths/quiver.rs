use ironlab_scene::maths::quiver::{Arrow, BARB_ANGLE_DEG, arrow, auto_scale};

use crate::assert_close;

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn all_points(a: &Arrow) -> impl Iterator<Item = [f64; 3]> + '_ {
    a.shaft.iter().chain(a.head.iter()).copied()
}

fn unit_grid_2d(n: usize) -> Vec<[f64; 3]> {
    (0..n)
        .flat_map(|j| (0..n).map(move |i| [i as f64, j as f64, 0.0]))
        .collect()
}

// Why: MATLAB-style automatic scaling must make the longest arrow nearly span one grid cell
// (0.9 of the spacing) regardless of the data's units, so arrows are legible without
// overlapping their neighbours.
#[test]
fn auto_scale_fits_longest_arrow_to_grid_spacing() {
    let positions = unit_grid_2d(10);
    let vectors: Vec<[f64; 3]> = positions
        .iter()
        .map(|p| [2.0 * p[0] / 9.0, 0.0, 0.0])
        .collect();
    let scale = auto_scale(&positions, &vectors);
    assert_close(scale, 0.45, 1e-12);
    assert_close(scale * 2.0, 0.9, 1e-12);
}

// Why: on a rectangular (non-square) grid of unit spacing the longest arrow must still stay
// inside its cell, never overlapping the neighbouring base, while not shrinking so far that the
// arrows become illegible.
#[test]
fn auto_scale_keeps_arrows_within_cells_on_rectangular_grids() {
    let positions: Vec<[f64; 3]> = (0..5)
        .flat_map(|j| (0..20).map(move |i| [i as f64, j as f64, 0.0]))
        .collect();
    let vectors = vec![[1.0, 0.0, 0.0]; positions.len()];
    let scale = auto_scale(&positions, &vectors);
    assert!(scale <= 0.9 + 1e-12 && scale > 0.8, "scale {scale}");
}

// Why: quiver3 uses the same rule in three dimensions, with volume in place of area.
#[test]
fn auto_scale_uses_volume_for_3d_lattices() {
    let positions: Vec<[f64; 3]> = (0..5)
        .flat_map(|k| {
            (0..5).flat_map(move |j| {
                (0..5).map(move |i| [2.0 * i as f64, 2.0 * j as f64, 2.0 * k as f64])
            })
        })
        .collect();
    let vectors = vec![[0.0, 0.0, 1.0]; positions.len()];
    assert_close(auto_scale(&positions, &vectors), 1.8, 1e-9);
}

// Why: bases along a line (a 1D transect) have zero area; the estimate must fall back to the
// extent along the line rather than dividing by zero.
#[test]
fn auto_scale_handles_collinear_bases() {
    let positions: Vec<[f64; 3]> = (0..11).map(|i| [i as f64, 3.0, 0.0]).collect();
    let vectors = vec![[0.0, 1.0, 0.0]; positions.len()];
    assert_close(auto_scale(&positions, &vectors), 0.9, 1e-12);
}

// Why: a single arrow has no neighbours; it gets unit spacing so it is still drawn at a
// predictable length.
#[test]
fn auto_scale_single_point_uses_unit_spacing() {
    assert_close(
        auto_scale(&[[4.0, 5.0, 6.0]], &[[3.0, 0.0, 0.0]]),
        0.3,
        1e-12,
    );
}

// Why: an all-zero field or empty data cannot be normalised; the neutral factor 1 avoids NaN or
// infinite geometry.
#[test]
fn auto_scale_of_zero_or_empty_field_is_one() {
    let positions = unit_grid_2d(3);
    assert_eq!(auto_scale(&positions, &vec![[0.0; 3]; 9]), 1.0);
    assert_eq!(auto_scale(&[], &[]), 1.0);
}

// Why: a single NaN vector (missing data) must not wipe out the scaling of the whole plot.
#[test]
fn auto_scale_ignores_non_finite_vectors() {
    let positions = unit_grid_2d(10);
    let mut vectors = vec![[2.0, 0.0, 0.0]; 100];
    let clean = auto_scale(&positions, &vectors);
    vectors[17] = [f64::NAN, 0.0, 0.0];
    vectors[42] = [f64::INFINITY, 0.0, 0.0];
    assert_eq!(auto_scale(&positions, &vectors), clean);
}

// Why: a missing base position must likewise only drop that arrow from the estimate; the scale of
// the remaining 99 arrows barely changes and stays finite.
#[test]
fn auto_scale_ignores_non_finite_positions() {
    let mut positions = unit_grid_2d(10);
    let vectors = vec![[2.0, 0.0, 0.0]; 100];
    positions[55] = [f64::NAN, 5.0, 0.0];
    let scale = auto_scale(&positions, &vectors);
    assert_close(scale, 0.9 * 9.0 / (99f64.sqrt() - 1.0) / 2.0, 1e-12);
}

// Why: the arrow tip is the data statement of the plot (base plus scaled vector); the head must
// meet the shaft exactly at the tip.
#[test]
fn arrow_tip_is_base_plus_scaled_vector() {
    let base = [1.0, -2.0, 0.5];
    let vector = [0.3, 0.4, -1.2];
    let a = arrow(base, vector, 2.5, 0.3);
    let tip = [1.75, -1.0, -2.5];
    for (actual, expected) in a.shaft[1].iter().zip(tip) {
        assert_close(*actual, expected, 1e-12);
    }
    assert_eq!(a.shaft[0], base);
    assert_eq!(a.head[1], a.shaft[1]);
}

// Why: 2D quiver heads must stay in the plot plane, have the requested length, open at the
// documented angle, and put the left barb on the counter-clockwise side.
#[test]
fn arrow_head_geometry_in_2d() {
    let a = arrow([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 2.0, 0.3);
    let tip = a.head[1];
    let (left, right) = (a.head[0], a.head[2]);
    assert!(all_points(&a).all(|p| p[2] == 0.0));
    let h = 0.3 * 2.0;
    let back = [-1.0, 0.0, 0.0];
    let cos = BARB_ANGLE_DEG.to_radians().cos();
    for barb in [left, right] {
        let offset = sub(barb, tip);
        assert_close(norm(offset), h, 1e-12);
        assert_close(dot(offset, back) / h, cos, 1e-12);
    }
    assert!(cross([1.0, 0.0, 0.0], sub(left, tip))[2] > 0.0);
    assert!(cross([1.0, 0.0, 0.0], sub(right, tip))[2] < 0.0);
}

// Why: in 3D the head must be a flat symmetric V about the shaft; its sideways direction is the
// documented horizontal perpendicular, which keeps heads stable as the view rotates.
#[test]
fn arrow_head_geometry_in_3d() {
    let vector = [1.0, 2.0, 3.0];
    let a = arrow([0.0; 3], vector, 1.0, 0.25);
    let tip = a.head[1];
    let d = {
        let n = norm(vector);
        [vector[0] / n, vector[1] / n, vector[2] / n]
    };
    let sideways = sub(a.head[0], a.head[2]);
    assert_close(sideways[2], 0.0, 1e-12);
    assert_close(dot(sideways, d), 0.0, 1e-12);
    let h = 0.25 * norm(vector);
    for barb in [a.head[0], a.head[2]] {
        assert_close(norm(sub(barb, tip)), h, 1e-12);
    }
}

// Why: vertical vectors (surface normals of flat regions in quiver3) have no horizontal
// perpendicular; the fallback must still give a visible, finite head in the x–z plane.
#[test]
fn arrow_head_for_vertical_vector_lies_in_xz_plane() {
    let base = [0.5, 0.25, 0.0];
    let a = arrow(base, [0.0, 0.0, 1.0], 1.0, 0.3);
    assert!(all_points(&a).all(|p| p.iter().all(|c| c.is_finite())));
    assert!(all_points(&a).all(|p| p[1] == base[1]));
    assert!(norm(sub(a.head[0], a.head[2])) > 0.1);

    // A vector that is vertical up to rounding must not normalise a vanishing cross product
    // into noise or NaN.
    let nearly = arrow(base, [1e-200, -1e-200, 1.0], 1.0, 0.3);
    assert!(all_points(&nearly).all(|p| p.iter().all(|c| c.is_finite())));
    assert!(norm(sub(nearly.head[0], nearly.head[2])) > 0.1);
}

// Why: zero vectors are common (stagnation points) and must not emit NaN coordinates, which
// would corrupt the PDF path or tessellation.
#[test]
fn zero_vector_gives_finite_degenerate_arrow() {
    let base = [1.0, 2.0, 3.0];
    for a in [
        arrow(base, [0.0; 3], 1.0, 0.3),
        arrow(base, [1.0, 1.0, 0.0], 0.0, 0.3),
    ] {
        assert!(all_points(&a).all(|p| p == base));
    }
}
