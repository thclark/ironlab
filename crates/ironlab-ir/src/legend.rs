//! Legends: the legend of an axes, legend panels, and the layout of legend entries.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::NodeId;
use crate::layout::Cell;

/// The legend of an axes, which lists the artists of the axes that have a display name.
///
/// The entries of an axes are shown either in a legend drawn inside its own plot area or
/// in a legend panel of the figure, never in both. In JSON a legend is an object whose
/// `type` names the variant and whose `value` holds its content, such as
/// `{"type": "inside", "value": {"location": "north_east", ...}}` or
/// `{"type": "panel", "value": 7}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Legend {
    /// The entries are listed in a legend drawn inside the plot area of the axes.
    Inside(InsideLegend),
    /// The entries are listed in the legend panel of the figure with this node
    /// identifier, together with the entries of every other axes that names the same
    /// panel.
    Panel(NodeId),
}

impl Default for Legend {
    /// Returns a legend inside the plot area, with the defaults of [`InsideLegend`].
    fn default() -> Self {
        Legend::Inside(InsideLegend::default())
    }
}

/// A legend drawn inside the plot area of its axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct InsideLegend {
    /// Where the legend is placed inside the plot area.
    pub location: LegendLocation,
    /// Whether the legend is drawn with a background and outline.
    pub boxed: bool,
    /// How the entries are arranged in columns.
    pub layout: LegendEntryLayout,
}

impl Default for InsideLegend {
    /// Returns a boxed legend in the top-right corner, with its entries in one column.
    fn default() -> Self {
        Self {
            location: LegendLocation::NorthEast,
            boxed: true,
            layout: LegendEntryLayout::default(),
        }
    }
}

/// A legend of the figure that occupies cells of the tile layout, as an axes does, and
/// lists the entries of every axes whose legend names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PanelLegend {
    /// The node identifier of the panel, unique within the figure.
    pub id: NodeId,
    /// The cells of the figure's tile layout that the panel occupies.
    pub cell: Cell,
    /// Where the block of entries is placed within the panel. A panel holds no data
    /// for the legend to avoid, so the location `best` places the block at the centre.
    pub align: LegendLocation,
    /// Whether the block of entries is drawn with a background and outline.
    pub boxed: bool,
    /// How the entries are arranged in columns.
    pub layout: LegendEntryLayout,
}

impl Default for PanelLegend {
    /// Returns an unboxed panel at the top-left cell of the tile layout, with its
    /// entries centred and arranged in as few columns as fit the panel.
    fn default() -> Self {
        Self {
            id: NodeId::default(),
            cell: Cell::default(),
            align: LegendLocation::Center,
            boxed: false,
            layout: LegendEntryLayout {
                columns: LegendEntryColumns::Auto,
                order: LegendEntryOrder::ColumnMajor,
            },
        }
    }
}

/// How the entries of a legend are arranged in columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct LegendEntryLayout {
    /// The number of columns.
    pub columns: LegendEntryColumns,
    /// The order in which the entries fill the columns.
    pub order: LegendEntryOrder,
}

impl Default for LegendEntryLayout {
    /// Returns a single column filled from top to bottom, which is the layout of an
    /// inside legend by default.
    fn default() -> Self {
        Self {
            columns: LegendEntryColumns::Count(1),
            order: LegendEntryOrder::ColumnMajor,
        }
    }
}

/// The number of columns in which the entries of a legend are arranged.
///
/// In JSON the number is an object whose `type` names the variant, such as
/// `{"type": "auto"}` or `{"type": "count", "value": 3}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum LegendEntryColumns {
    /// The fewest columns in which the entries fit the height available to the legend.
    Auto,
    /// A fixed number of columns, which must be at least one.
    Count(u32),
}

/// The order in which the entries of a legend fill its columns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LegendEntryOrder {
    /// Each column is filled from top to bottom before the next column is started.
    #[default]
    ColumnMajor,
    /// Each row is filled from left to right before the next row is started.
    RowMajor,
}

/// The placement of a legend inside the plot area of its axes, or of the block of
/// entries inside a legend panel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LegendLocation {
    /// The top-right corner.
    #[default]
    NorthEast,
    /// The top-left corner.
    NorthWest,
    /// The bottom-right corner.
    SouthEast,
    /// The bottom-left corner.
    SouthWest,
    /// The centre of the top edge.
    North,
    /// The centre of the bottom edge.
    South,
    /// The centre of the right edge.
    East,
    /// The centre of the left edge.
    West,
    /// The centre. A legend inside a plot area at this location covers the data at the
    /// centre of the plot.
    Center,
    /// The corner that overlaps the least data.
    Best,
}
