//! Interactive egui viewer for IronLAB figures.
//!
//! The viewer is a dumb consumer of the scene compiler: it draws the display list produced by
//! [`ironlab_scene::compile()`] and turns pointer input into typed edits of the figure IR (axis limits, 3D views and
//! artist visibility), which it records in a view overlay rather than applying to the figure it was given. The crate
//! is split into nine modules:
//!
//! - [`canvas`] converts a display list into one draw list of triangles in figure points with lyon, image tiles as
//!   textured quads, and the depth of every vertex of a three-dimensional axes.
//! - [`gpu`] holds the viewer's own wgpu pipelines, which draw those lists inside egui's render pass on screen and
//!   inside the offscreen renderer's pass headless, depth-testing the artists of three-dimensional axes.
//! - [`interaction`] holds the pure, GPU-free state machine that maps pointer gestures onto IR edits, keeps the
//!   source figure and the user's overlay apart, and composes the figure that is displayed.
//! - [`offscreen`] renders a figure through the same pipelines into an image without a window, for the documentation
//!   gallery and for tests.
//! - [`inspector`] builds what the property editor shows (the object tree, the properties of a node and the
//!   parameters of a figure) and turns a change made in it into a transaction; it, too, is pure logic.
//! - [`panel`] draws the property editor as a side panel: the object tree above, the inspector below.
//! - [`problems`] describes what the viewer has to tell the user about a figure it cannot draw as asked, and is
//!   pure logic too.
//! - [`style`] holds the text sizes and colours of the interface, which [`app::run`] installs on the egui context.
//! - [`app`] is the eframe application: one tab per figure, a toolbar, undo and redo, PDF export and saving.
//! - [`export`] writes a figure as a PDF, supplying the PDF exporter with the viewer's own renderer so that the
//!   dense parts of a figure are rasterised by the pipeline that draws the screen.
//! - [`files`] reads and writes `.fig` (Protocol Buffers) and `.json` figure files by extension, for the
//!   `ironlab-viewer` binary and for saving from the viewer.

pub mod app;
pub mod canvas;
pub mod export;
pub mod files;
pub mod gpu;
pub mod inspector;
pub mod interaction;
pub mod offscreen;
pub mod panel;
pub mod problems;
pub mod style;

pub use app::{ToolbarResponse, ViewerApp, run, toolbar};
pub use canvas::{MAX_TILE_SIDE, Resolution, ScreenTransform, tessellate};
pub use export::{ExportError, GpuRasteriser, export_pdf, write_pdf};
pub use gpu::{
    DEPTH_FORMAT, Draw, DrawKind, DrawList, GpuCallback, GpuConfig, GpuPainter, JOIN_AT_END,
    JOIN_AT_START, MAX_DASH_ENTRIES, Segment, StrokeParams, TileKey, Uploads, Vertex, Viewport,
};
pub use interaction::{
    DATATIP_RADIUS_POINTS, Datatip, FigureState, PixelDatatip, PixelValue,
    ROTATE_DEGREES_PER_POINT, Tip, Tool,
};
pub use offscreen::{
    OffscreenRenderer, RenderError, RenderedImage, render_display_list_offscreen, render_offscreen,
    with_shared_renderer,
};
pub use panel::{PropertyPanel, property_panel};
pub use problems::{Origin, Problem};
