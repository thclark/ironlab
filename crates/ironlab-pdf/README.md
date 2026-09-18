# ironlab-pdf

PDF export of [IronLAB](https://ironlab.org) figures, for publication-quality output from an interactive plotting tool for scientific computing in Rust.

The exporter draws a compiled display list onto a single PDF page with krilla. The page's MediaBox and CropBox equal the figure size in points with no margins, so a document preparation system such as LaTeX includes the figure unscaled at its physical size. Glyph runs are written as real text: fonts are embedded and subset, and each run carries its source text, so the text in the exported PDF can be searched, selected and copied.

Content that the scene compiler marked as dense, such as a surface with tens of thousands of faces, is drawn as a deflated image instead of one vector path per face once it reaches a configurable threshold. Everything else stays vector and all text stays selectable. This crate does not rasterise anything itself: it defines what it needs of a rasteriser and the caller supplies one, which in IronLAB is the viewer's own headless renderer, so the exported pixels are the ones that were on screen. Display lists can also be built by hand, so every item is checked before it reaches krilla and anything that cannot be represented is skipped rather than producing an invalid file.

## Where this crate sits

IronLAB is a Cargo workspace. `ironlab-pdf` is one of its two backends and depends on `ironlab-ir`, `ironlab-text` and `ironlab-scene`. It deliberately cannot reach the viewer, which is what keeps the rasteriser an interface rather than a second renderer that could drift from the screen.

Most users should depend on the [`ironlab`](https://crates.io/crates/ironlab) facade crate, whose `export_pdf` operation drives this crate with a rasteriser already supplied. Depend on `ironlab-pdf` directly only when you need this layer on its own, for instance to write a display list you produced yourself to a PDF page.

## Documentation

The documentation site is at [ironlab.org](https://ironlab.org), and the [architecture reference](https://ironlab.org/reference/architecture/) describes the backends and the raster fallback. The source is at [github.com/thclark/ironlab](https://github.com/thclark/ironlab).

IronLAB is moving quickly and breaking changes are expected. Pin to an exact version.

## Licence

This crate is licensed under the GNU Affero General Public License, either version 3 or (at your option) any later version (`AGPL-3.0-or-later`).
