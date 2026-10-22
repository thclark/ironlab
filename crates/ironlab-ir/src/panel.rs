//! Panels: nodes of the figure that occupy cells of the tile layout and hold one key.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::NodeId;
use crate::layout::Cell;
use crate::legend::PanelLegend;

/// A node of the figure that occupies cells of the tile layout, as an axes does, and
/// holds one key, such as a legend.
///
/// A panel is placed, fitted and checked against the layout in the same way whatever its
/// content; the content decides only what is drawn in it and how large that is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Panel {
    /// The node identifier of the panel, unique within the figure.
    pub id: NodeId,
    /// The cells of the figure's tile layout that the panel occupies.
    pub cell: Cell,
    /// Where the content is placed within the panel.
    pub align: PanelLocation,
    /// What the panel holds.
    pub content: PanelContent,
}

/// The placement of the content of a panel within the panel.
///
/// The locations are those of a legend inside a plot area, under the same names, except
/// that there is no `best`: a cell holds no data for the content to avoid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PanelLocation {
    /// The centre.
    #[default]
    Center,
    /// The centre of the top edge.
    North,
    /// The centre of the bottom edge.
    South,
    /// The centre of the right edge.
    East,
    /// The centre of the left edge.
    West,
    /// The top-right corner.
    NorthEast,
    /// The top-left corner.
    NorthWest,
    /// The bottom-right corner.
    SouthEast,
    /// The bottom-left corner.
    SouthWest,
}

/// What a panel holds.
///
/// In JSON the content is an object whose `type` names the variant and whose `value`
/// holds it, such as `{"type": "legend", "value": {"boxed": true, ...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PanelContent {
    /// A legend, which lists the entries of every axes whose legend names the panel.
    Legend(PanelLegend),
    /// A colourbar, which shows a colour scale along the cells of the panel.
    Colorbar(PanelColorbar),
}

/// A colourbar held by a panel: a colour scale shown as a bar that stretches along the
/// panel's cell span, inset by the aligned margins of the rows or columns it spans, so that
/// a bar spanning a row or column of axes is exactly as long as their plot boxes.
///
/// The bar holds no colours, limits or ticks of its own: those belong to the scale, so that
/// every bar of one scale agrees with every other and with the artists drawn through it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PanelColorbar {
    /// The colour scale that the bar shows.
    pub scale: NodeId,
    /// Whether the bar stands along the panel's rows or lies along its columns.
    pub orientation: ColorbarOrientation,
}

/// The direction in which a panel colourbar runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorbarOrientation {
    /// The bar stands upright, with low values at the bottom and its tick labels on the
    /// right.
    #[default]
    Vertical,
    /// The bar lies flat, with low values at the left and its tick labels below.
    Horizontal,
}
