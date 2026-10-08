//! LaTeX math typesetting with `latex-rust`.
//!
//! `latex_rust::parse` builds the math AST and `latex_rust::layout_with_em_size_pt` lays it out as a box tree, which
//! [`Walker`] converts to positioned glyphs and rules in points without recursion. `latex-rust` parses and lays out
//! recursively, and rejects input nested more deeply than `latex_rust::DEFAULT_MAX_NESTING_DEPTH` with an error,
//! which [`typeset`] reports so that the source falls back to plain text. Natively, parsing and layout run on a
//! helper thread with a stack of known size, so that the stack they use does not depend on the caller's; on
//! WebAssembly, whose one thread the linker gives a large stack, they run directly.

use latex_rust::{BoxContent, Dim, MathBox};

use crate::{FontId, GlyphRun, PositionedGlyph, SegmentLayout, TextItem};

/// Stack size of the thread on which each math segment is typeset natively.
///
/// `latex-rust` documents that input at its default nesting limit fits within the 2 MiB stack of a default thread in
/// an unoptimised build, with about twice the headroom needed. A typesetting thread of its own makes that hold
/// however deep the caller's stack already is (for example inside a GUI framework in an unoptimised build, or on a
/// worker thread with a small stack). The memory is reserved address space that the operating system commits only
/// as the stack grows, so a generous size costs little. WebAssembly has no such thread; see [`on_typeset_stack`].
#[cfg(not(target_arch = "wasm32"))]
const TYPESET_STACK_BYTES: usize = 8 * 1024 * 1024;

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
) -> Result<MathOutput, String> {
    // latex-rust parses and lays out recursively, and the AST and box tree
    // are also dropped recursively. All of that happens on a helper thread
    // with a stack of known size, so that the stack consumed does not depend
    // on how deep the caller's own stack already is. Only the flat result
    // crosses back.
    on_typeset_stack(|| typeset_nested(inner, size_pt, font))?
}

/// Runs `typeset` on a thread with [`TYPESET_STACK_BYTES`] of stack, so that
/// the stack it uses does not depend on the caller's, and returns what it
/// returned.
///
/// # Errors
///
/// Returns a description of the problem if the thread could not be started or
/// `typeset` panicked on it.
#[cfg(not(target_arch = "wasm32"))]
fn on_typeset_stack<T: Send>(typeset: impl FnOnce() -> T + Send) -> Result<T, String> {
    let spawned = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("ironlab-text-math".to_owned())
            .stack_size(TYPESET_STACK_BYTES)
            .spawn_scoped(scope, typeset)
            .map(|handle| handle.join())
    });
    match spawned {
        Ok(Ok(result)) => Ok(result),
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

/// Runs `typeset` directly: WebAssembly has one thread, whose stack the linker
/// sizes (the workspace's `.cargo/config.toml` asks for 8 MiB), and cannot
/// spawn another. A panic cannot be caught there either, as unwinding is
/// unsupported, so none is.
#[cfg(target_arch = "wasm32")]
fn on_typeset_stack<T: Send>(typeset: impl FnOnce() -> T + Send) -> Result<T, String> {
    Ok(typeset())
}

/// Parses, lays out and walks the math source `inner`.
///
/// This recurses as deeply as the source nests, so [`typeset`] runs it on a
/// thread with a [`TYPESET_STACK_BYTES`] stack.
fn typeset_nested(
    inner: &str,
    size_pt: f64,
    font: &latex_rust::MathFont,
) -> Result<MathOutput, String> {
    // latex-rust converts absolute TeX lengths, such as the delimiter
    // shortfall and AMSMath column spacing, to ems using this size.
    let em_size_pt = Dim::from_ieee32_bits((size_pt as f32).to_bits());
    let laid_out = latex_rust::parse(inner)
        .map_err(|e| e.to_string())
        .and_then(|ast| {
            latex_rust::layout_with_em_size_pt(&ast, font, latex_rust::MathStyle::Text, &em_size_pt)
                .map_err(|e| e.to_string())
        });
    let tree = laid_out.map_err(|message| {
        format!("The math could not be typeset ({message}), so it is shown as plain text.")
    })?;

    let mut walker = Walker::new(size_pt);
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

/// Converts a `latex-rust` dimension, an exact rational, to `f64`.
///
/// A dimension that overflowed is NaN and is caught by the finiteness checks
/// of the caller.
#[allow(clippy::cast_precision_loss)]
fn dim(d: &Dim) -> f64 {
    d.as_ratio()
        .map_or(f64::NAN, |(num, den)| num as f64 / den as f64)
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
/// rule spans from `height` above to `depth` below the baseline; a glyph is
/// drawn at its own scale of the em.
struct Walker {
    /// Em size of text-style math in points.
    size_pt: f64,
    /// Completed items.
    items: Vec<TextItem>,
    /// Glyph run being accumulated.
    run: Option<GlyphRun>,
    /// Whether colour was encountered and ignored.
    ignored_colour: bool,
}

impl Walker {
    fn new(size_pt: f64) -> Self {
        Self {
            size_pt,
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
                BoxContent::Glyph {
                    ch,
                    glyph_id,
                    scale,
                    ..
                } => self.glyph(*ch, *glyph_id, self.pt(scale), x, baseline),
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

    /// Adds a glyph set at `size_pt`.
    fn glyph(&mut self, ch: char, glyph_id: u16, size_pt: f64, x: f64, baseline: f64) {
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
