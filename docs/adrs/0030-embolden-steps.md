# ADR 0030: Emboldening in steps of designed faces

**Status:** Accepted

**Related:** [ADR 0004](0004-pdf-first-export-with-krilla.md)

## Context

A figure drawn for print has lines and text too fine to read on a projector. One figure should serve both, so a figure-level property `embolden` draws the figure heavier without changing what it says.

## Decision

Emboldening has three steps: regular, medium and semibold. At each step, plain text is set in the STIX Two Text face of that weight. Bold text, monospace text and mathematics have no heavier face, so their glyph outlines are stroked, thickening their stems by the same amount as the step thickens the stems of plain text. Every line and edge is thickened by the ratio of the step's stem width to the regular stem width, measured from the fonts. Marker sizes, tick lengths and spacing are unchanged. Heavier faces are wider than the regular face, so text measures wider at each step, and the legends sized from it grow slightly; this is the cost of designed faces over stroking alone, which would leave every measurement unchanged. An export may draw a figure at a step other than its own without changing the figure.

## Alternatives considered

### A continuous factor applied by stroking every glyph

A single number would allow any degree of emboldening, but stroking adds the same thickness to the thick stems and the thin hairlines of a glyph. A stroked glyph therefore loses the contrast of the design, its counters close and its letters crowd together, increasingly so as the factor grows. Designed faces keep the contrast, counters and spacing that their designer drew. Stroking is kept only for faces that have no heavier weight, and only by the small amounts that the steps require.

### Variable-weight fonts

A variable STIX Two Text would give a continuous range of designed weights. However, STIX Two Math has one weight, so mathematics would still have to be stroked. Both the PDF exporter and the canvas would also have to instance variable fonts. Three static faces give the steps that projection needs without either cost.

### Scaling marker sizes and tick lengths

Scaling these would make them easier to see from a distance, but a marker's size can represent data, and changing it would change what the figure shows.

### Separate figures for print and projection

An author could build a second figure with heavier styles. Such figures diverge as either is edited. An override at export draws both from one figure.
