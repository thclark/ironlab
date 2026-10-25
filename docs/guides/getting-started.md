# Getting started

This guide explains how to build figures with IronLAB's Rust API, how to save, load and export them, and how to open them in the interactive viewer. The complete set of properties that a figure can hold is described in the [figure schema reference](../reference/figure-schema.md), and working examples of every plot type are in the [gallery](../gallery/index.md).

## Installation

IronLAB requires Rust 1.95 or later. Add the `ironlab` crate to a project as a Git dependency:

```toml
[dependencies]
ironlab = { git = "https://github.com/thclark/ironlab" }
```

The fonts and the LaTeX mathematics typesetter are compiled into the library, so nothing else needs to be installed. The prelude imports every type and function used in this guide:

```rust
use ironlab::prelude::*;
```

## Figures, tiles and axes

A `Figure` is a page of a fixed physical size. Its properties are set with builder methods when it is created.

```rust
let mut fig = Figure::new()
    .size_mm(160.0, 100.0) // width and height in millimetres (the default)
    .tiles(2, 3)           // a grid of 2 rows and 3 columns of tiles (MATLAB's tiledlayout)
    .title("Overall title") // drawn above all axes (MATLAB's sgtitle)
    .font_size_pt(9.0);    // base font size, from which titles and tick labels are scaled
```

Axes are placed in tiles, numbered from zero starting at the top-left tile.

- `fig.axes(row, col)` returns the axes in that tile, creating a two-dimensional axes if the tile is empty. Calling it again with the same tile returns the same axes.
- `fig.axes3(row, col)` does the same but makes the axes three-dimensional.
- `fig.axes_span(row, col, row_span, col_span)` returns an axes whose top-left tile is `(row, col)` and makes it cover the given numbers of rows and columns.

Tiles are of equal size by default. `row_weights` and `col_weights` give the rows and columns relative sizes, so that, for example, a wide plot and a narrow [legend panel](legends.md#legend-panels) can share a row; [sizing rows and columns with weights](legends.md#sizing-rows-and-columns-with-weights) explains how weights combine with spans, and `fit_row` and `fit_col` make a row or column as large as the [legend panels it holds](legends.md#fitting-rows-and-columns-to-their-legends).

Each of these returns an `AxesMut` handle. The handle borrows the figure mutably, so it is held only while one axes is being built. An axes is referred to later, for example when linking axes, by the identifier returned from `ax.id()`.

## Plotting functions

Plotting functions are methods of `AxesMut`. They follow MATLAB's names and argument order, store their data in the figure, and return a handle whose setters change the new plot's properties and can be chained. The [equivalent functions](../reference/equivalent-functions.md) page maps the function names of MATLAB, matplotlib and Plotly to IronLAB's. Vectors are passed as anything that can be viewed as a slice of `f64`, such as `Vec<f64>`, `&[f64]` or an array, and are copied into the figure; the identifier of an array that the figure already holds can be passed instead, so that several plots share one array, as [sharing data between plots](#sharing-data-between-plots) describes.

The helper functions `linspace(start, end, n)`, `logspace(start_exp, end_exp, n)` and `meshgrid(&x, &y)` create coordinates as in MATLAB. Gridded data is passed as a `Matrix`, which has one row per y coordinate and one column per x coordinate, so that the value in row `j` and column `i` belongs to the point `(x[i], y[j])`.

```rust
let x = linspace(-1.5, 1.5, 61);
let y = linspace(-1.5, 1.5, 61);
let z = Matrix::from_fn(y.len(), x.len(), |row, col| x[col].powi(2) - y[row].powi(2));
```

### Lines

`plot(x, y)` draws a line through the points `(x[i], y[i])`. The line is solid, 0.75 pt wide and has no markers, and its colour is the next colour of the axes colour order. `plot3(x, y, z)` draws a line in three dimensions and converts the axes to three dimensions.

Every point-plotting function (`plot`, `plot3`, `loglog`, `semilogx`, `semilogy`, `scatter`, `scatter3`, `hline` and `vline`) adds a **series**, a sequence of points drawn as a line through them, a marker at each, or both, and returns its handle, a `SeriesMut`. The functions differ only in the defaults they set, so every setter below applies to a series drawn by any of them: a scatter can be joined by a line, and a line can be given markers sized and coloured by data.

```rust
let mut ax = fig.axes(0, 0);
ax.plot(&x, &y)
    .label("$x^2$")
    .color(Color::rgb(0.0, 0.45, 0.7))
    .line_width(1.5)
    .dash(Dash::DashDot)       // Solid, Dashed, Dotted, DashDot, DashDotDot, LongDash, or None to draw only markers
    .marker(Marker::Square)    // Circle, Square, Diamond, TriangleUp, TriangleDown, Plus, Cross, Point, VLine, HLine
    .marker_size(5.0)
    .marker_face(Color::WHITE)
    .marker_edge(Color::BLACK);
```

Non-finite values (such as `f64::NAN`) break a line into separate pieces.

### Logarithmic axes

`loglog(x, y)`, `semilogx(x, y)` and `semilogy(x, y)` draw a line as `plot` does and set the scales of the x and y axes to logarithmic, as MATLAB's functions of the same names do. The scale of any axis can also be set directly with `xscale`, `yscale` and `zscale`. Points with a non-positive coordinate on a logarithmic axis are not drawn, and validation warns about them. An image cannot be drawn on a logarithmic axis at all, and data sampled along one is drawn as a surface instead, as described in [images on logarithmic axes](#images-on-logarithmic-axes).

```rust
let f = logspace(0.0, 4.0, 200);
let gain: Vec<f64> = f.iter().map(|f| 1.0 / (1.0 + (f / 100.0).powi(2)).sqrt()).collect();
fig.axes(0, 0).loglog(&f, &gain);
```

### Scatter plots

`scatter(x, y)` draws a marker at each point, and `scatter3(x, y, z)` does so in three dimensions. The series has no line, and its markers are unfilled 4 pt circles in the next colour of the colour order unless changed.

```rust
fig.axes(0, 0)
    .scatter(&x, &y)
    .size_data(&sizes)       // one diameter in points per marker; or .marker_size(6.0) for all markers
    .color_data(&values)     // one value per marker, mapped through a colour scale; or .color(Color::BLACK)
    .marker(Marker::Diamond)
    .filled();
```

`color_data` colours a line too: with one value per point, each segment of the line is a gradient between the colours of its two points, and with one value per segment, each segment takes a single colour. [Colouring a series by data](colour.md#colouring-a-series-by-data) describes both forms.

### Reference lines and edge markers

A reference line is a line that a reader judges the data against, such as a tolerance, a threshold or the ends of an interval. `hline(y)` draws a horizontal reference line across the whole width of the plot area at the data value `y`, and `vline(x)` draws a vertical one across its whole height at `x`, as matplotlib's `axhline` and `axvline` and MATLAB's `yline` and `xline` do. A reference line is a series like any other: it is dark grey, 0.75 pt wide and solid unless changed, it takes no colour from the colour order, and the usual line setters change its look.

```rust
let mut ax = fig.axes(0, 0);
ax.semilogy(&alpha, &error);
ax.hline(1.0).dash(Dash::Dotted);                 // a tolerance at 1
ax.vline(0.0).dash(Dash::Dotted).color(Color::BLACK);
```

A reference line spans the plot area because each of its dimensions is placed in its own coordinates. Along the axis it crosses, its value is data, placed through the limits and the scale of that axis. Along the axis it spans, its values are the fractions 0 and 1 of the plot area, which map to the left and right edges (or the bottom and top edges) whatever the limits, the scale or the pan and zoom of the axis. The data value takes part in the automatic limits of its axis, as it does in matplotlib, so a tolerance above the data is in view without fixing the limits; the fractions take part in none, so the line never stretches the axis it spans.

The same mechanism places any series. Setting `x_coords(Coords::AxesFraction)` or `y_coords(Coords::AxesFraction)` on a series places that dimension as fractions of the plot area, 0 at the left or bottom edge and 1 at the right or top edge, and leaves the other dimension in data coordinates. Setting `clip(false)` lets its marks extend beyond the plot area along the dimension placed as fractions, and only along that dimension: along the dimension in data coordinates the artist stays clipped to the plot area, so a marker at a data value outside the limits is not drawn. `clip(false)` therefore has no effect on a series in data coordinates along both dimensions, and validation warns about it. Together they draw markers along an edge of the axes at data positions, which stay on the edge while a reader pans and zooms. The vertical tick `Marker::VLine` and the horizontal tick `Marker::HLine` are made for this: each is one stroke as long as the marker size.

```rust
// A tick along the top of the axes at every angle where the value is not a number.
let missing: Vec<f64> = alpha
    .iter()
    .zip(&value)
    .filter(|(_, v)| !v.is_finite())
    .map(|(a, _)| *a)
    .collect();
ax.plot(&missing, vec![0.985; missing.len()])
    .y_coords(Coords::AxesFraction)   // 98.5 % of the way up the plot area
    .clip(false)                      // drawn whole where it overhangs the top edge; still clipped in x
    .dash(Dash::None)
    .marker(Marker::VLine)
    .color(Color::rgb(0.9, 0.62, 0.0));
```

Only a two-dimensional axes has a plot area to take a fraction of, so axes-fraction coordinates in a three-dimensional axes are reported as an error by [validation](#validating-a-figure), and a three-dimensional axes clips every artist to its tile whatever `clip` says. A datatip on such a series in the [viewer](viewer.md#datatips) reports its data coordinate only. The [comparison sheet](../gallery/comparison_sheet.md) in the gallery uses reference lines and edge markers together, and the [categorical strip chart](../gallery/strip_chart.md) uses vertical reference lines across its rows. The reasons for the design are recorded in [ADR 0019](../adrs/0019-axis-direction-ticks-blended-coordinates-and-aspect.md#blended-coordinates-place-a-coordinate-relative-to-the-plot-area).

### Contours

`contour(x, y, z)` draws isolines of the field `z`, `contourf(x, y, z)` fills the bands between levels, and `contour3(x, y, z)` draws each isoline at the height of its level in a three-dimensional axes. Each isoline and band is coloured by its level through the [colour scale](colour.md) of the axes. A contour without levels of its own takes them from its scale: ten levels at round values through a continuous scale, and the band edges of a discrete scale, as [discrete scales and filled contours](colour.md#discrete-scales-and-filled-contours) shows.

```rust
let mut ax = fig.axes(0, 0);
ax.contourf(&x, &y, &z).levels(12);                        // about 12 levels at round values
ax.contour(&x, &y, &z).level_values([-1.0, 0.0, 1.0]).color(Color::BLACK);
```

The coordinates `x` and `y` of any gridded plot are either vectors, which describe a rectilinear grid with one coordinate per column or row, or matrices of the same shape as `z`, which describe a curvilinear grid with coordinates for every node.

### Vector fields

`quiver(x, y, u, v)` draws an arrow with components `(u[i], v[i])` at each point `(x[i], y[i])`, and `quiver3(x, y, z, u, v, w)` does so in three dimensions. Arrows are scaled automatically so that they do not overlap.

```rust
let (xx, yy) = meshgrid(&x, &y);
fig.axes(0, 0)
    .quiver(xx.values(), yy.values(), u.values(), v.values())
    .scale(0.8)          // multiply the automatic scale; or .no_scale() to use lengths in data units
    .head_size(0.3)      // head length as a fraction of arrow length
    .line_width(0.5)
    .color(Color::BLACK);
```

### Surfaces

`surf(x, y, z)` draws the surface of the heights `z` over a grid, whose faces are coloured by height through the [colour scale](colour.md) of the axes and outlined by thin black edges. `mesh(x, y, z)` draws a wireframe whose faces are filled with the background colour, so that they hide the edges behind them, and whose edges are coloured through the colour scale. Both convert the axes to three dimensions.

```rust
fig.axes3(0, 0)
    .surf(&x, &y, &z)
    .edge_color(None)                   // hide the edges
    .face_color(ColorSpec::Colormapped) // the default for surf
    .edge_width(0.25)
    .color_data(&c);                    // colour by another matrix instead of by height
```

Wherever a colour is set, a `Color` gives a fixed colour, `None` draws nothing, and a `ColorSpec` chooses the automatic colour (`ColorSpec::Auto`) or a colour mapped through the artist's colour scale (`ColorSpec::Colormapped`). On the faces and edges of a surface and the isolines of a contour, which have no colour of their own, the automatic colour is the mapped colour.

A surface of ten thousand faces or more is exported as an image rather than as one path per face; see [dense surfaces](#dense-surfaces).

#### Surfaces in two-dimensional axes

`surface(x, y, z)` draws the same surface as `surf`, with the same defaults, and leaves the axes as it is: a two-dimensional axes stays two-dimensional, and a three-dimensional axes keeps its view. A two-dimensional axes shows a surface from directly above, which makes it a pseudocolour plot: the equivalent of MATLAB's `pcolor`, of matplotlib's `pcolormesh` and `pcolor`, and of Plotly's `Heatmap`. The grid alone places the faces. The field `z` positions nothing, and colours the faces unless `color_data` is given.

```rust
let radius = logspace(0.0, 0.6, 17);                  // rings from r = 1 to r ≈ 4, crowded towards r = 1
let angle = linspace(0.0, std::f64::consts::TAU, 73); // the first and last spokes coincide
let (theta, r) = meshgrid(&angle, &radius);           // the angle and the radius of every node
let x = Matrix::from_fn(r.rows(), r.cols(), |row, col| r[(row, col)] * theta[(row, col)].cos());
let y = Matrix::from_fn(r.rows(), r.cols(), |row, col| r[(row, col)] * theta[(row, col)].sin());
let field = Matrix::from_fn(r.rows(), r.cols(), |row, col| r[(row, col)].ln() * theta[(row, col)].sin());

let mut ax = fig.axes(0, 0);
ax.surface(x, y, &field)   // coordinate matrices make a curvilinear grid
    .edge_color(None);     // no edges, as MATLAB's `shading flat`
ax.color_scale().colormap(Colormap::Viridis);
```

Out of the box, a surface in a two-dimensional axes and a [scalar image](#images) differ in one respect only: the values of an image are pixels, and the values of a surface are vertices.

| | `scalar_image` | `surface` in a two-dimensional axes |
| --- | --- | --- |
| A value belongs to | the centre of a pixel | a vertex of the grid |
| A field of n rows and m columns draws | n × m pixels, each in the colour of its own value | (n − 1) × (m − 1) faces, each in the colour of the mean of its four corner values |
| Placement | evenly spaced pixel centres, given by the first and last centre along each axis | any rectilinear grid (two coordinate vectors) or curvilinear grid (two coordinate matrices) |
| Logarithmic axes | refused by validation with an `ImageOnLogAxis` error | drawn, because every vertex is placed through the scale of its axis |
| Drawn as | one flat raster at its own resolution | one quadrilateral per face, with optional edges |

An image therefore suits evenly sampled data such as a camera frame or a matrix, and a surface suits data on an uneven, logarithmic, polar or otherwise mapped mesh. Because a field of n rows draws n − 1 rows of faces, a field with a single row or a single column has no face: the figure is valid, the surface is not drawn, validation warns of it with a `NothingToDraw` warning that names the surface, and the [report](#exporting-pdf) that `export_pdf` returns names it too. MATLAB's `pcolor` differs in one detail: with its default shading it gives each face the value of its first corner, so the last row and column of the field are never shown, whereas IronLAB gives each face the mean of its four corners, so every value contributes.

A surface added with `surface` is an ordinary surface, so a later `surf` or `mesh` in the same axes converts that axes to three dimensions and the earlier surface is drawn there with `z` as its height. The [Flow past a cylinder](../gallery/cylinder_flow.md) gallery entry draws a field on a polar mesh in this way, and the [figure schema](../reference/figure-schema.md#surface) states how a surface is stored.

### Images

An image is a raster of pixels, each with one flat colour, placed by the coordinates of its pixel centres. Two functions draw the two kinds of image, which differ in where the colour of a pixel comes from.

- `image(&pixels)` draws a true-colour image (MATLAB's `image` with a true-colour array). Its argument is a `Pixels`: the red, green and blue components of every pixel, with an optional alpha component, one byte each. Pixels are built from the bytes of a decoded image with `Pixels::from_rgb8` or `Pixels::from_rgba8`, from a function of the row and column with `Pixels::rgb_from_fn` or `Pixels::rgba_from_fn`, or from one `Matrix` per component with `Pixels::from_planes`, to which `with_alpha` attaches a plane of opacities.
- `scalar_image(&values)` draws a scalar image, whose pixels each hold one number, mapped to a colour through a [colour scale](colour.md) exactly as the colour data of a surface is. Through the default scale of the axes, a continuous scale, it is MATLAB's `imagesc`. Through an indexed scale, each whole number names an entry of a palette, as MATLAB's `image` with an indexed array does; [indexed and categorical scales](colour.md#indexed-and-categorical-scales) shows how.

The values of a scalar image come from a `Matrix`, stored as floating-point values, or from a `ByteMatrix`, its 8-bit counterpart, which stores bytes exactly and at a quarter of the size.

```rust
let x = linspace(-2.0, 2.0, 81);
let y = linspace(-1.0, 1.0, 41);
let field = Matrix::from_fn(y.len(), x.len(), |row, col| (x[col] * y[row]).sin());
let classes = ByteMatrix::from_fn(y.len(), x.len(), |row, col| ((row + col) % 4 * 85) as u8);
let pixels = Pixels::rgb_from_fn(64, 64, |row, col| Color::rgb(row as f32 / 63.0, col as f32 / 63.0, 0.5));

let mut fig = Figure::new().tiles(1, 3);
let mut ax = fig.axes(0, 0);
ax.scalar_image(&field)
    .pixel_columns(-2.0, 2.0)  // the centres of the first and last columns
    .pixel_rows(-1.0, 1.0)     // the centres of the first and last rows
    .label(r"$\sin xy$");
ax.color_scale()               // the scale the image created as the default of its axes
    .colormap(Colormap::Magma)
    .limits(-1.0, 1.0)
    .non_finite(Color::BLACK); // a NaN is drawn in black instead of being left transparent
let palette = fig.indexed_scale(Colormap::Viridis.sample(256)).id();
fig.axes(0, 1)
    .scalar_image(&classes)    // each byte names an entry of the 256-colour palette
    .color_scale(palette)
    .pixel_columns(-2.0, 2.0)
    .pixel_rows(-1.0, 1.0);
fig.axes(0, 2).image(&pixels).pixel_columns(0.0, 1.0).pixel_rows(1.0, 0.0); // rows count down from the top
```

An image is placed by the centres of its first and last pixels along each axis of its plane: `pixel_columns(first, last)` places its columns and `pixel_rows(first, last)` its rows. The pitch between centres follows from the number of pixels, and the image extends half a pitch beyond each centre, so the scalar image above, with 81 columns centred from −2 to 2, has a pitch of 0.05 and covers −2.025 to 2.025. Without a range the centres lie at 0, 1, …, n − 1, so an image of n columns covers −0.5 to n − 0.5. Row 0 of the array always lies at the first row centre and column 0 at the first column centre, so a range whose `last` is less than its `first` mirrors the image along that axis, as the row range of the `image` call above does. [Image orientation](#image-orientation) shows how the two ranges turn, stretch and shift an image. The x and y limits of an axes that holds only images are the exact edges of their pixels, as they are for the grid of a contour or surface.

What a pixel draws when its value lies outside the range of its colour scale, names no entry of a palette, or is not finite, is decided by the scale rather than by the image, so that every artist that names the scale treats such values alike. [Values out of range](colour.md#values-out-of-range) describes the policies.

A colour scale is shown to the reader by a colourbar, in a panel beside the plots or inside the plot area, as [showing a scale with a colourbar](colour.md#showing-a-scale-with-a-colourbar) describes. In a figure made with `Figure::new().tiles(1, 2).fit_col(1)`, for example, `fig.panel(0, 1).colorbar(scale)` shows the scale with the identifier `scale` beside the plot in the first column.

In a three-dimensional axes an image lies in one of the three coordinate planes, chosen with `plane`. `ImagePlane::Xy { z }` is the floor, `ImagePlane::Xz { y }` and `ImagePlane::Yz { x }` are the walls, and the offset is the coordinate of the plane along the third axis, or `None` for the low end of that axis, so `plane(ImagePlane::Xz { y: Some(0.0) })` stands the image on the wall at y = 0. The columns of the image run along the first axis of its plane and the rows along the second. Placing an image on a wall converts a two-dimensional axes to three dimensions, as `surf` does, whereas the xy plane leaves the axes as it is, because it is the only plane a two-dimensional axes can show. An image on a face of the axes box, as an image without an offset is, is painted behind everything else in the axes when its face is at the back of the current view and in front of everything else when it is at the front; an image at an interior offset is sorted among the faces, lines and markers by its depth.

#### Images on logarithmic axes

An image is a raster of evenly spaced pixels, and a logarithmic axis has no place for one: equal steps in the data are unequal distances on the page, so the pixels of one image would have to be of different widths, and the width of each would span an interval of the data that the image does not describe. An image whose plane has a logarithmic axis is therefore a validation error, `ImageOnLogAxis`, and the figure is neither exported nor shown. Only the two axes of the image's plane count. The third axis of a three-dimensional axes, along which the plane is only offset, may be logarithmic, provided that an explicit offset along it is positive; a plane offset to zero or to a negative coordinate along a logarithmic third axis has no position and is an `InvalidImagePlacement` error. The z axis of a two-dimensional axes is never drawn, so its scale does not matter. Why such an image is refused rather than drawn one path per pixel is recorded in [ADR 0021](../adrs/0021-images-refused-on-logarithmic-axes-and-alpha-and-opacity.md).

Data sampled along a logarithmic axis, such as a spectrogram with a logarithmic frequency axis, is drawn as a [surface](#surfaces-in-two-dimensional-axes) instead. A surface is made of vertices, each of which is placed through the scale of its axis, so its faces stretch and shrink with the logarithmic axis exactly as the data does. The values that would have been the pixels become the field of a surface whose grid holds the coordinates of the pixel centres:

```rust
let time = linspace(0.0, 8.0, 241);                            // the centres of the columns
let frequency = logspace(20f64.log10(), 20e3f64.log10(), 241); // the centres of the rows
// `psd` stands for whatever computes the power spectral density at one time and frequency.
let power = Matrix::from_fn(frequency.len(), time.len(), |row, col| psd(time[col], frequency[row]));

let mut ax = fig.axes(0, 0);
ax.surface(&time, &frequency, &power).edge_color(None);
ax.yscale(Scale::Log);
```

The values of a surface belong to its vertices, so a grid of n rows and m columns draws (n − 1) × (m − 1) faces, each in the colour of the mean of its four corners, as the [comparison above](#surfaces-in-two-dimensional-axes) describes. A face that touches a vertex at a coordinate that is not positive on a logarithmic axis is left out, and validation warns of it with `NonPositiveOnLogAxis`. In a three-dimensional axes the same values are drawn on the floor by a surface whose field `z` holds the height of the floor at every node and whose colour data, given with `color_data`, holds the values; on a wall, a curvilinear grid holds the wall's constant coordinate in one of its coordinate matrices. A true-colour image has no values for a colour scale to colour, so its colours must be given as the values from which they were made. The [Spectrogram on a logarithmic frequency axis](../gallery/sweep_spectrogram.md) gallery entry draws a spectrogram in this way.

An exported PDF embeds every image at its own resolution, without resampling, so a PDF grows with the number of pixels rather than with the size of the figure. A `.fig` file stores the bytes of a `Pixels` or a `ByteMatrix` as one byte each, whereas JSON writes one number per component as text, so a large image is saved as `.fig`.

In the viewer, resting the pointer over an image in a two-dimensional axes reads the pixel beneath it, as described in [datatips](viewer.md#datatips). The gallery entries [Image](../gallery/image.md), [Mapped image](../gallery/mapped_image.md) and [Indexed image](../gallery/indexed_image.md) show a true-colour image and a scalar image through a continuous and through an indexed scale, [Image orientation](../gallery/image_orientation.md) shows one photograph with three placements, [Mapped image and surface](../gallery/mapped_image_and_surface.md) shows an image on the floor of a three-dimensional axes beneath a surface that covers it, and [Correlation peak in 3D](../gallery/correlation_peak.md) shows images in each of the three coordinate planes at explicit offsets, on a face of the box and inside it. The decisions behind the image artists are recorded in [ADR 0011](../adrs/0011-image-artists.md), and the replacement of its colour-indexed and colour-mapped kinds by the scalar image in [ADR 0024](../adrs/0024-colour-scales-colourbars-and-colour-keys.md).

#### Image orientation

Row 0 of the array lies at the first row centre, and the default placement puts that centre at y = 0, at the bottom of an axes whose y coordinate increases upwards. Row 0 of a decoded photograph is its top row, and row 0 of a matrix is the row that is printed first, so with the default placement a photograph appears upside down and a matrix appears with its first row at the bottom. The row range turns the image the right way up, and the same two ranges express every other axis-aligned transform. The three recipes below draw the same 256 × 256 `pixels`.

```rust
// Default placement: the pixel in row r and column c is centred on (c, r), so row 0 lies at the bottom.
ax.image(&pixels);

// Flip in y: the default row centres written from the last to the first. Row 0 lies at the top, and the image
// covers the same region as before. For n rows the range runs from n − 1 to 0.
ax.image(&pixels).pixel_rows(255.0, 0.0);

// Flip, stretch and shift in both directions. Both ranges descend, so the image is upright and mirrored in x. The
// column pitch is (50 − 458) / 255 = −1.6 and the row pitch is (200 − 404) / 255 = −0.8, so the image is 1.6 times
// as wide and 0.8 times as high as before. The first and last centres say where it lies.
ax.image(&pixels)
    .pixel_columns(458.0, 50.0)
    .pixel_rows(404.0, 200.0);
```

The second recipe gives an image of n rows the matrix orientation that MATLAB's `axis ij` and matplotlib's `origin="upper"` give: row 0 is at the top, and the rows count downwards. In IronLAB the y axis still increases upwards, so the pixel in row r is centred on y = n − 1 − r, and a line or marker drawn over the image uses that coordinate. A range in the units of the data, such as `pixel_rows(y_max, y_min)` for an image whose top row was sampled at `y_max`, serves the same purpose as matplotlib's `extent` and MATLAB's `XData` and `YData`, with the difference that the range names the centres of the outermost pixels rather than their outer edges.

!!! note "Why IronLAB has no axis-direction or image-origin property"

    IronLAB deliberately has no property that reverses an axis for an image or that moves the origin of an image to its top, such as MATLAB's `axis ij` or matplotlib's `origin`. Such a property applies a transform that is not written where the data is placed, and a reader of the plotting code must know the state of the axes to tell which way up the data lies. Out of the box, the only difference between an image and a [surface in a two-dimensional axes](#surfaces-in-two-dimensional-axes) is that an image holds pixels and a surface holds vertices, as the comparison table in that section sets out. Both follow the same convention: the first row and the first column lie at the first coordinate given, and the y coordinate increases upwards. From that consistent representation, every flip, stretch and shift is written explicitly in the placement of the image, which makes it easier to understand what happens to the data. The benefit is greatest when the data is volumetric and images and surfaces share an axes, because every artist in that axes is then placed in the same coordinates by the same rule.

The [Image orientation](../gallery/image_orientation.md) gallery entry draws the three recipes side by side in axes with the same limits, and the [figure schema](../reference/figure-schema.md#image-placement) states how a placement is stored.

### Alpha and opacity

Surfaces, images and contours can be drawn partly transparent. **Alpha** has the shape of the data: an array that gives every node of a surface, or every pixel of an image, its own transparency, so that it varies over the artist. **Opacity** is one number from 0 (transparent) to 1 (opaque, the default) for the whole artist. What is drawn is their product: a pixel whose alpha is 0.6 in an image whose opacity is 0.5 is drawn with an alpha of 0.3.

Use alpha when the transparency is part of the data, such as a mask or the confidence of each measurement, and opacity when the whole artist should be fainter, for example so that what lies beneath it shows through. An opacity can be changed in the [property editor](viewer.md#changing-a-property) of the viewer without regenerating any data, and it is the compact way to make an artist uniformly translucent: an RGB image with an opacity of 0.5 says the same as an RGBA image whose alpha is 0.5 everywhere, in three quarters of the bytes. The reasons for the two properties, and for controlling the faces and edges of a surface independently, are recorded in [ADR 0021](../adrs/0021-images-refused-on-logarithmic-axes-and-alpha-and-opacity.md).

```rust
let mut fig = Figure::new().tiles(1, 2);

// A scalar image whose pixels fade with the confidence of each measurement. A NaN confidence makes its pixel
// transparent, and a `ByteMatrix` of alpha from 0 to 255 is read as fractions of 255.
fig.axes(0, 0)
    .scalar_image(&speed)
    .alpha(&confidence)
    .opacity(0.9);

// A surface whose faces take their alpha from the confidence and are translucent throughout, so that the contours
// beneath them show through, and whose mesh lines are fainter still.
let mut ax = fig.axes3(0, 1);
ax.contour(&x, &y, &speed);
ax.surf(&x, &y, &speed)
    .face_alpha(&confidence) // one alpha per node: a face takes the mean of its corners
    .face_opacity(0.8)
    .edge_opacity(0.25);     // `edge_alpha` gives the edges an alpha of their own
```

`alpha` is available on `scalar_image`, whose pixels are a single value each; a true-colour `image` carries its alpha in its pixels, built with `Pixels::from_rgba8`, `Pixels::rgba_from_fn` or `with_alpha`, and takes an `opacity` like a scalar image. A surface has `face_alpha` and `edge_alpha`, one value per node of its field, and `face_opacity` and `edge_opacity`, so that its faces and the mesh of edges that borders them are controlled independently. A contour has `line_opacity` for its isolines and `fill_opacity` for its filled bands, and no alpha array, because each isoline and band is drawn in a single colour. An alpha value outside 0 to 1 is clamped, a NaN makes its pixel or face transparent, and the alpha of an image also applies to a pixel drawn in the fixed colour of an [out-of-range policy](colour.md#values-out-of-range). An opacity outside 0 to 1 is reported by [validation](#validating-a-figure) as `InvalidOpacity`.

In a three-dimensional axes a translucent face or image never hides what lies behind it, wherever it comes in the order in which the axes sorts its artists; [three-dimensional axes in PDF](#three-dimensional-axes-in-pdf) explains when such an axes is written to a PDF as an image. The legend sample of a translucent artist takes its opacity. The [Measured speed faded by its confidence](../gallery/piv_confidence.md) gallery entry draws both examples above, and the [figure schema](../reference/figure-schema.md#alpha-and-opacity) states exactly how the two combine.

## Sharing data between plots

A figure holds the values that its plots draw as **data arrays**, and each plot refers to its arrays by identifier rather than containing them. Values passed to a plotting function, such as a `Vec<f64>`, a slice or a `Matrix`, are copied into a new data array that has no name. Each plot therefore owns its own copy, and the program may change or drop its values after the call without affecting the figure. A record whose channels are all sampled at the same times would, however, store its time base once for every channel it plots.

`fig.data(name, values)` adds a data array with a name to the figure and returns a `DataArrayMut` handle, whose `id()` method returns the identifier of the array and whose `name(..)` method renames it. Every data argument of a plotting function accepts that identifier in place of values. A plot given an identifier refers to the existing array instead of copying it, so the array is stored once however many plots draw it, in any axes and in any role:

```rust
let t = linspace(0.0, 10.0, 10_001);
let mut fig = Figure::new().tiles(2, 1);
let time = fig.data("time (s)", &t).id();
fig.axes(0, 0).plot(time, &pressure);    // the time base is shared, and the pressure copied
fig.axes(1, 0).plot(time, &temperature);
```

Identifiers and values can be mixed argument by argument, as in this example, where the time base is shared and each channel is copied. The handle of a series also returns the identifiers of the arrays it refers to, through `x_data()` and `y_data()`, so that the arrays of one plot can be given to another without first being made with `fig.data`:

```rust
let mut ax = fig.axes(0, 0);
let x = ax.plot(&t, &pressure).x_data();
ax.plot(x, &pressure_filtered); // shares the x values that the first line copied
```

The name of a data array identifies it to tools and is never drawn: the viewer's [property editor](viewer.md#selecting-an-object) lists the data arrays of a figure in a group of their own, each named by its name, as "Data array (time (s))", or by its shape when it has no name. An identifier that names no data array is accepted by the plotting function and reported by [`validate`](#validating-a-figure), as the error `UnknownData` when it names no node of the figure and as `WrongNodeKind` when it names a node of another kind, such as an axes.

Two rules follow from sharing, and both are worth knowing before a program relies on it.

- **Editing a shared array changes every plot that refers to it.** A data array has one set of values, so replacing or appending to them, for example with the `replace_data` and `append_data` [edits](../reference/figure-schema.md#edits) that stream new values into a figure, changes every plot that draws the array. Plots whose data must change independently need arrays of their own, which is what passing values gives them.
- **A setter deletes the array it replaces only when that array is no longer used and has no name.** The setters that give an existing plot a new array, namely `size_data` and `color_data` of a series, `color_data`, `face_alpha` and `edge_alpha` of a surface, and `alpha` of an image, accept an identifier or values in the same way as plotting functions. The array that the setter replaces is removed from the figure only when no other plot refers to it and it has no name. An array copied from values has no name, so it is removed as soon as nothing draws it. An array made with `fig.data` always has a name, so it is kept even when no plot refers to it any more, and `validate` then reports it with the warning `UnusedData`, because it is saved with the figure and enlarges every file.

The [decimated time series](../gallery/decimated_timeseries.md) of the gallery shares its record between its plots in this way. The reasons for keeping data arrays as nodes of the figure, and for this deletion rule, are recorded in [ADR 0022](../adrs/0022-figure-nodes-in-one-identifier-space-and-panels.md), and the [figure schema](../reference/figure-schema.md#data-arrays) describes how data arrays are stored.

## Titles, labels and LaTeX

Axes titles and labels are set on the axes handle, and each setter returns the handle so that they can be chained.

```rust
ax.title("Pressure field")
    .xlabel("$x$ (m)")
    .ylabel(r"$\sigma_{xx}$ (Pa)")
    .zlabel("$z$");
```

A title is centred over the plot area unless `title_align` says otherwise. `title_align(TitleAlign::Left)` puts its left edge on the left edge of the plot area, not of the tile, which suits a long title that names the parameter a panel varies and the ones it holds fixed; `TitleAlign::Right` does the same on the right.

```rust
ax.title("Position of maximum camber $p$ ($m = 0.04$, $t = 0.12$)")
    .title_align(TitleAlign::Left);
```

A title, an axis label or a tick label too long for the page is drawn clipped by the edge of the page, and the scene warning `TextOutsidePage`, which names the axes or the figure and quotes the text, says so. It is returned by `fig.layout_warnings()` and in the report of an [export](#exporting-pdf), so a program learns of a clipped title without looking at the page.

Every piece of text is stored as its source. By default, segments delimited by `$…$` are typeset as LaTeX mathematics and the rest is set as plain text, so `"$x$ (m)"` shows an italic x followed by an upright unit. A literal dollar sign inside such text is written `\$`. Text that should be shown exactly as written, dollar signs included, is created with `Text::plain`:

```rust
ax.ylabel(Text::plain("Cost in $"));
```

Outside mathematics, `\texttt{…}` sets its argument in monospace, which suits code identifiers such as the name of a case or a file. IronLAB bundles no monospace font: the letters and digits are taken from the Mathematical Monospace alphabet of STIX Two Math, and every character, punctuation and spaces included, occupies a cell of the same width. Within the argument, an underscore may be written bare or as `\_`, so that an identifier can be pasted unchanged; `\{`, `\}`, `\$`, `\&`, `\%` and `\#` produce the character after the backslash, and a `$` is a literal dollar sign rather than the start of mathematics. Monospace text is copied from a PDF as the characters that were written. Inside mathematics, `\texttt` and `\mathtt` set letters and digits in the same alphabet, but follow the rules of mathematics, in which spaces are dropped and a bare `_` starts a subscript.

```rust
let fig = Figure::new().title(r"Polars of \texttt{naca4412_n160_polar30_re1e6_iter100}");
```

Mathematics is typeset in STIX Two Math and plain text in STIX Two Text. Within mathematics, a hyphen is set as a minus sign and single letters are set in italic, as in TeX. A mathematical expression that the typesetter does not support does not prevent the figure from being built: its raw source is drawn in the text font, and the problem is reported as a warning, which the viewer shows in its [problems indicator](viewer.md#problems). Rust raw strings (`r"…"`) avoid doubling every backslash in LaTeX source.

## Axes properties

The remaining axes properties are also set on the axes handle.

| Method | Effect |
| --- | --- |
| `xlim(min, max)`, `ylim(min, max)`, `zlim(min, max)` | Fixes the range of an axis, and of every axes linked with it along that dimension. |
| `xscale(scale)`, `yscale(scale)`, `zscale(scale)` | Sets an axis to `Scale::Linear` or `Scale::Log`. |
| `xreverse(on)`, `yreverse(on)`, `zreverse(on)` | Reverses an axis, so that it runs from its upper limit to its lower limit; see [axis direction and ticks](#axis-direction-and-ticks). |
| `xticks(values)`, `yticks(values)`, `zticks(values)` | Places the ticks of an axis at the given values. |
| `xtick_step(step)`, `xtick_step_from(step, origin)` and the same for y and z | Places a tick at every multiple of the step, or at every `origin + k × step`, within the limits. |
| `xtick_labels(texts)`, `ytick_labels(texts)`, `ztick_labels(texts)` | Labels the ticks given by `xticks`, `yticks` or `zticks` with the texts, one per value. |
| `xtick_labels_hidden()`, `ytick_labels_hidden()`, `ztick_labels_hidden()` | Draws the ticks of an axis without labels and reserves no space for them. |
| `xtick_label_angle(degrees)`, `ytick_label_angle(degrees)`, `ztick_label_angle(degrees)` | Turns every tick label of an axis by the angle, anticlockwise from horizontal; see [turned tick labels and hidden axes](#turned-tick-labels-and-hidden-axes). |
| `xaxis_visible(on)`, `yaxis_visible(on)`, `zaxis_visible(on)` | Shows or hides an axis with its spine, ticks, tick labels and label, and gives its space to the plot area. |
| `aspect(aspect)` | Fixes the ratios of the drawn length of a unit of y, and of z, to that of a unit of x: `Aspect::Equal`, `Aspect::ratio(y)` or `Aspect::Ratio { y, z }`; see [equal aspect](#equal-aspect). |
| `aspect_adjust(adjust)` | Keeps a fixed aspect by shaping the plot area or the box to the limits (`AspectAdjust::Box`, the default) or by widening the limits (`AspectAdjust::Limits`). |
| `title_align(align)` | Aligns the title with the plot area: `TitleAlign::Center` (the default), `Left` or `Right`. |
| `grid(on)` | Shows or hides grid lines along every axis. |
| `box_on(on)` | Draws the full outline of the plot box, or only the edges that carry tick labels. |
| `color_scale()` | Returns the default [colour scale](colour.md) of the axes, which its colour-mapped plots share, so that its colormap (MATLAB's `colormap`) and its limits (MATLAB's `clim`) can be set. |
| `view(azimuth_deg, elevation_deg)` | Sets the camera of a three-dimensional axes. |

Limits that are not set are chosen automatically from the data and rounded outwards to tick values. The x and y limits of contour, surface and image plots are the exact extent of their grid or of their pixel edges instead, so that the field fills the axes, as in MATLAB; other plots in the same axes that reach beyond the grid still extend the limits to the next tick value.

## Axis direction and ticks

An axis normally runs from its lower limit at the left or the bottom to its upper limit at the right or the top. `yreverse(true)` turns the y axis round, which is how a pressure distribution is drawn, with suction (negative $C_p$) upwards. The limits are still given lower bound first, and a datatip reports the values as they are stored:

```rust
let mut ax = fig.axes(0, 0);
ax.plot(&x, &cp).label("upper surface");
ax.ylabel("$C_p$").ylim(-2.0, 1.0).yreverse(true);
```

Reversing an axis changes only the direction in which it is drawn. Its automatic limits, its ticks and its scale are unchanged, and panning and zooming in the viewer move the data with the pointer as on any other axis. Linked axes share limits only, so a reversed axis may be linked with an unreversed one, and each keeps its own direction.

Ticks are automatic unless they are set. `xtick_step(0.2)` places a tick at every multiple of 0.2 within the limits, which gives a family of panels one shared regular grid, and `xtick_step_from(0.1, 0.05)` places them at 0.05, 0.15, 0.25 and so on. `xticks(values)` places them at explicit values. Ticks never change the limits, and a tick outside the limits is not drawn. Automatic ticks on a short axis with narrow manual limits always include at least three labelled ticks on a y axis, and at least two on an x axis, wherever the limits hold that many at a round step.

`xtick_labels(texts)` names explicit ticks one by one, which is how a categorical axis is made: there is no categorical axis type, and the categories are the ticks `0, 1, …, N − 1` named by their labels. A strip chart lists its rows from the top, so it also reverses its y axis:

```rust
let rows: Vec<f64> = (0..cases.len()).map(|k| k as f64).collect();
let mut ax = fig.axes(0, 0);
ax.scatter(&positions, &row_of_each_position);
ax.ylim(-0.5, cases.len() as f64 - 0.5)
    .yreverse(true)       // the first case at the top
    .yticks(&rows)
    .ytick_labels(cases); // one name per row
```

`ytick_labels_hidden()` keeps the ticks of an axis but draws no labels and reserves no space for them, so that a panel beside one that names the rows, or the upper rows of a stack that shares one x axis, gives the space to its plot. Plot areas of equal [weight](legends.md#sizing-rows-and-columns-with-weights) are of equal size whatever their labels, so the two panels of a strip chart have plot areas of the same width although only one of them carries the names. Labels without explicit ticks, or of a different number, are reported by [`validate`](#validating-a-figure) as the error `InvalidTickLabels`, and a step that is not a finite positive number as the error `InvalidTicks`. The [categorical strip chart](../gallery/strip_chart.md) and the [pressure distribution](../gallery/pressure_distribution.md) of the gallery show both uses in full.

### Turned tick labels and hidden axes

Long category names collide when they are written level under a crowded axis. `xtick_label_angle(45.0)` turns every tick label of the x axis by 45 degrees anticlockwise from horizontal, so that the names rise to the right, and `xtick_label_angle(90.0)` stands them vertically. Each label is turned about its end nearest its tick, so a label still points at the tick it names: with a positive angle the end of each name lies just below its tick, and with a negative angle its start does. A label never comes closer to the plot than a level label does, and the layout reserves the space the turned labels take, so that they overlap neither the axis label nor a neighbouring axes, and none is clipped by the page. The angle applies to automatic, explicit and named labels alike; the common exponent `×10^k` of the axis stays level. Angles that differ by whole turns are the same angle, and 180 draws level labels upside down, centred on their ticks.

`xaxis_visible(false)` hides the x axis entirely: its spine (the bottom edge of the plot area, and the top edge when the box is on), its ticks, its tick labels and its label. No space is reserved for them, so the plot area grows into the space they would have taken. The grid lines of a hidden axis still follow `grid`, and its limits and scale still place the data. Hiding both axes removes the box, which consists of their spines. The plot areas of a row of axes stay aligned, so an axes beside one with turned labels keeps the same height whether or not its own x axis is shown.

```rust
let names = ["xpanel", "xbl", "xsolve", "xoper"];
let ticks = [0.0, 1.0, 2.0, 3.0];
let mut fig = Figure::new().tiles(2, 1);
let mut top = fig.axes(0, 0);
top.plot(ticks, [0.42, 0.18, 0.77, 0.35]);
top.xaxis_visible(false); // the panel below names the cases
let mut bottom = fig.axes(1, 0);
bottom.plot(ticks, [12.0, 31.0, 7.0, 19.0]);
bottom.xticks(ticks).xtick_labels(names).xtick_label_angle(45.0);
```

A three-dimensional axes draws its tick labels along the projected edges of its box, where a turn in the page has no agreed meaning, so it ignores the angle and [`validate`](#validating-a-figure) warns of it with `TickLabelAngleIgnored`; an angle that is not finite is the error `InvalidTickLabelAngle`. A hidden axis of a three-dimensional axes draws no ticks, tick labels or label, and the box is drawn as before. The [equivalent functions](../reference/equivalent-functions.md#axes) map both settings to MATLAB, matplotlib and Plotly, and the [figure schema](../reference/figure-schema.md#axis) gives the exact rules.

## Legends

A legend lists every plot of an axes that has a label, in the order the plots were added. The label of a plot is the text drawn beside its sample in the legend, and is set with `label`. It is shown inside the plot area with `legend` and hidden with `legend_off`.

```rust
ax.plot(&t, &a).label("$S_1(x)$");
ax.plot(&t, &b).label("$S_2(x)$");
ax.legend(LegendLocation::SouthEast);
```

The [legends guide](legends.md) describes the locations of a legend and how to arrange its entries in columns, and explains legend panels, which occupy tiles of the layout so that several axes can share one legend.

## Linking axes

Linking the limits of several axes along a dimension keeps those limits equal: setting the limits of one axes, or panning or zooming it in the viewer, changes all of them. Automatic limits of linked axes are computed over the data of every axes in the group, so linked axes agree even before any limits are set.

```rust
let mut fig = Figure::new().tiles(1, 3);
let left = fig.axes(0, 0).id();
fig.axes(0, 0).plot(&t, &decay);
fig.axes(0, 1).semilogy(&t, &growth);
let right = fig.axes(0, 2).id();
fig.axes(0, 2).plot(&t, &growth);

// The outer panels share their time axis; the middle panel pans independently.
fig.link(Dim::X, &[left, right])?;
```

`link` returns an error if an identifier is not an axes of the figure. Linking axes that already belong to a group for the same dimension merges the groups, so `link(Dim::X, &[a, b])` followed by `link(Dim::X, &[b, c])` links `a`, `b` and `c` together. When a group is formed, every member takes the limits of the first axes given.

Linking also returns an error when a member cannot show the limits it would take, as a logarithmic axis cannot show a range that reaches zero. Axes that cannot share limits cannot be linked, so the whole call is refused and the figure keeps the links and the limits it had, rather than leaving a group that is linked in name only. Setting limits on a linked axes is refused for the same reason when any member of its group cannot show them.

Two shortcuts link every axes of the figure, as MATLAB's `linkaxes(ax, 'x')` and `linkaxes(ax, 'y')` do:

```rust
fig.link_all_x()?;
fig.link_all_y()?;
```

These link only the axes that exist when they are called, so they are called after every axes has been created. `fig.link_all(Dim::Z)` links the z limits of every axes. The [linked subplots](../gallery/subplots_linked.md) gallery entry shows rows, columns and arbitrary pairs linked in one figure.

## Three-dimensional axes

A three-dimensional axes is drawn through an orthographic camera described by its azimuth (the rotation about the vertical axis) and its elevation (the angle of the view above the x–y plane). The default view is MATLAB's, with an azimuth of −37.5° and an elevation of 30°. An axes becomes three-dimensional when it is created with `axes3`, when `view` is called on it, when any of `plot3`, `scatter3`, `contour3`, `quiver3`, `surf` or `mesh` adds a plot to it, or when an [image](#images) is placed on one of its walls with `plane`. `surface` and the image functions leave an axes as it is, so [`surface` in a two-dimensional axes](#surfaces-in-two-dimensional-axes) draws a pseudocolour plot.

```rust
let mut ax = fig.axes3(0, 0);
ax.surf(&x, &y, &z);
ax.xlabel("$x$").ylabel("$y$").zlabel("$z$").view(-37.5, 30.0).grid(true);
```

A series or quiver without z data in a three-dimensional axes lies in the plane z = 0. Faces, line segments, markers and images are drawn from back to front for the current view, except that an image on a face of the axes box is painted behind or in front of all of them, and each surface face has a single flat colour. Each segment of a line is placed by its own depth, so that a line passing through a surface is drawn behind it and in front of it where it should be. The reasons for this approach are recorded in [ADR 0010](../adrs/0010-pdf-export-with-a-raster-fallback.md), those for images in [ADR 0011](../adrs/0011-image-artists.md), and those for lines drawn by segment in [ADR 0025](../adrs/0025-series-replaces-line-and-scatter.md).

## Parameters

Parameters are named values that describe a figure, such as the conditions of the experiment or simulation that produced its data. They do not change the drawing; they are saved with the figure so that collections of figures can be sorted, filtered and searched. A parameter is set with the `parameter` builder method, and its value may be a `bool`, an integer, an `f64` or a string:

```rust
let fig = Figure::new()
    .title("Wake behind a cylinder")
    .parameter("reynolds_number", 3900.0)
    .parameter("mesh_cells", 2_400_000)
    .parameter("solver", "LES")
    .parameter("converged", true);

assert_eq!(fig.parameters()["mesh_cells"], Parameter::Integer(2_400_000));
```

Setting a parameter again with the same name replaces its value. `fig.parameters()` returns every parameter in ascending order of name. Each value keeps its kind when the figure is saved in either format, so the number `3900.0` never reloads as an integer; how parameters are stored is described in the [figure schema reference](../reference/figure-schema.md#parameters).

Parameters are what the viewer's [figure browser](viewer.md#the-figure-browser) filters, orders and groups a collection of figures by; when to reach for a parameter rather than a tag is covered in [describing figures](describing-figures.md).

## Tags

A tag is a free word describing a figure, such as `surface` or `piv`. Tags are what parameters cannot be: a figure carries any number of them, and none of them is a name with a value under it. The Reynolds number is a parameter; `wake` is a tag. Like parameters, tags are never drawn.

```rust
let fig = Figure::new()
    .title("Wake behind a cylinder")
    .tag("wake")
    .tag("piv")
    .parameter("reynolds_number", 3900.0);

assert_eq!(fig.tags(), ["wake", "piv"]);
```

Tags are kept in the order they were given. A tag must not be empty, and adding the same tag twice is an error, which [`validate`](#validating-a-figure) reports as `InvalidTag`; tags are compared exactly, so `Surface` and `surface` are distinct tags.

Tags, with parameters, are what the [figure browser](viewer.md#the-figure-browser) filters and groups by, and between them they are the whole of what it has to work with (see [describing figures](describing-figures.md)): the viewer reads no properties off a figure of its own accord, so a figure is found by the words its author gave it and by its title. Tag a figure with whatever someone might later look for — what it shows, what it demonstrates, which study it belongs to — including things the figure's own structure would suggest, such as `surface`, if those are words anyone would search by. Which of them belong in a tag and which in a parameter is covered in [describing figures](describing-figures.md).

A tag need not be shared with another figure to be worth adding. The browser lists every tag it finds, with the number of figures behind each, so a tag carried by a single figure is how that figure is found rather than a tag wasted: the list of tags is itself what tells a reader what the collection can be narrowed by.

Tags were called labels before the word label was kept for text that is drawn beside what it identifies, such as an axis label or the label of a plot; the vocabulary is recorded in [ADR 0023](../adrs/0023-titles-labels-names-and-tags.md).

## Equal aspect

Where the data is geometry, such as an aerofoil section, a mesh or a map, one data unit must be drawn at the same length along every axis, or the figure misrepresents the shape. `aspect(Aspect::Equal)` asks for that, as MATLAB's `axis equal` and matplotlib's `set_aspect("equal")` do, and `aspect(Aspect::ratio(r))` draws a unit of y `r` times as long as a unit of x, as MATLAB's `daspect` does. On a logarithmic axis the unit is one decade.

```rust
let mut ax = fig.axes(0, 0);
ax.plot(&x, &y);
ax.aspect(Aspect::Equal)
    .aspect_adjust(AspectAdjust::Limits) // or AspectAdjust::Box, the default
    .xlim(-0.2, 1.2)
    .ylim(-0.4, 0.4)
    .xtick_step(0.2)
    .ytick_step(0.2);
```

The aspect is kept once the margins of the layout are known, so it survives a change of fonts, of tick labels or of the figure size, and it is kept by export and while the figure is panned and zoomed in the viewer. `AspectAdjust::Box` keeps the limits exactly as they are, manual or automatic, and shrinks the plot area along one dimension, centred in its tile. `AspectAdjust::Limits` keeps the plot area and widens the limits of the axis that is too short for it, symmetrically about their centre; manual limits are then the least range shown, and only the axis that must grow is widened. Axes [linked](#linking-axes) along a dimension widen together, and every panel of a linked family keeps its aspect. Panels linked in both x and y whose plot areas differ in shape cannot all keep the aspect by widening shared limits, so they keep it by their plot areas instead, and a warning says so; `AspectAdjust::Box` is the adjustment to choose for such a family.

A three-dimensional axes keeps a fixed aspect in the same two ways. With `AspectAdjust::Box` the limits are kept and the sides of the box take the lengths of the ranges, so that an isosurface or a sphere is drawn with its true shape; with `AspectAdjust::Limits` the box stays a cube and the shorter ranges widen about their centres. `Aspect::Ratio { y: 1.0, z: 4.0 }` draws a unit of z four times as long as a unit of x, the vertical exaggeration of a terrain map. The box keeps its proportions however the view is rotated, zoomed or panned.

```rust
let mut ax = fig.axes(0, 0);
ax.surf(&x, &y, &height);
ax.aspect(Aspect::Ratio { y: 1.0, z: 4.0 }); // the height drawn four times as long
```

[`validate`](#validating-a-figure) reports a ratio of y or of z that is not a finite positive number as the error `InvalidAspect`, on two-dimensional axes too, which ignore the ratio of z. The [NACA 4-digit family sheet](../gallery/aerofoil_family.md), the [equal aspect](../gallery/equal_aspect.md) figure and [equal aspect in three dimensions](../gallery/equal_aspect_3d.md) of the gallery show both adjustments, and the decision is recorded in [ADR 0019](../adrs/0019-axis-direction-ticks-blended-coordinates-and-aspect.md#the-aspect-of-an-axes-is-kept-everywhere).

## Validating a figure

The builder never panics because of inconsistent input, such as arrays of different lengths or an axes placed outside the tile layout. Such problems are found by `validate`, which returns a report of errors and warnings:

```rust
let report = fig.validate();
if !report.is_valid() {
    for issue in &report.errors {
        eprintln!("{}", issue.message);
    }
}
```

Exporting and showing a figure validate it first, and return `Error::Invalid` with the report when it has errors. Warnings do not prevent a figure from being exported or shown: they describe an artist that is drawn with less than its data holds, or not at all. Each warning names the artist it concerns and has a kind that says why. `NonPositiveOnLogAxis` reports data that a logarithmic axis cannot show, and `NothingToDraw` an artist whose data gives it nothing to draw: a line, scatter or quiver of no points, an image with no rows or no columns, or a contour or surface whose field has no rows or no columns or a single row or a single column, between whose nodes there is no cell. Empty and singleton data are valid in every artist, so a figure built before its data arrives is valid at every step, and the warning is how a program learns that an artist is missing from the picture:

```rust
use ironlab::IssueKind;

let report = fig.validate();
for issue in &report.warnings {
    if issue.kind == IssueKind::NothingToDraw {
        eprintln!("{:?} is not drawn: {}", issue.node, issue.message);
    }
}
```

The reasons for reporting an artist with nothing to draw as a warning rather than an error are recorded in [ADR 0012](../adrs/0012-empty-and-singleton-data.md). The same warning is reported for a legend panel that has nothing to list. The warning `UnusedData` reports a data array that no plot refers to, as described in [sharing data between plots](#sharing-data-between-plots). A further warning, `OverlappingCells`, reports two axes or panels whose cells share a tile of the layout, so that one is drawn over the other. The errors and warnings that concern legends and the sizes of the rows and columns of the tile layout are described in the [legends guide](legends.md).

## Saving and loading

A figure is saved in the format named by the extension of the file. A `.fig` file holds the default Protocol Buffers encoding, which is compact and fast to read even for large data. A `.json` file, conventionally named `.fig.json`, holds the JSON encoding, which can be read and edited as text and is useful for debugging and for other tools. Both encodings are described in the [figure schema reference](../reference/figure-schema.md#encodings).

```rust
fig.save("pressure.fig")?;
let fig = Figure::load("pressure.fig")?;

fig.save("pressure.fig.json")?;
let fig = Figure::load("pressure.fig.json")?;
```

Extensions are matched without regard to case. Any other extension, or none, makes `save` and `load` fail with `Error::UnsupportedFormat`, without writing or reading a file. `save_json` and `load_json` write and read JSON whatever the extension, and `to_protobuf` and `from_protobuf` convert a figure to and from the bytes of a `.fig` file, for example to send it to another process.

A figure is saved even if it has validation errors, so that it can be inspected or repaired later. Loading fails with `Error::Ir` if the file declares an incompatible schema version or does not describe a figure in the format of its extension. The viewer writes the same two formats, in the same way, with its [Save figure…](viewer.md#saving-the-figure) button, and the `<ironlab-figure>` element reads both to show a figure live in a web page, as described in [embedding figures in a web page](embedding.md). A loaded figure can be extended with the same builder methods as a new one. Properties that the builder does not cover are reached through `fig.ir_mut()`, which returns the underlying figure model.

## Exporting PDF

`export_pdf` writes the figure as a single-page PDF:

```rust
fig.export_pdf("pressure.pdf")?;
```

The page of the PDF is exactly the size of the figure, with no margins, and all text is embedded as real, selectable text in subsets of the bundled fonts. A plot hidden through its visibility flag (for example from the viewer's legend) is left out of the PDF.

`export_pdf` returns an `ExportReport` of what was left off the page and of what reached it other than as vectors: the validation warnings of the figure (see [validating a figure](#validating-a-figure)), the warnings the scene compiler raised while drawing it, such as a piece of LaTeX the typesetter does not support, and the warnings of the exporter itself. Each names the node it concerns. All three lists are empty for a figure from which nothing was left out and whose every artist is on the page as vectors, so a program can test the report to decide whether the figure is finished, and nothing is printed or logged:

```rust
let report = fig.export_pdf("pressure.pdf")?;
for issue in &report.validation {
    eprintln!("{:?}: {}", issue.node, issue.message);
}
for warning in &report.scene {
    eprintln!("{:?}: {}", warning.node, warning.message);
}
for warning in &report.export {
    eprintln!("{:?}: {}", warning.node, warning.message);
}
```

The first two lists overlap where the compiler leaves out an artist that validation warned of, and differ where the compiler finds a reason that validation cannot see. A program that wants one reason per artist reads `validation`; one that wants every reason reads both. The third list, `export`, says what the exporter drew as an image and why (`RasterisedForSize`, `RasterisedForDepth` or `RasterisedByRequest`, see the two sections below) and which three-dimensional axes it could not verify (`Unverified`, with its cause). Every raster on the page is explained there, so a page that a program expects to be wholly vector can be checked by testing that the list is empty. A caller that does not need the report can drop it, as `fig.export_pdf("pressure.pdf")?;` does.

### Dense surfaces

A surface is drawn as one filled path per face, so a surface of a hundred thousand faces is a hundred thousand paths: the file grows in proportion, and readers become slow to open it, for detail no printer can resolve. IronLAB therefore draws a surface of **ten thousand faces or more** as an image instead. The image is rendered by the same pipeline that draws the interactive viewer, at **600 dots per inch** by default, so it shows exactly what is on screen at a resolution suitable for print. Everything else on the page — the axes, ticks, tick labels, axis labels, titles, legends and every other artist — stays vector, and all text stays selectable.

Ten thousand faces is where the two representations cost about the same. On a figure of the default size at the default resolution, a surface of 4900 faces is 59 KiB as vectors and 98 KiB as an image, one of 9801 faces is 112 against 105 KiB, and one of 39 601 faces is 618 against 115 KiB.

`export_pdf_with` overrides both the decision and the resolution:

```rust
use ironlab::{RasterOptions, RasterPolicy};

// Never rasterise, whatever the surface costs: the figure is going to be edited in Inkscape.
fig.export_pdf_with("pressure.pdf", RasterOptions {
    policy: RasterPolicy::Never,
    ..RasterOptions::default()
})?;

// Always rasterise, at 1200 dots per inch, because the figure will be printed at full page size.
fig.export_pdf_with("pressure.pdf", RasterOptions {
    policy: RasterPolicy::Always,
    dpi: 1200.0,
    ..RasterOptions::default()
})?;

// Rasterise from five thousand faces upwards, at the default resolution.
fig.export_pdf_with("pressure.pdf", RasterOptions {
    policy: RasterPolicy::Auto { cells: 5_000 },
    ..RasterOptions::default()
})?;
```

Rendering the image needs a graphics adapter. A figure with nothing dense in it is exported without one, as it always was, so exporting on a machine without a GPU only fails for a figure that must be rasterised — and then it fails with a message naming `RasterPolicy::Never` rather than quietly writing something else. A dense artist drawn as an image is reported in the `export` list of the report as `RasterisedForSize`, with its cell count.

### Three-dimensional axes in PDF

The viewer draws the artists of a three-dimensional axes with a depth buffer, so that surfaces, lines, markers and images which cross one another are shown correctly wherever they cross. A PDF has no depth buffer: it can only paint one thing over another in a fixed order. `export_pdf` therefore proves, for each three-dimensional axes, whether painting its artists back to front shows the same picture as the depth buffer, by rendering the axes both ways through the viewer's own renderer at the export resolution. Where the two agree, the axes is written as vector paths, as a single `surf` or `contour3` is. Where they differ, as for two surfaces that intersect, arrows standing on a surface, a line lying along one, a sharply folded wireframe whose edges show through its folds by the width of the depth bias, or a translucent surface with something behind it that comes after it in the painter's order, the depth-tested render is embedded as an image, exactly as a dense surface is, and the report says so as `RasterisedForDepth`. The depth buffer draws the opaque artists of the axes first and the translucent ones after them, without letting a translucent face or image hide anything, so the depth-tested render shows through a translucent surface whatever the order. Two renders agree when no patch of six points differs by more than four levels of 255 on average, which admits the slivers that anti-aliasing leaves along the shared edges of faces and refuses any misdrawn face, marker or stretch of line.

`export_pdf_with` overrides the decision through `RasterOptions::depth`:

```rust
use ironlab::{DepthPolicy, RasterOptions};

// Write the axes as vectors back to front whatever they show, for a figure that will be edited by hand; the report
// carries an `Unverified` warning for each three-dimensional axes.
fig.export_pdf_with("field.pdf", RasterOptions {
    depth: DepthPolicy::Vector,
    ..RasterOptions::default()
})?;

// Embed the depth-tested render of every three-dimensional axes without checking, which needs a graphics adapter
// and is reported as `RasterisedByRequest`.
fig.export_pdf_with("field.pdf", RasterOptions {
    depth: DepthPolicy::Raster,
    ..RasterOptions::default()
})?;
```

Verifying an axes uses a graphics adapter. Without one the default policy still writes the page, back to front, and reports each three-dimensional axes as `Unverified` with the cause `NoAdapter`; see [running without a graphics device](#running-without-a-graphics-device).

### Including a figure in a LaTeX document

Set the size of the figure to the size it should have on the printed page, for example the width of the text column, and include the PDF without scaling it:

```latex
\usepackage{graphicx}
...
\begin{figure}
  \centering
  \includegraphics{pressure.pdf}
  \caption{The pressure field.}
\end{figure}
```

Including the figure unscaled keeps text at the font size set in IronLAB, so a 9 pt label is 9 pt on the page. Passing `width=` or `scale=` to `\includegraphics` rescales the text and line widths with the rest of the figure. The PDF works with pdfLaTeX, XeLaTeX and LuaLaTeX, none of which need the `--shell-escape` option to include it. A publication that is also read on the web can carry the figure itself beside the PDF, so that a reader explores it and exports the same PDF from the page; see [embedding figures in a web page](embedding.md).

## Exporting PNG

`export_png` writes the figure as a PNG image, for a slide, a web page, the [fallback content](embedding.md#fallback-content) of an embedded figure or a quick look at what a program produced:

```rust
fig.export_png("pressure.png")?;
```

The image is the figure as the viewer and the [gallery](../gallery/index.md) draw it, rendered by the same engine at the size of the figure and at 150 dots per inch (`ironlab::DEFAULT_PNG_DPI`), with an alpha channel, so a figure with a transparent background stays transparent. `export_png_with` chooses the resolution:

```rust
// A figure for a printed poster, at 300 dots per inch.
fig.export_png_with("pressure.png", 300.0)?;
```

The image is `round(width_pt · dpi / 72)` by `round(height_pt · dpi / 72)` pixels, where `width_pt` and `height_pt` are the size of the figure in points, so a figure of 160 by 100 mm is 945 by 591 pixels at the default resolution and the PNG agrees with the PDF on the physical size of the figure to within half a pixel.

Because the whole image is drawn by the renderer, every PNG export needs a graphics adapter; a machine without a graphics device provides one through a software adapter, see [running without a graphics device](#running-without-a-graphics-device), and without any adapter the export fails with a message naming the remedy. Both functions return the same `ExportReport` as `export_pdf`, holding the validation warnings of the figure and the warnings the scene compiler raised while drawing it; its `export` list is always empty, because nothing in a PNG is rasterised selectively. The PDF remains the format for print, where text stays selectable and lines stay vector; see [exporting PDF](#exporting-pdf).

## Running without a graphics device

Nothing in IronLAB needs a window: exporting a figure, rendering the documentation gallery and running the tests all draw through the same headless renderer, so they run on a server, in a container and in continuous integration. What they need, for some figures, is a graphics adapter for wgpu to draw with.

- Verifying a three-dimensional axes for [export](#three-dimensional-axes-in-pdf), and drawing one as an image, needs an adapter.
- Drawing a [dense surface](#dense-surfaces) as an image needs an adapter.
- [Exporting PNG](#exporting-png) needs an adapter, because the whole image is drawn by the renderer.
- Everything else, including every two-dimensional figure and a three-dimensional figure exported with `DepthPolicy::Vector`, needs none.

A machine without a graphics device uses a software adapter, which wgpu finds like any other:

| Platform | Software adapter |
| --- | --- |
| Debian and Ubuntu, including containers and GitHub's Linux runners | lavapipe, Mesa's Vulkan rasteriser: `apt-get install mesa-vulkan-drivers` |
| Fedora | `dnf install mesa-vulkan-drivers` |
| Arch | `pacman -S vulkan-swrast` |
| Windows | WARP, the Direct3D 12 software adapter, which is part of the operating system |
| macOS | Metal, which is always present |
| Anywhere Mesa cannot be installed | [SwiftShader](https://github.com/google/swiftshader), a portable Vulkan driver, pointed to by the `VK_ICD_FILENAMES` environment variable |

The `WGPU_BACKEND` environment variable selects a backend (`vulkan`, `metal`, `dx12` or `gl`) and admits no fallback when it is set, so leave it unset unless one backend must be forced.

With no adapter at all, an export that only needs to verify a three-dimensional axes still succeeds: the axes is written back to front and the report carries an `Unverified` warning whose message names the missing adapter and the remedy. An export that must draw an image, because the options force one or a dense surface needs one, fails with the same message rather than writing something else.

In the browser the graphics device is the browser's own, through WebGPU or, failing that, WebGL2, and a figure [embedded in a web page](embedding.md) needs one of the two: without either, the page keeps the still image the author placed in the element instead of drawing the live figure. The live figures of the [gallery](../gallery/index.md) need one of them for the same reason.

## Opening the viewer

There are three ways to open figures in the interactive viewer, whose controls are described in [using the viewer](viewer.md). When more than one figure is open, the viewer shows one at a time and its [figure browser](viewer.md#the-figure-browser) narrows the collection down to the one to look at.

- **From a program.** `fig.show()` opens the figure in a window and blocks until the window is closed. It consumes the figure; save or export it first if it is needed afterwards. On macOS the window must be opened from the main thread, so `show` is called from `main` or code that `main` calls directly.

    ```rust
    fig.show()?;
    ```

- **From saved files.** The viewer binary opens one or more `.fig` or `.json` files, choosing the format of each file from its extension:

    ```sh
    cargo run -p ironlab-viewer -- pressure.fig velocity.fig.json
    ```

- **From the gallery.** The gallery binary opens every example figure, or only the figures whose slugs are given:

    ```sh
    cargo run -p ironlab-gallery -- view
    cargo run -p ironlab-gallery -- view surf legend_toggle
    ```

The gallery binary also writes the PDF, `.fig` file, `.fig.json` file and PNG image of every example into a directory with `cargo run -p ironlab-gallery -- export <dir>`.

A saved figure can also be shown live in a web page, where a reader explores it with the viewer's own gestures without installing anything, as described in [embedding figures in a web page](embedding.md).
