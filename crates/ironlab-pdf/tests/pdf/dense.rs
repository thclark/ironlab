//! Raster fallback for dense content: when it is used, where the image lands and how it is embedded.
//!
//! The rasteriser here is the stub of `common.rs`, which paints a flat colour and records what it was asked to
//! render. That is deliberate: these tests are about the exporter's decisions and geometry, which must be exact, and
//! a stub makes them exact. That the pixels themselves come from the viewer's own GPU pipeline, and land where the
//! vector geometry would have, is proved end to end in `ironlab-viewer/tests/export.rs`, which has access to the
//! renderer this crate must not depend on.
//!
//! Pages are rasterised at 72 dpi, so one pixel is one point and pixel coordinates equal display-list coordinates.

use ironlab_pdf::{
    ExportWarningKind, PdfError, PdfOptions, RasterOptions, RasterPolicy, Rasteriser,
    render_display_list,
};
use ironlab_scene::display::{DisplayList, ItemKind, Rect, Rgba, Transform};

use crate::common::*;
use crate::require_tools;

/// Options that rasterise whatever is marked dense, at `dpi`.
fn always(dpi: f64) -> PdfOptions {
    PdfOptions {
        raster: RasterOptions {
            policy: RasterPolicy::Always,
            dpi,
            ..RasterOptions::default()
        },
        ..PdfOptions::default()
    }
}

/// Renders a list with the given options and rasteriser, panicking on failure, and returns the bytes of the page.
fn render_with(
    list: &DisplayList,
    options: &PdfOptions,
    raster: Option<&mut dyn Rasteriser>,
) -> Vec<u8> {
    render_reporting(list, options, raster).bytes
}

/// A page holding one dense red square, 40 by 20 points at (20, 10), on a 200 by 100 point page.
fn page_with_square(cells: u64) -> DisplayList {
    let mut list = page(200.0, 100.0);
    list.items.push(dense(
        cells,
        vec![filled_rect(Rect::new(20.0, 10.0, 40.0, 20.0), RED)],
    ));
    list
}

// WHY: this is the whole promise of the feature — the image must land exactly where the paths it replaces would
// have, at the same size. A raster placed in PDF's native y-up space, scaled by the resolution, or anchored at the
// page origin rather than at the content, would put the colour somewhere else, and this samples the four sides of
// the square to catch each of those.
#[test]
fn a_rasterised_square_covers_exactly_the_area_its_paths_covered() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("dense-place");
    let list = page_with_square(10_000);
    let (mut stub, _) = Stub::flat([255, 0, 0, 255]);
    let pdf = ws.write_pdf(
        "figure",
        &render_with(&list, &always(72.0), Some(&mut stub)),
    );

    for &viewer in &ENGINES {
        let image = rasterise(&pdf, viewer);
        assert_pixel(viewer, &image, 40.5, 20.5, RED_PX, 3, "the square's centre");
        assert_pixel(viewer, &image, 21.5, 11.5, RED_PX, 3, "its top-left corner");
        assert_pixel(
            viewer,
            &image,
            58.5,
            28.5,
            RED_PX,
            3,
            "its bottom-right corner",
        );
        assert_pixel(viewer, &image, 18.5, 20.5, WHITE_PX, 3, "left of it");
        assert_pixel(viewer, &image, 61.5, 20.5, WHITE_PX, 3, "right of it");
        assert_pixel(viewer, &image, 40.5, 7.5, WHITE_PX, 3, "above it");
        assert_pixel(viewer, &image, 40.5, 31.5, WHITE_PX, 3, "below it");
        assert_pixel(
            viewer,
            &image,
            40.5,
            79.5,
            WHITE_PX,
            3,
            "where a vertically flipped square would be",
        );
    }
}

// WHY: dense content is usually nested inside the group that clips an axes to its plot rectangle, and a PDF image is
// drawn in the space of the enclosing transforms. Placing the image in figure space means undoing those transforms;
// getting the inverse wrong, or forgetting it, moves the raster by the group's offset. A translated group is the
// smallest case that tells the two apart.
#[test]
fn a_raster_inside_a_translated_group_lands_where_its_paths_would_have() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("dense-transform");
    let mut list = page(200.0, 100.0);
    list.items.push(group(
        None,
        Some(Transform::translate(100.0, 40.0)),
        vec![dense(
            10_000,
            vec![filled_rect(Rect::new(0.0, 0.0, 40.0, 20.0), RED)],
        )],
    ));
    let (mut stub, _) = Stub::flat([255, 0, 0, 255]);
    let pdf = ws.write_pdf(
        "figure",
        &render_with(&list, &always(72.0), Some(&mut stub)),
    );

    for &viewer in &ENGINES {
        let image = rasterise(&pdf, viewer);
        assert_pixel(viewer, &image, 120.5, 50.5, RED_PX, 3, "inside the square");
        assert_pixel(
            viewer,
            &image,
            20.5,
            10.5,
            WHITE_PX,
            3,
            "where an untransformed raster would have landed",
        );
        assert_pixel(
            viewer,
            &image,
            180.5,
            85.5,
            WHITE_PX,
            3,
            "where a doubly translated raster would have landed",
        );
    }
}

// WHY: a clip that the vector geometry obeys must bind the raster too, or turning rasterisation on would let a
// surface spill out of its plot box. Both halves matter: the exporter must not render pixels it will not show, and
// the PDF clip must still be in force around the image.
#[test]
fn a_raster_is_clipped_to_the_box_that_would_have_clipped_its_paths() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("dense-clip");
    let mut list = page(200.0, 100.0);
    list.items.push(group(
        Some(Rect::new(0.0, 0.0, 50.0, 100.0)),
        None,
        vec![dense(
            10_000,
            vec![filled_rect(Rect::new(20.0, 10.0, 100.0, 20.0), RED)],
        )],
    ));
    let (mut stub, calls) = Stub::flat([255, 0, 0, 255]);
    let pdf = ws.write_pdf(
        "figure",
        &render_with(&list, &always(72.0), Some(&mut stub)),
    );

    let rendered = &calls.borrow()[0].0;
    assert!(
        rendered.width_pt <= 30.0,
        "the exporter renders only the 30 points of the square inside the clip, not {} points",
        rendered.width_pt
    );
    for &viewer in &ENGINES {
        let image = rasterise(&pdf, viewer);
        assert_pixel(viewer, &image, 40.5, 20.5, RED_PX, 3, "inside the clip");
        assert_pixel(viewer, &image, 55.5, 20.5, WHITE_PX, 3, "beyond the clip");
        assert_pixel(viewer, &image, 100.5, 20.5, WHITE_PX, 3, "well beyond it");
    }
}

// WHY: the raster replaces one artist, not the page, so whatever was painted before it must still show through where
// the artist is transparent, and whatever is painted after must still cover it. A raster drawn opaquely, or with its
// alpha premultiplied twice, or composited in the wrong order, fails one of these three samples.
#[test]
fn a_semi_transparent_raster_composites_with_the_vectors_around_it() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("dense-alpha");
    let mut list = page(200.0, 100.0);
    list.items
        .push(filled_rect(Rect::new(0.0, 0.0, 100.0, 100.0), BLUE));
    list.items.push(dense(
        10_000,
        vec![filled_rect(Rect::new(20.0, 10.0, 160.0, 20.0), RED)],
    ));
    list.items
        .push(filled_rect(Rect::new(150.0, 0.0, 50.0, 100.0), Rgba::BLACK));
    let (mut stub, _) = Stub::flat([255, 0, 0, 128]);
    let pdf = ws.write_pdf(
        "figure",
        &render_with(&list, &always(72.0), Some(&mut stub)),
    );

    for &viewer in &ENGINES {
        let image = rasterise(&pdf, viewer);
        assert_pixel(
            viewer,
            &image,
            50.5,
            20.5,
            [128, 0, 127],
            4,
            "half red over blue",
        );
        assert_pixel(
            viewer,
            &image,
            120.5,
            20.5,
            [255, 128, 128],
            4,
            "half red over the white page",
        );
        assert_pixel(
            viewer,
            &image,
            170.5,
            20.5,
            BLACK_PX,
            3,
            "the rectangle painted after the raster covers it",
        );
    }
}

// WHY: the resolution is the parameter a user sets to trade file size against print quality, so it must decide the
// number of samples and nothing else: the image must keep the same physical rectangle on the page and simply gain
// pixels. A resolution that leaked into the placement would rescale the figure.
#[test]
fn the_export_resolution_sets_the_sample_count_and_not_the_size_on_the_page() {
    require_tools!(IMAGE_TOOL);
    let ws = Workspace::new("dense-dpi");
    let list = page_with_square(10_000);

    for (dpi, expected) in [(72.0, (40, 20)), (144.0, (80, 40)), (288.0, (160, 80))] {
        let (mut stub, calls) = Stub::flat([255, 0, 0, 255]);
        let pdf = ws.write_pdf(
            &format!("figure-{dpi}"),
            &render_with(&list, &always(dpi), Some(&mut stub)),
        );
        assert_eq!(
            calls.borrow()[0].1,
            dpi,
            "the rasteriser is asked for {dpi} dots per inch"
        );
        let images = pdfimages(&pdf);
        assert_eq!(images.len(), 1, "one image at {dpi} dots per inch");
        assert_eq!(
            (images[0].width, images[0].height),
            expected,
            "sample count at {dpi} dots per inch"
        );
        assert!(
            (images[0].x_ppi - dpi).abs() <= 1.0 && (images[0].y_ppi - dpi).abs() <= 1.0,
            "the image is placed at {dpi} pixels per inch on the page, not {} by {}",
            images[0].x_ppi,
            images[0].y_ppi
        );
    }
}

// WHY: a figure is a print master. Lossy compression would put artefacts on the boundaries between faces, and
// interpolation would blur the hard edges that the vector drawing renders sharply, so the image must be deflated and
// must not be smoothed. The PDF default for `/Interpolate` is false, so writing no key is exactly what is wanted.
#[test]
fn the_image_is_deflated_and_not_interpolated() {
    require_tools!(IMAGE_TOOL);
    let ws = Workspace::new("dense-encoding");
    let (mut stub, _) = Stub::flat([255, 0, 0, 255]);
    let pdf = ws.write_pdf(
        "figure",
        &render_with(&page_with_square(10_000), &always(72.0), Some(&mut stub)),
    );

    let images = pdfimages(&pdf);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].encoding, "image", "deflated, not JPEG-compressed");
    assert!(!images[0].interpolated, "the image is not smoothed");
    assert!(
        String::from_utf8_lossy(&std::fs::read(&pdf).expect("read the PDF"))
            .contains("/FlateDecode"),
        "the image stream is written with the FlateDecode filter"
    );
}

// WHY: an image with no transparency needs no soft mask, and writing one anyway adds a quarter to the file for a
// stream of constant 255s and marks the page as using transparency, which strict print profiles restrict. The
// exporter must therefore notice an opaque readback, and must keep the mask when there is real transparency.
#[test]
fn a_soft_mask_is_written_only_when_the_raster_is_transparent() {
    require_tools!(IMAGE_TOOL);
    let ws = Workspace::new("dense-smask");
    let list = page_with_square(10_000);

    let (mut opaque, _) = Stub::flat([255, 0, 0, 255]);
    let pdf = ws.write_pdf(
        "opaque",
        &render_with(&list, &always(72.0), Some(&mut opaque)),
    );
    let images = pdfimages(&pdf);
    assert_eq!(
        images.len(),
        1,
        "an opaque raster is one object: {images:?}"
    );
    assert_eq!(images[0].components, 3, "red, green and blue only");

    let (mut translucent, _) = Stub::flat([255, 0, 0, 128]);
    let pdf = ws.write_pdf(
        "translucent",
        &render_with(&list, &always(72.0), Some(&mut translucent)),
    );
    let images = pdfimages(&pdf);
    assert_eq!(
        images.len(),
        2,
        "a translucent raster is an image and its soft mask: {images:?}"
    );
    assert!(
        images.iter().any(|i| i.kind == "smask"),
        "one of them is the soft mask: {images:?}"
    );
}

// WHY: the threshold exists so that a figure only pays for a raster when vectors would be worse, and the boundary is
// the only interesting part of it. A surface of exactly the threshold size must rasterise and one cell smaller must
// not, and in the vector case the paths must actually be drawn rather than silently dropped. The report must follow
// the same boundary: a raster is reported with the cell count that forced it, naming the surface and the option
// that keeps it vector, so that a program reading the report can tell a figure's size from a user's request and a
// reader knows what to change, and a surface kept vector is not reported at all, or the report would have something
// to say about every export.
#[test]
fn the_threshold_decides_at_its_own_boundary_and_the_vector_case_still_draws() {
    require_tools!(IMAGE_TOOL, RASTER_TOOLS[0]);
    let ws = Workspace::new("dense-threshold");
    let options = PdfOptions {
        raster: RasterOptions {
            policy: RasterPolicy::Auto { cells: 100 },
            dpi: 72.0,
            ..RasterOptions::default()
        },
        ..PdfOptions::default()
    };

    for (cells, rasterised) in [(99, false), (100, true), (101, true)] {
        let name = format!("{cells} cells against a threshold of 100");
        let (mut stub, calls) = Stub::flat([255, 0, 0, 255]);
        let rendered = render_reporting(&page_with_square(cells), &options, Some(&mut stub));
        let pdf = ws.write_pdf(&format!("figure-{cells}"), &rendered.bytes);
        assert_eq!(
            !calls.borrow().is_empty(),
            rasterised,
            "{name}: the rasteriser is asked exactly when the surface is rasterised"
        );
        assert_eq!(
            pdfimages(&pdf).len(),
            usize::from(rasterised),
            "{name}: images embedded"
        );
        let image = rasterise(&pdf, Engine::Poppler);
        assert_pixel(
            Engine::Poppler,
            &image,
            40.5,
            20.5,
            RED_PX,
            3,
            &format!("{name}: the square is drawn either way"),
        );
        let expected: Vec<_> = rasterised
            .then_some((SURFACE, ExportWarningKind::RasterisedForSize { cells }))
            .into_iter()
            .collect();
        assert_warnings(&rendered.warnings, &expected, &name);
        if rasterised {
            assert_message_names(
                &rendered.warnings[0],
                "`RasterPolicy::Never`",
                &format!("{name}: the option that keeps the surface vector"),
            );
        }
    }
}

// WHY: the user override has to beat the threshold in both directions, because the threshold is a guess about what is
// worth rasterising and the user knows their figure: a coarse surface that must be a raster because it will be
// scaled down, or a huge one that must stay vector because it is going to be edited. The report must give the
// override as the reason rather than a cell count, naming the option, so that a program reading it does not conclude
// that the surface was too big, and must say nothing when the override kept the surface vector, because nothing was
// done to it.
#[test]
fn the_user_override_beats_the_threshold_in_both_directions() {
    require_tools!(IMAGE_TOOL);
    let ws = Workspace::new("dense-override");
    let mut never = always(72.0);
    never.raster.policy = RasterPolicy::Never;
    let (mut stub, calls) = Stub::flat([255, 0, 0, 255]);
    let rendered = render_reporting(&page_with_square(1_000_000), &never, Some(&mut stub));
    let pdf = ws.write_pdf("never", &rendered.bytes);
    assert!(
        calls.borrow().is_empty() && pdfimages(&pdf).is_empty(),
        "a million cells stay vector when the user says never"
    );
    assert!(
        rendered.warnings.is_empty(),
        "nothing was rasterised, so there is nothing to report: {:?}",
        rendered.warnings
    );

    let (mut stub, calls) = Stub::flat([255, 0, 0, 255]);
    let rendered = render_reporting(&page_with_square(1), &always(72.0), Some(&mut stub));
    let pdf = ws.write_pdf("always", &rendered.bytes);
    assert!(
        calls.borrow().len() == 1 && pdfimages(&pdf).len() == 1,
        "a single cell is rasterised when the user says always"
    );
    assert_warnings(
        &rendered.warnings,
        &[(SURFACE, ExportWarningKind::RasterisedByRequest)],
        "the raster is reported as the user's request, naming the surface",
    );
    assert_message_names(
        &rendered.warnings[0],
        "`RasterPolicy::Always`",
        "the request that forced the raster",
    );
}

// WHY: a caller that has no renderer, such as a build that never touches a GPU, must still be able to export, and it
// must get the figure rather than a hole where the surface was. Passing no rasteriser is therefore a complete
// answer, not an error, whatever the policy says.
#[test]
fn without_a_rasteriser_dense_content_is_drawn_as_vector_geometry() {
    require_tools!(IMAGE_TOOL, RASTER_TOOLS[0]);
    let ws = Workspace::new("dense-none");
    let pdf = ws.write_pdf(
        "figure",
        &render_with(&page_with_square(1_000_000), &always(72.0), None),
    );

    assert!(
        pdfimages(&pdf).is_empty(),
        "nothing is embedded as an image"
    );
    let image = rasterise(&pdf, Engine::Poppler);
    assert_pixel(
        Engine::Poppler,
        &image,
        40.5,
        20.5,
        RED_PX,
        3,
        "the square is drawn as paths",
    );
}

// WHY: a rasteriser that was asked to render and could not has produced no picture, and quietly falling back to
// vectors would hand the user a file of a different size and kind from the one they asked for, without telling them.
// The error must repeat the cause and name the way out, which for dense content is the raster policy.
#[test]
fn a_rasteriser_that_fails_is_reported_rather_than_worked_around() {
    let mut failing = Failing;
    let text = engine();
    let result = render_display_list(
        &page_with_square(10_000),
        &text,
        &always(72.0),
        Some(&mut failing),
    );
    let Err(error) = result else {
        panic!("a failing rasteriser must not be ignored")
    };

    assert!(matches!(error, PdfError::Raster(_)), "got {error:?}");
    let message = error.to_string();
    assert!(
        message.contains("no adapter") && message.contains("`RasterPolicy::Never`"),
        "the message repeats the cause and names the way out: {message}"
    );
}

// WHY: the rasteriser is handed a display list, and everything about the result depends on that list being the dense
// geometry alone, measured in the image's own coordinates. A list still in figure coordinates would put the content
// off the edge of the image; a list carrying the rest of the page would rasterise the axes as well.
#[test]
fn the_rasteriser_is_given_only_the_dense_geometry_in_the_images_own_coordinates() {
    let mut list = page(200.0, 100.0);
    list.items
        .push(filled_rect(Rect::new(0.0, 0.0, 200.0, 100.0), BLUE));
    list.items.push(dense(
        10_000,
        vec![filled_rect(Rect::new(20.0, 10.0, 40.0, 20.0), RED)],
    ));
    let (mut stub, calls) = Stub::flat([255, 0, 0, 255]);
    render_with(&list, &always(72.0), Some(&mut stub));

    let calls = calls.borrow();
    assert_eq!(calls.len(), 1, "one dense group, one render");
    let rendered = &calls[0].0;
    assert_eq!(
        (rendered.width_pt, rendered.height_pt),
        (40.0, 20.0),
        "the list is the size of the square, not of the page"
    );
    assert_eq!(
        rendered.background.a, 0.0,
        "the background is transparent, so the page shows through everywhere the artist does not paint"
    );
    let mut corners = Vec::new();
    rendered.visit_leaves(|leaf, transform, _| {
        if let ItemKind::Path(path) = &leaf.kind {
            for segment in &path.segments {
                if let ironlab_scene::display::PathSegment::MoveTo(p) = segment {
                    corners.push(transform.apply(*p));
                }
            }
        }
    });
    assert_eq!(corners.len(), 1, "only the square is in the list");
    assert!(
        corners[0].x.abs() < 1e-9 && corners[0].y.abs() < 1e-9,
        "the square starts at the image's origin, not at {:?}",
        corners[0]
    );
}

// WHY: only the dense artist is rasterised. Axes, ticks, labels and legends must survive as vectors and as real text,
// because that is the reason a figure is a PDF at all; an exporter that rasterised the page, or drew the image over
// the furniture, would pass every geometric test above and still ruin the figure.
#[test]
fn the_furniture_around_a_raster_stays_vector_and_selectable() {
    require_tools!(IMAGE_TOOL, "pdftotext", "pdffonts");
    let ws = Workspace::new("dense-furniture");
    let text = engine();
    let mut list = page(200.0, 100.0);
    list.items.push(dense(
        10_000,
        vec![filled_rect(Rect::new(20.0, 10.0, 40.0, 20.0), RED)],
    ));
    list.items.extend(label(
        &text,
        "Reynolds number",
        false,
        10.0,
        ironlab_scene::display::Point::new(20.0, 70.0),
    ));
    list.items.push(stroked_line(
        ironlab_scene::display::Point::new(10.0, 90.0),
        ironlab_scene::display::Point::new(190.0, 90.0),
        solid_stroke(Rgba::BLACK, 1.0),
    ));
    let (mut stub, _) = Stub::flat([255, 0, 0, 255]);
    let pdf = ws.write_pdf(
        "figure",
        &render_with(&list, &always(72.0), Some(&mut stub)),
    );

    let images = pdfimages(&pdf);
    assert_eq!(images.len(), 1, "only the dense artist became an image");
    assert!(
        images[0].width < 200 && images[0].height < 100,
        "the image covers the artist, not the page: {images:?}"
    );
    assert!(
        pdftotext(&pdf).contains("Reynolds number"),
        "the label is still real text"
    );
    assert!(
        pdffonts(&pdf).iter().any(|font| font.embedded),
        "its font is still embedded"
    );
}
