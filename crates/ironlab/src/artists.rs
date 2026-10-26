//! Handles to artists, with chainable property setters.
//!
//! Each handle borrows the figure mutably and refers to its artist by node identifier.
//! Setters write directly to the artist in the figure IR and return the handle, so that
//! several properties can be set in one expression.

use ironlab_ir::{
    Artist, Bar, BarOffset, BarWidth, CellEdges, CellGap, Cells, Color, ColorSpec, Contour, Coords,
    DashStyle, Figure, Image, ImagePlane, Levels, MarkerShape, NodeId, Orientation, PerBar,
    PixelRange, Quiver, QuiverScale, ScalarImage, Series, Stairs, Surface, Text,
};

use crate::axes::AxesMut;
use crate::color::IntoColorSpec;
use crate::color_scale;
use crate::data::DataSource;

/// Defines the accessors of an artist handle's artist.
macro_rules! artist_access {
    ($handle:ident, $variant:ident) => {
        impl $handle<'_> {
            /// Returns the artist in the figure IR.
            #[allow(dead_code)]
            fn get(&self) -> &$variant {
                match self.fig.artist(self.id) {
                    Some((_, Artist::$variant(artist))) => artist,
                    _ => unreachable!(concat!(
                        "a ",
                        stringify!($handle),
                        " handle always refers to an artist of its kind"
                    )),
                }
            }

            /// Returns the artist in the figure IR, mutably.
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

/// Defines the constructor of an artist handle and the accessors of its artist; with
/// `scaled`, also the colour scale setter of an artist that maps numbers to colours, and a
/// constructor that records the default scale the plotting call created, if any.
macro_rules! artist_handle {
    ($handle:ident, $variant:ident) => {
        artist_access!($handle, $variant);

        impl<'a> $handle<'a> {
            /// Creates a handle to the artist with the given identifier, which must be
            /// an artist of this kind in the figure.
            pub(crate) fn new(fig: &'a mut Figure, id: NodeId) -> Self {
                Self { fig, id }
            }
        }
    };
    ($handle:ident, $variant:ident, scaled) => {
        artist_access!($handle, $variant);

        impl<'a> $handle<'a> {
            /// Creates a handle to the artist with the given identifier, which must be
            /// an artist of this kind in the figure, recording the colour scale that its
            /// plotting call created, if any.
            pub(crate) fn new(
                fig: &'a mut Figure,
                id: NodeId,
                created_scale: Option<NodeId>,
            ) -> Self {
                Self {
                    fig,
                    id,
                    created_scale,
                }
            }
        }

        impl $handle<'_> {
            /// Maps the numbers of the artist to colours through the colour scale with
            /// the given identifier, in place of the default scale of its axes.
            ///
            /// The identifier is that of a scale created with
            /// [`Figure::color_scale`](crate::Figure::color_scale),
            /// [`Figure::indexed_scale`](crate::Figure::indexed_scale),
            /// [`Figure::categorical_scale`](crate::Figure::categorical_scale) or
            /// [`AxesMut::color_scale`](crate::AxesMut::color_scale); a scale of any kind
            /// is accepted. Artists in several axes that name one scale share its limits
            /// and colours. When the plotting call that returned this handle created the
            /// default scale of the axes, and nothing else names that scale any more, it
            /// is deleted, so the figure keeps no scale the author did not ask for; a
            /// scale the author created is never deleted. An identifier that names no
            /// colour scale is reported by [`Figure::validate`](crate::Figure::validate).
            ///
            /// ```
            /// use ironlab::prelude::*;
            ///
            /// let x = linspace(0.0, 1.0, 11);
            /// let z = Matrix::from_fn(x.len(), x.len(), |row, col| x[col] * x[row]);
            ///
            /// let mut fig = Figure::new();
            /// let heat = fig.color_scale().colormap(Colormap::Inferno).id();
            /// fig.axes(0, 0).surface(&x, &x, &z).color_scale(heat);
            /// assert_eq!(fig.ir().color_scales.len(), 1);
            /// ```
            pub fn color_scale(&mut self, scale: NodeId) -> &mut Self {
                self.created_scale =
                    color_scale::replace_scale(self.fig, self.id, scale, self.created_scale);
                self
            }
        }
    };
}

artist_handle!(ContourMut, Contour, scaled);
artist_handle!(BarMut, Bar, scaled);
artist_handle!(StairsMut, Stairs);
artist_handle!(QuiverMut, Quiver);
artist_handle!(SurfaceMut, Surface, scaled);
artist_handle!(CellsMut, Cells, scaled);

/// Defines the setters that every kind of image has: its label, the placement of its
/// pixel centres, its plane and its opacity.
macro_rules! image_handle {
    ($handle:ident) => {
        impl $handle<'_> {
            /// Returns the node identifier of the image.
            #[must_use]
            pub fn id(&self) -> NodeId {
                self.id
            }

            /// Sets the label of the image, drawn beside its sample in a legend.
            pub fn label(&mut self, label: impl Into<Text>) -> &mut Self {
                self.artist().label = Some(label.into());
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

            /// Sets the opacity of the image, from 0 (transparent) to 1 (opaque, the
            /// default), which multiplies the alpha of every pixel, so that the whole
            /// image fades without its data being regenerated.
            ///
            /// An opacity outside that range, or one that is not finite, is reported by
            /// [`Figure::validate`](crate::Figure::validate).
            pub fn opacity(&mut self, opacity: f64) -> &mut Self {
                self.artist().opacity = opacity;
                self
            }
        }
    };
}

/// A handle to a series created by [`plot`](crate::AxesMut::plot),
/// [`scatter`](crate::AxesMut::scatter) or one of their relatives.
///
/// A series is a sequence of points drawn as a line through them, a marker at each, or
/// both. The plotting call sets the defaults of its family: [`plot`](crate::AxesMut::plot)
/// draws a solid line without markers, and [`scatter`](crate::AxesMut::scatter) draws
/// unfilled circles without a line. Every setter applies to a series of either family, so
/// that a scatter can be joined by a line and a plot can be given markers.
///
/// ```
/// use ironlab::prelude::*;
///
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .plot([0.0, 1.0, 2.0], [0.0, 1.0, 4.0])
///     .label("$x^2$")
///     .color(Color::rgb(0.0, 0.45, 0.7))
///     .line_width(1.5)
///     .dash(Dash::DashDot)
///     .marker(Marker::Square)
///     .marker_size(5.0)
///     .marker_face(Color::WHITE);
/// ```
///
/// A series coloured and sized by data:
///
/// ```
/// use ironlab::prelude::*;
///
/// let x = [0.0, 1.0, 2.0];
/// let y = [2.0, 0.0, 1.0];
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .scatter(x, y)
///     .size_data([3.0, 6.0, 9.0])
///     .color_data(y)
///     .marker(Marker::Diamond)
///     .filled();
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct SeriesMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
    /// The default colour scale of the axes that this handle created, if any.
    created_scale: Option<NodeId>,
}

artist_handle!(SeriesMut, Series, scaled);

impl SeriesMut<'_> {
    /// Returns the node identifier of the series.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the identifier of the data array of the x values, which another plotting
    /// call accepts to share it.
    #[must_use]
    pub fn x_data(&self) -> NodeId {
        self.get().x
    }

    /// Returns the identifier of the data array of the y values, which another plotting
    /// call accepts to share it.
    #[must_use]
    pub fn y_data(&self) -> NodeId {
        self.get().y
    }

    /// Returns the identifier of the data array of the z values, or `None` for a
    /// two-dimensional series.
    #[must_use]
    pub fn z_data(&self) -> Option<NodeId> {
        self.get().z
    }

    /// Sets the label of the series, drawn beside its sample in a legend.
    pub fn label(&mut self, label: impl Into<Text>) -> &mut Self {
        self.artist().label = Some(label.into());
        self
    }

    /// Sets the colour of the line, which is the colour of the series: markers whose
    /// face or edge colour is automatic follow it.
    ///
    /// A fixed colour set after [`color_data`](Self::color_data) keeps the colour data
    /// and its scale, so that a series can have a line of one colour and markers
    /// coloured by data, given by
    /// [`marker_face`](Self::marker_face)`(ColorSpec::Colormapped)` or
    /// [`marker_edge`](Self::marker_edge)`(ColorSpec::Colormapped)`.
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
    /// the markers, and any other pattern joins the markers of a scatter by a line.
    pub fn dash(&mut self, dash: DashStyle) -> &mut Self {
        self.artist().line.dash = dash;
        self
    }

    /// Sets the shape of the markers drawn at the points;
    /// [`Marker::None`](crate::Marker::None) draws no markers.
    pub fn marker(&mut self, shape: MarkerShape) -> &mut Self {
        self.artist().marker.shape = shape;
        self
    }

    /// Sets the diameter of every marker in points, and of the marker of the legend key.
    ///
    /// Markers given their own sizes by [`size_data`](Self::size_data) take those sizes
    /// instead, and the legend key keeps this one.
    pub fn marker_size(&mut self, size_pt: f64) -> &mut Self {
        self.artist().marker.size_pt = size_pt;
        self
    }

    /// Sets the colour of the marker interiors, which are unfilled by default; an
    /// automatic face takes the colour of the series.
    pub fn marker_face(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().marker.face = color.into_color_spec();
        self
    }

    /// Sets the colour of the marker outlines, which follow the colour of the series by
    /// default.
    pub fn marker_edge(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().marker.edge = color.into_color_spec();
        self
    }

    /// Fills the markers with the colour of the series (MATLAB's
    /// `scatter(..., 'filled')`), which is the colour of each point's colour data when
    /// the series has some.
    pub fn filled(&mut self) -> &mut Self {
        self.artist().marker.face = ironlab_ir::ColorSpec::Auto;
        self
    }

    /// Gives each marker its own diameter in points, one value per point (MATLAB's
    /// `scatter(x, y, sz)`).
    ///
    /// The [`marker_size`](Self::marker_size) remains the size of the marker of the
    /// legend key. Values of another number than the points are reported by
    /// [`Figure::validate`](crate::Figure::validate). Data that this call replaces is
    /// released from the figure when nothing else uses it and it has no name.
    pub fn size_data(&mut self, sizes_pt: impl Into<DataSource>) -> &mut Self {
        let data = sizes_pt.into().store(self.fig);
        if let Some(previous) = self.artist().size_data.replace(data) {
            release_data(self.fig, previous);
        }
        self
    }

    /// Colours the series by data mapped through a colour scale (MATLAB's
    /// `scatter(x, y, sz, c)`, and a line coloured along its length).
    ///
    /// The values are either one per point or one per segment, which is one fewer than
    /// the points. With one value per point, each marker takes the colour of its point
    /// and each segment of the line is drawn as a gradient between the colours of its two
    /// points. With one value per segment, each segment takes the colour of its value;
    /// this form needs a line, and markers coloured by data need values per point, so
    /// [`Figure::validate`](crate::Figure::validate) refuses it on a series that draws no
    /// line or whose marker face or edge is `Colormapped`.
    ///
    /// The line colour becomes `Colormapped`, so the line and the markers that follow its
    /// colour are coloured by the data; [`color`](Self::color) called afterwards gives the
    /// line a fixed colour and keeps the data. A series that names no colour scale yet
    /// takes the default scale of its axes (see
    /// [`AxesMut::color_scale`](crate::AxesMut::color_scale)), which is created if the
    /// axes has none; [`color_scale`](Self::color_scale) gives it another. Data that this
    /// call replaces is released from the figure when nothing else uses it and it has no
    /// name.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// // A trajectory coloured by its speed, one value per point.
    /// let t = linspace(0.0, 2.0, 41);
    /// let x: Vec<f64> = t.iter().map(|t| 10.0 * t).collect();
    /// let y: Vec<f64> = t.iter().map(|t| 10.0 * t - 4.905 * t * t).collect();
    /// let speed: Vec<f64> = t.iter().map(|t| 10.0_f64.hypot(10.0 - 9.81 * t)).collect();
    ///
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// ax.plot(&x, &y).color_data(&speed).line_width(2.0);
    /// ax.color_scale().colormap(Colormap::Plasma).title("Speed (m/s)");
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn color_data(&mut self, values: impl Into<DataSource>) -> &mut Self {
        let data = values.into().store(self.fig);
        let series = self.artist();
        series.line.color = ironlab_ir::ColorSpec::Colormapped;
        if let Some(previous) = series.color_data.replace(data) {
            release_data(self.fig, previous);
        }
        if self.get().color_scale.is_none() {
            let axes = self
                .fig
                .artist(self.id)
                .map(|(axes, _)| axes.id)
                .expect("a series handle always refers to an artist of its figure");
            let (scale, created) = color_scale::default_or_new(self.fig, axes);
            self.artist().color_scale = Some(scale);
            if created {
                self.created_scale = Some(scale);
            }
        }
        self
    }

    /// Sets how the x values are placed: as data along the x axis (the default), or as
    /// fractions of the plot area's width, 0 at its left edge and 1 at its right edge,
    /// whatever the limits, scale or pan and zoom of the axis.
    ///
    /// Values in axes-fraction coordinates take no part in the automatic x limits. Only
    /// a two-dimensional axes accepts them; [`Figure::validate`](crate::Figure::validate)
    /// reports them as an error in a three-dimensional one.
    pub fn x_coords(&mut self, coords: Coords) -> &mut Self {
        self.artist().x_coords = coords;
        self
    }

    /// Sets how the y values are placed: as data along the y axis (the default), or as
    /// fractions of the plot area's height, 0 at its bottom edge and 1 at its top edge,
    /// as [`x_coords`](Self::x_coords) places x values.
    pub fn y_coords(&mut self, coords: Coords) -> &mut Self {
        self.artist().y_coords = coords;
        self
    }

    /// Sets whether the series is clipped to the plot area (the default), or may draw
    /// beyond it along a dimension in axes-fraction coordinates, for example markers
    /// that sit on the top edge of the axes.
    ///
    /// Releasing the clip never releases it along a dimension in data coordinates: a
    /// marker at a data x outside the x limits is not drawn. A series in data
    /// coordinates along both dimensions is therefore always clipped, as is every
    /// artist of a three-dimensional axes, and
    /// [`Figure::validate`](crate::Figure::validate) warns that the release has no
    /// effect.
    pub fn clip(&mut self, clip: bool) -> &mut Self {
        self.artist().clip = clip;
        self
    }
}

/// A handle to contours created by [`contour`](crate::AxesMut::contour),
/// [`contourf`](crate::AxesMut::contourf) or [`contour3`](crate::AxesMut::contour3).
#[derive(Debug)]
pub struct ContourMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
    /// The default colour scale of the axes that the plotting call created, if any.
    created_scale: Option<NodeId>,
}

impl ContourMut<'_> {
    /// Returns the node identifier of the contours.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the identifier of the data array of the field, which another plotting call
    /// accepts to share it.
    #[must_use]
    pub fn z_data(&self) -> NodeId {
        self.get().z
    }

    /// Sets the label of the contours, drawn beside its sample in a legend.
    pub fn label(&mut self, label: impl Into<Text>) -> &mut Self {
        self.artist().label = Some(label.into());
        self
    }

    /// Chooses approximately `count` levels at nice values spanning the data range.
    ///
    /// Without levels of its own, a contour takes the band edges of a discrete colour
    /// scale, and ten automatic levels through a scale of any other kind.
    pub fn levels(&mut self, count: u32) -> &mut Self {
        self.artist().levels = Some(Levels::Auto { count });
        self
    }

    /// Draws contours at the given levels, which must be finite and increasing.
    ///
    /// Levels that are not are reported by
    /// [`Figure::validate`](crate::Figure::validate). A filled contour whose levels
    /// should match the bands of its colour scale takes them from a discrete scale
    /// instead (see [`ColorScaleMut::discrete`](crate::ColorScaleMut::discrete)).
    pub fn level_values(&mut self, levels: impl AsRef<[f64]>) -> &mut Self {
        self.artist().levels = Some(Levels::Explicit {
            values: levels.as_ref().to_vec(),
        });
        self
    }

    /// Sets the width of the isolines in points.
    pub fn line_width(&mut self, width_pt: f64) -> &mut Self {
        self.artist().line.width_pt = width_pt;
        self
    }

    /// Sets the colour of the isolines, which are coloured through the colour scale by
    /// level by default.
    pub fn color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().line.color = color.into_color_spec();
        self
    }

    /// Sets the opacity of the isolines, from 0 (transparent) to 1 (opaque, the
    /// default), which multiplies the alpha of their colour.
    ///
    /// An opacity outside that range, or one that is not finite, is reported by
    /// [`Figure::validate`](crate::Figure::validate).
    pub fn line_opacity(&mut self, opacity: f64) -> &mut Self {
        self.artist().line_opacity = opacity;
        self
    }

    /// Sets the opacity of the filled bands (MATLAB's scalar `FaceAlpha` of `contourf`),
    /// from 0 (transparent) to 1 (opaque, the default), which multiplies the alpha of
    /// their colour; it has no effect on contours that are not filled.
    ///
    /// An opacity outside that range, or one that is not finite, is reported by
    /// [`Figure::validate`](crate::Figure::validate).
    pub fn fill_opacity(&mut self, opacity: f64) -> &mut Self {
        self.artist().fill_opacity = opacity;
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

    /// Sets the label of the arrows, drawn beside its sample in a legend.
    pub fn label(&mut self, label: impl Into<Text>) -> &mut Self {
        self.artist().label = Some(label.into());
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

/// A handle to a surface created by [`surf`](crate::AxesMut::surf),
/// [`mesh`](crate::AxesMut::mesh) or [`surface`](crate::AxesMut::surface).
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
///     .label("$xy$");
/// ```
#[derive(Debug)]
pub struct SurfaceMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
    /// The default colour scale of the axes that the plotting call created, if any.
    created_scale: Option<NodeId>,
}

impl SurfaceMut<'_> {
    /// Returns the node identifier of the surface.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the identifier of the data array of the field, which another plotting call
    /// accepts to share it.
    #[must_use]
    pub fn z_data(&self) -> NodeId {
        self.get().z
    }

    /// Sets the label of the surface, drawn beside its sample in a legend.
    pub fn label(&mut self, label: impl Into<Text>) -> &mut Self {
        self.artist().label = Some(label.into());
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

    /// Colours the surface by the given values instead of by its field `z` (MATLAB's
    /// `surf(x, y, z, c)`); the matrix must have the same shape as the field.
    pub fn color_data(&mut self, c: impl Into<DataSource>) -> &mut Self {
        let data = c.into().store(self.fig);
        if let Some(previous) = self.artist().c.replace(data) {
            release_data(self.fig, previous);
        }
        self
    }

    /// Gives the faces an alpha at every node (MATLAB's `AlphaData` with interpolated
    /// face alpha), from 0 (transparent) to 1 (opaque); the matrix must have the same
    /// shape as the field.
    ///
    /// A face takes the mean of the alpha at its four corners, each clamped to the range
    /// 0 to 1, and a face with a NaN corner is transparent. The alpha multiplies the
    /// alpha of the face colour and is multiplied in turn by the
    /// [`face_opacity`](Self::face_opacity).
    pub fn face_alpha(&mut self, alpha: impl Into<DataSource>) -> &mut Self {
        let data = alpha.into().store(self.fig);
        if let Some(previous) = self.artist().face_alpha.replace(data) {
            release_data(self.fig, previous);
        }
        self
    }

    /// Gives the face edges an alpha at every node, as [`face_alpha`](Self::face_alpha)
    /// gives the faces theirs: the edges of a face take the mean of the alpha at its
    /// corners, multiplied by the alpha of the edge colour and by the
    /// [`edge_opacity`](Self::edge_opacity).
    pub fn edge_alpha(&mut self, alpha: impl Into<DataSource>) -> &mut Self {
        let data = alpha.into().store(self.fig);
        if let Some(previous) = self.artist().edge_alpha.replace(data) {
            release_data(self.fig, previous);
        }
        self
    }

    /// Sets the opacity of the faces (MATLAB's scalar `FaceAlpha`), from 0 (transparent)
    /// to 1 (opaque, the default), which multiplies the alpha of every face.
    ///
    /// An opacity outside that range, or one that is not finite, is reported by
    /// [`Figure::validate`](crate::Figure::validate).
    pub fn face_opacity(&mut self, opacity: f64) -> &mut Self {
        self.artist().face_opacity = opacity;
        self
    }

    /// Sets the opacity of the face edges (MATLAB's scalar `EdgeAlpha`), from 0
    /// (transparent) to 1 (opaque, the default), which multiplies the alpha of every
    /// edge, as [`face_opacity`](Self::face_opacity) does for the faces.
    pub fn edge_opacity(&mut self, opacity: f64) -> &mut Self {
        self.artist().edge_opacity = opacity;
        self
    }
}

/// A handle to a bar series created by [`bar`](crate::AxesMut::bar) or
/// [`barh`](crate::AxesMut::barh).
///
/// A bar series draws one rectangle for each position, which runs from its base for the
/// length of its value, so that a negative value extends the other way. Its setters choose
/// the width of the bars, their offset from their positions, their base, the margin that
/// the automatic limits leave beyond their ends, and the colours of the faces and the
/// outlines, which can be given per bar with [`colors`](Self::colors) or by data with
/// [`color_data`](Self::color_data). Grouped and stacked bars are series whose widths,
/// offsets and bases the [`bars`](crate::AxesMut::bars) helper writes.
///
/// Floating bars, each running from its own base for its length:
///
/// ```
/// use ironlab::prelude::*;
///
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .bar([0.0, 1.0, 2.0], [4.0, 3.0, 2.0])
///     .base_data([1.0, 2.0, -1.0])
///     .label("Ranges");
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct BarMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
    /// The colour scale that this handle created, if any.
    created_scale: Option<NodeId>,
}

impl BarMut<'_> {
    /// Returns the node identifier of the bar series.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets the label of the bar series, drawn beside its sample in a legend.
    pub fn label(&mut self, label: impl Into<Text>) -> &mut Self {
        self.artist().label = Some(label.into());
        self
    }

    /// Makes every bar the given fraction of the pitch wide, the pitch being the smallest
    /// distance between neighbouring distinct positions (MATLAB's `bar(x, y, width)`): 0.55
    /// by default, and 1 for bars that touch.
    pub fn width_pitch(&mut self, fraction: f64) -> &mut Self {
        self.set_width(BarWidth::Pitch(fraction));
        self
    }

    /// Makes every bar the same width, in data units (Matplotlib's `bar(x, y, width)`).
    pub fn width(&mut self, width: f64) -> &mut Self {
        self.set_width(BarWidth::Constant(width));
        self
    }

    /// Gives each bar a width of its own, in data units, one value per bar: the widths of
    /// the bins of a histogram of unequal bins. Data that this call replaces is released
    /// from the figure when nothing else uses it and it has no name.
    pub fn widths(&mut self, widths: impl Into<DataSource>) -> &mut Self {
        let data = widths.into().store(self.fig);
        self.set_width(BarWidth::Data(data));
        self
    }

    /// Centres every bar the given multiple of its own width from its position, negative
    /// to the low side, so that the bar stays beside its position whatever its width; this
    /// is how the columns of a grouped chart stand side by side.
    pub fn offset_widths(&mut self, widths: f64) -> &mut Self {
        self.artist().offset = BarOffset::Widths(widths);
        self
    }

    /// Centres every bar the given distance from its position, in data units along the
    /// category axis, whatever its width.
    pub fn offset(&mut self, offset: f64) -> &mut Self {
        self.artist().offset = BarOffset::Absolute(offset);
        self
    }

    /// Makes every bar start at the same value along the value axis (MATLAB's
    /// `BaseValue`), which is 0 by default; a negative value hangs down from it.
    pub fn base(&mut self, base: f64) -> &mut Self {
        self.set_base(PerBar::Constant(base));
        self
    }

    /// Makes each bar start at its own value, one per bar, which draws floating bars,
    /// such as ranges or the steps of a waterfall, and the parts of a stack. Data that
    /// this call replaces is released from the figure when nothing else uses it and it has
    /// no name.
    pub fn base_data(&mut self, bases: impl Into<DataSource>) -> &mut Self {
        let data = bases.into().store(self.fig);
        self.set_base(PerBar::Data(data));
        self
    }

    /// Sets the fraction of the span of the bars along the value axis that the automatic
    /// limits add beyond the ends of the bars, away from their base, so that the longest
    /// bar does not touch the box: 0.05 by default, and 0 for limits that end at the
    /// longest bar. Manual limits ignore it.
    pub fn margin(&mut self, margin: f64) -> &mut Self {
        self.artist().margin = margin;
        self
    }

    /// Sets the colour of the bars; `None` leaves them unfilled, and an automatic colour
    /// is the next colour of the colour order.
    pub fn face_color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().face = color.into_color_spec();
        self
    }

    /// Sets the colour of the outlines of the bars, which are black by default; `None`
    /// hides them, and an automatic colour is the colour of the series.
    pub fn edge_color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().edge.color = color.into_color_spec();
        self
    }

    /// Sets the width of the outlines of the bars in points.
    pub fn edge_width(&mut self, width_pt: f64) -> &mut Self {
        self.artist().edge.width_pt = width_pt;
        self
    }

    /// Sets the dash pattern of the outlines of the bars.
    pub fn edge_dash(&mut self, dash: DashStyle) -> &mut Self {
        self.artist().edge.dash = dash;
        self
    }

    /// Sets the opacity of the faces, from 0 (transparent) to 1 (opaque, the default),
    /// which multiplies the alpha of their colours.
    pub fn face_opacity(&mut self, opacity: f64) -> &mut Self {
        self.artist().face_opacity = opacity;
        self
    }

    /// Sets the opacity of the outlines, from 0 (transparent) to 1 (opaque, the
    /// default), which multiplies the alpha of their colours.
    pub fn edge_opacity(&mut self, opacity: f64) -> &mut Self {
        self.artist().edge_opacity = opacity;
        self
    }

    /// Colours the bars by data mapped through a colour scale, one value per bar, as
    /// [`SeriesMut::color_data`] colours markers.
    ///
    /// The face becomes `Colormapped`. Bars that name no colour scale yet take the
    /// default scale of their axes (see
    /// [`AxesMut::color_scale`](crate::AxesMut::color_scale)), which is created if the
    /// axes has none; [`color_scale`](Self::color_scale) gives them another. Data that
    /// this call replaces is released from the figure when nothing else uses it and it
    /// has no name.
    pub fn color_data(&mut self, values: impl Into<DataSource>) -> &mut Self {
        let data = values.into().store(self.fig);
        self.replace_color_data(data);
        if self.get().color_scale.is_none() {
            let axes = self.axes_id();
            let (scale, created) = color_scale::default_or_new(self.fig, axes);
            self.artist().color_scale = Some(scale);
            if created {
                self.created_scale = Some(scale);
            }
        }
        self
    }

    /// Gives each bar its own colour, one per bar, such as a colour that highlights one
    /// bar among grey ones.
    ///
    /// The colours are stored as the model colours individual bars: an indexed colour
    /// scale whose entries are the distinct colours, in the order they first appear, and,
    /// as colour data, the index of each bar's colour in it. The face becomes
    /// `Colormapped`. Colours given again replace the scale and the data this call
    /// created, and a colour scale that the bars named before is replaced as
    /// [`color_scale`](Self::color_scale) replaces it.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let grey = Color::rgb(0.6, 0.6, 0.6);
    /// let red = Color::rgb(0.8, 0.1, 0.1);
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0)
    ///     .bar([0.0, 1.0, 2.0, 3.0], [3.0, 1.0, 4.0, 2.0])
    ///     .colors([grey, grey, red, grey]);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn colors(&mut self, colors: impl IntoIterator<Item = Color>) -> &mut Self {
        let mut palette: Vec<Color> = Vec::new();
        let indices: Vec<f64> = colors
            .into_iter()
            .map(|colour| {
                let index = palette
                    .iter()
                    .position(|c| *c == colour)
                    .unwrap_or_else(|| {
                        palette.push(colour);
                        palette.len() - 1
                    });
                index as f64
            })
            .collect();
        let data = DataSource::from(indices).store(self.fig);
        self.replace_color_data(data);
        let scale = color_scale::ColorScaleMut::add(self.fig, color_scale::indexed(palette)).id();
        self.created_scale =
            color_scale::replace_scale(self.fig, self.id, scale, self.created_scale);
        self.created_scale = Some(scale);
        self
    }

    /// Returns the identifier of the axes of the bars.
    fn axes_id(&self) -> NodeId {
        self.fig
            .artist(self.id)
            .map(|(axes, _)| axes.id)
            .expect("a bar handle always refers to an artist of its figure")
    }

    /// Stores colour data and makes the face colormapped, releasing the data it replaces.
    fn replace_color_data(&mut self, data: NodeId) {
        let bar = self.artist();
        bar.face = ColorSpec::Colormapped;
        if let Some(previous) = bar.color_data.replace(data) {
            release_data(self.fig, previous);
        }
    }

    /// Sets the width, releasing the widths it replaces.
    fn set_width(&mut self, width: BarWidth) {
        if let BarWidth::Data(previous) = std::mem::replace(&mut self.artist().width, width) {
            release_data(self.fig, previous);
        }
    }

    /// Sets the base, releasing the bases it replaces.
    fn set_base(&mut self, base: PerBar) {
        set_bar_base(self.fig, self.id, base);
    }
}

/// Sets the base of the bar series `id`, releasing the bases it replaces.
fn set_bar_base(fig: &mut Figure, id: NodeId, base: PerBar) {
    let Some(Artist::Bar(bar)) = fig.artist_mut(id) else {
        unreachable!("a bar handle always refers to a bar series of its figure");
    };
    if let PerBar::Data(previous) = std::mem::replace(&mut bar.base, base) {
        release_data(fig, previous);
    }
}

/// The helper that writes grouped and stacked bars into ordinary bar series, created by
/// [`bars`](crate::AxesMut::bars).
///
/// The helper holds one set of positions and adds bar series at them, each with its own
/// label, values and colour from the colour order. A series added by
/// [`series`](Self::series) is a column of its own; a series added by
/// [`stacked`](Self::stacked) joins the column of the name it is given, in which the parts
/// rest on one another, positive and negative values apart (Plotly's relative mode). The
/// columns stand side by side at each position in the order they first appear.
///
/// Every call rewrites the width, the offset and, for a stacked column, the base of every
/// series the helper holds, so that the chart does not depend on the order of the calls.
/// For `m` columns, each series is a fraction `(1 − gap)(1 − group_gap) / m` of the pitch
/// wide, and the series of column `k` (from 0) are offset by
/// `(k − (m − 1) / 2) / (1 − group_gap)` of their width, so that the cluster is centred on
/// its position, neighbouring columns are the fraction `group_gap` of a column apart, and
/// the fraction `gap` of the pitch is left between clusters. A part of a stack rests on the
/// sum of the parts of its sign before it at the same index, a part that is not finite
/// contributing nothing. Styles set through [`last`](Self::last) or [`get`](Self::get) are
/// kept, because the helper rewrites only where the bars are placed (ADR 0026).
///
/// Three models compared across three properties, and gains and losses stacked:
///
/// ```
/// use ironlab::prelude::*;
///
/// let mut fig = Figure::new().tiles(1, 2);
/// let mut ax = fig.axes(0, 0);
/// ax.bars([0.0, 1.0, 2.0])
///     .series("Model A", [3.0, 4.5, 2.0])
///     .series("Model B", [2.5, 3.0, 4.0])
///     .series("Model C", [4.0, 2.0, 3.5]);
/// ax.xticks([0.0, 1.0, 2.0]).xtick_labels(["Lift", "Drag", "Mass"]);
///
/// let mut ax = fig.axes(0, 1);
/// ax.bars([0.0, 1.0])
///     .stacked("2026", "gains", [2.0, 3.0])
///     .stacked("2026", "losses", [-1.0, -2.0]);
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct BarsMut<'a> {
    fig: &'a mut Figure,
    axes: NodeId,
    /// The positions that every series of the helper shares.
    positions: NodeId,
    /// The orientation of every series.
    orientation: Orientation,
    /// The fraction of the pitch left between clusters.
    gap: f64,
    /// The fraction of a column left between neighbouring columns of a cluster.
    group_gap: f64,
    /// The margin of every series, once one has been given.
    margin: Option<f64>,
    /// The series, in the order they were added, with the column each belongs to.
    columns: BarColumns,
}

/// The column of a series of a helper that groups and stacks bar series: its own, or a
/// stack shared by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BarsColumn {
    /// A column of its own, named by the series' place among the members.
    Own(usize),
    /// A stacked column of the given name.
    Stacked(String),
}

/// The series of a helper that groups and stacks bar series, in the order they were added,
/// each with the column it belongs to: the one placement of grouped and stacked bars that
/// [`BarsMut`] and the histogram helper [`HistsMut`](crate::HistsMut) share.
#[derive(Debug, Default)]
pub(crate) struct BarColumns {
    members: Vec<(NodeId, BarsColumn)>,
}

impl BarColumns {
    /// Returns the column of the next series when it is a column of its own.
    pub(crate) fn own(&self) -> BarsColumn {
        BarsColumn::Own(self.members.len())
    }

    /// Adds a series to a column.
    pub(crate) fn push(&mut self, id: NodeId, column: BarsColumn) {
        self.members.push((id, column));
    }

    /// Returns the series, in the order they were added.
    pub(crate) fn ids(&self) -> Vec<NodeId> {
        self.members.iter().map(|(id, _)| *id).collect()
    }

    /// Returns the column of every series, in the order they were added.
    pub(crate) fn columns(&self) -> impl Iterator<Item = &BarsColumn> {
        self.members.iter().map(|(_, column)| column)
    }

    /// Returns the width of every series, as a fraction of the space of a cluster, and the
    /// offset of each series, in multiples of its own width, for clusters that leave the
    /// fraction `gap` of their space empty and columns that leave the fraction `group_gap`
    /// of their width empty between them.
    ///
    /// For `m` columns, each series is a fraction `(1 − gap)(1 − group_gap) / m` of the
    /// space wide, and the series of column `k` (from 0) are offset by
    /// `(k − (m − 1) / 2) / (1 − group_gap)` of their width, so that the cluster is centred
    /// on its position and the columns stand side by side in the order they first appear.
    pub(crate) fn placement(&self, gap: f64, group_gap: f64) -> (f64, Vec<(NodeId, f64)>) {
        let mut columns: Vec<&BarsColumn> = Vec::new();
        for (_, column) in &self.members {
            if !columns.contains(&column) {
                columns.push(column);
            }
        }
        let m = columns.len() as f64;
        let width = (1.0 - gap) * (1.0 - group_gap) / m;
        let offsets = self
            .members
            .iter()
            .map(|(id, column)| {
                let k = columns.iter().position(|c| *c == column).unwrap_or(0) as f64;
                (*id, (k - (m - 1.0) / 2.0) / (1.0 - group_gap))
            })
            .collect();
        (width, offsets)
    }

    /// Gives every series of a stacked column, whose bars number `n`, the running totals
    /// of the parts before it at each index as its bases, positive and negative values
    /// apart, a part that is not finite contributing nothing; the bases it replaces are
    /// released.
    pub(crate) fn stack(&self, fig: &mut Figure, n: usize) {
        let mut totals: Vec<(&str, Vec<f64>, Vec<f64>)> = Vec::new();
        let mut bases: Vec<(NodeId, Vec<f64>)> = Vec::new();
        for (id, column) in &self.members {
            let BarsColumn::Stacked(name) = column else {
                continue;
            };
            let at = match totals
                .iter()
                .position(|(column, ..)| *column == name.as_str())
            {
                Some(at) => at,
                None => {
                    totals.push((name.as_str(), vec![0.0; n], vec![0.0; n]));
                    totals.len() - 1
                }
            };
            let values: Vec<f64> = match fig.artist(*id) {
                Some((_, Artist::Bar(bar))) => fig
                    .data_array(bar.values)
                    .and_then(|data| data.array.as_f64())
                    .map(<[f64]>::to_vec)
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let (_, above, below) = &mut totals[at];
            let mut base = Vec::with_capacity(n);
            for k in 0..n {
                let value = values.get(k).copied().unwrap_or(f64::NAN);
                if value.is_finite() && value < 0.0 {
                    base.push(below[k]);
                    below[k] += value;
                } else {
                    base.push(above[k]);
                    if value.is_finite() {
                        above[k] += value;
                    }
                }
            }
            bases.push((*id, base));
        }
        for (id, base) in bases {
            let data = DataSource::from(base).store(fig);
            set_bar_base(fig, id, PerBar::Data(data));
        }
    }
}

impl<'a> BarsMut<'a> {
    /// Returns a helper for the axes `axes` holding no series yet, at the positions
    /// `positions`, which are stored once and shared by every series.
    pub(crate) fn new(fig: &'a mut Figure, axes: NodeId, positions: DataSource) -> Self {
        let positions = positions.store(fig);
        BarsMut {
            fig,
            axes,
            positions,
            orientation: Orientation::Vertical,
            gap: 0.45,
            group_gap: 0.0,
            margin: None,
            columns: BarColumns::default(),
        }
    }

    /// Adds a series of the given label and values as a column of its own.
    pub fn series(&mut self, label: impl Into<Text>, values: impl Into<DataSource>) -> &mut Self {
        let column = self.columns.own();
        self.add(column, label.into(), values.into())
    }

    /// Adds a series of the given label and values to the stacked column named `column`,
    /// which is created beside the columns before it when no series has named it yet. The
    /// series rests on the parts of the column added before it.
    pub fn stacked(
        &mut self,
        column: impl Into<String>,
        label: impl Into<Text>,
        values: impl Into<DataSource>,
    ) -> &mut Self {
        let column = BarsColumn::Stacked(column.into());
        self.add(column, label.into(), values.into())
    }

    /// Sets the fraction of the pitch left empty between neighbouring clusters, from 0 up
    /// to, but not including, 1: 0.45 by default, so that a lone column is as wide as a
    /// bar of [`bar`](crate::AxesMut::bar).
    pub fn gap(&mut self, gap: f64) -> &mut Self {
        self.gap = gap;
        self.lay_out();
        self
    }

    /// Sets the fraction of the width of a column left empty between neighbouring columns
    /// of a cluster, from 0 up to, but not including, 1: 0 by default, for columns that
    /// touch.
    pub fn group_gap(&mut self, gap: f64) -> &mut Self {
        self.group_gap = gap;
        self.lay_out();
        self
    }

    /// Lays every series flat, with the positions along y and the values along x, as
    /// [`barh`](crate::AxesMut::barh) does.
    pub fn horizontal(&mut self) -> &mut Self {
        self.orientation = Orientation::Horizontal;
        self.lay_out();
        self
    }

    /// Sets the margin of every series, as [`BarMut::margin`] sets it for one.
    pub fn margin(&mut self, margin: f64) -> &mut Self {
        self.margin = Some(margin);
        self.lay_out();
        self
    }

    /// Returns the series of the helper, in the order they were added.
    #[must_use]
    pub fn ids(&self) -> Vec<NodeId> {
        self.columns.ids()
    }

    /// Returns the handle of the series added last, for styling.
    ///
    /// # Panics
    ///
    /// Panics when the helper holds no series.
    pub fn last(&mut self) -> BarMut<'_> {
        let id = *self
            .columns
            .ids()
            .last()
            .expect("the bars helper holds a series");
        BarMut::new(self.fig, id, None)
    }

    /// Returns the handle of the series of the given index, in the order the series were
    /// added, for styling.
    ///
    /// # Panics
    ///
    /// Panics when the helper holds no series of that index.
    pub fn get(&mut self, index: usize) -> BarMut<'_> {
        let id = self.columns.ids()[index];
        BarMut::new(self.fig, id, None)
    }

    /// Adds a bar series to `column` and lays out every series again.
    fn add(&mut self, column: BarsColumn, label: Text, values: DataSource) -> &mut Self {
        let values = values.store(self.fig);
        let id = self.fig.alloc_node_id();
        let bar = Artist::Bar(Bar {
            id,
            label: Some(label),
            positions: self.positions,
            values,
            ..Bar::default()
        });
        self.fig
            .axes
            .iter_mut()
            .find(|axes| axes.id == self.axes)
            .expect("the bars helper refers to an axes of its figure")
            .artists
            .push(bar);
        self.columns.push(id, column);
        self.lay_out();
        self
    }

    /// Rewrites the width, the offset, the orientation and the margin of every series, and
    /// the base of every series of a stacked column, from the columns as they now stand.
    fn lay_out(&mut self) {
        let (width, placed) = self.columns.placement(self.gap, self.group_gap);
        for (id, offset) in placed {
            let Some(Artist::Bar(bar)) = self.fig.artist_mut(id) else {
                continue;
            };
            bar.width = BarWidth::Pitch(width);
            bar.offset = BarOffset::Widths(offset);
            bar.orientation = self.orientation;
            if let Some(margin) = self.margin {
                bar.margin = margin;
            }
        }
        let n = self
            .fig
            .data_array(self.positions)
            .map_or(0, |data| data.array.len());
        self.columns.stack(self.fig, n);
    }
}

/// A handle to stairs created by [`stairs`](crate::AxesMut::stairs) or
/// [`stairs_at`](crate::AxesMut::stairs_at).
///
/// Stairs draw a stepped outline, open by default. A [`base`](Self::base) drops the
/// outline to it at both ends, and a [`fill`](Self::fill) then fills the area between the
/// outline and the base, as a filled histogram is drawn.
///
/// ```
/// use ironlab::prelude::*;
///
/// let edges = [0.0, 1.0, 2.0, 4.0, 5.0];
/// let counts = [3.0, 7.0, 4.0, 1.0];
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .stairs(edges, counts)
///     .label("Counts")
///     .base(0.0)
///     .fill(Color::rgb(0.6, 0.75, 0.9));
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct StairsMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
}

impl StairsMut<'_> {
    /// Returns the node identifier of the stairs.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets the label of the stairs, drawn beside their sample in a legend.
    pub fn label(&mut self, label: impl Into<Text>) -> &mut Self {
        self.artist().label = Some(label.into());
        self
    }

    /// Drops the outline to the given value along the value axis at both ends, which
    /// closes it; a [`fill`](Self::fill) fills down to it.
    pub fn base(&mut self, base: f64) -> &mut Self {
        self.artist().base = Some(base);
        self
    }

    /// Removes the base, so that the outline starts and ends at the first and last values,
    /// as it does by default.
    pub fn open(&mut self) -> &mut Self {
        self.artist().base = None;
        self
    }

    /// Sets the colour of the outline, which is the next colour of the colour order by
    /// default.
    pub fn color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().line.color = color.into_color_spec();
        self
    }

    /// Sets the width of the outline in points.
    pub fn line_width(&mut self, width_pt: f64) -> &mut Self {
        self.artist().line.width_pt = width_pt;
        self
    }

    /// Sets the dash pattern of the outline; [`Dash::None`](crate::Dash::None) draws no
    /// outline, leaving only the fill.
    pub fn dash(&mut self, dash: DashStyle) -> &mut Self {
        self.artist().line.dash = dash;
        self
    }

    /// Fills the area between the outline and the base with the given colour; an
    /// automatic colour is the colour of the outline. A fill needs a [`base`](Self::base),
    /// without which [`Figure::validate`](crate::Figure::validate) reports an error.
    pub fn fill(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().fill = color.into_color_spec();
        self
    }

    /// Sets the opacity of the outline, from 0 (transparent) to 1 (opaque, the default).
    pub fn line_opacity(&mut self, opacity: f64) -> &mut Self {
        self.artist().line_opacity = opacity;
        self
    }

    /// Sets the opacity of the fill, from 0 (transparent) to 1 (opaque, the default).
    pub fn fill_opacity(&mut self, opacity: f64) -> &mut Self {
        self.artist().fill_opacity = opacity;
        self
    }

    /// Sets whether the edges or samples lie along x, with the values along y (the
    /// default), or along y, with the values along x.
    pub fn orientation(&mut self, orientation: Orientation) -> &mut Self {
        self.artist().orientation = orientation;
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
///     .label("gradient");
/// ```
#[derive(Debug)]
pub struct ImageMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
}

artist_handle!(ImageMut, Image);
image_handle!(ImageMut);

/// A handle to a scalar image created by [`scalar_image`](crate::AxesMut::scalar_image).
///
/// ```
/// use ironlab::prelude::*;
///
/// let field = Matrix::from_fn(20, 30, |row, col| (row as f64 - 10.0) * (col as f64 - 15.0));
/// let mut fig = Figure::new();
/// let signed = fig
///     .color_scale()
///     .colormap(Colormap::Coolwarm)
///     .limits(-100.0, 100.0)
///     .below(Color::BLACK)
///     .above(Color::WHITE)
///     .id();
/// fig.axes(0, 0)
///     .scalar_image(&field)
///     .color_scale(signed)
///     .plane(ImagePlane::Xz { y: Some(0.0) })
///     .label("product");
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct ScalarImageMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
    /// The default colour scale of the axes that the plotting call created, if any.
    created_scale: Option<NodeId>,
}

artist_handle!(ScalarImageMut, ScalarImage, scaled);
image_handle!(ScalarImageMut);

impl ScalarImageMut<'_> {
    /// Returns the identifier of the data array of the values, which another plotting
    /// call accepts to share it.
    #[must_use]
    pub fn values_data(&self) -> NodeId {
        self.get().values
    }

    /// Gives every pixel its own alpha (MATLAB's `AlphaData`), from 0 (transparent) to 1
    /// (opaque), as a matrix of the shape of the values.
    ///
    /// The alpha may be a [`Matrix`](crate::Matrix) of values from 0 to 1, which are
    /// clamped to that range and of which a NaN makes its pixel transparent, or a
    /// [`ByteMatrix`](crate::ByteMatrix) of values from 0 to 255. It multiplies the alpha
    /// of the colour of each pixel, including a colour given by an out-of-range policy of
    /// the colour scale, and is multiplied in turn by the [`opacity`](Self::opacity). A
    /// matrix of another shape is reported by
    /// [`Figure::validate`](crate::Figure::validate).
    pub fn alpha(&mut self, alpha: impl Into<DataSource>) -> &mut Self {
        let data = alpha.into().store(self.fig);
        if let Some(previous) = self.artist().alpha.replace(data) {
            release_data(self.fig, previous);
        }
        self
    }
}

/// A handle to cells created by [`cells`](crate::AxesMut::cells) or
/// [`occupancy`](crate::AxesMut::occupancy).
///
/// Cells are a grid of rectangles, each coloured by its value through a colour scale
/// (MATLAB's `heatmap` and `histogram2` drawn as tiles, and Matplotlib's `pcolormesh` on a
/// rectilinear grid). Cell `(j, i)` holds the value in row `j` and column `i` of the matrix
/// and lies between column edges `i` and `i + 1` along x and row edges `j` and `j + 1`
/// along y. Without explicit edges the cells are centred on whole numbers, so that cell
/// `(j, i)` is centred on `(i, j)`. The setters choose the edges, the gap that sets the
/// cells apart, their outlines, their alpha and their opacities.
///
/// A two-dimensional histogram of unequal bins:
///
/// ```
/// use ironlab::prelude::*;
///
/// let counts = Matrix::from_rows(&[[1.0, 4.0, 2.0], [3.0, 9.0, 5.0]]);
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .cells(&counts)
///     .column_edges([0.0, 1.0, 3.0, 7.0])
///     .row_edges([0.0, 0.5, 2.0])
///     .edge_color(Color::WHITE)
///     .label("Counts");
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct CellsMut<'a> {
    pub(crate) fig: &'a mut Figure,
    pub(crate) id: NodeId,
    /// The default colour scale of the axes that the plotting call created, if any.
    created_scale: Option<NodeId>,
}

impl CellsMut<'_> {
    /// Returns the node identifier of the cells.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the identifier of the data array of the values, which another plotting call
    /// accepts to share it.
    #[must_use]
    pub fn values_data(&self) -> NodeId {
        self.get().values
    }

    /// Sets the label of the cells, drawn beside their sample in a legend.
    pub fn label(&mut self, label: impl Into<Text>) -> &mut Self {
        self.artist().label = Some(label.into());
        self
    }

    /// Places the columns between the given edges along x, one more than the columns,
    /// finite and strictly monotonic, increasing or decreasing; uneven edges draw the bins
    /// of a histogram of unequal widths. Edges of another number, or that are not
    /// monotonic, are reported by [`Figure::validate`](crate::Figure::validate). Data that
    /// this call replaces is released from the figure when nothing else uses it and it has
    /// no name.
    pub fn column_edges(&mut self, edges: impl Into<DataSource>) -> &mut Self {
        let data = edges.into().store(self.fig);
        self.set_columns(CellEdges::Edges(data));
        self
    }

    /// Places the rows between the given edges along y, as
    /// [`column_edges`](Self::column_edges) places the columns. Row 0 lies between the
    /// first two edges.
    pub fn row_edges(&mut self, edges: impl Into<DataSource>) -> &mut Self {
        let data = edges.into().store(self.fig);
        self.set_rows(CellEdges::Edges(data));
        self
    }

    /// Centres the columns on whole numbers, column `k` spanning `k − 0.5` to `k + 0.5`,
    /// which is the default, releasing any explicit edges as
    /// [`column_edges`](Self::column_edges) does.
    pub fn unit_columns(&mut self) -> &mut Self {
        self.set_columns(CellEdges::Unit);
        self
    }

    /// Centres the rows on whole numbers, as [`unit_columns`](Self::unit_columns) centres
    /// the columns.
    pub fn unit_rows(&mut self) -> &mut Self {
        self.set_rows(CellEdges::Unit);
        self
    }

    /// Sets the fractions of the width and the height of each cell left empty, shared
    /// equally by the two sides of the cell and measured on the page, so that cells on a
    /// logarithmic axis stay centred in their bins. The default of 0 along both draws
    /// seamless cells; each fraction must be at least 0 and less than 1, as
    /// [`Figure::validate`](crate::Figure::validate) checks.
    pub fn gap(&mut self, x: f64, y: f64) -> &mut Self {
        self.artist().gap = CellGap { x, y };
        self
    }

    /// Sets the colour of the outlines of the cells, which have none by default; `None`
    /// hides them, and an automatic or colormapped colour gives each outline the colour of
    /// its cell.
    pub fn edge_color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.artist().edge.color = color.into_color_spec();
        self
    }

    /// Sets the width of the outlines of the cells in points.
    pub fn edge_width(&mut self, width_pt: f64) -> &mut Self {
        self.artist().edge.width_pt = width_pt;
        self
    }

    /// Gives every cell its own alpha, from 0 (transparent) to 1 (opaque), as a matrix of
    /// the shape of the values, as [`ScalarImageMut::alpha`] gives every pixel its own.
    ///
    /// The alpha multiplies the alpha of the colour of each cell and of its outline, and
    /// is multiplied in turn by the [`face_opacity`](Self::face_opacity) and the
    /// [`edge_opacity`](Self::edge_opacity); a cell whose alpha is 0 is not drawn. Data
    /// that this call replaces is released from the figure when nothing else uses it and
    /// it has no name.
    pub fn alpha(&mut self, alpha: impl Into<DataSource>) -> &mut Self {
        let data = alpha.into().store(self.fig);
        if let Some(previous) = self.artist().alpha.replace(data) {
            release_data(self.fig, previous);
        }
        self
    }

    /// Sets the opacity of the cells, from 0 (transparent) to 1 (opaque, the default),
    /// which multiplies the alpha of every cell.
    pub fn face_opacity(&mut self, opacity: f64) -> &mut Self {
        self.artist().face_opacity = opacity;
        self
    }

    /// Sets the opacity of the outlines of the cells, from 0 (transparent) to 1 (opaque,
    /// the default), which multiplies the alpha of their colour.
    pub fn edge_opacity(&mut self, opacity: f64) -> &mut Self {
        self.artist().edge_opacity = opacity;
        self
    }

    /// Sets the edges of the columns, releasing the edges they replace.
    fn set_columns(&mut self, edges: CellEdges) {
        if let CellEdges::Edges(previous) = std::mem::replace(&mut self.artist().columns, edges) {
            release_data(self.fig, previous);
        }
    }

    /// Sets the edges of the rows, releasing the edges they replace.
    fn set_rows(&mut self, edges: CellEdges) {
        if let CellEdges::Edges(previous) = std::mem::replace(&mut self.artist().rows, edges) {
            release_data(self.fig, previous);
        }
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

/// Removes an array that a setter has replaced from the figure's data arrays when no
/// artist refers to it any more and it has no name.
///
/// An array without a name is one the builder copied from values, which nothing outside
/// the figure can name; an array with a name was made by [`Figure::data`](crate::Figure::data)
/// (or by hand), and its maker may give it to another artist later, so it is kept. The rule
/// reads only the IR, so a figure wrapped from its IR is treated as the session that built
/// it was.
pub(crate) fn release_data(fig: &mut Figure, id: NodeId) {
    let referenced = fig
        .axes
        .iter()
        .flat_map(|axes| &axes.artists)
        .any(|artist| data_ids(artist).contains(&id));
    if !referenced {
        fig.data.retain(|data| data.id != id || data.name.is_some());
    }
}

/// Returns the identifier of every data array that an artist refers to.
pub(crate) fn data_ids(artist: &Artist) -> Vec<NodeId> {
    let grid_ids = |grid: &ironlab_ir::Grid| match *grid {
        ironlab_ir::Grid::Rectilinear { x, y } | ironlab_ir::Grid::Curvilinear { x, y } => [x, y],
    };
    match artist {
        Artist::Series(Series {
            x,
            y,
            z,
            size_data,
            color_data,
            ..
        }) => [*x, *y]
            .into_iter()
            .chain(*z)
            .chain(*size_data)
            .chain(*color_data)
            .collect(),
        Artist::Contour(Contour { grid, z, .. }) => {
            grid_ids(grid).into_iter().chain([*z]).collect()
        }
        Artist::Quiver(Quiver {
            x, y, z, u, v, w, ..
        }) => [*x, *y, *u, *v].into_iter().chain(*z).chain(*w).collect(),
        Artist::Surface(Surface {
            grid,
            z,
            c,
            face_alpha,
            edge_alpha,
            ..
        }) => grid_ids(grid)
            .into_iter()
            .chain([*z])
            .chain(*c)
            .chain(*face_alpha)
            .chain(*edge_alpha)
            .collect(),
        Artist::Image(Image { pixels, .. }) => vec![*pixels],
        Artist::ScalarImage(ScalarImage { values, alpha, .. }) => {
            [*values].into_iter().chain(*alpha).collect()
        }
        Artist::Bar(bar) => {
            let base = match bar.base {
                PerBar::Constant(_) => None,
                PerBar::Data(id) => Some(id),
            };
            let widths = match bar.width {
                BarWidth::Pitch(_) | BarWidth::Constant(_) => None,
                BarWidth::Data(id) => Some(id),
            };
            [bar.positions, bar.values]
                .into_iter()
                .chain(base)
                .chain(widths)
                .chain(bar.color_data)
                .chain(bar.binning.as_ref().map(|binning| binning.edges))
                .collect()
        }
        Artist::Stairs(stairs) => [stairs.data.positions(), stairs.data.values()]
            .into_iter()
            .chain(stairs.binning.as_ref().map(|binning| binning.edges))
            .collect(),
        Artist::Cells(cells) => {
            let edges = |edges: ironlab_ir::CellEdges| match edges {
                ironlab_ir::CellEdges::Unit => None,
                ironlab_ir::CellEdges::Edges(id) => Some(id),
            };
            [cells.values]
                .into_iter()
                .chain(edges(cells.columns))
                .chain(edges(cells.rows))
                .chain(cells.alpha)
                .chain(
                    cells
                        .binning
                        .iter()
                        .flat_map(|binning| [binning.x_edges, binning.y_edges]),
                )
                .collect()
        }
    }
}
