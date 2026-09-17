//! Wire types of `ironlab/ir/v0/style.proto`.

proto_file! {
    /// An sRGB colour with straight (non-premultiplied) alpha, each component from 0
    /// to 1.
    message Color {
        /// The red component.
        float r = 1;
        /// The green component.
        float g = 2;
        /// The blue component.
        float b = 3;
        /// The opacity, from 0 (transparent) to 1 (opaque).
        float a = 4;
    }

    /// How a colour is chosen for a stroke, fill or marker.
    message ColorSpec {
        /// The kind of colour; unset means the default of the context.
        oneof kind: ColorSpecKind {
            /// The renderer chooses the colour.
            Auto(ColorSpecAuto) automatic = 1;
            /// A fixed colour.
            Rgba(ColorSpecRgba) rgba = 2;
            /// Nothing is drawn.
            None(ColorSpecNone) none = 3;
            /// The colour is taken from the axes colormap.
            Colormapped(ColorSpecColormapped) colormapped = 4;
        }
    }

    /// The renderer chooses the colour: for series the next colour of the axes colour
    /// order, and inside a scatter marker the scatter colour.
    message ColorSpecAuto {}

    /// A fixed colour.
    message ColorSpecRgba {
        /// The colour to use.
        message Color color = 1;
    }

    /// Nothing is drawn.
    message ColorSpecNone {}

    /// The colour is taken from the axes colormap, indexed by the data value scaled
    /// into the axes colour limits.
    message ColorSpecColormapped {}

    /// The style of a stroked line.
    message LineStyle {
        /// The line colour.
        message ColorSpec color = 1;
        /// The line width in points.
        optional double width_pt = 2;
        /// The dash pattern of the line.
        enum DashStyle dash = 3;
    }

    /// The dash pattern of a stroked line.
    enum DashStyle {
        /// A continuous line.
        Solid = 1;
        /// A line of dashes.
        Dashed = 2;
        /// A line of dots.
        Dotted = 3;
        /// A line of alternating dashes and dots.
        DashDot = 4;
        /// No line is drawn.
        None = 5;
    }

    /// The style of the markers drawn at data points.
    message MarkerStyle {
        /// The marker shape.
        enum MarkerShape shape = 1;
        /// The marker size in points, measured as the width of the marker.
        optional double size_pt = 2;
        /// The colour of the marker interior.
        message ColorSpec face = 3;
        /// The colour of the marker outline.
        message ColorSpec edge = 4;
    }

    /// The shape of a marker.
    enum MarkerShape {
        /// No marker is drawn.
        None = 1;
        /// A circle.
        Circle = 2;
        /// An axis-aligned square.
        Square = 3;
        /// A square rotated by 45 degrees.
        Diamond = 4;
        /// A triangle pointing upwards.
        TriangleUp = 5;
        /// A triangle pointing downwards.
        TriangleDown = 6;
        /// A plus sign.
        Plus = 7;
        /// A diagonal cross.
        Cross = 8;
        /// A small filled dot.
        Point = 9;
    }
}
