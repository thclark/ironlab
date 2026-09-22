# ADR 0013: Depth-buffered three-dimensional artists, exported as vectors where the painter's order is proven

**Status:** Accepted

**Related:** [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0009](0009-view-dependent-decimation-with-source-index-maps.md), [ADR 0010](0010-pdf-export-with-a-raster-fallback.md), [ADR 0011](0011-image-artists.md)

## Context

[ADR 0003](0003-shared-scene-compiler-and-display-list.md) chose egui meshes for the canvas and accepted that, without a depth buffer, three-dimensional scenes rely on the compiler's painter's-algorithm sort, which cannot resolve intersecting surfaces. [ADR 0010](0010-pdf-export-with-a-raster-fallback.md) carried the limitation into the PDF and required that a viewer gaining a depth buffer document its difference from the painter-sorted page, and [ADR 0011](0011-image-artists.md) admitted that an image inside the box of a three-dimensional axes is painted wholly before or after a surface crossing its plane. The gallery entry that draws three colour-mapped planes through a correlation peak made the limitation visible, and [issue #4](https://github.com/thclark/ironlab/issues/4) asked for a depth buffer for every three-dimensional artist, under the rule that the exporter runs the same process as the viewer, so that what is printed is what was inspected.

A depth buffer belongs to a graphics pipeline, and egui's meshes have none. It also raises two questions that the single path of ADR 0003 makes hard to answer: a PDF has no depth buffer, and a painter's order and a depth test disagree wherever a face's edge, or a line or a marker lying on a surface, coincides with the face in depth.

## Decision

### Depths are literal in the display list

Every path and image that the compiler emits for a three-dimensional axes carries the depth at which it is painted, in its own local space: a plane, `depth(x, y) = a·x + b·y + c`, for a surface face, a marker, a filled contour band and an image, or one depth per endpoint for a polyline. Larger depths are nearer the viewer. The artists of one axes lie in one depth group, in the painter's order the compiler chose, between the back of the axes box and its front edges. A backend without a depth buffer draws the group in order and gets the painter's picture; one with a depth buffer clears it at the group and tests every item against it. No backend biases or reorders anything: the display list says exactly where everything is.

### The compiler separates what coincides

A depth test cannot separate what coincides, so the compiler moves things apart by amounts too small to see. Every face, and every image inside the box, is pushed a thousandth of the box behind its geometry and sorted a thousandth earlier, so that the lines and markers lying on a surface are painted, and depth-tested, in front of it, and the two orders agree about them. The edge of a face is not a stroke, which would spill half its width over the neighbouring faces and make the two orders disagree along every fold of a surface, but a ring filled with the edge colour between the face's outline and that outline moved half the edge width inwards; the rings of two neighbours meet to make an edge of the full width, and a ring lies a hundredth of the bias in front of its own fill, because a depth buffer interpolates one plane differently over two triangulations. A face takes the least-squares plane through its projected corners, so that a twisted face is flattened by at most its own twist and its fill and its edge never fight. A face whose projected outline has no inset, seen edge-on or too small for its edge, is stroked instead.

### The viewer draws depth groups with pipelines of its own

The canvas keeps drawing egui meshes for everything else and draws each depth group through IronLAB's own wgpu pipelines: the same lyon geometry, in screen units, with a depth at every vertex, depth-tested inside egui's render pass, which the window now creates with a depth attachment. The offscreen renderer draws the same lists through the same pipelines inside its own pass, so the gallery and the PDF's rasteriser see exactly what the window shows, and nothing needs a window. A list drawn outside a depth group goes through the same pipelines with the depth test off, and that is what the exporter's proof rests on.

### The exporter proves the painter's order or embeds the picture

A PDF has no depth buffer. Under the default policy the exporter asks the rasteriser for a three-dimensional axes twice at the export resolution, once with the depth test and once without, and writes the artists as vector paths in painter's order when the two renders show the same picture; otherwise it embeds the depth-tested render as an image, exactly as it embeds dense content. Two renders show the same picture when no patch of six points differs by more than four levels of 255 on average, which admits the slivers that anti-aliasing leaves along the shared edges of faces and refuses any misdrawn face, marker or stretch of line. A user who knows their figure forces either outcome.

### Every raster and every unverified axes is reported

Whenever the exporter replaces vector geometry with pixels, for depth, for the size of a dense artist or because the options ask, and whenever it draws a three-dimensional axes without being able to verify it, the export report carries a warning that names the node and the reason. The dense fallback of ADR 0010, which was silent, reports itself too.

### Machines without a graphics device

wgpu has no processor-only backend, and a rasteriser of IronLAB's own would be a second path to pixels. A machine without a graphics device uses a software adapter, such as lavapipe, which wgpu finds like any other. Without any adapter the default policy still writes the painter's order, with a warning that names the missing adapter and the remedy; only an image that the options force, or that a dense artist needs, fails.

## Consequences

- The viewer shows intersecting surfaces, crossing images, and lines and markers passing through surfaces correctly, and the gallery shows them the same way.
- A three-dimensional axes whose painter's order is exact, which a single height field with edges is, exports as vectors as before; one whose artists overlap in an order no painting can draw, such as two intersecting surfaces, arrows standing on a surface or a line lying on it, exports as an image, and the report says so. So does a sharply folded surface whose edges contrast with its faces, such as a wireframe mesh, because along a fold the edge of the far face lies within the bias of the near face's fill and shows through it under the depth test but not in the painter's order.
- Exporting a three-dimensional axes under the default policy uses a graphics adapter to verify it, and without one still succeeds with a warning. Every other export needs an adapter only for what must be an image, as before.
- Translucent faces are drawn in painter's order with depth writes, so a translucent face drawn before a nearer opaque one blends correctly and one drawn after hides what is behind it; the remedy is planned under [issue #34](https://github.com/thclark/ironlab/issues/34).
- Coplanar geometry is resolved by order, and a twisted face is drawn as a plane; both are invisible in practice.
- A display list built by hand must give every path and image of a depth group a depth, and the raster options of an export gain a depth policy.
- The egui-mesh path of the canvas is transitional: the change that follows draws every item through the viewer's own pipelines and removes it.
- The consequence of ADR 0010 that a viewer with a depth buffer must document its difference from the painter-sorted PDF is discharged: the exporter proves the two the same or prints the viewer's picture.

Follow-up work is tracked in GitHub issues:

- [#13: Replace the egui-mesh canvas with custom wgpu pipelines](https://github.com/thclark/ironlab/issues/13)
- [#1: Add a GPU picking pass](https://github.com/thclark/ironlab/issues/1)
- [#36: Read pixel datatips of images in three-dimensional axes](https://github.com/thclark/ironlab/issues/36)
- [#34: Add an opacity property to image artists](https://github.com/thclark/ironlab/issues/34)
