//! The view overlay: recording the user's sets, composing the displayed figure,
//! reconciling with the owner's transactions, resetting views, and undo and redo.

mod common;

use common::edits::{applied, line, manual, path, set, tx};
use common::{FigureBuilder, find_axes};
use ironlab_ir::command;
use ironlab_ir::overlay::{Conflict, Overlay, OverlayEntry, Resolution};
use ironlab_ir::*;

/// A source figure with a 2D axes `a` holding a line `line_a` on positive data, with
/// manual x and y limits, and a 3D axes `b` holding a 2D line `line_b` on the same data.
struct Fixture {
    source: Figure,
    a: NodeId,
    line_a: NodeId,
    b: NodeId,
    line_b: NodeId,
    x: DataId,
    y: DataId,
}

fn fixture() -> Fixture {
    let mut builder = FigureBuilder::new();
    builder.fig.layout = TileLayout { rows: 1, cols: 2 };
    let a = builder.axes2d(0, 0);
    builder.axes(a).x.limits = manual(0.0, 10.0);
    builder.axes(a).y.limits = manual(0.0, 10.0);
    let b = builder.axes3d(0, 1);
    let x = builder.vector(&[1.0, 2.0, 3.0]);
    let y = builder.vector(&[1.0, 4.0, 9.0]);
    let (line_a, line_b) = (builder.node(), builder.node());
    builder.push(a, line(line_a, x, y));
    builder.push(b, line(line_b, x, y));
    Fixture {
        source: builder.build(),
        a,
        line_a,
        b,
        line_b,
        x,
        y,
    }
}

/// Records one set as its own undo step.
fn record(overlay: &mut Overlay, node: NodeId, at: &str, value: Value) {
    overlay.record(&tx([set(node, at, value)])).unwrap();
}

/// The node and path of every entry, in order.
fn keys(overlay: &Overlay) -> Vec<(NodeId, String)> {
    overlay
        .entries()
        .iter()
        .map(|entry| (entry.node, entry.path.to_string()))
        .collect()
}

fn limits(min: f64, max: f64) -> Value {
    Value::Limits(manual(min, max))
}

// ---------------------------------------------------------------------------------
// Recording
// ---------------------------------------------------------------------------------

// Why: a drag sends many sets of the same property; the overlay must hold one entry with
// the latest value, or it would grow with every mouse movement.
#[test]
fn recording_the_same_property_again_keeps_one_entry_with_the_latest_value() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(1.0, 2.0));
    record(&mut overlay, f.a, "x.limits", limits(3.0, 4.0));
    overlay
        .record(&tx([
            set(f.a, "x.limits", limits(5.0, 6.0)),
            set(f.a, "x.limits", limits(7.0, 8.0)),
        ]))
        .unwrap();
    assert_eq!(
        overlay.entries(),
        [OverlayEntry {
            node: f.a,
            path: path("x.limits"),
            value: limits(7.0, 8.0),
        }]
    );
}

// Why: setting a whole value supersedes earlier sets of its parts, so setting
// `projection.view3d` after `projection.view3d.zoom` must leave one entry; the same path on
// another node is a different property.
#[test]
fn recording_a_property_replaces_the_entries_below_it_on_the_same_node() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(
        &mut overlay,
        f.b,
        "projection.view3d.zoom",
        Value::Double(2.0),
    );
    record(
        &mut overlay,
        f.b,
        "projection.view3d.azimuth_deg",
        Value::Double(10.0),
    );
    record(&mut overlay, f.a, "x.limits.min", Value::Double(1.0));
    record(
        &mut overlay,
        f.b,
        "projection.view3d",
        Value::View3d(View3d::default()),
    );
    assert_eq!(
        keys(&overlay),
        [
            (f.a, "x.limits.min".to_owned()),
            (f.b, "projection.view3d".to_owned())
        ]
    );
}

// Why: a finer change after a coarser one (one bound after the whole limits) must refine
// it rather than replace it, so both entries are kept and applied in order.
#[test]
fn a_descendant_recorded_after_its_ancestor_is_kept_and_applied_after_it() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(0.0, 20.0));
    record(&mut overlay, f.a, "x.limits.max", Value::Double(5.0));
    assert_eq!(
        keys(&overlay),
        [
            (f.a, "x.limits".to_owned()),
            (f.a, "x.limits.max".to_owned())
        ]
    );
    let composed = overlay.compose(&f.source);
    assert!(composed.dropped.is_empty(), "{:?}", composed.dropped);
    assert_eq!(find_axes(&composed.figure, f.a).x.limits, manual(0.0, 5.0));
}

// Why: the overlay holds only sets, which is what makes composition always defined; a
// structural or data edit must be refused as a whole, naming the offending edit.
#[test]
fn recording_a_transaction_with_an_edit_other_than_a_set_fails_and_records_nothing() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(
        &mut overlay,
        f.a,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    );
    assert!(overlay.undo());
    let result = overlay.record(&tx([
        set(f.a, "x.limits", limits(1.0, 2.0)),
        Edit::PutData {
            id: f.x,
            array: NdArray::vector(vec![1.0, 2.0, 3.0]),
        },
    ]));
    assert!(
        matches!(result, Err(EditError::NotASet { edit: Some(1) })),
        "{result:?}"
    );
    assert!(overlay.entries().is_empty());
    assert!(!overlay.can_undo());
    // A refused record is not a new step, so the undone step can still be redone.
    assert!(overlay.can_redo());
}

// ---------------------------------------------------------------------------------
// Composition
// ---------------------------------------------------------------------------------

// Why: the viewer draws the source with the user's changes on top, and the source (the
// owner's state) must not be touched by the user's view.
#[test]
fn composition_applies_the_entries_over_the_source_without_changing_it() {
    let f = fixture();
    let source = f.source.clone();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    record(&mut overlay, f.line_b, "visible", Value::Bool(false));

    let composed = overlay.compose(&source);
    let mut expected = f.source.clone();
    expected.axes[0].x.limits = manual(2.0, 3.0);
    expected.axes[1].artists[0].set_visible(false);
    assert_eq!(composed.figure, expected);
    assert!(composed.dropped.is_empty());
    assert_eq!(source, f.source);
}

// Why: when the owner removes a node, the user's changes to it can no longer apply; they
// must be skipped and reported (so the viewer can say so), and the other entries applied.
#[test]
fn composition_drops_entries_whose_node_no_longer_exists() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    record(
        &mut overlay,
        f.b,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    );
    let (source, _) = applied(&f.source, &tx([Edit::Remove { node: f.line_a }])).unwrap();

    let composed = overlay.compose(&source);
    assert_eq!(composed.dropped.len(), 1, "{:?}", composed.dropped);
    assert_eq!(composed.dropped[0].entry.node, f.line_a);
    assert!(matches!(
        composed.dropped[0].reason,
        EditError::UnknownNode { node, .. } if node == f.line_a
    ));
    assert_eq!(
        find_axes(&composed.figure, f.b).colormap,
        ColormapName::Gray
    );
}

// Why: a camera change made while an axes was 3D cannot apply once the owner makes it 2D;
// the entry must be dropped as unreachable rather than fail the whole composition.
#[test]
fn composition_drops_entries_whose_path_is_no_longer_reachable() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(
        &mut overlay,
        f.b,
        "projection.view3d.zoom",
        Value::Double(2.0),
    );
    let (source, _) = applied(
        &f.source,
        &tx([set(f.b, "projection", Value::Projection(Projection::TwoD))]),
    )
    .unwrap();

    let composed = overlay.compose(&source);
    assert_eq!(composed.figure, source);
    assert!(
        matches!(
            composed.dropped.as_slice(),
            [dropped] if matches!(dropped.reason, EditError::InactiveVariant { .. })
        ),
        "{:?}",
        composed.dropped
    );
}

// Why: types are checked only when an entry is applied, so an entry of the wrong type (from
// a faulty client) must be dropped with the type error rather than corrupt the display.
#[test]
fn composition_drops_entries_whose_value_does_not_have_the_type_of_the_property() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", Value::Double(1.0));
    let composed = overlay.compose(&f.source);
    assert_eq!(composed.figure, f.source);
    assert!(
        matches!(
            composed.dropped.as_slice(),
            [dropped] if matches!(dropped.reason, EditError::TypeMismatch { expected: ValueType::Limits, .. })
        ),
        "{:?}",
        composed.dropped
    );
}

// Why: a user's zoom to limits that include negative values becomes invalid when the owner
// switches that axis to a log scale; the entry that would add the error must be dropped
// and reported, while later valid entries still apply.
#[test]
fn composition_drops_entries_that_would_add_validation_errors() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(-1.0, 1.0));
    record(
        &mut overlay,
        f.a,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    );
    let (source, _) = applied(
        &f.source,
        &tx([
            set(f.a, "x.scale", Value::Scale(Scale::Log)),
            set(f.a, "x.limits", limits(1.0, 10.0)),
        ]),
    )
    .unwrap();

    let composed = overlay.compose(&source);
    assert!(
        matches!(
            composed.dropped.as_slice(),
            [dropped] if dropped.entry.path == path("x.limits") && matches!(&dropped.reason, EditError::Invalid(issues) if issues.iter().any(|i| i.kind == IssueKind::InvalidLimits))
        ),
        "{:?}",
        composed.dropped
    );
    let displayed = find_axes(&composed.figure, f.a);
    assert_eq!(displayed.x.limits, manual(1.0, 10.0));
    assert_eq!(displayed.colormap, ColormapName::Gray);
}

// Why: a source may already have errors (a loaded file); the user's view changes must
// still apply, because only errors the overlay adds are its fault.
#[test]
fn composition_keeps_entries_over_a_source_that_already_has_errors() {
    let f = fixture();
    let mut source = f.source.clone();
    source.size.width_mm = 0.0;
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    let composed = overlay.compose(&source);
    assert!(composed.dropped.is_empty(), "{:?}", composed.dropped);
    assert_eq!(find_axes(&composed.figure, f.a).x.limits, manual(2.0, 3.0));
}

// Why: once the viewer has told the user that entries were dropped, it discards them so
// that the notice is not repeated on every frame.
#[test]
fn discarding_dropped_entries_removes_them_from_the_overlay() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    let (source, _) = applied(&f.source, &tx([Edit::Remove { node: f.line_a }])).unwrap();

    let dropped = overlay.compose(&source).dropped;
    overlay.discard(&dropped);
    assert_eq!(keys(&overlay), [(f.a, "x.limits".to_owned())]);
    assert!(overlay.compose(&source).dropped.is_empty());
}

// Why: an entry that composition discards changes nothing the user can see, so the undo
// step that recording it pushed is spent: the user's first undo would appear to do
// nothing. Discarding the entry removes that step, so the next undo reaches the change
// before it.
#[test]
fn discarding_every_entry_of_a_step_leaves_no_step_to_undo() {
    let f = fixture();
    let (source, _) = applied(&f.source, &tx([Edit::Remove { node: f.line_a }])).unwrap();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    let kept = overlay.entries().to_vec();

    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    let dropped = overlay.compose(&source).dropped;
    assert_eq!(dropped.len(), 1, "{dropped:?}");
    overlay.discard(&dropped);

    assert_eq!(overlay.entries(), kept);
    assert!(
        overlay.undo(),
        "the step of the change that was kept remains"
    );
    assert!(overlay.entries().is_empty());
    assert!(
        !overlay.can_undo(),
        "the discarded change left no step behind"
    );
}

// Why: a gesture of many sets is one undo step, and discarding some of its entries must
// not break that: the step is kept while any of its entries survives, and only a step
// whose entries are all discarded disappears. A discard part-way through an open gesture
// must not consume a step of an earlier gesture either.
#[test]
fn a_gesture_whose_entries_are_discarded_keeps_the_steps_of_earlier_gestures() {
    let f = fixture();
    let (source, _) = applied(&f.source, &tx([Edit::Remove { node: f.line_a }])).unwrap();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    let kept = overlay.entries().to_vec();

    // One gesture that sets a surviving property and one that the figure cannot show.
    overlay.begin_step();
    record(&mut overlay, f.a, "y.limits", limits(4.0, 5.0));
    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    let dropped = overlay.compose(&source).dropped;
    overlay.discard(&dropped);
    overlay.end_step();

    assert_eq!(
        keys(&overlay),
        [(f.a, "x.limits".to_owned()), (f.a, "y.limits".to_owned())]
    );
    assert!(
        overlay.undo(),
        "the gesture kept one entry, so it is a step"
    );
    assert_eq!(overlay.entries(), kept);
    assert!(overlay.undo(), "the earlier step was not consumed");
    assert!(overlay.entries().is_empty());
    assert!(!overlay.can_undo());
}

// Why: saving a figure that the viewer owns writes the overlay into the source, which must
// give exactly the figure that was displayed.
#[test]
fn the_overlay_as_a_transaction_reproduces_the_displayed_figure() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(0.0, 20.0));
    record(&mut overlay, f.a, "x.limits.max", Value::Double(5.0));
    record(&mut overlay, f.line_b, "visible", Value::Bool(false));
    let (saved, _) = applied(&f.source, &overlay.to_transaction()).unwrap();
    assert_eq!(saved, overlay.compose(&f.source).figure);
}

// ---------------------------------------------------------------------------------
// Reconciliation with the owner
// ---------------------------------------------------------------------------------

// Why: new data or an unrelated property change from the owner must never reset or
// question the user's zoom.
#[test]
fn an_owner_set_of_an_unrelated_property_raises_no_conflict() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    let owner = tx([
        set(f.a, "x.grid", Value::Bool(true)),
        set(f.a, "y.limits", limits(1.0, 2.0)),
        set(f.a, "colormap", Value::ColormapName(ColormapName::Gray)),
        set(f.b, "x.limits", limits(1.0, 2.0)),
    ]);
    assert!(overlay.reconcile(&f.source, &owner).is_empty());
    assert!(overlay.conflicts().is_empty());
    assert_eq!(keys(&overlay), [(f.a, "x.limits".to_owned())]);
}

// Why: when the owner sets the same property as the user, or a property that contains or
// is contained by it, the two disagree about the same thing; the viewer must be told which
// node and which paths, and the user's entry must be kept until the user decides.
#[test]
fn an_owner_set_of_the_same_an_enclosing_or_an_enclosed_path_is_a_conflict() {
    let f = fixture();
    for (overlay_node, overlay_path, value, source_path, source_value) in [
        (
            f.a,
            "x.limits",
            limits(2.0, 3.0),
            "x.limits",
            limits(4.0, 5.0),
        ),
        (
            f.a,
            "x.limits",
            limits(2.0, 3.0),
            "x",
            Value::Axis(Axis::default()),
        ),
        (
            f.a,
            "x.limits",
            limits(2.0, 3.0),
            "x.limits.min",
            Value::Double(1.0),
        ),
        (
            f.b,
            "projection.view3d",
            Value::View3d(View3d::default()),
            "projection.view3d.azimuth_deg",
            Value::Double(10.0),
        ),
    ] {
        let mut overlay = Overlay::new();
        record(&mut overlay, overlay_node, overlay_path, value);
        let conflicts = overlay.reconcile(
            &f.source,
            &tx([set(overlay_node, source_path, source_value)]),
        );
        let expected = Conflict {
            node: overlay_node,
            overlay_path: path(overlay_path),
            source_path: path(source_path),
        };
        assert_eq!(
            conflicts,
            std::slice::from_ref(&expected),
            "{overlay_path} against {source_path}"
        );
        assert_eq!(overlay.conflicts(), [expected]);
        assert_eq!(overlay.entries().len(), 1, "the entry is kept");
    }
}

// Why: the viewer never edits data, so streamed or replaced data must never conflict with
// the user's view or drop an entry.
#[test]
fn data_edits_never_conflict_with_the_overlay() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    let before = overlay.entries().to_vec();
    let owner = tx([
        Edit::PutData {
            id: DataId(100),
            array: NdArray::vector(vec![1.0]),
        },
        Edit::AppendData {
            id: f.x,
            array: NdArray::vector(vec![4.0]),
            retain: Some(3),
        },
        Edit::AppendData {
            id: f.y,
            array: NdArray::vector(vec![16.0]),
            retain: Some(3),
        },
        Edit::RemoveData { id: DataId(100) },
    ]);
    applied(&f.source, &owner).expect("the owner's transaction applies");
    assert!(overlay.reconcile(&f.source, &owner).is_empty());
    assert_eq!(overlay.entries(), before);
}

// Why: entries of a removed node can never apply again, including those of the artists
// inside a removed axes; they are dropped silently (the plot is gone), with any pending
// conflicts of those entries.
#[test]
fn an_owner_removal_drops_the_entries_of_the_node_and_its_descendants_without_conflict() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    record(
        &mut overlay,
        f.b,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    );
    overlay.reconcile(&f.source, &tx([set(f.a, "x.limits", limits(4.0, 5.0))]));
    assert_eq!(overlay.conflicts().len(), 1);

    let conflicts = overlay.reconcile(&f.source, &tx([Edit::Remove { node: f.a }]));
    assert!(conflicts.is_empty());
    assert!(overlay.conflicts().is_empty());
    assert_eq!(keys(&overlay), [(f.b, "colormap".to_owned())]);
}

// Why: an owner that replaces a plot by removing it and inserting a new one with the same
// identifier has created a different node; the user's changes to the old one must not
// silently apply to the new one.
#[test]
fn an_owner_removal_and_reinsertion_with_the_same_identifier_drops_the_entries() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    let replacement = find_axes(&f.source, f.a).artists[0].clone();
    let owner = tx([
        Edit::Remove { node: f.line_a },
        Edit::Insert {
            parent: f.a,
            index: None,
            node: Node::Artist(replacement),
        },
    ]);
    assert!(overlay.reconcile(&f.source, &owner).is_empty());
    assert!(overlay.entries().is_empty());
}

// Why: conflict detection is syntactic (ADR 0008): it compares nodes and paths, not link
// groups. An owner's literal set of a partner axes does not touch the user's entry, so it
// raises no conflict although the two axes are linked; the owner's `set_limits` command sets
// every member of the group, including the user's axes, and so it does conflict.
#[test]
fn conflicts_compare_nodes_and_paths_not_link_groups() {
    let f = fixture();
    let mut source = f.source.clone();
    source.links = vec![AxisLink {
        dimension: Dimension::X,
        axes: vec![f.a, f.b],
    }];
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));

    let partner_only = tx([set(f.b, "x.limits", limits(4.0, 5.0))]);
    assert!(overlay.reconcile(&source, &partner_only).is_empty());
    assert!(overlay.conflicts().is_empty());
    source.apply(&partner_only).unwrap();

    let group = command::set_limits(&source, f.b, Dimension::X, manual(6.0, 7.0)).unwrap();
    assert_eq!(
        overlay.reconcile(&source, &group),
        [Conflict {
            node: f.a,
            overlay_path: path("x.limits"),
            source_path: path("x.limits"),
        }]
    );
}

// Why: an owner that follows streamed data may set the limits it shows on every frame, and
// may set them twice in one transaction; while the user's zoom is in place, that must leave
// one pending conflict for the viewer to show, not one per frame or per set.
#[test]
fn repeated_owner_sets_of_a_conflicting_path_leave_one_pending_conflict() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    let expected = Conflict {
        node: f.a,
        overlay_path: path("x.limits"),
        source_path: path("x.limits"),
    };
    let mut source = f.source.clone();
    for frame in 0..3 {
        let k = f64::from(frame);
        let owner = tx([
            set(f.a, "x.limits", limits(k, k + 10.0)),
            set(f.a, "x.limits", limits(k, k + 11.0)),
        ]);
        assert_eq!(
            overlay.reconcile(&source, &owner),
            std::slice::from_ref(&expected),
            "frame {frame}"
        );
        source.apply(&owner).unwrap();
    }
    assert_eq!(overlay.conflicts(), [expected]);
}

// Why: moving a plot to another axes keeps its identity, so the user's changes to it (hiding
// it, restyling it) must follow it without a conflict or a dropped entry.
#[test]
fn an_owner_move_keeps_the_entries_of_the_moved_node() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    let before = overlay.entries().to_vec();
    let owner = tx([Edit::Move {
        node: f.line_a,
        parent: f.b,
        index: Some(0),
    }]);
    let (source, _) = applied(&f.source, &owner).unwrap();

    assert!(overlay.reconcile(&f.source, &owner).is_empty());
    assert_eq!(overlay.entries(), before);
    let composed = overlay.compose(&source);
    assert!(composed.dropped.is_empty(), "{:?}", composed.dropped);
    let (parent, artist) = composed.figure.artist(f.line_a).unwrap();
    assert_eq!(parent.id, f.b);
    assert!(!artist.visible());
}

// Why: an owner that rearranges subplots may move a plot out of an axes and then remove that
// axes in one transaction; the plot survives, so the user's changes to it must survive too,
// while the changes to the removed axes are dropped.
#[test]
fn an_artist_moved_out_of_an_axes_before_the_axes_is_removed_keeps_its_entries() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    let owner = tx([
        Edit::Move {
            node: f.line_a,
            parent: f.b,
            index: None,
        },
        Edit::Remove { node: f.a },
    ]);
    let (source, _) = applied(&f.source, &owner).unwrap();

    assert!(overlay.reconcile(&f.source, &owner).is_empty());
    assert_eq!(keys(&overlay), [(f.line_a, "visible".to_owned())]);
    assert!(overlay.compose(&source).dropped.is_empty());
}

// Why: a plot that the owner adds is not one the user has changed, so adding it must leave
// the entries, the conflicts and the undo history untouched.
#[test]
fn an_owner_insertion_of_a_new_node_leaves_the_overlay_unchanged() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    let before = overlay.clone();
    let owner = tx([Edit::Insert {
        parent: f.a,
        index: Some(0),
        node: Node::Artist(line(NodeId(100), f.x, f.y)),
    }]);
    applied(&f.source, &owner).expect("the owner's transaction applies");
    assert!(overlay.reconcile(&f.source, &owner).is_empty());
    assert_eq!(overlay, before);
}

// Why: while a conflict awaits the user's decision, the display must keep showing what the
// user chose, not jump to the owner's value.
#[test]
fn the_users_value_remains_displayed_while_a_conflict_is_unresolved() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    let owner = tx([set(f.a, "x.limits", limits(4.0, 5.0))]);
    let (source, _) = applied(&f.source, &owner).unwrap();
    overlay.reconcile(&f.source, &owner);
    assert_eq!(overlay.conflicts().len(), 1);
    assert_eq!(
        find_axes(&overlay.compose(&source).figure, f.a).x.limits,
        manual(2.0, 3.0)
    );
}

// Why: choosing the owner's value removes the user's entry so the source shows through;
// choosing to keep the user's value keeps the entry; either way the notice goes away.
#[test]
fn resolving_a_conflict_uses_the_source_or_keeps_the_users_value() {
    let f = fixture();
    let owner = tx([set(f.a, "x.limits", limits(4.0, 5.0))]);
    let (source, _) = applied(&f.source, &owner).unwrap();

    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    let conflict = overlay.reconcile(&f.source, &owner).remove(0);
    assert!(overlay.resolve(&conflict, Resolution::UseSource));
    assert!(overlay.entries().is_empty());
    assert!(overlay.conflicts().is_empty());
    assert_eq!(overlay.compose(&source).figure, source);
    assert!(
        !overlay.resolve(&conflict, Resolution::UseSource),
        "already resolved"
    );

    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    let conflict = overlay.reconcile(&f.source, &owner).remove(0);
    assert!(overlay.resolve(&conflict, Resolution::KeepMine));
    assert_eq!(keys(&overlay), [(f.a, "x.limits".to_owned())]);
    assert!(overlay.conflicts().is_empty());
    assert_eq!(
        find_axes(&overlay.compose(&source).figure, f.a).x.limits,
        manual(2.0, 3.0)
    );
}

// ---------------------------------------------------------------------------------
// Reverting one property
// ---------------------------------------------------------------------------------

// Why: the property editor reverts the one property whose revert control was clicked, so
// a revert that also removed an entry of a path above or below it, or of another node,
// would silently throw away changes the user did not ask to lose.
#[test]
fn reverting_removes_the_entry_of_exactly_that_node_and_path() {
    let f = fixture();
    let mut overlay = mixed_overlay(&f);
    assert!(overlay.revert(f.b, &path("projection")));
    assert_eq!(
        keys(&overlay),
        [
            (f.a, "x.limits".to_owned()),
            (f.a, "y.limits.min".to_owned()),
            (f.a, "x.scale".to_owned()),
            (f.a, "colormap".to_owned()),
            (f.b, "projection.view3d.zoom".to_owned()),
            (f.b, "z.limits".to_owned()),
            (f.b, "x.limits".to_owned()),
            (f.line_a, "visible".to_owned()),
        ],
        "the entry below the reverted path is kept"
    );
    assert!(overlay.revert(f.a, &path("y.limits.min")));
    assert!(
        !overlay.revert(f.a, &path("y.limits")),
        "the ancestor of a reverted path never had an entry of its own"
    );
    assert_eq!(
        keys(&overlay),
        [
            (f.a, "x.limits".to_owned()),
            (f.a, "x.scale".to_owned()),
            (f.a, "colormap".to_owned()),
            (f.b, "projection.view3d.zoom".to_owned()),
            (f.b, "z.limits".to_owned()),
            (f.b, "x.limits".to_owned()),
            (f.line_a, "visible".to_owned()),
        ]
    );
    assert!(
        !overlay.revert(f.b, &path("x.scale")),
        "the same path on another node is not this node's entry"
    );
}

// Why: a revert is a change the user makes, as easy to do by accident as any other, so it
// must be undoable on its own rather than folded into the change before it.
#[test]
fn reverting_is_its_own_undo_step() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    record(&mut overlay, f.a, "x.scale", Value::Scale(Scale::Log));

    assert!(overlay.revert(f.a, &path("x.limits")));
    assert!(overlay.undo(), "the revert is a step");
    assert_eq!(
        overlay.entries(),
        [
            OverlayEntry {
                node: f.a,
                path: path("x.limits"),
                value: limits(2.0, 3.0),
            },
            OverlayEntry {
                node: f.a,
                path: path("x.scale"),
                value: Value::Scale(Scale::Log),
            },
        ],
        "undoing the revert restores the entry it removed and nothing else"
    );
    assert!(overlay.redo());
    assert_eq!(keys(&overlay), [(f.a, "x.scale".to_owned())]);
}

// Why: a revert control is shown for a property the overlay overrides, but a stale frame
// or a reconciliation can leave it pointing at an entry that is already gone; reverting
// nothing must then be inert rather than adding an undo step that appears to do nothing.
#[test]
fn reverting_a_property_that_the_overlay_does_not_override_changes_nothing() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    assert!(overlay.undo());
    assert!(!overlay.can_undo());

    assert!(!overlay.revert(f.a, &path("x.limits")));
    assert!(overlay.entries().is_empty());
    assert!(!overlay.can_undo(), "reverting nothing adds no step");
    assert!(overlay.redo(), "reverting nothing does not clear the redo");
}

// Why: an entry that is reverted while its conflict with the source is pending is gone,
// so the notice about it must go with it, or the viewer would ask the user to decide the
// fate of a change that no longer exists.
#[test]
fn reverting_an_entry_clears_its_pending_conflict() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    record(
        &mut overlay,
        f.a,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    );
    overlay.reconcile(
        &f.source,
        &tx([
            set(f.a, "x.limits", limits(0.0, 4.0)),
            set(f.a, "colormap", Value::ColormapName(ColormapName::Magma)),
        ]),
    );
    assert_eq!(overlay.conflicts().len(), 2);

    assert!(overlay.revert(f.a, &path("x.limits")));

    assert_eq!(
        overlay
            .conflicts()
            .iter()
            .map(|conflict| conflict.overlay_path.to_string())
            .collect::<Vec<String>>(),
        ["colormap".to_owned()]
    );
}

// ---------------------------------------------------------------------------------
// Reset
// ---------------------------------------------------------------------------------

/// An overlay with view and non-view entries on both axes and on an artist.
fn mixed_overlay(f: &Fixture) -> Overlay {
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    record(&mut overlay, f.a, "y.limits.min", Value::Double(1.0));
    record(&mut overlay, f.a, "x.scale", Value::Scale(Scale::Log));
    record(
        &mut overlay,
        f.a,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    );
    record(
        &mut overlay,
        f.b,
        "projection",
        Value::Projection(Projection::ThreeD {
            view3d: View3d::default(),
        }),
    );
    record(
        &mut overlay,
        f.b,
        "projection.view3d.zoom",
        Value::Double(2.0),
    );
    record(&mut overlay, f.b, "z.limits", limits(0.0, 1.0));
    record(&mut overlay, f.b, "x.limits", limits(0.0, 1.0));
    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    overlay
}

// Why: double-clicking an axes resets its view (limits and camera, at any depth) and
// nothing else: its other properties, the views of other axes and the visibility of plots
// are choices about content, not about the view.
#[test]
fn resetting_one_axes_removes_only_its_view_entries() {
    let f = fixture();
    let mut overlay = mixed_overlay(&f);
    overlay.reset_view(f.a);
    assert_eq!(
        keys(&overlay),
        [
            (f.a, "x.scale".to_owned()),
            (f.a, "colormap".to_owned()),
            (f.b, "projection".to_owned()),
            (f.b, "projection.view3d.zoom".to_owned()),
            (f.b, "z.limits".to_owned()),
            (f.b, "x.limits".to_owned()),
            (f.line_a, "visible".to_owned()),
        ]
    );
    overlay.reset_view(f.b);
    assert_eq!(
        keys(&overlay),
        [
            (f.a, "x.scale".to_owned()),
            (f.a, "colormap".to_owned()),
            (f.b, "projection".to_owned()),
            (f.line_a, "visible".to_owned()),
        ]
    );
}

// Why: Reset view removes the view entries of every axes, including a whole camera view
// and a refinement below it, and keeps visibility.
#[test]
fn resetting_every_view_keeps_visibility_and_other_properties() {
    let f = fixture();
    let mut overlay = mixed_overlay(&f);
    record(
        &mut overlay,
        f.b,
        "projection.view3d",
        Value::View3d(View3d::default()),
    );
    record(
        &mut overlay,
        f.b,
        "projection.view3d.azimuth_deg",
        Value::Double(5.0),
    );
    overlay.reset_all_views();
    assert_eq!(
        keys(&overlay),
        [
            (f.a, "x.scale".to_owned()),
            (f.a, "colormap".to_owned()),
            (f.b, "projection".to_owned()),
            (f.line_a, "visible".to_owned()),
        ]
    );
}

// ---------------------------------------------------------------------------------
// Clearing every change
// ---------------------------------------------------------------------------------

// Why: the viewer's "Revert all changes" control must take back everything the user did,
// of every kind, in one action; a clear that kept any entry would leave the figure in a
// state the user never chose and could not name.
#[test]
fn clearing_removes_every_entry_whatever_its_kind() {
    let f = fixture();
    let mut overlay = mixed_overlay(&f);
    assert!(overlay.clear());
    assert_eq!(keys(&overlay), [] as [(NodeId, String); 0]);
}

// Why: the changes an overlay holds are how one user is looking at a figure, not
// anything in the figure itself, so taking them all back is a clean slate rather than a
// step to go back from: nothing is left to undo or redo, and the overlay is as it was
// created.
#[test]
fn clearing_leaves_nothing_to_undo_or_redo() {
    let f = fixture();
    let mut overlay = mixed_overlay(&f);
    overlay.undo();
    assert!(
        overlay.can_undo() && overlay.can_redo(),
        "precondition: there is a history in both directions"
    );

    assert!(overlay.clear());

    assert!(
        !overlay.can_undo(),
        "the undo history goes with the entries"
    );
    assert!(!overlay.can_redo(), "so does the redo history");
    assert!(!overlay.undo());
    assert!(!overlay.redo());
    assert_eq!(overlay, Overlay::new());
}

// Why: the control is disabled when there is nothing to discard, but a stale frame can
// still click it; clearing an overlay that is already empty and has no history must then
// be inert, so that the clear reports a change only when it made one.
#[test]
fn clearing_an_overlay_that_is_already_empty_does_nothing() {
    let f = fixture();
    let mut overlay = Overlay::new();
    assert!(!overlay.clear());

    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    assert!(overlay.undo());
    assert!(
        overlay.entries().is_empty() && !overlay.can_undo() && overlay.can_redo(),
        "precondition: no entries, but a history to discard"
    );
    assert!(
        overlay.clear(),
        "a history left behind is still something to clear"
    );
    assert!(!overlay.clear());
}

// Why: the overlay is the user's changes alone, so taking them all back must leave the
// figure its owner defined exactly as it was, not a figure that merely resembles it.
#[test]
fn clearing_leaves_the_source_untouched_and_displays_it() {
    let f = fixture();
    let mut overlay = mixed_overlay(&f);
    assert_ne!(
        overlay.compose(&f.source).figure,
        f.source,
        "precondition: the overlay changes what is displayed"
    );

    overlay.clear();

    let composition = overlay.compose(&f.source);
    assert_eq!(composition.figure, f.source);
    assert!(composition.dropped.is_empty());
}

// Why: a conflict asks the user to decide the fate of one of their changes; once every
// change is gone there is nothing left to decide, and a pending notice would ask about a
// change that no longer exists.
#[test]
fn clearing_removes_the_pending_conflicts() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(2.0, 3.0));
    overlay.reconcile(&f.source, &tx([set(f.a, "x.limits", limits(0.0, 4.0))]));
    assert_eq!(overlay.conflicts().len(), 1);

    overlay.clear();

    assert!(overlay.conflicts().is_empty());
}

// ---------------------------------------------------------------------------------
// Undo and redo
// ---------------------------------------------------------------------------------

// Why: a drag is one gesture, so one undo must revert the whole drag, back to the state
// before it, not one mouse movement.
#[test]
fn a_gesture_of_many_sets_is_undone_in_one_step() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(
        &mut overlay,
        f.a,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    );
    let before_drag = overlay.entries().to_vec();

    overlay.begin_step();
    for k in 0..20 {
        let k = f64::from(k);
        record(&mut overlay, f.a, "x.limits", limits(k, k + 1.0));
        record(&mut overlay, f.a, "y.limits", limits(k, k + 2.0));
    }
    overlay.end_step();
    assert_eq!(overlay.entries().len(), 3);

    assert!(overlay.undo());
    assert_eq!(overlay.entries(), before_drag);
    assert!(overlay.undo());
    assert!(overlay.entries().is_empty());
    assert!(!overlay.can_undo());
}

// Why: redo must re-apply an undone gesture exactly, and a new change after an undo starts
// a new history in which the undone gesture can no longer be redone.
#[test]
fn redo_reapplies_an_undone_step_and_a_new_step_clears_redo() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.a, "x.limits", limits(1.0, 2.0));
    record(&mut overlay, f.a, "x.limits", limits(3.0, 4.0));
    let after = overlay.entries().to_vec();

    assert!(overlay.undo());
    assert_eq!(overlay.entries()[0].value, limits(1.0, 2.0));
    assert!(overlay.can_redo());
    assert!(overlay.redo());
    assert_eq!(overlay.entries(), after);

    assert!(overlay.undo());
    record(
        &mut overlay,
        f.a,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    );
    assert!(!overlay.can_redo());
    assert!(!overlay.redo());
}

// Why: a reset discards the user's view, which is easy to do by accident with a
// double-click, so it must be undoable like any other change.
#[test]
fn a_reset_is_an_undoable_step() {
    let f = fixture();
    let mut overlay = mixed_overlay(&f);
    let before = overlay.entries().to_vec();
    overlay.reset_all_views();
    let after = overlay.entries().to_vec();
    assert!(overlay.undo());
    assert_eq!(overlay.entries(), before);
    assert!(overlay.redo());
    assert_eq!(overlay.entries(), after);
}

// Why: undo acts on the user's changes only, and reconciling is not a user's change. After
// the owner removes a plot, undoing the user's later gesture must not bring back entries
// for the removed plot, which composition would report as dropped, or which would apply to
// a new plot that the owner inserts later under the same identifier.
#[test]
fn undo_after_an_owner_removal_never_restores_entries_of_the_removed_node() {
    let f = fixture();
    let mut overlay = Overlay::new();
    record(&mut overlay, f.line_a, "visible", Value::Bool(false));
    record(
        &mut overlay,
        f.a,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    );
    let removal = tx([Edit::Remove { node: f.line_a }]);
    let (source, _) = applied(&f.source, &removal).unwrap();
    overlay.reconcile(&f.source, &removal);
    assert_eq!(keys(&overlay), [(f.a, "colormap".to_owned())]);

    // The first undo reverts the colormap gesture, not the reconciliation.
    assert!(overlay.undo());
    assert!(overlay.entries().is_empty());
    assert!(overlay.compose(&source).dropped.is_empty());
    assert!(overlay.redo());
    assert_eq!(keys(&overlay), [(f.a, "colormap".to_owned())]);

    let reinsertion = tx([Edit::Insert {
        parent: f.a,
        index: None,
        node: Node::Artist(line(f.line_a, f.x, f.y)),
    }]);
    let (reinserted, _) = applied(&source, &reinsertion).unwrap();
    overlay.reconcile(&source, &reinsertion);
    while overlay.undo() {
        assert!(
            overlay.entries().iter().all(|entry| entry.node != f.line_a),
            "undo restored an entry of the removed node: {:?}",
            overlay.entries()
        );
    }
    let composed = overlay.compose(&reinserted);
    assert!(composed.figure.artist(f.line_a).unwrap().1.visible());
}

// Why: undo and redo with nothing to do are reachable from the keyboard at any time and
// must change nothing; a click that records nothing must not add an empty undo step.
#[test]
fn undo_redo_and_empty_gestures_with_nothing_to_do_change_nothing() {
    let f = fixture();
    let mut overlay = Overlay::new();
    assert!(!overlay.undo());
    assert!(!overlay.redo());
    overlay.begin_step();
    overlay.end_step();
    assert!(!overlay.can_undo());

    record(
        &mut overlay,
        f.b,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    );
    let entries = overlay.entries().to_vec();
    assert!(!overlay.redo());
    assert_eq!(overlay.entries(), entries);

    // A double-click on an axes with no view entries removes nothing and adds no step, so
    // one undo still reverts the colormap and leaves nothing to undo.
    overlay.reset_view(f.a);
    overlay.reset_all_views();
    assert!(overlay.undo());
    assert!(overlay.entries().is_empty());
    assert!(!overlay.can_undo());
}
