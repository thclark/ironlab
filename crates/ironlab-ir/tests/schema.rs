//! The committed JSON Schema of the figure format.

use std::path::PathBuf;

use serde_json::Value;

fn committed_schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schema/figure.schema.json")
}

// Why: `schema/figure.schema.json` is the reviewable contract of the file format, so any
// change to the Rust types must be accompanied by a regenerated, committed schema.
#[test]
fn committed_schema_matches_the_rust_types() {
    let path = committed_schema_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let committed: Value = serde_json::from_str(&text).expect("committed schema is JSON");
    assert!(
        committed == ironlab_ir::json_schema(),
        "schema/figure.schema.json is out of date; regenerate it with \
         `cargo run -p ironlab-ir --bin generate-schema` and review the diff"
    );
}
