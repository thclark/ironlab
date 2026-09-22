//! Generation of the documentation gallery.
//!
//! [`generate_docs`] writes a Markdown site section for the zensical documentation engine:
//!
//! - `index.md`: an introduction and a column of full-width cards, one per entry, each with a thumbnail, a title
//!   linking to the entry's page and the entry's description;
//! - `<slug>.md` for each entry: the title, the description, the rendered figure linked to its PDF, the exact source
//!   of the entry and, when the source uses the shared data helpers, a link to their page;
//! - `fields.md`: the source of the shared data helpers;
//! - `<slug>.png`, `<slug>-thumb.png` and `<slug>.pdf` for each entry, produced by a [`Renderer`];
//! - `../stylesheets/gallery.css`, the stylesheet of the cards, which the site configuration must list in
//!   `extra_css`.
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

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use ironlab_text::TextEngine;
use ironlab_viewer::RenderedImage;

use crate::error::GalleryError;
use crate::fields::FIELDS_SOURCE;
use crate::{GalleryEntry, all};

/// The resolution of the full-size figure image on each entry page, in dots per inch.
pub const DEFAULT_PNG_DPI: f64 = 150.0;

/// The resolution of the thumbnail image on the gallery index, in dots per inch.
///
/// The stylesheet shows a thumbnail at most 14 rem wide, which is at most 280 CSS pixels in the documentation theme. A
/// gallery figure is about 120 mm wide, so this resolution gives a thumbnail about 570 pixels wide: at least two image
/// pixels for every CSS pixel, which keeps the lines and the text of the figure sharp on a high-density display.
pub const DEFAULT_THUMBNAIL_DPI: f64 = 120.0;

/// The path of the gallery stylesheet relative to the documentation directory, as it must appear in the `extra_css`
/// list of `zensical.toml`.
pub const GALLERY_CSS_PATH: &str = "stylesheets/gallery.css";

/// The stylesheet of the gallery cards, written to [`GALLERY_CSS_PATH`] in the documentation directory.
///
/// Colours come from the theme's custom properties, so the cards follow the light and dark palettes.
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

    /// Exports a figure as a PDF document.
    ///
    /// # Errors
    ///
    /// Returns [`GalleryError::Pdf`] when the figure cannot be exported.
    fn pdf(&self, figure: &ironlab::ir::Figure) -> Result<Vec<u8>, GalleryError>;
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
        let image = ironlab_viewer::render_offscreen(figure, &self.text, dpi)
            .map_err(|error| GalleryError::Render(error.to_string()))?;
        encode_png(&image)
    }

    fn pdf(&self, figure: &ironlab::ir::Figure) -> Result<Vec<u8>, GalleryError> {
        let options = ironlab_pdf::PdfOptions::for_figure(figure);
        ironlab_viewer::export_pdf(figure, &self.text, &options)
            .map(|exported| exported.bytes)
            .map_err(|error| GalleryError::Pdf(error.to_string()))
    }
}

/// Encodes a rendered RGBA image as a PNG file.
///
/// # Errors
///
/// Returns [`GalleryError::Render`] when the pixel buffer does not match the image size.
pub fn encode_png(image: &RenderedImage) -> Result<Vec<u8>, GalleryError> {
    use image::ImageEncoder as _;

    let expected = u64::from(image.width) * u64::from(image.height) * 4;
    if image.rgba.len() as u64 != expected {
        return Err(GalleryError::Render(format!(
            "cannot encode PNG: a {}×{} image needs {expected} RGBA bytes, but {} were given",
            image.width,
            image.height,
            image.rgba.len()
        )));
    }
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &image.rgba,
            image.width,
            image.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|error| GalleryError::Render(format!("cannot encode PNG: {error}")))?;
    Ok(bytes)
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
    /// The images and PDFs.
    pub assets: Vec<PathBuf>,
    /// The stylesheet of the cards.
    pub stylesheet: PathBuf,
    /// Validation warnings of the figures, as `(slug, message)` pairs. Warnings do not stop generation.
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
        let assets = [
            (
                format!("{}.png", entry.slug),
                options.renderer.png(ir, options.png_dpi)?,
            ),
            (
                format!("{}-thumb.png", entry.slug),
                options.renderer.png(ir, options.thumbnail_dpi)?,
            ),
            (format!("{}.pdf", entry.slug), options.renderer.pdf(ir)?),
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
         involved. Every entry page shows the exact code that produced its figure, and offers the figure as the PDF \
         that IronLAB exports for inclusion in a publication. The figures that plot gridded or scattered data use \
         the functions on the [data helpers](fields.md) page.\n\n\
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
    format!(
        "# {title}\n\n\
         {description}\n\n\
         <div class=\"ironlab-figure\" markdown>\n\n\
         [![{alt}]({slug}.png)]({slug}.pdf)\n\n\
         </div>\n\n\
         Select the image, or [download the PDF]({slug}.pdf), to open the vector figure exported by IronLAB.\n\n\
         ## Source\n\n\
         The figure is built by the following code.{data_note}\n\n\
         {fence}rust\n{source}{newline}{fence}\n\n\
         [Back to the gallery](index.md)\n",
        title = escape_html(entry.title),
        description = escape_html(entry.description),
        alt = escape_link_text(entry.title),
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
