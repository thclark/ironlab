//! Opening figure files: the format is chosen by the extension, and every failure names the file.

mod common;

use std::path::PathBuf;
use std::process::Command;

use common::*;
use ironlab_ir::{Dimension, NdArray};
use ironlab_viewer::files::{OpenError, figure_stem, read_figure};

/// Returns a path in Cargo's per-crate temporary directory, removing any file left there by an earlier run.
fn fresh(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ironlab-viewer-file-tests");
    std::fs::create_dir_all(&dir).expect("create temporary directory");
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

/// A figure with two linked axes and a data array, so that a lossy reader would be noticed.
fn sample() -> ironlab_ir::Figure {
    let mut figure = figure_with(
        vec![axes_2d(2), axes_3d(3)],
        vec![link(Dimension::X, &[2, 3])],
    );
    figure.add_data(NdArray::vector(vec![0.0, f64::NAN, 2.5]));
    figure
}

// Why: `.fig` is the default format that the facade saves and that other tools write from the generated `.proto`
// files, so the viewer must open it.
#[test]
fn a_fig_file_is_read_as_protobuf() {
    let path = fresh("sample.fig");
    std::fs::write(&path, sample().to_protobuf()).unwrap();
    assert_eq!(read_figure(&path).unwrap(), sample());
}

// Why: JSON remains supported for debugging and for files written by other tools, under both `.json` and `.fig.json`.
#[test]
fn json_files_are_read_as_json() {
    for name in ["sample.fig.json", "sample.json"] {
        let path = fresh(name);
        std::fs::write(&path, sample().to_json()).unwrap();
        assert_eq!(read_figure(&path).unwrap(), sample(), "{name}");
    }
}

// Why: extensions are case-insensitive on the file systems of macOS and Windows, so `FIGURE.FIG` must open as `.fig`.
#[test]
fn extensions_are_matched_without_regard_to_case() {
    let path = fresh("UPPER.FIG");
    std::fs::write(&path, sample().to_protobuf()).unwrap();
    assert_eq!(read_figure(&path).unwrap(), sample());
}

// Why: a file whose extension names no figure format must be refused with a message that names the file and the
// supported extensions, before the file is read, so that the user learns what to rename rather than seeing a decoding
// error from a guessed format.
#[test]
fn an_unsupported_extension_is_refused_before_reading() {
    let path = fresh("notes.txt");
    let error = read_figure(&path).unwrap_err();
    assert!(
        matches!(&error, OpenError::UnsupportedFormat { path: p } if p == &path),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("notes.txt"), "{message}");
    assert!(
        message.contains(".fig") && message.contains(".json"),
        "{message}"
    );
}

// Why: when several files are opened at once, the user must be told which one is missing.
#[test]
fn a_missing_file_is_an_io_error_naming_the_file() {
    let path = fresh("absent.fig");
    let error = read_figure(&path).unwrap_err();
    assert!(
        matches!(&error, OpenError::Io { path: p, .. } if p == &path),
        "{error:?}"
    );
    assert!(error.to_string().contains("absent.fig"), "{error}");
}

// Why: a file with the right extension but the wrong content (such as JSON renamed to `.fig`) must be reported as an
// invalid figure naming the file, not opened with defaults and not a panic.
#[test]
fn content_that_is_not_a_figure_is_an_error_naming_the_file() {
    let fig_path = fresh("renamed_json.fig");
    std::fs::write(&fig_path, sample().to_json()).unwrap();
    let json_path = fresh("broken.fig.json");
    std::fs::write(&json_path, "{ not json").unwrap();

    for path in [fig_path, json_path] {
        let error = read_figure(&path).unwrap_err();
        assert!(
            matches!(&error, OpenError::Invalid { path: p, .. } if p == &path),
            "{error:?}"
        );
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(error.to_string().contains(&name), "{error}");
    }
}

// Why: the viewer suggests `<stem>.pdf` when exporting a tab titled with its file name, so every figure extension must
// be removed from the title (and only a figure extension), or the suggestion would be `pressure.fig.pdf`.
#[test]
fn figure_stem_removes_only_figure_extensions() {
    assert_eq!(figure_stem("pressure.fig"), "pressure");
    assert_eq!(figure_stem("pressure.fig.json"), "pressure");
    assert_eq!(figure_stem("pressure.json"), "pressure");
    assert_eq!(figure_stem("PRESSURE.FIG"), "PRESSURE");
    assert_eq!(figure_stem("notes.txt"), "notes.txt");
    assert_eq!(figure_stem("Two panels"), "Two panels");
    assert_eq!(figure_stem(".fig"), ".fig");
}

// Why: the binary is how users open saved figures from the command line; without arguments it must explain which files
// it accepts, and a bad argument must fail with a message naming the file, both without opening a window.
#[test]
fn the_binary_explains_its_usage_and_refuses_bad_files_without_a_window() {
    let binary = env!("CARGO_BIN_EXE_ironlab-viewer");

    let usage = Command::new(binary).output().expect("the binary runs");
    assert!(!usage.status.success());
    let stderr = String::from_utf8_lossy(&usage.stderr);
    assert!(
        stderr.contains("FIGURE.fig") && stderr.contains(".json"),
        "{stderr}"
    );

    let path = fresh("figure.png");
    let refused = Command::new(binary)
        .arg(&path)
        .output()
        .expect("the binary runs");
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("figure.png") && stderr.contains(".fig"),
        "{stderr}"
    );
}
