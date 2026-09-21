//! Tests of the documentation gallery generator.
//!
//! WHY: the generated pages are published as the project's gallery, and zensical only checks links between pages,
//! not images or downloads. These tests check the promises of the gallery: the index lists every entry, each page
//! shows exactly the code that produced its figure, and every link and image resolves to a generated file. A fake
//! renderer is used so that the generator is tested without a graphics adapter; the real renderer is exercised by
//! the ignored end-to-end test.

mod common;

use std::fs;
use std::path::Path;

use common::{
    FakeRenderer, file_names, is_external, link_targets, read, rust_fence, temp_dir, without_fences,
};
use ironlab::Figure;
use ironlab_gallery::docs::{
    DEFAULT_PNG_DPI, DEFAULT_THUMBNAIL_DPI, GALLERY_CSS, GALLERY_CSS_PATH, KEEP_FILE,
    entry_markdown, index_markdown,
};
use ironlab_gallery::fields::FIELDS_SOURCE;
use ironlab_gallery::{DocsOptions, GalleryEntry, GalleryError, all, generate_docs};

fn first() -> Figure {
    Figure::new().title("First figure")
}

fn second() -> Figure {
    Figure::new().title("Second figure")
}

fn invalid() -> Figure {
    Figure::new().size_mm(-10.0, 50.0).title("Invalid figure")
}

/// Entries that build without the plotting functions of the facade, so that the generator is tested on its own. The
/// entry titles differ from the figure titles, so that a generator that confused the two would be caught.
fn synthetic_entries() -> Vec<GalleryEntry> {
    vec![
        GalleryEntry {
            slug: "first",
            title: "First [entry]",
            description: "The first synthetic entry, whose title contains brackets.",
            source: "use ironlab::prelude::*;\n\nuse crate::fields::*;\n\npub fn figure() -> Figure {\n    Figure::new()\n}\n",
            build: first,
        },
        GalleryEntry {
            slug: "second",
            title: "Second entry",
            description: "A synthetic entry whose description mentions the <b> tag, and whose source contains a fenced \
                          block in a doc comment and uses no data helpers.",
            source: "/// ```\n/// let a = 1;\n/// ```\n/// Also ```` four ticks.\npub fn figure() -> Figure {\n    Figure::new()\n}",
            build: second,
        },
    ]
}

/// Generates the documentation of the synthetic entries with a fake renderer into a new directory, and returns the
/// directory.
fn generate_synthetic(name: &str) -> std::path::PathBuf {
    let out = temp_dir(name);
    let renderer = FakeRenderer::default();
    let options = DocsOptions {
        entries: synthetic_entries(),
        ..DocsOptions::new(&renderer)
    };
    generate_docs(&out, &options).expect("docs are generated");
    out
}

/// Returns the Markdown of each card of the index, in order.
fn cards(index: &str) -> Vec<String> {
    const CARD: &str = "<div class=\"card\" markdown>";
    index
        .split(CARD)
        .skip(1)
        .map(|card| card.split("</div>").next().unwrap_or_default().to_owned())
        .collect()
}

fn escaped_link_text(title: &str) -> String {
    title.replace('[', "\\[").replace(']', "\\]")
}

/// Checks that every link and image target in every generated page resolves to a generated file.
fn assert_links_resolve(out_dir: &Path, pages: &[&str]) {
    let mut broken = Vec::new();
    for page in pages {
        let text = read(&out_dir.join(page));
        let targets = link_targets(&text);
        assert!(!targets.is_empty(), "{page} has no links");
        for target in targets.iter().filter(|t| !is_external(t)) {
            let path = target.split('#').next().unwrap_or_default();
            if !out_dir.join(path).is_file() {
                broken.push(format!("{page}: {target}"));
            }
        }
    }
    assert!(broken.is_empty(), "broken links:\n{}", broken.join("\n"));
}

/// WHY: readers find figures through the index, so each card must show the thumbnail of its own entry, link both the
/// thumbnail and the title to that entry's page, and carry that entry's description. Checking each card on its own
/// catches a generator that pairs a title with another entry's thumbnail or page.
#[test]
fn each_index_card_shows_and_links_its_own_entry() {
    let out = generate_synthetic("docs-index");
    let index = read(&out.join("index.md"));
    assert!(
        index.contains("<div class=\"ironlab-gallery\" markdown>"),
        "the card grid is missing:\n{index}"
    );

    let entries = synthetic_entries();
    let cards = cards(&index);
    assert_eq!(
        cards.len(),
        entries.len(),
        "there must be one card per entry"
    );
    for (card, entry) in cards.iter().zip(&entries) {
        let slug = entry.slug;
        assert!(
            card.contains(&format!("({slug}-thumb.png)]({slug}.md)")),
            "the card of {slug} has no thumbnail linked to its page:\n{card}"
        );
        assert!(
            card.contains(&format!("[{}]({slug}.md)", escaped_link_text(entry.title))),
            "the card of {slug} has no title linked to its page:\n{card}"
        );
        let other = entries
            .iter()
            .find(|e| e.slug != slug)
            .expect("two entries");
        assert!(
            !card.contains(&format!("{}.md", other.slug))
                && !card.contains(&format!("{}-thumb", other.slug)),
            "the card of {slug} refers to {}:\n{card}",
            other.slug
        );
    }
    assert!(cards[0].contains(entries[0].description));
}

/// WHY: titles and descriptions are plain text; any HTML-like text in them must be shown literally, not interpreted
/// as markup that could break the card grid or the page.
#[test]
fn html_in_descriptions_is_shown_as_text() {
    let out = generate_synthetic("docs-escape");
    for page in ["index.md", "second.md"] {
        let text = without_fences(&read(&out.join(page)));
        assert!(
            !text.contains("<b>"),
            "{page} passes the description's <b> through as HTML"
        );
        assert!(
            text.contains("mentions the &lt;b&gt; tag"),
            "{page} does not show the description"
        );
    }
}

/// WHY: a description is Markdown, and an entry that shows the work of another author credits it with links to the
/// source and to the licence. The links must reach the reader on the entry page and on the index card. HTML does not
/// allow a link inside another link, so on the card the description must stay a paragraph of its own, outside the
/// links that the thumbnail and the title make to the entry page; a generator that wrapped the whole card in one link
/// would silently break the credit.
#[test]
fn links_in_a_description_are_kept_and_never_nested_in_a_card_link() {
    const LINK: &str = "[the source](https://example.com/photo.jpg)";
    let entry = GalleryEntry {
        slug: "credited",
        title: "Credited entry",
        description: "A synthetic entry that credits [the source](https://example.com/photo.jpg) of its data.",
        source: "pub fn figure() -> Figure {\n    Figure::new()\n}\n",
        build: first,
    };

    let page = without_fences(&entry_markdown(&entry));
    assert!(
        page.contains(LINK),
        "the entry page does not keep the link of the description"
    );

    let index = index_markdown(&[entry]);
    let cards = cards(&index);
    assert_eq!(cards.len(), 1);
    let paragraphs: Vec<&str> = cards[0]
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .collect();
    assert_eq!(
        paragraphs.len(),
        3,
        "a card is the thumbnail, the title and the description, each a paragraph of its own: {paragraphs:?}"
    );
    assert_eq!(
        paragraphs[2], entry.description,
        "the description must be a paragraph of its own, with its link unchanged and outside every other link"
    );
    assert!(
        !paragraphs[..2].iter().any(|p| p.contains("example.com")),
        "the link of the description appears inside the thumbnail or title link"
    );
}

/// WHY: the documentation promises that the code shown is exactly the code that produced the figure, including
/// sources that themselves contain backtick fences, which must not end the code block early. Each page must also show
/// the image and PDF of its own entry.
#[test]
fn each_page_embeds_the_exact_source_in_a_rust_fence() {
    let out = generate_synthetic("docs-source");
    for entry in synthetic_entries() {
        let page = read(&out.join(format!("{}.md", entry.slug)));
        assert!(
            page.starts_with(&format!("# {}\n", entry.title)),
            "the page of {} has no title heading",
            entry.slug
        );
        let shown = rust_fence(&page)
            .unwrap_or_else(|| panic!("the page of {} has no rust fence", entry.slug));
        let expected = if entry.source.ends_with('\n') {
            entry.source.to_owned()
        } else {
            format!("{}\n", entry.source)
        };
        assert_eq!(
            shown, expected,
            "the page of {} does not show its exact source",
            entry.slug
        );
        assert!(
            page.contains(&format!(
                "[![{}]({}.png)]({}.pdf)",
                escaped_link_text(entry.title),
                entry.slug,
                entry.slug
            )),
            "the page of {} does not show its own image linked to its own PDF",
            entry.slug
        );
    }
    assert!(read(&out.join("first.md")).contains(synthetic_entries()[0].description));
}

/// WHY: a reader who copies an example that uses `crate::fields` needs to find those functions, but a page whose code
/// uses no helpers must not claim that its data comes from them.
#[test]
fn only_pages_whose_source_uses_the_data_helpers_link_to_them() {
    let out = generate_synthetic("docs-fields-link");
    let links = |slug: &str| {
        link_targets(&read(&out.join(format!("{slug}.md"))))
            .iter()
            .any(|target| target == "fields.md")
    };
    assert!(
        links("first"),
        "the page of an entry using the helpers does not link to them"
    );
    assert!(
        !links("second"),
        "the page of an entry not using the helpers links to them"
    );
}

/// WHY: zensical validates links between pages but not images, downloads or links inside hand-written HTML, so a
/// missing asset would only be noticed by a reader.
#[test]
fn every_relative_link_and_image_resolves_to_a_generated_file() {
    let out_root = temp_dir("docs-links");
    let out = out_root.join("docs").join("gallery");
    let renderer = FakeRenderer::default();
    let options = DocsOptions {
        entries: synthetic_entries(),
        ..DocsOptions::new(&renderer)
    };
    let report = generate_docs(&out, &options).expect("docs are generated");

    assert_links_resolve(&out, &["index.md", "first.md", "second.md", "fields.md"]);
    for page in &report.pages {
        assert!(
            page.is_file(),
            "the report lists {} but it was not written",
            page.display()
        );
    }
    for asset in &report.assets {
        assert!(
            asset.is_file(),
            "the report lists {} but it was not written",
            asset.display()
        );
    }
}

/// WHY: the site configuration lists the stylesheet by its path in the docs directory, so it must be written there,
/// one level above the gallery directory, with the card grid rules.
#[test]
fn stylesheet_is_written_beside_the_gallery_directory() {
    let out_root = temp_dir("docs-css");
    let out = out_root.join("docs").join("gallery");
    let renderer = FakeRenderer::default();
    let options = DocsOptions {
        entries: synthetic_entries(),
        ..DocsOptions::new(&renderer)
    };
    let report = generate_docs(&out, &options).expect("docs are generated");

    let expected = out_root.join("docs").join(GALLERY_CSS_PATH);
    assert_eq!(
        fs::canonicalize(&report.stylesheet).unwrap(),
        fs::canonicalize(&expected).unwrap()
    );
    assert_eq!(read(&expected), GALLERY_CSS);
    assert!(GALLERY_CSS.contains(".ironlab-gallery"));
    assert!(GALLERY_CSS.contains(".card"));
}

/// WHY: the entry page shows a full-resolution image and the index a thumbnail at a lower resolution that is still
/// sharp at its displayed width, so that the index stays light while each page is sharp; each asset must be rendered
/// from its own figure at the right resolution.
#[test]
fn assets_are_rendered_from_their_figure_at_the_configured_resolutions() {
    let renderer = FakeRenderer::default();
    let options = DocsOptions::new(&renderer);
    assert_eq!((options.png_dpi, options.thumbnail_dpi), (150.0, 120.0));
    assert_eq!((DEFAULT_PNG_DPI, DEFAULT_THUMBNAIL_DPI), (150.0, 120.0));

    let out = generate_synthetic("docs-assets");
    for entry in synthetic_entries() {
        let slug = entry.slug;
        let figure = (entry.build)().into_ir();
        assert_eq!(
            fs::read(out.join(format!("{slug}.png"))).unwrap(),
            FakeRenderer::png_bytes(&figure, 150.0),
            "{slug}.png is not the full-size render of its figure"
        );
        assert_eq!(
            fs::read(out.join(format!("{slug}-thumb.png"))).unwrap(),
            FakeRenderer::png_bytes(&figure, 120.0),
            "{slug}-thumb.png is not the thumbnail of its figure"
        );
        assert_eq!(
            fs::read(out.join(format!("{slug}.pdf"))).unwrap(),
            FakeRenderer::pdf_bytes(&figure),
            "{slug}.pdf is not the export of its figure"
        );
    }
}

/// WHY: when an entry is renamed or removed, its old page and images must not stay in the published site, where they
/// would show outdated code. The generator therefore owns the gallery directory: after generation it holds exactly
/// the files reported, plus the `.gitkeep` placeholder that keeps the directory in version control.
#[test]
fn stale_files_are_removed_and_the_keep_file_is_preserved() {
    let out = temp_dir("docs-stale");
    fs::write(out.join(KEEP_FILE), "kept\n").unwrap();
    for stale in [
        "renamed.md",
        "renamed.png",
        "renamed-thumb.png",
        "renamed.pdf",
        "notes.txt",
    ] {
        fs::write(out.join(stale), "stale").unwrap();
    }
    let renderer = FakeRenderer::default();
    let options = DocsOptions {
        entries: synthetic_entries(),
        ..DocsOptions::new(&renderer)
    };
    let report = generate_docs(&out, &options).expect("docs are generated");

    let mut expected: Vec<String> = report
        .pages
        .iter()
        .chain(&report.assets)
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .chain([KEEP_FILE.to_owned()])
        .collect();
    expected.sort();
    assert_eq!(file_names(&out), expected);
    assert_eq!(
        read(&out.join(KEEP_FILE)),
        "kept\n",
        "the keep file is untouched"
    );
}

/// WHY: clearing the output directory is destructive, so pointing the generator at the documentation root by mistake
/// (which contains subdirectories, unlike a gallery directory) must fail without deleting anything.
#[test]
fn a_directory_with_subdirectories_is_refused_without_removing_anything() {
    let docs = temp_dir("docs-refuse");
    fs::write(docs.join("index.md"), "# Home\n").unwrap();
    fs::create_dir_all(docs.join("gallery")).unwrap();
    let renderer = FakeRenderer::default();
    let options = DocsOptions {
        entries: synthetic_entries(),
        ..DocsOptions::new(&renderer)
    };
    match generate_docs(&docs, &options) {
        Err(GalleryError::NotGalleryDirectory { .. }) => {}
        other => panic!("expected the directory to be refused, got {other:?}"),
    }
    assert_eq!(file_names(&docs), ["gallery", "index.md"]);
    assert!(renderer.calls.borrow().is_empty(), "nothing is rendered");
}

/// WHY: the data helpers page is linked from every entry and must show the helpers' actual source.
#[test]
fn fields_page_shows_the_fields_source() {
    let out = temp_dir("docs-fields");
    let renderer = FakeRenderer::default();
    let options = DocsOptions {
        entries: synthetic_entries(),
        ..DocsOptions::new(&renderer)
    };
    generate_docs(&out, &options).expect("docs are generated");

    let page = read(&out.join("fields.md"));
    assert!(
        page.starts_with("# "),
        "the data helpers page has no title heading"
    );
    let shown = rust_fence(&page).expect("the data helpers page has a rust fence");
    assert_eq!(shown.trim_end(), FIELDS_SOURCE.trim_end());
}

/// WHY: the docs build runs in CI, and a gallery figure with validation errors must fail it with the entry named,
/// rather than publish a broken figure.
#[test]
fn an_invalid_figure_fails_generation_and_names_the_entry() {
    let out = temp_dir("docs-invalid");
    let renderer = FakeRenderer::default();
    let mut entries = synthetic_entries();
    entries.push(GalleryEntry {
        slug: "broken",
        title: "Broken",
        description: "An entry with a negative width.",
        source: "",
        build: invalid,
    });
    let options = DocsOptions {
        entries,
        ..DocsOptions::new(&renderer)
    };
    match generate_docs(&out, &options) {
        Err(GalleryError::Invalid { slug, messages }) => {
            assert_eq!(slug, "broken");
            assert!(!messages.is_empty());
        }
        other => panic!("expected an invalid-figure error, got {other:?}"),
    }
    assert!(
        !renderer
            .calls
            .borrow()
            .iter()
            .any(|call| call.contains("Invalid figure")),
        "an invalid figure must not be rendered"
    );
}

/// WHY: the real gallery is what gets published, so the same promises must hold for every registered entry.
#[test]
fn the_real_gallery_generates_complete_pages() {
    let out_root = temp_dir("docs-real");
    let out = out_root.join("docs").join("gallery");
    let renderer = FakeRenderer::default();
    let report = generate_docs(&out, &DocsOptions::new(&renderer)).expect("docs are generated");

    let entries = all();
    assert_eq!(
        report.pages.len(),
        entries.len() + 2,
        "one page per entry, the index and the data helpers page"
    );
    assert_eq!(
        report.assets.len(),
        entries.len() * 3,
        "a PNG, a thumbnail and a PDF per entry"
    );

    let index = read(&out.join("index.md"));
    let mut pages = vec!["index.md".to_owned(), "fields.md".to_owned()];
    for entry in &entries {
        assert!(
            index.contains(entry.title),
            "the index does not name {}",
            entry.title
        );
        assert!(index.contains(&format!("({}-thumb.png)]({}.md)", entry.slug, entry.slug)));
        let figure = (entry.build)().into_ir();
        assert_eq!(
            fs::read(out.join(format!("{}.png", entry.slug))).unwrap(),
            FakeRenderer::png_bytes(&figure, DEFAULT_PNG_DPI),
            "{}.png is not the render of its own figure",
            entry.slug
        );
        let page = read(&out.join(format!("{}.md", entry.slug)));
        assert_eq!(
            link_targets(&page).iter().any(|t| t == "fields.md"),
            entry.source.contains("crate::fields"),
            "the page of {} links to the data helpers if and only if its source uses them",
            entry.slug
        );
        assert_eq!(
            rust_fence(&page).as_deref(),
            Some(entry.source),
            "the page of {} does not show its source",
            entry.slug
        );
        pages.push(format!("{}.md", entry.slug));
    }
    let pages: Vec<&str> = pages.iter().map(String::as_str).collect();
    assert_links_resolve(&out, &pages);
}

/// WHY: the link resolution tests are only as strong as the link parser; if it missed a form of link, a broken link
/// of that form would pass unnoticed.
#[test]
fn link_parser_finds_every_form_of_link_outside_code() {
    let page = "[![alt](thumb.png)](page.md) and [text](other.md#part)\n\
                <a href=\"raw.md\"><img src=\"raw.png\"></a>\n\
                ```rust\nlet ignored = \"[x](inside_code.md)\";\n```\n\
                [site](https://example.com)\n";
    let targets = link_targets(page);
    for expected in [
        "thumb.png",
        "page.md",
        "other.md#part",
        "raw.md",
        "raw.png",
        "https://example.com",
    ] {
        assert!(
            targets.iter().any(|t| t == expected),
            "{expected} was not found in {targets:?}"
        );
    }
    assert!(
        !targets.iter().any(|t| t.contains("inside_code")),
        "a link inside fenced code was reported"
    );
    assert!(is_external("https://example.com") && is_external("#top") && !is_external("page.md"));
}
