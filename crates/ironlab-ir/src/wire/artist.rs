//! Wire types of `ironlab/ir/v0/artist.proto`.

proto_file! {
    /// A drawable node of an axes.
    message Artist {
        /// The kind of artist; it must be set.
        oneof kind: ArtistKind {
            /// Isolines or filled isobands of a gridded field.
            Contour(Contour) contour = 3;
            /// Arrows at data points.
            Quiver(Quiver) quiver = 4;
            /// A gridded surface of faces.
            Surface(Surface) surface = 5;
            /// A raster of true-colour pixels.
            Image(Image) image = 6;
            /// A raster of numbers, each mapped to a colour through a colour scale.
            ScalarImage(ScalarImage) scalar_image = 9;
            /// Data points drawn as a line, as markers or as both.
            Series(Series) series = 10;
            /// Rectangles that rise from a base to each value, one per position.
            Bar(Bar) bar = 11;
            /// A stepped outline through values between edges or at samples.
            Stairs(Stairs) stairs = 12;
            /// A grid of rectangular cells, each coloured by its value through a colour
            /// scale.
            Cells(Cells) cells = 13;
        }
        /// The numbers and names of the line and the scatter, which a series replaced.
        reserved 1, 2, line, scatter;
        /// The numbers and names of the colour-indexed and colour-mapped images, which a
        /// scalar image replaced.
        reserved 7, 8, indexed_image, mapped_image;
    }

    /// Data points drawn as a line through them, as markers at them, or as both.
    message Series {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The label drawn beside the artist in a legend; absent when there is none.
        message Text label = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The data identifier of the x coordinates of the points.
        optional uint64 x = 4;
        /// The data identifier of the y coordinates of the points.
        optional uint64 y = 5;
        /// The data identifier of the z coordinates of the points; absent when the
        /// points lie in the plane z = 0.
        optional uint64 z = 6;
        /// The style of the line through the points, whose colour is the colour of the
        /// series.
        message LineStyle line = 7;
        /// The style of the markers at the points.
        message MarkerStyle marker = 8;
        /// The data identifier of the diameter of each marker in points, one per point;
        /// absent when every marker takes the marker size.
        optional uint64 size_data = 9;
        /// The data identifier of the values that colour the series, one per point or
        /// one per segment; absent for a series of fixed colours.
        optional uint64 color_data = 10;
        /// The node identifier of the colour scale through which the colour data is
        /// mapped; absent when the series names none.
        optional uint64 color_scale = 11;
        /// How the x values are placed: as data or as fractions of the plot area.
        enum Coords x_coords = 12;
        /// How the y values are placed: as data or as fractions of the plot area.
        enum Coords y_coords = 13;
        /// Whether the artist is clipped to the plot area.
        optional bool clip = 14;
    }

    /// How the values of one dimension of a series are placed in a 2D axes.
    enum Coords {
        /// The values are data, placed through the limits and scale of the axis.
        Data = 1;
        /// The values are fractions of the plot area along the dimension, 0 at its left
        /// or bottom edge and 1 at its right or top edge.
        AxesFraction = 2;
    }

    /// Isolines or filled isobands of a scalar field sampled on a grid.
    message Contour {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The label drawn beside the artist in a legend; absent when there is none.
        message Text label = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The grid on which the field is sampled.
        message Grid grid = 4;
        /// The data identifier of the field values, of shape `[ny, nx]`.
        optional uint64 z = 5;
        /// The field values at which isolines are drawn or between which bands are
        /// filled; absent for the levels of the colour scale.
        message Levels levels = 6;
        /// Whether the bands between levels are filled rather than only the isolines
        /// drawn.
        optional bool fill = 7;
        /// Where the contours are placed in 3D axes.
        message ContourPlacement placement = 8;
        /// The style of the isolines.
        message LineStyle line = 9;
        /// The opacity of the isolines, from 0 to 1; absent means 1.
        optional double line_opacity = 10;
        /// The opacity of the filled bands, from 0 to 1; absent means 1.
        optional double fill_opacity = 11;
        /// The node identifier of the colour scale through which levels and bands are
        /// coloured; absent when the contour names none.
        optional uint64 color_scale = 12;
    }

    /// The grid on which a field of shape `[ny, nx]` is sampled.
    message Grid {
        /// The kind of grid; unset means the default of the context.
        oneof kind: GridKind {
            /// An axis-aligned grid defined by one coordinate vector per axis.
            Rectilinear(GridRectilinear) rectilinear = 1;
            /// A structured grid whose every node has its own coordinates.
            Curvilinear(GridCurvilinear) curvilinear = 2;
        }
    }

    /// An axis-aligned grid defined by one coordinate vector per axis.
    message GridRectilinear {
        /// The data identifier of the x coordinates of the columns, `nx` values.
        optional uint64 x = 1;
        /// The data identifier of the y coordinates of the rows, `ny` values.
        optional uint64 y = 2;
    }

    /// A structured grid whose every node has its own coordinates.
    message GridCurvilinear {
        /// The data identifier of the x coordinate of every node, of shape `[ny, nx]`.
        optional uint64 x = 1;
        /// The data identifier of the y coordinate of every node, of shape `[ny, nx]`.
        optional uint64 y = 2;
    }

    /// The field values at which contours are drawn.
    message Levels {
        /// The kind of levels; unset means the default of the context.
        oneof kind: LevelsKind {
            /// Levels are chosen at nice values spanning the data range.
            Auto(LevelsAuto) automatic = 1;
            /// Levels are given explicitly.
            Explicit(LevelsExplicit) explicit_values = 2;
        }
    }

    /// Levels are chosen at nice values spanning the data range.
    message LevelsAuto {
        /// The approximate number of levels.
        optional uint32 count = 1;
    }

    /// Levels are given explicitly.
    message LevelsExplicit {
        /// The levels, in ascending order.
        repeated double values = 1;
    }

    /// Where contours are placed in 3D axes.
    message ContourPlacement {
        /// The kind of placement; unset means the default of the context.
        oneof kind: ContourPlacementKind {
            /// All contours lie in one horizontal plane.
            Plane(ContourPlacementPlane) plane = 1;
            /// Each isoline is drawn at the height of its level.
            AtLevel(ContourPlacementAtLevel) at_level = 2;
        }
    }

    /// All contours lie in one horizontal plane.
    message ContourPlacementPlane {
        /// The height of the plane in 3D axes; absent for the bottom of the z axis.
        optional double z = 1;
    }

    /// Each isoline is drawn at the height of its level; valid only in 3D axes.
    message ContourPlacementAtLevel {}

    /// Arrows representing a vector field at data points.
    message Quiver {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The label drawn beside the artist in a legend; absent when there is none.
        message Text label = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The data identifier of the x coordinates of the arrow tails.
        optional uint64 x = 4;
        /// The data identifier of the y coordinates of the arrow tails.
        optional uint64 y = 5;
        /// The data identifier of the z coordinates of the arrow tails; absent when the
        /// tails lie in the plane z = 0.
        optional uint64 z = 6;
        /// The data identifier of the x components of the vectors.
        optional uint64 u = 7;
        /// The data identifier of the y components of the vectors.
        optional uint64 v = 8;
        /// The data identifier of the z components of the vectors; absent when the
        /// vectors lie in horizontal planes.
        optional uint64 w = 9;
        /// How vector lengths are scaled into arrow lengths.
        message QuiverScale scale = 10;
        /// The style of the arrow shafts and heads.
        message LineStyle line = 11;
        /// The length of each arrow head as a fraction of its arrow's length.
        optional double head_size = 12;
    }

    /// How quiver vector lengths are scaled into arrow lengths.
    message QuiverScale {
        /// The kind of scaling; unset means the default of the context.
        oneof kind: QuiverScaleKind {
            /// Arrows are scaled so that the longest does not overlap its neighbours.
            Auto(QuiverScaleAuto) automatic = 1;
            /// The automatic scale is multiplied by a factor.
            Factor(QuiverScaleFactor) factor = 2;
            /// Arrows are drawn with the vectors' lengths in data units.
            Off(QuiverScaleOff) off = 3;
        }
    }

    /// Arrows are scaled so that the longest does not overlap its neighbours.
    message QuiverScaleAuto {}

    /// The automatic scale is multiplied by a factor.
    message QuiverScaleFactor {
        /// The multiplier applied to the automatic scale.
        optional double value = 1;
    }

    /// Arrows are drawn with the vectors' lengths in data units.
    message QuiverScaleOff {}

    /// A surface of quadrilateral faces over a grid, valid in either projection; in a
    /// two-dimensional axes it is seen from directly above.
    message Surface {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The label drawn beside the artist in a legend; absent when there is none.
        message Text label = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The grid on which the surface is sampled.
        message Grid grid = 4;
        /// The data identifier of the field, of shape `[ny, nx]`: the height of every node
        /// in a three-dimensional axes, and the colour data unless `c` is given.
        optional uint64 z = 5;
        /// The data identifier of the colour data of every node; absent when the
        /// surface is coloured by its field.
        optional uint64 c = 6;
        /// The colour of the faces.
        message ColorSpec face = 7;
        /// The colour of the face edges.
        message ColorSpec edge = 8;
        /// The width of the face edges in points.
        optional double edge_width_pt = 9;
        /// The data identifier of the alpha of the faces at every node, of shape
        /// `[ny, nx]`; absent when the faces are uniformly opaque before their opacity.
        optional uint64 face_alpha = 10;
        /// The data identifier of the alpha of the face edges at every node, of shape
        /// `[ny, nx]`; absent when the edges are uniformly opaque before their opacity.
        optional uint64 edge_alpha = 11;
        /// The opacity of the faces, from 0 to 1; absent means 1.
        optional double face_opacity = 12;
        /// The opacity of the face edges, from 0 to 1; absent means 1.
        optional double edge_opacity = 13;
        /// The node identifier of the colour scale through which the colour data is
        /// mapped; absent when the surface names none.
        optional uint64 color_scale = 14;
    }

    /// A true-colour image: a raster of pixels, each with its own colour.
    message Image {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The label drawn beside the artist in a legend; absent when there is none.
        message Text label = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The data identifier of the pixels, of shape `[ny, nx, 3]` (red, green and
        /// blue) or `[ny, nx, 4]` (red, green, blue and alpha), holding floating-point
        /// components from 0 to 1 or 8-bit components from 0 to 255.
        optional uint64 pixels = 4;
        /// Where the pixels lie in the axes.
        message ImagePlacement placement = 5;
        /// The opacity of the image, from 0 to 1, which multiplies the alpha of every
        /// pixel; absent means 1.
        optional double opacity = 6;
    }

    /// A scalar image: a raster of pixels, each holding one number that its colour scale
    /// maps to a colour.
    message ScalarImage {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The label drawn beside the artist in a legend; absent when there is none.
        message Text label = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The data identifier of the numbers, of shape `[ny, nx]`, holding
        /// floating-point or 8-bit values.
        optional uint64 values = 4;
        /// The node identifier of the colour scale through which the numbers are
        /// mapped; absent when the image names none.
        optional uint64 color_scale = 5;
        /// Where the pixels lie in the axes.
        message ImagePlacement placement = 6;
        /// The data identifier of the alpha of every pixel, of shape `[ny, nx]`,
        /// holding floating-point values from 0 to 1 or 8-bit values from 0 to 255;
        /// absent when the pixels are uniformly opaque before the opacity.
        optional uint64 alpha = 7;
        /// The opacity of the image, from 0 to 1, which multiplies the alpha of every
        /// pixel; absent means 1.
        optional double opacity = 8;
    }

    /// Where the pixels of an image lie in its axes: the plane of the image and the
    /// coordinates of its pixel centres along the two axes of that plane.
    message ImagePlacement {
        /// The plane in which the image lies; unset means the xy plane at the bottom
        /// of the z axis.
        message ImagePlane plane = 1;
        /// The centres of the first and last columns along the first axis of the
        /// plane; absent for centres at 0 to nx - 1.
        message PixelRange columns = 2;
        /// The centres of the first and last rows along the second axis of the plane;
        /// absent for centres at 0 to ny - 1.
        message PixelRange rows = 3;
    }

    /// The coordinates of the centres of the first and last pixels of an image along
    /// one axis of its plane; a last centre before the first mirrors the image.
    message PixelRange {
        /// The coordinate of the centre of the first pixel.
        optional double first = 1;
        /// The coordinate of the centre of the last pixel.
        optional double last = 2;
    }

    /// The plane of an axes in which an image lies, with the offset of the plane along
    /// the third axis.
    message ImagePlane {
        /// The plane; unset means the xy plane at the bottom of the z axis.
        oneof kind: ImagePlaneKind {
            /// The plane of the x and y axes.
            Xy(ImagePlaneXy) xy = 1;
            /// The plane of the x and z axes.
            Xz(ImagePlaneXz) xz = 2;
            /// The plane of the y and z axes.
            Yz(ImagePlaneYz) yz = 3;
        }
    }

    /// The plane of the x and y axes: the floor of a three-dimensional axes.
    message ImagePlaneXy {
        /// The height of the plane; absent for the bottom of the z axis. Ignored by
        /// two-dimensional axes.
        optional double z = 1;
    }

    /// The plane of the x and z axes: a wall of a three-dimensional axes.
    message ImagePlaneXz {
        /// The y coordinate of the plane; absent for the low end of the y axis.
        optional double y = 1;
    }

    /// The plane of the y and z axes: the other wall of a three-dimensional axes.
    message ImagePlaneYz {
        /// The x coordinate of the plane; absent for the low end of the x axis.
        optional double x = 1;
    }

    /// The direction in which bars and stairs extend.
    enum Orientation {
        /// Positions and edges lie along x and values along y.
        Vertical = 1;
        /// Positions and edges lie along y and values along x.
        Horizontal = 2;
    }

    /// A series of bars: one rectangle for each position along the category axis, from a
    /// base to the value along the value axis.
    message Bar {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The label drawn beside the artist in a legend; absent when there is none.
        message Text label = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// Whether the bars stand upright or lie flat; unspecified means upright.
        enum Orientation orientation = 4;
        /// The data identifier of the centre of the slot of each bar along the category
        /// axis, one value per bar.
        optional uint64 positions = 5;
        /// The data identifier of the end of each bar along the value axis, one value
        /// per bar.
        optional uint64 values = 6;
        /// Where each bar starts along the value axis; absent means 0.
        message BarBase base = 7;
        /// The extent of the slot of each bar along the category axis; absent means the
        /// pitch of the positions.
        message BarSlot slot = 8;
        /// The fraction of each slot left empty, from 0 up to, but not including, 1;
        /// absent means 0.2.
        optional double gap = 9;
        /// The group of bars of the axes to which the series belongs; absent when the
        /// series is drawn on its own.
        message BarGroup group = 10;
        /// The colour of the bars; absent means the next colour of the colour order.
        message ColorSpec face = 11;
        /// One fixed colour for each bar, which overrides the face; empty when every bar
        /// takes the face.
        repeated message Color colors = 12;
        /// The style of the outline of every bar; absent means no outline.
        message LineStyle edge = 13;
        /// The opacity of the bars, from 0 to 1; absent means 1.
        optional double face_opacity = 14;
    }

    /// Where the bars of a series start along the value axis.
    message BarBase {
        /// The kind of base; unset means the default of the context.
        oneof kind: BarBaseKind {
            /// Every bar starts at the same value.
            Constant(BarBaseConstant) constant = 1;
            /// Each bar starts at its own value.
            Data(BarBaseData) data = 2;
        }
    }

    /// Every bar starts at the same value.
    message BarBaseConstant {
        /// The value at which every bar starts.
        optional double constant = 1;
    }

    /// Each bar starts at its own value.
    message BarBaseData {
        /// The data identifier of the value at which each bar starts, one per bar.
        optional uint64 data = 1;
    }

    /// The extent of the slot of each bar along the category axis.
    message BarSlot {
        /// The kind of slot; unset means the default of the context.
        oneof kind: BarSlotKind {
            /// The smallest distance between neighbouring distinct positions.
            Pitch(BarSlotPitch) pitch = 1;
            /// The same extent for every bar, in data units.
            Width(BarSlotWidth) width = 2;
            /// One extent for each bar, in data units.
            Data(BarSlotData) data = 3;
        }
    }

    /// The smallest distance between neighbouring distinct positions, or 1 for a single
    /// distinct position.
    message BarSlotPitch {}

    /// The same extent for every bar, in data units.
    message BarSlotWidth {
        /// The extent of every slot; finite and positive.
        optional double width = 1;
    }

    /// One extent for each bar, in data units.
    message BarSlotData {
        /// The data identifier of the extent of each slot, one per bar; each finite and
        /// positive.
        optional uint64 data = 1;
    }

    /// The group of bars of an axes to which a bar series belongs.
    message BarGroup {
        /// The name of the group, which the members of one group share.
        string name = 1;
        /// How the members of the group share each position; unspecified means side by
        /// side.
        enum BarLayout layout = 2;
    }

    /// How the members of a group of bars share each position.
    enum BarLayout {
        /// The members stand side by side and touch.
        Grouped = 1;
        /// The members build on one another, positive and negative values separately.
        Stacked = 2;
    }

    /// A stepped outline through values held between edges or at samples.
    message Stairs {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The label drawn beside the artist in a legend; absent when there is none.
        message Text label = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// Whether the steps lie along x or along y; unspecified means along x.
        enum Orientation orientation = 4;
        /// The steps; they must be present.
        message StairsData data = 5;
        /// The value to which the outline drops at both ends; absent for an open
        /// outline.
        optional double base = 6;
        /// The style of the outline; absent means a solid line in the next colour of the
        /// colour order.
        message LineStyle line = 7;
        /// The colour of the area between the outline and the base; absent means none.
        message ColorSpec fill = 8;
        /// The opacity of the fill, from 0 to 1; absent means 1.
        optional double fill_opacity = 9;
    }

    /// The steps of stairs: values between edges, or values at samples.
    message StairsData {
        /// The kind of steps; it must be set.
        oneof kind: StairsDataKind {
            /// Each value holds between two neighbouring edges.
            Edges(StairsDataEdges) edges = 1;
            /// Each value holds at a sample.
            Samples(StairsDataSamples) samples = 2;
        }
    }

    /// Each value holds between two neighbouring edges.
    message StairsDataEdges {
        /// The data identifier of the edges of the steps, one more than the values.
        optional uint64 edges = 1;
        /// The data identifier of the value of each step.
        optional uint64 values = 2;
    }

    /// Each value holds at a sample.
    message StairsDataSamples {
        /// The data identifier of the positions of the samples, one per value.
        optional uint64 x = 1;
        /// The data identifier of the value at each sample.
        optional uint64 values = 2;
        /// Where each step occurs relative to its samples; unspecified means at the later
        /// sample.
        enum StepAt step = 3;
    }

    /// Where the step of stairs through samples occurs relative to its samples.
    enum StepAt {
        /// At the earlier sample.
        Pre = 1;
        /// At the later sample.
        Post = 2;
        /// Midway between the samples.
        Mid = 3;
    }

    /// A grid of rectangular cells, each coloured by its value through a colour scale;
    /// valid only in a two-dimensional axes.
    message Cells {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The label drawn beside the artist in a legend; absent when there is none.
        message Text label = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The data identifier of the values, of shape `[ny, nx]`, holding
        /// floating-point or 8-bit values.
        optional uint64 values = 4;
        /// The edges of the columns along x; absent means unit edges.
        message CellEdges columns = 5;
        /// The edges of the rows along y; absent means unit edges.
        message CellEdges rows = 6;
        /// The fractions of the extent of each cell left empty; absent means none.
        message CellGap gap = 7;
        /// The style of the outline of every cell; absent means no outline.
        message LineStyle edge = 8;
        /// The node identifier of the colour scale through which the values are mapped;
        /// absent when the cells name none.
        optional uint64 color_scale = 9;
        /// The data identifier of the alpha of every cell, of shape `[ny, nx]`, holding
        /// floating-point values from 0 to 1 or 8-bit values from 0 to 255; absent when
        /// the cells are uniformly opaque before the opacity.
        optional uint64 alpha = 10;
        /// The opacity of the cells, from 0 to 1; absent means 1.
        optional double opacity = 11;
    }

    /// The edges of the columns or rows of cells.
    message CellEdges {
        /// The kind of edges; unset means the default of the context.
        oneof kind: CellEdgesKind {
            /// Cell k spans k - 0.5 to k + 0.5.
            Unit(CellEdgesUnit) unit = 1;
            /// The edges given as data.
            Edges(CellEdgesEdges) edges = 2;
        }
    }

    /// Cell k spans k - 0.5 to k + 0.5.
    message CellEdgesUnit {}

    /// The edges given as data.
    message CellEdgesEdges {
        /// The data identifier of the edges, one more than the cells; finite and
        /// strictly monotonic.
        optional uint64 edges = 1;
    }

    /// The fractions of the extent of each cell left empty along x and along y.
    message CellGap {
        /// The fraction of the width of each cell left empty.
        optional double x = 1;
        /// The fraction of the height of each cell left empty.
        optional double y = 2;
    }
}
