# Using the viewer

The IronLAB viewer is a desktop window that shows one figure at a time and lets it be explored with the mouse or trackpad. How to open it is described in [getting started](getting-started.md#opening-the-viewer). When more than one figure is open, the [figure browser](#the-figure-browser) down the left-hand side is how the others are reached: it narrows them down to the one to look at. The same figures can be shown live in a web page, drawn by the same engine, as described [below](#in-the-browser).

The figure has a toolbar above a canvas, and beneath the canvas a strip of [details](#details). The canvas shows the figure as a page preview: the whole figure at its physical aspect ratio, scaled to fit the window. What the canvas shows is drawn from the same geometry as an exported PDF, so the preview and the export agree.

Every interaction described on this page changes the figure by setting a property of it — an axis limit, a three-dimensional view or the visibility of a plot — exactly as setting that property from the Rust API would. The viewer keeps the figure as it was opened, called the source, and the changes made to it, called the overlay, apart: what the canvas draws, what **Export PDF…** writes and what **Save figure…** writes is the source with the overlay applied. Because each change is a value of its own, any of them can be undone, and a view is restored by discarding the changes made to it rather than by copying an earlier figure back. The reasons for this design are recorded in [ADR 0008](../adrs/0008-typed-edits-and-a-view-overlay.md).

## The figure browser

When the viewer opens more than one figure, a browser appears down the left-hand side. It narrows the open figures
down to the one to look at, and clicking a figure in its list shows that figure; the row of the figure shown is
marked in the selection colour, and every row shows the figure's tags beneath its title, or the value the list is
ordered by. Dragging the browser's edge past its narrowest width shuts it, leaving a handle to drag it open again. A
viewer holding a single figure has nothing to browse, and opens without it.

The browser is built around the [tags and parameters](describing-figures.md) of a figure, which exist so that a collection of figures can be sorted,
filtered and searched. It offers exactly what the figures carry and nothing else: the viewer works out no
properties of its own, because the program that built the figures is the only thing that knows which of their
properties matter. A collection whose figures carry no tags and no parameters can still be searched by title and
listed, but there is nothing to filter it by until its author says what there should be.

When no figure of a collection carries a tag or a parameter, there is nothing to narrow it by, and the browser
says so rather than offering an empty menu: it explains that descriptions are added where the figures are built,
and links to [describing figures](describing-figures.md). The figures are still listed and can still be searched by
title.

An empty tag, or one a figure carries twice, is shown once and reported in the problems list: neither is possible
in a figure IronLAB wrote, but a file written by another program can carry either.

### Searching

The field at the top of the browser searches the titles, the tags and the parameters. Typing `surface` leaves the
figures that mention it anywhere; the list narrows as each character is typed.

A word can also ask about one parameter, which is how a search becomes precise:

| Typed | Meaning |
| --- | --- |
| `surface` | the figure mentions "surface" anywhere |
| `-stalled` | it does not mention "stalled" |
| `rig:CFD` | its `rig` parameter contains "CFD", ignoring case |
| `rig=CFD` | its `rig` parameter is exactly "CFD", ignoring case |
| `angle>=8` | its `angle` parameter is 8 or more, and `>`, `<` and `<=` compare in the same way |
| `tag:piv` | one of its tags contains "piv"; `tags:` asks the same |

Words are cumulative: every one of them must hold. A word only asks about a parameter when the collection has one
of that name, so a colon in a title, a file path or a web address searches rather than asking about a parameter
that does not exist. A figure that does not carry the parameter a word asks about is left out, so `solver:LES`
finds the computed runs and leaves the measured ones out, which is what asking about a solver means. Underscores
inside a typed number are separators, so `mesh_cells>=2_000_000` reads as it was written in the code that set it.

### Filtering

**Add**, at the right of the **Filters** caption, opens a menu within the panel beneath the caption, so that what
is beneath it moves down and the menu stays open until it is shut. Its first page lists the parameters worth
filtering on, the most useful first, each with how many values it takes and how much of the collection carries it;
typing in the field above the list narrows it. Choosing a parameter turns the page to its values, each with the
number of figures choosing it would leave. **Back to all parameters** turns back, **Done** shuts the menu, and
the control that opened it, which reads **Close** while the menu is open, shuts it too. A parameter that takes the same value on every figure divides nothing and is
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
the parameter, what it was narrowed to, and a cross; a chip that narrows the tags is drawn in the green of a tag. Clicking a chip removes that filter, and **Clear all**, which
appears beside **Add** while there is a filter and carries a restore mark, removes every filter. **Show all**, at the foot of the panel, removes every filter and empties the search field at
once.

### Ordering and grouping

**Sort** and **Group** each have a row of their own beneath the filters, captioned at the left with the control at
the right, as the filters are. **Sort** puts the list in order of the title or of any parameter. The button beside
its box reverses the order and says which way it runs, **Ascending** or **Descending**, with the triangle of a
combo box beside the word pointing up or down to match.
Numbers in a title are read as numbers, so "Run 9" comes before "Run 10". A figure that does not carry the parameter
comes last whichever way the order runs, because it has no place in an order taken from a value it does not have.
When the list is ordered by a parameter, each row shows that parameter's value beneath the title; otherwise it shows
the figure's tags.

**Group** breaks the list into runs under a heading that names the value the figures share and counts them.
Clicking a heading closes the group. Grouping by tags lists a figure under each of the tags it carries, because
that is how browsing by tags is meant to read. The figures the grouping does not apply to are gathered last, under
"not set" or "no tags".

The foot of the panel says how much of the collection is left, such as "12 of 26 figures".

## Tools

The toolbar has three tool buttons, which decide what dragging with the primary mouse button does. The active tool is highlighted, and Pan is active when a figure is opened.

| Tool | Dragging on a two-dimensional axes | Dragging on a three-dimensional axes |
| --- | --- | --- |
| **Pan** | Moves the limits so that the point grabbed stays under the pointer. | Moves the projected box so that it follows the pointer. |
| **Zoom** | Draws a rubber band and zooms to it on release. | Does nothing. |
| **Rotate** | Does nothing. | Rotates the view. |

The Rotate button is disabled when the figure has no three-dimensional axes. A drag acts on the axes under the pointer when the drag starts, and a drag that starts outside every axes does nothing. A drag that starts on a [colourbar](#colourbars) pans the limits of its colour scale instead, whichever tool is active.

## Zooming

Scrolling with the mouse wheel, or pinching on a trackpad, zooms the axes under the pointer, whichever tool is active; over a [colourbar](#colourbars) it zooms the limits of the colour scale instead. Scrolling up zooms in and scrolling down zooms out; one notch of a typical mouse wheel changes the scale by about 20 %.

- On a two-dimensional axes, the x and y limits are rescaled about the data point under the pointer, so that this point stays where it is. On a logarithmic axis the limits are rescaled in logarithmic space, so zooming behaves identically on every decade.
- On a three-dimensional axes, the magnification of the view changes; the limits of its axes do not.

With the Zoom tool, dragging on a two-dimensional axes draws a rubber band, which is confined to the plot area. On release, the x and y limits are set to the range of data that the band covers. A band less than 3 pt wide or high, measured on the figure page, is ignored, so that a click does not zoom to a sliver.

On an axes with a fixed [aspect](getting-started.md#equal-aspect), the shorter side of the band, relative to the shape of the plot area, is expanded about its centre until the band has the shape of the plot area, and the band is drawn expanded, so that it shows exactly what the zoom will show. The expanded band may reach beyond the plot area, because it holds everything that was selected. The wheel zooms both axes by the same factor and panning shifts both, so every gesture keeps the aspect, and an axes whose aspect is kept by widening its limits is navigated from the widened limits it shows. The box of a three-dimensional axes at a fixed aspect takes its proportions from its limits alone, so rotating, zooming and panning its view never change them.

## Panning

With the Pan tool, dragging on a two-dimensional axes shifts its x and y limits so that the data point grabbed at the start of the drag stays under the pointer. Dragging on a three-dimensional axes moves the projected box with the pointer; the limits of its axes do not change.

## Rotating

With the Rotate tool, dragging on a three-dimensional axes changes its azimuth and elevation, and the axes box follows the pointer as in MATLAB's `rotate3d`. The artists of a three-dimensional axes are drawn with a depth buffer, so surfaces, lines, markers and images that cross one another show correctly from every angle; opaque artists are drawn first and translucent faces, images, lines and markers after them, back to front, so that a translucent surface never hides what lies behind it, as [alpha and opacity](../reference/figure-schema.md#alpha-and-opacity) describes.

- Dragging to the right decreases the azimuth, and dragging to the left increases it.
- Dragging down increases the elevation, and dragging up decreases it.

The rate is half a degree for each point of pointer travel measured on the figure page, so the rate on screen depends on how large the figure is drawn. The elevation stops at −90° (looking up from below) and 90° (looking down from above). The azimuth is not limited.

## Restoring views

- **Double-clicking** an axes discards the changes made to the x, y and z limits and to the three-dimensional view of that axes, so that it shows the view the figure was opened with. Axes linked with it follow the restored limits.
- **Double-clicking** a colourbar discards the changes made to the limits of its colour scale, so that every bar of the scale and every plot that maps through it shows the limits the figure was opened with.
- **Refit**, in the toolbar after **Rotate**, discards the limit and three-dimensional view changes of every axes of the figure, and the limit changes of every colour scale. Pressing **R** does the same for the figure shown, unless a text field has keyboard focus or a modifier key is held.

None of these actions changes the visibility of plots, so plots hidden from the legend stay hidden, and none changes a property edited in the property editor other than the limits it restores; the norm and the number of bands of a colour scale, for example, are kept. Each can be undone. To discard every change instead, use [Revert all changes](#taking-changes-back) at the foot of the property editor.

A two-dimensional axes whose limits were automatic receives fixed limits as soon as it is panned or zoomed, computed from the limits it was showing, and so does a colour scale whose limits were automatic. The figure itself keeps its automatic limits, so restoring the view makes the limits automatic again.

## Undo and redo

Every gesture is one step of the history: a drag from press to release, one notch of the wheel, one click on a legend entry, a double-click and Refit each count as one. A change made in the [property editor](#the-property-editor) is a step in the same way: a drag of a numeric field, a visit to a text field, a choice from a combo box and a revert each count as one.

- **Undo**, in the toolbar or **⌘Z** (**Ctrl+Z** away from macOS), restores the figure to what it was before the most recent gesture.
- **Redo**, in the toolbar or **⌘⇧Z** (**Ctrl+Shift+Z**), applies the most recently undone gesture again.

Each button is disabled when there is nothing to undo or to redo, and both shortcuts are ignored while a text field has keyboard focus. Making a new gesture after undoing one discards what could have been redone, so the history never branches. Undo and redo act only on the changes made in the viewer: the figure as opened is the furthest back they go, and saving the figure makes the changes part of it, after which they can no longer be undone. [Revert all changes](#taking-changes-back) empties the history with the changes, so nothing can be undone after it either.

## Legend

Clicking an entry of a legend hides its plot, and clicking the entry again shows the plot. Either way the plot is selected, so the property editor shows it. The entry of a hidden plot stays in the legend and is drawn greyed out.

An entry of a [legend panel](legends.md#legend-panels) may stand for several plots, one in each axes that shares the series. Clicking such an entry hides all of its plots, or shows them all when every one of them is already hidden, and selects the first of them. The entry is greyed only when all of its plots are hidden. How entries come to be shared is explained in [sharing and merging entries](legends.md#sharing-and-merging-entries). An item of an [explicit list of entries](legends.md#choosing-the-entries) that names a plot behaves in the same way, and an item that stands for no plot, such as a marker shape explained in a second key, is not a toggle: clicking it does nothing. Nor is an item that shows a [colour scale](colour.md#colour-keys-in-legends): clicking its bar, or the entry of one of its categories, hides nothing, unless that entry is merged with the entry of a plot; clicking its bar selects the scale, as clicking any [colourbar](#colourbars) does. Double-clicking an entry counts as two clicks, so it leaves the plot as it was, and it does not restore the limits of the axes as double-clicking elsewhere in the axes does.

A hidden plot still takes part in the layout of its axes: its data still contributes to automatic axis limits and colour limits, and it keeps its colour. Hiding or showing a plot therefore never moves, rescales or recolours anything else.

## Colourbars

A [colourbar](colour.md#showing-a-scale-with-a-colourbar) is the axis of its colour scale, and the viewer treats it as one: hovering it reads the scale, and the wheel and a drag along it zoom and pan the scale's limits as they zoom and pan the limits of an axes. A colourbar inside an axes is drawn over the plots of the axes, on a box that hides them, so the bar and its box take the pointer before anything beneath them: over the box beside the bar nothing is read, and a gesture there changes nothing.

### Reading a bar

Resting the pointer on a bar shows what the scale says there, under the title of the scale when it has one, and marks what was read.

| What the pointer rests on | What is shown | What is marked |
| --- | --- | --- |
| A continuous bar | `value = …`, the value under the pointer. On a logarithmic scale the value is read in decades, so halfway up a bar of 1 to 100 reads 10. | A ring on the centre line of the bar, level with the pointer. |
| A discrete bar | `band = low – high`, the band under the pointer from its lower edge to its upper edge, because every value of a band is drawn in its one colour. A value exactly on an edge belongs to the band above it. | The outline of the band. |
| An indexed bar | `index = k`, the entry whose block is under the pointer, or `entry = name (index k)` when the scale names its ticks and one of them falls at that entry. | The outline of the entry's block. |
| A square of a categorical scale | `category = name (index k)`, as a pixel of an image through the scale reads. | The outline of the square. |
| An out-of-range key | The label beside the key, the author's or the default one as drawn, because the label is the whole statement of which values the key stands for. | The outline of the key. |

The title, the tick labels and the gaps between the pieces of a bar read nothing. A [bar item of a legend](colour.md#bars-are-items) is read exactly as a colourbar is. Every number is written as a [datatip](#datatips) writes coordinates.

### Zooming and panning a scale

Scrolling over a continuous bar, or over a discrete bar of automatic levels, zooms the limits of its scale about the value under the pointer, so that the colour you are looking at stays where it is; on a logarithmic scale the zoom is made in decades, so the limits stay positive however far it zooms out. Dragging along such a bar pans the limits, so that the value grabbed at the start of the drag stays under the pointer, with every tool alike, because a bar has one dimension and no camera; the Zoom tool draws no rubber band on a bar, and motion across the bar pans nothing. Nothing in the data bounds a pan, because the limits of a scale are a choice, and a reader may well pan past the values drawn to see where they sit in a wider range.

A gesture on a bar sets the limits of its scale exactly as the property editor would: one change of `kind.continuous.limits`, or of `kind.discrete.levels.limits` for a discrete scale, whose number of bands is kept. Every bar that shows the scale and every plot that maps through it is drawn from the one changed scale, so no bar can disagree with another or with the plots. Limits that were automatic become fixed at the limits the bar was showing, while the figure keeps its automatic limits, so [restoring the view](#restoring-views) makes them automatic again. Each wheel notch and each drag is one step of the [history](#undo-and-redo), on a colourbar inside a three-dimensional axes as anywhere else, and the axes beneath an inside bar is never moved by a gesture on the bar.

Some bars have no limits to move, and a gesture on them changes nothing and adds nothing to the history: an indexed or categorical bar, which is a list of entries; a discrete bar whose band edges the author listed, because moving every edge is not a pan; and a bar item of a legend, which belongs to its legend. Clicking any bar selects its colour scale in the [property editor](#the-property-editor), where its limits, its palette and everything else it draws can be changed.

## Datatips

Resting the pointer near a point of a series, whether drawn as a line or as markers, rings that point and shows its name, its coordinates and its index. The index is the position the point has in the arrays you plotted, so it is what you would use to find the same point in your own data. A series that places a dimension as fractions of the plot area, such as a [reference line or a row of edge markers](getting-started.md#reference-lines-and-edge-markers), reports only its data coordinate, because a fraction says where the mark was drawn rather than what was measured.

Very large series are thinned to what the plot can resolve before they are drawn, on screen and in an exported PDF alike, and only the points that were drawn can be read. The index reported is always the original one, never a position in the thinned series, and zooming in draws more of the series, so more of it becomes readable. How the thinning works is described under [large series are thinned for the current view](../reference/architecture.md#large-series-are-thinned-for-the-current-view).

Resting the pointer over an image (see [images](getting-started.md#images)) outlines the pixel under it and shows the name of the image, the row and column of the pixel in the array you supplied, the x and y coordinates of the pixel's centre, and what the array holds there. A scalar image through a continuous or discrete [colour scale](colour.md) shows the value of the pixel; through an indexed scale it shows the index as stored, so a fractional index, which names no entry, is shown as it is; through a categorical scale it shows the label of the category that the index names, with the index; and a true-colour image shows the red, green, blue and alpha components in the units the array stores them, bytes from 0 to 255 or fractions from 0 to 1. The row and column are the indices you would use to find the pixel in your own array, whichever way its [pixel ranges](../reference/figure-schema.md#image-placement) run, and the coordinates are those of the pixel's centre from its placement rather than of the pointer, so every position within one pixel reads the same. A pixel is read whatever colour it was painted: one left transparent by an [out-of-range policy](colour.md#values-out-of-range) of its scale, or one holding NaN, still reports what it holds. A point of a line or a scatter within reach of the pointer is read in preference to the pixel beneath it, because it is small and drawn over the image, so pointing at it means the point; moving the pointer out of reach of the point reads the pixel. A pixel smaller on screen than the ring drawn around a point is ringed at its centre instead of outlined, so the mark stays visible however fine the image. Images on the floor or a wall of a three-dimensional axes have no datatips for now, because the viewer cannot yet pick in three dimensions ([issue #1](https://github.com/thclark/ironlab/issues/1)). A [colourbar](#colourbars) is read before the points and pixels beneath it, because it is drawn over them.

## Details

Beneath the canvas, a strip says what the figure carries: its [tags](describing-figures.md) as a row of tag chips, then
its parameters as a table in ascending order of name. It is the same information the property editor edits, shown
where it can be read at a glance beside the figure it describes, because it is what the browser narrows a
collection by and a reader choosing between figures wants to see it without opening an editor. A figure that
carries nothing has no strip, and keeps the whole height for its canvas.

## The property editor

**Properties**, in the toolbar, opens a panel on the right of the figure; it is closed when a figure is opened, so the canvas has the whole width until the panel is asked for. The panel edits the figure you are looking at: select an object, change one of its properties, and the canvas redraws at once.

The panel has three parts: the objects of the figure at the top, the properties of the selected object below, and the control that takes back every change at the foot. The foot is a strip of the same height whatever it says, and the object tree and the properties divide the rest of the panel: the boundary between them can be dragged, and the tree is held short of the point at which the properties would have no room, so that the properties of the selected object can always be reached. Both the tree and the properties scroll within the height they are given.

### Selecting an object

The upper part lists the figure, a group named "Data arrays" holding its data arrays, its axes in drawing order with the plots of each axes in drawing order beneath it, and a group named "Panels" holding its panels, as a tree that can be collapsed. A group is listed only when the figure has something to put in it. Every row names the kind of the object first and the name the object carries after it in brackets, so that "Figure (A damped oscillator)", "Axes (Speed)" and "Line ($\sin \omega t$)" each say what they are as well as which one they are. The name in brackets is the title of the figure or of the axes, or the label of the plot — the text its legend entry carries — written as its source, because the tree does not typeset mathematics. An axes with no title is named by the cell it occupies, as "Axes (row 0, col 1)", which is what tells two untitled axes apart. A panel with a [title](legends.md#titling-a-panel) is named by its title and the cells it occupies, as "Key (row 2, col 0)", because the title is what it shows on the page and two panels may carry one title; a panel without one, or with a blank one, is named by the kind of its content and its cells, as "Legend panel (row 2, col 0)" or "Colourbar panel (rows 0–1, col 2)". A data array draws nothing on the page, so it is named by the name its program gave it, as "Data array (time (s))", or, when it has none, by its shape, as "Data array (1000 × 3)". A figure with no title and a plot with no label are named by their kind alone. A hidden plot is greyed, so a plot missing from the canvas can be found and shown again.

Clicking a row selects that object and shows its properties below. Clicking inside an axes on the canvas selects that axes, clicking a colourbar selects its colour scale, and clicking a legend entry selects its plot as well as hiding or showing it. The canvas cannot pick an individual plot ([issue #1](https://github.com/thclark/ironlab/issues/1)), so the tree is the way to reach one. No gesture changes the selection: panning, zooming and rotating leave the panel showing what it was showing.

### Changing a property

The properties of the selected object are gathered under the value they belong to — the scale, limits, direction, ticks and grid lines of the x axis appear together under "x" — and each row carries the control that suits what it holds: a checkbox, a number that is dragged or typed into, a text field with a row beneath it that says whether the text is read as LaTeX or literally, a colour swatch that opens a picker, or a combo box of the values the property can take. What each property means is described in the [figure schema](../reference/figure-schema.md), and hovering over the name of a property shows the same explanation.

A value that holds other values, such as the x axis or the style of a line, is a row carrying its name alone, in the same text as every other name, with a triangle before it that opens and closes the values it holds, as a node of the object tree does; those values are indented beneath it with a fine line down their left. The row is never a value in itself, because everything it holds is already on screen below it.

The properties are ordered alphabetically by the name you read, so that a property can be found by its name rather than by learning where the figure schema happens to list it. The headings and the properties that belong to no heading are ordered together as one list, because both are read at the left edge of the panel; the rows gathered under a heading are ordered among themselves. Letter case is ignored. The object tree above is not ordered this way: it stays in drawing order, which is what tells you which plot is drawn over which.

Every row is laid out in the same three columns, so that the panel is read down them rather than along each row. The name of the property is at the left, indented by how deeply the property is nested, so that a heading and a property that belongs to no group share a left edge and the properties gathered under a heading are indented beneath it. The control begins at the left of the column beside the names, so that the controls of an object line up with one another whatever they are, and a field or a combo box fills the column. The control that takes back a change to one property occupies a column of its own at the far right, which is kept clear on every row, so that no control moves sideways when the property it changes becomes one the user has changed.

A property that is absent, such as an axes with no title, is shown as "unset" with a control that gives it a value. The pixel ranges of an image are unset while its pixel centres lie at 0, 1, …; setting one gives it the range from 0 to 1, whose first and last centres are then typed in. A property that the figure does not use in the state the object is in is not shown until it applies: the bounds of manual limits appear once the limits are manual, the camera of an axes once the axes is three-dimensional, the label, scale, limits and grid lines of the z axis once the axes is three-dimensional, because a two-dimensional axes ignores its z axis, and the ratios of y and of z once the aspect is a ratio. None of this is an error, and each row returns as soon as the property applies again.

The entries of a palette are properties of their own. Each colour of an [indexed scale](colour.md#indexed-and-categorical-scales) has a colour row, `indexed.colors.0`, `indexed.colors.1` and so on; each category of a categorical scale has a colour row and a label row, `categorical.categories.0.color` and `categorical.categories.0.label`; and each colour of a listed colormap has a colour row, `continuous.colormap.colors.0` or `discrete.colormap.colors.0`. The rows are ordered by index, so entry 10 follows entry 9. Changing one entry changes that entry alone, and its revert control takes back that one change. How many entries a palette has, and their order, come from the program that builds the figure, so no row adds, removes or reorders entries.

The opacity of an image, and the opacities of the faces and of the edges of a surface, are numbers held between 0 and 1, so that dragging one fades the artist in or out without regenerating its data; the alpha arrays that vary the transparency over an artist are data, and are shown as the arrays they name. The difference between the two is explained under [alpha and opacity](../reference/figure-schema.md#alpha-and-opacity).

A change is recorded exactly as a gesture is: it is added to the overlay, the canvas redraws, and the figure the viewer was given is untouched until it is saved. A whole drag of a numeric field, and a whole visit to a text field, is one step of the history, so **Undo** takes back the change rather than the last pixel of it.

A change the figure cannot accept — limits that are not increasing, or a projection a plot cannot be drawn in — is refused. The figure is left exactly as it was, nothing is added to the history, and the reason appears in the [problems list](#problems).

### Taking changes back

A property you have changed is named in the colour that marks your own choices everywhere in the viewer, the blue of a filter chip, and carries a revert control at the right of its row, which takes back that one change and shows the figure's own value again. Reverting one property never disturbs another, and is itself a step of the history.

**Revert all changes**, in the bottom-right corner of the panel, discards every change you have made to this figure — axis limits, three-dimensional views, hidden plots and every property edited — and shows the figure as the program that built it defined it. The control says how many changes it would discard and is disabled when there are none. It is a clean slate rather than a step of the history: **Undo** does nothing after it, because no change is left to take back. The figure the viewer was given is never touched, so what **Revert all changes** restores is exactly that figure.

It is wider than **Refit** in the toolbar, which discards only the limits and three-dimensional views, keeps hidden plots hidden and every property you have edited, and can itself be undone.

### Parameters

The properties of the figure include its **parameters**: the named values that describe it, which are what make a collection of figures sortable and searchable (see [parameters](getting-started.md#parameters) and the [figure schema](../reference/figure-schema.md#parameters)). They are edited as a small table beneath the row that names them, which opens and closes the table as a value that holds other values does; in the table an entry can be added, renamed, given another kind (boolean, integer, number or string), changed and removed. The whole table is committed together, so a change to it is one step of the history.

While the table cannot be committed — an entry has no name, two entries share a name, or a number has not been typed in full — the reason is shown below it and the figure keeps the parameters it had. Typing is never interrupted; the change reaches the figure as soon as the table makes sense again.

### Tags

The properties of the figure also include its **tags**: the free words that describe it, which are what the
[figure browser](#the-figure-browser) filters and groups by (see [tags](getting-started.md#tags) and the
[figure schema](../reference/figure-schema.md#tags)). They are edited as one text field holding them separated by
commas, because they are short words and there are a handful of them. The spaces around a word are trimmed and an
empty word is dropped, so a trailing comma while typing never makes a tag of nothing.

A tag added twice is refused, and the reason appears in the problems list. Tags were called labels in earlier
versions; [ADR 0023](../adrs/0023-titles-labels-names-and-tags.md) records why the word label is now kept for text that
is drawn.

### What the editor does not change

The editor changes the properties of the objects a figure already has. Its structure — the tile layout, where each axes and panel sits in it, which axes, plots and panels there are, and what each panel holds — and the data those plots draw come from the program that builds the figure.

**A property the editor cannot change is shown with its value, drawn dimmed, and the reason it cannot be changed in its tooltip, and with nothing else beside it.** The dimmed value is what says the property is not yours to set here; hovering it says why. Six kinds of property are shown this way.

- **The data a plot draws**, shown as the array it names and that array's shape. The size data and colour data of a series are among them. The marker size of a series stays editable: it sizes every marker of a series without size data, and the marker of its legend sample in either case.
- **The data arrays of the figure.** Selecting a data array shows its name and its shape, and neither can be changed, because the values come from the program that wrote the figure. An array may be shared by several plots, and [sharing data between plots](getting-started.md#sharing-data-between-plots) explains how.
- **The rows and columns of the figure's tile layout, the cell each axes or panel occupies within it, and the kind of content a panel holds.** Where an axes or a panel sits, and whether a panel holds a legend or a colourbar, is part of how the figure is arranged, which the program that builds it defines. The relative sizes of the rows and columns are not part of that arrangement, and stay editable, as a list of weights in which the word `fit` stands for a row or column [fitted to its legends](legends.md#fitting-rows-and-columns-to-their-legends).
- **The legend panel in which an axes lists its entries**, shown as the identifier of the panel, because which axes share a legend is part of how the figure is arranged.
- **The colour scale that a plot maps its numbers through, and the colour scale that a colourbar shows**, shown as the identifier of the scale, because which plots and bars share a scale is part of how the figure is arranged. The scale itself is edited in its own row under "Colour scales", and every plot and colourbar that names it follows the change.
- **The groups of axes whose limits are linked**, shown as a count.

The properties of a [colourbar](colour.md#showing-a-scale-with-a-colourbar) stay editable: the orientation, the length and the placement of the scale's title of a colourbar in a panel, and the side, the length and the placement of the scale's title of a colourbar inside an axes, which are found among the properties of its axes. The length of a panel colourbar is optional: clearing it restores the automatic length, the whole span of the plots for a vertical bar and half of it for a horizontal one. The placement of the title is offered as a choice of **Along** and **Above**. What the bar draws, its colours, limits, ticks, title and keys, belongs to its scale and is edited there.

The [title](legends.md#titling-a-panel) of a panel and its alignment are edited like the title of an axes, whatever the panel holds: the title as optional text, so that a panel may have none, and its alignment as a choice of **Left**, **Centre** and **Right**. A length or a title that the figure cannot draw, such as a length of zero, is refused with the reason, as any other invalid value is.

The [explicit list of entries](legends.md#choosing-the-entries) of a legend is not shown at all, because which series a key explains, and in what order, is part of how the author describes the figure. The legend is nevertheless drawn and clicked in the viewer exactly as the list says.

A property that the figure merely constrains is not read-only. Limits that must increase and a font size that must be positive stay editable, and a value the figure will not accept is refused with its reason, which is the more useful answer.

A choice a combo box cannot honour is shown in the same spirit: it is listed, drawn greyed, and cannot be picked, and hovering over it gives the reason and says where the same choice does work. Removing it would hide from you that the figure has the value at all, whereas a greyed entry invites you to find the property that makes it available.

A colour may be **colormapped**, which colours a plot from its data through the [colour scale](colour.md) the plot names, and that works only where the figure holds a value to look the colour up by and the plot names a scale to look it up in: the isolines of a contour, coloured by their level; the faces and edges of a surface, coloured by its field or its colour data; and the line and markers of a series that has colour data. The choice is greyed on such a plot when it names no colour scale of the figure. A quiver and a series without colour data hold no value to look up and would be drawn in the middle colour of viridis, so the choice is greyed for them. The marker face and edge of a series coloured per segment are greyed too, because no value belongs to a point.

The **clamp** choice of an out-of-range policy of a colour scale, which draws a value that falls outside the range of the scale in its nearest end colour, is greyed for the **non_finite** category of the scale, because a value that is not finite has no nearest end of the scale to be clamped to; it stays available for the **below**, **above** and **outside** categories. What each choice draws is described under [values out of range](colour.md#values-out-of-range).

## Linked axes

When axes are linked along a dimension (see [linking axes](getting-started.md#linking-axes)), panning, zooming, box zooming or double-clicking one of them changes the limits of every axes in its group along that dimension. Linked axes at a fixed aspect keep it as they move together, and drawing the limits a gesture leaves draws them unchanged, so a linked family neither creeps nor oscillates from one gesture to the next. Axes that are not linked along that dimension are unaffected. For example, when the x limits of two axes are linked and their y limits are not, panning one of them moves both horizontally but only itself vertically.

Links apply to limits only. The azimuth, elevation, magnification and position of a three-dimensional view belong to its own axes, and rotating or zooming one three-dimensional axes does not change another.

## Saving the figure

**Save figure…**, in the toolbar, opens a save dialog and writes the figure as it is currently shown, with its current limits, three-dimensional views and plot visibility. The format follows the extension of the name given: `.fig` writes the default Protocol Buffers format and `.json` (including `.fig.json`) writes JSON, as described in [saving and loading](getting-started.md#saving-and-loading). A name with any other extension is refused and no file is written.

The figure written is the figure the viewer now holds: the changes saved become part of it, the undo history is emptied, and Refit restores the view as saved rather than the view the file was opened with. A notification in the bottom-right corner of the window reports whether the save succeeded. In a web page the same button downloads the file instead; see [in the browser](#in-the-browser).

## Exporting to PDF

**Export PDF…**, in the toolbar, opens a save dialog and writes the figure as it is currently shown: with its current limits, current three-dimensional views, and without the plots hidden from the legend. The exported page has the same properties as one written by the API, which are described in [exporting PDF](getting-started.md#exporting-pdf), and it is written with the default settings for [dense surfaces](getting-started.md#dense-surfaces). A notification in the bottom-right corner of the window reports whether the export succeeded. In a web page the same button downloads the file instead; see [in the browser](#in-the-browser).

## Problems

When something is wrong with a figure, the toolbar shows a problems indicator: the number of problems, for example "2 problems", written in the warning colour. It appears only when there is something to report. Clicking it opens a list of the problems and clicking away closes the list again.

Each entry names the object the problem concerns, as the [object tree](#selecting-an-object) names it, and the property where one is concerned. It then gives the reason in full and says how the problem arose, in one of three ways.

- **Reported while the figure was drawn.** Drawing revealed something that does not prevent the figure from being drawn, such as a LaTeX expression the typesetter does not support, data that cannot be placed on a logarithmic axis, or an artist whose data gives it nothing to draw (a series of no points, an image with no rows or no columns, or a field with no cell between its nodes). Unsupported mathematics is drawn as its raw source and an artist that cannot be drawn is left out, so the figure still appears. These are the problems that `Figure::validate` reports as warnings, and that the report returned by `Figure::export_pdf` lists, as described in [validating a figure](getting-started.md#validating-a-figure).
- **Your change could not be shown, so it was discarded.** A change you made can no longer be applied to the figure, so it was thrown away rather than silently ignored.
- **Your change was refused, so the figure is unchanged.** A change made in the property editor was checked before it was recorded and the figure would not accept it, so the figure keeps the value it had and the history keeps no empty step. Switching an axis that holds an image to a logarithmic scale is refused in this way, because an image cannot be placed on a logarithmic axis, and the reason names the image and says how to draw its values as a surface instead, as [images on logarithmic axes](getting-started.md#images-on-logarithmic-axes) describes.

The last two are reported until the change they concern is superseded, undone, reverted or discarded, after which the indicator falls silent again. Problems reported while the figure was drawn come back whenever the figure is drawn, so they persist until the figure itself changes.

## In the browser

A figure embedded in a web page with the `<ironlab-figure>` element is drawn by the same engine as this viewer and behaves in the same way: the same tools, gestures, datatips, legend clicks, double-click, Refit, undo and redo, and the same **Save figure…** and **Export PDF…**, each of which downloads the file the viewer would have written. The problems indicator is there too. The differences are these.

- The toolbar is a figurebar drawn in HTML above the figure, which a page may hide, and there is no [figure browser](#the-figure-browser) and no [property editor](#the-property-editor): a page holds one figure per element, and a reader changes the views of a figure and the visibility of its plots but not the properties its author set.
- Scrolling the mouse wheel zooms a figure only after a click or a touch has activated it, so that the wheel scrolls the page until the reader asks for the figure; **Escape**, focus leaving the figure or a click outside it hands the wheel back to the page. Dragging and pinching need no activation.
- **R**, **⌘Z** and **⌘⇧Z** (**Ctrl+Z** and **Ctrl+Shift+Z** away from macOS) work while the figure or one of its buttons has keyboard focus.
- **Save figure…** writes the encoding of the page's figure file, `.fig` or `.json`, under the name the page gives the figure, rather than offering a choice of name in a dialog, and the reader's changes stay undoable after the download because they are never folded into the file the page serves.
- **Export PDF…** performs the renders the exporter needs on the reader's own graphics device, and the export report's warnings are counted in a status line beneath the figure rather than in a notification.

How to put a figure in a page, and everything a page author controls, is described in [embedding figures in a web page](embedding.md).
