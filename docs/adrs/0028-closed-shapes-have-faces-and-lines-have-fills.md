# ADR 0028: Closed shapes have faces and edges, and lines have fills

**Status:** Accepted

## Context

Bars, stairs and cells each draw an area and an outline, and the model already names these parts in two ways: the face and edge of a surface or a marker, and the line and fill of a contour.

## Decision

An artist whose primary mark is a closed shape (bars, cells, the faces of a surface and markers) has a face and an edge. An artist whose primary mark is a line that may shade an area (stairs and contours) has a line and a fill.

## Alternatives considered

### One pair of words for every artist

Naming every area a face, or every area a fill, would give one vocabulary. It would, however, describe either an open step line as having edges, or a closed bar as having a line.
