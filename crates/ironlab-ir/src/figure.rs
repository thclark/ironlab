//! The figure: the root node of the IR.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artist::Artist;
use crate::axes::Axes;
use crate::data::NdArray;
use crate::error::IrError;
use crate::ids::{DataId, NodeId};
use crate::link::AxisLink;
use crate::style::Color;
use crate::text::Text;

/// The version of the figure schema implemented by this build.
///
/// Files are compatible when their major and minor components equal this version's;
/// the patch component may differ.
pub const SCHEMA_VERSION: &str = "0.2.0";

/// A figure: the root of the retained IR, holding axes, data and axis links.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Figure {
    /// The version of the figure schema that the file conforms to.
    pub schema_version: String,
    /// The node identifier of the figure, unique within the figure.
    pub id: NodeId,
    /// The title drawn above all axes (sgtitle).
    pub title: Option<Text>,
    /// The physical size of the figure.
    pub size: FigureSize,
    /// The font set used for all text.
    pub font_set: FontSetId,
    /// The base font size in points; titles and tick labels are scaled from it.
    pub font_size_pt: f64,
    /// The colour of the figure background.
    pub background: Color,
    /// The grid of cells in which axes are placed.
    pub layout: TileLayout,
    /// The numeric arrays referenced by artists, keyed by data identifier.
    pub data: BTreeMap<DataId, NdArray>,
    /// The axes of the figure, in drawing order.
    pub axes: Vec<Axes>,
    /// The groups of axes whose limits are linked along a dimension.
    pub links: Vec<AxisLink>,
    /// A record of the software that produced the figure.
    pub provenance: Provenance,
    /// Named values that describe the figure, used to sort, filter and search
    /// collections of figures, in ascending order of name.
    ///
    /// The property is omitted from JSON when there are no parameters.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, Parameter>,
    /// The state used to allocate node identifiers; not part of the figure's value.
    #[serde(skip)]
    #[schemars(skip)]
    pub id_allocator: NodeIdAllocator,
}

impl Default for Figure {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_owned(),
            id: NodeId(0),
            title: None,
            size: FigureSize::default(),
            font_set: FontSetId::StixTwo,
            font_size_pt: 9.0,
            background: Color::WHITE,
            layout: TileLayout::default(),
            data: BTreeMap::new(),
            axes: Vec::new(),
            links: Vec::new(),
            provenance: Provenance::default(),
            parameters: BTreeMap::new(),
            id_allocator: NodeIdAllocator::default(),
        }
    }
}

/// A named value that describes a figure, such as the Reynolds number of the flow that
/// it shows or the name of the solver that produced its data.
///
/// Parameters do not affect drawing. They exist so that collections of figures can be
/// sorted, filtered and searched. In JSON, a parameter is an object whose `type` names
/// its kind and whose `value` holds the value, such as `{"type": "number", "value":
/// 100000.0}`, so that the kind survives JSON's single number type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Parameter {
    /// A boolean.
    Bool(bool),
    /// A signed 64-bit integer.
    Integer(i64),
    /// A double-precision floating-point number, which must be finite.
    Number(f64),
    /// A string of Unicode text.
    String(String),
}

impl From<bool> for Parameter {
    fn from(value: bool) -> Self {
        Parameter::Bool(value)
    }
}

impl From<i32> for Parameter {
    fn from(value: i32) -> Self {
        Parameter::Integer(i64::from(value))
    }
}

impl From<i64> for Parameter {
    fn from(value: i64) -> Self {
        Parameter::Integer(value)
    }
}

impl From<f64> for Parameter {
    fn from(value: f64) -> Self {
        Parameter::Number(value)
    }
}

impl From<&str> for Parameter {
    fn from(value: &str) -> Self {
        Parameter::String(value.to_owned())
    }
}

impl From<String> for Parameter {
    fn from(value: String) -> Self {
        Parameter::String(value)
    }
}

/// The allocation state for node identifiers of a figure.
///
/// It remembers identifiers already handed out but not yet inserted into the figure,
/// so that successive allocations are distinct. It is not serialised, and it never
/// affects equality of figures.
#[derive(Debug, Clone, Copy, Default)]
pub struct NodeIdAllocator {
    /// The smallest identifier that has not been handed out by this allocator.
    pub next: u64,
}

impl PartialEq for NodeIdAllocator {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// The physical size of a figure.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FigureSize {
    /// The width in millimetres.
    pub width_mm: f64,
    /// The height in millimetres.
    pub height_mm: f64,
}

impl Default for FigureSize {
    fn default() -> Self {
        Self {
            width_mm: 160.0,
            height_mm: 100.0,
        }
    }
}

/// The identifier of a bundled font set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontSetId {
    /// STIX Two Text for text and STIX Two Math for mathematics.
    #[default]
    StixTwo,
}

/// The grid of cells in which the axes of a figure are placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct TileLayout {
    /// The number of rows; at least one.
    pub rows: u32,
    /// The number of columns; at least one.
    pub cols: u32,
}

impl Default for TileLayout {
    fn default() -> Self {
        Self { rows: 1, cols: 1 }
    }
}

/// A record of the software that produced a figure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Provenance {
    /// The version of IronLAB that wrote the figure.
    pub ironlab_version: String,
    /// The name and version of the mathematics typesetter.
    pub typesetter: String,
    /// The names of the fonts used for text.
    pub fonts: Vec<String>,
}

impl Default for Provenance {
    fn default() -> Self {
        Self {
            ironlab_version: env!("CARGO_PKG_VERSION").to_owned(),
            typesetter: "latex-rust 1.0.2".to_owned(),
            fonts: vec!["STIX Two Text".to_owned(), "STIX Two Math".to_owned()],
        }
    }
}

impl Figure {
    /// Creates an empty figure with default properties.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates a node identifier that differs from the figure's identifier, every
    /// axes and artist identifier in the figure, and every identifier previously
    /// allocated from this figure value.
    ///
    /// Identifiers are allocated above the largest identifier present, so allocation
    /// remains correct after a figure is loaded from JSON.
    ///
    /// # Panics
    ///
    /// Panics when no identifier remains above both the largest identifier present
    /// and every identifier previously allocated, which can only happen when the
    /// figure contains an identifier close to `u64::MAX`.
    pub fn alloc_node_id(&mut self) -> NodeId {
        const EXHAUSTED: &str = "the node identifier space is exhausted";
        let largest = self.node_ids().map(|id| id.0).max().unwrap_or(self.id.0);
        let id = largest
            .checked_add(1)
            .expect(EXHAUSTED)
            .max(self.id_allocator.next);
        self.id_allocator.next = id.checked_add(1).expect(EXHAUSTED);
        NodeId(id)
    }

    /// Returns the identifiers of every node: the figure, its axes and their artists.
    pub(crate) fn node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::once(self.id).chain(
            self.axes.iter().flat_map(|axes| {
                std::iter::once(axes.id).chain(axes.artists.iter().map(Artist::id))
            }),
        )
    }

    /// Inserts an array into the data table under a new identifier, which differs
    /// from every identifier already in the table, and returns that identifier.
    pub fn add_data(&mut self, array: NdArray) -> DataId {
        let id = match self.data.last_key_value() {
            None => DataId(0),
            Some((last, _)) => match last.0.checked_add(1) {
                Some(next) => DataId(next),
                // The largest identifier is taken, so reuse the first gap instead.
                None => (0..)
                    .map(DataId)
                    .zip(self.data.keys())
                    .find(|(candidate, used)| candidate != *used)
                    .map(|(candidate, _)| candidate)
                    .expect("the data identifier space is exhausted"),
            },
        };
        self.data.insert(id, array);
        id
    }

    /// Returns the axes with the given identifier.
    pub fn axes(&self, id: NodeId) -> Option<&Axes> {
        self.axes.iter().find(|axes| axes.id == id)
    }

    /// Returns the axes with the given identifier, mutably.
    pub fn axes_mut(&mut self, id: NodeId) -> Option<&mut Axes> {
        self.axes.iter_mut().find(|axes| axes.id == id)
    }

    /// Returns the artist with the given identifier together with the axes that
    /// contains it.
    pub fn artist(&self, id: NodeId) -> Option<(&Axes, &Artist)> {
        self.axes.iter().find_map(|axes| {
            axes.artists
                .iter()
                .find(|artist| artist.id() == id)
                .map(|artist| (axes, artist))
        })
    }

    /// Returns the artist with the given identifier, mutably.
    pub fn artist_mut(&mut self, id: NodeId) -> Option<&mut Artist> {
        self.axes
            .iter_mut()
            .flat_map(|axes| axes.artists.iter_mut())
            .find(|artist| artist.id() == id)
    }

    /// Serialises the figure as pretty-printed JSON (the `.fig.json` format).
    ///
    /// Floating-point numbers are written with the shortest representation that reads
    /// back to the same value, so a saved figure reloads bit for bit. Non-finite
    /// values in data arrays are written as `null` and reload as NaN. JSON cannot
    /// represent a non-finite value in any other numeric field (such as a limit, a
    /// view angle, a size or a number parameter): such a value is written as `null`,
    /// which [`Figure::from_json`] then rejects, so only figures whose scalar fields
    /// are finite (as [`Figure::validate`] and [`Figure::set_limits`] require) survive
    /// a round trip.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a figure always serialises to JSON")
    }

    /// Parses a figure from JSON (the `.fig.json` format).
    ///
    /// The schema version is checked before the rest of the document, so that a file
    /// from an incompatible version is reported as such rather than as malformed.
    ///
    /// Fields that this build does not know are ignored, so that a file written by a
    /// later patch release of the same minor schema version still loads. The loaded
    /// figure is not validated; call [`Figure::validate`] to check it.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::IncompatibleSchemaVersion`] when the declared schema version
    /// is not a `major.minor.patch` version with the same major and minor components
    /// as [`SCHEMA_VERSION`], and [`IrError::Json`] when the text is not valid JSON or
    /// does not describe a figure.
    pub fn from_json(json: &str) -> Result<Figure, IrError> {
        /// The part of a figure document read before the rest.
        #[derive(Deserialize)]
        struct VersionProbe {
            schema_version: serde_json::Value,
        }

        let probe: VersionProbe = serde_json::from_str(json)?;
        if !probe.schema_version.as_str().is_some_and(is_compatible) {
            let found = match probe.schema_version {
                serde_json::Value::String(version) => version,
                other => other.to_string(),
            };
            return Err(IrError::IncompatibleSchemaVersion {
                found,
                supported: SCHEMA_VERSION,
            });
        }
        Ok(serde_json::from_str(json)?)
    }
}

impl Figure {
    /// Encodes the figure as Protocol Buffers bytes (the `.fig` format).
    ///
    /// The message is described by the generated `.proto` files of package
    /// `ironlab.ir.v0` (see [`proto_files`](crate::proto_files)). Every floating-point
    /// value, including NaN with its payload, infinities, negative zero and subnormals,
    /// is stored as its IEEE 754 bits, in data arrays and in every other field. A figure
    /// always encodes to the same bytes.
    pub fn to_protobuf(&self) -> Vec<u8> {
        use prost::Message;

        crate::wire::Figure::from(self).encode_to_vec()
    }

    /// Decodes a figure from Protocol Buffers bytes (the `.fig` format).
    ///
    /// The schema version (field 1) is checked before the rest of the message is
    /// decoded, so that a file from an incompatible version is reported as such
    /// rather than as malformed. Fields that this build does not know are skipped, so
    /// that a file written by a later patch release of the same minor schema version
    /// still loads. Absent fields and unspecified enum values take the defaults of
    /// their context, except where the domain has no meaningful default (such as a
    /// node identifier or a reference to a data array), as described in
    /// [`wire`](crate::wire). The loaded figure is not validated; call
    /// [`Figure::validate`] to check it.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::IncompatibleSchemaVersion`] when the declared schema version
    /// is not a `major.minor.patch` version with the same major and minor components
    /// as [`SCHEMA_VERSION`], and [`IrError::Protobuf`] when the bytes are not a valid
    /// encoding of a figure, hold an enum value that this build does not define, or
    /// omit a value that has no default.
    pub fn from_protobuf(bytes: &[u8]) -> Result<Figure, IrError> {
        use prost::Message;

        /// The part of a figure message decoded before the rest: field 1, which every
        /// version of the schema declares as the schema version. Every other field is
        /// skipped as unknown, whatever its wire type.
        #[derive(Clone, PartialEq, prost::Message)]
        struct VersionProbe {
            #[prost(string, tag = "1")]
            schema_version: String,
        }

        let probe = VersionProbe::decode(bytes)?;
        if !is_compatible(&probe.schema_version) {
            return Err(IrError::IncompatibleSchemaVersion {
                found: probe.schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        Figure::try_from(crate::wire::Figure::decode(bytes)?)
    }
}

/// Returns whether a declared schema version is a `major.minor.patch` version with the
/// same major and minor components as [`SCHEMA_VERSION`].
fn is_compatible(version: &str) -> bool {
    parse_version(version)
        .zip(parse_version(SCHEMA_VERSION))
        .is_some_and(|(found, supported)| found[..2] == supported[..2])
}

/// Parses a `major.minor.patch` version whose components are unsigned decimal
/// integers.
fn parse_version(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.split('.').map(|part| {
        if !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()) {
            part.parse::<u64>().ok()
        } else {
            None
        }
    });
    let version = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(version)
}
