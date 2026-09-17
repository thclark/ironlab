//! Helpers shared by the integration tests.

#![allow(dead_code)]

use ironlab_text::{FontId, GlyphRun, PositionedGlyph, TextItem, TextLayout};

/// Tolerance for comparing lengths in points.
pub const EPS: f64 = 1e-6;

/// Returns every glyph run of a layout, in item order.
pub fn runs(layout: &TextLayout) -> Vec<&GlyphRun> {
    layout
        .items
        .iter()
        .filter_map(|item| match item {
            TextItem::Glyphs(run) => Some(run),
            TextItem::Rule { .. } => None,
        })
        .collect()
}

/// Returns every rule of a layout as `(x, y, width, height)`, in item order.
pub fn rules(layout: &TextLayout) -> Vec<(f64, f64, f64, f64)> {
    layout
        .items
        .iter()
        .filter_map(|item| match item {
            TextItem::Rule {
                x,
                y,
                width,
                height,
            } => Some((*x, *y, *width, *height)),
            TextItem::Glyphs(_) => None,
        })
        .collect()
}

/// Returns every glyph of a layout together with its run, in item order.
pub fn glyphs(layout: &TextLayout) -> Vec<(&GlyphRun, &PositionedGlyph)> {
    runs(layout)
        .into_iter()
        .flat_map(|run| run.glyphs.iter().map(move |g| (run, g)))
        .collect()
}

/// Returns the slice of the run's text that a glyph represents.
pub fn glyph_text<'a>(run: &'a GlyphRun, glyph: &PositionedGlyph) -> &'a str {
    &run.text[glyph.text_range.clone()]
}

/// Returns the unique glyph (and its run) representing `text`, panicking with
/// a description of the layout if there is not exactly one.
pub fn find_glyph<'a>(layout: &'a TextLayout, text: &str) -> (&'a GlyphRun, &'a PositionedGlyph) {
    let found: Vec<_> = glyphs(layout)
        .into_iter()
        .filter(|(run, g)| glyph_text(run, g) == text)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one glyph for {text:?} in {layout:#?}"
    );
    found[0]
}

/// Concatenates the text of all runs, in item order.
pub fn all_text(layout: &TextLayout) -> String {
    runs(layout).iter().map(|run| run.text.as_str()).collect()
}

/// Parses the bundled face `font` with ttf-parser directly from its file (for
/// text faces) or from `latex_rust` (for the math face), independently of the
/// engine under test.
pub fn reference_face(font: FontId) -> ttf_parser::Face<'static> {
    let bytes: &'static [u8] = match font {
        FontId::TextRegular => include_bytes!("../../fonts/STIXTwoText-Regular.otf"),
        FontId::TextItalic => include_bytes!("../../fonts/STIXTwoText-Italic.otf"),
        FontId::TextBold => include_bytes!("../../fonts/STIXTwoText-Bold.otf"),
        FontId::Math => latex_rust::STIX_TWO_MATH_OTF,
    };
    ttf_parser::Face::parse(bytes, 0).expect("bundled font parses")
}

/// Asserts that a layout's numeric fields are finite and its extents are
/// non-negative.
pub fn assert_well_formed(layout: &TextLayout) {
    assert!(
        layout.width.is_finite() && layout.width >= 0.0,
        "{layout:#?}"
    );
    assert!(
        layout.height.is_finite() && layout.height >= 0.0,
        "{layout:#?}"
    );
    assert!(
        layout.depth.is_finite() && layout.depth >= 0.0,
        "{layout:#?}"
    );
    for (run, g) in glyphs(layout) {
        assert!(run.size_pt.is_finite() && run.size_pt > 0.0, "{run:#?}");
        assert!(g.x.is_finite() && g.y.is_finite(), "{run:#?}");
        assert!(
            run.text.get(g.text_range.clone()).is_some(),
            "text_range {:?} is not a valid slice of {:?}",
            g.text_range,
            run.text
        );
    }
    for (x, y, w, h) in rules(layout) {
        assert!(x.is_finite() && y.is_finite(), "{layout:#?}");
        assert!(w.is_finite() && w >= 0.0 && h.is_finite() && h >= 0.0);
    }
}

/// Tolerance for comparing engine output against metrics read independently
/// from the font files, in points. It absorbs the rounding of latex-rust's
/// rational dimensions to floating point, not any design difference.
pub const METRIC_TOL: f64 = 1e-4;

/// Returns the Unicode Mathematical Italic counterpart of a Latin or lowercase
/// Greek letter, if it has one.
fn math_italic(ch: char) -> Option<char> {
    let code = u32::from(ch);
    let mapped = match ch {
        'h' => 0x210E,
        'a'..='z' => 0x1D44E + (code - u32::from('a')),
        'A'..='Z' => 0x1D434 + (code - u32::from('A')),
        '\u{03B1}'..='\u{03C9}' => 0x1D6FC + (code - 0x03B1),
        _ => return None,
    };
    char::from_u32(mapped)
}

/// Returns every glyph (and its run) whose text is `ch` or, for a letter, its
/// Mathematical Italic counterpart.
///
/// latex-rust 1.0.2 typesets math letters upright (`$x$` yields U+0078). TeX
/// convention sets them in italic, so the implementation may remap letters to
/// the Mathematical Italic block; tests that are not about that choice accept
/// either form.
pub fn find_math_glyphs(layout: &TextLayout, ch: char) -> Vec<(&GlyphRun, &PositionedGlyph)> {
    let plain = ch.to_string();
    let italic = math_italic(ch).map(|c| c.to_string());
    glyphs(layout)
        .into_iter()
        .filter(|(run, g)| {
            let text = glyph_text(run, g);
            text == plain || italic.as_deref() == Some(text)
        })
        .collect()
}

/// Returns the unique glyph representing the math character `ch` (see
/// [`find_math_glyphs`]), panicking with a description of the layout if there
/// is not exactly one.
pub fn find_math_glyph(layout: &TextLayout, ch: char) -> (&GlyphRun, &PositionedGlyph) {
    let found = find_math_glyphs(layout, ch);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one glyph for {ch:?} in {layout:#?}"
    );
    found[0]
}

/// Returns the advance of a positioned glyph in points, read from the font
/// file rather than from the engine.
pub fn advance_pt(run: &GlyphRun, glyph: &PositionedGlyph) -> f64 {
    let face = reference_face(run.font);
    let advance = face
        .glyph_hor_advance(ttf_parser::GlyphId(glyph.id))
        .expect("glyph id is in range");
    f64::from(advance) * run.size_pt / f64::from(face.units_per_em())
}

/// Returns the ink bounding box of a positioned glyph in layout coordinates
/// (points, y down), read from the font file rather than from the engine, or
/// `None` for a blank glyph.
pub fn ink_pt(run: &GlyphRun, glyph: &PositionedGlyph) -> Option<kurbo::Rect> {
    let face = reference_face(run.font);
    let bbox = face.glyph_bounding_box(ttf_parser::GlyphId(glyph.id))?;
    let scale = run.size_pt / f64::from(face.units_per_em());
    Some(kurbo::Rect::new(
        glyph.x + f64::from(bbox.x_min) * scale,
        glyph.y - f64::from(bbox.y_max) * scale,
        glyph.x + f64::from(bbox.x_max) * scale,
        glyph.y - f64::from(bbox.y_min) * scale,
    ))
}

/// Returns the plain-text height and depth at `size_pt`: the regular face's
/// ascender and the magnitude of its descender, scaled from font units.
pub fn text_face_extents(size_pt: f64) -> (f64, f64) {
    let face = reference_face(FontId::TextRegular);
    let scale = size_pt / f64::from(face.units_per_em());
    (
        f64::from(face.ascender()) * scale,
        -f64::from(face.descender()) * scale,
    )
}

/// Asserts that the height and depth of a layout enclose the ink of every
/// glyph and every rule, so that nothing drawn is clipped or overlaps
/// neighbouring elements placed against the reported extents.
pub fn assert_extents_cover_ink(layout: &TextLayout) {
    let tol = METRIC_TOL;
    for (run, g) in glyphs(layout) {
        if let Some(ink) = ink_pt(run, g) {
            assert!(
                -ink.y0 <= layout.height + tol && ink.y1 <= layout.depth + tol,
                "ink {ink:?} of {:?} exceeds height {} / depth {}",
                glyph_text(run, g),
                layout.height,
                layout.depth
            );
        }
    }
    for (_, y, _, h) in rules(layout) {
        assert!(-y <= layout.height + tol && y + h <= layout.depth + tol);
    }
}
