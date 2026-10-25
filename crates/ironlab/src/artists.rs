//! Handles to artists, with chainable property setters.
//!
//! Each handle borrows the figure mutably and refers to its artist by node identifier.
//! Setters write directly to the artist in the figure IR and return the handle, so that
//! several properties can be set in one expression.

use ironlab_ir::{
    Artist, Contour, Coords, DashStyle, Figure, Image, ImagePlane, Levels, MarkerShape, NodeId,
    PixelRange, Quiver, QuiverScale, ScalarImage, Series, Surface, Text,
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
artist_handle!(QuiverMut, Quiver);
artist_handle!(SurfaceMut, Surface, scaled);

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
fn release_data(fig: &mut Figure, id: NodeId) {
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
fn data_ids(artist: &Artist) -> Vec<NodeId> {
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
                ironlab_ir::BarBase::Constant(_) => None,
                ironlab_ir::BarBase::Data(id) => Some(id),
            };
            let widths = match bar.slot {
                ironlab_ir::BarSlot::Pitch | ironlab_ir::BarSlot::Width(_) => None,
                ironlab_ir::BarSlot::Data(id) => Some(id),
            };
            [bar.positions, bar.values]
                .into_iter()
                .chain(base)
                .chain(widths)
                .chain(bar.color_data)
                .collect()
        }
        Artist::Stairs(stairs) => vec![stairs.data.positions(), stairs.data.values()],
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
                .collect()
        }
    }
}
