//! End-to-end test of the documentation gallery with IronLAB's real renderer and a strict zensical build.
//!
//! WHY: the unit tests of the generator use a fake renderer and cannot run zensical. This test proves that every
//! gallery figure renders to a PNG of the expected size and exports to a PDF, and that the generated pages build
//! into a site with no broken links, exactly as `scripts/build-docs.sh` does.
//!
//! It is ignored by default because it needs a graphics adapter (a GPU or a software rasteriser such as lavapipe),
//! network access for `uvx` on first use, and takes a while. Run it with:
//!
//! ```sh
//! cargo test --release -p ironlab-gallery --test end_to_end -- --ignored --nocapture
//! ```

mod common;

use std::fs;
use std::process::Command;

use common::temp_dir;
use ironlab_gallery::docs::{DEFAULT_PNG_DPI, DEFAULT_THUMBNAIL_DPI, GALLERY_CSS_PATH};
use ironlab_gallery::{DocsOptions, IronlabRenderer, all, generate_docs};

/// The zensical version pinned by `scripts/build-docs.sh`.
const ZENSICAL: &str = "zensical==0.0.62";

fn expected_pixels(size_mm: f64, dpi: f64) -> u32 {
    (size_mm / 25.4 * dpi).round() as u32
}

#[test]
#[ignore = "needs a graphics adapter and uvx; run with --ignored"]
fn real_gallery_renders_and_builds_with_zensical_strict() {
    let site = temp_dir("end-to-end");
    let docs = site.join("docs");
    let gallery = docs.join("gallery");
    fs::create_dir_all(&gallery).unwrap();
    fs::write(
        site.join("zensical.toml"),
        format!(
            "[project]\nsite_name = \"IronLAB gallery test\"\ndocs_dir = \"docs\"\nsite_dir = \"site\"\n\
             extra_css = [\"{GALLERY_CSS_PATH}\"]\n"
        ),
    )
    .unwrap();
    fs::write(
        docs.join("index.md"),
        "# IronLAB\n\nSee the [gallery](gallery/index.md).\n",
    )
    .unwrap();

    let renderer = IronlabRenderer::new();
    let report =
        generate_docs(&gallery, &DocsOptions::new(&renderer)).expect("the gallery generates");
    for (slug, message) in &report.warnings {
        eprintln!("warning: {slug}: {message}");
    }

    for entry in all() {
        let figure = (entry.build)().into_ir();
        for (name, dpi) in [
            (format!("{}.png", entry.slug), DEFAULT_PNG_DPI),
            (format!("{}-thumb.png", entry.slug), DEFAULT_THUMBNAIL_DPI),
        ] {
            let image = image::open(gallery.join(&name))
                .unwrap_or_else(|e| panic!("{name} does not decode: {e}"));
            assert_eq!(
                (image.width(), image.height()),
                (
                    expected_pixels(figure.size.width_mm, dpi),
                    expected_pixels(figure.size.height_mm, dpi)
                ),
                "{name} has the wrong size"
            );
        }
        let pdf = fs::read(gallery.join(format!("{}.pdf", entry.slug))).unwrap();
        assert!(pdf.starts_with(b"%PDF-"), "{}.pdf is not a PDF", entry.slug);
    }

    let output = Command::new("uvx")
        .args([
            "--from", ZENSICAL, "zensical", "build", "--clean", "--strict",
        ])
        .current_dir(&site)
        .env("NO_COLOR", "1")
        .output()
        .expect("uvx runs");
    assert!(
        output.status.success(),
        "zensical build --strict failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for entry in all() {
        assert!(
            site.join("site/gallery")
                .join(entry.slug)
                .join("index.html")
                .is_file()
        );
    }
    println!("site built in {}", site.display());
}
