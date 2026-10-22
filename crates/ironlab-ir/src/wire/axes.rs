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
        /// The legend; absent when no legend is shown.
        message Legend legend = 11;
        /// The artists drawn in this axes, in drawing order.
        repeated message Artist artists = 12;
        /// How the title is aligned with the plot area.
        enum TitleAlign title_align = 13;
        /// The ratios of the length of one data unit of y, and of z, to that of x.
        message Aspect aspect = 14;
        /// How a fixed aspect is kept.
        enum AspectAdjust aspect_adjust = 15;
        /// The colourbar drawn inside the plot area; absent when the axes shows none.
        message InsideColorbar colorbar = 16;
        /// The numbers and names of the colormap and colour limits of the axes, which
        /// colour scales replaced.
        reserved 9, 10, colormap, clim;
    }

    /// A colourbar drawn inside the plot area of an axes, along one of its edges.
    message InsideColorbar {
        /// The node identifier of the colour scale that the bar shows; it must be present.
        optional uint64 scale = 1;
        /// The edge of the plot area along which the bar lies.
        enum InsideColorbarSide side = 2;
        /// The length of the bar and its keys as a fraction of the edge; absent means 0.8.
        optional double length = 3;
        /// Where the title of the scale is drawn beside the bar.
        enum ColorbarTitlePlacement title_placement = 4;
    }

    /// The edge of the plot area along which an inside colourbar lies.
    enum InsideColorbarSide {
        /// Along the bottom edge, horizontal.
        South = 1;
        /// Along the top edge, horizontal.
        North = 2;
        /// Along the right edge, vertical.
        East = 3;
        /// Along the left edge, vertical.
        West = 4;
    }

    /// Where the title of a colour scale is drawn beside a colourbar of the scale.
    enum ColorbarTitlePlacement {
        /// Beyond the tick labels, centred on the bar, reading from bottom to top beside a
        /// vertical bar.
        Along = 1;
        /// As an upright heading beside the bar.
        Above = 2;
    }

    /// How a title is aligned with what it heads: the plot area of an axes, or the
    /// content of a panel.
    enum TitleAlign {
        /// The title is centred over what it heads.
        Center = 1;
        /// The left edge of the title is on the left edge of what it heads.
        Left = 2;
        /// The right edge of the title is on the right edge of what it heads.
        Right = 3;
    }

    /// The ratios of the length of one data unit of y, and of z, to that of one data unit
    /// of x; on a logarithmic axis the data unit is one decade.
    message Aspect {
        /// The kind of aspect; unset means the default of the context.
        oneof kind: AspectKind {
            /// The plot area fills its space whatever the ratio.
            Auto(AspectAuto) automatic = 1;
            /// One data unit of y is drawn as long as one data unit of x.
            Equal(AspectEqual) equal = 2;
            /// One data unit of y and of z is drawn a fixed number of times as long as
            /// one of x.
            Ratio(AspectRatio) ratio = 3;
        }
    }

    /// The plot area fills its space whatever the ratio of the data units.
    message AspectAuto {}

    /// One data unit of y is drawn as long as one data unit of x.
    message AspectEqual {}

    /// One data unit of y, and of z, is drawn a fixed number of times as long as one of x.
    message AspectRatio {
        /// How many times as long a unit of y is drawn as a unit of x; finite and positive.
        optional double y = 1;
        /// How many times as long a unit of z is drawn as a unit of x; finite and positive.
        optional double z = 2;
    }

    /// How an axes keeps a fixed aspect.
    enum AspectAdjust {
        /// The limits are kept, and the plot area or the 3D box takes their shape.
        Box = 1;
        /// The plot area or the 3D cube is kept, and limits widen about their centres.
        Limits = 2;
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
        /// area's width, increasing to the right.
        optional double pan_x = 4;
        /// The vertical offset of the projected box as a fraction of the plot area's
        /// height, increasing downwards.
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
        /// Whether the axis runs from its upper limit to its lower limit: from left to
        /// right for a horizontal axis, and from bottom to top for a vertical axis.
        optional bool reverse = 5;
        /// Where the major ticks of the axis are placed.
        message Ticks ticks = 6;
        /// What is written at the major ticks of the axis.
        message TickLabels tick_labels = 7;
    }

    /// Where the major ticks of an axis are placed.
    message Ticks {
        /// The kind of ticks; unset means the default of the context.
        oneof kind: TicksKind {
            /// The ticks are chosen at round values from the limits and the length of
            /// the axis.
            Auto(TicksAuto) automatic = 1;
            /// A tick at every value origin + k × step within the limits.
            Step(TicksStep) step = 2;
            /// A tick at each of the given values within the limits.
            Values(TicksValues) explicit_values = 3;
        }
    }

    /// The ticks are chosen at round values from the limits and the length of the axis.
    message TicksAuto {}

    /// A tick at every value origin + k × step, for every integer k, within the limits.
    message TicksStep {
        /// The distance between neighbouring ticks, in data units.
        optional double step = 1;
        /// A value at which a tick is placed if it lies within the limits.
        optional double origin = 2;
    }

    /// A tick at each of the given values within the limits.
    message TicksValues {
        /// The tick values.
        repeated double values = 1;
    }

    /// What is written at the major ticks of an axis.
    message TickLabels {
        /// The kind of labels; unset means the default of the context.
        oneof kind: TickLabelsKind {
            /// Each tick is labelled with its value.
            Auto(TickLabelsAuto) automatic = 1;
            /// The ticks are drawn without labels.
            Hidden(TickLabelsHidden) hidden = 2;
            /// Each tick is labelled with the text at the same position in the list.
            Text(TickLabelsText) text = 3;
        }
    }

    /// Each tick is labelled with its value.
    message TickLabelsAuto {}

    /// The ticks are drawn without labels.
    message TickLabelsHidden {}

    /// Each tick is labelled with the text at the same position in the list.
    message TickLabelsText {
        /// One label per tick value.
        repeated message Text labels = 1;
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
}
