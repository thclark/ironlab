//! Integration tests of PDF export.
//!
//! The PDF is checked with external tools rather than by inspecting krilla's output structurally, because the
//! contract of this crate is how real consumers see the file: poppler (`pdfinfo`, `pdftotext`, `pdffonts`,
//! `pdftoppm`) and Ghostscript (`gs`). Raster checks run against both engines, since a subtly malformed PDF is often
//! tolerated by one viewer and not the other.
//!
//! When a tool is missing, a test prints a message and passes, unless the environment variable
//! `IRONLAB_REQUIRE_PDF_TOOLS` is set (as it is in CI), in which case the test fails.

mod common;
mod dense;
mod document;
mod export;
mod raster;
mod robustness;
mod text;
