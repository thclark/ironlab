# Using the viewer

The IronLAB viewer is a desktop window that shows one figure at a time and lets it be explored with the mouse or trackpad. How to open it is described in [getting started](getting-started.md#opening-the-viewer). When more than one figure is open, the [figure browser](#the-figure-browser) down the left-hand side is how the others are reached: it narrows them down to the one to look at.

The figure has a toolbar above a canvas, and beneath the canvas a strip of [details](#details). The canvas shows the figure as a page preview: the whole figure at its physical aspect ratio, scaled to fit the window. What the canvas shows is drawn from the same geometry as an exported PDF, so the preview and the export agree.

Every interaction described on this page changes the figure by setting a property of it — an axis limit, a three-dimensional view or the visibility of a plot — exactly as setting that property from the Rust API would. The viewer keeps the figure as it was opened, called the source, and the changes made to it, called the overlay, apart: what the canvas draws, what **Export PDF…** writes and what **Save figure…** writes is the source with the overlay applied. Because each change is a value of its own, any of them can be undone, and a view is restored by discarding the changes made to it rather than by copying an earlier figure back. The reasons for this design are recorded in [ADR 0008](../adrs/0008-typed-edits-and-a-view-overlay.md).

## The figure browser

When the viewer opens more than one figure, a browser appears down the left-hand side. It narrows the open figures
down to the one to look at, and clicking a figure in its list shows that figure; the row of the figure shown is
marked in the selection colour, and every row shows the figure's labels beneath its title, or the value the list is
ordered by. Dragging the browser's edge past its narrowest width shuts it, leaving a handle to drag it open again. A
viewer holding a single figure has nothing to browse, and opens without it.

The browser is built around the [labels and parameters](describing-figures.md) of a figure, which exist so that a collection of figures can be sorted,
filtered and searched. It offers exactly what the figures carry and nothing else: the viewer works out no
properties of its own, because the program that built the figures is the only thing that knows which of their
properties matter. A collection whose figures carry no labels and no parameters can still be searched by title and
listed, but there is nothing to filter it by until its author says what there should be.

When no figure of a collection carries a label or a parameter, there is nothing to narrow it by, and the browser
says so rather than offering an empty menu: it explains that descriptions are added where the figures are built,
and links to [describing figures](describing-figures.md). The figures are still listed and can still be searched by
title.

An empty label, or one a figure carries twice, is shown once and reported in the problems list: neither is possible
in a figure IronLAB wrote, but a file written by another program can carry either.

### Searching

The field at the top of the browser searches the titles, the labels and the parameters. Typing `surface` leaves the
figures that mention it anywhere; the list narrows as each character is typed.

A word can also ask about one parameter, which is how a search becomes precise:

| Typed | Meaning |
| --- | --- |
| `surface` | the figure mentions "surface" anywhere |
| `-stalled` | it does not mention "stalled" |
| `rig:CFD` | its `rig` parameter contains "CFD", ignoring case |
| `rig=CFD` | its `rig` parameter is exactly "CFD", ignoring case |
| `angle>=8` | its `angle` parameter is 8 or more, and `>`, `<` and `<=` compare in the same way |
| `label:piv` | one of its labels contains "piv" |

Words are cumulative: every one of them must hold. A word only asks about a parameter when the collection has one
of that name, so a colon in a title, a file path or a web address searches rather than asking about a parameter
that does not exist. A figure that does not carry the parameter a word asks about is left out, so `solver:LES`
finds the computed runs and leaves the measured ones out, which is what asking about a solver means. Underscores
inside a typed number are separators, so `mesh_cells>=2_000_000` reads as it was written in the code that set it.

### Filtering

**Edit**, at the right of the **Filters** caption, opens a menu within the panel beneath the caption, so that what
is beneath it moves down and the menu stays open until it is shut. Its first page lists the parameters worth
filtering on, the most useful first, each with how many values it takes and how much of the collection carries it;
typing in the field above the list narrows it. Choosing a parameter turns the page to its values, each with the
number of figures choosing it would leave. **Back to all parameters** turns back, **Done** shuts the menu, and
clicking **Edit** again shuts it too. A parameter that takes the same value on every figure divides nothing and is
not offered.

Which parameters are worth offering is decided from the collection rather than declared in advance, because the
parameters are the user's own. A parameter is offered in proportion to how much of the collection carries it and how
evenly it divides what it covers, and a parameter with a different value on nearly every figure — a run number, a
note — is treated as something to search for rather than something to filter by.

Values within one parameter are alternatives and parameters are cumulative: choosing two rigs widens the result to
the figures from either, while choosing a rig and a solver narrows it to the figures with both. The number beside a
value is worked out as though that parameter were not filtered at all, so it always says what choosing the value
would add, and choosing a second value never empties the list. A value that would leave nothing is still shown,
greyed, rather than disappearing as the reader reaches for it. The first fourteen values are shown, and the rest are
reached by asking for more. A parameter holding numbers is narrowed by the two ends of a range instead of by a list
of values: two number fields beneath a histogram of the values, which shows in colour how much of the collection the
range keeps. Widening the ends to the whole of the parameter removes the filter, because a range that keeps every
figure narrows nothing.

Each filter reads back as a chip beneath the menu, where chips can come and go without moving the menu, naming
the parameter, what it was narrowed to, and a cross (×); a chip on the labels is drawn in the green of a tag. Clicking a chip removes that filter, and **Clear** beside the
chips removes every filter. **Show all**, at the foot of the panel, removes every filter and empties the search field at
once.

### Ordering and grouping

**Sort** and **Group** each have a row of their own beneath the filters, captioned at the left with the control at
the right, as the filters are. **Sort** puts the list in order of the title or of any parameter. The button beside
its box reverses the order and says which way it runs, **Ascending ↑** or **Descending ↓**.
Numbers in a title are read as numbers, so "Run 9" comes before "Run 10". A figure that does not carry the parameter
comes last whichever way the order runs, because it has no place in an order taken from a value it does not have.
When the list is ordered by a parameter, each row shows that parameter's value beneath the title; otherwise it shows
the figure's labels.

**Group** breaks the list into runs under a heading that names the value the figures share and counts them.
Clicking a heading closes the group. Grouping by labels lists a figure under each of the labels it carries, because
that is how browsing by labels is meant to read. The figures the grouping does not apply to are gathered last, under
"not set" or "no labels".

The foot of the panel says how much of the collection is left, such as "12 of 26 figures".

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

With the Rotate tool, dragging on a three-dimensional axes changes its azimuth and elevation, and the axes box follows the pointer as in MATLAB's `rotate3d`. The artists of a three-dimensional axes are drawn with a depth buffer, so surfaces, lines, markers and images that cross one another show correctly from every angle; translucent faces are drawn back to front, which is exact except where a translucent face is nearer than an opaque one drawn after it ([issue #34](https://github.com/thclark/ironlab/issues/34)).

- Dragging to the right decreases the azimuth, and dragging to the left increases it.
- Dragging down increases the elevation, and dragging up decreases it.

The rate is half a degree for each point of pointer travel measured on the figure page, so the rate on screen depends on how large the figure is drawn. The elevation stops at −90° (looking up from below) and 90° (looking down from above). The azimuth is not limited.

## Restoring views

- **Double-clicking** an axes discards the changes made to the x, y and z limits and to the three-dimensional view of that axes, so that it shows the view the figure was opened with. Axes linked with it follow the restored limits.
- **Reset**, in the toolbar beside **Undo** and **Redo**, discards the limit and three-dimensional view changes of every axes of the figure. Pressing **R** does the same for the figure shown, unless a text field has keyboard focus or a modifier key is held.

Neither action changes the visibility of plots, so plots hidden from the legend stay hidden, and neither changes a property edited in the property editor. Both can be undone. To discard every change instead, use [Revert all changes](#taking-changes-back) at the foot of the property editor.

A two-dimensional axes whose limits were automatic receives fixed limits as soon as it is panned or zoomed, computed from the limits it was showing. The figure itself keeps its automatic limits, so restoring the view makes the limits automatic again.

## Undo and redo

Every gesture is one step of the history: a drag from press to release, one notch of the wheel, one click on a legend entry, a double-click and Reset each count as one. A change made in the [property editor](#the-property-editor) is a step in the same way: a drag of a numeric field, a visit to a text field, a choice from a combo box and a revert each count as one.

- **Undo**, in the toolbar or **⌘Z** (**Ctrl+Z** away from macOS), restores the figure to what it was before the most recent gesture.
- **Redo**, in the toolbar or **⌘⇧Z** (**Ctrl+Shift+Z**), applies the most recently undone gesture again.

Each button is disabled when there is nothing to undo or to redo, and both shortcuts are ignored while a text field has keyboard focus. Making a new gesture after undoing one discards what could have been redone, so the history never branches. Undo and redo act only on the changes made in the viewer: the figure as opened is the furthest back they go, and saving the figure makes the changes part of it, after which they can no longer be undone. [Revert all changes](#taking-changes-back) empties the history with the changes, so nothing can be undone after it either.

## Legend

Clicking an entry of a legend hides its plot, and clicking the entry again shows the plot. Either way the plot is selected, so the property editor shows it. The entry of a hidden plot stays in the legend and is drawn greyed out. Double-clicking an entry counts as two clicks, so it leaves the plot as it was, and it does not restore the limits of the axes as double-clicking elsewhere in the axes does.

A hidden plot still takes part in the layout of its axes: its data still contributes to automatic axis limits and colour limits, and it keeps its colour. Hiding or showing a plot therefore never moves, rescales or recolours anything else.

## Datatips

Resting the pointer near a point of a line or a scatter rings that point and shows its name, its coordinates and its index. The index is the position the point has in the arrays you plotted, so it is what you would use to find the same point in your own data.

Very large series are thinned to what the plot can resolve before they are drawn, on screen and in an exported PDF alike, and only the points that were drawn can be read. The index reported is always the original one, never a position in the thinned series, and zooming in draws more of the series, so more of it becomes readable. How the thinning works is described under [large series are thinned for the current view](../reference/architecture.md#large-series-are-thinned-for-the-current-view).

Resting the pointer over an image (see [images](getting-started.md#images)) outlines the pixel under it and shows the name of the image, the row and column of the pixel in the array you supplied, the x and y coordinates of the pixel's centre, and what the array holds there. A colour-mapped image shows the value of the pixel; a colour-indexed image shows the index as stored, so a floating-point index is not truncated to the entry it takes; and a true-colour image shows the red, green, blue and alpha components in the units the array stores them, bytes from 0 to 255 or fractions from 0 to 1. The row and column are the indices you would use to find the pixel in your own array, whichever way its [pixel ranges](../reference/figure-schema.md#image-placement) run, and the coordinates are those of the pixel's centre from its placement rather than of the pointer, so every position within one pixel reads the same. A pixel is read whatever colour it was painted: one left transparent by an [out-of-range policy](../reference/figure-schema.md#out-of-range-policies), or one holding NaN, still reports what it holds. A point of a line or a scatter within reach of the pointer is read in preference to the pixel beneath it, because it is small and drawn over the image, so pointing at it means the point; moving the pointer out of reach of the point reads the pixel. A pixel smaller on screen than the ring drawn around a point is ringed at its centre instead of outlined, so the mark stays visible however fine the image. Images on the floor or a wall of a three-dimensional axes have no datatips for now, because the viewer cannot yet pick in three dimensions ([issue #1](https://github.com/thclark/ironlab/issues/1)).

## Details

Beneath the canvas, a strip says what the figure carries: its [labels](describing-figures.md) as a row of tags, then
its parameters as a table in ascending order of name. It is the same information the property editor edits, shown
where it can be read at a glance beside the figure it describes, because it is what the browser narrows a
collection by and a reader choosing between figures wants to see it without opening an editor. A figure that
carries nothing has no strip, and keeps the whole height for its canvas.

## The property editor

**Properties**, in the toolbar, opens a panel on the right of the figure; it is closed when a figure is opened, so the canvas has the whole width until the panel is asked for. The panel edits the figure you are looking at: select an object, change one of its properties, and the canvas redraws at once.

The panel has three parts: the objects of the figure at the top, the properties of the selected object below, and the control that takes back every change at the foot. The foot is a strip of the same height whatever it says, and the object tree and the properties divide the rest of the panel: the boundary between them can be dragged, and the tree is held short of the point at which the properties would have no room, so that the properties of the selected object can always be reached. Both the tree and the properties scroll within the height they are given.

### Selecting an object

The upper part lists the figure, its axes in drawing order, and the plots of each axes in drawing order, as a tree that can be collapsed. Every row names the kind of the object first and the name the object carries after it in brackets, so that "Figure (A damped oscillator)", "Axes (Speed)" and "Line ($\sin \omega t$)" each say what they are as well as which one they are. The name in brackets is the title of the figure or of the axes, or the display name of the plot — the name its legend entry carries — written as its source, because the tree does not typeset mathematics. An axes with no title is named by the cell it occupies, as "Axes (row 0, col 1)", which is what tells two untitled axes apart. A figure with no title and a plot with no display name are named by their kind alone. A hidden plot is greyed, so a plot missing from the canvas can be found and shown again.

Clicking a row selects that object and shows its properties below. Clicking inside an axes on the canvas selects that axes, and clicking a legend entry selects its plot as well as hiding or showing it. The canvas cannot pick an individual plot ([issue #1](https://github.com/thclark/ironlab/issues/1)), so the tree is the way to reach one. No gesture changes the selection: panning, zooming and rotating leave the panel showing what it was showing.

### Changing a property

The properties of the selected object are gathered under the value they belong to — the scale, limits and grid lines of the x axis appear together under "x" — and each row carries the control that suits what it holds: a checkbox, a number that is dragged or typed into, a text field with a choice of LaTeX or literal, a colour picker, or a combo box of the values the property can take. What each property means is described in the [figure schema](../reference/figure-schema.md), and hovering over the name of a property shows the same explanation.

A value that holds other values, such as the x axis or the style of a line, is a heading carrying its name alone, and the values it holds are the rows beneath it. The heading is never a value in itself, because everything it holds is already on screen below it.

The properties are ordered alphabetically by the name you read, so that a property can be found by its name rather than by learning where the figure schema happens to list it. The headings and the properties that belong to no heading are ordered together as one list, because both are read at the left edge of the panel; the rows gathered under a heading are ordered among themselves. Letter case is ignored. The object tree above is not ordered this way: it stays in drawing order, which is what tells you which plot is drawn over which.

Every row is laid out in the same three columns, so that the panel is read down them rather than along each row. The name of the property is at the left, indented by how deeply the property is nested, so that a heading and a property that belongs to no group share a left edge and the properties gathered under a heading are indented beneath it. The control is at the right of the column beside the names, so that the controls of an object line up with one another whatever they are. The control that takes back a change to one property occupies a column of its own at the far right, which is kept clear on every row, so that no control moves sideways when the property it changes becomes one the user has changed.

A property that is absent, such as an axes with no title, is shown as "unset" with a control that gives it a value. The pixel ranges of an image are unset while its pixel centres lie at 0, 1, …; setting one gives it the range from 0 to 1, whose first and last centres are then typed in. A property that the figure does not use in the state the object is in is not shown until it applies: the bounds of manual limits appear once the limits are manual, the camera of an axes once the axes is three-dimensional, and the label, scale, limits and grid lines of the z axis once the axes is three-dimensional, because a two-dimensional axes ignores its z axis. None of this is an error, and each row returns as soon as the property applies again.

A change is recorded exactly as a gesture is: it is added to the overlay, the canvas redraws, and the figure the viewer was given is untouched until it is saved. A whole drag of a numeric field, and a whole visit to a text field, is one step of the history, so **Undo** takes back the change rather than the last pixel of it.

A change the figure cannot accept — limits that are not increasing, or a projection a plot cannot be drawn in — is refused. The figure is left exactly as it was, nothing is added to the history, and the reason appears in the [problems list](#problems).

### Taking changes back

A property you have changed is shown in bold and carries a **↺** control, which takes back that one change and shows the figure's own value again. Reverting one property never disturbs another, and is itself a step of the history.

**Revert all changes**, in the bottom-right corner of the panel, discards every change you have made to this figure — axis limits, three-dimensional views, hidden plots and every property edited — and shows the figure as the program that built it defined it. The control says how many changes it would discard and is disabled when there are none. It is a clean slate rather than a step of the history: **Undo** does nothing after it, because no change is left to take back. The figure the viewer was given is never touched, so what **Revert all changes** restores is exactly that figure.

It is wider than **Reset** in the toolbar, which discards only the limits and three-dimensional views, keeps hidden plots hidden and every property you have edited, and can itself be undone.

### Parameters

The properties of the figure include its **parameters**: the named values that describe it, which are what make a collection of figures sortable and searchable (see [parameters](getting-started.md#parameters) and the [figure schema](../reference/figure-schema.md#parameters)). They are edited as a small table, in which an entry can be added, renamed, given another kind (yes or no, whole number, number or text), changed and removed. The whole table is committed together, so a change to it is one step of the history.

While the table cannot be committed — an entry has no name, two entries share a name, or a number has not been typed in full — the reason is shown below it and the figure keeps the parameters it had. Typing is never interrupted; the change reaches the figure as soon as the table makes sense again.

### Labels

The properties of the figure also include its **labels**: the free words that describe it, which are what the
[figure browser](#the-figure-browser) filters and groups by (see [labels](getting-started.md#labels) and the
[figure schema](../reference/figure-schema.md#labels)). They are edited as one text field holding them separated by
commas, because they are short words and there are a handful of them. The spaces around a word are trimmed and an
empty word is dropped, so a trailing comma while typing never makes a label of nothing.

A label added twice is refused, and the reason appears in the problems list. The labels the browser reads off the
figure itself are not shown here and cannot be edited, because they are not part of the figure: they are worked out
from what it draws each time it is opened.

### What the editor does not change

The editor changes the properties of the objects a figure already has. Its structure — the tile layout, where each axes sits in it, which axes there are and which plots they hold — and the data those plots draw come from the program that builds the figure.

**A property the editor cannot change is shown with its value, drawn dimmed, and the reason it cannot be changed in its tooltip, and with nothing else beside it.** The dimmed value is what says the property is not yours to set here; hovering it says why. Four kinds of property are shown this way.

- **The data a plot draws**, shown as the array it names and that array's shape.
- **The rows and columns of the figure's tile layout, and the cell each axes occupies within it.** Where an axes sits is part of how the figure is arranged, which the program that builds it defines.
- **The groups of axes whose limits are linked**, shown as a count.
- **The marker size of a scatter**, because a scatter sizes its markers by its own **size**, which overrides it. Change **size** instead, to give every marker the same size or to take each marker's size from an array. Every other plot draws its markers at the size in its marker style, which stays editable.

A property that the figure merely constrains is not read-only. Limits that must increase and a font size that must be positive stay editable, and a value the figure will not accept is refused with its reason, which is the more useful answer.

A choice a combo box cannot honour is shown in the same spirit: it is listed, drawn greyed, and cannot be picked, and hovering over it gives the reason and says where the same choice does work. Removing it would hide from you that the figure has the value at all, whereas a greyed entry invites you to find the property that makes it available.

A colour may be **colormapped**, which colours a plot from its data through the axes colormap, and that works only where the figure holds a value to look the colour up by: the isolines of a contour, coloured by their level; the faces and edges of a surface, coloured by its field or its colour data; and a scatter whose colour comes from an array. A line, a quiver and a scatter with a single colour hold no such value and would be drawn in the middle colour of the colormap, and a marker takes the colour of the plot it belongs to whether or not it is colormapped, so the choice is greyed for all of them.

The **clamp** choice of an out-of-range policy, which draws a pixel of a colour-indexed or colour-mapped image that falls outside the range in the nearest end colour of the colormap, is greyed for the **non_finite** category of the image, because a value that is not finite has no nearest end of the colormap to be clamped to; it stays available for the **below** and **above** categories. What each choice draws is described under [out-of-range policies](../reference/figure-schema.md#out-of-range-policies).

## Linked axes

When axes are linked along a dimension (see [linking axes](getting-started.md#linking-axes)), panning, zooming, box zooming or double-clicking one of them changes the limits of every axes in its group along that dimension. Axes that are not linked along that dimension are unaffected. For example, when the x limits of two axes are linked and their y limits are not, panning one of them moves both horizontally but only itself vertically.

Links apply to limits only. The azimuth, elevation, magnification and position of a three-dimensional view belong to its own axes, and rotating or zooming one three-dimensional axes does not change another.

## Saving the figure

**Save figure…**, in the toolbar, opens a save dialog and writes the figure as it is currently shown, with its current limits, three-dimensional views and plot visibility. The format follows the extension of the name given: `.fig` writes the default Protocol Buffers format and `.json` (including `.fig.json`) writes JSON, as described in [saving and loading](getting-started.md#saving-and-loading). A name with any other extension is refused and no file is written.

The figure written is the figure the viewer now holds: the changes saved become part of it, the undo history is emptied, and Reset restores the view as saved rather than the view the file was opened with. A notification in the bottom-right corner of the window reports whether the save succeeded.

## Exporting to PDF

**Export PDF…**, in the toolbar, opens a save dialog and writes the figure as it is currently shown: with its current limits, current three-dimensional views, and without the plots hidden from the legend. The exported page has the same properties as one written by the API, which are described in [exporting PDF](getting-started.md#exporting-pdf), and it is written with the default settings for [dense surfaces](getting-started.md#dense-surfaces). A notification in the bottom-right corner of the window reports whether the export succeeded.

## Problems

When something is wrong with a figure, the toolbar shows a problems indicator: the number of problems, for example "2 problems", written in the warning colour. It appears only when there is something to report. Clicking it opens a list of the problems and clicking away closes the list again.

Each entry names the object the problem concerns, as the [object tree](#selecting-an-object) names it, and the property where one is concerned. It then gives the reason in full and says how the problem arose, in one of three ways.

- **Reported while the figure was drawn.** Drawing revealed something that does not prevent the figure from being drawn, such as a LaTeX expression the typesetter does not support, data that cannot be placed on a logarithmic axis, or an artist whose data gives it nothing to draw (a series of no points, an image with no rows or no columns, or a field with no cell between its nodes). Unsupported mathematics is drawn as its raw source and an artist that cannot be drawn is left out, so the figure still appears. These are the problems that `Figure::validate` reports as warnings, and that the report returned by `Figure::export_pdf` lists, as described in [validating a figure](getting-started.md#validating-a-figure).
- **Your change could not be shown, so it was discarded.** A change you made can no longer be applied to the figure, so it was thrown away rather than silently ignored.
- **Your change was refused, so the figure is unchanged.** A change made in the property editor was checked before it was recorded and the figure would not accept it, so the figure keeps the value it had and the history keeps no empty step.

The last two are reported until the change they concern is superseded, undone, reverted or discarded, after which the indicator falls silent again. Problems reported while the figure was drawn come back whenever the figure is drawn, so they persist until the figure itself changes.
