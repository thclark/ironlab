# Colour

A plot that maps its data to colours, such as a filled contour, a surface, a scalar image or a series coloured by data, does so through a **colour scale**. A colour scale is a node of the figure that turns a number into a colour, and every plot that maps numbers to colours names its scale. Several plots, in one axes or in several, may name the same scale, and then share its colormap, its limits and therefore its colours. A scale belongs to the figure rather than to an axes, so the colours of a plot never depend on where the plot lies.

This guide shows how a program creates scales, gives them to plots, chooses what a scale draws for values it cannot place, and shows a scale with a colourbar or in a legend. It assumes the plotting functions of [getting started](getting-started.md#plotting-functions). The examples use the prelude and the following data:

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

[Cells](getting-started.md#cells-and-categorical-rows) take a categorical scale in the same way, which is how an occupancy matrix colours each cell by its status: `occupancy` stores one row of cells per row of statuses, padding short rows with NaN, which the default non-finite policy draws as nothing, and a legend panel that lists the scale gives a key of the statuses as swatches. A cell holding a status is read in the [viewer](viewer.md#datatips) by the label of its category.

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

## Showing a scale with a colourbar

A **colourbar** shows a colour scale as a bar beside or inside the plots it colours, with ticks that read the bar as an axis is read. It holds only which scale it shows and where it is drawn. The colours, limits, band edges, ticks, tick labels, title and keys all belong to the scale, so every colourbar of one scale agrees with every other and with every plot drawn through it, and changing the limits of the scale changes the bar with the plots. A colourbar is drawn in one of two places: in a panel of the tile layout, or inside the plot area of an axes.

### A colourbar in a panel

A [panel](legends.md#legend-panels) is a node of the figure that occupies tiles of the layout, as an axes does, and holds one key. `panel.colorbar(scale)` makes that key a colourbar of the scale with the given identifier (MATLAB's and matplotlib's `colorbar`) and returns a `PanelColorbarMut` handle:

| Setter | Effect |
| --- | --- |
| `orientation(orientation)` | `ColorbarOrientation::Vertical`, the default, stands the bar upright with low values at the bottom and its tick labels on the right. `ColorbarOrientation::Horizontal` lays it flat with low values at the left and its tick labels below. |
| `length(fraction)` | Sets the length of the bar, its keys included, as a fraction of the span of the plots beside it. By default the length is automatic: the whole span for a vertical bar and half of it for a horizontal one. |
| `title_placement(placement)` | Sets where the title of the scale is drawn: `ColorbarTitlePlacement::Along`, the default, or `ColorbarTitlePlacement::Above` (see [The title of a bar](#the-title-of-a-bar)). |
| `id()` | Returns the identifier of the panel. |

A bar beside a plot belongs in a column whose size fits it, which `fit_col` makes:

```rust
let mut fig = Figure::new().tiles(1, 2).fit_col(1);
let mut ax = fig.axes(0, 0);
ax.contourf(&x, &y, &z);
let s = ax.color_scale().colormap(Colormap::Magma).title("Distance").id();
fig.panel(0, 1).colorbar(s);
```

The bar runs along the cells of its panel, which are inset by the margins of the rows or columns they span, and its length is measured against the span of the plots beside it. By default a vertical bar beside a row of plots is exactly as tall as their plot areas, and a horizontal bar beneath a row of plots is half as wide as they are, because a bar as long as a row of plots outweighs them. `length(fraction)` sets the length, keys included, to the given fraction of the span instead; a length that is not a finite number greater than 0 and at most 1 is the validation error `InvalidColorbarLength`, reported against the panel. A plot whose [aspect](getting-started.md#equal-aspect) is fixed may have a plot box smaller than its plot area, centred within it; the span then runs from the near edge of the first drawn plot box to the far edge of the last, so that the bar is always measured against the plots it serves.

The panel's [alignment](legends.md#legend-panels) places the bar both across its track and along the span. Along the span, a bar shorter than the span is centred by `Center` and by the two locations that centre it along the direction of the bar, and is pushed to the end that any other location names: `North`, `NorthEast` and `NorthWest` push a vertical bar to the top of the span, and `West`, `NorthWest` and `SouthWest` push a horizontal bar to its left end.

Across its length the bar takes the room it needs. In a [fitted](legends.md#fitting-rows-and-columns-to-their-legends) row or column, that room is the size of the track: the thickness of the bar, its ticks and tick labels, and its title, together with the title of the panel when it has one. The length of the bar never changes the size of the track. Every bar, in a panel, inside an axes or in a legend, is three quarters of the base font size thick.

A panel holds one key. Calling `colorbar` on a panel that holds a legend replaces the legend with a vertical colourbar, and calling `legend` on a panel that holds a colourbar replaces the colourbar with a legend of the default settings. Calling `colorbar` again on a panel that already holds a colourbar shows the new scale and keeps the orientation, the length and the placement of the title. The panel's own [title](legends.md#titling-a-panel) heads the bar, whatever the panel holds. An identifier that names no node is the validation error `UnknownColorScale`, and one that names a node other than a colour scale is `WrongNodeKind`; both name the panel.

### One colourbar for a row of plots

Plots that name one scale share its colours exactly, so one bar describes them all. A panel that spans the row beneath them, in a fitted row, shows the scale once, and its bar is measured against the plot areas of the whole row: half as long as the row and centred beneath it, unless `length` says otherwise:

```rust
let mut fig = Figure::new().tiles(2, 3).fit_row(1);
let shared = fig
    .color_scale()
    .colormap(Colormap::Inferno)
    .limits(0.0, 1.5)
    .title("Speed (m/s)")
    .id();
for col in 0..3 {
    fig.axes(0, col).scalar_image(&z).color_scale(shared);
}
fig.panel_span(1, 0, 1, 3)
    .colorbar(shared)
    .orientation(ColorbarOrientation::Horizontal);
```

A vertical bar in a fitted column, spanning the rows of a stack of plots, serves a column in the same way.

### A colourbar inside an axes

`ax.colorbar(scale, side)` draws a colourbar inside the plot area of the axes, along one of its edges (MATLAB's `colorbar` with a location such as `'east'`), and returns an `InsideColorbarMut` handle whose `length(fraction)` sets how long the bar is and whose `title_placement(placement)` sets where the title of the scale is drawn:

```rust
let mut fig = Figure::new();
let mut ax = fig.axes(0, 0);
ax.scalar_image(&z);
let s = ax.color_scale().title("Distance").id();
ax.colorbar(s, InsideColorbarSide::East).length(0.6);
```

The side is an `InsideColorbarSide`: `South`, the default, and `North` lay the bar flat along the bottom or top edge, and `East` and `West` stand it upright along the right or left edge. The tick labels face the interior of the plot area. The bar, its keys, its labels and its title stand on a white box with an outline, drawn as the box of an inside legend is drawn, so that they read over any plot. The length is the length of the box, keys included, as a fraction of the edge, centred along it; it is 0.8 by default, and a length that is not a finite number greater than 0 and at most 1 is the validation error `InvalidColorbarLength`. Calling `colorbar` again moves the bar to the new side or scale and keeps its length and the placement of its title, and `colorbar_off` removes it.

An inside colourbar takes no margin, so the plot area keeps its size whether or not the bar is shown, and the bar covers what is drawn beneath it. It suits a plot with a quiet region along one edge, such as open water on a map. A bar that needs room of its own belongs in a panel.

### The title of a bar

The title of the scale is drawn beside every bar of the scale, where the bar's `title_placement` says:

| Placement | Where the title is drawn |
| --- | --- |
| `Along`, the default | Beyond the tick labels, on their side of the bar, one gap from them and centred on the bar. The gap is two thirds of the gap between a title above a bar and the bar, because the line boxes of the tick labels and of the title each carry white space of their own. Beside a vertical bar the title is turned a quarter turn to read from bottom to top, as the label of a y axis does; beneath a horizontal bar it is upright, below the tick labels, as the label of an x axis is. |
| `Above` | As an upright heading. Above a vertical panel bar it lies in the margin above the first row the panel spans, where the title of an axes would be, so the bar keeps the span of the plots; above a horizontal panel bar it lies within the panel. |

A title along a bar takes room beside the bar rather than above it, so a vertical bar titled along its length leaves the plots where they would be without a title, and a fitted track grows only by the line of the title and one gap, however long the title is. A title along a bar is centred on the bar alone, not on its keys; a title longer than the bar runs beyond its ends.

Inside an axes, whose tick labels face the interior of the plot area, a title along the bar lies beyond the tick labels toward the interior, and a title above is a heading over the top end of a vertical bar and lies on the side of a horizontal bar away from its tick labels: below a bar along the south edge and above a bar along the north edge. Either way the title lies within the box behind the bar.

A [bar item](#bars-are-items) of a legend is always titled along its bar.

### How each kind of scale is drawn

A colourbar draws its scale according to the kind of the scale, so that it cannot misrepresent the mapping:

| Kind | The bar | Automatic ticks |
| --- | --- | --- |
| Continuous | A smooth gradient of 256 samples between the limits, placed linearly or logarithmically by the norm. | Round values, chosen as an axis chooses them; decades under a logarithmic norm. |
| Discrete | One block of a single colour per band, between its edges. | Every band edge. |
| Indexed | One block per entry, side by side, in the order of the entries. | Whole numbers only, each at the centre of the entry it names. |
| Categorical | One outlined square per category, separated by margins, each with its label beside it. | None; each category is labelled by its own label. |

The ends of a bar are always flat. The colour drawn for values out of range is never drawn at an end of the bar, because a colour there cannot be told apart from the last colour in range; such values are shown by [keys](#keys-for-values-out-of-range) instead.

### Ticks and tick labels

The ticks of a colourbar are set on its scale, with setters that mirror those of an axis:

| Setter | Effect |
| --- | --- |
| `ticks(values)` | Places the ticks at the given values. The values of an indexed scale are entry indices, and each tick is drawn at the centre of its entry. |
| `tick_step(step)` | Places a tick at every multiple of `step` within the range of the scale. |
| `tick_labels(pairs)` | Places the ticks at the values of `(value, label)` pairs and labels each tick with its text, in one call, because a label means nothing apart from its value. A plain string is read as LaTeX, as every text of a figure is. |
| `tick_labels_hidden()` | Hides the tick labels and keeps the ticks. |

These setters apply to continuous, discrete and indexed scales, and leave a categorical scale, which labels every category itself, as it is. Explicit ticks and labels are checked as those of an axis are, with the validation errors `InvalidTicks` and `InvalidTickLabels`. A long palette is usually best labelled at a few named entries:

```rust
let mut fig = Figure::new();
let terrain = fig
    .indexed_scale(Colormap::Gray.sample(256))
    .tick_labels([(0, "water"), (128, "land"), (255, "ice")])
    .id();
```

### Keys for values out of range

A colourbar can show, beside the bar, a **key** for each [category of values out of range](#values-out-of-range): a square as wide as the bar is thick, separated from the bar and from the next square by one margin, with its label on the side of the tick labels. Beside a vertical [bar item](#bars-are-items) of a legend, each key is instead an entry row of its own. Keys are off by default, and showing one never changes what the scale draws. `show_key(key)` shows the key of one category with its default label, and `key_label(key, text)` shows it with the given label:

```rust
let mut fig = Figure::new();
fig.color_scale()
    .limits(20.0, 100.0)
    .above(Color::rgb(0.3, 0.85, 0.95))
    .show_key(ColorScaleKey::Above) // labelled "> 100"
    .key_label(ColorScaleKey::NonFinite, "No reading");
```

A `ColorScaleKey` names the category: `Below` and `Above` on a continuous or discrete scale, `Outside` on an indexed or categorical scale, and `NonFinite` on every kind. A key that the kind of the scale does not have leaves the scale as it is, as the policy setters do.

The default labels use strict inequalities, because a value exactly at a limit lies inside the range. The limits are written as the ticks write them, and for a discrete scale they are its first and last band edges:

| Key | Default label |
| --- | --- |
| Above | `> upper` |
| Below | `< lower` |
| Outside | `Out of range` |
| NonFinite | `NaN / Inf` |

A square shows what the policy of its category paints. A colour policy fills it with that colour, and a clamp below or above the range fills it with the end colour that the clamp paints. A transparent or strict policy paints nothing, so its square is empty, as is the square of a clamp outside the entries or of non-finite values, which paints no single colour. An empty square is the honest key for a region left blank, such as concentrations below the lower limit of a logarithmic scale.

The keys lie beyond the end of the bar whose values they stand for, and the key for values that are not finite lies outermost, beyond the low end:

| Bar | Order of the pieces |
| --- | --- |
| Vertical, continuous or discrete | From the top: the above key, the bar, the below key, the non-finite key. |
| Horizontal, continuous or discrete | From the left: the non-finite key, the below key, the bar, the above key. |
| Vertical, indexed | From the top: the bar, the outside key, the non-finite key. |
| Horizontal, indexed | From the left: the non-finite key, the outside key, the bar. |
| Categorical | After the categories: the outside key, then the non-finite key. |

Along a horizontal bar the labels lie side by side beneath the squares, so the margin between the pieces widens until no two labels meet.

### Colourbars in the viewer

In the viewer a colourbar is the axis of its scale. Hovering a bar reads the value, the band, the entry, the category or the key under the pointer; the wheel over a continuous bar, or over a discrete bar of automatic levels, zooms the limits of the scale about the value under the pointer, and a drag along it pans them. A gesture sets the scale's limits once, so every bar of the scale and every plot drawn through it changes together, and a double-click on the bar restores the limits the figure was opened with. The colour of each entry of an indexed scale, the colour and label of each category, and each colour of a listed colormap can be changed in the property editor, while the number of entries cannot. [Colourbars](viewer.md#colourbars) in the viewer guide describes all of it.

## Colour keys in legends

A colour scale can also be shown as an item of a [legend](legends.md), so that the key of a classification, or the bar of a scale, sits among the entries that explain the rest of the figure. The item names the scale, as a colourbar does, and holds nothing else, so it agrees with every colourbar and every plot of the scale.

### Listing a scale

`item` on the legend of a panel recognises the identifier of a scale when it is called, and lists the scale; any other identifier lists the plot it names. `full_row` does the same on a row of its own. A legend inside an axes takes the item `LegendItem::color_scale(id)` in its explicit entries:

```rust
let mut fig = Figure::new().tiles(2, 1).fit_row(1);
let status = fig
    .categorical_scale([
        (Color::rgb(0.0, 0.45, 0.7), "converged"),
        (Color::rgb(0.9, 0.6, 0.0), "diverged"),
    ])
    .key_label(ColorScaleKey::NonFinite, "not run")
    .id();
let speed = fig.color_scale().limits(0.0, 1.0).title("Speed (m/s)").id();
let mut ax = fig.axes(0, 0);
let path = ax.plot(&x, &y).label("Path").id();
ax.legend(LegendLocation::NorthEast)
    .legend_entries(LegendEntries::Explicit(vec![
        LegendItem::artist(path),
        LegendItem::color_scale(speed),
    ]));
fig.panel(1, 0).legend().item(status);
```

A legend may list any scale of the figure, whichever axes the plots that use it belong to, and a scale that no plot uses is valid, so a key can explain a classification on its own. A scale enters a legend only as an explicit item: the automatic entries of a legend list plots alone, so adding a colour-mapped plot never adds a key that the author did not ask for.

### Categories are entries

A categorical scale is listed as one entry per category, in the order of the categories, each labelled with the category's label beside a swatch of its colour. The [keys](#keys-for-values-out-of-range) that the scale shows follow as further entries, the outside key and then the non-finite key, each a swatch filled with what is painted for its values, or empty for values drawn as nothing, labelled with its label or its default label. These are ordinary entries: they take their place among the other items of the list, and are arranged in columns and wrapped like any other entry. On a row of its own, each entry of the scale is on a row of its own.

In a legend panel, the entry of a category is merged with any other entry of the panel that has the same label and the same look, whether that entry lists a plot, stands for no plot or is another category, and the merged entry keeps the place of the first of them. A hand-written swatch of the same label and colour as a category is therefore one entry, and so is a surface of that colour with thin black edges listed under the category's label, whose entry then hides or shows the surface when it is clicked. Entries that share a label but not a look are both listed. Two items that repeat each other, of which neither is a category, are both listed, as the [legends guide](legends.md#choosing-the-entries) describes. A legend inside a plot area merges nothing, the entries of categories included: it lists every item as it is given.

### Bars are items

A continuous, discrete or indexed scale is drawn as a vertical bar in one column of the legend, five entry rows long unless the item asks for another number of rows. `LegendItem::color_scale(scale).with_rows(rows)` makes an item of the given number of rows, which is listed by `item` or `legend_entries` as any item is; a count of zero is the validation error `InvalidLegendRows`, reported against the owner of the legend:

```rust
fig.panel(0, 1)
    .legend()
    .item(gnss)
    .item(LegendItem::color_scale(height).with_rows(7));
```

The bar is drawn as a vertical colourbar of the scale is drawn, with the same thickness, colours, ticks and tick labels: it is centred on the column of samples, so that its centre line passes through the centres of the samples of the other entries in its column and of the swatches of its keys, and its tick labels lie to its right, on the side of the labels of the other entries. Its ends are kept half a label inside its rows, so that the tick labels at its ends stay clear of the entries above and below. The title of the scale lies along the bar, beyond its tick labels, reading from bottom to top, so it adds no row; a longer title is better served by more rows than by a heading.

Each [key](#keys-for-values-out-of-range) the scale shows is an entry row of its own, drawn as a swatch entry is drawn: the above key directly above the rows of the bar, and the below key and then the non-finite key directly below them. A key's swatch is filled with what is painted for its values, or left empty, and its label is the key's label or its default label. A key row belongs to its bar: it is never merged with another entry and is no toggle in the viewer. When the entries are divided among columns, the item counts as its rows and its keys and is never split between columns, and its width, ticks, tick labels and title included, sets the width of its column as a label does, so a column that holds nothing but the bar is exactly as wide as the bar.

On a row of its own, the bar lies horizontally across the width of the legend whatever rows it asks for, with its labels below it and its title below its labels, as a horizontal colourbar is drawn; its keys are squares beside the bar, as on a colourbar.

Neither a bar nor the entry of a category is a toggle in the viewer, because a scale is not a plot: clicking it hides nothing, unless the entry of a category is merged with the entry of a plot in a legend panel, and clicking a bar selects its scale in the property editor. A bar item is read by hovering as a colourbar is, but a gesture on it does not move the limits of its scale, because it belongs to its legend.

### Samples of colour-mapped plots

The sample of a plot that maps its data to colours is a **sweep** of its scale, so that the key shows what its colours mean rather than one colour of them. A sweep runs from the low end of the scale at the left to its high end at the right, as a horizontal colourbar does, whatever the data of the plot:

| Scale | Sweep |
| --- | --- |
| Continuous | A gradient through the colormap. A reversed colormap reverses the colours, never the direction of the values. |
| Discrete | One band per band of the scale, side by side, each of equal width and in its single colour. |
| Indexed or categorical | The entry colours side by side, in entry order, each of equal width. |

A filled contour, a scalar image and a surface with colour-mapped faces draw the sweep as a filled band, outlined in a fixed edge colour if the plot has one. Colour-mapped isolines, and a series whose line is coloured by data, draw the sweep along the line of the sample. A marker that follows the colour data of a series point by point takes the colour of the sweep where it sits: at the centre of the line, or, for a series that draws only markers, in a row of markers across the sample. The sweep takes the opacity of the plot, but not its alpha, which varies over the plot and has no single value to show. A legend panel merges two plots of the same label that sweep the same colours, as it merges two lines of the same colour.

A surface whose faces are one colour draws a swatch of that colour, outlined in its edge colour. Every swatch is drawn alike: a square of its colour, outlined in thin black, smaller than its row so that the swatches of neighbouring rows never touch. A `LegendSample::swatch` item, the entry of a category, and the key of a colourbar all draw it.

### Validation of scale items

A legend item that names no node of the figure as its scale is the validation error `UnknownColorScale`, and one that names a node other than a colour scale is `WrongNodeKind`; both are reported against the owner of the legend, the panel of a legend panel or the axes of an inside legend, and the scene compiler leaves the item out. A scale listed twice is the warning `DuplicateLegendItem`. A scale item is never `UnnamedLegendItem`, because the scale draws its own labels, so an untitled bar is a normal key. Removing a scale that a legend lists is refused, as removing a scale that a plot or a colourbar names is, unless the legend is removed in the same transaction.

## Colour scales in a figure file

A figure file holds its colour scales in the `color_scales` list of the figure, and each colour-mapped plot holds the identifier of its scale in its `color_scale` property. A colour-mapped plot that names no scale is the validation error `MissingColorScale`, because the axes holds no colormap to fall back on. The [figure schema reference](../reference/figure-schema.md#colour-scales) describes every property, the [mapping rules](../reference/figure-schema.md#mapping-rules) and the [automatic limits](../reference/figure-schema.md#automatic-limits).

A legend item that shows a scale is `{"content": {"type": "color_scale", "value": …}, "full_row": …}` in the explicit entries of the legend, as the reference describes under [legend entries](../reference/figure-schema.md#legend-entries). A colourbar is stored where it is shown: a panel that holds one has the content `{"type": "colorbar", "value": {"scale": …, "orientation": …}}`, and an axes that shows one inside its plot area holds it in its `colorbar` property. Both are described under [panel content](../reference/figure-schema.md#panel-content) and [inside colourbars](../reference/figure-schema.md#inside-colourbar) in the reference.

## Examples in the gallery

- [Branch map](../gallery/branch_map.md) colours a parameter sweep by the branch a nonlinear solver converged to, through a categorical scale, and keys it with a boxed legend panel under the plot in two columns: the categories of the scale, with the cases never run keyed by the non-finite key "not run", fill the first, and the curves and markers drawn over the map fill the second.
- [Case matrix](../gallery/case_matrix.md) bands a test matrix through a discrete scale shown by a horizontal colourbar, with a "not entered" key for the cases the model never entered, and lists a marker overlay in a legend inside the plot.
- [Markers over a surface](../gallery/markers_over_surface.md) keys a surface and the features drawn on it in two boxed legend panels stacked beside the plot, each headed by its title: the first lists two kinds of marker and a bar of the surface's scale, seven rows long, in one column, and the second lists a bar of a road's gradient, five rows long, in one column and four lines and markers in the other.
- [Colourbar shared by a row](../gallery/row_shared_colorbar.md) shows one scale for three plots with one horizontal bar in a fitted row, and a key for the temperatures above the scale.
- [Inside colourbar](../gallery/inside_colorbar.md) draws the bar of a bathymetry map inside the plot area, over open water, with a key naming the land above the range of the scale.
- [Indexed palette](../gallery/indexed_palette.md) draws an 8-bit elevation raster through its 256-entry palette, whose bar labels three entries by name.
- [Coloured trajectory](../gallery/coloured_trajectory.md) colours the flight of a baseball, in a vacuum and through air, by its speed, with one value per point, so that each segment is a gradient, and shows the speeds with a horizontal colourbar.
- [Scatter](../gallery/scatter_2d.md) colours and sizes the markers of a scatter by data.
- [Discrete colour scale](../gallery/discrete_contourf.md) fills the pressure coefficient around a cylinder through a discrete scale, with black isolines and a banded colourbar that take the same band edges.
- [Logarithmic colour scale](../gallery/log_colour_scale.md) draws the concentration downwind of a stack, which spans four decades, through a logarithmic scale that leaves the cleanest air undrawn, and shows an empty key for it beside the bar.
- [Indexed image](../gallery/indexed_image.md) draws numbered classes through a 256-entry palette, with indices beyond the palette drawn in a fixed colour.
- [Mapped image](../gallery/mapped_image.md) draws a field through a continuous scale, with a disc of NaN left transparent.
- [Measured speed faded by its confidence](../gallery/piv_confidence.md) fixes the limits of the scales of two axes to the same range, and leaves speeds beyond them undrawn in one of them.
