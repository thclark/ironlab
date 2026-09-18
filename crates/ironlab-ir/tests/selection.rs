//! Selections of nodes and data points, kept consistent with the edits of a transaction.

mod common;

use common::edits::{Streaming, applied, axes_node, rows, set, streaming_figure, tx};
use ironlab_ir::selection::Selection;
use ironlab_ir::*;

/// Appends one value per entry to the x and y data of the first line.
fn append_to_line(s: &Streaming, count: usize, retain: Option<u64>) -> Transaction {
    let values: Vec<f64> = (0..count).map(|k| 100.0 + k as f64).collect();
    tx([s.x, s.y].map(|id| Edit::AppendData {
        id,
        array: NdArray::vector(values.clone()),
        retain,
    }))
}

/// Returns the selection after a transaction, checking first that the transaction is one
/// the figure accepts, since selections are updated only for applied transactions.
fn updated(s: &Streaming, selection: &Selection, transaction: &Transaction) -> Option<Selection> {
    applied(&s.fig, transaction).expect("the transaction applies");
    selection.updated(&s.fig, transaction)
}

// Why: streaming appends new samples at the end, so the points a user selected keep their
// indices and must stay selected, also when a window is given that is still longer than the
// data (which discards nothing, and must not be computed as a negative shift).
#[test]
fn appending_without_discarding_keeps_the_selected_indices() {
    let s = streaming_figure();
    let selection = Selection::points(s.line, [1, 3]);
    for retain in [None, Some(7), Some(100)] {
        assert_eq!(
            updated(&s, &selection, &append_to_line(&s, 2, retain)),
            Some(selection.clone()),
            "retain {retain:?}"
        );
    }
}

// Why: a rolling window discards the oldest samples, so a selected sample moves down by the
// number discarded, and a selected sample that was discarded is no longer selectable. The
// append touches both x and y, and the indices must shift once, by the primary array.
#[test]
fn a_rolling_window_shifts_indices_down_by_the_discarded_count_and_drops_discarded_ones() {
    let s = streaming_figure();
    // Five values plus two appended, keeping five, discards two.
    let selection = Selection::points(s.line, [1, 3, 4]);
    assert_eq!(
        updated(&s, &selection, &append_to_line(&s, 2, Some(5))),
        Some(Selection::points(s.line, [1, 2]))
    );
}

// Why: a transaction may carry several frames of a stream, so the number of entries a
// window discards depends on the length left by the earlier appends, not on the length
// before the transaction: five values, plus two, plus one with a window of five, discard
// three.
#[test]
fn the_discarded_count_uses_the_length_left_by_earlier_edits_of_the_transaction() {
    let s = streaming_figure();
    let mut transaction = append_to_line(&s, 2, None);
    transaction
        .edits
        .extend(append_to_line(&s, 1, Some(5)).edits);
    assert_eq!(
        updated(&s, &Selection::points(s.line, [1, 3, 4]), &transaction),
        Some(Selection::points(s.line, [0, 1]))
    );
}

// Why: the flat indices of a field of shape `[ny, nx]` count whole rows, so discarding k rows
// shifts every index by k times nx.
#[test]
fn discarding_rows_of_a_field_shifts_flat_indices_by_whole_rows() {
    let s = streaming_figure();
    // Four rows of three plus one appended, keeping four, discards one row of three values.
    let transaction = tx([
        Edit::AppendData {
            id: s.z,
            array: rows(1, 3, 100.0),
            retain: Some(4),
        },
        Edit::AppendData {
            id: s.gy,
            array: NdArray::vector(vec![4.0]),
            retain: Some(4),
        },
    ]);
    let selection = Selection::points(s.contour, [2, 4, 11]);
    assert_eq!(
        updated(&s, &selection, &transaction),
        Some(Selection::points(s.contour, [1, 8]))
    );
}

// Why: when every selected sample has scrolled out of the window, the selection is still a
// selection of points of that artist, of which none remain; it must not silently widen to
// the whole artist.
#[test]
fn when_every_selected_index_is_discarded_an_empty_set_of_indices_remains() {
    let s = streaming_figure();
    let selection = Selection::points(s.line, [1, 3]);
    assert_eq!(
        updated(&s, &selection, &append_to_line(&s, 2, Some(2))),
        Some(Selection::points(s.line, []))
    );
}

// Why: replacing an array of the selected artist (its coordinates or its grid) may reorder
// or resize the data, so indices into it mean nothing any more; the artist stays selected.
#[test]
fn replacing_any_array_of_the_selected_artist_keeps_the_node_and_drops_the_indices() {
    let s = streaming_figure();
    let replace_y = tx([Edit::PutData {
        id: s.y,
        array: NdArray::vector(vec![0.0; 5]),
    }]);
    assert_eq!(
        updated(&s, &Selection::points(s.line, [0, 2]), &replace_y),
        Some(Selection::node(s.line))
    );
    let replace_grid = tx([Edit::PutData {
        id: s.gx,
        array: NdArray::vector(vec![5.0, 6.0, 7.0]),
    }]);
    assert_eq!(
        updated(&s, &Selection::points(s.contour, [0]), &replace_grid),
        Some(Selection::node(s.contour))
    );
}

// Why: a removed plot cannot be selected, whether it is removed itself or with its axes.
#[test]
fn removing_the_selected_node_or_its_axes_clears_the_selection() {
    let s = streaming_figure();
    let selection = Selection::points(s.contour, [0]);
    assert_eq!(
        updated(&s, &selection, &tx([Edit::Remove { node: s.contour }])),
        None
    );
    let remove_axes = tx([Edit::Remove { node: s.axes }]);
    assert_eq!(
        updated(&s, &Selection::points(s.line, [1]), &remove_axes),
        None
    );
}

// Why: moving a plot keeps its identity and its data, so the user's selection follows it,
// also when it is moved out of an axes that the same transaction then removes.
#[test]
fn moving_the_selected_artist_keeps_the_selection_even_when_its_old_axes_is_removed() {
    let s = streaming_figure();
    let selection = Selection::points(s.line, [2]);
    let move_line = Edit::Move {
        node: s.line,
        parent: s.other_axes,
        index: None,
    };
    assert_eq!(
        updated(&s, &selection, &tx([move_line.clone()])),
        Some(selection.clone())
    );
    assert_eq!(
        updated(
            &s,
            &selection,
            &tx([move_line, Edit::Remove { node: s.axes }])
        ),
        Some(selection)
    );
}

// Why: a node inserted with the identifier of a removed one is a different plot, which the
// user did not select.
#[test]
fn removing_and_reinserting_a_node_with_the_same_identifier_clears_the_selection() {
    let s = streaming_figure();
    let artist = s.fig.axes[0].artists[0].clone();
    let transaction = tx([
        Edit::Remove { node: s.line },
        Edit::Insert {
            parent: s.axes,
            index: None,
            node: Node::Artist(artist),
        },
    ]);
    assert_eq!(updated(&s, &Selection::node(s.line), &transaction), None);
}

// Why: edits of other plots, other data and other properties must not disturb what the user
// selected, including a rolling window on another line's data.
#[test]
fn edits_of_unrelated_data_and_nodes_leave_the_selection_unchanged() {
    let s = streaming_figure();
    let selection = Selection::points(s.line, [0, 4]);
    let transaction = tx([
        Edit::AppendData {
            id: s.other_x,
            array: NdArray::vector(vec![4.0]),
            retain: Some(2),
        },
        Edit::AppendData {
            id: s.other_y,
            array: NdArray::vector(vec![16.0]),
            retain: Some(2),
        },
        Edit::PutData {
            id: DataId(100),
            array: NdArray::vector(vec![1.0]),
        },
        set(
            s.other_axes,
            "colormap",
            Value::ColormapName(ColormapName::Gray),
        ),
        set(s.line, "visible", Value::Bool(false)),
        Edit::Insert {
            parent: s.fig.id,
            index: None,
            node: axes_node(NodeId(100), vec![]),
        },
        Edit::Remove { node: s.other_line },
    ]);
    assert_eq!(updated(&s, &selection, &transaction), Some(selection));
}

// Why: a plot selected as a whole has no indices to shift or drop, so data edits leave it
// selected as a whole.
#[test]
fn a_node_selected_as_a_whole_is_unaffected_by_its_data_edits() {
    let s = streaming_figure();
    let selection = Selection::node(s.line);
    assert_eq!(
        updated(&s, &selection, &append_to_line(&s, 2, Some(3))),
        Some(selection.clone())
    );
    let replace = tx([Edit::PutData {
        id: s.x,
        array: NdArray::vector(vec![0.0; 5]),
    }]);
    assert_eq!(updated(&s, &selection, &replace), Some(selection));
}
