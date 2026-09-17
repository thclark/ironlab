//! Retained figure intermediate representation for IronLAB.
//!
//! The types in this crate are the single source of truth for the IronLAB figure
//! format. A [`Figure`] is a serialisable tree of nodes (the figure, its axes and
//! their artists), each identified by a stable [`NodeId`], together with a table of
//! numeric arrays referenced by [`DataId`].
//!
//! # Formats
//!
//! The default file and transport format is Protocol Buffers (`.fig`), written by
//! [`Figure::to_protobuf`] and read by [`Figure::from_protobuf`] through the wire types
//! of the [`wire`] module. JSON (`.fig.json`), written by [`Figure::to_json`] and read by
//! [`Figure::from_json`], is a supported secondary format for debugging, other tools and
//! simple web pages. Both describe the same figure, so converting between them loses
//! nothing.
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
pub mod command;
mod data;
mod edit;
mod error;
mod figure;
mod ids;
mod link;
pub mod overlay;
pub mod selection;
mod style;
mod text;
mod validate;
pub mod wire;

pub use artist::{
    Artist, Contour, ContourPlacement, Grid, Levels, Line, Quiver, QuiverScale, Scatter,
    ScatterColor, ScatterSize, Surface,
};
pub use axes::{
    Axes, Axis, Cell, ColormapName, Legend, LegendLocation, Limits, Projection, Scale, View3d,
};
pub use data::NdArray;
pub use edit::{
    Choice, Edit, EditError, Node, NodeKind, PathError, Property, PropertyPath, Transaction, Value,
    ValueType, choices, properties,
};
pub use error::{IrError, ProtobufError};
pub use figure::{
    Figure, FigureSize, FontSetId, NodeIdAllocator, Parameter, Provenance, SCHEMA_VERSION,
    TileLayout,
};
pub use ids::{DataId, NodeId};
pub use link::{AxisLink, Dimension};
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
            "NodeId" | "DataId" => "ids",
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
