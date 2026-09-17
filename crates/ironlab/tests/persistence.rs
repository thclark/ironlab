//! Saving, loading and exporting figures.

use std::path::PathBuf;

use ironlab::ir::IssueKind;
use ironlab::prelude::*;

/// Returns a path in Cargo's per-crate temporary directory, unique to the test.
fn temp_path(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ironlab-facade-tests");
    std::fs::create_dir_all(&dir).expect("create temporary directory");
    dir.join(name)
}

/// A small figure that exercises a line, a surface and a link.
fn sample_figure() -> Figure {
    let x = linspace(0.0, 1.0, 5);
    let z = Matrix::from_fn(x.len(), x.len(), |row, col| (row * col) as f64);
    let mut fig = Figure::new().tiles(1, 2).title("Sample");
    fig.axes(0, 0)
        .plot(&x, &x)
        .display_name("$y = x$")
        .marker(Marker::Circle);
    fig.axes(0, 0).legend(LegendLocation::NorthWest);
    fig.axes(0, 1).surf(&x, &x, &z);
    fig.link_all_x();
    fig
}

// WHY: `.fig.json` is the interchange format between user code, the viewer binary and
// future bindings; saving and loading must reproduce the figure exactly.
#[test]
fn save_then_load_round_trips() {
    let fig = sample_figure();
    let path = temp_path("round_trip.fig.json");
    fig.save(&path).unwrap();
    let loaded = Figure::load(&path).unwrap();
    assert_eq!(loaded, fig);
}

// WHY: a loaded figure must be extensible with the builder without identifier
// collisions, since the id allocator is not serialised.
#[test]
fn a_loaded_figure_can_be_extended() {
    let path = temp_path("extend.fig.json");
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
// a panic, and must be distinguishable from a missing file.
#[test]
fn load_rejects_a_file_that_is_not_a_figure() {
    let path = temp_path("garbage.fig.json");
    std::fs::write(&path, b"this is not json {").unwrap();
    assert!(matches!(Figure::load(&path), Err(Error::Ir(_))));
}

// WHY: see load_rejects_a_file_that_is_not_a_figure; a missing file is an I/O error.
#[test]
fn load_reports_a_missing_file_as_io() {
    let path = temp_path("does_not_exist.fig.json");
    let _ = std::fs::remove_file(&path);
    assert!(matches!(Figure::load(&path), Err(Error::Io(_))));
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
