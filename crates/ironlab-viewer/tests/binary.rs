//! The `ironlab-viewer` binary: it opens figure files by extension and fails without a window when it cannot.

use std::path::PathBuf;
use std::process::Command;

/// Returns a path in Cargo's per-crate temporary directory, removing any file left there by an earlier run.
fn fresh(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ironlab-viewer-binary-tests");
    std::fs::create_dir_all(&dir).expect("create temporary directory");
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

// Why: the binary is how users open saved figures from the command line; without arguments it must explain which files
// it accepts, and a bad argument must fail with a message naming the file, both without opening a window.
#[test]
fn the_binary_explains_its_usage_and_refuses_bad_files_without_a_window() {
    let binary = env!("CARGO_BIN_EXE_ironlab-viewer");

    let usage = Command::new(binary).output().expect("the binary runs");
    assert!(!usage.status.success());
    let stderr = String::from_utf8_lossy(&usage.stderr);
    assert!(
        stderr.contains("FIGURE.fig") && stderr.contains(".json"),
        "{stderr}"
    );

    let path = fresh("figure.png");
    let refused = Command::new(binary)
        .arg(&path)
        .output()
        .expect("the binary runs");
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("figure.png") && stderr.contains(".fig"),
        "{stderr}"
    );
}
