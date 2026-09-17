//! LaTeX math typesetting with `latex-rust`.
//!
//! A math segment passes through four stages:
//!
//! 1. [`nesting_depth`] estimates how deeply the source nests. `latex-rust`
//!    parses and lays out recursively, so source above [`MAX_NESTING`] is
//!    rejected before it reaches the parser. The remaining stages run on a
//!    helper thread with a large stack, independent of the caller's stack.
//! 2. `latex_rust::parse` builds the math AST.
//! 3. [`rewrite`] applies the TeX conventions that `latex-rust` 1.0.2 does
//!    not: hyphen-minus becomes the minus sign U+2212, and unstyled Latin
//!    letters and lowercase Greek letters become Mathematical Italic.
//! 4. `latex_rust::layout` produces a box tree, which [`Walker`] converts to
//!    positioned glyphs and rules in points without recursion.

use latex_rust::{AtomKind, BoxContent, Dim, EnvRow, EqNumber, MathBox, MathNode, TextStyle};

use crate::fonts::Faces;
use crate::{FontId, GlyphRun, PositionedGlyph, SegmentLayout, TextItem};

/// Largest nesting depth, as measured by [`nesting_depth`], that is passed to
/// `latex-rust`.
///
/// `latex-rust` overflows the 2 MiB stack of an ordinary spawned thread at a
/// few dozen levels in unoptimised builds: measured with latex-rust 1.0.2 on
/// macOS, nested `\frac{1}{…}` overflows beyond 34 levels (this limit admits
/// 20), nested `\begin{matrix}` and `\sqrt{…}` beyond 30 (this limit admits 20
/// and 12), and nested `\left(` beyond 36 (this limit admits 24). Labels rarely
/// nest more than four or five levels, so the limit leaves ample room for real
/// labels while keeping a margin below the overflow even on a small stack;
/// [`typeset`] additionally runs `latex-rust` on a thread with a large stack.
pub(crate) const MAX_NESTING: usize = 24;

/// Stack size of the thread on which each math segment is typeset.
///
/// The memory is reserved address space that the operating system commits
/// only as the stack grows, so a generous size costs little. Together with
/// [`MAX_NESTING`] it keeps admitted math far from overflowing in any build.
const TYPESET_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Hard bound on AST depth for [`rewrite`], which recurses. The nesting guard
/// keeps real trees far shallower; this bound only makes the recursion
/// provably finite if the estimate were ever wrong.
const MAX_AST_DEPTH: usize = 256;

/// The unicode minus sign that replaces hyphen-minus in math.
const MINUS_SIGN: char = '\u{2212}';

/// The result of typesetting one math segment.
pub(crate) struct MathOutput {
    /// The typeset segment.
    pub layout: SegmentLayout,
    /// Whether the source used colour, which labels do not support and which
    /// was therefore ignored.
    pub ignored_colour: bool,
}

/// Typesets the math source `inner` (without `$` delimiters) at `size_pt`.
///
/// Returns a description of the problem if the source nests too deeply, cannot
/// be parsed or laid out, uses a construct that cannot be drawn with glyphs and
/// rules, or produces non-finite geometry.
pub(crate) fn typeset(
    inner: &str,
    size_pt: f64,
    font: &latex_rust::MathFont,
    params: &latex_rust::MathParams,
    faces: &Faces,
) -> Result<MathOutput, String> {
    let depth = nesting_depth(inner);
    if depth > MAX_NESTING {
        return Err(format!(
            "The math nests {depth} levels deep, more than the supported {MAX_NESTING}, so it \
             is shown as plain text."
        ));
    }

    // latex-rust parses and lays out recursively, and the AST and box tree
    // are also dropped recursively. All of that happens on a helper thread
    // with a large stack, so that the stack consumed does not depend on how
    // deep the caller's own stack already is (for example inside a GUI
    // framework in an unoptimised build). Only the flat result crosses back.
    let spawned = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("ironlab-text-math".to_owned())
            .stack_size(TYPESET_STACK_BYTES)
            .spawn_scoped(scope, || {
                typeset_nested(inner, size_pt, font, params, faces)
            })
            .map(|handle| handle.join())
    });
    match spawned {
        Ok(Ok(result)) => result,
        // latex-rust reports unsupported input as errors; a panic would be a
        // bug in it, which must still not take the figure down with it.
        Ok(Err(_)) => Err(
            "The math typesetter failed unexpectedly, so the math is shown as plain text."
                .to_owned(),
        ),
        Err(error) => Err(format!(
            "The math could not be typeset because a typesetting thread could not be started \
             ({error}), so it is shown as plain text."
        )),
    }
}

/// Parses, rewrites, lays out and walks the math source `inner`.
///
/// This recurses as deeply as the source nests, so [`typeset`] runs it on a
/// thread with a [`TYPESET_STACK_BYTES`] stack.
fn typeset_nested(
    inner: &str,
    size_pt: f64,
    font: &latex_rust::MathFont,
    params: &latex_rust::MathParams,
    faces: &Faces,
) -> Result<MathOutput, String> {
    let laid_out = latex_rust::parse(inner)
        .map_err(|e| e.to_string())
        .and_then(|mut ast| {
            if rewrite(&mut ast, 0) {
                latex_rust::layout(&ast, font, latex_rust::MathStyle::Text)
                    .map_err(|e| e.to_string())
            } else {
                Err("the math nests too deeply".to_owned())
            }
        });
    let tree = laid_out.map_err(|message| {
        format!("The math could not be typeset ({message}), so it is shown as plain text.")
    })?;

    let mut walker = Walker::new(size_pt, params, faces.get(FontId::Math));
    walker.walk(&tree)?;
    let output = walker.finish(&tree);
    let finite = output.layout.width.is_finite()
        && output.layout.height.is_finite()
        && output.layout.depth.is_finite();
    if finite {
        Ok(output)
    } else {
        Err("The math produced invalid dimensions, so it is shown as plain text.".to_owned())
    }
}

/// Estimates how deeply `source` nests, without parsing it recursively.
///
/// Each brace group nests one level inside its surroundings. Constructs that
/// wrap what follows them without braces also count: `^`, `_` and `'` scripts,
/// and commands that are not plain symbols (for example `\frac` or `\sqrt`),
/// each add a pending level that is resolved by the next atom or group, so
/// chains such as `\sqrt\sqrt x` or `x^\frac12` are measured. `\left`,
/// `\begin` and `\color` add a level until the matching `\right`, `\end` or the
/// end of the enclosing group. The estimate is deliberately conservative: it
/// may exceed the depth of the tree that `latex-rust` builds, but it is never
/// smaller than the nesting of braces and delimiters.
pub(crate) fn nesting_depth(source: &str) -> usize {
    /// Nesting state of one brace group.
    #[derive(Clone, Copy, Default)]
    struct Frame {
        /// Depth of the group itself.
        base: usize,
        /// Levels opened by `\left`, `\begin` or `\color` within the group.
        open: usize,
        /// Pending levels from scripts and commands awaiting their argument.
        pending: usize,
    }

    impl Frame {
        fn depth(self) -> usize {
            self.base + self.open + self.pending
        }
    }

    let mut stack: Vec<Frame> = Vec::new();
    let mut frame = Frame::default();
    let mut deepest = 0;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                let base = frame.depth() + 1;
                deepest = deepest.max(base);
                stack.push(frame);
                frame = Frame {
                    base,
                    ..Frame::default()
                };
            }
            '}' => {
                frame = stack.pop().unwrap_or_default();
                frame.pending = 0;
            }
            '^' | '_' | '\'' => {
                frame.pending += 1;
                deepest = deepest.max(frame.depth());
            }
            '\\' => {
                let mut name = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphabetic() {
                        name.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if name.is_empty() {
                    // A control symbol such as `\,`, `\{` or `\\` takes no
                    // argument and acts as an atom.
                    chars.next();
                    deepest = deepest.max(frame.depth());
                    frame.pending = 0;
                    continue;
                }
                match name.as_str() {
                    "left" | "begin" | "color" => {
                        frame.open += 1;
                        deepest = deepest.max(frame.depth());
                    }
                    "right" | "end" => frame.open = frame.open.saturating_sub(1),
                    _ if takes_no_argument(&name) => {
                        deepest = deepest.max(frame.depth());
                        frame.pending = 0;
                    }
                    _ => {
                        frame.pending += 1;
                        deepest = deepest.max(frame.depth());
                    }
                }
            }
            c if c.is_whitespace() => {}
            _ => {
                deepest = deepest.max(frame.depth());
                frame.pending = 0;
            }
        }
    }
    deepest
}

/// Returns whether the command `name` is a symbol or operator that takes no
/// argument, according to the `latex-rust` catalogue.
fn takes_no_argument(name: &str) -> bool {
    latex_rust::lookup(name).is_some_and(|entry| {
        matches!(
            entry.kind,
            latex_rust::SymbolKind::Symbol | latex_rust::SymbolKind::Operator
        )
    })
}

/// Applies TeX's character conventions to a parsed math tree in place.
///
/// Hyphen-minus atoms become the minus sign, keeping their atom class so that
/// binary-operator spacing is unchanged. Unstyled Latin letters and lowercase
/// Greek letters become their Mathematical Italic counterparts; letters inside
/// `\mathrm`, `\text`, `\operatorname` and other styled runs are parsed as text
/// or operator nodes and are left upright, as are digits.
///
/// Returns false, leaving the tree partly rewritten, if the tree is deeper than
/// [`MAX_AST_DEPTH`].
fn rewrite(node: &mut MathNode, depth: usize) -> bool {
    if depth > MAX_AST_DEPTH {
        return false;
    }
    let next = depth + 1;
    let all = |nodes: &mut [MathNode]| nodes.iter_mut().all(|n| rewrite(n, next));
    let opt =
        |node: &mut Option<Box<MathNode>>| node.as_deref_mut().is_none_or(|n| rewrite(n, next));
    match node {
        MathNode::Atom(ch, _) => {
            if *ch == '-' {
                *ch = MINUS_SIGN;
            } else if let Some(italic) = math_italic(*ch) {
                *ch = italic;
            }
            true
        }
        MathNode::Symbol(name) => {
            if let Some(italic) = latex_rust::glyph_char(name).and_then(math_italic) {
                let kind: AtomKind = latex_rust::symbol_atom_kind(name);
                *node = MathNode::Atom(italic, kind);
            }
            true
        }
        MathNode::Fraction(a, b)
        | MathNode::Superscript(a, b)
        | MathNode::Subscript(a, b)
        | MathNode::CancelTo(a, b) => rewrite(a, next) && rewrite(b, next),
        MathNode::SubSup(a, b, c) => rewrite(a, next) && rewrite(b, next) && rewrite(c, next),
        MathNode::Radical(index, body) => opt(index) && rewrite(body, next),
        MathNode::Row(items) | MathNode::Substack(items) => all(items),
        MathNode::Matrix(_, _, rows) => rows.iter_mut().all(|row| match row {
            EnvRow::Cells { cells, number, .. } => {
                all(cells)
                    && match number {
                        EqNumber::Tag { body, .. } => rewrite(body, next),
                        EqNumber::Default | EqNumber::Suppress => true,
                    }
            }
            EnvRow::Intertext(body) => rewrite(body, next),
            EnvRow::Hline => true,
        }),
        MathNode::Sum(a, b) | MathNode::Product(a, b) | MathNode::Integral(_, a, b) => {
            opt(a) && opt(b)
        }
        MathNode::Limit(a) => opt(a),
        MathNode::OverUnder(base, over, under) => rewrite(base, next) && opt(over) && opt(under),
        MathNode::Delimited(_, body, _)
        | MathNode::Accent(body, _)
        | MathNode::Tag { body, .. }
        | MathNode::Intertext(body)
        | MathNode::Color(_, body)
        | MathNode::TextColor(_, body)
        | MathNode::ColorBox(_, body)
        | MathNode::FColorBox(_, _, body)
        | MathNode::Phantom(_, body) => rewrite(body, next),
        MathNode::SizedDelim(..)
        | MathNode::Ref(_)
        | MathNode::Label(_)
        | MathNode::NoNumber
        | MathNode::Hline
        | MathNode::Text(..)
        | MathNode::Space(_)
        | MathNode::Operator(..)
        | MathNode::Strut(..) => true,
    }
}

/// Returns the Mathematical Italic form of an ASCII letter or a lowercase
/// Greek letter (including the variant forms ϵ, ϑ, ϰ, ϕ, ϱ and ϖ), or `None`
/// for any other character.
fn math_italic(ch: char) -> Option<char> {
    let applies = ch.is_ascii_alphabetic()
        || ('\u{03B1}'..='\u{03C9}').contains(&ch)
        || matches!(ch, 'ϵ' | 'ϑ' | 'ϰ' | 'ϕ' | 'ϱ' | 'ϖ');
    if !applies {
        return None;
    }
    let italic = latex_rust::styled_char(ch, TextStyle::It);
    (italic != ch).then_some(italic)
}

/// Converts a `latex-rust` dimension to `f64`.
///
/// `latex-rust` exposes its exact rational dimensions only through an IEEE
/// binary32 rounding, whose relative error (about 6e-8) is far below any
/// visible or measurable difference at label sizes. A dimension that
/// overflowed is NaN and is caught by the finiteness checks of the caller.
fn dim(d: &Dim) -> f64 {
    f64::from(f32::from_bits(d.to_ieee32_bits()))
}

/// One pending visit of the box-tree walk.
struct Visit<'t> {
    /// The box to draw.
    bx: &'t MathBox,
    /// Left edge of the box in points.
    x: f64,
    /// Baseline of the parent list in points (y down); the box's own
    /// baseline is this raised by its shift.
    parent_baseline: f64,
}

/// Converts a `latex-rust` box tree to positioned glyphs and rules.
///
/// The placement rules follow the `latex-rust` SVG renderer: a horizontal list
/// advances by each child's width; a vertical list sets its first child on the
/// baseline and stacks later children below it; a box's `shift` raises it;
/// overlaps, colour wrappers and frames place children at their own origin; a
/// rule spans from `height` above to `depth` below the baseline.
///
/// `latex-rust` glyph boxes carry no size, but their dimensions are already
/// scaled for script style. The walker therefore derives each glyph's scale as
/// its box width over the glyph's advance (or its box height over the ink
/// height when the advance is zero, as for combining accents), snapped to the
/// nearest of the text, script and script-script scales.
struct Walker<'f> {
    /// Em size of text-style math in points.
    size_pt: f64,
    /// The allowed glyph scales: text, script and script-script.
    scales: [f64; 3],
    /// Scale of the most recent glyph, used for glyphs with no measurable
    /// extent from which to derive one.
    last_scale: f64,
    /// The math face, for glyph advances and bounding boxes.
    face: &'f ttf_parser::Face<'static>,
    /// Completed items.
    items: Vec<TextItem>,
    /// Glyph run being accumulated.
    run: Option<GlyphRun>,
    /// Whether colour was encountered and ignored.
    ignored_colour: bool,
}

impl<'f> Walker<'f> {
    fn new(
        size_pt: f64,
        params: &latex_rust::MathParams,
        face: &'f ttf_parser::Face<'static>,
    ) -> Self {
        Self {
            size_pt,
            scales: [
                1.0,
                f64::from(params.script_percent_scale_down) / 100.0,
                f64::from(params.script_script_percent_scale_down) / 100.0,
            ],
            last_scale: 1.0,
            face,
            items: Vec::new(),
            run: None,
            ignored_colour: false,
        }
    }

    /// Converts an em dimension to points.
    fn pt(&self, d: &Dim) -> f64 {
        dim(d) * self.size_pt
    }

    /// Walks `root` iteratively in drawing order.
    fn walk(&mut self, root: &MathBox) -> Result<(), String> {
        let mut stack = vec![Visit {
            bx: root,
            x: 0.0,
            parent_baseline: 0.0,
        }];
        while let Some(Visit {
            bx,
            x,
            parent_baseline,
        }) = stack.pop()
        {
            let baseline = parent_baseline - self.pt(&bx.shift);
            // Children are pushed in reverse so that they are visited in order.
            match &bx.content {
                BoxContent::Empty | BoxContent::Kern(_) => {}
                BoxContent::Glyph { ch, glyph_id } => self.glyph(bx, *ch, *glyph_id, x, baseline),
                BoxContent::Rule => {
                    let top = baseline - self.pt(&bx.height);
                    let height = self.pt(&bx.height) + self.pt(&bx.depth);
                    self.rule(x, top, self.pt(&bx.width), height);
                }
                BoxContent::HList(children) => {
                    let mut child_x = x;
                    let mut visits = Vec::with_capacity(children.len());
                    for child in children {
                        visits.push(Visit {
                            bx: child,
                            x: child_x,
                            parent_baseline: baseline,
                        });
                        child_x += self.pt(&child.width);
                    }
                    stack.extend(visits.into_iter().rev());
                }
                BoxContent::VList(children) => {
                    let mut visits = Vec::with_capacity(children.len());
                    let mut below = 0.0;
                    for (i, child) in children.iter().enumerate() {
                        let child_baseline = if i == 0 {
                            baseline
                        } else {
                            baseline + below + self.pt(&child.height)
                        };
                        below += if i == 0 {
                            self.pt(&child.depth)
                        } else {
                            self.pt(&child.height) + self.pt(&child.depth)
                        };
                        visits.push(Visit {
                            bx: child,
                            x,
                            parent_baseline: child_baseline,
                        });
                    }
                    stack.extend(visits.into_iter().rev());
                }
                BoxContent::Overlap(children) => {
                    stack.extend(children.iter().rev().map(|child| Visit {
                        bx: child,
                        x,
                        parent_baseline: baseline,
                    }));
                }
                BoxContent::Color(_, inner) | BoxContent::BackColor(_, inner) => {
                    self.ignored_colour = true;
                    stack.push(Visit {
                        bx: inner,
                        x,
                        parent_baseline: baseline,
                    });
                }
                BoxContent::Frame {
                    thickness,
                    stroke,
                    inner,
                } => {
                    if stroke.is_some() {
                        self.ignored_colour = true;
                    }
                    let t = self.pt(thickness);
                    let width = self.pt(&bx.width);
                    let top = baseline - self.pt(&bx.height);
                    let height = self.pt(&bx.height) + self.pt(&bx.depth);
                    let side = (height - 2.0 * t).max(0.0);
                    self.rule(x, top, width, t);
                    self.rule(x, top + height - t, width, t);
                    self.rule(x, top + t, t, side);
                    self.rule(x + width - t, top + t, t, side);
                    stack.push(Visit {
                        bx: inner,
                        x,
                        parent_baseline: baseline,
                    });
                }
                BoxContent::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    thickness,
                } => {
                    let (x1, x2) = (x + self.pt(x1), x + self.pt(x2));
                    let (y1, y2) = (baseline - self.pt(y1), baseline - self.pt(y2));
                    let t = self.pt(thickness);
                    if y1 == y2 {
                        self.rule(x1.min(x2), y1 - t / 2.0, (x2 - x1).abs(), t);
                    } else if x1 == x2 {
                        self.rule(x1 - t / 2.0, y1.min(y2), t, (y2 - y1).abs());
                    } else {
                        return Err(
                            "The math draws a diagonal line (as \\cancel does), which labels \
                             cannot yet render, so it is shown as plain text."
                                .to_owned(),
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Adds a glyph whose box is `bx`, deriving its scale from the box.
    fn glyph(&mut self, bx: &MathBox, ch: char, glyph_id: u16, x: f64, baseline: f64) {
        let scale = self.glyph_scale(bx, glyph_id);
        let size_pt = self.size_pt * scale;
        let run = match &mut self.run {
            Some(run) if run.size_pt == size_pt => run,
            _ => {
                self.flush();
                self.run.insert(GlyphRun {
                    font: FontId::Math,
                    size_pt,
                    text: String::new(),
                    glyphs: Vec::new(),
                })
            }
        };
        let start = run.text.len();
        run.text.push(ch);
        run.glyphs.push(PositionedGlyph {
            id: glyph_id,
            x,
            y: baseline,
            text_range: start..run.text.len(),
        });
    }

    /// Derives and snaps the scale at which `glyph_id` was laid out in `bx`.
    fn glyph_scale(&mut self, bx: &MathBox, glyph_id: u16) -> f64 {
        let id = ttf_parser::GlyphId(glyph_id);
        let upem = f64::from(self.face.units_per_em());
        let advance = self
            .face
            .glyph_hor_advance(id)
            .map_or(0.0, |a| f64::from(a) / upem);
        let ratio = if advance > 0.0 {
            dim(&bx.width) / advance
        } else {
            let ink_height = self
                .face
                .glyph_bounding_box(id)
                .map_or(0.0, |b| f64::from(b.y_max.max(0)) / upem);
            let ink_depth = self
                .face
                .glyph_bounding_box(id)
                .map_or(0.0, |b| f64::from((-b.y_min).max(0)) / upem);
            if ink_height > 0.0 {
                dim(&bx.height) / ink_height
            } else if ink_depth > 0.0 {
                dim(&bx.depth) / ink_depth
            } else {
                f64::NAN
            }
        };
        let scale = if ratio.is_finite() {
            self.scales
                .iter()
                .copied()
                .min_by(|a, b| (a - ratio).abs().total_cmp(&(b - ratio).abs()))
                .unwrap_or(1.0)
        } else {
            self.last_scale
        };
        self.last_scale = scale;
        scale
    }

    /// Adds a filled rectangle, ignoring rectangles with no area.
    fn rule(&mut self, x: f64, y: f64, width: f64, height: f64) {
        if width > 0.0 && height > 0.0 {
            self.flush();
            self.items.push(TextItem::Rule {
                x,
                y,
                width,
                height,
            });
        }
    }

    /// Moves the run being accumulated, if any, to the completed items.
    fn flush(&mut self) {
        if let Some(run) = self.run.take() {
            self.items.push(TextItem::Glyphs(run));
        }
    }

    /// Completes the walk of `root`, returning the typeset segment.
    fn finish(mut self, root: &MathBox) -> MathOutput {
        self.flush();
        let items_finite = self.items.iter().all(|item| match item {
            TextItem::Glyphs(run) => run
                .glyphs
                .iter()
                .all(|g| g.x.is_finite() && g.y.is_finite()),
            TextItem::Rule {
                x,
                y,
                width,
                height,
            } => [x, y, width, height].iter().all(|v| v.is_finite()),
        });
        let invalid = if items_finite { 0.0 } else { f64::NAN };
        MathOutput {
            layout: SegmentLayout {
                width: self.pt(&root.width) + invalid,
                height: self.pt(&root.height).max(0.0) + invalid,
                depth: self.pt(&root.depth).max(0.0) + invalid,
                items: self.items,
            },
            ignored_colour: self.ignored_colour,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nesting_counts_groups_scripts_and_commands() {
        assert_eq!(nesting_depth("x"), 0);
        assert_eq!(nesting_depth("{x}"), 1);
        assert_eq!(nesting_depth("{{x}}"), 2);
        assert_eq!(nesting_depth(r"\alpha\beta\gamma\delta x"), 0);
        assert!(nesting_depth(r"\sqrt\sqrt\sqrt x") >= 3);
        assert!(nesting_depth("x''''") >= 4);
        assert!(nesting_depth(r"\left(\left(x\right)\right)") >= 2);
        let flat = "a^2 + b^2 + c^2 + d^2 + e^2 + f^2 + g^2 + h^2 + i^2 + j^2 + k^2 + l^2 + m^2";
        assert!(nesting_depth(flat) <= 2);
    }

    #[test]
    fn italic_applies_only_to_letters() {
        assert_eq!(math_italic('x'), Some('\u{1D465}'));
        assert_eq!(math_italic('h'), Some('\u{210E}'));
        assert_eq!(math_italic('A'), Some('\u{1D434}'));
        assert_eq!(math_italic('\u{03B1}'), Some('\u{1D6FC}'));
        assert_eq!(math_italic('2'), None);
        assert_eq!(math_italic('\u{0393}'), None);
    }
}
