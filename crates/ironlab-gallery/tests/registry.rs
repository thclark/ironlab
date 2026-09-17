//! Tests of the gallery registry and the correctness of every entry.
//!
//! WHY: the gallery is user-facing documentation. Its source files are shown verbatim as examples of the IronLAB API,
//! and its figures are the fixtures of the rendering and export tests, so an entry that does not build, does not
//! validate, or whose shown source differs from the code that ran would mislead readers and weaken those tests.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use ironlab_gallery::{all, find};

fn figures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/figures")
}

/// WHY: a figure that fails validation cannot be exported or shown, so the example would not work for a reader who
/// copies it. Warnings (such as non-positive data on a logarithmic axis, which is silently dropped) are also
/// rejected, because an example should not rely on data being discarded.
#[test]
fn every_entry_builds_a_valid_figure_without_warnings() {
    let mut failures = Vec::new();
    for entry in all() {
        let figure = (entry.build)();
        let report = figure.validate();
        if !report.errors.is_empty() || !report.warnings.is_empty() {
            let messages: Vec<_> = report
                .errors
                .iter()
                .chain(&report.warnings)
                .map(|e| e.message.as_str())
                .collect();
            failures.push(format!("{}: {}", entry.slug, messages.join("; ")));
        }
    }
    assert!(
        failures.is_empty(),
        "gallery figures with validation issues:\n{}",
        failures.join("\n")
    );
}

/// WHY: the plan requires every gallery figure to carry a title, and titles are the most visible check that figure
/// titles are laid out and exported.
#[test]
fn every_figure_has_a_title() {
    for entry in all() {
        let figure = (entry.build)();
        let title = figure.ir().title.as_ref();
        assert!(
            title.is_some_and(|t| !t.content.trim().is_empty()),
            "{} has no figure title",
            entry.slug
        );
    }
}

/// WHY: slugs name the documentation pages and asset files, so duplicates would overwrite each other's pages, and a
/// slug that differs from its file name would break the capture of the source shown on the page.
#[test]
fn slugs_are_unique_and_match_the_source_files() {
    let slugs: Vec<&str> = all().iter().map(|e| e.slug).collect();
    let unique: BTreeSet<&str> = slugs.iter().copied().collect();
    assert_eq!(unique.len(), slugs.len(), "duplicate slugs in {slugs:?}");

    let files: BTreeSet<String> = fs::read_dir(figures_dir())
        .expect("the figures directory exists")
        .map(|entry| entry.expect("the directory entry is readable").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    let registered: BTreeSet<String> = unique.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(
        registered, files,
        "every file in src/figures must be registered in the gallery! macro, and vice versa"
    );

    for slug in &slugs {
        assert!(
            slug.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "slug {slug} must be lower-case ASCII so that it is a portable file name and URL"
        );
    }
}

/// WHY: titles and descriptions are shown on the index cards and entry pages, and the documentation style requires
/// full sentences.
#[test]
fn titles_and_descriptions_are_written_for_readers() {
    for entry in all() {
        assert!(
            !entry.title.trim().is_empty(),
            "{} has an empty title",
            entry.slug
        );
        assert!(
            !entry.title.ends_with('.'),
            "the title of {} is a name, not a sentence",
            entry.slug
        );
        let description = entry.description.trim();
        assert!(
            !description.is_empty(),
            "{} has an empty description",
            entry.slug
        );
        assert!(
            description.ends_with('.'),
            "the description of {} must end with a full stop: {description:?}",
            entry.slug
        );
        assert!(
            description.chars().next().is_some_and(char::is_uppercase),
            "the description of {} must start with a capital letter",
            entry.slug
        );
        assert!(
            !description.contains("  "),
            "the description of {} contains a double space, usually a line-continuation artefact",
            entry.slug
        );
    }
}

/// WHY: the documentation promises that the code shown is exactly the code that produced the figure.
#[test]
fn source_is_the_file_on_disk() {
    for entry in all() {
        let path = figures_dir().join(format!("{}.rs", entry.slug));
        let on_disk = fs::read_to_string(&path).expect("the entry's source file is readable");
        assert_eq!(
            entry.source,
            on_disk,
            "the captured source of {} differs from {}",
            entry.slug,
            path.display()
        );
    }
}

/// WHY: entries are shown as examples of user code, so they must use only the public facade and must not reach into
/// the IR or other internal crates, and must not carry module documentation that the generated page replaces.
#[test]
fn sources_read_as_user_code() {
    for entry in all() {
        for forbidden in ["ir_mut", "ironlab::ir", "ironlab_", "unsafe", "//!"] {
            assert!(
                !entry.source.contains(forbidden),
                "the source of {} contains {forbidden:?}, which is not user-facing API",
                entry.slug
            );
        }
        assert!(
            entry.source.contains("use ironlab::prelude::*;"),
            "the source of {} must import the prelude",
            entry.slug
        );
        for item in [
            "pub const TITLE: &str",
            "pub const DESCRIPTION: &str",
            "pub fn figure() -> Figure",
        ] {
            assert!(
                entry.source.contains(item),
                "the source of {} lacks {item:?}",
                entry.slug
            );
        }
    }
}

/// WHY: the `gallery view` and `gallery export` commands select entries by slug.
#[test]
fn find_returns_the_entry_with_a_slug() {
    for entry in all() {
        let found = find(entry.slug).expect("every registered slug can be found");
        assert_eq!(found.title, entry.title);
    }
    assert!(find("no_such_entry").is_none());
}
