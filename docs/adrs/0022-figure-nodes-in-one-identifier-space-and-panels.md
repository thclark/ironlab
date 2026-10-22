# ADR 0022: Figure nodes in one identifier space, and panels that hold content

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0008](0008-typed-edits-and-a-view-overlay.md) and [ADR 0018](0018-legend-panels-with-entries-merged-by-name-and-look.md), each of which this record supersedes in part, and [ADR 0012](0012-empty-and-singleton-data.md)

## Context

[ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md) keeps numeric data in a table of arrays keyed by `DataId`, a second identifier space beside the `NodeId` of axes and artists. An array had no identity of its own beyond its key: it could not be named, it could not be shown in the inspector, and an edit that created, replaced or removed one went through dedicated edits, `PutData` and `RemoveData`, rather than the structural edits of [ADR 0008](0008-typed-edits-and-a-view-overlay.md). Although an artist referred to its arrays by identifier, the facade copied every slice it was given into a new array, so two artists plotted against one time base stored that time base twice ([issue #26](https://github.com/thclark/ironlab/issues/26)).

[ADR 0018](0018-legend-panels-with-entries-merged-by-name-and-look.md) added the legend panel, `PanelLegend`, as a node of the figure that occupies cells of the tile layout. A colourbar is another key that occupies cells, and ADR 0018 named the panel mechanism as its likely basis. A second node kind beside `PanelLegend` would have duplicated the fitting of tracks, placement within a cell, overlap checks and inspector grouping that every such key needs.

Each new kind of top-level node also had to be added by hand to about a dozen places that switched on the kind of a node: the lookup of a node by identifier, the structural edits, the tree that the overlay and selection walk, the registry of property paths, the wire conversion and validation. A kind left out of one of them still compiled.

## Decision

### Every node of the figure shares one identifier space

The figure holds its top-level nodes in one typed list per kind: `data` (data arrays), `axes` and `panels`. Each node carries a `NodeId` from the single identifier space of the figure, and every reference between nodes is a `NodeId`. `DataId` is removed.

A **data array**, `DataArray`, holds an identifier, an optional `name` and the array itself. The name identifies the array to tools such as the inspector and is never drawn, following [ADR 0023](0023-titles-labels-names-and-tags.md). Several artists may refer to one data array, in any role, and editing that array changes every artist that refers to it.

### Data arrays and panels change through the structural edits

`Insert`, `Remove` and `Move`, with the figure as parent, create, delete and reorder a node of any top-level kind, data arrays and panels included. `PutData` is replaced by `ReplaceData { node, array }`, which replaces the array of an existing data array and never creates one, and `AppendData` addresses its array by `NodeId` with its existing semantics. `RemoveData` is removed, because `Remove` covers it. An edit that names a node of the wrong kind, such as `ReplaceData` naming an axes, is refused with `WrongNodeKind`.

This supersedes the list of data edits in ADR 0008 and the keying of data by `DataId` in ADR 0001. The rest of both records stands.

### A reference to a node of the wrong kind is an error, and unused data a warning

Validation reports a reference that names no node as before, and reports a reference that names a node of the wrong kind as the error `WrongNodeKind`. This covers an artist's data references, a legend that names a panel, and a legend item that names a node that is not an artist. A data array that no artist refers to is valid, and validation reports it with the warning `UnusedData`, consistent with [ADR 0012](0012-empty-and-singleton-data.md).

### A panel is one node kind whose content decides what it draws

A **panel**, `Panel`, has an identifier, a `Cell` of the tile layout, an alignment `align` within that cell, and one `content`. Its alignment is a `PanelLocation`, which offers the nine fixed positions of a legend's location without `Best`, because a cell holds nothing for the content to avoid. Its content is a `PanelContent`, of which the only kind in this record is `Legend(PanelLegend)`; a `PanelLegend` now holds only what a legend needs (its box, entry layout and entries), because its cell and alignment belong to the panel. `Legend::Panel(NodeId)` must name a panel whose content is a legend.

Everything that depends only on a panel occupying cells is written once for every panel: fitting tracks, placement within the cell, insets by the aligned margins, hit testing and the checks `OverlappingCells` and `CellOutOfLayout`. Each kind of content supplies only its size along a track and its drawing. A colourbar, and any later key, is therefore a new kind of content rather than a new kind of node.

This supersedes the `PanelLegend` node of ADR 0018, whose rules for gathering and merging entries stand unchanged for the legend content of a panel.

### The kinds of node are declared once

A single declaration, `figure_nodes!` in `crates/ironlab-ir/src/nodes.rs`, lists every top-level node kind and every artist kind. From it are generated the `Node` and `NodeKind` types, the lookups by identifier, the property paths of every node, the structural edits, the node tree, the conversion of nodes to and from the wire format, the walk that runs each node's validation checks, and the methods shared by every artist. Adding a node kind means adding one line to the declaration and supplying that kind's own substance: its type, its properties, its wire message and its checks. A kind that lacks any of these does not compile.

## Alternatives considered

### Separate panel node kinds for legends and colourbars

A colourbar panel could have been a node kind of its own beside the legend panel. This was rejected because the two kinds would share everything except their size and drawing, and each further key would have repeated the shared parts again.

### A panel holding a list of contents

A panel could have held several contents, such as legend entries above a colourbar. This was rejected because the arrangement of several contents within one cell would need rules of its own. A legend item that shows a colour scale gives the same result within one legend, and a colourbar beside a legend is a second panel in the adjacent cell.

### Keeping data in a keyed table

The table of arrays could have been kept with `DataId` keys while the facade learnt to share them. This was rejected because an array would still have no identity of its own to name or show, and because a second identifier space needs edits, checks and wire messages of its own that the structural edits already provide for nodes.

## Consequences

- The facade's `fig.data(name, values)` adds a named data array whose identifier any plotting call accepts in place of values, and a figure that plots several series against one record stores that record once. The gallery's `decimated_timeseries` figure shares its record in this way.
- A facade setter that replaces an array deletes the old array only when nothing refers to it and it has no name. Arrays copied from values are unnamed, so they are deleted when replaced; arrays made with `fig.data` are named, so they are kept.
- The inspector shows data arrays and panels in groups of their own. A data array cannot be edited there, because its values come from the program that wrote the figure.
- Validation reports its issues in the order of the node tree.
- The wire format changes incompatibly. The figure's data field keeps its number and becomes a list of data arrays, whose identifier and array are encoded as the key and value of the earlier table were; its tags and panels take the numbers of the labels and legend panels they replace; and the numbers of the removed edit and node variants are reserved. The schema version moves with the release that carries this decision, so a file of an earlier version is rejected rather than misread.
