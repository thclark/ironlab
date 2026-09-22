//! IronLAB's own wgpu pipelines: geometry with a depth, drawn with a depth test.
//!
//! The artists of a three-dimensional axes reach the canvas as one [`DrawList`] per depth group (see
//! [`crate::canvas::drawables_with`]): triangles in screen units, each vertex with a depth `z` in `[0, 1]` (0 the
//! nearest) and a premultiplied colour, cut into [`Draw`]s that each name a texture, a clip rectangle and whether
//! the depth test applies. A [`GpuPainter`] draws such lists with pipelines of its own, inside egui's render pass in
//! the interactive canvas (through [`GpuCallback`], an [`egui_wgpu::CallbackTrait`]) and inside the offscreen
//! renderer's pass for the gallery and the PDF exporter, so that every route to pixels shares the shaders in
//! `gpu.wgsl`, and nothing here needs a window.
//!
//! # Drawing
//!
//! The vertex shader maps screen points to clip space exactly as egui's does, from the target's size in points, so
//! that a vertex lands on the same pixel as an egui mesh vertex at the same position. The fragment shader multiplies
//! the vertex colour by a nearest-sampled texture: a 1 × 1 white texture for solid geometry, or a tile of an image,
//! held as premultiplied gamma-space bytes exactly as the display list gives them. The blend state is egui's
//! premultiplied one, so that a translucent face composites as an egui mesh would. Two pipelines differ only in the
//! depth test (`LessEqual` with writes, or `Always` without), and a third clears the depth buffer over the whole
//! target with a triangle at the far plane that writes no colour, drawn before each list, so that the depth groups of
//! one frame never occlude one another. Every draw is clipped by the scissor rectangle of its clip, rounded to whole
//! pixels as egui rounds its own.
//!
//! # Caches
//!
//! A list's vertex and index buffers are uploaded once and kept for as long as the list is drawn, keyed by the
//! address of its `Arc`, which the painter holds so that the address cannot be reused; the tiles of images are kept
//! likewise, keyed by their sample buffer and tile. [`GpuPainter::retain_used`] drops what the frames since the
//! previous call did not draw, and [`GpuPainter::clear`] drops everything, which the offscreen renderer does after
//! each render.
//!
//! # The follow-on
//!
//! The change that follows this one draws every display item through these pipelines and deletes the egui-mesh
//! path of the canvas: the egui-mesh conversion, the texture provider and its two implementations, the geometric
//! clipping of meshes, the background mesh and the default white texture of the offscreen renderer, and egui's own
//! renderer in the offscreen path. Nothing here depends on any of them.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;
use ironlab_ir::NodeId;

/// The format of the depth attachment every pass drawing these pipelines carries.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// One vertex of a [`DrawList`].
///
/// `pos` is in screen units (egui points in the interactive canvas, pixels offscreen), the units of egui's meshes;
/// `z` is the depth in `[0, 1]` with 0 the nearest, normalised over the list; `uv` is the texture coordinate; and
/// `color` is premultiplied sRGB, as [`egui::Color32`] is.
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

/// One drawing of a [`DrawList`]: a range of its index buffer, drawn with a texture or as solid geometry, with or
/// without the depth test, clipped to `clip` in screen units, for the node `source`.
#[derive(Clone, Debug, PartialEq)]
pub struct Draw {
    pub indices: Range<u32>,
    pub texture: Option<TileKey>,
    pub depth_test: bool,
    pub clip: Option<egui::Rect>,
    pub source: Option<NodeId>,
}

/// The geometry of one depth group, or of one run of depth-carrying items outside any group, ready to upload.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawList {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<Draw>,
}

impl DrawList {
    /// Reports whether the list draws nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }
}

/// What the canvas produces for one stretch of the paint order: an egui mesh, or a list for these pipelines.
#[derive(Clone, Debug)]
pub enum Drawable {
    Mesh(egui::Mesh),
    Gpu(Arc<DrawList>),
}

/// The render target the pipelines are built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GpuConfig {
    pub target_format: wgpu::TextureFormat,
    pub samples: u32,
    pub depth_format: wgpu::TextureFormat,
}

/// An egui paint callback that draws one list through the [`GpuPainter`] kept in the renderer's callback
/// resources, creating the painter on first use.
pub struct GpuCallback {
    pub list: Arc<DrawList>,
    pub config: GpuConfig,
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
        painter.prepare(device, queue, screen, self.config, &self.list);
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(painter) = resources.get::<GpuPainter>() {
            painter.paint(pass, &info, self.config, &self.list);
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
    tested: wgpu::RenderPipeline,
    untested: wgpu::RenderPipeline,
    clear: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::BindGroup,
}

/// The uniform holding the target's size in points, and its bind group.
struct Screen {
    buffer: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

/// The buffers of one list, kept while the list is drawn.
struct ListBuffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
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

/// Draws [`DrawList`]s through the pipelines, keeping their buffers and textures across frames.
#[derive(Default)]
pub struct GpuPainter {
    screen: Option<Screen>,
    pipelines: HashMap<GpuConfig, Pipelines>,
    lists: HashMap<usize, ListBuffers>,
    tiles: HashMap<(usize, u32, u32), Tile>,
}

impl GpuPainter {
    /// Uploads what drawing `list` needs: the screen uniform, the pipelines for `config`, the list's buffers and
    /// the textures of its tiles, each once.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        config: GpuConfig,
        list: &Arc<DrawList>,
    ) {
        let uniform = self.screen.get_or_insert_with(|| Screen::new(device));
        let [width, height] = screen.size_in_pixels;
        let size = [
            width as f32 / screen.pixels_per_point,
            height as f32 / screen.pixels_per_point,
            0.0,
            0.0,
        ];
        queue.write_buffer(&uniform.buffer, 0, bytemuck::cast_slice(&size));
        let layout = &uniform.layout;
        let pipelines = self
            .pipelines
            .entry(config)
            .or_insert_with(|| Pipelines::new(device, queue, config, layout));

        if list.is_empty() || list.vertices.is_empty() || list.indices.is_empty() {
            return;
        }
        let key = Arc::as_ptr(list).addr();
        let buffers = self.lists.entry(key).or_insert_with(|| ListBuffers {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ironlab depth vertices"),
                contents: bytemuck::cast_slice(&list.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ironlab depth indices"),
                contents: bytemuck::cast_slice(&list.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            _list: Arc::clone(list),
            used: false,
        });
        buffers.used = true;
        for tile in list.draws.iter().filter_map(|draw| draw.texture.as_ref()) {
            let Some(pixels) = tile.pixels() else {
                continue;
            };
            let (width, height) = tile.size();
            if width == 0 || height == 0 {
                continue;
            }
            let entry = self.tiles.entry(tile.id()).or_insert_with(|| {
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

    /// Draws `list` into the pass, clearing the depth buffer first. The pass must carry a colour attachment of
    /// `config.target_format` with `config.samples` samples and a depth attachment of `config.depth_format`.
    pub fn paint(
        &self,
        pass: &mut wgpu::RenderPass<'static>,
        info: &egui::PaintCallbackInfo,
        config: GpuConfig,
        list: &Arc<DrawList>,
    ) {
        let (Some(screen), Some(pipelines)) = (&self.screen, self.pipelines.get(&config)) else {
            return;
        };
        let [width, height] = info.screen_size_px;
        if width == 0 || height == 0 {
            return;
        }
        pass.set_bind_group(0, &screen.bind_group, &[]);
        // The clearing pipeline shares the layout of the others, so it needs a texture bound even though it never
        // samples it.
        pass.set_bind_group(1, &pipelines.white, &[]);
        pass.set_scissor_rect(0, 0, width, height);
        pass.set_pipeline(&pipelines.clear);
        pass.draw(0..3, 0..1);

        let Some(buffers) = self.lists.get(&Arc::as_ptr(list).addr()) else {
            return;
        };
        pass.set_vertex_buffer(0, buffers.vertices.slice(..));
        pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
        let outer = info.clip_rect;
        for draw in &list.draws {
            let clip = match draw.clip {
                Some(clip) => clip.intersect(outer),
                None => outer,
            };
            let Some((x, y, w, h)) = scissor(clip, info.pixels_per_point, [width, height]) else {
                continue;
            };
            if draw.indices.is_empty() || draw.indices.end as usize > list.indices.len() {
                continue;
            }
            let texture = match &draw.texture {
                Some(tile) => match self.tiles.get(&tile.id()) {
                    Some(tile) => &tile.bind_group,
                    None => continue,
                },
                None => &pipelines.white,
            };
            pass.set_scissor_rect(x, y, w, h);
            pass.set_pipeline(if draw.depth_test {
                &pipelines.tested
            } else {
                &pipelines.untested
            });
            pass.set_bind_group(1, texture, &[]);
            pass.draw_indexed(draw.indices.clone(), 0, 0..1);
        }
    }

    /// Drops the buffers and textures that have not been drawn since the previous call, and starts a new round.
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

impl Screen {
    fn new(device: &wgpu::Device) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ironlab screen uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ironlab screen uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ironlab screen uniform"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Self {
            buffer,
            layout,
            bind_group,
        }
    }
}

impl Pipelines {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: GpuConfig,
        screen_layout: &wgpu::BindGroupLayout,
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ironlab gpu pipeline layout"),
            bind_group_layouts: &[Some(screen_layout), Some(&texture_layout)],
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
        let pipeline = |label, vertex: &str, fragment: &str, depth_state, write_mask, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: if vertex == "vs_main" {
                        &[Some(Vertex::LAYOUT)]
                    } else {
                        &[]
                    },
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
            "vs_main",
            "fs_main",
            depth(true, wgpu::CompareFunction::LessEqual),
            wgpu::ColorWrites::ALL,
            Some(BLEND),
        );
        let untested = pipeline(
            "ironlab untested",
            "vs_main",
            "fs_main",
            depth(false, wgpu::CompareFunction::Always),
            wgpu::ColorWrites::ALL,
            Some(BLEND),
        );
        let clear = pipeline(
            "ironlab depth clear",
            "vs_clear",
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
