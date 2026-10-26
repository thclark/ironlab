# ADR 0026: Bars are a primitive, coloured as series are

**Status:** Accepted

**Related:** [ADR 0024](0024-colour-scales-colourbars-and-colour-keys.md) and [ADR 0025](0025-series-replaces-line-and-scatter.md)

## Context

Grouped and stacked bar charts draw several series at each position, and the series must agree on how their bars are placed. Each series must nevertheless keep its own values, label, colour and legend entry. The author must keep control of every bar's width, offset and base, which histograms with uneven bins, floating bars and variable-width charts require. A bar chart also often colours its bars individually, for example to pick out one bar.

## Decision

### Bars are placed by their own geometry

A bar artist draws exactly the bars it describes and lays nothing out itself. Each bar is centred at its position plus the series' offset, which is a multiple of the bar's width or a distance along the category axis. Its width is a fraction of the pitch, a constant, or one width per bar. It runs from its base to its base plus its value, so that a value is a length, as in Matplotlib and Plotly. Grouping and stacking are facade helpers that write the offsets, widths and bases of ordinary bar series.

### Bars are coloured as series are

A bar series has one face colour specification. Individual bars are coloured through colour data and a colour scale of any kind, as the points of a series are, and a list of fixed colours is expressed as an indexed or categorical scale.

## Alternatives considered

### Bar groups in the model

A group could be a node that places its member series once, with members stacked by a shared stack name. The viewer could then switch between grouped and stacked layouts, and close a stack up when one of its series is hidden. But placing the bars in the group removes control from the bars, so that a series in a group could no longer have widths of its own. Each series could instead repeat the group's placement and join it by name, but then nothing would prevent the members from disagreeing.

### One bar artist holding a matrix of series

One artist could hold every series as a row of a matrix, as MATLAB's `bar(Y)` and pandas do. Legends, selection, the property editor and edit paths would then have to address series within an artist, which they do nowhere else.

### A list of colours on each bar series

A list of one colour per bar would be shorter to write. It would, however, be a second way of colouring the elements of an artist, alongside colour data and scales, and it could not key its colours in a legend.
