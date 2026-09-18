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
use ironlab_ir::{
    Choice, ColormapName, Dimension, Figure, NodeId, NodeKind, Parameter, Projection, Value, View3d,
};
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::{HitMap, LegendHit};
use ironlab_viewer::inspector::{
    Editor, ParameterKind, ParametersDraft, PropertyGroup, PropertyRow, commit, is_shown,
    property_groups, read_only_reason, tree_rows,
};
use ironlab_viewer::panel::{FOOTER_ID, OBJECT_TREE_ID, revert_all_label};
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
            (FLAT, 1, "Axes (Speed)"),
            (LINE, 2, "Line (Measured)"),
            (SCATTER, 2, "Scatter"),
            (SOLID, 1, "Axes (row 0, col 1)"),
            (SURFACE, 2, "Surface"),
        ],
        "every row names its kind first, and the name of the object itself follows in \
         brackets when it has one"
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

// Why: the tree mixes objects of six kinds, and a title such as "Speed" says nothing
// about what kind of object carries it. Naming the kind first, and the object's own name
// after it in brackets, is what makes a row readable on its own; this pins that rule for
// every kind of node, for a titled and an untitled object of each kind that can have a
// title, and for a name written as LaTeX, which the tree shows as its source because it
// does not typeset it.
#[test]
fn every_row_names_its_kind_first_and_its_own_name_in_brackets() {
    use ironlab_ir::{Artist, Axes, Cell, Contour, Line, Quiver, Scatter, Surface, Text};

    let titled = Axes {
        id: NodeId(2),
        title: Some(Text::plain("Speed")),
        artists: vec![
            Artist::Line(Line {
                id: NodeId(3),
                display_name: Some(Text::new(r"$\sin \omega t$")),
                ..Line::default()
            }),
            Artist::Scatter(Scatter {
                id: NodeId(4),
                display_name: Some(Text::plain("Samples")),
                ..Scatter::default()
            }),
            Artist::Contour(Contour {
                id: NodeId(5),
                ..Contour::default()
            }),
            Artist::Quiver(Quiver {
                id: NodeId(6),
                display_name: Some(Text::plain("Wind")),
                ..Quiver::default()
            }),
            Artist::Surface(Surface {
                id: NodeId(7),
                ..Surface::default()
            }),
        ],
        ..Axes::default()
    };
    let untitled = Axes {
        id: NodeId(8),
        cell: Cell {
            row: 1,
            col: 2,
            ..Cell::default()
        },
        ..Axes::default()
    };
    let mut figure = Figure {
        id: NodeId(1),
        title: Some(Text::plain("A damped oscillator")),
        axes: vec![titled, untitled],
        ..Figure::new()
    };

    let labels: Vec<String> = tree_rows(&figure)
        .into_iter()
        .map(|row| row.label)
        .collect();
    assert_eq!(
        labels,
        [
            "Figure (A damped oscillator)",
            "Axes (Speed)",
            r"Line ($\sin \omega t$)",
            "Scatter (Samples)",
            "Contour",
            "Quiver (Wind)",
            "Surface",
            "Axes (row 1, col 2)",
        ],
        "a title or display name is shown in brackets after the kind, as its source; an \
         axes with no title shows its cell instead, and a figure or artist with no name \
         shows its kind alone"
    );

    figure.title = None;
    assert_eq!(
        tree_rows(&figure)
            .first()
            .expect("the figure is a row")
            .label,
        "Figure",
        "a figure with no title is named by its kind alone, with no empty brackets"
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

// Why: a two-dimensional axes ignores its z axis altogether, so a label, a scale, limits
// and grid lines shown for it would be controls that change nothing on screen. The rule
// reads the projection rather than the path, so promoting the axes to three dimensions
// must bring the rows back; otherwise the panel would be the one place a z axis could not
// be set up.
#[test]
fn the_z_axis_is_shown_only_while_the_axes_is_three_dimensional() {
    let mut state = state_with_artists();

    let flat = row_paths(&groups_of(&state, FLAT));
    assert!(
        !flat.iter().any(|row| row.starts_with('z')),
        "a 2D axes shows nothing of its z axis: {flat:?}"
    );
    assert!(
        flat.contains(&"x/limits".to_owned()) && flat.contains(&"y/limits".to_owned()),
        "the axes it does draw are still shown: {flat:?}"
    );
    for label in ["label", "scale", "limits", "grid"] {
        assert!(
            !is_shown(
                state.figure(),
                FLAT,
                NodeKind::Axes,
                &path(&format!("z.{label}"))
            ),
            "z.{label} is hidden while the axes is two-dimensional"
        );
    }

    let solid = row_paths(&groups_of(&state, SOLID));
    for row in ["z/label", "z/scale", "z/limits", "z/limits.min", "z/grid"] {
        assert!(
            solid.contains(&(*row).to_owned()),
            "a 3D axes shows {row}: {solid:?}"
        );
    }

    // Promoting the flat axes must reveal exactly the rows that were hidden, because the
    // rule is asked afresh for every frame rather than fixed when the figure was opened.
    assert!(state.try_record(&set(
        FLAT.0,
        "projection",
        Value::Projection(Projection::ThreeD {
            view3d: View3d::default()
        })
    )));
    let promoted = row_paths(&groups_of(&state, FLAT));
    for row in ["z/label", "z/scale", "z/limits", "z/grid"] {
        assert!(
            promoted.contains(&(*row).to_owned()),
            "promoting the axes shows {row}: {promoted:?}"
        );
    }
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

/// The size of the window the panel is driven in, which is that of a window a user would
/// work in rather than one just big enough for the panel.
const WINDOW: egui::Vec2 = egui::vec2(520.0, 700.0);

fn panel_harness(figure: Figure, selected: Option<NodeId>) -> Harness<'static, PanelHarnessState> {
    let mut state = FigureState::new(figure);
    state.select(selected);
    sized_panel_harness(state, WINDOW)
}

/// The panel, open on `state`, in a window of `size`.
fn sized_panel_harness(
    state: FigureState,
    size: egui::Vec2,
) -> Harness<'static, PanelHarnessState> {
    let mut panel = PropertyPanel::default();
    panel.open = true;
    Harness::builder().with_size(size).build_ui_state(
        |ui, state: &mut PanelHarnessState| {
            property_panel(ui, &mut state.panel, &mut state.figure);
        },
        PanelHarnessState {
            figure: state,
            panel,
        },
    )
}

/// The checkbox of the boolean property named `name`. The name is drawn beside the
/// checkbox, in the left column of the row, so the accessibility tree holds it twice: as
/// the label that is read and as the name of the control it names.
fn checkbox<'t>(
    harness: &'t Harness<'_, PanelHarnessState>,
    name: &'t str,
) -> egui_kittest::Node<'t> {
    harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, name)
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
        harness.query_by_label("Axes (Speed)").is_none(),
        "the object tree is not shown until the panel is opened"
    );

    harness.get_by_label("Properties").click();
    harness.run();

    assert!(
        harness.query_by_label("Axes (Speed)").is_some(),
        "the panel shows the object tree"
    );

    harness.get_by_label("Properties").click();
    harness.run();
    assert!(
        harness.query_by_label("Axes (Speed)").is_none(),
        "it closes again"
    );
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

    harness.get_by_label("Line (Measured)").click();
    harness.run();

    assert_eq!(harness.state().figure.selection(), Some(LINE));
    assert!(
        harness.query_by_label_contains("node 4").is_some(),
        "the inspector names the kind and the identifier of the selected node"
    );
}

// Why: this is the whole point of the editor: a change made in it must reach the overlay
// through the same path as a gesture, change what is drawn, and leave the figure the user
// opened alone so that it can still be restored.
#[test]
fn toggling_a_checkbox_records_one_change_in_the_overlay_and_leaves_the_source_alone() {
    let mut harness = panel_harness(figure_with_artists(), Some(LINE));
    harness.run();

    checkbox(&harness, "visible").click();
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

    checkbox(&harness, "visible").click();
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
    harness.get_by_label("Line (Measured)").click();
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

// Why: a scatter takes the size of its markers from its own size property, so the size in
// its marker style changes nothing; a control that does nothing is worse than a locked row
// that says where the size comes from. Every other artist draws its markers at that size,
// so locking it for them would remove the only way to set it.
#[test]
fn the_marker_size_of_a_scatter_is_read_only_and_that_of_a_line_is_not() {
    let state = FigureState::new(figure_with_every_artist());
    let (line, scatter) = (NodeId(3), NodeId(4));

    let Editor::ReadOnly { reason } = row(&groups_of(&state, scatter), "marker", "size_pt").editor
    else {
        panic!(
            "a scatter's marker size is editable: {:?}",
            row(&groups_of(&state, scatter), "marker", "size_pt").editor
        );
    };
    assert!(
        reason.contains("the row named size"),
        "the reason must send the user to the row that does set the size: {reason}"
    );
    assert_eq!(
        read_only_reason(NodeKind::Scatter, &path("marker.size_pt")),
        Some(reason),
        "the reason the panel shows is the one the inspector states"
    );

    assert!(
        read_only_reason(NodeKind::Line, &path("marker.size_pt")).is_none(),
        "a line draws its markers at the size of its marker style"
    );
    assert!(
        matches!(
            row(&groups_of(&state, line), "marker", "size_pt").editor,
            Editor::Number { .. }
        ),
        "a line's marker size must keep its control"
    );
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

    checkbox(&harness, "visible").click();
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
    checkbox(&harness, "visible").click();
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

// ---------------------------------------------------------------------------------
// The shape of the panel
// ---------------------------------------------------------------------------------

/// The rectangle egui gave a part of the panel in the last frame it drew.
fn part_rect(harness: &Harness<'_, PanelHarnessState>, id: &'static str) -> egui::Rect {
    egui::PanelState::load(&harness.ctx, egui::Id::new(id))
        .unwrap_or_else(|| panic!("the panel drew no part under {id:?}"))
        .outer_rect
}

/// Draws the panel for a few seconds of frames without touching it, as a user does while
/// reading it. A part of the panel that takes its size from the size it was given last
/// creeps by a little each frame, so a layout fault of that kind shows only after the
/// panel has been drawn many times.
fn settle(harness: &mut Harness<'_, PanelHarnessState>) {
    for _ in 0..200 {
        harness.run();
    }
}

/// The height the object tree is given when there is room for it, which a window too
/// short for it must take back.
const TREE_HEIGHT: f32 = 180.0;

/// The artist of [`figure_with_many_axes`] that the tests of the panel's shape select:
/// the first plot of its first axes, which is a line and so has a visibility to toggle.
const CROWDED_LINE: NodeId = NodeId(3);

/// The state of a figure with one node selected.
fn selected_state(figure: Figure, node: NodeId) -> FigureState {
    let mut state = FigureState::new(figure);
    state.select(Some(node));
    state
}

/// Renames every axes and every artist but `spared`, so that the foot of the panel counts
/// many changes, and returns how many it counts.
fn change_every_node_but(state: &mut FigureState, spared: NodeId) -> usize {
    for row in tree_rows(state.figure()) {
        if row.node == spared || row.kind == NodeKind::Figure {
            continue;
        }
        let name = Value::Text(ironlab_ir::Text::plain(format!("Renamed {}", row.node.0)));
        let property = if row.kind == NodeKind::Axes {
            "title"
        } else {
            "display_name"
        };
        assert!(
            state.try_record(&set(row.node.0, property, name)),
            "the figure accepts a new {property} for {}",
            row.node
        );
    }
    let changes = state.change_count();
    assert!(
        changes >= 10,
        "the foot must have a large count to show: {changes}"
    );
    changes
}

// Why: the foot holds the control that discards every change, and that control names how
// many changes there are. The foot is also all that stands between the inspector and the
// bottom of the panel, so a foot that takes its height from what it holds steals the
// inspector's room whenever the label, the count or the width it is given changes — and
// a foot laid out from the height it was given last grows every frame until the inspector
// has none at all. The foot must therefore occupy the same strip whatever it says, in a
// roomy window and in a cramped one, for as long as the panel is drawn.
#[test]
fn the_foot_of_the_panel_keeps_one_height_whatever_it_counts() {
    for size in [WINDOW, egui::vec2(300.0, 360.0)] {
        let mut empty =
            sized_panel_harness(selected_state(figure_with_many_axes(), CROWDED_LINE), size);
        let mut counted = {
            let mut state = selected_state(figure_with_many_axes(), CROWDED_LINE);
            change_every_node_but(&mut state, CROWDED_LINE);
            sized_panel_harness(state, size)
        };
        empty.run();
        counted.run();

        let height = part_rect(&empty, FOOTER_ID).height();
        assert!(height > 0.0, "the foot is drawn in a window of {size:?}");
        assert_eq!(
            part_rect(&counted, FOOTER_ID).height(),
            height,
            "the foot is the same strip whether it counts no changes or many, in a \
             window of {size:?}"
        );

        settle(&mut empty);
        settle(&mut counted);
        for harness in [&empty, &counted] {
            assert_eq!(
                part_rect(harness, FOOTER_ID).height(),
                height,
                "the foot is the same strip however long it is drawn, in a window of {size:?}"
            );
        }
    }
}

// Why: the inspector is what the panel is for, and the foot is drawn after it, so a foot
// that grows covers it. A row the user cannot see, or can see but not click because
// something is drawn over it, is a property that cannot be edited at all — which is what
// the panel does. Both must hold whether the user has changed nothing yet or a great
// deal.
#[test]
fn a_property_row_stays_uncovered_and_clickable_however_many_changes_there_are() {
    for many in [false, true] {
        let mut state = selected_state(figure_with_many_axes(), CROWDED_LINE);
        let changes = if many {
            change_every_node_but(&mut state, CROWDED_LINE)
        } else {
            0
        };
        let mut harness = sized_panel_harness(state, WINDOW);
        settle(&mut harness);

        let foot = part_rect(&harness, FOOTER_ID);
        let row = checkbox(&harness, "visible").rect();
        assert!(
            row.height() > 0.0 && row.max.y <= foot.min.y,
            "the row of the line's visibility is drawn whole, above the foot of the \
             panel: the row is {row:?} and the foot is {foot:?} (with {changes} changes)"
        );
        assert!(
            harness.query_by_label(&revert_all_label(changes)).is_some(),
            "the foot counts the {changes} changes while the row is reached"
        );

        checkbox(&harness, "visible").click();
        harness.run();

        let state = &harness.state().figure;
        assert_eq!(
            state.overlay().entries().len(),
            changes + 1,
            "clicking the row reached the checkbox and recorded one change (with \
             {changes} changes already made)"
        );
        assert!(
            !state
                .figure()
                .artist(CROWDED_LINE)
                .expect("the line is in the figure")
                .1
                .visible(),
            "the click hid the line"
        );
    }
}

// Why: the object tree grows with the figure, and a figure of many axes and artists fills
// it. The tree and the inspector divide the height the foot leaves them, so a tree that
// took as much as its rows asked for would leave the inspector nothing and the properties
// of the node the user just selected in the tree would be unreachable. The tree must
// scroll within its share instead.
#[test]
fn a_figure_of_many_axes_still_leaves_the_inspector_its_room() {
    let figure = figure_with_many_axes();
    let rows = tree_rows(&figure).len();
    assert!(rows >= 30, "the tree of this figure is long: {rows} rows");
    let mut harness = sized_panel_harness(selected_state(figure, CROWDED_LINE), WINDOW);
    settle(&mut harness);

    let tree = part_rect(&harness, OBJECT_TREE_ID);
    let foot = part_rect(&harness, FOOTER_ID);
    let inspector = foot.min.y - tree.max.y;
    let row = checkbox(&harness, "visible").rect();
    assert!(
        row.min.y >= tree.max.y && row.max.y <= foot.min.y,
        "a property of the selected artist is drawn between the tree and the foot: the \
         row is {row:?}, the tree ends at {} and the foot starts at {}",
        tree.max.y,
        foot.min.y
    );
    assert!(
        inspector >= 4.0 * row.height(),
        "the inspector keeps room for several properties: {inspector} points for rows \
         of {} points",
        row.height()
    );
    assert!(
        tree.height() <= 0.5 * (foot.max.y - tree.min.y),
        "the object tree takes at most half of the panel however many nodes there are: \
         {} points of {}",
        tree.height(),
        foot.max.y - tree.min.y
    );
}

// Why: a window can be too short for the tree's usual height and the inspector's room
// both, and the user is then reading the properties of the object just selected. The tree
// must be the part that gives way, because it scrolls and because the inspector below it
// is what the panel is for; a tree that kept its height would leave the properties a strip
// too thin to use.
#[test]
fn a_short_window_takes_the_room_from_the_object_tree_rather_than_the_inspector() {
    let short = egui::vec2(300.0, 260.0);
    let mut harness =
        sized_panel_harness(selected_state(figure_with_many_axes(), CROWDED_LINE), short);
    settle(&mut harness);

    let tree = part_rect(&harness, OBJECT_TREE_ID);
    let foot = part_rect(&harness, FOOTER_ID);
    let row = checkbox(&harness, "visible").rect();
    assert!(
        tree.height() < TREE_HEIGHT,
        "the tree gave up part of its usual height: {} points",
        tree.height()
    );
    assert!(
        row.min.y >= tree.max.y && row.max.y <= foot.min.y,
        "a property row is still drawn between the tree and the foot: the row is \
         {row:?}, the tree ends at {} and the foot starts at {}",
        tree.max.y,
        foot.min.y
    );
    assert!(
        foot.min.y - tree.max.y >= 4.0 * row.height(),
        "the inspector still keeps room for several properties: {} points for rows of \
         {} points",
        foot.min.y - tree.max.y,
        row.height()
    );
}

// ---------------------------------------------------------------------------------
// The shape of a property row
// ---------------------------------------------------------------------------------

/// The rectangles of every widget the accessibility tree labels `label`, in the order it
/// lists them, or a panic when it labels none.
fn labelled_rects(harness: &Harness<'_, PanelHarnessState>, label: &str) -> Vec<egui::Rect> {
    harness
        .get_all_by_label(label)
        .map(|node| node.rect())
        .collect()
}

/// The rectangle of the name drawn in the left column of a property row, told apart by
/// its role from the control of the same name that may stand beside it.
fn name_rect(harness: &Harness<'_, PanelHarnessState>, label: &str) -> egui::Rect {
    harness
        .get_by_role_and_label(egui::accesskit::Role::Label, label)
        .rect()
}

// Why: a property row is read across three columns — what the property is called, what it
// is set to, and the control that takes back a change to it — and the panel is scanned
// down those columns rather than along one row. A name that begins wherever the row
// before it happened to end tells the reader nothing about what is nested under what, so
// every name of one depth must begin at one left edge, and a nested property must begin
// further right than the property it belongs to.
#[test]
fn every_property_name_of_one_depth_begins_at_the_same_left_edge() {
    let mut harness = panel_harness(figure_with_artists(), Some(SOLID));
    harness.run();

    // The x, y and z axes of the selected axes each hold a scale and manual limits, so
    // each of these names is drawn three times, once in each group.
    let shallow = labelled_rects(&harness, "scale");
    let deep = labelled_rects(&harness, "limits.min");
    assert_eq!(shallow.len(), 3, "the three axes each name their scale");
    assert_eq!(deep.len(), 3, "the three axes each name the lower bound");

    let left = shallow[0].min.x;
    for rect in &shallow {
        assert!(
            (rect.min.x - left).abs() < 0.5,
            "every property of one depth begins at one left edge: {shallow:?}"
        );
    }
    let nested = deep[0].min.x;
    for rect in &deep {
        assert!(
            (rect.min.x - nested).abs() < 0.5,
            "every nested property of one depth begins at one left edge: {deep:?}"
        );
    }
    assert!(
        nested > left + 8.0,
        "a property nested below another begins visibly further right: {nested} against \
         {left}"
    );
}

// Why: the headings gather thirty-odd properties into the values they belong to, which is
// the only structure the inspector has. A heading drawn smaller than the properties under
// it is the hardest text in the panel to read and the least like a heading, and a heading
// indented as though it were itself a property hides which rows belong to it. It must
// therefore be read at the body size and sit at the left edge of the rows, with the
// properties it gathers indented beneath it.
#[test]
fn a_group_heading_is_read_at_the_body_size_at_the_left_edge_of_the_rows() {
    let mut harness = panel_harness(figure_with_artists(), Some(SOLID));
    harness.run();

    let heading = name_rect(&harness, "x");
    let title = name_rect(&harness, "title");
    let inspector = harness.get_by_label_contains("node 3").rect();
    let inside = labelled_rects(&harness, "scale")[0];

    assert!(
        (heading.min.x - title.min.x).abs() < 0.5,
        "a heading begins where a property that belongs to no group begins: the heading \
         is {heading:?} and the property is {title:?}"
    );
    assert!(
        inside.min.x > heading.min.x + 8.0,
        "a property gathered under a heading is indented below it: {inside:?} under \
         {heading:?}"
    );
    assert!(
        (heading.height() - inspector.height()).abs() < 0.5,
        "a heading is drawn at the size of the body text, as the heading of the \
         inspector is: {} points against {} points",
        heading.height(),
        inspector.height()
    );
}

// Why: the controls are compared with one another down the panel — which axis is
// logarithmic, which plot is hidden — and a column of controls that begins at a different
// place on every row cannot be compared at a glance. Every control therefore ends at one
// right edge, whatever it is; a checkbox that carried its own label would sit at the left
// of its row instead, breaking that column exactly where a property is easiest to change
// by mistake.
#[test]
fn every_control_of_a_node_ends_at_one_right_edge_including_a_checkbox() {
    let mut harness = panel_harness(figure_with_artists(), Some(LINE));
    harness.run();

    let control = checkbox(&harness, "visible").rect();
    let numbers: Vec<egui::Rect> = harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .map(|node| node.rect())
        .collect();
    assert!(
        !numbers.is_empty(),
        "the line has numeric properties to line the checkbox up with"
    );
    for rect in &numbers {
        assert!(
            (rect.max.x - control.max.x).abs() < 1.0,
            "the checkbox ends where the numeric controls end: the checkbox is \
             {control:?} and the number is {rect:?}"
        );
    }
    assert!(
        control.min.x > name_rect(&harness, "visible").max.x,
        "the checkbox is drawn in the control column, to the right of its name"
    );
}

// Why: the control that takes back a change appears only on a row the user has changed.
// If it took its room from the row when it appeared, every control above and below would
// shift sideways the moment a property was edited, which is the moment the user is
// reading them most closely. The column it occupies is therefore reserved on every row,
// and the revert controls form a straight column of their own at the right.
#[test]
fn the_revert_column_is_reserved_so_a_control_stays_put_when_it_is_overridden() {
    let mut harness = panel_harness(figure_with_artists(), Some(LINE));
    harness.run();

    let before = checkbox(&harness, "visible").rect();
    assert!(
        harness.query_by_label("Revert visible").is_none(),
        "precondition: the property has not been changed yet"
    );

    checkbox(&harness, "visible").click();
    harness.run();

    let after = checkbox(&harness, "visible").rect();
    assert!(
        (after.min.x - before.min.x).abs() < 0.5 && (after.max.x - before.max.x).abs() < 0.5,
        "the control did not move when the property became overridden: {before:?} then \
         {after:?}"
    );
    let revert = harness.get_by_label("Revert visible").rect();
    assert!(
        revert.min.x >= after.max.x,
        "the revert control sits in its own column to the right of every control: \
         {revert:?} against {after:?}"
    );
}

// Why: the panel can be dragged narrow, and the names are what make the rows findable at
// all: a truncated name says nothing, while a control that has less room to draw itself
// in is still the same control. The room must therefore come out of the controls.
#[test]
fn a_narrow_panel_takes_the_room_from_the_controls_rather_than_the_names() {
    let mut wide = sized_panel_harness(selected_state(figure_with_artists(), LINE), WINDOW);
    let mut narrow = sized_panel_harness(
        selected_state(figure_with_artists(), LINE),
        egui::vec2(260.0, 700.0),
    );
    wide.run();
    narrow.run();

    let wide_name = name_rect(&wide, "visible");
    let narrow_name = name_rect(&narrow, "visible");
    let wide_control = checkbox(&wide, "visible").rect();
    let narrow_control = checkbox(&narrow, "visible").rect();

    assert!(
        (narrow_name.width() - wide_name.width()).abs() < 0.5,
        "the name column keeps its width in a narrow panel: {narrow_name:?} against \
         {wide_name:?}"
    );
    assert!(
        narrow_control.max.x - narrow_name.max.x < wide_control.max.x - wide_name.max.x,
        "the control column is what gives way: {} points against {} points",
        narrow_control.max.x - narrow_name.max.x,
        wide_control.max.x - wide_name.max.x
    );
}
