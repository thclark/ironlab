# ADR 0024: Colour scales as figure nodes, colourbars and colour keys

**Status:** Accepted

**Related:** [ADR 0008](0008-typed-edits-and-a-view-overlay.md), [ADR 0011](0011-image-artists.md), [ADR 0018](0018-legend-panels-with-entries-merged-by-name-and-look.md) and [ADR 0020](0020-explicit-legend-entries-and-fitted-tracks.md), each of which this record supersedes in part, [ADR 0019](0019-axis-direction-ticks-blended-coordinates-and-aspect.md), [ADR 0021](0021-images-refused-on-logarithmic-axes-and-alpha-and-opacity.md) and [ADR 0022](0022-figure-nodes-in-one-identifier-space-and-panels.md)

## Context

Before this decision, each axes held one colormap and one pair of colour limits, and every colour-mapped artist of the axes was drawn through them. Nothing in the figure recorded which artist used which scale, so a figure could not show a colourbar ([issue #8](https://github.com/thclark/ironlab/issues/8)), could not share one bar across a row of axes ([issue #88](https://github.com/thclark/ironlab/issues/88)), and could not give an artist a scale of its own or show a classification as a key ([issue #24](https://github.com/thclark/ironlab/issues/24)).

[ADR 0011](0011-image-artists.md) listed six stopgaps around that architecture. Out-of-range policies existed only on the two colour-mapped image kinds; the classification of a value as below, inside or above the range was done in the image code; a colormap had exactly 256 entries, which also fixed the domain of a colour-indexed image ([issue #40](https://github.com/thclark/ironlab/issues/40)); an image could not have a colormap of its own; and legend samples of colour-mapped artists used the middle colour of the colormap.

An earlier attempt (pull request #31) placed a colour scale on the axes and let artists override it. It was abandoned because the inheritance rule, deciding which scale an artist uses when neither it nor its axes says, leaked into every consumer of the figure.

## Decision

### A colour scale is a figure node that each artist names

A **colour scale**, `ColorScale`, is a node of the figure, held in `Figure.color_scales` in the single identifier space of [ADR 0022](0022-figure-nodes-in-one-identifier-space-and-panels.md). It turns a number into a colour, and it has an optional `title`, a `kind` and a policy for non-finite values. Every artist that maps values to colours (contours, surfaces, scatters and scalar images) names its scale explicitly with `color_scale`. The axes holds no colormap and no colour limits, and there is no inheritance: an artist whose style maps colours and which names no scale is the validation error `MissingColorScale`, and a reference that names no node or a node that is not a scale is `UnknownColorScale` or `WrongNodeKind`.

A scale's automatic limits range over the values of every artist that names it, in every axes, hidden artists included. One scale named by artists in several axes therefore gives those axes the same colours, which is how a row of subplots shares a colourbar.

Any artist accepts any kind of scale. A restriction of particular artists to particular kinds would make the interface harder to learn than the cases it excludes are worth.

### Four kinds, each fixing both the mapping and the drawing

A scale's kind fixes both how values become colours and how the scale is drawn, so that a scale cannot be drawn in a way that misrepresents its mapping.

| Kind | Mapping | Drawn as |
| --- | --- | --- |
| Continuous | Interpolates a colormap between its limits, linearly or logarithmically | A smooth bar |
| Discrete | Gives every value the single colour of the band between two edges in which it falls | A bar of bands |
| Indexed | Gives a whole number i the i-th colour of a palette | A bar of adjoining blocks, labelled by ticks |
| Categorical | Gives a whole number i the i-th category, each a colour with a required label | Separate outlined squares, each with its label |

A colormap is either one of the named colormaps, optionally reversed, or a listed colormap of 2 to 1024 colours, which is interpolated. A discrete scale with k bands gives band i the colour at i/(k − 1) of its colormap, so both ends of the colormap are used, and it bands every artist, a smooth surface included, so that its bar is truthful. A value on an inner band edge belongs to the band above it. The band edges are either explicit or chosen automatically within the scale's limits.

An indexed and a categorical scale map a whole number from 0 to the number of entries less one to that entry. The domain is the number of entries, which resolves issue #40. An indexed scale is the familiar palette of an indexed image, of which typically few entries are labelled, so its labels are ticks. A categorical scale is a classification whose every entry needs a label, so the type requires one.

### Out-of-range values are handled by the scale for every artist

Each category of value a scale cannot colour has a `ColorScaleOutOfRange`, which holds an `OutOfRange` policy (`strict`, `transparent`, `clamp` or a fixed colour), whether to draw a key for it, and an optional label for that key. A continuous or discrete scale has `below` and `above`, which default to `clamp`. An indexed or categorical scale has `outside`, which covers every finite value that is not a whole number in its domain, a fractional value included, and which defaults to `strict`, so that a value that names no entry is the validation error `ValueOutOfRange`. Every kind has `non_finite`, which defaults to `transparent`, because a missing value is how an author says that nothing is drawn there. `clamp` on an indexed or categorical scale truncates toward zero and then clamps to the ends.

Validation checks a strict category against the data an artist holds, never against values derived from it. Through an indexed or categorical scale a surface face therefore takes the value of its first corner rather than the mean of its four corners, since the mean of indices is not an index, and a contour with no levels of its own places its levels halfway between entries, so that each band holds exactly one entry.

The classification of a value as below, inside, above, outside or non-finite is one function of the scale, used by validation and by the scene compiler alike.

This supersedes ADR 0011's lenient default for index-mapped colour, the 256-entry domain of a colour-indexed image, and its scale per axes. It resolves ADR 0011's first four colormap stopgaps; the GPU lookup table of the fifth remains open.

### One image artist for values

`ScalarImage` replaces `IndexedImage` and `MappedImage`. A pixel holds one number, 64-bit floating-point or 8-bit, and the image names a scale like any other artist; whether its values are indices or measurements is a property of the scale, not of the image. `Image` remains the artist whose pixels hold colours. A contour's levels become optional, and a contour without levels takes the band edges of a discrete scale, so that a filled contour and its bar cannot disagree, and ten automatic levels otherwise.

### A scale is shown by a colourbar or a legend item

A scale holds everything needed to draw it: its colours, limits, band edges, ticks and tick labels. A colourbar holds only which scale it shows and where, so every bar and legend item that shows one scale agrees with it and with the artists. A scale may be shown in three places:

- **In a panel.** `PanelContent::Colorbar` shows a scale in a panel of the tile layout. The bar and its keys run along the cell span, inset by the aligned margins of its rows or columns, and are measured against the span of the plots beside them. Where a fixed aspect ratio shrinks a plot box within its area, that span instead runs from the near edge of the first drawn plot box to the far edge of the last among the axes in the rows or columns the panel spans; a panel whose rows or columns hold no axes keeps the whole span. The bar's `length` is a fraction of the span, or automatic: the whole span for a vertical bar, so that a bar beside a stack of plots is as tall as they are, and half of it for a horizontal bar, because a bar as wide as a row of plots outweighs them. The panel's alignment places a bar shorter than its span along the span as well as across its track.
- **Inside an axes.** An axes' `colorbar` draws a bar along one edge of its plot area, over the artists, on a framed box like that of an inside legend so that its labels can be read over any colour. The box, keys included, is the stated fraction of that edge long and centred on it.
- **In a legend.** `LegendItemContent::ColorScale` shows a scale as an item of a legend, beside the artist and custom items of [ADR 0020](0020-explicit-legend-entries-and-fitted-tracks.md). A categorical scale expands to one entry per category, labelled with the category's label and drawn with the category's colour as a swatch, followed by its out-of-range keys; these entries are ordered and wrapped like any other entry. In a legend panel a category entry merges with an entry of the same label and look, by the rule of [ADR 0018](0018-legend-panels-with-entries-merged-by-name-and-look.md), so that a hand-written swatch and a category of one colour and label are listed once; in a legend inside a plot area it merges nothing, as ADR 0020 requires of every item there. A continuous, discrete or indexed scale is drawn as a vertical bar in one column, over the number of entry rows the item asks for, five by default, centred on the column of samples so that it lines up with the samples of the entries above and below it; on a full row it lies horizontally across the legend. A bar item is drawn by the same code as a colourbar, so the two cannot disagree.

A bar's ends are always flat. A key for an out-of-range category is drawn only when the author asks for it, as a separate square beyond the bar and never as a cap at its end, because a colour at the end of a bar cannot be told apart from the last colour in range. Along a vertical bar the above key lies beyond the top, and the below key and then the non-finite key beyond the bottom; along a horizontal bar the non-finite and then the below key lie beyond the left end and the above key beyond the right. The outside key of an indexed scale takes the place of the below key. A square is filled with the colour that is painted, or left empty under `transparent` and `strict`, and is labelled `> upper`, `< lower`, `NaN / Inf` or `Out of range` unless the author gives a label. For a categorical scale the keys are further entries after the categories. Beside a vertical bar item of a legend each key is an entry row of its own, a swatch beside its label, directly above the bar's rows (the above key) or below them (the below or outside key, then the non-finite key), so that the keys keep step with the rows of the legend; these rows belong to the bar, so they are never merged with other entries.[^bar-item-keys]

Every swatch in a legend, whether a custom `Swatch` sample, a category entry or an out-of-range key, is drawn as one look: a square of the colour with a thin black outline, which supersedes the filled rectangle of ADR 0020. The legend sample of a surface drawn in one fixed colour is the same square, outlined in the surface's edge colour.

This supersedes ADR 0018's deferral of colourbars, ADR 0020's list of the two kinds of legend item content, and ADR 0011's stopgap of legend samples in the middle colour of the colormap: a colour-mapped artist's legend sample is a sweep of its whole scale from its low end at the left to its high end at the right, drawn as a gradient, as bands of equal width for a discrete scale, or as the entry colours side by side for an indexed or categorical scale. A sweep takes the artist's opacity but not its alpha, following [ADR 0021](0021-images-refused-on-logarithmic-axes-and-alpha-and-opacity.md).

### Where a bar's title lies, and how a bar is sized

The title of a scale is drawn beside every bar of the scale where the bar's `title_placement` says. `Along`, the default, lies beyond the tick labels on their side of the bar and centred on it, separated from the labels by two thirds of the gap that separates a heading from its bar, because the line boxes of the labels and of the title each carry white space of their own, turned a quarter turn to read from bottom to top beside a vertical bar, as the label of a y axis is, and upright below the labels of a horizontal bar, as the label of an x axis is. `Above` is an upright heading: above a vertical panel bar, in the margin above the first row the panel spans, and above a horizontal panel bar. Inside an axes, whose tick labels face the interior of the plot area, a title along the bar lies beyond them toward the interior, and a heading lies on the side of a horizontal bar away from them. A bar item of a legend is always titled along its bar, so that its title adds no row. A title along a bar takes room beside the bar rather than above it, so a fitted track grows only by the title's line, and a vertical bar titled along its length leaves the plots where they would be without a title. The scale's title remains a title in the sense of [ADR 0023](0023-titles-labels-names-and-tags.md), the text that names the scale, wherever it is drawn; the field that chooses where is named `title_placement` because it holds no text.

Every bar is three quarters of the base font size thick, so that it does not outweigh the labels beside it.[^layout-review]

A panel carries an optional `title` and a `title_align`, which head its content whatever the content is, because a key of either kind needs a heading and the panel, not its content, is what the reader sees as one block. In a legend panel the title lies above the entries, inside the border of a boxed panel; above a vertical colourbar that spans plots it lies in the margin above them, as an axes title does. The title is aligned with the left edge, the centre or the right edge of the content, and a panel built by the facade aligns it with the left edge. A legend has no title of its own. As ADR 0023 requires, the inspector names a titled panel by its title.

### A bar is the place to change its scale in the viewer

The viewer reads a bar from the hit map that the scene compiler publishes, which records each bar's value mapping, band edges and key labels as drawn, so the viewer derives nothing from the scale itself. Hovering a bar reports the value under the pointer on a continuous bar, the band on a discrete bar, the entry on an indexed bar, the category on a categorical scale and the label of a key. Dragging along a continuous bar, or a discrete bar with automatic levels, pans the scale's limits, and scrolling zooms them about the value under the pointer, in logarithmic space under a logarithmic norm. Each gesture is one `Set` of the scale's `limits` or `levels.limits`, made through the same view overlay as a gesture on an axes, so that every bar and artist that uses the scale changes together and a gesture can be undone. A bar of any other kind, a key and a bar item in a legend take no gesture, because their limits are not a range to move. Double-clicking a bar restores its scale's automatic limits, and resetting the view restores those of every scale.

The colours of an indexed scale's entries, the colour and label of each category, and the colours of a listed colormap are edited one entry at a time, through paths that end in the entry's index written as a decimal number, such as `kind.indexed.colors.3`. The lists themselves cannot be edited, because the number of entries comes from the program that wrote the figure. Because the number of entries differs between scales, these paths are not part of the registry of a kind; `properties_of` lists them for one node, and a path naming an entry beyond the end is refused with `EntryOutOfRange`.

This supersedes in part [ADR 0008](0008-typed-edits-and-a-view-overlay.md): a property path may end in a decimal index into a palette as well as in field names, the paths of palette entries come from the node rather than from the registry, and the limits of a colour scale are view entries beside the limits and three-dimensional view of an axes.

## Alternatives considered

### A scale on the axes that artists may override

This was pull request #31. It was rejected because the rule deciding which scale an artist uses had to be applied identically by the scene compiler, the viewer, the exporters and every program that reads a figure.

### Restricting each artist to particular kinds of scale

Keeping a colour-indexed and a colour-mapped image, each accepting only its own kind of scale, was rejected because the restriction would need to be learnt for each artist, and because the distinction it draws belongs to the scale.

### An indexed scale with a switch between a bar and squares

A single index-mapped kind with a display switch was rejected because each mode needs fields that the other does not use: ticks and tick labels for a bar of a palette, and a required label per entry for a key of categories.

### A title for a legend rather than for a panel

A title held by the legend content would have left a colourbar panel without a heading, or required a second field of the same meaning on the colourbar. Holding it on the panel gives both kinds of content one heading that survives a change of content.

### Out-of-range colours at the ends of the bar

Drawing the colours of out-of-range values as caps at the ends of the bar, as other plotting software does, was rejected because it makes those values look like the extreme values in range.

## Consequences

- The facade creates scales with `fig.color_scale()`, `fig.indexed_scale(..)` and `fig.categorical_scale(..)`, and an axes' default scale is the scale of its first colour-mapped artist, a rule that depends only on the figure. `ax.colormap(..)`, `ax.clim(..)`, `indexed_image` and `mapped_image` are removed.
- A figure whose indices are fractional or outside the palette, previously drawn by truncation, is now refused unless its scale chooses another policy.
- A scale that no artist names is valid and raises no warning, because a custom colourbar needs one.
- The schema changes incompatibly: the axes loses its colormap and colour limits, the two index and value image kinds become one, and colour-mapped artists gain a scale reference. A colour-scale legend item holds its scale and its rows as an object, and a panel and each colourbar gain the fields of their layout, so that a bar of a stated length or a titled panel is described by the figure rather than by the program that drew it.
- A length of a panel bar that is not a fraction of its span is the error `InvalidColorbarLength`, as for an inside bar, and a bar item of no rows is the error `InvalidLegendRows`.

[^bar-item-keys]: The first implementation drew the keys of a bar item as squares beyond the ends of its bar, as on a colourbar, and let them add to the height of the item. In a legend whose rows are evenly spaced, those squares fell between rows and out of step with the entries around them, so each key became an entry row of its own.

[^layout-review]: The first implementation drew every bar as thick as the base font size, every horizontal panel bar the whole span of its plots, and every title as a heading above its bar, adding to the height of a bar item and to the top margin above a vertical panel bar. A review of the gallery found the bars heavy beside their labels, a bar as wide as a row of plots overbearing, and the headings pushing the plots down, which led to the thickness, the automatic lengths and the default placement recorded here. A second review found a title along a bar too far from its tick labels when it lay a full heading gap beyond them, and a bar item that started at the left edge of its column of samples out of line with the centred marker samples above it, which led to the narrower gap and the centred bar item.
