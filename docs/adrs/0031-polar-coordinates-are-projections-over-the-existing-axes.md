# ADR 0031: Polar coordinates are projections over the existing axes

**Status:** Accepted

**Related:** [ADR 0019](0019-axis-direction-ticks-blended-coordinates-and-aspect.md), [ADR 0032](0032-data-is-joined-by-chords-and-shapes-follow-the-axes.md)

## Context

Polar, cylindrical and spherical axes place data by angle and radius. Every artist, the facade and the viewer already address the x, y and z axes of an axes, with their limits, scales, ticks, grids and directions.

## Decision

Polar, cylindrical and spherical axes are variants of the axes' projection, and their coordinates are carried by the existing x, y and z axes. In every one of them x carries the azimuth. In polar and cylindrical axes y carries the radius, and in cylindrical axes z carries the height. In spherical axes y carries the polar angle and z the radius. The x limits are the sector, `x.reverse` is the angular direction, and the radius is mapped from its lower limit at the centre. A radius beyond the lower limit has no position: it is never reflected through the centre.

A radar chart is a circular polar axes whose spokes are categorical angular ticks, with its values drawn as a filled series.

## Alternatives considered

### Separate angle and radius axis fields

Fields such as `theta` and `r` would name the coordinates directly, but every artist, limit computation, tick locator, link, edit path and inspector row would need a second form. Reusing x, y and z keeps one form, so every artist draws in the new axes through its existing code.

### Converting to Cartesian coordinates in the facade

The facade could convert angles and radii into x and y before storing them. The stored figure would then lose its coordinates. Its ticks, grid and datatips could not show angles, and an edit in the viewer could not change the sector or the direction.

### The radius on y in spherical axes

Keeping the radius on y in every projection would be uniform. However, the natural data of a spherical axes, such as the gain of an antenna, is a field over the two angles. With the angles on x and y and the radius on z, `surf(azimuth, elevation, r)` draws it directly, as a surface over x and y does in Cartesian axes.

### A separate direction property

A `clockwise` property would duplicate `x.reverse`, which already means "drawn from the upper limit to the lower". Two properties with the same effect could contradict each other.

### Reflecting a negative radius through the centre

matplotlib draws a point of negative radius on the opposite side of the centre. This places two different values at one point, and a radius below the lower limit of a non-zero or logarithmic radial range has no meaningful reflection. Such a value therefore breaks a line, as a non-finite value does.

### A polygonal grid or a radar artist for radar charts

A polygonal grid makes charts with different numbers of spokes look different, and a radar artist would duplicate the series. A filled series on circular axes needs neither.
