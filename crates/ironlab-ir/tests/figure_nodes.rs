//! Figure nodes in one identifier space: data arrays and panels as nodes of the figure
//! beside its axes, the references between nodes, and the structural and data edits that
//! create, change and delete them.
//!
//! A data array is a node, so that artists in any axes share one array by naming its
//! identifier and the array is stored once (issue #26). A panel is a node that occupies
//! cells of the tile layout and holds one key, which in this model is a legend.

mod common;

use std::collections::BTreeSet;

use common::edits::{applied, data_node, line, set, tx};
use common::{
    FigureBuilder, array, data_references, data_references_mut, kitchen_sink_figure, legend_panel,
    raw_fields, single_line_figure,
};
use ironlab_ir::*;
use serde_json::{Value as JsonValue, json};

/// A figure of two 2D axes side by side in a 1 × 3 layout, whose lines share one x and
/// one y array, with a legend panel in the third cell that both axes name. Each line has a
/// label, so that the panel lists two entries.
struct Shared {
    fig: Figure,
    axes: [NodeId; 2],
    lines: [NodeId; 2],
    x: NodeId,
    y: NodeId,
    panel: NodeId,
}

fn shared_figure() -> Shared {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(1, 3);
    let x = b.vector(&[1.0, 2.0, 3.0, 4.0]);
    b.data_array(x).name = Some("time (s)".to_owned());
    let y = b.vector(&[1.0, 4.0, 9.0, 16.0]);
    let panel = b.node();
    let axes = [b.axes2d(0, 0), b.axes2d(0, 1)];
    let lines = [b.node(), b.node()];
    for k in 0..2 {
        b.axes(axes[k]).legend = Some(Legend::Panel(panel));
        b.push(
            axes[k],
            Artist::Line(Line {
                id: lines[k],
                label: Some(Text::plain(format!("run {k}"))),
                x,
                y,
                ..Line::default()
            }),
        );
    }
    b.fig.panels.push(legend_panel(
        panel,
        Cell {
            col: 2,
            ..Cell::default()
        },
    ));
    Shared {
        fig: b.build(),
        axes,
        lines,
        x,
        y,
        panel,
    }
}

/// Returns the kinds of the issues of a list, in the order reported.
fn kinds(issues: &[ValidationIssue]) -> Vec<IssueKind> {
    issues.iter().map(|issue| issue.kind).collect()
}

/// Returns whether a list of issues holds one of the given kind against the given node.
fn has(issues: &[ValidationIssue], kind: IssueKind, node: NodeId) -> bool {
    issues
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

/// Returns the line of the given identifier, found without the API under test.
fn line_of(fig: &Figure, id: NodeId) -> &Line {
    fig.axes
        .iter()
        .flat_map(|axes| &axes.artists)
        .find_map(|artist| match artist {
            Artist::Line(line) if line.id == id => Some(line),
            _ => None,
        })
        .expect("the fixture line exists")
}

/// Returns the line of the given identifier for modification, found without the API under
/// test.
fn line_mut(fig: &mut Figure, id: NodeId) -> &mut Line {
    fig.axes
        .iter_mut()
        .flat_map(|axes| &mut axes.artists)
        .find_map(|artist| match artist {
            Artist::Line(line) if line.id == id => Some(line),
            _ => None,
        })
        .expect("the fixture line exists")
}

// ---------------------------------------------------------------------------------
// Sharing: an array stored once
// ---------------------------------------------------------------------------------

// Why: this is the point of issue #26. Two artists in two axes that plot the same record
// must store it once, in both formats, and must still refer to one array after a round
// trip; otherwise a figure of many views of one long record grows with every view.
#[test]
fn an_array_shared_by_artists_of_two_axes_is_stored_once_in_both_formats() {
    let s = shared_figure();
    assert_eq!(s.fig.validate().errors, vec![]);

    let value: JsonValue = serde_json::from_str(&s.fig.to_json()).unwrap();
    let ids: Vec<&JsonValue> = value["data"]
        .as_array()
        .expect("the data arrays are a list")
        .iter()
        .map(|data| &data["id"])
        .collect();
    assert_eq!(ids, [&json!(s.x.0), &json!(s.y.0)]);
    for k in 0..2 {
        let artist = &value["axes"][k]["artists"][0];
        assert_eq!(artist["x"], json!(s.x.0));
        assert_eq!(artist["y"], json!(s.y.0));
    }

    let bytes = s.fig.to_protobuf();
    assert_eq!(
        raw_fields(&bytes, 9).len(),
        2,
        "Figure.data holds two arrays"
    );

    for restored in [
        Figure::from_json(&s.fig.to_json()).unwrap(),
        Figure::from_protobuf(&bytes).unwrap(),
    ] {
        assert_eq!(restored, s.fig);
        assert_eq!(restored.data.len(), 2);
        let [first, second] = s.lines.map(|line| line_of(&restored, line).clone());
        assert_eq!((first.x, first.y), (second.x, second.y));
    }
}

// Why: the saving that sharing promises must be real in the binary format: an array of
// ten thousand values referred to by two artists costs its values once, so the figure is
// smaller than the same figure with a copy of the array for each artist by about the
// size of the copy (eight bytes per value).
#[test]
fn sharing_an_array_saves_its_size_in_the_binary_format() {
    let n = 10_000;
    let values: Vec<f64> = (0..n).map(|k| k as f64 * 0.5).collect();
    let build = |share: bool| {
        let mut b = FigureBuilder::new();
        b.fig.layout = TileLayout::new(1, 2);
        let x = b.vector(&values);
        let x_again = if share { x } else { b.vector(&values) };
        for (col, x) in [(0, x), (1, x_again)] {
            let axes = b.axes2d(0, col);
            let id = b.node();
            b.push(axes, line(id, x, x));
        }
        b.build()
    };
    let (shared, copied) = (build(true).to_protobuf(), build(false).to_protobuf());
    let saved = copied.len() - shared.len();
    assert!(
        saved >= 8 * n && saved < 8 * n + 64,
        "sharing saved {saved} bytes of a copy of {} bytes",
        8 * n
    );
}

// ---------------------------------------------------------------------------------
// One identifier space
// ---------------------------------------------------------------------------------

// Why: every kind of node shares one identifier space, so allocation must avoid the
// identifier of a node of every kind; whichever kind holds the largest identifier, a new
// identifier must lie above it, or a new node could take the identifier that references
// to an array, a panel, an axes or an artist name.
#[test]
fn allocation_lies_above_the_largest_identifier_of_any_kind_of_node() {
    let base = kitchen_sink_figure();
    let largest = NodeId(1 << 40);
    type GiveId = fn(&mut Figure, NodeId);
    let kinds: [(&str, GiveId); 4] = [
        ("a data array", |fig, id| fig.data[1].id = id),
        ("an axes", |fig, id| fig.axes[1].id = id),
        ("an artist", |fig, id| match &mut fig.axes[0].artists[0] {
            Artist::Line(line) => line.id = id,
            _ => unreachable!("the kitchen-sink figure's first artist is a line"),
        }),
        ("a panel", |fig, id| fig.panels[1].id = id),
    ];
    for (name, give) in kinds {
        let mut fig = base.clone();
        give(&mut fig, largest);
        for mut loaded in [
            Figure::from_json(&fig.to_json()).unwrap(),
            Figure::from_protobuf(&fig.to_protobuf()).unwrap(),
        ] {
            let first = loaded.alloc_node_id();
            let second = loaded.alloc_node_id();
            assert!(first > largest, "{name}: allocated {first}");
            assert!(second > first, "{name}: allocated {second} after {first}");
        }
    }
}

// Why: a reference names a node by identifier alone, so two nodes of different kinds
// with one identifier make every reference to it ambiguous: an artist naming the
// identifier as its data could mean the array or the artist. Validation must report the
// clash whichever kinds share the identifier, data arrays included.
#[test]
fn a_data_array_sharing_an_identifier_with_another_node_is_a_duplicate_node_id() {
    let s = shared_figure();
    let others = [
        ("an artist", s.lines[0]),
        ("an axes", s.axes[1]),
        ("a panel", s.panel),
        ("the figure", s.fig.id),
        ("another data array", s.y),
    ];
    for (name, other) in others {
        let mut fig = s.fig.clone();
        fig.data[0].id = other;
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::DuplicateNodeId, other),
            "{name}: {report:?}"
        );
    }
}

// Why: a client asks what a node is before it shows or edits it, so the kind of every
// node must be known by its identifier, data arrays and panels included.
#[test]
fn the_kind_of_a_data_array_and_of_a_panel_is_known_by_identifier() {
    let s = shared_figure();
    assert_eq!(s.fig.node_kind(s.x), Some(NodeKind::DataArray));
    assert_eq!(s.fig.node_kind(s.panel), Some(NodeKind::Panel));
    assert_eq!(s.fig.node_kind(s.axes[0]), Some(NodeKind::Axes));
    assert_eq!(s.fig.node_kind(s.lines[0]), Some(NodeKind::Line));
    assert_eq!(s.fig.node_kind(NodeId(999)), None);
}

// ---------------------------------------------------------------------------------
// Structural edits of data arrays
// ---------------------------------------------------------------------------------

// Why: a session adds a plot of new data in one step: it inserts the array under the
// figure and then an artist that names it, in one transaction, so that no mirror ever
// sees an artist without data or an array that nothing uses. The inverse must remove
// both and restore the figure exactly.
#[test]
fn a_data_array_and_an_artist_that_refers_to_it_are_inserted_in_one_transaction() {
    let (fig, axes, _) = single_line_figure();
    let (data, artist) = (NodeId(500), NodeId(501));
    let insert = tx([
        Edit::Insert {
            parent: fig.id,
            index: Some(0),
            node: Node::DataArray(DataArray {
                id: data,
                name: Some("pressure (Pa)".to_owned()),
                array: NdArray::vector(vec![5.0, 6.0, 7.0]),
            }),
        },
        Edit::Insert {
            parent: axes,
            index: None,
            node: Node::Artist(line(artist, data, data)),
        },
    ]);
    let (edited, inverse) = applied(&fig, &insert).unwrap();
    assert_eq!(edited.data[0].id, data, "inserted at the index given");
    assert_eq!(edited.data[1..], fig.data[..]);
    assert_eq!(line_of(&edited, artist).y, data);
    let report = edited.validate();
    assert_eq!(report.errors, vec![]);
    assert!(
        !has(&report.warnings, IssueKind::UnusedData, data),
        "{report:?}"
    );
    assert_eq!(applied(&edited, &inverse).unwrap().0, fig);

    // Validation runs after the whole transaction, so the artist may come first.
    let reversed = tx(insert.edits.iter().rev().cloned());
    let (edited_in_reverse, _) = applied(&fig, &reversed).unwrap();
    assert_eq!(edited_in_reverse, edited);
}

// Why: removing an array that an artist still names would leave the artist with a
// dangling reference, which validation refuses, so the removal is rolled back and the
// figure left as it was; removing the artist and its array together is a valid change,
// and its inverse must restore both where they were.
#[test]
fn removing_a_data_array_that_an_artist_refers_to_is_refused_unless_the_artist_goes_too() {
    let (fig, _, line_id) = single_line_figure();
    let y = line_of(&fig, line_id).y;

    let mut edited = fig.clone();
    let result = edited.apply(&tx([Edit::Remove { node: y }]));
    let Err(EditError::Invalid(issues)) = &result else {
        panic!("the removal was not refused as invalid: {result:?}");
    };
    assert!(
        has(issues, IssueKind::UnknownData, line_id),
        "the dangling reference is reported against the artist: {issues:?}"
    );
    assert_eq!(edited, fig, "a refused transaction changes nothing");

    let both = tx([Edit::Remove { node: line_id }, Edit::Remove { node: y }]);
    let (removed, inverse) = applied(&fig, &both).unwrap();
    assert!(removed.data.iter().all(|data| data.id != y));
    assert!(removed.artist(line_id).is_none());
    assert_eq!(applied(&removed, &inverse).unwrap().0, fig);
}

// Why: undo of a removal must bring the array back exactly as it was, with its
// identifier, its name and its values, at its old position among the figure's data, so
// that the file after undo is the file before the removal.
#[test]
fn the_inverse_of_removing_a_data_array_restores_it_at_its_position_with_its_name() {
    let mut fig = shared_figure().fig;
    let spare = DataArray {
        id: NodeId(900),
        name: Some("spare, ω".to_owned()),
        array: NdArray::vector(vec![f64::NAN, -0.0, 1.5]),
    };
    fig.data.insert(1, spare.clone());

    let (removed, inverse) = applied(&fig, &tx([Edit::Remove { node: spare.id }])).unwrap();
    assert_eq!(removed.data.len(), 2);
    assert_eq!(
        inverse.edits,
        [Edit::Insert {
            parent: fig.id,
            index: Some(1),
            node: Node::DataArray(spare),
        }]
    );
    assert_eq!(applied(&removed, &inverse).unwrap().0, fig);
}

// Why: a data array belongs to the figure like an axes and a panel, so it is reordered
// with `Move` under the figure like them; the order of the data is part of the file, so
// the inverse must restore it. An axes cannot hold an array, so a move into one is
// refused.
#[test]
fn a_data_array_is_reordered_under_the_figure_and_cannot_move_into_an_axes() {
    let s = shared_figure();
    let (moved, inverse) = applied(
        &s.fig,
        &tx([Edit::Move {
            node: s.y,
            parent: s.fig.id,
            index: Some(0),
        }]),
    )
    .unwrap();
    assert_eq!(
        moved.data.iter().map(|d| d.id).collect::<Vec<_>>(),
        [s.y, s.x]
    );
    assert_eq!(applied(&moved, &inverse).unwrap().0, s.fig);

    let result = applied(
        &s.fig,
        &tx([Edit::Move {
            node: s.y,
            parent: s.axes[0],
            index: None,
        }]),
    );
    assert!(
        matches!(result, Err(EditError::InvalidParent { edit: Some(0), node, parent })
            if node == s.y && parent == s.axes[0]),
        "{result:?}"
    );
}

// ---------------------------------------------------------------------------------
// Data edits on shared arrays
// ---------------------------------------------------------------------------------

// Why: an edit of data addresses the array, not an artist, so replacing an array that
// two artists share changes what both draw; this is deliberate, because the array is one
// record shown twice. The array keeps its identifier and its name, and the inverse
// restores the old values.
#[test]
fn replacing_a_shared_array_changes_every_artist_that_refers_to_it() {
    let s = shared_figure();
    let new = NdArray::vector(vec![2.0, 3.0, 5.0, 7.0]);
    let (edited, inverse) = applied(
        &s.fig,
        &tx([Edit::ReplaceData {
            node: s.y,
            array: new.clone(),
        }]),
    )
    .unwrap();
    for line in s.lines {
        assert_eq!(array(&edited, line_of(&edited, line).y), &new);
    }
    assert_eq!(edited.data.len(), s.fig.data.len());
    assert_eq!(
        edited.data_array(s.x).unwrap().name.as_deref(),
        Some("time (s)")
    );
    assert_eq!(
        inverse.edits,
        [Edit::ReplaceData {
            node: s.y,
            array: array(&s.fig, s.y).clone(),
        }]
    );
    assert_eq!(applied(&edited, &inverse).unwrap().0, s.fig);
}

// Why: because a replacement reaches every artist that shares the array, validation must
// judge it against all of them: an array whose length no longer matches the other
// coordinate of either line is refused, and the error names each line it breaks.
#[test]
fn replacing_a_shared_array_with_one_of_another_length_is_refused_for_every_artist() {
    let s = shared_figure();
    let mut edited = s.fig.clone();
    let result = edited.apply(&tx([Edit::ReplaceData {
        node: s.y,
        array: NdArray::vector(vec![1.0, 2.0]),
    }]));
    let Err(EditError::Invalid(issues)) = &result else {
        panic!("the replacement was not refused as invalid: {result:?}");
    };
    for line in s.lines {
        assert!(has(issues, IssueKind::ShapeMismatch, line), "{issues:?}");
    }
    assert_eq!(edited, s.fig);
}

// Why: a stream appends to the arrays it names by node identifier, with a rolling window,
// and every artist that shares the arrays scrolls with them; the inverse is a replacement
// of each array by its old values, which brings back the samples the window discarded.
#[test]
fn appending_with_retain_to_shared_arrays_scrolls_every_artist_that_refers_to_them() {
    let s = shared_figure();
    let append = tx([
        Edit::AppendData {
            node: s.x,
            array: NdArray::vector(vec![5.0, 6.0]),
            retain: Some(3),
        },
        Edit::AppendData {
            node: s.y,
            array: NdArray::vector(vec![25.0, 36.0]),
            retain: Some(3),
        },
    ]);
    let (edited, inverse) = applied(&s.fig, &append).unwrap();
    for line in s.lines {
        let line = line_of(&edited, line);
        assert_eq!(
            array(&edited, line.x),
            &NdArray::vector(vec![4.0, 5.0, 6.0])
        );
        assert_eq!(
            array(&edited, line.y),
            &NdArray::vector(vec![16.0, 25.0, 36.0])
        );
    }
    assert_eq!(
        inverse.edits,
        [
            Edit::ReplaceData {
                node: s.y,
                array: array(&s.fig, s.y).clone(),
            },
            Edit::ReplaceData {
                node: s.x,
                array: array(&s.fig, s.x).clone(),
            },
        ]
    );
    assert_eq!(applied(&edited, &inverse).unwrap().0, s.fig);
}

// ---------------------------------------------------------------------------------
// References to a node of the wrong kind
// ---------------------------------------------------------------------------------

// Why: in one identifier space a reference can name a node that exists but is of the
// wrong kind, such as an artist whose y names an axes. That is a different mistake from
// naming nothing, and the user fixes it differently, so it is reported as its own kind
// against the artist, and not also as unknown data; a reference that names nothing is
// still unknown data.
#[test]
fn an_artist_naming_a_node_that_is_not_a_data_array_is_wrong_node_kind() {
    let s = shared_figure();
    let targets = [
        ("an axes", s.axes[1]),
        ("a panel", s.panel),
        ("another artist", s.lines[1]),
        ("the figure", s.fig.id),
    ];
    for (name, target) in targets {
        let mut fig = s.fig.clone();
        line_mut(&mut fig, s.lines[0]).y = target;
        let report = fig.validate();
        assert_eq!(
            kinds(&report.errors),
            [IssueKind::WrongNodeKind],
            "{name}: {report:?}"
        );
        assert!(has(&report.errors, IssueKind::WrongNodeKind, s.lines[0]));
    }

    let mut fig = s.fig.clone();
    line_mut(&mut fig, s.lines[0]).y = NodeId(999);
    assert_eq!(kinds(&fig.validate().errors), [IssueKind::UnknownData]);
}

// Why: every data reference of every artist is a node identifier, the optional ones and
// those inside a tagged value included, so each must be checked for its kind; a check
// written only for the coordinates of a line would let a scatter coloured by an axes
// through.
#[test]
fn every_kind_of_data_reference_is_checked_for_its_kind() {
    let mut b = FigureBuilder::new();
    let axes = b.axes3d(0, 0);
    let (x, y) = (b.vector(&[1.0, 2.0]), b.vector(&[3.0, 4.0]));
    let scatter = b.node();
    b.push(
        axes,
        Artist::Scatter(Scatter {
            id: scatter,
            x,
            y,
            z: Some(axes),
            color: ScatterColor::Data { data: axes },
            ..Scatter::default()
        }),
    );
    let report = b.build().validate();
    assert!(
        has(&report.errors, IssueKind::WrongNodeKind, scatter),
        "{report:?}"
    );
    assert!(
        !kinds(&report.errors).contains(&IssueKind::UnknownData),
        "{report:?}"
    );
}

// Why: the check above covers two fields of one artist; the check of kind must reach every
// data reference of every artist, a surface's colour and alpha arrays, an image's alpha,
// a curvilinear grid and a quiver's optional w included. The kitchen-sink figure uses every
// reference field, so retargeting each of its references in turn at an axes must be
// reported as the wrong kind against that artist, and never as unknown data.
#[test]
fn every_data_reference_of_the_kitchen_sink_figure_is_checked_for_its_kind() {
    let base = kitchen_sink_figure();
    let wrong = base.axes[0].id;
    let mut checked = 0;
    for a in 0..base.axes.len() {
        for i in 0..base.axes[a].artists.len() {
            let count = data_references(&base.axes[a].artists[i]).len();
            for k in 0..count {
                let mut fig = base.clone();
                let artist = &mut fig.axes[a].artists[i];
                let id = artist.id();
                *data_references_mut(artist).remove(k) = wrong;
                let report = fig.validate();
                assert!(
                    has(&report.errors, IssueKind::WrongNodeKind, id),
                    "reference {k} of {id}: {report:?}"
                );
                assert!(
                    !kinds(&report.errors).contains(&IssueKind::UnknownData),
                    "reference {k} of {id}: {report:?}"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
}

// Why: a property editor may set a data reference to any identifier, so an edit that
// points an artist at a node of the wrong kind must be refused by validation like any
// other edit that makes the figure invalid, and leave the figure unchanged.
#[test]
fn an_edit_pointing_an_artist_at_an_axes_is_refused_as_wrong_node_kind() {
    let s = shared_figure();
    let mut edited = s.fig.clone();
    let result = edited.apply(&tx([set(s.lines[0], "y", Value::NodeId(s.axes[0]))]));
    let Err(EditError::Invalid(issues)) = &result else {
        panic!("the edit was not refused as invalid: {result:?}");
    };
    assert!(
        has(issues, IssueKind::WrongNodeKind, s.lines[0]),
        "{issues:?}"
    );
    assert_eq!(edited, s.fig);
}

// Why: a legend lists its entries in the panel it names, which must be a panel holding a
// legend; naming a data array or an axes is a reference to the wrong kind of node,
// reported against the axes whose legend it is, through validation and through an edit.
#[test]
fn a_legend_naming_a_data_array_or_an_axes_as_its_panel_is_wrong_node_kind() {
    let s = shared_figure();
    for (name, target) in [("a data array", s.x), ("an axes", s.axes[1])] {
        let mut fig = s.fig.clone();
        fig.axes[0].legend = Some(Legend::Panel(target));
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::WrongNodeKind, s.axes[0]),
            "{name}: {report:?}"
        );
        assert!(
            !kinds(&report.errors).contains(&IssueKind::DanglingPanelLegend),
            "{name}: {report:?}"
        );

        let mut edited = s.fig.clone();
        let result = edited.apply(&tx([set(
            s.axes[0],
            "legend",
            Value::Legend(Legend::Panel(target)),
        )]));
        assert!(
            matches!(&result, Err(EditError::Invalid(issues))
                if has(issues, IssueKind::WrongNodeKind, s.axes[0])),
            "{name}: {result:?}"
        );
        assert_eq!(edited, s.fig);
    }
}

// ---------------------------------------------------------------------------------
// Unused data
// ---------------------------------------------------------------------------------

// Why: an array that no artist refers to costs space in every saved file and is usually
// left behind by mistake, but it is also the normal state of data inserted before its
// artist, so it is a warning against the array and never an error; and it must stop being
// reported as soon as any artist refers to it, through any of its references.
#[test]
fn an_unreferenced_data_array_is_a_warning_until_an_artist_refers_to_it() {
    let (mut fig, _, _) = single_line_figure();
    let spare = NodeId(700);
    fig.data.push(DataArray {
        id: spare,
        name: None,
        array: NdArray::vector(vec![0.25, 0.5, 0.75]),
    });
    let report = fig.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(
        report
            .warnings
            .iter()
            .map(|w| (w.kind, w.node))
            .collect::<Vec<_>>(),
        [(IssueKind::UnusedData, Some(spare))]
    );

    // A scatter coloured by the array uses it, although it plots nothing along an axis.
    // The colour data is mapped through a scale, as colour data must be.
    let mut used = fig.clone();
    let line = line_of(&fig, fig.axes[0].artists[0].id()).clone();
    used.color_scales.push(ColorScale {
        id: NodeId(702),
        ..ColorScale::default()
    });
    used.axes[0].artists.push(Artist::Scatter(Scatter {
        id: NodeId(701),
        x: line.x,
        y: line.y,
        color: ScatterColor::Data { data: spare },
        color_scale: Some(NodeId(702)),
        ..Scatter::default()
    }));
    let report = used.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![], "a used array is not reported");

    // A hidden artist still refers to its data, which is kept for when it is shown.
    used.axes[0].artists[1].set_visible(false);
    assert_eq!(used.validate().warnings, vec![]);
}

// Why: warnings never reject a transaction, so inserting an array on its own, ahead of
// the artist that will use it, must be accepted even though the array is unused for now.
#[test]
fn inserting_an_unused_data_array_is_accepted_with_a_warning() {
    let (fig, _, _) = single_line_figure();
    let (edited, _) = applied(
        &fig,
        &tx([Edit::Insert {
            parent: fig.id,
            index: None,
            node: data_node(NodeId(700), NdArray::vector(vec![1.0])),
        }]),
    )
    .unwrap();
    assert!(has(
        &edited.validate().warnings,
        IssueKind::UnusedData,
        NodeId(700)
    ));
}

// Why: the kitchen-sink figure refers to every array it holds, through every kind of
// reference, so the hand-written list of references in the tests must account for every
// array, which keeps that list honest for the tests that rely on it.
#[test]
fn every_array_of_the_kitchen_sink_figure_is_referred_to_by_some_artist() {
    let fig = kitchen_sink_figure();
    let used: BTreeSet<NodeId> = fig
        .axes
        .iter()
        .flat_map(|axes| axes.artists.iter().flat_map(data_references))
        .collect();
    let held: BTreeSet<NodeId> = fig.data.iter().map(|data| data.id).collect();
    assert_eq!(used, held);
    assert!(
        !fig.validate()
            .warnings
            .iter()
            .any(|w| w.kind == IssueKind::UnusedData)
    );
}

// ---------------------------------------------------------------------------------
// Panels
// ---------------------------------------------------------------------------------

// Why: a panel with legend content is what an axes' legend names when its entries are
// listed outside its plot area; the figure is valid, and the panel has entries to draw,
// so it raises no warning either.
#[test]
fn a_panel_holding_a_legend_is_named_by_the_legends_of_axes() {
    let s = shared_figure();
    let report = s.fig.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![]);
    assert_eq!(s.fig.panel(s.panel).map(|p| p.id), Some(s.panel));
}

// Why: a panel occupies cells of the tile layout as an axes does, so the checks of a cell
// apply to it whatever its content: a cell outside the layout is an error against the
// panel, and a cell that overlaps an axes is a warning against the panel, which is drawn
// after every axes.
#[test]
fn the_cell_checks_of_the_tile_layout_apply_to_panels() {
    let s = shared_figure();

    let mut fig = s.fig.clone();
    fig.panels[0].cell.col = 3;
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::CellOutOfLayout, s.panel),
        "{report:?}"
    );

    let mut fig = s.fig.clone();
    fig.panels[0].cell.col = 1;
    let report = fig.validate();
    assert_eq!(report.errors, vec![]);
    assert!(
        has(&report.warnings, IssueKind::OverlappingCells, s.panel),
        "{report:?}"
    );
}

// Why: a fitted row or column takes its size from the panels in it, so a panel may sit
// in a fitted column alone, as an axes may not; the rule that an axes in fitted tracks
// alone is an error must keep counting panels as what gives a fitted track its size.
#[test]
fn a_panel_alone_in_a_fitted_column_is_valid_and_an_axes_there_is_not() {
    let s = shared_figure();
    let mut fig = s.fig.clone();
    fig.layout.col_weights = vec![
        TrackSize::Weight(1.0),
        TrackSize::Weight(1.0),
        TrackSize::Fit,
    ];
    assert_eq!(fig.validate().errors, vec![]);

    fig.axes[1].cell.col = 2;
    fig.panels[0].cell.col = 1;
    fig.layout.col_weights = vec![
        TrackSize::Weight(1.0),
        TrackSize::Weight(1.0),
        TrackSize::Fit,
    ];
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::AxesInFittedTrack, s.axes[1]),
        "{report:?}"
    );
}

// Why: the location of a panel's content has no `best`, because a cell holds no data for
// the content to avoid; every other location keeps the name it has as a legend location,
// in snake case, so that one vocabulary describes both and a hand-written file can use
// either without a lookup. A file that asks for `best` is refused rather than read as
// some other place. The default is the centre of the cell.
#[test]
fn a_panel_location_is_a_legend_location_without_best() {
    let pairs = [
        (PanelLocation::Center, LegendLocation::Center),
        (PanelLocation::North, LegendLocation::North),
        (PanelLocation::South, LegendLocation::South),
        (PanelLocation::East, LegendLocation::East),
        (PanelLocation::West, LegendLocation::West),
        (PanelLocation::NorthEast, LegendLocation::NorthEast),
        (PanelLocation::NorthWest, LegendLocation::NorthWest),
        (PanelLocation::SouthEast, LegendLocation::SouthEast),
        (PanelLocation::SouthWest, LegendLocation::SouthWest),
    ];
    for (panel, legend) in pairs {
        let written = serde_json::to_value(panel).unwrap();
        assert_eq!(written, serde_json::to_value(legend).unwrap(), "{panel:?}");
        assert_eq!(
            serde_json::from_value::<PanelLocation>(written).unwrap(),
            panel
        );
    }
    assert_eq!(
        serde_json::to_value(PanelLocation::NorthEast).unwrap(),
        json!("north_east")
    );
    assert!(serde_json::from_value::<PanelLocation>(json!("best")).is_err());
    assert_eq!(PanelLocation::default(), PanelLocation::Center);
}

// ---------------------------------------------------------------------------------
// Names and labels
// ---------------------------------------------------------------------------------

// Why: the name of a data array identifies it to tools such as the inspector, and is
// optional; whether present (in any script) or absent, it must survive both formats as
// it was, because a name that came back changed or invented would mislabel the record.
#[test]
fn the_name_of_a_data_array_survives_both_formats_present_or_absent() {
    let mut fig = shared_figure().fig;
    fig.data[0].name = Some("Δp, Pa · s⁻¹ \"quoted\"".to_owned());
    fig.data[1].name = None;
    for restored in [
        Figure::from_json(&fig.to_json()).unwrap(),
        Figure::from_protobuf(&fig.to_protobuf()).unwrap(),
    ] {
        assert_eq!(restored.data[0].name, fig.data[0].name);
        assert_eq!(restored.data[1].name, None);
        assert_eq!(restored, fig);
    }
    let value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    assert_eq!(value["data"][0]["name"], json!("Δp, Pa · s⁻¹ \"quoted\""));
    assert_eq!(value["data"][1]["name"], JsonValue::Null);
}

// Why: a name is never drawn and constrains nothing, so two arrays may share a name and
// an array may have an empty one; validation must accept both, or naming an array for the
// inspector could make a figure invalid.
#[test]
fn the_name_of_a_data_array_never_makes_a_figure_invalid() {
    let mut fig = shared_figure().fig;
    let before = fig.validate();
    fig.data[0].name = Some("same".to_owned());
    fig.data[1].name = Some("same".to_owned());
    assert_eq!(fig.validate(), before);
    fig.data[0].name = Some(String::new());
    assert_eq!(fig.validate(), before);
}

// Why: an artist's label is the text drawn beside it in a legend, and is written under
// that name in JSON. The old name `display_name` is gone, not kept beside it, because
// the rename is an intended breaking change that settles what `label` means.
#[test]
fn an_artist_label_is_written_as_label_and_never_as_display_name() {
    let s = shared_figure();
    let value: JsonValue = serde_json::from_str(&s.fig.to_json()).unwrap();
    let artist = &value["axes"][0]["artists"][0];
    assert_eq!(
        artist["label"],
        json!({ "content": "run 0", "interpreter": "none" })
    );
    assert!(artist.get("display_name").is_none(), "{artist}");
    assert_eq!(
        s.fig.artist(s.lines[0]).unwrap().1.label(),
        Some(&Text::plain("run 0"))
    );
}

// Why: the inspector lists the properties the registry describes, so every kind of
// artist must offer its label, and its parts, under the new name and nowhere under the
// old one; an artist kind left behind would show a dead row in the editor.
#[test]
fn every_kind_of_artist_lists_its_label_and_no_display_name() {
    for kind in [
        NodeKind::Line,
        NodeKind::Scatter,
        NodeKind::Contour,
        NodeKind::Quiver,
        NodeKind::Surface,
        NodeKind::Image,
        NodeKind::ScalarImage,
    ] {
        let paths: Vec<String> = properties(kind)
            .iter()
            .map(|p| p.path.to_string())
            .collect();
        for wanted in ["label", "label.content", "label.interpreter"] {
            assert!(paths.iter().any(|p| p == wanted), "{kind:?} lacks {wanted}");
        }
        assert!(
            !paths.iter().any(|p| p.contains("display_name")),
            "{kind:?} lists {paths:?}"
        );
        let label = properties(kind)
            .into_iter()
            .find(|p| p.path.to_string() == "label")
            .unwrap();
        assert_eq!(label.value_type, ValueType::Text);
        assert!(label.optional);
    }
}

// ---------------------------------------------------------------------------------
// Edits of nodes on the wire
// ---------------------------------------------------------------------------------

// Why: web clients write transactions by hand from the documented JSON form, so the form
// of an inserted data array and panel, and of the data edits that address an array by
// its node identifier, is a contract; the documented form must read back as the same
// transaction.
#[test]
fn the_json_form_of_node_and_data_edits_is_documented() {
    let transaction = tx([
        Edit::Insert {
            parent: NodeId(1),
            index: Some(0),
            node: Node::DataArray(DataArray {
                id: NodeId(5),
                name: Some("t".to_owned()),
                array: NdArray::vector(vec![1.0]),
            }),
        },
        Edit::Insert {
            parent: NodeId(1),
            index: None,
            node: Node::Panel(legend_panel(NodeId(6), Cell::default())),
        },
        Edit::ReplaceData {
            node: NodeId(5),
            array: NdArray::vector(vec![2.0]),
        },
        Edit::AppendData {
            node: NodeId(5),
            array: NdArray::vector(vec![3.0]),
            retain: None,
        },
        Edit::Remove { node: NodeId(5) },
    ]);
    let expected = json!({
        "edits": [
            {
                "type": "insert",
                "parent": 1,
                "index": 0,
                "node": {
                    "data_array": {
                        "id": 5,
                        "name": "t",
                        "array": { "shape": [1], "values": [1.0] }
                    }
                }
            },
            {
                "type": "insert",
                "parent": 1,
                "index": null,
                "node": {
                    "panel": {
                        "id": 6,
                        "cell": { "row": 0, "col": 0, "row_span": 1, "col_span": 1 },
                        "align": "center",
                        "content": {
                            "type": "legend",
                            "value": {
                                "boxed": true,
                                "layout": {
                                    "columns": { "type": "auto" },
                                    "order": "column_major"
                                }
                            }
                        }
                    }
                }
            },
            {
                "type": "replace_data",
                "node": 5,
                "array": { "shape": [1], "values": [2.0] }
            },
            {
                "type": "append_data",
                "node": 5,
                "array": { "shape": [1], "values": [3.0] },
                "retain": null
            },
            { "type": "remove", "node": 5 }
        ]
    });
    let written: JsonValue = serde_json::from_str(&transaction.to_json()).unwrap();
    assert_eq!(written, expected);
    assert_eq!(
        Transaction::from_json(&expected.to_string()).unwrap(),
        transaction
    );
    assert_eq!(
        Transaction::from_protobuf(&transaction.to_protobuf()).unwrap(),
        transaction
    );
}
