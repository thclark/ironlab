//! Writes the JSON Schema of the `.fig.json` format, one file per IR module, to
//! `target/ironlab-schema/`.
//!
//! The files are generated from the Rust types and are build artefacts: they are not
//! committed.

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let dir = target_dir().join("ironlab-schema");
    // Files from an earlier generation (such as those of a removed module) must not
    // survive into this one.
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    for (path, text) in ironlab_ir::json_schema_files() {
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
