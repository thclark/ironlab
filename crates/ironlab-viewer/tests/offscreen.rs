//! Headless rendering through the viewer's own mesh pipeline.
//!
//! These tests need a wgpu adapter. When none is available they print a message and pass, unless the environment
//! variable `IRONLAB_REQUIRE_GPU` is set (as in CI, which installs a software Vulkan adapter), in which case a missing
//! adapter fails the test.
//!
//! The image tests draw hand-built image items magnified so that every image pixel spans many device pixels, and
//! sample device pixels at the centres of image pixels and one device pixel either side of their boundaries, so that
//! what is asserted is the colour of the pixels and the hardness of their edges rather than the anti-aliasing of the
//! quad that carries them.

mod common;

use std::sync::Arc;

use common::{
    TEXT, depth_group, figure_with_mapped_image, figure_with_surface, find_image, gpu_required,
    image_sample, rendered_or_skip, scale_then_translate,
};
use ironlab_ir::{Artist, Axes, Axis, DataId, Figure, FigureSize, Limits, Line, NdArray, NodeId};
use ironlab_scene::display::{
    Depth, DepthPlane, DisplayList, Fill, FillRule, ImageItem, Item, ItemKind, LineCap, LineJoin,
    PathItem, PathSegment, Point, Rect, Rgba, Stroke, Transform,
};
use ironlab_scene::maths::camera::FACE_DEPTH_BIAS;
use ironlab_viewer::{RenderError, RenderedImage, render_display_list_offscreen, render_offscreen};

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

// Why: egui does not anti-alias custom meshes, so the renderer uses 4× MSAA; without it every edge in the gallery
// images would be a hard staircase. Without multisampling an opaque black fill on white produces only pure black and
// pure white pixels, so the presence of intermediate shades along an edge proves that coverage was sampled more than
// once per pixel. The edge has an irrational-looking slope and fractional end points so that it crosses pixels at
// every sub-pixel offset; the assertion only asks for intermediate shades in a quarter of the columns, which holds for
// any 4× sample pattern rather than counting pixels produced by one particular rasteriser.
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
// translucent colours over what lies beneath. A renderer that reordered meshes (for example batching by colour),
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
// colour and the boundary between two pixels must be a hard step. egui's renderer filters textures bilinearly in its
// own shader when it is asked for predictable filtering, whatever the texture's sampler says, which would smear a
// 2 × 2 image into a gradient; the renderer must leave that off and the texture must ask for nearest sampling. Each
// image pixel is magnified to 20 device pixels, so a bilinear blend would be visible over most of the pixel, and the
// samples one device pixel either side of a boundary would differ from the pure colours by half their contrast.
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
// and a half-transparent one is a straight-alpha blend with it. A renderer that uploaded straight alpha where egui
// expects premultiplied would draw the translucent pixel too bright, and one that ignored alpha would paint the
// transparent pixel opaque.
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
// exported PNG. The image sits in a clipped group, as it does beneath an axes, so that tile quads reaching far off
// the page are trimmed by the clipper before they reach the GPU. The side at which the renderer tiles is not pinned
// here.
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

/// `items` with the depth removed from every path and image, at any depth of grouping, so that the mesh path draws
/// what the GPU path drew with the depths in place.
fn without_depths(items: Vec<Item>) -> Vec<Item> {
    items
        .into_iter()
        .map(|mut item| {
            match &mut item.kind {
                ItemKind::Path(path) => path.depth = None,
                ItemKind::Image(image) => image.depth = None,
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
#[test]
fn crossing_faces_in_a_depth_group_show_the_nearer_one_and_in_a_plain_group_the_later_one() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    // Both cover (10, 10) to (90, 90); the red face is nearer on the right and the blue one on the left, and they
    // cross at x = 50.
    let faces = || {
        vec![
            face(
                10.0,
                10.0,
                90.0,
                90.0,
                red,
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
                blue,
                DepthPlane {
                    a: -0.01,
                    b: 0.0,
                    c: 1.0,
                },
            ),
        ]
    };
    let Some(tested) = render_page(Rgba::WHITE, vec![depth_group(faces())]) else {
        return;
    };
    let Some(painted) = render_page(Rgba::WHITE, vec![plain_group(faces())]) else {
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
// gamma space, so that the GPU path and the mesh path composite alike; a pipeline without blending, or one blending
// straight alpha, would paint the near face opaque or too bright.
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

    // Half-transparent red over blue composites to (128, 0, 128), as the mesh path composites half-transparent red
    // over white to (255, 128, 128).
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

// Why: outside a depth group a depth-carrying leaf is drawn by the GPU path without the depth test and a depthless
// one by the mesh path, and the exporter and the interactive canvas each use both for one figure; the two paths
// must agree pixel for pixel in their mapping, anti-aliasing, blending and texture sampling, or the box of an axes
// would not meet the surface inside it. The squares have fractional edges so that the anti-aliasing is compared
// too. The image is left unclipped: a fractional clip edge is where the paths differ, the mesh path clipping a quad
// geometrically and the GPU path scissoring to whole pixels.
#[test]
fn the_untested_gpu_path_draws_the_same_bytes_as_the_mesh_path() {
    let items = vec![
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
    ];
    let Some(gpu) = render_page(Rgba::WHITE, items.clone()) else {
        return;
    };
    let Some(mesh) = render_page(Rgba::WHITE, without_depths(items)) else {
        return;
    };

    assert_pixel(&gpu, 20, 20, RED_PX, 2, "the opaque square");
    // Half-transparent blue over red composites to about (128, 0, 128).
    assert_pixel(
        &gpu,
        40,
        40,
        [128, 0, 128, 255],
        2,
        "the translucent square over the opaque one",
    );
    assert_pixel(&gpu, 60, 60, RED_PX, 1, "the image's top-left pixel");
    assert_pixel(&gpu, 80, 80, YELLOW_PX, 1, "the image's bottom-right pixel");
    let differing: Vec<(u32, u32, [u8; 4], [u8; 4])> = (0..gpu.height)
        .flat_map(|y| (0..gpu.width).map(move |x| (x, y)))
        .filter(|&(x, y)| gpu.pixel(x, y) != mesh.pixel(x, y))
        .map(|(x, y)| (x, y, gpu.pixel(x, y), mesh.pixel(x, y)))
        .take(8)
        .collect();
    assert!(
        gpu.rgba == mesh.rgba,
        "the GPU path and the mesh path draw identical bytes; the first pixels that differ, as (x, y, GPU, mesh): \
         {differing:?}"
    );
}

// Why: an image with alpha (a NaN region left transparent, a fade at the edge of a disc) inside a 3D axes goes
// through the depth pipeline's own texture upload, which must premultiply the straight alpha of a four-channel tile
// as the mesh path's upload does; a tile uploaded straight would blend too bright, and one that ignored the fourth
// channel would paint the transparent pixels opaque over the face beneath.
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

    // Half-transparent red over blue composites to about (128, 0, 128), as the mesh path composites it.
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
// pipelines is checked on the drawables in `canvas.rs`.
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
