# ADR 0014: One draw list per figure, drawn by the viewer's own wgpu pipelines

**Status:** Accepted

**Related:** [ADR 0002](0002-egui-viewer-on-rerun-family-crates.md), [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0009](0009-view-dependent-decimation-with-source-index-maps.md), [ADR 0013](0013-depth-buffered-three-dimensional-artists.md)

## Context

[ADR 0003](0003-shared-scene-compiler-and-display-list.md) drew the display list as egui meshes for the MVP and promised custom wgpu pipelines later, with data uploaded once and pan and zoom applied as a transform ([issue #13](https://github.com/thclark/ironlab/issues/13)). Two later decisions changed what that promise means. [ADR 0009](0009-view-dependent-decimation-with-source-index-maps.md) made thinning view-dependent and put it in the scene compiler, so a pan or a zoom of an axes recompiles the drawn geometry by design, and the number of drawn points is bounded by the size of the plot rather than by the data. [ADR 0013](0013-depth-buffered-three-dimensional-artists.md) gave the viewer pipelines of its own for the artists of three-dimensional axes, beside the egui meshes of everything else, and with them a second way of clipping (a scissor rectangle rather than the geometric clipping of triangles), a second cache of image textures and a second path through the offscreen renderer. Every frame that moved the figure re-tessellated it and re-uploaded every mesh, and every window resize did the same.

## Decision

### Every item goes through the viewer's pipelines

The canvas produces one draw list per figure: the triangles of every path, glyph run and image tile, in figure points, in paint order, each draw naming its texture, its clip rectangle in figure points, its depth group and its node. The pipelines of ADR 0013 draw the whole list, on screen inside egui's render pass through one paint callback in the figure's place among the interface's shapes, and offscreen inside a pass of the renderer's own. egui draws only the interface. The egui-mesh conversion, the texture provider and its caches, the geometric clipping of triangles and egui's renderer in the offscreen path are removed.

### The list is in figure points and the mapping is a uniform

A vertex holds its figure-space position; the mapping from figure points to the screen, and the target's size, live in a uniform of thirty-two bytes per list. A frame in which nothing moved uploads nothing. A resize, a change of tab layout or a pan of the window rewrites the uniform alone. The list is rebuilt when the scene changes, which a gesture on an axes does by ADR 0009, and when the figure's scale on screen leaves a band of a quarter either way of the scale the list was built for, because curves are flattened and hairlines are widened for a resolution: within the band a curve stays within a tenth of a pixel of true and a hairline within a quarter of a pixel of one pixel wide, and outside it the list is built again for the new scale. Buffers and textures persist on the device for as long as their list is drawn, keyed by the identity of the list and of the image data, and are freed when a frame no longer draws them.

### Clipping is a scissor

Every draw is cut by the scissor rectangle of its clip, rounded to whole pixels as egui rounds its own. A geometric clip anti-aliased the edge of a plot; a scissor makes it a whole pixel, on screen and offscreen alike. An image tile whose bounds lie wholly outside its clip is not drawn at all.

### Glyphs are cached outline tessellations

A glyph is tessellated once per font, glyph and size bucket from its outline, and placed by translation and scale into every run that uses it. Under multisample anti-aliasing an outline tessellation is independent of resolution in a way a glyph atlas is not, and a figure has few glyphs, so no atlas is kept.

### What the change that follows adds

Strokes are still expanded into triangles by lyon in this change, and markers are still one path each. The change that follows carries markers as instances of one outline in the display list and expands strokes, with their joins, caps and dashes, on the GPU from the compiler's polylines, which closes the last of [issue #13](https://github.com/thclark/ironlab/issues/13).

## Consequences

- There is one route from the display list to pixels in the viewer: the same list, pipelines, blending and clipping produce the window, the gallery and the rasters the PDF exporter embeds or compares. The renders of the gallery before and after the change are identical for every figure without a clip edge, and differ by less than one level in 255 per block along clip edges.
- The canvas, the offscreen renderer and the application lost more code than they gained: the mesh path, its texture caches and the geometric clipper are gone.
- An idle frame uploads nothing, and a resize uploads thirty-two bytes per figure; a gesture on an axes recompiles the axes and rebuilds the list, whose size is bounded by the plot, as ADR 0009 intended.
- Clip edges are whole pixels rather than anti-aliased, which moves a plot's edge by at most a pixel and identically in every backend.
- The image tiles of the canvas are cut at the smaller of 8192 pixels and the device's largest texture side, in both the window and the offscreen renderer, so the two tile alike.
- [Issue #1](https://github.com/thclark/ironlab/issues/1) replays the same list into an integer target using the node every draw records.
