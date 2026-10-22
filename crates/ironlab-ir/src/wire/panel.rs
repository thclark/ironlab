//! Wire types of `ironlab/ir/v0/panel.proto`.

proto_file! {
    /// A node of the figure that occupies cells of the tile layout and holds one key,
    /// such as a legend or a colourbar.
    message Panel {
        /// The node identifier of the panel, unique within the figure; it must be
        /// present.
        optional uint64 id = 1;
        /// The cells of the figure's tile layout that the panel occupies.
        message Cell cell = 2;
        /// Where the content is placed within the panel.
        enum PanelLocation align = 3;
        /// What the panel holds; it must be present.
        message PanelContent content = 4;
        /// The heading drawn at the top of the panel's content; absent when there is no
        /// title.
        message Text title = 5;
        /// How the title is aligned with the content; absent means left.
        enum TitleAlign title_align = 6;
    }

    /// The placement of the content of a panel within the panel.
    enum PanelLocation {
        /// The centre.
        Center = 1;
        /// The centre of the top edge.
        North = 2;
        /// The centre of the bottom edge.
        South = 3;
        /// The centre of the right edge.
        East = 4;
        /// The centre of the left edge.
        West = 5;
        /// The top-right corner.
        NorthEast = 6;
        /// The top-left corner.
        NorthWest = 7;
        /// The bottom-right corner.
        SouthEast = 8;
        /// The bottom-left corner.
        SouthWest = 9;
    }

    /// What a panel holds.
    message PanelContent {
        /// The kind of content; it must be set.
        oneof kind: PanelContentKind {
            /// A legend.
            Legend(PanelContentLegend) legend = 1;
            /// A colourbar.
            Colorbar(PanelContentColorbar) colorbar = 2;
        }
    }

    /// A colourbar held by a panel, which shows a colour scale along the panel's cells.
    message PanelColorbar {
        /// The node identifier of the colour scale that the bar shows; it must be present.
        optional uint64 scale = 1;
        /// The direction in which the bar runs.
        enum ColorbarOrientation orientation = 2;
        /// The length of the bar and its keys as a fraction of the span of the plot boxes
        /// beside it; absent means the automatic length, the whole span for a vertical
        /// bar and half of it for a horizontal one.
        optional double length = 3;
        /// Where the title of the scale is drawn beside the bar.
        enum ColorbarTitlePlacement title_placement = 4;
    }

    /// The direction in which a panel colourbar runs.
    enum ColorbarOrientation {
        /// Upright, with low values at the bottom.
        Vertical = 1;
        /// Flat, with low values at the left.
        Horizontal = 2;
    }

    /// A colourbar, which shows a colour scale along the cells of the panel.
    message PanelContentColorbar {
        /// The colourbar; it must be present.
        message PanelColorbar colorbar = 1;
    }

    /// A legend, which lists the entries of every axes whose legend names the panel.
    message PanelContentLegend {
        /// The legend; absent means the default legend of a panel.
        message PanelLegend legend = 1;
    }
}
