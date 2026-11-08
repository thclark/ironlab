//! The generated JSON Schema of the `.fig.json` format.
//!
//! The schema is a build artefact generated from the Rust types, so there is no
//! committed copy to compare against; these tests check that generation works and that
//! the generated files form a consistent set.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};

// Why: `generate-schema` writes these files for other tools, so generating them must
// not fail, must produce at least one file, and must produce files that are JSON.
#[test]
fn json_schema_files_are_generated_and_each_parses_as_json() {
    let files = ironlab_ir::json_schema_files();
    assert!(!files.is_empty(), "no JSON Schema files were generated");
    for (path, text) in &files {
        assert!(
            path.is_relative(),
            "{} must be relative to the output directory",
            path.display()
        );
        serde_json::from_str::<Value>(text)
            .unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()));
    }
}

/// Normalises a relative path lexically, resolving `.` and `..` without touching the
/// file system, and returns `None` when the path leaves the output directory.
fn normalise(path: &Path) -> Option<PathBuf> {
    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::Normal(part) => parts.push(part),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.iter().collect())
}

/// Collects the value of every `$ref` keyword in a schema document.
fn collect_refs<'a>(value: &'a Value, refs: &mut Vec<&'a str>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                match (key.as_str(), inner) {
                    ("$ref", Value::String(reference)) => refs.push(reference),
                    _ => collect_refs(inner, refs),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_refs(item, refs)),
        _ => {}
    }
}

// Why: the schema is split into one file per module whose definitions refer to each
// other, so a reference to a file that is not generated, or to a definition that the
// file does not hold, makes the schema unusable by a validator that loads the files from
// disk. Each `$ref` must be relative, must name the file that contains it or another
// generated file (resolved against the directory of the file that contains it), and its
// fragment, if any, must be a JSON pointer that resolves within that file. This is a
// structural check only: it does not follow `$id` or percent-decode fragments.
#[test]
fn every_reference_resolves_to_a_definition_in_a_generated_file() {
    let files: BTreeMap<PathBuf, Value> = ironlab_ir::json_schema_files()
        .into_iter()
        .map(|(path, text)| {
            let normal = normalise(&path)
                .unwrap_or_else(|| panic!("{} leaves the output directory", path.display()));
            let value = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()));
            (normal, value)
        })
        .collect();

    let mut problems = Vec::new();
    let mut cross_file = 0;
    for (path, document) in &files {
        let mut refs = Vec::new();
        collect_refs(document, &mut refs);
        for reference in refs {
            let (file_part, fragment) = reference.split_once('#').unwrap_or((reference, ""));
            if file_part.contains(':') {
                problems.push(format!("{}: {reference} is not relative", path.display()));
                continue;
            }
            let target_path = if file_part.is_empty() {
                Some(path.clone())
            } else {
                cross_file += 1;
                normalise(&path.parent().unwrap_or(Path::new("")).join(file_part))
            };
            let Some(target) = target_path.as_ref().and_then(|p| files.get(p)) else {
                problems.push(format!(
                    "{}: {reference} names a file that is not generated",
                    path.display()
                ));
                continue;
            };
            if !fragment.is_empty() && target.pointer(fragment).is_none() {
                problems.push(format!(
                    "{}: {reference} names a definition that the file does not hold",
                    path.display()
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        cross_file > 0,
        "no file refers to another, so the schema is not split by module"
    );
}

/// Collects every string that a schema allows under its `enum` and `const` keywords,
/// following local references (`#/$defs/...`) within `root`, so that the check does not
/// depend on whether the generator inlines an enumeration or refers to a definition, or
/// wraps it to allow `null`.
fn allowed_strings(
    root: &Value,
    schema: &Value,
    seen: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    match schema {
        Value::Object(map) => {
            for (key, inner) in map {
                match (key.as_str(), inner) {
                    ("enum", Value::Array(items)) => {
                        out.extend(items.iter().filter_map(Value::as_str).map(str::to_owned));
                    }
                    ("const", Value::String(text)) => {
                        out.insert(text.clone());
                    }
                    ("$ref", Value::String(reference)) => {
                        if seen.insert(reference.clone()) {
                            let pointer = reference
                                .strip_prefix('#')
                                .unwrap_or_else(|| panic!("{reference} is not a local reference"));
                            let target = root
                                .pointer(pointer)
                                .unwrap_or_else(|| panic!("{reference} does not resolve"));
                            allowed_strings(root, target, seen, out);
                        }
                    }
                    ("description" | "title" | "default" | "examples", _) => {}
                    (_, inner) => allowed_strings(root, inner, seen, out),
                }
            }
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| allowed_strings(root, item, seen, out)),
        _ => {}
    }
}

// Why: the array definition is what other tools validate figure files against. It must
// keep the name `NdArray`, which artists' data and the edit protocol refer to, whatever
// Rust type now produces it; its `element` property must admit exactly the element types
// that the reader accepts (`f64` and `u8`) and must not be required, because every file
// written before the element type existed omits it; its `values` must still admit `null`,
// which is how a missing float is written; and the Rust enum that holds the values must
// not surface as a definition of its own (`Values`), because the split into one file per
// module maps each definition to the module of the wire type of the same name and there
// is no such wire type.
#[test]
fn the_array_definition_keeps_its_name_and_declares_the_element_type() {
    let schema = ironlab_ir::json_schema();
    let array = schema
        .pointer("/$defs/NdArray")
        .expect("the schema defines NdArray");
    let element = array
        .pointer("/properties/element")
        .expect("NdArray has an element property");
    let mut allowed = BTreeSet::new();
    allowed_strings(&schema, element, &mut BTreeSet::new(), &mut allowed);
    assert_eq!(
        allowed,
        BTreeSet::from(["f64".to_owned(), "u8".to_owned()]),
        "the element property allows {allowed:?}"
    );
    let required: Vec<&str> = array
        .get("required")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    assert!(
        !required.contains(&"element"),
        "element is required, so files without it would not validate: {required:?}"
    );
    assert!(
        required.contains(&"shape") && required.contains(&"values"),
        "shape and values are required: {required:?}"
    );
    assert_eq!(
        array.pointer("/properties/values/items/type"),
        Some(&json!(["number", "null"])),
        "the values must stay number-or-null, so that a missing float is still written as null"
    );
    assert!(
        schema.pointer("/$defs/Values").is_none(),
        "the schema defines Values"
    );

    let files: BTreeMap<PathBuf, Value> = ironlab_ir::json_schema_files()
        .into_iter()
        .map(|(path, text)| (path, serde_json::from_str(&text).expect("JSON")))
        .collect();
    assert!(
        files
            .get(Path::new("data.schema.json"))
            .and_then(|document| document.pointer("/$defs/NdArray"))
            .is_some(),
        "data.schema.json does not define NdArray"
    );
    for (path, document) in &files {
        assert!(
            document.pointer("/$defs/Values").is_none(),
            "{} defines Values",
            path.display()
        );
    }
}

// Why: web clients that build transactions as JSON need a schema to validate them against,
// generated like that of figures: `edit.schema.json` must describe a transaction document
// at its root (inline, as `figure.schema.json` describes a figure, or by a reference to its
// own definition), hold the definitions of edits, values and nodes, and refer to the files
// of other modules for the IR types that values and nodes contain rather than copy them.
#[test]
fn the_json_schema_of_a_transaction_is_generated_in_the_edit_module() {
    let files: BTreeMap<PathBuf, Value> = ironlab_ir::json_schema_files()
        .into_iter()
        .map(|(path, text)| (path, serde_json::from_str(&text).expect("JSON")))
        .collect();
    let edit = files
        .get(Path::new("edit.schema.json"))
        .expect("edit.schema.json is generated");
    for name in ["Edit", "Value", "Node"] {
        assert!(
            edit.pointer(&format!("/$defs/{name}")).is_some(),
            "edit.schema.json does not define {name}"
        );
    }
    let root_is_transaction = edit.pointer("/properties/edits").is_some()
        || (edit.get("$ref") == Some(&Value::from("#/$defs/Transaction"))
            && edit
                .pointer("/$defs/Transaction/properties/edits")
                .is_some());
    assert!(
        root_is_transaction,
        "the root of edit.schema.json does not describe a transaction"
    );
    for (module, name) in [("axes", "Limits"), ("artist", "Artist")] {
        assert!(
            edit.pointer(&format!("/$defs/{name}")).is_none(),
            "edit.schema.json copies {name} instead of referring to {module}.schema.json"
        );
    }
    let mut refs = Vec::new();
    collect_refs(edit, &mut refs);
    assert!(
        refs.contains(&"axes.schema.json#/$defs/Limits"),
        "edit.schema.json does not refer to the definition of limits in axes.schema.json"
    );
}

// Why: the image kinds and colour scales are new to the schema, and the tools that
// validate `.fig.json` files find their definitions by name in the file of their module
// (the image kinds in the artist module, and the colour scales with their out-of-range
// policies in the colour scale module); a definition missing, or one placed in another
// module's file (the split by module panics on a type that has no wire message of its
// name), would leave such files unvalidatable or the generation broken. The tagged types
// must also admit exactly the variant names of the documented form, which is what a
// hand-written file writes, and the replaced image kinds must be gone.
#[test]
fn the_image_types_are_defined_in_the_artist_module_with_their_variant_names() {
    let schema = ironlab_ir::json_schema();
    let files: BTreeMap<PathBuf, Value> = ironlab_ir::json_schema_files()
        .into_iter()
        .map(|(path, text)| (path, serde_json::from_str(&text).expect("JSON")))
        .collect();
    for (file, names) in [
        (
            "artist.schema.json",
            &[
                "Image",
                "ScalarImage",
                "ImagePlacement",
                "PixelRange",
                "ImagePlane",
            ][..],
        ),
        (
            "color_scale.schema.json",
            &[
                "ColorScale",
                "ColorScaleKind",
                "ColorScaleOutOfRange",
                "ContinuousColorScale",
                "DiscreteColorScale",
                "IndexedColorScale",
                "CategoricalColorScale",
                "ColorCategory",
                "Colormap",
                "ColorScaleNorm",
                "ColorScaleLevels",
                "OutOfRange",
            ][..],
        ),
    ] {
        let module = files
            .get(Path::new(file))
            .unwrap_or_else(|| panic!("{file} is generated"));
        for name in names {
            assert!(
                schema.pointer(&format!("/$defs/{name}")).is_some(),
                "the schema does not define {name}"
            );
            assert!(
                module.pointer(&format!("/$defs/{name}")).is_some(),
                "{file} does not define {name}"
            );
        }
    }
    for gone in ["IndexedImage", "MappedImage"] {
        assert!(
            schema.pointer(&format!("/$defs/{gone}")).is_none(),
            "the schema still defines {gone}"
        );
    }

    for name in ["Binning", "Binning2d", "Bins", "BinRule", "HistNorm"] {
        assert!(
            schema.pointer(&format!("/$defs/{name}")).is_some(),
            "the schema does not define {name}"
        );
    }
    for gone in ["BarBase", "BarSlot", "BarGroup", "BarLayout"] {
        assert!(
            schema.pointer(&format!("/$defs/{gone}")).is_none(),
            "the schema still defines {gone}"
        );
    }

    let strings_under = |name: &str| {
        let definition = schema
            .pointer(&format!("/$defs/{name}"))
            .unwrap_or_else(|| panic!("the schema defines {name}"));
        let mut allowed = BTreeSet::new();
        allowed_strings(&schema, definition, &mut BTreeSet::new(), &mut allowed);
        allowed
    };
    for (name, variants) in [
        ("ImagePlane", &["xy", "xz", "yz"][..]),
        (
            "OutOfRange",
            &["strict", "transparent", "clamp", "rgba"][..],
        ),
        ("Artist", &["image", "scalar_image"][..]),
        (
            "ColorScaleKind",
            &["continuous", "discrete", "indexed", "categorical"][..],
        ),
        ("Colormap", &["named", "listed"][..]),
        ("ColorScaleLevels", &["auto", "explicit"][..]),
        ("ColorScaleNorm", &["linear", "log"][..]),
    ] {
        let allowed = strings_under(name);
        for variant in variants {
            assert!(
                allowed.contains(*variant),
                "{name} does not admit {variant:?}; it admits {allowed:?}"
            );
        }
    }
    let artists = strings_under("Artist");
    for gone in ["indexed_image", "mapped_image"] {
        assert!(!artists.contains(gone), "Artist still admits {gone:?}");
    }
}

// Why: tools that validate `.fig.json` files find the definitions of bars, stairs and cells
// by name in the file of the artist module, and the split into one file per module places
// each definition with the wire message of the same name, so a type defined under another
// name or in another module would leave such files unvalidatable or break the generation.
// The tagged types must admit exactly the variant names of the documented form, which is
// what a hand-written file writes: the kinds of artist, the orientations, the bases, widths
// and offsets of bars, the two forms of stairs data with the three step positions, the two
// kinds of cell edges, and the kinds of bins, the bin rules and the normalisations of a
// binning record. The removed types of bar groups must no longer be defined, or a tool
// would accept a file that the reader refuses.
#[test]
fn the_bar_stairs_and_cells_types_are_defined_in_the_artist_module_with_their_variant_names() {
    let schema = ironlab_ir::json_schema();
    let files: BTreeMap<PathBuf, Value> = ironlab_ir::json_schema_files()
        .into_iter()
        .map(|(path, text)| (path, serde_json::from_str(&text).expect("JSON")))
        .collect();
    let artist = files
        .get(Path::new("artist.schema.json"))
        .expect("artist.schema.json is generated");
    for name in [
        "Bar",
        "Stairs",
        "Cells",
        "Orientation",
        "PerBar",
        "BarWidth",
        "BarOffset",
        "StairsData",
        "StepAt",
        "CellEdges",
        "CellGap",
    ] {
        assert!(
            schema.pointer(&format!("/$defs/{name}")).is_some(),
            "the schema does not define {name}"
        );
        assert!(
            artist.pointer(&format!("/$defs/{name}")).is_some(),
            "artist.schema.json does not define {name}"
        );
    }

    let strings_under = |name: &str| {
        let definition = schema
            .pointer(&format!("/$defs/{name}"))
            .unwrap_or_else(|| panic!("the schema defines {name}"));
        let mut allowed = BTreeSet::new();
        allowed_strings(&schema, definition, &mut BTreeSet::new(), &mut allowed);
        allowed
    };
    for (name, variants) in [
        ("Artist", &["bar", "stairs", "cells"][..]),
        ("Orientation", &["vertical", "horizontal"][..]),
        ("PerBar", &["constant", "data"][..]),
        ("BarWidth", &["pitch", "constant", "data"][..]),
        ("BarOffset", &["widths", "absolute"][..]),
        ("Bins", &["count", "width", "edges", "rule"][..]),
        (
            "BinRule",
            &[
                "auto",
                "sturges",
                "scott",
                "freedman_diaconis",
                "sqrt",
                "integers",
            ][..],
        ),
        (
            "HistNorm",
            &[
                "count",
                "probability",
                "percentage",
                "count_density",
                "density",
                "cumulative_count",
                "cdf",
            ][..],
        ),
        ("StairsData", &["edges", "samples"][..]),
        ("StepAt", &["pre", "post", "mid"][..]),
        ("CellEdges", &["unit", "edges"][..]),
    ] {
        let allowed = strings_under(name);
        for variant in variants {
            assert!(
                allowed.contains(*variant),
                "{name} does not admit {variant:?}; it admits {allowed:?}"
            );
        }
    }
}

// Why: files written before an axis could be hidden, its tick labels rotated or its side
// chosen have none of these properties, so the schema must not require them, or every
// existing file would fail validation against the schema while IronLAB still reads it.
#[test]
fn the_axis_definition_does_not_require_its_visibility_tick_label_angle_or_side() {
    let schema = ironlab_ir::json_schema();
    let axis = schema
        .pointer("/$defs/Axis")
        .expect("the schema defines Axis");
    for property in ["visible", "tick_label_angle", "side"] {
        assert!(
            axis.pointer(&format!("/properties/{property}")).is_some(),
            "Axis has no {property} property"
        );
    }
    let required: Vec<&str> = axis
        .get("required")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    for property in ["visible", "tick_label_angle", "side"] {
        assert!(
            !required.contains(&property),
            "{property} is required, so files written before it would not validate: {required:?}"
        );
    }
}

// Why: a binning record is left out of the file of a bar chart, stairs or cells that were
// not made from samples, so the schema must not require it, or every plain bar chart would
// fail validation against the schema while IronLAB still reads it; and the removed slot, gap
// and group of bars must no longer be described, since a hand-written file that follows the
// schema would then be refused by the reader.
#[test]
fn the_binning_record_is_described_but_not_required_and_bar_groups_are_gone() {
    let schema = ironlab_ir::json_schema();
    for name in ["Bar", "Stairs", "Cells"] {
        let definition = schema
            .pointer(&format!("/$defs/{name}"))
            .unwrap_or_else(|| panic!("the schema defines {name}"));
        let required: Vec<&str> = definition
            .get("required")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        assert!(
            definition.pointer("/properties/binning").is_some(),
            "{name} has no binning property"
        );
        assert!(
            !required.contains(&"binning"),
            "{name} requires binning: {required:?}"
        );
    }
    let bar = schema
        .pointer("/$defs/Bar")
        .expect("the schema defines Bar");
    for property in ["width", "offset", "margin"] {
        assert!(
            bar.pointer(&format!("/properties/{property}")).is_some(),
            "Bar has no {property} property"
        );
    }
    for gone in ["slot", "gap", "group"] {
        assert!(
            bar.pointer(&format!("/properties/{gone}")).is_none(),
            "Bar still has a {gone} property"
        );
    }
}

// Why: an opacity of 1 is left out of a written file, so the schema must not require any
// opacity of bars, stairs or cells, or every file that keeps them opaque would fail
// validation against the schema while IronLAB still reads it; nor may it still describe the
// per-bar colours or the single opacity of cells, which colour data and the face and edge
// opacities replaced, since a hand-written file that follows the schema would then be
// refused by the reader.
#[test]
fn the_opacities_of_bars_stairs_and_cells_are_described_but_not_required() {
    let schema = ironlab_ir::json_schema();
    for (name, opacities, gone) in [
        ("Bar", &["face_opacity", "edge_opacity"][..], "colors"),
        ("Stairs", &["line_opacity", "fill_opacity"][..], "opacity"),
        ("Cells", &["face_opacity", "edge_opacity"][..], "opacity"),
    ] {
        let definition = schema
            .pointer(&format!("/$defs/{name}"))
            .unwrap_or_else(|| panic!("the schema defines {name}"));
        let required: Vec<&str> = definition
            .get("required")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for opacity in opacities {
            assert!(
                definition
                    .pointer(&format!("/properties/{opacity}"))
                    .is_some(),
                "{name} has no {opacity} property"
            );
            assert!(
                !required.contains(opacity),
                "{name} requires {opacity}: {required:?}"
            );
        }
        assert!(
            definition.pointer(&format!("/properties/{gone}")).is_none(),
            "{name} still has a {gone} property"
        );
    }
}

/// A schema found while walking a schema document, with the place where it was found.
struct Visit<'a> {
    /// The location of the schema, such as `Axis.properties.limits` or `Artist.oneOf[3]`,
    /// which starts with the name of the definition (or the title of the root document)
    /// that holds it.
    path: String,
    /// The schema itself.
    schema: &'a Value,
    /// How the schema is reached from its parent.
    role: Role<'a>,
}

/// How a schema is reached from the schema that contains it.
enum Role<'a> {
    /// The schema is a root document or a definition under `$defs`.
    Definition,
    /// The schema is the value of the property `name` of the object schema `parent`.
    Property { name: &'a str, parent: &'a Value },
    /// The schema is a branch of the `oneOf` or `anyOf` of its parent.
    Branch,
    /// The schema is reached in any other way, such as through `items` or `allOf`.
    Other,
}

/// Walks the root documents of a figure and of a transaction, calling `visit` once for
/// each root, each definition under `$defs` and each schema nested within them through
/// `properties`, `items`, `prefixItems`, `additionalProperties`, `oneOf`, `anyOf` or
/// `allOf`.
///
/// The two roots share most of their definitions, which schemars generates identically,
/// so a definition is walked only once, under its own name.
fn walk_schemas(mut visit: impl FnMut(&Visit)) {
    fn walk<'a>(visit: &mut impl FnMut(&Visit), visited: Visit<'a>) {
        visit(&visited);
        let Visit { path, schema, .. } = visited;
        let Some(map) = schema.as_object() else {
            return;
        };
        if let Some(Value::Object(properties)) = map.get("properties") {
            for (name, inner) in properties {
                walk(
                    visit,
                    Visit {
                        path: format!("{path}.properties.{name}"),
                        schema: inner,
                        role: Role::Property {
                            name,
                            parent: schema,
                        },
                    },
                );
            }
        }
        for key in ["items", "additionalProperties"] {
            if let Some(inner @ Value::Object(_)) = map.get(key) {
                walk(
                    visit,
                    Visit {
                        path: format!("{path}.{key}"),
                        schema: inner,
                        role: Role::Other,
                    },
                );
            }
        }
        for key in ["prefixItems", "oneOf", "anyOf", "allOf"] {
            if let Some(Value::Array(branches)) = map.get(key) {
                for (index, inner) in branches.iter().enumerate() {
                    let role = if key == "oneOf" || key == "anyOf" {
                        Role::Branch
                    } else {
                        Role::Other
                    };
                    walk(
                        visit,
                        Visit {
                            path: format!("{path}.{key}[{index}]"),
                            schema: inner,
                            role,
                        },
                    );
                }
            }
        }
    }

    let mut walked = BTreeSet::new();
    for root in [
        ironlab_ir::json_schema(),
        ironlab_ir::transaction_json_schema(),
    ] {
        let title = root
            .get("title")
            .and_then(Value::as_str)
            .expect("a root document has a title")
            .to_owned();
        let mut document = root.clone();
        let definitions = document
            .as_object_mut()
            .and_then(|map| map.remove("$defs"))
            .unwrap_or_else(|| json!({}));
        walk(
            &mut visit,
            Visit {
                path: title,
                schema: &document,
                role: Role::Definition,
            },
        );
        for (name, definition) in definitions.as_object().into_iter().flatten() {
            if walked.insert(name.clone()) {
                walk(
                    &mut visit,
                    Visit {
                        path: name.clone(),
                        schema: definition,
                        role: Role::Definition,
                    },
                );
            }
        }
    }
}

/// Collects the location of every schema for which `offends` returns true, and fails
/// with all of them, sorted and one per line, so that one run gives a complete worklist.
fn assert_no_offenders(rule: &str, mut offends: impl FnMut(&Visit) -> bool) {
    let mut offenders = BTreeSet::new();
    walk_schemas(|visited| {
        if offends(visited) {
            offenders.insert(visited.path.clone());
        }
    });
    assert!(
        offenders.is_empty(),
        "{} locations break the rule that {rule}:\n{}",
        offenders.len(),
        offenders.into_iter().collect::<Vec<_>>().join("\n")
    );
}

/// Returns whether a schema has a non-empty `description`.
fn has_description(schema: &Value) -> bool {
    schema
        .get("description")
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

/// Returns whether a schema is the discriminator of a tagged enum variant: a `const`
/// with at most a `type` beside it, such as `{"const": "bar", "type": "string"}`.
fn is_discriminator(schema: &Value) -> bool {
    schema.as_object().is_some_and(|map| {
        map.contains_key("const") && map.keys().all(|key| key == "const" || key == "type")
    })
}

/// Returns the names in the `required` list of an object schema.
fn required_of(schema: &Value) -> Vec<&str> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

// Why: the owner reviews the model by reading the generated schema, and other tools show
// a definition's description as its documentation, so a definition without one is a part
// of the model that nobody can review or understand from the schema alone. The roots of
// the figure and transaction documents are checked as well, since they are definitions
// in all but placement.
#[test]
fn every_definition_has_a_description() {
    assert_no_offenders("every definition has a description", |visited| {
        matches!(visited.role, Role::Definition) && !has_description(visited.schema)
    });
}

// Why: a key without a description leaves the reader of the schema to guess what it
// means, its units and its effect, which defeats reviewing the model from the schema. A
// property whose schema is a bare `$ref` is not exempt, because the referenced definition
// says what the type is, not what this key is for. Two kinds of property are exempt,
// because the description of the variant that contains them already says everything:
// the discriminator of a tagged enum variant, whose schema is only a `const` (such as
// `"type": {"const": "bar"}`) naming the variant; and the `value` key of an adjacently
// tagged variant, whose object holds only the discriminator and `value`, when that
// variant object itself has a description, since `value` is then exactly the content
// that the variant describes.
#[test]
fn every_property_has_a_description() {
    assert_no_offenders(
        "every property has a description, except a variant's discriminator and the value \
         of a described adjacently tagged variant",
        |visited| {
            let Role::Property { name, parent } = visited.role else {
                return false;
            };
            if has_description(visited.schema) || is_discriminator(visited.schema) {
                return false;
            }
            let adjacent_content = name == "value"
                && has_description(parent)
                && parent
                    .get("properties")
                    .and_then(Value::as_object)
                    .is_some_and(|properties| {
                        properties.len() == 2
                            && properties
                                .iter()
                                .any(|(key, schema)| key != "value" && is_discriminator(schema))
                    });
            !adjacent_content
        },
    );
}

// Why: each variant of an enum is a distinct choice in the model, such as a kind of
// artist or a placement of a legend, and the reader must know what each choice means.
// A branch of a `oneOf` or `anyOf` that is only a `$ref` is exempt, since the referenced
// definition is checked on its own, and so is the `{"type": "null"}` branch of an
// optional value, which only says that the value may be absent. An `enum` array of more
// than one value is refused outright, because schemars emits that form only for unit
// variants without doc comments, so it always means undescribed variants.
#[test]
fn every_enum_variant_is_described() {
    assert_no_offenders(
        "every non-null branch of a oneOf or anyOf that is not a bare $ref has a \
         description, and no enum array lists more than one value",
        |visited| {
            let schema = visited.schema;
            let undescribed_enum = schema
                .get("enum")
                .and_then(Value::as_array)
                .is_some_and(|values| values.len() > 1);
            let undescribed_branch = matches!(visited.role, Role::Branch)
                && !has_description(schema)
                && !schema
                    .as_object()
                    .is_some_and(|map| map.len() == 1 && map.contains_key("$ref"))
                && *schema != json!({ "type": "null" });
            undescribed_enum || undescribed_branch
        },
    );
}

// Why: a property that a file may leave out takes a value that the reader supplies, and
// a reviewer or a hand-writing user cannot know that value unless the schema states it;
// without it the meaning of a figure file depends on code that the schema does not show.
// A property is omittable exactly when its object does not list it in `required`. An
// optional value that is left out when absent reads back as absent, so when its schema
// admits `null` its stated default must be `null`, not some other value.
#[test]
fn every_omittable_property_states_its_default() {
    /// Returns whether a schema admits the JSON value `null`.
    fn admits_null(schema: &Value) -> bool {
        let by_type = match schema.get("type") {
            Some(Value::String(kind)) => kind == "null",
            Some(Value::Array(kinds)) => kinds.contains(&json!("null")),
            _ => false,
        };
        let by_branch = ["anyOf", "oneOf"].iter().any(|key| {
            schema
                .get(key)
                .and_then(Value::as_array)
                .is_some_and(|branches| branches.iter().any(admits_null))
        });
        by_type || by_branch
    }

    assert_no_offenders(
        "every property missing from its object's required list states a default, which \
         is null when the property admits null",
        |visited| {
            let Role::Property { name, parent } = visited.role else {
                return false;
            };
            if required_of(parent).contains(&name) {
                return false;
            }
            match visited.schema.get("default") {
                None => true,
                Some(default) => admits_null(visited.schema) && !default.is_null(),
            }
        },
    );
}

// Why: a misspelt key in a hand-written figure (such as `colour_scale` for `color_scale`)
// would otherwise validate against the schema and then be silently ignored, leaving the
// author puzzled about why the figure looks wrong. Every object schema therefore states
// `"additionalProperties": false`, or `"unevaluatedProperties": false` for an object
// composed with `allOf`, where `additionalProperties` would reject the keys of the other
// parts. This constrains the schema only: the reader deliberately still ignores unknown
// keys so that a file from a later patch release loads (see "Versioning" in
// `docs/reference/figure-schema.md`), which is why the implementation must use
// `#[schemars(deny_unknown_fields)]` and never serde's attribute of the same name; the
// next test pins that behaviour of the reader.
#[test]
fn every_object_schema_rejects_unknown_keys() {
    assert_no_offenders(
        "every schema with properties sets additionalProperties (or, when composed with \
         allOf, unevaluatedProperties) to false",
        |visited| {
            let schema = visited.schema;
            if schema.get("properties").is_none() {
                return false;
            }
            let key = if schema.get("allOf").is_some() {
                "unevaluatedProperties"
            } else {
                "additionalProperties"
            };
            schema.get(key) != Some(&Value::Bool(false))
        },
    );
}

// Why: the schema rejects unknown keys so that a validator catches misspellings, but
// the reader must not, because a file written by a later patch release of the same minor
// schema version may carry properties that this build does not know, and the versioning
// policy promises that such a file loads. Making the schema strict must therefore never
// be done with serde's `deny_unknown_fields`; this test fails if it is, at the root of a
// figure or inside an axes and an axis.
#[test]
fn the_reader_ignores_unknown_keys_that_the_schema_rejects() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/decay.fig.json")).expect("the fixture is JSON");
    let expected =
        ironlab_ir::Figure::from_json(&fixture.to_string()).expect("the unmodified fixture loads");
    for pointer in ["", "/axes/0", "/axes/0/x"] {
        let mut document = fixture.clone();
        document
            .pointer_mut(pointer)
            .and_then(Value::as_object_mut)
            .unwrap_or_else(|| panic!("the fixture has an object at {pointer:?}"))
            .insert(
                "a_key_from_a_later_patch".to_owned(),
                json!({ "novel": [1] }),
            );
        let figure = ironlab_ir::Figure::from_json(&document.to_string()).unwrap_or_else(|e| {
            panic!("an unknown key at {pointer:?} stopped the figure from loading: {e}")
        });
        assert_eq!(
            figure, expected,
            "an unknown key at {pointer:?} changed the loaded figure"
        );
    }
}
