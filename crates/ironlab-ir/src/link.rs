//! Links between the limits of axes.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::axes::Limits;
use crate::error::IrError;
use crate::figure::Figure;
use crate::ids::NodeId;

/// A coordinate dimension of an axes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    /// The x axis.
    X,
    /// The y axis.
    Y,
    /// The z axis.
    Z,
}

/// A group of axes whose limits along one dimension are kept equal.
///
/// For each dimension, the groups of a figure are disjoint: an axes belongs to at
/// most one group per dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AxisLink {
    /// The dimension whose limits are linked.
    pub dimension: Dimension,
    /// The identifiers of the linked axes.
    pub axes: Vec<NodeId>,
}

impl Figure {
    /// Links the limits of the given axes along a dimension.
    ///
    /// Any existing group for that dimension that contains one of the given axes is
    /// merged with them into a single group, so groups for a dimension stay disjoint.
    /// The limits of every axes in the resulting group are then set to the current
    /// limits of the first given axes. Fewer than two distinct axes leave the links
    /// unchanged.
    ///
    /// The stored groups for the dimension are normalised at the same time: groups
    /// that overlap (which a hand-edited file may contain) are merged, repeated
    /// identifiers are removed, and groups with fewer than two members are dropped.
    /// Only the limits of the group containing the given axes are synchronised.
    ///
    /// This applies the transaction of [`command::link`](crate::command::link).
    ///
    /// # Errors
    ///
    /// Returns [`IrError::UnknownAxes`] when an identifier does not refer to an axes of
    /// the figure, and [`IrError::Edit`] with
    /// [`EditError::Invalid`](crate::EditError::Invalid) when an axes of the group
    /// cannot show the limits it would take, such as a logarithmic axis and limits that
    /// are not positive: axes that cannot share limits cannot meaningfully be linked, so
    /// the whole change is refused. The figure is left unchanged in every case.
    pub fn link(&mut self, dimension: Dimension, axes: &[NodeId]) -> Result<(), IrError> {
        let transaction = crate::command::link(self, dimension, axes)?;
        self.apply(&transaction)?;
        Ok(())
    }

    /// Links the limits of every axes of the figure along a dimension, synchronising
    /// them to the limits of the first axes.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::Edit`] with
    /// [`EditError::Invalid`](crate::EditError::Invalid) when an axes of the figure
    /// cannot show the limits of the first axes, as [`Figure::link`] describes; the
    /// figure is then left unchanged.
    pub fn link_all(&mut self, dimension: Dimension) -> Result<(), IrError> {
        let ids: Vec<NodeId> = self.axes.iter().map(|axes| axes.id).collect();
        self.link(dimension, &ids)
    }

    /// Returns the axes linked with the given axes along a dimension, including the
    /// given axes itself, in the order the axes appear in the figure.
    ///
    /// An unlinked axes yields only itself, and an unknown identifier yields an empty
    /// list. Groups that overlap are treated as one group, and identifiers in a group
    /// that do not refer to an axes are ignored.
    pub fn linked_axes(&self, axes: NodeId, dimension: Dimension) -> Vec<NodeId> {
        if self.axes(axes).is_none() {
            return Vec::new();
        }
        let groups = self
            .links
            .iter()
            .filter(|link| link.dimension == dimension)
            .map(|link| link.axes.iter().copied());
        let component = connected_components(groups)
            .into_iter()
            .find(|component| component.contains(&axes))
            .unwrap_or_else(|| BTreeSet::from([axes]));
        self.axes
            .iter()
            .map(|a| a.id)
            .filter(|id| component.contains(id))
            .fold(Vec::new(), |mut ids, id| {
                if !ids.contains(&id) {
                    ids.push(id);
                }
                ids
            })
    }

    /// Sets the limits of an axes along a dimension, and of every axes linked with it
    /// along that dimension.
    ///
    /// Every axes of the group takes the limits, so a member that cannot show them
    /// refuses the whole change rather than falling out of step with its group. This
    /// applies the transaction of
    /// [`command::set_limits`](crate::command::set_limits).
    ///
    /// # Errors
    ///
    /// Returns [`IrError::UnknownAxes`] when the identifier does not refer to an axes
    /// of the figure, [`IrError::InvalidLimits`] when manual limits are not finite or
    /// not strictly increasing, and [`IrError::Edit`] with
    /// [`EditError::Invalid`](crate::EditError::Invalid) when any axes of the group
    /// cannot show the limits, such as limits that are not positive on a logarithmic
    /// axis. The figure is left unchanged in every case.
    pub fn set_limits(
        &mut self,
        axes: NodeId,
        dimension: Dimension,
        limits: Limits,
    ) -> Result<(), IrError> {
        let transaction = crate::command::set_limits(self, axes, dimension, limits)?;
        self.apply(&transaction)?;
        Ok(())
    }

    /// Returns the link groups of the figure with the given axes linked along a
    /// dimension: the groups of the other dimensions unchanged, followed by the
    /// normalised, disjoint groups of this one in figure order.
    pub(crate) fn link_groups(&self, dimension: Dimension, axes: &[NodeId]) -> Vec<AxisLink> {
        let groups = self
            .links
            .iter()
            .filter(|link| link.dimension == dimension)
            .map(|link| link.axes.clone())
            .chain(std::iter::once(axes.to_vec()));
        let mut components: Vec<Vec<NodeId>> = connected_components(groups)
            .into_iter()
            .filter(|component| component.len() >= 2)
            .map(|component| self.in_figure_order(&component))
            .collect();
        components.sort_by_cached_key(|component| self.order_key(component[0]));

        let mut links: Vec<AxisLink> = self
            .links
            .iter()
            .filter(|link| link.dimension != dimension)
            .cloned()
            .collect();
        links.extend(
            components
                .into_iter()
                .map(|axes| AxisLink { dimension, axes }),
        );
        links
    }

    /// Orders identifiers as their axes appear in the figure, followed by any
    /// identifiers that are not axes in ascending order.
    fn in_figure_order(&self, ids: &BTreeSet<NodeId>) -> Vec<NodeId> {
        let mut ordered: Vec<NodeId> = ids.iter().copied().collect();
        ordered.sort_by_cached_key(|&id| self.order_key(id));
        ordered
    }

    /// Returns a key that sorts axes in figure order, before identifiers that are not
    /// axes.
    fn order_key(&self, id: NodeId) -> (usize, NodeId) {
        let position = self.axes.iter().position(|a| a.id == id);
        (position.unwrap_or(usize::MAX), id)
    }
}

/// Merges groups that share a member, returning the disjoint unions.
fn connected_components<G>(groups: impl IntoIterator<Item = G>) -> Vec<BTreeSet<NodeId>>
where
    G: IntoIterator<Item = NodeId>,
{
    let mut components: Vec<BTreeSet<NodeId>> = Vec::new();
    for group in groups {
        let mut merged: BTreeSet<NodeId> = group.into_iter().collect();
        components.retain(|component| {
            if component.is_disjoint(&merged) {
                true
            } else {
                merged.extend(component);
                false
            }
        });
        components.push(merged);
    }
    components
}
