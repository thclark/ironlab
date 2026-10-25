//! Bar charts in the facade (#92): `bar` and `barh`, and the `BarMut` handle that configures the `Bar` artist.
//!
//! These tests assert the figure description the facade stores, not its rendering, which the scene tests own. They
//! assume that `AxesMut::bar(positions, values)` and `AxesMut::barh(positions, values)` return a `BarMut`, exported
//! in the prelude, whose setters are `label`, `width`, `widths`, `gap`, `base`, `base_data`, `group`, `face_color`,
//! `edge_color`, `edge_width`, `face_opacity`, `edge_opacity`, `color_data`, `color_scale` and `colors`, named as
//! the setters of `SurfaceMut` and `SeriesMut` are.

mod common;

use common::{bar, data, has_error_at, has_warning};
use ironlab::IssueKind;
use ironlab::ir::{
    BarBase, BarGroup, BarLayout, BarSlot, ColorScaleKind, ColorSpec, DashStyle, NdArray,
    Orientation,
};
use ironlab::prelude::*;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREY: Color = Color::rgb(0.6, 0.6, 0.6);

// WHY: `bar(x, y)` must draw what MATLAB's and Matplotlib's `bar` draw without further calls: upright bars rising
// from 0, across four fifths of the pitch, in the next colour of the colour order with a thin black outline, on
// their own rather than in a group. Bars of fixed colours map nothing, so the call creates no colour scale, which
// would otherwise appear as a stray colourbar or key.
#[test]
fn bar_stores_the_positions_and_values_with_the_defaults_of_a_bar_chart() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).bar([1.0, 2.0, 3.0], [3.0, 1.0, 2.0]).id();
    let b = bar(&fig, id);
    assert_eq!(
        data(&fig, b.positions),
        &NdArray::vector(vec![1.0, 2.0, 3.0])
    );
    assert_eq!(data(&fig, b.values), &NdArray::vector(vec![3.0, 1.0, 2.0]));
    assert_eq!(b.orientation, Orientation::Vertical);
    assert_eq!(b.base, BarBase::Constant(0.0));
    assert_eq!(b.slot, BarSlot::Pitch);
    assert_eq!(b.gap, 0.2);
    assert_eq!(b.group, None);
    assert_eq!(b.face, ColorSpec::Auto);
    assert_eq!(
        b.edge.color,
        ColorSpec::Rgba {
            color: Color::BLACK
        }
    );
    assert_eq!(b.edge.width_pt, 0.5);
    assert_eq!(b.edge.dash, DashStyle::Solid);
    assert_eq!((b.color_data, b.color_scale), (None, None));
    assert!(
        fig.ir().color_scales.is_empty(),
        "no colour scale is created"
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: `barh` is the same chart lying flat, which is how long category names are made readable; it must differ from
// `bar` in its orientation only, so that every other setter means the same for both.
#[test]
fn barh_differs_from_bar_only_in_lying_flat() {
    let mut fig = Figure::new();
    let upright = fig.axes(0, 0).bar([0.0, 1.0], [2.0, 3.0]).id();
    let flat = fig.axes(0, 0).barh([0.0, 1.0], [2.0, 3.0]).id();
    let (upright, flat) = (bar(&fig, upright).clone(), bar(&fig, flat).clone());
    assert_eq!(flat.orientation, Orientation::Horizontal);
    assert_eq!(
        ironlab::ir::Bar {
            id: upright.id,
            positions: upright.positions,
            values: upright.values,
            orientation: Orientation::Horizontal,
            ..upright
        },
        ironlab::ir::Bar {
            id: upright.id,
            positions: upright.positions,
            values: upright.values,
            ..flat
        },
        "only the orientation differs"
    );
}

// WHY: each setter is the facade's name for one property of the bar artist, so each must write exactly that
// property: a fixed width sets a width slot and per-bar widths a data slot (how a histogram of unequal bins is
// stored), a base sets a constant base and per-bar bases a data base (floating bars), and the colour setters accept
// `None` for no face or no outline, as on a surface.
#[test]
fn bar_setters_write_the_properties_they_name() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .bar([0.0, 1.0], [2.0, 3.0])
        .label("Revenue")
        .width(0.5)
        .gap(0.1)
        .base(1.0)
        .face_color(RED)
        .edge_color(None)
        .edge_width(1.25)
        .face_opacity(0.5)
        .edge_opacity(0.25)
        .id();
    let b = bar(&fig, id);
    assert_eq!(b.label, Some(Text::new("Revenue")));
    assert_eq!(b.slot, BarSlot::Width(0.5));
    assert_eq!(b.gap, 0.1);
    assert_eq!(b.base, BarBase::Constant(1.0));
    assert_eq!(b.face, ColorSpec::Rgba { color: RED });
    assert_eq!(b.edge.color, ColorSpec::None);
    assert_eq!(b.edge.width_pt, 1.25);
    assert_eq!((b.face_opacity, b.edge_opacity), (0.5, 0.25));
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());

    let id = fig
        .axes(0, 0)
        .bar([0.5, 2.0], [4.0, 1.0])
        .widths([1.0, 2.0])
        .base_data([0.5, -1.0])
        .id();
    let b = bar(&fig, id);
    let BarSlot::Data(widths) = b.slot else {
        panic!("per-bar widths are a data slot, not {:?}", b.slot);
    };
    assert_eq!(data(&fig, widths), &NdArray::vector(vec![1.0, 2.0]));
    let BarBase::Data(bases) = b.base else {
        panic!("per-bar bases are a data base, not {:?}", b.base);
    };
    assert_eq!(data(&fig, bases), &NdArray::vector(vec![0.5, -1.0]));
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: a grouped comparison is made by giving several series the same group, each with its own label and colour;
// the group is stored by name and layout, and a group whose members agree is valid without a warning, so the
// recipe users follow gives a clean figure.
#[test]
fn group_puts_series_in_a_named_group_with_its_layout() {
    let mut fig = Figure::new();
    let mut ids = Vec::new();
    {
        let mut ax = fig.axes(0, 0);
        for (name, values) in [
            ("A", [1.0, 2.0, 3.0]),
            ("B", [2.0, 1.0, 2.0]),
            ("C", [3.0, 3.0, 1.0]),
        ] {
            ids.push(
                ax.bar([0.0, 1.0, 2.0], values)
                    .label(name)
                    .group("models", BarLayout::Grouped)
                    .id(),
            );
        }
        ax.bar([0.0, 1.0], [1.0, 2.0])
            .group("parts", BarLayout::Stacked);
    }
    for id in ids {
        assert_eq!(
            bar(&fig, id).group,
            Some(BarGroup {
                name: "models".to_owned(),
                layout: BarLayout::Grouped,
            })
        );
    }
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(
        !has_warning(&report, IssueKind::InconsistentBarGroup),
        "{report:?}"
    );
}

// WHY: colouring bars by a quantity follows the series recipe: colour data stores one value per bar, colours the
// faces through the default colour scale of the axes, which it names, and gives a valid figure with no further
// call; naming another scale replaces the default, which is deleted when nothing else names it.
#[test]
fn colour_data_colours_the_faces_through_the_default_scale_or_a_named_one() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .bar([0.0, 1.0, 2.0], [3.0, 1.0, 2.0])
        .color_data([0.1, 0.5, 0.9])
        .id();
    let default = fig.axes(0, 0).color_scale().id();
    let b = bar(&fig, id);
    let colours = b.color_data.expect("the bars have colour data");
    assert_eq!(data(&fig, colours), &NdArray::vector(vec![0.1, 0.5, 0.9]));
    assert_eq!(b.face, ColorSpec::Colormapped);
    assert_eq!(b.color_scale, Some(default));
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());

    let mut fig = Figure::new();
    let chosen = fig.indexed_scale([Color::BLACK, Color::WHITE]).id();
    let id = fig
        .axes(0, 0)
        .bar([0.0, 1.0], [3.0, 1.0])
        .color_data([0.0, 1.0])
        .color_scale(chosen)
        .id();
    assert_eq!(bar(&fig, id).color_scale, Some(chosen));
    assert_eq!(fig.ir().color_scales.len(), 1, "the default is deleted");
    assert!(fig.validate().is_valid());
}

// WHY: highlighting one bar, or colouring bars by category, is the most common reason to colour bars individually;
// `colors` gives one colour per bar and stores it the way the model colours individual bars: an indexed colour scale
// of the distinct colours, and per bar the index of its colour as colour data, so that each bar is drawn in exactly
// the colour given and a key of the scale lists each colour once. Giving colours again replaces them, and the
// figure keeps no scale the author no longer uses.
#[test]
fn colors_stores_an_indexed_scale_of_the_distinct_colours_and_each_bars_index() {
    let given = [GREY, GREY, RED, GREY];
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .bar([0.0, 1.0, 2.0, 3.0], [3.0, 1.0, 4.0, 2.0])
        .colors(given)
        .id();
    let b = bar(&fig, id).clone();
    assert_eq!(b.face, ColorSpec::Colormapped);
    let scale = b.color_scale.expect("the bars name a scale");
    let palette = match &fig.ir().color_scale(scale).expect("the scale exists").kind {
        ColorScaleKind::Indexed(indexed) => indexed.colors.clone(),
        other => panic!("the scale is indexed, not {other:?}"),
    };
    assert_eq!(
        palette.len(),
        2,
        "one entry per distinct colour: {palette:?}"
    );
    let indices = data(&fig, b.color_data.expect("the bars have colour data"))
        .as_f64()
        .expect("indices are stored as numbers")
        .to_vec();
    assert_eq!(indices.len(), given.len(), "one index per bar");
    for (k, (&index, colour)) in indices.iter().zip(given).enumerate() {
        assert_eq!(index.fract(), 0.0, "bar {k} names an entry");
        assert_eq!(
            palette[index as usize], colour,
            "bar {k} is drawn in its own colour"
        );
    }
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());

    let id = fig
        .axes(0, 0)
        .bar([0.0, 1.0], [1.0, 2.0])
        .colors([RED, GREY])
        .colors([GREY, GREY])
        .id();
    let b = bar(&fig, id);
    assert_eq!(
        fig.ir().color_scales.len(),
        2,
        "the replaced scale is not kept: {:?}",
        fig.ir().color_scales
    );
    let ColorScaleKind::Indexed(indexed) =
        &fig.ir().color_scale(b.color_scale.unwrap()).unwrap().kind
    else {
        panic!("the scale is indexed");
    };
    assert_eq!(indexed.colors, vec![GREY]);
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: categorical bars are bars at 0, 1, …, N − 1 named by text tick labels, the recipe the getting-started guide
// gives; the recipe must give a valid figure whose category axis carries one name per bar.
#[test]
fn categorical_bars_are_bars_at_whole_numbers_named_by_text_tick_labels() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.bar([0.0, 1.0, 2.0], [5.0, 3.0, 4.0]);
    ax.xticks([0.0, 1.0, 2.0])
        .xtick_labels(["North", "South", "East"]);
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: positions and values of different lengths are a mistake in the user's data, which must be reported by
// validation against the bar series rather than panic in the facade or draw a misleading chart.
#[test]
fn positions_and_values_of_different_lengths_are_reported_by_validate() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).bar([0.0, 1.0, 2.0], [1.0, 2.0]).id();
    assert!(
        has_error_at(&fig.validate(), IssueKind::ShapeMismatch, id),
        "{:?}",
        fig.validate()
    );
}
