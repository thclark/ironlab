# Colour

A plot that maps its data to colours, such as a filled contour, a surface, a scalar image or a series coloured by data, does so through a **colour scale**. A colour scale is a node of the figure that turns a number into a colour, and every plot that maps numbers to colours names its scale. Several plots, in one axes or in several, may name the same scale, and then share its colormap, its limits and therefore its colours. A scale belongs to the figure rather than to an axes, so the colours of a plot never depend on where the plot lies.

This guide shows how a program creates scales, gives them to plots and chooses what a scale draws for values it cannot place. It assumes the plotting functions of [getting started](getting-started.md#plotting-functions). The examples use the prelude and the following data:

```rust
use ironlab::prelude::*;

let x = linspace(-1.0, 1.0, 41);
let y = linspace(-1.0, 1.0, 41);
let z = Matrix::from_fn(y.len(), x.len(), |row, col| x[col].hypot(y[row]));
```

Every property of a scale is described in the [figure schema reference](../reference/figure-schema.md#colour-scales), and the reasons for the design are recorded in [ADR 0024](../adrs/0024-colour-scales-colourbars-and-colour-keys.md).

## Colour scales

A scale has one of four kinds, and its kind fixes both how values become colours and how the scale is drawn:

| Kind | How a value becomes a colour | Created with |
| --- | --- | --- |
| Continuous | Its position between two limits picks a colour of a colormap. | `fig.color_scale()` |
| Discrete | It takes the single colour of the band, between two edges, in which it falls. | `.discrete(edges)` or `.bands(count)` on a scale |
| Indexed | A whole number `i` takes entry `i` of a palette. | `fig.indexed_scale(colors)` |
| Categorical | A whole number `i` takes category `i`, a colour with a label. | `fig.categorical_scale(pairs)` |

Each of these calls returns a handle, a `ColorScaleMut`, whose setters configure the scale and whose `id()` is the identifier that plots accept. Any plot that maps colours accepts a scale of any kind.

### The default scale of an axes

A program that never mentions a scale still gets one. Every plotting call that maps numbers to colours names the **default scale** of its axes, and creates a continuous scale with viridis and automatic limits when the axes has none. The colour-mapped plots of one axes therefore share one scale, as they share the axes colormap and colour limits in MATLAB, while two axes side by side each have a scale of their own.

`ax.color_scale()` returns the default scale, so that its colormap and limits can be set, as MATLAB's `colormap` and `clim` set those of the current axes:

```rust
let mut fig = Figure::new();
let mut ax = fig.axes(0, 0);
ax.contourf(&x, &y, &z);
ax.contour(&x, &y, &z).color(Color::BLACK);
ax.color_scale().colormap(Colormap::Magma).limits(0.0, 1.5);
```

The default scale is the scale of the first plot of the axes, in the order the plots were added, that names one. The rule reads only the figure, so a figure loaded from a file, or wrapped with `Figure::from_ir`, keeps adding plots to the same scale. It follows that a scale returned by `ax.color_scale()` before the axes holds any colour-mapped plot is new and is not remembered by the axes. Configure the default scale after the first colour-mapped plot, as above, or create a scale first and give it to the first plot, after which it is the default.

### Sharing a scale between plots

`fig.color_scale()` creates a scale of the figure, and the `color_scale(id)` setter of a contour, surface, series or scalar image gives a plot that scale in place of the default of its axes. Plots that name one scale share its colours exactly, because the automatic limits of a scale range over the values of every plot that names it, in every axes:

```rust
let mut fig = Figure::new().tiles(1, 3);
let speed = fig.color_scale().colormap(Colormap::Viridis).title("Speed (m/s)").id();
for col in 0..3 {
    fig.axes(0, col).surface(&x, &y, &z).color_scale(speed);
}
assert_eq!(fig.ir().color_scales.len(), 1);
```

Each `surface` call above first creates a default scale for its own axes, and `color_scale(speed)` then replaces it. A default scale that the plotting call created, and that nothing else names any more, is deleted when it is replaced, so the figure keeps only the scales the program asked for. A scale that the program created itself is never deleted this way, and a scale that no plot names is valid, because a colourbar may show a scale of its own.

## Continuous scales

A continuous scale places each value between its two limits and takes the colour of its colormap at that position. A value exactly at a limit lies inside the range.

### Colormaps

`.colormap(..)` sets the palette that a continuous or discrete scale interpolates. A bundled colormap is named by `Colormap`: `Viridis` (the default), `Cividis`, `Magma`, `Inferno`, `Plasma`, `Coolwarm` or `Gray`. An `ir::Colormap` reverses a bundled colormap, or lists colours of the program's own, from 2 to 1024 of them, which are interpolated linearly:

```rust
use ironlab::ir;

let mut fig = Figure::new();
let reversed = fig
    .color_scale()
    .colormap(ir::Colormap::Named { name: Colormap::Magma, reverse: true })
    .id();
let diverging = fig
    .color_scale()
    .colormap(ir::Colormap::Listed {
        colors: vec![Color::rgb(0.1, 0.3, 0.7), Color::WHITE, Color::rgb(0.7, 0.1, 0.1)],
    })
    .id();
```

### Limits

The limits of a new scale are automatic: they are the exact range of the finite values of every plot that names the scale, hidden plots included, so that hiding a plot in the viewer does not change the colours of the others. `.limits(min, max)` fixes them, which is MATLAB's `clim` and matplotlib's `vmin` and `vmax`. Limits that are not finite and increasing are reported by validation.

### Logarithmic scales

`.norm(ColorScaleNorm::Log)` places values by their logarithm, so that each decade takes an equal share of the colormap. A field that spans several decades, such as a concentration or a power spectral density, shows its structure only on such a scale. Under a logarithmic norm the automatic limits range over the positive values only, manual limits must be positive, and a value that is not positive lies below the range:

```rust
let mut fig = Figure::new();
let decades = fig
    .color_scale()
    .colormap(Colormap::Inferno)
    .norm(ColorScaleNorm::Log)
    .limits(0.1, 1000.0)
    .below(OutOfRange::Transparent) // leave anything below 0.1 undrawn
    .id();
fig.axes(0, 0).scalar_image(&concentration).color_scale(decades);
```

## Discrete scales and filled contours

A discrete scale gives every value the single colour of the band in which it falls, and every plot that names it is banded, a smooth surface included. With k bands, band i takes the colour of the colormap at i / (k − 1), so that both ends of the colormap are used. A value exactly on an edge between two bands belongs to the upper band.

`.discrete(edges)` makes a scale discrete with explicit band edges, and `.bands(count)` with about `count` bands at round values within its limits. Either keeps the colormap and norm already chosen. A filled contour that has no levels of its own takes the band edges of a discrete scale as its levels, so the bands of the contour are exactly the bands of the scale, and black isolines that name the same scale fall on the same edges:

```rust
let mut fig = Figure::new();
let bands = fig
    .color_scale()
    .colormap(Colormap::Cividis)
    .discrete([0.0, 0.25, 0.5, 0.75, 1.0, 1.5])
    .id();
let mut ax = fig.axes(0, 0);
ax.contourf(&x, &y, &z).color_scale(bands);
ax.contour(&x, &y, &z).color(Color::BLACK); // names the default scale, which is now `bands`
```

A contour given levels of its own with `levels(n)` or `level_values(v)` keeps them whatever its scale. Through a continuous scale, a contour without levels of its own draws ten levels at round values.

## Indexed and categorical scales

An indexed scale is a palette, of which a whole number `i` takes entry `i`. It is the colour scale of an indexed image, MATLAB's `image` with an indexed array, and it suits classes that are numbered rather than named. `ColormapSample::sample`, which the prelude provides for `Colormap`, samples a colormap into a palette of any length; 256 entries of a bundled colormap are exactly its own entries:

```rust
let classes = ByteMatrix::from_fn(64, 64, |row, col| ((row + col) % 4 * 85) as u8);

let mut fig = Figure::new();
let palette = fig.indexed_scale(Colormap::Viridis.sample(256)).id();
fig.axes(0, 0).scalar_image(&classes).color_scale(palette);
```

A categorical scale is a list of categories, each a colour with a label, of which a whole number `i` takes category `i`. It suits classes that are named, such as the outcome of each case of a parameter sweep, and the label of every category is required, so that a key can tell every category apart:

```rust
let outcome = ByteMatrix::from_rows(&[[0, 0, 1], [0, 2, 1]]);

let mut fig = Figure::new();
let status = fig
    .categorical_scale([
        (Color::rgb(0.0, 0.6, 0.3), "converged"),
        (Color::rgb(0.8, 0.2, 0.1), "diverged"),
        (Color::rgb(0.9, 0.7, 0.1), "stalled"),
    ])
    .id();
fig.axes(0, 0).scalar_image(&outcome).color_scale(status);
```

Only a whole number from 0 to the number of entries less one names an entry. Any other value, a fractional one such as 2.7 included, lies outside the entries, and by default it is a validation error, because a fractional index is almost always a mistake in the data that a lenient rule would hide. An 8-bit value is always a whole number, so a `ByteMatrix` never falls outside a palette of 256 entries. When indices are computed as floating-point numbers, `.outside(OutOfRange::Clamp)` truncates each toward zero and clamps it to the first or last entry instead.

Through an indexed or categorical scale, a surface face takes the value of its first corner, because the mean of four indices is not an index, and a contour without levels of its own places its levels halfway between whole numbers, so that each band holds exactly one entry.

## Colouring a series by data

A series, which `plot`, `scatter` and their relatives draw, is coloured by a third quantity with `color_data(values)`. The values are mapped through the default scale of the axes, which the call creates if the axes has none, or through the scale given by `color_scale(id)`. The call also sets the line colour to colour-mapped, so that the line and every marker that follows the colour of the series take the colours of the data.

The number of values decides what they colour:

- **One value per point** colours each marker by the value at its point, and draws each segment of the line as a gradient from the colour of its first point to the colour of its second. This is the form for a quantity measured at the points, such as the speed along a trajectory or the time at which each sample was taken.
- **One value per segment**, one fewer than the points, colours each segment of the line in a single colour. This is the form for a quantity that belongs to the interval between two points, such as the mean speed over a time step or the state of a system between two events. It needs a line, so validation refuses it on a series drawn by `scatter` without a line, and on a series whose markers are coloured by data, because no value belongs to a point.

A projectile coloured by its speed, which is measured at each point of its path:

```rust
let t = linspace(0.0, 2.0, 81);
let (vx, vy0, g) = (10.0, 10.0, 9.81);
let x: Vec<f64> = t.iter().map(|t| vx * t).collect();
let y: Vec<f64> = t.iter().map(|t| vy0 * t - 0.5 * g * t * t).collect();
let speed: Vec<f64> = t.iter().map(|t| vx.hypot(vy0 - g * t)).collect();

let mut fig = Figure::new();
let mut ax = fig.axes(0, 0);
ax.plot(&x, &y).color_data(&speed).line_width(2.0);
ax.color_scale().colormap(Colormap::Plasma).title("Speed (m/s)");
```

The gradient interpolates between the colours at the two ends of a segment rather than between the two values, so that a segment between two entries of an indexed or categorical scale never passes through the colour of a third. Through a continuous scale the difference is invisible when the points are close together, as they are along a sampled trajectory.

`scatter(x, y).color_data(values)` colours each marker by its value, as MATLAB's `scatter(x, y, sz, c)` does, and `size_data(sizes)` gives each marker its own diameter in points. A fixed colour set with `color(..)` after `color_data` colours the line and keeps the colour data, so that a series can have a black line with markers coloured by data:

```rust
let mut fig = Figure::new();
fig.axes(0, 0)
    .plot(&x, &y)
    .marker(Marker::Circle)
    .color_data(&speed)
    .color(Color::BLACK)
    .marker_face(ColorSpec::Colormapped);
```

The [figure schema reference](../reference/figure-schema.md#colour-data-of-a-series) describes the rules exactly, and the [coloured trajectory](../gallery/coloured_trajectory.md) in the gallery colours a baseball's flight, in a vacuum and through air, by its speed.

## Values out of range

A value that a scale cannot place falls into one of four categories, and each category has a policy of its own, so that a figure may, for example, tolerate missing values while refusing values outside the range:

| Category | Values | Set with | Default |
| --- | --- | --- | --- |
| Below | Less than the lower limit or first band edge, or not positive under a logarithmic norm | `.below(policy)` | Clamp |
| Above | Greater than the upper limit or last band edge | `.above(policy)` | Clamp |
| Outside | Not a whole number that names an entry of an indexed or categorical scale | `.outside(policy)` | Strict |
| Non-finite | NaN or infinite, on a scale of any kind | `.non_finite(policy)` | Transparent |

A policy is one of the following:

- `OutOfRange::Clamp` draws the nearest end colour of the scale: the first colour below the range and the last above it. A non-finite value has no nearest end, so a clamp draws nothing for it.
- `OutOfRange::Transparent` draws nothing, so that the background shows through. A NaN is the usual way of saying that nothing is drawn at a point, which is why non-finite values are transparent by default.
- A `Color` draws the value in that colour.
- `OutOfRange::Strict` makes such a value a validation error, `ValueOutOfRange`, which names the plot and the first value refused, so that the figure is neither exported nor shown until the data is corrected.

The policies belong to the scale, so every plot that names a scale treats such values alike. Validation checks a strict category against the values that a plot holds, never against values derived from them, such as the mean of the corners of a surface face. No value lies beyond automatic limits, because they are the range of the data, except a value that is not positive under a logarithmic norm.

## Colour scales in a figure file

A figure file holds its colour scales in the `color_scales` list of the figure, and each colour-mapped plot holds the identifier of its scale in its `color_scale` property. A colour-mapped plot that names no scale is the validation error `MissingColorScale`, because the axes holds no colormap to fall back on. The [figure schema reference](../reference/figure-schema.md#colour-scales) describes every property, the [mapping rules](../reference/figure-schema.md#mapping-rules) and the [automatic limits](../reference/figure-schema.md#automatic-limits).

A scale is not yet drawn as a colourbar or as a key in a legend. In a [legend](legends.md), the sample of a colour-mapped plot is drawn in the middle colour of its scale.

## Examples in the gallery

- [Coloured trajectory](../gallery/coloured_trajectory.md) colours the flight of a baseball, in a vacuum and through air, by its speed, with one value per point, so that each segment is a gradient.
- [Scatter](../gallery/scatter_2d.md) colours and sizes the markers of a scatter by data.
- [Discrete colour scale](../gallery/discrete_contourf.md) fills the pressure coefficient around a cylinder through a discrete scale, with black isolines that take the same band edges.
- [Logarithmic colour scale](../gallery/log_colour_scale.md) draws the concentration downwind of a stack, which spans four decades, through a logarithmic scale that leaves the cleanest air undrawn.
- [Indexed image](../gallery/indexed_image.md) draws numbered classes through a 256-entry palette, with indices beyond the palette drawn in a fixed colour.
- [Mapped image](../gallery/mapped_image.md) draws a field through a continuous scale, with a disc of NaN left transparent.
- [Measured speed faded by its confidence](../gallery/piv_confidence.md) fixes the limits of the scales of two axes to the same range, and leaves speeds beyond them undrawn in one of them.
