//! Identifiers of nodes.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A stable identifier of a node: the figure, a data array, an axes, an artist or a
/// panel.
///
/// Every kind of node shares one identifier space, so node identifiers are unique within
/// a figure whatever the kind of node. They remain unchanged when the figure is saved and
/// loaded, so that interaction state, links, selections and the references between nodes
/// (such as an artist's reference to its data) refer to nodes across sessions.
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

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node {}", self.0)
    }
}
