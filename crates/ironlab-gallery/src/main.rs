//! The `gallery` command: view, export and document the IronLAB example figures.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ironlab_gallery::docs::DEFAULT_PNG_DPI;
use ironlab_gallery::{
    DocsOptions, GalleryEntry, GalleryError, IronlabRenderer, all, export_entries, find,
    generate_docs,
};

/// View, export and document the IronLAB gallery figures.
#[derive(Debug, Parser)]
#[command(name = "gallery", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Open gallery figures in the interactive viewer, one tab per figure.
    View {
        /// The slugs of the figures to open; every figure is opened when none is given.
        slugs: Vec<String>,
    },
    /// Write a PDF, a `.fig.json` file and a PNG image of each gallery figure into a directory.
    Export {
        /// The directory to write into, which is created if necessary.
        dir: PathBuf,
        /// The slugs of the figures to export; every figure is exported when none is given.
        #[arg(long = "slug")]
        slugs: Vec<String>,
        /// The resolution of the PNG images in dots per inch.
        #[arg(long, default_value_t = DEFAULT_PNG_DPI)]
        dpi: f64,
    },
    /// Generate the documentation gallery (Markdown pages, images and PDFs) into a directory.
    Docs {
        /// The gallery directory of the documentation, such as `docs/gallery`.
        dir: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::View { slugs } => {
            let figures = select(&slugs)?
                .into_iter()
                .map(|entry| (entry.title.to_owned(), (entry.build)().into_ir()))
                .collect();
            ironlab_viewer::run(figures)?;
        }
        Command::Export { dir, slugs, dpi } => {
            let renderer = IronlabRenderer::new();
            for path in export_entries(&dir, &select(&slugs)?, &renderer, dpi)? {
                println!("wrote {}", path.display());
            }
        }
        Command::Docs { dir } => {
            let renderer = IronlabRenderer::new();
            let report = generate_docs(&dir, &DocsOptions::new(&renderer))?;
            for (slug, message) in &report.warnings {
                eprintln!("warning: {slug}: {message}");
            }
            println!(
                "wrote {} pages and {} assets into {}, and the stylesheet {}",
                report.pages.len(),
                report.assets.len(),
                dir.display(),
                report.stylesheet.display()
            );
        }
    }
    Ok(())
}

/// Returns the entries named by `slugs`, in the given order, or every entry when no slug is given.
fn select(slugs: &[String]) -> Result<Vec<GalleryEntry>, GalleryError> {
    if slugs.is_empty() {
        return Ok(all());
    }
    slugs
        .iter()
        .map(|slug| find(slug).ok_or_else(|| GalleryError::UnknownSlug(slug.clone())))
        .collect()
}
