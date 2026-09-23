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
    Artist, Contour, ContourPlacement, Grid, Image, ImagePlacement, ImagePlane, IndexedImage,
    Levels, Line, MappedImage, OutOfRange, PixelRange, Quiver, QuiverScale, Scatter, ScatterColor,
    ScatterSize, Surface,
};
use crate::axes::{
    Axes, Axis, Cell, ColormapName, Legend, LegendLocation, Limits, Projection, Scale, View3d,
};
use crate::edit::path::PropertyPath;
use crate::edit::registry::{NodeKind, Property};
use crate::edit::value::{Value, ValueType};
use crate::figure::{Figure, FigureSize, FontSetId, Parameter, TileLayout};
use crate::ids::{DataId, NodeId};
use crate::link::AxisLink;
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
        let _ = $value;
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
        let _ = $place;
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
macro_rules! ir_tagged {
    ($(
        $ty:ident as $variant:ident {
            $( $case:ident {
                $( $rule:tt $name:literal => $field:ident : $fty:ty = $docs:literal; )*
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
                    $($( ($ty::$case { $field, .. }, $name) => walk_get!($rule, $field, rest), )*)*
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
                    $($( ($ty::$case { $field, .. }, $name) => walk_set!($rule, $field, rest, value), )*)*
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
    DataId => DataId;
    Vec<f64> => Doubles;
    Vec<String> => Strings;
    Vec<AxisLink> => Links;
    BTreeMap<String, Parameter> => Parameters;
    Interpreter => Interpreter;
    FontSetId => FontSetId;
    Scale => Scale;
    ColormapName => ColormapName;
    LegendLocation => LegendLocation;
    DashStyle => DashStyle;
    MarkerShape => MarkerShape;
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
        plain "layout" => layout: TileLayout = "The grid of cells in which axes are placed.";
        hidden "data" => data: () = "";
        hidden "axes" => axes: () = "";
        plain "links" => links: Vec<AxisLink> =
            "The groups of axes whose limits are linked along a dimension.";
        hidden "provenance" => provenance: () = "";
        plain "parameters" => parameters: BTreeMap<String, Parameter> =
            "Named values that describe the figure, used to sort, filter and search collections of figures.";
        plain "labels" => labels: Vec<String> =
            "Free words that describe the figure, used to filter and group collections of figures.";
    }

    Axes {
        hidden "id" => id: () = "";
        plain "cell" => cell: Cell = "The cells of the figure's tile layout that the axes occupies.";
        plain "projection" => projection: Projection =
            "Whether the axes is two- or three-dimensional, with its three-dimensional view.";
        optional "title" => title: Text = "The title drawn above the axes.";
        plain "x" => x: Axis =
            "The horizontal axis in two dimensions, or the first horizontal axis in three.";
        plain "y" => y: Axis =
            "The vertical axis in two dimensions, or the second horizontal axis in three.";
        plain "z" => z: Axis =
            "The vertical axis in three dimensions; ignored by two-dimensional axes.";
        plain "box" => box_: bool =
            "Whether the full outline of the plot box is drawn, rather than only the edges that carry tick labels.";
        plain "colormap" => colormap: ColormapName =
            "The colormap used by colormapped artists in this axes.";
        plain "clim" => clim: Limits =
            "The data values mapped to the first and last colours of the colormap.";
        optional "legend" => legend: Legend = "The legend, or absent when no legend is shown.";
        hidden "artists" => artists: () = "";
    }

    Line {
        hidden "id" => id: () = "";
        optional "display_name" => display_name: Text =
            "The name shown for the artist in the legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "x" => x: DataId = "The x coordinates of the points.";
        plain "y" => y: DataId = "The y coordinates of the points.";
        optional "z" => z: DataId =
            "The z coordinates of the points, allowed only in three-dimensional axes; when absent in three-dimensional axes the points lie in the plane z = 0.";
        plain "line" => line: LineStyle = "The style of the line through the points.";
        plain "marker" => marker: MarkerStyle = "The style of the markers at the points.";
    }

    Scatter {
        hidden "id" => id: () = "";
        optional "display_name" => display_name: Text =
            "The name shown for the artist in the legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "x" => x: DataId = "The x coordinates of the points.";
        plain "y" => y: DataId = "The y coordinates of the points.";
        optional "z" => z: DataId =
            "The z coordinates of the points, allowed only in three-dimensional axes; when absent in three-dimensional axes the points lie in the plane z = 0.";
        plain "size" => size: ScatterSize =
            "The size of the markers, which overrides the size of the marker style.";
        plain "color" => color: ScatterColor =
            "The colour of the markers, applied wherever the marker face or edge is automatic.";
        plain "marker" => marker: MarkerStyle =
            "The marker shape and the use of the scatter colour for face and edge.";
    }

    Contour {
        hidden "id" => id: () = "";
        optional "display_name" => display_name: Text =
            "The name shown for the artist in the legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "grid" => grid: Grid = "The grid on which the field is sampled.";
        plain "z" => z: DataId =
            "The field values, a two-dimensional array of shape [ny, nx].";
        plain "levels" => levels: Levels =
            "The field values at which isolines are drawn or between which bands are filled.";
        plain "fill" => fill: bool =
            "Whether the bands between levels are filled rather than only the isolines drawn.";
        plain "placement" => placement: ContourPlacement =
            "Where the contours are placed in three-dimensional axes.";
        plain "line" => line: LineStyle =
            "The style of the isolines; a colormapped colour takes each isoline's level.";
    }

    Quiver {
        hidden "id" => id: () = "";
        optional "display_name" => display_name: Text =
            "The name shown for the artist in the legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "x" => x: DataId = "The x coordinates of the arrow tails.";
        plain "y" => y: DataId = "The y coordinates of the arrow tails.";
        optional "z" => z: DataId =
            "The z coordinates of the arrow tails, allowed only in three-dimensional axes; when absent in three-dimensional axes the tails lie in the plane z = 0.";
        plain "u" => u: DataId = "The x components of the vectors.";
        plain "v" => v: DataId = "The y components of the vectors.";
        optional "w" => w: DataId =
            "The z components of the vectors, allowed only in three-dimensional axes; when absent in three-dimensional axes the vectors lie in horizontal planes.";
        plain "scale" => scale: QuiverScale =
            "How vector lengths are scaled into arrow lengths.";
        plain "line" => line: LineStyle = "The style of the arrow shafts and heads.";
        plain "head_size" => head_size: f64 =
            "The length of each arrow head as a fraction of its arrow's length.";
    }

    Surface {
        hidden "id" => id: () = "";
        optional "display_name" => display_name: Text =
            "The name shown for the artist in the legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "grid" => grid: Grid = "The grid on which the surface is sampled.";
        plain "z" => z: DataId =
            "The field, a two-dimensional array of shape [ny, nx]: the height of every node in a three-dimensional axes, and the colour data of every node unless c is given.";
        optional "c" => c: DataId =
            "The colour data of every node, with the same shape as the field; when absent the surface is coloured by its field.";
        plain "face" => face: ColorSpec = "The colour of the faces.";
        plain "edge" => edge: ColorSpec = "The colour of the face edges.";
        plain "edge_width_pt" => edge_width_pt: f64 = "The width of the face edges in points.";
    }

    Image {
        hidden "id" => id: () = "";
        optional "display_name" => display_name: Text =
            "The name shown for the artist in the legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "pixels" => pixels: DataId =
            "The pixels, a three-dimensional array of shape [ny, nx, 3] or [ny, nx, 4] holding the red, green, blue and optionally alpha components of every pixel: floating-point components from 0 to 1, or 8-bit components from 0 to 255.";
        plain "placement" => placement: ImagePlacement = "Where the pixels lie in the axes.";
    }

    IndexedImage {
        hidden "id" => id: () = "";
        optional "display_name" => display_name: Text =
            "The name shown for the artist in the legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "indices" => indices: DataId =
            "The indices into the axes colormap, a two-dimensional array of shape [ny, nx]: a floating-point index is truncated toward zero, and an index from 0 to 255 takes that entry of the colormap.";
        plain "placement" => placement: ImagePlacement = "Where the pixels lie in the axes.";
        plain "below" => below: OutOfRange =
            "What is drawn for a pixel whose truncated index is less than 0.";
        plain "above" => above: OutOfRange =
            "What is drawn for a pixel whose truncated index is greater than 255.";
        plain "non_finite" => non_finite: OutOfRange =
            "What is drawn for a pixel whose index is not finite.";
    }

    MappedImage {
        hidden "id" => id: () = "";
        optional "display_name" => display_name: Text =
            "The name shown for the artist in the legend.";
        plain "visible" => visible: bool = "Whether the artist is drawn.";
        plain "values" => values: DataId =
            "The values, a two-dimensional array of shape [ny, nx], mapped through the axes colormap and colour limits.";
        plain "placement" => placement: ImagePlacement = "Where the pixels lie in the axes.";
        plain "below" => below: OutOfRange =
            "What is drawn for a pixel whose value is less than the lower colour limit.";
        plain "above" => above: OutOfRange =
            "What is drawn for a pixel whose value is greater than the upper colour limit.";
        plain "non_finite" => non_finite: OutOfRange =
            "What is drawn for a pixel whose value is not finite.";
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
        plain "grid" => grid: bool =
            "Whether grid lines are drawn at the major ticks of this axis.";
    }

    Legend as Legend {
        plain "location" => location: LegendLocation =
            "Where the legend is placed inside the plot area.";
        plain "boxed" => boxed: bool =
            "Whether the legend is drawn with a background and outline.";
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

    Limits as Limits {
        Auto {}
        Manual {
            plain "min" => min: f64 = "The lower bound of the range.";
            plain "max" => max: f64 = "The upper bound of the range.";
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
            plain "x" => x: DataId = "The x coordinates of the grid.";
            plain "y" => y: DataId = "The y coordinates of the grid.";
        }
        Curvilinear {
            plain "x" => x: DataId = "The x coordinates of the grid.";
            plain "y" => y: DataId = "The y coordinates of the grid.";
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

    ScatterSize as ScatterSize {
        Scalar {
            plain "value" => value: f64 =
                "The marker size in points, measured as the width of the marker.";
        }
        Data {
            plain "data" => data: DataId =
                "An array of marker sizes in points, one per point.";
        }
    }

    ScatterColor as ScatterColor {
        Spec {
            plain "spec" => spec: ColorSpec = "The colour of every marker.";
        }
        Data {
            plain "data" => data: DataId =
                "An array of values, one per point, mapped through the axes colormap and colour limits.";
        }
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
            plain "color" => color: Color = "The colour the pixels are drawn in.";
        }
    }
}

// ---------------------------------------------------------------------------------
// Dispatch over the nodes of a figure
// ---------------------------------------------------------------------------------

/// Returns every settable property of a kind of node, in declaration order, without the
/// paths that several variants of a tagged value share more than once.
pub(crate) fn node_properties(kind: NodeKind) -> Vec<Property> {
    let mut properties = Vec::new();
    match kind {
        NodeKind::Figure => Figure::node_describe(&mut properties),
        NodeKind::Axes => Axes::node_describe(&mut properties),
        NodeKind::Line => Line::node_describe(&mut properties),
        NodeKind::Scatter => Scatter::node_describe(&mut properties),
        NodeKind::Contour => Contour::node_describe(&mut properties),
        NodeKind::Quiver => Quiver::node_describe(&mut properties),
        NodeKind::Surface => Surface::node_describe(&mut properties),
        NodeKind::Image => Image::node_describe(&mut properties),
        NodeKind::IndexedImage => IndexedImage::node_describe(&mut properties),
        NodeKind::MappedImage => MappedImage::node_describe(&mut properties),
    }
    let mut seen = std::collections::BTreeSet::new();
    properties.retain(|property| seen.insert(property.path.clone()));
    properties
}

/// Returns the kind of a node of a figure, or `None` when the identifier is not a node
/// of the figure.
pub(crate) fn kind_of(figure: &Figure, node: NodeId) -> Option<NodeKind> {
    if figure.id == node {
        return Some(NodeKind::Figure);
    }
    if figure.axes(node).is_some() {
        return Some(NodeKind::Axes);
    }
    figure.artist(node).map(|(_, artist)| match artist {
        Artist::Line(_) => NodeKind::Line,
        Artist::Scatter(_) => NodeKind::Scatter,
        Artist::Contour(_) => NodeKind::Contour,
        Artist::Quiver(_) => NodeKind::Quiver,
        Artist::Surface(_) => NodeKind::Surface,
        Artist::Image(_) => NodeKind::Image,
        Artist::IndexedImage(_) => NodeKind::IndexedImage,
        Artist::MappedImage(_) => NodeKind::MappedImage,
    })
}

/// Reads the property at a path within a node, or `None` when the identifier is not a
/// node of the figure.
pub(crate) fn get_in(
    figure: &Figure,
    node: NodeId,
    path: &[String],
) -> Option<Result<Value, Step>> {
    if figure.id == node {
        return Some(figure.node_get(path));
    }
    if let Some(axes) = figure.axes(node) {
        return Some(axes.node_get(path));
    }
    Some(match figure.artist(node)?.1 {
        Artist::Line(a) => a.node_get(path),
        Artist::Scatter(a) => a.node_get(path),
        Artist::Contour(a) => a.node_get(path),
        Artist::Quiver(a) => a.node_get(path),
        Artist::Surface(a) => a.node_get(path),
        Artist::Image(a) => a.node_get(path),
        Artist::IndexedImage(a) => a.node_get(path),
        Artist::MappedImage(a) => a.node_get(path),
    })
}

/// Writes the property at a path within a node, or returns `None` when the identifier is
/// not a node of the figure.
pub(crate) fn set_in(
    figure: &mut Figure,
    node: NodeId,
    path: &[String],
    value: Value,
) -> Option<Result<(), Step>> {
    if figure.id == node {
        return Some(figure.node_set(path, value));
    }
    if figure.axes(node).is_some() {
        let axes = figure.axes_mut(node).expect("the axes was just found");
        return Some(axes.node_set(path, value));
    }
    Some(match figure.artist_mut(node)? {
        Artist::Line(a) => a.node_set(path, value),
        Artist::Scatter(a) => a.node_set(path, value),
        Artist::Contour(a) => a.node_set(path, value),
        Artist::Quiver(a) => a.node_set(path, value),
        Artist::Surface(a) => a.node_set(path, value),
        Artist::Image(a) => a.node_set(path, value),
        Artist::IndexedImage(a) => a.node_set(path, value),
        Artist::MappedImage(a) => a.node_set(path, value),
    })
}
