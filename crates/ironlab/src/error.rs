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

    /// A plane of components does not have the shape of the pixels it would form or
    /// join, so [`Pixels::from_planes`](crate::Pixels::from_planes) or
    /// [`Pixels::with_alpha`](crate::Pixels::with_alpha) cannot build the pixels.
    #[error(
        "a plane of {} rows and {} columns does not match pixels of {} rows and {} columns",
        .found[0], .found[1], .expected[0], .expected[1]
    )]
    PlaneShapeMismatch {
        /// The rows and columns that every plane must have: those of the red plane given
        /// to `from_planes`, or of the pixels that `with_alpha` extends.
        expected: [usize; 2],
        /// The rows and columns of the first plane that differs.
        found: [usize; 2],
    },

    /// A component of a pixel is NaN or infinite, so it has no byte value;
    /// [`Pixels::from_planes`](crate::Pixels::from_planes) and
    /// [`Pixels::with_alpha`](crate::Pixels::with_alpha) refuse such a component rather
    /// than hide a failed computation.
    #[error(
        "the pixel in row {row} and column {col} has a non-finite {}",
        describe_channel(*.channel)
    )]
    NonFiniteComponent {
        /// The row of the pixel.
        row: usize,
        /// The column of the pixel.
        col: usize,
        /// The channel of the component: 0 for red, 1 for green, 2 for blue and 3 for
        /// alpha.
        channel: usize,
    },
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

/// Names the channel of a pixel component in an error message.
fn describe_channel(channel: usize) -> String {
    match channel {
        0 => "red component".to_owned(),
        1 => "green component".to_owned(),
        2 => "blue component".to_owned(),
        3 => "alpha".to_owned(),
        other => format!("component in channel {other}"),
    }
}
