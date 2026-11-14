//! Writes the Protocol Buffers definition of the `.fig` format, one `.proto` file per
//! IR module under `ironlab/ir/v0/`, together with a `buf.yaml`.
//!
//! The files are written to the `schema/proto/` directory at the root of the
//! workspace, where they are committed, or to the directory given with `--out DIR`.
//! The `ironlab/` tree and the `buf.yaml` of an earlier run are removed first, so that
//! the file of a removed module does not survive; nothing else in the directory is
//! touched.
//! Run `buf lint` in the output directory to check the files.

use std::path::{Path, PathBuf};

const USAGE: &str = "usage: generate-proto [--out DIR]";

fn main() -> std::io::Result<()> {
    let mut out = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
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
    let tree = dir.join("ironlab");
    if tree.exists() {
        std::fs::remove_dir_all(&tree)?;
    }
    let buf_yaml = dir.join("buf.yaml");
    if buf_yaml.exists() {
        std::fs::remove_file(&buf_yaml)?;
    }
    let files = ironlab_ir::proto_files().into_iter().chain([(
        PathBuf::from("buf.yaml"),
        ironlab_ir::wire::BUF_YAML.to_owned(),
    )]);
    for (path, text) in files {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, text)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// Returns the `schema/proto/` directory at the root of the workspace.
fn default_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lies two levels below the workspace root")
        .join("schema")
        .join("proto")
}

fn exit_with_usage(problem: &str) -> ! {
    eprintln!("generate-proto: {problem}\n{USAGE}");
    std::process::exit(2);
}
