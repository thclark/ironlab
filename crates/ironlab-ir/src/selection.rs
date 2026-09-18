//! Selections of nodes and of data points, kept consistent with edits.
//!
//! A [`Selection`] names a node and, optionally, flat indices into the primary array of
//! an artist: the x data of a line, scatter or quiver, and the z data of a contour or
//! surface, whose flat index of row `j` and column `i` is `j * nx + i`. When a
//! transaction is applied to the figure, [`Selection::updated`] returns the selection
//! that refers to the same things afterwards, as decided in ADR 0008.

use std::collections::BTreeSet;

use crate::artist::{Artist, Grid, ScatterColor, ScatterSize};
use crate::edit::{Edit, NodeTree, Transaction};
use crate::figure::Figure;
use crate::ids::{DataId, NodeId};

/// A selected node, with optional selected points of its primary data array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The selected node.
    pub node: NodeId,
    /// The flat indices of the selected entries of the node's primary array, or `None`
    /// when the node is selected as a whole.
    pub indices: Option<BTreeSet<usize>>,
}

impl Selection {
    /// Selects a node as a whole.
    pub fn node(node: NodeId) -> Self {
        Self {
            node,
            indices: None,
        }
    }

    /// Selects entries of the primary array of an artist by their flat indices.
    pub fn points(node: NodeId, indices: impl IntoIterator<Item = usize>) -> Self {
        Self {
            node,
            indices: Some(indices.into_iter().collect()),
        }
    }

    /// Returns the selection after a transaction is applied, or `None` when the
    /// selection is cleared.
    ///
    /// `before` is the figure before the transaction, which must have been applied
    /// successfully. The edits are considered in order:
    ///
    /// - appending to the primary array without `retain` keeps the indices;
    /// - appending to the primary array with a `retain` that discards the first `k`
    ///   entries along the first dimension shifts every index down by `k` times the
    ///   number of values in one entry (one for a vector, `nx` for an array of shape
    ///   `[ny, nx]`), and drops the indices that would become negative; when every index
    ///   is dropped the selection keeps an empty set of indices; `k` is found from the
    ///   length of the array as left by the earlier edits of the transaction, not from
    ///   its length in `before`;
    /// - replacing any array that the selected artist refers to with
    ///   [`Edit::PutData`] keeps the node and drops the indices;
    /// - removing the selected node, or its ancestor at the time of the removal, clears
    ///   the selection, even when a later edit inserts a node with the same identifier;
    ///   an artist that an earlier edit moved out of a removed axes stays selected;
    /// - every other edit leaves the selection unchanged.
    pub fn updated(&self, before: &Figure, transaction: &Transaction) -> Option<Selection> {
        let mut tree = NodeTree::of(before);
        let arrays = arrays_of(before, self.node);
        let mut indices = self.indices.clone();
        // The length of the primary array along its first dimension, and the number of
        // values in one entry of it, as the edits so far leave them.
        let mut window = arrays.as_ref().and_then(|arrays| {
            let array = before.data.get(&arrays.primary)?;
            let (&entries, rest) = array.shape.split_first()?;
            Some((entries, rest.iter().product::<usize>()))
        });
        for edit in &transaction.edits {
            match edit {
                Edit::Remove { node } => {
                    if tree.subtree(*node).contains(&self.node) {
                        return None;
                    }
                    tree.apply(edit);
                }
                Edit::PutData { id, .. } => {
                    if arrays
                        .as_ref()
                        .is_some_and(|arrays| arrays.all.contains(id))
                    {
                        indices = None;
                    }
                }
                Edit::AppendData { id, array, retain } => {
                    let primary = arrays.as_ref().is_some_and(|arrays| arrays.primary == *id);
                    if let Some((entries, values)) = &mut window
                        && primary
                    {
                        *entries += array.shape.first().copied().unwrap_or_default();
                        let kept = retain.map_or(*entries, |retain| {
                            usize::try_from(retain).unwrap_or(usize::MAX)
                        });
                        if kept < *entries {
                            let discarded = (*entries - kept) * *values;
                            *entries = kept;
                            if let Some(indices) = &mut indices {
                                *indices = indices
                                    .iter()
                                    .filter_map(|i| i.checked_sub(discarded))
                                    .collect();
                            }
                        }
                    }
                }
                _ => tree.apply(edit),
            }
        }
        Some(Selection {
            node: self.node,
            indices,
        })
    }
}

/// The data arrays of an artist: the primary array, whose entries its indices number,
/// and every array it refers to.
struct Arrays {
    primary: DataId,
    all: Vec<DataId>,
}

/// Returns the arrays of an artist of the figure, or `None` when the node is not an
/// artist.
fn arrays_of(figure: &Figure, node: NodeId) -> Option<Arrays> {
    let (_, artist) = figure.artist(node)?;
    let mut all = Vec::new();
    let primary = match artist {
        Artist::Line(a) => {
            all.extend([a.x, a.y]);
            all.extend(a.z);
            a.x
        }
        Artist::Scatter(a) => {
            all.extend([a.x, a.y]);
            all.extend(a.z);
            if let ScatterSize::Data { data } = a.size {
                all.push(data);
            }
            if let ScatterColor::Data { data } = a.color {
                all.push(data);
            }
            a.x
        }
        Artist::Quiver(a) => {
            all.extend([a.x, a.y, a.u, a.v]);
            all.extend(a.z);
            all.extend(a.w);
            a.x
        }
        Artist::Contour(a) => {
            let (x, y) = grid_data(a.grid);
            all.extend([x, y, a.z]);
            a.z
        }
        Artist::Surface(a) => {
            let (x, y) = grid_data(a.grid);
            all.extend([x, y, a.z]);
            all.extend(a.c);
            a.z
        }
    };
    Some(Arrays { primary, all })
}

/// Returns the coordinate arrays of a grid.
fn grid_data(grid: Grid) -> (DataId, DataId) {
    match grid {
        Grid::Rectilinear { x, y } | Grid::Curvilinear { x, y } => (x, y),
    }
}
