//! PDF export of IronLAB figures.
//!
//! The exporter draws a compiled [`DisplayList`] onto a single PDF page with krilla. The page's MediaBox (and
//! CropBox) equals the figure size in points with no margins, so that a document preparation system such as LaTeX
//! includes the figure unscaled at its physical size. Glyph runs are written as real text: fonts are embedded and
//! subset, and each run carries its source text so that the text in the PDF can be searched, selected and copied.
//!
//! Display-list coordinates have their origin at the top-left corner of the figure with y increasing downwards;
//! krilla uses the same convention for page content, so no flip is applied by this crate.
//!
//! # Invalid items
//!
//! The scene compiler guarantees that the items it emits are valid (see the `Validity` section of
//! [`ironlab_scene::display`]), but display lists can be built by hand, so every item is checked before it reaches
//! krilla, and items that cannot be represented are dealt with as follows.
//!
//! - A path with a non-finite coordinate, a path that does not start with a move, and a path whose stroke has a
//!   non-finite or non-positive width, or an invalid dash array or phase, is skipped entirely.
//! - A glyph run with a non-finite or non-positive size, or a non-finite glyph position, is skipped entirely. Glyphs
//!   whose identifier the font does not contain are dropped from their run. A run with any text range that is
//!   reversed, extends beyond its text or splits a character is drawn as glyph outlines, so that it keeps its
//!   appearance and loses only its copyable text.
//! - A group with a non-finite clip, or a non-finite or singular transform, is skipped with all of its items; a
//!   singular transform collapses its content to a line or a point, so nothing visible is lost.
//! - Colour channels are clamped to `[0, 1]`, with NaN treated as 0.

use std::collections::HashMap;
use std::path::Path;

use ironlab_ir::Figure;
use ironlab_scene::display::{self, DisplayList, Item, ItemKind, PathSegment, Rgba};
use ironlab_text::{FontId, TextEngine};
use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{
    Path as KrillaPath, PathBuilder, Point as KrillaPoint, Transform as KrillaTransform,
};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, Stroke, StrokeDash};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};

/// The miter limit fixed by the display list contract; krilla's default is 10.
const MITER_LIMIT: f32 = 4.0;

/// Options controlling the document-level properties of an exported PDF.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfOptions {
    /// The document title written to the PDF metadata, or `None` to omit it.
    pub title: Option<String>,
    /// The name of the application that created the document, written to the PDF metadata.
    pub creator: String,
    /// The document subject written to the PDF metadata, or `None` to omit it. Figure exports use it to record
    /// provenance.
    pub subject: Option<String>,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            title: None,
            creator: format!("IronLAB {}", env!("CARGO_PKG_VERSION")),
            subject: None,
        }
    }
}

impl PdfOptions {
    /// Returns the options with which [`export_pdf`] writes a figure.
    ///
    /// The title is the source string of the figure title (LaTeX markup included, since PDF metadata cannot hold
    /// typeset mathematics), or `None` when the figure has no title. The creator is `IronLAB <version>`, naming the
    /// version of this crate that exported the file. The subject records the figure's provenance in the form
    /// `written by IronLAB <version>; typesetter: <typesetter>; fonts: <font>, <font>, …`, where the version is the
    /// one recorded in the figure (the version that wrote the figure IR, which may differ from the exporting version).
    pub fn for_figure(figure: &Figure) -> Self {
        let provenance = &figure.provenance;
        Self {
            title: figure.title.as_ref().map(|title| title.content.clone()),
            subject: Some(format!(
                "written by IronLAB {}; typesetter: {}; fonts: {}",
                provenance.ironlab_version,
                provenance.typesetter,
                provenance.fonts.join(", ")
            )),
            ..Self::default()
        }
    }
}

/// An error that prevented a PDF from being produced.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// krilla refused to create or serialise the document, for example because the page size is not positive.
    #[error("PDF generation failed: {0}")]
    Krilla(String),
    /// The bytes of a bundled font could not be loaded by krilla.
    #[error("failed to load font {0:?}")]
    Font(FontId),
    /// The PDF could not be written to its destination.
    #[error("failed to write PDF: {0}")]
    Io(#[from] std::io::Error),
}

/// Draws a display list into a single-page PDF whose MediaBox equals the display list size in points.
///
/// The page is first filled with the display list's background colour, and the items are then drawn in order.
/// Items that cannot be represented, such as paths or glyphs with non-finite coordinates, are skipped so that the
/// resulting PDF is always valid.
///
/// # Errors
///
/// Returns [`PdfError::Krilla`] when the page size is not finite and positive or krilla fails to serialise the
/// document, and [`PdfError::Font`] when a bundled font cannot be loaded.
pub fn render_display_list(
    list: &DisplayList,
    text: &TextEngine,
    options: &PdfOptions,
) -> Result<Vec<u8>, PdfError> {
    let (width, height) = (list.width_pt as f32, list.height_pt as f32);
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return Err(PdfError::Krilla(format!(
            "page size {} x {} pt is not finite and positive",
            list.width_pt, list.height_pt
        )));
    }
    let page_box = krilla::geom::Rect::from_xywh(0.0, 0.0, width, height);
    let settings = PageSettings::from_wh(width, height)
        .ok_or_else(|| {
            PdfError::Krilla(format!(
                "krilla rejected the page size {} x {} pt",
                list.width_pt, list.height_pt
            ))
        })?
        .with_media_box(page_box)
        .with_crop_box(page_box);

    let mut document = Document::new();
    let mut metadata = Metadata::new().creator(options.creator.clone());
    if let Some(title) = &options.title {
        metadata = metadata.title(title.clone());
    }
    if let Some(subject) = &options.subject {
        metadata = metadata.description(subject.clone());
    }
    document.set_metadata(metadata);

    {
        let mut page = document.start_page_with(settings);
        let mut surface = page.surface();
        let mut painter = Painter {
            text,
            fonts: HashMap::new(),
        };
        let result = painter.draw_page(&mut surface, list, width, height);
        surface.finish();
        page.finish();
        result?;
    }

    document
        .finish()
        .map_err(|error| PdfError::Krilla(error.to_string()))
}

/// Compiles and exports a figure, with the document metadata given by [`PdfOptions::for_figure`].
///
/// # Errors
///
/// Returns an error under the same conditions as [`render_display_list`].
pub fn export_pdf(figure: &Figure, text: &TextEngine) -> Result<Vec<u8>, PdfError> {
    let scene = ironlab_scene::compile(figure, text);
    render_display_list(&scene.display_list, text, &PdfOptions::for_figure(figure))
}

/// Compiles and exports a figure, writing the PDF to `path`.
///
/// # Errors
///
/// Returns [`PdfError::Io`] when the file cannot be written, and otherwise the errors of [`export_pdf`].
pub fn write_pdf(
    figure: &Figure,
    text: &TextEngine,
    path: impl AsRef<Path>,
) -> Result<(), PdfError> {
    let bytes = export_pdf(figure, text)?;
    std::fs::write(path.as_ref(), bytes)?;
    Ok(())
}

/// A font loaded into krilla, with the number of glyphs it contains.
#[derive(Clone)]
struct LoadedFont {
    font: Font,
    glyph_count: u16,
}

/// Draws display items onto a krilla surface, loading each font at most once per document.
struct Painter<'a> {
    text: &'a TextEngine,
    fonts: HashMap<FontId, LoadedFont>,
}

impl Painter<'_> {
    fn draw_page(
        &mut self,
        surface: &mut Surface<'_>,
        list: &DisplayList,
        width: f32,
        height: f32,
    ) -> Result<(), PdfError> {
        let background = clamp_color(list.background);
        if background.a > 0.0
            && let Some(path) = rect_path(0.0, 0.0, width, height)
        {
            surface.set_fill(Some(fill(background, FillRule::NonZero)));
            surface.set_stroke(None);
            surface.draw_path(&path);
        }
        self.draw_items(surface, &list.items)
    }

    fn draw_items(&mut self, surface: &mut Surface<'_>, items: &[Item]) -> Result<(), PdfError> {
        for item in items {
            match &item.kind {
                ItemKind::Path(path) => draw_path(surface, path),
                ItemKind::Glyphs(glyphs) => self.draw_glyphs(surface, glyphs)?,
                ItemKind::Group {
                    clip,
                    transform,
                    items,
                } => self.draw_group(surface, *clip, *transform, items)?,
            }
        }
        Ok(())
    }

    fn draw_group(
        &mut self,
        surface: &mut Surface<'_>,
        clip: Option<display::Rect>,
        transform: Option<display::Transform>,
        items: &[Item],
    ) -> Result<(), PdfError> {
        let clip = match clip {
            Some(rect) => match clip_path(surface, rect) {
                Some(path) => Some(path),
                None => return Ok(()),
            },
            None => None,
        };
        let transform = match transform {
            Some(t) => match convert_transform(t) {
                Some(t) => Some(t),
                None => return Ok(()),
            },
            None => None,
        };

        // The clip is expressed in the parent space, so it is pushed before the group's transform.
        if let Some(path) = &clip {
            surface.push_clip_path(path, &FillRule::NonZero);
        }
        if let Some(t) = &transform {
            surface.push_transform(t);
        }
        let result = self.draw_items(surface, items);
        if transform.is_some() {
            surface.pop();
        }
        if clip.is_some() {
            surface.pop();
        }
        result
    }

    fn draw_glyphs(
        &mut self,
        surface: &mut Surface<'_>,
        run: &display::GlyphsItem,
    ) -> Result<(), PdfError> {
        let size = run.size_pt as f32;
        if !(size.is_finite() && size > 0.0) {
            return Ok(());
        }
        let positions_finite = run.glyphs.iter().all(|g| {
            let (x, y) = (g.x as f32, g.y as f32);
            x.is_finite() && y.is_finite()
        });
        if !positions_finite {
            return Ok(());
        }
        let color = clamp_color(run.color);
        if color.a <= 0.0 {
            return Ok(());
        }

        let loaded = self.font(run.font)?;
        let kept: Vec<&display::PlacedGlyph> = run
            .glyphs
            .iter()
            .filter(|g| g.id < loaded.glyph_count)
            .collect();
        let Some(first) = kept.first() else {
            return Ok(());
        };

        // krilla advances a pen by each glyph's x advance and raises each glyph by its y offset, both in em units
        // relative to the start point. Positions are absolute, so each advance is the distance to the next glyph and
        // each offset is the glyph's height above the first glyph's baseline. The last glyph advances by zero, which
        // only affects the (unused) pen position after the run.
        let (x0, y0) = (first.x, first.y);
        let em = f64::from(size);
        let glyphs: Vec<KrillaGlyph> = kept
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let advance = kept.get(i + 1).map_or(0.0, |next| (next.x - g.x) / em);
                KrillaGlyph::new(
                    GlyphId::new(u32::from(g.id)),
                    advance as f32,
                    0.0,
                    ((y0 - g.y) / em) as f32,
                    0.0,
                    g.text_range.clone(),
                    None,
                )
            })
            .collect();
        if !glyphs
            .iter()
            .all(|g| g.x_advance.is_finite() && g.y_offset.is_finite())
        {
            return Ok(());
        }

        // A run whose text ranges cannot slice its text keeps its appearance but not its copyable text.
        let ranges_valid = kept
            .iter()
            .all(|g| text_range_is_valid(&run.text, &g.text_range));

        surface.set_fill(Some(fill(color, FillRule::NonZero)));
        surface.set_stroke(None);
        surface.draw_glyphs(
            KrillaPoint::from_xy(x0 as f32, y0 as f32),
            &glyphs,
            loaded.font,
            if ranges_valid { &run.text } else { "" },
            size,
            !ranges_valid,
        );
        Ok(())
    }

    /// Returns the krilla font for `id`, loading it on first use.
    fn font(&mut self, id: FontId) -> Result<LoadedFont, PdfError> {
        if let Some(loaded) = self.fonts.get(&id) {
            return Ok(loaded.clone());
        }
        let bytes = self.text.font_bytes(id);
        let glyph_count = glyph_count(bytes).ok_or(PdfError::Font(id))?;
        let font = Font::new(bytes.into(), 0).ok_or(PdfError::Font(id))?;
        let loaded = LoadedFont { font, glyph_count };
        self.fonts.insert(id, loaded.clone());
        Ok(loaded)
    }
}

/// Draws a path item, or nothing when the item is invalid.
fn draw_path(surface: &mut Surface<'_>, item: &display::PathItem) {
    let Some(path) = convert_path(&item.segments) else {
        return;
    };
    let fill = item
        .fill
        .map(|f| (clamp_color(f.color), f.rule))
        .filter(|(color, _)| color.a > 0.0)
        .map(|(color, rule)| {
            fill(
                color,
                match rule {
                    display::FillRule::NonZero => FillRule::NonZero,
                    display::FillRule::EvenOdd => FillRule::EvenOdd,
                },
            )
        });
    let stroke = match &item.stroke {
        Some(s) => match convert_stroke(s) {
            Some(stroke) => stroke,
            None => return,
        },
        None => None,
    };
    if fill.is_none() && stroke.is_none() {
        return;
    }
    surface.set_fill(fill);
    surface.set_stroke(stroke);
    surface.draw_path(&path);
}

/// Converts path segments, returning `None` when the path does not start with a move, has a non-finite coordinate or
/// is empty.
fn convert_path(segments: &[PathSegment]) -> Option<KrillaPath> {
    if !matches!(segments.first(), Some(PathSegment::MoveTo(_))) {
        return None;
    }
    let mut builder = PathBuilder::new();
    for segment in segments {
        match *segment {
            PathSegment::MoveTo(p) => {
                let (x, y) = point(p)?;
                builder.move_to(x, y);
            }
            PathSegment::LineTo(p) => {
                let (x, y) = point(p)?;
                builder.line_to(x, y);
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let (x1, y1) = point(c1)?;
                let (x2, y2) = point(c2)?;
                let (x, y) = point(p)?;
                builder.cubic_to(x1, y1, x2, y2, x, y);
            }
            PathSegment::Close => builder.close(),
        }
    }
    builder.finish()
}

/// Converts a stroke. The outer `None` marks an invalid stroke (which invalidates its item); the inner `None` marks a
/// valid stroke that paints nothing because it is fully transparent.
fn convert_stroke(stroke: &display::Stroke) -> Option<Option<Stroke>> {
    let width = stroke.width as f32;
    if !(width.is_finite() && width > 0.0) {
        return None;
    }
    let dash = if stroke.dash.is_empty() {
        None
    } else {
        let array: Vec<f32> = stroke.dash.iter().map(|&d| d as f32).collect();
        let offset = stroke.dash_offset as f32;
        let sum: f32 = array.iter().sum();
        let valid = offset.is_finite()
            && array.iter().all(|d| d.is_finite() && *d >= 0.0)
            && sum.is_finite()
            && sum > 0.0;
        if !valid {
            return None;
        }
        Some(StrokeDash { array, offset })
    };
    let color = clamp_color(stroke.color);
    if color.a <= 0.0 {
        return Some(None);
    }
    Some(Some(Stroke {
        paint: rgb_color(color).into(),
        width,
        miter_limit: MITER_LIMIT,
        line_cap: match stroke.cap {
            display::LineCap::Butt => LineCap::Butt,
            display::LineCap::Round => LineCap::Round,
            display::LineCap::Square => LineCap::Square,
        },
        line_join: match stroke.join {
            display::LineJoin::Miter => LineJoin::Miter,
            display::LineJoin::Round => LineJoin::Round,
            display::LineJoin::Bevel => LineJoin::Bevel,
        },
        opacity: opacity(color.a),
        dash,
    }))
}

/// Builds the clip path of a group, or `None` when the clip is not finite or cannot be placed under the current
/// transform.
fn clip_path(surface: &Surface<'_>, rect: display::Rect) -> Option<KrillaPath> {
    let (x, y) = point(display::Point::new(rect.x, rect.y))?;
    let (right, bottom) = point(display::Point::new(rect.right(), rect.bottom()))?;
    // krilla maps the clip path through the current transform and requires the result to be finite.
    let ctm = surface.ctm();
    let maps_finitely = [(x, y), (right, y), (right, bottom), (x, bottom)]
        .iter()
        .all(|&(px, py)| {
            let tx = ctm.sx() * px + ctm.kx() * py + ctm.tx();
            let ty = ctm.ky() * px + ctm.sy() * py + ctm.ty();
            tx.is_finite() && ty.is_finite()
        });
    if !maps_finitely {
        return None;
    }
    rect_path(x.min(right), y.min(bottom), x.max(right), y.max(bottom))
}

/// Converts a group transform, returning `None` when it is not finite or is singular.
fn convert_transform(t: display::Transform) -> Option<KrillaTransform> {
    let [a, b, c, d, e, f] = [t.a, t.b, t.c, t.d, t.e, t.f].map(|v| v as f32);
    if ![a, b, c, d, e, f].iter().all(|v| v.is_finite()) {
        return None;
    }
    // A singular (or numerically singular) linear part collapses the group's content to a line or a point.
    let det = f64::from(a) * f64::from(d) - f64::from(b) * f64::from(c);
    let scale = [a, b, c, d]
        .iter()
        .map(|v| f64::from(v.abs()))
        .fold(0.0, f64::max);
    if !(det.is_finite() && det.abs() > 1e-9 * scale * scale) {
        return None;
    }
    Some(KrillaTransform::from_row(a, b, c, d, e, f))
}

/// The closed outline of the rectangle from `(left, top)` to `(right, bottom)`.
fn rect_path(left: f32, top: f32, right: f32, bottom: f32) -> Option<KrillaPath> {
    let mut builder = PathBuilder::new();
    builder.move_to(left, top);
    builder.line_to(right, top);
    builder.line_to(right, bottom);
    builder.line_to(left, bottom);
    builder.close();
    builder.finish()
}

/// Converts a point to single precision, returning `None` when either coordinate is not finite in single precision.
fn point(p: display::Point) -> Option<(f32, f32)> {
    let (x, y) = (p.x as f32, p.y as f32);
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

/// Reports whether `range` is a non-reversed byte range within `text` whose ends lie on character boundaries.
fn text_range_is_valid(text: &str, range: &std::ops::Range<usize>) -> bool {
    range.start <= range.end
        && range.end <= text.len()
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end)
}

/// Clamps every channel of a colour to `[0, 1]`, treating NaN as 0.
fn clamp_color(color: Rgba) -> Rgba {
    let clamp = |v: f32| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
    Rgba::new(
        clamp(color.r),
        clamp(color.g),
        clamp(color.b),
        clamp(color.a),
    )
}

/// Converts the colour channels of a clamped colour to 8-bit RGB.
fn rgb_color(color: Rgba) -> rgb::Color {
    let byte = |v: f32| (v * 255.0).round() as u8;
    rgb::Color::new(byte(color.r), byte(color.g), byte(color.b))
}

/// Converts a clamped alpha to a krilla opacity.
fn opacity(alpha: f32) -> NormalizedF32 {
    NormalizedF32::new(alpha).unwrap_or(NormalizedF32::ONE)
}

/// A fill in a clamped colour, with the colour's alpha as its opacity.
fn fill(color: Rgba, rule: FillRule) -> Fill {
    Fill {
        paint: rgb_color(color).into(),
        opacity: opacity(color.a),
        rule,
    }
}

/// Reads the number of glyphs of an OpenType font from its `maxp` table.
fn glyph_count(font: &[u8]) -> Option<u16> {
    let be_u16 = |at: usize| -> Option<u16> {
        Some(u16::from_be_bytes(font.get(at..at + 2)?.try_into().ok()?))
    };
    let be_u32 = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(font.get(at..at + 4)?.try_into().ok()?))
    };
    let table_count = usize::from(be_u16(4)?);
    (0..table_count).find_map(|i| {
        let record = 12 + 16 * i;
        if font.get(record..record + 4)? != b"maxp" {
            return None;
        }
        let offset = usize::try_from(be_u32(record + 8)?).ok()?;
        be_u16(offset + 4)
    })
}
