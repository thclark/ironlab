# ADR 0004: PDF-first export with krilla

**Status:** Accepted

**Related:** [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0005](0005-embedded-latex-math-with-latex-rust.md), [ADR 0007](0007-mvp-scope.md)

## Context

The primary use of an exported IronLAB figure is inclusion in a LaTeX document, and it must be trivial to include. The [background discussion](../background/concept-discussion.md) compared SVG and PDF as the first export format.

- `\includegraphics{figure.pdf}` works natively in pdfLaTeX, XeLaTeX and LuaLaTeX. Including SVG requires the `svg` package, which runs Inkscape through `--shell-escape`, an option that many journal build systems forbid.
- PDF has native shading types for smoothly shaded triangle meshes (types 4 and 5), which SVG lacks, and PDF supports print workflows that SVG cannot express.
- Text in the figure must be real text, embedded in the file with its fonts, so that it can be searched and selected and so that its size on the printed page is exactly the size set in the figure.
- krilla, from the Typst ecosystem, writes PDF with paths, transforms, clipping, opacity, dashing and positioned glyphs, and embeds subset fonts. Its high-level API excludes shadings.

Three-dimensional surfaces and filled contours also need a shading policy that the viewer and the PDF can both follow. Smooth (Gouraud) shading would require PDF shadings written below krilla's API with pdf-writer, per-vertex colour on screen, and a raster fallback for dense meshes.

## Decision

PDF is the first and, for the MVP, only export format, written with krilla by the `ironlab-pdf` crate from the display list of [ADR 0003](0003-shared-scene-compiler-and-display-list.md).

- **Page geometry.** Each figure is one page whose MediaBox, and CropBox, equal the figure size in points, with no margins. A figure is therefore included unscaled, and its fonts appear on the page at the sizes set in the figure. Users set the figure size to the size it should have in the document instead of scaling it in LaTeX.
- **Text.** Glyph runs are written with krilla's positioned-glyph API against the bundled fonts, embedded as subsets, and each run carries its source text, so text in the PDF can be selected, searched and copied. Mathematics is written the same way (see [ADR 0005](0005-embedded-latex-math-with-latex-rust.md)).
- **Shading.** Surface faces and filled contour bands are flat: each face or band is a single path filled with one colour, matching MATLAB's `shading faceted`. Three-dimensional faces, lines and markers are ordered back to front by the scene compiler (the painter's algorithm) on the CPU, and that order is used identically on screen and in the PDF.
- **Metadata.** The document title is the figure title's source, and the figure's provenance (IronLAB version, typesetter and fonts) is recorded in the document metadata.
- **Robustness.** Every display item is checked before it reaches krilla, and items that cannot be represented are skipped, so the exporter always writes a valid PDF.
- **SVG later.** SVG export is deferred. PDF and SVG share the same imaging model, so an SVG backend can consume the same display list when it is needed, for example for web figures or editing in Inkscape.

## Consequences

- A figure is included in LaTeX with a plain `\includegraphics{figure.pdf}` and no special packages or build options, and its text matches the size chosen in IronLAB.
- The screen and the PDF agree on shading and on three-dimensional paint order, because both use the compiler's flat faces and sorted order.
- Flat shading shows facets on coarse surfaces, which is the intended appearance of `shading faceted` but not a substitute for smooth shading. Interpolated shading, which needs PDF mesh shadings written with pdf-writer and matching colour on screen, is deferred.
- The painter's algorithm cannot draw intersecting or cyclically overlapping faces correctly. This is accepted for the MVP. If the viewer later gains a GPU depth buffer, the difference between the viewer and the painter-sorted PDF must be documented.
- Every face of a dense surface is a separate vector path, so very dense surfaces produce large files that render slowly. A raster fallback for dense surfaces and for images (rendered at the export resolution and embedded without interpolation, with axes and text kept as vectors) is deferred.
- Colour is written as RGB only; CMYK and PDF/X output are not supported.

Follow-up work is tracked in GitHub issues:

<!-- issues -->
