//! Linked axis limits: grouping, merging and propagation.

mod common;

use std::collections::BTreeSet;

use common::{limits_of, row_of_axes, single_line_figure};
use ironlab_ir::*;
use proptest::prelude::*;

fn manual(min: f64, max: f64) -> Limits {
    Limits::Manual { min, max }
}

// Why: the core purpose of linking is that panning or zooming one linked subplot moves its
// partners, while subplots outside the group stay where they are.
#[test]
fn set_limits_propagates_to_linked_axes_only() {
    let (mut fig, ids) = row_of_axes(3);
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    let c_before = limits_of(&fig, c, Dimension::X);

    fig.link(Dimension::X, &[a, b]).unwrap();
    fig.set_limits(b, Dimension::X, manual(-5.0, 5.0)).unwrap();

    assert_eq!(limits_of(&fig, a, Dimension::X), manual(-5.0, 5.0));
    assert_eq!(limits_of(&fig, b, Dimension::X), manual(-5.0, 5.0));
    assert_eq!(limits_of(&fig, c, Dimension::X), c_before);
}

// Why: linked axes must agree from the moment they are linked, not only after the next
// change; the first axes named is the reference, as in MATLAB's linkaxes.
#[test]
fn linking_synchronises_the_group_to_the_first_axes() {
    let (mut fig, ids) = row_of_axes(3);
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    let b_limits = limits_of(&fig, b, Dimension::Y);

    fig.link(Dimension::Y, &[b, c, a]).unwrap();

    for id in [a, b, c] {
        assert_eq!(limits_of(&fig, id, Dimension::Y), b_limits);
    }
}

// Why: users link subplots in several calls; linking A-B then B-C must link A, B and C
// transitively (union-find), and the stored groups for a dimension must stay disjoint so
// that the file has a single, unambiguous answer for each axes.
#[test]
fn overlapping_link_calls_merge_into_one_group() {
    let (mut fig, ids) = row_of_axes(3);
    let (a, b, c) = (ids[0], ids[1], ids[2]);

    fig.link(Dimension::X, &[a, b]).unwrap();
    fig.link(Dimension::X, &[b, c]).unwrap();

    assert_eq!(fig.linked_axes(a, Dimension::X), vec![a, b, c]);
    let x_groups: Vec<_> = fig
        .links
        .iter()
        .filter(|l| l.dimension == Dimension::X)
        .collect();
    assert_eq!(x_groups.len(), 1, "groups were not merged: {:?}", fig.links);
    let members: BTreeSet<_> = x_groups[0].axes.iter().copied().collect();
    assert_eq!(members, BTreeSet::from([a, b, c]));
    assert_eq!(
        x_groups[0].axes.len(),
        3,
        "a merged group must not repeat axes"
    );

    fig.set_limits(c, Dimension::X, manual(2.0, 3.0)).unwrap();
    assert_eq!(limits_of(&fig, a, Dimension::X), manual(2.0, 3.0));
}

// Why: separate groups on the same dimension (for example one per row of subplots) must
// stay independent until a link call bridges them, at which point they merge entirely.
#[test]
fn disjoint_groups_stay_independent_until_bridged() {
    let (mut fig, ids) = row_of_axes(4);
    let (a, b, c, d) = (ids[0], ids[1], ids[2], ids[3]);
    fig.link(Dimension::X, &[a, b]).unwrap();
    fig.link(Dimension::X, &[c, d]).unwrap();
    let d_before = limits_of(&fig, d, Dimension::X);

    fig.set_limits(a, Dimension::X, manual(7.0, 8.0)).unwrap();
    assert_eq!(limits_of(&fig, d, Dimension::X), d_before);

    fig.link(Dimension::X, &[b, c]).unwrap();
    assert_eq!(fig.linked_axes(d, Dimension::X), vec![a, b, c, d]);
    fig.set_limits(d, Dimension::X, manual(0.0, 0.5)).unwrap();
    for id in [a, b, c, d] {
        assert_eq!(limits_of(&fig, id, Dimension::X), manual(0.0, 0.5));
    }
}

// Why: sharing x between subplots must not also share y (or z); each dimension is linked
// independently, as with MATLAB's linkaxes(..., 'x').
#[test]
fn links_on_one_dimension_do_not_affect_another() {
    let (mut fig, ids) = row_of_axes(2);
    let (a, b) = (ids[0], ids[1]);
    let b_y = limits_of(&fig, b, Dimension::Y);
    let b_z = limits_of(&fig, b, Dimension::Z);

    fig.link(Dimension::X, &[a, b]).unwrap();
    assert_eq!(limits_of(&fig, b, Dimension::Y), b_y, "linking x changed y");
    fig.set_limits(a, Dimension::Y, manual(-1.0, 1.0)).unwrap();
    fig.set_limits(a, Dimension::Z, manual(-1.0, 1.0)).unwrap();

    assert_eq!(limits_of(&fig, b, Dimension::Y), b_y);
    assert_eq!(limits_of(&fig, b, Dimension::Z), b_z);
    assert_eq!(fig.linked_axes(a, Dimension::Y), vec![a]);
}

// Why: the viewer zooms and pans 3D axes by changing their z limits through `set_limits`,
// so z links between 3D axes must propagate like x and y links, including a reset of the
// group to automatic limits, and must not disturb the linked axes' other dimensions.
#[test]
fn z_limits_propagate_between_linked_three_d_axes() {
    let (mut fig, ids) = row_of_axes(3);
    for axes in &mut fig.axes {
        axes.projection = Projection::ThreeD {
            view3d: View3d::default(),
        };
    }
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    let c_z = limits_of(&fig, c, Dimension::Z);
    let b_x = limits_of(&fig, b, Dimension::X);

    fig.link(Dimension::Z, &[a, b]).unwrap();
    fig.set_limits(a, Dimension::Z, manual(-3.0, 3.0)).unwrap();

    assert_eq!(limits_of(&fig, b, Dimension::Z), manual(-3.0, 3.0));
    assert_eq!(limits_of(&fig, c, Dimension::Z), c_z);
    assert_eq!(limits_of(&fig, b, Dimension::X), b_x);

    fig.set_limits(b, Dimension::Z, Limits::Auto).unwrap();
    assert_eq!(limits_of(&fig, a, Dimension::Z), Limits::Auto);
    assert_eq!(limits_of(&fig, c, Dimension::Z), c_z);
}

// Why: `link_all` backs the "share all y" shortcut, which must link every axes in the
// figure on that dimension and leave the other dimensions alone.
#[test]
fn link_all_links_every_axes_on_one_dimension() {
    let (mut fig, ids) = row_of_axes(4);
    let first_y = limits_of(&fig, ids[0], Dimension::Y);
    let x_before: Vec<_> = ids
        .iter()
        .map(|&id| limits_of(&fig, id, Dimension::X))
        .collect();

    fig.link_all(Dimension::Y);

    for &id in &ids {
        assert_eq!(fig.linked_axes(id, Dimension::Y), ids);
        assert_eq!(limits_of(&fig, id, Dimension::Y), first_y);
    }
    fig.set_limits(ids[2], Dimension::Y, manual(3.0, 4.0))
        .unwrap();
    for &id in &ids {
        assert_eq!(limits_of(&fig, id, Dimension::Y), manual(3.0, 4.0));
        assert_eq!(fig.linked_axes(id, Dimension::X), vec![id]);
    }
    let x_after: Vec<_> = ids
        .iter()
        .map(|&id| limits_of(&fig, id, Dimension::X))
        .collect();
    assert_eq!(x_after, x_before);
}

// Why: `link_all` after a partial link must extend the existing group rather than add an
// overlapping one.
#[test]
fn link_all_absorbs_existing_groups() {
    let (mut fig, ids) = row_of_axes(3);
    fig.link(Dimension::Y, &[ids[1], ids[2]]).unwrap();
    fig.link_all(Dimension::Y);
    let y_groups = fig
        .links
        .iter()
        .filter(|l| l.dimension == Dimension::Y)
        .count();
    assert_eq!(y_groups, 1);
}

// Why: a typo or stale identifier must be reported, and a failed call must not leave a
// half-applied link or changed limits behind.
#[test]
fn linking_an_unknown_or_non_axes_identifier_fails_without_side_effects() {
    let (mut fig, axes, line) = single_line_figure();
    let before = fig.clone();

    let result = fig.link(Dimension::X, &[axes, NodeId(999)]);
    assert!(
        matches!(result, Err(IrError::UnknownAxes(NodeId(999)))),
        "{result:?}"
    );
    assert_eq!(fig, before);

    let result = fig.link(Dimension::X, &[axes, line]);
    assert!(
        matches!(result, Err(IrError::UnknownAxes(id)) if id == line),
        "{result:?}"
    );
    assert_eq!(fig, before);
}

// Why: setting limits on something that is not an axes is a caller error that must be
// reported rather than silently ignored.
#[test]
fn setting_limits_on_an_unknown_axes_fails() {
    let (mut fig, _, line) = single_line_figure();
    let result = fig.set_limits(line, Dimension::X, manual(0.0, 1.0));
    assert!(
        matches!(result, Err(IrError::UnknownAxes(id)) if id == line),
        "{result:?}"
    );
}

// Why: zooming or a caller's arithmetic can produce degenerate, reversed or non-finite
// limits; applying them would spread an undrawable range across a whole link group, so
// they must be refused with the figure left exactly as it was.
#[test]
fn setting_invalid_manual_limits_fails_without_side_effects() {
    let (mut fig, ids) = row_of_axes(2);
    fig.link(Dimension::X, &ids).unwrap();
    let before = fig.clone();
    for (min, max) in [
        (1.0, 1.0),
        (2.0, 1.0),
        (f64::NAN, 1.0),
        (0.0, f64::INFINITY),
    ] {
        let result = fig.set_limits(ids[1], Dimension::X, manual(min, max));
        assert!(
            matches!(result, Err(IrError::InvalidLimits { .. })),
            "[{min}, {max}] gave {result:?}"
        );
        assert_eq!(fig, before, "[{min}, {max}] changed the figure");
    }
}

// Why: a hand-edited or externally written file may store overlapping groups or groups
// with a single member; such groups must behave as their union, and the next link call
// must restore the invariant that stored groups for a dimension are disjoint and have at
// least two members, without disturbing other dimensions.
#[test]
fn linking_normalises_overlapping_and_degenerate_groups_from_a_file() {
    let (mut fig, ids) = row_of_axes(5);
    let (a, b, c, d, e) = (ids[0], ids[1], ids[2], ids[3], ids[4]);
    let y_link = AxisLink {
        dimension: Dimension::Y,
        axes: vec![a, e],
    };
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
        y_link.clone(),
    ];
    let mut fig = Figure::from_json(&fig.to_json()).unwrap();
    assert_eq!(fig.linked_axes(c, Dimension::X), vec![a, b, c]);
    assert_eq!(fig.linked_axes(d, Dimension::X), vec![d]);

    fig.link(Dimension::X, &[d, e]).unwrap();

    let x_groups: BTreeSet<BTreeSet<NodeId>> = fig
        .links
        .iter()
        .filter(|l| l.dimension == Dimension::X)
        .map(|l| {
            let members: BTreeSet<NodeId> = l.axes.iter().copied().collect();
            assert_eq!(members.len(), l.axes.len(), "repeated axes in {l:?}");
            members
        })
        .collect();
    assert_eq!(
        x_groups,
        BTreeSet::from([BTreeSet::from([a, b, c]), BTreeSet::from([d, e])])
    );
    assert_eq!(
        fig.links
            .iter()
            .filter(|l| l.dimension == Dimension::X)
            .count(),
        2
    );
    assert!(fig.links.contains(&y_link), "{:?}", fig.links);
    assert_eq!(
        limits_of(&fig, e, Dimension::X),
        limits_of(&fig, d, Dimension::X)
    );

    let d_before = limits_of(&fig, d, Dimension::X);
    fig.set_limits(a, Dimension::X, manual(-4.0, 4.0)).unwrap();
    for id in [a, b, c] {
        assert_eq!(limits_of(&fig, id, Dimension::X), manual(-4.0, 4.0));
    }
    assert_eq!(limits_of(&fig, d, Dimension::X), d_before);
}

// Why: callers (the viewer, which applies a change to every axes in a group) rely on the
// group containing the axes itself and on a deterministic order (figure order), whatever
// order the link was made in.
#[test]
fn linked_axes_includes_itself_in_figure_order() {
    let (mut fig, ids) = row_of_axes(3);
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    assert_eq!(fig.linked_axes(b, Dimension::X), vec![b]);
    fig.link(Dimension::X, &[c, a]).unwrap();
    assert_eq!(fig.linked_axes(c, Dimension::X), vec![a, c]);
    assert_eq!(fig.linked_axes(b, Dimension::X), vec![b]);
    assert_eq!(
        fig.linked_axes(NodeId(999), Dimension::X),
        Vec::<NodeId>::new()
    );
}

// Why: linking a single axes, or the same axes twice, has no partner to link to and must
// not create a degenerate group in the file.
#[test]
fn linking_fewer_than_two_distinct_axes_changes_nothing() {
    let (mut fig, ids) = row_of_axes(2);
    fig.link(Dimension::X, &[ids[0]]).unwrap();
    fig.link(Dimension::X, &[ids[1], ids[1]]).unwrap();
    fig.link(Dimension::X, &[]).unwrap();
    assert!(fig.links.is_empty(), "{:?}", fig.links);

    // The "share all x" shortcut on a figure with a single axes or none has no partner.
    let (mut single, _) = row_of_axes(1);
    single.link_all(Dimension::X);
    assert!(single.links.is_empty(), "{:?}", single.links);
    let mut empty = Figure::new();
    empty.link_all(Dimension::X);
    assert!(empty.links.is_empty(), "{:?}", empty.links);
}

// Why: links are part of the saved figure, so reopening a file must restore linked
// behaviour, not just the list of identifiers.
#[test]
fn links_keep_working_after_a_json_round_trip() {
    let (mut fig, ids) = row_of_axes(3);
    fig.link(Dimension::X, &[ids[0], ids[2]]).unwrap();
    let mut restored = Figure::from_json(&fig.to_json()).unwrap();
    restored
        .set_limits(ids[0], Dimension::X, manual(-2.0, 2.0))
        .unwrap();
    assert_eq!(
        limits_of(&restored, ids[2], Dimension::X),
        manual(-2.0, 2.0)
    );
    assert_ne!(
        limits_of(&restored, ids[1], Dimension::X),
        manual(-2.0, 2.0)
    );
}

/// Reference connected components of the "linked with" relation, computed naively.
fn reference_components(n: usize, calls: &[Vec<usize>]) -> Vec<BTreeSet<usize>> {
    let mut components: Vec<BTreeSet<usize>> = (0..n).map(|i| BTreeSet::from([i])).collect();
    for call in calls {
        for pair in call.windows(2) {
            let (i, j) = (pair[0], pair[1]);
            let ci = components.iter().position(|c| c.contains(&i)).unwrap();
            let cj = components.iter().position(|c| c.contains(&j)).unwrap();
            if ci != cj {
                let moved = components.remove(ci.max(cj));
                components[ci.min(cj)].extend(moved);
            }
        }
    }
    components
}

proptest! {
    // Why: arbitrary sequences of link calls (the "arbitrarily linked subplots" use case)
    // must always yield exactly the transitive closure of the calls, with disjoint stored
    // groups whose members agree immediately after linking, and with limits propagating
    // across each whole group and nowhere else.
    #[test]
    fn any_sequence_of_links_yields_the_transitive_groups(
        calls in prop::collection::vec(prop::collection::vec(0usize..6, 0..4), 0..8)
    ) {
        let n = 6;
        let (mut fig, ids) = row_of_axes(n as u32);
        for call in &calls {
            let axes: Vec<NodeId> = call.iter().map(|&i| ids[i]).collect();
            fig.link(Dimension::X, &axes).unwrap();
        }

        let expected = reference_components(n, &calls);
        for (i, &id) in ids.iter().enumerate() {
            let component = expected.iter().find(|c| c.contains(&i)).unwrap();
            let expected_ids: Vec<NodeId> = component.iter().map(|&k| ids[k]).collect();
            prop_assert_eq!(fig.linked_axes(id, Dimension::X), expected_ids);
        }

        let mut seen = BTreeSet::new();
        for link in fig.links.iter().filter(|l| l.dimension == Dimension::X) {
            prop_assert!(link.axes.len() >= 2);
            for id in &link.axes {
                prop_assert!(seen.insert(*id), "axes {:?} is in two groups", id);
            }
        }

        let before: Vec<_> = ids.iter().map(|&id| limits_of(&fig, id, Dimension::X)).collect();
        for component in &expected {
            let first = before[*component.first().unwrap()];
            if component.len() == 1 {
                let k = *component.first().unwrap() as f64;
                prop_assert_eq!(first, manual(k, k + 1.0), "an unlinked axes changed");
            }
            for &k in component {
                prop_assert_eq!(before[k], first, "linked axes disagree after linking");
            }
        }
        fig.set_limits(ids[0], Dimension::X, manual(-9.0, 9.0)).unwrap();
        let group_of_first = expected.iter().find(|c| c.contains(&0)).unwrap();
        for (i, &id) in ids.iter().enumerate() {
            let now = limits_of(&fig, id, Dimension::X);
            if group_of_first.contains(&i) {
                prop_assert_eq!(now, manual(-9.0, 9.0));
            } else {
                prop_assert_eq!(now, before[i]);
            }
        }
    }
}
