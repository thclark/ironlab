//! Wire types of `ironlab/ir/v0/figure.proto`.

proto_file! {
    /// A figure: the root of the retained IR, holding axes, data and axis links.
    message Figure {
        /// The version of the figure schema that the message conforms to, as
        /// `major.minor.patch`. This is field 1 in every version of the schema, so
        /// that a reader can check compatibility before decoding the other fields.
        string schema_version = 1;
        /// The node identifier of the figure, unique within the figure.
        optional uint64 id = 2;
        /// The title drawn above all axes; absent when there is no title.
        message Text title = 3;
        /// The physical size of the figure.
        message FigureSize size = 4;
        /// The font set used for all text.
        enum FontSetId font_set = 5;
        /// The base font size in points; titles and tick labels are scaled from it.
        optional double font_size_pt = 6;
        /// The colour of the figure background.
        message Color background = 7;
        /// The grid of cells in which axes are placed.
        message TileLayout layout = 8;
        /// The numeric arrays referenced by artists, keyed by data identifier.
        map<uint64, message NdArray> data = 9;
        /// The axes of the figure, in drawing order.
        repeated message Axes axes = 10;
        /// The groups of axes whose limits are linked along a dimension.
        repeated message AxisLink links = 11;
        /// A record of the software that produced the figure.
        message Provenance provenance = 12;
        /// Named values that describe the figure, used to sort, filter and search
        /// collections of figures, keyed by name and written in ascending order of the
        /// UTF-8 bytes of the name.
        map<string, message Parameter> parameters = 13;
    }

    /// The physical size of a figure.
    message FigureSize {
        /// The width in millimetres.
        optional double width_mm = 1;
        /// The height in millimetres.
        optional double height_mm = 2;
    }

    /// The identifier of a bundled font set.
    enum FontSetId {
        /// STIX Two Text for text and STIX Two Math for mathematics.
        StixTwo = 1;
    }

    /// The grid of cells in which the axes of a figure are placed.
    message TileLayout {
        /// The number of rows; at least one.
        optional uint32 rows = 1;
        /// The number of columns; at least one.
        optional uint32 cols = 2;
    }

    /// A record of the software that produced a figure.
    message Provenance {
        /// The version of IronLAB that wrote the figure.
        string ironlab_version = 1;
        /// The name and version of the mathematics typesetter.
        string typesetter = 2;
        /// The names of the fonts used for text.
        repeated string fonts = 3;
    }

    /// A named value that describes a figure.
    message Parameter {
        /// The kind of value; it must be set.
        oneof kind: ParameterKind {
            /// A boolean.
            Bool(ParameterBool) bool_value = 1;
            /// A signed 64-bit integer.
            Integer(ParameterInteger) integer_value = 2;
            /// A double-precision floating-point number.
            Number(ParameterNumber) number_value = 3;
            /// A string of Unicode text.
            String(ParameterString) string_value = 4;
        }
    }

    /// A boolean parameter.
    message ParameterBool {
        /// The value; it must be present.
        optional bool value = 1;
    }

    /// A signed 64-bit integer parameter.
    message ParameterInteger {
        /// The value; it must be present.
        optional int64 value = 1;
    }

    /// A double-precision floating-point number parameter.
    message ParameterNumber {
        /// The value, which must be present and, in a valid figure, finite.
        optional double value = 1;
    }

    /// A string parameter.
    message ParameterString {
        /// The value, which may be empty.
        string value = 1;
    }
}
