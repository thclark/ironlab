# Using the viewer

The IronLAB viewer is a desktop window that shows figures as tabs and lets them be explored with the mouse or trackpad. How to open it is described in [getting started](getting-started.md#opening-the-viewer).

Each tab has a toolbar above a canvas. The canvas shows the figure as a page preview: the whole figure at its physical aspect ratio, scaled to fit the tab. What the canvas shows is drawn from the same geometry as an exported PDF, so the preview and the export agree.

Every interaction described on this page changes the figure by setting a property of it — an axis limit, a three-dimensional view or the visibility of a plot — exactly as setting that property from the Rust API would. The viewer keeps the figure as it was opened, called the source, and the changes made to it, called the overlay, apart: what the canvas draws, what **Export PDF…** writes and what **Save figure…** writes is the source with the overlay applied. Because each change is a value of its own, any of them can be undone, and a view is restored by discarding the changes made to it rather than by copying an earlier figure back. The reasons for this design are recorded in [ADR 0008](../adrs/0008-typed-edits-and-a-view-overlay.md).

## Tools

The toolbar has three tool buttons, which decide what dragging with the primary mouse button does. The active tool is highlighted, and Pan is active when a figure is opened.

| Tool | Dragging on a two-dimensional axes | Dragging on a three-dimensional axes |
| --- | --- | --- |
| **Pan** | Moves the limits so that the point grabbed stays under the pointer. | Moves the projected box so that it follows the pointer. |
| **Zoom** | Draws a rubber band and zooms to it on release. | Does nothing. |
| **Rotate** | Does nothing. | Rotates the view. |

The Rotate button is disabled when the figure has no three-dimensional axes. A drag acts on the axes under the pointer when the drag starts, and a drag that starts outside every axes does nothing.

## Zooming

Scrolling with the mouse wheel, or pinching on a trackpad, zooms the axes under the pointer, whichever tool is active. Scrolling up zooms in and scrolling down zooms out; one notch of a typical mouse wheel changes the scale by about 20 %.

- On a two-dimensional axes, the x and y limits are rescaled about the data point under the pointer, so that this point stays where it is. On a logarithmic axis the limits are rescaled in logarithmic space, so zooming behaves identically on every decade.
- On a three-dimensional axes, the magnification of the view changes; the limits of its axes do not.

With the Zoom tool, dragging on a two-dimensional axes draws a rubber band, which is confined to the plot area. On release, the x and y limits are set to the range of data that the band covers. A band less than 3 pt wide or high, measured on the figure page, is ignored, so that a click does not zoom to a sliver.

## Panning

With the Pan tool, dragging on a two-dimensional axes shifts its x and y limits so that the data point grabbed at the start of the drag stays under the pointer. Dragging on a three-dimensional axes moves the projected box with the pointer; the limits of its axes do not change.

## Rotating

With the Rotate tool, dragging on a three-dimensional axes changes its azimuth and elevation, and the axes box follows the pointer as in MATLAB's `rotate3d`.

- Dragging to the right decreases the azimuth, and dragging to the left increases it.
- Dragging down increases the elevation, and dragging up decreases it.

The rate is half a degree for each point of pointer travel measured on the figure page, so the rate on screen depends on how large the figure is drawn. The elevation stops at −90° (looking up from below) and 90° (looking down from above). The azimuth is not limited.

## Restoring views

- **Double-clicking** an axes discards the changes made to the x, y and z limits and to the three-dimensional view of that axes, so that it shows the view the figure was opened with. Axes linked with it follow the restored limits.
- **Reset view**, in the toolbar, discards the limit and three-dimensional view changes of every axes of the figure. Pressing **R** does the same for the figure in the visible tab, unless a text field has keyboard focus or a modifier key is held.

Neither action changes the visibility of plots, so plots hidden from the legend stay hidden.

A two-dimensional axes whose limits were automatic receives fixed limits as soon as it is panned or zoomed, computed from the limits it was showing. The figure itself keeps its automatic limits, so restoring the view makes the limits automatic again.

## Undo and redo

Every gesture is one step of the history: a drag from press to release, one notch of the wheel, one click on a legend entry, a double-click and Reset view each count as one.

- **Undo**, in the toolbar or **⌘Z** (**Ctrl+Z** away from macOS), restores the figure to what it was before the most recent gesture.
- **Redo**, in the toolbar or **⌘⇧Z** (**Ctrl+Shift+Z**), applies the most recently undone gesture again.

Each button is disabled when there is nothing to undo or to redo, and both shortcuts are ignored while a text field has keyboard focus. Making a new gesture after undoing one discards what could have been redone, so the history never branches. Undo and redo act only on the changes made in the viewer: the figure as opened is the furthest back they go, and saving the figure makes the changes part of it, after which they can no longer be undone.

## Legend

Clicking an entry of a legend hides its plot, and clicking the entry again shows the plot. The entry of a hidden plot stays in the legend and is drawn greyed out. Double-clicking an entry counts as two clicks, so it leaves the plot as it was, and it does not restore the limits of the axes as double-clicking elsewhere in the axes does.

A hidden plot still takes part in the layout of its axes: its data still contributes to automatic axis limits and colour limits, and it keeps its colour. Hiding or showing a plot therefore never moves, rescales or recolours anything else.

## Linked axes

When axes are linked along a dimension (see [linking axes](getting-started.md#linking-axes)), panning, zooming, box zooming or double-clicking one of them changes the limits of every axes in its group along that dimension. Axes that are not linked along that dimension are unaffected. For example, when the x limits of two axes are linked and their y limits are not, panning one of them moves both horizontally but only itself vertically.

Links apply to limits only. The azimuth, elevation, magnification and position of a three-dimensional view belong to its own axes, and rotating or zooming one three-dimensional axes does not change another.

## Saving the figure

**Save figure…**, in the toolbar, opens a save dialog and writes the figure as it is currently shown, with its current limits, three-dimensional views and plot visibility. The format follows the extension of the name given: `.fig` writes the default Protocol Buffers format and `.json` (including `.fig.json`) writes JSON, as described in [saving and loading](getting-started.md#saving-and-loading). A name with any other extension is refused and no file is written.

The figure written is the figure the viewer now holds: the changes saved become part of it, the undo history is emptied, and Reset view restores the view as saved rather than the view the file was opened with. A notification in the bottom-right corner of the window reports whether the save succeeded.

## Exporting to PDF

**Export PDF…**, in the toolbar, opens a save dialog and writes the figure as it is currently shown: with its current limits, current three-dimensional views, and without the plots hidden from the legend. The exported page has the same properties as one written by the API, which are described in [exporting PDF](getting-started.md#exporting-pdf). A notification in the bottom-right corner of the window reports whether the export succeeded.

## Problems

When drawing a figure reveals problems that do not prevent it from being drawn, such as a LaTeX expression that the typesetter does not support or data that cannot be placed on a logarithmic axis, the toolbar shows a problems indicator: a warning sign followed by the number of problems, for example "2 problems". Hovering over the indicator lists each problem, with the identifier of the node (the figure, an axes or a plot) that it concerns. The figure is still drawn: unsupported mathematics is shown as its raw source, and data that cannot be placed is left out.

A change made in the viewer that the figure cannot accept, such as limits that are not increasing, is dropped rather than applied, and the same indicator gives the property it concerned and the reason. Such a change is reported until the view is reset or the figure is saved.
