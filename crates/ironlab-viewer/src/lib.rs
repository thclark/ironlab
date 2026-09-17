//! Interactive egui viewer for IronLAB figures.
//!
//! The viewer is a dumb consumer of the scene compiler: it draws the display list produced by
//! [`ironlab_scene::compile()`] and turns pointer input into typed edits of the figure IR (axis limits, 3D views and
//! artist visibility), which it records in a view overlay rather than applying to the figure it was given. The crate
//! is split into five modules:
//!
//! - [`canvas`] converts a display list into `egui` triangle meshes with lyon.
//! - [`interaction`] holds the pure, GPU-free state machine that maps pointer gestures onto IR edits, keeps the
//!   source figure and the user's overlay apart, and composes the figure that is displayed.
//! - [`offscreen`] renders a figure through the same meshes into an image without a window, for the documentation
//!   gallery and for tests.
//! - [`inspector`] builds what the property editor shows (the object tree, the properties of a node and the
//!   parameters of a figure) and turns a change made in it into a transaction; it, too, is pure logic.
//! - [`panel`] draws the property editor as a side panel: the object tree above, the inspector below.
//! - [`problems`] describes what the viewer has to tell the user about a figure it cannot draw as asked, and is
//!   pure logic too.
//! - [`app`] is the eframe application: one tab per figure, a toolbar, undo and redo, PDF export and saving.
//! - [`files`] reads and writes `.fig` (Protocol Buffers) and `.json` figure files by extension, for the
//!   `ironlab-viewer` binary and for saving from the viewer.

pub mod app;
pub mod canvas;
pub mod files;
pub mod inspector;
pub mod interaction;
pub mod offscreen;
pub mod panel;
pub mod problems;

pub use app::{ToolbarResponse, ViewerApp, run, toolbar};
pub use canvas::{ScreenTransform, tessellate};
pub use interaction::{FigureState, ROTATE_DEGREES_PER_POINT, Tool};
pub use offscreen::{
    OffscreenRenderer, RenderError, RenderedImage, render_display_list_offscreen, render_offscreen,
};
pub use panel::{PropertyPanel, property_panel};
pub use problems::{Origin, Problem};
