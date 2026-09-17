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

Every gesture is one step of the history: a drag from press to release, one notch of the wheel, one click on a legend entry, a double-click and Reset view each count as one. A change made in the [property editor](#the-property-editor) is a step in the same way: a drag of a numeric field, a visit to a text field, a choice from a combo box and a revert each count as one.

- **Undo**, in the toolbar or **⌘Z** (**Ctrl+Z** away from macOS), restores the figure to what it was before the most recent gesture.
- **Redo**, in the toolbar or **⌘⇧Z** (**Ctrl+Shift+Z**), applies the most recently undone gesture again.

Each button is disabled when there is nothing to undo or to redo, and both shortcuts are ignored while a text field has keyboard focus. Making a new gesture after undoing one discards what could have been redone, so the history never branches. Undo and redo act only on the changes made in the viewer: the figure as opened is the furthest back they go, and saving the figure makes the changes part of it, after which they can no longer be undone.

## Legend

Clicking an entry of a legend hides its plot, and clicking the entry again shows the plot. Either way the plot is selected, so the property editor shows it. The entry of a hidden plot stays in the legend and is drawn greyed out. Double-clicking an entry counts as two clicks, so it leaves the plot as it was, and it does not restore the limits of the axes as double-clicking elsewhere in the axes does.

A hidden plot still takes part in the layout of its axes: its data still contributes to automatic axis limits and colour limits, and it keeps its colour. Hiding or showing a plot therefore never moves, rescales or recolours anything else.

## The property editor

**Properties**, in the toolbar, opens and closes a panel on the right of the tab. It is closed when a figure is opened, so the canvas has the whole tab until the panel is asked for. It has two parts: the objects of the figure at the top and the properties of the selected object below.

### The objects of a figure

The upper part lists the figure, its axes in drawing order, and the plots of each axes in drawing order, as a tree that can be collapsed. Each row is named as the figure names it:

- An axes is named by its title, or, when it has none, by the cell it occupies, such as "Axes (row 0, column 1)".
- A plot is named by its display name — the name its legend entry carries — or, when it has none, by its kind: "Line", "Scatter", "Contour", "Quiver" or "Surface".
- A hidden plot is greyed, so that a plot missing from the canvas can be found and shown again.

Clicking a row selects that object, and the lower part then shows its properties. Clicking inside an axes on the canvas selects that axes, and clicking a legend entry selects its plot as well as hiding or showing it. The canvas cannot yet pick an individual plot ([issue #1](https://github.com/thclark/ironlab/issues/1)), so the tree is the way to reach one.

The selection is never changed by a gesture: panning, zooming and rotating leave the property editor showing what it was showing.

### The properties of an object

The lower part lists the properties of the selected object, gathered under the value they belong to: the scale, limits and grid lines of the x axis appear together under "x", and each row is named by what distinguishes it within that value, such as "limits" and "limits.min". Hovering over the name of a property shows what it means, in the same words as the Rust API documentation.

The control offered depends on what the property holds.

| The property holds | The control |
| --- | --- |
| A true or false value | A checkbox |
| A number | A field that is dragged, or clicked and typed into |
| A text | A field, with a combo box choosing whether it is read as LaTeX or literally |
| One of a fixed set of values, such as a scale or a colormap | A combo box |
| A choice between kinds of value, such as automatic or manual limits | A combo box, with the values of the kind chosen listed below it |
| A colour | A colour picker |
| A reference to a data array | The array and its shape, read-only |

A property that is absent — an axes with no title, a plot with no display name — is shown as "unset", with a control that gives it a value. A property that belongs to a kind of value that is not in use is not shown at all: the bounds of automatic limits appear only once the limits are manual, and the camera of an axes only when the axes is three-dimensional. Neither is an error.

**Data is shown but not edited here.** A plot refers to its data by identifier, and a plot that refers to an array of the wrong shape, or to no array, cannot be drawn; the viewer has no way to offer the arrays that would suit. Data is therefore changed through the program that owns the figure. This is a deliberate limit of the editor as it stands.

### Changing, marking and reverting

A change made in the editor is recorded exactly as a gesture is: it is added to the overlay, the canvas redraws, and the figure the viewer was given is untouched until it is saved. A whole drag of a numeric field, and a whole visit to a text field, is one step of the history, so **Undo** takes back the change rather than the last pixel of it.

A property that the user has changed is shown in bold and carries a **↺** control, which takes back that one change and shows the figure's own value again. Reverting one property never disturbs another, and is itself a step of the history.

A change that the figure cannot accept — limits that are not increasing, or a projection that a plot cannot be drawn in — is refused. The figure is left exactly as it was, nothing is added to the history, and the reason appears in the problems indicator described below.

### Parameters

The properties of the figure include its **parameters**: the named values that describe it, which are what make a collection of figures sortable and searchable (see [parameters](getting-started.md#parameters)). They are edited as a small table, in which an entry can be added, renamed, given another kind (yes or no, whole number, number or text), changed and removed. The whole table is committed together, so a change to it is one step of the history.

While the table cannot be committed — an entry has no name, two entries share a name, or a number has not been typed in full — the reason is shown below it and the figure keeps the parameters it had. Typing is never interrupted; the change reaches the figure as soon as the table makes sense again.

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

A change made in the viewer that the figure cannot accept, such as limits that are not increasing, is dropped rather than applied, and the same indicator gives the property it concerned and the reason. A change made in the property editor is refused before it is applied, so the figure keeps the value it had and the history keeps no empty step. Such a change is reported until the view is reset or the figure is saved.
