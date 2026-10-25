//! Persistence of figures as Protocol Buffers (`.fig`): lossless round trips, agreement
//! with the JSON format, defaults for absent values, forward compatibility, schema
//! version checks and the size of encoded data.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{
    SPECIAL_F64, float_bits, floats_mut, image_variants_figure, kitchen_sink_figure, legend_panel,
    single_line_figure, special_values_figure,
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
        typesetter_version: String::new(),
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
        reverse: Some(false),
        ticks: Some(wire::Ticks {
            kind: Some(wire::TicksKind::Auto(wire::TicksAuto {})),
        }),
        tick_labels: Some(wire::TickLabels {
            kind: Some(wire::TickLabelsKind::Auto(wire::TickLabelsAuto {})),
        }),
        visible: Some(true),
        tick_label_angle: Some(0.0),
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

fn wire_range(first: f64, last: f64) -> Option<wire::PixelRange> {
    Some(wire::PixelRange {
        first: Some(first),
        last: Some(last),
    })
}

fn wire_plane(kind: wire::ImagePlaneKind) -> Option<wire::ImagePlane> {
    Some(wire::ImagePlane { kind: Some(kind) })
}

fn wire_xy(z: Option<f64>) -> Option<wire::ImagePlane> {
    wire_plane(wire::ImagePlaneKind::Xy(wire::ImagePlaneXy { z }))
}

fn wire_xz(y: Option<f64>) -> Option<wire::ImagePlane> {
    wire_plane(wire::ImagePlaneKind::Xz(wire::ImagePlaneXz { y }))
}

fn wire_yz(x: Option<f64>) -> Option<wire::ImagePlane> {
    wire_plane(wire::ImagePlaneKind::Yz(wire::ImagePlaneYz { x }))
}

fn wire_placement(
    plane: Option<wire::ImagePlane>,
    columns: Option<wire::PixelRange>,
    rows: Option<wire::PixelRange>,
) -> Option<wire::ImagePlacement> {
    Some(wire::ImagePlacement {
        plane,
        columns,
        rows,
    })
}

fn wire_policy(kind: wire::OutOfRangeKind) -> Option<wire::OutOfRange> {
    Some(wire::OutOfRange { kind: Some(kind) })
}

fn wire_strict() -> Option<wire::OutOfRange> {
    wire_policy(wire::OutOfRangeKind::Strict(wire::OutOfRangeStrict {}))
}

fn wire_transparent() -> Option<wire::OutOfRange> {
    wire_policy(wire::OutOfRangeKind::Transparent(
        wire::OutOfRangeTransparent {},
    ))
}

fn wire_clamp() -> Option<wire::OutOfRange> {
    wire_policy(wire::OutOfRangeKind::Clamp(wire::OutOfRangeClamp {}))
}

fn wire_painted(r: f32, g: f32, b: f32, a: f32) -> Option<wire::OutOfRange> {
    wire_policy(wire::OutOfRangeKind::Rgba(wire::OutOfRangeRgba {
        color: Some(wire_color(r, g, b, a)),
    }))
}

/// Returns the plane variant of a wire placement, if the placement and its plane are
/// present and the plane's variant is set.
fn plane_kind(placement: Option<&wire::ImagePlacement>) -> Option<&wire::ImagePlaneKind> {
    placement?.plane.as_ref()?.kind.as_ref()
}

/// A wire array of floats, with its element written explicitly as the encoder writes it.
fn wire_floats(shape: Vec<u64>, values: Vec<f64>) -> wire::NdArray {
    wire::NdArray {
        shape,
        values,
        element: wire::NdArrayElement::F64 as i32,
        u8_values: vec![],
    }
}

/// A wire array of 8-bit values, held in the byte payload.
fn wire_bytes(shape: Vec<u64>, values: Vec<u8>) -> wire::NdArray {
    wire::NdArray {
        shape,
        values: vec![],
        element: wire::NdArrayElement::U8 as i32,
        u8_values: values,
    }
}

/// A wire data array with the given identifier, optional name and array.
fn wire_data(id: u64, name: Option<&str>, array: wire::NdArray) -> wire::DataArray {
    wire::DataArray {
        id: Some(id),
        array: Some(array),
        name: name.map(str::to_owned),
    }
}

/// The wire content of a panel that holds the given legend.
fn wire_panel_legend(legend: wire::PanelLegend) -> Option<wire::PanelContent> {
    Some(wire::PanelContent {
        kind: Some(wire::PanelContentKind::Legend(wire::PanelContentLegend {
            legend: Some(legend),
        })),
    })
}

/// A wire legend inside the plot area.
fn wire_inside(
    location: wire::LegendLocation,
    boxed: Option<bool>,
    layout: Option<wire::LegendEntryLayout>,
) -> Option<wire::Legend> {
    Some(wire::Legend {
        kind: Some(wire::LegendKind::Inside(wire::LegendInside {
            inside: Some(wire::InsideLegend {
                location: location as i32,
                boxed,
                layout,
                entries: None,
            }),
        })),
    })
}

/// A wire legend that lists its entries in the legend panel of the given identifier.
fn wire_in_panel(panel: u64) -> Option<wire::Legend> {
    Some(wire::Legend {
        kind: Some(wire::LegendKind::Panel(wire::LegendPanel {
            panel: Some(panel),
        })),
    })
}

/// A wire layout of legend entries in a fixed number of columns.
fn wire_columns(count: u32, order: wire::LegendEntryOrder) -> Option<wire::LegendEntryLayout> {
    Some(wire::LegendEntryLayout {
        columns: Some(wire::LegendEntryColumns {
            kind: Some(wire::LegendEntryColumnsKind::Count(
                wire::LegendEntryColumnsCount { count: Some(count) },
            )),
        }),
        order: order as i32,
    })
}

/// A wire size of a row or column of the tile layout that is a weight.
fn wire_weight(weight: f64) -> wire::TrackSize {
    wire::TrackSize {
        kind: Some(wire::TrackSizeKind::Weight(wire::TrackSizeWeight {
            weight: Some(weight),
        })),
    }
}

/// A wire size of a row or column of the tile layout that is fitted to its legend panels.
fn wire_fit() -> wire::TrackSize {
    wire::TrackSize {
        kind: Some(wire::TrackSizeKind::Fit(wire::TrackSizeFit {})),
    }
}

/// Wire legend entries that list exactly the given items.
fn wire_explicit(items: Vec<wire::LegendItem>) -> Option<wire::LegendEntries> {
    Some(wire::LegendEntries {
        kind: Some(wire::LegendEntriesKind::Explicit(
            wire::LegendEntriesExplicit { items },
        )),
    })
}

/// A wire legend item with the given content.
fn wire_item(kind: wire::LegendItemContentKind, full_row: Option<bool>) -> wire::LegendItem {
    wire::LegendItem {
        content: Some(wire::LegendItemContent { kind: Some(kind) }),
        full_row,
    }
}

/// A wire legend item that lists the artist with the given identifier.
fn wire_artist_item(artist: u64, full_row: Option<bool>) -> wire::LegendItem {
    wire_item(
        wire::LegendItemContentKind::Artist(wire::LegendItemContentArtist {
            artist: Some(artist),
        }),
        full_row,
    )
}

/// A wire legend item that shows a label beside a sample and stands for no artist.
fn wire_custom_item(
    label: Option<wire::Text>,
    sample: wire::LegendSampleKind,
    full_row: Option<bool>,
) -> wire::LegendItem {
    wire_item(
        wire::LegendItemContentKind::Custom(wire::LegendItemContentCustom {
            label,
            sample: Some(wire::LegendSample { kind: Some(sample) }),
        }),
        full_row,
    )
}

/// The inside legend of a wire legend, which must be one.
fn inside_mut(legend: &mut Option<wire::Legend>) -> &mut wire::InsideLegend {
    match legend.as_mut().and_then(|legend| legend.kind.as_mut()) {
        Some(wire::LegendKind::Inside(inside)) => inside
            .inside
            .as_mut()
            .expect("the inside legend is present"),
        _ => unreachable!("the legend is inside the plot area"),
    }
}

/// The inside legend of a domain legend, which must be one.
fn inside(legend: &Option<Legend>) -> &InsideLegend {
    match legend {
        Some(Legend::Inside(inside)) => inside,
        other => panic!("the legend decoded as {other:?}"),
    }
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
        floats_mut(&mut fig.data[0].array)[1] = f64::from_bits(sample);

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
    fig.data[0].id = NodeId(u64::MAX);
    let Artist::Series(line) = &mut fig.axes[0].artists[0] else {
        panic!("fixture holds a series");
    };
    line.x = NodeId(u64::MAX);
    assert_eq!(Figure::from_protobuf(&fig.to_protobuf()).unwrap(), fig);
}

// Why: an image's pixels are stored as bytes, and every one of the 256 byte values, in
// an array of more than one dimension, must reload as itself with its element type;
// figure equality distinguishes the element types, so it checks that the array did not
// come back as floats.
#[test]
fn every_byte_value_reloads_with_its_element_type() {
    let (mut fig, _, _) = single_line_figure();
    let values: Vec<u8> = (0..=255).collect();
    let array = NdArray::from_shape_u8(vec![16, 16], values.clone()).unwrap();
    fig.data.push(DataArray {
        id: NodeId(9),
        name: None,
        array: array.clone(),
    });
    let restored = Figure::from_protobuf(&fig.to_protobuf()).unwrap();
    assert_eq!(restored.data[2].array.as_u8(), Some(values.as_slice()));
    assert_eq!(restored.data[2].array, array);
    assert_eq!(restored, fig);
}

// Why: readers in other languages take an array's values from the payload that its
// element names, so the encoder must write the element of every array explicitly
// (floats included, whose element is not the protobuf zero value, as
// `the_encoder_writes_every_value_explicitly` also requires) and put the values in the
// payload of that element only, leaving the other payload empty so that no reader finds
// two candidate payloads.
#[test]
fn the_encoder_writes_the_element_of_every_array_and_one_payload() {
    let (mut fig, _, _) = single_line_figure();
    fig.data.push(DataArray {
        id: NodeId(9),
        name: None,
        array: NdArray::from_shape_u8(vec![2, 2], vec![0, 1, 254, 255]).unwrap(),
    });
    let wire_figure = wire::Figure::from(&fig);

    let floats = wire_figure.data[0]
        .array
        .as_ref()
        .expect("the array is written");
    assert_eq!(floats.element, wire::NdArrayElement::F64 as i32);
    assert_eq!(floats.shape, [3]);
    assert_eq!(floats.values, [1.0, 2.0, 3.0]);
    assert!(floats.u8_values.is_empty());

    assert_eq!(wire_figure.data[2].id, Some(9));
    let bytes = wire_figure.data[2]
        .array
        .as_ref()
        .expect("the array is written");
    assert_eq!(bytes.element, wire::NdArrayElement::U8 as i32);
    assert_eq!(bytes.shape, [2, 2]);
    assert_eq!(bytes.u8_values, [0, 1, 254, 255]);
    assert!(bytes.values.is_empty());
}

// Why: the kitchen-sink figure places each image kind on some planes with some
// policies, so a variant that the encoder or the decoder mishandles elsewhere (a wall
// without an offset, a negative offset, a mirrored or single-pixel range, a policy at a
// category where the kitchen sink uses another) would pass its round trip; every
// variant at every position must reload as itself, bit for bit, and re-encode to the
// same bytes.
#[test]
fn every_image_variant_survives_a_protobuf_round_trip() {
    let original = image_variants_figure();
    let bytes = original.to_protobuf();
    let restored = Figure::from_protobuf(&bytes).expect("own output decodes");
    assert_eq!(restored, original);
    assert_eq!(float_bits(&restored), float_bits(&original));
    assert_eq!(restored.to_protobuf(), bytes);
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
// decoder, two oneof variants or two grid kinds exchanged, or the data arrays renumbered,
// renamed or sorted; IronLAB would still reload its own figures, but readers and writers in
// other languages, which rely on the generated schema, would see the fields exchanged.
// Decoding a wire figure built by hand, in which every pair of fields of the same type
// holds different values and every oneof variant appears, pins each wire field to the
// domain field that it names; the round-trip tests then pin the encoder as well.
#[test]
fn each_wire_field_decodes_into_the_domain_field_that_it_names() {
    // Non-contiguous data identifiers, listed out of order, so that a decoder that
    // renumbers or sorts the data arrays is caught.
    const A: u64 = 7;
    const B: u64 = 10;
    const C: u64 = 12;
    const D: u64 = 15;
    // An array of bytes, which only the scalar image of indices refers to, so that the
    // byte payload is pinned to the domain's 8-bit values and the float payload to its
    // floats.
    const E: u64 = 18;
    // Colour scales of every kind, with every variant of their colormaps, band edges,
    // norms and policies, and different values in every pair of fields of the same type.
    const CONTINUOUS: u64 = 50;
    const DISCRETE: u64 = 51;
    const INDEXED: u64 = 52;
    const CATEGORICAL: u64 = 53;
    const BANDS: u64 = 54;
    let wire_category =
        |policy: Option<wire::OutOfRange>, show_key: bool, label: Option<wire::Text>| {
            Some(wire::ColorScaleOutOfRange {
                policy,
                show_key: Some(show_key),
                label,
            })
        };
    let wire_scale = |id: u64,
                      title: Option<wire::Text>,
                      kind: wire::ColorScaleKindKind,
                      non_finite: Option<wire::ColorScaleOutOfRange>| {
        wire::ColorScale {
            id: Some(id),
            title,
            kind: Some(wire::ColorScaleKind { kind: Some(kind) }),
            non_finite,
        }
    };
    let wire_ticks = |kind: wire::TicksKind| Some(wire::Ticks { kind: Some(kind) });
    let wire_labels = |kind: wire::TickLabelsKind| Some(wire::TickLabels { kind: Some(kind) });
    let wire_named = |name: wire::ColormapName, reverse: bool| {
        Some(wire::Colormap {
            kind: Some(wire::ColormapKind::Named(wire::ColormapNamed {
                name: name as i32,
                reverse: Some(reverse),
            })),
        })
    };
    // Listed out of the order of their identifiers, so that a decoder that sorts them is
    // caught.
    let color_scales = vec![
        wire_scale(
            CATEGORICAL,
            None,
            wire::ColorScaleKindKind::Categorical(wire::ColorScaleKindCategorical {
                categorical: Some(wire::CategoricalColorScale {
                    categories: vec![
                        wire::ColorCategory {
                            color: Some(wire_color(0.0625, 0.125, 0.1875, 0.25)),
                            label: wire_text("first", wire::Interpreter::None),
                        },
                        wire::ColorCategory {
                            color: Some(wire_color(0.3125, 0.375, 0.4375, 0.5)),
                            label: wire_text("$second$", wire::Interpreter::Latex),
                        },
                    ],
                    outside: wire_category(wire_strict(), false, None),
                }),
            }),
            wire_category(wire_transparent(), true, None),
        ),
        wire_scale(
            CONTINUOUS,
            wire_text("scale title", wire::Interpreter::Latex),
            wire::ColorScaleKindKind::Continuous(wire::ColorScaleKindContinuous {
                continuous: Some(wire::ContinuousColorScale {
                    colormap: wire_named(wire::ColormapName::Plasma, true),
                    limits: wire_manual(5.0, 6.0),
                    norm: wire::ColorScaleNorm::Log as i32,
                    below: wire_category(
                        wire_strict(),
                        true,
                        wire_text("low", wire::Interpreter::None),
                    ),
                    above: wire_category(wire_clamp(), false, None),
                    ticks: wire_ticks(wire::TicksKind::Step(wire::TicksStep {
                        step: Some(0.5),
                        origin: Some(5.25),
                    })),
                    tick_labels: wire_labels(wire::TickLabelsKind::Hidden(
                        wire::TickLabelsHidden {},
                    )),
                }),
            }),
            wire_category(
                wire_painted(0.5, 0.375, 0.25, 0.125),
                true,
                wire_text("$nan$", wire::Interpreter::Latex),
            ),
        ),
        wire_scale(
            BANDS,
            None,
            wire::ColorScaleKindKind::Discrete(wire::ColorScaleKindDiscrete {
                discrete: Some(wire::DiscreteColorScale {
                    colormap: wire_named(wire::ColormapName::Magma, false),
                    levels: Some(wire::ColorScaleLevels {
                        kind: Some(wire::ColorScaleLevelsKind::Auto(
                            wire::ColorScaleLevelsAuto {
                                count: Some(7),
                                limits: wire_manual(7.0, 8.0),
                            },
                        )),
                    }),
                    norm: wire::ColorScaleNorm::Log as i32,
                    below: wire_category(wire_clamp(), true, None),
                    above: wire_category(wire_strict(), false, None),
                    ticks: wire_ticks(wire::TicksKind::Auto(wire::TicksAuto {})),
                    tick_labels: wire_labels(wire::TickLabelsKind::Auto(wire::TickLabelsAuto {})),
                }),
            }),
            wire_category(wire_strict(), false, None),
        ),
        wire_scale(
            DISCRETE,
            None,
            wire::ColorScaleKindKind::Discrete(wire::ColorScaleKindDiscrete {
                discrete: Some(wire::DiscreteColorScale {
                    colormap: Some(wire::Colormap {
                        kind: Some(wire::ColormapKind::Listed(wire::ColormapListed {
                            colors: vec![
                                wire_color(0.5625, 0.625, 0.6875, 0.75),
                                wire_color(0.8125, 0.875, 0.9375, 1.0),
                            ],
                        })),
                    }),
                    levels: Some(wire::ColorScaleLevels {
                        kind: Some(wire::ColorScaleLevelsKind::Explicit(
                            wire::ColorScaleLevelsExplicit {
                                values: vec![1.0, 2.0, 4.0],
                            },
                        )),
                    }),
                    norm: wire::ColorScaleNorm::Linear as i32,
                    below: wire_category(wire_transparent(), false, None),
                    above: wire_category(wire_painted(0.125, 0.25, 0.375, 0.5), true, None),
                    ticks: wire_ticks(wire::TicksKind::Values(wire::TicksValues {
                        values: vec![1.0, 4.0],
                    })),
                    tick_labels: wire_labels(wire::TickLabelsKind::Text(wire::TickLabelsText {
                        labels: vec![
                            wire_text("one", wire::Interpreter::None).unwrap(),
                            wire_text("$4$", wire::Interpreter::Latex).unwrap(),
                        ],
                    })),
                }),
            }),
            wire_category(wire_transparent(), false, None),
        ),
        wire_scale(
            INDEXED,
            wire_text("indexed title", wire::Interpreter::None),
            wire::ColorScaleKindKind::Indexed(wire::ColorScaleKindIndexed {
                indexed: Some(wire::IndexedColorScale {
                    colors: vec![
                        wire_color(0.0, 0.25, 0.5, 0.75),
                        wire_color(1.0, 0.75, 0.5, 0.25),
                    ],
                    outside: wire_category(
                        wire_clamp(),
                        true,
                        wire_text("out", wire::Interpreter::None),
                    ),
                    ticks: wire_ticks(wire::TicksKind::Values(wire::TicksValues {
                        values: vec![0.0, 1.0],
                    })),
                    tick_labels: wire_labels(wire::TickLabelsKind::Auto(wire::TickLabelsAuto {})),
                }),
            }),
            wire_category(wire_clamp(), false, None),
        ),
    ];

    let mut wire_figure = wire::Figure {
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
        // A weight and a fitted size in each list, in different positions, so that the
        // two lists or the two kinds of size exchanged are caught.
        layout: Some(wire::TileLayout {
            rows: Some(2),
            cols: Some(3),
            row_weights: vec![wire_weight(3.0), wire_fit()],
            col_weights: vec![wire_fit(), wire_weight(0.25)],
        }),
        color_scales,
        data: vec![
            wire_data(
                C,
                Some("c, δ"),
                wire_floats(vec![3, 1], vec![6.0, 7.0, 8.0]),
            ),
            wire_data(A, Some("a"), wire_floats(vec![2], vec![1.0, 2.0])),
            wire_data(E, None, wire_bytes(vec![3], vec![13, 14, 15])),
            wire_data(B, None, wire_floats(vec![1, 3], vec![3.0, 4.0, 5.0])),
            wire_data(
                D,
                None,
                wire_floats(vec![2, 2], vec![9.0, 10.0, 11.0, 12.0]),
            ),
        ],
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
                title_align: wire::TitleAlign::Right as i32,
                aspect: Some(wire::Aspect {
                    kind: Some(wire::AspectKind::Auto(wire::AspectAuto {})),
                }),
                aspect_adjust: wire::AspectAdjust::Box as i32,
                x: wire_axis(
                    Some("x label"),
                    wire::Scale::Log,
                    wire_manual(1.0, 2.0),
                    true,
                )
                .map(|axis| wire::Axis {
                    reverse: Some(true),
                    visible: Some(false),
                    tick_label_angle: Some(30.0),
                    ticks: Some(wire::Ticks {
                        kind: Some(wire::TicksKind::Step(wire::TicksStep {
                            step: Some(0.25),
                            origin: Some(1.125),
                        })),
                    }),
                    tick_labels: Some(wire::TickLabels {
                        kind: Some(wire::TickLabelsKind::Hidden(wire::TickLabelsHidden {})),
                    }),
                    ..axis
                }),
                y: wire_axis(
                    Some("y label"),
                    wire::Scale::Linear,
                    wire_manual(3.0, 4.0),
                    false,
                )
                .map(|axis| wire::Axis {
                    tick_label_angle: Some(-60.0),
                    ticks: Some(wire::Ticks {
                        kind: Some(wire::TicksKind::Values(wire::TicksValues {
                            values: vec![3.0, 3.5],
                        })),
                    }),
                    tick_labels: Some(wire::TickLabels {
                        kind: Some(wire::TickLabelsKind::Text(wire::TickLabelsText {
                            labels: vec![
                                wire_text("low", wire::Interpreter::None).unwrap(),
                                wire_text("$h$", wire::Interpreter::Latex).unwrap(),
                            ],
                        })),
                    }),
                    ..axis
                }),
                z: wire_axis(None, wire::Scale::Log, wire_auto_limits(), true),
                colorbar: None,
                r#box: Some(false),
                legend: wire_inside(
                    wire::LegendLocation::SouthWest,
                    Some(false),
                    wire_columns(4, wire::LegendEntryOrder::RowMajor),
                ),
                artists: vec![
                    wire_artist(wire::ArtistKind::Series(wire::Series {
                        id: Some(30),
                        label: wire_text("line", wire::Interpreter::Latex),
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
                        size_data: None,
                        color_data: None,
                        color_scale: None,
                        x_coords: wire::Coords::AxesFraction as i32,
                        y_coords: wire::Coords::Data as i32,
                        clip: Some(false),
                    })),
                    wire_artist(wire::ArtistKind::Series(wire::Series {
                        id: Some(31),
                        label: wire_text("markers", wire::Interpreter::None),
                        visible: Some(true),
                        x: Some(B),
                        y: Some(C),
                        z: Some(A),
                        line: wire_line_style(wire_colormapped(), 0.75, wire::DashStyle::None),
                        marker: wire_marker(
                            wire::MarkerShape::Square,
                            3.5,
                            wire_auto_color(),
                            wire_no_color(),
                        ),
                        size_data: Some(C),
                        color_data: Some(D),
                        color_scale: Some(INDEXED),
                        x_coords: wire::Coords::Data as i32,
                        y_coords: wire::Coords::AxesFraction as i32,
                        clip: Some(true),
                    })),
                    wire_artist(wire::ArtistKind::Series(wire::Series {
                        id: Some(32),
                        label: None,
                        visible: Some(true),
                        x: Some(C),
                        y: Some(A),
                        z: None,
                        line: wire_line_style(wire_colormapped(), 4.5, wire::DashStyle::Dotted),
                        marker: wire_marker(
                            wire::MarkerShape::Plus,
                            5.5,
                            wire_rgba(0.0625, 0.1875, 0.3125, 0.4375),
                            wire_auto_color(),
                        ),
                        size_data: None,
                        color_data: None,
                        color_scale: None,
                        x_coords: wire::Coords::AxesFraction as i32,
                        y_coords: wire::Coords::AxesFraction as i32,
                        clip: Some(false),
                    })),
                    wire_artist(wire::ArtistKind::Contour(wire::Contour {
                        id: Some(33),
                        label: wire_text("contour", wire::Interpreter::Latex),
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
                        line_opacity: Some(0.0),
                        fill_opacity: Some(0.5),
                        color_scale: Some(DISCRETE),
                    })),
                    wire_artist(wire::ArtistKind::Contour(wire::Contour {
                        id: Some(34),
                        label: None,
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
                        line_opacity: Some(0.875),
                        fill_opacity: Some(0.0),
                        color_scale: Some(BANDS),
                    })),
                    wire_artist(wire::ArtistKind::Quiver(wire::Quiver {
                        id: Some(35),
                        label: wire_text("quiver", wire::Interpreter::Latex),
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
                        label: None,
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
                        label: None,
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
                        label: wire_text("surface", wire::Interpreter::None),
                        visible: Some(false),
                        grid: wire_rectilinear(D, A),
                        z: Some(B),
                        c: Some(C),
                        face: wire_rgba(0.25, 0.5, 0.75, 1.0),
                        edge: wire_auto_color(),
                        edge_width_pt: Some(0.625),
                        face_alpha: Some(D),
                        edge_alpha: Some(A),
                        face_opacity: Some(0.125),
                        edge_opacity: Some(0.375),
                        color_scale: Some(CATEGORICAL),
                    })),
                    wire_artist(wire::ArtistKind::Surface(wire::Surface {
                        id: Some(39),
                        label: None,
                        visible: Some(true),
                        grid: wire_curvilinear(A, D),
                        z: Some(C),
                        c: None,
                        face: wire_no_color(),
                        edge: wire_colormapped(),
                        edge_width_pt: Some(0.875),
                        face_alpha: Some(B),
                        edge_alpha: None,
                        face_opacity: Some(0.625),
                        edge_opacity: Some(0.25),
                        color_scale: Some(CONTINUOUS),
                    })),
                    wire_artist(wire::ArtistKind::Image(wire::Image {
                        id: Some(40),
                        label: wire_text("image", wire::Interpreter::Latex),
                        visible: Some(false),
                        pixels: Some(D),
                        placement: wire_placement(
                            wire_xz(Some(21.0)),
                            wire_range(22.0, 23.0),
                            wire_range(24.0, 25.0),
                        ),
                        opacity: Some(0.75),
                    })),
                    wire_artist(wire::ArtistKind::ScalarImage(wire::ScalarImage {
                        id: Some(41),
                        label: None,
                        visible: Some(true),
                        values: Some(E),
                        color_scale: Some(INDEXED),
                        placement: wire_placement(
                            wire_yz(Some(26.0)),
                            None,
                            wire_range(28.0, 27.0),
                        ),
                        alpha: Some(C),
                        opacity: Some(0.875),
                    })),
                    wire_artist(wire::ArtistKind::ScalarImage(wire::ScalarImage {
                        id: Some(42),
                        label: wire_text("mapped", wire::Interpreter::None),
                        visible: Some(false),
                        values: Some(B),
                        color_scale: Some(CONTINUOUS),
                        placement: wire_placement(
                            wire_xy(Some(29.0)),
                            wire_range(30.0, 31.0),
                            None,
                        ),
                        alpha: Some(A),
                        opacity: Some(0.0625),
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
                title_align: wire::TitleAlign::Left as i32,
                aspect: Some(wire::Aspect {
                    kind: Some(wire::AspectKind::Ratio(wire::AspectRatio {
                        y: Some(2.5),
                        z: Some(0.75),
                    })),
                }),
                aspect_adjust: wire::AspectAdjust::Limits as i32,
                x: wire_axis(None, wire::Scale::Linear, wire_auto_limits(), false),
                y: wire_axis(None, wire::Scale::Linear, wire_auto_limits(), false),
                z: wire_axis(None, wire::Scale::Linear, wire_auto_limits(), false),
                colorbar: None,
                r#box: Some(true),
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
            typesetter: "typesetter".to_owned(),
            typesetter_version: "6.5".to_owned(),
            fonts: vec!["Font B".to_owned(), "Font A".to_owned()],
        }),
        parameters: BTreeMap::from([
            (
                "a".to_owned(),
                wire::Parameter {
                    kind: Some(wire::ParameterKind::Number(wire::ParameterNumber {
                        value: Some(16.0),
                    })),
                },
            ),
            (
                "b".to_owned(),
                wire::Parameter {
                    kind: Some(wire::ParameterKind::Number(wire::ParameterNumber {
                        value: Some(17.0),
                    })),
                },
            ),
            (
                "c".to_owned(),
                wire::Parameter {
                    kind: Some(wire::ParameterKind::Bool(wire::ParameterBool {
                        value: Some(true),
                    })),
                },
            ),
            (
                "d".to_owned(),
                wire::Parameter {
                    kind: Some(wire::ParameterKind::Integer(wire::ParameterInteger {
                        value: Some(-18),
                    })),
                },
            ),
            (
                "e".to_owned(),
                wire::Parameter {
                    kind: Some(wire::ParameterKind::String(wire::ParameterString {
                        value: "parameter".to_owned(),
                    })),
                },
            ),
        ]),
        // Not in alphabetical order, so that a decoder that sorts them is caught.
        tags: vec!["tag two".to_owned(), "tag one".to_owned()],
        panels: vec![wire::Panel {
            id: Some(40),
            cell: Some(wire::Cell {
                row: Some(5),
                col: Some(6),
                row_span: Some(7),
                col_span: Some(8),
            }),
            align: wire::PanelLocation::West as i32,
            title: wire_text("panel title", wire::Interpreter::Latex),
            title_align: wire::TitleAlign::Right as i32,
            content: wire_panel_legend(wire::PanelLegend {
                boxed: Some(true),
                layout: wire_columns(9, wire::LegendEntryOrder::RowMajor),
                // Every kind of item: an artist of the figure on a row of its own, then a
                // custom line sample and a custom swatch, whose labels and colours differ.
                entries: wire_explicit(vec![
                    wire_artist_item(42, Some(true)),
                    wire_custom_item(
                        wire_text("custom line", wire::Interpreter::Latex),
                        wire::LegendSampleKind::Line(wire::LegendSampleLine {
                            line: wire_line_style(
                                wire_rgba(0.25, 0.375, 0.5, 0.625),
                                0.75,
                                wire::DashStyle::Dashed,
                            ),
                            marker: wire_marker(
                                wire::MarkerShape::Square,
                                3.5,
                                wire_rgba(0.75, 0.5, 0.25, 1.0),
                                wire_no_color(),
                            ),
                        }),
                        Some(false),
                    ),
                    wire_custom_item(
                        wire_text("custom swatch", wire::Interpreter::None),
                        wire::LegendSampleKind::Swatch(wire::LegendSampleSwatch {
                            color: Some(wire_color(0.125, 0.875, 0.375, 0.25)),
                        }),
                        Some(false),
                    ),
                ]),
            }),
        }],
    };
    // The inside legend lists two artists of its own axes, the later one first and on a
    // row of its own, so that a decoder that sorts the items or drops `full_row` is
    // caught.
    inside_mut(&mut wire_figure.axes[0].legend).entries = wire_explicit(vec![
        wire_artist_item(31, Some(true)),
        wire_artist_item(30, Some(false)),
    ]);

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
        reverse: false,
        ticks: Ticks::Auto,
        tick_labels: TickLabels::Auto,
        grid: false,
        visible: true,
        tick_label_angle: 0.0,
    };
    let category = |policy: OutOfRange, show_key: bool, label: Option<Text>| ColorScaleOutOfRange {
        policy,
        show_key,
        label,
    };
    let expected_scales = vec![
        ColorScale {
            id: NodeId(CATEGORICAL),
            title: None,
            kind: ColorScaleKind::Categorical(CategoricalColorScale {
                categories: vec![
                    ColorCategory {
                        color: Color::rgba(0.0625, 0.125, 0.1875, 0.25),
                        label: Text::plain("first"),
                    },
                    ColorCategory {
                        color: Color::rgba(0.3125, 0.375, 0.4375, 0.5),
                        label: Text::new("$second$"),
                    },
                ],
                outside: category(OutOfRange::Strict, false, None),
            }),
            non_finite: category(OutOfRange::Transparent, true, None),
        },
        ColorScale {
            id: NodeId(CONTINUOUS),
            title: text("scale title", Interpreter::Latex),
            kind: ColorScaleKind::Continuous(ContinuousColorScale {
                colormap: Colormap::Named {
                    name: ColormapName::Plasma,
                    reverse: true,
                },
                limits: Limits::Manual { min: 5.0, max: 6.0 },
                norm: ColorScaleNorm::Log,
                below: category(OutOfRange::Strict, true, text("low", Interpreter::None)),
                above: category(OutOfRange::Clamp, false, None),
                ticks: Ticks::Step {
                    step: 0.5,
                    origin: 5.25,
                },
                tick_labels: TickLabels::Hidden,
            }),
            non_finite: category(
                OutOfRange::Rgba {
                    color: Color::rgba(0.5, 0.375, 0.25, 0.125),
                },
                true,
                text("$nan$", Interpreter::Latex),
            ),
        },
        ColorScale {
            id: NodeId(BANDS),
            title: None,
            kind: ColorScaleKind::Discrete(DiscreteColorScale {
                colormap: Colormap::Named {
                    name: ColormapName::Magma,
                    reverse: false,
                },
                levels: ColorScaleLevels::Auto {
                    count: 7,
                    limits: Limits::Manual { min: 7.0, max: 8.0 },
                },
                norm: ColorScaleNorm::Log,
                below: category(OutOfRange::Clamp, true, None),
                above: category(OutOfRange::Strict, false, None),
                ticks: Ticks::Auto,
                tick_labels: TickLabels::Auto,
            }),
            non_finite: category(OutOfRange::Strict, false, None),
        },
        ColorScale {
            id: NodeId(DISCRETE),
            title: None,
            kind: ColorScaleKind::Discrete(DiscreteColorScale {
                colormap: Colormap::Listed {
                    colors: vec![
                        Color::rgba(0.5625, 0.625, 0.6875, 0.75),
                        Color::rgba(0.8125, 0.875, 0.9375, 1.0),
                    ],
                },
                levels: ColorScaleLevels::Explicit {
                    values: vec![1.0, 2.0, 4.0],
                },
                norm: ColorScaleNorm::Linear,
                below: category(OutOfRange::Transparent, false, None),
                above: category(
                    OutOfRange::Rgba {
                        color: Color::rgba(0.125, 0.25, 0.375, 0.5),
                    },
                    true,
                    None,
                ),
                ticks: Ticks::Values(vec![1.0, 4.0]),
                tick_labels: TickLabels::Text(vec![Text::plain("one"), Text::new("$4$")]),
            }),
            non_finite: category(OutOfRange::Transparent, false, None),
        },
        ColorScale {
            id: NodeId(INDEXED),
            title: text("indexed title", Interpreter::None),
            kind: ColorScaleKind::Indexed(IndexedColorScale {
                colors: vec![
                    Color::rgba(0.0, 0.25, 0.5, 0.75),
                    Color::rgba(1.0, 0.75, 0.5, 0.25),
                ],
                outside: category(OutOfRange::Clamp, true, text("out", Interpreter::None)),
                ticks: Ticks::Values(vec![0.0, 1.0]),
                tick_labels: TickLabels::Auto,
            }),
            non_finite: category(OutOfRange::Clamp, false, None),
        },
    ];
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
        layout: TileLayout {
            rows: 2,
            cols: 3,
            row_weights: vec![TrackSize::Weight(3.0), TrackSize::Fit],
            col_weights: vec![TrackSize::Fit, TrackSize::Weight(0.25)],
        },
        color_scales: expected_scales,
        data: vec![
            DataArray {
                id: NodeId(C),
                name: Some("c, δ".to_owned()),
                array: NdArray {
                    shape: vec![3, 1],
                    values: Values::F64(vec![6.0, 7.0, 8.0]),
                },
            },
            DataArray {
                id: NodeId(A),
                name: Some("a".to_owned()),
                array: NdArray::vector(vec![1.0, 2.0]),
            },
            DataArray {
                id: NodeId(E),
                name: None,
                array: NdArray {
                    shape: vec![3],
                    values: Values::U8(vec![13, 14, 15]),
                },
            },
            DataArray {
                id: NodeId(B),
                name: None,
                array: NdArray {
                    shape: vec![1, 3],
                    values: Values::F64(vec![3.0, 4.0, 5.0]),
                },
            },
            DataArray {
                id: NodeId(D),
                name: None,
                array: NdArray {
                    shape: vec![2, 2],
                    values: Values::F64(vec![9.0, 10.0, 11.0, 12.0]),
                },
            },
        ],
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
                title_align: TitleAlign::Right,
                aspect: Aspect::Auto,
                aspect_adjust: AspectAdjust::Box,
                x: Axis {
                    label: text("x label", Interpreter::Latex),
                    scale: Scale::Log,
                    limits: Limits::Manual { min: 1.0, max: 2.0 },
                    reverse: true,
                    ticks: Ticks::Step {
                        step: 0.25,
                        origin: 1.125,
                    },
                    tick_labels: TickLabels::Hidden,
                    grid: true,
                    visible: false,
                    tick_label_angle: 30.0,
                },
                y: Axis {
                    label: text("y label", Interpreter::Latex),
                    scale: Scale::Linear,
                    limits: Limits::Manual { min: 3.0, max: 4.0 },
                    reverse: false,
                    ticks: Ticks::Values(vec![3.0, 3.5]),
                    tick_labels: TickLabels::Text(vec![Text::plain("low"), Text::new("$h$")]),
                    grid: false,
                    visible: true,
                    tick_label_angle: -60.0,
                },
                z: Axis {
                    label: None,
                    scale: Scale::Log,
                    limits: Limits::Auto,
                    reverse: false,
                    ticks: Ticks::Auto,
                    tick_labels: TickLabels::Auto,
                    grid: true,
                    visible: true,
                    tick_label_angle: 0.0,
                },
                colorbar: None,
                box_: false,
                legend: Some(Legend::Inside(InsideLegend {
                    location: LegendLocation::SouthWest,
                    boxed: false,
                    layout: LegendEntryLayout {
                        columns: LegendEntryColumns::Count(4),
                        order: LegendEntryOrder::RowMajor,
                    },
                    entries: LegendEntries::Explicit(vec![
                        LegendItem {
                            content: LegendItemContent::Artist(NodeId(31)),
                            full_row: true,
                        },
                        LegendItem {
                            content: LegendItemContent::Artist(NodeId(30)),
                            full_row: false,
                        },
                    ]),
                })),
                artists: vec![
                    Artist::Series(Series {
                        id: NodeId(30),
                        label: text("line", Interpreter::Latex),
                        visible: false,
                        x: NodeId(A),
                        y: NodeId(B),
                        z: Some(NodeId(C)),
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
                        size_data: None,
                        color_data: None,
                        color_scale: None,
                        x_coords: Coords::AxesFraction,
                        y_coords: Coords::Data,
                        clip: false,
                    }),
                    Artist::Series(Series {
                        id: NodeId(31),
                        label: text("markers", Interpreter::None),
                        visible: true,
                        x: NodeId(B),
                        y: NodeId(C),
                        z: Some(NodeId(A)),
                        line: LineStyle {
                            color: ColorSpec::Colormapped,
                            width_pt: 0.75,
                            dash: DashStyle::None,
                        },
                        marker: MarkerStyle {
                            shape: MarkerShape::Square,
                            size_pt: 3.5,
                            face: ColorSpec::Auto,
                            edge: ColorSpec::None,
                        },
                        size_data: Some(NodeId(C)),
                        color_data: Some(NodeId(D)),
                        color_scale: Some(NodeId(INDEXED)),
                        x_coords: Coords::Data,
                        y_coords: Coords::AxesFraction,
                        clip: true,
                    }),
                    Artist::Series(Series {
                        id: NodeId(32),
                        label: None,
                        visible: true,
                        x: NodeId(C),
                        y: NodeId(A),
                        z: None,
                        line: LineStyle {
                            color: ColorSpec::Colormapped,
                            width_pt: 4.5,
                            dash: DashStyle::Dotted,
                        },
                        size_data: None,
                        color_data: None,
                        color_scale: None,
                        marker: MarkerStyle {
                            shape: MarkerShape::Plus,
                            size_pt: 5.5,
                            face: ColorSpec::Rgba {
                                color: Color::rgba(0.0625, 0.1875, 0.3125, 0.4375),
                            },
                            edge: ColorSpec::Auto,
                        },
                        x_coords: Coords::AxesFraction,
                        y_coords: Coords::AxesFraction,
                        clip: false,
                    }),
                    Artist::Contour(Contour {
                        id: NodeId(33),
                        label: text("contour", Interpreter::Latex),
                        visible: false,
                        grid: Grid::Curvilinear {
                            x: NodeId(B),
                            y: NodeId(C),
                        },
                        z: NodeId(D),
                        levels: Some(Levels::Explicit {
                            values: vec![1.0, 2.0, 3.0],
                        }),
                        fill: true,
                        placement: ContourPlacement::Plane { z: Some(-2.0) },
                        line: LineStyle {
                            color: ColorSpec::Auto,
                            width_pt: 1.5,
                            dash: DashStyle::Dotted,
                        },
                        line_opacity: 0.0,
                        fill_opacity: 0.5,
                        color_scale: Some(NodeId(DISCRETE)),
                    }),
                    Artist::Contour(Contour {
                        id: NodeId(34),
                        label: None,
                        visible: true,
                        grid: Grid::Rectilinear {
                            x: NodeId(C),
                            y: NodeId(B),
                        },
                        z: NodeId(A),
                        levels: Some(Levels::Auto { count: 5 }),
                        fill: false,
                        placement: ContourPlacement::AtLevel,
                        line: LineStyle {
                            color: ColorSpec::None,
                            width_pt: 1.75,
                            dash: DashStyle::Dashed,
                        },
                        line_opacity: 0.875,
                        fill_opacity: 0.0,
                        color_scale: Some(NodeId(BANDS)),
                    }),
                    Artist::Quiver(Quiver {
                        id: NodeId(35),
                        label: text("quiver", Interpreter::Latex),
                        visible: true,
                        x: NodeId(A),
                        y: NodeId(B),
                        z: Some(NodeId(C)),
                        u: NodeId(D),
                        v: NodeId(A),
                        w: Some(NodeId(B)),
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
                        label: None,
                        visible: true,
                        x: NodeId(B),
                        y: NodeId(A),
                        z: None,
                        u: NodeId(C),
                        v: NodeId(D),
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
                        label: None,
                        visible: true,
                        x: NodeId(C),
                        y: NodeId(D),
                        z: None,
                        u: NodeId(A),
                        v: NodeId(B),
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
                        label: text("surface", Interpreter::None),
                        visible: false,
                        grid: Grid::Rectilinear {
                            x: NodeId(D),
                            y: NodeId(A),
                        },
                        z: NodeId(B),
                        c: Some(NodeId(C)),
                        face: ColorSpec::Rgba {
                            color: Color::rgba(0.25, 0.5, 0.75, 1.0),
                        },
                        edge: ColorSpec::Auto,
                        edge_width_pt: 0.625,
                        face_alpha: Some(NodeId(D)),
                        edge_alpha: Some(NodeId(A)),
                        face_opacity: 0.125,
                        edge_opacity: 0.375,
                        color_scale: Some(NodeId(CATEGORICAL)),
                    }),
                    Artist::Surface(Surface {
                        id: NodeId(39),
                        label: None,
                        visible: true,
                        grid: Grid::Curvilinear {
                            x: NodeId(A),
                            y: NodeId(D),
                        },
                        z: NodeId(C),
                        c: None,
                        face: ColorSpec::None,
                        edge: ColorSpec::Colormapped,
                        edge_width_pt: 0.875,
                        face_alpha: Some(NodeId(B)),
                        edge_alpha: None,
                        face_opacity: 0.625,
                        edge_opacity: 0.25,
                        color_scale: Some(NodeId(CONTINUOUS)),
                    }),
                    Artist::Image(Image {
                        id: NodeId(40),
                        label: text("image", Interpreter::Latex),
                        visible: false,
                        pixels: NodeId(D),
                        placement: ImagePlacement {
                            plane: ImagePlane::Xz { y: Some(21.0) },
                            columns: Some(PixelRange {
                                first: 22.0,
                                last: 23.0,
                            }),
                            rows: Some(PixelRange {
                                first: 24.0,
                                last: 25.0,
                            }),
                        },
                        opacity: 0.75,
                    }),
                    Artist::ScalarImage(ScalarImage {
                        id: NodeId(41),
                        label: None,
                        visible: true,
                        values: NodeId(E),
                        color_scale: Some(NodeId(INDEXED)),
                        placement: ImagePlacement {
                            plane: ImagePlane::Yz { x: Some(26.0) },
                            columns: None,
                            rows: Some(PixelRange {
                                first: 28.0,
                                last: 27.0,
                            }),
                        },
                        alpha: Some(NodeId(C)),
                        opacity: 0.875,
                    }),
                    Artist::ScalarImage(ScalarImage {
                        id: NodeId(42),
                        label: text("mapped", Interpreter::None),
                        visible: false,
                        values: NodeId(B),
                        color_scale: Some(NodeId(CONTINUOUS)),
                        placement: ImagePlacement {
                            plane: ImagePlane::Xy { z: Some(29.0) },
                            columns: Some(PixelRange {
                                first: 30.0,
                                last: 31.0,
                            }),
                            rows: None,
                        },
                        alpha: Some(NodeId(A)),
                        opacity: 0.0625,
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
                title_align: TitleAlign::Left,
                aspect: Aspect::Ratio { y: 2.5, z: 0.75 },
                aspect_adjust: AspectAdjust::Limits,
                x: auto_axis.clone(),
                y: auto_axis.clone(),
                z: auto_axis,
                colorbar: None,
                box_: true,
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
            typesetter: "typesetter".to_owned(),
            typesetter_version: "6.5".to_owned(),
            fonts: vec!["Font B".to_owned(), "Font A".to_owned()],
        },
        parameters: BTreeMap::from([
            ("a".to_owned(), Parameter::Number(16.0)),
            ("b".to_owned(), Parameter::Number(17.0)),
            ("c".to_owned(), Parameter::Bool(true)),
            ("d".to_owned(), Parameter::Integer(-18)),
            ("e".to_owned(), Parameter::String("parameter".to_owned())),
        ]),
        tags: vec!["tag two".to_owned(), "tag one".to_owned()],
        panels: vec![Panel {
            id: NodeId(40),
            cell: Cell {
                row: 5,
                col: 6,
                row_span: 7,
                col_span: 8,
            },
            align: PanelLocation::West,
            title: text("panel title", Interpreter::Latex),
            title_align: TitleAlign::Right,
            content: PanelContent::Legend(PanelLegend {
                boxed: true,
                layout: LegendEntryLayout {
                    columns: LegendEntryColumns::Count(9),
                    order: LegendEntryOrder::RowMajor,
                },
                entries: LegendEntries::Explicit(vec![
                    LegendItem {
                        content: LegendItemContent::Artist(NodeId(42)),
                        full_row: true,
                    },
                    LegendItem {
                        content: LegendItemContent::Custom {
                            label: Text {
                                content: "custom line".to_owned(),
                                interpreter: Interpreter::Latex,
                            },
                            sample: LegendSample::Line {
                                line: LineStyle {
                                    color: ColorSpec::Rgba {
                                        color: Color::rgba(0.25, 0.375, 0.5, 0.625),
                                    },
                                    width_pt: 0.75,
                                    dash: DashStyle::Dashed,
                                },
                                marker: MarkerStyle {
                                    shape: MarkerShape::Square,
                                    size_pt: 3.5,
                                    face: ColorSpec::Rgba {
                                        color: Color::rgba(0.75, 0.5, 0.25, 1.0),
                                    },
                                    edge: ColorSpec::None,
                                },
                            },
                        },
                        full_row: false,
                    },
                    LegendItem {
                        content: LegendItemContent::Custom {
                            label: Text {
                                content: "custom swatch".to_owned(),
                                interpreter: Interpreter::None,
                            },
                            sample: LegendSample::Swatch {
                                color: Color::rgba(0.125, 0.875, 0.375, 0.25),
                            },
                        },
                        full_row: false,
                    },
                ]),
            }),
        }],
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

    /// A wire figure with a place for every enum: a title, a link, an array without
    /// values (so that either element decodes), a continuous colour scale with a named
    /// colormap, an axes with an x axis, a legend and a line with a line style and a
    /// marker style, and a panel holding a legend.
    fn carrier() -> wire::Figure {
        wire::Figure {
            font_set: wire::FontSetId::StixTwo as i32,
            color_scales: vec![wire::ColorScale {
                id: Some(5),
                kind: Some(wire::ColorScaleKind {
                    kind: Some(wire::ColorScaleKindKind::Continuous(
                        wire::ColorScaleKindContinuous {
                            continuous: Some(wire::ContinuousColorScale {
                                colormap: Some(wire::Colormap {
                                    kind: Some(wire::ColormapKind::Named(wire::ColormapNamed {
                                        name: wire::ColormapName::Viridis as i32,
                                        reverse: Some(false),
                                    })),
                                }),
                                norm: wire::ColorScaleNorm::Linear as i32,
                                ..wire::ContinuousColorScale::default()
                            }),
                        },
                    )),
                }),
                ..wire::ColorScale::default()
            }],
            title: wire_text("title", wire::Interpreter::Latex),
            data: vec![wire_data(0, None, wire_floats(vec![0], vec![]))],
            panels: vec![wire::Panel {
                id: Some(4),
                cell: Some(wire::Cell::default()),
                align: wire::PanelLocation::Center as i32,
                content: wire_panel_legend(wire::PanelLegend::default()),
                ..wire::Panel::default()
            }],
            axes: vec![wire::Axes {
                id: Some(2),
                x: wire_axis(None, wire::Scale::Linear, wire_auto_limits(), false),
                legend: wire_inside(
                    wire::LegendLocation::NorthEast,
                    Some(true),
                    wire_columns(1, wire::LegendEntryOrder::ColumnMajor),
                ),
                artists: vec![wire_artist(wire::ArtistKind::Series(wire::Series {
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
                    ..wire::Series::default()
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

    fn wire_line(fig: &mut wire::Figure) -> &mut wire::Series {
        match fig.axes[0].artists[0].kind.as_mut() {
            Some(wire::ArtistKind::Series(line)) => line,
            _ => unreachable!("the carrier holds a line"),
        }
    }

    fn wire_continuous(fig: &mut wire::Figure) -> &mut wire::ContinuousColorScale {
        match fig.color_scales[0]
            .kind
            .as_mut()
            .and_then(|kind| kind.kind.as_mut())
        {
            Some(wire::ColorScaleKindKind::Continuous(kind)) => kind
                .continuous
                .as_mut()
                .expect("the carrier's scale is present"),
            _ => unreachable!("the carrier holds a continuous scale"),
        }
    }

    fn continuous(fig: &Figure) -> &ContinuousColorScale {
        match &fig.color_scales[0].kind {
            ColorScaleKind::Continuous(scale) => scale,
            other => panic!("the carrier's scale decoded as {other:?}"),
        }
    }

    fn line(fig: &Figure) -> &Series {
        match &fig.axes[0].artists[0] {
            Artist::Series(line) => line,
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
            |w, v| {
                if let Some(wire::Colormap {
                    kind: Some(wire::ColormapKind::Named(named)),
                }) = wire_continuous(w).colormap.as_mut()
                {
                    named.name = v;
                }
            },
            |f| match &continuous(f).colormap {
                Colormap::Named { name, .. } => format!("{name:?}"),
                other => panic!("the carrier's colormap decoded as {other:?}"),
            },
        ),
        check::<wire::ColorScaleNorm>(
            |w, v| wire_continuous(w).norm = v,
            |f| format!("{:?}", continuous(f).norm),
        ),
        check::<wire::LegendLocation>(
            |w, v| inside_mut(&mut w.axes[0].legend).location = v,
            |f| format!("{:?}", inside(&f.axes[0].legend).location),
        ),
        check::<wire::PanelLocation>(
            |w, v| w.panels[0].align = v,
            |f| format!("{:?}", f.panels[0].align),
        ),
        // The carrier's panel holds a legend, so the colourbar of a panel replaces its
        // content, and the axes is given an inside colourbar of the carrier's scale.
        check::<wire::ColorbarOrientation>(
            |w, v| {
                w.panels[0].content = Some(wire::PanelContent {
                    kind: Some(wire::PanelContentKind::Colorbar(
                        wire::PanelContentColorbar {
                            colorbar: Some(wire::PanelColorbar {
                                scale: Some(5),
                                orientation: v,
                                ..wire::PanelColorbar::default()
                            }),
                        },
                    )),
                });
            },
            |f| match &f.panels[0].content {
                PanelContent::Colorbar(colorbar) => format!("{:?}", colorbar.orientation),
                PanelContent::Legend(_) => "a legend".to_owned(),
            },
        ),
        check::<wire::InsideColorbarSide>(
            |w, v| {
                w.axes[0].colorbar = Some(wire::InsideColorbar {
                    scale: Some(5),
                    side: v,
                    length: Some(0.5),
                    ..wire::InsideColorbar::default()
                });
            },
            |f| format!("{:?}", f.axes[0].colorbar.map(|c| c.side).unwrap()),
        ),
        // The placement of a scale's title is checked on a panel colourbar, which replaces
        // the carrier's legend as above.
        check::<wire::ColorbarTitlePlacement>(
            |w, v| {
                w.panels[0].content = Some(wire::PanelContent {
                    kind: Some(wire::PanelContentKind::Colorbar(
                        wire::PanelContentColorbar {
                            colorbar: Some(wire::PanelColorbar {
                                scale: Some(5),
                                title_placement: v,
                                ..wire::PanelColorbar::default()
                            }),
                        },
                    )),
                });
            },
            |f| match &f.panels[0].content {
                PanelContent::Colorbar(colorbar) => format!("{:?}", colorbar.title_placement),
                PanelContent::Legend(_) => "a legend".to_owned(),
            },
        ),
        check::<wire::LegendEntryOrder>(
            |w, v| {
                inside_mut(&mut w.axes[0].legend)
                    .layout
                    .as_mut()
                    .unwrap()
                    .order = v;
            },
            |f| format!("{:?}", inside(&f.axes[0].legend).layout.order),
        ),
        check::<wire::TitleAlign>(
            |w, v| w.axes[0].title_align = v,
            |f| format!("{:?}", f.axes[0].title_align),
        ),
        check::<wire::AspectAdjust>(
            |w, v| w.axes[0].aspect_adjust = v,
            |f| format!("{:?}", f.axes[0].aspect_adjust),
        ),
        check::<wire::DashStyle>(
            |w, v| wire_line(w).line.as_mut().unwrap().dash = v,
            |f| format!("{:?}", line(f).line.dash),
        ),
        check::<wire::MarkerShape>(
            |w, v| wire_line(w).marker.as_mut().unwrap().shape = v,
            |f| format!("{:?}", line(f).marker.shape),
        ),
        check::<wire::Coords>(
            |w, v| wire_line(w).x_coords = v,
            |f| format!("{:?}", line(f).x_coords),
        ),
        check::<wire::Dimension>(
            |w, v| w.links[0].dimension = v,
            |f| format!("{:?}", f.links[0].dimension),
        ),
        // Bars and stairs are added beside the carrier's line, so that their enums are
        // placed without changing it.
        check::<wire::Orientation>(
            |w, v| {
                w.axes[0]
                    .artists
                    .push(wire_artist(wire::ArtistKind::Bar(wire::Bar {
                        id: Some(6),
                        positions: Some(0),
                        values: Some(0),
                        orientation: v,
                        ..wire::Bar::default()
                    })));
            },
            |f| match &f.axes[0].artists[1] {
                Artist::Bar(bar) => format!("{:?}", bar.orientation),
                other => panic!("the carrier's bars decoded as {other:?}"),
            },
        ),
        check::<wire::BarLayout>(
            |w, v| {
                w.axes[0]
                    .artists
                    .push(wire_artist(wire::ArtistKind::Bar(wire::Bar {
                        id: Some(6),
                        positions: Some(0),
                        values: Some(0),
                        group: Some(wire::BarGroup {
                            name: "g".to_owned(),
                            layout: v,
                        }),
                        ..wire::Bar::default()
                    })));
            },
            |f| match &f.axes[0].artists[1] {
                Artist::Bar(Bar {
                    group: Some(group), ..
                }) => format!("{:?}", group.layout),
                other => panic!("the carrier's grouped bars decoded as {other:?}"),
            },
        ),
        check::<wire::StepAt>(
            |w, v| {
                w.axes[0]
                    .artists
                    .push(wire_artist(wire::ArtistKind::Stairs(wire::Stairs {
                        id: Some(6),
                        data: Some(wire::StairsData {
                            kind: Some(wire::StairsDataKind::Samples(wire::StairsDataSamples {
                                x: Some(0),
                                values: Some(0),
                                step: v,
                            })),
                        }),
                        ..wire::Stairs::default()
                    })));
            },
            |f| match &f.axes[0].artists[1] {
                Artist::Stairs(Stairs {
                    data: StairsData::Samples { step, .. },
                    ..
                }) => format!("{step:?}"),
                other => panic!("the carrier's stairs decoded as {other:?}"),
            },
        ),
        check::<wire::NdArrayElement>(
            |w, v| {
                w.data[0]
                    .array
                    .as_mut()
                    .expect("the carrier holds an array")
                    .element = v;
            },
            |f| format!("{:?}", f.data[0].array.element()),
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
// hidden artists would reappear, zero widths would widen, transparent images and
// surfaces (an opacity of 0) would become opaque, and an explicitly automatic contour
// colour would become colormapped. Every value here is present on the wire and
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
            row_weights: vec![wire_weight(0.0)],
            col_weights: vec![],
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
            colorbar: None,
            r#box: Some(false),
            legend: Some(wire::Legend {
                kind: Some(wire::LegendKind::Inside(wire::LegendInside {
                    inside: Some(wire::InsideLegend {
                        location: wire::LegendLocation::NorthEast as i32,
                        boxed: Some(false),
                        layout: wire_columns(0, wire::LegendEntryOrder::ColumnMajor),
                        // An explicit list of no items, which is present and empty, and
                        // not the automatic list that absent entries mean.
                        entries: wire_explicit(vec![]),
                    }),
                })),
            }),
            artists: vec![
                wire_artist(wire::ArtistKind::Series(wire::Series {
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
                    ..wire::Series::default()
                })),
                wire_artist(wire::ArtistKind::Series(wire::Series {
                    id: Some(4),
                    x: Some(0),
                    y: Some(0),
                    line: wire_line_style(wire_auto_color(), 0.0, wire::DashStyle::Solid),
                    marker: wire_marker(
                        wire::MarkerShape::None,
                        0.0,
                        wire_no_color(),
                        wire_auto_color(),
                    ),
                    // Size and colour data of identifier zero, which are present and not
                    // the absence that an unset field means.
                    size_data: Some(0),
                    color_data: Some(0),
                    ..wire::Series::default()
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
                    line_opacity: Some(0.0),
                    fill_opacity: Some(0.0),
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
                    face_opacity: Some(0.0),
                    edge_opacity: Some(0.0),
                    ..wire::Surface::default()
                })),
                // A floor offset and pixel centres of zero are values, not absences: an
                // absent offset means the low end of the axis and absent ranges mean
                // centres at 0 to n - 1.
                wire_artist(wire::ArtistKind::Image(wire::Image {
                    id: Some(8),
                    pixels: Some(0),
                    placement: wire_placement(
                        wire_xy(Some(0.0)),
                        wire_range(0.0, 0.0),
                        wire_range(0.0, 0.0),
                    ),
                    opacity: Some(0.0),
                    ..wire::Image::default()
                })),
                wire_artist(wire::ArtistKind::ScalarImage(wire::ScalarImage {
                    id: Some(9),
                    values: Some(0),
                    color_scale: Some(0),
                    placement: wire_placement(wire_xz(Some(0.0)), None, None),
                    alpha: Some(0),
                    opacity: Some(0.0),
                    ..wire::ScalarImage::default()
                })),
            ],
            ..wire::Axes::default()
        }],
        // A legend panel whose values are zero or the defaults of an inside legend, all of
        // which differ from those of a panel: no border, and a block at the top right in
        // one column.
        panels: vec![wire::Panel {
            id: Some(10),
            cell: Some(wire::Cell {
                row: Some(0),
                col: Some(0),
                row_span: Some(0),
                col_span: Some(0),
            }),
            align: wire::PanelLocation::NorthEast as i32,
            // Centred, which is the default of an axes title but not of a panel title.
            title: None,
            title_align: wire::TitleAlign::Center as i32,
            content: wire_panel_legend(wire::PanelLegend {
                boxed: Some(false),
                layout: wire_columns(1, wire::LegendEntryOrder::ColumnMajor),
                // An item that names the artist of identifier zero, which is an identifier
                // and not an absence, and whose `full_row` is present and false.
                entries: wire_explicit(vec![wire_artist_item(0, Some(false))]),
            }),
        }],
        // Colour scales whose categories hold policies present on the wire that are the
        // defaults of other categories (transparent below a continuous scale, which clamps
        // by default; transparent outside an indexed scale, which is strict by default;
        // clamped for non-finite values, which are transparent by default), whose keys
        // are present and false, and whose band count is zero.
        color_scales: vec![
            wire::ColorScale {
                id: Some(0),
                kind: Some(wire::ColorScaleKind {
                    kind: Some(wire::ColorScaleKindKind::Continuous(
                        wire::ColorScaleKindContinuous {
                            continuous: Some(wire::ContinuousColorScale {
                                below: Some(wire::ColorScaleOutOfRange {
                                    policy: wire_transparent(),
                                    show_key: Some(false),
                                    label: None,
                                }),
                                ..wire::ContinuousColorScale::default()
                            }),
                        },
                    )),
                }),
                non_finite: Some(wire::ColorScaleOutOfRange {
                    policy: wire_clamp(),
                    show_key: Some(false),
                    label: None,
                }),
                ..wire::ColorScale::default()
            },
            wire::ColorScale {
                id: Some(11),
                kind: Some(wire::ColorScaleKind {
                    kind: Some(wire::ColorScaleKindKind::Indexed(
                        wire::ColorScaleKindIndexed {
                            indexed: Some(wire::IndexedColorScale {
                                outside: Some(wire::ColorScaleOutOfRange {
                                    policy: wire_transparent(),
                                    show_key: Some(false),
                                    label: None,
                                }),
                                ..wire::IndexedColorScale::default()
                            }),
                        },
                    )),
                }),
                ..wire::ColorScale::default()
            },
            wire::ColorScale {
                id: Some(12),
                kind: Some(wire::ColorScaleKind {
                    kind: Some(wire::ColorScaleKindKind::Discrete(
                        wire::ColorScaleKindDiscrete {
                            discrete: Some(wire::DiscreteColorScale {
                                levels: Some(wire::ColorScaleLevels {
                                    kind: Some(wire::ColorScaleLevelsKind::Auto(
                                        wire::ColorScaleLevelsAuto {
                                            count: Some(0),
                                            limits: None,
                                        },
                                    )),
                                }),
                                ..wire::DiscreteColorScale::default()
                            }),
                        },
                    )),
                }),
                ..wire::ColorScale::default()
            },
        ],
        // A data array of identifier zero whose name is present and empty, which is a
        // name and not an absence.
        data: vec![wire_data(0, Some(""), wire_floats(vec![0], vec![]))],
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
        layout: TileLayout {
            row_weights: vec![TrackSize::Weight(0.0)],
            ..TileLayout::new(0, 0)
        },
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
            colorbar: None,
            box_: false,
            legend: Some(Legend::Inside(InsideLegend {
                location: LegendLocation::NorthEast,
                boxed: false,
                layout: LegendEntryLayout {
                    columns: LegendEntryColumns::Count(0),
                    order: LegendEntryOrder::ColumnMajor,
                },
                entries: LegendEntries::Explicit(vec![]),
            })),
            artists: vec![
                Artist::Series(Series {
                    id: NodeId(3),
                    visible: false,
                    line: zero_line,
                    marker: zero_marker,
                    ..Series::default()
                }),
                Artist::Series(Series {
                    id: NodeId(4),
                    line: zero_line,
                    marker: zero_marker,
                    size_data: Some(NodeId(0)),
                    color_data: Some(NodeId(0)),
                    ..Series::default()
                }),
                Artist::Contour(Contour {
                    id: NodeId(5),
                    levels: Some(Levels::Auto { count: 0 }),
                    placement: ContourPlacement::Plane { z: Some(0.0) },
                    line: zero_line,
                    line_opacity: 0.0,
                    fill_opacity: 0.0,
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
                    face_opacity: 0.0,
                    edge_opacity: 0.0,
                    ..Surface::default()
                }),
                Artist::Image(Image {
                    id: NodeId(8),
                    placement: ImagePlacement {
                        plane: ImagePlane::Xy { z: Some(0.0) },
                        columns: Some(PixelRange {
                            first: 0.0,
                            last: 0.0,
                        }),
                        rows: Some(PixelRange {
                            first: 0.0,
                            last: 0.0,
                        }),
                    },
                    opacity: 0.0,
                    ..Image::default()
                }),
                // Identifier 0 is a reference like any other, not an absent alpha or scale.
                Artist::ScalarImage(ScalarImage {
                    id: NodeId(9),
                    color_scale: Some(NodeId(0)),
                    placement: ImagePlacement {
                        plane: ImagePlane::Xz { y: Some(0.0) },
                        ..ImagePlacement::default()
                    },
                    alpha: Some(NodeId(0)),
                    opacity: 0.0,
                    ..ScalarImage::default()
                }),
            ],
            ..Axes::default()
        }],
        panels: vec![Panel {
            id: NodeId(10),
            cell: Cell {
                row: 0,
                col: 0,
                row_span: 0,
                col_span: 0,
            },
            align: PanelLocation::NorthEast,
            title: None,
            title_align: TitleAlign::Center,
            content: PanelContent::Legend(PanelLegend {
                boxed: false,
                layout: LegendEntryLayout::default(),
                entries: LegendEntries::Explicit(vec![LegendItem::artist(NodeId(0))]),
            }),
        }],
        data: vec![DataArray {
            id: NodeId(0),
            name: Some(String::new()),
            array: NdArray::vector(vec![]),
        }],
        color_scales: vec![
            ColorScale {
                id: NodeId(0),
                kind: ColorScaleKind::Continuous(ContinuousColorScale {
                    below: ColorScaleOutOfRange {
                        policy: OutOfRange::Transparent,
                        show_key: false,
                        label: None,
                    },
                    ..ContinuousColorScale::default()
                }),
                non_finite: ColorScaleOutOfRange {
                    policy: OutOfRange::Clamp,
                    show_key: false,
                    label: None,
                },
                ..ColorScale::default()
            },
            ColorScale {
                id: NodeId(11),
                kind: ColorScaleKind::Indexed(IndexedColorScale {
                    outside: ColorScaleOutOfRange {
                        policy: OutOfRange::Transparent,
                        show_key: false,
                        label: None,
                    },
                    ..IndexedColorScale::default()
                }),
                ..ColorScale::default()
            },
            ColorScale {
                id: NodeId(12),
                kind: ColorScaleKind::Discrete(DiscreteColorScale {
                    levels: ColorScaleLevels::Auto {
                        count: 0,
                        limits: Limits::Auto,
                    },
                    ..DiscreteColorScale::default()
                }),
                ..ColorScale::default()
            },
        ],
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
// fonts would reload listing the default fonts, an empty title would reload with text,
// and an empty array of bytes, whose payloads are both empty so that only its element
// says that it holds bytes, would reload as the float default.
#[test]
fn empty_strings_and_lists_reload_as_empty_rather_than_as_defaults() {
    let (mut fig, axes, _) = single_line_figure();
    fig.title = Some(Text::new(""));
    fig.provenance = Provenance {
        ironlab_version: String::new(),
        typesetter: String::new(),
        typesetter_version: String::new(),
        fonts: vec![],
    };
    fig.data.push(DataArray {
        id: NodeId(99),
        name: None,
        array: NdArray {
            shape: vec![0],
            values: Values::F64(vec![]),
        },
    });
    fig.data.push(DataArray {
        id: NodeId(98),
        name: None,
        array: NdArray::from_shape_u8(vec![0], vec![]).unwrap(),
    });
    fig.links.push(AxisLink {
        dimension: Dimension::Z,
        axes: vec![],
    });
    let contour = Artist::Contour(Contour {
        id: NodeId(50),
        label: Some(Text::plain("")),
        levels: Some(Levels::Explicit { values: vec![] }),
        ..Contour::default()
    });
    // A listed colormap of no colours, which must not reload as the default named
    // colormap, explicit band edges of none, and an empty category label.
    fig.color_scales.extend([
        ColorScale {
            id: NodeId(60),
            kind: ColorScaleKind::Continuous(ContinuousColorScale {
                colormap: Colormap::Listed { colors: vec![] },
                ..ContinuousColorScale::default()
            }),
            ..ColorScale::default()
        },
        ColorScale {
            id: NodeId(61),
            kind: ColorScaleKind::Discrete(DiscreteColorScale {
                levels: ColorScaleLevels::Explicit { values: vec![] },
                ..DiscreteColorScale::default()
            }),
            ..ColorScale::default()
        },
        ColorScale {
            id: NodeId(62),
            kind: ColorScaleKind::Categorical(CategoricalColorScale {
                categories: vec![ColorCategory {
                    color: Color::WHITE,
                    label: Text::plain(""),
                }],
                ..CategoricalColorScale::default()
            }),
            ..ColorScale::default()
        },
    ]);
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
// IR gives that field in its context: a series without a line or marker style draws the
// default line without markers, and a contour's line is colormapped, although a bare
// line style is automatic. Likewise, a legend whose entries are absent or unset lists its entries
// automatically, a legend item without `full_row` shares its row, and a row size whose
// kind is unset is a weight of one, as a row without a size is.
#[test]
fn unspecified_enums_and_absent_fields_decode_to_the_defaults_of_their_context() {
    let empty_line_style = || wire::LineStyle::default();
    let artist = |kind| wire::Artist { kind: Some(kind) };
    let mut wire_figure = wire::Figure {
        font_set: wire::FontSetId::Unspecified as i32,
        axes: vec![wire::Axes {
            id: Some(5),
            x: Some(wire::Axis {
                scale: wire::Scale::Unspecified as i32,
                limits: Some(wire::Limits { kind: None }),
                ..wire::Axis::default()
            }),
            legend: wire_inside(
                wire::LegendLocation::Unspecified,
                None,
                Some(wire::LegendEntryLayout {
                    columns: Some(wire::LegendEntryColumns { kind: None }),
                    order: wire::LegendEntryOrder::Unspecified as i32,
                }),
            ),
            artists: vec![
                artist(wire::ArtistKind::Series(wire::Series {
                    id: Some(6),
                    x: Some(0),
                    y: Some(0),
                    line: Some(empty_line_style()),
                    marker: Some(wire::MarkerStyle::default()),
                    ..wire::Series::default()
                })),
                // Absent line and marker styles, size data, colour data and scale.
                artist(wire::ArtistKind::Series(wire::Series {
                    id: Some(7),
                    x: Some(0),
                    y: Some(0),
                    line: None,
                    marker: None,
                    ..wire::Series::default()
                })),
                // Absent opacities: isolines and bands opaque.
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
                // Absent alpha and opacities: no alpha arrays, and faces and edges opaque.
                artist(wire::ArtistKind::Surface(wire::Surface {
                    id: Some(9),
                    grid: wire_rectilinear(0, 0),
                    z: Some(0),
                    edge: Some(wire::ColorSpec { kind: None }),
                    ..wire::Surface::default()
                })),
                // An absent placement, an unset plane, an absent colour scale and a wall
                // without an offset.
                artist(wire::ArtistKind::Image(wire::Image {
                    id: Some(10),
                    pixels: Some(0),
                    placement: None,
                    ..wire::Image::default()
                })),
                artist(wire::ArtistKind::ScalarImage(wire::ScalarImage {
                    id: Some(11),
                    values: Some(0),
                    placement: Some(wire::ImagePlacement {
                        plane: Some(wire::ImagePlane { kind: None }),
                        columns: None,
                        rows: None,
                    }),
                    ..wire::ScalarImage::default()
                })),
                artist(wire::ArtistKind::ScalarImage(wire::ScalarImage {
                    id: Some(12),
                    values: Some(0),
                    placement: wire_placement(wire_xz(None), None, None),
                    ..wire::ScalarImage::default()
                })),
            ],
            ..wire::Axes::default()
        }],
        // A legend panel with nothing but its identifier, an unspecified location, no cell
        // and a legend with an empty layout, which takes the defaults of a panel rather
        // than those of an inside legend: centred, boxed, in automatic columns.
        // The second panel's entries are present with an unset kind, and the third's name
        // the automatic variant; both mean the same as absent entries. The fourth panel's
        // content names the legend variant but holds no legend, which is the default
        // legend of a panel, as an inside legend that is absent is the default inside
        // legend.
        panels: vec![
            wire::Panel {
                id: Some(13),
                align: wire::PanelLocation::Unspecified as i32,
                content: wire_panel_legend(wire::PanelLegend {
                    layout: Some(wire::LegendEntryLayout::default()),
                    ..wire::PanelLegend::default()
                }),
                ..wire::Panel::default()
            },
            wire::Panel {
                id: Some(14),
                content: wire_panel_legend(wire::PanelLegend {
                    entries: Some(wire::LegendEntries { kind: None }),
                    ..wire::PanelLegend::default()
                }),
                ..wire::Panel::default()
            },
            wire::Panel {
                id: Some(15),
                content: wire_panel_legend(wire::PanelLegend {
                    entries: Some(wire::LegendEntries {
                        kind: Some(wire::LegendEntriesKind::Auto(wire::LegendEntriesAuto {})),
                    }),
                    ..wire::PanelLegend::default()
                }),
                ..wire::Panel::default()
            },
            wire::Panel {
                id: Some(16),
                content: Some(wire::PanelContent {
                    kind: Some(wire::PanelContentKind::Legend(wire::PanelContentLegend {
                        legend: None,
                    })),
                }),
                ..wire::Panel::default()
            },
        ],
        // Colour scales: one with nothing but its identifier, which is the default
        // continuous scale; a continuous scale whose colormap names an unspecified
        // colormap, whose norm is unspecified, whose below category holds an unset policy
        // and whose above category is absent, both of which take the clamp that is the
        // default of those categories rather than the transparent default of the policy
        // type, and whose non-finite category paints in a fixed colour without a colour;
        // a discrete scale whose band edges have an unset kind and an indexed scale whose
        // outside category is absent, which is strict; and a categorical scale with an
        // absent kind of scale message, which is its default.
        color_scales: vec![
            wire::ColorScale {
                id: Some(20),
                ..wire::ColorScale::default()
            },
            wire::ColorScale {
                id: Some(21),
                kind: Some(wire::ColorScaleKind {
                    kind: Some(wire::ColorScaleKindKind::Continuous(
                        wire::ColorScaleKindContinuous {
                            continuous: Some(wire::ContinuousColorScale {
                                colormap: Some(wire::Colormap {
                                    kind: Some(wire::ColormapKind::Named(wire::ColormapNamed {
                                        name: wire::ColormapName::Unspecified as i32,
                                        reverse: None,
                                    })),
                                }),
                                norm: wire::ColorScaleNorm::Unspecified as i32,
                                below: Some(wire::ColorScaleOutOfRange {
                                    policy: Some(wire::OutOfRange { kind: None }),
                                    show_key: None,
                                    label: None,
                                }),
                                above: None,
                                ..wire::ContinuousColorScale::default()
                            }),
                        },
                    )),
                }),
                non_finite: Some(wire::ColorScaleOutOfRange {
                    policy: wire_policy(wire::OutOfRangeKind::Rgba(wire::OutOfRangeRgba {
                        color: None,
                    })),
                    show_key: None,
                    label: None,
                }),
                ..wire::ColorScale::default()
            },
            wire::ColorScale {
                id: Some(22),
                kind: Some(wire::ColorScaleKind {
                    kind: Some(wire::ColorScaleKindKind::Discrete(
                        wire::ColorScaleKindDiscrete {
                            discrete: Some(wire::DiscreteColorScale {
                                levels: Some(wire::ColorScaleLevels { kind: None }),
                                ..wire::DiscreteColorScale::default()
                            }),
                        },
                    )),
                }),
                ..wire::ColorScale::default()
            },
            wire::ColorScale {
                id: Some(23),
                kind: Some(wire::ColorScaleKind {
                    kind: Some(wire::ColorScaleKindKind::Indexed(
                        wire::ColorScaleKindIndexed {
                            indexed: Some(wire::IndexedColorScale::default()),
                        },
                    )),
                }),
                ..wire::ColorScale::default()
            },
            wire::ColorScale {
                id: Some(24),
                kind: Some(wire::ColorScaleKind {
                    kind: Some(wire::ColorScaleKindKind::Categorical(
                        wire::ColorScaleKindCategorical { categorical: None },
                    )),
                }),
                ..wire::ColorScale::default()
            },
        ],
        // A data array whose name is absent, which is no name.
        data: vec![wire_data(0, None, wire_floats(vec![1], vec![0.5]))],
        // A row size whose kind is unset, which is a weight of one, as a missing size is.
        layout: Some(wire::TileLayout {
            row_weights: vec![wire::TrackSize { kind: None }],
            ..wire::TileLayout::default()
        }),
        ..minimal_wire_figure()
    };
    // Legend items without a `full_row`, a custom item without a label, a line sample
    // without styles and a swatch without a colour.
    inside_mut(&mut wire_figure.axes[0].legend).entries = wire_explicit(vec![
        wire_artist_item(6, None),
        wire_custom_item(
            None,
            wire::LegendSampleKind::Line(wire::LegendSampleLine {
                line: None,
                marker: None,
            }),
            None,
        ),
        wire_custom_item(
            None,
            wire::LegendSampleKind::Swatch(wire::LegendSampleSwatch { color: None }),
            None,
        ),
    ]);

    let decoded = Figure::from_protobuf(&encode_wire(&wire_figure)).unwrap();

    let expected = Figure {
        layout: TileLayout {
            row_weights: vec![TrackSize::Weight(1.0)],
            ..TileLayout::default()
        },
        axes: vec![Axes {
            id: NodeId(5),
            legend: Some(Legend::Inside(InsideLegend {
                entries: LegendEntries::Explicit(vec![
                    LegendItem {
                        content: LegendItemContent::Artist(NodeId(6)),
                        full_row: false,
                    },
                    // An absent label is empty, and absent styles of a line sample are the
                    // defaults of a bare style, as they are for a line artist.
                    LegendItem {
                        content: LegendItemContent::Custom {
                            label: Text::default(),
                            sample: LegendSample::Line {
                                line: LineStyle::default(),
                                marker: MarkerStyle::default(),
                            },
                        },
                        full_row: false,
                    },
                    // An absent colour is black, as for a fixed colour specification.
                    LegendItem {
                        content: LegendItemContent::Custom {
                            label: Text::default(),
                            sample: LegendSample::Swatch {
                                color: Color::BLACK,
                            },
                        },
                        full_row: false,
                    },
                ]),
                ..InsideLegend::default()
            })),
            artists: vec![
                Artist::Series(Series {
                    id: NodeId(6),
                    ..Series::default()
                }),
                Artist::Series(Series {
                    id: NodeId(7),
                    ..Series::default()
                }),
                // Levels that are present with an absent count are automatic levels of the
                // default count, not the absent levels that take the scale's.
                Artist::Contour(Contour {
                    id: NodeId(8),
                    levels: Some(Levels::default()),
                    ..Contour::default()
                }),
                Artist::Surface(Surface {
                    id: NodeId(9),
                    ..Surface::default()
                }),
                Artist::Image(Image {
                    id: NodeId(10),
                    ..Image::default()
                }),
                Artist::ScalarImage(ScalarImage {
                    id: NodeId(11),
                    ..ScalarImage::default()
                }),
                Artist::ScalarImage(ScalarImage {
                    id: NodeId(12),
                    placement: ImagePlacement {
                        plane: ImagePlane::Xz { y: None },
                        ..ImagePlacement::default()
                    },
                    ..ScalarImage::default()
                }),
            ],
            ..Axes::default()
        }],
        panels: [13, 14, 15, 16]
            .map(|id| legend_panel(NodeId(id), Cell::default()))
            .to_vec(),
        data: vec![DataArray {
            id: NodeId(0),
            name: None,
            array: NdArray::vector(vec![0.5]),
        }],
        color_scales: vec![
            ColorScale {
                id: NodeId(20),
                ..ColorScale::default()
            },
            ColorScale {
                id: NodeId(21),
                // A fixed colour that is absent is black, as it is for a fixed colour
                // specification.
                non_finite: ColorScaleOutOfRange::new(OutOfRange::Rgba {
                    color: Color::BLACK,
                }),
                ..ColorScale::default()
            },
            ColorScale {
                id: NodeId(22),
                kind: ColorScaleKind::Discrete(DiscreteColorScale::default()),
                ..ColorScale::default()
            },
            ColorScale {
                id: NodeId(23),
                kind: ColorScaleKind::Indexed(IndexedColorScale::default()),
                ..ColorScale::default()
            },
            ColorScale {
                id: NodeId(24),
                kind: ColorScaleKind::Categorical(CategoricalColorScale::default()),
                ..ColorScale::default()
            },
        ],
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
            wire_artist(wire::ArtistKind::Series(wire::Series {
                id: Some(3),
                x: Some(0),
                y: Some(1),
                ..wire::Series::default()
            })),
            wire_artist(wire::ArtistKind::Series(wire::Series {
                id: Some(4),
                x: Some(0),
                y: Some(1),
                size_data: Some(2),
                color_data: Some(2),
                ..wire::Series::default()
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
            wire_artist(wire::ArtistKind::Image(wire::Image {
                id: Some(8),
                pixels: Some(2),
                placement: wire_placement(wire_xy(None), wire_range(0.0, 1.0), None),
                ..wire::Image::default()
            })),
            wire_artist(wire::ArtistKind::ScalarImage(wire::ScalarImage {
                id: Some(9),
                values: Some(2),
                color_scale: Some(13),
                ..wire::ScalarImage::default()
            })),
            wire_artist(wire::ArtistKind::ScalarImage(wire::ScalarImage {
                id: Some(10),
                values: Some(2),
                color_scale: Some(12),
                placement: wire_placement(wire_yz(None), None, wire_range(0.0, 1.0)),
                ..wire::ScalarImage::default()
            })),
        ];
        wire::Figure {
            // A continuous scale with manual limits, whose bounds have no default, and a
            // categorical scale, whose category's label has none.
            color_scales: vec![
                wire::ColorScale {
                    id: Some(12),
                    kind: Some(wire::ColorScaleKind {
                        kind: Some(wire::ColorScaleKindKind::Continuous(
                            wire::ColorScaleKindContinuous {
                                continuous: Some(wire::ContinuousColorScale {
                                    limits: wire_manual(0.0, 1.0),
                                    ..wire::ContinuousColorScale::default()
                                }),
                            },
                        )),
                    }),
                    ..wire::ColorScale::default()
                },
                wire::ColorScale {
                    id: Some(13),
                    kind: Some(wire::ColorScaleKind {
                        kind: Some(wire::ColorScaleKindKind::Categorical(
                            wire::ColorScaleKindCategorical {
                                categorical: Some(wire::CategoricalColorScale {
                                    categories: vec![wire::ColorCategory {
                                        color: None,
                                        label: wire_text("only", wire::Interpreter::None),
                                    }],
                                    outside: None,
                                }),
                            },
                        )),
                    }),
                    ..wire::ColorScale::default()
                },
            ],
            axes: vec![wire::Axes {
                id: Some(2),
                legend: wire_in_panel(11),
                artists,
                ..wire::Axes::default()
            }],
            links: vec![wire::AxisLink {
                dimension: wire::Dimension::X as i32,
                axes: vec![2],
            }],
            panels: vec![wire::Panel {
                id: Some(11),
                content: wire_panel_legend(wire::PanelLegend {
                    layout: wire_columns(2, wire::LegendEntryOrder::RowMajor),
                    entries: wire_explicit(vec![
                        wire_artist_item(3, None),
                        wire_custom_item(
                            None,
                            wire::LegendSampleKind::Swatch(wire::LegendSampleSwatch {
                                color: None,
                            }),
                            None,
                        ),
                    ]),
                    ..wire::PanelLegend::default()
                }),
                ..wire::Panel::default()
            }],
            data: vec![
                wire_data(0, Some("x"), wire_floats(vec![1], vec![0.0])),
                wire_data(1, None, wire_floats(vec![1], vec![1.0])),
                wire_data(2, None, wire_floats(vec![1], vec![2.0])),
            ],
            layout: Some(wire::TileLayout {
                rows: Some(1),
                cols: Some(2),
                row_weights: vec![wire_weight(1.0)],
                col_weights: vec![wire_fit(), wire_weight(2.0)],
            }),
            ..minimal_wire_figure()
        }
    }

    /// Returns the legend of the only panel.
    fn legend(fig: &mut wire::Figure) -> &mut wire::PanelLegend {
        match fig.panels[0]
            .content
            .as_mut()
            .and_then(|content| content.kind.as_mut())
        {
            Some(wire::PanelContentKind::Legend(content)) => content
                .legend
                .as_mut()
                .expect("the complete figure's panel holds a legend"),
            _ => unreachable!("the complete figure's panel holds a legend"),
        }
    }

    /// Returns the item at the given index of the explicit entries of the only panel.
    fn item(fig: &mut wire::Figure, index: usize) -> &mut wire::LegendItem {
        match legend(fig)
            .entries
            .as_mut()
            .and_then(|entries| entries.kind.as_mut())
        {
            Some(wire::LegendEntriesKind::Explicit(explicit)) => &mut explicit.items[index],
            _ => unreachable!("the complete figure's panel lists explicit entries"),
        }
    }

    /// Returns the custom content of the second item of the only panel.
    fn custom(fig: &mut wire::Figure) -> &mut wire::LegendItemContentCustom {
        match item(fig, 1)
            .content
            .as_mut()
            .and_then(|content| content.kind.as_mut())
        {
            Some(wire::LegendItemContentKind::Custom(custom)) => custom,
            _ => unreachable!("the complete figure's second item is custom"),
        }
    }

    /// Returns the wire artist at the given index of the only axes.
    fn kind(fig: &mut wire::Figure, index: usize) -> &mut wire::ArtistKind {
        fig.axes[0].artists[index]
            .kind
            .as_mut()
            .expect("every artist of the complete figure has a kind")
    }

    /// Returns the continuous scale of the complete figure.
    fn continuous(fig: &mut wire::Figure) -> &mut wire::ContinuousColorScale {
        match fig.color_scales[0]
            .kind
            .as_mut()
            .and_then(|kind| kind.kind.as_mut())
        {
            Some(wire::ColorScaleKindKind::Continuous(kind)) => kind
                .continuous
                .as_mut()
                .expect("the complete figure's first scale is present"),
            _ => unreachable!("the complete figure's first scale is continuous"),
        }
    }

    /// Returns the only category of the categorical scale of the complete figure.
    fn category(fig: &mut wire::Figure) -> &mut wire::ColorCategory {
        match fig.color_scales[1]
            .kind
            .as_mut()
            .and_then(|kind| kind.kind.as_mut())
        {
            Some(wire::ColorScaleKindKind::Categorical(kind)) => {
                &mut kind
                    .categorical
                    .as_mut()
                    .expect("the complete figure's second scale is present")
                    .categories[0]
            }
            _ => unreachable!("the complete figure's second scale is categorical"),
        }
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
    let cases: [(&str, Removal); 44] = [
        ("id", |f| f.id = None),
        // A data array is a node, whose identifier has no default, and an array without
        // values is an empty array only when it says so: an absent array is a faulty
        // writer.
        ("data[1].id", |f| f.data[1].id = None),
        ("data[1].array", |f| f.data[1].array = None),
        // A panel holds a legend or another key only through its content, which has no
        // default: a legend is no likelier than any other key.
        ("panels[0].content", |f| f.panels[0].content = None),
        ("panels[0].content.kind", |f| {
            f.panels[0].content = Some(wire::PanelContent { kind: None });
        }),
        ("axes[0].id", |f| f.axes[0].id = None),
        ("axes[0].artists[0].kind", |f| {
            f.axes[0].artists[0].kind = None
        }),
        ("axes[0].artists[0].series.id", |f| {
            artist!(f, 0, Series).id = None
        }),
        ("axes[0].artists[0].series.x", |f| {
            artist!(f, 0, Series).x = None
        }),
        ("axes[0].artists[0].series.y", |f| {
            artist!(f, 0, Series).y = None
        }),
        // The size and colour data of a series are optional, so only its coordinates are
        // required whatever data it carries.
        ("axes[0].artists[1].series.x", |f| {
            artist!(f, 1, Series).x = None
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
        ("axes[0].artists[5].image.id", |f| {
            artist!(f, 5, Image).id = None
        }),
        ("axes[0].artists[5].image.pixels", |f| {
            artist!(f, 5, Image).pixels = None
        }),
        // A pixel range that is present must hold both centres, as manual limits must
        // hold both bounds: one centre alone places nothing.
        ("axes[0].artists[5].image.placement.columns.first", |f| {
            artist!(f, 5, Image).placement = wire_placement(
                wire_xy(None),
                Some(wire::PixelRange {
                    first: None,
                    last: Some(1.0),
                }),
                None,
            );
        }),
        ("axes[0].artists[6].scalar_image.values", |f| {
            artist!(f, 6, ScalarImage).values = None
        }),
        ("axes[0].artists[7].scalar_image.values", |f| {
            artist!(f, 7, ScalarImage).values = None
        }),
        ("axes[0].artists[7].scalar_image.placement.rows.last", |f| {
            artist!(f, 7, ScalarImage).placement = wire_placement(
                wire_yz(None),
                None,
                Some(wire::PixelRange {
                    first: Some(0.0),
                    last: None,
                }),
            );
        }),
        // A colour scale is a node, whose identifier has no default; the bounds of its
        // manual limits have none, as those of an axis have none; and a category's label,
        // which every key that shows the scale draws, has none.
        ("color_scales[1].id", |f| f.color_scales[1].id = None),
        (
            "color_scales[0].kind.continuous.continuous.limits.manual.min",
            |f| {
                continuous(f).limits = wire_manual(0.0, 1.0).map(|limits| match limits.kind {
                    Some(wire::LimitsKind::Manual(manual)) => wire::Limits {
                        kind: Some(wire::LimitsKind::Manual(wire::LimitsManual {
                            min: None,
                            ..manual
                        })),
                    },
                    _ => unreachable!("manual limits are manual"),
                })
            },
        ),
        (
            "color_scales[0].kind.continuous.continuous.limits.manual.max",
            |f| {
                continuous(f).limits = Some(wire::Limits {
                    kind: Some(wire::LimitsKind::Manual(wire::LimitsManual {
                        min: Some(0.0),
                        max: None,
                    })),
                });
            },
        ),
        (
            "color_scales[1].kind.categorical.categorical.categories[0].label",
            |f| category(f).label = None,
        ),
        ("axes[0].x.ticks.step.step", |f| {
            f.axes[0].x = Some(wire::Axis {
                ticks: Some(wire::Ticks {
                    kind: Some(wire::TicksKind::Step(wire::TicksStep {
                        step: None,
                        origin: Some(0.0),
                    })),
                }),
                ..wire::Axis::default()
            });
        }),
        ("axes[0].y.ticks.step.origin", |f| {
            f.axes[0].y = Some(wire::Axis {
                ticks: Some(wire::Ticks {
                    kind: Some(wire::TicksKind::Step(wire::TicksStep {
                        step: Some(0.5),
                        origin: None,
                    })),
                }),
                ..wire::Axis::default()
            });
        }),
        ("links[0].dimension", |f| {
            f.links[0].dimension = wire::Dimension::Unspecified as i32;
        }),
        ("panels[0].id", |f| f.panels[0].id = None),
        ("axes[0].legend.panel.panel", |f| {
            f.axes[0].legend = Some(wire::Legend {
                kind: Some(wire::LegendKind::Panel(wire::LegendPanel { panel: None })),
            });
        }),
        (
            "panels[0].content.legend.legend.layout.columns.count.count",
            |f| {
                legend(f).layout = Some(wire::LegendEntryLayout {
                    columns: Some(wire::LegendEntryColumns {
                        kind: Some(wire::LegendEntryColumnsKind::Count(
                            wire::LegendEntryColumnsCount { count: None },
                        )),
                    }),
                    order: wire::LegendEntryOrder::RowMajor as i32,
                });
            },
        ),
        // An item lists something only through its content, and an artist item names its
        // artist only through its identifier, of which zero is a valid value.
        (
            "panels[0].content.legend.legend.entries.explicit_items.items[0].content",
            |f| item(f, 0).content = None,
        ),
        (
            "panels[0].content.legend.legend.entries.explicit_items.items[0].content",
            |f| {
                item(f, 0).content = Some(wire::LegendItemContent { kind: None });
            },
        ),
        (
            "panels[0].content.legend.legend.entries.explicit_items.items[0].content.artist.artist",
            |f| {
                item(f, 0).content = Some(wire::LegendItemContent {
                    kind: Some(wire::LegendItemContentKind::Artist(
                        wire::LegendItemContentArtist { artist: None },
                    )),
                });
            },
        ),
        // A custom item has no default sample: a line and a swatch are equally likely.
        (
            "panels[0].content.legend.legend.entries.explicit_items.items[1].content.custom.sample",
            |f| custom(f).sample = None,
        ),
        (
            "panels[0].content.legend.legend.entries.explicit_items.items[1].content.custom.sample",
            |f| custom(f).sample = Some(wire::LegendSample { kind: None }),
        ),
        // A weight that is present as a variant must hold its value: an unset size is a
        // weight of one, but a weight variant without a weight is a faulty writer.
        ("layout.row_weights[0].weight.weight", |f| {
            f.layout.as_mut().unwrap().row_weights[0] = wire::TrackSize {
                kind: Some(wire::TrackSizeKind::Weight(wire::TrackSizeWeight {
                    weight: None,
                })),
            };
        }),
        ("layout.col_weights[1].weight.weight", |f| {
            f.layout.as_mut().unwrap().col_weights[1] = wire::TrackSize {
                kind: Some(wire::TrackSizeKind::Weight(wire::TrackSizeWeight {
                    weight: None,
                })),
            };
        }),
        // The same holds inside a legend in a plot area.
        (
            "axes[0].legend.inside.inside.entries.explicit_items.items[0].content",
            |f| {
                f.axes[0].legend = wire_inside(wire::LegendLocation::NorthEast, None, None);
                inside_mut(&mut f.axes[0].legend).entries = wire_explicit(vec![wire::LegendItem {
                    content: None,
                    full_row: Some(true),
                }]);
            },
        ),
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

// Why: the references and values that the IR itself makes optional (the z data of a
// series or quiver, the size and colour data of a series, the w data of a quiver, the
// colour data and the face and edge
// alpha of a surface, the height of a contour plane, the levels of a contour, the colour
// scale of a colour-mapped artist, the alpha of a scalar image, the title of a colour
// scale and the label of its key, the pixel ranges and plane offset of an image, the group,
// colour data and colour scale of a bar series, the base of stairs, and the colour scale
// and alpha of cells) must not
// become errors or defaults when absent, because a figure without them is complete; and
// the encoder must write them as absent rather than as a zero, which would move the
// image.
#[test]
fn absent_optional_references_decode_as_absent() {
    let (mut fig, axes, _) = single_line_figure();
    let artists = &mut fig
        .axes
        .iter_mut()
        .find(|a| a.id == axes)
        .expect("fixture axes exists")
        .artists;
    artists.push(Artist::Series(Series {
        id: NodeId(60),
        z: None,
        marker: MarkerStyle {
            shape: MarkerShape::Circle,
            ..MarkerStyle::default()
        },
        ..Series::default()
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
    artists.push(Artist::Image(Image {
        id: NodeId(64),
        ..Image::default()
    }));
    artists.push(Artist::ScalarImage(ScalarImage {
        id: NodeId(65),
        placement: ImagePlacement {
            plane: ImagePlane::Xz { y: None },
            ..ImagePlacement::default()
        },
        ..ScalarImage::default()
    }));
    artists.push(Artist::ScalarImage(ScalarImage {
        id: NodeId(66),
        placement: ImagePlacement {
            plane: ImagePlane::Yz { x: None },
            ..ImagePlacement::default()
        },
        ..ScalarImage::default()
    }));
    // A colour scale without a title, whose categories have no labels.
    fig.color_scales.push(ColorScale {
        id: NodeId(67),
        ..ColorScale::default()
    });
    // A bar series without a group, colour data or a colour scale, open stairs without a
    // base, and cells without a colour scale or alpha.
    let artists = &mut fig
        .axes
        .iter_mut()
        .find(|a| a.id == axes)
        .expect("fixture axes exists")
        .artists;
    artists.push(Artist::Bar(Bar {
        id: NodeId(68),
        ..Bar::default()
    }));
    artists.push(Artist::Stairs(Stairs {
        id: NodeId(69),
        ..Stairs::default()
    }));
    artists.push(Artist::Cells(Cells {
        id: NodeId(70),
        ..Cells::default()
    }));

    let wire_figure = wire::Figure::from(&fig);
    for artist in &wire_figure.axes[0].artists {
        let absent = match artist.kind.as_ref().expect("the encoder writes every kind") {
            wire::ArtistKind::Series(series) => {
                series.z.is_none()
                    && series.size_data.is_none()
                    && series.color_data.is_none()
                    && series.color_scale.is_none()
            }
            wire::ArtistKind::Quiver(quiver) => quiver.z.is_none() && quiver.w.is_none(),
            wire::ArtistKind::Surface(surface) => {
                surface.c.is_none()
                    && surface.face_alpha.is_none()
                    && surface.edge_alpha.is_none()
                    && surface.color_scale.is_none()
            }
            wire::ArtistKind::Contour(contour) => {
                contour.levels.is_none()
                    && contour.color_scale.is_none()
                    && matches!(
                        contour.placement.as_ref().and_then(|p| p.kind.as_ref()),
                        Some(wire::ContourPlacementKind::Plane(plane)) if plane.z.is_none()
                    )
            }
            wire::ArtistKind::Image(image) => {
                let placement = image.placement.as_ref();
                placement.is_some_and(|p| p.columns.is_none() && p.rows.is_none())
                    && matches!(
                        plane_kind(placement),
                        Some(wire::ImagePlaneKind::Xy(xy)) if xy.z.is_none()
                    )
            }
            wire::ArtistKind::Bar(bar) => {
                bar.group.is_none() && bar.color_data.is_none() && bar.color_scale.is_none()
            }
            wire::ArtistKind::Stairs(stairs) => stairs.base.is_none(),
            wire::ArtistKind::Cells(cells) => cells.color_scale.is_none() && cells.alpha.is_none(),
            wire::ArtistKind::ScalarImage(image) => {
                image.alpha.is_none()
                    && image.color_scale.is_none()
                    && matches!(
                        plane_kind(image.placement.as_ref()),
                        Some(wire::ImagePlaneKind::Xz(xz)) if xz.y.is_none()
                    ) | matches!(
                        plane_kind(image.placement.as_ref()),
                        Some(wire::ImagePlaneKind::Yz(yz)) if yz.x.is_none()
                    )
            }
        };
        assert!(absent, "the encoder wrote an absent reference: {artist:?}");
    }
    let scale = &wire_figure.color_scales[0];
    assert!(
        scale.title.is_none()
            && scale
                .non_finite
                .as_ref()
                .is_some_and(|category| category.label.is_none()),
        "the encoder wrote an absent title or label: {scale:?}"
    );
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

// Why: the element of an array is an enum like any other, so a value that this build
// does not define must be refused with the array named, rather than read as floats or
// as bytes; either guess would silently recolour every image that used the array.
#[test]
fn an_unknown_array_element_is_an_error_that_names_the_array() {
    let (fig, _, _) = single_line_figure();
    let mut wire_figure = wire::Figure::from(&fig);
    wire_figure.data[0]
        .array
        .as_mut()
        .expect("the encoder writes every array")
        .element = 99;
    let result = Figure::from_protobuf(&encode_wire(&wire_figure));
    assert!(
        matches!(
            &result,
            Err(IrError::Protobuf(ProtobufError::UnknownEnumValue { field, value: 99 }))
                if field == "data[0].array.element"
        ),
        "{result:?}"
    );
}

// Why: an array carries its values in the payload of its element (`values` for floats,
// `u8_values` for bytes) and never in both; a message in which the element and the
// payloads disagree comes from a faulty writer, and reading either payload would draw
// data the writer did not mean. An unspecified element means floats, as in every file
// written before the element type existed, so bytes under an unspecified element are a
// disagreement too. The error must name the array.
#[test]
fn an_array_whose_payloads_disagree_with_its_element_is_rejected() {
    use ironlab_ir::wire::NdArrayElement::{F64, U8, Unspecified};
    let cases = [
        ("floats with both payloads", F64, vec![1.0], vec![1]),
        ("floats with only a byte payload", F64, vec![], vec![1]),
        ("bytes with a float payload", U8, vec![1.0], vec![]),
        ("bytes with both payloads", U8, vec![1.0], vec![1]),
        (
            "unspecified with a byte payload",
            Unspecified,
            vec![],
            vec![1],
        ),
    ];
    for (case, element, values, u8_values) in cases {
        let (fig, _, _) = single_line_figure();
        let mut wire_figure = wire::Figure::from(&fig);
        wire_figure.data[0].array = Some(wire::NdArray {
            shape: vec![1],
            values,
            element: element as i32,
            u8_values,
        });
        let result = Figure::from_protobuf(&encode_wire(&wire_figure));
        assert!(
            matches!(
                &result,
                Err(IrError::Protobuf(ProtobufError::InvalidValue { field, .. }))
                    if field.starts_with("data[0]")
            ),
            "{case}: {result:?}"
        );
    }
}

// ---------------------------------------------------------------------------------
// Stability of field numbers
// ---------------------------------------------------------------------------------

/// A figure encoded by hand, field by field, from the field numbers of the schema: a 3D
/// axes with a view, labelled axes with automatic and manual limits, a legend, a line
/// with an RGBA colour and a marker, explicit legend entries of every kind of item and
/// sample in a legend panel, row and column sizes of every kind, images of both kinds (on
/// each plane, with and without an offset, with a pixel range or none, and with an
/// opacity and an alpha array or none), an indexed and a continuous colour scale that the
/// scalar images name (with every out-of-range policy at some category, a key shown and
/// labelled, a named colormap reversed, manual limits and a logarithmic norm), a surface with alpha arrays
/// and opacities for its faces and its edges, three data arrays (floats with a name and
/// without an element as every writer wrote them before the element type existed, floats
/// holding NaN with the element written, and bytes), a link, provenance, a parameter of
/// every kind and two tags.
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
    // An array of bytes: the element of bytes (field 3, value 2) and the bytes (field 4).
    let byte_array = |shape: &[u64], values: &[u8]| {
        [
            packed_varints(1, shape),
            varint_field(3, 2),
            length_delimited(4, values),
        ]
        .concat()
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

    // The content (field 1) of a legend item that names an artist: the artist variant
    // (field 1) of the LegendItemContent oneof, holding the identifier in field 1.
    let artist_item =
        |artist: u64| length_delimited(1, &length_delimited(1, &varint_field(1, artist)));
    // The content (field 1) of a legend item that stands for no artist: the custom variant
    // (field 2), holding a label (field 1) and a LegendSample (field 2).
    let custom_item = |label: Vec<u8>, sample: Vec<u8>| {
        length_delimited(
            1,
            &length_delimited(
                2,
                &[length_delimited(1, &label), length_delimited(2, &sample)].concat(),
            ),
        )
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
        // Size data (field 9), colour data (field 10) and a colour scale (field 11).
        varint_field(9, 0),
        varint_field(10, 7),
        varint_field(11, 31),
    ]
    .concat();

    // A pixel range: the centres of the first (field 1) and last (field 2) pixels.
    let pixel_range =
        |first: f64, last: f64| [double_field(1, first), double_field(2, last)].concat();
    // An image plane: its oneof variant (xy 1, xz 2, yz 3), holding the offset along the
    // third axis in field 1 when there is one.
    let plane = |variant: u64, offset: Option<f64>| {
        length_delimited(variant, &offset.map_or(vec![], |o| double_field(1, o)))
    };
    // An out-of-range policy: its oneof variant (strict 1, transparent 2, clamp 3, rgba 4),
    // holding the colour of a fixed colour in field 1.
    let policy = |variant: u64, colour_bytes: Option<Vec<u8>>| {
        length_delimited(
            variant,
            &colour_bytes.map_or(vec![], |c| length_delimited(1, &c)),
        )
    };

    // A true-colour image (Artist variant 6): id, name, visible, pixels (data 8), a
    // placement (field 5) on the xy plane (field 1) at z = 1.5 with column centres
    // (field 2) from 0 to 1 and no row range, and an opacity of 0.5 (field 6).
    let image = [
        varint_field(1, 4),
        length_delimited(2, &text("pixels", 2)),
        varint_field(3, 1),
        varint_field(4, 8),
        length_delimited(
            5,
            &[
                length_delimited(1, &plane(1, Some(1.5))),
                length_delimited(2, &pixel_range(0.0, 1.0)),
            ]
            .concat(),
        ),
        double_field(6, 0.5),
    ]
    .concat();
    // A scalar image (Artist variant 9): hidden, values from data 8, through colour scale
    // 30 (field 5), on the xz wall without an offset (placement field 6) with row
    // centres (field 3) mirrored from 3 to 0, the bytes of data 8 as its alpha (field 7)
    // and an opacity of 0.25 (field 8).
    let indexed_image = [
        varint_field(1, 5),
        varint_field(3, 0),
        varint_field(4, 8),
        varint_field(5, 30),
        length_delimited(
            6,
            &[
                length_delimited(1, &plane(2, None)),
                length_delimited(3, &pixel_range(3.0, 0.0)),
            ]
            .concat(),
        ),
        varint_field(7, 8),
        double_field(8, 0.25),
    ]
    .concat();
    // A scalar image (Artist variant 9) of the values of data 7, through colour scale 31,
    // on the yz wall at x = -2 with no pixel ranges, and the values of data 7 as its alpha
    // without an opacity, which is therefore 1.
    let mapped_image = [
        varint_field(1, 6),
        varint_field(3, 1),
        varint_field(4, 7),
        varint_field(5, 31),
        length_delimited(6, &length_delimited(1, &plane(3, Some(-2.0)))),
        varint_field(7, 7),
    ]
    .concat();
    // An out-of-range category of a colour scale: its policy (field 1), whether its key
    // is shown (field 2) and the label of the key (field 3).
    let category = |policy_bytes: Vec<u8>, show_key: Option<bool>, label: Option<Vec<u8>>| {
        [
            length_delimited(1, &policy_bytes),
            show_key.map_or(vec![], |k| varint_field(2, u64::from(k))),
            label.map_or(vec![], |l| length_delimited(3, &l)),
        ]
        .concat()
    };
    // An indexed colour scale (Figure field 16) of identifier 30: its kind (field 3) is
    // the indexed variant (ColorScaleKind field 3), whose ColorScaleKindIndexed holds the
    // IndexedColorScale in field 1: two colours (field 1) and a clamp outside the entries
    // (field 2) with its key shown. Non-finite values (field 4) are a half-transparent red.
    let indexed_scale = [
        varint_field(1, 30),
        length_delimited(
            3,
            &length_delimited(
                3,
                &length_delimited(
                    1,
                    &[
                        length_delimited(1, &colour(0.0, 0.0, 0.0, 1.0)),
                        length_delimited(1, &colour(1.0, 1.0, 1.0, 1.0)),
                        length_delimited(2, &category(policy(3, None), Some(true), None)),
                    ]
                    .concat(),
                ),
            ),
        ),
        length_delimited(
            4,
            &category(policy(4, Some(colour(1.0, 0.0, 0.0, 0.5))), None, None),
        ),
    ]
    .concat();
    // A continuous colour scale of identifier 31 with a title (field 2): its kind is the
    // continuous variant (ColorScaleKind field 1), holding a ContinuousColorScale whose
    // colormap (field 1) is the named variant (Colormap field 1) coolwarm (ColormapName
    // value 6) reversed, whose limits (field 2) are manual from -3 to 4, whose norm (field
    // 3) is logarithmic (value 2), transparent below (field 4) and opaque blue above
    // (field 5) with a labelled key. Non-finite values are strict.
    let continuous_scale = [
        varint_field(1, 31),
        length_delimited(2, &text("$T$", 1)),
        length_delimited(
            3,
            &length_delimited(
                1,
                &length_delimited(
                    1,
                    &[
                        length_delimited(
                            1,
                            &length_delimited(
                                1,
                                &[varint_field(1, 6), varint_field(2, 1)].concat(),
                            ),
                        ),
                        length_delimited(2, &limits_manual(-3.0, 4.0)),
                        varint_field(3, 2),
                        length_delimited(4, &category(policy(2, None), None, None)),
                        length_delimited(
                            5,
                            &category(
                                policy(4, Some(colour(0.0, 0.0, 1.0, 1.0))),
                                Some(true),
                                Some(text("hot", 2)),
                            ),
                        ),
                    ]
                    .concat(),
                ),
            ),
        ),
        length_delimited(4, &category(policy(1, None), None, None)),
    ]
    .concat();
    // A surface (Artist variant 5) on a rectilinear grid (field 4, variant 1) of data 0
    // and 7, of the field of data 7 (field 5), with the alpha of its faces from data 7
    // (field 10) and of its edges from data 0 (field 11), a face opacity of 0.75 (field
    // 12) and an edge opacity of 0 (field 13), which is written and kept although it is
    // zero.
    let surface = [
        varint_field(1, 7),
        length_delimited(
            4,
            &length_delimited(1, &[varint_field(1, 0), varint_field(2, 7)].concat()),
        ),
        varint_field(5, 7),
        varint_field(10, 7),
        varint_field(11, 0),
        double_field(12, 0.75),
        double_field(13, 0.0),
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
        // Legend: inside (oneof field 1), an InsideLegend (field 1) of location best,
        // unboxed, with a layout (field 3) of three columns (LegendEntryColumns field 2,
        // LegendEntryColumnsCount field 1) filled row by row (order field 2, value 2), and
        // entries (field 4) that are an explicit list (oneof field 2) of items (field 1):
        // the image on a row of its own (field 2), then the line without a `full_row`.
        length_delimited(
            11,
            &length_delimited(
                1,
                &length_delimited(
                    1,
                    &[
                        varint_field(1, 9),
                        varint_field(2, 0),
                        length_delimited(
                            3,
                            &[
                                length_delimited(1, &length_delimited(2, &varint_field(1, 3))),
                                varint_field(2, 2),
                            ]
                            .concat(),
                        ),
                        length_delimited(
                            4,
                            &length_delimited(
                                2,
                                &[
                                    length_delimited(
                                        1,
                                        &[artist_item(4), varint_field(2, 1)].concat(),
                                    ),
                                    length_delimited(1, &artist_item(3)),
                                ]
                                .concat(),
                            ),
                        ),
                    ]
                    .concat(),
                ),
            ),
        ),
        // The series is the artist oneof variant 10.
        length_delimited(12, &length_delimited(10, &line)),
        length_delimited(12, &length_delimited(6, &image)),
        length_delimited(12, &length_delimited(9, &indexed_image)),
        length_delimited(12, &length_delimited(9, &mapped_image)),
        length_delimited(12, &length_delimited(5, &surface)),
        // Title aligned right (field 13, value 3), an automatic aspect (field 14, oneof
        // variant 1) and an aspect kept by the limits (field 15, value 2).
        varint_field(13, 3),
        length_delimited(14, &length_delimited(1, &[])),
        varint_field(15, 2),
    ]
    .concat();

    // A map entry of the parameters holds the name in field 1 and a Parameter in field 2,
    // whose oneof variant (numbered 1 to 4) is a message holding the value in field 1.
    let parameter = |name: &str, variant: u64, value: Vec<u8>| {
        length_delimited(
            13,
            &[
                length_delimited(1, name.as_bytes()),
                length_delimited(2, &length_delimited(variant, &value)),
            ]
            .concat(),
        )
    };

    // A LegendSample of the line variant (field 1): a LineStyle (field 1) of an RGBA colour,
    // a width and a dashed line (value 2), and a MarkerStyle (field 2) of a square (value
    // 3) with no face (ColorSpec field 3) and an automatic edge (ColorSpec field 1).
    let line_sample = length_delimited(
        1,
        &[
            length_delimited(
                1,
                &[
                    length_delimited(
                        1,
                        &length_delimited(2, &length_delimited(1, &colour(0.25, 0.5, 0.75, 1.0))),
                    ),
                    double_field(2, 0.5),
                    varint_field(3, 2),
                ]
                .concat(),
            ),
            length_delimited(
                2,
                &[
                    varint_field(1, 3),
                    double_field(2, 4.0),
                    length_delimited(3, &empty(3)),
                    length_delimited(4, &empty(1)),
                ]
                .concat(),
            ),
        ]
        .concat(),
    );
    // A LegendSample of the swatch variant (field 2), holding its colour in field 1.
    let swatch_sample = length_delimited(2, &length_delimited(1, &colour(0.0, 0.5, 0.25, 0.5)));

    // PanelLegend: boxed (field 1), a layout (field 2) of automatic columns
    // (LegendEntryColumns field 1) filled column by column, and entries (field 3): an
    // explicit list (oneof field 2) of a custom line sample, then a custom swatch on a row
    // of its own.
    let panel_legend = [
        varint_field(1, 1),
        length_delimited(2, &[empty(1), varint_field(2, 1)].concat()),
        length_delimited(
            3,
            &length_delimited(
                2,
                &[
                    length_delimited(
                        1,
                        &custom_item(text("Separation $x_s$", 1), line_sample.clone()),
                    ),
                    length_delimited(
                        1,
                        &[
                            custom_item(text("Lower surface", 2), swatch_sample.clone()),
                            varint_field(2, 1),
                        ]
                        .concat(),
                    ),
                ]
                .concat(),
            ),
        ),
    ]
    .concat();

    // A DataArray (Figure field 9): its identifier (field 1), its array (field 2) and,
    // when it has one, its name (field 3).
    let data_entry = |id: u64, array: Vec<u8>, name: Option<&str>| {
        length_delimited(
            9,
            &[
                varint_field(1, id),
                length_delimited(2, &array),
                name.map_or(vec![], |name| length_delimited(3, name.as_bytes())),
            ]
            .concat(),
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
        // TileLayout: rows, columns, row sizes (field 5) and column sizes (field 6), each
        // a TrackSize message whose oneof holds a weight (field 1, TrackSizeWeight field
        // 1) or a fitted size (field 2). The second column size is an empty message, whose
        // unset oneof is a weight of one.
        length_delimited(
            8,
            &[
                varint_field(1, 2),
                varint_field(2, 3),
                length_delimited(5, &length_delimited(1, &double_field(1, 2.0))),
                length_delimited(5, &empty(2)),
                length_delimited(6, &length_delimited(1, &double_field(1, 0.5))),
                length_delimited(6, &[]),
                length_delimited(6, &length_delimited(1, &double_field(1, 1.5))),
            ]
            .concat(),
        ),
        // Written without an element, as every writer before the element type existed,
        // and with a name.
        data_entry(0, array(&[3], &[0.0, 1.0, 2.0]), Some("t (s)")),
        // Written with the element of floats (field 3, value 1), as this build writes it.
        data_entry(
            7,
            [array(&[3], &[1.0, f64::NAN, 0.135]), varint_field(3, 1)].concat(),
            None,
        ),
        data_entry(8, byte_array(&[2, 2], &[0, 1, 254, 255]), None),
        length_delimited(16, &indexed_scale),
        length_delimited(16, &continuous_scale),
        length_delimited(10, &axes),
        // A second axes whose legend (field 11) lists its entries in a panel (oneof field
        // 2, LegendPanel field 1).
        length_delimited(
            10,
            &[
                varint_field(1, 21),
                length_delimited(11, &length_delimited(2, &varint_field(1, 20))),
            ]
            .concat(),
        ),
        length_delimited(11, &[varint_field(1, 3), packed_varints(2, &[2])].concat()),
        length_delimited(
            12,
            &[
                length_delimited(1, b"0.1.0"),
                length_delimited(2, b"latex-rust"),
                length_delimited(3, b"STIX Two Text"),
                length_delimited(3, b"STIX Two Math"),
                length_delimited(4, b"1.0.2"),
            ]
            .concat(),
        ),
        // Out of order, as another writer may produce them.
        parameter("solver", 4, length_delimited(1, "k–ω SST".as_bytes())),
        parameter("reynolds_number", 3, double_field(1, 1.0e5)),
        // An int64 is a varint of its two's complement, ten bytes for a negative value.
        parameter("mesh_cells", 2, varint_field(1, (-4_096i64) as u64)),
        parameter("converged", 1, varint_field(1, 1)),
        // Tags are a sequence, so they are written and read in the order given.
        length_delimited(14, "boundary layer".as_bytes()),
        length_delimited(14, "k–ω".as_bytes()),
        // Panel (Figure field 15): identifier, cell, align south-west (PanelLocation value
        // 9), and its content (field 4): the legend variant (PanelContent field 1), whose
        // PanelContentLegend holds the PanelLegend in field 1.
        length_delimited(
            15,
            &[
                varint_field(1, 20),
                length_delimited(
                    2,
                    &[
                        varint_field(1, 1),
                        varint_field(2, 0),
                        varint_field(3, 1),
                        varint_field(4, 3),
                    ]
                    .concat(),
                ),
                varint_field(3, 9),
                length_delimited(4, &length_delimited(1, &length_delimited(1, &panel_legend))),
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
        layout: TileLayout {
            rows: 2,
            cols: 3,
            row_weights: vec![TrackSize::Weight(2.0), TrackSize::Fit],
            col_weights: vec![
                TrackSize::Weight(0.5),
                TrackSize::Weight(1.0),
                TrackSize::Weight(1.5),
            ],
        },
        data: vec![
            DataArray {
                id: NodeId(0),
                name: Some("t (s)".to_owned()),
                array: NdArray::vector(vec![0.0, 1.0, 2.0]),
            },
            DataArray {
                id: NodeId(7),
                name: None,
                array: NdArray::vector(vec![1.0, f64::NAN, 0.135]),
            },
            DataArray {
                id: NodeId(8),
                name: None,
                array: NdArray::from_shape_u8(vec![2, 2], vec![0, 1, 254, 255]).unwrap(),
            },
        ],
        color_scales: vec![
            ColorScale {
                id: NodeId(30),
                title: None,
                kind: ColorScaleKind::Indexed(IndexedColorScale {
                    colors: vec![Color::BLACK, Color::WHITE],
                    outside: ColorScaleOutOfRange {
                        policy: OutOfRange::Clamp,
                        show_key: true,
                        label: None,
                    },
                    ..IndexedColorScale::default()
                }),
                non_finite: ColorScaleOutOfRange::new(OutOfRange::Rgba {
                    color: Color::rgba(1.0, 0.0, 0.0, 0.5),
                }),
            },
            ColorScale {
                id: NodeId(31),
                title: Some(Text::new("$T$")),
                kind: ColorScaleKind::Continuous(ContinuousColorScale {
                    colormap: Colormap::Named {
                        name: ColormapName::Coolwarm,
                        reverse: true,
                    },
                    limits: Limits::Manual {
                        min: -3.0,
                        max: 4.0,
                    },
                    norm: ColorScaleNorm::Log,
                    below: ColorScaleOutOfRange::new(OutOfRange::Transparent),
                    above: ColorScaleOutOfRange {
                        policy: OutOfRange::Rgba {
                            color: Color::rgba(0.0, 0.0, 1.0, 1.0),
                        },
                        show_key: true,
                        label: Some(Text::plain("hot")),
                    },
                    ..ContinuousColorScale::default()
                }),
                non_finite: ColorScaleOutOfRange::new(OutOfRange::Strict),
            },
        ],
        axes: vec![
            Axes {
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
                title_align: TitleAlign::Right,
                aspect: Aspect::Auto,
                aspect_adjust: AspectAdjust::Limits,
                x: Axis {
                    label: Some(Text::new("$t$ (s)")),
                    scale: Scale::Linear,
                    limits: Limits::Auto,
                    grid: false,
                    ..Axis::default()
                },
                y: Axis {
                    label: None,
                    scale: Scale::Log,
                    limits: Limits::Manual { min: 0.1, max: 1.0 },
                    grid: true,
                    ..Axis::default()
                },
                z: Axis {
                    label: Some(Text::plain("z")),
                    scale: Scale::Linear,
                    limits: Limits::Manual {
                        min: -1.0,
                        max: 2.0,
                    },
                    grid: false,
                    ..Axis::default()
                },
                colorbar: None,
                box_: false,
                legend: Some(Legend::Inside(InsideLegend {
                    location: LegendLocation::Best,
                    boxed: false,
                    layout: LegendEntryLayout {
                        columns: LegendEntryColumns::Count(3),
                        order: LegendEntryOrder::RowMajor,
                    },
                    entries: LegendEntries::Explicit(vec![
                        LegendItem {
                            full_row: true,
                            ..LegendItem::artist(NodeId(4))
                        },
                        LegendItem::artist(NodeId(3)),
                    ]),
                })),
                artists: vec![
                    Artist::Series(Series {
                        id: NodeId(3),
                        label: Some(Text::plain("decay, $5")),
                        visible: true,
                        x: NodeId(0),
                        y: NodeId(7),
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
                        size_data: Some(NodeId(0)),
                        color_data: Some(NodeId(7)),
                        color_scale: Some(NodeId(31)),
                        x_coords: Coords::Data,
                        y_coords: Coords::Data,
                        clip: true,
                    }),
                    Artist::Image(Image {
                        id: NodeId(4),
                        label: Some(Text::plain("pixels")),
                        visible: true,
                        pixels: NodeId(8),
                        placement: ImagePlacement {
                            plane: ImagePlane::Xy { z: Some(1.5) },
                            columns: Some(PixelRange {
                                first: 0.0,
                                last: 1.0,
                            }),
                            rows: None,
                        },
                        opacity: 0.5,
                    }),
                    Artist::ScalarImage(ScalarImage {
                        id: NodeId(5),
                        label: None,
                        visible: false,
                        values: NodeId(8),
                        color_scale: Some(NodeId(30)),
                        placement: ImagePlacement {
                            plane: ImagePlane::Xz { y: None },
                            columns: None,
                            rows: Some(PixelRange {
                                first: 3.0,
                                last: 0.0,
                            }),
                        },
                        alpha: Some(NodeId(8)),
                        opacity: 0.25,
                    }),
                    Artist::ScalarImage(ScalarImage {
                        id: NodeId(6),
                        label: None,
                        visible: true,
                        values: NodeId(7),
                        color_scale: Some(NodeId(31)),
                        placement: ImagePlacement {
                            plane: ImagePlane::Yz { x: Some(-2.0) },
                            columns: None,
                            rows: None,
                        },
                        alpha: Some(NodeId(7)),
                        opacity: 1.0,
                    }),
                    Artist::Surface(Surface {
                        id: NodeId(7),
                        grid: Grid::Rectilinear {
                            x: NodeId(0),
                            y: NodeId(7),
                        },
                        z: NodeId(7),
                        face_alpha: Some(NodeId(7)),
                        edge_alpha: Some(NodeId(0)),
                        face_opacity: 0.75,
                        edge_opacity: 0.0,
                        ..Surface::default()
                    }),
                ],
            },
            Axes {
                id: NodeId(21),
                legend: Some(Legend::Panel(NodeId(20))),
                ..Axes::default()
            },
        ],
        links: vec![AxisLink {
            dimension: Dimension::Z,
            axes: vec![NodeId(2)],
        }],
        provenance: Provenance {
            ironlab_version: "0.1.0".to_owned(),
            typesetter: "latex-rust".to_owned(),
            typesetter_version: "1.0.2".to_owned(),
            fonts: vec!["STIX Two Text".to_owned(), "STIX Two Math".to_owned()],
        },
        parameters: BTreeMap::from([
            ("converged".to_owned(), Parameter::Bool(true)),
            ("mesh_cells".to_owned(), Parameter::Integer(-4_096)),
            ("reynolds_number".to_owned(), Parameter::Number(1.0e5)),
            ("solver".to_owned(), Parameter::String("k–ω SST".to_owned())),
        ]),
        tags: vec!["boundary layer".to_owned(), "k–ω".to_owned()],
        panels: vec![Panel {
            id: NodeId(20),
            cell: Cell {
                row: 1,
                col: 0,
                row_span: 1,
                col_span: 3,
            },
            align: PanelLocation::SouthWest,
            title: None,
            title_align: TitleAlign::Left,
            content: PanelContent::Legend(PanelLegend {
                boxed: true,
                layout: LegendEntryLayout {
                    columns: LegendEntryColumns::Auto,
                    order: LegendEntryOrder::ColumnMajor,
                },
                entries: LegendEntries::Explicit(vec![
                    LegendItem::custom(
                        LegendSample::Line {
                            line: LineStyle {
                                color: ColorSpec::Rgba {
                                    color: Color::rgba(0.25, 0.5, 0.75, 1.0),
                                },
                                width_pt: 0.5,
                                dash: DashStyle::Dashed,
                            },
                            marker: MarkerStyle {
                                shape: MarkerShape::Square,
                                size_pt: 4.0,
                                face: ColorSpec::None,
                                edge: ColorSpec::Auto,
                            },
                        },
                        Text::new("Separation $x_s$"),
                    ),
                    LegendItem {
                        full_row: true,
                        ..LegendItem::custom(
                            LegendSample::swatch(Color::rgba(0.0, 0.5, 0.25, 0.5)),
                            Text::plain("Lower surface"),
                        )
                    },
                ]),
            }),
        }],
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
    let Some(wire::ArtistKind::Series(wire_line)) = wire_artist.kind else {
        panic!("fixture holds a series");
    };

    // Rebuild the nesting figure → axes (field 10) → artist (field 12) → series (field 10),
    // appending unknown fields at every level. Field order within a message does not
    // affect decoding.
    let line = [wire_line.encode_to_vec(), unknown_fields()].concat();
    let artist = [length_delimited(10, &line), unknown_fields()].concat();
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

// Why: files from an incompatible schema (a different major or minor version, such as
// the 0.2.0 of the release before the image kinds) or with a version that cannot be
// compared must be rejected with a version error rather than misread, and the error
// must name the file's version and the supported one, so that the user knows which
// build to use.
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
        let error = Figure::from_protobuf(&fig.to_protobuf())
            .err()
            .unwrap_or_else(|| panic!("version {version:?} was accepted"));
        assert!(
            matches!(
                &error,
                IrError::IncompatibleSchemaVersion { found, supported }
                    if *found == version && *supported == SCHEMA_VERSION
            ),
            "version {version:?} gave {error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains(&version) && message.contains(SCHEMA_VERSION),
            "the message must name both versions: {message}"
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

// Why: patch releases of the schema are compatible by definition, and in both
// directions: a file of an earlier patch release lacks only fields that this build can
// do without, and a file of a later one carries only fields that this build may ignore.
// Both must load, and each must keep the version that it declares.
#[test]
fn an_earlier_or_later_patch_version_is_accepted() {
    let [major, minor, patch] = supported_version();
    for version in [
        format!("{major}.{minor}.0"),
        format!("{major}.{minor}.{}", patch + 42),
    ] {
        let fig = Figure {
            schema_version: version,
            ..kitchen_sink_figure()
        };
        assert_eq!(Figure::from_protobuf(&fig.to_protobuf()).unwrap(), fig);
    }
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
        artists: vec![Artist::Series(Series {
            id: line,
            x,
            y,
            ..Series::default()
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

// Why: the element type exists so that images are stored compactly; each 8-bit value
// must cost one byte in a `bytes` payload rather than a varint (two bytes for every
// value above 127) or a double, so an array of bytes that spans the whole range stays
// within 1.01 bytes per value including the figure's structure.
#[test]
fn large_byte_arrays_encode_at_one_byte_per_value() {
    const PIXELS: usize = 100_000;
    let mut fig = Figure::new();
    let values: Vec<u8> = (0..PIXELS).map(|i| (i % 256) as u8).collect();
    fig.add_data(NdArray::from_shape_u8(vec![250, 400], values).unwrap());

    let bytes = fig.to_protobuf();
    let per_value = bytes.len() as f64 / PIXELS as f64;
    assert!(
        per_value <= 1.01,
        "{} bytes for {PIXELS} values is {per_value:.3} bytes per value",
        bytes.len()
    );
}
