//! The visibility of an axis and the angle of its tick labels: their defaults, their JSON
//! and Protocol Buffers forms, and their validation.
//!
//! A hidden axis draws no spine, ticks, tick labels or label and reserves no space for
//! them, while its grid lines still follow `grid`; the tick label angle rotates every tick
//! label of the axis, in degrees anticlockwise from horizontal, about its end nearest its
//! tick.

mod common;

use common::single_line_figure;
use ironlab_ir::*;
use prost::Message;
use serde_json::{Value as JsonValue, json};

fn has_error(report: &ValidationReport, kind: IssueKind, node: NodeId) -> bool {
    report
        .errors
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

fn has_warning(report: &ValidationReport, kind: IssueKind, node: NodeId) -> bool {
    report
        .warnings
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

/// Returns the axis of an axes along a dimension for modification.
fn axis_mut(axes: &mut Axes, dimension: Dimension) -> &mut Axis {
    match dimension {
        Dimension::X => &mut axes.x,
        Dimension::Y => &mut axes.y,
        Dimension::Z => &mut axes.z,
    }
}

// Why: every axis of every figure written so far is shown with level tick labels, so
// those must be the defaults of a new axis; an axis hidden or rotated by default would
// change every existing chart.
#[test]
fn an_axis_is_visible_with_level_tick_labels_by_default() {
    let axis = Axis::default();
    assert!(axis.visible);
    assert_eq!(axis.tick_label_angle, 0.0);
    let axes = Axes::default();
    for axis in [&axes.x, &axes.y, &axes.z] {
        assert!(axis.visible);
        assert_eq!(axis.tick_label_angle, 0.0);
    }
}

// Why: files written before an axis could be hidden or rotated have neither property, and
// most figures written after keep both at their defaults; the defaults must therefore be
// left out of the JSON, so that such files are unchanged by a load and a save, and a file
// without them must load with every axis visible and level.
#[test]
fn a_visible_level_axis_is_written_without_the_properties_and_reads_back_without_them() {
    let (fig, _, _) = single_line_figure();
    let text = fig.to_json();
    let value: JsonValue = serde_json::from_str(&text).unwrap();
    for name in ["x", "y", "z"] {
        let axis = &value["axes"][0][name];
        assert!(axis.is_object(), "{name}: {axis}");
        for key in ["visible", "tick_label_angle"] {
            assert!(axis.get(key).is_none(), "{name}.{key} is written: {axis}");
        }
    }
    let restored = Figure::from_json(&text).expect("a file without the properties loads");
    for axis in [
        &restored.axes[0].x,
        &restored.axes[0].y,
        &restored.axes[0].z,
    ] {
        assert!(axis.visible);
        assert_eq!(axis.tick_label_angle, 0.0);
    }
    assert_eq!(restored, fig);
}

// Why: a hidden axis and a rotated one are written by hand from the documented form, so
// each property is written under its own name on its own axis when it differs from the
// default, and must read back on that axis; a reader that confused the axes would hide
// the wrong one.
#[test]
fn a_hidden_or_rotated_axis_is_written_in_the_documented_json_form() {
    let (mut fig, _, _) = single_line_figure();
    fig.axes[0].x.visible = false;
    fig.axes[0].y.tick_label_angle = 45.0;
    let text = fig.to_json();
    let value: JsonValue = serde_json::from_str(&text).unwrap();
    let axes = &value["axes"][0];
    assert_eq!(axes["x"]["visible"], json!(false));
    assert!(axes["x"].get("tick_label_angle").is_none(), "{}", axes["x"]);
    assert_eq!(axes["y"]["tick_label_angle"], json!(45.0));
    assert!(axes["y"].get("visible").is_none(), "{}", axes["y"]);
    assert_eq!(Figure::from_json(&text).expect("own output parses"), fig);
}

// Why: proto3 reads an absent boolean as false, so a reader that took the wire value at
// face value would hide every axis of every file written before the field existed, and
// of every writer in another language that leaves it out. An absent visibility must
// decode as visible and an absent angle as 0, on every axis.
#[test]
fn an_absent_visibility_decodes_as_visible_and_an_absent_angle_as_level() {
    let (mut fig, _, _) = single_line_figure();
    for dimension in [Dimension::X, Dimension::Y, Dimension::Z] {
        let axis = axis_mut(&mut fig.axes[0], dimension);
        axis.visible = true;
        axis.tick_label_angle = 0.0;
    }
    let mut wire_figure = wire::Figure::from(&fig);
    let axes = &mut wire_figure.axes[0];
    for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
        let axis = axis.as_mut().expect("the encoder writes every axis");
        axis.visible = None;
        axis.tick_label_angle = None;
    }
    let decoded = Figure::from_protobuf(&wire_figure.encode_to_vec()).unwrap();
    assert_eq!(decoded, fig);
}

// Why: a hidden axis and an angle of 0 written explicitly must be kept as written, and a
// visible axis written explicitly as visible; the encoder must write both properties on
// every axis, so that a reader in another language need not know their defaults; and
// each axis must keep its own values, so that hiding x does not hide y.
#[test]
fn the_visibility_and_angle_of_each_axis_survive_the_binary_format() {
    let (mut fig, _, _) = single_line_figure();
    fig.axes[0].x.visible = false;
    fig.axes[0].x.tick_label_angle = -0.0;
    fig.axes[0].y.tick_label_angle = 90.0;
    fig.axes[0].z.tick_label_angle = 12.5;
    let wire_figure = wire::Figure::from(&fig);
    let axes = &wire_figure.axes[0];
    let written: Vec<(Option<bool>, Option<f64>)> = [&axes.x, &axes.y, &axes.z]
        .into_iter()
        .map(|axis| {
            let axis = axis.as_ref().expect("the encoder writes every axis");
            (axis.visible, axis.tick_label_angle)
        })
        .collect();
    assert_eq!(
        written,
        [
            (Some(false), Some(-0.0)),
            (Some(true), Some(90.0)),
            (Some(true), Some(12.5)),
        ]
    );
    let decoded = Figure::from_protobuf(&fig.to_protobuf()).unwrap();
    assert_eq!(decoded, fig);
    assert!(
        decoded.axes[0].x.tick_label_angle.is_sign_negative(),
        "negative zero must reload as itself"
    );
}

// Why: hiding an axis is a display choice, not a mistake, so a hidden axis raises nothing,
// even when the box that it would otherwise close is drawn;
// and any finite angle is a rotation the renderer can draw, so a vertical label, a label
// rotated beyond a full turn and a negative angle are all valid in a two-dimensional axes.
#[test]
fn hidden_axes_and_finite_angles_in_two_dimensions_raise_no_issue() {
    type Change = fn(&mut Axes);
    let cases: [(&str, Change); 6] = [
        ("hidden x", |axes| axes.x.visible = false),
        ("hidden x and y", |axes| {
            axes.x.visible = false;
            axes.y.visible = false;
        }),
        // A hidden axis removes the box edges along it, so a drawn box with a hidden axis
        // is an open box by design, not a conflict between the two settings.
        ("hidden y with the box drawn", |axes| {
            axes.box_ = true;
            axes.y.visible = false;
        }),
        ("vertical x labels", |axes| axes.x.tick_label_angle = 90.0),
        ("negative y labels", |axes| axes.y.tick_label_angle = -45.0),
        ("beyond a full turn", |axes| axes.x.tick_label_angle = 400.0),
    ];
    for (name, change) in cases {
        let (mut fig, _, _) = single_line_figure();
        change(&mut fig.axes[0]);
        let report = fig.validate();
        assert_eq!(report.errors, vec![], "{name}: {report:?}");
        assert_eq!(report.warnings, vec![], "{name}: {report:?}");
    }
}

// Why: a three-dimensional axes can hide an axis as a two-dimensional one can (its ticks,
// tick labels and label are not drawn), while the edges of its box, which belong to the
// box rather than to one axis, stay as they are; hiding an axis there is a display choice
// like any other and raises nothing, on each of the three axes.
#[test]
fn a_hidden_axis_of_a_three_dimensional_axes_raises_no_issue() {
    for dimension in [Dimension::X, Dimension::Y, Dimension::Z] {
        let (mut fig, _, _) = single_line_figure();
        fig.axes[0].projection = Projection::ThreeD {
            view3d: View3d::default(),
        };
        let baseline = fig.validate();
        axis_mut(&mut fig.axes[0], dimension).visible = false;
        assert_eq!(fig.validate(), baseline, "hidden {dimension:?}");
    }
}

// Why: an angle that is not finite has no direction, and a renderer given one would draw
// the labels nowhere or propagate NaN into the layout; it is refused against the axes, on
// whichever axis it occurs.
#[test]
fn a_tick_label_angle_that_is_not_finite_is_an_error_on_the_axes() {
    for dimension in [Dimension::X, Dimension::Y] {
        for angle in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let (mut fig, axes, _) = single_line_figure();
            axis_mut(&mut fig.axes[0], dimension).tick_label_angle = angle;
            let report = fig.validate();
            assert!(
                has_error(&report, IssueKind::InvalidTickLabelAngle, axes),
                "{dimension:?} at {angle}: {report:?}"
            );
            assert_eq!(
                report.errors.len(),
                1,
                "{dimension:?} at {angle}: {report:?}"
            );
        }
    }
}

// Why: a three-dimensional axes draws its tick labels along projected edges, where a
// rotation in the page has no agreed meaning, so the angle is ignored there; the user who
// set it must be told rather than left wondering, but the figure still draws, so it is a
// warning against the axes, on every one of its three axes. An angle of 0 is the default
// and raises nothing.
#[test]
fn a_tick_label_angle_in_a_three_dimensional_axes_is_ignored_with_a_warning() {
    for dimension in [Dimension::X, Dimension::Y, Dimension::Z] {
        let (mut fig, axes, _) = single_line_figure();
        fig.axes[0].projection = Projection::ThreeD {
            view3d: View3d::default(),
        };
        axis_mut(&mut fig.axes[0], dimension).tick_label_angle = 30.0;
        let report = fig.validate();
        assert_eq!(report.errors, vec![], "{dimension:?}: {report:?}");
        assert!(
            has_warning(&report, IssueKind::TickLabelAngleIgnored, axes),
            "{dimension:?}: {report:?}"
        );
    }
    let (mut fig, _, _) = single_line_figure();
    fig.axes[0].projection = Projection::ThreeD {
        view3d: View3d::default(),
    };
    let report = fig.validate();
    assert!(
        report
            .warnings
            .iter()
            .all(|w| w.kind != IssueKind::TickLabelAngleIgnored),
        "{report:?}"
    );
}

// Why: a two-dimensional axes has no z axis and ignores its z limits, scale and ticks, as
// it does when drawing; an angle left on that unused axis, or one that is not finite
// there, must not be reported, or a figure switched from 3D to 2D would be refused for a
// property it cannot show.
#[test]
fn the_z_tick_label_angle_of_a_two_dimensional_axes_is_ignored() {
    for angle in [30.0, f64::NAN] {
        let (mut fig, _, _) = single_line_figure();
        fig.axes[0].z.tick_label_angle = angle;
        let report = fig.validate();
        assert_eq!(report.errors, vec![], "z at {angle}: {report:?}");
        assert_eq!(report.warnings, vec![], "z at {angle}: {report:?}");
    }
}
