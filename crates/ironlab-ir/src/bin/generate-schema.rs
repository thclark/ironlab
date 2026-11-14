//! Writes the JSON Schema of the `.fig.json` format as two self-contained documents,
//! each of whose references is local to its own definitions:
//! `ironlab-figure.schema.json`, which describes a figure, and
//! `ironlab-edit.schema.json`, which describes a transaction of the edit protocol.
//!
//! The documents are written to the `schema/` directory at the root of the workspace,
//! where they are committed, or to the directory given with `--out DIR`. Every other
//! `*.schema.json` file directly in that directory is removed, so that a document which
//! is no longer generated does not survive. Other files, such as the `.proto` files
//! that `generate-proto` writes to `schema/proto/`, are left alone.
//!
//! With `--print-version`, the command writes nothing and prints
//! [`ironlab_ir::SCHEMA_VERSION`] instead, so that the tools which check the schema
//! version read the constant as the build compiled it rather than parsing the Rust
//! source.

use std::path::{Path, PathBuf};

/// The name of the document that describes a figure.
const FIGURE_DOCUMENT: &str = "ironlab-figure.schema.json";

/// The name of the document that describes a transaction of the edit protocol.
const EDIT_DOCUMENT: &str = "ironlab-edit.schema.json";

/// The suffix of every file this command writes, by which stale files are found.
const SUFFIX: &str = ".schema.json";

const USAGE: &str = "usage: generate-schema [--out DIR] [--print-version]";

fn main() -> std::io::Result<()> {
    let mut out = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--print-version" => {
                println!("{}", ironlab_ir::SCHEMA_VERSION);
                return Ok(());
            }
            "--out" => match args.next() {
                Some(dir) => out = Some(PathBuf::from(dir)),
                None => exit_with_usage("--out needs a directory"),
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => exit_with_usage(&format!("unknown argument {other}")),
        }
    }
    let dir = out.unwrap_or_else(default_dir);
    std::fs::create_dir_all(&dir)?;
    remove_stale_documents(&dir)?;
    for (name, schema) in [
        (FIGURE_DOCUMENT, ironlab_ir::json_schema()),
        (EDIT_DOCUMENT, ironlab_ir::transaction_json_schema()),
    ] {
        let mut text = serde_json::to_string_pretty(&schema).expect("a JSON value serialises");
        text.push('\n');
        let path = dir.join(name);
        std::fs::write(&path, text)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// Removes every file directly in `dir` whose name ends in `.schema.json`.
fn remove_stale_documents(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.file_name().to_string_lossy().ends_with(SUFFIX) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Returns the `schema/` directory at the root of the workspace.
fn default_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lies two levels below the workspace root")
        .join("schema")
}

fn exit_with_usage(problem: &str) -> ! {
    eprintln!("generate-schema: {problem}\n{USAGE}");
    std::process::exit(2);
}
