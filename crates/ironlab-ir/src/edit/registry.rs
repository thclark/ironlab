//! The registry of settable properties of each kind of node.

use crate::edit::path::PropertyPath;
use crate::edit::value::ValueType;
use crate::edit::walk::{ENTRY, entry_templates};
use crate::figure::Figure;
use crate::ids::NodeId;
use crate::nodes::NodeKind;

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
/// so on. The entries of a palette (`kind.indexed.colors.0` and so on) are not listed,
/// because which entries there are depends on the node rather than on its kind; see
/// [`properties_of`]. A path that descends into a variant is listed once even when several variants
/// share the field (as the x data of rectilinear and curvilinear grids do). Identifiers,
/// the schema version, the provenance, the node lists and the values of a data array are
/// not settable and are not listed.
///
/// The property editor uses this list to offer and document properties, and the list is
/// compared with that of the `main` branch so that renamed or removed paths are reported
/// like other breaking changes.
pub fn properties(kind: NodeKind) -> Vec<Property> {
    crate::edit::walk::node_properties(kind)
}

/// Returns every settable property of one node as it stands: the registry of its kind
/// ([`properties`]), followed by one property per entry of each palette the node holds, in
/// index order, each followed by the properties below it.
///
/// Every entry property is `conditional`, because which entries there are depends on the
/// node. A palette whose variant is not set (the listed colours of a named colormap) has no
/// entries. Returns an empty list when the node is not in the figure.
pub fn properties_of(figure: &Figure, node: NodeId) -> Vec<Property> {
    let Some(kind) = figure.node_kind(node) else {
        return Vec::new();
    };
    let mut listed = properties(kind);
    let templates = entry_templates(kind);
    // The templates of one list share the segments before the entry, and are declared
    // together, entry first; each list is expanded entry by entry.
    let list_of = |template: &Property| -> Vec<String> {
        let segments = template.path.segments();
        let at = segments
            .iter()
            .position(|segment| segment == ENTRY)
            .expect("a template names an entry");
        segments[..at].to_vec()
    };
    let mut start = 0;
    while start < templates.len() {
        let list = list_of(&templates[start]);
        let end = templates[start..]
            .iter()
            .position(|template| list_of(template) != list)
            .map_or(templates.len(), |n| start + n);
        let group = &templates[start..end];
        for index in 0.. {
            let index = index.to_string();
            let entries: Vec<Property> = group
                .iter()
                .map(|template| Property {
                    path: PropertyPath::new(template.path.segments().iter().map(|segment| {
                        if segment == ENTRY {
                            index.clone()
                        } else {
                            segment.clone()
                        }
                    }))
                    .expect("an index is a valid segment"),
                    ..template.clone()
                })
                .collect();
            // The entry exists while the first property of the group can be read; the
            // first is the entry itself or, for a category, its colour.
            if figure.get(node, &entries[0].path).is_err() {
                break;
            }
            listed.extend(entries);
        }
        start = end;
    }
    listed
}
