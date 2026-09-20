//! Persistence of figures as `.fig.json`: round trips, the wire format and schema
//! version compatibility.

mod common;

use std::collections::BTreeMap;

use common::{kitchen_sink_figure, single_line_figure};
use ironlab_ir::*;
use proptest::prelude::*;
use serde_json::{Value, json};

const DECAY_FIXTURE: &str = include_str!("fixtures/decay.fig.json");

/// The figure that `fixtures/decay.fig.json` describes, built independently of serde.
fn decay_figure() -> Figure {
    let mut data = BTreeMap::new();
    data.insert(DataId(0), NdArray::vector(vec![0.0, 1.0, 2.0]));
    data.insert(DataId(7), NdArray::vector(vec![1.0, f64::NAN, 0.135]));
    Figure {
        schema_version: "0.2.0".to_owned(),
        id: NodeId(1),
        title: Some(Text::new("Decay of $e^{-t}$")),
        size: FigureSize {
            width_mm: 120.0,
            height_mm: 80.0,
        },
        font_set: FontSetId::StixTwo,
        font_size_pt: 9.0,
        background: Color::WHITE,
        layout: TileLayout { rows: 1, cols: 1 },
        data,
        axes: vec![Axes {
            id: NodeId(2),
            cell: Cell {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            projection: Projection::TwoD,
            title: None,
            x: Axis {
                label: Some(Text::new("$t$ (s)")),
                scale: Scale::Linear,
                limits: Limits::Auto,
                grid: false,
            },
            y: Axis {
                label: None,
                scale: Scale::Log,
                limits: Limits::Manual { min: 0.1, max: 1.0 },
                grid: true,
            },
            z: Axis::default(),
            box_: true,
            colormap: ColormapName::Viridis,
            clim: Limits::Auto,
            legend: Some(Legend {
                location: LegendLocation::NorthEast,
                boxed: true,
            }),
            artists: vec![Artist::Line(Line {
                id: NodeId(3),
                display_name: Some(Text::plain("decay, $5")),
                visible: true,
                x: DataId(0),
                y: DataId(7),
                z: None,
                line: LineStyle {
                    color: ColorSpec::Rgba {
                        color: Color::rgba(0.0, 114.0 / 255.0, 178.0 / 255.0, 204.0 / 255.0),
                    },
                    width_pt: 0.75,
                    dash: DashStyle::DashDot,
                },
                marker: MarkerStyle {
                    shape: MarkerShape::TriangleUp,
                    size_pt: 4.0,
                    face: ColorSpec::None,
                    edge: ColorSpec::Auto,
                },
            })],
        }],
        links: vec![],
        provenance: Provenance {
            ironlab_version: "0.1.0".to_owned(),
            typesetter: "latex-rust 1.0.2".to_owned(),
            fonts: vec!["STIX Two Text".to_owned(), "STIX Two Math".to_owned()],
        },
        parameters: BTreeMap::new(),
        id_allocator: NodeIdAllocator::default(),
    }
}

/// Returns the components of [`SCHEMA_VERSION`], so that the version tests describe
/// versions relative to the supported one and remain meaningful when it changes.
fn supported_version() -> [u64; 3] {
    let parts: Vec<u64> = SCHEMA_VERSION
        .split('.')
        .map(|part| part.parse().expect("SCHEMA_VERSION is major.minor.patch"))
        .collect();
    parts
        .try_into()
        .expect("SCHEMA_VERSION has three components")
}

/// Returns the decay fixture as a JSON value with its schema version replaced.
fn decay_fixture_with_version(version: Value) -> String {
    let mut value: Value = serde_json::from_str(DECAY_FIXTURE).unwrap();
    value["schema_version"] = version;
    value.to_string()
}

// Why: `.fig.json` is the persistence format, so saving and loading must lose nothing,
// for every artist type and every enum variant, including NaN data and hidden artists.
#[test]
fn every_artist_and_enum_variant_survives_a_json_round_trip() {
    let original = kitchen_sink_figure();
    let restored = Figure::from_json(&original.to_json()).expect("own output parses");
    assert_eq!(restored, original);
}

// Why: saving a figure that was just loaded must produce identical text, so that
// version-controlled figure files do not churn when opened and saved again.
#[test]
fn saving_a_loaded_figure_reproduces_the_same_text() {
    let first = kitchen_sink_figure().to_json();
    let second = Figure::from_json(&first).unwrap().to_json();
    assert_eq!(first, second);
}

// Why: a default figure is the starting point of every user figure, so its defaults
// must persist exactly rather than being replaced by different values on load.
#[test]
fn default_figure_survives_a_json_round_trip() {
    let original = Figure::new();
    assert_eq!(Figure::from_json(&original.to_json()).unwrap(), original);
}

// Why: the viewer edits a figure through the IR (zooming a 3D axes changes its view and,
// through a z link, the z limits of another axes; panning a log axes produces arbitrary
// floating-point limits; clicking a legend entry hides an artist), and saving must write
// exactly what is on screen: the reopened figure must be identical, and saving it again
// must not change the file.
#[test]
fn viewer_edits_survive_save_and_reopen() {
    let mut fig = Figure::from_json(&kitchen_sink_figure().to_json()).unwrap();
    let (lines, three, surfaces) = (fig.axes[0].id, fig.axes[4].id, fig.axes[5].id);
    let z_limits = Limits::Manual {
        min: 0.1 + 0.2,
        max: 7.0 / 3.0,
    };
    let x_limits = Limits::Manual {
        min: std::f64::consts::FRAC_1_SQRT_2,
        max: std::f64::consts::PI * 1.0e3,
    };
    let view3d = View3d {
        azimuth_deg: -37.5 + 123.456_789,
        elevation_deg: 89.999_999_999_999_99,
        zoom: 1.0 / 3.0,
        pan_x: -0.1 + 0.2,
        pan_y: 1.0e-17,
    };

    fig.set_limits(three, Dimension::Z, z_limits).unwrap();
    fig.set_limits(lines, Dimension::X, x_limits).unwrap();
    fig.axes_mut(three).unwrap().projection = Projection::ThreeD { view3d };
    let hidden = fig.axes[5].artists[0].id();
    fig.artist_mut(hidden).unwrap().set_visible(false);

    let saved = fig.to_json();
    let reopened = Figure::from_json(&saved).unwrap();
    assert_eq!(reopened, fig);
    assert_eq!(reopened.axes[5].id, surfaces);
    assert_eq!(
        reopened.axes[5].z.limits, z_limits,
        "z link did not propagate"
    );
    assert_eq!(reopened.axes[4].projection, Projection::ThreeD { view3d });
    assert!(!reopened.axes[5].artists[0].visible());
    assert_eq!(reopened.to_json(), saved);
}

fn finite_f64() -> impl Strategy<Value = f64> {
    prop::num::f64::POSITIVE
        | prop::num::f64::NEGATIVE
        | prop::num::f64::NORMAL
        | prop::num::f64::SUBNORMAL
        | prop::num::f64::ZERO
}

proptest! {
    // Why: pan, zoom and rotation produce arbitrary finite values for limits and views,
    // and properties outside the data table do not pass through the array serialiser, so
    // they too must reload bit for bit (including negative zero and subnormals), or a
    // reopened figure would drift from the saved one.
    #[test]
    fn arbitrary_finite_limits_and_views_reload_bit_for_bit(
        min in finite_f64(),
        max in finite_f64(),
        azimuth in finite_f64(),
        zoom in finite_f64(),
        pan in finite_f64(),
        width in finite_f64(),
    ) {
        let (mut fig, _, _) = single_line_figure();
        fig.size.width_mm = width;
        fig.axes[0].x.limits = Limits::Manual { min, max };
        fig.axes[0].projection = Projection::ThreeD {
            view3d: View3d { azimuth_deg: azimuth, elevation_deg: 30.0, zoom, pan_x: pan, pan_y: 0.0 },
        };

        let reopened = Figure::from_json(&fig.to_json()).unwrap();

        let Limits::Manual { min: rmin, max: rmax } = reopened.axes[0].x.limits else {
            panic!("limits changed kind");
        };
        let Projection::ThreeD { view3d } = reopened.axes[0].projection else {
            panic!("projection changed kind");
        };
        let pairs = [
            (min, rmin),
            (max, rmax),
            (azimuth, view3d.azimuth_deg),
            (zoom, view3d.zoom),
            (pan, view3d.pan_x),
            (width, reopened.size.width_mm),
        ];
        for (original, restored) in pairs {
            prop_assert_eq!(original.to_bits(), restored.to_bits());
        }
    }
}

// Why: other tools (future language bindings, hand-written files) produce JSON from the
// documented wire format rather than from our serde output, so a hand-written document
// must load as the intended figure.
#[test]
fn hand_written_document_loads_as_the_described_figure() {
    let loaded = Figure::from_json(DECAY_FIXTURE).expect("fixture is a valid figure");
    assert_eq!(loaded, decay_figure());
}

// Why: our own output must follow the documented wire format exactly (tag names, the
// `box` key, string data keys, integer ids, hex colours and null for NaN), not merely
// be readable by ourselves.
#[test]
fn serialised_figure_matches_the_hand_written_wire_format() {
    let written: Value = serde_json::from_str(&decay_figure().to_json()).unwrap();
    let expected: Value = serde_json::from_str(DECAY_FIXTURE).unwrap();
    assert_eq!(written, expected);
}

// Why: a figure that holds an image's pixels must save to `.fig.json` and reopen as the
// same figure: the array of bytes is written in the documented form (an `element` tag
// and integer values) and reloads with its element type, while the float arrays beside
// it are written exactly as before, and saving the reopened figure changes nothing.
#[test]
fn a_figure_with_an_array_of_bytes_survives_a_json_round_trip_in_the_documented_form() {
    let (mut fig, _, _) = single_line_figure();
    let bytes = NdArray::from_shape_u8(vec![2, 2], vec![0, 1, 254, 255]).unwrap();
    fig.data.insert(DataId(9), bytes.clone());

    let text = fig.to_json();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["data"]["9"],
        json!({ "shape": [2, 2], "element": "u8", "values": [0, 1, 254, 255] })
    );
    assert_eq!(
        value["data"]["0"],
        json!({ "shape": [3], "values": [1.0, 2.0, 3.0] })
    );

    let reopened = Figure::from_json(&text).expect("own output parses");
    assert_eq!(reopened.data[&DataId(9)], bytes);
    assert_eq!(reopened, fig);
    assert_eq!(reopened.to_json(), text);
}

// Why: the JSON produced for each tagged enum and unit enum is a public contract, and a
// renamed variant or a changed tagging strategy would silently break existing files.
#[test]
fn enum_variants_use_snake_case_names_and_a_type_tag() {
    let (mut fig, axes, _) = single_line_figure();
    fig.axes[0].projection = Projection::ThreeD {
        view3d: View3d {
            pan_x: 0.25,
            pan_y: -0.5,
            ..View3d::default()
        },
    };
    fig.axes[0].artists.push(Artist::Contour(Contour {
        id: NodeId(90),
        grid: Grid::Curvilinear {
            x: DataId(0),
            y: DataId(1),
        },
        levels: Levels::Explicit { values: vec![0.5] },
        placement: ContourPlacement::AtLevel,
        ..Contour::default()
    }));
    fig.links.push(AxisLink {
        dimension: Dimension::Z,
        axes: vec![axes],
    });
    let value: Value = serde_json::from_str(&fig.to_json()).unwrap();
    let axes_json = &value["axes"][0];

    assert_eq!(
        axes_json["projection"],
        json!({
            "type": "three_d",
            "view3d": { "azimuth_deg": -37.5, "elevation_deg": 30.0, "zoom": 1.0, "pan_x": 0.25, "pan_y": -0.5 }
        })
    );
    let contour = &axes_json["artists"][1];
    assert_eq!(contour["type"], "contour");
    assert_eq!(contour["id"], 90);
    assert_eq!(
        contour["grid"],
        json!({ "type": "curvilinear", "x": 0, "y": 1 })
    );
    assert_eq!(
        contour["levels"],
        json!({ "type": "explicit", "values": [0.5] })
    );
    assert_eq!(contour["placement"], json!({ "type": "at_level" }));
    assert_eq!(contour["line"]["color"], json!({ "type": "colormapped" }));
    assert_eq!(value["links"][0]["dimension"], "z");
    assert_eq!(value["font_set"], "stix_two");
}

// Why: files from an incompatible schema (different major or minor version) must be
// rejected with a version error rather than misread or reported as malformed JSON.
#[test]
fn incompatible_schema_versions_are_rejected() {
    let [major, minor, patch] = supported_version();
    let mut versions = vec![
        format!("{major}.{}.0", minor + 1),
        format!("{}.{minor}.{patch}", major + 1),
        format!("{}.0.0", major + 1),
    ];
    if minor > 0 {
        versions.push(format!("{major}.{}.9", minor - 1));
    }
    if major > 0 {
        versions.push(format!("{}.{minor}.{patch}", major - 1));
    }
    for version in versions {
        let result = Figure::from_json(&decay_fixture_with_version(json!(version)));
        assert!(
            matches!(result, Err(IrError::IncompatibleSchemaVersion { ref found, .. }) if *found == version),
            "version {version:?} gave {result:?}"
        );
    }
}

// Why: a version check must run before full deserialisation, so that a newer file with
// fields this build does not know is reported as a version problem.
#[test]
fn newer_minor_version_with_unknown_structure_reports_the_version() {
    let [major, minor, _] = supported_version();
    let newer = format!("{major}.{}.0", minor + 8);
    let json = json!({ "schema_version": newer, "id": "not-a-number", "novel": [] });
    let result = Figure::from_json(&json.to_string());
    assert!(
        matches!(result, Err(IrError::IncompatibleSchemaVersion { .. })),
        "{result:?}"
    );
}

// Why: patch releases of the schema are compatible by definition, so they must load.
#[test]
fn different_patch_version_is_accepted() {
    let [major, minor, patch] = supported_version();
    let version = format!("{major}.{minor}.{}", patch + 42);
    let fig = Figure::from_json(&decay_fixture_with_version(json!(version)))
        .expect("patch versions are compatible");
    assert_eq!(fig.schema_version, version);
}

// Why: a version string that is not `major.minor.patch` cannot be checked for
// compatibility and must not be silently accepted.
#[test]
fn malformed_or_missing_schema_version_is_rejected() {
    let [major, minor, patch] = supported_version();
    let malformed = [
        json!(format!("{major}.{minor}")),
        json!(format!("{major}.{minor}.{patch}.0")),
        json!("zero.one.zero"),
        json!(major),
        Value::Null,
    ];
    for version in malformed {
        let result = Figure::from_json(&decay_fixture_with_version(version.clone()));
        assert!(result.is_err(), "version {version} was accepted");
    }
    let mut value: Value = serde_json::from_str(DECAY_FIXTURE).unwrap();
    value.as_object_mut().unwrap().remove("schema_version");
    assert!(Figure::from_json(&value.to_string()).is_err());
}

// Why: a corrupt file must produce an error, not a panic or a partial figure.
#[test]
fn malformed_json_is_a_json_error() {
    let truncated = &DECAY_FIXTURE[..DECAY_FIXTURE.len() / 2];
    assert!(matches!(
        Figure::from_json(truncated),
        Err(IrError::Json(_))
    ));
}
