//! Tests of `gallery export`.
//!
//! WHY: the exported files are the inputs of the LaTeX inclusion check in CI and of the visual inspection of the
//! gallery, so each figure must produce its PDF, a PNG and a `.fig.json` file that reloads as the same figure.

mod common;

use std::fs;

use common::{FakeRenderer, temp_dir};
use ironlab::Figure;
use ironlab_gallery::docs::encode_png;
use ironlab_gallery::{GalleryEntry, export_entries};
use ironlab_viewer::RenderedImage;

fn titled() -> Figure {
    Figure::new().size_mm(80.0, 50.0).title("Exported figure")
}

fn entries() -> Vec<GalleryEntry> {
    vec![GalleryEntry {
        slug: "exported",
        title: "Exported",
        description: "A synthetic entry.",
        source: "",
        build: titled,
    }]
}

/// WHY: each entry must yield exactly the three files the CI jobs and the reviewer look for, and the JSON must
/// reload as the figure that was built, since the viewer opens these files.
#[test]
fn export_writes_pdf_png_and_reloadable_json_per_entry() {
    let out = temp_dir("export");
    let renderer = FakeRenderer::default();
    let written = export_entries(&out, &entries(), &renderer, 150.0).expect("export succeeds");

    let names: Vec<String> = written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["exported.pdf", "exported.fig.json", "exported.png"]);

    assert_eq!(
        fs::read(out.join("exported.pdf")).unwrap(),
        FakeRenderer::pdf_bytes(titled().ir())
    );
    assert_eq!(
        fs::read(out.join("exported.png")).unwrap(),
        FakeRenderer::png_bytes(titled().ir(), 150.0)
    );
    let json = fs::read_to_string(out.join("exported.fig.json")).unwrap();
    let reloaded = ironlab::ir::Figure::from_json(&json).expect("the exported JSON is a figure");
    assert_eq!(reloaded, titled().into_ir());
}

/// WHY: the PNG files must decode to the rendered pixels at the rendered size, or the gallery images would be
/// distorted or corrupt.
#[test]
fn encode_png_preserves_size_and_pixels() {
    let (width, height) = (3, 2);
    let rgba: Vec<u8> = (0..width * height * 4)
        .map(|i| (i * 11 % 256) as u8)
        .collect();
    let image = RenderedImage {
        width,
        height,
        rgba: rgba.clone(),
    };
    let bytes = encode_png(&image).expect("the image encodes");
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .expect("the PNG decodes")
        .to_rgba8();
    assert_eq!(decoded.dimensions(), (width, height));
    assert_eq!(decoded.into_raw(), rgba);
}

/// WHY: a pixel buffer that does not match the stated size must be reported, not written as a corrupt file.
#[test]
fn encode_png_rejects_a_buffer_of_the_wrong_length() {
    let image = RenderedImage {
        width: 4,
        height: 4,
        rgba: vec![0; 10],
    };
    assert!(encode_png(&image).is_err());
}
