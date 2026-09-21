//! Gallery of IronLAB example figures, with a docs generator.
//!
//! Each gallery entry is one source file in `src/figures/`, written only against the public [`ironlab`] API so that
//! it reads as user code. The [`gallery!`] macro registers the entries and captures the text of each file with
//! `include_str!`, so the source shown in the documentation is exactly the code that produced the rendered figure.
//! Data helpers shared by the entries live in [`fields`].
//!
//! The gallery serves three purposes: its figures are the fixtures of the rendering and export tests, the `gallery`
//! binary opens them in the viewer or exports them, and [`docs::generate_docs`] builds the documentation gallery.

pub mod docs;
pub mod error;
pub mod export;
pub mod fields;

pub use docs::{DocsOptions, DocsReport, IronlabRenderer, Renderer, generate_docs};
pub use error::GalleryError;
pub use export::export_entries;

/// One figure of the gallery.
#[derive(Clone, Copy, Debug)]
pub struct GalleryEntry {
    /// The identifier of the entry, which is also the name of its source file and of its documentation page.
    pub slug: &'static str,
    /// The title of the entry's documentation page.
    pub title: &'static str,
    /// One or two sentences describing what the figure demonstrates.
    pub description: &'static str,
    /// The complete text of the entry's source file.
    pub source: &'static str,
    /// Builds the figure.
    pub build: fn() -> ironlab::Figure,
}

impl GalleryEntry {
    /// Builds the entry's figure and validates it, returning the figure with the messages of its validation warnings.
    ///
    /// # Errors
    ///
    /// Returns [`GalleryError::Invalid`], naming the entry, when the figure has validation errors.
    pub fn build_validated(&self) -> Result<(ironlab::Figure, Vec<String>), GalleryError> {
        let figure = (self.build)();
        let validation = figure.validate();
        if !validation.is_valid() {
            return Err(GalleryError::Invalid {
                slug: self.slug.to_owned(),
                messages: validation
                    .errors
                    .into_iter()
                    .map(|issue| issue.message)
                    .collect(),
            });
        }
        let warnings = validation
            .warnings
            .into_iter()
            .map(|issue| issue.message)
            .collect();
        Ok((figure, warnings))
    }
}

/// Registers gallery entries.
///
/// For each slug, the macro declares the module `figures::<slug>` from `src/figures/<slug>.rs`, and adds an entry
/// to [`all`] built from the module's `TITLE`, `DESCRIPTION` and `figure` items and the text of the file.
macro_rules! gallery {
    ($($slug:ident),+ $(,)?) => {
        /// The source modules of the gallery entries, one per figure.
        pub mod figures {
            $(pub mod $slug;)+
        }

        /// Returns every gallery entry, in the order in which the gallery presents them.
        #[must_use]
        pub fn all() -> Vec<GalleryEntry> {
            vec![$(
                GalleryEntry {
                    slug: stringify!($slug),
                    title: figures::$slug::TITLE,
                    description: figures::$slug::DESCRIPTION,
                    source: include_str!(concat!("figures/", stringify!($slug), ".rs")),
                    build: figures::$slug::figure,
                },
            )+]
        }
    };
}

gallery!(
    line_markers,
    scatter_2d,
    decimated_timeseries,
    log_axes,
    latex_labels,
    legend_toggle,
    contour,
    contourf,
    quiver,
    image,
    mapped_image,
    indexed_image,
    surf,
    mesh,
    scatter3,
    contour3,
    quiver3,
    mapped_image_and_surface,
    image_planes,
    subplots_unlinked,
    subplots_linked,
    subplots_all_x,
    subplots_all_y,
);

/// Returns the gallery entry with the given slug, if there is one.
#[must_use]
pub fn find(slug: &str) -> Option<GalleryEntry> {
    all().into_iter().find(|entry| entry.slug == slug)
}
