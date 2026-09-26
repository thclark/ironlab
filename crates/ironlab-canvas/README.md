# ironlab-canvas

The figure canvas of [IronLAB](https://ironlab.org), an interactive plotting tool for scientific computing in Rust.

The canvas is the engine that every IronLAB host draws a figure with. It is a deliberately simple consumer of the scene compiler: it turns the display list of a figure into one draw list of triangles, stroke segments, marker instances and image tiles, and draws that list through its own wgpu pipelines, on screen inside the render pass of a host and offscreen into an image without a window. It maps pointer gestures onto typed edits of the figure model, recorded in a view overlay rather than applied to the figure it was given, so that undo, redo, saving and PDF export all act on what is on screen. It exports a figure as a PDF, rasterising through the same pipelines whatever the PDF exporter cannot keep as vectors, so that the pixels in a PDF are the pixels of the screen. It builds the object tree and the properties that a property editor shows, and it encodes and decodes `.fig` and `.json` figure files.

Nothing in the crate depends on a user interface toolkit. The egui viewer, [`ironlab-viewer`](https://crates.io/crates/ironlab-viewer), is one host of it; the browser host, `ironlab-web`, which shows a figure in a web page through the `<ironlab-figure>` element described in [embedding figures in a web page](https://ironlab.org/guides/embedding/), is another, and the crate builds for `wasm32-unknown-unknown` to prove that it has no native-only dependency.

## Where this crate sits

IronLAB is a Cargo workspace. `ironlab-canvas` depends on `ironlab-ir`, `ironlab-text`, `ironlab-scene` and `ironlab-pdf`, and `ironlab-viewer` and `ironlab-web` depend on it. Every PDF export in the project goes through this crate, because it is the crate that supplies the renderer the PDF backend asks for.

Most users should depend on the [`ironlab`](https://crates.io/crates/ironlab) facade crate, whose `export_pdf` operation uses this crate and whose `show` operation opens the viewer. Depend on `ironlab-canvas` directly only when you need this layer on its own, for instance to draw figures in an application of your own or to render figures offscreen.

## Documentation

The documentation site is at [ironlab.org](https://ironlab.org), where the [architecture](https://ironlab.org/reference/architecture/) page describes the one path from a figure to pixels that this crate completes. The source is at [github.com/thclark/ironlab](https://github.com/thclark/ironlab).

IronLAB is moving quickly and breaking changes are expected. Pin to an exact version.

## Licence

This crate is licensed under the GNU Affero General Public License, either version 3 or (at your option) any later version (`AGPL-3.0-or-later`).

If this conflicts with your use case, please raise an issue on the IronLAB repository and describe what you're trying to do. For commercial projects we can simply arrange a one-off license fee and for non-profit / academic efforts we may waive the fee.
