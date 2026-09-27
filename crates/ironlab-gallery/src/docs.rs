//! Generation of the documentation gallery.
//!
//! [`generate_docs`] writes a Markdown site section for the zensical documentation engine:
//!
//! - `index.md`: an introduction and a column of full-width cards, one per entry, each with a thumbnail, a title
//!   linking to the entry's page and the entry's description;
//! - `<slug>.md` for each entry: the title, the description, the figure shown live by the `<ironlab-figure>` element
//!   over the rendered image linked to its PDF, the exact source of the entry and, when the source uses the shared
//!   data helpers, a link to their page;
//! - `fields.md`: the source of the shared data helpers;
//! - `<slug>.png`, `<slug>-thumb.png` and `<slug>.pdf` for each entry, produced by a [`Renderer`], and `<slug>.fig`,
//!   the Protocol Buffers encoding of the figure that the live figure loads;
//! - `../stylesheets/gallery.css`, the stylesheet of the cards and of the live figure's chrome, which the site
//!   configuration must list in `extra_css`.
//!
//! The generator owns the gallery directory: it removes every file in it apart from `.gitkeep` before writing, so the
//! directory never holds the pages or assets of an entry that has been renamed or removed.
//!
//! # Links
//!
//! Every link is a relative Markdown link to a `.md` file or asset in the same directory. The card grid is HTML, but
//! the cards carry the `markdown` attribute (Python-Markdown's `md_in_html` extension, enabled by default in zensical),
//! so the links inside them are ordinary Markdown links. Zensical therefore rewrites them to its directory URLs and
//! checks them in strict builds, exactly as it does for links in the body of a page.
//!
//! The `<ironlab-figure>` block of an entry page is raw HTML without the `markdown` attribute, because Python-Markdown
//! must pass it through verbatim: an element it does not know would otherwise be wrapped in a paragraph. Zensical
//! nevertheless rewrites the `href` and `src` attributes of raw HTML exactly as it rewrites Markdown links, relative
//! to the source file, so the block's URLs are written like every other link, as bare file names in the gallery
//! directory, and reach the built page as `../<slug>.fig` beside the page's other links.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use ironlab_text::TextEngine;

use crate::error::GalleryError;
use crate::fields::FIELDS_SOURCE;
use crate::{GalleryEntry, all};

/// The resolution of the full-size figure image on each entry page, in dots per inch: the resolution at which
/// `Figure::export_png` writes a figure, so that the gallery images are what a user's own export produces.
pub use ironlab::DEFAULT_PNG_DPI;

/// The resolution of the thumbnail image on the gallery index, in dots per inch.
///
/// The stylesheet shows a thumbnail at most 14 rem wide, which is at most 280 CSS pixels in the documentation theme. A
/// gallery figure is about 120 mm wide, so this resolution gives a thumbnail about 570 pixels wide: at least two image
/// pixels for every CSS pixel, which keeps the lines and the text of the figure sharp on a high-density display.
pub const DEFAULT_THUMBNAIL_DPI: f64 = 120.0;

/// The path of the gallery stylesheet relative to the documentation directory, as it must appear in the `extra_css`
/// list of `zensical.toml`.
pub const GALLERY_CSS_PATH: &str = "stylesheets/gallery.css";

/// The stylesheet of the gallery cards and of the live figures, written to [`GALLERY_CSS_PATH`] in the documentation
/// directory.
///
/// Colours come from the theme's custom properties, so the cards follow the light and dark palettes. The stylesheet
/// also maps those palettes onto the `--ironlab-*` tokens of the `<ironlab-figure>` element, whose own defaults
/// follow the reader's system colour scheme rather than the site's; a body without a scheme attribute, which is what
/// a site without a palette toggle has, is the light scheme.
pub const GALLERY_CSS: &str = r#"/* Styles for the generated IronLAB figure gallery (written by `gallery docs`; do not edit). */

.md-typeset .ironlab-gallery {
  display: flex;
  flex-direction: column;
  gap: 0.8rem;
  margin: 1.5rem 0;
}

.md-typeset .ironlab-gallery .card {
  display: grid;
  grid-template-columns: 11rem minmax(0, 1fr);
  grid-template-rows: auto 1fr;
  column-gap: 1rem;
  align-items: start;
  padding: 0.7rem 0.9rem;
  border: 1px solid var(--md-default-fg-color--lightest);
  border-radius: 0.4rem;
  background-color: var(--md-default-bg-color);
  transition: box-shadow 125ms, border-color 125ms;
}

.md-typeset .ironlab-gallery .card:hover {
  border-color: var(--md-accent-fg-color);
  box-shadow: var(--md-shadow-z2);
}

.md-typeset .ironlab-gallery .card p {
  margin: 0 0 0.3rem;
}

/* The thumbnail occupies the left column beside the title and the description. */
.md-typeset .ironlab-gallery .card > p:first-child {
  grid-row: 1 / span 2;
  margin: 0;
}

.md-typeset .ironlab-gallery .card img {
  display: block;
  width: 100%;
  height: auto;
  background-color: #ffffff;
  border-radius: 0.2rem;
}

/* On a narrow screen the thumbnail sits above the text, at a slightly larger width. */
@media screen and (max-width: 40em) {
  .md-typeset .ironlab-gallery .card {
    grid-template-columns: minmax(0, 1fr);
  }

  .md-typeset .ironlab-gallery .card > p:first-child {
    grid-row: auto;
    max-width: 14rem;
    margin-bottom: 0.5rem;
  }
}

.md-typeset .ironlab-gallery .card strong a {
  color: var(--md-typeset-a-color);
}

.md-typeset .ironlab-figure img {
  display: block;
  max-width: 100%;
  height: auto;
  margin: 0 auto;
  background-color: #ffffff;
}

/* The live figure of an entry page. The element is a block, so it takes the width of the column and the figure's
   aspect ratio, and its fallback image inside it is the still image above until the first frame is drawn. */
.ironlab-figure ironlab-figure {
  display: block;
}

/* The element colours its figurebar, datatips and status line from `--ironlab-*` tokens whose defaults follow the
   reader's system colour scheme. The site's scheme is the one that matters here, so each token is mapped onto the
   theme's own variables: the panels take the code-block colour and the controls the page colour, the pressed tool
   the primary colour, and the rubber band and datatip marker the accent colour. The theme has no problem colour, so
   the viewer's own is kept. A body that carries no scheme attribute is the default (light) scheme. */
body:not([data-md-color-scheme="slate"]) ironlab-figure {
  --ironlab-bg: var(--md-code-bg-color);
  --ironlab-text: var(--md-default-fg-color);
  --ironlab-weak: var(--md-default-fg-color--light);
  --ironlab-widget: var(--md-default-bg-color);
  --ironlab-widget-hover: var(--md-accent-fg-color--transparent);
  --ironlab-stroke: var(--md-default-fg-color--lighter);
  --ironlab-accent: var(--md-primary-fg-color);
  --ironlab-accent-text: var(--md-primary-bg-color);
  --ironlab-selection: var(--md-accent-fg-color);
  --ironlab-problem: #a8412a;
  --ironlab-canvas-bg: var(--md-default-bg-color);
  --ironlab-font: var(--md-text-font-family);
  --ironlab-mono: var(--md-code-font-family);
}

body[data-md-color-scheme="slate"] ironlab-figure {
  --ironlab-bg: var(--md-code-bg-color);
  --ironlab-text: var(--md-default-fg-color);
  --ironlab-weak: var(--md-default-fg-color--light);
  --ironlab-widget: var(--md-default-bg-color);
  --ironlab-widget-hover: var(--md-accent-fg-color--transparent);
  --ironlab-stroke: var(--md-default-fg-color--lighter);
  --ironlab-accent: var(--md-primary-fg-color);
  --ironlab-accent-text: var(--md-primary-bg-color);
  --ironlab-selection: var(--md-accent-fg-color);
  --ironlab-problem: #e59280;
  --ironlab-canvas-bg: var(--md-default-bg-color);
  --ironlab-font: var(--md-text-font-family);
  --ironlab-mono: var(--md-code-font-family);
}
"#;

/// Draws figures for the documentation.
///
/// The docs generator only needs encoded files, so tests can substitute a renderer that does not require a graphics
/// adapter.
pub trait Renderer {
    /// Renders a figure as a PNG image at a resolution in dots per inch.
    ///
    /// # Errors
    ///
    /// Returns [`GalleryError::Render`] when the figure cannot be drawn or encoded.
    fn png(&self, figure: &ironlab::ir::Figure, dpi: f64) -> Result<Vec<u8>, GalleryError>;

    /// Exports a figure as a PDF document, with the exporter's warnings about what reached the page other than as
    /// vector geometry.
    ///
    /// # Errors
    ///
    /// Returns [`GalleryError::Pdf`] when the figure cannot be exported.
    fn pdf(&self, figure: &ironlab::ir::Figure) -> Result<ExportedPdf, GalleryError>;
}

/// A figure exported as a PDF document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportedPdf {
    /// The bytes of the document.
    pub bytes: Vec<u8>,
    /// The exporter's warnings, in words a reader can act on: what it drew as an image and why, and which
    /// three-dimensional axes it could not verify. Empty when every artist is on the page as vectors.
    pub warnings: Vec<String>,
}

/// The renderer used for the published documentation: IronLAB's own offscreen viewer pipeline for images, and its
/// PDF exporter for documents.
#[derive(Default)]
pub struct IronlabRenderer {
    text: TextEngine,
}

impl IronlabRenderer {
    /// Creates a renderer with the bundled fonts.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Renderer for IronlabRenderer {
    fn png(&self, figure: &ironlab::ir::Figure, dpi: f64) -> Result<Vec<u8>, GalleryError> {
        ironlab_canvas::render_offscreen(figure, &self.text, dpi)
            .map_err(|error| GalleryError::Render(error.to_string()))?
            .to_png()
            .map_err(|error| GalleryError::Render(error.to_string()))
    }

    fn pdf(&self, figure: &ironlab::ir::Figure) -> Result<ExportedPdf, GalleryError> {
        let options = ironlab_pdf::PdfOptions::for_figure(figure);
        let exported = ironlab_canvas::export_pdf(figure, &self.text, &options)
            .map_err(|error| GalleryError::Pdf(error.to_string()))?;
        Ok(ExportedPdf {
            bytes: exported.bytes,
            warnings: exported
                .export
                .into_iter()
                .map(|warning| warning.message)
                .collect(),
        })
    }
}

/// Options of [`generate_docs`].
pub struct DocsOptions<'a> {
    /// The renderer that produces the images and PDFs.
    pub renderer: &'a dyn Renderer,
    /// The entries to document, in the order of the index.
    pub entries: Vec<GalleryEntry>,
    /// The resolution of the full-size image on each entry page, in dots per inch.
    pub png_dpi: f64,
    /// The resolution of the thumbnail on the index, in dots per inch.
    pub thumbnail_dpi: f64,
}

impl<'a> DocsOptions<'a> {
    /// Creates options that document every gallery entry at the default resolutions.
    #[must_use]
    pub fn new(renderer: &'a dyn Renderer) -> Self {
        Self {
            renderer,
            entries: all(),
            png_dpi: DEFAULT_PNG_DPI,
            thumbnail_dpi: DEFAULT_THUMBNAIL_DPI,
        }
    }
}

/// The files written by [`generate_docs`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocsReport {
    /// The Markdown pages, starting with the index.
    pub pages: Vec<PathBuf>,
    /// The images, PDFs and `.fig` files.
    pub assets: Vec<PathBuf>,
    /// The stylesheet of the cards.
    pub stylesheet: PathBuf,
    /// Validation warnings of the figures and warnings of the PDF exporter, as `(slug, message)` pairs, in the
    /// order of the entries with each entry's validation warnings before its export warnings. Warnings do not stop
    /// generation.
    pub warnings: Vec<(String, String)>,
}

/// The name of the placeholder file that keeps the otherwise ignored gallery directory in version control. It is the
/// only file that [`generate_docs`] keeps when it clears the directory.
pub const KEEP_FILE: &str = ".gitkeep";

/// Writes the documentation gallery into `out_dir`, creating the directory if necessary.
///
/// The gallery directory belongs to the generator: every file already in it, apart from [`KEEP_FILE`], is removed
/// before the new files are written, so that the pages and assets of a renamed or removed entry do not linger in the
/// published site. Because the generator never writes subdirectories, a directory that contains one is not a gallery
/// directory (it is more likely the documentation root given by mistake), and generation is refused before anything
/// is removed.
///
/// # Errors
///
/// Returns [`GalleryError::NotGalleryDirectory`] when `out_dir` contains a subdirectory, [`GalleryError::Invalid`]
/// when a figure has validation errors, the renderer's error when a figure cannot be rendered or exported, and
/// [`GalleryError::Io`] when a file cannot be removed or written.
pub fn generate_docs(
    out_dir: &Path,
    options: &DocsOptions<'_>,
) -> Result<DocsReport, GalleryError> {
    fs::create_dir_all(out_dir).map_err(|error| GalleryError::io(out_dir, error))?;
    clear_gallery_directory(out_dir)?;
    let mut report = DocsReport::default();

    for entry in &options.entries {
        let (figure, warnings) = entry.build_validated()?;
        report.warnings.extend(
            warnings
                .into_iter()
                .map(|message| (entry.slug.to_owned(), message)),
        );

        let ir = figure.ir();
        let pdf = options.renderer.pdf(ir)?;
        report.warnings.extend(
            pdf.warnings
                .into_iter()
                .map(|message| (entry.slug.to_owned(), message)),
        );
        let assets = [
            (
                format!("{}.png", entry.slug),
                options.renderer.png(ir, options.png_dpi)?,
            ),
            (
                format!("{}-thumb.png", entry.slug),
                options.renderer.png(ir, options.thumbnail_dpi)?,
            ),
            (format!("{}.pdf", entry.slug), pdf.bytes),
            // The live figure of the entry page loads this file; it is the encoding `Figure::save` writes.
            (format!("{}.fig", entry.slug), ir.to_protobuf()),
        ];
        for (name, bytes) in assets {
            report.assets.push(write_file(&out_dir.join(name), bytes)?);
        }
        report.pages.push(write_file(
            &out_dir.join(format!("{}.md", entry.slug)),
            entry_markdown(entry),
        )?);
    }

    report.pages.insert(
        0,
        write_file(&out_dir.join("index.md"), index_markdown(&options.entries))?,
    );
    report
        .pages
        .push(write_file(&out_dir.join("fields.md"), fields_markdown())?);

    let docs_dir = match out_dir.parent() {
        Some(parent) => parent.to_path_buf(),
        None => out_dir.join(".."),
    };
    let stylesheet = docs_dir.join(GALLERY_CSS_PATH);
    if let Some(dir) = stylesheet.parent() {
        fs::create_dir_all(dir).map_err(|error| GalleryError::io(dir, error))?;
    }
    report.stylesheet = write_file(&stylesheet, GALLERY_CSS)?;

    Ok(report)
}

/// Returns the Markdown of the gallery index.
#[must_use]
pub fn index_markdown(entries: &[GalleryEntry]) -> String {
    let mut page = String::from(
        "# Gallery\n\n\
         This gallery shows the chart types and features of IronLAB. Every image in it is rendered by IronLAB's own \
         renderer, the same pipeline that draws the interactive viewer, and no other plotting or rasterising tool is \
         involved. Every entry page shows its figure live, drawn in the browser by that same engine, with the image \
         and the PDF as the fallback for a browser that cannot draw it; shows the exact code that produced the \
         figure; and offers the figure as the PDF that IronLAB exports for inclusion in a publication. The figures \
         that plot gridded or scattered data use the functions on the [data helpers](fields.md) page.\n\n\
         <div class=\"ironlab-gallery\" markdown>\n",
    );
    for entry in entries {
        let slug = entry.slug;
        let title = escape_link_text(entry.title);
        let _ = write!(
            page,
            "\n<div class=\"card\" markdown>\n\n\
             [![{title}]({slug}-thumb.png)]({slug}.md)\n\n\
             **[{title}]({slug}.md)**\n\n\
             {description}\n\n\
             </div>\n",
            description = escape_html(entry.description),
        );
    }
    page.push_str("\n</div>\n");
    page
}

/// Returns the Markdown of an entry's page.
#[must_use]
pub fn entry_markdown(entry: &GalleryEntry) -> String {
    let slug = entry.slug;
    let fence = fence_for(entry.source);
    let data_note = if uses_data_helpers(entry.source) {
        " Its data comes from the functions on the [data helpers](fields.md) page."
    } else {
        ""
    };
    // The block is raw HTML without the `markdown` attribute, so Python-Markdown passes it through verbatim; zensical
    // rewrites its URLs like Markdown links, so they are relative to this file (see the module documentation).
    format!(
        "# {title}\n\n\
         {description}\n\n\
         <div class=\"ironlab-figure\">\n\
         <ironlab-figure src=\"{slug}.fig\" name=\"{slug}\" alt=\"{alt}\">\n\
         <a href=\"{slug}.pdf\"><img alt=\"{alt}\" src=\"{slug}.png\"></a>\n\
         </ironlab-figure>\n\
         </div>\n\n\
         The figure above is live, drawn in the browser by the same engine that draws the viewer: drag it to pan, \
         and click it to zoom with the wheel. [Download the PDF]({slug}.pdf) for the vector figure that IronLAB \
         exports, or read how to [embed a figure of your own](../guides/embedding.md).\n\n\
         ## Source\n\n\
         The figure is built by the following code.{data_note}\n\n\
         {fence}rust\n{source}{newline}{fence}\n\n\
         [Back to the gallery](index.md)\n",
        title = escape_html(entry.title),
        description = escape_html(entry.description),
        alt = escape_attribute(entry.title),
        source = entry.source,
        newline = if entry.source.ends_with('\n') {
            ""
        } else {
            "\n"
        },
    )
}

/// Returns the Markdown of the data helpers page.
#[must_use]
pub fn fields_markdown() -> String {
    let fence = fence_for(FIELDS_SOURCE);
    format!(
        "# Data helpers\n\n\
         The gallery figures that plot gridded data sample a scalar field derived from the quadratic map \
         z ↦ z² + c that defines a Julia set. Starting from z₀ = x + iy, the map is applied three times with \
         c = −0.8 + 0.156i, and the field is ln(1 + |z₃|) on the square [−1.5, 1.5]². The module below also \
         provides the gradient and surface normal estimates used by the vector field figures, a deterministic \
         spiral of scattered points, and the pieces the image figures need: the real and imaginary parts of z₃, a \
         quantiser that turns the field into colormap indices, and a conversion from hue, saturation and lightness \
         to a colour.\n\n\
         {fence}rust\n{FIELDS_SOURCE}{newline}{fence}\n\n\
         [Back to the gallery](index.md)\n",
        newline = if FIELDS_SOURCE.ends_with('\n') {
            ""
        } else {
            "\n"
        },
    )
}

/// Returns whether an entry's source uses the shared data helpers, in which case its page links to their page.
fn uses_data_helpers(source: &str) -> bool {
    source.contains("crate::fields")
}

/// Removes every file in the gallery directory apart from [`KEEP_FILE`].
///
/// # Errors
///
/// Returns [`GalleryError::NotGalleryDirectory`], without removing anything, when the directory contains a
/// subdirectory, and [`GalleryError::Io`] when the directory cannot be read or a file cannot be removed.
fn clear_gallery_directory(dir: &Path) -> Result<(), GalleryError> {
    let mut stale = Vec::new();
    for item in fs::read_dir(dir).map_err(|error| GalleryError::io(dir, error))? {
        let item = item.map_err(|error| GalleryError::io(dir, error))?;
        let file_type = item
            .file_type()
            .map_err(|error| GalleryError::io(item.path(), error))?;
        if file_type.is_dir() {
            return Err(GalleryError::NotGalleryDirectory {
                dir: dir.to_path_buf(),
                subdirectory: item.path(),
            });
        }
        if item.file_name() != KEEP_FILE {
            stale.push(item.path());
        }
    }
    for path in stale {
        fs::remove_file(&path).map_err(|error| GalleryError::io(&path, error))?;
    }
    Ok(())
}

/// Returns a backtick fence longer than any run of backticks in `source`, and at least three backticks long.
fn fence_for(source: &str) -> String {
    let longest = source.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat((longest + 1).max(3))
}

/// Escapes the characters that would end or nest the text of a Markdown link.
fn escape_link_text(text: &str) -> String {
    escape_html(text).replace('[', "\\[").replace(']', "\\]")
}

/// Escapes the characters that would end or be read as markup inside a double-quoted HTML attribute.
fn escape_attribute(text: &str) -> String {
    escape_html(text).replace('"', "&quot;")
}

/// Escapes the characters that would be read as HTML.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Writes a file and returns its path.
fn write_file(path: &Path, contents: impl AsRef<[u8]>) -> Result<PathBuf, GalleryError> {
    fs::write(path, contents).map_err(|error| GalleryError::io(path, error))?;
    Ok(path.to_path_buf())
}
