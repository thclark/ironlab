use ironlab_scene::display::{DepthPlane, Point};
use ironlab_scene::maths::camera::{
    Camera, Plane, UNIT_BOX_CORNERS, back_planes, clamp_elevation, depth_order, depth_plane,
    fit_to_rect, normalise_box, wrap_azimuth,
};
use proptest::prelude::*;

use crate::assert_close;

const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];

fn cam(azimuth_deg: f64, elevation_deg: f64) -> Camera {
    Camera {
        azimuth_deg,
        elevation_deg,
    }
}

fn neg(p: [f64; 3]) -> [f64; 3] {
    [-p[0], -p[1], -p[2]]
}

#[track_caller]
fn assert_screen(camera: &Camera, p: [f64; 3], expected: [f64; 2]) {
    let s = camera.project(p).screen;
    assert_close(s[0], expected[0], 1e-12);
    assert_close(s[1], expected[1], 1e-12);
}

// Why: the default view must be MATLAB's, so figures ported from MATLAB look the same.
#[test]
fn default_camera_is_matlab_default_view() {
    assert_eq!(Camera::default(), cam(-37.5, 30.0));
}

// Why: el = 90 is the top-down view used to compare 3D plots with 2D ones; x must run right and
// y up, exactly as in a 2D axes.
#[test]
fn top_down_view_matches_2d_orientation() {
    let c = cam(0.0, 90.0);
    assert_screen(&c, X, [1.0, 0.0]);
    assert_screen(&c, Y, [0.0, 1.0]);
    assert!(c.project(Z).depth > c.project(neg(Z)).depth);
}

// Why: el = −90 is the reachable limit of drag rotation; the view from below must be the mirror
// of the top-down view (x right, y down, −z nearest) and the cos(−90°) ≈ 6e−17 rounding residue
// must not tip the back-plane choice, so the ceiling is at the back and the edge-on x and y
// walls fall back to their min faces.
#[test]
fn bottom_up_view_at_elevation_minus_ninety() {
    let c = cam(0.0, -90.0);
    assert_screen(&c, X, [1.0, 0.0]);
    assert_screen(&c, Y, [0.0, -1.0]);
    assert!(c.project(neg(Z)).depth > c.project(Z).depth);
    assert_eq!(back_planes(&c), [Plane::XMin, Plane::YMin, Plane::ZMax]);
}

// Why: az = 0, el = 0 is MATLAB's front view from −y: x right, z up, and +y recedes.
#[test]
fn front_view_looks_along_positive_y() {
    let c = cam(0.0, 0.0);
    assert_screen(&c, X, [1.0, 0.0]);
    assert_screen(&c, Z, [0.0, 1.0]);
    assert!(c.project(Y).depth < c.project(neg(Y)).depth);
}

// Why: azimuth rotates the viewpoint counter-clockwise about z, so az = 90 views from +x with +y
// to the right and az = 180 views from +y with +x to the left.
#[test]
fn azimuth_rotates_viewpoint_counter_clockwise() {
    let side = cam(90.0, 0.0);
    assert_screen(&side, Y, [1.0, 0.0]);
    assert!(side.project(X).depth > side.project(neg(X)).depth);

    let back = cam(180.0, 0.0);
    assert_screen(&back, X, [-1.0, 0.0]);
    assert!(back.project(Y).depth > back.project(neg(Y)).depth);
}

// Why: pins the derived appearance of MATLAB's default view: +x runs right and slightly up, +y
// runs left and up, z is foreshortened by cos 30°, the (−x, −y, +z) corner is nearest and the
// (−x, −y, −z) corner is lowest on screen.
#[test]
fn default_view_axis_directions_and_extreme_corners() {
    let c = Camera::default();
    let (sa, ca) = ((-37.5f64).to_radians().sin(), (-37.5f64).to_radians().cos());
    assert_screen(&c, X, [ca, -0.5 * sa]);
    assert_screen(&c, Y, [sa, 0.5 * ca]);
    assert_screen(&c, Z, [0.0, 30f64.to_radians().cos()]);
    let x_dir = c.project(X).screen;
    assert!(x_dir[0] > 0.0 && x_dir[1] > 0.0);
    let y_dir = c.project(Y).screen;
    assert!(y_dir[0] < 0.0 && y_dir[1] > 0.0);

    let nearest = UNIT_BOX_CORNERS
        .into_iter()
        .max_by(|a, b| c.project(*a).depth.total_cmp(&c.project(*b).depth))
        .unwrap();
    assert_eq!(nearest, [-0.5, -0.5, 0.5]);
    let lowest = UNIT_BOX_CORNERS
        .into_iter()
        .min_by(|a, b| c.project(*a).screen[1].total_cmp(&c.project(*b).screen[1]))
        .unwrap();
    assert_eq!(lowest, [-0.5, -0.5, -0.5]);
}

// Why: grid lines belong on the far walls; at MATLAB's default view those are the x-max and
// y-max walls and the floor, because the viewer sits on the −x, −y, +z side.
#[test]
fn back_planes_at_default_view() {
    assert_eq!(
        back_planes(&Camera::default()),
        [Plane::XMax, Plane::YMax, Plane::ZMin]
    );
}

// Why: edge-on faces are a tie; the documented tie-break (min face) keeps the floor as the back
// plane at el = 0 and avoids flicker from floating-point noise at exact axis-aligned views.
#[test]
fn back_planes_break_ties_towards_min_faces() {
    assert_eq!(
        back_planes(&cam(0.0, 0.0)),
        [Plane::XMin, Plane::YMax, Plane::ZMin]
    );
    assert_eq!(
        back_planes(&cam(0.0, 90.0)),
        [Plane::XMin, Plane::YMin, Plane::ZMin]
    );
}

// Why: views from below or from the opposite side must move the back walls accordingly.
#[test]
fn back_planes_follow_the_viewer() {
    assert_eq!(
        back_planes(&cam(-37.5, -30.0))[2],
        Plane::ZMax,
        "viewed from below, the ceiling is at the back"
    );
    assert_eq!(
        back_planes(&cam(142.5, 30.0)),
        [Plane::XMin, Plane::YMin, Plane::ZMin]
    );
}

// Why: the painter's algorithm draws far geometry first; a plane nearer the viewer must be
// painted entirely after a plane farther away. (At el = 60 the planes' centre depths do not
// interleave, so sorting by centre depth alone is enough to separate them.)
#[test]
fn depth_order_paints_far_plane_before_near_plane() {
    let c = cam(-37.5, 60.0);
    let mut items = Vec::new();
    for (plane, z) in [("top", 0.5), ("bottom", -0.5)] {
        for i in 0..4 {
            for j in 0..4 {
                let centre = [-0.375 + 0.25 * i as f64, -0.375 + 0.25 * j as f64, z];
                items.push((c.project(centre).depth, plane));
            }
        }
    }
    depth_order(&mut items);
    let first_top = items.iter().position(|(_, p)| *p == "top").unwrap();
    assert!(items[..first_top].iter().all(|(_, p)| *p == "bottom"));
    assert_eq!(first_top, 16);
}

// Why: coplanar items (a face and its edges) must keep insertion order so screen and PDF agree.
#[test]
fn depth_order_is_stable() {
    let mut items = vec![(1.0, 'a'), (0.0, 'b'), (1.0, 'c'), (0.0, 'd')];
    depth_order(&mut items);
    let order: String = items.iter().map(|(_, c)| *c).collect();
    assert_eq!(order, "bdac");
}

// Why: data limits must map onto the unit box so that the camera and box geometry are
// independent of data units.
#[test]
fn normalise_box_maps_limits_to_unit_box() {
    let lo = [0.0, -10.0, 100.0];
    let hi = [2.0, 10.0, 200.0];
    let lin = [false; 3];
    assert_eq!(normalise_box(lo, lo, hi, lin), [-0.5; 3]);
    assert_eq!(normalise_box(hi, lo, hi, lin), [0.5; 3]);
    assert_eq!(normalise_box([1.0, 0.0, 150.0], lo, hi, lin), [0.0; 3]);
    assert_eq!(normalise_box([4.0, 0.0, 150.0], lo, hi, lin)[0], 1.5);
}

// Why: log axes in 3D must place decades evenly, exactly as 2D log axes do.
#[test]
fn normalise_box_uses_log_space_on_log_axes() {
    let p = normalise_box(
        [10.0, 10.0, 0.0],
        [1.0, 1.0, 0.0],
        [100.0, 100.0, 1.0],
        [true, false, false],
    );
    assert_close(p[0], 0.0, 1e-12);
    assert_close(p[1], 10.0 / 99.0 - 0.5 - 1.0 / 99.0, 1e-12);
    let bad = normalise_box([-1.0, 1.0, 0.0], [1.0; 3], [100.0; 3], [true, false, false]);
    assert!(bad[0].is_nan());
}

// Why: a flat axis (constant data) must not produce NaN positions.
#[test]
fn normalise_box_degenerate_axis_maps_to_centre() {
    assert_eq!(
        normalise_box([3.0, 3.0, 3.0], [3.0; 3], [3.0; 3], [false, true, false]),
        [0.0; 3]
    );
}

// Why: rotating past the poles would flip the scene upside down; elevation must stop at ±90°.
#[test]
fn elevation_is_clamped() {
    assert_eq!(clamp_elevation(120.0), 90.0);
    assert_eq!(clamp_elevation(-95.0), -90.0);
    assert_eq!(clamp_elevation(45.0), 45.0);
    assert_eq!(clamp_elevation(f64::NAN), 30.0);
}

// Why: repeated drag rotation accumulates azimuth without bound; wrapping keeps it in MATLAB's
// displayed range and makes views comparable.
#[test]
fn azimuth_is_wrapped() {
    assert_eq!(wrap_azimuth(180.0), 180.0);
    assert_eq!(wrap_azimuth(-180.0), 180.0);
    assert_eq!(wrap_azimuth(540.0), 180.0);
    assert_eq!(wrap_azimuth(190.0), -170.0);
    assert_eq!(wrap_azimuth(-37.5), -37.5);
    assert_eq!(wrap_azimuth(360.0), 0.0);
    assert_eq!(wrap_azimuth(-370.0), -10.0);
    assert_eq!(wrap_azimuth(f64::NAN), -37.5);
    assert_eq!(wrap_azimuth(f64::INFINITY), -37.5);
}

// Why: the fit must not depend on the view (MATLAB's `axis vis3d`), otherwise the box pumps in
// size while the user drags to rotate it; the circumscribed sphere of the unit box (diameter √3)
// fits the shorter side of the rectangle and is centred.
#[test]
fn fit_to_rect_fits_circumscribed_sphere_to_shorter_side() {
    let (scale, offset) = fit_to_rect(200.0, 100.0);
    assert_close(scale, 100.0 / 3f64.sqrt(), 1e-12);
    assert_eq!(offset, [100.0, 50.0]);
    let (tall_scale, tall_offset) = fit_to_rect(60.0, 90.0);
    assert_close(tall_scale, 60.0 / 3f64.sqrt(), 1e-12);
    assert_eq!(tall_offset, [30.0, 45.0]);
}

// Why: the fit is as large as a rotation-invariant fit can be: in the view that looks along a
// body diagonal's perpendicular (az = 45°, el = atan √2) the box's projection spans the full
// diameter √3 vertically, so it touches both limiting sides of the rectangle.
#[test]
fn fit_to_rect_is_tight_in_the_widest_view() {
    let c = cam(45.0, 2f64.sqrt().atan().to_degrees());
    let (w, h) = (300.0, 120.0);
    let (scale, offset) = fit_to_rect(w, h);
    let ys: Vec<f64> = UNIT_BOX_CORNERS
        .iter()
        .map(|p| scale * c.project(*p).screen[1] + offset[1])
        .collect();
    let lowest = ys.iter().copied().fold(f64::INFINITY, f64::min);
    let highest = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert_close(lowest, 0.0, 1e-9);
    assert_close(highest, h, 1e-9);
}

// Why: a collapsed or invalid plot rectangle (a tile squeezed to nothing by its labels) must give
// a finite, empty fit rather than a negative or NaN scale that would mirror or corrupt the box.
#[test]
fn fit_to_rect_degenerate_rectangles_give_zero_scale() {
    assert_eq!(fit_to_rect(0.0, 50.0), (0.0, [0.0, 25.0]));
    let (scale, offset) = fit_to_rect(-10.0, 50.0);
    assert_eq!(scale, 0.0);
    assert!(offset.iter().all(|c| c.is_finite()));
    let (scale, offset) = fit_to_rect(f64::NAN, 50.0);
    assert_eq!(scale, 0.0);
    assert!(offset.iter().all(|c| c.is_finite()));
}

// WHY: three non-collinear points determine one plane, and that plane is the exact depth of
// every point of a triangular face; a fit that was merely close would let a face lose to a
// neighbour it should hide along their shared edge.
#[test]
fn depth_plane_through_three_points_reproduces_their_depths_exactly() {
    let points = [
        (Point::new(0.0, 0.0), 1.0),
        (Point::new(2.0, 0.0), 4.0),
        (Point::new(0.0, 3.0), -2.0),
    ];
    let plane = depth_plane(&points).expect("three non-collinear points define a plane");
    for (p, depth) in points {
        assert_close(plane.at(p), depth, 1e-12);
    }
}

// WHY: the four corners of a planar face (every face of a flat surface, every image) lie on one
// plane, and the fit must return that plane rather than a compromise between the corners, so
// that adjacent coplanar faces get identical depths along their shared edge and neither wins
// over the other by rounding.
#[test]
fn depth_plane_through_coplanar_points_is_that_plane() {
    let truth = DepthPlane {
        a: 0.25,
        b: -0.75,
        c: 2.0,
    };
    let corners = [
        Point::new(1.0, 1.0),
        Point::new(3.0, 1.0),
        Point::new(3.0, 3.0),
        Point::new(1.0, 3.0),
    ];
    let points: Vec<(Point, f64)> = corners.iter().map(|&p| (p, truth.at(p))).collect();
    let plane = depth_plane(&points).expect("the corners of a planar face define a plane");
    assert_close(plane.a, truth.a, 1e-12);
    assert_close(plane.b, truth.b, 1e-12);
    assert_close(plane.c, truth.c, 1e-12);
}

// WHY: a twisted face (its corners not coplanar, as on every curved surface) has no exact plane,
// and the fit must be the least-squares plane, so that the face's depth is the best single plane
// for all four corners rather than the plane through the first three, which would ignore the
// fourth corner entirely. For the corners (0, 0) → 0, (1, 0) → 0, (1, 1) → 1 and (0, 1) → 0 the
// sums are Σx² = Σy² = 2, Σxy = 1, Σx = Σy = 2, n = 4 and Σxd = Σyd = Σd = 1, so the normal
// equations are
//     2a +  b + 2c = 1
//      a + 2b + 2c = 1
//     2a + 2b + 4c = 1
// whose solution is a = b = 0.5, c = −0.25: the plane d = 0.5x + 0.5y − 0.25, with residuals
// −0.25, +0.25, −0.25 and +0.25 at the corners. (The plane through the first three corners is
// d = y, which the expected coefficients rule out.)
#[test]
fn depth_plane_through_a_twisted_quad_is_the_least_squares_plane() {
    let points = [
        (Point::new(0.0, 0.0), 0.0),
        (Point::new(1.0, 0.0), 0.0),
        (Point::new(1.0, 1.0), 1.0),
        (Point::new(0.0, 1.0), 0.0),
    ];
    let plane = depth_plane(&points).expect("four points define a fit");
    assert_close(plane.a, 0.5, 1e-12);
    assert_close(plane.b, 0.5, 1e-12);
    assert_close(plane.c, -0.25, 1e-12);
}

// WHY: a face seen edge-on projects to a line, so its tilt across the line is undetermined and
// the normal equations are singular; the fit must fall back to the constant plane at the mean
// depth rather than fail, or return a plane with an arbitrary and possibly huge tilt that would
// put the face in front of or behind everything else. The depths here rise along the line, so
// a fit that kept the tilt along the line would be told apart from the constant plane.
#[test]
fn depth_plane_of_collinear_positions_is_constant_at_the_mean_depth() {
    let points = [
        (Point::new(0.0, 0.0), 0.0),
        (Point::new(1.0, 1.0), 3.0),
        (Point::new(2.0, 2.0), 6.0),
    ];
    let plane = depth_plane(&points).expect("an edge-on face still has a depth");
    assert_eq!((plane.a, plane.b), (0.0, 0.0));
    assert_close(plane.c, 3.0, 1e-12);
}

// WHY: a real edge-on face is collinear only to rounding, at figure scale: hundreds of points
// across and off the line by a millionth of a point. Its normal equations are not exactly
// singular, and solving them regardless gives a tilt of the order of a million that would put
// the face in front of or behind everything else; the fit must judge singularity relative to
// the spread of the positions and fall back to the constant plane.
#[test]
fn depth_plane_of_nearly_collinear_positions_at_figure_scale_is_constant() {
    let points = [
        (Point::new(100.0, 100.0), 0.0),
        (Point::new(300.0, 300.0 + 1e-6), 1.0),
        (Point::new(500.0, 500.0), 0.0),
    ];
    let plane = depth_plane(&points).expect("a nearly edge-on face still has a depth");
    assert_eq!((plane.a, plane.b), (0.0, 0.0));
    assert_close(plane.c, 1.0 / 3.0, 1e-12);
}

// WHY: fewer than three points cannot fix a tilt: a marker is placed at one point and a pair of
// points is a segment. Both get the constant plane at their mean depth, so that they have a
// depth at all and it is the one their points share on average.
#[test]
fn depth_plane_of_fewer_than_three_points_is_constant_at_the_mean_depth() {
    let single = depth_plane(&[(Point::new(4.0, -2.0), 7.5)]).expect("a point has a depth");
    assert_eq!(single, DepthPlane::constant(7.5));
    let pair = [(Point::new(0.0, 0.0), 1.0), (Point::new(4.0, 0.0), 2.0)];
    let plane = depth_plane(&pair).expect("a segment has a depth");
    assert_eq!((plane.a, plane.b), (0.0, 0.0));
    assert_close(plane.c, 1.5, 1e-12);
}

// WHY: nothing can have a depth derived from no points, and a NaN depth or an infinite position
// would poison the fit silently (the normal equations still produce numbers); the caller must
// be told there is no plane, so that it skips the item as the validity contract requires.
#[test]
fn depth_plane_is_none_for_no_points_or_non_finite_input() {
    assert_eq!(depth_plane(&[]), None);
    let with_nan_depth = [
        (Point::new(0.0, 0.0), 1.0),
        (Point::new(1.0, 0.0), 2.0),
        (Point::new(0.0, 1.0), f64::NAN),
    ];
    assert_eq!(depth_plane(&with_nan_depth), None);
    let with_infinite_position = [
        (Point::new(0.0, 0.0), 1.0),
        (Point::new(1.0, 0.0), 2.0),
        (Point::new(f64::INFINITY, 1.0), 3.0),
    ];
    assert_eq!(depth_plane(&with_infinite_position), None);
}

proptest! {
    // Why: an orthographic view must be a pure rotation: orthonormal rows preserve shapes and a
    // right-handed basis guarantees the scene is never mirrored, for every reachable view.
    #[test]
    fn view_matrix_is_a_right_handed_rotation(az in -180f64..=180.0, el in -90f64..=90.0) {
        let [r, u, t] = cam(az, el).view_matrix();
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        for (a, b, expected) in [
            (r, r, 1.0), (u, u, 1.0), (t, t, 1.0), (r, u, 0.0), (r, t, 0.0), (u, t, 0.0),
        ] {
            prop_assert!((dot(a, b) - expected).abs() < 1e-12);
        }
        let rxu = [
            r[1] * u[2] - r[2] * u[1],
            r[2] * u[0] - r[0] * u[2],
            r[0] * u[1] - r[1] * u[0],
        ];
        for (actual, expected) in rxu.iter().zip(t) {
            prop_assert!((actual - expected).abs() < 1e-12);
        }
    }

    // Why: rotation invariance must never cost containment; for every view and rectangle, every
    // corner of the fitted box lies inside the plot rectangle.
    #[test]
    fn fitted_box_stays_inside_rect_for_every_view(
        az in -180f64..=180.0,
        el in -90f64..=90.0,
        w in 1f64..1000.0,
        h in 1f64..1000.0,
    ) {
        let c = cam(az, el);
        let (scale, offset) = fit_to_rect(w, h);
        let eps = 1e-9 * w.max(h);
        for corner in UNIT_BOX_CORNERS {
            let s = c.project(corner).screen;
            let (x, y) = (scale * s[0] + offset[0], scale * s[1] + offset[1]);
            prop_assert!(x >= -eps && x <= w + eps, "x = {} outside [0, {}]", x, w);
            prop_assert!(y >= -eps && y <= h + eps, "y = {} outside [0, {}]", y, h);
        }
    }

    // WHY: exactness for coplanar points must hold for every plane and every square, not only
    // the hand-picked ones; a fit that lost precision for faces far from the origin would give
    // neighbouring faces slightly different depths along their shared edge, where a depth test
    // then flickers between them.
    #[test]
    fn depth_plane_recovers_any_plane_from_the_corners_of_a_square(
        a in -10f64..10.0,
        b in -10f64..10.0,
        c in -10f64..10.0,
        x0 in -10f64..10.0,
        y0 in -10f64..10.0,
        side in 1f64..10.0,
    ) {
        let truth = DepthPlane { a, b, c };
        let corners = [
            Point::new(x0, y0),
            Point::new(x0 + side, y0),
            Point::new(x0 + side, y0 + side),
            Point::new(x0, y0 + side),
        ];
        let points: Vec<(Point, f64)> = corners.iter().map(|&p| (p, truth.at(p))).collect();
        let plane = depth_plane(&points);
        prop_assert!(plane.is_some(), "four finite corners define a plane");
        let plane = plane.expect("checked above");
        for (p, depth) in &points {
            let fitted = plane.at(*p);
            let tolerance = 1e-9 * (1.0 + depth.abs());
            prop_assert!((fitted - depth).abs() < tolerance, "at {p:?}: {fitted} vs {depth}");
        }
    }
}
