# ADR 0010: PDF-first export with a raster fallback for dense artists

**Status:** Accepted

**Supersedes:** [ADR 0004](0004-pdf-first-export-with-krilla.md)

**Related:** [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0005](0005-embedded-latex-math-with-latex-rust.md), [ADR 0007](0007-mvp-scope.md)

## Context

The primary use of an exported IronLAB figure is inclusion in a LaTeX document, and it must be trivial to include. The [background discussion](../background/concept-discussion.md) compared SVG and PDF as the first export format.

- `\includegraphics{figure.pdf}` works natively in pdfLaTeX, XeLaTeX and LuaLaTeX. Including SVG requires the `svg` package, which runs Inkscape through `--shell-escape`, an option that many journal build systems forbid.
- PDF has native shading types for smoothly shaded triangle meshes (types 4 and 5), which SVG lacks, and PDF supports print workflows that SVG cannot express.
- Text in the figure must be real text, embedded in the file with its fonts, so that it can be searched and selected and so that its size on the printed page is exactly the size set in the figure.
- krilla, from the Typst ecosystem, writes PDF with paths, transforms, clipping, opacity, dashing and positioned glyphs, and embeds subset fonts. It also embeds sampled images, deflated, with their transparency and their interpolation under the caller's control. Its high-level API excludes shadings.

Three-dimensional surfaces and filled contours also need a shading policy that the viewer and the PDF can both follow. Smooth (Gouraud) shading would require PDF shadings written below krilla's API with pdf-writer and per-vertex colour on screen.

[ADR 0004](0004-pdf-first-export-with-krilla.md) settled those questions and deferred one that experience has since forced. Flat shading writes one filled path per surface face, so the size of a figure grows in proportion to the number of cells its data has. A surface of tens of thousands of faces produces a file of megabytes that a reader takes seconds to open, for detail that no printer can resolve: once a face is narrower than a point, the boundaries between faces are finer than the paper can show. Sampling such an artist onto a pixel grid instead costs a size set by the area of the figure and the resolution asked for, rather than by the number of cells, and loses nothing that can be seen.

Rasterising raises a question that deferring it never had to answer: what draws the pixels. A rasteriser written for the exporter would be a second path from geometry to pixels, contrary to the single-path rule of [ADR 0003](0003-shared-scene-compiler-and-display-list.md), and it would drift from the viewer in precisely the way that rule exists to prevent — with the divergence visible only in print, where it is noticed last and matters most. The viewer already draws a figure into an image without a window, through the same tessellation and the same shaders as its interactive canvas, for the documentation gallery.

## Decision

PDF is the first and, for the MVP, only export format, written with krilla by the `ironlab-pdf` crate from the display list of [ADR 0003](0003-shared-scene-compiler-and-display-list.md).

### Decisions retained from ADR 0004

- **Page geometry.** Each figure is one page whose MediaBox, and CropBox, equal the figure size in points, with no margins. A figure is therefore included unscaled, and its fonts appear on the page at the sizes set in the figure. Users set the figure size to the size it should have in the document instead of scaling it in LaTeX.
- **Text.** Glyph runs are written with krilla's positioned-glyph API against the bundled fonts, embedded as subsets, and each run carries its source text, so text in the PDF can be selected, searched and copied. Mathematics is written the same way (see [ADR 0005](0005-embedded-latex-math-with-latex-rust.md)).
- **Shading.** Surface faces and filled contour bands are flat: each face or band is a single path filled with one colour, matching MATLAB's `shading faceted`. Three-dimensional faces, lines and markers are ordered back to front by the scene compiler (the painter's algorithm) on the CPU, and that order is used identically on screen and in the PDF.
- **Metadata.** The document title is the figure title's source, and the figure's provenance (IronLAB version, typesetter and fonts) is recorded in the document metadata.
- **Robustness.** Every display item is checked before it reaches krilla, and items that cannot be represented are skipped, so the exporter always writes a valid PDF.
- **SVG later.** SVG export is deferred. PDF and SVG share the same imaging model, so an SVG backend can consume the same display list when it is needed, for example for web figures or editing in Inkscape.

### Dense artists are drawn as images

The deferral of a raster fallback is replaced by the following decisions.

- **When.** An artist that the scene compiler marks as dense is drawn as an image instead of as vector paths once it reaches a threshold number of data cells, which defaults to ten thousand. That is where the two representations cost about the same at the default resolution on a figure of the default size: below it, vector output is both smaller and better, because it is resolution-independent, exactly coloured and editable; above it, the file grows in proportion to the cell count for no visible return. The threshold is an export parameter, and a user who knows their figure overrides the decision in either direction — forcing vectors for a figure that will be edited by hand, or an image for a coarse figure that will be scaled down.
- **At what resolution.** The export resolution is an export parameter in dots per inch, defaulting to 600, which is what journals ask for line art and for combined line and tone art. A figure is exported at its physical size, so this is the resolution the image has on the printed page.
- **What is rasterised.** Only the dense artist's own area. Axes, ticks, tick labels, axis labels, titles, legends, annotations and every other artist remain vector, and all text remains selectable. The image occupies the rectangle that the artist's geometry occupies in figure space, clipped exactly as that geometry is clipped, so it covers what the paths would have covered and nothing more.
- **How it is embedded.** As a deflated image XObject drawn without interpolation, so that no detail is lost to a lossy codec and the boundaries between faces stay as hard on the page as the vector drawing makes them. Transparency is preserved, so a translucent artist composites with the vector content beneath and above it exactly as it would have done as paths.
- **What draws it.** The exporter never rasterises anything itself. It is given a rasteriser, and the only implementation is the viewer's headless renderer, which draws the dense geometry alone through the tessellation and the shaders that draw the interactive canvas. There is therefore still exactly one path from geometry to pixels, and what is printed is what was inspected on screen.

### The display list gains an image primitive and a density marking

The display list of [ADR 0003](0003-shared-scene-compiler-and-display-list.md) gains two kinds of item.

- An **image**: a rectangle together with the true-colour samples drawn into it. This is the primitive that ADR 0003 reserved for image artists. The exporter builds one when it replaces dense geometry, and image artists will emit one directly, so both reach the page by the same route. Whether the display list should also be able to carry unmapped samples together with their colour mapping, so that changing an image artist's colour limits is a uniform update rather than a re-mapping of every sample, is deliberately left open; a second form can be added beside this one when image artists are designed.
- A **density marking**: a wrapper around the geometry of one artist that records how many data cells the artist drew. It carries neither a clip nor a transform of its own, so a backend that ignores it draws exactly the same picture, which is what the interactive canvas does. Because a three-dimensional axes sorts the faces of one artist among the geometry of others, the marking wraps each run of consecutive geometry separately, so that replacing runs with images preserves the back-to-front order.

The choice between vector and raster output belongs to the backend rather than to the compiler. The compiler records only what a backend needs in order to choose, because the canvas and the exporter choose differently: the canvas always draws vectors, and only the exporter has a resolution and a file size to trade against each other.

### The rasteriser is supplied to the exporter, not owned by it

The PDF backend declares what it needs of a rasteriser, and the viewer supplies one, because the dependency between the two crates runs from the viewer to the exporter and must not be reversed. Every export in the project therefore goes through the viewer, which wires them together; exporting without a rasteriser remains possible and draws every artist as vector paths, so a caller that has no renderer chooses that explicitly rather than receiving it silently.

Moving the viewer's renderer into a crate beneath the exporter was considered and rejected. It would remove the indirection, at the cost of putting a GPU stack into the dependencies of every consumer of the PDF backend.

## Consequences

- A figure is included in LaTeX with a plain `\includegraphics{figure.pdf}` and no special packages or build options, and its text matches the size chosen in IronLAB.
- The screen and the PDF agree on shading and on three-dimensional paint order, because both use the compiler's flat faces and sorted order, and they agree on a rasterised artist because the pixels are drawn by the viewer's own renderer.
- A dense surface exports as a file whose size is set by the area of the figure and the export resolution rather than by the number of cells in its data, and it opens immediately. A figure below the threshold is unchanged, so existing figures gain nothing and lose nothing.
- A rasterised artist is no longer resolution-independent. Enlarging such a figure beyond the export resolution shows pixels, and the artist can no longer be edited as paths in a vector editor. Both are the point of the trade, and both are reversed by turning rasterisation off.
- Exporting a figure that must be rasterised needs a graphics adapter. A figure with nothing dense in it is exported without one, as before, so the requirement falls only where it is unavoidable, and it is removed by turning rasterisation off.
- Flat shading shows facets on coarse surfaces, which is the intended appearance of `shading faceted` but not a substitute for smooth shading. Interpolated shading, which needs PDF mesh shadings written with pdf-writer and matching colour on screen, is deferred; a dense surface that is rasterised no longer needs it as urgently, because facets narrower than a point are invisible either way.
- The painter's algorithm cannot draw intersecting or cyclically overlapping faces correctly. This is accepted for the MVP. If the viewer later gains a GPU depth buffer, the difference between the viewer and the painter-sorted PDF must be documented.
- Colour is written as RGB only; CMYK and PDF/X output are not supported. Transparency in a rasterised artist is carried as a soft mask, which strict print profiles restrict, so an artist that is fully opaque is written without one.

Follow-up work is tracked in GitHub issues:

- [#5: Add interpolated (Gouraud) shading with PDF mesh shadings](https://github.com/thclark/ironlab/issues/5)
- [#7: Add image, imagesc and pcolor artists](https://github.com/thclark/ironlab/issues/7)
- [#12: Add an SVG export backend](https://github.com/thclark/ironlab/issues/12)
