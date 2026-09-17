//! Opens `.fig` (Protocol Buffers) and `.json` (JSON) figure files in the IronLAB viewer, one tab per file.

use std::path::Path;
use std::process::ExitCode;

use ironlab_viewer::files::read_figure;

const USAGE: &str = "\
usage: ironlab-viewer FIGURE.fig [FIGURE.fig ...]

Opens each figure file in a tab of the IronLAB viewer. The format of each file is
chosen by its extension: .fig files are read as Protocol Buffers (the default
format), and .json files (such as FIGURE.fig.json) are read as JSON.";

fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() || paths.iter().any(|arg| arg == "-h" || arg == "--help") {
        eprintln!("{USAGE}");
        return if paths.is_empty() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }
    let mut figures = Vec::with_capacity(paths.len());
    for path in &paths {
        let path = Path::new(path);
        match read_figure(path) {
            Ok(figure) => {
                let title = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                figures.push((title, figure));
            }
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    match ironlab_viewer::run(figures) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
