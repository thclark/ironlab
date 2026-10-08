//! Tests of the provenance that a new figure records.

use ironlab_ir::Provenance;

/// Returns the exact version to which the workspace manifest pins `latex-rust`, the typesetter that `ironlab-text`
/// uses.
fn pinned_latex_rust_version() -> String {
    let manifest = include_str!("../../../Cargo.toml");
    let entry = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("latex-rust ="))
        .expect("the workspace manifest declares latex-rust");
    let version = entry
        .split("version = \"=")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| panic!("latex-rust is pinned to an exact version: {entry}"));
    version.to_owned()
}

// WHY: `ironlab-ir` does not depend on latex-rust, so the typesetter version that a new figure records is written by
// hand. A figure whose rendering changes between releases is diagnosed from that record, so it must name the version
// that actually typeset the figure; this fails when the pin moves and the record does not.
#[test]
fn default_provenance_records_the_pinned_typesetter() {
    let provenance = Provenance::default();
    assert_eq!(provenance.typesetter, "latex-rust");
    assert_eq!(provenance.typesetter_version, pinned_latex_rust_version());
}
