//! Fitted rows and columns of the tile layout: a row or column that takes the size of the legend panels in it, so
//! that a key neither clips its entries nor leaves empty space, while the weighted rows and columns share the rest.

use ironlab_ir::{
    AxisLink, Dimension, LegendEntries, LegendEntryColumns, LegendEntryLayout, LegendEntryOrder,
    LegendItem, NodeId, Projection, TrackSize,
};
use ironlab_scene::display::Rect;
use ironlab_scene::hit::{LegendHit, LegendOwner};
use ironlab_scene::{Scene, SceneWarningKind};

use crate::common::{Fx, cell, compile_figure};
use crate::probe::{assert_close, axes_hit, inside, panel_entries, panel_rect};

/// The default base font size of a figure, in points.
const FONT_SIZE: f64 = 9.0;

/// Positions closer than this, in points, are taken to be the same row or column of entries.
const SAME: f64 = 0.5;

const W: TrackSize = TrackSize::Weight(1.0);

/// Returns whether an overflow is reported for a legend panel.
fn overflows(scene: &Scene, panel: NodeId) -> bool {
    scene.warnings.iter().any(|w| {
        matches!(&w.kind, SceneWarningKind::LegendOverflow { legend } if *legend == LegendOwner::Panel(panel))
    })
}

/// Returns the bounding box of a set of entries.
fn block(entries: &[LegendHit]) -> Rect {
    entries
        .iter()
        .map(|e| e.rect)
        .reduce(|a, b| {
            let (x, y) = (a.x.min(b.x), a.y.min(b.y));
            Rect::new(
                x,
                y,
                a.right().max(b.right()) - x,
                a.bottom().max(b.bottom()) - y,
            )
        })
        .expect("the legend has entries")
}

/// Returns the number of distinct values among `values`, to within [`SAME`].
fn distinct(values: impl IntoIterator<Item = f64>) -> usize {
    let mut seen: Vec<f64> = Vec::new();
    for v in values {
        if !seen.iter().any(|s| (s - v).abs() < SAME) {
            seen.push(v);
        }
    }
    seen.len()
}

/// A figure of one column: an axes in each of the first `axes_rows` rows, and a fitted last row holding a panel
/// that the axes name, listing `names`. Returns the figure, the axes, the lines and the panel.
fn stack_over_fitted_row(
    axes_rows: u32,
    names: &[String],
    layout: LegendEntryLayout,
    boxed: bool,
) -> (Fx, Vec<NodeId>, Vec<NodeId>, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = axes_rows + 1;
    fx.fig.layout.row_weights = (0..axes_rows).map(|_| W).chain([TrackSize::Fit]).collect();
    let axes: Vec<NodeId> = (0..axes_rows).map(|row| fx.axes2d(row, 0)).collect();
    let panel = fx.panel(cell(axes_rows, 0, 1, 1));
    fx.panel_mut(panel).layout = layout;
    fx.panel_mut(panel).boxed = boxed;
    let lines = names
        .iter()
        .map(|name| fx.named_line(axes[0], name, |_| {}))
        .collect();
    for &ax in &axes {
        fx.legend_in(ax, panel);
    }
    (fx, axes, lines, panel)
}

fn names(prefix: &str, n: usize) -> Vec<String> {
    (0..n).map(|k| format!("{prefix}{k}")).collect()
}

fn fixed(columns: u32) -> LegendEntryLayout {
    LegendEntryLayout {
        columns: LegendEntryColumns::Count(columns),
        order: LegendEntryOrder::ColumnMajor,
    }
}

const AUTO: LegendEntryLayout = LegendEntryLayout {
    columns: LegendEntryColumns::Auto,
    order: LegendEntryOrder::ColumnMajor,
};

// ---------------------------------------------------------------------------------------------------------------
// Fitted rows
// ---------------------------------------------------------------------------------------------------------------

// Why: a fitted row exists so that a builder never sizes a key by hand: whatever the number of entries, the row
// must hold all of them without clipping, and must grow by exactly the height that more entries take, never by a
// share of the page.
#[test]
fn a_fitted_row_grows_by_exactly_the_height_of_its_entries() {
    let measure = |n: usize| {
        let (fx, _, _, panel) = stack_over_fitted_row(2, &names("S", n), fixed(2), true);
        let scene = compile_figure(&fx.build());
        assert!(
            !overflows(&scene, panel),
            "{n} entries: {:?}",
            scene.warnings
        );
        let entries = panel_entries(&scene, panel);
        assert_eq!(entries.len(), n);
        (panel_rect(&scene, panel).height, block(&entries).height)
    };
    let (area_few, block_few) = measure(2);
    let (area_many, block_many) = measure(12);
    assert!(
        block_many > block_few + 4.0 * FONT_SIZE,
        "six rows against one"
    );
    assert_close(area_many - area_few, block_many - block_few, 1e-6);
}

// Why: the row must be no taller than its entries; an unbordered key in a fitted row is flush with the space it
// takes, so the space left above and below the entries is only the padding of the block itself.
#[test]
fn a_fitted_row_leaves_no_empty_space_around_an_unbordered_key() {
    let (fx, _, _, panel) = stack_over_fitted_row(2, &names("S", 6), fixed(3), false);
    let scene = compile_figure(&fx.build());
    let area = panel_rect(&scene, panel);
    let entries = block(&panel_entries(&scene, panel));
    assert!(inside(entries, area, 1e-6));
    assert!(
        area.height - entries.height < FONT_SIZE,
        "slack of {} pt around {} pt of entries",
        area.height - entries.height,
        entries.height
    );
}

// Why: the weighted rows share the height that the fitted rows leave in proportion to their weights, exactly as
// they share the whole height when no row is fitted.
#[test]
fn weighted_rows_share_the_height_that_a_fitted_row_leaves() {
    let (mut fx, axes, _, panel) = stack_over_fitted_row(2, &names("S", 4), fixed(4), true);
    let even = compile_figure(&fx.fig.clone());
    fx.fig.layout.row_weights = vec![
        TrackSize::Weight(1.0),
        TrackSize::Weight(3.0),
        TrackSize::Fit,
    ];
    let scene = compile_figure(&fx.build());
    let top = axes_hit(&scene, axes[0]).plot_rect;
    let bottom = axes_hit(&scene, axes[1]).plot_rect;
    // Both axes reserve the same margins, so a row's height is its plot's height plus the gap between the plots.
    let gap = bottom.y - top.bottom();
    assert_close(bottom.height + gap, 3.0 * (top.height + gap), 1e-6);
    // The fitted row keeps the height of its content whatever the weights of the others.
    assert_close(
        panel_rect(&scene, panel).height,
        panel_rect(&even, panel).height,
        1e-6,
    );
}

// Why: a fitted row that holds nothing has no content, so it takes no height and leaves the figure as it would be
// without the row.
#[test]
fn a_fitted_row_that_holds_no_panel_takes_no_height() {
    let mut with = Fx::new();
    with.fig.layout.rows = 3;
    with.fig.layout.row_weights = vec![W, W, TrackSize::Fit];
    let a = with.axes2d(0, 0);
    let b = with.axes2d(1, 0);
    let mut without = Fx::new();
    without.fig.layout.rows = 2;
    without.axes2d(0, 0);
    without.axes2d(1, 0);
    let with = compile_figure(&with.build());
    let without = compile_figure(&without.build());
    for id in [a, b] {
        let (p, q) = (
            axes_hit(&with, id).plot_rect,
            axes_hit(&without, id).plot_rect,
        );
        assert_close(p.y, q.y, 1e-6);
        assert_close(p.height, q.height, 1e-6);
    }
}

// Why: with automatic columns in a fitted row the height is free and the width is not, so the key takes as few rows
// as its width allows, and of the column counts that give that many rows the fewest, so that the columns are as
// evenly filled as they can be.
#[test]
fn automatic_columns_in_a_fitted_row_take_the_fewest_rows_the_width_allows() {
    // Short labels: all fit side by side, on one row.
    let (fx, _, _, panel) = stack_over_fitted_row(1, &names("S", 6), AUTO, true);
    let scene = compile_figure(&fx.build());
    assert!(!overflows(&scene, panel));
    let entries = panel_entries(&scene, panel);
    assert_eq!(distinct(entries.iter().map(|e| e.rect.y)), 1, "{entries:?}");

    // Long labels: only some fit across, so the key takes several rows, in the fewest columns that give that number
    // of rows.
    let long = names("A rather long label for perturbation level ", 12);
    let (fx, _, _, panel) = stack_over_fitted_row(1, &long, AUTO, true);
    let scene = compile_figure(&fx.build());
    assert!(!overflows(&scene, panel), "{:?}", scene.warnings);
    let entries = panel_entries(&scene, panel);
    let rows = distinct(entries.iter().map(|e| e.rect.y));
    let columns = distinct(entries.iter().map(|e| e.rect.x));
    assert!(rows > 1 && columns > 1, "{rows} rows of {columns} columns");
    assert_eq!(
        columns,
        12_usize.div_ceil(rows),
        "{rows} rows need no more columns"
    );
    // One row fewer would need more columns than the width holds.
    let (mut fx, _, _, panel) = stack_over_fitted_row(1, &long, AUTO, true);
    fx.panel_mut(panel).layout = fixed(12_usize.div_ceil(rows - 1) as u32);
    let fewer_rows = compile_figure(&fx.build());
    assert!(
        overflows(&fewer_rows, panel),
        "{} rows would fit the width",
        rows - 1
    );
}

// Why: issue #77's sheet: a stack of rows sharing x, with a key beneath it that lists the base case on a row of
// its own and the perturbation levels in columns. The fitted row must hold the whole key without a warning, so
// that the builder no longer probes for overflows by exporting.
#[test]
fn a_key_beneath_a_stack_of_linked_axes_fits_its_fitted_row() {
    let mut level_names = vec!["Base case (default inputs)".to_owned()];
    level_names.extend((1..=8).map(|k| format!("Perturbation {k} %")));
    let (mut fx, axes, lines, panel) = stack_over_fitted_row(4, &level_names, AUTO, true);
    fx.fig.links = vec![AxisLink {
        dimension: Dimension::X,
        axes: axes.clone(),
    }];
    let mut items = vec![LegendItem {
        full_row: true,
        ..LegendItem::artist(lines[0])
    }];
    items.extend(lines[1..].iter().map(|&l| LegendItem::artist(l)));
    fx.panel_mut(panel).entries = LegendEntries::Explicit(items);
    let scene = compile_figure(&fx.build());
    assert!(!overflows(&scene, panel), "{:?}", scene.warnings);
    let area = panel_rect(&scene, panel);
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 9);
    for e in &entries {
        assert!(inside(e.rect, area, 1e-6), "{:?} in {area:?}", e.rect);
    }
    assert!(
        distinct(entries[1..].iter().map(|e| e.rect.x)) > 1,
        "the levels take several columns"
    );
}

// Why: an axes may span a fitted row and a weighted one, as a tall plot beside a key does; the key in the fitted
// row then lines up with the plot area beside it, as a panel always does, and still holds its entries.
#[test]
fn a_panel_in_a_fitted_row_beside_a_tall_axes_lines_up_with_its_plot_and_fits() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    fx.fig.layout.row_weights = vec![W, TrackSize::Fit];
    let ax = fx.axes_in(cell(0, 0, 2, 1), Projection::TwoD);
    for name in names("S", 4) {
        fx.named_line(ax, &name, |_| {});
    }
    let panel = fx.panel(cell(1, 1, 1, 1));
    fx.panel_mut(panel).layout = fixed(1);
    fx.legend_in(ax, panel);
    let scene = compile_figure(&fx.build());
    assert!(!overflows(&scene, panel), "{:?}", scene.warnings);
    let area = panel_rect(&scene, panel);
    let plot = axes_hit(&scene, ax).plot_rect;
    assert_close(area.bottom(), plot.bottom(), 1e-6);
    assert_eq!(panel_entries(&scene, panel).len(), 4);
}

// Why: two keys side by side in one fitted row (a curve key and a marker key) share the row, which is as tall as the
// taller of them; neither clips its entries.
#[test]
fn a_fitted_row_holding_two_panels_is_as_tall_as_the_taller() {
    let measure = |left: usize, right: usize| {
        let mut fx = Fx::new();
        fx.fig.layout.rows = 2;
        fx.fig.layout.cols = 2;
        fx.fig.layout.row_weights = vec![W, TrackSize::Fit];
        let ax = fx.axes_in(cell(0, 0, 1, 2), Projection::TwoD);
        let a = fx.panel(cell(1, 0, 1, 1));
        let b = fx.panel(cell(1, 1, 1, 1));
        for (panel, n, prefix) in [(a, left, "L"), (b, right, "R")] {
            fx.panel_mut(panel).layout = fixed(1);
            let items = names(prefix, n)
                .iter()
                .map(|name| LegendItem::artist(fx.named_line(ax, name, |_| {})))
                .collect();
            fx.panel_mut(panel).entries = LegendEntries::Explicit(items);
        }
        let scene = compile_figure(&fx.build());
        assert!(
            !overflows(&scene, a) && !overflows(&scene, b),
            "{:?}",
            scene.warnings
        );
        let height = panel_rect(&scene, a).height;
        assert_close(panel_rect(&scene, b).height, height, 1e-6);
        height
    };
    let tall_left = measure(5, 2);
    let tall_right = measure(2, 5);
    let short = measure(2, 2);
    assert_close(tall_left, tall_right, 1e-6);
    assert!(tall_left > short + 2.0 * FONT_SIZE);
}

// ---------------------------------------------------------------------------------------------------------------
// Fitted columns
// ---------------------------------------------------------------------------------------------------------------

// Why: a key beside the plots in a fitted column is as wide as its widest entry needs, so a longer label widens the
// column by exactly its extra width and takes that width from the plots.
#[test]
fn a_fitted_column_grows_by_exactly_the_width_of_its_entries() {
    let measure = |label: &str| {
        let mut fx = Fx::new();
        fx.fig.layout.cols = 2;
        fx.fig.layout.col_weights = vec![W, TrackSize::Fit];
        let ax = fx.axes2d(0, 0);
        fx.named_line(ax, label, |_| {});
        fx.named_line(ax, "B", |_| {});
        let panel = fx.panel(cell(0, 1, 1, 1));
        fx.legend_in(ax, panel);
        let scene = compile_figure(&fx.build());
        assert!(!overflows(&scene, panel), "{:?}", scene.warnings);
        let entries = panel_entries(&scene, panel);
        (
            panel_rect(&scene, panel).width,
            block(&entries).width,
            axes_hit(&scene, ax).plot_rect.width,
        )
    };
    let (area_short, block_short, plot_short) = measure("A");
    let (area_long, block_long, plot_long) = measure("A much longer label");
    assert!(block_long > block_short + FONT_SIZE);
    assert_close(area_long - area_short, block_long - block_short, 1e-6);
    assert_close(plot_short - plot_long, area_long - area_short, 1e-6);
}
