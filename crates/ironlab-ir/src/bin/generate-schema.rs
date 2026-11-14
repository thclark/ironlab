//! Writes the JSON Schema of the `.fig.json` format to `target/ironlab-schema/`, in two
//! forms: one file per IR module (`<module>.schema.json`, of which `figure.schema.json`
//! describes a figure and `edit.schema.json` a transaction), and two self-contained
//! documents whose definitions hold every type they use: `ironlab-figure.schema.json`,
//! which describes a figure, and `ironlab-edit.schema.json`, which describes a
//! transaction of the edit protocol. No IR module is named `ironlab-figure` or
//! `ironlab-edit`, so neither single document clashes with a module file.
//!
//! It also writes `VERSION`, which holds [`ironlab_ir::SCHEMA_VERSION`] followed by a
//! newline, so that the tools which check the schema version read the constant as the
//! build compiled it rather than parsing the Rust source.
//!
//! The files are generated from the Rust types and are build artefacts: they are not
//! committed.

use std::path::{Path, PathBuf};

/// The name of the file that holds the whole figure schema as one document.
const FIGURE_DOCUMENT: &str = "ironlab-figure.schema.json";

/// The name of the file that holds the whole transaction schema as one document.
const EDIT_DOCUMENT: &str = "ironlab-edit.schema.json";

/// The name of the file that holds the schema version.
const VERSION_FILE: &str = "VERSION";

fn main() -> std::io::Result<()> {
    let dir = target_dir().join("ironlab-schema");
    // Files from an earlier generation (such as those of a removed module) must not
    // survive into this one.
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let files = ironlab_ir::json_schema_files();
    for name in [FIGURE_DOCUMENT, EDIT_DOCUMENT, VERSION_FILE] {
        assert!(
            files.iter().all(|(path, _)| path != Path::new(name)),
            "a module file is named {name}"
        );
    }
    let singles = [
        (FIGURE_DOCUMENT, ironlab_ir::json_schema()),
        (EDIT_DOCUMENT, ironlab_ir::transaction_json_schema()),
    ]
    .map(|(name, schema)| {
        let mut text = serde_json::to_string_pretty(&schema).expect("a JSON value serialises");
        text.push('\n');
        (PathBuf::from(name), text)
    });
    let version = (
        PathBuf::from(VERSION_FILE),
        format!("{}\n", ironlab_ir::SCHEMA_VERSION),
    );
    for (path, text) in files.into_iter().chain(singles).chain([version]) {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, text)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// Returns the Cargo target directory: `CARGO_TARGET_DIR` when it is set, and the
/// workspace's `target` directory otherwise.
fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"))
}
