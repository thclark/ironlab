//! The generated JSON Schema of the `.fig.json` format.
//!
//! The schema is a build artefact generated from the Rust types, so there is no
//! committed copy to compare against; these tests check that generation works and that
//! the generated files form a consistent set.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

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
