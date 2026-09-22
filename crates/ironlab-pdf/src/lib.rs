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
//! # Dense content
//!
//! Content that the scene compiler marked as dense, such as a surface with tens of thousands of faces, is drawn as a
//! deflated image XObject instead of one vector path per face when it reaches the threshold of
//! [`RasterOptions::policy`]. Everything else — axes, ticks, tick labels, axis labels, titles, legends and every
//! other artist — stays vector, and text stays selectable. The image is rendered by the [`Rasteriser`] the caller
//! supplies, which is the viewer's own headless GPU renderer, so the exported pixels are the ones the user saw on
//! screen. See [`raster`] for the placement rules and for what happens when no rasteriser is given.
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
//! - An image whose rectangle is not finite and positive, whose grid of samples is empty, whose channel count is
//!   neither three nor four, or whose samples are not exactly `width · height · channels` bytes, is skipped.
//! - A dense group that encloses no finite geometry within its clips, or that lies beneath a transform whose inverse
//!   would not be finite, is drawn as vector geometry, because there is nowhere to place an image for it.
//! - Colour channels are clamped to `[0, 1]`, with NaN treated as 0.

pub mod raster;

use std::collections::HashMap;
use std::path::Path;

use ironlab_ir::Figure;
use ironlab_scene::display::{self, DisplayList, Item, ItemKind, PathSegment, Rgba};
use ironlab_text::{FontId, TextEngine};

pub use ironlab_scene::SceneWarning;
use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{
    Path as KrillaPath, PathBuilder, Point as KrillaPoint, Size as KrillaSize,
    Transform as KrillaTransform,
};
use krilla::image::Image;
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, Stroke, StrokeDash};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};

pub use raster::{
    DEFAULT_RASTER_CELLS, DEFAULT_RASTER_DPI, RasterImage, RasterOptions, RasterPolicy, Rasteriser,
};

/// The miter limit fixed by the display list contract; krilla's default is 10.
const MITER_LIMIT: f32 = 4.0;

/// Options controlling the document-level properties of an exported PDF and its raster fallback.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfOptions {
    /// The document title written to the PDF metadata, or `None` to omit it.
    pub title: Option<String>,
    /// The name of the application that created the document, written to the PDF metadata.
    pub creator: String,
    /// The document subject written to the PDF metadata, or `None` to omit it. Figure exports use it to record
    /// provenance.
    pub subject: Option<String>,
    /// How dense content is drawn, and at what resolution it is rasterised.
    pub raster: RasterOptions,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            title: None,
            creator: format!("IronLAB {}", env!("CARGO_PKG_VERSION")),
            subject: None,
            raster: RasterOptions::default(),
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
    /// The rasteriser could not render dense content.
    #[error(
        "failed to rasterise dense content: {0}; set the export's raster policy to `Never` to draw it as vector \
         geometry instead"
    )]
    Raster(String),
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
/// Dense content is drawn as an image when `raster` is `Some` and [`RasterOptions::policy`] says so, and as vector
/// geometry otherwise. Passing `None` therefore guarantees a wholly vector page whatever the policy, which is what a
/// caller without access to a renderer wants.
///
/// # Errors
///
/// Returns [`PdfError::Krilla`] when the page size is not finite and positive or krilla fails to serialise the
/// document, [`PdfError::Font`] when a bundled font cannot be loaded, and [`PdfError::Raster`] when the rasteriser
/// fails on content the policy says to rasterise.
pub fn render_display_list(
    list: &DisplayList,
    text: &TextEngine,
    options: &PdfOptions,
    raster: Option<&mut dyn Rasteriser>,
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
            raster,
            options: options.raster,
            page: display::Rect::new(0.0, 0.0, list.width_pt, list.height_pt),
            to_figure: display::Transform::IDENTITY,
            clip: None,
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

/// A figure exported as a PDF: the bytes of the document, and the warnings the scene compiler raised while drawing
/// the figure, each naming the node it concerns, so that a caller learns what was left off the page.
#[derive(Clone, Debug, PartialEq)]
pub struct Exported {
    /// The PDF document.
    pub bytes: Vec<u8>,
    /// The warnings of the compiled scene, in the order the compiler raised them.
    pub warnings: Vec<SceneWarning>,
}

/// Compiles and exports a figure, with the document metadata given by [`PdfOptions::for_figure`].
///
/// `raster` renders the dense parts of the figure; pass `None` to draw the whole figure as vector geometry. The
/// viewer's headless renderer implements [`Rasteriser`], and `ironlab_viewer::export_pdf` wires it in. The
/// warnings the scene compiler raised while drawing the figure are returned with the bytes, because an artist the
/// compiler left out is missing from the page and the caller must be able to learn why.
///
/// # Errors
///
/// Returns an error under the same conditions as [`render_display_list`].
pub fn export_pdf(
    figure: &Figure,
    text: &TextEngine,
    raster: Option<&mut dyn Rasteriser>,
) -> Result<Exported, PdfError> {
    let scene = ironlab_scene::compile(figure, text);
    let bytes = render_display_list(
        &scene.display_list,
        text,
        &PdfOptions::for_figure(figure),
        raster,
    )?;
    Ok(Exported {
        bytes,
        warnings: scene.warnings,
    })
}

/// Compiles and exports a figure, writing the PDF to `path`, and returns the warnings the scene compiler raised
/// while drawing the figure, as [`export_pdf`] does.
///
/// # Errors
///
/// Returns [`PdfError::Io`] when the file cannot be written, and otherwise the errors of [`export_pdf`].
pub fn write_pdf(
    figure: &Figure,
    text: &TextEngine,
    raster: Option<&mut dyn Rasteriser>,
    path: impl AsRef<Path>,
) -> Result<Vec<SceneWarning>, PdfError> {
    let exported = export_pdf(figure, text, raster)?;
    std::fs::write(path.as_ref(), exported.bytes)?;
    Ok(exported.warnings)
}

/// A font loaded into krilla, with the number of glyphs it contains.
#[derive(Clone)]
struct LoadedFont {
    font: Font,
    glyph_count: u16,
}

/// Draws display items onto a krilla surface, loading each font at most once per document.
///
/// The painter mirrors krilla's own transform and clip stack in `to_figure` and `clip`, because krilla exposes
/// neither the accumulated clip nor an inverse of its transform, and both are needed to place a raster image: the
/// image's rectangle is computed in figure space, and the image is drawn beneath the inverse of the current
/// transform so that it lands there whatever groups enclose it.
struct Painter<'t, 'r> {
    text: &'t TextEngine,
    fonts: HashMap<FontId, LoadedFont>,
    /// The renderer of dense content, or `None` when dense content is drawn as vector geometry.
    raster: Option<&'r mut dyn Rasteriser>,
    options: RasterOptions,
    /// The page, in figure space, which bounds every raster.
    page: display::Rect,
    /// The transform from the current item space to figure space.
    to_figure: display::Transform,
    /// The intersection of the clips enclosing the current items, in figure space.
    clip: Option<display::Rect>,
}

impl Painter<'_, '_> {
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
                ItemKind::Image(image) => draw_image(surface, image),
                ItemKind::Group {
                    clip,
                    transform,
                    items,
                } => self.draw_group(surface, *clip, *transform, items)?,
                ItemKind::Dense { cells, items } => self.draw_dense(surface, *cells, items)?,
                ItemKind::Depth { items } => self.draw_items(surface, items)?,
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
                Some(path) => Some((rect, path)),
                None => return Ok(()),
            },
            None => None,
        };
        let transform = match transform {
            Some(t) => match convert_transform(t) {
                Some(converted) => Some((t, converted)),
                None => return Ok(()),
            },
            None => None,
        };

        // The clip is expressed in the parent space, so it is pushed before the group's transform.
        if let Some((_, path)) = &clip {
            surface.push_clip_path(path, &FillRule::NonZero);
        }
        if let Some((_, t)) = &transform {
            surface.push_transform(t);
        }
        let (outer_clip, outer_transform) = (self.clip, self.to_figure);
        if let Some((rect, _)) = &clip {
            self.clip = Some(match self.clip {
                Some(outer) => raster::intersect(outer, map_rect(self.to_figure, *rect)),
                None => map_rect(self.to_figure, *rect),
            });
        }
        if let Some((t, _)) = &transform {
            self.to_figure = t.then(self.to_figure);
        }

        let result = self.draw_items(surface, items);

        self.clip = outer_clip;
        self.to_figure = outer_transform;
        if transform.is_some() {
            surface.pop();
        }
        if clip.is_some() {
            surface.pop();
        }
        result
    }

    /// Draws content the scene compiler marked as dense, either as a raster image or as vector geometry.
    ///
    /// The items are drawn as vector geometry whenever there is no rasteriser, the policy keeps them vector, the
    /// content has no finite extent within its clips, or the current transform cannot be inverted (which would leave
    /// nowhere to put the image). Only a rasteriser that is asked to render and fails is an error, because at that
    /// point the caller has asked for something that cannot be delivered silently.
    fn draw_dense(
        &mut self,
        surface: &mut Surface<'_>,
        cells: u64,
        items: &[Item],
    ) -> Result<(), PdfError> {
        if self.raster.is_none() || !self.options.policy.rasterises(cells) {
            return self.draw_items(surface, items);
        }
        let Some(inverse) = invert(self.to_figure) else {
            return self.draw_items(surface, items);
        };
        let Some(plan) = raster::plan(items, self.to_figure, self.clip, self.page, &self.options)
        else {
            return self.draw_items(surface, items);
        };
        let rasteriser = self
            .raster
            .as_mut()
            .expect("the rasteriser was just checked to be present");
        let rendered = rasteriser
            .rasterise(&plan.list, self.options.dpi)
            .map_err(PdfError::Raster)?;
        let Some(image) = plan.image(&rendered) else {
            return self.draw_items(surface, items);
        };

        // The image rectangle is in figure space, so the enclosing transforms are undone before it is drawn. The
        // enclosing clips stay in force, which re-clips the raster to the plot box exactly as the vector geometry is.
        surface.push_transform(&inverse);
        draw_image(surface, &image);
        surface.pop();
        Ok(())
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

/// Draws an image item as a deflated image XObject, or nothing when the item is invalid.
///
/// krilla deflates the samples and, for an image with alpha, its soft mask, so both streams carry `/FlateDecode`.
/// It writes `/Interpolate` only when interpolation is asked for, and the key defaults to false, so omitting it is
/// exactly `/Interpolate false`: the raster is drawn with hard pixel edges, which is what a figure needs, because
/// smoothing would blur the boundaries between faces that the vector version draws sharply.
///
/// This is the only path by which an image reaches the PDF: the image items that the scene compiler emits for the
/// image artists of [issue #7](https://github.com/thclark/ironlab/issues/7) take it too, beneath the transforms of
/// the groups that place them.
fn draw_image(surface: &mut Surface<'_>, item: &display::ImageItem) {
    if !item.is_valid() {
        return;
    }
    let (Some(size), Some(origin)) = (
        KrillaSize::from_wh(item.rect.width as f32, item.rect.height as f32),
        point(display::Point::new(item.rect.x, item.rect.y)),
    ) else {
        return;
    };
    let Ok(image) = Image::from_custom(Samples::new(item), false) else {
        return;
    };
    surface.push_transform(&KrillaTransform::from_translate(origin.0, origin.1));
    surface.draw_image(image, size);
    surface.pop();
}

/// The samples of an [`display::ImageItem`] presented to krilla as a custom image.
///
/// PDF holds an image's colour and its transparency in separate streams, the second of which is a soft mask, so the
/// interleaved samples of the display list are split here. An opaque image keeps three channels and is written with
/// no soft mask at all, which is both smaller and free of the transparency that strict PDF profiles restrict.
#[derive(Clone, Hash)]
struct Samples {
    width: u32,
    height: u32,
    /// `width · height · 3` bytes of red, green and blue.
    color: std::sync::Arc<[u8]>,
    /// `width · height` bytes of straight alpha, or `None` when the image is opaque.
    alpha: Option<std::sync::Arc<[u8]>>,
}

impl Samples {
    fn new(item: &display::ImageItem) -> Self {
        let (color, alpha) = if item.channels == display::ImageItem::RGB {
            (item.samples.clone(), None)
        } else {
            let pixels = item.samples.len() / usize::from(display::ImageItem::RGBA);
            let mut color = Vec::with_capacity(pixels * usize::from(display::ImageItem::RGB));
            let mut alpha = Vec::with_capacity(pixels);
            for pixel in item.samples.as_chunks::<4>().0 {
                color.extend_from_slice(&pixel[..3]);
                alpha.push(pixel[3]);
            }
            (color.into(), Some(alpha.into()))
        };
        Self {
            width: item.width,
            height: item.height,
            color,
            alpha,
        }
    }
}

impl krilla::image::CustomImage for Samples {
    fn color_channel(&self) -> &[u8] {
        &self.color
    }

    fn alpha_channel(&self) -> Option<&[u8]> {
        self.alpha.as_deref()
    }

    fn bits_per_component(&self) -> krilla::image::BitsPerComponent {
        krilla::image::BitsPerComponent::Eight
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn icc_profile(&self) -> Option<&[u8]> {
        None
    }

    fn color_space(&self) -> krilla::image::ImageColorspace {
        krilla::image::ImageColorspace::Rgb
    }
}

/// The inverse of a transform, or `None` when it is not finite or is singular.
fn invert(t: display::Transform) -> Option<KrillaTransform> {
    let det = t.a * t.d - t.b * t.c;
    if !det.is_finite() || det == 0.0 {
        return None;
    }
    let inverse = display::Transform {
        a: t.d / det,
        b: -t.b / det,
        c: -t.c / det,
        d: t.a / det,
        e: (t.c * t.f - t.d * t.e) / det,
        f: (t.b * t.e - t.a * t.f) / det,
    };
    convert_transform(inverse)
}

/// Maps a rectangle through a transform, taking the axis-aligned bound of the result.
///
/// The scene compiler never places a clipped group beneath a rotation, so for the clips this is applied to the
/// bound is the mapped rectangle itself.
fn map_rect(t: display::Transform, rect: display::Rect) -> display::Rect {
    let a = t.apply(display::Point::new(rect.x, rect.y));
    let b = t.apply(display::Point::new(rect.right(), rect.bottom()));
    display::Rect::new(
        a.x.min(b.x),
        a.y.min(b.y),
        (b.x - a.x).abs(),
        (b.y - a.y).abs(),
    )
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
