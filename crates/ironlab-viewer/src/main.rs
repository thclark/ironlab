//! Opens `.fig.json` files in the IronLAB viewer, one tab per file.

use std::path::Path;
use std::process::ExitCode;

use ironlab_ir::Figure;

fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: ironlab-viewer FIGURE.fig.json [FIGURE.fig.json ...]");
        return ExitCode::FAILURE;
    }
    let mut figures = Vec::with_capacity(paths.len());
    for path in &paths {
        let figure = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|json| Figure::from_json(&json).map_err(|e| e.to_string()));
        match figure {
            Ok(figure) => {
                let title = Path::new(path)
                    .file_name()
                    .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
                figures.push((title, figure));
            }
            Err(error) => {
                eprintln!("{path}: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    match ironlab_viewer::run(figures) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
