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

    /// Emits the text turned a quarter turn to read upwards, with its baseline origin at `origin` in page space,
    /// recording a warning about `owner` when it extends beyond the page.
    ///
    /// The text occupies page x from `origin.x - height` to `origin.x + depth`, and page y from
    /// `origin.y - width` to `origin.y`.
    pub fn draw_upwards_on_page(
        &self,
        ctx: &mut Ctx,
        origin: Point,
        color: Rgba,
        owner: NodeId,
        out: &mut Vec<Item>,
    ) {
        self.draw_turned_on_page(ctx, origin, 90.0, color, owner, out);
    }

    /// Emits the text turned `degrees` anticlockwise, as [`TextBlock::draw_turned`] does, recording a warning about
    /// `owner` when its turned box extends beyond the page.
    pub fn draw_turned_on_page(
        &self,
        ctx: &mut Ctx,
        origin: Point,
        degrees: f64,
        color: Rgba,
        owner: NodeId,
        out: &mut Vec<Item>,
    ) {
        let b = self.turned_bounds(degrees);
        let bounds = Rect::new(origin.x + b.x, origin.y + b.y, b.width, b.height);
        ctx.check_on_page(owner, &self.source, bounds);
        self.draw_turned(origin, degrees, color, owner, out);
    }

    /// Returns the page-space offset of the point `local`, given in the text's own frame (x along the baseline from
    /// the baseline origin, y downwards), from the baseline origin once the text is turned `degrees` anticlockwise.
    pub fn turned_point(degrees: f64, local: Point) -> Point {
        let (s, c) = exact_sin_cos(degrees);
        // An anticlockwise turn on a page whose y axis points downwards: the baseline direction (1, 0) becomes
        // (cos, −sin), and the downward direction (0, 1) becomes (sin, cos).
        Point::new(c * local.x + s * local.y, -s * local.x + c * local.y)
    }

    /// Returns the corners of the text's box in its own frame, as [`TextBlock::turned_point`] takes them.
    pub fn corners(&self) -> [Point; 4] {
        let (w, h, d) = (self.width(), self.height(), self.depth());
        [
            Point::new(0.0, -h),
            Point::new(w, -h),
            Point::new(w, d),
            Point::new(0.0, d),
        ]
    }

    /// Returns the axis-aligned box that the text's box covers once turned `degrees` anticlockwise about its baseline
    /// origin, as an offset from that origin.
    pub fn turned_bounds(&self, degrees: f64) -> Rect {
        let points = self.corners().map(|p| Self::turned_point(degrees, p));
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for p in points {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    /// Emits the text with its baseline origin at `origin`.
    pub fn draw(&self, origin: Point, color: Rgba, source: NodeId, out: &mut Vec<Item>) {
        if !paths::finite(origin) {
            return;
        }
        out.extend(self.items(origin, color, source));
    }

    /// Emits the text turned `degrees` anticlockwise about its baseline origin, which lies at `origin` in page space.
    ///
    /// The text is wrapped in a group whose transform turns it; [`TextBlock::turned_bounds`] gives the box it covers.
    pub fn draw_turned(
        &self,
        origin: Point,
        degrees: f64,
        color: Rgba,
        source: NodeId,
        out: &mut Vec<Item>,
    ) {
        if !paths::finite(origin) || !degrees.is_finite() {
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
                // `Transform::rotate` turns positive angles clockwise on screen, so an anticlockwise turn is negated.
                transform: Some(
                    Transform::rotate(-degrees).then(Transform::translate(origin.x, origin.y)),
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
                            stroke_em: run.stroke_em,
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

/// Returns the sine and cosine of an angle in degrees, exact at whole quarter turns, so that the box of text turned
/// by a quarter turn is measured without rounding.
fn exact_sin_cos(degrees: f64) -> (f64, f64) {
    let quarter = degrees / 90.0;
    if quarter.fract() == 0.0 && quarter.is_finite() {
        match (quarter as i64).rem_euclid(4) {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        }
    } else {
        degrees.to_radians().sin_cos()
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
    let layout = ctx
        .text
        .layout_emboldened(content, math, size, ctx.embolden);
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
