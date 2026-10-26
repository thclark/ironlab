# ADR 0029: Histograms store their bins and how they were made

**Status:** Accepted

**Related:** [ADR 0026](0026-bars-are-a-primitive-coloured-as-series-are.md) and [ADR 0027](0027-cells-are-an-artist-of-their-own.md)

## Context

A histogram counts a sample into bins. The sample may be very large, and a reader of the figure wants to know the count and density of each bin and how the bins were chosen.

## Decision

A histogram is binned when the figure is built, not when it is drawn. The figure stores the binned values on the bars, stairs or cells that draw them. With them it stores a record of how the bins were made: the bin specification, the range, the normalisation, the number of samples counted and the number excluded. A datatip can then report the count and density of a bin without the sample.

## Alternatives considered

### Storing the sample

Storing the raw sample would let the viewer rebin, but it would make the figure as large as the sample and put statistics into the model and the scene compiler.

### Storing only the binned values

Storing only the binned values would keep the figure smallest. But a datatip could then report neither counts nor densities, and nothing would record how the bins were chosen.
