# Figure schema

This page explains the IronLAB figure model entity by entity, and the two encodings in which a figure is stored and transported. The model is the retained intermediate representation (IR) from which everything is drawn: the viewer, the PDF exporter and both file formats use it, and nothing that affects a drawing is stored anywhere else. The decision to build IronLAB around this model, and to encode it with Protocol Buffers by default, is recorded in [ADR 0001](../adrs/0001-retained-figure-ir-and-protobuf-wire-format.md).

## Source of truth

The Rust types in the `ironlab-ir` crate (`crates/ironlab-ir`) are the only definition of the model. The Protocol Buffers definition and the JSON Schema that describe the model to other software are generated from those types as build artefacts; neither is committed to the repository, so neither can drift from the code. How to generate them is described in [generating the schemas](#generating-the-schemas).

## Encodings

A figure has two encodings, which describe the same model:

| Encoding | File extension | Purpose |
| --- | --- | --- |
| Protocol Buffers | `.fig` | The default file and transport format: compact, fast for large arrays, and readable from any language with generated types. |
| JSON | `.json`, conventionally `.fig.json` | A secondary format for debugging, for tools that read text, and for simple web pages. |

`Figure::save` and `Figure::load` in the `ironlab` crate, and the `ironlab-viewer` binary, choose the encoding from the file extension, as described in [getting started](../guides/getting-started.md#saving-and-loading). The `<ironlab-figure>` element that shows a figure in a web page chooses it from the extension of its `src` attribute, or from its `format` attribute, as described in [embedding figures in a web page](../guides/embedding.md#the-element).

The entity sections below name each property as it appears in both encodings, and show tagged variants, enumerations and colours in their JSON form. The Protocol Buffers form of each follows from the conventions below.

### Protocol Buffers encoding

The Protocol Buffers messages are in package `ironlab.ir.v0`, with one `.proto` file per module of `ironlab-ir` at `ironlab/ir/v0/<module>.proto`; the root message is `Figure` in `ironlab/ir/v0/figure.proto`. The encoding follows these conventions.

- **Enumerations.** Every enum has the zero value `<ENUM>_UNSPECIFIED`, and every value is prefixed with the name of the enum in upper snake case. For example, the scale `log` is `SCALE_LOG`, and the legend location `north_east` is `LEGEND_LOCATION_NORTH_EAST`.
- **Presence.** Singular numeric and boolean fields are declared `optional`, so that a value that was written is distinguished from an absent one, and every value, including negative zero, reloads bit for bit.
- **Tagged variants.** An entity that takes one of several forms is a message with a single oneof named `kind`. Each variant of the oneof is a message of its own, such as `LimitsManual` with the fields `min` and `max`, so that a variant can gain fields in a later version without changing the others. A variant that holds a single value is also a message, such as `ParameterNumber` with the field `value`.
- **Numeric arrays.** An array is a `repeated uint64 shape`, an `NdArrayElement element` that names the type of its values, and one of two payloads: a packed `repeated double values` holds 64-bit floating-point values, in which NaN and infinities are stored natively as IEEE 754 values, and a `bytes u8_values` holds 8-bit unsigned integers, one byte per value. The encoder writes the element of every array and leaves the other payload empty. A reader takes an unspecified element as `ND_ARRAY_ELEMENT_F64`, which is what every file written before the element existed holds, and an array whose payloads disagree with its element (values in the other payload, or in both) is rejected with an error.
- **Data arrays.** The `data` of a figure is a `repeated DataArray`, written in the order the figure lists its data arrays. A DataArray holds its `id`, its `array` and its `name`, an `optional string` that is absent when the array has no name.
- **Parameters.** The `parameters` of a figure are a `map<string, Parameter>` keyed by name, written in ascending order of the UTF-8 bytes of the name, so that a figure always encodes to the same bytes.
- **Tags.** The `tags` of a figure are a `repeated string`, written in the order the figure carries them rather than sorted, because that order is part of the figure's value and is the order the viewer lists them in.
- **Colours.** A Color is a message of four `float` components, `r`, `g`, `b` and `a`, each from 0 to 1, stored at the full precision of the model.
- **Absent values.** An absent message, an absent field with presence, an unset oneof and an `_UNSPECIFIED` enum value take the default of their context in the model. For example, an absent line colour of a contour is colormapped, whereas an absent line colour of a line is automatic, and an absent [opacity](#alpha-and-opacity) is 1. The encoder writes every field that has a value, so a file written by IronLAB never relies on these defaults. An optional data reference, such as the colour data of a surface or the [alpha](#alpha-and-opacity) of an image, is written only when it is present, because 0 is a valid NodeId and cannot stand for an absent one.
- **Required values.** A value that has no default in the model, namely the kind of an artist, the `data` of [stairs](#stairs), the dimension of an axis link, and the kind of a parameter together with its value when it is a boolean, an integer or a number, makes decoding fail with an error when it is absent or unspecified.
- **Fields without presence.** Strings, bytes, repeated fields, maps and colour components cannot be distinguished from their empty or zero values, so they decode as the value on the wire: an empty string is empty and an empty list is empty.
- **Unknown values.** A field that the build does not know is skipped, so a file written by a later patch release still loads. An enum value that the build does not define makes decoding fail with an error rather than taking a default, because new enum values require a new minor version, and a file of another minor version is rejected before it is decoded.
- **Versioning.** Field 1 of `Figure` is `schema_version` in every version of the schema, so that a reader can check the version of a file before decoding the rest of it, as described in [versioning](#versioning).

Compatibility of the Protocol Buffers definition is checked in CI: `buf lint` checks the generated files, and `buf breaking` compares them with the files generated from the `main` branch, except when a commit of the pull request declares a breaking change, as described in [versioning](#versioning).

### JSON encoding

The JSON encoding follows these conventions.

- **Tagged variants.** An entity that takes one of several forms (such as an artist, a projection or limits) is a JSON object whose `type` property names the form in `snake_case`, alongside that form's own properties. For example, manual limits are `{"type": "manual", "min": 0, "max": 1}`. An entity whose forms each hold a single value, namely a [parameter](#parameters), the [legend](#legend) of an axes, the [number of legend columns](#legend-entry-layout), the [ticks](#ticks) and the [tick labels](#tick-labels) of an axis, the [size of a row or column](#tile-layout), the [entries of a legend](#legend-entries), the content of a legend item, the [content of a panel](#panel-content), the base and the slot of [bars](#bar) and the edges of [cells](#cells), holds that value in a `value` property, as in `{"type": "number", "value": 100000.0}`.
- **Enumerations.** An entity that is only a choice of name (such as a scale or a colormap) is a `snake_case` string, for example `"log"` or `"north_east"`.
- **Optional properties.** A property that may be absent is written as `null` when it has no value. The exceptions are the `parameters`, the `tags` and the `panels` of a figure, each of which is omitted when the figure has none, the `row_weights` and `col_weights` of the [tile layout](#tile-layout), each of which is omitted when it is empty, the `entries` of a [legend](#legend-entries), which are omitted when they are automatic, the [alpha and opacity](#alpha-and-opacity) of surfaces, images, bars, stairs and cells: an alpha reference is omitted when it is absent, and an opacity is omitted when it is 1. A file that omits them, such as a file written before they existed, therefore reads as opaque artists without alpha. Likewise, the `base` of [stairs](#stairs) is omitted when it is absent, and the `visible` and `tick_label_angle` of an [axis](#axis) are omitted when they are `true` and 0.
- **Non-finite numbers.** JSON cannot represent NaN or an infinity. In a data array of floating-point values, every non-finite value is written as `null` and read back as NaN. A non-finite value in any other numeric field cannot be represented, so a figure saved as JSON must have finite limits, sizes, view angles and number parameters, as [validation](#validation) requires.
- **Data arrays.** An array of 8-bit values carries `"element": "u8"` and writes its values as integers. An array of floating-point values has no `element` property, and an `element` of `null` or `"f64"` is read in the same way, so a file written before the element existed still loads. Under `"u8"`, every value must be an integer from 0 to 255: a whole number written with a fraction, such as `1.0`, is accepted as the byte it denotes, and `null`, a fraction, a number outside the range, a string and any other element name are refused.
- **Colours.** A Color is a string of eight bits per component, as described in [colours](#colours), so a colour that is not a multiple of 1/255 is rounded when it is saved as JSON.
- **Unknown properties.** A property that the build does not know is ignored, so a file written by a later patch release still loads.

A complete JSON file is shown in [a minimal JSON file](#a-minimal-json-file).

### Generating the schemas

The Protocol Buffers definition is written, together with a `buf.yaml` that configures `buf lint` and `buf breaking`, to `target/ironlab-proto/` by:

```sh
cargo run -p ironlab-ir --bin generate-proto
```

The JSON Schema of the JSON encoding, which uses JSON Schema draft 2020-12 and is split into one file per module of `ironlab-ir`, is written to `target/ironlab-schema/` by:

```sh
cargo run -p ironlab-ir --bin generate-schema
```

Both commands write to the directory named by `CARGO_TARGET_DIR` when it is set, and replace any files from an earlier run. Other languages generate their own types from the `.proto` files, for example with `buf generate` or `protoc`.

## Model conventions

The following conventions apply throughout the model, in both encodings.

- **Units.** Physical sizes are in millimetres (`_mm`), font sizes and line widths are in points (`_pt`, 1/72 inch), and angles are in degrees (`_deg`).
- **Identifiers.** Every node of a figure (the figure itself, its data arrays, its colour scales, its axes, their artists and its panels) is identified by a `NodeId`, an unsigned 64-bit integer, from one identifier space shared by all of them. Every reference from one node to another, such as an artist's reference to its data or to its colour scale, or a legend's reference to its panel, is a NodeId, and a reference to a node of the wrong kind is reported by [validation](#validation). The reasons for one identifier space are recorded in [ADR 0022](../adrs/0022-figure-nodes-in-one-identifier-space-and-panels.md).
- **Titles, labels, names and tags.** A property that holds text is named by whether and where its text is drawn. A **title** is drawn as a heading above what it names, such as the title of a figure, an axes or a panel; the title of a colour scale names the scale and is drawn along its bar unless the bar places it above. A **label** is drawn beside what it identifies, such as an axis label, a tick label or the label of an artist in a legend. A **name** identifies a node to tools and is never drawn, such as the name of a [data array](#data-arrays) or of a [parameter](#parameters). A **tag** describes a figure to the figure browser and is never drawn. The rule is recorded in [ADR 0023](../adrs/0023-titles-labels-names-and-tags.md).

## Figure

The figure is the root of the model: a page of a fixed physical size holding the data arrays that its artists plot, the colour scales through which they map numbers to colours, its axes, the links between them, and the panels that hold its keys.

| Property | Type | Meaning |
| --- | --- | --- |
| `schema_version` | string | The version of the schema the figure conforms to, such as `"0.7.0"`; see [versioning](#versioning). |
| `id` | NodeId | The identifier of the figure. |
| `title` | Text or `null` | The title drawn above all axes (MATLAB's `sgtitle`). |
| `size` | object | `width_mm` and `height_mm`, the physical size of the figure. The default is 160 mm by 100 mm. |
| `font_set` | string | The font set used for all text. The only font set is `"stix_two"`: STIX Two Text for text and STIX Two Math for mathematics. |
| `font_size_pt` | number | The base font size, from which titles and tick labels are scaled. The default is 9 pt. |
| `background` | Color | The colour of the page. The default is white. |
| `layout` | TileLayout | The grid of tiles in which axes and panels are placed, with the relative sizes of its rows and columns; see [tile layout](#tile-layout). The default is one tile. |
| `data` | array of DataArray | The [data arrays](#data-arrays) of the figure, which artists refer to by NodeId. |
| `color_scales` | array of ColorScale | The [colour scales](#colour-scales) of the figure, which colour-mapped artists refer to by NodeId. Omitted from JSON when the figure has none. |
| `axes` | array of Axes | The axes of the figure, in drawing order. |
| `panels` | array of Panel | The [panels](#panels) of the figure, which occupy tiles of the layout as axes do, hold its keys, and are drawn after every axes. Omitted from JSON when the figure has none. |
| `links` | array of AxisLink | The groups of axes whose limits are [linked](#links). |
| `provenance` | Provenance | A record of the software that wrote the figure; see [provenance](#provenance). |
| `parameters` | map | Named values that describe the figure, used to sort, filter and search collections of figures; see [parameters](#parameters). Omitted from JSON when the figure has none. |
| `tags` | array of string | Free words that describe the figure, used to filter and group collections of figures; see [tags](#tags). Omitted from JSON when the figure has none. |

Node identifiers are unique within a figure and do not change when a figure is saved and loaded, so that links, and in future selections and annotations, can refer to nodes across sessions.

Every property of this table and of the tables below can be read in the viewer's [property editor](../guides/viewer.md#the-property-editor), and most of them can be changed there; the editor leaves the structure of a figure and its data to the program that builds it.

### Tile layout

The tile layout divides the page below the figure title into a grid of tiles. Axes and panels each occupy a rectangular block of tiles, named by a `cell`.

| Property | Type | Meaning |
| --- | --- | --- |
| `rows` | integer | The number of rows, at least one. The default is 1. |
| `cols` | integer | The number of columns, at least one. The default is 1. |
| `row_weights` | array of TrackSize | The sizes of the rows, from the top. Omitted from JSON when it is empty. |
| `col_weights` | array of TrackSize | The sizes of the columns, from the left. Omitted from JSON when it is empty. |

A TrackSize is the size of one row or column, in one of two forms:

- `{"type": "weight", "value": number}` gives the plot areas of the row or column a share of the space, in proportion to the weight, which must be a finite positive number.
- `{"type": "fit"}` fits the row or column to the [panels](#panels) in it. A fitted row takes the height of the tallest area that a panel lying in that row alone needs for its content, and a fitted column the width of the widest such area in the same way. A legend needs the area of its entries, with the border inset of a boxed panel; a colourbar needs its thickness, its ticks, its tick labels and its title across the bar, and, along the bar, the length of its keys and the shortest bar. A fitted row or column in which no panel lies alone has no size.

The margins that the axes of every row and column reserve for their tick labels, axis labels and titles are taken out of the grid first, and the panels in a fitted row or column are aligned with the plot areas around them by those margins, or inset by the page padding on a side whose row or column holds no axes. The fitted rows then take their sizes, and the plot areas of the weighted rows share the rest of the height in proportion to their weights; the columns share the width in the same way. Plot areas of equal weight are therefore of equal size, however wide the labels beside them are and whatever fitted rows or columns lie beside them, and a legend panel in a weighted row or column without axes counts as a plot area of its weight. The rule for weights is recorded in [ADR 0019](../adrs/0019-axis-direction-ticks-blended-coordinates-and-aspect.md#plot-areas-not-cells-share-the-page-by-weight), and the rule for fitted sizes in [ADR 0020](../adrs/0020-explicit-legend-entries-and-fitted-tracks.md). A row or column without a size, because the list is shorter than the number of rows or columns, has a weight of 1, so empty lists, which are the default, give plot areas of equal size. A list must not be longer than the number of rows or columns it sizes. For example, `"col_weights": [{"type": "weight", "value": 2}, {"type": "weight", "value": 1}]` in a layout of three columns makes the plot area of the first column twice as wide as each of the other two, and `"row_weights": [{"type": "weight", "value": 1}, {"type": "fit"}]` makes the second row of two as tall as the legend panel it holds. An axes has no content size of its own, so an axes that lies in fitted rows alone, or in fitted columns alone, is a validation error; an axes that spans a fitted row and a weighted one is valid. When the fitted rows or columns need more than the space that the margins leave, they share it in proportion to their sizes, and the legends in them report that their entries do not fit with the scene warning `LegendOverflow`.

The number of rows and columns is part of the arrangement of the figure, so the property editor shows it read-only; the sizes can be changed there, as a list of weights in which the word `fit` stands for a fitted size. Two nodes whose cells share a tile are both drawn, the later over the earlier, and [validation](#validation) warns of them. In Protocol Buffers, the sizes are repeated `TrackSize` messages in fields 5 and 6 of the `TileLayout`, whose oneof holds a `TrackSizeWeight` or a `TrackSizeFit`; fields 3 and 4, which held the plain weights of schema 0.4, are reserved.

## Axes

An axes is a plotting region placed in one or more tiles of the figure's layout.

| Property | Type | Meaning |
| --- | --- | --- |
| `id` | NodeId | The identifier of the axes. |
| `cell` | object | The block of tiles occupied: `row` and `col` of the top-left tile, counted from zero at the top-left of the figure, and `row_span` and `col_span`, each at least one. Where an axes sits is part of the arrangement of the figure, so the property editor shows it read-only. |
| `projection` | Projection | Whether the axes is two- or three-dimensional. |
| `title` | Text or `null` | The title drawn above the axes. |
| `title_align` | string | How the title is aligned with the plot area: `center` (the default) centres it over the plot area, `left` puts its left edge on the left edge of the plot area, and `right` puts its right edge on the right edge of the plot area. |
| `x`, `y`, `z` | Axis | The coordinate axes. In a two-dimensional axes, `z` is ignored, and the property editor hides it until the axes is made three-dimensional. |
| `aspect` | Aspect | The ratios of the drawn length of one data unit of y, and of z, to that of one data unit of x; see [aspect](#aspect). |
| `aspect_adjust` | string | How a fixed aspect is kept: `box` (the default) or `limits`; see [aspect](#aspect). |
| `box` | boolean | Whether the full outline of the plot box is drawn, rather than only the edges that carry tick labels. The default is `true`. |
| `legend` | Legend or `null` | Where the entries of the axes are listed: in a legend inside the plot area, or in a legend panel of the figure; `null` when they are not listed. See [legend](#legend). |
| `colorbar` | InsideColorbar or `null` | The colourbar drawn inside the plot area, along one of its edges, or `null` when the axes shows none. See [inside colourbar](#inside-colourbar). |
| `artists` | array of Artist | The plots drawn in the axes, in drawing order: a later artist covers an earlier one. |

An axes holds no colormap and no colour limits. Every artist that maps numbers to colours names a [colour scale](#colour-scales) of the figure instead, so that the scale an artist uses never depends on where the artist lies.

### Aspect

- `{"type": "auto"}`, the default, lets the plot area fill the space the layout gives it, and keeps the box of a three-dimensional axes a cube, whatever the ratio of the lengths of the data units.
- `{"type": "equal"}` draws one data unit at the same length along every axis, so that geometry such as an aerofoil section or a sphere keeps its shape. It is the same as ratios of 1.
- `{"type": "ratio", "value": {"y": number, "z": number}}` draws one data unit of y `y` times as long as one data unit of x, and one data unit of z `z` times as long, as MATLAB's `daspect([1 y z])` does. A two-dimensional axes ignores `z`. Both must be finite positive numbers.

On a logarithmic axis the unit is one decade.

`aspect_adjust` says how a fixed aspect is kept:

- `"box"` keeps the limits exactly, manual or automatic. A two-dimensional axes shrinks its plot area along one dimension, centred in the space the layout gives it, once the margins of the layout are known. A three-dimensional axes gives the sides of its box the lengths of the spans of its limits multiplied by the ratios.
- `"limits"` keeps the shape of the plot area, or the cube of a three-dimensional axes, and widens the limits of the axes that are too short for it, symmetrically about their centres. Manual limits are then the least range shown: an axis that is long enough keeps them exactly, and only the others widen. Axes linked along a dimension widen together. Where linked axes ask for different shapes, their aspect cannot be kept by widening shared limits, and each keeps it by its plot area or its box instead, with a warning.

The box of a three-dimensional axes is fitted to the plot area by its circumscribing sphere, so it keeps its size and its proportions however the view is rotated.

The rules, and the way the viewer keeps the aspect while panning and zooming, are recorded in [ADR 0019](../adrs/0019-axis-direction-ticks-blended-coordinates-and-aspect.md#the-aspect-of-an-axes-is-kept-everywhere).

### Projection

- `{"type": "two_d"}` is a two-dimensional Cartesian axes.
- `{"type": "three_d", "view3d": View3d}` is a three-dimensional Cartesian axes seen through an orthographic camera.

A View3d has five properties:

| Property | Meaning |
| --- | --- |
| `azimuth_deg` | The rotation about the vertical axis, measured counterclockwise from the negative y axis when viewed from above. The default is −37.5. |
| `elevation_deg` | The angle of the view direction above the x–y plane, from −90 to 90. The default is 30. |
| `zoom` | The magnification of the projected box, where 1 fits the box to the plot area. |
| `pan_x` | The horizontal offset of the projected box as a fraction of the plot area's width, increasing to the right. The default is 0. |
| `pan_y` | The vertical offset of the projected box as a fraction of the plot area's height, increasing downwards. The default is 0. |

### Axis

| Property | Type | Meaning |
| --- | --- | --- |
| `label` | Text or `null` | The axis label. |
| `scale` | string | `"linear"` or `"log"`. On a logarithmic axis, non-positive values are not drawn. |
| `limits` | Limits | The range of data values shown. |
| `reverse` | boolean | Whether the axis runs from its upper limit to its lower limit: from left to right for a horizontal axis, and from bottom to top for a vertical one. The limits keep `min < max`. The default is `false`. |
| `ticks` | Ticks | Where the major ticks are placed; see [ticks](#ticks). |
| `tick_labels` | TickLabels | What is written at the major ticks; see [tick labels](#tick-labels). |
| `grid` | boolean | Whether grid lines are drawn at the major ticks of this axis. |
| `visible` | boolean | Whether the axis is drawn. The default is `true`. |
| `tick_label_angle` | number | The angle of every tick label in degrees anticlockwise from horizontal. The default is 0. |

A hidden axis, one whose `visible` is `false`, draws no spine (the edges of the plot box along it), no ticks, no tick labels and no axis label, and no space is reserved for them, so the plot area grows into the space they would have taken. Its grid lines still follow `grid`, and its limits, scale and ticks still place the data, so hiding an axis changes how the axes looks and nothing else. In a three-dimensional axes a hidden axis draws no ticks, tick labels or label, and the edges of the box, which belong to the box rather than to one axis, are drawn as they are for a visible axis.

The tick label angle rotates every tick label of the axis about its end nearest its tick, so that long category names can be written at an angle or vertically. The angle is taken modulo 360 and normalised to the interval (−180, 180]. An angle of 0 draws level labels as an axis without the property does, and an angle of 180 draws level labels upside down, each centred on its tick as a level label is. For any other angle the anchor of a label is the middle of the text's thickness at the end of the text that, once rotated, lies nearer the plot area: on the bottom axis that is the end of the text for a positive angle and its start for a negative one, and on the left axis it is the end of the text for angles from −90 to 90 and its start beyond them. Along the axis the anchor lies on the tick; across the axis the label is moved away from the plot area just far enough that the nearest point of its rotated box lies at the usual gap between the plot area and its tick labels, so that a rotated label never comes closer to the plot than a level one. The layout reserves the extent of the rotated labels, and the common exponent of the axis is not rotated. The angle may be any finite number; a non-finite angle is the validation error `InvalidTickLabelAngle`. A three-dimensional axes draws its tick labels along projected edges, where a rotation in the page has no agreed meaning, so it ignores the angle, and validation warns of an angle other than 0 there with `TickLabelAngleIgnored`. In JSON each property is omitted when it has its default, and in Protocol Buffers an absent `visible` is `true` and an absent `tick_label_angle` is 0, so files written before the properties existed show every axis with level labels.

A reversed axis behaves in every other respect as an unreversed one does: its automatic limits, its ticks, a logarithmic scale, panning and zooming in the viewer, datatips and export are unchanged, and only the direction in which values are drawn differs. [Links](#links) share limits only, so a reversed axis may be linked with an unreversed one, and each keeps its own direction. The reasons are recorded in [ADR 0019](../adrs/0019-axis-direction-ticks-blended-coordinates-and-aspect.md).

### Ticks

Ticks never change the limits of an axis, and a tick that lies outside the limits is not drawn.

- `{"type": "auto"}` chooses the ticks at round values from the limits and the length of the axis. On a linear axis whose manual limits are narrower than the round step chosen, more ticks are chosen, so that at least two lie within the limits of the x axis of a two-dimensional axes and at least three within those of any other axis, wherever the limits hold that many.
- `{"type": "step", "value": {"step": number, "origin": number}}` places a tick at every value `origin + k × step`, for every integer `k`, that lies within the limits. The step must be a finite positive number and the origin finite. A step so fine that it would place more than 500 ticks is drawn as automatic ticks, with a warning. On a logarithmic axis the step is a step in data units.
- `{"type": "values", "value": [number, …]}` places a tick at each of the values that lies within the limits, in any order. Every value must be finite, and positive on a logarithmic axis. An empty list draws an axis without ticks.

### Tick labels

- `{"type": "auto"}` labels each tick with its value. Automatic ticks are labelled as described in the documentation of the scene compiler. A regular step and explicit values are labelled with the fewest decimals that show every value, with a common exponent `×10^k` where automatic labels would have one, and as decades `10^n` on a logarithmic axis.
- `{"type": "hidden"}` draws the ticks without labels and without a common exponent, and reserves no space for labels, so the plot area grows into the space they would have taken.
- `{"type": "text", "value": [Text, …]}` labels the ticks with the texts, one for each explicit tick value, in the order of the values. It is valid only with `{"type": "values"}` ticks of the same number.

A categorical axis is expressed with these properties rather than with a type of its own: explicit ticks at `0, 1, …, N − 1`, explicit labels naming the categories, and, on a y axis, `reverse` so that the first category is at the top. The [categorical strip chart](../gallery/strip_chart.md) of the gallery is built this way.

### Limits

- `{"type": "auto"}` computes the range from the data, rounded outwards to tick values. For axis limits, the data of every axes linked along the same dimension is included, so that linked axes agree. Along x and y, an end of the range reached only by the grid of a contour or surface, by the pixel edges of an image, by the edges of [cells](#cells), or by the slots of [bars](#bar) or the edges of [stairs](#stairs) along their category axis, is the exact end of that grid or those edges rather than a tick value. The automatic limits of a colour scale are not rounded; they are described in [automatic limits](#automatic-limits).
- `{"type": "manual", "min": number, "max": number}` fixes the range. The bounds must be finite, `min` must be less than `max`, and both must be positive on a logarithmic axis.

### Inside colourbar

An **InsideColorbar** draws a [colour scale](#colour-scales) as a bar inside the plot area of its axes, along one edge, over the artists. The bar, its keys, its labels and its title stand on a box with a background and an outline, drawn as the box of a boxed inside legend is drawn, so that they read over any artist. The bar takes no margin, so the plot area keeps its size whether or not the bar is shown. What the bar draws belongs to its scale, as [colourbar panels](#colourbar-panels) describe.

| Property | Type | Meaning |
| --- | --- | --- |
| `scale` | NodeId | The colour scale that the bar shows. It must name a colour scale of the figure: an identifier that names no node is the error `UnknownColorScale`, and one that names another kind of node is the error `WrongNodeKind`, each reported against the axes. The property editor shows it read-only, because which scale a bar shows is part of the arrangement of the figure. |
| `side` | string | The edge of the plot area along which the bar lies, which also sets its direction: `south` (the default) and `north` lay it flat along the bottom or top edge, and `east` and `west` stand it upright along the right or left edge. The tick labels face the interior of the plot area. |
| `length` | number | The length of the box, keys included, as a fraction of the edge, centred along it: a finite number greater than 0 and at most 1, or the error `InvalidColorbarLength`. The default is 0.8. |
| `title_placement` | string | Where the title of the scale is drawn, within the box: `along` (the default) lies beyond the tick labels, toward the interior of the plot area, centred on the bar and turned to read upwards beside a vertical bar; `above` is an upright heading over the top end of a vertical bar, and lies on the side of a horizontal bar away from its tick labels, below a bar along the south edge and above a bar along the north edge. |

In Protocol Buffers the bar is an `InsideColorbar` message in field 16 of the `Axes`, absent when the axes shows none, whose `side` is the enum `InsideColorbarSide`, whose absent `length` means 0.8, and whose `title_placement` is the enum `ColorbarTitlePlacement`.

### Legend

The legend of an axes lists every artist of the axes that has a label, in artist order, unless its [entries](#legend-entries) are an explicit list. It takes one of two forms, each of which holds its content in a `value` property:

- `{"type": "inside", "value": InsideLegend}` draws the entries in a legend inside the plot area of the axes. This is the form that a legend takes by default.
- `{"type": "panel", "value": NodeId}` lists the entries in the [legend panel](#legend-panels) with that identifier, together with the entries of every other axes that names the same panel. The identifier must be that of a [panel](#panels) of the figure whose content is a legend: an identifier that names no node is the error `DanglingPanelLegend`, and one that names another kind of node, or a panel holding another kind of content, is the error `WrongNodeKind`. Which panel an axes uses is part of the arrangement of the figure, so the property editor shows it read-only.

An axes therefore shows its entries either inside its plot area or in a panel, never in both. In Protocol Buffers the two forms are the variants `inside`, a `LegendInside` whose field `inside` holds the InsideLegend, and `panel`, a `LegendPanel` whose field `panel` holds the identifier.

How to build each form, and how a legend panel merges the entries of several axes, is explained in the [legends guide](../guides/legends.md).

An InsideLegend has four properties:

| Property | Meaning |
| --- | --- |
| `location` | Where the legend is placed inside the plot area, as a [legend location](#legend-location). The default is `north_east`. |
| `boxed` | Whether the legend has a background and an outline. The default is `true`. |
| `layout` | How the entries are arranged in columns, as a [legend entry layout](#legend-entry-layout). The default is one column, filled from top to bottom. |
| `entries` | Which entries the legend lists, and in what order, as [legend entries](#legend-entries). The default is automatic entries. An item of an explicit list that names an artist must name an artist of the same axes. Omitted from JSON when the entries are automatic. |

#### Legend location

A legend location is one of `north_east`, `north_west`, `south_east`, `south_west` (the corners), `north`, `south`, `east`, `west` (the centres of the edges), `center`, or `best`. Inside a plot area, `best` chooses the corner that overlaps the least data, and `center` places the legend over the centre of the plot, where it covers the data. The content of a panel is placed by a [panel location](#panel-location) instead, which offers the same locations without `best`.

#### Legend entry layout

A legend entry layout arranges the entries of a legend in columns.

| Property | Meaning |
| --- | --- |
| `columns` | The number of columns: `{"type": "auto"}` chooses the fewest columns in which the entries fit the height available to the legend, and `{"type": "count", "value": n}` uses exactly `n` columns, which must be at least one. |
| `order` | The order in which the entries fill the columns: `column_major` fills each column from top to bottom before starting the next, and `row_major` fills each row from left to right before starting the next. The default is `column_major`. |

The default layout depends on where the entries are shown: an inside legend uses one column, and a legend panel uses automatic columns. A legend panel that lies in [fitted](#tile-layout) rows alone has a fixed width and a height that follows from its entries, so its automatic columns are chosen differently: the entries take the fewest rows that the width allows, in the fewest columns that give that many rows. An item on a row of its own, described below, takes a row whatever the number of columns.

#### Legend entries

The entries of a legend take one of two forms:

- `{"type": "auto"}` lists every artist with a label: for a legend inside a plot area, the artists of its axes in artist order, and for a legend panel, the artists of every axes that names the panel, merged by label and look, in the reading order of the layout. This is the default.
- `{"type": "explicit", "value": [LegendItem, ...]}` lists exactly the given items, in the given order, whatever the order in which the artists are drawn. An explicit list of no items lists nothing.

A LegendItem has two properties:

| Property | Meaning |
| --- | --- |
| `content` | What the item lists: `{"type": "artist", "value": NodeId}` lists that artist, with its label beside the sample drawn for it; `{"type": "custom", "value": {"label": Text, "sample": LegendSample}}` shows a label beside a sample and stands for no artist; and `{"type": "color_scale", "value": {"scale": NodeId, "rows": number}}` shows the [colour scale](#colour-scales) with the identifier `scale`, as described below. `rows` is the number of entry rows along which the bar of a continuous, discrete or indexed scale runs, a whole number of at least 1, or the error `InvalidLegendRows` against the owner of the legend; it is 5 for an item made by the facade unless the item asks for another number. It is ignored by a categorical scale and by an item on a row of its own, but validated whatever the kind of the scale or the row of the item. |
| `full_row` | Whether the item spans every column of the legend, on a row of its own at its position in the list. The label of such an item sets the width of no column. |

An item that shows a colour scale may name any scale of the figure, in a legend panel or inside a plot area, whether or not an artist uses it. What it draws depends on the kind of the scale:

- A **categorical** scale is listed as one entry per category, in category order, each labelled with the label of the category beside a swatch of its colour, followed by an entry for each [out-of-range key](#out-of-range-policies) that the scale shows, in the order of a colourbar's keys (outside, then non-finite), labelled with the key's label or its default label. These are ordinary entries, arranged in columns like any other. In a legend panel, an entry of a category is merged with any other entry of the panel that has the same label and the same sample, whether that entry lists an artist, stands for no artist or is another category, at the position of the first of them; it then stands for the artists of the entry it merges with. A legend inside a plot area merges nothing, so there every entry of a category is listed. On a row of its own, every entry of the scale is on a row of its own.
- A **continuous**, **discrete** or **indexed** scale is drawn as a vertical bar, `rows` entry rows long, in one column of the legend, as a vertical [colourbar](#colourbar-panels) of the scale is drawn: the same thickness, colours, ticks and tick labels, the bar centred on the sample column and its labels to its right. The title of the scale lies along the bar, beyond its tick labels, and adds no row. Each out-of-range key that the scale shows is an entry row of its own, a swatch beside the key's label as a swatch entry is drawn, directly above the bar's rows (the above key) or below them (the below or outside key, then the non-finite key); these rows are never merged with other entries. When the entries are divided among columns, such an item counts as its rows and its keys and is never split between columns. On a row of its own, the bar is horizontal and spans the width of the legend whatever its `rows`, with its labels below it and its title below them, and its keys are squares beside the bar, as a horizontal colourbar is drawn.

The items between two items on rows of their own form a group, which fills the columns on its own; every group has the same number of columns and the same order, and the columns of all groups share their widths, so that they line up. An item that names an artist in a legend panel may name an artist of any axes of the figure, and also stands for every artist of the axes that name the panel whose label and look match it, so that clicking it in the viewer hides or shows that series in all of them. An item that names an artist is greyed only when every artist it stands for is hidden, as a merged automatic entry is. An item that stands for no artist is never greyed, and clicking it does nothing; neither does clicking a bar item or the entry of a category, because a scale is not an artist, unless the entry is merged with an entry that lists an artist. The automatic entries of a legend list artists only, so a scale enters a legend only as an explicit item.

The sample drawn for an artist that maps numbers to colours is a sweep of its scale, from the low end at the left to the high end at the right, whatever the data of the artist: a gradient for a continuous scale, its bands side by side for a discrete scale, and its entry colours side by side for an indexed or categorical scale. Contour bands, a scalar image and a surface whose faces are colour-mapped draw the sweep as a filled band; colour-mapped isolines and a series whose line is colour-mapped draw it along the line of the sample; a marker that follows the colour data of a series point by point takes the colour of the sweep where the marker sits, and a series that draws only such markers draws a row of them across the sample. The sweep takes the opacity of the artist but not its alpha, which varies over the artist. A surface whose faces are one colour draws a swatch of that colour outlined in its edge colour.

A LegendSample is the sample of an item that stands for no artist, in one of two forms, each identified by its `type`:

- `{"type": "line", "line": LineStyle, "marker": MarkerStyle}` is drawn as the sample of a series is drawn: a horizontal line in the [LineStyle](#linestyle) with a marker in the [MarkerStyle](#markerstyle) at its centre. A dash of `none` draws the marker alone, and a marker shape of `none` draws the line alone. A line colour of `auto` is the ink in which the axes and labels of the figure are drawn, because the item belongs to no axes whose colour order could supply one, and a marker colour of `auto` takes the line colour.
- `{"type": "swatch", "color": Color}` is a square of the colour with a thin black outline, smaller than its row so that a margin separates the swatches of neighbouring rows. It is the sample that the category of a categorical scale draws, so in a legend panel a swatch of the same label and colour as a category is merged with the entry of the category.

In Protocol Buffers the entries are a `LegendEntries` message whose oneof holds a `LegendEntriesAuto` or a `LegendEntriesExplicit`, in field 4 of an `InsideLegend` and field 3 of a `PanelLegend`; an absent message means automatic entries. The content of an item is a `LegendItemContent` whose oneof holds a `LegendItemContentArtist`, a `LegendItemContentCustom` or a `LegendItemContentColorScale`, whose field `scale` holds the identifier of the scale and whose absent field `rows` means 5. Which entries a legend lists is part of how the figure is described by its author, so the property editor does not show them, and the [legends guide](../guides/legends.md#choosing-the-entries) explains how to build them.

## Panels

A **panel** is a node of the figure that occupies a block of tiles of the [tile layout](#tile-layout), as an axes does, and holds one key. Everything that follows from a panel occupying tiles is the same whatever it holds: how a [fitted](#tile-layout) row or column takes its size from the panels in it, how a panel is aligned with the plot areas around it, where its content is placed within it, and how [validation](#validation) checks its cell with `CellOutOfLayout` and `OverlappingCells`. The content decides only what is drawn in the panel and how much space that needs. The reasons for one kind of node with several kinds of content are recorded in [ADR 0022](../adrs/0022-figure-nodes-in-one-identifier-space-and-panels.md#a-panel-is-one-node-kind-whose-content-decides-what-it-draws).

| Property | Type | Meaning |
| --- | --- | --- |
| `id` | NodeId | The identifier of the panel. |
| `cell` | object | The block of tiles occupied, with the same properties as the `cell` of an [axes](#axes). The property editor shows it read-only, as it does the cell of an axes. |
| `align` | string | Where the content is placed within the panel, as a [panel location](#panel-location), and where a colourbar shorter than its span is placed along it, as [colourbar panels](#colourbar-panels) describe. The default is `center`. |
| `title` | Text or `null` | A heading drawn at the top of the panel's content, or `null` for none. In a legend panel it lies above the entries, inside the border of a boxed panel; above a vertical colourbar that spans plots it lies in the margin above the first row the panel spans, as the title of an axes does; above a horizontal colourbar it lies within the panel. A fitted row or column grows to hold it. The viewer's tree names a titled panel by its title. A title is not content, so a titled legend panel with nothing to list is still `NothingToDraw`. |
| `title_align` | string | How the title is aligned with the content it heads: `left`, `center` or `right`, as the `title_align` of an [axes](#axes) is aligned with its plot area. The facade gives a panel `left`. |
| `content` | PanelContent | What the panel holds, as [panel content](#panel-content). The property editor shows the kind of content read-only, because what a panel holds is part of the arrangement of the figure, and lets the properties of the content be changed. |

### Panel location

A panel location is one of `north_east`, `north_west`, `south_east`, `south_west` (the corners), `north`, `south`, `east`, `west` (the centres of the edges) or `center`. These are the [legend locations](#legend-location) without `best`, because a panel holds no data for its content to avoid. In Protocol Buffers a panel location is the enum `PanelLocation`, whose values are prefixed `PANEL_LOCATION_`.

### Panel content

The content of a panel is a tagged variant that holds its value in a `value` property, in one of two kinds:

- `{"type": "legend", "value": PanelLegend}` holds a [legend](#legend-panels) that lists the entries of every axes whose legend names the panel.
- `{"type": "colorbar", "value": PanelColorbar}` holds a [colourbar](#colourbar-panels) that shows a colour scale along the cells of the panel.

In Protocol Buffers the panel's `title` is a `Text` message in field 5, absent when there is none, and its `title_align` is the enum `TitleAlign` in field 6, whose absent value means `left`. The content is a `PanelContent` message whose oneof `kind` holds a `PanelContentLegend`, whose field `legend` holds the PanelLegend, or a `PanelContentColorbar`, whose field `colorbar` holds the PanelColorbar; an absent PanelLegend is the default legend of a panel.

### Legend panels

A panel whose content is a legend is called a **legend panel**. It is a legend of the figure rather than of one axes: it lists the entries of every axes whose [legend](#legend) names it, so that several axes can share one legend, or the items of an explicit list of [entries](#legend-entries). A figure may have several legend panels, for example one beneath each column of axes. A panel with no entries to list is valid but empty, and [validation](#validation) warns of it. A panel lists each series once when entries of several axes have the same label and are drawn the same way; the entries are listed in the reading order of the cells of their axes, by the row and then the column of the top-left tile of each cell, with the order of the axes in the figure breaking ties, and then in the order of the artists of each axes. The merge rule, the order of the entries and the placement of a panel are described in the [legends guide](../guides/legends.md#sharing-and-merging-entries), and the reasons for them in [ADR 0018](../adrs/0018-legend-panels-with-entries-merged-by-name-and-look.md).

The legend of a panel, a **PanelLegend**, holds only what a legend needs, because the cell and the alignment belong to the panel:

| Property | Type | Meaning |
| --- | --- | --- |
| `boxed` | boolean | Whether the area of the panel is outlined by a border, drawn as the box of an [axes](#axes) is drawn and without a background. The area is aligned with the plot areas around it, so the border continues the edges of their boxes, and the entries are kept inset from it. The default is `true`. |
| `layout` | object | How the entries are arranged in columns, as a [legend entry layout](#legend-entry-layout). The default is automatic columns, filled column by column. |
| `entries` | object | Which entries the panel lists, and in what order, as [legend entries](#legend-entries). The default is automatic entries. Omitted from JSON when the entries are automatic. |

### Colourbar panels

A panel whose content is a colourbar is called a **colourbar panel**. Its bar runs along the cells of the panel, which are inset by the aligned margins of the rows or columns they span, and its length is measured against the **span** of the plots beside it, so that one panel can show the scale that a whole row of plots shares. The span is the area of the panel along the bar, except that where a fixed [aspect](#aspect) shrinks a plot box within its plot area, the span runs instead from the near edge of the first drawn plot box to the far edge of the last among the axes in the rows or columns the panel spans; a panel whose rows or columns hold no axes keeps the whole area. The bar, keys included, is `length` of the span long, and the [panel location](#panel-location) places it along the span as well as across its track: along the span a bar shorter than the span is centred by `center` and by the locations that centre it along its direction (`east` and `west` along a vertical bar, `north` and `south` along a horizontal one), and is pushed to the end that any other location names. Across its length the bar takes the room it needs, which is the size of a [fitted](#tile-layout) row or column and does not depend on the length. Every bar is three quarters of the base font size thick. The title of the scale is drawn where `title_placement` says, and the title of the panel heads the bar.

A bar holds no colours, limits, ticks or keys of its own. Its colours are those of its [scale](#colour-scales), drawn according to the [kind](#colour-scale-kinds) of the scale: a gradient of 256 samples for a continuous scale, one block per band for a discrete scale, one block per entry for an indexed scale, and one outlined square per category, each with its label, for a categorical scale. Its ticks are the `ticks` and `tick_labels` of the scale, measured as those of an axis are; automatic ticks of a discrete scale mark every band edge, and the ticks of an indexed scale are entry indices, each drawn at the centre of its entry. The ends of the bar are flat and never carry the colour of values out of range. Such values are shown by the keys of the categories whose `show_key` is set, as [out-of-range policies](#out-of-range-policies) describe, each a separate square beyond the end of the bar whose values it stands for.

| Property | Type | Meaning |
| --- | --- | --- |
| `scale` | NodeId | The colour scale that the bar shows. It must name a colour scale of the figure: an identifier that names no node is the error `UnknownColorScale`, and one that names another kind of node is the error `WrongNodeKind`, each reported against the panel. The property editor shows it read-only, because which scale a bar shows is part of the arrangement of the figure. |
| `orientation` | string | `vertical` (the default) stands the bar upright, with low values at the bottom and its tick labels on the right; `horizontal` lays it flat, with low values at the left and its tick labels below. |
| `length` | number or `null` | The length of the bar, keys included, as a fraction of the span: a finite number greater than 0 and at most 1, or the error `InvalidColorbarLength` against the panel. `null`, the default, is the automatic length: the whole span for a vertical bar and half of it for a horizontal one. |
| `title_placement` | string | Where the title of the scale is drawn: `along` (the default) lies beyond the tick labels, one gap from them and centred on the bar, turned to read upwards beside a vertical bar and upright below the tick labels of a horizontal one; `above` is an upright heading, in the margin above the first row the panel spans for a vertical bar, as the title of an axes is, and within the panel above a horizontal bar. |

In Protocol Buffers the orientation is the enum `ColorbarOrientation`, an absent `length` (field 3) is the automatic length, and the placement is the enum `ColorbarTitlePlacement` (field 4). How to build a colourbar panel, and how to span a row of plots with one, is explained in the [colour guide](../guides/colour.md#showing-a-scale-with-a-colourbar).

## Colour scales

A **colour scale** is a node of the figure that turns numbers into colours. Every artist that maps numbers to colours, namely a [`scalar_image`](#scalar_image), a [`contour`](#contour), a [`surface`](#surface) and a [`series`](#series) coloured by data, names its scale explicitly by the NodeId in its `color_scale` property. Nothing is inherited: the axes holds no scale, and an artist that maps numbers to colours without naming a scale is the validation error `MissingColorScale`. Several artists, in one axes or in several, may name the same scale, and then share its limits and its colours. Any artist that takes a scale accepts a scale of any kind. A scale that no artist names is valid and raises no warning. The reasons for this model are recorded in [ADR 0024](../adrs/0024-colour-scales-colourbars-and-colour-keys.md), and the [colour guide](../guides/colour.md) shows how a program creates and shares scales.

| Property | Type | Meaning |
| --- | --- | --- |
| `id` | NodeId | The identifier of the scale. |
| `title` | Text or `null` | The title drawn beside the bar by every colourbar that shows the scale: along the bar or above it, as the `title_placement` of the bar says. |
| `kind` | ColorScaleKind | How values become colours, and how the scale is drawn; see [colour scale kinds](#colour-scale-kinds). |
| `non_finite` | ColorScaleOutOfRange | What is drawn for a value that is NaN or infinite, on every kind of scale; see [out-of-range policies](#out-of-range-policies). The default policy is `transparent`. |

### Colour scale kinds

The kind of a scale is a tagged variant that holds its value in a `value` property, such as `{"type": "continuous", "value": {"colormap": …, "limits": …}}`. The kind fixes both how values become colours and how the scale is drawn, so that a colourbar cannot misrepresent the mapping it shows.

- **`continuous`** interpolates a [colormap](#colormaps) between two limits, and is drawn as a smooth bar.
- **`discrete`** gives every value the single colour of the band in which it falls, and is drawn as a bar of bands.
- **`indexed`** is a palette, of which a whole number `i` takes entry `i`, such as the 256-entry palette of an indexed image. It is drawn as a bar of adjoining blocks, one per entry.
- **`categorical`** is a list of labelled categories, of which a whole number `i` takes category `i`. It is drawn as separate outlined squares, each with its label.

A **continuous** scale has the following properties.

| Property | Meaning |
| --- | --- |
| `colormap` | The [colormap](#colormaps) that the scale interpolates. The default is viridis. |
| `limits` | The values mapped to the first and last colours of the colormap, as [Limits](#limits). Automatic limits, the default, are described in [automatic limits](#automatic-limits). Manual limits must be finite and strictly increasing, and positive under a logarithmic norm. |
| `norm` | `linear` (the default) or `log`, the [norm](#norms) in which values are placed between the limits. |
| `below`, `above` | The ColorScaleOutOfRange of a value below the lower limit and above the upper limit. The default policy of each is `clamp`. |
| `ticks`, `tick_labels` | The [ticks](#ticks) and [tick labels](#tick-labels) of a colourbar that shows the scale. The defaults are automatic. |

A **discrete** scale has the properties of a continuous scale, except that `levels` takes the place of `limits`:

| Property | Meaning |
| --- | --- |
| `levels` | The [band edges](#band-edges) of the scale. The default is about ten automatic bands between automatic limits. |

With k bands, band i takes the colour of the colormap at the position i / (k − 1), so that both ends of the colormap are used; a single band takes the colour at the start. Automatic ticks of a discrete scale are one per band edge.

An **indexed** scale has the following properties.

| Property | Meaning |
| --- | --- |
| `colors` | The entries of the palette, as an array of [Color](#colours): at least one. A value `i` takes `colors[i]`. |
| `outside` | The ColorScaleOutOfRange of a value that names no entry. The default policy is `strict`. |
| `ticks`, `tick_labels` | The ticks of a colourbar that shows the scale, placed at the centres of the entries, and their labels. Automatic ticks fall on whole numbers only. A few entries of a long palette are labelled with explicit ticks and tick labels, such as the values 0, 128 and 255 labelled `water`, `land` and `ice`. |

A **categorical** scale has the following properties.

| Property | Meaning |
| --- | --- |
| `categories` | The categories, at least one, each an object of a `color` and a `label` (a [Text](#text)). The label is required, so that every key that shows the scale can tell every category apart. A value `i` takes `categories[i]`. |
| `outside` | The ColorScaleOutOfRange of a value that names no category. The default policy is `strict`. |

In Protocol Buffers a scale is a `ColorScale` message in field 16 of the `Figure`, whose `kind` is a `ColorScaleKind` message with a oneof of `ColorScaleKindContinuous`, `ColorScaleKindDiscrete`, `ColorScaleKindIndexed` and `ColorScaleKindCategorical`, each holding the message of its kind.

### Colormaps

A **Colormap** is the palette that a continuous or discrete scale interpolates, in one of two forms:

- `{"type": "named", "name": string, "reverse": boolean}` is a bundled colormap: `viridis` (the default), `cividis`, `magma`, `inferno`, `plasma`, `coolwarm` or `gray`. Each has 256 entries that share the positions from 0 to 1 equally, as matplotlib's colormaps do. With `reverse` set to `true`, the colormap runs from its last colour to its first.
- `{"type": "listed", "colors": [Color, …]}` is a colormap of the author's own: from 2 to 1024 colours, interpolated linearly in sRGB from the first, at the start of the scale, to the last, at its end. A listed colormap of any other length is the validation error `InvalidColormap`.

### Norms

The **norm** of a continuous or discrete scale is the space in which values are placed between its limits. Under `linear`, the position of a value is proportional to the value. Under `log`, it is proportional to the base-10 logarithm of the value, so that each decade takes an equal share of the colormap, and a value that is not positive lies below the range. An indexed or categorical scale has no norm, because a value names its entry directly.

### Band edges

The **band edges** (`levels`) of a discrete scale take one of two forms:

- `{"type": "auto", "count": integer, "limits": Limits}` chooses about `count` bands, at least one, between nice edges within the limits, whose lower and upper limits are the first and last edges. Under a logarithmic norm the inner edges are chosen as the ticks of a logarithmic axis are. Automatic limits are described in [automatic limits](#automatic-limits).
- `{"type": "explicit", "values": [number, …]}` gives the edges explicitly: at least two, finite, strictly increasing, and positive under a logarithmic norm.

Edges that break these rules, and an automatic count of zero, are the validation error `InvalidLevels`. A [contour](#contour) that has no levels of its own takes the band edges of a discrete scale as its levels, so that a filled contour and the bar of its scale cannot disagree.

### Out-of-range policies

Each category of value that a scale cannot place within its range is a **ColorScaleOutOfRange**, which holds the policy of the category and the look of its key:

| Property | Meaning |
| --- | --- |
| `policy` | The OutOfRange policy: what is drawn for a value of the category. |
| `show_key` | Whether a colourbar that shows the scale draws a key square for the category, separate from the bar. The default is `false`. The square is as wide as the bar is thick, lies one margin beyond the bar or the previous square, and carries its label on the side of the tick labels. It is filled with the colour that the policy paints, a fixed colour or the end colour of a clamp below or above the range, and is empty when the policy paints nothing or no single colour. Along a vertical bar the above key lies beyond the top, and the below key and then the non-finite key beyond the bottom; along a horizontal bar the non-finite key and then the below key lie beyond the left end, and the above key beyond the right. The outside key of an indexed scale takes the place of the below key, and the keys of a categorical scale follow its categories. |
| `label` | The label of the key square, or `null` for the default label: `> upper` above the range, `< lower` below it, `NaN / Inf` for non-finite values and `Out of range` outside the entries, with the limits formatted as the ticks are. |

The categories are `below` and `above` the range of a continuous or discrete scale, `outside` the entries of an indexed or categorical scale, and `non_finite` on every kind. One policy per category lets a figure tolerate NaN while refusing values outside the range, or the reverse. An **OutOfRange** policy takes one of four forms:

- `{"type": "strict"}` makes such a value a validation error, `ValueOutOfRange`, so that the figure is refused until the data is corrected. It is the default of `outside`.
- `{"type": "transparent"}` draws nothing for the value. It is the default of `non_finite`, because a NaN is how an author says that nothing is drawn at a point.
- `{"type": "clamp"}` draws the value in the nearest end colour of the scale: the first colour of the colormap, or the first band, below the range, and the last above it. It is the default of `below` and `above`. Outside the entries of an indexed or categorical scale, a clamp truncates the value toward zero and then clamps it to the first or last entry. A non-finite value has no nearest end, so a clamp at `non_finite` draws nothing; the property editor lists the choice there and shows it disabled with the reason, as [using the viewer](../guides/viewer.md#what-the-editor-does-not-change) describes.
- `{"type": "rgba", "color": Color}` draws the value in a fixed colour.

Validation checks a strict category against the numbers that an artist holds: the values of a scalar image, the field of a contour, the colour data of a surface (or its field) and the colour data of a series. It never checks numbers derived from them, such as the mean of the corners of a surface face. No value lies beyond automatic limits, which are the range of the data, except a value that is not positive under a logarithmic norm, which lies below the range. 8-bit values are whole numbers, so they never fall outside a palette of 256 entries.

### Mapping rules

Every artist maps its numbers to colours by one rule, which the scene compiler and validation share.

- **Continuous.** A value inside the limits takes the colour of the colormap at its position between them, measured in the space of the norm. A value exactly at a limit lies inside. When the limits are equal, every value inside takes the middle colour.
- **Discrete.** A value takes the colour of the band in which it falls. A value exactly on an edge between two bands belongs to the upper band, and a value exactly at the first or last edge lies inside. Every artist is banded, a smooth surface included, so that the bar that shows the scale is truthful.
- **Indexed and categorical.** A whole number `i` from 0 to the number of entries less one takes entry `i`; negative zero counts as 0. Any other finite value, a fractional value included, lies outside the entries.
- **Out of range.** A value below or above the range, outside the entries, or not finite follows the policy of its category, as [out-of-range policies](#out-of-range-policies) describes.

Through an indexed or categorical scale, a surface face takes the value of its first corner rather than the mean of its four corners, because the mean of indices is not an index, and a contour with no levels of its own places its levels halfway between whole numbers, so that each band holds exactly one entry and each isoline takes the entry of the band below it.

### Automatic limits

The automatic limits of a scale, whether the `limits` of a continuous scale or the `limits` of automatic band edges, are the least and greatest finite values that any artist naming the scale maps through it, in every axes, hidden artists included. Under a logarithmic norm only the positive finite values count. An 8-bit value counts as the whole number it denotes. A scale whose automatic limits range over no value at all is drawn between 0 and 1, or between 1 and 10 under a logarithmic norm.

Only the numbers that an artist actually maps count: a contour counts its field when its bands are filled or its isolines are colour-mapped, a surface counts its colour data (or its field) when its faces or edges are colour-mapped, and a series counts its colour data whenever it has some, whatever its colours. A contour whose isolines have a fixed colour and are not filled, for example, names a scale only for its levels and does not widen the scale's limits.

## Artists

An artist is a drawable node of an axes. Every artist has three common properties:

| Property | Type | Meaning |
| --- | --- | --- |
| `id` | NodeId | The identifier of the artist. |
| `label` | Text or `null` | The label drawn beside the sample of the artist in a legend. An artist without a label is not listed by automatic [legend entries](#legend-entries). |
| `visible` | boolean | Whether the artist is drawn. A hidden artist emits nothing, but its data still contributes to automatic limits and it keeps its place in the colour order, so hiding it changes nothing else. |

The artist variants are listed below, each with the MATLAB functions it represents. Array lengths and shapes are checked by validation, not by the schema.

### `series`

A sequence of points drawn as a line through them, a marker at each, or both, optionally sized and coloured by data (MATLAB's `plot`, `plot3`, `loglog`, `semilogx`, `semilogy`, `scatter` and `scatter3`). A line without markers and markers without a line are the same artist with different styles, so a series can move continuously between the two; the reasons are recorded in [ADR 0025](../adrs/0025-series-replaces-line-and-scatter.md).

| Property | Meaning |
| --- | --- |
| `x`, `y` | NodeIds of the data arrays of the point coordinates, one value per point. |
| `z` | NodeId of the data array of the z coordinates, or `null`. Allowed only in three-dimensional axes; without it, points lie in the plane z = 0. |
| `line` | The LineStyle of the line through the points. A dash of `none` or a colour of `none` draws no line, and a non-finite coordinate breaks the line. Its colour is the colour of the series, which an automatic marker face or edge follows; a `colormapped` colour takes the colours of the colour data. |
| `marker` | The MarkerStyle of the markers drawn at the points. A shape of `none`, the default, draws no markers. Its `size_pt` is the diameter of every marker without size data, and of the marker of the legend sample in either case. |
| `size_data` | NodeId of the data array of the marker diameters in points, one value per point, or `null`, for markers that all take `marker.size_pt`. |
| `color_data` | NodeId of the data array of the values that colour the series through its colour scale, or `null`, for a series of fixed colours. The array holds one value per point or one value per segment, as [colour data](#colour-data-of-a-series) describes. |
| `color_scale` | NodeId of the [colour scale](#colour-scales) through which the colour data is mapped, or `null`. A series with colour data must name one; a series without colour data maps nothing and needs none. |
| `x_coords`, `y_coords` | The [coordinates](#coordinates-and-clipping) of the x and y values: `data` (the default) or `axes_fraction`. |
| `clip` | Whether the artist is [clipped](#coordinates-and-clipping) to the plot area. The default is `true`. |

The facade's `plot` makes a series with a solid line in the automatic colour and no markers, which is the default of every property, and `scatter` makes one with a dash of `none` and unfilled circles. The edges of markers are stroked at the width of the line held between 0.5 pt and 1.5 pt when the series draws a line, and at 0.5 pt when it draws none.

#### Colour data of a series

The colour data of a series is mapped through the scale the series names, of any kind. Its length decides what it colours:

- **One value per point** colours each point. A marker whose face or edge is `auto` or `colormapped` takes the colour of its point's value, and, when the line colour is `colormapped`, each segment of the line is drawn as a gradient from the colour of its first point to the colour of its second. The gradient interpolates between the two colours in sRGB, not between the two values, so that a segment between two entries of an indexed or categorical scale never passes through the colour of a third. A gradient is drawn as pieces of one colour, cut so that no piece is longer than 1 pt on the page, with at most 256 pieces per segment.
- **One value per segment**, one fewer than the points, colours each segment of a `colormapped` line in the colour of its value; segment `i` joins points `i` and `i + 1`. The markers then take the line's resolved colour where they follow it. Validation refuses this form with a `ShapeMismatch` error when the series draws no line, because it has no segments to colour, and when a marker face or edge is `colormapped`, because no value belongs to a point.

Any other length is a `ShapeMismatch` error, as is size data whose length differs from the number of points. A value that the scale cannot place follows the scale's [out-of-range policy](#out-of-range-policies). A value per point for which the scale draws nothing, such as a missing value under the default policy, removes its point: its marker is not drawn and the line breaks there, so neither segment that meets the point is drawn. A value per segment for which the scale draws nothing removes that segment alone. When the line colour is fixed, the line is drawn in that colour and only the markers that follow the colour data are coloured by it, so a series can have a black line with colour-mapped markers.

In a two-dimensional axes a line of one colour is stroked as one path, and a colour-mapped line segment by segment. In a three-dimensional axes every line is drawn segment by segment, each segment carrying the depths of its two points, so that a line passing through a surface is drawn behind it and in front of it where it should be; each corner between two drawn segments is filled with a disc as wide as the line, so that it looks as a round join would.

### Coordinates and clipping

Each of the x and y dimensions of a series is placed in its own coordinates, named by `x_coords` and `y_coords`, so that one dimension can be data while the other is not, as in matplotlib's blended transforms.

- `data` places the values through the limits, the scale and the direction of the axis, and the values take part in its automatic limits.
- `axes_fraction` places the values as fractions of the plot area along that dimension: 0 is the left edge (for x) or the bottom edge (for y) and 1 the right or top edge, whatever the limits, the scale, the direction or the pan and zoom of the axis, and values outside `[0, 1]` lie beyond the plot area. These values take no part in the automatic limits of the axis, and a zero or negative fraction is not reported against a logarithmic axis, because a fraction is not a value along the axis.

A horizontal reference line is therefore a series with `x_coords` of `axes_fraction`, x values 0 and 1 and two equal y values in data coordinates; its y value takes part in the automatic y limits, as matplotlib's `axhline` does. A three-dimensional axes has no plot area to take a fraction of, so `axes_fraction` in either dimension of an artist of a three-dimensional axes is a validation error.

`clip` decides whether the marks of the artist are clipped to the plot area of a two-dimensional axes. With `false`, clipping is released only along a dimension in `axes_fraction` coordinates, where the marks are drawn wherever they are placed, on screen and in exported files; this lets a marker on an edge of the axes be drawn whole. Along a dimension in `data` coordinates the artist stays clipped to the plot area exactly as with `true`, so that a value outside the limits of that axis is never drawn: an edge tick at a data x and a fractional y may overhang the top or bottom edge, but is never drawn left or right of the plot area, however the axes is panned or zoomed. `clip` therefore has no effect on an artist in `data` coordinates along both dimensions, nor in a three-dimensional axes, which clips every artist to its tile, and validation reports a released clip there with the warning `IneffectiveClip`.

The property editor of the viewer shows `x_coords` and `y_coords` read-only, because the coordinates of the values decide what the values mean and are therefore set by the program that builds the figure, and lets `clip` be changed. A datatip reports only the coordinates in `data`. The reasons are recorded in [ADR 0019](../adrs/0019-axis-direction-ticks-blended-coordinates-and-aspect.md#blended-coordinates-place-a-coordinate-relative-to-the-plot-area).

### `contour`

Isolines or filled bands of a scalar field sampled on a grid (MATLAB's `contour`, `contourf` and `contour3`).

| Property | Meaning |
| --- | --- |
| `grid` | The [grid](#grids) on which the field is sampled. |
| `z` | NodeId of the data array of the field, an array of shape `[ny, nx]`. |
| `levels` | `null`, the default, takes the levels of the colour scale: the [band edges](#band-edges) of a discrete scale, levels halfway between the entries of an indexed or categorical scale, and ten automatic levels through a continuous scale. `{"type": "auto", "count": integer}` chooses about `count` levels at round values; `{"type": "explicit", "values": [number, …]}` gives strictly increasing levels. In Protocol Buffers an absent `levels` message is `null`. |
| `fill` | `true` fills the bands between levels (`contourf`); `false` draws only isolines. |
| `placement` | `{"type": "plane", "z": number or null}` places every contour in one horizontal plane, at the bottom of the z axis when `z` is `null`; `{"type": "at_level"}` places each isoline at the height of its level (`contour3`) and is valid only in three-dimensional axes. |
| `line` | The LineStyle of the isolines. A colormapped or automatic colour takes the colour of each isoline's level through the colour scale. |
| `color_scale` | NodeId of the [colour scale](#colour-scales) through which levels and bands are coloured, or `null`. A contour whose bands are filled, or whose isolines are colormapped or automatic, must name one. A contour that names a scale only for its levels, because its isolines have a fixed colour, may name one too. |
| `line_opacity` | The [opacity](#alpha-and-opacity) of the isolines, a number from 0 to 1, which multiplies the alpha of their colour. The default is 1. |
| `fill_opacity` | The [opacity](#alpha-and-opacity) of the filled bands, a number from 0 to 1, which multiplies the alpha of their colour. The default is 1, and it has no effect when `fill` is `false`. |

### `quiver`

Arrows representing a vector field (MATLAB's `quiver` and `quiver3`).

| Property | Meaning |
| --- | --- |
| `x`, `y`, `z` | NodeIds of the data arrays of the arrow tails, one value per arrow; `z` as for a line. |
| `u`, `v` | NodeIds of the data arrays of the vector components, one value per arrow. |
| `w` | NodeId of the data array of the z components, or `null`. Allowed only in three-dimensional axes. |
| `scale` | `{"type": "auto"}` scales arrows so that they do not overlap; `{"type": "factor", "value": number}` multiplies the automatic scale; `{"type": "off"}` draws vectors with their lengths in data units. |
| `line` | The LineStyle of the shafts and heads. |
| `head_size` | The length of each arrow head as a fraction of its arrow's length. The default is 0.3. |

### `surface`

A surface of quadrilateral faces over a grid (MATLAB's `surf` and `mesh`, and, in a two-dimensional axes, MATLAB's `pcolor`).

A surface is valid in either projection. In a three-dimensional axes `z` is the height of every node. In a two-dimensional axes the surface is seen from directly above, as a pseudocolour plot: the grid alone places the faces, and `z` positions nothing, reaches no axis limit and is not subject to the scale of the z axis, but still colours the faces unless `c` is given. The values belong to the nodes of the grid, so a grid of `ny` by `nx` nodes draws `(ny − 1) × (nx − 1)` faces, and a surface whose field has no rows or no columns, or a single row or a single column of nodes, is valid but draws nothing: [validation](#validation) warns of it and the scene compiler leaves it out with a warning, as [ADR 0012](../adrs/0012-empty-and-singleton-data.md) decides. The guide compares a surface in a two-dimensional axes with a scalar image in [surfaces](../guides/getting-started.md#surfaces-in-two-dimensional-axes).

Every node of a surface is placed through the scale of its axis, in either projection, so a surface may lie along logarithmic axes; a face that touches a node at a coordinate that is not positive along a logarithmic axis is not drawn, and validation warns of it with `NonPositiveOnLogAxis`. A surface is therefore the artist for values sampled along a logarithmic axis, on which an image cannot be placed, as [image placement](#image-placement) explains.

| Property | Meaning |
| --- | --- |
| `grid` | The [grid](#grids) on which the surface is sampled. |
| `z` | NodeId of the data array of the field, an array of shape `[ny, nx]`: the height of every node in a three-dimensional axes, and the colour data of every node unless `c` is given. |
| `c` | NodeId of the data array of colour data with the same shape as `z`, or `null` to colour by `z`. |
| `face` | The ColorSpec of the faces. A colormapped or automatic face takes the colour of the mean of its four corner values through the colour scale; a face with a missing corner is not drawn. |
| `edge` | The ColorSpec of the face edges, coloured as the faces are when colormapped or automatic. |
| `color_scale` | NodeId of the [colour scale](#colour-scales) through which the colour data is mapped, or `null`. A surface whose faces or edges are colormapped or automatic must name one. |
| `edge_width_pt` | The width of the edges. The default is 0.5 pt. |
| `face_alpha` | NodeId of the data array of the [alpha](#alpha-and-opacity) of the faces at every node, an array of floating-point values with the same shape as `z`, or absent for faces whose alpha is 1. A face takes the mean of the alpha at its four corners, each clamped to `[0, 1]` before the mean is taken, as it takes the mean of their colour data; a face with a corner whose alpha is not finite is transparent. |
| `edge_alpha` | NodeId of the data array of the alpha of the face edges at every node, with the same shape as `z`, or absent for edges whose alpha is 1. The edges of a face take the mean of the edge alpha at its corners, clamped and made transparent by a non-finite value as the face alpha is. It may name the same array as `face_alpha`. |
| `face_opacity` | The [opacity](#alpha-and-opacity) of the faces, a number from 0 to 1, which multiplies the alpha of every face. The default is 1. |
| `edge_opacity` | The opacity of the face edges, a number from 0 to 1, which multiplies the alpha of every edge. The default is 1. |

`surf` and `surface` store colormapped faces with black edges, and `mesh` stores faces in the background colour with colormapped edges. `surf` and `mesh` convert the axes to three dimensions, and `surface` leaves the projection of the axes as it is.

### `bar`

A series of bars: one rectangle for each position along a category axis, which extends along the value axis from a base to its value (MATLAB's `bar` and `barh`, and Matplotlib's `bar` and `barh`). A histogram drawn as bars is a bar series whose widths are the widths of its bins and whose gap is 0; the facade bins the samples and stores the bars, so the samples themselves are not part of the figure.

| Property | Meaning |
| --- | --- |
| `orientation` | `"vertical"` (the default) places the positions along x and the values along y, so that the bars stand upright; `"horizontal"` places the positions along y and the values along x, so that the bars lie flat. |
| `positions` | NodeId of the data array of the centre of the slot of each bar along the category axis, one value per bar. The positions need not be sorted. A bar whose position is not finite is not drawn, and takes no part in the pitch. |
| `values` | NodeId of the data array of the end of each bar along the value axis, one value per bar. A bar whose value is not finite is not drawn. |
| `base` | Where each bar starts along the value axis: `{"type": "constant", "value": number}`, which is `0` by default, or `{"type": "data", "value": NodeId}` for one base per bar, which draws floating bars. |
| `slot` | The extent of the slot of each bar along the category axis: `{"type": "pitch"}` (the default), `{"type": "width", "value": number}` for one width in data units, or `{"type": "data", "value": NodeId}` for one width per bar. Every width must be a finite positive number. |
| `gap` | The fraction of each slot left empty, measured on the page and shared equally by the two sides of the bar: a finite number from 0, at which neighbouring bars touch, up to, but not including, 1. The default is 0.2. |
| `group` | The [group of bars](#groups-of-bars) to which the series belongs, `{"name": string, "layout": "grouped" or "stacked"}`, or `null` (the default) for a series drawn on its own. |
| `face` | The ColorSpec of the bars: `auto` (the default) takes the next colour of the colour order, which is the colour of the series, as the line of a series does; a fixed colour, `none` or `colormapped` may also be given. A `colormapped` face gives each bar the colour of its value of the colour data, as [colours of bars](#colours-of-bars) describes. |
| `color_data` | NodeId of the data array of the values that colour the bars through their colour scale, one value per bar, or `null` (the default), for bars of fixed colours. |
| `color_scale` | NodeId of the [colour scale](#colour-scales) through which the colour data is mapped, or `null` (the default). Bars with colour data must name one; bars without colour data map nothing and need none. |
| `edge` | The LineStyle of the outline of every bar. By default it is a solid black line 0.5 pt wide, as the edges of a surface are, so that neighbouring bars of similar colours stay distinct; a colour of `none` draws no outline. |
| `face_opacity` | The [opacity](#alpha-and-opacity) of the faces of the bars, a number from 0 to 1, which multiplies the alpha of their colours. The default is 1. |
| `edge_opacity` | The opacity of the outlines of the bars, a number from 0 to 1, which multiplies the alpha of their colours. The default is 1. |

The **pitch** is the smallest distance between neighbouring distinct finite positions, taken in order of value rather than in the order of the array, or 1 when there is a single distinct position. Each bar is drawn across the fraction `1 − gap` of its slot, centred on its position. The gap is measured on the page rather than in data units, so that bars on a logarithmic category axis remain symmetric about their positions.

On a logarithmic value axis a base that is not positive has no place, so a bar whose base is not positive is drawn from the low edge of the plot area instead, and validation warns of it with `NonPositiveOnLogAxis`. A bar series of no values is valid and draws nothing, as [ADR 0012](../adrs/0012-empty-and-singleton-data.md) decides; validation warns of it with `NothingToDraw`.

Automatic [limits](#limits) along the category axis end exactly at the drawn extent of the bars, as they end at the edges of a grid, rather than at a tick value. Along the value axis they include the bases as well as the values, so that a bar chart rising from 0 shows 0.

In Protocol Buffers a bar series is the variant `bar`, field 11 of the oneof of an `Artist`.

#### Colours of bars

Bars colour individual bars as a [series](#colour-data-of-a-series) colours individual markers, and validation applies the same rules with the same issue kinds. The colour data holds one value per bar and is mapped through the scale that the bars name, which may be of any kind: a continuous scale colours bars by a quantity, and an indexed or categorical scale colours them by category, such as the model or the state of each bar. Colour data of any other length, or of more than one dimension, is a `ShapeMismatch` error, and colour data of 8-bit values is an `ElementTypeMismatch` error, as for a series. Bars with colour data map it whenever they have it, whatever their colours, so it reaches the [automatic limits](#automatic-limits) of the scale, and bars with colour data that name no scale are the error `MissingColorScale`. A value that the scale cannot place follows its [out-of-range policy](#out-of-range-policies).

The colour of the outline of a bar takes one of four forms:

- `none` draws no outline.
- A fixed colour, black by default, draws every outline in that colour.
- `auto` draws every outline in the colour of the series, the next colour of the colour order, as an automatic marker edge follows the colour of its series.
- `colormapped` gives the outline of each bar the colour of its bar's value through the scale, so that bars without a face, or bars set apart by a gap, can be outlined in their own colours.

A `colormapped` face or outline on bars without colour data has no value to map, so it is drawn as an `auto` one would be and needs no scale, as a `colormapped` marker of a series without colour data is.

#### Groups of bars

Bar series of the same axes whose groups share a `name` form one group, which is how several series are compared at each position: three models scored on four properties are three bar series in one group, each with its own colour and its own entry in a legend. A group is formed by name within one axes only, and a series without a group is never part of one. The members of a group need not have the same positions or the same number of bars.

- **Grouped** members (`"layout": "grouped"`, the default) stand side by side: at each position, the drawn part of the slot is split equally between the members, in artist order, and the members touch one another, so that the gap falls between positions.
- **Stacked** members (`"layout": "stacked"`) build on one another: at each position, each member starts where the members before it in artist order ended. Positive and negative values stack separately, upwards and downwards from the base, as Plotly's relative mode does.

The orientation, slot, gap, base and layout of a group are those of its first member in artist order. A later member that differs from the first in any of them is drawn by the first member's properties, and validation warns of it with `InconsistentBarGroup`, naming the first member and each property that differs.

### `stairs`

A stepped outline through values held between edges or at samples (MATLAB's `stairs`, and Matplotlib's `stairs` and `step`). Along the category axis the outline runs through each value across its step, and segments along the value axis join neighbouring steps. A histogram drawn as an outline or as a filled outline is stairs on the edges of its bins.

| Property | Meaning |
| --- | --- |
| `orientation` | `"vertical"` (the default) places the edges or samples along x and the values along y; `"horizontal"` places them along y and the values along x. |
| `data` | The steps, as described below. |
| `base` | The value along the value axis to which the outline drops at both ends, which closes it, or absent (the default) for an open outline that starts and ends at the first and last values. In JSON the property is omitted when it is absent. |
| `line` | The LineStyle of the outline. An automatic colour, the default, takes the next colour of the colour order. Stairs have neither colour data nor a colour scale, so `colormapped` is the validation error `ColormapNotSupported`. |
| `fill` | The ColorSpec of the area between the outline and the base: `none` by default. A fill other than `none` needs a `base`, without which it is the validation error `StairsFillWithoutBase`, and `colormapped` is the validation error `ColormapNotSupported`. |
| `line_opacity` | The [opacity](#alpha-and-opacity) of the outline, a number from 0 to 1, which multiplies the alpha of its colour. The default is 1. |
| `fill_opacity` | The opacity of the fill, a number from 0 to 1, which multiplies the alpha of its colour. The default is 1. |

The steps take one of two forms:

- `{"type": "edges", "edges": NodeId, "values": NodeId}` holds each value between two neighbouring edges, as the counts of a histogram hold between the edges of its bins, so there is one more edge than values.
- `{"type": "samples", "x": NodeId, "values": NodeId, "step": "pre", "post" or "mid"}` holds each value at a sample, so there are as many samples as values. `step` says where the step from one value to the next occurs, as the `where` argument of Matplotlib's `step` does: `post` (the default, as MATLAB's `stairs` draws) holds each value from its own sample up to the next, `pre` holds it from the previous sample up to its own, and `mid` changes value midway between the samples.

The edges or samples must be finite and strictly monotonic, increasing or decreasing; otherwise validation reports the error `InvalidEdges`. A value that is not finite breaks the outline. On a logarithmic value axis a base that is not positive has no place, so the outline drops to the low edge of the plot area instead, and validation warns of it with `NonPositiveOnLogAxis`. Stairs with no values are valid and draw nothing; validation warns of them with `NothingToDraw`. Automatic limits along the category axis end exactly at the drawn extent of the steps, and along the value axis they include the base.

In Protocol Buffers stairs are the variant `stairs`, field 12 of the oneof of an `Artist`; the `data` message must be present.

### `cells`

A grid of rectangular cells, each coloured by its value through a [colour scale](#colour-scales) (MATLAB's `heatmap` and `histogram2` drawn as tiles, and Matplotlib's `pcolormesh` on a rectilinear grid). A cell holds one value, unlike a [surface](#surface), whose values belong to the nodes between cells, and its edges may be uneven and separated by gaps, unlike the pixels of a [scalar image](#scalar_image). Cells are therefore the artist of a two-dimensional histogram, whose counts belong to bins of any width, and of a matrix of categories, such as an occupancy matrix, whose cells are set apart by gaps. Cells are valid only in a two-dimensional axes.

| Property | Meaning |
| --- | --- |
| `values` | NodeId of the data array of the values, an array of shape `[ny, nx]` of floating-point or 8-bit values, one per cell. An 8-bit value is mapped as the whole number it denotes. Cell `(j, i)` lies between column edges `i` and `i + 1` and between row edges `j` and `j + 1`, so row 0 lies between the first two row edges. |
| `columns` | The edges of the columns along x: `{"type": "unit"}` (the default), for which column `k` spans `k − 0.5` to `k + 0.5`, so that the centres lie at 0, 1, 2 and so on, as the pixels of an image without a range do; or `{"type": "edges", "value": NodeId}`, `nx + 1` edges that are finite and strictly monotonic, increasing or decreasing. |
| `rows` | The edges of the rows along y, in the same form, with `ny + 1` explicit edges. |
| `gap` | The fractions of the width and the height of each cell left empty, `{"x": number, "y": number}`, measured on the page and shared equally by the two sides of the cell. Each is a finite number from 0 up to, but not including, 1. The default is `{"x": 0, "y": 0}`, which draws seamless cells. |
| `edge` | The LineStyle of the outline of every cell. Its colour is `none` by default, which draws no outline, with a width of 0.5 pt; a `colormapped` colour gives each outline the colour of its cell. |
| `color_scale` | NodeId of the colour scale through which the values are mapped, or `null`. Cells always map their values, so they must name one. |
| `alpha` | NodeId of the data array of the [alpha](#alpha-and-opacity) of every cell, an array of the same shape as `values` of floating-point values from 0 to 1 or 8-bit values from 0 to 255, or absent for cells whose alpha is 1, as for a scalar image. |
| `face_opacity` | The [opacity](#alpha-and-opacity) of the cells, a number from 0 to 1, which multiplies the alpha of every cell. The default is 1. |
| `edge_opacity` | The opacity of the outlines of the cells, a number from 0 to 1, which multiplies the alpha of their colour. The default is 1. |

A value that the scale cannot place, a missing value (NaN) among them, follows the scale's [out-of-range policy](#out-of-range-policies), so the padding of ragged rows is drawn as the scale's non-finite category says. Cells are placed by their edges through the scales of the axes, so they may lie along logarithmic axes; a cell bounded by an edge that is not positive along a logarithmic axis is not drawn, and validation warns of it with `NonPositiveOnLogAxis`. Unit edges start at −0.5, so unit cells on a logarithmic axis always raise this warning. Cells with no rows or no columns are valid and draw nothing; validation warns of them with `NothingToDraw`. Automatic limits end exactly at the outer edges of the cells, as they end at the edges of a grid.

In Protocol Buffers cells are the variant `cells`, field 13 of the oneof of an `Artist`.

### `image`

A true-colour image: a raster of pixels, each with its own colour (MATLAB's `image` with a true-colour array). An image is drawn as flat, uninterpolated pixels rather than as a mesh, so it is planar: it lies in one of the coordinate planes of its axes, as its [placement](#image-placement) says. An image of either kind with no rows or no columns is valid and draws nothing; [validation](#validation) warns of it. The decisions behind the image artists are recorded in [ADR 0011](../adrs/0011-image-artists.md), and the replacement of its colour-indexed and colour-mapped kinds by one [`scalar_image`](#scalar_image) in [ADR 0024](../adrs/0024-colour-scales-colourbars-and-colour-keys.md#one-image-artist-for-values).

| Property | Meaning |
| --- | --- |
| `pixels` | NodeId of the data array of the pixels, an array of shape `[ny, nx, 3]` (the red, green and blue components of every pixel) or `[ny, nx, 4]` (with an alpha component). Floating-point components lie from 0 to 1 and 8-bit components from 0 to 255. A pixel with a non-finite component is transparent. |
| `placement` | The [ImagePlacement](#image-placement) of the pixels in the axes. |
| `opacity` | The [opacity](#alpha-and-opacity) of the image, a number from 0 to 1, which multiplies the alpha of every pixel: its alpha component in an array of four components, and 1 in an array of three. The default is 1. A true-colour image has no separate alpha array, because an array of four components already gives every pixel its own alpha. |

### `scalar_image`

An image whose pixels each hold one number, mapped to a colour through a [colour scale](#colour-scales) (MATLAB's `imagesc`, and `image` with an indexed array). Whether the numbers are measurements or indices is a property of the scale, not of the image: through a continuous scale the image is MATLAB's `imagesc`, and through an indexed scale each whole number names an entry of a palette, as MATLAB's `image` with an indexed array does. A scalar image is placed and drawn as an [`image`](#image) is.

| Property | Meaning |
| --- | --- |
| `values` | NodeId of the data array of the numbers, an array of shape `[ny, nx]` of floating-point or 8-bit values. An 8-bit value is mapped as the whole number it denotes. |
| `color_scale` | NodeId of the [colour scale](#colour-scales) through which the numbers are mapped, or `null`. A scalar image always maps its numbers, so it must name one. |
| `placement` | The [ImagePlacement](#image-placement) of the pixels in the axes. |
| `alpha` | NodeId of the data array of the [alpha](#alpha-and-opacity) of every pixel, an array of the same shape as `values`, or absent for pixels whose alpha is 1. It may hold floating-point values from 0 to 1 or 8-bit values from 0 to 255, as the components of a true-colour image may. A floating-point value outside `[0, 1]` is clamped to that range, and a pixel whose alpha is not finite is transparent. The alpha also multiplies the colour of a pixel drawn by a fixed-colour [out-of-range policy](#out-of-range-policies). |
| `opacity` | The [opacity](#alpha-and-opacity) of the image, a number from 0 to 1, which multiplies the alpha of every pixel. The default is 1. |

In Protocol Buffers a scalar image is the variant `scalar_image`, field 9 of the oneof of an `Artist`.

### Image placement

An **ImagePlacement** says where the pixels of an image lie in its axes.

| Property | Meaning |
| --- | --- |
| `plane` | The ImagePlane in which the image lies, with its offset along the third axis. |
| `columns` | A PixelRange giving the coordinates of the centres of the first and last columns along the first axis of the plane, or `null` for centres at 0, 1, …, nx − 1. |
| `rows` | A PixelRange giving the coordinates of the centres of the first and last rows along the second axis of the plane, or `null` for centres at 0, 1, …, ny − 1. Row 0 of the array lies at the first centre, whichever way the range runs. |

A **PixelRange** has two properties, `first` and `last`: the coordinates of the centres of the first and last pixels along one axis of the plane. With `n` pixels along the axis, the pitch between centres is `(last − first) / (n − 1)`, and the image covers half a pitch beyond each centre, so an image of `nx` columns with a `null` column range covers −0.5 to nx − 0.5. A `last` less than `first` mirrors the image along the axis, which is how an image is flipped; [Image orientation](../guides/getting-started.md#image-orientation) gives the placements that turn a photograph or a matrix the right way up, and explains why the model has no separate orientation property. An image with one pixel along an axis has a pitch of 1 whatever its range. Both centres must be finite, and they may coincide only when the image has one pixel along the axis, as [validation](#validation) checks. The range that the property editor gives an absent range runs from 0 to 1, which places an image of any number of pixels validly.

An **ImagePlane** takes one of three forms. The columns of the image run along the first axis of the plane and its rows along the second, and the offset is the coordinate of the plane along the third axis, or `null` for the low end of that axis.

- `{"type": "xy", "z": number or null}` is the plane of the x and y axes: the floor of a three-dimensional axes, at height `z`, and the only plane a two-dimensional axes can show, where `z` is ignored. It is the default.
- `{"type": "xz", "y": number or null}` is the plane of the x and z axes, a wall of a three-dimensional axes, at `y`. It is valid only in three-dimensional axes.
- `{"type": "yz", "x": number or null}` is the plane of the y and z axes, the other wall, at `x`. It is valid only in three-dimensional axes.

In a three-dimensional axes an image whose plane lies on a face of the axes box, because its offset is `null` or equals a limit of the third axis, is painted behind every other artist of the axes when that face is at the back of the current view and in front of every other artist when it is at the front; an image at an interior offset is sorted among the other artists by its depth.

An image is a raster of evenly spaced pixels, which has no placement on a logarithmic axis, where equal steps in the data are unequal distances on the page. An image whose plane has a logarithmic axis is therefore a validation error, `ImageOnLogAxis`, whose message names the image, its plane and the logarithmic axis, and states how to draw the same values as a [`surface`](#surface) instead: a grid of the coordinates of the pixel centres, with the values as its colour data. Every node of a surface is placed through the scale of its axis, so a surface is the artist for values sampled along a logarithmic axis; [images on logarithmic axes](../guides/getting-started.md#images-on-logarithmic-axes) shows the conversion. The z axis of a two-dimensional axes is ignored. The third axis of the plane may be logarithmic, but an explicit offset along it must then be positive, because a logarithmic axis has no position for zero or a negative coordinate; a plane offset to such a coordinate is an `InvalidImagePlacement` error. The decision is recorded in [ADR 0021](../adrs/0021-images-refused-on-logarithmic-axes-and-alpha-and-opacity.md).

### Alpha and opacity

Surfaces, images and contours can be drawn partly transparent, which the model expresses in two ways that differ in their shape.

- **Alpha** has the shape of the data: it is an array that gives every node of a surface, or every pixel of an image, its own transparency, so that the transparency varies over the artist. A confidence or a mask that belongs to the data, such as the validation confidence of each vector of a particle image velocimetry measurement, is an alpha. A surface has two alpha arrays, `face_alpha` for its faces and `edge_alpha` for their edges, so that the mesh can fade independently of the faces it borders; a scalar image and [cells](#cells) have one each, `alpha`; and a true-colour image carries its alpha in the fourth component of its pixels. A contour has no alpha array, because each of its isolines and bands is drawn in a single colour, so a transparency that varied over the field would have no single value to give it.
- **Opacity** is a single number from 0 (transparent) to 1 (opaque) that applies to the whole artist: `opacity` for an image, `face_opacity` and `edge_opacity` for the faces and the edges of a surface, of [bars](#bar) and of [cells](#cells), and `line_opacity` and `fill_opacity` for the lines and the filled areas of a [contour](#contour) and of [stairs](#stairs). Its default is 1. It is the property to change when an artist as a whole should be fainter, and it can be changed in the [property editor](../guides/viewer.md#changing-a-property) without regenerating any data. It is also the compact way to make an artist uniformly translucent: a true-colour image with three components and an opacity of 0.5 says the same as one with four components whose alpha is 0.5 everywhere, at three quarters of the size.

The transparency that is drawn is the product of the alpha of the colour, the alpha array and the opacity. For a face of a surface it is the alpha of the face colour, times the face alpha of the face (the mean of its corners), times the face opacity, and likewise for its edges; for a pixel of a scalar image it is the alpha of the colour the pixel resolves to through its scale, which is 1 for a colour of a bundled colormap and the alpha of the colour otherwise, times its alpha, times the opacity; for a pixel of a true-colour image it is its alpha component times the opacity; and for an isoline or a band of a contour it is the alpha of its colour times the line or the fill opacity. An alpha value outside `[0, 1]` is clamped to that range, an 8-bit alpha is read as a fraction of 255, and a pixel or a face whose alpha is not finite is transparent, as a pixel with a non-finite component is. An artist without alpha arrays and with an opacity of 1 is drawn exactly as it was before alpha and opacity existed.

The opacity, but not the alpha, carries into the sample of an artist in its [legend](#legend), because an alpha varies over the artist and has no single value to show. In a three-dimensional axes a translucent face or image never hides what lies behind it: the viewer draws the opaque artists of the axes first and blends the translucent ones over them, in the order of their depths, and a PDF export embeds the depth-tested render of the axes wherever painting it back to front would show a different picture, as [exporting three-dimensional axes](../guides/getting-started.md#three-dimensional-axes-in-pdf) describes. [Alpha and opacity](../guides/getting-started.md#alpha-and-opacity) in the guide shows how to set both. The decisions behind alpha and opacity are recorded in [ADR 0021](../adrs/0021-images-refused-on-logarithmic-axes-and-alpha-and-opacity.md).

## Styles

### LineStyle

| Property | Meaning |
| --- | --- |
| `color` | The ColorSpec of the line. |
| `width_pt` | The line width. The default is 0.75 pt. |
| `dash` | `solid` (the default), `dashed`, `dotted`, `dash_dot`, `dash_dot_dot`, `long_dash`, or `none`, which draws no line. |

### MarkerStyle

| Property | Meaning |
| --- | --- |
| `shape` | `none` (the default for lines), `circle`, `square`, `diamond`, `triangle_up`, `triangle_down`, `plus`, `cross`, `point`, `v_line` (a vertical tick, one stroke as tall as the marker size) or `h_line` (a horizontal tick, one stroke as wide as the marker size). |
| `size_pt` | The width of the marker. The default is 4 pt. |
| `face` | The ColorSpec of the marker interior. The default is `none`. |
| `edge` | The ColorSpec of the marker outline. The default is `auto`. |

## Colours

A **Color** is an sRGB colour with straight (not premultiplied) alpha, whose four components each lie from 0 to 1. In JSON it is written as the string `"#rrggbb"` when it is opaque and `"#rrggbbaa"` otherwise, and is therefore stored with eight bits per component. In Protocol Buffers it is a message of four `float` components, which keeps the full precision of the model.

A **ColorSpec** says how a colour is chosen:

- `{"type": "auto"}` lets the renderer choose. For the line colour of a series, and the colour of a quiver, this is the next colour of the axes colour order, which is the Okabe–Ito palette without black. For a marker face or edge, it is the resolved colour of the artist. For contour isolines and surface faces and edges, which have no colour of their own, it means `colormapped`, so such an artist must name a colour scale.
- `{"type": "rgba", "color": Color}` is a fixed colour.
- `{"type": "none"}` draws nothing.
- `{"type": "colormapped"}` maps the data value to a colour through the [colour scale](#colour-scales) that the artist names.

A colormapped colour is meaningful only where the model holds a value to map and the artist names a scale to map it through: the isolines of a contour, mapped by their level; the faces and edges of a surface, mapped by its field or its colour data; a series that has colour data; the faces and outlines of [bars](#colours-of-bars) that have colour data, mapped by the value of each bar; and the outlines of [cells](#cells), mapped by the value of their cell. Stairs take solid colours only and have neither colour data nor a colour scale, so a colormapped colour on them is the validation error `ColormapNotSupported`. A quiver and a series without colour data hold no such value, so a colormapped colour there is drawn as the middle colour of viridis; a marker takes the resolved colour of the artist it belongs to, so a colormapped marker face or edge is drawn exactly as an automatic one, and so is a colormapped face or outline of bars without colour data. The property editor lists the choice everywhere and shows it disabled, with the reason, where it has no meaning, as [using the viewer](../guides/viewer.md#what-the-editor-does-not-change) describes.

## Text

A **Text** is stored as its source, never as typeset glyphs, so that it can be edited and so that the renderer resolves it when it is drawn.

| Property | Meaning |
| --- | --- |
| `content` | The source text. |
| `interpreter` | `"latex"` (the default) treats segments delimited by `$…$` as LaTeX mathematics and the rest as plain text, with `\$` producing a literal dollar sign and `\texttt{…}` setting its argument in monospace, as [titles, labels and LaTeX](../guides/getting-started.md#titles-labels-and-latex) describes. `"none"` renders the content literally, dollar signs included. |

Mathematics that the typesetter cannot handle is drawn as its raw source and reported as a warning; it never makes a figure invalid. How text is resolved is described in the [architecture reference](architecture.md#text-resolution) and [ADR 0017](../adrs/0017-latex-rust-applies-tex-conventions.md).

## Data arrays

A **data array** is a node of the figure that holds one numeric array. Artists refer to their data by the NodeId of a data array rather than containing it, so several artists, of any axes and in any role, can share one array, which is then stored once. Editing a shared array changes every artist that refers to it. The decision is recorded in [ADR 0022](../adrs/0022-figure-nodes-in-one-identifier-space-and-panels.md), and [sharing data between plots](../guides/getting-started.md#sharing-data-between-plots) shows how a program shares an array.

| Property | Type | Meaning |
| --- | --- | --- |
| `id` | NodeId | The identifier of the data array. |
| `name` | string or `null` | The name that identifies the array to tools such as the viewer's property editor, or `null` when the array has none. The name is never drawn, and it constrains nothing: two arrays may share a name, and a name may be empty. |
| `array` | NdArray | The values of the array. |

The values of a data array change only through the `replace_data` and `append_data` [edits](#edits). The property editor lists the data arrays of a figure, named by their names or, for an array without one, by its shape, but changes neither their names nor their values, because the data comes from the program that wrote the figure. A data array that no artist refers to is valid, and [validation](#validation) reports it with the warning `UnusedData`, because it costs space in every saved file.

### NdArray

The values of a data array are an **NdArray**:

| Property | Meaning |
| --- | --- |
| `shape` | The length of each dimension, outermost first. A vector of `n` values has shape `[n]`; a field with `ny` rows and `nx` columns has shape `[ny, nx]`. |
| `element` | The type of the values: `"f64"` (the default) for 64-bit floating-point values, or `"u8"` for 8-bit unsigned integers. |
| `values` | The values in row-major order: the value in row `j` and column `i` of a two-dimensional array is `values[j * nx + i]`. |

The two image artists accept arrays of either element type: the pixels of an [`image`](#image) and the values of a [`scalar_image`](#scalar_image) may be floating-point or 8-bit values, and 8-bit values hold pixels compactly. Every other artist requires floating-point values, so any other artist that refers to an array of 8-bit values is an error, as [validation](#validation) describes; an array that no artist refers to may be of either type. A missing floating-point value is NaN; an 8-bit value has no missing value. The number of values must equal the product of the shape.

Protocol Buffers stores every floating-point value, including NaN and infinities, as an IEEE 754 double, and every 8-bit value as one byte, in the payload named by the element. JSON cannot represent non-finite numbers, so every non-finite floating-point value (NaN or an infinity) is written as `null` and read back as NaN; 8-bit values are written as integers under an `element` of `"u8"`, and floating-point values are written without an `element`, exactly as they were before the element existed.

Rows of a field correspond to y and columns to x, as in MATLAB: the value in row `j` and column `i` belongs to the point `(x[i], y[j])`.

## Grids

A **Grid** describes where the nodes of a field of shape `[ny, nx]` lie.

- `{"type": "rectilinear", "x": NodeId, "y": NodeId}` is an axis-aligned grid: `x` holds `nx` column coordinates and `y` holds `ny` row coordinates.
- `{"type": "curvilinear", "x": NodeId, "y": NodeId}` is a structured grid in which every node has its own coordinates: `x` and `y` are both arrays of shape `[ny, nx]`.

## Links

An **AxisLink** is a group of axes whose limits along one dimension are kept equal.

| Property | Meaning |
| --- | --- |
| `dimension` | `"x"`, `"y"` or `"z"`. |
| `axes` | The NodeIds of the linked axes. |

A link shares limits only: the direction, the ticks and the tick labels of each member are its own, so the axes of a group may differ in direction, and reversing one member reverses none of the others. For each dimension, groups are disjoint: an axes belongs to at most one group per dimension. Linking axes that already belong to groups merges those groups into one, which is a union-find operation. Setting the limits of any member, through the API or through the viewer, sets the limits of every member, and automatic limits are computed over the data of the whole group. A member that cannot show those limits, such as a logarithmic axis given a range that reaches zero, refuses the whole change, so axes that cannot share limits cannot be linked. The behaviour in the viewer is described in [using the viewer](../guides/viewer.md#linked-axes), and the design in [ADR 0008](../adrs/0008-typed-edits-and-a-view-overlay.md).

## Provenance

A **Provenance** records the software that wrote a figure, so that a figure that renders differently in a later version can be diagnosed.

| Property | Meaning |
| --- | --- |
| `ironlab_version` | The version of IronLAB that wrote the figure. |
| `typesetter` | The name of the mathematics typesetter, such as `"latex-rust"`. |
| `typesetter_version` | The version of the mathematics typesetter, such as `"2.1.1"`. |
| `fonts` | The names of the fonts used for text. |

The typesetter's name and version are separate fields, so that a new version of the same typesetter is not mistaken for a different typesetter ([ADR 0017](../adrs/0017-latex-rust-applies-tex-conventions.md)). The PDF exporter copies the provenance into the document metadata.

## Parameters

A **Parameter** is a named value that describes a figure, such as the Reynolds number of the flow that it shows or the solver that produced its data. Parameters do not affect drawing; they are stored with the figure so that collections of figures can be sorted, filtered and searched. The `parameters` of a figure map each name to one parameter, so a name occurs at most once, and they are kept in ascending order of name.

A parameter takes one of four forms:

| JSON | Protocol Buffers variant | Value |
| --- | --- | --- |
| `{"type": "bool", "value": true}` | `bool_value` (ParameterBool) | A boolean. |
| `{"type": "integer", "value": -4096}` | `integer_value` (ParameterInteger) | A signed 64-bit integer. |
| `{"type": "number", "value": 100000.0}` | `number_value` (ParameterNumber) | A double-precision floating-point number, which must be finite. |
| `{"type": "string", "value": "k–ω SST"}` | `string_value` (ParameterString) | A string, which may be empty. |

Parameters are edited in the viewer's [property editor](../guides/viewer.md#parameters) as well as through the API, and are what its [figure browser](../guides/viewer.md#the-figure-browser) filters, orders and groups a collection of figures by.

The form is stated explicitly because JSON has a single number type: without it, the number `3.0` would reload as the integer `3`. A JavaScript program reads a JSON integer as a double, which holds integers exactly only up to 2<sup>53</sup> in magnitude, so a larger integer parameter is exact only in readers that parse JSON integers as 64-bit integers. A parameter name must not be empty; it may contain any other Unicode text, and names that differ only in case are distinct.

```json
"parameters": {
  "converged": { "type": "bool", "value": true },
  "reynolds_number": { "type": "number", "value": 100000.0 },
  "solver": { "type": "string", "value": "k–ω SST" }
}
```

## Tags

A **tag** is a free word that describes a figure, such as `surface` or `piv`. Tags are what parameters cannot be: a figure carries any number of them, and none of them is a name with a value under it. Like parameters, they are never drawn; they are stored with the figure so that collections of figures can be filtered and grouped by them. They were formerly called labels, and were renamed so that a label is always text that is drawn, as [ADR 0023](../adrs/0023-titles-labels-names-and-tags.md) records.

The `tags` of a figure are an array of strings, kept in the order they were given, because that order is part of the figure's value and is the order the viewer lists them in. It is the one collection in the schema that is not sorted on writing.

A tag must not be empty, and no tag may occur twice in one figure. Tags are compared exactly, so `Surface` and `surface` are two distinct tags and neither is a repeat of the other. Both rules are reported by [validation](#validation) as the error `InvalidTag`, and the JSON Schema states them as well, as `minLength` on the items of the array and `uniqueItems` on the array itself, so a program generating figures from the schema is stopped before it writes one. The Protocol Buffers schema cannot express either rule, so a file in that encoding may carry an empty or repeated tag; the viewer reports it and shows the tags once each, rather than altering the file.

```json
"tags": ["surface", "3d", "piv"]
```

Tags are edited in the viewer's [property editor](../guides/viewer.md#tags) as well as through the API, and are what the [figure browser](../guides/viewer.md#the-figure-browser) filters and groups a collection of figures by.

## Edits

A change to a figure is an **edit**, and edits are applied in a **transaction**, which applies all of its edits in order or none of them. A transaction is the Protocol Buffers message `Transaction` of `ironlab/ir/v0/edit.proto`, and a JSON object of the same structure whose `edits` are objects with a `type` that names the kind of edit. The viewer records every gesture and every change in its property editor as a transaction, and the reasons for the design are recorded in [ADR 0008](../adrs/0008-typed-edits-and-a-view-overlay.md) and, for data arrays and panels, in [ADR 0022](../adrs/0022-figure-nodes-in-one-identifier-space-and-panels.md#data-arrays-and-panels-change-through-the-structural-edits).

| Edit | Fields | Effect |
| --- | --- | --- |
| `set` | `node`, `path`, `value` | Replaces the value of one property of a node, addressed by a path of field names that starts at the fields of the node, such as `x.limits` of an axes, `line.width_pt` of a series or `content.legend.layout.columns` of a panel. |
| `insert` | `parent`, `index`, `node` | Inserts a node with its subtree: a data array, a colour scale, an axes or a panel into the figure, which is then the parent, or an artist into an axes. `index` is the position in the parent's list of nodes of that kind, or `null` to append. The identifiers of the node and of its descendants must not be in use. |
| `remove` | `node` | Removes a data array, a colour scale, an axes, an artist or a panel, with its subtree. |
| `move` | `node`, `parent`, `index` | Moves a data array, a colour scale, an axes or a panel within the figure's list of its kind, or an artist within its axes or to another axes. |
| `replace_data` | `node`, `array` | Replaces the values of an existing data array, which keeps its identifier and its name, so that every artist that refers to it draws the new values. It never creates a data array; `insert` does. |
| `append_data` | `node`, `array`, `retain` | Appends entries to an existing data array along its first dimension. The entries must have the element type of the array and its shape after the first dimension. When `retain` is set, only the last `retain` entries are kept, which gives a rolling window for streamed data. |

The identifiers, the schema version, the provenance, the lists of nodes and the values of a data array cannot be set with `set`: nodes change only through `insert`, `remove` and `move`, and values only through `replace_data` and `append_data`.

A path segment of decimal digits names one **entry of a palette** by its index, after the segment that names the list, and it is the only place in a path where a numeric segment appears:

| Path | Value | What it names |
| --- | --- | --- |
| `kind.indexed.colors.<i>` | Color | Entry `i` of an [indexed scale](#colour-scale-kinds); its components are `kind.indexed.colors.<i>.r` and so on. |
| `kind.categorical.categories.<i>.color` | Color | The colour of category `i` of a categorical scale. |
| `kind.categorical.categories.<i>.label` | Text | The label of category `i`, with `.content` and `.interpreter` below it. |
| `kind.continuous.colormap.colors.<i>`, `kind.discrete.colormap.colors.<i>` | Color | Colour `i` of a [listed colormap](#colormaps); a named colormap has no such colours, and a path to one is the error `InactiveVariant`. |

The index is written in canonical decimal form, without a sign, a leading zero or an exponent, so that one entry has exactly one path, which clients compare as text; any other spelling, such as `01` or `+1`, is the error `UnknownPath`. An index past the last entry of the list is the error `EntryOutOfRange`, which names the node, the path and the number of entries, so that a client can tell a palette that shrank from a misspelt path. The list itself (`kind.indexed.colors`, `kind.categorical.categories` and `….colormap.colors`) and a category as a whole are read-only, because how many entries a palette has, and in which order, comes from the program that builds the figure. The registry of the settable properties of a kind of node lists no entry path, because which entries there are depends on the node; the properties of one node as it stands, its entries included, are listed by `properties_of`. An edit that names no node of the figure is refused with the error `UnknownNode`, or `UnknownData` for a data edit, and a data edit that names a node other than a data array, such as an axes, is refused with `WrongNodeKind`. After its edits, a transaction is [validated](#validation), and it is rolled back when validation reports an error that the figure did not have before; warnings never reject a transaction. Applying a transaction returns its inverse, which restores the figure exactly.

## Validation

The encodings describe the structure of a figure but cannot express every rule. `Figure::validate` checks the rest and returns errors, which prevent a figure from being exported or shown, and warnings, which do not.

Errors are reported for a reference to data that names no node of the figure (`UnknownData`), a reference that names a node of the wrong kind (`WrongNodeKind`), namely an artist's data that names a node other than a data array, an artist's colour scale or the scale of a colourbar, in a panel or inside an axes, that names a node other than a colour scale, a legend that names a node other than a panel holding a legend, an item of an explicit list of legend entries that lists an artist by a node other than an artist, and an item that shows a colour scale by a node other than a colour scale, an artist that maps numbers to colours but names no colour scale (`MissingColorScale`), an artist, a colourbar or a legend item whose colour scale names no node of the figure (`UnknownColorScale`, reported against the artist, against the panel or axes of the colourbar, or against the owner of the legend: the panel of a legend panel, or the axes of an inside legend), an inside colourbar, or a panel colourbar of stated length, whose length is not a finite number greater than 0 and at most 1 (`InvalidColorbarLength`, reported against the axes or the panel), a colour-scale item of a legend whose `rows` is 0 (`InvalidLegendRows`, reported against the owner of the legend), an indexed scale with no entries or a categorical scale with no categories (`InvalidColorScale`), a listed colormap of fewer than 2 or more than 1024 colours (`InvalidColormap`), manual limits of a colour scale that are not finite and increasing, or not positive under a logarithmic norm (`InvalidLimits`), band edges of a discrete scale that break the rules of [band edges](#band-edges) (`InvalidLevels`), explicit ticks or tick labels of a colour scale that break the rules of an axis (`InvalidTicks` and `InvalidTickLabels`), a number mapped through a colour scale that falls in a category whose [out-of-range policy](#out-of-range-policies) is strict (`ValueOutOfRange`), an array whose number of values does not match its shape, an artist other than an image or cells that refers to an array of 8-bit values (every artist other than the two image kinds requires floating-point values, the alpha of a surface among them, and cells accept 8-bit values only in their values and alpha), arrays of one artist with inconsistent lengths or shapes (for a series, size data whose length differs from the number of points, or colour data of neither one value per point nor one value per segment, as [colour data of a series](#colour-data-of-a-series) describes; for an image, pixels that are not an array of shape `[ny, nx, 3]` or `[ny, nx, 4]`, or values that are not two-dimensional; for a surface, colour data or alpha whose shape differs from the shape of `z`; for a scalar image, alpha whose shape differs from the shape of its values), an opacity that is not a finite number from 0 to 1 (`InvalidOpacity`), an artist or placement that needs the z axis in a two-dimensional axes (z or w data, contours at their level, or an image on the xz or yz plane; a surface is not among them, because a two-dimensional axes shows it from directly above), a series in a three-dimensional axes with a dimension in [axes-fraction coordinates](#coordinates-and-clipping), a link to an identifier that is not an axes, two nodes with the same identifier, a cell of an axes or a panel outside the tile layout or with a zero span, more row or column sizes than rows or columns or a weight that is not a finite positive number (`InvalidLayoutWeights`), an axes that lies in fitted rows alone or in fitted columns alone (`AxesInFittedTrack`), a legend that names no node as its panel (`DanglingPanelLegend`), an item of an explicit list of legend entries that names no artist of the figure, or, in a legend inside a plot area, an artist of another axes (`DanglingLegendItem`), a legend with a fixed number of columns of zero, a non-positive figure size or font size, invalid manual limits, a tick step that is not a finite positive number or a tick origin that is not finite (`InvalidTicks`), an explicit tick value that is not finite or, on a logarithmic axis, not positive (`InvalidTicks`), explicit tick labels whose ticks are not explicit values of the same number (`InvalidTickLabels`), an aspect ratio of y or of z that is not a finite positive number (`InvalidAspect`), empty, non-finite or non-increasing contour levels, a parameter with an empty name or a non-finite number, a tag that is empty or repeated (`InvalidTag`), an image placement whose pixel centres or plane offset are not finite or whose first and last centres coincide along an axis of more than one pixel, an image whose plane has a logarithmic axis (`ImageOnLogAxis`), and an image in a three-dimensional axes whose plane is offset to a coordinate that is not positive along a logarithmic third axis (`InvalidImagePlacement`). A problem of a colour scale itself is reported against the scale, whether or not an artist names it, because a colourbar may show a scale alone; a missing, unknown or refused scale reference and a refused value are reported against the artist, or against the panel or axes of a colourbar. Warnings are reported for an artist that is left out of the drawing because it draws with less than its data holds: finite non-positive data plotted along a logarithmic axis, which is not drawn; a series whose released `clip` has no effect, as [coordinates and clipping](#coordinates-and-clipping) describes, reported as `IneffectiveClip`; and an artist whose data gives it nothing to draw, namely a series or quiver of no points, an image of either kind with no rows or no columns, and a contour or surface whose field has no rows or no columns or a single row or a single column, between whose nodes there is no cell. The same warning, `NothingToDraw`, is reported for a [legend panel](#legend-panels) that has no entries to list: a panel with automatic entries that no axes names in its legend, or whose axes have no artist with a label, and a panel with an explicit list of no items. A panel with an explicit list of items is never empty, whether or not an axes names it. The warning `DuplicateLegendItem` is reported for an item of an explicit list whose content repeats that of an earlier item, because the legend then lists it twice, a colour scale listed twice included, whatever rows each item asks for, and the warning `UnnamedLegendItem` for an item that names an artist without a label, which is listed with its sample alone; an item that shows a colour scale is never unnamed, because the scale draws its own labels. Removing a colour scale that a legend item shows is refused as removing a scale that an artist or a colourbar names is, unless the legend goes in the same transaction. The warning `UnusedData` is reported for a [data array](#data-arrays) that no artist refers to; a hidden artist still refers to its data. For an artist, `NothingToDraw` names the artist and states which case applies and the shape of the array; it is reported only for an artist that is reported for no error, so that an error stands alone. The warning `OverlappingCells` is reported when the cells of two nodes of the [tile layout](#tile-layout), axes or panels, share at least one tile, so that one is drawn over the other. It is reported once for each such pair, against the later of the two nodes, taking the axes before the panels and each in the order the figure lists them, and its message names both nodes. A cell outside the tile layout, which is an error, is not compared with any other. Empty and singleton data are valid in every artist, so a figure built before its data arrives, or streamed from nothing, is valid at every step; the reasons are recorded in [ADR 0012](../adrs/0012-empty-and-singleton-data.md).

[Bars](#bar), [stairs](#stairs) and [cells](#cells) add their own checks. Errors are reported for any of them in a three-dimensional axes (`TwoDArtistInThreeDAxes`); for a gap of bars, or a gap of cells along x or y, that is not a finite number from 0 up to, but not including, 1 (`InvalidGap`); for a fixed slot width of bars, or a width given per bar, that is not a finite positive number (`InvalidBarSlot`); for edges of cells, or edges or samples of stairs, that are not finite and strictly monotonic (`InvalidEdges`); for a colormapped line or fill of stairs (`ColormapNotSupported`); and for stairs filled without a base (`StairsFillWithoutBase`). Their arrays must be one-dimensional with one entry per bar, the colour data of bars included when there is any, one more edge than values, or one sample per value, and the values of cells must be two-dimensional with one more column edge than columns, one more row edge than rows, and alpha of their shape; any other shape is a `ShapeMismatch` error. Only the values and the alpha of cells may hold 8-bit values. The colour data and colour scale of bars are checked by the rules of a series, as [colours of bars](#colours-of-bars) describes, and every opacity of bars, stairs and cells must be a number from 0 to 1 (`InvalidOpacity`). The warning `InconsistentBarGroup` is reported against a member of a [group of bars](#groups-of-bars) that differs from the first member in orientation, slot, gap, base or layout, and `NonPositiveOnLogAxis` against bars or stairs whose base, or cells whose edges, are not positive on a logarithmic axis. Bars and stairs of no values, and cells with no rows or no columns, raise `NothingToDraw`. A tick label angle of an [axis](#axis) that is drawn must be finite (`InvalidTickLabelAngle`, reported against the axes), and an angle other than 0 in a three-dimensional axes raises the warning `TickLabelAngleIgnored`; the z axis of a two-dimensional axes is ignored, as its limits and ticks are.

The issues of each node are reported in the order of the node tree: the figure, then its data arrays, then its colour scales, then each axes followed by its artists, and then its panels. The checks that compare several nodes, namely duplicate identifiers, overlapping cells and unused data arrays, are reported after them.

## A minimal JSON file

The following JSON file describes one two-dimensional axes with a series drawn as a line through three points, with a marker at each. The same figure saved as a `.fig` file holds the same fields in the Protocol Buffers encoding.

```json
{
  "schema_version": "0.7.0",
  "id": 0,
  "title": null,
  "size": { "width_mm": 80.0, "height_mm": 60.0 },
  "font_set": "stix_two",
  "font_size_pt": 9.0,
  "background": "#ffffff",
  "layout": { "rows": 1, "cols": 1 },
  "data": [
    { "id": 1, "name": null, "array": { "shape": [3], "values": [0.0, 1.0, 2.0] } },
    { "id": 2, "name": null, "array": { "shape": [3], "values": [0.0, 1.0, null] } }
  ],
  "axes": [
    {
      "id": 3,
      "cell": { "row": 0, "col": 0, "row_span": 1, "col_span": 1 },
      "projection": { "type": "two_d" },
      "title": null,
      "title_align": "center",
      "x": { "label": { "content": "$t$", "interpreter": "latex" }, "scale": "linear", "limits": { "type": "auto" }, "grid": false },
      "y": { "label": null, "scale": "linear", "limits": { "type": "manual", "min": -1.0, "max": 2.0 }, "grid": false },
      "z": { "label": null, "scale": "linear", "limits": { "type": "auto" }, "grid": false },
      "aspect": { "type": "auto" },
      "aspect_adjust": "box",
      "box": true,
      "legend": null,
      "artists": [
        {
          "type": "series",
          "id": 4,
          "label": null,
          "visible": true,
          "x": 1,
          "y": 2,
          "z": null,
          "line": { "color": { "type": "auto" }, "width_pt": 0.75, "dash": "solid" },
          "marker": { "shape": "circle", "size_pt": 4.0, "face": { "type": "none" }, "edge": { "type": "auto" } },
          "size_data": null,
          "color_data": null,
          "color_scale": null,
          "x_coords": "data",
          "y_coords": "data",
          "clip": true
        }
      ]
    }
  ],
  "links": [],
  "provenance": {
    "ironlab_version": "0.2.0",
    "typesetter": "latex-rust",
    "typesetter_version": "1.0.2",
    "fonts": ["STIX Two Text", "STIX Two Math"]
  }
}
```

The third y value is `null`, so it is missing: the line ends at the second point, and a marker is drawn only at the first two. The figure has no colour scales, parameters, tags or panels, and its tile layout has no weights, so the file has none of those properties.

## Versioning

The schema version has the form `major.minor.patch`, and the version implemented by the current build is `0.7.0`. It is the `schema_version` property in JSON and field 1 of the `Figure` message in Protocol Buffers, and it is checked before the rest of a file is read. A file loads when its major and minor components equal those of the build; the patch component may differ. Fields that a build does not recognise are ignored, so a file written by a later patch release of the same minor version still loads. A file with a different major or minor version is rejected with an error that names both versions, rather than being reported as malformed.

The version applies to both encodings. The Protocol Buffers package name carries only the major version (`v0`). `buf breaking` in CI reports any change to the generated `.proto` files that is incompatible with the files generated from the `main` branch, unless a commit of the pull request declares a breaking change with a `BREAKING-CHANGE` paragraph, as the [commit conventions](../conventions/git-commits-and-versioning.md) require for any breaking change. Any incompatible change that no commit declares is reported as a mistake. A declared breaking change of the schema also requires a new major version, or, while the major version is 0, a new minor version, so that readers reject files of the other version before decoding them.

Changes to the schema are versioned as follows.

- A **patch** version is for changes that every reader of the same minor version can ignore safely, such as a new optional property whose absence preserves the previous drawing.
- A **minor** version is for additions that older readers cannot draw correctly, such as a new artist variant, a new projection, or a new variant of any tagged entity.
- A **major** version is for changes that alter or remove the meaning of existing properties.

While the major version is 0, the project may make breaking changes in a minor version, and a file of a different minor version is rejected in either case.

The schema has had the following versions:

- **0.7.0** made [data arrays](#data-arrays) nodes of the figure, each with a NodeId and an optional `name`, so that data arrays, colour scales, axes, artists and panels share one identifier space and the separate `DataId` is removed; replaced the legend panels (`panel_legends`) with [panels](#panels) (`panels`) whose [content](#panel-content) is a legend or a colourbar; replaced the edits `put_data` and `remove_data` with [`replace_data`](#edits), so that data arrays are created and removed with `insert` and `remove` like every other node; and renamed the `display_name` of an [artist](#artists) to `label` and the `labels` of a figure to [`tags`](#tags). It added [colour scales](#colour-scales) as nodes of the figure (`color_scales`), of the four [kinds](#colour-scale-kinds) continuous, discrete, indexed and categorical, each with its [out-of-range policies](#out-of-range-policies); replaced `indexed_image` and `mapped_image` with [`scalar_image`](#scalar_image); removed the `colormap` and `clim` of an [axes](#axes); gave every artist that maps numbers to colours a `color_scale`; and made the levels of a [contour](#contour) optional, so that a contour without levels takes them from its scale. It replaced `line` and `scatter` with [`series`](#series), which may be sized and coloured by data, as [colour data of a series](#colour-data-of-a-series) describes. It added [colourbar panels](#colourbar-panels), the [inside colourbar](#inside-colourbar) of an axes, and [legend items](#legend-entries) that show a colour scale over a set number of rows, and gave every [panel](#panels) an optional title. A file of version 0.6 is rejected.
- **0.6.0** added the [alpha and opacity](#alpha-and-opacity) of the image, surface and contour artists: `opacity` on every image kind, `alpha` on `indexed_image` and `mapped_image` (since replaced by [`scalar_image`](#scalar_image)), `face_alpha`, `edge_alpha`, `face_opacity` and `edge_opacity` on [`surface`](#surface), and `line_opacity` and `fill_opacity` on [`contour`](#contour). A reader of version 0.5 would draw such artists opaque, so a file of version 0.5 is rejected.
- **0.5.0** added the direction (`reverse`), the [ticks](#ticks) and the [tick labels](#tick-labels) of an axis, the [coordinates and clipping](#coordinates-and-clipping) (`x_coords`, `y_coords`, `clip`) of lines and scatters, the markers `v_line` and `h_line`, the dash patterns `dash_dot_dot` and `long_dash`, the [aspect](#aspect) (`aspect`, `aspect_adjust`) and the title alignment (`title_align`) of an axes, the [explicit entries](#legend-entries) (`entries`) of a legend, and fitted tracks in the row and column sizes of the [tile layout](#tile-layout). A file of version 0.4 is rejected.
- **0.4.0** added [legend panels](#legend-panels) (`panel_legends`), changed the [legend](#legend) of an axes from a single object into a choice between a legend inside the plot area and a legend panel, added the [layout of legend entries](#legend-entry-layout) in columns and the legend location `center`, and added the row and column weights of the [tile layout](#tile-layout). A file of version 0.3 is rejected.
- **0.3.1** added the labels of a figure, which are now its [tags](#tags). It is a patch version because a reader of version 0.3.0 that ignores them still draws the figure correctly, so files are exchanged in both directions between the two.
- **0.3.0** added the image artists [`image`](#image), `indexed_image` and `mapped_image` (since replaced by [`scalar_image`](#scalar_image)), with their [placement](#image-placement) and out-of-range policies (since moved to the [colour scale](#out-of-range-policies)), and the element type of a [data array](#data-arrays), which lets an array hold 8-bit values (`element` in JSON, and `element` with the `u8_values` payload in Protocol Buffers). A file of version 0.2 is rejected.
- **0.2.0** added the [parameters](#parameters) of a figure, and replaced the `pan` array of a View3d with the properties `pan_x` and `pan_y`, so that JSON uses the same names as Protocol Buffers. A file of version 0.1 is rejected.
- **0.1.0** was the first version.

## Extending the model with new plot types

The model is designed so that further MATLAB plot types are added without restructuring it. Before any new plot type or entity is implemented, its data structure and options are defined in `ironlab-ir` and reviewed, as the project rules require.

- **Most plot types are new artist variants.** Stem, area and error-bar plots, patches and streamlines each become a new variant of Artist: a new `type` in JSON and a new variant of the `kind` oneof in Protocol Buffers. Each variant has the three common properties, refers to its data by the NodeIds of data arrays, reuses LineStyle, MarkerStyle, ColorSpec and Grid where they apply, and states its array-shape rules in validation. The [image artists](#image) were added in this way; a [`scalar_image`](#scalar_image) maps its numbers through a [colour scale](#colour-scales), as surfaces do, and adds only what a raster needs beyond that, namely a placement. [Bars](#bar), [stairs](#stairs) and [cells](#cells) were added in the same way, and share an orientation and the measurement of gaps on the page.
- **Statistical charts are made by the facade from existing artists.** A pseudocolour plot (`pcolor`) needed no new variant, because it is a [surface](#surface) in a two-dimensional axes. A histogram needs none either: the facade bins the samples and stores the result as [bars](#bar) or [stairs](#stairs), and a two-dimensional histogram as [cells](#cells), so the samples are not part of the figure and the viewer cannot rebin them.
- **New coordinate systems are new projections.** Polar and geographic axes become new variants of Projection, each with its own view properties, alongside `two_d` and `three_d`.
- **New keys are new kinds of panel content.** A key that occupies tiles of the layout is a new variant of [panel content](#panel-content) rather than a new kind of node, so it is placed, fitted and checked as every panel is. Colourbars were added in this way, as [panel content](#colourbar-panels) and as a [bar drawn inside the plot area](#inside-colourbar) of an axes, each showing a [colour scale](#colour-scales), as [ADR 0024](../adrs/0024-colour-scales-colourbars-and-colour-keys.md#a-scale-is-shown-by-a-colourbar-or-a-legend-item) records.
- **Annotations are nodes.** Pinned data tips and other annotations become nodes with their own identifiers, anchored to the artist and data index they describe, so that they are saved with the figure and exported like any other node.

Each such addition is a new variant of a tagged entity and therefore increments the minor version of the schema, as described in [versioning](#versioning). The scene compiler then learns to draw the new variant; neither backend changes, as explained in the [architecture reference](architecture.md#one-path-to-pixels).
