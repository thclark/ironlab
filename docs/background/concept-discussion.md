---
title: "Interactive scientific visualization engine in Rust"
source: "https://claude.ai/chat/7093d715-b93e-485d-8d79-cdefff03b0d2"
model: "claude-opus-5"
exported: 2026-09-17
---

# Human — Sep 16, 2026, 8:53 PM

I work in rust for scientific computing. I really miss MATLABs figure engine, where I can have an interactive figure - zoom in, data inspection, and basic property editing. What I've built is essentially SVG export, which is great because I can specify a font set, a width (eg for publication outputs) and a latex engine and I can output publication quality graphics straight away - MATLAB was never able to do that.

In rust, how might I go about building an application to do this - perhaps I'd connect to a socket to allow update of data or two-way connection/callbacks for data introspection, perhaps I could just write to figure files for one-way plot output.

WHilst built with rust, it wouldn't hurt if there were client libraries for other languages like python and c++ (and matlab/octave!).

Give me a choice of suggested top level architectures for how to do this.

# Claude — Sep 16, 2026, 8:55 PM

Short answer up front: the architecture question you're really facing isn't "socket or file", it's "what is the thing being transported". Get that right and all four transports below become cheap variations.

## The core decision: a retained figure IR with stable handles

What you miss about MATLAB isn't the window, it's handle graphics: every axes, line, text object has a stable identity you can `get`/`set` on, and the GUI property editor is just another client of that same mechanism. Your SVG exporter already has an implicit figure model inside it. Make it explicit:

- **A versioned, serialisable scene graph.** Figure → axes → artists, each node with a stable ID and a typed property bag. Data arrays referenced by handle, not inlined, so a pan/zoom never re-sends 10M points.
- **A shared layout/scale layer.** Tick generation, axis transforms, label metrics, legend placement. Both renderers call it, which is the only way your interactive view and your SVG export agree pixel-for-pixel.
- **Two backends over that IR:** IR → SVG (you have this) and IR → GPU scene.
- **Everything else is a delta stream over the IR:** `create_node`, `set_property`, `append_data`, `get_property`, `emit_event`.

Once that exists, a `.fig` file is a serialised IR, a socket session is an IR delta stream, and the GUI property editor is a local client issuing `set_property`. One model, four front doors.

## Architecture A: in-process library, viewer on a thread

`figure.show()` spawns a window in your process (winit + wgpu, or eframe). No IPC, no serialisation, direct access to your `ndarray` buffers, callbacks are just closures.

Good for: Rust-native users, fast iteration, zero deployment complexity.
Bad for: everything else. The windowing event loop wants the main thread on macOS, which fights any host that owns main (Python REPL, MATLAB, a solver's main loop). Batch/HPC jobs can't use it. Other languages can only reach it through FFI that now has to marshal GUI lifetimes. Your figure dies when the process dies.

## Architecture B: standalone viewer process + IPC (the one I'd pick)

A `figview` binary that owns all rendering and UI. A thin client library that serialises IR deltas over a Unix socket / named pipe locally, TCP when remote. The client can spawn the viewer automatically on first plot, or attach to a running one.

This is essentially Rerun's architecture, and it's worth reading their `ARCHITECTURE.md` as a reference design: the SDK encodes data with Apache Arrow, writes it to `.rrd` files or streams it over gRPC to a viewer, and the viewer is a native app built on wgpu and egui that can also be compiled to Wasm for the browser. Rerun itself is the wrong tool for you (it's a temporal log database, and it has no publication-grade vector export path), but the shape is exactly what you're describing.

Why it fits your constraints:
- Simulation crashes, figure survives. Long solver runs stream in updates.
- Language clients only need to speak the wire protocol, so Python/C++/MATLAB are tractable.
- Remote HPC works by port-forwarding a socket.
- The one-way file case falls out for free: the same delta stream, written to a file instead of a socket.

Costs: process lifecycle management (orphan viewers, reconnection, "which viewer owns this figure ID"), serialisation on every update, and you must design the protocol carefully because it becomes your compatibility surface.

## Architecture C: headless core + browser front end

The Rust core runs where the data is; the UI is a web page over a WebSocket. Two sub-variants: rebuild the UI in TS/canvas (duplicated effort, diverging appearance), or compile the same Rust viewer to Wasm and serve it (egui/eframe and wgpu both support this).

Strong for remote clusters and for notebook embedding, where "no local install, just a URL" is a real advantage. Weaker on desktop feel, GPU access varies, and you now own an HTTP server and its security story. Worth building as a *second* front end over Architecture B's protocol rather than as the primary.

## Architecture D: file-first, `.fig` + viewer

Library only ever writes a self-contained figure file. A separate viewer opens it, gives you zoom, inspection, property editing, and "export SVG" from within the GUI.

Dramatically the simplest: no sockets, no lifecycle, no threading, trivially reproducible, archivable alongside results, and the "client library" for any language is just a serialiser. You lose live streaming and callbacks into a running computation.

Given how cheap this is once the IR exists, I'd build it regardless, as the persistence layer of Architecture B.

## Suggested composite

Core crates: `fig-ir` (model + serde), `fig-layout` (scales, ticks, metrics), `fig-svg` (exporter, no GPU), `fig-render` (IR → GPU), `fig-viewer` (app), `fig-proto` (wire), `fig-client` (Rust client + C ABI).

Then: file-based (D) as the persistence and one-way path, socket (B) as the live path, wasm viewer (C) later if remote matters, in-process (A) as a convenience wrapper that just spawns the viewer and connects over a local socket.

## Cross-cutting choices worth deciding early

**Wire format.** Split control plane from data plane. Control (create/set/get/events) wants a schema'd, codegen-friendly format: Protobuf or Cap'n Proto, both of which hand you half your client libraries. Bulk numeric data wants Apache Arrow IPC: zero-copy from NumPy, first-class C++ support, and it survives the shared-memory optimisation later. JSON is tempting for debuggability; keep it as an optional encoding for the control plane only.

**Client bindings.** Don't hand-write the protocol four times. Write the encoder once in Rust, expose it as a `cdylib` with a C ABI, then thin bindings on top: PyO3 for Python (or a plain ctypes shim over the C ABI), a header-only C++ wrapper, and MATLAB/Octave via `loadlibrary`/`calllib` or a small MEX file. MATLAB is the awkward one; keep the C ABI small, flat, and free of callbacks-into-MATLAB if you can.

**Two-way callbacks.** Three levels, and pick the lowest one that meets your needs. (0) One-way push only. (1) Viewer queues events (point selected, region brushed, property edited), client polls or blocks on `wait_for_event()`. (2) Full bidirectional RPC where the viewer calls into client code. Level 1 gets you most of MATLAB's data-inspection feel and works in single-threaded hosts without a background interpreter thread, which levels the Python/MATLAB/C++ playing field. Level 2 means every client needs an event loop, and MATLAB will make you suffer for it.

**WYSIWYG export.** Your main technical risk. Two honest options: accept "interactive is a preview, SVG is the truth" and share only the layout layer, or make the GPU renderer consume the same vector primitives your SVG exporter emits (Vello is the natural fit here, being a GPU renderer for the same PostScript-style model that powers SVG, though it's still explicitly alpha; `vello_cpu` exists as a fallback for machines without compute shaders). The pragmatic middle ground: render interactively with whatever is fast, and have the "Export" button in the viewer call `fig-svg` on the current IR state, so the thing you publish always came from the vector path.

**Text and LaTeX.** Resolve LaTeX to vector paths once, at IR construction or first render, and cache them in the IR keyed by (source, font set). Then the viewer never shells out to a LaTeX engine, remote viewers work without a TeX install, and `.fig` files are self-contained.

**Large data.** Decimation (LTTB for line series, binning for scatter) belongs in the layout layer keyed on the current view transform, so the same code path shrinks your SVG output at export time. This is one of the places MATLAB's export genuinely fails, so it's worth designing in rather than bolting on.

# Human — Sep 16, 2026, 9:02 PM

what is an "IR"?

# Claude — Sep 16, 2026, 9:02 PM

Sorry, that's jargon I dropped in without unpacking. IR stands for **intermediate representation**: a data structure that sits between the thing that produces output and the things that consume it, so neither side has to know about the other.

The term comes from compilers. A compiler doesn't translate C directly into x86; it translates C into an IR (LLVM IR, say), then translates that IR into x86, ARM, or WebAssembly. Add a new language, and you only write a front end. Add a new CPU, and you only write a back end. Without the IR you'd need a separate translator for every language-CPU pair.

In your case the IR is a description of a figure: a tree of objects with properties, plus references to the data arrays. Something roughly like:

```rust
Figure {
    width_mm: 89.0,
    font_set: FontSet::Publication,
    axes: vec![Axes {
        id: NodeId(1),
        xlim: (0.0, 10.0),
        ylim: Auto,
        artists: vec![
            Line { id: NodeId(2), data: DataRef(7), color: ..., width: 0.8 },
            Text { id: NodeId(3), content: Latex("\\sigma_{xx}"), ... },
        ],
    }],
}
```

Note what it is *not*: it's not SVG, it's not GPU vertex buffers, it's not pixels. It's the figure described in terms of intent, before any decision about how to draw it.

That's what buys you the architecture. Your SVG exporter becomes IR → SVG. Your interactive viewer becomes IR → GPU. Your `.fig` file is that struct written to disk. Your socket protocol is edits to that struct sent as messages (`set property 2.color to red`, `append 500 points to DataRef(7)`). The GUI property editor is a widget that mutates a node and asks for a redraw. A Python client library doesn't need to know anything about rendering; it just needs to build this structure and serialise it.

The alternative, which is where most plotting libraries end up, is that the figure only exists implicitly, scattered across the rendering code as it runs. That works fine for one output format. It's why adding a second one usually means rewriting the whole thing.

The stable `NodeId` values are the other half of it, and they're what gives you the MATLAB feel specifically. MATLAB handles are exactly this: a name for a node in a retained figure structure that you can `get` and `set` on later, from a script or from the GUI, whichever happens to be holding it.

# Human — Sep 16, 2026, 9:06 PM

can we use modern tools for building the desktop app in rust? what's available that would be cross-platform?

# Claude — Sep 16, 2026, 9:07 PM

Yes, and the ecosystem is in decent shape now. The useful framing is that your viewer is really two layers with different requirements, and you should choose them separately:

1. **The shell**: menus, dock/tab layout, property inspector, data cursor tooltips, file dialogs. Conventional widget work.
2. **The canvas**: one large custom-painted region drawing your IR, needing to stay responsive while panning a few million points.

Most toolkit comparisons are about layer 1. For you, layer 2 is where the risk is, and it constrains which toolkits are viable (you need an escape hatch to raw GPU or raw 2D drawing inside a widget).

## Shell options

**egui + eframe** is the pragmatic default. Immediate mode, wgpu-backed, compiles to native and Wasm from the same source, and it has a proper escape hatch (`PaintCallback`) that hands you the wgpu render pass for your canvas. The strongest evidence is Rerun: their viewer uses wgpu over Vulkan/Metal/D3D12/OpenGLES/WebGL/WebGPU, with the GUI in egui via eframe on both native and web, which is your architecture almost exactly. It's actively developed (0.35 landed in June 2026, adding an inspection protocol and an MCP crate), and 0.34 switched font rendering from ab_glyph to skrifa + vello_cpu, which brought hinting and variable font support, relevant to you since your viewer needs to agree with your SVG font metrics.

Costs: immediate mode means you re-declare the UI every frame, which grates once you want undo/redo and complex retained state (though your IR already gives you a retained model to diff against, so this hurts less than usual). Styling is functional rather than beautiful. Every minor release has breaking changes.

**iced** is the other serious native option. Elm-style, retained, type-safe, with `iced_wgpu` and a shader widget for custom rendering. 0.14 shipped in December 2025; it's the toolkit behind the COSMIC desktop, which is meaningful production validation. The common sentiment is that egui is easier to start with but larger apps end up needing an architecture you have to invent yourself, whereas iced solves that upfront with its message pattern. Docs are thinner and the widget set is smaller. Still pre-1.0.

**Slint** is genuinely polished with a declarative DSL and a live-preview workflow, but check the licensing against your distribution plans first: it's multi-licensed (GPLv3, a royalty-free option with conditions, and paid commercial tiers), which is a real consideration for a scientific tool you may want others to embed.

**Xilem / Masonry** is the one to watch, not to bet on. It's built on Vello for rendering, wgpu, Parley/Fontique/Swash/Skrifa for text, AccessKit for accessibility and winit for windowing, and is explicitly alpha. That dependency list is precisely the stack a figure engine wants. Consider using those component crates directly today even if you skip the toolkit.

**Webview (Tauri / Dioxus desktop)** deserves more consideration than usual in your case, for one specific reason: a browser already renders SVG, with correct text layout, fonts, gradients, clipping and dash patterns. If your output format is SVG, a webview gives you exact WYSIWYG for free, plus DOM-level hit testing for data inspection (each point can literally be an element). The failure mode is data volume, so the realistic design is a hybrid: SVG for axes, text and annotations, a WebGL/canvas layer underneath for the data. You also inherit a JS/TS frontend and a bundled runtime, which sits awkwardly with "Rust scientific tool".

## Canvas options

- **Direct wgpu** in a paint callback. Fastest path for line strips and scatter with millions of points, since you can upload data once and re-render on pan/zoom by changing a transform uniform. Most work, most control.
- **Vello** for vector-accurate drawing: it fills the same slot as Skia or Cairo, drawing shapes, gradients, images and text with the same PostScript-style model that powers SVG and canvas, using GPU compute. It's still explicitly alpha, at 0.9, and requires a GPU with compute shader support. There's `vello_cpu` for machines without one.
- **tiny-skia + resvg** (CPU, mature, stable API). Worth taking seriously as your v1 canvas for a sneaky reason: you can render your own SVG exporter's output directly. Pan/zoom becomes "re-emit SVG at the new viewport, rasterise, blit". That's WYSIWYG by construction, and a working interactive viewer in a fraction of the time. It won't hold 60fps on huge datasets, but it will tell you very quickly whether the rest of your architecture is right.
- **skia-safe** if you want the battle-tested option and can tolerate the build.

## Supporting crates worth knowing

`winit` (windowing, used by all of the above), `kurbo` (Bézier geometry), `parley` + `swash` + `skrifa` + `fontique` (text shaping and layout, needed if your viewer must reproduce your SVG's text metrics exactly), `accesskit` (accessibility, which egui and Xilem already wire up), `rfd` (native file dialogs), `arboard` (clipboard, for copy-figure-to-clipboard).

## What I'd actually do

Start with **egui + eframe**, with the canvas as a `tiny-skia`/`resvg` render of your own SVG output. This gets a real app in front of you fast and forces the IR-to-SVG path to be correct from day one. Then, once the interaction model is settled, add a wgpu fast path for the data layer only, keeping the SVG path as the authoritative export and switching to it whenever the view is idle.

Two things this buys you. First, `cargo build --target wasm32-unknown-unknown` gives you the remote/HPC viewer nearly for free, which matters because forwarding a native GPU app over X11 or VNC from a cluster is miserable. Second, because everything renders from your IR, the toolkit choice stays reversible: if egui's immediate mode starts hurting at the property-editor stage, swapping the shell to iced is a rewrite of the chrome, not of the engine.

The real cross-platform cost, incidentally, isn't the toolkit. It's macOS code signing and notarisation, Windows SmartScreen, and Linux distribution. Budget for that separately, and look at `cargo-packager` or Tauri's bundler when you get there.

# Human — Sep 16, 2026, 9:20 PM

what about dioxus? It has a great looking component library...

# Claude — Sep 16, 2026, 9:21 PM

Dioxus is the most genuinely appealing option on pure developer experience, and the component library is real: Dioxus 0.7 shipped **Dioxus Primitives**, a first-party Radix-primitives implementation, and **Dioxus Components** is a shadcn-style styled layer on top of it, installed by copying source into your project with `dx components add button`. Nothing else in Rust has that quality of widget set. 0.7 also brought Subsecond hot-patching of Rust code at runtime, which is a real win for the fiddly UI work your property editor will involve.

The thing to be clear-eyed about is *why* it looks that good: those components are HTML and CSS. So the question isn't "is the component library nice", it's "what renders the HTML and CSS, and can I put my figure canvas inside it".

## The two rendering paths

**`dioxus-desktop`: system webview.** WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux. Mature, and it's the same deal as Tauri. Your canvas becomes `<canvas>` with WebGL/WebGPU, or, interestingly for you, SVG in the DOM. Costs: a Rust-to-DOM IPC bridge on every interaction, and WebKitGTK on Linux, which is the weak link if your users are on conservative cluster login nodes and old distributions. A figure viewer that fails to launch on a RHEL box is a figure viewer nobody uses.

**`dioxus-native` / Blitz: the WGPU HTML/CSS renderer.** This is the interesting one. Blitz combines work from Firefox (Stylo for CSS), Google, Servo and Bevy, with painting through Vello and accessibility through AccessKit. Dioxus's own release notes describe results often indistinguishable from Chrome or Safari, while also noting not every CSS feature is supported yet and listing live bugs. It's moving fast (0.7.10 in July 2026, 0.8 alphas since May 2026) but it's still young. Worth knowing: SVG rendering is a default-on feature of `dioxus-native`, so your exporter's output may be directly displayable.

## The question that actually decides it

Can you embed a custom wgpu-rendered region inside a Dioxus tree, and drive it at 60fps from Rust-side data?

In the webview path the answer is yes but you'll write JavaScript for the hot loop, because routing pointer events and viewport transforms through serialized IPC for every frame of a pan gesture is not going to feel like MATLAB. In the Blitz path I'd want you to verify this yourself rather than take my word for it. The direction Dioxus advertises is embedding Dioxus *into* a wgpu or Bevy app, which is arguably the arrangement you want anyway: your canvas owns the surface, Dioxus paints the chrome over it. But whether there's a clean custom-paint-node story today is exactly the sort of thing that changes month to month.

So: spend a day on a spike before committing. Render a 500k-point line series, pan and zoom it, measure frame times, on all three platforms you care about. Do the same in egui with a `PaintCallback`. That comparison will decide this far better than any feature matrix, including this one.

## The strategic argument in Dioxus's favour

Given your HPC angle from earlier, Dioxus is a web framework first. One component codebase gives you the desktop app *and* the browser-served remote viewer, with proper CSS styling and no Wasm-canvas compromises on the web side. egui also compiles to Wasm, but egui-in-a-browser always feels like a native app pretending, whereas Dioxus-on-web is just a web app.

## The argument against

Your component library is maybe 15% of this project. The IR, the SVG/GPU render agreement, the wire protocol, the Python/C++/MATLAB bindings and the data decimation are the other 85%, and none of them care which toolkit draws your toolbar. Picking your whole stack on the strength of the widget set optimises the small term, and the cost lands squarely on the part you can't compromise on, which is canvas throughput.

That said, this is exactly the risk your IR mitigates. If the viewer is a pure function of the IR plus a socket connection, building it twice is an expensive week, not a rewrite. If Dioxus is the thing that makes you actually enjoy building the property editor, that's not a trivial consideration for a side project that needs to survive contact with your day job.

My honest read: prototype the canvas in both, and let the frame times pick. If Blitz holds up for your canvas, Dioxus is the more pleasant long-term home. If it doesn't, egui wins on the strength of Rerun having already proven the exact shape of app you're building.

# Human — Sep 16, 2026, 9:28 PM

how does egui get styled? Are there component libraries that we can lift from?

# Claude — Sep 16, 2026, 9:29 PM

## How styling works

egui has no stylesheet. Styling is a single `egui::Style` struct living on the `Context`, with four parts that matter:

- `Visuals`: colours, strokes, corner radii, shadows, and a `WidgetVisuals` set per interaction state (`inactive`, `hovered`, `active`, `open`, `noninteractive`).
- `Spacing`: item spacing, button padding, indent, slider width, menu margins. This is what actually determines whether your app reads as dense-scientific or airy-consumer.
- `TextStyle` → `FontId` map: `Heading`, `Body`, `Monospace`, `Button`, `Small`, plus any custom styles you insert.
- `Interaction`: hover timings, tooltip delays, hit-test slop.

You set it once at startup with `ctx.set_style()` / `ctx.set_visuals()`, and override locally by scoping:

```rust
ui.scope(|ui| {
    ui.visuals_mut().widgets.inactive.weak_bg_fill = accent;
    ui.spacing_mut().button_padding = vec2(10.0, 4.0);
    ui.button("Export SVG");
});
```

Two useful facts. `Style` is serde-serializable, so you can ship themes as RON/JSON files and let users swap them. And egui's demo app includes a live style editor (`ctx.style_ui()`), so the practical workflow is to tweak sliders until it looks right, then serialise the result into your repo.

The pain point is exactly that scope dance. As one styling crate's README puts it, styling lives on `ui.visuals_mut()` which is global to the current `Ui`, so a custom hover colour on one button means cloning the visuals, mutating three `WidgetVisuals` states, and wrapping in a scope, which is correct but doesn't scale.

This is being worked on. egui 0.35 added **classes** as the first piece of CSS-like styling, letting you tag a container and have widgets respond to surrounding context:

```rust
ui.scope_builder(UiBuilder::new().with_class("my_container"), |ui| { ... });
```

The long-running design issue for this covers CSS-like selectors and theme files that could be live-loaded from a single text file, but that's a direction of travel, not something you can use today.

## What you can lift

There's no shadcn equivalent. What exists is a scattering of individual widget crates, several of which map suspiciously well onto your app:

**From Rerun, which is building your app already:**
- `egui_tiles` — tiling/docking panes. This is how you get multiple figures in tabs and split views.
- `egui_table` — large virtualised tables. Your data inspector.
- `re_ui` isn't published as a general-purpose crate, but it's their design system in the open Rerun repo under MIT/Apache. Read it. It's the closest thing to a worked example of "serious scientific egui app that doesn't look like a debug overlay".

**General:**
- `egui_extras` — `TableBuilder`, image loaders, date picker, syntax highlighting. Semi-official, ships in lockstep with egui.
- `egui_dock` — the other docking option if `egui_tiles` doesn't fit.
- `hello_egui` — a collection including drag-and-drop (`egui_dnd`), flexbox layout (`egui_flex`), material icons, animation helpers. Mixed maturity, honestly labelled as such.
- `egui_taffy` — Taffy (the flexbox/grid engine Blitz also uses) as an egui layout backend, if egui's linear layout starts fighting you.
- `egui_phosphor`, `egui-remixicon`, `egui_material_icons` — icon fonts, which do a lot of visual lifting for toolbars.
- `egui_styled` — Tailwind-style design tokens and per-widget hover/focus/active builders, wrapping the `visuals_mut` dance for you.
- `egui_notify` (toasts), `egui_file_dialog` or `rfd` (file pickers), `egui_commonmark` (help panes).
- Theme crates like `catppuccin-egui` exist if you want a ready palette rather than designing one.

For your object tree and property editor specifically, look at the derive-based inspector crates (`egui_probe`, `egui-struct` and similar) for interaction patterns, but don't adopt one. Your property editor must be driven by the IR's dynamic property bag so that it works over the socket on a figure the viewer didn't construct locally. Derive macros bind you to compile-time Rust types, which is the wrong shape.

## The real tax

Every one of these crates pins a specific egui version and republishes in lockstep (`egui_extras` 0.35 for egui 0.35, and so on). egui ships breaking changes each minor release. So an upgrade means waiting for the slowest crate in your dependency set, and a crate with one maintainer can strand you for months.

Mitigations: pin egui and upgrade deliberately rather than eagerly; prefer the Rerun-maintained crates, which track egui closely because Rerun sponsors egui development; and vendor anything small rather than depending on it.

## What I'd do for this app

Write a `tokens.rs` with your palette, spacing scale and type scale, and one `apply_theme(ctx)` that builds `Style` from it. That's an afternoon and it gets you 80% of the way to looking intentional. Take `egui_tiles` for figure docking and `egui_table` for the inspector. Hand-roll the property grid, since it's IR-driven anyway and a two-column label/editor layout is not where a component library saves you.

Worth remembering that in a figure application the chrome should be quiet. Your canvas is the visual centrepiece, and the rest of the UI succeeds by being dense, keyboard-efficient and out of the way. That's a much lower design bar than a consumer app, and it's the bar egui clears comfortably.

# Human — Sep 16, 2026, 9:35 PM

OK so rerun's styling isn't horrible, and it's certainly appropriate for a scientific app, so this is probably the way to go. We don't need tables (by data inspector i meant you can click to identify data points, not tabulate what you've got).

# Claude — Sep 16, 2026, 9:36 PM

Right, so the datatip/data cursor, not a table view. Drop `egui_table`, keep `egui_tiles`.

That changes the shape of the problem, because a data cursor is a canvas concern rather than a widget concern. A few things worth deciding early, because they're painful to retrofit.

**Hit-testing happens in screen space.** Nearest-point must be measured in pixels after the axis transform, not in data units, or log axes and non-square aspect ratios give you nonsense. For a sorted-x line series, binary search on x then check the two neighbours, which is trivial and covers most scientific plotting. For scatter and unsorted data, a lazily-built k-d tree or uniform grid per series, invalidated on data append. Brute force is fine to about 100k points if you reject early on bounding box.

GPU picking is the other option: render an ID buffer alongside the colour buffer and read back the pixel under the cursor. It gives you correct occlusion semantics for free, handles dense scatter and images uniformly, and costs one frame of readback latency, which nobody notices on hover. If you're already drawing the data layer with wgpu it's not much extra. Worth it if you expect image/surface plots, overkill if you're mostly line series.

**Decide snapping semantics.** MATLAB snaps to the nearest vertex by default and can interpolate along the segment. Offer both, because for a densely sampled signal "the value at this x" is often what you want, and for a sparse measurement series it definitely isn't.

**The index you report must be the original index.** If your decimation layer (LTTB, binning) is between the data and the screen, the picked point's index is an index into the decimated set. You need the decimator to carry an index map back to the source array. Retrofitting this later means touching every path. Get it in the decimator's output type from the first commit.

**Pinned datatips should be IR nodes.** This is the useful architectural line:

- *Hover tooltip*: pure viewer state, ephemeral, never in the IR.
- *Pinned datatip*: an annotation artist with a `NodeId`, anchored to `(series_ref, index)` plus a pixel offset for the callout box, with its own style properties.

That one decision buys you a lot. Pinned cursors export to SVG (so annotated figures go straight into a paper), they survive save/load in the `.fig`, they're creatable programmatically from Python, and they show up in the property editor like any other object. It's also exactly how MATLAB handles it.

**Format the datatip with your tick formatter.** The number shown in the cursor should use the same formatting the axis labels use, which means it's another consumer of the shared layout crate. If your x axis is showing dates, the cursor shows a date. If it's showing 1e-6 scaling with an exponent on the axis, the cursor should agree. This is the kind of detail that makes it feel finished.

**Selection state is one thing, not two.** This is the pattern worth stealing from `re_ui` specifically: Rerun keeps a single hover/selection state that every view reads from, and the selection panel just renders whatever is currently selected. Copy that. Then "click a data point" and "click an axes to edit its properties" go through the same mechanism, the property editor is a function of `selection: Option<NodeId>`, and hover highlighting across linked subplots is nearly free.

**The event path back to the client.** Click → hit test → resolve to source index → push `PointPicked { node, index, x, y }` onto the viewer's event queue → client polls. This is the level-1 callback design from earlier, and the same queue carries brush/lasso selections as index sets. Design the event payload to hold a set of indices rather than a single one from the start, even if v1 only ever emits one, so adding brushing later doesn't break the protocol.

A few interaction details that separate "has a tooltip" from "feels like MATLAB": the pinned callout should be draggable, arrow keys should step the cursor along the series, and the cursor should be able to snap to a different series with a modifier without moving the mouse.

# Human — Sep 16, 2026, 9:36 PM

can we do it solely on rerun-maintained crates?

# Claude — Sep 16, 2026, 9:36 PM

Pretty much, yes, and it's a stronger position than it sounds, because these aren't five independent maintainers who happen to be aligned. It's one release train.

Your GUI dependency set would be:

- **`egui` / `eframe` / `epaint` / `emath` / `ecolor`** — one workspace, released together. egui development is sponsored by Rerun, and emilk works there.
- **`egui_extras`** — same workspace, same cadence.
- **`egui_tiles`** — `rerun-io/egui_tiles`, by Emil Ernerfeldt, explicitly sponsored by Rerun. Worth noting it was built specifically because `egui_dock` only supports binary splits, whereas tiles does full horizontal/vertical and grid layouts, with behaviour overridable via a `Behavior` trait. For a figure application where users arrange subplot panes arbitrarily, that's the right one.
- **`egui_plot`** — extracted from the egui repo in 2024 and now co-owned by rerun.io. You're replacing it eventually, but it's a credible v0 canvas while you get the IR and socket layer working, and it's the best reference for egui-idiomatic pan/zoom/hover handling.

That's effectively a single upgrade decision rather than five, which kills the version-lockstep tax I flagged earlier. The crates that egui pulls in underneath (winit, wgpu, accesskit, skrifa, arboard) aren't Rerun's, but they're not your decision either since eframe brings them regardless.

**Where the family has gaps:**

- **`re_ui`.** It's on crates.io, but it's Rerun's internal design system: versioned with Rerun (0.34.x), not with egui, depends on other `re_*` crates, and carries no stability promise. Don't depend on it. It's MIT/Apache, so read it and vendor the parts you want, which is mainly the tokens, list-item primitives, section headers and collapsible panels. That's a few hundred lines you'll then own and can adapt to your property editor.
- **Icons.** Rerun ships its own SVG set inside `re_ui` rather than publishing an icon crate. Vendor theirs, or take `egui_phosphor` and accept one outside dependency.
- **Native file dialogs.** Nothing from Rerun. `rfd` is the standard answer and it's a small, stable dependency. Alternatively lean on drag-and-drop, which eframe handles natively, and skip the dialog for a while.
- **Your canvas renderer.** Rerun's `re_renderer` is their wgpu abstraction, also internal and shaped around point clouds and depth imagery. Not liftable, but readable.

**One thing you get for free by staying in this family:** egui 0.35 added an inspection protocol that exposes the AccessKit tree of a running app and accepts synthetic events, with an `egui_mcp` crate on top. For a GUI you need to regression-test (does the datatip still report the right index after a decimation change?), being able to drive the app programmatically is more valuable than it first appears.

**The one risk worth naming:** this is a lot of surface resting on one person's taste and one company's roadmap. Rerun funding it is real mitigation, and the licensing means a fork is always possible, but you should know you're making that bet rather than discovering it later. Given that your IR keeps the viewer replaceable, it's a bet I'd take.

# Human — Sep 16, 2026, 9:37 PM

definitely gpu picking; colour surfaces and images will be a near-immediate requirement

# Claude — Sep 16, 2026, 9:38 PM

Good call, and surfaces/images make it the right call rather than just a nice one, because occlusion between an image, a mesh and overlaid line series is exactly what analytic hit-testing handles badly.

## Pick pass mechanics

Render an ID target alongside colour. Two things make this much cheaper than the naive version:

**Scissor to the cursor.** You don't need a full-screen ID buffer. Set a scissor rect of maybe 32×32 around the pointer and run the pick pipeline over just that region. You're paying vertex cost for the scene but almost no fragment cost, and you can skip the pass entirely on frames where the pointer hasn't moved and no click happened.

**Read a window, not a pixel.** Copy the whole 32×32 back and spiral outward from centre to find the nearest non-zero ID. This is what gives you hit tolerance on 1px lines, which is otherwise miserable. Combine it with widening geometry in the pick pipeline only: render lines at `max(visual_width, 8px)` and markers at an inflated radius, so thin things are easy to grab without changing their appearance.

Readback is `copy_texture_to_buffer` then `map_async`, so you eat one frame of latency and want a small ring of staging buffers to avoid stalling. Nobody perceives one frame on hover.

**Format.** `Rg32Uint` gives you 32 bits of object slot plus 32 bits of element index, which is the natural split. Integer targets can't be blended or multisample-resolved, so the pick pass must be non-MSAA. Since it's a separate pass anyway, that's free. Order your draws identically to the colour pass and let later writes win, and occlusion matches what the user sees.

One compatibility note given your Wasm ambitions: the WebGL2 fallback path is fussier about integer render targets. Keep an `Rgba8Unorm` colour-encoded fallback with 24-bit IDs in your back pocket. It's a shader variant, not an architecture change.

**ID allocation.** Don't put your IR `NodeId` on the GPU. Build a dense `Vec<NodeId>` per frame from the draw list, index it with the slot, and reserve 0 for background.

## Element index, per primitive type

The clean rule is: *the pick buffer answers "which object", and the CPU answers "where within it" whenever the object has an invertible mapping.*

- **Images and regular grids**: write only the node ID. You know the placement transform, so invert the cursor position on the CPU to get exact `(i, j)`. Better than encoding texel indices, which overflow 32 bits on large arrays anyway.
- **Markers**: instance index, but upload your decimation index map as an instance attribute so the shader writes the *original* source index directly. This is where that index-map decision from earlier pays off.
- **Line strips**: per-vertex index attribute with `@interpolate(flat)`, which gives you the provoking vertex. Then refine on the CPU by projecting the cursor onto the segment, which is also how you implement interpolated (non-snapping) cursors.
- **Meshes/surfaces**: flat-interpolated per-vertex index rather than relying on a primitive-index builtin, then barycentric refinement on the CPU against the three vertices.

## What surfaces and images do to the IR

**Colormap, clim and scaling become IR properties**, not baked pixels. Upload `R32Float` (or `R16Float`) texture data and apply the colormap as a 1D LUT in the shader. Three payoffs: dragging clim is a uniform update so it's interactive for free, the datatip can report the underlying physical value rather than a colour, and NaN/out-of-range handling becomes a property (`NaN → transparent`, over/under colours) instead of a preprocessing step.

`R32Float` isn't filterable without the `float32-filterable` feature, so use nearest sampling. For scientific data that's the correct default anyway; silently bilinear-interpolating someone's measurement grid is a misfeature.

**Size limits bite early.** `max_texture_dimension_2d` defaults to 8192. Scientific images routinely exceed that, so plan for a tiled texture path before you have a user with a 20k×20k array.

**A colorbar is an IR node** linked to a colormap and clim, not a separately-constructed artist. Otherwise it desynchronises the moment someone drags the clim.

## The export problem, which is the real one

This is where your publication-quality promise meets its limit, so decide the policy deliberately:

- **Images and dense surfaces cannot be vectors.** Embed a base64 PNG in an SVG `<image>` element, with axes, ticks, labels and annotations still vector. That's what matplotlib does and it's the right answer.
- **Sparse grids can be.** Below roughly 10k cells, emitting one filled path per cell is smaller, sharper and infinitely zoomable. Pick the backend by cell count, with a user override.
- **Gouraud shading has no SVG equivalent.** SVG 1.1 has no mesh gradients and SVG 2's are effectively unimplemented. So: flat-shade per triangle, subdivide until flat shading is imperceptible, or rasterise. Note that PDF *does* have type 4/5 shadings, which is a genuine argument for a PDF backend later rather than SVG-then-convert.
- **Render the raster with the GPU at export resolution and read it back.** This is the trick worth taking: don't reimplement colormap application on the CPU for export. Render offscreen at the target DPI using the same pipeline, read it back, embed it. Guaranteed identical to screen, no duplicated code, and DPI becomes an export parameter rather than a rendering mode.

## Event payload

Make it an enum now, not a struct with optional fields:

```rust
enum PickEvent {
    Points { node: NodeId, indices: Vec<usize> },
    Pixel  { node: NodeId, i: usize, j: usize, value: f64 },
    Vertex { node: NodeId, tri: usize, bary: [f32; 3] },
}
```

Adding a variant is a protocol minor version. Adding a field to a struct that three language bindings already parse is not.

# Human — Sep 16, 2026, 9:46 PM

I'm fine with pdf first if that eases shading issues. It basically has to be something you build into latex trivially

# Claude — Sep 16, 2026, 9:47 PM

PDF-first is the right call, and not only for shading. `\includegraphics{fig.pdf}` works natively in pdflatex, xelatex and lualatex, whereas SVG needs the `svg` package shelling out to Inkscape with `--shell-escape`, which half the journal build systems forbid. You also get CMYK and PDF/X for print workflows, which SVG simply can't express.

## Crate

**krilla**, which sits on top of **pdf-writer** (both from the Typst ecosystem, currently 0.7). It gives you path fill/stroke, affine transforms, clip paths, alpha and luminosity masks, blend modes and layer isolation, both high-level text and low-level positioned-glyph APIs, strong OpenType support with proper CFF and TTF subsetting, linear/radial/sweep gradients and patterns, image embedding, PDF 1.4 through 2.0, and PDF/A and PDF/UA validated export modes.

The maintainer also makes a point that's worth internalising: PDF viewers are tolerant in what they accept, so a file that renders fine in the one viewer you tested can fail everywhere else, which is why testing was a primary design goal for the crate. That applies to you too, and I'll come back to it.

**The catch, and it's the one that bears directly on your reason for choosing PDF:** krilla explicitly lists complex colour spaces and shadings as out of scope, exposing only a relevant subset of the spec. So type 4/5 Gouraud mesh shadings and CMYK/Separation colour probably aren't in the high-level API. You'd drop to `pdf-writer` for those, writing the shading dictionaries and the mesh data stream yourself, or upstream a contribution. Check the current API before committing, since this is exactly the kind of thing that gets added.

## Shading reality check

Type 4/5 (free-form and lattice-form Gouraud triangle meshes) and type 6/7 (Coons and tensor patches) are genuinely in the spec and genuinely the right representation for a modest FEM mesh. But the threshold logic from the surface discussion still applies, just with a better ceiling:

- **Under a few thousand triangles**: mesh shading. Infinitely zoomable, small file, looks correct.
- **Above that**: the file balloons and viewers crawl. Rasterise at export DPI and embed as an image XObject, keeping axes, ticks, labels, colorbar and annotations vector.
- **Images and dense grids**: always raster, `FlateDecode` for lossless, and set `/Interpolate false` so viewers don't bilinear-smooth someone's measurement grid.

The GPU-render-then-read-back trick is still the way to produce those rasters, so DPI stays an export parameter rather than a separate code path.

## Making it drop into LaTeX cleanly

- **MediaBox equals the artwork, exactly**, in PDF points (1/72 inch). No page margins. Set `/CropBox` to match, since some tools read one and some the other. Your existing "specify a width in mm" feature maps straight onto this.
- **Never scale in `\includegraphics`.** The whole promise is that 8pt text in the figure is 8pt on the page. Emit at final size and tell users to include it unscaled.
- **Embed the document's font.** Latin Modern and the TeX Gyre families ship as OTF, so krilla can consume them directly with no Type1 `.pfb` pain.

## The LaTeX text fork, which you should decide now

Three strategies, and the choice reaches back into your layout layer:

**A. Self-contained.** Run a TeX engine at figure-build time, extract the glyph runs, embed them with the subset fonts. The PDF is independent and works anywhere. Font sizes and macros don't track the host document.

**B. PGF/TikZ backend.** Emit `.pgf` and let LaTeX draw everything. Perfect font match, but PGF can't express your mesh shadings or large images well, and compile times get ugly.

**C. Split output.** A graphics-only PDF plus a companion `.pdf_tex` file that places LaTeX-typeset text over it, which is what Inkscape's "omit text in PDF" mode produces. Shadings and images live in the PDF; text is real LaTeX with the document's fonts, sizes and macros available. Two files instead of one, and you must know text extents at layout time.

I'd ship A as the default and C behind a flag, because C is what people writing a thesis actually want once they notice the figure font doesn't match the body text.

The architectural consequence is worth acting on immediately: **make text measurement a trait with two implementations**, one reading font metrics directly and one querying a TeX engine for box dimensions. Layout calls the trait. Retrofitting that later means touching every axis, legend and annotation placement calculation.

## Keep SVG anyway

PDF and SVG share the PostScript imaging model, so a single `VectorBackend` trait (`move_to`, `curve_to`, `fill`, `stroke`, `clip`, `push_transform`, `draw_glyphs`, `draw_image`, `draw_shading`) covers both. Divergence is confined to shading, text encoding and colour spaces. SVG stays nearly free, and it's what people want for Inkscape touch-ups, web figures and slides.

## Testing, given the viewer-tolerance problem

Rasterise your own PDF output with at least two independent engines (pdfium and mupdf, or Ghostscript) and diff against the GPU render. That single test catches both "my PDF is subtly invalid" and "my interactive view has drifted from my export", which are your two highest-consequence bug classes. Add a CI job that compiles a minimal `.tex` file including a generated figure, so a malformed PDF fails the build rather than a user's submission deadline.

# Human — Sep 16, 2026, 9:52 PM

For Latex text, we'd do A only. It's an unholy nightmare scaling your own fonts to a figure layout; if people need different fonts we'll build them in.

# Claude — Sep 16, 2026, 9:53 PM

Agreed, and it's the choice that keeps the system honest. The figure is a self-contained artifact whose appearance is a function of the IR alone, which is what makes `.fig` files reproducible, remote viewers possible, and regression testing meaningful. Option C would have made every figure's appearance depend on the document including it, which quietly breaks all three.

So the two-implementation measurement trait goes away. What replaces it is narrower and clearly worth having:

```rust
trait MathTypesetter {
    fn typeset(&self, src: &str, size: Pt, set: FontSetId) -> Result<GlyphRun>;
}

struct GlyphRun {
    glyphs: Vec<(GlyphId, Point)>,   // font-relative positions
    font: FontRef,
    width: Pt, height: Pt, depth: Pt, // above and below baseline, separately
}
```

## The remaining fork inside A

Do you shell out to a TeX engine, or embed a math typesetter?

**Shell out** (run `latex` on a stub, parse the DVI for glyph positions, which is what matplotlib's `usetex` does) gives you the full macro language, `\newcommand`s users already have, `siunitx`, everything. The cost is a hard dependency on a TeX Live installation, a few hundred milliseconds per unique string, and output that depends on the user's TeX version.

**Embed** gives you a tool that runs anywhere with no external install, which matters a lot on cluster nodes and for the "pip install and plot" experience your Python users will expect. The realistic route is an OpenType MATH table layout engine (ReX is the Rust prior art; Typst's math engine is the other, and conveniently already feeds krilla). You lose arbitrary macro expansion and support a large-but-finite subset of LaTeX math.

I'd shell out first, because correctness is easier to reach and your early users are TeX users anyway, but keep the trait boundary clean so an embedded engine can land later as the default with shell-out as the escape hatch for exotic macros. That's a genuinely different tool at that point, and probably the thing that makes people switch.

**Either way, cache and inline.** Key on `(source, font set, size, engine identity)`, cache to disk, and store the resolved `GlyphRun` in the IR. Then a `.fig` opened on a machine with no TeX still renders correctly, the socket protocol never carries LaTeX source that the viewer would have to typeset, and reopening a figure is instant.

## Font sets, since you're building them in

The key point, which you're already doing: the IR property is a *set*, not a family, because text and math fonts must be chosen as a pair or the result looks wrong. Worth having from day one:

- **Latin Modern + Latin Modern Math** — the TeX default look.
- **TeX Gyre Termes + Termes Math**, or **STIX Two Text + STIX Two Math** — Times-alike, which is what IEEE and most Elsevier journals want.
- **TeX Gyre Heros + a sans math pairing** — Helvetica-alike, for Nature-family and presentation figures.

Check licences before bundling: the GUST fonts (Latin Modern, TeX Gyre) are under the GUST Font License, which permits redistribution, and STIX Two is OFL. Both are fine, but you want that written down in the repo rather than discovered by a user's legal team. Math fonts are large (STIX Two Math is a couple of MB), so krilla's subsetting is doing real work for you here.

## Two details that decide whether it looks right

**Use depth, not just height.** A TeX box has width, height above baseline and depth below it. Axis labels align on the baseline; centred display math aligns on the math axis; nothing useful aligns on the bbox centre. Getting this wrong is the difference between "looks like MATLAB" and "looks like a paper".

**Fast-path the strings that aren't math.** Most axis labels are plain text. Route only strings containing `$` or a backslash through the typesetter and shape the rest directly with rustybuzz/skrifa against the same font. On a figure with thirty tick labels that's the difference between instant and a visible pause.

One small addition worth making now: record the typesetter identity and font versions in the `.fig`. Regenerating a figure identically three years later is a real scientific requirement, and it costs you two strings.

# Human — Sep 16, 2026, 9:57 PM

I'm inclined to embed a typesetter; are there rust based math typesetters I can bring in directly? worst thing is to have to have users installing a typesetter

# Claude — Sep 16, 2026, 9:58 PM

Good news: there are real options now, and the ecosystem moved a lot in the last year.

## The selection criterion that matters most

Demand access to the **box tree or positioned glyph runs**, not just rendered SVG. If a crate only hands you SVG paths, you lose real text in the PDF: no copy-paste, no search, no font subsetting through krilla, and a much bigger file. You want `(GlyphId, position, font)` triples plus width/height/depth so you can feed krilla's low-level positioned-glyph API directly, and feed the same run to your GPU renderer as cached glyph outlines.

That single requirement eliminates about half the candidates.

## Candidates

**`latex-rust`** is the closest fit I've seen to your spec. Pure Rust, no JavaScript, no webview, no runtime. It parses to an AST, lays out with a TeX-faithful box model whose dimensions are exact rational `Dim` values with no hardware f32/f64 in layout arithmetic, and renders to SVG, PNG via tiny-skia, or **egui shapes**. It reads OpenType MATH metrics (σ1 x-height, σ2 quad, math axis height, fraction rule thickness, the whole Appendix G parameter set) through `ttf-parser`, and ships STIX Two Math under OFL.

Two things stand out for scientific use. `layout()` returns a `MathBox` with width, height and depth, which is exactly the interface your layout layer wants. And unsupported constructs return `Error` rather than a fake render, with the project's contributing rules stating outright that golds are the contract and code gets fixed to pass them rather than the reverse. A silently wrong equation in a published figure is the worst possible failure mode here, so that policy is worth a lot.

Caveats: it's young and appears to be one author, and symbol coverage is locked to a specific list. Check that `MathFont` loads arbitrary MATH fonts rather than only the bundled STIX Two, since your font sets need Latin Modern Math and TeX Gyre Termes Math too.

**`ReX`** is the original Rust prior art and still the best-known name, MIT/Apache, OpenType MATH based. But its README says plainly that it's in heavy development and not intended for use beyond testing and debugging, and its font pipeline involves running a `rexify.py` script and a FontForge re-encoding pass to generate glyph metrics, which is not something you want in your build. Check whether a maintained fork has moved past this before dismissing it, but the upstream posture is a warning.

**`katex-rs`** is a Rust reimplementation of KaTeX tracking 0.18.5, with macros, strictness modes and the rest, targeting server-side rendering and Wasm. But it emits HTML and MathML, which is the wrong output shape for you. Its value is as a *front end*: KaTeX's parser and macro semantics are battle-tested against a decade of real documents.

**`pulldown-latex`** similarly converts LaTeX math to MathML, aiming at latex2e and amsmath compatibility and explicitly trying to resemble what pdflatex, KaTeX or MathJax produce. Again a parser, not a layout engine.

**Typst's math engine** is the most mature Rust math layout in existence and already feeds krilla, which would give you a suspiciously clean pipeline. The problems are that it's Typst syntax rather than LaTeX, the crates aren't a stable public API, and depending on it means pulling in a whole document system. Worth watching, worth reading, not worth depending on yet.

## What I'd do

Take `latex-rust` behind your `MathTypesetter` trait, and spend a day first on a coverage spike: pull every math label out of your last two papers, run them through, and count the `Err`s. That tells you in an afternoon whether the symbol coverage meets your actual needs, which no README can.

Keep the shell-out-to-TeX implementation as a second impl behind the same trait. Not as the default, but as the escape hatch for the user with `\newcommand{\Reyn}{\mathrm{Re}}` in their preamble. It costs you a few hundred lines and it means "unsupported macro" is a config change rather than a dead end.

Two supporting pieces either way. Non-math text still needs shaping, so `rustybuzz` + `skrifa` handles your axis labels and the fast path we discussed. And log every unsupported command to a counter you can inspect, because after a hundred figures you'll know exactly which twenty commands are worth implementing rather than guessing.

The caching design makes this decision much less scary than it looks. Once a `GlyphRun` is resolved and inlined into the IR, the figure is correct forever regardless of what happens to the typesetting crate. You're betting on a young dependency for *new* figure creation only, which is a recoverable bet.

# Human — Sep 16, 2026, 10:37 PM

take latex-to-rust but remove shell-out-to-tex. No odouble solutions, building an MVP here and that's an extremely niche requirement

# Claude — Sep 16, 2026, 10:37 PM

Right call. And it simplifies more than just removing one code path.

**Drop the trait too.** With one implementation, `MathTypesetter` is speculative generality. The seam that actually protects you is the `GlyphRun` type, not an abstraction over producers of it. One module, one concrete function:

```rust
pub fn typeset(src: &str, size: Pt, font: &MathFont) -> Result<GlyphRun, TypesetError>
```

If you ever swap engines, you rewrite that function's body. The rest of the system only knows `GlyphRun`.

**Use `layout()`, not the egui renderer.** latex-rust offers egui shapes as an output, which is tempting for the viewer. Don't take it. That's a second path to the same pixels, and it'll drift from the PDF path exactly the way the WYSIWYG problem predicts. Go `layout()` → `MathBox` → your `GlyphRun` → both backends. One code path, same rule you just applied.

**Decide the error policy now, because there's no fallback behind it.** Unsupported constructs return `Err`, and that error now reaches the user. The wrong answers are failing the figure (nobody should lose a 12-hour run to a tick label) and silently dropping the text. Render the raw source in monospace with a visible warning marker, surface it in the viewer's problem list, and emit it on the event channel. The figure still builds, and the user can see precisely what didn't work.

**The coverage spike is now gating, not diligence.** Previously a failure meant falling back; now it means picking a different engine. So do it first, before any integration code: dump every math string from your last two papers into a file, run them through, count the errors, and look at what they are. An afternoon, and it decides the whole plan.

**Fonts.** Check whether `MathFont` loads arbitrary OpenType MATH faces or only the bundled STIX Two Math. If it's the latter, ship STIX Two only for the MVP. That's a Times-alike, which is what most journals want anyway, and adding Latin Modern Math becomes a later feature rather than a blocker. Either way, pin the version and be prepared to vendor the crate — it's MIT/Apache, and a fork is a legitimate outcome if it goes quiet.

**You can probably cut the disk cache too.** An in-process engine typesetting thirty axis labels is fast; the caching design was mostly insurance against a few hundred milliseconds of subprocess spawn per string. An in-memory memo keyed on `(source, font, size)` is enough for the MVP.

What stays, and isn't optional: **inlining the resolved `GlyphRun` into the IR**. That was never about speed. It's what makes a `.fig` render on a machine without your typesetter version, keeps LaTeX source off the wire protocol, and means a remote viewer never needs the engine at all.

The payoff for the whole decision is the one that matters for adoption: `pip install`, plot, get a publication-quality PDF with real math, on a cluster node with no TeX Live and no admin rights.

# Human — Sep 16, 2026, 10:55 PM

"It's what makes a `.fig` render on a machine without your typesetter version" doesn't make sense when we're building in our typesetter

# Claude — Sep 16, 2026, 10:56 PM

You're right, that was a stale justification. I carried it over from the shell-out design without rechecking it, and with the engine statically linked every argument in that sentence collapses. Worse, two of them invert: sending LaTeX source over the socket is *cheaper* than sending resolved glyph runs, and inlining at IR-construction time would mean the Python client needs the typesetter, which is exactly what you don't want.

There's also a flaw in the idea itself. Glyph IDs are only meaningful against a specific font file. An inlined `GlyphRun` without an embedded font subset doesn't freeze appearance, it just moves where the ambiguity lives. So it gives false confidence too.

Corrected MVP design:

- **The IR stores LaTeX source**, as a string property like any other.
- **The renderer resolves it lazily** and memoizes on `(source, font_set, size)` in process memory. Both backends and the GPU path read from the same memo.
- **Clients never typeset.** They send source. The viewer owns text resolution entirely, which also means the property editor can let someone retype a label and see it update without a round trip.

What survives is smaller and cheap: record the engine version and font identity in the `.fig` header. Two strings, and it means a figure that renders differently in 2029 is diagnosable rather than mysterious.

If archival reproducibility ever becomes a real requirement rather than a nice idea, the answer isn't inlining glyph runs. It's a "freeze" export that embeds font subsets alongside the resolved runs, which is a deliberate feature with a real cost, not something to do by default. Note that your PDF export already *is* that artifact, since krilla embeds the subsets. So the frozen form exists; it's just called a PDF.

# Human — Sep 16, 2026, 10:56 PM

also, who in hell is pip installing? I'm building in rust to avoid the whole mess of python

# Claude — Sep 16, 2026, 10:56 PM

Fair hit. I reached for it as shorthand for "installs without a system TeX" and picked the worst possible example given what you're building.

Though to be precise about where it came from: you asked for Python, C++ and MATLAB clients in your opening message. So `pip install` isn't invented, it's the delivery mechanism for a client library you specified. But it's a client concern, and I dragged it in as the justification for a decision that's actually about the Rust side.

The correct statement of the payoff is just: **`cargo add`, no system TeX, real math in the PDF.** That's the whole argument. Static binary, no TeX Live, no `--shell-escape`, no "works on my machine because I have MacTeX". On a cluster node with no admin rights, a self-contained binary either runs or it doesn't, and yours does.

The client libraries stay worth building, and they're a nicer story than usual precisely because of this. The wire protocol carries LaTeX source as strings. The viewer owns typesetting. So a Python or MATLAB client is a thin thing that serialises arrays and property values over a socket, with no TeX, no fonts, no rendering, and no native build beyond the C ABI shim. That's a much smaller surface than any Python plotting library, and it's the direct consequence of putting the engine in the viewer rather than the client.

Worth keeping that in the MVP scope discussion, though: none of the client libraries are MVP. The Rust API, the IR, the viewer and the PDF backend are. Sockets and bindings come after you've used the thing yourself for a few real figures and found out what the API actually wants to be.

