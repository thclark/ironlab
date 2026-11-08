# ADR 0033: Images are warped through a mesh

**Status:** Accepted

**Related:** [ADR 0011](0011-image-artists.md), [ADR 0021](0021-images-refused-on-logarithmic-axes-and-alpha-and-opacity.md), which this record supersedes in part, [ADR 0010](0010-pdf-export-with-a-raster-fallback.md), [ADR 0032](0032-data-is-joined-by-chords-and-shapes-follow-the-axes.md)

## Context

[ADR 0011](0011-image-artists.md) draws an image as one raster beneath one affine transform. In polar, cylindrical and spherical axes each pixel is an annular sector, a patch of a cylinder wall or a patch of a sphere, which no affine transform places. [ADR 0021](0021-images-refused-on-logarithmic-axes-and-alpha-and-opacity.md) refused images on logarithmic axes for the same reason.

## Decision

An image item in the display list has a geometry: the existing rectangle, or a mesh. A mesh is a lattice of nodes, each with a position in the image's pixel space, a point on the page and, in 3D, a depth. It places the image piecewise affinely between them. Along a logarithmic axis the nodes lie on the pixel edges, so every pixel lies exactly between its edges mapped through the axis. In curved projections the lattice is subdivided as a representing shape is. The GPU draws a mesh as textured triangles. PDF writes a mesh image with few enough cells as one vector path per pixel, and rasterises a larger one with the export warning `RasterisedForWarp`.

This supersedes the part of ADR 0021 that makes an image on a logarithmic axis the error `ImageOnLogAxis`. ADR 0021 held that pixels of unequal width on the page misrepresent evenly spaced data. Placed edge by edge, each pixel covers exactly the interval of data it describes, as a bar does on a logarithmic axis. An image whose pixel edges a logarithmic axis cannot place is the error `InvalidImagePlacement`.

## Alternatives considered

### Refusing images in curved and logarithmic axes

Refusing images would leave every polar heat map, warped spectrogram and globe to be rebuilt as a surface by the figure writer, although the image already holds the data and the placement.

### One path per pixel everywhere

Paths place every pixel exactly, but a large image becomes millions of paths, and adjacent paths show hairline seams in most viewers. Paths are kept only for small images in PDF, where they stay vectors.

### Resampling in the compiler

The compiler could resample the image into a page-aligned raster. The result would depend on the zoom at which it was made, would need resampling again on every pan, zoom or rotation, and would blur the pixel boundaries that an image of cells shows.

### A new display-list primitive

A separate warped-image primitive would duplicate the texture, opacity, colour-scale and hit-testing handling of the image item. A geometry field extends the image item, so every backend changes only how it places the texture.
