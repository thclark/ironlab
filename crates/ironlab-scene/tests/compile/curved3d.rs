//! Cylindrical and spherical axes: where data points land, how the two kinds of artist are drawn there, the image
//! lattice on discs, walls and spheres, automatic limits, the aspect of a cylinder, the decorations that frame both
//! kinds of axes, and the camera that navigates them.
//!
//! Every expected position is rebuilt here from the definition of the two coordinate systems and the public camera,
//! never read back from the compiler's own mapping code:
//!
//! - Cylindrical axes: x is the angle θ, y the radius r and z the height. The radial fraction is
//!   `ρ = hole + (1 − hole)·t_r`, where `t_r` is the position of r between its limits (in decades on a logarithmic
//!   axis, and `1 − t_r` when y is reversed). The azimuth in the box is `φ = zero_deg + s·θ·2π/turn`, with `s = −1`
//!   when x is reversed (clockwise) and `+1` otherwise, so that an angle keeps its true direction in a partial sector
//!   as it does in polar axes. The box point is `(0.5·ρ·cos φ, 0.5·ρ·sin φ, n_z)` with `n_z` the usual normalised
//!   height in [−0.5, 0.5].
//! - Spherical axes: x is the azimuth, y the polar angle and z the radius. `ρ = t_r` from z; the elevation is
//!   `ε = y·2π/turn` for `PolarAngle::Elevation` and `π/2 − y·2π/turn` for `PolarAngle::Inclination`; the azimuth is
//!   `φ = s·x·2π/turn`, zero along +X. The box point is `0.5·ρ·(cos ε cos φ, cos ε sin φ, sin ε)`.
//!
//! The box point is scaled by the sides of the box (`[1, 1, 1]`, or `[1, 1, h]` normalised to a longest side of 1 for
//! a cylinder with a fixed aspect), projected by [`Camera::project`], and placed in the plot rectangle by the
//! documented fit: what the axes draws is fitted by its own circumscribing sphere, of radius 0.5 for the sphere and
//! `√(s² + h²)/2` for a cylinder of diameter `s` and height `h`, so [`fit_to_rect`], which fits the sphere of radius
//! `√3/2`, is scaled by `(√3/2)/radius`, multiplied by the zoom of the view, about the centre of the plot rectangle
//! moved by the pan of the view in plot fractions. The tick labels placed outside the frame circles take room from
//! that fit, so the fixtures that measure positions hide them; the room they take is tested on its own.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use ironlab_ir::{
    AngleUnits, Artist, Aspect, Axes, ImagePlane, Limits, NodeId, PolarAngle, PolarView,
    Projection, QuiverScale, Scale, SphericalView, TickLabels, View3d,
};
use ironlab_scene::display::{Depth, ImageGeometry, ImageMesh, PathSegment, Point};
use ironlab_scene::hit::AxesHitKind;
use ironlab_scene::maths::camera::{Camera, FACE_DEPTH_BIAS, fit_to_rect};
use ironlab_scene::{Scene, SceneWarningKind};

use crate::common::{Fx, cell, compile_figure, placement, range};
use crate::probe::{Leaf, axes_hit, from_source, grouped_leaves, leaves, marker_instances};

/// Agreement expected of a point placed on the page from data, in points.
const TOL: f64 = 1e-6;

/// Agreement expected of a depth computed from data.
const DEPTH_TOL: f64 = 1e-9;

/// The largest distance, in points, that a lattice chord may stray from the curve it stands for on the page.
const CHORD: f64 = 0.05;

// ---------------------------------------------------------------------------------------------------------------------
// The two coordinate systems, rebuilt from their definitions
// ---------------------------------------------------------------------------------------------------------------------

/// The position of `v` between `lo` and `hi`, in decades when `log`, from 0 at `lo` to 1 at `hi`.
fn fraction(v: f64, lo: f64, hi: f64, log: bool) -> f64 {
    if log {
        (v.log10() - lo.log10()) / (hi.log10() - lo.log10())
    } else {
        (v - lo) / (hi - lo)
    }
}

/// How one axis of the fixture is set: its manual limits, scale and direction.
#[derive(Clone, Copy, Debug)]
struct AxisSpec {
    lo: f64,
    hi: f64,
    log: bool,
    reverse: bool,
}

impl AxisSpec {
    const fn linear(lo: f64, hi: f64) -> Self {
        Self {
            lo,
            hi,
            log: false,
            reverse: false,
        }
    }

    /// The position of `v` along the axis, from 0 at the lower limit to 1 at the upper, mirrored when reversed.
    fn t(&self, v: f64) -> f64 {
        let t = fraction(v, self.lo, self.hi, self.log);
        if self.reverse { 1.0 - t } else { t }
    }

    /// The normalised box coordinate of `v` along a straight (height) axis, in [−0.5, 0.5].
    fn n(&self, v: f64) -> f64 {
        self.t(v) - 0.5
    }

    fn apply(&self, axis: &mut ironlab_ir::Axis) {
        axis.limits = Limits::Manual {
            min: self.lo,
            max: self.hi,
        };
        axis.scale = if self.log { Scale::Log } else { Scale::Linear };
        axis.reverse = self.reverse;
    }
}

/// The definition of a cylindrical axes: its polar view and its three axes.
#[derive(Clone, Copy, Debug)]
struct Cylinder {
    polar: PolarView,
    theta: AxisSpec,
    r: AxisSpec,
    z: AxisSpec,
}

impl Cylinder {
    fn new() -> Self {
        Self {
            polar: PolarView::default(),
            theta: AxisSpec::linear(0.0, 360.0),
            r: AxisSpec::linear(0.0, 2.0),
            z: AxisSpec::linear(-1.0, 1.0),
        }
    }

    /// The box azimuth of an angle, in radians anticlockwise from +X seen from +Z.
    fn phi(&self, theta: f64) -> f64 {
        let sign = if self.theta.reverse { -1.0 } else { 1.0 };
        self.polar.zero_deg.to_radians() + sign * theta * TAU / self.polar.angle_units.turn()
    }

    /// The radial fraction of a radius, from the edge of the hole (or the centre) to the rim at 1.
    fn rho(&self, r: f64) -> f64 {
        self.polar.hole + (1.0 - self.polar.hole) * self.r.t(r)
    }

    /// The normalised box point of a data point `[θ, r, z]`.
    fn normalise(&self, p: [f64; 3]) -> [f64; 3] {
        let (phi, rho) = (self.phi(p[0]), self.rho(p[1]));
        [0.5 * rho * phi.cos(), 0.5 * rho * phi.sin(), self.z.n(p[2])]
    }

    fn projection(&self, view3d: View3d) -> Projection {
        Projection::Cylindrical {
            view3d,
            polar: self.polar,
        }
    }

    /// Sets the three axes, and hides the angular tick labels, which are placed outside the rim and so take room from
    /// the fit (see [`Lens`]).
    fn apply(&self, axes: &mut Axes) {
        self.theta.apply(&mut axes.x);
        self.r.apply(&mut axes.y);
        self.z.apply(&mut axes.z);
        axes.x.tick_labels = TickLabels::Hidden;
    }
}

/// The definition of a spherical axes: its view and its three axes.
#[derive(Clone, Copy, Debug)]
struct Sphere {
    spherical: SphericalView,
    az: AxisSpec,
    polar: AxisSpec,
    r: AxisSpec,
}

impl Sphere {
    fn new() -> Self {
        Self {
            spherical: SphericalView::default(),
            az: AxisSpec::linear(0.0, 360.0),
            polar: AxisSpec::linear(-90.0, 90.0),
            r: AxisSpec::linear(0.0, 2.0),
        }
    }

    fn inclination() -> Self {
        Self {
            spherical: SphericalView {
                polar_angle: PolarAngle::Inclination,
                ..SphericalView::default()
            },
            polar: AxisSpec::linear(0.0, 180.0),
            ..Self::new()
        }
    }

    fn turn(&self) -> f64 {
        self.spherical.angle_units.turn()
    }

    /// The normalised box point of a data point `[azimuth, polar angle, radius]`.
    fn normalise(&self, p: [f64; 3]) -> [f64; 3] {
        let sign = if self.az.reverse { -1.0 } else { 1.0 };
        let phi = sign * p[0] * TAU / self.turn();
        let angle = p[1] * TAU / self.turn();
        let eps = match self.spherical.polar_angle {
            PolarAngle::Elevation => angle,
            PolarAngle::Inclination => FRAC_PI_2 - angle,
        };
        let rho = self.r.t(p[2]);
        [
            0.5 * rho * eps.cos() * phi.cos(),
            0.5 * rho * eps.cos() * phi.sin(),
            0.5 * rho * eps.sin(),
        ]
    }

    fn projection(&self, view3d: View3d) -> Projection {
        Projection::Spherical {
            view3d,
            spherical: self.spherical,
        }
    }

    /// Sets the three axes, and hides the azimuth and polar-angle tick labels, which are placed outside the equator
    /// and the meridian and so take room from the fit (see [`Lens`]).
    fn apply(&self, axes: &mut Axes) {
        self.az.apply(&mut axes.x);
        self.polar.apply(&mut axes.y);
        self.r.apply(&mut axes.z);
        axes.x.tick_labels = TickLabels::Hidden;
        axes.y.tick_labels = TickLabels::Hidden;
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The camera and the fit, rebuilt from their public definitions
// ---------------------------------------------------------------------------------------------------------------------

/// The projection of the normalised box of an axes into figure space, rebuilt from the public camera and fit.
#[derive(Clone, Copy, Debug)]
struct Lens {
    camera: Camera,
    sides: [f64; 3],
    scale: f64,
    centre: Point,
}

impl Lens {
    /// The lens of an axes as compiled, with the given view, box sides and radius of the sphere that circumscribes
    /// what the axes draws. The camera is built from the raw view, which is correct for views that need no wrapping
    /// or clamping, as every view these tests use.
    fn of(scene: &Scene, ax: NodeId, view: View3d, sides: [f64; 3], radius: f64) -> Self {
        let plot = axes_hit(scene, ax).plot_rect;
        let (fit, offset) = fit_to_rect(plot.width, plot.height);
        Self {
            camera: Camera {
                azimuth_deg: view.azimuth_deg,
                elevation_deg: view.elevation_deg,
            },
            sides,
            // `fit_to_rect` fits the sphere of radius √3/2 about the unit cube; the drawing is fitted by its own.
            scale: fit * (0.5 * 3f64.sqrt()) / radius * view.zoom,
            centre: Point::new(
                plot.x + offset[0] + view.pan_x * plot.width,
                plot.y + offset[1] + view.pan_y * plot.height,
            ),
        }
    }

    /// The lens of a cylindrical axes whose box has the given sides `[s, s, h]`: the cylinder of diameter `s` and
    /// height `h` is circumscribed by the sphere of radius `√(s² + h²)/2`.
    fn cylinder(scene: &Scene, ax: NodeId, view: View3d, sides: [f64; 3]) -> Self {
        Self::of(scene, ax, view, sides, 0.5 * sides[0].hypot(sides[2]))
    }

    /// The lens of a spherical axes, whose sphere of unit diameter is its own circumscribing sphere.
    fn sphere(scene: &Scene, ax: NodeId, view: View3d) -> Self {
        Self::of(scene, ax, view, [1.0; 3], 0.5)
    }

    /// The figure-space position and the depth of a point of the normalised box.
    fn project(&self, n: [f64; 3]) -> (Point, f64) {
        let p = self
            .camera
            .project(std::array::from_fn(|k| n[k] * self.sides[k]));
        (
            Point::new(
                self.centre.x + self.scale * p.screen[0],
                self.centre.y - self.scale * p.screen[1],
            ),
            p.depth,
        )
    }

    /// The point of the normalised box on the plane where coordinate `fixed` equals `value` that projects to the
    /// figure-space point `q`. The orthographic projection restricted to a plane that is not seen edge-on is an
    /// invertible affine map, so this is a 2 × 2 solve.
    fn on_plane(&self, q: Point, fixed: usize, value: f64) -> [f64; 3] {
        let [right, up, _] = self.camera.view_matrix();
        let s = [
            (q.x - self.centre.x) / self.scale,
            -(q.y - self.centre.y) / self.scale,
        ];
        let free: Vec<usize> = (0..3).filter(|k| *k != fixed).collect();
        let (a, b) = (free[0], free[1]);
        let m = |row: [f64; 3], k: usize| row[k] * self.sides[k];
        let rhs = [s[0] - m(right, fixed) * value, s[1] - m(up, fixed) * value];
        let (m00, m01, m10, m11) = (m(right, a), m(right, b), m(up, a), m(up, b));
        let det = m00 * m11 - m01 * m10;
        assert!(det.abs() > 1e-3, "the plane is seen nearly edge-on");
        let mut n = [0.0; 3];
        n[fixed] = value;
        n[a] = (rhs[0] * m11 - m01 * rhs[1]) / det;
        n[b] = (m00 * rhs[1] - rhs[0] * m10) / det;
        n
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Fixtures and probes
// ---------------------------------------------------------------------------------------------------------------------

/// A one-tile figure holding one axes of `projection`, with the three axes set by `apply`.
fn axes_with(projection: Projection, apply: impl FnOnce(&mut Axes)) -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes_in(cell(0, 0, 1, 1), projection);
    apply(fx.ax(ax));
    (fx, ax)
}

fn cylinder_axes(c: &Cylinder, view3d: View3d) -> (Fx, NodeId) {
    axes_with(c.projection(view3d), |axes| c.apply(axes))
}

fn sphere_axes(s: &Sphere, view3d: View3d) -> (Fx, NodeId) {
    axes_with(s.projection(view3d), |axes| s.apply(axes))
}

/// Compiles a figure and requires that it raises no warning other than a tick label cut by the edge of the page, which
/// a zoomed or panned view may push there and which says nothing about the geometry these tests measure.
fn compile_clean(fx: Fx) -> Scene {
    let scene = compile_figure(&fx.build());
    let warnings: Vec<_> = scene
        .warnings
        .iter()
        .filter(|w| !matches!(w.kind, SceneWarningKind::TextOutsidePage { .. }))
        .collect();
    assert!(warnings.is_empty(), "{warnings:?}");
    scene
}

#[track_caller]
fn assert_at(actual: Point, expected: Point, what: &str) {
    assert!(
        (actual.x - expected.x).abs() <= TOL && (actual.y - expected.y).abs() <= TOL,
        "{what}: drawn at {actual:?}, expected {expected:?}"
    );
}

#[track_caller]
fn assert_depth(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= DEPTH_TOL,
        "{what}: depth {actual}, expected {expected}"
    );
}

/// Asserts that the markers of a scatter lie at the projections of `expected` (normalised box points), each with
/// the depth of its own point.
#[track_caller]
fn assert_markers_at(
    scene: &Scene,
    scatter: NodeId,
    lens: &Lens,
    expected: &[[f64; 3]],
    what: &str,
) {
    let markers = marker_instances(&leaves(scene), scatter);
    assert_eq!(
        markers.len(),
        expected.len(),
        "{what}: one marker per point"
    );
    for m in markers {
        let (p, d) = lens.project(expected[m.source_index]);
        let label = format!("{what}: point {}", m.source_index);
        assert_at(m.position, p, &label);
        assert_depth(m.depth, d, &label);
    }
}

/// Every straight segment drawn for an artist, in figure space.
fn segments_of(scene: &Scene, id: NodeId) -> Vec<(Point, Point)> {
    from_source(&leaves(scene), id)
        .iter()
        .flat_map(Leaf::line_segments)
        .collect()
}

/// The one image leaf of an artist and its mesh.
fn mesh_of(scene: &Scene, id: NodeId) -> (Leaf, ImageMesh) {
    let drawn: Vec<Leaf> = from_source(&leaves(scene), id)
        .into_iter()
        .filter(|l| l.image().is_some())
        .collect();
    assert_eq!(drawn.len(), 1, "the image is one item");
    let leaf = drawn[0].clone();
    let ImageGeometry::Mesh(mesh) = &leaf.image().unwrap().geometry else {
        panic!(
            "an image in curved 3D axes is drawn through a mesh: {:?}",
            leaf.image().unwrap().geometry
        );
    };
    let mesh = mesh.clone();
    (leaf, mesh)
}

/// Asserts that every node of an image mesh lies at the projection of the data point that its pixel-space position
/// names, with that point's depth pushed back by the face bias, that every pixel edge is a node, and that along both
/// lattice directions the chord between neighbouring nodes strays from the projected curve by at most [`CHORD`].
///
/// `data(u, v)` gives the data point of pixel-space position `(u, v)`, `normalise` its box point.
#[track_caller]
fn assert_mesh_follows(
    leaf: &Leaf,
    mesh: &ImageMesh,
    (nx, ny): (usize, usize),
    lens: &Lens,
    data: impl Fn(f64, f64) -> [f64; 3],
    normalise: impl Fn([f64; 3]) -> [f64; 3],
    what: &str,
) {
    for k in 0..=nx {
        assert!(
            mesh.u.contains(&(k as f64)),
            "{what}: a node at column edge {k}: {:?}",
            mesh.u
        );
    }
    for k in 0..=ny {
        assert!(
            mesh.v.contains(&(k as f64)),
            "{what}: a node at row edge {k}: {:?}",
            mesh.v
        );
    }
    let (nu, nv) = (mesh.u.len(), mesh.v.len());
    assert_eq!(mesh.points.len(), nu * nv, "{what}: one point per node");
    let depths = mesh
        .depths
        .as_ref()
        .unwrap_or_else(|| panic!("{what}: the mesh carries one depth per node"));
    assert_eq!(depths.len(), nu * nv, "{what}: one depth per node");
    let place = |u: f64, v: f64| lens.project(normalise(data(u, v)));
    for (j, &v) in mesh.v.iter().enumerate() {
        for (i, &u) in mesh.u.iter().enumerate() {
            let (p, d) = place(u, v);
            let label = format!("{what}: the node at pixel-space ({u}, {v})");
            assert_at(leaf.transform.apply(mesh.points[j * nu + i]), p, &label);
            assert_depth(depths[j * nu + i], d - FACE_DEPTH_BIAS, &label);
        }
    }
    let stray = |a: (f64, f64), b: (f64, f64)| {
        let (pa, pb) = (place(a.0, a.1).0, place(b.0, b.1).0);
        let mid = place((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0).0;
        let chord_mid = Point::new((pa.x + pb.x) / 2.0, (pa.y + pb.y) / 2.0);
        ((mid.x - chord_mid.x).powi(2) + (mid.y - chord_mid.y).powi(2)).sqrt()
    };
    for &v in &mesh.v {
        for pair in mesh.u.windows(2) {
            let e = stray((pair[0], v), (pair[1], v));
            assert!(
                e <= CHORD + 1e-9,
                "{what}: the lattice chord from u = {} to {} at v = {v} strays {e} pt",
                pair[0],
                pair[1]
            );
        }
    }
    for &u in &mesh.u {
        for pair in mesh.v.windows(2) {
            let e = stray((u, pair[0]), (u, pair[1]));
            assert!(
                e <= CHORD + 1e-9,
                "{what}: the lattice chord from v = {} to {} at u = {u} strays {e} pt",
                pair[0],
                pair[1]
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Where data points land: cylindrical axes
// ---------------------------------------------------------------------------------------------------------------------

const THETA: [f64; 5] = [0.0, 45.0, 135.0, 250.0, 300.0];
const R: [f64; 5] = [0.5, 1.0, 1.5, 2.0, 0.25];
const Z: [f64; 5] = [-1.0, -0.3, 0.0, 0.6, 1.0];

/// Compiles a scatter of `points` in cylindrical axes defined by `c` at `view3d`, and asserts that each marker lies
/// at the projection of its own box point with its own depth.
#[track_caller]
fn assert_cylinder_places(c: Cylinder, view3d: View3d, points: [[f64; 5]; 3], what: &str) {
    let (mut fx, ax) = cylinder_axes(&c, view3d);
    let scatter = fx.scatter(ax, &points[0], &points[1], Some(&points[2]), |_, _| {});
    let scene = compile_clean(fx);
    assert_eq!(
        axes_hit(&scene, ax).kind,
        AxesHitKind::ThreeD,
        "{what}: cylindrical axes are navigated through their camera"
    );
    let lens = Lens::cylinder(&scene, ax, view3d, [1.0; 3]);
    let expected: Vec<[f64; 3]> = (0..5)
        .map(|k| c.normalise([points[0][k], points[1][k], points[2][k]]))
        .collect();
    assert_markers_at(&scene, scatter, &lens, &expected, what);
}

// Why: cylindrical axes place a point by its angle around the z axis, its distance from that axis and its height, so
// a scatter of known points must land where the definition puts them: angle 0 along +X, angles increasing
// anticlockwise seen from above, the lower radial limit on the axis and the upper on the rim of a cylinder of unit
// diameter, and the height between the floor and the top of the box. Each marker also carries the depth of its own
// point, which the depth buffer and the painter's sort rely on.
#[test]
fn a_point_in_cylindrical_axes_lands_at_its_angle_radius_and_height() {
    assert_cylinder_places(Cylinder::new(), View3d::default(), [THETA, R, Z], "default");
}

// Why: the polar view of cylindrical axes is the polar view of polar axes, so its zero direction and its direction of
// increasing angle turn the floor exactly as they turn a polar plot: with angle 0 along +Y (north seen from above) and
// clockwise angles, 90° lies along +X, as `φ = zero_deg − θ` gives.
#[test]
fn zero_deg_and_a_clockwise_angle_turn_the_cylinder_as_they_turn_a_polar_plot() {
    let mut c = Cylinder::new();
    c.polar.zero_deg = 90.0;
    c.theta.reverse = true;
    assert_cylinder_places(c, View3d::default(), [THETA, R, Z], "north, clockwise");
}

// Why: a hole leaves the centre of the cylinder empty, so the lower radial limit lies on the wall of the hole rather
// than on the axis, as it does on the edge of the hole in polar axes; a point at the lower limit must not collapse to
// the axis.
#[test]
fn with_a_hole_the_lower_radial_limit_lies_on_the_wall_of_the_hole() {
    let mut c = Cylinder::new();
    c.polar.hole = 0.25;
    let mut r = R;
    r[0] = 0.0;
    assert_cylinder_places(c, View3d::default(), [THETA, r, Z], "hole of a quarter");
}

// Why: a logarithmic radius places each decade at an equal step out from the lower limit on the axis, as in polar
// axes; a linear placement of the same data would bunch the small radii at the centre.
#[test]
fn on_a_logarithmic_radius_each_decade_takes_an_equal_step_out_from_the_axis() {
    let mut c = Cylinder::new();
    c.r = AxisSpec {
        log: true,
        ..AxisSpec::linear(1.0, 100.0)
    };
    assert_cylinder_places(
        c,
        View3d::default(),
        [THETA, [1.0, 3.0, 10.0, 50.0, 100.0], Z],
        "logarithmic radius",
    );
}

// Why: a reversed radius puts the upper limit on the axis and the lower on the rim, as MATLAB's `RDir` does in polar
// axes; nothing is reflected through the axis.
#[test]
fn a_reversed_radius_puts_the_upper_limit_on_the_axis() {
    let mut c = Cylinder::new();
    c.r.reverse = true;
    assert_cylinder_places(c, View3d::default(), [THETA, R, Z], "reversed radius");
}

// Why: manual angular limits choose a sector, not a stretch: an angle keeps its true direction in a partial sector,
// as it does in polar axes, so that a cylindrical axes made from a polar one by `make_3d` draws each angle where the
// polar plot drew it.
#[test]
fn in_a_partial_sector_each_angle_keeps_its_true_direction() {
    let mut c = Cylinder::new();
    c.theta = AxisSpec::linear(-60.0, 60.0);
    assert_cylinder_places(
        c,
        View3d::default(),
        [[-50.0, -10.0, 0.0, 30.0, 55.0], R, Z],
        "sector −60° to 60°",
    );
}

// Why: the angle units fix the length of a turn, so the same directions given in radians land in the same places as
// in degrees.
#[test]
fn angles_in_radians_land_where_the_same_angles_in_degrees_land() {
    let mut c = Cylinder::new();
    c.polar.angle_units = AngleUnits::Radians;
    c.theta = AxisSpec::linear(0.0, TAU);
    assert_cylinder_places(
        c,
        View3d::default(),
        [THETA.map(f64::to_radians), R, Z],
        "radians",
    );
}

// Why: cylindrical axes are rotated, zoomed and panned through their `view3d` exactly as Cartesian 3D axes are: the
// camera turns the box, the zoom scales the drawing about the centre of the plot rectangle and the pan moves it by
// fractions of the plot size. A point must follow all three.
#[test]
fn the_camera_zoom_and_pan_of_the_view_move_cylindrical_points_as_they_move_a_box() {
    let view = View3d {
        azimuth_deg: 60.0,
        elevation_deg: 20.0,
        zoom: 1.5,
        pan_x: 0.1,
        pan_y: -0.05,
    };
    assert_cylinder_places(
        Cylinder::new(),
        view,
        [THETA, R, Z],
        "turned, zoomed and panned",
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Where data points land: spherical axes
// ---------------------------------------------------------------------------------------------------------------------

const AZ: [f64; 5] = [0.0, 45.0, 135.0, 250.0, 300.0];
const ELEVATION: [f64; 5] = [-80.0, -30.0, 0.0, 40.0, 85.0];
const RADIUS: [f64; 5] = [0.5, 1.0, 1.5, 2.0, 0.25];

#[track_caller]
fn assert_sphere_places(s: Sphere, view3d: View3d, points: [[f64; 5]; 3], what: &str) {
    let (mut fx, ax) = sphere_axes(&s, view3d);
    let scatter = fx.scatter(ax, &points[0], &points[1], Some(&points[2]), |_, _| {});
    let scene = compile_clean(fx);
    assert_eq!(
        axes_hit(&scene, ax).kind,
        AxesHitKind::ThreeD,
        "{what}: spherical axes are navigated through their camera"
    );
    let lens = Lens::sphere(&scene, ax, view3d);
    let expected: Vec<[f64; 3]> = (0..5)
        .map(|k| s.normalise([points[0][k], points[1][k], points[2][k]]))
        .collect();
    assert_markers_at(&scene, scatter, &lens, &expected, what);
}

// Why: spherical axes place a point by its azimuth, its elevation above the X–Y plane and its distance from the
// centre, so that `surf(az, el, r(az, el))` draws an antenna balloon directly; a scatter of known points must land on
// the sphere of its radius at its two angles, with its own depth.
#[test]
fn a_point_in_spherical_axes_lands_at_its_azimuth_elevation_and_radius() {
    assert_sphere_places(
        Sphere::new(),
        View3d::default(),
        [AZ, ELEVATION, RADIUS],
        "elevation",
    );
}

// Why: physics and antenna texts measure the polar angle down from +Z (inclination) as often as up from the X–Y
// plane (elevation); with inclination 0 is the north pole and 90° the equator, so the same directions given as
// inclinations must land where their elevations land.
#[test]
fn an_inclination_is_measured_down_from_the_north_pole() {
    assert_sphere_places(
        Sphere::inclination(),
        View3d::default(),
        [AZ, ELEVATION.map(|e| 90.0 - e), RADIUS],
        "inclination",
    );
}

// Why: the radius of spherical axes is carried by z, so a reversed z puts the upper radial limit at the centre, and a
// logarithmic z steps out by decades, as a reversed or logarithmic radius does in polar axes.
#[test]
fn a_reversed_or_logarithmic_radius_on_z_is_placed_as_a_polar_radius_is() {
    let mut s = Sphere::new();
    s.r.reverse = true;
    assert_sphere_places(
        s,
        View3d::default(),
        [AZ, ELEVATION, RADIUS],
        "reversed radius",
    );
    let mut s = Sphere::new();
    s.r = AxisSpec {
        log: true,
        ..AxisSpec::linear(1.0, 100.0)
    };
    assert_sphere_places(
        s,
        View3d::default(),
        [AZ, ELEVATION, [1.0, 3.0, 10.0, 50.0, 100.0]],
        "logarithmic radius",
    );
}

// Why: the direction of the azimuth is the direction of the x axis, as in polar axes: a reversed x runs the azimuth
// clockwise seen from above, mirroring every point through the X–Z plane.
#[test]
fn a_reversed_azimuth_runs_clockwise_seen_from_above() {
    let mut s = Sphere::new();
    s.az.reverse = true;
    assert_sphere_places(
        s,
        View3d::default(),
        [AZ, ELEVATION, RADIUS],
        "clockwise azimuth",
    );
}

// Why: both angles of spherical axes are in the angle units of its view, so the same directions given in radians land
// in the same places as in degrees.
#[test]
fn both_spherical_angles_follow_the_angle_units() {
    let mut s = Sphere::new();
    s.spherical.angle_units = AngleUnits::Radians;
    s.az = AxisSpec::linear(0.0, TAU);
    s.polar = AxisSpec::linear(-FRAC_PI_2, FRAC_PI_2);
    assert_sphere_places(
        s,
        View3d::default(),
        [
            AZ.map(f64::to_radians),
            ELEVATION.map(f64::to_radians),
            RADIUS,
        ],
        "radians",
    );
}

// Why: spherical axes are navigated through `view3d` like any 3D axes.
#[test]
fn the_camera_zoom_and_pan_of_the_view_move_spherical_points_as_they_move_a_box() {
    let view = View3d {
        azimuth_deg: 120.0,
        elevation_deg: -15.0,
        zoom: 0.8,
        pan_x: -0.1,
        pan_y: 0.05,
    };
    assert_sphere_places(
        Sphere::new(),
        view,
        [AZ, ELEVATION, RADIUS],
        "turned, zoomed and panned",
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Artists that draw data directly: straight chords between mapped points
// ---------------------------------------------------------------------------------------------------------------------

// Why: a series draws exactly the points it is given and joins them with straight lines in every projection; a helix
// is drawn by supplying its points. A `plot3` from (0°, 1, −1) to (90°, 1, 1) is therefore one straight chord in 3D
// between the two mapped points, not a spiral round the cylinder, and the same holds for a chord through the inside
// of a sphere.
#[test]
fn a_series_in_cylindrical_or_spherical_axes_is_one_straight_chord_between_its_mapped_points() {
    let c = Cylinder::new();
    let (mut fx, ax) = cylinder_axes(&c, View3d::default());
    let line = fx.line(ax, &[0.0, 90.0], &[1.0, 1.0], Some(&[-1.0, 1.0]), |_| {});
    let scene = compile_clean(fx);
    let lens = Lens::cylinder(&scene, ax, View3d::default(), [1.0; 3]);
    let segments = segments_of(&scene, line);
    assert_eq!(segments.len(), 1, "one straight segment: {segments:?}");
    assert_at(
        segments[0].0,
        lens.project(c.normalise([0.0, 1.0, -1.0])).0,
        "the first point",
    );
    assert_at(
        segments[0].1,
        lens.project(c.normalise([90.0, 1.0, 1.0])).0,
        "the second point",
    );

    let s = Sphere::new();
    let (mut fx, ax) = sphere_axes(&s, View3d::default());
    let line = fx.line(ax, &[0.0, 90.0], &[0.0, 0.0], Some(&[2.0, 2.0]), |_| {});
    let scene = compile_clean(fx);
    let lens = Lens::sphere(&scene, ax, View3d::default());
    let segments = segments_of(&scene, line);
    assert_eq!(segments.len(), 1, "one straight segment: {segments:?}");
    assert_at(
        segments[0].0,
        lens.project(s.normalise([0.0, 0.0, 2.0])).0,
        "the first point",
    );
    assert_at(
        segments[0].1,
        lens.project(s.normalise([90.0, 0.0, 2.0])).0,
        "the second point",
    );
}

// Why: a surface over an angle–radius grid (a membrane mode, `surf(θ, r, z)`) is drawn with as many nodes as the user
// supplies; each face is the quadrilateral of its four mapped grid nodes, with no vertices added along its edges, in
// cylindrical axes as in polar ones. The same holds on a sphere, where a balloon face is the quadrilateral of its four
// mapped corners.
#[test]
fn a_surface_face_is_the_quadrilateral_of_its_four_mapped_nodes() {
    let check =
        |scene: &Scene, surf: NodeId, lens: &Lens, corners: Vec<[[f64; 3]; 4]>, what: &str| {
            let leaves = leaves(scene);
            let faces: Vec<Vec<Point>> = from_source(&leaves, surf)
                .iter()
                .filter(|l| l.path().is_some_and(|p| p.fill.is_some()) && l.move_to_count() == 1)
                .map(|l| l.subpaths().remove(0))
                .collect();
            assert_eq!(faces.len(), corners.len(), "{what}: one face per grid cell");
            for (k, cell) in corners.iter().enumerate() {
                let projected: Vec<Point> = cell.iter().map(|n| lens.project(*n).0).collect();
                let face = faces
                    .iter()
                    .find(|f| {
                        projected.iter().all(|c| {
                            f.iter()
                                .any(|p| (p.x - c.x).abs() <= TOL && (p.y - c.y).abs() <= TOL)
                        })
                    })
                    .unwrap_or_else(|| {
                        panic!("{what}: a face has the corners of cell {k}: {faces:?}")
                    });
                assert_eq!(
                    face.len(),
                    4,
                    "{what}: a face has only its four corners: {face:?}"
                );
            }
        };

    let c = Cylinder::new();
    let (theta, r) = ([0.0, 90.0, 180.0], [1.0, 2.0]);
    let height = |t: f64, r: f64| 0.2 * r + t / 360.0;
    let (mut fx, ax) = cylinder_axes(&c, View3d::default());
    let surf = fx.surface(ax, &theta, &r, height, |_| {});
    let scene = compile_clean(fx);
    let lens = Lens::cylinder(&scene, ax, View3d::default(), [1.0; 3]);
    let corners = (0..2)
        .map(|i| {
            [(i, 0), (i + 1, 0), (i + 1, 1), (i, 1)]
                .map(|(a, b)| c.normalise([theta[a], r[b], height(theta[a], r[b])]))
        })
        .collect();
    check(&scene, surf, &lens, corners, "cylindrical");

    let s = Sphere::new();
    let (az, el) = ([0.0, 90.0, 180.0], [0.0, 45.0]);
    let (mut fx, ax) = sphere_axes(&s, View3d::default());
    let surf = fx.surface(ax, &az, &el, |_, _| 1.5, |_| {});
    let scene = compile_clean(fx);
    let lens = Lens::sphere(&scene, ax, View3d::default());
    let corners = (0..2)
        .map(|i| {
            [(i, 0), (i + 1, 0), (i + 1, 1), (i, 1)].map(|(a, b)| s.normalise([az[a], el[b], 1.5]))
        })
        .collect();
    check(&scene, surf, &lens, corners, "spherical");
}

// Why: a quiver arrow runs from its base to the tip at base + (u, v, w) in data units, u an increment of angle, v of
// radius and w of height, and its shaft is the straight chord between the two mapped points: an arrow tangent to a
// helix points along the chord, not round the cylinder.
#[test]
fn a_quiver_shaft_is_the_straight_chord_from_its_mapped_base_to_its_mapped_tip() {
    let c = Cylinder::new();
    let (mut fx, ax) = cylinder_axes(&c, View3d::default());
    let quiver = fx.quiver(
        ax,
        &[0.0],
        &[1.0],
        Some(&[0.0]),
        &[90.0],
        &[0.0],
        Some(&[0.5]),
    );
    for artist in &mut fx.ax(ax).artists {
        if let Artist::Quiver(q) = artist {
            q.scale = QuiverScale::Off;
        }
    }
    let scene = compile_clean(fx);
    let lens = Lens::cylinder(&scene, ax, View3d::default(), [1.0; 3]);
    let base = lens.project(c.normalise([0.0, 1.0, 0.0])).0;
    let tip = lens.project(c.normalise([90.0, 1.0, 0.5])).0;
    let subpaths: Vec<Vec<Point>> = from_source(&leaves(&scene), quiver)
        .iter()
        .flat_map(Leaf::subpaths)
        .collect();
    let shaft = subpaths
        .iter()
        .find(|s| (s[0].x - base.x).abs() <= TOL && (s[0].y - base.y).abs() <= TOL)
        .unwrap_or_else(|| panic!("a subpath starts at the base: {subpaths:?}"));
    assert_eq!(
        shaft.len(),
        2,
        "the shaft is one straight segment: {shaft:?}"
    );
    assert_at(shaft[1], tip, "the tip");
}

// ---------------------------------------------------------------------------------------------------------------------
// Images: discs, walls and globes
// ---------------------------------------------------------------------------------------------------------------------

// Why: an image on the X–Y plane of cylindrical axes is a disc (or an annulus) at its height: each pixel is an annular
// sector between its own angle and radius edges, so every pixel edge is a lattice node placed by the axes, and the
// lattice is refined along the angle so that its chords follow the projected arcs. Each node carries its own depth,
// pushed back by the face bias so that lines and markers lying on the disc show, which lets a GPU depth-test the disc
// against surfaces passing through it.
#[test]
fn an_image_on_the_xy_plane_of_cylindrical_axes_is_a_disc_of_mapped_nodes_with_depths() {
    let c = Cylinder::new();
    let (mut fx, ax) = cylinder_axes(&c, View3d::default());
    // Three columns centred on 15°, 45° and 75° (edges 0°, 30°, 60°, 90°); two rows centred on 0.75 and 1.25 (edges
    // 0.5, 1 and 1.5); at height 0.5.
    let image = fx.imagesc(
        ax,
        vec![2, 3],
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
        placement(
            ImagePlane::Xy { z: Some(0.5) },
            range(15.0, 75.0),
            range(0.75, 1.25),
        ),
        |_| {},
    );
    let scene = compile_clean(fx);
    let lens = Lens::cylinder(&scene, ax, View3d::default(), [1.0; 3]);
    let (leaf, mesh) = mesh_of(&scene, image);
    assert_mesh_follows(
        &leaf,
        &mesh,
        (3, 2),
        &lens,
        |u, v| [30.0 * u, 0.5 + 0.5 * v, 0.5],
        |p| c.normalise(p),
        "a disc at height 0.5",
    );
}

// Why: an image on the X–Z plane of cylindrical axes (angle and height at a fixed radius) wraps the wall of the
// cylinder, as a panorama does; its nodes lie on the wall at the radius of the plane, at their angles and heights.
#[test]
fn an_image_on_the_xz_plane_of_cylindrical_axes_wraps_the_wall_at_its_radius() {
    let c = Cylinder::new();
    let (mut fx, ax) = cylinder_axes(&c, View3d::default());
    // Four columns of 90° (centres 45° to 315°) and two rows of height 1 (centres −0.5 and 0.5), on the wall of radius
    // 1.5.
    let image = fx.imagesc(
        ax,
        vec![2, 4],
        (0..8).map(f64::from).collect::<Vec<_>>(),
        placement(
            ImagePlane::Xz { y: Some(1.5) },
            range(45.0, 315.0),
            range(-0.5, 0.5),
        ),
        |_| {},
    );
    let scene = compile_clean(fx);
    let lens = Lens::cylinder(&scene, ax, View3d::default(), [1.0; 3]);
    let (leaf, mesh) = mesh_of(&scene, image);
    assert_mesh_follows(
        &leaf,
        &mesh,
        (4, 2),
        &lens,
        |u, v| [90.0 * u, 1.5, -1.0 + v],
        |p| c.normalise(p),
        "a wall at radius 1.5",
    );
}

// Why: an image on the X–Y plane of spherical axes (azimuth and polar angle at a fixed radius) wraps the sphere, a
// globe: every node lies on the sphere of the plane's radius at its two angles, with its own depth, and the lattice
// follows both the parallels and the meridians, whose projections are curved.
#[test]
fn an_image_on_the_xy_plane_of_spherical_axes_wraps_the_sphere_at_its_radius() {
    let s = Sphere::new();
    let (mut fx, ax) = sphere_axes(&s, View3d::default());
    // Eight columns of 45° (centres 22.5° to 337.5°) and four rows of 45° (centres −67.5° to 67.5°), at radius 2.
    let image = fx.imagesc(
        ax,
        vec![4, 8],
        (0..32).map(f64::from).collect::<Vec<_>>(),
        placement(
            ImagePlane::Xy { z: Some(2.0) },
            range(22.5, 337.5),
            range(-67.5, 67.5),
        ),
        |_| {},
    );
    let scene = compile_clean(fx);
    let lens = Lens::sphere(&scene, ax, View3d::default());
    let (leaf, mesh) = mesh_of(&scene, image);
    assert_mesh_follows(
        &leaf,
        &mesh,
        (8, 4),
        &lens,
        |u, v| [45.0 * u, -90.0 + 45.0 * v, 2.0],
        |p| s.normalise(p),
        "a globe of radius 2",
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Automatic limits
// ---------------------------------------------------------------------------------------------------------------------

/// Every vertex of every leaf of an artist, in figure space and paint order.
fn vertices(scene: &Scene, id: NodeId) -> Vec<Point> {
    from_source(&leaves(scene), id)
        .iter()
        .flat_map(|l| l.subpaths().into_iter().flatten())
        .collect()
}

#[track_caller]
fn assert_same_drawing(a: &[Point], b: &[Point], what: &str) {
    assert!(!a.is_empty(), "{what}: something is drawn");
    assert_eq!(a.len(), b.len(), "{what}: the same number of vertices");
    for (p, q) in a.iter().zip(b) {
        assert_at(*p, *q, what);
    }
}

/// The radial limits that polar axes choose automatically for radii `r`, read from the published polar hit geometry.
fn polar_radial_limits(r: &[f64]) -> (f64, f64) {
    let mut fx = Fx::new();
    let ax = fx.axes_in(
        cell(0, 0, 1, 1),
        Projection::Polar {
            polar: PolarView::default(),
            view: ironlab_ir::View2d::default(),
        },
    );
    let theta: Vec<f64> = (0..r.len()).map(|k| 30.0 * k as f64).collect();
    fx.line(ax, &theta, r, None, |_| {});
    let scene = compile_figure(&fx.build());
    match axes_hit(&scene, ax).kind {
        AxesHitKind::Polar { y, .. } => (y.min, y.max),
        ref other => panic!("polar axes publish a polar hit, not {other:?}"),
    }
}

// Why: the angle is periodic, so the automatic angular limits of cylindrical and spherical axes are the full turn
// whatever the data, and the automatic polar-angle limits of spherical axes are its whole range (a quarter turn
// either side of the equator for elevation, half a turn from the north pole for inclination): data over a few
// degrees must be drawn in its true direction, not stretched round the whole axes. The radius takes the automatic
// limits of a polar radius, from 0 for non-negative data. A figure with automatic limits must therefore be drawn
// exactly as with those limits given by hand. The largest radius is 10, a multiple of every nice tick step a target
// of three to ten intervals can choose, so that the upper radial limit does not depend on the tick target, which a
// 3D axes chooses from its tile and a polar axes from its radius.
#[test]
fn automatic_angle_limits_are_the_full_turn_and_the_full_polar_range() {
    let r = [2.0, 10.0];
    let (r_lo, r_hi) = polar_radial_limits(&r);
    assert_eq!(
        (r_lo, r_hi),
        (0.0, 10.0),
        "precondition: a polar radius of positive data starts at 0 and ends at the largest radius"
    );

    let cylinder = |manual: bool| {
        let mut fx = Fx::new();
        let ax = fx.axes_in(
            cell(0, 0, 1, 1),
            Projection::Cylindrical {
                view3d: View3d::default(),
                polar: PolarView::default(),
            },
        );
        let line = fx.line(ax, &[10.0, 50.0], &r, Some(&[-1.0, 1.0]), |_| {});
        let axes = fx.ax(ax);
        axes.z.limits = Limits::Manual {
            min: -1.0,
            max: 1.0,
        };
        if manual {
            axes.x.limits = Limits::Manual {
                min: 0.0,
                max: 360.0,
            };
            axes.y.limits = Limits::Manual {
                min: r_lo,
                max: r_hi,
            };
        }
        let scene = compile_figure(&fx.build());
        vertices(&scene, line)
    };
    assert_same_drawing(&cylinder(false), &cylinder(true), "cylindrical");

    for (polar_angle, polar_data, polar_range) in [
        (PolarAngle::Elevation, [10.0, 20.0], (-90.0, 90.0)),
        (PolarAngle::Inclination, [40.0, 60.0], (0.0, 180.0)),
    ] {
        let sphere = |manual: bool| {
            let mut fx = Fx::new();
            let ax = fx.axes_in(
                cell(0, 0, 1, 1),
                Projection::Spherical {
                    view3d: View3d::default(),
                    spherical: SphericalView {
                        polar_angle,
                        ..SphericalView::default()
                    },
                },
            );
            let line = fx.line(ax, &[10.0, 50.0], &polar_data, Some(&r), |_| {});
            if manual {
                let axes = fx.ax(ax);
                axes.x.limits = Limits::Manual {
                    min: 0.0,
                    max: 360.0,
                };
                axes.y.limits = Limits::Manual {
                    min: polar_range.0,
                    max: polar_range.1,
                };
                axes.z.limits = Limits::Manual {
                    min: r_lo,
                    max: r_hi,
                };
            }
            let scene = compile_figure(&fx.build());
            vertices(&scene, line)
        };
        assert_same_drawing(
            &sphere(false),
            &sphere(true),
            &format!("spherical, {polar_angle:?}"),
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Aspect
// ---------------------------------------------------------------------------------------------------------------------

// Why: a cylinder keeps its height-to-radius ratio when an aspect is set, so that a pipe or a tank is drawn in its
// true proportions: with `Aspect::Equal` a unit of height is drawn as long as a unit of radius. The disc keeps its
// unit diameter, on which the radial span is drawn from the hole to the rim, so a unit of radius is
// `0.5·(1 − hole)/span_r` of the diameter and the box takes sides `[1, 1, h]` with `h = (z/y)·span_z·0.5·(1 −
// hole)/span_r`, normalised so that the longest side is 1. A tall ratio makes the height the longest side.
#[test]
fn an_aspect_in_cylindrical_axes_keeps_the_height_to_radius_ratio() {
    for (aspect, hole, sides, what) in [
        (
            Aspect::Equal,
            0.0,
            [1.0, 1.0, 0.5],
            "equal, radius 0 to 2, height 0 to 2",
        ),
        (
            Aspect::Equal,
            0.5,
            [1.0, 1.0, 0.25],
            "equal with a hole of a half",
        ),
        (
            Aspect::Ratio { y: 1.0, z: 4.0 },
            0.0,
            [0.5, 0.5, 1.0],
            "a unit of height four times a unit of radius",
        ),
    ] {
        let mut c = Cylinder::new();
        c.polar.hole = hole;
        c.z = AxisSpec::linear(0.0, 2.0);
        let (mut fx, ax) = cylinder_axes(&c, View3d::default());
        fx.ax(ax).aspect = aspect;
        let scatter = fx.scatter(ax, &THETA, &R, Some(&[0.0, 0.3, 1.0, 1.6, 2.0]), |_, _| {});
        let scene = compile_clean(fx);
        let lens = Lens::cylinder(&scene, ax, View3d::default(), sides);
        let expected: Vec<[f64; 3]> = (0..5)
            .map(|k| c.normalise([THETA[k], R[k], [0.0, 0.3, 1.0, 1.6, 2.0][k]]))
            .collect();
        assert_markers_at(&scene, scatter, &lens, &expected, what);
    }
}

// Why: a spherical axes is a sphere whatever its aspect (validation warns that the aspect is ignored), so an aspect
// must not squash it into an ellipsoid: it is drawn exactly as with an automatic aspect.
#[test]
fn an_aspect_in_spherical_axes_changes_nothing_drawn() {
    let draw = |aspect: Aspect| {
        let (mut fx, ax) = sphere_axes(&Sphere::new(), View3d::default());
        fx.ax(ax).aspect = aspect;
        let line = fx.line(ax, &AZ, &ELEVATION, Some(&RADIUS), |_| {});
        vertices(&compile_figure(&fx.build()), line)
    };
    assert_same_drawing(
        &draw(Aspect::Ratio { y: 1.0, z: 3.0 }),
        &draw(Aspect::Auto),
        "a sphere with an aspect",
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Decorations
// ---------------------------------------------------------------------------------------------------------------------

/// A piece of a decoration path in figure space: a straight segment, or a cubic Bézier given by its four points.
#[derive(Clone, Copy, Debug)]
enum Piece {
    Line(Point, Point),
    Cubic(Point, Point, Point, Point),
}

impl Piece {
    fn ends(&self) -> (Point, Point) {
        match *self {
            Piece::Line(a, b) | Piece::Cubic(a, _, _, b) => (a, b),
        }
    }

    /// The on-curve point at the middle of a curved piece.
    fn middle(&self) -> Option<Point> {
        match *self {
            Piece::Line(..) => None,
            Piece::Cubic(a, b, c, d) => Some(Point::new(
                0.125 * (a.x + 3.0 * b.x + 3.0 * c.x + d.x),
                0.125 * (a.y + 3.0 * b.y + 3.0 * c.y + d.y),
            )),
        }
    }
}

/// Every piece of every path that an axes draws as its decoration, each with its position in paint order and whether
/// it is painted before the axes' depth group (`Some(true)`), after it (`Some(false)`), or in an axes without one
/// (`None`).
fn decoration_pieces(scene: &Scene, ax: NodeId) -> Vec<(Piece, Option<bool>)> {
    let grouped = grouped_leaves(scene);
    let first_group = grouped.iter().position(|(_, g)| g.is_some());
    let mut out = Vec::new();
    for (k, (leaf, group)) in grouped.iter().enumerate() {
        if leaf.source != Some(ax) || group.is_some() {
            continue;
        }
        let Some(path) = leaf.path() else { continue };
        let before = first_group.map(|f| k < f);
        let at = |p: Point| leaf.transform.apply(p);
        let (mut start, mut current) = (None, None);
        for seg in &path.segments {
            match *seg {
                PathSegment::MoveTo(p) => {
                    start = Some(at(p));
                    current = start;
                }
                PathSegment::LineTo(p) => {
                    if let Some(c) = current {
                        out.push((Piece::Line(c, at(p)), before));
                    }
                    current = Some(at(p));
                }
                PathSegment::CubicTo(b, c, p) => {
                    if let Some(a) = current {
                        out.push((Piece::Cubic(a, at(b), at(c), at(p)), before));
                    }
                    current = Some(at(p));
                }
                PathSegment::Close => {
                    if let (Some(s), Some(c)) = (start, current) {
                        out.push((Piece::Line(c, s), before));
                    }
                    current = start;
                }
            }
        }
    }
    out
}

/// A circle of the normalised box: its centre on the coordinate axis normal to its plane, its plane, and its radius.
#[derive(Clone, Copy, Debug)]
struct BoxCircle {
    /// The coordinate held fixed on the circle's plane (0 for x, 1 for y, 2 for z).
    normal: usize,
    /// The value of that coordinate.
    at: f64,
    radius: f64,
}

impl BoxCircle {
    /// The two in-plane coordinates of the circle, in the order that gives its angle.
    fn plane_axes(&self) -> (usize, usize) {
        match self.normal {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        }
    }

    /// The angle about the circle's centre of a point of its plane, and its distance from the centre.
    fn polar(&self, n: [f64; 3]) -> (f64, f64) {
        let (a, b) = self.plane_axes();
        (n[b].atan2(n[a]), n[a].hypot(n[b]))
    }
}

/// The pieces of the decoration of an axes that lie on `circle`: both ends of a straight piece within [`TOL`] of the
/// circle in figure space, and also the middle of a curved piece within [`CHORD`].
fn pieces_on(
    pieces: &[(Piece, Option<bool>)],
    lens: &Lens,
    circle: BoxCircle,
) -> Vec<(Piece, Option<bool>)> {
    // A radial error δ in the plane is drawn at most δ·scale long.
    let off = |p: Point| {
        let n = lens.on_plane(p, circle.normal, circle.at);
        (circle.polar(n).1 - circle.radius).abs() * lens.scale
    };
    pieces
        .iter()
        .filter(|(piece, _)| {
            let (a, b) = piece.ends();
            off(a) <= TOL && off(b) <= TOL && piece.middle().is_none_or(|m| off(m) <= CHORD)
        })
        .copied()
        .collect()
}

/// Whether the pieces on a circle together cover its whole turn, each piece covering the shorter arc between its ends.
fn covers_the_turn(pieces: &[(Piece, Option<bool>)], lens: &Lens, circle: BoxCircle) -> bool {
    let mut covered = [false; 360];
    let angle = |p: Point| circle.polar(lens.on_plane(p, circle.normal, circle.at)).0;
    for (piece, _) in pieces {
        let (a, b) = piece.ends();
        let (mut from, to) = (angle(a), angle(b));
        let mut sweep = (to - from).rem_euclid(TAU);
        if sweep > PI {
            from = to;
            sweep = TAU - sweep;
        }
        for (bin, slot) in covered.iter_mut().enumerate() {
            let centre = (bin as f64 + 0.5).to_radians();
            if (centre - from).rem_euclid(TAU) <= sweep {
                *slot = true;
            }
        }
    }
    covered.iter().all(|c| *c)
}

/// Whether the straight pieces lying along the figure-space segment from `a` to `b` together cover it.
fn covers_segment(pieces: &[(Piece, Option<bool>)], a: Point, b: Point) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length2 = dx * dx + dy * dy;
    let length = length2.sqrt();
    let param = |p: Point| ((p.x - a.x) * dx + (p.y - a.y) * dy) / length2;
    let off = |p: Point| ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / length;
    let mut spans: Vec<(f64, f64)> = pieces
        .iter()
        .filter_map(|(piece, _)| match *piece {
            Piece::Line(p, q) if off(p) <= TOL && off(q) <= TOL => {
                let (s, t) = (param(p), param(q));
                Some((s.min(t), s.max(t)))
            }
            _ => None,
        })
        .collect();
    spans.sort_by(|x, y| x.0.total_cmp(&y.0));
    let slack = TOL / length;
    let mut reach = 0.0;
    for (s, t) in spans {
        if s > reach + slack {
            break;
        }
        reach = f64::max(reach, t);
    }
    reach >= 1.0 - slack
}

/// Cylindrical axes holding a surface across the middle of the cylinder, so that the axes has a depth group for its
/// decorations to be painted behind and in front of.
fn decorated_cylinder(edit: impl FnOnce(&mut Axes)) -> (Scene, NodeId, Lens, Cylinder) {
    let c = Cylinder::new();
    let (mut fx, ax) = cylinder_axes(&c, View3d::default());
    fx.surface(
        ax,
        &[0.0, 120.0, 240.0, 360.0],
        &[0.0, 2.0],
        |_, _| 0.0,
        |_| {},
    );
    edit(fx.ax(ax));
    let scene = compile_clean(fx);
    let lens = Lens::cylinder(&scene, ax, View3d::default(), [1.0; 3]);
    (scene, ax, lens, c)
}

// Why: the floor of cylindrical axes carries the polar frame at the lower height limit: the rim circle bounds the
// floor, so a whole circle of unit diameter lies in the plane of the lower z limit. Drawn through the depth group's
// projection, its far half is painted behind the artists and its near half in front of them, as the edges of a
// Cartesian box are: a rim segment painted behind is not wholly in front of the centre of the box, and one painted in
// front is not wholly behind it.
#[test]
fn the_rim_of_cylindrical_axes_is_a_whole_circle_on_the_floor_split_behind_and_in_front() {
    let (scene, ax, lens, _) = decorated_cylinder(|_| {});
    let rim = BoxCircle {
        normal: 2,
        at: -0.5,
        radius: 0.5,
    };
    let pieces = pieces_on(&decoration_pieces(&scene, ax), &lens, rim);
    assert!(
        covers_the_turn(&pieces, &lens, rim),
        "the rim is a whole circle on the floor: {pieces:?}"
    );
    let depth = |p: Point| lens.project(lens.on_plane(p, 2, -0.5)).1;
    let (mut behind, mut in_front) = (0, 0);
    for (piece, before) in &pieces {
        let (a, b) = piece.ends();
        match before {
            Some(true) => {
                behind += 1;
                assert!(
                    depth(a).min(depth(b)) <= DEPTH_TOL,
                    "a rim piece painted behind the artists is not wholly in front: {piece:?}"
                );
            }
            Some(false) => {
                in_front += 1;
                assert!(
                    depth(a).max(depth(b)) >= -DEPTH_TOL,
                    "a rim piece painted in front of the artists is not wholly behind: {piece:?}"
                );
            }
            None => panic!("the axes has a depth group"),
        }
    }
    assert!(
        behind > 0 && in_front > 0,
        "the rim is split: {behind} behind, {in_front} in front"
    );
}

/// Asserts that an interior decoration line runs from the box point `a` to the box point `b`: straight pieces of the
/// axes' own paths inside its depth group cover the projected segment, and every vertex of each of them carries the
/// depth of its own point of the segment, so that a depth test hides the line behind the artists in front of it. Such
/// a line must not also be drawn outside the depth group, where it would be painted over those artists.
#[track_caller]
fn assert_interior_line(
    scene: &Scene,
    ax: NodeId,
    lens: &Lens,
    a: [f64; 3],
    b: [f64; 3],
    what: &str,
) {
    let ((pa, da), (pb, db)) = (lens.project(a), lens.project(b));
    let mut pieces = Vec::new();
    for (leaf, group) in grouped_leaves(scene) {
        if leaf.source != Some(ax) || group.is_none() {
            continue;
        }
        let Some(path) = leaf.path() else { continue };
        let Some(Depth::Vertices(depths)) = &path.depth else {
            continue;
        };
        let mut previous: Option<(Point, f64)> = None;
        let mut k = 0;
        for seg in &path.segments {
            match *seg {
                PathSegment::MoveTo(p) => {
                    previous = Some((leaf.transform.apply(p), depths[k]));
                    k += 1;
                }
                PathSegment::LineTo(p) => {
                    let here = (leaf.transform.apply(p), depths[k]);
                    if let Some(from) = previous {
                        pieces.push((from, here));
                    }
                    previous = Some(here);
                    k += 1;
                }
                PathSegment::CubicTo(..) | PathSegment::Close => previous = None,
            }
        }
    }
    let (dx, dy) = (pb.x - pa.x, pb.y - pa.y);
    let length2 = dx * dx + dy * dy;
    let off = |p: Point| ((p.x - pa.x) * dy - (p.y - pa.y) * dx).abs() / length2.sqrt();
    let along: Vec<_> = pieces
        .into_iter()
        .filter(|((p, _), (q, _))| off(*p) <= TOL && off(*q) <= TOL)
        .collect();
    let as_pieces: Vec<(Piece, Option<bool>)> = along
        .iter()
        .map(|((p, _), (q, _))| (Piece::Line(*p, *q), None))
        .collect();
    assert!(
        covers_segment(&as_pieces, pa, pb),
        "{what}: interior pieces in the depth group cover the line from {pa:?} to {pb:?}"
    );
    for (p, depth) in along.iter().flat_map(|(p, q)| [*p, *q]) {
        let t = ((p.x - pa.x) * dx + (p.y - pa.y) * dy) / length2;
        assert_depth(
            depth,
            da + t * (db - da),
            &format!("{what}: a vertex at {p:?}"),
        );
    }
    assert!(
        !covers_segment(&decoration_pieces(scene, ax), pa, pb),
        "{what}: the line is not also painted outside the depth group"
    );
}

// Why: the z axis of cylindrical axes is the axis of the cylinder, so it runs up the centre from the floor to the top
// of the box, where its ticks and labels are read; a z axis on a corner of a Cartesian box would stand outside the
// cylinder. It passes through the drawn volume, so the artists in front of it must hide it: it is drawn among them,
// with a depth at every vertex, rather than painted over them.
#[test]
fn the_z_axis_of_cylindrical_axes_runs_up_the_centre_from_floor_to_top() {
    let (scene, ax, lens, _) = decorated_cylinder(|_| {});
    assert_interior_line(
        &scene,
        ax,
        &lens,
        [0.0, 0.0, -0.5],
        [0.0, 0.0, 0.5],
        "the z axis",
    );
}

// Why: the angular grid of the floor is the spokes at the angular ticks, placed by the same polar view as the data,
// so that a point at 90° lies on the 90° spoke whatever the zero direction and the direction of the angle. The
// automatic angular ticks of a full turn in degrees are its eight equal divisions. The spokes lie on the floor of the
// drawn volume, so an artist standing over them hides them, as a depth test decides.
#[test]
fn the_floor_spokes_lie_at_the_angular_ticks_turned_by_the_polar_view() {
    let (scene, ax, lens, mut c) = decorated_cylinder(|axes| {
        axes.x.grid = true;
        axes.x.reverse = true;
        if let Projection::Cylindrical { polar, .. } = &mut axes.projection {
            polar.zero_deg = 90.0;
        }
    });
    c.polar.zero_deg = 90.0;
    c.theta.reverse = true;
    for k in 0..8 {
        let theta = 45.0 * f64::from(k);
        assert_interior_line(
            &scene,
            ax,
            &lens,
            [0.0, 0.0, -0.5],
            c.normalise([theta, 2.0, -1.0]),
            &format!("the spoke at {theta}°"),
        );
    }
}

// Why: the radial axis of spherical axes runs from the centre of the sphere along azimuth 0, through the inside of
// whatever the axes draws; drawn over the artists, it would show through an opaque balloon or globe. It is drawn among
// them instead, with a depth at every vertex, as the spokes of the grid are.
#[test]
fn the_radial_axis_and_spokes_of_spherical_axes_are_hidden_by_the_artists_in_front() {
    let s = Sphere::new();
    let (mut fx, ax) = sphere_axes(&s, View3d::default());
    fx.line(ax, &AZ, &ELEVATION, Some(&RADIUS), |_| {});
    let axes = fx.ax(ax);
    axes.x.grid = true;
    axes.y.grid = true;
    let scene = compile_clean(fx);
    let lens = Lens::sphere(&scene, ax, View3d::default());
    assert_interior_line(
        &scene,
        ax,
        &lens,
        [0.0; 3],
        [0.5, 0.0, 0.0],
        "the radial axis",
    );
    assert_interior_line(
        &scene,
        ax,
        &lens,
        [0.0; 3],
        s.normalise([90.0, 0.0, 2.0]),
        "the azimuth spoke at 90°",
    );
    assert_interior_line(
        &scene,
        ax,
        &lens,
        [0.0; 3],
        s.normalise([0.0, 60.0, 2.0]),
        "the polar-angle spoke at 60°",
    );
}

// Why: `box_` closes the frame of cylindrical axes with the rim of the top, as it closes a Cartesian box with its top
// edges; without it the top is left open so that the data inside can be seen.
#[test]
fn the_top_rim_of_cylindrical_axes_is_drawn_only_with_the_box() {
    let top = BoxCircle {
        normal: 2,
        at: 0.5,
        radius: 0.5,
    };
    for box_ in [true, false] {
        let (scene, ax, lens, _) = decorated_cylinder(|axes| axes.box_ = box_);
        let pieces = pieces_on(&decoration_pieces(&scene, ax), &lens, top);
        assert_eq!(
            covers_the_turn(&pieces, &lens, top),
            box_,
            "the top rim is drawn when box_ is {box_}: {pieces:?}"
        );
    }
}

/// The two azimuths at which the wall of a cylinder is tangent to the viewing direction, as the lens draws it. The camera
/// keeps the z axis upright on the page, so the wall's silhouette lines stand where the rim reaches furthest left and
/// furthest right: the page x of the rim point at azimuth φ is `x_c + A·cos φ + B·sin φ`, extreme at `atan2(B, A)` and
/// half a turn from it.
fn silhouette_azimuths(lens: &Lens) -> [f64; 2] {
    let x = |phi: f64| lens.project([0.5 * phi.cos(), 0.5 * phi.sin(), -0.5]).0.x;
    let centre = lens.project([0.0, 0.0, -0.5]).0.x;
    let (a, b) = (x(0.0) - centre, x(FRAC_PI_2) - centre);
    let phi = b.atan2(a);
    [phi, phi + PI]
}

/// Whether the decoration of an axes painted outside its depth group draws the vertical line of the cylinder wall at
/// azimuth `phi`, from the floor to the top.
fn draws_wall_line(scene: &Scene, ax: NodeId, lens: &Lens, phi: f64) -> bool {
    let at = |z: f64| lens.project([0.5 * phi.cos(), 0.5 * phi.sin(), z]).0;
    covers_segment(&decoration_pieces(scene, ax), at(-0.5), at(0.5))
}

// Why: with the box on, the frame of cylindrical axes is the whole outline of the cylinder: the rims of the floor and
// the top and the two lines of the wall where it turns away from the viewer, which join the rims tangentially, so that
// the cylinder reads as a solid, as the twelve edges of a Cartesian box do. The silhouette follows the camera. With the
// box off, the wall lines go with the top rim, and the floor alone is left.
#[test]
fn with_the_box_on_cylindrical_axes_draw_the_silhouette_lines_of_the_wall() {
    for view in [
        View3d::default(),
        View3d {
            azimuth_deg: 70.0,
            elevation_deg: 15.0,
            ..View3d::default()
        },
    ] {
        for box_ in [true, false] {
            let c = Cylinder::new();
            let (mut fx, ax) = cylinder_axes(&c, view);
            fx.surface(
                ax,
                &[0.0, 120.0, 240.0, 360.0],
                &[0.0, 2.0],
                |_, _| 0.0,
                |_| {},
            );
            fx.ax(ax).box_ = box_;
            let scene = compile_clean(fx);
            let lens = Lens::cylinder(&scene, ax, view, [1.0; 3]);
            for phi in silhouette_azimuths(&lens) {
                assert_eq!(
                    draws_wall_line(&scene, ax, &lens, phi),
                    box_,
                    "{view:?}, box {box_}: the wall line at {:.1}°",
                    phi.to_degrees()
                );
            }
        }
    }
}

// Why: a partial sector is a slice of the cylinder, whose outline is the wall only where the slice has a wall: a
// silhouette line is drawn only at an azimuth inside the sector, beside the vertical edges of the sector, which close
// the slice. With the box off, every vertical line of the frame goes.
#[test]
fn a_partial_sector_draws_only_the_silhouette_lines_within_it() {
    let view = View3d::default();
    for box_ in [true, false] {
        let mut c = Cylinder::new();
        c.theta = AxisSpec::linear(0.0, 180.0);
        let (mut fx, ax) = cylinder_axes(&c, view);
        fx.surface(ax, &[0.0, 90.0, 180.0], &[0.0, 2.0], |_, _| 0.0, |_| {});
        fx.ax(ax).box_ = box_;
        let scene = compile_clean(fx);
        let lens = Lens::cylinder(&scene, ax, view, [1.0; 3]);
        let inside = |phi: f64| phi.rem_euclid(TAU) < PI;
        let azimuths = silhouette_azimuths(&lens);
        assert!(
            azimuths.iter().any(|a| inside(*a)) && azimuths.iter().any(|a| !inside(*a)),
            "precondition: one silhouette azimuth lies in the half-turn sector"
        );
        for phi in azimuths {
            assert_eq!(
                draws_wall_line(&scene, ax, &lens, phi),
                box_ && inside(phi),
                "box {box_}: the wall line at {:.1}°",
                phi.to_degrees()
            );
        }
        for phi in [0.0, PI] {
            assert_eq!(
                draws_wall_line(&scene, ax, &lens, phi),
                box_,
                "box {box_}: the sector edge at {:.0}°",
                phi.to_degrees()
            );
        }
    }
}

// Why: the frame of spherical axes is two great circles of the outer sphere: the equator in the X–Y plane, which
// carries the azimuth ticks, and the meridian of azimuth 0 in the X–Z plane, which carries the polar-angle ticks.
// Each must be a whole circle of the rim radius in its own plane, or the angles read off it would be wrong.
#[test]
fn the_equator_and_the_meridian_of_spherical_axes_are_whole_circles_in_their_planes() {
    let (mut fx, ax) = sphere_axes(&Sphere::new(), View3d::default());
    fx.line(ax, &AZ, &ELEVATION, Some(&RADIUS), |_| {});
    let scene = compile_clean(fx);
    let lens = Lens::sphere(&scene, ax, View3d::default());
    let pieces = decoration_pieces(&scene, ax);
    for (circle, what) in [
        (
            BoxCircle {
                normal: 2,
                at: 0.0,
                radius: 0.5,
            },
            "the equator in the X–Y plane",
        ),
        (
            BoxCircle {
                normal: 1,
                at: 0.0,
                radius: 0.5,
            },
            "the meridian in the X–Z plane",
        ),
    ] {
        let on = pieces_on(&pieces, &lens, circle);
        assert!(
            covers_the_turn(&on, &lens, circle),
            "{what} is a whole circle: {on:?}"
        );
    }
}

/// Asserts that every text the axes draws (its tick labels, since the fixtures have no axis labels or title) lies
/// inside its plot rectangle.
#[track_caller]
fn every_tick_label_inside_the_plot(scene: &Scene, ax: NodeId, what: &str) {
    let plot = axes_hit(scene, ax).plot_rect;
    let runs: Vec<Leaf> = leaves(scene)
        .into_iter()
        .filter(|l| l.source == Some(ax) && l.glyphs().is_some())
        .collect();
    assert!(!runs.is_empty(), "{what}: tick labels are drawn");
    for run in runs {
        let b = run.bbox().expect("a glyph run has a box");
        assert!(
            b.x >= plot.x - TOL
                && b.right() <= plot.right() + TOL
                && b.y >= plot.y - TOL
                && b.bottom() <= plot.bottom() + TOL,
            "{what}: the label {:?} at {b:?} lies inside the plot rectangle {plot:?}",
            run.glyphs().map(|g| &g.text)
        );
    }
}

// Why: the tick labels of cylindrical and spherical axes lie outside the frame circles, beyond the sphere that
// circumscribes the drawing, so the fit must leave room for them, or they would run into the title, the margins of
// the tile and the axes beside it. The room depends on the labels and not on the view, so in every view, turned or
// not, every tick label stays inside the plot rectangle.
#[test]
fn the_fit_leaves_room_inside_the_plot_for_the_tick_labels_round_the_frame() {
    for view in [
        View3d::default(),
        View3d {
            azimuth_deg: 70.0,
            elevation_deg: 80.0,
            ..View3d::default()
        },
        View3d {
            azimuth_deg: 160.0,
            elevation_deg: -10.0,
            ..View3d::default()
        },
    ] {
        let c = Cylinder::new();
        let (mut fx, ax) = cylinder_axes(&c, view);
        fx.ax(ax).x.tick_labels = TickLabels::Auto;
        fx.scatter(ax, &THETA, &R, Some(&Z), |_, _| {});
        let scene = compile_figure(&fx.build());
        every_tick_label_inside_the_plot(&scene, ax, &format!("cylinder at {view:?}"));

        let s = Sphere::new();
        let (mut fx, ax) = sphere_axes(&s, view);
        fx.ax(ax).x.tick_labels = TickLabels::Auto;
        fx.ax(ax).y.tick_labels = TickLabels::Auto;
        fx.scatter(ax, &AZ, &ELEVATION, Some(&RADIUS), |_, _| {});
        let scene = compile_figure(&fx.build());
        every_tick_label_inside_the_plot(&scene, ax, &format!("sphere at {view:?}"));
    }
}

// Why: a tick label on the far side of the frame may be covered by the artists in front of it, which is right only
// when an artist can cover all of it: a label cut in part by the edge of a globe or a balloon reads as a broken label.
// So a label is painted behind the artists only when it lies wholly inside the silhouette of the sphere; one that
// reaches outside it is painted in front. A globe that fills the sphere has labels of both kinds at the default view.
#[test]
fn a_far_side_tick_label_is_painted_behind_the_artists_only_inside_the_silhouette() {
    let s = Sphere::new();
    let (mut fx, ax) = sphere_axes(&s, View3d::default());
    fx.ax(ax).x.tick_labels = TickLabels::Auto;
    fx.ax(ax).y.tick_labels = TickLabels::Auto;
    fx.imagesc(
        ax,
        vec![4, 8],
        (0..32).map(f64::from).collect::<Vec<_>>(),
        placement(
            ImagePlane::Xy { z: Some(2.0) },
            range(22.5, 337.5),
            range(-67.5, 67.5),
        ),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let grouped = grouped_leaves(&scene);
    let first_group = grouped
        .iter()
        .position(|(_, g)| g.is_some())
        .expect("the globe is drawn in a depth group");
    // The silhouette of the sphere, rebuilt from the centre of the plot rectangle and the radius the drawing reaches:
    // the globe's own lattice, whose nodes lie on the sphere, gives that radius as the farthest node from the centre.
    let centre = Point::new(plot.x + plot.width / 2.0, plot.y + plot.height / 2.0);
    let radius = grouped
        .iter()
        .filter_map(|(l, _)| match l.image().map(|i| &i.geometry) {
            Some(ImageGeometry::Mesh(mesh)) => Some(
                mesh.points
                    .iter()
                    .map(|p| (p.x - centre.x).hypot(p.y - centre.y))
                    .fold(0.0, f64::max),
            ),
            _ => None,
        })
        .fold(0.0, f64::max);
    assert!(radius > 0.0, "precondition: the globe is drawn");
    let inside = |b: ironlab_scene::display::Rect| {
        [
            Point::new(b.x, b.y),
            Point::new(b.right(), b.y),
            Point::new(b.x, b.bottom()),
            Point::new(b.right(), b.bottom()),
        ]
        .iter()
        .all(|p| (p.x - centre.x).hypot(p.y - centre.y) <= radius + 1e-6)
    };
    let mut behind = 0;
    let mut outside = 0;
    for (k, (leaf, group)) in grouped.iter().enumerate() {
        if leaf.source != Some(ax) || group.is_some() || leaf.glyphs().is_none() {
            continue;
        }
        let b = leaf.bbox().expect("a glyph run has a box");
        if k < first_group {
            behind += 1;
            assert!(
                inside(b),
                "the label {:?} painted behind the globe lies wholly inside its silhouette: {b:?}",
                leaf.glyphs().map(|g| &g.text)
            );
        } else if !inside(b) {
            outside += 1;
        }
    }
    assert!(behind > 0, "some far-side label is hidden behind the globe");
    assert!(
        outside > 0,
        "labels outside the silhouette are painted in front"
    );
}

/// Spherical axes holding a line, with its polar-angle ticks every 30° and its polar-angle grid on, measuring the polar
/// angle as `s` measures it. With `labels`, the polar-angle tick labels are shown and the radial ones hidden; without,
/// every tick label is hidden, so that the fit takes no room for them and [`Lens`] places points exactly.
fn meridian_fixture(s: &Sphere, labels: bool) -> (Scene, NodeId, Lens) {
    let (mut fx, ax) = sphere_axes(s, View3d::default());
    fx.line(ax, &AZ, &ELEVATION, Some(&RADIUS), |_| {});
    let axes = fx.ax(ax);
    axes.y.ticks =
        ironlab_ir::Ticks::Values((0..7).map(|k| s.polar.lo + 30.0 * f64::from(k)).collect());
    if labels {
        axes.y.tick_labels = TickLabels::Auto;
        axes.z.tick_labels = TickLabels::Hidden;
    }
    axes.y.grid = true;
    let scene = compile_clean(fx);
    let lens = Lens::sphere(&scene, ax, View3d::default());
    (scene, ax, lens)
}

// Why: the meridian is a whole circle, and a reader reads a polar angle off whichever side of it is nearer the data,
// so its polar-angle ticks go all the way round: on the half at azimuth 0 and, mirrored, on the half at azimuth 180°,
// with the same values. With elevation, the labels read 0 at +X, up through 30 and 60 to 90 at the top, then 60, 30 and
// 0 at −X and down to −90 at the bottom. The two halves meet at the poles, whose labels are drawn once. Each label lies
// beyond its own point of the meridian, in its direction from the centre of the sphere rather than in the direction of
// the point at the same angle on the other half; directions are compared because the labels take room from the fit, so
// that [`Lens`] scales the sphere slightly larger than it is drawn, about the same centre. When the y grid is on, a
// spoke runs out to every tick of both halves.
#[test]
fn the_polar_angle_ticks_of_spherical_axes_go_all_the_way_round_the_meridian() {
    for (s, poles) in [
        (Sphere::new(), [-90.0, 90.0]),
        (Sphere::inclination(), [0.0, 180.0]),
    ] {
        let (scene, ax, lens) = meridian_fixture(&s, true);
        let leaves = leaves(&scene);
        let runs: Vec<(String, Point)> = from_source(&leaves, ax)
            .iter()
            .filter_map(|l| {
                let b = l.bbox()?;
                let text = l.glyphs()?.text.clone();
                Some((text, Point::new(b.x + b.width / 2.0, b.y + b.height / 2.0)))
            })
            .collect();
        let value = |text: &str| {
            text.replace('\u{2212}', "-")
                .trim_end_matches('°')
                .parse::<f64>()
                .ok()
        };
        let centre = lens.project([0.0; 3]).0;
        // The cosine of the angle at the centre of the sphere between the page points `a` and `b`.
        let cos = |a: Point, b: Point| {
            let (u, v) = (
                (a.x - centre.x, a.y - centre.y),
                (b.x - centre.x, b.y - centre.y),
            );
            (u.0 * v.0 + u.1 * v.1) / (u.0.hypot(u.1) * v.0.hypot(v.1))
        };
        let (spokes, spokes_ax, exact) = meridian_fixture(&s, false);
        for k in 0..7 {
            let angle = s.polar.lo + 30.0 * f64::from(k);
            let drawn: Vec<Point> = runs
                .iter()
                .filter(|(t, _)| value(t) == Some(angle))
                .map(|(_, c)| *c)
                .collect();
            if poles.contains(&angle) {
                assert_eq!(
                    drawn.len(),
                    1,
                    "the pole {angle} is labelled once: {runs:?}"
                );
                continue;
            }
            assert_eq!(
                drawn.len(),
                2,
                "{angle} is labelled on both halves: {runs:?}"
            );
            let near = lens.project(s.normalise([0.0, angle, 2.0])).0;
            let far = lens.project(s.normalise([180.0, angle, 2.0])).0;
            assert!(
                drawn.iter().any(|c| cos(*c, near) > cos(*c, far))
                    && drawn.iter().any(|c| cos(*c, far) > cos(*c, near)),
                "{angle}: one label beside each half of the meridian: {drawn:?}"
            );
            for azimuth in [0.0, 180.0] {
                assert_interior_line(
                    &spokes,
                    spokes_ax,
                    &exact,
                    [0.0; 3],
                    s.normalise([azimuth, angle, 2.0]),
                    &format!("the polar-angle spoke at {angle} on the half at azimuth {azimuth}°"),
                );
            }
        }
    }
}
