//! Wire types of `ironlab/ir/v0/edit.proto`.

// A oneof of the wire types holds its message by value, as `prost` generates it, so an
// edit that inserts an axes is much larger than the other edits. Transactions are
// decoded into the domain types at once, so the size of the wire types does not matter.
#![expect(
    clippy::large_enum_variant,
    reason = "oneof variants hold their messages by value, as prost generates them"
)]

proto_file! {
    /// An ordered list of edits that are applied to a figure together or not at all.
    message Transaction {
        /// The edits, in the order in which they are applied.
        repeated message Edit edits = 1;
    }

    /// A change to a figure.
    message Edit {
        /// The kind of edit; it must be set.
        oneof kind: EditKind {
            /// Replaces the value of a property of a node.
            Set(EditSet) set_property = 1;
            /// Inserts an axes or an artist, with its subtree.
            Insert(EditInsert) insert_node = 2;
            /// Removes a node and its subtree.
            Remove(EditRemove) remove_node = 3;
            /// Moves a node within its parent or to another parent.
            Move(EditMove) move_node = 4;
            /// Creates or replaces a data array.
            PutData(EditPutData) put_data = 5;
            /// Appends entries to a data array along its first dimension.
            AppendData(EditAppendData) append_data = 6;
            /// Removes a data array.
            RemoveData(EditRemoveData) remove_data = 7;
        }
    }

    /// Replaces the value of a property of a node.
    message EditSet {
        /// The identifier of the node; it must be present.
        optional uint64 node = 1;
        /// The path of the property within the node: Protocol Buffers field names
        /// separated by dots, such as `x.limits.min`; it must be a valid path.
        string path = 2;
        /// The new value; it must be present.
        message Value value = 3;
    }

    /// Inserts an axes into the figure, or an artist into an axes, with its subtree.
    message EditInsert {
        /// The identifier of the parent: the figure for an axes, or an axes for an
        /// artist; it must be present.
        optional uint64 parent = 1;
        /// The position in the parent's list; absent to append.
        optional uint32 index = 2;
        /// The node to insert; it must be present.
        message Node node = 3;
    }

    /// Removes a node and its subtree.
    message EditRemove {
        /// The identifier of the axes or artist; it must be present.
        optional uint64 node = 1;
    }

    /// Moves an axes within the figure, or an artist within its axes or to another axes.
    message EditMove {
        /// The identifier of the axes or artist; it must be present.
        optional uint64 node = 1;
        /// The identifier of the new parent; it must be present.
        optional uint64 parent = 2;
        /// The position in the new parent's list after the node is removed from its old
        /// position; absent to append.
        optional uint32 index = 3;
    }

    /// Creates a data array or replaces an existing one.
    message EditPutData {
        /// The data identifier; it must be present.
        optional uint64 id = 1;
        /// The new array; it must be present.
        message NdArray array = 2;
    }

    /// Appends entries to an existing array along its first dimension.
    message EditAppendData {
        /// The data identifier; it must be present.
        optional uint64 id = 1;
        /// The entries to append; it must be present.
        message NdArray array = 2;
        /// The number of entries along the first dimension to keep after appending,
        /// counted from the end; absent to keep every entry.
        optional uint64 retain = 3;
    }

    /// Removes a data array.
    message EditRemoveData {
        /// The data identifier; it must be present.
        optional uint64 id = 1;
    }

    /// A node that can be inserted into a figure, with its subtree.
    message Node {
        /// The kind of node; it must be set.
        oneof kind: NodeKind {
            /// An axes with its artists.
            Axes(Axes) axes = 1;
            /// An artist.
            Artist(Artist) artist = 2;
        }
    }

    /// The value of a property.
    message Value {
        /// The type of value; it must be set.
        oneof kind: ValueKind {
            /// No value: an optional property is cleared.
            Unset(ValueUnset) unset = 1;
            /// A boolean.
            Bool(ValueBool) bool_value = 2;
            /// An unsigned 32-bit integer.
            Uint32(ValueUint32) uint32_value = 3;
            /// A double-precision number.
            Double(ValueDouble) double_value = 4;
            /// A single-precision number: a component of a colour.
            Float(ValueFloat) float_value = 5;
            /// A string.
            String(ValueString) string_value = 6;
            /// A reference to a data array.
            DataId(ValueDataId) data_id_value = 7;
            /// A list of double-precision numbers.
            Doubles(ValueDoubles) doubles_value = 8;
            /// A text.
            Text(ValueText) text_value = 9;
            /// How the source of a text is interpreted.
            Interpreter(ValueInterpreter) interpreter_value = 10;
            /// The physical size of a figure.
            FigureSize(ValueFigureSize) figure_size_value = 11;
            /// A bundled font set.
            FontSetId(ValueFontSetId) font_set_id_value = 12;
            /// A colour.
            Color(ValueColor) color_value = 13;
            /// The tile layout of a figure.
            TileLayout(ValueTileLayout) tile_layout_value = 14;
            /// The axis links of a figure.
            Links(ValueLinks) links_value = 15;
            /// The named parameters of a figure.
            Parameters(ValueParameters) parameters_value = 16;
            /// The cells occupied by an axes.
            Cell(ValueCell) cell_value = 17;
            /// The projection of an axes.
            Projection(ValueProjection) projection_value = 18;
            /// The camera view of a three-dimensional axes.
            View3d(ValueView3d) view3d_value = 19;
            /// A coordinate axis.
            Axis(ValueAxis) axis_value = 20;
            /// The scale of a coordinate axis.
            Scale(ValueScale) scale_value = 21;
            /// A range of data values.
            Limits(ValueLimits) limits_value = 22;
            /// A colormap name.
            ColormapName(ValueColormapName) colormap_name_value = 23;
            /// A legend.
            Legend(ValueLegend) legend_value = 24;
            /// The placement of a legend.
            LegendLocation(ValueLegendLocation) legend_location_value = 25;
            /// How a colour is chosen.
            ColorSpec(ValueColorSpec) color_spec_value = 26;
            /// The style of a stroked line.
            LineStyle(ValueLineStyle) line_style_value = 27;
            /// A dash pattern.
            DashStyle(ValueDashStyle) dash_style_value = 28;
            /// The style of markers.
            MarkerStyle(ValueMarkerStyle) marker_style_value = 29;
            /// A marker shape.
            MarkerShape(ValueMarkerShape) marker_shape_value = 30;
            /// The size of scatter markers.
            ScatterSize(ValueScatterSize) scatter_size_value = 31;
            /// The colour of scatter markers.
            ScatterColor(ValueScatterColor) scatter_color_value = 32;
            /// The grid of a contour or surface.
            Grid(ValueGrid) grid_value = 33;
            /// The levels of a contour.
            Levels(ValueLevels) levels_value = 34;
            /// The placement of a contour in three-dimensional axes.
            ContourPlacement(ValueContourPlacement) contour_placement_value = 35;
            /// The scaling of quiver arrows.
            QuiverScale(ValueQuiverScale) quiver_scale_value = 36;
        }
    }

    /// No value: an optional property is cleared.
    message ValueUnset {}

    /// A boolean value.
    message ValueBool {
        /// The value; it must be present.
        optional bool value = 1;
    }

    /// An unsigned 32-bit integer value.
    message ValueUint32 {
        /// The value; it must be present.
        optional uint32 value = 1;
    }

    /// A double-precision number value.
    message ValueDouble {
        /// The value; it must be present.
        optional double value = 1;
    }

    /// A single-precision number value.
    message ValueFloat {
        /// The value; it must be present.
        optional float value = 1;
    }

    /// A string value.
    message ValueString {
        /// The value, which may be empty.
        string value = 1;
    }

    /// A reference to a data array.
    message ValueDataId {
        /// The data identifier; it must be present.
        optional uint64 value = 1;
    }

    /// A list of double-precision numbers.
    message ValueDoubles {
        /// The values, which may be empty.
        repeated double values = 1;
    }

    /// A text value.
    message ValueText {
        /// The text; it must be present.
        message Text value = 1;
    }

    /// An interpreter value.
    message ValueInterpreter {
        /// The interpreter; it must be specified.
        enum Interpreter value = 1;
    }

    /// A figure size value.
    message ValueFigureSize {
        /// The size; it must be present.
        message FigureSize value = 1;
    }

    /// A font set value.
    message ValueFontSetId {
        /// The font set; it must be specified.
        enum FontSetId value = 1;
    }

    /// A colour value.
    message ValueColor {
        /// The colour; it must be present.
        message Color value = 1;
    }

    /// A tile layout value.
    message ValueTileLayout {
        /// The layout; it must be present.
        message TileLayout value = 1;
    }

    /// The axis links of a figure.
    message ValueLinks {
        /// The links, which may be empty.
        repeated message AxisLink links = 1;
    }

    /// The named parameters of a figure.
    message ValueParameters {
        /// The parameters keyed by name, which may be empty.
        map<string, message Parameter> parameters = 1;
    }

    /// A cell value.
    message ValueCell {
        /// The cell; it must be present.
        message Cell value = 1;
    }

    /// A projection value.
    message ValueProjection {
        /// The projection; it must be present.
        message Projection value = 1;
    }

    /// A three-dimensional view value.
    message ValueView3d {
        /// The view; it must be present.
        message View3d value = 1;
    }

    /// A coordinate axis value.
    message ValueAxis {
        /// The axis; it must be present.
        message Axis value = 1;
    }

    /// A scale value.
    message ValueScale {
        /// The scale; it must be specified.
        enum Scale value = 1;
    }

    /// A limits value.
    message ValueLimits {
        /// The limits; it must be present.
        message Limits value = 1;
    }

    /// A colormap name value.
    message ValueColormapName {
        /// The colormap name; it must be specified.
        enum ColormapName value = 1;
    }

    /// A legend value.
    message ValueLegend {
        /// The legend; it must be present.
        message Legend value = 1;
    }

    /// A legend location value.
    message ValueLegendLocation {
        /// The location; it must be specified.
        enum LegendLocation value = 1;
    }

    /// A colour specification value.
    message ValueColorSpec {
        /// The colour specification; it must be present.
        message ColorSpec value = 1;
    }

    /// A line style value.
    message ValueLineStyle {
        /// The line style; it must be present.
        message LineStyle value = 1;
    }

    /// A dash style value.
    message ValueDashStyle {
        /// The dash style; it must be specified.
        enum DashStyle value = 1;
    }

    /// A marker style value.
    message ValueMarkerStyle {
        /// The marker style; it must be present.
        message MarkerStyle value = 1;
    }

    /// A marker shape value.
    message ValueMarkerShape {
        /// The marker shape; it must be specified.
        enum MarkerShape value = 1;
    }

    /// A scatter size value.
    message ValueScatterSize {
        /// The scatter size; it must be present.
        message ScatterSize value = 1;
    }

    /// A scatter colour value.
    message ValueScatterColor {
        /// The scatter colour; it must be present.
        message ScatterColor value = 1;
    }

    /// A grid value.
    message ValueGrid {
        /// The grid; it must be present.
        message Grid value = 1;
    }

    /// A levels value.
    message ValueLevels {
        /// The levels; it must be present.
        message Levels value = 1;
    }

    /// A contour placement value.
    message ValueContourPlacement {
        /// The placement; it must be present.
        message ContourPlacement value = 1;
    }

    /// A quiver scale value.
    message ValueQuiverScale {
        /// The quiver scale; it must be present.
        message QuiverScale value = 1;
    }
}
