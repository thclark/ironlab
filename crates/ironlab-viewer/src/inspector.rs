//! What the property editor shows, and what a change made in it commits.
//!
//! This module is pure logic with no GPU or windowing dependency, so everything the
//! panel draws can be unit tested. It answers three questions:
//!
//! - **What is in the figure?** [`tree_rows`] turns the displayed figure into the rows of
//!   the object tree: the figure, its axes and their artists, each labelled as a user
//!   would name it.
//! - **What can be changed about one node?** [`property_groups`] reads the property
//!   registry of [`ironlab_ir::properties`], keeps the properties that the node can
//!   actually show, gathers them under the value they belong to, and says which widget
//!   each one needs and which of them the overlay overrides.
//! - **What does a change commit?** [`commit`] turns a new value into the transaction
//!   that the viewer records, which is one [`Edit::Set`] for every property except the
//!   limits of an axes, which go through [`ironlab_ir::command::set_limits`] so that the
//!   axes linked with it follow, exactly as a gesture on the canvas does.
//!
//! # What the editor does not change
//!
//! The editor changes the properties of the nodes a figure already has. The structure of
//! a figure — its tile layout, its axes, its plots — and the data those plots draw come
//! from the program that builds the figure, which is what a figure viewer is for.
//!
//! Three kinds of property are therefore shown read-only rather than offered as controls
//! that could not be used well:
//!
//! - A reference to a data array (the x data of a line, the grid of a surface) is shown
//!   with the shape of the array it names.
//! - The rows and columns of the figure's tile layout are shown as they are, because the
//!   layout is the frame the program placed its axes in. The cell of an axes stays
//!   editable, because moving an axes within that frame is a change to the axes.
//! - The groups of axes whose limits are linked are shown as a count.
//!
//! [`read_only_reason`] states the reason for each, which the panel shows, so that a
//! read-only row is never a dead control with nothing to say for itself.

use std::collections::{BTreeMap, BTreeSet};

use ironlab_ir::overlay::Overlay;
use ironlab_ir::{
    Artist, Axes, Cell, Choice, DataId, Dimension, Edit, Figure, Limits, NodeId, NodeKind,
    Parameter, Property, PropertyPath, Transaction, Value, ValueType, command, properties,
    property_choices,
};

// ---------------------------------------------------------------------------------
// The object tree
// ---------------------------------------------------------------------------------

/// One row of the object tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// The node the row selects.
    pub node: NodeId,
    /// The kind of the node, which decides its properties.
    pub kind: NodeKind,
    /// The name shown: the title of the figure or of an axes, the display name of an
    /// artist, or a name made from the node's cell or kind when it has none.
    pub label: String,
    /// The depth in the tree: 0 for the figure, 1 for an axes, 2 for an artist.
    pub depth: usize,
    /// Whether the row is drawn greyed, which a hidden artist is.
    pub dimmed: bool,
}

/// Returns the rows of the object tree of a figure: the figure, then each axes in
/// drawing order, each followed by its artists in drawing order.
///
/// An axes is labelled by its title when it has one and otherwise by the cell it
/// occupies, such as `Axes (row 0, column 1)`, because that is what tells two untitled
/// axes apart on screen. An artist is labelled by its display name when it has one and
/// otherwise by its kind. A hidden artist is marked [`dimmed`](TreeRow::dimmed).
#[must_use]
pub fn tree_rows(figure: &Figure) -> Vec<TreeRow> {
    let mut rows = vec![TreeRow {
        node: figure.id,
        kind: NodeKind::Figure,
        label: text_label(figure.title.as_ref()).unwrap_or_else(|| "Figure".to_owned()),
        depth: 0,
        dimmed: false,
    }];
    for axes in &figure.axes {
        rows.push(TreeRow {
            node: axes.id,
            kind: NodeKind::Axes,
            label: text_label(axes.title.as_ref()).unwrap_or_else(|| cell_label(axes)),
            depth: 1,
            dimmed: false,
        });
        for artist in &axes.artists {
            let kind = artist_kind(artist);
            rows.push(TreeRow {
                node: artist.id(),
                kind,
                label: text_label(artist.display_name())
                    .unwrap_or_else(|| kind_name(kind).to_owned()),
                depth: 2,
                dimmed: !artist.visible(),
            });
        }
    }
    rows
}

/// Returns the name of a kind of node, as the object tree and the inspector write it.
#[must_use]
pub fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Figure => "Figure",
        NodeKind::Axes => "Axes",
        NodeKind::Line => "Line",
        NodeKind::Scatter => "Scatter",
        NodeKind::Contour => "Contour",
        NodeKind::Quiver => "Quiver",
        NodeKind::Surface => "Surface",
    }
}

/// The content of a text, or `None` when there is none or it is blank, so that a title of
/// spaces does not leave a row with no name.
fn text_label(text: Option<&ironlab_ir::Text>) -> Option<String> {
    let content = text?.content.trim();
    (!content.is_empty()).then(|| content.to_owned())
}

/// The name of an axes that has no title, taken from the cell it occupies.
fn cell_label(axes: &Axes) -> String {
    let Cell { row, col, .. } = axes.cell;
    format!("Axes (row {row}, column {col})")
}

fn artist_kind(artist: &Artist) -> NodeKind {
    match artist {
        Artist::Line(_) => NodeKind::Line,
        Artist::Scatter(_) => NodeKind::Scatter,
        Artist::Contour(_) => NodeKind::Contour,
        Artist::Quiver(_) => NodeKind::Quiver,
        Artist::Surface(_) => NodeKind::Surface,
    }
}

// ---------------------------------------------------------------------------------
// The inspector
// ---------------------------------------------------------------------------------

/// The widget that a property needs.
#[derive(Clone, Debug, PartialEq)]
pub enum Editor {
    /// A checkbox.
    Bool,
    /// A drag value, with the amount it changes per point of pointer travel and the
    /// range that the IR constrains it to, if any.
    Number {
        /// The change per point of pointer travel.
        speed: f64,
        /// The inclusive bounds the IR requires, or `None` when it requires none.
        range: Option<(f64, f64)>,
        /// Whether the value is a whole number.
        integer: bool,
    },
    /// A text field of a plain string.
    Text,
    /// A text field and a combo box of interpreters, which a [`Text`](ironlab_ir::Text)
    /// needs; the content and the interpreter below it are not listed separately.
    RichText,
    /// A combo box of every choice of this property of this node, from
    /// [`property_choices`], each marked with whether it is available here.
    Choice {
        /// The choices offered, in the order they are offered. A choice the IR would not
        /// act on here is listed like the rest, carrying the reason it cannot be taken,
        /// so that the panel can show it disabled rather than hide it.
        offered: Vec<Choice>,
    },
    /// A colour picker; the components below the colour are not listed separately.
    Color,
    /// A list of numbers, typed as text.
    Numbers,
    /// The named parameters of the figure, which have an editor of their own.
    Parameters,
    /// A reference to a data array, shown read-only with the shape of the array it
    /// refers to, or `None` when the figure has no such array.
    Data {
        /// The shape of the array referred to.
        shape: Option<Vec<usize>>,
    },
    /// A value that is only a container of the values below it, drawn as a heading.
    Group,
    /// A value that the editor shows but cannot change, with the reason it cannot, which
    /// the panel shows so that a read-only row is never a dead control.
    ReadOnly {
        /// Why the property cannot be changed here.
        reason: &'static str,
    },
}

/// One property of the selected node, as the inspector shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyRow {
    /// The path of the property within the node.
    pub path: PropertyPath,
    /// The label of the row: the path with the group's segment removed, which is empty
    /// for the property that names the group itself.
    pub label: String,
    /// How deeply the property is nested below the group: 0 for the group's own
    /// property, 1 for a property directly below it, and so on.
    pub depth: usize,
    /// The current value, which is [`Value::Unset`] when an optional value is absent.
    pub value: Value,
    /// The type of the property.
    pub value_type: ValueType,
    /// Whether the property may be absent, so that it can be unset and set again.
    pub optional: bool,
    /// The documentation of the property, shown as a tooltip.
    pub docs: &'static str,
    /// The widget the property needs.
    pub editor: Editor,
    /// Whether the overlay overrides exactly this property of this node.
    pub overridden: bool,
}

/// The properties of a node that belong to one value of it, such as its x axis.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyGroup {
    /// The first segment of the paths of the group's properties, such as `x`.
    pub name: String,
    /// The properties, in the order in which the registry lists them.
    pub rows: Vec<PropertyRow>,
}

/// Returns the properties of a node, grouped by the first segment of their paths, in the
/// order in which [`properties`] lists them.
///
/// Only the properties that the node can show are returned. A property that belongs to a
/// variant that is not set (the bounds of automatic limits, the camera of a
/// two-dimensional axes) and a property below an optional value that is absent (the
/// content of a title that the node does not have) are left out rather than shown as
/// errors; the value they hang from is still listed, so the user can set the variant or
/// give the absent value one. A value that is only a container of other values is left
/// out when those values are listed, because it has nothing to change of its own, and the
/// values below a text or a colour are left out because their editor covers them.
///
/// A row is marked [`overridden`](PropertyRow::overridden) when the overlay holds an
/// entry for exactly that node and path, which is what the panel marks and what its
/// revert control removes.
#[must_use]
pub fn property_groups(figure: &Figure, overlay: &Overlay, node: NodeId) -> Vec<PropertyGroup> {
    let Some(kind) = figure.node_kind(node) else {
        return Vec::new();
    };
    let overridden: BTreeSet<&PropertyPath> = overlay
        .entries()
        .iter()
        .filter(|entry| entry.node == node)
        .map(|entry| &entry.path)
        .collect();

    let mut covered: Vec<PropertyPath> = Vec::new();
    let mut rows: Vec<PropertyRow> = Vec::new();
    for property in properties(kind) {
        if covered
            .iter()
            .any(|above| *above != property.path && above.contains(&property.path))
        {
            continue;
        }
        let Ok(value) = figure.get(node, &property.path) else {
            continue;
        };
        let editor = editor_for(figure, kind, node, &property, &value);
        if matches!(editor, Editor::RichText | Editor::Color) {
            covered.push(property.path.clone());
        }
        let segments = property.path.segments();
        rows.push(PropertyRow {
            label: segments[1..].join("."),
            depth: segments.len() - 1,
            overridden: overridden.contains(&property.path),
            path: property.path,
            value,
            value_type: property.value_type,
            optional: property.optional,
            docs: property.docs,
            editor,
        });
    }

    // A container is a heading rather than a row, but only while the values it contains
    // are listed: an absent optional value has none, and its row is how it is given one.
    let containers: BTreeSet<PropertyPath> = rows
        .iter()
        .filter(|row| row.editor == Editor::Group)
        .filter(|row| {
            rows.iter()
                .any(|other| other.path != row.path && row.path.contains(&other.path))
        })
        .map(|row| row.path.clone())
        .collect();
    rows.retain(|row| !containers.contains(&row.path));

    let mut groups: Vec<PropertyGroup> = Vec::new();
    for row in rows {
        let name = row.path.segments()[0].clone();
        match groups.last_mut() {
            Some(group) if group.name == name => group.rows.push(row),
            _ => groups.push(PropertyGroup {
                name,
                rows: vec![row],
            }),
        }
    }
    groups
}

/// The reason the rows and columns of the figure's tile layout are not changed here.
const TILE_LAYOUT_REASON: &str = "The tile layout is set by the program that builds the \
     figure, together with the axes placed in it. The cell an axes occupies can be \
     changed here, which moves it within that layout.";

/// The reason the groups of linked axes are not changed here.
const LINKS_REASON: &str = "The groups of axes whose limits are linked are set by the \
     program that builds the figure, as part of how its axes relate to one another.";

/// Returns why a property is shown but cannot be changed in the panel, or `None` when it
/// can be changed.
///
/// A property is read-only here when it describes the structure of the figure rather than
/// a property of a node: the editor changes what the program's nodes look like, not which
/// nodes there are. A property whose value the IR merely constrains — limits that must
/// increase, a positive font size — stays editable, because refusing the change and
/// saying why is the better answer there.
///
/// The match over the kinds of node is exhaustive, so a kind added to the IR does not
/// compile until it is said what of it is read-only.
#[must_use]
pub fn read_only_reason(kind: NodeKind, path: &PropertyPath) -> Option<&'static str> {
    let segments = path.segments();
    match kind {
        NodeKind::Figure => {
            if segments == ["layout", "rows"] || segments == ["layout", "cols"] {
                Some(TILE_LAYOUT_REASON)
            } else if segments == ["links"] {
                Some(LINKS_REASON)
            } else {
                None
            }
        }
        NodeKind::Axes
        | NodeKind::Line
        | NodeKind::Scatter
        | NodeKind::Contour
        | NodeKind::Quiver
        | NodeKind::Surface => None,
    }
}

/// Returns the widget that a property needs, from what the panel can usefully do with it,
/// its type and its current value.
///
/// The match over the value types is exhaustive, so a type added to the IR does not
/// compile until it is given an editor.
fn editor_for(
    figure: &Figure,
    kind: NodeKind,
    node: NodeId,
    property: &Property,
    value: &Value,
) -> Editor {
    if let Some(reason) = read_only_reason(kind, &property.path) {
        return Editor::ReadOnly { reason };
    }
    match property.value_type {
        ValueType::Bool => Editor::Bool,
        ValueType::UInt32 => number_editor(&property.path, value, true),
        ValueType::Double | ValueType::Float => number_editor(&property.path, value, false),
        ValueType::String => Editor::Text,
        ValueType::Text => Editor::RichText,
        ValueType::Color => Editor::Color,
        ValueType::Doubles => Editor::Numbers,
        ValueType::Parameters => Editor::Parameters,
        ValueType::Links => Editor::ReadOnly {
            reason: LINKS_REASON,
        },
        ValueType::DataId => Editor::Data {
            shape: match value {
                Value::DataId(id) => figure.data.get(id).map(|array| array.shape.clone()),
                _ => None,
            },
        },
        ValueType::Projection
        | ValueType::Limits
        | ValueType::ColorSpec
        | ValueType::ScatterSize
        | ValueType::ScatterColor
        | ValueType::Grid
        | ValueType::Levels
        | ValueType::ContourPlacement
        | ValueType::QuiverScale
        | ValueType::Scale
        | ValueType::ColormapName
        | ValueType::LegendLocation
        | ValueType::MarkerShape
        | ValueType::DashStyle
        | ValueType::Interpreter
        | ValueType::FontSetId => Editor::Choice {
            offered: property_choices(figure, node, &property.path),
        },
        ValueType::FigureSize
        | ValueType::TileLayout
        | ValueType::Cell
        | ValueType::View3d
        | ValueType::Axis
        | ValueType::Legend
        | ValueType::LineStyle
        | ValueType::MarkerStyle => Editor::Group,
    }
}

/// Returns the drag value of a number, with the speed that suits its magnitude and the
/// range that the IR requires of it.
///
/// The ranges are the ones the IR states: an elevation lies between -90 and 90 degrees,
/// and a count of rows, columns, spans or levels is at least one. Every other bound (a
/// positive figure size or font size, for example) is left to validation, which refuses
/// the change and says why, because the IR states no usable upper or lower value for it.
fn number_editor(path: &PropertyPath, value: &Value, integer: bool) -> Editor {
    let last = path.segments().last().map_or("", String::as_str);
    let range = match last {
        "elevation_deg" => Some((-90.0, 90.0)),
        "rows" | "cols" | "row_span" | "col_span" | "count" => Some((1.0, f64::from(u32::MAX))),
        _ if integer => Some((0.0, f64::from(u32::MAX))),
        _ => None,
    };
    let magnitude = match value {
        Value::Double(number) if number.is_finite() => number.abs(),
        Value::Float(number) if number.is_finite() => f64::from(number.abs()),
        _ => 1.0,
    };
    let speed = match last {
        _ if integer => 1.0,
        "azimuth_deg" | "elevation_deg" => 0.5,
        "r" | "g" | "b" | "a" | "zoom" | "pan_x" | "pan_y" | "head_size" => 0.01,
        _ => 0.01 * magnitude.max(1.0),
    };
    Editor::Number {
        speed,
        range,
        integer,
    }
}

// ---------------------------------------------------------------------------------
// From a change to a transaction
// ---------------------------------------------------------------------------------

/// Returns the transaction that commits a new value of a property.
///
/// Every property is committed as one [`Edit::Set`] of exactly that property, except the
/// limits of an axes: `x.limits`, `y.limits` and `z.limits`, and the bounds below them,
/// are committed through [`command::set_limits`], so that every axes linked with this one
/// along that dimension takes the same limits. A gesture on the canvas sets limits the
/// same way, so editing and dragging leave the same figure.
///
/// A bound below automatic limits, and limits that the command refuses, fall back to one
/// set of the property named, which the viewer then refuses and reports in the usual way.
#[must_use]
pub fn commit(figure: &Figure, node: NodeId, path: &PropertyPath, value: Value) -> Transaction {
    limits_transaction(figure, node, path, &value).unwrap_or(Transaction {
        edits: vec![Edit::Set {
            node,
            path: path.clone(),
            value,
        }],
    })
}

/// The transaction that sets the limits of an axes and of the axes linked with it, or
/// `None` when the path is not the limits of an axes or the command refuses them.
fn limits_transaction(
    figure: &Figure,
    node: NodeId,
    path: &PropertyPath,
    value: &Value,
) -> Option<Transaction> {
    let segments = path.segments();
    let dimension = match segments.first()?.as_str() {
        "x" => Dimension::X,
        "y" => Dimension::Y,
        "z" => Dimension::Z,
        _ => return None,
    };
    if segments.get(1)? != "limits" {
        return None;
    }
    let axes = figure.axes(node)?;
    let current = match dimension {
        Dimension::X => axes.x.limits,
        Dimension::Y => axes.y.limits,
        Dimension::Z => axes.z.limits,
    };
    let limits = match (segments.len(), value) {
        (2, Value::Limits(limits)) => *limits,
        (3, Value::Double(bound)) => {
            let Limits::Manual { min, max } = current else {
                return None;
            };
            match segments[2].as_str() {
                "min" => Limits::Manual { min: *bound, max },
                "max" => Limits::Manual { min, max: *bound },
                _ => return None,
            }
        }
        _ => return None,
    };
    command::set_limits(figure, node, dimension, limits).ok()
}

// ---------------------------------------------------------------------------------
// The parameters of a figure
// ---------------------------------------------------------------------------------

/// The kind of a [`Parameter`], which the parameters editor offers in a combo box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParameterKind {
    /// A true or false value.
    Bool,
    /// A whole number.
    Integer,
    /// A number.
    Number,
    /// A string of text.
    #[default]
    String,
}

impl ParameterKind {
    /// Every kind, in the order the combo box offers them.
    pub const ALL: [ParameterKind; 4] = [
        ParameterKind::Bool,
        ParameterKind::Integer,
        ParameterKind::Number,
        ParameterKind::String,
    ];

    /// The name of the kind, written for a combo box.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ParameterKind::Bool => "Yes or no",
            ParameterKind::Integer => "Whole number",
            ParameterKind::Number => "Number",
            ParameterKind::String => "Text",
        }
    }

    /// The kind of an existing parameter.
    #[must_use]
    pub fn of(parameter: &Parameter) -> Self {
        match parameter {
            Parameter::Bool(_) => ParameterKind::Bool,
            Parameter::Integer(_) => ParameterKind::Integer,
            Parameter::Number(_) => ParameterKind::Number,
            Parameter::String(_) => ParameterKind::String,
        }
    }
}

/// One entry of the parameters editor, while it is being edited.
#[derive(Clone, Debug, PartialEq)]
pub struct ParameterRow {
    /// The name of the parameter.
    pub name: String,
    /// The kind of value it holds.
    pub kind: ParameterKind,
    /// The value as typed, used by every kind but [`ParameterKind::Bool`].
    pub text: String,
    /// The value of a [`ParameterKind::Bool`] parameter.
    pub flag: bool,
}

/// The named parameters of a figure while they are being edited.
///
/// The parameters of a figure are one property, set as a whole, but they are edited
/// entry by entry: an entry is added, renamed, given another kind, changed or removed. A
/// draft holds the entries as the user has typed them, so that a half-typed number or a
/// name being retyped is a state of the editor rather than a change to the figure, and
/// [`ParametersDraft::to_map`] says whether the draft can be committed and why not when
/// it cannot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParametersDraft {
    rows: Vec<ParameterRow>,
}

impl ParametersDraft {
    /// Creates a draft of the parameters of a figure, in the order they are stored
    /// (ascending by name).
    #[must_use]
    pub fn of(parameters: &BTreeMap<String, Parameter>) -> Self {
        Self {
            rows: parameters
                .iter()
                .map(|(name, parameter)| ParameterRow {
                    name: name.clone(),
                    kind: ParameterKind::of(parameter),
                    text: match parameter {
                        Parameter::Bool(_) => String::new(),
                        Parameter::Integer(value) => value.to_string(),
                        Parameter::Number(value) => value.to_string(),
                        Parameter::String(value) => value.clone(),
                    },
                    flag: matches!(parameter, Parameter::Bool(true)),
                })
                .collect(),
        }
    }

    /// Returns the entries, in the order they are shown.
    #[must_use]
    pub fn rows(&self) -> &[ParameterRow] {
        &self.rows
    }

    /// Returns the entries for editing.
    pub fn rows_mut(&mut self) -> &mut [ParameterRow] {
        &mut self.rows
    }

    /// Appends an entry of text under a name that no other entry has.
    pub fn add(&mut self) {
        let taken: BTreeSet<&str> = self.rows.iter().map(|row| row.name.as_str()).collect();
        let mut name = "parameter".to_owned();
        let mut suffix = 1;
        while taken.contains(name.as_str()) {
            suffix += 1;
            name = format!("parameter {suffix}");
        }
        self.rows.push(ParameterRow {
            name,
            kind: ParameterKind::String,
            text: String::new(),
            flag: false,
        });
    }

    /// Removes the entry at an index, and does nothing when there is none.
    pub fn remove(&mut self, index: usize) {
        if index < self.rows.len() {
            self.rows.remove(index);
        }
    }

    /// Renames the entry at an index.
    pub fn set_name(&mut self, index: usize, name: &str) {
        if let Some(row) = self.rows.get_mut(index) {
            row.name = name.to_owned();
        }
    }

    /// Changes the kind of the entry at an index.
    pub fn set_kind(&mut self, index: usize, kind: ParameterKind) {
        if let Some(row) = self.rows.get_mut(index) {
            row.kind = kind;
        }
    }

    /// Changes the typed value of the entry at an index.
    pub fn set_text(&mut self, index: usize, text: &str) {
        if let Some(row) = self.rows.get_mut(index) {
            row.text = text.to_owned();
        }
    }

    /// Returns the parameters the draft describes.
    ///
    /// # Errors
    ///
    /// Returns a sentence saying why the draft cannot be committed: an entry has no name,
    /// two entries share a name (which would silently lose one of them), or a number is
    /// not one that the IR accepts.
    pub fn to_map(&self) -> Result<BTreeMap<String, Parameter>, String> {
        let mut map = BTreeMap::new();
        for row in &self.rows {
            let name = row.name.trim();
            if name.is_empty() {
                return Err("a parameter has no name".to_owned());
            }
            let parameter = match row.kind {
                ParameterKind::Bool => Parameter::Bool(row.flag),
                ParameterKind::String => Parameter::String(row.text.clone()),
                ParameterKind::Integer => Parameter::Integer(
                    row.text
                        .trim()
                        .parse::<i64>()
                        .map_err(|_| format!("{name} is not a whole number: {:?}", row.text))?,
                ),
                ParameterKind::Number => {
                    let value = row
                        .text
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| format!("{name} is not a finite number: {:?}", row.text))?;
                    Parameter::Number(value)
                }
            };
            if map.insert(name.to_owned(), parameter).is_some() {
                return Err(format!("two parameters are named {name}"));
            }
        }
        Ok(map)
    }
}

/// Returns the value of a read-only property, written for the row that shows it.
///
/// A read-only row shows what the figure holds rather than a control, so each value is
/// written as a reader would say it rather than as the IR stores it.
#[must_use]
pub fn read_only_label(value: &Value) -> String {
    match value {
        Value::UInt32(number) => number.to_string(),
        Value::Double(number) => number.to_string(),
        Value::Float(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => if *flag { "yes" } else { "no" }.to_owned(),
        Value::Links(links) => match links.len() {
            0 => "no linked axes".to_owned(),
            1 => "1 group of linked axes".to_owned(),
            groups => format!("{groups} groups of linked axes"),
        },
        Value::Unset => "unset".to_owned(),
        other => format!("{other:?}"),
    }
}

/// Returns the shape of a data array, written for the read-only row of a data reference.
#[must_use]
pub fn shape_label(shape: Option<&[usize]>) -> String {
    match shape {
        None => "no such array".to_owned(),
        Some(shape) => {
            let lengths: Vec<String> = shape.iter().map(usize::to_string).collect();
            format!("[{}]", lengths.join(" × "))
        }
    }
}

/// Returns the identifier a data reference holds, for the read-only row that shows it.
#[must_use]
pub fn data_id(value: &Value) -> Option<DataId> {
    match value {
        Value::DataId(id) => Some(*id),
        _ => None,
    }
}
