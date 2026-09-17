//! Linking axes limits from the facade.

use ironlab::ir::{AxisLink, Dimension, Limits};
use ironlab::prelude::*;

/// Returns the link groups of the figure along one dimension, each sorted by id.
fn groups(fig: &Figure, dim: Dimension) -> Vec<Vec<NodeId>> {
    fig.ir()
        .links
        .iter()
        .filter(|link| link.dimension == dim)
        .map(|link| {
            let mut ids = link.axes.clone();
            ids.sort();
            ids
        })
        .collect()
}

fn sorted(mut ids: Vec<NodeId>) -> Vec<NodeId> {
    ids.sort();
    ids
}

/// A 2 by 2 figure with an axes in every tile, returning the axes ids in tile order.
fn four_axes() -> (Figure, [NodeId; 4]) {
    let mut fig = Figure::new().tiles(2, 2);
    let ids = [
        fig.axes(0, 0).id(),
        fig.axes(0, 1).id(),
        fig.axes(1, 0).id(),
        fig.axes(1, 1).id(),
    ];
    (fig, ids)
}

// WHY: link_all_x is the gallery's "shared x" shortcut; it must link every axes in a
// single X group and must not link Y, or panning vertically would move every subplot.
#[test]
fn link_all_x_links_every_axes_along_x_only() {
    let (mut fig, ids) = four_axes();
    fig.link_all_x().unwrap();
    assert_eq!(groups(&fig, Dimension::X), vec![sorted(ids.to_vec())]);
    assert!(groups(&fig, Dimension::Y).is_empty());
    assert!(groups(&fig, Dimension::Z).is_empty());
}

// WHY: see link_all_x_links_every_axes_along_x_only, for the y shortcut.
#[test]
fn link_all_y_links_every_axes_along_y_only() {
    let (mut fig, ids) = four_axes();
    fig.link_all_y().unwrap();
    assert_eq!(groups(&fig, Dimension::Y), vec![sorted(ids.to_vec())]);
    assert!(groups(&fig, Dimension::X).is_empty());
}

// WHY: both shortcuts return the figure so that they can be chained, and linking both
// dimensions must produce independent groups.
#[test]
fn link_shortcuts_chain() {
    let (mut fig, ids) = four_axes();
    fig.link_all_x().unwrap().link_all_y().unwrap();
    assert_eq!(groups(&fig, Dimension::X), vec![sorted(ids.to_vec())]);
    assert_eq!(groups(&fig, Dimension::Y), vec![sorted(ids.to_vec())]);
}

// WHY: arbitrary linking by id (a row sharing y, a column sharing x) is the core of the
// linked-subplots feature; the facade must delegate to the IR's union-find so that
// overlapping calls merge into one group.
#[test]
fn link_by_ids_creates_and_merges_groups() {
    let (mut fig, [a, b, c, d]) = four_axes();
    fig.link(Dim::Y, &[a, b]).unwrap();
    fig.link(Dim::X, &[a, c]).unwrap();
    assert_eq!(groups(&fig, Dimension::Y), vec![sorted(vec![a, b])]);
    assert_eq!(groups(&fig, Dimension::X), vec![sorted(vec![a, c])]);

    fig.link(Dim::X, &[c, d]).unwrap();
    assert_eq!(groups(&fig, Dimension::X), vec![sorted(vec![a, c, d])]);
}

// WHY: subplots must start in sync when they are linked, taking the limits of the
// first axes named; otherwise a gallery figure that fixes the limits of one panel and
// then links it shows its partners with different ranges until the user pans.
#[test]
fn link_synchronises_the_group_to_the_first_axes_along_that_dimension_only() {
    let (mut fig, [a, b, c, _]) = four_axes();
    fig.axes(1, 0).xlim(-5.0, 5.0).ylim(0.0, 1.0);

    fig.link(Dim::X, &[c, a, b]).unwrap();

    let manual = Limits::Manual {
        min: -5.0,
        max: 5.0,
    };
    for id in [a, b, c] {
        assert_eq!(fig.ir().axes(id).unwrap().x.limits, manual);
    }
    assert_eq!(fig.ir().axes(a).unwrap().y.limits, Limits::Auto);
}

// WHY: linking an identifier that is not an axes is a user error that must be returned
// (the builder's documented contract) and must leave the links untouched.
#[test]
fn link_with_an_unknown_id_is_an_error_and_changes_nothing() {
    let (mut fig, [a, b, ..]) = four_axes();
    fig.link(Dim::X, &[a, b]).unwrap();
    let before: Vec<AxisLink> = fig.ir().links.clone();

    let result = fig.link(Dim::X, &[a, NodeId(9_999)]);
    assert!(matches!(result, Err(Error::Ir(_))), "{result:?}");
    assert_eq!(fig.ir().links, before);
}

// WHY: link returns the figure on success so that several links can be chained with `?`.
#[test]
fn link_chains_on_success() -> Result<(), Error> {
    let (mut fig, [a, b, c, d]) = four_axes();
    fig.link(Dim::X, &[a, b])?.link(Dim::Y, &[c, d])?;
    assert_eq!(fig.ir().links.len(), 2);
    Ok(())
}
