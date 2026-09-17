//! Wire types of `ironlab/ir/v0/axes.proto`.

proto_file! {
    /// A plotting region placed in one or more cells of the figure's tile layout.
    message Axes {
        /// The node identifier of the axes, unique within the figure.
        optional uint64 id = 1;
        /// The cells of the figure's tile layout that the axes occupies.
        message Cell cell = 2;
        /// Whether the axes is two- or three-dimensional, with its 3D view.
        message Projection projection = 3;
        /// The title drawn above the axes; absent when there is no title.
        message Text title = 4;
        /// The horizontal axis in 2D, or the first horizontal axis in 3D.
        message Axis x = 5;
        /// The vertical axis in 2D, or the second horizontal axis in 3D.
        message Axis y = 6;
        /// The vertical axis in 3D; ignored by 2D axes.
        message Axis z = 7;
        /// Whether the full outline of the plot box is drawn, rather than only the
        /// edges that carry tick labels.
        optional bool r#box = 8;
        /// The colormap used by colormapped artists in this axes.
        enum ColormapName colormap = 9;
        /// The data values mapped to the first and last colours of the colormap.
        message Limits clim = 10;
        /// The legend; absent when no legend is shown.
        message Legend legend = 11;
        /// The artists drawn in this axes, in drawing order.
        repeated message Artist artists = 12;
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

    /// The projection of an axes.
    message Projection {
        /// The kind of projection; unset means the default of the context.
        oneof kind: ProjectionKind {
            /// A two-dimensional Cartesian axes.
            TwoD(ProjectionTwoD) two_d = 1;
            /// A three-dimensional Cartesian axes.
            ThreeD(ProjectionThreeD) three_d = 2;
        }
    }

    /// A two-dimensional Cartesian axes.
    message ProjectionTwoD {}

    /// A three-dimensional Cartesian axes viewed through an orthographic camera.
    message ProjectionThreeD {
        /// The camera view.
        message View3d view3d = 1;
    }

    /// The orthographic camera view of a three-dimensional axes.
    message View3d {
        /// The rotation about the vertical axis in degrees, measured counterclockwise
        /// from the negative y axis when viewed from above.
        optional double azimuth_deg = 1;
        /// The angle of the view direction above the x-y plane in degrees, from -90
        /// to 90.
        optional double elevation_deg = 2;
        /// The magnification of the projected box, where 1 fits the box to the plot
        /// area.
        optional double zoom = 3;
        /// The horizontal offset of the projected box as a fraction of the plot
        /// area's width.
        optional double pan_x = 4;
        /// The vertical offset of the projected box as a fraction of the plot area's
        /// height.
        optional double pan_y = 5;
    }

    /// One coordinate axis of an axes.
    message Axis {
        /// The axis label; absent when there is no label.
        message Text label = 1;
        /// The mapping from data values to positions along the axis.
        enum Scale scale = 2;
        /// The range of data values shown along the axis.
        message Limits limits = 3;
        /// Whether grid lines are drawn at the major ticks of this axis.
        optional bool grid = 4;
    }

    /// The mapping from data values to positions along an axis.
    enum Scale {
        /// Positions are proportional to values.
        Linear = 1;
        /// Positions are proportional to the base-10 logarithm of values.
        Log = 2;
    }

    /// A range of data values.
    message Limits {
        /// The kind of range; unset means the default of the context.
        oneof kind: LimitsKind {
            /// The range is computed from the data.
            Auto(LimitsAuto) automatic = 1;
            /// The range is fixed.
            Manual(LimitsManual) manual = 2;
        }
    }

    /// The range is computed from the data.
    message LimitsAuto {}

    /// The range is fixed.
    message LimitsManual {
        /// The lower bound of the range.
        optional double min = 1;
        /// The upper bound of the range.
        optional double max = 2;
    }

    /// The name of a colormap.
    enum ColormapName {
        /// The perceptually uniform blue–green–yellow colormap.
        Viridis = 1;
        /// The perceptually uniform blue–yellow colormap designed for colour-vision
        /// deficiency.
        Cividis = 2;
        /// The perceptually uniform black–purple–cream colormap.
        Magma = 3;
        /// The perceptually uniform black–red–yellow colormap.
        Inferno = 4;
        /// The perceptually uniform blue–purple–yellow colormap.
        Plasma = 5;
        /// The diverging blue–white–red colormap.
        Coolwarm = 6;
        /// The linear black–white colormap.
        Gray = 7;
    }

    /// A legend listing the artists of an axes that have a display name.
    message Legend {
        /// Where the legend is placed inside the plot area.
        enum LegendLocation location = 1;
        /// Whether the legend is drawn with a background and outline.
        optional bool boxed = 2;
    }

    /// The placement of a legend inside the plot area.
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
    }
}
