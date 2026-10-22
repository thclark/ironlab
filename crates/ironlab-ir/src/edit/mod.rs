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
//! `projection.view3d.azimuth_deg`, `legend.inside.location`, ...), the variant struct
//! of an artist (`line.width_pt`, `visible`, `levels`, ...), a panel (`align`,
//! `content.legend.layout.columns`, ...) or a data array (`name`). A path descends into a
//! tagged value by naming a field of the variant that is currently set, and into an
//! optional value only while it is present. The settable paths of each kind of node are
//! listed by [`properties`].
//!
//! Identifiers (`id`), the schema version (`schema_version`), the provenance
//! (`provenance`), the node lists (`data`, `axes`, `artists` and `panels`) and the values
//! of a data array (`array`) are read-only: nodes change only through the structural
//! edits, and the values of a data array only through the data edits.
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
pub(crate) mod walk;

pub use crate::nodes::{Node, NodeKind};
pub use choice::{Choice, choices, property_choices};
pub use path::{PathError, PropertyPath};
pub use registry::{Property, properties};
pub use value::{Value, ValueType};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use std::collections::{BTreeSet, HashMap};

use crate::data::{DataArray, NdArray, NdArrayElement, Values};
use crate::error::IrError;
use crate::figure::Figure;
use crate::ids::NodeId;
use crate::nodes::{TreeNode, address, at, at_mut, subtree_ids};
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
    /// Inserts a node with its subtree: a data array, an axes or a panel into the figure,
    /// or an artist into an axes.
    Insert {
        /// The parent: the figure for a data array, an axes or a panel, or an axes for an
        /// artist.
        parent: NodeId,
        /// The position in the parent's list of the node's kind at which the node is
        /// inserted, from zero to the length of the list, or `None` to append it.
        index: Option<u32>,
        /// The node, whose identifier and whose descendants' identifiers must not be in
        /// use in the figure.
        node: Node,
    },
    /// Removes a node and its subtree.
    Remove {
        /// The data array, axes, artist or panel to remove.
        node: NodeId,
    },
    /// Moves a data array, an axes or a panel within the figure's list of its kind, or an
    /// artist within its axes or to another axes.
    Move {
        /// The data array, axes, artist or panel to move.
        node: NodeId,
        /// The new parent: the figure for a data array, an axes or a panel, or an axes for
        /// an artist.
        parent: NodeId,
        /// The position in the new parent's list after the node has been removed from
        /// its old position, from zero to the length of that list, or `None` to append.
        index: Option<u32>,
    },
    /// Replaces the values of an existing data array, which keeps its identifier and its
    /// name, so that every artist that refers to it draws the new values.
    ReplaceData {
        /// The data array.
        node: NodeId,
        /// The new values.
        array: NdArray,
    },
    /// Appends entries to an existing data array along its first dimension.
    AppendData {
        /// The data array.
        node: NodeId,
        /// The entries to append, whose shape after the first dimension and whose
        /// element type must equal those of the existing array.
        array: NdArray,
        /// When set, only the last `retain` entries along the first dimension are kept
        /// after appending, which gives a rolling window for streamed data.
        retain: Option<u64>,
    },
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

    /// The identifier of a data edit does not refer to a node of the figure.
    #[error("edit {edit:?}: {node} is not in the figure")]
    UnknownData {
        /// The index of the edit.
        edit: Option<usize>,
        /// The identifier that was not found.
        node: NodeId,
    },

    /// The identifier of a data edit refers to a node of the figure that is not a data
    /// array, such as an axes.
    #[error("edit {edit:?}: {node} is not a data array")]
    WrongNodeKind {
        /// The index of the edit.
        edit: Option<usize>,
        /// The identifier of the node of the wrong kind.
        node: NodeId,
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
    /// version, the provenance, a node list or the values of a data array, or a path
    /// below one of them.
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

    /// The parent of an inserted or moved node cannot hold it: a data array, an axes and
    /// a panel must be placed in the figure, and an artist in an axes.
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
        "edit {edit:?}: entries of shape {appended:?} cannot be appended to {node} of shape {existing:?}"
    )]
    ShapeMismatch {
        /// The index of the edit.
        edit: Option<usize>,
        /// The data array appended to.
        node: NodeId,
        /// The shape of the existing array.
        existing: Vec<usize>,
        /// The shape of the appended entries.
        appended: Vec<usize>,
    },

    /// Appended entries hold values of another element type than the existing array,
    /// and could only be stored by converting them.
    #[error(
        "edit {edit:?}: entries of {appended} values cannot be appended to {node}, which holds {existing} values"
    )]
    ElementMismatch {
        /// The index of the edit.
        edit: Option<usize>,
        /// The data array appended to.
        node: NodeId,
        /// The element type of the existing array.
        existing: NdArrayElement,
        /// The element type of the appended entries.
        appended: NdArrayElement,
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
            | EditError::WrongNodeKind { edit, .. }
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
            | EditError::ElementMismatch { edit, .. }
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
        self.tree_node(node)
            .ok_or(EditError::UnknownNode { edit: None, node })?
            .get(path.segments())
            .map_err(|step| self.edit_error(None, node, path, step))
    }

    /// Returns the kind of the node with the given identifier, or `None` when the
    /// identifier is not a node of the figure.
    pub fn node_kind(&self, node: NodeId) -> Option<NodeKind> {
        self.tree_node(node).map(TreeNode::kind)
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
            Edit::ReplaceData { node, array } => {
                let data = self.data_array_for_edit(*node, index)?;
                let old = std::mem::replace(&mut data.array, array.clone());
                Ok(Edit::ReplaceData {
                    node: *node,
                    array: old,
                })
            }
            Edit::AppendData {
                node,
                array,
                retain,
            } => self.apply_append_data(*node, array, *retain, index),
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
        let old = self.get(node, path).map_err(|error| error.at_edit(index))?;
        let written = self
            .tree_node_mut(node)
            .expect("the node was just read")
            .set(path.segments(), value);
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
        at_index: Option<u32>,
        node: Node,
        index: Option<usize>,
    ) -> Result<Edit, EditError> {
        let id = node.id();
        let parent_at = address(self, parent).ok_or(EditError::UnknownNode {
            edit: index,
            node: parent,
        })?;
        let kind = node.kind();
        let lists = at(self, &parent_at).lists();
        let Some(list) = lists.iter().position(|list| list.holds(kind)) else {
            return Err(EditError::InvalidParent {
                edit: index,
                node: id,
                parent,
            });
        };
        let len = lists[list].count();
        let mut used: BTreeSet<NodeId> = self.node_ids().into_iter().collect();
        let ids = subtree_ids(node.tree());
        for &new in &ids {
            if !used.insert(new) {
                return Err(EditError::DuplicateId {
                    edit: index,
                    node: new,
                });
            }
        }
        let position = checked_position(at_index, len, parent, index)?;
        at_mut(self, &parent_at).lists_mut()[list].insert(position, node);
        for new in ids {
            self.id_allocator.next = self.id_allocator.next.max(new.0.saturating_add(1));
        }
        Ok(Edit::Remove { node: id })
    }

    fn apply_remove(&mut self, node: NodeId, index: Option<usize>) -> Result<Edit, EditError> {
        if node == self.id {
            return Err(EditError::RootNode { edit: index, node });
        }
        let mut node_at =
            address(self, node).ok_or(EditError::UnknownNode { edit: index, node })?;
        let (list, position) = node_at.pop().expect("only the root has an empty address");
        let parent = at(self, &node_at).id();
        let removed = at_mut(self, &node_at).lists_mut()[list].remove(position);
        Ok(Edit::Insert {
            parent,
            index: Some(as_index(position)),
            node: removed,
        })
    }

    fn apply_move(
        &mut self,
        node: NodeId,
        parent: NodeId,
        at_index: Option<u32>,
        index: Option<usize>,
    ) -> Result<Edit, EditError> {
        if node == self.id {
            return Err(EditError::RootNode { edit: index, node });
        }
        let mut node_at =
            address(self, node).ok_or(EditError::UnknownNode { edit: index, node })?;
        let parent_at = address(self, parent).ok_or(EditError::UnknownNode {
            edit: index,
            node: parent,
        })?;
        let kind = at(self, &node_at).kind();
        let lists = at(self, &parent_at).lists();
        let Some(list) = lists.iter().position(|list| list.holds(kind)) else {
            return Err(EditError::InvalidParent {
                edit: index,
                node,
                parent,
            });
        };
        let (from_list, from) = node_at.pop().expect("only the root has an empty address");
        let from_parent = at(self, &node_at).id();
        let same_list = from_parent == parent && from_list == list;
        let len = lists[list].count() - usize::from(same_list);
        let to = checked_position(at_index, len, parent, index)?;
        let moved = at_mut(self, &node_at).lists_mut()[from_list].remove(from);
        // The removal shifts no ancestor of the new parent, because only the figure holds
        // more than one level of nodes, but the parent is found again so that this holds
        // whatever the shape of the tree.
        let parent_at = address(self, parent).expect("the parent is still in the figure");
        at_mut(self, &parent_at).lists_mut()[list].insert(to, moved);
        Ok(Edit::Move {
            node,
            parent: from_parent,
            index: Some(as_index(from)),
        })
    }

    /// Returns the data array that a data edit names, or the error of an identifier that
    /// names no node or a node of another kind.
    fn data_array_for_edit(
        &mut self,
        node: NodeId,
        index: Option<usize>,
    ) -> Result<&mut DataArray, EditError> {
        if self.data_array(node).is_none() {
            return Err(match self.node_kind(node) {
                Some(_) => EditError::WrongNodeKind { edit: index, node },
                None => EditError::UnknownData { edit: index, node },
            });
        }
        Ok(self
            .data_array_mut(node)
            .expect("the data array was just found"))
    }

    fn apply_append_data(
        &mut self,
        node: NodeId,
        array: &NdArray,
        retain: Option<u64>,
        index: Option<usize>,
    ) -> Result<Edit, EditError> {
        let existing = &mut self.data_array_for_edit(node, index)?.array;
        if existing.element() != array.element() {
            return Err(EditError::ElementMismatch {
                edit: index,
                node,
                existing: existing.element(),
                appended: array.element(),
            });
        }
        let mismatched = existing.shape.is_empty()
            || array.shape.is_empty()
            || existing.shape[1..] != array.shape[1..];
        if mismatched {
            return Err(EditError::ShapeMismatch {
                edit: index,
                node,
                existing: existing.shape.clone(),
                appended: array.shape.clone(),
            });
        }
        let old = existing.clone();
        existing.shape[0] += array.shape[0];
        match (&mut existing.values, &array.values) {
            (Values::F64(values), Values::F64(appended)) => values.extend_from_slice(appended),
            (Values::U8(values), Values::U8(appended)) => values.extend_from_slice(appended),
            (Values::F64(_), Values::U8(_)) | (Values::U8(_), Values::F64(_)) => {
                unreachable!("the element types were compared above")
            }
        }
        if let Some(retain) = retain {
            let kept = usize::try_from(retain).unwrap_or(usize::MAX);
            if kept < existing.shape[0] {
                let discarded = existing.shape[0] - kept;
                let entry: usize = existing.shape[1..].iter().product();
                match &mut existing.values {
                    Values::F64(values) => {
                        values.drain(..discarded * entry);
                    }
                    Values::U8(values) => {
                        values.drain(..discarded * entry);
                    }
                }
                existing.shape[0] = kept;
            }
        }
        Ok(Edit::ReplaceData { node, array: old })
    }
}

/// Returns the position at which a node is inserted into a list of `len` nodes: the
/// position that an edit gives, or the end of the list when it gives none.
fn checked_position(
    at_index: Option<u32>,
    len: usize,
    parent: NodeId,
    index: Option<usize>,
) -> Result<usize, EditError> {
    let position = at_index.unwrap_or_else(|| as_index(len));
    if position as usize > len {
        return Err(EditError::IndexOutOfRange {
            edit: index,
            parent,
            index: position,
            len,
        });
    }
    Ok(position as usize)
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
