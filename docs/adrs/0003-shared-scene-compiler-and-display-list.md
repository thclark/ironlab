# ADR 0003: Shared scene compiler and display list

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0002](0002-egui-viewer-on-rerun-family-crates.md), [ADR 0004](0004-pdf-first-export-with-krilla.md), [ADR 0006](0006-interaction-mutates-the-ir.md), [ADR 0007](0007-mvp-scope.md)

## Context

A figure must look the same in the interactive viewer and in the exported PDF. The largest risk to that promise is duplication: if the viewer and the exporter each compute tick positions, label placement, contour lines or the order of three-dimensional faces, their results drift apart, and a user publishes something different from what they inspected.

The [background discussion](../background/concept-discussion.md) identified two honest positions: treat the interactive view as a preview and share only a layout layer, or make every renderer consume the same vector primitives. It also noted that latex-rust offers its own egui renderer, which would be a second path to the same pixels and would drift from the PDF path in exactly this way.

A decision was also needed on how the viewer draws in the MVP. Custom wgpu pipelines (line strips, instanced markers, triangle meshes, and a GPU picking pass) give the best performance for large data sets and correct occlusion for picking, but take substantially longer to build than drawing through egui's own mesh renderer.

## Decision

All geometry is computed in one place, the scene compiler `ironlab_scene::compile(&Figure, &TextEngine) -> Scene`.

- The compiler performs layout, tick generation, automatic limits, colormapping, contour extraction, quiver scaling, three-dimensional projection and depth sorting, and resolves text through the text engine.
- Its output is a backend-neutral display list, in points in figure space, containing only paths (with fill and stroke), positioned glyph runs, and groups (with clip and transform). Every item names the IR node that produced it, for future selection and picking. Image artists, when they are added, will add an image primitive to the display list.
- It also produces a hit map (plot rectangles, axis mappings and legend-entry rectangles) for interaction, and a list of warnings. Compilation never fails; problems become warnings.
- Every backend is a simple consumer of the display list that makes no layout decisions. There is no second path from the IR to pixels.

For the MVP, the viewer's canvas draws the display list as egui meshes: paths are tessellated with lyon (dashes are split first, because lyon does not dash), glyph outlines are tessellated once per glyph, and egui's wgpu renderer draws the meshes with four-times multisample anti-aliasing. The same meshes are rendered offscreen to produce the documentation gallery's images, so the gallery shows exactly what the viewer shows.

In a later change, custom wgpu pipelines will replace the canvas backend only: data uploaded to the GPU once, pan and zoom applied as a transform, and a GPU picking pass that renders node and element identifiers to an integer target read back around the pointer. The scene compiler and display list remain the interface to that backend.

## Consequences

- The viewer and the PDF cannot disagree about layout, ticks, text placement, contour geometry or three-dimensional paint order, because they receive identical geometry. Tests can compare an offscreen render with a rasterised PDF.
- Adding a plot type means teaching the compiler to emit primitives for it; neither backend changes.
- The egui-mesh canvas re-tessellates the figure after every change of limits or view. This is fast enough for the gallery's figures but will not hold interactive frame rates for millions of points; that limitation is accepted for the MVP and addressed by the custom pipelines.
- Egui meshes have no depth buffer, so three-dimensional scenes rely on the compiler's painter's-algorithm sort, which cannot resolve intersecting surfaces exactly. The same limitation applies to the PDF (see [ADR 0004](0004-pdf-first-export-with-krilla.md)).
- Picking in the MVP is analytic and limited to axes and legend entries through the hit map. Data tips, picking of individual data points, and occlusion-correct picking of surfaces and images wait for the GPU picking pass.

Follow-up work is tracked in GitHub issues:

<!-- issues -->
