//! The browser host of IronLAB figures.
//!
//! This crate compiles the figure canvas of [`ironlab_canvas`] to WebAssembly and exposes it to JavaScript through
//! `wasm_bindgen`, so that a web page shows a `.fig` or `.json` figure on a `<canvas>` and pans, zooms, rotates,
//! reads, saves and exports it exactly as the desktop viewer does, through the same engine. The JavaScript element
//! `<ironlab-figure>` in `www/ironlab.js` is the page-side half: it fetches the file, owns the canvas and the toolbar,
//! and forwards every pointer event here.
//!
//! The surface a page drives is two classes:
//!
//! - `Session` (in the `session` module) is created once per page. It finds the graphics backend the browser has (WebGPU,
//!   else WebGL 2), owns the text engine the figures are typeset with, and opens figures on canvases. A headless
//!   session, with no graphics device, opens figures that can be edited, saved and exported but not drawn.
//! - `FigureHandle` (in the `figure` module) is one open figure. Its input methods take raw pointer events in CSS pixels
//!   and answer each with an [`Outcome`], a plain object that tells the page what its chrome should now show; its
//!   `render()` draws a frame on the canvas; and `save()` and `export_pdf()` hand back the bytes of a download.
//!
//! The [`outcome`] module is ordinary Rust, compiled on every target, so that the shape of what the page reads is
//! proved by native tests without a browser; the rest of the crate is compiled for `wasm32-unknown-unknown` alone.

pub mod outcome;

#[cfg(target_arch = "wasm32")]
pub mod error;
#[cfg(target_arch = "wasm32")]
pub mod figure;
#[cfg(target_arch = "wasm32")]
pub mod gpu;
#[cfg(target_arch = "wasm32")]
pub mod session;

pub use outcome::{Datatip, Marker, Outcome, download_stem};

#[cfg(target_arch = "wasm32")]
pub use error::WebError;
#[cfg(target_arch = "wasm32")]
pub use figure::FigureHandle;
#[cfg(target_arch = "wasm32")]
pub use session::Session;

/// Runs when the module is instantiated: a panic is reported to the browser console with its message rather than
/// as the bare `unreachable` trap a module reports by default.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}
