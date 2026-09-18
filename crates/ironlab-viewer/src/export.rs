//! PDF export through the viewer's own renderer.
//!
//! [`ironlab_pdf`] writes vector geometry and text, and asks a [`Rasteriser`] for the parts of a figure that are too
//! dense to be worth writing as vector paths. This module supplies that rasteriser: [`GpuRasteriser`] wraps the
//! viewer's headless renderer, so the pixels embedded in a PDF come from the same device, the same tessellation and
//! the same shaders that draw the interactive canvas. There is no second rasteriser, and therefore nothing that can
//! drift from what the user inspected on screen.
//!
//! [`export_pdf`] and [`write_pdf`] are the export path the whole project uses: the viewer's "Export PDF…" command,
//! the `ironlab` crate's [`Figure::export_pdf`](../../ironlab/struct.Figure.html) and the documentation gallery.
//!
//! A figure with nothing dense in it is exported without ever touching the GPU, so exporting on a machine with no
//! graphics adapter works as it always has. Only a figure that must be rasterised needs an adapter, and when none is
//! available that is reported as [`ExportError::Render`] rather than quietly exported as something else.

use std::path::Path;

use ironlab_ir::Figure;
use ironlab_pdf::{PdfError, PdfOptions, RasterImage, Rasteriser};
use ironlab_scene::display::DisplayList;
use ironlab_text::TextEngine;

use crate::offscreen::{OffscreenRenderer, RenderError, with_shared_renderer};

/// A failure to export a figure as a PDF.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// The PDF could not be written.
    #[error(transparent)]
    Pdf(#[from] PdfError),
    /// The figure has content that the raster policy rasterises, and the renderer could not be created or could not
    /// draw it.
    #[error("the figure has content that must be rasterised, but {0}")]
    Render(#[from] RenderError),
}

/// The viewer's headless renderer, presented to the PDF exporter as a [`Rasteriser`].
pub struct GpuRasteriser<'a> {
    renderer: &'a mut OffscreenRenderer,
    text: &'a TextEngine,
    /// The first failure of the renderer. The exporter only learns that rasterising failed, as a message, so the
    /// failure itself is kept here: a readback failure means the device may have been lost, and only a
    /// [`RenderError`] reaching [`with_shared_renderer`] makes it replace the device rather than hand the same dead
    /// one to every later export.
    failure: Option<RenderError>,
}

impl<'a> GpuRasteriser<'a> {
    /// Wraps a renderer, which resolves any text in the rasterised content through `text`.
    pub fn new(renderer: &'a mut OffscreenRenderer, text: &'a TextEngine) -> Self {
        Self {
            renderer,
            text,
            failure: None,
        }
    }
}

impl Rasteriser for GpuRasteriser<'_> {
    fn rasterise(&mut self, list: &DisplayList, dpi: f64) -> Result<RasterImage, String> {
        let rendered = match self.renderer.render_display_list(list, self.text, dpi) {
            Ok(rendered) => rendered,
            Err(error) => {
                let message = error.to_string();
                self.failure.get_or_insert(error);
                return Err(message);
            }
        };
        Ok(RasterImage {
            width: rendered.width,
            height: rendered.height,
            rgba: rendered.rgba,
        })
    }
}

/// Compiles and exports a figure, rasterising its dense content on the GPU.
///
/// # Errors
///
/// Returns [`ExportError::Render`] when the figure has content the policy rasterises and the renderer cannot be
/// created or cannot draw it, and [`ExportError::Pdf`] when the PDF itself cannot be written.
pub fn export_pdf(
    figure: &Figure,
    text: &TextEngine,
    options: &PdfOptions,
) -> Result<Vec<u8>, ExportError> {
    let scene = ironlab_scene::compile(figure, text);
    render_display_list(&scene.display_list, text, options)
}

/// Exports an already compiled display list, rasterising its dense content on the GPU.
///
/// # Errors
///
/// As for [`export_pdf`].
pub fn render_display_list(
    list: &DisplayList,
    text: &TextEngine,
    options: &PdfOptions,
) -> Result<Vec<u8>, ExportError> {
    if !ironlab_pdf::raster::rasterises_any(list, &options.raster) {
        return Ok(ironlab_pdf::render_display_list(list, text, options, None)?);
    }
    with_shared_renderer(|renderer| {
        let mut raster = GpuRasteriser::new(renderer, text);
        let result = ironlab_pdf::render_display_list(list, text, options, Some(&mut raster));
        // A failure of the renderer is returned as itself, so that a lost device is recognised and replaced. Every
        // other failure is the exporter's and travels through the inner result, where it cannot be mistaken for one.
        match (result, raster.failure) {
            (Err(PdfError::Raster(_)), Some(failure)) => Err(failure),
            (result, _) => Ok(result),
        }
    })?
    .map_err(ExportError::Pdf)
}

/// Compiles and exports a figure, writing the PDF to `path`.
///
/// # Errors
///
/// Returns the errors of [`export_pdf`], and [`PdfError::Io`] when the file cannot be written.
pub fn write_pdf(
    figure: &Figure,
    text: &TextEngine,
    options: &PdfOptions,
    path: impl AsRef<Path>,
) -> Result<(), ExportError> {
    let bytes = export_pdf(figure, text, options)?;
    std::fs::write(path.as_ref(), bytes).map_err(|error| ExportError::Pdf(PdfError::Io(error)))
}
