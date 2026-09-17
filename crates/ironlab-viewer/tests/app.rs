//! The toolbar and the eframe application, driven through egui_kittest's accessibility tree.

mod common;

use common::*;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ironlab_ir::{Dimension, NodeId};
use ironlab_scene::SceneWarning;
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::HitMap;
use ironlab_viewer::{FigureState, Tool, ViewerApp, toolbar};

const PLOT: Rect = Rect::new(50.0, 20.0, 200.0, 100.0);

/// The state a toolbar harness owns: the figure state, the warnings shown, and whether export was ever requested.
struct ToolbarHarnessState {
    figure: FigureState,
    warnings: Vec<SceneWarning>,
    export_requested: bool,
}

fn toolbar_harness(
    figure: FigureState,
    warnings: Vec<SceneWarning>,
) -> Harness<'static, ToolbarHarnessState> {
    Harness::new_ui_state(
        |ui, state: &mut ToolbarHarnessState| {
            let response = toolbar(ui, &mut state.figure, &state.warnings);
            state.export_requested |= response.export_requested;
        },
        ToolbarHarnessState {
            figure,
            warnings,
            export_requested: false,
        },
    )
}

fn panned_2d_state() -> FigureState {
    let mut state = FigureState::new(figure_with(vec![axes_2d(2)], vec![]));
    let hit = HitMap {
        axes: vec![hit_2d(2, PLOT)],
        legend_entries: vec![],
    };
    state.drag_start(&hit, Point::new(100.0, 50.0));
    state.drag_update(Point::new(160.0, 90.0));
    state.drag_end(Point::new(160.0, 90.0));
    state
}

// Why: "Reset view" is the user's way back after getting lost; the button must be wired to the reset, not merely
// drawn.
#[test]
fn clicking_reset_view_after_a_pan_restores_the_limits() {
    let state = panned_2d_state();
    assert_ne!(
        manual_of(&state.current, 2, Dimension::X),
        (0.0, 10.0),
        "precondition: the pan moved the view"
    );
    let mut harness = toolbar_harness(state, vec![]);

    harness.get_by_label("Reset view").click();
    harness.run();

    let figure = &harness.state().figure;
    assert_eq!(manual_of(&figure.current, 2, Dimension::X), (0.0, 10.0));
    assert_eq!(manual_of(&figure.current, 2, Dimension::Y), (0.0, 5.0));
}

// Why: rotating a 2D axes is meaningless; the Rotate tool must be unavailable rather than silently doing nothing, and
// available as soon as the figure has a 3D axes.
#[test]
fn the_rotate_button_is_disabled_only_for_figures_without_3d_axes() {
    let harness = toolbar_harness(
        FigureState::new(figure_with(vec![axes_2d(2)], vec![])),
        vec![],
    );
    assert!(
        harness
            .get_by_label("Rotate")
            .accesskit_node()
            .is_disabled()
    );
    assert!(!harness.get_by_label("Pan").accesskit_node().is_disabled());

    let harness = toolbar_harness(
        FigureState::new(figure_with(vec![axes_2d(2), axes_3d(3)], vec![])),
        vec![],
    );
    assert!(
        !harness
            .get_by_label("Rotate")
            .accesskit_node()
            .is_disabled()
    );
}

// Why: the tool buttons are the only way to switch what a drag does; each must select its tool.
#[test]
fn tool_buttons_select_the_drag_tool() {
    let mut harness = toolbar_harness(
        FigureState::new(figure_with(vec![axes_3d(2)], vec![])),
        vec![],
    );

    harness.get_by_label("Zoom").click();
    harness.run();
    assert_eq!(harness.state().figure.tool, Tool::Zoom);

    harness.get_by_label("Rotate").click();
    harness.run();
    assert_eq!(harness.state().figure.tool, Tool::Rotate);

    harness.get_by_label("Pan").click();
    harness.run();
    assert_eq!(harness.state().figure.tool, Tool::Pan);
}

// Why: export needs a native save dialog, which the toolbar cannot own; it must report the request to the application
// and leave the figure untouched.
#[test]
fn clicking_export_pdf_reports_an_export_request() {
    let state = FigureState::new(figure_with(vec![axes_2d(2)], vec![]));
    let before = state.current.clone();
    let mut harness = toolbar_harness(state, vec![]);
    assert!(!harness.state().export_requested);

    harness.get_by_label("Export PDF…").click();
    harness.run();

    assert!(harness.state().export_requested);
    assert_eq!(harness.state().figure.current, before);
}

// Why: unsupported LaTeX and invalid data never stop a figure from drawing, so the problems indicator is the only
// signal that something was dropped; it must appear exactly when there are warnings.
#[test]
fn the_problems_indicator_appears_only_when_there_are_warnings() {
    let figure = || FigureState::new(figure_with(vec![axes_2d(2)], vec![]));
    let harness = toolbar_harness(figure(), vec![]);
    assert!(harness.query_by_label_contains("problem").is_none());

    let warnings = vec![
        SceneWarning {
            node: Some(NodeId(2)),
            message: "unsupported command \\foo".to_owned(),
        },
        SceneWarning {
            node: None,
            message: "non-positive data on a log axis was dropped".to_owned(),
        },
    ];
    let harness = toolbar_harness(figure(), warnings);
    assert!(harness.query_by_label_contains("2 problems").is_some());
}

fn app_harness(figures: Vec<(String, ironlab_ir::Figure)>) -> Harness<'static, ViewerApp> {
    Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_eframe(|_cc| ViewerApp::new(figures, TEXT.clone()))
}

// Why: the application must show one tab per figure, titled as given, with each tab's toolbar reflecting its own
// figure.
#[test]
fn the_app_shows_one_tab_per_figure_with_its_own_toolbar() {
    let mut harness = app_harness(vec![
        (
            "flat.fig.json".to_owned(),
            figure_with(vec![axes_2d(2)], vec![]),
        ),
        (
            "solid.fig.json".to_owned(),
            figure_with(vec![axes_3d(2)], vec![]),
        ),
    ]);
    harness.run();

    assert!(harness.query_by_label("flat.fig.json").is_some());
    assert!(harness.query_by_label("solid.fig.json").is_some());
    assert!(
        harness
            .get_by_label("Rotate")
            .accesskit_node()
            .is_disabled(),
        "the first (2D) tab is active"
    );

    harness.get_by_label("solid.fig.json").click();
    harness.run();
    assert!(
        !harness
            .get_by_label("Rotate")
            .accesskit_node()
            .is_disabled(),
        "the 3D tab enables Rotate"
    );
}

// Why: R is the documented keyboard shortcut for Reset view; it must act on the active figure.
#[test]
fn pressing_r_resets_the_view_of_the_active_figure() {
    let mut harness = app_harness(vec![(
        "flat.fig.json".to_owned(),
        figure_with(vec![axes_2d(2)], vec![]),
    )]);
    harness.run();
    harness
        .state_mut()
        .figure_state_mut(0)
        .expect("one figure")
        .current
        .axes_mut(NodeId(2))
        .unwrap()
        .x
        .limits = manual(3.0, 4.0);

    harness.key_press(egui::Key::R);
    harness.run();

    let state = harness.state().figure_state(0).expect("one figure");
    assert_eq!(manual_of(&state.current, 2, Dimension::X), (0.0, 10.0));
}

/// A position well inside the canvas of a 900 × 600 harness: the figure is scaled to fit the area below the tab bar
/// and toolbar, so this point lies near the figure centre, inside the plot rectangle of a single axes.
const CANVAS_CENTRE: egui::Pos2 = egui::pos2(450.0, 340.0);

// Why: the interaction logic is tested in figure space, so the canvas glue (screen-to-figure conversion, the hit map
// of the current compilation, forwarding the wheel) is otherwise untested; a wheel notch over the plot must zoom the
// active figure in about the pointer, as in MATLAB, where rolling the wheel away from the user zooms in.
#[test]
fn scrolling_up_over_the_canvas_zooms_the_figure_in() {
    let mut harness = app_harness(vec![(
        "flat.fig.json".to_owned(),
        figure_with(vec![axes_2d(2)], vec![]),
    )]);
    harness.run();

    harness.hover_at(CANVAS_CENTRE);
    harness.run();
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();

    let state = harness.state().figure_state(0).expect("one figure");
    let x = manual_of(&state.current, 2, Dimension::X);
    let y = manual_of(&state.current, 2, Dimension::Y);
    assert!(
        x.1 - x.0 < 10.0 && y.1 - y.0 < 5.0,
        "the view narrowed: x {x:?}, y {y:?}"
    );
    assert!(
        x.0 > 0.0 && x.1 < 10.0 && y.0 > 0.0 && y.1 < 5.0,
        "the zoom is about a point inside the plot, so both limits move inwards: x {x:?}, y {y:?}"
    );
}

// Why: a drag with the Pan tool must move the data with the pointer; a sign error or a missing screen-to-figure
// conversion in the canvas would move the view the wrong way or by the wrong amount.
#[test]
fn dragging_right_over_the_canvas_pans_the_data_to_the_right() {
    let mut harness = app_harness(vec![(
        "flat.fig.json".to_owned(),
        figure_with(vec![axes_2d(2)], vec![]),
    )]);
    harness.run();

    harness.drag_at(CANVAS_CENTRE);
    harness.run();
    harness.hover_at(CANVAS_CENTRE + egui::vec2(60.0, 0.0));
    harness.run();
    harness.drop_at(CANVAS_CENTRE + egui::vec2(60.0, 0.0));
    harness.run();

    let state = harness.state().figure_state(0).expect("one figure");
    let x = manual_of(&state.current, 2, Dimension::X);
    let y = manual_of(&state.current, 2, Dimension::Y);
    assert!(
        x.0 < 0.0 && x.1 < 10.0,
        "the data moved right, so the x limits decreased: {x:?}"
    );
    assert_close(x.1 - x.0, 10.0, 1e-9, "panning keeps the x range");
    assert_close(y.0, 0.0, 1e-9, "a horizontal drag leaves y alone");
    assert_close(y.1, 5.0, 1e-9, "a horizontal drag leaves y alone");
}
