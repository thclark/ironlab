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

use std::path::{Path, PathBuf};
use std::process::Command;

use common::{TEXT, figure_with_mapped_image, find_image, gpu_required, rendered_or_skip};
use image::{Rgb, RgbImage};
use ironlab_ir::{
    Artist, Axes, Axis, Cell, ColorSpec, DataId, Figure, Grid, Limits, NdArray, NodeId, Projection,
    Surface, Text, TileLayout, View3d,
};
use ironlab_pdf::{PdfOptions, RasterOptions, RasterPolicy};
use ironlab_scene::display::Rect;
use ironlab_viewer::{ExportError, RenderError, RenderedImage, render_display_list_offscreen};

/// The side of the grid of the dense surface. It has 139 × 139 = 19 321 faces, comfortably above the default
/// threshold, so the default settings rasterise it without being told to.
const DENSE_SIDE: usize = 140;

fn tools_required() -> bool {
    std::env::var_os("IRONLAB_REQUIRE_PDF_TOOLS").is_some()
}

/// Reports whether every tool is on `PATH`, printing a skip message when one is missing and tools are not required.
fn tools_available(tools: &[&str]) -> bool {
    let missing: Vec<&str> = tools
        .iter()
        .copied()
        .filter(|tool| {
            std::env::var_os("PATH").is_none_or(|path| {
                !std::env::split_paths(&path).any(|dir| dir.join(tool).is_file())
            })
        })
        .collect();
    if missing.is_empty() {
        return true;
    }
    assert!(
        !tools_required(),
        "required PDF tools are missing from PATH: {missing:?}"
    );
    eprintln!("skipping: PDF tools missing from PATH: {missing:?}");
    false
}

/// Unwraps an export, or returns `None` (skipping the test) when no adapter is available and a GPU is not required.
fn exported_or_skip(result: Result<Vec<u8>, ExportError>) -> Option<Vec<u8>> {
    match result {
        Ok(bytes) => Some(bytes),
        Err(ExportError::Render(RenderError::NoAdapter(message))) if !gpu_required() => {
            eprintln!(
                "skipping: no graphics adapter ({message}); set IRONLAB_REQUIRE_GPU to make this a failure"
            );
            None
        }
        Err(error) => panic!("export failed: {error}"),
    }
}

/// A scratch directory for one test, removed unless the test panics.
struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ironlab-export-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("create the test workspace");
        Self(dir)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(format!("{name}.pdf"));
        std::fs::write(&path, bytes).expect("write the PDF");
        path
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("test artefacts kept in {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn run(command: &mut Command) -> String {
    let description = format!("{command:?}");
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("failed to run {description}: {e}"));
    assert!(
        output.status.success(),
        "{description} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
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

/// Rasterises the single page of a PDF at `dpi` dots per inch.
///
/// Pages are compared at the resolution the figure was exported at, never below it. An embedded image is drawn with
/// `/Interpolate false`, so a PDF rasteriser asked for fewer pixels than the image has point-samples it and drops
/// the thin lines between faces, which would make a correctly placed raster look nothing like the paths it replaces.
fn rasterise(pdf: &Path, dpi: f64) -> RgbImage {
    let prefix = pdf.with_extension("");
    run(Command::new("pdftoppm")
        .args(["-r", &dpi.to_string(), "-png", "-singlefile"])
        .arg(pdf)
        .arg(&prefix));
    image::open(prefix.with_extension("png"))
        .expect("decode the rasterised page")
        .to_rgb8()
}

/// How much two rasters of the same page differ, in levels out of 255.
///
/// Block averages are compared rather than single pixels because a raster and the paths it replaces do not resolve a
/// boundary identically: a face edge that falls inside a pixel is covered by the GPU's multisampling in one and by
/// the PDF rasteriser's own anti-aliasing in the other, and the two disagree by tens of levels on that pixel alone.
/// Averaging over a block conserves the ink, so a raster in the wrong place, at the wrong scale or in the wrong
/// colours still changes the blocks it covers, while a sub-pixel difference along a boundary does not.
///
/// Both measures are needed. The mean over the page detects a raster that is displaced, rescaled or recoloured,
/// because such a raster disagrees with the paths nearly everywhere. The worst block detects a local defect, such as
/// a corner of the surface left unrendered, which the rest of the page would dilute out of the mean.
fn difference(a: &RgbImage, b: &RgbImage, block: u32) -> (f64, f64) {
    assert_eq!(a.dimensions(), b.dimensions(), "rasters of the same page");
    let (width, height) = a.dimensions();
    let mean = |image: &RgbImage, column: u32, row: u32, channel: usize| -> f64 {
        let total: u32 = (row * block..(row + 1) * block)
            .flat_map(|y| (column * block..(column + 1) * block).map(move |x| (x, y)))
            .map(|(x, y)| u32::from(image.get_pixel(x, y).0[channel]))
            .sum();
        f64::from(total) / f64::from(block * block)
    };
    let blocks: Vec<f64> = (0..height / block)
        .flat_map(|row| (0..width / block).map(move |column| (column, row)))
        .map(|(column, row)| {
            (0..3)
                .map(|channel| {
                    (mean(a, column, row, channel) - mean(b, column, row, channel)).abs()
                })
                .sum::<f64>()
                / 3.0
        })
        .collect();
    (
        blocks.iter().sum::<f64>() / blocks.len() as f64,
        blocks.iter().copied().fold(0.0, f64::max),
    )
}

/// The largest mean difference tolerated across the page, out of 255. A raster displaced by as little as a point, or
/// at a scale wrong by a percent, disagrees with the paths over the whole surface and moves this far beyond it.
const MEAN_TOLERANCE: f64 = 3.0;

/// The largest difference tolerated in any one block, out of 255. What remains within it is the disagreement between
/// two anti-aliasers where the projected faces are most foreshortened and several of them fall in one pixel, which no
/// correct implementation can remove.
const BLOCK_TOLERANCE: f64 = 16.0;

/// The side of the blocks compared, in points.
const BLOCK_PT: f64 = 6.0;

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

/// Asserts that two rasters of the same page, taken at `dpi`, show the same picture.
fn assert_same_picture(vector: &RgbImage, raster: &RgbImage, dpi: f64) {
    let block = (BLOCK_PT * dpi / 72.0).round().max(1.0) as u32;
    let (mean, worst) = difference(vector, raster, block);
    assert!(
        mean <= MEAN_TOLERANCE && worst <= BLOCK_TOLERANCE,
        "the raster and the paths it replaces differ by {mean:.2} of 255 on average (tolerance \
         {MEAN_TOLERANCE}) and by {worst:.1} in the worst block of {BLOCK_PT} points square (tolerance \
         {BLOCK_TOLERANCE})"
    );
}

fn options(policy: RasterPolicy, dpi: f64) -> PdfOptions {
    PdfOptions {
        raster: RasterOptions { policy, dpi },
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
    let (axis, values) = field(side, |x, y| {
        f64::from(((x * 12.0) as i32 + (y * 12.0) as i32) % 2 == 0)
    });
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
    let vector = ws.write("vector", &vector);
    let raster = ws.write("raster", &raster);
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
    assert_same_picture(&vector, &raster, 300.0);
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
    assert_same_picture(&vector, &raster, 300.0);
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
    let Some(bytes) = exported_or_skip(ironlab_viewer::export_pdf(
        &figure,
        &TEXT,
        &PdfOptions::for_figure(&figure),
    )) else {
        return;
    };
    let pdf = ws.write("figure", &bytes);

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
    let bytes = ironlab_viewer::export_pdf(&figure, &TEXT, &PdfOptions::default())
        .expect("a figure with no dense content exports without any renderer");

    assert!(
        bytes.starts_with(b"%PDF"),
        "the exported bytes are a PDF document"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------------------------------------------

/// The pixels of an offscreen render as an RGB image, which is what a raster of a PDF page is; the page is opaque.
fn rgb_of(rendered: &RenderedImage) -> RgbImage {
    RgbImage::from_fn(rendered.width, rendered.height, |x, y| {
        let [r, g, b, _] = rendered.pixel(x, y);
        Rgb([r, g, b])
    })
}

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
    let Some(bytes) = exported_or_skip(ironlab_viewer::export::render_display_list(
        &scene.display_list,
        &TEXT,
        &PdfOptions::for_figure(&figure),
    )) else {
        return;
    };
    let pdf = ws.write("figure", &bytes);
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
    assert_same_picture(&printed, &drawn, dpi);
}
