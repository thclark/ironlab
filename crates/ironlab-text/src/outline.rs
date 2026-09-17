//! Glyph outline extraction.

use kurbo::BezPath;

/// Collects a `ttf-parser` outline into an em-normalised, y-down [`BezPath`].
struct Builder {
    /// Reciprocal of the face's units per em.
    scale: f64,
    /// The path under construction.
    path: BezPath,
}

impl Builder {
    /// Converts a point from font units (y up) to em units (y down).
    fn point(&self, x: f32, y: f32) -> kurbo::Point {
        kurbo::Point::new(f64::from(x) * self.scale, -f64::from(y) * self.scale)
    }
}

impl ttf_parser::OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.path.move_to(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.path.line_to(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p1, p) = (self.point(x1, y1), self.point(x, y));
        self.path.quad_to(p1, p);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (p1, p2, p) = (self.point(x1, y1), self.point(x2, y2), self.point(x, y));
        self.path.curve_to(p1, p2, p);
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

/// Returns the outline of `glyph_id` in `face`, em-normalised with y pointing
/// down, or `None` if the identifier is out of range or the glyph is blank.
pub(crate) fn extract(face: &ttf_parser::Face<'_>, glyph_id: u16) -> Option<BezPath> {
    if glyph_id >= face.number_of_glyphs() {
        return None;
    }
    let mut builder = Builder {
        scale: 1.0 / f64::from(face.units_per_em()),
        path: BezPath::new(),
    };
    face.outline_glyph(ttf_parser::GlyphId(glyph_id), &mut builder)?;
    if builder.path.elements().is_empty() {
        None
    } else {
        Some(builder.path)
    }
}
