//! Artists: the drawable nodes of an axes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{DataId, NodeId};
use crate::link::Dimension;
use crate::style::{Color, ColorSpec, LineStyle, MarkerShape, MarkerStyle};
use crate::text::Text;

/// A drawable node of an axes.
///
/// Every artist has a node identifier, an optional display name shown in the
/// legend, and a visibility flag. Future plot types are added as new variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Artist {
    /// A polyline with optional markers (plot, plot3, loglog, semilogx, semilogy).
    Line(Line),
    /// Markers at data points with per-point size and colour (scatter, scatter3).
    Scatter(Scatter),
    /// Isolines or filled isobands of a gridded field (contour, contourf, contour3).
    Contour(Contour),
    /// Arrows at data points (quiver, quiver3).
    Quiver(Quiver),
    /// A gridded surface of faces (surf, mesh).
    Surface(Surface),
    /// A raster of true-colour pixels (image with a true-colour array).
    Image(Image),
    /// A raster of pixels that name entries of the axes colormap (image with an
    /// indexed array).
    IndexedImage(IndexedImage),
    /// A raster of data values mapped through the axes colormap and colour limits
    /// (imagesc).
    MappedImage(MappedImage),
}

impl Artist {
    /// Returns the node identifier of the artist.
    pub fn id(&self) -> NodeId {
        match self {
            Artist::Line(a) => a.id,
            Artist::Scatter(a) => a.id,
            Artist::Contour(a) => a.id,
            Artist::Quiver(a) => a.id,
            Artist::Surface(a) => a.id,
            Artist::Image(a) => a.id,
            Artist::IndexedImage(a) => a.id,
            Artist::MappedImage(a) => a.id,
        }
    }

    /// Returns the name shown for the artist in the legend.
    pub fn display_name(&self) -> Option<&Text> {
        match self {
            Artist::Line(a) => a.display_name.as_ref(),
            Artist::Scatter(a) => a.display_name.as_ref(),
            Artist::Contour(a) => a.display_name.as_ref(),
            Artist::Quiver(a) => a.display_name.as_ref(),
            Artist::Surface(a) => a.display_name.as_ref(),
            Artist::Image(a) => a.display_name.as_ref(),
            Artist::IndexedImage(a) => a.display_name.as_ref(),
            Artist::MappedImage(a) => a.display_name.as_ref(),
        }
    }

    /// Returns whether the artist is drawn.
    pub fn visible(&self) -> bool {
        match self {
            Artist::Line(a) => a.visible,
            Artist::Scatter(a) => a.visible,
            Artist::Contour(a) => a.visible,
            Artist::Quiver(a) => a.visible,
            Artist::Surface(a) => a.visible,
            Artist::Image(a) => a.visible,
            Artist::IndexedImage(a) => a.visible,
            Artist::MappedImage(a) => a.visible,
        }
    }

    /// Sets whether the artist is drawn.
    pub fn set_visible(&mut self, visible: bool) {
        match self {
            Artist::Line(a) => a.visible = visible,
            Artist::Scatter(a) => a.visible = visible,
            Artist::Contour(a) => a.visible = visible,
            Artist::Quiver(a) => a.visible = visible,
            Artist::Surface(a) => a.visible = visible,
            Artist::Image(a) => a.visible = visible,
            Artist::IndexedImage(a) => a.visible = visible,
            Artist::MappedImage(a) => a.visible = visible,
        }
    }
}

/// A polyline through data points, with optional markers at the points.
///
/// The x, y and (in 3D) z arrays hold one value per point and must have the same
/// number of elements. Non-finite values break the line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Line {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The name shown for the artist in the legend.
    pub display_name: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The x coordinates of the points.
    pub x: DataId,
    /// The y coordinates of the points.
    pub y: DataId,
    /// The z coordinates of the points, allowed only in 3D axes; when absent in 3D
    /// axes the points lie in the plane z = 0.
    pub z: Option<DataId>,
    /// The style of the line through the points.
    pub line: LineStyle,
    /// The style of the markers at the points.
    pub marker: MarkerStyle,
}

impl Default for Line {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            display_name: None,
            visible: true,
            x: DataId::default(),
            y: DataId::default(),
            z: None,
            line: LineStyle::default(),
            marker: MarkerStyle::default(),
        }
    }
}

/// Markers at data points, each with its own size and colour.
///
/// The x, y and (in 3D) z arrays hold one value per point and must have the same
/// number of elements, as must any size or colour data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Scatter {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The name shown for the artist in the legend.
    pub display_name: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The x coordinates of the points.
    pub x: DataId,
    /// The y coordinates of the points.
    pub y: DataId,
    /// The z coordinates of the points, allowed only in 3D axes; when absent in 3D
    /// axes the points lie in the plane z = 0.
    pub z: Option<DataId>,
    /// The size of the markers, which overrides `marker.size_pt`.
    pub size: ScatterSize,
    /// The colour of the markers, applied wherever `marker.face` or `marker.edge`
    /// is `auto`.
    pub color: ScatterColor,
    /// The marker shape and the use of the scatter colour for face and edge.
    pub marker: MarkerStyle,
}

impl Default for Scatter {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            display_name: None,
            visible: true,
            x: DataId::default(),
            y: DataId::default(),
            z: None,
            size: ScatterSize::default(),
            color: ScatterColor::default(),
            marker: MarkerStyle {
                shape: MarkerShape::Circle,
                ..MarkerStyle::default()
            },
        }
    }
}

/// The size of scatter markers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScatterSize {
    /// Every marker has the same size.
    Scalar {
        /// The marker size in points, measured as the width of the marker.
        value: f64,
    },
    /// Each marker has its own size.
    Data {
        /// An array of marker sizes in points, one per point.
        data: DataId,
    },
}

impl Default for ScatterSize {
    fn default() -> Self {
        ScatterSize::Scalar { value: 4.0 }
    }
}

/// The colour of scatter markers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScatterColor {
    /// Every marker has the same colour.
    Spec {
        /// The colour of every marker.
        spec: ColorSpec,
    },
    /// Each marker is coloured by a data value through the axes colormap.
    Data {
        /// An array of values, one per point, mapped through the axes colormap and
        /// colour limits.
        data: DataId,
    },
}

impl Default for ScatterColor {
    fn default() -> Self {
        ScatterColor::Spec {
            spec: ColorSpec::Auto,
        }
    }
}

/// Isolines or filled isobands of a scalar field sampled on a grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Contour {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The name shown for the artist in the legend.
    pub display_name: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The grid on which the field is sampled.
    pub grid: Grid,
    /// The field values, a two-dimensional array of shape `[ny, nx]`.
    pub z: DataId,
    /// The field values at which isolines are drawn or between which bands are filled.
    pub levels: Levels,
    /// Whether the bands between levels are filled (contourf) rather than only the
    /// isolines drawn (contour).
    pub fill: bool,
    /// Where the contours are placed in 3D axes.
    pub placement: ContourPlacement,
    /// The style of the isolines; a colormapped colour takes each isoline's level.
    pub line: LineStyle,
}

impl Default for Contour {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            display_name: None,
            visible: true,
            grid: Grid::default(),
            z: DataId::default(),
            levels: Levels::default(),
            fill: false,
            placement: ContourPlacement::default(),
            line: LineStyle {
                color: ColorSpec::Colormapped,
                ..LineStyle::default()
            },
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
        x: DataId,
        /// The y coordinates of the rows, an array of `ny` values.
        y: DataId,
    },
    /// A structured grid whose every node has its own coordinates.
    Curvilinear {
        /// The x coordinate of every node, an array of shape `[ny, nx]`.
        x: DataId,
        /// The y coordinate of every node, an array of shape `[ny, nx]`.
        y: DataId,
    },
}

impl Default for Grid {
    fn default() -> Self {
        Grid::Rectilinear {
            x: DataId::default(),
            y: DataId::default(),
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
    /// The name shown for the artist in the legend.
    pub display_name: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The x coordinates of the arrow tails.
    pub x: DataId,
    /// The y coordinates of the arrow tails.
    pub y: DataId,
    /// The z coordinates of the arrow tails, allowed only in 3D axes; when absent in
    /// 3D axes the tails lie in the plane z = 0.
    pub z: Option<DataId>,
    /// The x components of the vectors.
    pub u: DataId,
    /// The y components of the vectors.
    pub v: DataId,
    /// The z components of the vectors, allowed only in 3D axes; when absent in 3D
    /// axes the vectors lie in horizontal planes.
    pub w: Option<DataId>,
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
            display_name: None,
            visible: true,
            x: DataId::default(),
            y: DataId::default(),
            z: None,
            u: DataId::default(),
            v: DataId::default(),
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

/// A surface of quadrilateral faces over a grid (surf, mesh).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Surface {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The name shown for the artist in the legend.
    pub display_name: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The grid on which the surface is sampled.
    pub grid: Grid,
    /// The height of every node, a two-dimensional array of shape `[ny, nx]`.
    pub z: DataId,
    /// The colour data of every node, with the same shape as `z`; when absent the
    /// surface is coloured by `z`.
    pub c: Option<DataId>,
    /// The colour of the faces.
    pub face: ColorSpec,
    /// The colour of the face edges.
    pub edge: ColorSpec,
    /// The width of the face edges in points.
    pub edge_width_pt: f64,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            display_name: None,
            visible: true,
            grid: Grid::default(),
            z: DataId::default(),
            c: None,
            face: ColorSpec::Colormapped,
            edge: ColorSpec::Rgba {
                color: Color::BLACK,
            },
            edge_width_pt: 0.5,
        }
    }
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
    /// The name shown for the artist in the legend.
    pub display_name: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The pixels, a three-dimensional array of shape `[ny, nx, 3]` (the red, green and
    /// blue components of every pixel) or `[ny, nx, 4]` (with an alpha component), so
    /// that the components of the pixel in row `j` and column `i` start at index
    /// `(j * nx + i) * channels`. The array may hold floating-point components from 0
    /// to 1 or 8-bit components from 0 to 255. A pixel with a non-finite component is
    /// transparent.
    pub pixels: DataId,
    /// Where the pixels lie in the axes.
    pub placement: ImagePlacement,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            display_name: None,
            visible: true,
            pixels: DataId::default(),
            placement: ImagePlacement::default(),
        }
    }
}

/// A colour-indexed image: a raster of pixels whose values name entries of the axes
/// colormap directly (image with an indexed array).
///
/// An index is looked up without any mapping: a floating-point index is truncated toward
/// zero, and an index from 0 to 255 takes that entry of the 256-entry colormap. The
/// indices neither use nor change the colour limits of the axes. A pixel whose index lies
/// outside the colormap, or is not finite, is drawn as the policy of its category says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndexedImage {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The name shown for the artist in the legend.
    pub display_name: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The indices, a two-dimensional array of shape `[ny, nx]`. The array may hold
    /// floating-point indices, which are truncated toward zero, or 8-bit indices, which
    /// always lie within the colormap.
    pub indices: DataId,
    /// Where the pixels lie in the axes.
    pub placement: ImagePlacement,
    /// What is drawn for a pixel whose truncated index is less than 0.
    pub below: OutOfRange,
    /// What is drawn for a pixel whose truncated index is greater than 255.
    pub above: OutOfRange,
    /// What is drawn for a pixel whose index is NaN or infinite.
    pub non_finite: OutOfRange,
}

impl Default for IndexedImage {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            display_name: None,
            visible: true,
            indices: DataId::default(),
            placement: ImagePlacement::default(),
            below: OutOfRange::default(),
            above: OutOfRange::default(),
            non_finite: OutOfRange::default(),
        }
    }
}

/// A colour-mapped image: a raster of data values, each scaled through the colour limits
/// of the axes into its colormap (imagesc).
///
/// The values are coloured as the colour data of a surface is, and contribute to
/// automatic colour limits in the same way. A pixel whose value lies outside the colour
/// limits, or is not finite, is drawn as the policy of its category says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MappedImage {
    /// The node identifier of the artist, unique within the figure.
    pub id: NodeId,
    /// The name shown for the artist in the legend.
    pub display_name: Option<Text>,
    /// Whether the artist is drawn.
    pub visible: bool,
    /// The values, a two-dimensional array of shape `[ny, nx]`, mapped through the axes
    /// colormap and colour limits. The array may hold floating-point or 8-bit values; an
    /// 8-bit value is mapped as the number it denotes.
    pub values: DataId,
    /// Where the pixels lie in the axes.
    pub placement: ImagePlacement,
    /// What is drawn for a pixel whose value is less than the lower colour limit.
    pub below: OutOfRange,
    /// What is drawn for a pixel whose value is greater than the upper colour limit.
    pub above: OutOfRange,
    /// What is drawn for a pixel whose value is NaN or infinite.
    pub non_finite: OutOfRange,
}

impl Default for MappedImage {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            display_name: None,
            visible: true,
            values: DataId::default(),
            placement: ImagePlacement::default(),
            below: OutOfRange::default(),
            above: OutOfRange::default(),
            non_finite: OutOfRange::default(),
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

/// What is drawn for a pixel of a colour-indexed or colour-mapped image that the artist
/// cannot colour: an index outside the colormap, a value outside the colour limits, or
/// an index or value that is not finite.
///
/// Each of the three categories of such pixels (`below`, `above` and `non_finite`)
/// holds a policy of its own, so that, for example, non-finite values may be tolerated
/// while values outside the range are refused, or the reverse. Every category is
/// transparent by default, so that an image reaches the page whatever its data holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutOfRange {
    /// Such a pixel is a validation error, so the figure is refused until the data is
    /// corrected.
    Strict,
    /// Nothing is drawn for the pixel.
    #[default]
    Transparent,
    /// The pixel takes the nearest end colour of the colormap: its first entry below the
    /// range and its last entry above it. A non-finite value has no nearest end, so at
    /// the `non_finite` category a clamp draws nothing.
    Clamp,
    /// The pixel is drawn in a fixed colour.
    Rgba {
        /// The colour of the pixel.
        color: Color,
    },
}

impl From<Color> for OutOfRange {
    /// A colour is the fixed-colour policy: such a pixel is drawn in that colour.
    fn from(color: Color) -> Self {
        OutOfRange::Rgba { color }
    }
}
