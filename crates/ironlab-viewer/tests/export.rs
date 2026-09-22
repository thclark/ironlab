//! PDF export with the dense parts of a figure rasterised by the viewer's own renderer.
//!
//! The PDF crate's own tests drive the exporter with a stub rasteriser, because its geometry must be checked exactly.
//! These tests are the other half: they run the real pipeline, from a figure through the scene compiler, the viewer's
//! tessellation and its headless GPU device, into an embedded image, and they check the only property that matters
//! about that pipeline — that the picture does not change when the exporter switches from vector paths to pixels.
//!
//! The image test makes the same claim for an image artist: the pixels the exporter embeds are the data's own and
//! land where the viewer draws them, so the raster of the exported PDF is compared with the viewer's own rendering of
//! the same display list.
//!
//! They need a wgpu adapter, and a rasterised page needs poppler to inspect. A missing adapter fails the test only
//! when `IRONLAB_REQUIRE_GPU` is set, and a missing tool only when `IRONLAB_REQUIRE_PDF_TOOLS` is set, as both are in
//! CI.

mod common;

use std::path::Path;
use std::process::Command;

use common::{
    BLOCK_PT, BLOCK_TOLERANCE, TEXT, Workspace, assert_same_picture, difference,
    figure_with_mapped_image, find_image, gpu_required, rasterise, rendered_or_skip, rgb_of, run,
    tools_available,
};
use image::RgbImage;
use ironlab_ir::{
    Artist, Axes, Axis, Cell, Color, ColorSpec, DataId, Figure, Grid, Limits, Line, NdArray,
    NodeId, Projection, Surface, Text, TileLayout, View3d,
};
use ironlab_pdf::{
    DepthPolicy, ExportWarning, ExportWarningKind, PdfOptions, RasterOptions, RasterPolicy,
    UnverifiedCause,
};
use ironlab_scene::display::Rect;
use ironlab_viewer::{ExportError, RenderError, render_display_list_offscreen};

/// The side of the grid of the dense surface. It has 139 × 139 = 19 321 faces, comfortably above the default
/// threshold, so the default settings rasterise it without being told to.
const DENSE_SIDE: usize = 140;

/// Unwraps an export, or returns `None` (skipping the test) when no adapter is available and a GPU is not required.
fn exported_or_skip<T>(result: Result<T, ExportError>) -> Option<T> {
    match result {
        Ok(exported) => Some(exported),
        Err(ExportError::Render(RenderError::NoAdapter(message))) if !gpu_required() => {
            eprintln!(
                "skipping: no graphics adapter ({message}); set IRONLAB_REQUIRE_GPU to make this a failure"
            );
            None
        }
        Err(error) => panic!("export failed: {error}"),
    }
}

/// The `(width, height)` in samples of every image embedded in a PDF, ignoring soft masks.
fn embedded_images(pdf: &Path) -> Vec<(u32, u32)> {
    run(Command::new("pdfimages").arg("-list").arg(pdf))
        .lines()
        .skip(2)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split_whitespace().map(str::to_owned).collect())
        .filter(|t: &Vec<String>| t[2] == "image")
        .map(|t| {
            (
                t[3].parse().expect("image width"),
                t[4].parse().expect("image height"),
            )
        })
        .collect()
}

fn pdftotext(pdf: &Path) -> String {
    run(Command::new("pdftotext").arg(pdf).arg("-"))
}

/// The mean absolute difference per channel between two rasters, with `b` shifted by `(dx, dy)` pixels and only the
/// region the two then have in common compared.
fn shifted_difference(a: &RgbImage, b: &RgbImage, dx: i64, dy: i64) -> f64 {
    let (width, height) = a.dimensions();
    let (x0, x1) = ((-dx).max(0) as u32, width - dx.unsigned_abs() as u32);
    let (y0, y1) = ((-dy).max(0) as u32, height - dy.unsigned_abs() as u32);
    let mut total = 0u64;
    let mut count = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = a.get_pixel(x, y).0;
            let q = b
                .get_pixel((x as i64 + dx) as u32, (y as i64 + dy) as u32)
                .0;
            total += (0..3).map(|c| u64::from(p[c].abs_diff(q[c]))).sum::<u64>();
            count += 3;
        }
    }
    total as f64 / count as f64
}

/// Asserts that the raster is in register with the paths it replaces to better than `offset_pt` points.
///
/// The two pages are compared as they are and with the raster shifted by one step left, right, up and down. If the
/// raster were displaced, one of the shifts would undo part of that displacement and agree with the paths better
/// than no shift at all. This compares like with like — every measurement carries the same anti-aliasing noise — so
/// it detects a misplacement far smaller than a tolerance on the difference itself could.
fn assert_registered(vector: &RgbImage, raster: &RgbImage, dpi: f64, offset_pt: f64) {
    let step = (offset_pt * dpi / 72.0).round().max(1.0) as i64;
    let here = shifted_difference(vector, raster, 0, 0);
    for (dx, dy) in [(step, 0), (-step, 0), (0, step), (0, -step)] {
        let shifted = shifted_difference(vector, raster, dx, dy);
        assert!(
            shifted > here,
            "the raster agrees with the paths better when shifted by ({dx}, {dy}) pixels \
             ({shifted:.2}) than where it was drawn ({here:.2}), so it is out of register"
        );
    }
}

/// Options with the given dense policy and resolution, and the three-dimensional axes drawn as vectors in painter's
/// order, so that the dense comparisons measure the dense fallback alone.
fn options(policy: RasterPolicy, dpi: f64) -> PdfOptions {
    PdfOptions {
        raster: RasterOptions {
            policy,
            dpi,
            depth: DepthPolicy::Vector,
        },
        ..PdfOptions::default()
    }
}

/// Samples `f` on a `side` by `side` grid over `[0, 1]²` and returns the arrays of a surface over it.
fn field(side: usize, f: impl Fn(f64, f64) -> f64) -> (Vec<f64>, Vec<f64>) {
    let axis: Vec<f64> = (0..side).map(|i| i as f64 / (side - 1) as f64).collect();
    let values = axis
        .iter()
        .flat_map(|&y| axis.iter().map(move |&x| (x, y)))
        .map(|(x, y)| f(x, y))
        .collect();
    (axis, values)
}

/// A figure of one surface of `side` by `side` nodes, two-dimensional or three-dimensional, titled and labelled so
/// that the text around the surface can be looked for in the exported PDF.
fn surface_figure(side: usize, projection: Projection, edge: ColorSpec) -> Figure {
    // A field of alternating blocks about eight faces across, rather than a smooth one. Its edges are what make a
    // misplaced or rescaled raster measurable: a smooth field looks almost the same wherever it is put.
    surface_figure_of(side, projection, edge, |x, y| {
        f64::from(((x * 12.0) as i32 + (y * 12.0) as i32) % 2 == 0)
    })
}

/// [`surface_figure`] over the field `f`.
fn surface_figure_of(
    side: usize,
    projection: Projection,
    edge: ColorSpec,
    f: impl Fn(f64, f64) -> f64,
) -> Figure {
    let (axis, values) = field(side, f);
    let (gx, gy, z) = (DataId(0), DataId(1), DataId(2));
    let data = std::collections::BTreeMap::from([
        (gx, NdArray::vector(axis.clone())),
        (gy, NdArray::vector(axis)),
        (
            z,
            NdArray::from_shape(vec![side, side], values).expect("the shape matches the values"),
        ),
    ]);
    Figure {
        id: NodeId(1),
        layout: TileLayout { rows: 1, cols: 1 },
        data,
        axes: vec![Axes {
            id: NodeId(2),
            cell: Cell::default(),
            projection,
            title: Some(Text::plain("Amplitude")),
            x: Axis {
                label: Some(Text::plain("Streamwise")),
                ..Axis::default()
            },
            y: Axis {
                label: Some(Text::plain("Spanwise")),
                ..Axis::default()
            },
            z: Axis {
                limits: Limits::Manual { min: 0.0, max: 1.0 },
                ..Axis::default()
            },
            artists: vec![Artist::Surface(Surface {
                id: NodeId(3),
                grid: Grid::Rectilinear { x: gx, y: gy },
                z,
                edge,
                ..Surface::default()
            })],
            ..Axes::default()
        }],
        ..Figure::new()
    }
}

/// Exports one figure twice, as vectors and as a raster at `dpi`, and returns the two rasterised pages.
fn vector_and_raster_pages(
    ws: &Workspace,
    figure: &Figure,
    dpi: f64,
) -> Option<(RgbImage, RgbImage)> {
    let vector = exported_or_skip(ironlab_viewer::export_pdf(
        figure,
        &TEXT,
        &options(RasterPolicy::Never, dpi),
    ))?;
    let raster = exported_or_skip(ironlab_viewer::export_pdf(
        figure,
        &TEXT,
        &options(RasterPolicy::Always, dpi),
    ))?;
    let vector = ws.write("vector", &vector.bytes);
    let raster = ws.write("raster", &raster.bytes);
    assert!(
        embedded_images(&vector).is_empty(),
        "the vector export embeds no image"
    );
    assert_eq!(
        embedded_images(&raster).len(),
        1,
        "the raster export embeds exactly one image"
    );
    Some((rasterise(&vector, dpi), rasterise(&raster, dpi)))
}

// WHY: this is the claim the whole change rests on. The raster is produced by the viewer's pipeline and the vector
// paths by the exporter, so if the two agree on a surface of ten thousand faces — in position, in scale, in colour
// and in what the plot box clips away — then turning rasterisation on has not changed the figure, and the export has
// not drifted from the screen. Nothing weaker would catch a raster half a pixel out, at the wrong resolution, or
// with its colours run through a second, differently rounded colour map.
#[test]
fn a_rasterised_2d_surface_is_the_same_picture_as_the_paths_it_replaces() {
    if !tools_available(&["pdfimages", "pdftoppm"]) {
        return;
    }
    let ws = Workspace::new("surface-2d");
    // The faces are drawn without edges, so that the comparison measures where the raster is, how large it is and
    // what colours it holds, rather than how two rasterisers each anti-alias a grid of ten thousand hairlines.
    let figure = surface_figure(DENSE_SIDE, Projection::TwoD, ColorSpec::None);
    let Some((vector, raster)) = vector_and_raster_pages(&ws, &figure, 300.0) else {
        return;
    };

    assert_registered(&vector, &raster, 300.0, 1.0);
    assert_same_picture(
        &vector,
        &raster,
        300.0,
        "the rasterised two-dimensional surface and the paths it replaces",
    );
}

// WHY: in a three-dimensional axes the surface is a projection, its faces are sorted back to front, and its geometry
// is not axis-aligned, so the rectangle the image occupies is derived rather than given. A bound that clipped the
// projected faces, or that failed to cover them, would show here and nowhere else.
#[test]
fn a_rasterised_3d_surface_is_the_same_picture_as_the_paths_it_replaces() {
    if !tools_available(&["pdfimages", "pdftoppm"]) {
        return;
    }
    let ws = Workspace::new("surface-3d");
    let figure = surface_figure(
        DENSE_SIDE,
        Projection::ThreeD {
            view3d: View3d::default(),
        },
        ColorSpec::None,
    );
    let Some((vector, raster)) = vector_and_raster_pages(&ws, &figure, 300.0) else {
        return;
    };

    assert_registered(&vector, &raster, 300.0, 1.0);
    assert_same_picture(
        &vector,
        &raster,
        300.0,
        "the rasterised three-dimensional surface and the paths it replaces",
    );
}

// WHY: the reason to export a figure as a PDF rather than an image is that its text is text and its lines are lines.
// Rasterising the surface must therefore cost only the surface: the title, the axis labels and the tick labels must
// still be selectable, their fonts still embedded, and the one image must cover the plot box rather than the page.
#[test]
fn only_the_surface_is_rasterised_and_the_text_around_it_stays_selectable() {
    if !tools_available(&["pdfimages", "pdftotext"]) {
        return;
    }
    let ws = Workspace::new("furniture");
    let figure = surface_figure(DENSE_SIDE, Projection::TwoD, ColorSpec::default());
    // The default options, with no policy set, to show that a dense surface rasterises without being asked.
    let Some(exported) = exported_or_skip(ironlab_viewer::export_pdf(
        &figure,
        &TEXT,
        &PdfOptions::for_figure(&figure),
    )) else {
        return;
    };
    let pdf = ws.write("figure", &exported.bytes);

    let images = embedded_images(&pdf);
    assert_eq!(images.len(), 1, "the surface alone became an image");
    let page_px = |mm: f64| (mm * 72.0 / 25.4 * ironlab_pdf::DEFAULT_RASTER_DPI / 72.0) as u32;
    assert!(
        images[0].0 < page_px(figure.size.width_mm) && images[0].1 < page_px(figure.size.height_mm),
        "the image covers the plot box, not the whole page: {images:?}"
    );

    let text = pdftotext(&pdf);
    for expected in ["Amplitude", "Streamwise", "Spanwise"] {
        assert!(
            text.contains(expected),
            "{expected:?} is still selectable text; the PDF holds {text:?}"
        );
    }
}

// WHY: exporting must not start to require a graphics adapter. A figure with nothing dense in it has always been
// written from vector paths alone, and it must still be written on a machine that cannot render at all, which is
// exactly what a build server or a container without a GPU is.
#[test]
fn a_figure_with_nothing_dense_exports_without_a_renderer() {
    let figure = surface_figure(4, Projection::TwoD, ColorSpec::default());
    let exported = ironlab_viewer::export_pdf(&figure, &TEXT, &PdfOptions::default())
        .expect("a figure with no dense content exports without any renderer");

    assert!(
        exported.bytes.starts_with(b"%PDF"),
        "the exported bytes are a PDF document"
    );
    assert!(
        exported.warnings.is_empty(),
        "nothing was left off the page: {:?}",
        exported.warnings
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------------------------------------------

/// The inset, in points, by which the plot rectangle is shrunk before its interior is compared: enough to leave out
/// the box of the axes and the tick marks that point into the plot, which are at most half the font size long.
const PLOT_INSET_PT: f64 = 8.0;

/// The part of a raster of the page, taken at `dpi`, that lies inside the plot rectangle `plot` (in points) inset by
/// [`PLOT_INSET_PT`] on every side.
fn plot_interior(raster: &RgbImage, plot: Rect, dpi: f64) -> RgbImage {
    let px = |pt: f64| (pt * dpi / 72.0).round() as u32;
    let (x0, y0) = (px(plot.x + PLOT_INSET_PT), px(plot.y + PLOT_INSET_PT));
    let (x1, y1) = (
        px(plot.right() - PLOT_INSET_PT),
        px(plot.bottom() - PLOT_INSET_PT),
    );
    image::imageops::crop_imm(raster, x0, y0, x1 - x0, y1 - y0).to_image()
}

// WHY: an image artist reaches the PDF as an image XObject of the data's own pixels, one sample each, beneath the
// transform that places it in the axes, and reaches the screen as a textured quad from the same display list; the
// two must be the same picture. An exporter that resampled the image to the page resolution, placed it in PDF's y-up
// space, flipped its rows or scaled it to the wrong rectangle would pass the compiler's tests and still print
// something other than what the viewer shows. The comparison is confined to the plot interior, which holds nothing
// but the image, because the tick labels around it are drawn by two different text rasterisers whose glyphs need not
// agree pixel for pixel.
#[test]
fn a_mapped_image_is_embedded_at_its_data_resolution_and_prints_as_the_viewer_draws_it() {
    if !tools_available(&["pdfimages", "pdftoppm"]) {
        return;
    }
    let ws = Workspace::new("mapped-image");
    let (ny, nx) = (4usize, 6usize);
    let figure = figure_with_mapped_image(ny, nx);
    let scene = ironlab_scene::compile(&figure, &TEXT);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let item = find_image(&scene.display_list.items)
        .expect("the compiler emits an image item for the mapped image");
    assert_eq!((item.width, item.height), (nx as u32, ny as u32));
    // Twice the resolution the surface tests compare at, so that an image pixel spans well over a hundred device
    // pixels and a boundary placed a device pixel apart by the two rasterisers is a small part of every block.
    let dpi = 288.0;
    // An image is never dense, so the export needs no renderer; only the comparison does.
    let Some(rendered) = exported_or_skip(ironlab_viewer::export::render_display_list(
        &scene.display_list,
        &TEXT,
        &PdfOptions::for_figure(&figure),
    )) else {
        return;
    };
    let pdf = ws.write("figure", &rendered.bytes);
    assert_eq!(
        embedded_images(&pdf),
        vec![(nx as u32, ny as u32)],
        "the PDF holds one image, of the data's own pixel dimensions"
    );

    let Some(rendered) = rendered_or_skip(render_display_list_offscreen(
        &scene.display_list,
        &TEXT,
        dpi,
    )) else {
        return;
    };
    let plot = scene.hit_map.axes[0].plot_rect;
    let printed = plot_interior(&rasterise(&pdf, dpi), plot, dpi);
    let drawn = plot_interior(&rgb_of(&rendered), plot, dpi);
    let (width, height) = drawn.dimensions();
    assert!(
        drawn.get_pixel(0, 0) != drawn.get_pixel(width - 1, height - 1)
            && drawn.get_pixel(0, 0).0 != [255, 255, 255],
        "the viewer draws the ramp of the image across the plot interior"
    );
    assert_registered(&printed, &drawn, dpi, 2.0);
    assert_same_picture(
        &printed,
        &drawn,
        dpi,
        "the printed image and the viewer's drawing of it",
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Three-dimensional axes
// ---------------------------------------------------------------------------------------------------------------

/// Options with the given dense and depth policies at `dpi`.
fn depth_options(policy: RasterPolicy, depth: DepthPolicy, dpi: f64) -> PdfOptions {
    PdfOptions {
        raster: RasterOptions { policy, dpi, depth },
        ..PdfOptions::default()
    }
}

/// The node and kind of every export warning, in order.
fn kinds(warnings: &[ExportWarning]) -> Vec<(Option<NodeId>, ExportWarningKind)> {
    warnings.iter().map(|w| (w.node, w.kind.clone())).collect()
}

/// A three-dimensional figure of two planes that cut through each other along the diagonal `x + y = 1`, which runs
/// through the middle of the faces rather than along their edges: `z = 0.2 + 0.6x` coloured through the colormap
/// and `z = 0.8 − 0.6y` in grey, with a line from `(0, 0, 0.1)` to `(1, 1, 0.9)` passing through both. The axes is
/// node 2, the planes 3 and 4 and the line 5.
fn crossing_planes_figure() -> Figure {
    let side = 7;
    let (axis, rising) = field(side, |x, _| 0.2 + 0.6 * x);
    let (_, falling) = field(side, |_, y| 0.8 - 0.6 * y);
    let (gx, gy, z1, z2, lx, ly, lz) = (
        DataId(0),
        DataId(1),
        DataId(2),
        DataId(3),
        DataId(4),
        DataId(5),
        DataId(6),
    );
    let matrix = |values| NdArray::from_shape(vec![side, side], values).expect("a square field");
    let data = std::collections::BTreeMap::from([
        (gx, NdArray::vector(axis.clone())),
        (gy, NdArray::vector(axis)),
        (z1, matrix(rising)),
        (z2, matrix(falling)),
        (lx, NdArray::vector(vec![0.0, 1.0])),
        (ly, NdArray::vector(vec![0.0, 1.0])),
        (lz, NdArray::vector(vec![0.1, 0.9])),
    ]);
    Figure {
        id: NodeId(1),
        layout: TileLayout { rows: 1, cols: 1 },
        data,
        axes: vec![Axes {
            id: NodeId(2),
            cell: Cell::default(),
            projection: Projection::ThreeD {
                view3d: View3d::default(),
            },
            z: Axis {
                limits: Limits::Manual { min: 0.0, max: 1.0 },
                ..Axis::default()
            },
            artists: vec![
                Artist::Surface(Surface {
                    id: NodeId(3),
                    grid: Grid::Rectilinear { x: gx, y: gy },
                    z: z1,
                    edge: ColorSpec::None,
                    ..Surface::default()
                }),
                Artist::Surface(Surface {
                    id: NodeId(4),
                    grid: Grid::Rectilinear { x: gx, y: gy },
                    z: z2,
                    face: ColorSpec::Rgba {
                        color: Color::rgb(0.8, 0.8, 0.8),
                    },
                    edge: ColorSpec::None,
                    ..Surface::default()
                }),
                Artist::Line(Line {
                    id: NodeId(5),
                    x: lx,
                    y: ly,
                    z: Some(lz),
                    ..Line::default()
                }),
            ],
            ..Axes::default()
        }],
        ..Figure::new()
    }
}

// WHY: the default policy must keep a figure whose painter's order shows what the viewer shows as vectors, or every
// three-dimensional figure would become an image; and the vectors it writes must be the picture the depth-tested
// raster would have been, which is what the two renders of the verification compare. A smooth height field with
// edges is the commonest such figure.
#[test]
fn a_height_field_under_the_default_policy_stays_vector_and_matches_its_depth_tested_raster() {
    if !tools_available(&["pdfimages", "pdftoppm"]) {
        return;
    }
    let ws = Workspace::new("height-field");
    let figure = surface_figure_of(
        20,
        Projection::ThreeD {
            view3d: View3d::default(),
        },
        ColorSpec::default(),
        |x, y| 0.5 + 0.3 * (std::f64::consts::TAU * x).sin() * (std::f64::consts::TAU * y).cos(),
    );
    let dpi = 300.0;
    let Some(vector) = exported_or_skip(ironlab_viewer::export_pdf(
        &figure,
        &TEXT,
        &depth_options(RasterPolicy::Never, DepthPolicy::Auto, dpi),
    )) else {
        return;
    };
    let Some(raster) = exported_or_skip(ironlab_viewer::export_pdf(
        &figure,
        &TEXT,
        &depth_options(RasterPolicy::Never, DepthPolicy::Raster, dpi),
    )) else {
        return;
    };
    assert!(
        vector.export.is_empty(),
        "the verified axes is written as vectors with nothing to warn of: {:?}",
        vector.export
    );
    assert_eq!(
        kinds(&raster.export),
        vec![(Some(NodeId(2)), ExportWarningKind::RasterisedByRequest)],
        "forcing an image is reported as asked for"
    );
    let vector = ws.write("vector", &vector.bytes);
    let raster = ws.write("raster", &raster.bytes);
    assert!(
        embedded_images(&vector).is_empty(),
        "the verified export embeds no image"
    );
    assert_eq!(
        embedded_images(&raster).len(),
        1,
        "the forced export embeds one image"
    );
    let (vector, raster) = (rasterise(&vector, dpi), rasterise(&raster, dpi));
    assert_registered(&vector, &raster, dpi, 1.0);
    assert_same_picture(
        &vector,
        &raster,
        dpi,
        "the verified vectors and the depth-tested raster",
    );
}

// WHY: two surfaces that cut through each other cannot be painted back to front, so the default policy must embed
// the depth-tested render and say why; forcing vectors must draw the painter's order, which differs visibly around
// the crossing; and what the embedded image shows must be what the viewer draws, which is the golden rule.
#[test]
fn crossing_surfaces_under_the_default_policy_are_embedded_and_reported() {
    if !tools_available(&["pdfimages", "pdftoppm"]) {
        return;
    }
    let ws = Workspace::new("crossing");
    let figure = crossing_planes_figure();
    let dpi = 300.0;
    let Some(auto) = exported_or_skip(ironlab_viewer::export_pdf(
        &figure,
        &TEXT,
        &depth_options(RasterPolicy::Never, DepthPolicy::Auto, dpi),
    )) else {
        return;
    };
    let Some(vector) = exported_or_skip(ironlab_viewer::export_pdf(
        &figure,
        &TEXT,
        &depth_options(RasterPolicy::Never, DepthPolicy::Vector, dpi),
    )) else {
        return;
    };
    assert_eq!(
        kinds(&auto.export),
        vec![(Some(NodeId(2)), ExportWarningKind::RasterisedForDepth)],
        "the axes is reported as drawn as an image for its depth"
    );
    assert!(
        auto.export[0].message.contains("image"),
        "the message says what happened: {}",
        auto.export[0].message
    );
    assert_eq!(
        kinds(&vector.export),
        vec![(
            Some(NodeId(2)),
            ExportWarningKind::Unverified {
                cause: UnverifiedCause::PolicyVector
            }
        )],
        "forcing vectors is reported as unverified by request"
    );
    let auto_pdf = ws.write("auto", &auto.bytes);
    let vector_pdf = ws.write("vector", &vector.bytes);
    assert_eq!(
        embedded_images(&auto_pdf).len(),
        1,
        "the default export embeds exactly one image"
    );
    assert!(
        embedded_images(&vector_pdf).is_empty(),
        "the forced vector export embeds none"
    );

    let (auto_page, vector_page) = (rasterise(&auto_pdf, dpi), rasterise(&vector_pdf, dpi));
    let block = (BLOCK_PT * dpi / 72.0).round() as u32;
    let (_, worst) = difference(&vector_page, &auto_page, block);
    assert!(
        worst > BLOCK_TOLERANCE,
        "the painter's order differs visibly from the depth-tested picture around the crossing (worst block \
         {worst}, tolerance {BLOCK_TOLERANCE})"
    );

    let scene = ironlab_scene::compile(&figure, &TEXT);
    let Some(drawn) = rendered_or_skip(render_display_list_offscreen(
        &scene.display_list,
        &TEXT,
        dpi,
    )) else {
        return;
    };
    let plot = scene.hit_map.axes[0].plot_rect;
    let printed = plot_interior(&auto_page, plot, dpi);
    let drawn = plot_interior(&rgb_of(&drawn), plot, dpi);
    assert_registered(&printed, &drawn, dpi, 1.0);
    assert_same_picture(
        &printed,
        &drawn,
        dpi,
        "the embedded image and the viewer's drawing of the axes",
    );
}

// WHY: a figure of two-dimensional axes reaches the page as vectors and needs no verification, so its report must
// say nothing from the exporter, or every program checking the report would have to filter it.
#[test]
fn a_two_dimensional_figure_reports_nothing_from_the_exporter() {
    let figure = surface_figure(4, Projection::TwoD, ColorSpec::default());
    let exported = ironlab_viewer::export_pdf(&figure, &TEXT, &PdfOptions::default())
        .expect("a two-dimensional figure exports without a renderer");
    assert!(
        exported.export.is_empty(),
        "nothing to report: {:?}",
        exported.export
    );
}

// WHY: the dense fallback and the depth policy are separate decisions: a surface too large for vectors is still
// drawn as an image for its size inside an axes drawn as vectors, and the report must give both reasons, the axes
// first because it encloses the surface.
#[test]
fn a_dense_surface_in_an_axes_drawn_as_vectors_is_rasterised_for_its_size_with_both_warnings() {
    if !tools_available(&["pdfimages"]) {
        return;
    }
    let ws = Workspace::new("dense-in-vector-axes");
    let figure = surface_figure(
        DENSE_SIDE,
        Projection::ThreeD {
            view3d: View3d::default(),
        },
        ColorSpec::None,
    );
    let Some(exported) = exported_or_skip(ironlab_viewer::export_pdf(
        &figure,
        &TEXT,
        &depth_options(RasterPolicy::default(), DepthPolicy::Vector, 300.0),
    )) else {
        return;
    };
    let faces = ((DENSE_SIDE - 1) * (DENSE_SIDE - 1)) as u64;
    assert_eq!(
        kinds(&exported.export),
        vec![
            (
                Some(NodeId(2)),
                ExportWarningKind::Unverified {
                    cause: UnverifiedCause::PolicyVector
                }
            ),
            (
                Some(NodeId(3)),
                ExportWarningKind::RasterisedForSize { cells: faces }
            ),
        ],
        "the axes is unverified by request and the surface is an image for its size"
    );
    let pdf = ws.write("figure", &exported.bytes);
    assert_eq!(
        embedded_images(&pdf).len(),
        1,
        "the surface alone became an image"
    );
}

// WHY: a machine without a graphics adapter must still export a three-dimensional figure: under the vector policy the
// exporter never looks for one, and under the default policy it draws the painter's order and says it could not
// verify it, naming the missing adapter, rather than failing; only forcing an image is an error there. The check
// runs in a child process restricted, through `WGPU_BACKEND`, to a backend that does not exist on this platform,
// as the offscreen tests do.
#[test]
fn a_three_dimensional_figure_exports_without_an_adapter() {
    let figure = crossing_planes_figure();
    let exported = ironlab_viewer::export_pdf(
        &figure,
        &TEXT,
        &depth_options(RasterPolicy::Never, DepthPolicy::Vector, 300.0),
    )
    .expect("vectors need no adapter");
    assert!(exported.bytes.starts_with(b"%PDF"), "a PDF was written");
    assert_eq!(
        kinds(&exported.export),
        vec![(
            Some(NodeId(2)),
            ExportWarningKind::Unverified {
                cause: UnverifiedCause::PolicyVector
            }
        )]
    );

    let unavailable = if cfg!(windows) { "metal" } else { "dx12" };
    let output = Command::new(std::env::current_exe().expect("test binary path"))
        .args([
            "--exact",
            "no_adapter_export_probe",
            "--ignored",
            "--nocapture",
        ])
        .env("WGPU_BACKEND", unavailable)
        .env(NO_ADAPTER_EXPORT_PROBE, "1")
        .output()
        .expect("spawn the probe");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "the probe failed or did not run:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Set by [`a_three_dimensional_figure_exports_without_an_adapter`] for its child process, so that the probe does
/// nothing when run directly with a real adapter available.
const NO_ADAPTER_EXPORT_PROBE: &str = "IRONLAB_NO_ADAPTER_EXPORT_PROBE";

#[test]
#[ignore = "run in a child process with an unavailable WGPU_BACKEND by a_three_dimensional_figure_exports_without_an_adapter"]
fn no_adapter_export_probe() {
    if std::env::var_os(NO_ADAPTER_EXPORT_PROBE).is_none() {
        eprintln!(
            "skipping: only meaningful in the child process started by a_three_dimensional_figure_exports_without_an_adapter"
        );
        return;
    }
    let figure = crossing_planes_figure();
    let auto = ironlab_viewer::export_pdf(
        &figure,
        &TEXT,
        &depth_options(RasterPolicy::Never, DepthPolicy::Auto, 300.0),
    )
    .expect("the default policy exports without an adapter");
    assert_eq!(
        kinds(&auto.export),
        vec![(
            Some(NodeId(2)),
            ExportWarningKind::Unverified {
                cause: UnverifiedCause::NoAdapter
            }
        )],
        "the axes is reported as unverified for want of an adapter"
    );
    assert!(
        auto.export[0].message.contains("adapter"),
        "the message names the missing adapter: {}",
        auto.export[0].message
    );
    let forced = ironlab_viewer::export_pdf(
        &figure,
        &TEXT,
        &depth_options(RasterPolicy::Never, DepthPolicy::Raster, 300.0),
    );
    assert!(
        matches!(forced, Err(ExportError::Render(RenderError::NoAdapter(_)))),
        "forcing an image without an adapter is an error: {forced:?}"
    );
}
