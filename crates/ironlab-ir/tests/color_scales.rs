//! Colour scales as figure nodes (#40, part of #24): the four kinds and their defaults,
//! the references that artists make to them, their validation, their persistence, their
//! properties in the edit registry, and the choices the property editor offers for them.
//!
//! The IR these tests assume is the approved sketch of the colourbar epic plan, with these
//! details, which the sketch leaves open:
//!
//! - `ColorScale` is a figure node in `Figure.color_scales`, declared in `figure_nodes!` as
//!   `ColorScale in color_scales, found by color_scale and color_scale_mut as "colour
//!   scale", encoded as color_scale`, so `NodeKind::ColorScale`, `Node::ColorScale`,
//!   `Figure::color_scale` and `Figure::color_scale_mut` exist.
//! - `ColorScale`, `ContinuousColorScale`, `DiscreteColorScale`, `IndexedColorScale`,
//!   `CategoricalColorScale`, `ColorScaleKind`, `ColorScaleLevels`, `Colormap` and
//!   `ColorScaleNorm` implement `Default`, with the defaults that the plan states.
//!   `ColorScaleOutOfRange` has no meaningful single default, because its policy defaults
//!   differ by category, so the tests build it as a struct literal.
//! - `ScalarImage` is the artist `Artist::ScalarImage` with `NodeKind::ScalarImage`, and
//!   `Contour`, `Surface`, `ScalarImage` and `Scatter` each have `color_scale:
//!   Option<NodeId>`, `None` by default. `Scatter` keeps its colour data in this stage, so
//!   it names a scale as the other colour-mapped artists do.
//! - `Contour.levels` is `Option<Levels>`, `None` by default.
//! - Validation adds `IssueKind::MissingColorScale`, reported against the artist;
//!   `IssueKind::ValueOutOfRange`, which replaces `PixelOutOfRange` and is reported against
//!   the artist whose value falls in a strict category; and `IssueKind::InvalidColormap`,
//!   reported against the scale whose listed colormap has fewer than 2 or more than 1024
//!   colours. Invalid band edges are `InvalidLevels` and invalid limits `InvalidLimits`,
//!   both reported against the scale, as they are for an axis and a contour.
//! - A scale reference that names no node is `IssueKind::UnknownColorScale`, reported
//!   against the artist, as the sibling of `UnknownData` for a data reference that names
//!   no node. An indexed or categorical scale with no entries is
//!   `IssueKind::InvalidColorScale`, reported against the scale.
//! - Validation of an index-mapped scale checks the data values themselves (the nodes of a
//!   field, the pixels of an image, the colour data of a scatter), never values derived
//!   from them: the mean of a surface face's corners and the automatic levels of a contour
//!   are not checked, because the compiler colours a face by its first corner and places
//!   automatic levels between entries (see the scene tests).
//! - Under a logarithmic norm, automatic limits range over the positive finite values only,
//!   so a value that is not positive lies below the range and follows `below`.
//! - A line or quiver whose colour is `Colormapped` has no values to map and no scale
//!   reference in this stage (the `Series` of stage 3 resolves line colour properly). It
//!   keeps today's drawing, the middle colour of viridis, so it needs no scale and raises no
//!   `MissingColorScale`. The same holds for a scatter whose single colour is
//!   `Colormapped`.
//! - The properties of a scale are reached through the flattened variant of its kind, as a
//!   panel's legend is reached through `content.legend`: `kind.continuous.limits`,
//!   `kind.discrete.levels`, `kind.indexed.outside.policy`, and so on.

mod common;

use common::edits::{applied, path, set, tx};
use common::{
    FigureBuilder, categorical, color_scale_variants_figure, continuous, discrete, indexed, named,
    out_of_range,
};
use ironlab_ir::*;
use serde_json::{Value as JsonValue, json};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

/// Returns the kinds of the issues of a list, in the order reported.
fn kinds(issues: &[ValidationIssue]) -> Vec<IssueKind> {
    issues.iter().map(|issue| issue.kind).collect()
}

/// Returns whether a list of issues holds one of the given kind against the given node.
fn has(issues: &[ValidationIssue], kind: IssueKind, node: NodeId) -> bool {
    issues
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

/// A figure of one 2D axes holding a 1 × n scalar image of the given float values, mapped
/// through one scale of the given kind. Returns the figure, the image and the scale.
fn image_through(kind: ColorScaleKind, values: &[f64]) -> (Figure, NodeId, NodeId) {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let scale = b.color_scale(kind);
    let data = b.matrix(1, values.len(), |_, i| values[i]);
    let image = b.scalar_image(axes, data, Some(scale));
    (b.build(), image, scale)
}

/// As [`image_through`], for 8-bit values.
fn byte_image_through(kind: ColorScaleKind, values: &[u8]) -> (Figure, NodeId, NodeId) {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let scale = b.color_scale(kind);
    let data = b.bytes(vec![1, values.len()], values.to_vec());
    let image = b.scalar_image(axes, data, Some(scale));
    (b.build(), image, scale)
}

/// Returns the scale of a figure with the given identifier, found without the API under
/// test.
fn scale_of(fig: &Figure, id: NodeId) -> &ColorScale {
    fig.color_scales
        .iter()
        .find(|s| s.id == id)
        .expect("the fixture scale exists")
}

/// Returns the scale of a figure with the given identifier for modification.
fn scale_mut(fig: &mut Figure, id: NodeId) -> &mut ColorScale {
    fig.color_scales
        .iter_mut()
        .find(|s| s.id == id)
        .expect("the fixture scale exists")
}

// ---------------------------------------------------------------------------------
// The model: kinds, defaults and persistence
// ---------------------------------------------------------------------------------

// Why: a figure with scales of every kind and variant must reload exactly from both formats, or a saved colourbar would change.
#[test]
fn every_kind_and_variant_of_colour_scale_round_trips_through_json_and_protobuf() {
    let fig = color_scale_variants_figure();
    assert_eq!(fig.validate().errors, vec![], "the fixture is valid");
    assert_eq!(fig.color_scales.len(), 8);
    let json = Figure::from_json(&fig.to_json()).expect("the JSON form reloads");
    assert_eq!(json, fig, "JSON round trip");
    let binary = Figure::from_protobuf(&fig.to_protobuf()).expect("the binary form reloads");
    assert_eq!(binary, fig, "protobuf round trip");

    // Each kind alone, so that a fault in one kind is not masked by the others.
    for scale in &fig.color_scales {
        let mut alone = Figure::new();
        alone.color_scales.push(scale.clone());
        assert_eq!(
            Figure::from_json(&alone.to_json()).unwrap(),
            alone,
            "{:?}",
            scale.kind
        );
        assert_eq!(
            Figure::from_protobuf(&alone.to_protobuf()).unwrap(),
            alone,
            "{:?}",
            scale.kind
        );
    }
}

// Why: clients in other languages write scales as JSON, so the tagged forms the plan documents must be the forms written.
#[test]
fn the_json_form_of_a_colour_scale_tags_its_kind_colormap_levels_and_norm() {
    let fig = color_scale_variants_figure();
    let value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    let scales = value["color_scales"]
        .as_array()
        .expect("the colour scales are a list");
    let kinds: Vec<&JsonValue> = scales.iter().map(|s| &s["kind"]["type"]).collect();
    assert_eq!(
        kinds,
        [
            &json!("continuous"),
            &json!("continuous"),
            &json!("discrete"),
            &json!("discrete"),
            &json!("indexed"),
            &json!("indexed"),
            &json!("categorical"),
            &json!("continuous"),
        ]
    );
    let magma = &scales[0]["kind"]["value"];
    assert_eq!(magma["colormap"]["type"], json!("named"));
    assert_eq!(magma["colormap"]["name"], json!("magma"));
    assert_eq!(magma["colormap"]["reverse"], json!(true));
    assert_eq!(magma["norm"], json!("linear"));
    assert_eq!(magma["below"]["show_key"], json!(true));
    let listed = &scales[1]["kind"]["value"];
    assert_eq!(listed["colormap"]["type"], json!("listed"));
    assert_eq!(
        listed["colormap"]["colors"].as_array().map(Vec::len),
        Some(3)
    );
    assert_eq!(listed["norm"], json!("log"));
    assert_eq!(
        scales[2]["kind"]["value"]["levels"],
        json!({"type": "explicit", "values": [0.0, 2.5, 5.0, 12.0]})
    );
    assert_eq!(scales[3]["kind"]["value"]["levels"]["type"], json!("auto"));
    assert_eq!(scales[3]["kind"]["value"]["levels"]["count"], json!(5));
}

// Why: a categorical key shows every category's label, so the type itself must refuse a category without one.
#[test]
fn a_category_without_a_label_cannot_be_read() {
    let mut fig = Figure::new();
    fig.color_scales.push(ColorScale {
        id: NodeId(7),
        kind: categorical(vec![RED, GREEN], OutOfRange::Strict),
        ..ColorScale::default()
    });
    let mut value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    let categories = &mut value["color_scales"][0]["kind"]["value"]["categories"];
    categories[1]
        .as_object_mut()
        .expect("a category is an object")
        .remove("label");
    let result = Figure::from_json(&value.to_string());
    assert!(result.is_err(), "a category without a label was accepted");
}

// Why: a scale built with defaults must behave as the plan promises: viridis and automatic linear limits, clamped ends, NaN drawn as nothing, and strict indices.
#[test]
fn each_kind_has_the_defaults_the_model_promises() {
    let scale = ColorScale::default();
    assert_eq!(scale.title, None);
    assert_eq!(scale.non_finite.policy, OutOfRange::Transparent);
    assert!(!scale.non_finite.show_key);
    assert_eq!(scale.non_finite.label, None);
    assert_eq!(
        scale.kind,
        ColorScaleKind::Continuous(ContinuousColorScale::default())
    );

    let c = ContinuousColorScale::default();
    assert_eq!(c.colormap, named(ColormapName::Viridis));
    assert_eq!(c.colormap, Colormap::default());
    assert_eq!(c.limits, Limits::Auto);
    assert_eq!(c.norm, ColorScaleNorm::Linear);
    assert_eq!(c.below.policy, OutOfRange::Clamp);
    assert_eq!(c.above.policy, OutOfRange::Clamp);
    assert!(!c.below.show_key && !c.above.show_key);
    assert_eq!(c.ticks, Ticks::Auto);
    assert_eq!(c.tick_labels, TickLabels::Auto);

    let d = DiscreteColorScale::default();
    assert_eq!(d.colormap, named(ColormapName::Viridis));
    assert_eq!(
        d.levels,
        ColorScaleLevels::Auto {
            count: 10,
            limits: Limits::Auto
        }
    );
    assert_eq!(d.levels, ColorScaleLevels::default());
    assert_eq!(d.norm, ColorScaleNorm::Linear);
    assert_eq!(d.below.policy, OutOfRange::Clamp);
    assert_eq!(d.above.policy, OutOfRange::Clamp);

    let i = IndexedColorScale::default();
    assert_eq!(i.outside.policy, OutOfRange::Strict);
    assert!(!i.outside.show_key);
    assert_eq!(i.ticks, Ticks::Auto);

    let k = CategoricalColorScale::default();
    assert_eq!(k.outside.policy, OutOfRange::Strict);
    assert!(k.categories.is_empty());
}

// Why: an artist built with defaults names no scale and a contour has no levels of its own, so that it follows its scale's band edges.
#[test]
fn colour_mapped_artists_name_no_scale_and_contours_no_levels_by_default() {
    assert_eq!(ScalarImage::default().color_scale, None);
    assert_eq!(ScalarImage::default().alpha, None);
    assert_eq!(ScalarImage::default().opacity, 1.0);
    assert!(ScalarImage::default().visible);
    assert_eq!(Contour::default().color_scale, None);
    assert_eq!(Contour::default().levels, None);
    assert_eq!(Surface::default().color_scale, None);
    assert_eq!(Scatter::default().color_scale, None);
}

// ---------------------------------------------------------------------------------
// One identifier space
// ---------------------------------------------------------------------------------

// Why: clients ask what a node is before they show or edit it, so a scale must be known and found by identifier like any node.
#[test]
fn a_colour_scale_is_a_node_known_and_found_by_its_identifier() {
    let fig = color_scale_variants_figure();
    for scale in &fig.color_scales {
        assert_eq!(fig.node_kind(scale.id), Some(NodeKind::ColorScale));
        assert_eq!(fig.color_scale(scale.id), Some(scale));
    }
    let image = fig.axes[0].artists[0].id();
    assert_eq!(fig.node_kind(image), Some(NodeKind::ScalarImage));
    assert_eq!(fig.color_scale(image), None, "an artist is not a scale");
    assert!(NodeKind::ALL.contains(&NodeKind::ColorScale));
    assert!(NodeKind::ALL.contains(&NodeKind::ScalarImage));
}

// Why: a reference names a node by identifier alone, so a scale sharing an identifier with another node makes the reference ambiguous.
#[test]
fn a_colour_scale_sharing_an_identifier_with_another_node_is_a_duplicate_node_id() {
    let mut fig = color_scale_variants_figure();
    let axes = fig.axes[0].id;
    let first = fig.color_scales[0].id;
    scale_mut(&mut fig, first).id = axes;
    assert!(
        kinds(&fig.validate().errors).contains(&IssueKind::DuplicateNodeId),
        "{:?}",
        fig.validate()
    );
}

// Why: a new node must never take the identifier of a scale, or an artist's reference to that scale would change meaning.
#[test]
fn allocation_lies_above_the_identifier_of_every_colour_scale() {
    let mut fig = Figure::new();
    fig.color_scales.push(ColorScale {
        id: NodeId(900),
        ..ColorScale::default()
    });
    assert!(fig.alloc_node_id().0 > 900);
}

// ---------------------------------------------------------------------------------
// Structural edits of scales
// ---------------------------------------------------------------------------------

// Why: a session adds a scale and the artist that uses it in one step, and undo must remove both and restore the figure exactly.
#[test]
fn a_scale_and_an_artist_that_names_it_are_inserted_in_one_transaction() {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let values = b.matrix(2, 2, |j, i| (j * 2 + i) as f64);
    let fig = b.build();
    let (scale, image) = (NodeId(500), NodeId(501));
    let insert = tx([
        Edit::Insert {
            parent: fig.id,
            index: None,
            node: Node::ColorScale(ColorScale {
                id: scale,
                kind: continuous(named(ColormapName::Plasma), Limits::Auto),
                ..ColorScale::default()
            }),
        },
        Edit::Insert {
            parent: axes,
            index: None,
            node: Node::Artist(Artist::ScalarImage(ScalarImage {
                id: image,
                values,
                color_scale: Some(scale),
                ..ScalarImage::default()
            })),
        },
    ]);
    let (edited, inverse) = applied(&fig, &insert).unwrap();
    assert_eq!(edited.validate().errors, vec![]);
    assert_eq!(edited.node_kind(scale), Some(NodeKind::ColorScale));
    assert_eq!(applied(&edited, &inverse).unwrap().0, fig);
}

// Why: removing a scale an artist still names would leave the artist unable to be coloured, so it is refused unless the artist goes too.
#[test]
fn removing_a_scale_that_an_artist_names_is_refused_unless_the_artist_goes_too() {
    let (fig, image, scale) = image_through(
        continuous(named(ColormapName::Viridis), Limits::Auto),
        &[0.0, 1.0],
    );
    let mut edited = fig.clone();
    let result = edited.apply(&tx([Edit::Remove { node: scale }]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues))
            if issues.iter().any(|issue| issue.node == Some(image))),
        "the dangling reference is reported against the artist: {result:?}"
    );
    assert_eq!(edited, fig, "a refused transaction changes nothing");

    let both = tx([Edit::Remove { node: image }, Edit::Remove { node: scale }]);
    let (removed, inverse) = applied(&fig, &both).unwrap();
    assert!(removed.color_scales.is_empty());
    assert_eq!(applied(&removed, &inverse).unwrap().0, fig);
}

// Why: a custom colourbar needs a scale that no artist uses, so an unused scale is neither an error nor a warning.
#[test]
fn a_scale_that_no_artist_uses_is_valid_without_a_warning() {
    let mut fig = Figure::new();
    fig.color_scales.push(ColorScale {
        id: NodeId(2),
        kind: indexed(vec![RED, GREEN], OutOfRange::Strict),
        ..ColorScale::default()
    });
    let report = fig.validate();
    assert_eq!(report.errors, vec![]);
    assert!(
        report.warnings.iter().all(|w| w.node != Some(NodeId(2))),
        "{report:?}"
    );
}

// ---------------------------------------------------------------------------------
// References from artists to scales
// ---------------------------------------------------------------------------------

/// A figure of one 2D axes holding one colour-mapped artist of every kind that takes a
/// scale, each built by `edit` from its defaults, with the given scale reference. Returns
/// the figure and the identifiers of the scalar image, the contour, the surface and the
/// scatter, in that order.
fn every_mapped_artist(
    color_scale: Option<NodeId>,
    with_scale: bool,
    edit: impl Fn(&mut Artist),
) -> (Figure, [NodeId; 4]) {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    if with_scale {
        b.fig.color_scales.push(ColorScale {
            id: NodeId(400),
            ..ColorScale::default()
        });
    }
    let gx = b.vector(&[0.0, 1.0, 2.0]);
    let gy = b.vector(&[0.0, 1.0]);
    let field = b.matrix(2, 3, |j, i| (j * 3 + i) as f64);
    let points = b.vector(&[0.0, 1.0, 2.0]);
    let ids = [b.node(), b.node(), b.node(), b.node()];
    let mut artists = vec![
        Artist::ScalarImage(ScalarImage {
            id: ids[0],
            values: field,
            color_scale,
            ..ScalarImage::default()
        }),
        Artist::Contour(Contour {
            id: ids[1],
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: field,
            color_scale,
            ..Contour::default()
        }),
        Artist::Surface(Surface {
            id: ids[2],
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: field,
            face: ColorSpec::Colormapped,
            edge: ColorSpec::Rgba {
                color: Color::BLACK,
            },
            color_scale,
            ..Surface::default()
        }),
        Artist::Scatter(Scatter {
            id: ids[3],
            x: points,
            y: points,
            color: ScatterColor::Data { data: points },
            color_scale,
            ..Scatter::default()
        }),
    ];
    for artist in &mut artists {
        edit(artist);
        b.push(axes, artist.clone());
    }
    (b.build(), ids)
}

// Why: there is no axes colormap to fall back on, so an artist that maps values to colours without naming a scale cannot be drawn and must be reported.
#[test]
fn a_colour_mapped_artist_that_names_no_scale_is_missing_color_scale() {
    let (fig, ids) = every_mapped_artist(None, false, |_| {});
    let report = fig.validate();
    for (name, id) in ["scalar image", "contour", "surface", "scatter"]
        .into_iter()
        .zip(ids)
    {
        assert!(
            has(&report.errors, IssueKind::MissingColorScale, id),
            "{name}: {report:?}"
        );
    }

    // A filled contour maps its bands even when its isolines have a fixed colour.
    let (fig, ids) = every_mapped_artist(None, false, |artist| {
        if let Artist::Contour(c) = artist {
            c.fill = true;
            c.line.color = ColorSpec::Rgba {
                color: Color::BLACK,
            };
        }
    });
    assert!(has(
        &fig.validate().errors,
        IssueKind::MissingColorScale,
        ids[1]
    ));

    // Colour-mapped edges on a surface with fixed faces still need a scale.
    let (fig, ids) = every_mapped_artist(None, false, |artist| {
        if let Artist::Surface(s) = artist {
            s.face = ColorSpec::Rgba {
                color: Color::WHITE,
            };
            s.edge = ColorSpec::Colormapped;
        }
    });
    assert!(has(
        &fig.validate().errors,
        IssueKind::MissingColorScale,
        ids[2]
    ));
}

// Why: a contour's isolines and a surface's faces and edges have no colour of their own, so the scene draws an automatic colour there through the scale exactly as a colormapped one; the IR must agree, or such an artist would be drawn without a scale to draw it through and would take no part in its scale's automatic limits.
#[test]
fn an_automatic_isoline_face_or_edge_colour_is_colour_mapped() {
    let fixed = ColorSpec::Rgba {
        color: Color::BLACK,
    };
    let (fig, ids) = every_mapped_artist(None, false, |artist| match artist {
        Artist::Contour(c) => {
            c.fill = false;
            c.line.color = ColorSpec::Auto;
        }
        Artist::Surface(s) => {
            s.face = ColorSpec::Auto;
            s.edge = fixed;
        }
        _ => {}
    });
    let report = fig.validate();
    for (name, id) in [("contour", ids[1]), ("surface", ids[2])] {
        assert!(
            has(&report.errors, IssueKind::MissingColorScale, id),
            "{name}: {report:?}"
        );
    }

    // Automatic surface edges over fixed faces are colour-mapped too.
    let (fig, ids) = every_mapped_artist(None, false, |artist| {
        if let Artist::Surface(s) = artist {
            s.face = fixed;
            s.edge = ColorSpec::Auto;
        }
    });
    assert!(has(
        &fig.validate().errors,
        IssueKind::MissingColorScale,
        ids[2]
    ));

    // With a scale named, an automatic contour's field reaches the scale's automatic
    // limits, as a colormapped contour's does.
    let (fig, ids) = every_mapped_artist(Some(NodeId(400)), true, |artist| {
        if let Artist::Contour(c) = artist {
            c.line.color = ColorSpec::Auto;
        }
    });
    let Some((_, Artist::Contour(contour))) = fig.artist(ids[1]) else {
        panic!("the contour exists");
    };
    assert!(
        fig.color_scale_values(NodeId(400)).contains(&contour.z),
        "the automatic contour maps its field through the scale"
    );
}

// Why: an artist drawn in fixed colours maps nothing, so it must not be made to name a scale it would never use.
#[test]
fn an_artist_drawn_in_fixed_colours_needs_no_scale() {
    let fixed = ColorSpec::Rgba {
        color: Color::BLACK,
    };
    let (fig, ids) = every_mapped_artist(None, false, |artist| match artist {
        Artist::Contour(c) => {
            c.fill = false;
            c.line.color = fixed;
        }
        Artist::Surface(s) => {
            s.face = fixed;
            s.edge = fixed;
        }
        Artist::Scatter(s) => s.color = ScatterColor::Spec { spec: fixed },
        _ => {}
    });
    let report = fig.validate();
    for id in &ids[1..] {
        assert!(
            !has(&report.errors, IssueKind::MissingColorScale, *id),
            "{report:?}"
        );
    }
    // A scalar image always maps its values, so it alone still needs a scale.
    assert!(has(&report.errors, IssueKind::MissingColorScale, ids[0]));
}

// Why: every artist that takes a scale accepts any kind of scale, and naming one makes a colour-mapped artist valid.
#[test]
fn naming_a_scale_of_any_kind_makes_every_colour_mapped_artist_valid() {
    let kinds = [
        continuous(named(ColormapName::Viridis), Limits::Auto),
        discrete(&[0.0, 2.0, 5.0]),
        indexed(vec![RED, GREEN, BLUE, RED, GREEN, BLUE], OutOfRange::Strict),
        categorical(vec![RED, GREEN, BLUE, RED, GREEN, BLUE], OutOfRange::Strict),
    ];
    for kind in kinds {
        // Whole-number contour levels, so that the levels of an index-mapped scale name
        // entries; how levels that are not whole numbers are judged is not this test's
        // concern.
        let (mut fig, _) = every_mapped_artist(Some(NodeId(400)), true, |artist| {
            if let Artist::Contour(c) = artist {
                c.levels = Some(Levels::Explicit {
                    values: vec![1.0, 2.0, 3.0],
                });
            }
        });
        scale_mut(&mut fig, NodeId(400)).kind = kind.clone();
        assert_eq!(fig.validate().errors, vec![], "{kind:?}");
    }
}

// Why: in one identifier space a scale reference can name a node of another kind, which must be reported as such against the artist.
#[test]
fn a_scale_reference_naming_a_node_of_another_kind_is_wrong_node_kind() {
    let (fig, ids) = every_mapped_artist(Some(NodeId(400)), true, |_| {});
    let axes = fig.axes[0].id;
    let data = fig.data[0].id;
    for (name, target) in [
        ("an axes", axes),
        ("a data array", data),
        ("an artist", ids[3]),
    ] {
        for (k, id) in ids.into_iter().enumerate() {
            let mut wrong = fig.clone();
            match wrong.artist_mut(id).unwrap() {
                Artist::ScalarImage(a) => a.color_scale = Some(target),
                Artist::Contour(a) => a.color_scale = Some(target),
                Artist::Surface(a) => a.color_scale = Some(target),
                Artist::Scatter(a) => a.color_scale = Some(target),
                other => panic!("unexpected artist {other:?}"),
            }
            let report = wrong.validate();
            assert!(
                has(&report.errors, IssueKind::WrongNodeKind, id),
                "artist {k} naming {name}: {report:?}"
            );
            assert!(
                !kinds(&report.errors).contains(&IssueKind::MissingColorScale),
                "artist {k} naming {name} names something: {report:?}"
            );
        }
    }
}

// Why: a data reference that names a scale is as wrong as one naming an axes, so the one identifier space is checked in both directions.
#[test]
fn a_data_reference_naming_a_scale_is_wrong_node_kind() {
    let (mut fig, ids) = every_mapped_artist(Some(NodeId(400)), true, |_| {});
    match fig.artist_mut(ids[0]).unwrap() {
        Artist::ScalarImage(a) => a.values = NodeId(400),
        other => panic!("unexpected artist {other:?}"),
    }
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::WrongNodeKind, ids[0]),
        "{report:?}"
    );
    assert!(!kinds(&report.errors).contains(&IssueKind::UnknownData));
}

// Why: a scale reference that names no node cannot be drawn; the error must point at the artist that holds it and say that the scale is unknown, not that the artist names none.
#[test]
fn a_scale_reference_naming_no_node_is_unknown_color_scale_against_the_artist() {
    let (fig, ids) = every_mapped_artist(Some(NodeId(999)), false, |_| {});
    let report = fig.validate();
    for id in ids {
        assert!(
            has(&report.errors, IssueKind::UnknownColorScale, id),
            "{id}: {report:?}"
        );
    }
    assert!(
        !kinds(&report.errors).contains(&IssueKind::MissingColorScale),
        "every artist names a scale, though not one that exists: {report:?}"
    );
}

// Why: a line or quiver has no values to map in this stage, so a colormapped colour keeps today's drawing (the middle of viridis) and must not make the figure invalid for want of a scale it cannot name.
#[test]
fn a_colormapped_line_quiver_or_single_scatter_colour_needs_no_scale() {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let points = b.vector(&[0.0, 1.0]);
    let (line, quiver, scatter) = (b.node(), b.node(), b.node());
    let style = LineStyle {
        color: ColorSpec::Colormapped,
        ..LineStyle::default()
    };
    b.push(
        axes,
        Artist::Line(Line {
            id: line,
            x: points,
            y: points,
            line: style,
            ..Line::default()
        }),
    );
    b.push(
        axes,
        Artist::Quiver(Quiver {
            id: quiver,
            x: points,
            y: points,
            u: points,
            v: points,
            line: style,
            ..Quiver::default()
        }),
    );
    b.push(
        axes,
        Artist::Scatter(Scatter {
            id: scatter,
            x: points,
            y: points,
            color: ScatterColor::Spec {
                spec: ColorSpec::Colormapped,
            },
            ..Scatter::default()
        }),
    );
    let report = b.build().validate();
    assert_eq!(report.errors, vec![], "{report:?}");
}

// Why: a property editor may point an artist at any identifier, so an edit naming the wrong kind of node as a scale must be refused.
#[test]
fn an_edit_pointing_a_scale_reference_at_an_axes_is_refused() {
    let (fig, image, _) = image_through(
        continuous(named(ColormapName::Viridis), Limits::Auto),
        &[0.0, 1.0],
    );
    let axes = fig.axes[0].id;
    let mut edited = fig.clone();
    let result = edited.apply(&tx([set(image, "color_scale", Value::NodeId(axes))]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues))
            if has(issues, IssueKind::WrongNodeKind, image)),
        "{result:?}"
    );
    assert_eq!(edited, fig);
}

// ---------------------------------------------------------------------------------
// Values outside an index-mapped scale
// ---------------------------------------------------------------------------------

// Why: a strict index-mapped scale is the author's statement that every value names an entry, so 2.7 or 3 of three entries is refused, never silently drawn as another entry.
#[test]
fn a_fractional_or_out_of_range_index_is_value_out_of_range_under_strict() {
    let colors = vec![RED, GREEN, BLUE];
    for (name, kind) in [
        ("indexed", indexed(colors.clone(), OutOfRange::Strict)),
        (
            "categorical",
            categorical(colors.clone(), OutOfRange::Strict),
        ),
    ] {
        for (what, values) in [
            ("a fractional index", vec![0.0, 1.5]),
            ("an index one past the last entry", vec![0.0, 3.0]),
            ("a negative index", vec![-1.0, 2.0]),
            ("a small fraction", vec![2.0, 0.25]),
        ] {
            let (fig, image, _) = image_through(kind.clone(), &values);
            let report = fig.validate();
            assert!(
                has(&report.errors, IssueKind::ValueOutOfRange, image),
                "{name}, {what}: {report:?}"
            );
        }
        let (fig, image, _) = byte_image_through(kind.clone(), &[0, 1, 2, 3]);
        assert!(
            has(&fig.validate().errors, IssueKind::ValueOutOfRange, image),
            "{name}: a byte past the last entry"
        );

        // Every whole number from 0 to len − 1, as floats and as bytes, and NaN, which
        // the default non-finite policy draws as nothing, are all in range.
        let (fig, _, _) = image_through(kind.clone(), &[0.0, 1.0, 2.0, f64::NAN, -0.0]);
        assert_eq!(fig.validate().errors, vec![], "{name}");
        let (fig, _, _) = byte_image_through(kind.clone(), &[0, 1, 2]);
        assert_eq!(fig.validate().errors, vec![], "{name}");
    }
}

// Why: an author who chooses a lenient policy outside the entries has said what to draw there, so no value is refused.
#[test]
fn a_lenient_outside_policy_accepts_any_finite_index() {
    let colors = vec![RED, GREEN, BLUE];
    for policy in [
        OutOfRange::Transparent,
        OutOfRange::Clamp,
        OutOfRange::Rgba {
            color: Color::BLACK,
        },
    ] {
        for kind in [
            indexed(colors.clone(), policy),
            categorical(colors.clone(), policy),
        ] {
            let (fig, _, _) = image_through(kind.clone(), &[-4.0, 0.5, 2.9, 3.0, 1e9]);
            assert_eq!(fig.validate().errors, vec![], "{kind:?}");
        }
    }
}

// Why: colour data of any artist follows the scale's rules, so a scatter's fractional class through a strict indexed scale is refused like an image's.
#[test]
fn a_scatter_coloured_by_a_fractional_class_through_a_strict_indexed_scale_is_value_out_of_range() {
    let (mut fig, ids) = every_mapped_artist(Some(NodeId(400)), true, |_| {});
    scale_mut(&mut fig, NodeId(400)).kind = indexed(vec![RED, GREEN, BLUE], OutOfRange::Strict);
    // The scatter gets colour data of its own, so that no other artist sees the change.
    let colours = NodeId(1_500);
    fig.data.push(DataArray {
        id: colours,
        name: None,
        array: NdArray::vector(vec![0.0, 0.5, 2.0]),
    });
    match fig.artist_mut(ids[3]).unwrap() {
        Artist::Scatter(s) => s.color = ScatterColor::Data { data: colours },
        other => panic!("unexpected artist {other:?}"),
    }
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::ValueOutOfRange, ids[3]),
        "{report:?}"
    );
}

// Why: the strict policy on a continuous or discrete scale refuses values outside manual limits, and on any scale refuses NaN, as the image policies did.
#[test]
fn strict_below_above_and_non_finite_categories_refuse_their_values() {
    let strict = out_of_range(OutOfRange::Strict);
    let limited = |below: ColorScaleOutOfRange, above: ColorScaleOutOfRange| {
        ColorScaleKind::Continuous(ContinuousColorScale {
            limits: Limits::Manual {
                min: 0.0,
                max: 10.0,
            },
            below,
            above,
            ..ContinuousColorScale::default()
        })
    };
    let clamp = out_of_range(OutOfRange::Clamp);
    for (what, kind, values) in [
        (
            "below",
            limited(strict.clone(), clamp.clone()),
            vec![-1.0, 5.0],
        ),
        (
            "above",
            limited(clamp.clone(), strict.clone()),
            vec![11.0, 5.0],
        ),
        (
            "below a discrete scale",
            ColorScaleKind::Discrete(DiscreteColorScale {
                levels: ColorScaleLevels::Explicit {
                    values: vec![0.0, 5.0, 10.0],
                },
                below: strict.clone(),
                ..DiscreteColorScale::default()
            }),
            vec![-1.0, 5.0],
        ),
    ] {
        let (fig, image, _) = image_through(kind, &values);
        assert!(
            has(&fig.validate().errors, IssueKind::ValueOutOfRange, image),
            "{what}"
        );
    }

    // A value exactly at a limit, or at the first or last band edge, lies inside the
    // range, so a strict category does not refuse it.
    let (fig, _, _) = image_through(limited(strict.clone(), strict.clone()), &[0.0, 10.0]);
    assert_eq!(fig.validate().errors, vec![], "values at the limits");
    let edges = ColorScaleKind::Discrete(DiscreteColorScale {
        levels: ColorScaleLevels::Explicit {
            values: vec![0.0, 5.0, 10.0],
        },
        below: strict.clone(),
        above: strict.clone(),
        ..DiscreteColorScale::default()
    });
    let (fig, _, _) = image_through(edges, &[0.0, 5.0, 10.0]);
    assert_eq!(fig.validate().errors, vec![], "values at the band edges");

    // Automatic limits are the range of the data, so nothing lies outside them.
    let auto = ColorScaleKind::Continuous(ContinuousColorScale {
        below: strict.clone(),
        above: strict.clone(),
        ..ContinuousColorScale::default()
    });
    let (fig, _, _) = image_through(auto, &[-100.0, 5.0, 100.0]);
    assert_eq!(fig.validate().errors, vec![]);

    // Under a logarithmic norm, automatic limits range over the positive values only, so
    // zero and a negative value lie below them and a strict `below` refuses them.
    let log_auto = ColorScaleKind::Continuous(ContinuousColorScale {
        norm: ColorScaleNorm::Log,
        below: strict.clone(),
        ..ContinuousColorScale::default()
    });
    let (fig, _, _) = image_through(log_auto.clone(), &[1.0, 10.0, 100.0]);
    assert_eq!(fig.validate().errors, vec![], "positive values under log");
    for offending in [0.0, -3.0] {
        let (fig, image, _) = image_through(log_auto.clone(), &[offending, 1.0, 10.0]);
        assert!(
            has(&fig.validate().errors, IssueKind::ValueOutOfRange, image),
            "{offending} under a logarithmic norm with automatic limits"
        );
    }

    // A strict non-finite category refuses NaN on every kind.
    for kind in [
        continuous(named(ColormapName::Viridis), Limits::Auto),
        discrete(&[0.0, 1.0, 2.0]),
        indexed(vec![RED, GREEN], OutOfRange::Strict),
        categorical(vec![RED, GREEN], OutOfRange::Strict),
    ] {
        let (mut fig, image, scale) = image_through(kind.clone(), &[0.0, f64::NAN]);
        scale_mut(&mut fig, scale).non_finite = strict.clone();
        assert!(
            has(&fig.validate().errors, IssueKind::ValueOutOfRange, image),
            "{kind:?}"
        );
    }
}

// ---------------------------------------------------------------------------------
// Validation of the scale itself
// ---------------------------------------------------------------------------------

// Why: a listed colormap interpolates between its colours, which needs at least two, and #40 lifts the limit of 256 to 1024 entries, no further.
#[test]
fn a_listed_colormap_holds_from_two_to_1024_colours() {
    let colors = |n: usize| -> Vec<Color> {
        (0..n)
            .map(|k| Color::rgb((k % 256) as f32 / 255.0, 0.0, 0.0))
            .collect()
    };
    for (n, valid) in [
        (0, false),
        (1, false),
        (2, true),
        (1024, true),
        (1025, false),
    ] {
        let listed = Colormap::Listed { colors: colors(n) };
        for kind in [
            continuous(listed.clone(), Limits::Auto),
            ColorScaleKind::Discrete(DiscreteColorScale {
                colormap: listed.clone(),
                ..DiscreteColorScale::default()
            }),
        ] {
            let (fig, _, scale) = image_through(kind, &[0.0, 1.0]);
            let report = fig.validate();
            assert_eq!(
                !has(&report.errors, IssueKind::InvalidColormap, scale),
                valid,
                "{n} colours: {report:?}"
            );
        }
    }
}

// Why: an index-mapped scale with no entries maps no value to a colour, so it is refused against the scale, while one entry is a valid palette.
#[test]
fn an_indexed_or_categorical_scale_needs_at_least_one_entry() {
    for (name, empty, one) in [
        (
            "indexed",
            indexed(vec![], OutOfRange::Transparent),
            indexed(vec![RED], OutOfRange::Transparent),
        ),
        (
            "categorical",
            categorical(vec![], OutOfRange::Transparent),
            categorical(vec![RED], OutOfRange::Transparent),
        ),
    ] {
        let (fig, _, scale) = image_through(empty, &[0.0]);
        assert!(
            has(&fig.validate().errors, IssueKind::InvalidColorScale, scale),
            "{name} with no entries: {:?}",
            fig.validate()
        );
        let (fig, _, _) = image_through(one, &[0.0]);
        assert_eq!(fig.validate().errors, vec![], "{name} with one entry");
    }
    // A scale that no artist uses is checked too, because a colourbar may show it.
    let mut fig = Figure::new();
    fig.color_scales.push(ColorScale {
        id: NodeId(3),
        kind: indexed(vec![], OutOfRange::Strict),
        ..ColorScale::default()
    });
    assert!(has(
        &fig.validate().errors,
        IssueKind::InvalidColorScale,
        NodeId(3)
    ));
}

// Why: a strict index-mapped scale judges the author's data, not values the compiler derives from it, so a surface whose nodes all name entries is valid although its face means are fractional, and a contour with automatic levels is valid although its levels lie between entries.
#[test]
fn index_mapped_validation_checks_the_data_values_not_derived_ones() {
    for kind in [
        indexed(vec![RED, GREEN], OutOfRange::Strict),
        categorical(vec![RED, GREEN], OutOfRange::Strict),
    ] {
        let (mut fig, ids) = every_mapped_artist(Some(NodeId(400)), true, |artist| {
            if let Artist::Contour(c) = artist {
                c.levels = None;
                c.fill = true;
            }
        });
        scale_mut(&mut fig, NodeId(400)).kind = kind.clone();
        // A field of alternating 0 and 1, whose every face has the corner mean 0.5. The
        // scatter is given whole-number colour data of its own.
        let field = fig
            .data
            .iter()
            .find(|d| d.array.shape == [2, 3])
            .unwrap()
            .id;
        fig.data_array_mut(field).unwrap().array =
            NdArray::from_shape(vec![2, 3], vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0]).unwrap();
        let classes = NodeId(1_600);
        fig.data.push(DataArray {
            id: classes,
            name: None,
            array: NdArray::vector(vec![0.0, 1.0, 1.0]),
        });
        match fig.artist_mut(ids[3]).unwrap() {
            Artist::Scatter(s) => s.color = ScatterColor::Data { data: classes },
            other => panic!("unexpected artist {other:?}"),
        }
        assert_eq!(fig.validate().errors, vec![], "{kind:?}");

        // A node that names no entry is refused against every artist that maps it.
        fig.data_array_mut(field).unwrap().array =
            NdArray::from_shape(vec![2, 3], vec![0.0, 1.0, 0.0, 1.0, 0.0, 2.0]).unwrap();
        let report = fig.validate();
        for id in &ids[..3] {
            assert!(
                has(&report.errors, IssueKind::ValueOutOfRange, *id),
                "{kind:?}, {id}: {report:?}"
            );
        }
    }
}

// Why: a 1024-entry indexed scale is the point of #40, so every index up to 1023 must be accepted and 1024 refused.
#[test]
fn an_indexed_scale_of_1024_entries_accepts_every_index_up_to_1023() {
    let colors: Vec<Color> = (0..1024)
        .map(|k| Color::rgb((k % 256) as f32 / 255.0, (k / 256) as f32 / 3.0, 0.5))
        .collect();
    let kind = indexed(colors, OutOfRange::Strict);
    let (fig, _, _) = image_through(kind.clone(), &[0.0, 255.0, 256.0, 1023.0]);
    assert_eq!(fig.validate().errors, vec![]);
    let (fig, image, _) = image_through(kind, &[1024.0]);
    assert!(has(
        &fig.validate().errors,
        IssueKind::ValueOutOfRange,
        image
    ));
}

// Why: band edges must cut the values into ordered bands, so edges that are fewer than two, repeated, unordered or not finite are refused against the scale.
#[test]
fn explicit_band_edges_must_be_at_least_two_finite_and_strictly_increasing() {
    for (edges, valid) in [
        (vec![], false),
        (vec![1.0], false),
        (vec![0.0, 1.0], true),
        (vec![0.0, 2.0, 1.0], false),
        (vec![0.0, 1.0, 1.0], false),
        (vec![0.0, f64::NAN, 1.0], false),
        (vec![0.0, f64::INFINITY], false),
    ] {
        let (fig, _, scale) = image_through(discrete(&edges), &[0.5]);
        let report = fig.validate();
        assert_eq!(
            !has(&report.errors, IssueKind::InvalidLevels, scale),
            valid,
            "{edges:?}: {report:?}"
        );
    }
    let zero = ColorScaleKind::Discrete(DiscreteColorScale {
        levels: ColorScaleLevels::Auto {
            count: 0,
            limits: Limits::Auto,
        },
        ..DiscreteColorScale::default()
    });
    let (fig, _, scale) = image_through(zero, &[0.5]);
    assert!(has(&fig.validate().errors, IssueKind::InvalidLevels, scale));
}

// Why: manual limits that are not finite and increasing, or not positive under a logarithmic norm, place no value, so they are refused against the scale.
#[test]
fn manual_limits_must_be_finite_increasing_and_positive_under_a_log_norm() {
    let with = |limits: Limits, norm: ColorScaleNorm| {
        ColorScaleKind::Continuous(ContinuousColorScale {
            limits,
            norm,
            ..ContinuousColorScale::default()
        })
    };
    let manual = |min: f64, max: f64| Limits::Manual { min, max };
    for (limits, norm, valid) in [
        (manual(0.0, 1.0), ColorScaleNorm::Linear, true),
        (manual(1.0, 1.0), ColorScaleNorm::Linear, false),
        (manual(2.0, 1.0), ColorScaleNorm::Linear, false),
        (manual(0.0, f64::INFINITY), ColorScaleNorm::Linear, false),
        (manual(1.0, 1000.0), ColorScaleNorm::Log, true),
        (manual(0.0, 1000.0), ColorScaleNorm::Log, false),
        (manual(-10.0, 1000.0), ColorScaleNorm::Log, false),
    ] {
        let (fig, _, scale) = image_through(with(limits, norm), &[1.0, 10.0]);
        let report = fig.validate();
        assert_eq!(
            !has(&report.errors, IssueKind::InvalidLimits, scale),
            valid,
            "{limits:?} {norm:?}: {report:?}"
        );
    }

    // The limits of automatic band edges are checked in the same way.
    let log_bands = ColorScaleKind::Discrete(DiscreteColorScale {
        levels: ColorScaleLevels::Auto {
            count: 5,
            limits: manual(0.0, 100.0),
        },
        norm: ColorScaleNorm::Log,
        ..DiscreteColorScale::default()
    });
    let (fig, _, scale) = image_through(log_bands, &[1.0, 10.0]);
    assert!(has(&fig.validate().errors, IssueKind::InvalidLimits, scale));
}

// ---------------------------------------------------------------------------------
// Properties and edits
// ---------------------------------------------------------------------------------

/// Returns the paths that the registry lists for a kind of node.
fn registry_paths(kind: NodeKind) -> Vec<String> {
    properties(kind)
        .into_iter()
        .map(|p| p.path.to_string())
        .collect()
}

// Why: the property editor edits a scale through the registry, so the title, the kind, every out-of-range category and the limits must be listed.
#[test]
fn the_registry_lists_the_properties_of_a_colour_scale() {
    let listed = registry_paths(NodeKind::ColorScale);
    for expected in [
        "title",
        "kind",
        "non_finite",
        "non_finite.policy",
        "non_finite.show_key",
        "non_finite.label",
        "kind.continuous.colormap",
        "kind.continuous.limits",
        "kind.continuous.limits.min",
        "kind.continuous.limits.max",
        "kind.continuous.norm",
        "kind.continuous.below.policy",
        "kind.continuous.above.show_key",
        "kind.continuous.ticks",
        "kind.continuous.tick_labels",
        "kind.discrete.colormap",
        "kind.discrete.levels",
        "kind.discrete.norm",
        "kind.discrete.below.policy",
        "kind.discrete.above.policy",
        "kind.indexed.outside.policy",
        "kind.indexed.ticks",
        "kind.categorical.outside.policy",
    ] {
        assert!(
            listed.contains(&expected.to_owned()),
            "the registry lacks {expected}: {listed:?}"
        );
    }
    assert!(
        !listed.contains(&"id".to_owned()),
        "an identifier is read-only"
    );
}

// Why: each colour-mapped artist names its scale as an optional reference, and the policies and colormap that moved to the scale must no longer be listed where they were.
#[test]
fn artists_list_their_scale_reference_and_the_axes_no_colormap_or_limits() {
    for kind in [
        NodeKind::ScalarImage,
        NodeKind::Contour,
        NodeKind::Surface,
        NodeKind::Scatter,
    ] {
        let property = properties(kind)
            .into_iter()
            .find(|p| p.path.to_string() == "color_scale")
            .unwrap_or_else(|| panic!("{kind:?} lists no colour scale"));
        assert_eq!(property.value_type, ValueType::NodeId, "{kind:?}");
        assert!(property.optional, "{kind:?}");
    }
    let image = registry_paths(NodeKind::ScalarImage);
    for expected in [
        "label",
        "visible",
        "values",
        "placement",
        "alpha",
        "opacity",
    ] {
        assert!(
            image.contains(&expected.to_owned()),
            "{expected}: {image:?}"
        );
    }
    for gone in ["below", "above", "non_finite", "indices"] {
        assert!(!image.contains(&gone.to_owned()), "{gone}: {image:?}");
    }
    let axes = registry_paths(NodeKind::Axes);
    assert!(
        !axes.contains(&"colormap".to_owned()) && !axes.contains(&"clim".to_owned()),
        "{axes:?}"
    );
    let contour = properties(NodeKind::Contour)
        .into_iter()
        .find(|p| p.path.to_string() == "levels")
        .expect("a contour lists its levels");
    assert!(contour.optional, "a contour's levels are optional");
}

// Why: changing a scale's limits is one edit on the scale, which every artist that names it follows, and undo restores the old limits.
#[test]
fn one_set_of_a_scale_limits_is_accepted_and_undone() {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(1, 2);
    let left = b.axes2d(0, 0);
    let right = b.axes2d(0, 1);
    let scale = b.color_scale(continuous(named(ColormapName::Viridis), Limits::Auto));
    let values = b.matrix(1, 2, |_, i| i as f64);
    let images = [
        b.scalar_image(left, values, Some(scale)),
        b.scalar_image(right, values, Some(scale)),
    ];
    let fig = b.build();
    let limits = Limits::Manual { min: 0.0, max: 2.0 };
    let (edited, inverse) = applied(
        &fig,
        &tx([set(scale, "kind.continuous.limits", Value::Limits(limits))]),
    )
    .unwrap();
    assert_eq!(
        edited.get(scale, &path("kind.continuous.limits")).unwrap(),
        Value::Limits(limits)
    );
    assert_eq!(
        edited
            .get(scale, &path("kind.continuous.limits.max"))
            .unwrap(),
        Value::Double(2.0)
    );
    for image in images {
        assert_eq!(
            edited.get(image, &path("color_scale")).unwrap(),
            Value::NodeId(scale),
            "the artists still name the one scale"
        );
    }
    assert_eq!(applied(&edited, &inverse).unwrap().0, fig);
}

// Why: a path into another kind of scale is a stale edit, as a path into another variant always is, and must be refused without change.
#[test]
fn a_path_into_another_kind_of_scale_is_an_inactive_variant() {
    let (fig, _, scale) = image_through(indexed(vec![RED, GREEN], OutOfRange::Strict), &[0.0, 1.0]);
    let mut edited = fig.clone();
    let result = edited.apply(&tx([set(
        scale,
        "kind.continuous.limits",
        Value::Limits(Limits::Auto),
    )]));
    assert!(
        matches!(result, Err(EditError::InactiveVariant { .. })),
        "{result:?}"
    );
    assert_eq!(edited, fig);
}

// Why: the policy of each category of a scale is edited by path, and the kind's own categories are kept apart from the scale's non-finite one.
#[test]
fn the_policies_of_a_scale_are_set_by_path_and_undone() {
    let (fig, _, scale) = image_through(indexed(vec![RED, GREEN], OutOfRange::Strict), &[0.0, 1.0]);
    let (edited, inverse) = applied(
        &fig,
        &tx([
            set(
                scale,
                "kind.indexed.outside.policy",
                Value::OutOfRange(OutOfRange::Clamp),
            ),
            set(scale, "kind.indexed.outside.show_key", Value::Bool(true)),
            set(
                scale,
                "non_finite.policy",
                Value::OutOfRange(OutOfRange::Rgba { color: RED }),
            ),
            set(
                scale,
                "non_finite.label",
                Value::Text(Text::plain("not entered")),
            ),
        ]),
    )
    .unwrap();
    let edited_scale = scale_of(&edited, scale);
    let ColorScaleKind::Indexed(kind) = &edited_scale.kind else {
        panic!("the scale stayed indexed: {:?}", edited_scale.kind);
    };
    assert_eq!(kind.outside.policy, OutOfRange::Clamp);
    assert!(kind.outside.show_key);
    assert_eq!(
        edited_scale.non_finite.policy,
        OutOfRange::Rgba { color: RED }
    );
    assert_eq!(
        edited_scale.non_finite.label,
        Some(Text::plain("not entered"))
    );
    assert_eq!(applied(&edited, &inverse).unwrap().0, fig);
}

// ---------------------------------------------------------------------------------
// Choices offered by the property editor
// ---------------------------------------------------------------------------------

/// Returns whether the colormapped choice at a path of a node is offered as available.
fn colormapped_available(fig: &Figure, node: NodeId, at: &str) -> bool {
    property_choices(fig, node, &path(at))
        .into_iter()
        .find(|choice| choice.label == "Colormapped")
        .unwrap_or_else(|| panic!("no colormapped choice at {at}"))
        .available()
}

// Why: a colormapped colour is honoured only when the artist names a scale and has values to map, so the editor must offer it exactly then.
#[test]
fn colormapped_is_available_exactly_when_the_artist_names_a_scale_and_has_values_to_map() {
    for (with_scale, reference) in [(true, Some(NodeId(400))), (false, None)] {
        let (fig, ids) = every_mapped_artist(reference, with_scale, |_| {});
        assert_eq!(
            colormapped_available(&fig, ids[1], "line.color"),
            with_scale,
            "contour isolines with a scale: {with_scale}"
        );
        for at in ["face", "edge"] {
            assert_eq!(
                colormapped_available(&fig, ids[2], at),
                with_scale,
                "surface {at} with a scale: {with_scale}"
            );
        }
        // A marker takes the colour of its plot, whatever the scatter's scale.
        assert!(!colormapped_available(&fig, ids[3], "marker.face"));
    }

    // A scatter of one colour has no values to map, even when it names a scale.
    let (fig, ids) = every_mapped_artist(Some(NodeId(400)), true, |artist| {
        if let Artist::Scatter(s) = artist {
            s.color = ScatterColor::Spec {
                spec: ColorSpec::Auto,
            };
        }
    });
    assert!(!colormapped_available(&fig, ids[3], "color.spec"));
}

// Why: a non-finite value has no nearest end to be clamped to, so clamping is offered disabled at a scale's non-finite category and enabled at the others.
#[test]
fn clamping_is_unavailable_at_the_non_finite_category_of_a_scale() {
    let clamp_available = |fig: &Figure, scale: NodeId, at: &str| {
        property_choices(fig, scale, &path(at))
            .into_iter()
            .find(|choice| choice.label == "Clamp")
            .unwrap_or_else(|| panic!("no clamp choice at {at}"))
            .available()
    };
    let (fig, _, scale) = image_through(
        continuous(named(ColormapName::Viridis), Limits::Auto),
        &[0.0, 1.0],
    );
    assert!(!clamp_available(&fig, scale, "non_finite.policy"));
    assert!(clamp_available(&fig, scale, "kind.continuous.below.policy"));
    assert!(clamp_available(&fig, scale, "kind.continuous.above.policy"));
    let (fig, _, scale) = image_through(indexed(vec![RED, GREEN], OutOfRange::Strict), &[0.0, 1.0]);
    assert!(!clamp_available(&fig, scale, "non_finite.policy"));
    assert!(clamp_available(&fig, scale, "kind.indexed.outside.policy"));
}
