//! Inside legends and tile weights built through the user API. Legend panels are tested in
//! `panels.rs`.

mod common;

use common::axes;
use ironlab::ir::{InsideLegend, Legend, LegendEntryLayout};
use ironlab::prelude::*;
use ironlab::{LegendEntries, LegendItem, TrackSize};

// WHY: weights let a narrow panel and wide plots share a row; they are given as any list
// of numbers, or of sizes that mix weights and fitted tracks, from the top or from the
// left, and must reach the layout in that order.
#[test]
fn row_and_column_sizes_are_set_in_order() {
    let fig = Figure::new()
        .tiles(2, 3)
        .row_weights([3.0, 1.0])
        .col_weights(vec![
            TrackSize::Weight(2.0),
            TrackSize::Fit,
            TrackSize::Weight(1.0),
        ]);
    assert_eq!(
        fig.ir().layout.row_weights,
        vec![TrackSize::Weight(3.0), TrackSize::Weight(1.0)]
    );
    assert_eq!(
        fig.ir().layout.col_weights,
        vec![
            TrackSize::Weight(2.0),
            TrackSize::Fit,
            TrackSize::Weight(1.0)
        ]
    );
}

// WHY: fitting one row is the common case (a key beneath a stack of plots), so it must not
// require the author to restate the weights of every other row: the rows before it that
// have no size take the weight of one they had implicitly, and sizes already given are
// kept.
#[test]
fn fit_row_and_fit_col_keep_the_other_sizes() {
    let fig = Figure::new()
        .tiles(4, 3)
        .row_weights([2.0])
        .fit_row(2)
        .fit_col(0);
    assert_eq!(
        fig.ir().layout.row_weights,
        vec![
            TrackSize::Weight(2.0),
            TrackSize::Weight(1.0),
            TrackSize::Fit
        ]
    );
    assert_eq!(fig.ir().layout.col_weights, vec![TrackSize::Fit]);
    let refitted = Figure::new().tiles(2, 1).row_weights([3.0, 4.0]).fit_row(0);
    assert_eq!(
        refitted.ir().layout.row_weights,
        vec![TrackSize::Fit, TrackSize::Weight(4.0)]
    );
}

// WHY: a legend inside a plot area takes an explicit list as a panel does; setting it gives
// the axes an inside legend if it had none, as the other inside-legend setters do, and keeps
// the location of one it had.
#[test]
fn legend_entries_sets_the_entries_of_the_inside_legend() {
    let x = linspace(0.0, 1.0, 5);
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let line = ax.plot(&x, &x).label("Line").id();
    ax.legend(LegendLocation::SouthWest)
        .legend_entries(LegendEntries::Explicit(vec![LegendItem::artist(line)]));
    let id = ax.id();
    assert_eq!(
        axes(&fig, id).legend,
        Some(Legend::Inside(InsideLegend {
            location: LegendLocation::SouthWest,
            entries: LegendEntries::Explicit(vec![LegendItem::artist(line)]),
            ..InsideLegend::default()
        }))
    );
    assert!(fig.validate().is_valid());
}

// WHY: asking for legend columns on an axes without a legend means the user wants a
// legend; creating the default inside legend with those columns does what they asked,
// whereas setting columns on nothing would silently do nothing. An existing inside legend
// keeps its location and box.
#[test]
fn legend_columns_creates_a_default_inside_legend_or_keeps_the_existing_one() {
    let mut fig = Figure::new().tiles(1, 2);
    let bare = fig
        .axes(0, 0)
        .legend_columns(LegendEntryColumns::Count(2))
        .id();
    assert_eq!(
        axes(&fig, bare).legend,
        Some(Legend::Inside(InsideLegend {
            layout: LegendEntryLayout {
                columns: LegendEntryColumns::Count(2),
                ..LegendEntryLayout::default()
            },
            ..InsideLegend::default()
        }))
    );

    let placed = fig
        .axes(0, 1)
        .legend(LegendLocation::SouthWest)
        .legend_columns(LegendEntryColumns::Count(3))
        .id();
    let Some(Legend::Inside(legend)) = &axes(&fig, placed).legend else {
        panic!("an inside legend");
    };
    assert_eq!(legend.location, LegendLocation::SouthWest);
    assert_eq!(legend.layout.columns, LegendEntryColumns::Count(3));
}

// WHY: an inside legend must be as fully configurable from the axes handle as a panel is
// from its own, so that nobody has to edit the model directly for a row-major or unboxed
// legend; like `legend_columns`, each setter keeps the other settings of an existing legend.
#[test]
fn legend_boxed_and_legend_order_set_the_inside_legend_and_keep_its_other_settings() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .legend(LegendLocation::North)
        .legend_columns(LegendEntryColumns::Count(3))
        .legend_order(LegendEntryOrder::RowMajor)
        .legend_boxed(false)
        .id();
    assert_eq!(
        axes(&fig, id).legend,
        Some(Legend::Inside(InsideLegend {
            location: LegendLocation::North,
            boxed: false,
            layout: LegendEntryLayout {
                columns: LegendEntryColumns::Count(3),
                order: LegendEntryOrder::RowMajor,
            },
            entries: LegendEntries::Auto,
        }))
    );
}
