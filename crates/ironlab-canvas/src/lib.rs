//! The figure canvas of IronLAB: the engine that every host draws a figure with.
//!
//! The crate is a dumb consumer of the scene compiler. It turns the display list produced by
//! [`ironlab_scene::compile()`] into one draw list per figure and draws it through its own wgpu pipelines, on screen
//! inside a host's render pass and offscreen into an image without a window; it maps pointer gestures onto typed
//! edits of the figure IR (axis limits, 3D views and artist visibility), which it records in a view overlay rather
//! than applying to the figure it was given; and it exports a figure as a PDF, rasterising through the same pipelines
//! whatever the PDF exporter cannot keep as vectors. Nothing in it depends on a user interface toolkit, so the egui
//! viewer (`ironlab-viewer`) and a browser page draw figures with the same code, and the crate builds for
//! `wasm32-unknown-unknown`. It is split into nine modules:
//!
//! - [`canvas`] converts a display list into one draw list of triangles in figure points with lyon, image tiles as
//!   textured quads, and the depth of every vertex of a three-dimensional axes.
//! - [`gpu`] holds the wgpu pipelines, which draw those lists inside a host's render pass on screen and inside the
//!   offscreen renderer's pass headless, depth-testing the artists of three-dimensional axes.
//! - [`interaction`] holds the pure, GPU-free state machine that maps pointer gestures onto IR edits, keeps the
//!   source figure and the user's overlay apart, and composes the figure that is displayed.
//! - [`figure_canvas`] is the host-agnostic controller of one figure on a canvas: it fits the page into the area a
//!   host gives it, keeps the draw list for that fit, converts the gestures a host has decided into figure-space
//!   calls on the interaction state, and reports the cursor, rubber band and callout for the host to draw. It also
//!   holds the pointer recogniser a host without egui uses to decide those gestures by egui's own rules.
//! - [`offscreen`] renders a figure through the same pipelines into an image without a window, for the documentation
//!   gallery, for the PDF exporter's rasters and for tests.
//! - [`inspector`] builds what a property editor shows (the object tree, the properties of a node and the
//!   parameters of a figure) and turns a change made in it into a transaction; it, too, is pure logic.
//! - [`problems`] describes what a host has to tell the user about a figure it cannot draw as asked, and is pure
//!   logic too.
//! - [`export`] writes a figure as a PDF, performing the renders the PDF exporter asks for through the offscreen
//!   renderer so that the dense parts of a figure are rasterised by the pipeline that draws the screen.
//! - [`files`] encodes and decodes figures as `.fig` (Protocol Buffers) and `.json` bytes by file name, and reads
//!   and writes them as files, for the `ironlab-viewer` binary and for saving from a host.

pub mod canvas;
pub mod export;
pub mod figure_canvas;
pub mod files;
pub mod gpu;
pub mod inspector;
pub mod interaction;
pub mod offscreen;
pub mod problems;

pub use canvas::{MAX_TILE_SIDE, Resolution, ScreenTransform, tessellate};
pub use export::{ExportError, export_display_list};
#[cfg(not(target_arch = "wasm32"))]
pub use export::{export_pdf, write_pdf};
pub use figure_canvas::{
    CANVAS_MARGIN, Callout, Cursor, DATATIP_RING_POINTS, FigureCanvas, Fit, Gesture, Marker,
    Pointer, REBUILD_RATIO, WHEEL_ZOOM_RATE, datatip_text, pixel_datatip_text, wheel_factor,
};
pub use gpu::{
    DEPTH_FORMAT, Draw, DrawKind, DrawList, GpuConfig, GpuPainter, JOIN_AT_END, JOIN_AT_START,
    MAX_DASH_ENTRIES, MarkerGpu, MarkerVertex, Segment, StrokeParams, TileKey, Uploads, Vertex,
    Viewport,
};
pub use interaction::{
    DATATIP_RADIUS_POINTS, Datatip, FigureState, PixelDatatip, PixelValue,
    ROTATE_DEGREES_PER_POINT, Tip, Tool,
};
pub use offscreen::{OffscreenRenderer, RenderError, RenderedImage};
#[cfg(not(target_arch = "wasm32"))]
pub use offscreen::{render_display_list_offscreen, render_offscreen, with_shared_renderer};
pub use problems::{Origin, Problem};
