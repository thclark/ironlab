//! Bars, stairs and cells: their defaults, their JSON and Protocol Buffers forms, and
//! their validation.
//!
//! A bar series draws one rectangle per value from a base to the base plus the value, of a
//! width and at an offset from its position along the category axis, and may record the
//! binning of the histogram it draws; stairs draw a stepped outline through values held
//! between edges or at samples; and cells fill a grid of rectangles, one per value, through a
//! colour scale. Stairs and cells may record a binning too. As in `validate.rs`, each
//! invalid case is the smallest change to an otherwise valid figure, so that a test fails
//! only if that specific check is missing or over-eager.

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
// each four fifths of the spacing between neighbouring positions wide so that a gap
// separates them, centred on its position with no offset, outlined by a thin black line (as
// the faces of a surface are) so that neighbouring bars of similar colours stay distinct,
// with no colour data and no colour scale (so that nothing is mapped and no scale is
// required), with faces and edges fully opaque, with a margin of 5 % beyond the ends of the
// bars so that the tallest bar does not touch the box, and with no binning record, because
// a plain bar chart was not made from samples. A default edge of the automatic colour would
// outline every bar in the colour of its own face and so be invisible; a default width of a
// whole pitch would join the bars into a histogram.
#[test]
fn a_default_bar_series_is_a_plain_bar_chart_in_the_colour_order() {
    let bar = Bar::default();
    assert_eq!(bar.label, None);
    assert!(bar.visible);
    assert_eq!(bar.orientation, Orientation::Vertical);
    assert_eq!(bar.base, PerBar::Constant(0.0));
    assert_eq!(bar.width, BarWidth::Pitch(0.8));
    assert_eq!(bar.offset, BarOffset::Widths(0.0));
    assert_eq!(bar.face, ColorSpec::Auto);
    assert_eq!(bar.color_data, None);
    assert_eq!(bar.color_scale, None);
    assert_eq!(
        bar.edge,
        LineStyle {
            color: ColorSpec::Rgba {
                color: Color::BLACK,
            },
            width_pt: 0.5,
            dash: DashStyle::Solid,
        }
    );
    assert_eq!(bar.face_opacity, 1.0);
    assert_eq!(bar.edge_opacity, 1.0);
    assert_eq!(bar.margin, 0.05);
    assert_eq!(bar.binning, None);
    assert_eq!(PerBar::default(), PerBar::Constant(0.0));
    assert_eq!(BarWidth::default(), BarWidth::Pitch(0.8));
    assert_eq!(BarOffset::default(), BarOffset::Widths(0.0));
    assert_eq!(Orientation::default(), Orientation::Vertical);
    // Stairs and cells drawn from values rather than samples carry no binning record.
    assert_eq!(Stairs::default().binning, None);
    assert_eq!(Cells::default().binning, None);
}

// Why: stairs are what a step histogram draws, whose default is an open outline in the
// colour order: no base (so the outline does not drop to an axis it was not asked to
// reach), no fill, an outline and fill that are fully opaque, and the step after each
// sample, as MATLAB's `stairs` and Matplotlib's `step(where="post")` place it.
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
    assert_eq!(stairs.line_opacity, 1.0);
    assert_eq!(stairs.fill_opacity, 1.0);
    assert_eq!(StepAt::default(), StepAt::Post);
}

// Why: cells are what a 2D histogram and a heatmap draw, whose default is a seamless grid
// of cells centred on whole numbers (so that cell k of a matrix sits at k, as an image's
// pixels do), with no gaps and no outlines, mapped through a scale that the program must
// name, without alpha, and with faces and edges fully opaque, as a surface's are.
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
    assert_eq!(cells.face_opacity, 1.0);
    assert_eq!(cells.edge_opacity, 1.0);
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
// the form of a default bar series is a contract: the tag `bar`, the base, the width and the
// offset each as a tagged value with its number, the margin written as every plain property
// is, the colour data and colour scale as null as a series writes them; the opacities, which
// are 1 in almost every chart, and the binning record, which a plain bar chart has not, are
// left out. The removed slot, gap and group must not be written, or a hand-written file
// copied from the output would carry properties the reader no longer has.
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
            "width": { "type": "pitch", "value": 0.8 },
            "offset": { "type": "widths", "value": 0.0 },
            "face": { "type": "auto" },
            "color_data": null,
            "color_scale": null,
            "edge": { "color": { "type": "rgba", "color": "#000000" }, "width_pt": 0.5, "dash": "solid" },
            "margin": 0.05
        })
    );
}

// Why: the variants that a hand-written file uses for floating bars (a base given as
// data), for fixed and per-bar widths, for offsets in widths and in data units and for bars
// coloured by data through a scale must be written as documented, with face and edge
// opacities of 0 written although they are falsy, because they hide the faces and edges
// rather than leaving them at the default, and a margin of 0 written, because it puts the
// axis limit at the end of the tallest bar; a list of per-bar colours, which colour data
// replaced, must not be written at all.
#[test]
fn the_variants_of_a_bar_series_are_written_in_the_documented_json_form() {
    let (artists, _) = written_artists(vec![
        Artist::Bar(Bar {
            id: NodeId(3),
            orientation: Orientation::Horizontal,
            positions: NodeId(10),
            values: NodeId(11),
            base: PerBar::Data(NodeId(12)),
            width: BarWidth::Constant(0.5),
            offset: BarOffset::Absolute(-0.25),
            face: ColorSpec::Colormapped,
            color_data: Some(NodeId(14)),
            color_scale: Some(NodeId(20)),
            edge: LineStyle {
                color: ColorSpec::Colormapped,
                ..Bar::default().edge
            },
            face_opacity: 0.0,
            edge_opacity: 0.0,
            margin: 0.0,
            ..Bar::default()
        }),
        Artist::Bar(Bar {
            id: NodeId(4),
            positions: NodeId(10),
            values: NodeId(11),
            width: BarWidth::Data(NodeId(13)),
            offset: BarOffset::Widths(-1.5),
            ..Bar::default()
        }),
    ]);
    let first = &artists[0];
    assert_eq!(first["orientation"], json!("horizontal"));
    assert_eq!(first["base"], json!({ "type": "data", "value": 12 }));
    assert_eq!(first["width"], json!({ "type": "constant", "value": 0.5 }));
    assert_eq!(
        first["offset"],
        json!({ "type": "absolute", "value": -0.25 })
    );
    assert_eq!(first["face"], json!({ "type": "colormapped" }));
    assert_eq!(first["color_data"], json!(14));
    assert_eq!(first["color_scale"], json!(20));
    assert_eq!(first["edge"]["color"], json!({ "type": "colormapped" }));
    assert_eq!(first["face_opacity"], json!(0.0));
    assert_eq!(first["edge_opacity"], json!(0.0));
    assert_eq!(first["margin"], json!(0.0));
    for gone in ["colors", "slot", "gap", "group"] {
        assert_eq!(first.get(gone), None, "{gone} is written: {first}");
    }
    assert_eq!(artists[1]["width"], json!({ "type": "data", "value": 13 }));
    assert_eq!(
        artists[1]["offset"],
        json!({ "type": "widths", "value": -1.5 })
    );
    for opacity in ["face_opacity", "edge_opacity"] {
        assert_eq!(
            artists[1].get(opacity),
            None,
            "an opacity of 1 is written: {}",
            artists[1]
        );
    }
}

/// A binning record of every kind of field: bins at explicit edges, a range, a
/// normalisation other than counts, a total and both counts of excluded samples.
fn edges_binning() -> Binning {
    Binning {
        bins: Bins::Edges(vec![0.0, 1.0, 3.0, 6.0]),
        edges: NodeId(14),
        range: Some([0.0, 6.0]),
        normalization: HistNorm::Density,
        total: 120.0,
        excluded_non_finite: 2,
        excluded_out_of_range: 5,
    }
}

/// A two-dimensional binning record with bins of each kind along x and y, ranges, a
/// normalisation, a minimum count and both counts of excluded samples.
fn grid_binning() -> Binning2d {
    Binning2d {
        x_bins: Bins::Count(4),
        y_bins: Bins::Width(0.5),
        x_edges: NodeId(15),
        y_edges: NodeId(16),
        x_range: Some([-1.0, 1.0]),
        y_range: None,
        normalization: HistNorm::Probability,
        min_count: Some(1.0),
        total: 40.0,
        excluded_non_finite: 0,
        excluded_out_of_range: 3,
    }
}

// Why: a histogram written by IronLAB records how its bins were made, and a reader in
// another language learns that only from the documented form: the record is written under
// `binning` with its bins as a tagged value (count, width, edges or a named rule), its range
// as a pair, its normalisation and its rule by their snake-case names, and its total and
// excluded counts as numbers; a range that was not given is written as null, as every
// absent plain value is. Bars, stairs and cells each carry the record of their own
// dimension, and the record survives JSON unchanged, or a figure saved and reopened would
// forget what its bars count.
#[test]
fn binning_records_are_written_in_the_documented_json_form_and_survive_a_round_trip() {
    let (artists, _) = written_artists(vec![
        Artist::Bar(Bar {
            id: NodeId(3),
            positions: NodeId(10),
            values: NodeId(11),
            binning: Some(edges_binning()),
            ..Bar::default()
        }),
        Artist::Stairs(Stairs {
            id: NodeId(4),
            data: StairsData::Edges {
                edges: NodeId(12),
                values: NodeId(11),
            },
            binning: Some(Binning {
                bins: Bins::Rule(BinRule::FreedmanDiaconis),
                edges: NodeId(12),
                range: None,
                normalization: HistNorm::CumulativeCount,
                total: 7.0,
                excluded_non_finite: 0,
                excluded_out_of_range: 0,
            }),
            ..Stairs::default()
        }),
        Artist::Cells(Cells {
            id: NodeId(5),
            values: NodeId(13),
            binning: Some(grid_binning()),
            ..Cells::default()
        }),
    ]);
    assert_eq!(
        artists[0]["binning"],
        json!({
            "bins": { "type": "edges", "value": [0.0, 1.0, 3.0, 6.0] },
            "edges": 14,
            "range": [0.0, 6.0],
            "normalization": "density",
            "total": 120.0,
            "excluded_non_finite": 2,
            "excluded_out_of_range": 5
        })
    );
    assert_eq!(
        artists[1]["binning"]["bins"],
        json!({ "type": "rule", "value": "freedman_diaconis" })
    );
    assert_eq!(artists[1]["binning"]["range"], json!(null));
    assert_eq!(
        artists[1]["binning"]["normalization"],
        json!("cumulative_count")
    );
    assert_eq!(
        artists[2]["binning"],
        json!({
            "x_bins": { "type": "count", "value": 4 },
            "y_bins": { "type": "width", "value": 0.5 },
            "x_edges": 15,
            "y_edges": 16,
            "x_range": [-1.0, 1.0],
            "y_range": null,
            "normalization": "probability",
            "min_count": 1.0,
            "total": 40.0,
            "excluded_non_finite": 0,
            "excluded_out_of_range": 3
        })
    );
}

// Why: a hand-written file names a rule and a normalisation by the words the documentation
// gives, so each must have exactly the snake-case name a reader expects; a variant written
// under another name would make the documented file unreadable.
#[test]
fn every_bin_rule_and_normalisation_is_written_by_its_documented_name() {
    for (rule, name) in [
        (BinRule::Auto, "auto"),
        (BinRule::Sturges, "sturges"),
        (BinRule::Scott, "scott"),
        (BinRule::FreedmanDiaconis, "freedman_diaconis"),
        (BinRule::Sqrt, "sqrt"),
        (BinRule::Integers, "integers"),
    ] {
        assert_eq!(serde_json::to_value(rule).unwrap(), json!(name));
    }
    for (norm, name) in [
        (HistNorm::Count, "count"),
        (HistNorm::Probability, "probability"),
        (HistNorm::Percentage, "percentage"),
        (HistNorm::CountDensity, "count_density"),
        (HistNorm::Density, "density"),
        (HistNorm::CumulativeCount, "cumulative_count"),
        (HistNorm::Cdf, "cdf"),
    ] {
        assert_eq!(serde_json::to_value(norm).unwrap(), json!(name));
    }
}

// Why: the two forms of stairs data are written by hand from the documented form, so each
// must be tagged by its name with its arrays named as in the IR; and the base of open
// stairs, which is absent in a plain step plot, must be left out rather than written as
// null, while a base of 0, which closes the outline at the axis, must be written. The line
// and fill opacities are left out at 1 and written otherwise, as a contour's are.
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
            line_opacity: 0.25,
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
    assert_eq!(filled["line_opacity"], json!(0.25));
    assert_eq!(filled["fill_opacity"], json!(0.5));
}

// Why: cells are written by hand from the documented form, so unit edges must be a tag
// without a value and explicit edges a tagged reference, the gap an object of its two
// fractions, and the alpha and the face and edge opacities, which are absent or 1 in almost
// every heatmap, left out as they are for a surface; an opacity other than 1 is written,
// each under its own name.
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
    let (artists, _) = written_artists(vec![Artist::Cells(Cells {
        id: NodeId(3),
        values: NodeId(10),
        face_opacity: 0.5,
        edge_opacity: 0.0,
        ..Cells::default()
    })]);
    assert_eq!(artists[0]["face_opacity"], json!(0.5));
    assert_eq!(artists[0]["edge_opacity"], json!(0.0));
    assert_eq!(artists[0].get("opacity"), None, "{}", artists[0]);
}

// Why: a hand-written or web-written file leaves out what it does not set; bars without
// colour data, a colour scale, opacities or a binning record, stairs without a base,
// opacities or a binning record, and cells without alpha, opacities or a binning record must
// load with the defaults of those properties (nothing mapped, an open outline, no alpha,
// opaque faces, edges, lines and fills, nothing binned) rather than be refused as incomplete.
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
            "base": { "type": "constant", "value": 0.0 },
            "width": { "type": "pitch", "value": 0.8 },
            "offset": { "type": "widths", "value": 0.0 },
            "face": { "type": "auto" },
            "edge": { "color": { "type": "rgba", "color": "#000000" }, "width_pt": 0.5, "dash": "solid" },
            "margin": 0.05
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
// many places (a width of four fifths of the pitch, a margin of 0.05, every opacity of 1, a
// visible artist, an edge 0.5 points wide in black for bars and in no colour for cells). A
// writer in another language that leaves those fields out must get the defaults of the
// artist, not proto3's zeros or the defaults of another context: an absent width is four
// fifths of the pitch, not a width of 0, which would draw nothing; an absent offset centres
// the bars on their positions; an absent margin is 0.05, not 0; an absent bar edge is a
// thin black outline and an absent cell edge is no edge, neither of them the automatic edge
// that a line takes; and an absent orientation is vertical.
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
                bar.width = None;
                bar.offset = None;
                bar.margin = None;
                bar.face = None;
                bar.edge = None;
                bar.face_opacity = None;
                bar.edge_opacity = None;
            }
            wire::ArtistKind::Stairs(stairs) => {
                stairs.visible = None;
                stairs.orientation = 0;
                stairs.line = None;
                stairs.fill = None;
                stairs.line_opacity = None;
                stairs.fill_opacity = None;
            }
            wire::ArtistKind::Cells(cells) => {
                cells.visible = None;
                cells.columns = None;
                cells.rows = None;
                cells.gap = None;
                cells.edge = None;
                cells.face_opacity = None;
                cells.edge_opacity = None;
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

// Why: a base of 0, an offset of 0 and a margin of 0 are real values that differ from
// absence (a closed outline rather than an open one, bars centred on purpose, an axis that
// ends at the tallest bar), a base of negative zero is a different number in the binary
// format, every opacity of 0 hides rather than defaults, and identifier 0 is a valid
// reference (as the colour data, colour scale, bases and widths of bars too); each must
// survive both formats as itself, together with every variant of the tagged values, rather
// than collapse to proto3's zero or to absence.
#[test]
fn zero_bases_offsets_margins_and_every_variant_survive_a_protobuf_round_trip() {
    let mut fig = Figure::new();
    fig.axes.push(Axes {
        id: NodeId(2),
        artists: vec![
            Artist::Bar(Bar {
                id: NodeId(3),
                orientation: Orientation::Horizontal,
                positions: NodeId(10),
                values: NodeId(11),
                base: PerBar::Data(NodeId(0)),
                width: BarWidth::Data(NodeId(0)),
                offset: BarOffset::Absolute(0.0),
                face: ColorSpec::Colormapped,
                color_data: Some(NodeId(0)),
                color_scale: Some(NodeId(0)),
                face_opacity: 0.0,
                edge_opacity: 0.0,
                margin: 0.0,
                ..Bar::default()
            }),
            Artist::Bar(Bar {
                id: NodeId(4),
                positions: NodeId(10),
                values: NodeId(11),
                base: PerBar::Constant(-0.0),
                width: BarWidth::Constant(0.25),
                offset: BarOffset::Widths(0.0),
                ..Bar::default()
            }),
            Artist::Bar(Bar {
                id: NodeId(7),
                positions: NodeId(10),
                values: NodeId(11),
                width: BarWidth::Pitch(1.0),
                offset: BarOffset::Widths(-0.5),
                margin: 0.25,
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
                line_opacity: 0.0,
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
                face_opacity: 0.0,
                edge_opacity: 0.0,
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

/// Every kind of bins, every bin rule and every normalisation, in records whose totals,
/// counts and ranges include the zeros that proto3 would otherwise drop.
fn every_binning() -> Vec<Binning> {
    let rules = [
        BinRule::Auto,
        BinRule::Sturges,
        BinRule::Scott,
        BinRule::FreedmanDiaconis,
        BinRule::Sqrt,
        BinRule::Integers,
    ];
    let norms = [
        HistNorm::Count,
        HistNorm::Probability,
        HistNorm::Percentage,
        HistNorm::CountDensity,
        HistNorm::Density,
        HistNorm::CumulativeCount,
        HistNorm::Cdf,
    ];
    let mut bins = vec![
        Bins::Count(0),
        Bins::Count(12),
        Bins::Width(0.25),
        Bins::Edges(vec![]),
        Bins::Edges(vec![-1.0, 0.0, 2.5]),
    ];
    bins.extend(rules.map(Bins::Rule));
    bins.into_iter()
        .zip(norms.into_iter().cycle())
        .enumerate()
        .map(|(k, (bins, normalization))| Binning {
            bins,
            edges: NodeId(14),
            range: (k % 2 == 0).then_some([0.0, k as f64]),
            normalization,
            total: if k == 0 { 0.0 } else { 10.0 * k as f64 },
            excluded_non_finite: k as u64 % 3,
            excluded_out_of_range: if k == 1 { u64::MAX } else { 0 },
        })
        .collect()
}

// Why: a binning record is what lets a datatip recover the count behind a bar of a
// normalised histogram, so every field of it must survive both formats exactly: every kind
// of bins and every rule and normalisation by its own variant, a count of bins or of
// excluded samples of 0 as 0 rather than as absence, a count too large for a double as
// itself, a range that was given as given and one that was not as absent, on bars, stairs
// and cells alike; and a record of cells keeps its two dimensions apart. A record that
// decoded differently would make the datatips of a reopened figure report other counts.
#[test]
fn every_binning_record_survives_a_protobuf_and_a_json_round_trip_on_bars_stairs_and_cells() {
    let mut artists = Vec::new();
    for (k, binning) in every_binning().into_iter().enumerate() {
        let id = NodeId(100 + 3 * k as u64);
        artists.push(Artist::Bar(Bar {
            id,
            positions: NodeId(10),
            values: NodeId(11),
            binning: Some(binning.clone()),
            ..Bar::default()
        }));
        artists.push(Artist::Stairs(Stairs {
            id: NodeId(id.0 + 1),
            data: StairsData::Edges {
                edges: NodeId(12),
                values: NodeId(11),
            },
            binning: Some(binning.clone()),
            ..Stairs::default()
        }));
        artists.push(Artist::Cells(Cells {
            id: NodeId(id.0 + 2),
            values: NodeId(13),
            binning: Some(Binning2d {
                x_bins: binning.bins.clone(),
                y_bins: Bins::Count(k as u32),
                x_edges: NodeId(15),
                y_edges: NodeId(16),
                x_range: binning.range,
                y_range: binning.range.map(|[lo, hi]| [lo - 1.0, hi + 1.0]),
                normalization: binning.normalization,
                min_count: (k % 3 == 0).then_some(k as f64),
                total: binning.total,
                excluded_non_finite: binning.excluded_non_finite,
                excluded_out_of_range: binning.excluded_out_of_range,
            }),
            ..Cells::default()
        }));
    }
    let mut fig = Figure::new();
    fig.axes.push(Axes {
        id: NodeId(2),
        artists,
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

/// Gives the bar series of a fixture colour data of the given values, added under the
/// identifier 60, and names [`SCALE`] as its colour scale.
fn give_colour_data(fig: &mut Figure, values: &[f64]) -> NodeId {
    let data = push_vector(fig, NodeId(60), values);
    let bar = bar_mut(fig);
    bar.color_data = Some(data);
    bar.color_scale = Some(SCALE);
    data
}

// Why: automatic colour limits and colourbars are found from the artists that map numbers
// through a scale, so every artist that maps must report its scale and its numbers there,
// and every other artist must report none. Cells map their values; bars map their colour
// data whenever they have some, whatever their colours, as a series does, so that the
// limits of the scale cover the colours a colormapped face or edge would take; bars
// without colour data, and stairs, which take fixed colours or the colour order only, map
// nothing, or a scale's automatic limits would include bar heights or stair values.
#[test]
fn cells_and_bars_with_colour_data_map_through_a_colour_scale_and_stairs_never_do() {
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

    for face in [ColorSpec::Colormapped, ColorSpec::Auto] {
        let mut fx = fixture(false, bars(&[2.0, 5.0, 3.0]));
        let data = give_colour_data(&mut fx.fig, &[0.0, 1.0, 2.0]);
        bar_mut(&mut fx.fig).face = face;
        let artist = &fx.fig.axes[0].artists[0];
        assert_eq!(artist.color_scale(), Some(SCALE), "{face:?}");
        assert_eq!(
            artist.color_mapping(),
            Some(ColorMapping {
                color_scale: Some(SCALE),
                values: data,
            }),
            "{face:?}"
        );
        assert_eq!(fx.fig.color_scale_values(SCALE), vec![data], "{face:?}");
    }

    // A bar series without colour data maps nothing even when it names a scale and its
    // face is colormapped, because it has no number to map.
    let mut fx = fixture(false, bars(&[1.0, 2.0]));
    let bar = bar_mut(&mut fx.fig);
    bar.face = ColorSpec::Colormapped;
    bar.color_scale = Some(SCALE);
    let artist = &fx.fig.axes[0].artists[0];
    assert_eq!(artist.color_scale(), Some(SCALE));
    assert_eq!(artist.color_mapping(), None);
    assert_eq!(fx.fig.color_scale_values(SCALE), vec![]);

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
// are the cases a user meets first: a single bar (whose pitch is 1, having no neighbour),
// two bars at the same position (overlaid, as in MATLAB and Matplotlib), positions in no
// particular order (categories placed by a lookup rather than sorted), a missing value
// (NaN, which draws no bar for that position), negative lengths, touching bars (a width of
// the whole pitch), nearly invisible ones, bars wider than the pitch that overlap their
// neighbours, offsets in widths and in data units either way (how grouped bars are placed),
// floating bars, per-bar widths, no margin and a large one, a binning record of a
// histogram, bars and outlines coloured by data through a scale, outlined bars without a
// face, and horizontal bars. None must raise an issue.
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
        ("negative lengths", &[-2.0, 5.0, -3.0], Box::new(|_| {})),
        (
            "touching bars",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).width = BarWidth::Pitch(1.0)),
        ),
        (
            "nearly invisible bars",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).width = BarWidth::Pitch(0.01)),
        ),
        (
            "bars wider than the pitch",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).width = BarWidth::Pitch(1.5)),
        ),
        (
            "an offset of a negative number of widths",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).offset = BarOffset::Widths(-1.0)),
        ),
        (
            "an offset of a fractional number of widths",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).offset = BarOffset::Widths(2.5)),
        ),
        (
            "an absolute offset",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).offset = BarOffset::Absolute(-0.3)),
        ),
        (
            "floating bars",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                let base = push_vector(fig, NodeId(50), &[1.0, -1.0, 0.5]);
                bar_mut(fig).base = PerBar::Data(base);
            }),
        ),
        (
            "bars hanging from a raised base",
            &[-2.0, -5.0, 3.0],
            Box::new(|fig| bar_mut(fig).base = PerBar::Constant(10.0)),
        ),
        (
            "a fixed width",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).width = BarWidth::Constant(0.25)),
        ),
        (
            "a width per bar",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                let widths = push_vector(fig, NodeId(50), &[0.5, 1.0, 2.0]);
                bar_mut(fig).width = BarWidth::Data(widths);
            }),
        ),
        (
            "no margin",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).margin = 0.0),
        ),
        (
            "a margin larger than the bars",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| bar_mut(fig).margin = 2.0),
        ),
        (
            "a histogram's binning record",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                let edges = push_vector(fig, NodeId(51), &[0.5, 1.5, 2.5, 3.5]);
                bar_mut(fig).binning = Some(Binning {
                    bins: Bins::Edges(vec![0.5, 1.5, 2.5, 3.5]),
                    edges,
                    range: Some([0.5, 3.5]),
                    normalization: HistNorm::Count,
                    total: 10.0,
                    excluded_non_finite: 1,
                    excluded_out_of_range: 0,
                })
            }),
        ),
        // An empty sample is valid (ADR 0012), so a record of no samples counted is too.
        (
            "the binning record of an empty sample",
            &[0.0, 0.0, 0.0],
            Box::new(|fig| {
                let edges = unit_bin_edges(fig, NodeId(51), 3);
                bar_mut(fig).binning = Some(Binning {
                    bins: Bins::Rule(BinRule::Auto),
                    edges,
                    range: None,
                    normalization: HistNorm::Density,
                    total: 0.0,
                    excluded_non_finite: 0,
                    excluded_out_of_range: 0,
                })
            }),
        ),
        (
            "faces coloured by data",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                give_colour_data(fig, &[0.5, 1.0, 0.0]);
                bar_mut(fig).face = ColorSpec::Colormapped;
            }),
        ),
        (
            "outlines coloured by data around faces without a colour",
            &[2.0, 5.0, 3.0],
            Box::new(|fig| {
                give_colour_data(fig, &[0.5, 1.0, 0.0]);
                let bar = bar_mut(fig);
                bar.face = ColorSpec::None;
                bar.edge.color = ColorSpec::Colormapped;
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

// Why: a width is a length along the category axis, so a fraction of the pitch, a fixed
// width and a width given per bar must each be finite and positive; a zero width draws
// nothing and a negative one would flip the bar. Every width of the data is checked, not
// only the first. A bar no longer has a gap, so no width can be refused as one.
#[test]
fn a_bar_width_that_is_not_finite_and_positive_is_an_error_on_the_bars() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        for width in [BarWidth::Pitch(bad), BarWidth::Constant(bad)] {
            let mut fx = fixture(false, bars(&[2.0, 5.0]));
            bar_mut(&mut fx.fig).width = width;
            assert_only_error(
                &fx.fig.validate(),
                IssueKind::InvalidBarWidth,
                fx.artist,
                &format!("{width:?}"),
            );
        }
    }
    for bad in [0.0, -0.5, f64::NAN, f64::NEG_INFINITY] {
        let mut fx = fixture(false, bars(&[2.0, 5.0, 3.0]));
        let widths = push_vector(&mut fx.fig, NodeId(50), &[0.5, 0.5, bad]);
        bar_mut(&mut fx.fig).width = BarWidth::Data(widths);
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::InvalidBarWidth,
            fx.artist,
            &format!("a width of {bad} in the data"),
        );
    }
}

// Why: an offset places every bar of a series beside its position, so an offset that is not
// finite has no place: in widths it would multiply every width into NaN or infinity, and in
// data units it would put every bar nowhere. Both forms are refused, naming the series; a
// negative or zero offset is an ordinary placement and is not.
#[test]
fn a_bar_offset_that_is_not_finite_is_an_error_on_the_bars() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for offset in [BarOffset::Widths(bad), BarOffset::Absolute(bad)] {
            let mut fx = fixture(false, bars(&[2.0, 5.0]));
            bar_mut(&mut fx.fig).offset = offset;
            assert_only_error(
                &fx.fig.validate(),
                IssueKind::InvalidBarOffset,
                fx.artist,
                &format!("{offset:?}"),
            );
        }
    }
}

// Why: the margin is a fraction of the span of the bars added beyond their ends, so a
// negative margin would cut the tallest bar off at the box and a margin that is not finite
// would give the axis no limit at all; both are refused, naming the series. A margin of 0
// is the limit at the end of the tallest bar, which is valid.
#[test]
fn a_bar_margin_that_is_negative_or_not_finite_is_an_error_on_the_bars() {
    for margin in [-0.01, -1.0, f64::NAN, f64::INFINITY] {
        let mut fx = fixture(false, bars(&[2.0, 5.0]));
        bar_mut(&mut fx.fig).margin = margin;
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::InvalidBarMargin,
            fx.artist,
            &format!("margin {margin}"),
        );
    }
}

/// The binning record of a valid count histogram of ten samples in three unit bins.
fn counted() -> Binning {
    Binning {
        bins: Bins::Count(3),
        edges: NodeId(5002),
        range: Some([0.0, 3.0]),
        normalization: HistNorm::Count,
        total: 10.0,
        excluded_non_finite: 0,
        excluded_out_of_range: 0,
    }
}

/// Stores the edges `0, 1, …, n` of `n` unit bins in a built figure under the given
/// identifier and returns it.
fn unit_bin_edges(fig: &mut Figure, id: NodeId, n: usize) -> NodeId {
    let edges: Vec<f64> = (0..=n).map(|k| k as f64).collect();
    push_vector(fig, id, &edges)
}

// Why: a datatip finds the bin of a bar, a step or a cell among the resolved edges of the
// record and divides by that bin's width or area, so the edges must be an array of the
// figure, finite and strictly increasing, and one more than the bins the artist draws: one
// more than the bars or the values of stairs, and one more than the columns along x and the
// rows along y of cells. Edges that are missing, that bound no bin or that do not match the
// bins drawn would make every reading wrong, so each is refused against the artist that
// holds the record: a missing array as unknown data, edges that are not finite and
// increasing as invalid edges (bin edges may not decrease, as the bins of a record may not),
// and edges of the wrong number or shape as a shape mismatch.
#[test]
fn binning_edges_that_are_missing_bound_no_bins_or_miss_the_bins_drawn_are_errors() {
    let cases: [(&str, Option<NdArray>, IssueKind); 6] = [
        ("a missing array", None, IssueKind::UnknownData),
        (
            "an edge of NaN",
            Some(NdArray::vector(vec![0.0, 1.0, f64::NAN, 3.0])),
            IssueKind::InvalidEdges,
        ),
        (
            "edges out of order",
            Some(NdArray::vector(vec![0.0, 2.0, 1.0, 3.0])),
            IssueKind::InvalidEdges,
        ),
        (
            "edges that decrease",
            Some(NdArray::vector(vec![3.0, 2.0, 1.0, 0.0])),
            IssueKind::InvalidEdges,
        ),
        (
            "one edge too few",
            Some(NdArray::vector(vec![0.0, 1.0, 2.0])),
            IssueKind::ShapeMismatch,
        ),
        (
            "edges that are not a vector",
            Some(NdArray {
                shape: vec![2, 2],
                values: Values::F64(vec![0.0, 1.0, 2.0, 3.0]),
            }),
            IssueKind::ShapeMismatch,
        ),
    ];
    let place = |fig: &mut Figure, array: &Option<NdArray>| match array {
        Some(array) => {
            fig.data.push(DataArray {
                id: NodeId(5000),
                name: None,
                array: array.clone(),
            });
            NodeId(5000)
        }
        None => NodeId(999_999),
    };
    for (name, array, kind) in &cases {
        // Three bars, three steps and three columns each need four edges.
        let mut fx = fixture(false, bars(&[2.0, 5.0, 3.0]));
        let edges = place(&mut fx.fig, array);
        bar_mut(&mut fx.fig).binning = Some(Binning { edges, ..counted() });
        assert_only_error(
            &fx.fig.validate(),
            *kind,
            fx.artist,
            &format!("bars: {name}"),
        );

        let mut fx = fixture(false, stairs_on_edges(&[2.0, 5.0, 3.0]));
        let edges = place(&mut fx.fig, array);
        stairs_mut(&mut fx.fig).binning = Some(Binning { edges, ..counted() });
        assert_only_error(
            &fx.fig.validate(),
            *kind,
            fx.artist,
            &format!("stairs: {name}"),
        );

        for along_x in [true, false] {
            let mut fx = fixture(false, cells(3, 3));
            let spoilt = place(&mut fx.fig, array);
            let good = unit_bin_edges(&mut fx.fig, NodeId(5001), 3);
            let (x_edges, y_edges) = if along_x {
                (spoilt, good)
            } else {
                (good, spoilt)
            };
            cells_mut(&mut fx.fig).binning = Some(Binning2d {
                x_bins: Bins::Count(3),
                y_bins: Bins::Count(3),
                x_edges,
                y_edges,
                total: 10.0,
                ..Binning2d::default()
            });
            assert_only_error(
                &fx.fig.validate(),
                *kind,
                fx.artist,
                &format!("cells, along x {along_x}: {name}"),
            );
        }
    }
    // The edges of the cells are matched to the columns along x and to the rows along y,
    // which differ in number here.
    let mut fx = fixture(false, cells(2, 3));
    let x_edges = unit_bin_edges(&mut fx.fig, NodeId(5000), 3);
    let y_edges = unit_bin_edges(&mut fx.fig, NodeId(5001), 2);
    cells_mut(&mut fx.fig).binning = Some(Binning2d {
        x_bins: Bins::Count(3),
        y_bins: Bins::Count(2),
        x_edges,
        y_edges,
        total: 10.0,
        ..Binning2d::default()
    });
    assert_no_issues(&fx.fig.validate(), "cells of two rows and three columns");
    cells_mut(&mut fx.fig).binning.as_mut().unwrap().x_edges = y_edges;
    fx.fig.data.retain(|data| data.id != x_edges);
    assert_only_error(
        &fx.fig.validate(),
        IssueKind::ShapeMismatch,
        fx.artist,
        "row edges given as the column edges",
    );
}

// Why: a binning record is read back to recover counts from normalised values, so it must
// describe bins that can exist: a total of samples that is negative or not finite would
// turn every recovered count into nonsense, a bin width that is not finite and positive
// cannot tile a range, a range that is not finite or does not increase holds no bin, and
// edges that are not finite and strictly increasing bound no bin. Bin edges are a
// specification of how samples were counted, which every binning library gives in
// increasing order, so unlike the edges of stairs and cells, which may be drawn from right
// to left, they may not decrease. Each is refused against the artist that holds the record,
// bars, stairs or cells alike; edges with the kind that edges of stairs and cells already
// raise, and everything else as an invalid binning record.
#[test]
fn a_binning_record_that_describes_no_possible_bins_is_an_error_on_its_artist() {
    type Spoil = fn(&mut Binning);
    let cases: [(&str, IssueKind, Spoil); 11] = [
        ("a negative total", IssueKind::InvalidBinning, |b| {
            b.total = -1.0
        }),
        ("a total of NaN", IssueKind::InvalidBinning, |b| {
            b.total = f64::NAN
        }),
        ("an infinite total", IssueKind::InvalidBinning, |b| {
            b.total = f64::INFINITY
        }),
        ("a bin width of 0", IssueKind::InvalidBinning, |b| {
            b.bins = Bins::Width(0.0)
        }),
        ("a negative bin width", IssueKind::InvalidBinning, |b| {
            b.bins = Bins::Width(-0.5)
        }),
        ("a bin width of NaN", IssueKind::InvalidBinning, |b| {
            b.bins = Bins::Width(f64::NAN)
        }),
        ("a range that decreases", IssueKind::InvalidBinning, |b| {
            b.range = Some([3.0, 0.0])
        }),
        (
            "a range that is not finite",
            IssueKind::InvalidBinning,
            |b| b.range = Some([0.0, f64::INFINITY]),
        ),
        ("edges that repeat", IssueKind::InvalidEdges, |b| {
            b.bins = Bins::Edges(vec![0.0, 1.0, 1.0, 3.0])
        }),
        ("edges that decrease", IssueKind::InvalidEdges, |b| {
            b.bins = Bins::Edges(vec![3.0, 2.0, 1.0, 0.0])
        }),
        ("an edge of NaN", IssueKind::InvalidEdges, |b| {
            b.bins = Bins::Edges(vec![0.0, f64::NAN, 2.0, 3.0])
        }),
    ];
    for (name, kind, spoil) in cases {
        let mut binning = counted();
        spoil(&mut binning);

        let mut fx = fixture(false, bars(&[2.0, 5.0, 3.0]));
        unit_bin_edges(&mut fx.fig, NodeId(5002), 3);
        bar_mut(&mut fx.fig).binning = Some(binning.clone());
        assert_only_error(
            &fx.fig.validate(),
            kind,
            fx.artist,
            &format!("bars: {name}"),
        );

        let mut fx = fixture(false, stairs_on_edges(&[2.0, 5.0, 3.0]));
        unit_bin_edges(&mut fx.fig, NodeId(5002), 3);
        stairs_mut(&mut fx.fig).binning = Some(binning.clone());
        assert_only_error(
            &fx.fig.validate(),
            kind,
            fx.artist,
            &format!("stairs: {name}"),
        );

        let mut fx = fixture(false, cells(2, 3));
        let x_edges = unit_bin_edges(&mut fx.fig, NodeId(5002), 3);
        let y_edges = unit_bin_edges(&mut fx.fig, NodeId(5003), 2);
        cells_mut(&mut fx.fig).binning = Some(Binning2d {
            x_bins: binning.bins.clone(),
            y_bins: Bins::Count(2),
            x_edges,
            y_edges,
            x_range: binning.range,
            y_range: None,
            normalization: binning.normalization,
            min_count: None,
            total: binning.total,
            excluded_non_finite: 0,
            excluded_out_of_range: 0,
        });
        assert_only_error(
            &fx.fig.validate(),
            kind,
            fx.artist,
            &format!("cells: {name}"),
        );
    }
    // A minimum count below which cells are left empty is a number of samples, so it
    // cannot be negative or NaN either; each axis of a 2D record is checked on its own.
    type Spoil2d = fn(&mut Binning2d);
    let spoilt: [(&str, Spoil2d); 3] = [
        ("a negative minimum count", |b| b.min_count = Some(-1.0)),
        ("a minimum count of NaN", |b| b.min_count = Some(f64::NAN)),
        ("a y range that decreases", |b| {
            b.y_range = Some([1.0, -1.0])
        }),
    ];
    for (name, spoil) in spoilt {
        let mut binning = Binning2d {
            x_bins: Bins::Count(3),
            y_bins: Bins::Count(2),
            x_edges: NodeId(5002),
            y_edges: NodeId(5003),
            x_range: None,
            y_range: None,
            normalization: HistNorm::Count,
            min_count: Some(1.0),
            total: 10.0,
            excluded_non_finite: 0,
            excluded_out_of_range: 0,
        };
        spoil(&mut binning);
        let mut fx = fixture(false, cells(2, 3));
        unit_bin_edges(&mut fx.fig, NodeId(5002), 3);
        unit_bin_edges(&mut fx.fig, NodeId(5003), 2);
        cells_mut(&mut fx.fig).binning = Some(binning);
        assert_only_error(
            &fx.fig.validate(),
            IssueKind::InvalidBinning,
            fx.artist,
            name,
        );
    }
}

// Why: every array of a bar series holds one entry per bar, so arrays of different lengths
// leave bars without a position, a value, a base, a width or a value to colour them by;
// and each must be a vector, because a matrix of the same count has no order of bars that
// a reader could rely on. Colour data holds one value per bar only: unlike a series, bars
// have no segments to colour, so one value fewer than the bars is a mismatch too.
#[test]
fn bar_arrays_that_do_not_hold_one_entry_per_bar_are_shape_mismatches() {
    type Change = fn(&mut Figure);
    let cases: [(&str, Change); 8] = [
        ("fewer values than positions", |fig| {
            let values = bar_mut(fig).values;
            replace_array(fig, values, NdArray::vector(vec![1.0, 2.0]));
        }),
        ("a base of another length", |fig| {
            let base = push_vector(fig, NodeId(50), &[0.0, 0.0]);
            bar_mut(fig).base = PerBar::Data(base);
        }),
        ("widths of another length", |fig| {
            let widths = push_vector(fig, NodeId(50), &[1.0, 1.0, 1.0, 1.0]);
            bar_mut(fig).width = BarWidth::Data(widths);
        }),
        ("colour data of one value", |fig| {
            give_colour_data(fig, &[1.0]);
            bar_mut(fig).face = ColorSpec::Colormapped;
        }),
        ("colour data of one value per gap between bars", |fig| {
            give_colour_data(fig, &[1.0, 2.0]);
            bar_mut(fig).edge.color = ColorSpec::Colormapped;
        }),
        ("colour data as a matrix", |fig| {
            let data = push_array(fig, NodeId(60), vec![3, 1], vec![1.0, 2.0, 3.0]);
            let bar = bar_mut(fig);
            bar.color_data = Some(data);
            bar.color_scale = Some(SCALE);
            bar.face = ColorSpec::Colormapped;
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
// bar series, its colour data included as a series' is, is read as floating-point
// numbers, so an array of bytes there is refused as the wrong element type rather than
// read as garbage.
#[test]
fn bar_arrays_of_bytes_are_element_type_mismatches() {
    type Change = fn(&mut Figure, NodeId);
    let cases: [(&str, Change); 5] = [
        ("positions", |fig, bytes| bar_mut(fig).positions = bytes),
        ("values", |fig, bytes| bar_mut(fig).values = bytes),
        ("base", |fig, bytes| bar_mut(fig).base = PerBar::Data(bytes)),
        ("widths", |fig, bytes| {
            bar_mut(fig).width = BarWidth::Data(bytes)
        }),
        ("colour data", |fig, bytes| {
            let bar = bar_mut(fig);
            bar.color_data = Some(bytes);
            bar.color_scale = Some(SCALE);
            bar.face = ColorSpec::Colormapped;
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

// Why: stairs have no colour scale and no colour data, so a colormapped line or fill has no
// value to look a colour up by and no scale to look it up in; drawing it in some fallback
// colour would hide the mistake, so it is refused, naming the artist. Bars, which may
// have colour data, and cells, which map their values, are not affected.
#[test]
fn a_colormapped_colour_on_stairs_is_an_error() {
    type Change = fn(&mut Figure);
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

/// Gives the fixture's scale [`SCALE`] the given kind.
fn set_scale_kind(fig: &mut Figure, kind: ColorScaleKind) {
    fig.color_scales
        .iter_mut()
        .find(|scale| scale.id == SCALE)
        .expect("the fixture holds its scale")
        .kind = kind;
}

// Why: a bar chart coloured by category (the occupancy of each slot, the model of each
// bar) maps whole-number codes through a categorical or indexed scale, and one coloured by
// a quantity maps it through a continuous scale; bars, like series, accept every kind of
// scale, so none of these may be refused, whether the face or the outline is
// colormapped. Values that each scale places are used, so that only a wrongful
// restriction on the kind of scale can fail the test.
#[test]
fn bars_coloured_by_data_accept_every_kind_of_colour_scale() {
    let entries = vec![Color::BLACK, Color::WHITE, Color::BLACK];
    let kinds = [
        ("continuous", ColorScaleKind::default()),
        (
            "indexed",
            ColorScaleKind::Indexed(IndexedColorScale {
                colors: entries.clone(),
                outside: ColorScaleOutOfRange::new(OutOfRange::Strict),
                ..IndexedColorScale::default()
            }),
        ),
        (
            "categorical",
            ColorScaleKind::Categorical(CategoricalColorScale {
                categories: ["low", "middle", "high"]
                    .map(|label| ColorCategory {
                        color: Color::BLACK,
                        label: Text::plain(label),
                    })
                    .to_vec(),
                outside: ColorScaleOutOfRange::new(OutOfRange::Strict),
            }),
        ),
    ];
    for (name, kind) in kinds {
        for on_edge in [false, true] {
            let mut fx = fixture(false, bars(&[2.0, 5.0, 3.0]));
            set_scale_kind(&mut fx.fig, kind.clone());
            give_colour_data(&mut fx.fig, &[0.0, 2.0, 1.0]);
            let bar = bar_mut(&mut fx.fig);
            if on_edge {
                bar.edge.color = ColorSpec::Colormapped;
            } else {
                bar.face = ColorSpec::Colormapped;
            }
            assert_no_issues(
                &fx.fig.validate(),
                &format!(
                    "{name}, colormapped {}",
                    if on_edge { "edge" } else { "face" }
                ),
            );
        }
    }
}

// Why: bars map their colour data through their scale as a series does, so a strict policy
// of an index-mapped scale must refuse a code it cannot place, rather than draw that bar
// in no colour without a word.
#[test]
fn a_bar_colour_value_that_a_strict_scale_cannot_place_is_out_of_range() {
    let mut fx = fixture(false, bars(&[2.0, 5.0, 3.0]));
    set_scale_kind(
        &mut fx.fig,
        ColorScaleKind::Indexed(IndexedColorScale {
            colors: vec![Color::BLACK; 2],
            outside: ColorScaleOutOfRange::new(OutOfRange::Strict),
            ..IndexedColorScale::default()
        }),
    );
    give_colour_data(&mut fx.fig, &[0.0, 1.0, 2.0]);
    bar_mut(&mut fx.fig).face = ColorSpec::Colormapped;
    assert_only_error(
        &fx.fig.validate(),
        IssueKind::ValueOutOfRange,
        fx.artist,
        "the code 2 names no entry of two",
    );
}

/// Builds a figure of one 2D axes holding, over three points, either a bar series or a
/// series that draws markers without a line, each with the given colour data (added when
/// present) and colour scale reference ([`SCALE`] when `named`), and with its face (the
/// bars' or the markers') set to `face` and its edge to `edge`. Returns the report of its
/// validation and the identifier of the artist.
fn coloured(
    as_bars: bool,
    data: bool,
    named: bool,
    face: ColorSpec,
    edge: ColorSpec,
) -> (ValidationReport, NodeId) {
    let fx = fixture(false, move |b, id| {
        let points = b.vector(&[1.0, 2.0, 3.0]);
        let color_data = data.then(|| b.vector(&[0.0, 0.5, 1.0]));
        let color_scale = named.then_some(SCALE);
        if as_bars {
            Artist::Bar(Bar {
                id,
                positions: points,
                values: points,
                face,
                edge: LineStyle {
                    color: edge,
                    ..Bar::default().edge
                },
                color_data,
                color_scale,
                ..Bar::default()
            })
        } else {
            Artist::Series(Series {
                id,
                x: points,
                y: points,
                line: LineStyle {
                    dash: DashStyle::None,
                    ..LineStyle::default()
                },
                marker: MarkerStyle {
                    shape: MarkerShape::Circle,
                    face,
                    edge,
                    ..MarkerStyle::default()
                },
                color_data,
                color_scale,
                ..Series::default()
            })
        }
    });
    // A scale that no artist names is valid on its own, so it stays in the figure.
    (fx.fig.validate(), fx.artist)
}

// Why: bars colour individual bars the way a series colours individual markers, so they
// must follow the same rules, with the same issue kinds, or a program that builds both
// from one description of its colours would find one valid and the other refused. Colour
// data without a scale has nothing to be looked up in and is refused as a missing scale,
// whatever the colours, because it would still set the scale's limits; a colormapped face
// or edge without colour data has no number to map and is drawn as an automatic colour,
// so it needs no scale and raises nothing; and with both, a colormapped face or edge is
// valid. Each case is compared with the equivalent series and also stated outright, so
// that a change to the series rules cannot silently pass for bars.
#[test]
fn a_colormapped_bar_face_or_edge_follows_the_colour_rules_of_a_series() {
    use ColorSpec::{Auto, Colormapped};
    let none = ColorSpec::None;
    /// A case: its name, whether the artist has colour data and names a scale, its face
    /// and edge colours, and the errors expected of it.
    type Case = (
        &'static str,
        bool,
        bool,
        ColorSpec,
        ColorSpec,
        &'static [IssueKind],
    );
    let cases: [Case; 6] = [
        (
            "a colormapped face without colour data or a scale",
            false,
            false,
            Colormapped,
            none,
            &[],
        ),
        (
            "a colormapped edge without colour data, naming a scale",
            false,
            true,
            Auto,
            Colormapped,
            &[],
        ),
        (
            "a colormapped face with colour data but no scale",
            true,
            false,
            Colormapped,
            none,
            &[IssueKind::MissingColorScale],
        ),
        (
            "a colormapped edge with colour data but no scale",
            true,
            false,
            Auto,
            Colormapped,
            &[IssueKind::MissingColorScale],
        ),
        (
            "colour data without a scale or a colormapped colour",
            true,
            false,
            Auto,
            none,
            &[IssueKind::MissingColorScale],
        ),
        (
            "a colormapped face and edge with colour data and a scale",
            true,
            true,
            Colormapped,
            Colormapped,
            &[],
        ),
    ];
    for (name, data, named, face, edge, expected) in cases {
        let (bars, bar) = coloured(true, data, named, face, edge);
        let (series, _) = coloured(false, data, named, face, edge);
        assert_eq!(error_kinds(&bars), expected, "{name}: {bars:?}");
        assert!(
            expected.iter().all(|&kind| has_error(&bars, kind, bar)),
            "{name}: the errors are not against the bars: {bars:?}"
        );
        assert_eq!(
            error_kinds(&bars),
            error_kinds(&series),
            "{name}: the bars and the series differ: {bars:?} / {series:?}"
        );
        assert_eq!(
            warning_kinds(&bars),
            warning_kinds(&series),
            "{name}: the bars and the series differ: {bars:?} / {series:?}"
        );
    }
}

/// Builds a figure with one 2D axes holding bar series at positions 1, 2, …, n, one for each
/// set of values, each changed by its function with its index, and returns it.
fn bar_set(
    series: &[&'static [f64]],
    change: impl Fn(usize, &mut Bar, &mut FigureBuilder),
) -> Figure {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let n = series.first().map_or(0, |values| values.len());
    let positions: Vec<f64> = (1..=n).map(|k| k as f64).collect();
    let positions = b.vector(&positions);
    for (k, values) in series.iter().enumerate() {
        let id = b.node();
        let mut bar = Bar {
            id,
            positions,
            values: b.vector(values),
            ..Bar::default()
        };
        change(k, &mut bar, &mut b);
        b.push(axes, Artist::Bar(bar));
    }
    b.build()
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
// parts included. The facade's helper writes both as ordinary series: grouped columns as
// a narrower width with an offset in widths for each column, and stacked parts as bases
// given per bar from the running totals. Those exact forms, overlapping or not, must raise
// nothing, or the most common use of bars would always carry an issue; series of one axes
// are never compared with one another, so nothing can be inconsistent between them.
#[test]
fn the_series_that_grouping_and_stacking_write_raise_no_issue() {
    let models: [&'static [f64]; 3] = [
        &[0.8, 0.6, 0.9, 0.7],
        &[0.7, 0.9, 0.5, 0.8],
        &[0.9, 0.4, 0.6, f64::NAN],
    ];
    let fig = bar_set(&models, |k, bar, _| {
        bar.width = BarWidth::Pitch(0.8 / 3.0);
        bar.offset = BarOffset::Widths(k as f64 - 1.0);
    });
    assert_no_issues(&fig.validate(), "three grouped series");

    // Parts 1, −2, 3 / −1.5, 2, 0.5 / 2, 2, −4, positive and negative stacked apart.
    let parts: [&'static [f64]; 3] = [&[1.0, -2.0, 3.0], &[-1.5, 2.0, 0.5], &[2.0, 2.0, -4.0]];
    let bases: [&'static [f64]; 3] = [&[0.0, 0.0, 0.0], &[0.0, 0.0, 3.0], &[1.0, 2.0, 0.0]];
    let fig = bar_set(&parts, |k, bar, b| {
        bar.base = PerBar::Data(b.vector(bases[k]));
    });
    assert_no_issues(&fig.validate(), "three stacked series with negative parts");

    // Series that disagree in every property that a group once shared are still valid.
    let fig = bar_set(&[&[1.0, 2.0], &[2.0, 3.0]], |k, bar, _| {
        if k == 1 {
            bar.orientation = Orientation::Horizontal;
            bar.width = BarWidth::Constant(0.5);
            bar.base = PerBar::Constant(1.0);
        }
    });
    assert_no_issues(&fig.validate(), "series that differ");
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
            bar_mut(fig).base = PerBar::Data(base);
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
            bar_mut(fig).base = PerBar::Constant(1.0);
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

// Why: a value is a length from the base, not a place on the axis, so on a logarithmic
// value axis what must be positive is where a bar ends, `base + value`, not the value: a
// bar of length −5 hanging from 10 ends at 5 and is drawn, and must not be reported as a
// non-positive value, while a bar of length −20 hanging from 10 ends at −10, which the axis
// has no place for, so it is not drawn and the user is told, with a warning that says the
// bar is not drawn. The rule follows the orientation, and a bar of length 0 ends at its
// positive base and is drawn.
#[test]
fn on_a_logarithmic_value_axis_the_end_of_a_bar_not_its_length_must_be_positive() {
    type Change = fn(&mut Figure);
    let quiet: [(&str, &'static [f64], Change); 3] = [
        (
            "bars hanging from 10 that end above 0",
            &[-5.0, -9.0],
            |fig| {
                fig.axes[0].y.scale = Scale::Log;
                bar_mut(fig).base = PerBar::Constant(10.0);
            },
        ),
        ("a bar of length 0 on a positive base", &[0.0, 3.0], |fig| {
            fig.axes[0].y.scale = Scale::Log;
            bar_mut(fig).base = PerBar::Constant(1.0);
        }),
        (
            "floating bars whose ends are positive",
            &[-1.0, 4.0],
            |fig| {
                fig.axes[0].y.scale = Scale::Log;
                let base = push_vector(fig, NodeId(50), &[2.0, 0.5]);
                bar_mut(fig).base = PerBar::Data(base);
            },
        ),
    ];
    for (name, values, change) in quiet {
        let mut fx = fixture(false, bars(values));
        change(&mut fx.fig);
        assert_no_issues(&fx.fig.validate(), name);
    }
    let warned: [(&str, &'static [f64], Change); 2] = [
        (
            "a bar hanging from 10 below 0 under log y",
            &[-20.0, 5.0],
            |fig| {
                fig.axes[0].y.scale = Scale::Log;
                bar_mut(fig).base = PerBar::Constant(10.0);
            },
        ),
        ("a flat bar ending at 0 under log x", &[-10.0, 5.0], |fig| {
            fig.axes[0].x.scale = Scale::Log;
            let bar = bar_mut(fig);
            bar.orientation = Orientation::Horizontal;
            bar.base = PerBar::Constant(10.0);
        }),
    ];
    for (name, values, change) in warned {
        let mut fx = fixture(false, bars(values));
        change(&mut fx.fig);
        let report = fx.fig.validate();
        assert_eq!(report.errors, vec![], "{name}: {report:?}");
        assert_eq!(
            warning_kinds(&report),
            [IssueKind::NonPositiveOnLogAxis],
            "{name}: {report:?}"
        );
        assert!(
            has_warning(&report, IssueKind::NonPositiveOnLogAxis, fx.artist)
                && report.warnings[0].message.contains("not drawn"),
            "{name}: the message must say the bar is not drawn: {report:?}"
        );
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
        // Unlike stairs, cells have a colour scale, so an outline can take the colour of its
        // cell, which outlines cells separated by a gap in their own colours.
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
// no meaning; it is refused on every opacity of the new artists (the faces and edges of
// bars, the line and fill of stairs, and the faces and edges of cells) alike, while both
// ends of the range, transparent and opaque, are accepted. Each opacity is checked on its
// own, so that one that is not validated cannot hide behind another that is.
#[test]
fn an_opacity_outside_zero_to_one_is_an_error_on_bars_stairs_and_cells() {
    type SetOpacity = fn(&mut Artist, f64);
    let opacities: [(&str, usize, SetOpacity); 6] = [
        ("bar face", 0, |artist, value| {
            if let Artist::Bar(bar) = artist {
                bar.face_opacity = value;
            }
        }),
        ("bar edge", 0, |artist, value| {
            if let Artist::Bar(bar) = artist {
                bar.edge.color = ColorSpec::Auto;
                bar.edge_opacity = value;
            }
        }),
        ("stairs line", 1, |artist, value| {
            if let Artist::Stairs(stairs) = artist {
                stairs.line_opacity = value;
            }
        }),
        ("stairs fill", 1, |artist, value| {
            if let Artist::Stairs(stairs) = artist {
                stairs.base = Some(0.0);
                stairs.fill = ColorSpec::Auto;
                stairs.fill_opacity = value;
            }
        }),
        ("cell faces", 2, |artist, value| {
            if let Artist::Cells(cells) = artist {
                cells.face_opacity = value;
            }
        }),
        ("cell edges", 2, |artist, value| {
            if let Artist::Cells(cells) = artist {
                cells.edge.color = ColorSpec::Colormapped;
                cells.edge_opacity = value;
            }
        }),
    ];
    for (name, artist, set) in opacities {
        for value in [0.0, 0.5, 1.0] {
            let (_, make) = every_new_artist().swap_remove(artist);
            let mut fx = fixture(false, make);
            set(only_artist(&mut fx.fig), value);
            assert_no_issues(&fx.fig.validate(), &format!("{name} at {value}"));
        }
        for value in [-0.01, 1.01, f64::NAN, f64::INFINITY] {
            let (_, make) = every_new_artist().swap_remove(artist);
            let mut fx = fixture(false, make);
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
    bar_mut(&mut fx.fig).width = BarWidth::Pitch(0.0);
    assert_only_error(
        &fx.fig.validate(),
        IssueKind::InvalidBarWidth,
        fx.artist,
        "bars",
    );

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
