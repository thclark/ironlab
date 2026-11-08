//! Typed values of properties.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artist::{
    BarOffset, BarWidth, BinRule, Binning, Binning2d, Bins, CellEdges, CellGap, ContourPlacement,
    Coords, Grid, HistNorm, ImagePlacement, ImagePlane, Levels, Orientation, PerBar, PixelRange,
    QuiverScale, StairsData, StepAt,
};
use crate::axes::{
    AngleUnits, Aspect, AspectAdjust, Axis, AxisSide, ColourmapName, InsideColourbar,
    InsideColourbarSide, Limits, PolarAngle, PolarView, Projection, Scale, SphericalView,
    TickLabels, Ticks, TitleAlign, View3d,
};
use crate::colour_scale::{
    CategoricalColourScale, ColourScaleKind, ColourScaleLevels, ColourScaleNorm,
    ColourScaleOutOfRange, Colourmap, ContinuousColourScale, DiscreteColourScale,
    IndexedColourScale, OutOfRange,
};
use crate::figure::{Embolden, FigureSize, FontSetId, Parameter};
use crate::ids::NodeId;
use crate::layout::{Cell, TileLayout, TrackSize};
use crate::legend::{
    InsideLegend, Legend, LegendEntryColumns, LegendEntryLayout, LegendEntryOrder, LegendLocation,
    PanelLegend,
};
use crate::link::AxisLink;
use crate::panel::{
    ColourbarOrientation, ColourbarTitlePlacement, PanelColourbar, PanelContent, PanelLocation,
};
use crate::style::{Colour, ColourSpec, DashStyle, LineStyle, MarkerShape, MarkerStyle};
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
#[schemars(deny_unknown_fields)]
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
    /// A reference to a node, such as the data array that holds the x data of a line or
    /// the panel named by the legend of an axes.
    NodeId(NodeId),
    /// A list of double-precision numbers, such as explicit contour levels.
    Doubles(Vec<f64>),
    /// A list of strings: the tags of a figure.
    Strings(Vec<String>),
    /// A text, such as a title.
    Text(Text),
    /// A list of texts: the explicit labels of the ticks of an axis.
    Texts(Vec<Text>),
    /// How the source of a text is interpreted.
    Interpreter(Interpreter),
    /// The physical size of a figure.
    FigureSize(FigureSize),
    /// A bundled font set.
    FontSetId(FontSetId),
    /// How much heavier than designed a figure is drawn.
    Embolden(Embolden),
    /// A colour.
    Colour(Colour),
    /// The tile layout of a figure.
    TileLayout(TileLayout),
    /// The axis links of a figure.
    Links(Vec<AxisLink>),
    /// The named parameters of a figure.
    Parameters(BTreeMap<String, Parameter>),
    /// The cells occupied by an axes or a panel.
    Cell(Cell),
    /// The projection of an axes.
    Projection(Projection),
    /// The camera view of a three-dimensional axes.
    View3d(View3d),
    /// The polar view of a polar or cylindrical axes.
    PolarView(PolarView),
    /// The spherical view of a spherical axes.
    SphericalView(SphericalView),
    /// The data units of an angle.
    AngleUnits(AngleUnits),
    /// How the polar angle of a spherical axes is measured.
    PolarAngle(PolarAngle),
    /// The side of the plot on which an axis is drawn.
    AxisSide(AxisSide),
    /// A coordinate axis of an axes.
    Axis(Axis),
    /// The scale of a coordinate axis.
    Scale(Scale),
    /// A range of data values.
    Limits(Limits),
    /// Where the major ticks of an axis are placed.
    Ticks(Ticks),
    /// What is written at the major ticks of an axis.
    TickLabels(TickLabels),
    /// The ratio of the length of a data unit of y to that of x in a 2D axes.
    Aspect(Aspect),
    /// How a 2D axes keeps a fixed aspect.
    AspectAdjust(AspectAdjust),
    /// How the title of an axes is aligned with its plot area.
    TitleAlign(TitleAlign),
    /// A colourmap name.
    ColourmapName(ColourmapName),
    /// The legend of an axes.
    Legend(Legend),
    /// A legend drawn inside the plot area of its axes.
    InsideLegend(InsideLegend),
    /// The placement of a legend inside the plot area of its axes.
    LegendLocation(LegendLocation),
    /// The placement of the content of a panel within the panel.
    PanelLocation(PanelLocation),
    /// What a panel holds.
    PanelContent(PanelContent),
    /// The legend content of a panel.
    PanelLegend(PanelLegend),
    /// How the entries of a legend are arranged in columns.
    LegendEntryLayout(LegendEntryLayout),
    /// The number of columns of a legend.
    LegendEntryColumns(LegendEntryColumns),
    /// The order in which the entries of a legend fill its columns.
    LegendEntryOrder(LegendEntryOrder),
    /// How a colour is chosen.
    ColourSpec(ColourSpec),
    /// The style of a stroked line.
    LineStyle(LineStyle),
    /// A dash pattern.
    DashStyle(DashStyle),
    /// The style of markers.
    MarkerStyle(MarkerStyle),
    /// A marker shape.
    MarkerShape(MarkerShape),
    /// How the values of one dimension of a series are placed.
    Coords(Coords),
    /// A list of sizes of the rows or columns of the tile layout.
    TrackSizes(Vec<TrackSize>),
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
    /// What is drawn for a value that a colour scale cannot place.
    OutOfRange(OutOfRange),
    /// A category of values that a colour scale cannot place, with its policy and key.
    ColourScaleOutOfRange(ColourScaleOutOfRange),
    /// The kind of a colour scale, with everything particular to that kind.
    ColourScaleKind(ColourScaleKind),
    /// A continuous colour scale.
    ContinuousColourScale(ContinuousColourScale),
    /// A discrete colour scale.
    DiscreteColourScale(DiscreteColourScale),
    /// An indexed colour scale.
    IndexedColourScale(IndexedColourScale),
    /// A categorical colour scale.
    CategoricalColourScale(CategoricalColourScale),
    /// The palette of a continuous or discrete colour scale.
    Colourmap(Colourmap),
    /// The space in which a colour scale places values.
    ColourScaleNorm(ColourScaleNorm),
    /// The band edges of a discrete colour scale.
    ColourScaleLevels(ColourScaleLevels),
    /// The colourbar content of a panel.
    PanelColourbar(PanelColourbar),
    /// The direction in which a panel colourbar runs.
    ColourbarOrientation(ColourbarOrientation),
    /// The colourbar inside an axes.
    InsideColourbar(InsideColourbar),
    /// The edge along which an inside colourbar lies.
    InsideColourbarSide(InsideColourbarSide),
    /// Where the title of a colour scale is drawn beside a colourbar.
    ColourbarTitlePlacement(ColourbarTitlePlacement),
    /// The direction in which bars or stairs extend.
    Orientation(Orientation),
    /// A number given once for every bar of a series or once per bar, such as its base.
    PerBar(PerBar),
    /// How wide each bar of a series is along the category axis.
    BarWidth(BarWidth),
    /// How far the centre of each bar of a series lies from its position.
    BarOffset(BarOffset),
    /// How the bins of a histogram of one dimension were made from its samples.
    Binning(Binning),
    /// The steps of stairs: values between edges, or values at samples.
    StairsData(StairsData),
    /// Where the step of stairs through samples occurs relative to its samples.
    StepAt(StepAt),
    /// The edges of the columns or rows of cells.
    CellEdges(CellEdges),
    /// The fractions of the extent of each cell left empty along x and along y.
    CellGap(CellGap),
    /// How the bins of a histogram of two dimensions were made from its samples.
    Binning2d(Binning2d),
    /// How the bins of a histogram were specified.
    Bins(Bins),
    /// A named rule that chooses the number of bins of a histogram.
    BinRule(BinRule),
    /// How the counts of the bins of a histogram are normalised.
    HistNorm(HistNorm),
    /// An unsigned 64-bit integer, such as a number of samples.
    UInt64(u64),
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
    /// See [`Value::NodeId`].
    NodeId,
    /// See [`Value::Doubles`].
    Doubles,
    /// See [`Value::Strings`].
    Strings,
    /// See [`Value::Text`].
    Text,
    /// See [`Value::Texts`].
    Texts,
    /// See [`Value::Interpreter`].
    Interpreter,
    /// See [`Value::FigureSize`].
    FigureSize,
    /// See [`Value::FontSetId`].
    FontSetId,
    /// See [`Value::Embolden`].
    Embolden,
    /// See [`Value::Colour`].
    Colour,
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
    /// See [`Value::PolarView`].
    PolarView,
    /// See [`Value::SphericalView`].
    SphericalView,
    /// See [`Value::AngleUnits`].
    AngleUnits,
    /// See [`Value::PolarAngle`].
    PolarAngle,
    /// See [`Value::AxisSide`].
    AxisSide,
    /// See [`Value::Axis`].
    Axis,
    /// See [`Value::Scale`].
    Scale,
    /// See [`Value::Limits`].
    Limits,
    /// See [`Value::Ticks`].
    Ticks,
    /// See [`Value::TickLabels`].
    TickLabels,
    /// See [`Value::Aspect`].
    Aspect,
    /// See [`Value::AspectAdjust`].
    AspectAdjust,
    /// See [`Value::TitleAlign`].
    TitleAlign,
    /// See [`Value::ColourmapName`].
    ColourmapName,
    /// See [`Value::Legend`].
    Legend,
    /// See [`Value::InsideLegend`].
    InsideLegend,
    /// See [`Value::LegendLocation`].
    LegendLocation,
    /// See [`Value::PanelLocation`].
    PanelLocation,
    /// See [`Value::PanelContent`].
    PanelContent,
    /// See [`Value::PanelLegend`].
    PanelLegend,
    /// See [`Value::LegendEntryLayout`].
    LegendEntryLayout,
    /// See [`Value::LegendEntryColumns`].
    LegendEntryColumns,
    /// See [`Value::LegendEntryOrder`].
    LegendEntryOrder,
    /// See [`Value::ColourSpec`].
    ColourSpec,
    /// See [`Value::LineStyle`].
    LineStyle,
    /// See [`Value::DashStyle`].
    DashStyle,
    /// See [`Value::MarkerStyle`].
    MarkerStyle,
    /// See [`Value::MarkerShape`].
    MarkerShape,
    /// See [`Value::Coords`].
    Coords,
    /// See [`Value::TrackSizes`].
    TrackSizes,
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
    /// See [`Value::ColourScaleOutOfRange`].
    ColourScaleOutOfRange,
    /// See [`Value::ColourScaleKind`].
    ColourScaleKind,
    /// See [`Value::ContinuousColourScale`].
    ContinuousColourScale,
    /// See [`Value::DiscreteColourScale`].
    DiscreteColourScale,
    /// See [`Value::IndexedColourScale`].
    IndexedColourScale,
    /// See [`Value::CategoricalColourScale`].
    CategoricalColourScale,
    /// See [`Value::Colourmap`].
    Colourmap,
    /// See [`Value::ColourScaleNorm`].
    ColourScaleNorm,
    /// See [`Value::ColourScaleLevels`].
    ColourScaleLevels,
    /// See [`Value::PanelColourbar`].
    PanelColourbar,
    /// See [`Value::ColourbarOrientation`].
    ColourbarOrientation,
    /// See [`Value::InsideColourbar`].
    InsideColourbar,
    /// See [`Value::InsideColourbarSide`].
    InsideColourbarSide,
    /// See [`Value::ColourbarTitlePlacement`].
    ColourbarTitlePlacement,
    /// See [`Value::Orientation`].
    Orientation,
    /// See [`Value::PerBar`].
    PerBar,
    /// See [`Value::BarWidth`].
    BarWidth,
    /// See [`Value::BarOffset`].
    BarOffset,
    /// See [`Value::Binning`].
    Binning,
    /// See [`Value::StairsData`].
    StairsData,
    /// See [`Value::StepAt`].
    StepAt,
    /// See [`Value::CellEdges`].
    CellEdges,
    /// See [`Value::CellGap`].
    CellGap,
    /// See [`Value::Binning2d`].
    Binning2d,
    /// See [`Value::Bins`].
    Bins,
    /// See [`Value::BinRule`].
    BinRule,
    /// See [`Value::HistNorm`].
    HistNorm,
    /// See [`Value::UInt64`].
    UInt64,
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
            Value::NodeId(_) => ValueType::NodeId,
            Value::Doubles(_) => ValueType::Doubles,
            Value::Strings(_) => ValueType::Strings,
            Value::Text(_) => ValueType::Text,
            Value::Texts(_) => ValueType::Texts,
            Value::Interpreter(_) => ValueType::Interpreter,
            Value::FigureSize(_) => ValueType::FigureSize,
            Value::FontSetId(_) => ValueType::FontSetId,
            Value::Embolden(_) => ValueType::Embolden,
            Value::Colour(_) => ValueType::Colour,
            Value::TileLayout(_) => ValueType::TileLayout,
            Value::Links(_) => ValueType::Links,
            Value::Parameters(_) => ValueType::Parameters,
            Value::Cell(_) => ValueType::Cell,
            Value::Projection(_) => ValueType::Projection,
            Value::View3d(_) => ValueType::View3d,
            Value::PolarView(_) => ValueType::PolarView,
            Value::SphericalView(_) => ValueType::SphericalView,
            Value::AngleUnits(_) => ValueType::AngleUnits,
            Value::PolarAngle(_) => ValueType::PolarAngle,
            Value::AxisSide(_) => ValueType::AxisSide,
            Value::Axis(_) => ValueType::Axis,
            Value::Scale(_) => ValueType::Scale,
            Value::Limits(_) => ValueType::Limits,
            Value::Ticks(_) => ValueType::Ticks,
            Value::TickLabels(_) => ValueType::TickLabels,
            Value::Aspect(_) => ValueType::Aspect,
            Value::AspectAdjust(_) => ValueType::AspectAdjust,
            Value::TitleAlign(_) => ValueType::TitleAlign,
            Value::ColourmapName(_) => ValueType::ColourmapName,
            Value::Legend(_) => ValueType::Legend,
            Value::InsideLegend(_) => ValueType::InsideLegend,
            Value::LegendLocation(_) => ValueType::LegendLocation,
            Value::PanelLocation(_) => ValueType::PanelLocation,
            Value::PanelContent(_) => ValueType::PanelContent,
            Value::PanelLegend(_) => ValueType::PanelLegend,
            Value::LegendEntryLayout(_) => ValueType::LegendEntryLayout,
            Value::LegendEntryColumns(_) => ValueType::LegendEntryColumns,
            Value::LegendEntryOrder(_) => ValueType::LegendEntryOrder,
            Value::ColourSpec(_) => ValueType::ColourSpec,
            Value::LineStyle(_) => ValueType::LineStyle,
            Value::DashStyle(_) => ValueType::DashStyle,
            Value::MarkerStyle(_) => ValueType::MarkerStyle,
            Value::MarkerShape(_) => ValueType::MarkerShape,
            Value::Coords(_) => ValueType::Coords,
            Value::TrackSizes(_) => ValueType::TrackSizes,
            Value::Grid(_) => ValueType::Grid,
            Value::Levels(_) => ValueType::Levels,
            Value::ContourPlacement(_) => ValueType::ContourPlacement,
            Value::QuiverScale(_) => ValueType::QuiverScale,
            Value::ImagePlacement(_) => ValueType::ImagePlacement,
            Value::PixelRange(_) => ValueType::PixelRange,
            Value::ImagePlane(_) => ValueType::ImagePlane,
            Value::OutOfRange(_) => ValueType::OutOfRange,
            Value::ColourScaleOutOfRange(_) => ValueType::ColourScaleOutOfRange,
            Value::ColourScaleKind(_) => ValueType::ColourScaleKind,
            Value::ContinuousColourScale(_) => ValueType::ContinuousColourScale,
            Value::DiscreteColourScale(_) => ValueType::DiscreteColourScale,
            Value::IndexedColourScale(_) => ValueType::IndexedColourScale,
            Value::CategoricalColourScale(_) => ValueType::CategoricalColourScale,
            Value::Colourmap(_) => ValueType::Colourmap,
            Value::ColourScaleNorm(_) => ValueType::ColourScaleNorm,
            Value::ColourScaleLevels(_) => ValueType::ColourScaleLevels,
            Value::PanelColourbar(_) => ValueType::PanelColourbar,
            Value::ColourbarOrientation(_) => ValueType::ColourbarOrientation,
            Value::InsideColourbar(_) => ValueType::InsideColourbar,
            Value::InsideColourbarSide(_) => ValueType::InsideColourbarSide,
            Value::ColourbarTitlePlacement(_) => ValueType::ColourbarTitlePlacement,
            Value::Orientation(_) => ValueType::Orientation,
            Value::PerBar(_) => ValueType::PerBar,
            Value::BarWidth(_) => ValueType::BarWidth,
            Value::BarOffset(_) => ValueType::BarOffset,
            Value::Binning(_) => ValueType::Binning,
            Value::StairsData(_) => ValueType::StairsData,
            Value::StepAt(_) => ValueType::StepAt,
            Value::CellEdges(_) => ValueType::CellEdges,
            Value::CellGap(_) => ValueType::CellGap,
            Value::Binning2d(_) => ValueType::Binning2d,
            Value::Bins(_) => ValueType::Bins,
            Value::BinRule(_) => ValueType::BinRule,
            Value::HistNorm(_) => ValueType::HistNorm,
            Value::UInt64(_) => ValueType::UInt64,
        })
    }
}
