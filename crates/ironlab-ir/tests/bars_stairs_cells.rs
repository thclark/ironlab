//! Bars, stairs and cells: their defaults, their JSON and Protocol Buffers forms, and
//! their validation.
//!
//! A bar series draws one rectangle per value from a base to the value, centred in a slot
//! along the category axis; stairs draw a stepped outline through values held between
//! edges or at samples; and cells fill a grid of rectangles, one per value, through a
//! colour scale. As in `validate.rs`, each invalid case is the smallest change to an
//! otherwise valid figure, so that a test fails only if that specific check is missing or
//! over-eager.

mod common;

use common::FigureBuilder;
use ironlab_ir::*;
use serde_json::{Value as JsonValue, json};

/// The continuous colour scale that every fixture holds, through which cells are mapped
/// unless a case names another scale.
const SCALE: NodeId = NodeId(900);

fn has_error(report: &ValidationReport, kind: IssueKind, node: NodeId) -> bool {
    report
        .errors
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

fn has_warning(report: &ValidationReport, kind: IssueKind, node: NodeId) -> bool {
    report
        .warnings
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

fn error_kinds(report: &ValidationReport) -> Vec<IssueKind> {
    report.errors.iter().map(|issue| issue.kind).collect()
}

fn warning_kinds(report: &ValidationReport) -> Vec<IssueKind> {
    report.warnings.iter().map(|issue| issue.kind).collect()
}

/// Asserts that a report has no errors and no warnings; `at` names the case.
#[track_caller]
fn assert_no_issues(report: &ValidationReport, at: &str) {
    assert_eq!(report.errors, vec![], "{at}: {report:?}");
    assert_eq!(report.warnings, vec![], "{at}: {report:?}");
}

/// Asserts that a report holds exactly one error, of the given kind, against `node`, and
/// no warning; `at` names the case.
#[track_caller]
fn assert_only_error(report: &ValidationReport, kind: IssueKind, node: NodeId, at: &str) {
    assert_eq!(error_kinds(report), [kind], "{at}: {report:?}");
    assert!(has_error(report, kind, node), "{at}: {report:?}");
    assert_eq!(report.warnings, vec![], "{at}: {report:?}");
}

/// A figure with one axes holding one artist, built by [`fixture`].
struct Fixture {
    fig: Figure,
    artist: NodeId,
}

/// Builds a figure with one 2D or 3D axes, the colour scale [`SCALE`], and the single
/// artist that `make` returns after adding the arrays it needs to the builder, so that
/// the figure holds no array that the artist does not use.
fn fixture(three_d: bool, make: impl FnOnce(&mut FigureBuilder, NodeId) -> Artist) -> Fixture {
    let mut b = FigureBuilder::new();
    let axes = if three_d {
        b.axes3d(0, 0)
    } else {
        b.axes2d(0, 0)
    };
    b.fig.color_scales.push(ColorScale {
        id: SCALE,
        ..ColorScale::default()
    });
    let artist = b.node();
    let made = make(&mut b, artist);
    b.push(axes, made);
    Fixture {
        fig: b.build(),
        artist,
    }
}

/// Makes the artist of a fixture, adding the arrays it needs to the builder.
type Make = Box<dyn FnOnce(&mut FigureBuilder, NodeId) -> Artist>;

/// Returns the only artist of a fixture for modification.
fn only_artist(fig: &mut Figure) -> &mut Artist {
    &mut fig.axes[0].artists[0]
}

/// A vertical bar series of the given values at positions 1, 2, …, n, with the defaults
/// of a bar series otherwise.
fn bars(values: &'static [f64]) -> impl FnOnce(&mut FigureBuilder, NodeId) -> Artist {
    move |b, id| {
        let positions: Vec<f64> = (1..=values.len()).map(|k| k as f64).collect();
        Artist::Bar(Bar {
            id,
            positions: b.vector(&positions),
            values: b.vector(values),
            ..Bar::default()
        })
    }
}

/// Returns the bar series of a fixture for modification.
fn bar_mut(fig: &mut Figure) -> &mut Bar {
    match only_artist(fig) {
        Artist::Bar(bar) => bar,
        other => panic!("expected a bar series, found {other:?}"),
    }
}

/// Stairs through the given values between edges 0, 1, …, n, with the defaults of stairs
/// otherwise: an open outline.
fn stairs_on_edges(values: &'static [f64]) -> impl FnOnce(&mut FigureBuilder, NodeId) -> Artist {
    move |b, id| {
        let edges: Vec<f64> = (0..=values.len()).map(|k| k as f64).collect();
        Artist::Stairs(Stairs {
            id,
            data: StairsData::Edges {
                edges: b.vector(&edges),
                values: b.vector(values),
            },
            ..Stairs::default()
        })
    }
}

/// Returns the stairs of a fixture for modification.
fn stairs_mut(fig: &mut Figure) -> &mut Stairs {
    match only_artist(fig) {
        Artist::Stairs(stairs) => stairs,
        other => panic!("expected stairs, found {other:?}"),
    }
}

/// Cells of a `ny` × `nx` grid of values counting up from 1, on unit edges, mapped
/// through [`SCALE`].
fn cells(ny: usize, nx: usize) -> impl FnOnce(&mut FigureBuilder, NodeId) -> Artist {
    move |b, id| {
        Artist::Cells(Cells {
            id,
            values: b.matrix(ny, nx, |j, i| (j * nx + i) as f64 + 1.0),
            color_scale: Some(SCALE),
            ..Cells::default()
        })
    }
}

/// Returns the cells of a fixture for modification.
fn cells_mut(fig: &mut Figure) -> &mut Cells {
    match only_artist(fig) {
        Artist::Cells(cells) => cells,
        other => panic!("expected cells, found {other:?}"),
    }
}

/// Adds a one-dimensional array to a built figure under the given identifier, so that a
/// case can point an artist at an array the fixture did not make.
fn push_vector(fig: &mut Figure, id: NodeId, values: &[f64]) -> NodeId {
    fig.data.push(DataArray {
        id,
        name: None,
        array: NdArray::vector(values.to_vec()),
    });
    id
}

/// Adds an array of the given shape and values to a built figure under the given
/// identifier.
fn push_array(fig: &mut Figure, id: NodeId, shape: Vec<usize>, values: Vec<f64>) -> NodeId {
    fig.data.push(DataArray {
        id,
        name: None,
        array: NdArray::from_shape(shape, values).expect("the shape matches the values"),
    });
    id
}

/// Adds an array of 8-bit values of the given shape to a built figure under the given
/// identifier.
fn push_bytes(fig: &mut Figure, id: NodeId, shape: Vec<usize>, values: Vec<u8>) -> NodeId {
    fig.data.push(DataArray {
        id,
        name: None,
        array: NdArray::from_shape_u8(shape, values).expect("the shape matches the values"),
    });
    id
}

/// Replaces the values of an array of a built figure, keeping its identifier.
fn replace_array(fig: &mut Figure, id: NodeId, array: NdArray) {
    fig.data
        .iter_mut()
        .find(|data| data.id == id)
        .expect("the array is in the figure")
        .array = array;
}

// ---------------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------------

// Why: the defaults are what a program gets when it sets only the data, so they must draw
// an ordinary bar chart: bars in the next colour of the colour order, rising from zero,
// each taking four fifths of the spacing between neighbouring positions so that a gap
// separates them, standing alone rather than in a group, with no outline and fully opaque.
// A default edge of the automatic colour would outline every bar in the colour of its own
// face; a default gap of 0 would join the bars into a histogram.
#[test]
fn a_default_bar_series_is_a_plain_bar_chart_in_the_colour_order() {
    let bar = Bar::default();
    assert_eq!(bar.label, None);
    assert!(bar.visible);
    assert_eq!(bar.orientation, Orientation::Vertical);
    assert_eq!(bar.base, BarBase::Constant(0.0));
    assert_eq!(bar.slot, BarSlot::Pitch);
    assert_eq!(bar.gap, 0.2);
    assert_eq!(bar.group, None);
    assert_eq!(bar.face, ColorSpec::Auto);
    assert_eq!(bar.colors, vec![]);
    assert_eq!(
        bar.edge,
        LineStyle {
            color: ColorSpec::None,
            width_pt: 0.5,
            dash: DashStyle::Solid,
        }
    );
    assert_eq!(bar.face_opacity, 1.0);
    // A group that is created is grouped side by side unless it says otherwise, as
    // grouped bars are the common comparison.
    assert_eq!(BarLayout::default(), BarLayout::Grouped);
    assert_eq!(BarBase::default(), BarBase::Constant(0.0));
    assert_eq!(BarSlot::default(), BarSlot::Pitch);
    assert_eq!(Orientation::default(), Orientation::Vertical);
}

// Why: stairs are what a step histogram draws, whose default is an open outline in the
// colour order: no base (so the outline does not drop to an axis it was not asked to
// reach), no fill, and the step after each sample, as MATLAB's `stairs` and Matplotlib's
// `step(where="post")` place it.
#[test]
fn default_stairs_are_an_open_unfilled_outline_in_the_colour_order() {
    let stairs = Stairs::default();
    assert_eq!(stairs.label, None);
    assert!(stairs.visible);
    assert_eq!(stairs.orientation, Orientation::Vertical);
    assert!(
        matches!(stairs.data, StairsData::Edges { .. }),
        "{:?}",
        stairs.data
    );
    assert_eq!(stairs.base, None);
    assert_eq!(stairs.line, LineStyle::default());
    assert_eq!(stairs.line.color, ColorSpec::Auto);
    assert_eq!(stairs.fill, ColorSpec::None);
    assert_eq!(stairs.fill_opacity, 1.0);
    assert_eq!(StepAt::default(), StepAt::Post);
}

// Why: cells are what a 2D histogram and a heatmap draw, whose default is a seamless grid
// of cells centred on whole numbers (so that cell k of a matrix sits at k, as an image's
// pixels do), with no gaps and no outlines, mapped through a scale that the program must
// name, without alpha, and fully opaque.
#[test]
fn default_cells_are_a_seamless_grid_on_unit_edges_without_outlines() {
    let cells = Cells::default();
    assert_eq!(cells.label, None);
    assert!(cells.visible);
    assert_eq!(cells.columns, CellEdges::Unit);
    assert_eq!(cells.rows, CellEdges::Unit);
    assert_eq!(cells.gap, CellGap { x: 0.0, y: 0.0 });
    assert_eq!(cells.gap, CellGap::default());
    assert_eq!(
        cells.edge,
        LineStyle {
            color: ColorSpec::None,
            width_pt: 0.5,
            dash: DashStyle::Solid,
        }
    );
    assert_eq!(cells.color_scale, None);
    assert_eq!(cells.alpha, None);
    assert_eq!(cells.opacity, 1.0);
    assert_eq!(CellEdges::default(), CellEdges::Unit);
}

// ---------------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------------

/// Writes a figure holding the given artists in one axes as JSON, and returns the JSON of
/// its artists together with the figure read back from the text.
fn written_artists(artists: Vec<Artist>) -> (Vec<JsonValue>, Figure) {
    let mut fig = Figure::new();
    fig.axes.push(Axes {
        id: NodeId(2),
        artists,
        ..Axes::default()
    });
    let text = fig.to_json();
    let value: JsonValue = serde_json::from_str(&text).unwrap();
    let artists = value["axes"][0]["artists"].as_array().unwrap().clone();
    let restored = Figure::from_json(&text).expect("own output parses");
    assert_eq!(restored, fig, "the figure changed through JSON");
    (artists, restored)
}

// Why: web clients and other tools write `.fig.json` by hand from the documented form, so
// the form of a default bar series is a contract: the tag `bar`, the base as a tagged
// constant, the slot as a tag without a value, and every plain property written; the
// per-bar colours and the opacity, which are absent or 1 in almost every chart, are left
// out, so that files stay as short as those written before bars had them.
#[test]
fn a_default_bar_series_is_written_in_the_documented_json_form() {
    let (artists, _) = written_artists(vec![Artist::Bar(Bar {
        id: NodeId(3),
        positions: NodeId(10),
        values: NodeId(11),
        ..Bar::default()
    })]);
    assert_eq!(
        artists[0],
        json!({
            "type": "bar",
            "id": 3,
            "label": null,
            "visible": true,
            "orientation": "vertical",
            "positions": 10,
            "values": 11,
            "base": { "type": "constant", "value": 0.0 },
            "slot": { "type": "pitch" },
            "gap": 0.2,
            "group": null,
            "face": { "type": "auto" },
            "edge": { "color": { "type": "none" }, "width_pt": 0.5, "dash": "solid" }
        })
    );
}

// Why: the variants that a hand-written file uses for floating bars (a base given as
// data), for fixed and per-bar widths, for a stacked group and for highlighted bars must be
// written as documented, with a face opacity of 0 written although it is falsy, because it
// hides the faces rather than leaving them at the default.
#[test]
fn the_variants_of_a_bar_series_are_written_in_the_documented_json_form() {
    let red = Color::rgb(1.0, 0.0, 0.0);
    let (artists, _) = written_artists(vec![
        Artist::Bar(Bar {
            id: NodeId(3),
            orientation: Orientation::Horizontal,
            positions: NodeId(10),
            values: NodeId(11),
            base: BarBase::Data(NodeId(12)),
            slot: BarSlot::Width(0.5),
            gap: 0.0,
            group: Some(BarGroup {
                name: "models".to_owned(),
                layout: BarLayout::Stacked,
            }),
            colors: vec![red, Color::BLACK],
            face_opacity: 0.0,
            ..Bar::default()
        }),
        Artist::Bar(Bar {
            id: NodeId(4),
            positions: NodeId(10),
            values: NodeId(11),
            slot: BarSlot::Data(NodeId(13)),
            ..Bar::default()
        }),
    ]);
    let first = &artists[0];
    assert_eq!(first["orientation"], json!("horizontal"));
    assert_eq!(first["base"], json!({ "type": "data", "value": 12 }));
    assert_eq!(first["slot"], json!({ "type": "width", "value": 0.5 }));
    assert_eq!(first["gap"], json!(0.0));
    assert_eq!(
        first["group"],
        json!({ "name": "models", "layout": "stacked" })
    );
    assert_eq!(first["colors"], json!(["#ff0000", "#000000"]));
    assert_eq!(first["face_opacity"], json!(0.0));
    assert_eq!(artists[1]["slot"], json!({ "type": "data", "value": 13 }));
    assert_eq!(
        artists[1].get("colors"),
        None,
        "absent colours are written: {}",
        artists[1]
    );
}

// Why: the two forms of stairs data are written by hand from the documented form, so each
// must be tagged by its name with its arrays named as in the IR; and the base of open
// stairs, which is absent in a plain step plot, must be left out rather than written as
// null, while a base of 0, which closes the outline at the axis, must be written.
#[test]
fn stairs_are_written_in_the_documented_json_form() {
    let (artists, _) = written_artists(vec![
        Artist::Stairs(Stairs {
            id: NodeId(3),
            data: StairsData::Edges {
                edges: NodeId(10),
                values: NodeId(11),
            },
            ..Stairs::default()
        }),
        Artist::Stairs(Stairs {
            id: NodeId(4),
            orientation: Orientation::Horizontal,
            data: StairsData::Samples {
                x: NodeId(12),
                values: NodeId(11),
                step: StepAt::Mid,
            },
            base: Some(0.0),
            fill: ColorSpec::Auto,
            fill_opacity: 0.5,
            ..Stairs::default()
        }),
    ]);
    assert_eq!(
        artists[0],
        json!({
            "type": "stairs",
            "id": 3,
            "label": null,
            "visible": true,
            "orientation": "vertical",
            "data": { "type": "edges", "edges": 10, "values": 11 },
            "line": { "color": { "type": "auto" }, "width_pt": 0.75, "dash": "solid" },
            "fill": { "type": "none" }
        })
    );
    let filled = &artists[1];
    assert_eq!(
        filled["data"],
        json!({ "type": "samples", "x": 12, "values": 11, "step": "mid" })
    );
    assert_eq!(filled["base"], json!(0.0));
    assert_eq!(filled["fill"], json!({ "type": "auto" }));
    assert_eq!(filled["fill_opacity"], json!(0.5));
}

// Why: cells are written by hand from the documented form, so unit edges must be a tag
// without a value and explicit edges a tagged reference, the gap an object of its two
// fractions, and the alpha and opacity, which are absent or 1 in almost every heatmap,
// left out as they are for a scalar image.
#[test]
fn cells_are_written_in_the_documented_json_form() {
    let (artists, _) = written_artists(vec![Artist::Cells(Cells {
        id: NodeId(3),
        values: NodeId(10),
        columns: CellEdges::Unit,
        rows: CellEdges::Edges(NodeId(11)),
        gap: CellGap { x: 0.25, y: 0.0 },
        color_scale: Some(NodeId(20)),
        ..Cells::default()
    })]);
    assert_eq!(
        artists[0],
        json!({
            "type": "cells",
            "id": 3,
            "label": null,
            "visible": true,
            "values": 10,
            "columns": { "type": "unit" },
            "rows": { "type": "edges", "value": 11 },
            "gap": { "x": 0.25, "y": 0.0 },
            "edge": { "color": { "type": "none" }, "width_pt": 0.5, "dash": "solid" },
            "color_scale": 20
        })
    );
}

// Why: a hand-written or web-written file leaves out what it does not set; bars without
// colours or an opacity, stairs without a base or a fill opacity, and cells without alpha
// or an opacity must load with the defaults of those properties (no colours, an open
// outline, no alpha, opaque) rather than be refused as incomplete.
#[test]
fn bars_stairs_and_cells_that_leave_out_their_defaulted_properties_load() {
    let mut fig = Figure::new();
    fig.axes.push(Axes {
        id: NodeId(2),
        ..Axes::default()
    });
    let mut value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    value["axes"][0]["artists"] = json!([
        {
            "type": "bar", "id": 3, "label": null, "visible": true,
            "orientation": "vertical", "positions": 10, "values": 11,
            "base": { "type": "constant", "value": 0.0 }, "slot": { "type": "pitch" },
            "gap": 0.2, "group": null, "face": { "type": "auto" },
            "edge": { "color": { "type": "none" }, "width_pt": 0.5, "dash": "solid" }
        },
        {
            "type": "stairs", "id": 4, "label": null, "visible": true,
            "orientation": "vertical",
            "data": { "type": "edges", "edges": 12, "values": 11 },
            "line": { "color": { "type": "auto" }, "width_pt": 0.75, "dash": "solid" },
            "fill": { "type": "none" }
        },
        {
            "type": "cells", "id": 5, "label": null, "visible": true, "values": 13,
            "columns": { "type": "unit" }, "rows": { "type": "unit" },
            "gap": { "x": 0.0, "y": 0.0 },
            "edge": { "color": { "type": "none" }, "width_pt": 0.5, "dash": "solid" },
            "color_scale": null
        }
    ]);
    let loaded = Figure::from_json(&value.to_string()).expect("the file loads");
    let artists = &loaded.axes[0].artists;
    assert_eq!(
        artists[0],
        Artist::Bar(Bar {
            id: NodeId(3),
            positions: NodeId(10),
            values: NodeId(11),
            ..Bar::default()
        })
    );
    assert_eq!(
        artists[1],
        Artist::Stairs(Stairs {
            id: NodeId(4),
            data: StairsData::Edges {
                edges: NodeId(12),
                values: NodeId(11),
            },
            ..Stairs::default()
        })
    );
    assert_eq!(
        artists[2],
        Artist::Cells(Cells {
            id: NodeId(5),
            values: NodeId(13),
            ..Cells::default()
        })
    );
}

// ---------------------------------------------------------------------------------
// Protocol Buffers
// ---------------------------------------------------------------------------------

/// A figure with one axes holding a default bar series, default stairs and default cells
/// that name data, with every optional property absent.
fn default_artists_figure() -> Figure {
    let mut fig = Figure::new();
    fig.axes.push(Axes {
        id: NodeId(2),
        artists: vec![
            Artist::Bar(Bar {
                id: NodeId(3),
                positions: NodeId(10),
                values: NodeId(11),
                ..Bar::default()
            }),
            Artist::Stairs(Stairs {
                id: NodeId(4),
                data: StairsData::Edges {
                    edges: NodeId(12),
                    values: NodeId(11),
                },
                ..Stairs::default()
            }),
            Artist::Cells(Cells {
                id: NodeId(5),
                values: NodeId(13),
                ..Cells::default()
            }),
        ],
        ..Axes::default()
    });
    fig
}

// Why: proto3 omits zero-valued fields, and the defaults of these artists are not zero in
// many places (a gap of 0.2, an opacity of 1, a visible artist, an edge 0.5 points wide
// in no colour). A writer in another language that leaves those fields out must get the
// defaults of the artist, not proto3's zeros or the defaults of another context: an absent
// gap is 0.2, not 0, which would join the bars; an absent bar edge is no edge, not the
// automatic edge that a line takes; and an absent orientation is vertical.
#[test]
fn absent_fields_of_bars_stairs_and_cells_decode_to_the_defaults_of_each_artist() {
    let fig = default_artists_figure();
    let mut wire_figure = wire::Figure::from(&fig);
    for artist in &mut wire_figure.axes[0].artists {
        match artist.kind.as_mut().expect("the encoder writes every kind") {
            wire::ArtistKind::Bar(bar) => {
                bar.visible = None;
                bar.orientation = 0;
                bar.base = None;
                bar.slot = None;
                bar.gap = None;
                bar.face = None;
                bar.edge = None;
                bar.face_opacity = None;
            }
            wire::ArtistKind::Stairs(stairs) => {
                stairs.visible = None;
                stairs.orientation = 0;
                stairs.line = None;
                stairs.fill = None;
                stairs.fill_opacity = None;
            }
            wire::ArtistKind::Cells(cells) => {
                cells.visible = None;
                cells.columns = None;
                cells.rows = None;
                cells.gap = None;
                cells.edge = None;
                cells.opacity = None;
            }
            other => panic!("unexpected artist {other:?}"),
        }
    }
    use prost::Message;
    let decoded = Figure::from_protobuf(&wire_figure.encode_to_vec()).unwrap();
    assert_eq!(decoded.axes[0].artists, fig.axes[0].artists);
}

// Why: a data reference has no default, because 0 is a valid identifier, so a writer that
// leaves out the positions or values of bars, the data of stairs or the values of cells
// would otherwise draw the wrong array; the reader must refuse the file and name the
// field, so that the author of the faulty writer can find it.
#[test]
fn absent_data_of_bars_stairs_and_cells_is_an_error_that_names_the_field() {
    type Removal = fn(&mut wire::ArtistKind);
    let cases: [(&str, usize, Removal); 4] = [
        ("axes[0].artists[0].bar.positions", 0, |kind| {
            if let wire::ArtistKind::Bar(bar) = kind {
                bar.positions = None;
            }
        }),
        ("axes[0].artists[0].bar.values", 0, |kind| {
            if let wire::ArtistKind::Bar(bar) = kind {
                bar.values = None;
            }
        }),
        ("axes[0].artists[1].stairs.data", 1, |kind| {
            if let wire::ArtistKind::Stairs(stairs) = kind {
                stairs.data = None;
            }
        }),
        ("axes[0].artists[2].cells.values", 2, |kind| {
            if let wire::ArtistKind::Cells(cells) = kind {
                cells.values = None;
            }
        }),
    ];
    use prost::Message;
    for (field, index, remove) in cases {
        let mut wire_figure = wire::Figure::from(&default_artists_figure());
        remove(
            wire_figure.axes[0].artists[index]
                .kind
                .as_mut()
                .expect("the encoder writes every kind"),
        );
        let result = Figure::from_protobuf(&wire_figure.encode_to_vec());
        assert!(
            matches!(
                &result,
                Err(IrError::Protobuf(ProtobufError::MissingField { field: found })) if found == field
            ),
            "removing {field} gave {result:?}"
        );
    }
}

// Why: a base of 0 and a gap of 0 are real values that differ from absence (a closed
// outline rather than an open one, and touching bars rather than the default gap), a base
// of negative zero is a different number in the binary format, an opacity of 0 hides
// rather than defaults, an empty group name is a group rather than none, and identifier 0
// is a valid reference; each must survive both formats as itself, together with every
// variant of the tagged values, rather than collapse to proto3's zero or to absence.
#[test]
fn zero_bases_gaps_and_every_variant_survive_a_protobuf_round_trip() {
    let mut fig = Figure::new();
    fig.axes.push(Axes {
        id: NodeId(2),
        artists: vec![
            Artist::Bar(Bar {
                id: NodeId(3),
                orientation: Orientation::Horizontal,
                positions: NodeId(10),
                values: NodeId(11),
                base: BarBase::Data(NodeId(0)),
                slot: BarSlot::Data(NodeId(0)),
                gap: 0.0,
                group: Some(BarGroup {
                    name: String::new(),
                    layout: BarLayout::Stacked,
                }),
                colors: vec![Color::WHITE, Color::BLACK],
                face_opacity: 0.0,
                ..Bar::default()
            }),
            Artist::Bar(Bar {
                id: NodeId(4),
                positions: NodeId(10),
                values: NodeId(11),
                base: BarBase::Constant(-0.0),
                slot: BarSlot::Width(0.25),
                group: Some(BarGroup {
                    name: "g".to_owned(),
                    layout: BarLayout::Grouped,
                }),
                ..Bar::default()
            }),
            Artist::Stairs(Stairs {
                id: NodeId(5),
                data: StairsData::Samples {
                    x: NodeId(0),
                    values: NodeId(11),
                    step: StepAt::Pre,
                },
                base: Some(0.0),
                fill: ColorSpec::Auto,
                fill_opacity: 0.0,
                ..Stairs::default()
            }),
            Artist::Cells(Cells {
                id: NodeId(6),
                values: NodeId(13),
                columns: CellEdges::Edges(NodeId(0)),
                rows: CellEdges::Unit,
                gap: CellGap { x: 0.0, y: 0.5 },
                color_scale: Some(NodeId(0)),
                alpha: Some(NodeId(0)),
                opacity: 0.0,
                ..Cells::default()
            }),
        ],
        ..Axes::default()
    });
    let restored = Figure::from_protobuf(&fig.to_protobuf()).expect("own output decodes");
    assert_eq!(restored, fig);
    assert_eq!(restored.to_protobuf(), fig.to_protobuf());
    let restored = Figure::from_json(&fig.to_json()).expect("own output parses");
    assert_eq!(restored, fig);
}

// ---------------------------------------------------------------------------------
// Colour mapping
// ---------------------------------------------------------------------------------

// Why: only cells map numbers through a colour scale; bars and stairs take fixed colours
// or the colour order. Automatic colour limits and colourbars are found from the artists
// that map through a scale, so cells must report their scale and their values there, and
// bars and stairs must report none, or a scale's automatic limits would include bar
// heights.
#[test]
fn only_cells_map_their_values_through_a_colour_scale() {
    let fx = fixture(false, cells(2, 3));
    let values = match &fx.fig.axes[0].artists[0] {
        Artist::Cells(cells) => cells.values,
        other => panic!("expected cells, found {other:?}"),
    };
    let artist = &fx.fig.axes[0].artists[0];
    assert_eq!(artist.color_scale(), Some(SCALE));
    assert_eq!(
        artist.color_mapping(),
        Some(ColorMapping {
            color_scale: Some(SCALE),
            values,
        })
    );
    assert_eq!(fx.fig.color_scale_values(SCALE), vec![values]);

    let fixed: [Make; 2] = [
        Box::new(bars(&[1.0, 2.0])),
        Box::new(stairs_on_edges(&[1.0, 2.0])),
    ];
    for make in fixed {
        let fx = fixture(false, make);
        let artist = &fx.fig.axes[0].artists[0];
        assert_eq!(artist.color_scale(), None, "{artist:?}");
        assert_eq!(artist.color_mapping(), None, "{artist:?}");
        assert_eq!(fx.fig.color_scale_values(SCALE), vec![]);
    }
}

// ---------------------------------------------------------------------------------
// Validation of bars
// ---------------------------------------------------------------------------------

// Why: a check that reports a correct chart is as harmful as a missing check, and these
// are the cases a user meets first: a single bar (whose slot is 1, having no neighbour),
// two bars at the same position (overlaid, as in MATLAB and Matplotlib), positions in no
// particular order (categories placed by a lookup rather than sorted), a missing value
// (NaN, which draws no bar for that position), bars below the base, touching bars (a gap
// of 0) and nearly invisible ones (a gap just below 1), floating bars, per-bar widths,
// per-bar colours, outlined bars without a face, and horizontal bars. None must raise an
// issue.
#[test]
fn the_ordinary_forms_of_a_bar_chart_raise_no_issue() {
    type Change = Box<dyn FnOnce(&mut Figure)>;
    let cases: Vec<(&str, &'static [f64], Change)> = vec![
        ("defaults", &[2.0, 5.0, 3.0], Box::new(|_| {})),
        ("a single bar", &[4.0], Box::new(|_| {})),
        (
            "two bars at one position",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                let positions = bar_mut(fig).positions;
                replace_array(fig, positions, NdArray::vector(vec![1.0, 1.0, 2.0]));
            }),
        ),
        // The pitch is the smallest distance between neighbouring distinct positions, in
        // value order rather than array order, so positions need not be sorted.
        (
            "unsorted, unevenly spaced positions",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                let positions = bar_mut(fig).positions;
                replace_array(fig, positions, NdArray::vector(vec![3.0, 0.5, 1.0]));
            }),
        ),
        ("a missing value", &[2.0, f64::NAN, 3.0], Box::new(|_| {})),
        (
            "values below the base",
            &[-2.0, 5.0, -3.0],
            Box::new(|_| {}),
        ),
        (
            "touching bars",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).gap = 0.0),
        ),
        (
            "nearly invisible bars",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).gap = 0.99),
        ),
        (
            "floating bars",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                let base = push_vector(fig, NodeId(50), &[1.0, -1.0, 0.5]);
                bar_mut(fig).base = BarBase::Data(base);
            }),
        ),
        (
            "a fixed width",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).slot = BarSlot::Width(0.25)),
        ),
        (
            "a width per bar",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                let widths = push_vector(fig, NodeId(50), &[0.5, 1.0, 2.0]);
                bar_mut(fig).slot = BarSlot::Data(widths);
            }),
        ),
        (
            "a colour per bar",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                bar_mut(fig).colors = vec![Color::BLACK, Color::WHITE, Color::BLACK];
            }),
        ),
        (
            "outlines without faces",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                let bar = bar_mut(fig);
                bar.face = ColorSpec::None;
                bar.edge.color = ColorSpec::Auto;
            }),
        ),
        (
            "horizontal bars",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).orientation = Orientation::Horizontal),
        ),
    ];
    for (name, values, change) in cases {
        let mut fx = fixture(false, bars(values));
        change(&mut fx.fig);
        assert_no_issues(&fx.fig.validate(), name);
    }
}

// Why: the gap is the fraction of each slot left empty, so a gap of 1 or more leaves no
// bar at all and a negative gap makes bars overlap their neighbours' slots; neither is a
// chart anyone means, and a renderer would draw nothing or garbage silently. Both ends of
// the boundary matter: 0 is valid (touching bars) and 1 is not.
#[test]
fn a_bar_gap_outside_zero_to_one_is_an_error_on_the_bars() {
    for gap in [1.0, 1.5, -0.01, f64::NAN, f64::INFINITY] {
        let mut fx = fixture(false, bars(&[2.0, 5.0]));
        bar_mut(&mut fx.fig).gap = gap;
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::InvalidGap,
            fx.artist,
            &format!("gap {gap}"),
        );
    }
}

// Why: a slot is a length along the category axis, so a fixed width or a width given per
// bar must be finite and positive; a zero width draws nothing and a negative one would
// flip the bar. Every width of the data is checked, not only the first.
#[test]
fn a_bar_slot_that_is_not_finite_and_positive_is_an_error_on_the_bars() {
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut fx = fixture(false, bars(&[2.0, 5.0]));
        bar_mut(&mut fx.fig).slot = BarSlot::Width(width);
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::InvalidBarSlot,
            fx.artist,
            &format!("width {width}"),
        );
    }
    for bad in [0.0, -0.5, f64::NAN, f64::NEG_INFINITY] {
        let mut fx = fixture(false, bars(&[2.0, 5.0, 3.0]));
        let widths = push_vector(&mut fx.fig, NodeId(50), &[0.5, 0.5, bad]);
        bar_mut(&mut fx.fig).slot = BarSlot::Data(widths);
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::InvalidBarSlot,
            fx.artist,
            &format!("a width of {bad} in the data"),
        );
    }
}

// Why: every array of a bar series holds one entry per bar, so arrays of different lengths
// leave bars without a position, a value, a base, a width or a colour; and each must be a
// vector, because a matrix of the same count has no order of bars that a reader could
// rely on.
#[test]
fn bar_arrays_that_do_not_hold_one_entry_per_bar_are_shape_mismatches() {
    type Change = fn(&mut Figure);
    let cases: [(&str, Change); 6] = [
        ("fewer values than positions", |fig| {
            let values = bar_mut(fig).values;
            replace_array(fig, values, NdArray::vector(vec![1.0, 2.0]));
        }),
        ("a base of another length", |fig| {
            let base = push_vector(fig, NodeId(50), &[0.0, 0.0]);
            bar_mut(fig).base = BarBase::Data(base);
        }),
        ("widths of another length", |fig| {
            let widths = push_vector(fig, NodeId(50), &[1.0, 1.0, 1.0, 1.0]);
            bar_mut(fig).slot = BarSlot::Data(widths);
        }),
        ("colours of another length", |fig| {
            bar_mut(fig).colors = vec![Color::BLACK];
        }),
        ("positions as a matrix", |fig| {
            let positions = bar_mut(fig).positions;
            replace_array(
                fig,
                positions,
                NdArray::from_shape(vec![1, 3], vec![1.0, 2.0, 3.0]).unwrap(),
            );
        }),
        ("values as a matrix", |fig| {
            let values = bar_mut(fig).values;
            replace_array(
                fig,
                values,
                NdArray::from_shape(vec![3, 1], vec![1.0, 2.0, 3.0]).unwrap(),
            );
        }),
    ];
    for (name, change) in cases {
        let mut fx = fixture(false, bars(&[2.0, 5.0, 3.0]));
        change(&mut fx.fig);
        let report = fx.fig.validate();
        assert!(
            has_error(&report, IssueKind::ShapeMismatch, fx.artist),
            "{name}: {report:?}"
        );
    }
}

// Why: only images and the values and alpha of cells read 8-bit values; every array of a
// bar series is read as floating-point numbers, so an array of bytes there is refused as
// the wrong element type rather than read as garbage.
#[test]
fn bar_arrays_of_bytes_are_element_type_mismatches() {
    type Change = fn(&mut Figure, NodeId);
    let cases: [(&str, Change); 4] = [
        ("positions", |fig, bytes| bar_mut(fig).positions = bytes),
        ("values", |fig, bytes| bar_mut(fig).values = bytes),
        ("base", |fig, bytes| {
            bar_mut(fig).base = BarBase::Data(bytes)
        }),
        ("widths", |fig, bytes| {
            bar_mut(fig).slot = BarSlot::Data(bytes)
        }),
    ];
    for (name, change) in cases {
        let mut fx = fixture(false, bars(&[2.0, 5.0, 3.0]));
        let bytes = push_bytes(&mut fx.fig, NodeId(50), vec![3], vec![1, 2, 3]);
        change(&mut fx.fig, bytes);
        drop_unused(&mut fx.fig);
        let report = fx.fig.validate();
        assert!(
            has_error(&report, IssueKind::ElementTypeMismatch, fx.artist),
            "{name}: {report:?}"
        );
    }
}

/// Removes the arrays that the only artist of a fixture no longer refers to, so that a
/// case that retargets a reference raises no unused-data warning.
fn drop_unused(fig: &mut Figure) {
    let used = common::data_references(&fig.axes[0].artists[0]);
    fig.data.retain(|data| used.contains(&data.id));
}

// Why: bars and stairs have no colour scale, so a colormapped face, edge, line or fill has
// no scale to look its colour up in; drawing it in some fallback colour would hide the
// mistake, so it is refused, naming the artist. Cells, which do map their values, are not
// affected.
#[test]
fn a_colormapped_colour_on_bars_or_stairs_is_an_error() {
    type Change = fn(&mut Figure);
    let bar_cases: [(&str, Change); 2] = [
        ("bar face", |fig| bar_mut(fig).face = ColorSpec::Colormapped),
        ("bar edge", |fig| {
            bar_mut(fig).edge.color = ColorSpec::Colormapped;
        }),
    ];
    for (name, change) in bar_cases {
        let mut fx = fixture(false, bars(&[2.0, 5.0]));
        change(&mut fx.fig);
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::ColormapNotSupported,
            fx.artist,
            name,
        );
    }
    let stairs_cases: [(&str, Change); 2] = [
        ("stairs line", |fig| {
            stairs_mut(fig).line.color = ColorSpec::Colormapped;
        }),
        ("stairs fill", |fig| {
            let stairs = stairs_mut(fig);
            stairs.base = Some(0.0);
            stairs.fill = ColorSpec::Colormapped;
        }),
    ];
    for (name, change) in stairs_cases {
        let mut fx = fixture(false, stairs_on_edges(&[2.0, 5.0]));
        change(&mut fx.fig);
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::ColormapNotSupported,
            fx.artist,
            name,
        );
    }
}

/// The values of a bar series of a [`bar_set`], and the change that makes it a member of
/// the set.
type Member = (&'static [f64], Box<dyn FnOnce(&mut Bar)>);

/// Builds a figure with one 2D axes holding bar series of the values given for each, at
/// positions 1, 2, …, n, each changed by its function, and returns it with the
/// identifiers of the series in order.
fn bar_set(series: Vec<Member>) -> (Figure, Vec<NodeId>) {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let n = series.first().map_or(0, |(values, _)| values.len());
    let positions: Vec<f64> = (1..=n).map(|k| k as f64).collect();
    let positions = b.vector(&positions);
    let mut ids = Vec::new();
    for (values, change) in series {
        let id = b.node();
        let mut bar = Bar {
            id,
            positions,
            values: b.vector(values),
            ..Bar::default()
        };
        change(&mut bar);
        b.push(axes, Artist::Bar(bar));
        ids.push(id);
    }
    (b.build(), ids)
}

/// A change that puts a bar series in the group of the given name and layout.
fn in_group(name: &'static str, layout: BarLayout) -> Box<dyn FnOnce(&mut Bar)> {
    Box::new(move |bar: &mut Bar| {
        bar.group = Some(BarGroup {
            name: name.to_owned(),
            layout,
        })
    })
}

// Why: a missing category is written as a non-finite position as readily as a missing
// value is written as NaN (a lookup that finds no category, or a column padded to a common
// length); such a bar has no place along the category axis, so it is not drawn, and it
// must not shrink the pitch of the others or be reported, or a chart with one unplaced
// category would carry a warning and draw its bars at a pitch of NaN.
#[test]
fn a_bar_at_a_non_finite_position_draws_nothing_and_raises_no_issue() {
    for position in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut fx = fixture(false, bars(&[2.0, 5.0, 3.0]));
        let positions = bar_mut(&mut fx.fig).positions;
        replace_array(
            &mut fx.fig,
            positions,
            NdArray::vector(vec![1.0, position, 3.0]),
        );
        assert_no_issues(&fx.fig.validate(), &format!("a bar at {position}"));
    }
    // Every position missing leaves no bar, which is still not an error: the bars have
    // values, but nowhere to stand.
    let mut fx = fixture(false, bars(&[2.0, 5.0]));
    let positions = bar_mut(&mut fx.fig).positions;
    replace_array(&mut fx.fig, positions, NdArray::vector(vec![f64::NAN; 2]));
    assert_eq!(fx.fig.validate().errors, vec![], "every position missing");
}

// Why: grouped bars are how several series are compared at each position (three models
// scored on four properties), and stacked bars are how parts add up to a total, negative
// parts included (Plotly's relative mode). Series that agree on everything the group
// shares must raise nothing, whatever their colours, labels and values, or the most
// common use of the feature would always carry a warning.
#[test]
fn bars_of_a_group_that_agree_raise_no_issue_grouped_or_stacked() {
    let models: [&'static [f64]; 3] = [
        &[0.8, 0.6, 0.9, 0.7],
        &[0.7, 0.9, 0.5, 0.8],
        &[0.9, 0.4, 0.6, f64::NAN],
    ];
    let (fig, _) = bar_set(
        models
            .into_iter()
            .map(|values| (values, in_group("models", BarLayout::Grouped)))
            .collect(),
    );
    assert_no_issues(&fig.validate(), "three grouped series");

    let parts: [&'static [f64]; 3] = [&[1.0, -2.0, 3.0], &[-1.5, 2.0, 0.5], &[2.0, 2.0, -4.0]];
    let (fig, _) = bar_set(
        parts
            .into_iter()
            .map(|values| (values, in_group("parts", BarLayout::Stacked)))
            .collect(),
    );
    assert_no_issues(&fig.validate(), "three stacked series with negative parts");
}

// Why: the members of a group share one layout, which is taken from the first member in
// artist order; a later member that differs in orientation, slot, gap, base or layout
// (grouped or stacked) is drawn by
// the first member's rule, not its own, so the user must be told which member decides and
// which property was overridden. It is a warning, because the group still draws. The
// warning belongs to the member that differs, and a member that agrees with the first is
// not reported.
#[test]
fn a_group_member_that_differs_from_the_first_is_a_warning_naming_the_first_and_the_property() {
    type Change = fn(&mut Bar);
    let cases: [(&str, Change); 5] = [
        ("orientation", |bar| {
            bar.orientation = Orientation::Horizontal
        }),
        ("slot", |bar| bar.slot = BarSlot::Width(0.5)),
        ("gap", |bar| bar.gap = 0.0),
        ("base", |bar| bar.base = BarBase::Constant(1.0)),
        ("layout", |bar| {
            if let Some(group) = &mut bar.group {
                group.layout = BarLayout::Stacked;
            }
        }),
    ];
    for (property, change) in cases {
        let members: Vec<Member> = vec![
            (&[1.0, 2.0], in_group("g", BarLayout::Grouped)),
            (
                &[2.0, 3.0],
                Box::new(move |bar: &mut Bar| {
                    bar.group = Some(BarGroup {
                        name: "g".to_owned(),
                        layout: BarLayout::Grouped,
                    });
                    change(bar);
                }),
            ),
            (&[3.0, 1.0], in_group("g", BarLayout::Grouped)),
        ];
        let (fig, ids) = bar_set(members);
        let report = fig.validate();
        assert_eq!(report.errors, vec![], "{property}: {report:?}");
        assert_eq!(
            warning_kinds(&report),
            [IssueKind::InconsistentBarGroup],
            "{property}: {report:?}"
        );
        let warning = &report.warnings[0];
        assert_eq!(warning.node, Some(ids[1]), "{property}: {warning:?}");
        let first = ids[0].to_string();
        assert!(
            warning.message.contains(&first) && warning.message.contains(property),
            "{property}: the message must name {first} and {property}: {}",
            warning.message
        );
    }
}

// Why: a group is formed by name within one axes, so two charts that happen to use the
// same group name in different axes are separate groups, and differences between them are
// not inconsistencies; bars without a group are never compared with anything.
#[test]
fn bars_sharing_a_group_name_in_different_axes_or_without_a_group_are_not_compared() {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(1, 2);
    let left = b.axes2d(0, 0);
    let right = b.axes2d(0, 1);
    let positions = b.vector(&[1.0, 2.0]);
    let values = b.vector(&[3.0, 4.0]);
    for (axes, gap, group) in [
        (left, 0.2, Some("g")),
        (right, 0.0, Some("g")),
        (right, 0.5, None),
    ] {
        let id = b.node();
        b.push(
            axes,
            Artist::Bar(Bar {
                id,
                positions,
                values,
                gap,
                group: group.map(|name| BarGroup {
                    name: name.to_owned(),
                    layout: BarLayout::Grouped,
                }),
                ..Bar::default()
            }),
        );
    }
    assert_no_issues(&b.build().validate(), "groups in different axes");
}

// Why: stacked members build on one another by position value, and grouped members share
// each position's slot, so the members of a group need not hold the same positions or the
// same number of bars: a series that lacks a category, or that adds one, is ordinary (a
// model not scored on one property, or a part present only in some totals). Positions are
// not among the properties that the first member decides, so such a group raises nothing.
#[test]
fn members_of_a_group_at_different_positions_raise_no_issue() {
    for layout in [BarLayout::Grouped, BarLayout::Stacked] {
        let mut b = FigureBuilder::new();
        let axes = b.axes2d(0, 0);
        for (positions, values) in [
            (&[1.0, 2.0, 3.0][..], &[3.0, -1.0, 2.0][..]),
            (&[2.0, 3.0, 4.0, 5.0][..], &[1.0, 2.0, -2.0, 4.0][..]),
            (&[3.0][..], &[0.5][..]),
        ] {
            let id = b.node();
            let positions = b.vector(positions);
            let values = b.vector(values);
            b.push(
                axes,
                Artist::Bar(Bar {
                    id,
                    positions,
                    values,
                    group: Some(BarGroup {
                        name: "g".to_owned(),
                        layout,
                    }),
                    ..Bar::default()
                }),
            );
        }
        assert_no_issues(&b.build().validate(), &format!("{layout:?}"));
    }
}

// Why: on a logarithmic value axis a bar that rises from 0 has no place to start, so it is
// drawn from the low edge of the plot area; the user must be told, and the message must
// say where the bar is drawn from, but the chart still draws, so it is a warning. A
// positive base raises nothing, and a logarithmic category axis says nothing about the
// base. The rule follows the orientation: the value axis of horizontal bars is x.
#[test]
fn a_non_positive_bar_base_on_a_logarithmic_value_axis_is_a_warning() {
    type Change = fn(&mut Figure);
    let warned: [(&str, Change); 3] = [
        ("a base of 0 under log y", |fig| {
            fig.axes[0].y.scale = Scale::Log;
        }),
        ("a base given as data with a 0 under log y", |fig| {
            fig.axes[0].y.scale = Scale::Log;
            let base = push_vector(fig, NodeId(50), &[1.0, 0.0]);
            bar_mut(fig).base = BarBase::Data(base);
        }),
        ("horizontal bars from 0 under log x", |fig| {
            fig.axes[0].x.scale = Scale::Log;
            bar_mut(fig).orientation = Orientation::Horizontal;
        }),
    ];
    for (name, change) in warned {
        let mut fx = fixture(false, bars(&[2.0, 5.0]));
        change(&mut fx.fig);
        let report = fx.fig.validate();
        assert_eq!(report.errors, vec![], "{name}: {report:?}");
        assert!(
            has_warning(&report, IssueKind::NonPositiveOnLogAxis, fx.artist),
            "{name}: {report:?}"
        );
        assert!(
            report.warnings.iter().any(
                |w| w.kind == IssueKind::NonPositiveOnLogAxis && w.message.contains("low edge")
            ),
            "{name}: the message must say the bars are drawn from the low edge: {report:?}"
        );
    }
    let quiet: [(&str, Change); 2] = [
        ("a base of 1 under log y", |fig| {
            fig.axes[0].y.scale = Scale::Log;
            bar_mut(fig).base = BarBase::Constant(1.0);
        }),
        ("vertical bars from 0 under log x", |fig| {
            fig.axes[0].x.scale = Scale::Log;
        }),
    ];
    for (name, change) in quiet {
        let mut fx = fixture(false, bars(&[2.0, 5.0]));
        change(&mut fx.fig);
        assert_no_issues(&fx.fig.validate(), name);
    }
}

// ---------------------------------------------------------------------------------
// Validation of stairs
// ---------------------------------------------------------------------------------

// Why: the ordinary forms of a step plot must raise nothing: increasing and decreasing
// edges (a histogram drawn from right to left), samples with each step position, a
// missing value, a closed and filled outline, and an outline closed at a base without a
// fill.
#[test]
fn the_ordinary_forms_of_stairs_raise_no_issue() {
    type Change = Box<dyn FnOnce(&mut Figure)>;
    let mut cases: Vec<(String, Change)> = vec![
        ("increasing edges".to_owned(), Box::new(|_| {})),
        (
            "decreasing edges".to_owned(),
            Box::new(|fig| {
                let StairsData::Edges { edges, .. } = stairs_mut(fig).data else {
                    unreachable!("the fixture is on edges");
                };
                replace_array(fig, edges, NdArray::vector(vec![3.0, 2.0, 1.0, 0.0]));
            }),
        ),
        (
            "a missing value".to_owned(),
            Box::new(|fig| {
                let StairsData::Edges { values, .. } = stairs_mut(fig).data else {
                    unreachable!("the fixture is on edges");
                };
                replace_array(fig, values, NdArray::vector(vec![1.0, f64::NAN, 2.0]));
            }),
        ),
        (
            "filled to a base".to_owned(),
            Box::new(|fig| {
                let stairs = stairs_mut(fig);
                stairs.base = Some(0.0);
                stairs.fill = ColorSpec::Auto;
                stairs.fill_opacity = 0.5;
            }),
        ),
        (
            "closed without a fill".to_owned(),
            Box::new(|fig| stairs_mut(fig).base = Some(-1.0)),
        ),
    ];
    for step in [StepAt::Pre, StepAt::Post, StepAt::Mid] {
        cases.push((
            format!("samples stepping {step:?}"),
            Box::new(move |fig| {
                let StairsData::Edges { values, .. } = stairs_mut(fig).data else {
                    unreachable!("the fixture is on edges");
                };
                let x = push_vector(fig, NodeId(50), &[1.0, 2.0, 4.0]);
                stairs_mut(fig).data = StairsData::Samples { x, values, step };
                drop_unused(fig);
            }),
        ));
    }
    for (name, change) in cases {
        let mut fx = fixture(false, stairs_on_edges(&[1.0, 3.0, 2.0]));
        change(&mut fx.fig);
        assert_no_issues(&fx.fig.validate(), &name);
    }
}

// Why: stairs on edges hold each value between two edges, so there must be exactly one
// more edge than values (numpy's histogram returns n + 1 edges for n counts); stairs on
// samples hold one value at each sample, so there must be as many samples as values.
#[test]
fn stairs_whose_edges_or_samples_do_not_match_the_values_are_shape_mismatches() {
    for edges in [vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0, 3.0, 4.0]] {
        let mut fx = fixture(false, stairs_on_edges(&[1.0, 3.0, 2.0]));
        let StairsData::Edges { edges: id, .. } = stairs_mut(&mut fx.fig).data else {
            unreachable!("the fixture is on edges");
        };
        let count = edges.len();
        replace_array(&mut fx.fig, id, NdArray::vector(edges));
        let report = fx.fig.validate();
        assert!(
            has_error(&report, IssueKind::ShapeMismatch, fx.artist),
            "{count} edges for 3 values: {report:?}"
        );
    }
    let mut fx = fixture(false, stairs_on_edges(&[1.0, 3.0, 2.0]));
    let StairsData::Edges { values, .. } = stairs_mut(&mut fx.fig).data else {
        unreachable!("the fixture is on edges");
    };
    let x = push_vector(&mut fx.fig, NodeId(50), &[1.0, 2.0]);
    stairs_mut(&mut fx.fig).data = StairsData::Samples {
        x,
        values,
        step: StepAt::Post,
    };
    drop_unused(&mut fx.fig);
    let report = fx.fig.validate();
    assert!(
        has_error(&report, IssueKind::ShapeMismatch, fx.artist),
        "2 samples for 3 values: {report:?}"
    );
}

// Why: the edges of stairs bound the steps in order, so edges that repeat, turn back or
// are not finite leave a step of no width, a step drawn over its neighbour, or a step
// with no place; each is refused as invalid edges rather than drawn as a tangle.
#[test]
fn stairs_edges_that_are_not_finite_and_strictly_monotonic_are_an_error() {
    for edges in [
        vec![0.0, 1.0, 1.0, 2.0],
        vec![0.0, 2.0, 1.0, 3.0],
        vec![0.0, f64::NAN, 2.0, 3.0],
        vec![0.0, 1.0, 2.0, f64::INFINITY],
    ] {
        let mut fx = fixture(false, stairs_on_edges(&[1.0, 3.0, 2.0]));
        let StairsData::Edges { edges: id, .. } = stairs_mut(&mut fx.fig).data else {
            unreachable!("the fixture is on edges");
        };
        let at = format!("{edges:?}");
        replace_array(&mut fx.fig, id, NdArray::vector(edges));
        assert_only_error(&fx.fig.validate(), IssueKind::InvalidEdges, fx.artist, &at);
    }
}

// Why: each sample of stairs holds its value up to the next sample (or from the previous
// one), so samples that repeat, turn back or are not finite leave a step of no width, a step
// drawn back over its neighbour, or a step with no place, as edges would; they are refused
// as invalid edges rather than drawn as a tangle. Samples running from right to left are
// valid, as decreasing edges are.
#[test]
fn stairs_samples_that_are_not_finite_and_strictly_monotonic_are_an_error() {
    let with_samples = |samples: &[f64]| {
        let mut fx = fixture(false, stairs_on_edges(&[1.0, 3.0, 2.0]));
        let StairsData::Edges { values, .. } = stairs_mut(&mut fx.fig).data else {
            unreachable!("the fixture is on edges");
        };
        let x = push_vector(&mut fx.fig, NodeId(50), samples);
        stairs_mut(&mut fx.fig).data = StairsData::Samples {
            x,
            values,
            step: StepAt::Mid,
        };
        drop_unused(&mut fx.fig);
        fx
    };
    for samples in [
        [0.0, 1.0, 1.0],
        [0.0, 2.0, 1.0],
        [0.0, f64::NAN, 2.0],
        [f64::NEG_INFINITY, 1.0, 2.0],
    ] {
        let fx = with_samples(&samples);
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::InvalidEdges,
            fx.artist,
            &format!("samples {samples:?}"),
        );
    }
    let fx = with_samples(&[3.0, 2.0, 0.5]);
    assert_no_issues(&fx.fig.validate(), "decreasing samples");
}

// Why: a fill is the area between the outline and its base, so open stairs, which have no
// base, have nothing to fill to; guessing 0 would draw an area the user did not ask for,
// so any fill other than none on stairs without a base is refused.
#[test]
fn a_fill_on_stairs_without_a_base_is_an_error() {
    for fill in [
        ColorSpec::Auto,
        ColorSpec::Rgba {
            color: Color::BLACK,
        },
    ] {
        let mut fx = fixture(false, stairs_on_edges(&[1.0, 3.0, 2.0]));
        stairs_mut(&mut fx.fig).fill = fill;
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::StairsFillWithoutBase,
            fx.artist,
            &format!("{fill:?}"),
        );
    }
}

// Why: on a logarithmic value axis a base at or below zero has no place, so the outline
// cannot drop to it; the user must be told, as for bars, but open stairs have no base to
// report, and the value axis of horizontal stairs is x.
#[test]
fn a_non_positive_stairs_base_on_a_logarithmic_value_axis_is_a_warning() {
    for (name, horizontal, base) in [
        ("a base of 0 under log y", false, 0.0),
        ("a base of -1 under log x for horizontal stairs", true, -1.0),
    ] {
        let mut fx = fixture(false, stairs_on_edges(&[1.0, 3.0, 2.0]));
        let stairs = stairs_mut(&mut fx.fig);
        stairs.base = Some(base);
        if horizontal {
            stairs.orientation = Orientation::Horizontal;
            fx.fig.axes[0].x.scale = Scale::Log;
        } else {
            fx.fig.axes[0].y.scale = Scale::Log;
        }
        let report = fx.fig.validate();
        assert_eq!(report.errors, vec![], "{name}: {report:?}");
        assert!(
            has_warning(&report, IssueKind::NonPositiveOnLogAxis, fx.artist),
            "{name}: {report:?}"
        );
    }
    let mut fx = fixture(false, stairs_on_edges(&[1.0, 3.0, 2.0]));
    fx.fig.axes[0].y.scale = Scale::Log;
    assert_no_issues(&fx.fig.validate(), "open stairs under log y");
}

// Why: stairs read their edges, samples and values as floating-point numbers, so an array
// of bytes is refused as the wrong element type.
#[test]
fn stairs_arrays_of_bytes_are_element_type_mismatches() {
    let mut fx = fixture(false, stairs_on_edges(&[1.0, 3.0, 2.0]));
    let bytes = push_bytes(&mut fx.fig, NodeId(50), vec![3], vec![1, 3, 2]);
    let StairsData::Edges { edges, .. } = stairs_mut(&mut fx.fig).data else {
        unreachable!("the fixture is on edges");
    };
    stairs_mut(&mut fx.fig).data = StairsData::Edges {
        edges,
        values: bytes,
    };
    drop_unused(&mut fx.fig);
    let report = fx.fig.validate();
    assert!(
        has_error(&report, IssueKind::ElementTypeMismatch, fx.artist),
        "{report:?}"
    );
}

// ---------------------------------------------------------------------------------
// Validation of cells
// ---------------------------------------------------------------------------------

// Why: the ordinary forms of a heatmap and a 2D histogram must raise nothing: unit edges,
// explicit increasing edges (histogram bins), decreasing edges, a single cell, missing
// values, counts of
// bytes with alpha of bytes, gaps of 0 and just below 1 in each direction, and outlined
// cells.
#[test]
fn the_ordinary_forms_of_cells_raise_no_issue() {
    type Change = Box<dyn FnOnce(&mut Figure)>;
    let cases: Vec<(&str, (usize, usize), Change)> = vec![
        ("unit edges", (2, 3), Box::new(|_| {})),
        (
            "explicit edges",
            (2, 3),
            Box::new(|fig| {
                let columns = push_vector(fig, NodeId(50), &[0.0, 0.5, 2.0, 10.0]);
                let rows = push_vector(fig, NodeId(51), &[-1.0, 0.0, 1.0]);
                let cells = cells_mut(fig);
                cells.columns = CellEdges::Edges(columns);
                cells.rows = CellEdges::Edges(rows);
            }),
        ),
        (
            "decreasing edges",
            (2, 3),
            Box::new(|fig| {
                let rows = push_vector(fig, NodeId(51), &[2.0, 1.0, 0.0]);
                cells_mut(fig).rows = CellEdges::Edges(rows);
            }),
        ),
        ("a single cell", (1, 1), Box::new(|_| {})),
        // An empty bin of a 2D histogram whose `min_count` is not met, or a cell of no
        // value, is NaN, which the scale's non-finite policy draws as nothing.
        (
            "missing values",
            (2, 3),
            Box::new(|fig| {
                let values = cells_mut(fig).values;
                replace_array(
                    fig,
                    values,
                    NdArray::from_shape(vec![2, 3], vec![1.0, f64::NAN, 3.0, 4.0, 5.0, f64::NAN])
                        .unwrap(),
                );
            }),
        ),
        (
            "counts and alpha of bytes",
            (2, 3),
            Box::new(|fig| {
                let counts = push_bytes(fig, NodeId(50), vec![2, 3], vec![0, 1, 2, 3, 4, 255]);
                let alpha = push_bytes(fig, NodeId(51), vec![2, 3], vec![255; 6]);
                let cells = cells_mut(fig);
                cells.values = counts;
                cells.alpha = Some(alpha);
                drop_unused(fig);
            }),
        ),
        (
            "gaps of 0 and just below 1",
            (2, 3),
            Box::new(|fig| cells_mut(fig).gap = CellGap { x: 0.0, y: 0.99 }),
        ),
        (
            "outlined cells",
            (2, 3),
            Box::new(|fig| {
                cells_mut(fig).edge.color = ColorSpec::Rgba {
                    color: Color::WHITE,
                };
            }),
        ),
        // Unlike bars and stairs, cells have a colour scale, so an outline can take the
        // colour of its cell, which outlines cells separated by a gap in their own colours.
        (
            "outlines in the colour of each cell",
            (2, 3),
            Box::new(|fig| cells_mut(fig).edge.color = ColorSpec::Colormapped),
        ),
    ];
    for (name, (ny, nx), change) in cases {
        let mut fx = fixture(false, cells(ny, nx));
        change(&mut fx.fig);
        assert_no_issues(&fx.fig.validate(), name);
    }
}

// Why: cells always map their values to colours, so cells that name no colour scale
// cannot be drawn, and a scale reference that names nothing or names a data array is a
// broken reference; each is refused as it is for a scalar image.
#[test]
fn cells_must_name_a_colour_scale_that_exists() {
    let mut fx = fixture(false, cells(2, 3));
    cells_mut(&mut fx.fig).color_scale = None;
    assert!(
        has_error(&fx.fig.validate(), IssueKind::MissingColorScale, fx.artist),
        "no scale"
    );
    let mut fx = fixture(false, cells(2, 3));
    cells_mut(&mut fx.fig).color_scale = Some(NodeId(777));
    assert!(
        has_error(&fx.fig.validate(), IssueKind::UnknownColorScale, fx.artist),
        "a scale that does not exist"
    );
    let mut fx = fixture(false, cells(2, 3));
    let values = cells_mut(&mut fx.fig).values;
    cells_mut(&mut fx.fig).color_scale = Some(values);
    assert!(
        has_error(&fx.fig.validate(), IssueKind::WrongNodeKind, fx.artist),
        "a data array as the scale"
    );
}

// Why: cells hold one value per cell of a grid, so the values must be a matrix, there must
// be one more column edge than columns and one more row edge than rows (row 0 lying
// between the first two row edges), and alpha must give every cell exactly one value.
#[test]
fn cells_whose_values_edges_or_alpha_do_not_fit_the_grid_are_shape_mismatches() {
    type Change = fn(&mut Figure);
    let cases: [(&str, Change); 6] = [
        ("values as a vector", |fig| {
            let values = cells_mut(fig).values;
            replace_array(fig, values, NdArray::vector(vec![1.0; 6]));
        }),
        ("values in three dimensions", |fig| {
            let values = cells_mut(fig).values;
            replace_array(
                fig,
                values,
                NdArray::from_shape(vec![1, 2, 3], vec![1.0; 6]).unwrap(),
            );
        }),
        ("as many column edges as columns", |fig| {
            let columns = push_vector(fig, NodeId(50), &[0.0, 1.0, 2.0]);
            cells_mut(fig).columns = CellEdges::Edges(columns);
        }),
        ("too many row edges", |fig| {
            let rows = push_vector(fig, NodeId(50), &[0.0, 1.0, 2.0, 3.0]);
            cells_mut(fig).rows = CellEdges::Edges(rows);
        }),
        ("alpha transposed", |fig| {
            let alpha = push_array(fig, NodeId(50), vec![3, 2], vec![1.0; 6]);
            cells_mut(fig).alpha = Some(alpha);
        }),
        ("alpha as a vector of the same count", |fig| {
            let alpha = push_vector(fig, NodeId(50), &[1.0; 6]);
            cells_mut(fig).alpha = Some(alpha);
        }),
    ];
    for (name, change) in cases {
        let mut fx = fixture(false, cells(2, 3));
        change(&mut fx.fig);
        let report = fx.fig.validate();
        assert!(
            has_error(&report, IssueKind::ShapeMismatch, fx.artist),
            "{name}: {report:?}"
        );
    }
}

// Why: the edges of cells bound each cell in order, along columns and along rows alike,
// so edges that repeat, turn back or are not finite are refused as invalid edges; and
// edges are read as floating-point numbers, so edges of bytes are the wrong element type.
#[test]
fn cell_edges_that_are_not_finite_and_strictly_monotonic_are_an_error() {
    for (name, edges) in [
        ("a repeated edge", vec![0.0, 1.0, 1.0, 2.0]),
        ("an edge that turns back", vec![0.0, 2.0, 1.0, 3.0]),
        ("a missing edge", vec![0.0, f64::NAN, 2.0, 3.0]),
        ("an infinite edge", vec![f64::NEG_INFINITY, 1.0, 2.0, 3.0]),
    ] {
        let mut fx = fixture(false, cells(3, 3));
        let id = push_vector(&mut fx.fig, NodeId(50), &edges);
        cells_mut(&mut fx.fig).columns = CellEdges::Edges(id);
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::InvalidEdges,
            fx.artist,
            &format!("columns with {name}"),
        );
        let mut fx = fixture(false, cells(3, 3));
        let id = push_vector(&mut fx.fig, NodeId(50), &edges);
        cells_mut(&mut fx.fig).rows = CellEdges::Edges(id);
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::InvalidEdges,
            fx.artist,
            &format!("rows with {name}"),
        );
    }
    let mut fx = fixture(false, cells(2, 3));
    let bytes = push_bytes(&mut fx.fig, NodeId(50), vec![4], vec![0, 1, 2, 3]);
    cells_mut(&mut fx.fig).columns = CellEdges::Edges(bytes);
    let report = fx.fig.validate();
    assert!(
        has_error(&report, IssueKind::ElementTypeMismatch, fx.artist),
        "edges of bytes: {report:?}"
    );
}

// Why: the gap of cells is the fraction of each cell's extent left empty, along each
// direction on its own, so a gap of 1 or more in either direction leaves nothing of the
// cells, and a negative gap overlaps them; both are refused as for bars.
#[test]
fn a_cell_gap_outside_zero_to_one_in_either_direction_is_an_error() {
    for gap in [
        CellGap { x: 1.0, y: 0.0 },
        CellGap { x: 0.0, y: 1.0 },
        CellGap { x: -0.1, y: 0.2 },
        CellGap {
            x: 0.2,
            y: f64::NAN,
        },
    ] {
        let mut fx = fixture(false, cells(2, 3));
        cells_mut(&mut fx.fig).gap = gap;
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::InvalidGap,
            fx.artist,
            &format!("{gap:?}"),
        );
    }
}

// Why: cells are placed by their edges, so on a logarithmic axis an edge at or below zero
// has no place. Unit edges run from −0.5, so unit cells on a logarithmic axis always have
// such an edge, and explicit edges may include 0; the user must be told, as for any
// non-positive data on a logarithmic axis. Positive explicit edges raise nothing.
#[test]
fn cell_edges_that_are_not_positive_on_a_logarithmic_axis_are_a_warning() {
    let mut fx = fixture(false, cells(2, 3));
    fx.fig.axes[0].x.scale = Scale::Log;
    let report = fx.fig.validate();
    assert_eq!(report.errors, vec![], "unit columns: {report:?}");
    assert!(
        has_warning(&report, IssueKind::NonPositiveOnLogAxis, fx.artist),
        "unit columns under log x: {report:?}"
    );

    let mut fx = fixture(false, cells(2, 3));
    fx.fig.axes[0].y.scale = Scale::Log;
    let rows = push_vector(&mut fx.fig, NodeId(50), &[0.0, 1.0, 10.0]);
    cells_mut(&mut fx.fig).rows = CellEdges::Edges(rows);
    let report = fx.fig.validate();
    assert!(
        has_warning(&report, IssueKind::NonPositiveOnLogAxis, fx.artist),
        "rows from 0 under log y: {report:?}"
    );

    let mut fx = fixture(false, cells(2, 3));
    fx.fig.axes[0].x.scale = Scale::Log;
    fx.fig.axes[0].y.scale = Scale::Log;
    let columns = push_vector(&mut fx.fig, NodeId(50), &[1.0, 10.0, 100.0, 1000.0]);
    let rows = push_vector(&mut fx.fig, NodeId(51), &[0.1, 1.0, 10.0]);
    let cells = cells_mut(&mut fx.fig);
    cells.columns = CellEdges::Edges(columns);
    cells.rows = CellEdges::Edges(rows);
    assert_no_issues(&fx.fig.validate(), "positive edges under log x and y");
}

// Why: cells map their values through their scale as a scalar image does, so a strict
// policy of the scale must refuse a value it cannot place; otherwise a categorical
// occupancy matrix with a code outside its categories would draw that cell in no colour
// without a word.
#[test]
fn a_cell_value_that_a_strict_scale_cannot_place_is_out_of_range() {
    let mut fx = fixture(false, cells(2, 3)); // values 1 to 6
    let scale = fx
        .fig
        .color_scales
        .iter_mut()
        .find(|scale| scale.id == SCALE)
        .expect("the fixture holds its scale");
    scale.kind = ColorScaleKind::Indexed(IndexedColorScale {
        colors: vec![Color::BLACK; 4],
        outside: ColorScaleOutOfRange::new(OutOfRange::Strict),
        ..IndexedColorScale::default()
    });
    assert!(
        has_error(&fx.fig.validate(), IssueKind::ValueOutOfRange, fx.artist),
        "values 4 to 6 name no entry of four"
    );
}

// Why: the occupancy matrix (#74) is the figure that cells exist to draw, so its exact
// form must validate cleanly before any later stage builds on it: whole-number codes, as
// bytes or as floats, mapped through a categorical scale (strict outside its categories,
// which every code names), on unit edges with a gap in both directions, in an axes whose
// x axis is hidden and whose y axis is reversed so that the first row is at the top.
// Ragged rows are padded with NaN, which a categorical scale's non-finite policy draws as
// nothing; padding must not be mistaken for a code outside the categories.
#[test]
fn an_occupancy_matrix_of_categorical_codes_raises_no_issue() {
    type Push = Box<dyn FnOnce(&mut Figure, NodeId)>;
    let codes: [(&str, Push); 2] = [
        (
            "codes as bytes",
            Box::new(|fig, id| {
                push_bytes(fig, id, vec![2, 3], vec![0, 1, 2, 2, 1, 0]);
            }),
        ),
        (
            "codes as floats with ragged rows padded by NaN",
            Box::new(|fig, id| {
                push_array(
                    fig,
                    id,
                    vec![2, 3],
                    vec![0.0, 1.0, 2.0, 2.0, f64::NAN, f64::NAN],
                );
            }),
        ),
    ];
    for (name, push) in codes {
        let mut fx = fixture(false, cells(2, 3));
        let scale = fx
            .fig
            .color_scales
            .iter_mut()
            .find(|scale| scale.id == SCALE)
            .expect("the fixture holds its scale");
        scale.kind = ColorScaleKind::Categorical(CategoricalColorScale {
            categories: ["absent", "partial", "full"]
                .map(|label| ColorCategory {
                    color: Color::BLACK,
                    label: Text::plain(label),
                })
                .to_vec(),
            ..CategoricalColorScale::default()
        });
        push(&mut fx.fig, NodeId(50));
        let cells = cells_mut(&mut fx.fig);
        cells.values = NodeId(50);
        cells.gap = CellGap { x: 0.2, y: 0.2 };
        drop_unused(&mut fx.fig);
        fx.fig.axes[0].x.visible = false;
        fx.fig.axes[0].y.reverse = true;
        assert_no_issues(&fx.fig.validate(), name);
    }
}

// ---------------------------------------------------------------------------------
// Validation common to bars, stairs and cells
// ---------------------------------------------------------------------------------

/// The three artists of this file, each made with valid data.
fn every_new_artist() -> Vec<(&'static str, Make)> {
    vec![
        ("bars", Box::new(bars(&[2.0, 5.0, 3.0]))),
        ("stairs", Box::new(stairs_on_edges(&[1.0, 3.0, 2.0]))),
        ("cells", Box::new(cells(2, 3))),
    ]
}

// Why: bars, stairs and cells are drawn in the plane of a two-dimensional axes; a 3D axes
// has no single category or value axis for bars and no plane for cells, so each is
// refused there, naming the artist, rather than drawn in some arbitrary plane.
#[test]
fn bars_stairs_and_cells_in_a_three_dimensional_axes_are_errors() {
    for (name, make) in every_new_artist() {
        let fx = fixture(true, make);
        let report = fx.fig.validate();
        assert!(
            has_error(&report, IssueKind::TwoDArtistInThreeDAxes, fx.artist),
            "{name}: {report:?}"
        );
    }
}

// Why: an opacity is a fraction of full coverage, so a value outside [0, 1] or a NaN has
// no meaning; it is refused on the faces of bars, the fill of stairs and cells alike,
// while both ends of the range, transparent and opaque, are accepted.
#[test]
fn an_opacity_outside_zero_to_one_is_an_error_on_bars_stairs_and_cells() {
    type SetOpacity = fn(&mut Artist, f64);
    let set: [SetOpacity; 3] = [
        |artist, value| {
            if let Artist::Bar(bar) = artist {
                bar.face_opacity = value;
            }
        },
        |artist, value| {
            if let Artist::Stairs(stairs) = artist {
                stairs.base = Some(0.0);
                stairs.fill = ColorSpec::Auto;
                stairs.fill_opacity = value;
            }
        },
        |artist, value| {
            if let Artist::Cells(cells) = artist {
                cells.opacity = value;
            }
        },
    ];
    for ((name, make), set) in every_new_artist().into_iter().zip(set) {
        let mut fx = fixture(false, make);
        for value in [0.0, 0.5, 1.0] {
            set(only_artist(&mut fx.fig), value);
            assert_no_issues(&fx.fig.validate(), &format!("{name} at {value}"));
        }
        for value in [-0.01, 1.01, f64::NAN, f64::INFINITY] {
            set(only_artist(&mut fx.fig), value);
            assert_only_error(
                &fx.fig.validate(),
                IssueKind::InvalidOpacity,
                fx.artist,
                &format!("{name} at {value}"),
            );
        }
    }
}

/// Asserts that a report has no errors and exactly one warning, `NothingToDraw` against
/// `artist`, whose message states the shape of the artist's values as every validation
/// message writes a shape (`[0]`, `[0, 3]`); `at` names the case.
#[track_caller]
fn assert_nothing_to_draw(report: &ValidationReport, artist: NodeId, shape: &[usize], at: &str) {
    assert_eq!(report.errors, vec![], "{at}: {report:?}");
    assert_eq!(
        warning_kinds(report),
        [IssueKind::NothingToDraw],
        "{at}: {report:?}"
    );
    let warning = &report.warnings[0];
    assert_eq!(
        warning.node,
        Some(artist),
        "{at}: the warning must name the artist"
    );
    let shape = format!("{shape:?}");
    assert!(
        warning.message.contains(&shape),
        "{at}: the message must state the shape {shape}: {}",
        warning.message
    );
}

// Why: a chart built before its data arrives, or a histogram of no samples, has bars of no
// values, stairs with no steps or cells with no rows or no columns. ADR 0012 decides that
// such data is valid, so the figure must not be refused, but the user reads `validate()`
// to learn why nothing was drawn, so each artist must be named with a warning that states
// the shape of its values.
#[test]
fn empty_bars_stairs_and_cells_are_a_warning_that_the_artist_draws_nothing() {
    let fx = fixture(false, bars(&[]));
    assert_nothing_to_draw(&fx.fig.validate(), fx.artist, &[0], "bars of no values");

    // A histogram of no bins still has its one edge.
    let fx = fixture(false, stairs_on_edges(&[]));
    assert_nothing_to_draw(&fx.fig.validate(), fx.artist, &[0], "stairs on one edge");

    let fx = fixture(false, |b, id| {
        Artist::Stairs(Stairs {
            id,
            data: StairsData::Samples {
                x: b.vector(&[]),
                values: b.vector(&[]),
                step: StepAt::Post,
            },
            ..Stairs::default()
        })
    });
    assert_nothing_to_draw(&fx.fig.validate(), fx.artist, &[0], "stairs of no samples");

    for (ny, nx) in [(0, 3), (2, 0), (0, 0)] {
        let fx = fixture(false, cells(ny, nx));
        assert_nothing_to_draw(
            &fx.fig.validate(),
            fx.artist,
            &[ny, nx],
            &format!("cells of shape [{ny}, {nx}]"),
        );
    }

    // Explicit edges that fit a grid of no columns: one edge.
    let mut fx = fixture(false, cells(2, 0));
    let columns = push_vector(&mut fx.fig, NodeId(50), &[5.0]);
    cells_mut(&mut fx.fig).columns = CellEdges::Edges(columns);
    assert_nothing_to_draw(
        &fx.fig.validate(),
        fx.artist,
        &[2, 0],
        "cells on one column edge",
    );
}

// Why: when an empty artist is also wrong, the error is what must be fixed, and a second
// report that it draws nothing would only bury it; so an artist with an error is never
// also reported as drawing nothing.
#[test]
fn an_empty_artist_with_an_error_is_reported_for_the_error_alone() {
    let mut fx = fixture(false, bars(&[]));
    bar_mut(&mut fx.fig).gap = 1.0;
    assert_only_error(&fx.fig.validate(), IssueKind::InvalidGap, fx.artist, "bars");

    let mut fx = fixture(false, stairs_on_edges(&[]));
    stairs_mut(&mut fx.fig).fill = ColorSpec::Auto;
    assert_only_error(
        &fx.fig.validate(),
        IssueKind::StairsFillWithoutBase,
        fx.artist,
        "stairs",
    );

    let mut fx = fixture(false, cells(0, 3));
    cells_mut(&mut fx.fig).color_scale = None;
    assert_only_error(
        &fx.fig.validate(),
        IssueKind::MissingColorScale,
        fx.artist,
        "cells",
    );
}
