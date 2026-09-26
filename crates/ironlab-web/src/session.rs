//! The session of a page: the graphics backend the browser has, the text engine every figure is typeset with, and
//! the opening of figures on canvases.

use std::rc::Rc;

use ironlab_canvas::files::Format;
use ironlab_canvas::offscreen::{new_instance, request_adapter};
use ironlab_text::TextEngine;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::HtmlCanvasElement;

use crate::error::WebError;
use crate::figure::FigureHandle;
use crate::gpu::{Gpu, Screen};

thread_local! {
    /// The one text engine of the module. Building it parses the bundled fonts, so every session shares it.
    static TEXT: Rc<TextEngine> = Rc::new(TextEngine::new());
}

/// The module's text engine.
#[must_use]
pub fn text_engine() -> Rc<TextEngine> {
    TEXT.with(Rc::clone)
}

/// The graphics backend a session found.
enum Backend {
    /// WebGPU: one device, shared by every canvas.
    WebGpu(Gpu),
    /// WebGL 2: each canvas creates its own device from its own context.
    WebGl2,
    /// No graphics device: figures open headless and cannot be drawn.
    None,
}

/// A page's session: created once, it opens every figure on the page.
///
/// [`Session::create`] finds the graphics backend the browser has, WebGPU where it is available and WebGL 2
/// otherwise, and fails when it has neither. [`Session::headless`] has no graphics device at all: figures opened on
/// it can be edited, saved and exported, but not drawn, so that a page on a machine without a graphics backend still
/// gives its reader the figure's data and a PDF.
#[wasm_bindgen]
pub struct Session {
    instance: Option<wgpu::Instance>,
    backend: Backend,
    text: Rc<TextEngine>,
}

#[wasm_bindgen]
impl Session {
    /// Creates a session on the graphics backend the browser has: WebGPU where it is available, else WebGL 2.
    ///
    /// Under WebGPU one device is created here and shared by every canvas; under WebGL 2 a device belongs to a
    /// canvas, so only the availability of a context is proved here, on a canvas that is never shown.
    ///
    /// # Errors
    ///
    /// Rejects, naming the missing capability, when the browser has neither WebGPU nor WebGL 2.
    pub async fn create() -> Result<Session, JsError> {
        let instance = new_instance().await;
        let text = text_engine();
        if let Ok(adapter) = request_adapter(&instance, None).await
            && adapter.get_info().backend == wgpu::Backend::BrowserWebGpu
        {
            let gpu = Gpu::new(adapter).await.map_err(WebError::from)?;
            return Ok(Self {
                instance: Some(instance),
                backend: Backend::WebGpu(gpu),
                text,
            });
        }
        // A WebGL adapter is the context of a canvas, so a canvas that is never attached to the document is what
        // proves that one can be had.
        let probe = probe_canvas()?;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(probe))
            .map_err(|error| WebError::NoBackend(error.to_string()))?;
        request_adapter(&instance, Some(&surface))
            .await
            .map_err(|error| WebError::NoBackend(error.to_string()))?;
        drop(surface);
        Ok(Self {
            instance: Some(instance),
            backend: Backend::WebGl2,
            text,
        })
    }

    /// A session with a text engine and no graphics device: `open_headless` works, `open` fails, and `backend()` is
    /// `none`.
    #[must_use]
    pub fn headless() -> Session {
        Self {
            instance: None,
            backend: Backend::None,
            text: text_engine(),
        }
    }

    /// The version of the crate compiled into the module.
    #[must_use]
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    /// The graphics backend: `webgpu`, `webgl2` or `none`.
    #[must_use]
    pub fn backend(&self) -> String {
        match &self.backend {
            Backend::WebGpu(_) => "webgpu",
            Backend::WebGl2 => "webgl2",
            Backend::None => "none",
        }
        .to_owned()
    }

    /// Opens the figure encoded in `bytes` as `format` (`fig`, `json`, or a file name ending in `.fig` or `.json`)
    /// on `canvas`, creating a drawing surface on it. The handle has no size until its `resize` is called.
    ///
    /// # Errors
    ///
    /// Rejects when the session has no graphics device, when `format` names no format or the bytes are not a figure
    /// in it, or when no surface can be created on the canvas.
    pub async fn open(
        &self,
        canvas: HtmlCanvasElement,
        bytes: Vec<u8>,
        format: String,
    ) -> Result<FigureHandle, JsError> {
        let (format, figure) = decode(&bytes, &format)?;
        let instance = self.instance.as_ref().ok_or(WebError::Headless)?;
        let (gpu, screen) = match &self.backend {
            Backend::None => return Err(WebError::Headless.into()),
            Backend::WebGpu(gpu) => (gpu.clone(), Screen::new(instance, canvas, &gpu.adapter)?),
            Backend::WebGl2 => {
                // The adapter is the canvas's own context, so the surface comes first and the device follows it.
                let surface = instance
                    .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
                    .map_err(|error| WebError::Surface(error.to_string()))?;
                let adapter = request_adapter(instance, Some(&surface))
                    .await
                    .map_err(WebError::from)?;
                drop(surface);
                let gpu = Gpu::new(adapter).await.map_err(WebError::from)?;
                let screen = Screen::new(instance, canvas, &gpu.adapter)?;
                (gpu, screen)
            }
        };
        Ok(FigureHandle::new(
            figure,
            format,
            Rc::clone(&self.text),
            Some((gpu, screen)),
        ))
    }

    /// Opens the figure encoded in `bytes` as `format` with no canvas: the handle can be sized, driven, saved and
    /// exported, and its `render` is an error.
    ///
    /// # Errors
    ///
    /// Throws when `format` names no format or the bytes are not a figure in it.
    pub fn open_headless(&self, bytes: &[u8], format: &str) -> Result<FigureHandle, JsError> {
        let (format, figure) = decode(bytes, format)?;
        Ok(FigureHandle::new(
            figure,
            format,
            Rc::clone(&self.text),
            None,
        ))
    }
}

/// Decodes `bytes` as the figure format named by `name`.
fn decode(bytes: &[u8], name: &str) -> Result<(Format, ironlab_ir::Figure), WebError> {
    let format = if name.eq_ignore_ascii_case("fig") {
        Format::Fig
    } else if name.eq_ignore_ascii_case("json") {
        Format::Json
    } else {
        Format::from_name(name).ok_or_else(|| WebError::UnknownFormat(name.to_owned()))?
    };
    let figure = format.decode(bytes).map_err(|source| WebError::Decode {
        format: format.extension(),
        source,
    })?;
    Ok((format, figure))
}

/// A canvas element that is never attached to the document, for proving that a WebGL 2 context can be had.
fn probe_canvas() -> Result<HtmlCanvasElement, WebError> {
    use wasm_bindgen::JsCast;
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.create_element("canvas").ok())
        .and_then(|element| element.dyn_into::<HtmlCanvasElement>().ok())
        .ok_or_else(|| WebError::NoBackend("no document to create a canvas in".to_owned()))
}
