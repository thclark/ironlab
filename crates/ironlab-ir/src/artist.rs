//! Artists: the drawable nodes of an axes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::NodeId;
use crate::link::Dimension;
use crate::style::{Color, ColorSpec, DashStyle, LineStyle, MarkerShape, MarkerStyle};
use crate::text::Text;

/// A drawable node of an axes.
///
/// Every artist has a node identifier, an optional label drawn beside it in a legend,
/// and a visibility flag. An artist refers to its data by the node identifiers of data
/// arrays of the figure. Future plot types are added as new variants, each with a line in
/// the declaration of the kinds of node, which generates the methods of `Artist`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Artist {
    /// Data points drawn as a line, as markers or as both, optionally sized and coloured
    /// by data (plot, plot3, loglog, semilogx, semilogy, scatter, scatter3).
    Series(Series),
    /// Isolines or filled isobands of a gridded field (contour, contourf, contour3).
    Contour(Contour),
    /// Arrows at data points (quiver, quiver3).
    Quiver(Quiver),
    /// A gridded surface of faces (surf, mesh, and surface, which in 2D axes is a
    /// pseudocolour plot).
    Surface(Surface),
    /// A raster of true-colour pixels (image with a true-colour array).
    Image(Image),
    /// A raster of numbers, each mapped to a colour through a colour scale (image with
    /// an indexed array, imagesc).
    ScalarImage(ScalarImage),
    /// Rectangles that rise from a base to each value, one per position along a category
    /// axis (bar, barh).
    Bar(Bar),
    /// A stepped outline through values held between edges or at samples (stairs, step).
    Stairs(Stairs),
    /// A grid of rectangular cells between edges, each coloured by its value through a
    /// colour scale (heatmap, pcolormesh on a rectilinear grid, histogram2).
    Cells(Cells),
}

/// A series of data points, drawn as a line through them, as markers at them, or as both
/// (plot, plot3, loglog, semilogx, semilogy, scatter, scatter3).
///
/// The x, y and (in 3D) z arrays hold one value per point and must have the same number
/// of elements. Non-finite positions break the line. The line is drawn unless its dash is
/// `none` or its colour is `none`, and the markers are drawn unless their shape is
/// `none`, so a plot is a series without markers and a scatter is a series without a
/// line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Series {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The label drawn beside the artist in a legend.
    pub label: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The x coordinates of the points.
    pub x: NodeId,
    /// The y coordinates of the points.
    pub y: NodeId,
    /// The z coordinates of the points, allowed only in 3D axes; when absent in 3D
    /// axes the points lie in the plane z = 0.
    pub z: Option<NodeId>,
    /// The style of the line through the points. Its colour is the colour of the series,
    /// which an automatic marker face or edge follows; a colormapped colour takes the
    /// colours of the colour data.
    pub line: LineStyle,
    /// The style of the markers at the points.
    pub marker: MarkerStyle,
    /// The diameter of each marker in points, one value per point, or `None` for markers
    /// that all take `marker.size_pt`. The marker size remains the size of the legend
    /// key.
    pub size_data: Option<NodeId>,
    /// The values that colour the series through its colour scale, or `None` for a
    /// series of fixed colours. With one value per point, each marker takes the colour of
    /// its point and each segment of a colormapped line is a gradient between the colours
    /// of its two points. With one value per segment (one fewer than the points), each
    /// segment of the line takes the colour of its value and the markers take their own
    /// colours; this form is valid only when the series draws a line.
    pub color_data: Option<NodeId>,
    /// The colour scale through which the colour data is mapped. A series with colour
    /// data must name one; a series without colour data maps nothing.
    pub color_scale: Option<NodeId>,
    /// How the x values are placed: as data along the x axis, or as fractions of the
    /// plot area's width. Anything other than `data` is allowed only in 2D axes.
    pub x_coords: Coords,
    /// How the y values are placed: as data along the y axis, or as fractions of the
    /// plot area's height. Anything other than `data` is allowed only in 2D axes.
    pub y_coords: Coords,
    /// Whether the artist is clipped to the plot area. When false, clipping is released
    /// only along a dimension in axes-fraction coordinates, so that markers can sit on or
    /// beyond an edge of the axes; along a dimension in data coordinates the artist stays
    /// clipped to the plot area, so an artist in data coordinates along both dimensions
    /// is always clipped, and so is every artist of a 3D axes.
    pub clip: bool,
}

impl Default for Series {
    /// The series that `plot` draws: a solid automatic line without markers, size or
    /// colour data, or a colour scale, in data coordinates, clipped and visible.
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            label: None,
            visible: true,
            x: NodeId::default(),
            y: NodeId::default(),
            z: None,
            line: LineStyle::default(),
            marker: MarkerStyle::default(),
            size_data: None,
            color_data: None,
            color_scale: None,
            x_coords: Coords::Data,
            y_coords: Coords::Data,
            clip: true,
        }
    }
}

impl Series {
    /// Returns whether the series draws a line through its points: its dash is not
    /// `none` and its line colour is not `none`.
    pub fn draws_line(&self) -> bool {
        self.line.dash != DashStyle::None && self.line.color != ColorSpec::None
    }

    /// Returns whether the series draws markers at its points: their shape is not
    /// `none`.
    pub fn draws_markers(&self) -> bool {
        self.marker.shape != MarkerShape::None
    }
}

/// How the values of one dimension of a series are placed in a 2D axes.
///
/// A series places each dimension independently, so that a dimension in data
/// coordinates and one in axes-fraction coordinates make a blended placement, as
/// Matplotlib's blended transforms do: a reference line spanning the axes at a data
/// value, or markers along an edge of the axes at data positions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Coords {
    /// The values are data, placed through the limits and scale of the axis, and take
    /// part in its automatic limits.
    #[default]
    Data,
    /// The values are fractions of the plot area along the dimension: 0 is its left or
    /// bottom edge and 1 its right or top edge, whatever the limits, scale or direction
    /// of the axis. They take no part in the automatic limits of the axis, and values
    /// outside `[0, 1]` lie beyond the plot area.
    AxesFraction,
}

/// Isolines or filled isobands of a scalar field sampled on a grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Contour {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The label drawn beside the artist in a legend.
    pub label: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The grid on which the field is sampled.
    pub grid: Grid,
    /// The field values, a two-dimensional array of shape `[ny, nx]`.
    pub z: NodeId,
    /// The field values at which isolines are drawn or between which bands are filled, or
    /// `None` for the levels of the colour scale: the band edges of a discrete scale, and
    /// ten automatic levels otherwise, so that a contour and the bar of its discrete scale
    /// cannot disagree.
    pub levels: Option<Levels>,
    /// Whether the bands between levels are filled (contourf) rather than only the
    /// isolines drawn (contour).
    pub fill: bool,
    /// Where the contours are placed in 3D axes.
    pub placement: ContourPlacement,
    /// The style of the isolines; a colormapped colour takes the colour of each
    /// isoline's level through the colour scale.
    pub line: LineStyle,
    /// The colour scale through which levels and bands are coloured. A contour whose
    /// bands are filled, or whose isolines are colormapped or automatic, must name one.
    pub color_scale: Option<NodeId>,
    /// The opacity of the isolines, from 0 (transparent) to 1 (opaque), which multiplies
    /// the alpha of their colour.
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub line_opacity: f64,
    /// The opacity of the filled bands, from 0 (transparent) to 1 (opaque), which
    /// multiplies the alpha of their colour.
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub fill_opacity: f64,
}

impl Default for Contour {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            label: None,
            visible: true,
            grid: Grid::default(),
            z: NodeId::default(),
            levels: None,
            fill: false,
            placement: ContourPlacement::default(),
            line: LineStyle {
                color: ColorSpec::Colormapped,
                ..LineStyle::default()
            },
            color_scale: None,
            line_opacity: 1.0,
            fill_opacity: 1.0,
        }
    }
}

/// The grid on which a field of shape `[ny, nx]` is sampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Grid {
    /// An axis-aligned grid defined by one coordinate vector per axis.
    Rectilinear {
        /// The x coordinates of the columns, an array of `nx` values.
        x: NodeId,
        /// The y coordinates of the rows, an array of `ny` values.
        y: NodeId,
    },
    /// A structured grid whose every node has its own coordinates.
    Curvilinear {
        /// The x coordinate of every node, an array of shape `[ny, nx]`.
        x: NodeId,
        /// The y coordinate of every node, an array of shape `[ny, nx]`.
        y: NodeId,
    },
}

impl Default for Grid {
    fn default() -> Self {
        Grid::Rectilinear {
            x: NodeId::default(),
            y: NodeId::default(),
        }
    }
}

/// The field values at which contours are drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Levels {
    /// Levels are chosen at nice values spanning the data range.
    Auto {
        /// The approximate number of levels.
        count: u32,
    },
    /// Levels are given explicitly.
    Explicit {
        /// The levels, in ascending order.
        values: Vec<f64>,
    },
}

impl Default for Levels {
    fn default() -> Self {
        Levels::Auto { count: 10 }
    }
}

/// Where contours are placed in 3D axes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContourPlacement {
    /// All contours lie in one horizontal plane (contour, contourf).
    Plane {
        /// The height of the plane in 3D axes, or `null` for the bottom of the z
        /// axis; ignored by 2D axes.
        z: Option<f64>,
    },
    /// Each isoline is drawn at the height of its level (contour3); valid only in
    /// 3D axes.
    AtLevel,
}

impl Default for ContourPlacement {
    fn default() -> Self {
        ContourPlacement::Plane { z: None }
    }
}

/// Arrows representing a vector field at data points.
///
/// The position arrays and the component arrays hold one value per arrow and must
/// have the same number of elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Quiver {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The label drawn beside the artist in a legend.
    pub label: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The x coordinates of the arrow tails.
    pub x: NodeId,
    /// The y coordinates of the arrow tails.
    pub y: NodeId,
    /// The z coordinates of the arrow tails, allowed only in 3D axes; when absent in
    /// 3D axes the tails lie in the plane z = 0.
    pub z: Option<NodeId>,
    /// The x components of the vectors.
    pub u: NodeId,
    /// The y components of the vectors.
    pub v: NodeId,
    /// The z components of the vectors, allowed only in 3D axes; when absent in 3D
    /// axes the vectors lie in horizontal planes.
    pub w: Option<NodeId>,
    /// How vector lengths are scaled into arrow lengths.
    pub scale: QuiverScale,
    /// The style of the arrow shafts and heads.
    pub line: LineStyle,
    /// The length of each arrow head as a fraction of its arrow's length.
    pub head_size: f64,
}

impl Default for Quiver {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            label: None,
            visible: true,
            x: NodeId::default(),
            y: NodeId::default(),
            z: None,
            u: NodeId::default(),
            v: NodeId::default(),
            w: None,
            scale: QuiverScale::default(),
            line: LineStyle::default(),
            head_size: 0.3,
        }
    }
}

/// How quiver vector lengths are scaled into arrow lengths.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuiverScale {
    /// Arrows are scaled so that the longest does not overlap its neighbours.
    #[default]
    Auto,
    /// The automatic scale is multiplied by a factor.
    Factor {
        /// The multiplier applied to the automatic scale.
        value: f64,
    },
    /// Arrows are drawn with the vectors' lengths in data units.
    Off,
}

/// A surface of quadrilateral faces over a grid (surf, mesh, surface).
///
/// A surface is valid in either projection. In a three-dimensional axes `z` is the height
/// of every node. In a two-dimensional axes the surface is seen from directly above, so
/// that `z` positions nothing and the surface is a pseudocolour plot of its colour data.
/// A field of `ny` by `nx` nodes draws `(ny - 1)` by `(nx - 1)` faces, so a field with a
/// single row or a single column draws nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Surface {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The label drawn beside the artist in a legend.
    pub label: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The grid on which the surface is sampled.
    pub grid: Grid,
    /// The field, a two-dimensional array of shape `[ny, nx]`: the height of every node in
    /// a three-dimensional axes, and the colour data of every node unless `c` is given.
    pub z: NodeId,
    /// The colour data of every node, with the same shape as `z`; when absent the
    /// surface is coloured by `z`.
    pub c: Option<NodeId>,
    /// The colour of the faces.
    pub face: ColorSpec,
    /// The colour of the face edges.
    pub edge: ColorSpec,
    /// The width of the face edges in points.
    pub edge_width_pt: f64,
    /// The colour scale through which the colour data is mapped. A surface whose faces or
    /// edges are colormapped or automatic must name one.
    pub color_scale: Option<NodeId>,
    /// The alpha of the faces at every node, an array of 64-bit floating-point values
    /// with the same shape as `z`, or `None` for faces that are uniformly opaque before
    /// [`face_opacity`](Surface::face_opacity) applies. A face takes the mean of the
    /// alpha at its four corners, as it takes the mean of their colour data. A value
    /// outside `[0, 1]` is clamped to that range, and a face with a corner whose alpha
    /// is not finite is transparent.
    ///
    /// The property is omitted from JSON when it is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_alpha: Option<NodeId>,
    /// The alpha of the face edges at every node, an array of 64-bit floating-point
    /// values with the same shape as `z`, or `None` for edges that are uniformly opaque
    /// before [`edge_opacity`](Surface::edge_opacity) applies. The edges of a face take
    /// the mean of the edge alpha at its four corners, clamped and made transparent by
    /// a non-finite value as the face alpha is. It may name the same array as
    /// `face_alpha`.
    ///
    /// The property is omitted from JSON when it is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_alpha: Option<NodeId>,
    /// The opacity of the faces, from 0 (transparent) to 1 (opaque), which multiplies
    /// the alpha of every face: the face colour's own alpha and, when present, the
    /// [`face_alpha`](Surface::face_alpha) of the face.
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub face_opacity: f64,
    /// The opacity of the face edges, from 0 (transparent) to 1 (opaque), which
    /// multiplies the alpha of every edge: the edge colour's own alpha and, when
    /// present, the [`edge_alpha`](Surface::edge_alpha) of the face it borders.
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub edge_opacity: f64,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            label: None,
            visible: true,
            grid: Grid::default(),
            z: NodeId::default(),
            c: None,
            face: ColorSpec::Colormapped,
            edge: ColorSpec::Rgba {
                color: Color::BLACK,
            },
            edge_width_pt: 0.5,
            color_scale: None,
            face_alpha: None,
            edge_alpha: None,
            face_opacity: 1.0,
            edge_opacity: 1.0,
        }
    }
}

/// Returns the opacity of an opaque artist, which an opacity absent from JSON takes.
fn opaque() -> f64 {
    1.0
}

/// Returns whether an opacity is exactly 1, in which case JSON omits it.
fn is_opaque(opacity: &f64) -> bool {
    *opacity == 1.0
}

/// A true-colour image: a raster of pixels, each with its own colour (image with a
/// true-colour array).
///
/// An image is drawn as flat, uninterpolated pixels rather than as a mesh, so it is
/// planar: it lies in one of the coordinate planes of its axes, where its
/// [`placement`](Image::placement) puts it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Image {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The label drawn beside the artist in a legend.
    pub label: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The pixels, a three-dimensional array of shape `[ny, nx, 3]` (the red, green and
    /// blue components of every pixel) or `[ny, nx, 4]` (with an alpha component), so
    /// that the components of the pixel in row `j` and column `i` start at index
    /// `(j * nx + i) * channels`. The array may hold floating-point components from 0
    /// to 1 or 8-bit components from 0 to 255. A pixel with a non-finite component is
    /// transparent.
    pub pixels: NodeId,
    /// Where the pixels lie in the axes.
    pub placement: ImagePlacement,
    /// The opacity of the image, from 0 (transparent) to 1 (opaque), which multiplies
    /// the alpha of every pixel: its alpha component in a four-channel array, and 1 in
    /// a three-channel array. A true-colour image has no separate alpha array, because
    /// a four-channel array already gives every pixel its own alpha.
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub opacity: f64,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            label: None,
            visible: true,
            pixels: NodeId::default(),
            placement: ImagePlacement::default(),
            opacity: 1.0,
        }
    }
}

/// A scalar image: a raster of pixels, each holding one number that its colour scale maps
/// to a colour (image with an indexed array, and imagesc).
///
/// The scale decides what the numbers mean: through a continuous or discrete scale they
/// are values between limits, and through an indexed or categorical scale they are
/// indices of entries. A pixel whose number the scale cannot place (below or above its
/// range, outside its entries, or not finite) is drawn as the policy of that category of
/// the scale says. An image is drawn as flat, uninterpolated pixels rather than as a
/// mesh, so it is planar: it lies in one of the coordinate planes of its axes, where its
/// [`placement`](ScalarImage::placement) puts it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScalarImage {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The label drawn beside the artist in a legend.
    pub label: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The numbers, a two-dimensional array of shape `[ny, nx]`, so that the number of
    /// the pixel in row `j` and column `i` is at index `j * nx + i`. The array may hold
    /// floating-point values or 8-bit values; an 8-bit value is mapped as the whole
    /// number it denotes.
    pub values: NodeId,
    /// The colour scale through which the numbers are mapped. A scalar image always maps
    /// its numbers, so a figure in which it names no scale is invalid.
    pub color_scale: Option<NodeId>,
    /// Where the pixels lie in the axes.
    pub placement: ImagePlacement,
    /// The alpha of every pixel, a two-dimensional array with the same shape as
    /// `values`, or `None` for pixels that are uniformly opaque before
    /// [`opacity`](ScalarImage::opacity) applies. The array may hold floating-point values
    /// from 0 to 1 or 8-bit values from 0 to 255, as the components of a true-colour image
    /// may. A value outside the range is clamped to it, and a pixel whose alpha is not
    /// finite is transparent. The alpha also applies to a pixel drawn in the fixed colour
    /// of an out-of-range policy.
    ///
    /// The property is omitted from JSON when it is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<NodeId>,
    /// The opacity of the image, from 0 (transparent) to 1 (opaque), which multiplies
    /// the alpha of every pixel: the alpha of its colour and, when present, its value
    /// in [`alpha`](ScalarImage::alpha).
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub opacity: f64,
}

impl Default for ScalarImage {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            label: None,
            visible: true,
            values: NodeId::default(),
            color_scale: None,
            placement: ImagePlacement::default(),
            alpha: None,
            opacity: 1.0,
        }
    }
}

/// Where the pixels of an image lie in its axes: the plane of the image, and the
/// coordinates of its pixel centres along the two axes of that plane.
///
/// Along each axis of the plane the pixels are placed by the centres of the first and
/// last pixels, from which the pitch between centres follows, and the image covers half
/// a pitch beyond each centre. An absent range places the centres at 0, 1, …, n − 1, so
/// an image of `nx` columns covers −0.5 to nx − 0.5 along the first axis of its plane.
/// Row 0 of the array lies at the centre of the first row and column 0 at the centre of
/// the first column, whichever way the ranges run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImagePlacement {
    /// The plane in which the image lies, with its offset along the third axis.
    pub plane: ImagePlane,
    /// The centres of the first and last columns along the first axis of the plane, or
    /// `None` for centres at 0 to nx − 1.
    pub columns: Option<PixelRange>,
    /// The centres of the first and last rows along the second axis of the plane, or
    /// `None` for centres at 0 to ny − 1.
    pub rows: Option<PixelRange>,
}

/// The coordinates of the centres of the first and last pixels of an image along one
/// axis of its plane.
///
/// With `n` pixels along the axis the pitch is `(last − first) / (n − 1)`, so the image
/// covers `first − pitch / 2` to `last + pitch / 2`; a single pixel has a pitch of 1
/// whatever its range. A `last` less than `first` mirrors the image along the axis. Both
/// centres must be finite, and they may coincide only when the image has one pixel
/// along the axis, as validation checks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PixelRange {
    /// The coordinate of the centre of the first pixel.
    pub first: f64,
    /// The coordinate of the centre of the last pixel.
    pub last: f64,
}

impl Default for PixelRange {
    /// The range from 0 to 1, which places an image of any number of pixels validly; it
    /// is the range an absent range is given when the property editor creates one.
    fn default() -> Self {
        Self {
            first: 0.0,
            last: 1.0,
        }
    }
}

/// The plane of an axes in which an image lies, with the offset of the plane along the
/// third axis.
///
/// The columns of the image run along the first axis of the plane and its rows along
/// the second. The offset is the coordinate of the plane along the third axis, or
/// `None` for the low end of that axis. A two-dimensional axes shows only the xy plane
/// and ignores its offset; the xz and yz planes are the walls of a three-dimensional
/// axes and are valid only there.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImagePlane {
    /// The plane of the x and y axes: the floor of a three-dimensional axes.
    Xy {
        /// The height of the plane, or `None` for the bottom of the z axis; ignored by
        /// two-dimensional axes.
        z: Option<f64>,
    },
    /// The plane of the x and z axes: a wall of a three-dimensional axes.
    Xz {
        /// The y coordinate of the plane, or `None` for the low end of the y axis.
        y: Option<f64>,
    },
    /// The plane of the y and z axes: the other wall of a three-dimensional axes.
    Yz {
        /// The x coordinate of the plane, or `None` for the low end of the x axis.
        x: Option<f64>,
    },
}

impl Default for ImagePlane {
    /// The xy plane at the bottom of the z axis.
    fn default() -> Self {
        ImagePlane::Xy { z: None }
    }
}

impl ImagePlane {
    /// Returns the dimensions along which the columns and the rows of an image in this
    /// plane lie, in that order.
    pub fn axes(self) -> [Dimension; 2] {
        match self {
            ImagePlane::Xy { .. } => [Dimension::X, Dimension::Y],
            ImagePlane::Xz { .. } => [Dimension::X, Dimension::Z],
            ImagePlane::Yz { .. } => [Dimension::Y, Dimension::Z],
        }
    }

    /// Returns the offset of the plane along its third axis, or `None` for the low end
    /// of that axis.
    pub fn offset(self) -> Option<f64> {
        match self {
            ImagePlane::Xy { z } => z,
            ImagePlane::Xz { y } => y,
            ImagePlane::Yz { x } => x,
        }
    }
}

/// The direction in which bars and stairs extend.
///
/// The category axis is the axis along which the positions of bars, and the edges or
/// samples of stairs, lie; the value axis is the axis along which their values lie.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Positions and edges lie along x and values along y, so that bars stand upright
    /// (bar).
    #[default]
    Vertical,
    /// Positions and edges lie along y and values along x, so that bars lie flat (barh).
    Horizontal,
}

impl Orientation {
    /// Returns the dimension along which positions, edges and samples lie.
    #[must_use]
    pub fn category_dimension(self) -> Dimension {
        match self {
            Orientation::Vertical => Dimension::X,
            Orientation::Horizontal => Dimension::Y,
        }
    }

    /// Returns the dimension along which values and bases lie.
    #[must_use]
    pub fn value_dimension(self) -> Dimension {
        match self {
            Orientation::Vertical => Dimension::Y,
            Orientation::Horizontal => Dimension::X,
        }
    }
}

/// A series of bars: one rectangle for each position along the category axis, which
/// extends along the value axis from a base to the value (bar, barh).
///
/// The positions, the values and any base or widths given as data hold one entry per bar
/// and must have the same number of elements. Each bar is centred in a slot along the
/// category axis, of which the fraction `gap` is left empty, measured on the page so that
/// bars on a logarithmic axis remain symmetric. A bar whose position or value is not
/// finite is not drawn, and a non-finite position takes no part in the pitch. Bars colour
/// individual bars as a series colours individual markers: with colour data of one value
/// per bar, mapped through a colour scale of any kind, a colormapped face or edge takes
/// the colour of each bar's value.
///
/// Bars of one axes whose groups share a name form one group. The members of a grouped
/// group split the drawn part of each slot equally, in artist order, and touch one
/// another; the members of a stacked group at the same position build on one another,
/// positive and negative values separately. The orientation, slot, gap, base and layout
/// of a group are those of its first member in artist order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bar {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The label drawn beside the artist in a legend.
    pub label: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// Whether the bars stand upright or lie flat.
    pub orientation: Orientation,
    /// The centre of the slot of each bar along the category axis, one value per bar.
    pub positions: NodeId,
    /// The end of each bar along the value axis, one value per bar; a value that is not
    /// finite draws no bar.
    pub values: NodeId,
    /// Where each bar starts along the value axis.
    pub base: BarBase,
    /// The extent of the slot of each bar along the category axis.
    pub slot: BarSlot,
    /// The fraction of each slot left empty, measured on the page and shared equally by
    /// the two sides of the bar: a finite number from 0 (touching bars) up to, but not
    /// including, 1.
    pub gap: f64,
    /// The group of bars of the axes to which the series belongs, or `None` for a
    /// series drawn on its own.
    pub group: Option<BarGroup>,
    /// The colour of the bars: automatic for the next colour of the colour order, which
    /// is the colour of the series, a fixed colour, none, or colormapped, which gives each
    /// bar the colour of its value of the colour data through the colour scale. Without
    /// colour data a colormapped face has no value to map and is drawn as an automatic one.
    pub face: ColorSpec,
    /// The values that colour the bars through their colour scale, one per bar, or `None`
    /// for bars of fixed colours.
    pub color_data: Option<NodeId>,
    /// The colour scale through which the colour data is mapped. Bars with colour data
    /// must name one; bars without colour data map nothing.
    pub color_scale: Option<NodeId>,
    /// The style of the outline of every bar. Its colour is none for no outline, a fixed
    /// colour, automatic for the colour of the series (as an automatic marker edge is), or
    /// colormapped, which gives the outline of each bar the colour of its bar's value
    /// through the colour scale.
    pub edge: LineStyle,
    /// The opacity of the faces, from 0 (transparent) to 1 (opaque), which multiplies the
    /// alpha of their colours.
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub face_opacity: f64,
    /// The opacity of the outlines, from 0 (transparent) to 1 (opaque), which multiplies
    /// the alpha of their colours.
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub edge_opacity: f64,
}

impl Default for Bar {
    /// The series that `bar` draws: upright bars in the next colour of the colour order,
    /// rising from 0, each across four fifths of the pitch between positions, outlined by
    /// a black line 0.5 points wide, without colour data, a colour scale or a group,
    /// visible and opaque.
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            label: None,
            visible: true,
            orientation: Orientation::Vertical,
            positions: NodeId::default(),
            values: NodeId::default(),
            base: BarBase::default(),
            slot: BarSlot::default(),
            gap: 0.2,
            group: None,
            face: ColorSpec::Auto,
            color_data: None,
            color_scale: None,
            edge: LineStyle {
                color: ColorSpec::Rgba {
                    color: Color::BLACK,
                },
                width_pt: 0.5,
                dash: DashStyle::Solid,
            },
            face_opacity: 1.0,
            edge_opacity: 1.0,
        }
    }
}

/// Returns the edge of cells by default: no outline, 0.5 points wide and solid when a
/// colour is given.
fn no_edge() -> LineStyle {
    LineStyle {
        color: ColorSpec::None,
        width_pt: 0.5,
        dash: DashStyle::Solid,
    }
}

/// Where the bars of a series start along the value axis.
///
/// On a logarithmic value axis a base that is not positive has no place, so such a bar is
/// drawn from the low edge of the plot area instead, with a warning.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum BarBase {
    /// Every bar starts at the same value.
    Constant(f64),
    /// Each bar starts at its own value, one per bar, which draws floating bars.
    Data(NodeId),
}

impl Default for BarBase {
    /// Bars that rise from 0.
    fn default() -> Self {
        BarBase::Constant(0.0)
    }
}

/// The extent of the slot of each bar along the category axis, of which the bar is drawn
/// across the fraction `1 − gap`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum BarSlot {
    /// The smallest distance between neighbouring distinct finite positions, or 1 when
    /// there is a single distinct position.
    #[default]
    Pitch,
    /// The same extent for every bar, in data units: a finite positive number.
    Width(f64),
    /// One extent for each bar, in data units, each a finite positive number; this is how
    /// a histogram of unequal bins is drawn.
    Data(NodeId),
}

/// The group of bars of an axes to which a bar series belongs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct BarGroup {
    /// The name of the group; the bar series of one axes whose groups share a name form
    /// one group. It is never drawn.
    pub name: String,
    /// How the members of the group share each position.
    pub layout: BarLayout,
}

/// How the members of a group of bars share each position.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BarLayout {
    /// The members stand side by side: they split the drawn part of each slot equally,
    /// in artist order, and touch one another, so that the gap falls between positions.
    #[default]
    Grouped,
    /// The members at the same position build on one another, in artist order, positive
    /// and negative values separately, so that each column shows a total.
    Stacked,
}

/// A stepped outline through values held between edges or at samples (stairs, step).
///
/// Stairs draw the outline that a histogram of steps draws: along the category axis the
/// outline runs through each value across its step, and it joins neighbouring steps with
/// segments along the value axis. A value that is not finite breaks the outline. With a
/// base, the outline drops to the base at both ends and may be filled down to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stairs {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The label drawn beside the artist in a legend.
    pub label: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// Whether the steps lie along x, with values along y, or the reverse.
    pub orientation: Orientation,
    /// The steps: values between edges, or values at samples.
    pub data: StairsData,
    /// The value along the value axis to which the outline drops at both ends, or `None`
    /// for an open outline that starts and ends at the first and last values.
    ///
    /// The property is omitted from JSON when it is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<f64>,
    /// The style of the outline, whose automatic colour is the next colour of the colour
    /// order. A colormapped colour is invalid, because stairs have no colour data and no
    /// colour scale.
    pub line: LineStyle,
    /// The colour of the area between the outline and the base, or none for no fill. A
    /// fill other than none needs a base; a colormapped fill is invalid.
    pub fill: ColorSpec,
    /// The opacity of the outline, from 0 (transparent) to 1 (opaque), which multiplies
    /// the alpha of its colour.
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub line_opacity: f64,
    /// The opacity of the fill, from 0 (transparent) to 1 (opaque), which multiplies the
    /// alpha of its colour.
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub fill_opacity: f64,
}

impl Default for Stairs {
    /// The stairs that `stairs` draws: an open, unfilled outline in the next colour of the
    /// colour order, upright and visible.
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            label: None,
            visible: true,
            orientation: Orientation::Vertical,
            data: StairsData::default(),
            base: None,
            line: LineStyle::default(),
            fill: ColorSpec::None,
            line_opacity: 1.0,
            fill_opacity: 1.0,
        }
    }
}

/// The steps of stairs: values between edges, or values at samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StairsData {
    /// Each value holds between two neighbouring edges, as the counts of a histogram hold
    /// between its bin edges (Matplotlib's `stairs`).
    Edges {
        /// The edges of the steps along the category axis, one more than the values:
        /// finite and strictly monotonic, increasing or decreasing.
        edges: NodeId,
        /// The value of each step.
        values: NodeId,
    },
    /// Each value holds at a sample, and the step from one value to the next occurs where
    /// `step` places it (MATLAB's `stairs` and Matplotlib's `step`).
    Samples {
        /// The positions of the samples along the category axis, one per value: finite
        /// and strictly monotonic, increasing or decreasing.
        x: NodeId,
        /// The value at each sample.
        values: NodeId,
        /// Where each step occurs relative to its samples.
        step: StepAt,
    },
}

impl Default for StairsData {
    /// Steps between edges, naming the identifier 0 for both arrays.
    fn default() -> Self {
        StairsData::Edges {
            edges: NodeId::default(),
            values: NodeId::default(),
        }
    }
}

impl StairsData {
    /// Returns the array of values.
    #[must_use]
    pub fn values(self) -> NodeId {
        match self {
            StairsData::Edges { values, .. } | StairsData::Samples { values, .. } => values,
        }
    }

    /// Returns the array of positions along the category axis: the edges or the samples.
    #[must_use]
    pub fn positions(self) -> NodeId {
        match self {
            StairsData::Edges { edges, .. } => edges,
            StairsData::Samples { x, .. } => x,
        }
    }
}

/// Where the step of stairs through samples occurs relative to its samples, as the
/// `where` argument of Matplotlib's `step` places it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepAt {
    /// Each value holds from the previous sample up to its own, so the step occurs at the
    /// earlier sample.
    Pre,
    /// Each value holds from its own sample up to the next, so the step occurs at the
    /// later sample.
    #[default]
    Post,
    /// Each value holds from halfway to the previous sample to halfway to the next, so the
    /// step occurs midway between the samples.
    Mid,
}

/// A grid of rectangular cells, each coloured by its value through a colour scale
/// (heatmap, histogram2, pcolormesh on a rectilinear grid).
///
/// Cell `(j, i)` holds the value in row `j` and column `i` of `values` and spans the
/// column edges `i` and `i + 1` along x and the row edges `j` and `j + 1` along y, so that
/// row 0 lies between the first two row edges. The fraction `gap` of each cell's extent is
/// left empty, measured on the page so that cells on a logarithmic axis remain
/// symmetric. Cells are valid only in a two-dimensional axes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cells {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The label drawn beside the artist in a legend.
    pub label: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The values, a two-dimensional array of shape `[ny, nx]` with one value per cell.
    /// The array may hold floating-point values or 8-bit values; an 8-bit value is mapped
    /// as the whole number it denotes.
    pub values: NodeId,
    /// The edges of the columns along x.
    pub columns: CellEdges,
    /// The edges of the rows along y.
    pub rows: CellEdges,
    /// The fraction of each cell's extent left empty, along x and along y.
    pub gap: CellGap,
    /// The style of the outline of every cell; a colour of none draws no outline, and an
    /// automatic or colormapped colour gives each outline the colour of its cell.
    pub edge: LineStyle,
    /// The colour scale through which the values are mapped. Cells always map their
    /// values, so a figure in which they name no scale is invalid.
    pub color_scale: Option<NodeId>,
    /// The alpha of every cell, a two-dimensional array with the same shape as `values`,
    /// or `None` for cells that are uniformly opaque before
    /// [`face_opacity`](Cells::face_opacity) applies. The array may hold floating-point values from
    /// 0 to 1 or 8-bit values from 0 to 255, as the alpha of a scalar image may.
    ///
    /// The property is omitted from JSON when it is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<NodeId>,
    /// The opacity of the cells, from 0 (transparent) to 1 (opaque), which multiplies the
    /// alpha of every cell: the alpha of its colour and, when present, its
    /// [`alpha`](Cells::alpha).
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub face_opacity: f64,
    /// The opacity of the outlines of the cells, from 0 (transparent) to 1 (opaque), which
    /// multiplies the alpha of their colour.
    ///
    /// The property is omitted from JSON when it is 1, and is 1 when JSON omits it.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub edge_opacity: f64,
}

impl Default for Cells {
    /// Seamless cells centred on whole numbers, without outlines, alpha or a colour
    /// scale, visible and opaque.
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            label: None,
            visible: true,
            values: NodeId::default(),
            columns: CellEdges::Unit,
            rows: CellEdges::Unit,
            gap: CellGap::default(),
            edge: no_edge(),
            color_scale: None,
            alpha: None,
            face_opacity: 1.0,
            edge_opacity: 1.0,
        }
    }
}

/// The edges of the columns or rows of cells.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum CellEdges {
    /// Cell `k` spans `k − 0.5` to `k + 0.5`, so the centres lie at 0, 1, 2 and so on, as
    /// the pixels of an image without a range do.
    #[default]
    Unit,
    /// The edges given as data: one more than the cells, finite and strictly monotonic,
    /// increasing or decreasing.
    Edges(NodeId),
}

/// The fractions of the extent of each cell left empty along x and along y, measured on
/// the page and shared equally by the two sides of the cell; each is a finite number from
/// 0 up to, but not including, 1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CellGap {
    /// The fraction of the width of each cell left empty.
    pub x: f64,
    /// The fraction of the height of each cell left empty.
    pub y: f64,
}
