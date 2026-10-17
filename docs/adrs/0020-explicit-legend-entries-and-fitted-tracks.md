# ADR 0020: Explicit legend entries and rows and columns fitted to their legends

**Status:** Accepted

**Related:** [ADR 0008](0008-typed-edits-and-a-view-overlay.md), [ADR 0012](0012-empty-and-singleton-data.md), [ADR 0015](0015-figures-are-described-by-their-authors.md), [ADR 0018](0018-legend-panels-with-entries-merged-by-name-and-look.md), which this record extends, [ADR 0019](0019-axis-direction-ticks-blended-coordinates-and-aspect.md), whose division of the page this record extends

## Context

[ADR 0018](0018-legend-panels-with-entries-merged-by-name-and-look.md) made a legend list every artist with a display name, in the order in which the artists are drawn, and gave the rows and columns of the tile layout relative weights. Three kinds of figure cannot be drawn well under those rules.

The first is a reference and its variants: a baseline case drawn bold and on top of a family of perturbed cases. The reference is drawn last, so that the family never hides it, but readers expect it first in the key, on a row of its own, followed by the members in columns. Because the key followed the order of drawing, the reference was listed last, and its long label widened a whole column of members.

The second is a key that explains an encoding rather than names a series. When the shape of a marker stands for a kind of point and its colour for a surface, a key of one entry per artist repeats every combination drawn, and an entry that stands for a shape or a colour alone could only be made from a dummy artist with no data, which validation reports as having nothing to draw. ADR 0018 rejected an explicit list of entries because its labels would duplicate the display names of the artists; that objection does not apply to an item that stands for no artist, which has no display name to duplicate.

The third is any key whose size depends on its entries. A weight sizes a row as a share of the page, so a builder had to guess the height of a key, and a wrong guess silently clipped entries, because the `LegendOverflow` warning was available only from the report of an export. Builders exported trial PDF files to choose the number of columns of a key.

## Decision

### A legend lists either its automatic entries or an explicit list of items

`InsideLegend` and `PanelLegend` gain `entries`, a `LegendEntries` that is either `Auto`, the behaviour of ADR 0018, or `Explicit(Vec<LegendItem>)`, which lists exactly the given items in the given order. The order of an explicit list is independent of the order of drawing, so a reference drawn last is listed first without any notion of a drawing order separate from the order of the artists. An explicit list of no items lists nothing.

A `LegendItem` has a `content` and a `full_row` flag. Its content is `LegendItemContent::Artist(NodeId)`, which lists that artist with its display name and the sample drawn for it, or `LegendItemContent::Custom { label, sample }`, which shows a label beside a sample and stands for no artist. A `LegendSample` is either `Line { line, marker }`, drawn as the sample of a line artist is drawn, or `Swatch { color }`, a filled rectangle. The names of the new types begin with `LegendItem` and `LegendSample` because the names beginning with `LegendEntry` already describe the layout of entries.

### An artist item names any artist in a panel, and its own axes' artists inside a plot area

An artist item in a legend panel may name an artist of any axes of the figure, so that a key may list an artist whose axes sends its entries elsewhere, or that a panel no axes names may list artists at all. The item also stands for every artist of the axes that name the panel whose display name and look match the named artist, by the rule with which ADR 0018 merges entries, so that an item naming the reference of one plot in a stack hides or shows the reference in every plot of the stack when it is clicked. An artist item in a legend inside a plot area must name an artist of its own axes, and merges nothing, because such a legend describes its own axes.

An artist item that names no artist of the figure, or, inside a plot area, an artist of another axes, is the validation error `DanglingLegendItem`, because the reference is wrong rather than merely questionable; the scene compiler leaves the item out and draws the rest. An item whose content repeats that of an earlier item is the warning `DuplicateLegendItem`, and an item naming an artist without a display name, which is listed with its sample and no label, is the warning `UnnamedLegendItem`. Both are warnings rather than errors, consistent with [ADR 0012](0012-empty-and-singleton-data.md), because the legend can still be drawn and a display name can be given later, in code or in the viewer. A panel with an explicit list of no items is reported with `NothingToDraw`, and a panel with a non-empty explicit list is never empty, whether or not an axes names it.

### Hidden artists and items that stand for no artist

An artist item follows the toggle behaviour of ADR 0018: it is greyed only when every artist it stands for is hidden, and clicking it hides or shows all of them as one transaction. An item that stands for no artist has nothing to hide or show, so it is never greyed, and it is not recorded in the hit map, so that clicking it does nothing rather than appearing to fail. A line sample's automatic colour is the ink in which the axes and labels are drawn, because the item belongs to no axes whose colour order could supply one, and its marker's automatic colours take the line colour, as they do for a line artist.

### An item on a row of its own is placed at its position in the list

An item whose `full_row` is set spans every column on a row of its own, and its label sets the width of no column. The item is placed at its position in the list rather than gathered above the columns: the items between two such items form a group that fills the columns on its own, with the same number of columns and order as every other group, and the columns of all groups share their widths so that they line up. A reference listed first therefore sits above its family in columns, and an item on a row of its own in the middle of a list heads a second group, which a rule that gathered such items above the columns could not express. Automatic columns count these rows whatever the number of columns, so `full_row` items are valid with automatic columns.

### A row or column is weighted or fitted to its legends

`row_weights` and `col_weights` become lists of `TrackSize`, which is either `Weight(f64)`, as before, or `Fit`. [ADR 0019](0019-axis-direction-ticks-blended-coordinates-and-aspect.md#plot-areas-not-cells-share-the-page-by-weight) takes the aligned margins of every row and column out of the page first and lets the plot areas share what remains by weight. A fitted track takes its share of that remainder first: a fitted row takes the height of the tallest area that a legend panel lying in that row alone needs for its entries, with the inset of its border, and the page padding on a side at which no axes starts or ends, because the panel is inset by the padding there; the margins that align the panel with the plot areas around it are already taken out with every other margin. A fitted column is sized in the same way by width. The plot areas of the weighted rows then share the rest of the height in proportion to their weights, so that plot areas of equal weight remain of equal size beside a fitted track, as ADR 0019 requires. A fitted row or column in which no panel lies alone has no size. When the fitted tracks need more than the space the margins leave, they share it in proportion to their sizes, and their legends report an overflow.

In a fitted row the width of a panel is given and its height follows from its entries, so automatic columns are chosen the other way round from ADR 0018: the entries take the fewest rows that the width allows, in the fewest columns that give that many rows, which keeps the columns as evenly filled as possible. In a fitted column the height is given, and automatic columns keep the rule of ADR 0018.

The margins depend on the tick labels of the axes, whose number depends on the sizes of their cells, which depend in turn on the fitted sizes, and the width of a panel in a fitted row depends on the margins of the columns it spans. The scene compiler therefore lays the figure out, recomputes the fitted sizes from the margins found, and lays it out again until the sizes change by less than a millionth of a point, which takes two or three passes; it stops after four. The entries of every panel are chosen and their labels measured once, before the first pass, and the warnings of a repeated pass are discarded, so that a figure reports each problem once.

An axes has no content size, so an axes that lies in fitted rows alone, or in fitted columns alone, is the validation error `AxesInFittedTrack`. The figure could be drawn with the axes collapsed to nothing, but that is never what the code meant, so it is an error rather than a warning. An axes that spans a fitted track and a weighted one is valid, as a tall plot beside a key is.

### The scene warnings are available without an export

The facade gains `Figure::layout_warnings`, which compiles the scene with the bundled fonts, exports nothing and returns its warnings, `LegendOverflow` among them. It needs no graphics adapter. A builder that cannot use a fitted row can therefore choose the columns of a key, or the size of a row, without writing trial files.

### What the property editor may change

The sizes of the rows and columns stay editable, typed as a list in which the word `fit` stands for a fitted size. The explicit list of entries of a legend is not shown, because it describes which series the key explains, which is part of the description of the figure by its author and is built by code, as [ADR 0015](0015-figures-are-described-by-their-authors.md) requires; the viewer draws and toggles an explicit list exactly as an export does.

## Alternatives considered

### A drawing order separate from the order of the artists

The reference could have been listed first by giving artists a z-order, or a rank in the legend, so that drawing and listing could each be reordered. This was rejected because it adds two numbers to every artist to express what one list on the legend expresses directly, and because a rank would still give no way to list an item that stands for no artist or to set an item on a row of its own.

### Items on a row of their own gathered above the columns

All items on rows of their own could have been drawn above the grid of columns, in their order, which serves a key whose reference comes first. It was rejected because it cannot express an item in the middle of a list that heads a second family, and placing an item at its position serves both cases.

### Validating overflow

`Figure::validate` could have reported a legend whose entries do not fit. It was rejected because whether entries fit depends on text measurement, which validation of the model does not perform, and because the fitted row removes the need for most builders to ask. `layout_warnings` answers the question with the scene compiler that draws the figure.

## Consequences

- The schema changes incompatibly. The row and column sizes of a tile layout are written as tagged objects in JSON and as `TrackSize` messages in fields 5 and 6 of the `TileLayout` in Protocol Buffers, whose former fields 3 and 4 are reserved, and legends gain an `entries` property, omitted from JSON while it is automatic. The schema version moves when this work reaches `main`.
- `Legend`, `InsideLegend` and `PanelLegend` are no longer `Copy`, because an explicit list is a vector.
- A figure that sets no explicit entries and no fitted sizes draws exactly as before.
- The scene compiler chooses and measures the entries of every legend panel before the layout, and lays a figure with fitted rows or columns out more than once.
- The rules are taught in the [legends guide](../guides/legends.md#choosing-the-entries) and the encoding is described in the [figure schema reference](../reference/figure-schema.md#legend-entries).
