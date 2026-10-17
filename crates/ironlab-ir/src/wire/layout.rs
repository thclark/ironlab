//! Wire types of `ironlab/ir/v0/layout.proto`.

proto_file! {
    /// The grid of cells in which the axes and legend panels of a figure are placed.
    message TileLayout {
        /// The number of rows; at least one.
        optional uint32 rows = 1;
        /// The number of columns; at least one.
        optional uint32 cols = 2;
        /// The relative heights of the rows, from the top, each finite and positive. A
        /// row without a weight has a weight of one.
        repeated double row_weights = 3;
        /// The relative widths of the columns, from the left, each finite and positive.
        /// A column without a weight has a weight of one.
        repeated double col_weights = 4;
    }

    /// A rectangular block of cells in the figure's tile layout, numbered from zero
    /// starting at the top-left cell.
    message Cell {
        /// The zero-based index of the top row occupied.
        optional uint32 row = 1;
        /// The zero-based index of the leftmost column occupied.
        optional uint32 col = 2;
        /// The number of rows occupied; at least one.
        optional uint32 row_span = 3;
        /// The number of columns occupied; at least one.
        optional uint32 col_span = 4;
    }
}
