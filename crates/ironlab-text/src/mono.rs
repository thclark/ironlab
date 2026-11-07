//! Monospace text, set in the Mathematical Monospace alphabet of the math face.
//!
//! IronLAB bundles no monospace font. STIX Two Math, which it already carries for
//! math, has the Unicode Mathematical Monospace letters and digits (U+1D670 to U+1D6A3
//! and U+1D7F6 to U+1D7FF), so `\texttt{…}` outside math sets letters and digits in
//! those, and every other character in the math face's own glyph for it. Each
//! character is centred in a cell as wide as a monospace letter, so that underscores,
//! punctuation and spaces keep the fixed pitch that makes monospace text line up.

use latex_rust::TextStyle;

use crate::fonts::Faces;
use crate::{FontId, GlyphRun, PositionedGlyph, SegmentLayout, TextItem, TextWarning};

/// The monospace small letter a, whose advance is the width of every cell.
const MONOSPACE_A: char = '\u{1D68A}';

/// Sets `text` in monospace at `size_pt` points.
///
/// The run is in the math face and its text is `text` itself, so that copying it from a
/// PDF gives the characters the author wrote rather than mathematical alphanumerics.
/// The extents are those of plain text (the ascender and descender of the text face),
/// so that a label keeps its height when part of it is set in monospace. A character
/// that the math face cannot draw is reported in the returned warnings and takes an
/// empty cell.
pub(crate) fn shape(faces: &Faces, text: &str, size_pt: f64) -> (SegmentLayout, Vec<TextWarning>) {
    let face = faces.get(FontId::Math);
    let regular = faces.get(FontId::TextRegular);
    let scale = size_pt / f64::from(face.units_per_em());
    let text_scale = size_pt / f64::from(regular.units_per_em());
    let height = (f64::from(regular.ascender()) * text_scale).max(0.0);
    let depth = (-f64::from(regular.descender()) * text_scale).max(0.0);
    let advance = |glyph| f64::from(face.glyph_hor_advance(glyph).unwrap_or(0)) * scale;
    let cell = face.glyph_index(MONOSPACE_A).map_or(0.6 * size_pt, advance);

    let mut glyphs = Vec::with_capacity(text.chars().count());
    let mut warnings = Vec::new();
    let mut pen = 0.0;
    for (at, ch) in text.char_indices() {
        let styled = latex_rust::styled_char(ch, TextStyle::Tt);
        match face.glyph_index(styled).or_else(|| face.glyph_index(ch)) {
            Some(glyph) => glyphs.push(PositionedGlyph {
                id: glyph.0,
                x: pen + (cell - advance(glyph)) / 2.0,
                y: 0.0,
                text_range: at..at + ch.len_utf8(),
            }),
            None => warnings.push(TextWarning {
                source: format!(r"\texttt{{{text}}}"),
                message: format!(
                    "The character {ch:?} has no monospace glyph, so it is left out and its place left empty."
                ),
            }),
        }
        pen += cell;
    }

    let layout = SegmentLayout {
        items: if glyphs.is_empty() {
            Vec::new()
        } else {
            vec![TextItem::Glyphs(GlyphRun {
                stroke_em: 0.0,
                font: FontId::Math,
                size_pt,
                text: text.to_owned(),
                glyphs,
            })]
        },
        width: pen,
        height,
        depth,
    };
    (layout, warnings)
}
