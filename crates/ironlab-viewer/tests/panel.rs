//! The property editor: the object tree, the inspector, the parameters editor and the
//! selection that the canvas shares with them.
//!
//! The logic that builds the panel's contents and turns a change of a widget into a
//! transaction is pure, and is tested here directly; the panel itself is driven through
//! egui_kittest's accessibility tree, as the toolbar is.

mod common;

use common::*;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ironlab_ir::{Choice, ColormapName, Dimension, Figure, NodeId, NodeKind, Parameter, Value};
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::{HitMap, LegendHit};
use ironlab_viewer::inspector::{
    Editor, ParameterKind, ParametersDraft, PropertyGroup, PropertyRow, commit, property_groups,
    read_only_reason, tree_rows,
};
use ironlab_viewer::panel::revert_all_label;
use ironlab_viewer::{FigureState, Origin, PropertyPanel, property_panel};

const PLOT: Rect = Rect::new(50.0, 20.0, 200.0, 100.0);

/// The 2D axes, the 3D axes, the line, the scatter and the surface of
/// [`figure_with_artists`].
const FIGURE: NodeId = NodeId(1);
const FLAT: NodeId = NodeId(2);
const SOLID: NodeId = NodeId(3);
const LINE: NodeId = NodeId(4);
const SCATTER: NodeId = NodeId(5);
const SURFACE: NodeId = NodeId(6);

fn state_with_artists() -> FigureState {
    FigureState::new(figure_with_artists())
}

/// The group named `name` of the properties of a node, or a panic naming the groups
/// there are.
fn group<'a>(groups: &'a [PropertyGroup], name: &str) -> &'a PropertyGroup {
    groups
        .iter()
        .find(|group| group.name == name)
        .unwrap_or_else(|| {
            let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
            panic!("there is no group {name:?}; there are {names:?}")
        })
}

/// The labels of the rows of every group, as `group/label`.
fn row_paths(groups: &[PropertyGroup]) -> Vec<String> {
    groups
        .iter()
        .flat_map(|group| {
            group
                .rows
                .iter()
                .map(move |row| format!("{}/{}", group.name, row.label))
        })
        .collect()
}

fn groups_of(state: &FigureState, node: NodeId) -> Vec<PropertyGroup> {
    property_groups(state.figure(), state.overlay(), node)
}

// ---------------------------------------------------------------------------------
// The object tree
// ---------------------------------------------------------------------------------

// Why: the tree is the only way to reach an artist until the canvas can pick one
// (issue #1), so every node must be in it, under the axes it belongs to, in the order in
// which it is drawn; a node missing from the tree cannot be edited at all.
#[test]
fn the_tree_lists_every_node_under_its_parent_in_drawing_order() {
    let figure = figure_with_artists();
    let rows = tree_rows(&figure);
    let summary: Vec<(NodeId, usize, &str)> = rows
        .iter()
        .map(|row| (row.node, row.depth, row.label.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            (FIGURE, 0, "Figure"),
            (FLAT, 1, "Speed"),
            (LINE, 2, "Measured"),
            (SCATTER, 2, "Scatter"),
            (SOLID, 1, "Axes (row 0, column 1)"),
            (SURFACE, 2, "Surface"),
        ],
        "an axes is labelled by its title or else by its cell, and an artist by its \
         display name or else by its kind"
    );
    assert_eq!(
        rows.iter().map(|row| row.kind).collect::<Vec<NodeKind>>(),
        [
            NodeKind::Figure,
            NodeKind::Axes,
            NodeKind::Line,
            NodeKind::Scatter,
            NodeKind::Axes,
            NodeKind::Surface,
        ]
    );
}

// Why: a hidden plot is still in the tree and still editable, so the tree must show which
// plots are hidden; without that the user cannot tell why a plot is missing from the
// canvas.
#[test]
fn the_tree_greys_a_hidden_artist_and_nothing_else() {
    let figure = figure_with_artists();
    let dimmed: Vec<NodeId> = tree_rows(&figure)
        .into_iter()
        .filter(|row| row.dimmed)
        .map(|row| row.node)
        .collect();
    assert_eq!(dimmed, [SCATTER], "only the hidden scatter is greyed");
}

// ---------------------------------------------------------------------------------
// The inspector
// ---------------------------------------------------------------------------------

// Why: an axes has more than thirty properties; showing them as one flat list would be
// unusable, so they are gathered under the value they belong to, and each row is named by
// what distinguishes it within that value.
#[test]
fn the_properties_of_a_node_are_grouped_by_the_first_segment_of_their_path() {
    let groups = groups_of(&state_with_artists(), FLAT);
    let x = group(&groups, "x");
    let labels: Vec<&str> = x.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(
        labels,
        ["label", "scale", "limits", "grid"],
        "the rows of a group are labelled by the rest of their path, in registry order"
    );
    assert_eq!(x.rows[2].path.to_string(), "x.limits");
    assert!(
        groups.iter().any(|group| group.name == "y"),
        "each axis is its own group"
    );
    assert!(
        group(&groups, "clim")
            .rows
            .iter()
            .any(|row| row.label.is_empty()),
        "a property that is its group's only value keeps an empty label"
    );
}

// Why: a two-dimensional axes has no camera, so offering its azimuth would offer a change
// the IR refuses; a three-dimensional one must offer it, or the camera could only be
// changed by dragging.
#[test]
fn only_a_three_dimensional_axes_shows_the_properties_of_its_camera() {
    let state = state_with_artists();

    let flat = row_paths(&groups_of(&state, FLAT));
    assert!(
        flat.contains(&"x/limits".to_owned()),
        "a 2D axes shows its limits: {flat:?}"
    );
    assert!(
        !flat.iter().any(|row| row.contains("view3d")),
        "a 2D axes shows no camera: {flat:?}"
    );

    let solid = row_paths(&groups_of(&state, SOLID));
    assert!(
        solid.contains(&"projection/view3d.azimuth_deg".to_owned()),
        "a 3D axes shows its camera: {solid:?}"
    );
}

// Why: a property below a variant that is not set, or below an optional value that is
// absent, cannot be read; showing it as a broken row would fill the inspector with
// errors that mean nothing. The value itself must still be offered, so that the user can
// switch the variant or give the absent value one.
#[test]
fn a_property_of_an_inactive_variant_or_an_absent_optional_value_is_hidden_not_broken() {
    let state = state_with_artists();
    let rows = row_paths(&groups_of(&state, SOLID));

    assert!(
        rows.contains(&"title/".to_owned()),
        "the absent title is offered: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.starts_with("title/content")),
        "nothing below the absent title is shown: {rows:?}"
    );

    let groups = groups_of(&state, SOLID);
    let title = &group(&groups, "title").rows[0];
    assert_eq!(title.value, Value::Unset, "an absent value reads as unset");
    assert!(title.optional, "an unset row offers to give the value one");

    // The limits of the 3D axes are manual, so their bounds are shown; the colour limits
    // are automatic, so theirs are not.
    assert!(rows.contains(&"x/limits.min".to_owned()), "{rows:?}");
    assert!(
        !rows.iter().any(|row| row.starts_with("clim/min")),
        "the bounds of automatic limits belong to a variant that is not set: {rows:?}"
    );
}

// Why: a value that is only a container of other values, such as an axis or a line style,
// has nothing to edit of its own; showing it as a row would give the user a control that
// does nothing. A tagged value is different: its row is how the variant is chosen.
#[test]
fn a_composite_value_is_a_heading_and_a_tagged_value_is_a_row_with_the_values_below_it() {
    let mut state = state_with_artists();
    let x = group(&groups_of(&state, FLAT), "x").clone();
    assert!(
        !x.rows.iter().any(|row| row.label.is_empty()),
        "the axis itself is a heading, not a row: {:?}",
        row_paths(std::slice::from_ref(&x))
    );

    let limits = x
        .rows
        .iter()
        .find(|row| row.label == "limits")
        .expect("the limits are a row");
    assert!(
        matches!(limits.editor, Editor::Choice { .. }),
        "a tagged value is chosen from a list: {:?}",
        limits.editor
    );

    state.record(&set(FLAT.0, "x.limits", Value::Limits(manual(0.0, 1.0))));
    let x = group(&groups_of(&state, FLAT), "x").clone();
    let bounds: Vec<(&str, usize)> = x
        .rows
        .iter()
        .filter(|row| row.label.starts_with("limits."))
        .map(|row| (row.label.as_str(), row.depth))
        .collect();
    assert_eq!(
        bounds,
        [("limits.min", 2), ("limits.max", 2)],
        "the bounds of manual limits are nested below them"
    );
}

// Why: editing the data of a plot by typing an array identifier invites a figure that
// refers to data that is the wrong shape or missing; the reference is therefore shown
// with the shape of what it refers to, and left alone.
#[test]
fn a_data_reference_is_read_only_and_carries_the_shape_of_its_array() {
    let groups = groups_of(&state_with_artists(), LINE);
    let x = &group(&groups, "x").rows[0];
    assert_eq!(x.path.to_string(), "x");
    match &x.editor {
        Editor::Data { shape } => assert_eq!(shape.as_deref(), Some(&[3usize][..])),
        other => panic!("the x data of a line is a data reference, not {other:?}"),
    }
}

// Why: the mark and the revert control are how the user tells their own changes from the
// figure's, and how they take one back; marking the wrong property, or none, makes the
// overlay invisible.
#[test]
fn exactly_the_properties_the_overlay_overrides_are_marked() {
    let mut state = state_with_artists();
    state.record(&set(
        FLAT.0,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    ));

    let overridden: Vec<String> = groups_of(&state, FLAT)
        .iter()
        .flat_map(|group| group.rows.iter())
        .filter(|row| row.overridden)
        .map(|row| row.path.to_string())
        .collect();
    assert_eq!(overridden, ["colormap".to_owned()]);
}

// ---------------------------------------------------------------------------------
// From a widget to a transaction
// ---------------------------------------------------------------------------------

// Why: linked axes share their limits, and the property editor is another way to set
// them; if it set only the axes on screen, the link would be broken by editing rather
// than honoured, and the figure would disagree with itself.
#[test]
fn editing_the_limits_of_a_linked_axes_sets_every_axes_of_the_group() {
    let figure = figure_with(
        vec![axes_2d(2), axes_2d(3)],
        vec![link(Dimension::X, &[2, 3])],
    );
    let transaction = commit(
        &figure,
        NodeId(2),
        &path("x.limits"),
        Value::Limits(manual(1.0, 2.0)),
    );
    assert_eq!(
        transaction.edits.len(),
        2,
        "both axes of the group are set: {transaction:?}"
    );

    // A bound of the limits is set through the whole limits, so the group follows it too.
    let transaction = commit(
        &figure,
        NodeId(2),
        &path("x.limits.max"),
        Value::Double(7.0),
    );
    let mut figure = figure.clone();
    figure.apply(&transaction).expect("the limits are valid");
    assert_eq!(limits_of(&figure, 2, Dimension::X), manual(0.0, 7.0));
    assert_eq!(
        limits_of(&figure, 3, Dimension::X),
        manual(0.0, 7.0),
        "the linked axes follows"
    );
}

// Why: every other property belongs to the node the user is looking at and to nothing
// else, so committing one must be exactly one set of that property; anything more would
// change what the user did not touch.
#[test]
fn editing_any_other_property_is_one_set_of_that_property() {
    let figure = figure_with_artists();
    let transaction = commit(&figure, LINE, &path("visible"), Value::Bool(false));
    assert_eq!(transaction, set(LINE.0, "visible", Value::Bool(false)));
}

// ---------------------------------------------------------------------------------
// The parameters editor
// ---------------------------------------------------------------------------------

// Why: parameters are what make a collection of figures sortable and searchable, and
// they are the one property whose entries the user creates; adding, renaming, retyping,
// changing and removing must all reach the map that is committed.
#[test]
fn the_parameters_draft_adds_renames_retypes_changes_and_removes_entries() {
    let mut draft = ParametersDraft::of(&std::collections::BTreeMap::from([(
        "solver".to_owned(),
        Parameter::String("simple".to_owned()),
    )]));
    assert_eq!(draft.rows().len(), 1);

    draft.add();
    let fresh = draft.rows().len() - 1;
    draft.set_name(fresh, "reynolds_number");
    draft.set_kind(fresh, ParameterKind::Number);
    draft.set_text(fresh, "1e5");
    draft.set_text(0, "k–ω SST");
    assert_eq!(
        draft.to_map().expect("the draft is complete"),
        std::collections::BTreeMap::from([
            ("solver".to_owned(), Parameter::String("k–ω SST".to_owned())),
            ("reynolds_number".to_owned(), Parameter::Number(1.0e5)),
        ])
    );

    draft.set_name(0, "solver_name");
    draft.set_kind(fresh, ParameterKind::Integer);
    draft.set_text(fresh, "100000");
    assert_eq!(
        draft.to_map().expect("the draft is complete"),
        std::collections::BTreeMap::from([
            (
                "solver_name".to_owned(),
                Parameter::String("k–ω SST".to_owned())
            ),
            ("reynolds_number".to_owned(), Parameter::Integer(100_000)),
        ])
    );

    draft.remove(0);
    assert_eq!(
        draft.to_map().expect("the draft is complete"),
        std::collections::BTreeMap::from([(
            "reynolds_number".to_owned(),
            Parameter::Integer(100_000)
        )])
    );
}

// Why: a half-typed number and a name typed over another are ordinary states of an
// editor; committing them would write a figure the IR refuses or lose a parameter
// silently, so the draft must refuse to be committed and say why.
#[test]
fn the_parameters_draft_refuses_a_name_or_a_number_that_would_lose_or_break_an_entry() {
    let mut draft = ParametersDraft::of(&std::collections::BTreeMap::new());
    draft.add();
    draft.set_name(0, "");
    assert!(draft.to_map().is_err(), "an unnamed parameter is refused");

    draft.set_name(0, "n");
    draft.set_kind(0, ParameterKind::Integer);
    draft.set_text(0, "twelve");
    assert!(draft.to_map().is_err(), "an unparsable integer is refused");
    draft.set_text(0, "12");
    assert!(draft.to_map().is_ok());

    draft.add();
    draft.set_name(1, "n");
    assert!(draft.to_map().is_err(), "a repeated name is refused");

    draft.set_name(1, "m");
    draft.set_kind(1, ParameterKind::Number);
    draft.set_text(1, "inf");
    assert!(
        draft.to_map().is_err(),
        "a number that is not finite is refused"
    );
}

// ---------------------------------------------------------------------------------
// Selection, steps and refusals in the figure state
// ---------------------------------------------------------------------------------

fn hit_map_with_legend(artist: NodeId) -> HitMap {
    HitMap {
        axes: vec![hit_2d(FLAT.0, PLOT)],
        legend_entries: vec![LegendHit {
            axes: FLAT,
            artist,
            rect: Rect::new(60.0, 30.0, 20.0, 10.0),
        }],
    }
}

// Why: the canvas cannot yet pick a plot, so a legend entry is the only place on the
// canvas that names one; clicking it must select the plot as well as toggling it, or the
// inspector would still be showing something else.
#[test]
fn clicking_a_legend_entry_selects_the_artist_it_toggles() {
    let mut state = state_with_artists();
    let hit = hit_map_with_legend(LINE);

    assert!(
        state.click(&hit, Point::new(65.0, 35.0)),
        "the plot toggled"
    );

    assert_eq!(state.selection(), Some(LINE));
    assert!(
        !state
            .figure()
            .artist(LINE)
            .expect("the line is in the figure")
            .1
            .visible(),
        "the click still hides the plot"
    );
}

// Why: clicking an axes is how the user asks for the axes they are looking at, and it
// must not disturb the view; a click that also panned or reset would make selection
// dangerous.
#[test]
fn clicking_inside_an_axes_selects_it_without_changing_the_figure() {
    let mut state = state_with_artists();
    let before = state.figure().clone();
    let hit = HitMap {
        axes: vec![hit_2d(FLAT.0, PLOT)],
        legend_entries: vec![],
    };

    assert!(
        !state.click(&hit, Point::new(150.0, 70.0)),
        "selecting is not a change of the figure"
    );
    assert_eq!(state.selection(), Some(FLAT));
    assert_eq!(state.figure(), &before);

    assert!(!state.click(&hit, Point::new(1000.0, 1000.0)));
    assert_eq!(
        state.selection(),
        Some(FLAT),
        "a click outside every axes keeps the selection"
    );
}

// Why: the inspector shows the selected node while the user pans and zooms; a gesture
// that cleared or moved the selection would make the panel useless during navigation.
#[test]
fn the_selection_survives_a_gesture() {
    let mut state = state_with_artists();
    state.select(Some(SURFACE));
    let hit = HitMap {
        axes: vec![hit_2d(FLAT.0, PLOT)],
        legend_entries: vec![],
    };

    state.drag_start(&hit, Point::new(100.0, 50.0));
    state.drag_update(Point::new(140.0, 80.0));
    state.drag_end(Point::new(140.0, 80.0));

    assert_eq!(state.selection(), Some(SURFACE));
    assert!(state.can_undo(), "the gesture still happened");
}

// Why: a change the IR refuses must cost the user nothing: the figure must be what it
// was, the reason must be said where every other problem is said, and undo must still
// reach the change before it rather than an empty step.
#[test]
fn a_refused_edit_leaves_the_figure_unchanged_with_a_problem_and_no_undo_step() {
    let mut state = state_with_artists();
    state.try_record(&set(
        FLAT.0,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    ));
    let before = state.figure().clone();
    assert_eq!(state.problems(), &[]);

    // Limits that are not increasing are refused by validation.
    let refused = state.try_record(&set(FLAT.0, "x.limits", Value::Limits(manual(4.0, 4.0))));

    assert!(!refused, "a refused edit changes nothing");
    assert_eq!(state.figure(), &before);
    assert_eq!(
        state.overlay().entries().len(),
        1,
        "the refused edit is not in the overlay"
    );
    assert_eq!(state.problems().len(), 1, "the reason is reported");
    assert_eq!(state.problems()[0].origin, Origin::Refused);
    assert_eq!(
        state.problems()[0].path.as_ref().map(ToString::to_string),
        Some("x.limits".to_owned()),
        "the problem names the property"
    );

    assert!(state.undo(), "the change before the refusal is still there");
    assert!(
        !state.can_undo(),
        "the refusal left no step of its own to undo"
    );
}

// Why: dragging a numeric field sends a change on every frame; each one must not be its
// own undo step, or undoing a drag would take dozens of presses, exactly as for a drag on
// the canvas.
#[test]
fn the_edits_of_one_drag_of_a_field_are_one_undo_step() {
    let mut state = state_with_artists();
    let before = state.figure().clone();

    state.begin_edit_step();
    for max in [2.0, 3.0, 4.0] {
        state.try_record(&set(SOLID.0, "x.limits", Value::Limits(manual(-1.0, max))));
    }
    state.end_edit_step();

    assert_eq!(
        limits_of(state.figure(), SOLID.0, Dimension::X),
        manual(-1.0, 4.0)
    );
    assert!(state.undo());
    assert_eq!(state.figure(), &before, "one undo restores the whole drag");
    assert!(!state.can_undo());
}

// Why: the revert control takes back one property; reverting through the figure state
// must remove that entry, leave the others, and show the source value again at once.
#[test]
fn reverting_a_property_removes_its_entry_and_shows_the_source_again() {
    let mut state = state_with_artists();
    state.try_record(&set(
        FLAT.0,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    ));
    state.try_record(&set(LINE.0, "visible", Value::Bool(false)));

    assert!(state.revert(FLAT, &path("colormap")));

    assert_eq!(
        state
            .figure()
            .axes(FLAT)
            .expect("the axes is in the figure")
            .colormap,
        ColormapName::Viridis,
        "the source colormap is shown again"
    );
    assert_eq!(state.overlay().entries().len(), 1, "the other entry stays");
    assert!(
        !state.revert(FLAT, &path("colormap")),
        "there is no second entry to revert"
    );
}

// ---------------------------------------------------------------------------------
// The panel on screen
// ---------------------------------------------------------------------------------

struct PanelHarnessState {
    figure: FigureState,
    panel: PropertyPanel,
}

fn panel_harness(figure: Figure, selected: Option<NodeId>) -> Harness<'static, PanelHarnessState> {
    let mut state = FigureState::new(figure);
    state.select(selected);
    let mut panel = PropertyPanel::default();
    panel.open = true;
    Harness::builder()
        .with_size(egui::vec2(520.0, 700.0))
        .build_ui_state(
            |ui, state: &mut PanelHarnessState| {
                property_panel(ui, &mut state.panel, &mut state.figure);
            },
            PanelHarnessState {
                figure: state,
                panel,
            },
        )
}

fn app_harness(figure: Figure) -> Harness<'static, ironlab_viewer::ViewerApp> {
    Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_eframe(|_cc| {
            ironlab_viewer::ViewerApp::new(vec![("flat.fig".to_owned(), figure)], TEXT.clone())
        })
}

// Why: the panel takes space from the canvas, so a figure must open showing only the
// figure; the toolbar button is the promise that the editor is there when it is wanted.
#[test]
fn the_properties_panel_is_hidden_until_the_toolbar_button_is_clicked() {
    let mut harness = app_harness(figure_with_artists());
    harness.run();

    assert!(
        harness.query_by_label("Speed").is_none(),
        "the object tree is not shown until the panel is opened"
    );

    harness.get_by_label("Properties").click();
    harness.run();

    assert!(
        harness.query_by_label("Speed").is_some(),
        "the panel shows the object tree"
    );

    harness.get_by_label("Properties").click();
    harness.run();
    assert!(harness.query_by_label("Speed").is_none(), "it closes again");
}

/// A position inside the canvas of a 900 × 600 harness while the property editor takes
/// the right-hand side: the figure is scaled to fit what is left, so this point lies
/// inside the plot rectangle of the first axes.
const CANVAS_POINT: egui::Pos2 = egui::pos2(200.0, 340.0);

// Why: the panel must take space from the canvas without taking its input; a gesture that
// stopped working, or worked against the wrong part of the figure, whenever the editor was
// open would make the two unusable together.
#[test]
fn a_gesture_on_the_canvas_still_works_while_the_panel_is_open() {
    let mut harness = app_harness(figure_with_artists());
    harness.run();
    harness.get_by_label("Properties").click();
    harness.run();

    harness.drag_at(CANVAS_POINT);
    harness.run();
    harness.hover_at(CANVAS_POINT + egui::vec2(40.0, 0.0));
    harness.run();
    harness.drop_at(CANVAS_POINT + egui::vec2(40.0, 0.0));
    harness.run();

    let state = harness.state().figure_state(0).expect("one figure");
    assert!(
        state.can_undo(),
        "the drag panned an axes and is one step of the history"
    );
    assert!(
        state
            .overlay()
            .entries()
            .iter()
            .all(|entry| entry.path.to_string().ends_with("limits")),
        "the drag set limits: {:?}",
        state.overlay().entries()
    );
}

// Why: the tree and the inspector are one editor; clicking a node must be what decides
// which node's properties are shown, or the tree would be decoration.
#[test]
fn selecting_a_node_in_the_tree_shows_that_node_in_the_inspector() {
    let mut harness = panel_harness(figure_with_artists(), Some(FIGURE));
    harness.run();

    harness.get_by_label("Measured").click();
    harness.run();

    assert_eq!(harness.state().figure.selection(), Some(LINE));
    assert!(
        harness.query_by_label_contains("Line").is_some(),
        "the inspector names the kind and the identifier of the selected node"
    );
    assert!(
        harness.query_by_label_contains("node 4").is_some(),
        "the inspector names the identifier"
    );
}

// Why: this is the whole point of the editor: a change made in it must reach the overlay
// through the same path as a gesture, change what is drawn, and leave the figure the user
// opened alone so that it can still be restored.
#[test]
fn toggling_a_checkbox_records_one_change_in_the_overlay_and_leaves_the_source_alone() {
    let mut harness = panel_harness(figure_with_artists(), Some(LINE));
    harness.run();

    harness.get_by_label("visible").click();
    harness.run();

    let state = &harness.state().figure;
    assert!(
        !state
            .figure()
            .artist(LINE)
            .expect("the line is in the figure")
            .1
            .visible(),
        "the displayed figure hides the line"
    );
    assert!(
        state
            .source()
            .artist(LINE)
            .expect("the line is in the source")
            .1
            .visible(),
        "the source is untouched"
    );
    assert_eq!(state.overlay().entries().len(), 1);
    assert!(state.can_undo());
}

// Why: a property the user has changed must be distinguishable from one the figure
// defines, and takeable back in one click; a revert control on an unchanged property
// would offer to undo nothing.
#[test]
fn a_revert_control_appears_only_for_a_changed_property_and_takes_it_back() {
    let mut harness = panel_harness(figure_with_artists(), Some(LINE));
    harness.run();
    assert!(
        harness.query_by_label("Revert visible").is_none(),
        "an unchanged property has nothing to revert"
    );

    harness.get_by_label("visible").click();
    harness.run();
    harness.get_by_label("Revert visible").click();
    harness.run();

    let state = &harness.state().figure;
    assert!(
        state
            .figure()
            .artist(LINE)
            .expect("the line is in the figure")
            .1
            .visible(),
        "reverting shows the figure's own value again"
    );
    assert!(state.overlay().entries().is_empty());
}

// Why: parameters are how a figure is found again among hundreds; the editor must be
// able to create one, and must commit the map as a whole so that the change is one entry
// of the overlay and one step of the history.
#[test]
fn adding_a_parameter_records_one_set_of_the_whole_map() {
    let mut harness = panel_harness(figure_with_artists(), Some(FIGURE));
    harness.run();

    harness.get_by_label("Add parameter").click();
    harness.run();

    let state = &harness.state().figure;
    assert_eq!(
        state.overlay().entries().len(),
        1,
        "the whole map is one entry"
    );
    assert_eq!(state.overlay().entries()[0].path.to_string(), "parameters");
    assert_eq!(
        state.figure().parameters.len(),
        1,
        "the figure gains the parameter"
    );
    assert!(
        state.source().parameters.is_empty(),
        "the source is untouched"
    );
}

// Why: the inspector draws whatever the registry lists, so a property type it has no
// editor for, or a value it reads wrongly, shows up only when a node of that kind is
// selected; every kind of node and every kind of value must therefore be drawn at least
// once, and drawing must change nothing by itself.
#[test]
fn the_panel_draws_every_kind_of_node_without_changing_the_figure() {
    let figure = figure_with_every_artist();
    let expected = figure.clone();
    let nodes: Vec<NodeId> = tree_rows(&figure).iter().map(|row| row.node).collect();
    assert_eq!(nodes.len(), 7, "the figure holds every kind of node");

    for node in nodes {
        let mut harness = panel_harness(figure.clone(), Some(node));
        harness.run();
        let state = &harness.state().figure;
        assert_eq!(
            state.figure(),
            &expected,
            "drawing the properties of {node} changed the figure"
        );
        assert!(
            state.overlay().entries().is_empty(),
            "drawing the properties of {node} recorded a change"
        );
        assert!(
            !groups_of(state, node).is_empty(),
            "{node} has properties to show"
        );
    }
}

/// The widget of a given role that holds a given value, such as the text field of an
/// axes title or a combo box showing the choice it has; the run of text inside a widget
/// carries the same value, so the role tells them apart.
fn widget_showing<'t>(
    harness: &'t Harness<'_, PanelHarnessState>,
    role: egui::accesskit::Role,
    value: &'t str,
) -> egui_kittest::Node<'t> {
    use egui_kittest::kittest::NodeT;
    harness
        .get_all_by_value(value)
        .find(|node| node.accesskit_node().role() == role)
        .unwrap_or_else(|| panic!("no {role:?} shows {value:?}"))
}

/// The text field of the title of an axes, found by the text it holds.
fn title_field<'t>(
    harness: &'t Harness<'_, PanelHarnessState>,
    content: &'t str,
) -> egui_kittest::Node<'t> {
    widget_showing(harness, egui::accesskit::Role::TextInput, content)
}

// Why: a text field sends a change on every keystroke, and each one reaches the overlay;
// undoing a title typed in must take back the title, not the last letter. The step is
// held open while the field has the keyboard and must be closed when it is left, even
// when it is left by selecting another object, or every later change would join it.
#[test]
fn typing_into_a_text_field_is_one_undo_step_that_closes_when_the_field_is_left() {
    let mut harness = panel_harness(figure_with_artists(), Some(FLAT));
    harness.run();

    title_field(&harness, "Speed").focus();
    harness.run();
    title_field(&harness, "Speed").type_text("y");
    harness.run();
    title_field(&harness, "Speedy").type_text("!");
    harness.run();

    let title = |state: &FigureState| {
        state
            .figure()
            .axes(FLAT)
            .expect("the axes is in the figure")
            .title
            .clone()
            .map(|text| text.content)
    };
    assert_eq!(
        title(&harness.state().figure),
        Some("Speedy!".to_owned()),
        "every keystroke reached the figure"
    );
    assert_eq!(harness.state().figure.overlay().entries().len(), 1);
    assert!(
        !harness.state().figure.can_undo(),
        "the step is still open while the field has the keyboard"
    );

    // Selecting another object takes the field away, which must close the step.
    harness.get_by_label("Measured").click();
    harness.run();
    assert!(harness.state().figure.can_undo());

    assert!(harness.state_mut().figure.undo());
    assert_eq!(
        title(&harness.state().figure),
        Some("Speed".to_owned()),
        "one undo takes back the whole title"
    );
}

// Why: a combo box is the only way to change a colormap, a scale or the kind of a value,
// and its entries come from the choices the IR offers; if picking one did not commit the
// value behind its label, every such property would be unreachable.
#[test]
fn choosing_from_a_combo_box_commits_the_value_behind_its_label() {
    let mut harness = panel_harness(figure_with_artists(), Some(FLAT));
    harness.run();

    widget_showing(&harness, egui::accesskit::Role::ComboBox, "Viridis").scroll_to_me();
    harness.run();
    widget_showing(&harness, egui::accesskit::Role::ComboBox, "Viridis").click();
    harness.run();
    harness.run();
    harness.get_by_label("Gray").click();
    harness.run();

    let state = &harness.state().figure;
    assert_eq!(
        state
            .figure()
            .axes(FLAT)
            .expect("the axes is in the figure")
            .colormap,
        ColormapName::Gray
    );
    assert_eq!(state.overlay().entries().len(), 1);
    assert_eq!(state.overlay().entries()[0].path.to_string(), "colormap");
    assert_eq!(
        state
            .source()
            .axes(FLAT)
            .expect("the axes is in the source")
            .colormap,
        ColormapName::Viridis,
        "the source keeps its colormap"
    );
}

// Why: a choice the figure cannot act on is listed so that the user can see it exists and
// read what would make it available, which is worth nothing if clicking it changes the
// figure anyway. The entry must therefore be drawn disabled and must commit nothing.
#[test]
fn a_choice_that_cannot_be_taken_is_disabled_in_the_combo_box_and_commits_nothing() {
    let mut harness = panel_harness(figure_with_artists(), Some(LINE));
    harness.run();

    widget_showing(&harness, egui::accesskit::Role::ComboBox, "Automatic").scroll_to_me();
    harness.run();
    widget_showing(&harness, egui::accesskit::Role::ComboBox, "Automatic").click();
    harness.run();
    harness.run();

    {
        let colormapped = harness.get_by_label("Colormapped");
        assert!(
            colormapped.accesskit_node().is_disabled(),
            "a line cannot be coloured by the colormap, so the entry must be disabled"
        );
        assert!(
            !harness
                .get_by_label("Fixed colour")
                .accesskit_node()
                .is_disabled(),
            "a colour the line can take must stay live in the same list"
        );
        colormapped.click();
    }
    harness.run();

    let state = &harness.state().figure;
    assert!(
        state.overlay().entries().is_empty(),
        "clicking a disabled choice changed the figure: {:?}",
        state.overlay().entries()
    );
    assert!(!state.can_undo(), "nothing was recorded to undo");
}

// ---------------------------------------------------------------------------------
// Properties that the panel shows but does not change
// ---------------------------------------------------------------------------------

/// The row of a group whose label is `label`, or a panic naming the labels there are.
fn row<'a>(groups: &'a [PropertyGroup], group_name: &str, label: &str) -> &'a PropertyRow {
    let found = group(groups, group_name);
    found
        .rows
        .iter()
        .find(|row| row.label == label)
        .unwrap_or_else(|| {
            let labels: Vec<&str> = found.rows.iter().map(|row| row.label.as_str()).collect();
            panic!("the group {group_name:?} has no row {label:?}; it has {labels:?}")
        })
}

/// The labels a property offers in its combo box, or a panic when it offers none.
fn offered(groups: &[PropertyGroup], group_name: &str, label: &str) -> Vec<&'static str> {
    match &row(groups, group_name, label).editor {
        Editor::Choice { offered } => offered.iter().map(|choice| choice.label).collect(),
        other => panic!("{group_name}.{label} is not chosen from a list: {other:?}"),
    }
}

/// One choice a property offers, found by its label, or a panic naming the choices there
/// are.
fn choice<'a>(
    groups: &'a [PropertyGroup],
    group_name: &str,
    label: &str,
    choice_label: &str,
) -> &'a Choice {
    match &row(groups, group_name, label).editor {
        Editor::Choice { offered } => offered
            .iter()
            .find(|choice| choice.label == choice_label)
            .unwrap_or_else(|| {
                let labels: Vec<&str> = offered.iter().map(|choice| choice.label).collect();
                panic!("{group_name}.{label} offers no {choice_label:?}; it offers {labels:?}")
            }),
        other => panic!("{group_name}.{label} is not chosen from a list: {other:?}"),
    }
}

// Why: a colormapped colour promises that the plot is coloured by its data, and the scene
// compiler can keep that promise only where the IR gives it a value to look the colour up
// by. Removing the choice where it cannot be honoured would hide from the user that data
// colouring exists at all, so it is listed everywhere and marked where it cannot be taken;
// marking it where a surface face uses it would take away the reason a surface is drawn in
// colour.
#[test]
fn a_colormapped_colour_is_listed_everywhere_and_available_only_where_it_colours_by_data() {
    let state = FigureState::new(figure_with_every_artist());
    let (line, scatter, contour, quiver, surface) =
        (NodeId(3), NodeId(4), NodeId(5), NodeId(6), NodeId(7));

    for (node, group_name, label) in [
        (line, "line", "color"),
        (line, "marker", "face"),
        (line, "marker", "edge"),
        (scatter, "marker", "face"),
        (scatter, "marker", "edge"),
        (quiver, "line", "color"),
    ] {
        let labels = offered(&groups_of(&state, node), group_name, label);
        assert!(
            labels.contains(&"Colormapped"),
            "{group_name}.{label} of node {node} hides a colour instead of disabling it: \
             {labels:?}"
        );
        assert!(
            labels.contains(&"Fixed colour") && labels.contains(&"Automatic"),
            "{group_name}.{label} of node {node} lost the colours it can use: {labels:?}"
        );
        let reason = choice(&groups_of(&state, node), group_name, label, "Colormapped")
            .unavailable
            .unwrap_or_else(|| {
                panic!("{group_name}.{label} of node {node} offers a colour it cannot use")
            });
        assert!(
            reason.contains("colormap") && reason.ends_with('.'),
            "{group_name}.{label} of node {node} gives no usable reason: {reason}"
        );
    }

    for (node, group_name, label) in [
        (contour, "line", "color"),
        (surface, "face", ""),
        (surface, "edge", ""),
    ] {
        assert!(
            choice(&groups_of(&state, node), group_name, label, "Colormapped").available(),
            "{group_name}.{label} of node {node} cannot be coloured by its data"
        );
    }
}

// Why: a scatter is the one plot whose markers the IR does colour by data, through the
// array named by its colour; marking the colormapped colour specification unavailable
// must not touch that choice, or a scatter could no longer be coloured by value.
#[test]
fn a_scatter_still_offers_its_colour_from_data() {
    let state = FigureState::new(figure_with_every_artist());
    let groups = groups_of(&state, NodeId(4));
    assert_eq!(
        offered(&groups, "color", ""),
        ["Single colour", "From data"]
    );
    assert!(
        choice(&groups, "color", "", "From data").available(),
        "a scatter's colour from data must stay available"
    );
}

// Why: the tile layout is the frame the program placed its axes in, and the editor
// changes the properties of those axes rather than the structure around them. Showing a
// control that adds a row would offer a figure the panel cannot finish making.
#[test]
fn the_tile_layout_is_shown_read_only_and_says_why() {
    let state = state_with_artists();
    let groups = groups_of(&state, FIGURE);
    for label in ["rows", "cols"] {
        let row = row(&groups, "layout", label);
        let Editor::ReadOnly { reason } = row.editor else {
            panic!("layout.{label} is editable: {:?}", row.editor);
        };
        assert!(
            reason.contains("program that builds the figure"),
            "the reason must say where the layout comes from: {reason}"
        );
    }
    assert_eq!(
        read_only_reason(NodeKind::Figure, &path("layout.rows")),
        match row(&groups, "layout", "rows").editor {
            Editor::ReadOnly { reason } => Some(reason),
            _ => None,
        },
        "the reason the panel shows is the one the inspector states"
    );
}

// Why: moving an axes from one cell of the layout to another is a change to the axes, not
// to the structure around it, and a cell outside the layout is caught by validation; a
// read-only cell would make the panel unable to rearrange a figure at all.
#[test]
fn the_cell_of_an_axes_stays_editable() {
    let state = state_with_artists();
    for label in ["row", "col", "row_span", "col_span"] {
        assert!(
            read_only_reason(NodeKind::Axes, &path(&format!("cell.{label}"))).is_none(),
            "cell.{label} must stay editable"
        );
        assert!(
            matches!(
                row(&groups_of(&state, FLAT), "cell", label).editor,
                Editor::Number { .. }
            ),
            "cell.{label} must have a control"
        );
    }
}

// Why: a read-only row with nothing to say for itself looks like a control that is
// broken; the user must be able to find out why the value cannot be changed here.
#[test]
fn every_read_only_property_carries_a_reason() {
    let state = FigureState::new(figure_with_every_artist());
    let mut seen = 0;
    for node in (1..=7).map(NodeId) {
        for group in groups_of(&state, node) {
            for row in group.rows {
                if let Editor::ReadOnly { reason } = row.editor {
                    seen += 1;
                    assert!(
                        reason.len() > 40 && reason.ends_with('.'),
                        "the reason for {} is not a sentence: {reason:?}",
                        row.path
                    );
                }
            }
        }
    }
    assert!(seen > 0, "the figure has read-only properties to check");
}

// ---------------------------------------------------------------------------------
// Taking back every change
// ---------------------------------------------------------------------------------

/// A state with one change of each kind the viewer records: a limit, a visibility and a
/// property edited in the panel.
fn state_with_three_changes() -> FigureState {
    let mut state = state_with_artists();
    record_limits(&mut state, FLAT.0, Dimension::X, manual(2.0, 3.0));
    state.try_record(&set(LINE.0, "visible", Value::Bool(false)));
    state.try_record(&set(
        FLAT.0,
        "colormap",
        Value::ColormapName(ColormapName::Gray),
    ));
    assert_eq!(state.change_count(), 3, "three changes of three kinds");
    state
}

// Why: the control discards changes of every kind, not only the view ones that Reset view
// discards; a user who wants the figure back as its program defined it must get exactly
// that, with the program's own figure untouched.
#[test]
fn reverting_all_changes_discards_every_kind_of_change_and_leaves_the_source() {
    let mut state = state_with_three_changes();
    let source = state.source().clone();
    assert_ne!(state.figure(), &source, "precondition: the figure differs");

    assert!(state.revert_all());

    assert_eq!(state.change_count(), 0);
    assert_eq!(state.figure(), &source, "the figure its program defined");
    assert_eq!(state.source(), &source, "which was never touched");
    assert!(!state.revert_all(), "there is nothing left to discard");
}

// Why: these changes are how one user was looking at a figure rather than anything in the
// figure, so taking them all back is a clean slate: pressing undo straight afterwards must
// do nothing, because no action has been taken since.
#[test]
fn reverting_all_changes_leaves_nothing_to_undo_or_redo() {
    let mut state = state_with_three_changes();
    state.undo();
    assert!(
        state.can_undo() && state.can_redo(),
        "precondition: a history in both directions"
    );

    state.revert_all();

    assert!(!state.can_undo());
    assert!(!state.can_redo());
    assert!(!state.undo());
    assert!(!state.redo());
}

// Why: the control says how much it would throw away, so that a user knows what is at
// stake before clicking, and says nothing to throw away when there is none.
#[test]
fn the_revert_all_control_counts_the_changes_it_would_discard() {
    assert_eq!(revert_all_label(0), "Revert all changes");
    assert_eq!(revert_all_label(1), "Revert all changes (1)");
    assert_eq!(revert_all_label(7), "Revert all changes (7)");
}

// Why: a control that is enabled but does nothing teaches the user to distrust it; the
// panel must offer the control only when there is something for it to do, and must say
// how much that is.
#[test]
fn the_revert_all_control_is_disabled_until_there_is_a_change() {
    let mut harness = panel_harness(figure_with_artists(), Some(LINE));
    harness.run();
    assert!(
        harness
            .get_by_label(&revert_all_label(0))
            .accesskit_node()
            .is_disabled(),
        "nothing has been changed yet"
    );

    harness.get_by_label("visible").click();
    harness.run();

    assert!(harness.query_by_label(&revert_all_label(0)).is_none());
    assert!(
        !harness
            .get_by_label(&revert_all_label(1))
            .accesskit_node()
            .is_disabled()
    );
}

// Why: the control is the user's way out of a session of experimenting; clicking it must
// restore the figure the program defined, in the panel as well as in the state.
#[test]
fn clicking_revert_all_in_the_panel_restores_the_figure_the_program_defined() {
    let mut harness = panel_harness(figure_with_artists(), Some(LINE));
    harness.run();
    harness.get_by_label("visible").click();
    harness.run();
    let source = harness.state().figure.source().clone();
    assert_ne!(harness.state().figure.figure(), &source);

    harness.get_by_label(&revert_all_label(1)).click();
    harness.run();

    let state = &harness.state().figure;
    assert_eq!(state.figure(), &source);
    assert_eq!(state.change_count(), 0);
    assert!(!state.can_undo(), "the clean slate leaves nothing to undo");
}
