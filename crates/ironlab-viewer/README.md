# ironlab-viewer

The interactive egui viewer for [IronLAB](https://ironlab.org) figures, an interactive plotting tool for scientific computing in Rust.

The viewer is an eframe application that shows one or more figures, one at a time with a browser to choose between them, in which axes can be panned, zoomed and rotated, plots can be hidden or shown from the legend, and the properties of any object can be changed in a property editor. It is the egui host of the figure canvas, [`ironlab-canvas`](https://crates.io/crates/ironlab-canvas), which draws each figure through its own wgpu pipelines inside the window's render pass and turns every gesture into a typed edit of the figure model, recorded in a view overlay rather than applied to the figure it was given. Undo, redo, saving and PDF export all act on that overlay, so what is exported is exactly what is on screen. The offscreen renderer that draws a figure into an image without a window, and the PDF export that rasterises dense content through it, live in `ironlab-canvas`; this crate adds the window, the toolbar, the figure browser and the property editor around them.

## Where this crate sits

IronLAB is a Cargo workspace. `ironlab-viewer` sits at the top of the library stack and depends on `ironlab-canvas`, `ironlab-ir`, `ironlab-text` and `ironlab-pdf`. It is the only crate of the workspace that links egui, eframe and egui-wgpu.

Most users should depend on the [`ironlab`](https://crates.io/crates/ironlab) facade crate, whose `show` operation opens this viewer. Depend on `ironlab-viewer` directly only when you need this layer on its own, for instance to embed the viewer's panels in an egui application of your own; to draw figures without egui or to render them offscreen, depend on `ironlab-canvas` instead.

## The viewer binary

The crate ships a binary of the same name, which opens saved figure files:

```shell
cargo install ironlab-viewer
ironlab-viewer figure.fig
```

Several files may be given, and the browser down the left-hand side chooses between them. The format of each file is chosen by its extension: `.fig` files are read as Protocol Buffers, the default format, and `.json` files (such as `figure.fig.json`) are read as JSON.

## Documentation

The documentation site is at [ironlab.org](https://ironlab.org), where [using the viewer](https://ironlab.org/guides/viewer/) describes its tools, gestures and keyboard shortcuts. The source is at [github.com/thclark/ironlab](https://github.com/thclark/ironlab).

IronLAB is moving quickly and breaking changes are expected. Pin to an exact version.

## Licence

This crate is licensed under the GNU Affero General Public License, either version 3 or (at your option) any later version (`AGPL-3.0-or-later`).

If this conflicts with your use case, please raise an issue on the IronLAB repository and describe what you're trying to do. For commercial projects we can simply arrange a one-off license fee and for non-profit / academic efforts we may waive the fee.

## The widget gallery

Every control of the interface is drawn from the `widgets` module, and the module can be seen on its own, without
a figure, by running the example:

```bash
cargo run -p ironlab-viewer --example widgets
```

It opens a window showing every component in every state, so that the look of the interface can be worked on in
the abstract and the viewer picks the result up unchanged.
