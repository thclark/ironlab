//! Errors raised by operations on the figure IR.

use crate::ids::NodeId;

/// An error raised by an operation on the figure IR.
#[derive(Debug, thiserror::Error)]
pub enum IrError {
    /// The JSON text could not be parsed or does not describe a figure.
    #[error("invalid figure JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// The bytes could not be decoded as a Protocol Buffers figure (the `.fig`
    /// format), or they hold a value that the figure schema does not allow.
    #[error("invalid figure protobuf: {0}")]
    Protobuf(#[from] ProtobufError),

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

/// The reason why bytes could not be decoded as a Protocol Buffers figure.
#[derive(Debug, thiserror::Error)]
pub enum ProtobufError {
    /// The bytes are not a valid Protocol Buffers encoding of a figure message, for
    /// example because they are truncated inside a field.
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),

    /// An enum field holds a value that this build does not define.
    #[error("{field} has the unknown enum value {value}")]
    UnknownEnumValue {
        /// The path of the field within the figure, such as `axes[0].x.scale`.
        field: String,
        /// The value found.
        value: i32,
    },

    /// A field that has no default is absent or unspecified, such as the kind of an
    /// artist, the dimension of an axis link, a node identifier or a required
    /// reference to a data array.
    #[error("{field} is required but absent or unspecified")]
    MissingField {
        /// The path of the field within the figure, such as `axes[0].artists[1].kind`.
        field: String,
    },

    /// A field holds a value that cannot be represented in memory, such as an array
    /// dimension larger than the address space.
    #[error("{field} has an unrepresentable value: {reason}")]
    InvalidValue {
        /// The path of the field within the figure.
        field: String,
        /// Why the value cannot be represented.
        reason: String,
    },
}

impl From<prost::DecodeError> for IrError {
    fn from(error: prost::DecodeError) -> Self {
        IrError::Protobuf(ProtobufError::Decode(error))
    }
}
