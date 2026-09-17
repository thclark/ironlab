//! Retained figure intermediate representation for IronLAB.
//!
//! The types in this crate are the single source of truth for the IronLAB figure
//! format. A [`Figure`] is a serialisable tree of nodes (the figure, its axes and
//! their artists), each identified by a stable [`NodeId`], together with a table of
//! numeric arrays referenced by [`DataId`]. The JSON representation of a figure is
//! the `.fig.json` file format, and its JSON Schema (`schema/figure.schema.json`) is
//! generated from these types by [`json_schema`].
//!
//! # Defaults
//!
//! The [`Default`] implementations follow MATLAB where MATLAB has an equivalent:
//!
//! - A figure is 160 mm wide and 100 mm high, uses the STIX Two font set at a base
//!   size of 9 pt on a white background, and has a single-cell tile layout.
//! - An axes is two-dimensional with a full box, linear automatic axes without
//!   grid lines, the viridis colormap, automatic colour limits and no legend.
//! - A three-dimensional view has an azimuth of −37.5°, an elevation of 30°, a zoom
//!   of 1 and no pan.
//! - A line style is a solid 0.75 pt line in the automatic colour, and a marker style
//!   has no shape, a size of 4 pt, no face colour and the automatic edge colour.
//! - A scatter uses circles of 4 pt in the automatic colour; a contour draws ten
//!   automatic levels as colormapped, unfilled isolines in the bottom plane; a quiver
//!   scales arrows automatically with heads of 0.3 of the arrow length; a surface has
//!   colormapped faces and black edges 0.5 pt wide.
//! - Artists are visible and have no display name.

mod artist;
mod axes;
mod data;
mod error;
mod figure;
mod ids;
mod link;
mod style;
mod text;
mod validate;

pub use artist::{
    Artist, Contour, ContourPlacement, Grid, Levels, Line, Quiver, QuiverScale, Scatter,
    ScatterColor, ScatterSize, Surface,
};
pub use axes::{
    Axes, Axis, Cell, ColormapName, Legend, LegendLocation, Limits, Projection, Scale, View3d,
};
pub use data::NdArray;
pub use error::IrError;
pub use figure::{
    Figure, FigureSize, FontSetId, NodeIdAllocator, Provenance, SCHEMA_VERSION, TileLayout,
};
pub use ids::{DataId, NodeId};
pub use link::{AxisLink, Dimension};
pub use style::{Color, ColorSpec, DashStyle, LineStyle, MarkerShape, MarkerStyle};
pub use text::{Interpreter, Text};
pub use validate::{IssueKind, ValidationIssue, ValidationReport};

/// Generates the JSON Schema of the figure file format from the Rust types.
///
/// The result is the content of the committed `schema/figure.schema.json` file,
/// which is regenerated with `cargo run -p ironlab-ir --bin generate-schema`.
pub fn json_schema() -> serde_json::Value {
    schemars::schema_for!(Figure).to_value()
}
