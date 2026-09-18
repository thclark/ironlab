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
    Artist, Axes, Axis, AxisLink, Cell, Color, ColorSpec, ColormapName, Contour, ContourPlacement,
    DashStyle, DataId, Dimension, Edit, Figure, FigureSize, FontSetId, Grid, Interpreter, Legend,
    LegendLocation, Levels, Limits, Line, LineStyle, MarkerShape, MarkerStyle, NdArray, Node,
    NodeId, Parameter, Projection, Provenance, Quiver, QuiverScale, Scale, Scatter, ScatterColor,
    ScatterSize, Surface, Text, TileLayout, Transaction, Value, View3d,
};

type Result<T> = std::result::Result<T, ProtobufError>;

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
        NorthEast, NorthWest, SouthEast, SouthWest, North, South, East, West, Best,
    }
    DashStyle => DashStyle { Solid, Dashed, Dotted, DashDot, None }
    MarkerShape => MarkerShape {
        None, Circle, Square, Diamond, TriangleUp, TriangleDown, Plus, Cross, Point,
    }
    Dimension => Dimension { X, Y, Z }
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
            layout: Some(encode_tile_layout(figure.layout)),
            data: figure
                .data
                .iter()
                .map(|(id, array)| (id.0, encode_array(array)))
                .collect(),
            axes: figure.axes.iter().map(encode_axes).collect(),
            links: encode_links(&figure.links),
            provenance: Some(w::Provenance {
                ironlab_version: figure.provenance.ironlab_version.clone(),
                typesetter: figure.provenance.typesetter.clone(),
                fonts: figure.provenance.fonts.clone(),
            }),
            // Both maps are ordered by name, so the entries are written in ascending order
            // of the UTF-8 bytes of the name.
            parameters: figure
                .parameters
                .iter()
                .map(|(name, parameter)| (name.clone(), encode_parameter(parameter)))
                .collect(),
        }
    }
}

fn encode_array(array: &NdArray) -> w::NdArray {
    w::NdArray {
        shape: array.shape.iter().map(|&len| len as u64).collect(),
        values: array.values.clone(),
    }
}

fn encode_figure_size(size: FigureSize) -> w::FigureSize {
    w::FigureSize {
        width_mm: Some(size.width_mm),
        height_mm: Some(size.height_mm),
    }
}

fn encode_tile_layout(layout: TileLayout) -> w::TileLayout {
    w::TileLayout {
        rows: Some(layout.rows),
        cols: Some(layout.cols),
    }
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

fn encode_axis(axis: &Axis) -> w::Axis {
    w::Axis {
        label: axis.label.as_ref().map(encode_text),
        scale: axis.scale.to_wire(),
        limits: Some(encode_limits(axis.limits)),
        grid: Some(axis.grid),
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

fn encode_legend(legend: Legend) -> w::Legend {
    w::Legend {
        location: legend.location.to_wire(),
        boxed: Some(legend.boxed),
    }
}

fn encode_axes(axes: &Axes) -> w::Axes {
    w::Axes {
        id: Some(axes.id.0),
        cell: Some(encode_cell(axes.cell)),
        projection: Some(encode_projection(axes.projection)),
        title: axes.title.as_ref().map(encode_text),
        x: Some(encode_axis(&axes.x)),
        y: Some(encode_axis(&axes.y)),
        z: Some(encode_axis(&axes.z)),
        r#box: Some(axes.box_),
        colormap: axes.colormap.to_wire(),
        clim: Some(encode_limits(axes.clim)),
        legend: axes.legend.map(encode_legend),
        artists: axes.artists.iter().map(encode_artist).collect(),
    }
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

fn encode_artist(artist: &Artist) -> w::Artist {
    let kind = match artist {
        Artist::Line(line) => w::ArtistKind::Line(w::Line {
            id: Some(line.id.0),
            display_name: line.display_name.as_ref().map(encode_text),
            visible: Some(line.visible),
            x: Some(line.x.0),
            y: Some(line.y.0),
            z: line.z.map(|id| id.0),
            line: Some(encode_line_style(line.line)),
            marker: Some(encode_marker_style(line.marker)),
        }),
        Artist::Scatter(scatter) => w::ArtistKind::Scatter(w::Scatter {
            id: Some(scatter.id.0),
            display_name: scatter.display_name.as_ref().map(encode_text),
            visible: Some(scatter.visible),
            x: Some(scatter.x.0),
            y: Some(scatter.y.0),
            z: scatter.z.map(|id| id.0),
            size: Some(encode_scatter_size(scatter.size)),
            color: Some(encode_scatter_color(scatter.color)),
            marker: Some(encode_marker_style(scatter.marker)),
        }),
        Artist::Contour(contour) => w::ArtistKind::Contour(w::Contour {
            id: Some(contour.id.0),
            display_name: contour.display_name.as_ref().map(encode_text),
            visible: Some(contour.visible),
            grid: Some(encode_grid(contour.grid)),
            z: Some(contour.z.0),
            levels: Some(encode_levels(&contour.levels)),
            fill: Some(contour.fill),
            placement: Some(encode_contour_placement(contour.placement)),
            line: Some(encode_line_style(contour.line)),
        }),
        Artist::Quiver(quiver) => w::ArtistKind::Quiver(w::Quiver {
            id: Some(quiver.id.0),
            display_name: quiver.display_name.as_ref().map(encode_text),
            visible: Some(quiver.visible),
            x: Some(quiver.x.0),
            y: Some(quiver.y.0),
            z: quiver.z.map(|id| id.0),
            u: Some(quiver.u.0),
            v: Some(quiver.v.0),
            w: quiver.w.map(|id| id.0),
            scale: Some(encode_quiver_scale(quiver.scale)),
            line: Some(encode_line_style(quiver.line)),
            head_size: Some(quiver.head_size),
        }),
        Artist::Surface(surface) => w::ArtistKind::Surface(w::Surface {
            id: Some(surface.id.0),
            display_name: surface.display_name.as_ref().map(encode_text),
            visible: Some(surface.visible),
            grid: Some(encode_grid(surface.grid)),
            z: Some(surface.z.0),
            c: surface.c.map(|id| id.0),
            face: Some(encode_color_spec(surface.face)),
            edge: Some(encode_color_spec(surface.edge)),
            edge_width_pt: Some(surface.edge_width_pt),
        }),
    };
    w::Artist { kind: Some(kind) }
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
fn required<T>(value: Option<T>, at: &str, field: &str) -> Result<T> {
    value.ok_or_else(|| ProtobufError::MissingField {
        field: join(at, field),
    })
}

/// Returns the identifier of a node, which has no default.
fn node_id(value: Option<u64>, at: &str) -> Result<NodeId> {
    required(value, at, "id").map(NodeId)
}

/// Returns a reference to a data array, which has no default.
fn data_id(value: Option<u64>, at: &str, field: &str) -> Result<DataId> {
    required(value, at, field).map(DataId)
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
        .map(|(key, array)| {
            let array = decode_array(array, &format!("data[{key}]"))?;
            Ok((DataId(key), array))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let axes = wire
        .axes
        .into_iter()
        .enumerate()
        .map(|(i, axes)| decode_axes(axes, &format!("axes[{i}]")))
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
        layout: wire.layout.map_or(default.layout, decode_tile_layout),
        data,
        axes,
        links,
        provenance: Provenance {
            ironlab_version: provenance.ironlab_version,
            typesetter: provenance.typesetter,
            fonts: provenance.fonts,
        },
        parameters,
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
    Ok(NdArray {
        shape,
        values: wire.values,
    })
}

fn decode_figure_size(wire: w::FigureSize) -> FigureSize {
    let default = FigureSize::default();
    FigureSize {
        width_mm: wire.width_mm.unwrap_or(default.width_mm),
        height_mm: wire.height_mm.unwrap_or(default.height_mm),
    }
}

fn decode_tile_layout(wire: w::TileLayout) -> TileLayout {
    let default = TileLayout::default();
    TileLayout {
        rows: wire.rows.unwrap_or(default.rows),
        cols: wire.cols.unwrap_or(default.cols),
    }
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
    let default = Legend::default();
    Ok(Legend {
        location: decode_enum(wire.location, Some(default.location), at, "location")?,
        boxed: wire.boxed.unwrap_or(default.boxed),
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

fn decode_display_name(wire: Option<w::Text>, at: &str) -> Result<Option<Text>> {
    wire.map(|t| decode_text(t, &join(at, "display_name")))
        .transpose()
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

fn decode_axes(wire: w::Axes, at: &str) -> Result<Axes> {
    let default = Axes::default();
    let legend = wire
        .legend
        .map(|legend| decode_legend(legend, &join(at, "legend")))
        .transpose()?;
    let artists = wire
        .artists
        .into_iter()
        .enumerate()
        .map(|(i, artist)| decode_artist(artist, &format!("{at}.artists[{i}]")))
        .collect::<Result<Vec<_>>>()?;
    Ok(Axes {
        id: node_id(wire.id, at)?,
        cell: wire.cell.map_or(default.cell, decode_cell),
        projection: decode_projection(wire.projection, default.projection),
        title: wire
            .title
            .map(|t| decode_text(t, &join(at, "title")))
            .transpose()?,
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

fn decode_artist(wire: w::Artist, at: &str) -> Result<Artist> {
    Ok(match required(wire.kind, at, "kind")? {
        w::ArtistKind::Line(line) => Artist::Line(decode_line(line, &join(at, "line"))?),
        w::ArtistKind::Scatter(scatter) => {
            Artist::Scatter(decode_scatter(scatter, &join(at, "scatter"))?)
        }
        w::ArtistKind::Contour(contour) => {
            Artist::Contour(decode_contour(contour, &join(at, "contour"))?)
        }
        w::ArtistKind::Quiver(quiver) => {
            Artist::Quiver(decode_quiver(quiver, &join(at, "quiver"))?)
        }
        w::ArtistKind::Surface(surface) => {
            Artist::Surface(decode_surface(surface, &join(at, "surface"))?)
        }
    })
}

fn decode_line(wire: w::Line, at: &str) -> Result<Line> {
    let default = Line::default();
    Ok(Line {
        id: node_id(wire.id, at)?,
        display_name: decode_display_name(wire.display_name, at)?,
        visible: wire.visible.unwrap_or(default.visible),
        x: data_id(wire.x, at, "x")?,
        y: data_id(wire.y, at, "y")?,
        z: wire.z.map(DataId),
        line: decode_line_style(wire.line, default.line, &join(at, "line"))?,
        marker: decode_marker_style(wire.marker, default.marker, &join(at, "marker"))?,
    })
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
            data: data_id(data.data, at, "data.data")?,
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
            data: data_id(data.data, at, "data.data")?,
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

fn decode_scatter(wire: w::Scatter, at: &str) -> Result<Scatter> {
    let default = Scatter::default();
    let size = decode_scatter_size(wire.size, &join(at, "size"))?;
    let color = decode_scatter_color(wire.color, &join(at, "color"))?;
    Ok(Scatter {
        id: node_id(wire.id, at)?,
        display_name: decode_display_name(wire.display_name, at)?,
        visible: wire.visible.unwrap_or(default.visible),
        x: data_id(wire.x, at, "x")?,
        y: data_id(wire.y, at, "y")?,
        z: wire.z.map(DataId),
        size,
        color,
        marker: decode_marker_style(wire.marker, default.marker, &join(at, "marker"))?,
    })
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
                x: data_id(grid.x, &at, "x")?,
                y: data_id(grid.y, &at, "y")?,
            }
        }
        w::GridKind::Curvilinear(grid) => {
            let at = join(at, "curvilinear");
            Grid::Curvilinear {
                x: data_id(grid.x, &at, "x")?,
                y: data_id(grid.y, &at, "y")?,
            }
        }
    })
}

fn decode_contour(wire: w::Contour, at: &str) -> Result<Contour> {
    let default = Contour::default();
    let levels = decode_levels(wire.levels);
    let placement = decode_contour_placement(wire.placement);
    Ok(Contour {
        id: node_id(wire.id, at)?,
        display_name: decode_display_name(wire.display_name, at)?,
        visible: wire.visible.unwrap_or(default.visible),
        grid: decode_grid(wire.grid, &join(at, "grid"))?,
        z: data_id(wire.z, at, "z")?,
        levels,
        fill: wire.fill.unwrap_or(default.fill),
        placement,
        line: decode_line_style(wire.line, default.line, &join(at, "line"))?,
    })
}

fn decode_quiver(wire: w::Quiver, at: &str) -> Result<Quiver> {
    let default = Quiver::default();
    let scale = decode_quiver_scale(wire.scale, &join(at, "scale"))?;
    Ok(Quiver {
        id: node_id(wire.id, at)?,
        display_name: decode_display_name(wire.display_name, at)?,
        visible: wire.visible.unwrap_or(default.visible),
        x: data_id(wire.x, at, "x")?,
        y: data_id(wire.y, at, "y")?,
        z: wire.z.map(DataId),
        u: data_id(wire.u, at, "u")?,
        v: data_id(wire.v, at, "v")?,
        w: wire.w.map(DataId),
        scale,
        line: decode_line_style(wire.line, default.line, &join(at, "line"))?,
        head_size: wire.head_size.unwrap_or(default.head_size),
    })
}

fn decode_surface(wire: w::Surface, at: &str) -> Result<Surface> {
    let default = Surface::default();
    Ok(Surface {
        id: node_id(wire.id, at)?,
        display_name: decode_display_name(wire.display_name, at)?,
        visible: wire.visible.unwrap_or(default.visible),
        grid: decode_grid(wire.grid, &join(at, "grid"))?,
        z: data_id(wire.z, at, "z")?,
        c: wire.c.map(DataId),
        face: decode_color_spec(wire.face, default.face),
        edge: decode_color_spec(wire.edge, default.edge),
        edge_width_pt: wire.edge_width_pt.unwrap_or(default.edge_width_pt),
    })
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
            node: Some(encode_node(node)),
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
        Edit::PutData { id, array } => w::EditKind::PutData(w::EditPutData {
            id: Some(id.0),
            array: Some(encode_array(array)),
        }),
        Edit::AppendData { id, array, retain } => w::EditKind::AppendData(w::EditAppendData {
            id: Some(id.0),
            array: Some(encode_array(array)),
            retain: *retain,
        }),
        Edit::RemoveData { id } => w::EditKind::RemoveData(w::EditRemoveData { id: Some(id.0) }),
    };
    w::Edit { kind: Some(kind) }
}

fn encode_node(node: &Node) -> w::Node {
    let kind = match node {
        Node::Axes(axes) => w::NodeKind::Axes(encode_axes(axes)),
        Node::Artist(artist) => w::NodeKind::Artist(encode_artist(artist)),
    };
    w::Node { kind: Some(kind) }
}

fn encode_value(value: &Value) -> w::Value {
    let kind = match value {
        Value::Unset => w::ValueKind::Unset(w::ValueUnset {}),
        Value::Bool(v) => w::ValueKind::Bool(w::ValueBool { value: Some(*v) }),
        Value::UInt32(v) => w::ValueKind::Uint32(w::ValueUint32 { value: Some(*v) }),
        Value::Double(v) => w::ValueKind::Double(w::ValueDouble { value: Some(*v) }),
        Value::Float(v) => w::ValueKind::Float(w::ValueFloat { value: Some(*v) }),
        Value::String(v) => w::ValueKind::String(w::ValueString { value: v.clone() }),
        Value::DataId(v) => w::ValueKind::DataId(w::ValueDataId { value: Some(v.0) }),
        Value::Doubles(v) => w::ValueKind::Doubles(w::ValueDoubles { values: v.clone() }),
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
            value: Some(encode_tile_layout(*v)),
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
            value: Some(encode_legend(*v)),
        }),
        Value::LegendLocation(v) => {
            w::ValueKind::LegendLocation(w::ValueLegendLocation { value: v.to_wire() })
        }
        Value::ColorSpec(v) => w::ValueKind::ColorSpec(w::ValueColorSpec {
            value: Some(encode_color_spec(*v)),
        }),
        Value::LineStyle(v) => w::ValueKind::LineStyle(w::ValueLineStyle {
            value: Some(encode_line_style(*v)),
        }),
        Value::DashStyle(v) => w::ValueKind::DashStyle(w::ValueDashStyle { value: v.to_wire() }),
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
                node: decode_node(required(insert.node, &at, "node")?, &join(&at, "node"))?,
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
        w::EditKind::PutData(put) => {
            let at = join(at, "put_data");
            Edit::PutData {
                id: DataId(required(put.id, &at, "id")?),
                array: decode_array(required(put.array, &at, "array")?, &join(&at, "array"))?,
            }
        }
        w::EditKind::AppendData(append) => {
            let at = join(at, "append_data");
            Edit::AppendData {
                id: DataId(required(append.id, &at, "id")?),
                array: decode_array(required(append.array, &at, "array")?, &join(&at, "array"))?,
                retain: append.retain,
            }
        }
        w::EditKind::RemoveData(remove) => Edit::RemoveData {
            id: DataId(required(remove.id, &join(at, "remove_data"), "id")?),
        },
    })
}

fn decode_node(wire: w::Node, at: &str) -> Result<Node> {
    Ok(match required(wire.kind, at, "kind")? {
        w::NodeKind::Axes(axes) => Node::Axes(Box::new(decode_axes(axes, &join(at, "axes"))?)),
        w::NodeKind::Artist(artist) => Node::Artist(decode_artist(artist, &join(at, "artist"))?),
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
        w::ValueKind::DataId(v) => Value::DataId(DataId(required(
            v.value,
            &join(at, "data_id_value"),
            "value",
        )?)),
        w::ValueKind::Doubles(v) => Value::Doubles(v.values),
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
            Value::TileLayout(decode_tile_layout(layout))
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
    })
}
