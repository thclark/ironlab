//! Wire types of `ironlab/ir/v0/legend.proto`.

proto_file! {
    /// The legend of an axes, which lists the artists of the axes that have a label.
    message Legend {
        /// Where the entries are listed; unset means the default of the context.
        oneof kind: LegendKind {
            /// The entries are listed in a legend inside the plot area of the axes.
            Inside(LegendInside) inside = 1;
            /// The entries are listed in a panel of the figure that holds a legend.
            Panel(LegendPanel) panel = 2;
        }
    }

    /// The entries are listed in a legend inside the plot area of the axes.
    message LegendInside {
        /// The legend; absent means the default inside legend.
        message InsideLegend inside = 1;
    }

    /// The entries are listed in a panel of the figure that holds a legend.
    message LegendPanel {
        /// The node identifier of the panel; it must be present.
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
        /// Which entries the legend lists, and in what order; absent means automatic.
        message LegendEntries entries = 4;
    }

    /// The legend content of a panel, which lists the entries of every axes whose legend
    /// names the panel.
    message PanelLegend {
        /// Whether the area of the panel is outlined by a border drawn as the box of an
        /// axes is drawn.
        optional bool boxed = 1;
        /// How the entries are arranged in columns.
        message LegendEntryLayout layout = 2;
        /// Which entries the panel lists, and in what order; absent means automatic.
        message LegendEntries entries = 3;
    }

    /// Which entries a legend lists, and in what order.
    message LegendEntries {
        /// The kind of list; unset means automatic.
        oneof kind: LegendEntriesKind {
            /// Every artist with a label, in the order of the legend.
            Auto(LegendEntriesAuto) automatic = 1;
            /// Exactly the given items, in the given order.
            Explicit(LegendEntriesExplicit) explicit_items = 2;
        }
    }

    /// Every artist with a label, in the order of the legend.
    message LegendEntriesAuto {}

    /// Exactly the given items, in the given order.
    message LegendEntriesExplicit {
        /// The items, in the order the legend lists them.
        repeated message LegendItem items = 1;
    }

    /// One item of an explicit list of legend entries.
    message LegendItem {
        /// What the item lists; it must be present.
        message LegendItemContent content = 1;
        /// Whether the item spans every column of the legend, on a row of its own.
        optional bool full_row = 2;
    }

    /// What an item of an explicit list of legend entries lists.
    message LegendItemContent {
        /// The kind of content; it must be set.
        oneof kind: LegendItemContentKind {
            /// An artist of the figure.
            Artist(LegendItemContentArtist) artist = 1;
            /// A label beside a sample, standing for no artist.
            Custom(LegendItemContentCustom) custom = 2;
            /// A colour scale of the figure.
            ColorScale(LegendItemContentColorScale) color_scale = 3;
        }
    }

    /// A colour scale of the figure, shown as its categories or as a bar.
    message LegendItemContentColorScale {
        /// The node identifier of the colour scale; it must be present.
        optional uint64 scale = 1;
        /// The number of entry rows along which a vertical bar runs; absent means 5.
        optional uint32 rows = 2;
    }

    /// An artist of the figure, labelled by its label.
    message LegendItemContentArtist {
        /// The node identifier of the artist; it must be present.
        optional uint64 artist = 1;
    }

    /// A label beside a sample, standing for no artist.
    message LegendItemContentCustom {
        /// The label shown beside the sample.
        message Text label = 1;
        /// The sample drawn beside the label; it must be present.
        message LegendSample sample = 2;
    }

    /// The sample of a legend item that stands for no artist.
    message LegendSample {
        /// The kind of sample; it must be set.
        oneof kind: LegendSampleKind {
            /// A sample drawn as the sample of a line artist is drawn.
            Line(LegendSampleLine) line = 1;
            /// A filled rectangle of a colour.
            Swatch(LegendSampleSwatch) swatch = 2;
        }
    }

    /// A sample drawn as the sample of a line artist is drawn.
    message LegendSampleLine {
        /// The style of the line.
        message LineStyle line = 1;
        /// The style of the marker at the centre of the line.
        message MarkerStyle marker = 2;
    }

    /// A filled rectangle of a colour.
    message LegendSampleSwatch {
        /// The colour of the rectangle.
        message Color color = 1;
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

    /// The placement of a legend inside the plot area of its axes.
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
