# ADR 0021: Images refused on logarithmic axes, and alpha data and opacity for image, surface and contour artists

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0008](0008-typed-edits-and-a-view-overlay.md), [ADR 0011](0011-image-artists.md), [ADR 0012](0012-empty-and-singleton-data.md) and [ADR 0013](0013-depth-buffered-three-dimensional-artists.md), each of which this record supersedes in part

## Context

[ADR 0011](0011-image-artists.md) draws an image as one raster beneath one affine transform. A logarithmic axis is not affine, so an image whose plane touches one cannot be placed. ADR 0011 left such an image out of the drawing with a warning, and listed drawing it as one filled path per pixel as follow-up work. [ADR 0012](0012-empty-and-singleton-data.md) then cited that warning, `ImageOnLogAxis`, as one of the family of warnings for an artist drawn with less than its data holds.

Drawing one path per pixel would place every pixel, but it would not give the result a reader expects. The pixels of an image are evenly spaced in the data, so on a logarithmic axis each pixel would span an interval of the data that the image does not describe, and the pixels would have unequal widths on the page. Data sampled along a logarithmic axis is better described as a surface, whose nodes are positions in the data and are each placed through the scale of their axis. Most figures are built by programs, and many by agents acting for a user, and both respond better to a constraint that names the right artist than to a drawing that quietly misrepresents the data.

Separately, the only control of transparency the image, surface and contour artists had was the alpha channel of a true-colour image and the alpha of a fixed colour. A colour-mapped image, a surface or a filled contour could not be made translucent, and no artist could be faded uniformly without rewriting its data. [ADR 0013](0013-depth-buffered-three-dimensional-artists.md) recorded a related limitation of the viewer: a translucent face is drawn with depth writes, so one drawn before something behind it hides it.

## Decision

### An image on a logarithmic axis is an error

An image whose plane has a logarithmic axis is the validation error `ImageOnLogAxis`, and the figure is neither exported nor shown. Only the axes of the plane count; the z axis of a two-dimensional axes is never drawn, so its scale does not matter. The message names the image, its plane and the logarithmic axis, and states how to draw the same values as a surface in the case at hand: in a two-dimensional axes, a rectilinear grid of the pixel centres with the values as the field; on the floor of a three-dimensional axes, the same grid at a constant height with the values as colour data; and on a wall, a curvilinear grid that holds the wall's fixed coordinate. A plane offset to a coordinate that is not positive along a logarithmic third axis is an `InvalidImagePlacement` error rather than `ImageOnLogAxis`, because the plane itself can be drawn and the remedy is a positive offset, not a surface.

Edits are validated as [ADR 0008](0008-typed-edits-and-a-view-overlay.md) describes, so an edit that would make an image's axis logarithmic, such as a change of scale in the viewer, is refused with the same message. The scene compiler, which never fails, still leaves such an image out with a warning if it is handed one.

This supersedes the part of ADR 0011 that leaves such an image out with a warning and lists drawing it as one path per pixel as follow-up work, and the part of ADR 0012 that counts `ImageOnLogAxis` among the warnings. The rule of ADR 0012 stands: an error is for a figure that is wrong, and an image placed on an axis that cannot carry it is wrong.

### Alpha has the shape of the data; opacity is one number

Two properties control transparency, and they differ in kind. **Alpha** is an array with the shape of an artist's data, which varies transparency across the artist, for example to fade a measurement by its confidence. **Opacity** is a single number from 0 to 1, defaulting to 1, which fades the whole artist; it is the compact way to express a uniform transparency, and the property editor can change it without regenerating any data. The alpha an artist is drawn with is the product of the alpha of its colour, its alpha data and its opacity.

- `Image` gains `opacity`. It has no alpha array, because a true-colour image with varying transparency has an alpha channel.
- `IndexedImage` and `MappedImage` gain `alpha`, an optional array with the shape of their indices or values, holding floating-point values from 0 to 1 or 8-bit values from 0 to 255 as image arrays may, and `opacity`. The fixed colour of an out-of-range policy is multiplied by both, like every other pixel.
- `Surface` gains `face_alpha` and `edge_alpha`, optional floating-point arrays with the shape of `z`, and `face_opacity` and `edge_opacity`. Faces and edges are controlled independently because they are different elements of the drawing: a translucent surface usually keeps opaque edges, and a mesh may fade its edges alone. A face takes the mean of the alpha at its four corners, as it takes the mean of their colour data, and so does its edge.
- `Contour` gains `line_opacity` and `fill_opacity`, for its isolines and its filled bands, which are controlled independently for the same reason as the faces and edges of a surface: filled bands are commonly made translucent to show what lies beneath them while the isolines stay opaque. A contour has no alpha array, for the reasons given under the alternatives considered.

An alpha value outside `[0, 1]` is clamped, and a non-finite alpha makes its pixel or face transparent, as a non-finite component of a true-colour pixel does. An opacity that is not a finite number from 0 to 1 is the validation error `InvalidOpacity`. The legend sample of an artist takes its opacity but not its alpha, because an alpha varies over the artist and has no single value to show.

### Translucent draws of a depth group are drawn last, without depth writes

The canvas records whether each draw paints any colour with an alpha below 1. Inside a depth group, the opaque draws are drawn first with the depth test and depth writes, and the translucent draws after them, in the painter's order in which the compiler sorted them, with the depth test but without depth writes. A nearer opaque face still hides a translucent one behind it, and a translucent face never hides what lies behind it, wherever it comes in the order. This supersedes the consequence of ADR 0013 that a translucent face drawn before something behind it hides it. The PDF exporter is unchanged: as ADR 0013 decides, it writes a three-dimensional axes as vectors only when its painter's render and its depth-tested render prove to be the same picture, and embeds the depth-tested render otherwise.

## Alternatives considered

### Drawing an image on a logarithmic axis as one path per pixel

This is the follow-up work that ADR 0011 proposed. It was rejected because it draws a picture that misrepresents the data, as the context describes, at the cost of one path per pixel, whereas a surface describes the same values truthfully and is already drawn on every axis.

### A single opacity for a surface

A surface could have had one opacity for faces and edges together. It was rejected because the commonest translucent surface keeps opaque edges, which a single opacity cannot express without giving up the colormap for the edges.

### Alpha for each level of a contour

A contour could have had an alpha for each isoline and for each filled band. It was rejected because such an array is indexed by levels rather than by data, unlike every alpha of this decision. Under automatic levels the number of levels is chosen when the figure is drawn, so its author cannot know the length of the array; the bands include the two open-ended bands beyond the extreme levels, and bands that the data does not reach are left out, so the index of a band depends on the data. An alpha with the shape of the field, as a surface has, does not fit a contour either, because each isoline and each band is drawn in a single colour. A contour therefore has an opacity for its isolines and one for its bands, and no alpha.

## Consequences

- A figure that held an image on a logarithmic axis, and which was previously drawn without the image, is now refused, and its program must draw the values as a surface. The [guide](../guides/getting-started.md#images-on-logarithmic-axes) shows the conversion.
- The schema gains the alpha and opacity properties, which a reader of an earlier version would ignore and so draw the artists opaque; the schema version therefore moves to 0.6.0, and a file of 0.5 is rejected.
- A figure that sets no alpha and no opacity below 1 compiles to the same display list as before, so no existing drawing changes.
- An image with alpha data or an opacity below 1 is resolved with four channels, and a PDF embeds it with a soft mask.
- The difference between alpha and opacity is taught in the [guide](../guides/getting-started.md#alpha-and-opacity) and stated exactly in the [figure schema reference](../reference/figure-schema.md#alpha-and-opacity).
- [Issue #34](https://github.com/thclark/ironlab/issues/34) and [issue #35](https://github.com/thclark/ironlab/issues/35), listed as follow-up work by ADR 0011 and ADR 0013, are resolved by this decision.
