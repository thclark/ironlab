# Architecture

This page describes how IronLAB is divided into crates, how a figure becomes pixels on screen or a page of a PDF, how interaction changes a figure, and how text is resolved. The decisions behind this structure, and the alternatives considered, are recorded in the [architecture decision records](../adrs/index.md).

## Crates

IronLAB is a Cargo workspace whose crates live in `crates/`.

| Crate | Responsibility |
| --- | --- |
| `ironlab-ir` | The figure model: the Rust types of the [figure schema](figure-schema.md), which are its source of truth; their [encodings](figure-schema.md#encodings) as Protocol Buffers (`.fig`, through the `wire` module) and JSON (`.fig.json`); generation of the `.proto` files and the JSON Schema as build artefacts; validation, identifier allocation, and linking of axis limits. It has no knowledge of drawing. |
| `ironlab-text` | Text: the bundled STIX Two fonts, shaping of plain text with HarfRust, typesetting of LaTeX mathematics with latex-rust, the memo of resolved text layouts, and the cache of glyph outlines. |
| `ironlab-scene` | The scene compiler: layout of tiles, axes, titles, labels and legends; tick generation; automatic limits; colormaps; contour extraction; quiver scaling; the placement and colouring of the pixels of images; three-dimensional projection and depth sorting. Its output is a display list and a hit map. |
| `ironlab-pdf` | The PDF backend: draws a display list onto a single PDF page with krilla, embedding subset fonts and real text. |
| `ironlab-canvas` | The figure canvas, which every host draws a figure with: the canvas that turns a display list into one draw list and draws it through its own wgpu pipelines, the interaction state machine, the logic of the property editor and of the problems a figure reports, an offscreen renderer that draws a figure into an image without a window through the same pipelines, PDF export through that renderer, and the encoding and decoding of `.fig` and JSON figure files. It depends on no user interface toolkit and builds for the browser. |
| `ironlab-viewer` | The interactive viewer: an eframe application showing one figure at a time, chosen in a browser that narrows a collection by its labels and parameters, with a toolbar and the property editor. It is the egui host of `ironlab-canvas`, drawing each figure through the canvas crate's pipelines inside the window's render pass, and it ships the `ironlab-viewer` binary, which opens `.fig` and JSON files. |
| `ironlab-web` | The browser host: the canvas crate compiled to WebAssembly and exposed to JavaScript with wasm-bindgen, and the `<ironlab-figure>` element and stylesheet that load it, which draw a figure file into a `<canvas>` element of a web page through the canvas crate's pipelines, with a figurebar, save and PDF export written in HTML. It depends on `ironlab-canvas` and not on egui, is never published to crates.io, and is distributed as the bundle described in [embedding figures in a web page](../guides/embedding.md). |
| `ironlab` | The facade: the MATLAB-flavoured builder API, and the `show`, `save`, `load` and `export_pdf` operations described in [getting started](../guides/getting-started.md). `export_pdf` returns a report of the validation warnings and the scene warnings of the figure, so that a program learns what was left off the page. |
| `ironlab-gallery` | The example figures, each written against the facade API, and the `gallery` binary that views them, exports them and generates the documentation [gallery](../gallery/index.md). |

The dependencies run in one direction. `ironlab-ir` and `ironlab-text` depend on no other IronLAB crate; `ironlab-scene` depends on both of them; `ironlab-pdf` depends on `ironlab-scene`; `ironlab-canvas` depends on `ironlab-scene` and on `ironlab-pdf`, which it uses to export; `ironlab-viewer` and `ironlab-web` each depend on `ironlab-canvas` and on nothing of each other; the facade depends on every crate above it except `ironlab-web`; and the gallery depends on the facade, the canvas, the viewer and the PDF backend.

`ironlab-pdf` therefore cannot reach the canvas crate's renderer, and does not try to: it records the renders an export needs, as display lists with a resolution, and the canvas crate performs them and hands the images back for the exporter to embed. Every export in the project goes through `ironlab_canvas::export_pdf`, which renders those requests through the offscreen renderer, so the raster fallback described under [the backends](#the-backends) is always available and never duplicated.

The split between `ironlab-canvas` and its hosts is enforced by the compiler rather than by convention: the canvas crate is built for `wasm32-unknown-unknown` in continuous integration, which fails if anything in it comes to depend on egui, eframe, a file dialog or any other native-only crate. The two hosts are therefore guaranteed to draw figures with the same code, and neither interprets a figure: each forwards the gestures it decides to the canvas crate's `FigureCanvas`, which turns them into typed edits, and each draws only the chrome the canvas reports back, namely the cursor, the rubber band and the datatip callout. The egui host takes its gesture decisions from egui; the browser host feeds raw pointer events to the canvas crate's `Pointer` recogniser, which decides drags, clicks and double clicks with egui's own thresholds, so a gesture means the same thing in a window and in a page. The decision to build the browser host without egui is recorded in [ADR 0016](../adrs/0016-two-hosts-over-one-canvas.md).

## One path to pixels

Every drawing of a figure, whether on screen, in an offscreen image or on a PDF page, is produced by the same pipeline:

```text
                 ironlab-ir                      ironlab-text
           Figure (retained model)        fonts, shaping, LaTeX math
                     │                                │
                     └───────────────┬────────────────┘
                                     ▼
                    ironlab-scene::compile(&Figure, &TextEngine)
   layout · ticks · limits · colormaps · contours · quivers · images · 3D projection · depth sort
                                     │
                                     ▼
                  Scene { display_list, hit_map, warnings }
                                     │
               ┌─────────────────────┼──────────────────────┐
               ▼                     ▼                      ▼
     ironlab-canvas canvas   ironlab-canvas offscreen   ironlab-pdf
     lyon → one draw list    same list → image          krilla → PDF page
     → wgpu pipelines        (gallery images, tests,    (export; lists the
     (hosted by the viewer   the exporter's rasters)    rasters it needs, which
     and the web element)                               the canvas renders)
```

The scene compiler is the only place in which geometry is computed. Tick positions, text placement, contour lines, arrow shapes, the placement and colours of the pixels of images, projection and the order in which three-dimensional faces are painted are all decided there, once. The backends are deliberately simple consumers: they draw the items of the display list in order and make no decisions of their own. Because the screen and the PDF receive identical geometry, they cannot disagree about what a figure looks like. This rule, and the choice of an egui-mesh canvas for now with custom GPU pipelines later, are recorded in [ADR 0003](../adrs/0003-shared-scene-compiler-and-display-list.md).

### The display list

The display list is a backend-neutral description of one page. Its coordinates are in points in figure space, with the origin at the top-left corner of the figure, x increasing to the right and y increasing downwards. It has five kinds of item:

- **Path**: a sequence of move, line, cubic Bézier and close segments, with an optional fill (a colour and a fill rule) and an optional stroke (a colour, width, dash pattern, cap and join).
- **Glyphs**: a run of glyphs from one bundled font at one size, each placed at a point on its baseline, together with the text that the run represents, so that the PDF backend can write selectable text.
- **Image**: a rectangle and the true-colour samples drawn into it, three channels for an opaque image and four when it has straight alpha. The compiler emits one for every image artist it draws, in pixel space beneath a group whose transform places it in the axes, so that a mirrored, stretched or projected image is carried by the transform and its samples stay in row order; the PDF exporter's raster fallback builds one when it replaces dense geometry.
- **Group**: a list of items with an optional clip rectangle and an optional transform.
- **Dense**: a list of items drawn for one artist whose data is dense enough that a backend may replace them with an image, together with the number of data cells the artist drew. A dense item has neither a clip nor a transform of its own, so a backend that ignores the marking draws exactly the same picture.
- **Markers**: the markers of one artist as instances of one outline: the marker shape of width one centred on the origin, the width of its edge, and one instance per marker with its position, size, depth, face and edge colours and the index of its data point. The compiler emits one per artist in a two-dimensional axes and one per run of consecutive markers in the painter's order of a three-dimensional one, so that markers interleave with the faces around them exactly as they did when each was a path.
- **Depth**: the artists of one three-dimensional axes, in the painter's order the compiler chose, drawn with a depth buffer by a backend that has one. Like a dense item it carries neither clip nor transform, so a backend without a depth buffer draws the items in order and gets the painter's picture. Every path and image inside it carries the depth at which it is painted, in its own local space: a plane for a surface face, a marker, a filled contour band or an image, or one depth per endpoint for a polyline. The compiler pushes every face and every image inside the box a thousandth of the box behind its geometry and sorts it a thousandth earlier, so that lines and markers lying on a surface win in both orders, and draws the edge of a face as a ring inset inside the face rather than as a stroke, so that it never spills over its neighbours; the reasons are recorded in [ADR 0013](../adrs/0013-depth-buffered-three-dimensional-artists.md).

Every item names the node of the figure model that produced it, so that selection and picking can be added without changing the display list.

Compilation never fails. An artist whose data cannot be drawn, or gives it nothing to draw, is skipped, and text that cannot be typeset is drawn as its source; each such problem becomes a warning in the scene, naming the node it concerns, which the viewer shows in its [problems indicator](../guides/viewer.md#problems) and the PDF exporters return with the document. The compiler warns of every artist it leaves out, so that the problems indicator and `Figure::validate` agree about which artists are absent, as [ADR 0012](../adrs/0012-empty-and-singleton-data.md) decides.

### Large series are thinned for the current view

A series of a million points drawn into a plot a few hundred points wide cannot show a million distinct positions. The scene compiler therefore thins a line or a scatter that holds more points than its plot rectangle can resolve, before the geometry reaches the display list. Because the thinning happens in the compiler rather than in a backend, the canvas and the PDF draw the same thinned series, and the figure that is exported is the figure that was on screen.

The target is four drawn points per point of plot width, with a floor of 256 points, and it is not configurable: it is a fidelity-preserving optimisation rather than a choice about how a figure looks, and a knob would let a figure be exported at a different fidelity from the one it was designed at. A series no longer than the target is drawn in full, including the parts of it that lie outside the axes.

Thinning is applied in figure space, so it follows the view. The axis limits, the axis scales, the plot rectangle and, in three dimensions, the camera all decide which points survive, and the compiler runs again whenever any of them changes. Two rules are used:

- A **line** is thinned by the largest-triangle-three-buckets rule, which divides the points into consecutive buckets and keeps from each the one that forms the largest triangle with its neighbours. It keeps the peaks, troughs and corners that carry the shape of a curve, which taking every *n*th point loses. The first and last points always survive, and each run of drawable points is thinned on its own, so a non-finite value still breaks the line rather than being smoothed over.
- A **set of markers**, whether a scatter or the markers of a line, is thinned by keeping one marker per square of half a marker width, the one that would be painted over the others. The cost of a dense scatter is then set by the area of its plot rather than by the size of its data.

Segments and markers that the axes clip away are dropped before either rule is applied, so the whole budget is spent on what the reader can see: zooming into a thousand points of a million draws those thousand in full.

The reasons for thinning in the compiler rather than in a backend, for these two rules, for a fixed target and for carrying each drawn point's source index with it are recorded in [ADR 0009](../adrs/0009-view-dependent-decimation-with-source-index-maps.md). The gallery entry [Data decimation for very large datasets](../gallery/decimated_timeseries.md) shows the effect: two views of one hundred-thousand-sample record, each thinned to what its own view resolves.

### The hit map

The hit map is the geometry that the viewer needs to relate a pointer position to the figure: for each axes, its plot rectangle and, in two dimensions, the mapping between data values and figure coordinates along each axis; for each legend entry, its rectangle and the artist it represents; for each line and scatter, the points it drew; and, for each image drawn in a two-dimensional axes, the inverse of the transform that placed it together with its numbers of rows and columns.

Each drawn point carries its position in figure space together with the index it has in the artist's own data arrays, in one value, so that the two cannot be separated or fall out of step. The index is the one the user's data uses, never a position in the thinned series, which is what lets the viewer's [datatip](../guides/viewer.md#datatips) name the measurement the reader is pointing at however hard the series was thinned.

An image entry answers the same question for a raster: the inverse transform maps a point of figure space back into the pixel space of the image, where the row and column of the pixel under the point are those of the artist's own array, whichever way the image was mirrored or stretched. Only images in two-dimensional axes have an entry, because an image on the floor or a wall of a three-dimensional axes may be hidden by other geometry and the hit map cannot tell; picking there waits for the GPU picking pass.

### The backends

- The **canvas** turns the display list into one draw list per figure: fills and glyph outlines are tessellated with lyon into triangles in figure points (a glyph once per glyph and size); a stroke becomes one segment per edge of its flattened polyline, in the item space of its path, with the stroke's width, cap, join, dashes, colour and transform in a block of parameters per draw, which the viewer's stroke shaders expand into the body, joins, caps and dashes of the stroke exactly as a PDF reader strokes the same path; the markers of an artist become one outline tessellated once and one instance per marker, drawn through one instanced draw; an image item becomes one textured quad per tile of at most 8192 pixels on a side, cut at the device's largest texture side, whose vertices carry the tile's texture coordinates so that a mirrored or projected image lands where the compiler placed it, sampled with nearest filtering so that the edges of its pixels stay hard; and the leaves of a depth item carry the depth of every vertex. Each draw records its clip rectangle in figure points, its depth group and its node. The viewer's own wgpu pipelines draw the list inside egui's render pass, through one paint callback in the figure's place among the interface's shapes, with four-times multisample anti-aliasing: the mapping from figure points to the screen is a uniform, so a resize or a pan of the window re-uploads nothing but it; every draw is cut by the scissor rectangle of its clip, rounded to whole pixels as egui rounds its own; and the draws of a depth group are depth-tested against one another after the depth buffer is cleared for the group. The list is rebuilt only when the scene changes or the figure's scale on screen leaves a band of a quarter around the scale it was flattened for, and its buffers and textures stay on the device for as long as it is drawn. The figure is drawn at its physical aspect ratio, scaled to fit the window.
### The browser host

In a web page the same draw list is drawn by the same pipelines through a wgpu surface on the page's `<canvas>` element, on WebGPU where the browser offers it and on WebGL2 otherwise. The host draws each frame into colour and depth textures of its own with four samples per pixel and resolves it into the surface, in the render pass setup, `figure_pass`, that the offscreen renderer uses, so that the pixels on the page are the pixels of the gallery image. The surface is opaque, which is the only mode the web backends offer, so a figure with a transparent background is cleared to a backdrop colour that the page's theme supplies. Under WebGPU one device serves every figure on the page; under WebGL2 the adapter is each canvas element's own context, so every figure has a device of its own and browsers allow about sixteen live contexts per page, which is why the element creates a figure only when it comes near the viewport and releases it when it is removed. Export from a page has the fidelity of a native export because the exporter's renders are requested and replayed in two phases and the renderer's device creation and readback are awaited: the page performs the renders on the browser's device and hands the images back, exactly as the native viewer does through the blocking wrappers, and the export report's warnings are returned with the bytes. The bundle is 7.7 MB, or 3.8 MB compressed: the whole engine, with the fonts of `ironlab-text` making up under a megabyte of it; loading the fonts at run time from beside the bundle, and dropping WebGL2 once WebGPU is universal, are the two levers that will shrink it. How a page uses the element is described in [embedding figures in a web page](../guides/embedding.md).

- The **offscreen renderer** builds the same list and draws it through the same pipelines into a texture without a window, in a render pass of its own with a depth attachment, and reads the image back. The documentation gallery's images are made this way, so they show exactly what the viewer shows, and so are the rasters the PDF exporter embeds and compares. Nothing in this path needs a window or a hardware graphics device; see [running without a graphics device](../guides/getting-started.md#running-without-a-graphics-device).
- The **PDF backend** writes each path, each marker instance as a path, and each glyph run to a single page whose MediaBox and CropBox equal the figure size, with fonts embedded as subsets, and embeds each image item as a deflated image XObject at its own resolution, drawn without interpolation beneath the transform of its group and with a soft mask only when it has alpha. Every item is checked before it reaches krilla and anything that cannot be represented is skipped, so the exporter always writes a valid PDF, including from a display list assembled by hand rather than compiled from a figure. The decisions behind PDF-first export, and behind the raster fallback below, are recorded in [ADR 0010](../adrs/0010-pdf-export-with-a-raster-fallback.md), and those behind the image artists in [ADR 0011](../adrs/0011-image-artists.md).

The PDF backend draws a dense item as a deflated image XObject instead of one path per cell when the cell count reaches the threshold described in [exporting PDF](../guides/getting-started.md#dense-surfaces). It does not rasterise anything itself: it records the renders the page needs, and the viewer performs each through the offscreen renderer above, rendering the dense geometry alone at the export resolution into a transparent image and reading it back, before the exporter runs again with the images in hand. The export is split in this way because a browser cannot wait for the GPU inside the exporter's walk; the split changes nothing about the page, which is byte for byte the one a renderer answering inside the walk would produce. The pixels in an exported PDF are therefore the pixels the viewer would draw, and there is no second rasteriser that could drift from the screen. The image is placed at the rectangle the geometry occupies in figure space and clipped exactly as the geometry was, so it lands where the paths would have; everything else on the page, including all text, stays vector.

A PDF has no depth buffer, so a depth item is handled by the same rasteriser in a second way. Under the default policy the exporter asks for the axes twice at the export resolution, once with the depth test and once without, and writes the artists as vector paths in painter's order when the two renders show the same picture, which proves that the order draws what the viewer draws; otherwise it embeds the depth-tested render as an image, placed and clipped like a dense one. The policy can force either outcome, as described in [three-dimensional axes](../guides/getting-started.md#three-dimensional-axes). Whenever the exporter draws something as an image, or draws a three-dimensional axes without being able to verify it, the export report says so with the reason, so that nothing about the page changes silently. The decisions are recorded in [ADR 0013](../adrs/0013-depth-buffered-three-dimensional-artists.md).

## Interaction records edits in an overlay

The viewer has no view state of its own that affects drawing. Each gesture is converted into a transaction of typed edits of the figure model, in the same way that setting a property from the API edits it:

- panning and zooming a two-dimensional axes set its axis limits to manual values;
- panning, zooming and rotating a three-dimensional axes set the properties of its `projection.view3d`;
- clicking a legend entry sets the `visible` property of an artist;
- changing a property in the property editor sets that property.

Limits are set with the `set_limits` command, which reads the figure and returns the limits of every axes of the link group as literal edits, so linked axes follow without the viewer implementing the rule. An axes of the group that cannot show the limits makes the transaction fail, and the figure is left unchanged.

The viewer holds each figure as its owner defines it (the source, which for the viewer is the figure as opened or as last saved) and the user's edits (the overlay), and draws their composition, which is recomputed whenever either changes. A gesture therefore never mutates the source: undo and redo step through the overlay, double-click and Reset view discard the view entries of the overlay, and saving folds the overlay into the source. An overlay entry that the source cannot accept is dropped when the composition is made, and the reason is shown by the problems indicator.

The property editor is the first part of the viewer that is not a gesture, and it uses the same path. Its contents come from the property registry of `ironlab-ir`, which lists every settable path of each kind of node with its type and documentation, and from the choices of each value type, which give the variants of a tagged value and the values of an enumeration ready to be set. Which of those choices mean anything at a given property of a given node is also answered by `ironlab-ir`, because it is a fact about the semantics of the model rather than about the interface: a colormapped colour is offered only where the model holds a value to index the colormap by. What the editor shows and what a change commits are built in a module of pure logic, as gestures are, so the panel itself only draws. A change it cannot apply is refused before it is recorded, so the figure is left unchanged and no undo step is spent; one property of the overlay is taken back by reverting exactly that entry, and the whole overlay, with its history, by clearing it.

The editor changes the properties of the nodes a figure has, and leaves its structure and its data to the program that builds the figure; such properties are shown read-only with the reason. Everything the viewer has to report — the scene's warnings, the overlay entries that composition discarded and the changes the model refused — is carried as one kind of value, which the problems indicator lists with the node, the property and the origin of each.

Because the composed figure is the only state that drawing depends on, exporting and saving from the viewer write what is on screen, and the interaction logic is pure code with no GPU or window, which is tested directly. This design is recorded in [ADR 0008](../adrs/0008-typed-edits-and-a-view-overlay.md), and its behaviour from the user's side is described in [using the viewer](../guides/viewer.md).

```text
 pointer input ──▶ interaction::FigureState ──sets──▶ Overlay
                          ▲                             │
                          │                             ▼
                          │             Figure (source) + overlay = displayed figure
                          │ hit map                     │
                          └──────────── Scene ◀── ironlab-scene::compile
                                          │
                                          ├──▶ canvas (redraw)
                                          ├──▶ ironlab-pdf (Export PDF…)
                                          └──▶ .fig or .json (Save figure…)
```

## Text resolution

The figure model stores every piece of text as its source string with an interpreter, never as typeset glyphs. Text is resolved only when the scene compiler lays out a figure, by the text engine in `ironlab-text`:

1. With the LaTeX interpreter, the source is split into plain segments and mathematics segments delimited by `$…$`. With no interpreter, the whole source is one plain segment.
2. Plain segments are shaped with HarfRust against STIX Two Text.
3. Mathematics segments are parsed and laid out by latex-rust against STIX Two Math, which applies TeX's conventions itself: hyphens become minus signs, unstyled letters are set in mathematical italic, and scripts are drawn at the font's script sizes. latex-rust rejects input nested more deeply than its nesting limit with an error, and layout runs on a helper thread with a stack of known size (in the browser, which has one thread, the linker gives that thread a large stack instead), so that no input can overflow a stack.
4. The resulting box tree is converted into positioned glyphs and rules in points, and the segments are placed on a shared baseline.

Resolved layouts are memoised in memory, keyed on the source, the interpreter and the size, so each distinct label is typeset once per process. Mathematics that cannot be typeset is drawn as its raw source in the text font, and a warning is recorded, so a label never prevents a figure from being drawn. Glyph identifiers always refer to the bundled font files that both backends draw with. The reasons for embedding a typesetter, and for storing source rather than glyphs, are recorded in [ADR 0005](../adrs/0005-embedded-latex-math-with-latex-rust.md).
