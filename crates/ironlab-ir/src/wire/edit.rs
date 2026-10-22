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
            /// Inserts a data array, a colour scale, an axes, an artist or a panel, with its
            /// subtree.
            Insert(EditInsert) insert_node = 2;
            /// Removes a node and its subtree.
            Remove(EditRemove) remove_node = 3;
            /// Moves a node within its parent or to another parent.
            Move(EditMove) move_node = 4;
            /// Appends entries to a data array along its first dimension.
            AppendData(EditAppendData) append_data = 6;
            /// Replaces the values of an existing data array.
            ReplaceData(EditReplaceData) replace_data = 8;
        }
        /// The numbers and names of the edits that created and removed data arrays before
        /// data arrays were nodes, which are now inserted and removed as nodes.
        reserved 5, 7, put_data, remove_data;
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

    /// Inserts a data array, a colour scale, an axes or a panel into the figure, or an
    /// artist into an axes, with its subtree.
    message EditInsert {
        /// The identifier of the parent: the figure for a data array, a colour scale, an
        /// axes or a panel, or an axes for an artist; it must be present.
        optional uint64 parent = 1;
        /// The position in the parent's list; absent to append.
        optional uint32 index = 2;
        /// The node to insert; it must be present.
        message Node node = 3;
    }

    /// Removes a node and its subtree.
    message EditRemove {
        /// The identifier of the data array, axes, artist or panel; it must be present.
        optional uint64 node = 1;
    }

    /// Moves a data array, an axes or a panel within the figure, or an artist within its
    /// axes or to another axes.
    message EditMove {
        /// The identifier of the data array, axes, artist or panel; it must be present.
        optional uint64 node = 1;
        /// The identifier of the new parent; it must be present.
        optional uint64 parent = 2;
        /// The position in the new parent's list after the node is removed from its old
        /// position; absent to append.
        optional uint32 index = 3;
    }

    /// Replaces the values of an existing data array, which keeps its identifier and its
    /// name.
    message EditReplaceData {
        /// The identifier of the data array; it must be present.
        optional uint64 node = 1;
        /// The new values; it must be present.
        message NdArray array = 2;
    }

    /// Appends entries to an existing data array along its first dimension.
    message EditAppendData {
        /// The identifier of the data array; it must be present.
        optional uint64 node = 1;
        /// The entries to append; it must be present.
        message NdArray array = 2;
        /// The number of entries along the first dimension to keep after appending,
        /// counted from the end; absent to keep every entry.
        optional uint64 retain = 3;
    }

    /// A node that can be inserted into a figure, with its subtree.
    message Node {
        /// The kind of node; it must be set.
        oneof kind: NodeKind {
            /// An axes with its artists.
            Axes(Axes) axes = 1;
            /// An artist.
            Artist(Artist) artist = 2;
            /// A panel.
            Panel(Panel) panel = 4;
            /// A data array.
            DataArray(DataArray) data_array = 5;
            /// A colour scale.
            ColorScale(ColorScale) color_scale = 6;
        }
        /// The number and name of the legend panel, which a panel holding a legend
        /// replaced.
        reserved 3, panel_legend;
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
            /// The grid of a contour or surface.
            Grid(ValueGrid) grid_value = 33;
            /// The levels of a contour.
            Levels(ValueLevels) levels_value = 34;
            /// The placement of a contour in three-dimensional axes.
            ContourPlacement(ValueContourPlacement) contour_placement_value = 35;
            /// The scaling of quiver arrows.
            QuiverScale(ValueQuiverScale) quiver_scale_value = 36;
            /// The placement of an image in its axes.
            ImagePlacement(ValueImagePlacement) image_placement_value = 37;
            /// The centres of the first and last pixels of an image along an axis.
            PixelRange(ValuePixelRange) pixel_range_value = 38;
            /// The plane of an image.
            ImagePlane(ValueImagePlane) image_plane_value = 39;
            /// What is drawn for a value that a colour scale cannot place.
            OutOfRange(ValueOutOfRange) out_of_range_value = 40;
            /// The tags of a figure.
            Strings(ValueStrings) strings_value = 41;
            /// A legend drawn inside the plot area of its axes.
            InsideLegend(ValueInsideLegend) inside_legend_value = 42;
            /// How the entries of a legend are arranged in columns.
            LegendEntryLayout(ValueLegendEntryLayout) legend_entry_layout_value = 43;
            /// The number of columns of a legend.
            LegendEntryColumns(ValueLegendEntryColumns) legend_entry_columns_value = 44;
            /// The order in which the entries of a legend fill its columns.
            LegendEntryOrder(ValueLegendEntryOrder) legend_entry_order_value = 45;
            /// A reference to a node, such as the data array of an artist or the panel
            /// named by the legend of an axes.
            NodeId(ValueNodeId) node_id_value = 46;
            /// Where the major ticks of an axis are placed.
            Ticks(ValueTicks) ticks_value = 47;
            /// What is written at the major ticks of an axis.
            TickLabels(ValueTickLabels) tick_labels_value = 48;
            /// A list of texts.
            Texts(ValueTexts) texts_value = 49;
            /// How the values of one dimension of a series are placed.
            Coords(ValueCoords) coords_value = 50;
            /// A list of sizes of the rows or columns of the tile layout.
            TrackSizes(ValueTrackSizes) track_sizes_value = 51;
            /// The aspect of an axes.
            Aspect(ValueAspect) aspect_value = 52;
            /// How an axes keeps a fixed aspect.
            AspectAdjust(ValueAspectAdjust) aspect_adjust_value = 53;
            /// How the title of an axes is aligned with its plot area.
            TitleAlign(ValueTitleAlign) title_align_value = 54;
            /// The placement of the content of a panel.
            PanelLocation(ValuePanelLocation) panel_location_value = 55;
            /// What a panel holds.
            PanelContent(ValuePanelContent) panel_content_value = 56;
            /// The legend content of a panel.
            PanelLegend(ValuePanelLegend) panel_legend_value = 57;
            /// A category of values that a colour scale cannot place.
            ColorScaleOutOfRange(ValueColorScaleOutOfRange) color_scale_out_of_range_value = 58;
            /// The kind of a colour scale.
            ColorScaleKind(ValueColorScaleKind) color_scale_kind_value = 59;
            /// A continuous colour scale.
            ContinuousColorScale(ValueContinuousColorScale) continuous_color_scale_value = 60;
            /// A discrete colour scale.
            DiscreteColorScale(ValueDiscreteColorScale) discrete_color_scale_value = 61;
            /// An indexed colour scale.
            IndexedColorScale(ValueIndexedColorScale) indexed_color_scale_value = 62;
            /// A categorical colour scale.
            CategoricalColorScale(ValueCategoricalColorScale) categorical_color_scale_value = 63;
            /// The palette of a continuous or discrete colour scale.
            Colormap(ValueColormap) colormap_value = 64;
            /// The space in which a colour scale places values.
            ColorScaleNorm(ValueColorScaleNorm) color_scale_norm_value = 65;
            /// The band edges of a discrete colour scale.
            ColorScaleLevels(ValueColorScaleLevels) color_scale_levels_value = 66;
            /// The colourbar content of a panel.
            PanelColorbar(ValuePanelColorbar) panel_colorbar_value = 67;
            /// The direction in which a panel colourbar runs.
            ColorbarOrientation(ValueColorbarOrientation) colorbar_orientation_value = 68;
            /// The colourbar inside an axes.
            InsideColorbar(ValueInsideColorbar) inside_colorbar_value = 69;
            /// The edge along which an inside colourbar lies.
            InsideColorbarSide(ValueInsideColorbarSide) inside_colorbar_side_value = 70;
        }
        /// The number and name of the reference to a data array, which is now a
        /// reference to a node.
        reserved 7, data_id_value;
        /// The numbers and names of the size and colour of a scatter, which a series
        /// replaced with references to its size and colour data.
        reserved 31, 32, scatter_size_value, scatter_color_value;
    }

    /// A list of sizes of the rows or columns of the tile layout.
    message ValueTrackSizes {
        /// The sizes, which may be empty.
        repeated message TrackSize sizes = 1;
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

    /// A list of double-precision numbers.
    message ValueDoubles {
        /// The values, which may be empty.
        repeated double values = 1;
    }

    /// A list of strings.
    message ValueStrings {
        /// The values, in order, which may be empty.
        repeated string values = 1;
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

    /// A panel location value.
    message ValuePanelLocation {
        /// The location; it must be specified.
        enum PanelLocation value = 1;
    }

    /// A panel content value.
    message ValuePanelContent {
        /// The content; it must be present.
        message PanelContent value = 1;
    }

    /// A panel legend value.
    message ValuePanelLegend {
        /// The legend; it must be present.
        message PanelLegend value = 1;
    }

    /// An inside legend value.
    message ValueInsideLegend {
        /// The inside legend; it must be present.
        message InsideLegend value = 1;
    }

    /// A legend entry layout value.
    message ValueLegendEntryLayout {
        /// The layout; it must be present.
        message LegendEntryLayout value = 1;
    }

    /// A legend columns value.
    message ValueLegendEntryColumns {
        /// The number of columns; it must be present.
        message LegendEntryColumns value = 1;
    }

    /// A legend entry order value.
    message ValueLegendEntryOrder {
        /// The order; it must be specified.
        enum LegendEntryOrder value = 1;
    }

    /// A reference to a node.
    message ValueNodeId {
        /// The node identifier; it must be present.
        optional uint64 value = 1;
    }

    /// An aspect value.
    message ValueAspect {
        /// The aspect; it must be present.
        message Aspect value = 1;
    }

    /// An aspect adjustment value.
    message ValueAspectAdjust {
        /// The adjustment; it must be specified.
        enum AspectAdjust value = 1;
    }

    /// A title alignment value.
    message ValueTitleAlign {
        /// The alignment; it must be specified.
        enum TitleAlign value = 1;
    }

    /// A list of texts value.
    message ValueTexts {
        /// The texts.
        repeated message Text values = 1;
    }

    /// A ticks value.
    message ValueTicks {
        /// The ticks; it must be present.
        message Ticks value = 1;
    }

    /// A tick labels value.
    message ValueTickLabels {
        /// The tick labels; it must be present.
        message TickLabels value = 1;
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

    /// A coordinates value.
    message ValueCoords {
        /// How the values are placed; it must be specified.
        enum Coords value = 1;
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

    /// An image placement value.
    message ValueImagePlacement {
        /// The placement; it must be present.
        message ImagePlacement value = 1;
    }

    /// A pixel range value.
    message ValuePixelRange {
        /// The range; it must be present.
        message PixelRange value = 1;
    }

    /// An image plane value.
    message ValueImagePlane {
        /// The plane; it must be present.
        message ImagePlane value = 1;
    }

    /// An out-of-range policy value.
    message ValueOutOfRange {
        /// The policy; it must be present.
        message OutOfRange value = 1;
    }

    /// An out-of-range category of a colour scale.
    message ValueColorScaleOutOfRange {
        /// The category; it must be present.
        message ColorScaleOutOfRange value = 1;
    }

    /// A colour scale kind value.
    message ValueColorScaleKind {
        /// The kind; it must be present.
        message ColorScaleKind value = 1;
    }

    /// A continuous colour scale value.
    message ValueContinuousColorScale {
        /// The scale; it must be present.
        message ContinuousColorScale value = 1;
    }

    /// A discrete colour scale value.
    message ValueDiscreteColorScale {
        /// The scale; it must be present.
        message DiscreteColorScale value = 1;
    }

    /// An indexed colour scale value.
    message ValueIndexedColorScale {
        /// The scale; it must be present.
        message IndexedColorScale value = 1;
    }

    /// A categorical colour scale value.
    message ValueCategoricalColorScale {
        /// The scale; it must be present.
        message CategoricalColorScale value = 1;
    }

    /// A colormap value.
    message ValueColormap {
        /// The colormap; it must be present.
        message Colormap value = 1;
    }

    /// A colour scale norm value.
    message ValueColorScaleNorm {
        /// The norm; it must be specified.
        enum ColorScaleNorm value = 1;
    }

    /// A colour scale band edges value.
    message ValueColorScaleLevels {
        /// The band edges; it must be present.
        message ColorScaleLevels value = 1;
    }

    /// A panel colourbar value.
    message ValuePanelColorbar {
        /// The colourbar; it must be present.
        message PanelColorbar value = 1;
    }

    /// A colourbar orientation value.
    message ValueColorbarOrientation {
        /// The orientation; it must be specified.
        enum ColorbarOrientation value = 1;
    }

    /// An inside colourbar value.
    message ValueInsideColorbar {
        /// The colourbar; it must be present.
        message InsideColorbar value = 1;
    }

    /// An inside colourbar side value.
    message ValueInsideColorbarSide {
        /// The side; it must be specified.
        enum InsideColorbarSide value = 1;
    }
}
