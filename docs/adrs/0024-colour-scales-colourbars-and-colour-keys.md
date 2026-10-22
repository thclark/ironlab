# ADR 0024: Colour scales as figure nodes, colourbars and colour keys

**Status:** Accepted

**Related:** [ADR 0011](0011-image-artists.md) and [ADR 0018](0018-legend-panels-with-entries-merged-by-name-and-look.md), each of which this record supersedes in part, [ADR 0019](0019-axis-direction-ticks-blended-coordinates-and-aspect.md), [ADR 0020](0020-explicit-legend-entries-and-fitted-tracks.md), [ADR 0021](0021-images-refused-on-logarithmic-axes-and-alpha-and-opacity.md) and [ADR 0022](0022-figure-nodes-in-one-identifier-space-and-panels.md)

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

- **In a panel.** `PanelContent::Colorbar` shows a scale in a panel of the tile layout. The bar and its keys stretch along the cell span, inset by the aligned margins of its rows or columns, so that a bar spanning a row of axes is as long as their plot areas. An axes whose fixed aspect ratio shrinks its plot box within that area leaves the bar longer than the box.
- **Inside an axes.** An axes' `colorbar` draws a bar along one edge of its plot area, over the artists, on a framed box like that of an inside legend so that its labels can be read over any colour. The box, keys included, is the stated fraction of that edge long and centred on it.
- **In a legend.** `LegendItemContent::ColorScale` shows a scale as an item of a legend. A categorical scale expands to one ordinary entry per category, which is merged, ordered and wrapped like any other entry; a continuous, discrete or indexed scale occupies one column as a vertical bar.

A bar's ends are always flat. A key for an out-of-range category is drawn only when the author asks for it, as a separate square beyond the bar and never as a cap at its end, because a colour at the end of a bar cannot be told apart from the last colour in range. Along a vertical bar the above key lies beyond the top, and the below key and then the non-finite key beyond the bottom; along a horizontal bar the non-finite and then the below key lie beyond the left end and the above key beyond the right. The outside key of an indexed scale takes the place of the below key. A square is filled with the colour that is painted, or left empty under `transparent` and `strict`, and is labelled `> upper`, `< lower`, `NaN / Inf` or `Out of range` unless the author gives a label. For a categorical scale the keys are further entries after the categories.

This supersedes ADR 0018's deferral of colourbars, and ADR 0011's stopgap of legend samples in the middle colour of the colormap: a colour-mapped artist's legend sample is a sweep of its scale.

## Alternatives considered

### A scale on the axes that artists may override

This was pull request #31. It was rejected because the rule deciding which scale an artist uses had to be applied identically by the scene compiler, the viewer, the exporters and every program that reads a figure.

### Restricting each artist to particular kinds of scale

Keeping a colour-indexed and a colour-mapped image, each accepting only its own kind of scale, was rejected because the restriction would need to be learnt for each artist, and because the distinction it draws belongs to the scale.

### An indexed scale with a switch between a bar and squares

A single index-mapped kind with a display switch was rejected because each mode needs fields that the other does not use: ticks and tick labels for a bar of a palette, and a required label per entry for a key of categories.

### Out-of-range colours at the ends of the bar

Drawing the colours of out-of-range values as caps at the ends of the bar, as other plotting software does, was rejected because it makes those values look like the extreme values in range.

## Consequences

- The facade creates scales with `fig.color_scale()`, `fig.indexed_scale(..)` and `fig.categorical_scale(..)`, and an axes' default scale is the scale of its first colour-mapped artist, a rule that depends only on the figure. `ax.colormap(..)`, `ax.clim(..)`, `indexed_image` and `mapped_image` are removed.
- A figure whose indices are fractional or outside the palette, previously drawn by truncation, is now refused unless its scale chooses another policy.
- A scale that no artist names is valid and raises no warning, because a custom colourbar needs one.
- The schema changes incompatibly: the axes loses its colormap and colour limits, the two index and value image kinds become one, and colour-mapped artists gain a scale reference.
