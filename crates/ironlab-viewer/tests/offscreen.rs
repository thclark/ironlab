//! Headless rendering through the viewer's own wgpu pipelines.
//!
//! These tests need a wgpu adapter. When none is available they print a message and pass, unless the environment
//! variable `IRONLAB_REQUIRE_GPU` is set (as in CI, which installs a software Vulkan adapter), in which case a missing
//! adapter fails the test. Most of them render a display list through the offscreen renderer and read the pixels
//! back; the tests of the painter's caches drive a `GpuPainter` directly on a device made as the renderer makes its
//! own, and count what it uploads.
//!
//! The image tests draw hand-built image items magnified so that every image pixel spans many device pixels, and
//! sample device pixels at the centres of image pixels and one device pixel either side of their boundaries, so that
//! what is asserted is the colour of the pixels and the hardness of their edges rather than the anti-aliasing of the
//! quad that carries them.

mod common;

use std::ops::Range;
use std::sync::Arc;

use common::{
    TEXT, Workspace, assert_same_picture, depth_group, figure_with_mapped_image,
    figure_with_surface, find_image, glyph_h, gpu_or_skip, gpu_required, image_sample, rasterise,
    rendered_or_skip, rgb_of, scale_then_translate, tools_available,
};
use egui_wgpu::wgpu;
use image::RgbImage;
use ironlab_ir::{Artist, Axes, Axis, DataId, Figure, FigureSize, Limits, Line, NdArray, NodeId};
use ironlab_pdf::PdfOptions;
use ironlab_scene::display::{
    Depth, DepthPlane, DisplayList, Fill, FillRule, ImageItem, Item, ItemKind, LineCap, LineJoin,
    MarkerInstance, MarkersItem, PathItem, PathSegment, Point, Rect, Rgba, Stroke, Transform,
};
use ironlab_scene::maths::camera::FACE_DEPTH_BIAS;
use ironlab_viewer::offscreen::create_device;
use ironlab_viewer::{
    DEPTH_FORMAT, Draw, DrawKind, DrawList, GpuConfig, GpuPainter, OffscreenRenderer, RenderError,
    RenderedImage, ScreenTransform, TileKey, Uploads, Vertex, Viewport,
    render_display_list_offscreen, render_offscreen,
};

fn filled_polygon(points: &[(f64, f64)], color: Rgba) -> Item {
    let mut segments = vec![PathSegment::MoveTo(Point::new(points[0].0, points[0].1))];
    segments.extend(
        points[1..]
            .iter()
            .map(|&(x, y)| PathSegment::LineTo(Point::new(x, y))),
    );
    segments.push(PathSegment::Close);
    Item {
        source: None,
        kind: ItemKind::Path(PathItem {
            segments,
            fill: Some(Fill {
                color,
                rule: FillRule::NonZero,
            }),
            stroke: None,
            depth: None,
        }),
    }
}

fn close_to(actual: [u8; 4], expected: [u8; 4], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= tolerance)
}

// Why: gallery PNGs are generated from this renderer; the image must have the physical size implied by the figure
// size and DPI, the background must be the figure background, and content must land where the display list puts it.
// The rectangle is placed off-centre in both directions, so that a vertically or horizontally flipped readback, or a
// readback that mishandles row padding (144 pixels is not a multiple of the 256-byte row alignment), puts red where
// white is expected.
#[test]
fn a_red_rectangle_on_white_renders_at_the_requested_resolution_and_position() {
    let list = DisplayList {
        width_pt: 72.0,
        height_pt: 36.0,
        background: Rgba::WHITE,
        items: vec![filled_polygon(
            &[(9.0, 6.0), (36.0, 6.0), (36.0, 18.0), (9.0, 18.0)],
            Rgba::new(1.0, 0.0, 0.0, 1.0),
        )],
    };
    let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, 144.0)) else {
        return;
    };

    // At 144 dpi one point is two pixels, so the rectangle covers columns 18..72 and rows 12..36.
    assert_eq!((image.width, image.height), (144, 72));
    assert_eq!(image.rgba.len(), 144 * 72 * 4);
    let red = [255, 0, 0, 255];
    let white = [255, 255, 255, 255];
    for (x, y, expected, what) in [
        (45, 24, red, "the centre of the rectangle"),
        (
            20,
            14,
            red,
            "just inside the top-left corner of the rectangle",
        ),
        (
            70,
            34,
            red,
            "just inside the bottom-right corner of the rectangle",
        ),
        (1, 1, white, "the top-left corner of the image"),
        (142, 70, white, "the bottom-right corner of the image"),
        (45, 48, white, "the vertical mirror image of the rectangle"),
        (
            99,
            24,
            white,
            "the horizontal mirror image of the rectangle",
        ),
    ] {
        assert!(
            close_to(image.pixel(x, y), expected, 2),
            "{what} at ({x}, {y}): expected {expected:?}, got {:?}",
            image.pixel(x, y)
        );
    }
}

// Why: figure sizes in points are rarely whole pixels at a given DPI; the pixel size must be rounded, not truncated,
// so that PNG and PDF agree on physical size to within half a pixel.
#[test]
fn the_pixel_size_is_the_rounded_point_size_scaled_by_dpi() {
    let list = DisplayList {
        width_pt: 100.0,
        height_pt: 30.0,
        background: Rgba::WHITE,
        items: vec![],
    };
    let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, 50.0)) else {
        return;
    };

    // 100 × 50 / 72 = 69.4 and 30 × 50 / 72 = 20.8.
    assert_eq!((image.width, image.height), (69, 21));
}

// Why: the painter's pipelines draw triangles with no anti-aliasing of their own, so the renderer uses 4× MSAA;
// without it every edge in the gallery images would be a hard staircase. Without multisampling an opaque black fill
// on white produces only pure black and pure white pixels, so the presence of intermediate shades along an edge
// proves that coverage was sampled more than once per pixel. The edge has an irrational-looking slope and fractional
// end points so that it crosses pixels at every sub-pixel offset; the assertion only asks for intermediate shades in
// a quarter of the columns, which holds for any 4× sample pattern rather than counting pixels produced by one
// particular rasteriser.
#[test]
fn edges_are_anti_aliased() {
    let (y_left, y_right) = (8.3, 29.7);
    let list = DisplayList {
        width_pt: 64.0,
        height_pt: 64.0,
        background: Rgba::WHITE,
        items: vec![filled_polygon(
            &[(0.0, y_left), (64.0, y_right), (64.0, 64.0), (0.0, 64.0)],
            Rgba::BLACK,
        )],
    };
    let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, 72.0)) else {
        return;
    };

    let edge_y = |x: f64| y_left + (y_right - y_left) * x / 64.0;
    let is_intermediate = |p: [u8; 4]| p[0] > 20 && p[0] < 235;
    let mut columns_with_intermediate_shades = 0;
    for x in 0..64 {
        let edge = edge_y(f64::from(x) + 0.5);
        let mut found = false;
        for y in 0..64 {
            let distance = (f64::from(y) + 0.5 - edge).abs();
            let pixel = image.pixel(x, y);
            if distance <= 1.5 {
                found |= is_intermediate(pixel);
            } else if distance > 2.5 {
                assert!(
                    !is_intermediate(pixel),
                    "pixel ({x}, {y}) is {distance:.1} pixels from the edge but partially covered: {pixel:?}"
                );
            }
        }
        columns_with_intermediate_shades += usize::from(found);
    }
    assert!(
        columns_with_intermediate_shades >= 16,
        "expected intermediate shades along the edge in at least 16 of 64 columns, found {columns_with_intermediate_shades}"
    );
}

// Why: the viewer must be WYSIWYG with the PDF export, which draws items in order, clips groups and composites
// translucent colours over what lies beneath. A renderer that reordered draws (for example batching by colour),
// ignored clips, or blended straight rather than premultiplied alpha would show a different picture from the PDF.
#[test]
fn rendering_preserves_paint_order_group_clips_and_translucency() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    let list = DisplayList {
        width_pt: 80.0,
        height_pt: 60.0,
        background: Rgba::WHITE,
        items: vec![
            filled_polygon(
                &[(10.0, 10.0), (50.0, 10.0), (50.0, 50.0), (10.0, 50.0)],
                red,
            ),
            Item {
                source: None,
                kind: ItemKind::Group {
                    clip: Some(ironlab_scene::display::Rect::new(30.0, 0.0, 40.0, 60.0)),
                    transform: None,
                    items: vec![filled_polygon(
                        &[(20.0, 20.0), (60.0, 20.0), (60.0, 40.0), (20.0, 40.0)],
                        blue,
                    )],
                },
            },
            filled_polygon(
                &[(0.0, 52.0), (8.0, 52.0), (8.0, 60.0), (0.0, 60.0)],
                Rgba::new(1.0, 0.0, 0.0, 0.5),
            ),
        ],
    };
    let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, 72.0)) else {
        return;
    };

    let red = [255, 0, 0, 255];
    let blue = [0, 0, 255, 255];
    let white = [255, 255, 255, 255];
    for (x, y, expected, tolerance, what) in [
        (15, 30, red, 2, "red where only the red square lies"),
        (
            25,
            30,
            red,
            2,
            "red where the blue square lies outside its group clip",
        ),
        (
            40,
            30,
            blue,
            2,
            "blue where the later blue square overlaps the red one",
        ),
        (
            55,
            30,
            blue,
            2,
            "blue where only the clipped blue square lies",
        ),
        (
            65,
            30,
            white,
            2,
            "white inside the clip but beyond the blue square",
        ),
        (40, 15, red, 2, "red above the blue square"),
        // Half-transparent red over white composites to (255, 128, 128), as a PDF viewer shows it.
        (
            4,
            56,
            [255, 128, 128, 255],
            4,
            "pink where translucent red lies over white",
        ),
    ] {
        assert!(
            close_to(image.pixel(x, y), expected, tolerance),
            "{what} at ({x}, {y}): expected {expected:?}, got {:?}",
            image.pixel(x, y)
        );
    }
}

// Why: a figure with a transparent background exported as a PNG must composite correctly onto the docs page. PNG
// stores straight alpha, so a renderer that returned the GPU's premultiplied values would darken every translucent
// and anti-aliased pixel.
#[test]
fn a_transparent_background_yields_straight_alpha_pixels() {
    let list = DisplayList {
        width_pt: 40.0,
        height_pt: 20.0,
        background: Rgba::new(1.0, 1.0, 1.0, 0.0),
        items: vec![
            filled_polygon(
                &[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)],
                Rgba::new(1.0, 0.0, 0.0, 1.0),
            ),
            filled_polygon(
                &[(20.0, 0.0), (30.0, 0.0), (30.0, 20.0), (20.0, 20.0)],
                Rgba::new(1.0, 0.0, 0.0, 0.5),
            ),
        ],
    };
    let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, 72.0)) else {
        return;
    };

    assert!(
        close_to(image.pixel(10, 10), [255, 0, 0, 255], 2),
        "opaque red: {:?}",
        image.pixel(10, 10)
    );
    let translucent = image.pixel(25, 10);
    assert!(
        translucent[0] >= 250
            && translucent[1] <= 4
            && translucent[2] <= 4
            && translucent[3].abs_diff(128) <= 3,
        "translucent red keeps full-intensity colour with half alpha: {translucent:?}"
    );
    assert_eq!(image.pixel(35, 10)[3], 0, "the background is transparent");
}

// Why: the docs build renders every gallery figure; an empty page or one too large for the adapter must be reported
// as an error, because wgpu reports an invalid texture size through its uncaptured-error handler, which panics.
#[test]
fn empty_or_oversized_images_are_reported_as_invalid_sizes() {
    for (width_pt, height_pt, dpi) in [(0.0, 10.0, 72.0), (10.0, 10.0, 0.0), (1.0e6, 10.0, 72.0)] {
        let list = DisplayList {
            width_pt,
            height_pt,
            background: Rgba::WHITE,
            items: vec![],
        };
        match render_display_list_offscreen(&list, &TEXT, dpi) {
            Err(RenderError::InvalidSize { width, height, max }) => {
                assert!(
                    width == 0 || height == 0 || width > max || height > max,
                    "{width}×{height} with maximum {max} is reported as invalid"
                );
            }
            Err(RenderError::NoAdapter(_)) if !gpu_required() => {
                eprintln!("skipping: no graphics adapter");
                return;
            }
            other => panic!(
                "{width_pt}×{height_pt} pt at {dpi} dpi: expected RenderError::InvalidSize, got {other:?}"
            ),
        }
    }
}

// Why: `render_offscreen` is the entry point used by the docs gallery; it must compile the figure itself and draw the
// artists inside the axes, not merely produce a blank page of the right size.
#[test]
fn a_compiled_figure_renders_at_its_physical_size_with_ink_inside_the_axes() {
    let mut figure = Figure {
        id: NodeId(1),
        size: FigureSize {
            width_mm: 160.0,
            height_mm: 100.0,
        },
        ..Figure::new()
    };
    figure
        .data
        .insert(DataId(0), NdArray::vector(vec![0.0, 1.0, 2.0]));
    figure
        .data
        .insert(DataId(1), NdArray::vector(vec![0.0, 2.0, 1.0]));
    figure.axes.push(Axes {
        id: NodeId(2),
        x: Axis {
            limits: Limits::Auto,
            ..Axis::default()
        },
        artists: vec![Artist::Line(Line {
            id: NodeId(3),
            x: DataId(0),
            y: DataId(1),
            ..Line::default()
        })],
        ..Axes::default()
    });
    let Some(image) = rendered_or_skip(render_offscreen(&figure, &TEXT, 72.0)) else {
        return;
    };

    // 160 mm × 100 mm is 453.5 pt × 283.5 pt, which at 72 dpi rounds to 454 × 283 pixels.
    assert_eq!((image.width, image.height), (454, 283));
    let scene = ironlab_scene::compile(&figure, &TEXT);
    let plot = scene.hit_map.axes[0].plot_rect;
    let inked = (plot.y.ceil() as u32 + 2..plot.bottom().floor() as u32 - 2)
        .flat_map(|y| {
            (plot.x.ceil() as u32 + 2..plot.right().floor() as u32 - 2).map(move |x| (x, y))
        })
        .filter(|&(x, y)| image.pixel(x, y)[..3].iter().any(|&c| c < 200))
        .count();
    assert!(
        inked > 50,
        "the line is drawn inside the plot rectangle ({inked} inked pixels)"
    );
}

// Why: gallery generation and tests run on machines without a GPU; a missing adapter must surface as an error the
// caller can report or skip, never as a panic that aborts the documentation build. The check runs in a child process
// restricted, through `WGPU_BACKEND`, to a backend that is not compiled into wgpu on this platform (DirectX 12 exists
// only on Windows, Metal only on Apple platforms), so wgpu finds no adapter on any machine.
#[test]
fn a_missing_adapter_is_reported_as_an_error_not_a_panic() {
    let unavailable = if cfg!(windows) { "metal" } else { "dx12" };
    let output = std::process::Command::new(std::env::current_exe().expect("test binary path"))
        .args(["--exact", "no_adapter_probe", "--ignored", "--nocapture"])
        .env("WGPU_BACKEND", unavailable)
        .env(NO_ADAPTER_PROBE, "1")
        .output()
        .expect("spawn the probe");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "the probe failed or did not run:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Set by [`a_missing_adapter_is_reported_as_an_error_not_a_panic`] for its child process, so that the probe does
/// nothing when run directly (for example with `--include-ignored`) with a real adapter available.
const NO_ADAPTER_PROBE: &str = "IRONLAB_NO_ADAPTER_PROBE";

#[test]
#[ignore = "run in a child process with an unavailable WGPU_BACKEND by a_missing_adapter_is_reported_as_an_error_not_a_panic"]
fn no_adapter_probe() {
    if std::env::var_os(NO_ADAPTER_PROBE).is_none() {
        eprintln!(
            "skipping: only meaningful in the child process started by a_missing_adapter_is_reported_as_an_error_not_a_panic"
        );
        return;
    }
    let list = DisplayList {
        width_pt: 10.0,
        height_pt: 10.0,
        background: Rgba::WHITE,
        items: vec![],
    };
    match render_display_list_offscreen(&list, &TEXT, 72.0) {
        Err(RenderError::NoAdapter(message)) => {
            assert!(
                !message.is_empty(),
                "the error explains why no adapter was found"
            );
        }
        other => panic!("expected RenderError::NoAdapter, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------------------------------------------

const RED_PX: [u8; 4] = [255, 0, 0, 255];
const GREEN_PX: [u8; 4] = [0, 255, 0, 255];
const BLUE_PX: [u8; 4] = [0, 0, 255, 255];
const YELLOW_PX: [u8; 4] = [255, 255, 0, 255];
const WHITE_PX: [u8; 4] = [255, 255, 255, 255];

/// Four distinct opaque colours in row order: red and green on the top row, blue and yellow beneath.
const QUAD_PIXELS: [[u8; 4]; 4] = [RED_PX, GREEN_PX, BLUE_PX, YELLOW_PX];

/// `item` inside a group clipped to `clip`, as an axes clips every artist to its plot rectangle.
fn clipped(clip: Rect, item: Item) -> Item {
    Item {
        source: None,
        kind: ItemKind::Group {
            clip: Some(clip),
            transform: None,
            items: vec![item],
        },
    }
}

/// An image item of `width` by `height` pixels drawn into `rect`, beneath a group carrying `transform`, which is
/// how the scene compiler emits an image artist.
fn placed_image(
    rect: Rect,
    width: u32,
    height: u32,
    channels: u8,
    samples: Vec<u8>,
    transform: Transform,
) -> Item {
    Item {
        source: None,
        kind: ItemKind::Group {
            clip: None,
            transform: Some(transform),
            items: vec![Item {
                source: None,
                kind: ItemKind::Image(ImageItem {
                    rect,
                    width,
                    height,
                    channels,
                    samples: Arc::from(samples),
                    depth: None,
                }),
            }],
        },
    }
}

/// A page of `width_pt` by `height_pt` points with `background`, holding `items`.
fn page(width_pt: f64, height_pt: f64, background: Rgba, items: Vec<Item>) -> DisplayList {
    DisplayList {
        width_pt,
        height_pt,
        background,
        items,
    }
}

/// Asserts that the pixel immediately outside a scissor edge is untouched or, at most, half covered by `ink`.
///
/// A scissor rounds the clip to a whole pixel, and a driver decides for itself which samples of the pixel that
/// straddles the rounded edge lie across it: Metal covers none of them and lavapipe covers half. Both put the edge
/// within half a pixel of where the rounding placed it, which is all the scissor promises, so a test of where a
/// clip lands admits either and pins the pixel beyond it, which no driver may touch.
#[track_caller]
fn assert_outside_the_scissor(
    image: &RenderedImage,
    x: u32,
    y: u32,
    ink: [u8; 4],
    background: [u8; 4],
    what: &str,
) {
    let pixel = image.pixel(x, y);
    let half: [u8; 4] =
        std::array::from_fn(|i| ((u16::from(ink[i]) + u16::from(background[i])) / 2) as u8);
    assert!(
        close_to(pixel, background, 1) || close_to(pixel, half, 4),
        "{what} at ({x}, {y}) is untouched or at most half covered: expected {background:?} or \
         {half:?}, got {pixel:?}"
    );
}

/// Asserts that the pixel at `(x, y)` is within `tolerance` of `expected` in every channel.
#[track_caller]
fn assert_pixel(
    image: &RenderedImage,
    x: u32,
    y: u32,
    expected: [u8; 4],
    tolerance: u8,
    what: &str,
) {
    let actual = image.pixel(x, y);
    assert!(
        close_to(actual, expected, tolerance),
        "{what} at ({x}, {y}): expected {expected:?}, got {actual:?}"
    );
}

// Why: an image is data, and a reader measures colours off it, so every pixel must be drawn in exactly its sample
// colour and the boundary between two pixels must be a hard step. The painter must sample every tile with a nearest
// sampler and filter nothing in its shader; a linear sampler, or a shader that filters in its own code as egui's does
// when asked for predictable filtering, would smear a 2 × 2 image into a gradient. Each image pixel is magnified to
// 20 device pixels, so a bilinear blend would be visible over most of the pixel, and the samples one device pixel
// either side of a boundary would differ from the pure colours by half their contrast.
#[test]
fn a_two_by_two_image_renders_its_pixel_colours_exactly_with_hard_edges() {
    let list = page(
        40.0,
        40.0,
        Rgba::WHITE,
        vec![placed_image(
            Rect::new(0.0, 0.0, 2.0, 2.0),
            2,
            2,
            ImageItem::RGBA,
            QUAD_PIXELS.concat(),
            scale_then_translate(20.0, 20.0, 0.0, 0.0),
        )],
    );
    let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, 72.0)) else {
        return;
    };

    assert_eq!((image.width, image.height), (40, 40));
    let [red, green, blue, yellow] = QUAD_PIXELS;
    for (x, y, expected, what) in [
        (10, 10, red, "the centre of the top-left pixel"),
        (30, 10, green, "the centre of the top-right pixel"),
        (10, 30, blue, "the centre of the bottom-left pixel"),
        (30, 30, yellow, "the centre of the bottom-right pixel"),
        (
            19,
            10,
            red,
            "one device pixel left of the vertical boundary",
        ),
        (
            20,
            10,
            green,
            "one device pixel right of the vertical boundary",
        ),
        (
            10,
            19,
            red,
            "one device pixel above the horizontal boundary",
        ),
        (
            10,
            20,
            blue,
            "one device pixel below the horizontal boundary",
        ),
        (
            19,
            19,
            red,
            "the top-left pixel at the corner shared by all four",
        ),
        (20, 20, yellow, "the bottom-right pixel at that corner"),
        (0, 0, red, "the first device pixel of the image"),
        (39, 39, yellow, "the last device pixel of the image"),
    ] {
        assert_pixel(&image, x, y, expected, 1, what);
    }
}

// Why: an image with alpha (a NaN region left transparent, a fade at the edge of a disc) is composited over whatever
// lies beneath it, exactly as the PDF composites its soft mask: a transparent pixel shows the background untouched
// and a half-transparent one is a straight-alpha blend with it. A renderer that uploaded straight alpha where the
// blend state expects premultiplied would draw the translucent pixel too bright, and one that ignored alpha would
// paint the transparent pixel opaque.
#[test]
fn transparent_and_translucent_image_pixels_composite_over_the_background() {
    let list = page(
        40.0,
        20.0,
        Rgba::new(0.0, 0.0, 1.0, 1.0),
        vec![placed_image(
            Rect::new(0.0, 0.0, 2.0, 1.0),
            2,
            1,
            ImageItem::RGBA,
            vec![255, 0, 0, 0, 255, 0, 0, 128],
            scale_then_translate(20.0, 20.0, 0.0, 0.0),
        )],
    );
    let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, 72.0)) else {
        return;
    };

    assert_pixel(
        &image,
        10,
        10,
        BLUE_PX,
        1,
        "a transparent pixel shows the blue background",
    );
    // Half red over blue, as a PDF viewer composites it: (128, 0, 127).
    assert_pixel(
        &image,
        30,
        10,
        [128, 0, 127, 255],
        2,
        "a half-transparent red pixel blends with the blue background",
    );
}

// Why: a pixel range running backwards, and a wall of a three-dimensional axes seen from behind, place an image with
// a transform of negative determinant; the quad's triangles then wind the other way, and a pipeline that culled back
// faces would drop the image entirely. The columns (or rows) must come out mirrored, not merely present.
#[test]
fn a_negative_scale_mirrors_the_image() {
    let list = page(
        100.0,
        40.0,
        Rgba::WHITE,
        vec![
            // Mirrored in x: pixel space u ∈ [0, 2] maps to x = 40 − 20u, so column 0 lands on the right.
            placed_image(
                Rect::new(0.0, 0.0, 2.0, 2.0),
                2,
                2,
                ImageItem::RGBA,
                QUAD_PIXELS.concat(),
                scale_then_translate(-20.0, 20.0, 40.0, 0.0),
            ),
            // Mirrored in y: v ∈ [0, 2] maps to y = 40 − 20v, so row 0 lands at the bottom, 60 points to the right.
            placed_image(
                Rect::new(0.0, 0.0, 2.0, 2.0),
                2,
                2,
                ImageItem::RGBA,
                QUAD_PIXELS.concat(),
                scale_then_translate(20.0, -20.0, 60.0, 40.0),
            ),
        ],
    );
    let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, 72.0)) else {
        return;
    };

    let [red, green, blue, yellow] = QUAD_PIXELS;
    for (x, y, expected, what) in [
        (10, 10, green, "column 1 at the left when mirrored in x"),
        (30, 10, red, "column 0 at the right when mirrored in x"),
        (10, 30, yellow, "column 1 of row 1 at the left"),
        (30, 30, blue, "column 0 of row 1 at the right"),
        (70, 10, blue, "row 1 at the top when mirrored in y"),
        (90, 10, yellow, "column 1 of row 1 at the top"),
        (70, 30, red, "row 0 at the bottom when mirrored in y"),
        (90, 30, green, "column 1 of row 0 at the bottom"),
    ] {
        assert_pixel(&image, x, y, expected, 1, what);
    }
}

// Why: a GPU texture has a largest side, and a data image can exceed it, so the renderer cuts the raster into tiles,
// each a texture and a quad of its own, and where the tiles meet must be invisible. Whatever tiling the renderer
// applies to 8193 columns, the image drawn whole must show both colours split where the data splits them, and with
// the seam magnified the tiles must meet with no gap and no background between them and the last column must be
// present, because a tile a pixel short leaves a hairline of background through the data on screen and in every
// exported PNG. The image sits in a clipped group, as it does beneath an axes, so that the tiles lying wholly off
// the page are left undrawn rather than reaching the GPU. The side at which the renderer tiles is not pinned here.
#[test]
fn an_image_wider_than_one_texture_renders_whole_and_without_a_gap_at_the_tile_seam() {
    const WIDTH: u32 = 8193;
    const SPLIT: u32 = WIDTH / 2;
    let samples: Vec<u8> = (0..WIDTH)
        .flat_map(|i| {
            if i < SPLIT {
                [255u8, 0, 0]
            } else {
                [0, 0, 255]
            }
        })
        .collect();
    let item = |transform| {
        placed_image(
            Rect::new(0.0, 0.0, f64::from(WIDTH), 1.0),
            WIDTH,
            1,
            ImageItem::RGB,
            samples.clone(),
            transform,
        )
    };

    // The whole image squeezed into 400 points: 20.48 columns per device pixel at 72 dpi, so the split at column
    // 4096 falls at x ≈ 200.
    let whole = page(
        400.0,
        20.0,
        Rgba::WHITE,
        vec![clipped(
            Rect::new(0.0, 0.0, 400.0, 20.0),
            item(scale_then_translate(
                400.0 / f64::from(WIDTH),
                20.0,
                0.0,
                0.0,
            )),
        )],
    );
    let Some(image) = rendered_or_skip(render_display_list_offscreen(&whole, &TEXT, 72.0)) else {
        return;
    };
    for (x, expected, what) in [
        (100, RED_PX, "the left half"),
        (198, RED_PX, "just left of the split"),
        (201, BLUE_PX, "just right of the split"),
        (300, BLUE_PX, "the right half"),
        (399, BLUE_PX, "the last device pixel"),
    ] {
        assert_pixel(&image, x, 10, expected, 1, what);
    }

    // The seam magnified: every column is 10 points wide and the last column, a tile of its own when the renderer
    // tiles at 8192, lies at x ∈ [150, 160]; the raster ends there and the page beyond it is background.
    let seam = page(
        200.0,
        20.0,
        Rgba::WHITE,
        vec![clipped(
            Rect::new(0.0, 0.0, 200.0, 20.0),
            item(scale_then_translate(
                10.0,
                20.0,
                150.0 - 10.0 * f64::from(WIDTH - 1),
                0.0,
            )),
        )],
    );
    let Some(image) = rendered_or_skip(render_display_list_offscreen(&seam, &TEXT, 72.0)) else {
        return;
    };
    for x in 0..160 {
        assert_pixel(
            &image,
            x,
            10,
            BLUE_PX,
            1,
            "no gap or background at the tile seam: every device pixel up to the end of the last column",
        );
    }
    assert_pixel(
        &image,
        165,
        10,
        WHITE_PX,
        1,
        "the page beyond the last column is background",
    );
}

// Why: `render_offscreen` is what the gallery and the PDF exporter's raster path see, so an image artist must reach
// the pixels through the compiled display list: the compiler resolves a mapped image into samples beneath a placing
// transform, and the renderer must draw those samples where the axes put them, with row 0 at the bottom of the plot
// because y increases upwards in data space. Every pixel centre of a 3 × 3 image that fills its axes is sampled
// against the colour the compiler resolved for it, so an image drawn transposed, flipped or shifted against its own
// axes fails on the pixels it moves.
#[test]
fn a_compiled_figure_draws_a_mapped_image_over_its_axes_with_the_compilers_colours() {
    let figure = figure_with_mapped_image(3, 3);
    let Some(image) = rendered_or_skip(render_offscreen(&figure, &TEXT, 72.0)) else {
        return;
    };

    let scene = ironlab_scene::compile(&figure, &TEXT);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let item = find_image(&scene.display_list.items)
        .expect("the compiler emits an image item for the mapped image");
    assert_eq!((item.width, item.height), (3, 3));
    // At 72 dpi one point is one pixel, and by the default placement the image covers the plot rectangle exactly.
    let plot = scene.hit_map.axes[0].plot_rect;
    for row in 0..3 {
        for column in 0..3 {
            let x = plot.x + (f64::from(column) + 0.5) / 3.0 * plot.width;
            let y = plot.bottom() - (f64::from(row) + 0.5) / 3.0 * plot.height;
            assert_pixel(
                &image,
                x as u32,
                y as u32,
                image_sample(item, row, column),
                1,
                &format!("pixel ({row}, {column}) of the image, at ({x:.1}, {y:.1}) pt"),
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Depth groups, drawn through the depth-tested pipelines. The pages are 100 by 100 points rendered at 72 dpi, so
// that one point is one pixel and the sampled pixels lie well inside the shapes, away from anti-aliased edges.
// ---------------------------------------------------------------------------------------------------------------------

/// A filled square from `(x0, y0)` to `(x1, y1)` lying at `plane`, as the compiler emits the fill of a face.
fn face(x0: f64, y0: f64, x1: f64, y1: f64, color: Rgba, plane: DepthPlane) -> Item {
    let mut item = filled_polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], color);
    if let ItemKind::Path(path) = &mut item.kind {
        path.depth = Some(Depth::Plane(plane));
    }
    item
}

/// The outline of the square from `(x0, y0)` to `(x1, y1)`, stroked `width` wide in `color` and lying at `plane`,
/// as the compiler emits the edge of a face.
fn edge(x0: f64, y0: f64, x1: f64, y1: f64, width: f64, color: Rgba, plane: DepthPlane) -> Item {
    Item {
        source: None,
        kind: ItemKind::Path(PathItem {
            segments: vec![
                PathSegment::MoveTo(Point::new(x0, y0)),
                PathSegment::LineTo(Point::new(x1, y0)),
                PathSegment::LineTo(Point::new(x1, y1)),
                PathSegment::LineTo(Point::new(x0, y1)),
                PathSegment::Close,
            ],
            fill: None,
            stroke: Some(Stroke {
                color,
                width,
                dash: Vec::new(),
                dash_offset: 0.0,
                cap: LineCap::Butt,
                join: LineJoin::Miter,
            }),
            depth: Some(Depth::Plane(plane)),
        }),
    }
}

/// A two-by-two opaque image of one colour drawn into `rect` and lying at `plane` over its pixel space, as the
/// compiler emits an image inside the box of a 3D axes.
fn floor(rect: Rect, rgb: [u8; 3], plane: DepthPlane) -> Item {
    Item {
        source: None,
        kind: ItemKind::Image(ImageItem {
            rect,
            width: 2,
            height: 2,
            channels: ImageItem::RGB,
            samples: Arc::from(rgb.repeat(4)),
            depth: Some(plane),
        }),
    }
}

/// A two-by-two image of one straight-alpha colour `rgba` drawn into `rect` and lying at `plane` over its pixel
/// space, as the compiler emits an image with transparent or translucent pixels inside the box of a 3D axes.
fn translucent_floor(rect: Rect, rgba: [u8; 4], plane: DepthPlane) -> Item {
    Item {
        source: None,
        kind: ItemKind::Image(ImageItem {
            rect,
            width: 2,
            height: 2,
            channels: ImageItem::RGBA,
            samples: Arc::from(rgba.repeat(4)),
            depth: Some(plane),
        }),
    }
}

/// `items` with the depth removed from every path and image and zeroed on every marker instance, at any depth of
/// grouping, so that a render with the depths in place can be compared against one without them.
fn without_depths(items: Vec<Item>) -> Vec<Item> {
    items
        .into_iter()
        .map(|mut item| {
            match &mut item.kind {
                ItemKind::Path(path) => path.depth = None,
                ItemKind::Image(image) => image.depth = None,
                ItemKind::Markers(markers) => {
                    for instance in &mut markers.instances {
                        instance.depth = 0.0;
                    }
                }
                ItemKind::Group { items, .. }
                | ItemKind::Dense { items, .. }
                | ItemKind::Depth { items } => {
                    *items = without_depths(std::mem::take(items));
                }
                ItemKind::Glyphs(_) => {}
            }
            item
        })
        .collect()
}

/// A group without clip or transform, whose items a backend draws in the painter's order.
fn plain_group(items: Vec<Item>) -> Item {
    Item {
        source: None,
        kind: ItemKind::Group {
            clip: None,
            transform: None,
            items,
        },
    }
}

/// Renders `items` on a 100 by 100 point page with `background` at 72 dpi, so that one point is one pixel.
fn render_page(background: Rgba, items: Vec<Item>) -> Option<RenderedImage> {
    rendered_or_skip(render_display_list_offscreen(
        &page(100.0, 100.0, background, items),
        &TEXT,
        72.0,
    ))
}

// Why: two faces can cross in projection, and the painter's order can put only one of them in front; with a depth
// buffer each must show where it is the nearer. The same items in a plain group must still give the painter's
// picture, which is what the exporter draws, so the depth test must be switched by the depth group and not be on
// for every item that carries a depth.
/// Two opaque faces that both cover (10, 10) to (90, 90) and cross at x = 50: the red face is nearer on the right
/// and the blue one, listed second, on the left.
fn crossing_faces() -> Vec<Item> {
    vec![
        face(
            10.0,
            10.0,
            90.0,
            90.0,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            DepthPlane {
                a: 0.01,
                b: 0.0,
                c: 0.0,
            },
        ),
        face(
            10.0,
            10.0,
            90.0,
            90.0,
            Rgba::new(0.0, 0.0, 1.0, 1.0),
            DepthPlane {
                a: -0.01,
                b: 0.0,
                c: 1.0,
            },
        ),
    ]
}

#[test]
fn crossing_faces_in_a_depth_group_show_the_nearer_one_and_in_a_plain_group_the_later_one() {
    let Some(tested) = render_page(Rgba::WHITE, vec![depth_group(crossing_faces())]) else {
        return;
    };
    let Some(painted) = render_page(Rgba::WHITE, vec![plain_group(crossing_faces())]) else {
        return;
    };

    let red = [255, 0, 0, 255];
    let blue = [0, 0, 255, 255];
    assert_pixel(
        &tested,
        25,
        50,
        blue,
        2,
        "in the depth group, the blue face where it is nearer",
    );
    assert_pixel(
        &tested,
        75,
        50,
        red,
        2,
        "in the depth group, the red face where it is nearer",
    );
    assert_pixel(
        &tested,
        5,
        5,
        [255, 255, 255, 255],
        2,
        "the background beside the faces",
    );
    assert_pixel(
        &painted,
        25,
        50,
        blue,
        2,
        "in a plain group, the later blue face",
    );
    assert_pixel(
        &painted,
        75,
        50,
        blue,
        2,
        "in a plain group, the later blue face even where the red one is nearer",
    );
}

// Why: a marker lying on a face coincides with it in depth, and a depth test cannot separate what coincides; the
// compiler pushes the fill of a face back by `FACE_DEPTH_BIAS` so that the marker wins whichever is drawn first.
// The renderer must keep that distance through its normalisation and its depth format: the far pin in the corner
// gives the group the depth range of a real box, so that the bias is a thousandth of the range as it is in one
// rather than the whole of it, and a normalisation or a depth format too coarse for a thousandth would let the face
// break through the marker, which would then flicker or vanish.
#[test]
fn a_marker_on_a_face_is_visible_whichever_is_drawn_first_because_the_face_is_pushed_back() {
    let far_pin = || {
        face(
            2.0,
            2.0,
            6.0,
            6.0,
            Rgba::new(0.0, 1.0, 0.0, 1.0),
            DepthPlane::constant(-1.0),
        )
    };
    let surface = || {
        face(
            10.0,
            10.0,
            90.0,
            90.0,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            DepthPlane::constant(0.0).pushed_back(FACE_DEPTH_BIAS),
        )
    };
    let marker = || {
        face(
            45.0,
            45.0,
            55.0,
            55.0,
            Rgba::new(0.0, 0.0, 1.0, 1.0),
            DepthPlane::constant(0.0),
        )
    };
    for (order, items) in [
        (
            "the marker after the face",
            vec![far_pin(), surface(), marker()],
        ),
        (
            "the marker before the face",
            vec![far_pin(), marker(), surface()],
        ),
    ] {
        let Some(image) = render_page(Rgba::WHITE, vec![depth_group(items)]) else {
            return;
        };
        assert_pixel(
            &image,
            50,
            50,
            [0, 0, 255, 255],
            2,
            &format!("the marker's centre with {order}"),
        );
        assert_pixel(
            &image,
            25,
            50,
            [255, 0, 0, 255],
            2,
            &format!("the face beside the marker with {order}"),
        );
        assert_pixel(
            &image,
            4,
            4,
            GREEN_PX,
            2,
            &format!("the far pin in the corner, which fixes the depth range, with {order}"),
        );
    }
}

// Why: a face's edge shares the face's plane and the fill is pushed back by `FACE_DEPTH_BIAS`; on a steep plane the
// depth changes across the stroke's width by far more than the bias, so the edge stays in front only when every
// stroke vertex takes the plane at its own position. A depth read at the path's points, one depth per item, or a
// depth format too coarse for the bias would let the fill break through the edge, and the order must not matter.
#[test]
fn the_edge_of_a_steep_face_is_drawn_over_its_fill_whichever_is_drawn_first() {
    let plane = DepthPlane {
        a: 0.05,
        b: 0.0,
        c: 0.0,
    };
    let fill = || {
        face(
            20.0,
            20.0,
            80.0,
            80.0,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            plane.pushed_back(FACE_DEPTH_BIAS),
        )
    };
    let outline = || edge(20.0, 20.0, 80.0, 80.0, 2.0, Rgba::BLACK, plane);
    for (order, items) in [
        ("the edge after the fill", vec![fill(), outline()]),
        ("the edge before the fill", vec![outline(), fill()]),
    ] {
        let Some(image) = render_page(Rgba::WHITE, vec![depth_group(items)]) else {
            return;
        };
        // Eight pixels along each edge, away from the corners; the stroke covers the sampled pixels whole.
        for k in 0..8 {
            let along = 24 + 7 * k;
            assert_pixel(
                &image,
                along,
                20,
                [0, 0, 0, 255],
                2,
                &format!("the top edge with {order}"),
            );
            assert_pixel(
                &image,
                20,
                along,
                [0, 0, 0, 255],
                2,
                &format!("the left edge with {order}"),
            );
        }
        assert_pixel(
            &image,
            50,
            50,
            [255, 0, 0, 255],
            2,
            &format!("the fill inside the edge with {order}"),
        );
    }
}

// Why: an image inside the box of a 3D axes is a floor that faces stand on; it must be drawn through the depth
// pipeline with its own texture and be hidden exactly where a nearer face covers it, even though it is listed after
// the face, or floors would either vanish or paint over everything standing on them.
#[test]
fn a_floor_image_is_hidden_beneath_a_nearer_face_and_shows_beside_it() {
    let items = vec![
        face(
            10.0,
            10.0,
            50.0,
            90.0,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            DepthPlane::constant(0.5),
        ),
        floor(
            Rect::new(10.0, 10.0, 80.0, 80.0),
            [0, 255, 0],
            DepthPlane::constant(0.0),
        ),
    ];
    let Some(image) = render_page(Rgba::WHITE, vec![depth_group(items)]) else {
        return;
    };

    assert_pixel(
        &image,
        30,
        50,
        [255, 0, 0, 255],
        2,
        "the face over the left half of the floor",
    );
    assert_pixel(
        &image,
        70,
        50,
        [0, 255, 0, 255],
        2,
        "the floor beside the face",
    );
    assert_pixel(
        &image,
        5,
        50,
        [255, 255, 255, 255],
        2,
        "the background beside the floor",
    );
}

// Why: every depth group starts with a cleared depth buffer, so a face of the second group is drawn over the first
// group's content however near that was. The second group holds a near marker, so that its blue face normalises to
// the far end of the range: a buffer left uncleared would then reject the blue face under any comparison, and the
// red one would show through.
#[test]
fn the_depth_buffer_is_cleared_between_consecutive_depth_groups() {
    let items = vec![
        depth_group(vec![face(
            10.0,
            10.0,
            90.0,
            90.0,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            DepthPlane::constant(1.0),
        )]),
        depth_group(vec![
            face(
                10.0,
                10.0,
                90.0,
                90.0,
                Rgba::new(0.0, 0.0, 1.0, 1.0),
                DepthPlane::constant(0.0),
            ),
            face(
                10.0,
                10.0,
                20.0,
                20.0,
                Rgba::new(0.0, 1.0, 0.0, 1.0),
                DepthPlane::constant(1.0),
            ),
        ]),
    ];
    let Some(image) = render_page(Rgba::WHITE, items) else {
        return;
    };

    assert_pixel(
        &image,
        50,
        50,
        [0, 0, 255, 255],
        2,
        "the second group's face over the first group's",
    );
    assert_pixel(
        &image,
        15,
        15,
        [0, 255, 0, 255],
        2,
        "the second group's near marker",
    );
}

// Why: a translucent face nearer than an opaque one must blend over it as egui blends, with premultiplied alpha in
// gamma space, so that a face in a depth group composites as a translucent fill outside one does; a pipeline
// without blending, or one blending straight alpha, would paint the near face opaque or too bright.
#[test]
fn a_translucent_nearer_face_blends_over_the_opaque_face_behind_it() {
    let items = vec![
        face(
            10.0,
            10.0,
            90.0,
            90.0,
            Rgba::new(0.0, 0.0, 1.0, 1.0),
            DepthPlane::constant(0.0),
        ),
        face(
            10.0,
            10.0,
            90.0,
            90.0,
            Rgba::new(1.0, 0.0, 0.0, 0.5),
            DepthPlane::constant(1.0),
        ),
    ];
    let Some(image) = render_page(Rgba::WHITE, vec![depth_group(items)]) else {
        return;
    };

    // Half-transparent red over blue composites to (128, 0, 128), as half-transparent red over white composites to
    // (255, 128, 128) outside a depth group.
    assert_pixel(
        &image,
        50,
        50,
        [128, 0, 128, 255],
        4,
        "translucent red over opaque blue",
    );
}

// Why: the painter keys a list's buffers by the address of its `Arc`, and the offscreen renderer clears the painter
// after every render; a list allocated at the address the previous list was freed from would otherwise be drawn
// with the previous list's buffers, so a render of a different list after the first must show its own content.
// The allocator is not bound to hand out the same address again, so the test catches a stale cache when it does,
// which two lists of the same shape make likely.
#[test]
fn a_different_list_rendered_after_the_first_shows_its_own_content() {
    let square = |color| {
        vec![depth_group(vec![face(
            10.0,
            10.0,
            90.0,
            90.0,
            color,
            DepthPlane::constant(0.0),
        )])]
    };
    let Some(first) = render_page(Rgba::WHITE, square(Rgba::new(1.0, 0.0, 0.0, 1.0))) else {
        return;
    };
    let Some(second) = render_page(Rgba::WHITE, square(Rgba::new(0.0, 0.0, 1.0, 1.0))) else {
        return;
    };

    assert_pixel(
        &first,
        50,
        50,
        RED_PX,
        2,
        "the first render shows its red face",
    );
    assert_pixel(
        &second,
        50,
        50,
        BLUE_PX,
        2,
        "the second render shows its own blue face, not the first list's red one",
    );
}

// Why: a figure with a transparent background is exported as a PNG for the docs; the depth pipeline draws into the
// same cleared target and its output is converted to straight alpha, so a pixel beside a depth group must stay
// fully transparent and one inside it fully opaque.
#[test]
fn a_transparent_background_stays_transparent_beside_a_depth_group() {
    let Some(image) = render_page(
        Rgba::new(1.0, 1.0, 1.0, 0.0),
        vec![depth_group(vec![face(
            10.0,
            10.0,
            50.0,
            50.0,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            DepthPlane::constant(0.0),
        )])],
    ) else {
        return;
    };

    assert_eq!(
        image.pixel(75, 75)[3],
        0,
        "the background beside the face is transparent: {:?}",
        image.pixel(75, 75)
    );
    assert_pixel(&image, 30, 30, [255, 0, 0, 255], 2, "the opaque face");
}

// Why: an axes clips its artists to its plot rectangle, and the clip reaches the depth pipelines as a scissor
// rectangle rather than as clipped geometry; a depth group inside a clipped group, and a depth-carrying leaf inside
// one, must both be cut at the clip's edges, or a surface panned half out of its box would paint over the
// neighbouring axes. The same face is drawn both ways, so that the untested path is checked as the tested one is.
#[test]
fn a_clip_around_a_depth_group_or_a_depth_carrying_leaf_cuts_the_face_at_its_edges() {
    let clip = Rect::new(20.0, 20.0, 40.0, 40.0);
    let wide_face = || {
        face(
            10.0,
            10.0,
            90.0,
            90.0,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            DepthPlane::constant(0.0),
        )
    };
    for (how, item) in [
        (
            "a depth group inside the clipped group",
            clipped(clip, depth_group(vec![wide_face()])),
        ),
        (
            "a depth-carrying leaf inside the clipped group",
            clipped(clip, wide_face()),
        ),
    ] {
        let Some(image) = render_page(Rgba::WHITE, vec![item]) else {
            return;
        };
        assert_pixel(
            &image,
            15,
            50,
            WHITE_PX,
            2,
            &format!("the background left of the clip with {how}"),
        );
        assert_pixel(
            &image,
            30,
            50,
            RED_PX,
            2,
            &format!("the face inside the clip with {how}"),
        );
        assert_pixel(
            &image,
            59,
            50,
            RED_PX,
            2,
            &format!("the face at the last column inside the clip with {how}"),
        );
        assert_pixel(
            &image,
            60,
            50,
            WHITE_PX,
            2,
            &format!("the background at the first column beyond the clip with {how}"),
        );
    }
}

/// Three loose leaves that carry depths outside any depth group: an opaque square, a translucent square over it and
/// an image beside them, with fractional edges so that the anti-aliasing is part of the picture.
fn loose_items_with_depths() -> Vec<Item> {
    vec![
        face(
            10.5,
            10.5,
            50.5,
            50.5,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            DepthPlane::constant(0.0),
        ),
        face(
            30.5,
            30.5,
            70.5,
            70.5,
            Rgba::new(0.0, 0.0, 1.0, 0.5),
            DepthPlane::constant(1.0),
        ),
        Item {
            source: None,
            kind: ItemKind::Image(ImageItem {
                rect: Rect::new(55.0, 55.0, 30.0, 30.0),
                width: 2,
                height: 2,
                channels: ImageItem::RGB,
                samples: Arc::from(vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0]),
                depth: Some(DepthPlane::constant(2.0)),
            }),
        },
    ]
}

/// The first eight pixels at which two images of one size differ, as `(x, y, first, second)`.
fn first_differences(
    first: &RenderedImage,
    second: &RenderedImage,
) -> Vec<(u32, u32, [u8; 4], [u8; 4])> {
    (0..first.height)
        .flat_map(|y| (0..first.width).map(move |x| (x, y)))
        .filter(|&(x, y)| first.pixel(x, y) != second.pixel(x, y))
        .map(|(x, y)| (x, y, first.pixel(x, y), second.pixel(x, y)))
        .take(8)
        .collect()
}

// Why: the docs gallery is regenerated on every build and compared against what is checked in, so a pixel that
// differs from one render to the next would fail the build or hide a real change. Nothing in a render is left to
// chance: the colour and depth attachments are fresh textures cleared for every render, the draws go in list order,
// and the painter is cleared after each render so that no buffer or texture survives into the next. A depth buffer
// left uncleared, or a list drawn from another render's buffers, would show in the crossing faces of a depth group,
// where the nearer face wins only against a cleared buffer; the loose leaves drawn over them add anti-aliased edges,
// translucency and a texture to the comparison.
#[test]
fn two_renders_of_one_list_are_byte_identical() {
    let items = || {
        let mut items = vec![depth_group(crossing_faces())];
        items.extend(loose_items_with_depths());
        items
    };
    let Some(first) = render_page(Rgba::WHITE, items()) else {
        return;
    };
    let Some(second) = render_page(Rgba::WHITE, items()) else {
        return;
    };

    assert_eq!(
        (first.width, first.height),
        (second.width, second.height),
        "the two renders have one size"
    );
    assert_pixel(
        &first,
        75,
        25,
        RED_PX,
        2,
        "the red face where it is nearer, beside the loose leaves",
    );
    assert_pixel(
        &first,
        25,
        75,
        BLUE_PX,
        2,
        "the blue face where it is nearer, beneath the loose leaves",
    );
    let differing = first_differences(&first, &second);
    assert!(
        first.rgba == second.rgba,
        "two renders of one list draw identical bytes; the first pixels that differ, as (x, y, first, second): \
         {differing:?}"
    );
}

// Why: a depth means something only within a depth group, where it is normalised over the group and tested; a leaf
// that carries one outside any group is drawn without the depth test and with its depth discarded, so its picture
// must not depend on a field that means nothing there, or two lists that differ only in it would render
// differently. With the depths and without them, the same bytes; and the picture is the painter's, with the later
// translucent square over the earlier opaque one and the image on top of both.
#[test]
fn a_depth_outside_a_depth_group_is_ignored() {
    let items = loose_items_with_depths();
    let Some(with_depths) = render_page(Rgba::WHITE, items.clone()) else {
        return;
    };
    let Some(without) = render_page(Rgba::WHITE, without_depths(items)) else {
        return;
    };

    assert_pixel(&with_depths, 20, 20, RED_PX, 2, "the opaque square");
    // Half-transparent blue over red composites to about (128, 0, 128).
    assert_pixel(
        &with_depths,
        40,
        40,
        [128, 0, 128, 255],
        2,
        "the translucent square over the opaque one",
    );
    assert_pixel(
        &with_depths,
        60,
        60,
        RED_PX,
        1,
        "the image's top-left pixel",
    );
    assert_pixel(
        &with_depths,
        80,
        80,
        YELLOW_PX,
        1,
        "the image's bottom-right pixel",
    );
    let differing = first_differences(&with_depths, &without);
    assert!(
        with_depths.rgba == without.rgba,
        "depths outside a group change nothing; the first pixels that differ, as (x, y, with, without): \
         {differing:?}"
    );
}

// Why: an image with alpha (a NaN region left transparent, a fade at the edge of a disc) inside a 3D axes goes
// through the painter's texture upload, which must premultiply the straight alpha of a four-channel tile as the
// blend state expects; a tile uploaded straight would blend too bright, and one that ignored the fourth channel
// would paint the transparent pixels opaque over the face beneath.
#[test]
fn a_four_channel_tile_in_a_depth_group_composites_its_alpha_over_the_face_beneath() {
    let blue_face = || {
        face(
            10.0,
            10.0,
            90.0,
            90.0,
            Rgba::new(0.0, 0.0, 1.0, 1.0),
            DepthPlane::constant(0.0),
        )
    };
    let tile = |alpha| {
        translucent_floor(
            Rect::new(30.0, 30.0, 40.0, 40.0),
            [255, 0, 0, alpha],
            DepthPlane::constant(1.0),
        )
    };
    let Some(translucent) =
        render_page(Rgba::WHITE, vec![depth_group(vec![blue_face(), tile(128)])])
    else {
        return;
    };
    let Some(transparent) = render_page(Rgba::WHITE, vec![depth_group(vec![blue_face(), tile(0)])])
    else {
        return;
    };

    // Half-transparent red over blue composites to about (128, 0, 128), as a translucent fill composites.
    assert_pixel(
        &translucent,
        50,
        50,
        [128, 0, 128, 255],
        2,
        "a half-transparent red tile over the opaque blue face",
    );
    assert_pixel(&translucent, 20, 50, BLUE_PX, 2, "the face beside the tile");
    assert_pixel(
        &transparent,
        50,
        50,
        BLUE_PX,
        2,
        "a transparent tile leaves the blue face as it was",
    );
}

// Why: two faces can lie on one plane (the faces of two surfaces that meet, or a marker the compiler places on a
// face) with interpolated depths equal to the bit; the depth test must pass equal depths, so that the later of two
// coplanar faces wins as it does in the painter's order, rather than the earlier one, and rather than both being
// rejected where they coincide.
#[test]
fn of_two_coplanar_faces_the_later_one_wins() {
    let plane = DepthPlane {
        a: 0.01,
        b: 0.005,
        c: 0.0,
    };
    let red = || face(10.0, 10.0, 90.0, 90.0, Rgba::new(1.0, 0.0, 0.0, 1.0), plane);
    let blue = || face(10.0, 10.0, 90.0, 90.0, Rgba::new(0.0, 0.0, 1.0, 1.0), plane);
    for (order, items, expected, what) in [
        (
            "red then blue",
            vec![red(), blue()],
            BLUE_PX,
            "the later blue face",
        ),
        (
            "blue then red",
            vec![blue(), red()],
            RED_PX,
            "the later red face",
        ),
    ] {
        let Some(image) = render_page(Rgba::WHITE, vec![depth_group(items)]) else {
            return;
        };
        for (x, y) in [(50, 50), (15, 15), (85, 85)] {
            assert_pixel(&image, x, y, expected, 2, &format!("{what} with {order}"));
        }
    }
}

// Why: `render_offscreen` is what the gallery and the exporter see, and the compiler places a surface in a depth
// group, so a compiled surface must leave ink in the rendered pixels. Hiding the surface leaves the axes without a
// depth group, so the pixels that differ between the two renders are the surface's, and some of them must lie
// inside its axes. Which pipeline drew that ink the pixels cannot tell; that the surface reaches the depth-tested
// pipelines is checked on the draw list in `canvas.rs`.
#[test]
fn a_compiled_surface_leaves_ink_inside_its_axes_that_hiding_it_removes() {
    let Some(shown) = rendered_or_skip(render_offscreen(&figure_with_surface(true), &TEXT, 72.0))
    else {
        return;
    };
    let Some(hidden) = rendered_or_skip(render_offscreen(&figure_with_surface(false), &TEXT, 72.0))
    else {
        return;
    };
    assert_eq!(
        (shown.width, shown.height),
        (hidden.width, hidden.height),
        "the two renders have the figure's size"
    );

    let scene = ironlab_scene::compile(&figure_with_surface(true), &TEXT);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let plot = scene.hit_map.axes[0].plot_rect;
    let mut differing = 0;
    for y in 0..shown.height {
        for x in 0..shown.width {
            let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let inside =
                (plot.x..=plot.right()).contains(&cx) && (plot.y..=plot.bottom()).contains(&cy);
            if inside && shown.pixel(x, y) != hidden.pixel(x, y) {
                differing += 1;
            }
        }
    }
    assert!(
        differing > 0,
        "the surface leaves ink inside its axes at {plot:?}, which hiding it removes"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// The draws of every kind through the painter: the background, the scissor of a clip, glyph runs and the mapping
// uniform. The pages are 100 by 100 points rendered at 72 dpi unless a test says otherwise.
// ---------------------------------------------------------------------------------------------------------------------

/// The pixels of `image` that are not the white background, as `(x, y)`.
fn inked_pixels(image: &RenderedImage) -> Vec<(u32, u32)> {
    (0..image.height)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| !close_to(image.pixel(x, y), WHITE_PX, 1))
        .collect()
}

// Why: the figure background is the colour the renderer clears its target to, not a draw of the list, so that a
// page whose pixel size rounds up (30 points at 50 dpi is 20.8, so 21, pixels tall) is background to its last row
// rather than showing transparent black through a quad that ends a fraction of a pixel short. Every pixel of an
// empty page must be the background colour, whether its pixel size is whole or rounded up, and at any dpi.
#[test]
fn the_background_covers_every_pixel_of_the_image() {
    for (width_pt, height_pt, dpi, size, background, expected) in [
        (
            100.0,
            100.0,
            72.0,
            (100, 100),
            Rgba::new(0.2, 0.4, 0.6, 1.0),
            [51, 102, 153, 255],
        ),
        (
            72.0,
            36.0,
            144.0,
            (144, 72),
            Rgba::new(1.0, 0.5, 0.0, 1.0),
            [255, 128, 0, 255],
        ),
        (30.0, 50.0, 300.0, (125, 208), Rgba::BLACK, [0, 0, 0, 255]),
        (
            100.0,
            30.0,
            50.0,
            (69, 21),
            Rgba::new(0.0, 0.5, 0.0, 1.0),
            [0, 128, 0, 255],
        ),
    ] {
        let list = page(width_pt, height_pt, background, vec![]);
        let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, dpi)) else {
            return;
        };
        assert_eq!(
            (image.width, image.height),
            size,
            "the pixel size of {width_pt}×{height_pt} pt at {dpi} dpi"
        );
        let wrong = (0..image.height)
            .flat_map(|y| (0..image.width).map(move |x| (x, y)))
            .find(|&(x, y)| !close_to(image.pixel(x, y), expected, 1))
            .map(|(x, y)| (x, y, image.pixel(x, y)));
        assert_eq!(
            wrong, None,
            "every pixel of the {}×{} image at {dpi} dpi is the background {expected:?}; the first that is not, as \
             (x, y, pixel)",
            image.width, image.height
        );
    }
}

// Why: an axes clips its artists to its plot rectangle, and the clip reaches the painter as a scissor rectangle
// rather than as clipped geometry, so that every draw is cut alike, in whole pixels, as egui cuts its own clip
// rectangles. An edge of the clip at a fraction of a pixel must be rounded to the nearest pixel boundary: the pixel
// inside the rounded edge is painted whole, the pixel that straddles it is at most half covered, and the pixel
// beyond that is untouched. A geometric clip would blend the pixel the edge crosses, a scissor that truncated would
// move the edge by up to a pixel from where egui puts it, and one that took the clip in figure points for pixels
// would put it in the wrong place at every dpi but 72.
#[test]
fn a_clip_with_fractional_edges_is_cut_at_the_nearest_whole_pixels() {
    let clip = Rect::new(20.3, 30.6, 40.4, 29.8);
    let wide_face = || {
        filled_polygon(
            &[(5.0, 5.0), (95.0, 5.0), (95.0, 95.0), (5.0, 95.0)],
            Rgba::new(1.0, 0.0, 0.0, 1.0),
        )
    };
    // The clip spans x ∈ [20.3, 60.7] and y ∈ [30.6, 60.4] in points; at each dpi, the first and the last column
    // and row inside the scissor once every edge is rounded to the nearest pixel.
    for (dpi, (first_column, last_column), (first_row, last_row)) in [
        (72.0, (20_u32, 60_u32), (31_u32, 59_u32)),
        (144.0, (41, 120), (61, 120)),
    ] {
        let list = page(100.0, 100.0, Rgba::WHITE, vec![clipped(clip, wide_face())]);
        let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, dpi)) else {
            return;
        };
        let middle_column = (first_column + last_column) / 2;
        let middle_row = (first_row + last_row) / 2;
        for (x, y, what) in [
            (
                first_column,
                middle_row,
                "the first column inside the clip is painted whole",
            ),
            (
                last_column,
                middle_row,
                "the last column inside the clip is painted whole",
            ),
            (
                middle_column,
                first_row,
                "the first row inside the clip is painted whole",
            ),
            (
                middle_column,
                last_row,
                "the last row inside the clip is painted whole",
            ),
        ] {
            assert_pixel(&image, x, y, RED_PX, 1, &format!("{what} at {dpi} dpi"));
        }
        // The pixel that straddles each rounded edge, then the pixel beyond it, which the clip reaches under no
        // reading of the rounding. Every one of them lies inside the face, so only the scissor can leave it bare.
        for (x, y, what) in [
            (first_column - 1, middle_row, "the column before the clip"),
            (last_column + 1, middle_row, "the column after the clip"),
            (middle_column, first_row - 1, "the row before the clip"),
            (middle_column, last_row + 1, "the row after the clip"),
        ] {
            assert_outside_the_scissor(
                &image,
                x,
                y,
                RED_PX,
                WHITE_PX,
                &format!("{what} at {dpi} dpi"),
            );
        }
        for (x, y, what) in [
            (
                first_column - 2,
                middle_row,
                "the second column before the clip",
            ),
            (
                last_column + 2,
                middle_row,
                "the second column after the clip",
            ),
            (
                middle_column,
                first_row - 2,
                "the second row before the clip",
            ),
            (middle_column, last_row + 2, "the second row after the clip"),
        ] {
            assert_pixel(
                &image,
                x,
                y,
                WHITE_PX,
                1,
                &format!("{what} is untouched at {dpi} dpi"),
            );
        }
    }
}

// Why: every label and tick of a figure is a glyph run, and the vertex tests in `canvas.rs` pin where its outline
// goes; what only the pixels can tell is that the outline is filled and drawn in the run's colour, and that nothing
// of it lands outside the em box above the pen origin once rasterised. A run drawn as an outline, in the wrong
// colour, or flipped into the font's y-up space would fail here and nowhere else.
#[test]
fn a_glyph_run_renders_solid_ink_in_its_colour_inside_the_em_box_above_its_pen_origin() {
    for (origin, size, color, expected) in [
        (
            Point::new(30.0, 70.0),
            40.0,
            Rgba::new(0.0, 0.0, 1.0, 1.0),
            BLUE_PX,
        ),
        (
            Point::new(55.0, 40.0),
            32.0,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            RED_PX,
        ),
    ] {
        let Some(image) = render_page(Rgba::WHITE, vec![glyph_h(origin, size, color)]) else {
            return;
        };
        let inked = inked_pixels(&image);
        // The em box above the pen origin, widened by a pixel on the left and the top for the anti-aliasing of the
        // outline; a capital H sits on the baseline, so no row below the origin may carry ink.
        let outside: Vec<(u32, u32)> = inked
            .iter()
            .copied()
            .filter(|&(x, y)| {
                let (x, y) = (f64::from(x), f64::from(y));
                x < origin.x - 1.0
                    || x > origin.x + size
                    || y < origin.y - size - 1.0
                    || y > origin.y
            })
            .collect();
        assert!(
            outside.is_empty(),
            "the glyph of {size} pt at {origin:?} leaves ink outside the em box above its pen origin, at \
             {outside:?}"
        );
        assert!(
            inked
                .iter()
                .any(|&(x, y)| close_to(image.pixel(x, y), expected, 1)),
            "the glyph of {size} pt at {origin:?} has a pixel filled solid in the run colour {expected:?}"
        );
    }
}

// Why: the geometry of a list is in figure points at every dpi and reaches the pixels through the mapping uniform
// alone; a mapping applied to the wrong axis, in the wrong order with the origin, or not at all would draw a 144-dpi
// image with its content at the 72-dpi size in a corner, and the same fault would misplace every figure on screen.
// One list rendered at 72 and at 144 dpi must give an image of twice the size in which every solid interior lies at
// twice the coordinates, for a draw of every kind: a path, a clipped path, an image tile and a glyph run.
#[test]
fn the_same_list_at_twice_the_dpi_is_the_same_picture_at_twice_the_size() {
    let list = page(
        100.0,
        100.0,
        Rgba::WHITE,
        vec![
            filled_polygon(
                &[(5.0, 5.0), (45.0, 5.0), (45.0, 45.0), (5.0, 45.0)],
                Rgba::new(1.0, 0.0, 0.0, 1.0),
            ),
            clipped(
                Rect::new(50.0, 5.0, 45.0, 40.0),
                filled_polygon(
                    &[(30.0, 0.0), (100.0, 0.0), (100.0, 60.0), (30.0, 60.0)],
                    Rgba::new(0.0, 0.0, 1.0, 1.0),
                ),
            ),
            placed_image(
                Rect::new(0.0, 0.0, 2.0, 2.0),
                2,
                2,
                ImageItem::RGBA,
                QUAD_PIXELS.concat(),
                scale_then_translate(20.0, 20.0, 55.0, 55.0),
            ),
            // At 64 pt the stems of the H are about 6 pixels wide, so that a 3 × 3 block of solid black exists
            // below; stems under 3 pixels would leave no solid interior for the comparison to find.
            glyph_h(Point::new(5.0, 97.0), 64.0, Rgba::BLACK),
        ],
    );
    let Some(small) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, 72.0)) else {
        return;
    };
    let Some(large) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, 144.0)) else {
        return;
    };
    assert_eq!(
        (large.width, large.height),
        (2 * small.width, 2 * small.height),
        "twice the dpi gives twice the pixels"
    );

    // A pixel of the small image is solid when its 3 × 3 neighbourhood is one colour; the 2 × 2 block of the large
    // image at twice its coordinates then lies a whole point inside the same region, clear of any anti-aliased edge.
    let mut solid: Vec<(u32, u32, [u8; 4])> = Vec::new();
    for y in 1..small.height - 1 {
        for x in 1..small.width - 1 {
            let colour = small.pixel(x, y);
            if (y - 1..=y + 1).all(|ny| (x - 1..=x + 1).all(|nx| small.pixel(nx, ny) == colour)) {
                solid.push((x, y, colour));
            }
        }
    }
    for expected in [
        RED_PX,
        BLUE_PX,
        GREEN_PX,
        YELLOW_PX,
        WHITE_PX,
        [0, 0, 0, 255],
    ] {
        assert!(
            solid
                .iter()
                .any(|&(_, _, colour)| close_to(colour, expected, 1)),
            "the 72-dpi image has a solid interior of {expected:?}"
        );
    }
    let wrong = solid.iter().find_map(|&(x, y, colour)| {
        [
            (2 * x, 2 * y),
            (2 * x + 1, 2 * y),
            (2 * x, 2 * y + 1),
            (2 * x + 1, 2 * y + 1),
        ]
        .into_iter()
        .find(|&(lx, ly)| !close_to(large.pixel(lx, ly), colour, 1))
        .map(|(lx, ly)| (x, y, colour, lx, ly, large.pixel(lx, ly)))
    });
    assert_eq!(
        wrong, None,
        "every solid pixel of the 72-dpi image is the colour of the 2 × 2 block at twice its coordinates in the \
         144-dpi image; the first that is not, as (x, y, colour, X, Y, pixel)"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Strokes, expanded by the stroke pipeline: joins, caps, dashes, the hairline and the pen beneath a group transform,
// pinned by direct pixel probes and by pdftoppm's rasters of the same display lists exported through `ironlab_pdf`.
// ---------------------------------------------------------------------------------------------------------------------

/// A solid black stroke `width` wide with butt caps and miter joins, which every stroke test starts from.
fn pen(width: f64) -> Stroke {
    Stroke {
        color: Rgba::BLACK,
        width,
        dash: Vec::new(),
        dash_offset: 0.0,
        cap: LineCap::Butt,
        join: LineJoin::Miter,
    }
}

/// An open polyline through `points` stroked with `stroke`, neither filled nor given a depth.
fn stroked_polyline(points: &[(f64, f64)], stroke: Stroke) -> Item {
    stroked_path(points, false, stroke)
}

/// The polyline through `points` closed back to its first point and stroked with `stroke`, neither filled nor given
/// a depth: one closed subpath, whose seam is the joint at `points[0]`.
fn stroked_polygon(points: &[(f64, f64)], stroke: Stroke) -> Item {
    stroked_path(points, true, stroke)
}

fn stroked_path(points: &[(f64, f64)], closed: bool, stroke: Stroke) -> Item {
    let mut segments = vec![PathSegment::MoveTo(Point::new(points[0].0, points[0].1))];
    segments.extend(
        points[1..]
            .iter()
            .map(|&(x, y)| PathSegment::LineTo(Point::new(x, y))),
    );
    if closed {
        segments.push(PathSegment::Close);
    }
    Item {
        source: None,
        kind: ItemKind::Path(PathItem {
            segments,
            fill: None,
            stroke: Some(stroke),
            depth: None,
        }),
    }
}

/// The three joins and the three caps, named for the messages of the table-driven tests.
const JOINS: [(&str, LineJoin); 3] = [
    ("miter", LineJoin::Miter),
    ("round", LineJoin::Round),
    ("bevel", LineJoin::Bevel),
];
const CAPS: [(&str, LineCap); 3] = [
    ("butt", LineCap::Butt),
    ("round", LineCap::Round),
    ("square", LineCap::Square),
];

/// The brightness of a pixel of a black-on-white render: 0 where the ink covers it whole, 255 where none touches it.
fn level(pixel: [u8; 4]) -> u8 {
    ((u16::from(pixel[0]) + u16::from(pixel[1]) + u16::from(pixel[2])) / 3) as u8
}

/// What a pixel of a black-on-white render holds. The bands leave an eighth of the range at each end for the
/// rounding of a pixel covered whole or untouched, and take everything between as an anti-aliased edge; the probes
/// that ask for `Ink` or `Blank` lie at least a pixel inside or outside every edge, where no anti-aliasing reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shade {
    /// Covered whole by the ink: a level of at most 31.
    Ink,
    /// Untouched by the ink: a level of at least 224.
    Blank,
    /// Crossed by an anti-aliased edge: a level strictly between the two.
    Partial,
}

/// A column to probe on a known row, the shade expected there and what it is.
type ShadeProbe = (u32, Shade, &'static str);

/// A pixel to probe, the shade expected there and what it is.
type PixelProbe = (u32, u32, Shade, &'static str);

impl Shade {
    fn of(pixel: [u8; 4]) -> Self {
        match level(pixel) {
            0..=31 => Self::Ink,
            224..=255 => Self::Blank,
            _ => Self::Partial,
        }
    }
}

/// Asserts that the pixel at `(x, y)` of a black-on-white render holds `shade`.
#[track_caller]
fn assert_shade(image: &RenderedImage, x: u32, y: u32, shade: Shade, what: &str) {
    let pixel = image.pixel(x, y);
    assert!(
        Shade::of(pixel) == shade,
        "{what} at ({x}, {y}): expected {shade:?}, got {pixel:?}"
    );
}

/// The number of pixels of row `y` within `columns` that black ink covers by more than half.
fn inked_columns(image: &RenderedImage, y: u32, columns: Range<u32>) -> usize {
    columns.filter(|&x| level(image.pixel(x, y)) < 128).count()
}

/// The number of pixels of column `x` within `rows` that black ink covers by more than half.
fn inked_rows(image: &RenderedImage, x: u32, rows: Range<u32>) -> usize {
    rows.filter(|&y| level(image.pixel(x, y)) < 128).count()
}

/// The number of pixels of a raster that black ink covers by more than half.
fn dark_pixels(image: &RgbImage) -> usize {
    image
        .pixels()
        .filter(|p| (u16::from(p.0[0]) + u16::from(p.0[1]) + u16::from(p.0[2])) / 3 < 128)
        .count()
}

/// The zigzag of the comparisons with the PDF export, on a 120 by 80 point page: three arms of about 40 points,
/// several dash periods each, with a hairpin of about 20° between the first two, where a miter join exceeds the
/// limit of 4 and falls back to a bevel, and a turn of about 60° between the last two, where a miter of ratio 2
/// (half the limit) reaches 6 points beyond the vertex. Nothing about it is axis-aligned, so no rasteriser can snap
/// it to the pixel grid.
const ZIGZAG: [(f64, f64); 4] = [(33.0, 30.0), (40.0, 70.0), (47.0, 30.0), (78.0, 56.0)];

/// The dash patterns of the comparisons, in item units: none; dashes of 6 with gaps of 4; dashes of 1 with gaps of
/// 3, which butt caps draw as fine dashes and which round or square caps, extending every dash by half the width of
/// 6 at both ends, close up into a line with scalloped or straight edges, as PDF draws them; and dashes of no
/// length with gaps of 4, PDF's dotted line, which round caps draw as dots and butt or square caps as nothing (PDF
/// 32000, 8.5.3.2).
fn dash_patterns() -> [(&'static str, Vec<f64>); 4] {
    [
        ("solid", Vec::new()),
        ("6-4", vec![6.0, 4.0]),
        ("1-3", vec![1.0, 3.0]),
        ("0-4", vec![0.0, 4.0]),
    ]
}

/// The resolution of the comparisons with poppler: two pixels per point, so that the blocks compared are 12 pixels
/// square.
const COMPARISON_DPI: f64 = 144.0;

/// Renders `list` offscreen at [`COMPARISON_DPI`] and rasterises its PDF export at the same resolution with
/// poppler, saves both rasters as `name` in `ws` for inspection when the test fails, and asserts that they show the
/// same picture within the block tolerances of `assert_same_picture`. Both rasters must hold ink (at least 300 dark
/// pixels), so that two blank pages cannot agree, unless the list is `inked` by nothing, in which case both must be
/// blank. Returns `None` without asserting anything when no adapter is available and a GPU is not required, for
/// the caller to skip the rest of its test.
#[track_caller]
fn assert_drawn_as_printed(
    ws: &Workspace,
    name: &str,
    what: &str,
    list: &DisplayList,
    inked: bool,
) -> Option<()> {
    let drawn = rendered_or_skip(render_display_list_offscreen(list, &TEXT, COMPARISON_DPI))?;
    let exported = ironlab_pdf::render_display_list(list, &TEXT, &PdfOptions::default(), None)
        .unwrap_or_else(|error| panic!("{what}: the export failed: {error}"));
    assert!(
        exported.warnings.is_empty(),
        "{what}: the export has nothing to warn of: {:?}",
        exported.warnings
    );
    let printed = rasterise(&ws.write(name, &exported.bytes), COMPARISON_DPI);
    let drawn = rgb_of(&drawn);
    drawn
        .save(ws.path(&format!("{name}-viewer.png")))
        .expect("save the viewer's raster beside poppler's");
    assert_eq!(
        printed.dimensions(),
        drawn.dimensions(),
        "{what}: poppler and the viewer raster the page at the same size"
    );
    for (which, raster) in [("poppler", &printed), ("the viewer", &drawn)] {
        let dark = dark_pixels(raster);
        if inked {
            assert!(
                dark >= 300,
                "{what}: {which} inks at least 300 pixels, so that the comparison is not of two blank pages; it \
                 inked {dark}"
            );
        } else {
            assert_eq!(dark, 0, "{what}: {which} inks nothing");
        }
    }
    assert_same_picture(&printed, &drawn, COMPARISON_DPI, what);
    Some(())
}

// Why: the viewer must show the stroke that the PDF prints. The shaders expand every join, cap and dash themselves,
// so a miter that ignored the limit, a cap missing from the ends of a dash, a dash phase that restarted at every
// segment, a pattern scaled by the width or a round join drawn as a full disc on the wrong side would pass every
// test of the segments in `canvas.rs` and still put a different picture on the screen from the one on the page.
// Comparing block means against pdftoppm's raster of the same display list tolerates the two anti-aliasers'
// disagreement along an edge and nothing larger: at 144 dpi a block is 12 pixels square and may differ by 16
// levels of 255, so a missing round cap of the 6 pt pen (a half disc of 57 px², about 100 levels in the block that
// holds it) or a missing miter at the 60° turn (47 px² beyond the bevel, over 20 levels in some block however the
// blocks cut it) is detected, while a missing bevel at the hairpin (6 px², 11 levels) is not, and the probe test of
// the joins covers it; a systematic offset of half a pixel moves about 6 px² of the 12 px wide stroke across a
// block's edge (10 levels) and passes, one of a whole pixel (21 levels) is detected. Both rasters must hold ink, or
// two blank pages would agree, except for the dashes of no length with butt caps, which draw nothing in PDF and so
// must draw nothing here.
#[test]
fn every_join_cap_and_dash_pattern_of_a_zigzag_is_the_picture_poppler_prints_of_its_export() {
    if !tools_available(&["pdftoppm"]) {
        return;
    }
    let ws = Workspace::new("strokes");
    for (join_name, join) in JOINS {
        for (cap_name, cap) in CAPS {
            for (pattern_name, dash) in dash_patterns() {
                let what = format!("{join_name} joins, {cap_name} caps, {pattern_name}");
                let name = format!("{join_name}-{cap_name}-{pattern_name}");
                // Dashes of no length draw nothing with butt or square caps, in PDF as here, so both rasters are
                // blank in those cells and only the round-capped dots are compared as ink.
                let dotted = dash.first() == Some(&0.0);
                let inked = !(dotted && cap != LineCap::Round);
                let stroke = Stroke {
                    dash,
                    cap,
                    join,
                    ..pen(6.0)
                };
                let list = page(
                    120.0,
                    80.0,
                    Rgba::WHITE,
                    vec![stroked_polyline(&ZIGZAG, stroke)],
                );
                if assert_drawn_as_printed(&ws, &name, &what, &list, inked).is_none() {
                    return;
                }
            }
        }
    }
}

// Why: the seam of a closed dashed subpath is one joint measured twice, at arc length 0 by the first edge and at
// the perimeter by the closing one, and unless the perimeter is a multiple of the period the pattern differs
// between the two: PDF runs the pattern from the start around to the seam, so the dashes of the closing edge are
// placed and capped by the perimeter-side length, and the dash starting at the seam is capped there. A square of
// side 17 and a rectangle of 24 by 15.5 have perimeters of 68 and 79, which put the seam 2 and 3 pt into a gap of
// the closing edge while the first edge starts a dash, and no corner on the boundary of a dash; a shader that
// measured the seam at 0 on both sides would draw a join there, where poppler prints a gap and a capped dash.
// Where the dash runs through the seam on both sides poppler draws no join, printing every dash as a separate
// open piece, while the join of every corner within a dash calls for one, so that case is pinned by the probe test
// of the seam rather than here. Every coordinate is a multiple of half a point, so that at 144 dpi the edges of
// the pen and the ends of the dashes lie on pixel boundaries, which poppler otherwise snaps them to (its stroke
// adjustment), half a pixel from where the viewer anti-aliases them.
#[test]
fn a_dashed_closed_outline_is_the_picture_poppler_prints_of_its_export_around_the_seam() {
    if !tools_available(&["pdftoppm"]) {
        return;
    }
    let ws = Workspace::new("closed-strokes");
    for (cap_name, cap) in [("butt", LineCap::Butt), ("round", LineCap::Round)] {
        let what = format!(
            "a closed square of side 17 and a closed rectangle of 24 by 15.5 dashed 6-4 with {cap_name} caps"
        );
        let stroke = Stroke {
            dash: vec![6.0, 4.0],
            cap,
            ..pen(6.0)
        };
        let list = page(
            120.0,
            80.0,
            Rgba::WHITE,
            vec![
                stroked_polygon(
                    &[(15.0, 20.0), (32.0, 20.0), (32.0, 37.0), (15.0, 37.0)],
                    stroke.clone(),
                ),
                stroked_polygon(
                    &[(60.0, 20.0), (84.0, 20.0), (84.0, 35.5), (60.0, 35.5)],
                    stroke,
                ),
            ],
        );
        if assert_drawn_as_printed(&ws, &format!("outlines-{cap_name}"), &what, &list, true)
            .is_none()
        {
            return;
        }
    }
}

// Why: a corner of a dashed outline is joined only where the dash runs through it, and the seam of a closed
// subpath is a corner measured twice, at arc length 0 by the first edge and at the perimeter by the closing one:
// the join at the seam is drawn where both measures fall inside a dash, whole, as at any other corner inside a
// dash, and not at all where either falls in a gap or where a dash ends exactly at the corner (a dash is the
// half-open run of its arc lengths). Poppler never joins the seam of a dashed outline, so this is pinned by probes
// rather than by the comparison with its raster; a join drawn from the anti-aliased end of a dash at the corner,
// rather than from whether the corner lies inside the dash, would come out half covered at every seam, because
// the seam lies exactly at the start of the first dash whenever the phase is 0.
#[test]
fn the_seam_of_a_closed_dashed_outline_is_joined_whole_where_the_dash_runs_through_it_and_not_otherwise()
 {
    use Shade::{Blank, Ink};
    // A square of side 23 dashed 6 on, 4 off with a 6 pt pen, butt caps and miter joins, whose corners lie at the
    // arc lengths 0 (the seam, also 92), 23, 46 and 69. The outer corner of a miter join is the 3 pt square beyond
    // both edges, probed a pixel inside its edges.
    let corners = [
        ((15.0, 20.0), (13, 18), "the seam"),
        ((38.0, 20.0), (39, 18), "the corner at arc length 23"),
        ((38.0, 43.0), (39, 44), "the corner at arc length 46"),
        ((15.0, 43.0), (13, 44), "the corner at arc length 69"),
    ];
    // The dash phase, and the shade of each corner's join: a phase of 2 moves the dashes back by 2, so that the
    // seam lies inside a dash on both sides (4 pt into the last, 2 pt into the first), the corner at 23 inside the
    // dash from 18 to 24, 46 in a gap and 69 inside a dash; with a phase of 0 the seam lies 2 pt inside the last
    // dash (90 to 96) and at the start of the first, the corner at 23 inside the dash from 20 to 26, the dash from
    // 40 to 46 ends exactly at the corner at 46, and 69 lies in a gap.
    let cases: [(f64, [Shade; 4]); 2] = [
        (2.0, [Ink, Ink, Blank, Ink]),
        (0.0, [Ink, Ink, Blank, Blank]),
    ];
    for (dash_offset, shades) in cases {
        let what = format!("with a dash phase of {dash_offset}");
        let stroke = Stroke {
            dash: vec![6.0, 4.0],
            dash_offset,
            ..pen(6.0)
        };
        let Some(image) = render_page(
            Rgba::WHITE,
            vec![stroked_polygon(
                &corners.map(|(corner, _, _)| corner),
                stroke,
            )],
        ) else {
            return;
        };
        for ((_, (x, y), where_), shade) in corners.iter().zip(shades) {
            assert_shade(
                &image,
                *x,
                *y,
                shade,
                &format!("{what}: the outer corner of the join at {where_}"),
            );
        }
        // The first dash of the first edge runs from x = 15 to 21 with a phase of 0 and to 19 with a phase of 2,
        // and the gap after it to 25 or 23.
        assert_shade(
            &image,
            17,
            19,
            Ink,
            &format!("{what}: inside the first dash of the first edge"),
        );
        assert_shade(
            &image,
            22,
            20,
            Blank,
            &format!("{what}: inside the first gap of the first edge"),
        );
    }
}

// Why: PDF's dotted line is the pattern [0, w], dashes of no length that are nothing but their caps: a dot with
// round caps, and nothing with butt or projecting square caps, as PDF 32000 (8.5.3.2) has for a degenerate subpath
// and as poppler prints. A shader that skipped dashes of no length would lose the dotted line; one that gave them a
// square would print more than the PDF does.
#[test]
fn a_dash_of_no_length_is_a_dot_with_round_caps_and_nothing_with_butt_or_square_caps() {
    use Shade::{Blank, Ink};
    // An 8 pt line along y = 40 from x = 20 to 60 dashed [0, 16], whose dashes of no length lie at x = 20, 36 and
    // 52: round caps make discs of radius 4 about them, and butt and square caps make nothing.
    let cases: [(LineCap, &[PixelProbe]); 3] = [
        (
            LineCap::Butt,
            &[(36, 40, Blank, "the middle dash, which butt caps leave bare")],
        ),
        (
            LineCap::Round,
            &[
                (36, 40, Ink, "the centre of the middle dot"),
                (
                    36,
                    37,
                    Ink,
                    "3 pt above the centre of the middle dot, inside its radius of 4",
                ),
                (
                    32,
                    36,
                    Blank,
                    "the corner of the square a square cap would draw, outside the dot",
                ),
            ],
        ),
        (
            LineCap::Square,
            &[
                (
                    36,
                    40,
                    Blank,
                    "the middle dash, which square caps leave bare too",
                ),
                (
                    32,
                    36,
                    Blank,
                    "the corner of the square a square cap does not draw",
                ),
            ],
        ),
    ];
    for (cap, probes) in cases {
        let what = format!("{cap:?} caps");
        let stroke = Stroke {
            dash: vec![0.0, 16.0],
            cap,
            ..pen(8.0)
        };
        let Some(image) = render_page(
            Rgba::WHITE,
            vec![stroked_polyline(&[(20.0, 40.0), (60.0, 40.0)], stroke)],
        ) else {
            return;
        };
        for (x, y, shade, where_) in [
            (28, 40, Blank, "between the first dash and the middle one"),
            (44, 40, Blank, "between the middle dash and the last"),
        ] {
            assert_shade(&image, x, y, shade, &format!("{what}: {where_}"));
        }
        for &(x, y, shade, where_) in probes {
            assert_shade(&image, x, y, shade, &format!("{what}: {where_}"));
        }
    }
}

// Why: the miter join is the default of every plot line, and its limit is what keeps a sharp turn from growing a
// spike many widths long; the PDF fixes the limit at 4 widths, so the viewer must draw the miter at a turn whose
// miter ratio is 3.9 (an interior angle of 30°) and a bevel at one whose ratio is 11.5 (an interior angle of 10°),
// while round and bevel joins grow no tip at any angle. A join drawn on the wrong side, or not at all, leaves a
// notch between the ends of the two bodies on the outer side of the turn.
#[test]
fn a_miter_join_grows_a_tip_within_the_limit_and_falls_back_to_a_bevel_beyond_it() {
    // A V of two 40 pt arms coming from the left to meet at (60, 40.5), symmetric about the row of pixels centred
    // on y = 40.5, so that the tip of a miter lies along that row to the right of the vertex. The stroke is 8 pt
    // wide, so beyond the vertex along the row the bevel's chord lies 4 sin(half) away (1.0 pt at a half angle of
    // 15°, 0.35 pt at 5°), a round join reaches 4 pt, and a miter reaches 4 / sin(half): 15.5 pt at 15°, and at
    // 5° the 45.9 pt that the limit of 32 pt forbids.
    let vertex = (60.0, 40.5);
    let arm = 40.0;
    use Shade::{Blank, Ink, Partial};
    /// A column to probe on the row of the vertex, the shades accepted there and what it is: one shade where the
    /// pixel lies a pixel inside or outside every edge, and anything but `Blank` for the pixel just beyond the
    /// vertex with a bevel at 15°, whose far edge lies 0.04 pt short of the bevel's chord.
    type JoinProbe = (u32, &'static [Shade], &'static str);
    let cases: [(f64, LineJoin, &[JoinProbe]); 6] = [
        (
            15.0,
            LineJoin::Miter,
            &[
                (
                    60,
                    &[Ink],
                    "the notch between the bodies' ends, filled by the join",
                ),
                (62, &[Ink], "2 pt beyond the vertex, inside the miter"),
                (
                    68,
                    &[Ink],
                    "8 pt beyond the vertex, inside the miter by more than a pixel and beyond any round join",
                ),
                (
                    78,
                    &[Blank],
                    "18 pt beyond the vertex, past the miter's tip at 15.5 pt",
                ),
            ],
        ),
        (
            15.0,
            LineJoin::Round,
            &[
                (
                    60,
                    &[Ink],
                    "the notch between the bodies' ends, filled by the join",
                ),
                (
                    62,
                    &[Ink],
                    "2 pt beyond the vertex, inside the round join's arc of radius 4",
                ),
                (70, &[Blank], "10 pt beyond the vertex, past the arc"),
                (78, &[Blank], "18 pt beyond the vertex"),
            ],
        ),
        (
            15.0,
            LineJoin::Bevel,
            &[
                (
                    60,
                    &[Ink, Partial],
                    "the notch between the bodies' ends, filled by the join up to its chord 1.04 pt beyond the \
                     vertex, at the far edge of the pixel",
                ),
                (
                    62,
                    &[Blank],
                    "2 pt beyond the vertex, past the bevel's chord at 1 pt",
                ),
                (70, &[Blank], "10 pt beyond the vertex"),
                (78, &[Blank], "18 pt beyond the vertex"),
            ],
        ),
        (
            5.0,
            LineJoin::Miter,
            &[
                (
                    62,
                    &[Blank],
                    "2 pt beyond the vertex, past the chord of the bevel the miter falls back to",
                ),
                (
                    70,
                    &[Blank],
                    "10 pt beyond the vertex, which only a miter beyond the limit would reach",
                ),
                (78, &[Blank], "18 pt beyond the vertex"),
            ],
        ),
        (
            5.0,
            LineJoin::Round,
            &[
                (
                    62,
                    &[Ink],
                    "2 pt beyond the vertex, inside the round join's arc of radius 4",
                ),
                (70, &[Blank], "10 pt beyond the vertex, past the arc"),
                (78, &[Blank], "18 pt beyond the vertex"),
            ],
        ),
        (
            5.0,
            LineJoin::Bevel,
            &[
                (
                    62,
                    &[Blank],
                    "2 pt beyond the vertex, past the bevel's chord at 0.35 pt",
                ),
                (70, &[Blank], "10 pt beyond the vertex"),
                (78, &[Blank], "18 pt beyond the vertex"),
            ],
        ),
    ];
    for (degrees, join, probes) in cases {
        let what = format!("a {join:?} join at a half angle of {degrees}°");
        let (sin, cos) = f64::to_radians(degrees).sin_cos();
        let points = [
            (vertex.0 - arm * cos, vertex.1 - arm * sin),
            vertex,
            (vertex.0 - arm * cos, vertex.1 + arm * sin),
        ];
        let stroke = Stroke { join, ..pen(8.0) };
        let Some(image) = render_page(Rgba::WHITE, vec![stroked_polyline(&points, stroke)]) else {
            return;
        };
        // A point 20 pt along the lower arm from the vertex lies within 0.3 pt of the arm's centreline in the
        // pixel that holds it, deep inside the 4 pt half-width.
        let body = (
            (vertex.0 - 20.0 * cos).floor() as u32,
            (vertex.1 + 20.0 * sin).floor() as u32,
        );
        assert_shade(
            &image,
            body.0,
            body.1,
            Shade::Ink,
            &format!("{what}: the body of the lower arm"),
        );
        for &(x, accepted, where_) in probes {
            let pixel = image.pixel(x, 40);
            assert!(
                accepted.contains(&Shade::of(pixel)),
                "{what}: {where_} at ({x}, 40): expected one of {accepted:?}, got {pixel:?}"
            );
        }
    }
}

// Why: a translucent stroke is one shape in PDF, and where its pieces overlap — the bodies of two segments on the
// inner side of a turn, a round join over both bodies, a round cap over the end of a body — the page holds the
// colour once. Outside a depth group the stroke pipeline must therefore never blend a pixel with itself, whatever
// join or cap the stroke has; drawing every piece with plain alpha blending would darken every inner corner, every
// round join and every round cap of a translucent plot line.
#[test]
fn a_translucent_stroke_is_blended_once_where_its_bodies_join_and_caps_overlap() {
    // Half black over white composites to a level of 128 (127.5, rounded either way); blended twice it would be 64.
    let grey = [128, 128, 128, 255];
    for (join_name, join) in JOINS {
        for (cap_name, cap) in CAPS {
            let what = format!("{join_name} joins and {cap_name} caps");
            // An 8 pt right angle from (10, 50) along y = 50 to (50, 50) and up x = 50 to (50, 10): the bodies are
            // the rectangles [10, 50] × [46, 54] and [46, 54] × [10, 50], which overlap in [46, 50]².
            let stroke = Stroke {
                color: Rgba::new(0.0, 0.0, 0.0, 0.5),
                cap,
                join,
                ..pen(8.0)
            };
            let Some(image) = render_page(
                Rgba::WHITE,
                vec![stroked_polyline(
                    &[(10.0, 50.0), (50.0, 50.0), (50.0, 10.0)],
                    stroke,
                )],
            ) else {
                return;
            };
            let body = image.pixel(30, 50);
            assert!(
                close_to(body, grey, 3),
                "{what}: the body at (30, 50) is half black over white, got {body:?}"
            );
            // Every probe lies at least a pixel inside every edge of every piece that covers it, so that no
            // anti-aliasing of an edge enters the comparison.
            let same_as_body: &[(u32, u32, &str)] = &[
                (47, 47, "the inner corner, inside both bodies"),
                (
                    48,
                    48,
                    "the inner corner beside the vertex, inside both bodies and a round join's disc",
                ),
                (
                    50,
                    50,
                    "the join beside the vertex, beyond the ends of both bodies and 2 px inside every join",
                ),
                (
                    11,
                    50,
                    "the start of the first body, inside a round cap's disc",
                ),
                (
                    50,
                    11,
                    "the end of the second body, inside a round cap's disc",
                ),
            ];
            for &(x, y, where_) in same_as_body {
                let pixel = image.pixel(x, y);
                assert!(
                    close_to(pixel, body, 2),
                    "{what}: {where_} at ({x}, {y}) holds the stroke's colour once, as the body does: expected \
                     {body:?}, got {pixel:?}"
                );
            }
            for (x, y, where_) in [(8, 50, "the start cap"), (50, 8, "the end cap")] {
                let pixel = image.pixel(x, y);
                match cap {
                    LineCap::Butt => assert!(
                        close_to(pixel, WHITE_PX, 2),
                        "{what}: {where_} at ({x}, {y}) is bare with butt caps, got {pixel:?}"
                    ),
                    LineCap::Round | LineCap::Square => assert!(
                        close_to(pixel, body, 2),
                        "{what}: {where_} at ({x}, {y}) holds the stroke's colour once, as the body does: \
                         expected {body:?}, got {pixel:?}"
                    ),
                }
            }
        }
    }
}

// Why: a hairline is the thinnest line the device can draw, one pixel wide whatever the resolution, and it is what
// the compiler draws every axis line and tick with when the width is 0; a shader that took a width of 0 at its word
// would draw nothing, and one that scaled the hairline with the resolution would draw the ticks of a 300 dpi
// gallery image four pixels wide. The line runs down the centre of a pixel column at each resolution, so that the
// column is covered whole and its neighbours are not.
#[test]
fn a_hairline_covers_one_pixel_column_at_every_resolution() {
    for (dpi, x, column) in [(72.0, 20.5, 20u32), (144.0, 20.25, 40)] {
        let list = page(
            100.0,
            100.0,
            Rgba::WHITE,
            vec![stroked_polyline(&[(x, 10.0), (x, 90.0)], pen(0.0))],
        );
        let Some(image) = rendered_or_skip(render_display_list_offscreen(&list, &TEXT, dpi)) else {
            return;
        };
        let y = image.height / 2;
        assert_shade(
            &image,
            column,
            y,
            Shade::Ink,
            &format!("the column of the hairline at {dpi} dpi"),
        );
        for neighbour in [column - 1, column + 1] {
            let brightness = level(image.pixel(neighbour, y));
            assert!(
                brightness >= 192,
                "column {neighbour} beside the hairline at {dpi} dpi holds more than a quarter of the ink: \
                 level {brightness}"
            );
        }
        assert_eq!(
            inked_columns(&image, y, 0..image.width),
            1,
            "at {dpi} dpi exactly one column of row {y} is covered by more than half"
        );
    }
}

// Why: a dashed plot line is drawn by the fragment shader from the arc length, so a shader that measured the arc in
// the wrong units, took the phase with the wrong sign or ended the dashes hard would give dashes of the wrong
// length, in the wrong places, or with a stair-stepped end at every fractional position. Ink inside a dash, none
// inside a gap and an intermediate value in the pixel that an end crosses pin the pattern, the phase and the
// anti-aliasing of the ends at 72 dpi, where a point is a pixel.
#[test]
fn a_dashed_line_is_inked_in_its_dashes_bare_in_its_gaps_and_anti_aliased_where_a_dash_ends() {
    // A 4 pt line along y = 30 from x = 10.5 to 90.5 with dashes of 10 and gaps of 6, so that every dash end falls
    // at the middle of a pixel, and butt caps, so that the dashes end square where the pattern says.
    let cases: [(f64, &[ShadeProbe]); 2] = [
        (
            0.0,
            &[
                (15, Shade::Ink, "inside the first dash, x in [10.5, 20.5)"),
                (23, Shade::Blank, "inside the first gap, x in [20.5, 26.5)"),
                (30, Shade::Ink, "inside the second dash, x in [26.5, 36.5)"),
                (39, Shade::Blank, "inside the second gap, x in [36.5, 42.5)"),
                (
                    20,
                    Shade::Partial,
                    "the pixel the first dash's end crosses at x = 20.5",
                ),
                (
                    36,
                    Shade::Partial,
                    "the pixel the second dash's end crosses at x = 36.5",
                ),
            ],
        ),
        (
            10.0,
            &[
                (
                    13,
                    Shade::Blank,
                    "inside the gap the phase starts the line in, x in [10.5, 16.5)",
                ),
                (20, Shade::Ink, "inside the first dash, x in [16.5, 26.5)"),
                (
                    29,
                    Shade::Blank,
                    "inside the gap after it, x in [26.5, 32.5)",
                ),
                (35, Shade::Ink, "inside the second dash, x in [32.5, 42.5)"),
                (
                    16,
                    Shade::Partial,
                    "the pixel the first dash's start crosses at x = 16.5",
                ),
                (
                    26,
                    Shade::Partial,
                    "the pixel the first dash's end crosses at x = 26.5",
                ),
            ],
        ),
    ];
    for (offset, probes) in cases {
        let what = format!("with a dash offset of {offset}");
        let stroke = Stroke {
            dash: vec![10.0, 6.0],
            dash_offset: offset,
            ..pen(4.0)
        };
        let Some(image) = render_page(
            Rgba::WHITE,
            vec![stroked_polyline(&[(10.5, 30.0), (90.5, 30.0)], stroke)],
        ) else {
            return;
        };
        for &(x, shade, where_) in probes {
            assert_shade(&image, x, 30, shade, &format!("{where_} {what}"));
        }
        // The dashes are as wide as the line, whose body spans y in [28, 32].
        let x = probes
            .iter()
            .find(|(_, shade, _)| *shade == Shade::Ink)
            .expect("every case probes a dash")
            .0;
        assert_shade(
            &image,
            x,
            28,
            Shade::Ink,
            &format!("the top row of the line {what}"),
        );
        assert_shade(
            &image,
            x,
            31,
            Shade::Ink,
            &format!("the bottom row of the line {what}"),
        );
        assert_shade(
            &image,
            x,
            27,
            Shade::Blank,
            &format!("the row above the line {what}"),
        );
        assert_shade(
            &image,
            x,
            32,
            Shade::Blank,
            &format!("the row beneath the line {what}"),
        );
    }
}

// Why: a group transform scales the pen and the dashes with the geometry, as PDF does, so beneath the transform of
// an axes that stretches its data space unevenly a stroke is wider one way than the other and its dashes longer
// along the stretch than across it; only the hairline is exempt, being one screen point wide whichever way it
// runs, so that the axis lines and ticks drawn with a width of 0 stay the thinnest lines the device draws. A
// shader that expanded the stroke in figure points by the item width, by one stretch for every direction, or that
// measured the dashes in figure points, would draw both runs the same width or dash them alike; the transform is a
// pure scale at the origin and the runs are axis-aligned at whole points, so the edges of the pen and the ends of
// the dashes fall on pixel boundaries and the counts are exact.
#[test]
fn a_stroke_beneath_a_non_uniform_transform_is_widened_with_the_geometry() {
    let beneath_stretch = |items: Vec<Item>| {
        render_page(
            Rgba::WHITE,
            vec![Item {
                source: None,
                kind: ItemKind::Group {
                    clip: None,
                    transform: Some(scale_then_translate(4.0, 1.0, 0.0, 0.0)),
                    items,
                },
            }],
        )
    };

    // An L in item space, 2 units wide: down x = 5 from y = 20 to 60, then along y = 60 to x = 20. Beneath
    // scale(4, 1) the vertical run is at figure x = 20 with its pen spanning x in [16, 24], and the horizontal run
    // is at figure y = 60 with its pen spanning y in [59, 61].
    let Some(image) = beneath_stretch(vec![stroked_polyline(
        &[(5.0, 20.0), (5.0, 60.0), (20.0, 60.0)],
        pen(2.0),
    )]) else {
        return;
    };
    assert_eq!(
        inked_columns(&image, 40, 0..100),
        8,
        "the vertical run is eight pixels wide in row 40"
    );
    for x in 16..24 {
        assert_shade(&image, x, 40, Shade::Ink, "inside the vertical run");
    }
    for x in [15, 24] {
        assert_shade(&image, x, 40, Shade::Blank, "beside the vertical run");
    }
    assert_eq!(
        inked_rows(&image, 50, 0..100),
        2,
        "the horizontal run is two pixels tall in column 50"
    );
    for y in [59, 60] {
        assert_shade(&image, 50, y, Shade::Ink, "inside the horizontal run");
    }
    for y in [58, 61] {
        assert_shade(&image, 50, y, Shade::Blank, "beside the horizontal run");
    }
    // The miter at the corner is stretched with the pen: item [4, 5] × [60, 61] is figure [16, 20] × [60, 61].
    assert_shade(
        &image,
        17,
        60,
        Shade::Ink,
        "the miter at the corner, stretched with the pen",
    );

    // Two lines of the same pen dashed 5 on, 5 off in item units, with butt caps: one along y = 30 from item x = 5
    // to 20, whose dashes are stretched to 20 px, on figure x in [20, 40) and [60, 80); and one down x = 5 from
    // y = 50 to 90, whose dashes stay 5 px, on y in [50, 55), [60, 65), [70, 75) and [80, 85).
    let dashed = Stroke {
        dash: vec![5.0, 5.0],
        ..pen(2.0)
    };
    let Some(image) = beneath_stretch(vec![
        stroked_polyline(&[(5.0, 30.0), (20.0, 30.0)], dashed.clone()),
        stroked_polyline(&[(5.0, 50.0), (5.0, 90.0)], dashed),
    ]) else {
        return;
    };
    assert_eq!(
        inked_columns(&image, 30, 0..100),
        40,
        "the dashes along the stretch cover two runs of 20 pixels in row 30"
    );
    for (x, shade, where_) in [
        (30, Shade::Ink, "inside the first dash along the stretch"),
        (50, Shade::Blank, "inside the gap along the stretch"),
        (70, Shade::Ink, "inside the second dash along the stretch"),
    ] {
        assert_shade(&image, x, 30, shade, where_);
    }
    for (y, shade, where_) in [
        (
            29,
            Shade::Ink,
            "the top row of the dashed line along the stretch",
        ),
        (
            30,
            Shade::Ink,
            "the bottom row of the dashed line along the stretch",
        ),
        (
            28,
            Shade::Blank,
            "the row above the dashed line along the stretch",
        ),
        (
            31,
            Shade::Blank,
            "the row beneath the dashed line along the stretch",
        ),
    ] {
        assert_shade(&image, 30, y, shade, where_);
    }
    assert_eq!(
        inked_rows(&image, 20, 45..100),
        20,
        "the dashes across the stretch cover four runs of 5 pixels in column 20 beneath the line along it"
    );
    for (y, shade, where_) in [
        (52, Shade::Ink, "inside the first dash across the stretch"),
        (57, Shade::Blank, "inside the first gap across the stretch"),
        (62, Shade::Ink, "inside the second dash across the stretch"),
        (67, Shade::Blank, "inside the second gap across the stretch"),
    ] {
        assert_shade(&image, 20, y, shade, where_);
    }
    assert_eq!(
        inked_columns(&image, 52, 0..100),
        8,
        "a dash across the stretch is as wide as the stretched pen, eight pixels, in row 52"
    );

    // Two hairlines: down item x = 5.125, which is the centre of figure column 20, from y = 20 to 60; and along
    // y = 70.5, the centre of row 70, from item x = 5 to 20. Each covers its one column or row and no more.
    let Some(image) = beneath_stretch(vec![
        stroked_polyline(&[(5.125, 20.0), (5.125, 60.0)], pen(0.0)),
        stroked_polyline(&[(5.0, 70.5), (20.0, 70.5)], pen(0.0)),
    ]) else {
        return;
    };
    assert_shade(
        &image,
        20,
        40,
        Shade::Ink,
        "the column of the hairline down the stretch",
    );
    for x in [19, 21] {
        let brightness = level(image.pixel(x, 40));
        assert!(
            brightness >= 192,
            "column {x} beside the hairline down the stretch holds more than a quarter of the ink: level \
             {brightness}"
        );
    }
    assert_eq!(
        inked_columns(&image, 40, 0..100),
        1,
        "the hairline down the stretch is one pixel wide in row 40, not four"
    );
    assert_shade(
        &image,
        50,
        70,
        Shade::Ink,
        "the row of the hairline along the stretch",
    );
    for y in [69, 71] {
        let brightness = level(image.pixel(50, y));
        assert!(
            brightness >= 192,
            "row {y} beside the hairline along the stretch holds more than a quarter of the ink: level \
             {brightness}"
        );
    }
    assert_eq!(
        inked_rows(&image, 50, 0..100),
        1,
        "the hairline along the stretch is one pixel tall in column 50"
    );
}

// Why: the caps of a plot line are what PDF draws at its ends, so a butt cap must add nothing, a square cap must
// extend the body by half the width, and a round cap must add a half disc of that radius, no more: the corners of
// the square that a round cap would otherwise fill must stay bare. The line is 8 pt wide from (20, 40) to (60, 40)
// at 72 dpi, so its body is the rectangle [20, 60] × [36, 44] on pixel boundaries, a square cap adds [16, 20] and
// [60, 64] of it, and the round cap's arc of radius 4 about (20, 40) misses the pixel [16, 17] × [36, 37] whose
// nearest corner is 4.24 away, while it covers (17, 40) whole.
#[test]
fn caps_extend_a_line_beyond_its_ends_as_pdf_draws_them() {
    use Shade::{Blank, Ink};
    let cases: [(LineCap, &[PixelProbe]); 3] = [
        (
            LineCap::Butt,
            &[
                (18, 40, Blank, "2 pt before the start, bare with butt caps"),
                (17, 40, Blank, "3 pt before the start, bare with butt caps"),
                (61, 40, Blank, "1 pt after the end, bare with butt caps"),
                (
                    16,
                    36,
                    Blank,
                    "the top-left corner of a square cap's square, bare with butt caps",
                ),
                (
                    16,
                    43,
                    Blank,
                    "the bottom-left corner of a square cap's square, bare with butt caps",
                ),
                (
                    63,
                    36,
                    Blank,
                    "the top-right corner of a square cap's square, bare with butt caps",
                ),
                (
                    63,
                    43,
                    Blank,
                    "the bottom-right corner of a square cap's square, bare with butt caps",
                ),
            ],
        ),
        (
            LineCap::Round,
            &[
                (18, 40, Ink, "2 pt before the start, inside the round cap"),
                (17, 40, Ink, "3 pt before the start, inside the round cap"),
                (61, 40, Ink, "1 pt after the end, inside the round cap"),
                (
                    16,
                    36,
                    Blank,
                    "the top-left corner of the square, outside the round cap's arc",
                ),
                (
                    16,
                    43,
                    Blank,
                    "the bottom-left corner of the square, outside the round cap's arc",
                ),
                (
                    63,
                    36,
                    Blank,
                    "the top-right corner of the square, outside the round cap's arc",
                ),
                (
                    63,
                    43,
                    Blank,
                    "the bottom-right corner of the square, outside the round cap's arc",
                ),
            ],
        ),
        (
            LineCap::Square,
            &[
                (18, 40, Ink, "2 pt before the start, inside the square cap"),
                (17, 40, Ink, "3 pt before the start, inside the square cap"),
                (61, 40, Ink, "1 pt after the end, inside the square cap"),
                (16, 36, Ink, "the top-left corner of the square cap"),
                (16, 43, Ink, "the bottom-left corner of the square cap"),
                (63, 36, Ink, "the top-right corner of the square cap"),
                (63, 43, Ink, "the bottom-right corner of the square cap"),
            ],
        ),
    ];
    for (cap, probes) in cases {
        let what = format!("{cap:?} caps");
        let stroke = Stroke { cap, ..pen(8.0) };
        let Some(image) = render_page(
            Rgba::WHITE,
            vec![stroked_polyline(&[(20.0, 40.0), (60.0, 40.0)], stroke)],
        ) else {
            return;
        };
        for (x, y, shade, where_) in [
            (30, 40, Ink, "the middle of the body"),
            (30, 36, Ink, "the top row of the body"),
            (30, 43, Ink, "the bottom row of the body"),
            (30, 35, Blank, "the row above the body"),
            (30, 44, Blank, "the row beneath the body"),
            (20, 40, Ink, "the first column of the body"),
            (59, 40, Ink, "the last column of the body"),
            (14, 40, Blank, "6 pt before the start, beyond any cap"),
            (65, 40, Blank, "5 pt after the end, beyond any cap"),
        ] {
            assert_shade(&image, x, y, shade, &format!("{what}: {where_}"));
        }
        for &(x, y, shade, where_) in probes {
            assert_shade(&image, x, y, shade, &format!("{what}: {where_}"));
        }
    }
}

// Why: a stroke outside every depth group is depth-tested with `Less` against whatever the depth buffer holds, so
// that it never blends with itself, and the painter must therefore clear the buffer on leaving a depth group as
// well as on entering one: without the clear on leaving, a plot line listed after a 3D axes and crossing one of
// its faces (a legend line, the frame of a second axes) would fail the test against the face's depth wherever it
// crosses, and vanish there, although the paint order puts it on top.
#[test]
fn a_stroke_listed_after_a_depth_group_is_drawn_over_the_faces_of_the_group() {
    // A red face at depth 1, the nearest of its group (z = 0) beside a far pin at depth 0 (z = 1), and after the
    // group an 8 pt blue line crossing the face along y = 40.
    let group = depth_group(vec![
        face(
            20.0,
            20.0,
            60.0,
            60.0,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            DepthPlane::constant(1.0),
        ),
        face(
            80.0,
            80.0,
            90.0,
            90.0,
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            DepthPlane::constant(0.0),
        ),
    ]);
    let line = stroked_polyline(
        &[(10.0, 40.0), (90.0, 40.0)],
        Stroke {
            color: Rgba::new(0.0, 0.0, 1.0, 1.0),
            ..pen(8.0)
        },
    );
    let Some(image) = render_page(Rgba::WHITE, vec![group, line]) else {
        return;
    };
    assert_pixel(
        &image,
        40,
        40,
        BLUE_PX,
        1,
        "the line where it crosses the face",
    );
    assert_pixel(&image, 70, 40, BLUE_PX, 1, "the line beside the face");
    assert_pixel(&image, 40, 30, RED_PX, 1, "the face above the line");
    assert_pixel(&image, 40, 50, RED_PX, 1, "the face beneath the line");
}

// Why: every stroke draw reads its params from its own slot of one uniform buffer, bound through a dynamic offset,
// and outside every depth group takes a z below that of the strokes before it, so that the later of two crossing
// lines passes the depth test at the crossing and shows there, as PDF paints the later line over the earlier one.
// A painter that bound the slot of the first draw for the second would draw the second line in the first's colour
// and at the first's z, so that the first would show at the crossing.
#[test]
fn of_two_crossing_opaque_strokes_outside_a_depth_group_the_later_one_shows_at_the_crossing() {
    let red = stroked_polyline(
        &[(10.0, 50.0), (90.0, 50.0)],
        Stroke {
            color: Rgba::new(1.0, 0.0, 0.0, 1.0),
            ..pen(8.0)
        },
    );
    let blue = stroked_polyline(
        &[(50.0, 10.0), (50.0, 90.0)],
        Stroke {
            color: Rgba::new(0.0, 0.0, 1.0, 1.0),
            ..pen(8.0)
        },
    );
    let Some(image) = render_page(Rgba::WHITE, vec![red, blue]) else {
        return;
    };
    assert_pixel(
        &image,
        50,
        50,
        BLUE_PX,
        1,
        "the crossing, where the later line shows",
    );
    assert_pixel(
        &image,
        30,
        50,
        RED_PX,
        1,
        "the first line away from the crossing",
    );
    assert_pixel(
        &image,
        50,
        30,
        BLUE_PX,
        1,
        "the second line away from the crossing",
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Markers, drawn as instances of one outline through the marker pipelines: every shape against poppler's raster of
// the same list, the sizes and colours of a scatter, the blending of a translucent marker and of overlapping ones,
// a stroke listed after the markers, and markers among the faces of a depth group.
// ---------------------------------------------------------------------------------------------------------------------

/// A marker of `size` centred on `(x, y)` at depth 0, filled in `face` and edged in `edge` where given.
fn marker(x: f64, y: f64, size: f64, face: Option<Rgba>, edge: Option<Rgba>) -> MarkerInstance {
    MarkerInstance {
        position: Point::new(x, y),
        depth: 0.0,
        size_pt: size,
        face,
        edge,
        source_index: 0,
    }
}

/// `marker` moved to `depth`.
fn at_depth(mut marker: MarkerInstance, depth: f64) -> MarkerInstance {
    marker.depth = depth;
    marker
}

/// A markers item of `outline`, edged `edge_width` wide, holding `instances`.
fn markers_item(
    outline: Arc<[PathSegment]>,
    edge_width: f64,
    instances: Vec<MarkerInstance>,
) -> Item {
    Item {
        source: None,
        kind: ItemKind::Markers(MarkersItem {
            outline,
            edge_width,
            instances,
        }),
    }
}

/// The closed polyline through `points` as a marker outline.
fn closed_outline(points: &[(f64, f64)]) -> Arc<[PathSegment]> {
    let mut segments = vec![PathSegment::MoveTo(Point::new(points[0].0, points[0].1))];
    segments.extend(
        points[1..]
            .iter()
            .map(|&(x, y)| PathSegment::LineTo(Point::new(x, y))),
    );
    segments.push(PathSegment::Close);
    Arc::from(segments)
}

/// The open arms from the first to the second point of each of `arms` as a marker outline, which is only stroked.
fn open_outline(arms: &[[(f64, f64); 2]]) -> Arc<[PathSegment]> {
    Arc::from(
        arms.iter()
            .flat_map(|&[(x0, y0), (x1, y1)]| {
                [
                    PathSegment::MoveTo(Point::new(x0, y0)),
                    PathSegment::LineTo(Point::new(x1, y1)),
                ]
            })
            .collect::<Vec<_>>(),
    )
}

/// The circle of radius `r` about the origin as the four cubic Béziers the compiler draws a circular marker with.
fn circle_outline(r: f64) -> Arc<[PathSegment]> {
    // The control-point distance at which a cubic Bézier approximates a quarter circle.
    let k = 0.5523 * r;
    Arc::from(vec![
        PathSegment::MoveTo(Point::new(r, 0.0)),
        PathSegment::CubicTo(Point::new(r, k), Point::new(k, r), Point::new(0.0, r)),
        PathSegment::CubicTo(Point::new(-k, r), Point::new(-r, k), Point::new(-r, 0.0)),
        PathSegment::CubicTo(Point::new(-r, -k), Point::new(-k, -r), Point::new(0.0, -r)),
        PathSegment::CubicTo(Point::new(k, -r), Point::new(r, -k), Point::new(r, 0.0)),
        PathSegment::Close,
    ])
}

/// The unit square outline: the closed square of side 1 centred on the origin, which an instance of size `s` draws
/// as the square of side `s` about its centre.
fn unit_square() -> Arc<[PathSegment]> {
    closed_outline(&[(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)])
}

/// A marker shape: its name, its outline in unit space, and the face and the edge an instance of it takes.
type MarkerShape = (&'static str, Arc<[PathSegment]>, Option<Rgba>, Option<Rgba>);

/// A square of a scatter: its centre, its size, its face and edge colours, and the pixels expected of its face and
/// of its edge.
type ColouredSquare = ((u32, u32), f64, Rgba, Rgba, [u8; 4], [u8; 4]);

/// Every marker shape the compiler makes, in unit space, each with the face and the edge an instance of it takes
/// from `face` and `edge`: the closed shapes are filled and edged, the dot is filled with the edge colour and not
/// edged, and the open plus and cross are only edged.
fn marker_shapes(face: Rgba, edge: Rgba) -> Vec<MarkerShape> {
    let h = 0.5;
    let r = 1.15 * h;
    let half_base = r * 3f64.sqrt() / 2.0;
    let s = h * std::f64::consts::FRAC_1_SQRT_2;
    vec![
        ("circle", circle_outline(h), Some(face), Some(edge)),
        ("dot", circle_outline(1.0 / 6.0), Some(edge), None),
        (
            "square",
            closed_outline(&[(-0.45, -0.45), (0.45, -0.45), (0.45, 0.45), (-0.45, 0.45)]),
            Some(face),
            Some(edge),
        ),
        (
            "diamond",
            closed_outline(&[(0.0, -r), (r, 0.0), (0.0, r), (-r, 0.0)]),
            Some(face),
            Some(edge),
        ),
        (
            "triangle-up",
            closed_outline(&[(0.0, -r), (half_base, r / 2.0), (-half_base, r / 2.0)]),
            Some(face),
            Some(edge),
        ),
        (
            "triangle-down",
            closed_outline(&[(0.0, r), (half_base, -r / 2.0), (-half_base, -r / 2.0)]),
            Some(face),
            Some(edge),
        ),
        (
            "plus",
            open_outline(&[[(-h, 0.0), (h, 0.0)], [(0.0, -h), (0.0, h)]]),
            None,
            Some(edge),
        ),
        (
            "cross",
            open_outline(&[[(-s, -s), (s, s)], [(-s, s), (s, -s)]]),
            None,
            Some(edge),
        ),
    ]
}

/// The centres of the twelve markers of a comparison with the PDF export, on a 120 by 80 point page: none of them
/// on a whole point, so that neither rasteriser can snap a marker's outline to its pixel grid.
const MARKER_GRID: [(f64, f64); 12] = [
    (17.3, 16.4),
    (45.6, 16.4),
    (73.9, 16.4),
    (102.2, 16.4),
    (17.3, 40.1),
    (45.6, 40.1),
    (73.9, 40.1),
    (102.2, 40.1),
    (17.3, 63.7),
    (45.6, 63.7),
    (73.9, 63.7),
    (102.2, 63.7),
];

// Why: the viewer must show the marker that the PDF prints. The outline is tessellated once by lyon and laid out
// by the vertex shader, which scales the fill by the instance's size but offsets the edge by half the edge width
// unscaled, and the PDF exporter writes one path per instance instead; an edge scaled with the size, an outline
// mirrored or rotated, a dot not filled, an open outline given a face or a corner mitred rather than rounded would
// pass the tests of the vertices in `canvas.rs` and still put a different picture on the screen from the one on
// the page. Comparing block means against pdftoppm's raster of the same list tolerates the two anti-aliasers'
// disagreement along an edge and nothing larger, as the strokes comparison does: at 144 dpi a marker of size 12
// is 24 pixels across and its edge of width 1 two pixels wide, so an edge drawn twice as wide (a further 48 px² of
// black per marker, over 40 levels in the block that holds it) or a marker displaced by a pixel is detected. Every
// shape is drawn twelve times, so that the dot and the thin plus ink enough pixels for the comparison not to be
// of blank pages.
#[test]
fn every_marker_shape_is_the_picture_poppler_prints_of_its_export() {
    if !tools_available(&["pdftoppm"]) {
        return;
    }
    let ws = Workspace::new("markers");
    let blue = Rgba::new(0.1, 0.3, 0.8, 1.0);
    for (name, outline, face, edge) in marker_shapes(blue, Rgba::BLACK) {
        let instances = MARKER_GRID
            .iter()
            .map(|&(x, y)| marker(x, y, 12.0, face, edge))
            .collect();
        let list = page(
            120.0,
            80.0,
            Rgba::WHITE,
            vec![markers_item(outline, 1.0, instances)],
        );
        if assert_drawn_as_printed(&ws, name, &format!("{name} markers"), &list, true).is_none() {
            return;
        }
    }
}

// Why: a scatter colours and sizes every point on its own, and all of them are instances of one draw, so each
// instance must be laid out at its own size and drawn in its own two colours: an instance that took the size or
// the colours of its neighbour, or an edge that grew with the size, would misreport the data. The squares are
// probed at their centres, three points inside their edges, in their edge bands and two points beyond them, so
// that a marker drawn too small or too large fails as surely as one in the wrong colour.
#[test]
fn a_scatter_of_instances_renders_each_at_its_own_size_and_colours() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let green = Rgba::new(0.0, 1.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    // The centre, size, face and edge of each square, and the pixels expected of its face and its edge.
    let squares: [ColouredSquare; 3] = [
        ((20, 20), 10.0, red, blue, RED_PX, BLUE_PX),
        ((50, 50), 20.0, green, red, GREEN_PX, RED_PX),
        ((75, 75), 30.0, blue, green, BLUE_PX, GREEN_PX),
    ];
    let item = markers_item(
        unit_square(),
        2.0,
        squares
            .iter()
            .map(|&((x, y), size, face, edge, _, _)| {
                marker(f64::from(x), f64::from(y), size, Some(face), Some(edge))
            })
            .collect(),
    );
    let Some(image) = render_page(Rgba::WHITE, vec![item]) else {
        return;
    };
    for ((cx, cy), size, _, _, face_px, edge_px) in squares {
        // The square spans `half` either side of its centre and the edge band of width 2 a point either side of
        // the outline.
        let half = size as u32 / 2;
        let what = format!("the square of size {size}");
        for (x, y, expected, where_) in [
            (cx, cy, face_px, "the centre"),
            (
                cx + half - 3,
                cy,
                face_px,
                "the face 3 pt inside the right edge",
            ),
            (
                cx,
                cy - half + 3,
                face_px,
                "the face 3 pt inside the top edge",
            ),
            (cx + half, cy, edge_px, "the right edge band"),
            (cx, cy - half - 1, edge_px, "the top edge band"),
            (
                cx + half + 2,
                cy,
                WHITE_PX,
                "2 pt beyond the right edge band",
            ),
            (cx, cy - half - 3, WHITE_PX, "2 pt beyond the top edge band"),
        ] {
            assert_pixel(&image, x, y, expected, 2, &format!("{what}: {where_}"));
        }
    }
}

// Why: a marker is one filled and one stroked path in PDF, its edge painted over its fill, so a translucent marker
// must show its fill composited over the background once, its edge composited over the fill once where the band
// lies within the outline and over the background once where it lies beyond, at the corners as along the sides;
// an edge drawn under the fill, a fill drawn once per edge triangle, or a round join overlapping the bodies it
// joins would darken the band or the corners of every translucent marker of a scatter.
#[test]
fn a_translucent_marker_blends_its_fill_over_the_background_once_and_its_edge_over_its_fill_once() {
    let half_black = Rgba::new(0.0, 0.0, 0.0, 0.5);
    // A square of side 40 about (50, 50), spanning [30, 70] in both directions, with an edge of width 4 whose band
    // spans [28, 32] and [68, 72] on each side: its inner half lies over the fill and its outer half over the
    // background. Half black over white composites to a level of 128 (127.5, rounded either way) and half black
    // over that to 64.
    let item = markers_item(
        unit_square(),
        4.0,
        vec![marker(50.0, 50.0, 40.0, Some(half_black), Some(half_black))],
    );
    let Some(image) = render_page(Rgba::WHITE, vec![item]) else {
        return;
    };
    let grey = [128, 128, 128, 255];
    let dark = [64, 64, 64, 255];
    for (x, y, expected, where_) in [
        (50, 50, grey, "the centre of the fill"),
        (65, 50, grey, "the fill 3 pt inside the right band"),
        (
            67,
            67,
            grey,
            "the fill just inside the inner corner of the band",
        ),
        (
            68,
            50,
            dark,
            "the inner half of the right band, over the fill",
        ),
        (
            30,
            50,
            dark,
            "the inner half of the left band, over the fill",
        ),
        (
            50,
            30,
            dark,
            "the inner half of the top band, over the fill",
        ),
        (
            50,
            68,
            dark,
            "the inner half of the bottom band, over the fill",
        ),
        (68, 68, dark, "the inner corner of the band, over the fill"),
        (
            70,
            50,
            grey,
            "the outer half of the right band, over the background once",
        ),
        (
            70,
            70,
            grey,
            "the corner of the band beyond the fill, inside the round join, over the background once",
        ),
        (73, 50, WHITE_PX, "the background beyond the right band"),
        (26, 50, WHITE_PX, "the background beyond the left band"),
    ] {
        assert_pixel(&image, x, y, expected, 3, where_);
    }
}

// Why: outside every depth group the markers are drawn in order without a depth test, each instance's fill and
// then its edge, so where two opaque markers overlap the later one must cover the earlier one, its fill hiding
// the earlier one's edge, as PDF paints the later path over the earlier one; a draw that painted every instance's
// fill and then every edge would show the earlier marker's edge through the later marker's fill, and a depth
// written by the markers would let the earlier one show through. Two instances of one item and two items must
// agree, because the compiler gives a 2D axes one item per artist and a 3D axes one per run.
#[test]
fn of_two_overlapping_opaque_markers_outside_a_depth_group_the_later_one_shows_where_they_overlap()
{
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    // Squares of side 40 with edges of width 4: the first spans [20, 60] along y = 50 with its bands at [18, 22]
    // and [58, 62], the second [40, 80] with its bands at [38, 42] and [78, 82].
    let first = marker(40.0, 50.0, 40.0, Some(red), Some(Rgba::BLACK));
    let second = marker(60.0, 50.0, 40.0, Some(blue), Some(Rgba::BLACK));
    let black = [0, 0, 0, 255];
    for (what, items) in [
        (
            "two instances of one item",
            vec![markers_item(unit_square(), 4.0, vec![first, second])],
        ),
        (
            "two items",
            vec![
                markers_item(unit_square(), 4.0, vec![first]),
                markers_item(unit_square(), 4.0, vec![second]),
            ],
        ),
    ] {
        let Some(image) = render_page(Rgba::WHITE, items) else {
            return;
        };
        for (x, y, expected, where_) in [
            (
                50,
                50,
                BLUE_PX,
                "the overlap, where the later marker's fill shows",
            ),
            (
                60,
                50,
                BLUE_PX,
                "the first marker's right band, beneath the later marker's fill",
            ),
            (
                40,
                50,
                black,
                "the second marker's left band, over the first marker's fill",
            ),
            (30, 50, RED_PX, "the first marker beside the overlap"),
            (70, 50, BLUE_PX, "the second marker beside the overlap"),
            (20, 50, black, "the first marker's left band"),
            (80, 50, black, "the second marker's right band"),
        ] {
            assert_pixel(&image, x, y, expected, 1, &format!("{what}: {where_}"));
        }
    }
}

// Why: outside every depth group a markers draw goes through the pipeline that never writes depth, so that a
// stroke listed after the markers, which is depth-tested with `Less` against whatever the buffer holds at a z of
// its own, passes wherever it crosses them, as PDF paints the later path over the earlier ones; a markers draw
// that wrote its instances' z of 0 would cut every later stroke (the line of the next artist, the frame of the
// axes) wherever it crossed a marker, although the paint order puts the stroke on top. Two instances of one item
// and two items must agree, as for the overlap.
#[test]
fn a_stroke_listed_after_markers_outside_a_depth_group_shows_where_it_crosses_them() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    // The squares of the overlap test, spanning [20, 60] and [40, 80] along y = 50 with edges of width 4, and
    // after them an 8 pt green line along y = 40, through the fills of both squares, from x = 10 to x = 90.
    let first = marker(40.0, 50.0, 40.0, Some(red), Some(Rgba::BLACK));
    let second = marker(60.0, 50.0, 40.0, Some(blue), Some(Rgba::BLACK));
    let line = || {
        stroked_polyline(
            &[(10.0, 40.0), (90.0, 40.0)],
            Stroke {
                color: Rgba::new(0.0, 1.0, 0.0, 1.0),
                ..pen(8.0)
            },
        )
    };
    for (what, items) in [
        (
            "two instances of one item",
            vec![
                markers_item(unit_square(), 4.0, vec![first, second]),
                line(),
            ],
        ),
        (
            "two items",
            vec![
                markers_item(unit_square(), 4.0, vec![first]),
                markers_item(unit_square(), 4.0, vec![second]),
                line(),
            ],
        ),
    ] {
        let Some(image) = render_page(Rgba::WHITE, items) else {
            return;
        };
        for (x, y, expected, where_) in [
            (30, 40, GREEN_PX, "the line over the first marker's fill"),
            (
                40,
                40,
                GREEN_PX,
                "the line over the second marker's left band",
            ),
            (50, 40, GREEN_PX, "the line over the overlap"),
            (70, 40, GREEN_PX, "the line over the second marker's fill"),
            (85, 40, GREEN_PX, "the line beyond the markers"),
            (30, 50, RED_PX, "the first marker beneath the line"),
            (70, 50, BLUE_PX, "the second marker beneath the line"),
        ] {
            assert_pixel(&image, x, y, expected, 1, &format!("{what}: {where_}"));
        }
    }
}

// Why: the markers of a three-dimensional axes lie among its faces at their own depths, and the depth test must
// place each instance by its depth: a marker behind a nearer face is hidden by it and one in front of a farther
// face shows over it, whichever is listed first, as the faces themselves are placed. A marker drawn without the
// test, or at the depth of its item rather than of its instance, would show through the face in front of it or
// vanish behind the one behind it.
#[test]
fn markers_in_a_depth_group_hide_behind_a_nearer_face_and_show_over_a_farther_one() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let green = Rgba::new(0.0, 1.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    let nearer = || face(10.0, 10.0, 50.0, 90.0, red, DepthPlane::constant(1.0));
    let farther = || face(50.0, 10.0, 90.0, 90.0, green, DepthPlane::constant(0.0));
    // Two squares of side 16 at the depth between the faces: one on the nearer face and one on the farther.
    let dots = || {
        markers_item(
            unit_square(),
            1.0,
            vec![
                at_depth(marker(30.0, 50.0, 16.0, Some(blue), None), 0.5),
                at_depth(marker(70.0, 50.0, 16.0, Some(blue), None), 0.5),
            ],
        )
    };
    for (order, items) in [
        (
            "the markers after the faces",
            vec![nearer(), farther(), dots()],
        ),
        (
            "the markers before the faces",
            vec![dots(), nearer(), farther()],
        ),
    ] {
        let Some(image) = render_page(Rgba::WHITE, vec![depth_group(items)]) else {
            return;
        };
        for (x, y, expected, where_) in [
            (
                30,
                50,
                RED_PX,
                "the nearer face, hiding the marker behind it",
            ),
            (70, 50, BLUE_PX, "the marker over the farther face"),
            (30, 30, RED_PX, "the nearer face beside the hidden marker"),
            (70, 30, GREEN_PX, "the farther face beside the shown marker"),
        ] {
            assert_pixel(&image, x, y, expected, 2, &format!("{where_} with {order}"));
        }
    }
}

// Why: a marker whose face is `None` (a hollow circle, the default of an open marker style) is only its edge, so
// the fill slot must draw nothing rather than a transparent-black nothing over the page or an opaque black disc:
// the centre and the interior stay bare while the edge band is inked all the way round.
#[test]
fn a_marker_with_an_edge_and_no_face_leaves_its_centre_bare() {
    use Shade::{Blank, Ink};
    // A square of side 40 about (50, 50) with an edge of width 4: the band spans [28, 32] and [68, 72] on each side.
    let item = markers_item(
        unit_square(),
        4.0,
        vec![marker(50.0, 50.0, 40.0, None, Some(Rgba::BLACK))],
    );
    let Some(image) = render_page(Rgba::WHITE, vec![item]) else {
        return;
    };
    for (x, y, shade, where_) in [
        (50, 50, Blank, "the centre"),
        (65, 50, Blank, "the interior 3 pt inside the right band"),
        (35, 35, Blank, "the interior inside the top-left corner"),
        (70, 50, Ink, "the right band"),
        (30, 50, Ink, "the left band"),
        (50, 30, Ink, "the top band"),
        (50, 70, Ink, "the bottom band"),
        (70, 70, Ink, "the corner of the band, inside the round join"),
        (75, 50, Blank, "the background beyond the right band"),
    ] {
        assert_shade(&image, x, y, shade, where_);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The painter's caches, driven directly on a device: what `prepare` uploads, and what `retain_used` and `clear` drop.
// ---------------------------------------------------------------------------------------------------------------------

/// The device and queue the offscreen renderer would use, or `None` (skipping the test) when no adapter is
/// available and a GPU is not required.
fn device_or_skip() -> Option<(wgpu::Device, wgpu::Queue)> {
    gpu_or_skip(create_device()).map(|(device, queue, _sample_count)| (device, queue))
}

/// An offscreen renderer of this test's own, or `None` (skipping the test) when no adapter is available and a GPU
/// is not required.
fn renderer_or_skip() -> Option<OffscreenRenderer> {
    gpu_or_skip(OffscreenRenderer::new())
}

/// The render target the painter tests prepare pipelines for: the offscreen renderer's format, without
/// multisampling, since nothing is drawn.
const PAINTER_CONFIG: GpuConfig = GpuConfig {
    target_format: wgpu::TextureFormat::Rgba8Unorm,
    samples: 1,
    depth_format: DEPTH_FORMAT,
};

/// The mapping of figure points onto pixels one to one, from the top-left corner.
const ONE_TO_ONE: ScreenTransform = ScreenTransform {
    scale: 1.0,
    origin: egui::Pos2::ZERO,
};

/// A viewport of a 100 by 100 pixel target at one pixel per point, drawn whole and mapped by `to_screen`.
fn viewport(to_screen: ScreenTransform) -> Viewport {
    Viewport::whole([100, 100], 1.0, to_screen)
}

/// A list of one quad from `(0, 0)` to `(10, 10)` in figure points in the premultiplied `color`, textured with
/// `texture` when one is given, as the tessellator emits a filled rectangle or, in white, one tile of an image.
fn quad_list(color: [u8; 4], texture: Option<TileKey>) -> Arc<DrawList> {
    let vertex = |x: f32, y: f32, u: f32, v: f32| Vertex {
        pos: [x, y],
        z: 0.0,
        uv: [u, v],
        color,
    };
    Arc::new(DrawList {
        vertices: vec![
            vertex(0.0, 0.0, 0.0, 0.0),
            vertex(10.0, 0.0, 1.0, 0.0),
            vertex(0.0, 10.0, 0.0, 1.0),
            vertex(10.0, 10.0, 1.0, 1.0),
        ],
        indices: vec![0, 1, 2, 2, 1, 3],
        segments: Vec::new(),
        stroke_params: Vec::new(),
        marker_vertices: Vec::new(),
        marker_indices: Vec::new(),
        markers: Vec::new(),
        draws: vec![Draw {
            kind: DrawKind::Triangles(0..6),
            texture,
            depth_group: None,
            clip: None,
            source: None,
        }],
    })
}

/// A two-by-one opaque RGB image, red then blue, whose tiles the painter uploads as textures.
fn two_pixel_samples() -> Arc<[u8]> {
    Arc::from(vec![255, 0, 0, 0, 0, 255])
}

/// The tile of `columns` of the image of [`two_pixel_samples`] held in `samples`.
fn tile_of(samples: &Arc<[u8]>, columns: Range<u32>) -> TileKey {
    TileKey {
        samples: Arc::clone(samples),
        width: 2,
        channels: 3,
        columns,
        rows: 0..1,
    }
}

// Why: the interactive canvas prepares the same list every frame, and the geometry of a figure of a million points
// must not cross the bus every frame; the painter must recognise a list and a mapping it has already uploaded and
// upload nothing for them.
#[test]
fn preparing_an_unchanged_list_with_an_unchanged_mapping_uploads_nothing_the_second_time() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let mut painter = GpuPainter::default();
    let list = quad_list(WHITE_PX, None);

    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &list,
        &viewport(ONE_TO_ONE),
    );
    let first = painter.uploads();
    assert_eq!(
        first,
        Uploads {
            lists: 1,
            mappings: 1,
            tiles: 0,
        },
        "the first preparation uploads the list's buffers and its mapping, and no tile for a list without textures"
    );

    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &list,
        &viewport(ONE_TO_ONE),
    );
    assert_eq!(
        painter.uploads(),
        first,
        "the second preparation of the same list with the same mapping uploads nothing"
    );
}

// Why: a resize or a pan of the window, or a move to a screen of another density, changes only where the figure
// lands on the target, and re-uploading the geometry for that would make every resize as costly as a rebuild; a
// changed mapping, target size or scale factor must rewrite the mapping uniform of the list and nothing else, while
// a changed clip, which is a scissor at paint time, and a viewport equal to the last one uploaded for the list must
// write nothing.
#[test]
fn a_changed_mapping_rewrites_the_mapping_uniform_and_nothing_else() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let mut painter = GpuPainter::default();
    let samples = two_pixel_samples();
    let list = quad_list(WHITE_PX, Some(tile_of(&samples, 0..2)));
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &list,
        &viewport(ONE_TO_ONE),
    );
    let mut expected = painter.uploads();
    assert_eq!(
        expected,
        Uploads {
            lists: 1,
            mappings: 1,
            tiles: 1,
        },
        "the first preparation uploads the list, its mapping and its tile"
    );

    let scaled = ScreenTransform {
        scale: 2.0,
        origin: egui::Pos2::ZERO,
    };
    let moved = ScreenTransform {
        scale: 2.0,
        origin: egui::pos2(3.5, -7.25),
    };
    for (what, target, writes) in [
        ("a different scale", viewport(scaled), 1),
        ("the same viewport again", viewport(scaled), 0),
        ("a different origin", viewport(moved), 1),
        (
            "a different target size",
            Viewport::whole([200, 150], 1.0, moved),
            1,
        ),
        (
            "a different number of pixels per point",
            Viewport::whole([200, 150], 2.0, moved),
            1,
        ),
        (
            "a different clip and nothing else",
            Viewport {
                clip: egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(20.0, 20.0)),
                ..Viewport::whole([200, 150], 2.0, moved)
            },
            0,
        ),
        ("the first viewport again", viewport(ONE_TO_ONE), 1),
    ] {
        painter.prepare(&device, &queue, PAINTER_CONFIG, &list, &target);
        expected.mappings += writes;
        assert_eq!(
            painter.uploads(),
            expected,
            "{what} writes the mapping {writes} time(s) and uploads neither the list nor its tile"
        );
    }
}

// Why: the viewer shows one list per figure tab and the painter keys lists by their address; a second list must get
// buffers of its own rather than being drawn from the first list's, and a mapping of its own, which has never been
// uploaded and so is written once. Both lists are then kept, so preparing either again uploads nothing.
#[test]
fn a_second_list_uploads_its_own_buffers_and_mapping() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let mut painter = GpuPainter::default();
    let first = quad_list(WHITE_PX, None);
    let second = quad_list(WHITE_PX, None);

    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &first,
        &viewport(ONE_TO_ONE),
    );
    let after_first = painter.uploads();
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &second,
        &viewport(ONE_TO_ONE),
    );
    let after_second = painter.uploads();
    assert_eq!(
        after_second,
        Uploads {
            lists: after_first.lists + 1,
            mappings: after_first.mappings + 1,
            tiles: after_first.tiles,
        },
        "the second list uploads its own buffers and mapping, and no tile"
    );

    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &first,
        &viewport(ONE_TO_ONE),
    );
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &second,
        &viewport(ONE_TO_ONE),
    );
    assert_eq!(
        painter.uploads(),
        after_second,
        "both lists are kept, so preparing either again uploads nothing"
    );
}

// Why: the painter frees what the frames since the previous call did not draw, so that a closed tab's geometry
// leaves the device; a list prepared before a call must survive the call, or every frame would re-upload everything,
// and one not prepared between two calls must go, with its tiles, and be uploaded afresh when it is next drawn.
#[test]
fn retain_used_keeps_a_list_prepared_since_the_previous_call_and_drops_one_that_was_not() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let mut painter = GpuPainter::default();
    let samples = two_pixel_samples();
    let list = quad_list(WHITE_PX, Some(tile_of(&samples, 0..2)));

    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &list,
        &viewport(ONE_TO_ONE),
    );
    painter.retain_used();
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &list,
        &viewport(ONE_TO_ONE),
    );
    assert_eq!(
        painter.uploads(),
        Uploads {
            lists: 1,
            mappings: 1,
            tiles: 1,
        },
        "a list and a tile prepared before the call are kept through it and not uploaded again"
    );

    painter.retain_used();
    painter.retain_used();
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &list,
        &viewport(ONE_TO_ONE),
    );
    assert_eq!(
        painter.uploads(),
        Uploads {
            lists: 2,
            mappings: 2,
            tiles: 2,
        },
        "a list and a tile not prepared between two calls are dropped and uploaded again"
    );
}

// Why: the offscreen renderer clears the painter after every render so that no render leaves buffers or textures on
// the device; every list and tile prepared before `clear` must be uploaded again after it, which is how the test
// tells that they were dropped rather than kept.
#[test]
fn clear_drops_every_list_and_tile() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let mut painter = GpuPainter::default();
    let samples = two_pixel_samples();
    let textured = quad_list(WHITE_PX, Some(tile_of(&samples, 0..2)));
    let plain = quad_list(WHITE_PX, None);
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &textured,
        &viewport(ONE_TO_ONE),
    );
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &plain,
        &viewport(ONE_TO_ONE),
    );
    let before = painter.uploads();

    painter.clear();
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &textured,
        &viewport(ONE_TO_ONE),
    );
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &plain,
        &viewport(ONE_TO_ONE),
    );
    assert_eq!(
        painter.uploads(),
        Uploads {
            lists: before.lists + 2,
            mappings: before.mappings + 2,
            tiles: before.tiles + 1,
        },
        "after `clear` both lists, their mappings and the tile are uploaded again"
    );
}

// Why: a figure rebuilt at a new scale gives a new list whose image tiles are the tiles of the old one, the same
// samples cut at the same pixels, and the old list is drawn until the new one is ready; an image of a hundred
// megapixels must not cross the bus again for a zoom. The painter keys a tile by its sample buffer and its position,
// so a tile shared by two lists is uploaded once, while a different tile of the same buffer is a texture of its own.
#[test]
fn a_tile_shared_by_two_lists_is_uploaded_once_and_a_different_tile_of_the_buffer_again() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let mut painter = GpuPainter::default();
    let samples = two_pixel_samples();
    let first = quad_list(WHITE_PX, Some(tile_of(&samples, 0..1)));
    let second = quad_list(WHITE_PX, Some(tile_of(&samples, 0..1)));
    let other = quad_list(WHITE_PX, Some(tile_of(&samples, 1..2)));

    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &first,
        &viewport(ONE_TO_ONE),
    );
    assert_eq!(
        painter.uploads().tiles,
        1,
        "the first list uploads its tile"
    );
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &second,
        &viewport(ONE_TO_ONE),
    );
    assert_eq!(
        painter.uploads(),
        Uploads {
            lists: 2,
            mappings: 2,
            tiles: 1,
        },
        "the second list shares the first list's tile and uploads only its own buffers and mapping"
    );
    painter.prepare(
        &device,
        &queue,
        PAINTER_CONFIG,
        &other,
        &viewport(ONE_TO_ONE),
    );
    assert_eq!(
        painter.uploads(),
        Uploads {
            lists: 3,
            mappings: 3,
            tiles: 2,
        },
        "a tile of other columns of the same buffer is a texture of its own"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Placement through the viewport, drawn through the renderer's real pass with `render_list`.
// ---------------------------------------------------------------------------------------------------------------------

/// One pixel to check after a render: its column, its row, the colour expected there, the tolerance and what the
/// pixel is.
type PixelCheck = (u32, u32, [u8; 4], u8, &'static str);

/// One pixel straddling a rounded scissor edge, which [`assert_outside_the_scissor`] allows to be half covered:
/// its column, its row and what it is.
type StraddlingCheck = (u32, u32, &'static str);

// Why: the interactive canvas places a figure anywhere on a window of any scale factor and clips it to the canvas,
// and each of those reaches the pixels through the viewport alone: the mapping uniform carries the origin, the
// scale and the target's size in points, and the scissor carries the clip. An origin ignored or rounded would shift
// the figure by up to a pixel from where egui put the canvas, a scale factor ignored would draw a figure at half
// size on a high-density screen, a clip ignored would paint over the neighbouring panels, and a clip lying wholly
// off the target must draw nothing rather than ask the device for an empty scissor, which it refuses.
#[test]
fn the_viewport_places_scales_and_clips_the_list_on_the_target() {
    let Some(mut renderer) = renderer_or_skip() else {
        return;
    };
    let list = quad_list(RED_PX, None);
    let at = |x: f32, y: f32| ScreenTransform {
        scale: 1.0,
        origin: egui::pos2(x, y),
    };
    let clipped_to = |x: f32, y: f32, width: f32, height: f32| Viewport {
        clip: egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, height)),
        ..viewport(ONE_TO_ONE)
    };
    // Half of the four samples of a pixel lie either side of an edge at its middle, so a half-covered pixel is
    // the mean of red and white.
    let half_red = [255, 128, 128, 255];
    let cases: [(&str, Viewport, &[PixelCheck], &[StraddlingCheck]); 6] = [
        (
            "the whole target, unmoved",
            viewport(ONE_TO_ONE),
            &[
                (0, 0, RED_PX, 1, "the first pixel of the square"),
                (9, 9, RED_PX, 1, "the last pixel of the square"),
                (10, 5, WHITE_PX, 1, "the column beyond the square"),
                (5, 10, WHITE_PX, 1, "the row beneath the square"),
            ],
            &[],
        ),
        (
            "an origin half a pixel to the right",
            viewport(at(0.5, 0.0)),
            &[
                (0, 5, half_red, 4, "column 0, half covered"),
                (1, 5, RED_PX, 1, "column 1, the first solid column"),
                (9, 5, RED_PX, 1, "column 9, the last solid column"),
                (10, 5, half_red, 4, "column 10, half covered"),
                (11, 5, WHITE_PX, 1, "column 11, untouched"),
                (5, 9, RED_PX, 1, "row 9, unmoved"),
                (5, 10, WHITE_PX, 1, "row 10, untouched"),
            ],
            &[],
        ),
        (
            "two pixels per point",
            Viewport::whole([200, 200], 2.0, ONE_TO_ONE),
            &[
                (0, 0, RED_PX, 1, "the first pixel of the square"),
                (
                    19,
                    19,
                    RED_PX,
                    1,
                    "the last pixel of the square, twenty pixels across",
                ),
                (20, 10, WHITE_PX, 1, "the column beyond the square"),
                (10, 20, WHITE_PX, 1, "the row beneath the square"),
            ],
            &[],
        ),
        (
            "a clip at fractional points",
            clipped_to(2.3, 1.6, 5.0, 6.0),
            &[
                (
                    2,
                    4,
                    RED_PX,
                    1,
                    "the first column inside the clip, painted whole",
                ),
                (6, 4, RED_PX, 1, "the last column inside the clip"),
                (
                    4,
                    2,
                    RED_PX,
                    1,
                    "the first row inside the clip, painted whole",
                ),
                (4, 7, RED_PX, 1, "the last row inside the clip"),
                // The pixels beyond the ones that straddle the rounded edges, which no reading of the rounding
                // reaches; each lies inside the square, so only the scissor can leave it bare.
                (0, 4, WHITE_PX, 1, "the second column before the clip"),
                (8, 4, WHITE_PX, 1, "the second column after the clip"),
                (4, 0, WHITE_PX, 1, "the second row above the clip"),
                (4, 9, WHITE_PX, 1, "the second row beneath the clip"),
            ],
            &[
                (1, 4, "the column before the clip"),
                (7, 4, "the column after the clip"),
                (4, 1, "the row above the clip"),
                (4, 8, "the row beneath the clip"),
            ],
        ),
        (
            "a clip above and left of the target",
            clipped_to(-50.0, -50.0, 40.0, 40.0),
            &[
                (0, 0, WHITE_PX, 1, "the first pixel, undrawn"),
                (5, 5, WHITE_PX, 1, "the middle of the square, undrawn"),
            ],
            &[],
        ),
        (
            "a clip beyond the right edge of the target",
            clipped_to(150.0, 0.0, 40.0, 40.0),
            &[
                (5, 5, WHITE_PX, 1, "the middle of the square, undrawn"),
                (99, 5, WHITE_PX, 1, "the last column, undrawn"),
            ],
            &[],
        ),
    ];
    for (what, target, pixels, straddling) in cases {
        let image = renderer
            .render_list(&list, &target, WHITE_PX)
            .unwrap_or_else(|error| panic!("{what}: {error}"));
        assert_eq!(
            [image.width, image.height],
            target.size_px,
            "{what}: the image has the target's size"
        );
        for &(x, y, expected, tolerance, which) in pixels {
            assert_pixel(
                &image,
                x,
                y,
                expected,
                tolerance,
                &format!("{which} with {what}"),
            );
        }
        for &(x, y, which) in straddling {
            assert_outside_the_scissor(
                &image,
                x,
                y,
                RED_PX,
                WHITE_PX,
                &format!("{which} with {what}"),
            );
        }
    }
}
