# ADR 0025: A series replaces the line and scatter artists

**Status:** Accepted

**Related:** [ADR 0009](0009-view-dependent-decimation-with-source-index-maps.md), [ADR 0013](0013-depth-buffered-three-dimensional-artists.md), [ADR 0019](0019-axis-direction-ticks-blended-coordinates-and-aspect.md) and [ADR 0024](0024-colour-scales-colourbars-and-colour-keys.md)

## Context

A line and a scatter were two artist kinds that drew the same data in different ways: a line joined its points and could mark them, and a scatter marked its points and could not join them. Each property the two shared (coordinates, blended coordinates, clipping, markers) was declared, validated, encoded and drawn twice, and each property only one had (the scatter's per-point sizes and colours) was unavailable to the other ([issue #19](https://github.com/thclark/ironlab/issues/19)). A trajectory coloured by speed, which needs a line whose colour varies along it, could not be drawn ([issue #44](https://github.com/thclark/ironlab/issues/44)).

In a three-dimensional axes, a line was drawn as one path with one depth, so a line passing behind a surface and then in front of it was painted wholly behind or wholly in front of it in the painter's order that the PDF exporter relies on.

## Decision

### One artist for points that may be joined and marked

`Series` replaces `Line` and `Scatter`. It holds the coordinates of its points (`x`, `y` and optionally `z`), a line style and a marker style, either of which may draw nothing, and optionally `size_data`, `color_data` and the `color_scale` through which colour data is mapped. A line with no markers is the former line, and markers with no line are the former scatter. The blended coordinates and clipping of [ADR 0019](0019-axis-direction-ticks-blended-coordinates-and-aspect.md) apply to every series. The facade keeps `plot`, `scatter` and their variants as entry points that set the defaults of each family, and returns one handle, `SeriesMut`.

### Colour data colours points or segments

Colour data is mapped through the scale the series names, of any kind, as [ADR 0024](0024-colour-scales-colourbars-and-colour-keys.md) requires of every colour-mapped artist.

- **One value per point** colours each marker, and draws each line segment as a gradient between the colours of its two points. The gradient interpolates between the two colours, not between the two values, so that a segment between two categories never passes through the colour of a third.
- **One value per segment** colours each segment in one colour, and the markers take their own colours. A series that draws no line refuses such data, because it has no segments to colour, and a marker coloured by the colour scale refuses it, because no value belongs to a point.

A gradient is drawn as single-colour pieces, because neither backend strokes a path with a colour that varies along it. A segment is cut into the fewest pieces of equal length that are no longer than 1 pt on the page, and into no more than 256 pieces, so that no piece of a segment drawn in several pieces is shorter than half a point. The number of pieces depends only on the figure and its size, so the same figure always draws the same pieces. A missing colour value draws neither its point's marker nor the two segments that meet it. Decimation by [ADR 0009](0009-view-dependent-decimation-with-source-index-maps.md) keeps each drawn point's colour by its index in the source data, and a thinned segment coloured per segment takes the colour of the first source segment it replaces.

### Three-dimensional lines are ordered by segment

In a three-dimensional axes each segment of a line is its own item, carrying the depths of its two points, so that the painter's order places each segment correctly against the faces it passes. Each corner between two drawn segments is filled with a disc as wide as the line, drawn just behind the segments, because separate segments would otherwise leave a notch at every corner. Round caps were not used for this, because a stroke has one cap style for both of its ends and every dash, so round caps would also have rounded the ends of each run and each dash. Dash patterns continue from one segment to the next, and the ends of each run keep the line's own caps. In two dimensions a line without colour data is still stroked as one path.

## Alternatives considered

### Adding colour data to the line artist alone

Giving the line per-point colours while keeping the scatter would have resolved issue #44 but left every shared property declared twice, and would have added a second form of colour data to keep consistent with the scatter's.

### Interpolating values along a gradient

Mapping the values interpolated along a segment through the scale would follow a continuous scale exactly, but would pass through unrelated entries between two categories or palette entries, and would draw bands within one segment of a discrete scale. Interpolating the two resolved colours behaves the same for every kind of scale.

## Consequences

- `Line`, `Scatter`, `ScatterSize` and `ScatterColor` are removed from the schema, and their JSON forms are no longer read. Programs that built them must build a `Series`; the facade's entry points are unchanged in name.
- A three-dimensional line is drawn as separate segments with a disc at each inner corner, which changes the corners of three-dimensional lines slightly compared with a single stroked path, and makes their PDF output larger.
- A series that draws markers but no line strokes its marker edges at 0.5 pt; with a line, the marker edge takes the line's width clamped between 0.5 pt and 1.5 pt.
- Two-dimensional figures that use neither colour data on a line nor any other new property draw exactly as before.
