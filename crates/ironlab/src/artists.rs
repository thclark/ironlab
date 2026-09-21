//! Handles to artists, with chainable property setters.
//!
//! Each handle borrows the figure mutably and refers to its artist by node identifier.
//! Setters write directly to the artist in the figure IR and return the handle, so that
//! several properties can be set in one expression.

use ironlab_ir::{
    Artist, Contour, DashStyle, DataId, Figure, Image, ImagePlane, IndexedImage, Levels, Line,
    MappedImage, MarkerShape, NdArray, NodeId, OutOfRange, PixelRange, Quiver, QuiverScale,
    Scatter, ScatterColor, ScatterSize, Surface, Text,
};

use crate::axes::AxesMut;
use crate::color::IntoColorSpec;
use crate::grid::matrix_array;
use crate::matrix::Matrix;

/// Defines the constructor of an artist handle and the accessor of its artist.
macro_rules! artist_handle {
    ($handle:ident, $variant:ident) => {
        impl<'a> $handle<'a> {
            /// Creates a handle to the artist with the given identifier, which must be
            /// an artist of this kind in the figure.
            pub(crate) fn new(fig: &'a mut Figure, id: NodeId) -> Self {
                Self { fig, id }
            }

            /// Returns the artist in the figure IR.
            fn artist(&mut self) -> &mut $variant {
                match self.fig.artist_mut(self.id) {
                    Some(Artist::$variant(artist)) => artist,
                    _ => unreachable!(concat!(
                        "a ",
                        stringify!($handle),
                        " handle always refers to an artist of its kind"
                    )),
                }
            }
        }
    };
}

artist_handle!(LineMut, Line);
artist_handle!(ScatterMut, Scatter);
artist_handle!(ContourMut, Contour);
artist_handle!(QuiverMut, Quiver);
artist_handle!(SurfaceMut, Surface);

/// Defines the constructor and accessor of an image handle, and the setters that every
/// kind of image has: its legend name, the placement of its pixel centres and its
/// plane.
macro_rules! image_handle {
    ($handle:ident, $variant:ident) => {
        artist_handle!($handle, $variant);

        impl $handle<'_> {
            /// Returns the node identifier of the image.
            #[must_use]
            pub fn id(&self) -> NodeId {
                self.id
            }

            /// Sets the name shown for the image in the legend.
            pub fn display_name(&mut self, name: impl Into<Text>) -> &mut Self {
                self.artist().display_name = Some(name.into());
                self
            }

            /// Places the centres of the first and last columns of the image at the
            /// given coordinates along the first axis of its plane (MATLAB's `XData`).
            ///
            /// The pitch between centres follows from the number of columns, and the
            /// image covers half a pitch beyond each centre; a `last` less than `first`
            /// mirrors the image. Without a range the centres lie at 0, 1, …, n − 1.
            /// The coordinates must be finite and may coincide only when the image has
            /// one column, as [`Figure::validate`](crate::Figure::validate) checks.
            pub fn pixel_columns(&mut self, first: f64, last: f64) -> &mut Self {
                self.artist().placement.columns = Some(PixelRange { first, last });
                self
            }

            /// Places the centres of the first and last rows of the image at the given
            /// coordinates along the second axis of its plane (MATLAB's `YData`), as
            /// [`pixel_columns`](Self::pixel_columns) places the columns.
            ///
            /// Row 0 of the pixels lies at `first`, so an image whose rows count down
            /// from its top is placed the right way up by a `first` greater than `last`.
            pub fn pixel_rows(&mut self, first: f64, last: f64) -> &mut Self {
                self.artist().placement.rows = Some(PixelRange { first, last });
                self
            }

            /// Sets the plane of the axes in which the image lies, with its offset along
            /// the third axis.
            ///
            /// The xz and yz planes are the walls of a three-dimensional axes, so placing
            /// the image in one of them converts a two-dimensional axes to three
            /// dimensions with the default view, as [`surf`](crate::AxesMut::surf) does;
            /// an axes that is already three-dimensional keeps its view. The xy plane
            /// never changes the projection: it is the only plane of a two-dimensional
            /// axes, which ignores its height, and the floor of a three-dimensional one.
            pub fn plane(&mut self, plane: ImagePlane) -> &mut Self {
                self.artist().placement.plane = plane;
                make_3d_for_wall(self.fig, self.id, plane);
                self
            }
        }
    };
}

/// A handle to a line created by [`plot`](crate::AxesMut::plot) and its relatives.
///
/// ```
/// use ironlab::prelude::*;
///
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .plot([0.0, 1.0, 2.0], [0.0, 1.0, 4.0])
///     .display_name("$x^2$")
///     .color(Color::rgb(0.0, 0.45, 0.7))
///     .line_width(1.5)
///     .dash(Dash::DashDot)
///     .marker(Marker::Square)
///     .marker_size(5.0)
///     .marker_face(Color::WHITE);
/// ```
#[derive(Debug)]
pub struct LineMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
}

impl LineMut<'_> {
    /// Returns the node identifier of the line.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets the name shown for the line in the legend.
    pub fn display_name(&mut self, name: impl Into<Text>) -> &mut Self {
        self.artist().display_name = Some(name.into());
        self
    }

    /// Sets the colour of the line; markers whose edge colour is automatic follow it.
    pub fn color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().line.color = color.into_color_spec();
        self
    }

    /// Sets the line width in points.
    pub fn line_width(&mut self, width_pt: f64) -> &mut Self {
        self.artist().line.width_pt = width_pt;
        self
    }

    /// Sets the dash pattern of the line; [`Dash::None`](crate::Dash::None) draws only
    /// the markers.
    pub fn dash(&mut self, dash: DashStyle) -> &mut Self {
        self.artist().line.dash = dash;
        self
    }

    /// Sets the shape of the markers drawn at the data points.
    pub fn marker(&mut self, shape: MarkerShape) -> &mut Self {
        self.artist().marker.shape = shape;
        self
    }

    /// Sets the marker size in points.
    pub fn marker_size(&mut self, size_pt: f64) -> &mut Self {
        self.artist().marker.size_pt = size_pt;
        self
    }

    /// Sets the colour of the marker interiors, which are unfilled by default.
    pub fn marker_face(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().marker.face = color.into_color_spec();
        self
    }

    /// Sets the colour of the marker outlines, which follow the line colour by
    /// default.
    pub fn marker_edge(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().marker.edge = color.into_color_spec();
        self
    }
}

/// A handle to a scatter created by [`scatter`](crate::AxesMut::scatter) or
/// [`scatter3`](crate::AxesMut::scatter3).
///
/// ```
/// use ironlab::prelude::*;
///
/// let x = [0.0, 1.0, 2.0];
/// let y = [2.0, 0.0, 1.0];
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .scatter(x, y)
///     .sizes([3.0, 6.0, 9.0])
///     .colors(y)
///     .marker(Marker::Diamond)
///     .filled();
/// ```
#[derive(Debug)]
pub struct ScatterMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
}

impl ScatterMut<'_> {
    /// Returns the node identifier of the scatter.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets the name shown for the scatter in the legend.
    pub fn display_name(&mut self, name: impl Into<Text>) -> &mut Self {
        self.artist().display_name = Some(name.into());
        self
    }

    /// Gives every marker the same size in points.
    pub fn size(&mut self, size_pt: f64) -> &mut Self {
        let previous = std::mem::replace(
            &mut self.artist().size,
            ScatterSize::Scalar { value: size_pt },
        );
        if let ScatterSize::Data { data } = previous {
            release_data(self.fig, data);
        }
        self
    }

    /// Gives each marker its own size in points, one value per point.
    pub fn sizes(&mut self, sizes_pt: impl AsRef<[f64]>) -> &mut Self {
        let data = self
            .fig
            .add_data(NdArray::vector(sizes_pt.as_ref().to_vec()));
        let previous = std::mem::replace(&mut self.artist().size, ScatterSize::Data { data });
        if let ScatterSize::Data { data } = previous {
            release_data(self.fig, data);
        }
        self
    }

    /// Gives every marker the same colour.
    pub fn color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        let spec = color.into_color_spec();
        let previous = std::mem::replace(&mut self.artist().color, ScatterColor::Spec { spec });
        if let ScatterColor::Data { data } = previous {
            release_data(self.fig, data);
        }
        self
    }

    /// Colours each marker by a data value, one per point, mapped through the axes
    /// colormap and colour limits.
    pub fn colors(&mut self, values: impl AsRef<[f64]>) -> &mut Self {
        let data = self.fig.add_data(NdArray::vector(values.as_ref().to_vec()));
        let previous = std::mem::replace(&mut self.artist().color, ScatterColor::Data { data });
        if let ScatterColor::Data { data } = previous {
            release_data(self.fig, data);
        }
        self
    }

    /// Sets the marker shape.
    pub fn marker(&mut self, shape: MarkerShape) -> &mut Self {
        self.artist().marker.shape = shape;
        self
    }

    /// Fills the markers with their colour (MATLAB's `scatter(..., 'filled')`).
    pub fn filled(&mut self) -> &mut Self {
        self.artist().marker.face = ironlab_ir::ColorSpec::Auto;
        self
    }
}

/// A handle to contours created by [`contour`](crate::AxesMut::contour),
/// [`contourf`](crate::AxesMut::contourf) or [`contour3`](crate::AxesMut::contour3).
#[derive(Debug)]
pub struct ContourMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
}

impl ContourMut<'_> {
    /// Returns the node identifier of the contours.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets the name shown for the contours in the legend.
    pub fn display_name(&mut self, name: impl Into<Text>) -> &mut Self {
        self.artist().display_name = Some(name.into());
        self
    }

    /// Chooses approximately `count` levels at nice values spanning the data range.
    pub fn levels(&mut self, count: u32) -> &mut Self {
        self.artist().levels = Levels::Auto { count };
        self
    }

    /// Draws contours at the given levels, which must be finite and increasing.
    ///
    /// Levels that are not are reported by
    /// [`Figure::validate`](crate::Figure::validate).
    pub fn level_values(&mut self, levels: impl AsRef<[f64]>) -> &mut Self {
        self.artist().levels = Levels::Explicit {
            values: levels.as_ref().to_vec(),
        };
        self
    }

    /// Sets the width of the isolines in points.
    pub fn line_width(&mut self, width_pt: f64) -> &mut Self {
        self.artist().line.width_pt = width_pt;
        self
    }

    /// Sets the colour of the isolines, which are coloured from the colormap by level
    /// by default.
    pub fn color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().line.color = color.into_color_spec();
        self
    }
}

/// A handle to arrows created by [`quiver`](crate::AxesMut::quiver) or
/// [`quiver3`](crate::AxesMut::quiver3).
#[derive(Debug)]
pub struct QuiverMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
}

impl QuiverMut<'_> {
    /// Returns the node identifier of the arrows.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets the name shown for the arrows in the legend.
    pub fn display_name(&mut self, name: impl Into<Text>) -> &mut Self {
        self.artist().display_name = Some(name.into());
        self
    }

    /// Multiplies the automatic arrow scale by a factor (MATLAB's
    /// `quiver(..., scale)`).
    pub fn scale(&mut self, factor: f64) -> &mut Self {
        self.artist().scale = QuiverScale::Factor { value: factor };
        self
    }

    /// Draws arrows with the vectors' lengths in data units (MATLAB's
    /// `quiver(..., 'off')`).
    pub fn no_scale(&mut self) -> &mut Self {
        self.artist().scale = QuiverScale::Off;
        self
    }

    /// Sets the colour of the arrows.
    pub fn color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().line.color = color.into_color_spec();
        self
    }

    /// Sets the width of the arrow lines in points.
    pub fn line_width(&mut self, width_pt: f64) -> &mut Self {
        self.artist().line.width_pt = width_pt;
        self
    }

    /// Sets the length of each arrow head as a fraction of its arrow's length.
    pub fn head_size(&mut self, fraction: f64) -> &mut Self {
        self.artist().head_size = fraction;
        self
    }
}

/// A handle to a surface created by [`surf`](crate::AxesMut::surf) or
/// [`mesh`](crate::AxesMut::mesh).
///
/// ```
/// use ironlab::prelude::*;
///
/// let x = linspace(-1.0, 1.0, 21);
/// let y = linspace(-1.0, 1.0, 11);
/// let z = Matrix::from_fn(y.len(), x.len(), |row, col| x[col] * y[row]);
///
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .surf(&x, &y, &z)
///     .edge_color(None)
///     .display_name("$xy$");
/// ```
#[derive(Debug)]
pub struct SurfaceMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
}

impl SurfaceMut<'_> {
    /// Returns the node identifier of the surface.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets the name shown for the surface in the legend.
    pub fn display_name(&mut self, name: impl Into<Text>) -> &mut Self {
        self.artist().display_name = Some(name.into());
        self
    }

    /// Sets the colour of the faces; `None` leaves them unfilled.
    pub fn face_color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().face = color.into_color_spec();
        self
    }

    /// Sets the colour of the face edges; `None` hides them.
    pub fn edge_color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().edge = color.into_color_spec();
        self
    }

    /// Sets the width of the face edges in points.
    pub fn edge_width(&mut self, width_pt: f64) -> &mut Self {
        self.artist().edge_width_pt = width_pt;
        self
    }

    /// Colours the surface by the given values instead of by height (MATLAB's
    /// `surf(x, y, z, c)`); the matrix must have the same shape as the heights.
    pub fn color_data(&mut self, c: &Matrix) -> &mut Self {
        let data = self.fig.add_data(matrix_array(c));
        if let Some(previous) = self.artist().c.replace(data) {
            release_data(self.fig, previous);
        }
        self
    }
}

/// A handle to a true-colour image created by [`image`](crate::AxesMut::image).
///
/// ```
/// use ironlab::prelude::*;
///
/// let pixels = Pixels::rgb_from_fn(16, 16, |row, col| {
///     Color::rgb(row as f32 / 15.0, col as f32 / 15.0, 0.0)
/// });
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .image(&pixels)
///     .pixel_columns(0.0, 1.0)
///     .pixel_rows(1.0, 0.0)
///     .display_name("gradient");
/// ```
#[derive(Debug)]
pub struct ImageMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
}

image_handle!(ImageMut, Image);

/// A handle to a colour-indexed image created by
/// [`indexed_image`](crate::AxesMut::indexed_image).
///
/// ```
/// use ironlab::prelude::*;
///
/// let classes = ByteMatrix::from_fn(8, 8, |row, col| ((row + col) % 4 * 85) as u8);
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .indexed_image(&classes)
///     .pixel_columns(0.5, 7.5)
///     .above(OutOfRange::Clamp)
///     .display_name("classes");
/// ```
#[derive(Debug)]
pub struct IndexedImageMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
}

image_handle!(IndexedImageMut, IndexedImage);

impl IndexedImageMut<'_> {
    /// Sets what is drawn for a pixel whose truncated index is less than 0.
    ///
    /// A [`Color`](crate::Color) gives the fixed-colour policy [`OutOfRange::Rgba`].
    /// [`OutOfRange::Strict`] makes such a pixel a validation error,
    /// [`OutOfRange::Transparent`], the default, draws nothing, and
    /// [`OutOfRange::Clamp`] draws the first entry of the colormap.
    pub fn below(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        self.artist().below = policy.into();
        self
    }

    /// Sets what is drawn for a pixel whose truncated index is greater than 255.
    ///
    /// The policies are as for [`below`](Self::below), except that a clamp draws the
    /// last entry of the colormap.
    pub fn above(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        self.artist().above = policy.into();
        self
    }

    /// Sets what is drawn for a pixel whose index is NaN or infinite, which only
    /// floating-point indices can hold.
    ///
    /// The policies are as for [`below`](Self::below), except that a clamp draws
    /// nothing, because a non-finite index has no nearest end of the colormap.
    pub fn non_finite(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        self.artist().non_finite = policy.into();
        self
    }
}

/// A handle to a colour-mapped image created by
/// [`mapped_image`](crate::AxesMut::mapped_image).
///
/// ```
/// use ironlab::prelude::*;
///
/// let field = Matrix::from_fn(20, 30, |row, col| (row as f64 - 10.0) * (col as f64 - 15.0));
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .mapped_image(&field)
///     .plane(ImagePlane::Xz { y: Some(0.0) })
///     .below(Color::BLACK)
///     .above(Color::WHITE)
///     .non_finite(OutOfRange::Strict);
/// ```
#[derive(Debug)]
pub struct MappedImageMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
}

image_handle!(MappedImageMut, MappedImage);

impl MappedImageMut<'_> {
    /// Sets what is drawn for a pixel whose value is less than the lower colour limit.
    ///
    /// A [`Color`](crate::Color) gives the fixed-colour policy [`OutOfRange::Rgba`].
    /// [`OutOfRange::Strict`] makes such a pixel a validation error,
    /// [`OutOfRange::Transparent`], the default, draws nothing, and
    /// [`OutOfRange::Clamp`] draws the first entry of the colormap.
    pub fn below(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        self.artist().below = policy.into();
        self
    }

    /// Sets what is drawn for a pixel whose value is greater than the upper colour
    /// limit.
    ///
    /// The policies are as for [`below`](Self::below), except that a clamp draws the
    /// last entry of the colormap.
    pub fn above(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        self.artist().above = policy.into();
        self
    }

    /// Sets what is drawn for a pixel whose value is NaN or infinite.
    ///
    /// The policies are as for [`below`](Self::below), except that a clamp draws
    /// nothing, because a non-finite value has no nearest end of the colormap.
    pub fn non_finite(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        self.artist().non_finite = policy.into();
        self
    }
}

/// Converts the axes that holds an image to three dimensions when the image is placed
/// on a wall of the axes (the xz or yz plane), through the path that `surf` uses, so
/// that an axes that is already three-dimensional keeps its view. The xy plane leaves
/// the axes as it is.
fn make_3d_for_wall(fig: &mut Figure, id: NodeId, plane: ImagePlane) {
    if matches!(plane, ImagePlane::Xy { .. }) {
        return;
    }
    let axes_id = fig
        .artist(id)
        .map(|(axes, _)| axes.id)
        .expect("an image handle always refers to an artist of its figure");
    AxesMut::new(fig, axes_id).make_3d();
}

/// Removes an array that a setter has replaced from the figure's data table, unless
/// another artist still refers to it (as it may in a figure built by hand).
fn release_data(fig: &mut Figure, id: DataId) {
    let referenced = fig
        .axes
        .iter()
        .flat_map(|axes| &axes.artists)
        .any(|artist| data_ids(artist).contains(&id));
    if !referenced {
        fig.data.remove(&id);
    }
}

/// Returns every data identifier that an artist refers to.
fn data_ids(artist: &Artist) -> Vec<DataId> {
    let grid_ids = |grid: &ironlab_ir::Grid| match *grid {
        ironlab_ir::Grid::Rectilinear { x, y } | ironlab_ir::Grid::Curvilinear { x, y } => [x, y],
    };
    match artist {
        Artist::Line(Line { x, y, z, .. }) => [*x, *y].into_iter().chain(*z).collect(),
        Artist::Scatter(Scatter {
            x,
            y,
            z,
            size,
            color,
            ..
        }) => {
            let size = match size {
                ScatterSize::Data { data } => Some(*data),
                ScatterSize::Scalar { .. } => None,
            };
            let color = match color {
                ScatterColor::Data { data } => Some(*data),
                ScatterColor::Spec { .. } => None,
            };
            [*x, *y]
                .into_iter()
                .chain(*z)
                .chain(size)
                .chain(color)
                .collect()
        }
        Artist::Contour(Contour { grid, z, .. }) => {
            grid_ids(grid).into_iter().chain([*z]).collect()
        }
        Artist::Quiver(Quiver {
            x, y, z, u, v, w, ..
        }) => [*x, *y, *u, *v].into_iter().chain(*z).chain(*w).collect(),
        Artist::Surface(Surface { grid, z, c, .. }) => {
            grid_ids(grid).into_iter().chain([*z]).chain(*c).collect()
        }
        Artist::Image(Image { pixels, .. }) => vec![*pixels],
        Artist::IndexedImage(IndexedImage { indices, .. }) => vec![*indices],
        Artist::MappedImage(MappedImage { values, .. }) => vec![*values],
    }
}
