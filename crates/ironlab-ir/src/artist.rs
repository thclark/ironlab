//! Artists: the drawable nodes of an axes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{DataId, NodeId};
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
