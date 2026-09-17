//! The view overlay: the user's changes to a figure, kept apart from its source.
//!
//! As decided in ADR 0008, a figure shown in the viewer has two parts. The **source**
//! is the figure as its owner defines it, and every edit of the owner is applied to it.
//! The [`Overlay`] is an ordered list of [`Edit::Set`] entries made by
//! the user: limits, three-dimensional views, visibility and any property changed in the
//! property editor. The viewer draws the **displayed figure**, which is the source with
//! the overlay's entries applied on top, in order, by [`Overlay::compose`].
//!
//! When a transaction of the owner is applied to the source, [`Overlay::reconcile`]
//! compares it with the overlay. An entry whose node and path the transaction does not
//! touch is kept without notice, so new data never resets a user's zoom. An entry whose
//! path overlaps a path that the transaction sets is a [`Conflict`]; the user's value
//! remains displayed until the conflict is resolved with [`Overlay::resolve`]. An entry
//! whose node the transaction removes is dropped.
//!
//! Undo and redo act on the overlay only, in steps: a gesture of many sets is one step.

use crate::edit::{Edit, EditError, NodeTree, PropertyPath, Transaction, Value};
use crate::figure::Figure;
use crate::ids::NodeId;

/// The user's changes to a figure, as an ordered list of property sets, with the undo
/// history of those changes and the conflicts with the source that await resolution.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// The entries, in the order in which they are applied.
    entries: Vec<OverlayEntry>,
    /// The conflicts that have not been resolved.
    conflicts: Vec<Conflict>,
    /// The entries before each step that can be undone, most recent last.
    undo: Vec<Vec<OverlayEntry>>,
    /// The entries before each step that can be redone, most recent last.
    redo: Vec<Vec<OverlayEntry>>,
    /// The entries before the step that is open, if any.
    open_step: Option<Vec<OverlayEntry>>,
}

/// One property set by the user.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayEntry {
    /// The node whose property is set.
    pub node: NodeId,
    /// The path of the property within the node.
    pub path: PropertyPath,
    /// The value set by the user.
    pub value: Value,
}

/// An overlay entry whose property overlaps a property that the owner set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The node of the entry and of the owner's set.
    pub node: NodeId,
    /// The path of the overlay entry.
    pub overlay_path: PropertyPath,
    /// The path set by the owner, which equals, contains or is contained by the path of
    /// the entry.
    pub source_path: PropertyPath,
}

/// How a conflict is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resolution {
    /// The owner's value is used: the overlay entry is removed.
    UseSource,
    /// The user's value is kept: the entry remains and the conflict is cleared.
    KeepMine,
}

/// The displayed figure composed from a source and an overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct Composition {
    /// The source with every applicable overlay entry applied, in order.
    pub figure: Figure,
    /// The entries that could not be applied, in overlay order, each with the reason.
    pub dropped: Vec<Dropped>,
}

/// An overlay entry that composition could not apply.
#[derive(Debug, Clone, PartialEq)]
pub struct Dropped {
    /// The entry.
    pub entry: OverlayEntry,
    /// Why it could not be applied: [`EditError::UnknownNode`] when its node no longer
    /// exists, [`EditError::UnknownPath`], [`EditError::InactiveVariant`] or
    /// [`EditError::AbsentValue`] when its path is no longer reachable,
    /// [`EditError::TypeMismatch`] when its value does not have the type of the
    /// property, and [`EditError::Invalid`] when applying it would add validation errors
    /// that the source does not have.
    pub reason: EditError,
}

impl Overlay {
    /// Creates an empty overlay with no history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the entries, in the order in which they are applied.
    pub fn entries(&self) -> &[OverlayEntry] {
        &self.entries
    }

    /// Records the sets of a transaction made by the user.
    ///
    /// Each set is recorded in order. Recording a set removes every entry of the same
    /// node whose path the new path contains (the same path or a path below it), and
    /// then appends the new entry, so that the successive sets of a drag leave one entry
    /// and setting `projection.view3d` replaces an entry for
    /// `projection.view3d.zoom`. An entry for a path above the new path is kept, and the
    /// new entry is applied after it.
    ///
    /// The types of the values are not checked here; composition drops an entry whose
    /// value does not have the type of its property.
    ///
    /// Unless a step is open (see [`Overlay::begin_step`]), the transaction is one undo
    /// step, and recording it clears the redo history.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::NotASet`], with the index of the first edit that is not a
    /// set, when the transaction holds any other kind of edit; the entries and the undo
    /// and redo history are then left unchanged.
    pub fn record(&mut self, transaction: &Transaction) -> Result<(), EditError> {
        for (index, edit) in transaction.edits.iter().enumerate() {
            if !matches!(edit, Edit::Set { .. }) {
                return Err(EditError::NotASet { edit: Some(index) });
            }
        }
        let before = self.entries.clone();
        for edit in &transaction.edits {
            let Edit::Set { node, path, value } = edit else {
                unreachable!("every edit was checked to be a set");
            };
            self.entries
                .retain(|entry| !(entry.node == *node && path.contains(&entry.path)));
            self.entries.push(OverlayEntry {
                node: *node,
                path: path.clone(),
                value: value.clone(),
            });
        }
        self.commit(before);
        Ok(())
    }

    /// Opens an undo step, so that everything recorded or reset until
    /// [`Overlay::end_step`] is undone and redone as one step, as for a drag.
    pub fn begin_step(&mut self) {
        if self.open_step.is_none() {
            self.open_step = Some(self.entries.clone());
        }
    }

    /// Closes the open undo step. A step that changed nothing adds nothing to the undo
    /// history.
    pub fn end_step(&mut self) {
        if let Some(before) = self.open_step.take() {
            self.commit(before);
        }
    }

    /// Adds the entries before a change to the undo history, unless a step is open, in
    /// which case the change belongs to that step, or unless nothing changed.
    fn commit(&mut self, before: Vec<OverlayEntry>) {
        if self.open_step.is_none() && before != self.entries {
            self.undo.push(before);
            self.redo.clear();
        }
    }

    /// Composes the displayed figure by applying the entries to a copy of the source, in
    /// order, each as a transaction of its own.
    ///
    /// An entry that cannot be applied is skipped and reported with the reason, as
    /// described in [`Dropped`]. The validation errors that an entry may not add are
    /// those that the figure composed so far does not have, so an entry is never
    /// dropped for an error that the source already has. The source and the overlay are
    /// not changed; see [`Overlay::discard`] to remove dropped entries.
    pub fn compose(&self, source: &Figure) -> Composition {
        let mut figure = source.clone();
        let mut dropped = Vec::new();
        for entry in &self.entries {
            let transaction = Transaction {
                edits: vec![Edit::Set {
                    node: entry.node,
                    path: entry.path.clone(),
                    value: entry.value.clone(),
                }],
            };
            if let Err(reason) = figure.apply(&transaction) {
                dropped.push(Dropped {
                    entry: entry.clone(),
                    reason: reason.at_edit(None),
                });
            }
        }
        Composition { figure, dropped }
    }

    /// Removes the given dropped entries from the overlay, without adding an undo step.
    ///
    /// An entry that was discarded never changed what the user saw, so the step that
    /// recorded it is spent: if the discards leave the entries as they were before the
    /// most recent step, that step is removed from the undo history, and the next undo
    /// reaches the change before it. A step some of whose entries survive is kept, so a
    /// gesture of many sets remains one step; the redo history, which recording cleared,
    /// is not restored.
    pub fn discard(&mut self, dropped: &[Dropped]) {
        self.entries
            .retain(|entry| !dropped.iter().any(|d| d.entry == *entry));
        if self.open_step.is_none() && self.undo.last() == Some(&self.entries) {
            self.undo.pop();
        }
    }

    /// Compares the overlay with a transaction of the owner, and returns the conflicts
    /// that it raises.
    ///
    /// `source` is the source before the transaction was applied to it, from which the
    /// descendants of removed nodes are found; the caller reconciles only a transaction
    /// that the source accepts (for example by reconciling a copy of the overlay and
    /// keeping it once the transaction has been applied). For each edit of the
    /// transaction, in order:
    ///
    /// - a set of a path that overlaps the path of an entry of the same node (equal to
    ///   it, above it or below it) raises a conflict, and the entry is kept; the node of
    ///   the set is compared, not its link group, so a set of another axes in the same
    ///   link group raises no conflict;
    /// - the removal of a node drops the entries of that node and of its descendants at
    ///   the time of the removal (so not of an artist that an earlier edit moved out of a
    ///   removed axes), and their conflicts, without raising a conflict, so that an entry
    ///   never applies to a different node inserted later with the same identifier; the
    ///   same entries are removed from every step of the undo and redo history, so that
    ///   undo never restores them;
    /// - data edits, insertions, moves and sets of paths that do not overlap an entry
    ///   leave the overlay unchanged.
    ///
    /// Reconciling adds no undo step. The conflicts raised are returned, each once, and
    /// are also kept in the overlay until they are resolved; a conflict equal to one that
    /// is already pending is not kept twice, so an owner that sets the same property on
    /// every frame leaves one pending conflict.
    pub fn reconcile(&mut self, source: &Figure, transaction: &Transaction) -> Vec<Conflict> {
        let mut tree = NodeTree::of(source);
        let mut raised: Vec<Conflict> = Vec::new();
        for edit in &transaction.edits {
            match edit {
                Edit::Set { node, path, .. } => {
                    for entry in &self.entries {
                        if entry.node != *node || !entry.path.overlaps(path) {
                            continue;
                        }
                        let conflict = Conflict {
                            node: *node,
                            overlay_path: entry.path.clone(),
                            source_path: path.clone(),
                        };
                        if !raised.contains(&conflict) {
                            raised.push(conflict);
                        }
                    }
                }
                Edit::Remove { node } => {
                    let removed = tree.subtree(*node);
                    let gone = |node: &NodeId| removed.contains(node);
                    self.entries.retain(|entry| !gone(&entry.node));
                    self.conflicts.retain(|conflict| !gone(&conflict.node));
                    raised.retain(|conflict| !gone(&conflict.node));
                    let steps = self.undo.iter_mut().chain(self.redo.iter_mut());
                    for step in steps.chain(self.open_step.iter_mut()) {
                        step.retain(|entry| !gone(&entry.node));
                    }
                    tree.apply(edit);
                }
                _ => tree.apply(edit),
            }
        }
        for conflict in &raised {
            if !self.conflicts.contains(conflict) {
                self.conflicts.push(conflict.clone());
            }
        }
        raised
    }

    /// Returns the conflicts that have not been resolved, in the order they were raised.
    pub fn conflicts(&self) -> &[Conflict] {
        &self.conflicts
    }

    /// Resolves a conflict: [`Resolution::UseSource`] removes the overlay entry, and
    /// [`Resolution::KeepMine`] keeps it. Either way, every conflict of that entry is
    /// cleared. Returns whether the conflict was pending.
    pub fn resolve(&mut self, conflict: &Conflict, resolution: Resolution) -> bool {
        if !self.conflicts.contains(conflict) {
            return false;
        }
        self.conflicts.retain(|other| {
            other.node != conflict.node || other.overlay_path != conflict.overlay_path
        });
        if resolution == Resolution::UseSource {
            self.entries
                .retain(|entry| entry.node != conflict.node || entry.path != conflict.overlay_path);
        }
        true
    }

    /// Removes the entry for exactly one node and path, which is what the property
    /// editor's revert control does, and returns whether there was one.
    ///
    /// Only an entry whose node and path are equal to those given is removed: an entry
    /// for a path above it (`x.limits` when `x.limits.min` is reverted) or below it
    /// (`projection.view3d.zoom` when `projection` is reverted), and every entry of
    /// another node, is kept, so that reverting one property of the editor never
    /// discards another. A conflict pending on the entry is cleared with it, because the
    /// change it concerned is gone.
    ///
    /// Unless a step is open, a revert that removes an entry is one undo step, and a
    /// revert that removes nothing adds no step and leaves the redo history alone.
    pub fn revert(&mut self, node: NodeId, path: &PropertyPath) -> bool {
        let before = self.entries.clone();
        self.entries
            .retain(|entry| !(entry.node == node && entry.path == *path));
        if self.entries.len() == before.len() {
            return false;
        }
        self.conflicts
            .retain(|conflict| !(conflict.node == node && conflict.overlay_path == *path));
        self.commit(before);
        true
    }

    /// Removes the view entries of one axes: the entries of that axes whose path is
    /// `x.limits`, `y.limits`, `z.limits` or `projection.view3d`, or below one of them.
    ///
    /// Every other entry, including the visibility of artists and entries above those
    /// paths (such as `projection`), is kept, because hiding a plot is a choice about
    /// content rather than about the view. Unless a step is open, a reset that removes an
    /// entry is one undo step, and a reset that removes nothing adds no step.
    pub fn reset_view(&mut self, axes: NodeId) {
        self.reset(|entry| entry.node == axes);
    }

    /// Removes the view entries of every axes, as [`Overlay::reset_view`] does for one.
    pub fn reset_all_views(&mut self) {
        self.reset(|_| true);
    }

    /// Removes the view entries of the nodes that the predicate accepts.
    fn reset(&mut self, node: impl Fn(&OverlayEntry) -> bool) {
        let before = self.entries.clone();
        self.entries
            .retain(|entry| !(node(entry) && is_view_property(&entry.path)));
        self.commit(before);
    }

    /// Restores the entries to their state before the most recent step, and returns
    /// whether there was a step to undo.
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            None => false,
            Some(entries) => {
                self.redo
                    .push(std::mem::replace(&mut self.entries, entries));
                true
            }
        }
    }

    /// Re-applies the most recently undone step, and returns whether there was a step to
    /// redo.
    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            None => false,
            Some(entries) => {
                self.undo
                    .push(std::mem::replace(&mut self.entries, entries));
                true
            }
        }
    }

    /// Returns whether there is a step to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Returns whether there is a step to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Returns the entries as a transaction of sets, in order, which is what saving
    /// writes into a source that the viewer owns.
    pub fn to_transaction(&self) -> Transaction {
        Transaction {
            edits: self
                .entries
                .iter()
                .map(|entry| Edit::Set {
                    node: entry.node,
                    path: entry.path.clone(),
                    value: entry.value.clone(),
                })
                .collect(),
        }
    }
}

/// Returns whether a path names a property of the view of an axes, or a property below
/// one: its limits along a dimension, or its three-dimensional camera view.
fn is_view_property(path: &PropertyPath) -> bool {
    [
        PropertyPath::of(&["x", "limits"]),
        PropertyPath::of(&["y", "limits"]),
        PropertyPath::of(&["z", "limits"]),
        PropertyPath::of(&["projection", "view3d"]),
    ]
    .iter()
    .any(|view| view.contains(path))
}
