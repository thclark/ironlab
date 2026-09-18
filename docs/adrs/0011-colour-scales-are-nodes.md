# ADR 0011: Colour scales are nodes that axes and artists refer to

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0007](0007-mvp-scope.md), [ADR 0008](0008-typed-edits-and-a-view-overlay.md)

## Context

An axes currently holds its colormap and its colour limits as two inline properties, and every colourmapped artist of the axes is drawn through them. Three planned features cannot be built on that structure:

- **A colorbar** ([issue #8](https://github.com/thclark/ironlab/issues/8)) must show the mapping from values to colours without holding a copy of it, so that it cannot disagree with the mapping when the colour limits change interactively.
- **Image artists** ([issue #7](https://github.com/thclark/ironlab/issues/7)) need colours for values below the lower limit, above the upper limit and for values that are not numbers, and an indexed image needs a colormap whose colours are given explicitly.
- **An artist drawn through a mapping of its own**, which the inline properties cannot express at all. The rule wanted is that the colour scale attached to the axes is what the colorbar shows, the colour scale attached to an artist is what that artist is rendered through, and an artist with none of its own inherits that of its axes.

A colorbar beside a filled contour must also be divided exactly as the contour is. Today a contour derives its levels from the range of its own field, which a colorbar cannot know, so the two can disagree.

The [concept discussion](../background/concept-discussion.md) states that a colorbar is an IR node linked to a colormap and colour limits. Its intent, that a colorbar never holds a second copy of the mapping, is kept. A colour scale is such a node. A colorbar is not: it is a decoration of an axes, as a legend is, and holds nothing that could disagree with the scale it shows.

## Decision

### A colour scale is a node of the figure

A `ColorScale` is stored once in `Figure::color_scales`, a map keyed by `NodeId`, and is a node in its own right: it has an identifier drawn from the same space as those of the axes and the artists, a kind (`NodeKind::ColorScale`), and properties that are read and set at its own property paths. A scale is drawn nowhere and has no position in the drawing order, so it is not inserted, moved or removed by the structural edits of [ADR 0008](0008-typed-edits-and-a-view-overlay.md); it is created, replaced and removed by two edits, `PutColorScale` and `RemoveColorScale`, which mirror `PutData` and `RemoveData`. The viewer lists the colour scales of a figure in its object tree, where one is selected to be inspected, and a scale has no region on the canvas.

```rust
pub struct ColorScale {
    /// The name of the quantity the scale maps, which a colorbar draws as its title.
    pub name: Option<Text>,
    /// The colormap through which values between the colour limits are mapped.
    pub colormap: Colormap,
    /// The data values mapped to the first and last colours of the colormap.
    pub clim: Limits,
    /// The levels into which the scale is discretised, or `null` when it is continuous.
    pub levels: Option<Levels>,
    /// The colour of values below the lower colour limit.
    pub below: Color,
    /// The colour of values above the upper colour limit.
    pub above: Color,
    /// The colour of values that are not finite numbers.
    pub missing: Color,
}
```

The three colours `below`, `above` and `missing` default to fully transparent for every colourmapped artist, so that poorly defined data is visibly absent instead of silently saturated.

### Axes and artists refer to a scale by identifier

An axes holds `color_scale: NodeId`, which replaces its `colormap` and `clim` properties, and an axes always names a scale. Every artist holds `color_scale: Option<NodeId>`, in which an absent value means the scale of the axes that holds the artist.

Because references point from the axes and the artists to a scale and never the other way, removing an axes or an artist cannot leave a dangling reference, and no removal needs to cascade. A reference that names no colour scale of the figure is reported by validation as a warning, and the axes or artist is drawn through the default scale, because a default mapping always exists.

The automatic colour limits of a scale are the range of the finite colour values of every artist of the figure that resolves to that scale. An artist with a scale of its own therefore takes its limits from its own data alone, and the automatic limits of an axes' scale come only from the artists that inherit it.

The properties of a scale are set on the scale and never through a node that refers to it. Setting the colour limits is `Set { node, path: "clim", value }`, in which `node` is the identifier of the scale, whichever axes and artists are drawn through it. An axes or an artist has the single property `color_scale`, which holds the reference and has no paths beneath it. A change to a scale therefore has one address, so an entry of the viewer's overlay and a transaction of the owner that change the same property of the same scale are compared by node and path exactly as for any other property, however many axes share the scale. Which scale a node refers to is structure that the program builds, so the viewer shows the reference read-only, in the same way as it shows a data reference.

### Discretisation belongs to the scale, and is called levels

A scale is continuous, or it carries `Levels`, which is the type a contour already uses:

```rust
pub enum Levels {
    /// This many values, equally spaced from the lower colour limit to the upper one inclusive.
    Count { count: u32 },
    /// These values, which are finite and strictly increasing.
    Explicit { values: Vec<f64> },
}
```

A level is a value that has a colour. Level `i` of `k` takes the colour at the fraction `i / (k - 1)` of the colormap, so a custom colormap of `k` colours on a scale of `k` levels yields exactly those colours. A contour holds `levels: Option<Levels>`, in which an absent value means the levels of its scale, and the following rules apply.

| Artist | Scale | Result |
| --- | --- | --- |
| Continuous (a surface, a scatter, a mapped image) | Continuous | The artist and the colorbar are smooth. |
| Continuous | Discretised | The artist is rendered smoothly through the colormap and the colour limits. The colorbar shows the levels of the scale, which is coarse but correct. |
| Contour without levels of its own | Discretised | The contour draws the levels of the scale, so the contour and the colorbar agree by construction. |
| Contour without levels of its own | Continuous | The contour draws ten levels. |
| Contour with levels of its own | Continuous | The contour draws its own levels and the colorbar is smooth. |
| Contour with levels of its own | Discretised | Valid when the two are equal. When they differ, in count, in values or in form, validation reports an error. |

Two discretisations are therefore never combined, and a smooth colorbar is never shown where a discretised one is meant. An isoline at a level takes the colour of that level, and a filled region between two consecutive levels takes the colour of the lower one. The values of a contour's levels are resolved from the resolved colour limits of its scale and not from the range of the contour's own field, which is what allows a colorbar to show them. Automatic colour limits are not rounded, so a count of levels over automatic limits can give values that are not round; an author who wants round values sets the colour limits or gives the values explicitly.

### A colormap is a built-in name or a list of colours

```rust
pub enum Colormap {
    /// A colormap that IronLAB ships, named by a closed enumeration.
    Builtin { name: ColormapName },
    /// A colormap written out in the figure as two to 1024 colours, ordered from the
    /// lowest value to the highest.
    Custom { colors: Vec<Color> },
}
```

The enumeration is closed, so that a name that does not exist cannot be written, the generated schemas list every name, and the property editor offers a list. A custom colormap interpolates linearly in sRGB between its colours when it is used continuously. A colormap carries no rule about stepping: discreteness is a property of the scale alone, and an indexed image selects colour `i` by direct lookup.

### A colorbar is an optional property of an axes

```rust
pub struct Colorbar {
    /// The side of the plot area on which the bar is drawn.
    pub location: ColorbarLocation,
    /// Where the tick marks and their labels are placed along the bar.
    pub ticks: ColorbarTicks,
}

pub enum ColorbarTicks {
    /// Chosen when the figure is drawn.
    Auto,
    /// At the given values.
    At { values: Vec<f64> },
    /// No ticks and no tick labels.
    None,
}
```

An axes holds `colorbar: Option<Colorbar>`, as it holds its legend. A colorbar is drawn when the axes holds one and is not drawn otherwise; nothing is inferred from the artists or from the colour limits. The colorbar holds no colormap, no colour limits, no levels and no title: it shows the colour scale of its axes, and takes its title from the name of that scale, so the author states what the bar describes. An axes has at most one colorbar.

On a continuous scale, automatic ticks are placed at round values within the colour limits, and given tick values must lie within the colour limits. On a discretised scale, the bar is drawn as one step of colour per level, automatic ticks are placed one per level at the centre of its step and labelled with the value of the level, and given tick values must be as many as the levels. The `below` and `above` colours are drawn as caps at the ends of the bar.

## Alternatives considered

- **A colour scale that is not a node, edited through the axes or artist that refers to it.** A property path such as `color_scale.clim` of an axes would have resolved through the reference into a table of scales. One property of one shared scale would then have had as many addresses as nodes referring to it, so the overlay could not tell that a change by the owner through one axes and a change by the user through another were changes to the same value.
- **A colour scale held inline by the axes and optionally by each artist.** This needs no table, but two artists or two axes could not share a scale, and a shared quantity would be named and limited in several places that could drift apart.
- **A colorbar that names an artist.** The artist knows whether it is drawn in levels, but the reference would dangle when the artist is removed, so every removal would have to cascade, and the bar would imply that it describes one artist when it describes every artist drawn through the scale.
- **A colorbar that surveys the artists of its axes to decide whether it is discretised.** Adding a second contour with different levels would silently change a discretised bar into a smooth one. Holding the levels on the scale gives the bar one reference and nothing to survey.
- **A stepped or smooth blending rule on a custom colormap.** This would have put discreteness in two places. Sampling level `i` of `k` at `i / (k - 1)` gives an indexed image and a classification their exact colours with no such rule.
- **Rounding automatic colour limits outward so that levels fall on round values.** This would make the automatic limits of a discretised scale differ from those of a continuous one over the same data.

## Consequences

- The schema changes incompatibly: the `colormap` and `clim` properties of an axes are removed, the `levels` property of a contour becomes optional, and `Levels::Auto` is renamed `Levels::Count`. The project has no users yet, so no compatibility is provided, and the minor version of the schema is incremented.
- A figure read by hand takes two lookups to find the colours of an artist, as it already takes two to find its data.
- Setting colour limits by hand now leaves values outside them undrawn, where they were previously drawn in the end colours of the colormap.
- The scene compiler resolves every colour scale of the figure in one pass before it measures the axes decorations, because the tick labels of a colorbar enter the margins of its axes.
- The colour of a filled contour region changes from the colour at the middle of the region to the colour of its lower level, and the levels of a contour that inherits them span the colour limits of its scale.
- The façade configures colour through a handle to the scale, `ax.color_scale()`, and its `colormap` and `clim` methods on an axes are removed, so that a line of code that changes a shared scale reads as one.
- A colorbar for a scale other than that of its axes, and a key of labelled swatches for a classification, are deferred to [issue #24](https://github.com/thclark/ironlab/issues/24). `Colormap::Custom` is a variant with named fields so that per-colour labels can be added to it without a further incompatible change.
- The image artists of [issue #7](https://github.com/thclark/ironlab/issues/7) consume these structures and define none of their own for colour.
