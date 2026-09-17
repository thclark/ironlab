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
//! - **Clips** are applied geometrically: the tessellated triangles of a clipped leaf are each clipped against the
//!   clip rectangle (converted to screen space) with the Sutherland–Hodgman algorithm, and the resulting convex
//!   polygon is re-triangulated as a fan. This keeps the output a plain list of meshes, independent of the painter's
//!   clip rectangle, so that the same meshes can be drawn by the interactive canvas and by the offscreen renderer.
//! - **Colours** are straight alpha in the display list and are converted to premultiplied
//!   [`egui::Color32`] with [`egui::Color32::from_rgba_unmultiplied`] after conversion from `[0, 1]` to `[0, 255]`.
//!
//! All vertices use [`egui::epaint::WHITE_UV`] and the default texture, so the meshes are drawn as solid colour. The
//! meshes are not anti-aliased by egui; the viewer relies on 4× MSAA for smooth edges.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use ironlab_scene::display::{
    DisplayList, FillRule, GlyphsItem, ItemKind, LineCap, LineJoin, PathItem, PathSegment, Point,
    Rect, Rgba, Stroke, Transform,
};
use ironlab_text::{FontId, TextEngine};
use kurbo::PathEl;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};
use lyon_algorithms::measure::{PathMeasurements, SampleType};

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

/// Tessellates every item of `list` into screen-space meshes, in paint order.
///
/// The figure background is not included; callers paint it themselves (both the canvas and the offscreen renderer
/// paint it as a filled rectangle beneath these meshes). Items that produce no geometry (for example glyphs without
/// an outline, or paths entirely outside their clip) contribute no mesh. Invalid items, such as paths with
/// non-finite coordinates or strokes with a negative width, are skipped.
#[must_use]
pub fn tessellate(
    list: &DisplayList,
    text: &TextEngine,
    to_screen: ScreenTransform,
) -> Vec<egui::Mesh> {
    let mut meshes = Vec::new();
    if !(to_screen.scale.is_finite()
        && to_screen.scale > 0.0
        && to_screen.origin.x.is_finite()
        && to_screen.origin.y.is_finite())
    {
        return meshes;
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
    list.visit_leaves(|item, transform, clip| {
        let Some(context) = LeafContext::new(transform, screen, clip) else {
            return;
        };
        let mesh = match &item.kind {
            ItemKind::Path(path) => tessellate_path(path, &context, &mut tessellators),
            ItemKind::Glyphs(glyphs) => {
                tessellate_glyphs(glyphs, text, &context, &mut tessellators)
            }
            ItemKind::Group { .. } => None,
        };
        let Some(mut mesh) = mesh else {
            return;
        };
        if let Some(clip) = context.clip {
            mesh = clip_mesh(&mesh, clip);
        }
        if !mesh.indices.is_empty() {
            meshes.push(mesh);
        }
    });
    meshes
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

/// Appends lyon output (in item space) to `mesh`, mapped to screen space with a uniform colour. Returns `false` when
/// a vertex does not map to a finite screen position.
fn append(
    mesh: &mut egui::Mesh,
    buffers: &VertexBuffers<lyon::math::Point, u32>,
    color: egui::Color32,
    map: impl Fn(lyon::math::Point) -> egui::Pos2,
) -> bool {
    let base = mesh.vertices.len() as u32;
    for &v in &buffers.vertices {
        let pos = map(v);
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

/// One subpath of a display-list path, in item space.
struct SubPath {
    start: Point,
    segments: Vec<PathSegment>,
    closed: bool,
}

/// Splits display-list segments into subpaths, following PDF semantics: a segment after `Close` without a `MoveTo`
/// starts a new subpath at the start point of the closed one. Returns `None` when the path does not start with
/// `MoveTo` or contains a non-finite coordinate.
fn subpaths(segments: &[PathSegment]) -> Option<Vec<SubPath>> {
    if !matches!(segments.first(), Some(PathSegment::MoveTo(_))) {
        return None;
    }
    let mut result: Vec<SubPath> = Vec::new();
    let mut open = false;
    for segment in segments {
        match *segment {
            PathSegment::MoveTo(p) => {
                if !is_finite_point(p) {
                    return None;
                }
                result.push(SubPath {
                    start: p,
                    segments: Vec::new(),
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
                    let start = result.last()?.start;
                    result.push(SubPath {
                        start,
                        segments: Vec::new(),
                        closed: false,
                    });
                    open = true;
                }
                result.last_mut()?.segments.push(*segment);
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

/// Builds a lyon path from subpaths. When `explicit_close` is set, a closed subpath is emitted as an open subpath
/// that returns to its start, which lets path measurement include the closing edge.
fn lyon_path<'a>(subpaths: impl IntoIterator<Item = &'a SubPath>, explicit_close: bool) -> Path {
    let mut builder = Path::builder();
    for sub in subpaths {
        builder.begin(lyon_point(sub.start));
        for segment in &sub.segments {
            match *segment {
                PathSegment::LineTo(p) => {
                    builder.line_to(lyon_point(p));
                }
                PathSegment::CubicTo(c1, c2, p) => {
                    builder.cubic_bezier_to(lyon_point(c1), lyon_point(c2), lyon_point(p));
                }
                PathSegment::MoveTo(_) | PathSegment::Close => {}
            }
        }
        if sub.closed && explicit_close {
            builder.line_to(lyon_point(sub.start));
            builder.end(false);
        } else {
            builder.end(sub.closed);
        }
    }
    builder.build()
}

fn tessellate_path(
    item: &PathItem,
    context: &LeafContext,
    tessellators: &mut Tessellators,
) -> Option<egui::Mesh> {
    let subpaths = subpaths(&item.segments)?;
    let tolerance = context.local_tolerance();
    let map = |v: lyon::math::Point| context.apply(f64::from(v.x), f64::from(v.y));
    let mut mesh = egui::Mesh::default();

    if let Some(fill) = &item.fill
        && let Some(color) = color32(fill.color)
    {
        let path = lyon_path(&subpaths, false);
        let rule = match fill.rule {
            FillRule::NonZero => lyon::tessellation::FillRule::NonZero,
            FillRule::EvenOdd => lyon::tessellation::FillRule::EvenOdd,
        };
        let options = FillOptions::tolerance(tolerance).with_fill_rule(rule);
        let mut buffers: VertexBuffers<lyon::math::Point, u32> = VertexBuffers::new();
        let result = tessellators.fill.tessellate_path(
            &path,
            &options,
            &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position()),
        );
        if result.is_ok() && !append(&mut mesh, &buffers, color, map) {
            return None;
        }
    }

    if let Some(stroke) = &item.stroke
        && let Some(color) = color32(stroke.color)
        && let Some(path) = stroke_path(&subpaths, stroke, tolerance)
    {
        let width = if stroke.width == 0.0 {
            // A zero width is the thinnest line the device can draw, as in PDF.
            (1.0 / context.mean_stretch) as f32
        } else {
            stroke.width as f32
        };
        let cap = match stroke.cap {
            LineCap::Butt => lyon::tessellation::LineCap::Butt,
            LineCap::Round => lyon::tessellation::LineCap::Round,
            LineCap::Square => lyon::tessellation::LineCap::Square,
        };
        let join = match stroke.join {
            LineJoin::Miter => lyon::tessellation::LineJoin::Miter,
            LineJoin::Round => lyon::tessellation::LineJoin::Round,
            LineJoin::Bevel => lyon::tessellation::LineJoin::Bevel,
        };
        let options = StrokeOptions::tolerance(tolerance)
            .with_line_width(width)
            .with_line_cap(cap)
            .with_line_join(join)
            .with_miter_limit(MITER_LIMIT);
        let mut buffers: VertexBuffers<lyon::math::Point, u32> = VertexBuffers::new();
        let result = tessellators.stroke.tessellate_path(
            &path,
            &options,
            &mut BuffersBuilder::new(&mut buffers, |v: StrokeVertex| v.position()),
        );
        if result.is_ok() && !append(&mut mesh, &buffers, color, map) {
            return None;
        }
    }

    Some(mesh)
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

    let mut builder = Path::builder();
    for sub in subpaths {
        let path = lyon_path(std::iter::once(sub), true);
        let measurements = PathMeasurements::from_path(&path, tolerance);
        let length = f64::from(measurements.length());
        if length <= 0.0 {
            continue;
        }
        if length / period > MAX_DASHES_PER_SUBPATH {
            let solid = lyon_path(std::iter::once(sub), false);
            for event in solid.iter() {
                builder.path_event(event);
            }
            continue;
        }
        let mut sampler = measurements.create_sampler(&path, SampleType::Distance);

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
        if !append(&mut mesh, &buffers, color, map) {
            return None;
        }
    }
    Some(mesh)
}

/// Clips every triangle of `mesh` against `clip` with the Sutherland–Hodgman algorithm, re-triangulating clipped
/// triangles as fans. Triangles entirely inside keep their vertices; triangles entirely outside are dropped.
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
        let polygon = clip_polygon(vertices.iter().map(|v| v.pos).collect(), clip);
        if polygon.len() < 3 {
            continue;
        }
        let color = vertices[0].color;
        let base = out.vertices.len() as u32;
        out.vertices
            .extend(polygon.iter().map(|&pos| egui::epaint::Vertex {
                pos,
                uv: egui::epaint::WHITE_UV,
                color,
            }));
        for k in 1..polygon.len() as u32 - 1 {
            out.indices.extend([base, base + k, base + k + 1]);
        }
    }
    out
}

/// Clips a convex polygon against an axis-aligned rectangle.
fn clip_polygon(mut polygon: Vec<egui::Pos2>, clip: egui::Rect) -> Vec<egui::Pos2> {
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
            let (dc, dp) = (distance(current), distance(previous));
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

fn intersection(a: egui::Pos2, b: egui::Pos2, da: f32, db: f32) -> egui::Pos2 {
    let t = da / (da - db);
    a + (b - a) * t
}
