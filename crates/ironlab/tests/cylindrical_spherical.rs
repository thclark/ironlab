//! Cylindrical and spherical axes in the facade: creating and converting them, choosing the
//! polar-angle convention, the conversion of polar axes to cylindrical axes by every 3D builder,
//! and the existing 3D builders drawing into both unchanged.
//!
//! These tests assume this API:
//!
//! - `Figure::cylindrical_axes(row, col)` and `Figure::spherical_axes(row, col)`, which create the
//!   tile's axes, or convert it, to `Projection::Cylindrical` or `Projection::Spherical`; an axes
//!   that already has the projection keeps its views, as `axes3` and `polar_axes` do.
//! - `AxesMut::make_cylindrical()` and `AxesMut::make_spherical()`, which convert an axes keeping
//!   its artists, its camera (`view3d`) when it has one, and its polar view when it has one and
//!   becomes cylindrical.
//! - `AxesMut::polar_angle(PolarAngle)`, which sets the polar-angle convention of a spherical axes,
//!   converting the axes to spherical axes first, as the setters of the polar view convert to
//!   polar axes.
//! - `PolarAngle` and `SphericalView` in the prelude.

mod common;

use common::axes;
use ironlab::ir::{Artist, Projection, View3d};
use ironlab::prelude::*;

/// Returns the camera and polar view of a cylindrical axes, panicking if it is not cylindrical.
fn cylindrical(fig: &Figure, id: NodeId) -> (View3d, PolarView) {
    match axes(fig, id).projection {
        Projection::Cylindrical { view3d, polar } => (view3d, polar),
        other => panic!("expected cylindrical axes, found {other:?}"),
    }
}

/// Returns the camera and spherical view of a spherical axes, panicking if it is not spherical.
fn spherical(fig: &Figure, id: NodeId) -> (View3d, SphericalView) {
    match axes(fig, id).projection {
        Projection::Spherical { view3d, spherical } => (view3d, spherical),
        other => panic!("expected spherical axes, found {other:?}"),
    }
}

/// A view other than the default, by which a kept camera is told from a reset one.
const TURNED: (f64, f64) = (60.0, 15.0);

// WHY: `cylindrical_axes` and `spherical_axes` are the entry points of every cylindrical and
// spherical figure; each must produce the IR projection that the compiler draws as such, with the
// default camera and the documented default views (degrees, angle 0 along +X, no hole;
// elevation).
#[test]
fn cylindrical_and_spherical_axes_create_their_projections_with_default_views() {
    let mut fig = Figure::new().tiles(1, 2);
    let c = fig.cylindrical_axes(0, 0).id();
    let s = fig.spherical_axes(0, 1).id();
    assert_eq!(
        cylindrical(&fig, c),
        (View3d::default(), PolarView::default())
    );
    assert_eq!(
        spherical(&fig, s),
        (View3d::default(), SphericalView::default())
    );
    assert_eq!(SphericalView::default().polar_angle, PolarAngle::Elevation);
}

// WHY: calling the constructor again for the same tile returns the same axes; it must not reset
// a camera or a view the user has already set, or the order of calls would matter.
#[test]
fn calling_a_constructor_again_keeps_the_axes_and_its_views() {
    let mut fig = Figure::new().tiles(1, 2);
    let c = fig
        .cylindrical_axes(0, 0)
        .view(TURNED.0, TURNED.1)
        .theta_zero(ThetaZero::North)
        .id();
    assert_eq!(fig.cylindrical_axes(0, 0).id(), c);
    let (view3d, polar) = cylindrical(&fig, c);
    assert_eq!(
        (view3d.azimuth_deg, view3d.elevation_deg),
        TURNED,
        "the camera is kept"
    );
    assert_eq!(polar.zero_deg, 90.0, "the polar view is kept");

    let s = fig
        .spherical_axes(0, 1)
        .view(TURNED.0, TURNED.1)
        .polar_angle(PolarAngle::Inclination)
        .id();
    assert_eq!(fig.spherical_axes(0, 1).id(), s);
    let (view3d, view) = spherical(&fig, s);
    assert_eq!((view3d.azimuth_deg, view3d.elevation_deg), TURNED);
    assert_eq!(view.polar_angle, PolarAngle::Inclination);
}

// WHY: a user who has plotted into a tile and then asks for cylindrical or spherical axes expects
// the data to be redrawn in the new coordinates, not discarded; and a user who has turned the
// camera of a 3D axes expects to keep looking from where they were.
#[test]
fn converting_keeps_the_artists_and_the_camera() {
    let mut fig = Figure::new().tiles(1, 2);
    let a = fig.axes3(0, 0).view(TURNED.0, TURNED.1).id();
    let line = fig
        .axes(0, 0)
        .plot3([0.0, 90.0], [1.0, 2.0], [0.0, 1.0])
        .id();
    fig.axes(0, 0).make_cylindrical();
    let (view3d, polar) = cylindrical(&fig, a);
    assert_eq!((view3d.azimuth_deg, view3d.elevation_deg), TURNED);
    assert_eq!(polar, PolarView::default());
    assert_eq!(axes(&fig, a).artists.len(), 1);
    assert_eq!(axes(&fig, a).artists[0].id(), line);

    fig.axes(0, 0).make_spherical();
    let (view3d, _) = spherical(&fig, a);
    assert_eq!(
        (view3d.azimuth_deg, view3d.elevation_deg),
        TURNED,
        "the camera survives a change from cylindrical to spherical"
    );
    assert_eq!(axes(&fig, a).artists.len(), 1);

    let b = fig.axes(0, 1).id();
    fig.axes(0, 1).plot([0.0, 90.0], [1.0, 2.0]);
    assert_eq!(fig.spherical_axes(0, 1).id(), b);
    assert_eq!(spherical(&fig, b).0, View3d::default());
    assert_eq!(axes(&fig, b).artists.len(), 1);
}

// WHY: a polar plot becomes cylindrical when it is given a third dimension, and the angles of its
// data must keep the directions the polar plot gave them: the zero direction, the direction of the
// angle and the hole are the polar view, which the cylindrical axes must carry over, whether the
// conversion is asked for explicitly or made by a 3D builder or by setting a camera view.
#[test]
fn a_polar_axes_becomes_cylindrical_keeping_its_polar_view() {
    let mut fig = Figure::new().tiles(1, 4);
    for col in 0..4 {
        fig.polar_axes(0, col)
            .theta_zero(ThetaZero::North)
            .hole(0.2)
            .angle_units(AngleUnits::Radians);
    }
    let explicit = fig.axes(0, 0).make_cylindrical().id();
    let by_plot3 = fig.axes(0, 1).id();
    fig.axes(0, 1).plot3([0.0, 1.0], [1.0, 2.0], [0.0, 1.0]);
    let by_constructor = fig.cylindrical_axes(0, 2).id();
    let by_view = fig.axes(0, 3).view(TURNED.0, TURNED.1).id();
    for (id, how) in [
        (explicit, "make_cylindrical"),
        (by_plot3, "plot3"),
        (by_constructor, "cylindrical_axes"),
        (by_view, "view"),
    ] {
        let (_, polar) = cylindrical(&fig, id);
        assert_eq!(polar.zero_deg, 90.0, "{how} keeps the zero direction");
        assert_eq!(polar.hole, 0.2, "{how} keeps the hole");
        assert_eq!(
            polar.angle_units,
            AngleUnits::Radians,
            "{how} keeps the angle units"
        );
    }
}

// WHY: the polar angle of a spherical axes is measured up from the X–Y plane (elevation, the
// default, as in antenna patterns and geography) or down from +Z (inclination, as in physics);
// `polar_angle` chooses between them and must change nothing else of the view. Only a spherical
// axes has a polar angle, so on any other axes the setter converts it, as the setters of the
// polar view convert to polar axes, rather than being silently ignored.
#[test]
fn polar_angle_chooses_the_convention_of_a_spherical_axes() {
    let mut fig = Figure::new();
    let id = fig
        .spherical_axes(0, 0)
        .angle_units(AngleUnits::Radians)
        .polar_angle(PolarAngle::Inclination)
        .id();
    let (_, view) = spherical(&fig, id);
    assert_eq!(view.polar_angle, PolarAngle::Inclination);
    assert_eq!(view.angle_units, AngleUnits::Radians);
    fig.spherical_axes(0, 0).polar_angle(PolarAngle::Elevation);
    assert_eq!(spherical(&fig, id).1.polar_angle, PolarAngle::Elevation);

    let mut fig = Figure::new();
    let id = fig.axes(0, 0).polar_angle(PolarAngle::Inclination).id();
    assert_eq!(
        spherical(&fig, id).1.polar_angle,
        PolarAngle::Inclination,
        "a two-dimensional axes becomes spherical"
    );
}

// WHY: no artist is duplicated for the new axes: plot3, scatter3, surf, mesh, contour3, quiver3
// and images on every plane are built by the builders that draw them in Cartesian 3D axes. Each
// must leave the axes in its projection (a 3D builder converts only 2D and polar axes) and the
// figure must validate without errors or warnings, so that the scene compiler draws every one.
#[test]
fn the_three_dimensional_builders_draw_into_cylindrical_and_spherical_axes_and_validate() {
    let angle: Vec<f64> = (0..=8).map(|i| 45.0 * f64::from(i)).collect();
    let second = [10.0, 30.0, 50.0];
    let grid = |f: fn(f64, f64) -> f64| {
        Matrix::from_fn(second.len(), angle.len(), |row, col| {
            f(angle[col], second[row])
        })
    };
    let field = grid(|a, b| 1.0 + 0.2 * (a.to_radians()).cos() + b / 100.0);
    let x = grid(|a, _| a);
    let y = grid(|_, b| b);
    let pixels = Matrix::from_fn(2, 3, |row, col| (row * 3 + col) as f64);

    for spherical_axes in [false, true] {
        let mut fig = Figure::new();
        let mut ax = if spherical_axes {
            fig.spherical_axes(0, 0)
        } else {
            fig.cylindrical_axes(0, 0)
        };
        ax.plot3([0.0, 90.0, 180.0], [10.0, 20.0, 30.0], [1.0, 1.5, 2.0]);
        ax.scatter3([0.0, 90.0], [10.0, 20.0], [1.0, 2.0]);
        ax.surf(&x, &y, &field);
        ax.mesh(&x, &y, &field);
        ax.contour3(&angle, second, &field);
        ax.quiver3([0.0], [10.0], [1.0], [10.0], [5.0], [0.5]);
        for plane in [
            ImagePlane::Xy { z: Some(1.5) },
            ImagePlane::Xz { y: Some(20.0) },
            ImagePlane::Yz { x: Some(90.0) },
        ] {
            ax.scalar_image(&pixels)
                .pixel_columns(15.0, 75.0)
                .pixel_rows(15.0, 45.0)
                .plane(plane);
        }

        let kind = if spherical_axes {
            "spherical"
        } else {
            "cylindrical"
        };
        let report = fig.validate();
        assert!(report.errors.is_empty(), "{kind}: {:?}", report.errors);
        assert!(report.warnings.is_empty(), "{kind}: {:?}", report.warnings);
        let a = &fig.ir().axes[0];
        assert!(
            if spherical_axes {
                matches!(a.projection, Projection::Spherical { .. })
            } else {
                matches!(a.projection, Projection::Cylindrical { .. })
            },
            "{kind}: drawing leaves the axes {kind}, found {:?}",
            a.projection
        );
        assert_eq!(a.artists.len(), 9, "{kind}: one artist per builder call");
        assert!(
            a.artists
                .iter()
                .filter(|artist| matches!(artist, Artist::Series(_)))
                .count()
                == 2,
            "{kind}: no builder turns its artist into a series"
        );
    }
}

// WHY: users import `ironlab::prelude::*` and expect to name the vocabulary of spherical axes
// without reaching into `ironlab::ir`; this test fails to compile if a re-export is missing.
#[test]
fn the_prelude_names_the_spherical_vocabulary() {
    let _angle: PolarAngle = PolarAngle::Inclination;
    let _view: SphericalView = SphericalView::default();
}

/// Returns whether the x, y and z grids of an axes are on.
fn grids(fig: &Figure, id: NodeId) -> [bool; 3] {
    let a = axes(fig, id);
    [a.x.grid, a.y.grid, a.z.grid]
}

// WHY: cylindrical and spherical axes are read by their grids, as polar axes are: the spokes and
// circles on the floor of a cylinder give the angle and radius of a point, and the spokes and
// constant-radius circles of a sphere give its azimuth, polar angle and radius. The IR keeps
// every grid off by default, so the facade constructors must turn on the grids each kind is read
// by: x and y for a cylinder (its z grid would ring the wall and is left to the user), and all
// three for a sphere.
#[test]
fn cylindrical_and_spherical_axes_turn_on_the_grids_they_are_read_by() {
    let mut fig = Figure::new().tiles(1, 2);
    let c = fig.cylindrical_axes(0, 0).id();
    let s = fig.spherical_axes(0, 1).id();
    assert_eq!(
        grids(&fig, c),
        [true, true, false],
        "spokes and circles on the floor"
    );
    assert_eq!(grids(&fig, s), [true, true, true], "every grid of a sphere");
    let plain = ironlab::ir::Axes::default();
    assert!(
        !plain.x.grid && !plain.y.grid && !plain.z.grid,
        "the IR default keeps grids off"
    );
}

// WHY: the grids are a default, not a rule: `grid(false)` must remove them, and calling the
// constructor again for the same tile, which returns the same axes, must not turn them back on,
// or the order of calls would matter.
#[test]
fn grid_false_removes_the_default_grids_of_cylindrical_and_spherical_axes() {
    let mut fig = Figure::new().tiles(1, 2);
    let c = fig.cylindrical_axes(0, 0).grid(false).id();
    let s = fig.spherical_axes(0, 1).grid(false).id();
    assert_eq!(grids(&fig, c), [false; 3]);
    assert_eq!(grids(&fig, s), [false; 3]);
    fig.cylindrical_axes(0, 0);
    fig.spherical_axes(0, 1);
    assert_eq!(
        grids(&fig, c),
        [false; 3],
        "an existing cylinder keeps its grids"
    );
    assert_eq!(
        grids(&fig, s),
        [false; 3],
        "an existing sphere keeps its grids"
    );
}

// WHY: a 3D axes asked for as polar axes becomes cylindrical, and a polar axes given to
// `spherical_axes` becomes spherical; either way the axes newly takes a kind that is read by its
// grids, so it must get the grids that the constructor of that kind gives it.
#[test]
fn an_axes_converted_to_cylindrical_or_spherical_gets_their_grids() {
    let mut fig = Figure::new().tiles(1, 2);
    let a = fig.axes3(0, 0).id();
    fig.polar_axes(0, 0);
    assert!(matches!(
        axes(&fig, a).projection,
        Projection::Cylindrical { .. }
    ));
    assert_eq!(grids(&fig, a), [true, true, false]);

    let b = fig.polar_axes(0, 1).grid(false).id();
    fig.spherical_axes(0, 1);
    assert_eq!(grids(&fig, b), [true, true, true]);
}
