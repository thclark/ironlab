//! The visibility of each axis and the angle of its tick labels: the setters of each coordinate axis, and what
//! validation says of them.

mod common;

use common::{axes, has_error_at};
use ironlab::ir::IssueKind;
use ironlab::prelude::*;

// WHY: each visibility setter must reach its own axis; a mix-up hides the wrong axis without any visible error in
// code review. A hidden axis can be shown again, so that a figure built in steps can undo an earlier call.
#[test]
fn visibility_setters_hide_and_show_their_own_axis() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).id();
    let a = axes(&fig, id);
    assert!(
        a.x.visible && a.y.visible && a.z.visible,
        "axes are shown by default"
    );

    let id = fig.axes(0, 0).xaxis_visible(false).id();
    let a = axes(&fig, id);
    assert!(!a.x.visible && a.y.visible && a.z.visible);

    let id = fig
        .axes(0, 0)
        .yaxis_visible(false)
        .zaxis_visible(false)
        .xaxis_visible(true)
        .id();
    let a = axes(&fig, id);
    assert!(a.x.visible && !a.y.visible && !a.z.visible);
}

// WHY (#153): each angle setter must reach its own axis and store the angle as given, in degrees anticlockwise from
// horizontal, so that 45 gives labels rising to the right as in MATLAB's `xtickangle(45)`; a conversion to radians or
// a flipped sign here would turn every label the wrong way.
#[test]
fn angle_setters_set_the_angle_of_their_own_axis_in_degrees() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).id();
    let a = axes(&fig, id);
    assert_eq!(
        [
            a.x.tick_label_angle,
            a.y.tick_label_angle,
            a.z.tick_label_angle
        ],
        [0.0, 0.0, 0.0],
        "tick labels are level by default"
    );

    let id = fig
        .axes(0, 0)
        .xtick_label_angle(45.0)
        .ytick_label_angle(-30.0)
        .id();
    let a = axes(&fig, id);
    assert_eq!(
        [
            a.x.tick_label_angle,
            a.y.tick_label_angle,
            a.z.tick_label_angle
        ],
        [45.0, -30.0, 0.0]
    );

    let id = fig.axes(0, 0).ztick_label_angle(90.0).id();
    assert_eq!(axes(&fig, id).z.tick_label_angle, 90.0);
}

// WHY (#74, #153): the two properties are made to be used together on a 2D axes (a strip of category panels whose
// shared x axis is hidden on all but the bottom one, which names the categories at an angle); the composition must be
// a valid figure without warnings, so that the gallery and yFoil can rely on it.
#[test]
fn a_hidden_axis_beside_rotated_category_labels_is_a_valid_figure() {
    let names = ["first case", "second case", "third case"];
    let ticks = [0.0, 1.0, 2.0];
    let mut fig = Figure::new().tiles(2, 1);
    let mut top = fig.axes(0, 0);
    top.plot(ticks, [0.1, 0.4, 0.3]);
    top.xaxis_visible(false);
    let mut bottom = fig.axes(1, 0);
    bottom.plot(ticks, [0.2, 0.5, 0.1]);
    bottom
        .xticks(ticks)
        .xtick_labels(names)
        .xtick_label_angle(45.0);
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: the builder never panics on a value the IR rejects (the facade's contract); a non-finite angle must reach
// validation, which names the axes, rather than be clamped or silently dropped.
#[test]
fn a_non_finite_angle_does_not_panic_and_is_reported_by_validate() {
    for angle in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut fig = Figure::new();
        let id = fig.axes(0, 0).ytick_label_angle(angle).id();
        assert!(
            has_error_at(&fig.validate(), IssueKind::InvalidTickLabelAngle, id),
            "{angle}"
        );
    }
}

// WHY: a three-dimensional axes ignores the angle, and an author who sets one must be told so rather than wonder why
// the labels stay level; the setter stores the angle all the same, so that switching the axes to 2D honours it.
#[test]
fn an_angle_on_a_3d_axes_is_stored_and_warned_of() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .view(-37.5, 30.0)
        .ztick_label_angle(30.0)
        .id();
    assert_eq!(axes(&fig, id).z.tick_label_angle, 30.0);
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.kind == IssueKind::TickLabelAngleIgnored && w.node == Some(id)),
        "{report:?}"
    );
}
