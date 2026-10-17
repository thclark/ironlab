# ADR 0018: Legend panels in the tile layout, with entries merged by name and look

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0008](0008-typed-edits-and-a-view-overlay.md), [ADR 0012](0012-empty-and-singleton-data.md), [ADR 0015](0015-figures-are-described-by-their-authors.md)

## Context

Before this decision, a legend belonged to one axes and was drawn inside its plot area. It listed every artist of the axes that had a display name, in a single column, at one of nine locations. The tile layout divided the page into rows and columns of equal size, and an axes could span several of them but could not make one row or column larger than another.

That arrangement fails a common kind of figure: a grid of subplots that show the same series under different conditions. Each subplot repeats the same legend, which wastes space and covers data in every one of them, and the usual remedy in other plotting libraries, a single legend placed beside or beneath the grid, could not be expressed. The remedy has three parts that the model lacked: a legend that is not inside any axes, a way for several axes to send their entries to it without each series being listed once per axes, and a way to give that legend a row or column smaller than those holding the plots. A legend with many entries also needs several columns, which an inside legend lacked as well.

## Decision

### A legend is either inside its axes or in a panel

The `legend` of an axes is an enumeration, `Legend`, with two variants. `Legend::Inside(InsideLegend)` draws the entries inside the plot area, as before, with a location, a box and an entry layout. `Legend::Panel(NodeId)` lists the entries in the legend panel with that identifier. An axes therefore shows its entries in exactly one place, and an axes without a legend has `None`.

A **legend panel**, `PanelLegend`, is a node of the figure, listed in `Figure.panel_legends`. It has an identifier, a `Cell` of the tile layout, an alignment, a border and an entry layout. A panel occupies its cells exactly as an axes does, so the tile layout remains the only mechanism that places anything on the page. Panels are drawn after every axes, in the order the figure lists them.

The legend location gains the value `Center`. A panel aligns its block of entries by a legend location, and the centre is the most common alignment for a panel. Inside a plot area `Center` is allowed but covers the data. A panel holds no data for its entries to avoid, so `Best` in a panel means `Center`.

### Panels line up with the plot areas around them

A panel's area is the rectangle of its cells inset by the aligned margins of the rows and columns it occupies. Those margins are the space the axes in each row and column reserve for tick labels, axis labels and titles, so a panel beneath two plots spans exactly the width of their plot areas, and a panel beside a column of plots spans exactly their height. On a side whose row or column holds no axes, the page padding is used instead. A panel contributes no margins of its own, so adding a panel never moves a plot area.

A panel is outlined by a border by default. The border is drawn by the same code as the box of an axes, with the same stroke and no background, around the panel's area. Because that area is aligned with the plot areas around it, the edges of the border continue the edges of the neighbouring axes boxes, and the panel reads as one more tile of the grid of plots. The entries are kept inside the border by the inset that an inside legend keeps from the box of its axes. A bordered panel with no entries still draws its border, because the author placed the panel and validation already reports that it is empty. Setting `boxed` to false removes the border, and the block of entries then sits flush with the edges it is aligned to.

The property keeps the name `boxed` of an inside legend, because in both cases it decides whether the legend is framed. It frames different things: an inside legend draws a background and an outline around its block of entries, which must hide the data beneath it, whereas a panel outlines its whole area, which covers no data.

### Entries are merged when their name and their look both match

A panel gathers the entries of every axes that names it. Two entries are merged into one when their display names are equal, in both source text and interpreter, and their samples draw the same picture. The sample is computed by the scene compiler from what it draws for the entry: the kind of sample (line, marker, arrow or patch) and its resolved colours, line width, dash pattern, marker shape, marker size and edge width. A merged entry is drawn from the first of its artists in the order in which the panel lists entries, described below, and it stands for every artist it merges.

Entries with the same display name and a different look are all listed, and the scene compiler reports one `LegendNameClash` warning for that name, because a reader cannot tell which entry belongs to which series.

### Entries are listed in the reading order of the layout

A panel lists the entries of its axes in reading order. The axes are ordered by the top-left tile of their cells, clamped into the layout as the scene compiler places them, first by row and then by column; axes whose cells start at the same tile keep the order in which the figure lists them, which is the order in which they were created. Within each axes the entries follow the order of its artists. A merged entry takes the position of its first occurrence in this order. An inside legend lists the entries of its own axes, in artist order, and merges nothing.

Readers expect the entries of a shared key to follow the layout of the page, which they read from the top left, and not the order in which the code happened to create the axes. A figure whose code builds the right-hand column first, or fills a grid column by column, would otherwise list its series in an order that contradicts the picture the key describes.

### Entries are arranged in columns, in the same way for both kinds of legend

`LegendEntryLayout` arranges the entries of an inside legend and of a panel alike. Its `columns` are either `Count(n)`, a fixed number of at least one, or `Auto`, the fewest columns in which the block of entries fits the height available, which is the plot height less an inset for an inside legend and the panel height for a panel. Its `order` is `ColumnMajor`, which fills each column from the top before starting the next, or `RowMajor`, which fills each row from the left. An inside legend defaults to one column and a panel to automatic columns, both in column-major order.

A block of entries that does not fit the area available, even in as many columns as fit its width, raises a `LegendOverflow` warning. The block is drawn from the top-left corner of the area, clipped to it, and the entries that do not lie wholly inside the area are left out. The figure is not refused, because the warning states what is missing and the author can enlarge the panel or the figure.

### Rows and columns have weights

`TileLayout` gains `row_weights` and `col_weights`. The rows share the height of the page below the figure title in proportion to their weights, and the columns share its width in the same way. A row or column without a weight, because the list is shorter than the number of rows or columns, has a weight of 1, so empty lists give the equal tiles of earlier versions. A list longer than the number of rows or columns, or a weight that is not finite and positive, is a validation error. A cell that spans several rows or columns takes the sum of their weights, so an axes spanning two wide columns beside a narrow column for a panel is expressed without any further mechanism.

### Clicking a merged entry acts on every artist it stands for

Each legend entry in the hit map records every `(axes, artist)` pair it stands for, not one. Clicking an entry in the viewer hides all of those artists unless every one of them is already hidden, in which case it shows them all, so a series shared by several subplots is hidden or shown in all of them with one click. An entry is greyed only when all of its artists are hidden. The click is one transaction of typed edits, as [ADR 0008](0008-typed-edits-and-a-view-overlay.md) requires, and is undone in one step.

### Overlapping cells are reported

Validation reports the warning `OverlappingCells` whenever the cells of two nodes of the tile layout, axes or legend panels, share at least one tile. The later node is drawn over the earlier, so a panel placed over an axes hides part of the plot, which is almost always a mistake in the code that placed one of them. The figure can still be drawn, so the overlap is a warning rather than an error, consistent with the approach of [ADR 0012](0012-empty-and-singleton-data.md) to figures that are valid but questionable. Each overlapping pair is reported once, against the later of its two nodes, where the axes come before the legend panels and each are taken in the order the figure lists them, and the message names both nodes. A cell outside the tile layout is already the error `CellOutOfLayout`, and where it would be drawn is not defined by the figure, so it is not compared with any other cell.

### An empty panel is valid

A panel that no axes names, or whose axes have no artist with a display name, is valid and draws nothing. Validation reports it with the `NothingToDraw` warning, consistent with [ADR 0012](0012-empty-and-singleton-data.md). A legend that names a panel the figure does not have is a validation error, `DanglingPanelLegend`, because the reference is wrong rather than merely empty. A fixed column count of zero is the error `InvalidLegendColumns`, and invalid weights are the error `InvalidLayoutWeights`.

### What the property editor may change

The alignment, border and entry layout of a panel, the entry layout of an inside legend and the tile weights are editable in the viewer's property editor. The cell of a panel and the panel an axes names are read-only, because they are part of the arrangement of the figure, which is built by code and only viewed in the viewer.

### `Cell` and `TileLayout` move to a module of their own

`Cell` and `TileLayout` move from the axes and figure modules to a new `layout` module of the `ironlab-ir` crate, with a matching Protocol Buffers file. A legend panel needs a `Cell`, and an axes needs a `Legend`. With `Cell` defined alongside the axes, the generated legend and axes Protocol Buffers files would each import the other, which Protocol Buffers does not allow; a separate layout file that both import removes the cycle.

## Alternatives considered

### The panel lists its axes

A panel could have held the identifiers of the axes whose entries it lists, leaving the legend of each axes unchanged. This was rejected because it lets an axes both keep a legend inside its plot area and feed a panel, and lets two panels each claim the same axes. Each of those states is ambiguous: it is unclear whether the author meant the entries to appear twice, and nothing in the model says which placement is intended. Making the axes point at the panel, through a variant of its own legend, makes one destination per axes a property of the type rather than a rule that validation must enforce.

### Merging by display name alone

Merging every pair of entries with the same display name would have been simpler, and it is what a reader usually expects when the same series appears in several subplots. It was rejected because it hides genuinely different series. Two artists with the same name but different colours or markers would be listed once, with the look of the first, and the second series would have no entry that matches what is drawn. Requiring the look to match as well merges exactly the entries that a reader could not tell apart, and the clash warning reports the remaining case, in which the names match and the looks do not.

### An explicit list of entries in the panel

A panel could have held its own list of entries, each naming an artist and a label. This was rejected because the labels would duplicate the display names of the artists and go stale when a display name is edited, in code or in the property editor. The merge rule does not conflict with [ADR 0015](0015-figures-are-described-by-their-authors.md), which forbids the viewer from deriving descriptions of a figure, because the outcome is fully determined by what the author wrote: the author chooses the display names and the panel each axes names, the scene compiler applies one fixed rule to them, and a different grouping is obtained by naming the series differently.

## Consequences

- The schema version moves from 0.3.1 to 0.4.0, a breaking change. The `legend` of an axes is serialised as `{"type": "inside", "value": {...}}` or `{"type": "panel", "value": <id>}` instead of a plain object, and a file of version 0.3 is rejected. Figures written with earlier versions must be regenerated, which is acceptable because the project has no users who depend on the earlier format.
- A figure that sets no panel, no weights and no entry layout draws exactly as before.
- Colourbars are deliberately not covered. A colourbar is another figure-level key that would occupy cells of the tile layout, and the panel mechanism is a likely basis for it, but its design is left to a later decision.
- The order of the entries of a panel depends on the cells of its axes, so moving an axes to another tile can change the order of the entries of its panel, while reordering the axes in the figure changes it only for axes whose cells start at the same tile.
- A figure in which an axes or a panel is drawn over another, which earlier versions drew without comment, now raises the warning `OverlappingCells`.
- The rules of placement, merging, arrangement and overflow are stated in the documentation of the scene compiler and taught in the [legends guide](../guides/legends.md); the encoding is described in the [figure schema reference](../reference/figure-schema.md#legend-panels).
