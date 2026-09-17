# ADR 0006: Interaction mutates the IR

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0002](0002-egui-viewer-on-rerun-family-crates.md), [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0007](0007-mvp-scope.md)

## Context

The viewer lets users pan, zoom and rotate axes and hide plots from the legend, and then export the figure. An interactive viewer can hold such changes in two ways: as view state private to the viewer, applied on top of the figure when drawing, or as edits to the figure itself.

Private view state has to be applied again by every consumer that must reflect it, including the exporter. It is lost when a figure is saved, and it creates a second description of the figure that can disagree with the first. MATLAB takes the other approach: zooming an axes sets its `XLim` and `YLim` properties, exactly as a script would.

Linked axes add a further requirement. MATLAB's `linkaxes` keeps the limits of a set of axes equal, and IronLAB supports linking arbitrary sets of axes per dimension, including sets that are linked in several separate calls and turn out to overlap.

## Decision

Every viewer interaction is an edit of the figure IR.

- **Edits.** Panning, wheel zooming and box zooming a two-dimensional axes set its axis limits to manual values. Panning, zooming and rotating a three-dimensional axes change its `view3d` (azimuth, elevation, zoom and pan). Clicking a legend entry toggles the artist's `visible` flag. A gesture on an axes with automatic limits starts from the limits that the scene compiler resolved.
- **Pure interaction logic.** The mapping from gestures to edits lives in a module with no GPU or window dependency (`ironlab_viewer::interaction`), which takes figure-space positions and the compiled hit map. After each edit the scene is recompiled, and the next gesture is hit-tested against the new geometry.
- **Snapshot reset.** Each tab keeps a snapshot of the figure as it was opened. Reset view (and the R key) restores the limits and three-dimensional views of every axes from the snapshot, and double-clicking restores those of one axes. Neither changes artist visibility, because hiding a plot is a deliberate choice about content rather than a view.
- **Export of the current IR.** Export PDF writes the current figure, so what is on screen is what is exported.
- **Linked axes by union-find.** Links are stored in the IR as groups of axes per dimension. Linking axes that belong to existing groups for that dimension merges the groups, so the groups for a dimension are always disjoint connected components; stored groups that overlap, as a hand-edited file may contain, are treated as one. Every limit change, whether from the API or from the viewer, goes through `Figure::set_limits`, which applies the limits to every axes in the group.
- **Automatic limits over link groups.** The scene compiler computes automatic limits over the data of every axes in the same link group, so that linked axes agree before any limits are set and a gesture on one of them starts from limits that its group shares.
- **Hidden artists keep their influence.** A hidden artist still contributes to automatic limits and keeps its colour, so toggling visibility never moves, rescales or recolours anything else.

## Consequences

- Viewer changes, API changes and in future property edits over a socket are the same kind of operation on the same model, with one implementation of linking.
- A figure can be exported or saved at any moment of an interactive session and reproduces the view on screen.
- The interaction logic is unit-tested without a GPU: for example, that wheel zoom keeps the data point under the pointer fixed on linear and logarithmic axes, and that panning a linked axes moves its group and nothing else.
- Panning or zooming replaces automatic limits with manual ones, so adding data to a figure after interacting with it does not rescale those axes until their view is reset.
- Recompiling the scene after every edit costs time in proportion to the figure's complexity. This is acceptable for the MVP canvas and is revisited when custom GPU pipelines apply pan and zoom as transforms (see [ADR 0003](0003-shared-scene-compiler-and-display-list.md)).
- Three-dimensional views are per axes and are not linked; only limits are linked.

The behaviour is described for users in [using the viewer](../guides/viewer.md).
