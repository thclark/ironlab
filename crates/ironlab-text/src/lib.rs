//! Font sets, text shaping and LaTeX math typesetting for IronLAB.
//!
//! [`TextEngine`] turns label source text into positioned glyphs and filled
//! rules measured in points. Plain text is shaped with HarfRust against the
//! bundled STIX Two Text faces. When math parsing is requested, segments
//! delimited by unescaped `$…$` are typeset with `latex-rust` against the STIX
//! Two Math face that `latex-rust` embeds, so that glyph identifiers produced by
//! layout always refer to the same font bytes that the renderers draw with.
//!
//! All output coordinates share one convention: the origin is the left end of
//! the baseline, x increases to the right and y increases downwards, so glyphs
//! raised above the baseline have negative y.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex, PoisonError};

use kurbo::BezPath;

mod fonts;
mod math;
mod outline;
mod plain;
mod segments;

use segments::Segment;

/// Identifies one face of the bundled font set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FontId {
    /// STIX Two Text Regular, used for plain text.
    TextRegular,
    /// STIX Two Text Italic.
    TextItalic,
    /// STIX Two Text Bold.
    TextBold,
    /// STIX Two Math, as embedded by `latex-rust` (`latex_rust::STIX_TWO_MATH_OTF`).
    Math,
}

/// Key of the layout memo: source text, math parsing flag and the bit pattern
/// of the requested size in points.
type LayoutKey = (String, bool, u64);

/// Key of the outline cache: face and glyph identifier.
type OutlineKey = (FontId, u16);

/// Shapes and typesets label text, memoising results.
///
/// A single engine is intended to be created once and shared (for example in an
/// [`Arc`]) between the scene compiler, the viewer and the PDF exporter. It is
/// `Send` and `Sync`; its caches are guarded by mutexes.
pub struct TextEngine {
    /// Parsed STIX Two Math face used by `latex-rust` layout.
    math_font: latex_rust::MathFont,
    /// OpenType MATH constants, used to snap derived glyph scales to the
    /// script and script-script scale factors.
    math_params: latex_rust::MathParams,
    /// HarfRust shaping cache for STIX Two Text Regular, the face used for
    /// all plain text.
    shaper_data: harfrust::ShaperData,
    /// Parsed faces of the whole font set, for metrics and outlines.
    faces: fonts::Faces,
    /// Memoised layouts.
    layouts: Mutex<HashMap<LayoutKey, Arc<TextLayout>>>,
    /// Memoised glyph outlines; `None` records a glyph with no outline.
    outlines: Mutex<HashMap<OutlineKey, Option<Arc<BezPath>>>>,
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    /// Creates an engine over the bundled font set.
    ///
    /// # Panics
    ///
    /// Panics if a bundled font cannot be parsed. The fonts are compiled into
    /// the binary and checked by tests, so this indicates a corrupt build
    /// rather than a problem with any input.
    #[must_use]
    pub fn new() -> Self {
        let math_font =
            latex_rust::MathFont::stix_two_math().expect("the embedded STIX Two Math face parses");
        let math_params = latex_rust::MathParams::from_font(&math_font)
            .expect("the embedded STIX Two Math face has MATH constants");
        let regular = harfrust::FontRef::new(fonts::bytes(FontId::TextRegular))
            .expect("the bundled STIX Two Text Regular face parses");
        Self {
            math_font,
            math_params,
            shaper_data: harfrust::ShaperData::new(&regular),
            faces: fonts::Faces::parse(),
            layouts: Mutex::new(HashMap::new()),
            outlines: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the complete OpenType file of `font`, as embedded in the binary.
    ///
    /// The bytes for [`FontId::Math`] are exactly `latex_rust::STIX_TWO_MATH_OTF`.
    #[must_use]
    pub fn font_bytes(&self, font: FontId) -> &'static [u8] {
        fonts::bytes(font)
    }

    /// Returns the `unitsPerEm` value from the `head` table of `font`.
    #[must_use]
    pub fn units_per_em(&self, font: FontId) -> u16 {
        self.faces.get(font).units_per_em()
    }

    /// Lays out `content` at `size_pt` points.
    ///
    /// When `parse_math` is true, segments delimited by unescaped `$…$` are
    /// typeset as LaTeX math, `\$` produces a literal dollar sign, and an
    /// unmatched `$` is rendered literally with a warning. When `parse_math`
    /// is false, `content` is shaped verbatim, including any `$` and `\`
    /// characters.
    ///
    /// Consecutive segments are concatenated horizontally on a shared
    /// baseline. Math that `latex-rust` cannot parse or lay out is rendered as
    /// its raw source (including the delimiters) in the text face, and a
    /// [`TextWarning`] is recorded; this method never panics on user input.
    /// Because `latex-rust` parses and lays out recursively, math nested
    /// more deeply than a fixed limit is treated the same way rather than
    /// being passed to `latex-rust`, so that pathological input cannot
    /// overflow the calling thread's stack.
    ///
    /// Within math, a hyphen-minus (`-`) is replaced by the minus sign U+2212
    /// before layout, as in TeX, so that expressions such as `$-3$` and
    /// `$10^{-3}$` use the typographic minus of the math face and are spaced
    /// for its advance. Unstyled Latin letters and lowercase Greek letters are
    /// likewise replaced by their Unicode Mathematical Italic counterparts
    /// (so `$x$` is set with U+1D465), while digits and the contents of
    /// `\mathrm`, `\text` and `\operatorname` stay upright. Plain text is
    /// never altered.
    ///
    /// A size that is not a positive finite number yields an empty layout with
    /// a warning.
    ///
    /// Results are memoised on `(content, parse_math, size_pt)`, so repeated
    /// calls with identical arguments return the same [`Arc`].
    pub fn layout(&self, content: &str, parse_math: bool, size_pt: f64) -> Arc<TextLayout> {
        let key = (content.to_owned(), parse_math, size_pt.to_bits());
        if let Some(layout) = lock(&self.layouts).get(&key) {
            return Arc::clone(layout);
        }
        // Typeset without holding the lock, so that concurrent callers with
        // different labels do not wait on each other. If two callers race on
        // the same label, the first result inserted is the one both return.
        let computed = Arc::new(self.compute_layout(content, parse_math, size_pt));
        Arc::clone(lock(&self.layouts).entry(key).or_insert(computed))
    }

    /// Returns the outline of `glyph_id` in `font`, or `None` if the identifier
    /// is out of range or the glyph has no outline (for example a space).
    ///
    /// The path is em-normalised (1.0 is one em), has y pointing down (flipped
    /// from the font's y-up design space) and has its origin at the glyph
    /// origin on the baseline. Results are cached, so repeated calls return
    /// the same [`Arc`].
    pub fn glyph_outline(&self, font: FontId, glyph_id: u16) -> Option<Arc<BezPath>> {
        let key = (font, glyph_id);
        if let Some(cached) = lock(&self.outlines).get(&key) {
            return cached.clone();
        }
        let extracted = outline::extract(self.faces.get(font), glyph_id).map(Arc::new);
        lock(&self.outlines).entry(key).or_insert(extracted).clone()
    }

    /// Lays out `content` without consulting the memo.
    fn compute_layout(&self, content: &str, parse_math: bool, size_pt: f64) -> TextLayout {
        let mut builder = LayoutBuilder::default();
        if !(size_pt.is_finite() && size_pt > 0.0) {
            builder.warnings.push(TextWarning {
                source: content.to_owned(),
                message: format!(
                    "The text size {size_pt} pt is not a positive number, so the text is not drawn."
                ),
            });
            return builder.finish();
        }
        if content.is_empty() {
            return builder.finish();
        }
        if !parse_math {
            builder.append(plain::shape(
                &self.shaper_data,
                &self.faces,
                content,
                size_pt,
            ));
            return builder.finish();
        }

        let (segments, warnings) = segments::split(content);
        builder.warnings.extend(warnings);
        // Plain text, including the raw source of math that falls back, is
        // accumulated so that it is shaped as one run.
        let mut plain = String::new();
        for segment in segments {
            match segment {
                Segment::Plain(text) => plain.push_str(&text),
                Segment::Math(inner) => {
                    match math::typeset(
                        &inner,
                        size_pt,
                        &self.math_font,
                        &self.math_params,
                        &self.faces,
                    ) {
                        Ok(output) => {
                            self.flush_plain(&mut builder, &mut plain, size_pt);
                            if output.ignored_colour {
                                builder.warnings.push(TextWarning {
                                    source: format!("${inner}$"),
                                    message: "Colour in label math is not supported and was \
                                              ignored."
                                        .to_owned(),
                                });
                            }
                            builder.append(output.layout);
                        }
                        Err(message) => {
                            let source = format!("${inner}$");
                            plain.push_str(&source);
                            builder.warnings.push(TextWarning { source, message });
                        }
                    }
                }
            }
        }
        self.flush_plain(&mut builder, &mut plain, size_pt);
        builder.finish()
    }

    /// Shapes and appends any accumulated plain text.
    fn flush_plain(&self, builder: &mut LayoutBuilder, plain: &mut String, size_pt: f64) {
        if !plain.is_empty() {
            builder.append(plain::shape(&self.shaper_data, &self.faces, plain, size_pt));
            plain.clear();
        }
    }
}

/// Locks `mutex`, recovering the data if another thread panicked while
/// holding it. The guarded caches only ever receive complete entries, so a
/// poisoned cache is still consistent.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The typeset form of one segment, positioned with its origin at the left
/// end of its own baseline.
#[derive(Clone, Debug, Default)]
pub(crate) struct SegmentLayout {
    /// Glyph runs and rules.
    pub(crate) items: Vec<TextItem>,
    /// Advance width in points.
    pub(crate) width: f64,
    /// Extent above the baseline in points.
    pub(crate) height: f64,
    /// Extent below the baseline in points.
    pub(crate) depth: f64,
}

/// Concatenates segments along a shared baseline.
#[derive(Default)]
struct LayoutBuilder {
    /// Items placed so far.
    items: Vec<TextItem>,
    /// Pen position after the last segment.
    width: f64,
    /// Largest height of any segment.
    height: f64,
    /// Largest depth of any segment.
    depth: f64,
    /// Warnings collected so far.
    warnings: Vec<TextWarning>,
}

impl LayoutBuilder {
    /// Places `segment` at the current pen position and advances the pen.
    fn append(&mut self, segment: SegmentLayout) {
        let dx = self.width;
        self.items
            .extend(segment.items.into_iter().map(|item| match item {
                TextItem::Glyphs(mut run) => {
                    for glyph in &mut run.glyphs {
                        glyph.x += dx;
                    }
                    TextItem::Glyphs(run)
                }
                TextItem::Rule {
                    x,
                    y,
                    width,
                    height,
                } => TextItem::Rule {
                    x: x + dx,
                    y,
                    width,
                    height,
                },
            }));
        self.width += segment.width;
        self.height = self.height.max(segment.height);
        self.depth = self.depth.max(segment.depth);
    }

    /// Returns the finished layout.
    fn finish(self) -> TextLayout {
        TextLayout {
            items: self.items,
            width: self.width.max(0.0),
            height: self.height,
            depth: self.depth,
            warnings: self.warnings,
        }
    }
}

/// The typeset form of one piece of label text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    /// Glyph runs and rules, in left-to-right source order. Consecutive
    /// glyphs of one segment that share a font and a size are merged into a
    /// single run.
    pub items: Vec<TextItem>,
    /// Advance width in points: the pen position after the last glyph or
    /// rule, not the ink width.
    pub width: f64,
    /// Extent above the baseline in points; never negative.
    ///
    /// A plain text segment contributes the ascender of the text face scaled
    /// to the requested size, whatever its characters, so that labels set at
    /// one size share a height and align without jitter. A math segment
    /// contributes the height of its `latex-rust` box, which follows the ink
    /// of its glyphs and rules. The layout takes the largest contribution; an
    /// empty layout has zero height.
    pub height: f64,
    /// Extent below the baseline in points; never negative.
    ///
    /// A plain text segment contributes the magnitude of the text face
    /// descender scaled to the requested size, and a math segment contributes
    /// the depth of its `latex-rust` box. The layout takes the largest
    /// contribution; an empty layout has zero depth.
    pub depth: f64,
    /// Problems encountered while typesetting, such as unsupported math.
    pub warnings: Vec<TextWarning>,
}

/// One drawable element of a [`TextLayout`].
#[derive(Clone, Debug, PartialEq)]
pub enum TextItem {
    /// Glyphs from one font at one size.
    Glyphs(GlyphRun),
    /// A filled rectangle (fraction bar, radical overbar or other rule), in
    /// points, whose top-left corner is given relative to the layout origin
    /// (x right, y down, baseline at y = 0).
    Rule {
        /// Left edge.
        x: f64,
        /// Top edge.
        y: f64,
        /// Horizontal extent.
        width: f64,
        /// Vertical extent.
        height: f64,
    },
}

/// A sequence of glyphs sharing one font and one size.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    /// Face the glyph identifiers refer to.
    pub font: FontId,
    /// Em size in points at which every glyph in the run is drawn.
    pub size_pt: f64,
    /// The source text the glyphs represent (for copyable PDF text); glyph
    /// `text_range`s index into it. For math runs this is the characters of
    /// the typeset glyphs rather than the LaTeX commands (so `$-\alpha$`
    /// yields U+2212 followed by U+03B1). A glyph that replaces several
    /// characters, such as a ligature, claims all of them.
    pub text: String,
    /// Positioned glyphs.
    pub glyphs: Vec<PositionedGlyph>,
}

/// A glyph placed relative to the layout origin.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedGlyph {
    /// OpenType glyph identifier in the run's font.
    pub id: u16,
    /// Pen position in points, measured right from the layout origin.
    pub x: f64,
    /// Baseline position in points, measured down from the layout baseline.
    pub y: f64,
    /// Byte range of [`GlyphRun::text`] this glyph represents.
    pub text_range: Range<usize>,
}

/// A non-fatal problem found while typesetting.
#[derive(Clone, Debug, PartialEq)]
pub struct TextWarning {
    /// The offending source segment, for math including its `$` delimiters.
    pub source: String,
    /// A human-readable description of the problem.
    pub message: String,
}
