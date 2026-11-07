//! Drawing one figure heavier for projection: the figure's own step and the export override.
//!
//! WHY: an author who has built a figure for a paper must be able to show it on a slide without building it again.
//! The figure can store a heavier step, and an export can draw it at another step without changing the figure, so
//! that one script writes both the paper PDF and the slide PDF (or PNG) from the same figure.

use std::path::PathBuf;

use ironlab::RenderError;
use ironlab::prelude::*;

/// Returns a path in Cargo's per-crate temporary directory, unique to the test, removing any file an earlier run left.
fn fresh(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ironlab-facade-tests");
    std::fs::create_dir_all(&dir).expect("create temporary directory");
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

/// A figure with a plain title, plain and mathematical labels, a marked line and a legend.
fn slide_figure() -> Figure {
    let x = linspace(0.0, 1.0, 9);
    let y: Vec<f64> = x.iter().map(|x| x * x).collect();
    let mut fig = Figure::new()
        .size_mm(120.0, 80.0)
        .title("Pressure recovery");
    fig.axes(0, 0)
        .plot(&x, &y)
        .label("$C_p$")
        .marker(Marker::Circle);
    fig.axes(0, 0).legend(LegendLocation::NorthWest);
    fig.axes(0, 0).xlabel("Chord fraction").ylabel("$C_p$");
    fig
}

/// Unwraps what a PNG export returns: `None`, skipping the test, when no graphics adapter is available and the
/// environment variable `IRONLAB_REQUIRE_GPU` is not set, and a panic on any other error.
fn png_or_skip<T>(result: Result<T, Error>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(Error::Render(RenderError::NoAdapter(message)))
            if std::env::var_os("IRONLAB_REQUIRE_GPU").is_none() =>
        {
            eprintln!(
                "skipping: no graphics adapter ({message}); set IRONLAB_REQUIRE_GPU to make this a failure"
            );
            None
        }
        Err(error) => panic!("PNG export failed: {error}"),
    }
}

/// Counts the pixels of a PNG file that are clearly darker than a white page.
fn ink(path: &PathBuf) -> usize {
    image::open(path)
        .expect("the PNG decodes")
        .to_rgb8()
        .pixels()
        .filter(|px| px.0.iter().any(|&c| c < 160))
        .count()
}

// WHY: the builder is how an author asks for a slide figure, so it must store the step in the figure, where saving
// keeps it and the viewer shows it; a figure that asks for nothing is drawn as designed.
#[test]
fn the_builder_stores_the_step_in_the_figure_and_saving_keeps_it() {
    assert_eq!(Figure::new().ir().embolden, Embolden::Regular);
    let fig = slide_figure().embolden(Embolden::Semibold);
    assert_eq!(fig.ir().embolden, Embolden::Semibold);
    assert_eq!(
        fig.clone().embolden(Embolden::Medium).ir().embolden,
        Embolden::Medium,
        "a later call replaces the step"
    );

    for name in ["stored_step.fig", "stored_step.fig.json"] {
        let path = fresh(name);
        fig.save(&path).unwrap();
        assert_eq!(
            Figure::load(&path).unwrap().ir().embolden,
            Embolden::Semibold,
            "{name}"
        );
    }
}

// WHY: the override exists so that one figure gives both PDFs. It must change the PDF, or the slide would be as thin
// as the paper; it must draw what the figure would draw had it been stored at that step; and the paper PDF exported
// after it must be the one exported before it. (The export borrows the figure immutably, so the override cannot be
// written into the figure itself.)
#[test]
fn export_pdf_with_an_embolden_step_draws_the_slide_pdf_and_leaves_the_paper_pdf_as_it_was() {
    let fig = slide_figure();

    let plain = fresh("embolden_plain.pdf");
    fig.export_pdf(&plain).unwrap();
    let heavy = fresh("embolden_heavy.pdf");
    fig.export_pdf_with(
        &heavy,
        PdfExport {
            embolden: Some(Embolden::Semibold),
            ..PdfExport::default()
        },
    )
    .unwrap();
    let plain_after = fresh("embolden_plain_after.pdf");
    fig.export_pdf(&plain_after).unwrap();

    let stored = fresh("embolden_stored.pdf");
    fig.clone()
        .embolden(Embolden::Semibold)
        .export_pdf(&stored)
        .unwrap();

    let read = |path: &PathBuf| std::fs::read(path).unwrap();
    assert!(read(&heavy) != read(&plain), "the override changes the PDF");
    assert!(
        read(&plain_after) == read(&plain),
        "the paper PDF is the same before and after the slide PDF"
    );
    assert!(
        read(&heavy) == read(&stored),
        "the override draws the figure as if it were stored at that step"
    );
}

// WHY: slides are often made from PNG images, so the PNG export must honour the override in the same way: the image
// drawn at the heavier step carries more ink than the one drawn as designed, and is the image of the figure stored at
// that step.
#[test]
fn export_png_with_an_embolden_step_draws_the_figure_heavier_at_that_step() {
    let fig = slide_figure();

    let plain = fresh("embolden_plain.png");
    if png_or_skip(fig.export_png(&plain)).is_none() {
        return;
    }
    let heavy = fresh("embolden_heavy.png");
    fig.export_png_with(
        &heavy,
        PngExport {
            embolden: Some(Embolden::Semibold),
            ..PngExport::default()
        },
    )
    .unwrap();

    let (thin, thick) = (ink(&plain), ink(&heavy));
    assert!(
        thick > thin,
        "{thick} inked pixels at semibold against {thin} as designed"
    );

    let stored = fresh("embolden_stored.png");
    fig.clone()
        .embolden(Embolden::Semibold)
        .export_png(&stored)
        .unwrap();
    assert!(
        std::fs::read(&heavy).unwrap() == std::fs::read(&stored).unwrap(),
        "the override draws the figure as if it were stored at that step"
    );
}
