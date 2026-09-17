//! Numeric arrays.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::IrError;

/// A dense, row-major, n-dimensional array of 64-bit floating-point values.
///
/// A two-dimensional array with `ny` rows and `nx` columns has shape `[ny, nx]`,
/// and the value in row `j` and column `i` is `values[j * nx + i]`. A missing value
/// is NaN. JSON has no representation for non-finite numbers, so every non-finite
/// value (NaN or an infinity) is written as `null` and read back as NaN.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct NdArray {
    /// The length of each dimension, outermost first.
    pub shape: Vec<usize>,
    /// The values in row-major order; `null` denotes a missing value.
    #[serde(serialize_with = "serialize_values")]
    #[serde(deserialize_with = "deserialize_values")]
    #[schemars(with = "Vec<Option<f64>>")]
    pub values: Vec<f64>,
}

impl NdArray {
    /// Creates a one-dimensional array.
    pub fn vector(values: Vec<f64>) -> Self {
        Self {
            shape: vec![values.len()],
            values,
        }
    }

    /// Creates an array with the given shape.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::InvalidShape`] when the number of values differs from the
    /// product of the shape.
    pub fn from_shape(shape: Vec<usize>, values: Vec<f64>) -> Result<Self, IrError> {
        let count = shape
            .iter()
            .try_fold(1usize, |product, &len| product.checked_mul(len));
        if count == Some(values.len()) {
            Ok(Self { shape, values })
        } else {
            Err(IrError::InvalidShape {
                shape,
                len: values.len(),
            })
        }
    }

    /// Returns the number of values.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns true when the array holds no values.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Two arrays are equal when their shapes are equal and each pair of values is
/// either numerically equal or both NaN, so that missing values compare equal.
impl PartialEq for NdArray {
    fn eq(&self, other: &Self) -> bool {
        self.shape == other.shape
            && self.values.len() == other.values.len()
            && self
                .values
                .iter()
                .zip(&other.values)
                .all(|(a, b)| a == b || (a.is_nan() && b.is_nan()))
    }
}

/// Writes each finite value as a number and each non-finite value as `null`.
fn serialize_values<S: Serializer>(values: &[f64], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(values.iter().map(|v| v.is_finite().then_some(*v)))
}

/// Reads each number as itself and each `null` as NaN.
fn deserialize_values<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<f64>, D::Error> {
    let values = Vec::<Option<f64>>::deserialize(deserializer)?;
    Ok(values.into_iter().map(|v| v.unwrap_or(f64::NAN)).collect())
}
