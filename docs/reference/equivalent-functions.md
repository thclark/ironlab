# Equivalent functions

This page maps the plotting functions of MATLAB, matplotlib and Plotly to the IronLAB function that draws the same plot. IronLAB follows MATLAB's names where MATLAB has one, so most rows differ only for readers who arrive from matplotlib or Plotly. The exceptions are the rows in which one MATLAB function covers several kinds of plot, which IronLAB separates (`image`), and those in which IronLAB draws two MATLAB plots with one kind of object (`pcolor`, which is a surface in a two-dimensional axes).

Every IronLAB function in the table is a method of `AxesMut` and is described in [getting started](../guides/getting-started.md#plotting-functions). The matplotlib names are methods of `Axes`, or of `Axes3D` where the plot is three-dimensional. The Plotly names are trace types of `plotly.graph_objects` unless they begin with `px.` (Plotly Express) or `ff.` (the figure factory).

## Lines and markers

| Plot | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Line in two dimensions | `plot` | `plot` | `Scatter` with `mode="lines"` | `plot` |
| Line in three dimensions | `plot3` | `Axes3D.plot` | `Scatter3d` with `mode="lines"` | `plot3` |
| Line on logarithmic axes | `loglog`, `semilogx`, `semilogy` | `loglog`, `semilogx`, `semilogy` | `Scatter` on an axis with `type="log"` | `loglog`, `semilogx`, `semilogy`, or `xscale`, `yscale` and `zscale` on the axes |
| Markers in two dimensions | `scatter` | `scatter` | `Scatter` with `mode="markers"` | `scatter` |
| Markers in three dimensions | `scatter3` | `Axes3D.scatter` | `Scatter3d` with `mode="markers"` | `scatter3` |

## Gridded fields

| Plot | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Isolines | `contour` | `contour` | `Contour` with `contours_coloring="lines"` | `contour` |
| Filled contours | `contourf` | `contourf` | `Contour` | `contourf` |
| Isolines raised to their levels | `contour3` | `Axes3D.contour` | no equivalent trace | `contour3` |
| Surface in three dimensions | `surf` | `Axes3D.plot_surface` | `Surface` | `surf` |
| Wireframe in three dimensions | `mesh` | `Axes3D.plot_wireframe` | no equivalent trace | `mesh` |
| Pseudocolour plot: a field on the vertices of a rectilinear or curvilinear grid | `pcolor` | `pcolormesh`, `pcolor` | `Heatmap` with `x` and `y` coordinates, on a rectilinear grid only | `surface` in a two-dimensional axes |

`surface` adds the same object as `surf` and leaves the projection of the axes as it is, so it draws a pseudocolour plot in a two-dimensional axes and a surface in a three-dimensional one. [Surfaces in two-dimensional axes](../guides/getting-started.md#surfaces-in-two-dimensional-axes) explains how such a plot differs from a colour-mapped image and from MATLAB's `pcolor`, and the [Flow past a cylinder](../gallery/cylinder_flow.md) gallery entry shows one on a polar mesh.

## Images

| Plot | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Colour-mapped image: data values scaled through the colour limits into the colormap | `imagesc`, or `imshow` with a display range | `imshow` or `matshow` with a two-dimensional array | `Heatmap`, or `px.imshow` with a two-dimensional array | `mapped_image` |
| True-colour image | `image` or `imshow` with an m × n × 3 array | `imshow` with an m × n × 3 or m × n × 4 array | `Image`, or `px.imshow` with an m × n × 3 or m × n × 4 array | `image` |
| Colour-indexed image: values that name colormap entries directly | `image` with an indexed array, or `imshow(X, map)` | `imshow` with `norm=NoNorm()` | no equivalent trace | `indexed_image` |

[Images](../guides/getting-started.md#images) describes the three kinds and how each is placed.

The options that orient and place an image in MATLAB and matplotlib have one IronLAB equivalent, the explicit pixel ranges of the image.

| Purpose | MATLAB | matplotlib | IronLAB |
| --- | --- | --- | --- |
| Draw row 0 of an n-row array at the top | `axis ij`, which `image`, `imagesc` and `imshow` apply | `origin="upper"`, the default of `imshow` | `pixel_rows(n − 1, 0)`, or any row range whose `first` is greater than its `last` |
| Draw row 0 at the bottom | `axis xy` | `origin="lower"` | the default placement, or any row range whose `first` is less than its `last` |
| Place the image in data coordinates | `XData` and `YData`, which give the centres of the outermost pixels | `extent`, which gives the outer edges of the image | `pixel_columns(first, last)` and `pixel_rows(first, last)`, which give the centres of the outermost pixels |

IronLAB has no axis-direction or image-origin property, for the reason given in [Image orientation](../guides/getting-started.md#image-orientation).

## Vector fields

| Plot | MATLAB | matplotlib | Plotly | IronLAB |
| --- | --- | --- | --- | --- |
| Arrows in two dimensions | `quiver` | `quiver` | `ff.create_quiver` | `quiver` |
| Arrows in three dimensions | `quiver3` | `Axes3D.quiver` | `Cone` | `quiver3` |

## Plots without an equivalent

IronLAB does not yet draw bar charts, histograms, stem, stairs, area or error-bar plots, patches or streamlines. The [figure schema](figure-schema.md#extending-the-model-with-new-plot-types) describes how such plot types are added.
