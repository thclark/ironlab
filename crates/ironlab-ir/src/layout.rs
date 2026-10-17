//! The tile layout of a figure and the cells of it that axes and legend panels occupy.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The grid of cells in which the axes and legend panels of a figure are placed.
///
/// The plot areas of the rows share the height of the figure, and those of the columns
/// its width, in proportion to their weights, after the margins that the axes reserve for
/// their tick labels, axis labels and titles have been taken out, so that plot areas of
/// equal weight are of equal size. A row or column without a weight, because the list of
/// weights is shorter than the number of rows or columns, has a weight of one, so that
/// empty lists give plot areas of equal size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TileLayout {
    /// The number of rows; at least one.
    pub rows: u32,
    /// The number of columns; at least one.
    pub cols: u32,
    /// The relative heights of the rows, from the top, each finite and positive. A row
    /// without a weight has a weight of one, and there are no more weights than rows.
    ///
    /// The property is omitted from JSON when there are no weights.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_weights: Vec<f64>,
    /// The relative widths of the columns, from the left, each finite and positive. A
    /// column without a weight has a weight of one, and there are no more weights than
    /// columns.
    ///
    /// The property is omitted from JSON when there are no weights.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub col_weights: Vec<f64>,
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
