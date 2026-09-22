//! Helpers shared by the gallery's integration tests.

#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the helpers"
)]

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ironlab_gallery::{ExportedPdf, GalleryError, Renderer};

/// Creates a new, empty directory under the system temporary directory, unique to this call.
pub fn temp_dir(name: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "ironlab-gallery-{name}-{}-{nanos}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("the temporary directory can be created");
    dir
}

/// A renderer that needs no graphics adapter: it returns placeholder bytes that identify the figure (by a fingerprint
/// of its JSON) and the requested resolution, and remembers every call. Tests can therefore check that each generated
/// file was rendered from the right figure at the right resolution, even when two figures share a title. A PDF export
/// carries `export_warnings` for every figure, empty by default.
#[derive(Default)]
pub struct FakeRenderer {
    pub calls: RefCell<Vec<String>>,
    pub export_warnings: Vec<String>,
}

impl FakeRenderer {
    /// The bytes returned for a PNG of `figure` rendered at `dpi`.
    pub fn png_bytes(figure: &ironlab::ir::Figure, dpi: f64) -> Vec<u8> {
        format!("fake png of figure {} at {dpi} dpi", fingerprint(figure)).into_bytes()
    }

    /// The bytes returned for a PDF of `figure`.
    pub fn pdf_bytes(figure: &ironlab::ir::Figure) -> Vec<u8> {
        format!("fake pdf of figure {}", fingerprint(figure)).into_bytes()
    }
}

/// Returns a value that identifies a figure by its complete content.
fn fingerprint(figure: &ironlab::ir::Figure) -> u64 {
    let mut hasher = DefaultHasher::new();
    figure.to_json().hash(&mut hasher);
    hasher.finish()
}

fn title_of(figure: &ironlab::ir::Figure) -> String {
    figure
        .title
        .as_ref()
        .map_or_else(String::new, |t| t.content.clone())
}

impl Renderer for FakeRenderer {
    fn png(&self, figure: &ironlab::ir::Figure, dpi: f64) -> Result<Vec<u8>, GalleryError> {
        self.calls
            .borrow_mut()
            .push(format!("png {} {dpi}", title_of(figure)));
        Ok(Self::png_bytes(figure, dpi))
    }

    fn pdf(&self, figure: &ironlab::ir::Figure) -> Result<ExportedPdf, GalleryError> {
        self.calls
            .borrow_mut()
            .push(format!("pdf {}", title_of(figure)));
        Ok(ExportedPdf {
            bytes: Self::pdf_bytes(figure),
            warnings: self.export_warnings.clone(),
        })
    }
}

/// Returns the names of the files in a directory, sorted.
pub fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .map(|item| {
            item.expect("the directory entry is readable")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// Returns the text of a page with its fenced code blocks removed, so that link-like text inside source code is not
/// mistaken for a link.
pub fn without_fences(page: &str) -> String {
    let mut out = String::new();
    let mut fence: Option<String> = None;
    for line in page.lines() {
        let ticks: String = line.chars().take_while(|&c| c == '`').collect();
        match &fence {
            None if ticks.len() >= 3 => fence = Some(ticks),
            None => {
                out.push_str(line);
                out.push('\n');
            }
            Some(open) if line.trim_end() == open.as_str() => fence = None,
            Some(_) => {}
        }
    }
    out
}

/// Returns the contents of the first fenced code block whose info string is `rust`, including its final newline.
pub fn rust_fence(page: &str) -> Option<String> {
    let mut lines = page.lines();
    let fence = lines.by_ref().find_map(|line| {
        let ticks: String = line.chars().take_while(|&c| c == '`').collect();
        (ticks.len() >= 3 && line[ticks.len()..].trim() == "rust").then_some(ticks)
    })?;
    let mut body = String::new();
    for line in lines {
        if line.trim_end() == fence {
            return Some(body);
        }
        body.push_str(line);
        body.push('\n');
    }
    None
}

/// Returns every link and image target in a Markdown page with embedded HTML: the targets of Markdown links and
/// images (`[text](target)` and `![alt](target)`) and of `href` and `src` attributes. Fenced code is ignored.
pub fn link_targets(page: &str) -> Vec<String> {
    let text = without_fences(page);
    let mut targets = Vec::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("](") {
        let after = &rest[start + 2..];
        let end = after.find(')').expect("a Markdown link target is closed");
        targets.push(after[..end].trim().to_owned());
        rest = &after[end..];
    }
    for attribute in ["href=\"", "src=\""] {
        let mut rest = text.as_str();
        while let Some(start) = rest.find(attribute) {
            let after = &rest[start + attribute.len()..];
            let end = after.find('"').expect("an HTML attribute value is closed");
            targets.push(after[..end].to_owned());
            rest = &after[end..];
        }
    }
    targets
}

/// Returns whether a link target is external or a fragment, and therefore not a file of the generated site.
pub fn is_external(target: &str) -> bool {
    target.starts_with('#') || target.contains("://") || target.starts_with("mailto:")
}

/// Reads a file to a string, naming the file when it cannot be read.
pub fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}
