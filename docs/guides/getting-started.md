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

Each of these returns an `AxesMut` handle. The handle borrows the figure mutably, so it is held only while one axes is being built. An axes is referred to later, for example when linking axes, by the identifier returned from `ax.id()`.

## Plotting functions

Plotting functions are methods of `AxesMut`. They follow MATLAB's names and argument order, copy their data into the figure, and return a handle whose setters change the new plot's properties and can be chained. Vectors are passed as anything that can be viewed as a slice of `f64`, such as `Vec<f64>`, `&[f64]` or an array.

The helper functions `linspace(start, end, n)`, `logspace(start_exp, end_exp, n)` and `meshgrid(&x, &y)` create coordinates as in MATLAB. Gridded data is passed as a `Matrix`, which has one row per y coordinate and one column per x coordinate, so that the value in row `j` and column `i` belongs to the point `(x[i], y[j])`.

```rust
let x = linspace(-1.5, 1.5, 61);
let y = linspace(-1.5, 1.5, 61);
let z = Matrix::from_fn(y.len(), x.len(), |row, col| x[col].powi(2) - y[row].powi(2));
```

### Lines

`plot(x, y)` draws a line through the points `(x[i], y[i])`. The line is solid, 0.75 pt wide and has no markers, and its colour is the next colour of the axes colour order. `plot3(x, y, z)` draws a line in three dimensions and converts the axes to three dimensions.

```rust
let mut ax = fig.axes(0, 0);
ax.plot(&x, &y)
    .display_name("$x^2$")
    .color(Color::rgb(0.0, 0.45, 0.7))
    .line_width(1.5)
    .dash(Dash::DashDot)       // Solid, Dashed, Dotted, DashDot, or None to draw only markers
    .marker(Marker::Square)    // Circle, Square, Diamond, TriangleUp, TriangleDown, Plus, Cross, Point
    .marker_size(5.0)
    .marker_face(Color::WHITE)
    .marker_edge(Color::BLACK);
```

Non-finite values (such as `f64::NAN`) break a line into separate pieces.

### Logarithmic axes

`loglog(x, y)`, `semilogx(x, y)` and `semilogy(x, y)` draw a line as `plot` does and set the scales of the x and y axes to logarithmic, as MATLAB's functions of the same names do. The scale of any axis can also be set directly with `xscale`, `yscale` and `zscale`. Points with a non-positive coordinate on a logarithmic axis are not drawn, and validation warns about them.

```rust
let f = logspace(0.0, 4.0, 200);
let gain: Vec<f64> = f.iter().map(|f| 1.0 / (1.0 + (f / 100.0).powi(2)).sqrt()).collect();
fig.axes(0, 0).loglog(&f, &gain);
```

### Scatter plots

`scatter(x, y)` draws a marker at each point, and `scatter3(x, y, z)` does so in three dimensions. Markers are unfilled 4 pt circles in the next colour of the colour order unless changed.

```rust
fig.axes(0, 0)
    .scatter(&x, &y)
    .sizes(&sizes)           // one size in points per marker; or .size(6.0) for all markers
    .colors(&values)         // one value per marker, mapped through the colormap; or .color(Color::BLACK)
    .marker(Marker::Diamond)
    .filled();
```

### Contours

`contour(x, y, z)` draws isolines of the field `z`, `contourf(x, y, z)` fills the bands between levels, and `contour3(x, y, z)` draws each isoline at the height of its level in a three-dimensional axes. Ten levels are chosen automatically, and each isoline is coloured from the colormap by its level.

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

`surf(x, y, z)` draws a surface whose faces are coloured from the colormap by height and outlined by thin black edges. `mesh(x, y, z)` draws a wireframe whose faces are filled with the background colour, so that they hide the edges behind them, and whose edges are coloured from the colormap. Both convert the axes to three dimensions.

```rust
fig.axes3(0, 0)
    .surf(&x, &y, &z)
    .edge_color(None)                   // hide the edges
    .face_color(ColorSpec::Colormapped) // the default for surf
    .edge_width(0.25)
    .color_data(&c);                    // colour by another matrix instead of by height
```

Wherever a colour is set, a `Color` gives a fixed colour, `None` draws nothing, and a `ColorSpec` chooses the automatic colour (`ColorSpec::Auto`) or a colour taken from the colormap (`ColorSpec::Colormapped`).

A surface of ten thousand faces or more is exported as an image rather than as one path per face; see [dense surfaces](#dense-surfaces).

### Images

An image is a raster of pixels, each with one flat colour, placed by the coordinates of its pixel centres. Three functions draw the three kinds of image, which differ in where the colour of a pixel comes from.

- `image(&pixels)` draws a true-colour image (MATLAB's `image` with a true-colour array). Its argument is a `Pixels`: the red, green and blue components of every pixel, with an optional alpha component, one byte each. Pixels are built from the bytes of a decoded image with `Pixels::from_rgb8` or `Pixels::from_rgba8`, from a function of the row and column with `Pixels::rgb_from_fn` or `Pixels::rgba_from_fn`, or from one `Matrix` per component with `Pixels::from_planes`, to which `with_alpha` attaches a plane of opacities.
- `indexed_image(&indices)` draws a colour-indexed image, whose pixels name entries of the axes colormap directly (MATLAB's `image` with an indexed array). An index from 0 to 255 takes that entry of the 256-entry colormap, a floating-point index is truncated toward zero first, and the colour limits play no part.
- `mapped_image(&values)` draws a colour-mapped image, whose pixels are data values scaled through the colour limits of the axes into its colormap, exactly as the colour data of a surface is (MATLAB's `imagesc`).

The indices and values of the two mapped kinds come from a `Matrix`, stored as floating-point values, or from a `ByteMatrix`, its 8-bit counterpart, which stores bytes exactly and at a quarter of the size.

```rust
let x = linspace(-2.0, 2.0, 81);
let y = linspace(-1.0, 1.0, 41);
let field = Matrix::from_fn(y.len(), x.len(), |row, col| (x[col] * y[row]).sin());
let classes = ByteMatrix::from_fn(y.len(), x.len(), |row, col| ((row + col) % 4 * 85) as u8);
let pixels = Pixels::rgb_from_fn(64, 64, |row, col| Color::rgb(row as f32 / 63.0, col as f32 / 63.0, 0.5));

let mut fig = Figure::new().tiles(1, 3);
let mut ax = fig.axes(0, 0);
ax.mapped_image(&field)
    .pixel_columns(-2.0, 2.0)  // the centres of the first and last columns
    .pixel_rows(-1.0, 1.0)     // the centres of the first and last rows
    .above(OutOfRange::Clamp)  // a value above the colour limits takes the last colour
    .non_finite(Color::BLACK)  // a NaN is drawn in black instead of being left transparent
    .display_name(r"$\sin xy$");
ax.colormap(Colormap::Magma).clim(-1.0, 1.0);
fig.axes(0, 1).indexed_image(&classes).pixel_columns(-2.0, 2.0).pixel_rows(-1.0, 1.0);
fig.axes(0, 2).image(&pixels).pixel_columns(0.0, 1.0).pixel_rows(1.0, 0.0); // rows count down from the top
```

An image is placed by the centres of its first and last pixels along each axis of its plane: `pixel_columns(first, last)` places its columns and `pixel_rows(first, last)` its rows. The pitch between centres follows from the number of pixels, and the image extends half a pitch beyond each centre, so the mapped image above, with 81 columns centred from −2 to 2, has a pitch of 0.05 and covers −2.025 to 2.025. Without a range the centres lie at 0, 1, …, n − 1, so an image of n columns covers −0.5 to n − 0.5. Row 0 of the array always lies at the first row centre and column 0 at the first column centre, so a range whose `last` is less than its `first` mirrors the image along that axis: the rows of a decoded photograph count down from its top, so it is placed the right way up by a row range that runs from the top of the image to its bottom, as the `image` call above does. How an image is stretched, shifted or flipped is therefore written in its placement, rather than depending on the direction of an axis. The x and y limits of an axes that holds only images are the exact edges of their pixels, as they are for the grid of a contour or surface.

A pixel of a colour-indexed or colour-mapped image that the colormap cannot colour falls in one of three categories, and each category has a policy of its own: `below` (an index less than 0, or a value below the lower colour limit), `above` (an index greater than 255, or a value above the upper colour limit) and `non_finite` (an index or value that is NaN or infinite). A policy is `OutOfRange::Transparent`, the default, which draws nothing for the pixel; `OutOfRange::Clamp`, which draws the nearest end colour of the colormap, its first entry below the range and its last above it, and nothing for a non-finite value, which has no nearest end; a `Color`, which draws the pixel in that fixed colour; or `OutOfRange::Strict`, which makes such a pixel a validation error. The policies are lenient by default so that a figure reaches the page whatever its data holds, and strict on any combination of the three categories when the data must be good: leaving `non_finite` transparent while making `below` and `above` strict tolerates missing values but refuses a value outside the range, and the reverse refuses missing values while tolerating those outside the range.

In a three-dimensional axes an image lies in one of the three coordinate planes, chosen with `plane`. `ImagePlane::Xy { z }` is the floor, `ImagePlane::Xz { y }` and `ImagePlane::Yz { x }` are the walls, and the offset is the coordinate of the plane along the third axis, or `None` for the low end of that axis, so `plane(ImagePlane::Xz { y: Some(0.0) })` stands the image on the wall at y = 0. The columns of the image run along the first axis of its plane and the rows along the second. Placing an image on a wall converts a two-dimensional axes to three dimensions, as `surf` does, whereas the xy plane leaves the axes as it is, because it is the only plane a two-dimensional axes can show. An image on a face of the axes box, as an image without an offset is, is painted behind everything else in the axes when its face is at the back of the current view and in front of everything else when it is at the front; an image at an interior offset is sorted among the faces, lines and markers by its depth.

The pixels of an image are flat, so an image cannot be placed on a logarithmic axis: an image whose plane has a logarithmic axis is not drawn, and validation warns about it.

An exported PDF embeds every image at its own resolution, without resampling, so a PDF grows with the number of pixels rather than with the size of the figure. A `.fig` file stores the bytes of a `Pixels` or a `ByteMatrix` as one byte each, whereas JSON writes one number per component as text, so a large image is saved as `.fig`.

In the viewer, resting the pointer over an image in a two-dimensional axes reads the pixel beneath it, as described in [datatips](viewer.md#datatips). The gallery entries [Image](../gallery/image.md), [Mapped image](../gallery/mapped_image.md) and [Indexed image](../gallery/indexed_image.md) show each kind, [Mapped image and surface](../gallery/mapped_image_and_surface.md) shows an image on the floor of a three-dimensional axes beneath a surface that covers it, and [Image planes](../gallery/image_planes.md) shows images in each of the three coordinate planes at explicit offsets, on a face of the box and inside it. The decisions behind the three kinds are recorded in [ADR 0011](../adrs/0011-image-artists.md).

## Titles, labels and LaTeX

Axes titles and labels are set on the axes handle, and each setter returns the handle so that they can be chained.

```rust
ax.title("Pressure field")
    .xlabel("$x$ (m)")
    .ylabel(r"$\sigma_{xx}$ (Pa)")
    .zlabel("$z$");
```

Every piece of text is stored as its source. By default, segments delimited by `$…$` are typeset as LaTeX mathematics and the rest is set as plain text, so `"$x$ (m)"` shows an italic x followed by an upright unit. A literal dollar sign inside such text is written `\$`. Text that should be shown exactly as written, dollar signs included, is created with `Text::plain`:

```rust
ax.ylabel(Text::plain("Cost in $"));
```

Mathematics is typeset in STIX Two Math and plain text in STIX Two Text. Within mathematics, a hyphen is set as a minus sign and single letters are set in italic, as in TeX. A mathematical expression that the typesetter does not support does not prevent the figure from being built: its raw source is drawn in the text font, and the problem is reported as a warning, which the viewer shows in its [problems indicator](viewer.md#problems). Rust raw strings (`r"…"`) avoid doubling every backslash in LaTeX source.

## Axes properties

The remaining axes properties are also set on the axes handle.

| Method | Effect |
| --- | --- |
| `xlim(min, max)`, `ylim(min, max)`, `zlim(min, max)` | Fixes the range of an axis, and of every axes linked with it along that dimension. |
| `xscale(scale)`, `yscale(scale)`, `zscale(scale)` | Sets an axis to `Scale::Linear` or `Scale::Log`. |
| `grid(on)` | Shows or hides grid lines along every axis. |
| `box_on(on)` | Draws the full outline of the plot box, or only the edges that carry tick labels. |
| `colormap(name)` | Sets the colormap: `Viridis` (the default), `Cividis`, `Magma`, `Inferno`, `Plasma`, `Coolwarm` or `Gray`. |
| `clim(min, max)` | Fixes the data values mapped to the ends of the colormap. Limits that are not set are the range of the colour data of the axes, to which a mapped image contributes its values; an indexed image, whose indices name colours directly, contributes nothing. |
| `view(azimuth_deg, elevation_deg)` | Sets the camera of a three-dimensional axes. |

Limits that are not set are chosen automatically from the data and rounded outwards to tick values. The x and y limits of contour, surface and image plots are the exact extent of their grid or of their pixel edges instead, so that the field fills the axes, as in MATLAB; other plots in the same axes that reach beyond the grid still extend the limits to the next tick value.

## Legends

A legend lists every plot of an axes that has a display name, in the order the plots were added. It is shown with `legend` and hidden with `legend_off`.

```rust
ax.plot(&t, &a).display_name("$S_1(x)$");
ax.plot(&t, &b).display_name("$S_2(x)$");
ax.legend(LegendLocation::SouthEast);
```

The location is one of `NorthEast`, `NorthWest`, `SouthEast`, `SouthWest`, `North`, `South`, `East`, `West`, or `Best`, which chooses the corner that overlaps the least data. In the viewer, clicking a legend entry hides or shows its plot, as described in [using the viewer](viewer.md#legend).

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

A three-dimensional axes is drawn through an orthographic camera described by its azimuth (the rotation about the vertical axis) and its elevation (the angle of the view above the x–y plane). The default view is MATLAB's, with an azimuth of −37.5° and an elevation of 30°. An axes becomes three-dimensional when it is created with `axes3`, when `view` is called on it, when any of `plot3`, `scatter3`, `contour3`, `quiver3`, `surf` or `mesh` adds a plot to it, or when an [image](#images) is placed on one of its walls with `plane`.

```rust
let mut ax = fig.axes3(0, 0);
ax.surf(&x, &y, &z);
ax.xlabel("$x$").ylabel("$y$").zlabel("$z$").view(-37.5, 30.0).grid(true);
```

A line, scatter or quiver without z data in a three-dimensional axes lies in the plane z = 0. Faces, lines, markers and images are drawn from back to front for the current view, except that an image on a face of the axes box is painted behind or in front of all of them, and each surface face has a single flat colour. The reasons for this approach are recorded in [ADR 0010](../adrs/0010-pdf-export-with-a-raster-fallback.md), and those for images in [ADR 0011](../adrs/0011-image-artists.md).

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

Exporting and showing a figure validate it first, and return `Error::Invalid` with the report when it has errors. Warnings, such as non-positive data on a logarithmic axis, do not prevent a figure from being exported or shown.

## Saving and loading

A figure is saved in the format named by the extension of the file. A `.fig` file holds the default Protocol Buffers encoding, which is compact and fast to read even for large data. A `.json` file, conventionally named `.fig.json`, holds the JSON encoding, which can be read and edited as text and is useful for debugging and for other tools. Both encodings are described in the [figure schema reference](../reference/figure-schema.md#encodings).

```rust
fig.save("pressure.fig")?;
let fig = Figure::load("pressure.fig")?;

fig.save("pressure.fig.json")?;
let fig = Figure::load("pressure.fig.json")?;
```

Extensions are matched without regard to case. Any other extension, or none, makes `save` and `load` fail with `Error::UnsupportedFormat`, without writing or reading a file. `save_json` and `load_json` write and read JSON whatever the extension, and `to_protobuf` and `from_protobuf` convert a figure to and from the bytes of a `.fig` file, for example to send it to another process.

A figure is saved even if it has validation errors, so that it can be inspected or repaired later. Loading fails with `Error::Ir` if the file declares an incompatible schema version or does not describe a figure in the format of its extension. The viewer writes the same two formats, in the same way, with its [Save figure…](viewer.md#saving-the-figure) button. A loaded figure can be extended with the same builder methods as a new one. Properties that the builder does not cover are reached through `fig.ir_mut()`, which returns the underlying figure model.

## Exporting PDF

`export_pdf` writes the figure as a single-page PDF:

```rust
fig.export_pdf("pressure.pdf")?;
```

The page of the PDF is exactly the size of the figure, with no margins, and all text is embedded as real, selectable text in subsets of the bundled fonts. A plot hidden through its visibility flag (for example from the viewer's legend) is left out of the PDF.

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
})?;

// Rasterise from five thousand faces upwards, at the default resolution.
fig.export_pdf_with("pressure.pdf", RasterOptions {
    policy: RasterPolicy::Auto { cells: 5_000 },
    ..RasterOptions::default()
})?;
```

Rendering the image needs a graphics adapter. A figure with nothing dense in it is exported without one, as it always was, so exporting on a machine without a GPU only fails for a figure that must be rasterised — and then it fails with a message naming `RasterPolicy::Never` rather than quietly writing something else.

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

Including the figure unscaled keeps text at the font size set in IronLAB, so a 9 pt label is 9 pt on the page. Passing `width=` or `scale=` to `\includegraphics` rescales the text and line widths with the rest of the figure. The PDF works with pdfLaTeX, XeLaTeX and LuaLaTeX, none of which need the `--shell-escape` option to include it.

## Opening the viewer

There are three ways to open figures in the interactive viewer, whose controls are described in [using the viewer](viewer.md).

- **From a program.** `fig.show()` opens the figure in a window and blocks until the window is closed. It consumes the figure; save or export it first if it is needed afterwards. On macOS the window must be opened from the main thread, so `show` is called from `main` or code that `main` calls directly.

    ```rust
    fig.show()?;
    ```

- **From saved files.** The viewer binary opens one or more `.fig` or `.json` files, one tab per file, choosing the format of each file from its extension:

    ```sh
    cargo run -p ironlab-viewer -- pressure.fig velocity.fig.json
    ```

- **From the gallery.** The gallery binary opens every example figure, one tab per figure, or only the figures whose slugs are given:

    ```sh
    cargo run -p ironlab-gallery -- view
    cargo run -p ironlab-gallery -- view surf legend_toggle
    ```

The gallery binary also writes the PDF, `.fig` file, `.fig.json` file and PNG image of every example into a directory with `cargo run -p ironlab-gallery -- export <dir>`.
