//! IronLAB's own wgpu pipelines: every item of a figure, drawn from one list.
//!
//! A figure reaches the device as one [`DrawList`] (see [`crate::canvas::tessellate`]): triangles in figure points
//! for fills, glyphs and image tiles, and stroked polylines as [`Segment`]s in the item space of their paths, each
//! vertex or segment end with a depth `z` in `[0, 1]` (0 the nearest), cut into [`Draw`]s that each name a
//! texture, a clip rectangle, a depth group and the node they draw. A [`GpuPainter`] draws such lists with
//! pipelines of its own, inside egui's render pass in the interactive window (through [`GpuCallback`], an
//! [`egui_wgpu::CallbackTrait`]) and inside the offscreen renderer's own pass for the gallery and the PDF exporter,
//! so that every route to pixels shares the shaders in `gpu.wgsl`, and nothing here needs a window.
//!
//! # Drawing
//!
//! The vertex shaders map figure points to screen points through a [`Viewport`]'s mapping, held in a uniform per
//! list, and screen points to clip space from the target's size in points, exactly as egui maps its own meshes, so
//! that the interface and the figure land on the same pixel grid. The fragment shader of triangles multiplies the
//! vertex colour by a nearest-sampled texture: a 1 × 1 white texture for solid geometry, or a tile of an image,
//! held as premultiplied gamma-space bytes. The blend state is egui's premultiplied one, so that the figure
//! composites over the interface as egui's own shapes do.
//!
//! A stroke is expanded on the device: the stroke vertex shader turns each segment instance into its body quad,
//! the join at its start (miter within the PDF limit of four, else bevel; or round) and the caps at its free ends
//! (butt, round or square), in item space, and maps the result through the draw's item-to-figure transform held
//! with the stroke's width, colour and dash pattern in a [`StrokeParams`] slot of a uniform buffer bound with a
//! dynamic offset per draw. The stroke fragment shader dashes by arc length, giving each dash the caps of the
//! stroke and anti-aliasing its ends over a pixel.
//!
//! Two depth states serve everything: the draws of a depth group are depth-tested against one another
//! (`LessEqual` with writes; strokes there take `z` from their segments), after the depth buffer is cleared for the
//! group; every other triangle draw is drawn without the depth test (`Always`, no writes). A stroke outside every
//! group is depth-tested with `Less` and writes the `z` of its own params: the k-th such stroke since the last
//! clear lies at `1 - (k + 1) · 2⁻²⁰`, so later strokes pass over earlier ones while the parts of one stroke that
//! overlap, on the inner side of a turn or under a round join, take its colour exactly once, as a stroke does in
//! PDF. The depth buffer is cleared at the start of every list and whenever the depth group changes. Every draw is
//! clipped by the scissor rectangle of its clip, rounded to whole pixels as egui rounds its own.
//!
//! # Caches and uploads
//!
//! A list's buffers are uploaded once and kept for as long as the list is drawn, keyed by the address of its
//! `Arc`, which the painter holds so that the address cannot be reused; the list's mapping uniform is rewritten
//! only when the list lands somewhere else on the target; and the tiles of images are kept likewise, keyed by their
//! sample buffer and tile. A frame that draws the same lists at the same places therefore uploads nothing, and a
//! resize rewrites the mappings alone. [`GpuPainter::uploads`] counts what has been uploaded, so that this can be
//! tested. [`GpuPainter::retain_used`] drops what the frames since the previous call did not draw, and
//! [`GpuPainter::clear`] drops everything, which the offscreen renderer does after each render.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;
use ironlab_ir::NodeId;
use ironlab_scene::display::{Point, Rect};

use crate::canvas::ScreenTransform;

/// The format of the depth attachment every pass drawing these pipelines carries.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// One vertex of a [`DrawList`].
///
/// `pos` is in figure points; `z` is the depth in `[0, 1]` with 0 the nearest, normalised over the vertex's depth
/// group and 0 outside any group; `uv` is the texture coordinate; and `color` is premultiplied sRGB, as
/// [`egui::Color32`] is.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub z: f32,
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

impl Vertex {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32x2, 3 => Unorm8x4],
    };
}

/// A join is drawn at the start of the segment: `prev` is a real vertex of the polyline.
pub const JOIN_AT_START: u32 = 1;
/// A join is drawn at the end of the segment: `next` is a real vertex of the polyline.
pub const JOIN_AT_END: u32 = 2;

/// One segment of a stroked polyline, in the item space of its path, which the stroke pipeline expands.
///
/// Without [`JOIN_AT_START`] the stroke's cap is drawn at `p0`; without [`JOIN_AT_END`] it is drawn at `p1`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Segment {
    /// The vertex before `p0`, or `p0` itself when the segment starts an open subpath.
    pub prev: [f32; 2],
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    /// The vertex after `p1`, or `p1` itself when the segment ends an open subpath.
    pub next: [f32; 2],
    /// The depth at `p0` and at `p1`: normalised over the depth group (0 the nearest), 0 outside every group.
    pub z: [f32; 2],
    /// The distance along the subpath from its start, in item units, at `p0` and at `p1`.
    pub arc: [f32; 2],
    /// The change of `z` per item unit of x and of y, so that a vertex offset from the polyline for the stroke's
    /// width takes the depth of its own position on a `Depth::Plane`; zero for other depths and outside groups.
    pub grad: [f32; 2],
    /// [`JOIN_AT_START`] and [`JOIN_AT_END`].
    pub flags: u32,
    /// The arc length at `p0` as the segment before it measures it: `arc[0]`, except for the first segment of a
    /// closed subpath, where the segment before it is the closing one and the joint lies at the subpath's length.
    pub prev_arc: f32,
}

impl Segment {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Segment>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x2, 4 => Float32x2, 5 => Float32x2,
            6 => Float32x2, 7 => Uint32, 8 => Float32
        ],
    };
}

/// The number of vertices the stroke vertex shader emits per segment: six for the body and eight triangles for
/// each of the features at its ends (a join or a cap at the start, a cap at the end).
const STROKE_VERTICES: u32 = 6 + 3 * 8 + 3 * 8;

/// The largest number of dash entries a stroke draw carries; a longer pattern is drawn solid.
pub const MAX_DASH_ENTRIES: usize = 16;

/// The constants of one stroke draw, held in a slot of the list's uniform buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct StrokeParams {
    /// The item-to-figure transform: its linear part `[a, b, c, d]` (display-list `Transform` fields).
    pub linear: [f32; 4],
    /// The translation `[e, f]` of the transform, padded.
    pub offset: [f32; 4],
    /// The premultiplied sRGB colour as fractions.
    pub color: [f32; 4],
    /// The width in item units; 0 means one screen point.
    pub width: f32,
    /// 0 butt, 1 round, 2 square.
    pub cap: u32,
    /// 0 miter (within a limit of four), 1 round, 2 bevel.
    pub join: u32,
    /// The number of entries of `dashes` in use: 0 for a solid stroke, else even and at most
    /// [`MAX_DASH_ENTRIES`].
    pub dash_count: u32,
    /// The dash phase, in item units.
    pub dash_offset: f32,
    /// The sum of the dash entries, in item units.
    pub period: f32,
    /// The depth written by a stroke outside every depth group; unused when `vertex_z` is set.
    pub z: f32,
    /// 1 when the depth comes from the segments (inside a depth group), 0 when it is `z`.
    pub vertex_z: u32,
    /// The dash pattern: on, off, on, off, …, in item units.
    pub dashes: [f32; MAX_DASH_ENTRIES],
}

/// The stride of a [`StrokeParams`] slot in the uniform buffer, which every device accepts as a dynamic offset.
const PARAMS_STRIDE: u64 = 256;

/// One tile of an image: the pixels of `columns` × `rows` of an image whose rows hold `width` pixels of `channels`
/// bytes each, three for opaque RGB and four for RGB with straight alpha. No texture exists until a painter uploads
/// the tile.
#[derive(Clone, Debug, PartialEq)]
pub struct TileKey {
    pub samples: Arc<[u8]>,
    pub width: u32,
    pub channels: u8,
    pub columns: Range<u32>,
    pub rows: Range<u32>,
}

impl TileKey {
    /// The identity of the tile: its sample buffer's address and its first column and row.
    fn id(&self) -> (usize, u32, u32) {
        (
            Arc::as_ptr(&self.samples).cast::<u8>().addr(),
            self.columns.start,
            self.rows.start,
        )
    }

    fn size(&self) -> (u32, u32) {
        (
            self.columns.end.saturating_sub(self.columns.start),
            self.rows.end.saturating_sub(self.rows.start),
        )
    }

    /// The premultiplied RGBA bytes of the tile, row by row, or `None` when the key does not describe its samples.
    fn pixels(&self) -> Option<Vec<u8>> {
        let channels = usize::from(self.channels);
        if !(channels == 3 || channels == 4) {
            return None;
        }
        let (width, height) = self.size();
        let stride = self.width as usize * channels;
        let mut out = Vec::with_capacity(width as usize * height as usize * 4);
        for row in self.rows.clone() {
            let start = row as usize * stride + self.columns.start as usize * channels;
            let end = start + width as usize * channels;
            let line = self.samples.get(start..end)?;
            for pixel in line.chunks_exact(channels) {
                if channels == 3 {
                    out.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                } else {
                    let a = u32::from(pixel[3]);
                    let premultiply = |c: u8| ((u32::from(c) * a + 127) / 255) as u8;
                    out.extend_from_slice(&[
                        premultiply(pixel[0]),
                        premultiply(pixel[1]),
                        premultiply(pixel[2]),
                        pixel[3],
                    ]);
                }
            }
        }
        Some(out)
    }
}

/// What a [`Draw`] draws.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawKind {
    /// A range of the list's index buffer, drawn as triangles of its vertices.
    Triangles(Range<u32>),
    /// A range of the list's segments, expanded into a stroke with the parameters at index `params` of
    /// [`DrawList::stroke_params`].
    Stroke { segments: Range<u32>, params: u32 },
}

/// One drawing of a [`DrawList`]: triangles drawn with a texture or as solid geometry, or a stroke, inside a depth
/// group or outside every one, clipped to `clip` in figure points, for the node `source`.
#[derive(Clone, Debug, PartialEq)]
pub struct Draw {
    pub kind: DrawKind,
    /// The tile a triangle draw samples; `None` for solid geometry and for strokes.
    pub texture: Option<TileKey>,
    /// The depth group of the draw, numbered from 0 in paint order within the list, or `None` for a draw outside
    /// every depth group. The draws of a group are depth-tested against one another, and the depth buffer is
    /// cleared before the first of them; every other draw is drawn without the depth test.
    pub depth_group: Option<u32>,
    /// The clip rectangle of the draw in figure points, applied as a scissor, or `None` for an unclipped draw.
    pub clip: Option<Rect>,
    pub source: Option<NodeId>,
}

/// The geometry of one figure, ready to upload: every item in paint order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawList {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub segments: Vec<Segment>,
    pub stroke_params: Vec<StrokeParams>,
    pub draws: Vec<Draw>,
}

impl DrawList {
    /// Reports whether the list draws nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }
}

/// The render target the pipelines are built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GpuConfig {
    pub target_format: wgpu::TextureFormat,
    pub samples: u32,
    pub depth_format: wgpu::TextureFormat,
}

/// Where a list is drawn: the render target, the mapping from figure points to screen points, and the rectangle
/// that bounds every draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// The size of the render target in pixels.
    pub size_px: [u32; 2],
    /// Pixels per screen point: the window's scale factor on screen, 1 offscreen.
    pub pixels_per_point: f32,
    /// Figure points to screen points.
    pub to_screen: ScreenTransform,
    /// The rectangle in screen points that bounds every draw: the canvas on screen, the whole image offscreen.
    pub clip: egui::Rect,
}

impl Viewport {
    /// A viewport whose clip is the whole target.
    #[must_use]
    pub fn whole(size_px: [u32; 2], pixels_per_point: f32, to_screen: ScreenTransform) -> Self {
        let [width, height] = size_px;
        Self {
            size_px,
            pixels_per_point,
            to_screen,
            clip: egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(
                    width as f32 / pixels_per_point,
                    height as f32 / pixels_per_point,
                ),
            ),
        }
    }

    /// The viewport of an egui paint callback: the whole target, clipped to the painter's clip rectangle.
    #[must_use]
    pub fn from_callback(info: &egui::PaintCallbackInfo, to_screen: ScreenTransform) -> Self {
        Self {
            size_px: info.screen_size_px,
            pixels_per_point: info.pixels_per_point,
            to_screen,
            clip: info.clip_rect,
        }
    }

    /// The size of the target in screen points.
    fn size_in_points(&self) -> [f32; 2] {
        [
            self.size_px[0] as f32 / self.pixels_per_point,
            self.size_px[1] as f32 / self.pixels_per_point,
        ]
    }

    /// What fixes the clip-space position of every vertex: the target's size and the mapping.
    fn placement(&self) -> Placement {
        Placement {
            size_px: self.size_px,
            pixels_per_point: self.pixels_per_point,
            to_screen: self.to_screen,
        }
    }
}

/// The part of a [`Viewport`] the mapping uniform holds.
#[derive(Clone, Copy, PartialEq)]
struct Placement {
    size_px: [u32; 2],
    pixels_per_point: f32,
    to_screen: ScreenTransform,
}

/// An egui paint callback that draws one list at one place through the [`GpuPainter`] kept in the renderer's
/// callback resources, creating the painter on first use.
pub struct GpuCallback {
    pub list: Arc<DrawList>,
    pub config: GpuConfig,
    pub to_screen: ScreenTransform,
}

impl egui_wgpu::CallbackTrait for GpuCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let painter = resources
            .entry::<GpuPainter>()
            .or_insert_with(GpuPainter::default);
        let viewport = Viewport::whole(
            screen.size_in_pixels,
            screen.pixels_per_point,
            self.to_screen,
        );
        painter.prepare(device, queue, self.config, &self.list, &viewport);
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(painter) = resources.get::<GpuPainter>() {
            let viewport = Viewport::from_callback(&info, self.to_screen);
            painter.paint(pass, &viewport, self.config, &self.list);
        }
    }
}

/// egui's blend state for premultiplied colour.
const BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

/// The pipelines for one [`GpuConfig`].
struct Pipelines {
    /// Triangles inside a depth group.
    tested: wgpu::RenderPipeline,
    /// Triangles outside every depth group.
    untested: wgpu::RenderPipeline,
    /// Strokes inside a depth group, with the depth of their segments.
    stroke_tested: wgpu::RenderPipeline,
    /// Strokes outside every depth group, at the depth of their params.
    stroke_flat: wgpu::RenderPipeline,
    clear: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::BindGroup,
}

/// The bind group layouts shared by the pipelines of every configuration.
struct Layouts {
    mapping: wgpu::BindGroupLayout,
    params: wgpu::BindGroupLayout,
}

/// The size in bytes of the mapping uniform: the target's size in points and the mapping's origin and scale, each
/// padded to a `vec4`.
const MAPPING_SIZE: u64 = 32;

/// The buffers of one list, kept while the list is drawn.
struct ListBuffers {
    vertices: Option<wgpu::Buffer>,
    indices: Option<wgpu::Buffer>,
    segments: Option<wgpu::Buffer>,
    /// The stroke params, one [`PARAMS_STRIDE`] slot each, and their bind group.
    params: Option<wgpu::BindGroup>,
    /// The mapping uniform of the list and its bind group.
    mapping: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The placement the mapping uniform holds, or `None` before its first write.
    placement: Option<Placement>,
    /// Held so that the address the list is keyed by cannot be reused by another list.
    _list: Arc<DrawList>,
    used: bool,
}

/// The texture of one tile, kept while the tile is drawn.
struct Tile {
    /// Held so that the address the tile is keyed by cannot be reused by another buffer.
    _samples: Arc<[u8]>,
    bind_group: wgpu::BindGroup,
    used: bool,
}

/// Counts of what a [`GpuPainter`] has uploaded since it was made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Uploads {
    /// Lists whose buffers were uploaded.
    pub lists: u64,
    /// Writes of a list's mapping uniform.
    pub mappings: u64,
    /// Image tiles uploaded as textures.
    pub tiles: u64,
}

/// Draws [`DrawList`]s through the pipelines, keeping their buffers and textures across frames.
#[derive(Default)]
pub struct GpuPainter {
    layouts: Option<Layouts>,
    pipelines: HashMap<GpuConfig, Pipelines>,
    lists: HashMap<usize, ListBuffers>,
    tiles: HashMap<(usize, u32, u32), Tile>,
    uploads: Uploads,
}

impl GpuPainter {
    /// Uploads what drawing `list` in `viewport` needs, each once: the pipelines for `config`, the list's buffers,
    /// its mapping (rewritten only when the list lands elsewhere than it last did) and the textures of its tiles.
    /// Preparing an unchanged list at an unchanged place uploads nothing.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: GpuConfig,
        list: &Arc<DrawList>,
        viewport: &Viewport,
    ) {
        let layouts = self.layouts.get_or_insert_with(|| Layouts::new(device));
        let pipelines = self
            .pipelines
            .entry(config)
            .or_insert_with(|| Pipelines::new(device, queue, config, layouts));
        if list.is_empty() {
            return;
        }
        let uploads = &mut self.uploads;
        let key = Arc::as_ptr(list).addr();
        let buffers = self.lists.entry(key).or_insert_with(|| {
            uploads.lists += 1;
            ListBuffers::new(device, layouts, list)
        });
        buffers.used = true;
        let placement = viewport.placement();
        if buffers.placement != Some(placement) {
            let [width, height] = viewport.size_in_points();
            let origin = viewport.to_screen.origin;
            let contents: [f32; 8] = [
                width,
                height,
                0.0,
                0.0,
                origin.x,
                origin.y,
                viewport.to_screen.scale,
                0.0,
            ];
            queue.write_buffer(&buffers.mapping, 0, bytemuck::cast_slice(&contents));
            buffers.placement = Some(placement);
            uploads.mappings += 1;
        }
        for tile in list.draws.iter().filter_map(|draw| draw.texture.as_ref()) {
            let Some(pixels) = tile.pixels() else {
                continue;
            };
            let (width, height) = tile.size();
            if width == 0 || height == 0 {
                continue;
            }
            let entry = self.tiles.entry(tile.id()).or_insert_with(|| {
                uploads.tiles += 1;
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("ironlab image tile"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                queue.write_texture(
                    texture.as_image_copy(),
                    &pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * width),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                Tile {
                    _samples: Arc::clone(&tile.samples),
                    bind_group: pipelines.texture_bind_group(device, &view),
                    used: false,
                }
            });
            entry.used = true;
        }
    }

    /// Draws `list` into the pass at `viewport`. The pass must carry a colour attachment of `config.target_format`
    /// with `config.samples` samples and a depth attachment of `config.depth_format`; the list must have been
    /// prepared for `config` since the painter was last cleared.
    pub fn paint(
        &self,
        pass: &mut wgpu::RenderPass<'static>,
        viewport: &Viewport,
        config: GpuConfig,
        list: &Arc<DrawList>,
    ) {
        let Some(pipelines) = self.pipelines.get(&config) else {
            return;
        };
        let [width, height] = viewport.size_px;
        if width == 0 || height == 0 {
            return;
        }
        let Some(buffers) = self.lists.get(&Arc::as_ptr(list).addr()) else {
            return;
        };
        pass.set_bind_group(0, &buffers.bind_group, &[]);
        let clear_depth = |pass: &mut wgpu::RenderPass<'static>| {
            // The clearing pipeline shares the layout of the triangle pipelines, so it needs a texture bound even
            // though it never samples it.
            pass.set_bind_group(1, &pipelines.white, &[]);
            pass.set_scissor_rect(0, 0, width, height);
            pass.set_pipeline(&pipelines.clear);
            pass.draw(0..3, 0..1);
        };
        clear_depth(pass);
        let mut group = None;
        for draw in &list.draws {
            if draw.depth_group != group {
                group = draw.depth_group;
                clear_depth(pass);
            }
            let clip = match draw.clip {
                Some(clip) => to_points(clip, viewport.to_screen).intersect(viewport.clip),
                None => viewport.clip,
            };
            let Some((x, y, w, h)) = scissor(clip, viewport.pixels_per_point, [width, height])
            else {
                continue;
            };
            match &draw.kind {
                DrawKind::Triangles(indices) => {
                    if indices.is_empty() || indices.end as usize > list.indices.len() {
                        continue;
                    }
                    let (Some(vertices), Some(index_buffer)) =
                        (&buffers.vertices, &buffers.indices)
                    else {
                        continue;
                    };
                    let texture = match &draw.texture {
                        Some(tile) => match self.tiles.get(&tile.id()) {
                            Some(tile) => &tile.bind_group,
                            None => continue,
                        },
                        None => &pipelines.white,
                    };
                    pass.set_scissor_rect(x, y, w, h);
                    pass.set_pipeline(if draw.depth_group.is_some() {
                        &pipelines.tested
                    } else {
                        &pipelines.untested
                    });
                    pass.set_bind_group(1, texture, &[]);
                    pass.set_vertex_buffer(0, vertices.slice(..));
                    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(indices.clone(), 0, 0..1);
                }
                DrawKind::Stroke { segments, params } => {
                    if segments.is_empty()
                        || segments.end as usize > list.segments.len()
                        || *params as usize >= list.stroke_params.len()
                    {
                        continue;
                    }
                    let (Some(segment_buffer), Some(params_group)) =
                        (&buffers.segments, &buffers.params)
                    else {
                        continue;
                    };
                    pass.set_scissor_rect(x, y, w, h);
                    pass.set_pipeline(if draw.depth_group.is_some() {
                        &pipelines.stroke_tested
                    } else {
                        &pipelines.stroke_flat
                    });
                    pass.set_bind_group(
                        1,
                        params_group,
                        &[u32::try_from(u64::from(*params) * PARAMS_STRIDE).unwrap_or(u32::MAX)],
                    );
                    pass.set_vertex_buffer(0, segment_buffer.slice(..));
                    pass.draw(0..STROKE_VERTICES, segments.clone());
                }
            }
        }
    }

    /// Drops the buffers and textures that have not been prepared since the previous call, and starts a new round.
    pub fn retain_used(&mut self) {
        self.lists
            .retain(|_, entry| std::mem::replace(&mut entry.used, false));
        self.tiles
            .retain(|_, entry| std::mem::replace(&mut entry.used, false));
    }

    /// Drops every buffer and texture, keeping the pipelines.
    pub fn clear(&mut self) {
        self.lists.clear();
        self.tiles.clear();
    }

    /// What the painter has uploaded since it was made.
    #[must_use]
    pub fn uploads(&self) -> Uploads {
        self.uploads
    }
}

/// A rectangle in figure points mapped to screen points.
fn to_points(rect: Rect, to_screen: ScreenTransform) -> egui::Rect {
    egui::Rect::from_min_max(
        to_screen.apply(Point::new(rect.x, rect.y)),
        to_screen.apply(Point::new(rect.right(), rect.bottom())),
    )
}

/// The scissor rectangle, in pixels within a target of `size`, of a clip in points, rounded as egui rounds its own;
/// `None` when nothing of the target is inside it.
fn scissor(
    clip: egui::Rect,
    pixels_per_point: f32,
    size: [u32; 2],
) -> Option<(u32, u32, u32, u32)> {
    let to_px = |v: f32, limit: u32| (v * pixels_per_point).round().clamp(0.0, limit as f32) as u32;
    let (x0, y0) = (to_px(clip.min.x, size[0]), to_px(clip.min.y, size[1]));
    let (x1, y1) = (to_px(clip.max.x, size[0]), to_px(clip.max.y, size[1]));
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

impl Layouts {
    fn new(device: &wgpu::Device) -> Self {
        let uniform = |label, dynamic: bool, size: u64| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: dynamic,
                        min_binding_size: wgpu::BufferSize::new(size),
                    },
                    count: None,
                }],
            })
        };
        Self {
            mapping: uniform("ironlab mapping layout", false, MAPPING_SIZE),
            params: uniform(
                "ironlab stroke params layout",
                true,
                std::mem::size_of::<StrokeParams>() as u64,
            ),
        }
    }
}

impl ListBuffers {
    fn new(device: &wgpu::Device, layouts: &Layouts, list: &Arc<DrawList>) -> Self {
        let mapping = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ironlab mapping"),
            size: MAPPING_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ironlab mapping"),
            layout: &layouts.mapping,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: mapping.as_entire_binding(),
            }],
        });
        let buffer = |label, contents: &[u8], usage| {
            (!contents.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
            })
        };
        let params = (!list.stroke_params.is_empty()).then(|| {
            // One slot per stroke draw, each padded to the stride a dynamic offset must be a multiple of.
            let mut contents = vec![0u8; list.stroke_params.len() * PARAMS_STRIDE as usize];
            for (slot, params) in list.stroke_params.iter().enumerate() {
                let bytes = bytemuck::bytes_of(params);
                let start = slot * PARAMS_STRIDE as usize;
                contents[start..start + bytes.len()].copy_from_slice(bytes);
            }
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ironlab stroke params"),
                contents: &contents,
                usage: wgpu::BufferUsages::UNIFORM,
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ironlab stroke params"),
                layout: &layouts.params,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<StrokeParams>() as u64),
                    }),
                }],
            })
        });
        Self {
            vertices: buffer(
                "ironlab vertices",
                bytemuck::cast_slice(&list.vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: buffer(
                "ironlab indices",
                bytemuck::cast_slice(&list.indices),
                wgpu::BufferUsages::INDEX,
            ),
            segments: buffer(
                "ironlab segments",
                bytemuck::cast_slice(&list.segments),
                wgpu::BufferUsages::VERTEX,
            ),
            params,
            mapping,
            bind_group,
            placement: None,
            _list: Arc::clone(list),
            used: false,
        }
    }
}

impl Pipelines {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: GpuConfig,
        layouts: &Layouts,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ironlab gpu shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu.wgsl").into()),
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ironlab texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let triangle_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ironlab triangle pipeline layout"),
            bind_group_layouts: &[Some(&layouts.mapping), Some(&texture_layout)],
            immediate_size: 0,
        });
        let stroke_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ironlab stroke pipeline layout"),
            bind_group_layouts: &[Some(&layouts.mapping), Some(&layouts.params)],
            immediate_size: 0,
        });
        let multisample = wgpu::MultisampleState {
            count: config.samples.max(1),
            mask: !0,
            alpha_to_coverage_enabled: false,
        };
        let depth = |write: bool, compare: wgpu::CompareFunction| wgpu::DepthStencilState {
            format: config.depth_format,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let pipeline = |label,
                        layout: &wgpu::PipelineLayout,
                        vertex: &str,
                        buffers: &[wgpu::VertexBufferLayout<'_>],
                        fragment: &str,
                        depth_state,
                        write_mask,
                        blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: &buffers.iter().cloned().map(Some).collect::<Vec<_>>(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(depth_state),
                multisample,
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.target_format,
                        blend,
                        write_mask,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let tested = pipeline(
            "ironlab depth-tested",
            &triangle_layout,
            "vs_main",
            &[Vertex::LAYOUT],
            "fs_main",
            depth(true, wgpu::CompareFunction::LessEqual),
            wgpu::ColorWrites::ALL,
            Some(BLEND),
        );
        let untested = pipeline(
            "ironlab untested",
            &triangle_layout,
            "vs_main",
            &[Vertex::LAYOUT],
            "fs_main",
            depth(false, wgpu::CompareFunction::Always),
            wgpu::ColorWrites::ALL,
            Some(BLEND),
        );
        let stroke_tested = pipeline(
            "ironlab stroke depth-tested",
            &stroke_layout,
            "vs_stroke",
            &[Segment::LAYOUT],
            "fs_stroke",
            depth(true, wgpu::CompareFunction::LessEqual),
            wgpu::ColorWrites::ALL,
            Some(BLEND),
        );
        let stroke_flat = pipeline(
            "ironlab stroke flat",
            &stroke_layout,
            "vs_stroke",
            &[Segment::LAYOUT],
            "fs_stroke",
            depth(true, wgpu::CompareFunction::Less),
            wgpu::ColorWrites::ALL,
            Some(BLEND),
        );
        let clear = pipeline(
            "ironlab depth clear",
            &triangle_layout,
            "vs_clear",
            &[],
            "fs_clear",
            depth(true, wgpu::CompareFunction::Always),
            wgpu::ColorWrites::empty(),
            None,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ironlab nearest sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let white = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("ironlab white"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[255, 255, 255, 255],
        );
        let white_view = white.create_view(&wgpu::TextureViewDescriptor::default());
        let white = texture_bind_group(device, &texture_layout, &sampler, &white_view);
        Self {
            tested,
            untested,
            stroke_tested,
            stroke_flat,
            clear,
            texture_layout,
            sampler,
            white,
        }
    }

    fn texture_bind_group(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        texture_bind_group(device, &self.texture_layout, &self.sampler, view)
    }
}

/// A bind group sampling `view` with `sampler` under `layout`.
fn texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ironlab texture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}
