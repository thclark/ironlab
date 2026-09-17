//! The bundled font set.
//!
//! The three STIX Two Text faces are embedded from `fonts/`, whose files are
//! checksummed in `fonts/SHA256SUMS`. The math face is not bundled separately:
//! it is the copy embedded by `latex-rust`, so that glyph identifiers produced
//! by math layout always refer to the bytes that renderers draw with.

use crate::FontId;

/// STIX Two Text Regular.
const TEXT_REGULAR_OTF: &[u8] = include_bytes!("../fonts/STIXTwoText-Regular.otf");
/// STIX Two Text Italic.
const TEXT_ITALIC_OTF: &[u8] = include_bytes!("../fonts/STIXTwoText-Italic.otf");
/// STIX Two Text Bold.
const TEXT_BOLD_OTF: &[u8] = include_bytes!("../fonts/STIXTwoText-Bold.otf");

/// Every face of the font set, in the order used to index [`Faces`].
pub(crate) const ALL_FONTS: [FontId; 4] = [
    FontId::TextRegular,
    FontId::TextItalic,
    FontId::TextBold,
    FontId::Math,
];

/// Returns the complete OpenType file of `font`.
pub(crate) fn bytes(font: FontId) -> &'static [u8] {
    match font {
        FontId::TextRegular => TEXT_REGULAR_OTF,
        FontId::TextItalic => TEXT_ITALIC_OTF,
        FontId::TextBold => TEXT_BOLD_OTF,
        FontId::Math => latex_rust::STIX_TWO_MATH_OTF,
    }
}

/// Returns the position of `font` in [`ALL_FONTS`].
pub(crate) fn index(font: FontId) -> usize {
    match font {
        FontId::TextRegular => 0,
        FontId::TextItalic => 1,
        FontId::TextBold => 2,
        FontId::Math => 3,
    }
}

/// Parsed `ttf-parser` faces for the whole font set, indexed by [`index`].
pub(crate) struct Faces([ttf_parser::Face<'static>; 4]);

impl Faces {
    /// Parses every bundled face.
    ///
    /// # Panics
    ///
    /// Panics if a bundled font cannot be parsed. The fonts are compiled into
    /// the binary and verified by tests, so this indicates a corrupt build
    /// rather than bad user input.
    pub(crate) fn parse() -> Self {
        Self(ALL_FONTS.map(|font| {
            ttf_parser::Face::parse(bytes(font), 0)
                .unwrap_or_else(|error| panic!("bundled font {font:?} does not parse: {error}"))
        }))
    }

    /// Returns the parsed face of `font`.
    pub(crate) fn get(&self, font: FontId) -> &ttf_parser::Face<'static> {
        &self.0[index(font)]
    }
}
