//! Export of gallery figures as files.

use std::fs;
use std::path::{Path, PathBuf};

use crate::GalleryEntry;
use crate::docs::Renderer;
use crate::error::GalleryError;

/// Writes `<slug>.pdf`, `<slug>.fig` (Protocol Buffers), `<slug>.fig.json` (JSON) and `<slug>.png` (at `dpi`) for each
/// entry into `out_dir`, creating the directory if necessary, and returns the paths written.
///
/// # Errors
///
/// Returns [`GalleryError::Invalid`] when a figure has validation errors, the renderer's error when a figure cannot
/// be rendered or exported, and [`GalleryError::Io`] when a file cannot be written.
pub fn export_entries(
    out_dir: &Path,
    entries: &[GalleryEntry],
    renderer: &dyn Renderer,
    dpi: f64,
) -> Result<Vec<PathBuf>, GalleryError> {
    fs::create_dir_all(out_dir).map_err(|error| GalleryError::io(out_dir, error))?;
    let mut written = Vec::new();
    for entry in entries {
        let (figure, _warnings) = entry.build_validated()?;
        let ir = figure.ir();
        let files = [
            (format!("{}.pdf", entry.slug), renderer.pdf(ir)?.bytes),
            (format!("{}.fig", entry.slug), ir.to_protobuf()),
            (
                format!("{}.fig.json", entry.slug),
                ir.to_json().into_bytes(),
            ),
            (format!("{}.png", entry.slug), renderer.png(ir, dpi)?),
        ];
        for (name, bytes) in files {
            let path = out_dir.join(name);
            fs::write(&path, bytes).map_err(|error| GalleryError::io(&path, error))?;
            written.push(path);
        }
    }
    Ok(written)
}
