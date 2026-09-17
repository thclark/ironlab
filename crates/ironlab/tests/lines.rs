//! Line plots: plot, plot3, loglog, semilogx and semilogy.

mod common;

use common::{axes, data, has_error_at, is_3d, line, parent};
use ironlab::ir::{
    ColorSpec, DashStyle, Interpreter, IssueKind, LineStyle, MarkerShape, MarkerStyle, NdArray,
    Scale,
};
use ironlab::prelude::*;

// WHY: plot must copy the caller's arrays into the figure's data table unchanged and
// reference them from the new line, because the IR (not the caller's vectors) is what
// is saved, drawn and exported.
#[test]
fn plot_stores_copies_of_the_input_arrays() {
    let x = vec![0.0, 0.5, 1.0, f64::NAN];
    let y = vec![3.0, -1.0, 2.5, 4.0];
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).plot(&x, &y).id();

    let l = line(&fig, id);
    assert_eq!(data(&fig, l.x), &NdArray::vector(x));
    assert_eq!(data(&fig, l.y), &NdArray::vector(y));
    assert_eq!(l.z, None);
}

// WHY: automatic colours must stay symbolic (`Auto`) in the IR so that the scene
// compiler assigns the colour order per axes; baking a colour in at build time would
// break the order when series are hidden, reordered or loaded from a file.
#[test]
fn plot_uses_ir_default_styles_with_automatic_colour() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).plot([1.0, 2.0], [3.0, 4.0]).id();
    let l = line(&fig, id);
    assert_eq!(l.line, LineStyle::default());
    assert_eq!(l.line.color, ColorSpec::Auto);
    assert_eq!(l.marker, MarkerStyle::default());
    assert!(l.visible);
    assert_eq!(l.display_name, None);
}

// WHY: the plot must land in the axes it was called on, in call order, because
// drawing order and the automatic colour order follow the artist order.
#[test]
fn plots_are_appended_to_their_axes_in_order() {
    let mut fig = Figure::new().tiles(1, 2);
    let mut left = fig.axes(0, 0);
    let first = left.plot([0.0], [0.0]).id();
    let second = left.plot([1.0], [1.0]).id();
    let left_id = left.id();
    let right = fig.axes(0, 1).plot([2.0], [2.0]).id();

    let ids: Vec<NodeId> = axes(&fig, left_id).artists.iter().map(|a| a.id()).collect();
    assert_eq!(ids, vec![first, second]);
    assert_ne!(parent(&fig, right).id, left_id);
}

// WHY: every chained setter is a user-visible promise that a MATLAB property maps to a
// specific IR field; this pins each mapping so a setter cannot silently write the
// wrong field (for example marker_face writing the edge).
#[test]
fn line_setters_map_to_ir_fields() {
    let red = Color::rgb(0.8, 0.1, 0.1);
    let blue = Color::rgb(0.0, 0.2, 0.9);
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .plot([0.0, 1.0], [0.0, 1.0])
        .display_name("measured")
        .color(red)
        .line_width(1.75)
        .dash(Dash::Dashed)
        .marker(Marker::TriangleUp)
        .marker_size(6.5)
        .marker_face(Color::WHITE)
        .marker_edge(blue)
        .id();

    let l = line(&fig, id);
    assert_eq!(l.display_name, Some(Text::new("measured")));
    assert_eq!(l.line.color, ColorSpec::Rgba { color: red });
    assert_eq!(l.line.width_pt, 1.75);
    assert_eq!(l.line.dash, DashStyle::Dashed);
    assert_eq!(l.marker.shape, MarkerShape::TriangleUp);
    assert_eq!(l.marker.size_pt, 6.5);
    assert_eq!(
        l.marker.face,
        ColorSpec::Rgba {
            color: Color::WHITE
        }
    );
    assert_eq!(l.marker.edge, ColorSpec::Rgba { color: blue });
}

// WHY: setting the line colour must not bake that colour into the marker edge; the
// edge stays automatic so that it keeps following the line colour if the line colour
// is later changed (MATLAB's 'auto' marker edge).
#[test]
fn line_color_leaves_marker_edge_automatic() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .plot([0.0, 1.0], [0.0, 1.0])
        .marker(Marker::Circle)
        .color(Color::BLACK)
        .id();
    assert_eq!(line(&fig, id).marker.edge, ColorSpec::Auto);
}

// WHY: legend entries like "$\sin x$" must be typeset as mathematics, so a plain string
// has to keep the LaTeX interpreter; conversely users need a way to show literal
// dollar signs.
#[test]
fn display_name_interpreter_follows_text_conversion() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let math = ax.plot([0.0], [0.0]).display_name("$\\sin x$").id();
    let owned = ax
        .plot([0.0], [0.0])
        .display_name(String::from("$x^2$"))
        .id();
    let literal = ax
        .plot([0.0], [0.0])
        .display_name(Text::plain("costs $5"))
        .id();

    let name = |id| line(&fig, id).display_name.clone().unwrap();
    assert_eq!(name(math).content, "$\\sin x$");
    assert_eq!(name(math).interpreter, Interpreter::Latex);
    assert_eq!(name(owned).interpreter, Interpreter::Latex);
    assert_eq!(name(literal).content, "costs $5");
    assert_eq!(name(literal).interpreter, Interpreter::None);
}

// WHY: color() accepts `None` and a ColorSpec as well as a Color, so users can hide the
// line (markers only) or restore the automatic colour without touching IR types.
#[test]
fn line_color_accepts_none_and_color_spec() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let hidden = ax.plot([0.0], [0.0]).color(None).id();
    let reset = ax
        .plot([0.0], [0.0])
        .color(Color::BLACK)
        .color(ColorSpec::Auto)
        .id();
    assert_eq!(line(&fig, hidden).line.color, ColorSpec::None);
    assert_eq!(line(&fig, reset).line.color, ColorSpec::Auto);
}

// WHY: each log-plot function is defined by which axis scales it sets; a mix-up (for
// example semilogx making y logarithmic) produces a plausible but wrong figure that
// no rendering test would catch.
#[test]
fn loglog_sets_x_and_y_log_and_leaves_z_linear() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).loglog([1.0, 10.0], [1.0, 100.0]).id();
    let a = parent(&fig, id);
    assert_eq!(
        (a.x.scale, a.y.scale, a.z.scale),
        (Scale::Log, Scale::Log, Scale::Linear)
    );
}

// WHY: see loglog_sets_x_and_y_log_and_leaves_z_linear.
#[test]
fn semilogx_sets_only_x_log() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).semilogx([1.0, 10.0], [0.0, 1.0]).id();
    let a = parent(&fig, id);
    assert_eq!(
        (a.x.scale, a.y.scale, a.z.scale),
        (Scale::Log, Scale::Linear, Scale::Linear)
    );
}

// WHY: see loglog_sets_x_and_y_log_and_leaves_z_linear.
#[test]
fn semilogy_sets_only_y_log() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).semilogy([0.0, 1.0], [1.0, 10.0]).id();
    let a = parent(&fig, id);
    assert_eq!(
        (a.x.scale, a.y.scale, a.z.scale),
        (Scale::Linear, Scale::Log, Scale::Linear)
    );
}

// WHY: as in MATLAB, semilogx on axes previously made log-log sets y back to linear;
// the function describes the resulting axes, not a single toggle.
#[test]
fn semilogx_after_loglog_makes_y_linear() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.loglog([1.0, 10.0], [1.0, 10.0]);
    let id = ax.semilogx([1.0, 10.0], [1.0, 10.0]).id();
    let a = parent(&fig, id);
    assert_eq!((a.x.scale, a.y.scale), (Scale::Log, Scale::Linear));
}

// WHY: log plots are ordinary lines; they must store the same data and styles as plot
// so that legends and colour order treat them identically.
#[test]
fn log_plots_create_ordinary_lines() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).loglog([1.0, 10.0], [2.0, 20.0]).id();
    let l = line(&fig, id);
    assert_eq!(data(&fig, l.x), &NdArray::vector(vec![1.0, 10.0]));
    assert_eq!(data(&fig, l.y), &NdArray::vector(vec![2.0, 20.0]));
    assert_eq!(l.line.color, ColorSpec::Auto);
}

// WHY: plot3 is the 3D form of plot; it must store z and, like MATLAB, make the axes 3D
// so that the user does not get a ThreeDArtistInTwoDAxes error for ordinary use.
#[test]
fn plot3_stores_z_and_promotes_the_axes_to_3d() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .plot3([0.0, 1.0], [0.0, 1.0], [5.0, 6.0])
        .id();
    let l = line(&fig, id);
    let z = l.z.expect("plot3 stores z");
    assert_eq!(data(&fig, z), &NdArray::vector(vec![5.0, 6.0]));
    assert!(is_3d(parent(&fig, id)));
    assert!(fig.validate().is_valid());
}

// WHY: arrays of different lengths are a user error that must not panic in the
// builder, but must be reported against the offending line.
#[test]
fn mismatched_line_lengths_are_reported_by_validate() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).plot([0.0, 1.0, 2.0], [0.0, 1.0]).id();
    assert!(has_error_at(&fig.validate(), IssueKind::ShapeMismatch, id));
}
