//! Glyph outline tests.

mod common;

use std::sync::Arc;

use ironlab_text::{FontId, TextEngine};
use kurbo::Shape;

/// Asserts that the outline of `ch` in `font` is em-normalised, flipped to y
/// down and positioned exactly as the font's own bounding box says.
fn assert_outline_matches_font_bbox(engine: &TextEngine, font: FontId, ch: char) -> kurbo::Rect {
    let face = common::reference_face(font);
    let gid = face.glyph_index(ch).expect("glyph exists");
    let upem = f64::from(face.units_per_em());
    let reference = face.glyph_bounding_box(gid).expect("glyph has ink");

    let path = engine
        .glyph_outline(font, gid.0)
        .expect("glyph has an outline");
    assert!(path.elements().len() > 1);
    let bbox = path.bounding_box();

    let tol = 2.0 / upem;
    assert!(
        (bbox.x0 - f64::from(reference.x_min) / upem).abs() < tol,
        "{ch:?}: {bbox:?}"
    );
    assert!(
        (bbox.x1 - f64::from(reference.x_max) / upem).abs() < tol,
        "{ch:?}: {bbox:?}"
    );
    assert!(
        (bbox.y0 + f64::from(reference.y_max) / upem).abs() < tol,
        "{ch:?}: {bbox:?}"
    );
    assert!(
        (bbox.y1 + f64::from(reference.y_min) / upem).abs() < tol,
        "{ch:?}: {bbox:?}"
    );
    bbox
}

// The viewer tessellates these outlines to draw text; they must be in em units with y pointing down so that glyphs sit on the baseline, match the font's own bounding box, and are not upside down.
#[test]
fn text_outline_is_em_normalised_and_flipped() {
    let engine = TextEngine::new();
    let bbox = assert_outline_matches_font_bbox(&engine, FontId::TextRegular, 'H');
    assert!(bbox.y1 <= 0.01, "H sits on the baseline (y down): {bbox:?}");
    assert!(bbox.y0 < -0.5 && bbox.y0 > -1.0, "cap height: {bbox:?}");
    // A descender is the check that distinguishes a flip from a mirror about the x-height.
    let bbox = assert_outline_matches_font_bbox(&engine, FontId::TextRegular, 'g');
    assert!(
        bbox.y1 > 0.1 && bbox.y0 < -0.3,
        "g descends below the baseline: {bbox:?}"
    );
}

// Math glyphs are drawn from the latex-rust face, whose outlines must follow the same convention; the minus sign is checked because it floats well above the baseline, so a missing flip or offset is unmistakable.
#[test]
fn math_outline_is_em_normalised_and_flipped() {
    let engine = TextEngine::new();
    for ch in ['\u{03B1}', '\u{2212}', '\u{221A}'] {
        assert_outline_matches_font_bbox(&engine, FontId::Math, ch);
    }
    let minus = assert_outline_matches_font_bbox(&engine, FontId::Math, '\u{2212}');
    assert!(minus.y1 < -0.1, "minus is above the baseline: {minus:?}");
}

// Invalid glyph ids (from corrupt or foreign data) and blank glyphs must produce nothing to draw rather than a panic or an empty tessellation job.
#[test]
fn missing_or_blank_outlines_are_none() {
    let engine = TextEngine::new();
    for font in [FontId::TextRegular, FontId::Math] {
        let face = common::reference_face(font);
        assert!(
            engine
                .glyph_outline(font, face.number_of_glyphs())
                .is_none()
        );
        assert!(engine.glyph_outline(font, u16::MAX).is_none());
        let space = face.glyph_index(' ').expect("space exists").0;
        assert!(engine.glyph_outline(font, space).is_none());
    }
}

// Each glyph is tessellated once and instanced; the outline cache must hand back the same shared path on repeated requests.
#[test]
fn outlines_are_cached() {
    let engine = TextEngine::new();
    let gid = common::reference_face(FontId::TextRegular)
        .glyph_index('H')
        .expect("H exists")
        .0;
    let a = engine
        .glyph_outline(FontId::TextRegular, gid)
        .expect("outline");
    let b = engine
        .glyph_outline(FontId::TextRegular, gid)
        .expect("outline");
    assert!(Arc::ptr_eq(&a, &b));
}
