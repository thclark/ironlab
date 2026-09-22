//! Conversion of a display list into one [`DrawList`] for the pipelines of [`crate::gpu`].
//!
//! # Approach
//!
//! Every leaf of the display list is visited with [`DisplayList::visit_leaves_grouped`], which supplies the
//! accumulated group transform, the effective clip rectangle in figure space and the enclosing depth group. Fills,
//! glyphs and images become triangles in figure points, with the group transform applied and nothing else; strokes
//! become segments in the item space of their path, with the group transform carried in the draw's parameters. The
//! mapping from figure points to the screen is a uniform of the painter, so that a list built once serves every
//! placement of the figure on the target.
//!
//! - **Fills** are converted to a lyon path in item space and tessellated with lyon's `FillTessellator`, honouring
//!   the display list's [`FillRule`]. Curves are flattened to within [`SCREEN_TOLERANCE`] screen units at the
//!   [`Resolution`] the list is built for.
//! - **Strokes** are not tessellated. Each subpath is flattened into a polyline (a `CubicTo` within the same
//!   tolerance) and every edge of it becomes one [`Segment`], carrying its neighbours, the arc length along the
//!   subpath at its ends and whether a join or a cap is drawn at each end; the width, cap, join, dash pattern,
//!   colour and item-to-figure transform go into the draw's [`StrokeParams`]. The stroke pipeline expands the
//!   segments into the body, joins and caps of the stroke and dashes it by arc length, so the geometry uploaded
//!   for a polyline is one segment per edge and a resize or a pan re-uploads nothing.
//! - **Glyph runs** use [`TextEngine::glyph_outline`], which is em-normalised with y pointing down. Each outline is
//!   tessellated once per `(font, glyph, size bucket)` and cached, scaled by the run's `size_pt` and translated to
//!   the glyph origin.
//! - **Images** are drawn as textured quads. A valid image item is cut into tiles of at most
//!   [`Resolution::max_tile_side`] pixels on a side, numbered in row-major order, because a graphics device has a
//!   largest texture side and a data image can exceed it. Each tile is one quad: four white vertices at the corners
//!   of its sub-rectangle of the item rectangle, mapped through the same transforms as every other leaf (a
//!   parallelogram where a 3D placement shears), carrying the texture coordinates (0, 0), (1, 0), (0, 1) and (1, 1),
//!   so that the texture is drawn unmodulated, and a [`TileKey`] naming the pixels the painter uploads. Every tile
//!   edge is computed from its integer pixel coordinate through the same affine chain, so abutting tiles share
//!   bit-equal edges and show no seam. A tile whose bounding box lies wholly outside the leaf's clip is not drawn.
//! - **Clips** are recorded, not applied: every draw of a clipped leaf carries the leaf's clip in figure points, and
//!   the painter cuts the draw at the scissor rectangle of it. Nothing is clipped geometrically.
//! - **Colours** are straight alpha in the display list and are converted to premultiplied sRGB bytes as
//!   [`egui::Color32::from_rgba_unmultiplied`] converts them.
//! - **The background** of the list is not in the draw list: it is the colour of the page beneath every item, so
//!   the interactive canvas fills the figure's rectangle with it and the offscreen renderer clears its target to
//!   it, which covers every pixel of an image whose size rounds up from the page's.
//!
//! # Depth groups
//!
//! The leaves of an [`ItemKind::Depth`] group carry the group's number in paint order and the depth of every vertex
//! and segment end: a [`Depth::Plane`] evaluated at the item-space position, or a [`Depth::Vertices`] carried
//! through lyon as a custom attribute for fills and taken per endpoint for strokes (a point made by flattening a
//! curve interpolates the curve's end depths by its share of the flattened length); an image tile's corners take
//! its plane at their pixel-space positions. Depths are normalised to `[0, 1]` over the group, 0 the nearest. A
//! path in a group without a usable depth, an image in a group without a plane, and a glyph run in a group are
//! skipped. Outside every group the depth of an item is ignored and its vertices lie at `z = 0`; a stroke outside
//! every group instead takes a depth of its own from its params, decreasing with every such stroke since the list
//! or the last depth group began, which is how the painter keeps a stroke from blending with itself (see
//! [`crate::gpu`]).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use ironlab_scene::display::{
    Depth, DisplayList, FillRule, GlyphsItem, ImageItem, ItemKind, LineCap, LineJoin, PathItem,
    PathSegment, Point, Rect, Rgba, Stroke, Transform,
};
use ironlab_text::{FontId, TextEngine};
use kurbo::PathEl;
use lyon::path::Path;
use lyon::tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers};

use crate::gpu::{
    Draw, DrawKind, DrawList, JOIN_AT_END, JOIN_AT_START, MAX_DASH_ENTRIES, Segment, StrokeParams,
    TileKey, Vertex,
};

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
    pub fn apply(&self, p: Point) -> egui::Pos2 {
        egui::pos2(
            self.origin.x + self.scale * p.x as f32,
            self.origin.y + self.scale * p.y as f32,
        )
    }

    /// Maps a screen-space position back to figure space.
    #[must_use]
    pub fn invert(&self, p: egui::Pos2) -> Point {
        Point::new(
            f64::from((p.x - self.origin.x) / self.scale),
            f64::from((p.y - self.origin.y) / self.scale),
        )
    }
}

/// The resolution a draw list is built for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolution {
    /// Screen units per figure point: curves are flattened to within [`SCREEN_TOLERANCE`] screen units at this
    /// scale and glyphs take the cached tessellation of their size at it. A list built at one scale serves nearby
    /// scales; the canvas rebuilds when the scale has changed enough for the difference to show. A scale that is
    /// not finite and positive gives an empty list.
    pub scale: f32,
    /// The largest side of an image tile, in pixels, capped at [`MAX_TILE_SIDE`]; 0 draws no images.
    pub max_tile_side: u32,
}

impl Default for Resolution {
    fn default() -> Self {
        Self {
            scale: 1.0,
            max_tile_side: MAX_TILE_SIDE,
        }
    }
}

/// The largest distance, in screen units, between a curve and the polyline that approximates it.
pub const SCREEN_TOLERANCE: f64 = 0.05;

/// The largest side, in pixels, of an image tile, so that one upload never stalls a frame and every backend tiles
/// an image alike.
pub const MAX_TILE_SIDE: u32 = 8192;

/// The step in depth between consecutive strokes outside every depth group: the k-th such stroke since the list or
/// the last depth group began lies at `1 − (k + 1) · FLAT_STROKE_STEP`, so that a later stroke passes the depth test
/// over an earlier one while no stroke passes over itself.
const FLAT_STROKE_STEP: f32 = 1.0 / (1 << 20) as f32;

/// Tessellates every item of `list` into one draw list in figure points, in paint order, for `resolution`.
///
/// The background of the list is not included; the caller paints it beneath the list (see the module
/// documentation). Items that produce no geometry (for example glyphs without an outline) contribute no draw, and
/// invalid items, such as paths with non-finite coordinates or strokes with a negative width, are skipped while the
/// rest still draw.
#[must_use]
pub fn tessellate(list: &DisplayList, text: &TextEngine, resolution: Resolution) -> DrawList {
    let mut builder = ListBuilder::default();
    let scale = f64::from(resolution.scale);
    if !(scale.is_finite() && scale > 0.0) {
        return builder.finish();
    }
    let max_tile_side = resolution.max_tile_side.min(MAX_TILE_SIDE);
    let mut tessellator = FillTessellator::new();
    list.visit_leaves_grouped(|item, transform, clip, group| {
        builder.enter(group);
        let Some(context) = LeafContext::new(transform, scale, clip) else {
            return;
        };
        match &item.kind {
            ItemKind::Path(path) => {
                tessellate_path(path, &context, &mut tessellator, &mut builder, item.source);
            }
            ItemKind::Glyphs(glyphs) if group.is_none() => {
                tessellate_glyphs(
                    glyphs,
                    text,
                    &context,
                    &mut tessellator,
                    &mut builder,
                    item.source,
                );
            }
            ItemKind::Image(image) => {
                tessellate_image(image, &context, max_tile_side, &mut builder, item.source);
            }
            // A glyph run inside a depth group has no depth to draw at. Groups, dense and depth ones included, are
            // descended into by the traversal and never reach this point.
            ItemKind::Glyphs(_)
            | ItemKind::Group { .. }
            | ItemKind::Dense { .. }
            | ItemKind::Depth { .. } => {}
        }
    });
    builder.finish()
}

/// A [`DrawList`] under construction, with the depth of every vertex and segment end before normalisation.
#[derive(Default)]
struct ListBuilder {
    vertices: Vec<Vertex>,
    depths: Vec<f64>,
    indices: Vec<u32>,
    segments: Vec<Segment>,
    /// The depths at the two ends of each segment and the gradient of its depth over item space.
    segment_depths: Vec<(f64, f64, [f64; 2])>,
    stroke_params: Vec<StrokeParams>,
    draws: Vec<Draw>,
    /// The depth group being built with the indices of its first vertex and first segment, or `None` outside
    /// every group.
    group: Option<(u32, usize, usize)>,
    /// The number of strokes drawn outside every group since the list or the last group began.
    flat_strokes: u32,
}

impl ListBuilder {
    /// Moves to depth group `group`, closing the group being built when it is another.
    fn enter(&mut self, group: Option<usize>) {
        let group = group.map(|g| g as u32);
        if self.group.map(|(g, _, _)| g) == group {
            return;
        }
        self.close_group();
        self.flat_strokes = 0;
        if let Some(g) = group {
            self.group = Some((g, self.vertices.len(), self.segments.len()));
        }
    }

    /// Normalises the depths of the group being built into `z`, 0 the nearest, and leaves the group.
    fn close_group(&mut self) {
        let Some((_, vertex_start, segment_start)) = self.group.take() else {
            return;
        };
        let depths = &self.depths[vertex_start..];
        let segment_depths = &self.segment_depths[segment_start..];
        let (min, max) = depths
            .iter()
            .copied()
            .chain(segment_depths.iter().flat_map(|(a, b, _)| [*a, *b]))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| {
                (lo.min(d), hi.max(d))
            });
        let normalise = |depth: f64| {
            if max > min {
                ((max - depth) / (max - min)).clamp(0.0, 1.0) as f32
            } else {
                0.5
            }
        };
        // z falls as the depth rises, so the gradient of z is the negated gradient of the depth over the range.
        let slope = |g: f64| {
            if max > min {
                (-g / (max - min)) as f32
            } else {
                0.0
            }
        };
        for (vertex, depth) in self.vertices[vertex_start..].iter_mut().zip(depths) {
            vertex.z = normalise(*depth);
        }
        for (segment, (a, b, gradient)) in self.segments[segment_start..]
            .iter_mut()
            .zip(segment_depths)
        {
            segment.z = [normalise(*a), normalise(*b)];
            segment.grad = [slope(gradient[0]), slope(gradient[1])];
        }
    }

    /// Whether a leaf is being built inside a depth group.
    fn in_group(&self) -> bool {
        self.group.is_some()
    }

    fn depth_group(&self) -> Option<u32> {
        self.group.map(|(g, _, _)| g)
    }

    /// Adds one triangle draw of `vertices` (each with its depth) and `indices` relative to them, unless a vertex is
    /// not finite or the indices do not address the vertices.
    fn push(
        &mut self,
        vertices: impl IntoIterator<Item = (Vertex, f64)>,
        indices: impl IntoIterator<Item = u32>,
        texture: Option<TileKey>,
        clip: Option<Rect>,
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
            kind: DrawKind::Triangles(first_index..first_index + count),
            texture,
            depth_group: self.depth_group(),
            clip,
            source,
        });
    }

    /// Adds one stroke draw of `segments` (each with the depths at its ends) with `params`, unless there are none.
    /// Outside every group the draw takes the next depth of the flat strokes.
    fn push_stroke(
        &mut self,
        segments: Vec<(Segment, f64, f64)>,
        gradient: [f64; 2],
        mut params: StrokeParams,
        clip: Option<Rect>,
        source: Option<ironlab_ir::NodeId>,
    ) {
        if segments.is_empty() {
            return;
        }
        if self.in_group() {
            params.vertex_z = 1;
            params.z = 0.0;
        } else {
            params.vertex_z = 0;
            params.z = 1.0 - (self.flat_strokes + 1) as f32 * FLAT_STROKE_STEP;
            self.flat_strokes += 1;
        }
        let start = self.segments.len() as u32;
        for (segment, a, b) in segments {
            self.segments.push(segment);
            self.segment_depths.push((a, b, gradient));
        }
        let end = self.segments.len() as u32;
        let index = self.stroke_params.len() as u32;
        self.stroke_params.push(params);
        self.draws.push(Draw {
            kind: DrawKind::Stroke {
                segments: start..end,
                params: index,
            },
            texture: None,
            depth_group: self.depth_group(),
            clip,
            source,
        });
    }

    fn finish(mut self) -> DrawList {
        self.close_group();
        DrawList {
            vertices: self.vertices,
            indices: self.indices,
            segments: self.segments,
            stroke_params: self.stroke_params,
            draws: self.draws,
        }
    }
}

/// The mapping of one leaf item into figure space, and the resolution it is drawn at.
struct LeafContext {
    /// The transform from item space to figure space.
    to_figure: Transform,
    /// The largest factor by which `to_figure` stretches a length.
    max_stretch: f64,
    /// Screen units per figure point.
    scale: f64,
    /// The clip rectangle in figure space.
    clip: Option<Rect>,
}

impl LeafContext {
    /// Returns `None` when the transform or clip is not finite, when the transform is degenerate, or when the clip is
    /// empty, in all of which cases the item draws nothing.
    fn new(transform: Transform, scale: f64, clip: Option<Rect>) -> Option<Self> {
        let t = transform;
        if ![t.a, t.b, t.c, t.d, t.e, t.f].iter().all(|v| v.is_finite()) {
            return None;
        }
        let det = (t.a * t.d - t.b * t.c).abs();
        let half_sum = (t.a * t.a + t.b * t.b + t.c * t.c + t.d * t.d) / 2.0;
        let max_stretch = (half_sum + (half_sum * half_sum - det * det).max(0.0).sqrt()).sqrt();
        if !(det.is_finite() && det > 0.0 && max_stretch.is_finite() && max_stretch > 0.0) {
            return None;
        }
        if let Some(c) = clip
            && (![c.x, c.y, c.width, c.height].iter().all(|v| v.is_finite())
                || c.width <= 0.0
                || c.height <= 0.0)
        {
            return None;
        }
        Some(Self {
            to_figure: t,
            max_stretch,
            scale,
            clip,
        })
    }

    /// The flattening tolerance in item space that gives [`SCREEN_TOLERANCE`] on screen.
    fn local_tolerance(&self) -> f64 {
        (SCREEN_TOLERANCE / (self.max_stretch * self.scale)).max(1e-6)
    }

    /// The figure-space position of an item-space point.
    fn apply(&self, x: f64, y: f64) -> [f32; 2] {
        let p = self.to_figure.apply(Point::new(x, y));
        [p.x as f32, p.y as f32]
    }

    /// Whether a quad with the given figure-space corners can show through the clip: its bounding box overlaps the
    /// clip rectangle. Conservative for a sheared quad, which is drawn and cut by the scissor.
    fn may_show(&self, corners: &[[f32; 2]; 4]) -> bool {
        let Some(clip) = self.clip else {
            return true;
        };
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for corner in corners {
            let (x, y) = (f64::from(corner[0]), f64::from(corner[1]));
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        max_x > clip.x && min_x < clip.right() && max_y > clip.y && min_y < clip.bottom()
    }
}

/// Converts a straight-alpha display-list colour to premultiplied sRGB bytes, or `None` when a channel is not a
/// number.
pub(crate) fn premultiplied(color: Rgba) -> Option<[u8; 4]> {
    let channel = |c: f32| {
        if c.is_nan() {
            None
        } else {
            Some((c.clamp(0.0, 1.0) * 255.0).round() as u8)
        }
    };
    Some(
        egui::Color32::from_rgba_unmultiplied(
            channel(color.r)?,
            channel(color.g)?,
            channel(color.b)?,
            channel(color.a)?,
        )
        .to_array(),
    )
}

fn is_finite_point(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
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

/// Builds a lyon path from subpaths, with the depth of every endpoint as its one custom attribute.
fn lyon_path<'a>(subpaths: impl IntoIterator<Item = &'a SubPath>) -> Path {
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
        builder.end(sub.closed);
    }
    builder.build()
}

/// A vertex of tessellated fill geometry: its position in item space and its depth attribute, interpolated by lyon
/// from the depths of the path's endpoints (zero for a path without vertex depths).
#[derive(Clone, Copy, Debug)]
struct DepthVertex {
    pos: lyon::math::Point,
    depth: f32,
}

/// Tessellates the fill of a path in item space, or returns `None` when the path has no fill, its colour is not a
/// number or lyon cannot tessellate it.
fn fill_triangles(
    item: &PathItem,
    subpaths: &[SubPath],
    context: &LeafContext,
    tessellator: &mut FillTessellator,
) -> Option<([u8; 4], VertexBuffers<DepthVertex, u32>)> {
    let fill = item.fill.as_ref()?;
    let color = premultiplied(fill.color)?;
    let path = lyon_path(subpaths);
    let rule = match fill.rule {
        FillRule::NonZero => lyon::tessellation::FillRule::NonZero,
        FillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
    };
    let options = FillOptions::tolerance(context.local_tolerance() as f32).with_fill_rule(rule);
    let mut buffers: VertexBuffers<DepthVertex, u32> = VertexBuffers::new();
    tessellator
        .tessellate_path(
            &path,
            &options,
            &mut BuffersBuilder::new(&mut buffers, |mut v: FillVertex| DepthVertex {
                pos: v.position(),
                depth: v.interpolated_attributes()[0],
            }),
        )
        .ok()?;
    Some((color, buffers))
}

/// One vertex of a flattened subpath with its depth, before the depth is normalised.
#[derive(Clone, Copy)]
struct PolyPoint {
    p: Point,
    depth: f64,
}

/// Flattens a subpath into the distinct vertices of a polyline, each with its depth: `Depth::Plane` at the vertex,
/// the endpoint depth for `Depth::Vertices` (a vertex made by flattening a curve interpolates the curve's end
/// depths by its share of the flattened length), 0 without a depth. Consecutive coincident vertices are merged and
/// a closed subpath whose last vertex equals its first drops the duplicate.
fn flatten(sub: &SubPath, depth: Option<&Depth>, tolerance: f64) -> Vec<PolyPoint> {
    let depth_at = |p: Point, endpoint: f64| match depth {
        Some(Depth::Plane(plane)) => plane.at(p),
        Some(Depth::Vertices(_)) => endpoint,
        None => 0.0,
    };
    let mut points = vec![PolyPoint {
        p: sub.start,
        depth: depth_at(sub.start, f64::from(sub.start_depth)),
    }];
    let push = |points: &mut Vec<PolyPoint>, point: PolyPoint| {
        if points.last().is_none_or(|last| last.p != point.p) {
            points.push(point);
        }
    };
    for (segment, &end_depth) in sub.segments.iter().zip(&sub.depths) {
        let end_depth = f64::from(end_depth);
        match *segment {
            PathSegment::LineTo(p) => push(
                &mut points,
                PolyPoint {
                    p,
                    depth: depth_at(p, end_depth),
                },
            ),
            PathSegment::CubicTo(c1, c2, p) => {
                let from = points.last().copied().expect("the subpath has a start");
                let curve = [
                    PathEl::MoveTo(kurbo::Point::new(from.p.x, from.p.y)),
                    PathEl::CurveTo(
                        kurbo::Point::new(c1.x, c1.y),
                        kurbo::Point::new(c2.x, c2.y),
                        kurbo::Point::new(p.x, p.y),
                    ),
                ];
                let mut flattened: Vec<Point> = Vec::new();
                kurbo::flatten(curve, tolerance, |el| {
                    if let PathEl::LineTo(q) = el {
                        flattened.push(Point::new(q.x, q.y));
                    }
                });
                if flattened.last() != Some(&p) {
                    flattened.push(p);
                }
                // The depth along the curve is interpolated by arc length when it comes per endpoint.
                let mut lengths = Vec::with_capacity(flattened.len());
                let mut total = 0.0;
                let mut previous = from.p;
                for q in &flattened {
                    total += distance(*q, previous);
                    lengths.push(total);
                    previous = *q;
                }
                for (q, length) in flattened.iter().zip(lengths) {
                    let endpoint = if total > 0.0 {
                        from.depth + (end_depth - from.depth) * (length / total)
                    } else {
                        end_depth
                    };
                    push(
                        &mut points,
                        PolyPoint {
                            p: *q,
                            depth: depth_at(*q, endpoint),
                        },
                    );
                }
            }
            PathSegment::MoveTo(_) | PathSegment::Close => {}
        }
    }
    if sub.closed && points.len() > 1 && points.last().map(|last| last.p) == Some(points[0].p) {
        points.pop();
    }
    points
}

/// The segments of a stroke along the flattened subpaths, each with the depths at its ends.
fn stroke_segments(
    subpaths: &[SubPath],
    depth: Option<&Depth>,
    tolerance: f64,
) -> Vec<(Segment, f64, f64)> {
    let mut segments = Vec::new();
    for sub in subpaths {
        let points = flatten(sub, depth, tolerance);
        if points.len() < 2 {
            continue;
        }
        let n = points.len();
        let edges = if sub.closed { n } else { n - 1 };
        let perimeter: f64 = (0..edges)
            .map(|i| distance(points[i].p, points[(i + 1) % n].p))
            .sum();
        let mut arc = 0.0;
        for i in 0..edges {
            let a = points[i];
            let b = points[(i + 1) % n];
            let length = distance(a.p, b.p);
            // The joint at the seam of a closed subpath is the end of the closing segment as well as the start.
            let prev_arc = if sub.closed && i == 0 { perimeter } else { arc };
            let (prev, join_start) = if sub.closed {
                (points[(i + n - 1) % n].p, true)
            } else if i > 0 {
                (points[i - 1].p, true)
            } else {
                (a.p, false)
            };
            let (next, join_end) = if sub.closed {
                (points[(i + 2) % n].p, true)
            } else if i + 2 < n {
                (points[i + 2].p, true)
            } else {
                (b.p, false)
            };
            let flags =
                if join_start { JOIN_AT_START } else { 0 } | if join_end { JOIN_AT_END } else { 0 };
            let at = |p: Point| [p.x as f32, p.y as f32];
            segments.push((
                Segment {
                    prev: at(prev),
                    p0: at(a.p),
                    p1: at(b.p),
                    next: at(next),
                    z: [0.0, 0.0],
                    arc: [arc as f32, (arc + length) as f32],
                    grad: [0.0, 0.0],
                    flags,
                    prev_arc: prev_arc as f32,
                },
                a.depth,
                b.depth,
            ));
            arc += length;
        }
    }
    segments
}

/// The parameters of a stroke draw, or `None` when the stroke's colour, width or dash pattern is invalid.
fn stroke_params(stroke: &Stroke, transform: Transform) -> Option<StrokeParams> {
    if !(stroke.width.is_finite() && stroke.width >= 0.0) {
        return None;
    }
    let color = premultiplied(stroke.color)?;
    let mut params = StrokeParams {
        linear: [
            transform.a as f32,
            transform.b as f32,
            transform.c as f32,
            transform.d as f32,
        ],
        offset: [transform.e as f32, transform.f as f32, 0.0, 0.0],
        color: color.map(|c| f32::from(c) / 255.0),
        width: stroke.width as f32,
        cap: match stroke.cap {
            LineCap::Butt => 0,
            LineCap::Round => 1,
            LineCap::Square => 2,
        },
        join: match stroke.join {
            LineJoin::Miter => 0,
            LineJoin::Round => 1,
            LineJoin::Bevel => 2,
        },
        dash_count: 0,
        dash_offset: 0.0,
        period: 0.0,
        z: 0.0,
        vertex_z: 0,
        dashes: [0.0; MAX_DASH_ENTRIES],
    };
    if stroke.dash.is_empty() {
        return Some(params);
    }
    if !(stroke.dash_offset.is_finite() && stroke.dash.iter().all(|d| d.is_finite() && *d >= 0.0)) {
        return None;
    }
    // An odd-length dash array repeats with "on" and "off" swapped, so its effective pattern is the array twice.
    let pattern: Vec<f64> = if stroke.dash.len() % 2 == 1 {
        stroke.dash.iter().chain(&stroke.dash).copied().collect()
    } else {
        stroke.dash.clone()
    };
    let period: f64 = pattern.iter().sum();
    if !(period.is_finite() && period > 0.0) {
        return None;
    }
    if pattern.len() > MAX_DASH_ENTRIES {
        // Too intricate a pattern to carry; drawn solid.
        return Some(params);
    }
    for (slot, entry) in params.dashes.iter_mut().zip(&pattern) {
        *slot = *entry as f32;
    }
    params.dash_count = pattern.len() as u32;
    params.dash_offset = stroke.dash_offset.rem_euclid(period) as f32;
    params.period = period as f32;
    Some(params)
}

/// Adds a path to the list: its fill as one triangle draw, then its stroke as one stroke draw. Inside a depth group
/// a path without a usable depth adds nothing; outside one its depth is ignored.
fn tessellate_path(
    item: &PathItem,
    context: &LeafContext,
    tessellator: &mut FillTessellator,
    builder: &mut ListBuilder,
    source: Option<ironlab_ir::NodeId>,
) {
    let depth = if builder.in_group() {
        if !item.is_valid_depth() {
            return;
        }
        match &item.depth {
            Some(depth) => Some(depth),
            None => return,
        }
    } else {
        None
    };
    let depths = match depth {
        Some(Depth::Vertices(depths)) => Some(depths.as_slice()),
        _ => None,
    };
    let Some(subpaths) = subpaths(&item.segments, depths) else {
        return;
    };
    if let Some((color, buffers)) = fill_triangles(item, &subpaths, context, tessellator) {
        let depth_of = |v: DepthVertex| match depth {
            Some(Depth::Plane(plane)) => {
                plane.at(Point::new(f64::from(v.pos.x), f64::from(v.pos.y)))
            }
            Some(Depth::Vertices(_)) => f64::from(v.depth),
            None => 0.0,
        };
        let vertices = buffers.vertices.iter().map(|&v| {
            (
                Vertex {
                    pos: context.apply(f64::from(v.pos.x), f64::from(v.pos.y)),
                    z: 0.0,
                    uv: [0.0, 0.0],
                    color,
                },
                depth_of(v),
            )
        });
        builder.push(
            vertices,
            buffers.indices.iter().copied(),
            None,
            context.clip,
            source,
        );
    }
    if let Some(stroke) = &item.stroke
        && let Some(params) = stroke_params(stroke, context.to_figure)
    {
        let segments = stroke_segments(&subpaths, depth, context.local_tolerance());
        // Over a plane the depth of a vertex offset from the polyline follows the plane; other depths are known
        // at the polyline's vertices only.
        let gradient = match depth {
            Some(Depth::Plane(plane)) => [plane.a, plane.b],
            _ => [0.0, 0.0],
        };
        builder.push_stroke(segments, gradient, params, context.clip, source);
    }
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
    tessellator: &mut FillTessellator,
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
    tessellator
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

/// Adds a glyph run to the list as one draw of every glyph's triangles.
fn tessellate_glyphs(
    item: &GlyphsItem,
    text: &TextEngine,
    context: &LeafContext,
    tessellator: &mut FillTessellator,
    builder: &mut ListBuilder,
    source: Option<ironlab_ir::NodeId>,
) {
    if !(item.size_pt.is_finite() && item.size_pt > 0.0) {
        return;
    }
    let Some(color) = premultiplied(item.color) else {
        return;
    };
    let em = item.size_pt * context.max_stretch * context.scale;
    if !em.is_finite() {
        return;
    }
    let bucket = size_bucket(em);
    let size = item.size_pt;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for glyph in &item.glyphs {
        if !(glyph.x.is_finite() && glyph.y.is_finite()) {
            continue;
        }
        let Some(buffers) = glyph_tessellation(text, item.font, glyph.id, bucket, tessellator)
        else {
            continue;
        };
        let base = vertices.len() as u32;
        for v in &buffers.vertices {
            vertices.push((
                Vertex {
                    pos: context.apply(
                        glyph.x + size * f64::from(v.x),
                        glyph.y + size * f64::from(v.y),
                    ),
                    z: 0.0,
                    uv: [0.0, 0.0],
                    color,
                },
                0.0,
            ));
        }
        indices.extend(buffers.indices.iter().map(|&i| base + i));
    }
    builder.push(vertices, indices, None, context.clip, source);
}

/// Adds an image to the list as one draw per tile that may show through the clip.
///
/// The item is cut into tiles of at most `max_tile_side` pixels on a side, numbered in row-major order. Each tile's
/// quad has its four corners at the tile's sub-rectangle of the item rectangle mapped through the leaf context, so a
/// sheared placement gives a parallelogram; the texture coordinates (0, 0), (1, 0), (0, 1) and (1, 1) sit at its
/// top-left, top-right, bottom-left and bottom-right corners, and its vertices are white, so that the texture is
/// drawn unmodulated. The boundary between two pixel columns or rows is computed from its integer index alone, so
/// the tiles either side of it share bit-equal edges. Inside a depth group the corners take the item's plane at
/// their pixel-space positions, and an image without a plane adds nothing; outside one the plane is ignored. An
/// item with more tiles than a `u32` can number, which no tile side of this crate produces, is not drawn.
fn tessellate_image(
    item: &ImageItem,
    context: &LeafContext,
    max_tile_side: u32,
    builder: &mut ListBuilder,
    source: Option<ironlab_ir::NodeId>,
) {
    if max_tile_side == 0 || !item.is_valid() {
        return;
    }
    let plane = if builder.in_group() {
        match item.depth {
            Some(plane) => Some(plane),
            None => return,
        }
    } else {
        None
    };
    let columns = item.width.div_ceil(max_tile_side);
    let rows = item.height.div_ceil(max_tile_side);
    let Ok(tiles) = u32::try_from(u64::from(rows) * u64::from(columns)) else {
        return;
    };
    let rect = item.rect;
    let x_at = |column: u32| rect.x + rect.width * (f64::from(column) / f64::from(item.width));
    let y_at = |row: u32| rect.y + rect.height * (f64::from(row) / f64::from(item.height));
    for tile in 0..tiles {
        let (row, column) = (tile / columns, tile % columns);
        let (c0, r0) = (column * max_tile_side, row * max_tile_side);
        let c1 = c0.saturating_add(max_tile_side).min(item.width);
        let r1 = r0.saturating_add(max_tile_side).min(item.height);
        let (left, right, top, bottom) = (x_at(c0), x_at(c1), y_at(r0), y_at(r1));
        let corners = [
            context.apply(left, top),
            context.apply(right, top),
            context.apply(left, bottom),
            context.apply(right, bottom),
        ];
        if !corners
            .iter()
            .all(|pos| pos[0].is_finite() && pos[1].is_finite())
            || !context.may_show(&corners)
        {
            continue;
        }
        let pixel_corners = [
            Point::new(f64::from(c0), f64::from(r0)),
            Point::new(f64::from(c1), f64::from(r0)),
            Point::new(f64::from(c0), f64::from(r1)),
            Point::new(f64::from(c1), f64::from(r1)),
        ];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        let vertices = corners
            .iter()
            .zip(uvs)
            .zip(pixel_corners)
            .map(|((&pos, uv), pixel)| {
                (
                    Vertex {
                        pos,
                        z: 0.0,
                        uv,
                        color: [255, 255, 255, 255],
                    },
                    plane.map_or(0.0, |plane| plane.at(pixel)),
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
