//! Font provenance and embedding tests.

mod common;

use std::path::Path;

use ironlab_text::{FontId, TextEngine};
use sha2::{Digest, Sha256};

const ALL_FONTS: [FontId; 4] = [
    FontId::TextRegular,
    FontId::TextItalic,
    FontId::TextBold,
    FontId::Math,
];

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// Fonts are redistributed under the OFL, so the files in the repository must be exactly the audited upstream release recorded in SHA256SUMS.
#[test]
fn sha256sums_match_bundled_font_files() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts");
    let sums = std::fs::read_to_string(dir.join("SHA256SUMS")).expect("SHA256SUMS exists");
    let mut checked = 0;
    for line in sums.lines().filter(|l| !l.trim().is_empty()) {
        let (expected, name) = line.split_once("  ").expect("sha256sum line format");
        let bytes = std::fs::read(dir.join(name.trim())).expect("listed font file exists");
        assert_eq!(hex_sha256(&bytes), expected, "checksum mismatch for {name}");
        checked += 1;
    }
    assert_eq!(checked, 3, "SHA256SUMS must cover all three text faces");
    assert!(
        dir.join("OFL.txt").is_file(),
        "the OFL licence must ship with the fonts"
    );
}

// The binary must embed the checksummed files, not some other copy, or exported PDFs would carry unaudited fonts.
#[test]
fn embedded_text_faces_are_the_checksummed_files() {
    let engine = TextEngine::new();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts");
    for (font, name) in [
        (FontId::TextRegular, "STIXTwoText-Regular.otf"),
        (FontId::TextItalic, "STIXTwoText-Italic.otf"),
        (FontId::TextBold, "STIXTwoText-Bold.otf"),
    ] {
        let on_disk = std::fs::read(dir.join(name)).expect("font file exists");
        assert!(
            engine.font_bytes(font) == on_disk.as_slice(),
            "{font:?} bytes differ from {name}"
        );
    }
}

// Math glyph ids come from latex-rust's layout, so the math face drawn by the renderers must be byte-identical to the one latex-rust measured, or symbols would be wrong.
#[test]
fn math_face_is_latex_rust_embedded_face() {
    let engine = TextEngine::new();
    let bytes = engine.font_bytes(FontId::Math);
    assert!(bytes == latex_rust::STIX_TWO_MATH_OTF);
    assert_eq!(hex_sha256(bytes), latex_rust::STIX_TWO_MATH_SHA256);
}

// Renderers scale glyph outlines and advances by unitsPerEm, so the engine must report the value actually stored in each face.
#[test]
fn units_per_em_matches_each_face() {
    let engine = TextEngine::new();
    for font in ALL_FONTS {
        assert_eq!(
            engine.units_per_em(font),
            common::reference_face(font).units_per_em(),
            "{font:?}"
        );
    }
}
