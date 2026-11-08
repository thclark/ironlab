//! Image items: where an image lands beneath the transforms of its group, how its alpha composites with what lies
//! beneath it, and how it is embedded, checked with both engines.
//!
//! Pages are rasterised at 72 dpi, so one pixel is one point. Every image pixel is magnified to tens of points and
//! sampled at its centre, so that neither engine's handling of the boundaries between pixels affects the samples.
//! The scene compiler emits an image artist on linear axes in exactly this form: an image item in pixel space beneath
//! one group whose transform places it in the axes. On a logarithmic axis it emits an image drawn through a mesh of
//! its pixel edges in figure space, which the exporter writes as one filled path per pixel.

use std::sync::Arc;

use ironlab_pdf::PdfOptions;
use ironlab_scene::display::{
    DisplayList, ImageGeometry, ImageItem, ImageMesh, Item, ItemKind, Point, Rect, Transform,
};

use crate::common::*;
use crate::require_tools;

const GREEN_PX: [u8; 3] = [0, 255, 0];
const BLUE_PX: [u8; 3] = [0, 0, 255];
const YELLOW_PX: [u8; 3] = [255, 255, 0];

/// Four distinct opaque colours in row order: red and green on the top row, blue and yellow beneath.
const QUAD: [[u8; 3]; 4] = [RED_PX, GREEN_PX, BLUE_PX, YELLOW_PX];

/// An image item of `width` by `height` pixels with `channels` channels per pixel, drawn into `rect`.
fn image(rect: Rect, width: u32, height: u32, channels: u8, samples: Vec<u8>) -> Item {
    item(ItemKind::Image(ImageItem {
        geometry: ImageGeometry::Rect(rect),
        width,
        height,
        channels,
        samples: Arc::from(samples),
        depth: None,
    }))
}

/// A scaling by `sx` and `sy` followed by a translation to `(x, y)`.
fn scale_then_translate(sx: f64, sy: f64, x: f64, y: f64) -> Transform {
    Transform {
        a: sx,
        b: 0.0,
        c: 0.0,
        d: sy,
        e: x,
        f: y,
    }
}

// WHY: the scene compiler places every image with one transform-carrying group, so the exporter must draw the image
// beneath that transform in the display list's y-down space. A raster placed in PDF's native y-up space, drawn
// without the group transform, or with its rows the wrong way round under the mirroring scale puts the colours in
// the wrong pixels, and each sample below tells one of those apart from the right picture.
#[test]
fn an_image_lands_at_the_rectangle_its_group_transform_gives_it() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("image-transform");
    let text = engine();
    let mut list = page(200.0, 100.0);
    // Pixel space [0, 2] × [0, 2] scaled by 20 and mirrored in y, then moved to (100, 60): column 0 covers
    // x ∈ [100, 120] and column 1 x ∈ [120, 140], while row 0 covers y ∈ [40, 60] beneath row 1 at y ∈ [20, 40].
    list.items.push(group(
        None,
        Some(scale_then_translate(20.0, -20.0, 100.0, 60.0)),
        vec![image(
            Rect::new(0.0, 0.0, 2.0, 2.0),
            2,
            2,
            ImageItem::RGB,
            QUAD.concat(),
        )],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            110.5,
            50.5,
            RED_PX,
            3,
            "pixel (0, 0), the first sample, at the bottom left of the mirrored image",
        );
        assert_pixel(
            engine,
            &image,
            130.5,
            50.5,
            GREEN_PX,
            3,
            "pixel (0, 1) to its right",
        );
        assert_pixel(
            engine,
            &image,
            110.5,
            30.5,
            BLUE_PX,
            3,
            "pixel (1, 0) above it",
        );
        assert_pixel(engine, &image, 130.5, 30.5, YELLOW_PX, 3, "pixel (1, 1)");
        assert_pixel(engine, &image, 90.5, 50.5, WHITE_PX, 3, "left of the image");
        assert_pixel(
            engine,
            &image,
            150.5,
            50.5,
            WHITE_PX,
            3,
            "right of the image",
        );
        assert_pixel(engine, &image, 120.5, 10.5, WHITE_PX, 3, "above the image");
        assert_pixel(engine, &image, 120.5, 70.5, WHITE_PX, 3, "below the image");
        assert_pixel(
            engine,
            &image,
            1.5,
            1.5,
            WHITE_PX,
            3,
            "where the untransformed rectangle would lie",
        );
        assert_pixel(
            engine,
            &image,
            110.5,
            75.5,
            WHITE_PX,
            3,
            "where an image placed in PDF's y-up space would lie",
        );
    }
}

// WHY: an image with alpha (a NaN region left transparent, a fade at the edge of a disc) is written as an image with
// a soft mask, and the mask must be the straight alpha of the samples: a transparent pixel shows what was painted
// before it, a half-transparent one is a blend with it, and an exporter that dropped the mask, premultiplied the
// colour a second time or attached the mask to the wrong image fails one of the four samples.
#[test]
fn a_four_channel_image_composites_over_what_lies_beneath_it() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("image-alpha");
    let text = engine();
    let mut list = page(100.0, 100.0);
    list.items
        .push(filled_rect(Rect::new(0.0, 40.0, 100.0, 20.0), BLUE));
    // Two pixels, half-transparent red then fully transparent, each 40 points wide over y ∈ [20, 80].
    list.items.push(group(
        None,
        Some(scale_then_translate(40.0, 60.0, 10.0, 20.0)),
        vec![image(
            Rect::new(0.0, 0.0, 2.0, 1.0),
            2,
            1,
            ImageItem::RGBA,
            vec![255, 0, 0, 128, 255, 0, 0, 0],
        )],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            30.5,
            30.5,
            [255, 128, 128],
            4,
            "half-transparent red over the white page",
        );
        assert_pixel(
            engine,
            &image,
            30.5,
            50.5,
            [128, 0, 127],
            4,
            "half-transparent red over the blue band",
        );
        assert_pixel(
            engine,
            &image,
            70.5,
            30.5,
            WHITE_PX,
            3,
            "a transparent pixel over the page",
        );
        assert_pixel(
            engine,
            &image,
            70.5,
            50.5,
            BLUE_PX,
            3,
            "a transparent pixel over the band",
        );
    }
}

// WHY: an image artist carries the user's pixels, and the PDF must carry exactly those: one image object of the
// data's width and height placed at the data's own resolution, losslessly deflated, drawn with hard pixel edges, and
// given a soft mask only when it has alpha, because a mask on an opaque image is a quarter more file for a stream of
// 255s and marks the page as using transparency, which strict print profiles restrict.
#[test]
fn an_image_is_embedded_once_at_its_own_pixel_size_with_a_soft_mask_only_when_it_has_alpha() {
    require_tools!(IMAGE_TOOL);
    let ws = Workspace::new("image-embedding");
    let text = engine();
    // Five by three pixels, each 10 points square, so that the image is placed at 7.2 pixels per inch.
    let placed = |channels: u8, samples: Vec<u8>| {
        let mut list = page(100.0, 60.0);
        list.items.push(group(
            None,
            Some(scale_then_translate(10.0, 10.0, 10.0, 10.0)),
            vec![image(
                Rect::new(0.0, 0.0, 5.0, 3.0),
                5,
                3,
                channels,
                samples,
            )],
        ));
        list
    };

    let opaque = placed(ImageItem::RGB, vec![200; 5 * 3 * 3]);
    let pdf = ws.write_pdf("opaque", &render(&opaque, &text));
    let images = pdfimages(&pdf);
    assert_eq!(images.len(), 1, "an opaque image is one object: {images:?}");
    assert_eq!(
        (images[0].width, images[0].height),
        (5, 3),
        "the image has the data's pixel dimensions"
    );
    assert_eq!(images[0].components, 3, "red, green and blue only");
    assert_eq!(images[0].encoding, "image", "deflated, not JPEG-compressed");
    assert!(!images[0].interpolated, "the image is not smoothed");
    assert!(
        (images[0].x_ppi - 7.2).abs() <= 1.0 && (images[0].y_ppi - 7.2).abs() <= 1.0,
        "the image is placed at its own resolution of 7.2 pixels per inch, not {} by {}",
        images[0].x_ppi,
        images[0].y_ppi
    );

    let translucent = placed(ImageItem::RGBA, [200, 100, 50, 128].repeat(5 * 3));
    let pdf = ws.write_pdf("translucent", &render(&translucent, &text));
    let images = pdfimages(&pdf);
    assert_eq!(
        images.len(),
        2,
        "an image with alpha is the image and its soft mask: {images:?}"
    );
    assert!(
        images
            .iter()
            .any(|i| i.kind == "image" && (i.width, i.height) == (5, 3) && i.components == 3),
        "the colour is a three-component image of the data's dimensions: {images:?}"
    );
    assert!(
        images
            .iter()
            .any(|i| i.kind == "smask" && (i.width, i.height) == (5, 3)),
        "the soft mask has the image's dimensions: {images:?}"
    );
}

/// The figure-space x of the column edges of [`mesh_page`], unequally spaced as a logarithmic axis spaces them.
const MESH_X: [f64; 4] = [20.0, 60.0, 160.0, 280.0];
/// The figure-space y of the row edges of [`mesh_page`], row 0 at the top.
const MESH_Y: [f64; 3] = [40.0, 100.0, 180.0];

/// A 300 by 200 point page holding a 3 by 2 image with alpha drawn through a lattice with a node at every pixel edge,
/// at [`MESH_X`] and [`MESH_Y`]: red, green and blue on the top row, and yellow, a fully transparent pixel and a
/// half-transparent red beneath. The image lies inside a dense group of six cells, as the scene compiler emits it.
pub(crate) fn mesh_page() -> DisplayList {
    let points = MESH_Y
        .iter()
        .flat_map(|y| MESH_X.iter().map(move |x| Point::new(*x, *y)))
        .collect();
    let image = Item {
        source: Some(SURFACE),
        kind: ItemKind::Image(ImageItem {
            geometry: ImageGeometry::Mesh(ImageMesh {
                u: vec![0.0, 1.0, 2.0, 3.0],
                v: vec![0.0, 1.0, 2.0],
                points,
                depths: None,
            }),
            width: 3,
            height: 2,
            channels: ImageItem::RGBA,
            samples: Arc::from(vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, //
                255, 255, 0, 255, 0, 0, 0, 0, 255, 0, 0, 128,
            ]),
            depth: None,
        }),
    };
    let mut list = page(300.0, 200.0);
    list.items.push(dense(6, vec![image]));
    list
}

/// Asserts that a raster of [`mesh_page`] shows every pixel of its image between the drawn positions of its own
/// edges.
#[track_caller]
pub(crate) fn assert_mesh_drawn(engine: Engine, image: &image::RgbImage) {
    let centre = |i: usize, j: usize| {
        (
            0.5 * (MESH_X[i] + MESH_X[i + 1]),
            0.5 * (MESH_Y[j] + MESH_Y[j + 1]),
        )
    };
    /// A pixel by its column and row, its expected colour, the tolerance and what it shows.
    type Expected<'a> = ((usize, usize), [u8; 3], u8, &'a str);
    let expected: [Expected; 6] = [
        ((0, 0), RED_PX, 3, "pixel (0, 0)"),
        ((1, 0), GREEN_PX, 3, "pixel (0, 1)"),
        ((2, 0), BLUE_PX, 3, "pixel (0, 2)"),
        ((0, 1), YELLOW_PX, 3, "pixel (1, 0)"),
        (
            (1, 1),
            WHITE_PX,
            3,
            "the transparent pixel (1, 1) shows the page",
        ),
        (
            (2, 1),
            [255, 128, 128],
            4,
            "the half-transparent pixel (1, 2) blends with the page",
        ),
    ];
    for ((i, j), colour, tolerance, what) in expected {
        let (x, y) = centre(i, j);
        assert_pixel(engine, image, x + 0.5, y + 0.5, colour, tolerance, what);
    }
    // At x = 90 the lattice puts column 1, whose edges are drawn at 60 and 160; a rectangle stretched over the
    // lattice's bounds would put column 0 there, which spans 20 to 106.7 when columns are equally wide.
    assert_pixel(
        engine,
        image,
        90.5,
        70.5,
        GREEN_PX,
        3,
        "column 1 begins at its own edge, not at an equal division of the image",
    );
    for (x, y, what) in [
        (10.5, 70.5, "left of the image"),
        (290.5, 70.5, "right of the image"),
        (100.5, 20.5, "above the image"),
        (100.5, 190.5, "below the image"),
    ] {
        assert_pixel(engine, image, x, y, WHITE_PX, 3, what);
    }
}

// WHY: an image on a logarithmic axis reaches the exporter drawn through a mesh, and the PDF must show each pixel
// exactly between its own drawn edges, which a single raster under an affine transform cannot do. The exporter
// writes one filled path per pixel, in the pixel's colour with its alpha as the fill's alpha, and skips a fully
// transparent pixel; a small image, below the dense threshold, must therefore embed no image at all and ask no
// rasteriser for anything, so that the figure stays vector and every pixel edge lands where the axis says.
#[test]
fn a_small_mesh_image_is_written_as_one_filled_path_per_pixel() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1], IMAGE_TOOL);
    let ws = Workspace::new("image-mesh");
    let list = mesh_page();
    let (mut stub, calls) = Stub::flat([0, 0, 0, 255]);
    let rendered = render_reporting(&list, &PdfOptions::default(), Some(&mut stub));
    let pdf = ws.write_pdf("figure", &rendered.bytes);
    assert!(
        calls.borrow().is_empty(),
        "six pixels are far below the dense threshold, so no raster is requested"
    );
    assert!(
        pdfimages(&pdf).is_empty(),
        "the pixels are paths, not an embedded image"
    );
    assert!(
        rendered.warnings.is_empty(),
        "nothing was rasterised: {:?}",
        rendered.warnings
    );
    for engine in ENGINES {
        assert_mesh_drawn(engine, &rasterise(&pdf, engine));
    }
}
