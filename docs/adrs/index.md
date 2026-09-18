# Architecture decisions

This section records the significant decisions about IronLAB's design as a numbered chain of architecture decision records (ADRs). Each record states the context in which the decision was made, the decision itself, and its consequences, and links to the records it relates to. The discussion from which most of these decisions emerged is in the [background](../background/index.md) section, and the resulting structure is summarised in the [architecture reference](../reference/architecture.md).

Records that have been released on the `main` branch are never modified. A decision that changes is recorded in a new ADR that names the record it supersedes, and this index shows the earlier record as superseded.

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](0001-retained-figure-ir-and-protobuf-wire-format.md) | Figures are a retained IR with stable node identifiers whose Rust types are the source of truth; `.fig` files use Protocol Buffers, with JSON as a secondary format and both schemas generated. | Accepted |
| [0002](0002-egui-viewer-on-rerun-family-crates.md) | The viewer is built with egui and eframe, using only Rerun-family user-interface crates plus `rfd`. | Accepted |
| [0003](0003-shared-scene-compiler-and-display-list.md) | One scene compiler produces a display list that every backend draws; the canvas uses egui meshes now and custom wgpu pipelines with GPU picking later. | Accepted |
| [0004](0004-pdf-first-export-with-krilla.md) | PDF is the first export format, written with krilla, with the page equal to the artwork, real subset text, and flat, painter-sorted shading. | Superseded by [0010](0010-pdf-export-with-a-raster-fallback.md) |
| [0005](0005-embedded-latex-math-with-latex-rust.md) | LaTeX mathematics is typeset in process with latex-rust from source stored in the IR, with a fallback and warning for unsupported input. | Accepted |
| [0006](0006-interaction-mutates-the-ir.md) | Viewer interaction edits the IR, views reset from a snapshot, and linked axes form disjoint groups whose automatic limits are shared. | Superseded by [0008](0008-typed-edits-and-a-view-overlay.md) |
| [0007](0007-mvp-scope.md) | The MVP excludes sockets, bindings, GPU picking and smooth shading, and offers an in-process viewer and a file viewer. | Accepted |
| [0008](0008-typed-edits-and-a-view-overlay.md) | Figures change through typed, literal edits in atomic transactions, and the viewer shows the owner's source figure with the user's view changes as an overlay whose conflicts are detected by property path. | Accepted |
| [0009](0009-view-dependent-decimation-with-source-index-maps.md) | The scene compiler thins a series larger than its plot can resolve, keyed on the view, by largest-triangle-three-buckets for lines and by binning for markers, and every drawn point carries the index it has in the source data. | Accepted |
| [0010](0010-pdf-export-with-a-raster-fallback.md) | PDF export keeps the page equal to the artwork, real subset text and flat, painter-sorted shading, and draws an artist too dense for vector paths as an image rendered by the viewer's own renderer. | Accepted |
| [0011](0011-colour-scales-are-referenced-table-entries.md) | A colour scale is an entry in a table of the figure that axes and artists refer to by identifier; it carries its own levels, a colormap is a built-in name or a list of colours, and a colorbar is an optional property of an axes that shows the scale of that axes. | Accepted |
