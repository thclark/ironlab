//! Wire types of `ironlab/ir/v0/layout.proto`.

proto_file! {
    /// The grid of cells in which the axes and legend panels of a figure are placed.
    message TileLayout {
        /// The number of rows; at least one.
        optional uint32 rows = 1;
        /// The number of columns; at least one.
        optional uint32 cols = 2;
        /// The numbers of the weights of schema 0.4, which were plain numbers.
        reserved 3, 4;
        /// The sizes of the rows, from the top. A row without a size has a weight of
        /// one.
        repeated message TrackSize row_weights = 5;
        /// The sizes of the columns, from the left. A column without a size has a
        /// weight of one.
        repeated message TrackSize col_weights = 6;
    }

    /// The size of one row or column of a tile layout.
    message TrackSize {
        /// The kind of size; unset means a weight of one.
        oneof kind: TrackSizeKind {
            /// A share of the space that the fitted rows or columns leave.
            Weight(TrackSizeWeight) weight = 1;
            /// The size of the content of the legend panels in the row or column.
            Fit(TrackSizeFit) fit = 2;
        }
    }

    /// A share of the space that the fitted rows or columns leave.
    message TrackSizeWeight {
        /// The weight, which must be present, finite and positive.
        optional double weight = 1;
    }

    /// The size of the content of the legend panels in the row or column.
    message TrackSizeFit {}

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
