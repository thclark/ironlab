//! Commands: functions that read a figure and return a literal transaction.
//!
//! An [`Edit`] changes exactly what it names. Behaviour that depends on
//! the state of the figure, such as keeping the limits of linked axes equal, is provided
//! by the commands in this module instead. Each command reads the figure and returns a
//! [`Transaction`] of literal edits, which the caller applies with
//! [`Figure::apply`](crate::Figure::apply), sends to a client that mirrors the figure,
//! or records in an overlay. A client that applies the transaction therefore reaches
//! the same figure without implementing the rule itself.
//!
//! The conveniences [`Figure::set_limits`], [`Figure::link`] and [`Figure::link_all`]
//! apply the transactions of these commands.
//!
//! # Axes that cannot share the limits of their group
//!
//! The axes of a link group may differ in scale, so limits that suit one member may be
//! undrawable on another: a range that reaches zero cannot be shown on a logarithmic
//! axis. Sharing limits is what a link means, so [`link`] and [`set_limits`] set every
//! axes of the group, and a member that cannot show the limits makes the whole
//! transaction fail when it is applied. The figure is then left unchanged, because
//! transactions are atomic, and the user is told rather than left with a group that is
//! linked in name only.

use crate::axes::Limits;
use crate::edit::{Edit, PropertyPath, Transaction, Value};
use crate::error::IrError;
use crate::figure::Figure;
use crate::ids::NodeId;
use crate::link::{AxisLink, Dimension};

/// Returns a transaction that sets the limits of an axes along a dimension, and of
/// every axes linked with it along that dimension.
///
/// The transaction holds one [`Edit::Set`] of `x.limits`, `y.limits`
/// or `z.limits` for each axes of the link group, in the order the axes appear in the
/// figure, and nothing else. Every axes of the group is set, including one that cannot
/// show the limits (a logarithmic axis and limits that are not positive), so that
/// applying the transaction fails and the group is never left half-synchronised.
///
/// # Errors
///
/// Returns [`IrError::UnknownAxes`] when the identifier does not refer to an axes of the
/// figure, and [`IrError::InvalidLimits`] when manual limits are not finite or not
/// strictly increasing.
pub fn set_limits(
    figure: &Figure,
    axes: NodeId,
    dimension: Dimension,
    limits: Limits,
) -> Result<Transaction, IrError> {
    if figure.axes(axes).is_none() {
        return Err(IrError::UnknownAxes(axes));
    }
    if let Limits::Manual { min, max } = limits
        && !(min.is_finite() && max.is_finite() && min < max)
    {
        return Err(IrError::InvalidLimits { min, max });
    }
    let edits = figure
        .linked_axes(axes, dimension)
        .into_iter()
        .map(|id| set_limits_edit(id, dimension, limits))
        .collect();
    Ok(Transaction { edits })
}

/// Returns a transaction that links the limits of the given axes along a dimension.
///
/// The link groups are merged exactly as [`Figure::link`] describes: any existing group
/// for the dimension that shares an axes with the given axes is merged with them, the
/// stored groups for the dimension are normalised, and the limits of every axes in the
/// resulting group take the current limits of the first given axes. The transaction
/// holds one [`Edit::Set`] of the figure's `links`, followed by one set
/// of the limits along the dimension for each axes of the resulting group. Fewer than two
/// distinct axes give an empty transaction.
///
/// Every axes of the group is set, so a member that cannot show the reference limits (a
/// logarithmic axis and limits that are not positive) makes applying the transaction
/// fail, and the figure keeps the links and the limits it had.
///
/// # Errors
///
/// Returns [`IrError::UnknownAxes`] when an identifier does not refer to an axes of the
/// figure.
pub fn link(
    figure: &Figure,
    dimension: Dimension,
    axes: &[NodeId],
) -> Result<Transaction, IrError> {
    if let Some(&unknown) = axes.iter().find(|&&id| figure.axes(id).is_none()) {
        return Err(IrError::UnknownAxes(unknown));
    }
    let mut distinct: Vec<NodeId> = Vec::with_capacity(axes.len());
    for &id in axes {
        if !distinct.contains(&id) {
            distinct.push(id);
        }
    }
    let [reference, _, ..] = distinct[..] else {
        return Ok(Transaction::default());
    };

    let links = figure.link_groups(dimension, &distinct);
    let group: Vec<NodeId> = links
        .iter()
        .find(|link| link.dimension == dimension && link.axes.contains(&reference))
        .map(|link| link.axes.clone())
        .unwrap_or_default();
    let limits = axis_of(figure, reference, dimension).limits;

    let mut edits = vec![Edit::Set {
        node: figure.id,
        path: PropertyPath::of(&["links"]),
        value: Value::Links(links),
    }];
    edits.extend(
        group
            .into_iter()
            .map(|id| set_limits_edit(id, dimension, limits)),
    );
    Ok(Transaction { edits })
}

/// Returns a transaction that removes an axes and every reference to it from the link
/// groups, so that no link is left dangling.
///
/// The transaction holds a set of the figure's `links` without the axes, in which a
/// group left with fewer than two members is dropped, followed by the removal of the
/// axes. When the axes is in no link group, only the removal is returned.
///
/// # Errors
///
/// Returns [`IrError::UnknownAxes`] when the identifier does not refer to an axes of the
/// figure.
pub fn remove_axes(figure: &Figure, axes: NodeId) -> Result<Transaction, IrError> {
    if figure.axes(axes).is_none() {
        return Err(IrError::UnknownAxes(axes));
    }
    let links: Vec<AxisLink> = figure
        .links
        .iter()
        .filter_map(|link| {
            let members: Vec<NodeId> = link.axes.iter().copied().filter(|&id| id != axes).collect();
            (members.len() >= 2).then_some(AxisLink {
                dimension: link.dimension,
                axes: members,
            })
        })
        .collect();
    let mut edits = Vec::new();
    if links != figure.links {
        edits.push(Edit::Set {
            node: figure.id,
            path: PropertyPath::of(&["links"]),
            value: Value::Links(links),
        });
    }
    edits.push(Edit::Remove { node: axes });
    Ok(Transaction { edits })
}

/// Returns the set of the limits of one axes along a dimension.
fn set_limits_edit(axes: NodeId, dimension: Dimension, limits: Limits) -> Edit {
    let field = match dimension {
        Dimension::X => "x",
        Dimension::Y => "y",
        Dimension::Z => "z",
    };
    Edit::Set {
        node: axes,
        path: PropertyPath::of(&[field, "limits"]),
        value: Value::Limits(limits),
    }
}

/// Returns the coordinate axis of an axes of the figure along a dimension.
fn axis_of(figure: &Figure, axes: NodeId, dimension: Dimension) -> &crate::axes::Axis {
    let axes = figure.axes(axes).expect("the axes is in the figure");
    match dimension {
        Dimension::X => &axes.x,
        Dimension::Y => &axes.y,
        Dimension::Z => &axes.z,
    }
}
