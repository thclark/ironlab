//! Node identifier allocation, data insertion and node lookup.

mod common;

use std::collections::BTreeSet;

use common::{kitchen_sink_figure, single_line_figure};
use ironlab_ir::*;

/// Every node identifier in a figure, found without the API under test.
fn all_node_ids(fig: &Figure) -> BTreeSet<NodeId> {
    let mut ids = BTreeSet::from([fig.id]);
    for axes in &fig.axes {
        ids.insert(axes.id);
        ids.extend(axes.artists.iter().map(Artist::id));
    }
    ids
}

// Why: a newly allocated identifier must never collide with an existing node, or links
// and interaction would address the wrong node.
#[test]
fn allocated_node_ids_differ_from_every_existing_node() {
    let mut fig = kitchen_sink_figure();
    let existing = all_node_ids(&fig);
    let id = fig.alloc_node_id();
    assert!(
        !existing.contains(&id),
        "{id:?} collides with an existing node"
    );
}

// Why: builders allocate several identifiers before inserting any node (for example an
// axes and its artists), so successive allocations must differ even without insertion.
#[test]
fn successive_allocations_differ_without_insertion() {
    let mut fig = Figure::new();
    let ids: BTreeSet<NodeId> = (0..100).map(|_| fig.alloc_node_id()).collect();
    assert_eq!(ids.len(), 100);
    assert!(!ids.contains(&fig.id));
}

// Why: a loaded figure has no allocation history, so allocation must be derived from the
// identifiers present; otherwise editing a reopened file would create duplicate nodes.
#[test]
fn allocation_after_loading_avoids_high_existing_ids() {
    let (mut fig, _, _) = single_line_figure();
    fig.id = NodeId(40);
    fig.axes[0].id = NodeId(1_000_000);
    if let Artist::Line(line) = &mut fig.axes[0].artists[0] {
        line.id = NodeId(500);
    }
    let mut loaded = Figure::from_json(&fig.to_json()).unwrap();
    let existing = all_node_ids(&loaded);

    let first = loaded.alloc_node_id();
    let second = loaded.alloc_node_id();
    assert!(!existing.contains(&first) && !existing.contains(&second));
    assert_ne!(first, second);
}

// Why: allocation state is bookkeeping, not content, so it must not make otherwise
// identical figures compare unequal (which would break persistence equality checks).
#[test]
fn allocation_state_does_not_affect_figure_equality() {
    let original = kitchen_sink_figure();
    let mut allocated = original.clone();
    allocated.alloc_node_id();
    assert_eq!(allocated, original);
}

// Why: inserting data must never overwrite an array that artists already reference,
// including in a loaded figure with sparse data identifiers.
#[test]
fn added_data_never_replaces_existing_arrays() {
    let mut fig = Figure::from_json(include_str!("fixtures/decay.fig.json")).unwrap();
    let before = fig.data.clone();

    let first = fig.add_data(NdArray::vector(vec![42.0]));
    let second = fig.add_data(NdArray::vector(vec![43.0]));

    assert!(!before.contains_key(&first) && !before.contains_key(&second));
    assert_ne!(first, second);
    for (id, array) in &before {
        assert_eq!(&fig.data[id], array, "{id:?} was modified");
    }
    assert_eq!(fig.data[&first], NdArray::vector(vec![42.0]));
    assert_eq!(fig.data[&second], NdArray::vector(vec![43.0]));
}

// Why: the viewer and facade address axes by identifier; a lookup must find exactly that
// axes, and must not return an axes for an artist's or unknown identifier.
#[test]
fn axes_lookup_finds_only_axes() {
    let mut fig = kitchen_sink_figure();
    let target = fig.axes[4].id;
    assert_eq!(fig.axes(target).map(|a| a.id), Some(target));

    let artist_id = fig.axes[0].artists[0].id();
    assert!(fig.axes(artist_id).is_none());
    assert!(fig.axes(NodeId(u64::MAX)).is_none());
    assert!(fig.axes(fig.id).is_none());

    fig.axes_mut(target).unwrap().title = Some(Text::from("changed"));
    assert_eq!(fig.axes[4].title, Some(Text::from("changed")));
}

// Why: legend clicks identify an artist and need both the artist and the axes that holds
// it (for its legend and colormap), so the lookup must return the containing axes.
#[test]
fn artist_lookup_returns_the_containing_axes() {
    let fig = kitchen_sink_figure();
    let axes = &fig.axes[5];
    let wanted = axes.artists[1].id();

    let (found_axes, found_artist) = fig.artist(wanted).expect("artist exists");
    assert_eq!(found_axes.id, axes.id);
    assert_eq!(found_artist.id(), wanted);

    assert!(fig.artist(axes.id).is_none(), "an axes is not an artist");
    assert!(fig.artist(NodeId(u64::MAX)).is_none());
}

// Why: toggling a legend entry hides or shows the artist by mutating the IR, which must
// change exactly that artist (wherever it is in the figure) and persist when the figure
// is saved and reopened, so that the next render, export and file reflect it.
#[test]
fn artist_visibility_toggle_changes_only_that_artist_and_persists() {
    let original = kitchen_sink_figure();
    let mut fig = original.clone();
    let hidden = fig.axes[5].artists[1].id();
    let shown = fig.axes[0].artists[0].id();
    assert!(!fig.axes[5].artists[1].visible() && fig.axes[0].artists[0].visible());

    fig.artist_mut(hidden).unwrap().set_visible(true);
    fig.artist_mut(shown).unwrap().set_visible(false);
    assert!(fig.artist_mut(NodeId(u64::MAX)).is_none());
    assert!(
        fig.artist_mut(fig.axes[5].id).is_none(),
        "an axes is not an artist"
    );

    let mut expected = original;
    expected.axes[5].artists[1].set_visible(true);
    expected.axes[0].artists[0].set_visible(false);
    assert_eq!(fig, expected);

    let reopened = Figure::from_json(&fig.to_json()).unwrap();
    assert!(reopened.artist(hidden).unwrap().1.visible());
    assert!(!reopened.artist(shown).unwrap().1.visible());
    assert_eq!(reopened, expected);
}
