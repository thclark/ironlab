//! The wire formats of transactions: lossless Protocol Buffers and JSON round trips of
//! every kind of edit and every value, and the errors for malformed messages.

mod common;

use common::edits::{
    VALUE_VARIANTS, every_kind_transaction, rows, sample_values, set, tx, value_variants_in,
};
use common::{SPECIAL_F64, float_bits, floats, kitchen_sink_figure};
use ironlab_ir::*;
use prost::Message;

/// Decodes a transaction from its own Protocol Buffers encoding.
fn protobuf_round_trip(transaction: &Transaction) -> Transaction {
    Transaction::from_protobuf(&transaction.to_protobuf())
        .unwrap_or_else(|e| panic!("the encoding does not decode: {e:?}"))
}

/// Decodes a wire transaction built by hand, bypassing the domain encoder.
fn decode_wire(wire: wire::Transaction) -> Result<Transaction, IrError> {
    Transaction::from_protobuf(&wire.encode_to_vec())
}

/// A wire transaction holding one edit.
fn wire_tx(kind: wire::EditKind) -> wire::Transaction {
    wire::Transaction {
        edits: vec![wire::Edit { kind: Some(kind) }],
    }
}

/// A wire set of node 1 at `x.limits` with the given value.
fn wire_set(value: Option<wire::Value>) -> wire::EditKind {
    wire::EditKind::Set(wire::EditSet {
        node: Some(1),
        path: "x.limits".to_owned(),
        value,
    })
}

fn wire_value(kind: wire::ValueKind) -> Option<wire::Value> {
    Some(wire::Value { kind: Some(kind) })
}

/// A valid wire array of one value, so that only the field under test is missing.
fn one_value() -> wire::NdArray {
    wire::NdArray {
        shape: vec![1],
        values: vec![1.0],
        element: wire::NdArrayElement::F64 as i32,
        u8_values: vec![],
    }
}

/// Asserts that decoding fails because the field at the given path is missing.
#[track_caller]
fn assert_missing(result: Result<Transaction, IrError>, field: &str) {
    assert!(
        matches!(&result, Err(IrError::Protobuf(ProtobufError::MissingField { field: f })) if f == field),
        "expected {field} to be missing, got {result:?}"
    );
}

// Why: the samples used by the round-trip tests must contain a value of every variant, so
// that a variant whose encoding is lost or wrong cannot go untested; adding a variant to
// `Value` stops the tests compiling until it is sampled.
#[test]
fn the_sample_transaction_sets_a_value_of_every_variant() {
    assert_eq!(sample_values().len(), VALUE_VARIANTS);
    assert_eq!(
        value_variants_in(&every_kind_transaction()),
        (0..VALUE_VARIANTS).collect()
    );
}

// Why: sessions, browser viewers and clients in other languages exchange transactions as
// Protocol Buffers, so every kind of edit and every value, with identifiers above 2^53
// and NaN in arrays, must decode to the transaction that was encoded, and encode again to
// the same bytes.
#[test]
fn every_kind_of_edit_and_value_round_trips_through_protobuf() {
    let original = every_kind_transaction();
    let decoded = protobuf_round_trip(&original);
    assert_eq!(decoded, original);
    assert_eq!(decoded.to_protobuf(), original.to_protobuf());
}

// Why: JSON is the debugging and simple-web format of transactions, and must carry the same
// edits and values, including identifiers above 2^53 that JavaScript numbers cannot hold.
#[test]
fn every_kind_of_edit_and_value_round_trips_through_json() {
    let original = every_kind_transaction();
    let decoded = Transaction::from_json(&original.to_json())
        .unwrap_or_else(|e| panic!("the JSON does not parse: {e:?}"));
    assert_eq!(decoded, original);
}

// Why: the viewer and a session exchange edits of limits, views and data whose values may be
// negative zero, NaN with a payload, infinities or subnormals; undo and mirroring rely on
// every bit arriving intact.
#[test]
fn special_floats_in_values_and_arrays_round_trip_through_protobuf_bit_for_bit() {
    let mut edits: Vec<Edit> = SPECIAL_F64
        .iter()
        .map(|&v| set(NodeId(1), "projection.view3d.zoom", Value::Double(v)))
        .collect();
    edits.extend(
        [f32::NAN, -0.0, f32::from_bits(1), f32::INFINITY]
            .map(|v| set(NodeId(1), "background.r", Value::Float(v))),
    );
    edits.push(set(
        NodeId(1),
        "levels.values",
        Value::Doubles(SPECIAL_F64.to_vec()),
    ));
    edits.push(set(
        NodeId(1),
        "x.limits",
        Value::Limits(Limits::Manual {
            min: -0.0,
            max: SPECIAL_F64[2],
        }),
    ));
    edits.push(Edit::PutData {
        id: DataId(1),
        array: NdArray::vector(SPECIAL_F64.to_vec()),
    });
    let original = tx(edits);
    let bytes = original.to_protobuf();
    let decoded = Transaction::from_protobuf(&bytes).unwrap();
    assert_eq!(decoded.to_protobuf(), bytes);
    for (edit, &expected) in decoded.edits.iter().zip(&SPECIAL_F64) {
        let Edit::Set {
            value: Value::Double(v),
            ..
        } = edit
        else {
            panic!("the edit is a set of a double: {edit:?}");
        };
        assert_eq!(v.to_bits(), expected.to_bits());
    }
}

// Why: undo in a session sends the inverse of a transaction to mirrors, and the inverse of
// replacing or removing an array holds the old array, which may contain NaN with a payload,
// negative zero and infinities; the inverse must reach a mirror intact, so that applying it
// there restores the figure bit for bit.
#[test]
fn an_inverse_that_restores_special_floats_in_data_survives_protobuf() {
    let mut fig = kitchen_sink_figure();
    let replaced = DataId(1000);
    let removed = DataId(1001);
    fig.data
        .insert(replaced, NdArray::vector(SPECIAL_F64.to_vec()));
    fig.data.insert(
        removed,
        NdArray::vector(SPECIAL_F64.iter().rev().copied().collect()),
    );
    let before = float_bits(&fig);

    let mut mirror = fig.clone();
    let transaction = tx([
        Edit::PutData {
            id: replaced,
            array: NdArray::vector(vec![0.0; SPECIAL_F64.len()]),
        },
        Edit::RemoveData { id: removed },
    ]);
    let inverse = fig.apply(&transaction).unwrap();
    mirror
        .apply(&Transaction::from_protobuf(&transaction.to_protobuf()).unwrap())
        .unwrap();
    mirror
        .apply(&Transaction::from_protobuf(&inverse.to_protobuf()).unwrap())
        .unwrap();
    assert_eq!(float_bits(&mirror), before);
}

// Why: JSON must at least keep negative zero, which it can represent, and read non-finite
// array values back as NaN, as the figure format does.
#[test]
fn negative_zero_and_nan_in_arrays_survive_json() {
    let original = tx([
        set(NodeId(1), "projection.view3d.pan_x", Value::Double(-0.0)),
        Edit::PutData {
            id: DataId(1),
            array: NdArray::vector(vec![-0.0, f64::NAN, f64::INFINITY]),
        },
    ]);
    let decoded = Transaction::from_json(&original.to_json()).unwrap();
    let [
        Edit::Set {
            value: Value::Double(pan),
            ..
        },
        Edit::PutData { array, .. },
    ] = decoded.edits.as_slice()
    else {
        panic!("unexpected edits {decoded:?}");
    };
    assert_eq!(pan.to_bits(), (-0.0f64).to_bits());
    let values = floats(array);
    assert_eq!(values[0].to_bits(), (-0.0f64).to_bits());
    assert!(values[1].is_nan() && values[2].is_nan());
}

// Why: a web client that sends an image's pixels writes the array in the documented
// form, an `element` tag with integer values, and must get the same array back when the
// transaction is read; the float arrays of other edits keep their untagged form, which
// `the_json_form_of_an_edit_is_tagged_and_uses_dotted_paths` pins.
#[test]
fn an_array_of_bytes_in_an_edit_is_tagged_with_its_element_in_json() {
    let bytes = NdArray::from_shape_u8(vec![2], vec![0, 255]).unwrap();
    let transaction = tx([Edit::PutData {
        id: DataId(5),
        array: bytes.clone(),
    }]);
    let json: serde_json::Value = serde_json::from_str(&transaction.to_json()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "edits": [{
                "type": "put_data",
                "id": 5,
                "array": {"shape": [2], "element": "u8", "values": [0, 255]}
            }]
        })
    );
    let decoded = Transaction::from_json(&json.to_string()).unwrap();
    assert_eq!(decoded, transaction);
}

// Why: web clients build transactions as JSON by hand, so the documented form (edits
// tagged by `type`, paths as dotted strings, values tagged by `type` with a `value`) is a
// contract.
#[test]
fn the_json_form_of_an_edit_is_tagged_and_uses_dotted_paths() {
    let transaction = tx([
        set(
            NodeId(3),
            "x.limits",
            Value::Limits(Limits::Manual { min: 0.0, max: 1.0 }),
        ),
        set(NodeId(3), "title", Value::Unset),
        Edit::AppendData {
            id: DataId(2),
            array: rows(1, 2, 0.0),
            retain: Some(100),
        },
    ]);
    let json: serde_json::Value = serde_json::from_str(&transaction.to_json()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "edits": [
                {
                    "type": "set",
                    "node": 3,
                    "path": "x.limits",
                    "value": {"type": "limits", "value": {"type": "manual", "min": 0.0, "max": 1.0}}
                },
                {"type": "set", "node": 3, "path": "title", "value": {"type": "unset"}},
                {
                    "type": "append_data",
                    "id": 2,
                    "array": {"shape": [1, 2], "values": [0.0, 1.0]},
                    "retain": 100
                }
            ]
        })
    );
}

// Why: a path in JSON that is not a valid path must be refused when parsing, rather than
// produce an edit that can never apply.
#[test]
fn an_invalid_path_in_json_is_refused() {
    let json = r#"{"edits": [{"type": "set", "node": 3, "path": "x..limits", "value": {"type": "unset"}}]}"#;
    assert!(matches!(
        Transaction::from_json(json),
        Err(IrError::Json(_))
    ));
}

// Why: an edit has no context from which to take a default, so a message that omits the
// kind of an edit, of a value or of a node, an identifier an edit names, the value of a
// set, the node of an insertion, the array of a data edit, or the value held by a value
// message must be refused with the path of the missing field, like a malformed figure.
#[test]
fn a_transaction_missing_a_required_field_is_refused_with_its_path() {
    assert_missing(
        decode_wire(wire::Transaction {
            edits: vec![wire::Edit { kind: None }],
        }),
        "edits[0].kind",
    );
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::Set(wire::EditSet {
            node: None,
            path: "x.limits".to_owned(),
            value: wire_value(wire::ValueKind::Unset(wire::ValueUnset {})),
        }))),
        "edits[0].set_property.node",
    );
    assert_missing(
        decode_wire(wire_tx(wire_set(None))),
        "edits[0].set_property.value",
    );
    assert_missing(
        decode_wire(wire_tx(wire_set(Some(wire::Value { kind: None })))),
        "edits[0].set_property.value.kind",
    );
    assert_missing(
        decode_wire(wire_tx(wire_set(wire_value(wire::ValueKind::Double(
            wire::ValueDouble { value: None },
        ))))),
        "edits[0].set_property.value.double_value.value",
    );
    assert_missing(
        decode_wire(wire_tx(wire_set(wire_value(wire::ValueKind::Limits(
            wire::ValueLimits { value: None },
        ))))),
        "edits[0].set_property.value.limits_value.value",
    );
    assert_missing(
        decode_wire(wire_tx(wire_set(wire_value(wire::ValueKind::Interpreter(
            wire::ValueInterpreter { value: 0 },
        ))))),
        "edits[0].set_property.value.interpreter_value.value",
    );
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::Insert(wire::EditInsert {
            parent: Some(1),
            index: None,
            node: None,
        }))),
        "edits[0].insert_node.node",
    );
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::Insert(wire::EditInsert {
            parent: Some(1),
            index: None,
            node: Some(wire::Node { kind: None }),
        }))),
        "edits[0].insert_node.node.kind",
    );
    // The node of this insertion is a complete axes encoded by IronLAB, so that only the
    // parent is missing.
    let mut insertion = wire::Transaction::decode(
        tx([Edit::Insert {
            parent: NodeId(1),
            index: None,
            node: Node::Axes(Box::new(kitchen_sink_figure().axes[0].clone())),
        }])
        .to_protobuf()
        .as_slice(),
    )
    .expect("IronLAB's encoding decodes");
    let Some(wire::EditKind::Insert(insert)) = &mut insertion.edits[0].kind else {
        panic!("the edit is an insertion");
    };
    insert.parent = None;
    assert_missing(decode_wire(insertion), "edits[0].insert_node.parent");
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::Remove(wire::EditRemove {
            node: None,
        }))),
        "edits[0].remove_node.node",
    );
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::Move(wire::EditMove {
            node: None,
            parent: Some(2),
            index: Some(0),
        }))),
        "edits[0].move_node.node",
    );
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::Move(wire::EditMove {
            node: Some(2),
            parent: None,
            index: Some(0),
        }))),
        "edits[0].move_node.parent",
    );
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::PutData(wire::EditPutData {
            id: None,
            array: Some(one_value()),
        }))),
        "edits[0].put_data.id",
    );
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::AppendData(wire::EditAppendData {
            id: Some(1),
            array: None,
            retain: Some(3),
        }))),
        "edits[0].append_data.array",
    );
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::AppendData(wire::EditAppendData {
            id: None,
            array: Some(one_value()),
            retain: None,
        }))),
        "edits[0].append_data.id",
    );
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::PutData(wire::EditPutData {
            id: Some(1),
            array: None,
        }))),
        "edits[0].put_data.array",
    );
    assert_missing(
        decode_wire(wire::Transaction {
            edits: vec![
                wire::Edit {
                    kind: Some(wire::EditKind::Remove(wire::EditRemove { node: Some(1) })),
                },
                wire::Edit {
                    kind: Some(wire::EditKind::RemoveData(wire::EditRemoveData {
                        id: None,
                    })),
                },
            ],
        }),
        "edits[1].remove_data.id",
    );
}

// Why: a node inside an insertion is decoded with the rules of the figure format, so its
// missing identifier is refused, with a path that leads into the node.
#[test]
fn an_inserted_node_is_decoded_with_the_rules_of_the_figure_format() {
    let line = wire::Line {
        id: None,
        x: Some(0),
        y: Some(1),
        ..wire::Line::default()
    };
    assert_missing(
        decode_wire(wire_tx(wire::EditKind::Insert(wire::EditInsert {
            parent: Some(1),
            index: None,
            node: Some(wire::Node {
                kind: Some(wire::NodeKind::Artist(wire::Artist {
                    kind: Some(wire::ArtistKind::Line(line)),
                })),
            }),
        }))),
        "edits[0].insert_node.node.artist.line.id",
    );
}

// Why: an enum value this build does not define can only come from a faulty writer or
// another version, and must be refused rather than read as a default.
#[test]
fn an_unknown_enum_value_in_a_value_is_refused() {
    let result = decode_wire(wire_tx(wire_set(wire_value(wire::ValueKind::Scale(
        wire::ValueScale { value: 99 },
    )))));
    assert!(
        matches!(
            &result,
            Err(IrError::Protobuf(ProtobufError::UnknownEnumValue { field, value: 99 }))
                if field == "edits[0].set_property.value.scale_value.value"
        ),
        "{result:?}"
    );
}

// Why: a path string that is not a valid path (including the empty string, which is how an
// absent path decodes) must be refused when decoding.
#[test]
fn an_invalid_path_on_the_wire_is_refused() {
    for text in ["", "x..limits"] {
        let result = decode_wire(wire_tx(wire::EditKind::Set(wire::EditSet {
            node: Some(1),
            path: text.to_owned(),
            value: wire_value(wire::ValueKind::Unset(wire::ValueUnset {})),
        })));
        assert!(
            matches!(
                &result,
                Err(IrError::Protobuf(ProtobufError::InvalidValue { field, .. }))
                    if field == "edits[0].set_property.path"
            ),
            "{text:?}: {result:?}"
        );
    }
}

// Why: a later patch release may add fields to transactions; a reader must skip fields it
// does not know, as it does for figures.
#[test]
fn unknown_fields_in_a_transaction_are_skipped() {
    let original = tx([set(NodeId(7), "visible", Value::Bool(false))]);
    let mut bytes = original.to_protobuf();
    // Field 1000 as a varint holding 42.
    bytes.extend([0xc0, 0x3e, 42]);
    assert_eq!(Transaction::from_protobuf(&bytes).unwrap(), original);
}
