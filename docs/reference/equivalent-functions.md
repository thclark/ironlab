# Equivalent functions

This page maps the plotting functions of MATLAB, matplotlib and Plotly to the IronLAB function that draws the same plot. IronLAB follows MATLAB's names where MATLAB has one, so most rows differ only for readers who arrive from matplotlib or Plotly. The exceptions are the rows in which one MATLAB function covers several kinds of plot, which IronLAB separates (`image`), those in which IronLAB draws two MATLAB plots with one kind of object (`pcolor`, which is a surface in a two-dimensional axes, and `imagesc` and indexed `image`, which are both a scalar image), and the colour functions, which IronLAB applies to a colour scale rather than to an axes.

Every IronLAB function in the plot tables is a method of `AxesMut` and is described in [getting started](../guides/getting-started.md#plotting-functions). The matplotlib names are methods of `Axes`, or of `Axes3D` where the plot is three-dimensional. The Plotly names are trace types of `plotly.graph_objects` unless they begin with `px.` (Plotly Express) or `ff.` (the figure factory).

## Lines and markers

| Plot | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Line in two dimensions | `plot` | `plot` | `Scatter` with `mode="lines"` | `plot` |
| Line in three dimensions | `plot3` | `Axes3D.plot` | `Scatter3d` with `mode="lines"` | `plot3` |
| Line on logarithmic axes | `loglog`, `semilogx`, `semilogy` | `loglog`, `semilogx`, `semilogy` | `Scatter` on an axis with `type="log"` | `loglog`, `semilogx`, `semilogy`, or `xscale`, `yscale` and `zscale` on the axes |
| Markers in two dimensions | `scatter` | `scatter` | `Scatter` with `mode="markers"` | `scatter` |
| Markers in three dimensions | `scatter3` | `Axes3D.scatter` | `Scatter3d` with `mode="markers"` | `scatter3` |
| Line with markers | `plot` with a marker in the line specification | `plot` with `marker=` | `Scatter` with `mode="lines+markers"` | `plot` with `marker`, or `scatter` with `dash` |
| Markers sized and coloured by data | `scatter(x, y, sz, c)` | `scatter` with `s=` and `c=` | `Scatter` with `marker.size` and `marker.color` set to arrays | `scatter` with `size_data` and `color_data` |
| Line coloured along its length by values at its points | `patch` or `surface` with `EdgeColor` set to `interp` | no direct equivalent; a `LineCollection` colours each segment in one colour | no equivalent for a line | `plot` with `color_data`, one value per point |
| Line coloured segment by segment | `patch` with `EdgeColor` set to `flat` | `LineCollection` with `set_array` | no equivalent for a line | `plot` with `color_data`, one value per segment |
| Markers along named rows (a categorical strip chart) | `scatter` with `yticks` and `yticklabels`, or `swarmchart` | `scatter` with `set_yticks` and `set_yticklabels` | `px.strip` | `scatter` with `yticks`, `ytick_labels` and `yreverse` on the axes |
| Horizontal reference line across the axes | `yline` | `axhline` | `add_hline` | `hline` |
| Vertical reference line across the axes | `xline` | `axvline` | `add_vline` | `vline` |

Every function in this table adds the same kind of object, a series, and returns the same handle, so that the functions differ only in the defaults they set: a line drawn by `plot` can be given markers sized and coloured by data, and markers drawn by `scatter` can be joined by a line. [Colouring a series by data](../guides/colour.md#colouring-a-series-by-data) describes colour data per point and per segment.

A line or a set of markers placed in data coordinates along one axis and in fractions of the plot area along the other, which matplotlib draws with a blended transform such as `ax.get_xaxis_transform()`, is a series whose `x_coords` or `y_coords` is `Coords::AxesFraction`; matplotlib's `clip_on=False` is `clip(false)`. [Reference lines and edge markers](../guides/getting-started.md#reference-lines-and-edge-markers) describes both.

## Gridded fields

| Plot | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Isolines | `contour` | `contour` | `Contour` with `contours_coloring="lines"` | `contour` |
| Filled contours | `contourf` | `contourf` | `Contour` | `contourf` |
| Isolines raised to their levels | `contour3` | `Axes3D.contour` | no equivalent trace | `contour3` |
| Surface in three dimensions | `surf` | `Axes3D.plot_surface` | `Surface` | `surf` |
| Wireframe in three dimensions | `mesh` | `Axes3D.plot_wireframe` | no equivalent trace | `mesh` |
| Pseudocolour plot: a field on the vertices of a rectilinear or curvilinear grid | `pcolor` | `pcolormesh`, `pcolor` | `Heatmap` with `x` and `y` coordinates, on a rectilinear grid only | `surface` in a two-dimensional axes |

`surface` adds the same object as `surf` and leaves the projection of the axes as it is, so it draws a pseudocolour plot in a two-dimensional axes and a surface in a three-dimensional one. [Surfaces in two-dimensional axes](../guides/getting-started.md#surfaces-in-two-dimensional-axes) explains how such a plot differs from a scalar image and from MATLAB's `pcolor`, and the [Flow past a cylinder](../gallery/cylinder_flow.md) gallery entry shows one on a polar mesh.

## Images

| Plot | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Colour-mapped image: data values scaled through the colour limits into the colormap | `imagesc`, or `imshow` with a display range | `imshow` or `matshow` with a two-dimensional array | `Heatmap`, or `px.imshow` with a two-dimensional array | `scalar_image`, through a continuous colour scale (the default) |
| True-colour image | `image` or `imshow` with an m × n × 3 array | `imshow` with an m × n × 3 or m × n × 4 array | `Image`, or `px.imshow` with an m × n × 3 or m × n × 4 array | `image` |
| Colour-indexed image: values that name colormap entries directly | `image` with an indexed array, or `imshow(X, map)` | `imshow` with `norm=NoNorm()` | no equivalent trace | `scalar_image`, through an indexed colour scale made by `fig.indexed_scale(..)` |

[Images](../guides/getting-started.md#images) describes the two image artists and how each is placed, and the [colour guide](../guides/colour.md#indexed-and-categorical-scales) describes the indexed scale.

The options that orient and place an image in MATLAB and matplotlib have one IronLAB equivalent, the explicit pixel ranges of the image.

| Purpose | MATLAB | matplotlib | IronLAB |
| --- | --- | --- | --- |
| Draw row 0 of an n-row array at the top | `axis ij`, which `image`, `imagesc` and `imshow` apply | `origin="upper"`, the default of `imshow` | `pixel_rows(n − 1, 0)`, or any row range whose `first` is greater than its `last` |
| Draw row 0 at the bottom | `axis xy` | `origin="lower"` | the default placement, or any row range whose `first` is less than its `last` |
| Place the image in data coordinates | `XData` and `YData`, which give the centres of the outermost pixels | `extent`, which gives the outer edges of the image | `pixel_columns(first, last)` and `pixel_rows(first, last)`, which give the centres of the outermost pixels |

IronLAB has no axis-direction or image-origin property, for the reason given in [Image orientation](../guides/getting-started.md#image-orientation).

## Colour

MATLAB and matplotlib set the colormap and the colour limits of an axes, or of one plot, while IronLAB sets them on a [colour scale](../guides/colour.md), which every colour-mapped plot names. The colour-mapped plots of an axes share its default scale, which `ax.color_scale()` returns, so the MATLAB functions that act on the current axes act on that scale in IronLAB.

| Purpose | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Choose the colormap | `colormap(ax, name)` | `cmap=` on the plot, or `set_cmap` | `colorscale=` on the trace | `ax.color_scale().colormap(Colormap::Magma)`, or `.colormap(..)` on a scale made by `fig.color_scale()` |
| Use a colormap of one's own | `colormap(ax, m)` with an m × 3 array | `ListedColormap` or `LinearSegmentedColormap` | a list of colours as `colorscale` | `.colormap(ir::Colormap::Listed { colors })` |
| Reverse a colormap | `colormap(flipud(m))` | a name ending in `_r`, or `cmap.reversed()` | `reversescale=True` | `.colormap(ir::Colormap::Named { name, reverse: true })` |
| Fix the colour limits | `clim` (`caxis` before R2022a) | `vmin=` and `vmax=`, or `set_clim` | `zmin=` and `zmax=`, or `cmin=` and `cmax=` | `.limits(min, max)` on the scale |
| Map values logarithmically | `set(gca, 'ColorScale', 'log')` | `norm=LogNorm()` | no option; the values are transformed first | `.norm(ColorScaleNorm::Log)` on the scale |
| Share colours across several plots | the same `clim` set on each axes | the same `norm` passed to each plot | `coloraxis` shared by the traces | one scale given to each plot with `.color_scale(id)` |
| Colour out-of-range values | no option for values beyond the limits | `cmap.set_under`, `set_over` and `set_bad` | no option | `.below(..)`, `.above(..)`, `.outside(..)` and `.non_finite(..)` on the scale |
| Sample a colormap into a palette | `parula(256)`, or any colormap function with a count | `cmap(np.linspace(0, 1, n))` | `px.colors.sample_colorscale` | `Colormap::Viridis.sample(256)` |

### Colourbars

A colourbar in IronLAB shows a scale rather than a plot, and everything it draws, its ticks and title included, belongs to that scale, so the setters below are called on the scale. The [colour guide](../guides/colour.md#showing-a-scale-with-a-colourbar) describes colourbars.

| Purpose | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Show a colourbar beside an axes | `colorbar` | `fig.colorbar(mappable, ax=ax)` | `showscale=True` on the trace | `fig.panel(row, col).colorbar(scale)` in a fitted column |
| Lay the bar flat beneath the plots | `colorbar('southoutside')` | `orientation="horizontal"` | `colorbar=dict(orientation="h")` | `.orientation(ColorbarOrientation::Horizontal)` on the panel's colourbar, in a fitted row |
| Share one bar across several axes | `colorbar` on a tiled layout with `Layout.Tile = 'east'` | `fig.colorbar(mappable, ax=[ax1, ax2, ax3])` | `coloraxis` shared by the traces | `fig.panel_span(..).colorbar(scale)` spanning their rows or columns |
| Draw the bar inside the plot area | `colorbar('east')` and the other inside locations | `cax=ax.inset_axes(..)` | `colorbar=dict(x=, y=, len=)` | `ax.colorbar(scale, InsideColorbarSide::East)`, with `.length(fraction)` |
| Title the bar | `title(c, text)` or `c.Label.String` | `cbar.set_label(text)` | `colorbar=dict(title=text)` | `.title(text)` on the scale |
| Place the ticks | `c.Ticks = v` | `cbar.set_ticks(v)` | `colorbar=dict(tickvals=v)` | `.ticks(v)` or `.tick_step(step)` on the scale |
| Label the ticks | `c.TickLabels = labels` | `cbar.set_ticks(v, labels=labels)` | `colorbar=dict(tickvals=v, ticktext=labels)` | `.tick_labels([(value, label), ..])` on the scale |
| Show the colour of values beyond the limits | no option | `extend="min"`, `"max"` or `"both"`, drawn as pointed ends | no option | `.show_key(ColorScaleKey::Above)` and the other keys on the scale, drawn as separate squares |

### Levels of filled contours

| Purpose | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Choose about n levels | `contourf(x, y, z, n)` | `contourf(x, y, z, n)` | `ncontours=n` | `.levels(n)` on the contour |
| Give the levels explicitly | `contourf(x, y, z, v)` with a vector `v` | `contourf(x, y, z, levels=v)` | `contours=dict(start=, end=, size=)`, evenly spaced only | `.level_values(v)` on the contour |
| Fill bands that match a stepped colour bar | `contourf` with a vector of levels and a colormap of one colour per band | `contourf` with `levels=v` and a `BoundaryNorm` of the same boundaries | no equivalent | a discrete scale, `fig.color_scale().discrete(v)`, named by a `contourf` with no levels of its own |

A filled contour with no levels of its own takes the band edges of a discrete scale, so the bands of the contour and the bands of the scale cannot disagree; the [colour guide](../guides/colour.md#discrete-scales-and-filled-contours) shows this.

## Vector fields

| Plot | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Arrows in two dimensions | `quiver` | `quiver` | `ff.create_quiver` | `quiver` |
| Arrows in three dimensions | `quiver3` | `Axes3D.quiver` | `Cone` | `quiver3` |

## Plots without an equivalent

IronLAB does not yet draw bar charts, histograms, stem, stairs, area or error-bar plots, patches or streamlines. The [figure schema](figure-schema.md#extending-the-model-with-new-plot-types) describes how such plot types are added.
