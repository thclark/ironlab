//! Writes the JSON Schema of the figure format to `schema/figure.schema.json`.

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("schema");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("figure.schema.json");
    let mut text = serde_json::to_string_pretty(&ironlab_ir::json_schema())
        .expect("a JSON value always serialises");
    text.push('\n');
    std::fs::write(&path, text)?;
    println!("wrote {}", path.display());
    Ok(())
}
