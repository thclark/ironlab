//! Document-level structure: header, page count, page geometry, background and metadata.

use ironlab_pdf::{PdfError, PdfOptions, render_display_list};
use ironlab_scene::display::Rgba;

use crate::common::*;
use crate::require_tools;

/// The width of the default 160 mm figure in points.
const WIDTH_160_MM: f64 = 160.0 / 25.4 * 72.0;
/// The height of the default 100 mm figure in points.
const HEIGHT_100_MM: f64 = 100.0 / 25.4 * 72.0;

/// `pdfinfo` prints sizes to two decimal places.
const PDFINFO_TOLERANCE: f64 = 0.006;

#[test]
fn output_starts_with_pdf_header() {
    // WHY: every PDF consumer identifies the format by the `%PDF-` header; without it the bytes are not a PDF at all.
    let text = engine();
    let bytes = render(&page(100.0, 50.0), &text);
    assert!(
        bytes.starts_with(b"%PDF-"),
        "output starts with {:?}",
        &bytes[..bytes.len().min(16)]
    );
}

#[test]
fn document_has_exactly_one_page_sized_to_the_display_list() {
    // WHY: LaTeX includes figures unscaled, so the page size in points is the published physical size of the figure.
    require_tools!("pdfinfo");
    let ws = Workspace::new("page-size");
    let text = engine();
    let pdf = ws.write_pdf("figure", &render(&page(WIDTH_160_MM, HEIGHT_100_MM), &text));

    let info = pdfinfo(&pdf, false);
    assert_eq!(
        info.get("Pages").map(String::as_str),
        Some("1"),
        "pdfinfo: {info:?}"
    );
    let (width, height) = parse_page_size(&info["Page size"]);
    assert!(
        (width - WIDTH_160_MM).abs() < PDFINFO_TOLERANCE,
        "page width {width}, expected {WIDTH_160_MM}"
    );
    assert!(
        (height - HEIGHT_100_MM).abs() < PDFINFO_TOLERANCE,
        "page height {height}, expected {HEIGHT_100_MM}"
    );
    assert_eq!(
        info.get("Page rot").map(String::as_str),
        Some("0"),
        "a rotated page would swap the figure's axes"
    );
}

#[test]
fn media_box_and_crop_box_are_the_artwork_rectangle_at_the_origin() {
    // WHY: a CropBox differing from the MediaBox, or a box offset from the origin, makes viewers and LaTeX clip or
    // pad the figure differently from its declared size.
    require_tools!("pdfinfo");
    let ws = Workspace::new("boxes");
    let text = engine();
    let pdf = ws.write_pdf("figure", &render(&page(300.0, 125.5), &text));

    let info = pdfinfo(&pdf, true);
    let media = parse_box(&info["MediaBox"]);
    let crop = parse_box(&info["CropBox"]);
    for (actual, expected) in media.iter().zip([0.0, 0.0, 300.0, 125.5]) {
        assert!(
            (actual - expected).abs() < PDFINFO_TOLERANCE,
            "MediaBox {media:?}, expected [0, 0, 300, 125.5]"
        );
    }
    assert_eq!(media, crop, "MediaBox and CropBox differ");
}

#[test]
fn empty_display_list_produces_a_page_filled_with_the_background() {
    // WHY: the background is part of the artwork (a figure on a coloured slide), and a figure with no items must still
    // be a valid, correctly sized page rather than an error or an empty document.
    require_tools!("pdfinfo", "pdftoppm", "gs");
    let ws = Workspace::new("background");
    let text = engine();
    let mut list = page(120.0, 80.0);
    list.background = Rgba::from_u8([51, 102, 204]);

    let pdf = ws.write_pdf("figure", &render(&list, &text));
    assert_eq!(pdfinfo(&pdf, false)["Pages"], "1");
    for engine in ENGINES {
        let image = rasterise(&pdf, engine);
        assert_eq!(image.dimensions(), (120, 80), "{engine:?}: raster size");
        for (x, y) in [
            (0.5, 0.5),
            (119.5, 0.5),
            (0.5, 79.5),
            (119.5, 79.5),
            (60.0, 40.0),
        ] {
            assert_pixel(engine, &image, x, y, [51, 102, 204], 3, "background");
        }
    }
}

#[test]
fn transparent_background_paints_nothing() {
    // WHY: a figure with a transparent background is placed over coloured slides or boxes in LaTeX, so the page must
    // leave its background unpainted rather than paint an opaque rectangle (in its colour, or in white). The check uses
    // rasterisers that keep a transparent page transparent, and an opaque background as a control proving that they
    // report painted pixels as opaque.
    require_tools!(ALPHA_RASTER_TOOLS[0], ALPHA_RASTER_TOOLS[1]);
    let ws = Workspace::new("transparent-background");
    let text = engine();
    let square = ironlab_scene::display::Rect::new(40.0, 20.0, 40.0, 40.0);
    let figure = |background: Rgba| {
        let mut list = page(120.0, 80.0);
        list.background = background;
        list.items.push(filled_rect(square, RED));
        render(&list, &text)
    };
    let transparent = ws.write_pdf("transparent", &figure(Rgba::new(0.2, 0.4, 0.8, 0.0)));
    let opaque = ws.write_pdf("opaque", &figure(Rgba::WHITE));

    for engine in ENGINES {
        let image = rasterise_with_alpha(&transparent, engine);
        assert_eq!(image.dimensions(), (120, 80), "{engine:?}: raster size");
        for (x, y) in [(0.5, 0.5), (119.5, 79.5), (20.5, 40.5), (100.5, 40.5)] {
            let [_, _, _, alpha] = image.get_pixel(x as u32, y as u32).0;
            assert_eq!(
                alpha, 0,
                "{engine:?}: pixel at ({x}, {y}) outside the item has alpha {alpha}, so the background was painted"
            );
        }
        let [r, g, b, alpha] = image.get_pixel(60, 40).0;
        assert!(
            alpha == 255 && close_to([r, g, b], RED_PX, 3),
            "{engine:?}: the item on a transparent background is [{r}, {g}, {b}, {alpha}], expected opaque red"
        );

        let control = rasterise_with_alpha(&opaque, engine);
        let [r, g, b, alpha] = control.get_pixel(0, 0).0;
        assert!(
            alpha == 255 && close_to([r, g, b], WHITE_PX, 3),
            "{engine:?}: an opaque white background rasterised as [{r}, {g}, {b}, {alpha}]"
        );
    }
}

#[test]
fn options_are_written_to_the_document_metadata() {
    // WHY: the title and creator identify the figure and the software that produced it in document managers and
    // journal submission systems, which read them from the PDF metadata.
    require_tools!("pdfinfo");
    let ws = Workspace::new("metadata");
    let text = engine();
    let options = PdfOptions {
        raster: Default::default(),
        title: Some("Damped oscillation".to_owned()),
        creator: "IronLAB test suite".to_owned(),
        subject: Some("typesetter: test; fonts: A, B".to_owned()),
    };
    let bytes = render_display_list(&page(100.0, 100.0), &text, &options, None)
        .expect("render")
        .bytes;
    let info = pdfinfo(&ws.write_pdf("figure", &bytes), false);
    assert_eq!(
        info.get("Title").map(String::as_str),
        Some("Damped oscillation"),
        "pdfinfo: {info:?}"
    );
    assert_eq!(
        info.get("Creator").map(String::as_str),
        Some("IronLAB test suite"),
        "pdfinfo: {info:?}"
    );
    assert_eq!(
        info.get("Subject").map(String::as_str),
        Some("typesetter: test; fonts: A, B"),
        "pdfinfo: {info:?}"
    );
}

#[test]
fn absent_title_is_omitted_from_the_metadata() {
    // WHY: an untitled figure must not acquire a placeholder title (such as "Untitled"), which would then be shown by
    // viewers in place of the file name; the same holds for an absent subject.
    require_tools!("pdfinfo");
    let ws = Workspace::new("no-title");
    let text = engine();
    let options = PdfOptions {
        raster: Default::default(),
        title: None,
        creator: "IronLAB test suite".to_owned(),
        subject: None,
    };
    let bytes = render_display_list(&page(100.0, 100.0), &text, &options, None)
        .expect("render")
        .bytes;
    let info = pdfinfo(&ws.write_pdf("figure", &bytes), false);
    assert!(
        info.get("Title").is_none_or(String::is_empty),
        "unexpected title in {info:?}"
    );
    assert!(
        info.get("Subject").is_none_or(String::is_empty),
        "unexpected subject in {info:?}"
    );
}

#[test]
fn identical_display_lists_produce_identical_bytes() {
    // WHY: reproducible output lets figures be committed alongside papers, cached by content hash and compared in
    // review without spurious differences from timestamps or random identifiers. The renders are more than a second
    // apart, because PDF dates have a resolution of one second and two renders in quick succession would share a
    // timestamp. Both text fonts and the math font are used, so that font subsetting and resource naming are covered.
    let text = engine();
    let mut list = page(200.0, 100.0);
    list.items.push(filled_rect(
        ironlab_scene::display::Rect::new(10.0, 10.0, 50.0, 20.0),
        RED,
    ));
    list.items.extend(label(
        &text,
        "Time (s)",
        false,
        10.0,
        ironlab_scene::display::Point::new(20.0, 60.0),
    ));
    list.items.extend(label(
        &text,
        "$\\alpha^{2}$",
        true,
        10.0,
        ironlab_scene::display::Point::new(120.0, 60.0),
    ));
    let first = render(&list, &text);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let second = render(&list, &engine());
    assert!(
        first == second,
        "two renders of the same display list differ"
    );
}

#[test]
fn degenerate_page_size_is_an_error() {
    // WHY: a zero, negative or non-finite page size cannot be represented as a valid MediaBox, and must be reported to
    // the caller rather than panicking or writing a PDF that viewers reject.
    let text = engine();
    for (width, height) in [
        (0.0, 100.0),
        (100.0, -1.0),
        (f64::NAN, 100.0),
        (100.0, f64::INFINITY),
    ] {
        let result = render_display_list(&page(width, height), &text, &PdfOptions::default(), None);
        assert!(
            matches!(result, Err(PdfError::Krilla(_))),
            "page size {width} x {height} gave {:?}",
            result.map(|rendered| rendered.bytes.len())
        );
    }
}
