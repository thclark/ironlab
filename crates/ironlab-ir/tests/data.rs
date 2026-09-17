//! Numeric arrays: missing values, JSON representation and shape checks.

use ironlab_ir::*;
use proptest::prelude::*;
use serde_json::json;

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
    assert!(array.values[0].is_nan());
    assert_eq!(array.values[1], 4.0);
}

// Why: serialising infinities with a plain JSON writer would fail or emit invalid JSON;
// the documented behaviour is that every non-finite value is stored as missing.
#[test]
fn infinities_are_written_as_null_and_read_as_nan() {
    let array = NdArray::vector(vec![f64::INFINITY, f64::NEG_INFINITY]);
    let value = serde_json::to_value(&array).unwrap();
    assert_eq!(value["values"], json!([null, null]));
    let restored: NdArray = serde_json::from_value(value).unwrap();
    assert!(restored.values.iter().all(|v| v.is_nan()));
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
        prop_assert_eq!(restored.values.len(), values.len());
        for (original, restored) in values.iter().zip(&restored.values) {
            if original.is_finite() {
                prop_assert_eq!(original.to_bits(), restored.to_bits());
            } else {
                prop_assert!(restored.is_nan());
            }
        }
    }

}
