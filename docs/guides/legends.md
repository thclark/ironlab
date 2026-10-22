# Legends

A legend lists the plots of a figure that have a label, each beside a sample of how the plot is drawn, or the items that its author lists explicitly. IronLAB offers two kinds of legend. A legend **inside an axes** is drawn in a corner or at an edge of that axes' plot area and lists only the plots of that axes. A **legend panel** is a [panel](../reference/figure-schema.md#panels) of the figure that holds a legend: it occupies tiles of the figure's layout, as an axes does, and lists the plots of every axes that sends its entries to it, so that a grid of subplots showing the same series can share one legend instead of repeating it in every plot.

The sample of a plot that maps its data to colours, such as a filled contour, a surface or a series coloured by data, is drawn in the middle colour of its [colour scale](colour.md), and a colour scale itself is not yet shown as a key; the [colour guide](colour.md) describes scales. This guide assumes the figure, tile and axes concepts of [getting started](getting-started.md#figures-tiles-and-axes). The examples use the prelude and the following data:

```rust
use ironlab::prelude::*;

let t = linspace(0.0, 10.0, 201);
let a: Vec<f64> = t.iter().map(|t| t.sin()).collect();
let b: Vec<f64> = t.iter().map(|t| t.cos()).collect();
```

## Legends inside an axes

A plot has a legend entry only when it has a label, which is the text drawn beside its sample. An axes has no legend until one is asked for: `legend` shows a boxed legend of every labelled plot of the axes, in the order the plots were added, at the given location, and `legend_off` hides it again.

```rust
let mut fig = Figure::new();
let mut ax = fig.axes(0, 0);
ax.plot(&t, &a).label("$\\sin t$");
ax.plot(&t, &b).label("$\\cos t$");
ax.legend(LegendLocation::SouthEast);
```

### Locations

The location of a legend inside a plot area is one of the following values of `LegendLocation`:

| Location | Placement |
| --- | --- |
| `NorthEast`, `NorthWest`, `SouthEast`, `SouthWest` | The corners of the plot area. `NorthEast` is the default. |
| `North`, `South`, `East`, `West` | The centres of the edges of the plot area. |
| `Center` | The centre of the plot area, where the legend covers the data. |
| `Best` | Whichever corner overlaps the fewest drawn data points. |

The legend is kept a short distance inside the edges of the plot area at every location.

### Columns and fill order

A legend inside an axes has one column by default. `legend_columns` sets the number of columns, either a fixed count or `LegendEntryColumns::Auto`, which chooses the fewest columns for which the legend fits the height of the plot area. Calling `legend_columns` on an axes that has no legend inside its plot area gives it one at the default location.

```rust
ax.legend(LegendLocation::South)
    .legend_columns(LegendEntryColumns::Count(2));
```

Entries fill the columns in column-major order by default: each column is filled from the top before the next is started. Row-major order fills each row from the left before the next is started, so that a legend of six entries in three columns reads across in two rows. `legend_order` sets the order, and `legend_boxed(false)` draws the legend without its background and outline. Like `legend_columns`, each of these setters keeps the other settings of an existing legend inside the plot area, and gives the axes one at the default location if it has none.

```rust
ax.legend(LegendLocation::North)
    .legend_columns(LegendEntryColumns::Count(3))
    .legend_order(LegendEntryOrder::RowMajor)
    .legend_boxed(false);
```

## Legend panels

### Creating a panel

A panel is a node of the figure that occupies tiles of the layout, as an axes does, and holds one key; a legend is the only key a panel holds at present. `fig.panel(row, col)` returns the panel whose top-left tile is `(row, col)`, creating a panel that holds a legend in that single tile if there is none. `fig.panel_span(row, col, row_span, col_span)` does the same and makes the panel cover the given numbers of rows and columns. Both return a `PanelMut` handle:

| Method | Effect |
| --- | --- |
| `align(location)` | Places the content within the panel. The location is a `PanelLocation`, which offers the locations listed [above](#locations) except `Best`, because a panel holds no data for its content to avoid. The default is `Center`. |
| `legend()` | Returns a `PanelLegendMut` handle to the legend that the panel holds, whose setters are listed below. |
| `id()` | Returns the identifier of the panel, which axes use to send their entries to it. |

The setters of a `PanelLegendMut` handle can be chained:

| Method | Effect |
| --- | --- |
| `boxed(on)` | Outlines the panel with a border drawn as the box of an axes is drawn, which is the default. The area of a panel is aligned with the plot areas around it, so the border continues the edges of their boxes. `boxed(false)` removes the border. |
| `columns(columns)` | Sets the number of columns of entries. A panel uses `LegendEntryColumns::Auto` by default. |
| `order(order)` | Sets the order in which entries fill the columns, `LegendEntryOrder::ColumnMajor` (the default) or `LegendEntryOrder::RowMajor`. |
| `collect_all()` | Sends the entries of every axes that the figure has at that moment to the panel. |
| `id()` | Returns the identifier of the panel, as `PanelMut::id` does. |

```rust
let key = fig
    .panel(0, 2)
    .align(PanelLocation::NorthWest)
    .legend()
    .boxed(false)
    .id();
```

Like an axes handle, both handles borrow the figure mutably, so they are held only while the panel is being set up. The identifier returned by `id()` is kept instead, to refer to the panel later. The reasons for making a legend the content of a panel, rather than a kind of node of its own, are recorded in [ADR 0022](../adrs/0022-figure-nodes-in-one-identifier-space-and-panels.md#a-panel-is-one-node-kind-whose-content-decides-what-it-draws).

A panel is placed in its tiles whether or not an axes already occupies them. A panel whose tiles include a tile of an axes is drawn over the plot, and [`validate`](getting-started.md#validating-a-figure) reports the pair with the warning `OverlappingCells`, as it reports two axes that share a tile. The warning names both nodes, and it is removed by giving the panel tiles of its own, for example by adding a row or column to the layout.

### Sending entries to a panel

An axes sends its entries to a panel with `legend_in`, giving the identifier of the panel. The axes then draws no legend inside its plot area: an axes lists its entries in exactly one place, either inside its own plot area or in one panel.

```rust
let mut fig = Figure::new().tiles(1, 3);
let key = fig.panel(0, 2).id();
for col in 0..2 {
    let mut ax = fig.axes(0, col);
    ax.plot(&t, &a).label("Inlet");
    ax.plot(&t, &b).label("Outlet");
    ax.legend_in(key);
}
```

When every axes of the figure should share one panel, `collect_all` sends the entries of all of them at once. It affects only the axes that exist when it is called, so it is called after the axes have been created:

```rust
let mut fig = Figure::new().tiles(1, 3);
for col in 0..2 {
    fig.axes(0, col).plot(&t, &a).label("Inlet");
}
fig.panel(0, 2).legend().collect_all();
```

An identifier that names no node of the figure is reported as the error `DanglingPanelLegend` by [`validate`](getting-started.md#validating-a-figure), and one that names a node other than a panel holding a legend, such as an axes, as the error `WrongNodeKind`. A panel that lists nothing, because no axes sends entries to it or none of those axes has a labelled plot, is valid and draws nothing, and validation reports it with the warning `NothingToDraw`, as it does an artist with no data.

### How a panel lines up with the plots

The area of a panel is its block of tiles less the margins of the rows and columns it occupies. Those margins are the space that the axes in the same rows and columns reserve for their tick labels, axis labels and titles. A panel beneath two plots therefore spans exactly the width of their plot areas, and a panel beside a column of plots spans exactly their height. On a side whose row or column holds no axes, such as the top and bottom of a row that holds only a panel, the panel is inset by the page padding instead. A panel reserves no margin of its own, so adding one never moves a plot area. The block of entries is then placed within the panel's area by its alignment.

By default a panel outlines its area with a border drawn exactly as the box of an axes is drawn, so the edges of the border continue the edges of the boxes of the plots beside or above it, and the panel reads as one more tile of the grid. The entries are kept inside the border by the same inset that a legend inside a plot area keeps from the box of its axes, at every alignment. A panel with no entries still draws its border. `boxed(false)` removes the border, and the block of entries then sits flush with the edges it is aligned to.

## Sharing and merging entries

A panel gathers the entries of every axes that sends entries to it, and merges entries that a reader could not tell apart. Two entries are merged into one when both of the following hold:

- their labels are the same text, interpreted the same way; and
- their samples draw the same picture: the same kind of sample, colour, line width, dash pattern, marker shape, marker size and marker edge.

A merged entry is drawn from the first plot it stands for, and it stands for every plot it merges. Automatic colours are assigned from the colour order separately in each axes, so the first plot of every axes takes the first colour, the second plot the second colour, and so on. A series added in the same position in every axes, as in the examples above, therefore merges into a single entry without any colour being set.

Two plots with the same label but a different look are both listed, because merging them would leave one of the series without an entry that matches what is drawn. The two entries cannot be told apart by their labels, so the scene compiler reports one `LegendNameClash` warning for the label. The warning is shown in the viewer's [problems list](viewer.md#problems), and it is returned in the `scene` warnings of the report that `export_pdf` and `export_png` return. Giving the series different labels, or the same look, removes the warning.

```rust
let mut fig = Figure::new().tiles(1, 3);
let key = fig.panel(0, 2).id();
let mut left = fig.axes(0, 0);
left.plot(&t, &a).label("Model").color(Color::rgb(0.0, 0.0, 0.0));
left.legend_in(key);
let mut right = fig.axes(0, 1);
right.plot(&t, &b).label("Model").color(Color::rgb(0.8, 0.1, 0.1));
right.legend_in(key);
// The panel lists "Model" twice, in black and in red, and the export warns of the clash.
```

The entries of a panel are listed in the reading order of the page. The axes are taken by the top-left tile of their cells, row by row from the top and, within a row, column by column from the left, and the plots of each axes follow in the order they were added. Two axes whose cells start at the same tile are taken in the order they were created. The order in which the code creates the axes therefore does not matter: a figure that builds its right-hand plot first still lists the entries of the left-hand plot first. A merged entry takes the position at which its first plot appears in this order.

A legend inside an axes lists every labelled plot of that axes and merges nothing.

## Several panels

A figure may have any number of panels, and each axes chooses which of them receives its entries. A common arrangement is one panel beneath each column of a grid, when each column shows a different set of series:

```rust
let mut fig = Figure::new().tiles(3, 2).row_weights([2.0, 2.0, 1.0]);
for col in 0..2 {
    let key = fig.panel(2, col).id();
    for row in 0..2 {
        let mut ax = fig.axes(row, col);
        ax.plot(&t, &a).label(format!("Case {}", 2 * col + 1));
        ax.plot(&t, &b).label(format!("Case {}", 2 * col + 2));
        ax.legend_in(key);
    }
}
```

Each panel is aligned with the plot areas of its own column, and the two axes of a column share the entries of their panel.

## Choosing the entries

By default a legend lists every labelled plot, in the order described above, in which the plots of each axes follow the order in which they were added, which is also the order in which they are drawn. That order does not always suit the key. The reference of a family of cases, for example, is drawn last so that the other cases never hide it, but a reader expects it first in the key. An **explicit list** of entries names exactly the items that a legend lists, in the order they are given, whatever the order of drawing. There is no separate drawing order to set: a plot is drawn where it was added, and the explicit list decides the key alone.

The item setters of a panel's legend build an explicit list one item at a time. `item` appends an item that lists a plot, given its identifier, and the first call turns the automatic entries of the panel into an explicit list, so that the plots it does not name are no longer listed:

```rust
let mut fig = Figure::new().tiles(2, 1).fit_row(1);
let mut ax = fig.axes(0, 0);
let small = ax.plot(&t, &a).label("Small perturbation").id();
let large = ax.plot(&t, &b).label("Large perturbation").id();
// The reference is added last, so that it is drawn on top of the others.
let reference = ax.plot(&t, &a).label("Reference").color(Color::rgb(0.0, 0.0, 0.0)).id();
let key = fig.panel(1, 0).legend().item(reference).item(small).item(large).id();
fig.axes(0, 0).legend_in(key);
```

The key lies in a row [fitted to its legend](#fitting-rows-and-columns-to-their-legends), which is described below, so that the row is as tall as the key needs.

| Method | Effect |
| --- | --- |
| `item(artist)` | Appends an item that lists the plot with this identifier, with its label beside its sample. It also accepts a `LegendItem`. |
| `full_row(artist)` | Appends an item that lists the plot on a row of its own, spanning every column, as [described below](#items-on-a-row-of-their-own). |
| `custom_item(sample, label)` | Appends an item that shows a sample and a label and stands for no plot, as [described below](#items-that-stand-for-no-plot). |
| `entries(entries)` | Replaces the entries of the panel: `LegendEntries::Auto` restores the automatic entries, and `LegendEntries::Explicit(items)` sets a whole list of `LegendItem` values at once. |

An item in a panel may name a plot of any axes of the figure, including one that sends its own entries elsewhere. It also stands for every plot of the axes that send their entries to the panel whose label and look match it, as a merged entry does, so that an item naming the reference in one plot of a stack hides or shows the reference in every plot of the stack. Like a merged entry, the item is greyed only when every plot it stands for is hidden, and clicking it hides or shows all of them. A legend inside an axes takes an explicit list through `legend_entries`, whose items must name plots of that axes and merge nothing:

```rust
ax.legend(LegendLocation::NorthWest).legend_entries(LegendEntries::Explicit(vec![
    LegendItem::artist(reference),
    LegendItem::artist(small),
]));
```

[`validate`](getting-started.md#validating-a-figure) reports an item that names no plot of the figure, or, in a legend inside an axes, a plot of another axes, as the error `DanglingLegendItem`, and the scene compiler leaves such an item out. It reports an item that repeats an earlier item with the warning `DuplicateLegendItem`, because the legend then lists it twice, and an item naming a plot without a label with the warning `UnnamedLegendItem`, because the item then shows its sample alone. An explicit list of no items lists nothing, and validation reports such a panel with the warning `NothingToDraw`.

### Items that stand for no plot

Some keys explain an encoding rather than name a series: the shape of a marker may stand for a kind of point and its colour for a surface, whatever plot the marker belongs to. Listing one entry per combination drawn repeats every shape in every colour. An item that **stands for no plot** shows a label beside a sample that the author describes, so that each shape and each colour is explained once. A `LegendSample` is one of the following:

| Sample | Drawn as |
| --- | --- |
| `LegendSample::marker(shape, color)` | A single filled marker of the shape and colour, without a line. |
| `LegendSample::line(color)` | A solid line of the colour, without a marker. |
| `LegendSample::swatch(color)` | A filled rectangle of the colour. |
| `LegendSample::Line { line, marker }` | A line and a marker of any `LineStyle` and `MarkerStyle`, drawn as the sample of a plot is drawn. A dash of `Dash::None` draws the marker alone. An automatic line colour is the ink of the labels, and an automatic marker colour takes the line colour. |

A panel that no axes sends its entries to may list such items alone, as a second key beside the first:

```rust
let grey = Color::rgb(0.45, 0.45, 0.45);
let mut fig = Figure::new().tiles(2, 2).fit_row(1);
let curves = fig.panel(1, 0).id();
// ... plots in the axes above, which send their entries to `curves` ...
fig.panel(1, 1)
    .legend()
    .custom_item(LegendSample::marker(Marker::Square, grey), "Transition")
    .custom_item(LegendSample::marker(Marker::TriangleUp, grey), "Separation")
    .custom_item(LegendSample::swatch(Color::rgb(0.85, 0.55, 0.0)), "Upper surface")
    .custom_item(LegendSample::swatch(Color::rgb(0.0, 0.55, 0.4)), "Lower surface")
    .columns(LegendEntryColumns::Count(2));
```

An item that stands for no plot is never greyed, and clicking it in the viewer does nothing, because there is nothing for it to hide or show.

### Items on a row of their own

An item on a row of its own spans every column of the legend, and its label sets the width of no column, so a long label, such as that of a reference case, neither widens a column of short labels nor needs a column to itself. The item is placed at its position in the list: the items before it fill the columns above it, and the items after it fill the columns below it. Each run of items between two items on rows of their own is a group that fills the columns on its own, in the order of the legend, and the columns of every group share their widths, so that they line up. A reference listed first on a row of its own therefore sits above the members of its family in columns, and an item on a row of its own in the middle of a list heads a second group.

```rust
let mut panel = fig.panel(1, 0);
let mut key = panel.legend();
key.full_row(reference);
for &level in &levels {
    key.item(level);
}
key.columns(LegendEntryColumns::Count(3));
```

`LegendItem { full_row: true, ..LegendItem::custom(sample, label) }` puts an item that stands for no plot on a row of its own in the same way. When automatic columns are chosen, each item on a row of its own counts as one row whatever the number of columns, so such items may be used with automatic columns.

## Sizing rows and columns with weights

The tiles of a figure are of equal size by default, which suits a grid of plots but makes a panel as large as a plot. Weights give rows and columns different sizes. `row_weights` sets the relative heights of the rows, from the top, and `col_weights` the relative widths of the columns, from the left. The space that the axes of each row and column need for their tick labels, axis labels and titles is set aside first, and the plot areas of the rows then share the height that remains in proportion to their weights, and those of the columns the width, so that plot areas of equal weight are of equal size whatever is written beside them. A legend panel in a row or column without axes counts as a plot area of its weight. [ADR 0019](../adrs/0019-axis-direction-ticks-blended-coordinates-and-aspect.md#plot-areas-not-cells-share-the-page-by-weight) records why.

```rust
// The plots of the top row are three times the height of the panel in the row below.
let fig = Figure::new().tiles(2, 2).row_weights([3.0, 1.0]);
```

A row or column without a weight, because fewer weights than rows or columns are given, has a weight of 1, so `col_weights([2.0])` in a layout of three columns makes the plot area of the first column twice as wide as each of the others. A weight must be a finite positive number, and there must be no more weights than rows or columns; [`validate`](getting-started.md#validating-a-figure) reports any other list as the error `InvalidLayoutWeights`.

An axes or panel that spans several rows or columns takes the shares of all of them, together with the margins between them. This makes it possible to combine a wide plot with a narrow panel in one row. In the following figure, the axes spans the first two columns and its plot area is four times as wide as the panel in the third column:

```rust
let mut fig = Figure::new().tiles(1, 3).col_weights([2.0, 2.0, 1.0]);
let key = fig.panel(0, 2).align(PanelLocation::West).id();
let mut ax = fig.axes_span(0, 0, 1, 2);
ax.plot(&t, &a).label("Inlet");
ax.plot(&t, &b).label("Outlet");
ax.legend_in(key);
```

Weights apply to every axes and panel of the figure, not only to legends, and the same layout can make one plot of a grid larger than the others.

## Fitting rows and columns to their legends

A weight sizes a row as a share of the page, so the height that a key needs must otherwise be guessed, and a wrong guess either clips the key or leaves space empty. A **fitted** row takes the height of the legend panels in it instead. The margins of the axes are set aside first, as for weights, the fitted rows then take their heights, and the plot areas of the weighted rows share the rest, so plot areas of equal weight stay of equal size above or beside a fitted row. `fit_row` fits one row and keeps the sizes of the others, and `fit_col` does the same for a column:

```rust
// Three plots sharing the time axis, and a key beneath them as tall as its entries.
let mut fig = Figure::new().tiles(4, 1).fit_row(3);
let key = fig.panel(3, 0).id();
for row in 0..3 {
    let mut ax = fig.axes(row, 0);
    ax.plot(&t, &a).label("Inlet");
    ax.plot(&t, &b).label("Outlet");
    ax.legend_in(key);
}
fig.link_all_x()?;
```

`row_weights` and `col_weights` also accept a list of `TrackSize` values, in which `TrackSize::Weight(w)` is a weight and `TrackSize::Fit` a fitted size, such as `row_weights([TrackSize::Weight(1.0), TrackSize::Weight(1.0), TrackSize::Fit])`.

A fitted row is as tall as the tallest area that a panel lying in that row alone needs for its entries, together with the margins that align the panel with the plot areas beside it, or the page padding on a side whose row holds no axes. The area includes the inset of the panel's border, if it has one, so that the panel neither clips its entries nor leaves space around them. In a fitted row the width of a panel is fixed and its height follows its entries, so automatic columns are chosen the other way round: the entries take the fewest rows that the width allows, in the fewest columns that give that many rows. A fitted column is as wide as the widest area that a panel lying in that column alone needs, with automatic columns chosen to fit the height as usual. A panel that spans a fitted row and another row does not size the fitted row, and a fitted row or column that holds no panel of its own has no size.

An axes has no content size, so an axes that lies in fitted rows alone, or in fitted columns alone, is reported by [`validate`](getting-started.md#validating-a-figure) as the error `AxesInFittedTrack`. An axes may span a fitted row together with a weighted one, as a tall plot beside a key does.

## Many entries

A panel arranges its entries in as few columns as fit its height, so a panel across the width of a figure with a short row spreads a long list into several columns without being told how many. Automatic columns are chosen as follows: the entries are first placed in one column; while that block is taller than the area available, one more column is tried; and the search stops at the first count that fits the height or when another column would no longer fit the width. A fixed count, set with `columns(LegendEntryColumns::Count(n))`, is used as given, except that there are never more columns than entries. Rows are as tall as their tallest entry and columns as wide as their widest, so that entries line up both across and down.

```rust
let mut fig = Figure::new().tiles(2, 2).row_weights([3.0, 1.0]);
let key = fig.panel_span(1, 0, 1, 2).id();
for col in 0..2 {
    let mut ax = fig.axes(0, col);
    for k in 1..=12 {
        let y: Vec<f64> = t.iter().map(|t| (t * k as f64 / 4.0).sin()).collect();
        ax.plot(&t, &y).label(format!("Mode {k}"));
    }
    ax.legend_in(key);
}
```

When the entries do not fit the area available even in as many columns as fit its width, the legend overflows. The block of entries is then drawn from the top-left corner of the area and clipped to it, the entries that do not lie wholly inside the area are left out, and the scene compiler reports a `LegendOverflow` warning. The remedy is to make the panel larger, with more tiles, a larger weight or a larger figure, or to use fewer entries. The same rules apply to a legend inside an axes, whose available area is the plot area less a small inset.

A key in a [fitted row](#fitting-rows-and-columns-to-their-legends) does not overflow its height unless the fitted rows together need more than the height of the figure, in which case they share it in proportion to their sizes and their legends report `LegendOverflow`. When a key must share the page by weight, a program can learn whether it fits from `layout_warnings`, which lays the figure out without exporting it and returns the warnings of the scene compiler, the same warnings that the `scene` list of an export report holds. It needs no graphics adapter, so it can be called repeatedly, for example to choose the number of columns of a key:

```rust
use ironlab::SceneWarningKind;

let fits = |fig: &Figure| {
    !fig.layout_warnings()
        .iter()
        .any(|warning| matches!(warning.kind, SceneWarningKind::LegendOverflow { .. }))
};
for columns in [1, 2, 3, 4] {
    fig.panel(1, 0).legend().columns(LegendEntryColumns::Count(columns));
    if fits(&fig) {
        break;
    }
}
```

The report of an export holds the same warnings, and a program can check for both legend warnings there:

```rust
let report = fig.export_pdf("modes.pdf")?;
for warning in &report.scene {
    if matches!(
        warning.kind,
        SceneWarningKind::LegendOverflow { .. } | SceneWarningKind::LegendNameClash { .. }
    ) {
        eprintln!("{}", warning.message);
    }
}
```

## Legends in the viewer

In the [viewer](viewer.md#legend), clicking a legend entry hides or shows its plot. Clicking a merged entry acts on every plot the entry stands for, so a series shared by several subplots is hidden in all of them at once, and the first of those plots is selected. The click hides all of the plots unless every one of them is already hidden, in which case it shows them all. A merged entry is greyed only when all of its plots are hidden, and one click is one step of the undo history. An item of an explicit list that names a plot behaves in the same way, and acts on every plot it stands for; an item that stands for no plot is not a toggle, and clicking it does nothing.

The property editor lists the panels of a figure after the axes, in a group named "Panels", each named by its content and the tiles it occupies, as "Legend panel (row 2, col 0)" or "Legend panel (rows 0–1, col 2)". The alignment of a panel, the border, columns and order of its legend, the location, box, columns and order of a legend inside an axes, and the sizes of the rows and columns of the tile layout can be changed there; the sizes are typed as a list of weights in which the word `fit` stands for a fitted size. The tiles a panel occupies, the kind of content it holds, the panel that an axes sends its entries to and the explicit list of entries of a legend cannot, because they are part of the arrangement and the description of the figure, which are built in code. A legend with an explicit list is drawn and clicked in the viewer exactly as in an export.

## Legends in a figure file

In the JSON encoding, the `legend` of an axes is either `{"type": "inside", "value": {...}}`, holding the location, box and entry layout, or `{"type": "panel", "value": 7}`, naming the identifier of a panel. The panels are listed in the `panels` property of the figure, each holding its legend in its `content`, and the sizes of the rows and columns are the `row_weights` and `col_weights` of its `layout`. A shared panel beneath two plots, as in the [many entries](#many-entries) example, is stored as follows, with the rest of each axes left out:

```json
{
  "layout": {
    "rows": 2,
    "cols": 2,
    "row_weights": [{"type": "weight", "value": 3.0}, {"type": "weight", "value": 1.0}]
  },
  "axes": [
    {"id": 2, "cell": {"row": 0, "col": 0, "row_span": 1, "col_span": 1}, "legend": {"type": "panel", "value": 1}},
    {"id": 15, "cell": {"row": 0, "col": 1, "row_span": 1, "col_span": 1}, "legend": {"type": "panel", "value": 1}}
  ],
  "panels": [
    {
      "id": 1,
      "cell": {"row": 1, "col": 0, "row_span": 1, "col_span": 2},
      "align": "center",
      "content": {
        "type": "legend",
        "value": {"boxed": true, "layout": {"columns": {"type": "auto"}, "order": "column_major"}}
      }
    }
  ]
}
```

The entries of a legend are omitted while they are automatic. An explicit list is stored in the `entries` of the legend; a reference on a row of its own, followed by a plot and an item that stands for no plot, is stored as follows:

```json
"entries": {
  "type": "explicit",
  "value": [
    {"content": {"type": "artist", "value": 12}, "full_row": true},
    {"content": {"type": "artist", "value": 9}, "full_row": false},
    {
      "content": {
        "type": "custom",
        "value": {
          "label": {"content": "Upper surface", "interpreter": "latex"},
          "sample": {"type": "swatch", "color": "#d98c00"}
        }
      },
      "full_row": false
    }
  ]
}
```

Every property is described in the [figure schema reference](../reference/figure-schema.md#legend), and the reasons for the design are recorded in [ADR 0018](../adrs/0018-legend-panels-with-entries-merged-by-name-and-look.md), [ADR 0020](../adrs/0020-explicit-legend-entries-and-fitted-tracks.md) and [ADR 0022](../adrs/0022-figure-nodes-in-one-identifier-space-and-panels.md).

## Examples in the gallery

- [Legend toggling](../gallery/legend_toggle.md) shows a legend inside an axes, with entries typeset in LaTeX, whose series can be hidden from the legend.
- [Shared legend panel](../gallery/panel_legend_shared.md) shows temperatures at five weather stations in a grid of five axes that draw the same series, with one panel in the sixth tile listing each series once.
- [Wide legend panel](../gallery/panel_legend_wide.md) shows the step and magnitude responses of a second-order system at twelve damping ratios, with a panel across the full width beneath the two plots, in a row made shorter by row weights, whose entries are arranged in automatic columns. Its panel is drawn without a border, for comparison with the bordered panel of the previous figure.
- [Reference and variants](../gallery/reference_and_variants.md) shows a stack of three plots that share the angle of attack, with a key in a fitted row beneath them that lists the base case first, on a row of its own, although it is drawn last, followed by six perturbation levels in columns.
- [Two keys](../gallery/two_keys.md) shows a curve key beside a marker key whose items stand for no plot: a grey shape for each kind of point, and a colour swatch for each surface, both in one fitted row.
