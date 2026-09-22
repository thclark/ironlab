//! Headless rendering of figures through the viewer's own pipelines.
//!
//! [`render_offscreen`] compiles a figure, tessellates its display list with [`crate::canvas::tessellate`] for
//! `dpi / 72` pixels per point, and draws the list with a [`GpuPainter`] into an offscreen texture. No window or
//! surface is created, so it runs in CI on a software adapter (for example lavapipe), and the pipelines, buffers
//! and textures are the ones the interactive window draws with.
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
//! 2. The image size is checked against the device's maximum texture dimension before any texture is created, and
//!    an invalid size is reported as [`RenderError::InvalidSize`].
//! 3. The display list is tessellated into one draw list in figure points, with image tiles no larger than the
//!    device's maximum texture dimension capped at [`MAX_TILE_SIDE`], and the painter uploads its buffers, its
//!    mapping and its tiles.
//! 4. The list is drawn in one render pass, for `Rgba8Unorm` (the pipelines blend in gamma space and output
//!    gamma-encoded colour into a non-sRGB target) with 4× multisampling when the adapter supports it, into a
//!    multisampled colour texture that is cleared to the list's background colour, premultiplied, and resolved
//!    into a single-sample `COPY_SRC` texture, with a depth attachment of [`DEPTH_FORMAT`] cleared to the far
//!    plane. Clearing rather than drawing the background covers every pixel, including the last row or column of
//!    an image whose size rounds up from the page's, and a transparent background stays transparent. The mapping
//!    is `dpi / 72` pixels per figure point from the top-left corner, and the clip is the whole image.
//! 5. The resolved texture is copied into a `MAP_READ` buffer with rows padded to
//!    `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`, the device is polled until the copy completes, and the padding is
//!    stripped. The painter is then emptied, whether or not the draw and the readback succeeded, so that no render
//!    leaves buffers or textures on the device. The GPU output has premultiplied alpha, which is converted to
//!    straight alpha.
//!
//! The pixel size of the image is `round(width_pt · dpi / 72)` by `round(height_pt · dpi / 72)`. Two renders of
//! one list are identical.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use egui_wgpu::wgpu;
use ironlab_ir::Figure;
use ironlab_scene::display::DisplayList;
use ironlab_text::TextEngine;

use crate::canvas::{MAX_TILE_SIDE, Resolution, ScreenTransform, premultiplied, tessellate};
use crate::gpu::{DEPTH_FORMAT, DrawList, GpuConfig, GpuPainter, Viewport};

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

/// The texture format rendered into. The pipelines output gamma-encoded colour into a non-sRGB target.
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
    painter: GpuPainter,
    sample_count: u32,
}

impl OffscreenRenderer {
    /// Creates a device on the first available adapter.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::NoAdapter`] when no adapter is available and [`RenderError::Device`] when the adapter
    /// cannot create a device.
    pub fn new() -> Result<Self, RenderError> {
        let (device, queue, sample_count) = create_device()?;
        Ok(Self {
            device,
            queue,
            painter: GpuPainter::default(),
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
        let background = premultiplied(list.background).unwrap_or([0, 0, 0, 0]);
        let list = Arc::new(tessellate(
            list,
            text,
            Resolution {
                scale,
                max_tile_side: max.min(MAX_TILE_SIDE),
            },
        ));
        let viewport = Viewport::whole(
            [width, height],
            1.0,
            ScreenTransform {
                scale,
                origin: egui::Pos2::ZERO,
            },
        );
        self.render_list(&list, &viewport, background)
    }

    /// Draws a list at `viewport` over `background` (premultiplied sRGB bytes) into an image of the viewport's
    /// size and reads it back. This is the pass every render takes; [`render_display_list`](Self::render_display_list)
    /// builds the list and the viewport for a resolution, and a caller with a list of its own places it anywhere
    /// on the target.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::InvalidSize`] when the viewport's size is invalid for the device and
    /// [`RenderError::Readback`] when rendering or readback fails.
    pub fn render_list(
        &mut self,
        list: &Arc<DrawList>,
        viewport: &Viewport,
        background: [u8; 4],
    ) -> Result<RenderedImage, RenderError> {
        let max = self.device.limits().max_texture_dimension_2d;
        let [width, height] = viewport.size_px;
        if width == 0 || height == 0 || width > max || height > max {
            return Err(RenderError::InvalidSize { width, height, max });
        }
        let config = GpuConfig {
            target_format: FORMAT,
            samples: self.sample_count,
            depth_format: DEPTH_FORMAT,
        };
        // The error scopes cover the uploads as well as the draw; an upload the device refuses is then reported
        // rather than raised as an uncaptured error.
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let out_of_memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        self.painter
            .prepare(&self.device, &self.queue, config, list, viewport);
        let rendered = self.draw(list, viewport, config, background);
        self.painter.clear();
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

    /// Draws a prepared list over `background` into a new texture and reads the result back as premultiplied RGBA
    /// bytes.
    fn draw(
        &mut self,
        list: &Arc<DrawList>,
        viewport: &Viewport,
        config: GpuConfig,
        background: [u8; 4],
    ) -> Result<Vec<u8>, RenderError> {
        let [width, height] = viewport.size_px;
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
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ironlab offscreen pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: f64::from(background[0]) / 255.0,
                                g: f64::from(background[1]) / 255.0,
                                b: f64::from(background[2]) / 255.0,
                                a: f64::from(background[3]) / 255.0,
                            }),
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
            self.painter.paint(&mut pass, viewport, config, list);
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
        let submission = self.queue.submit(std::iter::once(encoder.finish()));

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

/// Creates a device and queue on the first available adapter, with the sample count the adapter supports for
/// [`FORMAT`].
///
/// # Errors
///
/// Returns [`RenderError::NoAdapter`] when no adapter is available and [`RenderError::Device`] when the adapter
/// cannot create a device.
pub fn create_device() -> Result<(wgpu::Device, wgpu::Queue, u32), RenderError> {
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
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("ironlab offscreen device"),
        required_limits:
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter_limits.clone()),
        ..Default::default()
    }))
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
    Ok((device, queue, sample_count))
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
