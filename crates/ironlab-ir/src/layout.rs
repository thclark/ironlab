//! The tile layout of a figure and the cells of it that axes and legend panels occupy.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The grid of cells in which the axes and legend panels of a figure are placed.
///
/// A row or column is either weighted or fitted to the legend panels it holds. The margins
/// that the axes reserve for their tick labels, axis labels and titles are taken out of
/// the figure first. A fitted row then takes the height that the legend panels in it need
/// for their entries, and a fitted column the width. The plot areas of the weighted rows
/// share the rest of the height, and those of the weighted columns the rest of the width,
/// in proportion to their weights, so that plot areas of equal weight are of equal size. A
/// row or column without a size, because the list of sizes is shorter than the number of
/// rows or columns, has a weight of one, so that empty lists give plot areas of equal size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TileLayout {
    /// The number of rows; at least one.
    pub rows: u32,
    /// The number of columns; at least one.
    pub cols: u32,
    /// The sizes of the rows, from the top: a finite positive weight, or fitted to the
    /// legend panels in the row. A row without a size has a weight of one, and there are
    /// no more sizes than rows.
    ///
    /// The property is omitted from JSON when there are no sizes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_weights: Vec<TrackSize>,
    /// The sizes of the columns, from the left: a finite positive weight, or fitted to the
    /// legend panels in the column. A column without a size has a weight of one, and there
    /// are no more sizes than columns.
    ///
    /// The property is omitted from JSON when there are no sizes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub col_weights: Vec<TrackSize>,
}

impl TileLayout {
    /// Returns a layout of the given numbers of rows and columns, all of equal size.
    pub fn new(rows: u32, cols: u32) -> Self {
        Self {
            rows,
            cols,
            row_weights: Vec::new(),
            col_weights: Vec::new(),
        }
    }
}

/// The size of one row or column of a tile layout.
///
/// In JSON a size is an object whose `type` names the variant, such as
/// `{"type": "weight", "value": 2.0}` or `{"type": "fit"}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TrackSize {
    /// A share of the height or width that the fitted rows or columns leave, in
    /// proportion to this weight, which must be finite and positive.
    Weight(f64),
    /// The size of the content of the legend panels that lie in this row or column
    /// alone, with their borders and the margins that align them with the plot areas
    /// around them. An axes cannot lie in fitted rows or columns alone, because an axes
    /// has no content size of its own.
    Fit,
}

impl Default for TrackSize {
    /// Returns a weight of one.
    fn default() -> Self {
        TrackSize::Weight(1.0)
    }
}

impl From<f64> for TrackSize {
    /// Returns the weight of the given value.
    fn from(weight: f64) -> Self {
        TrackSize::Weight(weight)
    }
}

impl Default for TileLayout {
    /// Returns a layout of a single cell.
    fn default() -> Self {
        Self::new(1, 1)
    }
}

/// A rectangular block of cells in the figure's tile layout.
///
/// Rows and columns are numbered from zero, starting at the top-left cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Cell {
    /// The zero-based index of the top row occupied.
    pub row: u32,
    /// The zero-based index of the leftmost column occupied.
    pub col: u32,
    /// The number of rows occupied; at least one.
    pub row_span: u32,
    /// The number of columns occupied; at least one.
    pub col_span: u32,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1,
        }
    }
}
