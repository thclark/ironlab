//! Backend-neutral display list produced by the scene compiler.
//!
//! Every backend (the interactive canvas and the PDF exporter) draws exactly these primitives, so that what is
//! shown on screen and what is exported cannot diverge. Coordinates are in points (1/72 inch) in figure space,
//! with the origin at the top-left corner of the figure, x increasing to the right and y increasing downwards.

use ironlab_ir::NodeId;
use ironlab_text::FontId;

/// An RGBA colour with straight (non-premultiplied) alpha, each channel in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Rgba = Rgba::new(1.0, 1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_u8(rgb: [u8; 3]) -> Self {
        Self::new(rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, 1.0)
    }

    /// Returns the same colour with its alpha multiplied by `factor`.
    pub fn with_alpha_factor(self, factor: f32) -> Self {
        Self { a: self.a * factor, ..self }
    }
}

/// A point in figure space, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in figure space, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }
}

/// An affine transform mapping local coordinates `(x, y)` to `(a·x + c·y + e, b·x + d·y + f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Transform {
    pub const IDENTITY: Transform = Transform { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translate(x: f64, y: f64) -> Self {
        Self { e: x, f: y, ..Self::IDENTITY }
    }

    /// A rotation by `degrees` about the local origin; positive angles turn clockwise on screen because y points down.
    pub fn rotate(degrees: f64) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        Self { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 }
    }

    /// Returns the transform that applies `self` first and then `other`.
    pub fn then(self, other: Transform) -> Self {
        Self {
            a: other.a * self.a + other.c * self.b,
            b: other.b * self.a + other.d * self.b,
            c: other.a * self.c + other.c * self.d,
            d: other.b * self.c + other.d * self.d,
            e: other.a * self.e + other.c * self.f + other.e,
            f: other.b * self.e + other.d * self.f + other.f,
        }
    }

    pub fn apply(&self, p: Point) -> Point {
        Point::new(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }
}

/// One segment of a path outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    CubicTo(Point, Point, Point),
    Close,
}

/// The rule deciding which regions of a self-intersecting or multi-contour path are inside.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// A fill applied to the interior of a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fill {
    pub color: Rgba,
    pub rule: FillRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// A stroke applied along a path. Dash lengths are in points; an empty `dash` means a solid line.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub color: Rgba,
    pub width: f64,
    pub dash: Vec<f64>,
    pub dash_offset: f64,
    pub cap: LineCap,
    pub join: LineJoin,
}

/// A filled and/or stroked path.
#[derive(Clone, Debug, PartialEq)]
pub struct PathItem {
    pub segments: Vec<PathSegment>,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

/// A run of glyphs from one font at one size, positioned in figure space.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphsItem {
    pub font: FontId,
    pub size_pt: f64,
    pub color: Rgba,
    /// The text the glyphs represent, used by backends that preserve selectable text.
    pub text: String,
    pub glyphs: Vec<PlacedGlyph>,
}

/// A glyph placed with its origin (on the baseline) at `(x, y)` in figure space.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub id: u16,
    pub x: f64,
    pub y: f64,
    pub text_range: std::ops::Range<usize>,
}

/// A display item. `source` names the IR node that produced it, for future selection and picking.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub source: Option<NodeId>,
    pub kind: ItemKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    Path(PathItem),
    Glyphs(GlyphsItem),
    /// A group of items. `clip` is expressed in the parent coordinate space and applied before `transform`; the
    /// items are expressed in the group's local space, which `transform` maps into the parent space.
    Group {
        clip: Option<Rect>,
        transform: Option<Transform>,
        items: Vec<Item>,
    },
}

/// The complete, ordered list of items to draw for a figure; later items paint over earlier ones.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayList {
    pub width_pt: f64,
    pub height_pt: f64,
    pub background: Rgba,
    pub items: Vec<Item>,
}
