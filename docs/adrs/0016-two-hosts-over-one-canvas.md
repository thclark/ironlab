# ADR 0016: Two hosts over one canvas: the native egui viewer and figures embedded in HTML

**Status:** Accepted

**Supersedes:** [ADR 0002](0002-egui-viewer-on-rerun-family-crates.md)

**Related:** [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0010](0010-pdf-export-with-a-raster-fallback.md), [ADR 0013](0013-depth-buffered-three-dimensional-artists.md), [ADR 0014](0014-wgpu-canvas.md)

## Context

[ADR 0002](0002-egui-viewer-on-rerun-family-crates.md) built the viewer with egui and eframe on wgpu, confined its user-interface dependencies to the Rerun family of crates, and noted among its consequences that "eframe also compiles to WebAssembly, which keeps a browser viewer possible without a second user-interface implementation". [ADR 0007](0007-mvp-scope.md) deferred that browser viewer. Since then the canvas has become a set of wgpu pipelines of its own ([ADR 0014](0014-wgpu-canvas.md)), egui drawing only the interface around it, and the PDF exporter has been split so that the renders it needs are performed by whoever holds the device ([ADR 0010](0010-pdf-export-with-a-raster-fallback.md), [ADR 0013](0013-depth-buffered-three-dimensional-artists.md)).

Two needs now make a browser host worth building. The documentation at [ironlab.org](https://ironlab.org) is the primary showcase of the project, and its gallery showed every figure as a static image beside a PDF, which demonstrates the export and nothing of the interaction that distinguishes IronLAB from a plotting library. And IronLAB exists to produce figures for publications; a publication that is read on the web can carry the figure itself rather than a picture of it, so that a reader pans into the data, hides a series or rotates a surface, and exports the very PDF the author would have, without installing anything.

The question this decision answers is how the browser host is built, and in particular whether the eframe route that ADR 0002 foresaw is the right one.

## Decision

### The engine is one crate, and the compiler enforces the split

Everything that draws, interprets or exports a figure moves out of `ironlab-viewer` into a crate `ironlab-canvas`: the wgpu pipelines and tessellation of ADR 0014, the interaction state machine of [ADR 0008](0008-typed-edits-and-a-view-overlay.md), the `FigureCanvas` controller (which fits a figure into an area, turns decided gestures into typed edits, and reports the cursor, rubber band and datatip callout its host should draw), the offscreen renderer, PDF export through that renderer, and the reading and writing of figure files. It depends on egui's maths and colour crates, `emath` and `ecolor`, and on nothing else of egui.

The split is enforced rather than promised: continuous integration builds `ironlab-canvas` for `wasm32-unknown-unknown`, which fails the moment anything in it comes to depend on eframe, egui, a file dialog or any other native-only crate. A host, whichever it is, draws figures with this crate and nothing else.

### The native host is unchanged

`ironlab-viewer` remains the egui host that ADR 0002 chose: eframe, egui, `rfd` and the Rerun-family crates, drawing each figure through the canvas crate's pipelines inside the window's render pass. Nothing about ADR 0002's reasoning for the desktop changes, and the desktop viewer keeps its figure browser and property editor. What this record supersedes is ADR 0002's account of how a browser viewer would arrive.

### The browser host is built without egui

The browser host, `ironlab-web`, is a library built with wasm-bindgen (a `cdylib`, which wasm-bindgen consumes, and an `rlib`, so that its native tests and the workspace's lints build it) and marked `publish = false`, because its product is a bundle of files for a web page rather than a library for Cargo. It depends on `ironlab-canvas` and not on egui or eframe, for three reasons.

- eframe's web painter fixes multisampling at one sample per pixel, and the canvas relies on four: the strokes of ADR 0014 and the glyph outlines it tessellates are anti-aliased by multisampling alone, so a figure drawn through eframe on the web would not look like the figure in the window or in the gallery.
- egui would add megabytes to a bundle that a publication page loads, and an egui application owns its canvas: it captures the wheel and the keyboard over the whole element, which fights the scrolling and the shortcuts of the page around it.
- The controls of a figure in a page are wanted as HTML, so that they inherit the page's fonts and colours, are reached by assistive technology and are restyled with CSS, none of which an interface drawn into a canvas offers.

ADR 0002 foresaw a browser viewer without a second user-interface implementation. This decision takes the second implementation deliberately: the shell, meaning the toolbar and the chrome around the figure, is written twice, once in egui and once in HTML, and the engine is written once. The shell is the small part, as ADR 0002 itself argued when it declined to let a component library decide the stack.

### Hosts draw chrome only, and both decide gestures alike

Neither host interprets a figure. Each forwards the gestures it decides to `FigureCanvas`, which turns them into typed edits, and each draws what the canvas reports back: the pointer cursor, the rubber band of a zoom and the callout of a datatip. So that the two hosts decide gestures identically, the canvas crate carries a `Pointer` recogniser that turns raw presses, moves and releases of the primary button into the drag, click and double-click gestures egui decides, with egui 0.36's own thresholds for click distance, click duration and double-click delay. The egui host receives those decisions from egui; the browser host feeds pointer events to the recogniser and receives the same decisions.

### One path to pixels, through a surface on the canvas element

The single path to pixels of [ADR 0003](0003-shared-scene-compiler-and-display-list.md) and ADR 0014 is kept. In the browser the display list is compiled, tessellated and drawn by the same `GpuPainter`, through a wgpu surface on the page's `<canvas>` element, on WebGPU where the browser offers it and otherwise on WebGL2. The frame is drawn into colour and depth textures of the host's own with four samples per pixel and resolved into the surface, in the same render pass setup, `figure_pass`, that the offscreen renderer uses. The pixels a reader sees in a page are therefore the pixels of the gallery PNG.

The web backends offer only an opaque surface. A figure whose background is transparent is drawn over a backdrop colour that the host supplies, following the page's theme, rather than over the page itself.

### Devices are shared where the platform allows

Under WebGPU one device per page serves every figure, and the number of figures on a page is not limited by the graphics stack. Under WebGL2 the adapter is the canvas element's own rendering context, so each figure has a device of its own, and browsers cap live WebGL contexts at about sixteen per page. Figures are therefore created lazily, when their element comes near the viewport, through an `IntersectionObserver`, and released when their element is removed from the document. The gallery index keeps its static thumbnails for this reason, and each gallery page embeds one live figure.

### Export has full fidelity because the engine is asynchronous

The exporter never blocks on the GPU. Device creation and readback in the offscreen renderer are asynchronous, with blocking wrappers for native callers, and PDF export runs in the two phases of `ironlab-pdf`'s `raster_requests` and `render_with_rasters`: the exporter records the renders a page needs, the host performs them, awaiting each, on whichever device it has, and the exporter replays them into the document. The same flow runs natively, so **Export PDF…** in a page produces, on the reader's graphics device, the document the author's machine would produce, dense surfaces and verified three-dimensional axes included, and the export report's warnings are returned with the bytes, shown in the page and dispatched as an event.

### The element degrades to its fallback

A figure is placed in a page as `<ironlab-figure src="…">` with fallback content, by convention the PNG of the figure linking to its PDF. The fallback is shown until the first frame is drawn and is kept when the browser has neither WebGPU nor WebGL2, when the script fails to load or when the figure file cannot be read. A page with embedded figures is therefore a page that already works without them, and the live figure is an enhancement of it.

### The wheel is taken only by an activated figure

A figure zooms on the wheel only after a click or a touch has activated it, and it is deactivated by Escape, by focus leaving it or by a click outside it. Dragging and pinching always act. A figure that took the wheel on hover would trap the reader scrolling a page, and one that never took it could not be zoomed with a mouse; activation is the compromise, and it is the same one map and chart embeds have settled on.

### Distribution is a bundle of static files, beside the release and on ironlab.org

The bundle, assembled by `scripts/build-web.sh`, holds the element (`ironlab.js`), the wasm-bindgen output (`ironlab_core.js` and `ironlab_core_bg.wasm`) with its TypeScript declarations, the stylesheet, a README and the licence. It is attached to each GitHub release as `ironlab-web-<version>.tar.gz`, with a checksum beside it, and every run of continuous integration attaches the bundle it built as an artefact; a publication copies its files beside its figure files, which pins the version for as long as the publication is read. ironlab.org serves the current release under `/embed/` for its own pages and for anyone content to track the current version. There is no npm package for now, because the bundle has no dependencies for a package manager to resolve and a publication should not resolve anything at build time.

## Consequences

- There is one engine. A change to tessellation, a stroke shader, the interaction rules or the exporter reaches the window, the gallery, the page and the PDF together, and cannot reach one without the others.
- The shell exists twice, and a control added to the viewer's toolbar is added to the figurebar by hand, or deliberately left out. The property editor and the figure browser are left out of the page on purpose: a page holds one figure per element, and the author's properties are the author's.
- The bundle is 7.7 MB, or 3.8 MB compressed: the engine, wgpu with both of its web backends, the PDF writer and the text engine with the fonts and the LaTeX typesetter it compiles in, of which the fonts are under a megabyte. Two levers remain: loading the fonts at run time from beside the bundle instead of compiling them in, and dropping the WebGL2 backend once WebGPU is available in every browser a publication must support, which removes a second shader compiler from the module as well as the sixteen-context limit.
- Under WebGL2 a page is limited to about sixteen live figures, which lazy creation hides from a reader scrolling a long article but not from a grid of figures that are all in view at once.
- `ironlab-canvas` is a new crate on crates.io, and trusted publishing cannot claim a crate that does not exist, so its first publication is made by hand with a scoped token, as [commits and versioning](../conventions/git-commits-and-versioning.md#publication-to-cratesio) records, after which the release workflow publishes it like the others. `ironlab-web` is never published to crates.io.
- ADR 0002's choice of egui and eframe for the desktop, its confinement of the viewer's dependencies to the Rerun family, and its argument that the retained figure model keeps the viewer replaceable all still hold; the last is what this decision relies on. Only its expectation that a browser viewer would be eframe compiled to WebAssembly is withdrawn.
- The two backends of the browser draw the same figure, but a difference between a browser's WebGL2 and WebGPU implementations, such as the treatment of a scissor edge that ADR 0014 already tolerates, is a difference between two pages of the same site. The comparison tolerances of the offscreen tests are the ones the browser is held to.
