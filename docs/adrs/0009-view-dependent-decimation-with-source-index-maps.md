# ADR 0009: View-dependent decimation with source index maps

**Status:** Accepted

**Related:** [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0007](0007-mvp-scope.md), [ADR 0008](0008-typed-edits-and-a-view-overlay.md)

## Context

A series of a million points drawn into a plot a few hundred points wide cannot show a million distinct positions. Most of its geometry is finer than any display or printer resolves, so it costs frame time on the canvas and bytes in the exported file without changing the picture. [ADR 0007](0007-mvp-scope.md) excluded thinning such series from the MVP, and recorded the resulting limit on performance with large data sets.

Where the thinning is done decides whether it can be trusted. Were the canvas to thin a series for the screen while the PDF exporter drew it in full, or thinned it by a different rule, a user would publish a figure they had never inspected. That is the failure the single scene compiler of [ADR 0003](0003-shared-scene-compiler-and-display-list.md) exists to prevent, and thinning is exactly the kind of geometric decision that ADR reserves to the compiler.

Which points matter depends on the view rather than on the data alone: on the axis limits, the axis scales, the size of the plot rectangle and, in three dimensions, the camera. The viewer records every gesture as typed edits of the displayed figure ([ADR 0008](0008-typed-edits-and-a-view-overlay.md)) and recompiles after each one, so a decision made inside the compiler is already recomputed whenever the view changes.

Thinning also introduces a second numbering of a series. Once only some of its points are drawn, a picked point has an index into what was drawn, which is not the index it has in the arrays the user plotted. The [background discussion](../background/concept-discussion.md) warned that retrofitting the map between the two means touching every path from the data to the screen, and that it belongs in the decimator's output type from the first implementation. A datatip that named the wrong measurement would be worse than no datatip at all.

## Decision

### Thinning belongs to the scene compiler

A line or scatter that holds more points than its plot rectangle can resolve is thinned by `ironlab_scene::compile`, before its geometry reaches the display list. The canvas, the offscreen renderer and the PDF exporter therefore draw the same thinned series, and the exported figure cannot disagree with the one on screen. No backend thins anything, and no backend may.

Thinning is applied to positions in figure space, which are the result of the whole view transform, so the axis limits, the axis scales, the plot rectangle and the three-dimensional camera all decide which points survive. It is recomputed on each compilation rather than cached against the data, because the view changes far more often than the data does and a stale selection would show detail belonging to a view the reader has left.

### Two rules, one for each shape data takes

- A **line** is thinned by the largest-triangle-three-buckets rule: the points are divided into consecutive buckets and the one forming the largest triangle with its neighbours is kept from each. It preserves the peaks, troughs and corners that carry the shape of a curve, which keeping every *n*th point loses; a transient in a signal is usually the reason the figure was drawn.
- A **set of markers**, whether a scatter or the markers of a line, is thinned by keeping one marker per square of half a marker width: the one that would be painted over the others, which is the deepest in three dimensions and otherwise the last in source order. Markers are discrete symbols far wider than the spacing of a dense series, so a square of the plot is the unit in which they are distinguishable, and the cost of a scatter is then set by the area of its plot rather than by the size of its data.

Each rule is applied to a run of consecutive drawable points, never to the series as a whole. A non-finite value breaks a line, as the line artist's contract requires, and thinning a series across such a break would draw a triangle over the gap and join the two halves.

Segments and markers that the axes clip away are dropped before either rule is applied. They paint nothing, so removing them changes nothing on the page, and it spends the whole allowance on what the reader can see: zooming into a thousand points of a million draws those thousand in full.

### The target is fixed and derived from the plot rectangle

The compiler aims for four drawn points per point of plot width, with a floor of 256 points, and a series no longer than that target is drawn in full. One point is 1/72 inch, so four samples per point resolves detail 1/288 inch apart, which is finer than a display shows or print resolves; the floor keeps a curve recognisable in a plot only a few millimetres wide.

The target is not user-controllable. Thinning at this resolution preserves the appearance of a figure rather than choosing it, so it is not a stylistic decision to expose. A setting would also let a figure be exported at a different fidelity from the one at which it was designed, which is the failure this decision exists to prevent. `Figure::parameters` would in any case be the wrong home, because parameters describe a figure for sorting and searching and are defined not to affect drawing.

### The index map is carried in the drawn point

The unit of thinning is a sample, which holds a point's position in figure space, its painting depth, and the index it has in the artist's own data arrays:

```rust
pub struct Sample {
    pub source_index: usize,
    pub position: Point,
    pub depth: f64,
}
```

Every stage between an artist's arrays and the display list — mapping into figure space, splitting into runs, clipping to the view, thinning, and recording what was drawn — takes and returns samples. There is deliberately no type that holds drawn positions without their indices, and no pair of parallel collections that a caller could index inconsistently, so the map cannot be dropped and cannot fall out of step.

The hit map publishes the drawn points of every line and scatter as samples of that same type, so the type the decimator produces is the type picking consumes. A front end reports the index the user's arrays use, never a position in the thinned series, and reads the values it shows from those arrays at that index.

## Consequences

- The screen and the PDF cannot disagree about which points of a series are drawn, and the cost of drawing a figure is bounded by the size of its plots rather than by the size of its data.
- A series no longer than the target is drawn exactly as before, including the parts of it that lie outside the axes, so no existing figure changes.
- Picking and datatips report indices into the user's data. The viewer's hover datatip is the first consumer; data cursors and pinned datatips ([#2](https://github.com/thclark/ironlab/issues/2)) and the GPU picking pass ([#1](https://github.com/thclark/ironlab/issues/1)) will read the same map, and the second of those must carry the source index through to the shader rather than inventing a second numbering.
- Only points that were drawn can be picked. A datatip cannot name a point that thinning removed, and the remedy is to zoom in, which draws more of the series.
- The triangle areas that the line rule compares all scale together when an axis is rescaled, so zooming a linear two-dimensional axes does not by itself change which points are chosen. The selection follows the view through the target, through the clipping of what is off screen and through non-linear scales and the three-dimensional camera. The clipping is what makes zooming sharpen a curve.
- Dropping clipped-away segments restarts the dash pattern at each end of a visible stretch of a thinned dashed line. The joins concerned are off the page and the phase already moves with every pan, so this is accepted; it never affects a series short enough to be drawn in full.
- The allowance is spent per run rather than per series, so a series broken into many runs by non-finite values can draw more points in total than the target. Keeping breaks exact is worth more than a hard limit on the whole series.
