//! Conversions between the domain types and the wire types.
//!
//! Encoding is infallible and writes every field that has a value. Decoding merges
//! each present field onto the default of its context, as described in the
//! [module documentation](super), and fails on unknown enum values and on absent
//! values that have no default.

use std::collections::BTreeMap;

use crate::error::{IrError, ProtobufError};
use crate::wire as w;
use crate::{
    Artist, Aspect, AspectAdjust, Axes, Axis, AxisLink, Cell, Color, ColorSpec, ColormapName,
    Contour, ContourPlacement, Coords, DashStyle, DataArray, Dimension, Edit, Figure, FigureSize,
    FontSetId, Grid, Image, ImagePlacement, ImagePlane, IndexedImage, InsideLegend, Interpreter,
    Legend, LegendEntries, LegendEntryColumns, LegendEntryLayout, LegendEntryOrder, LegendItem,
    LegendItemContent, LegendLocation, LegendSample, Levels, Limits, Line, LineStyle, MappedImage,
    MarkerShape, MarkerStyle, NdArray, NdArrayElement, Node, NodeId, OutOfRange, Panel,
    PanelContent, PanelLegend, PanelLocation, Parameter, PixelRange, Projection, Provenance,
    Quiver, QuiverScale, Scale, Scatter, ScatterColor, ScatterSize, Surface, Text, TickLabels,
    Ticks, TileLayout, TitleAlign, TrackSize, Transaction, Value, Values, View3d,
};

type Result<T> = std::result::Result<T, ProtobufError>;

// ---------------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------------

/// A domain type that is encoded as a wire message of its own.
///
/// Every kind of node, every kind of artist, [`Artist`] and [`Node`] implement it. The
/// implementations for [`Artist`] and [`Node`] are generated from the declaration of the
/// kinds of node, and set the variant of the wire oneof of the same name as the kind.
pub(crate) trait Encode {
    /// The wire message.
    type Message;

    /// Returns the wire message of the value.
    fn encode(&self) -> Self::Message;
}

/// A domain type that is decoded from a wire message of its own, as [`Encode`] encodes
/// it.
pub(crate) trait Decode: Sized {
    /// The wire message.
    type Message;

    /// Decodes the message at path `at`.
    fn decode(wire: Self::Message, at: &str) -> Result<Self>;

    /// Decodes the message held by the field `field` of the message at path `at`.
    fn decode_field(wire: Self::Message, at: &str, field: &str) -> Result<Self> {
        Self::decode(wire, &join(at, field))
    }
}

// ---------------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------------

/// A domain enum with a wire enum of the same variants.
trait WireEnum: Sized {
    /// Returns the number of the wire value of the same name.
    fn to_wire(self) -> i32;

    /// Returns the domain variant of a wire value: `Some(None)` for the unspecified
    /// value, and `None` for a number that the wire enum does not define.
    fn from_wire(number: i32) -> Option<Option<Self>>;
}

/// Implements [`WireEnum`] for a domain enum whose variants have the same names as the
/// values of a wire enum. Both matches are exhaustive, so a variant added to either
/// enum without the other does not compile.
macro_rules! wire_enum {
    ($($domain:ident => $wire:ident { $($variant:ident),+ $(,)? })*) => {$(
        impl WireEnum for $domain {
            fn to_wire(self) -> i32 {
                match self {
                    $($domain::$variant => w::$wire::$variant as i32,)+
                }
            }

            fn from_wire(number: i32) -> Option<Option<Self>> {
                match w::$wire::try_from(number).ok()? {
                    w::$wire::Unspecified => Some(None),
                    $(w::$wire::$variant => Some(Some($domain::$variant)),)+
                }
            }
        }
    )*};
}

wire_enum! {
    FontSetId => FontSetId { StixTwo }
    Interpreter => Interpreter { Latex, None }
    Scale => Scale { Linear, Log }
    ColormapName => ColormapName { Viridis, Cividis, Magma, Inferno, Plasma, Coolwarm, Gray }
    LegendLocation => LegendLocation {
        NorthEast, NorthWest, SouthEast, SouthWest, North, South, East, West, Center, Best,
    }
    PanelLocation => PanelLocation {
        Center, North, South, East, West, NorthEast, NorthWest, SouthEast, SouthWest,
    }
    LegendEntryOrder => LegendEntryOrder { ColumnMajor, RowMajor }
    DashStyle => DashStyle { Solid, Dashed, Dotted, DashDot, None, DashDotDot, LongDash }
    AspectAdjust => AspectAdjust { Box, Limits }
    TitleAlign => TitleAlign { Center, Left, Right }
    MarkerShape => MarkerShape {
        None, Circle, Square, Diamond, TriangleUp, TriangleDown, Plus, Cross, Point, VLine, HLine,
    }
    Coords => Coords { Data, AxesFraction }
    Dimension => Dimension { X, Y, Z }
    NdArrayElement => NdArrayElement { F64, U8 }
}

// ---------------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------------

impl From<&Figure> for w::Figure {
    fn from(figure: &Figure) -> Self {
        w::Figure {
            schema_version: figure.schema_version.clone(),
            id: Some(figure.id.0),
            title: figure.title.as_ref().map(encode_text),
            size: Some(encode_figure_size(figure.size)),
            font_set: figure.font_set.to_wire(),
            font_size_pt: Some(figure.font_size_pt),
            background: Some(encode_color(figure.background)),
            layout: Some(encode_tile_layout(&figure.layout)),
            data: figure.data.iter().map(DataArray::encode).collect(),
            axes: figure.axes.iter().map(Axes::encode).collect(),
            links: encode_links(&figure.links),
            provenance: Some(w::Provenance {
                ironlab_version: figure.provenance.ironlab_version.clone(),
                typesetter: figure.provenance.typesetter.clone(),
                typesetter_version: figure.provenance.typesetter_version.clone(),
                fonts: figure.provenance.fonts.clone(),
            }),
            // Both maps are ordered by name, so the entries are written in ascending order
            // of the UTF-8 bytes of the name.
            parameters: figure
                .parameters
                .iter()
                .map(|(name, parameter)| (name.clone(), encode_parameter(parameter)))
                .collect(),
            // Tags are a sequence rather than a map, so they are written in the order the
            // figure carries them rather than sorted; that order is part of the figure's value.
            tags: figure.tags.clone(),
            panels: figure.panels.iter().map(Panel::encode).collect(),
        }
    }
}

impl Encode for DataArray {
    type Message = w::DataArray;

    fn encode(&self) -> w::DataArray {
        w::DataArray {
            id: Some(self.id.0),
            array: Some(encode_array(&self.array)),
            name: self.name.clone(),
        }
    }
}

/// Encodes an array, writing its element explicitly and its values in the payload of
/// that element only, so that a reader never finds two candidate payloads.
fn encode_array(array: &NdArray) -> w::NdArray {
    let (values, u8_values) = match &array.values {
        Values::F64(values) => (values.clone(), Vec::new()),
        Values::U8(values) => (Vec::new(), values.clone()),
    };
    w::NdArray {
        shape: array.shape.iter().map(|&len| len as u64).collect(),
        values,
        element: array.element().to_wire(),
        u8_values,
    }
}

fn encode_figure_size(size: FigureSize) -> w::FigureSize {
    w::FigureSize {
        width_mm: Some(size.width_mm),
        height_mm: Some(size.height_mm),
    }
}

fn encode_tile_layout(layout: &TileLayout) -> w::TileLayout {
    w::TileLayout {
        rows: Some(layout.rows),
        cols: Some(layout.cols),
        row_weights: layout
            .row_weights
            .iter()
            .map(|&size| encode_track_size(size))
            .collect(),
        col_weights: layout
            .col_weights
            .iter()
            .map(|&size| encode_track_size(size))
            .collect(),
    }
}

fn encode_track_size(size: TrackSize) -> w::TrackSize {
    let kind = match size {
        TrackSize::Weight(weight) => w::TrackSizeKind::Weight(w::TrackSizeWeight {
            weight: Some(weight),
        }),
        TrackSize::Fit => w::TrackSizeKind::Fit(w::TrackSizeFit {}),
    };
    w::TrackSize { kind: Some(kind) }
}

fn encode_links(links: &[AxisLink]) -> Vec<w::AxisLink> {
    links
        .iter()
        .map(|link| w::AxisLink {
            dimension: link.dimension.to_wire(),
            axes: link.axes.iter().map(|id| id.0).collect(),
        })
        .collect()
}

fn encode_parameter(parameter: &Parameter) -> w::Parameter {
    let kind = match parameter {
        Parameter::Bool(value) => w::ParameterKind::Bool(w::ParameterBool {
            value: Some(*value),
        }),
        Parameter::Integer(value) => w::ParameterKind::Integer(w::ParameterInteger {
            value: Some(*value),
        }),
        Parameter::Number(value) => w::ParameterKind::Number(w::ParameterNumber {
            value: Some(*value),
        }),
        Parameter::String(value) => w::ParameterKind::String(w::ParameterString {
            value: value.clone(),
        }),
    };
    w::Parameter { kind: Some(kind) }
}

fn encode_text(text: &Text) -> w::Text {
    w::Text {
        content: text.content.clone(),
        interpreter: text.interpreter.to_wire(),
    }
}

fn encode_color(color: Color) -> w::Color {
    w::Color {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}

fn encode_color_spec(spec: ColorSpec) -> w::ColorSpec {
    let kind = match spec {
        ColorSpec::Auto => w::ColorSpecKind::Auto(w::ColorSpecAuto {}),
        ColorSpec::Rgba { color } => w::ColorSpecKind::Rgba(w::ColorSpecRgba {
            color: Some(encode_color(color)),
        }),
        ColorSpec::None => w::ColorSpecKind::None(w::ColorSpecNone {}),
        ColorSpec::Colormapped => w::ColorSpecKind::Colormapped(w::ColorSpecColormapped {}),
    };
    w::ColorSpec { kind: Some(kind) }
}

fn encode_line_style(style: LineStyle) -> w::LineStyle {
    w::LineStyle {
        color: Some(encode_color_spec(style.color)),
        width_pt: Some(style.width_pt),
        dash: style.dash.to_wire(),
    }
}

fn encode_marker_style(style: MarkerStyle) -> w::MarkerStyle {
    w::MarkerStyle {
        shape: style.shape.to_wire(),
        size_pt: Some(style.size_pt),
        face: Some(encode_color_spec(style.face)),
        edge: Some(encode_color_spec(style.edge)),
    }
}

fn encode_limits(limits: Limits) -> w::Limits {
    let kind = match limits {
        Limits::Auto => w::LimitsKind::Auto(w::LimitsAuto {}),
        Limits::Manual { min, max } => w::LimitsKind::Manual(w::LimitsManual {
            min: Some(min),
            max: Some(max),
        }),
    };
    w::Limits { kind: Some(kind) }
}

fn encode_ticks(ticks: &Ticks) -> w::Ticks {
    let kind = match ticks {
        Ticks::Auto => w::TicksKind::Auto(w::TicksAuto {}),
        Ticks::Step { step, origin } => w::TicksKind::Step(w::TicksStep {
            step: Some(*step),
            origin: Some(*origin),
        }),
        Ticks::Values(values) => w::TicksKind::Values(w::TicksValues {
            values: values.clone(),
        }),
    };
    w::Ticks { kind: Some(kind) }
}

fn encode_tick_labels(labels: &TickLabels) -> w::TickLabels {
    let kind = match labels {
        TickLabels::Auto => w::TickLabelsKind::Auto(w::TickLabelsAuto {}),
        TickLabels::Hidden => w::TickLabelsKind::Hidden(w::TickLabelsHidden {}),
        TickLabels::Text(texts) => w::TickLabelsKind::Text(w::TickLabelsText {
            labels: texts.iter().map(encode_text).collect(),
        }),
    };
    w::TickLabels { kind: Some(kind) }
}

fn encode_axis(axis: &Axis) -> w::Axis {
    w::Axis {
        label: axis.label.as_ref().map(encode_text),
        scale: axis.scale.to_wire(),
        limits: Some(encode_limits(axis.limits)),
        grid: Some(axis.grid),
        reverse: Some(axis.reverse),
        ticks: Some(encode_ticks(&axis.ticks)),
        tick_labels: Some(encode_tick_labels(&axis.tick_labels)),
    }
}

fn encode_view3d(view: View3d) -> w::View3d {
    w::View3d {
        azimuth_deg: Some(view.azimuth_deg),
        elevation_deg: Some(view.elevation_deg),
        zoom: Some(view.zoom),
        pan_x: Some(view.pan_x),
        pan_y: Some(view.pan_y),
    }
}

fn encode_projection(projection: Projection) -> w::Projection {
    let kind = match projection {
        Projection::TwoD => w::ProjectionKind::TwoD(w::ProjectionTwoD {}),
        Projection::ThreeD { view3d } => w::ProjectionKind::ThreeD(w::ProjectionThreeD {
            view3d: Some(encode_view3d(view3d)),
        }),
    };
    w::Projection { kind: Some(kind) }
}

fn encode_cell(cell: Cell) -> w::Cell {
    w::Cell {
        row: Some(cell.row),
        col: Some(cell.col),
        row_span: Some(cell.row_span),
        col_span: Some(cell.col_span),
    }
}

fn encode_legend(legend: &Legend) -> w::Legend {
    let kind = match legend {
        Legend::Inside(inside) => w::LegendKind::Inside(w::LegendInside {
            inside: Some(encode_inside_legend(inside)),
        }),
        Legend::Panel(panel) => w::LegendKind::Panel(w::LegendPanel {
            panel: Some(panel.0),
        }),
    };
    w::Legend { kind: Some(kind) }
}

fn encode_inside_legend(legend: &InsideLegend) -> w::InsideLegend {
    w::InsideLegend {
        location: legend.location.to_wire(),
        boxed: Some(legend.boxed),
        layout: Some(encode_legend_entry_layout(legend.layout)),
        entries: Some(encode_legend_entries(&legend.entries)),
    }
}

impl Encode for Panel {
    type Message = w::Panel;

    fn encode(&self) -> w::Panel {
        w::Panel {
            id: Some(self.id.0),
            cell: Some(encode_cell(self.cell)),
            align: self.align.to_wire(),
            content: Some(encode_panel_content(&self.content)),
        }
    }
}

fn encode_panel_content(content: &PanelContent) -> w::PanelContent {
    let kind = match content {
        PanelContent::Legend(legend) => w::PanelContentKind::Legend(w::PanelContentLegend {
            legend: Some(encode_panel_legend(legend)),
        }),
    };
    w::PanelContent { kind: Some(kind) }
}

fn encode_panel_legend(legend: &PanelLegend) -> w::PanelLegend {
    w::PanelLegend {
        boxed: Some(legend.boxed),
        layout: Some(encode_legend_entry_layout(legend.layout)),
        entries: Some(encode_legend_entries(&legend.entries)),
    }
}

fn encode_legend_entries(entries: &LegendEntries) -> w::LegendEntries {
    let kind = match entries {
        LegendEntries::Auto => w::LegendEntriesKind::Auto(w::LegendEntriesAuto {}),
        LegendEntries::Explicit(items) => {
            w::LegendEntriesKind::Explicit(w::LegendEntriesExplicit {
                items: items.iter().map(encode_legend_item).collect(),
            })
        }
    };
    w::LegendEntries { kind: Some(kind) }
}

fn encode_legend_item(item: &LegendItem) -> w::LegendItem {
    let kind = match &item.content {
        LegendItemContent::Artist(artist) => {
            w::LegendItemContentKind::Artist(w::LegendItemContentArtist {
                artist: Some(artist.0),
            })
        }
        LegendItemContent::Custom { label, sample } => {
            w::LegendItemContentKind::Custom(w::LegendItemContentCustom {
                label: Some(encode_text(label)),
                sample: Some(encode_legend_sample(*sample)),
            })
        }
    };
    w::LegendItem {
        content: Some(w::LegendItemContent { kind: Some(kind) }),
        full_row: Some(item.full_row),
    }
}

fn encode_legend_sample(sample: LegendSample) -> w::LegendSample {
    let kind = match sample {
        LegendSample::Line { line, marker } => w::LegendSampleKind::Line(w::LegendSampleLine {
            line: Some(encode_line_style(line)),
            marker: Some(encode_marker_style(marker)),
        }),
        LegendSample::Swatch { color } => w::LegendSampleKind::Swatch(w::LegendSampleSwatch {
            color: Some(encode_color(color)),
        }),
    };
    w::LegendSample { kind: Some(kind) }
}

fn encode_legend_entry_layout(layout: LegendEntryLayout) -> w::LegendEntryLayout {
    w::LegendEntryLayout {
        columns: Some(encode_legend_entry_columns(layout.columns)),
        order: layout.order.to_wire(),
    }
}

fn encode_legend_entry_columns(columns: LegendEntryColumns) -> w::LegendEntryColumns {
    let kind = match columns {
        LegendEntryColumns::Auto => w::LegendEntryColumnsKind::Auto(w::LegendEntryColumnsAuto {}),
        LegendEntryColumns::Count(count) => {
            w::LegendEntryColumnsKind::Count(w::LegendEntryColumnsCount { count: Some(count) })
        }
    };
    w::LegendEntryColumns { kind: Some(kind) }
}

impl Encode for Axes {
    type Message = w::Axes;

    fn encode(&self) -> w::Axes {
        w::Axes {
            id: Some(self.id.0),
            cell: Some(encode_cell(self.cell)),
            projection: Some(encode_projection(self.projection)),
            title: self.title.as_ref().map(encode_text),
            x: Some(encode_axis(&self.x)),
            y: Some(encode_axis(&self.y)),
            z: Some(encode_axis(&self.z)),
            r#box: Some(self.box_),
            colormap: self.colormap.to_wire(),
            clim: Some(encode_limits(self.clim)),
            legend: self.legend.as_ref().map(encode_legend),
            artists: self.artists.iter().map(Artist::encode).collect(),
            title_align: self.title_align.to_wire(),
            aspect: Some(encode_aspect(self.aspect)),
            aspect_adjust: self.aspect_adjust.to_wire(),
        }
    }
}

fn encode_aspect(aspect: Aspect) -> w::Aspect {
    let kind = match aspect {
        Aspect::Auto => w::AspectKind::Auto(w::AspectAuto {}),
        Aspect::Equal => w::AspectKind::Equal(w::AspectEqual {}),
        Aspect::Ratio { y, z } => w::AspectKind::Ratio(w::AspectRatio {
            y: Some(y),
            z: Some(z),
        }),
    };
    w::Aspect { kind: Some(kind) }
}

fn encode_grid(grid: Grid) -> w::Grid {
    let kind = match grid {
        Grid::Rectilinear { x, y } => w::GridKind::Rectilinear(w::GridRectilinear {
            x: Some(x.0),
            y: Some(y.0),
        }),
        Grid::Curvilinear { x, y } => w::GridKind::Curvilinear(w::GridCurvilinear {
            x: Some(x.0),
            y: Some(y.0),
        }),
    };
    w::Grid { kind: Some(kind) }
}

fn encode_scatter_size(size: ScatterSize) -> w::ScatterSize {
    let kind = match size {
        ScatterSize::Scalar { value } => {
            w::ScatterSizeKind::Scalar(w::ScatterSizeScalar { value: Some(value) })
        }
        ScatterSize::Data { data } => {
            w::ScatterSizeKind::Data(w::ScatterSizeData { data: Some(data.0) })
        }
    };
    w::ScatterSize { kind: Some(kind) }
}

fn encode_scatter_color(color: ScatterColor) -> w::ScatterColor {
    let kind = match color {
        ScatterColor::Spec { spec } => w::ScatterColorKind::Spec(w::ScatterColorSpec {
            spec: Some(encode_color_spec(spec)),
        }),
        ScatterColor::Data { data } => {
            w::ScatterColorKind::Data(w::ScatterColorData { data: Some(data.0) })
        }
    };
    w::ScatterColor { kind: Some(kind) }
}

fn encode_levels(levels: &Levels) -> w::Levels {
    let kind = match levels {
        Levels::Auto { count } => w::LevelsKind::Auto(w::LevelsAuto {
            count: Some(*count),
        }),
        Levels::Explicit { values } => w::LevelsKind::Explicit(w::LevelsExplicit {
            values: values.clone(),
        }),
    };
    w::Levels { kind: Some(kind) }
}

fn encode_contour_placement(placement: ContourPlacement) -> w::ContourPlacement {
    let kind = match placement {
        ContourPlacement::Plane { z } => {
            w::ContourPlacementKind::Plane(w::ContourPlacementPlane { z })
        }
        ContourPlacement::AtLevel => {
            w::ContourPlacementKind::AtLevel(w::ContourPlacementAtLevel {})
        }
    };
    w::ContourPlacement { kind: Some(kind) }
}

fn encode_quiver_scale(scale: QuiverScale) -> w::QuiverScale {
    let kind = match scale {
        QuiverScale::Auto => w::QuiverScaleKind::Auto(w::QuiverScaleAuto {}),
        QuiverScale::Factor { value } => {
            w::QuiverScaleKind::Factor(w::QuiverScaleFactor { value: Some(value) })
        }
        QuiverScale::Off => w::QuiverScaleKind::Off(w::QuiverScaleOff {}),
    };
    w::QuiverScale { kind: Some(kind) }
}

fn encode_pixel_range(range: PixelRange) -> w::PixelRange {
    w::PixelRange {
        first: Some(range.first),
        last: Some(range.last),
    }
}

fn encode_image_plane(plane: ImagePlane) -> w::ImagePlane {
    let kind = match plane {
        ImagePlane::Xy { z } => w::ImagePlaneKind::Xy(w::ImagePlaneXy { z }),
        ImagePlane::Xz { y } => w::ImagePlaneKind::Xz(w::ImagePlaneXz { y }),
        ImagePlane::Yz { x } => w::ImagePlaneKind::Yz(w::ImagePlaneYz { x }),
    };
    w::ImagePlane { kind: Some(kind) }
}

/// Encodes the placement of an image, writing an absent pixel range as absent rather
/// than as a range of zeros, which would move the image.
fn encode_image_placement(placement: ImagePlacement) -> w::ImagePlacement {
    w::ImagePlacement {
        plane: Some(encode_image_plane(placement.plane)),
        columns: placement.columns.map(encode_pixel_range),
        rows: placement.rows.map(encode_pixel_range),
    }
}

fn encode_out_of_range(policy: OutOfRange) -> w::OutOfRange {
    let kind = match policy {
        OutOfRange::Strict => w::OutOfRangeKind::Strict(w::OutOfRangeStrict {}),
        OutOfRange::Transparent => w::OutOfRangeKind::Transparent(w::OutOfRangeTransparent {}),
        OutOfRange::Clamp => w::OutOfRangeKind::Clamp(w::OutOfRangeClamp {}),
        OutOfRange::Rgba { color } => w::OutOfRangeKind::Rgba(w::OutOfRangeRgba {
            color: Some(encode_color(color)),
        }),
    };
    w::OutOfRange { kind: Some(kind) }
}

impl Encode for Line {
    type Message = w::Line;

    fn encode(&self) -> w::Line {
        w::Line {
            id: Some(self.id.0),
            label: self.label.as_ref().map(encode_text),
            visible: Some(self.visible),
            x: Some(self.x.0),
            y: Some(self.y.0),
            z: self.z.map(|id| id.0),
            line: Some(encode_line_style(self.line)),
            marker: Some(encode_marker_style(self.marker)),
            x_coords: self.x_coords.to_wire(),
            y_coords: self.y_coords.to_wire(),
            clip: Some(self.clip),
        }
    }
}

impl Encode for Scatter {
    type Message = w::Scatter;

    fn encode(&self) -> w::Scatter {
        w::Scatter {
            id: Some(self.id.0),
            label: self.label.as_ref().map(encode_text),
            visible: Some(self.visible),
            x: Some(self.x.0),
            y: Some(self.y.0),
            z: self.z.map(|id| id.0),
            size: Some(encode_scatter_size(self.size)),
            color: Some(encode_scatter_color(self.color)),
            marker: Some(encode_marker_style(self.marker)),
            x_coords: self.x_coords.to_wire(),
            y_coords: self.y_coords.to_wire(),
            clip: Some(self.clip),
        }
    }
}

impl Encode for Contour {
    type Message = w::Contour;

    fn encode(&self) -> w::Contour {
        w::Contour {
            id: Some(self.id.0),
            label: self.label.as_ref().map(encode_text),
            visible: Some(self.visible),
            grid: Some(encode_grid(self.grid)),
            z: Some(self.z.0),
            levels: Some(encode_levels(&self.levels)),
            fill: Some(self.fill),
            placement: Some(encode_contour_placement(self.placement)),
            line: Some(encode_line_style(self.line)),
            line_opacity: Some(self.line_opacity),
            fill_opacity: Some(self.fill_opacity),
        }
    }
}

impl Encode for Quiver {
    type Message = w::Quiver;

    fn encode(&self) -> w::Quiver {
        w::Quiver {
            id: Some(self.id.0),
            label: self.label.as_ref().map(encode_text),
            visible: Some(self.visible),
            x: Some(self.x.0),
            y: Some(self.y.0),
            z: self.z.map(|id| id.0),
            u: Some(self.u.0),
            v: Some(self.v.0),
            w: self.w.map(|id| id.0),
            scale: Some(encode_quiver_scale(self.scale)),
            line: Some(encode_line_style(self.line)),
            head_size: Some(self.head_size),
        }
    }
}

impl Encode for Surface {
    type Message = w::Surface;

    fn encode(&self) -> w::Surface {
        w::Surface {
            id: Some(self.id.0),
            label: self.label.as_ref().map(encode_text),
            visible: Some(self.visible),
            grid: Some(encode_grid(self.grid)),
            z: Some(self.z.0),
            c: self.c.map(|id| id.0),
            face: Some(encode_color_spec(self.face)),
            edge: Some(encode_color_spec(self.edge)),
            edge_width_pt: Some(self.edge_width_pt),
            face_alpha: self.face_alpha.map(|id| id.0),
            edge_alpha: self.edge_alpha.map(|id| id.0),
            face_opacity: Some(self.face_opacity),
            edge_opacity: Some(self.edge_opacity),
        }
    }
}

impl Encode for Image {
    type Message = w::Image;

    fn encode(&self) -> w::Image {
        w::Image {
            id: Some(self.id.0),
            label: self.label.as_ref().map(encode_text),
            visible: Some(self.visible),
            pixels: Some(self.pixels.0),
            placement: Some(encode_image_placement(self.placement)),
            opacity: Some(self.opacity),
        }
    }
}

impl Encode for IndexedImage {
    type Message = w::IndexedImage;

    fn encode(&self) -> w::IndexedImage {
        w::IndexedImage {
            id: Some(self.id.0),
            label: self.label.as_ref().map(encode_text),
            visible: Some(self.visible),
            indices: Some(self.indices.0),
            placement: Some(encode_image_placement(self.placement)),
            below: Some(encode_out_of_range(self.below)),
            above: Some(encode_out_of_range(self.above)),
            non_finite: Some(encode_out_of_range(self.non_finite)),
            alpha: self.alpha.map(|id| id.0),
            opacity: Some(self.opacity),
        }
    }
}

impl Encode for MappedImage {
    type Message = w::MappedImage;

    fn encode(&self) -> w::MappedImage {
        w::MappedImage {
            id: Some(self.id.0),
            label: self.label.as_ref().map(encode_text),
            visible: Some(self.visible),
            values: Some(self.values.0),
            placement: Some(encode_image_placement(self.placement)),
            below: Some(encode_out_of_range(self.below)),
            above: Some(encode_out_of_range(self.above)),
            non_finite: Some(encode_out_of_range(self.non_finite)),
            alpha: self.alpha.map(|id| id.0),
            opacity: Some(self.opacity),
        }
    }
}

// ---------------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------------

impl TryFrom<w::Figure> for Figure {
    type Error = IrError;

    fn try_from(wire: w::Figure) -> std::result::Result<Self, Self::Error> {
        Ok(decode_figure(wire)?)
    }
}

/// Returns the path of a field within the message at path `at`, which is empty for the
/// figure.
fn join(at: &str, field: &str) -> String {
    if at.is_empty() {
        field.to_owned()
    } else {
        format!("{at}.{field}")
    }
}

/// Returns a value that has no default, or reports it as missing.
pub(crate) fn required<T>(value: Option<T>, at: &str, field: &str) -> Result<T> {
    value.ok_or_else(|| ProtobufError::MissingField {
        field: join(at, field),
    })
}

/// Returns the identifier of a node, which has no default.
fn node_id(value: Option<u64>, at: &str) -> Result<NodeId> {
    required(value, at, "id").map(NodeId)
}

/// Returns a reference to a node, such as the data array of an artist, which has no
/// default.
fn reference(value: Option<u64>, at: &str, field: &str) -> Result<NodeId> {
    required(value, at, field).map(NodeId)
}

/// Decodes an enum value, taking `default` for the unspecified value, or reporting the
/// unspecified value as missing when there is no default.
fn decode_enum<E: WireEnum>(number: i32, default: Option<E>, at: &str, field: &str) -> Result<E> {
    match E::from_wire(number) {
        Some(Some(value)) => Ok(value),
        Some(None) => default.ok_or_else(|| ProtobufError::MissingField {
            field: join(at, field),
        }),
        None => Err(ProtobufError::UnknownEnumValue {
            field: join(at, field),
            value: number,
        }),
    }
}

fn decode_figure(wire: w::Figure) -> Result<Figure> {
    let default = Figure::default();
    let provenance = wire.provenance.unwrap_or_default();
    let data = wire
        .data
        .into_iter()
        .enumerate()
        .map(|(i, data)| DataArray::decode(data, &format!("data[{i}]")))
        .collect::<Result<Vec<_>>>()?;
    let axes = wire
        .axes
        .into_iter()
        .enumerate()
        .map(|(i, axes)| Axes::decode(axes, &format!("axes[{i}]")))
        .collect::<Result<Vec<_>>>()?;
    let panels = wire
        .panels
        .into_iter()
        .enumerate()
        .map(|(i, panel)| Panel::decode(panel, &format!("panels[{i}]")))
        .collect::<Result<Vec<_>>>()?;
    let links = decode_links(wire.links, "")?;
    let parameters = wire
        .parameters
        .into_iter()
        .map(|(name, parameter)| {
            let parameter = decode_parameter(parameter, &format!("parameters[{name:?}]"))?;
            Ok((name, parameter))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    Ok(Figure {
        schema_version: wire.schema_version,
        id: node_id(wire.id, "")?,
        title: wire.title.map(|t| decode_text(t, "title")).transpose()?,
        size: wire.size.map_or(default.size, decode_figure_size),
        font_set: decode_enum(wire.font_set, Some(default.font_set), "", "font_set")?,
        font_size_pt: wire.font_size_pt.unwrap_or(default.font_size_pt),
        background: wire.background.map_or(default.background, decode_color),
        layout: wire
            .layout
            .map(|layout| decode_tile_layout(layout, "layout"))
            .transpose()?
            .unwrap_or(default.layout),
        data,
        axes,
        panels,
        links,
        provenance: Provenance {
            ironlab_version: provenance.ironlab_version,
            typesetter: provenance.typesetter,
            typesetter_version: provenance.typesetter_version,
            fonts: provenance.fonts,
        },
        parameters,
        tags: wire.tags,
        id_allocator: default.id_allocator,
    })
}

/// Decodes the axis links of a figure, whose dimensions have no default.
fn decode_links(wire: Vec<w::AxisLink>, at: &str) -> Result<Vec<AxisLink>> {
    wire.into_iter()
        .enumerate()
        .map(|(i, link)| {
            let at = join(at, &format!("links[{i}]"));
            Ok(AxisLink {
                dimension: decode_enum(link.dimension, None, &at, "dimension")?,
                axes: link.axes.into_iter().map(NodeId).collect(),
            })
        })
        .collect()
}

/// Decodes a parameter, whose kind and value have no default.
fn decode_parameter(wire: w::Parameter, at: &str) -> Result<Parameter> {
    Ok(match required(wire.kind, at, "kind")? {
        w::ParameterKind::Bool(bool) => {
            Parameter::Bool(required(bool.value, &join(at, "bool_value"), "value")?)
        }
        w::ParameterKind::Integer(integer) => Parameter::Integer(required(
            integer.value,
            &join(at, "integer_value"),
            "value",
        )?),
        w::ParameterKind::Number(number) => {
            Parameter::Number(required(number.value, &join(at, "number_value"), "value")?)
        }
        w::ParameterKind::String(string) => Parameter::String(string.value),
    })
}

/// Decodes an array, taking an unspecified element as floating-point values and
/// refusing values in the payload of the other element.
fn decode_array(wire: w::NdArray, at: &str) -> Result<NdArray> {
    let shape = wire
        .shape
        .into_iter()
        .map(|len| {
            usize::try_from(len).map_err(|_| ProtobufError::InvalidValue {
                field: join(at, "shape"),
                reason: format!("the dimension {len} exceeds the address space"),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let element = decode_enum(wire.element, Some(NdArrayElement::F64), at, "element")?;
    let values = match element {
        NdArrayElement::F64 if wire.u8_values.is_empty() => Values::F64(wire.values),
        NdArrayElement::U8 if wire.values.is_empty() => Values::U8(wire.u8_values),
        NdArrayElement::F64 => {
            return Err(ProtobufError::InvalidValue {
                field: join(at, "u8_values"),
                reason: format!(
                    "the element of the array is f64, whose values belong in `values`, \
                     but `u8_values` holds {} bytes",
                    wire.u8_values.len()
                ),
            });
        }
        NdArrayElement::U8 => {
            return Err(ProtobufError::InvalidValue {
                field: join(at, "values"),
                reason: format!(
                    "the element of the array is u8, whose values belong in `u8_values`, \
                     but `values` holds {} doubles",
                    wire.values.len()
                ),
            });
        }
    };
    Ok(NdArray { shape, values })
}

fn decode_figure_size(wire: w::FigureSize) -> FigureSize {
    let default = FigureSize::default();
    FigureSize {
        width_mm: wire.width_mm.unwrap_or(default.width_mm),
        height_mm: wire.height_mm.unwrap_or(default.height_mm),
    }
}

fn decode_tile_layout(wire: w::TileLayout, at: &str) -> Result<TileLayout> {
    let default = TileLayout::default();
    let sizes = |list: Vec<w::TrackSize>, field: &str| -> Result<Vec<TrackSize>> {
        list.into_iter()
            .enumerate()
            .map(|(i, size)| decode_track_size(size, &join(at, &format!("{field}[{i}]"))))
            .collect()
    };
    Ok(TileLayout {
        rows: wire.rows.unwrap_or(default.rows),
        cols: wire.cols.unwrap_or(default.cols),
        row_weights: sizes(wire.row_weights, "row_weights")?,
        col_weights: sizes(wire.col_weights, "col_weights")?,
    })
}

/// Decodes the size of a row or column, which is a weight of one when unset.
fn decode_track_size(wire: w::TrackSize, at: &str) -> Result<TrackSize> {
    Ok(match wire.kind {
        None => TrackSize::default(),
        Some(w::TrackSizeKind::Weight(weight)) => {
            TrackSize::Weight(required(weight.weight, &join(at, "weight"), "weight")?)
        }
        Some(w::TrackSizeKind::Fit(_)) => TrackSize::Fit,
    })
}

fn decode_cell(wire: w::Cell) -> Cell {
    let default = Cell::default();
    Cell {
        row: wire.row.unwrap_or(default.row),
        col: wire.col.unwrap_or(default.col),
        row_span: wire.row_span.unwrap_or(default.row_span),
        col_span: wire.col_span.unwrap_or(default.col_span),
    }
}

fn decode_view3d(wire: w::View3d) -> View3d {
    let default = View3d::default();
    View3d {
        azimuth_deg: wire.azimuth_deg.unwrap_or(default.azimuth_deg),
        elevation_deg: wire.elevation_deg.unwrap_or(default.elevation_deg),
        zoom: wire.zoom.unwrap_or(default.zoom),
        pan_x: wire.pan_x.unwrap_or(default.pan_x),
        pan_y: wire.pan_y.unwrap_or(default.pan_y),
    }
}

fn decode_legend(wire: w::Legend, at: &str) -> Result<Legend> {
    Ok(match wire.kind {
        None => Legend::default(),
        Some(w::LegendKind::Inside(inside)) => Legend::Inside(match inside.inside {
            None => InsideLegend::default(),
            Some(legend) => decode_inside_legend(legend, &join(at, "inside.inside"))?,
        }),
        Some(w::LegendKind::Panel(panel)) => {
            Legend::Panel(NodeId(required(panel.panel, &join(at, "panel"), "panel")?))
        }
    })
}

fn decode_inside_legend(wire: w::InsideLegend, at: &str) -> Result<InsideLegend> {
    let default = InsideLegend::default();
    Ok(InsideLegend {
        location: decode_enum(wire.location, Some(default.location), at, "location")?,
        boxed: wire.boxed.unwrap_or(default.boxed),
        layout: decode_legend_entry_layout(wire.layout, default.layout, &join(at, "layout"))?,
        entries: decode_legend_entries(wire.entries, &join(at, "entries"))?,
    })
}

/// Decodes a data array, whose identifier and array have no default.
impl Decode for DataArray {
    type Message = w::DataArray;

    fn decode(wire: w::DataArray, at: &str) -> Result<Self> {
        Ok(DataArray {
            id: node_id(wire.id, at)?,
            name: wire.name,
            array: decode_array(required(wire.array, at, "array")?, &join(at, "array"))?,
        })
    }
}

impl Decode for Panel {
    type Message = w::Panel;

    fn decode(wire: w::Panel, at: &str) -> Result<Self> {
        Ok(Panel {
            id: node_id(wire.id, at)?,
            cell: wire.cell.map_or(Cell::default(), decode_cell),
            align: decode_enum(wire.align, Some(PanelLocation::default()), at, "align")?,
            content: decode_panel_content(wire.content, &join(at, "content"))?,
        })
    }
}

/// Decodes the content of a panel, which has no default: a legend is no likelier than
/// any other key.
fn decode_panel_content(wire: Option<w::PanelContent>, at: &str) -> Result<PanelContent> {
    let content = wire.ok_or_else(|| ProtobufError::MissingField {
        field: at.to_owned(),
    })?;
    Ok(match required(content.kind, at, "kind")? {
        w::PanelContentKind::Legend(legend) => {
            let at = join(at, "legend");
            PanelContent::Legend(match legend.legend {
                Some(legend) => decode_panel_legend(legend, &join(&at, "legend"))?,
                None => PanelLegend::default(),
            })
        }
    })
}

/// Decodes the legend content of a panel onto the defaults of a panel's legend.
fn decode_panel_legend(wire: w::PanelLegend, at: &str) -> Result<PanelLegend> {
    let default = PanelLegend::default();
    Ok(PanelLegend {
        boxed: wire.boxed.unwrap_or(default.boxed),
        layout: decode_legend_entry_layout(wire.layout, default.layout, &join(at, "layout"))?,
        entries: decode_legend_entries(wire.entries, &join(at, "entries"))?,
    })
}

/// Decodes the entries of a legend, which are automatic when absent or unset.
fn decode_legend_entries(wire: Option<w::LegendEntries>, at: &str) -> Result<LegendEntries> {
    Ok(match wire.and_then(|entries| entries.kind) {
        None | Some(w::LegendEntriesKind::Auto(_)) => LegendEntries::Auto,
        Some(w::LegendEntriesKind::Explicit(explicit)) => {
            let at = join(at, "explicit_items");
            LegendEntries::Explicit(
                explicit
                    .items
                    .into_iter()
                    .enumerate()
                    .map(|(i, item)| decode_legend_item(item, &join(&at, &format!("items[{i}]"))))
                    .collect::<Result<_>>()?,
            )
        }
    })
}

/// Decodes an item of an explicit list of legend entries, whose content has no default.
fn decode_legend_item(wire: w::LegendItem, at: &str) -> Result<LegendItem> {
    let content = required(wire.content.and_then(|content| content.kind), at, "content")?;
    let content =
        match content {
            w::LegendItemContentKind::Artist(artist) => LegendItemContent::Artist(NodeId(
                required(artist.artist, &join(at, "content.artist"), "artist")?,
            )),
            w::LegendItemContentKind::Custom(custom) => {
                let at = join(at, "content.custom");
                LegendItemContent::Custom {
                    label: custom
                        .label
                        .map(|label| decode_text(label, &join(&at, "label")))
                        .transpose()?
                        .unwrap_or_default(),
                    sample: decode_legend_sample(custom.sample, &at)?,
                }
            }
        };
    Ok(LegendItem {
        content,
        full_row: wire.full_row.unwrap_or(false),
    })
}

/// Decodes the sample of a legend item, which has no default.
fn decode_legend_sample(wire: Option<w::LegendSample>, at: &str) -> Result<LegendSample> {
    let kind = required(wire.and_then(|sample| sample.kind), at, "sample")?;
    let at = join(at, "sample");
    Ok(match kind {
        w::LegendSampleKind::Line(line) => {
            let at = join(&at, "line");
            LegendSample::Line {
                line: decode_line_style(line.line, LineStyle::default(), &join(&at, "line"))?,
                marker: decode_marker_style(
                    line.marker,
                    MarkerStyle::default(),
                    &join(&at, "marker"),
                )?,
            }
        }
        w::LegendSampleKind::Swatch(swatch) => LegendSample::Swatch {
            color: swatch.color.map(decode_color).unwrap_or_default(),
        },
    })
}

/// Decodes the layout of the entries of a legend, whose absent parts take the parts of
/// the default layout of its context.
fn decode_legend_entry_layout(
    wire: Option<w::LegendEntryLayout>,
    default: LegendEntryLayout,
    at: &str,
) -> Result<LegendEntryLayout> {
    let wire = wire.unwrap_or_default();
    Ok(LegendEntryLayout {
        columns: decode_legend_entry_columns(wire.columns, default.columns, &join(at, "columns"))?,
        order: decode_enum(wire.order, Some(default.order), at, "order")?,
    })
}

/// Decodes a number of legend columns; a fixed number has no default.
fn decode_legend_entry_columns(
    wire: Option<w::LegendEntryColumns>,
    default: LegendEntryColumns,
    at: &str,
) -> Result<LegendEntryColumns> {
    Ok(match wire.and_then(|columns| columns.kind) {
        None => default,
        Some(w::LegendEntryColumnsKind::Auto(_)) => LegendEntryColumns::Auto,
        Some(w::LegendEntryColumnsKind::Count(count)) => {
            LegendEntryColumns::Count(required(count.count, &join(at, "count"), "count")?)
        }
    })
}

fn decode_text(wire: w::Text, at: &str) -> Result<Text> {
    Ok(Text {
        content: wire.content,
        interpreter: decode_enum(
            wire.interpreter,
            Some(Interpreter::default()),
            at,
            "interpreter",
        )?,
    })
}

fn decode_label(wire: Option<w::Text>, at: &str) -> Result<Option<Text>> {
    wire.map(|t| decode_text(t, &join(at, "label"))).transpose()
}

fn decode_color(wire: w::Color) -> Color {
    Color::rgba(wire.r, wire.g, wire.b, wire.a)
}

fn decode_color_spec(wire: Option<w::ColorSpec>, default: ColorSpec) -> ColorSpec {
    match wire.and_then(|spec| spec.kind) {
        None => default,
        Some(w::ColorSpecKind::Auto(_)) => ColorSpec::Auto,
        Some(w::ColorSpecKind::Rgba(rgba)) => ColorSpec::Rgba {
            color: rgba.color.map(decode_color).unwrap_or_default(),
        },
        Some(w::ColorSpecKind::None(_)) => ColorSpec::None,
        Some(w::ColorSpecKind::Colormapped(_)) => ColorSpec::Colormapped,
    }
}

fn decode_line_style(
    wire: Option<w::LineStyle>,
    default: LineStyle,
    at: &str,
) -> Result<LineStyle> {
    let wire = wire.unwrap_or_default();
    Ok(LineStyle {
        color: decode_color_spec(wire.color, default.color),
        width_pt: wire.width_pt.unwrap_or(default.width_pt),
        dash: decode_enum(wire.dash, Some(default.dash), at, "dash")?,
    })
}

fn decode_marker_style(
    wire: Option<w::MarkerStyle>,
    default: MarkerStyle,
    at: &str,
) -> Result<MarkerStyle> {
    let wire = wire.unwrap_or_default();
    Ok(MarkerStyle {
        shape: decode_enum(wire.shape, Some(default.shape), at, "shape")?,
        size_pt: wire.size_pt.unwrap_or(default.size_pt),
        face: decode_color_spec(wire.face, default.face),
        edge: decode_color_spec(wire.edge, default.edge),
    })
}

fn decode_limits(wire: Option<w::Limits>, default: Limits, at: &str) -> Result<Limits> {
    Ok(match wire.and_then(|limits| limits.kind) {
        None => default,
        Some(w::LimitsKind::Auto(_)) => Limits::Auto,
        Some(w::LimitsKind::Manual(manual)) => {
            let at = join(at, "manual");
            Limits::Manual {
                min: required(manual.min, &at, "min")?,
                max: required(manual.max, &at, "max")?,
            }
        }
    })
}

fn decode_aspect(wire: Option<w::Aspect>, at: &str) -> Result<Aspect> {
    Ok(match wire.and_then(|aspect| aspect.kind) {
        None => Aspect::default(),
        Some(w::AspectKind::Auto(_)) => Aspect::Auto,
        Some(w::AspectKind::Equal(_)) => Aspect::Equal,
        Some(w::AspectKind::Ratio(ratio)) => {
            let at = join(at, "ratio");
            Aspect::Ratio {
                y: required(ratio.y, &at, "y")?,
                z: required(ratio.z, &at, "z")?,
            }
        }
    })
}

fn decode_ticks(wire: Option<w::Ticks>, at: &str) -> Result<Ticks> {
    Ok(match wire.and_then(|ticks| ticks.kind) {
        None => Ticks::default(),
        Some(w::TicksKind::Auto(_)) => Ticks::Auto,
        Some(w::TicksKind::Step(step)) => {
            let at = join(at, "step");
            Ticks::Step {
                step: required(step.step, &at, "step")?,
                origin: required(step.origin, &at, "origin")?,
            }
        }
        Some(w::TicksKind::Values(values)) => Ticks::Values(values.values),
    })
}

fn decode_tick_labels(wire: Option<w::TickLabels>, at: &str) -> Result<TickLabels> {
    Ok(match wire.and_then(|labels| labels.kind) {
        None => TickLabels::default(),
        Some(w::TickLabelsKind::Auto(_)) => TickLabels::Auto,
        Some(w::TickLabelsKind::Hidden(_)) => TickLabels::Hidden,
        Some(w::TickLabelsKind::Text(text)) => {
            let at = join(at, "text");
            TickLabels::Text(
                text.labels
                    .into_iter()
                    .enumerate()
                    .map(|(i, label)| decode_text(label, &join(&at, &format!("labels[{i}]"))))
                    .collect::<Result<Vec<Text>>>()?,
            )
        }
    })
}

fn decode_axis(wire: Option<w::Axis>, at: &str) -> Result<Axis> {
    let default = Axis::default();
    let wire = wire.unwrap_or_default();
    Ok(Axis {
        label: wire
            .label
            .map(|t| decode_text(t, &join(at, "label")))
            .transpose()?,
        scale: decode_enum(wire.scale, Some(default.scale), at, "scale")?,
        limits: decode_limits(wire.limits, default.limits, &join(at, "limits"))?,
        reverse: wire.reverse.unwrap_or(default.reverse),
        ticks: decode_ticks(wire.ticks, &join(at, "ticks"))?,
        tick_labels: decode_tick_labels(wire.tick_labels, &join(at, "tick_labels"))?,
        grid: wire.grid.unwrap_or(default.grid),
    })
}

fn decode_projection(wire: Option<w::Projection>, default: Projection) -> Projection {
    match wire.and_then(|projection| projection.kind) {
        None => default,
        Some(w::ProjectionKind::TwoD(_)) => Projection::TwoD,
        Some(w::ProjectionKind::ThreeD(three_d)) => Projection::ThreeD {
            view3d: decode_view3d(three_d.view3d.unwrap_or_default()),
        },
    }
}

impl Decode for Axes {
    type Message = w::Axes;

    fn decode(wire: w::Axes, at: &str) -> Result<Self> {
        let default = Axes::default();
        let legend = wire
            .legend
            .map(|legend| decode_legend(legend, &join(at, "legend")))
            .transpose()?;
        let artists = wire
            .artists
            .into_iter()
            .enumerate()
            .map(|(i, artist)| Artist::decode(artist, &format!("{at}.artists[{i}]")))
            .collect::<Result<Vec<_>>>()?;
        Ok(Axes {
            id: node_id(wire.id, at)?,
            cell: wire.cell.map_or(default.cell, decode_cell),
            projection: decode_projection(wire.projection, default.projection),
            title: wire
                .title
                .map(|t| decode_text(t, &join(at, "title")))
                .transpose()?,
            title_align: decode_enum(
                wire.title_align,
                Some(default.title_align),
                at,
                "title_align",
            )?,
            aspect: decode_aspect(wire.aspect, &join(at, "aspect"))?,
            aspect_adjust: decode_enum(
                wire.aspect_adjust,
                Some(default.aspect_adjust),
                at,
                "aspect_adjust",
            )?,
            x: decode_axis(wire.x, &join(at, "x"))?,
            y: decode_axis(wire.y, &join(at, "y"))?,
            z: decode_axis(wire.z, &join(at, "z"))?,
            box_: wire.r#box.unwrap_or(default.box_),
            colormap: decode_enum(wire.colormap, Some(default.colormap), at, "colormap")?,
            clim: decode_limits(wire.clim, default.clim, &join(at, "clim"))?,
            legend,
            artists,
        })
    }
}

impl Decode for Line {
    type Message = w::Line;

    fn decode(wire: w::Line, at: &str) -> Result<Self> {
        let default = Line::default();
        Ok(Line {
            id: node_id(wire.id, at)?,
            label: decode_label(wire.label, at)?,
            visible: wire.visible.unwrap_or(default.visible),
            x: reference(wire.x, at, "x")?,
            y: reference(wire.y, at, "y")?,
            z: wire.z.map(NodeId),
            line: decode_line_style(wire.line, default.line, &join(at, "line"))?,
            marker: decode_marker_style(wire.marker, default.marker, &join(at, "marker"))?,
            x_coords: decode_enum(wire.x_coords, Some(default.x_coords), at, "x_coords")?,
            y_coords: decode_enum(wire.y_coords, Some(default.y_coords), at, "y_coords")?,
            clip: wire.clip.unwrap_or(default.clip),
        })
    }
}

/// Decodes the size of scatter markers; `at` is the path of the size itself.
fn decode_scatter_size(wire: Option<w::ScatterSize>, at: &str) -> Result<ScatterSize> {
    Ok(match wire.and_then(|size| size.kind) {
        None => ScatterSize::default(),
        Some(w::ScatterSizeKind::Scalar(scalar)) => {
            let ScatterSize::Scalar { value } = ScatterSize::default() else {
                unreachable!("the default scatter size is a scalar");
            };
            ScatterSize::Scalar {
                value: scalar.value.unwrap_or(value),
            }
        }
        Some(w::ScatterSizeKind::Data(data)) => ScatterSize::Data {
            data: reference(data.data, at, "data.data")?,
        },
    })
}

/// Decodes the colour of scatter markers; `at` is the path of the colour itself.
fn decode_scatter_color(wire: Option<w::ScatterColor>, at: &str) -> Result<ScatterColor> {
    Ok(match wire.and_then(|color| color.kind) {
        None => ScatterColor::default(),
        Some(w::ScatterColorKind::Spec(spec)) => {
            let ScatterColor::Spec { spec: default_spec } = ScatterColor::default() else {
                unreachable!("the default scatter colour is a colour specification");
            };
            ScatterColor::Spec {
                spec: decode_color_spec(spec.spec, default_spec),
            }
        }
        Some(w::ScatterColorKind::Data(data)) => ScatterColor::Data {
            data: reference(data.data, at, "data.data")?,
        },
    })
}

/// Decodes the levels of a contour.
fn decode_levels(wire: Option<w::Levels>) -> Levels {
    match wire.and_then(|levels| levels.kind) {
        None => Levels::default(),
        Some(w::LevelsKind::Auto(auto)) => {
            let Levels::Auto { count } = Levels::default() else {
                unreachable!("the default levels are automatic");
            };
            Levels::Auto {
                count: auto.count.unwrap_or(count),
            }
        }
        Some(w::LevelsKind::Explicit(explicit)) => Levels::Explicit {
            values: explicit.values,
        },
    }
}

/// Decodes the placement of a contour.
fn decode_contour_placement(wire: Option<w::ContourPlacement>) -> ContourPlacement {
    match wire.and_then(|placement| placement.kind) {
        None => ContourPlacement::default(),
        Some(w::ContourPlacementKind::Plane(plane)) => ContourPlacement::Plane { z: plane.z },
        Some(w::ContourPlacementKind::AtLevel(_)) => ContourPlacement::AtLevel,
    }
}

/// Decodes the scaling of quiver arrows; `at` is the path of the scaling itself.
fn decode_quiver_scale(wire: Option<w::QuiverScale>, at: &str) -> Result<QuiverScale> {
    Ok(match wire.and_then(|scale| scale.kind) {
        None => QuiverScale::default(),
        Some(w::QuiverScaleKind::Auto(_)) => QuiverScale::Auto,
        Some(w::QuiverScaleKind::Factor(factor)) => QuiverScale::Factor {
            value: required(factor.value, at, "factor.value")?,
        },
        Some(w::QuiverScaleKind::Off(_)) => QuiverScale::Off,
    })
}

impl Decode for Scatter {
    type Message = w::Scatter;

    fn decode(wire: w::Scatter, at: &str) -> Result<Self> {
        let default = Scatter::default();
        let size = decode_scatter_size(wire.size, &join(at, "size"))?;
        let color = decode_scatter_color(wire.color, &join(at, "color"))?;
        Ok(Scatter {
            id: node_id(wire.id, at)?,
            label: decode_label(wire.label, at)?,
            visible: wire.visible.unwrap_or(default.visible),
            x: reference(wire.x, at, "x")?,
            y: reference(wire.y, at, "y")?,
            z: wire.z.map(NodeId),
            size,
            color,
            marker: decode_marker_style(wire.marker, default.marker, &join(at, "marker"))?,
            x_coords: decode_enum(wire.x_coords, Some(default.x_coords), at, "x_coords")?,
            y_coords: decode_enum(wire.y_coords, Some(default.y_coords), at, "y_coords")?,
            clip: wire.clip.unwrap_or(default.clip),
        })
    }
}

/// Decodes the grid of a contour or surface, which has no default; `at` is the path of
/// the grid itself.
fn decode_grid(wire: Option<w::Grid>, at: &str) -> Result<Grid> {
    let wire = wire.ok_or_else(|| ProtobufError::MissingField {
        field: at.to_owned(),
    })?;
    Ok(match required(wire.kind, at, "kind")? {
        w::GridKind::Rectilinear(grid) => {
            let at = join(at, "rectilinear");
            Grid::Rectilinear {
                x: reference(grid.x, &at, "x")?,
                y: reference(grid.y, &at, "y")?,
            }
        }
        w::GridKind::Curvilinear(grid) => {
            let at = join(at, "curvilinear");
            Grid::Curvilinear {
                x: reference(grid.x, &at, "x")?,
                y: reference(grid.y, &at, "y")?,
            }
        }
    })
}

impl Decode for Contour {
    type Message = w::Contour;

    fn decode(wire: w::Contour, at: &str) -> Result<Self> {
        let default = Contour::default();
        let levels = decode_levels(wire.levels);
        let placement = decode_contour_placement(wire.placement);
        Ok(Contour {
            id: node_id(wire.id, at)?,
            label: decode_label(wire.label, at)?,
            visible: wire.visible.unwrap_or(default.visible),
            grid: decode_grid(wire.grid, &join(at, "grid"))?,
            z: reference(wire.z, at, "z")?,
            levels,
            fill: wire.fill.unwrap_or(default.fill),
            placement,
            line: decode_line_style(wire.line, default.line, &join(at, "line"))?,
            line_opacity: wire.line_opacity.unwrap_or(default.line_opacity),
            fill_opacity: wire.fill_opacity.unwrap_or(default.fill_opacity),
        })
    }
}

impl Decode for Quiver {
    type Message = w::Quiver;

    fn decode(wire: w::Quiver, at: &str) -> Result<Self> {
        let default = Quiver::default();
        let scale = decode_quiver_scale(wire.scale, &join(at, "scale"))?;
        Ok(Quiver {
            id: node_id(wire.id, at)?,
            label: decode_label(wire.label, at)?,
            visible: wire.visible.unwrap_or(default.visible),
            x: reference(wire.x, at, "x")?,
            y: reference(wire.y, at, "y")?,
            z: wire.z.map(NodeId),
            u: reference(wire.u, at, "u")?,
            v: reference(wire.v, at, "v")?,
            w: wire.w.map(NodeId),
            scale,
            line: decode_line_style(wire.line, default.line, &join(at, "line"))?,
            head_size: wire.head_size.unwrap_or(default.head_size),
        })
    }
}

impl Decode for Surface {
    type Message = w::Surface;

    fn decode(wire: w::Surface, at: &str) -> Result<Self> {
        let default = Surface::default();
        Ok(Surface {
            id: node_id(wire.id, at)?,
            label: decode_label(wire.label, at)?,
            visible: wire.visible.unwrap_or(default.visible),
            grid: decode_grid(wire.grid, &join(at, "grid"))?,
            z: reference(wire.z, at, "z")?,
            c: wire.c.map(NodeId),
            face: decode_color_spec(wire.face, default.face),
            edge: decode_color_spec(wire.edge, default.edge),
            edge_width_pt: wire.edge_width_pt.unwrap_or(default.edge_width_pt),
            face_alpha: wire.face_alpha.map(NodeId),
            edge_alpha: wire.edge_alpha.map(NodeId),
            face_opacity: wire.face_opacity.unwrap_or(default.face_opacity),
            edge_opacity: wire.edge_opacity.unwrap_or(default.edge_opacity),
        })
    }
}

/// Decodes the centres of the first and last pixels of an image along an axis, neither
/// of which has a default, because one centre alone places nothing; `at` is the path of
/// the range itself.
fn decode_pixel_range(wire: w::PixelRange, at: &str) -> Result<PixelRange> {
    Ok(PixelRange {
        first: required(wire.first, at, "first")?,
        last: required(wire.last, at, "last")?,
    })
}

/// Decodes the plane of an image, taking an unset plane as the default and an absent
/// offset as the low end of the third axis.
fn decode_image_plane(wire: Option<w::ImagePlane>) -> ImagePlane {
    match wire.and_then(|plane| plane.kind) {
        None => ImagePlane::default(),
        Some(w::ImagePlaneKind::Xy(xy)) => ImagePlane::Xy { z: xy.z },
        Some(w::ImagePlaneKind::Xz(xz)) => ImagePlane::Xz { y: xz.y },
        Some(w::ImagePlaneKind::Yz(yz)) => ImagePlane::Yz { x: yz.x },
    }
}

/// Decodes the placement of an image, taking an absent placement as the default and an
/// absent pixel range as absent; `at` is the path of the placement itself.
fn decode_image_placement(wire: Option<w::ImagePlacement>, at: &str) -> Result<ImagePlacement> {
    let wire = wire.unwrap_or_default();
    Ok(ImagePlacement {
        plane: decode_image_plane(wire.plane),
        columns: wire
            .columns
            .map(|range| decode_pixel_range(range, &join(at, "columns")))
            .transpose()?,
        rows: wire
            .rows
            .map(|range| decode_pixel_range(range, &join(at, "rows")))
            .transpose()?,
    })
}

/// Decodes an out-of-range policy, taking an unset policy as the default and a fixed
/// colour without a colour as black, as a fixed colour specification is taken.
fn decode_out_of_range(wire: Option<w::OutOfRange>) -> OutOfRange {
    match wire.and_then(|policy| policy.kind) {
        None => OutOfRange::default(),
        Some(w::OutOfRangeKind::Strict(_)) => OutOfRange::Strict,
        Some(w::OutOfRangeKind::Transparent(_)) => OutOfRange::Transparent,
        Some(w::OutOfRangeKind::Clamp(_)) => OutOfRange::Clamp,
        Some(w::OutOfRangeKind::Rgba(rgba)) => OutOfRange::Rgba {
            color: rgba.color.map(decode_color).unwrap_or_default(),
        },
    }
}

impl Decode for Image {
    type Message = w::Image;

    fn decode(wire: w::Image, at: &str) -> Result<Self> {
        let default = Image::default();
        Ok(Image {
            id: node_id(wire.id, at)?,
            label: decode_label(wire.label, at)?,
            visible: wire.visible.unwrap_or(default.visible),
            pixels: reference(wire.pixels, at, "pixels")?,
            placement: decode_image_placement(wire.placement, &join(at, "placement"))?,
            opacity: wire.opacity.unwrap_or(default.opacity),
        })
    }
}

impl Decode for IndexedImage {
    type Message = w::IndexedImage;

    fn decode(wire: w::IndexedImage, at: &str) -> Result<Self> {
        let default = IndexedImage::default();
        Ok(IndexedImage {
            id: node_id(wire.id, at)?,
            label: decode_label(wire.label, at)?,
            visible: wire.visible.unwrap_or(default.visible),
            indices: reference(wire.indices, at, "indices")?,
            placement: decode_image_placement(wire.placement, &join(at, "placement"))?,
            below: decode_out_of_range(wire.below),
            above: decode_out_of_range(wire.above),
            non_finite: decode_out_of_range(wire.non_finite),
            alpha: wire.alpha.map(NodeId),
            opacity: wire.opacity.unwrap_or(default.opacity),
        })
    }
}

impl Decode for MappedImage {
    type Message = w::MappedImage;

    fn decode(wire: w::MappedImage, at: &str) -> Result<Self> {
        let default = MappedImage::default();
        Ok(MappedImage {
            id: node_id(wire.id, at)?,
            label: decode_label(wire.label, at)?,
            visible: wire.visible.unwrap_or(default.visible),
            values: reference(wire.values, at, "values")?,
            placement: decode_image_placement(wire.placement, &join(at, "placement"))?,
            below: decode_out_of_range(wire.below),
            above: decode_out_of_range(wire.above),
            non_finite: decode_out_of_range(wire.non_finite),
            alpha: wire.alpha.map(NodeId),
            opacity: wire.opacity.unwrap_or(default.opacity),
        })
    }
}

// ---------------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------------

impl From<&Transaction> for w::Transaction {
    /// Encodes a transaction, writing every field that has a value.
    fn from(transaction: &Transaction) -> Self {
        w::Transaction {
            edits: transaction.edits.iter().map(encode_edit).collect(),
        }
    }
}

fn encode_edit(edit: &Edit) -> w::Edit {
    let kind = match edit {
        Edit::Set { node, path, value } => w::EditKind::Set(w::EditSet {
            node: Some(node.0),
            path: path.to_string(),
            value: Some(encode_value(value)),
        }),
        Edit::Insert {
            parent,
            index,
            node,
        } => w::EditKind::Insert(w::EditInsert {
            parent: Some(parent.0),
            index: *index,
            node: Some(node.encode()),
        }),
        Edit::Remove { node } => w::EditKind::Remove(w::EditRemove { node: Some(node.0) }),
        Edit::Move {
            node,
            parent,
            index,
        } => w::EditKind::Move(w::EditMove {
            node: Some(node.0),
            parent: Some(parent.0),
            index: *index,
        }),
        Edit::ReplaceData { node, array } => w::EditKind::ReplaceData(w::EditReplaceData {
            node: Some(node.0),
            array: Some(encode_array(array)),
        }),
        Edit::AppendData {
            node,
            array,
            retain,
        } => w::EditKind::AppendData(w::EditAppendData {
            node: Some(node.0),
            array: Some(encode_array(array)),
            retain: *retain,
        }),
    };
    w::Edit { kind: Some(kind) }
}

fn encode_value(value: &Value) -> w::Value {
    let kind = match value {
        Value::Unset => w::ValueKind::Unset(w::ValueUnset {}),
        Value::Bool(v) => w::ValueKind::Bool(w::ValueBool { value: Some(*v) }),
        Value::UInt32(v) => w::ValueKind::Uint32(w::ValueUint32 { value: Some(*v) }),
        Value::Double(v) => w::ValueKind::Double(w::ValueDouble { value: Some(*v) }),
        Value::Float(v) => w::ValueKind::Float(w::ValueFloat { value: Some(*v) }),
        Value::String(v) => w::ValueKind::String(w::ValueString { value: v.clone() }),
        Value::NodeId(v) => w::ValueKind::NodeId(w::ValueNodeId { value: Some(v.0) }),
        Value::Doubles(v) => w::ValueKind::Doubles(w::ValueDoubles { values: v.clone() }),
        Value::TrackSizes(v) => w::ValueKind::TrackSizes(w::ValueTrackSizes {
            sizes: v.iter().map(|&size| encode_track_size(size)).collect(),
        }),
        Value::Strings(v) => w::ValueKind::Strings(w::ValueStrings { values: v.clone() }),
        Value::Texts(v) => w::ValueKind::Texts(w::ValueTexts {
            values: v.iter().map(encode_text).collect(),
        }),
        Value::Text(v) => w::ValueKind::Text(w::ValueText {
            value: Some(encode_text(v)),
        }),
        Value::Interpreter(v) => {
            w::ValueKind::Interpreter(w::ValueInterpreter { value: v.to_wire() })
        }
        Value::FigureSize(v) => w::ValueKind::FigureSize(w::ValueFigureSize {
            value: Some(encode_figure_size(*v)),
        }),
        Value::FontSetId(v) => w::ValueKind::FontSetId(w::ValueFontSetId { value: v.to_wire() }),
        Value::Color(v) => w::ValueKind::Color(w::ValueColor {
            value: Some(encode_color(*v)),
        }),
        Value::TileLayout(v) => w::ValueKind::TileLayout(w::ValueTileLayout {
            value: Some(encode_tile_layout(v)),
        }),
        Value::Links(v) => w::ValueKind::Links(w::ValueLinks {
            links: encode_links(v),
        }),
        Value::Parameters(v) => w::ValueKind::Parameters(w::ValueParameters {
            parameters: v
                .iter()
                .map(|(name, parameter)| (name.clone(), encode_parameter(parameter)))
                .collect(),
        }),
        Value::Cell(v) => w::ValueKind::Cell(w::ValueCell {
            value: Some(encode_cell(*v)),
        }),
        Value::Projection(v) => w::ValueKind::Projection(w::ValueProjection {
            value: Some(encode_projection(*v)),
        }),
        Value::View3d(v) => w::ValueKind::View3d(w::ValueView3d {
            value: Some(encode_view3d(*v)),
        }),
        Value::Axis(v) => w::ValueKind::Axis(w::ValueAxis {
            value: Some(encode_axis(v)),
        }),
        Value::Scale(v) => w::ValueKind::Scale(w::ValueScale { value: v.to_wire() }),
        Value::Limits(v) => w::ValueKind::Limits(w::ValueLimits {
            value: Some(encode_limits(*v)),
        }),
        Value::ColormapName(v) => {
            w::ValueKind::ColormapName(w::ValueColormapName { value: v.to_wire() })
        }
        Value::Legend(v) => w::ValueKind::Legend(w::ValueLegend {
            value: Some(encode_legend(v)),
        }),
        Value::InsideLegend(v) => w::ValueKind::InsideLegend(w::ValueInsideLegend {
            value: Some(encode_inside_legend(v)),
        }),
        Value::LegendLocation(v) => {
            w::ValueKind::LegendLocation(w::ValueLegendLocation { value: v.to_wire() })
        }
        Value::PanelLocation(v) => {
            w::ValueKind::PanelLocation(w::ValuePanelLocation { value: v.to_wire() })
        }
        Value::PanelContent(v) => w::ValueKind::PanelContent(w::ValuePanelContent {
            value: Some(encode_panel_content(v)),
        }),
        Value::PanelLegend(v) => w::ValueKind::PanelLegend(w::ValuePanelLegend {
            value: Some(encode_panel_legend(v)),
        }),
        Value::LegendEntryLayout(v) => w::ValueKind::LegendEntryLayout(w::ValueLegendEntryLayout {
            value: Some(encode_legend_entry_layout(*v)),
        }),
        Value::LegendEntryColumns(v) => {
            w::ValueKind::LegendEntryColumns(w::ValueLegendEntryColumns {
                value: Some(encode_legend_entry_columns(*v)),
            })
        }
        Value::LegendEntryOrder(v) => {
            w::ValueKind::LegendEntryOrder(w::ValueLegendEntryOrder { value: v.to_wire() })
        }
        Value::Aspect(v) => w::ValueKind::Aspect(w::ValueAspect {
            value: Some(encode_aspect(*v)),
        }),
        Value::AspectAdjust(v) => {
            w::ValueKind::AspectAdjust(w::ValueAspectAdjust { value: v.to_wire() })
        }
        Value::TitleAlign(v) => w::ValueKind::TitleAlign(w::ValueTitleAlign { value: v.to_wire() }),
        Value::Ticks(v) => w::ValueKind::Ticks(w::ValueTicks {
            value: Some(encode_ticks(v)),
        }),
        Value::TickLabels(v) => w::ValueKind::TickLabels(w::ValueTickLabels {
            value: Some(encode_tick_labels(v)),
        }),
        Value::ColorSpec(v) => w::ValueKind::ColorSpec(w::ValueColorSpec {
            value: Some(encode_color_spec(*v)),
        }),
        Value::LineStyle(v) => w::ValueKind::LineStyle(w::ValueLineStyle {
            value: Some(encode_line_style(*v)),
        }),
        Value::DashStyle(v) => w::ValueKind::DashStyle(w::ValueDashStyle { value: v.to_wire() }),
        Value::Coords(v) => w::ValueKind::Coords(w::ValueCoords { value: v.to_wire() }),
        Value::MarkerStyle(v) => w::ValueKind::MarkerStyle(w::ValueMarkerStyle {
            value: Some(encode_marker_style(*v)),
        }),
        Value::MarkerShape(v) => {
            w::ValueKind::MarkerShape(w::ValueMarkerShape { value: v.to_wire() })
        }
        Value::ScatterSize(v) => w::ValueKind::ScatterSize(w::ValueScatterSize {
            value: Some(encode_scatter_size(*v)),
        }),
        Value::ScatterColor(v) => w::ValueKind::ScatterColor(w::ValueScatterColor {
            value: Some(encode_scatter_color(*v)),
        }),
        Value::Grid(v) => w::ValueKind::Grid(w::ValueGrid {
            value: Some(encode_grid(*v)),
        }),
        Value::Levels(v) => w::ValueKind::Levels(w::ValueLevels {
            value: Some(encode_levels(v)),
        }),
        Value::ContourPlacement(v) => w::ValueKind::ContourPlacement(w::ValueContourPlacement {
            value: Some(encode_contour_placement(*v)),
        }),
        Value::QuiverScale(v) => w::ValueKind::QuiverScale(w::ValueQuiverScale {
            value: Some(encode_quiver_scale(*v)),
        }),
        Value::ImagePlacement(v) => w::ValueKind::ImagePlacement(w::ValueImagePlacement {
            value: Some(encode_image_placement(*v)),
        }),
        Value::PixelRange(v) => w::ValueKind::PixelRange(w::ValuePixelRange {
            value: Some(encode_pixel_range(*v)),
        }),
        Value::ImagePlane(v) => w::ValueKind::ImagePlane(w::ValueImagePlane {
            value: Some(encode_image_plane(*v)),
        }),
        Value::OutOfRange(v) => w::ValueKind::OutOfRange(w::ValueOutOfRange {
            value: Some(encode_out_of_range(*v)),
        }),
    };
    w::Value { kind: Some(kind) }
}

impl TryFrom<w::Transaction> for Transaction {
    type Error = IrError;

    /// Decodes a transaction. Error paths start at the transaction, such as
    /// `edits[0].set_property.value.kind`.
    fn try_from(wire: w::Transaction) -> std::result::Result<Self, Self::Error> {
        let edits = wire
            .edits
            .into_iter()
            .enumerate()
            .map(|(i, edit)| decode_edit(edit, &format!("edits[{i}]")))
            .collect::<Result<Vec<_>>>()?;
        Ok(Transaction { edits })
    }
}

fn decode_edit(wire: w::Edit, at: &str) -> Result<Edit> {
    Ok(match required(wire.kind, at, "kind")? {
        w::EditKind::Set(set) => {
            let at = join(at, "set_property");
            let node = NodeId(required(set.node, &at, "node")?);
            let path = set
                .path
                .parse()
                .map_err(|reason| ProtobufError::InvalidValue {
                    field: join(&at, "path"),
                    reason: format!("{reason}"),
                })?;
            let value = required(set.value, &at, "value")?;
            Edit::Set {
                node,
                path,
                value: decode_value(value, &join(&at, "value"))?,
            }
        }
        w::EditKind::Insert(insert) => {
            let at = join(at, "insert_node");
            Edit::Insert {
                parent: NodeId(required(insert.parent, &at, "parent")?),
                index: insert.index,
                node: Node::decode_field(required(insert.node, &at, "node")?, &at, "node")?,
            }
        }
        w::EditKind::Remove(remove) => Edit::Remove {
            node: NodeId(required(remove.node, &join(at, "remove_node"), "node")?),
        },
        w::EditKind::Move(move_node) => {
            let at = join(at, "move_node");
            Edit::Move {
                node: NodeId(required(move_node.node, &at, "node")?),
                parent: NodeId(required(move_node.parent, &at, "parent")?),
                index: move_node.index,
            }
        }
        w::EditKind::ReplaceData(replace) => {
            let at = join(at, "replace_data");
            Edit::ReplaceData {
                node: NodeId(required(replace.node, &at, "node")?),
                array: decode_array(required(replace.array, &at, "array")?, &join(&at, "array"))?,
            }
        }
        w::EditKind::AppendData(append) => {
            let at = join(at, "append_data");
            Edit::AppendData {
                node: NodeId(required(append.node, &at, "node")?),
                array: decode_array(required(append.array, &at, "array")?, &join(&at, "array"))?,
                retain: append.retain,
            }
        }
    })
}

/// Returns the message held by a variant of a value message, with the path of that
/// message for the errors of the values within it.
fn value_message<T>(value: Option<T>, at: &str, variant: &str) -> Result<(T, String)> {
    let at = join(at, variant);
    let message = required(value, &at, "value")?;
    let within = join(&at, "value");
    Ok((message, within))
}

/// Decodes the value of a set. A value has no enclosing node to give it a context, so
/// the message that a value holds must be present and its enum value specified, while
/// the values within that message take the defaults of their types.
fn decode_value(wire: w::Value, at: &str) -> Result<Value> {
    Ok(match required(wire.kind, at, "kind")? {
        w::ValueKind::Unset(_) => Value::Unset,
        w::ValueKind::Bool(v) => Value::Bool(required(v.value, &join(at, "bool_value"), "value")?),
        w::ValueKind::Uint32(v) => {
            Value::UInt32(required(v.value, &join(at, "uint32_value"), "value")?)
        }
        w::ValueKind::Double(v) => {
            Value::Double(required(v.value, &join(at, "double_value"), "value")?)
        }
        w::ValueKind::Float(v) => {
            Value::Float(required(v.value, &join(at, "float_value"), "value")?)
        }
        w::ValueKind::String(v) => Value::String(v.value),
        w::ValueKind::Doubles(v) => Value::Doubles(v.values),
        w::ValueKind::TrackSizes(v) => {
            let at = join(at, "track_sizes_value");
            Value::TrackSizes(
                v.sizes
                    .into_iter()
                    .enumerate()
                    .map(|(i, size)| decode_track_size(size, &join(&at, &format!("sizes[{i}]"))))
                    .collect::<Result<_>>()?,
            )
        }
        w::ValueKind::Strings(v) => Value::Strings(v.values),
        w::ValueKind::Texts(v) => {
            let at = join(at, "texts_value");
            Value::Texts(
                v.values
                    .into_iter()
                    .enumerate()
                    .map(|(i, text)| decode_text(text, &join(&at, &format!("values[{i}]"))))
                    .collect::<Result<Vec<Text>>>()?,
            )
        }
        w::ValueKind::NodeId(v) => Value::NodeId(NodeId(required(
            v.value,
            &join(at, "node_id_value"),
            "value",
        )?)),
        w::ValueKind::Text(v) => {
            let (text, at) = value_message(v.value, at, "text_value")?;
            Value::Text(decode_text(text, &at)?)
        }
        w::ValueKind::Interpreter(v) => Value::Interpreter(decode_enum(
            v.value,
            None,
            &join(at, "interpreter_value"),
            "value",
        )?),
        w::ValueKind::FigureSize(v) => {
            let (size, _) = value_message(v.value, at, "figure_size_value")?;
            Value::FigureSize(decode_figure_size(size))
        }
        w::ValueKind::FontSetId(v) => Value::FontSetId(decode_enum(
            v.value,
            None,
            &join(at, "font_set_id_value"),
            "value",
        )?),
        w::ValueKind::Color(v) => {
            let (color, _) = value_message(v.value, at, "color_value")?;
            Value::Color(decode_color(color))
        }
        w::ValueKind::TileLayout(v) => {
            let (layout, _) = value_message(v.value, at, "tile_layout_value")?;
            Value::TileLayout(decode_tile_layout(layout, &join(at, "tile_layout_value"))?)
        }
        w::ValueKind::Links(v) => {
            let at = join(at, "links_value");
            Value::Links(decode_links(v.links, &at)?)
        }
        w::ValueKind::Parameters(v) => {
            let at = join(at, "parameters_value");
            Value::Parameters(
                v.parameters
                    .into_iter()
                    .map(|(name, parameter)| {
                        let at = format!("{at}.parameters[{name:?}]");
                        Ok((name, decode_parameter(parameter, &at)?))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()?,
            )
        }
        w::ValueKind::Cell(v) => {
            let (cell, _) = value_message(v.value, at, "cell_value")?;
            Value::Cell(decode_cell(cell))
        }
        w::ValueKind::Projection(v) => {
            let (projection, _) = value_message(v.value, at, "projection_value")?;
            Value::Projection(decode_projection(Some(projection), Projection::default()))
        }
        w::ValueKind::View3d(v) => {
            let (view, _) = value_message(v.value, at, "view3d_value")?;
            Value::View3d(decode_view3d(view))
        }
        w::ValueKind::Axis(v) => {
            let (axis, at) = value_message(v.value, at, "axis_value")?;
            Value::Axis(decode_axis(Some(axis), &at)?)
        }
        w::ValueKind::Scale(v) => Value::Scale(decode_enum(
            v.value,
            None,
            &join(at, "scale_value"),
            "value",
        )?),
        w::ValueKind::Limits(v) => {
            let (limits, at) = value_message(v.value, at, "limits_value")?;
            Value::Limits(decode_limits(Some(limits), Limits::default(), &at)?)
        }
        w::ValueKind::ColormapName(v) => Value::ColormapName(decode_enum(
            v.value,
            None,
            &join(at, "colormap_name_value"),
            "value",
        )?),
        w::ValueKind::Legend(v) => {
            let (legend, at) = value_message(v.value, at, "legend_value")?;
            Value::Legend(decode_legend(legend, &at)?)
        }
        w::ValueKind::LegendLocation(v) => Value::LegendLocation(decode_enum(
            v.value,
            None,
            &join(at, "legend_location_value"),
            "value",
        )?),
        w::ValueKind::PanelLocation(v) => Value::PanelLocation(decode_enum(
            v.value,
            None,
            &join(at, "panel_location_value"),
            "value",
        )?),
        w::ValueKind::PanelContent(v) => {
            let at = join(at, "panel_content_value");
            Value::PanelContent(decode_panel_content(v.value, &join(&at, "value"))?)
        }
        w::ValueKind::PanelLegend(v) => {
            let (legend, at) = value_message(v.value, at, "panel_legend_value")?;
            Value::PanelLegend(decode_panel_legend(legend, &at)?)
        }
        w::ValueKind::InsideLegend(v) => {
            let (legend, at) = value_message(v.value, at, "inside_legend_value")?;
            Value::InsideLegend(decode_inside_legend(legend, &at)?)
        }
        w::ValueKind::LegendEntryLayout(v) => {
            let (layout, at) = value_message(v.value, at, "legend_entry_layout_value")?;
            Value::LegendEntryLayout(decode_legend_entry_layout(
                Some(layout),
                LegendEntryLayout::default(),
                &at,
            )?)
        }
        w::ValueKind::LegendEntryColumns(v) => {
            let (columns, at) = value_message(v.value, at, "legend_entry_columns_value")?;
            Value::LegendEntryColumns(decode_legend_entry_columns(
                Some(columns),
                LegendEntryLayout::default().columns,
                &at,
            )?)
        }
        w::ValueKind::LegendEntryOrder(v) => Value::LegendEntryOrder(decode_enum(
            v.value,
            None,
            &join(at, "legend_entry_order_value"),
            "value",
        )?),
        w::ValueKind::Aspect(v) => {
            let (aspect, at) = value_message(v.value, at, "aspect_value")?;
            Value::Aspect(decode_aspect(Some(aspect), &at)?)
        }
        w::ValueKind::AspectAdjust(v) => Value::AspectAdjust(decode_enum(
            v.value,
            None,
            &join(at, "aspect_adjust_value"),
            "value",
        )?),
        w::ValueKind::TitleAlign(v) => Value::TitleAlign(decode_enum(
            v.value,
            None,
            &join(at, "title_align_value"),
            "value",
        )?),
        w::ValueKind::Ticks(v) => {
            let (ticks, at) = value_message(v.value, at, "ticks_value")?;
            Value::Ticks(decode_ticks(Some(ticks), &at)?)
        }
        w::ValueKind::TickLabels(v) => {
            let (labels, at) = value_message(v.value, at, "tick_labels_value")?;
            Value::TickLabels(decode_tick_labels(Some(labels), &at)?)
        }
        w::ValueKind::ColorSpec(v) => {
            let (spec, _) = value_message(v.value, at, "color_spec_value")?;
            Value::ColorSpec(decode_color_spec(Some(spec), ColorSpec::default()))
        }
        w::ValueKind::LineStyle(v) => {
            let (style, at) = value_message(v.value, at, "line_style_value")?;
            Value::LineStyle(decode_line_style(Some(style), LineStyle::default(), &at)?)
        }
        w::ValueKind::DashStyle(v) => Value::DashStyle(decode_enum(
            v.value,
            None,
            &join(at, "dash_style_value"),
            "value",
        )?),
        w::ValueKind::Coords(v) => Value::Coords(decode_enum(
            v.value,
            None,
            &join(at, "coords_value"),
            "value",
        )?),
        w::ValueKind::MarkerStyle(v) => {
            let (style, at) = value_message(v.value, at, "marker_style_value")?;
            Value::MarkerStyle(decode_marker_style(
                Some(style),
                MarkerStyle::default(),
                &at,
            )?)
        }
        w::ValueKind::MarkerShape(v) => Value::MarkerShape(decode_enum(
            v.value,
            None,
            &join(at, "marker_shape_value"),
            "value",
        )?),
        w::ValueKind::ScatterSize(v) => {
            let (size, at) = value_message(v.value, at, "scatter_size_value")?;
            Value::ScatterSize(decode_scatter_size(Some(size), &at)?)
        }
        w::ValueKind::ScatterColor(v) => {
            let (color, at) = value_message(v.value, at, "scatter_color_value")?;
            Value::ScatterColor(decode_scatter_color(Some(color), &at)?)
        }
        w::ValueKind::Grid(v) => {
            let (grid, at) = value_message(v.value, at, "grid_value")?;
            Value::Grid(decode_grid(Some(grid), &at)?)
        }
        w::ValueKind::Levels(v) => {
            let (levels, _) = value_message(v.value, at, "levels_value")?;
            Value::Levels(decode_levels(Some(levels)))
        }
        w::ValueKind::ContourPlacement(v) => {
            let (placement, _) = value_message(v.value, at, "contour_placement_value")?;
            Value::ContourPlacement(decode_contour_placement(Some(placement)))
        }
        w::ValueKind::QuiverScale(v) => {
            let (scale, at) = value_message(v.value, at, "quiver_scale_value")?;
            Value::QuiverScale(decode_quiver_scale(Some(scale), &at)?)
        }
        w::ValueKind::ImagePlacement(v) => {
            let (placement, at) = value_message(v.value, at, "image_placement_value")?;
            Value::ImagePlacement(decode_image_placement(Some(placement), &at)?)
        }
        w::ValueKind::PixelRange(v) => {
            let (range, at) = value_message(v.value, at, "pixel_range_value")?;
            Value::PixelRange(decode_pixel_range(range, &at)?)
        }
        w::ValueKind::ImagePlane(v) => {
            let (plane, _) = value_message(v.value, at, "image_plane_value")?;
            Value::ImagePlane(decode_image_plane(Some(plane)))
        }
        w::ValueKind::OutOfRange(v) => {
            let (policy, _) = value_message(v.value, at, "out_of_range_value")?;
            Value::OutOfRange(decode_out_of_range(Some(policy)))
        }
    })
}
