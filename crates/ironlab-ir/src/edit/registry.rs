//! The registry of settable properties of each kind of node.

use crate::edit::path::PropertyPath;
use crate::edit::value::ValueType;

/// The kind of a node, which determines the properties it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NodeKind {
    /// The figure, the root node.
    Figure,
    /// An axes.
    Axes,
    /// A line artist.
    Line,
    /// A scatter artist.
    Scatter,
    /// A contour artist.
    Contour,
    /// A quiver artist.
    Quiver,
    /// A surface artist.
    Surface,
    /// A true-colour image artist.
    Image,
    /// A colour-indexed image artist.
    IndexedImage,
    /// A colour-mapped image artist.
    MappedImage,
}

impl NodeKind {
    /// Every kind of node, in declaration order.
    pub const ALL: [NodeKind; 10] = [
        NodeKind::Figure,
        NodeKind::Axes,
        NodeKind::Line,
        NodeKind::Scatter,
        NodeKind::Contour,
        NodeKind::Quiver,
        NodeKind::Surface,
        NodeKind::Image,
        NodeKind::IndexedImage,
        NodeKind::MappedImage,
    ];
}

/// A settable property of a kind of node, as listed by [`properties`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// The path of the property within the node.
    pub path: PropertyPath,
    /// The type of the property's value.
    pub value_type: ValueType,
    /// Whether the property may be absent, so that it accepts
    /// [`Value::Unset`](crate::Value::Unset).
    pub optional: bool,
    /// Whether the property can be reached only while a particular variant of a tagged
    /// value on its path is set (such as `x.limits.min`, which requires manual limits),
    /// or only while an optional value on its path is present (such as
    /// `title.content`).
    pub conditional: bool,
    /// The documentation of the property, taken from the documentation of the IR field.
    pub docs: &'static str,
}

/// Returns every settable property of a kind of node, in the order in which the fields
/// are declared, each followed by the properties below it.
///
/// The list contains every path that [`Edit::Set`](crate::Edit::Set) accepts for some
/// node of the kind, including every intermediate path: a node kind with manual limits
/// on its x axis lists `x`, `x.label`, `x.label.content`, `x.limits`, `x.limits.min` and
/// so on. A path that descends into a variant is listed once even when several variants
/// share the field (as the x data of rectilinear and curvilinear grids do). Identifiers,
/// the schema version, the provenance, the node lists and the data table are not
/// settable and are not listed.
///
/// The property editor uses this list to offer and document properties, and the list is
/// compared with that of the `main` branch so that renamed or removed paths are reported
/// like other breaking changes.
pub fn properties(kind: NodeKind) -> Vec<Property> {
    crate::edit::walk::node_properties(kind)
}
