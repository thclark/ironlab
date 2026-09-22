//! Headless rendering of figures through the viewer's own mesh pipeline.
//!
//! [`render_offscreen`] compiles a figure, tessellates its display list with [`crate::canvas::tessellate_with`] at
//! `dpi / 72` pixels per point, and draws the meshes with [`egui_wgpu::Renderer`] into an offscreen texture. No
//! window or surface is created, so it runs in CI on a software adapter (for example lavapipe).
//!
//! # Pipeline
//!
//! 1. A wgpu instance is created without a display handle
//!    (`egui_wgpu::WgpuSetupCreateNew::without_display_handle`), an adapter is requested with no compatible
//!    surface, and a device and queue are requested from it. The backends honour the `WGPU_BACKEND` environment
//!    variable, as `without_display_handle` does, and there is no fallback to other backends when it is set. Failure
//!    to find an adapter is reported as [`RenderError::NoAdapter`], never as a panic. An [`OffscreenRenderer`] owns
//!    the device and can render any number of images; [`render_offscreen`] and [`render_display_list_offscreen`]
//!    share one process-wide renderer, created on first successful use, so that rendering a whole gallery creates
//!    the device only once.
//! 2. An [`egui_wgpu::Renderer`] is created for `Rgba8Unorm` (egui blends in gamma space and outputs gamma-encoded
//!    colour into a non-sRGB target) with 4× multisampling (when the adapter supports it), a depth attachment of
//!    [`DEPTH_FORMAT`] for the depth groups of three-dimensional axes, with dithering disabled,
//!    so that repeated renders of the same figure are identical, and with egui's predictable texture filtering off.
//!    That option makes egui's shader filter every texture bilinearly in its own code, whatever the texture's
//!    sampler asks for, which would blur the pixel edges of every image; with it off the sampler of each texture is
//!    honoured, and every texture made here asks for nearest filtering.
//! 3. The default egui texture (`TextureId::Managed(0)`) is uploaded as a 1×1 white image, because the meshes of
//!    paths and glyphs sample it at [`egui::epaint::WHITE_UV`].
//! 4. The image size is checked against the device's maximum texture dimension before any texture is created, and
//!    an invalid size is reported as [`RenderError::InvalidSize`].
//! 5. The display list is tessellated with a texture provider that uploads each visible tile of each image item
//!    through `Renderer::update_texture` as a user texture with nearest filtering, tiling by the device's maximum
//!    texture dimension capped at [`MAX_TILE_SIDE`]. The ids are `TextureId::User(n)` with `n` counted from
//!    [`TILE_TEXTURE_BASE`], far above the count the renderer numbers its own registered textures from, and are
//!    recorded so that step 7 can free them. The depth groups of three-dimensional axes become draw lists for the
//!    pipelines of [`crate::gpu`] instead, drawn by paint callbacks in their place in the paint order through the
//!    [`GpuPainter`] kept in the renderer's callback resources, which uploads their buffers and image tiles itself
//!    and is emptied after every render.
//! 6. The figure background is painted as a rectangle mesh beneath the meshes of the display list. The meshes are
//!    wrapped in `egui::ClippedPrimitive`s whose clip rectangle is the whole image, and `Renderer::update_buffers`
//!    and `Renderer::render` draw them with `ScreenDescriptor { size_in_pixels: [width, height], pixels_per_point:
//!    1.0 }` into a multisampled colour texture that is cleared to transparent black and resolved into a
//!    single-sample `COPY_SRC` texture, with a depth attachment cleared to the far plane. A transparent background
//!    therefore stays transparent.
//! 7. The resolved texture is copied into a `MAP_READ` buffer with rows padded to
//!    `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`, the device is polled until the copy completes, and the padding is
//!    stripped. The textures of step 5 are then freed with `Renderer::free_texture`, whether or not the draw and
//!    the readback succeeded, so that no render leaves textures on the device. The GPU output has premultiplied
//!    alpha, which is converted to straight alpha.
//!
//! The pixel size of the image is `round(width_pt · dpi / 72)` by `round(height_pt · dpi / 72)`.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use egui_wgpu::wgpu;
use ironlab_ir::Figure;
use ironlab_scene::display::DisplayList;
use ironlab_text::TextEngine;

use crate::canvas::{MAX_TILE_SIDE, ScreenTransform, TextureProvider, color32, drawables_with};
use crate::gpu::{DEPTH_FORMAT, Drawable, GpuCallback, GpuConfig, GpuPainter};

/// An 8-bit RGBA image with straight alpha, stored row by row from the top-left pixel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedImage {
    pub width: u32,
    pub height: u32,
    /// `width · height · 4` bytes.
    pub rgba: Vec<u8>,
}

impl RenderedImage {
    /// Returns the RGBA value of the pixel at column `x` and row `y`.
    ///
    /// # Panics
    ///
    /// Panics when the pixel is outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(
            x < self.width && y < self.height,
            "pixel ({x}, {y}) is outside the image"
        );
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }
}

/// A failure to render offscreen.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// No wgpu adapter is available, for example on a machine without a GPU or a software rasteriser.
    #[error(
        "no graphics adapter is available for offscreen rendering: {0}; a software adapter such as lavapipe from \
         Mesa (the `mesa-vulkan-drivers` package on Debian and Ubuntu) serves on a machine without a graphics device"
    )]
    NoAdapter(String),
    /// The adapter refused to create a device.
    #[error("the graphics adapter could not create a device: {0}")]
    Device(String),
    /// The requested image is empty or exceeds the adapter's maximum texture dimension.
    #[error("cannot render an image of {width}×{height} pixels (maximum dimension {max})")]
    InvalidSize { width: u32, height: u32, max: u32 },
    /// The rendered image could not be read back from the GPU.
    #[error("the rendered image could not be read back: {0}")]
    Readback(String),
}

/// Compiles `figure` and renders it at `dpi` dots per inch.
///
/// # Errors
///
/// Returns a [`RenderError`] when no adapter or device is available, when the image size is invalid for the adapter,
/// or when readback fails.
pub fn render_offscreen(
    figure: &Figure,
    text: &TextEngine,
    dpi: f64,
) -> Result<RenderedImage, RenderError> {
    let scene = ironlab_scene::compile(figure, text);
    render_display_list_offscreen(&scene.display_list, text, dpi)
}

/// Renders an already compiled display list at `dpi` dots per inch.
///
/// # Errors
///
/// As for [`render_offscreen`].
pub fn render_display_list_offscreen(
    list: &DisplayList,
    text: &TextEngine,
    dpi: f64,
) -> Result<RenderedImage, RenderError> {
    with_shared_renderer(|renderer| renderer.render_display_list(list, text, dpi))
}

/// Runs `use_renderer` against the process-wide renderer, creating its device on first use.
///
/// The device is by far the most expensive part of an offscreen render, so every caller in the process shares one,
/// and the renderer is discarded when a render fails at readback, which is how a lost device shows itself.
///
/// # Errors
///
/// Returns [`RenderError::NoAdapter`] or [`RenderError::Device`] when the renderer cannot be created, and otherwise
/// whatever `use_renderer` returns.
pub fn with_shared_renderer<T>(
    use_renderer: impl FnOnce(&mut OffscreenRenderer) -> Result<T, RenderError>,
) -> Result<T, RenderError> {
    static SHARED: Mutex<Option<OffscreenRenderer>> = Mutex::new(None);
    let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
    if shared.is_none() {
        *shared = Some(OffscreenRenderer::new()?);
    }
    let renderer = shared
        .as_mut()
        .expect("the shared renderer was just created");
    let result = use_renderer(renderer);
    if matches!(result, Err(RenderError::Readback(_))) {
        // The device may have been lost; create a new one on the next call.
        *shared = None;
    }
    result
}

/// The longest time to wait for the GPU to finish a render and its readback.
const WAIT_TIMEOUT: Duration = Duration::from_secs(60);

/// The texture format rendered into. egui outputs gamma-encoded colour into a non-sRGB target.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The number of samples per pixel used for anti-aliasing, when the adapter supports it.
const MSAA_SAMPLES: u32 = 4;

/// The first user texture id under which the tiles of images are uploaded. `egui_wgpu::Renderer` numbers the
/// textures it registers itself from 0, so ids counted from here can never collide with them.
const TILE_TEXTURE_BASE: u64 = 1 << 32;

/// The texture provider of one offscreen render: it uploads every tile it is asked for as a user texture of the
/// renderer and records the ids, so that the textures can be freed once the render is read back.
struct TileUploader<'a> {
    renderer: &'a mut egui_wgpu::Renderer,
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    /// The device's largest texture side, capped at [`MAX_TILE_SIDE`].
    max_side: u32,
    /// The ids of the textures uploaded so far, in order.
    ids: Vec<egui::TextureId>,
}

impl TextureProvider for TileUploader<'_> {
    fn max_side(&self) -> u32 {
        self.max_side
    }

    fn texture(
        &mut self,
        _samples: &Arc<[u8]>,
        _tile: u32,
        render: &mut dyn FnMut() -> egui::ColorImage,
    ) -> egui::TextureId {
        let id = egui::TextureId::User(TILE_TEXTURE_BASE + self.ids.len() as u64);
        self.renderer.update_texture(
            self.device,
            self.queue,
            id,
            &egui::epaint::ImageDelta::full(render(), egui::TextureOptions::NEAREST),
        );
        self.ids.push(id);
        id
    }
}

/// A headless renderer that owns a wgpu device and draws display lists into images.
///
/// Creating the device is by far the most expensive step of an offscreen render, so a caller rendering many images
/// should create one renderer and reuse it. [`render_display_list_offscreen`] does this with a process-wide instance.
pub struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
    sample_count: u32,
}

impl OffscreenRenderer {
    /// Creates a device on the first available adapter and prepares an egui renderer on it.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::NoAdapter`] when no adapter is available and [`RenderError::Device`] when the adapter
    /// cannot create a device.
    pub fn new() -> Result<Self, RenderError> {
        let setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
        let instance =
            pollster::block_on(egui_wgpu::WgpuSetup::CreateNew(setup.clone()).new_instance());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: setup.power_preference,
            ..Default::default()
        }))
        .map_err(|error| {
            RenderError::NoAdapter(format!(
                "{error} (backends {:?})",
                setup.instance_descriptor.backends
            ))
        })?;
        let adapter_limits = adapter.limits();
        let (device, queue) =
            pollster::block_on(
                adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("ironlab offscreen device"),
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter_limits.clone()),
                    ..Default::default()
                }),
            )
            .map_err(|error| RenderError::Device(error.to_string()))?;

        let sample_count = if adapter
            .get_texture_format_features(FORMAT)
            .flags
            .sample_count_supported(MSAA_SAMPLES)
        {
            MSAA_SAMPLES
        } else {
            1
        };
        let mut renderer = egui_wgpu::Renderer::new(
            &device,
            FORMAT,
            egui_wgpu::RendererOptions {
                msaa_samples: sample_count,
                depth_stencil_format: Some(DEPTH_FORMAT),
                dithering: false,
                predictable_texture_filtering: false,
            },
        );
        renderer.update_texture(
            &device,
            &queue,
            egui::TextureId::Managed(0),
            &egui::epaint::ImageDelta::full(
                egui::ColorImage::new([1, 1], vec![egui::Color32::WHITE]),
                egui::TextureOptions::NEAREST,
            ),
        );
        Ok(Self {
            device,
            queue,
            renderer,
            sample_count,
        })
    }

    /// Compiles `figure` and renders it at `dpi` dots per inch.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::InvalidSize`] when the image size is invalid for the device and
    /// [`RenderError::Readback`] when rendering or readback fails.
    pub fn render(
        &mut self,
        figure: &Figure,
        text: &TextEngine,
        dpi: f64,
    ) -> Result<RenderedImage, RenderError> {
        let scene = ironlab_scene::compile(figure, text);
        self.render_display_list(&scene.display_list, text, dpi)
    }

    /// Renders an already compiled display list at `dpi` dots per inch.
    ///
    /// # Errors
    ///
    /// As for [`OffscreenRenderer::render`].
    pub fn render_display_list(
        &mut self,
        list: &DisplayList,
        text: &TextEngine,
        dpi: f64,
    ) -> Result<RenderedImage, RenderError> {
        let max = self.device.limits().max_texture_dimension_2d;
        let pixels = |points: f64| {
            let value = (points * dpi / 72.0).round();
            if value.is_finite() && value > 0.0 {
                value.min(f64::from(u32::MAX)) as u32
            } else {
                0
            }
        };
        let (width, height) = (pixels(list.width_pt), pixels(list.height_pt));
        if width == 0 || height == 0 || width > max || height > max {
            return Err(RenderError::InvalidSize { width, height, max });
        }

        let scale = (dpi / 72.0) as f32;
        // The error scopes are opened before the tessellation so that they cover the texture uploads it makes as
        // well as the draw; an upload the device refuses is then reported rather than raised as an uncaptured error.
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let out_of_memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let mut meshes = Vec::new();
        if let Some(background) = background_mesh(list, scale) {
            meshes.push(background);
        }
        let mut uploader = TileUploader {
            renderer: &mut self.renderer,
            device: &self.device,
            queue: &self.queue,
            max_side: max.min(MAX_TILE_SIDE),
            ids: Vec::new(),
        };
        let drawables = drawables_with(
            list,
            text,
            ScreenTransform {
                scale,
                origin: egui::Pos2::ZERO,
            },
            &mut uploader,
        );
        let tile_textures = uploader.ids;
        let screen_rect =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32));
        let config = GpuConfig {
            target_format: FORMAT,
            samples: self.sample_count,
            depth_format: DEPTH_FORMAT,
        };
        let primitives: Vec<egui::ClippedPrimitive> = meshes
            .into_iter()
            .map(Drawable::Mesh)
            .chain(drawables)
            .map(|drawable| egui::ClippedPrimitive {
                clip_rect: screen_rect,
                primitive: match drawable {
                    Drawable::Mesh(mesh) => egui::epaint::Primitive::Mesh(mesh),
                    // The callback covers the whole image, so that its vertex mapping is the whole target's; every
                    // draw of the list clips itself.
                    Drawable::Gpu(list) => {
                        egui::epaint::Primitive::Callback(egui_wgpu::Callback::new_paint_callback(
                            screen_rect,
                            GpuCallback { list, config },
                        ))
                    }
                },
            })
            .collect();
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: 1.0,
        };

        let rendered = self.draw(&primitives, &screen);
        for id in &tile_textures {
            self.renderer.free_texture(id);
        }
        if let Some(painter) = self.renderer.callback_resources.get_mut::<GpuPainter>() {
            painter.clear();
        }
        let oom = pollster::block_on(out_of_memory.pop());
        let invalid = pollster::block_on(validation.pop());
        if let Some(error) = oom.or(invalid) {
            return Err(RenderError::Readback(error.to_string()));
        }
        let mut rgba = rendered?;
        unpremultiply(&mut rgba);
        Ok(RenderedImage {
            width,
            height,
            rgba,
        })
    }

    /// Draws the primitives into a new texture and reads the result back as premultiplied RGBA bytes.
    fn draw(
        &mut self,
        primitives: &[egui::ClippedPrimitive],
        screen: &egui_wgpu::ScreenDescriptor,
    ) -> Result<Vec<u8>, RenderError> {
        let [width, height] = screen.size_in_pixels;
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = |label, sample_count, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage,
                view_formats: &[],
            })
        };
        let resolved = texture(
            "ironlab offscreen resolved",
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let resolved_view = resolved.create_view(&wgpu::TextureViewDescriptor::default());
        let multisampled = (self.sample_count > 1).then(|| {
            texture(
                "ironlab offscreen multisampled",
                self.sample_count,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
        });
        let multisampled_view = multisampled
            .as_ref()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
        let (view, resolve_target) = match &multisampled_view {
            Some(ms) => (ms, Some(&resolved_view)),
            None => (&resolved_view, None),
        };
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ironlab offscreen depth"),
            size,
            mip_level_count: 1,
            sample_count: self.sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ironlab offscreen encoder"),
            });
        let user_buffers = self.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            primitives,
            screen,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ironlab offscreen pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, primitives, screen);
        }

        let unpadded_bytes_per_row = width as usize * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ironlab offscreen readback"),
            size: (padded_bytes_per_row * height as usize) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            resolved.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row as u32),
                    rows_per_image: None,
                },
            },
            size,
        );
        let submission = self.queue.submit(
            user_buffers
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(WAIT_TIMEOUT),
            })
            .map_err(|error| RenderError::Readback(error.to_string()))?;
        receiver
            .recv_timeout(WAIT_TIMEOUT)
            .map_err(|error| RenderError::Readback(error.to_string()))?
            .map_err(|error| RenderError::Readback(error.to_string()))?;
        let data = slice
            .get_mapped_range()
            .map_err(|error| RenderError::Readback(error.to_string()))?;
        let mut rgba = Vec::with_capacity(unpadded_bytes_per_row * height as usize);
        for row in data.chunks_exact(padded_bytes_per_row) {
            rgba.extend_from_slice(&row[..unpadded_bytes_per_row]);
        }
        drop(data);
        buffer.unmap();
        Ok(rgba)
    }
}

/// A rectangle covering the page in the display list's background colour, in pixels, or `None` when the background
/// is fully transparent or invalid.
fn background_mesh(list: &DisplayList, scale: f32) -> Option<egui::Mesh> {
    let color = color32(list.background)?;
    if color.a() == 0 {
        return None;
    }
    let size = egui::vec2(list.width_pt as f32 * scale, list.height_pt as f32 * scale);
    let mut mesh = egui::Mesh::default();
    mesh.add_colored_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, size), color);
    Some(mesh)
}

/// Converts premultiplied RGBA bytes to straight alpha in place. Fully transparent pixels become transparent black.
fn unpremultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = pixel[3];
        match alpha {
            0 => pixel[..3].fill(0),
            255 => {}
            _ => {
                for channel in &mut pixel[..3] {
                    let straight =
                        (u32::from(*channel) * 255 + u32::from(alpha) / 2) / u32::from(alpha);
                    *channel = straight.min(255) as u8;
                }
            }
        }
    }
}
