//! Emboldened text: glyph runs drawn with a stroke along their outlines, and figures exported at a heavier step.
//!
//! WHY: mathematics, monospace and bold text have no heavier designed face, so a figure drawn for projection thickens
//! them by stroking their outlines. The stroke must be drawn in the run's colour, must leave the text selectable and
//! searchable in the PDF, and must not darken translucent text where it overlaps the fill.

use ironlab_ir::{Artist, Axes, Axis, Embolden, Figure, FigureSize, NdArray, Series, Text};
use ironlab_pdf::export_pdf;
use ironlab_scene::display::{DisplayList, ItemKind, Point, Rect, Rgba};

use crate::common::*;
use crate::require_tools;

/// The stroke of the heaviest step, in ems: STIX Two Text SemiBold's stem less Regular's, as calibrated.
const STROKE_EM: f64 = 0.038;

/// A page holding `content` laid out at `size_pt` with its baseline origin at `origin`, in `color`, every glyph run
/// stroked by `stroke_em`.
fn stroked_label(
    text: &ironlab_text::TextEngine,
    content: &str,
    size_pt: f64,
    origin: Point,
    color: Rgba,
    stroke_em: f64,
) -> DisplayList {
    let mut list = page(240.0, 140.0);
    let mut items = text_items(&text.layout(content, true, size_pt), origin, color);
    for item in &mut items {
        if let ItemKind::Glyphs(glyphs) = &mut item.kind {
            glyphs.stroke_em = stroke_em;
        }
    }
    list.items.extend(items);
    list
}

#[test]
fn a_stroked_glyph_run_is_heavier_in_its_own_colour_and_remains_extractable() {
    // WHY: the stroke is what makes mathematics as heavy as the designed face of the text beside it, so it must add
    // ink, all of it in the run's colour (a black or default-coloured stroke would outline red text in black), and the
    // run must still be real text that a reader can select and search.
    require_tools!("pdftotext", RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let text = engine();
    let origin = Point::new(20.0, 90.0);
    let content = r"Hl $x^2$";
    let region = Rect::new(0.0, 0.0, 240.0, 140.0);
    let is_red_ink = |px: [u8; 3]| px[0] > 200 && px[1] < 160 && px[2] < 160;
    let is_off_colour = |px: [u8; 3]| px[0] < 200;

    let ws = Workspace::new("stroked-run");
    let thin = ws.write_pdf(
        "thin",
        &render(
            &stroked_label(&text, content, 48.0, origin, RED, 0.0),
            &text,
        ),
    );
    let heavy = ws.write_pdf(
        "heavy",
        &render(
            &stroked_label(&text, content, 48.0, origin, RED, STROKE_EM),
            &text,
        ),
    );

    for engine in ENGINES {
        let thin_ink = count_pixels(&rasterise(&thin, engine), region, is_red_ink);
        let heavy_image = rasterise(&heavy, engine);
        let heavy_ink = count_pixels(&heavy_image, region, is_red_ink);
        assert!(
            heavy_ink as f64 > 1.1 * thin_ink as f64,
            "{engine:?}: the stroked run covers {heavy_ink} red pixels against {thin_ink} unstroked"
        );
        assert_eq!(
            count_pixels(&heavy_image, region, is_off_colour),
            0,
            "{engine:?}: the stroke is drawn in a colour other than the run's"
        );
    }
    let extracted = pdftotext(&heavy);
    assert!(
        extracted.contains("Hl"),
        "the stroked run is selectable text: {extracted:?}"
    );
    assert_eq!(
        pdftotext(&thin).split_whitespace().collect::<Vec<_>>(),
        extracted.split_whitespace().collect::<Vec<_>>(),
        "stroking changes nothing about the extracted text"
    );
}

#[test]
fn translucent_stroked_text_is_not_darker_where_the_stroke_overlaps_the_fill() {
    // WHY: a translucent label drawn as a fill and a stroke composited separately would be twice as dark along the
    // inside of every outline, where the two overlap, so an emboldened translucent label would look blotched. The run
    // must composite as one shape: inside the stem, both where the stroke overlaps the fill (just inside the outline)
    // and where it does not (the middle), the colour is that of the translucent fill alone. The glyph is large so that
    // the overlap, half the stroke wide, spans several pixels at the 72 dpi the PDF is rasterised at.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let text = engine();
    let size = 160.0;
    let origin = Point::new(60.0, 120.0);
    let translucent = Rgba::new(1.0, 0.0, 0.0, 0.5);
    let ws = Workspace::new("translucent-stroke");
    let opaque = ws.write_pdf(
        "opaque",
        &render(&stroked_label(&text, "l", size, origin, RED, 0.0), &text),
    );
    let fill = ws.write_pdf(
        "fill",
        &render(
            &stroked_label(&text, "l", size, origin, translucent, 0.0),
            &text,
        ),
    );
    let stroked = ws.write_pdf(
        "stroked",
        &render(
            &stroked_label(&text, "l", size, origin, translucent, STROKE_EM),
            &text,
        ),
    );

    for engine in ENGINES {
        // The middle of the stem of the l at a quarter of the em above the baseline, found from the opaque glyph.
        let row = origin.y - 0.25 * size;
        let image = rasterise(&opaque, engine);
        let columns: Vec<u32> = (0..image.width())
            .filter(|&x| pixel(&image, f64::from(x) + 0.5, row)[1] < 60)
            .collect();
        assert!(
            columns.len() >= 4,
            "{engine:?}: the stem is found: {columns:?}"
        );
        // The second inked column lies about a pixel inside the outline, well within the inner half of a stroke of
        // 0.038 × 160 ≈ 6 pt, so it is where a fill and a stroke composited separately would double up.
        let inside_edge = f64::from(columns[1]) + 0.5;
        let middle = f64::from(columns[columns.len() / 2]) + 0.5;
        let (fill_image, stroked_image) = (rasterise(&fill, engine), rasterise(&stroked, engine));
        for (place, x) in [
            ("just inside the outline", inside_edge),
            ("in the middle", middle),
        ] {
            let fill_px = pixel(&fill_image, x, row);
            let stroked_px = pixel(&stroked_image, x, row);
            assert!(
                close_to(stroked_px, fill_px, 12),
                "{engine:?}: {place} of the stem the stroked translucent l is {stroked_px:?}, the fill alone \
                 {fill_px:?}"
            );
            assert!(
                close_to(fill_px, [255, 128, 128], 16),
                "{engine:?}: half-transparent red is {fill_px:?}"
            );
        }
    }
}

/// A figure with a plain title, a plain x label and a mathematical y label over one line.
fn labelled_figure(step: Embolden) -> Figure {
    let mut figure = Figure::new();
    figure.size = FigureSize {
        width_mm: 85.0,
        height_mm: 60.0,
    };
    figure.embolden = step;
    figure.title = Some(Text::new("Damped oscillation"));
    let t: Vec<f64> = (0..50).map(|i| f64::from(i) * 0.2).collect();
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
        y: Axis {
            label: Some(Text::new(r"$\Delta p$ (kPa)")),
            ..Axis::default()
        },
        artists: vec![Artist::Series(Series {
            id: line_id,
            x: x_id,
            y: y_id,
            ..Series::default()
        })],
        ..Axes::default()
    });
    figure
}

#[test]
fn a_figure_stored_at_a_heavier_step_exports_heavier_with_the_same_selectable_text() {
    // WHY: this is the end-to-end contract of emboldening for slides: the same figure, stored at a heavier step,
    // exports with its plain text in the heavier designed face and with more ink, and a reader can still select and
    // search every label as in the print figure.
    require_tools!("pdftotext", "pdffonts", RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let text = engine();
    let ws = Workspace::new("emboldened-figure");
    let regular = export_pdf(&labelled_figure(Embolden::Regular), &text, None).expect("export");
    let semibold = export_pdf(&labelled_figure(Embolden::Semibold), &text, None).expect("export");
    assert!(semibold.warnings.is_empty(), "{:?}", semibold.warnings);
    let regular = ws.write_pdf("regular", &regular.bytes);
    let semibold = ws.write_pdf("semibold", &semibold.bytes);

    let fonts = pdffonts(&semibold);
    assert!(
        fonts
            .iter()
            .any(|f| f.name.contains("STIXTwoText-SemiBold")),
        "the plain text is set in SemiBold: {fonts:?}"
    );
    assert!(
        !fonts.iter().any(|f| f.name.contains("STIXTwoText-Regular")),
        "no plain text is left in the regular face: {fonts:?}"
    );
    let words = |pdf| {
        pdftotext(pdf)
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let heavy_words = words(&semibold);
    for expected in ["Damped", "oscillation", "Time", "(s)", "(kPa)"] {
        assert!(
            heavy_words.iter().any(|w| w == expected),
            "{expected:?} is selectable: {heavy_words:?}"
        );
    }
    assert_eq!(
        heavy_words,
        words(&regular),
        "the text is the same at both steps"
    );

    let page = Rect::new(0.0, 0.0, 85.0 / 25.4 * 72.0, 60.0 / 25.4 * 72.0);
    for engine in ENGINES {
        let thin = count_pixels(&rasterise(&regular, engine), page, is_ink);
        let heavy = count_pixels(&rasterise(&semibold, engine), page, is_ink);
        assert!(
            heavy > thin,
            "{engine:?}: {heavy} ink pixels at semibold against {thin} at regular"
        );
    }
}
