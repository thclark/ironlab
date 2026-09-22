//! Orthographic 3D camera with MATLAB `view(az, el)` conventions.
//!
//! # Conventions
//!
//! Data are first normalised into the unit box `[-0.5, 0.5]³` with [`normalise_box`]. The
//! camera looks at the box centre from the direction given by azimuth `az` and elevation `el`:
//!
//! - `az = 0, el = 0` views from −y towards +y, with +x to the right and +z up.
//! - Increasing `az` rotates the viewpoint counter-clockwise about +z when seen from above, so
//!   `az = 90` views from +x, with +y to the right.
//! - `el = 90` views from +z straight down, with +x to the right and +y up (for `az = 0`).
//! - The default view is `az = -37.5`, `el = 30`.
//!
//! # View matrix
//!
//! With `a = az` and `e = el` in radians, the rows of the view matrix are the screen right
//! vector, the screen up vector and the unit vector pointing from the box centre towards the
//! viewer:
//!
//! ```text
//! right  = ( cos a,          sin a,          0     )
//! up     = (-sin e · sin a,  sin e · cos a,  cos e )
//! toward = ( cos e · sin a, -cos e · cos a,  sin e )
//! ```
//!
//! This is the rotation part of MATLAB's `viewmtx(az, el)`. The rows are orthonormal and
//! `right × up = toward`, so the projection neither distorts lengths within the screen plane nor
//! mirrors the scene. A point `p` projects to `screen = (right · p, up · p)` with y pointing up,
//! and `depth = toward · p`, which is larger for points closer to the viewer.
//!
//! At the default view, `toward ≈ (-0.527, -0.687, 0.500)`: the viewer is on the −x, −y, +z
//! side, so the nearest corner of the box is `(-0.5, -0.5, 0.5)` and the lowest corner on screen
//! is `(-0.5, -0.5, -0.5)`. The +x axis points to the right and slightly up on screen
//! (`≈ (0.793, 0.304)`), and the +y axis points to the left and up (`≈ (-0.609, 0.397)`).

use crate::display::{DepthPlane, Point};

/// The eight corners of the normalised data box `[-0.5, 0.5]³`.
pub const UNIT_BOX_CORNERS: [[f64; 3]; 8] = [
    [-0.5, -0.5, -0.5],
    [0.5, -0.5, -0.5],
    [0.5, 0.5, -0.5],
    [-0.5, 0.5, -0.5],
    [-0.5, -0.5, 0.5],
    [0.5, -0.5, 0.5],
    [0.5, 0.5, 0.5],
    [-0.5, 0.5, 0.5],
];

/// MATLAB's default azimuth for 3D axes, in degrees.
pub const DEFAULT_AZIMUTH_DEG: f64 = -37.5;

/// MATLAB's default elevation for 3D axes, in degrees.
pub const DEFAULT_ELEVATION_DEG: f64 = 30.0;

/// An orthographic camera orientation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Rotation of the viewpoint about +z, in degrees, counter-clockwise seen from above.
    pub azimuth_deg: f64,
    /// Angle of the viewpoint above the x–y plane, in degrees.
    pub elevation_deg: f64,
}

impl Default for Camera {
    /// Returns MATLAB's default 3D view (`az = -37.5`, `el = 30`).
    fn default() -> Self {
        Self {
            azimuth_deg: DEFAULT_AZIMUTH_DEG,
            elevation_deg: DEFAULT_ELEVATION_DEG,
        }
    }
}

/// A point projected by a [`Camera`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projected {
    /// Screen position in normalised box units, with x to the right and y up.
    pub screen: [f64; 2],
    /// Signed distance along the viewing direction; larger values are closer to the viewer.
    pub depth: f64,
}

impl Camera {
    /// Returns the view matrix whose rows are the screen right, screen up and towards-viewer
    /// unit vectors (see the module documentation).
    pub fn view_matrix(&self) -> [[f64; 3]; 3] {
        let (sa, ca) = (
            self.azimuth_deg.to_radians().sin(),
            self.azimuth_deg.to_radians().cos(),
        );
        let (se, ce) = (
            self.elevation_deg.to_radians().sin(),
            self.elevation_deg.to_radians().cos(),
        );
        [
            [ca, sa, 0.0],
            [-se * sa, se * ca, ce],
            [ce * sa, -ce * ca, se],
        ]
    }

    /// Projects a point in normalised box coordinates onto the screen.
    pub fn project(&self, p: [f64; 3]) -> Projected {
        let [right, up, toward] = self.view_matrix();
        let dot = |row: [f64; 3]| row[0] * p[0] + row[1] * p[1] + row[2] * p[2];
        Projected {
            screen: [dot(right), dot(up)],
            depth: dot(toward),
        }
    }
}

/// Maps a data point into the normalised box `[-0.5, 0.5]³`.
///
/// Along each axis, `lo` maps to −0.5 and `hi` maps to +0.5. On an axis flagged in `log`, the
/// mapping is linear in `log10` of the value. Values outside `[lo, hi]` are extrapolated, not
/// clamped, because clipping is the caller's responsibility. A degenerate axis (`lo == hi`) maps
/// every value to 0. On a log axis, a non-positive value, bound or limit gives NaN for that
/// component.
pub fn normalise_box(p: [f64; 3], lo: [f64; 3], hi: [f64; 3], log: [bool; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| {
        let (mut v, mut l, mut h) = (p[axis], lo[axis], hi[axis]);
        if log[axis] {
            if !(v > 0.0 && l > 0.0 && h > 0.0) {
                return f64::NAN;
            }
            (v, l, h) = (v.log10(), l.log10(), h.log10());
        }
        if l == h { 0.0 } else { (v - l) / (h - l) - 0.5 }
    })
}

/// Clamps an elevation to [-90, 90] degrees. NaN becomes the default elevation.
pub fn clamp_elevation(el: f64) -> f64 {
    if el.is_nan() {
        DEFAULT_ELEVATION_DEG
    } else {
        el.clamp(-90.0, 90.0)
    }
}

/// Wraps an azimuth into (-180, 180] degrees. A non-finite azimuth becomes the default azimuth.
pub fn wrap_azimuth(az: f64) -> f64 {
    if !az.is_finite() {
        return DEFAULT_AZIMUTH_DEG;
    }
    // rem_euclid gives [0, 360] (360 only through rounding of tiny negative inputs); values above
    // 180 move down by a full turn, which maps 360 to 0 and keeps 180 itself.
    let wrapped = az.rem_euclid(360.0);
    if wrapped > 180.0 {
        wrapped - 360.0
    } else {
        wrapped
    }
}

/// A face of the normalised data box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Plane {
    /// The face at x = −0.5.
    XMin,
    /// The face at x = +0.5.
    XMax,
    /// The face at y = −0.5.
    YMin,
    /// The face at y = +0.5.
    YMax,
    /// The face at z = −0.5.
    ZMin,
    /// The face at z = +0.5.
    ZMax,
}

/// Returns, for the x, y and z axes in that order, the face of the box farther from the viewer.
///
/// These are the faces that carry grid lines behind the data. For each axis, if the
/// towards-viewer vector has a positive component along that axis the viewer is on the max
/// side, so the min face is at the back, and vice versa. When the component is zero to within
/// 1e-12, both faces are edge-on and the min face is returned, so the floor stays `ZMin` at
/// `el = 0`.
pub fn back_planes(cam: &Camera) -> [Plane; 3] {
    const EDGE_ON: f64 = 1e-12;
    let toward = cam.view_matrix()[2];
    let pick = |component: f64, min: Plane, max: Plane| {
        if component < -EDGE_ON { max } else { min }
    };
    [
        pick(toward[0], Plane::XMin, Plane::XMax),
        pick(toward[1], Plane::YMin, Plane::YMax),
        pick(toward[2], Plane::ZMin, Plane::ZMax),
    ]
}

/// The distance by which a surface face, and an image inside the axes box, are painted behind their geometry, in
/// normalised-box depth units (the box's depth range is at most √3 whatever the view, the zoom or the size of the
/// plot, so the distance means the same in every figure).
///
/// The lines and markers lying on a surface coincide with its faces in depth, and a depth test cannot separate what
/// coincides. Pushing the faces back by this much puts them in front, wherever the face is and however it is
/// inclined, because a line's depth and the face's plane are compared at the same point. The painter's order sorts
/// the face at its mean depth less this distance, so that the two orders agree. Anything within this distance
/// behind a face therefore shows through it, which at a thousandth of the box is invisible in practice.
pub const FACE_DEPTH_BIAS: f64 = 1e-3;

/// The distance by which the edge ring of a surface face is painted in front of the face's fill, in normalised-box
/// depth units.
///
/// The ring and the fill lie on one plane, but a depth buffer interpolates that plane over two different
/// triangulations, and the two interpolations differ by rounding, so a ring at exactly the fill's depth loses the
/// tie along some of its length. Lifting the ring by a hundred times the resolution of a 32-bit depth buffer, and a
/// hundredth of [`FACE_DEPTH_BIAS`], keeps it in front of its own fill and behind everything the bias puts in front
/// of the face.
pub const EDGE_DEPTH_LIFT: f64 = 1e-5;

/// The largest ratio of the determinant of the centred normal equations to the squared spread of the positions at
/// which the positions still count as collinear, so that a face seen edge-on to within rounding gets a constant
/// depth rather than a tilt of the order of the reciprocal of the rounding.
const COLLINEAR_TOLERANCE: f64 = 1e-12;

/// Fits the plane `depth(x, y) = a·x + b·y + c` to points given as `(position, depth)`, by least squares.
///
/// The plane is exact through three non-collinear points and through any coplanar set, which is what the four
/// corners of a planar face are, and the least-squares fit otherwise, which is the best single plane for a twisted
/// face. When the positions are collinear (a face seen edge-on) or there are fewer than three of them (a marker or a
/// segment), the tilt is undetermined and the plane is the constant one at the mean depth. The normal equations are
/// formed about the centroid, so that a face far from the origin loses no precision, and collinearity is judged
/// relative to the spread of the positions ([`COLLINEAR_TOLERANCE`]), so that it does not depend on the figure's
/// scale.
///
/// Returns `None` for no points at all, or for any non-finite coordinate or depth.
pub fn depth_plane(points: &[(Point, f64)]) -> Option<DepthPlane> {
    if points.is_empty()
        || !points
            .iter()
            .all(|(p, d)| p.x.is_finite() && p.y.is_finite() && d.is_finite())
    {
        return None;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|(p, _)| p.x).sum::<f64>() / n;
    let mean_y = points.iter().map(|(p, _)| p.y).sum::<f64>() / n;
    let mean_d = points.iter().map(|(_, d)| d).sum::<f64>() / n;
    if points.len() < 3 {
        return Some(DepthPlane::constant(mean_d));
    }
    let (mut sxx, mut syy, mut sxy, mut sxd, mut syd) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (p, d) in points {
        let (x, y, d) = (p.x - mean_x, p.y - mean_y, d - mean_d);
        sxx += x * x;
        syy += y * y;
        sxy += x * y;
        sxd += x * d;
        syd += y * d;
    }
    let spread = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    if !det.is_finite() || det <= COLLINEAR_TOLERANCE * spread * spread {
        return Some(DepthPlane::constant(mean_d));
    }
    let a = (syy * sxd - sxy * syd) / det;
    let b = (sxx * syd - sxy * sxd) / det;
    let plane = DepthPlane {
        a,
        b,
        c: mean_d - a * mean_x - b * mean_y,
    };
    plane.is_finite().then_some(plane)
}

/// Sorts depth-tagged items back-to-front, in ascending depth, for painter's-algorithm drawing.
///
/// The sort is stable, so items at equal depth keep their insertion order and the screen and the
/// PDF draw coplanar geometry identically. Depths are compared with [`f64::total_cmp`].
///
/// Takes a slice rather than `&mut Vec` so that any contiguous buffer can be sorted in place; a
/// `&mut Vec` argument coerces to it.
pub fn depth_order<T>(items: &mut [(f64, T)]) {
    // `sort_by` is a stable sort.
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
}

/// Computes the uniform scale and offset that place the projected unit box in a rectangle.
///
/// Returns `(scale, offset)` such that `scale · screen + offset` maps the screen coordinates of
/// [`Camera::project`] into `[0, rect_w] × [0, rect_h]`, in the same y-up orientation as the
/// screen coordinates.
///
/// The fit is independent of the view, as with MATLAB's `axis vis3d`, so the box does not change
/// size while the user drags to rotate it. The box `[-0.5, 0.5]³` lies inside the sphere of
/// diameter √3 about the origin, and every orthographic projection of that sphere is a disc of
/// the same diameter centred on the screen origin. The scale is therefore
/// `min(rect_w, rect_h) / √3` and the offset is the rectangle centre `(rect_w / 2, rect_h / 2)`,
/// so every projected point of the box lies inside the rectangle for every view, and the box
/// touches the rectangle in the views where its projection reaches the full diameter √3.
///
/// Negative or NaN dimensions are treated as zero, which gives a zero scale.
pub fn fit_to_rect(rect_w: f64, rect_h: f64) -> (f64, [f64; 2]) {
    // `max` returns the non-NaN operand, so NaN and negative sizes both become zero.
    let (w, h) = (rect_w.max(0.0), rect_h.max(0.0));
    (w.min(h) / 3f64.sqrt(), [w / 2.0, h / 2.0])
}
