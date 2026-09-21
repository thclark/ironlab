//! Saving, loading and exporting figures.

mod common;

use std::path::PathBuf;

use common::image_figure;
use ironlab::ir::IssueKind;
use ironlab::prelude::*;

/// Returns a path in Cargo's per-crate temporary directory, unique to the test.
fn temp_path(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ironlab-facade-tests");
    std::fs::create_dir_all(&dir).expect("create temporary directory");
    dir.join(name)
}

/// A small figure that exercises a line, a surface, a link and a parameter of every kind.
fn sample_figure() -> Figure {
    let x = linspace(0.0, 1.0, 5);
    let z = Matrix::from_fn(x.len(), x.len(), |row, col| (row * col) as f64);
    let mut fig = Figure::new()
        .tiles(1, 2)
        .title("Sample")
        .parameter("converged", true)
        .parameter("cells", -25)
        .parameter("reynolds_number", 1e5)
        .parameter("solver", "k–ω SST");
    fig.axes(0, 0)
        .plot(&x, &x)
        .display_name("$y = x$")
        .marker(Marker::Circle);
    fig.axes(0, 0).legend(LegendLocation::NorthWest);
    fig.axes(0, 1).surf(&x, &x, &z);
    fig.link_all_x().unwrap();
    fig
}

/// Removes a file left by an earlier run, so that a test observes only what it writes.
fn fresh(name: &str) -> PathBuf {
    let path = temp_path(name);
    let _ = std::fs::remove_file(&path);
    path
}

// WHY: `.fig` is the default file and transport format between user code, the viewer
// binary and future bindings; saving and loading it must reproduce the figure exactly.
#[test]
fn save_then_load_round_trips_a_fig_file() {
    let fig = sample_figure();
    let path = fresh("round_trip.fig");
    fig.save(&path).unwrap();
    let loaded = Figure::load(&path).unwrap();
    assert_eq!(loaded, fig);
}

// WHY: JSON is the supported secondary format for debugging, other tools and web pages,
// so it must reproduce the figure exactly too, under both of its accepted extensions.
#[test]
fn save_then_load_round_trips_json_files() {
    let fig = sample_figure();
    for name in ["round_trip.fig.json", "round_trip.json"] {
        let path = fresh(name);
        fig.save(&path).unwrap();
        let loaded = Figure::load(&path).unwrap();
        assert_eq!(loaded, fig, "{name}");
    }
}

// WHY: images add the 8-bit element type, three-dimensional arrays, a NaN in mapped
// values, pixel ranges, wall planes with offsets and out-of-range policies to what a
// file must carry; a figure built through the facade with all of them must come back
// equal from both formats, or an image figure saved today could not be reopened or
// viewed. The fixture is a valid figure, so what is round-tripped is a figure a user
// would actually save.
#[test]
fn save_then_load_round_trips_a_figure_with_every_image_kind_in_both_formats() {
    let fig = image_figure();
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    for name in ["images.fig", "images.fig.json"] {
        let path = fresh(name);
        fig.save(&path).unwrap();
        let loaded = Figure::load(&path).unwrap();
        assert_eq!(loaded, fig, "{name}");
    }
}

// WHY: the extension is the only statement of the format that a user makes, so `.fig`
// must write the Protocol Buffers encoding (which the viewer and other languages decode
// from the generated `.proto` files), not JSON with a misleading name.
#[test]
fn save_writes_protobuf_to_a_fig_file() {
    let fig = sample_figure();
    let path = fresh("dispatch.fig");
    fig.save(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), fig.ir().to_protobuf());
}

// WHY: see save_writes_protobuf_to_a_fig_file; `.json` and `.fig.json` must write the
// JSON encoding, which other tools read as text.
#[test]
fn save_writes_json_to_a_json_file() {
    let fig = sample_figure();
    for name in ["dispatch.fig.json", "dispatch.json"] {
        let path = fresh(name);
        fig.save(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            fig.ir().to_json(),
            "{name}"
        );
    }
}

// WHY: load must decode by extension too; a `.fig` file holding the bytes that
// `to_protobuf` produces (for example one received over a socket and written to disk)
// must load, and a `.json` file must be read as JSON.
#[test]
fn load_reads_each_format_by_extension() {
    let fig = sample_figure();
    let fig_path = fresh("load_dispatch.fig");
    std::fs::write(&fig_path, fig.to_protobuf()).unwrap();
    assert_eq!(Figure::load(&fig_path).unwrap(), fig);

    let json_path = fresh("load_dispatch.fig.json");
    std::fs::write(&json_path, fig.ir().to_json()).unwrap();
    assert_eq!(Figure::load(&json_path).unwrap(), fig);
}

// WHY: file systems on macOS and Windows treat extensions case-insensitively, and users
// type `FIGURE.FIG` as readily as `figure.fig`; the format must not depend on case.
#[test]
fn extensions_are_matched_without_regard_to_case() {
    let fig = sample_figure();
    let path = fresh("upper_case.FIG");
    fig.save(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), fig.ir().to_protobuf());
    assert_eq!(Figure::load(&path).unwrap(), fig);
}

// WHY: an extension that names neither format (or no extension at all) must be refused
// with an error that says so, rather than silently choosing a format that a later load
// or another tool would misread; and nothing may be written.
#[test]
fn save_refuses_an_unsupported_extension_without_writing() {
    for name in ["figure.txt", "figure", "figure.fig.bak"] {
        let path = fresh(name);
        let error = sample_figure().save(&path).unwrap_err();
        assert!(
            matches!(&error, Error::UnsupportedFormat(p) if p == &path),
            "{name}: {error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains(".fig") && message.contains(".json"),
            "the message must name the supported extensions: {message}"
        );
        assert!(!path.exists(), "{name} was written");
    }
}

// WHY: see save_refuses_an_unsupported_extension_without_writing; the format is decided
// before the file is opened, so an unsupported extension is reported as such even when
// the file does not exist, instead of as a misleading I/O error.
#[test]
fn load_refuses_an_unsupported_extension_before_reading() {
    let path = fresh("missing.txt");
    assert!(matches!(
        Figure::load(&path),
        Err(Error::UnsupportedFormat(p)) if p == path
    ));
}

// WHY: callers who choose their own file names (such as a temporary file or a web
// upload) still need to write and read JSON explicitly, whatever the extension.
#[test]
fn save_json_and_load_json_ignore_the_extension() {
    let fig = sample_figure();
    let path = fresh("explicit.txt");
    fig.save_json(&path).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), fig.ir().to_json());
    assert_eq!(Figure::load_json(&path).unwrap(), fig);
}

// WHY: protobuf is also the transport format (sockets, other processes), where figures
// are bytes rather than files; the facade must convert to and from bytes without
// dropping to the IR crate.
#[test]
fn protobuf_bytes_round_trip() {
    let fig = sample_figure();
    let bytes = fig.to_protobuf();
    assert_eq!(bytes, fig.ir().to_protobuf());
    assert_eq!(Figure::from_protobuf(&bytes).unwrap(), fig);
}

// WHY: a loaded figure must be extensible with the builder without identifier
// collisions, since the id allocator is not serialised.
#[test]
fn a_loaded_figure_can_be_extended() {
    let path = fresh("extend.fig");
    sample_figure().save(&path).unwrap();
    let mut loaded = Figure::load(&path).unwrap();
    loaded.axes(0, 0).plot([0.0, 1.0], [1.0, 0.0]);
    let report = loaded.validate();
    assert!(
        !report
            .errors
            .iter()
            .any(|issue| issue.kind == IssueKind::DuplicateNodeId),
        "{report:?}"
    );
}

// WHY: loading a file that is not a figure must be an error the caller can handle, not
// a panic, and must be distinguishable from a missing file, in either format.
#[test]
fn load_rejects_a_file_that_is_not_a_figure() {
    // A JSON file in a `.fig` file is not a valid protobuf figure, and vice versa: the
    // extension, not the content, decides how a file is read.
    let fig_path = fresh("garbage.fig");
    std::fs::write(&fig_path, sample_figure().ir().to_json()).unwrap();
    assert!(matches!(Figure::load(&fig_path), Err(Error::Ir(_))));

    let json_path = fresh("garbage.fig.json");
    std::fs::write(&json_path, b"this is not json {").unwrap();
    assert!(matches!(Figure::load(&json_path), Err(Error::Ir(_))));
}

// WHY: see load_rejects_a_file_that_is_not_a_figure; a missing file is an I/O error.
#[test]
fn load_reports_a_missing_file_as_io() {
    for name in ["does_not_exist.fig", "does_not_exist.fig.json"] {
        let path = fresh(name);
        assert!(matches!(Figure::load(&path), Err(Error::Io(_))), "{name}");
    }
}

// WHY: export_pdf is the publication path; the file must be a PDF (checked by its
// header here; page size and text are checked by the PDF crate's tests).
#[test]
fn export_pdf_writes_a_pdf_file() {
    let path = temp_path("sample.pdf");
    let _ = std::fs::remove_file(&path);
    sample_figure().export_pdf(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"%PDF"), "file does not start with %PDF");
}

// WHY: exporting an invalid figure must fail with the validation report (so the user
// sees what is wrong) and must not leave a partial file behind.
#[test]
fn export_pdf_refuses_an_invalid_figure() {
    let path = temp_path("invalid.pdf");
    let _ = std::fs::remove_file(&path);
    let mut fig = Figure::new();
    fig.axes(0, 0).plot([0.0, 1.0, 2.0], [0.0]);
    let error = fig.export_pdf(&path).unwrap_err();
    match &error {
        Error::Invalid(report) => {
            assert!(!report.is_valid());
            // The message a user sees from `?` or `{error}` names each problem.
            let message = error.to_string();
            for issue in &report.errors {
                assert!(message.contains(&issue.message), "{message}");
            }
        }
        other => panic!("expected Error::Invalid, found {other:?}"),
    }
    assert!(!path.exists());
}

// WHY: a destination that cannot be written is an I/O problem of the caller's
// environment, not a failure of the exporter, and is documented as Error::Io so that
// callers can handle it like any other file error.
#[test]
fn export_pdf_to_a_missing_directory_is_an_io_error() {
    let path = temp_path("no_such_directory").join("figure.pdf");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
    let result = sample_figure().export_pdf(&path);
    assert!(matches!(result, Err(Error::Io(_))), "{result:?}");
}

// WHY: show() must check the figure before opening a window, so that an invalid figure
// is reported to the caller instead of opening a viewer that cannot draw it. The
// check happens before the event loop starts, so this test needs no display.
#[test]
fn show_refuses_an_invalid_figure_without_opening_a_window() {
    let mut fig = Figure::new();
    fig.axes(0, 0).plot([0.0, 1.0, 2.0], [0.0]);
    assert!(matches!(fig.show(), Err(Error::Invalid(_))));
}

// WHY: users propagate facade errors with `?` into `Box<dyn Error + Send + Sync>` and
// error-handling crates, and across threads; that requires the error to be `Send`,
// `Sync` and `'static`, which a non-thread-safe source type would silently break.
#[test]
fn error_is_thread_safe_and_boxable() {
    fn assert_thread_safe<E: std::error::Error + Send + Sync + 'static>() {}
    assert_thread_safe::<Error>();
}
