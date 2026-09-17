# ADR 0008: Typed edits and a view overlay

**Status:** Accepted

**Supersedes:** [ADR 0006](0006-interaction-mutates-the-ir.md)

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0002](0002-egui-viewer-on-rerun-family-crates.md), [ADR 0007](0007-mvp-scope.md)

## Context

In the MVP, figures are built by a program, saved, loaded and viewed, and the viewer changes a figure by mutating its IR directly ([ADR 0006](0006-interaction-mutates-the-ir.md)). Three planned features need changes to a figure to be values in their own right rather than direct mutations:

- **A property editor**, which changes any property of any node and must offer undo and redo.
- **Sessions**, in which a running process creates a figure, shows it in a viewer, and keeps changing it (streaming new data, changing a colormap, adding a plot) while a user interacts with the same figure. Clients in other languages and a browser viewer must be able to send and apply the same changes.
- **Undo**, which needs every change to have an inverse.

Sessions also raise a question that the MVP never faced: what happens when the process changes a figure that the user has already changed. Suppose a user zooms an axes and the process then appends data or changes the colormap. Discarding the user's zoom whenever the process sends anything makes live figures unusable, and silently discarding the process's change makes the display lie about the process's state.

Two general approaches to concurrent changes were considered and rejected. Buffering the process's changes in the viewer until the user accepts them hides the process's state and accumulates without bound when data is streamed. Rebasing the user's view operations onto the process's changes with operational transforms requires rules for how each kind of operation interacts with every other kind; view operations interact nonlinearly (a change of scale can invalidate limits, and a change of projection invalidates a camera), so such rules would be large and fragile.

## Decision

### Edits

A change to a figure is an **edit**, and edits are applied in **transactions**. The edit types are Rust types in `ironlab-ir` (the source of truth, as in [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md)), and their Protocol Buffers form is generated into `ironlab/ir/v0/edit.proto`.

- `Set { node, path, value }` replaces the value at a property path of a node.
- `Insert { parent, index, node }` inserts an axes or an artist, with its subtree, into a parent at an index or at the end.
- `Remove { node }` removes a node and its subtree.
- `Move { node, parent, index }` reorders a node or moves an artist to another axes.
- `PutData { id, array }` creates or replaces a data array.
- `AppendData { id, array, retain }` appends along an array's first dimension and optionally keeps only the last `retain` entries, which gives a rolling window for streamed data.
- `RemoveData { id }` removes a data array.

The rules for edits are as follows.

- **Property paths.** A path is a dot-separated sequence of Protocol Buffers field names, such as `x.limits`, `projection.view3d.azimuth_deg` or `line.width_pt`. A path may descend into the variant of a tagged value that is currently set, and setting a path whose variant is not set is an error. The node lists (`axes`, `artists`), the data table, identifiers, the schema version and the provenance are not reachable by `Set`; nodes and data change only through the structural and data edits.
- **Typed values.** The value of a `Set` is a `Value` with one variant per IR value type and an `Unset` variant that clears an optional property. The type of a value is checked against its path when the edit is applied. A registry of property paths, generated from the IR types, gives each path's type and documentation to the property editor, and a test compares the registry with that of `main` so that renamed or removed paths are reported like other breaking changes.
- **Literal edits.** An edit changes exactly what it names and nothing else. Behaviour that depends on the state of the figure is provided by **commands**, Rust functions that read the figure and return a literal transaction: `set_limits` sets the limits of every axes in a link group, `link` sets the figure's link groups and synchronises their limits, and `reset_view` clears view changes (see below). A client that mirrors a figure therefore applies a stream of edits without reimplementing linking or any other rule.
- **Limits that a linked axes cannot show.** The axes of a link group may differ in scale, so limits that suit one member can be undrawable on another: a range that reaches zero cannot be shown on a logarithmic axis. Sharing limits is what a link means, so `link` and `set_limits` emit the limits of every axes of the group. A member that cannot show them makes the transaction fail when it is applied, and the figure is left unchanged, because transactions are atomic. Linking axes that cannot share limits, and setting limits that a partner cannot show, therefore fail loudly instead of leaving a group that is linked in name only; linking every axes of a figure can fail for the same reason, so it returns a result.
- **Identifiers from the creator.** The creator of a node or data array chooses its identifier, so that a transaction can create an axes and then refer to it. Applying an edit that reuses an identifier fails.
- **Atomic transactions.** A transaction applies all of its edits or none. Applying it returns its inverse. After the edits, the figure is validated, and the transaction is rejected, and rolled back by applying the inverses collected so far in reverse order, if validation reports an issue that the figure did not have before, because a loaded figure may already contain issues.

### Source and overlay

A figure shown in the viewer has two parts:

- The **source** is the figure as its owner defines it. The owner is the process of a session, or the viewer itself when it opens a file with no session. Every edit from the owner is applied to the source as soon as it arrives.
- The **overlay** is an ordered list of `Set` edits made by the user in the viewer: limits, three-dimensional views, visibility, and any property changed in the property editor. Successive sets of the same path in the same node collapse into one entry, so a drag leaves one entry.

The viewer draws the **displayed figure**, which is the source with the overlay's edits applied on top, in order. Because the overlay contains only `Set` edits, composition is always defined: an entry whose node no longer exists, or whose path is no longer reachable, is dropped.

When the owner's transaction is applied to the source, each overlay entry is compared with it:

- An entry whose node and path the transaction does not touch is kept without notice. New data therefore never resets a user's zoom.
- An entry whose path equals, contains or is contained by a path that the transaction sets (for example `projection` and `projection.view3d.azimuth_deg`) is a **conflict**. The user's value remains displayed, and the viewer shows a notice that names the property and offers to use the source's value or keep the user's.
- An entry whose node the transaction removes is dropped.
- If the displayed figure has validation issues that the source does not have, the overlay entries responsible are dropped and the viewer says so.

The viewer never edits data, so a data edit never conflicts with the overlay. Selections that refer to data indices are kept consistent with data edits: an append without `retain` leaves indices unchanged, an append that discards the first *k* entries shifts indices down by *k* and drops those that become negative, replacing an array keeps the selected node but drops its indices, and removing a node clears its selection.

The overlay replaces the snapshot of [ADR 0006](0006-interaction-mutates-the-ir.md):

- **Reset.** Double-clicking an axes removes that axes' view entries (limits and three-dimensional view) from the overlay, and Reset view removes the view entries of every axes. Visibility entries are kept, because hiding a plot is a choice about content rather than about the view.
- **Undo.** Undo and redo act on the overlay only. A gesture is one undo step. The user cannot undo the owner's edits.
- **Overrides.** The property editor marks the properties that the overlay overrides and offers to revert each one.
- **Export and save.** PDF export draws the displayed figure. When the viewer owns the source, saving writes the overlay into the source and clears it.

### Sessions

In a session, the host assigns each transaction a revision number and records its origin. Transactions from several owners are applied in revision order, and a later value replaces an earlier one without notice. Each viewer of a session has its own overlay. The transport, the session messages and events from the viewer (selection, picking and brushing) belong to a separate package that is decided when sockets are implemented; a way for a user to propose overlay entries back to the owner is deferred to it.

### Decisions retained from ADR 0006

The following decisions of [ADR 0006](0006-interaction-mutates-the-ir.md) are unchanged. Gesture logic lives in a module with no GPU or window dependency and is hit-tested against the most recent compilation. Link groups are disjoint per dimension, and automatic limits are computed over each group. A hidden artist still contributes to automatic limits and keeps its colour. Panning or zooming a two-dimensional axes writes manual limits that start from the limits the compiler resolved.

## Alternatives considered

- **Field-mask patches.** Following Google's AIP-134, an edit could carry a partial node message and a mask of paths. The value's type would be correct by construction, and the protocol would not change when the IR gains a type. Applying a patch requires reflection over the wire messages and a conversion of the node to its wire form and back for every edit, and one patch sets several properties, which suits undo, change notification and conflict detection less well than one edit per property.
- **A generated variant per property.** Every settable property could be a variant of a generated oneof, which `buf breaking` would check completely. The protocol would change with every new field, nested properties would need flattened names, and stable variant numbers would be hard to generate.
- **Buffering the owner's edits and rebasing the user's**, rejected for the reasons given in the context.

## Consequences

- The viewer, the property editor, the Rust API, future clients in other languages and a browser viewer all change figures through the same typed edits, and a figure can be mirrored by applying its edit stream.
- Conflict detection compares node identifiers and paths, so it is exact and cheap, and it applies to every property the IR gains without new rules.
- Conflict detection is syntactic, not semantic. An owner's edit that does not touch a path in the overlay can still make the user's view unhelpful: an axes zoomed onto a region can become empty when the data moves elsewhere. The viewer shows no notice in that case, and Reset view is the remedy. This limitation is accepted until use shows which semantic cases matter.
- The type of a `Set` value is checked when the edit is applied rather than when it is constructed, and the `Value` type gains a variant whenever the IR gains a value type.
- Undo does not reverse changes made by a session's owner, and the overlay is not saved with the figure unless the viewer owns the source.
