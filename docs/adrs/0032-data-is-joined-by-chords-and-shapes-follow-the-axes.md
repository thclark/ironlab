# ADR 0032: Data is joined by chords, and shapes that represent data follow the axes

**Status:** Accepted

**Related:** [ADR 0031](0031-polar-coordinates-are-projections-over-the-existing-axes.md), [ADR 0003](0003-shared-scene-compiler-and-display-list.md)

## Context

In a non-Cartesian projection, a straight edge between two points in data space is a curve on the page. Every artist must decide whether to draw its edges as straight lines on the page or as curves that follow the axes.

## Decision

An artist that draws data directly (a series, its fill, a contour, a surface or a quiver) maps each data point and joins consecutive points with straight lines on the page, as in Cartesian axes. An artist that draws a shape representing data (a bar, a stair run, a cell or an image pixel) builds its outline from edges at a constant coordinate in data space, and that outline follows the axes, subdivided until its chord error is below 0.05 pt. A bar is therefore a wedge whose top is an arc, and a radar outline is a straight polygon.

The plot region of a polar axes is clipped by a sector clip in the display list, which each backend applies, rather than by clipping geometry in the compiler.

## Alternatives considered

### Interpolating every edge in data space

matplotlib interpolates the edges of lines in polar axes, so a line between two points becomes an arc. This invents values between the samples that the data does not hold, and a line from 0° to 180° becomes a half circle rather than a diameter. A figure writer who wants a curve supplies the points of the curve, as in Cartesian axes.

### Joining every edge with chords

Drawing representing shapes with chords would make a full-turn bar a triangle, and a stair run a polygon whose radius changes along its length. The shape would no longer show its value.

### Clipping geometry in the compiler

The compiler would have to clip polylines, filled polygons, markers and images against arcs. A clipped stroke would lose its joins and caps at the boundary, and markers would be cut along chords. A sector clip in the display list clips every draw exactly at the curved boundary, with one test per fragment on the GPU and a clip path in PDF.
