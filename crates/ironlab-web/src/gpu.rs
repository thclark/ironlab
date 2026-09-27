//! The graphics side of the browser host: the device a session or a canvas draws with, and the surface of one canvas
//! with the render targets a frame is drawn through.
//!
//! A frame is drawn exactly as the offscreen renderer draws an image: the draw list is prepared on the
//! [`GpuPainter`], the pass is begun by [`figure_pass`] on a multisampled colour target (when the adapter supports
//! four samples) that resolves into the surface's texture, with a depth attachment of [`DEPTH_FORMAT`], and the
//! painter paints into it. The surface format is the first format the adapter offers that is not sRGB, because the
//! pipelines output gamma-encoded colour into a non-sRGB target, as they do offscreen.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use ironlab_canvas::offscreen::{RenderError, figure_pass, request_device};
use ironlab_canvas::{DEPTH_FORMAT, DrawList, GpuConfig, GpuPainter, Viewport};

use crate::error::WebError;

/// A graphics device, its queue and the painter that draws with it.
///
/// Under WebGPU a session creates one of these and every canvas shares it, so pipelines are compiled once and a draw
/// list is uploaded once; under WebGL 2 each canvas has its own, because a WebGL adapter is the context of one canvas.
#[derive(Clone)]
pub struct Gpu {
    /// The adapter the device was created from, which a surface is configured against.
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// The painter shared by every canvas drawing through this device.
    pub painter: Rc<RefCell<GpuPainter>>,
    /// The samples per pixel frames are drawn with: 4 when the adapter supports it, else 1.
    pub sample_count: u32,
    /// The last error the device reported outside an error scope, which the next frame reports to the page.
    uncaptured: Arc<Mutex<Option<String>>>,
}

impl Gpu {
    /// Creates a device on `adapter`, awaiting it.
    ///
    /// The device's uncaptured errors are logged to the console and kept for the next frame to report, rather than
    /// raised as the panic wgpu raises by default, which would stop the module.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Device`] when the adapter cannot create a device.
    pub async fn new(adapter: wgpu::Adapter) -> Result<Self, RenderError> {
        let (device, queue, sample_count) = request_device(&adapter).await?;
        let uncaptured = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&uncaptured);
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            let message = error.to_string();
            web_sys::console::error_1(&format!("ironlab-web: {message}").into());
            *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(message);
        }));
        Ok(Self {
            adapter,
            device,
            queue,
            painter: Rc::default(),
            sample_count,
            uncaptured,
        })
    }

    /// The name of the backend the device runs on: `webgpu` or `webgl2`.
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        match self.adapter.get_info().backend {
            wgpu::Backend::BrowserWebGpu => "webgpu",
            _ => "webgl2",
        }
    }

    /// The largest texture side the device allows, which bounds the tiles of an image.
    #[must_use]
    pub fn max_texture_side(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    /// Takes the last error the device reported outside an error scope, if there is one.
    fn take_uncaptured(&self) -> Option<String> {
        self.uncaptured
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

/// The multisampled colour target and the depth target of a surface, at the surface's size.
struct Targets {
    /// The multisampled colour texture the pass draws into before resolving to the surface, or `None` when frames
    /// are drawn with one sample straight into the surface.
    colour: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
}

impl Targets {
    fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration, sample_count: u32) -> Self {
        let size = wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };
        let texture = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size,
                    mip_level_count: 1,
                    sample_count,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        Self {
            colour: (sample_count > 1)
                .then(|| texture("ironlab canvas multisampled", config.format)),
            depth: texture("ironlab canvas depth", DEPTH_FORMAT),
        }
    }
}

/// The surface of one canvas: its configuration, and the render targets of its size.
pub struct Screen {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// Whether the surface has been configured, which the first `resize` does; a frame cannot be drawn before it.
    configured: bool,
    targets: Option<Targets>,
}

impl Screen {
    /// Creates the surface of `canvas` on `instance` and chooses its configuration against `adapter`: the first
    /// non-sRGB format the adapter offers, an opaque alpha mode, first-in-first-out presentation and a frame latency
    /// of two. The surface is configured when it is first given a size.
    ///
    /// # Errors
    ///
    /// Returns [`WebError::Surface`] when the surface cannot be created or the adapter cannot draw to it.
    pub fn new(
        instance: &wgpu::Instance,
        canvas: web_sys::HtmlCanvasElement,
        adapter: &wgpu::Adapter,
    ) -> Result<Self, WebError> {
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|error| WebError::Surface(error.to_string()))?;
        let capabilities = surface.get_capabilities(adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| {
                WebError::Surface("the adapter offers no texture format for the canvas".to_owned())
            })?;
        let mut config = surface.get_default_config(adapter, 1, 1).ok_or_else(|| {
            WebError::Surface("the adapter cannot present to the canvas".to_owned())
        })?;
        config.format = format;
        config.alpha_mode = if capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::Opaque)
        {
            wgpu::CompositeAlphaMode::Opaque
        } else {
            capabilities
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto)
        };
        config.present_mode = wgpu::PresentMode::Fifo;
        config.desired_maximum_frame_latency = 2;
        Ok(Self {
            surface,
            config,
            configured: false,
            targets: None,
        })
    }

    /// Configures the surface for `width` by `height` device pixels (at least one each) and drops the render targets,
    /// which are recreated at the new size by the next frame.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(device, &self.config);
        self.configured = true;
        self.targets = None;
    }

    /// The size of the surface in device pixels.
    #[must_use]
    pub fn size(&self) -> [u32; 2] {
        [self.config.width, self.config.height]
    }

    /// Acquires the surface's next texture: `None` when the frame should be skipped (a timeout, or an occluded
    /// canvas), the texture otherwise. A lost or outdated surface is configured again and tried once more.
    fn acquire(&mut self, device: &wgpu::Device) -> Result<Option<wgpu::SurfaceTexture>, WebError> {
        for attempt in 0..2 {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => return Ok(Some(texture)),
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(None);
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost
                    if attempt == 0 =>
                {
                    self.surface.configure(device, &self.config);
                    self.targets = None;
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    return Err(WebError::Frame("the surface is outdated".to_owned()));
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    return Err(WebError::Frame("the surface was lost".to_owned()));
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(WebError::Frame(
                        "the surface configuration is invalid".to_owned(),
                    ));
                }
            }
        }
        Ok(None)
    }

    /// Draws one frame: clears the canvas to `clear` (premultiplied sRGB bytes) and paints `list`, when there is
    /// one, at `viewport`, then presents it. A frame the surface cannot provide right now is skipped without error.
    ///
    /// # Errors
    ///
    /// Returns [`WebError::NotSized`] before the first `resize`, [`WebError::Frame`] when the surface cannot provide a
    /// texture, and [`WebError::Device`] when the device reported an error since the last frame.
    pub fn frame(
        &mut self,
        gpu: &Gpu,
        list: Option<&Arc<DrawList>>,
        viewport: &Viewport,
        clear: [u8; 4],
    ) -> Result<(), WebError> {
        if !self.configured {
            return Err(WebError::NotSized);
        }
        let Some(texture) = self.acquire(&gpu.device)? else {
            return Ok(());
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let targets = self
            .targets
            .get_or_insert_with(|| Targets::new(&gpu.device, &self.config, gpu.sample_count));
        let config = GpuConfig {
            target_format: self.config.format,
            samples: gpu.sample_count,
            depth_format: DEPTH_FORMAT,
        };
        let mut painter = gpu.painter.borrow_mut();
        if let Some(list) = list {
            painter.prepare(&gpu.device, &gpu.queue, config, list, viewport);
        }
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ironlab canvas encoder"),
            });
        {
            let (colour, resolve) = match &targets.colour {
                Some(multisampled) => (multisampled, Some(&view)),
                None => (&view, None),
            };
            let mut pass = figure_pass(&mut encoder, colour, resolve, &targets.depth, clear);
            if let Some(list) = list {
                painter.paint(&mut pass, viewport, config, list);
            }
        }
        gpu.queue.submit(std::iter::once(encoder.finish()));
        gpu.queue.present(texture);
        match gpu.take_uncaptured() {
            Some(message) => Err(WebError::Device(message)),
            None => Ok(()),
        }
    }
}
