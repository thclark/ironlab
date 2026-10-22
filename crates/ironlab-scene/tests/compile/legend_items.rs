//! Explicit legend entries: a legend that lists exactly the items its author names, in the author's order, with
//! items that stand for no artist and items on a row of their own.

use ironlab_ir::{
    Color, ColorSpec, DashStyle, InsideLegend, Legend, LegendEntries, LegendEntryColumns,
    LegendEntryLayout, LegendEntryOrder, LegendItem, LegendSample, LineStyle, MarkerShape,
    MarkerStyle, NodeId, PanelContent, TrackSize, View3d,
};
use ironlab_scene::display::Rect;
use ironlab_scene::hit::{LegendHit, LegendOwner};
use ironlab_scene::{Scene, SceneWarningKind};

use crate::common::{Fx, cell, compile_figure, rgb8};
use crate::probe::{
    Leaf, assert_close, axes_legend_entries, entry_artist, from_source, glyph_runs, inside, leaves,
    panel_entries, panel_rect, text_bbox,
};

const GREY: Color = Color::rgb(0.5, 0.5, 0.5);
const AMBER: Color = Color::rgb(0.9, 0.6, 0.0);
const RED: Color = Color::rgb(0.8, 0.1, 0.1);

/// Positions closer than this, in points, are taken to be the same row or column of entries.
const SAME: f64 = 0.5;

/// A 2 × 1 layout with an axes above a tall legend panel that the axes names, holding lines named by `names` and
/// drawn in that order. The panel arranges its entries in one column. Returns the figure, the axes, the lines and the
/// panel.
fn axes_over_panel(names: &[&str]) -> (Fx, NodeId, Vec<NodeId>, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    let ax = fx.axes2d(0, 0);
    let lines = names
        .iter()
        .map(|name| fx.named_line(ax, name, |_| {}))
        .collect();
    let panel = fx.panel(cell(1, 0, 1, 1));
    fx.legend_mut(panel).layout = fixed(1, LegendEntryOrder::ColumnMajor);
    fx.legend_in(ax, panel);
    (fx, ax, lines, panel)
}

/// Sets the entries of a panel to an explicit list of items.
fn explicit(fx: &mut Fx, panel: NodeId, items: Vec<LegendItem>) {
    fx.legend_mut(panel).entries = LegendEntries::Explicit(items);
}

/// An item that lists an artist on a row of its own.
fn full_row(artist: NodeId) -> LegendItem {
    LegendItem {
        full_row: true,
        ..LegendItem::artist(artist)
    }
}

/// Returns the artists the entries of a panel stand for, one list per entry, in the order the entries are listed.
fn targets(scene: &Scene, panel: NodeId) -> Vec<Vec<NodeId>> {
    panel_entries(scene, panel)
        .into_iter()
        .map(|e| e.targets.into_iter().map(|(_, artist)| artist).collect())
        .collect()
}

/// Returns the bounding box of the label of `text` drawn by a legend.
#[track_caller]
fn label(scene: &Scene, text: &str) -> Rect {
    text_bbox(&leaves(scene), text).unwrap_or_else(|| panic!("no label {text:?} is drawn"))
}

/// Returns the items a legend draws for the sample beside the label `text`: the items of `source` that end to the
/// left of the label, start no further left than one sample width from it, and whose vertical centre lies within the
/// height of the label.
fn sample_beside(scene: &Scene, source: NodeId, text: &str) -> Vec<Leaf> {
    let label = label(scene, text);
    from_source(&leaves(scene), source)
        .into_iter()
        .filter(|leaf| leaf.glyphs().is_none())
        .filter(|leaf| {
            leaf.visible_bbox().is_some_and(|b| {
                let cy = b.y + b.height / 2.0;
                b.right() <= label.x + 1e-6
                    && b.x >= label.x - 4.0 * label.height.max(9.0)
                    && cy >= label.y
                    && cy <= label.bottom()
            })
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

/// Returns the distinct left edges of the given rectangles, from the left.
fn lefts(rects: &[Rect]) -> Vec<f64> {
    let mut xs: Vec<f64> = Vec::new();
    for r in rects {
        if !xs.iter().any(|x| (x - r.x).abs() < SAME) {
            xs.push(r.x);
        }
    }
    xs.sort_by(f64::total_cmp);
    xs
}

fn fixed(columns: u32, order: LegendEntryOrder) -> LegendEntryLayout {
    LegendEntryLayout {
        columns: LegendEntryColumns::Count(columns),
        order,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Which items, in which order
// ---------------------------------------------------------------------------------------------------------------

// Why: the reference of a reference-and-variants figure is drawn last, so that the family never hides it, but a
// reader expects it first in the key. The order of an explicit list must therefore decide the order of the key on
// its own, and must not change the order in which the artists are painted.
#[test]
fn an_explicit_list_orders_the_key_without_changing_the_order_of_painting() {
    let (mut fx, ax, lines, panel) = axes_over_panel(&["Small", "Large", "Reference"]);
    let [small, large, reference] = lines[..] else {
        unreachable!()
    };
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::artist(reference),
            LegendItem::artist(small),
            LegendItem::artist(large),
        ],
    );
    let scene = compile_figure(&fx.build());
    assert_eq!(
        targets(&scene, panel),
        vec![vec![reference], vec![small], vec![large]]
    );
    let painted: Vec<NodeId> = leaves(&scene)
        .iter()
        .filter_map(|leaf| leaf.source)
        .filter(|source| lines.contains(source))
        .fold(Vec::new(), |mut order, source| {
            if order.last() != Some(&source) {
                order.push(source);
            }
            order
        });
    assert_eq!(
        painted,
        vec![small, large, reference],
        "the artists of axes {ax} are painted in the order they are drawn"
    );
    assert!(
        label(&scene, "Reference").y < label(&scene, "Small").y,
        "the reference is listed above the first member"
    );
}

// Why: an explicit list is exactly what the author asked for; a named artist that the author left out of it must
// not reappear, or the author could not keep a helper series (a fit, a construction line) out of the key.
#[test]
fn an_explicit_list_lists_only_its_items() {
    let (mut fx, _, lines, panel) = axes_over_panel(&["A", "B", "C"]);
    explicit(&mut fx, panel, vec![LegendItem::artist(lines[1])]);
    let scene = compile_figure(&fx.build());
    assert_eq!(targets(&scene, panel), vec![vec![lines[1]]]);
    let leaves = leaves(&scene);
    assert!(
        text_bbox(&leaves, "B").is_some(),
        "the listed item is drawn"
    );
    assert!(text_bbox(&leaves, "A").is_none() && text_bbox(&leaves, "C").is_none());
}

// Why: an explicit list of no items lists nothing; it must not fall back to the automatic entries, and an empty key
// is not an overflow.
#[test]
fn an_empty_explicit_list_lists_nothing() {
    let (mut fx, _, _, panel) = axes_over_panel(&["A", "B"]);
    explicit(&mut fx, panel, Vec::new());
    let scene = compile_figure(&fx.build());
    assert!(panel_entries(&scene, panel).is_empty());
    let leaves = leaves(&scene);
    assert!(text_bbox(&leaves, "A").is_none() && text_bbox(&leaves, "B").is_none());
    assert!(!overflows(&scene, LegendOwner::Panel(panel)));
}

// Why: a stack of axes that all draw the same base case names one of those artists in the key; clicking the item
// must hide or show the base case in every axes of the stack, as an automatic entry of the same name and look
// would, so an artist item stands for the artists of the axes that name the panel which match it in name and look.
#[test]
fn an_artist_item_stands_for_the_matching_artists_of_every_axes_that_names_the_panel() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 3;
    let top = fx.axes2d(0, 0);
    let bottom = fx.axes2d(1, 0);
    let panel = fx.panel(cell(2, 0, 1, 1));
    let red = |l: &mut ironlab_ir::Series| l.line.color = ColorSpec::Rgba { color: RED };
    let base_top = fx.named_line(top, "Base", red);
    let base_bottom = fx.named_line(bottom, "Base", red);
    // The same name in a different look is another series, which the item does not stand for.
    fx.named_line(bottom, "Base", |l| {
        l.line.color = ColorSpec::Rgba { color: AMBER }
    });
    fx.legend_in(top, panel);
    fx.legend_in(bottom, panel);
    explicit(&mut fx, panel, vec![LegendItem::artist(base_bottom)]);
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 1);
    let mut stands_for = entries[0].targets.clone();
    stands_for.sort();
    let mut expected = vec![(top, base_top), (bottom, base_bottom)];
    expected.sort();
    assert_eq!(stands_for, expected);
}

// Why: a second key may list an artist of an axes that sends its own entries elsewhere, or nowhere; the item must
// be listed and must toggle that artist, which belongs to no axes that names the panel.
#[test]
fn an_artist_item_may_name_an_artist_of_an_axes_that_does_not_name_the_panel() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    let ax = fx.axes2d(0, 0);
    let line = fx.named_line(ax, "Measured", |_| {});
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend::default()));
    let panel = fx.panel(cell(1, 0, 1, 1));
    explicit(&mut fx, panel, vec![LegendItem::artist(line)]);
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].targets, vec![(ax, line)]);
    assert_eq!(
        axes_legend_entries(&scene, ax).len(),
        1,
        "the axes still lists the artist in its own legend"
    );
}

// Why: the artist an item names need not belong to an axes that names the panel, but the series it draws may also be
// drawn in axes that do; the item then stands for all of them, so that one click acts on the whole series.
#[test]
fn an_item_naming_an_artist_elsewhere_also_stands_for_its_twins_in_the_axes_that_name_the_panel() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 3;
    let elsewhere = fx.axes2d(0, 0);
    let naming = fx.axes2d(1, 0);
    let panel = fx.panel(cell(2, 0, 1, 1));
    let named = fx.named_line(elsewhere, "Base", |_| {});
    let twin = fx.named_line(naming, "Base", |_| {});
    fx.legend_in(naming, panel);
    explicit(&mut fx, panel, vec![LegendItem::artist(named)]);
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].targets, vec![(elsewhere, named), (naming, twin)]);
}

// Why: compilation never fails; an item naming an artist the figure does not have is a validation error, but the
// viewer must still draw the rest of the key while the author fixes it.
#[test]
fn an_item_naming_no_artist_is_left_out_and_the_others_are_listed() {
    let (mut fx, _, lines, panel) = axes_over_panel(&["A", "B"]);
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::artist(NodeId(9_999)),
            LegendItem::artist(lines[1]),
            LegendItem::artist(lines[0]),
        ],
    );
    let scene = compile_figure(&fx.build());
    assert_eq!(targets(&scene, panel), vec![vec![lines[1]], vec![lines[0]]]);
}

// Why: the same reorderable key is wanted inside a plot area, where the reference series is again drawn last and
// listed first.
#[test]
fn an_inside_legend_lists_its_explicit_items_in_their_order() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let member = fx.named_line(ax, "Member", |_| {});
    let reference = fx.named_line(ax, "Reference", |_| {});
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        entries: LegendEntries::Explicit(vec![
            LegendItem::artist(reference),
            LegendItem::artist(member),
        ]),
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    let order: Vec<NodeId> = axes_legend_entries(&scene, ax)
        .iter()
        .map(entry_artist)
        .collect();
    assert_eq!(order, vec![reference, member]);
}

// Why: a legend inside a plot area describes its own axes; an item naming an artist of another axes is a validation
// error, and the compiler leaves it out rather than let one plot's key toggle a series of another plot.
#[test]
fn an_inside_legend_leaves_out_an_item_naming_an_artist_of_another_axes() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let ax = fx.axes2d(0, 0);
    let other = fx.axes2d(0, 1);
    let own = fx.named_line(ax, "Own", |_| {});
    let foreign = fx.named_line(other, "Foreign", |_| {});
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        entries: LegendEntries::Explicit(vec![
            LegendItem::artist(foreign),
            LegendItem::artist(own),
        ]),
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    let order: Vec<NodeId> = axes_legend_entries(&scene, ax)
        .iter()
        .map(entry_artist)
        .collect();
    assert_eq!(order, vec![own]);
}

// Why: a three-dimensional axes draws its inside legend through the same path, and must honour the explicit list
// there too, custom items included.
#[test]
fn a_three_dimensional_axes_lists_its_explicit_items() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let first = fx.named_line(ax, "First", |_| {});
    let second = fx.named_line(ax, "Second", |_| {});
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        entries: LegendEntries::Explicit(vec![
            LegendItem::artist(second),
            LegendItem::custom(LegendSample::swatch(AMBER), "Swatch"),
            LegendItem::artist(first),
        ]),
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    let order: Vec<NodeId> = axes_legend_entries(&scene, ax)
        .iter()
        .map(entry_artist)
        .collect();
    assert_eq!(order, vec![second, first]);
    let swatch = label(&scene, "Swatch");
    assert!(label(&scene, "Second").y < swatch.y && swatch.y < label(&scene, "First").y);
    assert!(!sample_beside(&scene, ax, "Swatch").is_empty());
}

// ---------------------------------------------------------------------------------------------------------------
// Items that stand for no artist
// ---------------------------------------------------------------------------------------------------------------

/// A panel that no axes names, listing an artist, a grey square marker and an amber swatch in one column, beneath an
/// axes of one line. Returns the figure, the axes, its line and the panel.
fn marker_key() -> (Fx, NodeId, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    let ax = fx.axes2d(0, 0);
    let curve = fx.named_line(ax, "Curve", |_| {});
    let panel = fx.panel(cell(1, 0, 1, 1));
    fx.legend_mut(panel).layout = fixed(1, LegendEntryOrder::ColumnMajor);
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::artist(curve),
            LegendItem::custom(
                LegendSample::marker(MarkerShape::Square, GREY),
                "Transition",
            ),
            LegendItem::custom(LegendSample::swatch(AMBER), "Upper surface"),
        ],
    );
    (fx, ax, curve, panel)
}

// Why: a key that explains an encoding (a marker shape for a kind of point, a colour for a side) is drawn from
// items that stand for no artist; it must show each label beside a sample drawn as the author described it, even
// in a panel that no axes names.
#[test]
fn custom_items_draw_their_labels_beside_their_samples() {
    let (fx, _, _, panel) = marker_key();
    let scene = compile_figure(&fx.build());

    let square = sample_beside(&scene, panel, "Transition");
    let markers: Vec<_> = square.iter().flat_map(|leaf| leaf.instances()).collect();
    assert_eq!(markers.len(), 1, "one marker is the sample: {square:?}");
    assert!(
        square.iter().all(|leaf| leaf.markers().is_some()),
        "a dash of none draws no line beside the marker: {square:?}"
    );
    let colours: Vec<_> = square.iter().flat_map(Leaf::colors).collect();
    assert!(!colours.is_empty());
    for colour in colours {
        assert_eq!(rgb8(colour), [128, 128, 128], "the marker is grey");
    }

    // The look of a swatch (an outlined square with a margin) is pinned in `legend_color_keys.rs`; here it is enough
    // that the sample beside the label is filled with the swatch's colour.
    let swatch = sample_beside(&scene, panel, "Upper surface");
    assert!(
        swatch
            .iter()
            .filter_map(|leaf| leaf.path()?.fill)
            .any(|fill| rgb8(fill.color) == [230, 153, 0]),
        "the patch is filled with the colour of the swatch: {swatch:?}"
    );
}

// Why: clicking a legend item hides or shows the artists it stands for; an item that stands for no artist has
// nothing to toggle, so it must not be a click target that does nothing, while an artist item beside it still is.
#[test]
fn custom_items_are_not_click_targets() {
    let (fx, ax, curve, panel) = marker_key();
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].targets, vec![(ax, curve)]);
}

// Why: a hidden artist's item stays in the key, greyed and clickable, so that the reader can show it again, exactly
// as an automatic entry does; an item that stands for no artist is never hidden, so it is never greyed.
#[test]
fn a_hidden_artists_item_is_greyed_and_a_custom_item_is_not() {
    let (mut fx, ax, lines, panel) = axes_over_panel(&["Hidden"]);
    fx.fig.axes[0].artists[0].set_visible(false);
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::artist(lines[0]),
            LegendItem::custom(LegendSample::swatch(AMBER), "Swatch"),
        ],
    );
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].targets, vec![(ax, lines[0])]);
    let alphas = |text: &str| -> Vec<f32> {
        sample_beside(&scene, panel, text)
            .iter()
            .flat_map(Leaf::colors)
            .map(|c| c.a)
            .collect()
    };
    let hidden = alphas("Hidden");
    assert!(
        !hidden.is_empty() && hidden.iter().all(|a| *a < 1.0),
        "{hidden:?}"
    );
    let custom = alphas("Swatch");
    assert!(
        !custom.is_empty() && custom.iter().all(|a| *a == 1.0),
        "{custom:?}"
    );
}

// Why: an item's sample is drawn as the sample of a line artist is drawn, so a line sample with an automatic colour
// needs a colour that no axes supplies; the ink of the label beside it keeps the sample legible on any background
// the figure allows and reads as part of the same item.
#[test]
fn an_automatic_colour_in_a_custom_line_sample_is_the_ink_of_its_label() {
    let (mut fx, _, _, panel) = axes_over_panel(&["Unused"]);
    explicit(
        &mut fx,
        panel,
        vec![LegendItem::custom(
            LegendSample::Line {
                line: LineStyle::default(),
                marker: MarkerStyle::default(),
            },
            "Ink",
        )],
    );
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let ink = glyph_runs(&leaves)
        .into_iter()
        .find(|(_, run)| run.text == "Ink")
        .map(|(leaf, _)| leaf.colors()[0])
        .expect("the label is drawn");
    let sample = sample_beside(&scene, panel, "Ink");
    assert!(!sample.is_empty());
    for colour in sample.iter().flat_map(Leaf::colors) {
        assert_eq!(rgb8(colour), rgb8(ink));
    }
}

// Why: a vertical or horizontal tick marks where a series is not finite, or a reference value, and a key must be able
// to explain it; such a marker is open, so its sample is stroked in its colour rather than filled, whether it is an
// item that stands for no plot or the marker of a plot that the key lists.
#[test]
fn vertical_and_horizontal_tick_markers_are_drawn_in_samples() {
    let (mut fx, ax, _, panel) = axes_over_panel(&["Unused"]);
    let ticked = fx.named_line(ax, "Ticked", |l| {
        l.marker.shape = MarkerShape::HLine;
        l.line.color = ColorSpec::Rgba { color: RED };
    });
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::custom(
                LegendSample::marker(MarkerShape::VLine, AMBER),
                "Not finite",
            ),
            LegendItem::artist(ticked),
        ],
    );
    let scene = compile_figure(&fx.build());
    for (text, colour) in [("Not finite", [230, 153, 0]), ("Ticked", [204, 26, 26])] {
        let sample = sample_beside(&scene, panel, text);
        let markers: Vec<_> = sample.iter().flat_map(Leaf::instances).collect();
        assert_eq!(markers.len(), 1, "{text}: one marker in {sample:?}");
        assert!(
            markers[0].face.is_none(),
            "{text}: an open marker has no face"
        );
        let edge = markers[0].edge.expect("the tick is stroked");
        assert_eq!(rgb8(edge), colour, "{text}");
    }
}

// Why: a line sample is drawn as the author styled it; a sample of the default marker shape, none, draws the line
// alone, in its colour and dash.
#[test]
fn a_custom_line_sample_without_a_marker_draws_its_line_as_styled() {
    let (mut fx, _, _, panel) = axes_over_panel(&["Unused"]);
    explicit(
        &mut fx,
        panel,
        vec![LegendItem::custom(
            LegendSample::Line {
                line: LineStyle {
                    color: ColorSpec::Rgba { color: RED },
                    width_pt: 1.0,
                    dash: DashStyle::Dashed,
                },
                marker: MarkerStyle::default(),
            },
            "Dashed",
        )],
    );
    let scene = compile_figure(&fx.build());
    let sample = sample_beside(&scene, panel, "Dashed");
    assert!(
        sample.iter().all(|leaf| leaf.markers().is_none()),
        "no marker"
    );
    let stroked: Vec<_> = sample.iter().filter(|leaf| leaf.path().is_some()).collect();
    assert_eq!(stroked.len(), 1, "one stroked line: {sample:?}");
    let stroke = stroked[0]
        .path()
        .and_then(|p| p.stroke.clone())
        .expect("the line is stroked");
    assert!(!stroke.dash.is_empty(), "the line is dashed");
    assert_eq!(rgb8(stroke.color), [204, 26, 26], "the line is red");
}

// ---------------------------------------------------------------------------------------------------------------
// Items on a row of their own
// ---------------------------------------------------------------------------------------------------------------

// Why: the reference of a reference-and-variants key sits alone on the first row, set apart from the members
// arranged in columns beneath it, and its long label must not widen a column of members.
#[test]
fn a_full_row_item_sits_alone_above_the_columns_and_sets_no_column_width() {
    let names = [
        "Base case at the design point (default)",
        "M1",
        "M2",
        "M3",
        "M4",
        "M5",
        "M6",
    ];
    let (mut fx, _, lines, panel) = axes_over_panel(&names);
    fx.legend_mut(panel).layout = fixed(3, LegendEntryOrder::ColumnMajor);
    let mut items = vec![full_row(lines[0])];
    items.extend(lines[1..].iter().map(|&l| LegendItem::artist(l)));
    explicit(&mut fx, panel, items);
    let with_reference = compile_figure(&fx.fig.clone());

    let entries = panel_entries(&with_reference, panel);
    assert_eq!(entries.len(), 7);
    let reference = &entries[0];
    let members = &entries[1..];
    for member in members {
        assert!(
            member.rect.y >= reference.rect.bottom() - 1e-6,
            "the reference is on a row above every member"
        );
    }
    let member_rects: Vec<Rect> = members.iter().map(|e| e.rect).collect();
    assert_eq!(
        lefts(&member_rects).len(),
        3,
        "the members fill three columns"
    );
    let members_block = block(members);
    assert!(
        reference.rect.width >= members_block.width - 1e-6,
        "the reference's row spans every column"
    );

    // The same members without the reference are arranged in columns of the same widths.
    explicit(
        &mut fx,
        panel,
        lines[1..].iter().map(|&l| LegendItem::artist(l)).collect(),
    );
    let without = compile_figure(&fx.build());
    let alone: Vec<Rect> = panel_entries(&without, panel)
        .iter()
        .map(|e| e.rect)
        .collect();
    assert_eq!(alone.len(), 6);
    for (a, b) in member_rects.iter().zip(&alone) {
        assert_close(a.width, b.width, 1e-6);
    }
}

// Why: a full-row item can also head a group in the middle of a key (a second family after a first), so it is
// placed at its own position in the list: the items before it fill the columns above it, the items after it fill
// the columns below it, and the columns of both groups line up because they share their widths.
#[test]
fn a_full_row_item_in_the_middle_divides_the_columns_at_its_position() {
    let long = "D, a member with a long label";
    let (mut fx, _, lines, panel) = axes_over_panel(&["A", "B", "C", "Second family", long, "E"]);
    fx.legend_mut(panel).layout = fixed(2, LegendEntryOrder::ColumnMajor);
    let items = lines
        .iter()
        .enumerate()
        .map(|(k, &l)| {
            if k == 3 {
                full_row(l)
            } else {
                LegendItem::artist(l)
            }
        })
        .collect();
    explicit(&mut fx, panel, items);
    let scene = compile_figure(&fx.build());
    let rect = |text: &str| label(&scene, text);
    let heading = rect("Second family");
    for above in ["A", "B", "C"] {
        assert!(
            rect(above).bottom() <= heading.y + 1e-6,
            "{above} is above the heading"
        );
    }
    for below in [long, "E"] {
        assert!(
            rect(below).y >= heading.bottom() - 1e-6,
            "{below} is below the heading"
        );
    }
    // Column-major order within the group above: A and B fill the first column, C starts the second.
    assert_close(rect("A").x, rect("B").x, SAME);
    assert!(rect("B").y > rect("A").y + SAME);
    // The group below starts a new pair of columns, which line up with the columns above: the long label of D
    // widens the first column of both groups.
    assert_close(rect(long).x, rect("A").x, SAME);
    assert_close(rect("E").x, rect("C").x, SAME);
    assert!(rect("C").x > rect(long).right());
    assert_close(rect(long).y, rect("E").y, SAME);
}

// Why: a full-row item can come first and last at once, as a heading and a footnote of a key; both span every
// column and the members lie between them.
#[test]
fn full_row_items_first_and_last_enclose_the_columns() {
    let (mut fx, _, lines, panel) = axes_over_panel(&["Heading", "A", "B", "C", "D", "Footnote"]);
    fx.legend_mut(panel).layout = fixed(2, LegendEntryOrder::ColumnMajor);
    let last = lines.len() - 1;
    let items = lines
        .iter()
        .enumerate()
        .map(|(k, &l)| {
            if k == 0 || k == last {
                full_row(l)
            } else {
                LegendItem::artist(l)
            }
        })
        .collect();
    explicit(&mut fx, panel, items);
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 6);
    let members = block(&entries[1..5]);
    let (heading, footnote) = (entries[0].rect, entries[5].rect);
    assert!(heading.bottom() <= members.y + 1e-6);
    assert!(footnote.y >= members.bottom() - 1e-6);
    for full in [heading, footnote] {
        assert!(
            full.width >= members.width - 1e-6,
            "{full:?} spans {members:?}"
        );
    }
}

// Why: row-major order fills each row of a group from the left; the group after a full-row item starts again at
// the first column rather than continuing the row the group before it left unfinished.
#[test]
fn row_major_groups_restart_at_the_first_column_after_a_full_row_item() {
    let (mut fx, _, lines, panel) = axes_over_panel(&["A", "B", "C", "Heading", "D", "E"]);
    fx.legend_mut(panel).layout = fixed(2, LegendEntryOrder::RowMajor);
    let items = lines
        .iter()
        .enumerate()
        .map(|(k, &l)| {
            if k == 3 {
                full_row(l)
            } else {
                LegendItem::artist(l)
            }
        })
        .collect();
    explicit(&mut fx, panel, items);
    let scene = compile_figure(&fx.build());
    let rect = |text: &str| label(&scene, text);
    assert_close(rect("A").y, rect("B").y, SAME);
    assert!(rect("B").x > rect("A").x + SAME);
    assert_close(rect("C").x, rect("A").x, SAME);
    assert!(rect("C").y > rect("A").y + SAME);
    assert_close(rect("D").x, rect("A").x, SAME);
    assert_close(rect("E").x, rect("B").x, SAME);
    assert!(rect("D").y > rect("Heading").y + SAME);
}

// Why: automatic columns are chosen so that the block fits the height of the panel; a full-row item takes a row
// whatever the number of columns, so the search must count it. With exactly twice as many members as one column
// holds, two columns would fit the members but not the full row as well, so the key needs three.
#[test]
fn automatic_columns_count_full_rows_when_fitting_the_height() {
    let names: Vec<String> = (0..60).map(|k| format!("S{k}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let (mut fx, _, lines, panel) = axes_over_panel(&refs);
    fx.fig.layout.row_weights = vec![TrackSize::Weight(3.0), TrackSize::Weight(1.0)];
    // The number of entries one column holds is the largest number that does not overflow it.
    let fits = |n: usize| {
        let mut fx = fx.fig.clone();
        let PanelContent::Legend(legend) = &mut fx.panels[0].content else {
            panic!("the panel holds a legend");
        };
        legend.entries =
            LegendEntries::Explicit(lines[..n].iter().map(|&l| LegendItem::artist(l)).collect());
        !overflows(&compile_figure(&fx), LegendOwner::Panel(panel))
    };
    let capacity = (1..60).take_while(|&n| fits(n)).last().unwrap_or(0);
    assert!((3..30).contains(&capacity), "{capacity} entries fit");

    fx.legend_mut(panel).layout.columns = LegendEntryColumns::Auto;
    let mut items = vec![full_row(lines[0])];
    items.extend(
        lines[1..=2 * capacity]
            .iter()
            .map(|&l| LegendItem::artist(l)),
    );
    explicit(&mut fx, panel, items);
    let scene = compile_figure(&fx.build());
    assert!(
        !overflows(&scene, LegendOwner::Panel(panel)),
        "{:?}",
        scene.warnings
    );
    let area = panel_rect(&scene, panel);
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 2 * capacity + 1);
    for e in &entries {
        assert!(inside(e.rect, area, 1e-6), "{:?} in {area:?}", e.rect);
    }
    let members: Vec<Rect> = entries[1..].iter().map(|e| e.rect).collect();
    assert_eq!(lefts(&members).len(), 3);
}
