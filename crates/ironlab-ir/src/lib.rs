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
//! The schemas of both formats are generated from the Rust types as build artefacts
//! and are not committed: [`proto_files`] returns the `.proto` files (written by
//! `cargo run -p ironlab-ir --bin generate-proto`), and [`json_schema_files`] returns
//! the JSON Schema files (written by `cargo run -p ironlab-ir --bin generate-schema`).
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
    Artist, Contour, ContourPlacement, Coords, Grid, Image, ImagePlacement, ImagePlane, Levels,
    PixelRange, Quiver, QuiverScale, ScalarImage, Series, Surface,
};
pub use axes::{
    Aspect, AspectAdjust, Axes, Axis, ColormapName, InsideColorbar, InsideColorbarSide, Limits,
    Projection, Scale, TickLabels, Ticks, TitleAlign, View3d,
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
    Figure, FigureSize, FontSetId, NodeIdAllocator, Parameter, Provenance, SCHEMA_VERSION,
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
pub fn json_schema() -> serde_json::Value {
    schemars::schema_for!(Figure).to_value()
}

/// Generates the JSON Schema of a transaction of the edit protocol from the Rust types,
/// as a single document whose definitions hold every type.
pub fn transaction_json_schema() -> serde_json::Value {
    schemars::schema_for!(Transaction).to_value()
}

/// Generates the JSON Schema of the `.fig.json` format as one file per module of this
/// crate, with paths relative to the output directory.
///
/// Each file holds the definitions of the types declared in its module and refers to
/// the definitions of other modules by relative references. The files are written to
/// `target/ironlab-schema/` by `cargo run -p ironlab-ir --bin generate-schema`.
///
/// Two files describe a document rather than only definitions: `figure.schema.json`
/// describes a figure, and `edit.schema.json` describes a transaction of the edit
/// protocol. Every other file is named `<module>.schema.json` and holds only
/// definitions.
///
/// # Panics
///
/// Panics when a definition belongs to no module of this crate, which is a mistake in
/// the mapping of types to modules.
pub fn json_schema_files() -> Vec<(PathBuf, String)> {
    use serde_json::{Map, Value};

    /// The module of the figure document, whose file also describes a figure.
    const FIGURE: &str = "figure";

    /// The module of the edit protocol, whose file also describes a transaction.
    const EDIT: &str = "edit";

    /// Returns the module that declares the domain type of the given name.
    fn module_of(name: &str) -> &'static str {
        match name {
            "NodeId" => "ids",
            // A property path has no wire message of its own: it is a string field of
            // the edit messages.
            "PropertyPath" => EDIT,
            _ => wire::schema::module_declaring(name)
                .unwrap_or_else(|| panic!("the JSON Schema definition {name} has no module")),
        }
    }

    /// Rewrites every local reference to a definition of another module into a
    /// reference to that module's file.
    fn relocate_refs(value: &mut Value, module: &str) {
        match value {
            Value::Object(map) => {
                for (key, inner) in map.iter_mut() {
                    match (key.as_str(), inner) {
                        ("$ref", Value::String(reference)) => {
                            if let Some(name) = reference.strip_prefix("#/$defs/") {
                                let owner = module_of(name);
                                if owner != module {
                                    *reference = format!("{owner}.schema.json#/$defs/{name}");
                                }
                            }
                        }
                        (_, inner) => relocate_refs(inner, module),
                    }
                }
            }
            Value::Array(items) => items
                .iter_mut()
                .for_each(|item| relocate_refs(item, module)),
            _ => {}
        }
    }

    /// Takes the definitions out of a root document, leaving the document that
    /// describes its own type.
    fn split_definitions(root: &mut Value) -> Map<String, Value> {
        let object = root
            .as_object_mut()
            .expect("the root of a JSON Schema is an object");
        match object.remove("$defs") {
            Some(Value::Object(definitions)) => definitions,
            _ => Map::new(),
        }
    }

    let mut roots = std::collections::BTreeMap::from([
        (FIGURE, json_schema()),
        (EDIT, transaction_json_schema()),
    ]);
    let dialect = roots[FIGURE].get("$schema").cloned();

    // The two roots describe overlapping sets of types, which schemars generates
    // identically, so the definitions of both are collected into one set.
    let mut definitions = Map::new();
    for root in roots.values_mut() {
        definitions.extend(split_definitions(root));
    }

    let mut modules: std::collections::BTreeMap<&'static str, Map<String, Value>> =
        roots.keys().map(|module| (*module, Map::new())).collect();
    for (name, mut definition) in definitions {
        let module = module_of(&name);
        relocate_refs(&mut definition, module);
        modules.entry(module).or_default().insert(name, definition);
    }
    for (module, root) in roots.iter_mut() {
        relocate_refs(root, module);
    }

    modules
        .into_iter()
        .map(|(module, definitions)| {
            let mut document = roots.remove(module).unwrap_or_else(|| {
                let mut document = Map::new();
                if let Some(dialect) = &dialect {
                    document.insert("$schema".to_owned(), dialect.clone());
                }
                document.insert(
                    "title".to_owned(),
                    Value::String(format!("The definitions of the {module} module")),
                );
                Value::Object(document)
            });
            document
                .as_object_mut()
                .expect("a schema document is an object")
                .insert("$defs".to_owned(), Value::Object(definitions));
            let mut text =
                serde_json::to_string_pretty(&document).expect("a JSON value always serialises");
            text.push('\n');
            (PathBuf::from(format!("{module}.schema.json")), text)
        })
        .collect()
}

/// Generates the Protocol Buffers definition of the `.fig` format as one `.proto` file
/// per module of this crate, in package `ironlab.ir.v0`, with paths relative to the
/// root of a proto source tree (such as `ironlab/ir/v0/figure.proto`).
///
/// The files are rendered from the same declarations that define the [`wire`] types,
/// so they describe exactly the bytes that [`Figure::to_protobuf`] writes. They are
/// written, together with a `buf.yaml`, to `target/ironlab-proto/` by
/// `cargo run -p ironlab-ir --bin generate-proto`.
pub fn proto_files() -> Vec<(PathBuf, String)> {
    wire::schema::render_files()
}
