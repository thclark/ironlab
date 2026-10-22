//! Panels: nodes of the figure that occupy cells of the tile layout and hold one key.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::axes::TitleAlign;
use crate::ids::NodeId;
use crate::layout::Cell;
use crate::legend::PanelLegend;
use crate::text::Text;

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
    /// Where the content is placed within the panel. Along the span of a colourbar shorter
    /// than the span, it also places the bar: a location that names an end of the span
    /// pushes the bar to that end, and any other location centres it.
    pub align: PanelLocation,
    /// A heading drawn at the top of the panel's content, or `None` for no heading. In a
    /// boxed legend it lies inside the border, above the entries; above a standing colourbar
    /// that spans plots, it lies in the margin above them, as an axes title does.
    pub title: Option<Text>,
    /// How the title is aligned with the content it heads: with its left edge, its centre
    /// or its right edge.
    pub title_align: TitleAlign,
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

/// A colourbar held by a panel: a colour scale shown as a bar that runs along the panel's
/// cell span, inset by the aligned margins of the rows or columns it spans, so that a bar
/// spanning a row or column of axes is measured against their drawn plot boxes.
///
/// The bar holds no colours, limits or ticks of its own: those belong to the scale, so that
/// every bar of one scale agrees with every other and with the artists drawn through it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PanelColorbar {
    /// The colour scale that the bar shows.
    pub scale: NodeId,
    /// Whether the bar stands along the panel's rows or lies along its columns.
    pub orientation: ColorbarOrientation,
    /// The length of the bar, its keys included, as a fraction of the span of the drawn
    /// plot boxes beside it, or `None` for the automatic length: the whole span for a
    /// standing bar and half of it for a lying one. A fraction is a finite number greater
    /// than 0 and at most 1. The bar is placed along the span by the panel's `align`.
    pub length: Option<f64>,
    /// Where the title of the scale is drawn beside the bar.
    pub title_placement: ColorbarTitlePlacement,
}

/// Where the title of a colour scale is drawn beside a colourbar of the scale.
///
/// The field that holds it is named `title_placement`, not `title`, because a title is
/// drawn text and this is where the scale's title is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColorbarTitlePlacement {
    /// Beyond the tick labels, on their side of the bar, centred on its length. Along a
    /// standing bar the title is rotated a quarter turn counter-clockwise, so that it reads
    /// from bottom to top, as the label of a y axis does.
    #[default]
    Along,
    /// As an upright heading beside the bar: above a standing bar (above a standing panel
    /// bar, in the margin above the first row the panel spans) and above a lying panel bar.
    /// On a lying bar inside an axes, whose tick labels face the interior of the plot area,
    /// the heading lies on the side of the bar away from them: below a bar along the south
    /// edge and above a bar along the north edge.
    Above,
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
