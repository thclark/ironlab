//! Errors of the gallery and its docs generator.

use std::fmt;
use std::path::PathBuf;

/// An error raised while rendering, exporting or documenting gallery figures.
#[derive(Debug)]
pub enum GalleryError {
    /// A file or directory could not be read or written.
    Io {
        /// The path that could not be read or written.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// A gallery figure has validation errors.
    Invalid {
        /// The slug of the entry whose figure is invalid.
        slug: String,
        /// The messages of the validation errors.
        messages: Vec<String>,
    },
    /// A figure could not be rendered to an image.
    Render(String),
    /// A figure could not be exported to PDF.
    Pdf(String),
    /// A slug given on the command line does not name a gallery entry.
    UnknownSlug(String),
    /// The directory given for the documentation gallery contains a subdirectory, so it is not a directory that the
    /// generator owns and may clear.
    NotGalleryDirectory {
        /// The directory given for the gallery.
        dir: PathBuf,
        /// The subdirectory found in it.
        subdirectory: PathBuf,
    },
}

impl GalleryError {
    /// Creates an [`GalleryError::Io`] error for a path.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for GalleryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot access {}: {source}", path.display()),
            Self::Invalid { slug, messages } => {
                write!(
                    f,
                    "gallery figure {slug} is invalid: {}",
                    messages.join("; ")
                )
            }
            Self::Render(message) => write!(f, "rendering failed: {message}"),
            Self::Pdf(message) => write!(f, "PDF export failed: {message}"),
            Self::UnknownSlug(slug) => write!(f, "there is no gallery entry named {slug}"),
            Self::NotGalleryDirectory { dir, subdirectory } => write!(
                f,
                "{} is not a gallery directory, because it contains the subdirectory {}; the generator clears its \
                 output directory, so give it a directory of its own, such as docs/gallery",
                dir.display(),
                subdirectory.display()
            ),
        }
    }
}

impl std::error::Error for GalleryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
