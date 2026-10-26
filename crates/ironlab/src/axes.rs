//! The axes handle and its plotting functions.

use ironlab_ir::{
    Artist, Aspect, AspectAdjust, Axes, Axis, Bar, CellGap, Cells, Color, ColorSpec, Contour,
    ContourPlacement, Coords, DashStyle, Dimension, Image, InsideColorbar, InsideColorbarSide,
    InsideLegend, IrError, Legend, LegendEntries, LegendEntryColumns, LegendEntryOrder,
    LegendLocation, Limits, MarkerShape, NdArray, NodeId, Orientation, Projection, Quiver,
    ScalarImage, Scale, Series, Stairs, StairsData, StepAt, Surface, Text, TickLabels, Ticks,
    TitleAlign, View3d,
};

use crate::artists::{
    BarMut, BarsMut, CellsMut, ContourMut, ImageMut, QuiverMut, ScalarImageMut, SeriesMut,
    StairsMut, SurfaceMut,
};
use crate::color_scale::{self, ColorScaleMut};
use crate::colorbar::InsideColorbarMut;
use crate::data::{DataSource, store_grid};

/// The colour of a reference line drawn by [`hline`](AxesMut::hline) or
/// [`vline`](AxesMut::vline): the dark grey of MATLAB's `xline` and `yline`, which is
/// also the colour of the axes box.
const REFERENCE_LINE_COLOR: Color = Color::rgb(0.15, 0.15, 0.15);

/// A handle to one axes of a figure, through which plots are added and axes
/// properties are set.
///
/// The handle borrows the figure mutably, so it is short-lived: hold it in a variable
/// while building one axes, and keep its [`id`](AxesMut::id) to refer to the axes
/// later, for example to link it with others.
///
/// Plotting functions follow MATLAB's names and argument order and return a handle to
/// the new plot whose setters can be chained. Property setters return the axes handle
/// so that they can be chained too.
///
/// Every data argument is a [`DataSource`]: values, which are copied into the figure so
/// that the caller's data may be dropped or changed afterwards, or the identifier of a data
/// array of the figure, which the new plot shares (see [`Figure::data`](crate::Figure::data)).
/// Plotting functions that need three dimensions (`plot3`,
/// `scatter3`, `contour3`, `quiver3`, `surf` and `mesh`) convert a two-dimensional
/// axes to three dimensions with the default view, as MATLAB does; so does placing an
/// image on a wall of the axes with [`plane`](ImageMut::plane). [`surface`](AxesMut::surface)
/// and the image functions leave the axes as it is.
///
/// ```
/// use ironlab::prelude::*;
///
/// let x = linspace(0.0, 1.0, 11);
/// let y: Vec<f64> = x.iter().map(|x| x * x).collect();
///
/// let mut fig = Figure::new();
/// let mut ax = fig.axes(0, 0);
/// ax.plot(&x, &y).marker(Marker::Circle).label("$x^2$");
/// ax.title("A parabola").xlabel("$x$").ylabel("$y$").grid(true);
/// ```
#[derive(Debug)]
pub struct AxesMut<'f> {
    pub(crate) fig: &'f mut ironlab_ir::Figure,
    pub(crate) id: NodeId,
}

impl<'f> AxesMut<'f> {
    /// Creates a handle to the axes with the given identifier, which must be an axes
    /// of the figure.
    pub(crate) fn new(fig: &'f mut ironlab_ir::Figure, id: NodeId) -> Self {
        Self { fig, id }
    }
}

impl AxesMut<'_> {
    /// Returns the node identifier of the axes.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    // Series.

    /// Plots a line through the points `(x[i], y[i])` (MATLAB's `plot`).
    ///
    /// The series is a solid line 0.75 pt wide without markers; its colour is the next
    /// colour of the axes colour order, chosen when the figure is drawn. The returned
    /// [`SeriesMut`] adds markers, colours the series by data and sets every other
    /// property of the series.
    pub fn plot(&mut self, x: impl Into<DataSource>, y: impl Into<DataSource>) -> SeriesMut<'_> {
        self.add_series(x.into(), y.into(), None)
    }

    /// Plots a line through the points `(x[i], y[i], z[i])` in three dimensions
    /// (MATLAB's `plot3`), converting the axes to three dimensions.
    pub fn plot3(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
        z: impl Into<DataSource>,
    ) -> SeriesMut<'_> {
        self.make_3d();
        self.add_series(x.into(), y.into(), Some(z.into()))
    }

    /// Plots a line with logarithmic x and y axes (MATLAB's `loglog`).
    ///
    /// Points with a non-positive coordinate are not drawn, and
    /// [`Figure::validate`](crate::Figure::validate) warns about them.
    pub fn loglog(&mut self, x: impl Into<DataSource>, y: impl Into<DataSource>) -> SeriesMut<'_> {
        self.set_xy_scales(Scale::Log, Scale::Log);
        self.add_series(x.into(), y.into(), None)
    }

    /// Plots a line with a logarithmic x axis and a linear y axis (MATLAB's
    /// `semilogx`).
    pub fn semilogx(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
    ) -> SeriesMut<'_> {
        self.set_xy_scales(Scale::Log, Scale::Linear);
        self.add_series(x.into(), y.into(), None)
    }

    /// Plots a line with a linear x axis and a logarithmic y axis (MATLAB's
    /// `semilogy`).
    pub fn semilogy(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
    ) -> SeriesMut<'_> {
        self.set_xy_scales(Scale::Linear, Scale::Log);
        self.add_series(x.into(), y.into(), None)
    }

    // Reference lines.

    /// Draws a horizontal reference line across the whole width of the plot area at
    /// `y` (Matplotlib's `axhline`, MATLAB's `yline`).
    ///
    /// The line is a series whose x values are the fractions 0 and 1 of the plot
    /// area ([`Coords::AxesFraction`]), so it spans the plot area at any limits, pan or
    /// zoom, and whose y values are the data value `y`. The x fractions take no part in
    /// the automatic x limits, but `y` takes part in the automatic y limits, so the line
    /// is always in view unless the y limits are fixed. It is solid, 0.75 pt wide and
    /// dark grey, and takes no colour from the axes colour order; the usual line setters
    /// ([`dash`](SeriesMut::dash), [`color`](SeriesMut::color),
    /// [`line_width`](SeriesMut::line_width)) change its look. A reference line is valid
    /// only in a two-dimensional axes.
    pub fn hline(&mut self, y: f64) -> SeriesMut<'_> {
        let mut line = self.add_series([0.0, 1.0].into(), [y, y].into(), None);
        line.x_coords(Coords::AxesFraction)
            .color(REFERENCE_LINE_COLOR);
        line
    }

    /// Draws a vertical reference line across the whole height of the plot area at
    /// `x` (Matplotlib's `axvline`, MATLAB's `xline`).
    ///
    /// The line spans the plot area as [`hline`](AxesMut::hline) does, with the roles of
    /// x and y exchanged: its y values are the fractions 0 and 1 of the plot area and
    /// its x values the data value `x`, which takes part in the automatic x limits.
    pub fn vline(&mut self, x: f64) -> SeriesMut<'_> {
        let mut line = self.add_series([x, x].into(), [0.0, 1.0].into(), None);
        line.y_coords(Coords::AxesFraction)
            .color(REFERENCE_LINE_COLOR);
        line
    }

    /// Draws a marker at each point `(x[i], y[i])` (MATLAB's `scatter`).
    ///
    /// The series is unfilled 4 pt circles without a line, in the next colour of the
    /// axes colour order. It differs from a series drawn by [`plot`](AxesMut::plot) only
    /// in these defaults: [`dash`](SeriesMut::dash) joins the circles by a line, and
    /// [`size_data`](SeriesMut::size_data) and [`color_data`](SeriesMut::color_data) size
    /// and colour each marker by data.
    pub fn scatter(&mut self, x: impl Into<DataSource>, y: impl Into<DataSource>) -> SeriesMut<'_> {
        let mut series = self.add_series(x.into(), y.into(), None);
        series.dash(DashStyle::None).marker(MarkerShape::Circle);
        series
    }

    /// Draws a marker at each point `(x[i], y[i], z[i])` in three dimensions
    /// (MATLAB's `scatter3`), converting the axes to three dimensions.
    pub fn scatter3(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
        z: impl Into<DataSource>,
    ) -> SeriesMut<'_> {
        self.make_3d();
        let mut series = self.add_series(x.into(), y.into(), Some(z.into()));
        series.dash(DashStyle::None).marker(MarkerShape::Circle);
        series
    }

    // Bars and stairs.

    /// Draws a bar at each position, rising from 0 for the length of its value (MATLAB's
    /// and Matplotlib's `bar`).
    ///
    /// The bars stand upright in the next colour of the axes colour order, outlined by a
    /// thin black line, each four fifths of the pitch wide, the pitch being the smallest
    /// distance between neighbouring positions, and centred on its position. A negative
    /// value hangs down from the base. The automatic limits leave a margin of 0.05 of the
    /// span of the bars beyond their ends. The returned [`BarMut`] sets the width, the
    /// offset, the base, the margin and the colours; [`bars`](Self::bars) writes grouped
    /// and stacked bars. Categorical bars are bars at 0, 1, …, N − 1, named by
    /// [`xticks`](Self::xticks) and [`xtick_labels`](Self::xtick_labels).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// ax.bar([0.0, 1.0, 2.0], [5.0, 3.0, 4.0]);
    /// ax.xticks([0.0, 1.0, 2.0]).xtick_labels(["North", "South", "East"]);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn bar(
        &mut self,
        positions: impl Into<DataSource>,
        values: impl Into<DataSource>,
    ) -> BarMut<'_> {
        self.add_bar(positions.into(), values.into(), Orientation::Vertical)
    }

    /// Draws a bar lying flat at each position along y, extending from 0 for the length of
    /// its value along x (MATLAB's and Matplotlib's `barh`), which is how long category
    /// names are made readable; it differs from [`bar`](Self::bar) only in its
    /// orientation.
    pub fn barh(
        &mut self,
        positions: impl Into<DataSource>,
        values: impl Into<DataSource>,
    ) -> BarMut<'_> {
        self.add_bar(positions.into(), values.into(), Orientation::Horizontal)
    }

    /// Returns the helper that draws grouped and stacked bars at the given positions
    /// (MATLAB's `bar(x, Y)` and `bar(x, Y, 'stacked')`, Matplotlib's grouped and stacked
    /// bars), each series an ordinary bar series whose width, offset and base the helper
    /// writes; see [`BarsMut`].
    pub fn bars(&mut self, positions: impl Into<DataSource>) -> BarsMut<'_> {
        BarsMut::new(self.fig, self.id, positions.into())
    }

    /// Draws a stepped outline holding each value between two neighbouring edges, of
    /// which there is one more than values (Matplotlib's `stairs`), as the outline of a
    /// histogram is drawn.
    ///
    /// The outline is open and unfilled, in the next colour of the axes colour order; the
    /// returned [`StairsMut`] gives it a base, to which it drops at both ends, and a fill.
    pub fn stairs(
        &mut self,
        edges: impl Into<DataSource>,
        values: impl Into<DataSource>,
    ) -> StairsMut<'_> {
        let edges = edges.into().store(self.fig);
        let values = values.into().store(self.fig);
        self.add_stairs(StairsData::Edges { edges, values })
    }

    /// Draws a stepped outline through values held at samples (MATLAB's `stairs` and
    /// Matplotlib's `step`), whose steps occur where `step` places them: after each sample
    /// ([`StepAt::Post`], as MATLAB's `stairs` draws), before it ([`StepAt::Pre`]), or
    /// midway between samples ([`StepAt::Mid`]).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let t = [0.0, 1.0, 2.0, 4.0];
    /// let level = [1.0, 3.0, 2.0, 5.0];
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0).stairs_at(t, level, StepAt::Post).label("Set point");
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn stairs_at(
        &mut self,
        x: impl Into<DataSource>,
        values: impl Into<DataSource>,
        step: StepAt,
    ) -> StairsMut<'_> {
        let x = x.into().store(self.fig);
        let values = values.into().store(self.fig);
        self.add_stairs(StairsData::Samples { x, values, step })
    }

    // Contour plots.

    /// Draws isolines of the field `z` sampled on a grid (MATLAB's `contour`).
    ///
    /// The field has one row per y coordinate and one column per x coordinate; see
    /// [`DataSource`] for the accepted forms of `x`, `y` and `z`. The levels are those of
    /// the colour scale (the band edges of a discrete scale, and ten automatic levels
    /// otherwise), and each isoline is coloured by its level through the default colour
    /// scale of the axes (see [`color_scale`](AxesMut::color_scale)).
    ///
    /// The coordinates `x` and `y` describe the grid, here and in every other gridded
    /// plot ([`contourf`](AxesMut::contourf), [`contour3`](AxesMut::contour3),
    /// [`surf`](AxesMut::surf), [`mesh`](AxesMut::mesh) and
    /// [`surface`](AxesMut::surface)). Two vectors give one coordinate per column (x) and
    /// per row (y) of an axis-aligned, rectilinear grid, which is the preferred form
    /// because it is stored once per dimension rather than once per node. When either is a
    /// matrix of the field's shape, the grid is curvilinear and each matrix gives the
    /// coordinate of every node. A vector of values mixed with a matrix is then repeated
    /// into a matrix of the field's shape, as [`meshgrid`](crate::meshgrid) would, when its
    /// length matches the field along its dimension; a vector of any other length, and a
    /// vector given by identifier, is stored as given, so that
    /// [`Figure::validate`](crate::Figure::validate) reports the mismatch. The rank of an
    /// array given by identifier is read from the figure.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(-1.0, 1.0, 21);
    /// let y = linspace(-1.0, 1.0, 21);
    /// let z = Matrix::from_fn(y.len(), x.len(), |row, col| x[col].hypot(y[row]));
    ///
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0).contour(&x, &y, &z).levels(5);
    /// ```
    pub fn contour(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
        z: impl Into<DataSource>,
    ) -> ContourMut<'_> {
        let placement = ContourPlacement::default();
        self.add_contour(x.into(), y.into(), z.into(), false, placement)
    }

    /// Fills the bands between levels of the field `z` sampled on a grid (MATLAB's
    /// `contourf`).
    pub fn contourf(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
        z: impl Into<DataSource>,
    ) -> ContourMut<'_> {
        let placement = ContourPlacement::default();
        self.add_contour(x.into(), y.into(), z.into(), true, placement)
    }

    /// Draws each isoline of the field `z` at the height of its level (MATLAB's
    /// `contour3`), converting the axes to three dimensions.
    pub fn contour3(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
        z: impl Into<DataSource>,
    ) -> ContourMut<'_> {
        self.make_3d();
        self.add_contour(
            x.into(),
            y.into(),
            z.into(),
            false,
            ContourPlacement::AtLevel,
        )
    }

    // Vector fields.

    /// Draws an arrow with components `(u[i], v[i])` at each point `(x[i], y[i])`
    /// (MATLAB's `quiver`).
    ///
    /// Arrows are scaled automatically so that they do not overlap; see
    /// [`QuiverMut::scale`] and [`QuiverMut::no_scale`].
    pub fn quiver(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
        u: impl Into<DataSource>,
        v: impl Into<DataSource>,
    ) -> QuiverMut<'_> {
        let x = x.into().store(self.fig);
        let y = y.into().store(self.fig);
        let u = u.into().store(self.fig);
        let v = v.into().store(self.fig);
        let id = self.push_artist(|id| {
            Artist::Quiver(Quiver {
                id,
                x,
                y,
                u,
                v,
                ..Quiver::default()
            })
        });
        QuiverMut::new(self.fig, id)
    }

    /// Draws an arrow with components `(u[i], v[i], w[i])` at each point
    /// `(x[i], y[i], z[i])` (MATLAB's `quiver3`), converting the axes to three
    /// dimensions.
    pub fn quiver3(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
        z: impl Into<DataSource>,
        u: impl Into<DataSource>,
        v: impl Into<DataSource>,
        w: impl Into<DataSource>,
    ) -> QuiverMut<'_> {
        self.make_3d();
        let x = x.into().store(self.fig);
        let y = y.into().store(self.fig);
        let z = z.into().store(self.fig);
        let u = u.into().store(self.fig);
        let v = v.into().store(self.fig);
        let w = w.into().store(self.fig);
        let id = self.push_artist(|id| {
            Artist::Quiver(Quiver {
                id,
                x,
                y,
                z: Some(z),
                u,
                v,
                w: Some(w),
                ..Quiver::default()
            })
        });
        QuiverMut::new(self.fig, id)
    }

    // Surfaces.

    /// Draws the surface of heights `z` over a grid (MATLAB's `surf`), converting the
    /// axes to three dimensions.
    ///
    /// Faces are coloured by height through the default colour scale of the axes (see
    /// [`color_scale`](AxesMut::color_scale)) and outlined by thin black edges. [`surface`](AxesMut::surface) adds the same surface without converting the
    /// axes.
    pub fn surf(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
        z: impl Into<DataSource>,
    ) -> SurfaceMut<'_> {
        self.make_3d();
        let defaults = Surface::default();
        self.add_surface(x.into(), y.into(), z.into(), defaults.face, defaults.edge)
    }

    /// Draws the surface of heights `z` over a grid as a wireframe (MATLAB's `mesh`),
    /// converting the axes to three dimensions.
    ///
    /// Faces are filled with the figure background colour, so that they hide the edges
    /// behind them, and edges are coloured by height through the default colour scale of
    /// the axes.
    pub fn mesh(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
        z: impl Into<DataSource>,
    ) -> SurfaceMut<'_> {
        self.make_3d();
        let face = ColorSpec::Rgba {
            color: self.fig.background,
        };
        self.add_surface(x.into(), y.into(), z.into(), face, ColorSpec::Colormapped)
    }

    /// Draws the surface of the field `z` over a grid and leaves the axes as it is: a
    /// two-dimensional axes stays two-dimensional, and a three-dimensional axes keeps
    /// its view.
    ///
    /// In a two-dimensional axes the surface is seen from directly above, which makes
    /// it a pseudocolour plot: the equivalent of MATLAB's `pcolor`, of matplotlib's
    /// `pcolormesh` and `pcolor`, and of Plotly's `Heatmap` with vertex coordinates. The
    /// grid alone places the faces, and `z` positions nothing but still colours the
    /// faces unless [`color_data`](SurfaceMut::color_data) is given. In a
    /// three-dimensional axes the surface is drawn exactly as [`surf`](AxesMut::surf)
    /// draws it, with `z` as the height of every node.
    ///
    /// The values of `z` belong to the vertices of the grid, not to its faces. A field
    /// of n rows and m columns therefore draws (n − 1) × (m − 1) faces, and a field with
    /// a single row or a single column draws nothing. Each face takes the colour of the
    /// mean of the values at its four corners. MATLAB's `pcolor` with its default flat
    /// shading instead gives each face the value of its first corner and never shows
    /// the last row and column of the field.
    ///
    /// The grid is given as for `surf`: two vectors make a rectilinear grid, and two
    /// matrices of the shape of `z` make a curvilinear grid, on which every vertex has
    /// its own position. Because a surface is made of vertices, it can be drawn on
    /// logarithmic axes, which an image cannot. Faces are coloured through the colour
    /// scale and outlined by thin black edges, as for `surf`; `edge_color(None)` removes the
    /// edges, which suits a fine grid (MATLAB's `shading flat`).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// // A field on a polar mesh, given as coordinate matrices.
    /// let (rings, spokes) = (12, 48);
    /// let radius = |row: usize| 1.0 + row as f64 / (rings - 1) as f64;
    /// let angle = |col: usize| std::f64::consts::TAU * col as f64 / (spokes - 1) as f64;
    /// let x = Matrix::from_fn(rings, spokes, |row, col| radius(row) * angle(col).cos());
    /// let y = Matrix::from_fn(rings, spokes, |row, col| radius(row) * angle(col).sin());
    /// let field = Matrix::from_fn(rings, spokes, |row, col| radius(row) * angle(col).sin());
    ///
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0).surface(x, y, &field).edge_color(None);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn surface(
        &mut self,
        x: impl Into<DataSource>,
        y: impl Into<DataSource>,
        z: impl Into<DataSource>,
    ) -> SurfaceMut<'_> {
        let defaults = Surface::default();
        self.add_surface(x.into(), y.into(), z.into(), defaults.face, defaults.edge)
    }

    // Images.

    /// Draws a true-colour image: a raster of pixels, each with its own colour (MATLAB's
    /// `image` with a true-colour array).
    ///
    /// The image lies in the xy plane of the axes with the centres of its pixels at 0,
    /// 1, …, n − 1 along each axis, so that an image of `nx` columns covers −0.5 to
    /// nx − 0.5, and row 0 of the pixels lies at y = 0.
    /// [`pixel_columns`](ImageMut::pixel_columns) and [`pixel_rows`](ImageMut::pixel_rows)
    /// place the pixel centres over other coordinates, and [`plane`](ImageMut::plane)
    /// puts the image on the floor or a wall of a three-dimensional axes. The axes is
    /// left as it is: a two-dimensional axes stays two-dimensional. An axis of the
    /// image's plane must not be logarithmic: a raster of evenly spaced pixels has no
    /// placement on a logarithmic axis, so validation refuses the figure with an
    /// `ImageOnLogAxis` error, and values sampled along such an axis are drawn with
    /// [`surface`](AxesMut::surface) instead.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let pixels = Pixels::rgb_from_fn(32, 32, |row, col| {
    ///     Color::rgb(row as f32 / 31.0, col as f32 / 31.0, 0.5)
    /// });
    ///
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0).image(&pixels).pixel_columns(0.0, 1.0).pixel_rows(1.0, 0.0);
    /// ```
    pub fn image(&mut self, pixels: impl Into<DataSource>) -> ImageMut<'_> {
        let pixels = pixels.into().store(self.fig);
        let id = self.push_artist(|id| {
            Artist::Image(Image {
                id,
                pixels,
                ..Image::default()
            })
        });
        ImageMut::new(self.fig, id)
    }

    /// Draws a scalar image: a raster of numbers, each mapped to a colour through a colour
    /// scale (MATLAB's `imagesc`, and `image` with an indexed array).
    ///
    /// The image names the default colour scale of the axes, which is created if the
    /// axes has none (see [`color_scale`](AxesMut::color_scale)), so it shares that scale
    /// with the other colour-mapped artists of the axes, as `imagesc` shares the axes'
    /// colormap and colour limits; [`ScalarImageMut::color_scale`] gives it another. A
    /// continuous scale maps the values between its limits, as `imagesc` does, and an
    /// indexed scale makes each whole number `i` take entry `i` of its palette, as
    /// `image` with an indexed array does. What a value outside the scale draws is the
    /// scale's policy: see [`ColorScaleMut::below`], [`ColorScaleMut::outside`] and
    /// [`ColorScaleMut::non_finite`].
    ///
    /// The values are taken from a [`Matrix`](crate::Matrix), stored as floating-point
    /// values, or a [`ByteMatrix`](crate::ByteMatrix), stored as bytes, by value or by
    /// reference, and the image is placed as by [`image`](AxesMut::image).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(-2.0, 2.0, 81);
    /// let y = linspace(-1.0, 1.0, 41);
    /// let field = Matrix::from_fn(y.len(), x.len(), |row, col| (x[col] * y[row]).sin());
    ///
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// ax.scalar_image(&field).pixel_columns(-2.0, 2.0).pixel_rows(-1.0, 1.0);
    /// ax.color_scale()
    ///     .colormap(Colormap::Magma)
    ///     .limits(-1.0, 1.0)
    ///     .non_finite(Color::BLACK);
    /// assert!(fig.validate().is_valid());
    /// ```
    ///
    /// An indexed image whose bytes name the entries of a 256-colour palette:
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let classes = ByteMatrix::from_fn(8, 8, |row, col| ((row + col) % 4 * 85) as u8);
    ///
    /// let mut fig = Figure::new();
    /// let palette = fig.indexed_scale(Colormap::Viridis.sample(256)).id();
    /// fig.axes(0, 0).scalar_image(&classes).color_scale(palette);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn scalar_image(&mut self, values: impl Into<DataSource>) -> ScalarImageMut<'_> {
        let values = values.into().store(self.fig);
        let (scale, created) = self.default_scale();
        let id = self.push_artist(|id| {
            Artist::ScalarImage(ScalarImage {
                id,
                values,
                color_scale: Some(scale),
                ..ScalarImage::default()
            })
        });
        ScalarImageMut::new(self.fig, id, created.then_some(scale))
    }

    // Cells and categorical axes.

    /// Draws a grid of cells, each coloured by its value through a colour scale (MATLAB's
    /// `heatmap`, and Matplotlib's `pcolormesh` given the edges of its cells).
    ///
    /// `values` is a matrix of `ny` rows and `nx` columns, a [`Matrix`](crate::Matrix) of
    /// numbers or a [`ByteMatrix`](crate::ByteMatrix) of whole numbers such as the indices
    /// of categories, or the identifier of such an array in the figure. Cell `(j, i)` holds
    /// the value in row `j` and column `i`; without explicit edges it is centred on
    /// `(i, j)`. The cells are seamless and have no outlines, as a heatmap does, and are
    /// mapped through the default colour scale of the axes (see
    /// [`color_scale`](AxesMut::color_scale)), which is created when the axes has none;
    /// [`CellsMut::color_scale`] names another, such as a categorical scale whose
    /// categories the values name. A value that the scale cannot place, NaN among them,
    /// follows the scale's out-of-range policy, which by default draws nothing for NaN.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let z = Matrix::from_fn(4, 6, |row, col| (row * col) as f64);
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// ax.cells(&z).edge_color(Color::WHITE);
    /// ax.color_scale().colormap(Colormap::Cividis);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn cells(&mut self, values: impl Into<DataSource>) -> CellsMut<'_> {
        let values = values.into().store(self.fig);
        self.add_cells(values, CellGap::default())
    }

    /// Draws an occupancy matrix: one row of cells per given row, each cell coloured by
    /// its value, usually the index of a category of a categorical colour scale.
    ///
    /// The rows may be of different lengths, as the branches of the subroutines of a
    /// program are; each is padded with NaN at its end to the length of the longest, and
    /// NaN draws nothing under the default out-of-range policy, so that a short row ends
    /// where its data ends. The cells lie on unit edges, cell `i` of row `j` centred on
    /// `(i, j)`, with a gap of 0.2 along x and y that sets each cell apart, and the x axis
    /// is hidden, because the position of a cell in its row names nothing.
    ///
    /// The rows are named by [`ycategories`](AxesMut::ycategories), whose names are given
    /// in the order of the rows, or by [`ycategory_groups`](AxesMut::ycategory_groups),
    /// whose [`CategoryRows::place`] arranges the data of each member in the rows that it
    /// lays out, header and blank rows included.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// let status = fig
    ///     .categorical_scale([
    ///         (Color::rgb(0.0, 0.6, 0.3), "taken"),
    ///         (Color::rgb(0.8, 0.2, 0.1), "not taken"),
    ///     ])
    ///     .id();
    /// let mut ax = fig.axes(0, 0);
    /// ax.occupancy([vec![0.0, 1.0, 0.0], vec![1.0], vec![0.0, 0.0]])
    ///     .color_scale(status);
    /// ax.ycategories([r"\texttt{psilin}", r"\texttt{ggcalc}", r"\texttt{blsolv}"]);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn occupancy<R: AsRef<[f64]>>(
        &mut self,
        rows: impl IntoIterator<Item = R>,
    ) -> CellsMut<'_> {
        let rows: Vec<R> = rows.into_iter().collect();
        let nx = rows.iter().map(|row| row.as_ref().len()).max().unwrap_or(0);
        let mut values = Vec::with_capacity(rows.len() * nx);
        for row in &rows {
            let row = row.as_ref();
            values.extend_from_slice(row);
            values.extend(std::iter::repeat_n(f64::NAN, nx - row.len()));
        }
        let array = NdArray::from_shape(vec![rows.len(), nx], values)
            .expect("the padded rows fill the shape");
        let values = DataSource::from(array).store(self.fig);
        self.axes().x.visible = false;
        self.add_cells(values, CellGap { x: 0.2, y: 0.2 })
    }

    /// Makes the x axis categorical: a tick at 0, 1, …, N − 1, labelled with the given
    /// names in order, which is where [`cells`](AxesMut::cells) on unit edges and bars at
    /// whole positions lie. The first category is at the left.
    ///
    /// The names are LaTeX, as every other text of the figure is, so that
    /// `\texttt{…}` sets a name of code in monospace. The limits are not changed.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0)
    ///     .xcategories([r"\texttt{case_a}", r"\texttt{case_b}"])
    ///     .xtick_label_angle(90.0);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn xcategories<T: Into<Text>>(&mut self, names: impl IntoIterator<Item = T>) -> &mut Self {
        let labels: Vec<Text> = names.into_iter().map(Into::into).collect();
        let axis = &mut self.axes().x;
        axis.ticks = Ticks::Values((0..labels.len()).map(|k| k as f64).collect());
        axis.tick_labels = TickLabels::Text(labels);
        self
    }

    /// Makes the y axis categorical, as [`xcategories`](AxesMut::xcategories) makes the x
    /// axis, and reverses it, so that the first category is at the top, as the rows of a
    /// table are read.
    pub fn ycategories<T: Into<Text>>(&mut self, names: impl IntoIterator<Item = T>) -> &mut Self {
        let labels: Vec<Text> = names.into_iter().map(Into::into).collect();
        let axis = &mut self.axes().y;
        axis.ticks = Ticks::Values((0..labels.len()).map(|k| k as f64).collect());
        axis.tick_labels = TickLabels::Text(labels);
        axis.reverse = true;
        self
    }

    /// Makes the y axis categorical with its rows in groups, and returns where each row
    /// lies, so that data can be placed beside its name.
    ///
    /// From the top, each group takes a header row, labelled with the name of the group
    /// in bold, then one row per member, labelled with its name, and consecutive groups
    /// are separated by one blank row, which has no tick. A header keeps its tick mark,
    /// because an axis draws a tick label only beside a tick. Rows are numbered from 0 at
    /// the top, and the y axis is reversed, as [`ycategories`](AxesMut::ycategories)
    /// reverses it. The limits are not changed.
    ///
    /// The names of the groups and of the members are LaTeX source, like every other text
    /// of the figure: a header is written `\textbf{group}` around the name as given,
    /// without escaping it, so that a name may itself hold LaTeX, and a character that
    /// LaTeX reserves must be escaped by the caller.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// let rows = ax.ycategory_groups([
    ///     ("xpanel.f", vec!["psilin", "ggcalc"]),
    ///     ("xbl.f", vec!["blsolv"]),
    /// ]);
    /// // 0 xpanel.f, 1 psilin, 2 ggcalc, 3 blank, 4 xbl.f, 5 blsolv.
    /// assert_eq!(rows.len(), 6);
    /// assert_eq!(rows.row("xbl.f", "blsolv"), Some(5));
    /// ax.occupancy(rows.place(&[vec![0.0, 1.0], vec![1.0], vec![0.0, 0.0, 1.0]]));
    /// ```
    pub fn ycategory_groups<G, M, N>(
        &mut self,
        groups: impl IntoIterator<Item = (G, M)>,
    ) -> CategoryRows
    where
        G: AsRef<str>,
        M: IntoIterator<Item = N>,
        N: AsRef<str>,
    {
        let mut rows = CategoryRows::default();
        let mut ticks = Vec::new();
        let mut labels = Vec::new();
        for (group, members) in groups {
            if !rows.groups.is_empty() {
                rows.len += 1;
            }
            let name = group.as_ref().to_owned();
            let header = rows.len;
            ticks.push(header as f64);
            labels.push(Text::new(format!(r"\textbf{{{name}}}")));
            rows.len += 1;
            let mut placed = Vec::new();
            for member in members {
                let member = member.as_ref().to_owned();
                ticks.push(rows.len as f64);
                labels.push(Text::new(member.clone()));
                placed.push((member, rows.len));
                rows.len += 1;
            }
            rows.groups.push(CategoryGroup {
                name,
                header,
                members: placed,
            });
        }
        let axis = &mut self.axes().y;
        axis.ticks = Ticks::Values(ticks);
        axis.tick_labels = TickLabels::Text(labels);
        axis.reverse = true;
        rows
    }

    // Colour scales.

    /// Returns the default colour scale of the axes: the scale that its colour-mapped
    /// artists share unless they are given another.
    ///
    /// The default scale is the scale of the first artist of the axes, in the order the
    /// artists were added, that names one. Every plotting call that maps numbers to
    /// colours ([`contour`](AxesMut::contour), [`contourf`](AxesMut::contourf),
    /// [`contour3`](AxesMut::contour3), [`surf`](AxesMut::surf), [`mesh`](AxesMut::mesh),
    /// [`surface`](AxesMut::surface), [`scalar_image`](AxesMut::scalar_image) and
    /// [`SeriesMut::color_data`](crate::SeriesMut::color_data)) names it, creating a
    /// continuous scale with automatic limits when the axes has none. This reproduces
    /// MATLAB's axes colormap and colour limits, which every colour-mapped plot of an axes
    /// shares.
    ///
    /// The rule reads only the figure, so a figure loaded from a file or wrapped with
    /// [`Figure::from_ir`](crate::Figure::from_ir) keeps adding artists to the same
    /// scale. It follows that a scale returned here before any colour-mapped artist has
    /// been plotted is new and is not remembered: configure the default scale after the
    /// first colour-mapped plot, or create a scale with
    /// [`Figure::color_scale`](crate::Figure::color_scale) and give it to the first plot
    /// with its `color_scale` setter, after which it is the default.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(-1.0, 1.0, 21);
    /// let z = Matrix::from_fn(x.len(), x.len(), |row, col| x[col].hypot(x[row]));
    ///
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// ax.contourf(&x, &x, &z);
    /// ax.contour(&x, &x, &z).color(Color::BLACK);
    /// ax.color_scale().colormap(Colormap::Cividis).limits(0.0, 1.5);
    /// assert_eq!(fig.ir().color_scales.len(), 1);
    /// ```
    pub fn color_scale(&mut self) -> ColorScaleMut<'_> {
        let (scale, _) = self.default_scale();
        ColorScaleMut::new(self.fig, scale)
    }

    // Axes properties.

    /// Sets the title drawn above the axes.
    pub fn title(&mut self, title: impl Into<Text>) -> &mut Self {
        self.axes().title = Some(title.into());
        self
    }

    /// Sets how the title is aligned with the plot area: centred over it (the default),
    /// or with its left or right edge on the left or right edge of the plot area, rather
    /// than of the tile.
    ///
    /// A long title that names the varied and the fixed parameters of a panel reads best
    /// aligned on the left.
    pub fn title_align(&mut self, align: TitleAlign) -> &mut Self {
        self.axes().title_align = align;
        self
    }

    /// Fixes the ratios of the length of one data unit of y, and of z, to that of one data
    /// unit of x (MATLAB's `daspect` and `axis equal`; matplotlib's `set_aspect` and
    /// `set_box_aspect`).
    ///
    /// [`Aspect::Equal`] draws one unit at the same length along every axis, so that
    /// geometry such as an aerofoil section or a sphere keeps its shape;
    /// [`Aspect::ratio(y)`](Aspect::ratio) draws one unit of y `y` times as long as one unit
    /// of x, and `Aspect::Ratio { y, z }` also one unit of z `z` times as long, which a 2D
    /// axes ignores. On a logarithmic axis the unit is one decade. The aspect is kept by the
    /// layout, by export and while the figure is panned, zoomed and rotated in a viewer, in
    /// the way [`aspect_adjust`](AxesMut::aspect_adjust) chooses.
    /// [`Figure::validate`](crate::Figure::validate) reports a ratio that is not a finite
    /// positive number.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let t = linspace(0.0, std::f64::consts::TAU, 65);
    /// let x: Vec<f64> = t.iter().map(|t| t.cos()).collect();
    /// let y: Vec<f64> = t.iter().map(|t| t.sin()).collect();
    ///
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// ax.plot(&x, &y);
    /// ax.aspect(Aspect::Equal).aspect_adjust(AspectAdjust::Limits);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn aspect(&mut self, aspect: Aspect) -> &mut Self {
        self.axes().aspect = aspect;
        self
    }

    /// Sets how a fixed [`aspect`](AxesMut::aspect) is kept.
    ///
    /// [`AspectAdjust::Box`], the default, keeps the limits exactly as they are, manual or
    /// automatic: a 2D axes shrinks its plot area along one dimension and centres it in the
    /// space the layout gives it, and a 3D axes gives the sides of its box the lengths of
    /// the scaled ranges. [`AspectAdjust::Limits`] keeps the plot area, or the cube of a 3D
    /// axes, and widens the limits of the axes that are too short symmetrically about their
    /// centres, so that manual limits are the least range shown rather than the exact one.
    pub fn aspect_adjust(&mut self, adjust: AspectAdjust) -> &mut Self {
        self.axes().aspect_adjust = adjust;
        self
    }

    /// Sets the label of the x axis.
    pub fn xlabel(&mut self, label: impl Into<Text>) -> &mut Self {
        self.axes().x.label = Some(label.into());
        self
    }

    /// Sets the label of the y axis.
    pub fn ylabel(&mut self, label: impl Into<Text>) -> &mut Self {
        self.axes().y.label = Some(label.into());
        self
    }

    /// Sets the label of the z axis, which is drawn only by three-dimensional axes.
    pub fn zlabel(&mut self, label: impl Into<Text>) -> &mut Self {
        self.axes().z.label = Some(label.into());
        self
    }

    /// Fixes the range of the x axis, and of every axes whose x limits are linked
    /// with this one.
    ///
    /// Limits that are not finite or not increasing are stored on this axes only and
    /// reported by [`Figure::validate`](crate::Figure::validate).
    pub fn xlim(&mut self, min: f64, max: f64) -> &mut Self {
        self.set_limits(Dimension::X, min, max)
    }

    /// Fixes the range of the y axis, and of every axes whose y limits are linked
    /// with this one.
    ///
    /// Invalid limits are handled as by [`xlim`](AxesMut::xlim).
    pub fn ylim(&mut self, min: f64, max: f64) -> &mut Self {
        self.set_limits(Dimension::Y, min, max)
    }

    /// Fixes the range of the z axis, and of every axes whose z limits are linked
    /// with this one.
    ///
    /// Invalid limits are handled as by [`xlim`](AxesMut::xlim).
    pub fn zlim(&mut self, min: f64, max: f64) -> &mut Self {
        self.set_limits(Dimension::Z, min, max)
    }

    /// Sets whether the x axis is linear or logarithmic.
    pub fn xscale(&mut self, scale: Scale) -> &mut Self {
        self.axes().x.scale = scale;
        self
    }

    /// Sets whether the y axis is linear or logarithmic.
    pub fn yscale(&mut self, scale: Scale) -> &mut Self {
        self.axes().y.scale = scale;
        self
    }

    /// Sets whether the z axis is linear or logarithmic.
    pub fn zscale(&mut self, scale: Scale) -> &mut Self {
        self.axes().z.scale = scale;
        self
    }

    /// Sets whether the x axis is reversed, running from its upper limit at the left
    /// to its lower limit at the right (MATLAB's `XDir` set to `reverse`).
    ///
    /// The limits keep their meaning, with the lower limit first; only the direction
    /// in which they are drawn changes. Axes linked along x share limits only, so
    /// reversing this axis leaves the axes linked with it as they are.
    pub fn xreverse(&mut self, on: bool) -> &mut Self {
        self.axes().x.reverse = on;
        self
    }

    /// Sets whether the y axis is reversed, running from its upper limit at the bottom
    /// to its lower limit at the top (MATLAB's `YDir` set to `reverse`).
    ///
    /// A pressure distribution is drawn this way, with suction (negative values)
    /// upwards; see [`xreverse`](AxesMut::xreverse) for how a reversed axis relates to
    /// the axes linked with it.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(0.0, 1.0, 11);
    /// let cp: Vec<f64> = x.iter().map(|x| 1.0 - 4.0 * x * (1.0 - x)).collect();
    ///
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// ax.plot(&x, &cp);
    /// ax.ylabel("$C_p$").yreverse(true);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn yreverse(&mut self, on: bool) -> &mut Self {
        self.axes().y.reverse = on;
        self
    }

    /// Sets whether the z axis is reversed, running from its upper limit at the bottom
    /// of the box to its lower limit at the top; drawn only by three-dimensional axes.
    pub fn zreverse(&mut self, on: bool) -> &mut Self {
        self.axes().z.reverse = on;
        self
    }

    /// Places the major ticks of the x axis at the given values (MATLAB's `xticks`).
    ///
    /// Values outside the limits are not drawn, and the limits are not changed. The
    /// ticks are labelled with their values unless [`xtick_labels`](AxesMut::xtick_labels)
    /// names them or [`xtick_labels_hidden`](AxesMut::xtick_labels_hidden) hides them.
    /// A value that is not finite, or not positive on a logarithmic axis, is reported
    /// by [`Figure::validate`](crate::Figure::validate).
    pub fn xticks(&mut self, values: impl AsRef<[f64]>) -> &mut Self {
        self.axes().x.ticks = Ticks::Values(values.as_ref().to_vec());
        self
    }

    /// Places the major ticks of the y axis at the given values (MATLAB's `yticks`).
    ///
    /// Explicit ticks and explicit labels make a categorical axis: one tick per
    /// category, named by its label. See [`xticks`](AxesMut::xticks).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let cases = ["naca_0012", "naca_2412", "naca_4415"];
    /// let rows: Vec<f64> = (0..cases.len()).map(|k| k as f64).collect();
    ///
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// ax.scatter([0.31, 0.27, 0.22], &rows);
    /// ax.ylim(-0.5, 2.5)
    ///     .yreverse(true) // the first case at the top
    ///     .yticks(&rows)
    ///     .ytick_labels(cases);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn yticks(&mut self, values: impl AsRef<[f64]>) -> &mut Self {
        self.axes().y.ticks = Ticks::Values(values.as_ref().to_vec());
        self
    }

    /// Places the major ticks of the z axis at the given values (MATLAB's `zticks`).
    /// See [`xticks`](AxesMut::xticks).
    pub fn zticks(&mut self, values: impl AsRef<[f64]>) -> &mut Self {
        self.axes().z.ticks = Ticks::Values(values.as_ref().to_vec());
        self
    }

    /// Places a major tick of the x axis at every multiple of `step` within the limits.
    ///
    /// A step that is not a finite positive number is reported by
    /// [`Figure::validate`](crate::Figure::validate); one so fine that it would place
    /// more than 500 ticks is drawn as automatic ticks, with a warning in the report of
    /// the export.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0).xlim(-0.2, 1.2).xtick_step(0.2).ylim(-0.4, 0.4).ytick_step(0.2);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn xtick_step(&mut self, step: f64) -> &mut Self {
        self.xtick_step_from(step, 0.0)
    }

    /// Places a major tick of the y axis at every multiple of `step` within the limits.
    /// See [`xtick_step`](AxesMut::xtick_step).
    pub fn ytick_step(&mut self, step: f64) -> &mut Self {
        self.ytick_step_from(step, 0.0)
    }

    /// Places a major tick of the z axis at every multiple of `step` within the limits.
    /// See [`xtick_step`](AxesMut::xtick_step).
    pub fn ztick_step(&mut self, step: f64) -> &mut Self {
        self.ztick_step_from(step, 0.0)
    }

    /// Places a major tick of the x axis at every value `origin + k × step`, for every
    /// integer `k`, within the limits, so that the ticks need not include zero.
    pub fn xtick_step_from(&mut self, step: f64, origin: f64) -> &mut Self {
        self.axes().x.ticks = Ticks::Step { step, origin };
        self
    }

    /// Places a major tick of the y axis at every value `origin + k × step` within the
    /// limits. See [`xtick_step_from`](AxesMut::xtick_step_from).
    pub fn ytick_step_from(&mut self, step: f64, origin: f64) -> &mut Self {
        self.axes().y.ticks = Ticks::Step { step, origin };
        self
    }

    /// Places a major tick of the z axis at every value `origin + k × step` within the
    /// limits. See [`xtick_step_from`](AxesMut::xtick_step_from).
    pub fn ztick_step_from(&mut self, step: f64, origin: f64) -> &mut Self {
        self.axes().z.ticks = Ticks::Step { step, origin };
        self
    }

    /// Labels the ticks of the x axis with the given texts, one per value given to
    /// [`xticks`](AxesMut::xticks), in the same order (MATLAB's `xticklabels`).
    ///
    /// A plain string is interpreted as LaTeX, so `$…$` is typeset as mathematics, as
    /// for every other text of the figure. Labels without explicit tick values, or of a
    /// different number, are reported by [`Figure::validate`](crate::Figure::validate).
    pub fn xtick_labels<T: Into<Text>>(
        &mut self,
        labels: impl IntoIterator<Item = T>,
    ) -> &mut Self {
        self.axes().x.tick_labels = TickLabels::Text(labels.into_iter().map(Into::into).collect());
        self
    }

    /// Labels the ticks of the y axis with the given texts, one per value given to
    /// [`yticks`](AxesMut::yticks) (MATLAB's `yticklabels`). See
    /// [`xtick_labels`](AxesMut::xtick_labels).
    pub fn ytick_labels<T: Into<Text>>(
        &mut self,
        labels: impl IntoIterator<Item = T>,
    ) -> &mut Self {
        self.axes().y.tick_labels = TickLabels::Text(labels.into_iter().map(Into::into).collect());
        self
    }

    /// Labels the ticks of the z axis with the given texts, one per value given to
    /// [`zticks`](AxesMut::zticks) (MATLAB's `zticklabels`). See
    /// [`xtick_labels`](AxesMut::xtick_labels).
    pub fn ztick_labels<T: Into<Text>>(
        &mut self,
        labels: impl IntoIterator<Item = T>,
    ) -> &mut Self {
        self.axes().z.tick_labels = TickLabels::Text(labels.into_iter().map(Into::into).collect());
        self
    }

    /// Draws the ticks of the x axis without labels, and reserves no space for them
    /// (matplotlib's `labelbottom=False`).
    ///
    /// This suits a stack of rows that share one x axis, where only the bottom row is
    /// labelled; plot areas of equal weight keep equal sizes whatever their labels.
    pub fn xtick_labels_hidden(&mut self) -> &mut Self {
        self.axes().x.tick_labels = TickLabels::Hidden;
        self
    }

    /// Draws the ticks of the y axis without labels, and reserves no space for them;
    /// see [`xtick_labels_hidden`](AxesMut::xtick_labels_hidden).
    pub fn ytick_labels_hidden(&mut self) -> &mut Self {
        self.axes().y.tick_labels = TickLabels::Hidden;
        self
    }

    /// Draws the ticks of the z axis without labels; see
    /// [`xtick_labels_hidden`](AxesMut::xtick_labels_hidden).
    pub fn ztick_labels_hidden(&mut self) -> &mut Self {
        self.axes().z.tick_labels = TickLabels::Hidden;
        self
    }

    /// Turns every tick label of the x axis by `degrees` anticlockwise from
    /// horizontal (MATLAB's `xtickangle`, matplotlib's
    /// `tick_params(axis="x", labelrotation=…)`).
    ///
    /// A positive angle gives labels that rise to the right, a negative one labels
    /// that fall to the right, and 90 labels that stand vertically. Each label is
    /// turned about its end nearest its tick, so that it still points at the tick it
    /// names, and the layout reserves the space the turned labels take. The common
    /// exponent of the axis stays level, and the angle has no effect on hidden
    /// labels. Three-dimensional axes ignore the angle, which
    /// [`Figure::validate`](crate::Figure::validate) warns of; an angle that is not
    /// finite is reported as an error.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// ax.plot([1.0, 2.0, 3.0], [12.0, 7.0, 9.0]);
    /// ax.xticks([1.0, 2.0, 3.0])
    ///     .xtick_labels(["boundary layer", "panel method", "transition"])
    ///     .xtick_label_angle(45.0);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn xtick_label_angle(&mut self, degrees: f64) -> &mut Self {
        self.axes().x.tick_label_angle = degrees;
        self
    }

    /// Turns every tick label of the y axis by `degrees` anticlockwise from
    /// horizontal (MATLAB's `ytickangle`); see
    /// [`xtick_label_angle`](AxesMut::xtick_label_angle).
    pub fn ytick_label_angle(&mut self, degrees: f64) -> &mut Self {
        self.axes().y.tick_label_angle = degrees;
        self
    }

    /// Sets the angle of the tick labels of the z axis (MATLAB's `ztickangle`),
    /// which three-dimensional axes ignore, as
    /// [`xtick_label_angle`](AxesMut::xtick_label_angle) describes; the angle is
    /// stored so that it is kept in the figure.
    pub fn ztick_label_angle(&mut self, degrees: f64) -> &mut Self {
        self.axes().z.tick_label_angle = degrees;
        self
    }

    /// Shows or hides the x axis (matplotlib's `ax.xaxis.set_visible`, MATLAB's
    /// `ax.XAxis.Visible`).
    ///
    /// A hidden axis draws no spine (the bottom edge of the plot area, and the top
    /// edge when the box is on), no tick marks, no tick labels and no axis label, and
    /// no space is reserved for them, so the plot area grows into the space they
    /// would have taken. Its grid lines still follow [`grid`](AxesMut::grid), and its
    /// limits and scale still place the data. Hiding both axes removes the box. A
    /// hidden axis of a three-dimensional axes draws no ticks, tick labels or label,
    /// and the box is drawn as before.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(0.0, 1.0, 21);
    /// let squares: Vec<f64> = x.iter().map(|x| x * x).collect();
    /// let roots: Vec<f64> = x.iter().map(|x| x.sqrt()).collect();
    ///
    /// let mut fig = Figure::new().tiles(2, 1);
    /// let mut top = fig.axes(0, 0);
    /// top.plot(&x, &squares);
    /// top.xaxis_visible(false);
    /// let mut bottom = fig.axes(1, 0);
    /// bottom.plot(&x, &roots);
    /// bottom.xlabel("$x/c$");
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn xaxis_visible(&mut self, visible: bool) -> &mut Self {
        self.axes().x.visible = visible;
        self
    }

    /// Shows or hides the y axis (matplotlib's `ax.yaxis.set_visible`, MATLAB's
    /// `ax.YAxis.Visible`); see [`xaxis_visible`](AxesMut::xaxis_visible). A hidden y
    /// axis draws no left edge (nor right edge with the box on), ticks, tick labels or
    /// label, and the plot area grows to the left.
    pub fn yaxis_visible(&mut self, visible: bool) -> &mut Self {
        self.axes().y.visible = visible;
        self
    }

    /// Shows or hides the z axis of a three-dimensional axes; a hidden z axis draws
    /// no ticks, tick labels or label. See [`xaxis_visible`](AxesMut::xaxis_visible).
    pub fn zaxis_visible(&mut self, visible: bool) -> &mut Self {
        self.axes().z.visible = visible;
        self
    }

    /// Shows or hides grid lines along every axis (MATLAB's `grid on` and
    /// `grid off`).
    pub fn grid(&mut self, on: bool) -> &mut Self {
        let axes = self.axes();
        for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
            axis.grid = on;
        }
        self
    }

    /// Shows a boxed legend of every artist that has a label, at the given
    /// location.
    pub fn legend(&mut self, location: LegendLocation) -> &mut Self {
        self.axes().legend = Some(Legend::Inside(InsideLegend {
            location,
            ..InsideLegend::default()
        }));
        self
    }

    /// Lists the entries of the axes in the legend panel with the given identifier,
    /// replacing any legend inside the plot area.
    ///
    /// The identifier is that of a panel created with [`Figure::panel`] or
    /// [`Figure::panel_span`](crate::Figure::panel_span); one that names no panel holding a
    /// legend is reported by [`validate`](crate::Figure::validate).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(1, 2);
    /// let key = fig.panel(0, 1).id();
    /// let mut ax = fig.axes(0, 0);
    /// ax.plot(&[0.0, 1.0], &[0.0, 1.0]).label("Linear");
    /// ax.legend_in(key);
    /// assert!(fig.validate().is_valid());
    /// ```
    ///
    /// [`Figure::panel`]: crate::Figure::panel
    pub fn legend_in(&mut self, panel: NodeId) -> &mut Self {
        self.axes().legend = Some(Legend::Panel(panel));
        self
    }

    /// Sets the number of columns of the legend inside the plot area, showing a legend
    /// with the default location and box if the axes has none or lists its entries in a
    /// legend panel.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0)
    ///     .legend(LegendLocation::South)
    ///     .legend_columns(LegendEntryColumns::Count(3));
    /// ```
    pub fn legend_columns(&mut self, columns: LegendEntryColumns) -> &mut Self {
        self.inside_legend(|inside| inside.layout.columns = columns)
    }

    /// Sets the order in which the entries of the legend inside the plot area fill its
    /// columns, showing a legend with the default location and box if the axes has none or
    /// lists its entries in a legend panel.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0)
    ///     .legend(LegendLocation::North)
    ///     .legend_columns(LegendEntryColumns::Count(3))
    ///     .legend_order(LegendEntryOrder::RowMajor);
    /// ```
    pub fn legend_order(&mut self, order: LegendEntryOrder) -> &mut Self {
        self.inside_legend(|inside| inside.layout.order = order)
    }

    /// Sets whether the legend inside the plot area is drawn with a background and
    /// outline, showing a legend with the default location if the axes has none or lists
    /// its entries in a legend panel.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0).legend(LegendLocation::NorthWest).legend_boxed(false);
    /// ```
    pub fn legend_boxed(&mut self, boxed: bool) -> &mut Self {
        self.inside_legend(|inside| inside.boxed = boxed)
    }

    /// Sets which entries the legend inside the plot area lists, and in what order,
    /// showing a legend with the default location and box if the axes has none or lists
    /// its entries in a legend panel.
    ///
    /// `LegendEntries::Auto`, the default, lists every artist of the axes that has a
    /// label, in the order the artists were added. An explicit list names exactly
    /// the items to list, in their order, whatever the order in which the artists are
    /// drawn; an item that names an artist must name an artist of this axes.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(0.0, 1.0, 11);
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// let member = ax.plot(&x, &x).label("Member").id();
    /// // The reference is drawn last, on top of the member, but listed first.
    /// let reference = ax.plot(&x, &x).label("Reference").id();
    /// ax.legend(LegendLocation::NorthWest)
    ///     .legend_entries(LegendEntries::Explicit(vec![
    ///         LegendItem::artist(reference),
    ///         LegendItem::artist(member),
    ///     ]));
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn legend_entries(&mut self, entries: LegendEntries) -> &mut Self {
        self.inside_legend(|inside| inside.entries = entries)
    }

    /// Applies `change` to the legend inside the plot area, starting from the default
    /// inside legend if the axes has none or lists its entries in a legend panel.
    fn inside_legend(&mut self, change: impl FnOnce(&mut InsideLegend)) -> &mut Self {
        let axes = self.axes();
        let mut inside = match axes.legend.take() {
            Some(Legend::Inside(inside)) => inside,
            Some(Legend::Panel(_)) | None => InsideLegend::default(),
        };
        change(&mut inside);
        axes.legend = Some(Legend::Inside(inside));
        self
    }

    /// Hides the legend (MATLAB's `legend off`).
    pub fn legend_off(&mut self) -> &mut Self {
        self.axes().legend = None;
        self
    }

    /// Shows the colour scale with the given identifier as a colourbar inside the plot
    /// area, along the given edge (MATLAB's `colorbar` with a location such as `'east'`),
    /// and returns the handle through which its length is set.
    ///
    /// The bar is horizontal along the south and north edges and vertical along the east
    /// and west edges, with its tick labels facing the interior of the plot area. It stands
    /// on a boxed backdrop over the artists and takes no margin, so the plot area keeps its
    /// size; a bar that needs room of its own belongs in a panel (see
    /// [`PanelMut::colorbar`](crate::PanelMut::colorbar)). A colourbar the axes already
    /// shows keeps its length and the placement of its title and takes the new scale and
    /// edge; a new one is four fifths of the edge long, with the scale's title along it. An identifier that names no colour scale is reported by
    /// [`validate`](crate::Figure::validate).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(-1.0, 1.0, 41);
    /// let z = Matrix::from_fn(x.len(), x.len(), |row, col| x[col].powi(2) - x[row].powi(2));
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// ax.scalar_image(&z);
    /// let s = ax.color_scale().title("Pressure (Pa)").id();
    /// ax.colorbar(s, InsideColorbarSide::East).length(0.6);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn colorbar(&mut self, scale: NodeId, side: InsideColorbarSide) -> InsideColorbarMut<'_> {
        let axes = self.axes();
        let kept = axes.colorbar.unwrap_or_default();
        axes.colorbar = Some(InsideColorbar {
            scale,
            side,
            ..kept
        });
        InsideColorbarMut::new(self.fig, self.id)
    }

    /// Hides the colourbar inside the plot area (MATLAB's `colorbar off`). A panel that
    /// shows a scale of the axes is not affected.
    pub fn colorbar_off(&mut self) -> &mut Self {
        self.axes().colorbar = None;
        self
    }

    /// Sets the camera view of the axes by azimuth and elevation in degrees (MATLAB's
    /// `view`), converting the axes to three dimensions.
    ///
    /// The zoom and pan of an existing three-dimensional view are kept.
    pub fn view(&mut self, azimuth_deg: f64, elevation_deg: f64) -> &mut Self {
        let axes = self.axes();
        let view3d = match axes.projection {
            Projection::ThreeD { view3d } => view3d,
            Projection::TwoD => View3d::default(),
        };
        axes.projection = Projection::ThreeD {
            view3d: View3d {
                azimuth_deg,
                elevation_deg,
                ..view3d
            },
        };
        self
    }

    /// Sets whether the full outline of the plot box is drawn (MATLAB's `box on` and
    /// `box off`).
    pub fn box_on(&mut self, on: bool) -> &mut Self {
        self.axes().box_ = on;
        self
    }

    // Helpers.

    /// Returns the axes in the figure IR.
    fn axes(&mut self) -> &mut Axes {
        self.fig
            .axes_mut(self.id)
            .expect("an axes handle always refers to an axes of its figure")
    }

    /// Converts a two-dimensional axes to three dimensions with the default view,
    /// leaving a three-dimensional axes unchanged.
    pub(crate) fn make_3d(&mut self) {
        let axes = self.axes();
        if axes.projection == Projection::TwoD {
            axes.projection = Projection::ThreeD {
                view3d: View3d::default(),
            };
        }
    }

    /// Sets the scales of the x and y axes.
    fn set_xy_scales(&mut self, x: Scale, y: Scale) {
        let axes = self.axes();
        axes.x.scale = x;
        axes.y.scale = y;
    }

    /// Fixes the limits along a dimension.
    ///
    /// Valid limits are set through [`ironlab_ir::Figure::set_limits`], which
    /// propagates them to linked axes. Invalid limits are written to this axes only,
    /// so that validation reports them once without overwriting valid limits of the
    /// linked axes.
    fn set_limits(&mut self, dim: Dimension, min: f64, max: f64) -> &mut Self {
        let limits = Limits::Manual { min, max };
        match self.fig.set_limits(self.id, dim, limits) {
            Ok(()) => {}
            Err(IrError::InvalidLimits { .. }) => axis_mut(self.axes(), dim).limits = limits,
            Err(error) => unreachable!("an axes handle refers to an axes of its figure: {error}"),
        }
        self
    }

    /// Returns the default colour scale of the axes, creating it when the axes has none,
    /// and whether it was created.
    fn default_scale(&mut self) -> (NodeId, bool) {
        color_scale::default_or_new(self.fig, self.id)
    }

    /// Appends the artist built from a newly allocated identifier to the axes and
    /// returns that identifier.
    fn push_artist(&mut self, build: impl FnOnce(NodeId) -> Artist) -> NodeId {
        let id = self.fig.alloc_node_id();
        self.axes().artists.push(build(id));
        id
    }

    /// Adds a series with the defaults of `plot`, with z data when given.
    fn add_series(&mut self, x: DataSource, y: DataSource, z: Option<DataSource>) -> SeriesMut<'_> {
        let x = x.store(self.fig);
        let y = y.store(self.fig);
        let z = z.map(|z| z.store(self.fig));
        let id = self.push_artist(|id| {
            Artist::Series(Series {
                id,
                x,
                y,
                z,
                ..Series::default()
            })
        });
        SeriesMut::new(self.fig, id, None)
    }

    /// Adds a bar series with the defaults of `bar`, in the given orientation.
    fn add_bar(
        &mut self,
        positions: DataSource,
        values: DataSource,
        orientation: Orientation,
    ) -> BarMut<'_> {
        let positions = positions.store(self.fig);
        let values = values.store(self.fig);
        let id = self.push_artist(|id| {
            Artist::Bar(Bar {
                id,
                orientation,
                positions,
                values,
                ..Bar::default()
            })
        });
        BarMut::new(self.fig, id, None)
    }

    /// Adds cells of the stored values on unit edges with the given gap, mapped through the
    /// default colour scale of the axes.
    fn add_cells(&mut self, values: NodeId, gap: CellGap) -> CellsMut<'_> {
        let (scale, created) = self.default_scale();
        let id = self.push_artist(|id| {
            Artist::Cells(Cells {
                id,
                values,
                gap,
                color_scale: Some(scale),
                ..Cells::default()
            })
        });
        CellsMut::new(self.fig, id, created.then_some(scale))
    }

    /// Adds stairs with the defaults of `stairs`.
    fn add_stairs(&mut self, data: StairsData) -> StairsMut<'_> {
        let id = self.push_artist(|id| {
            Artist::Stairs(Stairs {
                id,
                data,
                ..Stairs::default()
            })
        });
        StairsMut::new(self.fig, id)
    }

    /// Adds contours of a gridded field.
    fn add_contour(
        &mut self,
        x: DataSource,
        y: DataSource,
        z: DataSource,
        fill: bool,
        placement: ContourPlacement,
    ) -> ContourMut<'_> {
        let (grid, z) = store_grid(self.fig, x, y, z);
        let (scale, created) = self.default_scale();
        let id = self.push_artist(|id| {
            Artist::Contour(Contour {
                id,
                grid,
                z,
                fill,
                placement,
                color_scale: Some(scale),
                ..Contour::default()
            })
        });
        ContourMut::new(self.fig, id, created.then_some(scale))
    }

    /// Adds a surface over a gridded field, leaving the projection of the axes as it is.
    fn add_surface(
        &mut self,
        x: DataSource,
        y: DataSource,
        z: DataSource,
        face: ColorSpec,
        edge: ColorSpec,
    ) -> SurfaceMut<'_> {
        let (grid, z) = store_grid(self.fig, x, y, z);
        let (scale, created) = self.default_scale();
        let id = self.push_artist(|id| {
            Artist::Surface(Surface {
                id,
                grid,
                z,
                face,
                edge,
                color_scale: Some(scale),
                ..Surface::default()
            })
        });
        SurfaceMut::new(self.fig, id, created.then_some(scale))
    }
}

/// Returns the coordinate axis of an axes along a dimension, mutably.
fn axis_mut(axes: &mut Axes, dim: Dimension) -> &mut Axis {
    match dim {
        Dimension::X => &mut axes.x,
        Dimension::Y => &mut axes.y,
        Dimension::Z => &mut axes.z,
    }
}

/// One group of the rows that [`ycategory_groups`](AxesMut::ycategory_groups) lays out.
#[derive(Clone, Debug, PartialEq)]
struct CategoryGroup {
    /// The name of the group, as given.
    name: String,
    /// The row of the header of the group.
    header: usize,
    /// The name of each member, as given, with its row, in the order given.
    members: Vec<(String, usize)>,
}

/// The rows of a categorical y axis laid out in groups by
/// [`ycategory_groups`](AxesMut::ycategory_groups): where each header and each member
/// lies, and how many rows there are, header and blank rows included.
///
/// Rows are numbered from 0 at the top, and the number of a row is its y coordinate on
/// cells with unit edges, so that [`place`](CategoryRows::place) gives
/// [`occupancy`](AxesMut::occupancy) one row of data per row of the layout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CategoryRows {
    groups: Vec<CategoryGroup>,
    len: usize,
}

impl CategoryRows {
    /// Returns the number of rows laid out, header and blank rows included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns whether no row was laid out, which is the case when there were no groups.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the row of the member `member` of the group `group`, or `None` when that
    /// group has no such member. A name that occurs in several groups is found in the
    /// group named only.
    #[must_use]
    pub fn row(&self, group: &str, member: &str) -> Option<usize> {
        self.groups
            .iter()
            .filter(|g| g.name == group)
            .flat_map(|g| &g.members)
            .find(|(name, _)| name == member)
            .map(|&(_, row)| row)
    }

    /// Returns the row of the header of the group `group`, or `None` when there is no
    /// such group.
    #[must_use]
    pub fn header(&self, group: &str) -> Option<usize> {
        self.groups
            .iter()
            .find(|g| g.name == group)
            .map(|g| g.header)
    }

    /// Returns every row of the layout, holding the data of each member in its own row and
    /// nothing in a header or blank row, ready for [`occupancy`](AxesMut::occupancy).
    ///
    /// `member_rows` holds one row of data per member, in the order the members were
    /// given, group by group.
    ///
    /// # Panics
    ///
    /// Panics when `member_rows` does not hold one row per member, because the rows could
    /// not then be matched to the names, and placing them anyway would draw the data of
    /// one member beside the name of another.
    #[must_use]
    pub fn place<R: AsRef<[f64]>>(
        &self,
        member_rows: impl IntoIterator<Item = R>,
    ) -> Vec<Vec<f64>> {
        let given: Vec<R> = member_rows.into_iter().collect();
        let members: Vec<usize> = self
            .groups
            .iter()
            .flat_map(|g| g.members.iter().map(|&(_, row)| row))
            .collect();
        assert!(
            given.len() == members.len(),
            "CategoryRows::place needs one row per member: {} members were laid out, but {} rows were given",
            members.len(),
            given.len()
        );
        let mut rows = vec![Vec::new(); self.len];
        for (row, data) in members.into_iter().zip(&given) {
            rows[row] = data.as_ref().to_vec();
        }
        rows
    }
}
