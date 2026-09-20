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
