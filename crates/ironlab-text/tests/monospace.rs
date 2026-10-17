//! Tests for monospace text set with `\texttt{…}` outside math.
//!
//! WHY: code identifiers such as case names (`naca4412_n160_polar30_re1e6_iter100`) are read character by character
//! and compared by eye down a column, which a proportional face makes harder; IronLAB bundles no monospace font, so
//! `\texttt` sets them in the Mathematical Monospace alphabet of the STIX Two Math face it already carries.

mod common;

use common::{EPS, all_text, glyphs, runs};
use ironlab_text::{FontId, TextEngine};

const CASE_NAME: &str = "naca4412_n160_polar30_re1e6_iter100";

/// Returns the glyph identifier of `ch` in the math face.
fn math_glyph(ch: char) -> u16 {
    common::reference_face(FontId::Math)
        .glyph_index(ch)
        .unwrap_or_else(|| panic!("STIX Two Math has {ch:?}"))
        .0
}

/// Returns the advance, in points at `size_pt`, of one cell of monospace text: the advance of the monospace `a`.
fn cell_pt(size_pt: f64) -> f64 {
    let face = common::reference_face(FontId::Math);
    let glyph = face
        .glyph_index('\u{1D68A}')
        .expect("STIX Two Math has the monospace a");
    let advance = face
        .glyph_hor_advance(glyph)
        .expect("the glyph has an advance");
    f64::from(advance) * size_pt / f64::from(face.units_per_em())
}

// A case name with underscores and digits must come out as written: every character in the monospace alphabet of the
// math face where it has one, an underscore as an underscore (not a subscript), and the copyable text the name itself.
#[test]
fn identifier_with_underscores_and_digits_is_set_in_monospace() {
    let engine = TextEngine::new();
    let layout = engine.layout(&format!(r"\texttt{{{CASE_NAME}}}"), true, 10.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    assert!(runs(&layout).iter().all(|r| r.font == FontId::Math));
    assert_eq!(all_text(&layout), CASE_NAME);

    let set = glyphs(&layout);
    assert_eq!(set.len(), CASE_NAME.chars().count());
    for ((_, glyph), ch) in set.iter().zip(CASE_NAME.chars()) {
        let expected = match ch {
            'a'..='z' => char::from_u32(0x1D68A + (ch as u32 - 'a' as u32)).unwrap(),
            '0'..='9' => char::from_u32(0x1D7F6 + (ch as u32 - '0' as u32)).unwrap(),
            other => other,
        };
        assert_eq!(glyph.id, math_glyph(expected), "the glyph of {ch:?}");
        assert!(glyph.y.abs() < EPS, "{ch:?} sits on the baseline");
    }
}

// Monospace means every character takes the same advance, so that names of equal length line up in a column whatever
// their letters; narrow and wide letters, digits and underscores alike.
#[test]
fn every_character_takes_one_cell() {
    let engine = TextEngine::new();
    let cell = cell_pt(10.0);
    for text in ["iiii", "WWWW", "1_._", "a b-"] {
        let layout = engine.layout(&format!(r"\texttt{{{text}}}"), true, 10.0);
        assert!(layout.warnings.is_empty(), "{text}: {:?}", layout.warnings);
        assert!(
            (layout.width - 4.0 * cell).abs() < EPS,
            "{text}: width {} is four cells of {cell}",
            layout.width
        );
    }
}

// LaTeX writes an underscore in text as `\_`; IronLAB also accepts a bare underscore inside `\texttt`, where it can mean
// nothing but itself, so that identifiers can be pasted unchanged. Both must give the same result.
#[test]
fn escaped_and_bare_underscores_are_the_same() {
    let engine = TextEngine::new();
    let escaped = engine.layout(r"\texttt{re1e6\_iter100}", true, 10.0);
    let bare = engine.layout(r"\texttt{re1e6_iter100}", true, 10.0);
    assert!(escaped.warnings.is_empty(), "{:?}", escaped.warnings);
    assert_eq!(escaped, bare);
}

// The other characters that LaTeX reserves are written with a backslash in `\texttt`, and must come out literally.
#[test]
fn escaped_reserved_characters_are_literal() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\texttt{\{a\}\$\&\%\#}", true, 10.0);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    assert_eq!(all_text(&layout), "{a}$&%#");
}

// A title such as "Polars of \texttt{case} at $M = 0.7$" mixes the three kinds of text along one baseline, in source
// order, and must measure as the sum of its parts so that the scene compiler can place it.
#[test]
fn monospace_mixes_with_plain_text_and_math() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"Polars of \texttt{a_1} at $M$", true, 10.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    let texts: Vec<(FontId, &str)> = runs(&layout)
        .iter()
        .map(|r| (r.font, r.text.as_str()))
        .collect();
    assert_eq!(texts[0], (FontId::TextRegular, "Polars of "));
    assert_eq!(texts[1], (FontId::Math, "a_1"));
    assert_eq!(texts[2], (FontId::TextRegular, " at "));
    assert_eq!(texts[3].0, FontId::Math);

    let parts = engine.layout("Polars of ", false, 10.0).width
        + engine.layout(r"\texttt{a_1}", true, 10.0).width
        + engine.layout(" at ", false, 10.0).width
        + engine.layout("$M$", true, 10.0).width;
    assert!(
        (layout.width - parts).abs() < EPS,
        "{} vs {parts}",
        layout.width
    );
}

// Monospace text shares the vertical extents of plain text, so that a label that switches to monospace keeps its
// height and the labels of a column align.
#[test]
fn monospace_has_the_extents_of_plain_text() {
    let engine = TextEngine::new();
    let mono = engine.layout(r"\texttt{gyp}", true, 10.0);
    let plain = engine.layout("x", true, 10.0);
    assert!((mono.height - plain.height).abs() < EPS);
    assert!((mono.depth - plain.depth).abs() < EPS);
}

// With the interpreter off, nothing is interpreted, `\texttt` included.
#[test]
fn texttt_is_verbatim_without_the_interpreter() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\texttt{a_b}", false, 10.0);
    assert!(runs(&layout).iter().all(|r| r.font == FontId::TextRegular));
    assert_eq!(all_text(&layout), r"\texttt{a_b}");
}

// A `\texttt{` that is never closed must not swallow the rest of the label or fail: it is shown as written, with a
// warning that says why.
#[test]
fn unclosed_texttt_is_shown_literally_with_a_warning() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"case \texttt{abc", true, 10.0);
    assert_eq!(layout.warnings.len(), 1, "{:?}", layout.warnings);
    assert!(runs(&layout).iter().all(|r| r.font == FontId::TextRegular));
    assert_eq!(all_text(&layout), r"case \texttt{abc");
}

// Braces nested inside the argument are balanced, as LaTeX balances them, so a group does not end the argument early.
#[test]
fn nested_braces_are_balanced() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\texttt{f{x}}y", true, 10.0);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    let texts: Vec<(FontId, &str)> = runs(&layout)
        .iter()
        .map(|r| (r.font, r.text.as_str()))
        .collect();
    assert_eq!(
        texts,
        [(FontId::Math, "fx"), (FontId::TextRegular, "y")],
        "the inner braces group, as in LaTeX, and are not printed"
    );
}

// Inside math, `\texttt` is latex-rust's: it sets letters and digits in the same monospace alphabet, so a name reads the
// same in a title and in an equation.
#[test]
fn texttt_in_math_uses_the_same_monospace_letters() {
    let engine = TextEngine::new();
    let math = engine.layout(r"$\texttt{re1}$", true, 10.0);
    let text = engine.layout(r"\texttt{re1}", true, 10.0);
    assert!(math.warnings.is_empty(), "{:?}", math.warnings);
    let ids = |layout: &ironlab_text::TextLayout| -> Vec<u16> {
        glyphs(layout).iter().map(|(_, g)| g.id).collect()
    };
    assert_eq!(ids(&math), ids(&text));
}

/// Returns the advance, in points at `size_pt`, of the glyph `id` of the math face.
fn advance_pt(id: u16, size_pt: f64) -> f64 {
    let face = common::reference_face(FontId::Math);
    let advance = face
        .glyph_hor_advance(ttf_parser::GlyphId(id))
        .expect("the glyph has an advance");
    f64::from(advance) * size_pt / f64::from(face.units_per_em())
}

// The cells of monospace text are only useful if every glyph sits in its own: a narrow underscore or full stop is
// centred in its cell rather than pulled left, so that a column of names reads as a grid.
#[test]
fn every_glyph_is_centred_in_its_own_cell() {
    let engine = TextEngine::new();
    let cell = cell_pt(10.0);
    let layout = engine.layout(r"\texttt{i_W.1}", true, 10.0);
    let set = glyphs(&layout);
    assert_eq!(set.len(), 5);
    for (i, (_, glyph)) in set.iter().enumerate() {
        let centre = glyph.x + advance_pt(glyph.id, 10.0) / 2.0;
        assert!(
            (centre - (i as f64 + 0.5) * cell).abs() < EPS,
            "glyph {i} is centred at {centre}, not in cell {i}"
        );
    }
}

// Copying a name from a PDF must give the name as written, character by character, the escaped characters included;
// each glyph claims exactly its own character of the run's text.
#[test]
fn each_glyph_claims_its_own_character_of_the_copyable_text() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\texttt{\{a\}\$\&\%\#\ b}", true, 10.0);
    let run = runs(&layout)[0];
    let claimed: String = run
        .glyphs
        .iter()
        .map(|g| &run.text[g.text_range.clone()])
        .collect();
    assert_eq!(claimed, "{a}$&%# b");
}

// An empty argument sets nothing and takes no room, so plain text after it starts where it would have without it.
#[test]
fn an_empty_argument_takes_no_room() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\texttt{}x", true, 10.0);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    let plain = engine.layout("x", true, 10.0);
    assert!((layout.width - plain.width).abs() < EPS);
    assert!(glyphs(&layout)[0].1.x.abs() < EPS);
}

// Inside `\texttt` a dollar sign is a character of the name, not the start of math, so the argument is set whole and
// its text is what was written.
#[test]
fn a_dollar_sign_inside_the_argument_is_literal() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\texttt{a$b$}", true, 10.0);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    assert_eq!(runs(&layout).len(), 1);
    assert_eq!(all_text(&layout), "a$b$");
}

// An escaped closing brace is a character, so it cannot close the argument; an argument closed only by escaped braces
// is unclosed and reported.
#[test]
fn an_escaped_closing_brace_does_not_close_the_argument() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\texttt{a\}", true, 10.0);
    assert_eq!(layout.warnings.len(), 1, "{:?}", layout.warnings);
}

// An unclosed `\texttt{` is kept as text, and what follows it is read as usual, math included, so one missing brace
// does not lose the rest of the label.
#[test]
fn text_after_an_unclosed_texttt_is_read_as_usual() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"case \texttt{abc $x$", true, 10.0);
    assert_eq!(layout.warnings.len(), 1, "{:?}", layout.warnings);
    let fonts: Vec<FontId> = runs(&layout).iter().map(|r| r.font).collect();
    assert_eq!(fonts, [FontId::TextRegular, FontId::Math]);
    assert_eq!(runs(&layout)[0].text, r"case \texttt{abc ");
}

// A backslash before any other character is not an escape IronLAB knows, so it is shown as written rather than
// silently dropped.
#[test]
fn an_unknown_escape_is_kept_verbatim() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"\texttt{a\nb}", true, 10.0);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    assert_eq!(all_text(&layout), r"a\nb");
}

// A character the math face cannot draw is reported, and its cell is left empty, so that the characters after it
// stay in their columns.
#[test]
fn a_character_without_a_glyph_is_reported_and_keeps_its_cell() {
    let engine = TextEngine::new();
    let cell = cell_pt(10.0);
    let layout = engine.layout("\\texttt{a\u{4E2D}b}", true, 10.0);
    assert_eq!(layout.warnings.len(), 1, "{:?}", layout.warnings);
    assert!((layout.width - 3.0 * cell).abs() < EPS);
    let set = glyphs(&layout);
    assert_eq!(set.len(), 2);
    let last = set[1].1;
    let centre = last.x + advance_pt(last.id, 10.0) / 2.0;
    assert!(
        (centre - 2.5 * cell).abs() < EPS,
        "b stays in the third cell"
    );
}
