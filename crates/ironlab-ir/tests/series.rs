//! Series (#19, #44): the one artist that replaces `Line` and `Scatter`, its defaults, its
//! persistence, the validation of its size and colour data, its colour scale reference, its
//! properties in the edit registry and the choices the property editor offers for them.
//!
//! The IR these tests assume is the following, which the plan of the colourbar epic sketches
//! and these tests make precise:
//!
//! ```text
//! pub struct Series {
//!     pub id: NodeId,
//!     pub label: Option<Text>,
//!     pub visible: bool,
//!     pub x: NodeId,
//!     pub y: NodeId,
//!     pub z: Option<NodeId>,
//!     pub line: LineStyle,               // dash None or colour None: no line is drawn
//!     pub marker: MarkerStyle,           // shape None: no marker is drawn
//!     pub size_data: Option<NodeId>,     // the diameter of each marker in points, n values
//!     pub color_data: Option<NodeId>,    // n values (per point) or n − 1 (per segment)
//!     pub color_scale: Option<NodeId>,   // required when color_data is set
//!     pub x_coords: Coords,
//!     pub y_coords: Coords,
//!     pub clip: bool,
//! }
//! ```
//!
//! - `Artist::Series`, `NodeKind::Series`, and a wire message `Series`; the JSON tag of the
//!   artist is `"series"`. `Line`, `Scatter`, `ScatterSize` and `ScatterColor` no longer exist,
//!   and no compatibility shim reads the old `"line"` or `"scatter"` forms (the project has no
//!   users yet, so breaking changes are free).
//! - `Series::default()` is the default of `plot`: a solid automatic line 0.75 pt wide
//!   (`LineStyle::default()`), no markers (`MarkerStyle::default()`), no size or colour data,
//!   no scale, both dimensions in data coordinates, clipped and visible. The facade's
//!   `scatter` sets its own defaults over it.
//! - `line.color` is the colour of the series, which an automatic marker face or edge
//!   follows, as a line's automatic marker edge follows its line today.
//! - `color_data` must hold n values (one per point) or n − 1 values (one per segment), where
//!   n is the number of points. n − 1 values are refused when the series draws no line, which
//!   is when its dash is `None` or its line colour is `None`; that refusal, and every other
//!   length, is `IssueKind::ShapeMismatch` against the series. `size_data` must hold exactly
//!   n values.
//! - A series whose `color_data` is set maps it, so `Artist::color_mapping` returns its colour
//!   data and scale, and it must name a scale (`MissingColorScale`) whatever its colour
//!   specifications. A series without colour data maps nothing, even when it names a scale.
//!   Any kind of scale is accepted.
//! - The registry lists `size_data`, `color_data` and `color_scale` as optional node
//!   identifiers, and no longer lists `size` or `color`. A colormapped colour is available at
//!   `line.color`, `marker.face` and `marker.edge` exactly when the series has colour data and
//!   names a colour scale of the figure.

mod common;

use common::FigureBuilder;
use common::edits::{applied, path, set, tx};
use common::{categorical, continuous, discrete, indexed, named};
use ironlab_ir::*;
use serde_json::{Value as JsonValue, json};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

/// Returns whether a list of issues holds one of the given kind against the given node.
fn has(issues: &[ValidationIssue], kind: IssueKind, node: NodeId) -> bool {
    issues
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

/// Returns the kinds of the issues of a list, in the order reported.
fn kinds(issues: &[ValidationIssue]) -> Vec<IssueKind> {
    issues.iter().map(|issue| issue.kind).collect()
}

/// The values 0, 1, …, n − 1.
fn count(n: usize) -> Vec<f64> {
    (0..n).map(|i| i as f64).collect()
}

/// Adds a continuous scale of viridis between automatic limits.
fn viridis(b: &mut FigureBuilder) -> NodeId {
    b.color_scale(continuous(named(ColormapName::Viridis), Limits::Auto))
}

/// A figure of one 2D axes holding one series of `n` points, built from the defaults by
/// `edit`. Returns the figure and the identifier of the series.
fn figure_with(n: usize, edit: impl FnOnce(&mut Series, &mut FigureBuilder)) -> (Figure, NodeId) {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let x = b.vector(&count(n));
    let y = b.vector(&count(n));
    let id = b.node();
    let mut series = Series {
        id,
        x,
        y,
        ..Series::default()
    };
    edit(&mut series, &mut b);
    b.push(axes, Artist::Series(series));
    (b.build(), id)
}

/// A series of `n` points coloured by `colours` values through a continuous scale, with a
/// line and circle markers unless `edit` says otherwise.
fn coloured(n: usize, colours: usize, edit: impl FnOnce(&mut Series)) -> (Figure, NodeId) {
    figure_with(n, |s, b| {
        s.color_data = Some(b.vector(&count(colours)));
        s.color_scale = Some(viridis(b));
        s.line.color = ColorSpec::Colormapped;
        s.marker.shape = MarkerShape::Circle;
        edit(s);
    })
}

/// Returns the series of a figure, found without the API under test.
fn series_of(fig: &Figure, id: NodeId) -> &Series {
    fig.axes
        .iter()
        .flat_map(|axes| &axes.artists)
        .find_map(|artist| match artist {
            Artist::Series(s) if s.id == id => Some(s),
            _ => None,
        })
        .expect("the fixture series exists")
}

// ---------------------------------------------------------------------------------
// The model: defaults, kinds and persistence
// ---------------------------------------------------------------------------------

// Why: the default series is what `plot` draws, so it must be a solid automatic line without markers, data arrays or a scale.
#[test]
fn the_default_series_is_a_solid_automatic_line_without_markers_data_or_scale() {
    let s = Series::default();
    assert!(s.visible);
    assert_eq!(s.label, None);
    assert_eq!(s.z, None);
    assert_eq!(s.line, LineStyle::default());
    assert_eq!(s.line.dash, DashStyle::Solid);
    assert_eq!(s.line.color, ColorSpec::Auto);
    assert_eq!(s.marker, MarkerStyle::default());
    assert_eq!(s.marker.shape, MarkerShape::None);
    assert_eq!(s.size_data, None);
    assert_eq!(s.color_data, None);
    assert_eq!(s.color_scale, None);
    assert_eq!(s.x_coords, Coords::Data);
    assert_eq!(s.y_coords, Coords::Data);
    assert!(s.clip);
}

// Why: a series is one kind of node, so the tree, the inspector and the edit paths must see it as `Series` and nothing else.
#[test]
fn a_series_is_a_node_of_kind_series() {
    let (fig, id) = figure_with(3, |_, _| {});
    assert_eq!(fig.node_kind(id), Some(NodeKind::Series));
    assert!(NodeKind::ALL.contains(&NodeKind::Series));
    let (_, artist) = fig
        .artist(id)
        .expect("the series is an artist of the figure");
    assert_eq!(artist.id(), id);
    assert!(artist.visible());
}

// Why: every property a series can carry must reload exactly from both formats, or a saved figure would change its look or lose its data references.
#[test]
fn a_series_with_every_property_set_or_absent_round_trips_through_json_and_protobuf() {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(1, 2);
    let flat = b.axes2d(0, 0);
    let deep = b.axes3d(0, 1);
    let scale = b.color_scale(indexed(vec![RED, GREEN, BLUE], OutOfRange::Strict));
    let x = b.vector(&[0.0, 1.0, 2.0]);
    let y = b.vector(&[2.0, 0.0, 1.0]);
    let z = b.vector(&[1.0, 2.0, 3.0]);
    let sizes = b.vector(&[3.0, 6.0, 9.0]);
    let classes = b.vector(&[0.0, 2.0]);
    let (full, bare) = (b.node(), b.node());
    b.push(
        deep,
        Artist::Series(Series {
            id: full,
            label: Some(Text::new("$u_\\infty$")),
            visible: false,
            x,
            y,
            z: Some(z),
            line: LineStyle {
                color: ColorSpec::Colormapped,
                width_pt: 1.25,
                dash: DashStyle::DashDot,
            },
            marker: MarkerStyle {
                shape: MarkerShape::Diamond,
                size_pt: 5.5,
                face: ColorSpec::Rgba { color: GREEN },
                edge: ColorSpec::None,
            },
            size_data: Some(sizes),
            color_data: Some(classes),
            color_scale: Some(scale),
            ..Series::default()
        }),
    );
    b.push(
        flat,
        Artist::Series(Series {
            id: bare,
            x,
            y,
            x_coords: Coords::AxesFraction,
            clip: false,
            ..Series::default()
        }),
    );
    let fig = b.build();
    assert_eq!(fig.validate().errors, vec![], "the fixture is valid");
    assert_eq!(
        Figure::from_json(&fig.to_json()).expect("the JSON form reloads"),
        fig,
        "JSON round trip"
    );
    assert_eq!(
        Figure::from_protobuf(&fig.to_protobuf()).expect("the binary form reloads"),
        fig,
        "protobuf round trip"
    );
}

// Why: clients in other languages write series as JSON, so the tag and the three references must be the forms the schema documents, and the two removed tagged properties must be gone.
#[test]
fn the_json_form_of_a_series_is_tagged_series_and_names_its_data_and_scale_directly() {
    let mut sizes = NodeId::default();
    let mut colours = NodeId::default();
    let mut scale = NodeId::default();
    let (fig, _) = figure_with(3, |s, b| {
        sizes = b.vector(&[1.0, 2.0, 3.0]);
        colours = b.vector(&[0.0, 1.0, 2.0]);
        scale = viridis(b);
        s.size_data = Some(sizes);
        s.color_data = Some(colours);
        s.color_scale = Some(scale);
    });
    let value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    let artist = &value["axes"][0]["artists"][0];
    assert_eq!(artist["type"], json!("series"));
    assert_eq!(artist["size_data"], json!(sizes.0));
    assert_eq!(artist["color_data"], json!(colours.0));
    assert_eq!(artist["color_scale"], json!(scale.0));
    for gone in ["size", "color"] {
        assert!(
            artist.get(gone).is_none(),
            "a series has no `{gone}` property: {artist}"
        );
    }
}

// Why: there is one artist for points, and no compatibility shim while the project has no users, so the old line and scatter forms must be refused rather than read silently.
#[test]
fn the_json_forms_of_the_removed_line_and_scatter_are_refused() {
    let (fig, _) = figure_with(3, |_, _| {});
    let mut value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    for old in ["line", "scatter"] {
        value["axes"][0]["artists"][0]["type"] = json!(old);
        assert!(
            Figure::from_json(&value.to_string()).is_err(),
            "an artist tagged {old:?} is not read"
        );
    }
}

// ---------------------------------------------------------------------------------
// Validation of size and colour data
// ---------------------------------------------------------------------------------

// Why: one colour value per point is the ordinary case (#44): each marker and each end of each segment has its own value, so it must be valid with a line, with markers, and with both.
#[test]
fn colour_data_of_one_value_per_point_is_valid_with_a_line_markers_or_both() {
    for (what, dash, shape) in [
        ("line and markers", DashStyle::Solid, MarkerShape::Circle),
        ("line alone", DashStyle::Solid, MarkerShape::None),
        ("markers alone", DashStyle::None, MarkerShape::Circle),
    ] {
        let (fig, _) = coloured(4, 4, |s| {
            s.line.dash = dash;
            s.marker.shape = shape;
        });
        assert_eq!(fig.validate().errors, vec![], "{what}");
    }
}

// Why: one colour value per segment colours each segment in one colour (#44), which a series with a line can draw whether or not it has markers.
#[test]
fn colour_data_of_one_value_per_segment_is_valid_when_the_series_draws_a_line() {
    for (what, shape) in [
        ("line and markers", MarkerShape::Circle),
        ("line alone", MarkerShape::None),
    ] {
        let (fig, _) = coloured(4, 3, |s| {
            s.marker.shape = shape;
            // With no value per point, the markers take fixed colours.
            s.marker.edge = ColorSpec::Rgba {
                color: Color::BLACK,
            };
        });
        assert_eq!(fig.validate().errors, vec![], "{what}");
    }
}

// Why: a value per segment means nothing to a series without segments, so a series that draws no line must refuse it rather than draw nothing silently.
#[test]
fn colour_data_of_one_value_per_segment_is_refused_when_the_series_draws_no_line() {
    type Edit = fn(&mut Series);
    let edits: [(&str, Edit); 2] = [
        ("a dash of none", |s| s.line.dash = DashStyle::None),
        ("a line colour of none", |s| s.line.color = ColorSpec::None),
    ];
    for (what, edit) in edits {
        let (fig, id) = coloured(4, 3, |s| {
            s.marker.face = ColorSpec::Colormapped;
            edit(s);
        });
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::ShapeMismatch, id),
            "{what}: {report:?}"
        );
    }
}

// Why: only n and n − 1 have a meaning, so every other length of colour data is the ordinary mismatch of arrays, whatever the series draws.
#[test]
fn colour_data_of_any_other_length_is_a_shape_mismatch() {
    for colours in [0, 2, 5, 8] {
        let (fig, id) = coloured(4, colours, |_| {});
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::ShapeMismatch, id),
            "{colours} values for 4 points: {report:?}"
        );
    }
}

// Why: empty and single-point data are valid (ADR 0012), and "n − 1 values" is a natural place to subtract one from zero, so a series of no points with no colour values, and a series of one point with one, must validate without error rather than fail or panic.
#[test]
fn empty_and_single_point_series_with_one_colour_value_per_point_are_valid() {
    for n in [0, 1] {
        let (fig, _) = coloured(n, n, |_| {});
        assert_eq!(fig.validate().errors, vec![], "{n} points");
    }
}

// Why: a size belongs to a marker and a marker to a point, so size data must hold exactly one value per point, with no per-segment form.
#[test]
fn size_data_must_hold_exactly_one_value_per_point() {
    let sized = |sizes: usize| {
        figure_with(4, |s, b| {
            s.marker.shape = MarkerShape::Circle;
            s.size_data = Some(b.vector(&vec![4.0; sizes]));
        })
    };
    let (fig, _) = sized(4);
    assert_eq!(fig.validate().errors, vec![]);
    for sizes in [3, 5] {
        let (fig, id) = sized(sizes);
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::ShapeMismatch, id),
            "{sizes} sizes for 4 points: {report:?}"
        );
    }
}

// Why: there is no axes colormap to fall back on, so colour data without a scale cannot be drawn and must be reported, whichever colours use it.
#[test]
fn colour_data_without_a_scale_is_missing_color_scale() {
    for (what, line, face) in [
        ("a mapped line", ColorSpec::Colormapped, ColorSpec::None),
        ("mapped markers", ColorSpec::Auto, ColorSpec::Colormapped),
        ("automatic colours", ColorSpec::Auto, ColorSpec::Auto),
    ] {
        let (fig, id) = coloured(3, 3, |s| {
            s.color_scale = None;
            s.line.color = line;
            s.marker.face = face;
        });
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::MissingColorScale, id),
            "{what}: {report:?}"
        );
    }
}

// Why: a scale without colour data maps nothing, so naming one must not make a series invalid or change what it maps.
#[test]
fn a_scale_without_colour_data_is_valid_and_maps_nothing() {
    let (fig, id) = figure_with(3, |s, b| s.color_scale = Some(viridis(b)));
    assert_eq!(fig.validate().errors, vec![]);
    let (_, artist) = fig.artist(id).unwrap();
    assert_eq!(artist.color_mapping(), None);
    assert_eq!(artist.color_scale(), series_of(&fig, id).color_scale);
}

// Why: validation, automatic limits and the compiler all decide from `color_mapping` whether an artist is colour-mapped, so a series with colour data must report its scale and values there, per point or per segment.
#[test]
fn a_series_with_colour_data_maps_its_colour_data_through_its_scale() {
    for colours in [3, 2] {
        let (fig, id) = coloured(3, colours, |_| {});
        let s = series_of(&fig, id);
        let (_, artist) = fig.artist(id).unwrap();
        assert_eq!(
            artist.color_mapping(),
            Some(ColorMapping {
                color_scale: s.color_scale,
                values: s.color_data.unwrap(),
            }),
            "{colours} values"
        );
    }
}

// Why: any artist that uses a scale accepts any kind of scale (plan, "Decisions"), so a series must validate through each of the four kinds with values each can map.
#[test]
fn a_series_accepts_every_kind_of_scale() {
    for kind in [
        continuous(named(ColormapName::Magma), Limits::Auto),
        discrete(&[0.0, 1.0, 2.0]),
        indexed(vec![RED, GREEN, BLUE], OutOfRange::Strict),
        categorical(vec![RED, GREEN, BLUE], OutOfRange::Strict),
    ] {
        let (fig, _) = figure_with(3, |s, b| {
            s.color_data = Some(b.vector(&[0.0, 1.0, 2.0]));
            s.color_scale = Some(b.color_scale(kind.clone()));
            s.marker.shape = MarkerShape::Circle;
            s.marker.edge = ColorSpec::Colormapped;
        });
        assert_eq!(fig.validate().errors, vec![], "{kind:?}");
    }
}

// Why: colour data follows its scale's rules whatever its length, so a per-segment class outside a strict indexed scale must be refused as a per-point one is.
#[test]
fn colour_data_outside_a_strict_indexed_scale_is_value_out_of_range_per_point_or_per_segment() {
    for (n, colours) in [(3, vec![0.0, 0.5, 2.0]), (3, vec![0.0, 3.0])] {
        let (fig, id) = figure_with(n, |s, b| {
            s.color_data = Some(b.vector(&colours));
            s.color_scale =
                Some(b.color_scale(indexed(vec![RED, GREEN, BLUE], OutOfRange::Strict)));
            s.line.color = ColorSpec::Colormapped;
        });
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::ValueOutOfRange, id),
            "{colours:?}: {report:?}"
        );
    }
}

// Why: the size and colour data are references like the coordinates, so a reference to a missing node, to a node of another kind or to 8-bit values must be refused as it is for x and y.
#[test]
fn size_and_colour_data_are_checked_as_references_to_floating_point_data_arrays() {
    for which in ["size_data", "color_data"] {
        let point = |s: &mut Series, target: NodeId| match which {
            "size_data" => s.size_data = Some(target),
            _ => s.color_data = Some(target),
        };
        let (fig, id) = coloured(3, 3, |s| point(s, NodeId(9_999)));
        assert!(
            has(&fig.validate().errors, IssueKind::UnknownData, id),
            "{which} naming no node"
        );

        let (fig, id) = coloured(3, 3, |s| {
            let scale = s.color_scale.unwrap();
            point(s, scale);
        });
        assert!(
            has(&fig.validate().errors, IssueKind::WrongNodeKind, id),
            "{which} naming a colour scale"
        );

        let (fig, id) = figure_with(3, |s, b| {
            let bytes = b.bytes(vec![3], vec![1, 2, 3]);
            s.color_scale = Some(viridis(b));
            s.color_data = Some(b.vector(&[0.0, 1.0, 2.0]));
            point(s, bytes);
        });
        assert!(
            has(&fig.validate().errors, IssueKind::ElementTypeMismatch, id),
            "{which} naming 8-bit values"
        );
    }
}

// Why: an array a series uses through its size or colour data is in use, so it must not be warned of as unused data.
#[test]
fn size_and_colour_data_count_as_used() {
    let (fig, _) = coloured(3, 3, |s| s.size_data = s.color_data);
    let report = fig.validate();
    assert!(
        !kinds(&report.warnings).contains(&IssueKind::UnusedData),
        "{report:?}"
    );
}

// Why: a hidden artist still contributes to its scale's automatic limits (ADR 0024), and values per segment are values like any other, so both must widen the limits.
#[test]
fn hidden_series_and_values_per_segment_contribute_to_automatic_colour_limits() {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let scale = viridis(&mut b);
    let points = b.vector(&[0.0, 1.0, 2.0]);
    let per_point = b.vector(&[1.0, 2.0, 3.0]);
    let per_segment = b.vector(&[-5.0, 40.0]);
    let (shown, hidden) = (b.node(), b.node());
    b.push(
        axes,
        Artist::Series(Series {
            id: shown,
            x: points,
            y: points,
            color_data: Some(per_point),
            color_scale: Some(scale),
            ..Series::default()
        }),
    );
    b.push(
        axes,
        Artist::Series(Series {
            id: hidden,
            visible: false,
            x: points,
            y: points,
            color_data: Some(per_segment),
            color_scale: Some(scale),
            ..Series::default()
        }),
    );
    let fig = b.build();
    assert_eq!(fig.validate().errors, vec![]);
    assert_eq!(fig.automatic_color_limits(scale), Some((-5.0, 40.0)));
}

// Why: blended coordinates (ADR 0019) belong to the series as they did to a line and a scatter, so its placement rules must still be enforced.
#[test]
fn a_series_keeps_the_rules_of_blended_coordinates_and_its_clip() {
    let mut b = FigureBuilder::new();
    let axes = b.axes3d(0, 0);
    let x = b.vector(&[0.0, 1.0]);
    let id = b.node();
    b.push(
        axes,
        Artist::Series(Series {
            id,
            x,
            y: x,
            x_coords: Coords::AxesFraction,
            ..Series::default()
        }),
    );
    let fig = b.build();
    assert!(has(
        &fig.validate().errors,
        IssueKind::AxesFractionInThreeDAxes,
        id
    ));

    let (fig, id) = figure_with(2, |s, _| s.clip = false);
    assert!(has(
        &fig.validate().warnings,
        IssueKind::IneffectiveClip,
        id
    ));
}

// ---------------------------------------------------------------------------------
// The edit registry and the property editor
// ---------------------------------------------------------------------------------

/// The paths of the properties of a kind of node, as text.
fn registry_paths(kind: NodeKind) -> Vec<String> {
    properties(kind)
        .into_iter()
        .map(|p| p.path.to_string())
        .collect()
}

// Why: the property editor edits a series through the registry, so every property must be listed once, the three references as optional identifiers, and the removed tagged properties not at all.
#[test]
fn the_registry_lists_the_properties_of_a_series() {
    let listed = registry_paths(NodeKind::Series);
    for expected in [
        "label",
        "visible",
        "x",
        "y",
        "z",
        "line",
        "line.color",
        "line.width_pt",
        "line.dash",
        "marker",
        "marker.shape",
        "marker.size_pt",
        "marker.face",
        "marker.edge",
        "size_data",
        "color_data",
        "color_scale",
        "x_coords",
        "y_coords",
        "clip",
    ] {
        assert!(
            listed.contains(&expected.to_owned()),
            "the registry lacks {expected}: {listed:?}"
        );
    }
    for gone in ["id", "size", "color"] {
        assert!(
            !listed.contains(&gone.to_owned()),
            "{gone} is not a settable property of a series: {listed:?}"
        );
    }
    for reference in ["size_data", "color_data", "color_scale", "z"] {
        let property = properties(NodeKind::Series)
            .into_iter()
            .find(|p| p.path.to_string() == reference)
            .unwrap();
        assert_eq!(property.value_type, ValueType::NodeId, "{reference}");
        assert!(property.optional, "{reference}");
    }
}

// Why: colour data is optional, so an editor must be able to give a series colour data and take it away in one transaction each, and undo must restore the series exactly.
#[test]
fn colour_and_size_data_are_set_and_unset_by_path_and_undone() {
    let mut colours = NodeId::default();
    let mut scale = NodeId::default();
    let (fig, id) = figure_with(3, |s, b| {
        colours = b.vector(&[0.0, 1.0, 2.0]);
        scale = viridis(b);
        s.marker.shape = MarkerShape::Circle;
    });
    let (edited, inverse) = applied(
        &fig,
        &tx([
            set(id, "color_data", Value::NodeId(colours)),
            set(id, "size_data", Value::NodeId(colours)),
            set(id, "color_scale", Value::NodeId(scale)),
            set(id, "marker.edge", Value::ColorSpec(ColorSpec::Colormapped)),
        ]),
    )
    .unwrap();
    let s = series_of(&edited, id);
    assert_eq!(s.color_data, Some(colours));
    assert_eq!(s.size_data, Some(colours));
    assert_eq!(s.color_scale, Some(scale));
    assert_eq!(edited.validate().errors, vec![]);
    assert_eq!(applied(&edited, &inverse).unwrap().0, fig, "undo");

    let (cleared, _) = applied(
        &edited,
        &tx([
            set(id, "color_data", Value::Unset),
            set(id, "size_data", Value::Unset),
        ]),
    )
    .unwrap();
    assert_eq!(series_of(&cleared, id).color_data, None);
    assert_eq!(series_of(&cleared, id).size_data, None);
    assert_eq!(cleared.get(id, &path("color_data")).unwrap(), Value::Unset);
}

/// Returns why a colormapped colour is unavailable at a property of a node, or `None` when
/// it is available.
fn colormapped_unavailable(fig: &Figure, node: NodeId, at: &str) -> Option<&'static str> {
    property_choices(fig, node, &path(at))
        .into_iter()
        .find(|choice| choice.value == Value::ColorSpec(ColorSpec::Colormapped))
        .unwrap_or_else(|| panic!("{at} offers a colormapped colour"))
        .unavailable
}

// Why: the rule for a colormapped colour is structural (#19): it is available on the line and on both parts of the markers exactly when the series has values to map and a scale to map them through.
#[test]
fn a_colormapped_colour_is_available_exactly_when_the_series_has_colour_data_and_a_scale() {
    let (mapped, id) = coloured(3, 3, |_| {});
    for at in ["line.color", "marker.face", "marker.edge"] {
        assert_eq!(
            colormapped_unavailable(&mapped, id, at),
            None,
            "{at} with colour data and a scale"
        );
    }

    let (no_data, id) = figure_with(3, |s, b| s.color_scale = Some(viridis(b)));
    let (no_scale, unscaled) = coloured(3, 3, |s| s.color_scale = None);
    for at in ["line.color", "marker.face", "marker.edge"] {
        assert!(
            colormapped_unavailable(&no_data, id, at).is_some(),
            "{at} without colour data"
        );
        assert!(
            colormapped_unavailable(&no_scale, unscaled, at).is_some(),
            "{at} without a scale"
        );
    }
}
