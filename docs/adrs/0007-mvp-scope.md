# ADR 0007: Scope of the minimum viable product

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0002](0002-egui-viewer-on-rerun-family-crates.md), [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0004](0004-pdf-first-export-with-krilla.md), [ADR 0005](0005-embedded-latex-math-with-latex-rust.md), [ADR 0006](0006-interaction-mutates-the-ir.md)

## Context

The [background discussion](../background/concept-discussion.md) described a full system: a standalone viewer process fed over a socket by client libraries in Rust, Python, C++ and MATLAB; a browser viewer; GPU picking and data tips; smooth shading; images; and decimation of large data sets. It also concluded that the API and protocol should be shaped by real use before they become compatibility surfaces, and that the Rust API, the figure model, the viewer and the PDF backend come first.

The first release therefore needs a boundary that delivers a usable tool while keeping every deferred feature possible without restructuring.

## Decision

The MVP contains:

- the figure IR and its Protocol Buffers and JSON encodings ([ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md));
- a MATLAB-flavoured Rust API with the plot types `plot`, `plot3`, `loglog`, `semilogx`, `semilogy`, `scatter`, `scatter3`, `contour`, `contourf`, `contour3`, `quiver`, `quiver3`, `surf` and `mesh`, together with titles, LaTeX labels, legends, tiled layouts, and axes linked arbitrarily or with the all-x and all-y shortcuts;
- saving and loading `.fig` files, and JSON files for debugging and other tools;
- PDF export with flat shading ([ADR 0004](0004-pdf-first-export-with-krilla.md)) and embedded LaTeX mathematics in the STIX Two font set ([ADR 0005](0005-embedded-latex-math-with-latex-rust.md));
- the viewer ([ADR 0002](0002-egui-viewer-on-rerun-family-crates.md)) with pan, wheel zoom, box zoom, rotation of three-dimensional axes, per-axes and whole-figure view reset, legend click to toggle visibility, export to PDF and a problems indicator ([ADR 0006](0006-interaction-mutates-the-ir.md));
- two ways to reach the viewer: in process, where `Figure::show` opens a window and blocks until it is closed, and from files, where the `ironlab-viewer` binary opens `.fig` and JSON files as tabs;
- a gallery of example figures that is also the documentation gallery and a test suite.

The MVP excludes:

- sockets, a protocol for sending figures and edits between processes, a standalone viewer process fed by clients, and event queues for callbacks;
- client libraries and bindings for other languages, including a C ABI;
- a browser or WebAssembly viewer;
- GPU picking, data tips and custom wgpu canvas pipelines ([ADR 0003](0003-shared-scene-compiler-and-display-list.md));
- interpolated (Gouraud) shading, raster fallback for dense surfaces, image artists and colorbars;
- decimation of large data sets;
- SVG export, CMYK output and font sets other than STIX Two;
- a property editor.

## Consequences

- The first release is usable for Rust programs that build figures, inspect them and export them for publication.
- The in-process viewer blocks the calling thread and, on macOS, must run on the main thread, so it does not suit hosts that own the main thread or long computations that should keep running while a figure is shown. Saving a `.fig` file and opening it in the viewer binary is the alternative until a standalone viewer process exists.
- Every excluded feature has a defined place in the existing design: sockets and bindings carry edits of the IR, the GPU canvas replaces only the canvas backend, new plot types are new artist variants, and new export formats consume the display list. None requires restructuring the MVP.
- Performance with very large data sets is limited by the egui-mesh canvas and the absence of decimation.

Follow-up work is tracked in GitHub issues:

- [#13: Replace the egui-mesh canvas with custom wgpu pipelines](https://github.com/thclark/ironlab/issues/13)
- [#1: Add a GPU picking pass](https://github.com/thclark/ironlab/issues/1)
- [#2: Add data cursors and pinned datatips](https://github.com/thclark/ironlab/issues/2)
- [#3: Add view-dependent decimation with source index maps](https://github.com/thclark/ironlab/issues/3)
- [#4: Use depth buffering for 3D artists in the viewer](https://github.com/thclark/ironlab/issues/4)
- [#5: Add interpolated (Gouraud) shading with PDF mesh shadings](https://github.com/thclark/ironlab/issues/5)
- [#6: Rasterise dense surfaces and images in PDF export](https://github.com/thclark/ironlab/issues/6)
- [#7: Add image, imagesc and pcolor artists](https://github.com/thclark/ironlab/issues/7)
- [#8: Add a colorbar node](https://github.com/thclark/ironlab/issues/8)
- [#9: Check the semantic version of the Cargo workspace in CI](https://github.com/thclark/ironlab/issues/9)
- [#10: Report latex-rust typesetting defects upstream](https://github.com/thclark/ironlab/issues/10)
- [#11: Improve 3D axes clipping, label orientation and fit](https://github.com/thclark/ironlab/issues/11)
- [#12: Add an SVG export backend](https://github.com/thclark/ironlab/issues/12)
