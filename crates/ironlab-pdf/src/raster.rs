//! Raster fallback for dense display-list content.
//!
//! A surface with tens of thousands of faces cannot reasonably be written as one vector path per face: the content
//! stream grows without bound, the file takes seconds to open, and no printer can resolve the difference. The scene
//! compiler marks such content with [`ItemKind::Dense`], and this module decides whether to draw it as vectors or as
//! a raster image, works out where the image belongs on the page, and builds the display list that is rendered into
//! it.
//!
//! # The raster comes from the viewer's renderer
//!
//! This crate never rasterises anything itself. The caller supplies a [`Rasteriser`], and the only implementation is
//! the viewer's headless GPU renderer (`ironlab_viewer::offscreen::OffscreenRenderer`), which tessellates and draws a
//! display list exactly as the interactive canvas does. The exported image is therefore the picture the user
//! inspected on screen, at print resolution, rather than the output of a second rasteriser that would drift from it.
//! Without a rasteriser, dense content is drawn as vector geometry whatever the policy says.
//!
//! # Three-dimensional axes
//!
//! The viewer draws the artists of a three-dimensional axes, which the scene compiler wraps in
//! [`ItemKind::Depth`], with a depth buffer, and a PDF has none. Under [`DepthPolicy::Auto`] the exporter therefore
//! asks the rasteriser for the axes twice, with and without the depth test, and writes the artists as vector paths in
//! painter's order only when the two renders are identical, which proves that the order shows the same picture at
//! the export resolution; otherwise it embeds the depth-tested render as an image, exactly as it embeds dense
//! content. [`DepthPolicy::Raster`] always embeds the render and [`DepthPolicy::Vector`] never asks. Every such
//! decision, and every dense group drawn as an image, is reported in the warnings of the rendered page. Two renders
//! show the same picture when they agree within [`SAME_PICTURE_TOLERANCE`] over every patch of
//! [`SAME_PICTURE_PATCH_PT`], which admits the slivers that anti-aliasing leaves along the shared edges of faces
//! and refuses any misdrawn face, marker or stretch of line.
//!
//! # Placement
//!
//! The image occupies the smallest rectangle that contains the dense geometry, intersected with the clips that
//! enclose it (in practice the axes' plot rectangle) and with the page, and grown outwards to whole pixels of the
//! export resolution so that no pixel is a fraction of a device pixel at the chosen resolution. The image is drawn at
//! that rectangle in figure space, so it lands exactly where the vector geometry would have, and it is drawn beneath
//! the same PDF clip, so content outside the plot box stays outside it.

use std::sync::Arc;

use ironlab_scene::display::{
    DisplayList, ImageItem, Item, ItemKind, PathSegment, Point, Rect, Rgba, Transform,
};

/// How the exporter draws a three-dimensional axes, whose artists the viewer draws with a depth buffer that a PDF
/// cannot have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DepthPolicy {
    /// Render the axes twice through the rasteriser, with and without the depth test, and write its artists as
    /// vector paths in painter's order when the two renders are identical, which proves that the order shows the
    /// same picture at the export resolution; otherwise embed the depth-tested render as an image.
    #[default]
    Auto,
    /// Write the artists as vector paths in painter's order without checking, with a warning.
    Vector,
    /// Embed the depth-tested render as an image, whatever the painter's order would show.
    Raster,
}

/// Whether an export needs a rasteriser at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Nothing is rasterised and nothing verified, so no rasteriser is needed.
    No,
    /// Three-dimensional axes are verified under [`DepthPolicy::Auto`] and nothing must be drawn as an image; an
    /// export without a rasteriser succeeds, drawing the axes back to front with a warning.
    ToVerify,
    /// Dense content the policy rasterises, or a three-dimensional axes under [`DepthPolicy::Raster`], must be drawn
    /// as an image, which fails without a rasteriser.
    ToDraw,
}

/// The number of data cells at which [`RasterPolicy::Auto`] switches a dense artist from vector to raster output.
///
/// Ten thousand cells is where the two representations cost the same. Each face is a filled, optionally stroked path
/// of the order of a hundred bytes of content stream, so the vector figure grows in proportion to the cell count,
/// while the deflated raster has a size set by the figure's area and [`DEFAULT_RASTER_DPI`] and does not grow at
/// all. Measured on a surface filling a figure of the default size at the default resolution, the whole document is
/// 33 KiB as vectors and 81 KiB as a raster at 2401 faces, 59 against 98 KiB at 4900, 112 against 105 KiB at 9801,
/// and 618 against 115 KiB at 39 601.
///
/// Below the threshold, vector output is therefore both smaller and better: resolution-independent, exactly
/// coloured, and editable. Above it the file grows without visible return, because a 100 × 100 grid across a plot
/// 300 points wide already puts three points across a face, which the default resolution samples twenty-five times.
pub const DEFAULT_RASTER_CELLS: u64 = 10_000;

/// The default resolution, in dots per inch, at which rasterised content is rendered.
///
/// Journals ask for 600 dots per inch for line art and for combined line and tone art, which is what a rasterised
/// surface is. At this resolution the rasterised part of a figure the size of a journal page is about 3800 by 2400
/// pixels, which deflates to a few hundred kilobytes.
pub const DEFAULT_RASTER_DPI: f64 = 600.0;

/// How the exporter chooses between vector and raster output for content the scene compiler marked as dense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterPolicy {
    /// Draw dense content as a raster image when it reaches `cells` data cells, and as vector geometry below that.
    Auto { cells: u64 },
    /// Draw dense content as vector geometry, however many cells it has.
    Never,
    /// Draw dense content as a raster image, however few cells it has.
    Always,
}

impl Default for RasterPolicy {
    fn default() -> Self {
        Self::Auto {
            cells: DEFAULT_RASTER_CELLS,
        }
    }
}

impl RasterPolicy {
    /// Reports whether content of `cells` data cells is drawn as a raster image.
    #[must_use]
    pub fn rasterises(self, cells: u64) -> bool {
        match self {
            Self::Auto { cells: threshold } => cells >= threshold,
            Self::Never => false,
            Self::Always => true,
        }
    }
}

/// The settings controlling the raster fallback for dense content and the drawing of three-dimensional axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterOptions {
    /// The choice between vector and raster output for dense content.
    pub policy: RasterPolicy,
    /// The resolution, in dots per inch, at which rasterised content is rendered. A figure is exported at its
    /// physical size, so this is the resolution the raster has on the printed page.
    pub dpi: f64,
    /// How three-dimensional axes are drawn.
    pub depth: DepthPolicy,
}

impl Default for RasterOptions {
    fn default() -> Self {
        Self {
            policy: RasterPolicy::default(),
            dpi: DEFAULT_RASTER_DPI,
            depth: DepthPolicy::default(),
        }
    }
}

impl RasterOptions {
    /// The resolution in pixels per point, or `None` when the resolution is not finite and positive.
    fn pixels_per_point(&self) -> Option<f64> {
        (self.dpi.is_finite() && self.dpi > 0.0).then(|| self.dpi / 72.0)
    }
}

/// An 8-bit RGBA image with straight (non-premultiplied) alpha, stored row by row from the top-left pixel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    /// `width · height · 4` bytes.
    pub rgba: Vec<u8>,
}

/// The side, in points, of the patches over which two renders of a three-dimensional axes are compared: about the
/// size of a character, so that a misdrawn marker or a hidden stretch of a line fills a noticeable part of a patch
/// while the slivers that anti-aliasing leaves along the shared edges of faces do not.
pub const SAME_PICTURE_PATCH_PT: f64 = 6.0;

/// The largest mean difference over a patch, in levels of 255 per channel, at which two renders still show the same
/// picture. A patch of the export resolution differs by this much when about a fortieth of it changes from one
/// colour to a contrasting one, which is a 0.15 pt sliver along a 6 pt edge, or a dot a third of a point across.
pub const SAME_PICTURE_TOLERANCE: f64 = 4.0;

/// Reports whether two renders of the same size show the same picture: no patch of [`SAME_PICTURE_PATCH_PT`] at
/// `dpi` differs by more than [`SAME_PICTURE_TOLERANCE`] on average over its pixels and channels. Renders of
/// different sizes never do.
///
/// The patches tile the image from its top-left corner, and the last patch of a row or column that the tiling does
/// not divide exactly is pulled back to the edge so that it is a whole patch overlapping its neighbour: every patch
/// covers the same area, so a sliver along the right or bottom edge is judged by the same rule as one in the
/// middle. An image smaller than a patch is one patch.
#[must_use]
pub fn same_picture(a: &RasterImage, b: &RasterImage, dpi: f64) -> bool {
    if a.width != b.width || a.height != b.height || a.rgba.len() != b.rgba.len() {
        return false;
    }
    let (width, height) = (a.width as usize, a.height as usize);
    if width == 0 || height == 0 || a.rgba.len() != width * height * 4 {
        return a.rgba == b.rgba;
    }
    let patch = if dpi.is_finite() && dpi > 0.0 {
        ((SAME_PICTURE_PATCH_PT * dpi / 72.0).round() as usize).max(1)
    } else {
        1
    };
    let starts = |extent: usize| {
        (0..extent)
            .step_by(patch)
            .map(move |start| start.min(extent.saturating_sub(patch)))
    };
    let limit = SAME_PICTURE_TOLERANCE * 4.0;
    for y0 in starts(height) {
        for x0 in starts(width) {
            let (y1, x1) = ((y0 + patch).min(height), (x0 + patch).min(width));
            let mut sum: u64 = 0;
            for y in y0..y1 {
                let start = (y * width + x0) * 4;
                let end = (y * width + x1) * 4;
                sum += a.rgba[start..end]
                    .iter()
                    .zip(&b.rgba[start..end])
                    .map(|(p, q)| u64::from(p.abs_diff(*q)))
                    .sum::<u64>();
            }
            let pixels = ((y1 - y0) * (x1 - x0)) as f64;
            if sum as f64 > limit * pixels {
                return false;
            }
        }
    }
    true
}

/// A renderer that draws a display list into an image.
///
/// The exporter calls this for the dense parts of a figure only. An implementation must draw the list exactly as the
/// interactive viewer draws it, at `dpi` dots per inch, into an image of `round(width_pt · dpi / 72)` by
/// `round(height_pt · dpi / 72)` pixels, leaving unpainted areas transparent.
pub trait Rasteriser {
    /// Renders `list` at `dpi` dots per inch, or explains why it could not.
    ///
    /// # Errors
    ///
    /// Returns a message describing the failure, which the exporter reports as [`crate::PdfError::Raster`].
    fn rasterise(&mut self, list: &DisplayList, dpi: f64) -> Result<RasterImage, String>;
}

impl<T: Rasteriser + ?Sized> Rasteriser for &mut T {
    fn rasterise(&mut self, list: &DisplayList, dpi: f64) -> Result<RasterImage, String> {
        (**self).rasterise(list, dpi)
    }
}

/// Reports whether any dense content of `list` would be drawn as a raster image under `options`.
#[must_use]
pub fn rasterises_any(list: &DisplayList, options: &RasterOptions) -> bool {
    fn walk(items: &[Item], options: &RasterOptions) -> bool {
        items.iter().any(|item| match &item.kind {
            ItemKind::Dense { cells, items } => {
                options.policy.rasterises(*cells) || walk(items, options)
            }
            ItemKind::Group { items, .. } | ItemKind::Depth { items } => walk(items, options),
            _ => false,
        })
    }
    options.pixels_per_point().is_some() && walk(&list.items, options)
}

/// Reports whether an export of `list` under `options` needs a rasteriser: to draw an image, only to verify the
/// painter's order of three-dimensional axes, or not at all.
///
/// A caller uses this to decide whether to look for a graphics adapter, and what a missing one means: nothing when
/// the answer is [`Need::No`], a warning when it is [`Need::ToVerify`], and an error when it is [`Need::ToDraw`].
#[must_use]
pub fn needs_rasteriser(list: &DisplayList, options: &RasterOptions) -> Need {
    fn depth_groups(items: &[Item]) -> bool {
        items.iter().any(|item| match &item.kind {
            ItemKind::Depth { .. } => true,
            ItemKind::Group { items, .. } | ItemKind::Dense { items, .. } => depth_groups(items),
            _ => false,
        })
    }
    if options.pixels_per_point().is_none() {
        return Need::No;
    }
    let depth = depth_groups(&list.items);
    if rasterises_any(list, options) || (depth && options.depth == DepthPolicy::Raster) {
        Need::ToDraw
    } else if depth && options.depth == DepthPolicy::Auto {
        Need::ToVerify
    } else {
        Need::No
    }
}

/// A dense group prepared for drawing as an image: where it goes and what is rendered into it.
pub(crate) struct Plan {
    /// The rectangle the image occupies, in figure space.
    pub rect: Rect,
    /// The display list rendered into the image, one point of which is one point of `rect`.
    pub list: DisplayList,
    /// The resolution of the image, in pixels per point.
    scale: f64,
}

impl Plan {
    /// Turns a readback into the image item that is drawn on the page, or returns `None` when the readback does not
    /// hold the samples it claims to.
    ///
    /// The rectangle is recomputed from the pixel dimensions the rasteriser actually produced, so the image is
    /// always drawn at exactly the requested resolution even if the rasteriser rounded the size differently from
    /// the plan. It keeps the planned top-left corner, which is on a whole pixel of that resolution.
    ///
    /// A readback in which every pixel is opaque is stored with three channels, which drops a soft mask that would
    /// otherwise be a quarter of the image's size and say nothing.
    pub fn image(&self, rendered: &RasterImage) -> Option<ImageItem> {
        let pixels = (rendered.width as usize).checked_mul(rendered.height as usize)?;
        if rendered.width == 0 || rendered.height == 0 || rendered.rgba.len() != pixels * 4 {
            return None;
        }
        let opaque = rendered
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255);
        let (channels, samples): (u8, Arc<[u8]>) = if opaque {
            let mut rgb = Vec::with_capacity(pixels * 3);
            for pixel in rendered.rgba.as_chunks::<4>().0 {
                rgb.extend_from_slice(&pixel[..3]);
            }
            (ImageItem::RGB, rgb.into())
        } else {
            (ImageItem::RGBA, rendered.rgba.as_slice().into())
        };
        let item = ImageItem {
            rect: Rect::new(
                self.rect.x,
                self.rect.y,
                f64::from(rendered.width) / self.scale,
                f64::from(rendered.height) / self.scale,
            ),
            width: rendered.width,
            height: rendered.height,
            channels,
            samples,
            depth: None,
        };
        item.is_valid().then_some(item)
    }
}

/// Plans the image for a group of items, or returns `None` when they should be drawn as vector geometry.
///
/// `to_figure` maps the group's coordinates into figure space and `clip` is the intersection of the clips enclosing
/// it, both as accumulated by the painter. `page` bounds the rectangle so that geometry running far outside the
/// figure cannot demand an enormous image. With `depth_tested` the rendered list holds the items in a depth group,
/// so that the rasteriser draws them with its depth buffer; without it they are drawn in order.
pub(crate) fn plan(
    items: &[Item],
    to_figure: Transform,
    clip: Option<Rect>,
    page: Rect,
    options: &RasterOptions,
    depth_tested: bool,
) -> Option<Plan> {
    let scale = options.pixels_per_point()?;
    let rect = snap_out(extent(items, to_figure, clip, page)?, scale)?;

    // The clip is expressed in the parent space of the group, which for the rendered list is the image's own space.
    let local_clip = clip.map(|c| Rect::new(c.x - rect.x, c.y - rect.y, c.width, c.height));
    let items = if depth_tested {
        vec![Item {
            source: None,
            kind: ItemKind::Depth {
                items: items.to_vec(),
            },
        }]
    } else {
        items.to_vec()
    };
    Some(Plan {
        rect,
        scale,
        list: DisplayList {
            width_pt: rect.width,
            height_pt: rect.height,
            background: Rgba::new(0.0, 0.0, 0.0, 0.0),
            items: vec![Item {
                source: None,
                kind: ItemKind::Group {
                    clip: local_clip,
                    transform: Some(to_figure.then(Transform::translate(-rect.x, -rect.y))),
                    items,
                },
            }],
        },
    })
}

/// The rectangle of figure space in which a group of items can show: the bounds of the items mapped through
/// `to_figure`, within the page and the enclosing `clip`. `None` when the items enclose no finite geometry or when
/// nothing of them lies within the clip and the page, so that there is nothing to draw, to verify or to rasterise.
pub(crate) fn extent(
    items: &[Item],
    to_figure: Transform,
    clip: Option<Rect>,
    page: Rect,
) -> Option<Rect> {
    let mut rect = intersect(bounds(items, to_figure)?, page);
    if let Some(clip) = clip {
        rect = intersect(rect, clip);
    }
    (rect.width > 0.0 && rect.height > 0.0).then_some(rect)
}

/// Grows a rectangle outwards to whole pixels of a resolution of `scale` pixels per point, or returns `None` when it
/// is empty or does not survive the rounding.
fn snap_out(rect: Rect, scale: f64) -> Option<Rect> {
    let x0 = (rect.x * scale).floor();
    let y0 = (rect.y * scale).floor();
    let x1 = (rect.right() * scale).ceil();
    let y1 = (rect.bottom() * scale).ceil();
    let finite = [x0, y0, x1, y1].iter().all(|v| v.is_finite());
    if !finite || x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(Rect::new(
        x0 / scale,
        y0 / scale,
        (x1 - x0) / scale,
        (y1 - y0) / scale,
    ))
}

/// The intersection of two rectangles, which is empty when they do not overlap.
pub(crate) fn intersect(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    Rect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
}

/// The smallest rectangle containing every item, mapped through `transform`, or `None` when the items enclose no
/// finite geometry.
///
/// The bound is conservative rather than tight: a curve is bounded by its control points, a stroke by half its width
/// times the miter limit, and a glyph run by its origins grown by its em size. Every one of those is a superset of
/// the ink, which is what the image must cover.
fn bounds(items: &[Item], transform: Transform) -> Option<Rect> {
    let mut box_ = BoundingBox::default();
    accumulate(items, transform, &mut box_);
    box_.finish()
}

/// A growing axis-aligned bound in figure space.
#[derive(Debug)]
struct BoundingBox {
    min: Point,
    max: Point,
}

impl Default for BoundingBox {
    fn default() -> Self {
        Self {
            min: Point::new(f64::INFINITY, f64::INFINITY),
            max: Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        }
    }
}

impl BoundingBox {
    fn add(&mut self, p: Point, margin: f64) {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return;
        }
        self.min = Point::new(self.min.x.min(p.x - margin), self.min.y.min(p.y - margin));
        self.max = Point::new(self.max.x.max(p.x + margin), self.max.y.max(p.y + margin));
    }

    fn finish(&self) -> Option<Rect> {
        (self.min.x <= self.max.x && self.min.y <= self.max.y).then(|| {
            Rect::new(
                self.min.x,
                self.min.y,
                self.max.x - self.min.x,
                self.max.y - self.min.y,
            )
        })
    }
}

/// Adds every item to a bound, mapping its coordinates through `transform`.
fn accumulate(items: &[Item], transform: Transform, box_: &mut BoundingBox) {
    for item in items {
        match &item.kind {
            ItemKind::Path(path) => {
                let margin = path.stroke.as_ref().map_or(0.0, |s| {
                    let scale = linear_scale(transform);
                    // A miter join reaches at most the miter limit times half the stroke width beyond the vertex.
                    let reach = s.width * f64::from(crate::MITER_LIMIT) / 2.0;
                    if reach.is_finite() && reach > 0.0 {
                        reach * scale
                    } else {
                        0.0
                    }
                });
                for segment in &path.segments {
                    match *segment {
                        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => {
                            box_.add(transform.apply(p), margin);
                        }
                        PathSegment::CubicTo(c1, c2, p) => {
                            for q in [c1, c2, p] {
                                box_.add(transform.apply(q), margin);
                            }
                        }
                        PathSegment::Close => {}
                    }
                }
            }
            ItemKind::Glyphs(run) => {
                let margin = if run.size_pt.is_finite() && run.size_pt > 0.0 {
                    run.size_pt * linear_scale(transform)
                } else {
                    0.0
                };
                for glyph in &run.glyphs {
                    box_.add(transform.apply(Point::new(glyph.x, glyph.y)), margin);
                }
            }
            ItemKind::Group {
                transform: group,
                items,
                ..
            } => {
                let inner = group.map_or(transform, |t| t.then(transform));
                accumulate(items, inner, box_);
            }
            ItemKind::Dense { items, .. } | ItemKind::Depth { items } => {
                accumulate(items, transform, box_);
            }
            ItemKind::Image(image) => {
                box_.add(transform.apply(Point::new(image.rect.x, image.rect.y)), 0.0);
                box_.add(
                    transform.apply(Point::new(image.rect.right(), image.rect.bottom())),
                    0.0,
                );
            }
        }
    }
}

/// The largest factor by which a transform can lengthen a vector, bounding how far it spreads a stroke.
fn linear_scale(t: Transform) -> f64 {
    let scale = (t.a * t.a + t.b * t.b)
        .sqrt()
        .max((t.c * t.c + t.d * t.d).sqrt());
    if scale.is_finite() { scale } else { 0.0 }
}
