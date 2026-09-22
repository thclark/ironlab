//! Conversion of a display list into `egui` triangle meshes.
//!
//! # Approach
//!
//! Every leaf of the display list is visited with [`DisplayList::visit_leaves`], which supplies the accumulated
//! group transform and the effective clip rectangle in figure space.
//!
//! - **Paths** are converted to a lyon path in screen coordinates (group transform applied first, then the
//!   [`ScreenTransform`]). Fills are tessellated with lyon's `FillTessellator`, honouring the display list's
//!   [`FillRule`]. Strokes are tessellated with lyon's `StrokeTessellator`, with the
//!   width scaled by [`ScreenTransform::scale`] and caps and joins mapped one to one. Lyon has no dashing, so a dashed
//!   stroke is first split into its "on" intervals with `lyon_algorithms::measure::PathMeasurements::split_range`
//!   (dash lengths and offset scaled to screen units), and each interval is stroked as an open sub-path.
//! - **Glyph runs** use [`TextEngine::glyph_outline`], which is em-normalised with y pointing down. Each outline is
//!   scaled by the run's `size_pt`, translated to the glyph origin, transformed into screen space and filled with the
//!   non-zero rule. Tessellated glyphs may be cached per `(font, glyph, size bucket)`.
//! - **Images** are drawn as textured quads. A valid image item is cut into tiles of at most
//!   [`TextureProvider::max_side`] pixels on a side, numbered in row-major order, because a graphics device has a
//!   largest texture side and a data image can exceed it. The pixels of a tile are built from the item's samples
//!   ([`egui::ColorImage::from_rgb`] for three channels and [`egui::ColorImage::from_rgba_unmultiplied`] for four,
//!   so that straight alpha is premultiplied as egui expects) and handed to a [`TextureProvider`], which returns
//!   the texture to sample. The tile is one quad: four white vertices at the corners of its sub-rectangle of the
//!   item rectangle, mapped through the same transforms as every other leaf (a parallelogram where a 3D placement
//!   shears), carrying the texture coordinates (0, 0), (1, 0), (0, 1) and (1, 1), so that the texture is drawn
//!   unmodulated. Every tile edge is computed from its integer pixel coordinate through the same affine chain, so
//!   abutting tiles share bit-equal screen edges and show no seam. A tile whose quad lies wholly outside the leaf's
//!   clip is never requested from the provider. Textures are sampled with nearest filtering
//!   ([`egui::TextureOptions::NEAREST`]), so that pixel edges are as hard on screen as they are in an exported PDF.
//! - **Clips** are applied geometrically: the tessellated triangles of a clipped leaf are each clipped against the
//!   clip rectangle (converted to screen space) with the Sutherland–Hodgman algorithm, and the resulting convex
//!   polygon is re-triangulated as a fan. A vertex made where an edge crosses the clip takes its texture coordinate
//!   from the same interpolation parameter as its position, so a clipped image keeps exactly the part of its
//!   texture that remains visible. This keeps the output a plain list of meshes, independent of the painter's clip
//!   rectangle, so that the same meshes can be drawn by the interactive canvas and by the offscreen renderer.
//! - **Colours** are straight alpha in the display list and are converted to premultiplied
//!   [`egui::Color32`] with [`egui::Color32::from_rgba_unmultiplied`] after conversion from `[0, 1]` to `[0, 255]`.
//!
//! The vertices of paths and glyphs use [`egui::epaint::WHITE_UV`] and the default texture, so those meshes are drawn
//! as solid colour; the vertices of an image tile sample its texture. The meshes are not anti-aliased by egui; the
//! viewer relies on 4× MSAA for smooth edges.
//!
//! # Depth groups
//!
//! [`drawables_with`] is the whole of the above with one addition: the leaves of an [`ItemKind::Depth`] group are
//! not made into meshes but into one [`DrawList`] for the depth-tested pipelines of [`crate::gpu`], in the group's
//! place in the paint order, and so is every run of consecutive leaves outside any group that carry a depth, drawn
//! without the depth test. The triangles are the same lyon output, in screen units, with the depth of every vertex:
//! a [`Depth::Plane`] evaluated at the vertex's item-space position, or a [`Depth::Vertices`] carried through lyon
//! as a custom attribute and so interpolated along fills, strokes and dash pieces alike; an image tile's corners take
//! its plane at their pixel-space positions. Depths are normalised to `[0, 1]` over the list, 0 the nearest.
//! [`tessellate_with`] and [`tessellate`] are the meshes alone, for callers with no depth pipelines.
//!
//! # Texture providers
//!
//! [`tessellate`] draws no images, for callers that have no textures to draw them with; [`tessellate_with`] takes a
//! [`TextureProvider`]. The interactive canvas provides a [`TextureCache`], which keeps a texture for every sample
//! buffer and tile across frames, so that a gesture re-uploads nothing, and frees the textures that a rebuild of the
//! meshes did not request. The offscreen renderer uploads the tiles of one render as user textures and frees them
//! once the render is read back (see [`crate::offscreen`]).

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use ironlab_scene::display::{
    Depth, DisplayList, FillRule, GlyphsItem, ImageItem, ItemKind, LineCap, LineJoin, PathItem,
    PathSegment, Point, Rect, Rgba, Stroke, Transform,
};
use ironlab_text::{FontId, TextEngine};
use kurbo::PathEl;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};
use lyon_algorithms::measure::{PathMeasurements, SampleType};

use crate::gpu::{Draw, DrawList, Drawable, TileKey, Vertex};

/// The mapping from figure space (points, y down) to screen space (egui points or pixels, y down).
///
/// A figure-space point `p` maps to `origin + scale · p`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenTransform {
    /// Screen units per figure point. In the interactive canvas this is egui points per figure point; offscreen it is
    /// pixels per figure point, `dpi / 72`.
    pub scale: f32,
    /// The screen position of the figure's top-left corner.
    pub origin: egui::Pos2,
}

impl ScreenTransform {
    /// Maps a figure-space point to screen space.
    #[must_use]
    pub fn apply(&self, p: ironlab_scene::display::Point) -> egui::Pos2 {
        egui::pos2(
            self.origin.x + self.scale * p.x as f32,
            self.origin.y + self.scale * p.y as f32,
        )
    }

    /// Maps a screen-space position back to figure space.
    #[must_use]
    pub fn invert(&self, p: egui::Pos2) -> ironlab_scene::display::Point {
        ironlab_scene::display::Point::new(
            f64::from((p.x - self.origin.x) / self.scale),
            f64::from((p.y - self.origin.y) / self.scale),
        )
    }
}

/// The largest distance, in screen units, between a curve and the polyline that approximates it.
const SCREEN_TOLERANCE: f64 = 0.05;

/// The miter limit of the display list's miter joins.
const MITER_LIMIT: f32 = 4.0;

/// The largest number of dash intervals drawn along one subpath; longer patterns are drawn solid, because at that
/// density the dashes are indistinguishable from a solid line and would only cost memory.
const MAX_DASHES_PER_SUBPATH: f64 = 100_000.0;

/// The largest side, in pixels, of an image tile that the providers of this crate hand out. Each caps the limit of
/// its graphics device at this, so that one upload never stalls a frame and both backends tile an image alike.
pub(crate) const MAX_TILE_SIDE: u32 = 8192;

/// Supplies the textures that image items are drawn with.
///
/// [`tessellate_with`] cuts every valid image item into tiles of at most [`max_side`](Self::max_side) pixels on a
/// side and asks for one texture per tile that is visible through the item's clip. What a provider does with the
/// request is its own affair: the [`TextureCache`] of the interactive canvas keeps textures across frames, the
/// offscreen renderer uploads each tile for one render, and a test can record what was asked for.
pub trait TextureProvider {
    /// The largest number of pixels a texture may have along either side, which is at least 1 for a provider that
    /// can supply textures. A provider that reports 0 has none to give, and image items are then skipped.
    fn max_side(&self) -> u32;

    /// Returns the texture holding tile `tile` of the image whose samples are `samples`.
    ///
    /// Tiles are numbered in row-major order, `row · columns + column`, over the tiling that
    /// [`max_side`](Self::max_side) implies. `render` builds the tile's pixels from the samples, and a provider that
    /// already holds a texture for this buffer and tile need not call it. A provider may key what it holds by the
    /// identity of the buffer, because a display list shares one buffer per image and never changes its contents.
    fn texture(
        &mut self,
        samples: &Arc<[u8]>,
        tile: u32,
        render: &mut dyn FnMut() -> egui::ColorImage,
    ) -> egui::TextureId;
}

/// The provider of [`tessellate`], which has no textures to give.
struct NoTextures;

impl TextureProvider for NoTextures {
    fn max_side(&self) -> u32 {
        0
    }

    fn texture(
        &mut self,
        _samples: &Arc<[u8]>,
        _tile: u32,
        _render: &mut dyn FnMut() -> egui::ColorImage,
    ) -> egui::TextureId {
        egui::TextureId::default()
    }
}

/// Tessellates every item of `list` into screen-space meshes, in paint order, drawing no image items.
///
/// The figure background is not included; callers paint it themselves (both the canvas and the offscreen renderer
/// paint it as a filled rectangle beneath these meshes). Items that produce no geometry (for example glyphs without
/// an outline, or paths entirely outside their clip) contribute no mesh. Invalid items, such as paths with
/// non-finite coordinates or strokes with a negative width, are skipped. Image items need textures to be drawn
/// with and are skipped here; a caller that draws them supplies a [`TextureProvider`] to [`tessellate_with`].
#[must_use]
pub fn tessellate(
    list: &DisplayList,
    text: &TextEngine,
    to_screen: ScreenTransform,
) -> Vec<egui::Mesh> {
    tessellate_with(list, text, to_screen, &mut NoTextures)
}

/// Tessellates every item of `list` into screen-space meshes, in paint order, drawing image items with textures
/// from `textures`.
///
/// Everything said of [`tessellate`] holds here too. An image item that is not [`ImageItem::is_valid`] is skipped
/// without a texture being requested; a valid one yields one mesh per tile of it that is visible through its clip,
/// each sampling the texture the provider returned for that tile, in the item's place in the paint order. Every
/// other mesh samples egui's default texture at [`egui::epaint::WHITE_UV`]. Unlike [`tessellate`], a call has an
/// effect beyond its result: the provider is asked for a texture for every visible tile, and may upload or record
/// what it is asked for. The leaves of depth groups, and leaves that carry a depth, contribute nothing: they are
/// drawn only through [`drawables_with`].
pub fn tessellate_with(
    list: &DisplayList,
    text: &TextEngine,
    to_screen: ScreenTransform,
    textures: &mut dyn TextureProvider,
) -> Vec<egui::Mesh> {
    drawables_with(list, text, to_screen, textures)
        .into_iter()
        .filter_map(|drawable| match drawable {
            Drawable::Mesh(mesh) => Some(mesh),
            Drawable::Gpu(_) => None,
        })
        .collect()
}

/// Tessellates every item of `list` in paint order: egui meshes for the leaves outside depth groups that carry no
/// depth, exactly as [`tessellate_with`] makes them, and one [`DrawList`] for the depth-tested pipelines per depth
/// group, in its place, holding every leaf of the group with its depth. A run of consecutive leaves outside any
/// group that carry a depth becomes one list drawn without the depth test. A path whose depth is not usable
/// ([`PathItem::is_valid_depth`]), a path or an image inside a depth group without a depth, and a glyph run inside
/// one are skipped. The provider is never asked for the tiles of an image drawn through a list.
pub fn drawables_with(
    list: &DisplayList,
    text: &TextEngine,
    to_screen: ScreenTransform,
    textures: &mut dyn TextureProvider,
) -> Vec<Drawable> {
    let mut out = Vec::new();
    if !(to_screen.scale.is_finite()
        && to_screen.scale > 0.0
        && to_screen.origin.x.is_finite()
        && to_screen.origin.y.is_finite())
    {
        return out;
    }
    let screen = Transform {
        a: f64::from(to_screen.scale),
        b: 0.0,
        c: 0.0,
        d: f64::from(to_screen.scale),
        e: f64::from(to_screen.origin.x),
        f: f64::from(to_screen.origin.y),
    };
    let mut tessellators = Tessellators::default();
    // The meshes of the leaf being visited: none or one for a path or a glyph run, one per visible tile for an image.
    let mut leaf = Vec::new();
    // The list being built for the current depth group (`Some(index)`), or for the current run of depth-carrying
    // leaves outside any group (`None`).
    let mut current: Option<(Option<usize>, ListBuilder)> = None;
    let flush = |current: &mut Option<(Option<usize>, ListBuilder)>, out: &mut Vec<Drawable>| {
        if let Some((_, builder)) = current.take()
            && let Some(list) = builder.finish()
        {
            out.push(Drawable::Gpu(list));
        }
    };
    list.visit_leaves_grouped(|item, transform, clip, group| {
        let Some(context) = LeafContext::new(transform, screen, clip) else {
            return;
        };
        let carries_depth = match &item.kind {
            ItemKind::Path(path) => path.depth.is_some(),
            ItemKind::Image(image) => image.depth.is_some(),
            _ => false,
        };
        let key = if group.is_some() {
            Some(group)
        } else if carries_depth {
            Some(None)
        } else {
            None
        };
        let Some(key) = key else {
            flush(&mut current, &mut out);
            match &item.kind {
                ItemKind::Path(path) => {
                    leaf.extend(tessellate_path(path, &context, &mut tessellators));
                }
                ItemKind::Glyphs(glyphs) => {
                    leaf.extend(tessellate_glyphs(glyphs, text, &context, &mut tessellators));
                }
                ItemKind::Image(image) => tessellate_image(image, &context, textures, &mut leaf),
                // Groups, dense and depth ones included, are descended into by the traversal and never reach this
                // point.
                ItemKind::Group { .. } | ItemKind::Dense { .. } | ItemKind::Depth { .. } => {}
            }
            for mut mesh in leaf.drain(..) {
                if let Some(clip) = context.clip {
                    mesh = clip_mesh(&mesh, clip);
                }
                if !mesh.indices.is_empty() {
                    out.push(Drawable::Mesh(mesh));
                }
            }
            return;
        };
        if current.as_ref().is_none_or(|(k, _)| *k != key) {
            flush(&mut current, &mut out);
            current = Some((key, ListBuilder::new(key.is_some())));
        }
        let builder = &mut current.as_mut().expect("a list was just started").1;
        match &item.kind {
            ItemKind::Path(path) => {
                tessellate_path_depth(path, &context, &mut tessellators, builder, item.source);
            }
            ItemKind::Image(image) => tessellate_image_depth(image, &context, builder, item.source),
            _ => {}
        }
    });
    flush(&mut current, &mut out);
    out
}

/// A [`DrawList`] under construction, with the depth of every vertex before normalisation.
struct ListBuilder {
    depth_test: bool,
    vertices: Vec<Vertex>,
    depths: Vec<f64>,
    indices: Vec<u32>,
    draws: Vec<Draw>,
}

impl ListBuilder {
    fn new(depth_test: bool) -> Self {
        Self {
            depth_test,
            vertices: Vec::new(),
            depths: Vec::new(),
            indices: Vec::new(),
            draws: Vec::new(),
        }
    }

    /// Adds one draw of `vertices` (each with its depth) and `indices` relative to them, unless a vertex is not
    /// finite.
    fn push(
        &mut self,
        vertices: impl IntoIterator<Item = (Vertex, f64)>,
        indices: impl IntoIterator<Item = u32>,
        texture: Option<TileKey>,
        clip: Option<egui::Rect>,
        source: Option<ironlab_ir::NodeId>,
    ) {
        let base = self.vertices.len() as u32;
        let first_index = self.indices.len() as u32;
        let mut added = 0usize;
        for (vertex, depth) in vertices {
            if !(vertex.pos[0].is_finite() && vertex.pos[1].is_finite() && depth.is_finite()) {
                self.vertices.truncate(base as usize);
                self.depths.truncate(base as usize);
                return;
            }
            self.vertices.push(vertex);
            self.depths.push(depth);
            added += 1;
        }
        let mut count = 0u32;
        for index in indices {
            if index as usize >= added {
                self.vertices.truncate(base as usize);
                self.depths.truncate(base as usize);
                self.indices.truncate(first_index as usize);
                return;
            }
            self.indices.push(base + index);
            count += 1;
        }
        if count == 0 {
            self.vertices.truncate(base as usize);
            self.depths.truncate(base as usize);
            return;
        }
        self.draws.push(Draw {
            indices: first_index..first_index + count,
            texture,
            depth_test: self.depth_test,
            clip,
            source,
        });
    }

    /// Normalises the depths into `z` and returns the list, or `None` when nothing was added.
    fn finish(mut self) -> Option<Arc<DrawList>> {
        if self.draws.is_empty() {
            return None;
        }
        let (min, max) = self
            .depths
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| {
                (lo.min(*d), hi.max(*d))
            });
        for (vertex, depth) in self.vertices.iter_mut().zip(&self.depths) {
            vertex.z = if max > min {
                ((max - depth) / (max - min)).clamp(0.0, 1.0) as f32
            } else {
                0.5
            };
        }
        Some(Arc::new(DrawList {
            vertices: self.vertices,
            indices: self.indices,
            draws: self.draws,
        }))
    }
}

/// Reusable lyon tessellators.
#[derive(Default)]
struct Tessellators {
    fill: FillTessellator,
    stroke: StrokeTessellator,
}

/// The mapping of one leaf item into screen space.
struct LeafContext {
    /// The composite transform from item space to screen space.
    to_screen: Transform,
    /// The largest factor by which `to_screen` stretches a length.
    max_stretch: f64,
    /// The geometric mean of the stretch of `to_screen` (the square root of its absolute determinant).
    mean_stretch: f64,
    /// The clip rectangle in screen space.
    clip: Option<egui::Rect>,
}

impl LeafContext {
    /// Returns `None` when the transform or clip is not finite, when the transform is degenerate, or when the clip is
    /// empty, in all of which cases the item draws nothing.
    fn new(transform: Transform, screen: Transform, clip: Option<Rect>) -> Option<Self> {
        let t = transform.then(screen);
        if ![t.a, t.b, t.c, t.d, t.e, t.f].iter().all(|v| v.is_finite()) {
            return None;
        }
        let det = (t.a * t.d - t.b * t.c).abs();
        let half_sum = (t.a * t.a + t.b * t.b + t.c * t.c + t.d * t.d) / 2.0;
        let max_stretch = (half_sum + (half_sum * half_sum - det * det).max(0.0).sqrt()).sqrt();
        if !(det.is_finite() && det > 0.0 && max_stretch.is_finite() && max_stretch > 0.0) {
            return None;
        }
        let clip = match clip {
            None => None,
            Some(c) => {
                if ![c.x, c.y, c.width, c.height].iter().all(|v| v.is_finite())
                    || c.width <= 0.0
                    || c.height <= 0.0
                {
                    return None;
                }
                let min = screen.apply(Point::new(c.x, c.y));
                let max = screen.apply(Point::new(c.right(), c.bottom()));
                Some(egui::Rect::from_min_max(
                    egui::pos2(min.x as f32, min.y as f32),
                    egui::pos2(max.x as f32, max.y as f32),
                ))
            }
        };
        Some(Self {
            to_screen: t,
            max_stretch,
            mean_stretch: det.sqrt(),
            clip,
        })
    }

    /// The flattening tolerance in item space that gives [`SCREEN_TOLERANCE`] on screen.
    fn local_tolerance(&self) -> f32 {
        ((SCREEN_TOLERANCE / self.max_stretch) as f32).max(1e-6)
    }

    fn apply(&self, x: f64, y: f64) -> egui::Pos2 {
        let p = self.to_screen.apply(Point::new(x, y));
        egui::pos2(p.x as f32, p.y as f32)
    }
}

/// Converts a straight-alpha display-list colour to a premultiplied egui colour, or `None` when a channel is not a
/// number.
pub(crate) fn color32(color: Rgba) -> Option<egui::Color32> {
    let channel = |c: f32| {
        if c.is_nan() {
            None
        } else {
            Some((c.clamp(0.0, 1.0) * 255.0).round() as u8)
        }
    };
    Some(egui::Color32::from_rgba_unmultiplied(
        channel(color.r)?,
        channel(color.g)?,
        channel(color.b)?,
        channel(color.a)?,
    ))
}

/// Appends lyon output (in item space) to `mesh`, mapped to screen space with a uniform colour, `position` giving
/// each vertex's item-space position. Returns `false` when a vertex does not map to a finite screen position.
fn append<V: Copy>(
    mesh: &mut egui::Mesh,
    buffers: &VertexBuffers<V, u32>,
    position: impl Fn(V) -> lyon::math::Point,
    color: egui::Color32,
    map: impl Fn(lyon::math::Point) -> egui::Pos2,
) -> bool {
    let base = mesh.vertices.len() as u32;
    for &v in &buffers.vertices {
        let pos = map(position(v));
        if !(pos.x.is_finite() && pos.y.is_finite()) {
            return false;
        }
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv: egui::epaint::WHITE_UV,
            color,
        });
    }
    mesh.indices
        .extend(buffers.indices.iter().map(|&i| base + i));
    true
}

fn is_finite_point(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn lyon_point(p: Point) -> lyon::math::Point {
    lyon::math::point(p.x as f32, p.y as f32)
}

/// One subpath of a display-list path, in item space, with the depth attribute of its start and of the end of each
/// segment (zero for a path without vertex depths, and for `Close`).
struct SubPath {
    start: Point,
    start_depth: f32,
    segments: Vec<PathSegment>,
    depths: Vec<f32>,
    closed: bool,
}

/// Splits display-list segments into subpaths, following PDF semantics: a segment after `Close` without a `MoveTo`
/// starts a new subpath at the start point of the closed one. `depths`, when given, are the depths of the path's
/// endpoints in order. Returns `None` when the path does not start with `MoveTo` or contains a non-finite
/// coordinate.
fn subpaths(segments: &[PathSegment], depths: Option<&[f64]>) -> Option<Vec<SubPath>> {
    if !matches!(segments.first(), Some(PathSegment::MoveTo(_))) {
        return None;
    }
    let mut result: Vec<SubPath> = Vec::new();
    let mut open = false;
    let mut endpoint = 0usize;
    let mut next_depth = || {
        let depth = depths
            .and_then(|d| d.get(endpoint))
            .map_or(0.0, |d| *d as f32);
        endpoint += 1;
        depth
    };
    for segment in segments {
        match *segment {
            PathSegment::MoveTo(p) => {
                if !is_finite_point(p) {
                    return None;
                }
                result.push(SubPath {
                    start: p,
                    start_depth: next_depth(),
                    segments: Vec::new(),
                    depths: Vec::new(),
                    closed: false,
                });
                open = true;
            }
            PathSegment::LineTo(p) | PathSegment::CubicTo(_, _, p) => {
                if let PathSegment::CubicTo(c1, c2, _) = *segment
                    && !(is_finite_point(c1) && is_finite_point(c2))
                {
                    return None;
                }
                if !is_finite_point(p) {
                    return None;
                }
                if !open {
                    let last = result.last()?;
                    let (start, start_depth) = (last.start, last.start_depth);
                    result.push(SubPath {
                        start,
                        start_depth,
                        segments: Vec::new(),
                        depths: Vec::new(),
                        closed: false,
                    });
                    open = true;
                }
                let depth = next_depth();
                let last = result.last_mut()?;
                last.segments.push(*segment);
                last.depths.push(depth);
            }
            PathSegment::Close => {
                if open {
                    result.last_mut()?.closed = true;
                    open = false;
                }
            }
        }
    }
    Some(result)
}

/// Builds a lyon path from subpaths, with the depth of every endpoint as its one custom attribute. When
/// `explicit_close` is set, a closed subpath is emitted as an open subpath that returns to its start, which lets path
/// measurement include the closing edge.
fn lyon_path<'a>(subpaths: impl IntoIterator<Item = &'a SubPath>, explicit_close: bool) -> Path {
    let mut builder = Path::builder_with_attributes(1);
    for sub in subpaths {
        builder.begin(lyon_point(sub.start), &[sub.start_depth]);
        for (segment, depth) in sub.segments.iter().zip(&sub.depths) {
            match *segment {
                PathSegment::LineTo(p) => {
                    builder.line_to(lyon_point(p), &[*depth]);
                }
                PathSegment::CubicTo(c1, c2, p) => {
                    builder.cubic_bezier_to(
                        lyon_point(c1),
                        lyon_point(c2),
                        lyon_point(p),
                        &[*depth],
                    );
                }
                PathSegment::MoveTo(_) | PathSegment::Close => {}
            }
        }
        if sub.closed && explicit_close {
            builder.line_to(lyon_point(sub.start), &[sub.start_depth]);
            builder.end(false);
        } else {
            builder.end(sub.closed);
        }
    }
    builder.build()
}

/// Appends every event of `path`, with its attributes, to `builder`.
fn append_events(builder: &mut lyon::path::path::BuilderWithAttributes, path: &Path) {
    use lyon::path::Event;
    for event in path.iter_with_attributes() {
        match event {
            Event::Begin {
                at: (p, attributes),
            } => {
                builder.begin(p, attributes);
            }
            Event::Line {
                to: (p, attributes),
                ..
            } => {
                builder.line_to(p, attributes);
            }
            Event::Quadratic {
                ctrl,
                to: (p, attributes),
                ..
            } => {
                builder.quadratic_bezier_to(ctrl, p, attributes);
            }
            Event::Cubic {
                ctrl1,
                ctrl2,
                to: (p, attributes),
                ..
            } => {
                builder.cubic_bezier_to(ctrl1, ctrl2, p, attributes);
            }
            Event::End { close, .. } => builder.end(close),
        }
    }
}

/// A vertex of tessellated path geometry: its position in item space and its depth attribute, interpolated by lyon
/// from the depths of the path's endpoints (zero for a path without vertex depths).
#[derive(Clone, Copy, Debug)]
struct DepthVertex {
    pos: lyon::math::Point,
    depth: f32,
}

/// The triangles of a path's fill and of its stroke, in item space, each with its colour.
struct Geometry {
    fill: Option<(egui::Color32, VertexBuffers<DepthVertex, u32>)>,
    stroke: Option<(egui::Color32, VertexBuffers<DepthVertex, u32>)>,
}

/// Tessellates a path in item space. Returns `None` when the path is invalid, and a geometry without a fill or a
/// stroke when that part has no colour or cannot be tessellated.
fn path_geometry(
    item: &PathItem,
    context: &LeafContext,
    tessellators: &mut Tessellators,
) -> Option<Geometry> {
    let depths = match &item.depth {
        Some(Depth::Vertices(depths)) => Some(depths.as_slice()),
        _ => None,
    };
    let subpaths = subpaths(&item.segments, depths)?;
    let tolerance = context.local_tolerance();
    let vertex = |pos: lyon::math::Point, depth: f32| DepthVertex { pos, depth };

    let mut fill = None;
    if let Some(f) = &item.fill
        && let Some(color) = color32(f.color)
    {
        let path = lyon_path(&subpaths, false);
        let rule = match f.rule {
            FillRule::NonZero => lyon::tessellation::FillRule::NonZero,
            FillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
        };
        let options = FillOptions::tolerance(tolerance).with_fill_rule(rule);
        let mut buffers: VertexBuffers<DepthVertex, u32> = VertexBuffers::new();
        let result = tessellators.fill.tessellate_path(
            &path,
            &options,
            &mut BuffersBuilder::new(&mut buffers, |mut v: FillVertex| {
                vertex(v.position(), v.interpolated_attributes()[0])
            }),
        );
        if result.is_ok() {
            fill = Some((color, buffers));
        }
    }

    let mut stroke = None;
    if let Some(s) = &item.stroke
        && let Some(color) = color32(s.color)
        && let Some(path) = stroke_path(&subpaths, s, tolerance)
    {
        let width = if s.width == 0.0 {
            // A zero width is the thinnest line the device can draw, as in PDF.
            (1.0 / context.mean_stretch) as f32
        } else {
            s.width as f32
        };
        let cap = match s.cap {
            LineCap::Butt => lyon::tessellation::LineCap::Butt,
            LineCap::Round => lyon::tessellation::LineCap::Round,
            LineCap::Square => lyon::tessellation::LineCap::Square,
        };
        let join = match s.join {
            LineJoin::Miter => lyon::tessellation::LineJoin::Miter,
            LineJoin::Round => lyon::tessellation::LineJoin::Round,
            LineJoin::Bevel => lyon::tessellation::LineJoin::Bevel,
        };
        let options = StrokeOptions::tolerance(tolerance)
            .with_line_width(width)
            .with_line_cap(cap)
            .with_line_join(join)
            .with_miter_limit(MITER_LIMIT);
        let mut buffers: VertexBuffers<DepthVertex, u32> = VertexBuffers::new();
        let result = tessellators.stroke.tessellate_path(
            &path,
            &options,
            &mut BuffersBuilder::new(&mut buffers, |mut v: StrokeVertex| {
                vertex(v.position(), v.interpolated_attributes()[0])
            }),
        );
        if result.is_ok() {
            stroke = Some((color, buffers));
        }
    }
    Some(Geometry { fill, stroke })
}

fn tessellate_path(
    item: &PathItem,
    context: &LeafContext,
    tessellators: &mut Tessellators,
) -> Option<egui::Mesh> {
    let geometry = path_geometry(item, context, tessellators)?;
    let map = |v: lyon::math::Point| context.apply(f64::from(v.x), f64::from(v.y));
    let mut mesh = egui::Mesh::default();
    for (color, buffers) in [geometry.fill, geometry.stroke].into_iter().flatten() {
        if !append(&mut mesh, &buffers, |v: DepthVertex| v.pos, color, map) {
            return None;
        }
    }
    Some(mesh)
}

/// Adds a path with a depth to a list as one draw, its fill before its stroke. A path without a usable depth adds
/// nothing.
fn tessellate_path_depth(
    item: &PathItem,
    context: &LeafContext,
    tessellators: &mut Tessellators,
    builder: &mut ListBuilder,
    source: Option<ironlab_ir::NodeId>,
) {
    let Some(depth) = &item.depth else { return };
    if !item.is_valid_depth() {
        return;
    }
    let Some(geometry) = path_geometry(item, context, tessellators) else {
        return;
    };
    let depth_of = |v: DepthVertex| match depth {
        Depth::Plane(plane) => plane.at(Point::new(f64::from(v.pos.x), f64::from(v.pos.y))),
        Depth::Vertices(_) => f64::from(v.depth),
    };
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (color, buffers) in [geometry.fill, geometry.stroke].into_iter().flatten() {
        let base = vertices.len() as u32;
        for &v in &buffers.vertices {
            let pos = context.apply(f64::from(v.pos.x), f64::from(v.pos.y));
            vertices.push((
                Vertex {
                    pos: [pos.x, pos.y],
                    z: 0.0,
                    uv: [0.0, 0.0],
                    color: color.to_array(),
                },
                depth_of(v),
            ));
        }
        indices.extend(buffers.indices.iter().map(|&i| base + i));
    }
    builder.push(vertices, indices, None, context.clip, source);
}

/// Returns the path to stroke: the path itself for a solid stroke, or its "on" dash intervals. Returns `None` when
/// the stroke width, dash array or dash offset is invalid.
fn stroke_path(subpaths: &[SubPath], stroke: &Stroke, tolerance: f32) -> Option<Path> {
    if !(stroke.width.is_finite() && stroke.width >= 0.0) {
        return None;
    }
    if stroke.dash.is_empty() {
        return Some(lyon_path(subpaths, false));
    }
    if !(stroke.dash_offset.is_finite() && stroke.dash.iter().all(|d| d.is_finite() && *d >= 0.0)) {
        return None;
    }
    let period: f64 = stroke.dash.iter().sum();
    if !(period.is_finite() && period > 0.0) {
        return None;
    }
    // An odd-length dash array repeats with "on" and "off" swapped, so its effective pattern is the array twice.
    let pattern: Vec<f64> = if stroke.dash.len() % 2 == 1 {
        stroke.dash.iter().chain(&stroke.dash).copied().collect()
    } else {
        stroke.dash.clone()
    };
    let period = if pattern.len() == stroke.dash.len() {
        period
    } else {
        2.0 * period
    };

    let mut builder = Path::builder_with_attributes(1);
    for sub in subpaths {
        let path = lyon_path(std::iter::once(sub), true);
        let measurements = PathMeasurements::from_path(&path, tolerance);
        let length = f64::from(measurements.length());
        if length <= 0.0 {
            continue;
        }
        if length / period > MAX_DASHES_PER_SUBPATH {
            append_events(&mut builder, &lyon_path(std::iter::once(sub), false));
            continue;
        }
        // The sampler interpolates the depth attribute, so every dash piece keeps the depths along the path.
        let mut sampler =
            measurements.create_sampler_with_attributes(&path, &path, SampleType::Distance);

        // The dash phase restarts at the beginning of every subpath, as in PDF.
        let mut index = 0;
        let mut remaining = pattern[0];
        let mut phase = stroke.dash_offset.rem_euclid(period);
        while phase > 0.0 {
            if phase >= remaining {
                phase -= remaining;
                index = (index + 1) % pattern.len();
                remaining = pattern[index];
            } else {
                remaining -= phase;
                phase = 0.0;
            }
        }
        let mut position = 0.0;
        while position < length {
            let end = position + remaining;
            if index % 2 == 0 && end > position {
                sampler.split_range(position as f32..end.min(length) as f32, &mut builder);
            }
            position = end;
            index = (index + 1) % pattern.len();
            remaining = pattern[index];
        }
    }
    Some(builder.build())
}

/// The bucket of screen sizes that share one cached glyph tessellation: a glyph whose em spans `s` screen units uses
/// the tessellation made for `2^ceil(log2 s)` units, which is at least as fine as needed.
fn size_bucket(em_in_screen_units: f64) -> i32 {
    em_in_screen_units.log2().ceil().clamp(-8.0, 16.0) as i32
}

type GlyphKey = (FontId, u16, i32);

/// Triangles in em space (y down, origin at the glyph origin) produced by lyon.
type GlyphTriangles = VertexBuffers<lyon::math::Point, u32>;

/// Em-space glyph tessellations, shared across calls because labels reuse the same few glyphs at the same sizes.
static GLYPH_CACHE: LazyLock<Mutex<HashMap<GlyphKey, Arc<GlyphTriangles>>>> =
    LazyLock::new(Mutex::default);

/// The number of cached glyph tessellations beyond which the cache is cleared.
const GLYPH_CACHE_LIMIT: usize = 20_000;

fn glyph_tessellation(
    text: &TextEngine,
    font: FontId,
    glyph: u16,
    bucket: i32,
    tessellators: &mut Tessellators,
) -> Option<Arc<GlyphTriangles>> {
    let key = (font, glyph, bucket);
    if let Some(cached) = GLYPH_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
    {
        return Some(Arc::clone(cached));
    }
    let outline = text.glyph_outline(font, glyph)?;
    let mut builder = Path::builder();
    let mut open = false;
    for element in outline.elements() {
        let p = |q: kurbo::Point| lyon::math::point(q.x as f32, q.y as f32);
        match *element {
            PathEl::MoveTo(q) => {
                if open {
                    builder.end(true);
                }
                builder.begin(p(q));
                open = true;
            }
            PathEl::LineTo(q) if open => {
                builder.line_to(p(q));
            }
            PathEl::QuadTo(c, q) if open => {
                builder.quadratic_bezier_to(p(c), p(q));
            }
            PathEl::CurveTo(c1, c2, q) if open => {
                builder.cubic_bezier_to(p(c1), p(c2), p(q));
            }
            PathEl::ClosePath if open => {
                builder.end(true);
                open = false;
            }
            _ => {}
        }
    }
    if open {
        builder.end(true);
    }
    let path = builder.build();
    let tolerance = (SCREEN_TOLERANCE / 2f64.powi(bucket)) as f32;
    let mut buffers: VertexBuffers<lyon::math::Point, u32> = VertexBuffers::new();
    tessellators
        .fill
        .tessellate_path(
            &path,
            &FillOptions::non_zero().with_tolerance(tolerance),
            &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position()),
        )
        .ok()?;
    let buffers = Arc::new(buffers);
    let mut cache = GLYPH_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if cache.len() >= GLYPH_CACHE_LIMIT {
        cache.clear();
    }
    cache.insert(key, Arc::clone(&buffers));
    Some(buffers)
}

fn tessellate_glyphs(
    item: &GlyphsItem,
    text: &TextEngine,
    context: &LeafContext,
    tessellators: &mut Tessellators,
) -> Option<egui::Mesh> {
    if !(item.size_pt.is_finite() && item.size_pt > 0.0) {
        return None;
    }
    let color = color32(item.color)?;
    let em = item.size_pt * context.max_stretch;
    if !em.is_finite() {
        return None;
    }
    let bucket = size_bucket(em);
    let size = item.size_pt;
    let mut mesh = egui::Mesh::default();
    for glyph in &item.glyphs {
        if !(glyph.x.is_finite() && glyph.y.is_finite()) {
            continue;
        }
        let Some(buffers) = glyph_tessellation(text, item.font, glyph.id, bucket, tessellators)
        else {
            continue;
        };
        let (x, y) = (glyph.x, glyph.y);
        let map = |v: lyon::math::Point| {
            context.apply(x + size * f64::from(v.x), y + size * f64::from(v.y))
        };
        if !append(&mut mesh, &buffers, |p| p, color, map) {
            return None;
        }
    }
    Some(mesh)
}

/// Tessellates an image item into one textured quad per visible tile, appending the meshes to `out`.
///
/// The item is cut into tiles of at most the provider's largest side, numbered in row-major order. Each tile's quad
/// has its four corners at the tile's sub-rectangle of the item rectangle mapped through the leaf context, so a
/// sheared placement gives a parallelogram; the texture coordinates (0, 0), (1, 0), (0, 1) and (1, 1) sit at its
/// top-left, top-right, bottom-left and bottom-right corners, and its vertices are white, so that the texture is
/// drawn unmodulated. The boundary between two pixel columns or rows is computed from its integer index alone, so
/// the tiles either side of it share bit-equal screen edges. A tile whose quad lies wholly outside the clip is
/// neither requested from the provider nor drawn, and an item with more tiles than a `u32` can number, which no
/// provider of this crate produces, is not drawn.
fn tessellate_image(
    item: &ImageItem,
    context: &LeafContext,
    textures: &mut dyn TextureProvider,
    out: &mut Vec<egui::Mesh>,
) {
    let max_side = textures.max_side();
    if max_side == 0 || !item.is_valid() {
        return;
    }
    let columns = item.width.div_ceil(max_side);
    let rows = item.height.div_ceil(max_side);
    let Ok(tiles) = u32::try_from(u64::from(rows) * u64::from(columns)) else {
        return;
    };
    let rect = item.rect;
    let x_at = |column: u32| rect.x + rect.width * (f64::from(column) / f64::from(item.width));
    let y_at = |row: u32| rect.y + rect.height * (f64::from(row) / f64::from(item.height));
    for tile in 0..tiles {
        let (row, column) = (tile / columns, tile % columns);
        let (c0, r0) = (column * max_side, row * max_side);
        let c1 = c0.saturating_add(max_side).min(item.width);
        let r1 = r0.saturating_add(max_side).min(item.height);
        let (left, right, top, bottom) = (x_at(c0), x_at(c1), y_at(r0), y_at(r1));
        let corners = [
            (context.apply(left, top), egui::pos2(0.0, 0.0)),
            (context.apply(right, top), egui::pos2(1.0, 0.0)),
            (context.apply(left, bottom), egui::pos2(0.0, 1.0)),
            (context.apply(right, bottom), egui::pos2(1.0, 1.0)),
        ];
        if !corners
            .iter()
            .all(|(pos, _)| pos.x.is_finite() && pos.y.is_finite())
        {
            continue;
        }
        if let Some(clip) = context.clip
            && !quad_is_visible(&corners, clip)
        {
            continue;
        }
        let mut render = || tile_image(item, (c0, c1), (r0, r1));
        let texture = textures.texture(&item.samples, tile, &mut render);
        let mut mesh = egui::Mesh::with_texture(texture);
        mesh.vertices
            .extend(corners.iter().map(|&(pos, uv)| egui::epaint::Vertex {
                pos,
                uv,
                color: egui::Color32::WHITE,
            }));
        mesh.indices.extend([0, 1, 2, 2, 1, 3]);
        out.push(mesh);
    }
}

/// Adds an image with a depth plane to a list as one draw per visible tile, the tiles cut at [`MAX_TILE_SIDE`] and
/// numbered as [`tessellate_image`] numbers them, each a white textured quad whose four corners take the plane at
/// their pixel-space positions. An image without a depth, or an invalid one, adds nothing.
fn tessellate_image_depth(
    item: &ImageItem,
    context: &LeafContext,
    builder: &mut ListBuilder,
    source: Option<ironlab_ir::NodeId>,
) {
    let Some(plane) = item.depth else { return };
    if !item.is_valid() {
        return;
    }
    let columns = item.width.div_ceil(MAX_TILE_SIDE);
    let rows = item.height.div_ceil(MAX_TILE_SIDE);
    let Ok(tiles) = u32::try_from(u64::from(rows) * u64::from(columns)) else {
        return;
    };
    let rect = item.rect;
    let x_at = |column: u32| rect.x + rect.width * (f64::from(column) / f64::from(item.width));
    let y_at = |row: u32| rect.y + rect.height * (f64::from(row) / f64::from(item.height));
    for tile in 0..tiles {
        let (row, column) = (tile / columns, tile % columns);
        let (c0, r0) = (column * MAX_TILE_SIDE, row * MAX_TILE_SIDE);
        let c1 = c0.saturating_add(MAX_TILE_SIDE).min(item.width);
        let r1 = r0.saturating_add(MAX_TILE_SIDE).min(item.height);
        let (left, right, top, bottom) = (x_at(c0), x_at(c1), y_at(r0), y_at(r1));
        let corners = [
            (context.apply(left, top), egui::pos2(0.0, 0.0)),
            (context.apply(right, top), egui::pos2(1.0, 0.0)),
            (context.apply(left, bottom), egui::pos2(0.0, 1.0)),
            (context.apply(right, bottom), egui::pos2(1.0, 1.0)),
        ];
        if !corners
            .iter()
            .all(|(pos, _)| pos.x.is_finite() && pos.y.is_finite())
        {
            continue;
        }
        if let Some(clip) = context.clip
            && !quad_is_visible(&corners, clip)
        {
            continue;
        }
        let pixel_corners = [
            Point::new(f64::from(c0), f64::from(r0)),
            Point::new(f64::from(c1), f64::from(r0)),
            Point::new(f64::from(c0), f64::from(r1)),
            Point::new(f64::from(c1), f64::from(r1)),
        ];
        let vertices = corners
            .iter()
            .zip(pixel_corners)
            .map(|(&(pos, uv), pixel)| {
                (
                    Vertex {
                        pos: [pos.x, pos.y],
                        z: 0.0,
                        uv: [uv.x, uv.y],
                        color: [255, 255, 255, 255],
                    },
                    plane.at(pixel),
                )
            });
        let key = TileKey {
            samples: Arc::clone(&item.samples),
            width: item.width,
            channels: item.channels,
            columns: c0..c1,
            rows: r0..r1,
        };
        builder.push(
            vertices,
            [0, 1, 2, 2, 1, 3],
            Some(key),
            context.clip,
            source,
        );
    }
}

/// The pixels of columns `c0..c1` and rows `r0..r1` of an image item as an egui image: opaque texels for three
/// channels, and texels premultiplied from straight alpha for four.
fn tile_image(item: &ImageItem, (c0, c1): (u32, u32), (r0, r1): (u32, u32)) -> egui::ColorImage {
    let channels = usize::from(item.channels);
    let width = item.width as usize;
    let (c0, c1, r0, r1) = (c0 as usize, c1 as usize, r0 as usize, r1 as usize);
    // The rows of a tile spanning every column are one contiguous run of the samples.
    let bytes: Cow<'_, [u8]> = if c0 == 0 && c1 == width {
        Cow::Borrowed(&item.samples[r0 * width * channels..r1 * width * channels])
    } else {
        Cow::Owned(
            (r0..r1)
                .flat_map(|row| {
                    let start = (row * width + c0) * channels;
                    item.samples[start..start + (c1 - c0) * channels]
                        .iter()
                        .copied()
                })
                .collect(),
        )
    };
    let size = [c1 - c0, r1 - r0];
    if item.channels == ImageItem::RGB {
        egui::ColorImage::from_rgb(size, &bytes)
    } else {
        egui::ColorImage::from_rgba_unmultiplied(size, &bytes)
    }
}

/// Reports whether a quad given by its top-left, top-right, bottom-left and bottom-right corners has a part of
/// positive area inside `clip`.
fn quad_is_visible(corners: &[TexturedPoint; 4], clip: egui::Rect) -> bool {
    let polygon = clip_polygon(vec![corners[0], corners[1], corners[3], corners[2]], clip);
    polygon.len() >= 3 && polygon_area(&polygon) > 0.0
}

/// The unsigned area of a polygon given by its vertices in perimeter order.
fn polygon_area(polygon: &[TexturedPoint]) -> f32 {
    let twice: f32 = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|((a, _), (b, _))| a.x * b.y - b.x * a.y)
        .sum();
    twice.abs() / 2.0
}

/// A screen position with the texture coordinate its vertex carries.
type TexturedPoint = (egui::Pos2, egui::Pos2);

/// Clips every triangle of `mesh` against `clip` with the Sutherland–Hodgman algorithm, re-triangulating clipped
/// triangles as fans. Triangles entirely inside keep their vertices; triangles entirely outside are dropped. A
/// vertex made on the clip boundary carries the texture coordinate interpolated with its position. The vertices of
/// a triangle share one colour in every mesh this module builds, so a clipped triangle keeps the colour of its
/// first vertex.
fn clip_mesh(mesh: &egui::Mesh, clip: egui::Rect) -> egui::Mesh {
    let mut out = egui::Mesh {
        texture_id: mesh.texture_id,
        ..Default::default()
    };
    // Maps an input vertex index to its output index, for vertices of triangles kept whole.
    let mut remap: Vec<u32> = vec![u32::MAX; mesh.vertices.len()];
    let inside = |p: egui::Pos2| {
        p.x >= clip.min.x && p.x <= clip.max.x && p.y >= clip.min.y && p.y <= clip.max.y
    };
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let vertices = [
            mesh.vertices[triangle[0] as usize],
            mesh.vertices[triangle[1] as usize],
            mesh.vertices[triangle[2] as usize],
        ];
        if vertices.iter().all(|v| inside(v.pos)) {
            for &i in triangle {
                if remap[i as usize] == u32::MAX {
                    remap[i as usize] = out.vertices.len() as u32;
                    out.vertices.push(mesh.vertices[i as usize]);
                }
                out.indices.push(remap[i as usize]);
            }
            continue;
        }
        let xs = vertices.map(|v| v.pos.x);
        let ys = vertices.map(|v| v.pos.y);
        let below = |values: [f32; 3], limit: f32| values.iter().all(|&v| v < limit);
        let above = |values: [f32; 3], limit: f32| values.iter().all(|&v| v > limit);
        if below(xs, clip.min.x)
            || above(xs, clip.max.x)
            || below(ys, clip.min.y)
            || above(ys, clip.max.y)
        {
            continue;
        }
        let polygon = clip_polygon(vertices.iter().map(|v| (v.pos, v.uv)).collect(), clip);
        if polygon.len() < 3 {
            continue;
        }
        let color = vertices[0].color;
        let base = out.vertices.len() as u32;
        out.vertices
            .extend(
                polygon
                    .iter()
                    .map(|&(pos, uv)| egui::epaint::Vertex { pos, uv, color }),
            );
        for k in 1..polygon.len() as u32 - 1 {
            out.indices.extend([base, base + k, base + k + 1]);
        }
    }
    out
}

/// Clips a convex polygon against an axis-aligned rectangle, interpolating the texture coordinate of every vertex
/// made on the rectangle's boundary with its position.
fn clip_polygon(mut polygon: Vec<TexturedPoint>, clip: egui::Rect) -> Vec<TexturedPoint> {
    // Each edge is described by a signed distance that is non-negative inside.
    let edges: [&dyn Fn(egui::Pos2) -> f32; 4] = [
        &|p| p.x - clip.min.x,
        &|p| clip.max.x - p.x,
        &|p| p.y - clip.min.y,
        &|p| clip.max.y - p.y,
    ];
    for distance in edges {
        if polygon.is_empty() {
            break;
        }
        let input = std::mem::take(&mut polygon);
        for (i, &current) in input.iter().enumerate() {
            let previous = input[(i + input.len() - 1) % input.len()];
            let (dc, dp) = (distance(current.0), distance(previous.0));
            if dc >= 0.0 {
                if dp < 0.0 {
                    polygon.push(intersection(previous, current, dp, dc));
                }
                polygon.push(current);
            } else if dp >= 0.0 {
                polygon.push(intersection(previous, current, dp, dc));
            }
        }
    }
    polygon
}

/// The point on the segment from `a` to `b` at which the signed distance passes from `da` to `db` through zero,
/// with its texture coordinate taken at the same parameter.
fn intersection(a: TexturedPoint, b: TexturedPoint, da: f32, db: f32) -> TexturedPoint {
    let t = da / (da - db);
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// One texture held by a [`TextureCache`].
struct CachedTexture {
    /// A clone of the sample buffer the texture was made from, held so that the allocator cannot give the buffer's
    /// address to another buffer while the texture is cached: a figure whose image data is replaced by an array of
    /// the same size could otherwise be drawn with the old pixels.
    _samples: Arc<[u8]>,
    /// The texture, which is freed from the context when the handle is dropped.
    handle: egui::TextureHandle,
    /// Whether the texture has been requested since the previous [`TextureCache::retain_requested`].
    requested: bool,
}

/// The texture provider of the interactive canvas: a cache of the textures of the images on screen, keyed by the
/// address of their sample buffer and their tile, that keeps a texture across frames for as long as the rebuilt
/// meshes keep asking for it.
///
/// The samples of an image do not change between frames, only the view does, so the cache spares the graphics
/// device an upload per gesture: a request for a buffer and tile it holds is answered without rendering the pixels
/// again. A request it does not hold renders the tile and loads it into the context with
/// [`egui::TextureOptions::NEAREST`], so that the pixel edges are hard on screen as they are offscreen and in a PDF.
/// After every rebuild of the meshes, [`retain_requested`](Self::retain_requested) frees the textures the rebuild did
/// not ask for, so that a figure edited to hold other data leaves no textures behind and a pan through a large
/// figure does not let the device's memory grow without bound. Each entry holds a clone of its sample buffer for as
/// long as it exists, so that the address the cache keys by cannot be reused by another buffer in the meantime.
///
/// The largest side of a tile is the limit the backend reports through the context's input, capped at
/// [`MAX_TILE_SIDE`], and is read when it is asked for, so a limit set by a later pass is honoured.
pub struct TextureCache {
    ctx: egui::Context,
    entries: HashMap<(usize, u32), CachedTexture>,
}

impl TextureCache {
    /// Creates an empty cache that loads its textures into `ctx`.
    #[must_use]
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            ctx,
            entries: HashMap::new(),
        }
    }

    /// Frees every texture that has not been requested since the previous call (or, for the first call, since the
    /// cache was made), and starts a new round of requests. The interactive canvas calls this after each rebuild of
    /// its meshes, so that the cache holds exactly the textures the meshes on screen sample.
    pub fn retain_requested(&mut self) {
        self.entries
            .retain(|_, entry| std::mem::replace(&mut entry.requested, false));
    }
}

impl TextureProvider for TextureCache {
    fn max_side(&self) -> u32 {
        let limit = self.ctx.input(|input| input.max_texture_side);
        u32::try_from(limit).map_or(MAX_TILE_SIDE, |limit| limit.min(MAX_TILE_SIDE))
    }

    fn texture(
        &mut self,
        samples: &Arc<[u8]>,
        tile: u32,
        render: &mut dyn FnMut() -> egui::ColorImage,
    ) -> egui::TextureId {
        let address = Arc::as_ptr(samples).cast::<u8>().addr();
        let entry = self
            .entries
            .entry((address, tile))
            .or_insert_with(|| CachedTexture {
                _samples: Arc::clone(samples),
                handle: self.ctx.load_texture(
                    format!("ironlab image {address:#x} tile {tile}"),
                    render(),
                    egui::TextureOptions::NEAREST,
                ),
                requested: false,
            });
        entry.requested = true;
        entry.handle.id()
    }
}
