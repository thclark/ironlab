//! Errors raised by operations on the figure IR.

use crate::ids::NodeId;

/// An error raised by an operation on the figure IR.
#[derive(Debug, thiserror::Error)]
pub enum IrError {
    /// The JSON text could not be parsed or does not describe a figure.
    #[error("invalid figure JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// The file declares a schema version whose major or minor component differs
    /// from the version supported by this build, or which is not a valid version.
    #[error("incompatible schema version {found:?}; this build supports {supported}")]
    IncompatibleSchemaVersion {
        /// The schema version declared by the file.
        found: String,
        /// The schema version supported by this build.
        supported: &'static str,
    },

    /// The given node identifier does not refer to an axes of the figure.
    #[error("{0} is not an axes of this figure")]
    UnknownAxes(NodeId),

    /// Manual limits are not finite or are not strictly increasing.
    #[error("invalid limits [{min}, {max}]; limits must be finite with min < max")]
    InvalidLimits {
        /// The requested lower bound.
        min: f64,
        /// The requested upper bound.
        max: f64,
    },

    /// The string is not a colour of the form `#rrggbb` or `#rrggbbaa`.
    #[error("invalid colour {0:?}; expected #rrggbb or #rrggbbaa")]
    InvalidColor(String),

    /// The number of values does not equal the product of the array's shape.
    #[error("array of {len} values does not match shape {shape:?}")]
    InvalidShape {
        /// The requested shape.
        shape: Vec<usize>,
        /// The number of values supplied.
        len: usize,
    },
}
