//! What can go wrong in the browser host, as one error type that becomes a JavaScript `Error` at the boundary.

use ironlab_canvas::ExportError;
use ironlab_canvas::offscreen::RenderError;
use ironlab_ir::IrError;

/// A failure of the browser host. Every variant becomes a JavaScript `Error` whose message is the variant's text,
/// through wasm-bindgen's conversion of any `Error` into a `JsError`, which a page shows in its status line.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    /// The name given for a figure's format is neither `fig` nor `json` nor a file name ending in one of them.
    #[error("{0:?} names no figure format; expected \"fig\" (Protocol Buffers) or \"json\"")]
    UnknownFormat(String),
    /// The bytes are not a figure in the named format.
    #[error("the bytes are not a {format} figure: {source}")]
    Decode {
        /// The format the bytes were read as.
        format: &'static str,
        /// The IR's own error.
        source: IrError,
    },
    /// The browser has neither WebGPU nor WebGL 2, so no figure can be drawn.
    #[error("this browser has neither WebGPU nor WebGL2, so figures cannot be drawn: {0}")]
    NoBackend(String),
    /// The session has no graphics device, so it cannot open a figure on a canvas.
    #[error("the session has no graphics device, so it cannot open a figure on a canvas")]
    Headless,
    /// The canvas has no drawing surface.
    #[error("a drawing surface could not be created on the canvas: {0}")]
    Surface(String),
    /// The handle has no surface to render to, because it was opened headless or released.
    #[error("the figure has no canvas to render to")]
    NoSurface,
    /// `render()` was called before `resize()` gave the surface a size.
    #[error("the figure cannot render before resize() has given its canvas a size")]
    NotSized,
    /// The surface's texture could not be acquired for a frame.
    #[error("the canvas surface could not provide a frame: {0}")]
    Frame(String),
    /// The graphics device reported an error while a frame was drawn.
    #[error("the graphics device reported an error while drawing: {0}")]
    Device(String),
    /// The adapter or device could not be created.
    #[error(transparent)]
    Render(#[from] RenderError),
    /// The PDF could not be exported.
    #[error(transparent)]
    Export(#[from] ExportError),
}
