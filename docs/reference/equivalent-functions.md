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
| Heatmap of tiles: one value per cell, centred on whole numbers | `heatmap` | `pcolormesh` with one value per cell, or `imshow` | `Heatmap` | `cells` |
| Tiles between uneven edges, such as the bins of a two-dimensional histogram | `histogram2` with `DisplayStyle` set to `tile`, from its counts | `pcolormesh(x_edges, y_edges, c)`, with one more edge than cells along each axis | `Heatmap` with `x` and `y` holding the edges | `cells` with `column_edges` and `row_edges` |
| Tiles set apart by a gap | no equivalent; `heatmap` draws seamless tiles | no equivalent; `pcolormesh` with `edgecolors="w"` and `linewidth=` draws lines over the tiles instead | `Heatmap` with `xgap` and `ygap` | `cells` with `gap(x, y)` |
| Ragged rows of tiles coloured by a status (an occupancy matrix) | `heatmap` of rows padded with NaN | `pcolormesh` of rows padded with NaN | `Heatmap` of rows padded with `None`, with `xgap` and `ygap` | `occupancy`, through a categorical scale |

`surface` adds the same object as `surf` and leaves the projection of the axes as it is, so it draws a pseudocolour plot in a two-dimensional axes and a surface in a three-dimensional one. [Surfaces in two-dimensional axes](../guides/getting-started.md#surfaces-in-two-dimensional-axes) explains how such a plot differs from a scalar image and from MATLAB's `pcolor`, and the [Flow past a cylinder](../gallery/cylinder_flow.md) gallery entry shows one on a polar mesh.

A value that belongs to a cell between edges, rather than to a vertex of a grid, is drawn by `cells`, which is what matplotlib's `pcolormesh` draws when it is given one more edge than values along each axis. `occupancy` pads ragged rows with NaN and sets the cells apart by a gap, as [cells and categorical rows](../guides/getting-started.md#cells-and-categorical-rows) describes.

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
| Shorten the bar | `c.Position` | `fig.colorbar(.., shrink=0.5)` | `colorbar=dict(len=0.5)` | `.length(0.5)` on the panel's colourbar, placed along the plots by the panel's `align` |
| Title the bar | `title(c, text)` or `c.Label.String` | `cbar.set_label(text)` | `colorbar=dict(title=text)` | `.title(text)` on the scale, drawn along the bar unless the bar's `.title_placement(ColorbarTitlePlacement::Above)` makes it a heading |
| Place the ticks | `c.Ticks = v` | `cbar.set_ticks(v)` | `colorbar=dict(tickvals=v)` | `.ticks(v)` or `.tick_step(step)` on the scale |
| Label the ticks | `c.TickLabels = labels` | `cbar.set_ticks(v, labels=labels)` | `colorbar=dict(tickvals=v, ticktext=labels)` | `.tick_labels([(value, label), ..])` on the scale |
| Show the colour of values beyond the limits | no option | `extend="min"`, `"max"` or `"both"`, drawn as pointed ends | no option | `.show_key(ColorScaleKey::Above)` and the other keys on the scale, drawn as separate squares |
| List the categories of a classification in a legend | dummy patches, one per class, passed to `legend` | `legend(handles=[Patch(color=c, label=l), ..])` | one dummy trace per class | `.item(scale)` on a legend with the identifier of a categorical scale |
| Put a colour bar among legend entries | no option | no option | no option | `.item(scale)` on a legend with the identifier of a continuous, discrete or indexed scale, [drawn as a bar](../guides/colour.md#bars-are-items) |
| Title a legend or a colourbar beside the plots | `title(lgd, text)` | `ax.legend(title=text)` | `legend=dict(title=dict(text=text))` | `.title(text)` on the [panel](../guides/legends.md#titling-a-panel) that holds the legend or the bar |

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

## Bars and stairs

| Plot | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Upright bars | `bar` | `bar` | `Bar` | `bar` |
| Bars lying flat | `barh` | `barh` | `Bar` with `orientation="h"` | `barh` |
| Several series side by side at each position | `bar` with a matrix of values, `'grouped'` | one `bar` per series, each offset by hand | `Bar` traces with `barmode="group"` | `bars` with one `series` per series |
| Several series stacked at each position | `bar` with a matrix of values and `'stacked'` | one `bar` per series, each with `bottom=` the sum of those below | `Bar` traces with `barmode="relative"` | `bars` with one `stacked` per series, each naming the same column |
| Floating bars | `bar` with `BaseValue`, one base for all bars | `bar` with `bottom=` an array | `Bar` with `base=` an array | `bar` with `base_data` |
| One colour per bar | `bar` with `FaceColor` set to `flat` and `CData` | `bar` with `color=` a list | `Bar` with `marker.color` set to a list | `bar` with `colors` |
| Stepped outline between edges | `histogram` with `DisplayStyle` set to `stairs` | `stairs` | `Scatter` with `line_shape="hv"` through the edges | `stairs` |
| Stepped line through samples | `stairs` | `step`, or `plot` with `drawstyle="steps-post"` | `Scatter` with `line_shape="hv"`, `"vh"` or `"hvh"` | `stairs_at` with `StepAt::Post`, `StepAt::Pre` or `StepAt::Mid` |

Bars that compare series side by side or stack them are separate series in one named group, so that each series keeps its own label, colour and legend entry, as [bar charts](../guides/getting-started.md#bar-charts) describes. The positions are always numbers: categorical bars stand at 0, 1, …, N − 1 and are named by text tick labels.

## Axes

These are settings of an axis rather than plots, and in IronLAB they are methods of `AxesMut`, described in [axis direction and ticks](../guides/getting-started.md#axis-direction-and-ticks) and, for categorical axes, in [cells and categorical rows](../guides/getting-started.md#cells-and-categorical-rows).

| Setting | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Turn the tick labels of an axis | `xtickangle(a)`, `ytickangle(a)` | `tick_params(axis="x", labelrotation=a)`, or `rotation=a` with `ha="right"` and `rotation_mode="anchor"` for labels that end at their ticks | `xaxis=dict(tickangle=-a)` | `xtick_label_angle(a)`, `ytick_label_angle(a)` |
| Hide an axis with its spine, ticks, tick labels and label | `ax.XAxis.Visible = 'off'` | `ax.xaxis.set_visible(False)` together with `ax.spines["bottom"].set_visible(False)` | `xaxis=dict(visible=False)` | `xaxis_visible(false)`, `yaxis_visible(false)` |
| Hide the tick labels and keep the ticks | `xticklabels({})` | `tick_params(labelbottom=False)` | `xaxis=dict(showticklabels=False)` | `xtick_labels_hidden()` |
| Name the positions 0, 1, …, N − 1 of an axis (a categorical axis) | `xticks(0:n-1)` with `xticklabels(names)`, or a `categorical` array | `set_xticks(range(n), labels=names)` | `xaxis=dict(type="category")` | `xcategories(names)` |
| Name the rows from the top | `yticks` and `yticklabels` with `set(gca, 'YDir', 'reverse')` | `set_yticks(range(n), labels=names)` with `invert_yaxis()` | `yaxis=dict(type="category", autorange="reversed")` | `ycategories(names)` |
| Rows in groups under bold headers | no equivalent | no equivalent | no equivalent | `ycategory_groups(groups)`, with `CategoryRows::place` to arrange the data |

Every angle is in degrees anticlockwise from horizontal, as in MATLAB, so 45 gives labels that rise to the right; Plotly measures `tickangle` clockwise, so its angle has the opposite sign. IronLAB turns every label about its end nearest its tick, which matplotlib does only with the alignment options in the table, and reserves the space the turned labels take, so that no label is clipped by the page. A hidden axis also gives its space to the plot area, and its grid lines still follow `grid`.

## Plots without an equivalent

IronLAB does not yet bin samples into histograms of one or two dimensions, although it draws binned counts as bars, stairs or cells; nor does it draw stem, area or error-bar plots, patches or streamlines. The [figure schema](figure-schema.md#extending-the-model-with-new-plot-types) describes how such plot types are added.
