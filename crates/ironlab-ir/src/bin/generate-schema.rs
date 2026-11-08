//! Writes the JSON Schema of the `.fig.json` format to `target/ironlab-schema/`, in two
//! forms: one file per IR module (`<module>.schema.json`, of which `figure.schema.json`
//! describes a figure and `edit.schema.json` a transaction), and the whole figure schema
//! as one self-contained document, `ironlab.schema.json`, whose definitions hold every
//! type. No IR module is named `ironlab`, so the single document never clashes with a
//! module file.
//!
//! The files are generated from the Rust types and are build artefacts: they are not
//! committed.

use std::path::{Path, PathBuf};

/// The name of the file that holds the whole figure schema as one document.
const SINGLE_DOCUMENT: &str = "ironlab.schema.json";

fn main() -> std::io::Result<()> {
    let dir = target_dir().join("ironlab-schema");
    // Files from an earlier generation (such as those of a removed module) must not
    // survive into this one.
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let mut single =
        serde_json::to_string_pretty(&ironlab_ir::json_schema()).expect("a JSON value serialises");
    single.push('\n');
    let files = ironlab_ir::json_schema_files();
    assert!(
        files
            .iter()
            .all(|(path, _)| path != Path::new(SINGLE_DOCUMENT)),
        "a module file is named {SINGLE_DOCUMENT}"
    );
    for (path, text) in files
        .into_iter()
        .chain([(PathBuf::from(SINGLE_DOCUMENT), single)])
    {
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
