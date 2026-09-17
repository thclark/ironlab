//! What the viewer has to tell the user about a figure it cannot draw as asked.
//!
//! Three quite different things go wrong, and the user needs to tell them apart before
//! they can act:
//!
//! - The scene compiler reports a [`SceneWarning`] while drawing the figure: mathematics
//!   it cannot typeset, data it cannot place on a logarithmic axis. Nothing the user did
//!   caused it, and it returns every time the figure is drawn.
//! - Composing the overlay [`discards`](Origin::Discarded) an entry, because the change
//!   it holds can no longer be applied to the source. The change is gone.
//! - The IR [`refuses`](Origin::Refused) a change before it is recorded. The figure is
//!   left exactly as it was.
//!
//! A [`Problem`] carries which of the three it is, the node and property it concerns
//! where those are known, and the reason in full, so that the problems list can name each
//! one rather than showing a count. This module is pure logic with no GPU or windowing
//! dependency, so everything the list says can be unit tested.

use ironlab_ir::{Figure, NodeId, PropertyPath};
use ironlab_scene::SceneWarning;

use crate::inspector::{kind_name, tree_rows};

/// How a problem arose, which decides how the user should read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The scene compiler reported it while the figure was drawn.
    Scene,
    /// A change of the user's could not be shown, so it was discarded.
    Discarded,
    /// A change of the user's was refused, so the figure is unchanged.
    Refused,
}

impl Origin {
    /// The sentence that says how the problem arose, shown under it in the problems list.
    #[must_use]
    pub fn explanation(self) -> &'static str {
        match self {
            Origin::Scene => "Reported while the figure was drawn.",
            Origin::Discarded => "Your change could not be shown, so it was discarded.",
            Origin::Refused => "Your change was refused, so the figure is unchanged.",
        }
    }
}

/// One thing the viewer has to tell the user about the figure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// How the problem arose.
    pub origin: Origin,
    /// The node it concerns, or `None` when it concerns the figure as a whole.
    pub node: Option<NodeId>,
    /// The property it concerns, or `None` when it concerns no single property.
    pub path: Option<PropertyPath>,
    /// The reason, in full, as the compiler or the IR gave it.
    pub detail: String,
}

impl Problem {
    /// The problem that a warning of the scene compiler raises.
    #[must_use]
    pub fn from_scene(warning: &SceneWarning) -> Self {
        Self {
            origin: Origin::Scene,
            node: warning.node,
            path: None,
            detail: warning.message.clone(),
        }
    }

    /// The heading of the problem in the list: the object it concerns and, where one is
    /// known, the property of it.
    ///
    /// The object is named as the object tree names it, so that the user can find it
    /// there; a node that is no longer in the figure is named by its identifier, because
    /// there is nothing else left to call it.
    #[must_use]
    pub fn subject(&self, figure: &Figure) -> String {
        let object = match self.node {
            None => "The figure".to_owned(),
            Some(node) => node_label(figure, node),
        };
        match &self.path {
            Some(path) => format!("{object} — {path}"),
            None => object,
        }
    }
}

/// The name of a node, as the object tree writes it, qualified by its kind so that
/// "Speed" is recognisable as an axes.
fn node_label(figure: &Figure, node: NodeId) -> String {
    let Some(row) = tree_rows(figure).into_iter().find(|row| row.node == node) else {
        return format!("Node {}", node.0);
    };
    let kind = kind_name(row.kind);
    if row.label == kind {
        kind.to_owned()
    } else {
        format!("{kind} “{}”", row.label)
    }
}

/// The label of the problems indicator, or `None` when there is nothing to report.
///
/// The indicator is silent when there are no problems, so that a figure that draws
/// cleanly says nothing at all.
#[must_use]
pub fn indicator_label(problems: &[Problem]) -> Option<String> {
    let count = problems.len();
    match count {
        0 => None,
        1 => Some("⚠ 1 problem".to_owned()),
        _ => Some(format!("⚠ {count} problems")),
    }
}
