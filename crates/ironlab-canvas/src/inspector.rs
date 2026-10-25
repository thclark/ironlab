//! What the property editor shows, and what a change made in it commits.
//!
//! This module is pure logic with no GPU or windowing dependency, so everything the
//! panel draws can be unit tested. It answers three questions:
//!
//! - **What is in the figure?** [`tree_rows`] turns the displayed figure into the rows of
//!   the object tree: the figure, its data arrays, its colour scales, its axes and their
//!   artists, and its panels, each labelled as a user would name it.
//! - **What can be changed about one node?** [`property_groups`] reads the properties
//!   of the node as it stands from [`ironlab_ir::properties_of`] (the registry of its kind
//!   and one property per entry of each palette it holds), keeps the properties that the
//!   node can actually show, gathers them under the value they belong to, orders them by the name
//!   the user reads, and says which widget each one needs and which of them the overlay
//!   overrides.
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
//! **A property the editor cannot change is shown with its value, drawn dimmed, and the
//! reason it cannot be changed in its tooltip, and with nothing else beside it.** That is
//! the whole rule, and it holds for every such property:
//!
//! - A reference to a data array (the x data of a series, the grid of a surface) is shown
//!   with the shape of the array it names; [`DATA_REASON`] says why.
//! - The rows and columns of the figure's tile layout, and the cell each axes or panel
//!   occupies within it, are shown as they are, because the layout is the frame the
//!   program placed its axes in and where an axes sits in that frame is part of it.
//! - The panel in which an axes lists its legend entries is shown as the identifier of
//!   the panel, because which axes share a legend is part of how the figure is arranged.
//! - What a panel holds (a legend, in this version) is shown as it is, because which key a
//!   panel holds is part of how the figure is arranged; the properties of that key stay
//!   editable.
//! - The name of a data array is shown as it is, because a data array, like the data a
//!   plot draws, comes from the program that builds the figure.
//! - The colour scale an artist maps its numbers through is shown as the identifier of
//!   the scale, because which artists share a scale is part of how the figure is arranged;
//!   the properties of the scale itself stay editable on the scale.
//! - The groups of axes whose limits are linked are shown as a count.
//!
//! [`read_only_reason`] states the reason for each of the last six, and the panel draws
//! all seven through one function, so that a read-only row is never a dead control with
//! nothing to say for itself and never grows an ornament of its own.
//!
//! A value that is only a container of the values below it is not a read-only row but a
//! heading: [`is_composite`] names those types, and a heading carries its name alone,
//! because the values it holds are the rows beneath it.
//!
//! A property that the figure ignores altogether is hidden instead of shown read-only,
//! because there is nothing about it worth reading. [`is_shown`] decides that.
//!
//! The entries of a palette — the colours of an indexed colour scale, the categories of a
//! categorical one and the colours of a listed colormap — each have rows of their own: a
//! colour row per entry, and a colour row and a label row per category, ordered by index.
//! The list itself has no row, because the IR gives it no settable path: how many entries
//! a palette has, and in which order, comes from the program that builds the figure, as
//! the data a plot draws does.

use std::collections::{BTreeMap, BTreeSet};

use ironlab_ir::overlay::Overlay;
use ironlab_ir::{
    Artist, Cell, Choice, ColorScaleKind, DataArray, Dimension, Edit, Figure, Limits, NodeId,
    NodeKind, Panel, PanelContent, Parameter, Projection, Property, PropertyPath, Transaction,
    Value, ValueType, command, properties_of, property_choices,
};

// ---------------------------------------------------------------------------------
// The object tree
// ---------------------------------------------------------------------------------

/// One row of the object tree: a node of the figure, or a group that gathers the nodes of
/// one kind that the figure holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// The node the row selects, or `None` for a group, which selects nothing.
    pub node: Option<NodeId>,
    /// The kind of the node, which decides its properties, or the kind of the nodes that a
    /// group gathers.
    pub kind: NodeKind,
    /// The name shown: the kind of the node, followed in brackets by the title of the
    /// figure, of an axes or of a colour scale, the label of an artist, the name of a data
    /// array, or the cell of an axes that has no title or of a panel. A figure with no title
    /// and an artist with no label are shown as their kind alone, a data array with no name
    /// is shown with its shape, and a colour scale with no title with its kind. A group is
    /// shown as the plural of the kind it gathers.
    pub label: String,
    /// The depth in the tree: 0 for the figure, 1 for an axes or a group, and 2 for an
    /// artist or a member of a group.
    pub depth: usize,
    /// Whether the row is drawn greyed, which a hidden artist is.
    pub dimmed: bool,
}

/// Returns the rows of the object tree of a figure, in the order of the figure's tree: the
/// figure; the group "Data arrays" with each data array; the group "Colour scales" with
/// each colour scale; each axes in drawing order, each followed by its artists in drawing
/// order; and the group "Panels" with each panel. A group is listed only when the figure
/// has a node of its kind.
///
/// Every row begins with the kind of the object, and the name the object carries follows
/// it in brackets: `Figure (A damped oscillator)`, `Axes (Speed)`, `Line ($\sin \omega
/// t$)`, `Data array (pressure)`. The kind comes first because a title alone says nothing
/// about what carries it, and the name is shown as its source, because the tree does not
/// typeset LaTeX.
///
/// An axes with no title is named by the cell it occupies, as `Axes (row 0, col 1)`,
/// because that is what tells two untitled axes apart on screen. A panel with a title is
/// named by its title followed by its cell, as `Key (row 2, col 0)`, because two panels
/// may carry one title, and a panel without one by its content and its cell, as `Legend
/// panel (rows 0–1, col 2)`. A data array
/// with no name is named by its shape, as `Data array (1000 × 3)`, and a colour scale with
/// no title by its kind, as `Colour scale (continuous)`. A figure with no title
/// and an artist with no label are named by their kind alone, with no empty brackets. A
/// hidden artist is marked [`dimmed`](TreeRow::dimmed).
#[must_use]
pub fn tree_rows(figure: &Figure) -> Vec<TreeRow> {
    let row = |node: NodeId, kind: NodeKind, label: String, depth: usize| TreeRow {
        node: Some(node),
        kind,
        label,
        depth,
        dimmed: false,
    };
    let group = |kind: NodeKind, label: &str| TreeRow {
        node: None,
        kind,
        label: label.to_owned(),
        depth: 1,
        dimmed: false,
    };
    let mut rows = vec![row(
        figure.id,
        NodeKind::Figure,
        row_label(NodeKind::Figure, text_label(figure.title.as_ref())),
        0,
    )];
    if !figure.data.is_empty() {
        rows.push(group(NodeKind::DataArray, "Data arrays"));
    }
    for data in &figure.data {
        let name = row_label(NodeKind::DataArray, Some(data_array_name(data)));
        rows.push(row(data.id, NodeKind::DataArray, name, 2));
    }
    if !figure.color_scales.is_empty() {
        rows.push(group(NodeKind::ColorScale, "Colour scales"));
    }
    for scale in &figure.color_scales {
        let name = text_label(scale.title.as_ref()).unwrap_or_else(|| {
            match scale.kind {
                ColorScaleKind::Continuous(_) => "continuous",
                ColorScaleKind::Discrete(_) => "discrete",
                ColorScaleKind::Indexed(_) => "indexed",
                ColorScaleKind::Categorical(_) => "categorical",
            }
            .to_owned()
        });
        let name = row_label(NodeKind::ColorScale, Some(name));
        rows.push(row(scale.id, NodeKind::ColorScale, name, 2));
    }
    for axes in &figure.axes {
        let name = text_label(axes.title.as_ref()).unwrap_or_else(|| cell_label(axes.cell));
        rows.push(row(
            axes.id,
            NodeKind::Axes,
            row_label(NodeKind::Axes, Some(name)),
            1,
        ));
        for artist in &axes.artists {
            let kind = artist_kind(artist);
            rows.push(TreeRow {
                dimmed: !artist.visible(),
                ..row(
                    artist.id(),
                    kind,
                    row_label(kind, text_label(artist.label())),
                    2,
                )
            });
        }
    }
    if !figure.panels.is_empty() {
        rows.push(group(NodeKind::Panel, "Panels"));
    }
    for panel in &figure.panels {
        rows.push(row(panel.id, NodeKind::Panel, panel_label(panel), 2));
    }
    rows
}

/// The label of a row: the kind of the object, followed by the name it carries in
/// brackets, or the kind alone when it carries none.
fn row_label(kind: NodeKind, name: Option<String>) -> String {
    match name {
        Some(name) => format!("{} ({name})", kind_name(kind)),
        None => kind_name(kind).to_owned(),
    }
}

/// Returns the name of a kind of node, as the object tree and the inspector write it.
#[must_use]
pub fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Figure => "Figure",
        NodeKind::Axes => "Axes",
        NodeKind::Series => "Series",
        NodeKind::Contour => "Contour",
        NodeKind::Quiver => "Quiver",
        NodeKind::Surface => "Surface",
        NodeKind::Image => "Image",
        NodeKind::ScalarImage => "Scalar image",
        NodeKind::Bar => "Bar",
        NodeKind::Stairs => "Stairs",
        NodeKind::Cells => "Cells",
        NodeKind::DataArray => "Data array",
        NodeKind::ColorScale => "Colour scale",
        NodeKind::Panel => "Panel",
    }
}

/// The content of a text, or `None` when there is none or it is blank, so that a title of
/// spaces does not leave a row with no name.
fn text_label(text: Option<&ironlab_ir::Text>) -> Option<String> {
    let content = text?.content.trim();
    (!content.is_empty()).then(|| content.to_owned())
}

/// The name of a data array: its name, or its shape when it has none or its name is
/// blank.
fn data_array_name(data: &DataArray) -> String {
    let name = data.name.as_deref().map(str::trim).unwrap_or_default();
    if name.is_empty() {
        let lengths: Vec<String> = data.array.shape.iter().map(usize::to_string).collect();
        lengths.join(" × ")
    } else {
        name.to_owned()
    }
}

/// The name of a panel: its title, or the kind of its content when it has no title or a
/// blank one, followed in brackets by its cell.
fn panel_label(panel: &Panel) -> String {
    let name = text_label(panel.title.as_ref()).unwrap_or_else(|| {
        match panel.content {
            PanelContent::Legend(_) => "Legend panel",
            PanelContent::Colorbar(_) => "Colourbar panel",
        }
        .to_owned()
    });
    format!("{name} ({})", cell_label(panel.cell))
}

/// The name of a block of cells of the tile layout, by its rows and columns: a single
/// row or column by its index, as `row 1`, and a span by its first and last, as
/// `rows 0–1`.
fn cell_label(cell: Cell) -> String {
    let tracks = |noun: &str, first: u32, span: u32| match span {
        0 | 1 => format!("{noun} {first}"),
        span => format!("{noun}s {first}–{}", first.saturating_add(span - 1)),
    };
    format!(
        "{}, {}",
        tracks("row", cell.row, cell.row_span),
        tracks("col", cell.col, cell.col_span)
    )
}

fn artist_kind(artist: &Artist) -> NodeKind {
    match artist {
        Artist::Series(_) => NodeKind::Series,
        Artist::Contour(_) => NodeKind::Contour,
        Artist::Quiver(_) => NodeKind::Quiver,
        Artist::Surface(_) => NodeKind::Surface,
        Artist::Image(_) => NodeKind::Image,
        Artist::ScalarImage(_) => NodeKind::ScalarImage,
        Artist::Bar(_) => NodeKind::Bar,
        Artist::Stairs(_) => NodeKind::Stairs,
        Artist::Cells(_) => NodeKind::Cells,
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
    /// A list of numbers, typed as text, or a list of the sizes of the rows or columns
    /// of the tile layout, in which the word `fit` stands for a fitted size.
    Numbers,
    /// A list of words, typed as text: the tags of the figure.
    Words,
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
    /// The properties, ordered by the name the user reads.
    pub rows: Vec<PropertyRow>,
}

/// Orders items by a name, ignoring case and keeping the order of equal names.
///
/// The inspector is read by looking for a name, so its contents are ordered by the name
/// on screen rather than by the order the registry happens to list them in. Case is
/// ignored because a reader looking for `Limits` and a reader looking for `limits` are
/// looking for the same row, and the sort is stable so that two names that differ only in
/// case keep the order the registry gave them rather than swapping between frames.
///
/// A name is compared segment by segment, and a segment of digits, which names an entry of
/// a palette by its index, is compared as a number, so that entry 10 follows entry 9
/// rather than entry 1.
fn order_by_name<T>(items: &mut [T], name: impl Fn(&T) -> &str) {
    items.sort_by_cached_key(|item| name_key(name(item)));
}

/// One segment of a name as it is ordered: an index before any word, indices by value and
/// words ignoring case.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NameSegment {
    Index(u128),
    Word(String),
}

/// Returns the key by which a name is ordered.
fn name_key(name: &str) -> Vec<NameSegment> {
    name.split('.')
        .map(|segment| {
            let digits = !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit());
            match segment.parse() {
                Ok(index) if digits => NameSegment::Index(index),
                _ => NameSegment::Word(segment.to_lowercase()),
            }
        })
        .collect()
}

/// Returns the properties of a node, grouped by the first segment of their paths.
///
/// The groups are ordered by their names and the rows within each group by the names they
/// show, both ignoring case, so that a property can be found by reading down the panel for
/// its name. A group's name is what the panel writes at the top level, whether it names a
/// heading or the single row of a group that has one, so ordering the groups by name
/// orders the headings and the ungrouped rows together as one alphabetical list. The row
/// that names a group itself carries an empty label and therefore stays at the head of its
/// group, above the values it gathers.
///
/// Only the properties that the node can show are returned. A property that belongs to a
/// variant that is not set (the bounds of automatic limits, the camera of a
/// two-dimensional axes) and a property below an optional value that is absent (the
/// content of a title that the node does not have) are left out rather than shown as
/// errors; the value they hang from is still listed, so the user can set the variant or
/// give the absent value one. A property that the figure ignores in the state the node is
/// in is left out by [`is_shown`]. A value that is only a container of other values is
/// left out when those values are listed, because it has nothing to change of its own,
/// and the values below a text or a colour are left out because their editor covers them.
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
    for property in properties_of(figure, node) {
        if !is_shown(figure, node, kind, &property.path) {
            continue;
        }
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
        match groups.iter_mut().find(|group| group.name == name) {
            Some(group) => group.rows.push(row),
            None => groups.push(PropertyGroup {
                name,
                rows: vec![row],
            }),
        }
    }
    order_by_name(&mut groups, |group| &group.name);
    for group in &mut groups {
        order_by_name(&mut group.rows, |row| &row.label);
    }
    groups
}

/// Returns whether the panel shows a property of a node at all.
///
/// A property is hidden when the figure ignores it in the state the node is in, so that
/// the panel never offers a control that changes nothing and says nothing. Most such
/// properties cannot even be read: the camera of a two-dimensional axes belongs to a
/// variant that is not set, and [`property_groups`] leaves it out for that reason alone.
///
/// The z axis of a two-dimensional axes is the one property that must be hidden by a rule
/// of its own. An axes holds an x, a y and a z axis whatever its projection, so the label,
/// the scale, the limits and the grid lines of the z axis can all be read and set, and a
/// two-dimensional axes draws none of them. The rule therefore reads the projection of the
/// node rather than the path alone. It is asked afresh for every frame the panel draws, so
/// promoting an axes to three dimensions brings the rows back at once.
#[must_use]
pub fn is_shown(figure: &Figure, node: NodeId, kind: NodeKind, path: &PropertyPath) -> bool {
    if kind != NodeKind::Axes {
        return true;
    }
    let Some(first) = path.segments().first() else {
        return true;
    };
    let three_d = |figure: &Figure| {
        figure
            .axes(node)
            .is_none_or(|axes| matches!(axes.projection, Projection::ThreeD { .. }))
    };
    match first.as_str() {
        "z" => three_d(figure),
        _ => true,
    }
}

/// The reason the rows and columns of the figure's tile layout are not changed here.
const TILE_LAYOUT_REASON: &str = "The tile layout is set by the program that builds the \
     figure, together with the axes placed in it. The editor changes the properties of \
     the objects a figure has, not which objects there are or where they sit.";

/// The reason the cell an axes occupies is not changed here.
const CELL_REASON: &str = "The cell an axes occupies, like the tile layout it sits in, is \
     set by the program that builds the figure. The editor changes how a figure looks, \
     not how it is arranged.";

/// The reason the cell a panel occupies is not changed here.
const PANEL_CELL_REASON: &str = "The cell a panel occupies, like the tile layout it sits \
     in, is set by the program that builds the figure. The editor changes how a figure \
     looks, not how it is arranged.";

/// The reason the kind of content a panel holds is not changed here.
const PANEL_CONTENT_REASON: &str = "What a panel holds, like the cell it occupies, is set by \
     the program that builds the figure. The editor changes how the key in it looks, not \
     which key the figure has.";

/// The reason the panel an axes lists its legend entries in is not changed here.
const PANEL_REFERENCE_REASON: &str = "The panel in which an axes lists its legend entries \
     is set by the program that builds the figure, as part of how its axes and legends \
     relate to one another.";

/// The reason the name of a data array is not changed here.
const DATA_ARRAY_REASON: &str = "A data array, its name included, comes from the program \
     that builds the figure, which is where it is changed. The editor changes how the \
     figure looks, not what it draws.";

/// The reason the colour scale an artist maps its numbers through is not changed here.
const COLOR_SCALE_REFERENCE_REASON: &str = "The colour scale an artist maps its numbers \
     through is set by the program that builds the figure, as part of how its artists \
     share their scales. The scale itself is edited in its own row under Colour scales.";

/// The reason the colour scale a colourbar shows is not changed here.
const COLORBAR_SCALE_REASON: &str = "The colour scale a colourbar shows is set by the \
     program that builds the figure, as the scale of an artist is. The scale itself is edited \
     in its own row under Colour scales.";

/// The reason the explicit labels of the ticks of an axis are not changed here.
const TICK_LABELS_REASON: &str = "Explicit tick labels name the tick values one by one, and \
     both are set together by the program that builds the figure, which is where they are \
     changed.";

/// The reason the groups of linked axes are not changed here.
const LINKS_REASON: &str = "The groups of axes whose limits are linked are set by the \
     program that builds the figure, as part of how its axes relate to one another.";

/// The reason the coordinates in which a series places its values are not changed here.
const COORDS_REASON: &str = "Whether a plot places its values as data or as fractions of \
     the plot area is set by the program that builds the figure, because it decides what \
     the values mean. The editor changes how a figure looks, not what it draws.";

/// The reason the data a plot draws is not changed here, which the row of a data reference
/// shows exactly as every other read-only row shows its own reason.
pub const DATA_REASON: &str = "The data a plot draws comes from the program that builds \
     the figure, which is where it is changed. The editor changes how the figure looks, \
     not what it draws.";

/// Returns why a property is shown but cannot be changed in the panel, or `None` when it
/// can be changed.
///
/// A property is read-only here when it describes the structure of the figure rather than
/// a property of a node, or when another property of the same node overrides it: the
/// editor changes what the program's nodes look like, not which nodes there are, and a
/// control whose value another property overrules would do nothing and explain nothing. A
/// property whose value the IR merely constrains — limits that must increase, a positive
/// font size — stays editable, because refusing the change and saying why is the better
/// answer there.
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
        // The coordinates of a series say what its values mean, as its data does, so they
        // come from the program that builds the figure.
        NodeKind::Series if segments == ["x_coords"] || segments == ["y_coords"] => {
            Some(COORDS_REASON)
        }
        // Where an axes sits in the tile layout is part of the structure of the figure,
        // which the program that builds it defines, as the layout itself is.
        NodeKind::Axes if segments.first().is_some_and(|first| first == "cell") => {
            Some(CELL_REASON)
        }
        // Which legend panel an axes lists its entries in relates the axes to another
        // node of the figure, which is structure as the cell of an axes is.
        NodeKind::Axes if segments == ["legend", "panel"] => Some(PANEL_REFERENCE_REASON),
        NodeKind::Panel if segments.first().is_some_and(|first| first == "cell") => {
            Some(PANEL_CELL_REASON)
        }
        // Which kind of key a panel holds is structure, as its cell is; the properties of
        // the key it holds, which lie below `content`, stay editable.
        NodeKind::Panel if segments == ["content"] => Some(PANEL_CONTENT_REASON),
        // Which colour scale a colourbar shows relates it to another node of the figure, as
        // the scale of an artist does.
        NodeKind::Panel if segments == ["content", "colorbar", "scale"] => {
            Some(COLORBAR_SCALE_REASON)
        }
        NodeKind::Axes if segments == ["colorbar", "scale"] => Some(COLORBAR_SCALE_REASON),
        NodeKind::DataArray => Some(DATA_ARRAY_REASON),
        // Which colour scale an artist names relates the artist to another node of the
        // figure, which is structure as the panel of a legend is.
        NodeKind::Series
        | NodeKind::Contour
        | NodeKind::Surface
        | NodeKind::ScalarImage
        | NodeKind::Cells
            if segments == ["color_scale"] =>
        {
            Some(COLOR_SCALE_REFERENCE_REASON)
        }
        // The pixels or values of an image are a data reference, which is read-only by its
        // type, so an image has no read-only property of its own. Every property of a colour
        // scale that has a row is editable: the lists of its entries, categories and listed
        // colours have none.
        NodeKind::Axes
        | NodeKind::Series
        | NodeKind::Contour
        | NodeKind::Quiver
        | NodeKind::Surface
        | NodeKind::Image
        | NodeKind::ScalarImage
        | NodeKind::Bar
        | NodeKind::Stairs
        | NodeKind::Cells
        | NodeKind::ColorScale
        | NodeKind::Panel => None,
    }
}

/// Returns whether a value is only a container of the values below it, such as an axis, a
/// marker style, the cell an axes occupies or the placement of an image.
///
/// A container has nothing of its own to show or to change: what it holds is the rows
/// beneath it. The inspector therefore draws it as a heading carrying its name alone, and
/// never as a value beside that name. A tagged value such as limits, a projection, the
/// plane of an image or an out-of-range policy is not a container, because choosing its
/// variant is a change in itself.
///
/// The match over the value types is exhaustive, so a type added to the IR does not
/// compile until it is said whether it is a container.
#[must_use]
pub fn is_composite(value_type: ValueType) -> bool {
    match value_type {
        ValueType::FigureSize
        | ValueType::TileLayout
        | ValueType::Cell
        | ValueType::View3d
        | ValueType::Axis
        | ValueType::InsideLegend
        | ValueType::LegendEntryLayout
        | ValueType::PanelLegend
        | ValueType::PanelColorbar
        | ValueType::InsideColorbar
        | ValueType::LineStyle
        | ValueType::MarkerStyle
        | ValueType::ImagePlacement
        | ValueType::PixelRange
        | ValueType::ColorScaleOutOfRange
        | ValueType::ContinuousColorScale
        | ValueType::DiscreteColorScale
        | ValueType::IndexedColorScale
        | ValueType::CategoricalColorScale
        | ValueType::BarGroup
        | ValueType::CellGap => true,
        ValueType::Bool
        | ValueType::UInt32
        | ValueType::Double
        | ValueType::Float
        | ValueType::String
        | ValueType::NodeId
        | ValueType::Doubles
        | ValueType::TrackSizes
        | ValueType::Strings
        | ValueType::Texts
        | ValueType::Text
        | ValueType::Interpreter
        | ValueType::FontSetId
        | ValueType::Color
        | ValueType::Links
        | ValueType::Parameters
        | ValueType::Projection
        | ValueType::Scale
        | ValueType::Limits
        | ValueType::Ticks
        | ValueType::TickLabels
        | ValueType::Aspect
        | ValueType::AspectAdjust
        | ValueType::TitleAlign
        | ValueType::ColormapName
        | ValueType::Legend
        | ValueType::LegendLocation
        | ValueType::PanelLocation
        | ValueType::PanelContent
        | ValueType::ColorbarOrientation
        | ValueType::InsideColorbarSide
        | ValueType::ColorbarTitlePlacement
        | ValueType::LegendEntryColumns
        | ValueType::LegendEntryOrder
        | ValueType::ColorSpec
        | ValueType::DashStyle
        | ValueType::MarkerShape
        | ValueType::Coords
        | ValueType::Grid
        | ValueType::Levels
        | ValueType::ContourPlacement
        | ValueType::QuiverScale
        | ValueType::ImagePlane
        | ValueType::OutOfRange
        | ValueType::ColorScaleKind
        | ValueType::Colormap
        | ValueType::ColorScaleNorm
        | ValueType::ColorScaleLevels
        | ValueType::Orientation
        | ValueType::BarBase
        | ValueType::BarSlot
        | ValueType::BarLayout
        | ValueType::StairsData
        | ValueType::StepAt
        | ValueType::CellEdges => false,
    }
}

/// Returns the widget that a property needs, from what the panel can usefully do with it,
/// its type and its current value.
///
/// A container is a heading whatever else is true of it, so [`is_composite`] is asked
/// before [`read_only_reason`]. A read-only container would otherwise be a row, and a row
/// shows its value: the cell an axes occupies would print itself whole beside its own
/// name, where the panel should write `cell` and list `row`, `col`, `row_span` and
/// `col_span` beneath it.
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
    if is_composite(property.value_type) {
        return Editor::Group;
    }
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
        ValueType::Doubles | ValueType::TrackSizes => Editor::Numbers,
        ValueType::Strings => Editor::Words,
        ValueType::Texts => Editor::ReadOnly {
            reason: TICK_LABELS_REASON,
        },
        ValueType::Parameters => Editor::Parameters,
        ValueType::Links => Editor::ReadOnly {
            reason: LINKS_REASON,
        },
        // The panel of the legend of an axes and the colour scale of an artist are the
        // references to a node that are not data references, and [`read_only_reason`]
        // answered for them above, so every reference that reaches this arm refers to a
        // data array.
        ValueType::NodeId => Editor::Data {
            shape: data_id(value)
                .and_then(|id| figure.data_array(id))
                .map(|data| data.array.shape.clone()),
        },
        ValueType::Projection
        | ValueType::Limits
        | ValueType::Ticks
        | ValueType::TickLabels
        | ValueType::Aspect
        | ValueType::AspectAdjust
        | ValueType::TitleAlign
        | ValueType::ColorSpec
        | ValueType::Grid
        | ValueType::Levels
        | ValueType::ContourPlacement
        | ValueType::QuiverScale
        | ValueType::ImagePlane
        | ValueType::OutOfRange
        | ValueType::ColorScaleKind
        | ValueType::Colormap
        | ValueType::ColorScaleNorm
        | ValueType::ColorScaleLevels
        | ValueType::Scale
        | ValueType::ColormapName
        | ValueType::Legend
        | ValueType::LegendLocation
        | ValueType::PanelLocation
        | ValueType::PanelContent
        | ValueType::ColorbarOrientation
        | ValueType::InsideColorbarSide
        | ValueType::ColorbarTitlePlacement
        | ValueType::LegendEntryColumns
        | ValueType::LegendEntryOrder
        | ValueType::MarkerShape
        | ValueType::DashStyle
        | ValueType::Coords
        | ValueType::Interpreter
        | ValueType::FontSetId
        | ValueType::Orientation
        | ValueType::BarBase
        | ValueType::BarSlot
        | ValueType::BarLayout
        | ValueType::StairsData
        | ValueType::StepAt
        | ValueType::CellEdges => Editor::Choice {
            offered: property_choices(figure, node, &property.path),
        },
        // The container types, which [`is_composite`] answered for above. They are listed
        // again so that this match stays exhaustive and a type added to the IR must be
        // given an editor here as well as a place in `is_composite`.
        ValueType::FigureSize
        | ValueType::TileLayout
        | ValueType::Cell
        | ValueType::View3d
        | ValueType::Axis
        | ValueType::InsideLegend
        | ValueType::LegendEntryLayout
        | ValueType::PanelLegend
        | ValueType::PanelColorbar
        | ValueType::InsideColorbar
        | ValueType::LineStyle
        | ValueType::MarkerStyle
        | ValueType::ImagePlacement
        | ValueType::PixelRange
        | ValueType::ColorScaleOutOfRange
        | ValueType::ContinuousColorScale
        | ValueType::DiscreteColorScale
        | ValueType::IndexedColorScale
        | ValueType::CategoricalColorScale
        | ValueType::BarGroup
        | ValueType::CellGap => Editor::Group,
    }
}

/// Returns the drag value of a number, with the speed that suits its magnitude and the
/// range that the IR requires of it.
///
/// The ranges are the ones the IR states: an elevation lies between -90 and 90 degrees,
/// an opacity between 0 and 1, and a count of rows, columns, spans or levels is at least
/// one. Every other bound (a
/// positive figure size or font size, for example) is left to validation, which refuses
/// the change and says why, because the IR states no usable upper or lower value for it.
fn number_editor(path: &PropertyPath, value: &Value, integer: bool) -> Editor {
    let last = path.segments().last().map_or("", String::as_str);
    let range = match last {
        "elevation_deg" => Some((-90.0, 90.0)),
        "opacity" | "face_opacity" | "edge_opacity" | "line_opacity" | "fill_opacity" => {
            Some((0.0, 1.0))
        }
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
        "r" | "g" | "b" | "a" | "zoom" | "pan_x" | "pan_y" | "head_size" | "opacity"
        | "face_opacity" | "edge_opacity" | "line_opacity" | "fill_opacity" => 0.01,
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

    /// The name of the kind, written for a combo box as a scientist names a type.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ParameterKind::Bool => "boolean",
            ParameterKind::Integer => "integer",
            ParameterKind::Number => "number",
            ParameterKind::String => "string",
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
/// written as a reader would say it rather than as the IR stores it. No value is ever
/// written through [`Debug`], which would put the field names of the IR on screen.
#[must_use]
pub fn read_only_label(value: &Value) -> String {
    match value {
        Value::UInt32(number) => number.to_string(),
        Value::Double(number) => number.to_string(),
        Value::Float(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => if *flag { "yes" } else { "no" }.to_owned(),
        Value::NodeId(id) => id.to_string(),
        Value::Links(links) => match links.len() {
            0 => "no linked axes".to_owned(),
            1 => "1 group of linked axes".to_owned(),
            groups => format!("{groups} groups of linked axes"),
        },
        Value::Unset => "unset".to_owned(),
        // No other value reaches a read-only row: a container is drawn as a heading with
        // its members beneath it, and a tagged value is chosen from a list. Should one
        // ever arrive, it is named by its type rather than dumped, because the name of a
        // value is readable where the contents of one are not.
        other => other
            .value_type()
            .map_or_else(String::new, |value_type| format!("{value_type:?}")),
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
pub fn data_id(value: &Value) -> Option<NodeId> {
    match value {
        Value::NodeId(id) => Some(*id),
        _ => None,
    }
}
