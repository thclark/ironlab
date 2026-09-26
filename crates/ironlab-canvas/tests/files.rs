//! Opening figure files: the format is chosen by the extension, and every failure names the file.

mod common;

use std::path::PathBuf;

use common::*;
use ironlab_canvas::files::{Format, OpenError, SaveError, figure_stem, read_figure, write_figure};
use ironlab_ir::{Dimension, NdArray, Parameter};

/// Returns a path in Cargo's per-crate temporary directory, removing any file left there by an earlier run.
fn fresh(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ironlab-canvas-file-tests");
    std::fs::create_dir_all(&dir).expect("create temporary directory");
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

/// A figure with two linked axes and a data array, so that a lossy reader would be noticed.
fn sample() -> ironlab_ir::Figure {
    let mut figure = figure_with(
        vec![axes_2d(2), axes_3d(3)],
        vec![link(Dimension::X, &[2, 3])],
    );
    figure.add_data(NdArray::vector(vec![0.0, f64::NAN, 2.5]));
    figure
}

// Why: `.fig` is the default format that the facade saves and that other tools write from the generated `.proto`
// files, so the viewer must open it.
#[test]
fn a_fig_file_is_read_as_protobuf() {
    let path = fresh("sample.fig");
    std::fs::write(&path, sample().to_protobuf()).unwrap();
    assert_eq!(read_figure(&path).unwrap(), sample());
}

// Why: JSON remains supported for debugging and for files written by other tools, under both `.json` and `.fig.json`.
#[test]
fn json_files_are_read_as_json() {
    for name in ["sample.fig.json", "sample.json"] {
        let path = fresh(name);
        std::fs::write(&path, sample().to_json()).unwrap();
        assert_eq!(read_figure(&path).unwrap(), sample(), "{name}");
    }
}

// Why: extensions are case-insensitive on the file systems of macOS and Windows, so `FIGURE.FIG` must open as `.fig`.
#[test]
fn extensions_are_matched_without_regard_to_case() {
    let path = fresh("UPPER.FIG");
    std::fs::write(&path, sample().to_protobuf()).unwrap();
    assert_eq!(read_figure(&path).unwrap(), sample());
}

// Why: a file whose extension names no figure format must be refused with a message that names the file and the
// supported extensions, before the file is read, so that the user learns what to rename rather than seeing a decoding
// error from a guessed format.
#[test]
fn an_unsupported_extension_is_refused_before_reading() {
    let path = fresh("notes.txt");
    let error = read_figure(&path).unwrap_err();
    assert!(
        matches!(&error, OpenError::UnsupportedFormat { path: p } if p == &path),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("notes.txt"), "{message}");
    assert!(
        message.contains(".fig") && message.contains(".json"),
        "{message}"
    );
}

// Why: when several files are opened at once, the user must be told which one is missing.
#[test]
fn a_missing_file_is_an_io_error_naming_the_file() {
    let path = fresh("absent.fig");
    let error = read_figure(&path).unwrap_err();
    assert!(
        matches!(&error, OpenError::Io { path: p, .. } if p == &path),
        "{error:?}"
    );
    assert!(error.to_string().contains("absent.fig"), "{error}");
}

// Why: a file with the right extension but the wrong content (such as JSON renamed to `.fig`) must be reported as an
// invalid figure naming the file, not opened with defaults and not a panic.
#[test]
fn content_that_is_not_a_figure_is_an_error_naming_the_file() {
    let fig_path = fresh("renamed_json.fig");
    std::fs::write(&fig_path, sample().to_json()).unwrap();
    let json_path = fresh("broken.fig.json");
    std::fs::write(&json_path, "{ not json").unwrap();

    for path in [fig_path, json_path] {
        let error = read_figure(&path).unwrap_err();
        assert!(
            matches!(&error, OpenError::Invalid { path: p, .. } if p == &path),
            "{error:?}"
        );
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(error.to_string().contains(&name), "{error}");
    }
}

// Why: the viewer suggests `<stem>.pdf` when exporting a tab titled with its file name, so every figure extension must
// be removed from the title (and only a figure extension), or the suggestion would be `pressure.fig.pdf`.
#[test]
fn figure_stem_removes_only_figure_extensions() {
    assert_eq!(figure_stem("pressure.fig"), "pressure");
    assert_eq!(figure_stem("pressure.fig.json"), "pressure");
    assert_eq!(figure_stem("pressure.json"), "pressure");
    assert_eq!(figure_stem("PRESSURE.FIG"), "PRESSURE");
    assert_eq!(figure_stem("notes.txt"), "notes.txt");
    assert_eq!(figure_stem("Two panels"), "Two panels");
    assert_eq!(figure_stem(".fig"), ".fig");
}

// Why: "Save figure…" writes the figure the user is looking at, and the format must follow the name they typed, so
// that a `.fig` name gives the default Protocol Buffers file and a `.json` name a readable one; either must read back
// as the same figure.
#[test]
fn a_figure_is_written_in_the_format_named_by_the_extension_and_reads_back_unchanged() {
    for name in [
        "written.fig",
        "written.fig.json",
        "written.json",
        "WRITTEN.FIG",
    ] {
        let path = fresh(name);
        write_figure(&path, &sample()).unwrap_or_else(|error| panic!("{name}: {error}"));
        let bytes = std::fs::read(&path).unwrap();
        let is_json = name.to_ascii_lowercase().ends_with(".json");
        assert_eq!(
            bytes.starts_with(b"{"),
            is_json,
            "{name} is written as {}",
            if is_json { "JSON" } else { "Protocol Buffers" }
        );
        assert_eq!(read_figure(&path).unwrap(), sample(), "{name}");
    }
}

// Why: a user who types a name with no figure extension must be told what to rename it to, and must not be left with a
// file whose content does not match its name.
#[test]
fn saving_to_an_unsupported_extension_is_refused_and_writes_no_file() {
    let path = fresh("figure.png");
    let error = write_figure(&path, &sample()).unwrap_err();
    assert!(
        matches!(&error, SaveError::UnsupportedFormat { path: p } if p == &path),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("figure.png"), "{message}");
    assert!(
        message.contains(".fig") && message.contains(".json"),
        "{message}"
    );
    assert!(!path.exists(), "no file was written");
}

// Why: a save into a directory that does not exist (or that the user cannot write to) must name the file it failed on,
// because the viewer reports the failure in a notification with no other context.
#[test]
fn a_save_that_cannot_be_written_is_an_io_error_naming_the_file() {
    let path = fresh("absent").join("figure.fig");
    let error = write_figure(&path, &sample()).unwrap_err();
    assert!(
        matches!(&error, SaveError::Io { path: p, .. } if p == &path),
        "{error:?}"
    );
    assert!(error.to_string().contains("figure.fig"), "{error}");
}

/// A figure that exercises every part of both encodings: axes with titles and artists of three kinds, a data array
/// holding a NaN (written as `null` in JSON and as its IEEE 754 bits in Protocol Buffers), a figure title, a
/// parameter of every kind and labels (both omitted from JSON when empty, so present here to prove they survive).
fn rich() -> ironlab_ir::Figure {
    let mut figure = figure_with_artists();
    figure.title = Some(ironlab_ir::Text::plain("Speed against height"));
    figure.add_data(NdArray::vector(vec![0.0, f64::NAN, 2.5]));
    figure
        .parameters
        .insert("reynolds".to_owned(), Parameter::Number(1.5e6));
    figure
        .parameters
        .insert("run".to_owned(), Parameter::Integer(42));
    figure
        .parameters
        .insert("converged".to_owned(), Parameter::Bool(true));
    figure
        .parameters
        .insert("case".to_owned(), Parameter::String("baseline".to_owned()));
    figure.labels = vec!["surface".to_owned(), "3d".to_owned()];
    figure
}

// Why: a browser host has no path to hand to `read_figure`; it has a file name from a fetch or a file picker and must
// choose the decoder from the name alone. The choice must be the one `read_figure` makes today: `.fig` is Protocol
// Buffers, `.json` (so also `.fig.json`) is JSON, the comparison ignores case (files.rs documents this, and macOS and
// Windows file systems are case-insensitive), only the last extension counts, and a dot in a directory name is not
// an extension.
#[test]
fn the_format_is_chosen_by_the_file_name_s_extension() {
    assert_eq!(Format::from_name("figure.fig"), Some(Format::Fig));
    assert_eq!(Format::from_name("figure.json"), Some(Format::Json));
    assert_eq!(
        Format::from_name("figure.fig.json"),
        Some(Format::Json),
        "`.fig.json` is JSON, because the last extension decides"
    );
    assert_eq!(
        Format::from_name("dir.with.dots/figure.fig"),
        Some(Format::Fig),
        "dots in a directory name are not an extension"
    );

    assert_eq!(Format::from_name("FIGURE.FIG"), Some(Format::Fig));
    assert_eq!(Format::from_name("Figure.Json"), Some(Format::Json));
    assert_eq!(Format::from_name("figure.FIG.json"), Some(Format::Json));

    assert_eq!(Format::from_name("figure"), None, "no extension");
    assert_eq!(Format::from_name("figure.pdf"), None);
    assert_eq!(Format::from_name("figure.txt"), None);
    assert_eq!(
        Format::from_name("figure.json.fig"),
        Some(Format::Fig),
        "only the last extension is consulted"
    );
    assert_eq!(
        Format::from_name("dir.fig/figure"),
        None,
        "an extension on a directory does not name the file's format"
    );
}

// Why: a browser host encodes a figure for download and decodes what it fetched; if either format were lossy the
// user would save a different figure from the one they viewed. The fixture holds every kind of content that has its
// own encoding rule (NaN in data, an optional title, artists of several kinds, parameters of every type, labels), so
// that a field dropped by either codec is noticed.
#[test]
fn encoding_then_decoding_in_either_format_round_trips_a_figure() {
    let figure = rich();
    for format in [Format::Fig, Format::Json] {
        let bytes = format.encode(&figure);
        assert!(!bytes.is_empty(), "{format:?} encodes to some bytes");
        let decoded = format
            .decode(&bytes)
            .unwrap_or_else(|error| panic!("{format:?}: {error}"));
        assert_eq!(decoded, figure, "{format:?} round trip");
    }
}

// Why: a `.fig` downloaded from the browser must be byte for byte what the desktop viewer would have saved, and vice
// versa, so that the two hosts are one program with two ways of reaching the disk. This pins `write_figure` to
// `encode`, so that neither can gain a header, a trailing newline or a different JSON layout without the other.
#[test]
fn the_bytes_a_format_encodes_are_the_bytes_the_file_writer_writes() {
    let figure = rich();
    for (name, format) in [
        ("encoded.fig", Format::Fig),
        ("encoded.json", Format::Json),
        ("encoded.fig.json", Format::Json),
    ] {
        let path = fresh(name);
        write_figure(&path, &figure).unwrap_or_else(|error| panic!("{name}: {error}"));
        let written = std::fs::read(&path).unwrap();
        assert_eq!(
            written,
            format.encode(&figure),
            "{name}: the file holds exactly what {format:?} encodes"
        );
        assert_eq!(
            format.decode(&written).unwrap(),
            read_figure(&path).unwrap(),
            "{name}: decoding the bytes and reading the file agree"
        );
    }

    // JSON is text, so the bytes must also be the UTF-8 of the IR's own JSON, which other tools read.
    assert_eq!(
        Format::Json.encode(&figure),
        figure.to_json().into_bytes(),
        "the JSON bytes are the IR's JSON text as UTF-8"
    );
    assert_eq!(
        Format::Fig.encode(&figure),
        figure.to_protobuf(),
        "the Protocol Buffers bytes are the IR's own encoding"
    );
}

// Why: in a browser the bytes come from a network response or a file the user picked, and a wrong or damaged file
// must be reported as an error the host can show, never a panic that kills the page. The cases are the ones a user
// produces: a `.fig` served from a `.json` URL and the reverse, a file that is not a figure at all, a truncated file
// and an empty one.
#[test]
fn decoding_the_wrong_format_or_corrupt_bytes_is_an_error_not_a_panic() {
    let figure = rich();
    let protobuf = Format::Fig.encode(&figure);
    let json = Format::Json.encode(&figure);

    // Protocol Buffers bytes are never valid JSON: they do not start with `{`.
    let error = Format::Json.decode(&protobuf).unwrap_err();
    assert!(
        error.to_string().to_ascii_lowercase().contains("json"),
        "{error}"
    );

    // Text that is not JSON.
    assert!(Format::Json.decode(b"{ not json").is_err());
    assert!(Format::Fig.decode(b"not a figure").is_err());

    // JSON text handed to the Protocol Buffers decoder. Protocol Buffers is a binary encoding, so text may be
    // rejected outright or decode as nonsense; either is acceptable, but the result is never the figure and never a
    // panic. Today it is rejected, as `content_that_is_not_a_figure_is_an_error_naming_the_file` also shows through
    // `read_figure`.
    match Format::Fig.decode(&json) {
        Err(_) => {}
        Ok(garbage) => assert_ne!(
            garbage, figure,
            "JSON text decoded as Protocol Buffers cannot be the figure"
        ),
    }

    // Empty input. An empty Protocol Buffers message is a legal encoding of a message whose fields are all default,
    // so the decoder may accept it, but it must then fail the schema-version check rather than produce a figure.
    assert!(Format::Json.decode(b"").is_err(), "empty JSON is an error");
    assert!(
        Format::Fig.decode(b"").is_err(),
        "an empty message declares no schema version and is refused"
    );

    // A truncated file, of every length, must not panic; the decoders are given prefixes of both encodings.
    for cut in [1, 2, protobuf.len() / 2, protobuf.len() - 1] {
        match Format::Fig.decode(&protobuf[..cut]) {
            Err(_) => {}
            Ok(partial) => assert_ne!(partial, figure, "a prefix of {cut} bytes is not the figure"),
        }
    }
    for cut in [1, json.len() / 2, json.len() - 1] {
        assert!(
            Format::Json.decode(&json[..cut]).is_err(),
            "JSON cut at {cut} bytes is an error"
        );
    }
}

// Why: a browser host names the file it offers for download, and the name must select the same format when it is
// opened again, on any host; otherwise a figure saved as JSON would be opened as Protocol Buffers.
#[test]
fn the_extension_names_the_file_a_format_saves_as() {
    assert_eq!(Format::Fig.extension(), "fig");
    assert_eq!(Format::Json.extension(), "json");
    for format in [Format::Fig, Format::Json] {
        let name = format!("figure.{}", format.extension());
        assert_eq!(
            Format::from_name(&name),
            Some(format),
            "{name} opens in the format that named it"
        );
        assert_eq!(
            figure_stem(&name),
            "figure",
            "{name} is stripped back to its stem, so a PDF export is suggested as figure.pdf"
        );
    }
}
