//! Layout, scales and scene compilation from figure IR to a display list.

pub mod compile;
pub mod display;
pub mod hit;
pub mod maths;

pub use compile::{Scene, SceneWarning, compile};
