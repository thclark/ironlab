//! Numeric arrays: missing values, the element type, JSON representation, equality and
//! shape checks.

use ironlab_ir::*;
use proptest::prelude::*;
use serde_json::json;

/// An array of 8-bit values with the given shape.
fn bytes(shape: Vec<usize>, values: Vec<u8>) -> NdArray {
    NdArray::from_shape_u8(shape, values).expect("the shape matches the values")
}

/// The values of an `f64` array.
fn floats(array: &NdArray) -> &[f64] {
    array.as_f64().expect("the array holds f64 values")
}

// Why: NaN marks missing data (gaps in lines, holes in surfaces) and JSON has no NaN, so
// the documented representation is `null`.
#[test]
fn nan_is_written_as_null() {
    let array = NdArray::vector(vec![1.5, f64::NAN, -2.0]);
    let value = serde_json::to_value(&array).unwrap();
    assert_eq!(value, json!({ "shape": [3], "values": [1.5, null, -2.0] }));
}

// Why: a `null` written by us or by another tool must come back as a missing value.
#[test]
fn null_is_read_as_nan() {
    let array: NdArray =
        serde_json::from_value(json!({ "shape": [2], "values": [null, 4.0] })).unwrap();
    assert_eq!(array.shape, vec![2]);
    assert!(floats(&array)[0].is_nan());
    assert_eq!(floats(&array)[1], 4.0);
}

// Why: serialising infinities with a plain JSON writer would fail or emit invalid JSON;
// the documented behaviour is that every non-finite value is stored as missing.
#[test]
fn infinities_are_written_as_null_and_read_as_nan() {
    let array = NdArray::vector(vec![f64::INFINITY, f64::NEG_INFINITY]);
    let value = serde_json::to_value(&array).unwrap();
    assert_eq!(value["values"], json!([null, null]));
    let restored: NdArray = serde_json::from_value(value).unwrap();
    assert!(floats(&restored).iter().all(|v| v.is_nan()));
}

// Why: values that are not numbers or null are corrupt data and must be rejected rather
// than coerced.
#[test]
fn non_numeric_values_are_rejected() {
    let result: Result<NdArray, _> =
        serde_json::from_value(json!({ "shape": [1], "values": ["1.0"] }));
    assert!(result.is_err());
}

// Why: figure equality underpins the persistence tests, and arrays with missing values
// must compare equal to themselves even though NaN != NaN.
#[test]
fn arrays_with_nan_in_the_same_places_are_equal() {
    let a = NdArray::vector(vec![1.0, f64::NAN]);
    assert_eq!(a, a.clone());
    assert_ne!(a, NdArray::vector(vec![1.0, 2.0]));
    let reshaped = NdArray {
        shape: vec![1, 2],
        values: a.values.clone(),
    };
    assert_ne!(a, reshaped);
}

// Why: a shape whose element count differs from the number of values would make every
// consumer index out of bounds, so the checked constructor must refuse it.
#[test]
fn from_shape_rejects_a_mismatched_value_count() {
    assert!(NdArray::from_shape(vec![2, 3], vec![0.0; 6]).is_ok());
    assert!(matches!(
        NdArray::from_shape(vec![2, 3], vec![0.0; 5]),
        Err(IrError::InvalidShape { len: 5, .. })
    ));
}

// ---------------------------------------------------------------------------------
// The element type
// ---------------------------------------------------------------------------------

// Why: images hold pixels as bytes, and an array of bytes has the same shape rules as
// an array of floats; the checked constructor must count bytes against the shape as
// `from_shape` counts floats, or a consumer would index past the end of the bytes.
#[test]
fn from_shape_u8_rejects_a_mismatched_byte_count() {
    assert!(NdArray::from_shape_u8(vec![2, 3], vec![0; 6]).is_ok());
    assert!(matches!(
        NdArray::from_shape_u8(vec![2, 3], vec![0; 5]),
        Err(IrError::InvalidShape { shape, len: 5 }) if shape == [2, 3]
    ));
}

// Why: every consumer branches on the element type to reach the values, so the
// accessors must report it faithfully: the typed slice of the other element is absent
// rather than converted, `len` and `is_empty` count elements of either type, and `get`
// widens a byte to the float it denotes so that a datatip can show either kind of
// value without knowing which it holds.
#[test]
fn accessors_report_the_element_type_and_widen_bytes_to_floats() {
    let byte_array = bytes(vec![3], vec![0, 128, 255]);
    assert_eq!(byte_array.element(), NdArrayElement::U8);
    assert_eq!(byte_array.len(), 3);
    assert!(!byte_array.is_empty());
    assert_eq!(byte_array.as_u8(), Some(&[0, 128, 255][..]));
    assert_eq!(byte_array.as_f64(), None);
    assert_eq!(byte_array.get(1), Some(128.0));
    assert_eq!(byte_array.get(2), Some(255.0));
    assert_eq!(byte_array.get(3), None);

    let float_array = NdArray::vector(vec![1.5, f64::NAN]);
    assert_eq!(float_array.element(), NdArrayElement::F64);
    assert_eq!(floats(&float_array).len(), 2);
    assert_eq!(floats(&float_array)[0], 1.5);
    assert!(floats(&float_array)[1].is_nan());
    assert_eq!(float_array.as_u8(), None);
    assert_eq!(float_array.get(0), Some(1.5));
    assert!(float_array.get(1).is_some_and(f64::is_nan));
    assert_eq!(float_array.get(2), None);

    let empty = bytes(vec![0], vec![]);
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());
    assert_eq!(empty.element(), NdArrayElement::U8);
}

// Why: `NdArray::default()` is what the wire decoder and the edit protocol start from
// when nothing else is given, and every file written before the element type existed
// holds floats, so the default must be an empty array of floats.
#[test]
fn the_default_array_is_an_empty_float_vector() {
    let array = NdArray::default();
    assert_eq!(array.element(), NdArrayElement::F64);
    assert!(array.is_empty());
    assert_eq!(array, NdArray::vector(vec![]));
}

// Why: figure equality decides whether a reloaded figure matches the saved one, and an
// image of bytes drawn from a byte 1 is not the same data as a float 1.0 (it is
// coloured by a different rule), so arrays of different element types must never
// compare equal, while byte arrays compare by shape and value like float arrays.
#[test]
fn arrays_of_different_element_types_are_not_equal() {
    let byte_one = bytes(vec![1], vec![1]);
    assert_ne!(byte_one, NdArray::vector(vec![1.0]));
    assert_ne!(NdArray::vector(vec![1.0]), byte_one);
    assert_eq!(byte_one, byte_one.clone());
    assert_eq!(bytes(vec![2], vec![1, 2]), bytes(vec![2], vec![1, 2]));
    assert_ne!(bytes(vec![2], vec![1, 2]), bytes(vec![2], vec![1, 3]));
    assert_ne!(bytes(vec![2], vec![1, 2]), bytes(vec![1, 2], vec![1, 2]));
    assert_ne!(bytes(vec![0], vec![]), NdArray::vector(vec![]));
}

// Why: the JSON form of an array is a public contract read by other tools. Bytes are
// written as JSON integers (not `1.0`, which a reader in another language would parse
// as a float) under an `element` tag that names the type, so that a reader knows how to
// interpret the values before it reads them.
#[test]
fn an_eight_bit_array_is_written_with_its_element_tag_and_integer_values() {
    let value = serde_json::to_value(bytes(vec![1, 2], vec![1, 255])).unwrap();
    assert_eq!(
        value,
        json!({ "shape": [1, 2], "element": "u8", "values": [1, 255] })
    );
    assert!(
        value["values"][0].is_u64(),
        "a byte must be written as an integer: {}",
        value["values"][0]
    );
}

// Why: every `.fig.json` written before the element type existed has no `element`
// key, and such files must keep loading and must not change when saved again; the
// absence of the tag therefore means `f64`, and `f64` arrays are written without it.
#[test]
fn a_float_array_is_written_without_an_element_tag_and_read_without_one() {
    let value = serde_json::to_value(NdArray::vector(vec![1.5, 2.0])).unwrap();
    assert_eq!(value, json!({ "shape": [2], "values": [1.5, 2.0] }));
    assert!(value.get("element").is_none());

    let array: NdArray = serde_json::from_value(json!({ "shape": [2], "values": [1, 2] })).unwrap();
    assert_eq!(array.element(), NdArrayElement::F64);
    assert_eq!(array, NdArray::vector(vec![1.0, 2.0]));
}

// Why: a writer in another language may name the element type of a float array
// explicitly, or write the tag as `null` (as a serialiser of an optional field does),
// and both must mean exactly what the absence of the tag means.
#[test]
fn an_explicit_or_null_f64_element_tag_is_accepted() {
    for tag in [json!("f64"), json!(null)] {
        let array: NdArray = serde_json::from_value(
            json!({ "shape": [3], "element": tag, "values": [1, null, 2.5] }),
        )
        .unwrap();
        assert_eq!(array.element(), NdArrayElement::F64, "element {tag}");
        assert_eq!(array, NdArray::vector(vec![1.0, f64::NAN, 2.5]));
    }
}

// Why: an array tagged `u8` must load as bytes, not as floats that happen to be whole
// numbers, because the element type decides how an image artist colours its pixels. A
// writer whose JSON library prints whole numbers with a decimal point (as Python prints
// a float) still means bytes, so an integral float is accepted as the byte it denotes;
// and an empty array of bytes, which has no value to tell its type by, must keep its
// element through a round trip.
#[test]
fn an_array_tagged_u8_is_read_as_bytes() {
    let array: NdArray = serde_json::from_value(
        json!({ "shape": [2, 2], "element": "u8", "values": [0, 1, 254, 255] }),
    )
    .unwrap();
    assert_eq!(array.element(), NdArrayElement::U8);
    assert_eq!(array, bytes(vec![2, 2], vec![0, 1, 254, 255]));

    let integral: NdArray =
        serde_json::from_value(json!({ "shape": [2], "element": "u8", "values": [1.0, 255.0] }))
            .unwrap();
    assert_eq!(integral, bytes(vec![2], vec![1, 255]));

    let empty = bytes(vec![0], vec![]);
    let restored: NdArray = serde_json::from_value(serde_json::to_value(&empty).unwrap()).unwrap();
    assert_eq!(restored.element(), NdArrayElement::U8);
    assert_eq!(restored, empty);
}

// Why: a byte has no missing value and no fraction, and cannot hold a value outside
// 0..=255, so `null`, a fractional number, an out-of-range number and a string in a
// `u8` array are corrupt data that must be rejected rather than truncated, wrapped or
// coerced into a pixel value the author never wrote.
#[test]
fn values_that_are_not_bytes_are_rejected_for_an_eight_bit_array() {
    for bad in [json!(null), json!(1.5), json!(256), json!(-1), json!("1")] {
        let result: Result<NdArray, _> =
            serde_json::from_value(json!({ "shape": [1], "element": "u8", "values": [bad] }));
        assert!(result.is_err(), "{bad} was accepted as a byte");
    }
}

// Why: an element type that this build does not define cannot be interpreted; a file
// from a later version that adds one must be refused rather than read as floats.
#[test]
fn an_unknown_element_name_is_rejected() {
    for bad in [json!("u16"), json!("float"), json!("U8"), json!(8)] {
        let result: Result<NdArray, _> =
            serde_json::from_value(json!({ "shape": [1], "element": bad, "values": [1] }));
        assert!(result.is_err(), "the element {bad} was accepted");
    }
}

// Why: pixel data must not drift through save and load; every one of the 256 byte
// values must reload as itself, in an array of more than one dimension.
#[test]
fn every_byte_value_round_trips_through_json() {
    let values: Vec<u8> = (0..=255).collect();
    let array = bytes(vec![16, 16], values.clone());
    let text = serde_json::to_string(&array).unwrap();
    let restored: NdArray = serde_json::from_str(&text).unwrap();
    assert_eq!(restored.shape, vec![16, 16]);
    assert_eq!(restored.as_u8(), Some(values.as_slice()));
    assert_eq!(restored, array);
}

fn value_with_non_finite() -> impl Strategy<Value = f64> {
    prop_oneof![
        4 => any::<f64>(),
        1 => Just(f64::NAN),
        1 => Just(f64::INFINITY),
        1 => Just(f64::NEG_INFINITY),
        1 => Just(-0.0),
        1 => Just(f64::MIN_POSITIVE / 3.0),
    ]
}

proptest! {
    // Why: plotted data must not drift through save and load; finite values (including
    // extremes, subnormals and negative zero) must be bit-exact, and every non-finite
    // value must become a missing value.
    #[test]
    fn array_values_round_trip_through_json(values in prop::collection::vec(value_with_non_finite(), 0..64)) {
        let array = NdArray::vector(values.clone());
        let text = serde_json::to_string(&array).unwrap();
        let restored: NdArray = serde_json::from_str(&text).unwrap();
        prop_assert_eq!(&restored.shape, &array.shape);
        prop_assert_eq!(restored.element(), NdArrayElement::F64);
        prop_assert_eq!(restored.len(), values.len());
        for (original, restored) in values.iter().zip(floats(&restored)) {
            if original.is_finite() {
                prop_assert_eq!(original.to_bits(), restored.to_bits());
            } else {
                prop_assert!(restored.is_nan());
            }
        }
    }

    // Why: bytes have no representation problem in JSON, so an array of bytes of any
    // length, including none, must reload exactly and keep its element type.
    #[test]
    fn byte_arrays_round_trip_through_json(values in prop::collection::vec(any::<u8>(), 0..64)) {
        let array = bytes(vec![values.len()], values.clone());
        let text = serde_json::to_string(&array).unwrap();
        let restored: NdArray = serde_json::from_str(&text).unwrap();
        prop_assert_eq!(restored.element(), NdArrayElement::U8);
        prop_assert_eq!(restored.as_u8(), Some(values.as_slice()));
        prop_assert_eq!(restored, array);
    }
}
