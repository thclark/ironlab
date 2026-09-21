//! Wire types of `ironlab/ir/v0/artist.proto`.

proto_file! {
    /// A drawable node of an axes.
    message Artist {
        /// The kind of artist; it must be set.
        oneof kind: ArtistKind {
            /// A polyline with optional markers.
            Line(Line) line = 1;
            /// Markers at data points with per-point size and colour.
            Scatter(Scatter) scatter = 2;
            /// Isolines or filled isobands of a gridded field.
            Contour(Contour) contour = 3;
            /// Arrows at data points.
            Quiver(Quiver) quiver = 4;
            /// A gridded surface of faces.
            Surface(Surface) surface = 5;
            /// A raster of true-colour pixels.
            Image(Image) image = 6;
            /// A raster of pixels that name entries of the axes colormap.
            IndexedImage(IndexedImage) indexed_image = 7;
            /// A raster of data values mapped through the axes colormap and colour
            /// limits.
            MappedImage(MappedImage) mapped_image = 8;
        }
    }

    /// A polyline through data points, with optional markers at the points.
    message Line {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The name shown for the artist in the legend; absent when there is none.
        message Text display_name = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The data identifier of the x coordinates of the points.
        optional uint64 x = 4;
        /// The data identifier of the y coordinates of the points.
        optional uint64 y = 5;
        /// The data identifier of the z coordinates of the points; absent when the
        /// points lie in the plane z = 0.
        optional uint64 z = 6;
        /// The style of the line through the points.
        message LineStyle line = 7;
        /// The style of the markers at the points.
        message MarkerStyle marker = 8;
    }

    /// Markers at data points, each with its own size and colour.
    message Scatter {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The name shown for the artist in the legend; absent when there is none.
        message Text display_name = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The data identifier of the x coordinates of the points.
        optional uint64 x = 4;
        /// The data identifier of the y coordinates of the points.
        optional uint64 y = 5;
        /// The data identifier of the z coordinates of the points; absent when the
        /// points lie in the plane z = 0.
        optional uint64 z = 6;
        /// The size of the markers, which overrides the marker style's size.
        message ScatterSize size = 7;
        /// The colour of the markers, applied wherever the marker face or edge is
        /// automatic.
        message ScatterColor color = 8;
        /// The marker shape and the use of the scatter colour for face and edge.
        message MarkerStyle marker = 9;
    }

    /// The size of scatter markers.
    message ScatterSize {
        /// The kind of size; unset means the default of the context.
        oneof kind: ScatterSizeKind {
            /// Every marker has the same size.
            Scalar(ScatterSizeScalar) scalar = 1;
            /// Each marker has its own size.
            Data(ScatterSizeData) data = 2;
        }
    }

    /// Every marker has the same size.
    message ScatterSizeScalar {
        /// The marker size in points, measured as the width of the marker.
        optional double value = 1;
    }

    /// Each marker has its own size.
    message ScatterSizeData {
        /// The data identifier of the marker sizes in points, one per point.
        optional uint64 data = 1;
    }

    /// The colour of scatter markers.
    message ScatterColor {
        /// The kind of colour; unset means the default of the context.
        oneof kind: ScatterColorKind {
            /// Every marker has the same colour.
            Spec(ScatterColorSpec) spec = 1;
            /// Each marker is coloured by a data value through the axes colormap.
            Data(ScatterColorData) data = 2;
        }
    }

    /// Every marker has the same colour.
    message ScatterColorSpec {
        /// The colour of every marker.
        message ColorSpec spec = 1;
    }

    /// Each marker is coloured by a data value through the axes colormap.
    message ScatterColorData {
        /// The data identifier of the values, one per point, mapped through the axes
        /// colormap and colour limits.
        optional uint64 data = 1;
    }

    /// Isolines or filled isobands of a scalar field sampled on a grid.
    message Contour {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The name shown for the artist in the legend; absent when there is none.
        message Text display_name = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The grid on which the field is sampled.
        message Grid grid = 4;
        /// The data identifier of the field values, of shape `[ny, nx]`.
        optional uint64 z = 5;
        /// The field values at which isolines are drawn or between which bands are
        /// filled.
        message Levels levels = 6;
        /// Whether the bands between levels are filled rather than only the isolines
        /// drawn.
        optional bool fill = 7;
        /// Where the contours are placed in 3D axes.
        message ContourPlacement placement = 8;
        /// The style of the isolines.
        message LineStyle line = 9;
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
        /// The name shown for the artist in the legend; absent when there is none.
        message Text display_name = 2;
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
        /// The name shown for the artist in the legend; absent when there is none.
        message Text display_name = 2;
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
    }

    /// A true-colour image: a raster of pixels, each with its own colour.
    message Image {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The name shown for the artist in the legend; absent when there is none.
        message Text display_name = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The data identifier of the pixels, of shape `[ny, nx, 3]` (red, green and
        /// blue) or `[ny, nx, 4]` (red, green, blue and alpha), holding floating-point
        /// components from 0 to 1 or 8-bit components from 0 to 255.
        optional uint64 pixels = 4;
        /// Where the pixels lie in the axes.
        message ImagePlacement placement = 5;
    }

    /// A colour-indexed image: a raster of pixels whose values name entries of the axes
    /// colormap directly, an index being truncated toward zero and taking the entry
    /// from 0 to 255 of that number.
    message IndexedImage {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The name shown for the artist in the legend; absent when there is none.
        message Text display_name = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The data identifier of the indices, of shape `[ny, nx]`, holding
        /// floating-point or 8-bit values.
        optional uint64 indices = 4;
        /// Where the pixels lie in the axes.
        message ImagePlacement placement = 5;
        /// What is drawn for a pixel whose truncated index is less than 0.
        message OutOfRange below = 6;
        /// What is drawn for a pixel whose truncated index is greater than 255.
        message OutOfRange above = 7;
        /// What is drawn for a pixel whose index is not finite.
        message OutOfRange non_finite = 8;
    }

    /// A colour-mapped image: a raster of data values, each scaled through the colour
    /// limits of the axes into its colormap.
    message MappedImage {
        /// The node identifier of the artist, unique within the figure.
        optional uint64 id = 1;
        /// The name shown for the artist in the legend; absent when there is none.
        message Text display_name = 2;
        /// Whether the artist is drawn.
        optional bool visible = 3;
        /// The data identifier of the values, of shape `[ny, nx]`, holding
        /// floating-point or 8-bit values.
        optional uint64 values = 4;
        /// Where the pixels lie in the axes.
        message ImagePlacement placement = 5;
        /// What is drawn for a pixel whose value is less than the lower colour limit.
        message OutOfRange below = 6;
        /// What is drawn for a pixel whose value is greater than the upper colour
        /// limit.
        message OutOfRange above = 7;
        /// What is drawn for a pixel whose value is not finite.
        message OutOfRange non_finite = 8;
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

    /// What is drawn for a pixel of a colour-indexed or colour-mapped image that the
    /// artist cannot colour.
    message OutOfRange {
        /// The policy; unset means the default of the context, which is transparent.
        oneof kind: OutOfRangeKind {
            /// Such a pixel makes the figure invalid.
            Strict(OutOfRangeStrict) strict = 1;
            /// Nothing is drawn for the pixel.
            Transparent(OutOfRangeTransparent) transparent = 2;
            /// The pixel takes the nearest end colour of the colormap.
            Clamp(OutOfRangeClamp) clamp = 3;
            /// The pixel is drawn in a fixed colour.
            Rgba(OutOfRangeRgba) rgba = 4;
        }
    }

    /// Such a pixel makes the figure invalid, so it is refused until the data is
    /// corrected.
    message OutOfRangeStrict {}

    /// Nothing is drawn for the pixel.
    message OutOfRangeTransparent {}

    /// The pixel takes the nearest end colour of the colormap: its first entry below the
    /// range and its last entry above it; a non-finite value has no nearest end, so
    /// nothing is drawn for it.
    message OutOfRangeClamp {}

    /// The pixel is drawn in a fixed colour.
    message OutOfRangeRgba {
        /// The colour of the pixel; absent means black.
        message Color color = 1;
    }
}
