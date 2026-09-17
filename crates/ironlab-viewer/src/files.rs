//! Reading figure files, in the format named by their extension.
//!
//! A `.fig` file holds the default Protocol Buffers encoding of a figure, and a `.json` file (including `.fig.json`)
//! holds the JSON encoding. Extensions are matched without regard to case.

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

/// Reads a figure from a `.fig` (Protocol Buffers) or `.json` (JSON) file.
///
/// The format is decided from the extension before the file is read. The figure is not validated.
///
/// # Errors
///
/// Returns [`OpenError::UnsupportedFormat`] when the extension is neither `.fig` nor `.json`, [`OpenError::Io`] when
/// the file cannot be read, and [`OpenError::Invalid`] when its content is not a figure in that format.
pub fn read_figure(path: &Path) -> Result<Figure, OpenError> {
    let extension = path.extension().and_then(|extension| extension.to_str());
    let io = |source| OpenError::Io {
        path: path.to_path_buf(),
        source,
    };
    let figure = match extension {
        Some(ext) if ext.eq_ignore_ascii_case("fig") => {
            Figure::from_protobuf(&std::fs::read(path).map_err(io)?)
        }
        Some(ext) if ext.eq_ignore_ascii_case("json") => {
            Figure::from_json(&std::fs::read_to_string(path).map_err(io)?)
        }
        _ => {
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
