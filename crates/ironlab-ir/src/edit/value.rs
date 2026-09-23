//! Typed values of properties.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artist::{
    ContourPlacement, Grid, ImagePlacement, ImagePlane, Levels, OutOfRange, PixelRange,
    QuiverScale, ScatterColor, ScatterSize,
};
use crate::axes::{
    Axis, Cell, ColormapName, Legend, LegendLocation, Limits, Projection, Scale, View3d,
};
use crate::figure::{FigureSize, FontSetId, Parameter, TileLayout};
use crate::ids::DataId;
use crate::link::AxisLink;
use crate::style::{Color, ColorSpec, DashStyle, LineStyle, MarkerShape, MarkerStyle};
use crate::text::{Interpreter, Text};

/// The value of a property, as set by [`Edit::Set`](crate::Edit::Set) and returned by
/// [`Figure::get`](crate::Figure::get).
///
/// There is one variant for each type of value that a property path can address, and
/// the variant [`Value::Unset`], which clears an optional property. The type of a value
/// is checked against its path when an edit is applied, so a value of the wrong type is
/// rejected then rather than when the value is constructed.
///
/// Reading a property converts the IR value into a variant of this type, so a property
/// of a type that has no variant cannot be read, and a new value type in the IR must
/// gain a variant here before its properties can be edited.
///
/// In JSON a value is an object whose `type` names the variant in `snake_case` and whose
/// `value` holds the value in its usual JSON form, such as
/// `{"type": "limits", "value": {"type": "manual", "min": 0, "max": 1}}`; an unset value
/// is `{"type": "unset"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    /// No value: an optional property is cleared.
    Unset,
    /// A boolean, such as the visibility of an artist.
    Bool(bool),
    /// An unsigned 32-bit integer, such as a number of rows of the tile layout.
    #[serde(rename = "uint32")]
    UInt32(u32),
    /// A double-precision number, such as a line width or a limit.
    Double(f64),
    /// A single-precision number: a component of a colour.
    Float(f32),
    /// A string: the source of a text.
    String(String),
    /// A reference to a data array, such as the x data of a line.
    DataId(DataId),
    /// A list of double-precision numbers: explicit contour levels.
    Doubles(Vec<f64>),
    /// A list of strings: the labels of a figure.
    Strings(Vec<String>),
    /// A text, such as a title.
    Text(Text),
    /// How the source of a text is interpreted.
    Interpreter(Interpreter),
    /// The physical size of a figure.
    FigureSize(FigureSize),
    /// A bundled font set.
    FontSetId(FontSetId),
    /// A colour.
    Color(Color),
    /// The tile layout of a figure.
    TileLayout(TileLayout),
    /// The axis links of a figure.
    Links(Vec<AxisLink>),
    /// The named parameters of a figure.
    Parameters(BTreeMap<String, Parameter>),
    /// The cells occupied by an axes.
    Cell(Cell),
    /// The projection of an axes.
    Projection(Projection),
    /// The camera view of a three-dimensional axes.
    View3d(View3d),
    /// A coordinate axis of an axes.
    Axis(Axis),
    /// The scale of a coordinate axis.
    Scale(Scale),
    /// A range of data values.
    Limits(Limits),
    /// A colormap name.
    ColormapName(ColormapName),
    /// A legend.
    Legend(Legend),
    /// The placement of a legend.
    LegendLocation(LegendLocation),
    /// How a colour is chosen.
    ColorSpec(ColorSpec),
    /// The style of a stroked line.
    LineStyle(LineStyle),
    /// A dash pattern.
    DashStyle(DashStyle),
    /// The style of markers.
    MarkerStyle(MarkerStyle),
    /// A marker shape.
    MarkerShape(MarkerShape),
    /// The size of scatter markers.
    ScatterSize(ScatterSize),
    /// The colour of scatter markers.
    ScatterColor(ScatterColor),
    /// The grid of a contour or surface.
    Grid(Grid),
    /// The levels of a contour.
    Levels(Levels),
    /// The placement of a contour in three-dimensional axes.
    ContourPlacement(ContourPlacement),
    /// The scaling of quiver arrows.
    QuiverScale(QuiverScale),
    /// The placement of an image in its axes.
    ImagePlacement(ImagePlacement),
    /// The centres of the first and last pixels of an image along an axis.
    PixelRange(PixelRange),
    /// The plane of an image.
    ImagePlane(ImagePlane),
    /// The policy of an image for pixels it cannot colour.
    OutOfRange(OutOfRange),
}

/// The type of a [`Value`] other than [`Value::Unset`], with one variant for each
/// variant of [`Value`] of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ValueType {
    /// See [`Value::Bool`].
    Bool,
    /// See [`Value::UInt32`].
    UInt32,
    /// See [`Value::Double`].
    Double,
    /// See [`Value::Float`].
    Float,
    /// See [`Value::String`].
    String,
    /// See [`Value::DataId`].
    DataId,
    /// See [`Value::Doubles`].
    Doubles,
    /// See [`Value::Strings`].
    Strings,
    /// See [`Value::Text`].
    Text,
    /// See [`Value::Interpreter`].
    Interpreter,
    /// See [`Value::FigureSize`].
    FigureSize,
    /// See [`Value::FontSetId`].
    FontSetId,
    /// See [`Value::Color`].
    Color,
    /// See [`Value::TileLayout`].
    TileLayout,
    /// See [`Value::Links`].
    Links,
    /// See [`Value::Parameters`].
    Parameters,
    /// See [`Value::Cell`].
    Cell,
    /// See [`Value::Projection`].
    Projection,
    /// See [`Value::View3d`].
    View3d,
    /// See [`Value::Axis`].
    Axis,
    /// See [`Value::Scale`].
    Scale,
    /// See [`Value::Limits`].
    Limits,
    /// See [`Value::ColormapName`].
    ColormapName,
    /// See [`Value::Legend`].
    Legend,
    /// See [`Value::LegendLocation`].
    LegendLocation,
    /// See [`Value::ColorSpec`].
    ColorSpec,
    /// See [`Value::LineStyle`].
    LineStyle,
    /// See [`Value::DashStyle`].
    DashStyle,
    /// See [`Value::MarkerStyle`].
    MarkerStyle,
    /// See [`Value::MarkerShape`].
    MarkerShape,
    /// See [`Value::ScatterSize`].
    ScatterSize,
    /// See [`Value::ScatterColor`].
    ScatterColor,
    /// See [`Value::Grid`].
    Grid,
    /// See [`Value::Levels`].
    Levels,
    /// See [`Value::ContourPlacement`].
    ContourPlacement,
    /// See [`Value::QuiverScale`].
    QuiverScale,
    /// See [`Value::ImagePlacement`].
    ImagePlacement,
    /// See [`Value::PixelRange`].
    PixelRange,
    /// See [`Value::ImagePlane`].
    ImagePlane,
    /// See [`Value::OutOfRange`].
    OutOfRange,
}

impl Value {
    /// Returns the type of the value, or `None` for [`Value::Unset`].
    ///
    /// The match is exhaustive, so a variant added to [`Value`] does not compile until
    /// it is given a type of the same name in [`ValueType`].
    pub fn value_type(&self) -> Option<ValueType> {
        Some(match self {
            Value::Unset => return None,
            Value::Bool(_) => ValueType::Bool,
            Value::UInt32(_) => ValueType::UInt32,
            Value::Double(_) => ValueType::Double,
            Value::Float(_) => ValueType::Float,
            Value::String(_) => ValueType::String,
            Value::DataId(_) => ValueType::DataId,
            Value::Doubles(_) => ValueType::Doubles,
            Value::Strings(_) => ValueType::Strings,
            Value::Text(_) => ValueType::Text,
            Value::Interpreter(_) => ValueType::Interpreter,
            Value::FigureSize(_) => ValueType::FigureSize,
            Value::FontSetId(_) => ValueType::FontSetId,
            Value::Color(_) => ValueType::Color,
            Value::TileLayout(_) => ValueType::TileLayout,
            Value::Links(_) => ValueType::Links,
            Value::Parameters(_) => ValueType::Parameters,
            Value::Cell(_) => ValueType::Cell,
            Value::Projection(_) => ValueType::Projection,
            Value::View3d(_) => ValueType::View3d,
            Value::Axis(_) => ValueType::Axis,
            Value::Scale(_) => ValueType::Scale,
            Value::Limits(_) => ValueType::Limits,
            Value::ColormapName(_) => ValueType::ColormapName,
            Value::Legend(_) => ValueType::Legend,
            Value::LegendLocation(_) => ValueType::LegendLocation,
            Value::ColorSpec(_) => ValueType::ColorSpec,
            Value::LineStyle(_) => ValueType::LineStyle,
            Value::DashStyle(_) => ValueType::DashStyle,
            Value::MarkerStyle(_) => ValueType::MarkerStyle,
            Value::MarkerShape(_) => ValueType::MarkerShape,
            Value::ScatterSize(_) => ValueType::ScatterSize,
            Value::ScatterColor(_) => ValueType::ScatterColor,
            Value::Grid(_) => ValueType::Grid,
            Value::Levels(_) => ValueType::Levels,
            Value::ContourPlacement(_) => ValueType::ContourPlacement,
            Value::QuiverScale(_) => ValueType::QuiverScale,
            Value::ImagePlacement(_) => ValueType::ImagePlacement,
            Value::PixelRange(_) => ValueType::PixelRange,
            Value::ImagePlane(_) => ValueType::ImagePlane,
            Value::OutOfRange(_) => ValueType::OutOfRange,
        })
    }
}
