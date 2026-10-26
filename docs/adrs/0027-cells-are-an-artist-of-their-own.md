# ADR 0027: Cells are an artist of their own

**Status:** Accepted

**Related:** [ADR 0011](0011-image-artists.md) and [ADR 0024](0024-colour-scales-colourbars-and-colour-keys.md)

## Context

Two-dimensional histograms and occupancy matrices are grids of cells, each holding one value. The cells may have uneven edges, may lie along logarithmic axes, and may be separated by gaps.

## Decision

A grid of cells is a new artist rather than an extension of an existing one.

## Alternatives considered

### Extending the surface or the scalar image

The values of a surface belong to the nodes between its faces, not to the faces themselves. A scalar image has evenly spaced pixels and cannot be placed on a logarithmic axis. Neither has gaps between its elements. Each could be extended to accept one value per face, or uneven edges and gaps. It would then carry properties that make sense only for coarse grids of categories, alongside those of an artist also used for large meshes and images.
