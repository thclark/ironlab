//! Stairs in the facade (#99): `stairs` and `stairs_at`, and the `StairsMut` handle that configures the `Stairs`
//! artist.
//!
//! These tests assert the figure description the facade stores. They assume that `AxesMut::stairs(edges, values)`
//! and `AxesMut::stairs_at(x, values, StepAt)` return a `StairsMut`, exported in the prelude, whose setters are
//! `label`, `base`, `open`, the line setters of `SeriesMut` (`color`, `line_width`, `dash`), `fill`,
//! `line_opacity`, `fill_opacity` and `orientation`.

mod common;

use common::{data, has_error_at, stairs};
use ironlab::IssueKind;
use ironlab::ir::{ColorSpec, DashStyle, NdArray, Orientation, StairsData, StepAt};
use ironlab::prelude::*;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

// WHY: `stairs(edges, values)` is Matplotlib's `stairs`, the outline of a histogram: it must store the edges and the
// values as given, one more edge than values, as an open, unfilled, upright outline in the next colour of the colour
// order, which is what `stairs` draws without further calls.
#[test]
fn stairs_stores_edges_and_values_as_an_open_unfilled_outline() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .stairs([0.0, 1.0, 3.0, 4.0], [2.0, 5.0, 1.0])
        .id();
    let s = stairs(&fig, id);
    let StairsData::Edges { edges, values } = s.data else {
        panic!("stairs between edges, not {:?}", s.data);
    };
    assert_eq!(
        data(&fig, edges),
        &NdArray::vector(vec![0.0, 1.0, 3.0, 4.0])
    );
    assert_eq!(data(&fig, values), &NdArray::vector(vec![2.0, 5.0, 1.0]));
    assert_eq!(s.base, None);
    assert_eq!(s.fill, ColorSpec::None);
    assert_eq!(s.line.color, ColorSpec::Auto);
    assert_eq!(s.orientation, Orientation::Vertical);
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: `stairs_at(x, values, step)` is MATLAB's `stairs` and Matplotlib's `step`: values held at samples, which step
// before, after or midway between them as the author asks; the step must be stored as given, since each choice moves
// every change of level.
#[test]
fn stairs_at_stores_samples_values_and_where_the_steps_occur() {
    for step in [StepAt::Pre, StepAt::Post, StepAt::Mid] {
        let mut fig = Figure::new();
        let id = fig
            .axes(0, 0)
            .stairs_at([0.0, 1.0, 2.0], [1.0, 3.0, 2.0], step)
            .id();
        let s = stairs(&fig, id);
        let StairsData::Samples {
            x,
            values,
            step: stored,
        } = s.data
        else {
            panic!("stairs through samples, not {:?}", s.data);
        };
        assert_eq!(stored, step);
        assert_eq!(data(&fig, x), &NdArray::vector(vec![0.0, 1.0, 2.0]));
        assert_eq!(data(&fig, values), &NdArray::vector(vec![1.0, 3.0, 2.0]));
        assert!(fig.validate().is_valid(), "{:?}", fig.validate());
    }
}

// WHY: each setter is the facade's name for one property of the stairs artist: a base closes the outline and `open`
// removes it again, the line setters are those of a series so that an outline is styled as a line is, and the fill
// and the two opacities style the area under a based outline.
#[test]
fn stairs_setters_write_the_properties_they_name() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .stairs([0.0, 1.0, 2.0], [1.0, 2.0])
        .label("Counts")
        .base(0.0)
        .color(RED)
        .line_width(2.0)
        .dash(DashStyle::Dashed)
        .fill(BLUE)
        .line_opacity(0.75)
        .fill_opacity(0.5)
        .orientation(Orientation::Horizontal)
        .id();
    let s = stairs(&fig, id);
    assert_eq!(s.label, Some(Text::new("Counts")));
    assert_eq!(s.base, Some(0.0));
    assert_eq!(s.line.color, ColorSpec::Rgba { color: RED });
    assert_eq!(s.line.width_pt, 2.0);
    assert_eq!(s.line.dash, DashStyle::Dashed);
    assert_eq!(s.fill, ColorSpec::Rgba { color: BLUE });
    assert_eq!((s.line_opacity, s.fill_opacity), (0.75, 0.5));
    assert_eq!(s.orientation, Orientation::Horizontal);
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());

    let id = fig
        .axes(0, 0)
        .stairs([0.0, 1.0, 2.0], [1.0, 2.0])
        .base(0.0)
        .open()
        .id();
    assert_eq!(stairs(&fig, id).base, None, "`open` removes the base");
}

// WHY: a fill needs a base to fill down to; the facade must not invent one, because a base of 0 is wrong for data
// on a logarithmic axis or about a reference level, so a fill without a base is left for validation to report
// against the stairs, where the author learns to give one.
#[test]
fn a_fill_without_a_base_is_reported_by_validate_rather_than_given_a_base() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .stairs([0.0, 1.0, 2.0], [1.0, 2.0])
        .fill(BLUE)
        .id();
    assert_eq!(stairs(&fig, id).base, None, "no base is invented");
    assert!(
        has_error_at(&fig.validate(), IssueKind::StairsFillWithoutBase, id),
        "{:?}",
        fig.validate()
    );
}
