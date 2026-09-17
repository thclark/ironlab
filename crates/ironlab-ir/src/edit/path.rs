//! Property paths: the addresses of properties within a node.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

/// The address of a property within a node, as a sequence of Protocol Buffers field
/// names such as `x.limits.min` or `projection.view3d.azimuth_deg`.
///
/// A path starts at the fields of the node itself: the fields of the figure, of an
/// axes, or of the variant struct of an artist (such as `line.width_pt` of a line). A
/// path descends into a tagged value (such as limits or a projection) by naming a field
/// of the variant that is currently set, so `x.limits.min` addresses the lower bound of
/// manual limits and `projection.view3d` the camera view of a three-dimensional axes.
///
/// A path has at least one segment, no segment is empty and no segment contains `.`. Whether a path names a
/// property of a particular node is checked only when an edit is applied or a property
/// is read, because it depends on the kind of node and on the variants currently set.
///
/// In both encodings a path is a single string of segments joined by `.`, which is the
/// form parsed by [`FromStr`] and written by [`fmt::Display`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PropertyPath {
    segments: Vec<String>,
}

/// The reason why a sequence of segments is not a property path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// The path has no segments, as the empty string has.
    #[error("a property path must have at least one segment")]
    Empty,

    /// A segment of the path is empty, as in `x..limits`, `.x` or `x.`.
    #[error("the property path {path:?} has an empty segment at position {position}")]
    EmptySegment {
        /// The path as written, with its segments joined by `.`.
        path: String,
        /// The zero-based position of the first empty segment.
        position: usize,
    },

    /// A segment given to [`PropertyPath::new`] contains `.`, so the path could not be
    /// written as a string and parsed back to the same segments.
    #[error("the segment {segment:?} at position {position} of a property path contains '.'")]
    SeparatorInSegment {
        /// The segment as given.
        segment: String,
        /// The zero-based position of the segment.
        position: usize,
    },
}

impl PropertyPath {
    /// Creates a path from its segments.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::Empty`] when there are no segments,
    /// [`PathError::EmptySegment`] when a segment is empty, and
    /// [`PathError::SeparatorInSegment`] when a segment contains `.`.
    pub fn new<S: Into<String>>(segments: impl IntoIterator<Item = S>) -> Result<Self, PathError> {
        let segments: Vec<String> = segments.into_iter().map(Into::into).collect();
        if segments.is_empty() {
            return Err(PathError::Empty);
        }
        for (position, segment) in segments.iter().enumerate() {
            if segment.is_empty() {
                return Err(PathError::EmptySegment {
                    path: segments.join("."),
                    position,
                });
            }
            if segment.contains('.') {
                return Err(PathError::SeparatorInSegment {
                    segment: segment.clone(),
                    position,
                });
            }
        }
        Ok(Self { segments })
    }

    /// Creates a path from segments that are known to be valid, as the property
    /// registry's are, because they are the field names of the wire schema.
    ///
    /// # Panics
    ///
    /// Panics when the segments are not a valid path, which is a mistake in the
    /// registry rather than in a caller's input.
    pub(crate) fn of(segments: &[&'static str]) -> Self {
        Self::new(segments.iter().copied()).expect("the registry names valid paths")
    }

    /// Returns the segments of the path, outermost first.
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Returns whether this path is equal to `other` or is an ancestor of it, comparing
    /// whole segments, so that `x.limits` contains `x.limits` and `x.limits.min` but
    /// not `x.limits_min` or `x`.
    pub fn contains(&self, other: &PropertyPath) -> bool {
        other.segments.starts_with(&self.segments)
    }

    /// Returns whether either path contains the other, which is the relation by which
    /// an overlay entry conflicts with an edit of the owner.
    pub fn overlaps(&self, other: &PropertyPath) -> bool {
        self.contains(other) || other.contains(self)
    }
}

impl FromStr for PropertyPath {
    type Err = PathError;

    /// Parses a path whose segments are separated by `.`.
    fn from_str(path: &str) -> Result<Self, Self::Err> {
        if path.is_empty() {
            return Err(PathError::Empty);
        }
        PropertyPath::new(path.split('.'))
    }
}

impl fmt::Display for PropertyPath {
    /// Writes the segments of the path separated by `.`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut segments = self.segments.iter();
        if let Some(first) = segments.next() {
            f.write_str(first)?;
        }
        for segment in segments {
            write!(f, ".{segment}")?;
        }
        Ok(())
    }
}

impl TryFrom<String> for PropertyPath {
    type Error = PathError;

    fn try_from(path: String) -> Result<Self, Self::Error> {
        path.parse()
    }
}

impl From<PropertyPath> for String {
    fn from(path: PropertyPath) -> Self {
        path.to_string()
    }
}

impl JsonSchema for PropertyPath {
    fn schema_name() -> Cow<'static, str> {
        "PropertyPath".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^[^.]+(\\.[^.]+)*$",
            "description": "The address of a property within a node: Protocol Buffers field names separated by dots, such as x.limits.min."
        })
    }
}
