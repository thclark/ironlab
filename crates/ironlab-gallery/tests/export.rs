//! Tests of `gallery export`.
//!
//! WHY: the exported files are the inputs of the LaTeX inclusion check in CI and of the visual inspection of the
//! gallery, so each figure must produce its PDF, a PNG, and the figure itself in both file formats (the default `.fig`
//! and the secondary `.fig.json`), each of which reloads as the same figure.

mod common;

use std::fs;

use common::{FakeRenderer, temp_dir};
use ironlab::Figure;
use ironlab_gallery::{GalleryEntry, export_entries};

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

/// WHY: each entry must yield exactly the four files the CI jobs and the reviewer look for, and both figure files must
/// reload as the figure that was built, since the viewer opens either format and other tools read them.
#[test]
fn export_writes_pdf_png_and_reloadable_figure_files_per_entry() {
    let out = temp_dir("export");
    let renderer = FakeRenderer::default();
    let written = export_entries(&out, &entries(), &renderer, 150.0).expect("export succeeds");

    let names: Vec<String> = written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "exported.pdf",
            "exported.fig",
            "exported.fig.json",
            "exported.png"
        ]
    );

    assert_eq!(
        fs::read(out.join("exported.pdf")).unwrap(),
        FakeRenderer::pdf_bytes(titled().ir())
    );
    assert_eq!(
        fs::read(out.join("exported.png")).unwrap(),
        FakeRenderer::png_bytes(titled().ir(), 150.0)
    );

    let bytes = fs::read(out.join("exported.fig")).unwrap();
    let from_protobuf = ironlab::ir::Figure::from_protobuf(&bytes)
        .expect("the exported .fig file is a protobuf figure");
    assert_eq!(from_protobuf, titled().into_ir());

    let json = fs::read_to_string(out.join("exported.fig.json")).unwrap();
    let from_json = ironlab::ir::Figure::from_json(&json)
        .expect("the exported .fig.json file is a JSON figure");
    assert_eq!(from_json, titled().into_ir());
}
