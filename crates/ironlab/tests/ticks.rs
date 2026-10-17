//! Axis direction, ticks and tick labels: the setters of each coordinate axis, and what validation says of them.

mod common;

use common::{axes, has_error_at};
use ironlab::ir::{IssueKind, Limits};
use ironlab::prelude::*;
use ironlab::{TickLabels, Ticks};

// WHY: each setter must reach its own axis; a mix-up reverses or relabels the wrong axis without any visible error in
// code review.
#[test]
fn reverse_setters_reverse_their_own_axis() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).yreverse(true).id();
    let a = axes(&fig, id);
    assert!(!a.x.reverse && a.y.reverse && !a.z.reverse);

    let id = fig
        .axes(0, 0)
        .xreverse(true)
        .zreverse(true)
        .yreverse(false)
        .id();
    let a = axes(&fig, id);
    assert!(a.x.reverse && !a.y.reverse && a.z.reverse);
}

// WHY: links share limits only, so that a reversed axis can be linked to a normal one; setting the direction of one
// member of a link group must therefore leave the others as they are, while its limits still propagate.
#[test]
fn reversing_an_axis_does_not_reverse_the_axes_linked_with_it() {
    let mut fig = Figure::new().tiles(1, 2);
    let a = fig.axes(0, 0).id();
    let b = fig.axes(0, 1).id();
    fig.link(Dim::Y, &[a, b]).unwrap();

    fig.axes(0, 0).yreverse(true).ylim(-1.5, 1.0);

    assert!(axes(&fig, a).y.reverse);
    assert!(!axes(&fig, b).y.reverse);
    assert_eq!(
        axes(&fig, b).y.limits,
        Limits::Manual {
            min: -1.5,
            max: 1.0
        }
    );
    assert!(fig.validate().is_valid());
}

// WHY: explicit ticks, a regular step and explicit labels are the building blocks of a categorical axis and of a
// shared regular grid; each must be stored on its own axis exactly as given.
#[test]
fn tick_setters_set_their_own_axis() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .xtick_step(0.2)
        .yticks([0.0, 1.0, 2.0])
        .ytick_labels(["first", "second", "third"])
        .ztick_step_from(0.1, 0.05)
        .id();
    let a = axes(&fig, id);
    assert_eq!(
        a.x.ticks,
        Ticks::Step {
            step: 0.2,
            origin: 0.0
        }
    );
    assert_eq!(a.x.tick_labels, TickLabels::Auto);
    assert_eq!(a.y.ticks, Ticks::Values(vec![0.0, 1.0, 2.0]));
    assert_eq!(
        a.y.tick_labels,
        TickLabels::Text(vec![
            Text::new("first"),
            Text::new("second"),
            Text::new("third")
        ])
    );
    assert_eq!(
        a.z.ticks,
        Ticks::Step {
            step: 0.1,
            origin: 0.05
        }
    );
    assert!(fig.validate().is_valid());
}

// WHY: hidden labels are how the inner panels of a shared grid drop their labels, so the setter must hide the labels of
// its own axis only and keep the ticks the axis had.
#[test]
fn hiding_tick_labels_keeps_the_ticks() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .xticks([0.0, 0.5, 1.0])
        .xtick_labels_hidden()
        .ytick_labels_hidden()
        .id();
    let a = axes(&fig, id);
    assert_eq!(a.x.ticks, Ticks::Values(vec![0.0, 0.5, 1.0]));
    assert_eq!(a.x.tick_labels, TickLabels::Hidden);
    assert_eq!(a.y.ticks, Ticks::Auto);
    assert_eq!(a.y.tick_labels, TickLabels::Hidden);
    assert_eq!(a.z.tick_labels, TickLabels::Auto);
}

// WHY: the builder never panics on a value the IR rejects (the facade's contract); a label count that does not match
// the ticks, or labels without explicit ticks, must reach validation, which names the axes.
#[test]
fn mismatched_tick_labels_do_not_panic_and_are_reported_by_validate() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .xticks([0.0, 1.0])
        .xtick_labels(["only one"])
        .id();
    assert!(has_error_at(
        &fig.validate(),
        IssueKind::InvalidTickLabels,
        id
    ));

    let mut fig = Figure::new();
    let id = fig.axes(0, 0).ytick_labels(["a", "b"]).id();
    assert!(has_error_at(
        &fig.validate(),
        IssueKind::InvalidTickLabels,
        id
    ));

    let mut fig = Figure::new();
    let id = fig.axes(0, 0).xtick_step(0.0).id();
    assert!(has_error_at(&fig.validate(), IssueKind::InvalidTicks, id));
}

// WHY (#79): a categorical strip chart is built from explicit ticks, explicit labels and a reversed y axis, with no
// categorical axis type; the composition must be a valid figure, so that the gallery and yFoil can rely on it. (Its
// drawing is covered by the gallery's strip chart, which is exported in the gallery's tests.)
#[test]
fn a_strip_chart_is_built_from_ticks_labels_and_a_reversed_axis() {
    let names = ["case_a", "case_b", "case_c"];
    let rows: Vec<f64> = (0..names.len()).map(|k| k as f64).collect();
    let mut fig = Figure::new().tiles(1, 2);
    for col in 0..2 {
        let mut ax = fig.axes(0, col);
        ax.scatter([0.2, 0.4, 0.9], &rows);
        ax.xlim(0.0, 1.0)
            .ylim(-0.5, names.len() as f64 - 0.5)
            .yreverse(true)
            .yticks(&rows);
        if col == 0 {
            ax.ytick_labels(names);
        } else {
            ax.ytick_labels_hidden();
        }
    }
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}
