//! Tests for bold text set with `\textbf{…}` outside math.
//!
//! WHY: the header rows of a grouped categorical axis (the source files of the branch map of #74, laid out by
//! `ycategory_groups`) are told apart from the member rows beneath them by being set in bold, which a label writes as
//! `\textbf{…}`. IronLAB bundles STIX Two Text Bold, so the argument is shaped in that face; without support the label
//! would print its backslash and braces literally.

mod common;

use common::{all_text, runs};
use ironlab_text::{FontId, TextEngine};

// A header such as `\textbf{xpanel.f}` must come out as the file name alone, every character of it in the bold face,
// without a warning, so that the header reads as a bold name rather than as LaTeX source.
#[test]
fn textbf_sets_its_argument_in_the_bold_text_face() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\textbf{xpanel.f}", true, 10.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    assert!(
        runs(&layout).iter().all(|r| r.font == FontId::TextBold),
        "every run is bold: {:?}",
        runs(&layout)
    );
    assert_eq!(all_text(&layout), "xpanel.f");
}

// Bold text mixes with plain text along one baseline, in source order, as monospace does.
#[test]
fn bold_mixes_with_plain_text() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"in \textbf{xbl.f} only", true, 10.0);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    let texts: Vec<(FontId, &str)> = runs(&layout)
        .iter()
        .map(|r| (r.font, r.text.as_str()))
        .collect();
    assert_eq!(
        texts,
        vec![
            (FontId::TextRegular, "in "),
            (FontId::TextBold, "xbl.f"),
            (FontId::TextRegular, " only"),
        ]
    );
}

// With the interpreter off, nothing is interpreted, `\textbf` included.
#[test]
fn textbf_is_verbatim_without_the_interpreter() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\textbf{a}", false, 10.0);
    assert!(runs(&layout).iter().all(|r| r.font == FontId::TextRegular));
    assert_eq!(all_text(&layout), r"\textbf{a}");
}

// A `\textbf{` that is never closed must not swallow the rest of the label or fail: it is shown as written, with one
// warning that says why, exactly as an unclosed `\texttt{` is.
#[test]
fn unclosed_textbf_is_shown_literally_with_a_warning() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"file \textbf{xbl.f", true, 10.0);
    assert_eq!(layout.warnings.len(), 1, "{:?}", layout.warnings);
    assert!(runs(&layout).iter().all(|r| r.font == FontId::TextRegular));
    assert_eq!(all_text(&layout), r"file \textbf{xbl.f");
}

// Braces nested inside the argument are balanced, as LaTeX balances them, so an inner group neither ends the bold
// text early nor is printed.
#[test]
fn nested_braces_in_textbf_are_balanced() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\textbf{x{bl}.f} rest", true, 10.0);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    let bold: String = runs(&layout)
        .iter()
        .filter(|r| r.font == FontId::TextBold)
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(bold, "xbl.f", "the whole argument is bold, braces removed");
    assert_eq!(all_text(&layout), "xbl.f rest");
}

// The point of bold is that a header looks different from the members beneath it, so the bold face must really be
// used: every glyph is the one STIX Two Text Bold maps its character to, read here from the font file independently
// of the engine, so that the argument alone is shaped and nothing of the command is; and the word is wider than in
// the regular face, which shows that the bold metrics were used (STIX Two numbers the glyphs of its text faces alike,
// so the identifiers alone cannot tell the faces apart, and the run's font is checked by the first test).
#[test]
fn bold_text_is_set_with_the_glyphs_of_the_bold_face() {
    let engine = TextEngine::new();
    let bold = engine.layout(r"\textbf{xpanel.f}", true, 10.0);
    let face = common::reference_face(FontId::TextBold);
    let drawn: Vec<(&str, u16)> = common::glyphs(&bold)
        .into_iter()
        .map(|(run, glyph)| (common::glyph_text(run, glyph), glyph.id))
        .collect();
    let expected: Vec<(String, u16)> = "xpanel.f"
        .chars()
        .map(|ch| {
            let id = face
                .glyph_index(ch)
                .unwrap_or_else(|| panic!("STIX Two Text Bold has {ch:?}"));
            (ch.to_string(), id.0)
        })
        .collect();
    assert_eq!(
        drawn,
        expected
            .iter()
            .map(|(t, id)| (t.as_str(), *id))
            .collect::<Vec<_>>()
    );
    let regular = engine.layout("xpanel.f", true, 10.0);
    assert!(
        bold.width > regular.width,
        "bold {} against regular {}",
        bold.width,
        regular.width
    );
}
