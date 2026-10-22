//! Colour keys in legends (#24): a colour scale listed as an item of a legend, and the sample of a colour-mapped
//! artist drawn as a sweep of its scale. These tests own what the compiler draws for them: the entries a categorical
//! scale expands to, the bar a continuous, discrete or indexed scale draws in one column, their out-of-range keys,
//! the one swatch look, the sweep that replaces the middle-colour stopgap, and what clicking them toggles.
//!
//! Positions are asserted as relationships (one column, five rows, above, beside, equal to the bar of a panel
//! colourbar of the same scale) rather than as numbers of points, so that the tests survive a change of style
//! constants; colours are worked out from the scale (a red-to-blue listed colormap, the public viridis table) rather
//! than from the compiler's own mapping.
//!
//! The API these tests assume, beyond the IR of `crates/ironlab-ir/tests/legend_color_keys.rs`
//! (`LegendItemContent::ColorScale { scale, rows }` and `LegendItem::color_scale(NodeId)`) and the colourbar hit geometry of
//! `colorbars.rs`:
//!
//! ```text
//! // crates/ironlab-scene/src/hit.rs
//! pub enum ColorbarOwner {
//!     Panel(NodeId),
//!     Axes(NodeId),
//!     /// A bar item of a legend: inside the plot area of an axes or in a legend panel. A legend may hold several
//!     /// bar items, which `ColorbarHit::scale` tells apart.
//!     Legend(LegendOwner),                       // new
//! }
//! // A bar item records one `ColorbarHit` in `HitMap::colorbars`, with `orientation: Vertical`, `entries` empty,
//! // its `keys`, and `backdrop: None` (the box of an inside legend is the legend's, not the bar's). It records no
//! // `LegendHit`, because it stands for no artist. A category entry records no `LegendHit` either, unless it is
//! // merged with an artist entry, whose artists it then stands for.
//! ```
//!
//! Every display item of a legend, its bar items included, names the legend's source: the panel for a legend panel
//! and the axes for an inside legend.
//!
//! The rules these tests pin, beyond those the plan states:
//!
//! - **Five rows.** A bar item of the default five rows counts as five entries when the entries are divided among
//!   columns, and lies in one column over five consecutive rows, each as tall as the row of an entry with a one-line
//!   label. Its bar starts at the left edge of the sample column, where the samples of the other entries of the
//!   column start, and its tick labels lie to its right, on the side of the labels of the other entries; its width
//!   (bar, ticks and tick labels) sets the width of its column as a label does. Other row counts, the title along
//!   the bar and the keys as entry rows of their own are the layout stage's, in `colorbar_layout.rs`.
//! - **Drawn as a colourbar.** The bar, its ticks and its tick labels are those of a vertical colourbar of the same
//!   scale: the same thickness, the same colours at the same fractions of the length and the same explicit ticks.
//! - **Merging.** A category entry is merged with any other entry of the same legend panel that has the same label
//!   and the same look, whether that entry comes from an artist item, a custom item or another category, at the
//!   position of the first; entries that share a label but not a look are both listed. Two custom items that
//!   repeat each other are still both listed (ADR 0020), because neither is a category entry.
//! - **One swatch look.** A custom swatch and a category entry draw the same sample: a square of the colour, filled,
//!   with a thin black outline, smaller than its row so that a margin separates the swatches of neighbouring rows.
//!   A surface whose faces are one fixed colour draws a square of that colour outlined in its edge colour, so a
//!   surface with the default thin black edges draws exactly the swatch, and merges with a category entry of the same
//!   label and colour.
//! - **Sweeps.** The sweep of a colour-mapped artist runs across the sample column from the scale's low end at the
//!   left to its high end at the right, whatever the artist's data, as a lying colourbar does. A series that draws a
//!   line draws the line in the sweep, and its marker at the centre, when the marker follows the colour data, in the
//!   sweep's colour at the centre. A series that draws only markers coloured by data draws a row of several markers
//!   across the sample, each in the sweep's colour at its place, on one horizontal line.
//! - **Empty and missing scales.** An item naming a categorical scale with no categories, or naming no scale, draws
//!   nothing and the rest of the legend is drawn; the compiler adds no warning against the legend, because validation
//!   already reports the scale or the reference.

use ironlab_ir::{
    CategoricalColorScale, Cell, Color, ColorCategory, ColorScaleKind, ColorScaleLevels,
    ColorScaleNorm, ColorScaleOutOfRange, ColorSpec, ColorbarOrientation, Colormap, ColormapName,
    ContinuousColorScale, DashStyle, DiscreteColorScale, FigureSize, IndexedColorScale,
    InsideLegend, Legend, LegendEntries, LegendEntryColumns, LegendEntryLayout, LegendEntryOrder,
    LegendItem, LegendLocation, LegendSample, Limits, MarkerShape, NodeId, OutOfRange, Panel,
    PanelColorbar, PanelContent, PanelLocation, Text, Ticks, TitleAlign, TrackSize,
};
use ironlab_scene::display::{Point, Rect, Rgba};
use ironlab_scene::hit::{ColorbarHit, ColorbarOwner, LegendOwner};
use ironlab_scene::maths::colormap::VIRIDIS;
use ironlab_scene::{Scene, SceneWarningKind};
use ironlab_text::FontId;

use crate::common::{Fx, cell, compile_figure, out_of_range, rgb8, text, xy};
use crate::probe::{
    Leaf, assert_close, axes_hit, from_source, glyph_runs, inside, leaves, panel_entries,
    panel_rect, parse_number, pixel, text_bbox,
};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const GREY: Color = Color::rgb(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0);
const AMBER: Color = Color::rgb(0.9, 0.6, 0.0);

/// Positions closer than this, in points, are taken to be the same row or column of entries.
const SAME: f64 = 0.5;

/// Geometry that the compiler computes exactly, such as two pieces drawn by the same code.
const EXACT: f64 = 1e-6;

// ---------------------------------------------------------------------------------------------------------------
// Scales
// ---------------------------------------------------------------------------------------------------------------

fn manual(min: f64, max: f64) -> Limits {
    Limits::Manual { min, max }
}

/// A continuous scale of viridis with manual limits [0, 10].
fn viridis() -> ColorScaleKind {
    ColorScaleKind::Continuous(ContinuousColorScale {
        limits: manual(0.0, 10.0),
        ..ContinuousColorScale::default()
    })
}

/// A continuous scale interpolating from red at 0 to blue at 1, whose two ends are unmistakable.
fn red_to_blue() -> ColorScaleKind {
    ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Listed {
            colors: vec![RED, BLUE],
        },
        limits: manual(0.0, 1.0),
        ..ContinuousColorScale::default()
    })
}

/// A discrete scale of the red-to-blue colormap with the given band edges.
fn red_to_blue_bands(edges: &[f64]) -> ColorScaleKind {
    ColorScaleKind::Discrete(DiscreteColorScale {
        colormap: Colormap::Listed {
            colors: vec![RED, BLUE],
        },
        levels: ColorScaleLevels::Explicit {
            values: edges.to_vec(),
        },
        ..DiscreteColorScale::default()
    })
}

/// An indexed scale of the given colours, strict outside them.
fn palette(colors: Vec<Color>) -> ColorScaleKind {
    ColorScaleKind::Indexed(IndexedColorScale {
        colors,
        ..IndexedColorScale::default()
    })
}

/// A categorical scale of the given colours and plain labels, strict outside them, without keys.
fn categories(entries: &[(Color, &str)]) -> ColorScaleKind {
    ColorScaleKind::Categorical(CategoricalColorScale {
        categories: entries
            .iter()
            .map(|&(color, label)| ColorCategory {
                color,
                label: Text::plain(label),
            })
            .collect(),
        outside: out_of_range(OutOfRange::Strict),
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

/// Returns the continuous scale with the given identifier for modification.
fn continuous_mut(fx: &mut Fx, id: NodeId) -> &mut ContinuousColorScale {
    match &mut fx.scale_mut(id).kind {
        ColorScaleKind::Continuous(scale) => scale,
        other => panic!("the scale is continuous, found {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------------------------

fn fixed(columns: u32, order: LegendEntryOrder) -> LegendEntryLayout {
    LegendEntryLayout {
        columns: LegendEntryColumns::Count(columns),
        order,
    }
}

/// Sets the entries of a legend panel to an explicit list of items.
fn explicit(fx: &mut Fx, panel: NodeId, items: Vec<LegendItem>) {
    fx.legend_mut(panel).entries = LegendEntries::Explicit(items);
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
    fx.legend_mut(panel).layout = fixed(1, LegendEntryOrder::ColumnMajor);
    (fx, ax, panel)
}

/// [`axes_over_key`] with two named lines, "Before" and "After", and a scale of the given kind listed between them
/// in the panel. Returns the figure, the panel, the scale and the two lines.
struct Between {
    fx: Fx,
    panel: NodeId,
    scale: NodeId,
    before: NodeId,
    after: NodeId,
}

fn between(kind: ColorScaleKind) -> Between {
    let (mut fx, ax, panel) = axes_over_key();
    let before = fx.named_line(ax, "Before", |_| {});
    let after = fx.named_line(ax, "After", |_| {});
    let scale = fx.color_scale(kind);
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::artist(before),
            LegendItem::color_scale(scale),
            LegendItem::artist(after),
        ],
    );
    Between {
        fx,
        panel,
        scale,
        before,
        after,
    }
}

/// Adds a vertical colourbar panel of `scale` in `cell` and returns its identifier.
fn colorbar_panel(fx: &mut Fx, cell: Cell, scale: NodeId) -> NodeId {
    let id = fx.node();
    fx.fig.panels.push(Panel {
        id,
        cell,
        align: PanelLocation::Center,
        title: None,
        title_align: TitleAlign::Left,
        content: PanelContent::Colorbar(PanelColorbar {
            scale,
            orientation: ColorbarOrientation::Vertical,
            ..PanelColorbar::default()
        }),
    });
    id
}

/// A 2 × 1 layout with an axes above a legend panel of one column that the axes names, the axes holding one artist
/// labelled "Mapped" that `add` adds. Returns the compiled scene and the panel.
fn swept(add: impl FnOnce(&mut Fx, NodeId)) -> (Scene, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    let ax = fx.axes2d(0, 0);
    let panel = fx.panel(cell(1, 0, 1, 1));
    fx.legend_mut(panel).layout = fixed(1, LegendEntryOrder::ColumnMajor);
    fx.legend_in(ax, panel);
    add(&mut fx, ax);
    (compile_figure(&fx.build()), panel)
}

/// A two-by-two field over a small grid, for images, contours and surfaces.
const GRID_X: [f64; 3] = [0.0, 1.0, 2.0];
const GRID_Y: [f64; 2] = [0.0, 1.0];

// ---------------------------------------------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------------------------------------------

/// Returns the bounding box of the label `text`, which must be drawn exactly once.
#[track_caller]
fn label(scene: &Scene, text: &str) -> Rect {
    let all = leaves(scene);
    let runs = crate::probe::runs_with_text(&all, text);
    assert_eq!(runs.len(), 1, "the label {text:?} is drawn once");
    text_bbox(&all, text).expect("the label has a box")
}

/// Returns the number of times the label `text` is drawn.
fn label_count(scene: &Scene, text: &str) -> usize {
    crate::probe::runs_with_text(&leaves(scene), text).len()
}

/// Returns the items a legend draws for the sample beside the label `text`: the items of `source` that end to the
/// left of the label, start no further left than four label heights from it, and whose vertical centre lies within
/// the height of the label.
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

/// Returns the box of the sample beside the label `text`: the union of the boxes of its items.
#[track_caller]
fn sample_box(scene: &Scene, source: NodeId, text: &str) -> Rect {
    sample_beside(scene, source, text)
        .iter()
        .filter_map(Leaf::bbox)
        .reduce(crate::probe::union)
        .unwrap_or_else(|| panic!("a sample is drawn beside {text:?}"))
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

/// Returns every filled axis-aligned rectangle among the leaves, with its fill colour.
fn filled_rects(leaves: &[Leaf]) -> Vec<(Rect, Rgba)> {
    leaves
        .iter()
        .filter(|l| is_rectangle(l))
        .filter_map(|l| Some((l.bbox()?, l.path()?.fill?.color)))
        .collect()
}

/// Returns every stroked axis-aligned rectangle among the leaves, with its stroke colour and width.
fn stroked_rects(leaves: &[Leaf]) -> Vec<(Rect, Rgba, f64)> {
    leaves
        .iter()
        .filter(|l| is_rectangle(l))
        .filter_map(|l| {
            let stroke = l.path()?.stroke.as_ref()?;
            Some((l.bbox()?, stroke.color, stroke.width))
        })
        .collect()
}

/// Returns whether two rectangles agree to within `tol` points on every edge.
fn rect_close(a: Rect, b: Rect, tol: f64) -> bool {
    (a.x - b.x).abs() <= tol
        && (a.y - b.y).abs() <= tol
        && (a.right() - b.right()).abs() <= tol
        && (a.bottom() - b.bottom()).abs() <= tol
}

/// The swatch beside a label: its square, its fill, and the colour and width of its outline, if it has one.
#[derive(Debug)]
struct Swatch {
    square: Rect,
    fill: Option<Rgba>,
    outline: Option<(Rgba, f64)>,
}

/// Returns the swatch drawn beside the label `text`: the one square sample, filled or not, with its outline. The fill
/// and the outline may be one item or two, and an outline drawn as a separate rectangle may be inset or outset by up
/// to its width (at most a point); the square is the filled rectangle, or the outline when nothing is filled.
#[track_caller]
fn swatch_beside(scene: &Scene, source: NodeId, text: &str) -> Swatch {
    let sample = sample_beside(scene, source, text);
    let squares: Vec<Rect> = sample
        .iter()
        .filter(|l| is_rectangle(l))
        .filter_map(Leaf::bbox)
        .collect();
    let square = filled_rects(&sample)
        .first()
        .map(|(r, _)| *r)
        .or_else(|| squares.first().copied())
        .unwrap_or_else(|| panic!("a square is drawn beside {text:?}: {sample:?}"));
    assert!(
        squares.iter().all(|r| rect_close(*r, square, 1.0)),
        "the sample beside {text:?} is one square, filled and outlined: {squares:?}"
    );
    Swatch {
        square,
        fill: filled_rects(&sample).first().map(|(_, c)| *c),
        outline: stroked_rects(&sample).first().map(|(_, c, w)| (*c, *w)),
    }
}

/// Returns the paint of the top-most image or filled path under `p`, as a colour with its alpha, ignoring strokes.
fn fill_under(leaves: &[Leaf], p: Point) -> Option<Rgba> {
    leaves.iter().rev().find_map(|l| {
        if let Some(image) = l.image() {
            let b = l.bbox()?;
            if !b.contains(p) {
                return None;
            }
            let local = l.transform.inverse()?.apply(p);
            let fx = (local.x - image.rect.x) / image.rect.width;
            let fy = (local.y - image.rect.y) / image.rect.height;
            let column = ((fx * f64::from(image.width)).floor().max(0.0) as usize)
                .min(image.width as usize - 1);
            let row = ((fy * f64::from(image.height)).floor().max(0.0) as usize)
                .min(image.height as usize - 1);
            let px = pixel(image, row, column);
            return Some(Rgba::new(
                f32::from(px[0]) / 255.0,
                f32::from(px[1]) / 255.0,
                f32::from(px[2]) / 255.0,
                f32::from(px[3]) / 255.0,
            ));
        }
        let fill = l.path()?.fill?;
        l.bbox().filter(|b| b.contains(p)).map(|_| fill.color)
    })
}

/// Returns the colour of the top-most stroked line that passes within its half width (and a quarter point) of `p`.
fn stroke_under(leaves: &[Leaf], p: Point) -> Option<Rgba> {
    leaves.iter().rev().find_map(|l| {
        let path = l.path()?;
        let stroke = path.stroke.as_ref()?;
        if path.fill.is_some() {
            return None;
        }
        let reach = stroke.width / 2.0 + 0.25;
        l.line_segments()
            .iter()
            .any(|&(a, b)| distance_to_segment(p, a, b) <= reach)
            .then_some(stroke.color)
    })
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    let (qx, qy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - qx).powi(2) + (p.y - qy).powi(2)).sqrt()
}

/// Returns the point at the fraction `f` across the sample beside the label `text`, on its horizontal centre line.
fn across_sample(scene: &Scene, source: NodeId, text: &str, f: f64) -> Point {
    let b = sample_box(scene, source, text);
    Point::new(b.x + f * b.width, b.y + b.height / 2.0)
}

/// Returns the fill of the sweep beside the label `text` at the fraction `f` across it.
#[track_caller]
fn sweep_fill(scene: &Scene, source: NodeId, text: &str, f: f64) -> Rgba {
    let p = across_sample(scene, source, text, f);
    fill_under(&sample_beside(scene, source, text), p)
        .unwrap_or_else(|| panic!("the sample beside {text:?} is filled at {f} across it"))
}

/// Returns the colour of the line sweep beside the label `text` at the fraction `f` across it. The line may be drawn
/// as stroked segments, or as a thin filled strip or image, so a fill is accepted where no stroke passes.
#[track_caller]
fn sweep_stroke(scene: &Scene, source: NodeId, text: &str, f: f64) -> Rgba {
    let p = across_sample(scene, source, text, f);
    let sample = sample_beside(scene, source, text);
    stroke_under(&sample, p)
        .or_else(|| fill_under(&sample, p))
        .unwrap_or_else(|| panic!("the line beside {text:?} is drawn at {f} across it"))
}

/// Returns the distance between two colours in 8-bit sRGB.
fn distance(a: [u8; 3], b: [u8; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (f64::from(*x) - f64::from(y)).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// The 8-bit colour of an IR colour.
fn rgb8_of(color: Color) -> [u8; 3] {
    [color.r, color.g, color.b].map(|c| (c * 255.0).round() as u8)
}

/// Asserts that a colour is nearer to `near` than to `far`, naming what is compared.
#[track_caller]
fn assert_nearer(colour: Rgba, near: [u8; 3], far: [u8; 3], what: &str) {
    let c = rgb8(colour);
    assert!(
        distance(c, near) < distance(c, far),
        "{what}: {c:?} should be nearer {near:?} than {far:?}"
    );
}

/// Returns the artists that clicking at `p` toggles: the targets of the legend entry there, or none.
fn toggles(scene: &Scene, p: Point) -> Vec<NodeId> {
    scene
        .hit_map
        .legend_entry_at(p)
        .map(|e| e.targets.iter().map(|&(_, artist)| artist).collect())
        .unwrap_or_default()
}

fn centre(r: Rect) -> Point {
    Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
}

/// Returns the bar items of a legend, in paint order.
fn legend_bars(scene: &Scene, legend: LegendOwner) -> Vec<ColorbarHit> {
    scene
        .hit_map
        .colorbars
        .iter()
        .filter(|c| c.owner == ColorbarOwner::Legend(legend))
        .cloned()
        .collect()
}

/// Returns the one bar item of a legend.
#[track_caller]
fn legend_bar(scene: &Scene, legend: LegendOwner) -> ColorbarHit {
    let bars = legend_bars(scene, legend);
    assert_eq!(
        bars.len(),
        1,
        "one bar item in {legend:?}: {:?}",
        scene.hit_map.colorbars
    );
    bars[0].clone()
}

/// Returns the one colourbar of a colourbar panel.
#[track_caller]
fn panel_bar(scene: &Scene, panel: NodeId) -> ColorbarHit {
    let found: Vec<&ColorbarHit> = scene
        .hit_map
        .colorbars
        .iter()
        .filter(|c| c.owner == ColorbarOwner::Panel(panel))
        .collect();
    assert_eq!(found.len(), 1, "one colourbar of panel {panel}");
    found[0].clone()
}

/// Returns the point on the centre line of a vertical bar at the fraction `f` of its length from its bottom.
fn along(hit: &ColorbarHit, f: f64) -> Point {
    let b = hit.bar;
    Point::new(b.x + b.width / 2.0, b.bottom() - f * b.height)
}

/// Returns the colour a bar paints at the fraction `f` of its length from its bottom.
#[track_caller]
fn bar_paint(leaves: &[Leaf], hit: &ColorbarHit, f: f64) -> [u8; 3] {
    rgb8(fill_under(leaves, along(hit, f)).unwrap_or_else(|| panic!("the bar is painted at {f}")))
}

/// Returns the plain numeric labels among the leaves (math runs are left out), with their values and boxes.
fn numeric(leaves: &[Leaf]) -> Vec<(f64, Rect, String)> {
    glyph_runs(leaves)
        .into_iter()
        .filter(|(_, g)| g.font != FontId::Math)
        .filter_map(|(l, g)| Some((parse_number(&g.text)?, l.bbox()?, g.text.clone())))
        .collect()
}

/// Returns the numeric labels that a bar's ticks carry: those to the right of the bar whose centre lies within its
/// length, give or take half a label.
fn tick_labels(leaves: &[Leaf], hit: &ColorbarHit) -> Vec<(f64, Rect, String)> {
    numeric(leaves)
        .into_iter()
        .filter(|(_, b, _)| {
            let cy = b.y + b.height / 2.0;
            b.x >= hit.bar.right()
                && cy >= hit.bar.y - b.height
                && cy <= hit.bar.bottom() + b.height
        })
        .collect()
}

/// Returns the y coordinate of the value `v` on a linear vertical bar from `min` to `max`.
fn place(hit: &ColorbarHit, v: f64, min: f64, max: f64) -> f64 {
    along(hit, (v - min) / (max - min)).y
}

/// Returns whether an overflow is reported for a legend.
fn overflows(scene: &Scene, legend: LegendOwner) -> bool {
    scene
        .warnings
        .iter()
        .any(|w| matches!(&w.kind, SceneWarningKind::LegendOverflow { legend: l } if *l == legend))
}

// ---------------------------------------------------------------------------------------------------------------
// A categorical scale expands to ordinary entries
// ---------------------------------------------------------------------------------------------------------------

// Why: a categorical scale is a classification whose every category has a label, so in a legend it is simply a list
// of entries, one per category, labelled by the category, in the order of the categories, each beside a swatch of the
// category's colour: the reader reads it as any other key.
#[test]
fn a_categorical_item_lists_one_entry_per_category_in_category_order() {
    let (mut fx, _, panel) = axes_over_key();
    let entries = [(RED, "taken"), (GREEN, "skipped"), (BLUE, "failed")];
    let status = fx.color_scale(categories(&entries));
    explicit(&mut fx, panel, vec![LegendItem::color_scale(status)]);
    let scene = compile_figure(&fx.build());
    let mut previous: Option<Rect> = None;
    for (colour, name) in entries {
        let b = label(&scene, name);
        if let Some(p) = previous {
            assert!(
                b.y > p.bottom() - EXACT,
                "{name} is listed below the category before it"
            );
            assert_close(b.x, p.x, SAME);
        }
        previous = Some(b);
        let swatch = swatch_beside(&scene, panel, name);
        assert_eq!(
            swatch.fill.map(rgb8),
            Some(rgb8_of(colour)),
            "{name} is keyed by its colour"
        );
    }
    assert!(
        legend_bars(&scene, LegendOwner::Panel(panel)).is_empty(),
        "a categorical scale draws no bar"
    );
}

// Why: category entries are ordinary entries, so they take their place among the other items of the list and fill
// the columns in the legend's order like any other entry; a key of six categories in two columns reads down the
// first column and then the second.
#[test]
fn category_entries_are_ordered_and_wrapped_into_columns_like_any_other_entry() {
    let (mut fx, ax, panel) = axes_over_key();
    let first = fx.named_line(ax, "First", |_| {});
    let status = fx.color_scale(categories(&[
        (RED, "a"),
        (GREEN, "b"),
        (BLUE, "c"),
        (AMBER, "d"),
    ]));
    fx.legend_mut(panel).layout = fixed(2, LegendEntryOrder::ColumnMajor);
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::artist(first),
            LegendItem::color_scale(status),
            LegendItem::custom(LegendSample::swatch(GREY), "Last"),
        ],
    );
    let scene = compile_figure(&fx.build());
    let r = |t: &str| label(&scene, t);
    // Six entries in two columns, column-major: First, a, b down the first column; c, d, Last down the second. The
    // boxes of glyphs are their ink, so labels of one row are compared by their bottoms, which lie on the baseline
    // when no letter descends, rather than by their tops, which differ between "c" and "First".
    assert_close(r("First").x, r("a").x, SAME);
    assert_close(r("a").x, r("b").x, SAME);
    assert!(r("First").y < r("a").y && r("a").y < r("b").y);
    assert!(r("c").x > r("b").right());
    assert_close(r("c").x, r("d").x, SAME);
    assert_close(r("d").x, r("Last").x, SAME);
    assert_close(r("c").bottom(), r("First").bottom(), SAME);
    assert_close(r("Last").bottom(), r("b").bottom(), SAME);
}

// Why: there is one swatch look, so that a hand-written swatch and a category of a scale, which mean the same thing
// to the reader, look the same: a square of the colour, outlined in thin black so that a pale colour still reads
// as a swatch, with a margin outside it so that the swatches of neighbouring rows never touch.
#[test]
fn a_custom_swatch_and_a_category_entry_draw_the_same_outlined_square() {
    let (mut fx, _, panel) = axes_over_key();
    let status = fx.color_scale(categories(&[(AMBER, "Category"), (GREEN, "Next")]));
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::custom(LegendSample::swatch(AMBER), "Hand-written"),
            LegendItem::color_scale(status),
        ],
    );
    let scene = compile_figure(&fx.build());
    let hand = swatch_beside(&scene, panel, "Hand-written");
    let category = swatch_beside(&scene, panel, "Category");
    let next = swatch_beside(&scene, panel, "Next");
    for (what, s) in [("the custom swatch", &hand), ("the category", &category)] {
        assert_close(s.square.width, s.square.height, EXACT);
        assert_eq!(s.fill.map(rgb8), Some(rgb8_of(AMBER)), "{what} is filled");
        let (colour, width) = s.outline.unwrap_or_else(|| panic!("{what} is outlined"));
        assert_eq!(rgb8(colour), [0, 0, 0], "{what} is outlined in black");
        assert!(
            width > 0.0 && width <= 1.0,
            "{what} is outlined thinly: {width}"
        );
    }
    assert!(
        rect_close(
            hand.square,
            Rect::new(
                hand.square.x,
                hand.square.y,
                category.square.width,
                category.square.height
            ),
            EXACT
        ),
        "the two squares are the same size: {hand:?} and {category:?}"
    );
    assert_close(hand.square.x, category.square.x, EXACT);
    assert!(
        next.square.y - category.square.bottom() > 0.0
            && category.square.y - hand.square.bottom() > 0.0,
        "a margin separates the swatches of neighbouring rows"
    );
}

// Why: the plan's core promise for categories: an entry merges by name and look like any other, so a hand-written
// swatch that says what a category says (same label, same colour) is one entry, not two, wherever it stands in the
// list; the merged entry takes the place of the first. A swatch of the same label in another colour is another
// thing and is listed as well.
#[test]
fn a_custom_swatch_of_the_same_label_and_colour_merges_with_a_category_entry() {
    let build = |items: fn(NodeId) -> Vec<LegendItem>| {
        let (mut fx, _, panel) = axes_over_key();
        let status = fx.color_scale(categories(&[(RED, "alpha"), (GREEN, "beta")]));
        explicit(&mut fx, panel, items(status));
        compile_figure(&fx.build())
    };

    let custom_first = build(|s| {
        vec![
            LegendItem::custom(LegendSample::Swatch { color: RED }, Text::plain("alpha")),
            LegendItem::color_scale(s),
        ]
    });
    assert_eq!(
        label_count(&custom_first, "alpha"),
        1,
        "the custom swatch and the category are one entry"
    );
    assert!(label(&custom_first, "alpha").y < label(&custom_first, "beta").y);

    let category_first = build(|s| {
        vec![
            LegendItem::color_scale(s),
            LegendItem::custom(LegendSample::Swatch { color: GREEN }, Text::plain("beta")),
        ]
    });
    assert_eq!(label_count(&category_first, "beta"), 1);
    assert!(
        label(&category_first, "alpha").y < label(&category_first, "beta").y,
        "the merged entry keeps the place of the category"
    );

    // A merge cannot be told from dropping one of two identical entries by looking at the merged entry, but it can by
    // its place: a custom swatch listed before the scale that repeats its second category takes that category to the
    // place of the custom swatch, ahead of the first category, which dropping the custom swatch would not.
    let custom_before_second = build(|s| {
        vec![
            LegendItem::custom(LegendSample::Swatch { color: GREEN }, Text::plain("beta")),
            LegendItem::color_scale(s),
        ]
    });
    assert_eq!(label_count(&custom_before_second, "beta"), 1);
    assert!(
        label(&custom_before_second, "beta").y < label(&custom_before_second, "alpha").y,
        "the merged entry takes the place of the custom swatch, the first of the two"
    );

    let other_colour = build(|s| {
        vec![
            LegendItem::custom(LegendSample::Swatch { color: BLUE }, Text::plain("alpha")),
            LegendItem::color_scale(s),
        ]
    });
    assert_eq!(
        label_count(&other_colour, "alpha"),
        2,
        "a different look is a different entry"
    );
}

// Why: a key may list a category and the plot drawn in that category's colour under the same name; the reader sees
// one thing, so it is one entry, and because it now stands for the plot, clicking it hides or shows the plot, while a
// category that stands for no plot remains no click target.
#[test]
fn a_category_entry_merges_with_an_artist_entry_of_the_same_label_and_look() {
    let (mut fx, ax, panel) = axes_over_key();
    let status = fx.color_scale(categories(&[(RED, "alpha"), (GREEN, "beta")]));
    // A surface of one fixed colour with the default thin black edges draws exactly the swatch.
    let surface = fx.surface(
        ax,
        &GRID_X,
        &GRID_Y,
        |x, y| x + y,
        |s| {
            s.label = Some(Text::plain("alpha"));
            s.face = ColorSpec::Rgba { color: RED };
        },
    );
    explicit(
        &mut fx,
        panel,
        vec![LegendItem::color_scale(status), LegendItem::artist(surface)],
    );
    let scene = compile_figure(&fx.build());
    assert_eq!(
        label_count(&scene, "alpha"),
        1,
        "one entry for the category and the surface"
    );
    assert!(
        label(&scene, "alpha").y < label(&scene, "beta").y,
        "at the place of the category"
    );
    assert_eq!(
        toggles(&scene, centre(label(&scene, "alpha"))),
        vec![surface]
    );
    assert_eq!(
        toggles(&scene, centre(label(&scene, "beta"))),
        Vec::<NodeId>::new()
    );
    let entries = panel_entries(&scene, panel);
    assert_eq!(
        entries.len(),
        1,
        "only the merged entry is a click target: {entries:?}"
    );
    assert_eq!(entries[0].targets, vec![(ax, surface)]);
}

// Why: for a categorical scale the out-of-range keys are simply further entries after the categories (#74's
// "not entered" swatch): each a swatch filled with what is painted for its values, or empty for values drawn as
// nothing, labelled with the author's label or the default one, in the order of a colourbar's keys.
#[test]
fn the_keys_of_a_categorical_item_are_further_entries_after_its_categories() {
    let (mut fx, _, panel) = axes_over_key();
    let mut kind = categories(&[(RED, "taken"), (GREEN, "skipped")]);
    if let ColorScaleKind::Categorical(c) = &mut kind {
        c.outside = keyed(OutOfRange::Rgba { color: GREY });
    }
    let status = fx.color_scale(kind);
    fx.scale_mut(status).non_finite = ColorScaleOutOfRange {
        policy: OutOfRange::Transparent,
        show_key: true,
        label: Some(Text::plain("not entered")),
    };
    explicit(&mut fx, panel, vec![LegendItem::color_scale(status)]);
    let scene = compile_figure(&fx.build());
    let order = ["taken", "skipped", "Out of range", "not entered"];
    for pair in order.windows(2) {
        assert!(
            label(&scene, pair[0]).y < label(&scene, pair[1]).y,
            "{} is listed before {}",
            pair[0],
            pair[1]
        );
        assert_close(label(&scene, pair[0]).x, label(&scene, pair[1]).x, SAME);
    }
    assert_eq!(
        label_count(&scene, "NaN / Inf"),
        0,
        "the author's label replaces the default"
    );
    let outside = swatch_beside(&scene, panel, "Out of range");
    assert_eq!(
        outside.fill.map(rgb8),
        Some(rgb8_of(GREY)),
        "the outside key shows its fixed colour"
    );
    let nothing = swatch_beside(&scene, panel, "not entered");
    assert!(
        nothing.fill.is_none_or(|c| c.a == 0.0),
        "values drawn as nothing are keyed by an empty square: {nothing:?}"
    );
    assert!(
        nothing.outline.is_some(),
        "the empty square is still outlined"
    );
    let category = swatch_beside(&scene, panel, "taken");
    assert_close(nothing.square.width, category.square.width, EXACT);
    assert_close(nothing.square.x, category.square.x, EXACT);
}

// Why: an item on a row of its own spans every column, and a categorical scale is a list of ordinary entries, so a
// categorical item on a row of its own puts each of its entries on a row of its own, in category order, spanning the
// columns between the entries around it rather than being wrapped among them.
#[test]
fn a_categorical_item_on_a_row_of_its_own_puts_each_category_on_a_row_of_its_own() {
    let (mut fx, ax, panel) = axes_over_key();
    let lines: Vec<NodeId> = ["A", "B", "C", "D"]
        .iter()
        .map(|n| fx.named_line(ax, n, |_| {}))
        .collect();
    let status = fx.color_scale(categories(&[(RED, "taken"), (GREEN, "skipped")]));
    fx.legend_mut(panel).layout = fixed(2, LegendEntryOrder::RowMajor);
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::artist(lines[0]),
            LegendItem::artist(lines[1]),
            LegendItem {
                full_row: true,
                ..LegendItem::color_scale(status)
            },
            LegendItem::artist(lines[2]),
            LegendItem::artist(lines[3]),
        ],
    );
    let scene = compile_figure(&fx.build());
    let r = |t: &str| label(&scene, t);
    // A and B share the first row; then each category has a row to itself; then C and D share a row.
    assert_close(r("A").bottom(), r("B").bottom(), SAME);
    assert!(r("B").x > r("A").right());
    assert!(r("taken").y > r("A").bottom());
    assert!(r("skipped").y > r("taken").bottom());
    assert!(r("C").y > r("skipped").bottom());
    assert_close(r("C").bottom(), r("D").bottom(), SAME);
    for name in ["taken", "skipped"] {
        assert_close(r(name).x, r("A").x, SAME);
        let swatch = swatch_beside(&scene, panel, name);
        assert!(swatch.outline.is_some(), "{name} is keyed by a swatch");
    }
    assert!(legend_bars(&scene, LegendOwner::Panel(panel)).is_empty());
}

// Why: the non-finite key without a label of the author's is labelled as a colourbar labels it, so that the same
// scale says the same thing in a bar and in a legend.
#[test]
fn a_categorical_non_finite_key_without_a_label_takes_the_default_label() {
    let (mut fx, _, panel) = axes_over_key();
    let status = fx.color_scale(categories(&[(RED, "taken")]));
    fx.scale_mut(status).non_finite = keyed(OutOfRange::Transparent);
    explicit(&mut fx, panel, vec![LegendItem::color_scale(status)]);
    let scene = compile_figure(&fx.build());
    assert!(label(&scene, "taken").y < label(&scene, "NaN / Inf").y);
}

// Why: a categorical scale with no categories is invalid (validation reports it against the scale), but compilation
// never fails: the item draws nothing, the rest of the key is drawn from the top, nothing overflows, and the compiler
// does not repeat against the legend what validation already says against the scale.
#[test]
fn an_empty_categorical_item_draws_nothing_and_the_rest_of_the_key_is_drawn() {
    let (mut fx, _, panel) = axes_over_key();
    let empty = fx.color_scale(categories(&[]));
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::color_scale(empty),
            LegendItem::color_scale(NodeId(9_999)),
            LegendItem::custom(LegendSample::swatch(AMBER), "Survivor"),
        ],
    );
    let lone = {
        let (mut fx, _, panel) = axes_over_key();
        explicit(
            &mut fx,
            panel,
            vec![LegendItem::custom(LegendSample::swatch(AMBER), "Survivor")],
        );
        compile_figure(&fx.build())
    };
    let scene = compile_figure(&fx.build());
    assert_close(
        label(&scene, "Survivor").y,
        label(&lone, "Survivor").y,
        EXACT,
    );
    assert!(legend_bars(&scene, LegendOwner::Panel(panel)).is_empty());
    assert!(!overflows(&scene, LegendOwner::Panel(panel)));
    assert!(
        scene.warnings.iter().all(|w| w.node != Some(panel)),
        "{:?}",
        scene.warnings
    );
}

// Why: a key inside a plot area lists categories as a panel does, so that a single plot of a classification can
// carry its key over the plot.
#[test]
fn an_inside_legend_lists_the_categories_of_a_scale() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let status = fx.color_scale(categories(&[(RED, "taken"), (GREEN, "skipped")]));
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        entries: LegendEntries::Explicit(vec![LegendItem::color_scale(status)]),
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    for (colour, name) in [(RED, "taken"), (GREEN, "skipped")] {
        assert!(
            inside(label(&scene, name), plot, EXACT),
            "{name} lies in the plot area"
        );
        assert_eq!(
            swatch_beside(&scene, ax, name).fill.map(rgb8),
            Some(rgb8_of(colour))
        );
    }
}

// Why: a legend inside a plot area lists its items as they are given and merges nothing (ADR 0020), category entries
// included: only a legend panel, which gathers entries from several axes, merges entries of the same label and look.
#[test]
fn an_inside_legend_lists_a_custom_swatch_and_a_category_of_the_same_label_and_colour() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let status = fx.color_scale(categories(&[(RED, "taken"), (GREEN, "skipped")]));
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        entries: LegendEntries::Explicit(vec![
            LegendItem::custom(LegendSample::Swatch { color: RED }, Text::plain("taken")),
            LegendItem::color_scale(status),
        ]),
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    assert_eq!(
        label_count(&scene, "taken"),
        2,
        "the custom swatch and the category are both listed"
    );
    assert_eq!(label_count(&scene, "skipped"), 1);
}

// ---------------------------------------------------------------------------------------------------------------
// A continuous, discrete or indexed scale draws a bar in one column
// ---------------------------------------------------------------------------------------------------------------

// Why: a bar is read against its ticks, so it needs more length than an entry row gives; five rows is long enough to
// read and short enough to sit among other entries. It lies in the column of the entries around it, starting where
// their samples start, so the key stays one aligned list, and it is a vertical bar recorded for hovering like any
// other bar of the scale.
#[test]
fn a_bar_item_is_a_vertical_bar_five_entry_rows_long_in_its_column() {
    let b = between(viridis());
    let scene = compile_figure(&b.fx.build());
    let owner = LegendOwner::Panel(b.panel);
    let bar = legend_bar(&scene, owner);
    assert_eq!(bar.scale, b.scale);
    assert_eq!(bar.orientation, ColorbarOrientation::Vertical);
    assert!(bar.entries.is_empty() && bar.keys.is_empty() && bar.backdrop.is_none());

    let entries = panel_entries(&scene, b.panel);
    let before = entries
        .iter()
        .find(|e| e.targets[0].1 == b.before)
        .unwrap()
        .rect;
    let after = entries
        .iter()
        .find(|e| e.targets[0].1 == b.after)
        .unwrap()
        .rect;
    let pitch = before.height;
    assert_close(after.y - before.bottom(), 5.0 * pitch, SAME);
    assert!(
        bar.bar.y >= before.bottom() - EXACT && bar.bar.bottom() <= after.y + EXACT,
        "the bar lies within its five rows: {bar:?} between {before:?} and {after:?}"
    );
    assert!(
        bar.bar.height > 4.0 * pitch,
        "the bar runs along most of its rows"
    );
    assert_close(bar.bar.x, sample_box(&scene, b.panel, "Before").x, 0.01);
}

// Why: a bar item is ticked on the side of the labels of the other entries, its right, so that the key reads left
// to right as samples and then words; every tick label sits at the place of its value along the bar.
#[test]
fn a_bar_item_is_ticked_on_its_label_side_at_the_places_of_the_values() {
    let mut b = between(viridis());
    continuous_mut(&mut b.fx, b.scale).ticks = Ticks::Values(vec![0.0, 5.0, 10.0]);
    let scene = compile_figure(&b.fx.build());
    let bar = legend_bar(&scene, LegendOwner::Panel(b.panel));
    let drawn = from_source(&leaves(&scene), b.panel);
    let ticks = tick_labels(&drawn, &bar);
    let mut values: Vec<f64> = ticks.iter().map(|t| t.0).collect();
    values.sort_by(f64::total_cmp);
    assert_eq!(values, [0.0, 5.0, 10.0], "{ticks:?}");
    for (v, b, _) in &ticks {
        assert_close(
            b.y + b.height / 2.0,
            place(&bar, *v, 0.0, 10.0),
            b.height / 2.0,
        );
    }
    let segments: Vec<(Point, Point)> = drawn.iter().flat_map(Leaf::line_segments).collect();
    for v in [0.0, 5.0, 10.0] {
        let y = place(&bar, v, 0.0, 10.0);
        assert!(
            segments.iter().any(|(a, c)| {
                (a.y - y).abs() < 0.5
                    && (c.y - y).abs() < 0.5
                    && (a.x.min(c.x) - bar.bar.right()).abs() < 0.5
                    && a.x.max(c.x) > bar.bar.right() + 0.5
            }),
            "a tick mark at {v} leaves the right edge of the bar"
        );
    }
}

// The title of a bar item, which reads along the bar, and its out-of-range keys, which are entry rows of their own,
// are tested in `colorbar_layout.rs`, beside the other choices of where a scale's title and keys are drawn.

// Why: a bar item holds no colours, limits or ticks of its own: it is a vertical colourbar of its scale, so a bar in
// a legend and a bar in a panel of the same scale must agree on thickness, colours at every fraction of their
// lengths and explicit ticks, for each kind of scale that draws a bar, or one of them lies about the plots.
#[test]
fn a_bar_item_agrees_with_a_panel_colourbar_of_the_same_scale() {
    let kinds = [
        (
            "continuous",
            ColorScaleKind::Continuous(ContinuousColorScale {
                limits: manual(0.0, 10.0),
                ticks: Ticks::Values(vec![0.0, 5.0, 10.0]),
                ..ContinuousColorScale::default()
            }),
            // Within samples of the 256 of a continuous bar, not on the boundary between two, where rounding decides.
            vec![0.1, 0.55, 0.9],
        ),
        (
            "discrete",
            ColorScaleKind::Discrete(DiscreteColorScale {
                levels: ColorScaleLevels::Explicit {
                    values: vec![0.0, 1.0, 5.0, 10.0],
                },
                ticks: Ticks::Values(vec![0.0, 1.0, 5.0, 10.0]),
                ..DiscreteColorScale::default()
            }),
            vec![0.05, 0.3, 0.75],
        ),
        (
            "indexed",
            ColorScaleKind::Indexed(IndexedColorScale {
                colors: vec![RED, GREEN, BLUE],
                ticks: Ticks::Values(vec![0.0, 1.0, 2.0]),
                ..IndexedColorScale::default()
            }),
            vec![1.0 / 6.0, 0.5, 5.0 / 6.0],
        ),
    ];
    for (what, kind, fractions) in kinds {
        let mut fx = Fx::new();
        fx.fig.size = FigureSize {
            width_mm: 160.0,
            height_mm: 200.0,
        };
        fx.fig.layout.rows = 2;
        fx.fig.layout.cols = 2;
        fx.fig.layout.col_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit];
        fx.axes2d(0, 0);
        let scale = fx.color_scale(kind);
        let bar_panel = colorbar_panel(&mut fx, cell(0, 1, 1, 1), scale);
        let key = fx.panel(cell(1, 0, 1, 1));
        explicit(&mut fx, key, vec![LegendItem::color_scale(scale)]);
        let scene = compile_figure(&fx.build());
        let in_panel = panel_bar(&scene, bar_panel);
        let in_legend = legend_bar(&scene, LegendOwner::Panel(key));
        assert_close(in_legend.bar.width, in_panel.bar.width, EXACT);
        let all = leaves(&scene);
        let panel_drawn = from_source(&all, bar_panel);
        let legend_drawn = from_source(&all, key);
        for f in fractions {
            assert_eq!(
                bar_paint(&legend_drawn, &in_legend, f),
                bar_paint(&panel_drawn, &in_panel, f),
                "{what}: the colour at {f} of the bar"
            );
        }
        let texts = |leaves: &[Leaf], hit: &ColorbarHit| {
            let mut t: Vec<String> = tick_labels(leaves, hit).into_iter().map(|t| t.2).collect();
            t.sort();
            t
        };
        assert_eq!(
            texts(&legend_drawn, &in_legend),
            texts(&panel_drawn, &in_panel),
            "{what}: the tick labels"
        );
    }
}

// Why: a bar item belongs to one column and counts as five entries when the entries are divided among columns, so a
// key of a bar and five series in two columns puts the bar alone in the first column, beside the five series, and
// the series are not pushed into the bar or its tick labels.
#[test]
fn a_bar_item_occupies_one_column_of_a_multi_column_legend() {
    let (mut fx, ax, panel) = axes_over_key();
    let names = ["A", "B", "C", "D", "E"];
    let lines: Vec<NodeId> = names.iter().map(|n| fx.named_line(ax, n, |_| {})).collect();
    let scale = fx.color_scale(viridis());
    fx.legend_mut(panel).layout = fixed(2, LegendEntryOrder::ColumnMajor);
    let mut items = vec![LegendItem::color_scale(scale)];
    items.extend(lines.iter().map(|&l| LegendItem::artist(l)));
    explicit(&mut fx, panel, items);
    let scene = compile_figure(&fx.build());
    let bar = legend_bar(&scene, LegendOwner::Panel(panel));
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 5);
    let column = entries[0].rect.x;
    for e in &entries {
        assert_close(e.rect.x, column, SAME);
    }
    let rightmost_tick = tick_labels(&from_source(&leaves(&scene), panel), &bar)
        .iter()
        .map(|t| t.1.right())
        .fold(bar.bar.right(), f64::max);
    assert!(
        rightmost_tick <= column + EXACT,
        "the series lie right of the bar and its labels"
    );
    let pitch = entries[0].rect.height;
    assert!(
        bar.bar.y >= entries[0].rect.y - EXACT,
        "the bar starts on the first row"
    );
    assert!(
        bar.bar.bottom() <= entries[4].rect.bottom() + EXACT,
        "the bar ends on the fifth row"
    );
    assert!(bar.bar.height > 4.0 * pitch);
}

// Why: under a logarithmic norm a bar item is a logarithmic axis, as a colourbar is: decades lie at equal distances
// and are labelled as decades, and the colour at the middle of the bar is that of the geometric middle. As on a
// colourbar, the stride between labelled decades depends on the room, so the test asks for both ends and an even
// stride rather than for every decade. Four decades are spanned, so that a stride of one or of two reaches both ends:
// the five rows of a bar item have room for three labels, which over three decades would label 1 and 100 only.
#[test]
fn a_logarithmic_bar_item_places_decades_evenly_and_labels_them_as_decades() {
    let b = between(ColorScaleKind::Continuous(ContinuousColorScale {
        limits: manual(1.0, 10_000.0),
        norm: ColorScaleNorm::Log,
        ..ContinuousColorScale::default()
    }));
    let scene = compile_figure(&b.fx.build());
    let hit = legend_bar(&scene, LegendOwner::Panel(b.panel));
    let drawn = from_source(&leaves(&scene), b.panel);
    let math: Vec<(Rect, String, f64)> = glyph_runs(&drawn)
        .into_iter()
        .filter(|(_, g)| g.font == FontId::Math)
        .filter_map(|(l, g)| Some((l.bbox()?, g.text.clone(), g.size_pt)))
        .filter(|(b, _, _)| b.x >= hit.bar.right())
        .collect();
    let base_size = math.iter().map(|m| m.2).fold(0.0, f64::max);
    let mut bases: Vec<Rect> = math
        .iter()
        .filter(|m| m.2 == base_size && m.1 == "10")
        .map(|m| m.0)
        .collect();
    assert!(bases.len() >= 2, "decades are labelled as 10^k: {math:?}");
    // From the bottom up.
    bases.sort_by(|a, b| b.y.total_cmp(&a.y));
    let mut exponents: Vec<(f64, i32)> = math
        .iter()
        .filter(|m| m.2 < base_size)
        .filter_map(|m| Some((m.0.y, m.1.replace('\u{2212}', "-").parse().ok()?)))
        .collect();
    exponents.sort_by(|a, b| b.0.total_cmp(&a.0));
    let exponents: Vec<i32> = exponents.iter().map(|e| e.1).collect();
    assert_eq!(
        exponents.len(),
        bases.len(),
        "one exponent per base: {math:?}"
    );
    assert_eq!(
        exponents.first(),
        Some(&0),
        "1 is labelled 10^0: {exponents:?}"
    );
    assert_eq!(
        exponents.last(),
        Some(&4),
        "10 000 is labelled 10^4: {exponents:?}"
    );
    let stride = exponents[1] - exponents[0];
    assert!(
        stride > 0 && exponents.windows(2).all(|w| w[1] - w[0] == stride),
        "{exponents:?}"
    );
    for (b, k) in bases.iter().zip(&exponents) {
        assert_close(
            b.y + b.height / 2.0,
            along(&hit, f64::from(*k) / 4.0).y,
            b.height / 2.0,
        );
    }
    let middle = bar_paint(&drawn, &hit, 0.5);
    let expected = VIRIDIS[128];
    assert!(
        middle.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 3),
        "the middle of the bar, at 100: {middle:?}, expected {expected:?}"
    );
}

// Why: inside a plot area a bar item is drawn the same way, within the legend's box, so that its tick labels read
// over the plot, and the artist entry beside it still toggles its artist.
#[test]
fn a_bar_item_is_drawn_inside_the_box_of_an_inside_legend() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let line = fx.named_line(ax, "Curve", |_| {});
    let scale = fx.color_scale(viridis());
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        location: LegendLocation::NorthWest,
        entries: LegendEntries::Explicit(vec![
            LegendItem::artist(line),
            LegendItem::color_scale(scale),
        ]),
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    let hit = legend_bar(&scene, LegendOwner::Axes(ax));
    let plot = axes_hit(&scene, ax).plot_rect;
    assert!(inside(hit.bar, plot, EXACT));
    let drawn = from_source(&leaves(&scene), ax);
    let backdrop = filled_rects(&drawn)
        .into_iter()
        .filter(|(r, c)| rgb8(*c) == [255, 255, 255] && inside(hit.bar, *r, EXACT))
        .map(|(r, _)| r)
        .next()
        .expect("the legend's box lies behind the bar");
    for (_, b, text) in tick_labels(&drawn, &hit) {
        assert!(
            inside(b, backdrop, EXACT),
            "{text} lies in the legend's box"
        );
    }
    assert_eq!(toggles(&scene, centre(label(&scene, "Curve"))), vec![line]);
    assert_eq!(toggles(&scene, centre(hit.bar)), Vec::<NodeId>::new());
}

// Why: an item on a row of its own spans the width of the legend, and a bar is read along its length, so a bar item
// on a row of its own lies across the legend, as long as the entries beside and below it are wide, with its ticks
// below it, as a lying colourbar of the scale is drawn; the entries after it keep clear of its labels, and it is no
// click target.
#[test]
fn a_bar_item_on_a_row_of_its_own_lies_across_the_width_of_the_legend() {
    let (mut fx, ax, panel) = axes_over_key();
    let lines: Vec<NodeId> = [
        "Alpha series",
        "Beta series",
        "Gamma series",
        "Delta series",
    ]
    .iter()
    .map(|n| fx.named_line(ax, n, |_| {}))
    .collect();
    let scale = fx.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
        limits: manual(0.0, 10.0),
        ticks: Ticks::Values(vec![0.0, 5.0, 10.0]),
        ..ContinuousColorScale::default()
    }));
    fx.legend_mut(panel).layout = fixed(2, LegendEntryOrder::RowMajor);
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::artist(lines[0]),
            LegendItem::artist(lines[1]),
            LegendItem {
                full_row: true,
                ..LegendItem::color_scale(scale)
            },
            LegendItem::artist(lines[2]),
            LegendItem::artist(lines[3]),
        ],
    );
    let scene = compile_figure(&fx.build());
    let bar = legend_bar(&scene, LegendOwner::Panel(panel));
    assert_eq!(bar.orientation, ColorbarOrientation::Horizontal);
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 4, "only the series are click targets");
    let left = entries
        .iter()
        .map(|e| e.rect.x)
        .fold(f64::INFINITY, f64::min);
    let right = entries
        .iter()
        .map(|e| e.rect.right())
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        bar.bar.x >= left - EXACT && bar.bar.right() <= right + EXACT,
        "the bar lies within the width of the legend: {:?} within {left}..{right}",
        bar.bar
    );
    assert!(
        bar.bar.width > 0.8 * (right - left),
        "the bar spans most of the width of the legend: {:?} of {}",
        bar.bar,
        right - left
    );
    assert!(bar.bar.width > 3.0 * bar.bar.height, "the bar lies flat");
    let ticks: Vec<(f64, Rect)> = numeric(&from_source(&leaves(&scene), panel))
        .into_iter()
        .map(|(v, b, _)| (v, b))
        .collect();
    let mut values: Vec<f64> = ticks.iter().map(|t| t.0).collect();
    values.sort_by(f64::total_cmp);
    assert_eq!(values, [0.0, 5.0, 10.0], "{ticks:?}");
    for (v, b) in &ticks {
        assert!(
            b.y >= bar.bar.bottom(),
            "the tick label {v} lies below the bar"
        );
        // The labels at the ends are kept within the legend, as those of a colourbar inside a plot area are kept
        // within its box; the label of the middle tick is centred on its place.
        assert!(
            b.x >= left - EXACT && b.right() <= right + EXACT,
            "{v} lies within the legend"
        );
        if *v == 5.0 {
            assert_close(b.x + b.width / 2.0, bar.bar.x + bar.bar.width / 2.0, SAME);
        }
        assert!(
            label(&scene, "Gamma series").y >= b.bottom() - EXACT,
            "the entries after the bar keep clear of its labels"
        );
    }
    assert!(label(&scene, "Alpha series").bottom() <= bar.bar.y + EXACT);
    assert_eq!(toggles(&scene, centre(bar.bar)), Vec::<NodeId>::new());
}

// Why: a fitted column exists so that a key is never sized by hand; a legend panel that holds a bar item alone in a
// fitted column must take the width of the bar and its tick labels, so that nothing of the key overflows.
#[test]
fn a_fitted_column_holds_a_bar_item_and_its_tick_labels() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    fx.fig.layout.col_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit];
    fx.axes2d(0, 0);
    let scale = fx.color_scale(viridis());
    fx.scale_mut(scale).title = Some(Text::plain("Speed"));
    let panel = fx.panel(cell(0, 1, 1, 1));
    explicit(&mut fx, panel, vec![LegendItem::color_scale(scale)]);
    let scene = compile_figure(&fx.build());
    assert!(
        !overflows(&scene, LegendOwner::Panel(panel)),
        "{:?}",
        scene.warnings
    );
    let area = panel_rect(&scene, panel);
    let hit = legend_bar(&scene, LegendOwner::Panel(panel));
    assert!(inside(hit.bar, area, EXACT));
    for (_, b, text) in tick_labels(&from_source(&leaves(&scene), panel), &hit) {
        assert!(
            inside(b, area, 0.01),
            "{text} lies in the panel: {b:?} in {area:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Clicking and automatic legends
// ---------------------------------------------------------------------------------------------------------------

// Why: clicking a legend entry hides or shows the artists it stands for; a scale is not an artist, so neither a bar
// item nor a category entry is a click target, while an artist entry beside them still toggles its artist.
#[test]
fn scale_items_toggle_nothing_and_artist_entries_still_toggle_their_artists() {
    let (mut fx, ax, panel) = axes_over_key();
    let line = fx.named_line(ax, "Curve", |_| {});
    let speed = fx.color_scale(viridis());
    let status = fx.color_scale(categories(&[(RED, "taken")]));
    explicit(
        &mut fx,
        panel,
        vec![
            LegendItem::color_scale(speed),
            LegendItem::color_scale(status),
            LegendItem::artist(line),
        ],
    );
    let scene = compile_figure(&fx.build());
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].targets, vec![(ax, line)]);
    let bar = legend_bar(&scene, LegendOwner::Panel(panel));
    assert_eq!(toggles(&scene, centre(bar.bar)), Vec::<NodeId>::new());
    assert_eq!(
        toggles(&scene, centre(label(&scene, "taken"))),
        Vec::<NodeId>::new()
    );
    assert_eq!(toggles(&scene, centre(label(&scene, "Curve"))), vec![line]);
}

// Why: an automatic legend lists the labelled artists, as it always has; a scale enters a legend only when the author
// lists it, so that adding a colour-mapped plot never adds a key the author did not ask for.
#[test]
fn automatic_legends_list_artists_and_never_scales() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 3;
    let ax = fx.axes2d(0, 0);
    let other = fx.axes2d(1, 0);
    let status = fx.color_scale(categories(&[(RED, "taken"), (GREEN, "skipped")]));
    let speed = fx.color_scale(viridis());
    let codes = fx.scalar_image(
        ax,
        vec![1, 2],
        vec![0.0, 1.0],
        Some(status),
        xy(None, None),
        |i| i.label = text("Status"),
    );
    fx.scalar_image(
        other,
        vec![1, 2],
        vec![0.0, 10.0],
        Some(speed),
        xy(None, None),
        |i| i.label = text("Speed"),
    );
    fx.ax(ax).legend = Some(Legend::default());
    let panel = fx.panel(cell(2, 0, 1, 1));
    fx.legend_in(other, panel);
    let scene = compile_figure(&fx.build());
    let inside_entries = crate::probe::axes_legend_entries(&scene, ax);
    assert_eq!(inside_entries.len(), 1);
    assert_eq!(inside_entries[0].targets, vec![(ax, codes)]);
    assert_eq!(label_count(&scene, "taken"), 0);
    assert_eq!(label_count(&scene, "skipped"), 0);
    assert!(legend_bars(&scene, LegendOwner::Axes(ax)).is_empty());
    assert!(legend_bars(&scene, LegendOwner::Panel(panel)).is_empty());
    assert_eq!(panel_entries(&scene, panel).len(), 1);
}

// ---------------------------------------------------------------------------------------------------------------
// The sample of a colour-mapped artist is a sweep of its scale
// ---------------------------------------------------------------------------------------------------------------

// Why: the middle colour of a scale (the stopgap this replaces) tells the reader nothing about what the colours of the
// plot mean; a sweep shows the whole scale, from its low end at the left to its high end at the right as a lying
// colourbar does, so the sample of a colour-mapped image shows more than one colour, and those are the scale's ends.
#[test]
fn the_sample_of_a_colour_mapped_image_is_a_gradient_from_the_low_to_the_high_end() {
    let (scene, panel) = swept(|fx, ax| {
        let scale = fx.color_scale(red_to_blue());
        fx.scalar_image(
            ax,
            vec![1, 2],
            vec![0.4, 0.6],
            Some(scale),
            xy(None, None),
            |i| i.label = text("Mapped"),
        );
    });
    let left = sweep_fill(&scene, panel, "Mapped", 0.08);
    let right = sweep_fill(&scene, panel, "Mapped", 0.92);
    assert_ne!(
        rgb8(left),
        rgb8(right),
        "the sample shows more than one colour"
    );
    assert_nearer(
        left,
        rgb8_of(RED),
        rgb8_of(BLUE),
        "the left end of the sweep",
    );
    assert_nearer(
        right,
        rgb8_of(BLUE),
        rgb8_of(RED),
        "the right end of the sweep",
    );
}

// Why: a reversed colormap reverses the colours of the sweep, never the direction of the values: the low end of the
// scale stays at the left, so the sweep starts with the last colour of the colormap.
#[test]
fn the_sweep_of_a_reversed_colormap_starts_with_its_last_colour() {
    let (scene, panel) = swept(|fx, ax| {
        let scale = fx.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
            colormap: Colormap::Named {
                name: ColormapName::Viridis,
                reverse: true,
            },
            limits: manual(0.0, 10.0),
            ..ContinuousColorScale::default()
        }));
        fx.scalar_image(
            ax,
            vec![1, 2],
            vec![0.0, 10.0],
            Some(scale),
            xy(None, None),
            |i| i.label = text("Mapped"),
        );
    });
    assert_nearer(
        sweep_fill(&scene, panel, "Mapped", 0.05),
        VIRIDIS[255],
        VIRIDIS[0],
        "the left end",
    );
    assert_nearer(
        sweep_fill(&scene, panel, "Mapped", 0.95),
        VIRIDIS[0],
        VIRIDIS[255],
        "the right end",
    );
}

// Why: a discrete scale is drawn in bands everywhere (ADR 0024), so its sweep is a row of adjoining bands, one per
// band of the scale, each in its single colour, from the first band at the left; a gradient would suggest colours the
// plot never shows.
#[test]
fn the_sweep_of_a_discrete_scale_is_a_row_of_its_bands() {
    let (scene, panel) = swept(|fx, ax| {
        let scale = fx.color_scale(red_to_blue_bands(&[0.0, 1.0, 2.0, 3.0]));
        fx.scalar_image(
            ax,
            vec![1, 2],
            vec![0.5, 2.5],
            Some(scale),
            xy(None, None),
            |i| i.label = text("Mapped"),
        );
    });
    let colour = |f: f64| rgb8(sweep_fill(&scene, panel, "Mapped", f));
    let (first, middle, last) = (colour(1.0 / 6.0), colour(0.5), colour(5.0 / 6.0));
    assert!(
        first != middle && middle != last && first != last,
        "one colour per band: {first:?} {middle:?} {last:?}"
    );
    assert_eq!(
        colour(0.04),
        first,
        "the first band is one colour across its width"
    );
    assert_eq!(
        colour(0.30),
        first,
        "the first band is one colour across its width"
    );
    assert_eq!(
        colour(0.96),
        last,
        "the last band is one colour across its width"
    );
    assert_eq!(
        first,
        rgb8_of(RED),
        "the first band takes the low end of the colormap"
    );
    assert_eq!(
        last,
        rgb8_of(BLUE),
        "the last band takes the high end of the colormap"
    );
}

// Why: an indexed or categorical scale has no order of values between its entries, so its sweep is its entry
// colours side by side, in entry order from the left, each over an equal share of the sample.
#[test]
fn the_sweep_of_an_indexed_or_categorical_scale_is_its_entry_colours_side_by_side() {
    for kind in [
        palette(vec![RED, GREEN, BLUE]),
        categories(&[(RED, "r"), (GREEN, "g"), (BLUE, "b")]),
    ] {
        let (scene, panel) = swept(|fx, ax| {
            let scale = fx.color_scale(kind);
            fx.scalar_image(
                ax,
                vec![1, 3],
                vec![0.0, 1.0, 2.0],
                Some(scale),
                xy(None, None),
                |i| i.label = text("Mapped"),
            );
        });
        for (i, colour) in [RED, GREEN, BLUE].into_iter().enumerate() {
            let f = (i as f64 + 0.5) / 3.0;
            assert_eq!(
                rgb8(sweep_fill(&scene, panel, "Mapped", f)),
                rgb8_of(colour),
                "entry {i}"
            );
        }
    }
}

// Why: every colour-mapped artist keys its scale the same way, so a filled contour and a colour-mapped surface sweep
// their scale, and the isolines of an unfilled colour-mapped contour draw the sweep along the line of their sample.
#[test]
fn contours_and_surfaces_sweep_their_scales() {
    let (scene, panel) = swept(|fx, ax| {
        let scale = fx.color_scale(red_to_blue());
        fx.contour(
            ax,
            &GRID_X,
            &GRID_Y,
            |x, y| (x + y) / 3.0,
            |c| {
                c.label = text("Bands");
                c.fill = true;
                c.color_scale = Some(scale);
            },
        );
        fx.contour(
            ax,
            &GRID_X,
            &GRID_Y,
            |x, y| (x + y) / 3.0,
            |c| {
                c.label = text("Isolines");
                c.fill = false;
                c.line.color = ColorSpec::Colormapped;
                c.color_scale = Some(scale);
            },
        );
        fx.surface(
            ax,
            &GRID_X,
            &GRID_Y,
            |x, y| (x + y) / 3.0,
            |s| {
                s.label = text("Surface");
                s.color_scale = Some(scale);
            },
        );
    });
    for name in ["Bands", "Surface"] {
        assert_nearer(
            sweep_fill(&scene, panel, name, 0.1),
            rgb8_of(RED),
            rgb8_of(BLUE),
            name,
        );
        assert_nearer(
            sweep_fill(&scene, panel, name, 0.9),
            rgb8_of(BLUE),
            rgb8_of(RED),
            name,
        );
    }
    assert_nearer(
        sweep_stroke(&scene, panel, "Isolines", 0.1),
        rgb8_of(RED),
        rgb8_of(BLUE),
        "isolines",
    );
    assert_nearer(
        sweep_stroke(&scene, panel, "Isolines", 0.9),
        rgb8_of(BLUE),
        rgb8_of(RED),
        "isolines",
    );
}

// Why: a series coloured by data draws its line in the colours of its scale, so its sample draws the line as a sweep
// of the scale; a marker that follows the colour data sits at the centre of the sample, in the sweep's colour there.
#[test]
fn a_series_coloured_per_point_sweeps_its_line_and_colours_its_marker_where_it_sits() {
    let (scene, panel) = swept(|fx, ax| {
        let scale = fx.color_scale(red_to_blue());
        fx.series(ax, &[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0], None, |s, fx| {
            s.label = text("Mapped");
            s.color_data = Some(fx.vector(&[0.0, 0.5, 1.0]));
            s.color_scale = Some(scale);
            s.line.color = ColorSpec::Colormapped;
            s.marker.shape = MarkerShape::Circle;
        });
    });
    assert_nearer(
        sweep_stroke(&scene, panel, "Mapped", 0.08),
        rgb8_of(RED),
        rgb8_of(BLUE),
        "the left of the line",
    );
    assert_nearer(
        sweep_stroke(&scene, panel, "Mapped", 0.92),
        rgb8_of(BLUE),
        rgb8_of(RED),
        "the right of the line",
    );
    let markers: Vec<_> = sample_beside(&scene, panel, "Mapped")
        .iter()
        .flat_map(Leaf::instances)
        .collect();
    assert_eq!(markers.len(), 1, "one marker at the centre: {markers:?}");
    // The default marker is open, drawn by its edge alone, so the colour it is drawn in is its edge's.
    let face = rgb8(
        markers[0]
            .face
            .or(markers[0].edge)
            .expect("the marker is drawn in a colour"),
    );
    assert!(
        face[0].abs_diff(face[2]) <= 26 && face[0] > 77 && face[2] > 77,
        "the marker takes the middle of the sweep, halfway from red to blue: {face:?}"
    );
}

// Why: the sample keys the scale, not the data: a series coloured per segment whose values cover only the middle of
// its scale still sweeps the whole scale, so its sample reads like the colourbar of the same scale.
#[test]
fn a_series_coloured_per_segment_sweeps_its_whole_scale_and_not_its_data() {
    let (scene, panel) = swept(|fx, ax| {
        let scale = fx.color_scale(red_to_blue());
        fx.series(ax, &[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0], None, |s, fx| {
            s.label = text("Mapped");
            s.color_data = Some(fx.vector(&[0.4, 0.6]));
            s.color_scale = Some(scale);
            s.line.color = ColorSpec::Colormapped;
        });
    });
    let near_low = [
        (0.6f32 * 255.0).round() as u8,
        0,
        (0.4f32 * 255.0).round() as u8,
    ];
    let left = sweep_stroke(&scene, panel, "Mapped", 0.05);
    assert_nearer(
        left,
        rgb8_of(RED),
        near_low,
        "the left end is the scale's low end, not the data's",
    );
    assert_nearer(
        sweep_stroke(&scene, panel, "Mapped", 0.95),
        rgb8_of(BLUE),
        rgb8_of(RED),
        "the right end",
    );
}

// Why: a series that draws only markers coloured by data has no line to sweep, so its sample is a row of markers
// across the sample, each in the sweep's colour at its place, which shows the scale as the markers show the data.
#[test]
fn a_series_of_markers_coloured_by_data_draws_a_row_of_markers_in_the_sweep() {
    let (scene, panel) = swept(|fx, ax| {
        let scale = fx.color_scale(red_to_blue());
        fx.series(ax, &[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0], None, |s, fx| {
            s.label = text("Mapped");
            s.color_data = Some(fx.vector(&[0.0, 0.5, 1.0]));
            s.color_scale = Some(scale);
            s.line.dash = DashStyle::None;
            s.marker.shape = MarkerShape::Circle;
            s.marker.face = ColorSpec::Colormapped;
            s.marker.edge = ColorSpec::Colormapped;
        });
    });
    let mut markers: Vec<_> = sample_beside(&scene, panel, "Mapped")
        .iter()
        .flat_map(Leaf::instances)
        .collect();
    assert!(markers.len() >= 2, "a row of markers: {markers:?}");
    markers.sort_by(|a, b| a.position.x.total_cmp(&b.position.x));
    for m in &markers {
        assert_close(m.position.y, markers[0].position.y, EXACT);
    }
    let first = markers.first().unwrap().face.expect("filled");
    let last = markers.last().unwrap().face.expect("filled");
    assert_nearer(first, rgb8_of(RED), rgb8_of(BLUE), "the leftmost marker");
    assert_nearer(last, rgb8_of(BLUE), rgb8_of(RED), "the rightmost marker");
}

// Why: ADR 0021: the sample of an artist takes its opacity, one number that fades the whole artist, but not its
// alpha, which varies over the artist and has no single value to show; a sweep is no exception.
#[test]
fn a_sweep_takes_the_artist_s_opacity_but_not_its_alpha() {
    let (scene, panel) = swept(|fx, ax| {
        let scale = fx.color_scale(red_to_blue());
        let alpha = fx.matrix(1, 2, vec![0.2, 0.2]);
        fx.scalar_image(
            ax,
            vec![1, 2],
            vec![0.0, 1.0],
            Some(scale),
            xy(None, None),
            |i| {
                i.label = text("Image");
                i.opacity = 0.5;
                i.alpha = Some(alpha);
            },
        );
        let face_alpha = fx.matrix(2, 3, vec![0.1; 6]);
        fx.surface(
            ax,
            &GRID_X,
            &GRID_Y,
            |x, y| (x + y) / 3.0,
            |s| {
                s.label = text("Surface");
                s.color_scale = Some(scale);
                s.face_opacity = 0.4;
                s.face_alpha = Some(face_alpha);
            },
        );
    });
    for (name, opacity) in [("Image", 0.5), ("Surface", 0.4)] {
        for f in [0.1, 0.5, 0.9] {
            let a = sweep_fill(&scene, panel, name, f).a;
            assert!(
                (f64::from(a) - opacity).abs() <= 0.01,
                "{name} at {f}: alpha {a}, expected the opacity {opacity}"
            );
        }
    }
}

// Why: the sweep is the look of a colour-mapped entry, so a legend panel merges two plots of the same name that sweep
// the same scale, as it merges lines of the same colour, and lists two plots of the same name that sweep different
// scales separately, warning that the reader cannot tell them apart by name.
#[test]
fn a_panel_merges_colour_mapped_entries_exactly_when_they_sweep_the_same_colours() {
    let build = |shared: bool| {
        let mut fx = Fx::new();
        fx.fig.layout.rows = 3;
        let top = fx.axes2d(0, 0);
        let bottom = fx.axes2d(1, 0);
        let panel = fx.panel(cell(2, 0, 1, 1));
        let first = fx.color_scale(red_to_blue());
        let second = if shared {
            first
        } else {
            fx.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
                colormap: Colormap::Listed {
                    colors: vec![BLUE, RED],
                },
                limits: manual(0.0, 1.0),
                ..ContinuousColorScale::default()
            }))
        };
        let a = fx.scalar_image(
            top,
            vec![1, 2],
            vec![0.0, 1.0],
            Some(first),
            xy(None, None),
            |i| i.label = text("Speed"),
        );
        let b = fx.scalar_image(
            bottom,
            vec![1, 2],
            vec![0.0, 1.0],
            Some(second),
            xy(None, None),
            |i| i.label = text("Speed"),
        );
        fx.legend_in(top, panel);
        fx.legend_in(bottom, panel);
        (compile_figure(&fx.build()), panel, (top, a), (bottom, b))
    };

    let (scene, panel, a, b) = build(true);
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 1, "one merged entry: {entries:?}");
    assert_eq!(entries[0].targets, vec![a, b]);

    let (scene, panel, _, _) = build(false);
    assert_eq!(panel_entries(&scene, panel).len(), 2);
    assert!(scene.warnings.iter().any(|w| matches!(
        &w.kind,
        SceneWarningKind::LegendNameClash { legend, name } if *legend == LegendOwner::Panel(panel) && name == "Speed"
    )));
}
