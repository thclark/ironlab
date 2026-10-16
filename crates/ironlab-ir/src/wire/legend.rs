//! Wire types of `ironlab/ir/v0/legend.proto`.

proto_file! {
    /// The legend of an axes, which lists the artists of the axes that have a display
    /// name.
    message Legend {
        /// Where the entries are listed; unset means the default of the context.
        oneof kind: LegendKind {
            /// The entries are listed in a legend inside the plot area of the axes.
            Inside(LegendInside) inside = 1;
            /// The entries are listed in a legend panel of the figure.
            Panel(LegendPanel) panel = 2;
        }
    }

    /// The entries are listed in a legend inside the plot area of the axes.
    message LegendInside {
        /// The legend; absent means the default inside legend.
        message InsideLegend inside = 1;
    }

    /// The entries are listed in a legend panel of the figure.
    message LegendPanel {
        /// The node identifier of the legend panel; it must be present.
        optional uint64 panel = 1;
    }

    /// A legend drawn inside the plot area of its axes.
    message InsideLegend {
        /// Where the legend is placed inside the plot area.
        enum LegendLocation location = 1;
        /// Whether the legend is drawn with a background and outline.
        optional bool boxed = 2;
        /// How the entries are arranged in columns.
        message LegendEntryLayout layout = 3;
    }

    /// A legend of the figure that occupies cells of the tile layout and lists the
    /// entries of every axes whose legend names it.
    message PanelLegend {
        /// The node identifier of the panel, unique within the figure.
        optional uint64 id = 1;
        /// The cells of the figure's tile layout that the panel occupies.
        message Cell cell = 2;
        /// Where the block of entries is placed within the panel.
        enum LegendLocation align = 3;
        /// Whether the area of the panel is outlined by a border drawn as the box of an
        /// axes is drawn.
        optional bool boxed = 4;
        /// How the entries are arranged in columns.
        message LegendEntryLayout layout = 5;
    }

    /// How the entries of a legend are arranged in columns.
    message LegendEntryLayout {
        /// The number of columns.
        message LegendEntryColumns columns = 1;
        /// The order in which the entries fill the columns.
        enum LegendEntryOrder order = 2;
    }

    /// The number of columns in which the entries of a legend are arranged.
    message LegendEntryColumns {
        /// The kind of number; unset means the default of the context.
        oneof kind: LegendEntryColumnsKind {
            /// The fewest columns in which the entries fit the available height.
            Auto(LegendEntryColumnsAuto) automatic = 1;
            /// A fixed number of columns.
            Count(LegendEntryColumnsCount) count = 2;
        }
    }

    /// The fewest columns in which the entries fit the available height.
    message LegendEntryColumnsAuto {}

    /// A fixed number of columns.
    message LegendEntryColumnsCount {
        /// The number of columns, which must be present and at least one.
        optional uint32 count = 1;
    }

    /// The order in which the entries of a legend fill its columns.
    enum LegendEntryOrder {
        /// Each column is filled from top to bottom before the next column is started.
        ColumnMajor = 1;
        /// Each row is filled from left to right before the next row is started.
        RowMajor = 2;
    }

    /// The placement of a legend inside the plot area of its axes, or of the block of
    /// entries inside a legend panel.
    enum LegendLocation {
        /// The top-right corner.
        NorthEast = 1;
        /// The top-left corner.
        NorthWest = 2;
        /// The bottom-right corner.
        SouthEast = 3;
        /// The bottom-left corner.
        SouthWest = 4;
        /// The centre of the top edge.
        North = 5;
        /// The centre of the bottom edge.
        South = 6;
        /// The centre of the right edge.
        East = 7;
        /// The centre of the left edge.
        West = 8;
        /// The corner that overlaps the least data.
        Best = 9;
        /// The centre.
        Center = 10;
    }
}
