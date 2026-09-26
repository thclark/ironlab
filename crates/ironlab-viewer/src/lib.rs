//! Interactive egui viewer for IronLAB figures.
//!
//! The viewer is a dumb consumer of the scene compiler: it draws the display list produced by
//! [`ironlab_scene::compile()`] and turns pointer input into typed edits of the figure IR (axis limits, 3D views and
//! artist visibility), which it records in a view overlay rather than applying to the figure it was given. The crate
//! is split into fifteen modules:
//!
//! - [`canvas`] converts a display list into one draw list of triangles in figure points with lyon, image tiles as
//!   textured quads, and the depth of every vertex of a three-dimensional axes.
//! - [`gpu`] holds the viewer's own wgpu pipelines, which draw those lists inside egui's render pass on screen and
//!   inside the offscreen renderer's pass headless, depth-testing the artists of three-dimensional axes.
//! - [`callback`] is the egui paint callback through which the window's render pass reaches those pipelines; it is
//!   the only module that links `egui_wgpu`, and is left out of a browser build.
//! - [`interaction`] holds the pure, GPU-free state machine that maps pointer gestures onto IR edits, keeps the
//!   source figure and the user's overlay apart, and composes the figure that is displayed.
//! - [`figure_canvas`] is the host-agnostic controller of one figure on a canvas: it fits the page into the area a
//!   host gives it, keeps the draw list for that fit, converts the gestures a host has decided into figure-space
//!   calls on the interaction state, and reports the cursor, rubber band and callout for the host to draw. It also
//!   holds the pointer recogniser a host without egui uses to decide those gestures by egui's own rules.
//! - [`offscreen`] renders a figure through the same pipelines into an image without a window, for the documentation
//!   gallery and for tests.
//! - [`inspector`] builds what the property editor shows (the object tree, the properties of a node and the
//!   parameters of a figure) and turns a change made in it into a transaction; it, too, is pure logic.
//! - [`panel`] draws the property editor as a side panel: the object tree above, the inspector below.
//! - [`problems`] describes what the viewer has to tell the user about a figure it cannot draw as asked, and is
//!   pure logic too.
//! - [`browse`] works out what a collection of figures offers to be filtered, ordered and grouped by, from the
//!   labels and parameters its figures carry, and applies what the user chose; like [`inspector`], it is pure
//!   logic with no egui in it.
//! - [`sidebar`] draws that as the figure browser: a left-hand panel that narrows the open figures down to the one
//!   to look at.
//! - [`style`] holds the text sizes and colours of the interface, which [`app::run`] installs on the egui context.
//! - [`app`] is the eframe application: one figure shown at a time, the figure browser beside it, a toolbar, undo and
//!   redo, PDF export and saving; its figure pane is the egui host of [`figure_canvas`].
//! - [`export`] writes a figure as a PDF, performing the renders the PDF exporter asks for through the viewer's own
//!   renderer so that the dense parts of a figure are rasterised by the pipeline that draws the screen.
//! - [`files`] reads and writes `.fig` (Protocol Buffers) and `.json` figure files by extension, for the
//!   `ironlab-viewer` binary and for saving from the viewer.

pub mod app;
pub mod browse;
#[cfg(not(target_arch = "wasm32"))]
pub mod callback;
pub mod canvas;
pub mod export;
pub mod figure_canvas;
pub mod files;
pub mod gpu;
pub mod inspector;
pub mod interaction;
pub mod offscreen;
pub mod panel;
pub mod problems;
pub mod sidebar;
pub mod style;
pub mod widgets;

pub use app::{ToolbarResponse, ViewerApp, run, toolbar};
pub use browse::{
    Browse, Comparison, Constraint, Facet, FacetKey, FacetKind, FacetValue, FigureCard, Filter,
    Group, Query, Results, Sort, SortKey, Term, browsable_labels, describe_facets, parameter_names,
};
#[cfg(not(target_arch = "wasm32"))]
pub use callback::GpuCallback;
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
pub use panel::{PropertyPanel, property_panel};
pub use problems::{Origin, Problem};
pub use sidebar::{BrowserResponse, FigureBrowser, figure_browser};
