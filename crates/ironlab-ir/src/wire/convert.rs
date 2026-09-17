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
    DashStyle, DataId, Dimension, Figure, FigureSize, FontSetId, Grid, Interpreter, Legend,
    LegendLocation, Levels, Limits, Line, LineStyle, MarkerShape, MarkerStyle, NdArray, NodeId,
    Parameter, Projection, Provenance, Quiver, QuiverScale, Scale, Scatter, ScatterColor,
    ScatterSize, Surface, Text, TileLayout, View3d,
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
            size: Some(w::FigureSize {
                width_mm: Some(figure.size.width_mm),
                height_mm: Some(figure.size.height_mm),
            }),
            font_set: figure.font_set.to_wire(),
            font_size_pt: Some(figure.font_size_pt),
            background: Some(encode_color(figure.background)),
            layout: Some(w::TileLayout {
                rows: Some(figure.layout.rows),
                cols: Some(figure.layout.cols),
            }),
            data: figure
                .data
                .iter()
                .map(|(id, array)| {
                    let array = w::NdArray {
                        shape: array.shape.iter().map(|&len| len as u64).collect(),
                        values: array.values.clone(),
                    };
                    (id.0, array)
                })
                .collect(),
            axes: figure.axes.iter().map(encode_axes).collect(),
            links: figure
                .links
                .iter()
                .map(|link| w::AxisLink {
                    dimension: link.dimension.to_wire(),
                    axes: link.axes.iter().map(|id| id.0).collect(),
                })
                .collect(),
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

fn encode_axes(axes: &Axes) -> w::Axes {
    let projection = match axes.projection {
        Projection::TwoD => w::ProjectionKind::TwoD(w::ProjectionTwoD {}),
        Projection::ThreeD { view3d } => w::ProjectionKind::ThreeD(w::ProjectionThreeD {
            view3d: Some(w::View3d {
                azimuth_deg: Some(view3d.azimuth_deg),
                elevation_deg: Some(view3d.elevation_deg),
                zoom: Some(view3d.zoom),
                pan_x: Some(view3d.pan_x),
                pan_y: Some(view3d.pan_y),
            }),
        }),
    };
    w::Axes {
        id: Some(axes.id.0),
        cell: Some(w::Cell {
            row: Some(axes.cell.row),
            col: Some(axes.cell.col),
            row_span: Some(axes.cell.row_span),
            col_span: Some(axes.cell.col_span),
        }),
        projection: Some(w::Projection {
            kind: Some(projection),
        }),
        title: axes.title.as_ref().map(encode_text),
        x: Some(encode_axis(&axes.x)),
        y: Some(encode_axis(&axes.y)),
        z: Some(encode_axis(&axes.z)),
        r#box: Some(axes.box_),
        colormap: axes.colormap.to_wire(),
        clim: Some(encode_limits(axes.clim)),
        legend: axes.legend.map(|legend| w::Legend {
            location: legend.location.to_wire(),
            boxed: Some(legend.boxed),
        }),
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
        Artist::Scatter(scatter) => {
            let size = match scatter.size {
                ScatterSize::Scalar { value } => {
                    w::ScatterSizeKind::Scalar(w::ScatterSizeScalar { value: Some(value) })
                }
                ScatterSize::Data { data } => {
                    w::ScatterSizeKind::Data(w::ScatterSizeData { data: Some(data.0) })
                }
            };
            let color = match scatter.color {
                ScatterColor::Spec { spec } => w::ScatterColorKind::Spec(w::ScatterColorSpec {
                    spec: Some(encode_color_spec(spec)),
                }),
                ScatterColor::Data { data } => {
                    w::ScatterColorKind::Data(w::ScatterColorData { data: Some(data.0) })
                }
            };
            w::ArtistKind::Scatter(w::Scatter {
                id: Some(scatter.id.0),
                display_name: scatter.display_name.as_ref().map(encode_text),
                visible: Some(scatter.visible),
                x: Some(scatter.x.0),
                y: Some(scatter.y.0),
                z: scatter.z.map(|id| id.0),
                size: Some(w::ScatterSize { kind: Some(size) }),
                color: Some(w::ScatterColor { kind: Some(color) }),
                marker: Some(encode_marker_style(scatter.marker)),
            })
        }
        Artist::Contour(contour) => {
            let levels = match &contour.levels {
                Levels::Auto { count } => w::LevelsKind::Auto(w::LevelsAuto {
                    count: Some(*count),
                }),
                Levels::Explicit { values } => w::LevelsKind::Explicit(w::LevelsExplicit {
                    values: values.clone(),
                }),
            };
            let placement = match contour.placement {
                ContourPlacement::Plane { z } => {
                    w::ContourPlacementKind::Plane(w::ContourPlacementPlane { z })
                }
                ContourPlacement::AtLevel => {
                    w::ContourPlacementKind::AtLevel(w::ContourPlacementAtLevel {})
                }
            };
            w::ArtistKind::Contour(w::Contour {
                id: Some(contour.id.0),
                display_name: contour.display_name.as_ref().map(encode_text),
                visible: Some(contour.visible),
                grid: Some(encode_grid(contour.grid)),
                z: Some(contour.z.0),
                levels: Some(w::Levels { kind: Some(levels) }),
                fill: Some(contour.fill),
                placement: Some(w::ContourPlacement {
                    kind: Some(placement),
                }),
                line: Some(encode_line_style(contour.line)),
            })
        }
        Artist::Quiver(quiver) => {
            let scale = match quiver.scale {
                QuiverScale::Auto => w::QuiverScaleKind::Auto(w::QuiverScaleAuto {}),
                QuiverScale::Factor { value } => {
                    w::QuiverScaleKind::Factor(w::QuiverScaleFactor { value: Some(value) })
                }
                QuiverScale::Off => w::QuiverScaleKind::Off(w::QuiverScaleOff {}),
            };
            w::ArtistKind::Quiver(w::Quiver {
                id: Some(quiver.id.0),
                display_name: quiver.display_name.as_ref().map(encode_text),
                visible: Some(quiver.visible),
                x: Some(quiver.x.0),
                y: Some(quiver.y.0),
                z: quiver.z.map(|id| id.0),
                u: Some(quiver.u.0),
                v: Some(quiver.v.0),
                w: quiver.w.map(|id| id.0),
                scale: Some(w::QuiverScale { kind: Some(scale) }),
                line: Some(encode_line_style(quiver.line)),
                head_size: Some(quiver.head_size),
            })
        }
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
    let size = wire.size.unwrap_or_default();
    let layout = wire.layout.unwrap_or_default();
    let provenance = wire.provenance.unwrap_or_default();
    let data = wire
        .data
        .into_iter()
        .map(|(key, array)| Ok((DataId(key), decode_array(array, key)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let axes = wire
        .axes
        .into_iter()
        .enumerate()
        .map(|(i, axes)| decode_axes(axes, &format!("axes[{i}]")))
        .collect::<Result<Vec<_>>>()?;
    let links = wire
        .links
        .into_iter()
        .enumerate()
        .map(|(i, link)| {
            Ok(AxisLink {
                dimension: decode_enum(link.dimension, None, &format!("links[{i}]"), "dimension")?,
                axes: link.axes.into_iter().map(NodeId).collect(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
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
        size: FigureSize {
            width_mm: size.width_mm.unwrap_or(default.size.width_mm),
            height_mm: size.height_mm.unwrap_or(default.size.height_mm),
        },
        font_set: decode_enum(wire.font_set, Some(default.font_set), "", "font_set")?,
        font_size_pt: wire.font_size_pt.unwrap_or(default.font_size_pt),
        background: wire.background.map_or(default.background, decode_color),
        layout: TileLayout {
            rows: layout.rows.unwrap_or(default.layout.rows),
            cols: layout.cols.unwrap_or(default.layout.cols),
        },
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

fn decode_array(wire: w::NdArray, key: u64) -> Result<NdArray> {
    let shape = wire
        .shape
        .into_iter()
        .map(|len| {
            usize::try_from(len).map_err(|_| ProtobufError::InvalidValue {
                field: format!("data[{key}].shape"),
                reason: format!("the dimension {len} exceeds the address space"),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NdArray {
        shape,
        values: wire.values,
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
        Some(w::ProjectionKind::ThreeD(three_d)) => {
            let view = View3d::default();
            let wire = three_d.view3d.unwrap_or_default();
            Projection::ThreeD {
                view3d: View3d {
                    azimuth_deg: wire.azimuth_deg.unwrap_or(view.azimuth_deg),
                    elevation_deg: wire.elevation_deg.unwrap_or(view.elevation_deg),
                    zoom: wire.zoom.unwrap_or(view.zoom),
                    pan_x: wire.pan_x.unwrap_or(view.pan_x),
                    pan_y: wire.pan_y.unwrap_or(view.pan_y),
                },
            }
        }
    }
}

fn decode_axes(wire: w::Axes, at: &str) -> Result<Axes> {
    let default = Axes::default();
    let cell = wire.cell.unwrap_or_default();
    let legend = wire
        .legend
        .map(|legend| {
            let default = Legend::default();
            Ok::<_, ProtobufError>(Legend {
                location: decode_enum(
                    legend.location,
                    Some(default.location),
                    &join(at, "legend"),
                    "location",
                )?,
                boxed: legend.boxed.unwrap_or(default.boxed),
            })
        })
        .transpose()?;
    let artists = wire
        .artists
        .into_iter()
        .enumerate()
        .map(|(i, artist)| decode_artist(artist, &format!("{at}.artists[{i}]")))
        .collect::<Result<Vec<_>>>()?;
    Ok(Axes {
        id: node_id(wire.id, at)?,
        cell: Cell {
            row: cell.row.unwrap_or(default.cell.row),
            col: cell.col.unwrap_or(default.cell.col),
            row_span: cell.row_span.unwrap_or(default.cell.row_span),
            col_span: cell.col_span.unwrap_or(default.cell.col_span),
        },
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

fn decode_scatter(wire: w::Scatter, at: &str) -> Result<Scatter> {
    let default = Scatter::default();
    let size = match wire.size.and_then(|size| size.kind) {
        None => default.size,
        Some(w::ScatterSizeKind::Scalar(scalar)) => {
            let ScatterSize::Scalar { value } = ScatterSize::default() else {
                unreachable!("the default scatter size is a scalar");
            };
            ScatterSize::Scalar {
                value: scalar.value.unwrap_or(value),
            }
        }
        Some(w::ScatterSizeKind::Data(data)) => ScatterSize::Data {
            data: data_id(data.data, at, "size.data.data")?,
        },
    };
    let color = match wire.color.and_then(|color| color.kind) {
        None => default.color,
        Some(w::ScatterColorKind::Spec(spec)) => {
            let ScatterColor::Spec { spec: default_spec } = ScatterColor::default() else {
                unreachable!("the default scatter colour is a colour specification");
            };
            ScatterColor::Spec {
                spec: decode_color_spec(spec.spec, default_spec),
            }
        }
        Some(w::ScatterColorKind::Data(data)) => ScatterColor::Data {
            data: data_id(data.data, at, "color.data.data")?,
        },
    };
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

/// Decodes the grid of a contour or surface, which has no default.
fn decode_grid(wire: Option<w::Grid>, at: &str) -> Result<Grid> {
    let at = join(at, "grid");
    let wire = wire.ok_or_else(|| ProtobufError::MissingField { field: at.clone() })?;
    Ok(match required(wire.kind, &at, "kind")? {
        w::GridKind::Rectilinear(grid) => {
            let at = join(&at, "rectilinear");
            Grid::Rectilinear {
                x: data_id(grid.x, &at, "x")?,
                y: data_id(grid.y, &at, "y")?,
            }
        }
        w::GridKind::Curvilinear(grid) => {
            let at = join(&at, "curvilinear");
            Grid::Curvilinear {
                x: data_id(grid.x, &at, "x")?,
                y: data_id(grid.y, &at, "y")?,
            }
        }
    })
}

fn decode_contour(wire: w::Contour, at: &str) -> Result<Contour> {
    let default = Contour::default();
    let levels = match wire.levels.and_then(|levels| levels.kind) {
        None => default.levels,
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
    };
    let placement = match wire.placement.and_then(|placement| placement.kind) {
        None => default.placement,
        Some(w::ContourPlacementKind::Plane(plane)) => ContourPlacement::Plane { z: plane.z },
        Some(w::ContourPlacementKind::AtLevel(_)) => ContourPlacement::AtLevel,
    };
    Ok(Contour {
        id: node_id(wire.id, at)?,
        display_name: decode_display_name(wire.display_name, at)?,
        visible: wire.visible.unwrap_or(default.visible),
        grid: decode_grid(wire.grid, at)?,
        z: data_id(wire.z, at, "z")?,
        levels,
        fill: wire.fill.unwrap_or(default.fill),
        placement,
        line: decode_line_style(wire.line, default.line, &join(at, "line"))?,
    })
}

fn decode_quiver(wire: w::Quiver, at: &str) -> Result<Quiver> {
    let default = Quiver::default();
    let scale = match wire.scale.and_then(|scale| scale.kind) {
        None => default.scale,
        Some(w::QuiverScaleKind::Auto(_)) => QuiverScale::Auto,
        Some(w::QuiverScaleKind::Factor(factor)) => QuiverScale::Factor {
            value: required(factor.value, at, "scale.factor.value")?,
        },
        Some(w::QuiverScaleKind::Off(_)) => QuiverScale::Off,
    };
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
        grid: decode_grid(wire.grid, at)?,
        z: data_id(wire.z, at, "z")?,
        c: wire.c.map(DataId),
        face: decode_color_spec(wire.face, default.face),
        edge: decode_color_spec(wire.edge, default.edge),
        edge_width_pt: wire.edge_width_pt.unwrap_or(default.edge_width_pt),
    })
}
