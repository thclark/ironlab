//! Tests for splitting mixed text into plain and math segments.

mod common;

use common::{EPS, all_text, runs};
use ironlab_text::{FontId, TextEngine};

// Mixed labels such as "Pressure $p$ (Pa)" must read in source order along one baseline, with no overlap between the plain and math parts.
#[test]
fn mixed_text_runs_are_ordered_left_to_right() {
    let engine = TextEngine::new();
    let layout = engine.layout("Pressure $p$ (Pa)", true, 9.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);

    let all = runs(&layout);
    let fonts: Vec<FontId> = all.iter().map(|r| r.font).collect();
    assert_eq!(
        fonts,
        [FontId::TextRegular, FontId::Math, FontId::TextRegular]
    );
    assert_eq!(all[0].text, "Pressure ");
    assert_eq!(all[1].glyphs.len(), 1);
    common::find_math_glyph(&layout, 'p');
    assert_eq!(all[2].text, " (Pa)");

    let plain_end = engine.layout("Pressure ", false, 9.0).width;
    let math_width = engine.layout("$p$", true, 9.0).width;
    let math_x = all[1].glyphs[0].x;
    assert!(math_x >= plain_end - EPS, "math starts after the plain run");
    assert!(all[2].glyphs[0].x >= math_x + math_width - EPS);
    assert!(all.iter().flat_map(|r| &r.glyphs).all(|g| g.y.abs() < EPS));
}

// The scene compiler measures labels as a whole, so the width of a mixed label must equal the sum of its segments placed side by side.
#[test]
fn mixed_text_width_is_the_sum_of_segments() {
    let engine = TextEngine::new();
    let whole = engine.layout("Pressure $p$ (Pa)", true, 9.0).width;
    let parts = engine.layout("Pressure ", false, 9.0).width
        + engine.layout("$p$", true, 9.0).width
        + engine.layout(" (Pa)", false, 9.0).width;
    assert!((whole - parts).abs() < EPS, "{whole} vs {parts}");
}

// Currency and other literal dollar signs must be expressible in LaTeX-interpreted labels via \$, without starting math or leaving the backslash visible.
#[test]
fn escaped_dollar_is_a_literal_dollar() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"Cost \$5", true, 9.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    assert!(runs(&layout).iter().all(|r| r.font == FontId::TextRegular));
    assert_eq!(all_text(&layout), "Cost $5");
    let glyph_count: usize = runs(&layout).iter().map(|r| r.glyphs.len()).sum();
    assert_eq!(glyph_count, "Cost $5".chars().count());
}

// With the interpreter off, labels are shown exactly as written, including dollars and backslashes, so non-LaTeX users are never surprised.
#[test]
fn parse_math_false_is_verbatim() {
    let engine = TextEngine::new();
    for source in ["$x$", r"Cost \$5"] {
        let layout = engine.layout(source, false, 9.0);
        assert!(layout.warnings.is_empty());
        assert!(runs(&layout).iter().all(|r| r.font == FontId::TextRegular));
        assert_eq!(all_text(&layout), source);
    }
}

// A single stray dollar (for example "Cost ($)") is a common authoring slip; it must render literally rather than swallowing the rest of the label, and warn so the author can escape it.
#[test]
fn unmatched_dollar_is_literal_with_warning() {
    let engine = TextEngine::new();
    let layout = engine.layout("Cost ($)", true, 9.0);
    common::assert_well_formed(&layout);
    assert!(runs(&layout).iter().all(|r| r.font == FontId::TextRegular));
    assert_eq!(all_text(&layout), "Cost ($)");
    assert_eq!(layout.warnings.len(), 1, "{:?}", layout.warnings);
}
