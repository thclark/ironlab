//! Interactive egui viewer for IronLAB figures.
//!
//! The viewer is a dumb consumer of the scene compiler: it draws the display list produced by
//! [`ironlab_scene::compile()`] and turns pointer input into edits of the figure IR (axis limits, 3D views and
//! artist visibility). The crate is split into four modules:
//!
//! - [`canvas`] converts a display list into `egui` triangle meshes with lyon.
//! - [`interaction`] holds the pure, GPU-free state machine that maps pointer gestures onto IR edits.
//! - [`offscreen`] renders a figure through the same meshes into an image without a window, for the documentation
//!   gallery and for tests.
//! - [`app`] is the eframe application: one tab per figure, a toolbar, and PDF export.

pub mod app;
pub mod canvas;
pub mod interaction;
pub mod offscreen;

pub use app::{ToolbarResponse, ViewerApp, run, toolbar};
pub use canvas::{ScreenTransform, tessellate};
pub use interaction::{FigureState, ROTATE_DEGREES_PER_POINT, Tool};
pub use offscreen::{
    OffscreenRenderer, RenderError, RenderedImage, render_display_list_offscreen, render_offscreen,
};
