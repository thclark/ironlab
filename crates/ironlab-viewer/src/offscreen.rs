//! Headless rendering of figures through the viewer's own mesh pipeline.
//!
//! [`render_offscreen`] compiles a figure, tessellates its display list with [`crate::canvas::tessellate`] at
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
//!    colour into a non-sRGB target) with 4× multisampling (when the adapter supports it) and dithering disabled,
//!    so that repeated renders of the same figure are identical.
//! 3. The default egui texture (`TextureId::Managed(0)`) is uploaded as a 1×1 white image, because the meshes sample
//!    it at [`egui::epaint::WHITE_UV`].
//! 4. The image size is checked against the device's maximum texture dimension before any texture is created, and
//!    an invalid size is reported as [`RenderError::InvalidSize`].
//! 5. The figure background is painted as a rectangle mesh beneath the meshes of the display list. The meshes are
//!    wrapped in `egui::ClippedPrimitive`s whose clip rectangle is the whole image, and `Renderer::update_buffers`
//!    and `Renderer::render` draw them with `ScreenDescriptor { size_in_pixels: [width, height], pixels_per_point:
//!    1.0 }` into a multisampled colour texture that is cleared to transparent black and resolved into a
//!    single-sample `COPY_SRC` texture. A transparent background therefore stays transparent.
//! 6. The resolved texture is copied into a `MAP_READ` buffer with rows padded to
//!    `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`, the device is polled until the copy completes, and the padding is
//!    stripped. The GPU output has premultiplied alpha, which is converted to straight alpha.
//!
//! The pixel size of the image is `round(width_pt · dpi / 72)` by `round(height_pt · dpi / 72)`.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use egui_wgpu::wgpu;
use ironlab_ir::Figure;
use ironlab_scene::display::DisplayList;
use ironlab_text::TextEngine;

use crate::canvas::{ScreenTransform, color32, tessellate};

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
    #[error("no graphics adapter is available for offscreen rendering: {0}")]
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
    static SHARED: Mutex<Option<OffscreenRenderer>> = Mutex::new(None);
    let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
    if shared.is_none() {
        *shared = Some(OffscreenRenderer::new()?);
    }
    let renderer = shared
        .as_mut()
        .expect("the shared renderer was just created");
    let result = renderer.render_display_list(list, text, dpi);
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
                depth_stencil_format: None,
                dithering: false,
                predictable_texture_filtering: true,
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
        let mut meshes = Vec::new();
        if let Some(background) = background_mesh(list, scale) {
            meshes.push(background);
        }
        meshes.extend(tessellate(
            list,
            text,
            ScreenTransform {
                scale,
                origin: egui::Pos2::ZERO,
            },
        ));
        let screen_rect =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32));
        let primitives: Vec<egui::ClippedPrimitive> = meshes
            .into_iter()
            .map(|mesh| egui::ClippedPrimitive {
                clip_rect: screen_rect,
                primitive: egui::epaint::Primitive::Mesh(mesh),
            })
            .collect();
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: 1.0,
        };

        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let out_of_memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let rendered = self.draw(&primitives, &screen);
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
