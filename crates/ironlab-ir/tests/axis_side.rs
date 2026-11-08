//! The side of the plot on which an axis is drawn: its default, its JSON and Protocol Buffers
//! forms, its path in the edit model and its choices, and the warning raised where a projection
//! ignores it.
//!
//! An axis draws its line, tick marks, tick labels and label on the low side of the plot (the
//! left for an axis that runs up the page, the bottom for one that runs across it), through its
//! middle, or on its high side. Two-dimensional and three-dimensional Cartesian axes honour the
//! side on every axis that they draw, and cylindrical axes honour it on z; polar axes, spherical
//! axes and the angle and radius of cylindrical axes ignore it.

mod common;

use common::edits::{applied, path, set, tx};
use common::{kitchen_sink_figure, single_line_figure};
use ironlab_ir::*;
use prost::Message;
use serde_json::{Value as JsonValue, json};

const SIDES: [AxisSide; 3] = [AxisSide::Low, AxisSide::Centre, AxisSide::High];
const DIMENSIONS: [Dimension; 3] = [Dimension::X, Dimension::Y, Dimension::Z];

fn has_warning(report: &ValidationReport, kind: IssueKind, node: NodeId) -> bool {
    report
        .warnings
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

fn reports(report: &ValidationReport, kind: IssueKind) -> bool {
    report
        .errors
        .iter()
        .chain(&report.warnings)
        .any(|issue| issue.kind == kind)
}

/// Returns the axis of an axes along a dimension for modification.
fn axis_mut(axes: &mut Axes, dimension: Dimension) -> &mut Axis {
    match dimension {
        Dimension::X => &mut axes.x,
        Dimension::Y => &mut axes.y,
        Dimension::Z => &mut axes.z,
    }
}

fn three_d() -> Projection {
    Projection::ThreeD {
        view3d: View3d::default(),
    }
}

fn polar() -> Projection {
    Projection::Polar {
        polar: PolarView::default(),
    }
}

fn cylindrical() -> Projection {
    Projection::Cylindrical {
        view3d: View3d::default(),
        polar: PolarView::default(),
    }
}

fn spherical() -> Projection {
    Projection::Spherical {
        view3d: View3d::default(),
        spherical: SphericalView::default(),
    }
}

/// The single-line figure (x of 1, 2, 3 and y of 1, 4, 9, valid in every projection) with its
/// axes given a projection.
fn line_in(projection: Projection) -> (Figure, NodeId) {
    let (mut fig, axes, _) = single_line_figure();
    fig.axes[0].projection = projection;
    (fig, axes)
}

// ---------------------------------------------------------------------------------
// Defaults and file formats
// ---------------------------------------------------------------------------------

// Why: every axis of every figure written so far is drawn on the low side (the left spine for
// y, the bottom spine for x, the leftmost edge for the z of a 3D box), so Low must be the
// default of a new axis; any other default would move the axes of every existing chart.
#[test]
fn an_axis_is_on_the_low_side_by_default() {
    assert_eq!(AxisSide::default(), AxisSide::Low);
    assert_eq!(Axis::default().side, AxisSide::Low);
    let axes = Axes::default();
    for axis in [&axes.x, &axes.y, &axes.z] {
        assert_eq!(axis.side, AxisSide::Low);
    }
}

// Why: files written before an axis could change side have no side, and most figures written
// after keep the default; Low must therefore be left out of the JSON, so that such files are
// unchanged by a load and a save, and a file without the property must load with every axis on
// the low side.
#[test]
fn a_low_axis_is_written_without_its_side_and_a_file_without_it_reads_as_low() {
    let (fig, _, _) = single_line_figure();
    let text = fig.to_json();
    let value: JsonValue = serde_json::from_str(&text).unwrap();
    for name in ["x", "y", "z"] {
        let axis = &value["axes"][0][name];
        assert!(axis.is_object(), "{name}: {axis}");
        assert!(axis.get("side").is_none(), "{name}.side is written: {axis}");
    }
    let restored = Figure::from_json(&text).expect("a file without a side loads");
    for axis in [
        &restored.axes[0].x,
        &restored.axes[0].y,
        &restored.axes[0].z,
    ] {
        assert_eq!(axis.side, AxisSide::Low);
    }
    assert_eq!(restored, fig);
}

// Why: a side is written by hand from the documented form, in the snake_case of every other
// plain enumeration of the axes, under the axis it belongs to; a reader that confused the axes
// would move the wrong one.
#[test]
fn each_side_is_written_in_snake_case_on_its_own_axis() {
    let (mut fig, _, _) = single_line_figure();
    fig.axes[0].x.side = AxisSide::High;
    fig.axes[0].y.side = AxisSide::Centre;
    let text = fig.to_json();
    let value: JsonValue = serde_json::from_str(&text).unwrap();
    let axes = &value["axes"][0];
    assert_eq!(axes["x"]["side"], json!("high"));
    assert_eq!(axes["y"]["side"], json!("centre"));
    assert!(axes["z"].get("side").is_none(), "{}", axes["z"]);
    assert_eq!(Figure::from_json(&text).expect("own output parses"), fig);

    // The title alignment is also written "centre", so the side is replaced where it stands.
    let mut explicit_low = value.clone();
    explicit_low["axes"][0]["y"]["side"] = json!("low");
    let reread = Figure::from_json(&explicit_low.to_string()).expect("an explicit low side parses");
    assert_eq!(reread.axes[0].y.side, AxisSide::Low);
}

// Why: a saved figure must reopen exactly as it was, with each axis on the side it was put on,
// whichever of the two file formats it was saved in, and saving it again must write the same
// bytes, so that a load and a save do not show as a change.
#[test]
fn every_side_of_every_axis_survives_json_and_protobuf() {
    for dimension in DIMENSIONS {
        for side in SIDES {
            let (mut fig, _) = line_in(three_d());
            axis_mut(&mut fig.axes[0], dimension).side = side;
            let at = format!("{dimension:?} {side:?}");
            let text = fig.to_json();
            let reopened = Figure::from_json(&text).expect("own output parses");
            assert_eq!(reopened, fig, "JSON of {at}");
            assert_eq!(reopened.to_json(), text, "JSON of {at} is stable");
            let bytes = fig.to_protobuf();
            let decoded = Figure::from_protobuf(&bytes).expect("own output decodes");
            assert_eq!(decoded, fig, "protobuf of {at}");
            assert_eq!(decoded.to_protobuf(), bytes, "protobuf of {at} is stable");
        }
    }
}

// Why: proto3 reads an absent enumeration as its zero value, which is "unspecified"; files
// written before the field existed, and writers in other languages that leave it out, must be
// read with every axis on the low side, as they were drawn.
#[test]
fn an_absent_side_decodes_as_low() {
    let (fig, _, _) = single_line_figure();
    let mut wire_figure = wire::Figure::from(&fig);
    let axes = &mut wire_figure.axes[0];
    for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
        axis.as_mut().expect("the encoder writes every axis").side = 0;
    }
    let decoded = Figure::from_protobuf(&wire_figure.encode_to_vec()).unwrap();
    for axis in [&decoded.axes[0].x, &decoded.axes[0].y, &decoded.axes[0].z] {
        assert_eq!(axis.side, AxisSide::Low);
    }
    assert_eq!(decoded, fig);
}

// Why: the encoder must write the side of every axis as a real value, never as "unspecified",
// so that a reader in another language need not know the default; and each axis must keep its
// own side, so that moving x to the top does not move y.
#[test]
fn the_encoder_writes_the_side_of_each_axis() {
    let (mut fig, _) = line_in(three_d());
    fig.axes[0].x.side = AxisSide::High;
    fig.axes[0].y.side = AxisSide::Centre;
    let wire_figure = wire::Figure::from(&fig);
    let axes = &wire_figure.axes[0];
    let written: Vec<i32> = [&axes.x, &axes.y, &axes.z]
        .into_iter()
        .map(|axis| axis.as_ref().expect("the encoder writes every axis").side)
        .collect();
    assert_eq!(
        written,
        [
            wire::AxisSide::High as i32,
            wire::AxisSide::Centre as i32,
            wire::AxisSide::Low as i32,
        ]
    );
}

// ---------------------------------------------------------------------------------
// Edits and choices
// ---------------------------------------------------------------------------------

// Why: the property editor and clients in other languages move an axis by path; the side of
// each axis must be readable and settable at `x.side`, `y.side` and `z.side`, change exactly
// that axis, and be undone by the inverse.
#[test]
fn the_side_of_each_axis_is_set_through_its_path() {
    let fig = kitchen_sink_figure();
    let axes = fig.axes[0].id; // 2D, every axis on the low side
    let (edited, inverse) = applied(
        &fig,
        &tx([
            set(axes, "x.side", Value::AxisSide(AxisSide::High)),
            set(axes, "y.side", Value::AxisSide(AxisSide::Centre)),
        ]),
    )
    .unwrap();
    assert_eq!(
        edited.get(axes, &path("x.side")).unwrap(),
        Value::AxisSide(AxisSide::High)
    );
    assert_eq!(
        edited.get(axes, &path("y.side")).unwrap(),
        Value::AxisSide(AxisSide::Centre)
    );
    assert_eq!(
        edited.get(axes, &path("z.side")).unwrap(),
        Value::AxisSide(AxisSide::Low)
    );
    let mut expected = fig.clone();
    expected.axes[0].x.side = AxisSide::High;
    expected.axes[0].y.side = AxisSide::Centre;
    assert_eq!(edited, expected);
    let (restored, _) = applied(&edited, &inverse).unwrap();
    assert_eq!(restored, fig);

    let (fig, axes) = line_in(three_d());
    let (edited, _) = applied(
        &fig,
        &tx([set(axes, "z.side", Value::AxisSide(AxisSide::High))]),
    )
    .unwrap();
    assert_eq!(edited.axes[0].z.side, AxisSide::High);
}

// Why: the property editor offers the side in a combo box, which must hold every side, in the
// order in which they lie on the page (low, middle, high), so that the entries read as the
// positions they name.
#[test]
fn the_side_choices_are_low_centre_and_high_in_page_order() {
    let offered: Vec<Value> = choices(ValueType::AxisSide, None)
        .into_iter()
        .map(|choice| choice.value)
        .collect();
    assert_eq!(offered, SIDES.map(Value::AxisSide).to_vec());
}

// ---------------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------------

// Why: polar axes draw the angle round the rim and the radius along a spoke, spherical axes draw
// every axis on the sphere, and the angle and radius of cylindrical axes lie on the floor, so
// none of them has a side to move to. A user who set one must be told rather than left
// wondering, but the figure still draws, so it is a warning against the axes.
#[test]
fn a_side_where_it_is_ignored_is_warned_of_against_the_axes() {
    let cases = [
        ("polar", polar(), Dimension::X),
        ("polar", polar(), Dimension::Y),
        ("spherical", spherical(), Dimension::X),
        ("spherical", spherical(), Dimension::Y),
        ("spherical", spherical(), Dimension::Z),
        ("cylindrical", cylindrical(), Dimension::X),
        ("cylindrical", cylindrical(), Dimension::Y),
    ];
    for (name, projection, dimension) in cases {
        for side in [AxisSide::Centre, AxisSide::High] {
            let (mut fig, axes) = line_in(projection);
            axis_mut(&mut fig.axes[0], dimension).side = side;
            let report = fig.validate();
            let at = format!("{name} {dimension:?} {side:?}");
            assert_eq!(report.errors, vec![], "{at}");
            assert!(
                has_warning(&report, IssueKind::AxisSideIgnored, axes),
                "{at}: {report:?}"
            );
        }
    }
}

// Why: Cartesian axes draw every axis on the side asked for, and the z axis of cylindrical axes
// can stand on either wall or up the axis of the cylinder; a side there is a display choice like
// any other and must raise nothing, or every figure with an axis on the right would carry a
// spurious warning.
#[test]
fn a_side_where_it_is_honoured_raises_nothing() {
    let cases = [
        ("2D", Projection::TwoD, Dimension::X),
        ("2D", Projection::TwoD, Dimension::Y),
        ("3D", three_d(), Dimension::X),
        ("3D", three_d(), Dimension::Y),
        ("3D", three_d(), Dimension::Z),
        ("cylindrical", cylindrical(), Dimension::Z),
    ];
    for (name, projection, dimension) in cases {
        for side in [AxisSide::Centre, AxisSide::High] {
            let (mut fig, _) = line_in(projection);
            let baseline = fig.validate();
            axis_mut(&mut fig.axes[0], dimension).side = side;
            let report = fig.validate();
            let at = format!("{name} {dimension:?} {side:?}");
            assert!(
                !reports(&report, IssueKind::AxisSideIgnored),
                "{at}: {report:?}"
            );
            assert_eq!(report, baseline, "{at}");
        }
    }
}

// Why: Low is the default, which every projection accepts; a figure that never mentions the side
// must never be warned about it.
#[test]
fn a_default_side_is_never_warned_of() {
    for (name, projection) in [
        ("2D", Projection::TwoD),
        ("3D", three_d()),
        ("polar", polar()),
        ("cylindrical", cylindrical()),
        ("spherical", spherical()),
    ] {
        let (fig, _) = line_in(projection);
        assert!(
            !reports(&fig.validate(), IssueKind::AxisSideIgnored),
            "{name}"
        );
    }
}

// Why: a two-dimensional axes, Cartesian or polar, has no z axis and ignores all of its
// properties without comment, as it does a z tick label angle; a side left on that unused axis
// must not be reported, or a figure switched from 3D to 2D would be warned about a property it
// cannot show.
#[test]
fn the_z_side_of_a_two_dimensional_axes_is_ignored_silently() {
    for (name, projection) in [("2D", Projection::TwoD), ("polar", polar())] {
        let (mut fig, _) = line_in(projection);
        let baseline = fig.validate();
        fig.axes[0].z.side = AxisSide::High;
        assert_eq!(fig.validate(), baseline, "{name}");
    }
}
