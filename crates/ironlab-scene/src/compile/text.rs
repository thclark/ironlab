//! Measurement and placement of typeset text.

use std::sync::Arc;

use ironlab_ir::{Interpreter, NodeId, Text};
use ironlab_text::{TextItem, TextLayout};

use crate::display::{GlyphsItem, Item, ItemKind, PlacedGlyph, Point, Rect, Rgba, Transform};

use super::Ctx;
use super::paths::{self, PathBuilder};

/// A piece of typeset text, measured but not yet placed.
#[derive(Clone)]
pub(crate) struct TextBlock {
    layout: Arc<TextLayout>,
    /// The source text the block was typeset from, which a warning about the block quotes.
    source: Arc<str>,
}

impl TextBlock {
    /// The source text the block was typeset from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The advance width in points.
    pub fn width(&self) -> f64 {
        finite_or_zero(self.layout.width)
    }

    /// The extent above the baseline in points.
    pub fn height(&self) -> f64 {
        finite_or_zero(self.layout.height)
    }

    /// The extent below the baseline in points.
    pub fn depth(&self) -> f64 {
        finite_or_zero(self.layout.depth)
    }

    /// The extent from the top of the text to its bottom, in points.
    pub fn total_height(&self) -> f64 {
        self.height() + self.depth()
    }

    /// Returns the baseline origin that puts the centre of the text's box at `centre`.
    pub fn origin_for_centre(&self, centre: Point) -> Point {
        Point::new(
            centre.x - self.width() / 2.0,
            centre.y + (self.height() - self.depth()) / 2.0,
        )
    }

    /// Returns the box of the text when its baseline origin is at `origin`.
    pub fn bounds(&self, origin: Point) -> Rect {
        Rect::new(
            origin.x,
            origin.y - self.height(),
            self.width(),
            self.total_height(),
        )
    }

    /// Emits the text with its baseline origin at `origin`, recording a warning about `owner` when it extends beyond
    /// the page.
    pub fn draw_on_page(
        &self,
        ctx: &mut Ctx,
        origin: Point,
        color: Rgba,
        owner: NodeId,
        out: &mut Vec<Item>,
    ) {
        ctx.check_on_page(owner, &self.source, self.bounds(origin));
        self.draw(origin, color, owner, out);
    }

    /// Emits the text turned a quarter turn to read upwards, as [`TextBlock::draw_upwards`] does, recording a warning
    /// about `owner` when it extends beyond the page.
    pub fn draw_upwards_on_page(
        &self,
        ctx: &mut Ctx,
        origin: Point,
        color: Rgba,
        owner: NodeId,
        out: &mut Vec<Item>,
    ) {
        let bounds = Rect::new(
            origin.x - self.height(),
            origin.y - self.width(),
            self.total_height(),
            self.width(),
        );
        ctx.check_on_page(owner, &self.source, bounds);
        self.draw_upwards(origin, color, owner, out);
    }

    /// Emits the text with its baseline origin at `origin`.
    pub fn draw(&self, origin: Point, color: Rgba, source: NodeId, out: &mut Vec<Item>) {
        if !paths::finite(origin) {
            return;
        }
        out.extend(self.items(origin, color, source));
    }

    /// Emits the text turned a quarter turn to read upwards, with its baseline origin at `origin` in page space.
    ///
    /// The text occupies page x from `origin.x - height` to `origin.x + depth`, and page y from
    /// `origin.y - width` to `origin.y`.
    pub fn draw_upwards(&self, origin: Point, color: Rgba, source: NodeId, out: &mut Vec<Item>) {
        if !paths::finite(origin) {
            return;
        }
        let items = self.items(Point::new(0.0, 0.0), color, source);
        if items.is_empty() {
            return;
        }
        out.push(Item {
            source: Some(source),
            kind: ItemKind::Group {
                clip: None,
                transform: Some(
                    Transform::rotate(-90.0).then(Transform::translate(origin.x, origin.y)),
                ),
                items,
            },
        });
    }

    /// Converts the layout into display items with the baseline origin at `origin`.
    fn items(&self, origin: Point, color: Rgba, source: NodeId) -> Vec<Item> {
        let mut out = Vec::new();
        for item in &self.layout.items {
            match item {
                TextItem::Glyphs(run) => {
                    let glyphs: Vec<PlacedGlyph> = run
                        .glyphs
                        .iter()
                        .filter(|g| g.x.is_finite() && g.y.is_finite())
                        .filter(|g| {
                            g.text_range.start <= g.text_range.end
                                && run.text.get(g.text_range.clone()).is_some()
                        })
                        .map(|g| PlacedGlyph {
                            id: g.id,
                            x: origin.x + g.x,
                            y: origin.y + g.y,
                            text_range: g.text_range.clone(),
                        })
                        .collect();
                    if glyphs.is_empty() || !(run.size_pt.is_finite() && run.size_pt > 0.0) {
                        continue;
                    }
                    out.push(Item {
                        source: Some(source),
                        kind: ItemKind::Glyphs(GlyphsItem {
                            font: run.font,
                            size_pt: run.size_pt,
                            color,
                            text: run.text.clone(),
                            glyphs,
                        }),
                    });
                }
                TextItem::Rule {
                    x,
                    y,
                    width,
                    height,
                } => {
                    let r = Rect::new(origin.x + x, origin.y + y, *width, *height);
                    if ![r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite()) {
                        continue;
                    }
                    let mut b = PathBuilder::new();
                    b.rect(r);
                    out.extend(paths::item(
                        source,
                        b.finish(),
                        Some(paths::fill(color)),
                        None,
                    ));
                }
            }
        }
        out
    }
}

fn finite_or_zero(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// Typesets IR text at `size` points, reporting any typesetting problem as a warning about `owner`.
pub(super) fn measure(ctx: &mut Ctx, text: &Text, size: f64, owner: NodeId) -> TextBlock {
    let math = text.interpreter == Interpreter::Latex;
    measure_str(ctx, &text.content, math, size, owner)
}

/// Typesets a string at `size` points, parsing `$…$` math when `math` is true, and reports any typesetting
/// problem as a warning about `owner`.
pub(super) fn measure_str(
    ctx: &mut Ctx,
    content: &str,
    math: bool,
    size: f64,
    owner: NodeId,
) -> TextBlock {
    let layout = ctx.text.layout(content, math, size);
    for warning in &layout.warnings {
        ctx.warn(
            Some(owner),
            format!("In the text {:?}: {}", warning.source, warning.message),
        );
    }
    TextBlock {
        layout,
        source: Arc::from(content),
    }
}
