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

### Strokes are expanded on the device

A stroked path is not tessellated. The canvas flattens each subpath into a polyline and uploads one segment per edge, in the item space of the path, with its neighbours, the arc length at its ends and the depth at its ends; the stroke's width, cap, join, dash pattern, colour and item-to-figure transform go into a small block of parameters per draw. The stroke vertex shader expands every segment into its body, the join at its start (a miter within the PDF limit of four, else a bevel; or an arc) and the caps at its free ends (butt, round or square), and the fragment shader dashes by arc length, giving every dash the caps of the stroke, cutting the dashes at the ends of the subpath and anti-aliasing their ends over a pixel, all as the PDF operators define them. A dashed stroke with round or square caps draws a fan around each joint whose coverage the dashes decide, because the caps of the dashes ending or starting beside the joint reach where a join would. Widths are transformed with the geometry, so a stroke beneath a non-uniform transform has an anisotropic pen, as in PDF. A hairline (width zero) is one screen point wide.

A stroke outside every depth group is depth-tested against itself: the k-th such stroke since the list or the last depth group began writes a depth of `1 − (k + 1) · 2⁻²⁰`, so a later stroke passes over an earlier one while the parts of one stroke that overlap, on the inner side of a turn or under a round join, take its colour exactly once. A translucent polyline therefore has no dark specks at its joints, as it has none in PDF. Inside a depth group a stroke takes the depth of its segments, following the plane of its face across its own width, and its overlapping parts may blend twice; that is accepted.

### Markers are instances of one outline

The markers of an artist are one item of the display list: the outline of the marker in unit space, the width of its edge, and one instance per marker carrying its position, size, depth, face and edge colours and the index of its data point. The compiler emits one such item per artist in a two-dimensional axes and, in a three-dimensional one, one per run of markers that the depth sort leaves together, so the painter's order is exactly what it was when every marker was a path. The PDF exporter writes one path per instance, so the page is unchanged. The viewer tessellates the outline once, its fill with lyon and its edge with lyon's stroker at a nominal width with the offset of every edge vertex recorded, and draws the instances through one instanced draw, fill before edge, each instance at its own centre, size, depth and colours: a scatter of ten thousand points uploads ten thousand instances of twenty-four bytes and one outline. This closes the last of [issue #13](https://github.com/thclark/ironlab/issues/13).

## Consequences

- There is one route from the display list to pixels in the viewer: the same list, pipelines, blending and clipping produce the window, the gallery and the rasters the PDF exporter embeds or compares. The renders of the gallery before and after the change are identical for every figure without a clip edge, and differ by less than one level in 255 per block along clip edges.
- The canvas, the offscreen renderer and the application lost more code than they gained: the mesh path, its texture caches and the geometric clipper are gone.
- A display list built by hand must carry markers as a markers item; a consumer that handled paths, glyphs and images handles one more kind of leaf, and the picking pass of [issue #1](https://github.com/thclark/ironlab/issues/1) reads a marker's data index from its instance.
- An idle frame uploads nothing, and a resize uploads thirty-two bytes per figure; a gesture on an axes recompiles the axes and rebuilds the list, whose size is bounded by the plot, as ADR 0009 intended. A polyline of a thousand points uploads a thousand segments of sixty-four bytes rather than the triangles of its stroke, and no dash is split on the processor.
- The strokes the viewer draws and the strokes a PDF reader draws are two implementations of one contract; the comparison tests render every join, cap and dash pattern through both and require the same picture within the tolerances the dense-surface exports use.
- Clip edges are whole pixels rather than anti-aliased, which moves a plot's edge by at most a pixel and identically in every backend.
- The image tiles of the canvas are cut at the smaller of 8192 pixels and the device's largest texture side, in both the window and the offscreen renderer, so the two tile alike.
- [Issue #1](https://github.com/thclark/ironlab/issues/1) replays the same list into an integer target using the node every draw records.
