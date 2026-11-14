//! Retained figure intermediate representation for IronLAB.
//!
//! The types in this crate are the single source of truth for the IronLAB figure
//! format. A [`Figure`] is a serialisable tree of nodes (the figure, its data arrays, its
//! colour scales, its axes and their artists, and its panels), each identified by a
//! stable [`NodeId`] from one identifier space. A data array ([`DataArray`]) holds a
//! numeric array ([`NdArray`]) that artists refer to by the identifier of the data array.
//! An array holds either 64-bit floating-point values or 8-bit unsigned integers, which is
//! its element type ([`NdArrayElement`]); the two image artists accept either, and every
//! other artist requires floating-point values. A colour scale ([`ColorScale`]) turns
//! numbers into colours, and every artist that maps numbers to colours names its scale
//! by the identifier of the scale.
//!
//! # Formats
//!
//! The default file and transport format is Protocol Buffers (`.fig`), written by
//! [`Figure::to_protobuf`] and read by [`Figure::from_protobuf`] through the wire types
//! of the [`wire`] module. JSON (`.fig.json`), written by [`Figure::to_json`] and read by
//! [`Figure::from_json`], is a supported secondary format for debugging, other tools and
//! simple web pages. Both describe the same figure, so converting between them loses
//! nothing. An array keeps its element type in both: Protocol Buffers stores
//! floating-point values as IEEE 754 doubles and 8-bit values as bytes, and JSON writes
//! floating-point values as numbers with `null` for a non-finite value and 8-bit values
//! as integers under an `element` of `"u8"`.
//!
//! The schemas of both formats are generated from the Rust types and committed in the
//! `schema/` directory of the repository, where a pre-commit hook and CI keep them in
//! step with the types: [`proto_files`] returns the `.proto` files (written by
//! `cargo run -p ironlab-ir --bin generate-proto`), and [`json_schema`] and
//! [`transaction_json_schema`] return the JSON Schema documents of a figure and of a
//! transaction of the edit protocol (written by
//! `cargo run -p ironlab-ir --bin generate-schema`).
//!
//! # Defaults
//!
//! The [`Default`] implementations follow MATLAB where MATLAB has an equivalent:
//!
//! - A figure is 160 mm wide and 100 mm high, uses the STIX Two font set at a base
//!   size of 9 pt on a white background, and has a single-cell tile layout.
//! - An axes is two-dimensional with a full box, an automatic aspect (kept, when it is
//!   fixed, by shrinking the plot area), a title centred over the plot area and no
//!   legend. Each of its axes is linear, has automatic limits, automatic ticks and
//!   automatic tick labels, runs in the usual direction (left to right, bottom to top)
//!   and has no grid lines.
//! - A three-dimensional view has an azimuth of −37.5°, an elevation of 30°, a zoom
//!   of 1 and no pan.
//! - A line style is a solid 0.75 pt line in the automatic colour, and a marker style
//!   has no shape, a size of 4 pt, no face colour and the automatic edge colour.
//! - A series draws a line in the line style's defaults, without markers, size or
//!   colour data; a contour takes the levels of its colour scale and draws them as
//!   colormapped, unfilled isolines in the bottom plane; a quiver scales arrows automatically with heads of 0.3 of the arrow length; a surface has
//!   colormapped faces and black edges 0.5 pt wide.
//! - An image of either kind lies in the xy plane (the floor of a three-dimensional
//!   axes, at the bottom of its z axis) with its pixel centres at 0 to n − 1 along each
//!   axis of the plane. A colour-mapped artist names no colour scale.
//! - A colour scale is continuous, interpolating viridis in linear space between
//!   automatic limits and clamping values beyond them. Every kind of scale draws nothing
//!   for a value that is not finite. An indexed or categorical scale refuses a value
//!   that names none of its entries, and a discrete scale has ten automatic bands.
//! - Artists are visible and have no label, and data arrays have no name.
//! - A legend inside a plot area sits in the top-right corner, is boxed and lists its
//!   entries in one column. The content of a panel is centred in it, and a legend in a
//!   panel is outlined by a border drawn as the box of an axes is drawn and arranges its
//!   entries in as few columns as fit the panel. Entries fill each column before the
//!   next.
//! - The rows and columns of a tile layout all have a weight of one, so its plot areas
//!   are of equal size.
//! - A legend lists its entries automatically, rather than from an explicit list of items.

mod artist;
mod axes;
mod color_scale;
pub mod command;
mod data;
mod edit;
mod error;
mod figure;
mod ids;
mod layout;
mod legend;
mod link;
mod nodes;
pub mod overlay;
mod panel;
pub mod selection;
mod style;
mod text;
mod validate;
pub mod wire;

pub use artist::{
    Artist, Bar, BarOffset, BarWidth, BinRule, Binning, Binning2d, Bins, CellEdges, CellGap, Cells,
    Contour, ContourPlacement, Coords, Grid, HistNorm, Image, ImagePlacement, ImagePlane, Levels,
    Orientation, PerBar, PixelRange, Quiver, QuiverScale, ScalarImage, Series, Stairs, StairsData,
    StepAt, Surface,
};
pub use axes::{
    AngleUnits, Aspect, AspectAdjust, Axes, Axis, AxisSide, ColormapName, InsideColorbar,
    InsideColorbarSide, Limits, PolarAngle, PolarView, Projection, Scale, SphericalView,
    TickLabels, Ticks, TitleAlign, View3d,
};
pub use color_scale::{
    CategoricalColorScale, ColorCategory, ColorMapping, ColorScale, ColorScaleClass,
    ColorScaleDomain, ColorScaleKey, ColorScaleKind, ColorScaleLevels, ColorScaleNorm,
    ColorScaleOutOfRange, ColorScaleSlot, Colormap, ContinuousColorScale, DiscreteColorScale,
    IndexedColorScale, LISTED_COLORMAP_MAX, LISTED_COLORMAP_MIN, OutOfRange,
};
pub use data::{DataArray, NdArray, NdArrayElement, Values};
pub use edit::{
    Choice, Edit, EditError, Node, NodeKind, PathError, Property, PropertyPath, Transaction, Value,
    ValueType, choices, properties, properties_of, property_choices,
};
pub use error::{IrError, ProtobufError};
pub use figure::{
    Embolden, Figure, FigureSize, FontSetId, NodeIdAllocator, Parameter, Provenance, SCHEMA_VERSION,
};
pub use ids::NodeId;
pub use layout::{Cell, TileLayout, TrackSize};
pub use legend::{
    DEFAULT_BAR_ROWS, InsideLegend, Legend, LegendEntries, LegendEntryColumns, LegendEntryLayout,
    LegendEntryOrder, LegendItem, LegendItemContent, LegendLocation, LegendSample, PanelLegend,
};
pub use link::{AxisLink, Dimension};
pub use panel::{
    ColorbarOrientation, ColorbarTitlePlacement, Panel, PanelColorbar, PanelContent, PanelLocation,
};
pub use style::{Color, ColorSpec, DashStyle, LineStyle, MarkerShape, MarkerStyle};
pub use text::{Interpreter, Text};
pub use validate::{IssueKind, ValidationIssue, ValidationReport};

use std::path::PathBuf;

/// Generates the JSON Schema of the `.fig.json` format from the Rust types, as a single
/// document whose definitions hold every type.
///
/// `cargo run -p ironlab-ir --bin generate-schema` writes it to
/// `schema/ironlab-figure.schema.json`.
pub fn json_schema() -> serde_json::Value {
    schemars::schema_for!(Figure).to_value()
}

/// Generates the JSON Schema of a transaction of the edit protocol from the Rust types,
/// as a single document whose definitions hold every type.
///
/// `cargo run -p ironlab-ir --bin generate-schema` writes it to
/// `schema/ironlab-edit.schema.json`.
pub fn transaction_json_schema() -> serde_json::Value {
    schemars::schema_for!(Transaction).to_value()
}

/// Generates the Protocol Buffers definition of the `.fig` format as one `.proto` file
/// per module of this crate, in package `ironlab.ir.v0`, with paths relative to the
/// root of a proto source tree (such as `ironlab/ir/v0/figure.proto`).
///
/// The files are rendered from the same declarations that define the [`wire`] types,
/// so they describe exactly the bytes that [`Figure::to_protobuf`] writes. They are
/// written, together with a `buf.yaml`, to `schema/proto/` by
/// `cargo run -p ironlab-ir --bin generate-proto`.
pub fn proto_files() -> Vec<(PathBuf, String)> {
    wire::schema::render_files()
}
