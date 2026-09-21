# ADR 0011: Image artists as pixel-centred planar rasters

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0009](0009-view-dependent-decimation-with-source-index-maps.md), [ADR 0010](0010-pdf-export-with-a-raster-fallback.md)

## Context

[Issue #7](https://github.com/thclark/ironlab/issues/7) asks for MATLAB's `image`, `imagesc` and `pcolor`. Those three functions describe three different things, and their names confuse them. `image` draws either true colour or colormap indices, depending on the shape of its array and on its `CDataMapping` property; `imagesc` is `image` with the mapping set to scale the data through the colour limits; and `pcolor` is a surface seen from above, whose cells are bounded by the vertices of a grid and which therefore drops the last row and column of its data. What a reader needs to know about a raster is what a pixel holds, and each answer to that question needs a different treatment of the colormap.

The colormap architecture of the scene compiler is about to be redesigned. An axes holds one colormap and one pair of colour limits, the lookup tables are fixed at 256 entries, and the colour scale clamps every value into the table. Images need more than that scale gives them, and the way they get it must not pre-empt the redesign.

The display list of [ADR 0003](0003-shared-scene-compiler-and-display-list.md) gained an image primitive in [ADR 0010](0010-pdf-export-with-a-raster-fallback.md) for the raster fallback of the PDF exporter, which reserved it for image artists and left open whether the display list should also carry unmapped samples with their colour mapping. The viewer reports the index a picked point has in the user's own data ([ADR 0009](0009-view-dependent-decimation-with-source-index-maps.md)), and a pixel of an image needs the same treatment.

## Decision

### Three kinds of image rather than MATLAB's three functions

The figure model gains three artist kinds, distinguished by what a pixel holds and where its colour comes from.

| Kind | A pixel holds | Its colour comes from | MATLAB |
| --- | --- | --- | --- |
| `image` | Red, green, blue and optionally alpha components (`pixels`) | The pixel itself | `image` with an m×n×3 array |
| `indexed_image` | An index into the axes colormap (`indices`) | The entry at that index, whatever the colour limits | `image` with `CDataMapping` `direct` |
| `mapped_image` | A data value (`values`) | The colormap, indexed by the value scaled through the colour limits | `imagesc`, or `image` with `CDataMapping` `scaled` |

The data fields carry these descriptive names rather than MATLAB's `CData`, so that each kind states what its array holds. The facade offers `image`, `indexed_image` and `mapped_image` and no `imagesc` alias: one name per kind is a clearer story than MATLAB's, in which one function draws two kinds and a property decides which. `pcolor` is a surface, not an image, and is out of scope; it is enabled by lifting the restriction that a surface needs a three-dimensional axes, not by an image kind.

### An image is a raster of pixel centres in a plane

A pixel has a centre and one flat colour, and an image is never a mesh. It is planar: it lies in one coordinate plane of its axes, which in a two-dimensional axes is the plane of the axes and in a three-dimensional axes is the floor or one of the two walls, and it is not two-dimensional geometry that could be bent over a grid.

Its placement is explicit. An `ImagePlacement` names the plane (`xy`, `xz` or `yz`, each with an offset along the third axis, or none for the low end of that axis) and, along each axis of the plane, a `PixelRange` giving the coordinates of the centres of the first and last pixels. The pitch follows from the number of pixels, the image extends half a pitch beyond each centre, an absent range centres the pixels on 0 to n − 1, and a range whose last centre lies before its first mirrors the image. Row 0 of the array always lies at the first row centre. Stretching, translating and flipping an image are therefore written in the figure, whereas MATLAB's `image` shows its rows top-down by reversing the direction of the y axis, so that the same array is drawn one way up on its own and the other way up over a plot. An axis-direction property is a separate concern and is listed as follow-up work.

### Data arrays gain an 8-bit element type in the IR's own array record

Pixels are bytes, and holding them as 64-bit floating-point numbers would cost eight times their size in memory, in `.fig` files and in JSON. The array record of the model (`NdArray`) therefore gains an element type: 64-bit floating-point values or 8-bit unsigned integers. The JSON form gains an `element` tag that is written only for bytes, and the Protocol Buffers form an `NdArrayElement` enum with a `bytes u8_values` payload, so that files written before the element existed still load. The three image kinds accept either element; every other artist requires floating-point values, which validation checks.

The model keeps its own array record rather than adopting a type from `ndarray` or `nalgebra`. The record defines the wire and JSON forms of an array, the `.proto` files and the JSON Schema are generated from it, and `ironlab-ir` carries no dependency into every reader of a figure. Conversions from those libraries belong in the facade and are follow-up work.

### A colour-indexed image is a direct lookup

An index names an entry of the colormap without any mapping: a floating-point index is truncated toward zero, and an index from 0 to 255 takes that entry. The colour limits play no part; the indices neither use them nor contribute to them, so a segmentation map keeps its colours whatever else is drawn in the axes, and a colour-mapped image beside it keeps its colour limits. MATLAB rounds an index down, and truncation agrees with that for every index from 0 upwards, so the half-open interval from 0 to 256 maps onto the 256 entries with each entry owning one unit of it; the two differ only between −1 and 0, which truncation gives to entry 0 rather than to the category below the colormap.

### Out-of-range pixels have a policy per category

A pixel of a colour-indexed or colour-mapped image that the colormap cannot colour falls in one of three categories: `below` (an index less than 0, or a value below the lower colour limit), `above` (an index greater than 255, or a value above the upper colour limit) and `non_finite` (an index or value that is NaN or infinite). Each category holds its own `OutOfRange` policy: `strict`, which makes such a pixel a validation error; `transparent`, which draws nothing; `clamp`, which draws the nearest end colour of the colormap and nothing at `non_finite`, where there is no nearest end; or a fixed colour.

The default is lenient on every category, so that ink reaches paper quickly: a figure reaches the page whatever its data holds, with the pixels that cannot be coloured visibly absent rather than the whole figure refused. Strictness is switchable on any combination of the three categories for data that must be good, so that missing values may be tolerated while a value outside the range is refused, or the reverse. One tagged type per category was chosen over reusing `ColorSpec`, which would have overloaded `colormapped` to mean the nearest end colour, and over one policy for all three categories, which cannot express those combinations. Validation checks a strict category against the data, and the compiler applies the policies pixel by pixel, skipping an artist that a strict policy refuses with one warning that names the artist, the first offending pixel and the category.

### One transform-carrying group is the single emission path

The compiler draws an image of any kind, in a two- or three-dimensional axes, as one image item in pixel space beneath one group whose affine transform maps pixel space into figure space. Three corners of the pixel rectangle are mapped through the axes, by the axis maps in two dimensions and by the projector in three, both of which are affine on linear axes, and the transform follows from them. Mirroring, stretching and projection are carried by the transform, the samples stay in row order, and the inverse of the same transform is what the hit map records. There is no second code path for three dimensions and no reordering of samples. An image whose plane touches a logarithmic axis, on which the mapping is not affine, is not drawn, with one warning naming it; drawing it as one path per pixel is follow-up work.

In a three-dimensional axes the image is one primitive in the painter's order. An image on a face of the axes box, which an image whose plane has no offset always is and one whose offset equals a limit of the third axis also is, is painted behind everything else when that face is at the back of the current view and in front of everything else when it is at the front, because nothing inside the box can lie beyond a face of it. An image at an interior offset is sorted by the mean depth of its four corners, with the limitation of the painter's algorithm that a surface crossing its plane is painted wholly before or after it.

### Images are embedded at their own resolution

The image item of an image artist is the primitive that ADR 0010 introduced, and it takes the same route to the page as the raster fallback: the PDF exporter embeds it as a deflated image XObject without interpolation, beneath the transform of its group, with a soft mask only when it has alpha. It is embedded pixel for pixel, at its native resolution, and it is never marked dense, so no backend resamples the pixels the user supplied and the export resolution of ADR 0010 does not apply to it. A very large image therefore makes a large file. Resampling on export is follow-up work, and it must be asked for, because it changes the reader's data.

### Backends draw textured quads

The canvas draws an image item as textured quads through the same transform chain as every path, cut into tiles of at most 8192 pixels on a side, which is the default largest texture side of wgpu, capped further by the device where it allows less. The textures are sampled with nearest filtering, so that pixel edges stay as hard on screen as they are in the PDF. Textures come from a provider that is asked lazily, tile by tile, for the tiles that are visible: the interactive canvas keeps a cache keyed by the sample buffer and the tile, holding the buffer so that a reused address cannot serve a stale texture, and prunes the cache after each rebuild of its meshes; the offscreen renderer uploads the tiles of one render and frees them afterwards, with egui's predictable texture filtering turned off, because with it on egui's shader filters bilinearly whatever the sampler asks for.

### The hit map records pixels and datatips read them

For every image drawn in a two-dimensional axes, the hit map records the inverse of the placement transform with the numbers of rows and columns, from which the pixel under a pointer is found. A datatip reports the row and column of the artist's own array, the coordinates of the pixel's centre from the placement rather than the pointer's position, and what the array holds there: the value of a colour-mapped image, the untruncated index of a colour-indexed image, or the components of a true-colour pixel. A drawn point within reach of the pointer wins over the pixel beneath it, because it is small and painted on top. Images in three-dimensional axes have no entry, because the hit map cannot tell what occludes them; picking there waits for the GPU picking pass.

### The answer to ADR 0010's open question

The display list keeps carrying resolved samples. A second image form holding unmapped samples with their colour mapping would make a change of colour limits a uniform update instead of a re-mapping of every pixel, but it must carry the colour scale, which is what the colormap redesign is about to change, and defining it now would fix the shape of the scale before the redesign. It can be added beside the present form without disturbing it.

## Consequences

- An image reaches the screen and the page by one route: the compiler resolves its pixels once, and every backend draws the same image item beneath the same transform, so the canvas, the gallery and the PDF cannot disagree about where a pixel lies or what colour it has.
- How an image is stretched, shifted or flipped is stated in the figure, so a saved figure reopens with the same orientation everywhere, and a datatip can name the row and column of the user's array whichever way the image was placed.
- Every array carries an element type, and a figure of the previous schema version is rejected, which is accepted while the project is pre-stability. Files written before the element existed load unchanged, because an absent element is floating-point.
- A colour-indexed image is independent of the colour limits, so it does not contend with a colour-mapped image in the same axes; a colour-mapped image shares the axes' colormap and colour limits with every other colormapped artist and cannot use a colormap of its own.
- Undefined and out-of-range data is visibly absent by default, and a figure is refused only where its author asked for strictness.
- The samples are resolved on the CPU at compilation and re-uploaded when the scene is recompiled, so a very large colour-mapped image is slow to pan, and a tile of 8192 by 8192 pixels with alpha occupies 256 MiB of texture memory, with a larger image split into several resident tiles.
- An image cannot be placed on a logarithmic axis, an exported PDF grows with the number of pixels of its images, and images in three-dimensional axes have no datatips.
- The property editor changes the placement, the plane and the policies of an image; the pixels, indices and values are data, which come from the program that builds the figure.

### Colormap workarounds to feed forward

Each of the following is a deliberate stopgap around the present colormap architecture, to be removed or moved when the colour scale is redesigned.

1. Out-of-range policies live on the two mapped image kinds; surfaces, contours and scatters keep clamping and paint nothing for NaN. The redesigned scale should own the handling of values under, over and outside the range for every artist.
2. The classification into below, inside and above is done in the image code over `normalise`, because `ColourScale::colour` clamps; the scale should expose it once.
3. `clamp` is defined by the fixed 256-entry table (`lut[0]` and `lut[255]`), as is the domain 0 to 255 of a colour-indexed image; colormaps of other lengths, which are planned, must supply their length and ends.
4. One colormap and one pair of colour limits per axes; an image cannot use a colormap of its own.
5. Samples are resolved on the CPU per compilation and re-uploaded per gesture; the GPU lookup table and the unmapped image form wait for the redesign, since that form must carry the scale.
6. Legend samples for the mapped kinds use the middle colour of the colormap until a colourbar ([#8](https://github.com/thclark/ironlab/issues/8)) exists.

### Follow-up work

Follow-up work is tracked in GitHub issues:

- [#32: Convert ndarray and nalgebra arrays into Matrix, GridCoords and Pixels](https://github.com/thclark/ironlab/issues/32)
- [#33: Add an axis direction property so images can be shown in matrix orientation](https://github.com/thclark/ironlab/issues/33)
- [#34: Add an opacity property to image artists](https://github.com/thclark/ironlab/issues/34)
- [#35: Draw images on logarithmic axes as one path per pixel](https://github.com/thclark/ironlab/issues/35)
- [#36: Read pixel datatips of images in three-dimensional axes](https://github.com/thclark/ironlab/issues/36), which needs the GPU picking pass of [#1](https://github.com/thclark/ironlab/issues/1)
- [#37: Resample very large images at export resolution on request](https://github.com/thclark/ironlab/issues/37)
- [#38: Add an interpolation property for images](https://github.com/thclark/ironlab/issues/38)
- [#39: Allow surfaces in two-dimensional axes to give pcolor](https://github.com/thclark/ironlab/issues/39)
- [#40: Let colormaps of other lengths define the domain of indexed images](https://github.com/thclark/ironlab/issues/40)
