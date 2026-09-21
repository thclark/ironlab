//! The axes handle and its plotting functions.

use ironlab_ir::{
    Artist, Axes, Axis, ColorSpec, ColormapName, Contour, ContourPlacement, Dimension, Image,
    IndexedImage, IrError, Legend, LegendLocation, Limits, Line, MappedImage, NdArray, NodeId,
    Projection, Quiver, Scale, Scatter, Surface, Text, View3d,
};

use crate::artists::{
    ContourMut, ImageMut, IndexedImageMut, LineMut, MappedImageMut, QuiverMut, ScatterMut,
    SurfaceMut,
};
use crate::grid::{GridCoords, matrix_array, store_grid};
use crate::matrix::Matrix;
use crate::pixels::{ImageValues, Pixels};

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
/// Every array is copied into the figure, so the caller's data may be dropped or
/// changed afterwards. Plotting functions that need three dimensions (`plot3`,
/// `scatter3`, `contour3`, `quiver3`, `surf` and `mesh`) convert a two-dimensional
/// axes to three dimensions with the default view, as MATLAB does; so does placing an
/// image on a wall of the axes with [`plane`](ImageMut::plane).
///
/// ```
/// use ironlab::prelude::*;
///
/// let x = linspace(0.0, 1.0, 11);
/// let y: Vec<f64> = x.iter().map(|x| x * x).collect();
///
/// let mut fig = Figure::new();
/// let mut ax = fig.axes(0, 0);
/// ax.plot(&x, &y).marker(Marker::Circle).display_name("$x^2$");
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

    // Line plots.

    /// Plots a line through the points `(x[i], y[i])` (MATLAB's `plot`).
    ///
    /// The line is solid, 0.75 pt wide and has no markers; its colour is the next
    /// colour of the axes colour order, chosen when the figure is drawn.
    pub fn plot(&mut self, x: impl AsRef<[f64]>, y: impl AsRef<[f64]>) -> LineMut<'_> {
        self.add_line(x.as_ref(), y.as_ref(), None)
    }

    /// Plots a line through the points `(x[i], y[i], z[i])` in three dimensions
    /// (MATLAB's `plot3`), converting the axes to three dimensions.
    pub fn plot3(
        &mut self,
        x: impl AsRef<[f64]>,
        y: impl AsRef<[f64]>,
        z: impl AsRef<[f64]>,
    ) -> LineMut<'_> {
        self.make_3d();
        self.add_line(x.as_ref(), y.as_ref(), Some(z.as_ref()))
    }

    /// Plots a line with logarithmic x and y axes (MATLAB's `loglog`).
    ///
    /// Points with a non-positive coordinate are not drawn, and
    /// [`Figure::validate`](crate::Figure::validate) warns about them.
    pub fn loglog(&mut self, x: impl AsRef<[f64]>, y: impl AsRef<[f64]>) -> LineMut<'_> {
        self.set_xy_scales(Scale::Log, Scale::Log);
        self.add_line(x.as_ref(), y.as_ref(), None)
    }

    /// Plots a line with a logarithmic x axis and a linear y axis (MATLAB's
    /// `semilogx`).
    pub fn semilogx(&mut self, x: impl AsRef<[f64]>, y: impl AsRef<[f64]>) -> LineMut<'_> {
        self.set_xy_scales(Scale::Log, Scale::Linear);
        self.add_line(x.as_ref(), y.as_ref(), None)
    }

    /// Plots a line with a linear x axis and a logarithmic y axis (MATLAB's
    /// `semilogy`).
    pub fn semilogy(&mut self, x: impl AsRef<[f64]>, y: impl AsRef<[f64]>) -> LineMut<'_> {
        self.set_xy_scales(Scale::Linear, Scale::Log);
        self.add_line(x.as_ref(), y.as_ref(), None)
    }

    // Scatter plots.

    /// Draws a marker at each point `(x[i], y[i])` (MATLAB's `scatter`).
    ///
    /// Markers are unfilled 4 pt circles in the next colour of the axes colour order.
    pub fn scatter(&mut self, x: impl AsRef<[f64]>, y: impl AsRef<[f64]>) -> ScatterMut<'_> {
        self.add_scatter(x.as_ref(), y.as_ref(), None)
    }

    /// Draws a marker at each point `(x[i], y[i], z[i])` in three dimensions
    /// (MATLAB's `scatter3`), converting the axes to three dimensions.
    pub fn scatter3(
        &mut self,
        x: impl AsRef<[f64]>,
        y: impl AsRef<[f64]>,
        z: impl AsRef<[f64]>,
    ) -> ScatterMut<'_> {
        self.make_3d();
        self.add_scatter(x.as_ref(), y.as_ref(), Some(z.as_ref()))
    }

    // Contour plots.

    /// Draws isolines of the field `z` sampled on a grid (MATLAB's `contour`).
    ///
    /// The field has one row per y coordinate and one column per x coordinate; see
    /// [`GridCoords`] for the accepted forms of `x` and `y`. Ten levels are chosen
    /// automatically and each isoline is coloured from the colormap by its level.
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
        x: impl Into<GridCoords>,
        y: impl Into<GridCoords>,
        z: &Matrix,
    ) -> ContourMut<'_> {
        let placement = ContourPlacement::default();
        self.add_contour(x.into(), y.into(), z, false, placement)
    }

    /// Fills the bands between levels of the field `z` sampled on a grid (MATLAB's
    /// `contourf`).
    pub fn contourf(
        &mut self,
        x: impl Into<GridCoords>,
        y: impl Into<GridCoords>,
        z: &Matrix,
    ) -> ContourMut<'_> {
        let placement = ContourPlacement::default();
        self.add_contour(x.into(), y.into(), z, true, placement)
    }

    /// Draws each isoline of the field `z` at the height of its level (MATLAB's
    /// `contour3`), converting the axes to three dimensions.
    pub fn contour3(
        &mut self,
        x: impl Into<GridCoords>,
        y: impl Into<GridCoords>,
        z: &Matrix,
    ) -> ContourMut<'_> {
        self.make_3d();
        self.add_contour(x.into(), y.into(), z, false, ContourPlacement::AtLevel)
    }

    // Vector fields.

    /// Draws an arrow with components `(u[i], v[i])` at each point `(x[i], y[i])`
    /// (MATLAB's `quiver`).
    ///
    /// Arrows are scaled automatically so that they do not overlap; see
    /// [`QuiverMut::scale`] and [`QuiverMut::no_scale`].
    pub fn quiver(
        &mut self,
        x: impl AsRef<[f64]>,
        y: impl AsRef<[f64]>,
        u: impl AsRef<[f64]>,
        v: impl AsRef<[f64]>,
    ) -> QuiverMut<'_> {
        let x = self.store_vector(x.as_ref());
        let y = self.store_vector(y.as_ref());
        let u = self.store_vector(u.as_ref());
        let v = self.store_vector(v.as_ref());
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
        x: impl AsRef<[f64]>,
        y: impl AsRef<[f64]>,
        z: impl AsRef<[f64]>,
        u: impl AsRef<[f64]>,
        v: impl AsRef<[f64]>,
        w: impl AsRef<[f64]>,
    ) -> QuiverMut<'_> {
        self.make_3d();
        let x = self.store_vector(x.as_ref());
        let y = self.store_vector(y.as_ref());
        let z = self.store_vector(z.as_ref());
        let u = self.store_vector(u.as_ref());
        let v = self.store_vector(v.as_ref());
        let w = self.store_vector(w.as_ref());
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
    /// Faces are coloured from the colormap by height and outlined by thin black
    /// edges.
    pub fn surf(
        &mut self,
        x: impl Into<GridCoords>,
        y: impl Into<GridCoords>,
        z: &Matrix,
    ) -> SurfaceMut<'_> {
        let defaults = Surface::default();
        self.add_surface(x.into(), y.into(), z, defaults.face, defaults.edge)
    }

    /// Draws the surface of heights `z` over a grid as a wireframe (MATLAB's `mesh`),
    /// converting the axes to three dimensions.
    ///
    /// Faces are filled with the figure background colour, so that they hide the edges
    /// behind them, and edges are coloured from the colormap by height.
    pub fn mesh(
        &mut self,
        x: impl Into<GridCoords>,
        y: impl Into<GridCoords>,
        z: &Matrix,
    ) -> SurfaceMut<'_> {
        let face = ColorSpec::Rgba {
            color: self.fig.background,
        };
        self.add_surface(x.into(), y.into(), z, face, ColorSpec::Colormapped)
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
    /// left as it is: a two-dimensional axes stays two-dimensional.
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
    pub fn image(&mut self, pixels: &Pixels) -> ImageMut<'_> {
        let pixels = self.fig.add_data(pixels.to_array());
        let id = self.push_artist(|id| {
            Artist::Image(Image {
                id,
                pixels,
                ..Image::default()
            })
        });
        ImageMut::new(self.fig, id)
    }

    /// Draws a colour-indexed image: a raster of pixels whose values name entries of the
    /// axes colormap directly (MATLAB's `image` with an indexed array).
    ///
    /// An index from 0 to 255 takes that entry of the colormap; a floating-point index
    /// is truncated toward zero first. The indices neither use nor change the colour
    /// limits of the axes. A pixel whose index lies outside the colormap, or is not
    /// finite, is transparent unless the handle's [`below`](IndexedImageMut::below),
    /// [`above`](IndexedImageMut::above) and [`non_finite`](IndexedImageMut::non_finite)
    /// policies say otherwise. The indices are taken from a [`ByteMatrix`](crate::ByteMatrix), stored as
    /// bytes, or a [`Matrix`], stored as floating-point values, by value or by
    /// reference, and the image is placed as by [`image`](AxesMut::image).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let classes = ByteMatrix::from_fn(8, 8, |row, col| ((row + col) % 4 * 85) as u8);
    ///
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0).indexed_image(&classes).above(OutOfRange::Clamp);
    /// ```
    pub fn indexed_image(&mut self, indices: impl Into<ImageValues>) -> IndexedImageMut<'_> {
        let indices = self.fig.add_data(indices.into().into_array());
        let id = self.push_artist(|id| {
            Artist::IndexedImage(IndexedImage {
                id,
                indices,
                ..IndexedImage::default()
            })
        });
        IndexedImageMut::new(self.fig, id)
    }

    /// Draws a colour-mapped image: a raster of data values, each scaled through the
    /// colour limits of the axes into its colormap (MATLAB's `imagesc`).
    ///
    /// The values are coloured as the colour data of a surface is, and contribute to
    /// automatic colour limits in the same way; [`clim`](AxesMut::clim) fixes the
    /// limits. A pixel whose value lies outside the colour limits, or is not finite, is
    /// transparent unless the handle's [`below`](MappedImageMut::below),
    /// [`above`](MappedImageMut::above) and [`non_finite`](MappedImageMut::non_finite)
    /// policies say otherwise. The values are taken from a [`Matrix`] or a
    /// [`ByteMatrix`](crate::ByteMatrix) as by [`indexed_image`](AxesMut::indexed_image), and the image is
    /// placed as by [`image`](AxesMut::image).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(-2.0, 2.0, 81);
    /// let y = linspace(-1.0, 1.0, 41);
    /// let field = Matrix::from_fn(y.len(), x.len(), |row, col| (x[col] * y[row]).sin());
    ///
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0)
    ///     .mapped_image(&field)
    ///     .pixel_columns(-2.0, 2.0)
    ///     .pixel_rows(-1.0, 1.0)
    ///     .non_finite(Color::BLACK);
    /// fig.axes(0, 0).colormap(Colormap::Magma).clim(-1.0, 1.0);
    /// ```
    pub fn mapped_image(&mut self, values: impl Into<ImageValues>) -> MappedImageMut<'_> {
        let values = self.fig.add_data(values.into().into_array());
        let id = self.push_artist(|id| {
            Artist::MappedImage(MappedImage {
                id,
                values,
                ..MappedImage::default()
            })
        });
        MappedImageMut::new(self.fig, id)
    }

    // Axes properties.

    /// Sets the title drawn above the axes.
    pub fn title(&mut self, title: impl Into<Text>) -> &mut Self {
        self.axes().title = Some(title.into());
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

    /// Shows or hides grid lines along every axis (MATLAB's `grid on` and
    /// `grid off`).
    pub fn grid(&mut self, on: bool) -> &mut Self {
        let axes = self.axes();
        for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
            axis.grid = on;
        }
        self
    }

    /// Shows a boxed legend of every artist that has a display name, at the given
    /// location.
    pub fn legend(&mut self, location: LegendLocation) -> &mut Self {
        self.axes().legend = Some(Legend {
            location,
            ..Legend::default()
        });
        self
    }

    /// Hides the legend (MATLAB's `legend off`).
    pub fn legend_off(&mut self) -> &mut Self {
        self.axes().legend = None;
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

    /// Sets the colormap used by colormapped artists in this axes.
    pub fn colormap(&mut self, colormap: ColormapName) -> &mut Self {
        self.axes().colormap = colormap;
        self
    }

    /// Fixes the data values mapped to the first and last colours of the colormap
    /// (MATLAB's `clim`).
    ///
    /// Limits that are not finite or not increasing are reported by
    /// [`Figure::validate`](crate::Figure::validate).
    pub fn clim(&mut self, min: f64, max: f64) -> &mut Self {
        self.axes().clim = Limits::Manual { min, max };
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

    /// Copies a vector into the figure's data table.
    fn store_vector(&mut self, values: &[f64]) -> ironlab_ir::DataId {
        self.fig.add_data(NdArray::vector(values.to_vec()))
    }

    /// Appends the artist built from a newly allocated identifier to the axes and
    /// returns that identifier.
    fn push_artist(&mut self, build: impl FnOnce(NodeId) -> Artist) -> NodeId {
        let id = self.fig.alloc_node_id();
        self.axes().artists.push(build(id));
        id
    }

    /// Adds a line, with z data when given.
    fn add_line(&mut self, x: &[f64], y: &[f64], z: Option<&[f64]>) -> LineMut<'_> {
        let x = self.store_vector(x);
        let y = self.store_vector(y);
        let z = z.map(|z| self.store_vector(z));
        let id = self.push_artist(|id| {
            Artist::Line(Line {
                id,
                x,
                y,
                z,
                ..Line::default()
            })
        });
        LineMut::new(self.fig, id)
    }

    /// Adds a scatter, with z data when given.
    fn add_scatter(&mut self, x: &[f64], y: &[f64], z: Option<&[f64]>) -> ScatterMut<'_> {
        let x = self.store_vector(x);
        let y = self.store_vector(y);
        let z = z.map(|z| self.store_vector(z));
        let id = self.push_artist(|id| {
            Artist::Scatter(Scatter {
                id,
                x,
                y,
                z,
                ..Scatter::default()
            })
        });
        ScatterMut::new(self.fig, id)
    }

    /// Adds contours of a gridded field.
    fn add_contour(
        &mut self,
        x: GridCoords,
        y: GridCoords,
        z: &Matrix,
        fill: bool,
        placement: ContourPlacement,
    ) -> ContourMut<'_> {
        let grid = store_grid(self.fig, x, y, z);
        let z = self.fig.add_data(matrix_array(z));
        let id = self.push_artist(|id| {
            Artist::Contour(Contour {
                id,
                grid,
                z,
                fill,
                placement,
                ..Contour::default()
            })
        });
        ContourMut::new(self.fig, id)
    }

    /// Adds a surface over a gridded field, converting the axes to three dimensions.
    fn add_surface(
        &mut self,
        x: GridCoords,
        y: GridCoords,
        z: &Matrix,
        face: ColorSpec,
        edge: ColorSpec,
    ) -> SurfaceMut<'_> {
        self.make_3d();
        let grid = store_grid(self.fig, x, y, z);
        let z = self.fig.add_data(matrix_array(z));
        let id = self.push_artist(|id| {
            Artist::Surface(Surface {
                id,
                grid,
                z,
                face,
                edge,
                ..Surface::default()
            })
        });
        SurfaceMut::new(self.fig, id)
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
