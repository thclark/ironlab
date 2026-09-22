//! Saving, loading and exporting figures.

mod common;

use std::path::PathBuf;

use common::image_figure;
use ironlab::ir::IssueKind;
use ironlab::prelude::*;
use ironlab::{
    DepthPolicy, ExportReport, ExportWarning, ExportWarningKind, SceneWarning, UnverifiedCause,
};

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

/// A figure holding a line that is drawn and a pseudocolour surface of a single row of
/// values, which validation warns of and the scene compiler leaves out, with the
/// identifier of the surface.
fn figure_with_an_undrawn_surface() -> (Figure, NodeId) {
    let x = linspace(0.0, 3.0, 4);
    let mut fig = Figure::new();
    fig.axes(0, 0).plot(&x, &x);
    let surface = fig
        .axes(0, 0)
        .surface(&x, [0.0], &Matrix::from_fn(1, 4, |_, col| col as f64))
        .id();
    (fig, surface)
}

// WHY: ADR 0012 decides that `export_pdf` returns a report of what was left off the page,
// because a program (or an agent working for a user) learns from the call itself that an
// artist is missing, rather than from a terminal it does not read or a page it does not
// look at. The report must carry the validation warnings of the figure and the warnings
// the scene compiler raised while drawing it, each naming the artist it concerns, and the
// page must still be written, because a warning never refuses a figure. The figure has no
// dense artist, so the export needs no graphics adapter.
#[test]
fn export_pdf_reports_the_artists_left_off_the_page_and_still_writes_the_file() {
    let path = fresh("undrawn_surface.pdf");
    let (fig, surface) = figure_with_an_undrawn_surface();
    let report: ExportReport = fig.export_pdf(&path).unwrap();

    let kinds: Vec<(IssueKind, Option<NodeId>)> = report
        .validation
        .iter()
        .map(|issue| (issue.kind, issue.node))
        .collect();
    assert_eq!(
        kinds,
        vec![(IssueKind::NothingToDraw, Some(surface))],
        "the validation warnings name the surface and nothing else: {:?}",
        report.validation
    );

    let named: Vec<&SceneWarning> = report
        .scene
        .iter()
        .filter(|warning| warning.node == Some(surface))
        .collect();
    assert_eq!(
        named.len(),
        1,
        "one compiler warning names the surface: {:?}",
        report.scene
    );
    assert!(
        report
            .scene
            .iter()
            .all(|warning| warning.node == Some(surface)),
        "no compiler warning concerns anything else: {:?}",
        report.scene
    );

    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"%PDF"), "the page was written");
}

// WHY: see export_pdf_reports_the_artists_left_off_the_page_and_still_writes_the_file;
// a program that tests the report to decide whether a figure is finished must find it
// empty for a figure from which nothing was left out, or every export would need
// interpreting.
#[test]
fn export_pdf_of_a_figure_with_nothing_left_out_returns_an_empty_report() {
    let path = fresh("complete.pdf");
    let report = sample_figure().export_pdf(&path).unwrap();
    assert_eq!(report.validation, vec![], "{report:?}");
    assert_eq!(report.scene, vec![], "{report:?}");
    assert!(path.exists(), "the page was written");
}

// WHY: `export_pdf_with` is `export_pdf` with the raster options chosen by the caller, so
// it must return the same report and still write the page; a caller who sets a policy
// must not lose the reasons an artist is missing. The report describes what was left off
// the page, which the raster policy does not change.
#[test]
fn export_pdf_with_returns_the_same_report_as_export_pdf() {
    let (fig, surface) = figure_with_an_undrawn_surface();
    let plain_path = fresh("undrawn_plain.pdf");
    let chosen_path = fresh("undrawn_chosen.pdf");
    let plain = fig.export_pdf(&plain_path).unwrap();
    let chosen = fig
        .export_pdf_with(
            &chosen_path,
            RasterOptions {
                policy: RasterPolicy::Never,
                ..RasterOptions::default()
            },
        )
        .unwrap();
    assert_eq!(chosen.validation, plain.validation);
    assert_eq!(chosen.scene, plain.scene);
    assert!(
        chosen
            .validation
            .iter()
            .any(|issue| issue.node == Some(surface)),
        "{chosen:?}"
    );
    assert!(
        plain_path.exists() && chosen_path.exists(),
        "both pages were written"
    );
}

/// A figure of one two-dimensional axes holding a line, which gives a rasteriser nothing
/// to check and nothing to draw.
fn two_dimensional_figure() -> Figure {
    let x = linspace(0.0, 1.0, 5);
    let mut fig = Figure::new();
    fig.axes(0, 0).plot(&x, &x);
    fig
}

/// A figure of one three-dimensional axes holding a small surface, with the identifier
/// of the axes.
fn three_dimensional_figure() -> (Figure, NodeId) {
    let x = linspace(0.0, 1.0, 5);
    let z = Matrix::from_fn(x.len(), x.len(), |row, col| (row * col) as f64);
    let mut fig = Figure::new();
    fig.axes(0, 0).surf(&x, &x, &z);
    let axes = fig.axes(0, 0).id();
    (fig, axes)
}

/// The node and kind of every export warning, which is what the tests compare; the
/// messages are prose.
fn reported(warnings: &[ExportWarning]) -> Vec<(Option<NodeId>, ExportWarningKind)> {
    warnings
        .iter()
        .map(|warning| (warning.node, warning.kind.clone()))
        .collect()
}

/// The raster options that keep every three-dimensional axes vector.
fn vector_depth() -> RasterOptions {
    RasterOptions {
        depth: DepthPolicy::Vector,
        ..RasterOptions::default()
    }
}

// WHY: the report's third list is what the exporter did to the page, as opposed to what
// was left off it: an axes rasterised, or drawn unchecked. A two-dimensional figure gives
// the exporter nothing to check and nothing to rasterise, so the list must be empty, or a
// program testing the report to decide whether a figure is finished would have to
// interpret every export.
#[test]
fn export_pdf_of_a_two_dimensional_figure_reports_nothing_from_the_exporter() {
    let path = fresh("two_dimensional.pdf");
    let report = two_dimensional_figure().export_pdf(&path).unwrap();
    assert!(report.export.is_empty(), "{report:?}");
    assert!(
        report.validation.is_empty() && report.scene.is_empty(),
        "{report:?}"
    );
    assert!(path.exists(), "the page was written");
}

// WHY: `DepthPolicy::Vector` keeps a three-dimensional axes vector, drawn in the painter's
// order, which nobody has checked against a depth test. The user who asked for it must
// be told through the report, naming the axes, that their own choice is the reason, and
// the page must still be written. The report must reach the facade's caller, because the
// facade is where a program reads it, and the export must need no graphics adapter,
// because the user asked for vectors.
#[test]
fn export_pdf_with_the_vector_depth_policy_reports_the_three_dimensional_axes_as_unverified() {
    let path = fresh("three_dimensional_vector.pdf");
    let (fig, axes) = three_dimensional_figure();
    let report = fig.export_pdf_with(&path, vector_depth()).unwrap();

    assert_eq!(
        reported(&report.export),
        vec![(
            Some(axes),
            ExportWarningKind::Unverified {
                cause: UnverifiedCause::PolicyVector,
            },
        )],
        "the exporter reports the axes it left unchecked, and nothing else: {report:?}"
    );
    assert!(
        !report.export[0].message.trim().is_empty(),
        "the warning carries a message: {report:?}"
    );
    assert!(
        report.validation.is_empty() && report.scene.is_empty(),
        "nothing was left off the page: {report:?}"
    );
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"%PDF"), "the page was written");
}

// WHY: the report names the axes each warning concerns so that a program can act on one
// axes and not another; a figure with two three-dimensional axes must therefore be
// reported once per axes, each under its own identifier, rather than once for the figure.
#[test]
fn export_pdf_with_the_vector_depth_policy_reports_each_three_dimensional_axes_once() {
    let path = fresh("two_three_dimensional_axes.pdf");
    let x = linspace(0.0, 1.0, 5);
    let z = Matrix::from_fn(x.len(), x.len(), |row, col| (row + col) as f64);
    let mut fig = Figure::new().tiles(1, 2);
    fig.axes(0, 0).surf(&x, &x, &z);
    fig.axes(0, 1).mesh(&x, &x, &z);
    let (left, right) = (fig.axes(0, 0).id(), fig.axes(0, 1).id());
    assert_ne!(left, right, "the two axes are distinct nodes");

    let report = fig.export_pdf_with(&path, vector_depth()).unwrap();
    let mut nodes: Vec<Option<NodeId>> = report.export.iter().map(|w| w.node).collect();
    nodes.sort();
    let mut expected = vec![Some(left), Some(right)];
    expected.sort();
    assert_eq!(nodes, expected, "one warning per axes: {report:?}");
    assert!(
        report.export.iter().all(|warning| {
            warning.kind
                == ExportWarningKind::Unverified {
                    cause: UnverifiedCause::PolicyVector,
                }
        }),
        "every warning gives the user's choice as the reason: {report:?}"
    );
    assert!(path.exists(), "the page was written");
}

// WHY: under the default policy the exporter proves the painter's order of a
// three-dimensional axes through the graphics adapter, and what it can prove depends on
// the machine: one with an adapter either proves the order, and reports nothing, or finds
// it wrong and embeds an image, while one without an adapter draws the axes unchecked and
// says so. A program reading the report must therefore meet exactly one of those three
// outcomes, each naming the axes, and the page must be written in every one of them, or a
// figure that exports on a workstation would fail on a build machine without a graphics
// device. The test admits every outcome rather than assuming one, because it runs on both
// kinds of machine; which one a given machine gives is the viewer's business.
#[test]
fn export_pdf_of_a_three_dimensional_figure_reports_only_what_the_machine_could_verify() {
    let path = fresh("three_dimensional_auto.pdf");
    let (fig, axes) = three_dimensional_figure();
    let report = fig.export_pdf(&path).unwrap();

    assert!(
        report.export.len() <= 1,
        "the one axes is reported at most once: {report:?}"
    );
    for warning in &report.export {
        assert_eq!(
            warning.node,
            Some(axes),
            "every warning names the axes: {report:?}"
        );
        assert!(
            matches!(
                warning.kind,
                ExportWarningKind::Unverified {
                    cause: UnverifiedCause::NoAdapter
                } | ExportWarningKind::RasterisedForDepth
            ),
            "the exporter either had no adapter to verify the axes or found its order wrong: {warning:?}"
        );
        assert!(
            !warning.message.trim().is_empty(),
            "the warning carries a message: {warning:?}"
        );
    }
    assert!(
        report.validation.is_empty() && report.scene.is_empty(),
        "nothing was left off the page: {report:?}"
    );
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        bytes.starts_with(b"%PDF"),
        "the page was written whatever the machine could verify"
    );
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
    assert!(!path.exists(), "no partial file is left behind");
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
