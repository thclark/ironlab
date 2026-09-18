# Architecture

This page describes how IronLAB is divided into crates, how a figure becomes pixels on screen or a page of a PDF, how interaction changes a figure, and how text is resolved. The decisions behind this structure, and the alternatives considered, are recorded in the [architecture decision records](../adrs/index.md).

## Crates

IronLAB is a Cargo workspace whose crates live in `crates/`.

| Crate | Responsibility |
| --- | --- |
| `ironlab-ir` | The figure model: the Rust types of the [figure schema](figure-schema.md), which are its source of truth; their [encodings](figure-schema.md#encodings) as Protocol Buffers (`.fig`, through the `wire` module) and JSON (`.fig.json`); generation of the `.proto` files and the JSON Schema as build artefacts; validation, identifier allocation, and linking of axis limits. It has no knowledge of drawing. |
| `ironlab-text` | Text: the bundled STIX Two fonts, shaping of plain text with HarfRust, typesetting of LaTeX mathematics with latex-rust, the memo of resolved text layouts, and the cache of glyph outlines. |
| `ironlab-scene` | The scene compiler: layout of tiles, axes, titles, labels and legends; tick generation; automatic limits; colormaps; contour extraction; quiver scaling; three-dimensional projection and depth sorting. Its output is a display list and a hit map. |
| `ironlab-pdf` | The PDF backend: draws a display list onto a single PDF page with krilla, embedding subset fonts and real text. |
| `ironlab-viewer` | The interactive viewer: an eframe application with one tab per figure, a toolbar, the canvas that draws a display list as egui meshes, the interaction state machine, the property editor, an offscreen renderer that draws a figure into an image without a window, and the `ironlab-viewer` binary, which opens `.fig` and JSON files. |
| `ironlab` | The facade: the MATLAB-flavoured builder API, and the `show`, `save`, `load` and `export_pdf` operations described in [getting started](../guides/getting-started.md). |
| `ironlab-gallery` | The example figures, each written against the facade API, and the `gallery` binary that views them, exports them and generates the documentation [gallery](../gallery/index.md). |

The dependencies run in one direction. `ironlab-ir` and `ironlab-text` depend on no other IronLAB crate; `ironlab-scene` depends on both of them; `ironlab-pdf` depends on `ironlab-scene`; `ironlab-viewer` depends on `ironlab-scene` and on `ironlab-pdf`, which it uses to export; the facade depends on every crate above it; and the gallery depends on the facade, the viewer and the PDF backend.

## One path to pixels

Every drawing of a figure, whether on screen, in an offscreen image or on a PDF page, is produced by the same pipeline:

```text
                 ironlab-ir                      ironlab-text
           Figure (retained model)        fonts, shaping, LaTeX math
                     │                                │
                     └───────────────┬────────────────┘
                                     ▼
                    ironlab-scene::compile(&Figure, &TextEngine)
      layout · ticks · limits · colormaps · contours · quivers · 3D projection · depth sort
                                     │
                                     ▼
                  Scene { display_list, hit_map, warnings }
                                     │
               ┌─────────────────────┼──────────────────────┐
               ▼                     ▼                      ▼
     ironlab-viewer canvas   ironlab-viewer offscreen   ironlab-pdf
     lyon → egui meshes      same meshes → image        krilla → PDF page
     (interactive window)    (gallery images, tests)    (export)
```

The scene compiler is the only place in which geometry is computed. Tick positions, text placement, contour lines, arrow shapes, projection and the order in which three-dimensional faces are painted are all decided there, once. The backends are deliberately simple consumers: they draw the items of the display list in order and make no decisions of their own. Because the screen and the PDF receive identical geometry, they cannot disagree about what a figure looks like. This rule, and the choice of an egui-mesh canvas for now with custom GPU pipelines later, are recorded in [ADR 0003](../adrs/0003-shared-scene-compiler-and-display-list.md).

### The display list

The display list is a backend-neutral description of one page. Its coordinates are in points in figure space, with the origin at the top-left corner of the figure, x increasing to the right and y increasing downwards. It has three kinds of item:

- **Path**: a sequence of move, line, cubic Bézier and close segments, with an optional fill (a colour and a fill rule) and an optional stroke (a colour, width, dash pattern, cap and join).
- **Glyphs**: a run of glyphs from one bundled font at one size, each placed at a point on its baseline, together with the text that the run represents, so that the PDF backend can write selectable text.
- **Group**: a list of items with an optional clip rectangle and an optional transform.

Every item names the node of the figure model that produced it, so that selection and picking can be added without changing the display list.

Compilation never fails. An artist whose data cannot be drawn is skipped, and text that cannot be typeset is drawn as its source; each such problem becomes a warning in the scene, which the viewer shows in its [problems indicator](../guides/viewer.md#problems).

### Large series are thinned for the current view

A series of a million points drawn into a plot a few hundred points wide cannot show a million distinct positions. The scene compiler therefore thins a line or a scatter that holds more points than its plot rectangle can resolve, before the geometry reaches the display list. Because the thinning happens in the compiler rather than in a backend, the canvas and the PDF draw the same thinned series, and the figure that is exported is the figure that was on screen.

The target is four drawn points per point of plot width, with a floor of 256 points, and it is not configurable: it is a fidelity-preserving optimisation rather than a choice about how a figure looks, and a knob would let a figure be exported at a different fidelity from the one it was designed at. A series no longer than the target is drawn in full, including the parts of it that lie outside the axes.

Thinning is applied in figure space, so it follows the view. The axis limits, the axis scales, the plot rectangle and, in three dimensions, the camera all decide which points survive, and the compiler runs again whenever any of them changes. Two rules are used:

- A **line** is thinned by the largest-triangle-three-buckets rule, which divides the points into consecutive buckets and keeps from each the one that forms the largest triangle with its neighbours. It keeps the peaks, troughs and corners that carry the shape of a curve, which taking every *n*th point loses. The first and last points always survive, and each run of drawable points is thinned on its own, so a non-finite value still breaks the line rather than being smoothed over.
- A **set of markers**, whether a scatter or the markers of a line, is thinned by keeping one marker per square of half a marker width, the one that would be painted over the others. The cost of a dense scatter is then set by the area of its plot rather than by the size of its data.

Segments and markers that the axes clip away are dropped before either rule is applied, so the whole budget is spent on what the reader can see: zooming into a thousand points of a million draws those thousand in full.

### The hit map

The hit map is the geometry that the viewer needs to relate a pointer position to the figure: for each axes, its plot rectangle and, in two dimensions, the mapping between data values and figure coordinates along each axis; for each legend entry, its rectangle and the artist it represents; and, for each line and scatter, the points it drew.

Each drawn point carries its position in figure space together with the index it has in the artist's own data arrays, in one value, so that the two cannot be separated or fall out of step. The index is the one the user's data uses, never a position in the thinned series, which is what lets the viewer's [datatip](../guides/viewer.md#datatips) name the measurement the reader is pointing at however hard the series was thinned.

### The backends

- The **canvas** tessellates paths with lyon (splitting dashed strokes into dashes first) and glyph outlines into triangle meshes, which egui draws with its wgpu renderer using four-times multisample anti-aliasing. The figure is drawn at its physical aspect ratio, scaled to fit its tab.
- The **offscreen renderer** draws the same meshes into a texture without a window and reads the image back. The documentation gallery's images are made this way, so they show exactly what the viewer shows.
- The **PDF backend** writes each path and glyph run to a single page whose MediaBox and CropBox equal the figure size, with fonts embedded as subsets. The decisions behind PDF-first export are recorded in [ADR 0004](../adrs/0004-pdf-first-export-with-krilla.md).

## Interaction records edits in an overlay

The viewer has no view state of its own that affects drawing. Each gesture is converted into a transaction of typed edits of the figure model, in the same way that setting a property from the API edits it:

- panning and zooming a two-dimensional axes set its axis limits to manual values;
- panning, zooming and rotating a three-dimensional axes set the properties of its `projection.view3d`;
- clicking a legend entry sets the `visible` property of an artist;
- changing a property in the property editor sets that property.

Limits are set with the `set_limits` command, which reads the figure and returns the limits of every axes of the link group as literal edits, so linked axes follow without the viewer implementing the rule. An axes of the group that cannot show the limits makes the transaction fail, and the figure is left unchanged.

A tab holds the figure as its owner defines it (the source, which for the viewer is the figure as opened or as last saved) and the user's edits (the overlay), and draws their composition, which is recomputed whenever either changes. A gesture therefore never mutates the source: undo and redo step through the overlay, double-click and Reset view discard the view entries of the overlay, and saving folds the overlay into the source. An overlay entry that the source cannot accept is dropped when the composition is made, and the reason is shown by the problems indicator.

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
3. Mathematics segments are parsed and laid out by latex-rust against STIX Two Math. Before layout, hyphens are replaced by minus signs and unstyled letters by their mathematical italic forms, as TeX does. Layout runs on a helper thread with a large stack, and deeply nested input is rejected before it reaches the parser, so that no input can overflow a stack.
4. The resulting box tree is converted into positioned glyphs and rules in points, and the segments are placed on a shared baseline.

Resolved layouts are memoised in memory, keyed on the source, the interpreter and the size, so each distinct label is typeset once per process. Mathematics that cannot be typeset is drawn as its raw source in the text font, and a warning is recorded, so a label never prevents a figure from being drawn. Glyph identifiers always refer to the bundled font files that both backends draw with. The reasons for embedding a typesetter, and for storing source rather than glyphs, are recorded in [ADR 0005](../adrs/0005-embedded-latex-math-with-latex-rust.md).
