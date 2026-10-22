//! The layout of colourbars, colour keys and panels: where the title of a scale is drawn beside a bar, how long a
//! panel colourbar is along its span and where along the span it sits, how thick every bar is, how many entry rows a
//! bar item of a legend fills and where its out-of-range keys go, and where the title of a panel is drawn. These
//! tests own what the compiler draws for those choices; the model of the choices belongs to
//! `crates/ironlab-ir/tests/colorbar_layout.rs`, whose header sketches the IR assumed here (`ColorbarTitlePlacement`,
//! `PanelColorbar::{length, title_placement}`, `InsideColorbar::title_placement`, `Panel::{title, title_align}`,
//! `LegendItemContent::ColorScale { scale, rows }` and `LegendItem::with_rows`).
//!
//! Positions are asserted as relationships (a title centred on its bar, a gap equal to another gap, a track that
//! grows by the extent of what was added, a row count read from the spacing of the entries around an item) rather
//! than as numbers of points. Text is measured by its line box: the advance width of its layout along the baseline,
//! and its ascent and descent across it, mapped through the transform of its glyph run, which is the box the
//! compiler lays text out by; the ink of a glyph run, which `Leaf::bbox` gives, differs from it by the side bearings
//! and the shapes of the glyphs.
//!
//! The scene API these tests assume is that of `colorbars.rs` and `legend_color_keys.rs`, with these additions:
//!
//! ```text
//! // Every bar (a panel bar, an inside bar and a bar item) is 0.75 × the base font size thick.
//! //
//! // A bar item records its out-of-range keys in its `ColorbarHit::keys`, in the order a vertical colourbar lists
//! // them (above, below or outside, non-finite), each with the rectangle of the swatch of its entry row, so that the
//! // viewer can still report the key under the pointer. Its key rows record no `LegendHit`, because they stand for no
//! // artist, and they are never merged with other entries, because they belong to their bar.
//! //
//! // The title of a panel is drawn by the panel: its glyph runs name the panel as their source.
//! ```
//!
//! The rules these tests pin, beyond those that the decisions state:
//!
//! - **Along.** A scale's title placed `Along` lies beyond the band of the bar's tick labels, on their side of the
//!   bar, one gap from the band, centred on the bar's length. On a standing bar it is rotated a quarter turn
//!   counter-clockwise, so that it reads from bottom to top; on a lying bar it is upright. The gap is the one that
//!   separates a title placed `Above` from the bar it heads, so the two placements are spaced alike. A fitted track
//!   grows across the bar by the title's line box and that gap, and a title along a standing panel bar takes nothing
//!   from the margin above the plots. Inside an axes the tick labels face the interior of the plot area, so the
//!   title lies beyond them toward the interior.
//! - **Above.** A scale's title placed `Above` is an upright heading: over the top end of a standing bar and over a
//!   lying panel bar. On a lying bar inside an axes the tick labels face the interior of the plot area (upward from
//!   a bar along the south edge, downward from one along the north edge), so the heading lies on the other side of
//!   the bar, away from them: below a south bar and above a north bar, within the box behind the bar.
//! - **Automatic length.** A standing panel bar spans the drawn plot boxes beside it, and a lying one half of that
//!   span; a stated length is that fraction of the span. The keys of a bar lie within its length. The length of a
//!   bar never changes the size of its fitted track, which its extent across the bar sets.
//! - **Placement along the span.** A shortened bar, keys included, is placed along the span by the panel's
//!   alignment: centred by the centre and by the two locations that centre it along a span of its direction, and
//!   pushed to the end that the location names otherwise.
//! - **Bar items.** A bar item of `rows` rows lies in exactly `rows` entry rows; its keys are entry rows of their
//!   own, a swatch beside a label as every swatch entry is, directly above the bar (the above key) and below it (the
//!   below or outside key, then the non-finite key); its title reads along it beside its tick labels and adds no row.
//!   A bar item on a row of its own lies across the legend whatever its rows, titled along it below its tick labels.
//! - **Panel titles.** A panel title heads the content of its panel and is aligned with the content's left edge,
//!   centre or right edge. In a boxed legend panel it lies inside the border above the entries; above an unboxed
//!   legend or a colourbar it lies above the content. A fitted track grows to hold it, and above a standing bar that
//!   spans the plots it lies above the bar, which keeps the span of the plots.

use ironlab_ir::{
    Cell, Color, ColorScaleKey, ColorScaleKind, ColorScaleOutOfRange, ColorbarOrientation,
    ColorbarTitlePlacement, ContinuousColorScale, FigureSize, InsideColorbar, InsideColorbarSide,
    LegendEntries, LegendEntryColumns, LegendEntryLayout, LegendEntryOrder, LegendItem,
    LegendSample, Limits, NodeId, OutOfRange, Panel, PanelColorbar, PanelContent, PanelLocation,
    Text, TitleAlign, TrackSize,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{Point, Rect, Rgba};
use ironlab_scene::hit::{ColorbarHit, ColorbarOwner, LegendHit, LegendOwner};
use ironlab_scene::maths::colormap::VIRIDIS;
use ironlab_text::FontId;

use crate::common::{Fx, cell, compile_figure, engine, rgb8, xy};
use crate::probe::{
    Leaf, assert_close, axes_hit, from_source, glyph_runs, inside, leaves, outlines, panel_entries,
    panel_rect, parse_number, union,
};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const AMBER: Color = Color::rgb(0.9, 0.6, 0.0);

/// The default base font size of a figure, in points.
const FONT_SIZE: f64 = 9.0;

/// Geometry that the compiler computes exactly, such as a bar whose ends are those of a plot box.
const EXACT: f64 = 1e-6;

/// Positions of text laid out from the same measurements, which agree to rounding.
const TEXT: f64 = 0.05;

/// Positions closer than this, in points, are taken to be the same row or column of entries.
const SAME: f64 = 0.5;

/// The title of a scale in these tests, which is no number, so that it is never taken for a tick label.
const SPEED: &str = "Speed (m/s)";

// ---------------------------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------------------------

/// A continuous scale of viridis with manual limits from `min` to `max` and the defaults of its kind otherwise.
fn viridis(min: f64, max: f64) -> ColorScaleKind {
    ColorScaleKind::Continuous(ContinuousColorScale {
        limits: Limits::Manual { min, max },
        ..ContinuousColorScale::default()
    })
}

/// An out-of-range category with the given policy whose key is shown with the default label.
fn keyed(policy: OutOfRange) -> ColorScaleOutOfRange {
    ColorScaleOutOfRange {
        policy,
        show_key: true,
        label: None,
    }
}

/// Shows the above, below and non-finite keys of the continuous scale `scale`: the above key in red, the below key
/// clamped, and the non-finite key empty.
fn show_keys(fx: &mut Fx, scale: NodeId) {
    match &mut fx.scale_mut(scale).kind {
        ColorScaleKind::Continuous(c) => {
            c.above = keyed(OutOfRange::Rgba { color: RED });
            c.below = keyed(OutOfRange::Clamp);
        }
        other => panic!("the scale is continuous, found {other:?}"),
    }
    fx.scale_mut(scale).non_finite = keyed(OutOfRange::Transparent);
}

/// Adds a colourbar panel of `scale` in `cell` with the given orientation, centred, untitled, of automatic length
/// and with the scale's title along the bar, and returns its identifier.
fn colorbar_panel(
    fx: &mut Fx,
    cell: Cell,
    scale: NodeId,
    orientation: ColorbarOrientation,
) -> NodeId {
    let id = fx.node();
    fx.fig.panels.push(Panel {
        id,
        cell,
        align: PanelLocation::Center,
        title: None,
        title_align: TitleAlign::Left,
        content: PanelContent::Colorbar(PanelColorbar {
            scale,
            orientation,
            ..PanelColorbar::default()
        }),
    });
    id
}

/// Returns the colourbar of the colourbar panel with the given identifier for modification.
fn bar_mut(fx: &mut Fx, id: NodeId) -> &mut PanelColorbar {
    match &mut fx.panel_mut(id).content {
        PanelContent::Colorbar(colorbar) => colorbar,
        PanelContent::Legend(_) => panic!("panel {id} holds a legend"),
    }
}

/// A figure of `rows` 2D axes stacked in the first column, each holding an image of the values 0 to 10 through one
/// continuous scale of viridis over [0, 10], and a vertical colourbar panel of that scale spanning every row of a
/// second column of the given size. Returns the figure, the axes, the scale and the panel.
fn column_beside_stack(rows: u32, column: TrackSize) -> (Fx, Vec<NodeId>, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = rows;
    fx.fig.layout.cols = 2;
    fx.fig.layout.col_weights = vec![TrackSize::Weight(1.0), column];
    let scale = fx.color_scale(viridis(0.0, 10.0));
    let axes: Vec<NodeId> = (0..rows).map(|row| fx.axes2d(row, 0)).collect();
    for &ax in &axes {
        fx.scalar_image(
            ax,
            vec![1, 2],
            vec![0.0, 10.0],
            Some(scale),
            xy(None, None),
            |_| {},
        );
    }
    let panel = colorbar_panel(
        &mut fx,
        cell(0, 1, rows, 1),
        scale,
        ColorbarOrientation::Vertical,
    );
    (fx, axes, scale, panel)
}

/// A figure of three 2D axes in a row over a row of the given size holding a horizontal colourbar panel spanning all
/// three columns, of one continuous scale over [0, 10] that every axes' image names. Returns the figure, the axes,
/// the scale and the panel.
fn row_over_bar(row: TrackSize) -> (Fx, Vec<NodeId>, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 3;
    fx.fig.layout.row_weights = vec![TrackSize::Weight(1.0), row];
    let scale = fx.color_scale(viridis(0.0, 10.0));
    let axes: Vec<NodeId> = (0..3).map(|col| fx.axes2d(0, col)).collect();
    for &ax in &axes {
        fx.scalar_image(
            ax,
            vec![1, 2],
            vec![0.0, 10.0],
            Some(scale),
            xy(None, None),
            |_| {},
        );
    }
    let panel = colorbar_panel(
        &mut fx,
        cell(1, 0, 1, 3),
        scale,
        ColorbarOrientation::Horizontal,
    );
    (fx, axes, scale, panel)
}

/// A figure of one 2D axes holding an image through a continuous scale of viridis over [0, 10] titled [`SPEED`],
/// and an inside colourbar of that scale on the given side, with the given placement of its title. Returns the
/// figure and the axes.
fn inside_bar(side: InsideColorbarSide, placement: ColorbarTitlePlacement) -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scale = fx.color_scale(viridis(0.0, 10.0));
    fx.scale_mut(scale).title = Some(Text::plain(SPEED));
    fx.scalar_image(
        ax,
        vec![2, 2],
        vec![0.0, 4.0, 6.0, 10.0],
        Some(scale),
        xy(None, None),
        |_| {},
    );
    fx.ax(ax).colorbar = Some(InsideColorbar {
        scale,
        side,
        length: 0.8,
        title_placement: placement,
    });
    (fx, ax)
}

/// A tall page with an axes above a legend panel twice its height, which no axes names and which arranges its
/// entries in one column. Returns the figure, the axes and the panel.
fn axes_over_key() -> (Fx, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.size = FigureSize {
        width_mm: 160.0,
        height_mm: 200.0,
    };
    fx.fig.layout.rows = 2;
    fx.fig.layout.row_weights = vec![TrackSize::Weight(1.0), TrackSize::Weight(2.0)];
    let ax = fx.axes2d(0, 0);
    let panel = fx.panel(cell(1, 0, 1, 1));
    fx.legend_mut(panel).layout = one_column();
    (fx, ax, panel)
}

/// A layout of one column filled from top to bottom.
fn one_column() -> LegendEntryLayout {
    LegendEntryLayout {
        columns: LegendEntryColumns::Count(1),
        order: LegendEntryOrder::ColumnMajor,
    }
}

/// [`axes_over_key`] with three named lines, "First", "Before" and "After", and a continuous scale of viridis over
/// [0, 1] listed between the last two as a bar item of `rows` rows. The first two lines are consecutive ordinary
/// entries, so the distance between their baselines is the pitch of the legend's rows as drawn.
struct Between {
    fx: Fx,
    panel: NodeId,
    scale: NodeId,
}

fn between(rows: u32) -> Between {
    let (mut fx, ax, panel) = axes_over_key();
    let first = fx.named_line(ax, "First", |_| {});
    let before = fx.named_line(ax, "Before", |_| {});
    let after = fx.named_line(ax, "After", |_| {});
    let scale = fx.color_scale(viridis(0.0, 1.0));
    fx.legend_mut(panel).entries = LegendEntries::Explicit(vec![
        LegendItem::artist(first),
        LegendItem::artist(before),
        LegendItem::color_scale(scale).with_rows(rows),
        LegendItem::artist(after),
    ]);
    Between { fx, panel, scale }
}

/// A figure of one axes beside a fitted column holding a legend panel that lists a continuous scale of viridis over
/// [0, 10] as its only item, a bar item of five rows. Returns the figure, the scale and the panel.
fn bar_item_in_fitted_column() -> (Fx, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    fx.fig.layout.col_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit];
    fx.axes2d(0, 0);
    let scale = fx.color_scale(viridis(0.0, 10.0));
    let panel = fx.panel(cell(0, 1, 1, 1));
    fx.legend_mut(panel).entries = LegendEntries::Explicit(vec![LegendItem::color_scale(scale)]);
    (fx, scale, panel)
}

/// A 2 × 2 layout with two axes in the top row, each listing two named lines in a legend panel spanning the bottom
/// row, of the given size. Returns the figure and the panel.
fn legend_under_two_axes(row: TrackSize) -> (Fx, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    fx.fig.layout.row_weights = vec![TrackSize::Weight(1.0), row];
    let panel = fx.panel(cell(1, 0, 1, 2));
    for (col, names) in [
        (0, ["Upstream", "Downstream"]),
        (1, ["Left wall", "Right wall"]),
    ] {
        let ax = fx.axes2d(0, col);
        for name in names {
            fx.named_line(ax, name, |_| {});
        }
        fx.legend_in(ax, panel);
    }
    (fx, panel)
}

// ---------------------------------------------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------------------------------------------

/// Returns the hit geometry of the one colourbar of an owner.
#[track_caller]
fn bar_of(scene: &Scene, owner: ColorbarOwner) -> ColorbarHit {
    let found: Vec<&ColorbarHit> = scene
        .hit_map
        .colorbars
        .iter()
        .filter(|c| c.owner == owner)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "one colourbar of {owner:?}: {:?}",
        scene.hit_map.colorbars
    );
    found[0].clone()
}

/// Returns the leaves that a node drew, in paint order.
fn drawn_by(scene: &Scene, id: NodeId) -> Vec<Leaf> {
    from_source(&leaves(scene), id)
}

/// Returns the one glyph run among the leaves whose text is exactly `text`.
#[track_caller]
fn run<'a>(leaves: &'a [Leaf], text: &str) -> &'a Leaf {
    let found: Vec<&Leaf> = glyph_runs(leaves)
        .into_iter()
        .filter(|(_, g)| g.text == text)
        .map(|(l, _)| l)
        .collect();
    assert_eq!(found.len(), 1, "one run of {text:?}");
    found[0]
}

/// The line box of a glyph run in figure space, and whether the run reads upwards.
#[derive(Clone, Copy, Debug)]
struct LineBox {
    rect: Rect,
    upwards: bool,
    /// Whether the run reads from left to right, unrotated.
    upright: bool,
}

/// Returns the line box of a glyph run of plain text: from its first glyph's origin along the advance width of its
/// layout, and across from its ascent above the baseline to its descent below it, mapped into figure space.
fn line_box(leaf: &Leaf) -> LineBox {
    let glyphs = leaf.glyphs().expect("a glyph run");
    let first = &glyphs.glyphs[0];
    let layout = engine().layout(&glyphs.text, false, glyphs.size_pt);
    let (x0, x1) = (first.x, first.x + layout.width);
    let (y0, y1) = (first.y - layout.height, first.y + layout.depth);
    let corners = [
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x0, y1),
        Point::new(x1, y1),
    ]
    .map(|p| leaf.transform.apply(p));
    let rect = corners[1..]
        .iter()
        .fold(Rect::new(corners[0].x, corners[0].y, 0.0, 0.0), |r, p| {
            union(r, Rect::new(p.x, p.y, 0.0, 0.0))
        });
    let origin = leaf.transform.apply(Point::new(0.0, 0.0));
    let ahead = leaf.transform.apply(Point::new(1.0, 0.0));
    LineBox {
        rect,
        upwards: leaf.reads_upwards(),
        upright: (ahead.y - origin.y).abs() < 1e-6 && ahead.x - origin.x > 0.999,
    }
}

/// Returns the line box of the one run of `text` that `id` drew.
#[track_caller]
fn text_box(scene: &Scene, id: NodeId, text: &str) -> LineBox {
    line_box(run(&drawn_by(scene, id), text))
}

/// Returns the baseline of the one run of `text` among the leaves: the y of its first glyph's origin in figure space.
#[track_caller]
fn baseline(leaves: &[Leaf], text: &str) -> f64 {
    let leaf = run(leaves, text);
    let first = &leaf.glyphs().expect("a glyph run").glyphs[0];
    leaf.transform.apply(Point::new(first.x, first.y)).y
}

/// Returns the line boxes of the plain numeric labels among the leaves (math runs are left out), with their values.
fn numeric(leaves: &[Leaf]) -> Vec<(f64, Rect)> {
    glyph_runs(leaves)
        .into_iter()
        .filter(|(_, g)| g.font != FontId::Math)
        .filter_map(|(l, g)| Some((parse_number(&g.text)?, line_box(l).rect)))
        .collect()
}

/// Returns the line boxes of the tick labels of a bar: the numeric labels on the given side of the bar whose centre
/// lies along the bar, give or take a label, and, for a bar inside an axes, within the box behind the bar, so that the
/// tick labels of the axes, which the axes draws too, are left out.
fn tick_boxes(leaves: &[Leaf], hit: &ColorbarHit) -> Vec<Rect> {
    let b = hit.bar;
    let boxes: Vec<Rect> = numeric(leaves)
        .into_iter()
        .map(|(_, r)| r)
        .filter(|r| {
            hit.backdrop
                .is_none_or(|backdrop| inside(*r, backdrop, SAME))
        })
        .filter(|r| {
            let (cx, cy) = (r.x + r.width / 2.0, r.y + r.height / 2.0);
            match hit.orientation {
                ColorbarOrientation::Vertical => {
                    (r.x >= b.right() || r.right() <= b.x)
                        && cy >= b.y - r.height
                        && cy <= b.bottom() + r.height
                }
                ColorbarOrientation::Horizontal => {
                    (r.y >= b.bottom() || r.bottom() <= b.y)
                        && cx >= b.x - r.width
                        && cx <= b.right() + r.width
                }
            }
        })
        .collect();
    assert!(boxes.len() >= 2, "the bar is ticked: {boxes:?}");
    boxes
}

/// Returns the union of rectangles.
fn union_of(rects: impl IntoIterator<Item = Rect>) -> Rect {
    rects
        .into_iter()
        .reduce(union)
        .expect("at least one rectangle")
}

/// Returns the thickness of a bar: its extent across its length.
fn thickness(hit: &ColorbarHit) -> f64 {
    match hit.orientation {
        ColorbarOrientation::Vertical => hit.bar.width,
        ColorbarOrientation::Horizontal => hit.bar.height,
    }
}

/// Returns the rectangle of the key of the given category, if one is drawn.
fn key_rect(hit: &ColorbarHit, key: ColorScaleKey) -> Option<Rect> {
    hit.keys.iter().find(|k| k.key == key).map(|k| k.rect)
}

/// Returns the extent of a bar along its length together with its keys: the union of the bar and its key squares.
fn with_keys(hit: &ColorbarHit) -> Rect {
    hit.keys.iter().map(|k| k.rect).fold(hit.bar, union)
}

/// Returns the start and the end of a rectangle along a direction: its top and bottom when `vertical`, and its left
/// and right otherwise.
fn ends(r: Rect, vertical: bool) -> (f64, f64) {
    if vertical {
        (r.y, r.bottom())
    } else {
        (r.x, r.right())
    }
}

/// Returns the span of the drawn plot boxes of the given axes along a direction.
fn span(scene: &Scene, axes: &[NodeId], vertical: bool) -> (f64, f64) {
    let boxes = union_of(axes.iter().map(|&a| axes_hit(scene, a).plot_rect));
    ends(boxes, vertical)
}

/// Returns the centre of a rectangle.
fn centre(r: Rect) -> Point {
    Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
}

/// Returns the entry of a legend panel that stands for the artist `artist`.
#[track_caller]
fn entry_of(scene: &Scene, panel: NodeId, artist: NodeId) -> LegendHit {
    panel_entries(scene, panel)
        .into_iter()
        .find(|e| e.targets.iter().any(|t| t.1 == artist))
        .unwrap_or_else(|| panic!("an entry stands for {artist}"))
}

/// Returns whether a leaf is a path whose only subpath is an axis-aligned rectangle of positive size.
fn is_rectangle(leaf: &Leaf) -> bool {
    let subpaths = leaf.subpaths();
    let [corners] = subpaths.as_slice() else {
        return false;
    };
    corners.len() == 4
        && (0..4).all(|k| {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            (a.x - b.x).abs() < 1e-9 || (a.y - b.y).abs() < 1e-9
        })
        && leaf.bbox().is_some_and(|b| b.width > 0.0 && b.height > 0.0)
}

/// The swatch of an entry: its square, its fill and whether it is outlined.
#[derive(Debug)]
struct Swatch {
    square: Rect,
    fill: Option<Rgba>,
    outlined: bool,
}

/// Returns the swatch drawn by `source` left of the label `text`, in its row: the rectangles that end left of the
/// label, start within four label heights of it and whose centre lies within its line box, taken together.
#[track_caller]
fn swatch_left_of(scene: &Scene, source: NodeId, text: &str) -> Swatch {
    let label = text_box(scene, source, text).rect;
    let found: Vec<Leaf> = drawn_by(scene, source)
        .into_iter()
        .filter(is_rectangle)
        .filter(|l| {
            l.bbox().is_some_and(|b| {
                let cy = b.y + b.height / 2.0;
                b.right() <= label.x + EXACT
                    && b.x >= label.x - 4.0 * label.height
                    && cy >= label.y
                    && cy <= label.bottom()
            })
        })
        .collect();
    assert!(!found.is_empty(), "a swatch is drawn beside {text:?}");
    let fill = found.iter().find_map(|l| l.path()?.fill.map(|f| f.color));
    let filled = found
        .iter()
        .find(|l| l.path().is_some_and(|p| p.fill.is_some()))
        .and_then(Leaf::bbox);
    let square = filled.unwrap_or_else(|| union_of(found.iter().filter_map(Leaf::bbox)));
    Swatch {
        square,
        fill,
        outlined: found
            .iter()
            .any(|l| l.path().is_some_and(|p| p.stroke.is_some())),
    }
}

/// Returns the text of the one glyph run drawn by `id` that starts with `prefix`.
#[track_caller]
fn text_starting(scene: &Scene, id: NodeId, prefix: &str) -> String {
    let drawn = drawn_by(scene, id);
    let found: Vec<String> = glyph_runs(&drawn)
        .into_iter()
        .filter(|(_, g)| g.text.starts_with(prefix))
        .map(|(_, g)| g.text.clone())
        .collect();
    assert_eq!(found.len(), 1, "one label starting {prefix:?}: {found:?}");
    found[0].clone()
}

// ---------------------------------------------------------------------------------------------------------------
// Thickness
// ---------------------------------------------------------------------------------------------------------------

// Why: the user's review found bars as thick as the font size heavy beside their labels; every bar of a figure, in a
// panel, inside an axes or in a legend, is three quarters of the base font size thick, at any font size.
#[test]
fn every_bar_is_three_quarters_of_the_font_size_thick() {
    for font_size in [FONT_SIZE, 12.0] {
        // A standing bar beside the axes, and under them a legend listing the scale as a bar item and a lying bar.
        let (mut fx, axes, scale, panel) = column_beside_stack(1, TrackSize::Fit);
        fx.fig.font_size_pt = font_size;
        fx.fig.layout.rows = 2;
        fx.fig.layout.row_weights = vec![TrackSize::Weight(1.0), TrackSize::Weight(1.0)];
        fx.ax(axes[0]).colorbar = Some(InsideColorbar {
            scale,
            ..InsideColorbar::default()
        });
        let key = fx.panel(cell(1, 0, 1, 1));
        fx.legend_mut(key).entries = LegendEntries::Explicit(vec![LegendItem::color_scale(scale)]);
        let lying = colorbar_panel(
            &mut fx,
            cell(1, 1, 1, 1),
            scale,
            ColorbarOrientation::Horizontal,
        );
        let scene = compile_figure(&fx.build());
        for (what, owner) in [
            ("a standing panel bar", ColorbarOwner::Panel(panel)),
            ("a lying panel bar", ColorbarOwner::Panel(lying)),
            ("an inside bar", ColorbarOwner::Axes(axes[0])),
            ("a bar item", ColorbarOwner::Legend(LegendOwner::Panel(key))),
        ] {
            let thick = thickness(&bar_of(&scene, owner));
            assert!(
                (thick - 0.75 * font_size).abs() <= EXACT,
                "{what} at {font_size} pt is {thick} pt thick, not three quarters of the font size"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// A scale's title along its bar
// ---------------------------------------------------------------------------------------------------------------

// Why: the user's review settled that a scale's title reads along a standing bar, as a y label reads along its axis:
// rotated to read from bottom to top, beyond the tick labels so that it never covers them, and centred on the bar so
// that it names the whole of it. It is the default, so a bar whose author chose nothing is drawn so.
#[test]
fn along_a_standing_panel_bar_the_title_reads_upwards_beyond_its_tick_labels_centred_on_the_bar() {
    let (mut fx, _, scale, panel) = column_beside_stack(2, TrackSize::Fit);
    fx.scale_mut(scale).title = Some(Text::plain(SPEED));
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let title = text_box(&scene, panel, SPEED);
    assert!(
        title.upwards,
        "the title reads from bottom to top: {title:?}"
    );
    let band = union_of(tick_boxes(&drawn_by(&scene, panel), &hit));
    assert!(
        band.x >= hit.bar.right(),
        "the tick labels are right of the bar"
    );
    assert!(
        title.rect.x > band.right(),
        "the title lies beyond the tick labels: {title:?} beyond {band:?}"
    );
    assert_close(centre(title.rect).y, centre(hit.bar).y, TEXT);
    let area = panel_rect(&scene, panel);
    assert!(
        inside(title.rect, area, SAME),
        "{title:?} lies in the panel {area:?}"
    );
}

// Why: a title along a standing bar takes room beside the bar, not above it, so the plots keep the top margin they
// have without it and the bar keeps the span of the plots; that is what the user's review asked for, a title that no
// longer pushes the plots down.
#[test]
fn a_title_along_a_standing_bar_leaves_the_plots_where_they_were() {
    let (mut fx, axes, scale, panel) = column_beside_stack(2, TrackSize::Fit);
    let untitled = compile_figure(&fx.fig.clone());
    fx.scale_mut(scale).title = Some(Text::plain(SPEED));
    let scene = compile_figure(&fx.build());
    let top = axes_hit(&scene, axes[0]).plot_rect;
    assert_close(top.y, axes_hit(&untitled, axes[0]).plot_rect.y, EXACT);
    let (start, end) = span(&scene, &axes, true);
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel)).bar;
    assert_close(bar.y, start, EXACT);
    assert_close(bar.bottom(), end, EXACT);
}

// Why: a fitted column is sized to its content, and a title turned along the bar adds its line height across the bar,
// not its length; a long title must therefore widen the column by its line box and one gap, far less than its width,
// and stay inside the column.
#[test]
fn a_title_along_a_standing_bar_widens_its_fitted_column_by_its_line_height_and_a_gap() {
    let long = "Velocity magnitude in the wake (m/s)";
    let (mut fx, axes, scale, panel) = column_beside_stack(1, TrackSize::Fit);
    let narrow = panel_rect(&compile_figure(&fx.fig.clone()), panel).width;
    fx.scale_mut(scale).title = Some(Text::plain(long));
    let scene = compile_figure(&fx.build());
    let area = panel_rect(&scene, panel);
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let title = text_box(&scene, panel, long);
    let band = union_of(tick_boxes(&drawn_by(&scene, panel), &hit));
    let growth = area.width - narrow;
    assert_close(growth, title.rect.right() - band.right(), SAME);
    assert!(
        growth < title.rect.height / 2.0,
        "the column grows by the title's line, not by its length: {growth}"
    );
    assert!(
        title.rect.right() <= area.right() + SAME,
        "{title:?} within {area:?}"
    );
    assert!(
        title.rect.height <= axes_hit(&scene, axes[0]).plot_rect.height,
        "the fixture's title fits the bar"
    );
}

// Why: along a lying bar the title lies below the tick labels, upright and centred on the bar, the place a reader looks
// after reading the values, as an x label lies below its axis.
#[test]
fn along_a_lying_panel_bar_the_title_lies_below_its_tick_labels_centred_on_the_bar() {
    let (mut fx, _, scale, panel) = row_over_bar(TrackSize::Fit);
    fx.scale_mut(scale).title = Some(Text::plain(SPEED));
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let title = text_box(&scene, panel, SPEED);
    assert!(
        title.upright,
        "the title reads from left to right: {title:?}"
    );
    let band = union_of(tick_boxes(&drawn_by(&scene, panel), &hit));
    assert!(
        band.y >= hit.bar.bottom(),
        "the tick labels are below the bar"
    );
    assert!(
        title.rect.y > band.bottom(),
        "the title lies below the tick labels: {title:?} below {band:?}"
    );
    assert_close(centre(title.rect).x, centre(hit.bar).x, TEXT);
}

// Why: a fitted row under the plots holds the bar, its labels and the title below them, so it deepens by the title's
// line box and one gap, and the title stays inside it.
#[test]
fn a_title_along_a_lying_bar_deepens_its_fitted_row_by_its_line_height_and_a_gap() {
    let (mut fx, _, scale, panel) = row_over_bar(TrackSize::Fit);
    let shallow = panel_rect(&compile_figure(&fx.fig.clone()), panel).height;
    fx.scale_mut(scale).title = Some(Text::plain(SPEED));
    let scene = compile_figure(&fx.build());
    let area = panel_rect(&scene, panel);
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let title = text_box(&scene, panel, SPEED);
    let band = union_of(tick_boxes(&drawn_by(&scene, panel), &hit));
    assert_close(
        area.height - shallow,
        title.rect.bottom() - band.bottom(),
        SAME,
    );
    assert!(
        title.rect.bottom() <= area.bottom() + SAME,
        "{title:?} within {area:?}"
    );
}

// Why: the two placements are spaced alike: the gap between a title along a bar and the tick labels it lies beyond is
// the gap between a title above a bar and the bar it heads, on a standing and on a lying bar, so that switching the
// placement moves the title without changing how loosely it is set.
#[test]
fn a_title_along_its_bar_is_one_title_gap_from_the_tick_labels_as_a_title_above_is_from_the_bar() {
    let measure = |vertical: bool, placement: ColorbarTitlePlacement| {
        let (mut fx, _, scale, panel) = if vertical {
            column_beside_stack(1, TrackSize::Fit)
        } else {
            row_over_bar(TrackSize::Fit)
        };
        bar_mut(&mut fx, panel).title_placement = placement;
        fx.scale_mut(scale).title = Some(Text::plain(SPEED));
        let scene = compile_figure(&fx.build());
        let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
        let text = text_box(&scene, panel, SPEED).rect;
        match placement {
            ColorbarTitlePlacement::Above => hit.bar.y - text.bottom(),
            ColorbarTitlePlacement::Along => {
                let band = union_of(tick_boxes(&drawn_by(&scene, panel), &hit));
                if vertical {
                    text.x - band.right()
                } else {
                    text.y - band.bottom()
                }
            }
        }
    };
    let above = measure(true, ColorbarTitlePlacement::Above);
    assert!(above > 0.0, "a title above a bar is separated from it");
    assert_close(measure(false, ColorbarTitlePlacement::Above), above, TEXT);
    assert_close(measure(true, ColorbarTitlePlacement::Along), above, TEXT);
    assert_close(measure(false, ColorbarTitlePlacement::Along), above, TEXT);
}

// Why: inside an axes the tick labels of a bar face the interior of the plot area, so a title along a standing inside
// bar lies beyond them toward the interior, reading upwards and centred on the bar, within the box behind the bar.
#[test]
fn along_a_standing_inside_bar_the_title_reads_upwards_beyond_its_tick_labels_toward_the_interior()
{
    for side in [InsideColorbarSide::East, InsideColorbarSide::West] {
        let (fx, ax) = inside_bar(side, ColorbarTitlePlacement::Along);
        let scene = compile_figure(&fx.build());
        let hit = bar_of(&scene, ColorbarOwner::Axes(ax));
        let title = text_box(&scene, ax, SPEED);
        assert!(
            title.upwards,
            "{side:?}: the title reads from bottom to top"
        );
        let band = union_of(tick_boxes(&drawn_by(&scene, ax), &hit));
        if side == InsideColorbarSide::East {
            assert!(
                band.right() <= hit.bar.x && title.rect.right() < band.x,
                "{side:?}: {title:?} left of {band:?}"
            );
        } else {
            assert!(
                band.x >= hit.bar.right() && title.rect.x > band.right(),
                "{side:?}: {title:?} right of {band:?}"
            );
        }
        assert_close(centre(title.rect).y, centre(hit.bar).y, TEXT);
        let backdrop = hit.backdrop.expect("an inside bar is boxed");
        assert!(
            inside(title.rect, backdrop, SAME),
            "{side:?}: {title:?} lies in the box {backdrop:?}"
        );
    }
}

// Why: placed above, the title of a standing inside bar is the heading it always was, upright over the top end of the
// bar, so an author who prefers headings keeps them.
#[test]
fn above_a_standing_inside_bar_the_title_is_an_upright_heading_over_its_top_end() {
    let (fx, ax) = inside_bar(InsideColorbarSide::East, ColorbarTitlePlacement::Above);
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Axes(ax));
    let title = text_box(&scene, ax, SPEED);
    assert!(title.upright, "{title:?}");
    assert!(
        title.rect.bottom() <= hit.bar.y + EXACT,
        "{title:?} above {:?}",
        hit.bar
    );
}

// Why: placed above, the title of a lying inside bar is a heading on the side of the bar away from its tick labels,
// which face the interior of the plot area: below a bar along the south edge and above one along the north edge. A
// heading on the labels' side would be the `Along` placement again, and one beyond the box would sit on the data, so
// the title and the labels lie on opposite sides of the bar and both lie within the box.
#[test]
fn above_a_lying_inside_bar_the_title_lies_opposite_its_tick_labels_within_the_box() {
    for side in [InsideColorbarSide::South, InsideColorbarSide::North] {
        let (fx, ax) = inside_bar(side, ColorbarTitlePlacement::Above);
        let scene = compile_figure(&fx.build());
        let hit = bar_of(&scene, ColorbarOwner::Axes(ax));
        let title = text_box(&scene, ax, SPEED);
        assert!(title.upright, "{side:?}: {title:?}");
        let band = union_of(tick_boxes(&drawn_by(&scene, ax), &hit));
        let (labels_above, title_above) = (
            band.bottom() <= hit.bar.y,
            title.rect.bottom() <= hit.bar.y + EXACT,
        );
        let (labels_below, title_below) = (
            band.y >= hit.bar.bottom(),
            title.rect.y >= hit.bar.bottom() - EXACT,
        );
        // The labels face the interior: up from a south bar, down from a north bar.
        if side == InsideColorbarSide::South {
            assert!(
                labels_above,
                "{side:?}: the tick labels {band:?} face the interior"
            );
        } else {
            assert!(
                labels_below,
                "{side:?}: the tick labels {band:?} face the interior"
            );
        }
        assert!(
            (labels_above && title_below) || (labels_below && title_above),
            "{side:?}: the title {title:?} and the tick labels {band:?} lie on opposite sides of {:?}",
            hit.bar
        );
        assert_close(centre(title.rect).x, centre(hit.bar).x, TEXT);
        let backdrop = hit.backdrop.expect("an inside bar is boxed");
        for (what, r) in [("title", title.rect), ("tick labels", band)] {
            assert!(
                inside(r, backdrop, SAME),
                "{side:?}: the {what} {r:?} lie in the box {backdrop:?}"
            );
        }
    }
}

// Why: along a lying inside bar the title lies beyond the tick labels toward the interior of the plot area, upright and
// centred on the bar: above the labels of a bar along the south edge and below those of a bar along the north edge.
#[test]
fn along_a_lying_inside_bar_the_title_lies_beyond_its_tick_labels_toward_the_interior() {
    for side in [InsideColorbarSide::South, InsideColorbarSide::North] {
        let (fx, ax) = inside_bar(side, ColorbarTitlePlacement::Along);
        let scene = compile_figure(&fx.build());
        let hit = bar_of(&scene, ColorbarOwner::Axes(ax));
        let title = text_box(&scene, ax, SPEED);
        assert!(title.upright, "{side:?}: {title:?}");
        let band = union_of(tick_boxes(&drawn_by(&scene, ax), &hit));
        if side == InsideColorbarSide::South {
            assert!(
                band.bottom() <= hit.bar.y && title.rect.bottom() < band.y,
                "{side:?}: {title:?} above {band:?}"
            );
        } else {
            assert!(
                band.y >= hit.bar.bottom() && title.rect.y > band.bottom(),
                "{side:?}: {title:?} below {band:?}"
            );
        }
        assert_close(centre(title.rect).x, centre(hit.bar).x, TEXT);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The length of a panel colourbar
// ---------------------------------------------------------------------------------------------------------------

// Why: the user's review found a lying bar as long as a row of plots overbearing; by default it is half the span of
// the plot boxes above it, centred under them, while a standing bar keeps the whole span of the plots beside it.
#[test]
fn a_lying_panel_bar_is_half_the_span_of_its_plots_by_default_and_centred_on_it() {
    let (fx, axes, _, panel) = row_over_bar(TrackSize::Fit);
    let scene = compile_figure(&fx.build());
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel)).bar;
    let (start, end) = span(&scene, &axes, false);
    assert_close(bar.width, 0.5 * (end - start), EXACT);
    assert_close(centre(bar).x, (start + end) / 2.0, EXACT);
}

// Why: the automatic length is a fraction like any other: the whole span for a standing bar and half of it for a lying
// one, so stating that fraction draws the same bar.
#[test]
fn the_automatic_length_is_the_whole_span_standing_and_half_of_it_lying() {
    let rect = |vertical: bool, length: Option<f64>| {
        let (mut fx, _, _, panel) = if vertical {
            column_beside_stack(2, TrackSize::Fit)
        } else {
            row_over_bar(TrackSize::Fit)
        };
        bar_mut(&mut fx, panel).length = length;
        bar_of(&compile_figure(&fx.build()), ColorbarOwner::Panel(panel)).bar
    };
    for (vertical, fraction) in [(true, 1.0), (false, 0.5)] {
        let (automatic, stated) = (rect(vertical, None), rect(vertical, Some(fraction)));
        for (a, b) in [
            (automatic.x, stated.x),
            (automatic.y, stated.y),
            (automatic.width, stated.width),
            (automatic.height, stated.height),
        ] {
            assert_close(a, b, EXACT);
        }
    }
}

// Why: a stated length is the fraction of the span that the whole key takes, its out-of-range keys included, as the
// length of an inside bar is a fraction of its edge; so the keys fit within it, and the bar shrinks to make room.
#[test]
fn a_stated_length_is_that_fraction_of_the_span_keys_included() {
    for vertical in [true, false] {
        let (mut fx, axes, scale, panel) = if vertical {
            column_beside_stack(2, TrackSize::Fit)
        } else {
            row_over_bar(TrackSize::Fit)
        };
        show_keys(&mut fx, scale);
        bar_mut(&mut fx, panel).length = Some(0.6);
        let scene = compile_figure(&fx.build());
        let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
        assert_eq!(hit.keys.len(), 3, "vertical {vertical}: {hit:?}");
        let (start, end) = span(&scene, &axes, vertical);
        let (lo, hi) = ends(with_keys(&hit), vertical);
        assert_close(hi - lo, 0.6 * (end - start), EXACT);
        assert_close((lo + hi) / 2.0, (start + end) / 2.0, EXACT);
    }
}

// Why: a shortened bar is a key that sits somewhere along its span, and the panel's alignment, which already places
// the bar across its track, says where: the centre centres it, and a location that names an end of the span pushes
// it there, so that a short bar can line up with the top of a stack of plots or the left of a row.
#[test]
fn a_shortened_bar_is_placed_along_its_span_by_the_alignment_of_the_panel() {
    use PanelLocation::*;
    // Along a standing bar the ends are the top and the bottom; along a lying one, the left and the right.
    let cases = [
        (
            true,
            [North, NorthEast, NorthWest],
            [South, SouthEast, SouthWest],
            [Center, East, West],
        ),
        (
            false,
            [West, NorthWest, SouthWest],
            [East, NorthEast, SouthEast],
            [Center, North, South],
        ),
    ];
    for (vertical, starts, finishes, centres) in cases {
        let place = |align: PanelLocation| {
            let (mut fx, axes, _, panel) = if vertical {
                column_beside_stack(2, TrackSize::Fit)
            } else {
                row_over_bar(TrackSize::Fit)
            };
            fx.panel_mut(panel).align = align;
            bar_mut(&mut fx, panel).length = Some(0.5);
            let scene = compile_figure(&fx.build());
            (
                ends(bar_of(&scene, ColorbarOwner::Panel(panel)).bar, vertical),
                span(&scene, &axes, vertical),
            )
        };
        for align in starts {
            let ((lo, _), (start, _)) = place(align);
            assert_close(lo, start, EXACT);
        }
        for align in finishes {
            let ((_, hi), (_, end)) = place(align);
            assert_close(hi, end, EXACT);
        }
        for align in centres {
            let ((lo, hi), (start, end)) = place(align);
            assert_close((lo + hi) / 2.0, (start + end) / 2.0, EXACT);
            assert_close(hi - lo, 0.5 * (end - start), EXACT);
        }
    }
}

// Why: a fitted track takes its size from the extent of its content across the bar, which the bar's length does not
// change; shortening a bar must not move the plots.
#[test]
fn the_length_of_a_bar_does_not_change_the_size_of_its_fitted_track() {
    for vertical in [true, false] {
        let size = |length: Option<f64>| {
            let (mut fx, _, _, panel) = if vertical {
                column_beside_stack(2, TrackSize::Fit)
            } else {
                row_over_bar(TrackSize::Fit)
            };
            bar_mut(&mut fx, panel).length = length;
            let area = panel_rect(&compile_figure(&fx.build()), panel);
            if vertical { area.width } else { area.height }
        };
        assert_close(size(Some(0.3)), size(None), EXACT);
        assert_close(size(Some(1.0)), size(None), EXACT);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Bar items in legends
// ---------------------------------------------------------------------------------------------------------------

/// Returns the pitch of the rows of the legend of [`between`], read from the baselines of its first two entries, and
/// the baselines of the labels of the entries before and after the bar item, as drawn. Reading the pitch from the
/// spacing of drawn labels, rather than from the size of an entry's click target, ties the count of rows to what the
/// reader sees.
fn rows_around(scene: &Scene, b: &Between) -> (f64, f64, f64) {
    let drawn = drawn_by(scene, b.panel);
    let before = baseline(&drawn, "Before");
    let pitch = before - baseline(&drawn, "First");
    assert!(pitch > 0.0, "the entries run down the column");
    (pitch, before, baseline(&drawn, "After"))
}

// Why: an author who wants a longer or a shorter key asks for its rows, and gets exactly that many: the entries
// around the item stand that many rows apart, so the item takes no more and no less room than it asked for, and each
// row added lengthens the bar by one row.
#[test]
fn a_bar_item_lies_in_exactly_the_rows_it_asks_for() {
    let mut lengths = Vec::new();
    for rows in [1, 3, 5, 8] {
        let b = between(rows);
        let scene = compile_figure(&b.fx.fig.clone());
        let (pitch, before, after) = rows_around(&scene, &b);
        assert_close(after - before, f64::from(rows + 1) * pitch, SAME);
        let hit = bar_of(&scene, ColorbarOwner::Legend(LegendOwner::Panel(b.panel)));
        let (above, below) = (
            text_box(&scene, b.panel, "Before").rect,
            text_box(&scene, b.panel, "After").rect,
        );
        assert!(
            hit.bar.y >= above.bottom() - EXACT && hit.bar.bottom() <= below.y + EXACT,
            "{rows} rows: the bar {:?} lies between the entries around it, {above:?} and {below:?}",
            hit.bar
        );
        assert!(hit.bar.height > 0.0, "{rows} rows: the bar is drawn");
        lengths.push((rows, hit.bar.height, pitch));
    }
    for pair in lengths.windows(2) {
        let ((r0, h0, pitch), (r1, h1, _)) = (pair[0], pair[1]);
        assert_close(h1 - h0, f64::from(r1 - r0) * pitch, SAME);
    }
}

// Why: a bar item's title reads along its bar beside its tick labels, rotated and centred on the bar as on a standing
// colourbar, so it adds no row: the entries around the item stay where they are and the bar keeps its length.
#[test]
fn a_bar_item_s_title_reads_upwards_beside_its_tick_labels_and_adds_no_row() {
    let plain = between(5);
    let untitled = compile_figure(&plain.fx.fig.clone());
    let mut b = between(5);
    b.fx.scale_mut(b.scale).title = Some(Text::plain(SPEED));
    let scene = compile_figure(&b.fx.fig.clone());
    let owner = ColorbarOwner::Legend(LegendOwner::Panel(b.panel));
    let hit = bar_of(&scene, owner);
    let title = text_box(&scene, b.panel, SPEED);
    assert!(title.upwards, "{title:?}");
    let band = union_of(tick_boxes(&drawn_by(&scene, b.panel), &hit));
    assert!(
        title.rect.x > band.right(),
        "{title:?} beyond the tick labels {band:?}"
    );
    assert_close(centre(title.rect).y, centre(hit.bar).y, TEXT);
    let (_, before, after) = rows_around(&scene, &b);
    let (_, plain_before, plain_after) = rows_around(&untitled, &plain);
    assert_close(before, plain_before, EXACT);
    assert_close(after, plain_after, EXACT);
    assert_close(hit.bar.height, bar_of(&untitled, owner).bar.height, EXACT);
}

// Why: a fitted column holding a titled bar item widens by the title's line box and one gap, as a fitted column of a
// standing colourbar does, and the title stays inside the panel.
#[test]
fn a_titled_bar_item_widens_its_fitted_column_by_the_title_s_line_height_and_a_gap() {
    let (mut fx, scale, panel) = bar_item_in_fitted_column();
    let narrow = panel_rect(&compile_figure(&fx.fig.clone()), panel).width;
    fx.scale_mut(scale).title = Some(Text::plain(SPEED));
    let scene = compile_figure(&fx.build());
    let area = panel_rect(&scene, panel);
    let hit = bar_of(&scene, ColorbarOwner::Legend(LegendOwner::Panel(panel)));
    let title = text_box(&scene, panel, SPEED);
    let band = union_of(tick_boxes(&drawn_by(&scene, panel), &hit));
    assert_close(area.width - narrow, title.rect.right() - band.right(), SAME);
    assert!(
        inside(title.rect, area, SAME),
        "{title:?} lies in the panel {area:?}"
    );
}

// Why: the user's review found the keys of a bar item, drawn as squares beyond the ends of its bar, out of step with
// the rows of the legend; each shown key is instead an ordinary entry row, a swatch beside a label like any other
// swatch entry, directly above the bar's rows (the above key) or below them (the below key, then the non-finite key),
// so that five rows and three keys are eight rows. The swatch shows what is painted, the label is the key's default
// label, and the bar keeps its five rows.
#[test]
fn the_keys_of_a_bar_item_are_entry_rows_directly_above_and_below_its_bar() {
    let plain = {
        let b = between(5);
        let scene = compile_figure(&b.fx.fig.clone());
        bar_of(&scene, ColorbarOwner::Legend(LegendOwner::Panel(b.panel)))
            .bar
            .height
    };
    let mut b = between(5);
    show_keys(&mut b.fx, b.scale);
    // A custom swatch after the item, whose swatch the keys' swatches must match.
    if let LegendEntries::Explicit(items) = &mut b.fx.legend_mut(b.panel).entries {
        items.push(LegendItem::custom(
            LegendSample::swatch(AMBER),
            "Hand-written",
        ));
    }
    let scene = compile_figure(&b.fx.fig.clone());
    let hit = bar_of(&scene, ColorbarOwner::Legend(LegendOwner::Panel(b.panel)));
    assert_close(hit.bar.height, plain, SAME);
    let (pitch, before, after) = rows_around(&scene, &b);
    let above = text_starting(&scene, b.panel, "> ");
    let below = text_starting(&scene, b.panel, "< ");
    let nan = "NaN / Inf".to_owned();
    let drawn = drawn_by(&scene, b.panel);
    let at = |text: &str| baseline(&drawn, text);
    assert_close(at(&above) - before, pitch, SAME);
    assert_close(at(&below) - at(&above), 6.0 * pitch, SAME);
    assert_close(at(&nan) - at(&below), pitch, SAME);
    assert_close(after - at(&nan), pitch, SAME);
    assert_close(after - before, 9.0 * pitch, SAME);
    // The bar lies between the above key's row and the below key's row.
    let (above_box, below_box) = (
        text_box(&scene, b.panel, &above).rect,
        text_box(&scene, b.panel, &below).rect,
    );
    assert!(
        hit.bar.y >= above_box.bottom() && hit.bar.bottom() <= below_box.y,
        "{:?} lies between {above_box:?} and {below_box:?}",
        hit.bar
    );

    // Each key is an ordinary swatch entry: its label in the label column, its swatch the custom swatch's size and
    // place across the row, filled with what is painted and outlined.
    let label_x = text_box(&scene, b.panel, "Before").rect.x;
    let reference = swatch_left_of(&scene, b.panel, "Hand-written");
    for (text, fill) in [
        (above.as_str(), Some([255, 0, 0])),
        (below.as_str(), Some(VIRIDIS[0])),
        (nan.as_str(), None),
    ] {
        assert_close(text_box(&scene, b.panel, text).rect.x, label_x, SAME);
        let swatch = swatch_left_of(&scene, b.panel, text);
        assert_close(swatch.square.x, reference.square.x, SAME);
        assert_close(swatch.square.width, reference.square.width, SAME);
        assert_close(swatch.square.height, reference.square.height, SAME);
        assert!(swatch.outlined, "{text}: the swatch is outlined");
        match fill {
            Some(colour) => assert_eq!(
                swatch.fill.map(rgb8),
                Some(colour),
                "{text}: the swatch shows what is painted"
            ),
            None => assert!(
                swatch.fill.is_none_or(|c| c.a == 0.0),
                "{text}: values drawn as nothing are keyed empty"
            ),
        }
        // The bar still records the key, at its swatch, so that the viewer can report it.
        let key = match text {
            t if t == above => ColorScaleKey::Above,
            t if t == below => ColorScaleKey::Below,
            _ => ColorScaleKey::NonFinite,
        };
        let rect = key_rect(&hit, key).unwrap_or_else(|| panic!("{key:?} is recorded"));
        assert!(
            (rect.x - swatch.square.x).abs() <= 1.0 && (rect.y - swatch.square.y).abs() <= 1.0,
            "{key:?}: {rect:?} is the swatch {:?}",
            swatch.square
        );
    }
}

// Why: a bar item counts as its rows and its keys when the entries are divided among columns, as it counted as five
// entries before; a bar of one row with two keys is three entries, so a key of it and five series in two columns of
// four entries each holds the bar, its keys and the first series in the first column and the other four series in
// the second. Counted as five, the bar would stand alone in the first column instead.
#[test]
fn a_bar_item_counts_as_its_rows_and_keys_when_entries_are_divided_among_columns() {
    let (mut fx, ax, panel) = axes_over_key();
    let lines: Vec<NodeId> = ["A", "B", "C", "D", "E"]
        .iter()
        .map(|n| fx.named_line(ax, n, |_| {}))
        .collect();
    let scale = fx.color_scale(viridis(0.0, 1.0));
    match &mut fx.scale_mut(scale).kind {
        ColorScaleKind::Continuous(c) => c.above = keyed(OutOfRange::Clamp),
        _ => unreachable!(),
    }
    fx.scale_mut(scale).non_finite = keyed(OutOfRange::Transparent);
    fx.legend_mut(panel).layout = LegendEntryLayout {
        columns: LegendEntryColumns::Count(2),
        order: LegendEntryOrder::ColumnMajor,
    };
    let mut items = vec![LegendItem::color_scale(scale).with_rows(1)];
    items.extend(lines.iter().map(|&l| LegendItem::artist(l)));
    fx.legend_mut(panel).entries = LegendEntries::Explicit(items);
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 5, "only the series are click targets");
    let first = entry_of(&scene, panel, lines[0]).rect;
    let second_column = entry_of(&scene, panel, lines[1]).rect.x;
    assert!(
        first.x < second_column,
        "the first series is in the first column"
    );
    for &line in &lines[1..] {
        assert_close(entry_of(&scene, panel, line).rect.x, second_column, SAME);
    }
    let hit = bar_of(&scene, ColorbarOwner::Legend(LegendOwner::Panel(panel)));
    assert!(
        hit.bar.right() < second_column,
        "the bar is in the first column"
    );
    let nan = text_box(&scene, panel, "NaN / Inf").rect;
    for text in [text_starting(&scene, panel, "> "), "NaN / Inf".to_owned()] {
        let key = text_box(&scene, panel, &text).rect;
        assert!(
            key.right() <= second_column + EXACT,
            "the key {text:?} is in the first column too: {key:?}"
        );
    }
    assert!(
        nan.y >= hit.bar.bottom(),
        "the non-finite key is below the bar"
    );
    assert!(
        first.y >= nan.bottom() - EXACT,
        "the first series follows the non-finite key"
    );
}

// Why: on a row of its own a bar item lies across the legend, so its length is the legend's width, not a number of
// rows, and its rows are ignored; its title reads along it, upright below its tick labels and centred on the bar, as
// along a lying colourbar.
#[test]
fn a_bar_item_on_a_row_of_its_own_ignores_its_rows_and_is_titled_below_its_tick_labels() {
    let draw = |rows: u32| {
        let (mut fx, ax, panel) = axes_over_key();
        let line = fx.named_line(ax, "Alpha series with a long label", |_| {});
        let scale = fx.color_scale(viridis(0.0, 10.0));
        fx.scale_mut(scale).title = Some(Text::plain(SPEED));
        fx.legend_mut(panel).entries = LegendEntries::Explicit(vec![
            LegendItem::artist(line),
            LegendItem {
                full_row: true,
                ..LegendItem::color_scale(scale).with_rows(rows)
            },
        ]);
        (compile_figure(&fx.build()), panel)
    };
    let (scene, panel) = draw(5);
    let hit = bar_of(&scene, ColorbarOwner::Legend(LegendOwner::Panel(panel)));
    assert_eq!(hit.orientation, ColorbarOrientation::Horizontal);
    let (other, _) = draw(9);
    let longer = bar_of(&other, ColorbarOwner::Legend(LegendOwner::Panel(panel))).bar;
    for (a, b) in [
        (hit.bar.x, longer.x),
        (hit.bar.y, longer.y),
        (hit.bar.width, longer.width),
        (hit.bar.height, longer.height),
    ] {
        assert_close(a, b, EXACT);
    }
    let title = text_box(&scene, panel, SPEED);
    assert!(title.upright, "{title:?}");
    let band = union_of(tick_boxes(&drawn_by(&scene, panel), &hit));
    assert!(
        title.rect.y > band.bottom(),
        "{title:?} below the tick labels {band:?}"
    );
    assert_close(centre(title.rect).x, centre(hit.bar).x, TEXT);
}

// ---------------------------------------------------------------------------------------------------------------
// The title of a panel
// ---------------------------------------------------------------------------------------------------------------

/// Returns the union of the entries of a legend panel: the block its title is aligned with.
fn block(scene: &Scene, panel: NodeId) -> Rect {
    union_of(panel_entries(scene, panel).into_iter().map(|e| e.rect))
}

// Why: a legend no longer has a title of its own; the panel that holds it does, and in a boxed legend panel the title
// heads the entries inside the border, so that the border frames the key and its heading together.
#[test]
fn a_boxed_legend_panel_s_title_lies_inside_its_border_above_its_entries() {
    let (mut fx, panel) = legend_under_two_axes(TrackSize::Fit);
    fx.panel_mut(panel).title = Some(Text::plain("Boundaries"));
    let scene = compile_figure(&fx.build());
    let title = text_box(&scene, panel, "Boundaries");
    assert!(title.upright, "{title:?}");
    let border = outlines(&scene, panel)
        .first()
        .and_then(Leaf::bbox)
        .expect("a boxed panel draws its border");
    assert!(
        inside(title.rect, border, EXACT),
        "{title:?} lies inside the border {border:?}"
    );
    assert!(title.rect.y > border.y, "the title is kept off the border");
    let entries = block(&scene, panel);
    assert!(
        title.rect.bottom() <= entries.y + EXACT,
        "{title:?} above the entries {entries:?}"
    );
}

// Why: above an unboxed legend the title heads the entries, with nothing between them; it must still lie above them
// and inside the panel, so that it reads as the legend's heading rather than as an entry.
#[test]
fn an_unboxed_legend_panel_s_title_lies_above_its_entries() {
    let (mut fx, panel) = legend_under_two_axes(TrackSize::Fit);
    fx.legend_mut(panel).boxed = false;
    fx.panel_mut(panel).title = Some(Text::plain("Boundaries"));
    let scene = compile_figure(&fx.build());
    let title = text_box(&scene, panel, "Boundaries");
    let entries = block(&scene, panel);
    assert!(
        title.rect.bottom() <= entries.y + EXACT,
        "{title:?} above the entries {entries:?}"
    );
    let area = panel_rect(&scene, panel);
    assert!(
        inside(title.rect, area, SAME),
        "{title:?} lies in the panel {area:?}"
    );
}

// Why: the title is aligned with the content it heads, as an axes title is aligned with its plot area: its left edge
// on the content's left edge, its centre on the content's centre, or its right edge on the content's right edge; a
// panel's content is the block of a legend's entries or a colourbar with its labels.
#[test]
fn a_panel_title_is_aligned_with_the_left_edge_centre_or_right_edge_of_its_content() {
    for align in [TitleAlign::Left, TitleAlign::Center, TitleAlign::Right] {
        let (mut fx, panel) = legend_under_two_axes(TrackSize::Fit);
        fx.panel_mut(panel).title = Some(Text::plain("Key"));
        fx.panel_mut(panel).title_align = align;
        let scene = compile_figure(&fx.build());
        let title = text_box(&scene, panel, "Key").rect;
        let entries = block(&scene, panel);
        let (t, c) = match align {
            TitleAlign::Left => (title.x, entries.x),
            TitleAlign::Center => (centre(title).x, centre(entries).x),
            TitleAlign::Right => (title.right(), entries.right()),
        };
        assert_close(t, c, SAME);

        let (mut fx, _, _, bar_panel) = column_beside_stack(1, TrackSize::Fit);
        fx.panel_mut(bar_panel).title = Some(Text::plain("Key"));
        fx.panel_mut(bar_panel).title_align = align;
        let scene = compile_figure(&fx.build());
        let title = text_box(&scene, bar_panel, "Key").rect;
        let hit = bar_of(&scene, ColorbarOwner::Panel(bar_panel));
        let content = union(
            hit.bar,
            union_of(tick_boxes(&drawn_by(&scene, bar_panel), &hit)),
        );
        let (t, c) = match align {
            TitleAlign::Left => (title.x, content.x),
            TitleAlign::Center => (centre(title).x, centre(content).x),
            TitleAlign::Right => (title.right(), content.right()),
        };
        assert_close(t, c, TEXT);
    }
}

// Why: above a standing bar that spans the plots, the panel's title heads the bar while the bar keeps the span of the
// plots exactly, as a scale's title placed above it does; a scale title above the bar lies between the two, nearer
// the bar it names.
#[test]
fn a_panel_title_heads_a_standing_bar_which_keeps_the_span_of_its_plots() {
    let (mut fx, axes, scale, panel) = column_beside_stack(2, TrackSize::Fit);
    fx.panel_mut(panel).title = Some(Text::plain("Key"));
    fx.scale_mut(scale).title = Some(Text::plain(SPEED));
    bar_mut(&mut fx, panel).title_placement = ColorbarTitlePlacement::Above;
    let scene = compile_figure(&fx.build());
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel)).bar;
    let (start, end) = span(&scene, &axes, true);
    assert_close(bar.y, start, EXACT);
    assert_close(bar.bottom(), end, EXACT);
    let title = text_box(&scene, panel, "Key").rect;
    let heading = text_box(&scene, panel, SPEED).rect;
    assert!(
        heading.bottom() <= bar.y + EXACT,
        "{heading:?} above {bar:?}"
    );
    assert!(
        title.bottom() <= heading.y + EXACT,
        "{title:?} above the scale's title {heading:?}"
    );
    assert!(title.y >= 0.0, "{title:?} lies on the page");
}

// Why: a fitted track is sized to its content, and the panel's title is part of it: a fitted row grows by the title's
// line box and a gap, whether the panel holds a legend or a lying colourbar, and a fitted column widens to hold a
// title wider than the content beneath it.
#[test]
fn a_panel_title_adds_to_the_size_of_a_fitted_track() {
    let depth = |titled: bool, colourbar: bool| {
        let (mut fx, panel) = if colourbar {
            let (fx, _, _, panel) = row_over_bar(TrackSize::Fit);
            (fx, panel)
        } else {
            legend_under_two_axes(TrackSize::Fit)
        };
        if titled {
            fx.panel_mut(panel).title = Some(Text::plain("Key"));
        }
        let scene = compile_figure(&fx.build());
        let height = panel_rect(&scene, panel).height;
        let title = titled.then(|| text_box(&scene, panel, "Key").rect);
        (height, title)
    };
    for colourbar in [false, true] {
        let (plain, _) = depth(false, colourbar);
        let (titled, title) = depth(true, colourbar);
        let title = title.expect("the title is drawn");
        let growth = titled - plain;
        assert!(
            growth > title.height && growth < title.height + FONT_SIZE,
            "colourbar {colourbar}: the row grows by the title's line and a gap, {growth} for {title:?}"
        );
    }

    let (mut fx, _, panel) = bar_item_in_fitted_column();
    let narrow = panel_rect(&compile_figure(&fx.fig.clone()), panel).width;
    let long = "A heading far wider than the bar beneath it";
    fx.panel_mut(panel).title = Some(Text::plain(long));
    let scene = compile_figure(&fx.build());
    let area = panel_rect(&scene, panel);
    let title = text_box(&scene, panel, long).rect;
    assert!(area.width > narrow, "the column widens to hold the title");
    assert!(
        inside(title, area, SAME),
        "{title:?} lies in the panel {area:?}"
    );
}
