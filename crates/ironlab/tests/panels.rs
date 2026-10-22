//! Panels built through the user API: nodes that occupy cells of the tile layout, as an axes
//! does, and hold one key, which in this stage is always a legend.
//!
//! The API these tests assume, which replaces `Figure::panel_legend` and
//! `Figure::panel_legend_span`:
//!
//! - `Figure::panel(&mut self, row: u32, col: u32) -> PanelMut<'_>` and
//!   `Figure::panel_span(&mut self, row, col, row_span, col_span) -> PanelMut<'_>` return
//!   the panel whose top-left tile is at the row and column, creating one if there is none,
//!   as `axes` and `axes_span` do; `panel_span` re-spans an existing panel.
//! - `PanelMut::align(&mut self, PanelLocation) -> &mut Self` and
//!   `PanelMut::id(&self) -> NodeId`.
//! - `PanelMut::legend(&mut self) -> PanelLegendMut<'_>` makes the content of the panel a
//!   legend, keeping the legend it already holds, and returns the handle that offers what
//!   `panel_legend` offered (`boxed`, `columns`, `order`, `entries`, `item`, `full_row`,
//!   `custom_item`, `collect_all` and `id`), without `align`, which is now the panel's.
//!   The `id` of that handle is the identifier of the panel.
//! - A panel created without a call to `legend` holds a default legend, because a legend is
//!   the only content a panel has in this stage; `legend` (and `colorbar` in stage 4)
//!   replaces content of another kind and keeps content of its own kind.
//! - `PanelLocation` is exported by the prelude, as `LegendLocation` is.

use ironlab::ir::{Cell, Legend, LegendEntryLayout, Panel, PanelContent, PanelLegend};
use ironlab::prelude::*;
use ironlab::{LegendEntries, LegendItem, LegendSample, TrackSize};

/// Returns the panel with the given identifier.
#[track_caller]
fn panel(fig: &Figure, id: NodeId) -> &Panel {
    fig.ir()
        .panel(id)
        .unwrap_or_else(|| panic!("figure has no panel {id}"))
}

/// Returns the legend that the panel with the given identifier holds.
#[track_caller]
fn legend(fig: &Figure, id: NodeId) -> &PanelLegend {
    match &panel(fig, id).content {
        PanelContent::Legend(legend) => legend,
        #[allow(unreachable_patterns)]
        other => panic!("panel {id} holds {other:?}, not a legend"),
    }
}

// PROMISE: a legend panel is placed in the tile layout exactly as a spanning axes is, and
// has the defaults suited to a key beside or below plots (centred, framed by a border that
// continues the boxes of the plots, as many columns as fit), which differ from those of a
// legend inside a plot area.
#[test]
fn panel_span_legend_creates_a_legend_panel_in_its_cells_with_the_panel_defaults() {
    let mut fig = Figure::new().tiles(2, 2);
    let id = fig.panel_span(1, 0, 1, 2).legend().id();
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
    assert_eq!(p.align, PanelLocation::Center);
    let l = legend(&fig, id);
    assert!(l.boxed);
    assert_eq!(l.layout.columns, LegendEntryColumns::Auto);
    assert_eq!(l.layout.order, LegendEntryOrder::ColumnMajor);
    assert_eq!(l.entries, LegendEntries::Auto);
    assert_eq!(fig.ir().panels.len(), 1);
}

// PROMISE: a panel always holds content, so one created without saying what it holds is
// a legend panel with the defaults above, the only content a panel has in this stage,
// and an axes can send its entries to it straight away.
#[test]
fn a_panel_created_without_content_is_a_legend_panel() {
    let mut fig = Figure::new().tiles(1, 2);
    let bare = fig.panel(0, 1).id();
    let explicit = fig.panel(0, 0).legend().id();
    assert_eq!(legend(&fig, bare), legend(&fig, explicit));
    assert_eq!(panel(&fig, bare).align, panel(&fig, explicit).align);
}

// PROMISE: users re-borrow a panel by its cell, as they do an axes, to change it after
// creating it; a second call for the same top-left cell returns the same panel (re-spanned
// by `panel_span`) rather than stacking a second one on it, and asking again for its legend
// keeps the legend's settings rather than resetting them.
#[test]
fn a_panel_at_the_same_top_left_cell_is_the_same_panel_and_keeps_its_legend() {
    let mut fig = Figure::new().tiles(2, 2);
    let first = fig.panel(1, 0).legend().boxed(false).id();
    assert_eq!(fig.panel(1, 0).id(), first);
    let spanned = fig.panel_span(1, 0, 1, 2).legend().id();
    assert_eq!(spanned, first);
    assert_eq!(fig.ir().panels.len(), 1);
    assert_eq!(panel(&fig, first).cell.col_span, 2);
    assert!(!legend(&fig, first).boxed, "the legend kept its settings");
    // A panel is found by its cell in the IR, so it is found in a wrapped figure too.
    let mut wrapped = Figure::from_ir(fig.into_ir());
    assert_eq!(wrapped.panel(1, 0).id(), first);
    assert_eq!(wrapped.ir().panels.len(), 1);
}

// PROMISE: the setters are the only way user code chooses where a key sits in its panel
// and how its entries are arranged; each must reach the IR, and they chain off the
// creating call as in the guide's examples, the panel's own first and then its legend's.
#[test]
fn panel_and_legend_setters_chain_and_set_the_panel() {
    let mut fig = Figure::new().tiles(1, 2);
    let id = fig
        .panel(0, 1)
        .align(PanelLocation::NorthWest)
        .legend()
        .boxed(false)
        .columns(LegendEntryColumns::Count(2))
        .order(LegendEntryOrder::RowMajor)
        .id();
    assert_eq!(panel(&fig, id).align, PanelLocation::NorthWest);
    let l = legend(&fig, id);
    assert!(!l.boxed);
    assert_eq!(
        l.layout,
        LegendEntryLayout {
            columns: LegendEntryColumns::Count(2),
            order: LegendEntryOrder::RowMajor,
        }
    );
}

// PROMISE: an axes sends its entries to a panel by naming it, which replaces any legend
// inside its plot area, because the IR allows one or the other, never both.
#[test]
fn legend_in_points_the_axes_at_the_panel_instead_of_an_inside_legend() {
    let mut fig = Figure::new().tiles(1, 2);
    let key = fig.panel(0, 1).legend().id();
    let mut ax = fig.axes(0, 0);
    ax.legend(LegendLocation::SouthWest);
    let ax = ax.legend_in(key).id();
    assert_eq!(fig.ir().axes(ax).unwrap().legend, Some(Legend::Panel(key)));
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: one key for the whole figure is the common case, so `collect_all` points every
// axes the figure has at the panel in one call; it acts on the axes that exist when it is
// called, so an axes added afterwards keeps its own legend.
#[test]
fn collect_all_points_every_existing_axes_at_the_panel() {
    let mut fig = Figure::new().tiles(2, 3);
    let a = fig.axes(0, 0).id();
    let b = fig.axes(0, 1).id();
    let c = fig.axes(1, 0).legend(LegendLocation::NorthEast).id();
    let key = fig.panel(1, 2).legend().collect_all().id();
    for id in [a, b, c] {
        assert_eq!(
            fig.ir().axes(id).unwrap().legend,
            Some(Legend::Panel(key)),
            "{id}"
        );
    }
    let later = fig.axes(1, 1).id();
    assert_ne!(
        fig.ir().axes(later).unwrap().legend,
        Some(Legend::Panel(key))
    );
}

// PROMISE: the item setters build the explicit list of a key one item at a time, in the
// order the author calls them, which is the order of the key; the first call turns
// automatic entries into an explicit list, so artists the author did not name are no
// longer listed, and `entries(Auto)` restores the automatic list.
#[test]
fn legend_item_setters_build_an_explicit_list_in_call_order() {
    let x = linspace(0.0, 1.0, 5);
    let mut fig = Figure::new().tiles(2, 1);
    let mut ax = fig.axes(0, 0);
    let member = ax.plot(&x, &x).label("Member").id();
    let base = ax.plot(&x, &x).label("Base").id();
    let swatch = LegendSample::swatch(Color::rgb(0.9, 0.6, 0.0));
    let key = fig
        .panel(1, 0)
        .legend()
        .full_row(base)
        .item(member)
        .custom_item(swatch, "Upper surface")
        .id();
    assert_eq!(
        legend(&fig, key).entries,
        LegendEntries::Explicit(vec![
            LegendItem {
                full_row: true,
                ..LegendItem::artist(base)
            },
            LegendItem::artist(member),
            LegendItem::custom(swatch, "Upper surface"),
        ])
    );
    fig.panel(1, 0).legend().entries(LegendEntries::Auto);
    assert_eq!(legend(&fig, key).entries, LegendEntries::Auto);
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
        .panel(3, 0)
        .legend()
        .columns(LegendEntryColumns::Count(1))
        .id();
    for row in 0..3 {
        let mut ax = fig.axes(row, 0);
        for k in 0..12 {
            ax.plot(&x, &x).label(format!("Level {k}"));
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

// PROMISE: a builder chooses the columns of a key, or the size of its row, from whether the
// key fits; it learns that from the layout alone, without exporting a file, and the answer
// is the one an export gives. A panel created through `fig.panel` is laid out by the one
// panel layout path, so it overflows a short row, and fits a row fitted to it, whatever
// its number of entries.
#[test]
fn a_legend_panel_overflows_a_short_row_and_fits_a_fitted_one() {
    let short = stack_with_a_key(TrackSize::Weight(1.0));
    assert!(
        overflows(&short.layout_warnings()),
        "twelve entries in one column overflow a quarter of the page"
    );
    let path = std::env::temp_dir().join(format!(
        "ironlab-panel-layout-warnings-{}.pdf",
        std::process::id()
    ));
    let report = short.export_pdf(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(report.scene, short.layout_warnings());
    let fitted = stack_with_a_key(TrackSize::Fit);
    assert!(fitted.validate().is_valid(), "{:?}", fitted.validate());
    let warnings = fitted.layout_warnings();
    assert!(!overflows(&warnings), "{warnings:?}");
}

// PROMISE: the API produces figures the validator accepts, with panel identifiers drawn
// from the same allocator as data arrays, axes and artists, so that no two nodes share one;
// this is the example of the legends guide, so it is also free of warnings, every panel
// having entries to list and every array an artist.
#[test]
fn a_figure_with_weights_and_a_shared_spanning_panel_validates_cleanly() {
    let x = linspace(0.0, 1.0, 11);
    let a: Vec<f64> = x.iter().map(|v| v * v).collect();
    let mut fig = Figure::new().tiles(2, 3).col_weights([2.0, 2.0, 1.0]);
    let key = fig
        .panel_span(0, 2, 2, 1)
        .align(PanelLocation::North)
        .legend()
        .columns(LegendEntryColumns::Auto)
        .id();
    let shared = fig.data("x", &x).id();
    for col in 0..2 {
        let mut ax = fig.axes(0, col);
        ax.plot(shared, &a).label("Sensor A");
        ax.legend_in(key);
    }
    let p = panel(&fig, key);
    assert_eq!(
        p.cell,
        Cell {
            row: 0,
            col: 2,
            row_span: 2,
            col_span: 1
        }
    );
    assert_eq!(p.align, PanelLocation::North);
    let report = fig.validate();
    assert_eq!(report.errors, vec![], "{report:?}");
    assert_eq!(report.warnings, vec![], "{report:?}");
}
