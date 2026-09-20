//! Property paths and the property registry: parsing, reading and setting properties by
//! path, the errors for paths that cannot be set, and the agreement of the registry with
//! the IR and its wire schema.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::edits::{applied, nodes, path, perturb, representative_figures, set, tx};
use common::{OPTIONAL_IN_IR, find_axes, kitchen_sink_figure, single_line_figure};
use ironlab_ir::*;
use prost_reflect::{
    DescriptorPool, DynamicMessage, FieldDescriptor, Kind, MessageDescriptor, ReflectMessage,
    Value as WireValue,
};

// ---------------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------------

// Why: clients in every language write paths as dot-separated strings, and the viewer
// shows them back to the user, so parsing and display must be exact inverses and must
// split on dots only.
#[test]
fn a_path_parses_from_and_displays_as_dot_separated_field_names() {
    for text in [
        "title",
        "x.limits",
        "projection.view3d.azimuth_deg",
        "box",
        "line.color.color.r",
    ] {
        let parsed = path(text);
        assert_eq!(parsed.to_string(), text);
        assert_eq!(
            parsed.segments(),
            text.split('.').collect::<Vec<_>>().as_slice()
        );
        assert_eq!(PropertyPath::new(text.split('.')).unwrap(), parsed);
    }
}

// Why: an empty segment can never name a field, and accepting one would let `x..limits`
// and `x.limits` denote different entries of an overlay for the same property. The error
// must say where the empty segment is, so that a client can point at it.
#[test]
fn a_path_with_no_segments_or_an_empty_segment_is_rejected() {
    assert_eq!("".parse::<PropertyPath>(), Err(PathError::Empty));
    assert_eq!(
        PropertyPath::new(Vec::<String>::new()),
        Err(PathError::Empty)
    );
    for (text, position) in [(".", 0), (".x", 0), ("x.", 1), ("x..limits", 1)] {
        let result = text.parse::<PropertyPath>();
        assert!(
            matches!(&result, Err(PathError::EmptySegment { position: p, .. }) if *p == position),
            "{text:?}: {result:?}"
        );
    }
    assert!(matches!(
        PropertyPath::new(["x", ""]),
        Err(PathError::EmptySegment { position: 1, .. })
    ));
}

// Why: a path travels as a single dotted string, so a segment that contains a dot would
// be written as one path and read back as another; `new` must refuse it rather than
// create a path that does not survive the wire.
#[test]
fn a_segment_containing_a_dot_is_rejected() {
    assert_eq!(
        PropertyPath::new(["x.limits", "min"]),
        Err(PathError::SeparatorInSegment {
            segment: "x.limits".to_owned(),
            position: 0,
        })
    );
}

// Why: conflict detection and view reset compare paths by containment, which must work on
// whole segments; comparing strings would make `x.limits` contain an unrelated
// `x.limits_min`.
#[test]
fn containment_and_overlap_compare_whole_segments() {
    let limits = path("x.limits");
    assert!(limits.contains(&path("x.limits")));
    assert!(limits.contains(&path("x.limits.min")));
    assert!(!limits.contains(&path("x")));
    assert!(!limits.contains(&path("x.limits_min")));
    assert!(!limits.contains(&path("y.limits")));

    // Overlap is symmetric: an owner's set of `x` conflicts with a user's `x.limits`, and
    // an owner's set of `x.limits` with a user's `x`.
    assert!(limits.overlaps(&path("x")));
    assert!(path("x").overlaps(&limits));
    assert!(limits.overlaps(&path("x.limits")));
    assert!(limits.overlaps(&path("x.limits.max")));
    assert!(!limits.overlaps(&path("x.scale")));
    assert!(!limits.overlaps(&path("x.limits_min")));
}

// ---------------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------------

// Why: this is the test that keeps the registry honest. For every node of figures that
// together set every variant, every listed property must be readable (unless it depends on
// a variant or optional value that is not set), must hold a value of the listed type, and
// must accept its own value back without any change; and every listed property must be
// readable somewhere, so the registry lists nothing that `get` and `apply` cannot reach.
#[test]
fn every_registry_property_can_be_read_and_set_to_its_own_value_without_change() {
    let mut reached: BTreeMap<NodeKind, BTreeSet<PropertyPath>> = BTreeMap::new();
    for fig in representative_figures() {
        for (node, kind) in nodes(&fig) {
            assert_eq!(fig.node_kind(node), Some(kind), "{node}");
            for property in properties(kind) {
                let at = format!("{kind:?} {node} {}", property.path);
                match fig.get(node, &property.path) {
                    Ok(value) => {
                        match value.value_type() {
                            None => assert!(property.optional, "{at}: unset but not optional"),
                            Some(found) => assert_eq!(found, property.value_type, "{at}"),
                        }
                        let edit = Edit::Set {
                            node,
                            path: property.path.clone(),
                            value,
                        };
                        let (edited, inverse) = applied(&fig, &tx([edit])).unwrap_or_else(|e| {
                            panic!("{at}: setting its own value failed: {e:?}")
                        });
                        assert_eq!(
                            edited, fig,
                            "{at}: setting its own value changed the figure"
                        );
                        let (restored, _) = applied(&edited, &inverse).unwrap();
                        assert_eq!(restored, fig, "{at}: the inverse changed the figure");
                        reached.entry(kind).or_default().insert(property.path);
                    }
                    Err(EditError::InactiveVariant { .. } | EditError::AbsentValue { .. })
                        if property.conditional => {}
                    Err(error) => panic!("{at}: {error:?}"),
                }
            }
        }
    }
    for kind in NodeKind::ALL {
        let unreached: Vec<String> = properties(kind)
            .into_iter()
            .filter(|p| !reached.get(&kind).is_some_and(|r| r.contains(&p.path)))
            .map(|p| p.path.to_string())
            .collect();
        assert!(
            unreached.is_empty(),
            "{kind:?}: no representative figure reaches {unreached:?}"
        );
    }
}

/// Compiles the generated `.proto` files with `protox`.
fn descriptor_pool() -> DescriptorPool {
    struct Files(BTreeMap<String, String>);

    impl protox::file::FileResolver for Files {
        fn open_file(&self, name: &str) -> Result<protox::file::File, protox::Error> {
            match self.0.get(name) {
                Some(source) => protox::file::File::from_source(name, source),
                None => Err(protox::Error::file_not_found(name)),
            }
        }
    }

    let files: BTreeMap<String, String> = proto_files()
        .into_iter()
        .map(|(p, text)| (p.to_str().expect("UTF-8").replace('\\', "/"), text))
        .collect();
    let names: Vec<String> = files.keys().cloned().collect();
    let mut compiler = protox::Compiler::with_file_resolver(Files(files));
    compiler
        .open_files(&names)
        .expect("the generated files compile");
    compiler.descriptor_pool()
}

/// The flags of a settable path derived from the wire schema: whether it is optional, and
/// whether it lies below a oneof variant or an optional value.
type Flags = (bool, bool);

/// Collects the settable paths below a message of the wire schema, independently of the
/// registry: every field is a path, a singular message field (other than a list or a map)
/// is descended into, and a tagged value (a message whose only oneof is `kind`) is
/// flattened, so that the fields of each variant message are named directly below it.
fn collect_paths(
    message: &MessageDescriptor,
    prefix: &str,
    conditional: bool,
    skip: &[&str],
    out: &mut BTreeMap<String, Flags>,
) {
    let tagged = message
        .oneofs()
        .any(|o| !o.is_synthetic() && o.name() == "kind");
    let fields: Vec<(prost_reflect::FieldDescriptor, bool)> = if tagged {
        message
            .fields()
            .flat_map(|variant| match variant.kind() {
                Kind::Message(inner) => inner.fields().map(|f| (f, true)).collect::<Vec<_>>(),
                _ => panic!("{} has a variant that is not a message", message.name()),
            })
            .collect()
    } else {
        message.fields().map(|f| (f, false)).collect()
    };
    for (field, in_variant) in fields {
        if skip.contains(&field.name()) {
            continue;
        }
        let at = format!("{prefix}{}", field.name());
        let optional = OPTIONAL_IN_IR.contains(&field.full_name());
        let below = conditional || in_variant;
        out.insert(at.clone(), (optional, below));
        if let Kind::Message(inner) = field.kind()
            && !field.is_list()
            && !field.is_map()
        {
            collect_paths(&inner, &format!("{at}."), below || optional, &[], out);
        }
    }
}

// Why: the registry is what the property editor offers and what clients rely on, so it
// must list exactly the settable fields of the IR, named as the wire schema names them,
// with the IR's optional properties marked optional and the properties below a variant or
// an optional value marked conditional. The expectation is independent of the registry:
// the registry is generated from the Rust IR types, whereas the wire declarations in
// `wire/` are written separately (and checked against the encoder by `proto_schema.rs`),
// and the optional and read-only fields are listed here by hand. A field added to the IR
// and the wire schema without a registry entry, or a registry path that does not follow
// the wire naming, fails here. Because the paths are exactly the wire field names, a
// renamed or removed path is also reported by `buf breaking` against main, which is how
// ADR 0008 asks for such changes to be reported.
#[test]
fn the_registry_lists_exactly_the_settable_fields_of_the_wire_schema() {
    let pool = descriptor_pool();
    let read_only: &[&str] = &["id"];
    let cases: [(NodeKind, &str, &[&str]); 10] = [
        (
            NodeKind::Figure,
            "Figure",
            &["schema_version", "id", "data", "axes", "provenance"],
        ),
        (NodeKind::Axes, "Axes", &["id", "artists"]),
        (NodeKind::Line, "Line", read_only),
        (NodeKind::Scatter, "Scatter", read_only),
        (NodeKind::Contour, "Contour", read_only),
        (NodeKind::Quiver, "Quiver", read_only),
        (NodeKind::Surface, "Surface", read_only),
        (NodeKind::Image, "Image", read_only),
        (NodeKind::IndexedImage, "IndexedImage", read_only),
        (NodeKind::MappedImage, "MappedImage", read_only),
    ];
    for (kind, message, skip) in cases {
        let descriptor = pool
            .get_message_by_name(&format!("ironlab.ir.v0.{message}"))
            .expect("the message is generated");
        let mut expected = BTreeMap::new();
        collect_paths(&descriptor, "", false, skip, &mut expected);

        let registry = properties(kind);
        let listed: BTreeMap<String, Flags> = registry
            .iter()
            .map(|p| (p.path.to_string(), (p.optional, p.conditional)))
            .collect();
        assert_eq!(listed.len(), registry.len(), "{kind:?} lists a path twice");
        assert_eq!(listed, expected, "{kind:?}");
    }
}

// Why: the property editor shows the documentation of each property to users who have not
// read the Rust source.
#[test]
fn every_registry_property_is_documented() {
    for kind in NodeKind::ALL {
        for property in properties(kind) {
            assert!(
                !property.docs.trim().is_empty(),
                "{kind:?} {} is undocumented",
                property.path
            );
        }
    }
}

/// The leaves of the wire message of one node, keyed by their property paths.
type Leaves = BTreeMap<String, String>;

/// Renders a wire value so that equal values render equally; the entries of a map are
/// sorted, because reflection holds them in a hash map.
fn render(value: &WireValue) -> String {
    match value {
        WireValue::Map(entries) => {
            let mut rendered: Vec<String> = entries
                .iter()
                .map(|(key, item)| format!("{key:?}: {}", render(item)))
                .collect();
            rendered.sort();
            format!("{{{}}}", rendered.join(", "))
        }
        WireValue::List(items) => {
            let rendered: Vec<String> = items.iter().map(render).collect();
            format!("[{}]", rendered.join(", "))
        }
        other => format!("{other:?}"),
    }
}

/// Returns the variant field that is set in a tagged message (one whose only oneof is
/// `kind`), `Some(None)` when none is set, or `None` when the message is not tagged.
fn tagged_variant(message: &DynamicMessage) -> Option<Option<FieldDescriptor>> {
    let descriptor = message.descriptor();
    let tagged = descriptor
        .oneofs()
        .any(|o| !o.is_synthetic() && o.name() == "kind");
    tagged.then(|| descriptor.fields().find(|f| message.has_field(f)))
}

/// Collects the leaves of a wire message by the path convention of property paths, using
/// reflection over the generated schema only: a singular message field is descended into,
/// the variant of a tagged value is recorded at the value's path and its fields are named
/// directly below it, and a scalar, enum, list or map is a leaf.
fn flatten_wire(message: &DynamicMessage, prefix: &str, skip: &[&str], out: &mut Leaves) {
    for field in message.descriptor().fields() {
        if skip.contains(&field.name()) {
            continue;
        }
        let at = format!("{prefix}{}", field.name());
        if field.supports_presence() && !message.has_field(&field) {
            out.insert(at, "absent".to_owned());
            continue;
        }
        let value = message.get_field(&field);
        match &*value {
            WireValue::Message(inner) if !field.is_list() && !field.is_map() => {
                match tagged_variant(inner) {
                    None => flatten_wire(inner, &format!("{at}."), &[], out),
                    Some(None) => {
                        out.insert(at, "no variant".to_owned());
                    }
                    Some(Some(variant)) => {
                        out.insert(at.clone(), format!("variant {}", variant.name()));
                        if let WireValue::Message(fields) = &*inner.get_field(&variant) {
                            flatten_wire(fields, &format!("{at}."), &[], out);
                        }
                    }
                }
            }
            other => {
                out.insert(at, render(other));
            }
        }
    }
}

/// Returns the leaves of every node of a figure, in the order of [`nodes`], read from the
/// figure's Protocol Buffers encoding through the generated schema.
fn wire_leaves_by_node(fig: &Figure, pool: &DescriptorPool) -> Vec<Leaves> {
    let descriptor = pool
        .get_message_by_name("ironlab.ir.v0.Figure")
        .expect("the package declares Figure");
    let figure = DynamicMessage::decode(descriptor, fig.to_protobuf().as_slice())
        .expect("the generated schema decodes a figure");
    let mut all = Vec::new();
    let mut leaves = Leaves::new();
    flatten_wire(&figure, "", &["axes"], &mut leaves);
    all.push(leaves);
    let axes_field = figure.get_field_by_name("axes").expect("Figure.axes");
    let WireValue::List(axes_list) = &*axes_field else {
        panic!("Figure.axes is a list");
    };
    for axes in axes_list {
        let WireValue::Message(axes) = axes else {
            panic!("an axes is a message");
        };
        let mut leaves = Leaves::new();
        flatten_wire(axes, "", &["artists"], &mut leaves);
        all.push(leaves);
        let artists_field = axes.get_field_by_name("artists").expect("Axes.artists");
        let WireValue::List(artists) = &*artists_field else {
            panic!("Axes.artists is a list");
        };
        for artist in artists {
            let WireValue::Message(artist) = artist else {
                panic!("an artist is a message");
            };
            let Some(Some(variant)) = tagged_variant(artist) else {
                panic!("the kind of an artist is set");
            };
            let fields = artist.get_field(&variant);
            let WireValue::Message(fields) = &*fields else {
                panic!("an artist variant is a message");
            };
            let mut leaves = Leaves::new();
            flatten_wire(fields, "", &[], &mut leaves);
            all.push(leaves);
        }
    }
    all
}

// Why: clients in other languages address a property by the field names of the generated
// `.proto` files, so setting a path must change the wire field of that name, and nothing
// outside it on any node. Setting a property to its own value cannot detect a `get` and a
// `set` that agree on the wrong field (`pan_x` for `pan_y`, `y.limits` for `x.limits`);
// this test can, because it reads the result through the generated schema rather than
// through `get`.
#[test]
fn setting_a_path_changes_exactly_the_wire_field_of_that_name() {
    let pool = descriptor_pool();
    let (mut attempted, mut accepted) = (0, 0);
    for fig in representative_figures() {
        let before = wire_leaves_by_node(&fig, &pool);
        for (index, (node, kind)) in nodes(&fig).into_iter().enumerate() {
            for property in properties(kind) {
                let Ok(value) = fig.get(node, &property.path) else {
                    continue;
                };
                let changed = perturb(&value);
                if changed == value {
                    continue; // An absent optional value, or the only font set.
                }
                attempted += 1;
                let at = format!("{kind:?} {node} {}", property.path);
                let edit = Edit::Set {
                    node,
                    path: property.path.clone(),
                    value: changed,
                };
                let edited = match applied(&fig, &tx([edit])) {
                    Ok((edited, _)) => edited,
                    // Some perturbations (a swapped grid, a wider cell) make the figure invalid.
                    Err(EditError::Invalid(_)) => continue,
                    Err(error) => panic!("{at}: {error:?}"),
                };
                accepted += 1;
                let after = wire_leaves_by_node(&edited, &pool);
                assert_eq!(after.len(), before.len(), "{at}: the set changed the nodes");
                let named = property.path.to_string();
                let below = format!("{named}.");
                let mut changed_named_field = false;
                for (k, (old, new)) in before.iter().zip(&after).enumerate() {
                    let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
                    for key in keys
                        .into_iter()
                        .filter(|key| old.get(*key) != new.get(*key))
                    {
                        assert!(
                            k == index && (*key == named || key.starts_with(&below)),
                            "{at}: changed the wire field {key} of node {k}: {:?} to {:?}",
                            old.get(key),
                            new.get(key)
                        );
                        changed_named_field = true;
                    }
                }
                assert!(changed_named_field, "{at}: changed no wire field");
            }
        }
    }
    assert!(
        accepted * 2 >= attempted,
        "only {accepted} of {attempted} perturbed sets were applied"
    );
}

// ---------------------------------------------------------------------------------
// Reading and setting by path
// ---------------------------------------------------------------------------------

// Why: the viewer drags a single camera angle or a single bound, so setting through the
// active variant must change exactly that field and leave its siblings alone.
#[test]
fn setting_a_field_of_the_active_variant_changes_only_that_field() {
    let fig = kitchen_sink_figure();
    let two_d = fig.axes[0].id; // manual x limits [0.5, 20]
    let three_d = fig.axes[4].id;

    let (edited, _) = applied(&fig, &tx([set(two_d, "x.limits.min", Value::Double(1.0))])).unwrap();
    let mut expected = fig.clone();
    expected.axes[0].x.limits = Limits::Manual {
        min: 1.0,
        max: 20.0,
    };
    assert_eq!(edited, expected);
    assert_eq!(
        edited.get(two_d, &path("x.limits")).unwrap(),
        Value::Limits(Limits::Manual {
            min: 1.0,
            max: 20.0
        })
    );

    let (edited, _) = applied(
        &fig,
        &tx([set(
            three_d,
            "projection.view3d.azimuth_deg",
            Value::Double(90.0),
        )]),
    )
    .unwrap();
    let mut expected = fig.clone();
    let Projection::ThreeD { view3d } = &mut expected.axes[4].projection else {
        panic!("the fixture axes is 3D");
    };
    view3d.azimuth_deg = 90.0;
    assert_eq!(edited, expected);
}

// Why: segments are Protocol Buffers field names, which clients in other languages see;
// the Rust spelling of a field (`box_`) and the name of a wire variant message
// (`three_d`) are not part of a path.
#[test]
fn paths_use_wire_field_names_and_flatten_variants() {
    let fig = kitchen_sink_figure();
    let two_d = fig.axes[0].id;
    let three_d = fig.axes[4].id;
    assert_eq!(fig.get(two_d, &path("box")).unwrap(), Value::Bool(false));
    assert_eq!(
        fig.get(three_d, &path("projection.view3d.pan_x")).unwrap(),
        Value::Double(0.1)
    );
    for (node, at) in [
        (two_d, "box_"),
        (three_d, "projection.three_d.view3d"),
        (two_d, "x.limits.manual.min"),
    ] {
        assert!(
            matches!(fig.get(node, &path(at)), Err(EditError::UnknownPath { .. })),
            "{at}"
        );
    }
}

// Why: a path through the wrong variant is a stale or mistaken edit (a 3D camera change
// sent to an axes that has become 2D) and must be refused as such, distinctly from a
// misspelled field, so that the overlay can drop it and a client can explain it.
#[test]
fn a_path_into_a_variant_that_is_not_set_is_an_inactive_variant() {
    let fig = kitchen_sink_figure();
    let two_d = fig.axes[1].id; // automatic x limits
    let contour = fig.axes[2].artists[0].id(); // automatic levels
    let quiver = fig.axes[3].artists[0].id(); // automatic scale
    let image = fig.axes[6].artists[0].id(); // on the xy plane
    let mapped = fig.axes[7].artists[0].id(); // transparent below
    for (node, at, value) in [
        (two_d, "projection.view3d.zoom", Value::Double(2.0)),
        (two_d, "projection.view3d", Value::View3d(View3d::default())),
        (two_d, "x.limits.min", Value::Double(0.0)),
        (contour, "levels.values", Value::Doubles(vec![1.0])),
        (quiver, "scale.value", Value::Double(2.0)),
        (image, "placement.plane.y", Value::Double(1.0)),
        (image, "placement.plane.x", Value::Unset),
        (mapped, "below.color", Value::Color(Color::BLACK)),
        (mapped, "below.color.a", Value::Float(0.5)),
    ] {
        let expected_path = path(at);
        assert!(
            matches!(
                fig.get(node, &expected_path),
                Err(EditError::InactiveVariant { edit: None, node: n, ref path }) if n == node && *path == expected_path
            ),
            "get {at}"
        );
        let mut edited = fig.clone();
        let result = edited.apply(&tx([set(node, at, value)]));
        assert!(
            matches!(
                result,
                Err(EditError::InactiveVariant { edit: Some(0), node: n, ref path }) if n == node && *path == expected_path
            ),
            "set {at}: {result:?}"
        );
        assert_eq!(edited, fig);
    }
}

// Why: a field below an absent optional value (the content of a missing title) has no
// value to read or change; setting the whole optional value is the way to create it.
#[test]
fn a_path_through_an_absent_optional_value_is_an_absent_value() {
    let fig = kitchen_sink_figure();
    let untitled = fig.axes[9].id; // no title and no legend
    let unnamed = fig.axes[1].artists[0].id(); // no display name
    let image = fig.axes[6].artists[0].id(); // no pixel ranges
    for (node, at, value) in [
        (untitled, "title.content", Value::String("t".to_owned())),
        (untitled, "legend.boxed", Value::Bool(true)),
        (
            unnamed,
            "display_name.content",
            Value::String("n".to_owned()),
        ),
        (image, "placement.columns.first", Value::Double(0.0)),
        (image, "placement.rows.last", Value::Double(0.0)),
    ] {
        assert!(
            matches!(
                fig.get(node, &path(at)),
                Err(EditError::AbsentValue { edit: None, .. })
            ),
            "get {at}"
        );
        let result = applied(&fig, &tx([set(node, at, value)]));
        assert!(
            matches!(result, Err(EditError::AbsentValue { edit: Some(0), .. })),
            "set {at}: {result:?}"
        );
    }
}

// Why: nodes and data change only through structural and data edits; a set of an
// identifier, a node list or the data table must be refused with an error that says the
// property is read-only, so that a client does not conclude that the path is misspelled.
#[test]
fn identifiers_node_lists_data_version_and_provenance_are_read_only() {
    let fig = kitchen_sink_figure();
    let axes = fig.axes[0].id;
    let artist = fig.axes[0].artists[0].id();
    for (node, at) in [
        (fig.id, "id"),
        (fig.id, "schema_version"),
        (fig.id, "provenance"),
        (fig.id, "provenance.fonts"),
        (fig.id, "axes"),
        (fig.id, "data"),
        (axes, "id"),
        (axes, "artists"),
        (artist, "id"),
    ] {
        assert!(
            matches!(
                fig.get(node, &path(at)),
                Err(EditError::ReadOnly { edit: None, .. })
            ),
            "get {at}"
        );
        let mut edited = fig.clone();
        let result = edited.apply(&tx([set(node, at, Value::Bool(true))]));
        assert!(
            matches!(result, Err(EditError::ReadOnly { edit: Some(0), node: n, .. }) if n == node),
            "set {at}: {result:?}"
        );
        assert_eq!(edited, fig);
    }
}

// Why: a misspelled field, a field of another kind of node, or a path that continues
// below a number must be reported as an unknown path naming the node and the path.
#[test]
fn a_field_that_the_node_does_not_have_is_an_unknown_path() {
    let fig = kitchen_sink_figure();
    let axes = fig.axes[0].id; // manual x limits
    let line = fig.axes[0].artists[0].id();
    for (node, at) in [
        (fig.id, "colour"),
        (axes, "x.limts"),
        (axes, "x.limits.min.more"),
        (line, "levels"),
        (line, "line.line"),
    ] {
        let expected_path = path(at);
        assert!(
            matches!(
                fig.get(node, &expected_path),
                Err(EditError::UnknownPath { edit: None, .. })
            ),
            "get {at}"
        );
        let result = applied(&fig, &tx([set(node, at, Value::Bool(true))]));
        assert!(
            matches!(
                result,
                Err(EditError::UnknownPath { edit: Some(0), node: n, ref path }) if n == node && *path == expected_path
            ),
            "set {at}: {result:?}"
        );
    }
}

// Why: a misspelled path must be reported as unknown in every state of the node. Were the
// error to depend on which variant or optional value happens to be set, a client could not
// tell a typo from a stale edit, and the overlay would report a typo as an entry that no
// longer applies.
#[test]
fn a_misspelled_path_is_unknown_whatever_variant_or_optional_value_is_set() {
    let fig = kitchen_sink_figure();
    let two_d = fig.axes[1].id; // automatic x limits
    let untitled = fig.axes[9].id; // no title
    for (node, at) in [
        (two_d, "projection.view3d.azimth_deg"),
        (two_d, "x.limits.minimum"),
        (untitled, "title.contnt"),
    ] {
        let expected_path = path(at);
        assert!(
            matches!(
                fig.get(node, &expected_path),
                Err(EditError::UnknownPath { edit: None, .. })
            ),
            "get {at}"
        );
        let result = applied(&fig, &tx([set(node, at, Value::Double(1.0))]));
        assert!(
            matches!(
                &result,
                Err(EditError::UnknownPath { edit: Some(0), node: n, path }) if *n == node && *path == expected_path
            ),
            "set {at}: {result:?}"
        );
    }
}

// Why: parameter names are arbitrary Unicode text that may contain dots, and link groups
// and explicit levels are lists, so none of their entries can be addressed by path
// segments; a client changes them by setting the whole value, which must keep every name
// exactly as given.
#[test]
fn parameters_links_and_level_values_are_set_whole_and_not_addressed_by_segments() {
    let fig = kitchen_sink_figure();
    let explicit = fig.axes[2].artists[1].id(); // explicit levels
    for (node, at) in [
        (fig.id, "parameters.converged"),
        (fig.id, "parameters.k–ω SST"),
        (fig.id, "links.0"),
        (explicit, "levels.values.0"),
    ] {
        assert!(
            matches!(
                fig.get(node, &path(at)),
                Err(EditError::UnknownPath { edit: None, .. })
            ),
            "get {at}"
        );
    }
    let parameters = BTreeMap::from([
        ("k–ω SST".to_owned(), Parameter::String("ω".to_owned())),
        ("re.number".to_owned(), Parameter::Number(1.0e5)),
    ]);
    let (edited, _) = applied(
        &fig,
        &tx([set(
            fig.id,
            "parameters",
            Value::Parameters(parameters.clone()),
        )]),
    )
    .unwrap();
    assert_eq!(edited.parameters, parameters);
    assert_eq!(
        edited.get(fig.id, &path("parameters")).unwrap(),
        Value::Parameters(parameters)
    );
}

// Why: a set on a node that does not exist must be refused rather than ignored.
#[test]
fn reading_or_setting_a_property_of_an_unknown_node_fails() {
    let fig = kitchen_sink_figure();
    let missing = NodeId(u64::MAX);
    assert!(matches!(
        fig.get(missing, &path("title")),
        Err(EditError::UnknownNode { edit: None, node }) if node == missing
    ));
    assert!(matches!(
        applied(&fig, &tx([set(missing, "title", Value::Unset)])),
        Err(EditError::UnknownNode { edit: Some(0), node }) if node == missing
    ));
}

// Why: values are typed only when applied, so a value of the wrong type (including a
// number of the wrong width) must be refused, and the error must name the type the
// property expects so that a client can correct it.
#[test]
fn a_value_of_the_wrong_type_is_refused_with_the_expected_type() {
    let fig = kitchen_sink_figure();
    let axes = fig.axes[0].id;
    let line = fig.axes[0].artists[0].id();
    let image = fig.axes[6].artists[0].id();
    let mapped = fig.axes[7].artists[0].id();
    for (node, at, value, expected, found) in [
        (
            axes,
            "x.limits",
            Value::Double(1.0),
            ValueType::Limits,
            Some(ValueType::Double),
        ),
        // A policy is not a colour specification, and a plane is not a contour placement,
        // although each pair looks alike.
        (
            mapped,
            "non_finite",
            Value::ColorSpec(ColorSpec::None),
            ValueType::OutOfRange,
            Some(ValueType::ColorSpec),
        ),
        (
            image,
            "placement.plane",
            Value::ContourPlacement(ContourPlacement::default()),
            ValueType::ImagePlane,
            Some(ValueType::ContourPlacement),
        ),
        (
            image,
            "placement",
            Value::PixelRange(PixelRange {
                first: 0.0,
                last: 1.0,
            }),
            ValueType::ImagePlacement,
            Some(ValueType::PixelRange),
        ),
        (
            line,
            "visible",
            Value::UInt32(1),
            ValueType::Bool,
            Some(ValueType::UInt32),
        ),
        (
            fig.id,
            "layout.rows",
            Value::Double(2.0),
            ValueType::UInt32,
            Some(ValueType::Double),
        ),
        (
            fig.id,
            "background.r",
            Value::Double(0.5),
            ValueType::Float,
            Some(ValueType::Double),
        ),
        (axes, "x.limits", Value::Unset, ValueType::Limits, None),
        (line, "visible", Value::Unset, ValueType::Bool, None),
    ] {
        let mut edited = fig.clone();
        let result = edited.apply(&tx([set(node, at, value)]));
        assert!(
            matches!(
                result,
                Err(EditError::TypeMismatch { edit: Some(0), node: n, expected: e, found: f, .. })
                    if n == node && e == expected && f == found
            ),
            "{at}: {result:?}"
        );
        assert_eq!(edited, fig);
    }
}

// Why: `Unset` is how a user removes a title, a legend or the z data of a line, and how a
// client creates such a value is by setting the whole value; both must work, and must be
// read back as `Unset` and as the value respectively.
#[test]
fn unset_clears_an_optional_property_and_a_whole_value_creates_it() {
    let fig = kitchen_sink_figure();
    let titled = fig.axes[0].id;
    let untitled = fig.axes[9].id;
    let three_d_line = fig.axes[4].artists[0].id();

    let (edited, _) = applied(
        &fig,
        &tx([
            set(titled, "title", Value::Unset),
            set(titled, "legend", Value::Unset),
            set(three_d_line, "z", Value::Unset),
            set(untitled, "title", Value::Text(Text::new("new"))),
        ]),
    )
    .unwrap();
    assert_eq!(edited.get(titled, &path("title")).unwrap(), Value::Unset);
    assert_eq!(find_axes(&edited, titled).legend, None);
    let Artist::Line(line) = &edited.axes[4].artists[0] else {
        panic!("the fixture artist is a line");
    };
    assert_eq!(line.z, None);
    assert_eq!(
        edited.get(untitled, &path("title.content")).unwrap(),
        Value::String("new".to_owned())
    );
}

// Why: the placement of an image is edited a field at a time from the property editor
// (a wall chosen, a pixel centre dragged, a range given or taken away, a colour picked
// for the pixels a policy paints), and each part follows a different rule of the walk:
// the plane is a tagged value whose offset exists only under the plane that has it and
// may be absent, the ranges are optional values created and cleared whole, and the
// colour of a policy exists only under a fixed colour. Each must be pathed as the value
// it mirrors (a contour plane, an optional title, a fixed colour specification) or the
// editor's rows for images would be dead, and the inverse must undo the lot.
#[test]
fn an_image_is_edited_through_its_plane_its_ranges_and_the_colour_of_a_policy() {
    let fig = kitchen_sink_figure();
    let image = fig.axes[5].artists[2].id(); // on the xz wall, with columns and no rows
    let mapped = fig.axes[7].artists[0].id(); // on the floor, transparent below
    let (edited, inverse) = applied(
        &fig,
        &tx([
            set(
                image,
                "placement.plane",
                Value::ImagePlane(ImagePlane::Yz { x: None }),
            ),
            set(image, "placement.plane.x", Value::Double(-2.5)),
            set(
                image,
                "placement.rows",
                Value::PixelRange(PixelRange {
                    first: 3.0,
                    last: 1.0,
                }),
            ),
            set(image, "placement.rows.last", Value::Double(0.0)),
            set(image, "placement.columns", Value::Unset),
            set(
                mapped,
                "below",
                Value::OutOfRange(OutOfRange::Rgba {
                    color: Color::BLACK,
                }),
            ),
            set(mapped, "below.color.a", Value::Float(0.5)),
            set(mapped, "placement.plane.z", Value::Double(4.0)),
        ]),
    )
    .unwrap();
    assert_eq!(
        edited.get(image, &path("placement")).unwrap(),
        Value::ImagePlacement(ImagePlacement {
            plane: ImagePlane::Yz { x: Some(-2.5) },
            columns: None,
            rows: Some(PixelRange {
                first: 3.0,
                last: 0.0,
            }),
        })
    );
    assert_eq!(
        edited.get(image, &path("placement.plane.x")).unwrap(),
        Value::Double(-2.5)
    );
    assert_eq!(
        edited.get(image, &path("placement.columns")).unwrap(),
        Value::Unset
    );
    assert_eq!(
        edited.get(mapped, &path("below")).unwrap(),
        Value::OutOfRange(OutOfRange::Rgba {
            color: Color::rgba(0.0, 0.0, 0.0, 0.5),
        })
    );
    assert_eq!(
        edited.get(mapped, &path("placement.plane")).unwrap(),
        Value::ImagePlane(ImagePlane::Xy { z: Some(4.0) })
    );
    let (restored, _) = applied(&edited, &inverse).unwrap();
    assert_eq!(restored, fig);
}

// Why: a transaction can switch limits to manual and then adjust one bound, because each
// edit sees the figure as the edits before it left it; in the opposite order the bound is
// set while the limits are still automatic, which must fail and leave nothing applied.
#[test]
fn a_later_edit_sees_the_variant_set_by_an_earlier_edit() {
    let (fig, axes, _) = single_line_figure();
    let (edited, _) = applied(
        &fig,
        &tx([
            set(
                axes,
                "x.limits",
                Value::Limits(Limits::Manual { min: 0.0, max: 1.0 }),
            ),
            set(axes, "x.limits.max", Value::Double(5.0)),
        ]),
    )
    .unwrap();
    assert_eq!(
        find_axes(&edited, axes).x.limits,
        Limits::Manual { min: 0.0, max: 5.0 }
    );

    let mut reversed = fig.clone();
    let result = reversed.apply(&tx([
        set(axes, "x.limits.max", Value::Double(5.0)),
        set(
            axes,
            "x.limits",
            Value::Limits(Limits::Manual { min: 0.0, max: 1.0 }),
        ),
    ]));
    assert!(
        matches!(
            result,
            Err(EditError::InactiveVariant { edit: Some(0), .. })
        ),
        "{result:?}"
    );
    assert_eq!(reversed, fig);
}
