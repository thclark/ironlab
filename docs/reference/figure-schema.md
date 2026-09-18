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

`Figure::save` and `Figure::load` in the `ironlab` crate, and the `ironlab-viewer` binary, choose the encoding from the file extension, as described in [getting started](../guides/getting-started.md#saving-and-loading).

The entity sections below name each property as it appears in both encodings, and show tagged variants, enumerations and colours in their JSON form. The Protocol Buffers form of each follows from the conventions below.

### Protocol Buffers encoding

The Protocol Buffers messages are in package `ironlab.ir.v0`, with one `.proto` file per module of `ironlab-ir` at `ironlab/ir/v0/<module>.proto`; the root message is `Figure` in `ironlab/ir/v0/figure.proto`. The encoding follows these conventions.

- **Enumerations.** Every enum has the zero value `<ENUM>_UNSPECIFIED`, and every value is prefixed with the name of the enum in upper snake case. For example, the scale `log` is `SCALE_LOG`, and the legend location `north_east` is `LEGEND_LOCATION_NORTH_EAST`.
- **Presence.** Singular numeric and boolean fields are declared `optional`, so that a value that was written is distinguished from an absent one, and every value, including negative zero, reloads bit for bit.
- **Tagged variants.** An entity that takes one of several forms is a message with a single oneof named `kind`. Each variant of the oneof is a message of its own, such as `LimitsManual` with the fields `min` and `max`, so that a variant can gain fields in a later version without changing the others. A variant that holds a single value is also a message, such as `ParameterNumber` with the field `value`.
- **Numeric arrays.** An array is a `repeated uint64 shape` and a packed `repeated double values`. NaN and infinities are stored natively as IEEE 754 values.
- **Data table.** The `data` of a figure is a `map<uint64, NdArray>` keyed by DataId, written in ascending order of key.
- **Parameters.** The `parameters` of a figure are a `map<string, Parameter>` keyed by name, written in ascending order of the UTF-8 bytes of the name, so that a figure always encodes to the same bytes.
- **Colours.** A Color is a message of four `float` components, `r`, `g`, `b` and `a`, each from 0 to 1, stored at the full precision of the model.
- **Absent values.** An absent message, an absent field with presence, an unset oneof and an `_UNSPECIFIED` enum value take the default of their context in the model. For example, an absent line colour of a contour is colormapped, whereas an absent line colour of a line is automatic. The encoder writes every field that has a value, so a file written by IronLAB never relies on these defaults.
- **Required values.** A value that has no default in the model, namely the kind of an artist, the dimension of an axis link, and the kind of a parameter together with its value when it is a boolean, an integer or a number, makes decoding fail with an error when it is absent or unspecified.
- **Fields without presence.** Strings, repeated fields, maps and colour components cannot be distinguished from their empty or zero values, so they decode as the value on the wire: an empty string is empty and an empty list is empty.
- **Unknown values.** A field that the build does not know is skipped, so a file written by a later patch release still loads. An enum value that the build does not define makes decoding fail with an error rather than taking a default, because new enum values require a new minor version, and a file of another minor version is rejected before it is decoded.
- **Versioning.** Field 1 of `Figure` is `schema_version` in every version of the schema, so that a reader can check the version of a file before decoding the rest of it, as described in [versioning](#versioning).

Compatibility of the Protocol Buffers definition is checked in CI: `buf lint` checks the generated files, and `buf breaking` compares them with the files generated from the `main` branch.

### JSON encoding

The JSON encoding follows these conventions.

- **Tagged variants.** An entity that takes one of several forms (such as an artist, a projection or limits) is a JSON object whose `type` property names the form in `snake_case`, alongside that form's own properties. For example, manual limits are `{"type": "manual", "min": 0, "max": 1}`. A [parameter](#parameters), whose forms are single values, holds its value in a `value` property, as in `{"type": "number", "value": 100000.0}`.
- **Enumerations.** An entity that is only a choice of name (such as a scale or a colormap) is a `snake_case` string, for example `"log"` or `"north_east"`.
- **Optional properties.** A property that may be absent is written as `null` when it has no value. The one exception is the `parameters` of a figure, which are omitted when the figure has none.
- **Data table.** The `data` of a figure is an object keyed by DataId written as a decimal string.
- **Non-finite numbers.** JSON cannot represent NaN or an infinity. In a data array, every non-finite value is written as `null` and read back as NaN. A non-finite value in any other numeric field cannot be represented, so a figure saved as JSON must have finite limits, sizes, view angles and number parameters, as [validation](#validation) requires.
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
- **Identifiers.** Nodes (the figure, its axes and their artists) are identified by a `NodeId`, and arrays by a `DataId`. Both are unsigned 64-bit integers.

## Figure

The figure is the root of the model: a page of a fixed physical size holding axes, the data they plot and the links between them.

| Property | Type | Meaning |
| --- | --- | --- |
| `schema_version` | string | The version of the schema the figure conforms to, such as `"0.2.0"`; see [versioning](#versioning). |
| `id` | NodeId | The identifier of the figure. |
| `title` | Text or `null` | The title drawn above all axes (MATLAB's `sgtitle`). |
| `size` | object | `width_mm` and `height_mm`, the physical size of the figure. The default is 160 mm by 100 mm. |
| `font_set` | string | The font set used for all text. The only font set is `"stix_two"`: STIX Two Text for text and STIX Two Math for mathematics. |
| `font_size_pt` | number | The base font size, from which titles and tick labels are scaled. The default is 9 pt. |
| `background` | Color | The colour of the page. The default is white. |
| `layout` | object | `rows` and `cols`, the grid of tiles in which axes are placed. The default is one tile. |
| `data` | map | The [data arrays](#data-arrays), keyed by DataId. |
| `axes` | array of Axes | The axes of the figure, in drawing order. |
| `links` | array of AxisLink | The groups of axes whose limits are [linked](#links). |
| `provenance` | Provenance | A record of the software that wrote the figure; see [provenance](#provenance). |
| `parameters` | map | Named values that describe the figure, used to sort, filter and search collections of figures; see [parameters](#parameters). Omitted from JSON when the figure has none. |

Node identifiers are unique within a figure and do not change when a figure is saved and loaded, so that links, and in future selections and annotations, can refer to nodes across sessions.

Every property of this table and of the tables below can be read in the viewer's [property editor](../guides/viewer.md#the-property-editor), and most of them can be changed there; the editor leaves the structure of a figure and its data to the program that builds it.

## Axes

An axes is a plotting region placed in one or more tiles of the figure's layout.

| Property | Type | Meaning |
| --- | --- | --- |
| `id` | NodeId | The identifier of the axes. |
| `cell` | object | The block of tiles occupied: `row` and `col` of the top-left tile, counted from zero at the top-left of the figure, and `row_span` and `col_span`, each at least one. Where an axes sits is part of the arrangement of the figure, so the property editor shows it read-only. |
| `projection` | Projection | Whether the axes is two- or three-dimensional. |
| `title` | Text or `null` | The title drawn above the axes. |
| `x`, `y`, `z` | Axis | The coordinate axes. In a two-dimensional axes, `z` is ignored, and the property editor hides it until the axes is made three-dimensional. |
| `box` | boolean | Whether the full outline of the plot box is drawn, rather than only the edges that carry tick labels. The default is `true`. |
| `colormap` | string | The colormap used by colormapped colours: `viridis` (the default), `cividis`, `magma`, `inferno`, `plasma`, `coolwarm` or `gray`. |
| `clim` | Limits | The data values mapped to the first and last colours of the colormap. Automatic colour limits are the exact range of the axes' colour data. |
| `legend` | Legend or `null` | The legend, or `null` when no legend is shown. |
| `artists` | array of Artist | The plots drawn in the axes, in drawing order: a later artist covers an earlier one. |

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
| `grid` | boolean | Whether grid lines are drawn at the major ticks of this axis. |

### Limits

- `{"type": "auto"}` computes the range from the data, rounded outwards to tick values. For axis limits, the data of every axes linked along the same dimension is included, so that linked axes agree. Along x and y, an end of the range reached only by the grid of a contour or surface is the exact end of that grid rather than a tick value. Automatic colour limits are the exact range of the colour data.
- `{"type": "manual", "min": number, "max": number}` fixes the range. The bounds must be finite, `min` must be less than `max`, and both must be positive on a logarithmic axis.

### Legend

| Property | Meaning |
| --- | --- |
| `location` | Where the legend is placed inside the plot area: `north_east` (the default), `north_west`, `south_east`, `south_west`, `north`, `south`, `east`, `west`, or `best`, which chooses the corner that overlaps the least data. |
| `boxed` | Whether the legend has a background and an outline. The default is `true`. |

A legend lists every artist of its axes that has a display name, in artist order.

## Artists

An artist is a drawable node of an axes. Every artist has three common properties:

| Property | Type | Meaning |
| --- | --- | --- |
| `id` | NodeId | The identifier of the artist. |
| `display_name` | Text or `null` | The name shown in the legend. An artist without a display name has no legend entry. |
| `visible` | boolean | Whether the artist is drawn. A hidden artist emits nothing, but its data still contributes to automatic limits and it keeps its place in the colour order, so hiding it changes nothing else. |

The artist variants are listed below, each with the MATLAB functions it represents. Array lengths and shapes are checked by validation, not by the schema.

### `line`

A polyline through data points, with optional markers (MATLAB's `plot`, `plot3`, `loglog`, `semilogx` and `semilogy`).

| Property | Meaning |
| --- | --- |
| `x`, `y` | DataIds of the point coordinates, one value per point. |
| `z` | DataId of the z coordinates, or `null`. Allowed only in three-dimensional axes; without it, points lie in the plane z = 0. |
| `line` | The LineStyle of the line. A non-finite coordinate breaks the line. |
| `marker` | The MarkerStyle of the markers drawn at the points. |

### `scatter`

Markers at data points, each with its own size and colour (MATLAB's `scatter` and `scatter3`).

| Property | Meaning |
| --- | --- |
| `x`, `y`, `z` | As for a line. |
| `size` | `{"type": "scalar", "value": number}` gives every marker the same size in points; `{"type": "data", "data": DataId}` gives one size per point. It overrides `marker.size_pt`, which the property editor therefore shows read-only on a scatter. |
| `color` | `{"type": "spec", "spec": ColorSpec}` gives every marker the same colour; `{"type": "data", "data": DataId}` gives one value per point, mapped through the axes colormap and colour limits. The colour applies wherever `marker.face` or `marker.edge` is `auto`. |
| `marker` | The MarkerStyle; its shape defaults to `circle`. |

### `contour`

Isolines or filled bands of a scalar field sampled on a grid (MATLAB's `contour`, `contourf` and `contour3`).

| Property | Meaning |
| --- | --- |
| `grid` | The [grid](#grids) on which the field is sampled. |
| `z` | DataId of the field, an array of shape `[ny, nx]`. |
| `levels` | `{"type": "auto", "count": integer}` chooses about `count` levels at round values (the default count is 10); `{"type": "explicit", "values": [number, …]}` gives strictly increasing levels. |
| `fill` | `true` fills the bands between levels (`contourf`); `false` draws only isolines. |
| `placement` | `{"type": "plane", "z": number or null}` places every contour in one horizontal plane, at the bottom of the z axis when `z` is `null`; `{"type": "at_level"}` places each isoline at the height of its level (`contour3`) and is valid only in three-dimensional axes. |
| `line` | The LineStyle of the isolines. A colormapped colour takes each isoline's level. |

### `quiver`

Arrows representing a vector field (MATLAB's `quiver` and `quiver3`).

| Property | Meaning |
| --- | --- |
| `x`, `y`, `z` | DataIds of the arrow tails, one value per arrow; `z` as for a line. |
| `u`, `v` | DataIds of the vector components, one value per arrow. |
| `w` | DataId of the z components, or `null`. Allowed only in three-dimensional axes. |
| `scale` | `{"type": "auto"}` scales arrows so that they do not overlap; `{"type": "factor", "value": number}` multiplies the automatic scale; `{"type": "off"}` draws vectors with their lengths in data units. |
| `line` | The LineStyle of the shafts and heads. |
| `head_size` | The length of each arrow head as a fraction of its arrow's length. The default is 0.3. |

### `surface`

A surface of quadrilateral faces over a grid (MATLAB's `surf` and `mesh`).

| Property | Meaning |
| --- | --- |
| `grid` | The [grid](#grids) on which the surface is sampled. |
| `z` | DataId of the height of every node, an array of shape `[ny, nx]`. |
| `c` | DataId of colour data with the same shape as `z`, or `null` to colour by `z`. |
| `face` | The ColorSpec of the faces. A colormapped face takes the colour of the mean of its four corner values; a face with a missing corner is not drawn. |
| `edge` | The ColorSpec of the face edges. |
| `edge_width_pt` | The width of the edges. The default is 0.5 pt. |

`surf` stores colormapped faces with black edges, and `mesh` stores faces in the background colour with colormapped edges.

## Styles

### LineStyle

| Property | Meaning |
| --- | --- |
| `color` | The ColorSpec of the line. |
| `width_pt` | The line width. The default is 0.75 pt. |
| `dash` | `solid` (the default), `dashed`, `dotted`, `dash_dot`, or `none`, which draws no line. |

### MarkerStyle

| Property | Meaning |
| --- | --- |
| `shape` | `none` (the default for lines), `circle`, `square`, `diamond`, `triangle_up`, `triangle_down`, `plus`, `cross` or `point`. |
| `size_pt` | The width of the marker. The default is 4 pt. |
| `face` | The ColorSpec of the marker interior. The default is `none`. |
| `edge` | The ColorSpec of the marker outline. The default is `auto`. |

## Colours

A **Color** is an sRGB colour with straight (not premultiplied) alpha, whose four components each lie from 0 to 1. In JSON it is written as the string `"#rrggbb"` when it is opaque and `"#rrggbbaa"` otherwise, and is therefore stored with eight bits per component. In Protocol Buffers it is a message of four `float` components, which keeps the full precision of the model.

A **ColorSpec** says how a colour is chosen:

- `{"type": "auto"}` lets the renderer choose. For the primary colour of a line, scatter or quiver, this is the next colour of the axes colour order, which is the Okabe–Ito palette without black. For a marker face or edge, it is the resolved colour of the artist. For contour isolines and surface faces and edges, it means `colormapped`.
- `{"type": "rgba", "color": Color}` is a fixed colour.
- `{"type": "none"}` draws nothing.
- `{"type": "colormapped"}` takes the colour from the axes colormap, indexed by the data value scaled into the axes colour limits.

A colormapped colour is meaningful only where the model holds a value to index the colormap by: the isolines of a contour, indexed by their level; the faces and edges of a surface, indexed by its height or its colour data; and a scatter whose colour comes from an array. A line, a quiver and a scatter with a single colour hold no such value, so a colormapped colour there is drawn as the middle colour of the colormap; a marker takes the resolved colour of the artist it belongs to, so a colormapped marker face or edge is drawn exactly as an automatic one. The property editor lists the choice everywhere and shows it disabled, with the reason, where it has no meaning, as [using the viewer](../guides/viewer.md#what-the-editor-does-not-change) describes.

## Text

A **Text** is stored as its source, never as typeset glyphs, so that it can be edited and so that the renderer resolves it when it is drawn.

| Property | Meaning |
| --- | --- |
| `content` | The source text. |
| `interpreter` | `"latex"` (the default) treats segments delimited by `$…$` as LaTeX mathematics and the rest as plain text, with `\$` producing a literal dollar sign. `"none"` renders the content literally, dollar signs included. |

Mathematics that the typesetter cannot handle is drawn as its raw source and reported as a warning; it never makes a figure invalid. How text is resolved is described in the [architecture reference](architecture.md#text-resolution) and [ADR 0005](../adrs/0005-embedded-latex-math-with-latex-rust.md).

## Data arrays

Artists refer to their numeric data by DataId rather than containing it, so that several artists can share one array and each array is stored once. Each entry of the figure's `data` object is an **NdArray**:

| Property | Meaning |
| --- | --- |
| `shape` | The length of each dimension, outermost first. A vector of `n` values has shape `[n]`; a field with `ny` rows and `nx` columns has shape `[ny, nx]`. |
| `values` | The values in row-major order: the value in row `j` and column `i` of a two-dimensional array is `values[j * nx + i]`. |

A missing value is NaN. Protocol Buffers stores every value, including NaN and infinities, as an IEEE 754 double. JSON cannot represent non-finite numbers, so every non-finite value (NaN or an infinity) is written as `null` and read back as NaN. The number of values must equal the product of the shape.

Rows of a field correspond to y and columns to x, as in MATLAB: the value in row `j` and column `i` belongs to the point `(x[i], y[j])`.

## Grids

A **Grid** describes where the nodes of a field of shape `[ny, nx]` lie.

- `{"type": "rectilinear", "x": DataId, "y": DataId}` is an axis-aligned grid: `x` holds `nx` column coordinates and `y` holds `ny` row coordinates.
- `{"type": "curvilinear", "x": DataId, "y": DataId}` is a structured grid in which every node has its own coordinates: `x` and `y` are both arrays of shape `[ny, nx]`.

## Links

An **AxisLink** is a group of axes whose limits along one dimension are kept equal.

| Property | Meaning |
| --- | --- |
| `dimension` | `"x"`, `"y"` or `"z"`. |
| `axes` | The NodeIds of the linked axes. |

For each dimension, groups are disjoint: an axes belongs to at most one group per dimension. Linking axes that already belong to groups merges those groups into one, which is a union-find operation. Setting the limits of any member, through the API or through the viewer, sets the limits of every member, and automatic limits are computed over the data of the whole group. A member that cannot show those limits, such as a logarithmic axis given a range that reaches zero, refuses the whole change, so axes that cannot share limits cannot be linked. The behaviour in the viewer is described in [using the viewer](../guides/viewer.md#linked-axes), and the design in [ADR 0008](../adrs/0008-typed-edits-and-a-view-overlay.md).

## Provenance

A **Provenance** records the software that wrote a figure, so that a figure that renders differently in a later version can be diagnosed.

| Property | Meaning |
| --- | --- |
| `ironlab_version` | The version of IronLAB that wrote the figure. |
| `typesetter` | The name and version of the mathematics typesetter, such as `"latex-rust 1.0.2"`. |
| `fonts` | The names of the fonts used for text. |

The PDF exporter copies the provenance into the document metadata.

## Parameters

A **Parameter** is a named value that describes a figure, such as the Reynolds number of the flow that it shows or the solver that produced its data. Parameters do not affect drawing; they are stored with the figure so that collections of figures can be sorted, filtered and searched. The `parameters` of a figure map each name to one parameter, so a name occurs at most once, and they are kept in ascending order of name.

A parameter takes one of four forms:

| JSON | Protocol Buffers variant | Value |
| --- | --- | --- |
| `{"type": "bool", "value": true}` | `bool_value` (ParameterBool) | A boolean. |
| `{"type": "integer", "value": -4096}` | `integer_value` (ParameterInteger) | A signed 64-bit integer. |
| `{"type": "number", "value": 100000.0}` | `number_value` (ParameterNumber) | A double-precision floating-point number, which must be finite. |
| `{"type": "string", "value": "k–ω SST"}` | `string_value` (ParameterString) | A string, which may be empty. |

Parameters are edited in the viewer's [property editor](../guides/viewer.md#parameters) as well as through the API.

The form is stated explicitly because JSON has a single number type: without it, the number `3.0` would reload as the integer `3`. A JavaScript program reads a JSON integer as a double, which holds integers exactly only up to 2<sup>53</sup> in magnitude, so a larger integer parameter is exact only in readers that parse JSON integers as 64-bit integers. A parameter name must not be empty; it may contain any other Unicode text, and names that differ only in case are distinct.

```json
"parameters": {
  "converged": { "type": "bool", "value": true },
  "reynolds_number": { "type": "number", "value": 100000.0 },
  "solver": { "type": "string", "value": "k–ω SST" }
}
```

## Validation

The encodings describe the structure of a figure but cannot express every rule. `Figure::validate` checks the rest and returns errors, which prevent a figure from being exported or shown, and warnings, which do not.

Errors are reported for a reference to a DataId that is not in `data`, an array whose number of values does not match its shape, arrays of one artist with inconsistent lengths or shapes, a three-dimensional artist or placement in a two-dimensional axes, a link to an identifier that is not an axes, two nodes with the same identifier, a cell outside the tile layout or with a zero span, a non-positive figure size or font size, invalid manual limits, empty, non-finite or non-increasing contour levels, and a parameter with an empty name or a non-finite number. Warnings are reported for finite non-positive data plotted along a logarithmic axis, which is not drawn.

## A minimal JSON file

The following JSON file describes one two-dimensional axes with a line through three points. The same figure saved as a `.fig` file holds the same fields in the Protocol Buffers encoding.

```json
{
  "schema_version": "0.2.0",
  "id": 0,
  "title": null,
  "size": { "width_mm": 80.0, "height_mm": 60.0 },
  "font_set": "stix_two",
  "font_size_pt": 9.0,
  "background": "#ffffff",
  "layout": { "rows": 1, "cols": 1 },
  "data": {
    "0": { "shape": [3], "values": [0.0, 1.0, 2.0] },
    "1": { "shape": [3], "values": [0.0, 1.0, null] }
  },
  "axes": [
    {
      "id": 1,
      "cell": { "row": 0, "col": 0, "row_span": 1, "col_span": 1 },
      "projection": { "type": "two_d" },
      "title": null,
      "x": { "label": { "content": "$t$", "interpreter": "latex" }, "scale": "linear", "limits": { "type": "auto" }, "grid": false },
      "y": { "label": null, "scale": "linear", "limits": { "type": "manual", "min": -1.0, "max": 2.0 }, "grid": false },
      "z": { "label": null, "scale": "linear", "limits": { "type": "auto" }, "grid": false },
      "box": true,
      "colormap": "viridis",
      "clim": { "type": "auto" },
      "legend": null,
      "artists": [
        {
          "type": "line",
          "id": 2,
          "display_name": null,
          "visible": true,
          "x": 0,
          "y": 1,
          "z": null,
          "line": { "color": { "type": "auto" }, "width_pt": 0.75, "dash": "solid" },
          "marker": { "shape": "circle", "size_pt": 4.0, "face": { "type": "none" }, "edge": { "type": "auto" } }
        }
      ]
    }
  ],
  "links": [],
  "provenance": {
    "ironlab_version": "0.2.0",
    "typesetter": "latex-rust 1.0.2",
    "fonts": ["STIX Two Text", "STIX Two Math"]
  }
}
```

The third y value is `null`, so it is missing: the line ends at the second point, and a marker is drawn only at the first two. The figure has no parameters, so the file has no `parameters` property.

## Versioning

The schema version has the form `major.minor.patch`, and the version implemented by the current build is `0.2.0`. It is the `schema_version` property in JSON and field 1 of the `Figure` message in Protocol Buffers, and it is checked before the rest of a file is read. A file loads when its major and minor components equal those of the build; the patch component may differ. Fields that a build does not recognise are ignored, so a file written by a later patch release of the same minor version still loads. A file with a different major or minor version is rejected with an error that names both versions, rather than being reported as malformed.

The version applies to both encodings. The Protocol Buffers package name carries only the major version (`v0`). Independently of the version, `buf breaking` in CI reports any change to the generated `.proto` files that is incompatible with the files generated from the `main` branch.

Changes to the schema are versioned as follows.

- A **patch** version is for changes that every reader of the same minor version can ignore safely, such as a new optional property whose absence preserves the previous drawing.
- A **minor** version is for additions that older readers cannot draw correctly, such as a new artist variant, a new projection, or a new variant of any tagged entity.
- A **major** version is for changes that alter or remove the meaning of existing properties.

While the major version is 0, the project may make breaking changes in a minor version, and a file of a different minor version is rejected in either case.

The schema has had the following versions:

- **0.2.0** added the [parameters](#parameters) of a figure, and replaced the `pan` array of a View3d with the properties `pan_x` and `pan_y`, so that JSON uses the same names as Protocol Buffers. A file of version 0.1 is rejected.
- **0.1.0** was the first version.

## Extending the model with new plot types

The model is designed so that further MATLAB plot types are added without restructuring it. Before any new plot type or entity is implemented, its data structure and options are defined in `ironlab-ir` and reviewed, as the project rules require.

- **Most plot types are new artist variants.** Bar charts, histograms, stem, stairs, area and error-bar plots, images (`image`, `imagesc`), pseudocolour plots (`pcolor`), patches and streamlines each become a new variant of Artist: a new `type` in JSON and a new variant of the `kind` oneof in Protocol Buffers. Each variant has the three common properties, refers to its data by DataId, reuses LineStyle, MarkerStyle, ColorSpec and Grid where they apply, and states its array-shape rules in validation. Image-like artists take their colours from a colour scale, as [ADR 0011](../adrs/0011-colour-scales-are-nodes.md) describes.
- **New coordinate systems are new projections.** Polar and geographic axes become new variants of Projection, each with its own view properties, alongside `two_d` and `three_d`.
- **New axes-level decorations are new axes properties.** A colorbar, for example, becomes an optional property of an axes that shows the colour scale of that axes and holds no colormap or colour limits of its own, so that it cannot disagree with them ([ADR 0011](../adrs/0011-colour-scales-are-nodes.md)).
- **Annotations are nodes.** Pinned data tips and other annotations become nodes with their own identifiers, anchored to the artist and data index they describe, so that they are saved with the figure and exported like any other node.

Each such addition is a new variant of a tagged entity and therefore increments the minor version of the schema, as described in [versioning](#versioning). The scene compiler then learns to draw the new variant; neither backend changes, as explained in the [architecture reference](architecture.md#one-path-to-pixels).
