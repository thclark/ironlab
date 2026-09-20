//! Numeric arrays.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::IrError;

/// A dense, row-major, n-dimensional array of numeric values.
///
/// A two-dimensional array with `ny` rows and `nx` columns has shape `[ny, nx]`, and
/// the value in row `j` and column `i` is at index `j * nx + i` of the values. The
/// values are of one of two element types, which [`Values`] distinguishes and
/// [`NdArray::element`] names: 64-bit floating-point numbers, in which a missing value
/// is NaN, or 8-bit unsigned integers, which hold the pixels of images compactly. The
/// three image artists accept either element type; every other artist requires
/// floating-point values, and one that refers to an array of 8-bit values is reported
/// by validation.
///
/// # JSON
///
/// An array is written with its `shape`, its `values` in row-major order and, for 8-bit
/// values only, an `element` of `"u8"`. Floating-point values are written as numbers,
/// except that JSON has no representation for non-finite numbers, so every non-finite
/// value (NaN or an infinity) is written as `null` and read back as NaN; 8-bit values
/// are written as integers. When an array is read, an `element` that is absent, `null`
/// or `"f64"` means floating-point values, so that files written before the element
/// existed still load. Under `"u8"`, every value must be an integer from 0 to 255 (a
/// whole number written with a fraction, such as `1.0`, is accepted as the byte it
/// denotes) and `null` is refused, because a byte has no missing value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "NdArrayJson", into = "NdArrayJson")]
pub struct NdArray {
    /// The length of each dimension, outermost first.
    pub shape: Vec<usize>,
    /// The values in row-major order.
    pub values: Values,
}

/// The values of an [`NdArray`] in row-major order, of one of the element types.
#[derive(Debug, Clone)]
pub enum Values {
    /// 64-bit floating-point values, in which a missing value is NaN.
    F64(Vec<f64>),
    /// 8-bit unsigned integers, such as the components of the pixels of an image.
    U8(Vec<u8>),
}

/// The element type of an array: 64-bit floating-point values or 8-bit unsigned
/// integers.
///
/// In JSON the element is the string `"f64"` or `"u8"`, which is also how it is
/// displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NdArrayElement {
    /// 64-bit floating-point values.
    F64,
    /// 8-bit unsigned integers.
    U8,
}

impl fmt::Display for NdArrayElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NdArrayElement::F64 => "f64",
            NdArrayElement::U8 => "u8",
        })
    }
}

impl Default for Values {
    /// No values, of the floating-point element type.
    fn default() -> Self {
        Values::F64(Vec::new())
    }
}

impl Default for NdArray {
    /// An empty vector of floating-point values, with shape `[0]`.
    fn default() -> Self {
        Self::vector(Vec::new())
    }
}

impl NdArray {
    /// Creates a one-dimensional array of floating-point values.
    pub fn vector(values: Vec<f64>) -> Self {
        Self {
            shape: vec![values.len()],
            values: Values::F64(values),
        }
    }

    /// Creates an array of floating-point values with the given shape.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::InvalidShape`] when the number of values differs from the
    /// product of the shape.
    pub fn from_shape(shape: Vec<usize>, values: Vec<f64>) -> Result<Self, IrError> {
        Ok(Self {
            shape: checked_shape(shape, values.len())?,
            values: Values::F64(values),
        })
    }

    /// Creates an array of 8-bit values with the given shape.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::InvalidShape`] when the number of values differs from the
    /// product of the shape.
    pub fn from_shape_u8(shape: Vec<usize>, values: Vec<u8>) -> Result<Self, IrError> {
        Ok(Self {
            shape: checked_shape(shape, values.len())?,
            values: Values::U8(values),
        })
    }

    /// Returns the element type of the values.
    pub fn element(&self) -> NdArrayElement {
        match self.values {
            Values::F64(_) => NdArrayElement::F64,
            Values::U8(_) => NdArrayElement::U8,
        }
    }

    /// Returns the number of values, of either element type.
    pub fn len(&self) -> usize {
        match &self.values {
            Values::F64(values) => values.len(),
            Values::U8(values) => values.len(),
        }
    }

    /// Returns true when the array holds no values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the values when they are floating-point, and `None` when they are
    /// 8-bit; the values are never converted.
    pub fn as_f64(&self) -> Option<&[f64]> {
        match &self.values {
            Values::F64(values) => Some(values),
            Values::U8(_) => None,
        }
    }

    /// Returns the values when they are 8-bit, and `None` when they are
    /// floating-point; the values are never converted.
    pub fn as_u8(&self) -> Option<&[u8]> {
        match &self.values {
            Values::U8(values) => Some(values),
            Values::F64(_) => None,
        }
    }

    /// Returns the value at a row-major index as a floating-point number, widening an
    /// 8-bit value to the number it denotes, or `None` beyond the end of the values.
    pub fn get(&self, index: usize) -> Option<f64> {
        match &self.values {
            Values::F64(values) => values.get(index).copied(),
            Values::U8(values) => values.get(index).map(|&value| f64::from(value)),
        }
    }
}

/// Returns the shape when the product of its dimensions equals `len`.
fn checked_shape(shape: Vec<usize>, len: usize) -> Result<Vec<usize>, IrError> {
    let count = shape
        .iter()
        .try_fold(1usize, |product, &dimension| product.checked_mul(dimension));
    if count == Some(len) {
        Ok(shape)
    } else {
        Err(IrError::InvalidShape { shape, len })
    }
}

/// Two arrays are equal when their shapes and element types are equal and each pair
/// of values is equal, where two NaN floating-point values count as equal so that
/// missing values compare equal. An array of 8-bit values therefore never equals an
/// array of floating-point values, whatever the values.
impl PartialEq for NdArray {
    fn eq(&self, other: &Self) -> bool {
        self.shape == other.shape
            && match (&self.values, &other.values) {
                (Values::F64(a), Values::F64(b)) => {
                    a.len() == b.len()
                        && a.iter()
                            .zip(b)
                            .all(|(a, b)| a == b || (a.is_nan() && b.is_nan()))
                }
                (Values::U8(a), Values::U8(b)) => a == b,
                (Values::F64(_), Values::U8(_)) | (Values::U8(_), Values::F64(_)) => false,
            }
    }
}

// ---------------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------------

/// The JSON form of an array, which an [`NdArray`] is converted to and from.
#[derive(Serialize, Deserialize)]
struct NdArrayJson {
    shape: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    element: Option<NdArrayElement>,
    values: Vec<JsonValue>,
}

/// One value of an array as JSON holds it.
///
/// Every value is read as a number or `null`, whatever the element, because the
/// `element` that says how to interpret the values is a sibling property that the
/// reader of a value cannot see; the conversion into an [`NdArray`] then interprets
/// each value by the element.
enum JsonValue {
    /// A number, as read from JSON or as written for a finite floating-point value.
    Number(f64),
    /// An integer, as written for an 8-bit value.
    Integer(u8),
    /// `null`, as read from JSON or as written for a non-finite floating-point value.
    Null,
}

impl Serialize for JsonValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match *self {
            JsonValue::Number(value) => serializer.serialize_f64(value),
            JsonValue::Integer(value) => serializer.serialize_u8(value),
            JsonValue::Null => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for JsonValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Option::<f64>::deserialize(deserializer)? {
            Some(value) => JsonValue::Number(value),
            None => JsonValue::Null,
        })
    }
}

impl From<NdArray> for NdArrayJson {
    fn from(array: NdArray) -> Self {
        let (element, values) = match array.values {
            Values::F64(values) => (
                None,
                values
                    .into_iter()
                    .map(|value| {
                        if value.is_finite() {
                            JsonValue::Number(value)
                        } else {
                            JsonValue::Null
                        }
                    })
                    .collect(),
            ),
            Values::U8(values) => (
                Some(NdArrayElement::U8),
                values.into_iter().map(JsonValue::Integer).collect(),
            ),
        };
        NdArrayJson {
            shape: array.shape,
            element,
            values,
        }
    }
}

impl TryFrom<NdArrayJson> for NdArray {
    type Error = String;

    fn try_from(json: NdArrayJson) -> Result<Self, Self::Error> {
        let values = match json.element.unwrap_or(NdArrayElement::F64) {
            NdArrayElement::F64 => Values::F64(
                json.values
                    .into_iter()
                    .map(|value| match value {
                        JsonValue::Number(value) => value,
                        JsonValue::Integer(value) => f64::from(value),
                        JsonValue::Null => f64::NAN,
                    })
                    .collect(),
            ),
            NdArrayElement::U8 => Values::U8(
                json.values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        byte(&value).ok_or_else(|| {
                            format!(
                                "value {index} of the array is {value}, which is not an \
                                 integer from 0 to 255 as the element u8 requires"
                            )
                        })
                    })
                    .collect::<Result<_, _>>()?,
            ),
        };
        Ok(NdArray {
            shape: json.shape,
            values,
        })
    }
}

impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonValue::Number(value) => write!(f, "{value}"),
            JsonValue::Integer(value) => write!(f, "{value}"),
            JsonValue::Null => f.write_str("null"),
        }
    }
}

/// Returns the byte that a JSON value denotes: an integer from 0 to 255, which may be
/// written with a fraction of zero. `null` and every other number denote no byte.
fn byte(value: &JsonValue) -> Option<u8> {
    match *value {
        JsonValue::Integer(value) => Some(value),
        // The range excludes NaN, and the fraction check makes the truncating cast exact.
        JsonValue::Number(value) if (0.0..=255.0).contains(&value) && value.fract() == 0.0 => {
            Some(value as u8)
        }
        JsonValue::Number(_) | JsonValue::Null => None,
    }
}

// ---------------------------------------------------------------------------------
// JSON Schema
// ---------------------------------------------------------------------------------

/// The schema is written by hand, as the schema of a colour is, because a schema
/// derived from the JSON form would carry the name `NdArrayJson` rather than `NdArray`,
/// which the artists and the edit protocol refer to, and would describe the values
/// without saying how the element interprets them. JSON Schema cannot make the type
/// of the values depend on the element, so the values are numbers or `null` for both
/// elements and the descriptions state the rule.
impl JsonSchema for NdArray {
    fn schema_name() -> Cow<'static, str> {
        "NdArray".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut shape = generator.subschema_for::<Vec<usize>>();
        shape.insert(
            "description".to_owned(),
            "The length of each dimension, outermost first.".into(),
        );
        let mut element = generator.subschema_for::<Option<NdArrayElement>>();
        element.insert(
            "description".to_owned(),
            "The element type of the values: u8 for 8-bit unsigned integers, written as \
             integers, or f64 for 64-bit floating-point values, which is also what an \
             absent or null element means."
                .into(),
        );
        let mut values = generator.subschema_for::<Vec<Option<f64>>>();
        values.insert(
            "description".to_owned(),
            "The values in row-major order. A non-finite floating-point value is written \
             as null and read back as NaN; an 8-bit value is an integer from 0 to 255 \
             and is never null."
                .into(),
        );
        json_schema!({
            "type": "object",
            "description": "A dense, row-major, n-dimensional array of 64-bit floating-point values or of 8-bit unsigned integers.",
            "properties": {
                "shape": shape,
                "element": element,
                "values": values
            },
            "required": ["shape", "values"]
        })
    }
}
