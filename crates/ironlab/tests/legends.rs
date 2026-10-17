//! Legend panels, legend columns and tile weights built through the user API.

mod common;

use common::axes;
use ironlab::ir::{Cell, InsideLegend, Legend, LegendEntryLayout, PanelLegend};
use ironlab::prelude::*;
use ironlab::{LegendEntries, LegendItem, LegendSample, TrackSize};

/// Returns the legend panel with the given identifier.
#[track_caller]
fn panel(fig: &Figure, id: NodeId) -> &PanelLegend {
    fig.ir()
        .panel_legend(id)
        .unwrap_or_else(|| panic!("figure has no legend panel {id}"))
}

// WHY: a panel is placed in the tile layout exactly as a spanning axes is, and its
// defaults are the ones suited to a key beside or below plots (centred, framed by a border
// that continues the boxes of the plots, as many columns as fit), which differ from those of a legend inside a plot area; a panel created
// with other defaults would look like a misplaced inside legend.
#[test]
fn panel_legend_span_creates_a_panel_in_its_cells_with_the_panel_defaults() {
    let mut fig = Figure::new().tiles(2, 2);
    let id = fig.panel_legend_span(1, 0, 1, 2).id();
    let p = panel(&fig, id);
    assert_eq!(
        p.cell,
        Cell {
            row: 1,
            col: 0,
            row_span: 1,
            col_span: 2
        }
    );
    assert_eq!(p.align, LegendLocation::Center);
    assert!(p.boxed);
    assert_eq!(p.layout.columns, LegendEntryColumns::Auto);
    assert_eq!(p.layout.order, LegendEntryOrder::ColumnMajor);
    assert_eq!(fig.ir().panel_legends.len(), 1);
}

// WHY: users re-borrow a panel by its cell, as they do an axes, to set a property after
// creating it; a second call for the same top-left cell must return the same panel (and
// re-span it) rather than stacking a second panel on the first.
#[test]
fn a_panel_at_the_same_top_left_cell_is_the_same_panel() {
    let mut fig = Figure::new().tiles(2, 2);
    let first = fig.panel_legend(1, 0).id();
    assert_eq!(fig.panel_legend(1, 0).id(), first);
    let spanned = fig.panel_legend_span(1, 0, 1, 2).id();
    assert_eq!(spanned, first);
    assert_eq!(fig.ir().panel_legends.len(), 1);
    assert_eq!(panel(&fig, first).cell.col_span, 2);
}

// WHY: the setters of a panel are the only way user code chooses where its key sits and
// how it is arranged; each must reach the panel, and they must chain off the creating call
// as in the guide's examples.
#[test]
fn panel_setters_chain_and_set_the_panel() {
    let mut fig = Figure::new().tiles(1, 2);
    let id = fig
        .panel_legend(0, 1)
        .align(LegendLocation::NorthWest)
        .boxed(false)
        .columns(LegendEntryColumns::Count(2))
        .order(LegendEntryOrder::RowMajor)
        .id();
    let p = panel(&fig, id);
    assert_eq!(p.align, LegendLocation::NorthWest);
    assert!(!p.boxed);
    assert_eq!(
        p.layout,
        LegendEntryLayout {
            columns: LegendEntryColumns::Count(2),
            order: LegendEntryOrder::RowMajor,
        }
    );
}

// WHY: an axes sends its entries to a panel by naming it; that must replace any legend
// inside its plot area, because the IR allows one or the other, never both.
#[test]
fn legend_in_points_the_axes_at_the_panel_instead_of_an_inside_legend() {
    let mut fig = Figure::new().tiles(1, 2);
    let key = fig.panel_legend(0, 1).id();
    let mut ax = fig.axes(0, 0);
    ax.legend(LegendLocation::SouthWest);
    let ax = ax.legend_in(key).id();
    assert_eq!(axes(&fig, ax).legend, Some(Legend::Panel(key)));
}

// WHY: one key for the whole figure is the common case, so `collect_all` points every axes
// the figure has at the panel in one call; it acts on the axes that exist when it is
// called, so an axes added afterwards keeps its own legend, as the documentation states.
#[test]
fn collect_all_points_every_existing_axes_at_the_panel() {
    let mut fig = Figure::new().tiles(2, 3);
    let a = fig.axes(0, 0).id();
    let b = fig.axes(0, 1).id();
    let c = fig.axes(1, 0).legend(LegendLocation::NorthEast).id();
    let key = fig.panel_legend(1, 2).collect_all().id();
    for id in [a, b, c] {
        assert_eq!(axes(&fig, id).legend, Some(Legend::Panel(key)), "{id}");
    }
    let later = fig.axes(1, 1).id();
    assert_ne!(axes(&fig, later).legend, Some(Legend::Panel(key)));
}

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

// WHY: the item setters build the explicit list one item at a time, in the order the author
// calls them, which is the order of the key; the first call turns automatic entries into an
// explicit list, so that the artists the author did not name are no longer listed.
#[test]
fn panel_item_setters_build_an_explicit_list_in_call_order() {
    let x = linspace(0.0, 1.0, 5);
    let mut fig = Figure::new().tiles(2, 1);
    let mut ax = fig.axes(0, 0);
    let member = ax.plot(&x, &x).display_name("Member").id();
    let base = ax.plot(&x, &x).display_name("Base").id();
    let swatch = LegendSample::swatch(Color::rgb(0.9, 0.6, 0.0));
    let key = fig
        .panel_legend(1, 0)
        .full_row(base)
        .item(member)
        .custom_item(swatch, "Upper surface")
        .id();
    assert_eq!(
        panel(&fig, key).entries,
        LegendEntries::Explicit(vec![
            LegendItem {
                full_row: true,
                ..LegendItem::artist(base)
            },
            LegendItem::artist(member),
            LegendItem::custom(swatch, "Upper surface"),
        ])
    );
    fig.panel_legend(1, 0).entries(LegendEntries::Auto);
    assert_eq!(panel(&fig, key).entries, LegendEntries::Auto);
}

// WHY: a legend inside a plot area takes an explicit list as a panel does; setting it gives
// the axes an inside legend if it had none, as the other inside-legend setters do, and keeps
// the location of one it had.
#[test]
fn legend_entries_sets_the_entries_of_the_inside_legend() {
    let x = linspace(0.0, 1.0, 5);
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let line = ax.plot(&x, &x).display_name("Line").id();
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

/// A stack of three plots over a key of twelve series in one column, in a row of the given
/// size.
fn stack_with_a_key(key_row: TrackSize) -> Figure {
    let x = linspace(0.0, 1.0, 5);
    let mut fig = Figure::new()
        .size_mm(120.0, 120.0)
        .tiles(4, 1)
        .row_weights([
            TrackSize::Weight(1.0),
            TrackSize::Weight(1.0),
            TrackSize::Weight(1.0),
            key_row,
        ]);
    let key = fig
        .panel_legend(3, 0)
        .columns(LegendEntryColumns::Count(1))
        .id();
    for row in 0..3 {
        let mut ax = fig.axes(row, 0);
        for k in 0..12 {
            ax.plot(&x, &x).display_name(format!("Level {k}"));
        }
        ax.legend_in(key);
    }
    fig.link_all_x().unwrap();
    fig
}

/// Returns whether the warnings report a legend that does not fit.
fn overflows(warnings: &[ironlab::SceneWarning]) -> bool {
    warnings
        .iter()
        .any(|w| matches!(w.kind, ironlab::SceneWarningKind::LegendOverflow { .. }))
}

// WHY: a builder chooses the columns of a key, or the size of its row, from whether the key
// fits; it must learn that from the layout alone, without exporting a file, and the answer
// must be the one an export would give.
#[test]
fn layout_warnings_report_an_overflow_that_export_would_report() {
    let fig = stack_with_a_key(TrackSize::Weight(1.0));
    let warnings = fig.layout_warnings();
    assert!(
        overflows(&warnings),
        "twelve entries in one column overflow a quarter of the page"
    );
    let path = std::env::temp_dir().join(format!(
        "ironlab-layout-warnings-{}.pdf",
        std::process::id()
    ));
    let report = fig.export_pdf(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(report.scene, warnings);
}

// WHY: a fitted row exists so that the builder does not need to probe at all: the same key
// in a fitted row fits, whatever the number of entries.
#[test]
fn a_key_in_a_fitted_row_does_not_overflow() {
    let fig = stack_with_a_key(TrackSize::Fit);
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
    let warnings = fig.layout_warnings();
    assert!(!overflows(&warnings), "{warnings:?}");
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

// WHY: the API must produce figures the validator accepts, with panel identifiers drawn
// from the same allocator as axes and artists so that no two nodes share one; this is the
// example of the legends guide, so it must also be free of warnings, every panel having
// entries to list.
#[test]
fn a_figure_with_weights_and_a_shared_panel_validates_cleanly() {
    let x = linspace(0.0, 1.0, 11);
    let a: Vec<f64> = x.iter().map(|v| v * v).collect();
    let mut fig = Figure::new().tiles(2, 3).col_weights([2.0, 2.0, 1.0]);
    let key = fig
        .panel_legend_span(0, 2, 2, 1)
        .columns(LegendEntryColumns::Auto)
        .id();
    for col in 0..2 {
        let mut ax = fig.axes(0, col);
        ax.plot(&x, &a).display_name("Sensor A");
        ax.legend_in(key);
    }
    let report = fig.validate();
    assert_eq!(report.errors, vec![], "{report:?}");
    assert_eq!(report.warnings, vec![], "{report:?}");
}
