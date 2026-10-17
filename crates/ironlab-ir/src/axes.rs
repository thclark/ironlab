//! Axes, their coordinate axes and projection.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artist::Artist;
use crate::ids::NodeId;
use crate::layout::Cell;
use crate::legend::Legend;
use crate::text::Text;

/// A plotting region placed in one or more cells of the figure's tile layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Axes {
    /// The node identifier of the axes, unique within the figure.
    pub id: NodeId,
    /// The cells of the figure's tile layout that the axes occupies.
    pub cell: Cell,
    /// Whether the axes is two- or three-dimensional, with its 3D view.
    pub projection: Projection,
    /// The title drawn above the axes.
    pub title: Option<Text>,
    /// The horizontal axis in 2D, or the first horizontal axis in 3D.
    pub x: Axis,
    /// The vertical axis in 2D, or the second horizontal axis in 3D.
    pub y: Axis,
    /// The vertical axis in 3D; ignored by 2D axes.
    pub z: Axis,
    /// Whether the full outline of the plot box is drawn, rather than only the
    /// edges that carry tick labels.
    #[serde(rename = "box")]
    pub box_: bool,
    /// The colormap used by colormapped artists in this axes.
    pub colormap: ColormapName,
    /// The data values mapped to the first and last colours of the colormap.
    pub clim: Limits,
    /// The legend, or `null` when no legend is shown.
    pub legend: Option<Legend>,
    /// The artists drawn in this axes, in drawing order.
    pub artists: Vec<Artist>,
}

impl Default for Axes {
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            cell: Cell::default(),
            projection: Projection::TwoD,
            title: None,
            x: Axis::default(),
            y: Axis::default(),
            z: Axis::default(),
            box_: true,
            colormap: ColormapName::Viridis,
            clim: Limits::Auto,
            legend: None,
            artists: Vec::new(),
        }
    }
}

/// The projection of an axes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Projection {
    /// A two-dimensional Cartesian axes.
    #[default]
    TwoD,
    /// A three-dimensional Cartesian axes viewed through an orthographic camera.
    ThreeD {
        /// The camera view.
        view3d: View3d,
    },
}

/// The orthographic camera view of a three-dimensional axes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct View3d {
    /// The rotation about the vertical axis in degrees, measured counterclockwise
    /// from the negative y axis when viewed from above.
    pub azimuth_deg: f64,
    /// The angle of the view direction above the x-y plane in degrees, from -90 to 90.
    pub elevation_deg: f64,
    /// The magnification of the projected box, where 1 fits the box to the plot area.
    pub zoom: f64,
    /// The horizontal offset of the projected box as a fraction of the plot area's
    /// width, increasing to the right.
    pub pan_x: f64,
    /// The vertical offset of the projected box as a fraction of the plot area's
    /// height, increasing downwards.
    pub pan_y: f64,
}

impl Default for View3d {
    fn default() -> Self {
        Self {
            azimuth_deg: -37.5,
            elevation_deg: 30.0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

/// One coordinate axis of an axes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Axis {
    /// The axis label.
    pub label: Option<Text>,
    /// The mapping from data values to positions along the axis.
    pub scale: Scale,
    /// The range of data values shown along the axis.
    pub limits: Limits,
    /// Whether the axis runs from its upper limit to its lower limit: from left to
    /// right for a horizontal axis, and from bottom to top for a vertical axis. The
    /// limits themselves keep `min < max`; only the direction in which they are drawn
    /// changes.
    pub reverse: bool,
    /// Where the major ticks of the axis are placed.
    pub ticks: Ticks,
    /// What is written at the major ticks of the axis.
    pub tick_labels: TickLabels,
    /// Whether grid lines are drawn at the major ticks of this axis.
    pub grid: bool,
}

/// Where the major ticks of an axis are placed.
///
/// Ticks never change the limits of an axis: automatic limits are computed as they are
/// for automatic ticks, and a tick that lies outside the limits is not drawn.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Ticks {
    /// The ticks are chosen at round values from the limits and the length of the axis.
    #[default]
    Auto,
    /// A tick at every value `origin + k × step`, for every integer `k`, that lies
    /// within the limits.
    Step {
        /// The distance between neighbouring ticks, in data units; finite and positive.
        step: f64,
        /// A value at which a tick is placed if it lies within the limits; finite.
        origin: f64,
    },
    /// A tick at each of the given values that lies within the limits; every value is
    /// finite, and positive on a logarithmic axis.
    Values(Vec<f64>),
}

/// What is written at the major ticks of an axis.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TickLabels {
    /// Each tick is labelled with its value, formatted for the scale of the axis.
    #[default]
    Auto,
    /// The ticks are drawn without labels, and no space is reserved for labels.
    Hidden,
    /// Each tick is labelled with the text at the same position in the list. Valid only
    /// with [`Ticks::Values`] holding as many values as there are texts.
    Text(Vec<Text>),
}

/// The mapping from data values to positions along an axis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    /// Positions are proportional to values.
    #[default]
    Linear,
    /// Positions are proportional to the base-10 logarithm of values; non-positive
    /// values are not drawn.
    Log,
}

/// A range of data values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Limits {
    /// The range is computed from the data. Axis limits are rounded outwards to ticks, except where only the grid of a
    /// contour or surface sets the x or y range, which then ends exactly at the grid.
    #[default]
    Auto,
    /// The range is fixed.
    Manual {
        /// The lower bound of the range.
        min: f64,
        /// The upper bound of the range.
        max: f64,
    },
}

/// The name of a colormap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColormapName {
    /// The perceptually uniform blue–green–yellow colormap.
    #[default]
    Viridis,
    /// The perceptually uniform blue–yellow colormap designed for colour-vision
    /// deficiency.
    Cividis,
    /// The perceptually uniform black–purple–cream colormap.
    Magma,
    /// The perceptually uniform black–red–yellow colormap.
    Inferno,
    /// The perceptually uniform blue–purple–yellow colormap.
    Plasma,
    /// The diverging blue–white–red colormap.
    Coolwarm,
    /// The linear black–white colormap.
    Gray,
}
