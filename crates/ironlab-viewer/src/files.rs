//! Reading and writing figure files, in the format named by their extension.
//!
//! A `.fig` file holds the default Protocol Buffers encoding of a figure, and a `.json` file (including `.fig.json`)
//! holds the JSON encoding. Extensions are matched without regard to case, and both formats describe a figure
//! completely, so a figure written by [`write_figure`] is read back unchanged by [`read_figure`].

use std::path::{Path, PathBuf};

use ironlab_ir::{Figure, IrError};

/// The extensions of figure files, longest first, so that `.fig.json` is removed whole by [`figure_stem`].
const FIGURE_EXTENSIONS: [&str; 3] = [".fig.json", ".json", ".fig"];

/// An error raised when a figure file cannot be opened. Every variant names the file.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The extension of the file names no figure format.
    #[error(
        "{}: unsupported file type; expected a .fig file (Protocol Buffers) or a .json file such as .fig.json (JSON)",
        .path.display()
    )]
    UnsupportedFormat {
        /// The file.
        path: PathBuf,
    },

    /// The file could not be read.
    #[error("{}: {source}", .path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },

    /// The content of the file is not a figure of a compatible schema version in the format named by its extension.
    #[error("{}: {source}", .path.display())]
    Invalid {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: IrError,
    },
}

/// The format in which a figure file is written, chosen by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    /// Protocol Buffers, the default format, written to a `.fig` file.
    Protobuf,
    /// JSON, written to a `.json` file such as `.fig.json`.
    Json,
}

impl Format {
    /// Returns the format named by the extension of `path`, or `None` when it names no figure format.
    fn of(path: &Path) -> Option<Self> {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("fig") => Some(Self::Protobuf),
            Some(ext) if ext.eq_ignore_ascii_case("json") => Some(Self::Json),
            _ => None,
        }
    }
}

/// Writes a figure to a `.fig` (Protocol Buffers) or `.json` (JSON) file.
///
/// The format is decided from the extension before anything is written, so a path that names no figure format leaves
/// no file behind. The figure is not validated: a figure with problems can be saved and repaired later, as the
/// facade's `Figure::save` does.
///
/// # Errors
///
/// Returns [`SaveError::UnsupportedFormat`] when the extension is neither `.fig` nor `.json`, and [`SaveError::Io`]
/// when the file cannot be written.
pub fn write_figure(path: &Path, figure: &Figure) -> Result<(), SaveError> {
    let bytes = match Format::of(path) {
        Some(Format::Protobuf) => figure.to_protobuf(),
        Some(Format::Json) => figure.to_json().into_bytes(),
        None => {
            return Err(SaveError::UnsupportedFormat {
                path: path.to_path_buf(),
            });
        }
    };
    std::fs::write(path, bytes).map_err(|source| SaveError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// An error raised when a figure file cannot be written. Every variant names the file.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// The extension of the file names no figure format.
    #[error(
        "{}: unsupported file type; expected a .fig file (Protocol Buffers) or a .json file such as .fig.json (JSON)",
        .path.display()
    )]
    UnsupportedFormat {
        /// The file.
        path: PathBuf,
    },

    /// The file could not be written.
    #[error("{}: {source}", .path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
}

/// Reads a figure from a `.fig` (Protocol Buffers) or `.json` (JSON) file.
///
/// The format is decided from the extension before the file is read. The figure is not validated.
///
/// # Errors
///
/// Returns [`OpenError::UnsupportedFormat`] when the extension is neither `.fig` nor `.json`, [`OpenError::Io`] when
/// the file cannot be read, and [`OpenError::Invalid`] when its content is not a figure in that format.
pub fn read_figure(path: &Path) -> Result<Figure, OpenError> {
    let io = |source| OpenError::Io {
        path: path.to_path_buf(),
        source,
    };
    let figure = match Format::of(path) {
        Some(Format::Protobuf) => Figure::from_protobuf(&std::fs::read(path).map_err(io)?),
        Some(Format::Json) => Figure::from_json(&std::fs::read_to_string(path).map_err(io)?),
        None => {
            return Err(OpenError::UnsupportedFormat {
                path: path.to_path_buf(),
            });
        }
    };
    figure.map_err(|source| OpenError::Invalid {
        path: path.to_path_buf(),
        source,
    })
}

/// Returns a file name without its figure extension (`.fig`, `.fig.json` or `.json`, in any case), or the name
/// unchanged when it has none.
#[must_use]
pub fn figure_stem(name: &str) -> &str {
    FIGURE_EXTENSIONS
        .iter()
        .find_map(|extension| {
            let split = name.len().checked_sub(extension.len())?;
            let (stem, suffix) = (name.get(..split)?, name.get(split..)?);
            (!stem.is_empty() && suffix.eq_ignore_ascii_case(extension)).then_some(stem)
        })
        .unwrap_or(name)
}
