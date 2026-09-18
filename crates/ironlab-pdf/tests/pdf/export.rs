//! Figure export: compiling a figure IR and writing it as a PDF.
//!
//! Tests that call `export_pdf` or `write_pdf` depend on the scene compiler and pass once `ironlab_scene::compile` is
//! implemented; tests of `PdfOptions::for_figure` do not.

use ironlab_ir::{Artist, Axes, Axis, Figure, FigureSize, Line, NdArray, Provenance, Text};
use ironlab_pdf::{PdfError, PdfOptions, export_pdf, write_pdf};

use crate::common::*;
use crate::require_tools;

/// A single-column journal figure: 85 mm by 60 mm, deliberately not the default size.
const WIDTH_MM: f64 = 85.0;
const HEIGHT_MM: f64 = 60.0;

/// A figure with a title, one 2D axes with an x label, and one damped-oscillation line.
fn damped_oscillation() -> Figure {
    let mut figure = Figure::new();
    figure.size = FigureSize {
        width_mm: WIDTH_MM,
        height_mm: HEIGHT_MM,
    };
    figure.title = Some(Text::new("Damped oscillation"));
    let t: Vec<f64> = (0..200).map(|i| f64::from(i) * 0.05).collect();
    let y: Vec<f64> = t
        .iter()
        .map(|t| (-0.3 * t).exp() * (2.0 * t).cos())
        .collect();
    let x_id = figure.add_data(NdArray::vector(t));
    let y_id = figure.add_data(NdArray::vector(y));
    let axes_id = figure.alloc_node_id();
    let line_id = figure.alloc_node_id();
    figure.axes.push(Axes {
        id: axes_id,
        x: Axis {
            label: Some(Text::new("Time (s)")),
            ..Axis::default()
        },
        artists: vec![Artist::Line(Line {
            id: line_id,
            x: x_id,
            y: y_id,
            ..Line::default()
        })],
        ..Axes::default()
    });
    figure
}

#[test]
fn exported_figure_is_one_page_at_the_figure_size_with_its_text() {
    // WHY: this is the end-to-end publication contract: the figure's physical size becomes the page size, and its
    // title and labels are real text in embedded fonts.
    require_tools!("pdfinfo", "pdftotext", "pdffonts");
    let ws = Workspace::new("export");
    let text = engine();
    let bytes = export_pdf(&damped_oscillation(), &text, None).expect("export figure");
    assert!(bytes.starts_with(b"%PDF-"));
    let pdf = ws.write_pdf("figure", &bytes);

    let info = pdfinfo(&pdf, false);
    assert_eq!(
        info.get("Pages").map(String::as_str),
        Some("1"),
        "pdfinfo: {info:?}"
    );
    let (width, height) = parse_page_size(&info["Page size"]);
    let (expected_width, expected_height) = (WIDTH_MM / 25.4 * 72.0, HEIGHT_MM / 25.4 * 72.0);
    assert!(
        (width - expected_width).abs() < 0.006,
        "page width {width}, expected {expected_width}"
    );
    assert!(
        (height - expected_height).abs() < 0.006,
        "page height {height}, expected {expected_height}"
    );

    let extracted = pdftotext(&pdf);
    assert!(
        extracted.contains("Damped oscillation"),
        "title missing from {extracted:?}"
    );
    assert!(
        extracted.contains("Time (s)"),
        "x label missing from {extracted:?}"
    );

    let fonts = pdffonts(&pdf);
    assert!(
        !fonts.is_empty(),
        "the figure has text but the PDF has no fonts"
    );
    assert!(
        fonts.iter().all(|f| f.embedded && f.subset),
        "fonts not embedded and subset: {fonts:?}"
    );
}

#[test]
fn exported_figure_rasterises_identically_in_both_engines() {
    // WHY: a complete figure exercises clipping, many strokes and mixed fonts together, which is where invalid content
    // streams that one engine tolerates are most likely.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("export-engines");
    let text = engine();
    let pdf = ws.write_pdf(
        "figure",
        &export_pdf(&damped_oscillation(), &text, None).expect("export figure"),
    );
    let poppler = rasterise(&pdf, Engine::Poppler);
    let ghostscript = rasterise(&pdf, Engine::Ghostscript);
    assert_engines_agree(&poppler, &ghostscript);
}

#[test]
fn write_pdf_writes_the_exported_bytes() {
    // WHY: `write_pdf` is the viewer's "Export PDF…" path and must not diverge from `export_pdf`, which is what the
    // rest of this suite checks.
    let ws = Workspace::new("write");
    let text = engine();
    let figure = damped_oscillation();
    let path = ws.dir.join("written.pdf");
    write_pdf(&figure, &text, None, &path).expect("write figure");
    let written = std::fs::read(&path).expect("read written PDF");
    let exported = export_pdf(&figure, &text, None).expect("export figure");
    assert!(
        written == exported,
        "written file differs from exported bytes"
    );
}

#[test]
fn write_pdf_to_an_unwritable_path_is_an_io_error() {
    // WHY: the viewer reports I/O failures (a missing directory, a read-only volume) to the user, which requires them to
    // be distinguishable from export failures.
    let ws = Workspace::new("write-error");
    let text = engine();
    let path = ws.dir.join("missing-directory").join("figure.pdf");
    let result = write_pdf(&damped_oscillation(), &text, None, &path);
    assert!(
        matches!(result, Err(PdfError::Io(_))),
        "writing to {} gave {result:?}",
        path.display()
    );
}

/// A provenance that differs from the default in every field, so that tests can tell values taken from the figure from
/// hard-coded defaults.
fn distinctive_provenance() -> Provenance {
    Provenance {
        ironlab_version: "0.0.0-test".to_owned(),
        typesetter: "latex-rust 9.9.9-test".to_owned(),
        fonts: vec!["Test Serif".to_owned(), "Test Math".to_owned()],
    }
}

#[test]
fn figure_options_take_the_title_source_and_record_provenance() {
    // WHY: the PDF title is what document managers show for the file, so it must be the figure's own title; LaTeX
    // markup is kept as written because metadata cannot hold typeset mathematics and the source is what the author
    // wrote. The subject records which version of IronLAB wrote the figure and which typesetter and fonts produced the
    // text, so that a figure whose rendering changes between releases can be traced; values must come from the figure,
    // not from defaults compiled into the exporter.
    let mut figure = Figure::new();
    figure.title = Some(Text::new("Decay of $\\alpha$ particles"));
    figure.provenance = distinctive_provenance();

    let options = PdfOptions::for_figure(&figure);
    assert_eq!(
        options.title.as_deref(),
        Some("Decay of $\\alpha$ particles")
    );
    assert_eq!(
        options.creator,
        format!("IronLAB {}", env!("CARGO_PKG_VERSION")),
        "the creator names the exporting version of IronLAB"
    );
    let subject = options.subject.expect("the subject records provenance");
    for expected in [
        "written by IronLAB 0.0.0-test",
        "typesetter: latex-rust 9.9.9-test",
        "fonts: Test Serif, Test Math",
    ] {
        assert!(
            subject.contains(expected),
            "subject {subject:?} does not contain {expected:?}"
        );
    }
}

#[test]
fn figure_options_omit_the_title_of_an_untitled_figure() {
    // WHY: an untitled figure must not be given a placeholder title, which viewers would show in place of the file name.
    let figure = Figure::new();
    assert_eq!(PdfOptions::for_figure(&figure).title, None);
}

#[test]
fn exported_figure_metadata_names_its_title_creator_and_provenance() {
    // WHY: `export_pdf` must actually apply the figure's options, as read back by a real consumer of the file.
    require_tools!("pdfinfo");
    let ws = Workspace::new("export-metadata");
    let text = engine();
    let mut figure = damped_oscillation();
    figure.provenance = distinctive_provenance();
    let pdf = ws.write_pdf(
        "figure",
        &export_pdf(&figure, &text, None).expect("export figure"),
    );

    let info = pdfinfo(&pdf, false);
    assert_eq!(
        info.get("Title").map(String::as_str),
        Some("Damped oscillation"),
        "pdfinfo: {info:?}"
    );
    assert_eq!(
        info.get("Creator").cloned(),
        Some(format!("IronLAB {}", env!("CARGO_PKG_VERSION"))),
        "pdfinfo: {info:?}"
    );
    let subject = info.get("Subject").cloned().unwrap_or_default();
    assert!(
        subject.contains("latex-rust 9.9.9-test") && subject.contains("Test Serif"),
        "subject {subject:?} does not record the figure's provenance; pdfinfo: {info:?}"
    );
}

/// A figure holding one line of `n` points, far more than a page of this size can resolve.
fn dense_line(n: usize) -> Figure {
    let mut figure = Figure::new();
    figure.size = FigureSize {
        width_mm: WIDTH_MM,
        height_mm: HEIGHT_MM,
    };
    let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let y: Vec<f64> = (0..n).map(|i| (i as f64 / 250.0).sin()).collect();
    let x_id = figure.add_data(NdArray::vector(x));
    let y_id = figure.add_data(NdArray::vector(y));
    let axes_id = figure.alloc_node_id();
    let line_id = figure.alloc_node_id();
    figure.axes.push(Axes {
        id: axes_id,
        artists: vec![Artist::Line(Line {
            id: line_id,
            x: x_id,
            y: y_id,
            ..Line::default()
        })],
        ..Axes::default()
    });
    figure
}

/// Counts the path segments of every leaf of a display list.
fn segment_count(list: &ironlab_scene::display::DisplayList) -> usize {
    let mut total = 0;
    list.visit_leaves(|item, _, _| {
        if let ironlab_scene::display::ItemKind::Path(path) = &item.kind {
            total += path.segments.len();
        }
    });
    total
}

#[test]
fn a_dense_figure_exports_exactly_the_geometry_that_the_screen_draws() {
    // WHY: decimation must not be a property of the canvas. The exporter draws the display list of the compiled
    // scene, which is the very list the viewer tessellates, so a series thinned for the current view is thinned
    // identically in the vector file. Were the two to diverge, a user would export a figure they had never seen.
    let text = engine();
    let points = 200_000;
    let figure = dense_line(points);
    let scene = ironlab_scene::compile(&figure, &text);

    let segments = segment_count(&scene.display_list);
    assert!(
        segments < points / 20,
        "the compiled scene the exporter draws holds {segments} path segments for {points} data points"
    );

    let exported = export_pdf(&figure, &text).expect("export figure");
    let from_the_compiled_scene = ironlab_pdf::render_display_list(
        &scene.display_list,
        &text,
        &PdfOptions::for_figure(&figure),
    )
    .expect("render the compiled display list");
    assert_eq!(
        exported, from_the_compiled_scene,
        "exporting a figure draws the compiled scene and nothing else"
    );
    assert!(
        exported.len() < 200_000,
        "the exported file is {} bytes, so the decimated geometry reached the vector output",
        exported.len()
    );
}
