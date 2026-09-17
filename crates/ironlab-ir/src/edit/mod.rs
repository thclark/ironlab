//! Typed edits of a figure, applied in atomic transactions.
//!
//! A change to a figure is an [`Edit`], and edits are applied in a [`Transaction`] by
//! [`Figure::apply`], as decided in ADR 0008. An edit changes exactly what it names and
//! nothing else: setting the limits of a linked axes changes only that axes. Behaviour
//! that depends on the state of the figure, such as keeping linked axes in step, is
//! provided by the functions of the [`command`](crate::command) module, which read a
//! figure and return a literal transaction. A client that mirrors a figure therefore
//! applies a stream of transactions without reimplementing any rule.
//!
//! # Property paths
//!
//! [`Edit::Set`] addresses a property by a [`PropertyPath`] of Protocol Buffers field
//! names that starts at the fields of the node: the figure (`title`, `size.width_mm`,
//! `layout.rows`, `links`, `parameters`, ...), an axes (`x.limits`, `colormap`,
//! `projection.view3d.azimuth_deg`, ...) or the variant struct of an artist
//! (`line.width_pt`, `visible`, `levels`, ...). A path descends into a tagged value by
//! naming a field of the variant that is currently set, and into an optional value only
//! while it is present. The settable paths of each kind of node are listed by
//! [`properties`].
//!
//! Identifiers (`id`), the schema version (`schema_version`), the provenance
//! (`provenance`), the node lists (`axes`, `artists`) and the data table (`data`) are
//! read-only: nodes and data change only through the structural and data edits.
//!
//! # Transactions
//!
//! A transaction applies all of its edits, in order, or none. Each edit sees the figure
//! as left by the edits before it, so a transaction can insert an axes and then set its
//! properties. After the edits, the figure is validated, and the transaction is rejected
//! and rolled back when validation reports an error that the figure did not have before
//! (compared by node and kind, as [`Figure::apply`] describes);
//! warnings never reject a transaction, and errors that the figure already had do not
//! prevent it from being edited. Applying a transaction returns its inverse, which
//! restores the figure exactly, including the bits of every floating-point value.

mod choice;
mod path;
mod registry;
mod value;
mod walk;

pub use choice::{Choice, choices, meaningful_choices};
pub use path::{PathError, PropertyPath};
pub use registry::{NodeKind, Property, properties};
pub use value::{Value, ValueType};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use std::collections::{BTreeSet, HashMap};

use crate::artist::Artist;
use crate::axes::Axes;
use crate::data::NdArray;
use crate::error::IrError;
use crate::figure::Figure;
use crate::ids::{DataId, NodeId};
use crate::validate::{IssueKind, ValidationIssue, ValidationReport};
use crate::wire;
use walk::Step;

/// An ordered list of edits that are applied together or not at all.
///
/// A transaction is encoded as the Protocol Buffers message `Transaction` of
/// `ironlab/ir/v0/edit.proto`, and as JSON with the same structure.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Transaction {
    /// The edits, in the order in which they are applied.
    pub edits: Vec<Edit>,
}

/// A change to a figure.
///
/// In JSON an edit is an object whose `type` names the kind of edit in `snake_case`,
/// alongside the fields of that kind, such as
/// `{"type": "set", "node": 3, "path": "x.limits", "value": {"type": "limits", ...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Edit {
    /// Replaces the value of a property of a node.
    Set {
        /// The node whose property is set.
        node: NodeId,
        /// The path of the property within the node.
        path: PropertyPath,
        /// The new value, whose type must be that of the property, or
        /// [`Value::Unset`] to clear an optional property.
        value: Value,
    },
    /// Inserts an axes into the figure, or an artist into an axes, with its subtree.
    Insert {
        /// The parent: the figure for an axes, or an axes for an artist.
        parent: NodeId,
        /// The position in the parent's list at which the node is inserted, from zero
        /// to the length of the list, or `None` to append it.
        index: Option<u32>,
        /// The node, whose identifier and whose descendants' identifiers must not be in
        /// use in the figure.
        node: Node,
    },
    /// Removes a node and its subtree.
    Remove {
        /// The axes or artist to remove.
        node: NodeId,
    },
    /// Moves an axes within the figure's list, or an artist within its axes or to
    /// another axes.
    Move {
        /// The axes or artist to move.
        node: NodeId,
        /// The new parent: the figure for an axes, or an axes for an artist.
        parent: NodeId,
        /// The position in the new parent's list after the node has been removed from
        /// its old position, from zero to the length of that list, or `None` to append.
        index: Option<u32>,
    },
    /// Creates a data array or replaces an existing one.
    PutData {
        /// The identifier of the array.
        id: DataId,
        /// The new array.
        array: NdArray,
    },
    /// Appends entries to an existing array along its first dimension.
    AppendData {
        /// The identifier of the existing array.
        id: DataId,
        /// The entries to append, whose shape after the first dimension must equal that
        /// of the existing array.
        array: NdArray,
        /// When set, only the last `retain` entries along the first dimension are kept
        /// after appending, which gives a rolling window for streamed data.
        retain: Option<u64>,
    },
    /// Removes a data array.
    RemoveData {
        /// The identifier of the array.
        id: DataId,
    },
}

/// A node that can be inserted into a figure, with its subtree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Node {
    /// An axes with its artists, boxed because an axes is much larger than an artist.
    Axes(Box<Axes>),
    /// An artist.
    Artist(Artist),
}

impl Node {
    /// Returns the identifier of the node.
    pub fn id(&self) -> NodeId {
        match self {
            Node::Axes(axes) => axes.id,
            Node::Artist(artist) => artist.id(),
        }
    }

    /// Returns the identifiers of the node and of its descendants, the node first.
    fn subtree_ids(&self) -> Vec<NodeId> {
        match self {
            Node::Axes(axes) => std::iter::once(axes.id)
                .chain(axes.artists.iter().map(Artist::id))
                .collect(),
            Node::Artist(artist) => vec![artist.id()],
        }
    }
}

/// An error raised when an edit cannot be applied or a property cannot be read.
///
/// Every variant that concerns one edit carries the zero-based index of that edit in its
/// transaction, which is `None` when the error comes from [`Figure::get`] or from a
/// single edit outside a transaction.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EditError {
    /// The node identifier does not refer to a node of the figure.
    #[error("edit {edit:?}: {node} is not in the figure")]
    UnknownNode {
        /// The index of the edit.
        edit: Option<usize>,
        /// The identifier that was not found.
        node: NodeId,
    },

    /// The data identifier does not refer to an array of the figure.
    #[error("edit {edit:?}: {id} is not in the data table")]
    UnknownData {
        /// The index of the edit.
        edit: Option<usize>,
        /// The identifier that was not found.
        id: DataId,
    },

    /// The path names a field that the node, or a value on the path, does not have.
    #[error("edit {edit:?}: {node} has no property {path}")]
    UnknownPath {
        /// The index of the edit.
        edit: Option<usize>,
        /// The node addressed.
        node: NodeId,
        /// The path addressed.
        path: PropertyPath,
    },

    /// The value is not of the type of the property, or is [`Value::Unset`] for a
    /// property that is not optional.
    #[error("edit {edit:?}: {path} of {node} takes a value of type {expected:?}, not {found:?}")]
    TypeMismatch {
        /// The index of the edit.
        edit: Option<usize>,
        /// The node addressed.
        node: NodeId,
        /// The path addressed.
        path: PropertyPath,
        /// The type of the property.
        expected: ValueType,
        /// The type of the value supplied, or `None` for [`Value::Unset`].
        found: Option<ValueType>,
    },

    /// The path descends into a variant of a tagged value that is not the variant
    /// currently set, such as `projection.view3d` of a two-dimensional axes.
    #[error("edit {edit:?}: {path} of {node} names a variant that is not set")]
    InactiveVariant {
        /// The index of the edit.
        edit: Option<usize>,
        /// The node addressed.
        node: NodeId,
        /// The path addressed.
        path: PropertyPath,
    },

    /// The path descends through an optional value that is absent, such as
    /// `title.content` of a node without a title.
    #[error("edit {edit:?}: {path} of {node} descends through an absent value")]
    AbsentValue {
        /// The index of the edit.
        edit: Option<usize>,
        /// The node addressed.
        node: NodeId,
        /// The path addressed.
        path: PropertyPath,
    },

    /// The path names a property that edits cannot set: an identifier, the schema
    /// version, the provenance, a node list or the data table, or a path below one of
    /// them.
    #[error("edit {edit:?}: {path} of {node} is read-only")]
    ReadOnly {
        /// The index of the edit.
        edit: Option<usize>,
        /// The node addressed.
        node: NodeId,
        /// The path addressed.
        path: PropertyPath,
    },

    /// An inserted node, or a node of its subtree, has an identifier that is already in
    /// use in the figure or that occurs twice in the subtree.
    #[error("edit {edit:?}: {node} is already in use")]
    DuplicateId {
        /// The index of the edit.
        edit: Option<usize>,
        /// The identifier that is already in use.
        node: NodeId,
    },

    /// The parent of an inserted or moved node cannot hold it: an axes must be placed in
    /// the figure and an artist in an axes.
    #[error("edit {edit:?}: {parent} cannot hold {node}")]
    InvalidParent {
        /// The index of the edit.
        edit: Option<usize>,
        /// The node inserted or moved.
        node: NodeId,
        /// The parent named by the edit.
        parent: NodeId,
    },

    /// The figure node was named by an edit that removes or moves a node; the figure is
    /// the root and can be neither.
    #[error("edit {edit:?}: the figure node {node} cannot be removed or moved")]
    RootNode {
        /// The index of the edit.
        edit: Option<usize>,
        /// The identifier of the figure.
        node: NodeId,
    },

    /// The index of an inserted or moved node is larger than the length of the list it
    /// is placed in.
    #[error("edit {edit:?}: index {index} is beyond the {len} children of {parent}")]
    IndexOutOfRange {
        /// The index of the edit.
        edit: Option<usize>,
        /// The parent named by the edit.
        parent: NodeId,
        /// The index requested.
        index: u32,
        /// The length of the list, after removing a moved node from it.
        len: usize,
    },

    /// Appended entries have a shape after the first dimension that differs from that
    /// of the existing array, or either array has no dimensions.
    #[error(
        "edit {edit:?}: entries of shape {appended:?} cannot be appended to {id} of shape {existing:?}"
    )]
    ShapeMismatch {
        /// The index of the edit.
        edit: Option<usize>,
        /// The array appended to.
        id: DataId,
        /// The shape of the existing array.
        existing: Vec<usize>,
        /// The shape of the appended entries.
        appended: Vec<usize>,
    },

    /// An edit other than [`Edit::Set`] was given to an overlay, which holds only sets.
    #[error("edit {edit:?} is not a set, and an overlay holds only sets")]
    NotASet {
        /// The index of the edit.
        edit: Option<usize>,
    },

    /// After its edits, the figure has validation errors that it did not have before;
    /// the transaction was rolled back.
    #[error("the transaction would introduce validation errors: {0:?}")]
    Invalid(Vec<ValidationIssue>),
}

impl EditError {
    /// Returns the error with the index of the edit it concerns replaced, so that an
    /// error raised by a transaction of one edit can be reported without an index.
    pub(crate) fn at_edit(mut self, index: Option<usize>) -> Self {
        let edit = match &mut self {
            EditError::UnknownNode { edit, .. }
            | EditError::UnknownData { edit, .. }
            | EditError::UnknownPath { edit, .. }
            | EditError::TypeMismatch { edit, .. }
            | EditError::InactiveVariant { edit, .. }
            | EditError::AbsentValue { edit, .. }
            | EditError::ReadOnly { edit, .. }
            | EditError::DuplicateId { edit, .. }
            | EditError::InvalidParent { edit, .. }
            | EditError::RootNode { edit, .. }
            | EditError::IndexOutOfRange { edit, .. }
            | EditError::ShapeMismatch { edit, .. }
            | EditError::NotASet { edit } => edit,
            EditError::Invalid(_) => return self,
        };
        *edit = index;
        self
    }
}

impl Figure {
    /// Applies a transaction, and returns its inverse.
    ///
    /// The edits are applied in order, each to the figure as left by the edits before
    /// it. The figure is then validated, and the transaction is rejected when validation
    /// reports an error that the figure did not report before. Errors are compared by
    /// their node and kind, counting repeats: the transaction is rejected when, for some
    /// node (or for no node) and some kind, validation reports more errors after the
    /// edits than before. An error of a kind that the figure already reports for another
    /// node is therefore new, as is a second error of the same kind on the same node,
    /// while changing a value that is already invalid to another invalid value is not.
    /// Messages are not compared, because they quote the values. Warnings never reject a
    /// transaction.
    ///
    /// The inverse lists the inverse of each edit in reverse order, so applying it
    /// restores the figure to a value equal to the one before, with every floating-point
    /// value restored bit for bit. Every node identifier inserted by the transaction is
    /// excluded from later allocation by [`Figure::alloc_node_id`], even after the
    /// inverse removes the node again.
    ///
    /// # Errors
    ///
    /// Returns the error of the first edit that cannot be applied, identified by its
    /// index, or [`EditError::Invalid`] with the new validation errors. In either case
    /// the figure is left equal to the figure before the call.
    pub fn apply(&mut self, transaction: &Transaction) -> Result<Transaction, EditError> {
        let before = error_counts(&self.validate());
        let mut inverses: Vec<Edit> = Vec::with_capacity(transaction.edits.len());
        for (index, edit) in transaction.edits.iter().enumerate() {
            match self.apply_edit(edit, Some(index)) {
                Ok(inverse) => inverses.push(inverse),
                Err(error) => {
                    self.roll_back(inverses);
                    return Err(error);
                }
            }
        }
        let introduced = introduced_errors(before, self.validate());
        if !introduced.is_empty() {
            self.roll_back(inverses);
            return Err(EditError::Invalid(introduced));
        }
        inverses.reverse();
        Ok(Transaction { edits: inverses })
    }

    /// Returns the value of a property of a node.
    ///
    /// An optional property that is absent is returned as [`Value::Unset`].
    ///
    /// # Errors
    ///
    /// Returns [`EditError::UnknownNode`], [`EditError::UnknownPath`],
    /// [`EditError::InactiveVariant`], [`EditError::AbsentValue`] or
    /// [`EditError::ReadOnly`], with no edit index, when the property cannot be read.
    ///
    /// A path that no node of the kind can have (one that [`properties`] does not list
    /// and that does not lie below a read-only property) is an
    /// [`EditError::UnknownPath`] whatever the variants and optional values currently
    /// set, so that a misspelled path is reported as such in every state of the node.
    /// The same holds for [`Edit::Set`].
    pub fn get(&self, node: NodeId, path: &PropertyPath) -> Result<Value, EditError> {
        walk::get_in(self, node, path.segments())
            .ok_or(EditError::UnknownNode { edit: None, node })?
            .map_err(|step| self.edit_error(None, node, path, step))
    }

    /// Returns the kind of the node with the given identifier, or `None` when the
    /// identifier is not a node of the figure.
    pub fn node_kind(&self, node: NodeId) -> Option<NodeKind> {
        walk::kind_of(self, node)
    }

    /// Returns the error of a failed step of a path walk.
    ///
    /// A path that the node's kind never has is unknown whatever the variants and
    /// optional values currently set, so a step that stopped at an inactive variant or
    /// an absent value is reported as unknown unless the registry lists the whole path.
    fn edit_error(
        &self,
        index: Option<usize>,
        node: NodeId,
        path: &PropertyPath,
        step: Step,
    ) -> EditError {
        let listed = || {
            self.node_kind(node)
                .is_some_and(|kind| properties(kind).iter().any(|p| p.path == *path))
        };
        let unknown = EditError::UnknownPath {
            edit: index,
            node,
            path: path.clone(),
        };
        match step {
            Step::Unknown => unknown,
            Step::ReadOnly => EditError::ReadOnly {
                edit: index,
                node,
                path: path.clone(),
            },
            Step::Inactive if listed() => EditError::InactiveVariant {
                edit: index,
                node,
                path: path.clone(),
            },
            Step::Absent if listed() => EditError::AbsentValue {
                edit: index,
                node,
                path: path.clone(),
            },
            Step::Inactive | Step::Absent => unknown,
            Step::Type { expected, found } => EditError::TypeMismatch {
                edit: index,
                node,
                path: path.clone(),
                expected,
                found,
            },
        }
    }

    /// Applies one edit and returns its inverse, leaving the figure unchanged when the
    /// edit cannot be applied. The figure is not validated.
    pub(crate) fn apply_edit(
        &mut self,
        edit: &Edit,
        index: Option<usize>,
    ) -> Result<Edit, EditError> {
        match edit {
            Edit::Set { node, path, value } => self.apply_set(*node, path, value.clone(), index),
            Edit::Insert {
                parent,
                index: at,
                node,
            } => self.apply_insert(*parent, *at, node.clone(), index),
            Edit::Remove { node } => self.apply_remove(*node, index),
            Edit::Move {
                node,
                parent,
                index: at,
            } => self.apply_move(*node, *parent, *at, index),
            Edit::PutData { id, array } => Ok(self.apply_put_data(*id, array.clone())),
            Edit::AppendData { id, array, retain } => {
                self.apply_append_data(*id, array, *retain, index)
            }
            Edit::RemoveData { id } => match self.data.remove(id) {
                Some(array) => Ok(Edit::PutData { id: *id, array }),
                None => Err(EditError::UnknownData {
                    edit: index,
                    id: *id,
                }),
            },
        }
    }

    /// Applies the inverses of the edits applied so far, in reverse order.
    fn roll_back(&mut self, inverses: Vec<Edit>) {
        for inverse in inverses.into_iter().rev() {
            self.apply_edit(&inverse, None)
                .expect("the inverse of an applied edit always applies");
        }
    }

    fn apply_set(
        &mut self,
        node: NodeId,
        path: &PropertyPath,
        value: Value,
        index: Option<usize>,
    ) -> Result<Edit, EditError> {
        let missing = || EditError::UnknownNode { edit: index, node };
        let old = walk::get_in(self, node, path.segments())
            .ok_or_else(missing)?
            .map_err(|step| self.edit_error(index, node, path, step))?;
        let written = walk::set_in(self, node, path.segments(), value).ok_or_else(missing)?;
        written.map_err(|step| self.edit_error(index, node, path, step))?;
        Ok(Edit::Set {
            node,
            path: path.clone(),
            value: old,
        })
    }

    fn apply_insert(
        &mut self,
        parent: NodeId,
        at: Option<u32>,
        node: Node,
        index: Option<usize>,
    ) -> Result<Edit, EditError> {
        let id = node.id();
        let parent_kind = self.node_kind(parent).ok_or(EditError::UnknownNode {
            edit: index,
            node: parent,
        })?;
        let fits = match node {
            Node::Axes(_) => parent_kind == NodeKind::Figure,
            Node::Artist(_) => parent_kind == NodeKind::Axes,
        };
        if !fits {
            return Err(EditError::InvalidParent {
                edit: index,
                node: id,
                parent,
            });
        }
        let mut used: BTreeSet<NodeId> = self.node_ids().collect();
        for new in node.subtree_ids() {
            if !used.insert(new) {
                return Err(EditError::DuplicateId {
                    edit: index,
                    node: new,
                });
            }
        }
        let len = match &node {
            Node::Axes(_) => self.axes.len(),
            Node::Artist(_) => self
                .axes(parent)
                .expect("the parent was found to be an axes")
                .artists
                .len(),
        };
        let at = at.unwrap_or_else(|| as_index(len));
        if at as usize > len {
            return Err(EditError::IndexOutOfRange {
                edit: index,
                parent,
                index: at,
                len,
            });
        }
        let ids = node.subtree_ids();
        match node {
            Node::Axes(axes) => self.axes.insert(at as usize, *axes),
            Node::Artist(artist) => self
                .axes_mut(parent)
                .expect("the parent was found to be an axes")
                .artists
                .insert(at as usize, artist),
        }
        for new in ids {
            self.id_allocator.next = self.id_allocator.next.max(new.0.saturating_add(1));
        }
        Ok(Edit::Remove { node: id })
    }

    fn apply_remove(&mut self, node: NodeId, index: Option<usize>) -> Result<Edit, EditError> {
        let figure = self.id;
        if node == figure {
            return Err(EditError::RootNode { edit: index, node });
        }
        if let Some(position) = self.axes.iter().position(|axes| axes.id == node) {
            let axes = self.axes.remove(position);
            return Ok(Edit::Insert {
                parent: figure,
                index: Some(as_index(position)),
                node: Node::Axes(Box::new(axes)),
            });
        }
        for axes in &mut self.axes {
            if let Some(position) = axes.artists.iter().position(|artist| artist.id() == node) {
                let artist = axes.artists.remove(position);
                return Ok(Edit::Insert {
                    parent: axes.id,
                    index: Some(as_index(position)),
                    node: Node::Artist(artist),
                });
            }
        }
        Err(EditError::UnknownNode { edit: index, node })
    }

    fn apply_move(
        &mut self,
        node: NodeId,
        parent: NodeId,
        at: Option<u32>,
        index: Option<usize>,
    ) -> Result<Edit, EditError> {
        if node == self.id {
            return Err(EditError::RootNode { edit: index, node });
        }
        let kind = self
            .node_kind(node)
            .ok_or(EditError::UnknownNode { edit: index, node })?;
        let parent_kind = self.node_kind(parent).ok_or(EditError::UnknownNode {
            edit: index,
            node: parent,
        })?;
        let wanted = if kind == NodeKind::Axes {
            NodeKind::Figure
        } else {
            NodeKind::Axes
        };
        if parent_kind != wanted {
            return Err(EditError::InvalidParent {
                edit: index,
                node,
                parent,
            });
        }
        if kind == NodeKind::Axes {
            let from = self
                .axes
                .iter()
                .position(|axes| axes.id == node)
                .expect("the node was found to be an axes");
            let len = self.axes.len() - 1;
            let to = at.unwrap_or_else(|| as_index(len));
            if to as usize > len {
                return Err(EditError::IndexOutOfRange {
                    edit: index,
                    parent,
                    index: to,
                    len,
                });
            }
            let axes = self.axes.remove(from);
            self.axes.insert(to as usize, axes);
            return Ok(Edit::Move {
                node,
                parent,
                index: Some(as_index(from)),
            });
        }
        let (source, from) = self
            .axes
            .iter()
            .enumerate()
            .find_map(|(a, axes)| {
                let position = axes.artists.iter().position(|artist| artist.id() == node)?;
                Some((a, position))
            })
            .expect("the node was found to be an artist");
        let target = self
            .axes
            .iter()
            .position(|axes| axes.id == parent)
            .expect("the parent was found to be an axes");
        let len = self.axes[target].artists.len() - usize::from(target == source);
        let to = at.unwrap_or_else(|| as_index(len));
        if to as usize > len {
            return Err(EditError::IndexOutOfRange {
                edit: index,
                parent,
                index: to,
                len,
            });
        }
        let artist = self.axes[source].artists.remove(from);
        self.axes[target].artists.insert(to as usize, artist);
        Ok(Edit::Move {
            node,
            parent: self.axes[source].id,
            index: Some(as_index(from)),
        })
    }

    fn apply_put_data(&mut self, id: DataId, array: NdArray) -> Edit {
        match self.data.insert(id, array) {
            Some(old) => Edit::PutData { id, array: old },
            None => Edit::RemoveData { id },
        }
    }

    fn apply_append_data(
        &mut self,
        id: DataId,
        array: &NdArray,
        retain: Option<u64>,
        index: Option<usize>,
    ) -> Result<Edit, EditError> {
        let existing = self
            .data
            .get_mut(&id)
            .ok_or(EditError::UnknownData { edit: index, id })?;
        let mismatched = existing.shape.is_empty()
            || array.shape.is_empty()
            || existing.shape[1..] != array.shape[1..];
        if mismatched {
            return Err(EditError::ShapeMismatch {
                edit: index,
                id,
                existing: existing.shape.clone(),
                appended: array.shape.clone(),
            });
        }
        let old = existing.clone();
        existing.shape[0] += array.shape[0];
        existing.values.extend_from_slice(&array.values);
        if let Some(retain) = retain {
            let kept = usize::try_from(retain).unwrap_or(usize::MAX);
            if kept < existing.shape[0] {
                let discarded = existing.shape[0] - kept;
                let entry: usize = existing.shape[1..].iter().product();
                existing.values.drain(..discarded * entry);
                existing.shape[0] = kept;
            }
        }
        Ok(Edit::PutData { id, array: old })
    }
}

/// Returns an index within a list as it is written in an edit.
fn as_index(position: usize) -> u32 {
    u32::try_from(position).unwrap_or(u32::MAX)
}

/// Counts the validation errors of a report by the node and kind of each error.
fn error_counts(report: &ValidationReport) -> HashMap<(Option<NodeId>, IssueKind), usize> {
    let mut counts = HashMap::new();
    for issue in &report.errors {
        *counts.entry((issue.node, issue.kind)).or_insert(0) += 1;
    }
    counts
}

/// Returns the errors of a report that the counts of an earlier report do not account
/// for, which are the errors that the edits introduced.
fn introduced_errors(
    mut before: HashMap<(Option<NodeId>, IssueKind), usize>,
    after: ValidationReport,
) -> Vec<ValidationIssue> {
    let mut introduced = Vec::new();
    for issue in after.errors {
        match before.get_mut(&(issue.node, issue.kind)) {
            Some(remaining) if *remaining > 0 => *remaining -= 1,
            _ => introduced.push(issue),
        }
    }
    introduced
}

impl Transaction {
    /// Encodes the transaction as Protocol Buffers bytes, as the message `Transaction`
    /// of `ironlab/ir/v0/edit.proto`.
    ///
    /// Every floating-point value, in values and in arrays, is stored as its IEEE 754
    /// bits.
    pub fn to_protobuf(&self) -> Vec<u8> {
        use prost::Message;

        wire::Transaction::from(self).encode_to_vec()
    }

    /// Decodes a transaction from Protocol Buffers bytes.
    ///
    /// Absent and unknown values follow the conventions of the figure format described
    /// in [`wire`](crate::wire). The identifiers named by an edit, the value of a set, the
    /// node of an insertion and the array of a data edit have no default, and neither
    /// does the kind of an edit, a value or a node. A value within a [`Value`] that is
    /// absent takes the [`Default`] of its type, because a value has no enclosing node to
    /// give it a context.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::Protobuf`] when the bytes are not a valid encoding of a
    /// transaction, hold an enum value that this build does not define, omit a value
    /// that has no default, or hold a property path that is not valid.
    pub fn from_protobuf(bytes: &[u8]) -> Result<Transaction, IrError> {
        use prost::Message;

        Transaction::try_from(wire::Transaction::decode(bytes)?)
    }

    /// Serialises the transaction as JSON.
    ///
    /// As in the figure format, non-finite values in data arrays are written as `null`
    /// and read back as NaN, and a non-finite value anywhere else cannot be represented.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a transaction always serialises to JSON")
    }

    /// Parses a transaction from JSON.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::Json`] when the text is not valid JSON or does not describe a
    /// transaction, including when a property path is not valid.
    pub fn from_json(json: &str) -> Result<Transaction, IrError> {
        Ok(serde_json::from_str(json)?)
    }
}

/// The tree of a figure's nodes, as the structural edits of a transaction leave it.
///
/// The overlay and selections need to know which nodes a removal takes with it at the
/// moment of the removal, so that an artist moved out of an axes earlier in the same
/// transaction is not counted among its descendants. Only the shape of the tree is
/// tracked, so following a transaction costs nothing like a copy of the figure.
pub(crate) struct NodeTree {
    /// The artists of each axes, in tree order.
    axes: Vec<(NodeId, Vec<NodeId>)>,
}

impl NodeTree {
    /// Returns the tree of a figure.
    pub(crate) fn of(figure: &Figure) -> Self {
        NodeTree {
            axes: figure
                .axes
                .iter()
                .map(|axes| (axes.id, axes.artists.iter().map(Artist::id).collect()))
                .collect(),
        }
    }

    /// Returns the node and its descendants, or nothing when the tree does not hold the
    /// node.
    pub(crate) fn subtree(&self, node: NodeId) -> Vec<NodeId> {
        for (axes, artists) in &self.axes {
            if *axes == node {
                return std::iter::once(node)
                    .chain(artists.iter().copied())
                    .collect();
            }
            if artists.contains(&node) {
                return vec![node];
            }
        }
        Vec::new()
    }

    /// Applies the structural effect of an edit; every other edit leaves the tree
    /// unchanged. Positions within a list are not tracked, because only membership and
    /// descent matter.
    pub(crate) fn apply(&mut self, edit: &Edit) {
        match edit {
            Edit::Insert { parent, node, .. } => match node {
                Node::Axes(axes) => self
                    .axes
                    .push((axes.id, axes.artists.iter().map(Artist::id).collect())),
                Node::Artist(artist) => self.push_artist(*parent, artist.id()),
            },
            Edit::Remove { node } => self.remove(*node),
            // Moving an axes only reorders the figure's list.
            Edit::Move { node, parent, .. } if !self.axes.iter().any(|(id, _)| id == node) => {
                self.remove(*node);
                self.push_artist(*parent, *node);
            }
            _ => {}
        }
    }

    fn push_artist(&mut self, parent: NodeId, artist: NodeId) {
        if let Some((_, artists)) = self.axes.iter_mut().find(|(id, _)| *id == parent) {
            artists.push(artist);
        }
    }

    fn remove(&mut self, node: NodeId) {
        if let Some(position) = self.axes.iter().position(|(id, _)| *id == node) {
            self.axes.remove(position);
            return;
        }
        for (_, artists) in &mut self.axes {
            artists.retain(|id| *id != node);
        }
    }
}
