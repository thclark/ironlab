//! Automatic scaling and arrow geometry for vector field plots.

/// The angle between each arrow-head barb and the shaft, in degrees.
pub const BARB_ANGLE_DEG: f64 = 20.0;

/// The geometry of one arrow, in the same units as the positions it was built from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arrow {
    /// The shaft from the base to the tip.
    pub shaft: [[f64; 3]; 2],
    /// The head as an open polyline: the left barb end, the tip, and the right barb end.
    pub head: [[f64; 3]; 3],
}

/// Computes the factor by which vectors are multiplied so that the longest arrow is 0.9 times
/// the typical spacing between arrow bases.
///
/// The typical spacing is estimated from the axis-aligned bounding box of the finite positions
/// (those whose coordinates are all finite).
/// Let `d` be the number of axes along which the box has a non-zero extent, `V` the product of
/// those extents and `n` the number of finite positions. The spacing is
/// `V^(1/d) / max(n^(1/d) - 1, 1)`, which is exactly the node spacing for a square (or cubic)
/// regular grid. For a rectangular grid with the same spacing along every axis it never exceeds
/// that spacing (by the inequality of arithmetic and geometric means), so arrows stay inside
/// their cells. For a grid whose spacing differs between axes it is a compromise, and arrows
/// along the finer axis may reach into the neighbouring cell. If all bases coincide (`d == 0`), the spacing is 1.
///
/// The factor is `0.9 · spacing / L`, where `L` is the largest length among the finite vectors.
/// If there are no bases, or `L` is zero or not finite, the factor is 1.
///
/// MATLAB's `quiver` instead measures spacing along the cell diagonal, which lets arrows along
/// the grid axes overlap their neighbours' bases; this function keeps every arrow inside its own
/// cell.
///
/// Vectors are paired with positions by index; entries beyond the shorter slice are ignored.
pub fn auto_scale(positions: &[[f64; 3]], vectors: &[[f64; 3]]) -> f64 {
    let n = positions.len().min(vectors.len());
    let (positions, vectors) = (&positions[..n], &vectors[..n]);

    let longest = vectors
        .iter()
        .filter(|v| v.iter().all(|c| c.is_finite()))
        .map(|v| length(*v))
        .fold(0.0, f64::max);
    if !(longest > 0.0 && longest.is_finite()) {
        return 1.0;
    }

    let mut count = 0usize;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in positions.iter().filter(|p| p.iter().all(|c| c.is_finite())) {
        count += 1;
        for axis in 0..3 {
            lo[axis] = lo[axis].min(p[axis]);
            hi[axis] = hi[axis].max(p[axis]);
        }
    }
    if count == 0 {
        return 1.0;
    }

    let extents: Vec<f64> = (0..3).map(|a| hi[a] - lo[a]).filter(|e| *e > 0.0).collect();
    let spacing = if extents.is_empty() {
        1.0
    } else {
        let dims = extents.len() as f64;
        let volume: f64 = extents.iter().product();
        volume.powf(1.0 / dims) / ((count as f64).powf(1.0 / dims) - 1.0).max(1.0)
    };
    0.9 * spacing / longest
}

/// Builds the arrow for `vector` drawn from `base`.
///
/// The tip is `base + scale · vector`. The head length `h` is `head_fraction` times the arrow
/// length `|scale · vector|`. With `d` the unit arrow direction and `p` a unit vector
/// perpendicular to `d`, the barb ends are `tip - h·cos(20°)·d ± h·sin(20°)·p`, the left barb
/// taking the `+` sign.
///
/// `p` is the horizontal unit vector `normalise(ẑ × d)`, so the head lies in the plane spanned
/// by the arrow and the horizontal direction perpendicular to it. For a two-dimensional vector
/// (zero z component) this is the x–y plane and "left" is the counter-clockwise side of the
/// arrow. For a vertical vector, where `|ẑ × d| < 1e-12`, `p` is `x̂`, so the head lies in the
/// x–z plane.
///
/// A zero-length arrow (zero vector or zero scale) has every point equal to `base`, and every
/// coordinate is finite provided the inputs are.
pub fn arrow(base: [f64; 3], vector: [f64; 3], scale: f64, head_fraction: f64) -> Arrow {
    let scaled = vector.map(|c| scale * c);
    let len = length(scaled);
    if len == 0.0 {
        return Arrow {
            shaft: [base; 2],
            head: [base; 3],
        };
    }
    let tip = [0, 1, 2].map(|a| base[a] + scaled[a]);
    let d = scaled.map(|c| c / len);

    // ẑ × d = (−d_y, d_x, 0).
    let horizontal = d[0].hypot(d[1]);
    let p = if horizontal < 1e-12 {
        [1.0, 0.0, 0.0]
    } else {
        [-d[1] / horizontal, d[0] / horizontal, 0.0]
    };

    let h = head_fraction * len;
    let (back, side) = (
        h * BARB_ANGLE_DEG.to_radians().cos(),
        h * BARB_ANGLE_DEG.to_radians().sin(),
    );
    let barb = |sign: f64| [0, 1, 2].map(|a| tip[a] - back * d[a] + sign * side * p[a]);
    Arrow {
        shaft: [base, tip],
        head: [barb(1.0), tip, barb(-1.0)],
    }
}

/// Euclidean length, computed with `hypot` so that tiny or large components neither underflow
/// nor overflow in the intermediate squares.
fn length(v: [f64; 3]) -> f64 {
    v[0].hypot(v[1]).hypot(v[2])
}
