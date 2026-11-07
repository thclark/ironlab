//! Polar, cylindrical and spherical projections in the IR: their defaults, their JSON and
//! Protocol Buffers forms, their validation, and how the edit model, the property choices
//! and the view overlay treat them.
//!
//! The projections reuse the axes' own x, y and z axes: x carries the angle, its limits are
//! the sector and `x.reverse` the direction; y carries the radius in polar and cylindrical
//! axes, and the polar angle in spherical axes, whose radius is z. The tests here check the
//! rules that follow from that reuse rather than restating the types.

mod common;

use std::f64::consts::{PI, TAU};

use common::edits::{applied, line, manual, path, set, tx};
use common::{FigureBuilder, find_axes, raw_fields, single_line_figure};
use ironlab_ir::overlay::Overlay;
use ironlab_ir::*;
use prost::Message;
use serde_json::{Value as JsonValue, json};

// ---------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------

fn polar(polar: PolarView) -> Projection {
    Projection::Polar {
        polar,
        view: View2d::default(),
    }
}

fn cylindrical(polar: PolarView) -> Projection {
    Projection::Cylindrical {
        view3d: View3d::default(),
        polar,
    }
}

fn spherical(spherical: SphericalView) -> Projection {
    Projection::Spherical {
        view3d: View3d::default(),
        spherical,
    }
}

/// The three projections that measure an angle along x, each with its default view.
fn angular_projections() -> [(&'static str, Projection); 3] {
    [
        ("polar", polar(PolarView::default())),
        ("cylindrical", cylindrical(PolarView::default())),
        ("spherical", spherical(SphericalView::default())),
    ]
}

/// The single-line figure (x of 1, 2, 3 and y of 1, 4, 9, both positive and within every
/// automatic sector) with its axes given a projection.
fn line_in(projection: Projection) -> (Figure, NodeId, NodeId) {
    let (mut fig, axes, line) = single_line_figure();
    fig.axes[0].projection = projection;
    (fig, axes, line)
}

/// Returns the polar view of a projection that has one, so that a test can change it.
fn polar_view_mut(projection: &mut Projection) -> &mut PolarView {
    match projection {
        Projection::Polar { polar, .. } | Projection::Cylindrical { polar, .. } => polar,
        other => panic!("{other:?} has no polar view"),
    }
}

/// Returns the angle units of any projection that measures angles.
fn angle_units_mut(projection: &mut Projection) -> &mut AngleUnits {
    match projection {
        Projection::Polar { polar, .. } | Projection::Cylindrical { polar, .. } => {
            &mut polar.angle_units
        }
        Projection::Spherical { spherical, .. } => &mut spherical.angle_units,
        other => panic!("{other:?} measures no angle"),
    }
}

fn has_error(report: &ValidationReport, kind: IssueKind, node: NodeId) -> bool {
    report
        .errors
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

fn has_warning(report: &ValidationReport, kind: IssueKind, node: NodeId) -> bool {
    report
        .warnings
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

fn reports(report: &ValidationReport, kind: IssueKind) -> bool {
    report
        .errors
        .iter()
        .chain(&report.warnings)
        .any(|issue| issue.kind == kind)
}

/// Asserts that a report holds exactly one error, of the given kind and against the given
/// node, so that a check cannot pass by raising some other error instead.
#[track_caller]
fn assert_only_error(report: &ValidationReport, kind: IssueKind, node: NodeId, at: &str) {
    assert!(has_error(report, kind, node), "{at}: {report:?}");
    assert_eq!(report.errors.len(), 1, "{at}: {report:?}");
}

#[track_caller]
fn assert_no_issues(report: &ValidationReport, at: &str) {
    assert_eq!(report.errors, vec![], "{at}");
    assert_eq!(report.warnings, vec![], "{at}");
}

// ---------------------------------------------------------------------------------
// Defaults and helpers of the types
// ---------------------------------------------------------------------------------

// Why: a polar axes made without any further choice must look like the polar plot every
// user expects (degrees, zero to the east, no hole, radial labels placed automatically),
// and a spherical axes must measure elevation, the default the plan agreed; the facade
// and the property choices both start from these defaults.
#[test]
fn the_views_default_to_degrees_east_no_hole_and_elevation() {
    assert_eq!(
        PolarView::default(),
        PolarView {
            angle_units: AngleUnits::Degrees,
            zero_deg: 0.0,
            hole: 0.0,
            radial_labels_at: None,
        }
    );
    assert_eq!(
        SphericalView::default(),
        SphericalView {
            angle_units: AngleUnits::Degrees,
            polar_angle: PolarAngle::Elevation,
        }
    );
    assert_eq!(AngleUnits::default(), AngleUnits::Degrees);
    assert_eq!(PolarAngle::default(), PolarAngle::Elevation);
}

// Why: the length of a turn in data units fixes the automatic sector, the limit on manual
// sectors and the spherical polar-angle range, so each unit must give exactly its turn:
// 360 degrees, 2π radians, and the stated count of any other unit.
#[test]
fn a_turn_is_three_hundred_and_sixty_degrees_two_pi_radians_or_the_stated_count() {
    assert_eq!(AngleUnits::Degrees.turn(), 360.0);
    assert_eq!(AngleUnits::Radians.turn(), TAU);
    assert_eq!(AngleUnits::PerTurn(12.0).turn(), 12.0);
    assert_eq!(AngleUnits::PerTurn(24.0).turn(), 24.0);
    assert_eq!(AngleUnits::PerTurn(0.5).turn(), 0.5);
}

// Why: every crate decides between the 2D and 3D layout, interaction and validation by
// asking the projection whether it is three-dimensional; a cylindrical or spherical axes
// treated as 2D would be drawn flat, and a polar axes treated as 3D would refuse bars.
#[test]
fn cylindrical_and_spherical_axes_are_three_dimensional_and_polar_axes_are_not() {
    assert!(!Projection::TwoD.is_3d());
    assert!(
        Projection::ThreeD {
            view3d: View3d::default()
        }
        .is_3d()
    );
    assert!(!polar(PolarView::default()).is_3d());
    assert!(cylindrical(PolarView::default()).is_3d());
    assert!(spherical(SphericalView::default()).is_3d());
}

// ---------------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------------

/// The JSON value of the projection of the first axes of a figure.
fn projection_json(fig: &Figure) -> JsonValue {
    let value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    value["axes"][0]["projection"].clone()
}

// Why: the JSON form is a public contract that hand-written files and other languages
// follow; the projection is tagged by `type` in snake case with its views as named
// fields, the angle units are tagged as the aspect is (a `type`, and the count of a
// custom unit as `value`), the polar angle is a plain snake-case name, an automatic
// radial label spoke is written as null, and the pan of polar axes is written in full as
// the camera of 3D axes is.
#[test]
fn the_angular_projections_are_written_in_the_documented_json_form() {
    let (fig, _, _) = line_in(polar(PolarView {
        angle_units: AngleUnits::PerTurn(12.0),
        zero_deg: 90.0,
        hole: 0.25,
        radial_labels_at: None,
    }));
    assert_eq!(
        projection_json(&fig),
        json!({
            "type": "polar",
            "polar": {
                "angle_units": { "type": "per_turn", "value": 12.0 },
                "zero_deg": 90.0,
                "hole": 0.25,
                "radial_labels_at": null
            },
            "view": { "pan_x": 0.0, "pan_y": 0.0 }
        })
    );

    let (fig, _, _) = line_in(Projection::Cylindrical {
        view3d: View3d {
            pan_x: 0.25,
            ..View3d::default()
        },
        polar: PolarView {
            angle_units: AngleUnits::Radians,
            radial_labels_at: Some(0.5),
            ..PolarView::default()
        },
    });
    assert_eq!(
        projection_json(&fig),
        json!({
            "type": "cylindrical",
            "view3d": { "azimuth_deg": -37.5, "elevation_deg": 30.0, "zoom": 1.0, "pan_x": 0.25, "pan_y": 0.0 },
            "polar": {
                "angle_units": { "type": "radians" },
                "zero_deg": 0.0,
                "hole": 0.0,
                "radial_labels_at": 0.5
            }
        })
    );

    let (fig, _, _) = line_in(spherical(SphericalView {
        angle_units: AngleUnits::Degrees,
        polar_angle: PolarAngle::Inclination,
    }));
    assert_eq!(
        projection_json(&fig),
        json!({
            "type": "spherical",
            "view3d": { "azimuth_deg": -37.5, "elevation_deg": 30.0, "zoom": 1.0, "pan_x": 0.0, "pan_y": 0.0 },
            "spherical": {
                "angle_units": { "type": "degrees" },
                "polar_angle": "inclination"
            }
        })
    );
}

// Why: adding projections must not change how existing files are written, or every saved
// 2D and 3D figure would be rewritten on its next save and older readers could not open
// files of unchanged figures.
#[test]
fn the_json_of_two_and_three_dimensional_axes_is_unchanged() {
    let (fig, _, _) = line_in(Projection::TwoD);
    assert_eq!(projection_json(&fig), json!({ "type": "two_d" }));
    let (fig, _, _) = line_in(Projection::ThreeD {
        view3d: View3d::default(),
    });
    assert_eq!(
        projection_json(&fig),
        json!({
            "type": "three_d",
            "view3d": { "azimuth_deg": -37.5, "elevation_deg": 30.0, "zoom": 1.0, "pan_x": 0.0, "pan_y": 0.0 }
        })
    );
}

/// Every new projection with views away from their defaults, covering every variant of the
/// angle units, both polar angles, and an automatic and a fixed radial label spoke.
fn every_angular_projection() -> Vec<Projection> {
    vec![
        polar(PolarView::default()),
        polar(PolarView {
            angle_units: AngleUnits::Radians,
            zero_deg: -90.0,
            hole: 0.5,
            radial_labels_at: Some(PI / 8.0),
        }),
        polar(PolarView {
            angle_units: AngleUnits::PerTurn(5.0),
            zero_deg: 90.0,
            hole: 0.0,
            radial_labels_at: Some(2.5),
        }),
        Projection::Polar {
            polar: PolarView::default(),
            view: View2d {
                pan_x: 0.375,
                pan_y: -0.125,
            },
        },
        Projection::Cylindrical {
            view3d: View3d {
                azimuth_deg: 10.0,
                elevation_deg: -20.0,
                zoom: 2.0,
                pan_x: 0.25,
                pan_y: -0.125,
            },
            polar: PolarView {
                angle_units: AngleUnits::PerTurn(24.0),
                zero_deg: 45.0,
                hole: 0.125,
                radial_labels_at: None,
            },
        },
        spherical(SphericalView::default()),
        Projection::Spherical {
            view3d: View3d {
                azimuth_deg: 60.0,
                ..View3d::default()
            },
            spherical: SphericalView {
                angle_units: AngleUnits::Radians,
                polar_angle: PolarAngle::Inclination,
            },
        },
    ]
}

// Why: a saved polar figure must reopen exactly as it was, with every field of its view,
// whichever of the two file formats it was saved in.
#[test]
fn every_angular_projection_survives_json_and_protobuf_round_trips() {
    for projection in every_angular_projection() {
        let (fig, _, _) = line_in(projection);
        let text = fig.to_json();
        let reopened = Figure::from_json(&text).expect("own output parses");
        assert_eq!(reopened, fig, "JSON of {projection:?}");
        assert_eq!(reopened.to_json(), text, "JSON of {projection:?} is stable");
        let bytes = fig.to_protobuf();
        let decoded = Figure::from_protobuf(&bytes).expect("own output decodes");
        assert_eq!(decoded, fig, "protobuf of {projection:?}");
        assert_eq!(
            decoded.to_protobuf(),
            bytes,
            "protobuf of {projection:?} is stable"
        );
    }
}

// Why: the oneof field numbers are the binary contract with readers in other languages:
// two-dimensional and three-dimensional axes keep 1 and 2, and the new projections take
// 3 (polar), 4 (cylindrical) and 5 (spherical); a renumbering would make old readers
// decode a polar axes as some other projection.
#[test]
fn the_angular_projections_take_the_oneof_field_numbers_three_to_five() {
    for (projection, number) in [
        (Projection::TwoD, 1),
        (
            Projection::ThreeD {
                view3d: View3d::default(),
            },
            2,
        ),
        (polar(PolarView::default()), 3),
        (cylindrical(PolarView::default()), 4),
        (spherical(SphericalView::default()), 5),
    ] {
        let (fig, _, _) = line_in(projection);
        let wire_fig = wire::Figure::decode(fig.to_protobuf().as_slice()).unwrap();
        let encoded = wire_fig.axes[0]
            .projection
            .as_ref()
            .expect("the projection is written")
            .encode_to_vec();
        assert_eq!(
            raw_fields(&encoded, number).len(),
            1,
            "{projection:?} is not field {number}: {encoded:?}"
        );
    }
}

// ---------------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------------

// Why: the plain polar, cylindrical and spherical axes of a line must be accepted without
// a word, or every user of the new projections would be shown a warning for doing
// nothing unusual.
#[test]
fn a_line_in_default_angular_axes_raises_no_issue() {
    for (name, projection) in angular_projections() {
        let (fig, _, _) = line_in(projection);
        assert_no_issues(&fig.validate(), name);
    }
}

// Why: an angle has no logarithm worth drawing (a turn has no origin and wraps), so a
// logarithmic angular axis is refused on x of every angular projection and on y, the
// polar angle, of spherical axes, against the axes.
#[test]
fn a_logarithmic_angle_is_an_error() {
    let cases: [(&str, Projection, Dimension); 4] = [
        ("polar x", polar(PolarView::default()), Dimension::X),
        (
            "cylindrical x",
            cylindrical(PolarView::default()),
            Dimension::X,
        ),
        (
            "spherical x",
            spherical(SphericalView::default()),
            Dimension::X,
        ),
        (
            "spherical y",
            spherical(SphericalView::default()),
            Dimension::Y,
        ),
    ];
    for (name, projection, dimension) in cases {
        let (mut fig, axes, _) = line_in(projection);
        match dimension {
            Dimension::X => fig.axes[0].x.scale = Scale::Log,
            Dimension::Y => fig.axes[0].y.scale = Scale::Log,
            Dimension::Z => unreachable!(),
        }
        assert_only_error(&fig.validate(), IssueKind::LogAngleAxis, axes, name);
    }
}

// Why: a logarithmic radius is a common and supported choice (the lower limit sits at the
// centre), as is a logarithmic cylindrical height and a logarithmic spherical radius; none
// of them is an angle, so none may be refused as one.
#[test]
fn a_logarithmic_radius_or_height_is_valid() {
    let cases: [(&str, Projection, Dimension); 4] = [
        ("polar radius", polar(PolarView::default()), Dimension::Y),
        (
            "cylindrical radius",
            cylindrical(PolarView::default()),
            Dimension::Y,
        ),
        (
            "cylindrical height",
            cylindrical(PolarView::default()),
            Dimension::Z,
        ),
        (
            "spherical radius",
            spherical(SphericalView::default()),
            Dimension::Z,
        ),
    ];
    for (name, projection, dimension) in cases {
        let (mut fig, _, _) = line_in(projection);
        match dimension {
            Dimension::Y => fig.axes[0].y.scale = Scale::Log,
            Dimension::Z => fig.axes[0].z.scale = Scale::Log,
            Dimension::X => unreachable!(),
        }
        let report = fig.validate();
        assert!(
            !reports(&report, IssueKind::LogAngleAxis),
            "{name}: {report:?}"
        );
        assert_eq!(report.errors, vec![], "{name}");
    }
}

// Why: a sector is part of one turn; limits spanning more would draw data over itself, so
// they are refused, in whatever units the angle is measured, while exactly one turn (the
// full circle, written with explicit limits) and one placed anywhere on the circle are
// accepted.
#[test]
fn manual_angular_limits_spanning_more_than_a_turn_are_an_error() {
    let cases: [(AngleUnits, [f64; 2], bool); 9] = [
        (AngleUnits::Degrees, [0.0, 360.0], true),
        (AngleUnits::Degrees, [-180.0, 180.0], true),
        (AngleUnits::Degrees, [-60.0, 60.0], true),
        (AngleUnits::Degrees, [0.0, 360.5], false),
        (AngleUnits::Degrees, [-200.0, 200.0], false),
        (AngleUnits::Radians, [0.0, TAU], true),
        (AngleUnits::Radians, [0.0, 6.3], false),
        (AngleUnits::PerTurn(12.0), [0.0, 12.0], true),
        (AngleUnits::PerTurn(12.0), [0.0, 13.0], false),
    ];
    for (name, projection) in angular_projections() {
        for (units, [min, max], valid) in cases {
            let (mut fig, axes, _) = line_in(projection);
            *angle_units_mut(&mut fig.axes[0].projection) = units;
            fig.axes[0].x.limits = manual(min, max);
            let report = fig.validate();
            let at = format!("{name} x in {units:?} over [{min}, {max}]");
            if valid {
                assert_eq!(report.errors, vec![], "{at}");
            } else {
                assert_only_error(&report, IssueKind::SectorExceedsTurn, axes, &at);
            }
        }
    }
}

// Why: the polar angle of spherical axes is bounded by its convention: an elevation runs
// from a quarter turn below the X–Y plane to a quarter turn above it, and an inclination
// from the +Z pole to the −Z pole over half a turn. Limits beyond those name directions
// that do not exist and are refused; the full ranges themselves are accepted, in degrees
// and in radians.
#[test]
fn spherical_polar_angle_limits_beyond_the_convention_are_an_error() {
    let cases: [(PolarAngle, AngleUnits, [f64; 2], bool); 12] = [
        (
            PolarAngle::Elevation,
            AngleUnits::Degrees,
            [-90.0, 90.0],
            true,
        ),
        (
            PolarAngle::Elevation,
            AngleUnits::Degrees,
            [0.0, 45.0],
            true,
        ),
        (
            PolarAngle::Elevation,
            AngleUnits::Degrees,
            [-91.0, 0.0],
            false,
        ),
        (
            PolarAngle::Elevation,
            AngleUnits::Degrees,
            [0.0, 91.0],
            false,
        ),
        (
            PolarAngle::Elevation,
            AngleUnits::Radians,
            [-PI / 2.0, PI / 2.0],
            true,
        ),
        (
            PolarAngle::Elevation,
            AngleUnits::Radians,
            [0.0, 1.6],
            false,
        ),
        (
            PolarAngle::Inclination,
            AngleUnits::Degrees,
            [0.0, 180.0],
            true,
        ),
        (
            PolarAngle::Inclination,
            AngleUnits::Degrees,
            [-1.0, 90.0],
            false,
        ),
        (
            PolarAngle::Inclination,
            AngleUnits::Degrees,
            [0.0, 181.0],
            false,
        ),
        // Elevation limits that would be valid elevations are not valid inclinations.
        (
            PolarAngle::Inclination,
            AngleUnits::Degrees,
            [-90.0, 90.0],
            false,
        ),
        (
            PolarAngle::Inclination,
            AngleUnits::Radians,
            [0.0, PI],
            true,
        ),
        (
            PolarAngle::Inclination,
            AngleUnits::Radians,
            [0.0, 3.2],
            false,
        ),
    ];
    for (polar_angle, angle_units, [min, max], valid) in cases {
        let (mut fig, axes, _) = line_in(spherical(SphericalView {
            angle_units,
            polar_angle,
        }));
        fig.axes[0].y.limits = manual(min, max);
        let report = fig.validate();
        let at = format!("{polar_angle:?} in {angle_units:?} over [{min}, {max}]");
        if valid {
            assert_eq!(report.errors, vec![], "{at}");
        } else {
            assert_only_error(&report, IssueKind::SectorExceedsTurn, axes, &at);
        }
    }
}

// Why: y of polar and cylindrical axes is a radius, not an angle, so its limits are not
// bounded by a turn; a radial range of a thousand must not be mistaken for a sector.
#[test]
fn radial_limits_are_not_bounded_by_a_turn() {
    for projection in [
        polar(PolarView::default()),
        cylindrical(PolarView::default()),
    ] {
        let (mut fig, _, _) = line_in(projection);
        fig.axes[0].y.limits = manual(0.0, 1000.0);
        assert_eq!(fig.validate().errors, vec![], "{projection:?}");
    }
}

// Why: the hole is a fraction of the rim radius left empty: a hole of 1 or more would
// leave no room for data, a negative one has no meaning and NaN no size, so each is
// refused against the axes, in polar and cylindrical axes alike; any fraction below 1 is
// accepted.
#[test]
fn a_hole_outside_zero_to_one_is_an_error() {
    for projection in [
        polar(PolarView::default()),
        cylindrical(PolarView::default()),
    ] {
        for (hole, valid) in [
            (0.0, true),
            (0.5, true),
            (0.99, true),
            (1.0, false),
            (1.5, false),
            (-0.1, false),
            (f64::NAN, false),
            (f64::INFINITY, false),
        ] {
            let (mut fig, axes, _) = line_in(projection);
            polar_view_mut(&mut fig.axes[0].projection).hole = hole;
            let report = fig.validate();
            let at = format!("hole {hole} in {projection:?}");
            if valid {
                assert_eq!(report.errors, vec![], "{at}");
            } else {
                assert_only_error(&report, IssueKind::InvalidHole, axes, &at);
            }
        }
    }
}

// Why: a custom angle unit is a count per turn, so it must be a finite positive number;
// zero, a negative count, infinity and NaN cannot divide a turn, and each is refused in
// every angular projection.
#[test]
fn a_count_per_turn_that_is_not_finite_and_positive_is_an_error() {
    for (name, projection) in angular_projections() {
        for count in [0.0, -12.0, f64::INFINITY, f64::NAN] {
            let (mut fig, axes, _) = line_in(projection);
            *angle_units_mut(&mut fig.axes[0].projection) = AngleUnits::PerTurn(count);
            let report = fig.validate();
            assert!(
                has_error(&report, IssueKind::InvalidAngleUnits, axes),
                "{name} with {count} per turn: {report:?}"
            );
        }
        let (mut fig, _, _) = line_in(projection);
        *angle_units_mut(&mut fig.axes[0].projection) = AngleUnits::PerTurn(0.25);
        assert_eq!(fig.validate().errors, vec![], "{name} with 0.25 per turn");
    }
}

// Why: polar axes are always drawn round, and a spherical axes is a sphere, so an aspect
// other than automatic is not honoured there; the user who set one must be told, but the
// figure still draws, so it is a warning against the axes. A cylindrical axes keeps its
// height-to-radius ratio, so its aspect is honoured and raises nothing.
#[test]
fn an_aspect_in_polar_or_spherical_axes_is_ignored_with_a_warning() {
    for aspect in [Aspect::Equal, Aspect::ratio(2.0)] {
        for (name, projection, warned) in [
            ("polar", polar(PolarView::default()), true),
            ("spherical", spherical(SphericalView::default()), true),
            ("cylindrical", cylindrical(PolarView::default()), false),
        ] {
            let (mut fig, axes, _) = line_in(projection);
            fig.axes[0].aspect = aspect;
            let report = fig.validate();
            let at = format!("{aspect:?} in {name}");
            assert_eq!(report.errors, vec![], "{at}");
            assert_eq!(
                has_warning(&report, IssueKind::AspectIgnored, axes),
                warned,
                "{at}: {report:?}"
            );
        }
        let (fig, _, _) = line_in(Projection::TwoD);
        assert!(!reports(&fig.validate(), IssueKind::AspectIgnored));
    }
    for (name, projection) in angular_projections() {
        let (fig, _, _) = line_in(projection);
        assert!(
            !reports(&fig.validate(), IssueKind::AspectIgnored),
            "automatic aspect in {name}"
        );
    }
}

// Why: polar axes are a plane, so an artist that needs a third dimension (a series with
// z, an image on a wall) cannot be drawn there and is refused as it is in 2D Cartesian
// axes.
#[test]
fn a_three_dimensional_artist_in_polar_axes_is_an_error() {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    b.axes(axes).projection = polar(PolarView::default());
    let x = b.vector(&[1.0, 2.0, 3.0]);
    let y = b.vector(&[1.0, 4.0, 9.0]);
    let z = b.vector(&[0.0, 1.0, 2.0]);
    let id = b.node();
    b.push(
        axes,
        Artist::Series(Series {
            id,
            x,
            y,
            z: Some(z),
            ..Series::default()
        }),
    );
    let fig = b.build();
    assert_only_error(
        &fig.validate(),
        IssueKind::ThreeDArtistInTwoDAxes,
        id,
        "a series with z",
    );

    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    b.axes(axes).projection = polar(PolarView::default());
    let pixels = b.bytes(vec![2, 2, 3], vec![0; 12]);
    let id = b.node();
    b.push(
        axes,
        Artist::Image(Image {
            id,
            pixels,
            placement: ImagePlacement {
                plane: ImagePlane::Xz { y: None },
                ..ImagePlacement::default()
            },
            ..Image::default()
        }),
    );
    let fig = b.build();
    assert!(
        has_error(&fig.validate(), IssueKind::ThreeDArtistInTwoDAxes, id),
        "an image on the xz wall: {:?}",
        fig.validate()
    );
}

/// A figure of one axes with the given projection holding a bar series, stairs or cells,
/// built from valid data; returns the figure and the artist.
fn two_d_artist_in(projection: Projection, kind: &str) -> (Figure, NodeId) {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    b.axes(axes).projection = projection;
    let scale = b.color_scale(ColorScaleKind::default());
    let id = b.node();
    let artist = match kind {
        "bars" => Artist::Bar(Bar {
            id,
            positions: b.vector(&[45.0, 135.0, 225.0]),
            values: b.vector(&[2.0, 5.0, 3.0]),
            ..Bar::default()
        }),
        "stairs" => Artist::Stairs(Stairs {
            id,
            data: StairsData::Edges {
                edges: b.vector(&[0.0, 90.0, 180.0, 270.0]),
                values: b.vector(&[1.0, 3.0, 2.0]),
            },
            ..Stairs::default()
        }),
        "cells" => Artist::Cells(Cells {
            id,
            values: b.matrix(2, 3, |j, i| (j * 3 + i) as f64 + 1.0),
            color_scale: Some(scale),
            ..Cells::default()
        }),
        other => unreachable!("{other}"),
    };
    b.push(axes, artist);
    (b.build(), id)
}

// Why: bars, stairs and cells are drawn in a plane; polar axes are a plane (a bar becomes
// a wedge, a cell an annular sector), so they are accepted there, but cylindrical and
// spherical axes, like 3D Cartesian axes, have no plane for them and refuse them against
// the artist.
#[test]
fn bars_stairs_and_cells_are_valid_in_polar_axes_and_refused_in_three_dimensional_ones() {
    for kind in ["bars", "stairs", "cells"] {
        let (fig, _) = two_d_artist_in(polar(PolarView::default()), kind);
        let report = fig.validate();
        assert_eq!(report.errors, vec![], "{kind} in polar axes");
        for (name, projection) in [
            ("cylindrical", cylindrical(PolarView::default())),
            ("spherical", spherical(SphericalView::default())),
        ] {
            let (fig, artist) = two_d_artist_in(projection, kind);
            assert!(
                has_error(&fig.validate(), IssueKind::TwoDArtistInThreeDAxes, artist),
                "{kind} in {name} axes: {:?}",
                fig.validate()
            );
        }
    }
}

// Why: axes-fraction coordinates in polar axes are fractions of the sector and of the
// radial span (a horizontal reference line is a circle, a vertical one a spoke), so they
// are accepted; in cylindrical and spherical axes, as in 3D Cartesian axes, a fraction of
// the plot area has no position in the box, and they are refused.
#[test]
fn axes_fraction_coordinates_are_valid_in_polar_axes_only() {
    for (x_coords, y_coords) in [
        (Coords::AxesFraction, Coords::Data),
        (Coords::Data, Coords::AxesFraction),
    ] {
        for (name, projection, refused) in [
            ("polar", polar(PolarView::default()), false),
            ("cylindrical", cylindrical(PolarView::default()), true),
            ("spherical", spherical(SphericalView::default()), true),
        ] {
            let (mut fig, _, line) = line_in(projection);
            let Artist::Series(series) = &mut fig.axes[0].artists[0] else {
                unreachable!("the fixture holds a series");
            };
            series.x_coords = x_coords;
            series.y_coords = y_coords;
            let report = fig.validate();
            let at = format!("{x_coords:?}, {y_coords:?} in {name}");
            assert_eq!(
                has_error(&report, IssueKind::AxesFractionInThreeDAxes, line),
                refused,
                "{at}: {report:?}"
            );
            if !refused {
                assert_eq!(report.errors, vec![], "{at}");
            }
        }
    }
}

// Why: the angular tick labels of polar axes are placed around the rim, outside it, where
// a rotation in the page would collide with the ring; the angle set on x is ignored with
// a warning against the axes. The radial labels lie along a spoke like an ordinary axis,
// so an angle on y is honoured and raises nothing. Cylindrical and spherical axes are
// three-dimensional and ignore the angle on every axis, as 3D Cartesian axes do.
#[test]
fn a_tick_label_angle_on_the_angular_axis_is_ignored_with_a_warning() {
    let (mut fig, axes, _) = line_in(polar(PolarView::default()));
    fig.axes[0].x.tick_label_angle = 30.0;
    let report = fig.validate();
    assert_eq!(report.errors, vec![], "polar x");
    assert!(
        has_warning(&report, IssueKind::TickLabelAngleIgnored, axes),
        "polar x: {report:?}"
    );

    let (mut fig, _, _) = line_in(polar(PolarView::default()));
    fig.axes[0].y.tick_label_angle = 30.0;
    assert_no_issues(&fig.validate(), "polar y");

    for (name, projection) in [
        ("cylindrical", cylindrical(PolarView::default())),
        ("spherical", spherical(SphericalView::default())),
    ] {
        for dimension in [Dimension::X, Dimension::Y, Dimension::Z] {
            let (mut fig, axes, _) = line_in(projection);
            match dimension {
                Dimension::X => fig.axes[0].x.tick_label_angle = 30.0,
                Dimension::Y => fig.axes[0].y.tick_label_angle = 30.0,
                Dimension::Z => fig.axes[0].z.tick_label_angle = 30.0,
            }
            assert!(
                has_warning(&fig.validate(), IssueKind::TickLabelAngleIgnored, axes),
                "{name} {dimension:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------
// Edits
// ---------------------------------------------------------------------------------

// Why: the property editor and clients in other languages change the polar view by path.
// Paths name wire fields and flatten variants (as `projection.view3d.zoom` does for 3D
// axes), so the polar view of polar and cylindrical axes is `projection.polar` and the
// spherical view `projection.spherical`; each field must be readable and settable there
// and change exactly that field.
#[test]
fn the_fields_of_the_views_are_set_by_flattened_paths() {
    let (fig, axes, _) = line_in(polar(PolarView::default()));
    assert_eq!(
        fig.get(axes, &path("projection.polar")).unwrap(),
        Value::PolarView(PolarView::default())
    );
    assert_eq!(
        fig.get(axes, &path("projection.polar.zero_deg")).unwrap(),
        Value::Double(0.0)
    );
    let cases: [(&str, Value, PolarView); 5] = [
        (
            "projection.polar.zero_deg",
            Value::Double(90.0),
            PolarView {
                zero_deg: 90.0,
                ..PolarView::default()
            },
        ),
        (
            "projection.polar.hole",
            Value::Double(0.5),
            PolarView {
                hole: 0.5,
                ..PolarView::default()
            },
        ),
        (
            "projection.polar.angle_units",
            Value::AngleUnits(AngleUnits::Radians),
            PolarView {
                angle_units: AngleUnits::Radians,
                ..PolarView::default()
            },
        ),
        (
            "projection.polar.radial_labels_at",
            Value::Double(22.5),
            PolarView {
                radial_labels_at: Some(22.5),
                ..PolarView::default()
            },
        ),
        (
            "projection.polar",
            Value::PolarView(PolarView {
                hole: 0.25,
                ..PolarView::default()
            }),
            PolarView {
                hole: 0.25,
                ..PolarView::default()
            },
        ),
    ];
    for (at, value, expected) in cases {
        let (edited, inverse) = applied(&fig, &tx([set(axes, at, value)])).unwrap();
        assert_eq!(edited.axes[0].projection, polar(expected), "{at}");
        let (restored, _) = applied(&edited, &inverse).unwrap();
        assert_eq!(restored, fig, "{at}: the inverse");
    }

    // An automatic radial label spoke is set by unsetting the optional value.
    let (mut fig, axes, _) = line_in(polar(PolarView {
        radial_labels_at: Some(45.0),
        ..PolarView::default()
    }));
    let (edited, _) = applied(
        &fig,
        &tx([set(axes, "projection.polar.radial_labels_at", Value::Unset)]),
    )
    .unwrap();
    assert_eq!(edited.axes[0].projection, polar(PolarView::default()));

    // The count of a custom unit is reached through its variant, as other tuple variants are.
    polar_view_mut(&mut fig.axes[0].projection).angle_units = AngleUnits::PerTurn(12.0);
    let (edited, _) = applied(
        &fig,
        &tx([set(
            axes,
            "projection.polar.angle_units.per_turn",
            Value::Double(24.0),
        )]),
    )
    .unwrap();
    assert_eq!(
        polar_view_mut(&mut edited.axes[0].projection.clone()).angle_units,
        AngleUnits::PerTurn(24.0)
    );
}

// Why: the pan of polar axes is a view property that the pan gesture sets and the property
// editor shows, so it is read and set at `projection.view`, beside `projection.polar`, as
// the camera of 3D axes is at `projection.view3d`; each field must change exactly that
// field and leave the polar view alone, and the inverse must restore the figure.
#[test]
fn the_pan_of_polar_axes_is_set_by_flattened_paths() {
    let (fig, axes, _) = line_in(polar(PolarView::default()));
    assert_eq!(
        fig.get(axes, &path("projection.view")).unwrap(),
        Value::View2d(View2d::default())
    );
    let panned = |pan_x: f64, pan_y: f64| Projection::Polar {
        polar: PolarView::default(),
        view: View2d { pan_x, pan_y },
    };
    let cases: [(&str, Value, Projection); 3] = [
        (
            "projection.view.pan_x",
            Value::Double(0.25),
            panned(0.25, 0.0),
        ),
        (
            "projection.view.pan_y",
            Value::Double(-0.5),
            panned(0.0, -0.5),
        ),
        (
            "projection.view",
            Value::View2d(View2d {
                pan_x: 0.125,
                pan_y: 0.375,
            }),
            panned(0.125, 0.375),
        ),
    ];
    for (at, value, expected) in cases {
        let (edited, inverse) = applied(&fig, &tx([set(axes, at, value)])).unwrap();
        assert_eq!(edited.axes[0].projection, expected, "{at}");
        assert_eq!(
            edited.get(axes, &path(at)).unwrap(),
            value_at(&expected, at),
            "{at}: read back"
        );
        let (restored, _) = applied(&edited, &inverse).unwrap();
        assert_eq!(restored, fig, "{at}: the inverse");
    }
}

/// The value at a path of the pan of a polar projection.
fn value_at(projection: &Projection, at: &str) -> Value {
    let Projection::Polar { view, .. } = projection else {
        panic!("the projection is polar");
    };
    match at {
        "projection.view.pan_x" => Value::Double(view.pan_x),
        "projection.view.pan_y" => Value::Double(view.pan_y),
        _ => Value::View2d(*view),
    }
}

// Why: cylindrical axes carry both a camera and a polar view, and spherical axes a camera
// and a spherical view; each must be reachable at the same flattened paths as in the
// projections they combine, so that one editor row and one drag serve every projection.
#[test]
fn the_camera_and_views_of_three_dimensional_angular_axes_are_set_by_path() {
    let (fig, axes, _) = line_in(cylindrical(PolarView::default()));
    let (edited, _) = applied(
        &fig,
        &tx([
            set(axes, "projection.view3d.azimuth_deg", Value::Double(15.0)),
            set(axes, "projection.polar.zero_deg", Value::Double(90.0)),
        ]),
    )
    .unwrap();
    assert_eq!(
        edited.axes[0].projection,
        Projection::Cylindrical {
            view3d: View3d {
                azimuth_deg: 15.0,
                ..View3d::default()
            },
            polar: PolarView {
                zero_deg: 90.0,
                ..PolarView::default()
            },
        }
    );

    let (fig, axes, _) = line_in(spherical(SphericalView::default()));
    let (edited, _) = applied(
        &fig,
        &tx([
            set(axes, "projection.view3d.zoom", Value::Double(2.0)),
            set(
                axes,
                "projection.spherical.polar_angle",
                Value::PolarAngle(PolarAngle::Inclination),
            ),
            set(
                axes,
                "projection.spherical.angle_units",
                Value::AngleUnits(AngleUnits::Radians),
            ),
        ]),
    )
    .unwrap();
    assert_eq!(
        edited.axes[0].projection,
        Projection::Spherical {
            view3d: View3d {
                zoom: 2.0,
                ..View3d::default()
            },
            spherical: SphericalView {
                angle_units: AngleUnits::Radians,
                polar_angle: PolarAngle::Inclination,
            },
        }
    );
    assert_eq!(
        edited.get(axes, &path("projection.spherical")).unwrap(),
        Value::SphericalView(SphericalView {
            angle_units: AngleUnits::Radians,
            polar_angle: PolarAngle::Inclination,
        })
    );
}

// Why: a stale edit (a polar view change sent to axes that have become spherical, or a
// camera change sent to polar axes) must be told apart from a typo so that the overlay can
// drop it; and the name of a variant is never part of a path.
#[test]
fn a_path_into_another_projection_is_an_inactive_variant_and_variant_names_are_unknown() {
    let (polar_fig, polar_axes, _) = line_in(polar(PolarView::default()));
    let (spherical_fig, spherical_axes, _) = line_in(spherical(SphericalView::default()));
    for (fig, axes, at) in [
        (&polar_fig, polar_axes, "projection.view3d.zoom"),
        (&polar_fig, polar_axes, "projection.spherical.polar_angle"),
        (&spherical_fig, spherical_axes, "projection.polar.zero_deg"),
    ] {
        assert!(
            matches!(
                fig.get(axes, &path(at)),
                Err(EditError::InactiveVariant { .. })
            ),
            "{at}: {:?}",
            fig.get(axes, &path(at))
        );
    }
    let (cyl_fig, cyl_axes, _) = line_in(cylindrical(PolarView::default()));
    for (fig, axes, at) in [
        (&polar_fig, polar_axes, "projection.polar.polar.zero_deg"),
        (&cyl_fig, cyl_axes, "projection.cylindrical.polar"),
        (&cyl_fig, cyl_axes, "projection.cylindrical.view3d.zoom"),
        (&polar_fig, polar_axes, "projection.polar.zero"),
    ] {
        assert!(
            matches!(fig.get(axes, &path(at)), Err(EditError::UnknownPath { .. })),
            "{at}: {:?}",
            fig.get(axes, &path(at))
        );
    }
}

// Why: an edit that would make the figure invalid (a hole of the whole radius, a sector
// of more than a turn by a change of units) must be refused as a whole, leaving the
// figure as it was, as every other invalid edit is.
#[test]
fn an_edit_that_makes_the_view_invalid_is_refused() {
    let (fig, axes, _) = line_in(polar(PolarView::default()));
    assert!(matches!(
        applied(
            &fig,
            &tx([set(axes, "projection.polar.hole", Value::Double(1.0))])
        ),
        Err(EditError::Invalid(_))
    ));
    let (mut fig, axes, _) = line_in(polar(PolarView::default()));
    fig.axes[0].x.limits = manual(0.0, 180.0);
    assert!(matches!(
        applied(
            &fig,
            &tx([set(
                axes,
                "projection.polar.angle_units",
                Value::AngleUnits(AngleUnits::Radians)
            )])
        ),
        Err(EditError::Invalid(_))
    ));
}

// Why: transactions that change the new views travel between the viewer, sessions and
// clients in other languages, so a set of each new value type, with the awkward values
// (a fractional count per turn, a negative zero direction, an absent label spoke), must
// survive both transaction formats exactly.
#[test]
fn sets_of_the_new_value_types_survive_both_transaction_formats() {
    let values = [
        Value::Projection(polar(PolarView {
            angle_units: AngleUnits::PerTurn(7.5),
            zero_deg: -0.0,
            hole: 0.125,
            radial_labels_at: None,
        })),
        Value::Projection(Projection::Cylindrical {
            view3d: View3d {
                zoom: 1.5,
                ..View3d::default()
            },
            polar: PolarView {
                radial_labels_at: Some(1.0),
                ..PolarView::default()
            },
        }),
        Value::Projection(spherical(SphericalView {
            angle_units: AngleUnits::Radians,
            polar_angle: PolarAngle::Inclination,
        })),
        Value::PolarView(PolarView {
            angle_units: AngleUnits::Radians,
            zero_deg: 270.0,
            hole: 0.0,
            radial_labels_at: Some(-PI),
        }),
        Value::SphericalView(SphericalView::default()),
        Value::AngleUnits(AngleUnits::Degrees),
        Value::AngleUnits(AngleUnits::Radians),
        Value::AngleUnits(AngleUnits::PerTurn(24.0)),
        Value::PolarAngle(PolarAngle::Elevation),
        Value::PolarAngle(PolarAngle::Inclination),
    ];
    let transaction = tx(values
        .iter()
        .cloned()
        .map(|value| set(NodeId(2), "projection", value)));
    let decoded = Transaction::from_protobuf(&transaction.to_protobuf()).unwrap();
    assert_eq!(decoded, transaction);
    let Edit::Set {
        value: Value::Projection(Projection::Polar { polar, .. }),
        ..
    } = &decoded.edits[0]
    else {
        panic!("the first set holds polar axes");
    };
    assert!(polar.zero_deg.is_sign_negative(), "negative zero survives");
    let reread = Transaction::from_json(&transaction.to_json()).unwrap();
    assert_eq!(reread, transaction);
    for (value, decoded) in values.iter().zip(&decoded.edits) {
        let Edit::Set { value: got, .. } = decoded else {
            unreachable!()
        };
        assert_eq!(got.value_type(), value.value_type(), "{value:?}");
    }
}

// ---------------------------------------------------------------------------------
// Choices
// ---------------------------------------------------------------------------------

fn choose(value_type: ValueType, replaced: Option<&Value>, label: &str) -> Value {
    choices(value_type, replaced)
        .into_iter()
        .find(|choice| choice.label == label)
        .unwrap_or_else(|| panic!("{value_type:?} offers {label:?}"))
        .value
}

// Why: the property editor offers the projection as a combo box; the user must find all
// five projections there under plain names, in the order of increasing generality that
// the documentation uses.
#[test]
fn the_projection_choices_are_the_five_projections_in_order() {
    let labels: Vec<&str> = choices(ValueType::Projection, None)
        .iter()
        .map(|choice| choice.label)
        .collect();
    assert_eq!(
        labels,
        [
            "Two-dimensional",
            "Three-dimensional",
            "Polar",
            "Cylindrical",
            "Spherical"
        ]
    );
    assert_eq!(
        choose(ValueType::Projection, None, "Polar"),
        Value::Projection(polar(PolarView::default()))
    );
    assert_eq!(
        choose(ValueType::Projection, None, "Cylindrical"),
        Value::Projection(cylindrical(PolarView::default()))
    );
    assert_eq!(
        choose(ValueType::Projection, None, "Spherical"),
        Value::Projection(spherical(SphericalView::default()))
    );
}

// Why: switching between 3D projections must keep the camera the user is looking through,
// and switching between polar and cylindrical axes must keep the polar view (its zero
// direction, units and hole); otherwise every switch would throw the view away and the
// user would have to rebuild it.
#[test]
fn a_projection_choice_carries_the_camera_and_the_polar_view_across() {
    let camera = View3d {
        azimuth_deg: 12.0,
        elevation_deg: 45.0,
        zoom: 1.5,
        pan_x: 0.1,
        pan_y: -0.2,
    };
    let view = PolarView {
        angle_units: AngleUnits::Radians,
        zero_deg: 90.0,
        hole: 0.25,
        radial_labels_at: Some(1.0),
    };

    let three_d = Value::Projection(Projection::ThreeD { view3d: camera });
    assert_eq!(
        choose(ValueType::Projection, Some(&three_d), "Cylindrical"),
        Value::Projection(Projection::Cylindrical {
            view3d: camera,
            polar: PolarView::default(),
        })
    );
    assert_eq!(
        choose(ValueType::Projection, Some(&three_d), "Spherical"),
        Value::Projection(Projection::Spherical {
            view3d: camera,
            spherical: SphericalView::default(),
        })
    );

    let cylinder = Value::Projection(Projection::Cylindrical {
        view3d: camera,
        polar: view,
    });
    assert_eq!(
        choose(ValueType::Projection, Some(&cylinder), "Polar"),
        Value::Projection(polar(view))
    );
    assert_eq!(
        choose(ValueType::Projection, Some(&cylinder), "Three-dimensional"),
        Value::Projection(Projection::ThreeD { view3d: camera })
    );
    let Value::Projection(Projection::Spherical { view3d, .. }) =
        choose(ValueType::Projection, Some(&cylinder), "Spherical")
    else {
        panic!("the choice is spherical");
    };
    assert_eq!(view3d, camera);

    // The pan of polar axes is kept when polar axes are chosen again, and a projection
    // without one gives polar axes centred in the plot area.
    let pan = View2d {
        pan_x: 0.25,
        pan_y: -0.125,
    };
    let panned = Value::Projection(Projection::Polar {
        polar: view,
        view: pan,
    });
    assert_eq!(
        choose(ValueType::Projection, Some(&panned), "Polar"),
        panned
    );
    assert_eq!(
        choose(ValueType::Projection, Some(&three_d), "Polar"),
        Value::Projection(polar(PolarView::default()))
    );

    let flat = Value::Projection(polar(view));
    assert_eq!(
        choose(ValueType::Projection, Some(&flat), "Cylindrical"),
        Value::Projection(Projection::Cylindrical {
            view3d: View3d::default(),
            polar: view,
        })
    );

    let sphere = Value::Projection(Projection::Spherical {
        view3d: camera,
        spherical: SphericalView::default(),
    });
    assert_eq!(
        choose(ValueType::Projection, Some(&sphere), "Three-dimensional"),
        Value::Projection(Projection::ThreeD { view3d: camera })
    );
    let Value::Projection(Projection::Cylindrical { view3d, .. }) =
        choose(ValueType::Projection, Some(&sphere), "Cylindrical")
    else {
        panic!("the choice is cylindrical");
    };
    assert_eq!(view3d, camera);
}

// ---------------------------------------------------------------------------------
// The view overlay
// ---------------------------------------------------------------------------------

/// A figure of a polar axes and a cylindrical axes, each holding a line, with the zero
/// direction of the polar axes at north.
fn overlay_fixture() -> (Figure, NodeId, NodeId) {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(1, 2);
    let p = b.axes2d(0, 0);
    b.axes(p).projection = polar(PolarView {
        zero_deg: 90.0,
        ..PolarView::default()
    });
    let c = b.axes2d(0, 1);
    b.axes(c).projection = cylindrical(PolarView::default());
    let x = b.vector(&[1.0, 2.0, 3.0]);
    let y = b.vector(&[1.0, 4.0, 9.0]);
    let (lp, lc) = (b.node(), b.node());
    b.push(p, line(lp, x, y));
    b.push(c, line(lc, x, y));
    (b.build(), p, c)
}

fn record(overlay: &mut Overlay, node: NodeId, at: &str, value: Value) {
    overlay.record(&tx([set(node, at, value)])).unwrap();
}

fn keys(overlay: &Overlay) -> Vec<(NodeId, String)> {
    overlay
        .entries()
        .iter()
        .map(|entry| (entry.node, entry.path.to_string()))
        .collect()
}

// Why: turning a polar plot by dragging changes its zero direction, and orbiting a
// cylindrical or spherical axes changes its camera, and panning a polar axes moves it;
// all are changes of view, so a double-click must reset them, restoring the zero direction
// the figure was written with and centring the polar axes again. The hole, the units and
// the other properties are choices about content and must survive the reset.
#[test]
fn resetting_the_view_restores_the_zero_direction_and_the_camera_and_keeps_the_rest() {
    let (source, p, c) = overlay_fixture();
    let mut overlay = Overlay::new();
    record(
        &mut overlay,
        p,
        "projection.polar.zero_deg",
        Value::Double(10.0),
    );
    record(&mut overlay, p, "projection.polar.hole", Value::Double(0.5));
    record(&mut overlay, p, "y.limits", Value::Limits(manual(0.0, 5.0)));
    record(
        &mut overlay,
        p,
        "projection.view.pan_x",
        Value::Double(0.25),
    );
    record(
        &mut overlay,
        c,
        "projection.view3d.zoom",
        Value::Double(2.0),
    );
    record(
        &mut overlay,
        c,
        "projection.polar.zero_deg",
        Value::Double(30.0),
    );
    record(
        &mut overlay,
        c,
        "projection.polar.angle_units",
        Value::AngleUnits(AngleUnits::Radians),
    );

    let composed = overlay.compose(&source);
    assert_eq!(composed.dropped, vec![]);
    let Projection::Polar { polar: shown, .. } = find_axes(&composed.figure, p).projection else {
        panic!("the axes stays polar");
    };
    assert_eq!(shown.zero_deg, 10.0);
    let Projection::Polar { view: moved, .. } = find_axes(&composed.figure, p).projection else {
        panic!("the axes stays polar");
    };
    assert_eq!(moved.pan_x, 0.25);

    overlay.reset_view(p);
    assert_eq!(
        keys(&overlay),
        [
            (p, "projection.polar.hole".to_owned()),
            (c, "projection.view3d.zoom".to_owned()),
            (c, "projection.polar.zero_deg".to_owned()),
            (c, "projection.polar.angle_units".to_owned()),
        ]
    );
    let composed = overlay.compose(&source);
    let Projection::Polar { polar: shown, .. } = find_axes(&composed.figure, p).projection else {
        panic!("the axes stays polar");
    };
    assert_eq!(
        shown.zero_deg, 90.0,
        "the written zero direction is restored"
    );
    assert_eq!(shown.hole, 0.5, "the hole is kept");
    let Projection::Polar { view: moved, .. } = find_axes(&composed.figure, p).projection else {
        panic!("the axes stays polar");
    };
    assert_eq!(moved, View2d::default(), "the pan is reset");

    overlay.reset_view(c);
    assert_eq!(
        keys(&overlay),
        [
            (p, "projection.polar.hole".to_owned()),
            (c, "projection.polar.angle_units".to_owned()),
        ]
    );
}

// Why: a spherical axes is orbited like any 3D axes, so its camera is a view property and
// Reset view must clear it with every other view.
#[test]
fn resetting_every_view_clears_the_camera_of_spherical_axes() {
    let (source, axes, _) = line_in(spherical(SphericalView::default()));
    let mut overlay = Overlay::new();
    record(
        &mut overlay,
        axes,
        "projection.view3d.azimuth_deg",
        Value::Double(5.0),
    );
    record(
        &mut overlay,
        axes,
        "projection.spherical.polar_angle",
        Value::PolarAngle(PolarAngle::Inclination),
    );
    overlay.reset_all_views();
    assert_eq!(
        keys(&overlay),
        [(axes, "projection.spherical.polar_angle".to_owned())]
    );
    assert_eq!(overlay.compose(&source).dropped, vec![]);
}
