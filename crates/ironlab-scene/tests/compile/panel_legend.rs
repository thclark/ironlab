//! Legend panels and the arrangement of legend entries: which entries a panel lists, how entries of several axes
//! are merged, how entries fill columns, where the block of entries sits, and what happens when they do not fit.

use ironlab_ir::{
    Color, ColorSpec, DashStyle, InsideLegend, Legend, LegendEntryColumns, LegendEntryLayout,
    LegendEntryOrder, LegendLocation, Line, NodeId, Projection, View3d,
};
use ironlab_scene::display::Rect;
use ironlab_scene::hit::{LegendHit, LegendOwner};
use ironlab_scene::{Scene, SceneWarningKind};

use crate::common::{Fx, cell, compile_figure};
use crate::probe::{
    Leaf, assert_close, axes_hit, axes_legend_entries, centre, disjoint, from_source, glyph_runs,
    inside, leaves, outlines, panel_entries, panel_rect, union,
};

const RED: Color = Color::rgb(0.8, 0.1, 0.1);
const BLUE: Color = Color::rgb(0.1, 0.2, 0.8);

/// Positions closer than this, in points, are taken to be the same row or column of entries.
const SAME: f64 = 0.5;

/// A 2 × 2 layout with two axes in the top row and a legend panel spanning the bottom row, to which neither axes
/// points yet. Returns the figure, the axes (left, right) and the panel.
fn two_axes_and_a_panel() -> (Fx, [NodeId; 2], NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    let panel = fx.panel(cell(1, 0, 1, 2));
    (fx, [left, right], panel)
}

/// Sets the colour of a line.
fn coloured(color: Color) -> impl FnOnce(&mut Line) {
    move |l| l.line.color = ColorSpec::Rgba { color }
}

/// Returns the targets of every entry of a panel, in the order the entries are listed.
fn targets(scene: &Scene, panel: NodeId) -> Vec<Vec<(NodeId, NodeId)>> {
    panel_entries(scene, panel)
        .into_iter()
        .map(|e| e.targets)
        .collect()
}

/// Returns the name clashes reported for a legend.
fn clashes(scene: &Scene, legend: LegendOwner) -> Vec<String> {
    scene
        .warnings
        .iter()
        .filter_map(|w| match &w.kind {
            SceneWarningKind::LegendNameClash { legend: l, name } if *l == legend => {
                Some(name.clone())
            }
            _ => None,
        })
        .collect()
}

/// Returns whether an overflow is reported for a legend.
fn overflows(scene: &Scene, legend: LegendOwner) -> bool {
    scene
        .warnings
        .iter()
        .any(|w| matches!(&w.kind, SceneWarningKind::LegendOverflow { legend: l } if *l == legend))
}

/// Returns the bounding box of a set of entries.
fn block(entries: &[LegendHit]) -> Rect {
    entries
        .iter()
        .map(|e| e.rect)
        .reduce(union)
        .expect("the legend has entries")
}

/// Returns the distinct left edges of a set of entries, from the left; each is one column of entries.
fn columns(entries: &[LegendHit]) -> Vec<f64> {
    let mut xs: Vec<f64> = Vec::new();
    for e in entries {
        if !xs.iter().any(|x| (x - e.rect.x).abs() < SAME) {
            xs.push(e.rect.x);
        }
    }
    xs.sort_by(f64::total_cmp);
    xs
}

/// Returns the index of the column of `entry` among `columns`.
fn column_of(entry: &LegendHit, columns: &[f64]) -> usize {
    columns
        .iter()
        .position(|x| (x - entry.rect.x).abs() < SAME)
        .expect("the entry starts one of the columns")
}

/// Asserts that no two entries overlap.
#[track_caller]
fn assert_entries_disjoint(entries: &[LegendHit]) {
    for (i, a) in entries.iter().enumerate() {
        for b in &entries[i + 1..] {
            assert!(
                disjoint(a.rect, b.rect),
                "entries overlap: {:?} and {:?}",
                a.rect,
                b.rect
            );
        }
    }
}

/// A 2 × 1 layout with an axes of `n` named lines, labelled `S0`, `S1`, …, above a legend panel of the given
/// layout that lists them. The rows are weighted `top : 1`, so a large `top` makes the panel short.
fn panel_of(n: usize, top: f64, layout: LegendEntryLayout) -> (Fx, NodeId, Vec<NodeId>, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.row_weights = vec![top, 1.0];
    let ax = fx.axes2d(0, 0);
    let lines = (0..n)
        .map(|k| fx.named_line(ax, &format!("S{k}"), |_| {}))
        .collect();
    let panel = fx.panel(cell(1, 0, 1, 1));
    fx.panel_mut(panel).layout = layout;
    fx.legend_in(ax, panel);
    (fx, ax, lines, panel)
}

fn fixed(columns: u32, order: LegendEntryOrder) -> LegendEntryLayout {
    LegendEntryLayout {
        columns: LegendEntryColumns::Count(columns),
        order,
    }
}

const AUTO: LegendEntryLayout = LegendEntryLayout {
    columns: LegendEntryColumns::Auto,
    order: LegendEntryOrder::ColumnMajor,
};

// ---------------------------------------------------------------------------------------------------------------
// Which entries a panel lists
// ---------------------------------------------------------------------------------------------------------------

// Why: an axes points at a panel instead of showing a legend of its own, so its entries must move out of its plot
// area into the panel; an entry drawn in both places would cover data for nothing and double every click target.
#[test]
fn an_axes_pointing_at_a_panel_lists_its_entries_there_and_not_in_its_plot_area() {
    let (mut fx, [left, _], panel) = two_axes_and_a_panel();
    let first = fx.named_line(left, "First", |_| {});
    let second = fx.named_line(left, "Second", |_| {});
    fx.legend_in(left, panel);
    let scene = compile_figure(&fx.build());

    assert!(axes_legend_entries(&scene, left).is_empty());
    let plot = axes_hit(&scene, left).plot_rect;
    let leaves = leaves(&scene);
    assert!(
        glyph_runs(&leaves)
            .iter()
            .all(|(l, _)| l.bbox().is_none_or(|b| disjoint(b, plot))),
        "no text is drawn inside the plot area"
    );
    assert_eq!(
        targets(&scene, panel),
        vec![vec![(left, first)], vec![(left, second)]]
    );
}

// Why: the reason for a shared panel is that a series drawn in several subplots appears once in the key; entries
// with the same name and the same look are the same series to the reader, so they become one entry that stands
// for every artist, in reading order, so that toggling it hides the series everywhere.
#[test]
fn entries_with_the_same_name_and_look_from_two_axes_merge_into_one() {
    let (mut fx, [left, right], panel) = two_axes_and_a_panel();
    let a = fx.named_line(left, "Sensor A", coloured(RED));
    let b = fx.named_line(right, "Sensor A", coloured(RED));
    fx.legend_in(left, panel);
    fx.legend_in(right, panel);
    let scene = compile_figure(&fx.build());
    assert_eq!(targets(&scene, panel), vec![vec![(left, a), (right, b)]]);
    assert!(clashes(&scene, LegendOwner::Panel(panel)).is_empty());
}

// Why: two series that share a name but are drawn differently are different series, so merging them would make
// the key lie about one of them; both are listed, and because the reader cannot tell which name is which, the
// clash is reported by name against the panel.
#[test]
fn entries_with_the_same_name_but_a_different_look_are_both_listed_and_reported() {
    fn red(l: &mut Line) {
        l.line.color = ColorSpec::Rgba { color: RED };
    }
    fn blue(l: &mut Line) {
        l.line.color = ColorSpec::Rgba { color: BLUE };
    }
    fn red_dashed(l: &mut Line) {
        red(l);
        l.line.dash = DashStyle::Dashed;
    }
    for (case, other) in [("colour", blue as fn(&mut Line)), ("dash", red_dashed)] {
        let (mut fx, [left, right], panel) = two_axes_and_a_panel();
        let a = fx.named_line(left, "Sensor A", red);
        let b = fx.named_line(right, "Sensor A", other);
        fx.legend_in(left, panel);
        fx.legend_in(right, panel);
        let scene = compile_figure(&fx.build());
        assert_eq!(
            targets(&scene, panel),
            vec![vec![(left, a)], vec![(right, b)]],
            "{case}"
        );
        assert_eq!(
            clashes(&scene, LegendOwner::Panel(panel)),
            vec!["Sensor A".to_owned()],
            "{case}: {:?}",
            scene.warnings
        );
    }
}

// Why: a reader scans a page of subplots from the top left, so a shared key whose entries followed the order in
// which the code happened to create the axes would contradict the layout it describes. Entries are listed by the
// reading order of the axes' cells and then by the order of the artists in each axes, never sorted by name; the
// right-hand axes is created first here so that creation order and reading order disagree.
#[test]
fn entries_of_several_axes_are_listed_in_the_reading_order_of_their_cells() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    let right = fx.axes2d(0, 1);
    let left = fx.axes2d(0, 0);
    let panel = fx.panel(cell(1, 0, 1, 2));
    let zulu = fx.named_line(right, "Zulu", |_| {});
    let alpha = fx.named_line(right, "Alpha", |_| {});
    let mike = fx.named_line(left, "Mike", |_| {});
    fx.legend_in(left, panel);
    fx.legend_in(right, panel);
    let scene = compile_figure(&fx.build());
    assert_eq!(
        targets(&scene, panel),
        vec![
            vec![(left, mike)],
            vec![(right, zulu)],
            vec![(right, alpha)]
        ]
    );
}

// Why: reading order runs along a row before moving down to the next, so an axes in the first row is read before
// one in the second even when it lies further to the right; ordering by column first would read the page in
// columns, which is not how a page is read.
#[test]
fn an_axes_in_a_lower_row_is_listed_after_one_further_right_in_a_higher_row() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 3;
    fx.fig.layout.cols = 2;
    let lower_left = fx.axes2d(1, 0);
    let upper_right = fx.axes2d(0, 1);
    let panel = fx.panel(cell(2, 0, 1, 2));
    let low = fx.named_line(lower_left, "Low", |_| {});
    let high = fx.named_line(upper_right, "High", |_| {});
    fx.legend_in(lower_left, panel);
    fx.legend_in(upper_right, panel);
    let scene = compile_figure(&fx.build());
    assert_eq!(
        targets(&scene, panel),
        vec![vec![(upper_right, high)], vec![(lower_left, low)]]
    );
}

// Why: two axes whose cells start at the same tile have no reading order between them, and the order of a key must
// still be deterministic; the order in which the axes were created breaks the tie, so the listing is stable and
// predictable from the code. The axes created first spans more rows, so a rule based on the size of the cell
// would list them the other way round.
#[test]
fn axes_whose_cells_start_at_the_same_tile_are_listed_in_creation_order() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 3;
    fx.fig.layout.cols = 1;
    let tall = fx.axes_in(cell(0, 0, 2, 1), Projection::TwoD);
    let short = fx.axes2d(0, 0);
    let panel = fx.panel(cell(2, 0, 1, 1));
    let first = fx.named_line(tall, "First", |_| {});
    let second = fx.named_line(short, "Second", |_| {});
    fx.legend_in(short, panel);
    fx.legend_in(tall, panel);
    let scene = compile_figure(&fx.build());
    assert_eq!(
        targets(&scene, panel),
        vec![vec![(tall, first)], vec![(short, second)]]
    );
}

// Why: a figure can carry one panel per column or per group of plots, so each panel must list only the entries of
// the axes that point at it; a panel that gathered every entry would make several panels pointless.
#[test]
fn each_of_two_panels_lists_only_the_entries_of_the_axes_pointing_at_it() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    let under_left = fx.panel(cell(1, 0, 1, 1));
    let under_right = fx.panel(cell(1, 1, 1, 1));
    let a = fx.named_line(left, "Same name", |_| {});
    let b = fx.named_line(right, "Same name", |_| {});
    fx.legend_in(left, under_left);
    fx.legend_in(right, under_right);
    let scene = compile_figure(&fx.build());
    assert_eq!(targets(&scene, under_left), vec![vec![(left, a)]]);
    assert_eq!(targets(&scene, under_right), vec![vec![(right, b)]]);
    for panel in [under_left, under_right] {
        let area = panel_rect(&scene, panel);
        for entry in panel_entries(&scene, panel) {
            assert!(
                inside(entry.rect, area, 1e-6),
                "{:?} in {area:?}",
                entry.rect
            );
        }
    }
}

// Why: a panel with nothing to list is a valid state (a figure built before its series are named, or a panel
// reserved for axes added later), so compiling must still succeed and list no entries; the validation report, not
// the drawing, tells the user (see the IR validation tests). The author placed the panel, so a boxed panel still
// draws its border, which keeps the layout of the page as it will look once the panel has entries; an unboxed panel
// draws nothing at all.
#[test]
fn a_panel_with_nothing_to_list_draws_only_its_border() {
    for boxed in [true, false] {
        // No axes points at the panel.
        let (mut fx, [left, _], unused) = two_axes_and_a_panel();
        fx.panel_mut(unused).boxed = boxed;
        fx.named_line(left, "Named", |_| {});
        let scene = compile_figure(&fx.build());
        assert!(panel_entries(&scene, unused).is_empty());
        assert_only_border(&scene, unused, boxed);

        // An axes points at the panel, but none of its artists has a display name.
        let (mut fx, [left, _], panel) = two_axes_and_a_panel();
        fx.panel_mut(panel).boxed = boxed;
        fx.line(left, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
        fx.legend_in(left, panel);
        let scene = compile_figure(&fx.build());
        assert!(panel_entries(&scene, panel).is_empty());
        assert_only_border(&scene, panel, boxed);
    }
}

/// Asserts that a panel draws its border and nothing else when boxed, and nothing at all when not.
#[track_caller]
fn assert_only_border(scene: &Scene, panel: NodeId, boxed: bool) {
    let drawn = from_source(&leaves(scene), panel);
    if boxed {
        assert_eq!(drawn.len(), 1, "{drawn:?}");
        assert!(border(scene, panel).is_some(), "{drawn:?}");
    } else {
        assert!(drawn.is_empty(), "{drawn:?}");
    }
}

// Why: a figure read from a file may name a legend panel it does not have; validation reports that as an error,
// but the viewer still compiles the figure to show it, so compiling must not panic, and the entries of that axes
// are listed nowhere rather than falling back to an inside legend the figure did not ask for.
#[test]
fn an_axes_naming_a_missing_panel_compiles_and_lists_its_entries_nowhere() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.named_line(ax, "Named", |_| {});
    let missing = NodeId(999);
    fx.ax(ax).legend = Some(Legend::Panel(missing));
    let scene = compile_figure(&fx.build());
    assert!(axes_legend_entries(&scene, ax).is_empty());
    assert!(scene.hit_map.legend_entries.is_empty());
}

// Why: a three-dimensional axes collects its legend entries by a separate drawing path from a flat one, and a
// figure that mixes them (a surface beside its cross-sections) shares one key; the entries of a 3D axes must
// reach the panel and merge with the same series in a 2D axes.
#[test]
fn a_three_dimensional_axes_lists_its_entries_in_a_panel() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let solid = fx.axes3d(0, 1, View3d::default());
    let panel = fx.panel(cell(1, 0, 1, 2));
    let flat = fx.named_line(left, "Profile", coloured(RED));
    let raised = fx.line(solid, &[0.0, 1.0], &[0.0, 1.0], Some(&[0.0, 1.0]), |l| {
        l.display_name = Some(ironlab_ir::Text::new("Profile"));
        l.line.color = ColorSpec::Rgba { color: RED };
    });
    fx.legend_in(left, panel);
    fx.legend_in(solid, panel);
    let scene = compile_figure(&fx.build());
    assert!(axes_legend_entries(&scene, solid).is_empty());
    assert_eq!(
        targets(&scene, panel),
        vec![vec![(left, flat), (solid, raised)]]
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Columns and fill order
// ---------------------------------------------------------------------------------------------------------------

// Why: column-major order is how a reader scans a multi-column key (down, then across), so with three columns and
// seven entries the first column holds entries 0 to 2, the second 3 to 5 and the third 6, each column read from
// the top.
#[test]
fn a_fixed_column_count_fills_columns_top_to_bottom_in_column_major_order() {
    let (fx, _, _, panel) = panel_of(7, 1.0, fixed(3, LegendEntryOrder::ColumnMajor));
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 7);
    let cols = columns(&entries);
    assert_eq!(cols.len(), 3, "{entries:?}");
    let placed: Vec<usize> = entries.iter().map(|e| column_of(e, &cols)).collect();
    assert_eq!(placed, vec![0, 0, 0, 1, 1, 1, 2]);
    for run in [&entries[0..3], &entries[3..6]] {
        assert!(
            run.windows(2)
                .all(|p| p[0].rect.bottom() <= p[1].rect.y + 1e-6),
            "each column is read from the top: {run:?}"
        );
    }
    assert_close(entries[3].rect.y, entries[0].rect.y, SAME);
    assert_close(entries[6].rect.y, entries[0].rect.y, SAME);
    assert_entries_disjoint(&entries);
}

// Why: row-major order suits a short wide key read like a line of text, so with three columns the first three
// entries sit side by side in the first row, from the left, and the seventh starts the third row.
#[test]
fn a_fixed_column_count_fills_rows_left_to_right_in_row_major_order() {
    let (fx, _, _, panel) = panel_of(7, 1.0, fixed(3, LegendEntryOrder::RowMajor));
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 7);
    let cols = columns(&entries);
    assert_eq!(cols.len(), 3, "{entries:?}");
    let placed: Vec<usize> = entries.iter().map(|e| column_of(e, &cols)).collect();
    assert_eq!(placed, vec![0, 1, 2, 0, 1, 2, 0]);
    for row in entries.chunks(3) {
        for e in row {
            assert_close(e.rect.y, row[0].rect.y, SAME);
        }
    }
    assert!(entries[0].rect.bottom() <= entries[3].rect.y + 1e-6);
    assert!(entries[3].rect.bottom() <= entries[6].rect.y + 1e-6);
    assert_entries_disjoint(&entries);
}

// Why: automatic columns exist so that the user does not have to count; a panel tall enough for every entry must
// keep the single column a reader expects, and add columns only when they are needed.
#[test]
fn automatic_columns_keep_one_column_in_a_tall_panel() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let ax = fx.axes2d(0, 0);
    let panel = fx.panel(cell(0, 1, 1, 1));
    for k in 0..5 {
        fx.named_line(ax, &format!("S{k}"), |_| {});
    }
    fx.legend_in(ax, panel);
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 5);
    assert_eq!(columns(&entries).len(), 1, "{entries:?}");
}

// Why: in a short wide panel one column would overflow while most of the width stays empty, so automatic columns
// must use the fewest columns that fit: as many entries per column as one column can hold, and no more columns
// than that requires. The capacity of a column is measured, not assumed, as the largest number of entries the
// same panel keeps in one column.
#[test]
fn automatic_columns_use_the_fewest_columns_that_fit_a_short_panel() {
    const TOP: f64 = 6.0;
    let column_count = |n: usize| {
        let (fx, _, _, panel) = panel_of(n, TOP, AUTO);
        let scene = compile_figure(&fx.build());
        columns(&panel_entries(&scene, panel)).len()
    };
    let capacity = (1..=40)
        .take_while(|&n| column_count(n) == 1)
        .last()
        .expect("one entry fits the panel");
    assert!(
        capacity < 40,
        "the panel must be short enough to need several columns"
    );

    // The fill order decides which entry goes where, not how many columns are needed.
    let n = 2 * capacity + 1;
    for order in [LegendEntryOrder::ColumnMajor, LegendEntryOrder::RowMajor] {
        let layout = LegendEntryLayout { order, ..AUTO };
        let (fx, _, _, panel) = panel_of(n, TOP, layout);
        let scene = compile_figure(&fx.build());
        let entries = panel_entries(&scene, panel);
        assert_eq!(
            entries.len(),
            n,
            "{order:?}: every entry fits the panel's width"
        );
        let cols = columns(&entries);
        assert_eq!(cols.len(), n.div_ceil(capacity), "{order:?}");
        if order == LegendEntryOrder::RowMajor {
            let placed: Vec<usize> = entries.iter().map(|e| column_of(e, &cols)).collect();
            assert_eq!(
                &placed[..cols.len()],
                &(0..cols.len()).collect::<Vec<_>>()[..],
                "{order:?}: the first row fills from the left"
            );
        }
        let area = panel_rect(&scene, panel);
        for e in &entries {
            assert!(
                inside(e.rect, area, 1e-6),
                "{order:?}: {:?} in {area:?}",
                e.rect
            );
        }
        assert_entries_disjoint(&entries);
        assert!(!overflows(&scene, LegendOwner::Panel(panel)), "{order:?}");
    }
}

// Why: a fixed column count is the most the user asked for, and fewer entries than columns is ordinary (a count
// chosen for a full key, then a series removed); it must lay the entries out in one row, in order, rather than
// fail, divide by zero, or report an overflow.
#[test]
fn a_column_count_larger_than_the_number_of_entries_puts_them_in_one_row() {
    for order in [LegendEntryOrder::ColumnMajor, LegendEntryOrder::RowMajor] {
        let (fx, _, _, panel) = panel_of(3, 1.0, fixed(5, order));
        let scene = compile_figure(&fx.build());
        let entries = panel_entries(&scene, panel);
        assert_eq!(entries.len(), 3, "{order:?}");
        let cols = columns(&entries);
        let placed: Vec<usize> = entries.iter().map(|e| column_of(e, &cols)).collect();
        assert_eq!(placed, vec![0, 1, 2], "{order:?}: {entries:?}");
        for e in &entries {
            assert_close(e.rect.y, entries[0].rect.y, SAME);
        }
        assert!(!overflows(&scene, LegendOwner::Panel(panel)), "{order:?}");
    }
}

// Why: entries that do not fit even in as many columns as the width allows cannot all be shown; the panel must
// say so, because a silently truncated key misleads, and it must not spill over the plots or off the page. The panel
// is tall enough for a few rows, so that the test sees entries being drawn and kept inside it, rather than a panel
// too small to hold anything, where staying inside is trivially true.
#[test]
fn entries_that_cannot_fit_a_small_panel_are_reported_and_only_those_inside_it_are_drawn() {
    let (fx, ax, _, panel) = panel_of(200, 1.0, AUTO);
    let scene = compile_figure(&fx.build());
    assert!(
        overflows(&scene, LegendOwner::Panel(panel)),
        "{:?}",
        scene.warnings
    );
    let area = panel_rect(&scene, panel);
    let plot = axes_hit(&scene, ax).plot_rect;
    let entries = panel_entries(&scene, panel);
    assert!(
        !entries.is_empty() && entries.len() < 200,
        "the panel fits some but not all of the entries, and drew {}",
        entries.len()
    );
    assert!(
        columns(&entries).len() > 1,
        "the entries that fit fill the width: {entries:?}"
    );
    for e in &entries {
        assert!(inside(e.rect, area, 1e-6), "{:?} in {area:?}", e.rect);
    }
    assert_entries_disjoint(&entries);
    let items: Vec<Rect> = from_source(&leaves(&scene), panel)
        .iter()
        .filter_map(|leaf| leaf.visible_bbox())
        .collect();
    assert!(!items.is_empty(), "the panel draws the entries that fit");
    for b in items {
        assert!(inside(b, area, 0.5), "panel item at {b:?} leaves {area:?}");
        assert!(disjoint(b, plot), "panel item at {b:?} covers the plot");
    }
}

// Why: multi-column layout is a property of every legend, so an inside legend with two columns must place its
// entries side by side inside the plot area, filled column by column, rather than in the one tall column that
// would cover more of the data.
#[test]
fn an_inside_legend_with_two_columns_lays_its_entries_in_two_columns_in_the_plot_area() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    for k in 0..4 {
        fx.named_line(ax, &format!("S{k}"), |_| {});
    }
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        layout: fixed(2, LegendEntryOrder::ColumnMajor),
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    let entries = axes_legend_entries(&scene, ax);
    assert_eq!(entries.len(), 4);
    let cols = columns(&entries);
    assert_eq!(cols.len(), 2, "{entries:?}");
    let placed: Vec<usize> = entries.iter().map(|e| column_of(e, &cols)).collect();
    assert_eq!(placed, vec![0, 0, 1, 1]);
    let plot = axes_hit(&scene, ax).plot_rect;
    for e in &entries {
        assert!(inside(e.rect, plot, 1e-6), "{:?} in {plot:?}", e.rect);
    }
    assert_entries_disjoint(&entries);
}

// Why: automatic columns are measured against the height the legend has, which for an inside legend is the plot
// area; a short plot with many entries must spread them over columns that stay inside the plot rather than run a
// single column off its bottom edge over the axis and its labels.
#[test]
fn an_inside_legend_with_automatic_columns_fits_the_height_of_a_short_plot() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 3;
    let ax = fx.axes2d(0, 0);
    for k in 0..12 {
        fx.named_line(ax, &format!("S{k}"), |_| {});
    }
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        layout: AUTO,
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    let entries = axes_legend_entries(&scene, ax);
    assert_eq!(entries.len(), 12);
    assert!(columns(&entries).len() > 1, "{entries:?}");
    let plot = axes_hit(&scene, ax).plot_rect;
    for e in &entries {
        assert!(inside(e.rect, plot, 1e-6), "{:?} in {plot:?}", e.rect);
    }
    assert_entries_disjoint(&entries);
    assert!(
        !overflows(&scene, LegendOwner::Axes(ax)),
        "{:?}",
        scene.warnings
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Placement of the block of entries
// ---------------------------------------------------------------------------------------------------------------

/// Compiles a wide panel of three entries with the given alignment, returning the block of entries and the panel.
fn aligned(align: LegendLocation) -> (Rect, Rect, Vec<LegendHit>) {
    let (mut fx, _, _, panel) = panel_of(3, 1.0, fixed(1, LegendEntryOrder::ColumnMajor));
    fx.panel_mut(panel).align = align;
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    (block(&entries), panel_rect(&scene, panel), entries)
}

// Why: alignment lets a panel's key sit against the edge it belongs to, such as flush left under a row of plots;
// north-west must put the block at the panel's top-left corner, no further in than the legend's own padding (about
// half the font size) in an unboxed panel, and than the padding and the inset from the border (about 1.15 times the
// font size together) in a boxed one.
#[test]
fn a_north_west_panel_puts_its_entries_at_the_top_left() {
    let font = 9.0;
    for (boxed, reach) in [(false, font), (true, 1.2 * font)] {
        let (mut fx, _, _, panel) = panel_of(3, 1.0, fixed(1, LegendEntryOrder::ColumnMajor));
        fx.panel_mut(panel).align = LegendLocation::NorthWest;
        fx.panel_mut(panel).boxed = boxed;
        let scene = compile_figure(&fx.build());
        let (block, area) = (
            block(&panel_entries(&scene, panel)),
            panel_rect(&scene, panel),
        );
        assert!(
            block.x >= area.x - 1e-6 && block.x - area.x <= reach,
            "boxed {boxed}: {block:?} in {area:?}"
        );
        assert!(
            block.y >= area.y - 1e-6 && block.y - area.y <= reach,
            "boxed {boxed}: {block:?} in {area:?}"
        );
    }
}

// Why: a panel's default alignment is the centre, where a key below or beside a group of plots looks balanced;
// the centre of the block must be the centre of the panel.
#[test]
fn a_centred_panel_centres_its_entries() {
    let (block, area, _) = aligned(LegendLocation::Center);
    let (b, a) = (centre(block), centre(area));
    assert_close(b.x, a.x, 1.0);
    assert_close(b.y, a.y, 1.0);
}

// Why: `best` avoids data inside a plot area, but a panel holds no data, so it has no corner to prefer and must
// behave exactly as the centre rather than picking a corner arbitrarily.
#[test]
fn best_in_a_panel_is_the_centre() {
    let (_, _, best) = aligned(LegendLocation::Best);
    let (_, _, center) = aligned(LegendLocation::Center);
    assert_eq!(best, center);
}

// ---------------------------------------------------------------------------------------------------------------
// Hidden artists
// ---------------------------------------------------------------------------------------------------------------

// Why: a merged entry stands for a series shown in several subplots, so it is greyed only when the series is
// hidden everywhere; greying it while one of its artists is still drawn would tell the reader the series is gone
// when it is not. Visibility is not part of the look that decides merging, or a click that hid one artist before
// the other would split the entry in two under the pointer.
#[test]
fn a_merged_entry_is_greyed_only_when_every_artist_it_stands_for_is_hidden() {
    for (hide_left, hide_right, greyed) in [
        (false, false, false),
        (true, false, false),
        (true, true, true),
    ] {
        let (mut fx, [left, right], panel) = two_axes_and_a_panel();
        let a = fx.named_line(left, "Shared", |l| {
            l.line.color = ColorSpec::Rgba { color: RED };
            l.visible = !hide_left;
        });
        let b = fx.named_line(right, "Shared", |l| {
            l.line.color = ColorSpec::Rgba { color: RED };
            l.visible = !hide_right;
        });
        fx.legend_in(left, panel);
        fx.legend_in(right, panel);
        let scene = compile_figure(&fx.build());
        assert_eq!(
            targets(&scene, panel),
            vec![vec![(left, a), (right, b)]],
            "the entries merge whatever their visibility"
        );
        let entries = panel_entries(&scene, panel);
        let colours: Vec<f32> = from_source(&leaves(&scene), panel)
            .iter()
            .filter(|l| l.bbox().is_some_and(|b| inside(b, entries[0].rect, 0.5)))
            .flat_map(|l| l.colors())
            .map(|c| c.a)
            .collect();
        assert!(!colours.is_empty(), "the entry is drawn");
        if greyed {
            assert!(
                colours.iter().all(|a| *a < 1.0),
                "hidden everywhere, the entry is greyed: {colours:?}"
            );
        } else {
            assert!(
                colours.contains(&1.0),
                "hidden in {} of 2 axes, the entry is not greyed: {colours:?}",
                usize::from(hide_left) + usize::from(hide_right)
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The border of a panel
// ---------------------------------------------------------------------------------------------------------------

/// Returns the border a legend panel draws around its area, if it draws one.
fn border(scene: &Scene, panel: NodeId) -> Option<Leaf> {
    let area = panel_rect(scene, panel);
    let mut found: Vec<Leaf> = outlines(scene, panel)
        .into_iter()
        .filter(|leaf| leaf.bbox().is_some_and(|b| inside(area, b, 1e-6)))
        .collect();
    assert!(found.len() <= 1, "a panel draws one border: {found:?}");
    found.pop()
}

/// Returns the box an axes draws around its plot area.
#[track_caller]
fn axes_box(scene: &Scene, ax: NodeId) -> Leaf {
    let plot = axes_hit(scene, ax).plot_rect;
    outlines(scene, ax)
        .into_iter()
        .find(|leaf| {
            leaf.bbox()
                .is_some_and(|b| inside(b, plot, 1e-6) && inside(plot, b, 1e-6))
        })
        .expect("the axes draws a box around its plot area")
}

// Why: a panel's border must read as the same kind of frame as the boxes of the plots it sits beside; a border of
// a different weight, colour or dash would set the key apart from the plots it describes. The border is an
// outline only, as the axes box is, so that it frames the panel without painting over the figure's background.
#[test]
fn a_boxed_panel_draws_its_border_with_the_stroke_of_the_axes_box() {
    let (mut fx, [left, _], panel) = two_axes_and_a_panel();
    fx.named_line(left, "Named", |_| {});
    fx.legend_in(left, panel);
    let scene = compile_figure(&fx.build());
    let border = border(&scene, panel).expect("a panel is boxed by default");
    let frame = axes_box(&scene, left);
    let (b, f) = (border.path().unwrap(), frame.path().unwrap());
    assert_eq!(b.stroke, f.stroke);
    assert_eq!(b.fill, None);
    let area = panel_rect(&scene, panel);
    let drawn = border.bbox().unwrap();
    assert!(
        inside(drawn, area, 1e-6) && inside(area, drawn, 1e-6),
        "the border {drawn:?} outlines the panel {area:?}"
    );
}

// Why: an unboxed panel is how an author asks for a key with no frame, so nothing may outline its area, while
// its entries are still listed.
#[test]
fn an_unboxed_panel_draws_no_border() {
    let (mut fx, [left, _], panel) = two_axes_and_a_panel();
    fx.named_line(left, "Named", |_| {});
    fx.legend_in(left, panel);
    fx.panel_mut(panel).boxed = false;
    let scene = compile_figure(&fx.build());
    assert_eq!(panel_entries(&scene, panel).len(), 1);
    assert!(border(&scene, panel).is_none());
    assert!(outlines(&scene, panel).is_empty());
}

// Why: entries that touch the border look cramped and can be read as part of the frame, so at every alignment,
// the corners and edges included, everything the entries draw keeps a clear gap from the inside of the border.
#[test]
fn entries_keep_a_gap_from_the_border_at_every_alignment() {
    const GAP: f64 = 2.0;
    for align in [
        LegendLocation::NorthWest,
        LegendLocation::North,
        LegendLocation::NorthEast,
        LegendLocation::West,
        LegendLocation::Center,
        LegendLocation::East,
        LegendLocation::SouthWest,
        LegendLocation::South,
        LegendLocation::SouthEast,
        LegendLocation::Best,
    ] {
        let (mut fx, _, _, panel) = panel_of(3, 1.0, fixed(1, LegendEntryOrder::ColumnMajor));
        fx.panel_mut(panel).align = align;
        let scene = compile_figure(&fx.build());
        let border = border(&scene, panel).expect("a panel is boxed by default");
        let half = border.path().unwrap().stroke.as_ref().unwrap().width / 2.0;
        let b = border.bbox().unwrap();
        let inner = Rect::new(
            b.x + half,
            b.y + half,
            b.width - 2.0 * half,
            b.height - 2.0 * half,
        );
        let ink = from_source(&leaves(&scene), panel)
            .iter()
            .filter(|leaf| leaf.bbox() != Some(b))
            .filter_map(Leaf::visible_bbox)
            .reduce(union)
            .expect("the panel draws its entries");
        let gaps = [
            ink.x - inner.x,
            ink.y - inner.y,
            inner.right() - ink.right(),
            inner.bottom() - ink.bottom(),
        ];
        assert!(
            gaps.iter().all(|g| *g >= GAP),
            "{align:?}: the entries {ink:?} come within {gaps:?} of the border {inner:?}"
        );
    }
}
