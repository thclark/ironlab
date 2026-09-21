//! Image items: where an image lands beneath the transforms of its group, how its alpha composites with what lies
//! beneath it, and how it is embedded, checked with both engines.
//!
//! Pages are rasterised at 72 dpi, so one pixel is one point. Every image pixel is magnified to tens of points and
//! sampled at its centre, so that neither engine's handling of the boundaries between pixels affects the samples.
//! The scene compiler emits an image artist in exactly this form: an image item in pixel space beneath one group
//! whose transform places it in the axes.

use std::sync::Arc;

use ironlab_scene::display::{ImageItem, Item, ItemKind, Rect, Transform};

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
        rect,
        width,
        height,
        channels,
        samples: Arc::from(samples),
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
