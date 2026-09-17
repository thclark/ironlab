//! Identifiers for nodes and data arrays.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A stable identifier of a node (the figure, an axes or an artist).
///
/// Node identifiers are unique within a figure and remain unchanged when the figure
/// is saved and loaded, so that interaction state, links and future selections can
/// refer to nodes across sessions.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct NodeId(pub u64);

/// An identifier of a numeric array in the figure's data table.
///
/// Artists refer to their data by identifier rather than containing it, so that
/// several artists can share one array and the array is stored once.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct DataId(pub u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node {}", self.0)
    }
}

impl fmt::Display for DataId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "data {}", self.0)
    }
}
