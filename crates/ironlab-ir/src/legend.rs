//! Legends: the legend of an axes, the legend content of a panel, and the layout of legend
//! entries.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::NodeId;
use crate::style::{Color, ColorSpec, DashStyle, LineStyle, MarkerShape, MarkerStyle};
use crate::text::Text;

/// The legend of an axes, which lists the artists of the axes that have a label.
///
/// The entries of an axes are shown either in a legend drawn inside its own plot area or
/// in a panel of the figure that holds a legend, never in both. In JSON a legend is an object whose
/// `type` names the variant and whose `value` holds its content, such as
/// `{"type": "inside", "value": {"location": "north_east", ...}}` or
/// `{"type": "panel", "value": 7}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Legend {
    /// The entries are listed in a legend drawn inside the plot area of the axes.
    Inside(InsideLegend),
    /// The entries are listed in the panel of the figure with this node identifier, which
    /// must hold a legend, together with the entries of every other axes that names the
    /// same panel.
    Panel(NodeId),
}

impl Default for Legend {
    /// Returns a legend inside the plot area, with the defaults of [`InsideLegend`].
    fn default() -> Self {
        Legend::Inside(InsideLegend::default())
    }
}

/// A legend drawn inside the plot area of its axes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InsideLegend {
    /// Where the legend is placed inside the plot area.
    pub location: LegendLocation,
    /// Whether the legend is drawn with a background and outline.
    pub boxed: bool,
    /// How the entries are arranged in columns.
    pub layout: LegendEntryLayout,
    /// Which entries the legend lists, and in what order: the labelled artists of its axes
    /// in artist order, or an explicit list of items. An item that names an artist must
    /// name an artist of the same axes.
    ///
    /// The property is omitted from JSON when the entries are automatic.
    #[serde(default, skip_serializing_if = "LegendEntries::is_auto")]
    pub entries: LegendEntries,
}

impl Default for InsideLegend {
    /// Returns a boxed legend in the top-right corner, with its entries in one column.
    fn default() -> Self {
        Self {
            location: LegendLocation::NorthEast,
            boxed: true,
            layout: LegendEntryLayout::default(),
            entries: LegendEntries::Auto,
        }
    }
}

/// The legend content of a panel, which lists the entries of every axes whose legend
/// names the panel.
///
/// Where the panel lies and where its content sits in it are properties of the
/// [`Panel`](crate::Panel); a legend content says only what is listed and how.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PanelLegend {
    /// Whether the area of the panel is outlined by a border drawn as the box of an
    /// axes is drawn. The area of a panel is aligned with the plot areas of the axes in
    /// its rows and columns, so the border continues the edges of their boxes. The
    /// border has no background, and the entries are kept inset from it.
    pub boxed: bool,
    /// How the entries are arranged in columns.
    pub layout: LegendEntryLayout,
    /// Which entries the panel lists, and in what order: the labelled artists of every
    /// axes that names the panel, merged by label and look, in the reading order of the
    /// layout, or an explicit list of items. An item that names an artist may name an
    /// artist of any axes of the figure.
    ///
    /// The property is omitted from JSON when the entries are automatic.
    #[serde(default, skip_serializing_if = "LegendEntries::is_auto")]
    pub entries: LegendEntries,
}

impl Default for PanelLegend {
    /// Returns a boxed legend whose entries are arranged in as few columns as fit the
    /// panel.
    fn default() -> Self {
        Self {
            boxed: true,
            layout: LegendEntryLayout {
                columns: LegendEntryColumns::Auto,
                order: LegendEntryOrder::ColumnMajor,
            },
            entries: LegendEntries::Auto,
        }
    }
}

/// Which entries a legend lists, and in what order.
///
/// In JSON the entries are an object whose `type` names the variant, such as
/// `{"type": "auto"}` or `{"type": "explicit", "value": [...]}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum LegendEntries {
    /// Every artist with a label, in the order of the legend: for a legend inside a plot
    /// area, the artists of its axes in artist order; for the legend of a panel, the
    /// artists of every axes that names the panel, in the reading order of the layout,
    /// with the entries of the same label and look merged into one.
    #[default]
    Auto,
    /// Exactly these items, in this order. The order of the items is independent of the
    /// order in which the artists are drawn, so a series drawn last, on top of the
    /// others, can be listed first.
    Explicit(Vec<LegendItem>),
}

impl LegendEntries {
    /// Returns whether the entries are automatic.
    pub fn is_auto(&self) -> bool {
        matches!(self, LegendEntries::Auto)
    }
}

/// One item of an explicit list of legend entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LegendItem {
    /// What the item lists: an artist, or a label and sample that stand for no artist.
    pub content: LegendItemContent,
    /// Whether the item spans every column of the legend, on a row of its own at its
    /// position in the list. The label of such an item does not set the width of any
    /// column.
    pub full_row: bool,
}

impl LegendItem {
    /// Returns an item, not on a row of its own, that lists the artist with this node
    /// identifier.
    pub fn artist(artist: NodeId) -> Self {
        Self {
            content: LegendItemContent::Artist(artist),
            full_row: false,
        }
    }

    /// Returns an item, not on a row of its own, that shows a sample and a label and
    /// stands for no artist.
    pub fn custom(sample: LegendSample, label: impl Into<Text>) -> Self {
        Self {
            content: LegendItemContent::Custom {
                label: label.into(),
                sample,
            },
            full_row: false,
        }
    }
}

impl From<NodeId> for LegendItem {
    /// Returns an item, not on a row of its own, that lists the artist with this node
    /// identifier.
    fn from(artist: NodeId) -> Self {
        LegendItem::artist(artist)
    }
}

/// What an item of an explicit list of legend entries lists.
///
/// In JSON the content is an object whose `type` names the variant, such as
/// `{"type": "artist", "value": 12}` or
/// `{"type": "custom", "value": {"label": {...}, "sample": {...}}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum LegendItemContent {
    /// The artist with this node identifier, labelled by its label beside the sample
    /// drawn for it. Clicking the item in the viewer hides or shows the artist.
    Artist(NodeId),
    /// A label beside a sample, standing for no artist, such as the meaning of a marker
    /// shape that many artists share. Clicking the item does nothing.
    Custom {
        /// The label shown beside the sample.
        label: Text,
        /// The sample drawn beside the label.
        sample: LegendSample,
    },
}

/// The sample of a legend item that stands for no artist.
///
/// In JSON a sample is an object whose `type` names the variant, such as
/// `{"type": "swatch", "color": "#e69f00"}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LegendSample {
    /// A sample drawn as the sample of a series is drawn: a horizontal line with a
    /// marker at its centre. A line whose dash is `none` draws only the marker, and a
    /// marker whose shape is `none` draws only the line. A line colour of `auto` is the
    /// ink in which the axes and labels of the figure are drawn, because the item belongs
    /// to no axes whose colour order could supply one, and a marker colour of `auto`
    /// takes the line colour, as it does for a series.
    Line {
        /// The style of the line.
        line: LineStyle,
        /// The style of the marker at the centre of the line.
        marker: MarkerStyle,
    },
    /// A filled rectangle of a colour, as the sample of a surface is drawn.
    Swatch {
        /// The colour of the rectangle.
        color: Color,
    },
}

impl LegendSample {
    /// Returns a sample of a solid line of the given colour, without a marker.
    pub fn line(color: Color) -> Self {
        LegendSample::Line {
            line: LineStyle {
                color: ColorSpec::Rgba { color },
                ..LineStyle::default()
            },
            marker: MarkerStyle::default(),
        }
    }

    /// Returns a sample of a single filled marker of the given shape and colour, without
    /// a line.
    pub fn marker(shape: MarkerShape, color: Color) -> Self {
        LegendSample::Line {
            line: LineStyle {
                color: ColorSpec::Rgba { color },
                dash: DashStyle::None,
                ..LineStyle::default()
            },
            marker: MarkerStyle {
                shape,
                face: ColorSpec::Auto,
                edge: ColorSpec::Auto,
                ..MarkerStyle::default()
            },
        }
    }

    /// Returns a sample of a filled rectangle of the given colour.
    pub fn swatch(color: Color) -> Self {
        LegendSample::Swatch { color }
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

/// The placement of a legend inside the plot area of its axes.
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
