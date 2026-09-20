//! Applying transactions: sets, structural edits and data edits, atomicity, validation
//! against the issues a figure already has, and inverses.

mod common;

use std::sync::LazyLock;

use common::edits::{
    Streaming, applied, artist_ids, axes_ids, axes_node, line, manual, nodes, path, perturb, rows,
    set, streaming_figure, tx,
};
use common::{
    FigureBuilder, SPECIAL_F64, find_axes, float_bits, floats, kitchen_sink_figure, limits_of,
    row_of_axes, single_line_figure,
};
use ironlab_ir::*;
use proptest::prelude::*;

/// Returns whether a list of issues contains one of the given kind.
fn has_issue(issues: &[ValidationIssue], kind: IssueKind) -> bool {
    issues.iter().any(|issue| issue.kind == kind)
}

/// An array of 8-bit values with the given shape.
fn bytes(shape: Vec<usize>, values: Vec<u8>) -> NdArray {
    NdArray::from_shape_u8(shape, values).expect("the shape matches the values")
}

/// A figure with one 2D axes holding three lines on shared data, and a second, empty
/// 2D axes. Returns the figure, the first axes, its lines in order and the second axes.
fn three_lines_figure() -> (Figure, NodeId, [NodeId; 3], NodeId) {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout { rows: 1, cols: 2 };
    let axes = b.axes2d(0, 0);
    let x = b.vector(&[1.0, 2.0]);
    let y = b.vector(&[3.0, 4.0]);
    let lines = [b.node(), b.node(), b.node()];
    for id in lines {
        b.push(axes, line(id, x, y));
    }
    let empty = b.axes2d(0, 1);
    (b.build(), axes, lines, empty)
}

// ---------------------------------------------------------------------------------
// Set
// ---------------------------------------------------------------------------------

// Why: edits are literal, so that a client mirroring a figure reaches the same state by
// applying the same edits without knowing any rule; setting the limits of a linked axes
// must not propagate to its partners (that is the job of `command::set_limits`).
#[test]
fn a_set_changes_only_the_named_property_of_the_named_node_even_when_linked() {
    let (mut fig, ids) = row_of_axes(3);
    fig.links = vec![AxisLink {
        dimension: Dimension::X,
        axes: ids.clone(),
    }];
    let (edited, _) = applied(
        &fig,
        &tx([set(ids[1], "x.limits", Value::Limits(manual(-5.0, 5.0)))]),
    )
    .unwrap();
    let mut expected = fig.clone();
    expected.axes[1].x.limits = manual(-5.0, 5.0);
    assert_eq!(edited, expected);
    assert_eq!(
        limits_of(&edited, ids[0], Dimension::X),
        limits_of(&fig, ids[0], Dimension::X)
    );
}

// ---------------------------------------------------------------------------------
// Insert
// ---------------------------------------------------------------------------------

// Why: a program adds subplots in order, and a property editor may insert one at a given
// position; `None` must append and an index must place the axes at that position.
#[test]
fn an_inserted_axes_is_appended_or_placed_at_its_index() {
    let (fig, ids) = row_of_axes(2);
    let new = NodeId(100);
    for (index, expected) in [
        (None, vec![ids[0], ids[1], new]),
        (Some(0), vec![new, ids[0], ids[1]]),
        (Some(1), vec![ids[0], new, ids[1]]),
        (Some(2), vec![ids[0], ids[1], new]),
    ] {
        let insert = Edit::Insert {
            parent: fig.id,
            index,
            node: axes_node(new, vec![]),
        };
        let (edited, _) = applied(&fig, &tx([insert])).unwrap();
        assert_eq!(axes_ids(&edited), expected, "index {index:?}");
    }
}

// Why: artists are drawn in list order, so an artist inserted at an index must be drawn at
// that position within its axes, with its subtree intact; the first position and the
// position equal to the length of the list are both valid, and `None` appends.
#[test]
fn an_inserted_artist_is_placed_in_drawing_order_within_its_axes() {
    let (fig, axes, [a, b, c], _) = three_lines_figure();
    let Artist::Line(template) = &find_axes(&fig, axes).artists[0] else {
        panic!("the fixture artist is a line");
    };
    let new = NodeId(50);
    let artist = line(new, template.x, template.y);
    for (index, expected) in [
        (Some(0), [new, a, b, c]),
        (Some(1), [a, new, b, c]),
        (Some(3), [a, b, c, new]),
        (None, [a, b, c, new]),
    ] {
        let (edited, _) = applied(
            &fig,
            &tx([Edit::Insert {
                parent: axes,
                index,
                node: Node::Artist(artist.clone()),
            }]),
        )
        .unwrap();
        assert_eq!(artist_ids(&edited, axes), expected, "index {index:?}");
        assert_eq!(edited.artist(new).unwrap().1, &artist);
    }
}

// Why: an axes belongs in the figure and an artist in an axes; any other parent would
// build a tree that the IR cannot represent, so it must be refused naming both nodes.
#[test]
fn inserting_under_a_parent_that_cannot_hold_the_node_fails() {
    let (fig, axes, lines, _) = three_lines_figure();
    let artist = find_axes(&fig, axes).artists[0].clone();
    let with_id = |id| match artist.clone() {
        Artist::Line(l) => Artist::Line(Line { id, ..l }),
        _ => unreachable!(),
    };
    let cases = [
        (axes, axes_node(NodeId(100), vec![])),
        (lines[0], axes_node(NodeId(100), vec![])),
        (fig.id, Node::Artist(with_id(NodeId(100)))),
        (lines[0], Node::Artist(with_id(NodeId(100)))),
    ];
    for (parent, node) in cases {
        let mut edited = fig.clone();
        let result = edited.apply(&tx([Edit::Insert {
            parent,
            index: None,
            node,
        }]));
        assert!(
            matches!(
                result,
                Err(EditError::InvalidParent { edit: Some(0), node: NodeId(100), parent: p }) if p == parent
            ),
            "{result:?}"
        );
        assert_eq!(edited, fig);
    }

    let result = applied(
        &fig,
        &tx([Edit::Insert {
            parent: NodeId(999),
            index: None,
            node: Node::Artist(with_id(NodeId(100))),
        }]),
    );
    assert!(
        matches!(
            result,
            Err(EditError::UnknownNode {
                edit: Some(0),
                node: NodeId(999)
            })
        ),
        "{result:?}"
    );
}

// Why: an index past the end of the list has no meaning; clamping it silently would put
// the node somewhere the sender did not ask for.
#[test]
fn inserting_beyond_the_end_of_the_list_fails() {
    let (fig, _) = row_of_axes(2);
    let result = applied(
        &fig,
        &tx([Edit::Insert {
            parent: fig.id,
            index: Some(3),
            node: axes_node(NodeId(100), vec![]),
        }]),
    );
    assert!(
        matches!(
            result,
            Err(EditError::IndexOutOfRange { edit: Some(0), parent, index: 3, len: 2 }) if parent == fig.id
        ),
        "{result:?}"
    );
}

// Why: identifiers are chosen by the creator of a node, so a transaction that reuses an
// identifier (of the figure, of another node, or twice within the inserted subtree) must
// be refused, or links, overlays and selections would address two nodes at once.
#[test]
fn inserting_a_node_whose_identifier_is_in_use_fails() {
    let (fig, axes, lines, _) = three_lines_figure();
    let Artist::Line(template) = &find_axes(&fig, axes).artists[0] else {
        panic!("the fixture artist is a line");
    };
    let (x, y) = (template.x, template.y);
    let cases: Vec<(NodeId, Node, NodeId)> = vec![
        (fig.id, axes_node(fig.id, vec![]), fig.id),
        (fig.id, axes_node(axes, vec![]), axes),
        (
            fig.id,
            axes_node(NodeId(100), vec![line(lines[1], x, y)]),
            lines[1],
        ),
        (
            fig.id,
            axes_node(
                NodeId(100),
                vec![line(NodeId(101), x, y), line(NodeId(101), x, y)],
            ),
            NodeId(101),
        ),
        (
            fig.id,
            axes_node(NodeId(100), vec![line(NodeId(100), x, y)]),
            NodeId(100),
        ),
        (axes, Node::Artist(line(axes, x, y)), axes),
        (axes, Node::Artist(line(lines[2], x, y)), lines[2]),
    ];
    for (parent, node, duplicate) in cases {
        let mut edited = fig.clone();
        let result = edited.apply(&tx([Edit::Insert {
            parent,
            index: None,
            node,
        }]));
        assert!(
            matches!(result, Err(EditError::DuplicateId { edit: Some(0), node }) if node == duplicate),
            "{duplicate}: {result:?}"
        );
        assert_eq!(edited, fig);
    }
}

// Why: because the creator chooses identifiers, one transaction can create an axes, put an
// artist in it and set its properties, which is how a session adds a plot in one step.
#[test]
fn a_transaction_can_create_a_node_and_then_refer_to_it() {
    let (fig, axes, _) = single_line_figure();
    let Artist::Line(template) = &find_axes(&fig, axes).artists[0] else {
        panic!("the fixture artist is a line");
    };
    let (new_axes, new_line) = (NodeId(100), NodeId(101));
    let (edited, _) = applied(
        &fig,
        &tx([
            Edit::Insert {
                parent: fig.id,
                index: None,
                node: axes_node(new_axes, vec![]),
            },
            Edit::Insert {
                parent: new_axes,
                index: None,
                node: Node::Artist(line(new_line, template.x, template.y)),
            },
            set(new_axes, "title", Value::Text(Text::new("created"))),
            set(new_line, "visible", Value::Bool(false)),
        ]),
    )
    .unwrap();
    assert_eq!(
        find_axes(&edited, new_axes).title,
        Some(Text::new("created"))
    );
    assert!(!edited.artist(new_line).unwrap().1.visible());
}

// Why: a program that allocates identifiers must never be handed one that a transaction
// inserted, even after undo removed that node again, or a later edit meant for the new
// node would apply to an overlay entry or selection of the old one.
#[test]
fn allocation_never_hands_out_an_identifier_that_was_inserted() {
    let (fig, _, _) = single_line_figure();
    let next = fig.clone().alloc_node_id();
    let insert = tx([Edit::Insert {
        parent: fig.id,
        index: None,
        node: axes_node(next, vec![]),
    }]);

    let mut inserted = fig.clone();
    inserted.apply(&insert).unwrap();
    assert_ne!(inserted.alloc_node_id(), next);

    let mut undone = fig.clone();
    let inverse = undone.apply(&insert).unwrap();
    undone.apply(&inverse).unwrap();
    assert_eq!(undone, fig);
    assert_ne!(undone.alloc_node_id(), next);
}

// ---------------------------------------------------------------------------------
// Remove and move
// ---------------------------------------------------------------------------------

// Why: removing an axes removes the plots drawn in it; leaving its artists behind would
// orphan them.
#[test]
fn removing_a_node_removes_its_subtree() {
    let (fig, axes, lines, empty) = three_lines_figure();
    let (edited, _) = applied(&fig, &tx([Edit::Remove { node: lines[1] }])).unwrap();
    assert_eq!(artist_ids(&edited, axes), [lines[0], lines[2]]);

    let (edited, _) = applied(&fig, &tx([Edit::Remove { node: axes }])).unwrap();
    assert_eq!(axes_ids(&edited), [empty]);
    for id in lines {
        assert_eq!(edited.node_kind(id), None);
    }

    let result = applied(&fig, &tx([Edit::Remove { node: NodeId(999) }]));
    assert!(matches!(
        result,
        Err(EditError::UnknownNode {
            edit: Some(0),
            node: NodeId(999)
        })
    ));
}

// Why: the figure is the root of the tree; removing or moving it has no meaning.
#[test]
fn the_figure_node_can_be_neither_removed_nor_moved() {
    let (fig, axes, _) = single_line_figure();
    for edit in [
        Edit::Remove { node: fig.id },
        Edit::Move {
            node: fig.id,
            parent: axes,
            index: None,
        },
    ] {
        let mut edited = fig.clone();
        let result = edited.apply(&tx([edit]));
        assert!(
            matches!(result, Err(EditError::RootNode { edit: Some(0), node }) if node == fig.id),
            "{result:?}"
        );
        assert_eq!(edited, fig);
    }
}

// Why: `Remove` is literal, so removing a linked axes leaves a link to a missing axes,
// which validation reports; the transaction must be refused (use `command::remove_axes`)
// rather than leave a figure that no longer validates.
#[test]
fn removing_a_linked_axes_alone_is_refused_as_a_dangling_link() {
    let (mut fig, ids) = row_of_axes(2);
    fig.links = vec![AxisLink {
        dimension: Dimension::Y,
        axes: ids.clone(),
    }];
    let mut edited = fig.clone();
    let result = edited.apply(&tx([Edit::Remove { node: ids[0] }]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues)) if has_issue(issues, IssueKind::DanglingLink)),
        "{result:?}"
    );
    assert_eq!(edited, fig);
}

// Why: reordering by dragging names the target position in the list as it will look once
// the node has left its old position; this is the convention that makes "move to the end"
// the same index whichever node is moved.
#[test]
fn a_move_index_refers_to_the_list_after_the_node_is_removed() {
    let (fig, axes, [a, b, c], _) = three_lines_figure();
    for (node, index, expected) in [
        (a, Some(2), [b, c, a]),
        (c, Some(0), [c, a, b]),
        (a, Some(1), [b, a, c]),
        (b, None, [a, c, b]),
    ] {
        let (edited, _) = applied(
            &fig,
            &tx([Edit::Move {
                node,
                parent: axes,
                index,
            }]),
        )
        .unwrap();
        assert_eq!(artist_ids(&edited, axes), expected, "{node} to {index:?}");
    }

    let (fig, ids) = row_of_axes(3);
    let (edited, _) = applied(
        &fig,
        &tx([Edit::Move {
            node: ids[0],
            parent: fig.id,
            index: Some(2),
        }]),
    )
    .unwrap();
    assert_eq!(axes_ids(&edited), [ids[1], ids[2], ids[0]]);
}

// Why: moving a plot to another subplot keeps the artist (and its identifier, so overlay
// entries and selections follow it) and removes it from its old axes.
#[test]
fn an_artist_moves_to_another_axes() {
    let (fig, axes, [a, b, c], empty) = three_lines_figure();
    let (edited, _) = applied(
        &fig,
        &tx([Edit::Move {
            node: b,
            parent: empty,
            index: Some(0),
        }]),
    )
    .unwrap();
    assert_eq!(artist_ids(&edited, axes), [a, c]);
    assert_eq!(artist_ids(&edited, empty), [b]);
    assert_eq!(edited.artist(b).unwrap().0.id, empty);
}

// Why: a move to a parent that cannot hold the node, to an unknown parent, or past the end
// of the target list must be refused and leave the node where it was.
#[test]
fn a_move_to_an_invalid_parent_or_index_fails() {
    let (fig, axes, [a, _, _], empty) = three_lines_figure();
    let cases = [(a, fig.id, None), (axes, empty, None), (a, a, None)];
    for (node, parent, index) in cases {
        let result = applied(
            &fig,
            &tx([Edit::Move {
                node,
                parent,
                index,
            }]),
        );
        assert!(
            matches!(result, Err(EditError::InvalidParent { edit: Some(0), node: n, parent: p }) if n == node && p == parent),
            "{node} to {parent}: {result:?}"
        );
    }
    let result = applied(
        &fig,
        &tx([Edit::Move {
            node: a,
            parent: axes,
            index: Some(3),
        }]),
    );
    assert!(
        matches!(
            result,
            Err(EditError::IndexOutOfRange {
                edit: Some(0),
                index: 3,
                len: 2,
                ..
            })
        ),
        "{result:?}"
    );
    let result = applied(
        &fig,
        &tx([Edit::Move {
            node: a,
            parent: empty,
            index: Some(1),
        }]),
    );
    assert!(
        matches!(
            result,
            Err(EditError::IndexOutOfRange {
                edit: Some(0),
                index: 1,
                len: 0,
                ..
            })
        ),
        "{result:?}"
    );
    let result = applied(
        &fig,
        &tx([Edit::Move {
            node: a,
            parent: NodeId(999),
            index: None,
        }]),
    );
    assert!(matches!(
        result,
        Err(EditError::UnknownNode {
            edit: Some(0),
            node: NodeId(999)
        })
    ));
    let result = applied(
        &fig,
        &tx([Edit::Move {
            node: NodeId(999),
            parent: axes,
            index: None,
        }]),
    );
    assert!(matches!(
        result,
        Err(EditError::UnknownNode {
            edit: Some(0),
            node: NodeId(999)
        })
    ));
}

// ---------------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------------

// Why: a session sends a whole new array to create data for a new plot or to replace a
// frame of data.
#[test]
fn put_data_creates_or_replaces_an_array() {
    let s = streaming_figure();
    let new = NdArray::vector(vec![7.0, 8.0]);
    let replacement = NdArray::vector(vec![20.0, 21.0, 22.0, 23.0, 24.0]);
    let (edited, _) = applied(
        &s.fig,
        &tx([
            Edit::PutData {
                id: DataId(100),
                array: new.clone(),
            },
            Edit::PutData {
                id: s.y,
                array: replacement.clone(),
            },
        ]),
    )
    .unwrap();
    assert_eq!(edited.data[&DataId(100)], new);
    assert_eq!(edited.data[&s.y], replacement);
}

// Why: removing an array that an artist still refers to would leave the artist without
// data; validation reports it and the removal must be refused, while removing an unused
// array succeeds and removing an unknown one is an error.
#[test]
fn removing_data_that_an_artist_refers_to_is_refused() {
    let s = streaming_figure();
    let mut edited = s.fig.clone();
    let result = edited.apply(&tx([Edit::RemoveData { id: s.y }]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues)) if has_issue(issues, IssueKind::UnknownData)),
        "{result:?}"
    );
    assert_eq!(edited, s.fig);

    let (edited, _) = applied(
        &s.fig,
        &tx([
            Edit::PutData {
                id: DataId(100),
                array: NdArray::vector(vec![1.0]),
            },
            Edit::RemoveData { id: DataId(100) },
        ]),
    )
    .unwrap();
    assert_eq!(edited, s.fig);

    let result = applied(&s.fig, &tx([Edit::RemoveData { id: DataId(999) }]));
    assert!(matches!(
        result,
        Err(EditError::UnknownData {
            edit: Some(0),
            id: DataId(999)
        })
    ));
}

/// Appends the given values to the x and y data of the first line of the streaming figure.
fn append_to_line(s: &Streaming, xs: &[f64], ys: &[f64], retain: Option<u64>) -> Transaction {
    tx([
        Edit::AppendData {
            id: s.x,
            array: NdArray::vector(xs.to_vec()),
            retain,
        },
        Edit::AppendData {
            id: s.y,
            array: NdArray::vector(ys.to_vec()),
            retain,
        },
    ])
}

// Why: streaming appends samples to the end of each coordinate vector; appending to every
// coordinate of a line together keeps the line valid.
#[test]
fn appending_extends_a_vector() {
    let s = streaming_figure();
    let (edited, _) = applied(
        &s.fig,
        &append_to_line(&s, &[5.0, 6.0], &[15.0, 16.0], None),
    )
    .unwrap();
    assert_eq!(
        edited.data[&s.x],
        NdArray::vector(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
    );
    assert_eq!(edited.data[&s.y].shape, vec![7]);
}

// Why: a waterfall or spectrogram streams whole rows of a field of shape `[ny, nx]`, so
// appending must extend the first dimension and concatenate the values row-major.
#[test]
fn appending_rows_extends_a_two_dimensional_array() {
    let s = streaming_figure();
    let (edited, _) = applied(
        &s.fig,
        &tx([
            Edit::AppendData {
                id: s.z,
                array: rows(1, 3, 100.0),
                retain: None,
            },
            Edit::AppendData {
                id: s.gy,
                array: NdArray::vector(vec![4.0]),
                retain: None,
            },
        ]),
    )
    .unwrap();
    let z = &edited.data[&s.z];
    assert_eq!(z.shape, vec![5, 3]);
    assert_eq!(&floats(z)[..12], floats(&s.fig.data[&s.z]));
    assert_eq!(&floats(z)[12..], &[100.0, 101.0, 102.0]);
}

// Why: entries whose shape after the first dimension differs cannot be rows of the array;
// accepting them would corrupt every row after the first.
#[test]
fn appending_entries_of_another_trailing_shape_fails() {
    let s = streaming_figure();
    for (id, array, existing) in [
        (s.z, rows(1, 2, 0.0), vec![4, 3]),
        (s.z, NdArray::vector(vec![1.0, 2.0, 3.0]), vec![4, 3]),
        (s.x, rows(1, 1, 0.0), vec![5]),
        // An array with no dimensions has no first dimension to append along.
        (
            s.x,
            NdArray {
                shape: vec![],
                values: Values::F64(vec![1.0]),
            },
            vec![5],
        ),
    ] {
        let appended = array.shape.clone();
        let mut edited = s.fig.clone();
        let result = edited.apply(&tx([Edit::AppendData {
            id,
            array,
            retain: None,
        }]));
        assert!(
            matches!(
                &result,
                Err(EditError::ShapeMismatch { edit: Some(0), id: i, existing: e, appended: a })
                    if *i == id && *e == existing && *a == appended
            ),
            "{result:?}"
        );
        assert_eq!(edited, s.fig);
    }
    let result = applied(
        &s.fig,
        &tx([Edit::AppendData {
            id: DataId(999),
            array: NdArray::vector(vec![1.0]),
            retain: None,
        }]),
    );
    assert!(matches!(
        result,
        Err(EditError::UnknownData {
            edit: Some(0),
            id: DataId(999)
        })
    ));
}

// Why: `retain` gives streamed data a rolling window: only the last entries along the
// first dimension are kept, whole rows for a field, and a window longer than the data
// keeps everything.
#[test]
fn retain_keeps_the_last_entries_along_the_first_dimension() {
    let s = streaming_figure();
    let (edited, _) = applied(
        &s.fig,
        &append_to_line(&s, &[5.0, 6.0], &[15.0, 16.0], Some(4)),
    )
    .unwrap();
    assert_eq!(edited.data[&s.x], NdArray::vector(vec![3.0, 4.0, 5.0, 6.0]));
    assert_eq!(
        edited.data[&s.y],
        NdArray::vector(vec![13.0, 14.0, 15.0, 16.0])
    );

    let (edited, _) = applied(&s.fig, &append_to_line(&s, &[5.0], &[15.0], Some(100))).unwrap();
    assert_eq!(edited.data[&s.x].shape, vec![6]);

    let (edited, _) = applied(
        &s.fig,
        &tx([
            Edit::AppendData {
                id: s.z,
                array: rows(1, 3, 100.0),
                retain: Some(3),
            },
            Edit::AppendData {
                id: s.gy,
                array: NdArray::vector(vec![4.0]),
                retain: Some(3),
            },
        ]),
    )
    .unwrap();
    let z = &edited.data[&s.z];
    assert_eq!(z.shape, vec![3, 3]);
    assert_eq!(&floats(z)[..6], &floats(&s.fig.data[&s.z])[6..]);
    assert_eq!(&floats(z)[6..], &[100.0, 101.0, 102.0]);
    assert_eq!(edited.data[&s.gy], NdArray::vector(vec![2.0, 3.0, 4.0]));
}

// Why: a stream may deliver a frame with no new samples; appending no entries must leave
// the data unchanged (for a field, zero rows of the right width), while a window given with
// it still trims the data, because `retain` describes the data after the append.
#[test]
fn appending_no_entries_changes_nothing_but_retain_still_applies() {
    let s = streaming_figure();
    let (edited, _) = applied(
        &s.fig,
        &tx([
            Edit::AppendData {
                id: s.x,
                array: NdArray::vector(vec![]),
                retain: None,
            },
            Edit::AppendData {
                id: s.z,
                array: rows(0, 3, 0.0),
                retain: None,
            },
        ]),
    )
    .unwrap();
    assert_eq!(edited, s.fig);

    let (edited, _) = applied(&s.fig, &append_to_line(&s, &[], &[], Some(2))).unwrap();
    assert_eq!(edited.data[&s.x], NdArray::vector(vec![3.0, 4.0]));
    assert_eq!(edited.data[&s.y], NdArray::vector(vec![13.0, 14.0]));
}

// Why: `retain: Some(0)` is a window of no entries, which clears the data (keeping the
// trailing shape of a field) rather than being read as "no window"; its inverse must bring
// every discarded entry back.
#[test]
fn retain_zero_keeps_no_entries_and_its_inverse_restores_them() {
    let s = streaming_figure();
    let transaction = tx([
        Edit::AppendData {
            id: s.z,
            array: rows(1, 3, 100.0),
            retain: Some(0),
        },
        Edit::AppendData {
            id: s.gy,
            array: NdArray::vector(vec![4.0]),
            retain: Some(0),
        },
    ]);
    let (edited, inverse) = applied(&s.fig, &transaction).unwrap();
    assert_eq!(edited.data[&s.z].shape, vec![0, 3]);
    assert!(edited.data[&s.z].is_empty());
    assert_eq!(edited.data[&s.gy], NdArray::vector(vec![]));
    let (restored, _) = applied(&edited, &inverse).unwrap();
    assert_eq!(restored, s.fig);
}

// Why: validation runs after the whole transaction, so appending to x and y of a line in
// one transaction is valid, but appending to x alone leaves coordinate arrays of different
// lengths, which must be refused.
#[test]
fn appending_to_one_coordinate_of_a_line_alone_is_refused() {
    let s = streaming_figure();
    let mut edited = s.fig.clone();
    let result = edited.apply(&tx([Edit::AppendData {
        id: s.x,
        array: NdArray::vector(vec![5.0]),
        retain: None,
    }]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues)) if has_issue(issues, IssueKind::ShapeMismatch)),
        "{result:?}"
    );
    assert_eq!(edited, s.fig);
}

// Why: a session sends an image's pixels as an array of bytes; `PutData` must store it
// with its element type, and its inverse must be the removal of a new array or the
// restoration of the array it replaced, exactly as for floats, so that undo brings
// back bytes and floats alike.
#[test]
fn put_data_stores_an_array_of_bytes_and_its_inverse_restores_what_was_there() {
    let s = streaming_figure();
    let id = DataId(100);
    let pixels = bytes(vec![2, 2], vec![0, 1, 254, 255]);
    let (with_bytes, inverse) = applied(
        &s.fig,
        &tx([Edit::PutData {
            id,
            array: pixels.clone(),
        }]),
    )
    .unwrap();
    assert_eq!(with_bytes.data[&id], pixels);
    assert_eq!(inverse.edits, [Edit::RemoveData { id }]);

    let float_array = NdArray::vector(vec![1.0, 2.0]);
    let (with_floats, inverse) = applied(
        &with_bytes,
        &tx([Edit::PutData {
            id,
            array: float_array.clone(),
        }]),
    )
    .unwrap();
    assert_eq!(with_floats.data[&id], float_array);
    assert_eq!(
        inverse.edits,
        [Edit::PutData {
            id,
            array: pixels.clone()
        }]
    );
    let (restored, _) = applied(&with_floats, &inverse).unwrap();
    assert_eq!(restored, with_bytes);
}

// Why: the artists that exist today read floats, so replacing the array of a line with
// bytes would leave the line undrawable; validation reports it, and the transaction
// must be refused and leave the figure unchanged.
#[test]
fn replacing_the_array_of_an_artist_with_bytes_is_refused() {
    let s = streaming_figure();
    let mut edited = s.fig.clone();
    let result = edited.apply(&tx([Edit::PutData {
        id: s.y,
        array: bytes(vec![5], vec![10, 11, 12, 13, 14]),
    }]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues)) if has_issue(issues, IssueKind::ElementTypeMismatch)),
        "{result:?}"
    );
    assert_eq!(edited, s.fig);
}

// Why: a stream of frames appends rows of bytes to an image exactly as it appends rows
// of floats to a field: along the first dimension, concatenated in row-major order, with
// `retain` keeping the last rows; and the inverse must restore the bytes exactly.
#[test]
fn appending_bytes_to_an_array_of_bytes_extends_it() {
    let s = streaming_figure();
    let id = DataId(100);
    let (fig, _) = applied(
        &s.fig,
        &tx([Edit::PutData {
            id,
            array: bytes(vec![2, 3], vec![0, 1, 2, 3, 4, 5]),
        }]),
    )
    .unwrap();

    let (edited, inverse) = applied(
        &fig,
        &tx([Edit::AppendData {
            id,
            array: bytes(vec![1, 3], vec![6, 7, 8]),
            retain: None,
        }]),
    )
    .unwrap();
    assert_eq!(edited.data[&id], bytes(vec![3, 3], (0..9).collect()));
    let (restored, _) = applied(&edited, &inverse).unwrap();
    assert_eq!(restored, fig);

    let (windowed, inverse) = applied(
        &fig,
        &tx([Edit::AppendData {
            id,
            array: bytes(vec![1, 3], vec![6, 7, 8]),
            retain: Some(2),
        }]),
    )
    .unwrap();
    assert_eq!(
        windowed.data[&id],
        bytes(vec![2, 3], vec![3, 4, 5, 6, 7, 8])
    );
    let (restored, _) = applied(&windowed, &inverse).unwrap();
    assert_eq!(restored, fig);
}

// Why: bytes appended to floats, or floats to bytes, could only be stored by converting
// them, which would change the data the sender meant; the append must be refused with
// an error that names the failing edit, the array and both element types (the trailing
// shapes agree, so the shape check must not be what catches it), and the figure must be
// left unchanged, which includes rolling back the edit that succeeded before it.
#[test]
fn appending_entries_of_another_element_type_fails() {
    let s = streaming_figure();
    let id = DataId(100);
    let (fig, _) = applied(
        &s.fig,
        &tx([Edit::PutData {
            id,
            array: bytes(vec![2], vec![1, 2]),
        }]),
    )
    .unwrap();
    let cases = [
        (
            s.x,
            bytes(vec![1], vec![5]),
            NdArrayElement::F64,
            NdArrayElement::U8,
        ),
        (
            id,
            NdArray::vector(vec![5.0]),
            NdArrayElement::U8,
            NdArrayElement::F64,
        ),
    ];
    for (target, array, existing, appended) in cases {
        let mut edited = fig.clone();
        let result = edited.apply(&tx([
            Edit::PutData {
                id: DataId(101),
                array: NdArray::vector(vec![1.0]),
            },
            Edit::AppendData {
                id: target,
                array,
                retain: None,
            },
        ]));
        assert!(
            matches!(
                &result,
                Err(EditError::ElementMismatch { edit: Some(1), id: i, existing: e, appended: a })
                    if *i == target && *e == existing && *a == appended
            ),
            "{result:?}"
        );
        assert_eq!(edited, fig);
    }
}

// ---------------------------------------------------------------------------------
// Atomicity and validation
// ---------------------------------------------------------------------------------

// Why: a transaction applies all of its edits or none, so a client never sees a figure
// that is half-way through a change; the error must identify the failing edit.
#[test]
fn a_failing_edit_leaves_the_figure_unchanged_and_is_identified_by_index() {
    let s = streaming_figure();
    let mut edits = vec![
        set(s.fig.id, "title", Value::Text(Text::new("changed"))),
        Edit::Insert {
            parent: s.fig.id,
            index: Some(0),
            node: axes_node(NodeId(100), vec![]),
        },
        Edit::PutData {
            id: DataId(100),
            array: NdArray::vector(vec![1.0]),
        },
        Edit::Remove { node: s.other_line },
        Edit::Move {
            node: s.line,
            parent: NodeId(100),
            index: None,
        },
    ];
    edits.extend(append_to_line(&s, &[5.0], &[15.0], Some(2)).edits);
    edits.push(set(NodeId(999), "title", Value::Unset));
    let failing = edits.len() - 1;

    let mut edited = s.fig.clone();
    let result = edited.apply(&tx(edits));
    assert!(
        matches!(result, Err(EditError::UnknownNode { edit: Some(i), node: NodeId(999) }) if i == failing),
        "{result:?}"
    );
    assert_eq!(edited, s.fig);
}

// Why: typed values can still describe an undrawable figure (reversed limits); validation
// after the edits must refuse the transaction and roll every edit back.
#[test]
fn a_transaction_that_introduces_a_validation_error_is_rolled_back() {
    let (fig, axes, _) = single_line_figure();
    let mut edited = fig.clone();
    let result = edited.apply(&tx([
        set(axes, "colormap", Value::ColormapName(ColormapName::Gray)),
        set(axes, "x.limits", Value::Limits(manual(2.0, 1.0))),
    ]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues)) if has_issue(issues, IssueKind::InvalidLimits)),
        "{result:?}"
    );
    assert_eq!(edited, fig);
}

// Why: a loaded figure may already contain errors; the user must still be able to edit it
// (for example to fix it), so only errors that the transaction introduces reject it.
#[test]
fn an_error_the_figure_already_has_does_not_prevent_editing() {
    let (mut fig, axes, _) = single_line_figure();
    fig.size.width_mm = 0.0;
    let (edited, _) = applied(
        &fig,
        &tx([set(
            axes,
            "colormap",
            Value::ColormapName(ColormapName::Gray),
        )]),
    )
    .unwrap();
    assert_eq!(find_axes(&edited, axes).colormap, ColormapName::Gray);

    let (fixed, _) = applied(
        &fig,
        &tx([set(fig.id, "size.width_mm", Value::Double(100.0))]),
    )
    .unwrap();
    assert!(fixed.validate().is_valid());
}

// Why: tolerance of existing errors must not become a loophole: a new error of a kind the
// figure already has elsewhere (invalid limits on another axes) is still new.
#[test]
fn a_new_error_of_a_kind_already_present_on_another_node_is_refused() {
    let (mut fig, ids) = row_of_axes(2);
    fig.axes[0].x.limits = manual(1.0, 1.0);
    let mut edited = fig.clone();
    let result = edited.apply(&tx([set(
        ids[1],
        "x.limits",
        Value::Limits(manual(3.0, 2.0)),
    )]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues)) if issues.iter().any(|i| i.kind == IssueKind::InvalidLimits && i.node == Some(ids[1]))),
        "{result:?}"
    );
    assert_eq!(edited, fig);
}

// Why: nor may it become a loophole on the same node: an axes whose x limits are already
// invalid must not also receive invalid y limits, while the user must still be able to
// change the invalid x limits to other values on the way to fixing them. Errors are
// therefore counted by node and kind rather than compared by message, which quotes values.
#[test]
fn a_second_error_of_the_same_kind_on_the_same_node_is_refused_but_changing_the_first_is_not() {
    let (mut fig, ids) = row_of_axes(1);
    fig.axes[0].x.limits = manual(1.0, 1.0);

    let mut edited = fig.clone();
    let result = edited.apply(&tx([set(
        ids[0],
        "y.limits",
        Value::Limits(manual(3.0, 2.0)),
    )]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues)) if issues.iter().any(|i| i.kind == IssueKind::InvalidLimits && i.node == Some(ids[0]))),
        "{result:?}"
    );
    assert_eq!(edited, fig);

    let (edited, _) = applied(
        &fig,
        &tx([set(ids[0], "x.limits", Value::Limits(manual(5.0, 4.0)))]),
    )
    .unwrap();
    assert_eq!(limits_of(&edited, ids[0], Dimension::X), manual(5.0, 4.0));
}

// Why: warnings describe data that is tolerated (non-positive values on a log axis are
// omitted), so switching an axis to a log scale over such data must be allowed.
#[test]
fn warnings_do_not_reject_a_transaction() {
    let s = streaming_figure();
    let (edited, _) = applied(
        &s.fig,
        &tx([set(s.axes, "x.scale", Value::Scale(Scale::Log))]),
    )
    .unwrap();
    assert!(!edited.validate().warnings.is_empty());
    assert_eq!(find_axes(&edited, s.axes).x.scale, Scale::Log);
}

// Why: an empty transaction (a gesture that changed nothing) must be harmless.
#[test]
fn an_empty_transaction_changes_nothing_and_has_an_empty_inverse() {
    let fig = kitchen_sink_figure();
    let (edited, inverse) = applied(&fig, &Transaction::default()).unwrap();
    assert_eq!(edited, fig);
    assert!(inverse.edits.is_empty());
}

// ---------------------------------------------------------------------------------
// Inverses
// ---------------------------------------------------------------------------------

/// Returns single edits of every kind on every node and array of a figure: a changed value
/// for every readable property, `Unset` for every present optional property, the removal,
/// reordering and duplication of every node, and the replacement, extension, truncation
/// and removal of every array (of floats or of bytes, each with entries of its own
/// element type). Some of them are invalid for the figure.
fn candidate_edits(fig: &Figure) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut fresh = nodes(fig).iter().map(|(id, _)| id.0).max().unwrap() + 1000;
    let mut fresh_id = || {
        fresh += 1;
        NodeId(fresh)
    };
    for (node, kind) in nodes(fig) {
        for property in properties(kind) {
            let Ok(value) = fig.get(node, &property.path) else {
                continue;
            };
            if value != Value::Unset {
                edits.push(Edit::Set {
                    node,
                    path: property.path.clone(),
                    value: perturb(&value),
                });
                if property.optional {
                    edits.push(Edit::Set {
                        node,
                        path: property.path.clone(),
                        value: Value::Unset,
                    });
                }
            }
        }
    }
    let first_axes = fig.axes[0].id;
    for axes in &fig.axes {
        edits.push(Edit::Remove { node: axes.id });
        edits.push(Edit::Move {
            node: axes.id,
            parent: fig.id,
            index: Some(0),
        });
        let mut copy = axes.clone();
        copy.id = fresh_id();
        for artist in &mut copy.artists {
            let id = fresh_id();
            set_artist_id(artist, id);
        }
        edits.push(Edit::Insert {
            parent: fig.id,
            index: None,
            node: Node::Axes(Box::new(copy)),
        });
        for artist in &axes.artists {
            edits.push(Edit::Remove { node: artist.id() });
            edits.push(Edit::Move {
                node: artist.id(),
                parent: axes.id,
                index: Some(0),
            });
            edits.push(Edit::Move {
                node: artist.id(),
                parent: first_axes,
                index: None,
            });
            let mut copy = artist.clone();
            set_artist_id(&mut copy, fresh_id());
            edits.push(Edit::Insert {
                parent: axes.id,
                index: Some(0),
                node: Node::Artist(copy),
            });
        }
    }
    let unused = DataId(fig.data.keys().map(|id| id.0).max().unwrap() + 1);
    edits.push(Edit::PutData {
        id: unused,
        array: NdArray::vector(vec![1.0, f64::NAN]),
    });
    for (&id, array) in &fig.data {
        let shifted = match &array.values {
            Values::F64(values) => NdArray::from_shape(
                array.shape.clone(),
                values.iter().map(|v| v + 1.0).collect(),
            ),
            Values::U8(values) => NdArray::from_shape_u8(
                array.shape.clone(),
                values.iter().map(|v| v.wrapping_add(1)).collect(),
            ),
        }
        .expect("the shape is unchanged");
        edits.push(Edit::PutData { id, array: shifted });
        let entry_shape: Vec<usize> = [1]
            .into_iter()
            .chain(array.shape[1..].iter().copied())
            .collect();
        let entry_len = array.shape[1..].iter().product::<usize>();
        let entry = match array.element() {
            NdArrayElement::F64 => NdArray::from_shape(entry_shape, vec![0.5; entry_len]),
            NdArrayElement::U8 => NdArray::from_shape_u8(entry_shape, vec![7; entry_len]),
        }
        .expect("the entry has the trailing shape of the array");
        edits.push(Edit::AppendData {
            id,
            array: entry.clone(),
            retain: None,
        });
        edits.push(Edit::AppendData {
            id,
            array: entry,
            retain: Some(1),
        });
        edits.push(Edit::RemoveData { id });
    }
    edits
}

fn set_artist_id(artist: &mut Artist, id: NodeId) {
    match artist {
        Artist::Line(a) => a.id = id,
        Artist::Scatter(a) => a.id = id,
        Artist::Contour(a) => a.id = id,
        Artist::Quiver(a) => a.id = id,
        Artist::Surface(a) => a.id = id,
        Artist::Image(a) => a.id = id,
        Artist::IndexedImage(a) => a.id = id,
        Artist::MappedImage(a) => a.id = id,
    }
}

/// Applies a transaction to a copy of a figure and checks the contract of the result: a
/// refused transaction leaves the figure equal to the original, and an applied one is
/// undone exactly by its inverse, whose own inverse redoes it. Returns whether the
/// transaction was applied.
fn check_undo(fig: &Figure, transaction: &Transaction) -> Result<bool, String> {
    let mut edited = fig.clone();
    match edited.apply(transaction) {
        Err(error) => {
            if edited != *fig {
                return Err(format!("refused with {error:?} but changed the figure"));
            }
            Ok(false)
        }
        Ok(inverse) => {
            let after = edited.clone();
            let redo = edited
                .apply(&inverse)
                .map_err(|e| format!("the inverse was refused: {e:?}"))?;
            // Figure equality treats -0.0 as 0.0 and every NaN as equal, so bits are
            // compared as well.
            if edited != *fig || float_bits(&edited) != float_bits(fig) {
                return Err("the inverse did not restore the figure".to_owned());
            }
            edited
                .apply(&redo)
                .map_err(|e| format!("the inverse of the inverse was refused: {e:?}"))?;
            if edited != after {
                return Err("the inverse of the inverse did not redo the transaction".to_owned());
            }
            Ok(true)
        }
    }
}

// Why: undo in the property editor and in sessions applies the inverse of each
// transaction, so for every kind of edit on every node and array, applying the inverse
// must restore the figure exactly, and a refused edit must change nothing.
#[test]
fn the_inverse_of_every_single_edit_restores_the_figure() {
    let fig = kitchen_sink_figure();
    let candidates = candidate_edits(&fig);
    let mut applied_count = 0;
    for edit in &candidates {
        match check_undo(&fig, &tx([edit.clone()])) {
            Ok(was_applied) => applied_count += usize::from(was_applied),
            Err(problem) => panic!("{edit:?}: {problem}"),
        }
    }
    // Most candidates are valid; if nearly all were refused the test would prove little.
    assert!(
        applied_count * 2 >= candidates.len(),
        "only {applied_count} of {} candidate edits were applied",
        candidates.len()
    );
}

// Why: the inverse must undo edits in reverse order, or a transaction that sets the same
// property twice, or creates a value and then changes it, would be "undone" to its
// intermediate state.
#[test]
fn the_inverse_undoes_edits_in_reverse_order() {
    let (fig, axes, line_id) = single_line_figure();
    let transaction = tx([
        set(axes, "x.limits", Value::Limits(manual(1.0, 2.0))),
        set(axes, "x.limits", Value::Limits(manual(3.0, 4.0))),
        set(fig.id, "title", Value::Text(Text::new("a"))),
        set(fig.id, "title.content", Value::String("b".to_owned())),
        Edit::Move {
            node: line_id,
            parent: axes,
            index: None,
        },
        Edit::Remove { node: line_id },
    ]);
    let (edited, inverse) = applied(&fig, &transaction).unwrap();
    let (restored, _) = applied(&edited, &inverse).unwrap();
    assert_eq!(restored, fig);
}

// Why: the inverses of edits on the same node or array interact: undoing a move must
// return an artist to its original axes and position before the inverse of an earlier set
// addresses it, and undoing the removal of an array must restore the entries appended to
// it in the same transaction. The random transactions below rarely pick such
// combinations from hundreds of candidates, so they are checked here explicitly.
#[test]
fn inverses_of_interacting_edits_on_the_same_node_or_array_restore_the_figure() {
    let (fig, axes, [a, b, c], empty) = three_lines_figure();
    let transactions = [
        // A move across axes from the middle of a list, undone to the middle again.
        tx([Edit::Move {
            node: b,
            parent: empty,
            index: None,
        }]),
        tx([
            set(b, "line.width_pt", Value::Double(3.0)),
            Edit::Move {
                node: b,
                parent: empty,
                index: Some(0),
            },
            set(b, "visible", Value::Bool(false)),
            Edit::Move {
                node: c,
                parent: axes,
                index: Some(0),
            },
            Edit::Remove { node: b },
        ]),
        tx([
            Edit::Move {
                node: a,
                parent: empty,
                index: None,
            },
            Edit::Remove { node: axes },
        ]),
        tx([
            Edit::PutData {
                id: DataId(100),
                array: NdArray::vector(vec![1.0, f64::NAN]),
            },
            Edit::AppendData {
                id: DataId(100),
                array: NdArray::vector(vec![-0.0, 2.0]),
                retain: None,
            },
            Edit::AppendData {
                id: DataId(100),
                array: NdArray::vector(vec![3.0]),
                retain: Some(2),
            },
            Edit::RemoveData { id: DataId(100) },
        ]),
    ];
    for transaction in &transactions {
        match check_undo(&fig, transaction) {
            Ok(true) => {}
            Ok(false) => panic!("{transaction:?} was refused"),
            Err(problem) => panic!("{transaction:?}: {problem}"),
        }
    }
}

/// The kitchen-sink figure with the candidate edits of [`candidate_edits`], computed once.
static CANDIDATES: LazyLock<(Figure, Vec<Edit>)> = LazyLock::new(|| {
    let fig = kitchen_sink_figure();
    let edits = candidate_edits(&fig);
    (fig, edits)
});

proptest! {
    // Each case applies at most three transactions to a copy of the kitchen-sink figure, so
    // the default number of cases runs in well under a second; it is set explicitly so that
    // the run time does not change with the proptest defaults or environment.
    #![proptest_config(ProptestConfig::with_cases(256))]

    // Why: real transactions mix kinds of edit whose inverses interact (a set on a node
    // that a later edit moves or removes, an append to an array that is then replaced);
    // any such transaction must be undone exactly by its inverse, or refused without
    // change.
    #[test]
    fn any_transaction_of_candidate_edits_is_undone_by_its_inverse(
        picks in prop::collection::vec(any::<prop::sample::Index>(), 1..6)
    ) {
        let (fig, candidates) = &*CANDIDATES;
        let transaction = tx(picks.iter().map(|pick| pick.get(candidates).clone()));
        if let Err(problem) = check_undo(fig, &transaction) {
            prop_assert!(false, "{:?}: {}", transaction, problem);
        }
    }
}

/// A 3D figure whose view angles, line width, placement height and data hold special
/// floating-point values that validation does not reject. Returns the figure, the axes,
/// the line and its x data.
fn special_floats_figure() -> (Figure, NodeId, NodeId, DataId) {
    let mut b = FigureBuilder::new();
    let axes = b.axes3d(0, 0);
    b.axes(axes).projection = Projection::ThreeD {
        view3d: View3d {
            azimuth_deg: SPECIAL_F64[2],
            elevation_deg: -0.0,
            zoom: SPECIAL_F64[1],
            pan_x: SPECIAL_F64[8],
            pan_y: SPECIAL_F64[3],
        },
    };
    let x = b.vector(&SPECIAL_F64);
    let y = b.vector(&SPECIAL_F64);
    let id = b.node();
    b.push(
        axes,
        Artist::Line(Line {
            id,
            x,
            y,
            line: LineStyle {
                width_pt: SPECIAL_F64[0],
                ..LineStyle::default()
            },
            ..Line::default()
        }),
    );
    (b.build(), axes, id, x)
}

// Why: undo must restore a figure bit for bit, including NaN payloads and negative zero,
// which figure equality does not distinguish; so must reading a value back after setting
// it, because the viewer writes such values while dragging.
#[test]
fn negative_zero_and_nan_survive_set_get_and_inverse_bit_for_bit() {
    let (fig, axes, line_id, x) = special_floats_figure();
    let before = float_bits(&fig);

    let mut edited = fig.clone();
    let inverse = edited
        .apply(&tx([
            set(axes, "projection.view3d.azimuth_deg", Value::Double(1.0)),
            set(axes, "projection.view3d.elevation_deg", Value::Double(2.0)),
            set(axes, "projection.view3d.zoom", Value::Double(3.0)),
            set(line_id, "line.width_pt", Value::Double(4.0)),
            Edit::PutData {
                id: x,
                array: NdArray::vector(vec![0.0; SPECIAL_F64.len()]),
            },
        ]))
        .unwrap();
    edited.apply(&inverse).unwrap();
    assert_eq!(float_bits(&edited), before);

    let mut edited = fig.clone();
    let inverse = edited
        .apply(&tx([
            set(axes, "projection.view3d.pan_x", Value::Double(-0.0)),
            set(line_id, "line.width_pt", Value::Double(SPECIAL_F64[3])),
        ]))
        .unwrap();
    let Value::Double(pan_x) = edited.get(axes, &path("projection.view3d.pan_x")).unwrap() else {
        panic!("pan_x is a double");
    };
    assert_eq!(pan_x.to_bits(), (-0.0f64).to_bits());
    let Value::Double(width) = edited.get(line_id, &path("line.width_pt")).unwrap() else {
        panic!("the width is a double");
    };
    assert_eq!(width.to_bits(), SPECIAL_F64[3].to_bits());
    let Value::Projection(Projection::ThreeD { view3d }) =
        edited.get(axes, &path("projection")).unwrap()
    else {
        panic!("the projection is 3D");
    };
    assert_eq!(view3d.azimuth_deg.to_bits(), SPECIAL_F64[2].to_bits());
    edited.apply(&inverse).unwrap();
    assert_eq!(float_bits(&edited), before);
}

// Why: undoing a rolling-window append must bring back the entries that `retain`
// discarded, not only remove the appended ones.
#[test]
fn the_inverse_of_an_append_with_retain_restores_the_discarded_entries() {
    let s = streaming_figure();
    let (edited, inverse) = applied(
        &s.fig,
        &append_to_line(&s, &[5.0, 6.0], &[15.0, 16.0], Some(3)),
    )
    .unwrap();
    assert_eq!(edited.data[&s.x].shape, vec![3]);
    let (restored, _) = applied(&edited, &inverse).unwrap();
    assert_eq!(restored, s.fig);
}
