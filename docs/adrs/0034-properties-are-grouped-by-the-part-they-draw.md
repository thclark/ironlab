# ADR 0034: Properties are grouped by the part they draw

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0008](0008-typed-edits-and-a-view-overlay.md), [ADR 0023](0023-titles-labels-names-and-tags.md), [ADR 0028](0028-closed-shapes-have-faces-and-lines-have-fills.md)

## Context

The figure model grew without one rule for structuring properties. A part was turned off in six ways:
- a `visible` flag;
- a noun used as a flag, such as `grid` and `box`;
- other flags, such as `boxed` and `show_key`;
- an "off" variant of an enumeration, such as `TickLabels::Hidden`, `DashStyle::None`, `MarkerShape::None` and `ColorSpec::None`;
- the absence of an optional value;
- an empty list of ticks.

A part's style sat sometimes inside it, as in `line: LineStyle`, and sometimes beside it, as in `fill_opacity`, `edge_width_pt`, `title_align` and `tick_label_angle`. Titles and labels were optional text, so a label could not be kept, for tools and the viewer to refer to, while not being drawn.

## Decision

Every part that is drawn is a structure of its own. Its first field is `visible`, followed by its own style and placement, and no property of a part sits beside it. Parts nest: an axis holds its line, tick marks, tick labels, grid and label, and a marker holds its face and edge.

Whether a part exists and whether it is drawn are separate properties. An optional value says whether the part exists, which is structure that the figure's code decides. `visible` says whether it is drawn, which the viewer may change. A hidden part keeps its content and reserves no space.

`visible` is the only flag that means "drawn". Other flags describe behaviour, such as `reverse` and `clip`, and no enumeration has a variant that means "not drawn".

## Alternatives considered

### Flat fields with a prefix

Fields such as `line_visible`, `line_colour` and `line_width_pt` need no nesting. However, the number of fields grows with every property of every part. Such fields cannot be shared between artists as one type, and nothing in the model shows which fields belong together.

### Groups by property

Groups such as `visibility.line` and `colour.line` collect one kind of setting. However, they scatter the settings of one part across several structures, so moving, copying or explaining a part means visiting all of them. Grouping by part keeps everything about a part in one place. It also matches the property paths that the viewer groups rows by, such as `x.line.visible`.

### Optional values alone

Absence could mean "not drawn", as it did for titles and labels. A hidden title would then lose its text. The viewer, which edits properties but does not build figures, could not hide a part and show it again.

### "Off" variants in enumerations

An enumeration variant such as `DashStyle::None` turns a part off inside one of its styles. This gives each part its own way of being hidden. A tool would have to know every such variant to tell whether something is drawn.
