//! Helpers shared by the PDF integration tests: display-list builders, external tool drivers and pixel sampling.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use image::{RgbImage, RgbaImage};
use ironlab_pdf::{PdfOptions, render_display_list};
use ironlab_scene::display::{
    DisplayList, Fill, FillRule, GlyphsItem, Item, ItemKind, LineCap, LineJoin, PathItem,
    PathSegment, PlacedGlyph, Point, Rect, Rgba, Stroke, Transform,
};
use ironlab_text::{TextEngine, TextItem, TextLayout};

/// The environment variable that turns a missing external tool into a test failure.
pub const REQUIRE_TOOLS_VAR: &str = "IRONLAB_REQUIRE_PDF_TOOLS";

pub const RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);
pub const BLUE: Rgba = Rgba::new(0.0, 0.0, 1.0, 1.0);

/// Returns early from the enclosing test when any of the named tools is missing and tools are not required.
#[macro_export]
macro_rules! require_tools {
    ($($tool:expr),+ $(,)?) => {
        if !$crate::common::tools_available(&[$($tool),+]) {
            return;
        }
    };
}

/// Reports whether every tool is on `PATH`.
///
/// A missing tool panics when [`REQUIRE_TOOLS_VAR`] is set, and otherwise prints a skip message and returns false.
pub fn tools_available(tools: &[&str]) -> bool {
    let missing: Vec<&str> = tools
        .iter()
        .copied()
        .filter(|tool| find_on_path(tool).is_none())
        .collect();
    if missing.is_empty() {
        return true;
    }
    if std::env::var_os(REQUIRE_TOOLS_VAR).is_some() {
        panic!(
            "required PDF tools are missing from PATH: {missing:?} ({REQUIRE_TOOLS_VAR} is set)"
        );
    }
    eprintln!(
        "SKIPPED: PDF tools missing from PATH: {missing:?}. Install poppler and Ghostscript, or set \
         {REQUIRE_TOOLS_VAR} to make this a failure."
    );
    false
}

fn find_on_path(tool: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(tool))
        .find(|candidate| candidate.is_file())
}

/// A per-test scratch directory under `<temp>/ironlab-pdf-tests/`.
///
/// The directory is removed when the test passes and kept (with its path printed) when the test panics, so that the
/// PDF and rasters of a failing test can be inspected.
pub struct Workspace {
    pub dir: PathBuf,
}

impl Workspace {
    pub fn new(name: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = format!(
            "{name}-{}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let dir = std::env::temp_dir().join("ironlab-pdf-tests").join(unique);
        std::fs::create_dir_all(&dir).expect("create test workspace");
        Self { dir }
    }

    /// Writes `bytes` to `<name>.pdf` in the workspace and returns its path.
    pub fn write_pdf(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.dir.join(format!("{name}.pdf"));
        std::fs::write(&path, bytes).expect("write PDF to test workspace");
        path
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("test artefacts kept in {}", self.dir.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// Returns a text engine over the bundled fonts.
pub fn engine() -> TextEngine {
    TextEngine::new()
}

/// Renders a display list with default options.
pub fn render(list: &DisplayList, text: &TextEngine) -> Vec<u8> {
    render_display_list(list, text, &PdfOptions::default()).expect("render display list")
}

/// An empty display list on a white page.
pub fn page(width_pt: f64, height_pt: f64) -> DisplayList {
    DisplayList {
        width_pt,
        height_pt,
        background: Rgba::WHITE,
        items: Vec::new(),
    }
}

pub fn item(kind: ItemKind) -> Item {
    Item { source: None, kind }
}

/// The closed outline of a rectangle, traversed clockwise on screen (y down).
pub fn rect_segments(r: Rect) -> Vec<PathSegment> {
    vec![
        PathSegment::MoveTo(Point::new(r.x, r.y)),
        PathSegment::LineTo(Point::new(r.right(), r.y)),
        PathSegment::LineTo(Point::new(r.right(), r.bottom())),
        PathSegment::LineTo(Point::new(r.x, r.bottom())),
        PathSegment::Close,
    ]
}

pub fn filled_path(segments: Vec<PathSegment>, color: Rgba, rule: FillRule) -> Item {
    item(ItemKind::Path(PathItem {
        segments,
        fill: Some(Fill { color, rule }),
        stroke: None,
    }))
}

pub fn filled_rect(r: Rect, color: Rgba) -> Item {
    filled_path(rect_segments(r), color, FillRule::NonZero)
}

pub fn solid_stroke(color: Rgba, width: f64) -> Stroke {
    Stroke {
        color,
        width,
        dash: Vec::new(),
        dash_offset: 0.0,
        cap: LineCap::Butt,
        join: LineJoin::Miter,
    }
}

pub fn stroked_line(from: Point, to: Point, stroke: Stroke) -> Item {
    item(ItemKind::Path(PathItem {
        segments: vec![PathSegment::MoveTo(from), PathSegment::LineTo(to)],
        fill: None,
        stroke: Some(stroke),
    }))
}

pub fn group(clip: Option<Rect>, transform: Option<Transform>, items: Vec<Item>) -> Item {
    item(ItemKind::Group {
        clip,
        transform,
        items,
    })
}

/// Converts a text layout into display items with the layout origin (left end of the baseline) at `origin`.
///
/// Glyph runs become glyph items and rules become filled rectangles, which is how the scene compiler places labels.
pub fn text_items(layout: &TextLayout, origin: Point, color: Rgba) -> Vec<Item> {
    layout
        .items
        .iter()
        .map(|text_item| match text_item {
            TextItem::Glyphs(run) => item(ItemKind::Glyphs(GlyphsItem {
                font: run.font,
                size_pt: run.size_pt,
                color,
                text: run.text.clone(),
                glyphs: run
                    .glyphs
                    .iter()
                    .map(|g| PlacedGlyph {
                        id: g.id,
                        x: origin.x + g.x,
                        y: origin.y + g.y,
                        text_range: g.text_range.clone(),
                    })
                    .collect(),
            })),
            TextItem::Rule {
                x,
                y,
                width,
                height,
            } => filled_rect(
                Rect::new(origin.x + x, origin.y + y, *width, *height),
                color,
            ),
        })
        .collect()
}

/// Lays out `content` and returns it as display items placed at `origin`.
pub fn label(
    text: &TextEngine,
    content: &str,
    parse_math: bool,
    size_pt: f64,
    origin: Point,
) -> Vec<Item> {
    text_items(
        &text.layout(content, parse_math, size_pt),
        origin,
        Rgba::BLACK,
    )
}

/// Runs a command, panicking with its output when it cannot be started or exits unsuccessfully.
pub fn run(command: &mut Command) -> Output {
    let description = format!("{command:?}");
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("failed to run {description}: {e}"));
    assert!(
        output.status.success(),
        "{description} exited with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Asserts that poppler reported no syntax errors or warnings while reading a file.
pub fn assert_poppler_quiet(tool: &str, output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.trim().is_empty(),
        "{tool} reported problems with the PDF:\n{stderr}"
    );
}

/// Runs `pdfinfo` (optionally with `-box`) and returns its `key: value` lines.
pub fn pdfinfo(path: &Path, boxes: bool) -> BTreeMap<String, String> {
    let mut command = Command::new("pdfinfo");
    if boxes {
        command.arg("-box");
    }
    let output = run(command.arg(path));
    assert_poppler_quiet("pdfinfo", &output);
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

/// Parses a `pdfinfo` "Page size" value such as `453.54 x 283.46 pts` into `(width, height)`.
pub fn parse_page_size(value: &str) -> (f64, f64) {
    let mut numbers = value
        .split_whitespace()
        .filter_map(|token| token.parse::<f64>().ok());
    let width = numbers
        .next()
        .unwrap_or_else(|| panic!("no width in page size {value:?}"));
    let height = numbers
        .next()
        .unwrap_or_else(|| panic!("no height in page size {value:?}"));
    (width, height)
}

/// Parses a `pdfinfo -box` value such as `0.00 0.00 453.54 283.46` into four numbers.
pub fn parse_box(value: &str) -> [f64; 4] {
    let numbers: Vec<f64> = value
        .split_whitespace()
        .map(|t| t.parse().expect("box coordinate"))
        .collect();
    numbers
        .try_into()
        .unwrap_or_else(|_| panic!("box {value:?} does not have four coordinates"))
}

/// Extracts the text of a PDF with `pdftotext`.
pub fn pdftotext(path: &Path) -> String {
    let output = run(Command::new("pdftotext").arg(path).arg("-"));
    assert_poppler_quiet("pdftotext", &output);
    String::from_utf8(output.stdout).expect("pdftotext output is UTF-8")
}

/// A word and its bounding box, in points from the top-left corner of the page, as reported by `pdftotext -bbox`.
#[derive(Debug)]
pub struct Word {
    pub text: String,
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}

/// Extracts the words of a PDF with their bounding boxes.
pub fn pdftotext_words(path: &Path) -> Vec<Word> {
    let output = run(Command::new("pdftotext").arg("-bbox").arg(path).arg("-"));
    assert_poppler_quiet("pdftotext", &output);
    let html = String::from_utf8(output.stdout).expect("pdftotext output is UTF-8");
    html.lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("<word ")?;
            let (attributes, text) = rest.split_once('>')?;
            let text = text.strip_suffix("</word>")?;
            let attribute = |name: &str| -> Option<f64> {
                let start = attributes.find(&format!("{name}=\""))? + name.len() + 2;
                let end = start + attributes[start..].find('"')?;
                attributes[start..end].parse().ok()
            };
            Some(Word {
                text: text
                    .replace("&amp;", "&")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">"),
                x_min: attribute("xMin")?,
                y_min: attribute("yMin")?,
                x_max: attribute("xMax")?,
                y_max: attribute("yMax")?,
            })
        })
        .collect()
}

/// A font as reported by `pdffonts`.
#[derive(Debug)]
pub struct FontInfo {
    pub name: String,
    pub embedded: bool,
    pub subset: bool,
    pub unicode: bool,
}

/// Lists the fonts of a PDF with `pdffonts`.
pub fn pdffonts(path: &Path) -> Vec<FontInfo> {
    let output = run(Command::new("pdffonts").arg(path));
    assert_poppler_quiet("pdffonts", &output);
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(2)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            // Columns: name, type (may contain spaces), encoding, emb, sub, uni, object number, generation.
            let tokens: Vec<&str> = line.split_whitespace().collect();
            let n = tokens.len();
            assert!(n >= 8, "unexpected pdffonts line {line:?}");
            let yes = |token: &str| token == "yes";
            FontInfo {
                name: tokens[0].to_owned(),
                embedded: yes(tokens[n - 5]),
                subset: yes(tokens[n - 4]),
                unicode: yes(tokens[n - 3]),
            }
        })
        .collect()
}

/// An independent PDF rasteriser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Poppler,
    Ghostscript,
}

pub const ENGINES: [Engine; 2] = [Engine::Poppler, Engine::Ghostscript];

/// The tools needed to rasterise with every engine.
pub const RASTER_TOOLS: [&str; 2] = ["pdftoppm", "gs"];

/// Rasterises the single page of a PDF at 72 dpi, so that one pixel is one point, with anti-aliasing enabled.
///
/// The rasteriser must not report any error or warning about the file.
pub fn rasterise(pdf: &Path, engine: Engine) -> RgbImage {
    let stem = pdf.with_extension("");
    let png = match engine {
        Engine::Poppler => {
            let prefix = PathBuf::from(format!("{}-poppler", stem.display()));
            let output = run(Command::new("pdftoppm")
                .args(["-r", "72", "-png", "-singlefile"])
                .arg(pdf)
                .arg(&prefix));
            assert_poppler_quiet("pdftoppm", &output);
            prefix.with_extension("png")
        }
        Engine::Ghostscript => {
            let png = PathBuf::from(format!("{}-ghostscript.png", stem.display()));
            let output = run(Command::new("gs")
                .args([
                    "-q",
                    "-dNOPAUSE",
                    "-dBATCH",
                    "-dSAFER",
                    "-sDEVICE=png16m",
                    "-r72",
                    "-dTextAlphaBits=4",
                    "-dGraphicsAlphaBits=4",
                ])
                .arg(format!("-sOutputFile={}", png.display()))
                .arg(pdf));
            assert_ghostscript_quiet(&output);
            png
        }
    };
    image::open(&png)
        .unwrap_or_else(|e| panic!("decode {}: {e}", png.display()))
        .to_rgb8()
}

/// The tools needed to rasterise with every engine while keeping unpainted areas transparent.
pub const ALPHA_RASTER_TOOLS: [&str; 2] = ["pdftocairo", "gs"];

/// Rasterises the single page of a PDF at 72 dpi into RGBA, leaving areas that nothing paints fully transparent.
///
/// Poppler is driven through `pdftocairo -transp` and Ghostscript through its `pngalpha` device. The rasteriser must
/// not report any error or warning about the file.
pub fn rasterise_with_alpha(pdf: &Path, engine: Engine) -> RgbaImage {
    let stem = pdf.with_extension("");
    let png = match engine {
        Engine::Poppler => {
            let prefix = PathBuf::from(format!("{}-poppler-alpha", stem.display()));
            let output = run(Command::new("pdftocairo")
                .args(["-png", "-transp", "-r", "72", "-singlefile"])
                .arg(pdf)
                .arg(&prefix));
            assert_poppler_quiet("pdftocairo", &output);
            prefix.with_extension("png")
        }
        Engine::Ghostscript => {
            let png = PathBuf::from(format!("{}-ghostscript-alpha.png", stem.display()));
            let output = run(Command::new("gs")
                .args([
                    "-q",
                    "-dNOPAUSE",
                    "-dBATCH",
                    "-dSAFER",
                    "-sDEVICE=pngalpha",
                    "-r72",
                    "-dTextAlphaBits=4",
                    "-dGraphicsAlphaBits=4",
                ])
                .arg(format!("-sOutputFile={}", png.display()))
                .arg(pdf));
            assert_ghostscript_quiet(&output);
            png
        }
    };
    image::open(&png)
        .unwrap_or_else(|e| panic!("decode {}: {e}", png.display()))
        .to_rgba8()
}

/// Asserts that Ghostscript reported no errors or repairs while reading a file.
pub fn assert_ghostscript_quiet(output: &Output) {
    let messages = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !messages.contains("****") && !messages.to_lowercase().contains("error"),
        "Ghostscript reported problems with the PDF:\n{messages}"
    );
}

/// Renders a display list, writes it into the workspace and rasterises it with every engine.
pub fn render_and_rasterise(
    ws: &Workspace,
    list: &DisplayList,
    text: &TextEngine,
) -> Vec<(Engine, RgbImage)> {
    let pdf = ws.write_pdf("figure", &render(list, text));
    ENGINES
        .iter()
        .map(|&engine| (engine, rasterise(&pdf, engine)))
        .collect()
}

/// Returns the colour of the pixel covering the point `(x, y)`.
pub fn pixel(image: &RgbImage, x: f64, y: f64) -> [u8; 3] {
    let (px, py) = (x.floor() as u32, y.floor() as u32);
    assert!(
        px < image.width() && py < image.height(),
        "sample ({x}, {y}) lies outside the {}x{} raster",
        image.width(),
        image.height()
    );
    image.get_pixel(px, py).0
}

/// Reports whether each channel of `actual` is within `tolerance` of `expected`.
pub fn close_to(actual: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(&a, e)| a.abs_diff(e) <= tolerance)
}

/// Asserts that the pixel at `(x, y)` is within `tolerance` of `expected` in every channel.
pub fn assert_pixel(
    engine: Engine,
    image: &RgbImage,
    x: f64,
    y: f64,
    expected: [u8; 3],
    tolerance: u8,
    what: &str,
) {
    let actual = pixel(image, x, y);
    assert!(
        close_to(actual, expected, tolerance),
        "{engine:?}: {what}: pixel at ({x}, {y}) is {actual:?}, expected {expected:?} ± {tolerance}"
    );
}

pub const WHITE_PX: [u8; 3] = [255, 255, 255];
pub const BLACK_PX: [u8; 3] = [0, 0, 0];
pub const RED_PX: [u8; 3] = [255, 0, 0];

/// Counts the pixels in the rectangle `region` (in points) for which `predicate` holds.
pub fn count_pixels(image: &RgbImage, region: Rect, predicate: impl Fn([u8; 3]) -> bool) -> usize {
    let x0 = region.x.max(0.0).floor() as u32;
    let y0 = region.y.max(0.0).floor() as u32;
    let x1 = (region.right().ceil() as u32).min(image.width());
    let y1 = (region.bottom().ceil() as u32).min(image.height());
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| predicate(image.get_pixel(x, y).0))
        .count()
}

/// Reports whether a pixel is clearly darker than the white page.
pub fn is_ink(px: [u8; 3]) -> bool {
    px.iter().any(|&c| c < 160)
}

/// The mean absolute difference per channel between two equally sized rasters, in `[0, 255]`.
pub fn mean_abs_diff(a: &RgbImage, b: &RgbImage) -> f64 {
    assert_eq!(
        a.dimensions(),
        b.dimensions(),
        "rasters have different dimensions"
    );
    let total: u64 = a
        .as_raw()
        .iter()
        .zip(b.as_raw())
        .map(|(&x, &y)| u64::from(x.abs_diff(y)))
        .sum();
    total as f64 / a.as_raw().len() as f64
}
