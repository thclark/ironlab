//! PDF export through the viewer's own renderer.
//!
//! [`ironlab_pdf`] writes vector geometry and text, and needs an image for the parts of a figure that are too dense
//! to be worth writing as vector paths, and two renders of each three-dimensional axes to verify its painter's
//! order. This module supplies those images in the exporter's two-phase form
//! ([`ironlab_pdf::twophase`]): [`export_display_list`] asks the exporter which renders the page needs, performs
//! each through the viewer's headless renderer, and hands the images back for the exporter to embed. The pixels in
//! a PDF therefore come from the same device, the same tessellation and the same shaders that draw the interactive
//! canvas; there is no second rasteriser, and nothing that can drift from what the user inspected on screen. The
//! renders are awaited rather than blocked on, so the same export runs on the desktop and in a browser, and both
//! write the bytes a single pass with the renderer inside the exporter's walk would have written.
//!
//! [`export_pdf`] and [`write_pdf`] are the export path the whole project uses: the viewer's "Export PDF…" command,
//! the `ironlab` crate's [`Figure::export_pdf`](../../ironlab/struct.Figure.html) and the documentation gallery.
//! They block on the process-wide renderer of [`with_shared_renderer`].
//!
//! A figure with nothing to rasterise and no three-dimensional axes is exported without ever touching the GPU, so
//! exporting on a machine with no graphics adapter works as it always has. A three-dimensional axes under the
//! default policy needs an adapter only to verify that its painter's order shows what the viewer shows; without one
//! it is still exported, back to front, with an [`ironlab_pdf::ExportWarning`] that says so and names a software
//! adapter as the remedy. Only a figure that must be rasterised needs an adapter, and when none is available that
//! is reported as [`ExportError::Render`] rather than quietly exported as something else.

use std::path::Path;

use ironlab_ir::Figure;
use ironlab_pdf::raster::{Need, needs_rasteriser};
use ironlab_pdf::{
    ExportWarning, ExportWarningKind, Exported, PdfError, PdfOptions, RasterImage, Rendered,
    UnverifiedCause,
};
use ironlab_scene::SceneWarning;
use ironlab_scene::display::DisplayList;
use ironlab_text::TextEngine;

#[cfg(not(target_arch = "wasm32"))]
use crate::offscreen::with_shared_renderer;
use crate::offscreen::{OffscreenRenderer, RenderError};

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

/// Compiles and exports a figure, rasterising its dense content on the GPU.
///
/// The warnings the scene compiler raised while drawing the figure are returned with the bytes, as
/// [`ironlab_pdf::export_pdf`] returns them, so that a caller learns which artists were left off the page.
///
/// # Errors
///
/// Returns [`ExportError::Render`] when the figure has content the policy rasterises and the renderer cannot be
/// created or cannot draw it, and [`ExportError::Pdf`] when the PDF itself cannot be written.
#[cfg(not(target_arch = "wasm32"))]
pub fn export_pdf(
    figure: &Figure,
    text: &TextEngine,
    options: &PdfOptions,
) -> Result<Exported, ExportError> {
    let scene = ironlab_scene::compile(figure, text);
    let rendered = render_display_list(&scene.display_list, text, options)?;
    Ok(Exported {
        bytes: rendered.bytes,
        warnings: scene.warnings,
        export: rendered.warnings,
    })
}

/// Exports a compiled list, rendering what the options rasterise or verify through `renderer`, or without one
/// when creating it failed.
///
/// A list that needs no renderer is exported without asking for one, whatever `renderer` holds. Otherwise the
/// exporter's plan of renders is performed through `renderer`, one awaited render at a time, and the images are
/// replayed into the export. A list that needs the renderer only to verify its three-dimensional axes is exported
/// without it when `renderer` reports that no adapter is available, with each such axes drawn back to front and a
/// warning naming the missing adapter; a list that must be rasterised is not.
///
/// # Errors
///
/// Returns [`ExportError::Render`] with the failure to create the renderer, when the list needs one, or with a
/// failure of the renderer to draw a request, and [`ExportError::Pdf`] when the PDF itself cannot be written.
pub async fn export_display_list(
    list: &DisplayList,
    text: &TextEngine,
    options: &PdfOptions,
    renderer: Result<&mut OffscreenRenderer, RenderError>,
) -> Result<Rendered, ExportError> {
    let need = needs_rasteriser(list, &options.raster);
    let renderer = match (renderer, need) {
        (_, Need::No) => {
            return Ok(ironlab_pdf::render_display_list(list, text, options, None)?);
        }
        (Ok(renderer), _) => renderer,
        (Err(RenderError::NoAdapter(message)), Need::ToVerify) => {
            return unverified(list, text, options, &message);
        }
        (Err(error), _) => return Err(ExportError::Render(error)),
    };
    let requests = ironlab_pdf::raster_requests(list, text, options)?;
    let mut rasters = Vec::with_capacity(requests.len());
    for request in &requests {
        let image = renderer
            .render_display_list_async(&request.list, text, request.dpi)
            .await
            .map_err(ExportError::Render)?;
        rasters.push(RasterImage {
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        });
    }
    Ok(ironlab_pdf::render_with_rasters(
        list, text, options, &requests, rasters,
    )?)
}

/// Exports a list whose three-dimensional axes could not be verified because no graphics adapter is available
/// (`message` says why), drawing each back to front with a warning that names the missing adapter and its remedy.
fn unverified(
    list: &DisplayList,
    text: &TextEngine,
    options: &PdfOptions,
    message: &str,
) -> Result<Rendered, ExportError> {
    let mut rendered = ironlab_pdf::render_display_list(list, text, options, None)?;
    for warning in &mut rendered.warnings {
        if let ExportWarningKind::Unverified {
            cause: UnverifiedCause::NoRasteriser,
        } = warning.kind
        {
            *warning = ExportWarning::unverified(
                warning.node,
                UnverifiedCause::NoAdapter,
                &format!(
                    "no graphics adapter is available to verify it ({message}); a software adapter such as \
                     lavapipe from Mesa serves on a machine without a graphics device"
                ),
            );
        }
    }
    Ok(rendered)
}

/// Exports an already compiled display list, rasterising and verifying through the process-wide renderer what the
/// options ask, and blocking on it.
///
/// This is [`export_display_list`] over [`with_shared_renderer`]: a list that needs no renderer never creates the
/// device, and a list that needs one only to verify its three-dimensional axes is exported without it when no
/// adapter is available.
///
/// # Errors
///
/// As for [`export_display_list`].
#[cfg(not(target_arch = "wasm32"))]
pub fn render_display_list(
    list: &DisplayList,
    text: &TextEngine,
    options: &PdfOptions,
) -> Result<Rendered, ExportError> {
    if needs_rasteriser(list, &options.raster) == Need::No {
        return Ok(ironlab_pdf::render_display_list(list, text, options, None)?);
    }
    let attempt = with_shared_renderer(|renderer| {
        // A failure of the renderer is returned as itself, so that a lost device is recognised and replaced. Every
        // other failure is the exporter's and travels through the inner result, where it cannot be mistaken for one.
        match pollster::block_on(export_display_list(list, text, options, Ok(renderer))) {
            Err(ExportError::Render(error)) => Err(error),
            result => Ok(result),
        }
    });
    match attempt {
        Ok(result) => result,
        Err(error) => pollster::block_on(export_display_list(list, text, options, Err(error))),
    }
}

/// Compiles and exports a figure, writing the PDF to `path`, and returns the warnings the scene compiler raised
/// while drawing the figure, as [`export_pdf`] does.
///
/// # Errors
///
/// Returns the errors of [`export_pdf`], and [`PdfError::Io`] when the file cannot be written.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_pdf(
    figure: &Figure,
    text: &TextEngine,
    options: &PdfOptions,
    path: impl AsRef<Path>,
) -> Result<Vec<SceneWarning>, ExportError> {
    let exported = export_pdf(figure, text, options)?;
    std::fs::write(path.as_ref(), exported.bytes)
        .map_err(|error| ExportError::Pdf(PdfError::Io(error)))?;
    Ok(exported.warnings)
}
