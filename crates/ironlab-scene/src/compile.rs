//! Compilation of a figure IR into a display list and hit map.

use ironlab_ir::{Figure, NodeId};
use ironlab_text::TextEngine;

use crate::display::DisplayList;
use crate::hit::HitMap;

/// A problem found while compiling a figure that did not prevent the figure from being drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneWarning {
    /// The IR node the warning concerns, when it concerns a specific node.
    pub node: Option<NodeId>,
    pub message: String,
}

/// The compiled form of a figure: everything a backend draws, and everything a front end needs to interact with it.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub display_list: DisplayList,
    pub hit_map: HitMap,
    pub warnings: Vec<SceneWarning>,
}

/// Compiles `figure` into a scene.
///
/// This is the only place where layout, tick generation, text placement, contour extraction, projection and depth
/// sorting happen; every backend draws the resulting display list without further interpretation. Compilation never
/// fails: invalid artists are skipped and reported as warnings, so a figure is always drawable.
pub fn compile(figure: &Figure, text: &TextEngine) -> Scene {
    let _ = (figure, text);
    todo!("scene compilation")
}
