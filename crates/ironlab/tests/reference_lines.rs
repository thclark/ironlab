//! Reference lines (hline and vline), blended coordinates and clipping.

mod common;

use common::{data, has_error_at, series};
use ironlab::ir::{ColorSpec, IssueKind, NdArray};
use ironlab::prelude::*;

// WHY: a reference line is a line whose one dimension spans the plot area as the fractions 0 and 1 and whose other
// dimension holds the data value twice; that is what keeps it spanning the axes at any limits, and what keeps the
// fractions out of the automatic limits while the data value takes part in them, as Matplotlib's axhline does.
#[test]
fn hline_and_vline_span_the_plot_area_at_a_data_value() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let h = ax.hline(1.5).id();
    let v = ax.vline(-2.0).id();

    let h_line = series(&fig, h);
    assert_eq!(h_line.x_coords, Coords::AxesFraction);
    assert_eq!(h_line.y_coords, Coords::Data);
    assert_eq!(data(&fig, h_line.x), &NdArray::vector(vec![0.0, 1.0]));
    assert_eq!(data(&fig, h_line.y), &NdArray::vector(vec![1.5, 1.5]));
    assert!(h_line.clip, "a reference line is clipped like any line");

    let v_line = series(&fig, v);
    assert_eq!(v_line.x_coords, Coords::Data);
    assert_eq!(v_line.y_coords, Coords::AxesFraction);
    assert_eq!(data(&fig, v_line.x), &NdArray::vector(vec![-2.0, -2.0]));
    assert_eq!(data(&fig, v_line.y), &NdArray::vector(vec![0.0, 1.0]));
}

// WHY: a reference line is a judgement against the data, not a series of it, so it takes a fixed dark grey and leaves
// the colour order to the data: the first data series after it is still the first colour of the order.
#[test]
fn a_reference_line_is_dark_grey_and_takes_no_colour_from_the_colour_order() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let h = ax.hline(0.0).id();
    let plotted = ax.plot([0.0, 1.0], [0.0, 1.0]).id();
    let ColorSpec::Rgba { color } = series(&fig, h).line.color else {
        panic!("the reference line has a fixed colour");
    };
    assert_eq!(color, Color::rgb(0.15, 0.15, 0.15));
    assert_eq!(series(&fig, plotted).line.color, ColorSpec::Auto);
}

// WHY: the style setters of a line are how a reference line is made dotted, coloured or thickened, so they must apply
// to a reference line exactly as they do to any line.
#[test]
fn a_reference_line_takes_the_line_style_setters() {
    let mut fig = Figure::new();
    let orange = Color::rgb(0.9, 0.6, 0.0);
    let id = fig
        .axes(0, 0)
        .vline(0.5)
        .dash(Dash::Dotted)
        .color(orange)
        .line_width(0.5)
        .id();
    let style = series(&fig, id).line;
    assert_eq!(style.dash, Dash::Dotted);
    assert_eq!(style.color, ColorSpec::Rgba { color: orange });
    assert_eq!(style.width_pt, 0.5);
}

// WHY: the setters are the general mechanism behind the reference lines: markers along an edge at data positions are a
// line or scatter with one dimension in axes-fraction coordinates and clipping off, and each setter must write its
// own field of the IR.
#[test]
fn coordinate_and_clip_setters_write_their_fields_on_lines_and_scatters() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let l = ax
        .plot([1.0, 2.0], [0.985, 0.985])
        .y_coords(Coords::AxesFraction)
        .clip(false)
        .marker(Marker::VLine)
        .id();
    let s = ax
        .scatter([0.5], [3.0])
        .x_coords(Coords::AxesFraction)
        .clip(false)
        .id();

    let l = series(&fig, l);
    assert_eq!(
        (l.x_coords, l.y_coords, l.clip),
        (Coords::Data, Coords::AxesFraction, false)
    );
    assert_eq!(l.marker.shape, Marker::VLine);
    let s = series(&fig, s);
    assert_eq!(
        (s.x_coords, s.y_coords, s.clip),
        (Coords::AxesFraction, Coords::Data, false)
    );
}

// WHY: a reference line belongs to a two-dimensional axes, whose plot area has edges to span; one added to a
// three-dimensional axes cannot be drawn as described and must be reported, not drawn somewhere arbitrary.
#[test]
fn a_reference_line_in_a_three_dimensional_axes_is_reported_by_validate() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.plot3([0.0, 1.0], [0.0, 1.0], [0.0, 1.0]);
    let h = ax.hline(0.5).id();
    assert!(has_error_at(
        &fig.validate(),
        IssueKind::AxesFractionInThreeDAxes,
        h
    ));
}
