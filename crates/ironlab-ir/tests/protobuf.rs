//! Persistence of figures as Protocol Buffers (`.fig`): lossless round trips, agreement
//! with the JSON format, defaults for absent values, forward compatibility, schema
//! version checks and the size of encoded data.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{
    SPECIAL_F64, float_bits, kitchen_sink_figure, single_line_figure, special_values_figure,
};
use ironlab_ir::*;
use proptest::prelude::*;
use prost::Message;

/// Returns the key of a protobuf field: its number shifted left by three bits, combined
/// with its wire type, encoded as a varint.
fn field_key(number: u64, wire_type: u64) -> Vec<u8> {
    varint((number << 3) | wire_type)
}

/// Encodes an unsigned integer as a protobuf varint.
fn varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            bytes.push(byte);
            return bytes;
        }
        bytes.push(byte | 0x80);
    }
}

/// Encodes a length-delimited field (wire type 2) holding the given bytes.
fn length_delimited(number: u64, payload: &[u8]) -> Vec<u8> {
    [
        field_key(number, 2),
        varint(payload.len() as u64),
        payload.to_vec(),
    ]
    .concat()
}

/// Returns fields with numbers that no version of the schema uses, one of each wire
/// type (varint, 64-bit, length-delimited and 32-bit), as a later version might add.
fn unknown_fields() -> Vec<u8> {
    [
        field_key(1000, 0),
        varint(42),
        field_key(1001, 1),
        2.5f64.to_le_bytes().to_vec(),
        length_delimited(1002, &[field_key(1, 0), varint(7)].concat()),
        field_key(1003, 5),
        1.5f32.to_le_bytes().to_vec(),
    ]
    .concat()
}

/// Encodes a wire figure directly, bypassing the domain conversion, to construct bytes
/// that IronLAB itself would not write.
fn encode_wire(figure: &wire::Figure) -> Vec<u8> {
    figure.encode_to_vec()
}

/// A wire figure that holds only the supported schema version and the identifier of
/// the figure, which has no default. The identifier is that of the default figure.
fn minimal_wire_figure() -> wire::Figure {
    wire::Figure {
        schema_version: SCHEMA_VERSION.to_owned(),
        id: Some(Figure::default().id.0),
        ..wire::Figure::default()
    }
}

/// The provenance that an absent provenance message decodes to: a file that records no
/// provenance was not written by this build, so it must not claim to have been.
fn empty_provenance() -> Provenance {
    Provenance {
        ironlab_version: String::new(),
        typesetter: String::new(),
        fonts: vec![],
    }
}

/// Encodes a varint field (wire type 0).
fn varint_field(number: u64, value: u64) -> Vec<u8> {
    [field_key(number, 0), varint(value)].concat()
}

/// Encodes a `double` field (wire type 1).
fn double_field(number: u64, value: f64) -> Vec<u8> {
    [field_key(number, 1), value.to_le_bytes().to_vec()].concat()
}

/// Encodes a `float` field (wire type 5).
fn float_field(number: u64, value: f32) -> Vec<u8> {
    [field_key(number, 5), value.to_le_bytes().to_vec()].concat()
}

/// Encodes a packed repeated `double` field.
fn packed_doubles(number: u64, values: &[f64]) -> Vec<u8> {
    let payload: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    length_delimited(number, &payload)
}

/// Encodes a packed repeated varint field.
fn packed_varints(number: u64, values: &[u64]) -> Vec<u8> {
    let payload: Vec<u8> = values.iter().flat_map(|v| varint(*v)).collect();
    length_delimited(number, &payload)
}

fn wire_text(content: &str, interpreter: wire::Interpreter) -> Option<wire::Text> {
    Some(wire::Text {
        content: content.to_owned(),
        interpreter: interpreter as i32,
    })
}

fn wire_color(r: f32, g: f32, b: f32, a: f32) -> wire::Color {
    wire::Color { r, g, b, a }
}

fn wire_spec(kind: wire::ColorSpecKind) -> Option<wire::ColorSpec> {
    Some(wire::ColorSpec { kind: Some(kind) })
}

fn wire_rgba(r: f32, g: f32, b: f32, a: f32) -> Option<wire::ColorSpec> {
    wire_spec(wire::ColorSpecKind::Rgba(wire::ColorSpecRgba {
        color: Some(wire_color(r, g, b, a)),
    }))
}

fn wire_auto_color() -> Option<wire::ColorSpec> {
    wire_spec(wire::ColorSpecKind::Auto(wire::ColorSpecAuto {}))
}

fn wire_no_color() -> Option<wire::ColorSpec> {
    wire_spec(wire::ColorSpecKind::None(wire::ColorSpecNone {}))
}

fn wire_colormapped() -> Option<wire::ColorSpec> {
    wire_spec(wire::ColorSpecKind::Colormapped(
        wire::ColorSpecColormapped {},
    ))
}

fn wire_manual(min: f64, max: f64) -> Option<wire::Limits> {
    Some(wire::Limits {
        kind: Some(wire::LimitsKind::Manual(wire::LimitsManual {
            min: Some(min),
            max: Some(max),
        })),
    })
}

fn wire_auto_limits() -> Option<wire::Limits> {
    Some(wire::Limits {
        kind: Some(wire::LimitsKind::Auto(wire::LimitsAuto {})),
    })
}

fn wire_axis(
    label: Option<&str>,
    scale: wire::Scale,
    limits: Option<wire::Limits>,
    grid: bool,
) -> Option<wire::Axis> {
    Some(wire::Axis {
        label: label.and_then(|l| wire_text(l, wire::Interpreter::Latex)),
        scale: scale as i32,
        limits,
        grid: Some(grid),
    })
}

fn wire_line_style(
    color: Option<wire::ColorSpec>,
    width_pt: f64,
    dash: wire::DashStyle,
) -> Option<wire::LineStyle> {
    Some(wire::LineStyle {
        color,
        width_pt: Some(width_pt),
        dash: dash as i32,
    })
}

fn wire_marker(
    shape: wire::MarkerShape,
    size_pt: f64,
    face: Option<wire::ColorSpec>,
    edge: Option<wire::ColorSpec>,
) -> Option<wire::MarkerStyle> {
    Some(wire::MarkerStyle {
        shape: shape as i32,
        size_pt: Some(size_pt),
        face,
        edge,
    })
}

fn wire_artist(kind: wire::ArtistKind) -> wire::Artist {
    wire::Artist { kind: Some(kind) }
}

fn wire_rectilinear(x: u64, y: u64) -> Option<wire::Grid> {
    Some(wire::Grid {
        kind: Some(wire::GridKind::Rectilinear(wire::GridRectilinear {
            x: Some(x),
            y: Some(y),
        })),
    })
}

fn wire_curvilinear(x: u64, y: u64) -> Option<wire::Grid> {
    Some(wire::Grid {
        kind: Some(wire::GridKind::Curvilinear(wire::GridCurvilinear {
            x: Some(x),
            y: Some(y),
        })),
    })
}

fn wire_quiver_scale(kind: wire::QuiverScaleKind) -> Option<wire::QuiverScale> {
    Some(wire::QuiverScale { kind: Some(kind) })
}

// ---------------------------------------------------------------------------------
// Lossless round trips
// ---------------------------------------------------------------------------------

// Why: `.fig` is the default persistence format, so saving and loading must lose
// nothing, for every artist type and every variant of every enum, including NaN data,
// hidden artists, links and provenance.
#[test]
fn every_artist_and_enum_variant_survives_a_protobuf_round_trip() {
    let original = kitchen_sink_figure();
    let restored = Figure::from_protobuf(&original.to_protobuf()).expect("own output decodes");
    assert_eq!(restored, original);
}

// Why: a default figure is the starting point of every user figure. Proto3 omits many
// zero-valued fields from the wire, and the IR's defaults (such as a visible artist, a
// box drawn, a 1 × 1 layout) are often not zero, so they must reload as themselves.
#[test]
fn default_figure_survives_a_protobuf_round_trip() {
    let original = Figure::new();
    assert_eq!(
        Figure::from_protobuf(&original.to_protobuf()).unwrap(),
        original
    );
}

// Why: saving a figure that was just loaded must produce identical bytes, so that
// version-controlled `.fig` files do not churn when opened and saved again, and so that
// equal figures can be compared or deduplicated by their encoding.
#[test]
fn saving_a_loaded_figure_reproduces_the_same_bytes() {
    let first = kitchen_sink_figure().to_protobuf();
    let second = Figure::from_protobuf(&first).unwrap().to_protobuf();
    assert_eq!(first, second);
}

// Why: data arrays hold measured values in which NaN marks missing samples, and pan,
// zoom and rotation produce arbitrary limits and views. Unlike JSON, the binary format
// stores IEEE 754 bits, so every value in every float field (NaN with its sign and
// payload, infinities, negative zero, subnormals and the extremes) must reload bit for
// bit; `Figure`'s equality cannot check this, because it treats NaNs as equal and
// negative zero as zero.
#[test]
fn special_float_values_reload_bit_for_bit_in_arrays_and_scalar_fields() {
    let original = special_values_figure();
    let expected = float_bits(&original);
    assert!(
        expected.len() > SPECIAL_F64.len() * 4,
        "the fixture should place each special value in several fields"
    );
    let restored = Figure::from_protobuf(&original.to_protobuf()).unwrap();
    let found = float_bits(&restored);
    assert_eq!(found.len(), expected.len(), "fields were added or lost");
    for ((path, want), (_, got)) in expected.iter().zip(&found) {
        assert_eq!(
            want,
            got,
            "{path}: {:e} reloaded as {:e}",
            f64::from_bits(*want),
            f64::from_bits(*got)
        );
    }
}

proptest! {
    // Why: the special values above are chosen by hand; any bit pattern at all, in a
    // scalar field with presence or in a packed array, must also survive, so that no
    // normalisation of NaN payloads or signs happens anywhere in the encoder or decoder.
    #[test]
    fn arbitrary_float_bits_reload_bit_for_bit(
        min in any::<u64>(),
        max in any::<u64>(),
        azimuth in any::<u64>(),
        pan in any::<u64>(),
        width in any::<u64>(),
        sample in any::<u64>(),
    ) {
        let (mut fig, _, _) = single_line_figure();
        fig.size.width_mm = f64::from_bits(width);
        fig.axes[0].x.limits = Limits::Manual { min: f64::from_bits(min), max: f64::from_bits(max) };
        fig.axes[0].projection = Projection::ThreeD {
            view3d: View3d {
                azimuth_deg: f64::from_bits(azimuth),
                pan_x: f64::from_bits(pan),
                pan_y: 0.0,
                ..View3d::default()
            },
        };
        fig.data.get_mut(&DataId(0)).unwrap().values[1] = f64::from_bits(sample);

        let restored = Figure::from_protobuf(&fig.to_protobuf()).unwrap();
        prop_assert_eq!(float_bits(&restored), float_bits(&fig));
    }
}

// Why: JSON stores a colour as eight bits per component, but the binary format stores
// the in-memory `f32` components, so a colour produced by interpolation or picked in the
// viewer must reload exactly rather than rounded to the nearest of 256 levels.
#[test]
fn colours_reload_at_full_precision() {
    let (mut fig, _, _) = single_line_figure();
    let colour = Color::rgba(0.123_456_79, 1.0 / 3.0, f32::MIN_POSITIVE, 0.999_999_9);
    fig.background = colour;
    let restored = Figure::from_protobuf(&fig.to_protobuf()).unwrap();
    let bits = |c: Color| [c.r, c.g, c.b, c.a].map(f32::to_bits);
    assert_eq!(bits(restored.background), bits(colour));
}

// Why: identifiers and layout counts are unsigned integers that a figure built by a
// script or a remote client may set anywhere in their range, and varint encoding must
// not truncate the largest values.
#[test]
fn extreme_identifiers_and_counts_survive_a_round_trip() {
    let (mut fig, _, _) = single_line_figure();
    fig.id = NodeId(u64::MAX);
    fig.axes[0].cell = Cell {
        row: u32::MAX,
        col: u32::MAX - 1,
        row_span: u32::MAX,
        col_span: 1,
    };
    let array = fig.data.remove(&DataId(0)).unwrap();
    fig.data.insert(DataId(u64::MAX), array);
    let Artist::Line(line) = &mut fig.axes[0].artists[0] else {
        panic!("fixture holds a line");
    };
    line.x = DataId(u64::MAX);
    assert_eq!(Figure::from_protobuf(&fig.to_protobuf()).unwrap(), fig);
}

// ---------------------------------------------------------------------------------
// Agreement between the binary and JSON formats
// ---------------------------------------------------------------------------------

// Why: JSON remains a supported secondary format, so converting a `.fig` to `.fig.json`
// (for debugging or hand editing) and back must reproduce the same bytes; otherwise the
// two formats describe different figures. The kitchen-sink colours are multiples of
// 1/255, which the eight-bit JSON representation holds exactly.
#[test]
fn protobuf_to_json_to_protobuf_reproduces_the_same_bytes() {
    let bytes = kitchen_sink_figure().to_protobuf();
    let json = Figure::from_protobuf(&bytes).unwrap().to_json();
    let via_json = Figure::from_json(&json).unwrap();
    assert_eq!(via_json.to_protobuf(), bytes);
}

// ---------------------------------------------------------------------------------
// The meaning of each wire field
// ---------------------------------------------------------------------------------

// Why: a round trip cannot detect a mapping that is wrong in the same way in both
// directions, such as the x and y data identifiers exchanged by both the encoder and the
// decoder, two oneof variants or two grid kinds exchanged, or the keys of the data table
// renumbered; IronLAB would still reload its own figures, but readers and writers in
// other languages, which rely on the generated schema, would see the fields exchanged.
// Decoding a wire figure built by hand, in which every pair of fields of the same type
// holds different values and every oneof variant appears, pins each wire field to the
// domain field that it names; the round-trip tests then pin the encoder as well.
#[test]
fn each_wire_field_decodes_into_the_domain_field_that_it_names() {
    // Non-contiguous data identifiers, so that a decoder that renumbers keys is caught.
    const A: u64 = 7;
    const B: u64 = 10;
    const C: u64 = 12;
    const D: u64 = 15;

    let wire_figure = wire::Figure {
        schema_version: SCHEMA_VERSION.to_owned(),
        id: Some(1),
        title: wire_text("figure title", wire::Interpreter::None),
        size: Some(wire::FigureSize {
            width_mm: Some(101.0),
            height_mm: Some(102.0),
        }),
        font_set: wire::FontSetId::StixTwo as i32,
        font_size_pt: Some(7.5),
        background: Some(wire_color(0.125, 0.25, 0.375, 0.5)),
        layout: Some(wire::TileLayout {
            rows: Some(2),
            cols: Some(3),
        }),
        data: BTreeMap::from([
            (
                A,
                wire::NdArray {
                    shape: vec![2],
                    values: vec![1.0, 2.0],
                },
            ),
            (
                B,
                wire::NdArray {
                    shape: vec![1, 3],
                    values: vec![3.0, 4.0, 5.0],
                },
            ),
            (
                C,
                wire::NdArray {
                    shape: vec![3, 1],
                    values: vec![6.0, 7.0, 8.0],
                },
            ),
            (
                D,
                wire::NdArray {
                    shape: vec![2, 2],
                    values: vec![9.0, 10.0, 11.0, 12.0],
                },
            ),
        ]),
        axes: vec![
            wire::Axes {
                id: Some(20),
                cell: Some(wire::Cell {
                    row: Some(1),
                    col: Some(2),
                    row_span: Some(3),
                    col_span: Some(4),
                }),
                projection: Some(wire::Projection {
                    kind: Some(wire::ProjectionKind::ThreeD(wire::ProjectionThreeD {
                        view3d: Some(wire::View3d {
                            azimuth_deg: Some(11.0),
                            elevation_deg: Some(12.0),
                            zoom: Some(13.0),
                            pan_x: Some(14.0),
                            pan_y: Some(15.0),
                        }),
                    })),
                }),
                title: wire_text("axes title", wire::Interpreter::Latex),
                x: wire_axis(
                    Some("x label"),
                    wire::Scale::Log,
                    wire_manual(1.0, 2.0),
                    true,
                ),
                y: wire_axis(
                    Some("y label"),
                    wire::Scale::Linear,
                    wire_manual(3.0, 4.0),
                    false,
                ),
                z: wire_axis(None, wire::Scale::Log, wire_auto_limits(), true),
                r#box: Some(false),
                colormap: wire::ColormapName::Magma as i32,
                clim: wire_manual(5.0, 6.0),
                legend: Some(wire::Legend {
                    location: wire::LegendLocation::SouthWest as i32,
                    boxed: Some(false),
                }),
                artists: vec![
                    wire_artist(wire::ArtistKind::Line(wire::Line {
                        id: Some(30),
                        display_name: wire_text("line", wire::Interpreter::Latex),
                        visible: Some(false),
                        x: Some(A),
                        y: Some(B),
                        z: Some(C),
                        line: wire_line_style(
                            wire_rgba(0.5, 0.625, 0.75, 0.875),
                            1.25,
                            wire::DashStyle::DashDot,
                        ),
                        marker: wire_marker(
                            wire::MarkerShape::TriangleDown,
                            2.5,
                            wire_no_color(),
                            wire_colormapped(),
                        ),
                    })),
                    wire_artist(wire::ArtistKind::Scatter(wire::Scatter {
                        id: Some(31),
                        display_name: wire_text("scatter", wire::Interpreter::None),
                        visible: Some(true),
                        x: Some(B),
                        y: Some(C),
                        z: Some(A),
                        size: Some(wire::ScatterSize {
                            kind: Some(wire::ScatterSizeKind::Data(wire::ScatterSizeData {
                                data: Some(C),
                            })),
                        }),
                        color: Some(wire::ScatterColor {
                            kind: Some(wire::ScatterColorKind::Data(wire::ScatterColorData {
                                data: Some(D),
                            })),
                        }),
                        marker: wire_marker(
                            wire::MarkerShape::Square,
                            3.5,
                            wire_auto_color(),
                            wire_no_color(),
                        ),
                    })),
                    wire_artist(wire::ArtistKind::Scatter(wire::Scatter {
                        id: Some(32),
                        display_name: None,
                        visible: Some(true),
                        x: Some(C),
                        y: Some(A),
                        z: None,
                        size: Some(wire::ScatterSize {
                            kind: Some(wire::ScatterSizeKind::Scalar(wire::ScatterSizeScalar {
                                value: Some(4.5),
                            })),
                        }),
                        color: Some(wire::ScatterColor {
                            kind: Some(wire::ScatterColorKind::Spec(wire::ScatterColorSpec {
                                spec: wire_colormapped(),
                            })),
                        }),
                        marker: wire_marker(
                            wire::MarkerShape::Plus,
                            5.5,
                            wire_rgba(0.0625, 0.1875, 0.3125, 0.4375),
                            wire_auto_color(),
                        ),
                    })),
                    wire_artist(wire::ArtistKind::Contour(wire::Contour {
                        id: Some(33),
                        display_name: wire_text("contour", wire::Interpreter::Latex),
                        visible: Some(false),
                        grid: wire_curvilinear(B, C),
                        z: Some(D),
                        levels: Some(wire::Levels {
                            kind: Some(wire::LevelsKind::Explicit(wire::LevelsExplicit {
                                values: vec![1.0, 2.0, 3.0],
                            })),
                        }),
                        fill: Some(true),
                        placement: Some(wire::ContourPlacement {
                            kind: Some(wire::ContourPlacementKind::Plane(
                                wire::ContourPlacementPlane { z: Some(-2.0) },
                            )),
                        }),
                        line: wire_line_style(wire_auto_color(), 1.5, wire::DashStyle::Dotted),
                    })),
                    wire_artist(wire::ArtistKind::Contour(wire::Contour {
                        id: Some(34),
                        display_name: None,
                        visible: Some(true),
                        grid: wire_rectilinear(C, B),
                        z: Some(A),
                        levels: Some(wire::Levels {
                            kind: Some(wire::LevelsKind::Auto(wire::LevelsAuto { count: Some(5) })),
                        }),
                        fill: Some(false),
                        placement: Some(wire::ContourPlacement {
                            kind: Some(wire::ContourPlacementKind::AtLevel(
                                wire::ContourPlacementAtLevel {},
                            )),
                        }),
                        line: wire_line_style(wire_no_color(), 1.75, wire::DashStyle::Dashed),
                    })),
                    wire_artist(wire::ArtistKind::Quiver(wire::Quiver {
                        id: Some(35),
                        display_name: wire_text("quiver", wire::Interpreter::Latex),
                        visible: Some(true),
                        x: Some(A),
                        y: Some(B),
                        z: Some(C),
                        u: Some(D),
                        v: Some(A),
                        w: Some(B),
                        scale: wire_quiver_scale(wire::QuiverScaleKind::Factor(
                            wire::QuiverScaleFactor { value: Some(0.25) },
                        )),
                        line: wire_line_style(wire_colormapped(), 2.25, wire::DashStyle::None),
                        head_size: Some(0.125),
                    })),
                    wire_artist(wire::ArtistKind::Quiver(wire::Quiver {
                        id: Some(36),
                        display_name: None,
                        visible: Some(true),
                        x: Some(B),
                        y: Some(A),
                        z: None,
                        u: Some(C),
                        v: Some(D),
                        w: None,
                        scale: wire_quiver_scale(wire::QuiverScaleKind::Off(
                            wire::QuiverScaleOff {},
                        )),
                        line: wire_line_style(wire_auto_color(), 0.5, wire::DashStyle::Solid),
                        head_size: Some(0.375),
                    })),
                    wire_artist(wire::ArtistKind::Quiver(wire::Quiver {
                        id: Some(37),
                        display_name: None,
                        visible: Some(true),
                        x: Some(C),
                        y: Some(D),
                        z: None,
                        u: Some(A),
                        v: Some(B),
                        w: None,
                        scale: wire_quiver_scale(wire::QuiverScaleKind::Auto(
                            wire::QuiverScaleAuto {},
                        )),
                        line: wire_line_style(wire_auto_color(), 0.5, wire::DashStyle::Solid),
                        head_size: Some(0.625),
                    })),
                    wire_artist(wire::ArtistKind::Surface(wire::Surface {
                        id: Some(38),
                        display_name: wire_text("surface", wire::Interpreter::None),
                        visible: Some(false),
                        grid: wire_rectilinear(D, A),
                        z: Some(B),
                        c: Some(C),
                        face: wire_rgba(0.25, 0.5, 0.75, 1.0),
                        edge: wire_auto_color(),
                        edge_width_pt: Some(0.625),
                    })),
                    wire_artist(wire::ArtistKind::Surface(wire::Surface {
                        id: Some(39),
                        display_name: None,
                        visible: Some(true),
                        grid: wire_curvilinear(A, D),
                        z: Some(C),
                        c: None,
                        face: wire_no_color(),
                        edge: wire_colormapped(),
                        edge_width_pt: Some(0.875),
                    })),
                ],
            },
            wire::Axes {
                id: Some(21),
                cell: Some(wire::Cell {
                    row: Some(0),
                    col: Some(1),
                    row_span: Some(1),
                    col_span: Some(2),
                }),
                projection: Some(wire::Projection {
                    kind: Some(wire::ProjectionKind::TwoD(wire::ProjectionTwoD {})),
                }),
                title: None,
                x: wire_axis(None, wire::Scale::Linear, wire_auto_limits(), false),
                y: wire_axis(None, wire::Scale::Linear, wire_auto_limits(), false),
                z: wire_axis(None, wire::Scale::Linear, wire_auto_limits(), false),
                r#box: Some(true),
                colormap: wire::ColormapName::Cividis as i32,
                clim: wire_auto_limits(),
                legend: None,
                artists: vec![],
            },
        ],
        links: vec![wire::AxisLink {
            dimension: wire::Dimension::Y as i32,
            axes: vec![21, 20],
        }],
        provenance: Some(wire::Provenance {
            ironlab_version: "9.8.7".to_owned(),
            typesetter: "typesetter 6.5".to_owned(),
            fonts: vec!["Font B".to_owned(), "Font A".to_owned()],
        }),
    };

    // Every field is written out, so that no expected value comes from a default.
    let text = |content: &str, interpreter| {
        Some(Text {
            content: content.to_owned(),
            interpreter,
        })
    };
    let auto_axis = Axis {
        label: None,
        scale: Scale::Linear,
        limits: Limits::Auto,
        grid: false,
    };
    let expected = Figure {
        schema_version: SCHEMA_VERSION.to_owned(),
        id: NodeId(1),
        title: text("figure title", Interpreter::None),
        size: FigureSize {
            width_mm: 101.0,
            height_mm: 102.0,
        },
        font_set: FontSetId::StixTwo,
        font_size_pt: 7.5,
        background: Color::rgba(0.125, 0.25, 0.375, 0.5),
        layout: TileLayout { rows: 2, cols: 3 },
        data: BTreeMap::from([
            (DataId(A), NdArray::vector(vec![1.0, 2.0])),
            (
                DataId(B),
                NdArray {
                    shape: vec![1, 3],
                    values: vec![3.0, 4.0, 5.0],
                },
            ),
            (
                DataId(C),
                NdArray {
                    shape: vec![3, 1],
                    values: vec![6.0, 7.0, 8.0],
                },
            ),
            (
                DataId(D),
                NdArray {
                    shape: vec![2, 2],
                    values: vec![9.0, 10.0, 11.0, 12.0],
                },
            ),
        ]),
        axes: vec![
            Axes {
                id: NodeId(20),
                cell: Cell {
                    row: 1,
                    col: 2,
                    row_span: 3,
                    col_span: 4,
                },
                projection: Projection::ThreeD {
                    view3d: View3d {
                        azimuth_deg: 11.0,
                        elevation_deg: 12.0,
                        zoom: 13.0,
                        pan_x: 14.0,
                        pan_y: 15.0,
                    },
                },
                title: text("axes title", Interpreter::Latex),
                x: Axis {
                    label: text("x label", Interpreter::Latex),
                    scale: Scale::Log,
                    limits: Limits::Manual { min: 1.0, max: 2.0 },
                    grid: true,
                },
                y: Axis {
                    label: text("y label", Interpreter::Latex),
                    scale: Scale::Linear,
                    limits: Limits::Manual { min: 3.0, max: 4.0 },
                    grid: false,
                },
                z: Axis {
                    label: None,
                    scale: Scale::Log,
                    limits: Limits::Auto,
                    grid: true,
                },
                box_: false,
                colormap: ColormapName::Magma,
                clim: Limits::Manual { min: 5.0, max: 6.0 },
                legend: Some(Legend {
                    location: LegendLocation::SouthWest,
                    boxed: false,
                }),
                artists: vec![
                    Artist::Line(Line {
                        id: NodeId(30),
                        display_name: text("line", Interpreter::Latex),
                        visible: false,
                        x: DataId(A),
                        y: DataId(B),
                        z: Some(DataId(C)),
                        line: LineStyle {
                            color: ColorSpec::Rgba {
                                color: Color::rgba(0.5, 0.625, 0.75, 0.875),
                            },
                            width_pt: 1.25,
                            dash: DashStyle::DashDot,
                        },
                        marker: MarkerStyle {
                            shape: MarkerShape::TriangleDown,
                            size_pt: 2.5,
                            face: ColorSpec::None,
                            edge: ColorSpec::Colormapped,
                        },
                    }),
                    Artist::Scatter(Scatter {
                        id: NodeId(31),
                        display_name: text("scatter", Interpreter::None),
                        visible: true,
                        x: DataId(B),
                        y: DataId(C),
                        z: Some(DataId(A)),
                        size: ScatterSize::Data { data: DataId(C) },
                        color: ScatterColor::Data { data: DataId(D) },
                        marker: MarkerStyle {
                            shape: MarkerShape::Square,
                            size_pt: 3.5,
                            face: ColorSpec::Auto,
                            edge: ColorSpec::None,
                        },
                    }),
                    Artist::Scatter(Scatter {
                        id: NodeId(32),
                        display_name: None,
                        visible: true,
                        x: DataId(C),
                        y: DataId(A),
                        z: None,
                        size: ScatterSize::Scalar { value: 4.5 },
                        color: ScatterColor::Spec {
                            spec: ColorSpec::Colormapped,
                        },
                        marker: MarkerStyle {
                            shape: MarkerShape::Plus,
                            size_pt: 5.5,
                            face: ColorSpec::Rgba {
                                color: Color::rgba(0.0625, 0.1875, 0.3125, 0.4375),
                            },
                            edge: ColorSpec::Auto,
                        },
                    }),
                    Artist::Contour(Contour {
                        id: NodeId(33),
                        display_name: text("contour", Interpreter::Latex),
                        visible: false,
                        grid: Grid::Curvilinear {
                            x: DataId(B),
                            y: DataId(C),
                        },
                        z: DataId(D),
                        levels: Levels::Explicit {
                            values: vec![1.0, 2.0, 3.0],
                        },
                        fill: true,
                        placement: ContourPlacement::Plane { z: Some(-2.0) },
                        line: LineStyle {
                            color: ColorSpec::Auto,
                            width_pt: 1.5,
                            dash: DashStyle::Dotted,
                        },
                    }),
                    Artist::Contour(Contour {
                        id: NodeId(34),
                        display_name: None,
                        visible: true,
                        grid: Grid::Rectilinear {
                            x: DataId(C),
                            y: DataId(B),
                        },
                        z: DataId(A),
                        levels: Levels::Auto { count: 5 },
                        fill: false,
                        placement: ContourPlacement::AtLevel,
                        line: LineStyle {
                            color: ColorSpec::None,
                            width_pt: 1.75,
                            dash: DashStyle::Dashed,
                        },
                    }),
                    Artist::Quiver(Quiver {
                        id: NodeId(35),
                        display_name: text("quiver", Interpreter::Latex),
                        visible: true,
                        x: DataId(A),
                        y: DataId(B),
                        z: Some(DataId(C)),
                        u: DataId(D),
                        v: DataId(A),
                        w: Some(DataId(B)),
                        scale: QuiverScale::Factor { value: 0.25 },
                        line: LineStyle {
                            color: ColorSpec::Colormapped,
                            width_pt: 2.25,
                            dash: DashStyle::None,
                        },
                        head_size: 0.125,
                    }),
                    Artist::Quiver(Quiver {
                        id: NodeId(36),
                        display_name: None,
                        visible: true,
                        x: DataId(B),
                        y: DataId(A),
                        z: None,
                        u: DataId(C),
                        v: DataId(D),
                        w: None,
                        scale: QuiverScale::Off,
                        line: LineStyle {
                            color: ColorSpec::Auto,
                            width_pt: 0.5,
                            dash: DashStyle::Solid,
                        },
                        head_size: 0.375,
                    }),
                    Artist::Quiver(Quiver {
                        id: NodeId(37),
                        display_name: None,
                        visible: true,
                        x: DataId(C),
                        y: DataId(D),
                        z: None,
                        u: DataId(A),
                        v: DataId(B),
                        w: None,
                        scale: QuiverScale::Auto,
                        line: LineStyle {
                            color: ColorSpec::Auto,
                            width_pt: 0.5,
                            dash: DashStyle::Solid,
                        },
                        head_size: 0.625,
                    }),
                    Artist::Surface(Surface {
                        id: NodeId(38),
                        display_name: text("surface", Interpreter::None),
                        visible: false,
                        grid: Grid::Rectilinear {
                            x: DataId(D),
                            y: DataId(A),
                        },
                        z: DataId(B),
                        c: Some(DataId(C)),
                        face: ColorSpec::Rgba {
                            color: Color::rgba(0.25, 0.5, 0.75, 1.0),
                        },
                        edge: ColorSpec::Auto,
                        edge_width_pt: 0.625,
                    }),
                    Artist::Surface(Surface {
                        id: NodeId(39),
                        display_name: None,
                        visible: true,
                        grid: Grid::Curvilinear {
                            x: DataId(A),
                            y: DataId(D),
                        },
                        z: DataId(C),
                        c: None,
                        face: ColorSpec::None,
                        edge: ColorSpec::Colormapped,
                        edge_width_pt: 0.875,
                    }),
                ],
            },
            Axes {
                id: NodeId(21),
                cell: Cell {
                    row: 0,
                    col: 1,
                    row_span: 1,
                    col_span: 2,
                },
                projection: Projection::TwoD,
                title: None,
                x: auto_axis.clone(),
                y: auto_axis.clone(),
                z: auto_axis,
                box_: true,
                colormap: ColormapName::Cividis,
                clim: Limits::Auto,
                legend: None,
                artists: vec![],
            },
        ],
        links: vec![AxisLink {
            dimension: Dimension::Y,
            axes: vec![NodeId(21), NodeId(20)],
        }],
        provenance: Provenance {
            ironlab_version: "9.8.7".to_owned(),
            typesetter: "typesetter 6.5".to_owned(),
            fonts: vec!["Font B".to_owned(), "Font A".to_owned()],
        },
        id_allocator: NodeIdAllocator::default(),
    };

    let decoded = Figure::from_protobuf(&encode_wire(&wire_figure)).unwrap();
    assert_eq!(decoded, expected);
    // The float fields are compared bit for bit as well, because `Figure`'s equality
    // cannot tell every exchange of values apart (for example -0.0 and 0.0).
    assert_eq!(float_bits(&decoded), float_bits(&expected));
}

// Why: an enum mapped by position rather than by name (off by one, or in a different
// order from the domain enum) would pass every round trip, yet other-language clients
// would read `LEGEND_LOCATION_NORTH_WEST` where IronLAB means north-east. Every value of
// every wire enum, found by trying every number rather than by listing the values, must
// decode to the domain variant of the same name. The generated schema is also checked
// for enums that this test does not place, so that a new enum cannot be left out.
#[test]
fn each_wire_enum_value_decodes_to_the_domain_variant_of_the_same_name() {
    /// Decodes the carrier figure with every defined value of the enum `E` placed by
    /// `set`, and compares the name of each value with the name that `get` reads from
    /// the decoded figure. Returns the name of the enum.
    fn check<E: TryFrom<i32> + std::fmt::Debug>(
        set: impl Fn(&mut wire::Figure, i32),
        get: impl Fn(&Figure) -> String,
    ) -> String {
        let name = std::any::type_name::<E>()
            .rsplit("::")
            .next()
            .expect("a type name has a last segment")
            .to_owned();
        let mut values = 0;
        for number in 1..=1024 {
            let Ok(value) = E::try_from(number) else {
                continue;
            };
            let mut wire_figure = carrier();
            set(&mut wire_figure, number);
            let decoded = Figure::from_protobuf(&encode_wire(&wire_figure))
                .unwrap_or_else(|e| panic!("{name} value {number} ({value:?}) gave {e:?}"));
            assert_eq!(
                get(&decoded),
                format!("{value:?}"),
                "{name} value {number} decodes to the wrong variant"
            );
            values += 1;
        }
        assert!(values > 0, "{name} has no values other than unspecified");
        name
    }

    /// A wire figure with a place for every enum: a title, a link, and an axes with an
    /// x axis, a legend and a line with a line style and a marker style.
    fn carrier() -> wire::Figure {
        wire::Figure {
            font_set: wire::FontSetId::StixTwo as i32,
            title: wire_text("title", wire::Interpreter::Latex),
            axes: vec![wire::Axes {
                id: Some(2),
                x: wire_axis(None, wire::Scale::Linear, wire_auto_limits(), false),
                colormap: wire::ColormapName::Viridis as i32,
                legend: Some(wire::Legend {
                    location: wire::LegendLocation::NorthEast as i32,
                    boxed: Some(true),
                }),
                artists: vec![wire_artist(wire::ArtistKind::Line(wire::Line {
                    id: Some(3),
                    x: Some(0),
                    y: Some(0),
                    line: wire_line_style(wire_auto_color(), 0.75, wire::DashStyle::Solid),
                    marker: wire_marker(
                        wire::MarkerShape::Circle,
                        4.0,
                        wire_no_color(),
                        wire_auto_color(),
                    ),
                    ..wire::Line::default()
                }))],
                ..wire::Axes::default()
            }],
            links: vec![wire::AxisLink {
                dimension: wire::Dimension::X as i32,
                axes: vec![2],
            }],
            ..minimal_wire_figure()
        }
    }

    fn wire_line(fig: &mut wire::Figure) -> &mut wire::Line {
        match fig.axes[0].artists[0].kind.as_mut() {
            Some(wire::ArtistKind::Line(line)) => line,
            _ => unreachable!("the carrier holds a line"),
        }
    }

    fn line(fig: &Figure) -> &Line {
        match &fig.axes[0].artists[0] {
            Artist::Line(line) => line,
            other => panic!("the carrier's line decoded as {other:?}"),
        }
    }

    let checked: BTreeSet<String> = [
        check::<wire::FontSetId>(|w, v| w.font_set = v, |f| format!("{:?}", f.font_set)),
        check::<wire::Interpreter>(
            |w, v| w.title.as_mut().unwrap().interpreter = v,
            |f| format!("{:?}", f.title.as_ref().unwrap().interpreter),
        ),
        check::<wire::Scale>(
            |w, v| w.axes[0].x.as_mut().unwrap().scale = v,
            |f| format!("{:?}", f.axes[0].x.scale),
        ),
        check::<wire::ColormapName>(
            |w, v| w.axes[0].colormap = v,
            |f| format!("{:?}", f.axes[0].colormap),
        ),
        check::<wire::LegendLocation>(
            |w, v| w.axes[0].legend.as_mut().unwrap().location = v,
            |f| format!("{:?}", f.axes[0].legend.unwrap().location),
        ),
        check::<wire::DashStyle>(
            |w, v| wire_line(w).line.as_mut().unwrap().dash = v,
            |f| format!("{:?}", line(f).line.dash),
        ),
        check::<wire::MarkerShape>(
            |w, v| wire_line(w).marker.as_mut().unwrap().shape = v,
            |f| format!("{:?}", line(f).marker.shape),
        ),
        check::<wire::Dimension>(
            |w, v| w.links[0].dimension = v,
            |f| format!("{:?}", f.links[0].dimension),
        ),
    ]
    .into_iter()
    .collect();

    let declared: BTreeSet<String> = proto_files()
        .iter()
        .flat_map(|(_, text)| text.lines())
        .filter_map(|line| line.strip_prefix("enum "))
        .map(|rest| rest.trim_end_matches(" {").to_owned())
        .collect();
    assert_eq!(
        checked, declared,
        "every enum of the schema must be checked"
    );
}

// Why: a decoder that treats a protobuf zero or `false` as absent, or that applies the
// default of a context over a value that is present, would silently change figures:
// hidden artists would reappear, zero widths would widen, and an explicitly automatic
// contour colour would become colormapped. Every value here is present on the wire and
// differs from the default that its absence would produce.
#[test]
fn present_values_are_kept_when_they_equal_zero_or_the_default_of_another_context() {
    let zero_view = wire::View3d {
        azimuth_deg: Some(0.0),
        elevation_deg: Some(0.0),
        zoom: Some(0.0),
        pan_x: Some(0.0),
        pan_y: Some(0.0),
    };
    let wire_figure = wire::Figure {
        size: Some(wire::FigureSize {
            width_mm: Some(0.0),
            height_mm: Some(0.0),
        }),
        font_size_pt: Some(0.0),
        layout: Some(wire::TileLayout {
            rows: Some(0),
            cols: Some(0),
        }),
        axes: vec![wire::Axes {
            id: Some(2),
            cell: Some(wire::Cell {
                row: Some(0),
                col: Some(0),
                row_span: Some(0),
                col_span: Some(0),
            }),
            projection: Some(wire::Projection {
                kind: Some(wire::ProjectionKind::ThreeD(wire::ProjectionThreeD {
                    view3d: Some(zero_view),
                })),
            }),
            r#box: Some(false),
            legend: Some(wire::Legend {
                location: wire::LegendLocation::NorthEast as i32,
                boxed: Some(false),
            }),
            artists: vec![
                wire_artist(wire::ArtistKind::Line(wire::Line {
                    id: Some(3),
                    visible: Some(false),
                    x: Some(0),
                    y: Some(0),
                    line: wire_line_style(wire_auto_color(), 0.0, wire::DashStyle::Solid),
                    marker: wire_marker(
                        wire::MarkerShape::None,
                        0.0,
                        wire_no_color(),
                        wire_auto_color(),
                    ),
                    ..wire::Line::default()
                })),
                wire_artist(wire::ArtistKind::Scatter(wire::Scatter {
                    id: Some(4),
                    x: Some(0),
                    y: Some(0),
                    size: Some(wire::ScatterSize {
                        kind: Some(wire::ScatterSizeKind::Scalar(wire::ScatterSizeScalar {
                            value: Some(0.0),
                        })),
                    }),
                    // A scatter's marker is a circle by default.
                    marker: wire_marker(
                        wire::MarkerShape::None,
                        0.0,
                        wire_no_color(),
                        wire_auto_color(),
                    ),
                    ..wire::Scatter::default()
                })),
                wire_artist(wire::ArtistKind::Contour(wire::Contour {
                    id: Some(5),
                    grid: wire_rectilinear(0, 0),
                    z: Some(0),
                    levels: Some(wire::Levels {
                        kind: Some(wire::LevelsKind::Auto(wire::LevelsAuto { count: Some(0) })),
                    }),
                    placement: Some(wire::ContourPlacement {
                        kind: Some(wire::ContourPlacementKind::Plane(
                            wire::ContourPlacementPlane { z: Some(0.0) },
                        )),
                    }),
                    // A contour's line is colormapped by default.
                    line: wire_line_style(wire_auto_color(), 0.0, wire::DashStyle::Solid),
                    ..wire::Contour::default()
                })),
                wire_artist(wire::ArtistKind::Quiver(wire::Quiver {
                    id: Some(6),
                    x: Some(0),
                    y: Some(0),
                    u: Some(0),
                    v: Some(0),
                    scale: wire_quiver_scale(wire::QuiverScaleKind::Factor(
                        wire::QuiverScaleFactor { value: Some(0.0) },
                    )),
                    head_size: Some(0.0),
                    ..wire::Quiver::default()
                })),
                wire_artist(wire::ArtistKind::Surface(wire::Surface {
                    id: Some(7),
                    grid: wire_rectilinear(0, 0),
                    z: Some(0),
                    // A surface's faces are colormapped and its edges black by default.
                    face: wire_auto_color(),
                    edge: wire_no_color(),
                    edge_width_pt: Some(0.0),
                    ..wire::Surface::default()
                })),
            ],
            ..wire::Axes::default()
        }],
        ..minimal_wire_figure()
    };

    let zero_line = LineStyle {
        color: ColorSpec::Auto,
        width_pt: 0.0,
        dash: DashStyle::Solid,
    };
    let zero_marker = MarkerStyle {
        shape: MarkerShape::None,
        size_pt: 0.0,
        face: ColorSpec::None,
        edge: ColorSpec::Auto,
    };
    let expected = Figure {
        size: FigureSize {
            width_mm: 0.0,
            height_mm: 0.0,
        },
        font_size_pt: 0.0,
        layout: TileLayout { rows: 0, cols: 0 },
        axes: vec![Axes {
            id: NodeId(2),
            cell: Cell {
                row: 0,
                col: 0,
                row_span: 0,
                col_span: 0,
            },
            projection: Projection::ThreeD {
                view3d: View3d {
                    azimuth_deg: 0.0,
                    elevation_deg: 0.0,
                    zoom: 0.0,
                    pan_x: 0.0,
                    pan_y: 0.0,
                },
            },
            box_: false,
            legend: Some(Legend {
                location: LegendLocation::NorthEast,
                boxed: false,
            }),
            artists: vec![
                Artist::Line(Line {
                    id: NodeId(3),
                    visible: false,
                    line: zero_line,
                    marker: zero_marker,
                    ..Line::default()
                }),
                Artist::Scatter(Scatter {
                    id: NodeId(4),
                    size: ScatterSize::Scalar { value: 0.0 },
                    marker: zero_marker,
                    ..Scatter::default()
                }),
                Artist::Contour(Contour {
                    id: NodeId(5),
                    levels: Levels::Auto { count: 0 },
                    placement: ContourPlacement::Plane { z: Some(0.0) },
                    line: zero_line,
                    ..Contour::default()
                }),
                Artist::Quiver(Quiver {
                    id: NodeId(6),
                    scale: QuiverScale::Factor { value: 0.0 },
                    head_size: 0.0,
                    ..Quiver::default()
                }),
                Artist::Surface(Surface {
                    id: NodeId(7),
                    face: ColorSpec::Auto,
                    edge: ColorSpec::None,
                    edge_width_pt: 0.0,
                    ..Surface::default()
                }),
            ],
            ..Axes::default()
        }],
        provenance: empty_provenance(),
        ..Figure::default()
    };
    assert_eq!(
        Figure::from_protobuf(&encode_wire(&wire_figure)).unwrap(),
        expected
    );
}

// Why: strings, repeated fields and maps have no presence in proto3, so an empty string
// or list cannot be told apart from an absent one on the wire. Such a field must decode
// as empty, not as the default of its context; otherwise a provenance that lists no
// fonts would reload listing the default fonts, and an empty title would reload with
// text.
#[test]
fn empty_strings_and_lists_reload_as_empty_rather_than_as_defaults() {
    let (mut fig, axes, _) = single_line_figure();
    fig.title = Some(Text::new(""));
    fig.provenance = Provenance {
        ironlab_version: String::new(),
        typesetter: String::new(),
        fonts: vec![],
    };
    fig.data.insert(
        DataId(99),
        NdArray {
            shape: vec![0],
            values: vec![],
        },
    );
    fig.links.push(AxisLink {
        dimension: Dimension::Z,
        axes: vec![],
    });
    let contour = Artist::Contour(Contour {
        id: NodeId(50),
        display_name: Some(Text::plain("")),
        levels: Levels::Explicit { values: vec![] },
        ..Contour::default()
    });
    fig.axes
        .iter_mut()
        .find(|a| a.id == axes)
        .expect("fixture axes exists")
        .artists
        .push(contour);

    assert_eq!(Figure::from_protobuf(&fig.to_protobuf()).unwrap(), fig);
}

// ---------------------------------------------------------------------------------
// Absent, unspecified and unknown values
// ---------------------------------------------------------------------------------

// Why: other writers (and future versions that stop writing a field) may omit fields,
// and an absent field must mean the IR's default rather than the protobuf zero value,
// which would otherwise load as an invisible, zero-sized figure. The provenance is the
// exception: an absent provenance must not be replaced by that of the reading build,
// which would falsely claim that this build wrote the file.
#[test]
fn a_message_holding_only_the_version_and_the_id_decodes_to_the_default_figure_without_provenance()
{
    let bytes = encode_wire(&minimal_wire_figure());
    let expected = Figure {
        provenance: empty_provenance(),
        ..Figure::default()
    };
    assert_eq!(Figure::from_protobuf(&bytes).unwrap(), expected);
}

// Why: an `_UNSPECIFIED` enum value, an unset oneof and an empty nested message are how
// other writers express "no preference", and each must decode to the default that the
// IR gives that field in its context: a scatter's marker is a circle and a contour's
// line is colormapped, although a bare marker style has no shape and a bare line style
// is automatic.
#[test]
fn unspecified_enums_and_absent_fields_decode_to_the_defaults_of_their_context() {
    let empty_line_style = || wire::LineStyle::default();
    let artist = |kind| wire::Artist { kind: Some(kind) };
    let wire_figure = wire::Figure {
        font_set: wire::FontSetId::Unspecified as i32,
        axes: vec![wire::Axes {
            id: Some(5),
            x: Some(wire::Axis {
                scale: wire::Scale::Unspecified as i32,
                limits: Some(wire::Limits { kind: None }),
                ..wire::Axis::default()
            }),
            colormap: wire::ColormapName::Unspecified as i32,
            legend: Some(wire::Legend {
                location: wire::LegendLocation::Unspecified as i32,
                boxed: None,
            }),
            artists: vec![
                artist(wire::ArtistKind::Line(wire::Line {
                    id: Some(6),
                    x: Some(0),
                    y: Some(0),
                    line: Some(empty_line_style()),
                    marker: Some(wire::MarkerStyle::default()),
                    ..wire::Line::default()
                })),
                artist(wire::ArtistKind::Scatter(wire::Scatter {
                    id: Some(7),
                    x: Some(0),
                    y: Some(0),
                    size: Some(wire::ScatterSize { kind: None }),
                    marker: Some(wire::MarkerStyle::default()),
                    ..wire::Scatter::default()
                })),
                artist(wire::ArtistKind::Contour(wire::Contour {
                    id: Some(8),
                    grid: wire_rectilinear(0, 0),
                    z: Some(0),
                    levels: Some(wire::Levels {
                        kind: Some(wire::LevelsKind::Auto(wire::LevelsAuto { count: None })),
                    }),
                    line: Some(empty_line_style()),
                    ..wire::Contour::default()
                })),
                artist(wire::ArtistKind::Surface(wire::Surface {
                    id: Some(9),
                    grid: wire_rectilinear(0, 0),
                    z: Some(0),
                    edge: Some(wire::ColorSpec { kind: None }),
                    ..wire::Surface::default()
                })),
            ],
            ..wire::Axes::default()
        }],
        ..minimal_wire_figure()
    };

    let decoded = Figure::from_protobuf(&encode_wire(&wire_figure)).unwrap();

    let expected = Figure {
        axes: vec![Axes {
            id: NodeId(5),
            legend: Some(Legend::default()),
            artists: vec![
                Artist::Line(Line {
                    id: NodeId(6),
                    ..Line::default()
                }),
                Artist::Scatter(Scatter {
                    id: NodeId(7),
                    ..Scatter::default()
                }),
                Artist::Contour(Contour {
                    id: NodeId(8),
                    ..Contour::default()
                }),
                Artist::Surface(Surface {
                    id: NodeId(9),
                    ..Surface::default()
                }),
            ],
            ..Axes::default()
        }],
        provenance: empty_provenance(),
        ..Figure::default()
    };
    assert_eq!(decoded, expected);
}

// Why: some values have no default in the IR (what kind of artist to draw, which
// dimension a link applies to, the bounds of manual limits), so their absence must be
// reported rather than guessed. Identifiers of nodes and references to data arrays are
// the most dangerous case: zero is a valid identifier, so a decoder that substituted it
// would silently draw an artist from the wrong array or give two nodes the same
// identifier. The error must name the field, so that the author of a faulty writer can
// find it.
#[test]
fn absent_values_without_a_default_are_errors_that_name_the_field() {
    /// A figure holding one artist of every kind, with every reference present, so
    /// that each case removes exactly one value.
    fn complete() -> wire::Figure {
        let artists = vec![
            wire_artist(wire::ArtistKind::Line(wire::Line {
                id: Some(3),
                x: Some(0),
                y: Some(1),
                ..wire::Line::default()
            })),
            wire_artist(wire::ArtistKind::Scatter(wire::Scatter {
                id: Some(4),
                x: Some(0),
                y: Some(1),
                size: Some(wire::ScatterSize {
                    kind: Some(wire::ScatterSizeKind::Data(wire::ScatterSizeData {
                        data: Some(2),
                    })),
                }),
                color: Some(wire::ScatterColor {
                    kind: Some(wire::ScatterColorKind::Data(wire::ScatterColorData {
                        data: Some(2),
                    })),
                }),
                ..wire::Scatter::default()
            })),
            wire_artist(wire::ArtistKind::Contour(wire::Contour {
                id: Some(5),
                grid: wire_rectilinear(0, 1),
                z: Some(2),
                ..wire::Contour::default()
            })),
            wire_artist(wire::ArtistKind::Quiver(wire::Quiver {
                id: Some(6),
                x: Some(0),
                y: Some(1),
                u: Some(2),
                v: Some(2),
                scale: wire_quiver_scale(wire::QuiverScaleKind::Factor(wire::QuiverScaleFactor {
                    value: Some(2.0),
                })),
                ..wire::Quiver::default()
            })),
            wire_artist(wire::ArtistKind::Surface(wire::Surface {
                id: Some(7),
                grid: wire_curvilinear(0, 1),
                z: Some(2),
                ..wire::Surface::default()
            })),
        ];
        wire::Figure {
            axes: vec![wire::Axes {
                id: Some(2),
                clim: wire_manual(0.0, 1.0),
                artists,
                ..wire::Axes::default()
            }],
            links: vec![wire::AxisLink {
                dimension: wire::Dimension::X as i32,
                axes: vec![2],
            }],
            ..minimal_wire_figure()
        }
    }

    /// Returns the wire artist at the given index of the only axes.
    fn kind(fig: &mut wire::Figure, index: usize) -> &mut wire::ArtistKind {
        fig.axes[0].artists[index]
            .kind
            .as_mut()
            .expect("every artist of the complete figure has a kind")
    }

    macro_rules! artist {
        ($fig:ident, $index:literal, $variant:ident) => {
            match kind($fig, $index) {
                wire::ArtistKind::$variant(artist) => artist,
                _ => unreachable!("the complete figure's artist has another kind"),
            }
        };
    }

    Figure::from_protobuf(&encode_wire(&complete()))
        .expect("the complete figure decodes, so each case fails only for its removal");

    type Removal = fn(&mut wire::Figure);
    let cases: [(&str, Removal); 21] = [
        ("id", |f| f.id = None),
        ("axes[0].id", |f| f.axes[0].id = None),
        ("axes[0].artists[0].kind", |f| {
            f.axes[0].artists[0].kind = None
        }),
        ("axes[0].artists[0].line.id", |f| {
            artist!(f, 0, Line).id = None
        }),
        ("axes[0].artists[0].line.x", |f| {
            artist!(f, 0, Line).x = None
        }),
        ("axes[0].artists[0].line.y", |f| {
            artist!(f, 0, Line).y = None
        }),
        ("axes[0].artists[1].scatter.x", |f| {
            artist!(f, 1, Scatter).x = None
        }),
        ("axes[0].artists[1].scatter.size.data.data", |f| {
            artist!(f, 1, Scatter).size = Some(wire::ScatterSize {
                kind: Some(wire::ScatterSizeKind::Data(wire::ScatterSizeData {
                    data: None,
                })),
            });
        }),
        ("axes[0].artists[1].scatter.color.data.data", |f| {
            artist!(f, 1, Scatter).color = Some(wire::ScatterColor {
                kind: Some(wire::ScatterColorKind::Data(wire::ScatterColorData {
                    data: None,
                })),
            });
        }),
        ("axes[0].artists[2].contour.grid", |f| {
            artist!(f, 2, Contour).grid = None
        }),
        ("axes[0].artists[2].contour.grid.kind", |f| {
            artist!(f, 2, Contour).grid = Some(wire::Grid { kind: None });
        }),
        ("axes[0].artists[2].contour.grid.rectilinear.y", |f| {
            artist!(f, 2, Contour).grid = Some(wire::Grid {
                kind: Some(wire::GridKind::Rectilinear(wire::GridRectilinear {
                    x: Some(0),
                    y: None,
                })),
            });
        }),
        ("axes[0].artists[2].contour.z", |f| {
            artist!(f, 2, Contour).z = None
        }),
        ("axes[0].artists[3].quiver.u", |f| {
            artist!(f, 3, Quiver).u = None
        }),
        ("axes[0].artists[3].quiver.v", |f| {
            artist!(f, 3, Quiver).v = None
        }),
        ("axes[0].artists[3].quiver.scale.factor.value", |f| {
            artist!(f, 3, Quiver).scale =
                wire_quiver_scale(wire::QuiverScaleKind::Factor(wire::QuiverScaleFactor {
                    value: None,
                }));
        }),
        ("axes[0].artists[4].surface.grid.curvilinear.x", |f| {
            artist!(f, 4, Surface).grid = Some(wire::Grid {
                kind: Some(wire::GridKind::Curvilinear(wire::GridCurvilinear {
                    x: None,
                    y: Some(1),
                })),
            });
        }),
        ("axes[0].artists[4].surface.z", |f| {
            artist!(f, 4, Surface).z = None
        }),
        ("axes[0].clim.manual.min", |f| {
            f.axes[0].clim = Some(wire::Limits {
                kind: Some(wire::LimitsKind::Manual(wire::LimitsManual {
                    min: None,
                    max: Some(1.0),
                })),
            });
        }),
        ("axes[0].clim.manual.max", |f| {
            f.axes[0].clim = Some(wire::Limits {
                kind: Some(wire::LimitsKind::Manual(wire::LimitsManual {
                    min: Some(0.0),
                    max: None,
                })),
            });
        }),
        ("links[0].dimension", |f| {
            f.links[0].dimension = wire::Dimension::Unspecified as i32;
        }),
    ];
    for (field, remove) in cases {
        let mut fig = complete();
        remove(&mut fig);
        let result = Figure::from_protobuf(&encode_wire(&fig));
        assert!(
            matches!(
                &result,
                Err(IrError::Protobuf(ProtobufError::MissingField { field: found })) if found == field
            ),
            "removing {field} gave {result:?}"
        );
    }
}

// Why: the references that the IR itself makes optional (the z data of a line, scatter
// or quiver, the w data of a quiver, the colour data of a surface and the height of a
// contour plane) must not become errors or defaults when absent, because a figure
// without them is complete.
#[test]
fn absent_optional_references_decode_as_absent() {
    let (mut fig, axes, _) = single_line_figure();
    let artists = &mut fig
        .axes
        .iter_mut()
        .find(|a| a.id == axes)
        .expect("fixture axes exists")
        .artists;
    artists.push(Artist::Scatter(Scatter {
        id: NodeId(60),
        z: None,
        ..Scatter::default()
    }));
    artists.push(Artist::Quiver(Quiver {
        id: NodeId(61),
        z: None,
        w: None,
        ..Quiver::default()
    }));
    artists.push(Artist::Surface(Surface {
        id: NodeId(62),
        c: None,
        ..Surface::default()
    }));
    artists.push(Artist::Contour(Contour {
        id: NodeId(63),
        placement: ContourPlacement::Plane { z: None },
        ..Contour::default()
    }));

    let wire_figure = wire::Figure::from(&fig);
    for artist in &wire_figure.axes[0].artists {
        let absent = match artist.kind.as_ref().expect("the encoder writes every kind") {
            wire::ArtistKind::Line(line) => line.z.is_none(),
            wire::ArtistKind::Scatter(scatter) => scatter.z.is_none(),
            wire::ArtistKind::Quiver(quiver) => quiver.z.is_none() && quiver.w.is_none(),
            wire::ArtistKind::Surface(surface) => surface.c.is_none(),
            wire::ArtistKind::Contour(contour) => matches!(
                contour.placement.as_ref().and_then(|p| p.kind.as_ref()),
                Some(wire::ContourPlacementKind::Plane(plane)) if plane.z.is_none()
            ),
        };
        assert!(absent, "the encoder wrote an absent reference: {artist:?}");
    }
    assert_eq!(
        Figure::from_protobuf(&encode_wire(&wire_figure)).unwrap(),
        fig
    );
}

// Why: new enum values are introduced only with a new minor schema version, which is
// rejected before decoding, so an unknown value in a compatible file indicates
// corruption or a faulty writer; silently substituting a default would hide it and
// change the figure.
#[test]
fn unknown_enum_values_are_errors() {
    let (fig, _, _) = single_line_figure();
    let mut wire_figure = wire::Figure::from(&fig);
    wire_figure.axes[0]
        .x
        .as_mut()
        .expect("the encoder writes every axis")
        .scale = 99;
    let result = Figure::from_protobuf(&encode_wire(&wire_figure));
    assert!(
        matches!(
            result,
            Err(IrError::Protobuf(ProtobufError::UnknownEnumValue {
                value: 99,
                ..
            }))
        ),
        "{result:?}"
    );
}

// ---------------------------------------------------------------------------------
// Stability of field numbers
// ---------------------------------------------------------------------------------

/// A figure encoded by hand, field by field, from the field numbers of schema 0.1: a 3D
/// axes with a view, labelled axes with automatic and manual limits, a legend, a line
/// with an RGBA colour and a marker, two data arrays (one holding NaN), a link and
/// provenance.
///
/// The bytes are built without the wire types, so that renumbering a field in the
/// `proto_file!` declarations changes what the decoder expects but not these bytes.
fn hand_encoded_bytes() -> Vec<u8> {
    let text = |content: &str, interpreter: u64| {
        [
            length_delimited(1, content.as_bytes()),
            varint_field(2, interpreter),
        ]
        .concat()
    };
    let colour = |r: f32, g: f32, b: f32, a: f32| {
        [
            float_field(1, r),
            float_field(2, g),
            float_field(3, b),
            float_field(4, a),
        ]
        .concat()
    };
    let empty = |number| length_delimited(number, &[]);
    let array = |shape: &[u64], values: &[f64]| {
        [packed_varints(1, shape), packed_doubles(2, values)].concat()
    };
    let limits_manual = |min: f64, max: f64| {
        length_delimited(2, &[double_field(1, min), double_field(2, max)].concat())
    };
    let axis = |label: Option<Vec<u8>>, scale: u64, limits: Vec<u8>, grid: bool| {
        [
            label.map_or(vec![], |l| length_delimited(1, &l)),
            varint_field(2, scale),
            length_delimited(3, &limits),
            varint_field(4, u64::from(grid)),
        ]
        .concat()
    };

    let line = [
        varint_field(1, 3),
        length_delimited(2, &text("decay, $5", 2)),
        varint_field(3, 1),
        varint_field(4, 0),
        varint_field(5, 7),
        // LineStyle: colour rgba (ColorSpec field 2, ColorSpecRgba field 1), width, dash.
        length_delimited(
            7,
            &[
                length_delimited(
                    1,
                    &length_delimited(2, &length_delimited(1, &colour(0.0, 0.5, 0.25, 0.75))),
                ),
                double_field(2, 1.5),
                varint_field(3, 4),
            ]
            .concat(),
        ),
        // MarkerStyle: triangle up, size, face none (field 3), edge colormapped (field 4).
        length_delimited(
            8,
            &[
                varint_field(1, 5),
                double_field(2, 6.0),
                length_delimited(3, &empty(3)),
                length_delimited(4, &empty(4)),
            ]
            .concat(),
        ),
    ]
    .concat();

    let axes = [
        varint_field(1, 2),
        length_delimited(
            2,
            &[
                varint_field(1, 1),
                varint_field(2, 2),
                varint_field(3, 1),
                varint_field(4, 3),
            ]
            .concat(),
        ),
        // Projection three_d (field 2) with its View3d (field 1).
        length_delimited(
            3,
            &length_delimited(
                2,
                &length_delimited(
                    1,
                    &[
                        double_field(1, -37.5),
                        double_field(2, 30.0),
                        double_field(3, 1.25),
                        double_field(4, 0.125),
                        double_field(5, -0.25),
                    ]
                    .concat(),
                ),
            ),
        ),
        length_delimited(4, &text("Axes", 1)),
        length_delimited(5, &axis(Some(text("$t$ (s)", 1)), 1, empty(1), false)),
        length_delimited(6, &axis(None, 2, limits_manual(0.1, 1.0), true)),
        length_delimited(
            7,
            &axis(Some(text("z", 2)), 1, limits_manual(-1.0, 2.0), false),
        ),
        varint_field(8, 0),
        varint_field(9, 6),
        length_delimited(10, &limits_manual(-3.0, 4.0)),
        length_delimited(11, &[varint_field(1, 9), varint_field(2, 0)].concat()),
        length_delimited(12, &length_delimited(1, &line)),
    ]
    .concat();

    let data_entry = |key: u64, value: Vec<u8>| {
        length_delimited(
            9,
            &[varint_field(1, key), length_delimited(2, &value)].concat(),
        )
    };

    [
        length_delimited(1, SCHEMA_VERSION.as_bytes()),
        varint_field(2, 1),
        length_delimited(3, &text("Decay of $e^{-t}$", 1)),
        length_delimited(4, &[double_field(1, 120.0), double_field(2, 80.0)].concat()),
        varint_field(5, 1),
        double_field(6, 9.5),
        length_delimited(7, &colour(1.0, 1.0, 0.5, 1.0)),
        length_delimited(8, &[varint_field(1, 2), varint_field(2, 3)].concat()),
        data_entry(0, array(&[3], &[0.0, 1.0, 2.0])),
        data_entry(7, array(&[3], &[1.0, f64::NAN, 0.135])),
        length_delimited(10, &axes),
        length_delimited(11, &[varint_field(1, 3), packed_varints(2, &[2])].concat()),
        length_delimited(
            12,
            &[
                length_delimited(1, b"0.1.0"),
                length_delimited(2, b"latex-rust 1.0.2"),
                length_delimited(3, b"STIX Two Text"),
                length_delimited(3, b"STIX Two Math"),
            ]
            .concat(),
        ),
    ]
    .concat()
}

// Why: field numbers are the compatibility contract of `.fig` files and of clients in
// other languages; a file written today must decode to the same figure in every later
// build of the same minor schema version. These bytes are written from the numbers of
// the schema rather than through the wire types, so renumbering a field, an enum value
// or a oneof variant in the declarations makes this test fail. (`buf breaking` in CI
// checks every number of the schema; this test also runs where buf is not installed.)
#[test]
fn bytes_encoded_by_hand_from_the_schema_field_numbers_decode_to_the_described_figure() {
    let expected = Figure {
        schema_version: SCHEMA_VERSION.to_owned(),
        id: NodeId(1),
        title: Some(Text::new("Decay of $e^{-t}$")),
        size: FigureSize {
            width_mm: 120.0,
            height_mm: 80.0,
        },
        font_set: FontSetId::StixTwo,
        font_size_pt: 9.5,
        background: Color::rgba(1.0, 1.0, 0.5, 1.0),
        layout: TileLayout { rows: 2, cols: 3 },
        data: BTreeMap::from([
            (DataId(0), NdArray::vector(vec![0.0, 1.0, 2.0])),
            (DataId(7), NdArray::vector(vec![1.0, f64::NAN, 0.135])),
        ]),
        axes: vec![Axes {
            id: NodeId(2),
            cell: Cell {
                row: 1,
                col: 2,
                row_span: 1,
                col_span: 3,
            },
            projection: Projection::ThreeD {
                view3d: View3d {
                    azimuth_deg: -37.5,
                    elevation_deg: 30.0,
                    zoom: 1.25,
                    pan_x: 0.125,
                    pan_y: -0.25,
                },
            },
            title: Some(Text::new("Axes")),
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
            z: Axis {
                label: Some(Text::plain("z")),
                scale: Scale::Linear,
                limits: Limits::Manual {
                    min: -1.0,
                    max: 2.0,
                },
                grid: false,
            },
            box_: false,
            colormap: ColormapName::Coolwarm,
            clim: Limits::Manual {
                min: -3.0,
                max: 4.0,
            },
            legend: Some(Legend {
                location: LegendLocation::Best,
                boxed: false,
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
                        color: Color::rgba(0.0, 0.5, 0.25, 0.75),
                    },
                    width_pt: 1.5,
                    dash: DashStyle::DashDot,
                },
                marker: MarkerStyle {
                    shape: MarkerShape::TriangleUp,
                    size_pt: 6.0,
                    face: ColorSpec::None,
                    edge: ColorSpec::Colormapped,
                },
            })],
        }],
        links: vec![AxisLink {
            dimension: Dimension::Z,
            axes: vec![NodeId(2)],
        }],
        provenance: Provenance {
            ironlab_version: "0.1.0".to_owned(),
            typesetter: "latex-rust 1.0.2".to_owned(),
            fonts: vec!["STIX Two Text".to_owned(), "STIX Two Math".to_owned()],
        },
        id_allocator: NodeIdAllocator::default(),
    };

    let decoded = Figure::from_protobuf(&hand_encoded_bytes()).unwrap();
    assert_eq!(decoded, expected);
    assert_eq!(float_bits(&decoded), float_bits(&expected));
}

// ---------------------------------------------------------------------------------
// Forward compatibility and versions
// ---------------------------------------------------------------------------------

// Why: a file written by a later patch release may contain fields that this build does
// not know, at any depth; they must be skipped rather than rejected, so that patch
// releases stay compatible as the version rules promise.
#[test]
fn unknown_fields_are_skipped_at_every_depth() {
    let (fig, _, _) = single_line_figure();
    let mut wire_figure = wire::Figure::from(&fig);
    let mut wire_axes = wire_figure.axes.remove(0);
    let wire_artist = wire_axes.artists.remove(0);
    let Some(wire::ArtistKind::Line(wire_line)) = wire_artist.kind else {
        panic!("fixture holds a line");
    };

    // Rebuild the nesting figure → axes (field 10) → artist (field 12) → line (field 1),
    // appending unknown fields at every level. Field order within a message does not
    // affect decoding.
    let line = [wire_line.encode_to_vec(), unknown_fields()].concat();
    let artist = [length_delimited(1, &line), unknown_fields()].concat();
    let axes = [
        wire_axes.encode_to_vec(),
        length_delimited(12, &artist),
        unknown_fields(),
    ]
    .concat();
    let bytes = [
        encode_wire(&wire_figure),
        length_delimited(10, &axes),
        unknown_fields(),
    ]
    .concat();

    assert_eq!(Figure::from_protobuf(&bytes).unwrap(), fig);
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

// Why: files from an incompatible schema (a different major or minor version) or with a
// version that cannot be compared must be rejected with a version error rather than
// misread.
#[test]
fn incompatible_or_malformed_schema_versions_are_rejected() {
    let [major, minor, patch] = supported_version();
    let mut versions = vec![
        format!("{major}.{}.0", minor + 1),
        format!("{}.{minor}.{patch}", major + 1),
        format!("{}.0.0", major + 1),
        format!("{major}.{minor}"),
        format!("{major}.{minor}.{patch}.0"),
        format!("v{major}.{minor}.{patch}"),
        String::new(),
        "zero.one.zero".to_owned(),
    ];
    if minor > 0 {
        versions.push(format!("{major}.{}.9", minor - 1));
    }
    if major > 0 {
        versions.push(format!("{}.{minor}.{patch}", major - 1));
    }
    for version in versions {
        let fig = Figure {
            schema_version: version.clone(),
            ..kitchen_sink_figure()
        };
        let result = Figure::from_protobuf(&fig.to_protobuf());
        assert!(
            matches!(result, Err(IrError::IncompatibleSchemaVersion { ref found, .. }) if *found == version),
            "version {version:?} gave {result:?}"
        );
    }
}

// Why: a newer minor version may change the type of an existing field, so the version
// (always field 1) must be checked before the rest of the message is decoded, and the
// user told that the file is too new rather than corrupt.
#[test]
fn newer_minor_version_with_incompatible_structure_reports_the_version() {
    let [major, minor, _] = supported_version();
    let newer = format!("{major}.{}.0", minor + 8);
    let bytes = [
        length_delimited(1, newer.as_bytes()),
        // Field 2 (the figure identifier, a varint in 0.1) written as a string.
        length_delimited(2, b"abc"),
        // Field 10 (the axes, messages in 0.1) written as a 64-bit value.
        field_key(10, 1),
        7.0f64.to_le_bytes().to_vec(),
    ]
    .concat();
    let result = Figure::from_protobuf(&bytes);
    assert!(
        matches!(result, Err(IrError::IncompatibleSchemaVersion { ref found, .. }) if *found == newer),
        "{result:?}"
    );
}

// Why: patch releases of the schema are compatible by definition, so they must load, and
// the declared version must be kept.
#[test]
fn different_patch_version_is_accepted() {
    let [major, minor, patch] = supported_version();
    let fig = Figure {
        schema_version: format!("{major}.{minor}.{}", patch + 42),
        ..kitchen_sink_figure()
    };
    assert_eq!(Figure::from_protobuf(&fig.to_protobuf()).unwrap(), fig);
}

// Why: a file cut short (by an interrupted write or transfer) must produce an error when
// the cut falls inside a field. Protobuf cannot detect a cut that falls exactly between
// top-level fields, so every other cut must at least not panic.
#[test]
fn truncated_bytes_are_a_protobuf_error_and_never_panic() {
    let bytes = kitchen_sink_figure().to_protobuf();
    let result = Figure::from_protobuf(&bytes[..bytes.len() - 1]);
    assert!(
        matches!(result, Err(IrError::Protobuf(ProtobufError::Decode(_)))),
        "cut inside the last field gave {result:?}"
    );
    for cut in 0..bytes.len() {
        let _ = Figure::from_protobuf(&bytes[..cut]);
    }
}

// Why: bytes that are not a figure at all (such as a JSON file given the wrong
// extension) must produce an error, not a panic or an empty figure.
#[test]
fn bytes_that_are_not_protobuf_are_rejected() {
    let json = kitchen_sink_figure().to_json();
    assert!(Figure::from_protobuf(json.as_bytes()).is_err());
}

// ---------------------------------------------------------------------------------
// Size
// ---------------------------------------------------------------------------------

// Why: the binary format exists so that large data is compact; each value must cost its
// eight IEEE 754 bytes in a packed array rather than a key per value (which would cost
// nine bytes) or a decimal representation, so a figure dominated by data stays within
// 8.1 bytes per value including the figure's structure.
#[test]
fn large_arrays_encode_at_eight_bytes_per_value() {
    const POINTS: usize = 100_000;
    let mut fig = Figure::new();
    let x: Vec<f64> = (0..POINTS).map(|i| i as f64 * 1.0e-3).collect();
    let y: Vec<f64> = x.iter().map(|t| (t * 7.3).sin()).collect();
    let x = fig.add_data(NdArray::vector(x));
    let y = fig.add_data(NdArray::vector(y));
    let axes = fig.alloc_node_id();
    let line = fig.alloc_node_id();
    fig.axes.push(Axes {
        id: axes,
        artists: vec![Artist::Line(Line {
            id: line,
            x,
            y,
            ..Line::default()
        })],
        ..Axes::default()
    });

    let bytes = fig.to_protobuf();
    let per_value = bytes.len() as f64 / (2 * POINTS) as f64;
    assert!(
        per_value <= 8.1,
        "{} bytes for {} values is {per_value:.3} bytes per value",
        bytes.len(),
        2 * POINTS
    );
}
