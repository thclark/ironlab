//! Headless rendering through the viewer's own mesh pipeline.
//!
//! These tests need a wgpu adapter. When none is available they print a message and pass, unless the environment
//! variable `IRONLAB_REQUIRE_GPU` is set (as in CI, which installs a software Vulkan adapter), in which case a missing
//! adapter fails the test.

mod common;

use common::TEXT;
use ironlab_ir::{Artist, Axes, Axis, DataId, Figure, FigureSize, Limits, Line, NdArray, NodeId};
use ironlab_scene::display::{
    DisplayList, Fill, FillRule, Item, ItemKind, PathItem, PathSegment, Point, Rgba,
};
use ironlab_viewer::{RenderError, RenderedImage, render_display_list_offscreen, render_offscreen};

fn gpu_required() -> bool {
    std::env::var_os("IRONLAB_REQUIRE_GPU").is_some()
}

/// Unwraps a render result, or returns `None` (skipping the test) when no adapter is available and a GPU is not
/// required.
fn rendered_or_skip(result: Result<RenderedImage, RenderError>) -> Option<RenderedImage> {
    match result {
        Ok(image) => Some(image),
        Err(RenderError::NoAdapter(message)) if !gpu_required() => {
            eprintln!(
                "skipping: no graphics adapter ({message}); set IRONLAB_REQUIRE_GPU to make this a failure"
            );
            None
        }
        Err(error) => panic!("offscreen rendering failed: {error}"),
    }
}

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
