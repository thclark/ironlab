//! Handles to artists, with chainable property setters.
//!
//! Each handle borrows the figure mutably and refers to its artist by node identifier.
//! Setters write directly to the artist in the figure IR and return the handle, so that
//! several properties can be set in one expression.

use ironlab_ir::{
    Artist, Contour, DashStyle, DataId, Figure, Image, IndexedImage, Levels, Line, MappedImage,
    MarkerShape, NdArray, NodeId, Quiver, QuiverScale, Scatter, ScatterColor, ScatterSize, Surface,
    Text,
};

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
