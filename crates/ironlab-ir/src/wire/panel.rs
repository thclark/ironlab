//! Wire types of `ironlab/ir/v0/panel.proto`.

proto_file! {
    /// A node of the figure that occupies cells of the tile layout and holds one key,
    /// such as a legend.
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
        }
    }

    /// A legend, which lists the entries of every axes whose legend names the panel.
    message PanelContentLegend {
        /// The legend; absent means the default legend of a panel.
        message PanelLegend legend = 1;
    }
}
