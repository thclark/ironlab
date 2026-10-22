//! The single description of the IR's settable properties, and the walk that reads and
//! writes them.
//!
//! Every settable property of every kind of node is declared exactly once, in the
//! `ir_node!`, `ir_value!` and `ir_tagged!` invocations at the end of this file. Each
//! declaration names the field as the Protocol Buffers schema names it, gives its Rust
//! field and type, says whether it may be absent, and documents it. From that one
//! declaration the macros generate the entry in the registry that [`properties`] returns,
//! the read performed by [`Figure::get`] and the write performed by
//! [`Edit::Set`](crate::Edit::Set), so a reader and a writer cannot disagree about which
//! field a path names, and a field added to the IR without a declaration here is missing
//! from all three at once.
//!
//! [`properties`]: crate::properties
//! [`Figure::get`]: crate::Figure::get

use std::collections::BTreeMap;

use crate::artist::{
    Contour, ContourPlacement, Coords, Grid, Image, ImagePlacement, ImagePlane, Levels, PixelRange,
    Quiver, QuiverScale, ScalarImage, Series, Surface,
};
use crate::axes::{
    Aspect, AspectAdjust, Axes, Axis, ColormapName, InsideColorbar, InsideColorbarSide, Limits,
    Projection, Scale, TickLabels, Ticks, TitleAlign, View3d,
};
use crate::color_scale::{
    CategoricalColorScale, ColorScale, ColorScaleKind, ColorScaleLevels, ColorScaleNorm,
    ColorScaleOutOfRange, Colormap, ContinuousColorScale, DiscreteColorScale, IndexedColorScale,
    OutOfRange,
};
use crate::data::DataArray;
use crate::edit::path::PropertyPath;
use crate::edit::registry::Property;
use crate::edit::value::{Value, ValueType};
use crate::figure::{Figure, FigureSize, FontSetId, Parameter};
use crate::ids::NodeId;
use crate::layout::{Cell, TileLayout, TrackSize};
use crate::legend::{
    InsideLegend, Legend, LegendEntryColumns, LegendEntryLayout, LegendEntryOrder, LegendLocation,
    PanelLegend,
};
use crate::link::AxisLink;
use crate::nodes::{NodeKind, describe};
use crate::panel::{ColorbarOrientation, Panel, PanelColorbar, PanelContent, PanelLocation};
use crate::style::{Color, ColorSpec, DashStyle, LineStyle, MarkerShape, MarkerStyle};
use crate::text::{Interpreter, Text};

/// Why a step of a path walk failed, before it is given the node and the path that make
/// it an [`EditError`](crate::EditError).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Step {
    /// The path names a field that the value does not have.
    Unknown,
    /// The path descends into a variant that is not the one currently set.
    Inactive,
    /// The path descends through an optional value that is absent.
    Absent,
    /// The path names a property that edits cannot set.
    ReadOnly,
    /// The value is not of the type of the property.
    Type {
        /// The type of the property.
        expected: ValueType,
        /// The type of the value supplied, or `None` for [`Value::Unset`].
        found: Option<ValueType>,
    },
}

/// A value of the IR that a property path can name or descend into.
pub(crate) trait Walk: Clone {
    /// The type of the [`Value`] that this value becomes.
    const TYPE: ValueType;

    /// Returns the value itself for an empty path, or the property at the path within
    /// it.
    fn get(&self, path: &[String]) -> Result<Value, Step>;

    /// Replaces the value itself for an empty path, or the property at the path within
    /// it.
    fn set(&mut self, path: &[String], value: Value) -> Result<(), Step>;

    /// Appends every property below this value to `out`, with the segments of `prefix`
    /// before each path, leaving `prefix` as it found it.
    fn describe(prefix: &mut Vec<&'static str>, conditional: bool, out: &mut Vec<Property>);
}

/// A node of the figure tree, whose fields a property path starts at.
pub(crate) trait WalkNode {
    /// Returns the property at the path within the node.
    fn node_get(&self, path: &[String]) -> Result<Value, Step>;

    /// Replaces the property at the path within the node.
    fn node_set(&mut self, path: &[String], value: Value) -> Result<(), Step>;

    /// Appends every settable property of the node to `out`.
    fn node_describe(out: &mut Vec<Property>);
}

/// Clones a value without naming its type, so that cloning a value that happens to be
/// `Copy` reads the same for every kind of property.
fn cloned<T: Clone>(value: &T) -> T {
    value.clone()
}

// ---------------------------------------------------------------------------------
// The per-field rules, shared by the three macros below
// ---------------------------------------------------------------------------------

/// Reads a field according to its rule: `plain` descends into it, `optional` reads an
/// absent value as [`Value::Unset`] and refuses to descend through it, and `hidden`
/// reports the field, and everything below it, as read-only.
///
/// A hidden field is named only so that the declaration is checked against the
/// structure; its type is never used, and is written `()`.
macro_rules! walk_get {
    (hidden, $value:expr, $rest:expr) => {{
        let _ = ($value, $rest);
        Err(Step::ReadOnly)
    }};
    (optional, $value:expr, $rest:expr) => {
        match $value {
            Some(inner) => Walk::get(inner, $rest),
            None if $rest.is_empty() => Ok(Value::Unset),
            None => Err(Step::Absent),
        }
    };
    (plain, $value:expr, $rest:expr) => {
        Walk::get($value, $rest)
    };
}

/// Writes a field according to its rule. An optional field is cleared by
/// [`Value::Unset`] and created from its default by a value of its own type, so that
/// the type of the value is checked in exactly one place.
macro_rules! walk_set {
    (hidden, $place:expr, $rest:expr, $value:expr) => {{
        let _ = ($place, $rest);
        Err(Step::ReadOnly)
    }};
    (optional, $place:expr, $rest:expr, $value:expr) => {{
        let place = $place;
        let value = $value;
        if $rest.is_empty() && value == Value::Unset {
            *place = None;
            Ok(())
        } else {
            match place {
                Some(inner) => Walk::set(inner, $rest, value),
                None if $rest.is_empty() => {
                    let mut fresh = Default::default();
                    Walk::set(&mut fresh, $rest, value)?;
                    *place = Some(fresh);
                    Ok(())
                }
                None => Err(Step::Absent),
            }
        }
    }};
    (plain, $place:expr, $rest:expr, $value:expr) => {
        Walk::set($place, $rest, $value)
    };
}

/// Returns whether a field's rule makes it optional.
macro_rules! walk_optional {
    (optional) => {
        true
    };
    ($rule:tt) => {
        false
    };
}

/// Appends a field and the properties below it to the registry; a hidden field is not
/// settable and is not listed.
macro_rules! walk_describe {
    (hidden, $ty:ty, $name:literal, $docs:literal, $prefix:expr, $conditional:expr, $out:expr) => {};
    ($rule:tt, $ty:ty, $name:literal, $docs:literal, $prefix:expr, $conditional:expr, $out:expr) => {{
        $prefix.push($name);
        $out.push(Property {
            path: PropertyPath::of($prefix),
            value_type: <$ty as Walk>::TYPE,
            optional: walk_optional!($rule),
            conditional: $conditional,
            docs: $docs,
        });
        <$ty as Walk>::describe($prefix, $conditional || walk_optional!($rule), $out);
        $prefix.pop();
    }};
}

// ---------------------------------------------------------------------------------
// The three kinds of declaration
// ---------------------------------------------------------------------------------

/// Declares a value of the IR that has no fields of its own: a number, a string, a
/// reference, a list, a map or a simple enumeration.
macro_rules! ir_leaf {
    ($($ty:ty => $variant:ident;)*) => {$(
        impl Walk for $ty {
            const TYPE: ValueType = ValueType::$variant;

            fn get(&self, path: &[String]) -> Result<Value, Step> {
                if path.is_empty() {
                    Ok(Value::$variant(cloned(self)))
                } else {
                    Err(Step::Unknown)
                }
            }

            fn set(&mut self, path: &[String], value: Value) -> Result<(), Step> {
                if !path.is_empty() {
                    return Err(Step::Unknown);
                }
                match value {
                    Value::$variant(new) => {
                        *self = new;
                        Ok(())
                    }
                    other => Err(Step::Type {
                        expected: ValueType::$variant,
                        found: other.value_type(),
                    }),
                }
            }

            fn describe(_prefix: &mut Vec<&'static str>, _conditional: bool, _out: &mut Vec<Property>) {}
        }
    )*};
}

/// Declares a value of the IR that is a structure of fields.
macro_rules! ir_value {
    ($(
        $ty:ident as $variant:ident {
            $( $rule:tt $name:literal => $field:ident : $fty:ty = $docs:literal; )*
        }
    )*) => {$(
        impl Walk for $ty {
            const TYPE: ValueType = ValueType::$variant;

            fn get(&self, path: &[String]) -> Result<Value, Step> {
                let Some((head, rest)) = path.split_first() else {
                    return Ok(Value::$variant(cloned(self)));
                };
                match head.as_str() {
                    $( $name => walk_get!($rule, &self.$field, rest), )*
                    _ => Err(Step::Unknown),
                }
            }

            fn set(&mut self, path: &[String], value: Value) -> Result<(), Step> {
                let Some((head, rest)) = path.split_first() else {
                    return match value {
                        Value::$variant(new) => {
                            *self = new;
                            Ok(())
                        }
                        other => Err(Step::Type {
                            expected: ValueType::$variant,
                            found: other.value_type(),
                        }),
                    };
                };
                match head.as_str() {
                    $( $name => walk_set!($rule, &mut self.$field, rest, value), )*
                    _ => Err(Step::Unknown),
                }
            }

            fn describe(prefix: &mut Vec<&'static str>, conditional: bool, out: &mut Vec<Property>) {
                $( walk_describe!($rule, $fty, $name, $docs, prefix, conditional, out); )*
            }
        }
    )*};
}

/// Declares a tagged value of the IR: an enumeration whose variants carry fields, which
/// a path descends into by naming a field of the variant that is currently set.
///
/// A field of a variant is named by its identifier, or by its position (`0`) for a
/// variant that holds a single unnamed value, which the declaration gives a name.
macro_rules! ir_tagged {
    ($(
        $ty:ident as $variant:ident {
            $( $case:ident {
                $( $rule:tt $name:literal => $field:tt : $fty:ty = $docs:literal; )*
            } )*
        }
    )*) => {$(
        impl Walk for $ty {
            const TYPE: ValueType = ValueType::$variant;

            fn get(&self, path: &[String]) -> Result<Value, Step> {
                let Some((head, rest)) = path.split_first() else {
                    return Ok(Value::$variant(cloned(self)));
                };
                match (self, head.as_str()) {
                    $($( ($ty::$case { $field: bound, .. }, $name) => walk_get!($rule, bound, rest), )*)*
                    (_, other) => Err(inactive_or_unknown(other, &[$($($name,)*)*])),
                }
            }

            fn set(&mut self, path: &[String], value: Value) -> Result<(), Step> {
                let Some((head, rest)) = path.split_first() else {
                    return match value {
                        Value::$variant(new) => {
                            *self = new;
                            Ok(())
                        }
                        other => Err(Step::Type {
                            expected: ValueType::$variant,
                            found: other.value_type(),
                        }),
                    };
                };
                match (self, head.as_str()) {
                    $($( ($ty::$case { $field: bound, .. }, $name) => walk_set!($rule, bound, rest, value), )*)*
                    (_, other) => Err(inactive_or_unknown(other, &[$($($name,)*)*])),
                }
            }

            fn describe(prefix: &mut Vec<&'static str>, _conditional: bool, out: &mut Vec<Property>) {
                // Every field of a variant is reachable only while that variant is set.
                $($( walk_describe!($rule, $fty, $name, $docs, prefix, true, out); )*)*
            }
        }
    )*};
}

/// Declares a node of the figure tree, whose fields a property path starts at.
macro_rules! ir_node {
    ($(
        $ty:ident {
            $( $rule:tt $name:literal => $field:ident : $fty:ty = $docs:literal; )*
        }
    )*) => {$(
        impl WalkNode for $ty {
            fn node_get(&self, path: &[String]) -> Result<Value, Step> {
                let Some((head, rest)) = path.split_first() else {
                    return Err(Step::Unknown);
                };
                match head.as_str() {
                    $( $name => walk_get!($rule, &self.$field, rest), )*
                    _ => Err(Step::Unknown),
                }
            }

            fn node_set(&mut self, path: &[String], value: Value) -> Result<(), Step> {
                let Some((head, rest)) = path.split_first() else {
                    return Err(Step::Unknown);
                };
                match head.as_str() {
                    $( $name => walk_set!($rule, &mut self.$field, rest, value), )*
                    _ => Err(Step::Unknown),
                }
            }

            fn node_describe(out: &mut Vec<Property>) {
                let prefix = &mut Vec::new();
                $( walk_describe!($rule, $fty, $name, $docs, prefix, false, out); )*
            }
        }
    )*};
}

/// Returns the failure of a path that no variant of a tagged value has, or that belongs
/// to a variant other than the one currently set.
fn inactive_or_unknown(head: &str, fields: &[&str]) -> Step {
    if fields.contains(&head) {
        Step::Inactive
    } else {
        Step::Unknown
    }
}

// ---------------------------------------------------------------------------------
// The properties of the IR
// ---------------------------------------------------------------------------------

ir_leaf! {
    bool => Bool;
    u32 => UInt32;
    f64 => Double;
    f32 => Float;
    String => String;
    NodeId => NodeId;
    Vec<f64> => Doubles;
    Vec<TrackSize> => TrackSizes;
    Vec<String> => Strings;
    Vec<Text> => Texts;
    Vec<AxisLink> => Links;
    BTreeMap<String, Parameter> => Parameters;
    Interpreter => Interpreter;
    FontSetId => FontSetId;
    Scale => Scale;
    ColormapName => ColormapName;
    ColorScaleNorm => ColorScaleNorm;
    LegendLocation => LegendLocation;
    PanelLocation => PanelLocation;
    ColorbarOrientation => ColorbarOrientation;
    InsideColorbarSide => InsideColorbarSide;
    LegendEntryOrder => LegendEntryOrder;
    AspectAdjust => AspectAdjust;
    TitleAlign => TitleAlign;
    DashStyle => DashStyle;
    MarkerShape => MarkerShape;
    Coords => Coords;
}

ir_node! {
    Figure {
        hidden "schema_version" => schema_version: () = "";
        hidden "id" => id: () = "";
        optional "title" => title: Text = "The title drawn above all axes.";
        plain "size" => size: FigureSize = "The physical size of the figure.";
        plain "font_set" => font_set: FontSetId = "The font set used for all text.";
        plain "font_size_pt" => font_size_pt: f64 =
            "The base font size in points; titles and tick labels are scaled from it.";
        plain "background" => background: Color = "The colour of the figure background.";
        plain "layout" => layout: TileLayout =
            "The grid of cells in which axes and panels are placed.";
        hidden "data" => data: () = "";
        hidden "color_scales" => color_scales: () = "";
        hidden "axes" => axes: () = "";
        hidden "panels" => panels: () = "";
        plain "links" => links: Vec<AxisLink> =
            "The groups of axes whose limits are linked along a dimension.";
        hidden "provenance" => provenance: () = "";
        plain "parameters" => parameters: BTreeMap<String, Parameter> =
            "Named values that describe the figure, used to sort, filter and search collections of figures.";
        plain "tags" => tags: Vec<String> =
            "Free words that describe the figure, used to filter and group collections of figures; never drawn.";
    }

    DataArray {
        hidden "id" => id: () = "";
        optional "name" => name: String =
            "The name that identifies the array to tools such as the inspector; never drawn.";
        hidden "array" => array: () = "";
    }

    ColorScale {
        hidden "id" => id: () = "";
        optional "title" => title: Text =
            "The title drawn with the scale: above a colourbar that shows it, and as the label of a bar item that shows it in a legend.";
        plain "kind" => kind: ColorScaleKind =
            "How values become colours and how the scale is drawn: continuous, discrete, indexed or categorical.";
        plain "non_finite" => non_finite: ColorScaleOutOfRange =
            "What is drawn for a value that is NaN or infinite, on every kind of scale.";
    }

    Axes {
        hidden "id" => id: () = "";
        plain "cell" => cell: Cell = "The cells of the figure's tile layout that the axes occupies.";
        plain "projection" => projection: Projection =
            "Whether the axes is two- or three-dimensional, with its three-dimensional view.";
        optional "title" => title: Text = "The title drawn above the axes.";
        plain "title_align" => title_align: TitleAlign =
            "How the title is aligned with the plot area: centred over it, or with its left or right edge on the left or right edge of the plot area.";
        plain "x" => x: Axis =
            "The horizontal axis in two dimensions, or the first horizontal axis in three.";
        plain "y" => y: Axis =
            "The vertical axis in two dimensions, or the second horizontal axis in three.";
        plain "z" => z: Axis =
            "The vertical axis in three dimensions; ignored by two-dimensional axes.";
        plain "aspect" => aspect: Aspect =
            "The ratios of the length of one data unit of y, and of z, to the length of one data unit of x; on a logarithmic axis the unit is one decade.";
        plain "aspect_adjust" => aspect_adjust: AspectAdjust =
            "How a fixed aspect is kept: by shaping the plot area or the box of a three-dimensional axes to the limits, or by widening the limits.";
        optional "colorbar" => colorbar: InsideColorbar =
            "The colourbar drawn inside the plot area along one of its edges, over the artists, or absent when the axes shows none.";
        plain "box" => box_: bool =
            "Whether the full outline of the plot box is drawn, rather than only the edges that carry tick labels.";
        optional "legend" => legend: Legend =
            "Where the legend entries of the axes are listed, or absent when they are not listed.";
        hidden "artists" => artists: () = "";
    }

    Panel {
        hidden "id" => id: () = "";
        plain "cell" => cell: Cell = "The cells of the figure's tile layout that the panel occupies.";
        plain "align" => align: PanelLocation = "Where the content is placed within the panel.";
        plain "content" => content: PanelContent = "What the panel holds.";
    }

    Series {
        hidden "id" => id: () = "";
        optional "label" => label: Text = "The label drawn beside the artist in a legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "x" => x: NodeId = "The x coordinates of the points.";
        plain "y" => y: NodeId = "The y coordinates of the points.";
        optional "z" => z: NodeId =
            "The z coordinates of the points, allowed only in three-dimensional axes; when absent in three-dimensional axes the points lie in the plane z = 0.";
        plain "line" => line: LineStyle =
            "The style of the line through the points, whose colour is the colour of the series; a dash or a colour of none draws no line.";
        plain "marker" => marker: MarkerStyle =
            "The style of the markers at the points; a shape of none draws no markers.";
        optional "size_data" => size_data: NodeId =
            "The diameter of each marker in points, one value per point; when absent every marker takes the marker size, which is always the size of the legend key.";
        optional "color_data" => color_data: NodeId =
            "The values that colour the series through its colour scale: one per point, which colours the markers and draws each segment as a gradient, or one per segment, which colours each segment of the line.";
        optional "color_scale" => color_scale: NodeId =
            "The colour scale through which the colour data is mapped; a series with colour data must name one.";
        plain "x_coords" => x_coords: Coords =
            "How the x values are placed: as data along the x axis, or as fractions of the plot area's width.";
        plain "y_coords" => y_coords: Coords =
            "How the y values are placed: as data along the y axis, or as fractions of the plot area's height.";
        plain "clip" => clip: bool =
            "Whether the artist is clipped to the plot area; when it is not, marks may extend beyond the plot area along a dimension in axes-fraction coordinates, but never along a dimension in data coordinates.";
    }

    Contour {
        hidden "id" => id: () = "";
        optional "label" => label: Text = "The label drawn beside the artist in a legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "grid" => grid: Grid = "The grid on which the field is sampled.";
        plain "z" => z: NodeId =
            "The field values, a two-dimensional array of shape [ny, nx].";
        optional "levels" => levels: Levels =
            "The field values at which isolines are drawn or between which bands are filled, or absent for the levels of the colour scale: the band edges of a discrete scale, and ten automatic levels otherwise.";
        plain "fill" => fill: bool =
            "Whether the bands between levels are filled rather than only the isolines drawn.";
        plain "placement" => placement: ContourPlacement =
            "Where the contours are placed in three-dimensional axes.";
        plain "line" => line: LineStyle =
            "The style of the isolines; a colormapped colour takes the colour of each isoline's level through the colour scale.";
        optional "color_scale" => color_scale: NodeId =
            "The colour scale through which levels and bands are coloured; a contour whose bands are filled or whose isolines are colormapped must name one.";
        plain "line_opacity" => line_opacity: f64 =
            "The opacity of the isolines, from 0 (transparent) to 1 (opaque), which multiplies the alpha of their colour.";
        plain "fill_opacity" => fill_opacity: f64 =
            "The opacity of the filled bands, from 0 (transparent) to 1 (opaque), which multiplies the alpha of their colour.";
    }

    Quiver {
        hidden "id" => id: () = "";
        optional "label" => label: Text = "The label drawn beside the artist in a legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "x" => x: NodeId = "The x coordinates of the arrow tails.";
        plain "y" => y: NodeId = "The y coordinates of the arrow tails.";
        optional "z" => z: NodeId =
            "The z coordinates of the arrow tails, allowed only in three-dimensional axes; when absent in three-dimensional axes the tails lie in the plane z = 0.";
        plain "u" => u: NodeId = "The x components of the vectors.";
        plain "v" => v: NodeId = "The y components of the vectors.";
        optional "w" => w: NodeId =
            "The z components of the vectors, allowed only in three-dimensional axes; when absent in three-dimensional axes the vectors lie in horizontal planes.";
        plain "scale" => scale: QuiverScale =
            "How vector lengths are scaled into arrow lengths.";
        plain "line" => line: LineStyle = "The style of the arrow shafts and heads.";
        plain "head_size" => head_size: f64 =
            "The length of each arrow head as a fraction of its arrow's length.";
    }

    Surface {
        hidden "id" => id: () = "";
        optional "label" => label: Text = "The label drawn beside the artist in a legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "grid" => grid: Grid = "The grid on which the surface is sampled.";
        plain "z" => z: NodeId =
            "The field, a two-dimensional array of shape [ny, nx]: the height of every node in a three-dimensional axes, and the colour data of every node unless c is given.";
        optional "c" => c: NodeId =
            "The colour data of every node, with the same shape as the field; when absent the surface is coloured by its field.";
        plain "face" => face: ColorSpec = "The colour of the faces.";
        plain "edge" => edge: ColorSpec = "The colour of the face edges.";
        plain "edge_width_pt" => edge_width_pt: f64 = "The width of the face edges in points.";
        optional "color_scale" => color_scale: NodeId =
            "The colour scale through which the colour data is mapped; a surface whose faces or edges are colormapped must name one.";
        optional "face_alpha" => face_alpha: NodeId =
            "The alpha of the faces at every node, with the same shape as the field; a face takes the mean of its corners' alpha, which is clamped to [0, 1], and a non-finite alpha makes the face transparent. When absent the faces are uniformly opaque before their opacity applies.";
        optional "edge_alpha" => edge_alpha: NodeId =
            "The alpha of the face edges at every node, with the same shape as the field; the edges of a face take the mean of its corners' edge alpha. When absent the edges are uniformly opaque before their opacity applies.";
        plain "face_opacity" => face_opacity: f64 =
            "The opacity of the faces, from 0 (transparent) to 1 (opaque), which multiplies the face colour's alpha and the face alpha.";
        plain "edge_opacity" => edge_opacity: f64 =
            "The opacity of the face edges, from 0 (transparent) to 1 (opaque), which multiplies the edge colour's alpha and the edge alpha.";
    }

    Image {
        hidden "id" => id: () = "";
        optional "label" => label: Text = "The label drawn beside the artist in a legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "pixels" => pixels: NodeId =
            "The pixels, a three-dimensional array of shape [ny, nx, 3] or [ny, nx, 4] holding the red, green, blue and optionally alpha components of every pixel: floating-point components from 0 to 1, or 8-bit components from 0 to 255.";
        plain "placement" => placement: ImagePlacement = "Where the pixels lie in the axes.";
        plain "opacity" => opacity: f64 =
            "The opacity of the image, from 0 (transparent) to 1 (opaque), which multiplies the alpha of every pixel.";
    }

    ScalarImage {
        hidden "id" => id: () = "";
        optional "label" => label: Text = "The label drawn beside the artist in a legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "values" => values: NodeId =
            "The numbers, a two-dimensional array of shape [ny, nx] of floating-point or 8-bit values, each mapped to a colour through the colour scale.";
        optional "color_scale" => color_scale: NodeId =
            "The colour scale through which the numbers are mapped; a scalar image must name one.";
        plain "placement" => placement: ImagePlacement = "Where the pixels lie in the axes.";
        optional "alpha" => alpha: NodeId =
            "The alpha of every pixel, with the same shape as the values: floating-point values from 0 to 1 or 8-bit values from 0 to 255, clamped to that range, a non-finite alpha making the pixel transparent. When absent the pixels are uniformly opaque before the opacity applies.";
        plain "opacity" => opacity: f64 =
            "The opacity of the image, from 0 (transparent) to 1 (opaque), which multiplies the alpha of every pixel.";
    }
}

ir_value! {
    Text as Text {
        plain "content" => content: String =
            "The source text. With the LaTeX interpreter, segments delimited by $…$ are typeset as mathematics and the remainder as plain text.";
        plain "interpreter" => interpreter: Interpreter = "How the source text is interpreted.";
    }

    FigureSize as FigureSize {
        plain "width_mm" => width_mm: f64 = "The width in millimetres.";
        plain "height_mm" => height_mm: f64 = "The height in millimetres.";
    }

    TileLayout as TileLayout {
        plain "rows" => rows: u32 = "The number of rows; at least one.";
        plain "cols" => cols: u32 = "The number of columns; at least one.";
        plain "row_weights" => row_weights: Vec<TrackSize> =
            "The sizes of the rows, from the top: a relative weight, or fitted to the panels in the row; a row without a size has a weight of one.";
        plain "col_weights" => col_weights: Vec<TrackSize> =
            "The sizes of the columns, from the left: a relative weight, or fitted to the panels in the column; a column without a size has a weight of one.";
    }

    Color as Color {
        plain "r" => r: f32 = "The red component, from 0 to 1.";
        plain "g" => g: f32 = "The green component, from 0 to 1.";
        plain "b" => b: f32 = "The blue component, from 0 to 1.";
        plain "a" => a: f32 = "The opacity, from 0 (transparent) to 1 (opaque).";
    }

    Cell as Cell {
        plain "row" => row: u32 = "The zero-based index of the top row occupied.";
        plain "col" => col: u32 = "The zero-based index of the leftmost column occupied.";
        plain "row_span" => row_span: u32 = "The number of rows occupied; at least one.";
        plain "col_span" => col_span: u32 = "The number of columns occupied; at least one.";
    }

    View3d as View3d {
        plain "azimuth_deg" => azimuth_deg: f64 =
            "The rotation about the vertical axis in degrees, measured counterclockwise from the negative y axis when viewed from above.";
        plain "elevation_deg" => elevation_deg: f64 =
            "The angle of the view direction above the x-y plane in degrees, from -90 to 90.";
        plain "zoom" => zoom: f64 =
            "The magnification of the projected box, where 1 fits the box to the plot area.";
        plain "pan_x" => pan_x: f64 =
            "The horizontal offset of the projected box as a fraction of the plot area's width, increasing to the right.";
        plain "pan_y" => pan_y: f64 =
            "The vertical offset of the projected box as a fraction of the plot area's height, increasing downwards.";
    }

    Axis as Axis {
        optional "label" => label: Text = "The axis label.";
        plain "scale" => scale: Scale =
            "The mapping from data values to positions along the axis.";
        plain "limits" => limits: Limits = "The range of data values shown along the axis.";
        plain "reverse" => reverse: bool =
            "Whether the axis runs from its upper limit to its lower limit: from left to right for a horizontal axis, and from bottom to top for a vertical axis.";
        plain "ticks" => ticks: Ticks = "Where the major ticks of the axis are placed.";
        plain "tick_labels" => tick_labels: TickLabels =
            "What is written at the major ticks of the axis.";
        plain "grid" => grid: bool =
            "Whether grid lines are drawn at the major ticks of this axis.";
    }

    InsideLegend as InsideLegend {
        plain "location" => location: LegendLocation =
            "Where the legend is placed inside the plot area.";
        plain "boxed" => boxed: bool =
            "Whether the legend is drawn with a background and outline.";
        plain "layout" => layout: LegendEntryLayout = "How the entries are arranged in columns.";
        hidden "entries" => entries: () = "";
    }

    PanelColorbar as PanelColorbar {
        plain "scale" => scale: NodeId = "The colour scale that the colourbar shows.";
        plain "orientation" => orientation: ColorbarOrientation =
            "Whether the bar stands upright, with low values at the bottom, or lies flat, with low values at the left.";
    }

    InsideColorbar as InsideColorbar {
        plain "scale" => scale: NodeId = "The colour scale that the colourbar shows.";
        plain "side" => side: InsideColorbarSide =
            "The edge of the plot area along which the bar lies, which also sets whether it lies flat or stands upright.";
        plain "length" => length: f64 =
            "The length of the bar, with any keys it shows, as a fraction of the edge it lies along, centred on it; greater than 0 and at most 1.";
    }

    PanelLegend as PanelLegend {
        plain "boxed" => boxed: bool =
            "Whether the area of the panel is outlined by a border drawn as the box of an axes is drawn.";
        plain "layout" => layout: LegendEntryLayout = "How the entries are arranged in columns.";
        hidden "entries" => entries: () = "";
    }

    LegendEntryLayout as LegendEntryLayout {
        plain "columns" => columns: LegendEntryColumns = "The number of columns.";
        plain "order" => order: LegendEntryOrder =
            "The order in which the entries fill the columns.";
    }

    LineStyle as LineStyle {
        plain "color" => color: ColorSpec = "The line colour.";
        plain "width_pt" => width_pt: f64 = "The line width in points.";
        plain "dash" => dash: DashStyle = "The dash pattern of the line.";
    }

    MarkerStyle as MarkerStyle {
        plain "shape" => shape: MarkerShape = "The marker shape.";
        plain "size_pt" => size_pt: f64 =
            "The marker size in points, measured as the width of the marker.";
        plain "face" => face: ColorSpec = "The colour of the marker interior.";
        plain "edge" => edge: ColorSpec = "The colour of the marker outline.";
    }

    ImagePlacement as ImagePlacement {
        plain "plane" => plane: ImagePlane =
            "The plane of the axes in which the image lies, with its offset along the third axis.";
        optional "columns" => columns: PixelRange =
            "The coordinates of the centres of the first and last columns along the first axis of the plane, or absent for centres at 0 to nx − 1.";
        optional "rows" => rows: PixelRange =
            "The coordinates of the centres of the first and last rows along the second axis of the plane, or absent for centres at 0 to ny − 1.";
    }

    ColorScaleOutOfRange as ColorScaleOutOfRange {
        plain "policy" => policy: OutOfRange = "What is drawn for a value of the category.";
        plain "show_key" => show_key: bool =
            "Whether a colourbar that shows the scale draws a key square for the category, separate from the bar.";
        optional "label" => label: Text =
            "The label of the key square, or absent for the default label.";
    }

    ContinuousColorScale as ContinuousColorScale {
        plain "colormap" => colormap: Colormap = "The palette that the scale interpolates.";
        plain "limits" => limits: Limits =
            "The values mapped to the first and last colours of the colormap; automatic limits are the range of the values that every artist maps through the scale.";
        plain "norm" => norm: ColorScaleNorm =
            "Whether values are interpolated in linear or logarithmic space.";
        plain "below" => below: ColorScaleOutOfRange =
            "What is drawn for a value below the lower limit, or not positive under a logarithmic norm.";
        plain "above" => above: ColorScaleOutOfRange =
            "What is drawn for a value above the upper limit.";
        plain "ticks" => ticks: Ticks =
            "Where the ticks of a colourbar that shows the scale are placed.";
        plain "tick_labels" => tick_labels: TickLabels =
            "What is written at the ticks of a colourbar that shows the scale.";
    }

    DiscreteColorScale as DiscreteColorScale {
        plain "colormap" => colormap: Colormap =
            "The palette from which the bands take their colours.";
        plain "levels" => levels: ColorScaleLevels = "The edges of the bands.";
        plain "norm" => norm: ColorScaleNorm =
            "Whether band edges are chosen, and the colormap sampled, in linear or logarithmic space.";
        plain "below" => below: ColorScaleOutOfRange =
            "What is drawn for a value below the first edge, or not positive under a logarithmic norm.";
        plain "above" => above: ColorScaleOutOfRange =
            "What is drawn for a value above the last edge.";
        plain "ticks" => ticks: Ticks =
            "Where the ticks of a colourbar that shows the scale are placed; automatic ticks are one per band edge.";
        plain "tick_labels" => tick_labels: TickLabels =
            "What is written at the ticks of a colourbar that shows the scale.";
    }

    IndexedColorScale as IndexedColorScale {
        hidden "colors" => colors: () = "";
        plain "outside" => outside: ColorScaleOutOfRange =
            "What is drawn for a value that names no entry.";
        plain "ticks" => ticks: Ticks =
            "Where the ticks of a colourbar that shows the scale are placed, at the centres of the entries.";
        plain "tick_labels" => tick_labels: TickLabels =
            "What is written at the ticks of a colourbar that shows the scale.";
    }

    CategoricalColorScale as CategoricalColorScale {
        hidden "categories" => categories: () = "";
        plain "outside" => outside: ColorScaleOutOfRange =
            "What is drawn for a value that names no category.";
    }

    PixelRange as PixelRange {
        plain "first" => first: f64 = "The coordinate of the centre of the first pixel.";
        plain "last" => last: f64 =
            "The coordinate of the centre of the last pixel; a last centre before the first mirrors the image.";
    }
}

ir_tagged! {
    Projection as Projection {
        TwoD {}
        ThreeD {
            plain "view3d" => view3d: View3d = "The camera view.";
        }
    }

    Legend as Legend {
        Inside {
            plain "inside" => 0: InsideLegend =
                "The legend drawn inside the plot area of the axes.";
        }
        Panel {
            plain "panel" => 0: NodeId =
                "The panel of the figure, holding a legend, in which the entries are listed.";
        }
    }

    PanelContent as PanelContent {
        Legend {
            plain "legend" => 0: PanelLegend =
                "A legend, which lists the entries of every axes whose legend names the panel.";
        }
        Colorbar {
            plain "colorbar" => 0: PanelColorbar =
                "A colourbar, which shows a colour scale along the cells of the panel.";
        }
    }

    LegendEntryColumns as LegendEntryColumns {
        Auto {}
        Count {
            plain "count" => 0: u32 = "The number of columns; at least one.";
        }
    }

    Limits as Limits {
        Auto {}
        Manual {
            plain "min" => min: f64 = "The lower bound of the range.";
            plain "max" => max: f64 = "The upper bound of the range.";
        }
    }

    Aspect as Aspect {
        Auto {}
        Equal {}
        Ratio {
            plain "y" => y: f64 =
                "How many times as long one data unit of y is drawn as one data unit of x; finite and positive.";
            plain "z" => z: f64 =
                "How many times as long one data unit of z is drawn as one data unit of x, in three-dimensional axes; finite and positive.";
        }
    }

    Ticks as Ticks {
        Auto {}
        Step {
            plain "step" => step: f64 =
                "The distance between neighbouring ticks, in data units; finite and positive.";
            plain "origin" => origin: f64 =
                "A value at which a tick is placed if it lies within the limits; finite.";
        }
        Values {
            plain "values" => 0: Vec<f64> =
                "The tick values; each is finite, and positive on a logarithmic axis.";
        }
    }

    TickLabels as TickLabels {
        Auto {}
        Hidden {}
        Text {
            plain "labels" => 0: Vec<Text> =
                "One label for each explicit tick value, in the order of the values.";
        }
    }

    ColorSpec as ColorSpec {
        Auto {}
        Rgba {
            plain "color" => color: Color = "The colour to use.";
        }
        None {}
        Colormapped {}
    }

    Grid as Grid {
        Rectilinear {
            plain "x" => x: NodeId = "The x coordinates of the grid.";
            plain "y" => y: NodeId = "The y coordinates of the grid.";
        }
        Curvilinear {
            plain "x" => x: NodeId = "The x coordinates of the grid.";
            plain "y" => y: NodeId = "The y coordinates of the grid.";
        }
    }

    Levels as Levels {
        Auto {
            plain "count" => count: u32 = "The approximate number of levels.";
        }
        Explicit {
            plain "values" => values: Vec<f64> = "The levels, in ascending order.";
        }
    }

    ContourPlacement as ContourPlacement {
        Plane {
            optional "z" => z: f64 =
                "The height of the plane in three-dimensional axes, or absent for the bottom of the z axis; ignored by two-dimensional axes.";
        }
        AtLevel {}
    }

    QuiverScale as QuiverScale {
        Auto {}
        Factor {
            plain "value" => value: f64 = "The multiplier applied to the automatic scale.";
        }
        Off {}
    }

    ImagePlane as ImagePlane {
        Xy {
            optional "z" => z: f64 =
                "The height of the plane in three-dimensional axes, or absent for the bottom of the z axis; ignored by two-dimensional axes.";
        }
        Xz {
            optional "y" => y: f64 =
                "The y coordinate of the plane, or absent for the low end of the y axis.";
        }
        Yz {
            optional "x" => x: f64 =
                "The x coordinate of the plane, or absent for the low end of the x axis.";
        }
    }

    OutOfRange as OutOfRange {
        Strict {}
        Transparent {}
        Clamp {}
        Rgba {
            plain "color" => color: Color = "The colour in which the values are drawn.";
        }
    }

    ColorScaleKind as ColorScaleKind {
        Continuous {
            plain "continuous" => 0: ContinuousColorScale =
                "A colormap interpolated between limits, drawn as a smooth bar.";
        }
        Discrete {
            plain "discrete" => 0: DiscreteColorScale =
                "Bands between edges, each in one colour of a colormap, drawn as a bar of bands.";
        }
        Indexed {
            plain "indexed" => 0: IndexedColorScale =
                "A palette of which a whole number i takes entry i, drawn as a bar of adjoining blocks.";
        }
        Categorical {
            plain "categorical" => 0: CategoricalColorScale =
                "Labelled categories of which a whole number i takes category i, drawn as separate outlined squares.";
        }
    }

    Colormap as Colormap {
        Named {
            plain "name" => name: ColormapName = "The name of the bundled colormap.";
            plain "reverse" => reverse: bool =
                "Whether the colormap runs from its last colour to its first.";
        }
        Listed {
            hidden "colors" => colors: () = "";
        }
    }

    ColorScaleLevels as ColorScaleLevels {
        Auto {
            plain "count" => count: u32 = "The approximate number of bands; at least one.";
            plain "limits" => limits: Limits =
                "The range that the edges span, from the first edge to the last.";
        }
        Explicit {
            plain "values" => values: Vec<f64> =
                "The edges: at least two, finite, strictly increasing, and positive under a logarithmic norm.";
        }
    }
}

// ---------------------------------------------------------------------------------
// The registry of a kind of node
// ---------------------------------------------------------------------------------

/// Returns every settable property of a kind of node, in declaration order, without the
/// paths that several variants of a tagged value share more than once.
pub(crate) fn node_properties(kind: NodeKind) -> Vec<Property> {
    let mut properties = Vec::new();
    describe(kind, &mut properties);
    let mut seen = std::collections::BTreeSet::new();
    properties.retain(|property| seen.insert(property.path.clone()));
    properties
}
