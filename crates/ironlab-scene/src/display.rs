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
//!
//! # Depth
//!
//! The artists of a three-dimensional axes lie inside an [`ItemKind::Depth`] group, in the painter's order the scene
//! compiler chose, and every path and image among them carries the depth at which it is painted: a [`DepthPlane`]
//! over the item's local space, or one depth per endpoint of a path. Larger depths are nearer the viewer. A backend
//! with a depth buffer clears it at the group and tests every item against it, so that faces, lines, markers and
//! images which cross one another are resolved pixel by pixel; a backend without one draws the items in order and
//! gets the painter's picture. Depths are literal: no backend biases or reorders them. Inside a depth group every
//! path and image carries a depth, a path's vertex depths are one finite value per endpoint, and no glyph run or
//! further depth group appears.

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

    /// Returns the transform that undoes this one, or `None` when this one is singular or not finite, so that no
    /// point has a unique preimage.
    pub fn inverse(&self) -> Option<Transform> {
        let det = self.a * self.d - self.b * self.c;
        if !det.is_finite() || det == 0.0 {
            return None;
        }
        let inverse = Transform {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            e: (self.c * self.f - self.d * self.e) / det,
            f: (self.b * self.e - self.a * self.f) / det,
        };
        [
            inverse.a, inverse.b, inverse.c, inverse.d, inverse.e, inverse.f,
        ]
        .iter()
        .all(|v| v.is_finite())
        .then_some(inverse)
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

/// A depth that is affine over an item's local space: `depth(x, y) = a·x + b·y + c`.
///
/// Larger depths are nearer the viewer, as [`crate::maths::camera::Camera::project`] reports them; a backend
/// normalises the depths of one depth group among themselves before it tests them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DepthPlane {
    pub a: f64,
    pub b: f64,
    pub c: f64,
}

impl DepthPlane {
    /// The same depth everywhere: `a = b = 0`.
    pub const fn constant(depth: f64) -> Self {
        Self {
            a: 0.0,
            b: 0.0,
            c: depth,
        }
    }

    /// The depth at a point of the item's local space.
    pub fn at(&self, p: Point) -> f64 {
        self.a * p.x + self.b * p.y + self.c
    }

    /// Whether every coefficient is finite.
    pub fn is_finite(&self) -> bool {
        self.a.is_finite() && self.b.is_finite() && self.c.is_finite()
    }

    /// The same plane moved `by` farther from the viewer: every depth smaller by `by`, the tilt unchanged.
    pub fn pushed_back(self, by: f64) -> Self {
        Self {
            c: self.c - by,
            ..self
        }
    }
}

/// How deep a path lies, in its local coordinate space; larger is nearer the viewer.
#[derive(Clone, Debug, PartialEq)]
pub enum Depth {
    /// One plane for the whole path. The scene compiler gives the faces of a surface the plane fitted to their
    /// projected corners, shared by a face's fill and its edge so that the two never fight; markers a constant
    /// plane; and filled contour bands the plane they lie in.
    Plane(DepthPlane),
    /// One depth per endpoint of the path's segments (`MoveTo`, `LineTo` and the end point of `CubicTo`; none for
    /// `Close`), interpolated linearly along each segment: the polylines of plot3, contour3 and quiver3.
    Vertices(Vec<f64>),
}

/// A filled and/or stroked path.
///
/// `depth` is `Some` for every path inside an [`ItemKind::Depth`] group and `None` elsewhere.
#[derive(Clone, Debug, PartialEq)]
pub struct PathItem {
    pub segments: Vec<PathSegment>,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
    pub depth: Option<Depth>,
}

impl PathItem {
    /// The number of endpoints of the path: one for each `MoveTo`, `LineTo` and `CubicTo`; a `Close` has none.
    pub fn endpoint_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| !matches!(s, PathSegment::Close))
            .count()
    }

    /// Reports whether the depth can be used: none, a finite plane, or one finite depth per endpoint.
    pub fn is_valid_depth(&self) -> bool {
        match &self.depth {
            None => true,
            Some(Depth::Plane(plane)) => plane.is_finite(),
            Some(Depth::Vertices(depths)) => {
                depths.len() == self.endpoint_count() && depths.iter().all(|d| d.is_finite())
            }
        }
    }
}

/// A raster image drawn into an axis-aligned rectangle of the item's local coordinate space.
///
/// `samples` holds `width · height · channels` bytes, row by row from the top-left pixel of the image, which is
/// drawn at the top-left corner of `rect`: three channels for opaque red, green and blue, and four for red, green,
/// blue and straight (non-premultiplied) alpha. The values are sRGB, as everywhere else in the display list.
///
/// The samples are resolved true colour rather than data values with a colour mapping, so that a backend draws them
/// without consulting anything else. The scene compiler resolves the pixels of every image artist into this form,
/// so changing the colour limits of an axes re-maps every pixel of its colour-mapped images. A second variant that
/// carries unmapped samples with their colour mapping, so that such a change is a uniform update instead, waits for
/// the colormap redesign, which must define the mapping it would carry; it can be added beside this one without
/// disturbing it.
///
/// The samples are shared behind an [`Arc`] so that cloning a display list does not copy them.
///
/// `depth`, present for an image inside an [`ItemKind::Depth`] group, is the depth of the image over its own pixel
/// space, from which a backend places every pixel in depth.
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
    /// The depth of the image over its pixel space, inside a depth group.
    pub depth: Option<DepthPlane>,
}

impl ImageItem {
    /// The number of channels of an opaque image.
    pub const RGB: u8 = 3;
    /// The number of channels of an image with straight alpha.
    pub const RGBA: u8 = 4;

    /// Reports whether the item can be drawn: a positive, finite rectangle, a non-empty grid of samples, a supported
    /// channel count, exactly `width · height · channels` bytes of samples, and a finite depth when it has one.
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
            && self.depth.is_none_or(|plane| plane.is_finite())
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
            .field("depth", &self.depth)
            .finish()
    }
}

/// The markers of one artist, drawn as instances of one outline.
///
/// The outline is the marker of width 1 centred on the origin; every instance scales it by its size and moves it to
/// its position. A closed outline is filled with the instance's face colour under the non-zero rule and its edge is
/// stroked with the instance's edge colour at `edge_width` (in the item's units, not scaled by the size), with butt
/// caps and round joins, as the scene compiler strokes every line; an open outline (a plus or a cross) is only
/// stroked. The PDF exporter writes one path per instance; the viewer tessellates the outline once and draws the
/// instances through one instanced draw. An item with no instances draws nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkersItem {
    /// The outline of a marker of width 1 centred on the origin, shared by every instance.
    pub outline: Arc<[PathSegment]>,
    /// The width of the edge, in the item's units.
    pub edge_width: f64,
    pub instances: Vec<MarkerInstance>,
}

/// One marker of a [`MarkersItem`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkerInstance {
    /// The centre of the marker in the item's local space.
    pub position: Point,
    /// The depth of the marker, in the units of [`crate::maths::camera::Camera::project`]: a constant plane, which
    /// a backend with a depth buffer draws the marker at inside an [`ItemKind::Depth`] group and ignores elsewhere.
    pub depth: f64,
    /// The width of the marker, in the item's units.
    pub size_pt: f64,
    /// The colour of the interior, or `None` for a marker that is not filled.
    pub face: Option<Rgba>,
    /// The colour of the edge, or `None` for a marker without an edge.
    pub edge: Option<Rgba>,
    /// The index of the marker's point in the artist's data, carried for picking (issue #1).
    pub source_index: usize,
}

impl MarkersItem {
    /// Reports whether the item can be drawn: the outline starts with `MoveTo`, has a segment beyond it and only
    /// finite coordinates; `edge_width` is finite and not negative; and every instance has a finite position, a
    /// finite positive size, a finite depth and colours whose channels are numbers.
    pub fn is_valid(&self) -> bool {
        let finite = |p: Point| p.x.is_finite() && p.y.is_finite();
        let outline_ok = matches!(self.outline.first(), Some(PathSegment::MoveTo(_)))
            && self.outline.len() > 1
            && self.outline.iter().all(|segment| match *segment {
                PathSegment::MoveTo(p) | PathSegment::LineTo(p) => finite(p),
                PathSegment::CubicTo(c1, c2, p) => finite(c1) && finite(c2) && finite(p),
                PathSegment::Close => true,
            });
        let colour_ok =
            |c: Option<Rgba>| c.is_none_or(|c| [c.r, c.g, c.b, c.a].iter().all(|v| !v.is_nan()));
        outline_ok
            && self.edge_width.is_finite()
            && self.edge_width >= 0.0
            && self.instances.iter().all(|instance| {
                finite(instance.position)
                    && instance.size_pt.is_finite()
                    && instance.size_pt > 0.0
                    && instance.depth.is_finite()
                    && colour_ok(instance.face)
                    && colour_ok(instance.edge)
            })
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
    /// A raster image. The scene compiler emits one for every image artist it draws, in pixel space beneath a group
    /// whose transform places it in the axes (see [`crate::compile::compile`]), and the PDF exporter builds them
    /// when it replaces dense vector content with a raster.
    Image(ImageItem),
    /// The markers of one artist as instances of one outline. The scene compiler emits one per artist in a 2D axes
    /// and one per run of consecutive markers in the painter's order of a 3D axes.
    Markers(MarkersItem),
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
    /// `cells` is the number of data cells (surface faces) that the artist draws over the whole display list, which
    /// is what a backend thresholds on. An image artist is already a raster and is never marked dense, so that no
    /// backend resamples the pixels the user supplied. It is deliberately not the number of items in
    /// this group, because depth sorting in a 3D axes can split one artist's geometry into several dense groups
    /// separated by the geometry of other artists; every one of them records the same total.
    Dense {
        cells: u64,
        items: Vec<Item>,
    },
    /// The artists of one three-dimensional axes, in the painter's order the scene compiler chose, drawn with a
    /// depth buffer by a backend that has one.
    ///
    /// Like a dense group it carries neither clip nor transform, so a backend without a depth buffer draws its items
    /// in order and gets the painter's picture, which agrees with the depth-tested one wherever the painter's order
    /// is exact. Every path and image inside carries a depth; groups and dense groups may appear inside; glyph runs
    /// and further depth groups never do; and the group is never empty. A backend with a depth buffer clears the
    /// buffer at the group and tests every item against it.
    Depth {
        items: Vec<Item>,
    },
}

/// A visitor of leaves: the leaf, its accumulated transform, its clip in figure space, and its depth group.
type LeafVisitor<'a> = dyn FnMut(&Item, Transform, Option<Rect>, Option<usize>) + 'a;

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
    /// Visits every leaf item (paths, glyph runs, images and markers) in paint order.
    ///
    /// The callback receives the item, the accumulated transform from item space to figure space, and the
    /// intersection of all enclosing clips in figure space. Clips are only meaningful beneath translations and
    /// scalings; the scene compiler never places a clipped group beneath a rotation.
    ///
    /// [`ItemKind::Dense`] and [`ItemKind::Depth`] groups are descended into like any other group, so a backend that
    /// draws leaves through this method draws dense content as vector geometry, and the artists of a
    /// three-dimensional axes in painter's order, without having to know about either marking.
    pub fn visit_leaves(&self, mut visit: impl FnMut(&Item, Transform, Option<Rect>)) {
        self.visit_leaves_grouped(|item, transform, clip, _| visit(item, transform, clip));
    }

    /// Visits every leaf item as [`visit_leaves`](Self::visit_leaves) does, reporting with each leaf the depth group
    /// it lies in.
    ///
    /// The depth groups of the list are numbered from 0 in the order they are met in paint order, whatever their
    /// nesting, an empty group taking a number like any other, so that the numbering is a function of the list's
    /// structure alone; a leaf outside every depth group is reported with `None`. A backend with a depth buffer uses
    /// the number to bundle the leaves of one group into one depth-tested drawing.
    pub fn visit_leaves_grouped(
        &self,
        mut visit: impl FnMut(&Item, Transform, Option<Rect>, Option<usize>),
    ) {
        fn walk(
            items: &[Item],
            transform: Transform,
            clip: Option<Rect>,
            group: Option<usize>,
            next_group: &mut usize,
            visit: &mut LeafVisitor<'_>,
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
                        walk(items, transform, clip, group, next_group, visit);
                    }
                    ItemKind::Dense { items, .. } => {
                        walk(items, transform, clip, group, next_group, visit);
                    }
                    ItemKind::Depth { items } => {
                        let index = *next_group;
                        *next_group += 1;
                        walk(items, transform, clip, Some(index), next_group, visit);
                    }
                    _ => visit(item, transform, clip, group),
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
        let mut next_group = 0;
        walk(
            &self.items,
            Transform::IDENTITY,
            None,
            None,
            &mut next_group,
            &mut visit,
        );
    }
}
