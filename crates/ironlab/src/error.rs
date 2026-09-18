//! Errors returned by the facade.

use std::path::PathBuf;

use ironlab_ir::{IrError, ValidationReport};

/// An error returned when a figure cannot be saved, loaded, linked, exported or shown.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file could not be read or written.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The extension of a path names no figure format, so the figure cannot be saved to
    /// it or loaded from it. The supported extensions are `.fig` (Protocol Buffers) and
    /// `.json` (JSON, including `.fig.json`).
    #[error(
        "cannot tell the figure format of {}: use the extension .fig for Protocol Buffers or .json (such as .fig.json) for JSON",
        .0.display()
    )]
    UnsupportedFormat(PathBuf),

    /// An operation on the figure IR failed, for example because a file is not a
    /// valid figure or an identifier does not refer to an axes.
    #[error(transparent)]
    Ir(#[from] IrError),

    /// The PDF exporter could not produce the document, or the figure has content that
    /// must be rasterised and no graphics adapter is available to render it.
    #[error(transparent)]
    Export(#[from] ironlab_viewer::ExportError),

    /// The interactive viewer could not be started or failed while running.
    #[error("viewer failed: {0}")]
    Viewer(#[from] eframe::Error),

    /// The figure has validation errors, so it cannot be exported or shown.
    #[error("the figure is invalid: {}", summarise(.0))]
    Invalid(ValidationReport),
}

/// Joins the messages of the errors in a validation report into one line.
fn summarise(report: &ValidationReport) -> String {
    report
        .errors
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}
