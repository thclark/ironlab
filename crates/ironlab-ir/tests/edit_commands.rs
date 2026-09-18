//! Commands: `set_limits`, `link` and `remove_axes` return literal transactions that
//! implement figure-wide rules, and the `Figure` conveniences apply them.

mod common;

use std::collections::BTreeSet;

use common::edits::{applied, axes_ids, manual, path, set};
use common::{limits_of, row_of_axes, single_line_figure};
use ironlab_ir::command;
use ironlab_ir::*;

/// The links of a figure along one dimension, as sets of members.
fn groups(fig: &Figure, dimension: Dimension) -> BTreeSet<BTreeSet<NodeId>> {
    fig.links
        .iter()
        .filter(|link| link.dimension == dimension)
        .map(|link| link.axes.iter().copied().collect())
        .collect()
}

// Why: panning a linked subplot must move its partners, and a client that mirrors the
// figure must see that as plain sets it can apply; the command returns exactly one set of
// the limits for each member of the group, in figure order, and nothing for other axes.
#[test]
fn set_limits_sets_every_axes_of_the_link_group_and_nothing_else() {
    let (mut fig, ids) = row_of_axes(4);
    fig.links = vec![AxisLink {
        dimension: Dimension::Z,
        axes: vec![ids[2], ids[0], ids[1]],
    }];
    let limits = manual(-5.0, 5.0);
    let transaction = command::set_limits(&fig, ids[1], Dimension::Z, limits).unwrap();
    assert_eq!(
        transaction.edits,
        [ids[0], ids[1], ids[2]].map(|id| set(id, "z.limits", Value::Limits(limits)))
    );

    let (edited, _) = applied(&fig, &transaction).unwrap();
    for &id in &ids[..3] {
        assert_eq!(limits_of(&edited, id, Dimension::Z), limits);
    }
    assert_eq!(
        limits_of(&edited, ids[3], Dimension::Z),
        limits_of(&fig, ids[3], Dimension::Z)
    );
}

// Why: an axes outside any group changes alone.
#[test]
fn set_limits_on_an_unlinked_axes_sets_only_that_axes() {
    let (fig, ids) = row_of_axes(2);
    let transaction = command::set_limits(&fig, ids[1], Dimension::X, Limits::Auto).unwrap();
    assert_eq!(
        transaction.edits,
        [set(ids[1], "x.limits", Value::Limits(Limits::Auto))]
    );
}

// Why: a caller error (an identifier that is not an axes, or limits that are reversed,
// empty or not finite) must be reported before any transaction exists, rather than
// produce a transaction that spreads an undrawable range across a group.
#[test]
fn set_limits_refuses_an_unknown_axes_and_invalid_manual_limits() {
    let (fig, axes, line) = single_line_figure();
    for id in [line, NodeId(999), fig.id] {
        let result = command::set_limits(&fig, id, Dimension::X, manual(0.0, 1.0));
        assert!(
            matches!(result, Err(IrError::UnknownAxes(u)) if u == id),
            "{result:?}"
        );
    }
    for (min, max) in [
        (1.0, 1.0),
        (2.0, 1.0),
        (f64::NAN, 1.0),
        (0.0, f64::INFINITY),
    ] {
        let result = command::set_limits(&fig, axes, Dimension::X, manual(min, max));
        assert!(
            matches!(result, Err(IrError::InvalidLimits { .. })),
            "[{min}, {max}]: {result:?}"
        );
    }
}

// Why: the command must merge groups exactly as linking always has (transitively, with
// the first given axes as the reference for the limits), leaving other dimensions alone.
#[test]
fn link_merges_overlapping_groups_and_synchronises_to_the_first_given_axes() {
    let (mut fig, ids) = row_of_axes(4);
    let (a, b, c, d) = (ids[0], ids[1], ids[2], ids[3]);
    let y_link = AxisLink {
        dimension: Dimension::Y,
        axes: vec![a, d],
    };
    fig.links = vec![
        AxisLink {
            dimension: Dimension::X,
            axes: vec![a, b],
        },
        y_link.clone(),
    ];
    let reference = limits_of(&fig, c, Dimension::X);

    let transaction = command::link(&fig, Dimension::X, &[c, b]).unwrap();
    let (edited, _) = applied(&fig, &transaction).unwrap();

    assert_eq!(
        groups(&edited, Dimension::X),
        BTreeSet::from([BTreeSet::from([a, b, c])])
    );
    let x_link = edited
        .links
        .iter()
        .find(|link| link.dimension == Dimension::X)
        .unwrap();
    assert_eq!(x_link.axes, [a, b, c], "groups are stored in figure order");
    assert!(edited.links.contains(&y_link));
    for id in [a, b, c] {
        assert_eq!(limits_of(&edited, id, Dimension::X), reference);
    }
    assert_eq!(
        limits_of(&edited, d, Dimension::X),
        limits_of(&fig, d, Dimension::X)
    );
}

// Why: a file may hold overlapping or single-member groups; linking must normalise the
// stored groups for the dimension as `Figure::link` always has.
#[test]
fn link_normalises_overlapping_and_degenerate_groups() {
    let (mut fig, ids) = row_of_axes(5);
    let (a, b, c, d, e) = (ids[0], ids[1], ids[2], ids[3], ids[4]);
    fig.links = vec![
        AxisLink {
            dimension: Dimension::X,
            axes: vec![a, b],
        },
        AxisLink {
            dimension: Dimension::X,
            axes: vec![b, c, b],
        },
        AxisLink {
            dimension: Dimension::X,
            axes: vec![d],
        },
    ];
    let (edited, _) = applied(&fig, &command::link(&fig, Dimension::X, &[d, e]).unwrap()).unwrap();
    assert_eq!(
        groups(&edited, Dimension::X),
        BTreeSet::from([BTreeSet::from([a, b, c]), BTreeSet::from([d, e])])
    );
    for link in &edited.links {
        let members: BTreeSet<_> = link.axes.iter().collect();
        assert_eq!(members.len(), link.axes.len(), "{link:?} repeats an axes");
    }
}

// Why: a link transaction must be expressible with literal edits only: one set of the
// figure's links and one set of the limits of each member of the resulting group.
#[test]
fn a_link_transaction_sets_the_links_and_the_limits_of_the_group_only() {
    let (fig, ids) = row_of_axes(4);
    let transaction = command::link(&fig, Dimension::Y, &[ids[2], ids[0]]).unwrap();
    let [first, rest @ ..] = transaction.edits.as_slice() else {
        panic!("the transaction is empty");
    };
    assert!(
        matches!(first, Edit::Set { node, path: p, value: Value::Links(_) } if *node == fig.id && *p == path("links")),
        "{first:?}"
    );
    let limited: BTreeSet<NodeId> = rest
        .iter()
        .map(|edit| match edit {
            Edit::Set {
                node,
                path: p,
                value: Value::Limits(_),
            } if *p == path("y.limits") => *node,
            other => panic!("unexpected edit {other:?}"),
        })
        .collect();
    assert_eq!(limited, BTreeSet::from([ids[0], ids[2]]));
}

// Why: the reason for commands is that a mirror applies the transaction without knowing
// the linking rule; a transaction sent over the wire and applied to a copy must give the
// same figure as applying it locally.
#[test]
fn a_mirror_that_applies_command_transactions_reaches_the_same_figure() {
    let (fig, ids) = row_of_axes(3);
    let mut local = fig.clone();
    let mut mirror = fig.clone();
    let link = command::link(&local, Dimension::X, &[ids[0], ids[2]]).unwrap();
    local.apply(&link).unwrap();
    mirror
        .apply(&Transaction::from_protobuf(&link.to_protobuf()).unwrap())
        .unwrap();
    let limits = command::set_limits(&local, ids[2], Dimension::X, manual(-1.0, 1.0)).unwrap();
    local.apply(&limits).unwrap();
    mirror
        .apply(&Transaction::from_json(&limits.to_json()).unwrap())
        .unwrap();
    assert_eq!(mirror, local);
    assert_eq!(limits_of(&mirror, ids[0], Dimension::X), manual(-1.0, 1.0));
}

// Why: linking fewer than two distinct axes has no partner, so it must produce no edits;
// an unknown identifier is a caller error.
#[test]
fn link_with_fewer_than_two_distinct_axes_is_empty_and_unknown_axes_fail() {
    let (fig, ids) = row_of_axes(2);
    for axes in [&[][..], &[ids[0]][..], &[ids[1], ids[1]][..]] {
        assert!(
            command::link(&fig, Dimension::X, axes)
                .unwrap()
                .edits
                .is_empty()
        );
    }
    let result = command::link(&fig, Dimension::X, &[ids[0], NodeId(999)]);
    assert!(
        matches!(result, Err(IrError::UnknownAxes(NodeId(999)))),
        "{result:?}"
    );
}

// Why: removing a subplot must not leave a link to it, which would make the figure invalid
// and the removal impossible; the command removes the axes from every group, dropping
// groups left with a single member, and the rest of each group stays linked.
#[test]
fn remove_axes_also_removes_the_axes_from_its_link_groups() {
    let (mut fig, ids) = row_of_axes(3);
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    fig.links = vec![
        AxisLink {
            dimension: Dimension::X,
            axes: vec![a, b, c],
        },
        AxisLink {
            dimension: Dimension::Y,
            axes: vec![a, b],
        },
    ];
    let transaction = command::remove_axes(&fig, b).unwrap();
    assert_eq!(transaction.edits.last(), Some(&Edit::Remove { node: b }));

    let (edited, inverse) = applied(&fig, &transaction).unwrap();
    assert_eq!(axes_ids(&edited), [a, c]);
    assert_eq!(
        groups(&edited, Dimension::X),
        BTreeSet::from([BTreeSet::from([a, c])])
    );
    assert!(groups(&edited, Dimension::Y).is_empty());
    assert!(edited.validate().is_valid(), "{:?}", edited.validate());

    let (restored, _) = applied(&edited, &inverse).unwrap();
    assert_eq!(restored, fig);
}

// Why: an axes in no group needs only the literal removal; an unknown one is an error.
#[test]
fn remove_axes_of_an_unlinked_or_unknown_axes() {
    let (fig, ids) = row_of_axes(2);
    assert_eq!(
        command::remove_axes(&fig, ids[0]).unwrap().edits,
        [Edit::Remove { node: ids[0] }]
    );
    let result = command::remove_axes(&fig, NodeId(999));
    assert!(
        matches!(result, Err(IrError::UnknownAxes(NodeId(999)))),
        "{result:?}"
    );
}

// ---------------------------------------------------------------------------------
// The conveniences of `Figure`
// ---------------------------------------------------------------------------------

// Why: `Figure::set_limits`, `Figure::link` and `Figure::link_all` remain as conveniences
// for programs, and must behave exactly as applying the commands does, so that a program
// and a mirror of its transactions never disagree.
#[test]
fn the_figure_conveniences_apply_the_commands() {
    let (fig, ids) = row_of_axes(3);

    let mut convenient = fig.clone();
    convenient.link(Dimension::X, &[ids[1], ids[2]]).unwrap();
    let (expected, _) = applied(
        &fig,
        &command::link(&fig, Dimension::X, &[ids[1], ids[2]]).unwrap(),
    )
    .unwrap();
    assert_eq!(convenient, expected);

    let limits = manual(4.0, 5.0);
    let transaction = command::set_limits(&convenient, ids[2], Dimension::X, limits).unwrap();
    let (expected, _) = applied(&convenient, &transaction).unwrap();
    convenient.set_limits(ids[2], Dimension::X, limits).unwrap();
    assert_eq!(convenient, expected);

    let mut all = fig.clone();
    all.link_all(Dimension::Y).unwrap();
    let (expected, _) = applied(&fig, &command::link(&fig, Dimension::Y, &ids).unwrap()).unwrap();
    assert_eq!(all, expected);
}

// Why: because the conveniences apply transactions, they are validated like any other
// transaction: non-positive manual limits on a logarithmic axis are refused and leave the
// figure unchanged, where they were previously written and only reported by validation.
#[test]
fn figure_set_limits_refuses_limits_that_validation_rejects() {
    let (mut fig, ids) = row_of_axes(2);
    fig.axes[1].x.scale = Scale::Log; // limits [1, 2], which are valid on a log axis
    let before = fig.clone();
    let result = fig.set_limits(ids[1], Dimension::X, manual(-1.0, 1.0));
    assert!(
        matches!(&result, Err(IrError::Edit(EditError::Invalid(issues))) if issues.iter().any(|i| i.kind == IssueKind::InvalidLimits)),
        "{result:?}"
    );
    assert_eq!(fig, before);
}

// ---------------------------------------------------------------------------------
// Axes that cannot share the limits of their group
// ---------------------------------------------------------------------------------

/// A figure of two axes waiting to be linked: a linear one whose limits reach below zero,
/// and a logarithmic one whose limits are positive. Returns the figure and the two axes.
fn linear_and_log() -> (Figure, NodeId, NodeId) {
    let (mut fig, ids) = row_of_axes(2);
    fig.axes[0].x.limits = manual(-1.0, 1.0);
    fig.axes[1].x.scale = Scale::Log;
    fig.axes[1].x.limits = manual(1.0, 2.0);
    (fig, ids[0], ids[1])
}

/// Asserts that an error refuses the limits because an axis cannot show them.
fn assert_refuses_limits<T: std::fmt::Debug>(result: &Result<T, IrError>) {
    assert!(
        matches!(result, Err(IrError::Edit(EditError::Invalid(issues))) if issues.iter().any(|i| i.kind == IssueKind::InvalidLimits)),
        "{result:?}"
    );
}

// Why: the axes of a link group may differ in scale, so limits that suit one member can be
// undrawable on another: a range that reaches zero cannot be shown on a logarithmic axis.
// Sharing limits is what linking means, so a group that cannot share them is a mistake the
// user must be told about rather than a silent half-link. The command therefore sets every
// member of the group, applying the transaction fails, and the figure keeps the links and
// the limits it had, because transactions are atomic.
#[test]
fn linking_axes_that_cannot_share_limits_is_refused_and_changes_nothing() {
    let (fig, linear, log) = linear_and_log();

    let transaction = command::link(&fig, Dimension::X, &[linear, log]).unwrap();
    let limited: BTreeSet<NodeId> = transaction
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::Set {
                node,
                value: Value::Limits(_),
                ..
            } => Some(*node),
            _ => None,
        })
        .collect();
    assert_eq!(
        limited,
        BTreeSet::from([linear, log]),
        "every member of the group is set, so that the logarithmic axes refuses the range"
    );
    assert_refuses_limits(&applied(&fig, &transaction).map_err(IrError::Edit));

    // The conveniences refuse in the same way, and leave the figure exactly as it was.
    let mut refused = fig.clone();
    assert_refuses_limits(&refused.link(Dimension::X, &[linear, log]));
    assert_eq!(refused, fig, "an atomic transaction changes nothing");

    let mut refused_all = fig.clone();
    assert_refuses_limits(&refused_all.link_all(Dimension::X));
    assert_eq!(refused_all, fig, "an atomic transaction changes nothing");
}

// Why: the reference limits are those of the first axes given, so the same pair links
// cleanly the other way round, when the reference range is positive. Linking still does
// what it is for: after it, every member shows the reference limits.
#[test]
fn linking_axes_that_can_share_limits_synchronises_them() {
    let (fig, linear, log) = linear_and_log();

    let mut linked = fig.clone();
    linked.link(Dimension::X, &[log, linear]).unwrap();

    for id in [linear, log] {
        assert_eq!(limits_of(&linked, id, Dimension::X), manual(1.0, 2.0));
    }
    assert_eq!(
        groups(&linked, Dimension::X),
        BTreeSet::from([BTreeSet::from([linear, log])])
    );
    assert!(linked.validate().is_valid(), "{:?}", linked.validate());
}

// Why: a user who zooms one axes of a group asks for that range across the whole group,
// because that is what a link promises; a partner that cannot show it must refuse the
// change rather than fall out of step with its group behind the user's back. The refusal
// does not depend on which member of the group was asked.
#[test]
fn set_limits_is_refused_when_a_linked_partner_cannot_show_the_limits() {
    let (mut fig, linear, log) = linear_and_log();
    fig.links = vec![AxisLink {
        dimension: Dimension::X,
        axes: vec![linear, log],
    }];

    let limits = manual(-5.0, 5.0);
    let transaction = command::set_limits(&fig, linear, Dimension::X, limits).unwrap();
    assert_eq!(
        transaction.edits,
        [linear, log].map(|id| set(id, "x.limits", Value::Limits(limits))),
        "every member of the group is set, in figure order"
    );

    for asked in [linear, log] {
        let mut refused = fig.clone();
        assert_refuses_limits(&refused.set_limits(asked, Dimension::X, limits));
        assert_eq!(refused, fig, "an atomic transaction changes nothing");
    }
}
