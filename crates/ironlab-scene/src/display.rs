//! Backend-neutral display list produced by the scene compiler.
//!
//! Every backend (the interactive canvas and the PDF exporter) draws exactly these primitives, so that what is
//! shown on screen and what is exported cannot diverge. Coordinates are in points (1/72 inch) in figure space,
//! with the origin at the top-left corner of the figure, x increasing to the right and y increasing downwards.
//!
//! # Validity
//!
//! The scene compiler guarantees that every item it emits is valid: coordinates, widths, sizes and transforms are
//! finite; every path starts with `MoveTo`; dash arrays contain finite, non-negative lengths with a positive sum;
//! colour channels lie in `[0, 1]`; glyph text ranges are byte ranges within their run's text; and no clipped group
//! is placed beneath a group whose transform rotates or skews. Backends must nevertheless skip (not panic on) items
//! that violate these rules, because display lists can be built by hand.

use std::sync::Arc;

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
        Self::new(
            rgb[0] as f32 / 255.0,
            rgb[1] as f32 / 255.0,
            rgb[2] as f32 / 255.0,
            1.0,
        )
    }

    /// Returns the same colour with its alpha multiplied by `factor`.
    pub fn with_alpha_factor(self, factor: f32) -> Self {
        Self {
            a: self.a * factor,
            ..self
        }
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
        Self {
            x,
            y,
            width,
            height,
        }
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
    pub const IDENTITY: Transform = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(x: f64, y: f64) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }

    /// A rotation by `degrees` about the local origin; positive angles turn clockwise on screen because y points down.
    pub fn rotate(degrees: f64) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
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
        Point::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
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

/// A stroke applied along a path.
///
/// Width and dash lengths are in the item's local space, so an enclosing group transform scales them exactly as it
/// scales the geometry (the PDF convention). An empty `dash` means a solid line. `dash_offset` is the dash phase:
/// the distance into the dash pattern at which the stroke starts. Miter joins use a miter limit of 4.
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

/// A raster image drawn into an axis-aligned rectangle of the item's local coordinate space.
///
/// `samples` holds `width · height · channels` bytes, row by row from the top-left pixel of the image, which is
/// drawn at the top-left corner of `rect`: three channels for opaque red, green and blue, and four for red, green,
/// blue and straight (non-premultiplied) alpha. The values are sRGB, as everywhere else in the display list.
///
/// The samples are resolved true colour rather than data values with a colour mapping, so that a backend draws them
/// without consulting anything else. Whether the display list should also be able to carry unmapped samples with
/// their colour mapping, so that changing the colour limits of an image artist is a uniform update rather than a
/// re-mapping of every pixel, is an open question for the image artists of
/// [issue #7](https://github.com/thclark/ironlab/issues/7); a second variant can be added beside this one without
/// disturbing it.
///
/// The samples are shared behind an [`Arc`] so that cloning a display list does not copy them.
#[derive(Clone, PartialEq)]
pub struct ImageItem {
    /// Where the image is drawn, in the item's local coordinate space.
    pub rect: Rect,
    /// The number of columns of samples.
    pub width: u32,
    /// The number of rows of samples.
    pub height: u32,
    /// The number of channels per sample: 3 for opaque RGB, 4 for RGB with straight alpha.
    pub channels: u8,
    /// `width · height · channels` bytes.
    pub samples: Arc<[u8]>,
}

impl ImageItem {
    /// The number of channels of an opaque image.
    pub const RGB: u8 = 3;
    /// The number of channels of an image with straight alpha.
    pub const RGBA: u8 = 4;

    /// Reports whether the item can be drawn: a positive, finite rectangle, a non-empty grid of samples, a supported
    /// channel count, and exactly `width · height · channels` bytes of samples.
    pub fn is_valid(&self) -> bool {
        let finite = [self.rect.x, self.rect.y, self.rect.width, self.rect.height]
            .iter()
            .all(|v| v.is_finite());
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(usize::from(self.channels)));
        finite
            && self.rect.width > 0.0
            && self.rect.height > 0.0
            && self.width > 0
            && self.height > 0
            && matches!(self.channels, Self::RGB | Self::RGBA)
            && expected == Some(self.samples.len())
            && !self.samples.is_empty()
    }
}

impl std::fmt::Debug for ImageItem {
    /// Prints the shape of the image rather than its samples, of which there can be millions.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageItem")
            .field("rect", &self.rect)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("channels", &self.channels)
            .field("samples", &format_args!("{} bytes", self.samples.len()))
            .finish()
    }
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
    /// The byte range of the run's `text` that this glyph represents.
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
    /// A raster image. The scene compiler does not emit image items yet; the PDF exporter builds them when it
    /// replaces dense vector content with a raster, and the image artists of
    /// [issue #7](https://github.com/thclark/ironlab/issues/7) will emit them directly.
    Image(ImageItem),
    /// A group of items. `clip` is expressed in the parent coordinate space and applied before `transform`; the
    /// items are expressed in the group's local space, which `transform` maps into the parent space.
    Group {
        clip: Option<Rect>,
        transform: Option<Transform>,
        items: Vec<Item>,
    },
    /// Items drawn for one artist whose data is dense enough that a backend may draw them as a raster image instead
    /// of as vector geometry.
    ///
    /// A dense group carries neither a clip nor a transform: its items are expressed in the enclosing coordinate
    /// space and are subject to the enclosing clips, so a backend that ignores the marking and draws the items
    /// produces exactly the same picture as one that honours it. The interactive canvas ignores it; the PDF
    /// exporter uses it to replace the items with an image XObject (see [`crate::display`] consumers).
    ///
    /// `cells` is the number of data cells (surface faces, and in future image pixels) that the artist draws over
    /// the whole display list, which is what a backend thresholds on. It is deliberately not the number of items in
    /// this group, because depth sorting in a 3D axes can split one artist's geometry into several dense groups
    /// separated by the geometry of other artists; every one of them records the same total.
    Dense {
        cells: u64,
        items: Vec<Item>,
    },
}

/// The complete, ordered list of items to draw for a figure; later items paint over earlier ones.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayList {
    pub width_pt: f64,
    pub height_pt: f64,
    /// The colour painted over the whole page before any item. A fully transparent background paints nothing.
    pub background: Rgba,
    pub items: Vec<Item>,
}

impl DisplayList {
    /// Visits every leaf item (paths and glyph runs) in paint order.
    ///
    /// The callback receives the item, the accumulated transform from item space to figure space, and the
    /// intersection of all enclosing clips in figure space. Clips are only meaningful beneath translations and
    /// scalings; the scene compiler never places a clipped group beneath a rotation.
    ///
    /// [`ItemKind::Dense`] groups are descended into like any other group, so a backend that draws leaves through
    /// this method draws dense content as vector geometry without having to know about the marking.
    pub fn visit_leaves(&self, mut visit: impl FnMut(&Item, Transform, Option<Rect>)) {
        fn walk(
            items: &[Item],
            transform: Transform,
            clip: Option<Rect>,
            visit: &mut dyn FnMut(&Item, Transform, Option<Rect>),
        ) {
            for item in items {
                match &item.kind {
                    ItemKind::Group {
                        clip: group_clip,
                        transform: group_transform,
                        items,
                    } => {
                        let clip = match group_clip {
                            Some(c) => {
                                let a = transform.apply(Point::new(c.x, c.y));
                                let b = transform.apply(Point::new(c.right(), c.bottom()));
                                let local = Rect::new(
                                    a.x.min(b.x),
                                    a.y.min(b.y),
                                    (b.x - a.x).abs(),
                                    (b.y - a.y).abs(),
                                );
                                Some(match clip {
                                    Some(outer) => intersect(outer, local),
                                    None => local,
                                })
                            }
                            None => clip,
                        };
                        let transform = match group_transform {
                            Some(t) => t.then(transform),
                            None => transform,
                        };
                        walk(items, transform, clip, visit);
                    }
                    ItemKind::Dense { items, .. } => walk(items, transform, clip, visit),
                    _ => visit(item, transform, clip),
                }
            }
        }
        fn intersect(a: Rect, b: Rect) -> Rect {
            let x = a.x.max(b.x);
            let y = a.y.max(b.y);
            let right = a.right().min(b.right());
            let bottom = a.bottom().min(b.bottom());
            Rect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
        }
        walk(&self.items, Transform::IDENTITY, None, &mut visit);
    }
}
