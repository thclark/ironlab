//! Plain text shaping tests.

mod common;

use common::{EPS, METRIC_TOL, advance_pt, all_text, glyph_text, runs, text_face_extents};
use ironlab_text::{FontId, TextEngine, TextItem};

// Axis labels are mostly plain text; each character must become a correctly ordered glyph in the body face, with real extents so the scene compiler can reserve space for it.
#[test]
fn plain_label_is_one_run_of_regular_glyphs() {
    let engine = TextEngine::new();
    let layout = engine.layout("Time (s)", false, 9.0);
    common::assert_well_formed(&layout);

    assert_eq!(layout.items.len(), 1, "{layout:#?}");
    let TextItem::Glyphs(run) = &layout.items[0] else {
        panic!("expected a glyph run, got {layout:#?}");
    };
    assert_eq!(run.font, FontId::TextRegular);
    assert!((run.size_pt - 9.0).abs() < EPS);
    assert_eq!(run.text, "Time (s)");
    assert_eq!(run.glyphs.len(), "Time (s)".chars().count());
    assert!(run.glyphs[0].x.abs() < EPS, "text starts at the origin");
    assert!(
        run.glyphs.windows(2).all(|w| w[1].x > w[0].x),
        "pen advances left to right"
    );
    assert!(
        run.glyphs.iter().all(|g| g.y.abs() < EPS),
        "on the baseline"
    );
    assert!(layout.warnings.is_empty());
}

// Tick labels are centred and right-aligned using the reported width, so the width must be the pen position after the last glyph, in points at the requested size (not font units, not ink width).
#[test]
fn plain_width_is_the_end_of_the_last_advance() {
    let engine = TextEngine::new();
    for source in ["Time (s)", "0.25", "W"] {
        let layout = engine.layout(source, false, 9.0);
        let run = runs(&layout)[0];
        let last = run.glyphs.last().expect("non-empty label has glyphs");
        let expected = last.x + advance_pt(run, last);
        assert!(
            (layout.width - expected).abs() < METRIC_TOL,
            "{source:?}: width {} vs pen end {expected}",
            layout.width
        );
    }
}

// Tick labels along an axis are aligned on their baselines and offset from the axis by their height or depth. If extents followed the ink, "10" and "-5" or "ace" and "Hgy" would sit at different distances from the axis and the labels would jitter; plain text therefore reports the face ascender and descender, independent of content.
#[test]
fn plain_extents_are_the_face_ascender_and_descender() {
    let engine = TextEngine::new();
    for size in [9.0, 14.5] {
        let (ascender, descender) = text_face_extents(size);
        assert!(ascender > 0.0 && descender > 0.0);
        for source in ["ace", "Hgy", "Time (s)", "0.5", "-"] {
            let layout = engine.layout(source, false, size);
            assert!(
                (layout.height - ascender).abs() < METRIC_TOL,
                "{source:?} at {size}: height {} vs ascender {ascender}",
                layout.height
            );
            assert!(
                (layout.depth - descender).abs() < METRIC_TOL,
                "{source:?} at {size}: depth {} vs descender {descender}",
                layout.depth
            );
        }
    }
}

// PDF export writes the run text as ActualText so labels can be copied and searched; the glyph ranges must map back onto the original characters in order.
#[test]
fn plain_text_ranges_reconstruct_the_source() {
    let engine = TextEngine::new();
    let layout = engine.layout("Time (s)", false, 9.0);
    let run = runs(&layout)[0];
    let rebuilt: String = run.glyphs.iter().map(|g| glyph_text(run, g)).collect();
    assert_eq!(rebuilt, "Time (s)");
}

// STIX Two Text has "fi", "ff" and "ffi" ligatures, which HarfRust applies by default; a ligature glyph must claim every character it replaces, or copying "Coefficient" from the PDF would yield "Coeffcient". The test does not require ligatures to be applied, only that the ranges tile the source without gaps or overlaps.
#[test]
fn text_ranges_tile_the_source_when_ligatures_form() {
    let engine = TextEngine::new();
    let source = "Coefficient of friction, affine flow";
    let layout = engine.layout(source, false, 9.0);
    common::assert_well_formed(&layout);
    let run = runs(&layout)[0];
    assert_eq!(run.text, source);
    let mut end = 0;
    for g in &run.glyphs {
        assert_eq!(g.text_range.start, end, "gap or overlap at {g:?}");
        assert!(g.text_range.end > g.text_range.start, "empty range {g:?}");
        end = g.text_range.end;
    }
    assert_eq!(end, source.len());
}

// Glyph ids must come from the regular face's own cmap, otherwise the PDF would show the wrong characters.
#[test]
fn plain_glyph_ids_match_the_regular_face_cmap() {
    let engine = TextEngine::new();
    let layout = engine.layout("Time (s)", false, 9.0);
    let face = common::reference_face(FontId::TextRegular);
    let run = runs(&layout)[0];
    for g in &run.glyphs {
        let ch = glyph_text(run, g)
            .chars()
            .next()
            .expect("one char per glyph");
        let expected = face.glyph_index(ch).expect("STIX covers ASCII").0;
        assert_eq!(g.id, expected, "glyph for {ch:?}");
    }
}

// Most labels contain no math; enabling the LaTeX interpreter must not change their rendering, warn, or pull them through latex-rust.
#[test]
fn plain_text_is_never_routed_through_math() {
    let engine = TextEngine::new();
    let with_math = engine.layout("Velocity", true, 9.0);
    let without_math = engine.layout("Velocity", false, 9.0);
    assert!(
        runs(&with_math)
            .iter()
            .all(|r| r.font == FontId::TextRegular)
    );
    assert!(with_math.warnings.is_empty());
    assert_eq!(*with_math, *without_math);
}

// Non-ASCII characters in labels (units such as µm or °C) must shape to real glyphs and keep valid UTF-8 byte ranges.
#[test]
fn non_ascii_text_keeps_valid_ranges() {
    let engine = TextEngine::new();
    let layout = engine.layout("Length (µm) at 20 °C", false, 9.0);
    common::assert_well_formed(&layout);
    assert_eq!(all_text(&layout), "Length (µm) at 20 °C");
    let face = common::reference_face(FontId::TextRegular);
    let run = runs(&layout)[0];
    for ch in ['µ', '°'] {
        let id = face.glyph_index(ch).expect("STIX has the character").0;
        let glyph = run
            .glyphs
            .iter()
            .find(|g| g.id == id)
            .unwrap_or_else(|| panic!("no glyph for {ch:?} in {run:#?}"));
        assert_eq!(glyph_text(run, glyph), ch.to_string());
    }
}

// An empty label (for example an unset title) must lay out to nothing rather than panicking or reserving space.
#[test]
fn empty_text_has_no_items_and_no_extent() {
    let engine = TextEngine::new();
    for parse_math in [false, true] {
        let layout = engine.layout("", parse_math, 9.0);
        assert!(layout.items.is_empty());
        assert_eq!(layout.width, 0.0);
        assert_eq!(layout.height, 0.0);
        assert_eq!(layout.depth, 0.0);
        assert!(layout.warnings.is_empty());
    }
}
