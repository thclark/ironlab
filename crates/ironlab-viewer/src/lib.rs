//! Interactive egui viewer for IronLAB figures.
//!
//! The viewer is the egui host of the figure canvas, [`ironlab_canvas`]: the canvas crate turns a display list into
//! one draw list and draws it through its own wgpu pipelines, maps gestures onto typed edits of the figure IR, builds
//! what the property editor shows and exports PDFs, and this crate draws the window around it with egui and eframe.
//! The crate is split into eight modules:
//!
//! - [`callback`] is the egui paint callback through which the window's render pass reaches the pipelines of
//!   [`ironlab_canvas::gpu`]; it is the only module that links `egui_wgpu`, and is left out of a browser build.
//! - [`panel`] draws the property editor as a side panel: the object tree above, the inspector below, both built by
//!   [`ironlab_canvas::inspector`].
//! - [`browse`] works out what a collection of figures offers to be filtered, ordered and grouped by, from the
//!   labels and parameters its figures carry, and applies what the user chose; it is pure logic with no egui in it.
//! - [`sidebar`] draws that as the figure browser: a left-hand panel that narrows the open figures down to the one
//!   to look at.
//! - [`widgets`] holds every control of the interface, drawn in one design language.
//! - [`style`] holds the text sizes and colours of the interface, which [`app::run`] installs on the egui context.
//! - [`app`] is the eframe application: one figure shown at a time, the figure browser beside it, a toolbar, undo and
//!   redo, PDF export and saving; its figure pane is the egui host of [`ironlab_canvas::FigureCanvas`].
//!
//! The binary of the same name opens `.fig` (Protocol Buffers) and `.json` figure files, read by
//! [`ironlab_canvas::files`].

pub mod app;
pub mod browse;
#[cfg(not(target_arch = "wasm32"))]
pub mod callback;
pub mod panel;
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
pub use panel::{PropertyPanel, property_panel};
pub use sidebar::{BrowserResponse, FigureBrowser, figure_browser};
