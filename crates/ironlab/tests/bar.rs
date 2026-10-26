//! Bar charts in the facade (#92): `bar` and `barh`, the `BarMut` handle that configures the `Bar` artist, and the
//! `bars` helper that writes grouped and stacked bars into ordinary bar series.
//!
//! These tests assert the figure description the facade stores, not its rendering, which the scene tests own. They
//! assume that `AxesMut::bar(positions, values)` and `AxesMut::barh(positions, values)` return a `BarMut`, exported
//! in the prelude, whose setters are `label`, `width_pitch`, `width`, `widths`, `offset_widths`, `offset`, `base`,
//! `base_data`, `margin`, `face_color`, `edge_color`, `edge_width`, `face_opacity`, `edge_opacity`, `color_data`,
//! `color_scale` and `colors`, named as the setters of `SurfaceMut` and `SeriesMut` are.
//!
//! They assume too that `AxesMut::bars(positions)` returns a `BarsMut`, exported in the prelude, which holds one
//! set of positions and adds series to it:
//!
//! ```text
//! pub fn series(&mut self, label: impl Into<Text>, values: impl Into<DataSource>) -> &mut Self; // a column of its own
//! pub fn stacked(&mut self, column: impl Into<String>, label: impl Into<Text>, values: impl Into<DataSource>)
//!     -> &mut Self;                                       // joins the named column and stacks within it
//! pub fn gap(&mut self, gap: f64) -> &mut Self;            // between clusters, as a fraction of the pitch; 0.2
//! pub fn group_gap(&mut self, gap: f64) -> &mut Self;      // between columns, as a fraction of a column; 0
//! pub fn horizontal(&mut self) -> &mut Self;               // every series lies flat
//! pub fn margin(&mut self, margin: f64) -> &mut Self;      // the margin of every series
//! pub fn ids(&self) -> Vec<NodeId>;                        // the series, in the order they were added
//! pub fn last(&mut self) -> BarMut<'_>;                    // the series added last, for styling
//! pub fn get(&mut self, index: usize) -> BarMut<'_>;       // the series of that index, for styling
//! ```
//!
//! Every call of the helper rewrites the width, offset and base of every series it holds, so that the result does not
//! depend on the order of the calls: for `m` columns each series is `Pitch((1 − gap)(1 − group_gap) / m)` wide and
//! the series of column `k` are offset by `Widths((k − (m − 1) / 2) / (1 − group_gap))`, and the series of a stacked
//! column rest on the running totals of the series before them at the same position, positive and negative apart.

mod common;

use common::{bar, data, has_error_at};
use ironlab::IssueKind;
use ironlab::ir::{
    BarOffset, BarWidth, ColorScaleKind, ColorSpec, DashStyle, NdArray, NodeId, Orientation, PerBar,
};
use ironlab::prelude::*;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREY: Color = Color::rgb(0.6, 0.6, 0.6);

/// Agreement expected of the arithmetic of the helper.
const TOL: f64 = 1e-12;

/// The base of each bar of a series, whether it is stored as a constant or as data, so that a test of where a bar
/// starts does not depend on how the facade chose to store it.
fn bases(fig: &Figure, id: NodeId) -> Vec<f64> {
    let b = bar(fig, id);
    let n = data(fig, b.values).len();
    match b.base {
        PerBar::Constant(base) => vec![base; n],
        PerBar::Data(bases) => data(fig, bases)
            .as_f64()
            .expect("bases are numbers")
            .to_vec(),
    }
}

/// The fraction of the pitch that a series of the helper is wide.
#[track_caller]
fn pitch_fraction(fig: &Figure, id: NodeId) -> f64 {
    match bar(fig, id).width {
        BarWidth::Pitch(fraction) => fraction,
        other => panic!("the helper writes widths as fractions of the pitch, not {other:?}"),
    }
}

/// The offset of a series of the helper, in its own widths.
#[track_caller]
fn offset_in_widths(fig: &Figure, id: NodeId) -> f64 {
    match bar(fig, id).offset {
        BarOffset::Widths(widths) => widths,
        other => panic!("the helper writes offsets in widths, not {other:?}"),
    }
}

/// The centre of the column of a series of the helper, relative to its position, as a fraction of the pitch.
fn centre(fig: &Figure, id: NodeId) -> f64 {
    pitch_fraction(fig, id) * offset_in_widths(fig, id)
}

#[track_caller]
fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= TOL,
        "{what}: expected {expected}, got {actual}"
    );
}

#[track_caller]
fn assert_all_close(actual: &[f64], expected: &[f64], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}: {actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert_close(*a, *e, what);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// One series
// ---------------------------------------------------------------------------------------------------------------

// WHY: `bar(x, y)` must draw what MATLAB's and Matplotlib's `bar` draw without further calls: upright bars rising
// from 0 for the lengths given, four fifths of the pitch wide and centred on their positions, in the next colour of
// the colour order with a thin black outline, with the margin that keeps the tallest bar off the box, and without a
// binning record, because plain bars were not made from samples. Bars of fixed colours map nothing, so the call
// creates no colour scale, which would otherwise appear as a stray colourbar or key.
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
    assert_eq!(b.base, PerBar::Constant(0.0));
    assert_eq!(b.width, BarWidth::Pitch(0.8));
    assert_eq!(b.offset, BarOffset::Widths(0.0));
    assert_eq!(b.margin, 0.05);
    assert_eq!(b.binning, None);
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
// property: a fraction of the pitch, a fixed width and per-bar widths (how a histogram of unequal bins is stored) are
// the three forms of the width; an offset in widths and an offset in data units the two forms of the offset; a base
// sets a constant base and per-bar bases a data base (floating bars); the margin is stored as given; and the colour
// setters accept `None` for no face or no outline, as on a surface. A later setter of the same property replaces the
// earlier one, whichever form either took.
#[test]
fn bar_setters_write_the_properties_they_name() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .bar([0.0, 1.0], [2.0, 3.0])
        .label("Revenue")
        .width_pitch(0.5)
        .offset_widths(-1.0)
        .base(1.0)
        .margin(0.0)
        .face_color(RED)
        .edge_color(None)
        .edge_width(1.25)
        .face_opacity(0.5)
        .edge_opacity(0.25)
        .id();
    let b = bar(&fig, id);
    assert_eq!(b.label, Some(Text::new("Revenue")));
    assert_eq!(b.width, BarWidth::Pitch(0.5));
    assert_eq!(b.offset, BarOffset::Widths(-1.0));
    assert_eq!(b.base, PerBar::Constant(1.0));
    assert_eq!(b.margin, 0.0);
    assert_eq!(b.face, ColorSpec::Rgba { color: RED });
    assert_eq!(b.edge.color, ColorSpec::None);
    assert_eq!(b.edge.width_pt, 1.25);
    assert_eq!((b.face_opacity, b.edge_opacity), (0.5, 0.25));
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());

    let id = fig
        .axes(0, 0)
        .bar([0.0, 3.0], [1.0, 2.0])
        .width(0.25)
        .offset(0.3)
        .id();
    let b = bar(&fig, id);
    assert_eq!(b.width, BarWidth::Constant(0.25));
    assert_eq!(b.offset, BarOffset::Absolute(0.3));

    let id = fig
        .axes(0, 0)
        .bar([0.5, 2.0], [4.0, 1.0])
        .width_pitch(0.5)
        .widths([1.0, 2.0])
        .offset(0.3)
        .offset_widths(0.5)
        .base(2.0)
        .base_data([0.5, -1.0])
        .id();
    let b = bar(&fig, id);
    let BarWidth::Data(widths) = b.width else {
        panic!("per-bar widths are a data width, not {:?}", b.width);
    };
    assert_eq!(data(&fig, widths), &NdArray::vector(vec![1.0, 2.0]));
    assert_eq!(
        b.offset,
        BarOffset::Widths(0.5),
        "the later offset replaces the earlier"
    );
    let PerBar::Data(bases) = b.base else {
        panic!("per-bar bases are a data base, not {:?}", b.base);
    };
    assert_eq!(data(&fig, bases), &NdArray::vector(vec![0.5, -1.0]));
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());

    // An offset in widths is a multiple of whatever width the bars have, so setting the width after it must leave
    // it a multiple, not convert it into the distance it was at the earlier width.
    let id = fig
        .axes(0, 0)
        .bar([0.0, 1.0], [1.0, 2.0])
        .offset_widths(1.0)
        .width(0.25)
        .id();
    assert_eq!(bar(&fig, id).offset, BarOffset::Widths(1.0));
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

// ---------------------------------------------------------------------------------------------------------------
// The `bars` helper: grouped bars
// ---------------------------------------------------------------------------------------------------------------

// WHY: a grouped comparison (three models scored on three properties) must come out of the helper as one ordinary bar
// series per model, each with its own label, values and colour from the colour order, all at the helper's positions,
// so that legends, selection and the property editor treat each model as they treat any series. At the default gap
// of 0.2 and no gap within a cluster, three columns are each a third of four fifths of the pitch wide and are offset
// by −1, 0 and 1 of their widths, which is the geometry the scene draws as three touching bars centred on each
// position; and the figure is valid without a warning, so the recipe users follow gives a clean figure.
#[test]
fn series_of_the_helper_are_columns_a_third_of_the_drawn_cluster_wide_side_by_side() {
    let mut fig = Figure::new();
    let ids = fig
        .axes(0, 0)
        .bars([0.0, 1.0, 2.0])
        .series("Haiku", [1.0, 2.0, 3.0])
        .series("ChatGPT", [2.0, 1.0, 2.0])
        .series("Gemini", [3.0, 3.0, 1.0])
        .ids();
    assert_eq!(ids.len(), 3, "one series per call");
    for (k, (&id, (name, values))) in ids
        .iter()
        .zip([
            ("Haiku", [1.0, 2.0, 3.0]),
            ("ChatGPT", [2.0, 1.0, 2.0]),
            ("Gemini", [3.0, 3.0, 1.0]),
        ])
        .enumerate()
    {
        let b = bar(&fig, id);
        assert_eq!(b.label, Some(Text::new(name)), "column {k}");
        assert_eq!(data(&fig, b.values), &NdArray::vector(values.to_vec()));
        assert_eq!(
            data(&fig, b.positions),
            &NdArray::vector(vec![0.0, 1.0, 2.0])
        );
        assert_eq!(b.orientation, Orientation::Vertical);
        assert_eq!(b.face, ColorSpec::Auto, "column {k} takes the colour order");
        assert_eq!(b.margin, 0.05, "column {k} keeps the default margin");
        assert_close(pitch_fraction(&fig, id), 0.8 / 3.0, "a third of 0.8");
        assert_close(
            offset_in_widths(&fig, id),
            k as f64 - 1.0,
            "offset by whole widths",
        );
        assert_all_close(&bases(&fig, id), &[0.0; 3], "grouped columns rise from 0");
    }
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: the gap between clusters and the gap within a cluster are the two spacings a grouped chart is tuned by, and
// each must mean what it says whatever the number of columns: neighbouring columns are a column's width plus the
// gap within the cluster apart, so that with no gap within they touch exactly; the cluster is centred on its
// position, so that the space either side of it is the same; and the outer edges of the cluster leave the gap
// between clusters, as a fraction of the pitch, between one cluster and the next.
#[test]
fn the_gap_between_clusters_and_the_gap_within_a_cluster_space_the_columns_symmetrically() {
    for (m, gap, group_gap) in [(2, 0.2, 0.0), (3, 0.3, 0.1), (4, 0.0, 0.25)] {
        let mut fig = Figure::new();
        let ids = {
            let mut ax = fig.axes(0, 0);
            let mut bars = ax.bars([0.0, 1.0]);
            bars.gap(gap).group_gap(group_gap);
            for k in 0..m {
                bars.series(format!("s{k}"), [1.0, 2.0]);
            }
            bars.ids()
        };
        let what = format!("{m} columns, gap {gap}, gap within {group_gap}");
        let width = (1.0 - gap) * (1.0 - group_gap) / m as f64;
        let slot = (1.0 - gap) / m as f64;
        for (k, &id) in ids.iter().enumerate() {
            assert_close(pitch_fraction(&fig, id), width, &format!("{what}: width"));
            assert_close(
                centre(&fig, id),
                (k as f64 - (m as f64 - 1.0) / 2.0) * slot,
                &format!("{what}: centre of column {k}"),
            );
        }
        for pair in ids.windows(2) {
            let space = centre(&fig, pair[1]) - centre(&fig, pair[0]) - width;
            assert_close(space, slot * group_gap, &format!("{what}: space within"));
        }
        let (first, last) = (ids[0], ids[m - 1]);
        let left = centre(&fig, first) - width / 2.0;
        let right = centre(&fig, last) + width / 2.0;
        assert_close(left, -right, &format!("{what}: the cluster is centred"));
        assert_close(
            1.0 - (right - left),
            gap + slot * group_gap,
            &format!(
                "{what}: the gap between clusters, with half a gap within beside each outer column"
            ),
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The `bars` helper: stacked bars
// ---------------------------------------------------------------------------------------------------------------

// WHY: a stack shows a total and its parts, so each part must rest exactly where the parts before it ended at the
// same position, in the order they were added. Positive and negative values stack separately, upwards and downwards
// from 0 (Plotly's relative mode): were a negative part stacked onto the top of a positive one it would hide inside
// it and the column would no longer show the positive total. One column of parts is as wide as a lone column, and
// every part shares its offset.
#[test]
fn stacked_series_rest_on_the_running_totals_of_the_parts_before_them_positive_and_negative_apart()
{
    let mut fig = Figure::new();
    let ids = fig
        .axes(0, 0)
        .bars([0.0, 1.0, 2.0])
        .stacked("total", "A", [1.0, -2.0, 3.0])
        .stacked("total", "B", [2.0, -1.0, -1.0])
        .stacked("total", "C", [0.5, 4.0, 1.0])
        .ids();
    // Position 0: all positive, so 0 → 1 → 3 → 3.5. Position 1: A and B are negative and stack downwards, and C is
    // positive and rises from 0, not from them. Position 2: B is negative between two positive parts, so C rests on
    // A, the last positive part.
    assert_all_close(&bases(&fig, ids[0]), &[0.0, 0.0, 0.0], "A rests on 0");
    assert_all_close(&bases(&fig, ids[1]), &[1.0, -2.0, 0.0], "B rests on A");
    assert_all_close(
        &bases(&fig, ids[2]),
        &[3.0, 0.0, 3.0],
        "C rests on the parts of its sign",
    );
    for &id in &ids {
        assert_close(
            pitch_fraction(&fig, id),
            0.8,
            "a column of parts is as wide as a lone column",
        );
        assert_close(
            offset_in_widths(&fig, id),
            0.0,
            "and centred on its position",
        );
        assert_eq!(bar(&fig, id).orientation, Orientation::Vertical);
    }
    let report = fig.validate();
    assert!(
        report.is_valid() && report.warnings.is_empty(),
        "{report:?}"
    );
}

// WHY: a part of a stack that is missing (NaN) has no length, so it contributes nothing: the part above it rests
// where the part below it ended. Adding the missing value to the running total would make every later part at that
// position rest on NaN, and the column would lose its top for one missing number.
#[test]
fn a_part_of_a_stack_without_a_finite_value_contributes_nothing_to_the_parts_above_it() {
    let mut fig = Figure::new();
    let ids = fig
        .axes(0, 0)
        .bars([0.0, 1.0])
        .stacked("total", "A", [1.0, f64::NAN])
        .stacked("total", "B", [f64::INFINITY, 2.0])
        .stacked("total", "C", [2.0, 2.0])
        .ids();
    assert_all_close(
        &bases(&fig, ids[1]),
        &[1.0, 0.0],
        "B rests on A, or on 0 where A is missing",
    );
    assert_all_close(
        &bases(&fig, ids[2]),
        &[1.0, 2.0],
        "C passes over the missing parts",
    );
}

// WHY: stacks side by side (gains and losses of two years) and lone columns beside stacks are common; each named
// column is one column of the cluster, its parts stacking within it alone, and a series added with `series` is a
// column of its own. Columns are placed in the order they first appear, so a part added to an earlier column later
// joins that column rather than starting a new one.
#[test]
fn stacked_columns_stand_side_by_side_with_each_other_and_with_lone_columns() {
    let mut fig = Figure::new();
    let ids = fig
        .axes(0, 0)
        .bars([0.0, 1.0])
        .series("Haiku", [1.0, 1.0])
        .stacked("2026", "gains", [2.0, 3.0])
        .stacked("2025", "gains", [1.0, -1.0])
        .stacked("2026", "losses", [-1.0, 4.0])
        .stacked("2025", "losses", [2.0, -2.0])
        .ids();
    let [haiku, gains_26, gains_25, losses_26, losses_25] = ids[..] else {
        panic!("five series: {ids:?}");
    };
    let width = 0.8 / 3.0;
    for (id, column) in [
        (haiku, 0),
        (gains_26, 1),
        (losses_26, 1),
        (gains_25, 2),
        (losses_25, 2),
    ] {
        assert_close(pitch_fraction(&fig, id), width, "three columns");
        assert_close(
            offset_in_widths(&fig, id),
            column as f64 - 1.0,
            "in the order they first appear",
        );
    }
    assert_all_close(
        &bases(&fig, haiku),
        &[0.0, 0.0],
        "a lone column rises from 0",
    );
    assert_all_close(
        &bases(&fig, losses_26),
        &[0.0, 3.0],
        "2026 stacks on 2026 alone",
    );
    assert_all_close(
        &bases(&fig, losses_25),
        &[1.0, -1.0],
        "2025 stacks on 2025 alone",
    );
    let report = fig.validate();
    assert!(
        report.is_valid() && report.warnings.is_empty(),
        "{report:?}"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// The `bars` helper: order of calls, orientation, margin and styling
// ---------------------------------------------------------------------------------------------------------------

/// What the helper decides of a series, independent of the identifiers of its node and arrays.
fn layout_of(
    fig: &Figure,
    id: NodeId,
) -> (
    Option<Text>,
    Orientation,
    BarWidth,
    BarOffset,
    Vec<f64>,
    f64,
) {
    let b = bar(fig, id);
    (
        b.label.clone(),
        b.orientation,
        b.width,
        b.offset,
        bases(fig, id),
        b.margin,
    )
}

// WHY: every call of the helper rewrites the widths, offsets and bases of all its series, so the result must be the
// same whatever the order of the calls: setting the gaps, the orientation or the margin before the series or after
// them, and adding a column after others have been laid out (which narrows and moves them), must give identical
// series. A result that depended on the order would make the chart depend on how the code happened to be written.
#[test]
fn the_helper_gives_the_same_series_whatever_the_order_of_its_calls() {
    let mut before = Figure::new();
    let first = before
        .axes(0, 0)
        .bars([0.0, 1.0, 2.0])
        .gap(0.1)
        .group_gap(0.05)
        .horizontal()
        .margin(0.0)
        .series("Haiku", [1.0, 2.0, 3.0])
        .stacked("2026", "gains", [2.0, -1.0, 1.0])
        .stacked("2026", "losses", [-1.0, -2.0, 2.0])
        .series("ChatGPT", [3.0, 2.0, 1.0])
        .ids();
    let mut after = Figure::new();
    let second = after
        .axes(0, 0)
        .bars([0.0, 1.0, 2.0])
        .series("Haiku", [1.0, 2.0, 3.0])
        .stacked("2026", "gains", [2.0, -1.0, 1.0])
        .series("ChatGPT", [3.0, 2.0, 1.0])
        .margin(0.0)
        .horizontal()
        .stacked("2026", "losses", [-1.0, -2.0, 2.0])
        .group_gap(0.05)
        .gap(0.1)
        .ids();
    let by_label = |fig: &Figure, ids: &[NodeId]| {
        let mut layouts: Vec<_> = ids.iter().map(|&id| layout_of(fig, id)).collect();
        layouts.sort_by_key(|layout| format!("{:?}", layout.0));
        layouts
    };
    let (a, b) = (by_label(&before, &first), by_label(&after, &second));
    assert_eq!(a.len(), 4);
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(
            (&x.0, x.1, x.5),
            (&y.0, y.1, y.5),
            "label, orientation and margin"
        );
        assert_eq!(x.2, y.2, "{:?}: width", x.0);
        assert_eq!(x.3, y.3, "{:?}: offset", x.0);
        assert_all_close(&x.4, &y.4, &format!("{:?}: bases", x.0));
        assert_eq!(x.1, Orientation::Horizontal, "every series lies flat");
        assert_eq!(x.5, 0.0, "every series takes the margin");
    }
    for fig in [&before, &after] {
        let report = fig.validate();
        assert!(
            report.is_valid() && report.warnings.is_empty(),
            "{report:?}"
        );
    }
}

// WHY: a series of a grouped chart is styled like any other, so the helper must hand out the `BarMut` of the series
// added last or of any series by its index; and a style set that way must survive the later calls of the helper,
// which rewrite only where the bars are placed, or styling a column before adding the next would be undone.
#[test]
fn the_helper_hands_out_each_series_for_styling_and_keeps_its_style_through_later_calls() {
    let mut fig = Figure::new();
    let ids = {
        let mut ax = fig.axes(0, 0);
        let mut bars = ax.bars([0.0, 1.0]);
        bars.series("A", [1.0, 2.0]);
        bars.last().face_color(RED).edge_width(1.5);
        bars.series("B", [2.0, 1.0]);
        bars.get(1).face_color(GREY);
        bars.series("C", [1.0, 1.0]);
        bars.ids()
    };
    assert_eq!(bar(&fig, ids[0]).face, ColorSpec::Rgba { color: RED });
    assert_eq!(bar(&fig, ids[0]).edge.width_pt, 1.5);
    assert_eq!(bar(&fig, ids[1]).face, ColorSpec::Rgba { color: GREY });
    assert_eq!(bar(&fig, ids[2]).face, ColorSpec::Auto);
    assert_close(
        pitch_fraction(&fig, ids[0]),
        0.8 / 3.0,
        "the styled series was laid out again when C was added",
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}
