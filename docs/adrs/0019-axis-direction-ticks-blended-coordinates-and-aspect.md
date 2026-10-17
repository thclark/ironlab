# ADR 0019: Axis direction, ticks, blended coordinates and aspect

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0008](0008-typed-edits-and-a-view-overlay.md), [ADR 0012](0012-empty-and-singleton-data.md), [ADR 0018](0018-legend-panels-with-entries-merged-by-name-and-look.md), which this record supersedes in part

## Context

The figures of yFoil, an aerofoil analysis tool, moved to IronLAB 0.12.0 and exposed four gaps in how an axes maps data onto the page. They are recorded in issues [#76](https://github.com/thclark/ironlab/issues/76), [#77](https://github.com/thclark/ironlab/issues/77), [#78](https://github.com/thclark/ironlab/issues/78), [#79](https://github.com/thclark/ironlab/issues/79) and [#80](https://github.com/thclark/ironlab/issues/80), and each gap is common to scientific plotting well beyond aerodynamics.

1. **Direction.** A pressure distribution plots the pressure coefficient with suction, which is negative, upwards, so its y axis runs from positive values at the bottom to negative values at the top. Depth in geoscience and stellar magnitude in astronomy are drawn the same way. Manual limits whose minimum exceeds their maximum are the error `InvalidLimits`, and the axis had no property for its direction, so yFoil plotted the negated quantity, and its datatips reported a value that nobody reads.
2. **Ticks.** The only ticks were those of the automatic locator. A family of panels that should share one regular grid (x from −0.2 to 1.2 in steps of 0.2) could not be given it; a categorical strip chart, which names one row per test case, could not name its rows; a stack of rows sharing one x axis could not drop the x tick labels of the upper rows; and a short row with manual limits from −0.15 to 0.46 received a single tick label, 0.0, from which no value can be read.
3. **Blended coordinates.** A reference line that spans the axes, such as a vertical line at x = 0.5 from the bottom of the plot to its top whatever the y limits, has a data coordinate along one axis and a coordinate relative to the plot area along the other. A line or a scatter could only be placed in data coordinates, so yFoil drew such lines between computed limits.
4. **Aspect.** Where the data is geometry, such as an aerofoil section, one data unit must have the same length along x and y, or the figure misrepresents the shape. IronLAB had no aspect property, so yFoil sized its figures from plot areas measured on rendered output, a calculation that fails whenever the margins change and that a reader undoes with the first zoom in the viewer.

The four gaps are decided together because each changes how an axis maps data onto the page, and because the viewer's gestures, the layout and export must treat all four consistently.

## Decision

### An axis is reversed by a flag, and its limits keep their order

`Axis` gains `reverse: bool`. A reversed axis runs from its upper limit at its start (the left of a horizontal axis, the bottom of a vertical one) to its lower limit at its end. The limits keep their meaning, with `min < max`: manual limits that run downwards remain the error `InvalidLimits`, and automatic limits are computed exactly as for the axis unreversed. Direction is therefore a property of the drawing only, never of the range of data shown.

A reversed axis behaves everywhere as a normal one does. In a two-dimensional axes the scene compiler swaps the two ends of the axis map that it publishes in the hit map, so that the map's `start` is the coordinate of the upper limit. Everything that is drawn or inverted through that map follows without further change: artists, images, ticks, tick labels, grid lines, the `best` location of a legend, datatips and pixel picking, and the pan, wheel zoom and rubber-band zoom of the native viewer and of the embedded figure, which compute new limits from the map captured at the start of the gesture and always write them lower bound first. In a three-dimensional axes the compiler mirrors the normalised box along each reversed axis, so that every artist, tick, grid line and tick label placed through the box is mirrored alike, and the choice of the box edges that carry tick labels, which is made in the normalised box, is unaffected. A logarithmic axis is reversed in the same way. Export draws the same display list and needs nothing of its own.

Axis links share limits only. A reversed axis may therefore be linked with an unreversed one, and each keeps its own direction. This is a deliberate choice rather than an oversight: a link states that two axes show the same range of the same quantity, which is independent of the direction in which either draws it, and a pressure plot linked to an unreversed plot of the same quantity is a legitimate figure. Setting the direction of one member of a link group leaves the others as they are.

`reverse` is an ordinary property of the axis, so the property editor of both hosts offers it beside the scale and the limits.

### Ticks and their labels are properties of each axis

`Axis` gains `ticks: Ticks` and `tick_labels: TickLabels`.

`Ticks` is `Auto`, the existing locator; `Step { step, origin }`, a tick at every value `origin + k × step`, for every integer `k`, that lies within the limits; or `Values(Vec<f64>)`, a tick at each of the given values that lies within the limits. `TickLabels` is `Auto`, the formatted values; `Hidden`, ticks drawn without labels; or `Text(Vec<Text>)`, one label for each explicit tick value, in the order of the values. Ticks never change the limits: automatic limits are computed as for automatic ticks, and a tick outside the limits is not drawn.

Validation reports the error `InvalidTicks` for a step that is not a finite positive number, an origin that is not finite, an explicit tick value that is not finite, and an explicit tick value that is not positive on a logarithmic axis, and the error `InvalidTickLabels` for explicit labels whose ticks are not explicit values, or whose number differs from the number of values. These are errors, as invalid manual limits are, because the author asked for a placement that cannot be drawn, whereas [ADR 0012](0012-empty-and-singleton-data.md) reserves warnings for data that is valid but draws less than it holds. An empty list of values is valid and draws an axis without ticks. Only the axes that are drawn are checked: x and y, and z in a three-dimensional axes. The scene compiler never fails on a figure that validation would refuse: a step so fine that it would place more than 500 ticks falls back to automatic ticks with a warning, and explicit labels that do not name their values one by one are replaced by automatic labels.

Automatic labels of a step or of explicit values show every value with the fewest decimals that show all of them, with the common exponent of automatic labels, and as decades on a logarithmic axis, so that every axis of a figure uses one notation. A step on a logarithmic axis is a step in data units.

Hidden labels reserve no space, so the plot area of an axes whose neighbour already names the ticks grows into the space the labels would have taken, and an axis label moves in beside the plot.

There is no categorical axis type. A categorical strip chart is a scatter with `ticks: Values([0, 1, …, N − 1])`, `tick_labels: Text(names)` and `reverse: true` on y, so that the first item is at the top; a panel beside it that shows the same rows sets `tick_labels: Hidden`. The pieces are general, each is useful alone, and a categorical type would duplicate them.

#### The automatic locator guarantees a minimum number of ticks

The automatic locator chooses a target number of intervals from the length of the axis. On a short axis with manual limits narrower than the round step of that target, few or no ticks fell within the limits. The locator now raises the target until at least two ticks lie within the limits on the x axis of a two-dimensional axes, and at least three on every other axis, whenever the limits hold that many at a round step. The x axis is allowed fewer because its labels must also fit along it; two ticks are the fewest from which a value can be read. The rule needs no property, so every figure benefits from it without change.

#### Plot areas, not cells, share the page by weight

A panel that names its rows and a panel beside it that hides its labels must have plot areas of the same physical width, so that a reader can compare positions across them, and the margin of the first must be wide enough for its longest name. [ADR 0018](0018-legend-panels-with-entries-merged-by-name-and-look.md) divided the page into cells in proportion to the weights of the rows and columns, and inset each plot area by its aligned margins, so a column with wide tick labels had a narrower plot area than its neighbour of equal weight.

This record supersedes that division. The aligned margins of every row and column are taken out of the page first, and the plot areas then share what remains in proportion to their weights, so that plot areas of equal weight are of equal size whatever their tick labels, axis labels and titles. A cell block spanning several rows or columns takes their shares together with the margins between them. A legend panel in a row or column without axes counts as a plot area of its weight, inset by the page padding. When the margins would leave the plot areas less than a fifth of the page, every margin is scaled down by the same factor. Equal weights still divide the page exactly as no weights do.

The same rule gives a stack of rows that shows x tick labels only on its bottom row plot areas of equal height, as [#77](https://github.com/thclark/ironlab/issues/77) asks.

### Blended coordinates place a coordinate relative to the plot area

`Line` and `Scatter` gain `x_coords` and `y_coords`, each a `Coords` with the variants `Data`, the default and the behaviour until now, and `AxesFraction`. A value in axes-fraction coordinates is a fraction of the plot area along that dimension: 0 is the left edge for x and the bottom edge for y, and 1 the right or top edge, whatever the limits, the scale, the direction or the pan and zoom of the axis. The fraction is measured on the physical plot area rather than through the axis map, so that on a reversed axis a value in data coordinates is mapped reversed while a fraction of 0 is still the left or bottom edge. Values outside `[0, 1]` lie beyond the plot area. Blended coordinates apply in two-dimensional axes only: a three-dimensional axes has no plot area against which a fraction could be measured, so a line or scatter of a three-dimensional axes with a dimension in axes-fraction coordinates is the validation error `AxesFractionInThreeDAxes`.

The scene compiler places each line and scatter through its own mapping: the axis map of the axes along a dimension in data coordinates, and a map of `[0, 1]` onto the physical extent of the plot area along a dimension in axes-fraction coordinates. Both viewers recompile the scene after every pan, zoom and change of limits, so a blended artist is placed afresh each time and keeps to the edge of the axes.

A dimension in axes-fraction coordinates takes no part in automatic limits, because its values do not lie on the axis, and a fraction of zero or less is not reported against a logarithmic axis. The dimension in data coordinates of the same artist takes part as any data does. A horizontal reference line is therefore a line whose x values are the fractions 0 and 1 and whose y values are its data value twice: it never widens the x limits, and its y value widens the y limits, as matplotlib's `axhline` does, so that a tolerance above the data is in view without fixed limits. Axes linked along y share that widening, as they share every data extent.

`Line` and `Scatter` also gain `clip: bool`, `true` by default. Setting it to `false` releases clipping only along a dimension in axes-fraction coordinates, where the clip of the artist extends to the page, so that a marker can sit on an edge of the axes or beyond it and be drawn whole. Along a dimension in data coordinates the artist stays clipped to the plot area exactly as with `true`, so that a value outside the limits of that axis is never drawn: a tick at a data x and at a fraction of the height may overhang the top edge, but is never drawn beside the plot area or over a neighbouring axes, however the reader pans. A released clip therefore has no effect on an artist in data coordinates along both dimensions, nor in a three-dimensional axes, which clips every artist to its tile; validation reports such a release with the warning `IneffectiveClip`, a warning rather than an error because the artist is still drawn. The compiler draws the artists of an axes in groups that share one clip, in paint order, and thins a dense series against the clip of its own artist. Both viewers and export draw these groups, so the rule holds on screen and on the page alike.

`MarkerShape` gains `VLine` and `HLine`, a vertical and a horizontal tick, each one stroke as long as the marker size. The facade offers `hline(y)` and `vline(x)` on the axes handle, which build such reference lines as ordinary lines, dark grey and outside the colour order, rather than as a new kind of artist, together with the setters `x_coords`, `y_coords` and `clip` on the handles of lines and scatters.

The coordinates decide what the values of an artist mean, as its data does, so they are set by the program that builds the figure. The edit protocol nevertheless names every settable field of the figure, so `x_coords` and `y_coords` are properties of the edit model with the value type `Coords`; the property editor of both hosts shows them read-only, with the reason, and offers `clip` as an ordinary property. A datatip reports the data coordinates of the point under the pointer only: the `x` and `y` of a datatip become optional, and a coordinate in axes-fraction coordinates is absent, because it says where a mark was drawn rather than what was measured.

Clamping data that lies outside the limits to the edge of the plot area, and flagging it there, was considered and rejected. Manual limits already decide what is shown, and a clamped point is drawn where its value is not, which a reader cannot distinguish from a measured value at the edge.

#### Monospace text is set with `\texttt` outside mathematics

Names of test cases and other code identifiers are read character by character, and the sheets that compare cases set them in a monospace face. IronLAB bundles no monospace font. latex-rust supports `\texttt` and `\mathtt` only inside mathematics, where letters and digits are set in the Mathematical Monospace alphabet of STIX Two Math and the rules of mathematics apply: spaces are dropped and a bare `_` starts a subscript. The text engine therefore recognises `\texttt{…}` in the plain text of a label interpreted as LaTeX, and sets its argument itself: letters and digits in the same Mathematical Monospace alphabet of the math face that latex-rust embeds, every other character in that face's own glyph, each centred in a cell of one fixed width, with the extents of plain text, and with the characters as written as the text that is copied from a PDF. Within the argument a bare underscore is a literal underscore, as `\_` is, so that an identifier can be pasted unchanged; LaTeX itself rejects a bare underscore outside mathematics, and the departure is deliberate. Because the segment is resolved by the text engine, it applies to every text of a figure, in every renderer and in export. No font is added and nothing is required of latex-rust.

### The aspect of an axes is kept everywhere

`Axes` gains `aspect: Aspect`, which is `Auto`, the behaviour until now; `Equal`, one data unit of the same length along x and y; or `Ratio(f64)`, a fixed ratio of the length of a unit of y to that of a unit of x (MATLAB's `daspect`). It also gains `aspect_adjust: AspectAdjust`, which is `Box`, the plot area shrinks along one dimension to give the ratio, or `Limits`, the limits of one dimension widen to give it.

The aspect is kept by the layout, which applies it after the margins are known, by both viewers while panning and zooming, where a rubber-band zoom expands the shorter side of the band to the ratio, by export, and across linked axes and rows and columns of unequal weight.

An inset axes, which [#80](https://github.com/thclark/ironlab/issues/80) sketches for drawing the aerofoil section under its pressure distribution at equal aspect, was considered and is out of the scope of this decision.

## Alternatives considered

### A direction enumeration, or limits written in reverse

A `Direction` enumeration with the values `Normal` and `Reverse` was considered. It was rejected because an axis has exactly two directions, so a boolean says the same more compactly and no further value is foreseen. Accepting manual limits whose minimum exceeds their maximum as a reversed axis, as matplotlib's `set_ylim` does, was also rejected: the order of the bounds would then carry two meanings, and limits inverted by a faulty computation would silently flip the axis instead of being reported.

### A categorical axis type

A categorical axis, holding the names of its categories and placing data by name, was considered. It was rejected because explicit ticks, explicit labels and a reversed axis express the same figure, are each useful on their own, and leave the data numeric, so that the rows of a strip chart can still be panned, zoomed and linked like any other axis. Reporting the name of a category in a datatip is a separate question, recorded in [issue #2](https://github.com/thclark/ironlab/issues/2).

### A minimum tick count, and shared tick labels, as properties

A property for the minimum number of ticks of an axis was considered. It was rejected because the fixed rule above repairs every short axis without the author having to notice the problem. A figure-level property that shows tick labels only on the outer axes of a grid was also considered. It was deferred, because hiding the labels of each inner axis is explicit, already possible, and keeps every decision on the axis it concerns.

### Weights for cells

Keeping the division of [ADR 0018](0018-legend-panels-with-entries-merged-by-name-and-look.md), in which cells share the page by weight, and equalising plot areas only when asked was considered. It was rejected because a weight is read as the relative size of a plot, and plot areas of equal weight but unequal size are a defect a reader sees, not a choice an author makes.

## Consequences

- `Axis` gains `reverse`, `ticks` and `tick_labels`. In JSON the three are required, so figures written before this change must be regenerated, which is acceptable because the project has no users who depend on the earlier format. In Protocol Buffers they are new fields, and a file written before this change decodes with an unreversed axis and automatic ticks and labels.
- `Line` and `Scatter` gain `x_coords`, `y_coords` and `clip`, required in JSON as the properties of `Axis` are, and `MarkerShape` gains `VLine` and `HLine`. Validation gains the error `AxesFractionInThreeDAxes` and the warning `IneffectiveClip`, the edit model gains the value type `Coords`, and the `x` and `y` of a datatip become optional.
- The edit model gains the value types `Ticks`, `TickLabels` and `Texts`. The property editor offers `reverse`, the kind of ticks and the kind of tick labels; the explicit texts of tick labels are read-only there, because they name the tick values one by one and both are set by the program that builds the figure.
- Every figure of more than one row or column whose margins differ between rows or columns is drawn with slightly different plot areas, because the plot areas, and no longer the cells, share the page by weight. A figure of one tile, or of rows and columns with equal margins, is drawn as before.
- A short axis with manual limits may gain ticks it did not have, because the automatic locator now guarantees a minimum number of ticks.
- The rules are stated in the documentation of the scene compiler and of the IR types; the encoding is described in the [figure schema reference](../reference/figure-schema.md#axis), and the use in [getting started](../guides/getting-started.md#axis-direction-and-ticks).
