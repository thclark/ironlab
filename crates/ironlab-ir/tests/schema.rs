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
// what a hand-written file writes: the kinds of artist, the orientations, the bases, slots
// and layouts of bars, the two forms of stairs data with the three step positions, and the
// two kinds of cell edges.
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
        "BarBase",
        "BarSlot",
        "BarGroup",
        "BarLayout",
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
        ("BarBase", &["constant", "data"][..]),
        ("BarSlot", &["pitch", "width", "data"][..]),
        ("BarLayout", &["grouped", "stacked"][..]),
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

// Why: files written before an axis could be hidden or its tick labels rotated have
// neither property, so the schema must not require them, or every existing file would
// fail validation against the schema while IronLAB still reads it.
#[test]
fn the_axis_definition_does_not_require_its_visibility_or_tick_label_angle() {
    let schema = ironlab_ir::json_schema();
    let axis = schema
        .pointer("/$defs/Axis")
        .expect("the schema defines Axis");
    for property in ["visible", "tick_label_angle"] {
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
    for property in ["visible", "tick_label_angle"] {
        assert!(
            !required.contains(&property),
            "{property} is required, so files written before it would not validate: {required:?}"
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
