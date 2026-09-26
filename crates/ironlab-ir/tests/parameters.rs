//! Figure parameters: named values that describe a figure, used to sort, filter and
//! search collections of figures. These tests cover their persistence in both
//! encodings, the determinism of the encodings, their JSON form, the decoding of values
//! absent from the wire, and their JSON Schema.
//!
//! Their validation is tested in `validate.rs`, and the stability of their field numbers
//! in `protobuf.rs`.

mod common;

use std::collections::BTreeMap;

use common::{raw_fields, read_varint, single_line_figure};
use ironlab_ir::*;
use prost::Message;
use serde_json::{Value, json};

/// Returns a figure holding the given parameters and otherwise a single line.
fn figure_with(parameters: &[(&str, Parameter)]) -> Figure {
    let (mut fig, _, _) = single_line_figure();
    fig.parameters = parameters
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect();
    fig
}

/// Parameters at the edges of what each kind holds: both booleans, the extremes of the
/// integer range and zero, integers that a double does not hold exactly, negative zero,
/// a subnormal and the extremes of the finite range, an integral number (which must not
/// become an integer), empty and non-ASCII strings, and names that differ only in case,
/// contain whitespace or are not ASCII.
fn edge_parameters() -> Vec<(&'static str, Parameter)> {
    vec![
        ("true", Parameter::Bool(true)),
        ("false", Parameter::Bool(false)),
        ("i64 min", Parameter::Integer(i64::MIN)),
        ("i64 max", Parameter::Integer(i64::MAX)),
        ("zero", Parameter::Integer(0)),
        ("2^53 + 1", Parameter::Integer((1 << 53) + 1)),
        ("i64 max - 1", Parameter::Integer(i64::MAX - 1)),
        ("negative zero", Parameter::Number(-0.0)),
        ("subnormal", Parameter::Number(f64::from_bits(1))),
        ("f64 max", Parameter::Number(f64::MAX)),
        ("f64 min", Parameter::Number(f64::MIN)),
        ("integral number", Parameter::Number(3.0)),
        ("empty string", Parameter::String(String::new())),
        ("Solver", Parameter::String("k–ω SST".to_owned())),
        (
            "solver",
            Parameter::String("\u{1F30A} \"quoted\"\n".to_owned()),
        ),
        ("Reynolds-Zahl Re", Parameter::Number(1.0e5)),
        ("数", Parameter::Integer(-7)),
    ]
}

/// Returns every parameter as its name and a description of its kind and exact value,
/// in which a number is described by its bits, so that a comparison detects a change of
/// kind (an integral number read back as an integer) and a change of the sign of zero or
/// of a NaN payload, which the equality of `f64` does not.
fn exact_parameters(fig: &Figure) -> Vec<(String, String)> {
    fig.parameters
        .iter()
        .map(|(name, value)| {
            let exact = match value {
                Parameter::Bool(value) => format!("bool {value}"),
                Parameter::Integer(value) => format!("integer {value}"),
                Parameter::Number(value) => format!("number {:#018x}", value.to_bits()),
                Parameter::String(value) => format!("string {value:?}"),
            };
            (name.clone(), exact)
        })
        .collect()
}

// ---------------------------------------------------------------------------------
// Round trips
// ---------------------------------------------------------------------------------

// Why: parameters are what collections of figures are sorted and searched by, so a saved
// `.fig` must reload every parameter as exactly the value that was written: an integer at
// the extremes of its range must not be truncated, negative zero must keep its sign, and
// names must keep their case, whitespace and non-ASCII characters.
#[test]
fn parameters_survive_a_protobuf_round_trip_exactly() {
    let fig = figure_with(&edge_parameters());
    let restored = Figure::from_protobuf(&fig.to_protobuf()).unwrap();
    assert_eq!(restored, fig);
    assert_eq!(exact_parameters(&restored), exact_parameters(&fig));
}

// Why: the binary format stores IEEE 754 bits, so even a non-finite number (which
// validation rejects, because JSON cannot hold it) must not be altered when it is saved,
// so that a figure being repaired reloads as it was.
#[test]
fn non_finite_number_parameters_survive_a_protobuf_round_trip_bit_for_bit() {
    let fig = figure_with(&[
        (
            "nan with payload",
            Parameter::Number(f64::from_bits(0x7ff8_dead_beef_0001)),
        ),
        ("negative nan", Parameter::Number(-f64::NAN)),
        ("infinity", Parameter::Number(f64::INFINITY)),
        ("negative infinity", Parameter::Number(f64::NEG_INFINITY)),
    ]);
    let restored = Figure::from_protobuf(&fig.to_protobuf()).unwrap();
    assert_eq!(exact_parameters(&restored), exact_parameters(&fig));
}

// Why: JSON is the secondary format, used by other tools and hand-written files. It does
// not distinguish integers from numbers, so the representation must, or a number such as
// 3.0 would reload as an integer and sort differently from the figures beside it; and
// every value that JSON can hold must reload exactly, including the extremes of the
// integer range (beyond the 2^53 that a double holds exactly) and negative zero.
#[test]
fn parameters_survive_a_json_round_trip_exactly() {
    let fig = figure_with(&edge_parameters());
    let json = fig.to_json();
    let restored = Figure::from_json(&json).unwrap();
    assert_eq!(restored, fig);
    assert_eq!(exact_parameters(&restored), exact_parameters(&fig));
    assert_eq!(restored.to_json(), json, "saving again changes the text");
}

// Why: JSON cannot represent a non-finite number, so it is written as `null`. Reading that
// `null` must fail rather than substitute NaN (as a data array does), because a parameter
// that silently changed would put the figure in the wrong place when a collection is
// sorted; validation reports non-finite parameters so that this is found before saving.
#[test]
fn a_non_finite_number_parameter_cannot_be_reloaded_from_json() {
    let fig = figure_with(&[("reynolds_number", Parameter::Number(f64::NAN))]);
    assert!(matches!(
        Figure::from_json(&fig.to_json()),
        Err(IrError::Json(_))
    ));
}

// Why: the two encodings must describe the same figure, so converting a `.fig` holding
// parameters to JSON and back must reproduce the same bytes.
#[test]
fn parameters_convert_from_protobuf_to_json_and_back_to_the_same_bytes() {
    let bytes = figure_with(&edge_parameters()).to_protobuf();
    let json = Figure::from_protobuf(&bytes).unwrap().to_json();
    assert_eq!(Figure::from_json(&json).unwrap().to_protobuf(), bytes);
}

// ---------------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------------

// Why: equal figures must encode to the same bytes and the same text, so that
// version-controlled files do not churn and figures can be deduplicated by their
// encoding. A protobuf map and a JSON object have no inherent order, so the order in
// which parameters were set must not leak into either encoding. The ordered map of the
// model guarantees this today; the test guards against a change to a map that keeps
// insertion order, and it is the only test of the order of keys in JSON.
#[test]
fn encodings_do_not_depend_on_the_order_in_which_parameters_are_set() {
    let parameters = edge_parameters();
    let mut forwards = figure_with(&[]);
    for (name, value) in &parameters {
        forwards
            .parameters
            .insert((*name).to_owned(), value.clone());
    }
    let mut backwards = figure_with(&[]);
    for (name, value) in parameters.iter().rev() {
        backwards
            .parameters
            .insert((*name).to_owned(), value.clone());
    }
    assert_eq!(forwards.to_protobuf(), backwards.to_protobuf());
    assert_eq!(forwards.to_json(), backwards.to_json());
}

// Why: readers in other languages see the map entries in the order in which they are
// written, and a documented order makes files comparable byte for byte, so the entries
// must be written in ascending order of the UTF-8 bytes of the names.
#[test]
fn parameters_are_written_in_ascending_order_of_name() {
    let fig = figure_with(&edge_parameters());
    let wire_figure = wire::Figure::decode(fig.to_protobuf().as_slice()).unwrap();
    // Decoding into a map would reorder the entries, so read the raw field 13 entries.
    let written: Vec<String> = raw_parameter_names(&fig.to_protobuf());
    let mut sorted = written.clone();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    assert_eq!(written, sorted);
    assert_eq!(written.len(), wire_figure.parameters.len());
}

/// Returns the names of the parameter map entries (field 13 of `Figure`) in the order in
/// which they appear in the bytes.
fn raw_parameter_names(bytes: &[u8]) -> Vec<String> {
    raw_fields(bytes, 13)
        .iter()
        .map(|entry| {
            let mut entry = entry.as_slice();
            let key = read_varint(&mut entry);
            assert_eq!(
                key,
                (1 << 3) | 2,
                "the name is the first field of the entry"
            );
            let len = read_varint(&mut entry) as usize;
            String::from_utf8(entry[..len].to_vec()).expect("a name is UTF-8")
        })
        .collect()
}

// ---------------------------------------------------------------------------------
// The JSON form
// ---------------------------------------------------------------------------------

// Why: the JSON form of parameters is a public contract for other tools and hand-written
// files. Parameters are an object keyed by name, and each value is tagged with its kind
// in `snake_case`, beside a `value` property that holds a JSON boolean, integer, number or
// string, so that the kind survives JSON's single number type.
#[test]
fn parameters_are_written_as_an_object_of_tagged_values() {
    let fig = figure_with(&[
        ("converged", Parameter::Bool(true)),
        ("mesh_cells", Parameter::Integer(-4_096)),
        ("reynolds_number", Parameter::Number(1.0e5)),
        ("solver", Parameter::String("k–ω SST".to_owned())),
    ]);
    let value: Value = serde_json::from_str(&fig.to_json()).unwrap();
    assert_eq!(
        value["parameters"],
        json!({
            "converged": { "type": "bool", "value": true },
            "mesh_cells": { "type": "integer", "value": -4096 },
            "reynolds_number": { "type": "number", "value": 100000.0 },
            "solver": { "type": "string", "value": "k–ω SST" },
        })
    );
    assert!(value["parameters"]["mesh_cells"]["value"].is_i64());
    assert!(value["parameters"]["reynolds_number"]["value"].is_f64());
}

// Why: a hand-written file states a number as JSON allows, often without a decimal
// point, and the tag, not the lexical form of the value, decides the kind.
#[test]
fn a_hand_written_number_without_a_decimal_point_loads_as_a_number() {
    let mut value: Value = serde_json::from_str(&Figure::new().to_json()).unwrap();
    value["parameters"] = json!({
        "count": { "type": "integer", "value": 12 },
        "ratio": { "type": "number", "value": 2 },
    });
    let fig = Figure::from_json(&value.to_string()).unwrap();
    assert_eq!(fig.parameters["count"], Parameter::Integer(12));
    assert_eq!(fig.parameters["ratio"], Parameter::Number(2.0));
}

// Why: JSON does not order the properties of an object, so a hand-written file may put
// `value` before `type`. Such a parameter must load as the same value, including an
// integer that a double does not hold exactly, which a reader that buffered the value as
// a double before seeing the tag would corrupt.
#[test]
fn a_parameter_whose_value_precedes_its_type_loads_exactly() {
    let mut value: Value = serde_json::from_str(&Figure::new().to_json()).unwrap();
    value["parameters"] = json!({});
    let text = value.to_string().replace(
        r#""parameters":{}"#,
        r#""parameters":{"big":{"value":9007199254740993,"type":"integer"},"ratio":{"value":0.5,"type":"number"}}"#,
    );
    assert!(
        text.contains("9007199254740993"),
        "the document was not edited"
    );
    let fig = Figure::from_json(&text).unwrap();
    assert_eq!(
        fig.parameters["big"],
        Parameter::Integer(9_007_199_254_740_993)
    );
    assert_eq!(fig.parameters["ratio"], Parameter::Number(0.5));
}

// Why: a tag that does not match its value is a faulty file, and guessing a kind (or
// saturating an integer beyond the range of i64) would change how the figure sorts; it
// must be an error.
#[test]
fn a_value_that_does_not_match_its_tag_is_rejected() {
    for parameter in [
        json!({ "type": "integer", "value": 1.5 }),
        json!({ "type": "integer", "value": 9_223_372_036_854_775_808u64 }),
        json!({ "type": "bool", "value": 1 }),
        json!({ "type": "string", "value": 1 }),
        json!({ "type": "number", "value": "1" }),
        json!({ "type": "decimal", "value": 1 }),
        json!({ "value": 1 }),
    ] {
        let mut value: Value = serde_json::from_str(&Figure::new().to_json()).unwrap();
        value["parameters"] = json!({ "p": parameter.clone() });
        assert!(
            matches!(Figure::from_json(&value.to_string()), Err(IrError::Json(_))),
            "{parameter} was accepted"
        );
    }
}

// Why: most figures have no parameters, and a property that is always empty only adds
// noise to every file; a figure without parameters must write no `parameters` property,
// and a file without the property must load with no parameters.
#[test]
fn a_figure_without_parameters_writes_no_parameters_property_and_reloads_without_them() {
    let fig = Figure::new();
    let value: Value = serde_json::from_str(&fig.to_json()).unwrap();
    assert!(
        value.get("parameters").is_none(),
        "an empty parameters property was written"
    );
    let restored = Figure::from_json(&fig.to_json()).unwrap();
    assert!(restored.parameters.is_empty());
}

// Why: see the previous test; in protobuf, an empty map must add no bytes at all, so that
// a figure without parameters encodes exactly as it would if the schema had no
// parameters field, apart from its schema version.
#[test]
fn a_figure_without_parameters_writes_no_parameter_bytes() {
    let fig = figure_with(&[]);
    assert!(raw_parameter_names(&fig.to_protobuf()).is_empty());
    let mut with_one = fig.clone();
    with_one
        .parameters
        .insert("p".to_owned(), Parameter::Bool(false));
    assert_eq!(raw_parameter_names(&with_one.to_protobuf()), ["p"]);
}

// ---------------------------------------------------------------------------------
// Absent values on the wire
// ---------------------------------------------------------------------------------

/// Returns the bytes of a figure whose only parameter is the given wire parameter.
fn with_wire_parameter(name: &str, parameter: wire::Parameter) -> Vec<u8> {
    let mut wire_figure = wire::Figure::from(&Figure::new());
    wire_figure.parameters = BTreeMap::from([(name.to_owned(), parameter)]);
    wire_figure.encode_to_vec()
}

// Why: a parameter has no default kind or value: substituting `false`, zero or an empty
// string would put the figure in the wrong place when a collection is sorted or
// filtered. An absent kind, and an absent boolean, integer or number, must therefore be
// errors that name the parameter, so that the author of a faulty writer can find it.
#[test]
fn absent_parameter_kinds_and_values_are_errors_that_name_the_parameter() {
    let cases = [
        (wire::Parameter { kind: None }, r#"parameters["Re"].kind"#),
        (
            wire::Parameter {
                kind: Some(wire::ParameterKind::Bool(wire::ParameterBool {
                    value: None,
                })),
            },
            r#"parameters["Re"].bool_value.value"#,
        ),
        (
            wire::Parameter {
                kind: Some(wire::ParameterKind::Integer(wire::ParameterInteger {
                    value: None,
                })),
            },
            r#"parameters["Re"].integer_value.value"#,
        ),
        (
            wire::Parameter {
                kind: Some(wire::ParameterKind::Number(wire::ParameterNumber {
                    value: None,
                })),
            },
            r#"parameters["Re"].number_value.value"#,
        ),
    ];
    for (parameter, field) in cases {
        let result = Figure::from_protobuf(&with_wire_parameter("Re", parameter));
        assert!(
            matches!(
                &result,
                Err(IrError::Protobuf(ProtobufError::MissingField { field: found })) if found == field
            ),
            "expected {field} to be missing, got {result:?}"
        );
    }
}

// Why: a string has no presence in proto3, so an empty string value is indistinguishable
// from an absent one and must load as the empty string that the writer most likely meant,
// rather than being rejected.
#[test]
fn an_absent_string_value_loads_as_the_empty_string() {
    let parameter = wire::Parameter {
        kind: Some(wire::ParameterKind::String(wire::ParameterString {
            value: String::new(),
        })),
    };
    let fig = Figure::from_protobuf(&with_wire_parameter("note", parameter)).unwrap();
    assert_eq!(fig.parameters["note"], Parameter::String(String::new()));
}

// ---------------------------------------------------------------------------------
// JSON Schema
// ---------------------------------------------------------------------------------

// Why: other tools validate `.fig.json` files against the generated JSON Schema, so the
// schema must describe parameters where a reader of the figure module looks for them,
// and must not require the property that a figure without parameters omits.
#[test]
fn the_json_schema_declares_optional_parameters_in_the_figure_module() {
    let files: BTreeMap<String, Value> = json_schema_files()
        .into_iter()
        .map(|(path, text)| {
            (
                path.display().to_string(),
                serde_json::from_str(&text).unwrap(),
            )
        })
        .collect();
    let figure = &files["figure.schema.json"];
    assert!(
        figure["$defs"].get("Parameter").is_some(),
        "Parameter is not defined in figure.schema.json"
    );
    assert!(figure["properties"].get("parameters").is_some());
    let required = figure["required"]
        .as_array()
        .expect("a figure has required properties");
    assert!(
        !required.contains(&json!("parameters")),
        "parameters must not be required"
    );
    assert!(required.contains(&json!("provenance")));
}
