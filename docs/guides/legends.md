# Legends

A legend lists the plots of a figure that have a display name, each beside a sample of how the plot is drawn. IronLAB offers two kinds of legend. A legend **inside an axes** is drawn in a corner or at an edge of that axes' plot area and lists only the plots of that axes. A **legend panel** occupies tiles of the figure's layout, as an axes does, and lists the plots of every axes that sends its entries to it, so that a grid of subplots showing the same series can share one legend instead of repeating it in every plot.

This guide assumes the figure, tile and axes concepts of [getting started](getting-started.md#figures-tiles-and-axes). The examples use the prelude and the following data:

```rust
use ironlab::prelude::*;

let t = linspace(0.0, 10.0, 201);
let a: Vec<f64> = t.iter().map(|t| t.sin()).collect();
let b: Vec<f64> = t.iter().map(|t| t.cos()).collect();
```

## Legends inside an axes

A plot has a legend entry only when it has a display name. An axes has no legend until one is asked for: `legend` shows a boxed legend of every named plot of the axes, in the order the plots were added, at the given location, and `legend_off` hides it again.

```rust
let mut fig = Figure::new();
let mut ax = fig.axes(0, 0);
ax.plot(&t, &a).display_name("$\\sin t$");
ax.plot(&t, &b).display_name("$\\cos t$");
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

`fig.panel_legend(row, col)` returns the legend panel whose top-left tile is `(row, col)`, creating a panel in that single tile if there is none. `fig.panel_legend_span(row, col, row_span, col_span)` does the same and makes the panel cover the given numbers of rows and columns. Both return a `PanelLegendMut` handle, whose setters can be chained:

| Method | Effect |
| --- | --- |
| `align(location)` | Places the block of entries within the panel, using the locations listed [above](#locations). The default is `Center`, and `Best` also means the centre, because a panel holds no data for its entries to avoid. |
| `boxed(on)` | Outlines the panel with a border drawn as the box of an axes is drawn, which is the default. The area of a panel is aligned with the plot areas around it, so the border continues the edges of their boxes. `boxed(false)` removes the border. |
| `columns(columns)` | Sets the number of columns of entries. A panel uses `LegendEntryColumns::Auto` by default. |
| `order(order)` | Sets the order in which entries fill the columns, `LegendEntryOrder::ColumnMajor` (the default) or `LegendEntryOrder::RowMajor`. |
| `collect_all()` | Sends the entries of every axes that the figure has at that moment to the panel. |
| `id()` | Returns the identifier of the panel, which axes use to send their entries to it. |

Like an axes handle, the panel handle borrows the figure mutably, so it is held only while the panel is being set up. The identifier returned by `id()` is kept instead, to refer to the panel later.

A panel is placed in its tiles whether or not an axes already occupies them. A panel whose tiles include a tile of an axes is drawn over the plot, and [`validate`](getting-started.md#validating-a-figure) reports the pair with the warning `OverlappingCells`, as it reports two axes that share a tile. The warning names both nodes, and it is removed by giving the panel tiles of its own, for example by adding a row or column to the layout.

### Sending entries to a panel

An axes sends its entries to a panel with `legend_in`, giving the identifier of the panel. The axes then draws no legend inside its plot area: an axes lists its entries in exactly one place, either inside its own plot area or in one panel.

```rust
let mut fig = Figure::new().tiles(1, 3);
let key = fig.panel_legend(0, 2).id();
for col in 0..2 {
    let mut ax = fig.axes(0, col);
    ax.plot(&t, &a).display_name("Inlet");
    ax.plot(&t, &b).display_name("Outlet");
    ax.legend_in(key);
}
```

When every axes of the figure should share one panel, `collect_all` sends the entries of all of them at once. It affects only the axes that exist when it is called, so it is called after the axes have been created:

```rust
let mut fig = Figure::new().tiles(1, 3);
for col in 0..2 {
    fig.axes(0, col).plot(&t, &a).display_name("Inlet");
}
fig.panel_legend(0, 2).collect_all();
```

An identifier that names no panel of the figure is reported as the error `DanglingPanelLegend` by [`validate`](getting-started.md#validating-a-figure). A panel that lists nothing, because no axes sends entries to it or none of those axes has a named plot, is valid and draws nothing, and validation reports it with the warning `NothingToDraw`, as it does an artist with no data.

### How a panel lines up with the plots

The area of a panel is its block of tiles less the margins of the rows and columns it occupies. Those margins are the space that the axes in the same rows and columns reserve for their tick labels, axis labels and titles. A panel beneath two plots therefore spans exactly the width of their plot areas, and a panel beside a column of plots spans exactly their height. On a side whose row or column holds no axes, such as the top and bottom of a row that holds only a panel, the panel is inset by the page padding instead. A panel reserves no margin of its own, so adding one never moves a plot area. The block of entries is then placed within the panel's area by its alignment.

By default a panel outlines its area with a border drawn exactly as the box of an axes is drawn, so the edges of the border continue the edges of the boxes of the plots beside or above it, and the panel reads as one more tile of the grid. The entries are kept inside the border by the same inset that a legend inside a plot area keeps from the box of its axes, at every alignment. A panel with no entries still draws its border. `boxed(false)` removes the border, and the block of entries then sits flush with the edges it is aligned to.

## Sharing and merging entries

A panel gathers the entries of every axes that sends entries to it, and merges entries that a reader could not tell apart. Two entries are merged into one when both of the following hold:

- their display names are the same text, interpreted the same way; and
- their samples draw the same picture: the same kind of sample, colour, line width, dash pattern, marker shape, marker size and marker edge.

A merged entry is drawn from the first plot it stands for, and it stands for every plot it merges. Automatic colours are assigned from the colour order separately in each axes, so the first plot of every axes takes the first colour, the second plot the second colour, and so on. A series added in the same position in every axes, as in the examples above, therefore merges into a single entry without any colour being set.

Two plots with the same display name but a different look are both listed, because merging them would leave one of the series without an entry that matches what is drawn. The two entries cannot be told apart by name, so the scene compiler reports one `LegendNameClash` warning for the name. The warning is shown in the viewer's [problems list](viewer.md#problems), and it is returned in the `scene` warnings of the report that `export_pdf` and `export_png` return. Giving the series different names, or the same look, removes the warning.

```rust
let mut fig = Figure::new().tiles(1, 3);
let key = fig.panel_legend(0, 2).id();
let mut left = fig.axes(0, 0);
left.plot(&t, &a).display_name("Model").color(Color::rgb(0.0, 0.0, 0.0));
left.legend_in(key);
let mut right = fig.axes(0, 1);
right.plot(&t, &b).display_name("Model").color(Color::rgb(0.8, 0.1, 0.1));
right.legend_in(key);
// The panel lists "Model" twice, in black and in red, and the export warns of the clash.
```

The entries of a panel are listed in the reading order of the page. The axes are taken by the top-left tile of their cells, row by row from the top and, within a row, column by column from the left, and the plots of each axes follow in the order they were added. Two axes whose cells start at the same tile are taken in the order they were created. The order in which the code creates the axes therefore does not matter: a figure that builds its right-hand plot first still lists the entries of the left-hand plot first. A merged entry takes the position at which its first plot appears in this order.

A legend inside an axes lists every named plot of that axes and merges nothing.

## Several panels

A figure may have any number of panels, and each axes chooses which of them receives its entries. A common arrangement is one panel beneath each column of a grid, when each column shows a different set of series:

```rust
let mut fig = Figure::new().tiles(3, 2).row_weights([2.0, 2.0, 1.0]);
for col in 0..2 {
    let key = fig.panel_legend(2, col).id();
    for row in 0..2 {
        let mut ax = fig.axes(row, col);
        ax.plot(&t, &a).display_name(format!("Case {}", 2 * col + 1));
        ax.plot(&t, &b).display_name(format!("Case {}", 2 * col + 2));
        ax.legend_in(key);
    }
}
```

Each panel is aligned with the plot areas of its own column, and the two axes of a column share the entries of their panel.

## Sizing rows and columns with weights

The tiles of a figure are of equal size by default, which suits a grid of plots but makes a panel as large as a plot. Weights give rows and columns different sizes. `row_weights` sets the relative heights of the rows, from the top, and `col_weights` the relative widths of the columns, from the left. Each row shares the height of the figure below its title in proportion to its weight, and each column shares the width in the same way.

```rust
// The top row of plots is three times the height of the row that holds the panel.
let fig = Figure::new().tiles(2, 2).row_weights([3.0, 1.0]);
```

A row or column without a weight, because fewer weights than rows or columns are given, has a weight of 1, so `col_weights([2.0])` in a layout of three columns makes the first column twice as wide as each of the others. A weight must be a finite positive number, and there must be no more weights than rows or columns; [`validate`](getting-started.md#validating-a-figure) reports any other list as the error `InvalidLayoutWeights`.

An axes or panel that spans several rows or columns takes the sum of their weights. This makes it possible to combine a wide plot with a narrow panel in one row. In the following figure, the axes spans the first two columns and takes four fifths of the width, and the panel in the third column takes the remaining fifth:

```rust
let mut fig = Figure::new().tiles(1, 3).col_weights([2.0, 2.0, 1.0]);
let key = fig.panel_legend(0, 2).align(LegendLocation::West).id();
let mut ax = fig.axes_span(0, 0, 1, 2);
ax.plot(&t, &a).display_name("Inlet");
ax.plot(&t, &b).display_name("Outlet");
ax.legend_in(key);
```

Weights apply to every axes and panel of the figure, not only to legends, and the same layout can make one plot of a grid larger than the others.

## Many entries

A panel arranges its entries in as few columns as fit its height, so a panel across the width of a figure with a short row spreads a long list into several columns without being told how many. Automatic columns are chosen as follows: the entries are first placed in one column; while that block is taller than the area available, one more column is tried; and the search stops at the first count that fits the height or when another column would no longer fit the width. A fixed count, set with `columns(LegendEntryColumns::Count(n))`, is used as given, except that there are never more columns than entries. Rows are as tall as their tallest entry and columns as wide as their widest, so that entries line up both across and down.

```rust
let mut fig = Figure::new().tiles(2, 2).row_weights([3.0, 1.0]);
let key = fig.panel_legend_span(1, 0, 1, 2).id();
for col in 0..2 {
    let mut ax = fig.axes(0, col);
    for k in 1..=12 {
        let y: Vec<f64> = t.iter().map(|t| (t * k as f64 / 4.0).sin()).collect();
        ax.plot(&t, &y).display_name(format!("Mode {k}"));
    }
    ax.legend_in(key);
}
```

When the entries do not fit the area available even in as many columns as fit its width, the legend overflows. The block of entries is then drawn from the top-left corner of the area and clipped to it, the entries that do not lie wholly inside the area are left out, and the scene compiler reports a `LegendOverflow` warning. The remedy is to make the panel larger, with more tiles, a larger weight or a larger figure, or to use fewer entries. The same rules apply to a legend inside an axes, whose available area is the plot area less a small inset.

A program can check for both legend warnings in the report of an export:

```rust
use ironlab::SceneWarningKind;

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

In the [viewer](viewer.md#legend), clicking a legend entry hides or shows its plot. Clicking a merged entry acts on every plot the entry stands for, so a series shared by several subplots is hidden in all of them at once, and the first of those plots is selected. The click hides all of the plots unless every one of them is already hidden, in which case it shows them all. A merged entry is greyed only when all of its plots are hidden, and one click is one step of the undo history.

The property editor lists each legend panel after the axes, named by the tile it occupies, as "Legend panel (row 2, col 0)". The alignment, border, columns and order of a panel, the location, box, columns and order of a legend inside an axes, and the weights of the tile layout can be changed there. The tiles a panel occupies and the panel that an axes sends its entries to cannot, because they are part of the arrangement of the figure, which is built in code.

## Legends in a figure file

In the JSON encoding, the `legend` of an axes is either `{"type": "inside", "value": {...}}`, holding the location, box and entry layout, or `{"type": "panel", "value": 7}`, naming the identifier of a panel. The panels are listed in the `panel_legends` property of the figure, and the weights are the `row_weights` and `col_weights` of its `layout`. A shared panel beneath two plots, as in the [many entries](#many-entries) example, is stored as follows, with the rest of each axes left out:

```json
{
  "layout": {"rows": 2, "cols": 2, "row_weights": [3.0, 1.0]},
  "axes": [
    {"id": 2, "cell": {"row": 0, "col": 0, "row_span": 1, "col_span": 1}, "legend": {"type": "panel", "value": 1}},
    {"id": 15, "cell": {"row": 0, "col": 1, "row_span": 1, "col_span": 1}, "legend": {"type": "panel", "value": 1}}
  ],
  "panel_legends": [
    {
      "id": 1,
      "cell": {"row": 1, "col": 0, "row_span": 1, "col_span": 2},
      "align": "center",
      "boxed": true,
      "layout": {"columns": {"type": "auto"}, "order": "column_major"}
    }
  ]
}
```

Every property is described in the [figure schema reference](../reference/figure-schema.md#legend), and the reasons for the design are recorded in [ADR 0018](../adrs/0018-legend-panels-with-entries-merged-by-name-and-look.md).

## Examples in the gallery

- [Legend toggling](../gallery/legend_toggle.md) shows a legend inside an axes, with entries typeset in LaTeX, whose series can be hidden from the legend.
- [Shared legend panel](../gallery/panel_legend_shared.md) shows temperatures at five weather stations in a grid of five axes that draw the same series, with one panel in the sixth tile listing each series once.
- [Wide legend panel](../gallery/panel_legend_wide.md) shows the step and magnitude responses of a second-order system at twelve damping ratios, with a panel across the full width beneath the two plots, in a row made shorter by row weights, whose entries are arranged in automatic columns. Its panel is drawn without a border, for comparison with the bordered panel of the previous figure.
