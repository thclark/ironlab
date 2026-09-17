//! Writes the Protocol Buffers definition of the `.fig` format, one `.proto` file per
//! IR module, together with a `buf.yaml`, to `target/ironlab-proto/`.
//!
//! The files are generated from the Rust wire types and are build artefacts: they are
//! not committed. Run `buf lint` in the output directory to check them.

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let dir = target_dir().join("ironlab-proto");
    // Files from an earlier generation (such as those of a removed module) must not
    // survive into this one.
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let files = ironlab_ir::proto_files().into_iter().chain([(
        PathBuf::from("buf.yaml"),
        ironlab_ir::wire::BUF_YAML.to_owned(),
    )]);
    for (path, text) in files {
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
