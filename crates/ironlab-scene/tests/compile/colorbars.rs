//! Colourbars (#8, #88): a colour scale drawn as a bar in a panel of its own or inside an axes along one edge of its
//! plot area. These tests own what the compiler draws for a colourbar: the bar of each kind of scale, its ticks and
//! title, its out-of-range keys, its length along the tile layout, its thickness, and its place inside an axes.
//!
//! Expected colours are worked out from the public 256-entry tables and the arithmetic of the plan, not from the
//! compiler's own mapping, as in the colour scale tests. Positions are asserted as relationships (a bar as long as the
//! plot boxes it spans, equal margins, an order along the bar, a tick at the place of its value) rather than as
//! numbers of points, so that the tests survive a change of style constants.
//!
//! The scene API these tests assume, beyond the IR of `crates/ironlab-ir/tests/colorbars.rs`:
//!
//! ```text
//! // crates/ironlab-scene/src/hit.rs
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! pub enum ColorbarOwner { Panel(NodeId), Axes(NodeId) }
//!
//! #[derive(Clone, Debug, PartialEq)]
//! pub struct ColorbarHit {
//!     pub owner: ColorbarOwner,
//!     pub scale: NodeId,
//!     /// Vertical or horizontal; an inside colourbar is horizontal on the south and north edges and
//!     /// vertical on the east and west edges.
//!     pub orientation: ironlab_ir::ColorbarOrientation,
//!     /// The bar, in figure space: for a continuous, discrete or indexed scale the coloured bar itself, without
//!     /// ticks, labels, title or keys; for a categorical scale the union of its category squares.
//!     pub bar: Rect,
//!     /// The squares of a categorical scale, one per category in category order; empty for any other kind.
//!     pub entries: Vec<Rect>,
//!     /// The out-of-range keys drawn, each with the category it shows (`ironlab_ir::ColorScaleKey`).
//!     pub keys: Vec<ColorbarKeyHit>,
//!     /// The box behind an inside colourbar, enclosing all of it; `None` for a panel colourbar.
//!     pub backdrop: Option<Rect>,
//! }
//!
//! #[derive(Clone, Copy, Debug, PartialEq)]
//! pub struct ColorbarKeyHit { pub key: ColorScaleKey, pub rect: Rect }
//!
//! pub struct HitMap { ..., pub colorbars: Vec<ColorbarHit> }
//! ```
//!
//! Every display item of a colourbar names its owner as its source: the panel for a panel colourbar, the axes for
//! an inside colourbar. A continuous or indexed bar is one `ImageItem` whose samples run along the bar, row 0 at the
//! top of its rectangle in its own coordinates as for every image item; a discrete bar is one filled axis-aligned
//! rectangle per band; a categorical scale is one filled square per category with a black outline; a key is a
//! square, filled or not, with an outline. Default key labels are plain text, one glyph run each.
//!
//! The rules these tests pin, beyond those the plan states:
//!
//! - **Low values at the bottom or the left, always.** A reversed colormap reverses the colours along the bar, never
//!   the direction of the values.
//! - **Panel bars keep the span.** A panel colourbar is exactly as long as its cell span inset by the aligned margins
//!   of the spanned rows or columns, which is the area the hit map publishes for the panel, except that a fixed
//!   aspect that shrinks the plot boxes within their areas shortens the bar to run from the near edge of the first
//!   drawn box to the far edge of the last among the axes in those rows or columns. Out-of-range keys, when
//!   shown, fit inside that length: the bar shrinks to make room for them. These tests give a horizontal panel bar
//!   the full length (`length: Some(1.0)`) wherever they are about the span, because its automatic length is half
//!   the span; the automatic lengths and shorter bars belong to `colorbar_layout.rs`. A title placed `Above` is a
//!   heading above the bar, so above a vertical panel bar it lies in the margin above the panel's area, as an axes
//!   title lies above its plot box, and adds to the top margin of the first row it spans, which keeps the bar as long
//!   as the plots. The default placement, along the bar, belongs to `colorbar_layout.rs`.
//! - **Categorical order.** The squares of a categorical scale are read like a list: category 0 at the top of a
//!   vertical key and at the left of a horizontal one, then its out-of-range keys after the last category.
//! - **Inside bars.** An inside bar is boxed as a boxed inside legend is (`ColorbarHit::backdrop`, a white filled
//!   rectangle with a frame, drawn over the artists and beneath the bar). The box encloses the title, the bar, the
//!   keys and every label; it is `length` × the edge long, centred along the edge, and inset from the edge by one
//!   margin that is the same on every side. The bar together with its keys is centred in the box, and the bar shrinks
//!   to make room for the keys.
//! - **Unused scales.** A bar of a scale that no artist names takes the automatic limits that stage 2 gives a scale
//!   over no value, `[0, 1]`, or `[1, 10]` under a logarithmic norm, and raises no warning: a custom colourbar is a
//!   normal figure.
//! - **Unusable references.** A colourbar whose scale reference names no scale, which validation refuses, is not
//!   drawn, and the compiler warns against its owner rather than failing.

use ironlab_ir::{
    Aspect, CategoricalColorScale, Cell, Color, ColorCategory, ColorScaleKey, ColorScaleKind,
    ColorScaleLevels, ColorScaleNorm, ColorScaleOutOfRange, ColorbarOrientation,
    ColorbarTitlePlacement, Colormap, ColormapName, ContinuousColorScale, DiscreteColorScale,
    FigureSize, IndexedColorScale, InsideColorbar, InsideColorbarSide, Limits, NodeId, OutOfRange,
    Panel, PanelColorbar, PanelContent, PanelLocation, Text, TickLabels, Ticks, TitleAlign,
    TrackSize, View3d,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{Point, Rect, Rgba};
use ironlab_scene::hit::{AxesHitKind, ColorbarHit, ColorbarOwner, LegendOwner};
use ironlab_scene::maths::colormap::VIRIDIS;
use ironlab_text::FontId;

use crate::common::{Fx, cell, compile_figure, out_of_range, palette_colors, rgb8, rgb8_close, xy};
use crate::probe::{
    Leaf, assert_close, axes_hit, from_source, glyph_runs, leaves, panel_rect, parse_number, pixel,
    union,
};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const GREY: Color = Color::rgb(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0);

/// The default base font size of a figure, in points.
const FONT_SIZE: f64 = 9.0;

/// Geometry that the compiler computes exactly, such as a bar whose ends are those of a plot box.
const EXACT: f64 = 1e-6;

// ---------------------------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------------------------

/// A continuous scale of viridis with the given limits and the defaults of its kind otherwise.
fn viridis(limits: Limits) -> ColorScaleKind {
    ColorScaleKind::Continuous(ContinuousColorScale {
        limits,
        ..ContinuousColorScale::default()
    })
}

/// Manual limits.
fn manual(min: f64, max: f64) -> Limits {
    Limits::Manual { min, max }
}

/// A discrete scale of viridis with the given band edges and the defaults of its kind otherwise.
fn bands(edges: &[f64]) -> ColorScaleKind {
    ColorScaleKind::Discrete(DiscreteColorScale {
        levels: ColorScaleLevels::Explicit {
            values: edges.to_vec(),
        },
        ..DiscreteColorScale::default()
    })
}

/// An indexed scale of the given colours, strict outside them, with the defaults of its kind otherwise.
fn palette(colors: Vec<Color>) -> ColorScaleKind {
    ColorScaleKind::Indexed(IndexedColorScale {
        colors,
        ..IndexedColorScale::default()
    })
}

/// A categorical scale of the given colours and plain labels, strict outside them.
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

/// Adds a colourbar panel of `scale` in `cell` with the given orientation, centred, and returns its identifier.
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

/// Returns the continuous scale with the given identifier for modification.
fn continuous_mut(fx: &mut Fx, id: NodeId) -> &mut ContinuousColorScale {
    match &mut fx.scale_mut(id).kind {
        ColorScaleKind::Continuous(scale) => scale,
        other => panic!("the scale is continuous, found {other:?}"),
    }
}

/// A figure of `rows` 2D axes stacked in the first column, each holding an image of the values 0 to 10 through one
/// continuous scale of viridis with automatic limits, and a vertical colourbar panel of that scale spanning every
/// row of a second column of the given size (#88). Returns the figure, the axes, the scale and the panel.
fn column_beside_stack(rows: u32, column: TrackSize) -> (Fx, Vec<NodeId>, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = rows;
    fx.fig.layout.cols = 2;
    fx.fig.layout.col_weights = vec![TrackSize::Weight(1.0), column];
    let scale = fx.color_scale(viridis(Limits::Auto));
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

/// A figure of three 2D axes in a row over a fitted row holding a horizontal colourbar panel spanning all three
/// columns, of one continuous scale with manual limits [0, 10] that every axes' image names. The bar is given the
/// full length of the span, which a horizontal bar does not take by default. Returns the figure, the axes, the scale
/// and the panel.
fn row_over_fitted_bar() -> (Fx, Vec<NodeId>, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 3;
    fx.fig.layout.row_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit];
    let scale = fx.color_scale(viridis(manual(0.0, 10.0)));
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
    bar_mut(&mut fx, panel).length = Some(1.0);
    (fx, axes, scale, panel)
}

/// A figure of one 2D axes beside a fitted column holding a vertical colourbar panel of a scale of the given kind
/// that no artist names, so that the bar shows the scale alone. Returns the figure, the axes, the scale and the panel.
fn lone_bar(kind: ColorScaleKind) -> (Fx, NodeId, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    fx.fig.layout.col_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit];
    let ax = fx.axes2d(0, 0);
    let scale = fx.color_scale(kind);
    let panel = colorbar_panel(
        &mut fx,
        cell(0, 1, 1, 1),
        scale,
        ColorbarOrientation::Vertical,
    );
    (fx, ax, scale, panel)
}

/// As [`lone_bar`], horizontal, in a fitted row under the axes.
fn lone_horizontal_bar(kind: ColorScaleKind) -> (Fx, NodeId, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.row_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit];
    let ax = fx.axes2d(0, 0);
    let scale = fx.color_scale(kind);
    let panel = colorbar_panel(
        &mut fx,
        cell(1, 0, 1, 1),
        scale,
        ColorbarOrientation::Horizontal,
    );
    (fx, ax, scale, panel)
}

/// A figure of one 2D axes holding an image of the values 0 to 10 through a continuous scale of viridis with
/// manual limits [0, 10], and an inside colourbar of that scale on the given side with the given length. Returns
/// the figure, the axes, the image and the scale.
fn inside_bar(side: InsideColorbarSide, length: f64) -> (Fx, NodeId, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scale = fx.color_scale(viridis(manual(0.0, 10.0)));
    let image = fx.scalar_image(
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
        length,
        ..InsideColorbar::default()
    });
    (fx, ax, image, scale)
}

// ---------------------------------------------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------------------------------------------

/// Returns the hit geometry of the colourbar of an owner.
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

/// Returns the leaves that a colourbar's owner drew, in paint order.
fn owned(scene: &Scene, owner: ColorbarOwner) -> Vec<Leaf> {
    let id = match owner {
        ColorbarOwner::Panel(id)
        | ColorbarOwner::Axes(id)
        | ColorbarOwner::Legend(LegendOwner::Panel(id) | LegendOwner::Axes(id)) => id,
    };
    from_source(&leaves(scene), id)
}

/// Returns whether two rectangles agree to within `tol` points on every edge.
fn rect_close(a: Rect, b: Rect, tol: f64) -> bool {
    (a.x - b.x).abs() <= tol
        && (a.y - b.y).abs() <= tol
        && (a.right() - b.right()).abs() <= tol
        && (a.bottom() - b.bottom()).abs() <= tol
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

/// Returns every stroked axis-aligned rectangle among the leaves, with its stroke colour.
fn stroked_rects(leaves: &[Leaf]) -> Vec<(Rect, Rgba)> {
    leaves
        .iter()
        .filter(|l| is_rectangle(l))
        .filter_map(|l| Some((l.bbox()?, l.path()?.stroke.as_ref()?.color)))
        .collect()
}

/// Returns the fill of the filled rectangle drawn at `rect`, if any.
fn fill_at(leaves: &[Leaf], rect: Rect) -> Option<Rgba> {
    filled_rects(leaves)
        .into_iter()
        .find(|(r, _)| rect_close(*r, rect, 0.01))
        .map(|(_, c)| c)
}

/// Returns whether a black outline is drawn around `rect`, allowing for the half width of the stroke.
fn outlined_in_black(leaves: &[Leaf], rect: Rect) -> bool {
    stroked_rects(leaves)
        .into_iter()
        .any(|(r, c)| rect_close(r, rect, 1.0) && rgb8(c) == [0, 0, 0])
}

/// Returns whether any filled shape covers the centre of `rect`, so that the square is not empty.
fn anything_filled_inside(leaves: &[Leaf], rect: Rect) -> bool {
    let centre = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    filled_rects(leaves).into_iter().any(|(r, _)| {
        r.x <= centre.x && centre.x <= r.right() && r.y <= centre.y && centre.y <= r.bottom()
    }) || leaves.iter().any(|l| {
        l.image().is_some()
            && l.bbox().is_some_and(|b| {
                b.x < centre.x && centre.x < b.right() && b.y < centre.y && centre.y < b.bottom()
            })
    })
}

/// Returns the image leaf that draws the bar of a colourbar: the one image the owner drew over exactly the bar.
#[track_caller]
fn bar_image(leaves: &[Leaf], bar: Rect) -> Leaf {
    let found: Vec<&Leaf> = leaves
        .iter()
        .filter(|l| l.image().is_some() && l.bbox().is_some_and(|b| rect_close(b, bar, 0.01)))
        .collect();
    assert_eq!(found.len(), 1, "one image covers the bar {bar:?} exactly");
    found[0].clone()
}

/// Returns the point on the centre line of a bar at the fraction `f` of its length from its low end: the bottom of a
/// vertical bar and the left of a horizontal one.
fn along(hit: &ColorbarHit, f: f64) -> Point {
    let b = hit.bar;
    match hit.orientation {
        ColorbarOrientation::Vertical => Point::new(b.x + b.width / 2.0, b.bottom() - f * b.height),
        ColorbarOrientation::Horizontal => Point::new(b.x + f * b.width, b.y + b.height / 2.0),
    }
}

/// Returns the sample of an image leaf under a point in figure space, as four 8-bit channels.
#[track_caller]
fn sample_at(leaf: &Leaf, p: Point) -> [u8; 4] {
    let image = leaf.image().expect("an image leaf");
    let local = leaf
        .transform
        .inverse()
        .expect("an invertible transform")
        .apply(p);
    let fx = (local.x - image.rect.x) / image.rect.width;
    let fy = (local.y - image.rect.y) / image.rect.height;
    let column = ((fx * f64::from(image.width)).floor() as usize).min(image.width as usize - 1);
    let row = ((fy * f64::from(image.height)).floor() as usize).min(image.height as usize - 1);
    pixel(image, row, column)
}

/// Returns the number of samples an image leaf of a bar holds along the bar: its larger dimension, whichever way the
/// implementation lays the samples out before transforming them onto the bar.
fn samples_along(leaf: &Leaf) -> u32 {
    let image = leaf.image().expect("an image leaf");
    image.height.max(image.width)
}

/// Returns the colour of the bar at the fraction `f` of its length from its low end.
fn bar_colour(leaves: &[Leaf], hit: &ColorbarHit, f: f64) -> [u8; 3] {
    let px = sample_at(&bar_image(leaves, hit.bar), along(hit, f));
    [px[0], px[1], px[2]]
}

/// The 8-bit colour of an IR colour.
fn rgb8_of(color: Color) -> [u8; 3] {
    [color.r, color.g, color.b].map(|c| (c * 255.0).round() as u8)
}

/// The 8-bit colour of the viridis table entry at the normalised position `t`.
fn viridis_at(t: f64) -> [u8; 3] {
    VIRIDIS[((t * 256.0).floor() as usize).min(255)]
}

/// Asserts that two 8-bit colours are within `slack` levels per channel, naming what is compared.
#[track_caller]
fn assert_rgb(actual: [u8; 3], expected: [u8; 3], slack: u8, what: &str) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, e)| a.abs_diff(e) <= slack),
        "{what}: {actual:?}, expected {expected:?}"
    );
}

/// A label of a colourbar: its text and its bounding box in figure space.
#[derive(Clone, Debug)]
struct Label {
    text: String,
    bbox: Rect,
}

/// Returns every glyph run among the leaves, with its text and bounding box.
fn labels(leaves: &[Leaf]) -> Vec<Label> {
    glyph_runs(leaves)
        .into_iter()
        .filter_map(|(l, g)| {
            Some(Label {
                text: g.text.clone(),
                bbox: l.bbox()?,
            })
        })
        .collect()
}

/// Returns the plain numeric labels among the leaves (math runs, such as decades, are left out), with their values.
fn numeric(leaves: &[Leaf]) -> Vec<(f64, Rect, String)> {
    glyph_runs(leaves)
        .into_iter()
        .filter(|(_, g)| g.font != FontId::Math)
        .filter_map(|(l, g)| Some((parse_number(&g.text)?, l.bbox()?, g.text.clone())))
        .collect()
}

/// Returns the bounding box of the glyph run whose text is exactly `text`.
#[track_caller]
fn label_box(leaves: &[Leaf], text: &str) -> Rect {
    let found: Vec<Label> = labels(leaves)
        .into_iter()
        .filter(|l| l.text == text)
        .collect();
    assert_eq!(found.len(), 1, "one label {text:?}: {:?}", labels(leaves));
    found[0].bbox
}

/// Returns the coordinate along the bar of the centre of a box: its centre y for a vertical bar, x for a
/// horizontal one.
fn centre_along(orientation: ColorbarOrientation, b: Rect) -> f64 {
    match orientation {
        ColorbarOrientation::Vertical => b.y + b.height / 2.0,
        ColorbarOrientation::Horizontal => b.x + b.width / 2.0,
    }
}

/// Returns half the extent of a box along a bar, which is how far a label centred on its tick may lie from it.
fn half_size_along(orientation: ColorbarOrientation, b: Rect) -> f64 {
    match orientation {
        ColorbarOrientation::Vertical => b.height / 2.0,
        ColorbarOrientation::Horizontal => b.width / 2.0,
    }
}

/// Returns the coordinate along a bar of the value `v` on a linear bar from `min` to `max`.
fn place(hit: &ColorbarHit, v: f64, min: f64, max: f64) -> f64 {
    let f = (v - min) / (max - min);
    let p = along(hit, f);
    match hit.orientation {
        ColorbarOrientation::Vertical => p.y,
        ColorbarOrientation::Horizontal => p.x,
    }
}

/// Returns the thickness of a bar: its extent across its length.
fn thickness(hit: &ColorbarHit) -> f64 {
    match hit.orientation {
        ColorbarOrientation::Vertical => hit.bar.width,
        ColorbarOrientation::Horizontal => hit.bar.height,
    }
}

/// Returns the length of a bar.
fn length(hit: &ColorbarHit) -> f64 {
    match hit.orientation {
        ColorbarOrientation::Vertical => hit.bar.height,
        ColorbarOrientation::Horizontal => hit.bar.width,
    }
}

/// Returns the rectangle of the key of the given category, if one is drawn.
fn key_rect(hit: &ColorbarHit, key: ColorScaleKey) -> Option<Rect> {
    hit.keys.iter().find(|k| k.key == key).map(|k| k.rect)
}

/// Returns the warnings that name `id`.
fn warnings_naming(scene: &Scene, id: NodeId) -> Vec<String> {
    scene
        .warnings
        .iter()
        .filter(|w| w.node == Some(id))
        .map(|w| w.message.clone())
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// A panel colourbar along the tile layout (#88)
// ---------------------------------------------------------------------------------------------------------------

// Why: this is #88. A bar shared by a column of plots must run exactly from the top of the upper plot box to the
// bottom of the lower one, so that the reader can line values up with the plots; any other length looks like a
// mistake and hides that the bar belongs to both.
#[test]
fn a_vertical_bar_spanning_two_rows_is_as_long_as_the_span_of_their_plot_boxes() {
    let (mut fx, axes, scale, panel) = column_beside_stack(2, TrackSize::Fit);
    // Decorations that make the aligned margins of the rows differ from those of a bare axes, so that a bar inset by
    // any fixed margin, or by the margins of the wrong rows, misses the plot boxes.
    fx.ax(axes[0]).title = Some(Text::plain("Upper"));
    fx.ax(axes[1]).x.label = Some(Text::plain("Time (s)"));
    let scene = compile_figure(&fx.build());
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel));
    assert_eq!(bar.scale, scale);
    assert_eq!(bar.orientation, ColorbarOrientation::Vertical);
    let top = axes_hit(&scene, axes[0]).plot_rect;
    let bottom = axes_hit(&scene, axes[1]).plot_rect;
    assert_close(bar.bar.y, top.y, EXACT);
    assert_close(bar.bar.bottom(), bottom.bottom(), EXACT);
    // The panel's area is that span, so the bar fills its area along the track.
    let area = panel_rect(&scene, panel);
    assert_close(bar.bar.y, area.y, EXACT);
    assert_close(bar.bar.bottom(), area.bottom(), EXACT);
}

// Why: the same rule along a row: a bar under a row of three plots is as long as the span from the left of the
// first plot box to the right of the last, the layout matplotlib users reach for with `fig.colorbar(ax=axs)`.
#[test]
fn a_horizontal_bar_under_a_row_of_three_axes_is_as_long_as_the_span_of_their_plot_boxes() {
    let (mut fx, axes, _, panel) = row_over_fitted_bar();
    // A y label widens the left margin of the first column only, so the span is not the row inset by equal margins.
    fx.ax(axes[0]).y.label = Some(Text::plain("Depth (m)"));
    let scene = compile_figure(&fx.build());
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel));
    assert_eq!(bar.orientation, ColorbarOrientation::Horizontal);
    let left = axes_hit(&scene, axes[0]).plot_rect;
    let right = axes_hit(&scene, axes[2]).plot_rect;
    assert_close(bar.bar.x, left.x, EXACT);
    assert_close(bar.bar.right(), right.right(), EXACT);
    // The bar lies below the plots, not over them.
    assert!(bar.bar.y > left.bottom());
}

// Why: a bar fits its length to the plot boxes of the axes in its columns, but an axes that spans more columns than
// the bar's panel must not stretch the bar over the neighbouring panel: the bar stays within its own cell span, as
// matplotlib's `fig.colorbar(ax=...)` beside a legend keeps the two apart.
#[test]
fn a_bar_under_an_axes_spanning_more_columns_stays_within_its_own_columns() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    fx.fig.layout.row_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit];
    let scale = fx.color_scale(viridis(manual(0.0, 10.0)));
    let ax = fx.axes2d(0, 0);
    fx.ax(ax).cell = cell(0, 0, 1, 2);
    fx.scalar_image(
        ax,
        vec![1, 2],
        vec![0.0, 10.0],
        Some(scale),
        xy(None, None),
        |_| {},
    );
    let legend = fx.panel(cell(1, 0, 1, 1));
    fx.legend_in(ax, legend);
    let panel = colorbar_panel(
        &mut fx,
        cell(1, 1, 1, 1),
        scale,
        ColorbarOrientation::Horizontal,
    );
    // The full length, so that the bar would reach the end of the plot box if nothing held it within its columns.
    bar_mut(&mut fx, panel).length = Some(1.0);
    let scene = compile_figure(&fx.build());
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel)).bar;
    let area = panel_rect(&scene, panel);
    let legend_area = panel_rect(&scene, legend);
    let plot = axes_hit(&scene, ax).plot_rect;
    // The bar lies within its own cell span, inset as the panel's area is, and ends where the plot box ends.
    assert!(
        bar.x >= area.x - EXACT,
        "bar {bar:?} starts left of its area {area:?}"
    );
    assert!(
        bar.right() <= area.right() + EXACT,
        "bar {bar:?} ends right of its area {area:?}"
    );
    assert_close(bar.right(), plot.right(), EXACT);
    // It does not reach over the legend panel beside it.
    assert!(
        bar.x >= legend_area.right() - EXACT,
        "bar {bar:?} overlaps the legend panel {legend_area:?}"
    );
}

/// A figure of `count` 2D axes of equal aspect holding a line from `(0, 0)` to `(dx, dy)`, laid along a row over a
/// fitted row holding a horizontal colourbar (`vertical` false), or stacked in a column beside a fitted column holding
/// a vertical one, on a page of the given size in millimetres. Either bar takes the full length of the span. Returns
/// the figure, the axes and the panel.
fn equal_aspect_beside_bar(
    count: u32,
    vertical: bool,
    (dx, dy): (f64, f64),
    (width_mm, height_mm): (f64, f64),
) -> (Fx, Vec<NodeId>, NodeId) {
    let mut fx = Fx::new();
    fx.fig.size = FigureSize {
        width_mm,
        height_mm,
    };
    let scale = fx.color_scale(viridis(manual(0.0, 10.0)));
    let tracks = vec![TrackSize::Weight(1.0), TrackSize::Fit];
    let axes: Vec<NodeId> = if vertical {
        fx.fig.layout.rows = count;
        fx.fig.layout.cols = 2;
        fx.fig.layout.col_weights = tracks;
        (0..count).map(|row| fx.axes2d(row, 0)).collect()
    } else {
        fx.fig.layout.rows = 2;
        fx.fig.layout.cols = count;
        fx.fig.layout.row_weights = tracks;
        (0..count).map(|col| fx.axes2d(0, col)).collect()
    };
    for &ax in &axes {
        fx.line(ax, &[0.0, dx], &[0.0, dy], None, |_| {});
        fx.ax(ax).aspect = Aspect::Equal;
    }
    let (cell, orientation) = if vertical {
        (cell(0, 1, count, 1), ColorbarOrientation::Vertical)
    } else {
        (cell(1, 0, 1, count), ColorbarOrientation::Horizontal)
    };
    let panel = colorbar_panel(&mut fx, cell, scale, orientation);
    bar_mut(&mut fx, panel).length = Some(1.0);
    (fx, axes, panel)
}

// Why: a fixed aspect shrinks a plot box within its plot area, and a bar that kept the length of the areas would
// then overhang the plots it belongs to, which reads as a mistake. The bar follows the boxes as drawn: under a row of
// tall, narrow equal-aspect plots on a tall page it runs from the left of the first box to the right of the last.
#[test]
fn a_horizontal_bar_under_equal_aspect_plots_spans_their_drawn_boxes() {
    let (fx, axes, panel) = equal_aspect_beside_bar(3, false, (1.0, 10.0), (160.0, 200.0));
    let scene = compile_figure(&fx.build());
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel));
    let left = axes_hit(&scene, axes[0]).plot_rect;
    let right = axes_hit(&scene, axes[2]).plot_rect;
    // The aspect did shrink the boxes, or the test would not tell the boxes from the areas.
    assert!(right.right() - left.x < panel_rect(&scene, panel).width - 1.0);
    assert_close(bar.bar.x, left.x, EXACT);
    assert_close(bar.bar.right(), right.right(), EXACT);
    assert!(bar.bar.y > left.bottom());
}

// Why: the same rule along a column: beside a stack of wide, short equal-aspect plots on a wide page, a vertical bar
// runs from the top of the upper box to the bottom of the lower one.
#[test]
fn a_vertical_bar_beside_equal_aspect_plots_spans_their_drawn_boxes() {
    let (fx, axes, panel) = equal_aspect_beside_bar(2, true, (10.0, 1.0), (200.0, 120.0));
    let scene = compile_figure(&fx.build());
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel));
    let top = axes_hit(&scene, axes[0]).plot_rect;
    let bottom = axes_hit(&scene, axes[1]).plot_rect;
    assert!(bottom.bottom() - top.y < panel_rect(&scene, panel).height - 1.0);
    assert_close(bar.bar.y, top.y, EXACT);
    assert_close(bar.bar.bottom(), bottom.bottom(), EXACT);
    assert!(bar.bar.x > top.right());
}

// Why: the bar is a key, not a plot, so its thickness is a constant of the style, fixed by the font size and not by
// the space its cell happens to have; a bar in a wide weighted column must not swell to fill it, and every bar of a
// figure, in a panel or inside an axes, must be equally thick.
#[test]
fn the_thickness_of_a_bar_follows_the_font_size_and_not_the_space_of_its_cell() {
    let thick = |column: TrackSize, font_size: f64| {
        let (mut fx, axes, scale, panel) = column_beside_stack(1, column);
        fx.fig.font_size_pt = font_size;
        fx.ax(axes[0]).colorbar = Some(InsideColorbar {
            scale,
            side: InsideColorbarSide::South,
            length: 0.8,
            ..InsideColorbar::default()
        });
        let scene = compile_figure(&fx.build());
        let in_panel = thickness(&bar_of(&scene, ColorbarOwner::Panel(panel)));
        let inside = thickness(&bar_of(&scene, ColorbarOwner::Axes(axes[0])));
        assert_close(inside, in_panel, EXACT);
        in_panel
    };
    let fitted = thick(TrackSize::Fit, FONT_SIZE);
    assert!(fitted > 0.0);
    assert_close(thick(TrackSize::Weight(1.0), FONT_SIZE), fitted, EXACT);
    assert_close(thick(TrackSize::Fit, 2.0 * FONT_SIZE), 2.0 * fitted, EXACT);
}

// Why: a vertical bar is read with its values on the right and a horizontal one with its values below, as
// matplotlib and MATLAB draw them; a label on the other side would collide with the plots the bar sits beside.
#[test]
fn a_panel_bar_has_its_tick_labels_on_the_right_or_below() {
    let (fx, _, _, panel) = column_beside_stack(1, TrackSize::Fit);
    let scene = compile_figure(&fx.build());
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel));
    let ticks = numeric(&owned(&scene, ColorbarOwner::Panel(panel)));
    assert!(ticks.len() >= 2, "{ticks:?}");
    for (_, b, text) in &ticks {
        assert!(b.x >= bar.bar.right(), "{text} lies right of the bar");
    }

    let (fx, _, _, panel) = row_over_fitted_bar();
    let scene = compile_figure(&fx.build());
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel));
    let ticks = numeric(&owned(&scene, ColorbarOwner::Panel(panel)));
    assert!(ticks.len() >= 2, "{ticks:?}");
    for (_, b, text) in &ticks {
        assert!(b.y >= bar.bar.bottom(), "{text} lies below the bar");
    }
}

// Why: a fitted track exists so that a key is never sized by hand; a bar whose tick labels are longer needs a wider
// column, which must grow by the extra width of the labels and take it from the plots, and every label must lie
// inside the panel so that none is clipped.
#[test]
fn longer_tick_labels_widen_a_fitted_column_by_their_extra_width() {
    let measure = |first: &str| {
        let (mut fx, axes, scale, panel) = column_beside_stack(1, TrackSize::Fit);
        let c = continuous_mut(&mut fx, scale);
        c.limits = manual(0.0, 10.0);
        c.ticks = Ticks::Values(vec![0.0, 10.0]);
        c.tick_labels = TickLabels::Text(vec![Text::plain(first), Text::plain("b")]);
        let scene = compile_figure(&fx.build());
        let area = panel_rect(&scene, panel);
        let drawn = owned(&scene, ColorbarOwner::Panel(panel));
        for label in labels(&drawn) {
            assert!(
                label.bbox.x >= area.x - EXACT && label.bbox.right() <= area.right() + 0.5,
                "{} lies inside the column {area:?}: {:?}",
                label.text,
                label.bbox
            );
        }
        (
            area.width,
            label_box(&drawn, first).width,
            axes_hit(&scene, axes[0]).plot_rect.width,
        )
    };
    let (short_area, short_label, short_plot) = measure("a");
    let (long_area, long_label, long_plot) = measure("a much longer label");
    assert!(long_label > short_label + FONT_SIZE);
    assert_close(long_area - short_area, long_label - short_label, 1.0);
    assert_close(short_plot - long_plot, long_area - short_area, EXACT);
}

// Why: in a fitted row a horizontal bar takes the height of the bar, its ticks and its labels, so nothing of the key
// is clipped and no empty band is left under the plots.
#[test]
fn a_fitted_row_holds_a_horizontal_bar_and_its_labels_without_slack() {
    let (fx, _, _, panel) = row_over_fitted_bar();
    let scene = compile_figure(&fx.build());
    let area = panel_rect(&scene, panel);
    let bar = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    let lowest = labels(&drawn)
        .iter()
        .map(|l| l.bbox.bottom())
        .fold(bar.bar.bottom(), f64::max);
    assert!(
        bar.bar.y >= area.y - EXACT && lowest <= area.bottom() + 0.5,
        "{area:?}"
    );
    assert!(
        area.height - (lowest - bar.bar.y) < FONT_SIZE,
        "slack of {} pt",
        area.height - (lowest - bar.bar.y)
    );
}

// Why: across its track the bar is placed by the panel's alignment, as a legend is; moving it from west to east must
// move it across the column without changing its length, which the plots fix.
#[test]
fn a_panel_bar_is_placed_across_its_track_by_the_alignment_of_the_panel() {
    let place_with = |align: PanelLocation| {
        let (mut fx, _, _, panel) = column_beside_stack(1, TrackSize::Weight(1.0));
        fx.panel_mut(panel).align = align;
        let scene = compile_figure(&fx.build());
        bar_of(&scene, ColorbarOwner::Panel(panel)).bar
    };
    let west = place_with(PanelLocation::West);
    let centre = place_with(PanelLocation::Center);
    let east = place_with(PanelLocation::East);
    assert!(
        west.x < centre.x && centre.x < east.x,
        "{west:?} {centre:?} {east:?}"
    );
    assert_close(west.height, east.height, EXACT);
    assert_close(west.y, east.y, EXACT);
}

// Why: the title names what the colours mean; placed `Above`, it is a heading (ADR 0023): drawn above the bar, over
// it, whether the bar stands or lies. Above a vertical bar that spans the plot boxes it lies in the margin above them,
// as an axes title does, and that margin grows to hold it, so the bar keeps the span of the plots and the title stays
// on the page.
#[test]
fn the_scale_title_placed_above_is_a_heading_above_the_bar() {
    for vertical in [true, false] {
        let (mut fx, axes, scale, panel) = if vertical {
            column_beside_stack(2, TrackSize::Fit)
        } else {
            row_over_fitted_bar()
        };
        bar_mut(&mut fx, panel).title_placement = ColorbarTitlePlacement::Above;
        let untitled = compile_figure(&fx.fig.clone());
        fx.scale_mut(scale).title = Some(Text::plain("Speed (m/s)"));
        let scene = compile_figure(&fx.build());
        let bar = bar_of(&scene, ColorbarOwner::Panel(panel)).bar;
        let title = label_box(&owned(&scene, ColorbarOwner::Panel(panel)), "Speed (m/s)");
        assert!(
            title.bottom() <= bar.y + EXACT,
            "vertical {vertical}: {title:?} above {bar:?}"
        );
        assert!(
            title.x < bar.right() && title.right() > bar.x,
            "vertical {vertical}: the title {title:?} stands over the bar {bar:?}"
        );
        let top = axes_hit(&scene, axes[0]).plot_rect;
        if vertical {
            // The heading lies in the margin above the first row, which grows to hold it, and the bar still spans the
            // plot boxes exactly.
            assert!(
                title.bottom() <= top.y + EXACT,
                "{title:?} above the plots {top:?}"
            );
            assert!(title.y >= 0.0, "{title:?} lies on the page");
            assert!(
                top.y > axes_hit(&untitled, axes[0]).plot_rect.y,
                "the top margin of the first row grows to hold the title"
            );
            let bottom = axes_hit(&scene, axes[1]).plot_rect;
            assert_close(bar.y, top.y, EXACT);
            assert_close(bar.bottom(), bottom.bottom(), EXACT);
        } else {
            // Under the plots the heading lies in the fitted row with the bar, between the bar and the plots, which it
            // leaves where they were.
            let area = panel_rect(&scene, panel);
            assert!(title.y >= area.y - EXACT, "{title:?} within {area:?}");
            assert_close(top.y, axes_hit(&untitled, axes[0]).plot_rect.y, EXACT);
        }
        assert!(
            warnings_naming(&scene, panel).is_empty(),
            "{:?}",
            scene.warnings
        );
    }
}

// Why: a heading wider than the bar and its labels must still fit its fitted column, so the column widens to hold it,
// rather than the title spilling over the plots beside it.
#[test]
fn a_title_above_wider_than_the_bar_widens_its_fitted_column_to_hold_it() {
    let (mut fx, _, scale, panel) = column_beside_stack(1, TrackSize::Fit);
    bar_mut(&mut fx, panel).title_placement = ColorbarTitlePlacement::Above;
    let narrow = panel_rect(&compile_figure(&fx.fig.clone()), panel).width;
    fx.scale_mut(scale).title = Some(Text::plain("Velocity magnitude in the wake (m/s)"));
    let scene = compile_figure(&fx.build());
    let area = panel_rect(&scene, panel);
    let title = label_box(
        &owned(&scene, ColorbarOwner::Panel(panel)),
        "Velocity magnitude in the wake (m/s)",
    );
    assert!(area.width > narrow);
    assert!(
        title.x >= area.x - 0.5 && title.right() <= area.right() + 0.5,
        "{title:?} within {area:?}"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// What each kind of scale draws
// ---------------------------------------------------------------------------------------------------------------

// Why: a continuous bar is the scale itself, so its colour at each place must be the colour the scale gives the value
// there, smoothly enough that no band shows: at least 256 samples along the bar, in a rectangle with flat ends.
#[test]
fn a_continuous_bar_is_an_image_of_the_scale_from_its_lower_to_its_upper_limit() {
    let (fx, _, _, panel) = lone_bar(viridis(manual(0.0, 10.0)));
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    let image = bar_image(&drawn, hit.bar);
    assert!(samples_along(&image) >= 256);
    for f in [0.01, 0.25, 0.5, 0.75, 0.99] {
        assert_rgb(
            bar_colour(&drawn, &hit, f),
            viridis_at(f),
            3,
            &format!("at {f} of the bar"),
        );
    }
    assert!(hit.keys.is_empty(), "keys are off by default");
    assert!(hit.entries.is_empty());
}

// Why: a reader looks for low values at the bottom (or the left) of every bar; reversing the colormap reverses the
// colours along the bar, never the direction of the values, which the ticks still show increasing upwards.
#[test]
fn a_reversed_colormap_reverses_the_colours_but_low_values_stay_at_the_bottom_and_left() {
    for vertical in [true, false] {
        let kind = ColorScaleKind::Continuous(ContinuousColorScale {
            colormap: Colormap::Named {
                name: ColormapName::Viridis,
                reverse: true,
            },
            limits: manual(0.0, 10.0),
            ..ContinuousColorScale::default()
        });
        let (fx, _, _, panel) = if vertical {
            lone_bar(kind)
        } else {
            lone_horizontal_bar(kind)
        };
        let scene = compile_figure(&fx.build());
        let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
        let drawn = owned(&scene, ColorbarOwner::Panel(panel));
        assert_rgb(
            bar_colour(&drawn, &hit, 0.005),
            VIRIDIS[255],
            3,
            "the low end",
        );
        assert_rgb(
            bar_colour(&drawn, &hit, 0.995),
            VIRIDIS[0],
            3,
            "the high end",
        );
        let ticks = numeric(&drawn);
        let lowest = ticks.iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
        let highest = ticks.iter().max_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
        let (lo, hi) = (
            centre_along(hit.orientation, lowest.1),
            centre_along(hit.orientation, highest.1),
        );
        if vertical {
            assert!(lo > hi, "the lowest value is labelled below the highest");
        } else {
            assert!(lo < hi, "the lowest value is labelled left of the highest");
        }
    }
}

// Why: the bar is a true axis of the values, so every tick label must sit at the place of its value along the bar;
// automatic ticks are nice values like an axis's, and include both limits when the limits are round.
#[test]
fn continuous_tick_labels_sit_at_the_places_of_their_values() {
    let (fx, _, _, panel) = lone_bar(viridis(manual(0.0, 10.0)));
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let ticks = numeric(&owned(&scene, ColorbarOwner::Panel(panel)));
    assert!(ticks.len() >= 3, "{ticks:?}");
    let values: Vec<f64> = ticks.iter().map(|t| t.0).collect();
    assert!(
        values.contains(&0.0) && values.contains(&10.0),
        "{values:?}"
    );
    for (v, b, text) in &ticks {
        assert_close(
            centre_along(hit.orientation, *b),
            place(&hit, *v, 0.0, 10.0),
            half_size_along(hit.orientation, *b),
        );
        assert!((0.0..=10.0).contains(v), "{text} lies within the limits");
    }
}

// Why: a reader matches a tick label to the bar by its tick mark, which must leave the bar's edge on the label side
// at the place of the value; a mark elsewhere points at the wrong colour.
#[test]
fn each_tick_mark_leaves_the_bar_on_its_label_side_at_the_place_of_its_value() {
    let (mut fx, _, scale, panel) = lone_bar(viridis(manual(0.0, 10.0)));
    continuous_mut(&mut fx, scale).ticks = Ticks::Values(vec![0.0, 2.5, 10.0]);
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    let segments: Vec<(Point, Point)> = drawn.iter().flat_map(Leaf::line_segments).collect();
    for v in [0.0, 2.5, 10.0] {
        let y = place(&hit, v, 0.0, 10.0);
        let mark = segments.iter().find(|(a, b)| {
            (a.y - y).abs() < 0.5
                && (b.y - y).abs() < 0.5
                && a.x.min(b.x) >= hit.bar.right() - 0.5
                && a.x.min(b.x) <= hit.bar.right() + 0.5
                && a.x.max(b.x) > hit.bar.right() + 0.5
                && (a.x - b.x).abs() < FONT_SIZE
        });
        assert!(
            mark.is_some(),
            "a tick mark at {v} on the right edge of the bar: {segments:?}"
        );
    }
}

// Why: under a logarithmic norm the bar is a logarithmic axis, so decades lie at equal distances and are labelled as
// decades, exactly as on a logarithmic axis; and the colour at the middle of the bar is the colour of the geometric
// middle of the limits.
#[test]
fn a_log_bar_places_decades_evenly_and_labels_them_as_decades() {
    let kind = ColorScaleKind::Continuous(ContinuousColorScale {
        limits: manual(1.0, 1000.0),
        norm: ColorScaleNorm::Log,
        ..ContinuousColorScale::default()
    });
    let (fx, _, _, panel) = lone_bar(kind);
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    let math: Vec<(Rect, String, f64)> = glyph_runs(&drawn)
        .into_iter()
        .filter(|(_, g)| g.font == FontId::Math)
        .filter_map(|(l, g)| Some((l.bbox()?, g.text.clone(), g.size_pt)))
        .collect();
    let base_size = math.iter().map(|m| m.2).fold(0.0, f64::max);
    let mut bases: Vec<Rect> = math
        .iter()
        .filter(|m| m.2 == base_size && m.1 == "10")
        .map(|m| m.0)
        .collect();
    // As on a logarithmic axis, the stride between labelled decades depends on the room, so the test asks for at
    // least the two ends and an even stride rather than for every decade.
    assert!(bases.len() >= 2, "decades are labelled as 10^k: {math:?}");
    bases.sort_by(|a, b| b.y.total_cmp(&a.y));
    let mut exponents: Vec<(f64, i32)> = math
        .iter()
        .filter(|m| m.2 < base_size)
        .filter_map(|m| Some((m.0.y, m.1.replace('\u{2212}', "-").parse().ok()?)))
        .collect();
    // From the bottom up.
    exponents.sort_by(|a, b| b.0.total_cmp(&a.0));
    let exponents: Vec<i32> = exponents.iter().map(|e| e.1).collect();
    assert_eq!(
        exponents.len(),
        bases.len(),
        "one exponent per base: {math:?}"
    );
    assert_eq!(exponents[0], 0, "1 is labelled 10^0: {exponents:?}");
    assert_eq!(
        *exponents.last().unwrap(),
        3,
        "1000 is labelled 10^3: {exponents:?}"
    );
    let stride = exponents[1] - exponents[0];
    assert!(
        stride > 0 && exponents.windows(2).all(|w| w[1] - w[0] == stride),
        "{exponents:?}"
    );
    // Decade k of three lies k thirds of the way along the bar, so the decades are evenly spaced.
    for (b, k) in bases.iter().zip(&exponents) {
        assert_close(
            centre_along(hit.orientation, *b),
            along(&hit, f64::from(*k) / 3.0).y,
            b.height / 2.0,
        );
    }
    assert_rgb(
        bar_colour(&drawn, &hit, 0.5),
        viridis_at(0.5),
        3,
        "the middle, at about 31.6",
    );
}

// Why: a discrete bar is a true axis too, so each band must be as long as the range of values it holds: with uneven
// edges 0, 1, 5 and 10 the bands are a tenth, four tenths and half of the bar, adjoining, each in the one colour the
// scale gives its values, and the automatic ticks mark every edge.
#[test]
fn a_discrete_bar_draws_uneven_bands_in_proportion_to_their_values() {
    let (fx, _, _, panel) = lone_bar(bands(&[0.0, 1.0, 5.0, 10.0]));
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    let mut inside: Vec<(Rect, Rgba)> = filled_rects(&drawn)
        .into_iter()
        .filter(|(r, _)| {
            r.x >= hit.bar.x - EXACT
                && r.right() <= hit.bar.right() + EXACT
                && r.y >= hit.bar.y - EXACT
                && r.bottom() <= hit.bar.bottom() + EXACT
        })
        .collect();
    assert_eq!(inside.len(), 3, "one rectangle per band: {inside:?}");
    // From the bottom up.
    inside.sort_by(|a, b| b.0.y.total_cmp(&a.0.y));
    let len = length(&hit);
    for ((rect, colour), (share, t)) in inside.iter().zip([(0.1, 0.0), (0.4, 0.5), (0.5, 1.0)]) {
        assert_close(rect.height, share * len, EXACT);
        assert_close(rect.width, hit.bar.width, EXACT);
        assert!(
            rgb8_close(rgb8(*colour), viridis_at(t)),
            "band at {t}: {:?}",
            rgb8(*colour)
        );
    }
    assert_close(inside[0].0.bottom(), hit.bar.bottom(), EXACT);
    for pair in inside.windows(2) {
        assert_close(pair[0].0.y, pair[1].0.bottom(), EXACT);
    }
    assert_close(inside[2].0.y, hit.bar.y, EXACT);

    let ticks = numeric(&drawn);
    let mut values: Vec<f64> = ticks.iter().map(|t| t.0).collect();
    values.sort_by(f64::total_cmp);
    assert_eq!(values, [0.0, 1.0, 5.0, 10.0], "one tick per band edge");
    for (v, b, _) in &ticks {
        assert_close(
            centre_along(hit.orientation, *b),
            place(&hit, *v, 0.0, 10.0),
            half_size_along(hit.orientation, *b),
        );
    }
}

// Why: an indexed bar is the familiar bar of an indexed image: one block per entry, adjoining, each in its entry's
// colour, from entry 0 at the bottom.
#[test]
fn an_indexed_bar_draws_one_adjoining_block_per_entry_from_the_bottom() {
    let colors = vec![RED, GREEN, BLUE, GREY];
    let (fx, _, _, panel) = lone_bar(palette(colors.clone()));
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    for (i, color) in colors.iter().enumerate() {
        let f = (i as f64 + 0.5) / 4.0;
        assert_eq!(bar_colour(&drawn, &hit, f), rgb8_of(*color), "entry {i}");
        // The blocks adjoin: just inside each end of a block is still that block.
        assert_eq!(
            bar_colour(&drawn, &hit, (i as f64 + 0.02) / 4.0),
            rgb8_of(*color),
            "entry {i}, low end"
        );
        assert_eq!(
            bar_colour(&drawn, &hit, (i as f64 + 0.98) / 4.0),
            rgb8_of(*color),
            "entry {i}, high end"
        );
    }
}

// Why: an entry of a palette is labelled where its block is, so a tick sits at the centre of its entry's block, not
// at an edge between two entries; and an automatic tick never names a fraction, which is no entry at all.
#[test]
fn indexed_ticks_sit_at_entry_centres_and_automatic_ticks_are_whole_numbers() {
    let (mut fx, _, scale, panel) = lone_bar(palette(vec![RED, GREEN, BLUE, GREY]));
    let scene = compile_figure(&fx.fig.clone());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let ticks = numeric(&owned(&scene, ColorbarOwner::Panel(panel)));
    assert!(ticks.len() >= 2, "{ticks:?}");
    for (v, b, text) in &ticks {
        assert_eq!(v.fract(), 0.0, "{text} is a whole number");
        assert!((0.0..=3.0).contains(v), "{text} names an entry");
        assert_close(
            centre_along(hit.orientation, *b),
            along(&hit, (v + 0.5) / 4.0).y,
            half_size_along(hit.orientation, *b),
        );
    }

    // Labelled entries, as an indexed image's few named classes are.
    if let ColorScaleKind::Indexed(indexed) = &mut fx.scale_mut(scale).kind {
        indexed.ticks = Ticks::Values(vec![0.0, 2.0]);
        indexed.tick_labels = TickLabels::Text(vec![Text::plain("water"), Text::plain("ice")]);
    }
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    for (text, entry) in [("water", 0.0), ("ice", 2.0)] {
        let b = label_box(&drawn, text);
        assert_close(
            centre_along(hit.orientation, b),
            along(&hit, (entry + 0.5) / 4.0).y,
            b.height / 2.0,
        );
        assert!(b.x >= hit.bar.right(), "{text} lies on the label side");
    }
}

// Why: the 256-entry palette of an indexed image is the common case (#40); its automatic ticks must still be whole
// numbers within the palette, few enough to read.
#[test]
fn a_256_entry_palette_is_ticked_at_a_few_whole_numbers() {
    let (fx, _, _, panel) = lone_bar(palette(palette_colors(&VIRIDIS)));
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    assert!(samples_along(&bar_image(&drawn, hit.bar)) >= 256);
    let ticks = numeric(&drawn);
    assert!((3..=12).contains(&ticks.len()), "{ticks:?}");
    for (v, _, text) in &ticks {
        assert_eq!(v.fract(), 0.0, "{text}");
        assert!((0.0..=255.0).contains(v), "{text}");
    }
    assert_eq!(bar_colour(&drawn, &hit, 0.5 / 256.0), VIRIDIS[0]);
    assert_eq!(bar_colour(&drawn, &hit, 255.5 / 256.0), VIRIDIS[255]);
}

// Why: a palette of one entry is valid (stage 2), and its bar is one block of that colour labelled 0 at its centre;
// automatic ticks must not invent fractions to have more than one.
#[test]
fn a_one_entry_palette_is_one_block_labelled_zero_at_its_centre() {
    let (fx, _, _, panel) = lone_bar(palette(vec![GREEN]));
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    for f in [0.02, 0.5, 0.98] {
        assert_eq!(bar_colour(&drawn, &hit, f), rgb8_of(GREEN));
    }
    let ticks = numeric(&drawn);
    assert_eq!(
        ticks.iter().map(|t| t.2.as_str()).collect::<Vec<_>>(),
        ["0"]
    );
    assert_close(
        centre_along(hit.orientation, ticks[0].1),
        along(&hit, 0.5).y,
        ticks[0].1.height / 2.0,
    );
}

// Why: a categorical scale is a classification, not a range, so it is drawn as a list of separate swatches, never as
// adjoining blocks that suggest an order of values: each square outlined in black, separated by a margin, labelled
// beside it on the tick-label side, read from the top.
#[test]
fn a_categorical_scale_is_drawn_as_separate_outlined_squares_each_with_its_label() {
    let entries = [(RED, "alpha"), (GREEN, "beta"), (BLUE, "gamma")];
    let (fx, _, _, panel) = lone_bar(categories(&entries));
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    assert!(
        drawn.iter().all(|l| l.image().is_none()),
        "no bar of adjoining blocks"
    );
    assert_eq!(hit.entries.len(), 3);
    let side = hit.entries[0].width;
    let mut gaps = Vec::new();
    for (k, (square, (color, label))) in hit.entries.iter().zip(entries).enumerate() {
        assert_close(square.width, side, EXACT);
        assert_close(square.height, side, EXACT);
        assert_close(square.x, hit.entries[0].x, EXACT);
        let fill = fill_at(&drawn, *square).unwrap_or_else(|| panic!("category {k} is filled"));
        assert_eq!(rgb8(fill), rgb8_of(color), "category {k}");
        assert!(
            outlined_in_black(&drawn, *square),
            "category {k} has a black outline"
        );
        let b = label_box(&drawn, label);
        assert!(b.x >= square.right(), "{label} lies right of its square");
        assert_close(
            b.y + b.height / 2.0,
            square.y + square.height / 2.0,
            b.height / 2.0,
        );
        if k > 0 {
            gaps.push(square.y - hit.entries[k - 1].bottom());
        }
    }
    assert!(gaps[0] > 0.0, "the squares never adjoin");
    assert_close(gaps[1], gaps[0], EXACT);
}

// Why: lying under a row of plots, a categorical key reads from left to right with each label under its square, the
// side on which a horizontal bar carries its labels.
#[test]
fn a_horizontal_categorical_key_reads_from_left_to_right_with_labels_below() {
    let entries = [(RED, "alpha"), (GREEN, "beta"), (BLUE, "gamma")];
    let (fx, _, _, panel) = lone_horizontal_bar(categories(&entries));
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    for (k, (square, (_, label))) in hit.entries.iter().zip(entries).enumerate() {
        if k > 0 {
            assert!(
                square.x > hit.entries[k - 1].right(),
                "category {k} lies right of the one before"
            );
        }
        let b = label_box(&drawn, label);
        assert!(b.y >= square.bottom(), "{label} lies below its square");
        assert_close(
            b.x + b.width / 2.0,
            square.x + square.width / 2.0,
            b.width / 2.0,
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Out-of-range keys
// ---------------------------------------------------------------------------------------------------------------

/// A vertical bar of a continuous scale of viridis between [0, 1] in a panel spanning two rows of plots, whose above
/// category draws red, below category clamps, and non-finite category draws nothing, each with its key shown.
fn keyed_bar() -> (Fx, Vec<NodeId>, NodeId, NodeId) {
    let (mut fx, axes, scale, panel) = column_beside_stack(2, TrackSize::Fit);
    let c = continuous_mut(&mut fx, scale);
    c.limits = manual(0.0, 1.0);
    c.above = keyed(OutOfRange::Rgba { color: RED });
    c.below = keyed(OutOfRange::Clamp);
    fx.scale_mut(scale).non_finite = keyed(OutOfRange::Transparent);
    (fx, axes, scale, panel)
}

// Why: keys are separate squares beyond the ends of the bar, never caps, so that an out-of-range colour cannot be
// mistaken for the last colour in range; along a vertical bar they lie above key, bar, below key, non-finite key from
// the top, each as wide as the bar is thick and separated by one equal margin, and they fit within the span of the
// plots, so that the whole key still lines up with them.
#[test]
fn keys_of_a_vertical_bar_lie_above_and_below_it_with_equal_margins_within_the_span() {
    let (fx, axes, _, panel) = keyed_bar();
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let above = key_rect(&hit, ColorScaleKey::Above).expect("the above key is drawn");
    let below = key_rect(&hit, ColorScaleKey::Below).expect("the below key is drawn");
    let nan = key_rect(&hit, ColorScaleKey::NonFinite).expect("the non-finite key is drawn");
    assert!(key_rect(&hit, ColorScaleKey::Outside).is_none());
    let t = thickness(&hit);
    for (name, key) in [("above", above), ("below", below), ("non-finite", nan)] {
        assert!(
            (key.width - t).abs() <= EXACT && (key.height - t).abs() <= EXACT,
            "the {name} key is a square as wide as the bar is thick: {key:?}"
        );
        assert!(
            (key.x - hit.bar.x).abs() <= EXACT,
            "the {name} key lines up with the bar"
        );
    }
    let margin = hit.bar.y - above.bottom();
    assert!(margin > 0.0, "the above key is separate from the bar");
    assert_close(below.y - hit.bar.bottom(), margin, EXACT);
    assert_close(nan.y - below.bottom(), margin, EXACT);
    // The whole key, bar and squares, spans the plots exactly; the bar shrank to make room.
    let top = axes_hit(&scene, axes[0]).plot_rect;
    let bottom = axes_hit(&scene, axes[1]).plot_rect;
    assert_close(above.y, top.y, EXACT);
    assert_close(nan.bottom(), bottom.bottom(), EXACT);
    assert!(length(&hit) < bottom.bottom() - top.y);
}

// Why: along a horizontal bar the low end is the left, so the keys of values below the range, and of no value at
// all, lie beyond the left end, and the above key beyond the right, in that order and with one margin.
#[test]
fn keys_of_a_horizontal_bar_lie_non_finite_below_bar_above_from_the_left() {
    let (mut fx, axes, scale, panel) = row_over_fitted_bar();
    let c = continuous_mut(&mut fx, scale);
    c.above = keyed(OutOfRange::Clamp);
    c.below = keyed(OutOfRange::Clamp);
    fx.scale_mut(scale).non_finite = keyed(OutOfRange::Rgba { color: GREY });
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let nan = key_rect(&hit, ColorScaleKey::NonFinite).unwrap();
    let below = key_rect(&hit, ColorScaleKey::Below).unwrap();
    let above = key_rect(&hit, ColorScaleKey::Above).unwrap();
    let margin = below.x - nan.right();
    assert!(margin > 0.0);
    assert_close(hit.bar.x - below.right(), margin, EXACT);
    assert_close(above.x - hit.bar.right(), margin, EXACT);
    for key in [nan, below, above] {
        assert_close(key.y, hit.bar.y, EXACT);
        assert_close(key.height, thickness(&hit), EXACT);
    }
    // The keys fit within the span of the plots, as along a vertical bar.
    assert_close(nan.x, axes_hit(&scene, axes[0]).plot_rect.x, EXACT);
    assert_close(
        above.right(),
        axes_hit(&scene, axes[2]).plot_rect.right(),
        EXACT,
    );
    // The labels of the keys are below them, on the label side of the bar.
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    let b = label_box(&drawn, "NaN / Inf");
    assert!(b.y >= nan.bottom());
}

// Why: a key shows what is painted for its values: the fixed colour of a fixed-colour policy, the end colour of the
// scale under a clamp, and nothing under a transparent or strict policy, whose square is left empty with its outline
// so that the reader sees that such values are not drawn.
#[test]
fn each_policy_fills_its_key_with_what_is_painted() {
    for (policy, expected) in [
        (OutOfRange::Rgba { color: RED }, Some(rgb8_of(RED))),
        (OutOfRange::Clamp, Some(VIRIDIS[255])),
        (OutOfRange::Transparent, None),
        (OutOfRange::Strict, None),
    ] {
        let (mut fx, _, scale, panel) = lone_bar(viridis(manual(0.0, 1.0)));
        continuous_mut(&mut fx, scale).above = keyed(policy);
        let scene = compile_figure(&fx.build());
        let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
        let drawn = owned(&scene, ColorbarOwner::Panel(panel));
        let key = key_rect(&hit, ColorScaleKey::Above)
            .unwrap_or_else(|| panic!("{policy:?}: a key is drawn"));
        match expected {
            Some(colour) => {
                let fill =
                    fill_at(&drawn, key).unwrap_or_else(|| panic!("{policy:?}: the key is filled"));
                assert!(
                    rgb8_close(rgb8(fill), colour),
                    "{policy:?}: {:?}",
                    rgb8(fill)
                );
            }
            None => {
                assert!(
                    !anything_filled_inside(&drawn, key),
                    "{policy:?}: the key is empty"
                );
                assert!(
                    outlined_in_black(&drawn, key),
                    "{policy:?}: the empty key is outlined"
                );
            }
        }
    }
    // A clamp below takes the first colour.
    let (mut fx, _, scale, panel) = lone_bar(viridis(manual(0.0, 1.0)));
    continuous_mut(&mut fx, scale).below = keyed(OutOfRange::Clamp);
    let scene = compile_figure(&fx.build());
    let key = key_rect(
        &bar_of(&scene, ColorbarOwner::Panel(panel)),
        ColorScaleKey::Below,
    )
    .unwrap();
    let fill =
        fill_at(&owned(&scene, ColorbarOwner::Panel(panel)), key).expect("the below key is filled");
    assert!(rgb8_close(rgb8(fill), VIRIDIS[0]));
}

// Why: a key's default label states the range it stands for with a strict inequality (a value at a limit is inside),
// in the same format as the bar's ticks so that "> 1.0" matches the "1.0" at the end of the bar; and an author's
// label replaces the default entirely.
#[test]
fn default_key_labels_use_strict_inequalities_with_the_limits_formatted_as_ticks() {
    let (fx, _, scale, panel) = keyed_bar();
    let scene = compile_figure(&fx.fig.clone());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    let ticks = numeric(&drawn);
    let text_at = |v: f64| {
        ticks
            .iter()
            .find(|t| t.0 == v)
            .map(|t| t.2.clone())
            .unwrap_or_else(|| panic!("a tick at {v}: {ticks:?}"))
    };
    let expected: [(ColorScaleKey, String); 3] = [
        (ColorScaleKey::Above, format!("> {}", text_at(1.0))),
        (ColorScaleKey::Below, format!("< {}", text_at(0.0))),
        (ColorScaleKey::NonFinite, String::from("NaN / Inf")),
    ];
    for (key, text) in expected {
        let square = key_rect(&hit, key).unwrap();
        let b = label_box(&drawn, &text);
        assert!(b.x >= square.right(), "{text} lies right of its key");
        assert_close(
            b.y + b.height / 2.0,
            square.y + square.height / 2.0,
            b.height / 2.0,
        );
    }

    let mut fx = fx;
    continuous_mut(&mut fx, scale).below.label = Some(Text::plain("too cold"));
    let scene = compile_figure(&fx.build());
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    label_box(&drawn, "too cold");
    assert!(
        labels(&drawn).iter().all(|l| !l.text.starts_with('<')),
        "the author's label replaces the default: {:?}",
        labels(&drawn)
    );
}

// Why: the limits of a discrete scale are its first and last band edges, so its keys say "below the first edge" and
// "above the last", in the format of its ticks.
#[test]
fn the_default_key_labels_of_a_discrete_scale_name_its_first_and_last_edges() {
    let kind = ColorScaleKind::Discrete(DiscreteColorScale {
        levels: ColorScaleLevels::Explicit {
            values: vec![0.0, 2.5, 5.0, 12.0],
        },
        below: keyed(OutOfRange::Clamp),
        above: keyed(OutOfRange::Clamp),
        ..DiscreteColorScale::default()
    });
    let (fx, _, _, panel) = lone_bar(kind);
    let scene = compile_figure(&fx.build());
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    let ticks = numeric(&drawn);
    let text_at = |v: f64| {
        ticks
            .iter()
            .find(|t| t.0 == v)
            .map(|t| t.2.clone())
            .expect("a tick at each edge")
    };
    label_box(&drawn, &format!("> {}", text_at(12.0)));
    label_box(&drawn, &format!("< {}", text_at(0.0)));
}

// Why: an indexed bar has no below or above, only values that name no entry; its outside key takes the place of the
// below key, under the bar, followed by the non-finite key, and is labelled "Out of range".
#[test]
fn an_indexed_bar_shows_its_outside_key_below_the_bar_then_the_non_finite_key() {
    let kind = ColorScaleKind::Indexed(IndexedColorScale {
        colors: vec![RED, GREEN, BLUE],
        outside: keyed(OutOfRange::Rgba { color: GREY }),
        ..IndexedColorScale::default()
    });
    let (mut fx, _, scale, panel) = lone_bar(kind);
    fx.scale_mut(scale).non_finite = keyed(OutOfRange::Transparent);
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    assert!(
        key_rect(&hit, ColorScaleKey::Below).is_none()
            && key_rect(&hit, ColorScaleKey::Above).is_none()
    );
    let outside = key_rect(&hit, ColorScaleKey::Outside).expect("the outside key is drawn");
    let nan = key_rect(&hit, ColorScaleKey::NonFinite).unwrap();
    let margin = outside.y - hit.bar.bottom();
    assert!(margin > 0.0);
    assert_close(nan.y - outside.bottom(), margin, EXACT);
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    assert!(rgb8_close(
        rgb8(fill_at(&drawn, outside).expect("filled")),
        rgb8_of(GREY)
    ));
    let b = label_box(&drawn, "Out of range");
    assert_close(
        b.y + b.height / 2.0,
        outside.y + outside.height / 2.0,
        b.height / 2.0,
    );
}

// Why: for a categorical scale the keys are simply more entries of the list (#74's "not entered" swatch), after the
// categories, laid out like them with the same margin, and the key of an author's label shows that label.
#[test]
fn the_keys_of_a_categorical_scale_follow_its_categories_as_further_squares() {
    let mut kind = categories(&[(RED, "taken"), (GREEN, "skipped")]);
    if let ColorScaleKind::Categorical(c) = &mut kind {
        c.outside = keyed(OutOfRange::Rgba { color: GREY });
    }
    let (mut fx, _, scale, panel) = lone_bar(kind);
    fx.scale_mut(scale).non_finite = ColorScaleOutOfRange {
        policy: OutOfRange::Transparent,
        show_key: true,
        label: Some(Text::plain("not entered")),
    };
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let outside = key_rect(&hit, ColorScaleKey::Outside).unwrap();
    let nan = key_rect(&hit, ColorScaleKey::NonFinite).unwrap();
    let margin = hit.entries[1].y - hit.entries[0].bottom();
    assert_close(outside.y - hit.entries[1].bottom(), margin, EXACT);
    assert_close(nan.y - outside.bottom(), margin, EXACT);
    for key in [outside, nan] {
        assert_close(key.width, hit.entries[0].width, EXACT);
        assert_close(key.x, hit.entries[0].x, EXACT);
    }
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    label_box(&drawn, "Out of range");
    label_box(&drawn, "not entered");
    assert!(labels(&drawn).iter().all(|l| l.text != "NaN / Inf"));
}

// Why: the ends of a bar are always flat and never carry an out-of-range colour, which could not be told from the
// last colour in range; with keys off, nothing filled lies beyond either end of the bar.
#[test]
fn nothing_is_drawn_beyond_the_ends_of_a_bar_without_keys() {
    let (mut fx, _, scale, panel) = lone_bar(viridis(manual(0.0, 1.0)));
    let c = continuous_mut(&mut fx, scale);
    c.below = out_of_range(OutOfRange::Rgba { color: RED });
    c.above = out_of_range(OutOfRange::Rgba { color: BLUE });
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let drawn = owned(&scene, ColorbarOwner::Panel(panel));
    assert!(hit.keys.is_empty());
    for (r, c) in filled_rects(&drawn) {
        assert!(
            r.y >= hit.bar.y - EXACT && r.bottom() <= hit.bar.bottom() + EXACT,
            "{r:?} in {:?}",
            rgb8(c)
        );
    }
    for leaf in drawn.iter().filter(|l| l.image().is_some()) {
        assert!(
            rect_close(leaf.bbox().unwrap(), hit.bar, 0.01),
            "the only image is the bar itself"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Inside colourbars
// ---------------------------------------------------------------------------------------------------------------

// Why: an inside bar is an overlay, so adding one must not move or shrink the plot box (margin space is what a panel
// in a fitted track is for), and it is drawn over the artists, which it would otherwise be hidden behind.
#[test]
fn an_inside_bar_takes_no_margin_and_is_drawn_over_the_artists() {
    let (mut fx, ax, image, _) = inside_bar(InsideColorbarSide::South, 0.8);
    let with = compile_figure(&fx.fig.clone());
    fx.ax(ax).colorbar = None;
    let without = compile_figure(&fx.build());
    let plot = axes_hit(&with, ax).plot_rect;
    assert!(
        rect_close(plot, axes_hit(&without, ax).plot_rect, EXACT),
        "the plot box is unchanged"
    );

    let hit = bar_of(&with, ColorbarOwner::Axes(ax));
    let all = leaves(&with);
    let bar_index = all
        .iter()
        .position(|l| {
            l.source == Some(ax)
                && l.image().is_some()
                && l.bbox().is_some_and(|b| rect_close(b, hit.bar, 0.01))
        })
        .expect("the bar is drawn");
    let last_artist = all
        .iter()
        .rposition(|l| l.source == Some(image))
        .expect("the image is drawn");
    assert!(
        bar_index > last_artist,
        "the bar is painted after the image"
    );
    assert!(without.hit_map.colorbars.is_empty());
}

// Why: an inside bar lies along the named edge in a box that is centred, `length` of that edge long and inset from it
// by one margin that is the same on every side, with its ticks and labels toward the plot's interior so that nothing
// leaves the plot box.
#[test]
fn an_inside_bar_lies_along_its_edge_centred_inset_and_labelled_toward_the_interior() {
    let mut margins = Vec::new();
    for side in [
        InsideColorbarSide::South,
        InsideColorbarSide::North,
        InsideColorbarSide::East,
        InsideColorbarSide::West,
    ] {
        let (fx, ax, _, _) = inside_bar(side, 0.8);
        let scene = compile_figure(&fx.build());
        let plot = axes_hit(&scene, ax).plot_rect;
        let hit = bar_of(&scene, ColorbarOwner::Axes(ax));
        let bar = hit.bar;
        let backdrop = hit.backdrop.expect("an inside bar is boxed");
        assert!(
            rect_close(union(backdrop, bar), backdrop, EXACT),
            "{side:?}: the box encloses the bar"
        );
        let horizontal = matches!(side, InsideColorbarSide::South | InsideColorbarSide::North);
        assert_eq!(
            hit.orientation,
            if horizontal {
                ColorbarOrientation::Horizontal
            } else {
                ColorbarOrientation::Vertical
            },
            "{side:?}"
        );
        if horizontal {
            assert_close(backdrop.width, 0.8 * plot.width, EXACT);
            assert_close(
                backdrop.x + backdrop.width / 2.0,
                plot.x + plot.width / 2.0,
                EXACT,
            );
            assert_close(bar.x + bar.width / 2.0, plot.x + plot.width / 2.0, EXACT);
        } else {
            assert_close(backdrop.height, 0.8 * plot.height, EXACT);
            assert_close(
                backdrop.y + backdrop.height / 2.0,
                plot.y + plot.height / 2.0,
                EXACT,
            );
        }
        let margin = match side {
            InsideColorbarSide::South => plot.bottom() - backdrop.bottom(),
            InsideColorbarSide::North => backdrop.y - plot.y,
            InsideColorbarSide::East => plot.right() - backdrop.right(),
            InsideColorbarSide::West => backdrop.x - plot.x,
        };
        assert!(margin > 0.0, "{side:?} is inset from its edge");
        margins.push(margin);

        // The tick labels of the bar: the numeric runs of the axes inside the plot box.
        let ticks: Vec<Rect> = numeric(&owned(&scene, ColorbarOwner::Axes(ax)))
            .into_iter()
            .map(|t| t.1)
            .filter(|b| {
                b.x >= backdrop.x
                    && b.right() <= backdrop.right()
                    && b.y >= backdrop.y
                    && b.bottom() <= backdrop.bottom()
            })
            .collect();
        assert!(
            ticks.len() >= 2,
            "{side:?}: the bar's labels lie inside its box, inside the plot box"
        );
        for b in ticks {
            match side {
                InsideColorbarSide::South => {
                    assert!(b.bottom() <= bar.y + EXACT, "{side:?}: labels above")
                }
                InsideColorbarSide::North => {
                    assert!(b.y >= bar.bottom() - EXACT, "{side:?}: labels below")
                }
                InsideColorbarSide::East => {
                    assert!(b.right() <= bar.x + EXACT, "{side:?}: labels to the left")
                }
                InsideColorbarSide::West => {
                    assert!(b.x >= bar.right() - EXACT, "{side:?}: labels to the right")
                }
            }
        }
    }
    for m in &margins {
        assert_close(*m, margins[0], EXACT);
    }
}

// Why: the length is the author's choice of how much of the edge the boxed bar takes; half is half.
#[test]
fn the_length_of_an_inside_bar_is_its_fraction_of_the_edge() {
    let (fx, ax, _, _) = inside_bar(InsideColorbarSide::East, 0.5);
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let hit = bar_of(&scene, ColorbarOwner::Axes(ax));
    assert_close(
        hit.backdrop.expect("an inside bar is boxed").height,
        0.5 * plot.height,
        EXACT,
    );
    assert!(hit.bar.height < 0.5 * plot.height && hit.bar.height > 0.4 * plot.height);
}

// Why: an inside bar keeps its keys within its length, as a panel bar does, so that the whole key, bar and squares
// together, lies in the box that is `length` of the edge, centred in it and so along the edge. The keys are uneven
// (two beyond the low end, one beyond the high end), so a bar centred on its own with keys hung outside it fails.
#[test]
fn the_keys_of_an_inside_bar_fit_within_its_length() {
    let (mut fx, ax, _, scale) = inside_bar(InsideColorbarSide::South, 0.8);
    let c = continuous_mut(&mut fx, scale);
    c.below = keyed(OutOfRange::Clamp);
    c.above = keyed(OutOfRange::Clamp);
    fx.scale_mut(scale).non_finite = keyed(OutOfRange::Transparent);
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let hit = bar_of(&scene, ColorbarOwner::Axes(ax));
    let nan = key_rect(&hit, ColorScaleKey::NonFinite).unwrap();
    let above = key_rect(&hit, ColorScaleKey::Above).unwrap();
    let backdrop = hit.backdrop.expect("an inside bar is boxed");
    assert_close(backdrop.width, 0.8 * plot.width, EXACT);
    assert!(nan.x >= backdrop.x && above.right() <= backdrop.right());
    assert_close(
        (nan.x + above.right()) / 2.0,
        plot.x + plot.width / 2.0,
        EXACT,
    );
    assert!(
        length(&hit) < 0.8 * plot.width,
        "the bar shrank to make room"
    );
}

// Why: a 3D axes has a plot box too, and an inside bar of its surface's scale is drawn there in the same way, without
// changing the box.
#[test]
fn an_inside_bar_works_on_a_three_dimensional_axes() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let g = [0.0, 1.0, 2.0];
    fx.surface(ax, &g, &g, |x, y| x + y, |_| {});
    let scale = fx.axes_scale(ax);
    let without = compile_figure(&fx.fig.clone());
    fx.ax(ax).colorbar = Some(InsideColorbar {
        scale,
        side: InsideColorbarSide::East,
        length: 0.8,
        ..InsideColorbar::default()
    });
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    assert!(rect_close(plot, axes_hit(&without, ax).plot_rect, EXACT));
    let hit = bar_of(&scene, ColorbarOwner::Axes(ax));
    assert_eq!(hit.orientation, ColorbarOrientation::Vertical);
    let backdrop = hit.backdrop.expect("an inside bar is boxed");
    assert!(
        backdrop.x >= plot.x
            && backdrop.right() <= plot.right()
            && backdrop.y >= plot.y
            && backdrop.bottom() <= plot.bottom()
    );
    assert!(rect_close(union(backdrop, hit.bar), backdrop, EXACT));
    assert_close(backdrop.height, 0.8 * plot.height, EXACT);
    assert!(matches!(axes_hit(&scene, ax).kind, AxesHitKind::ThreeD));
}

// Why: an inside bar is read over whatever the plot shows, so it stands on a box like a boxed inside legend's: the box
// is painted over the artists and beneath every part of the bar, and encloses all of it (title, bar, keys, tick marks
// and labels), or a label would sit on the data the box is there to hide. That holds wherever the title is placed.
#[test]
fn an_inside_bar_stands_on_a_box_over_the_artists_that_encloses_all_of_it() {
    for (side, placement) in [
        (InsideColorbarSide::South, ColorbarTitlePlacement::Along),
        (InsideColorbarSide::South, ColorbarTitlePlacement::Above),
        (InsideColorbarSide::East, ColorbarTitlePlacement::Along),
        (InsideColorbarSide::East, ColorbarTitlePlacement::Above),
    ] {
        let (mut fx, ax, image, scale) = inside_bar(side, 0.8);
        fx.ax(ax)
            .colorbar
            .as_mut()
            .expect("the fixture's bar")
            .title_placement = placement;
        fx.scale_mut(scale).title = Some(Text::plain("Speed (m/s)"));
        let c = continuous_mut(&mut fx, scale);
        c.below = keyed(OutOfRange::Clamp);
        c.above = keyed(OutOfRange::Clamp);
        fx.scale_mut(scale).non_finite = keyed(OutOfRange::Transparent);
        let scene = compile_figure(&fx.build());
        let hit = bar_of(&scene, ColorbarOwner::Axes(ax));
        let backdrop = hit.backdrop.expect("an inside bar is boxed");
        let all = leaves(&scene);
        let at = all
            .iter()
            .position(|l| {
                l.source == Some(ax)
                    && is_rectangle(l)
                    && l.bbox().is_some_and(|b| rect_close(b, backdrop, EXACT))
                    && l.path()
                        .and_then(|p| p.fill)
                        .is_some_and(|f| rgb8(f.color) == [255, 255, 255])
            })
            .unwrap_or_else(|| panic!("{side:?}: a white box is drawn at {backdrop:?}"));
        let last_artist = all
            .iter()
            .rposition(|l| l.source == Some(image))
            .expect("the image is drawn");
        assert!(
            at > last_artist,
            "{side:?}: the box is painted over the artists"
        );
        // Everything the axes paints after the box is the colourbar (the fixture has no legend).
        let after: Vec<&Leaf> = all[at + 1..]
            .iter()
            .filter(|l| l.source == Some(ax))
            .collect();
        assert!(
            after.iter().any(|l| l.image().is_some()),
            "{side:?}: the bar is painted on the box"
        );
        assert!(
            labels(&owned(&scene, ColorbarOwner::Axes(ax)))
                .iter()
                .any(|l| l.text == "Speed (m/s)"),
            "{side:?}: the title is drawn"
        );
        for leaf in after {
            let b = leaf.bbox().expect("a drawn leaf has a box");
            assert!(
                b.x >= backdrop.x - 0.5
                    && b.right() <= backdrop.right() + 0.5
                    && b.y >= backdrop.y - 0.5
                    && b.bottom() <= backdrop.bottom() + 0.5,
                "{side:?}: {b:?} lies inside the box {backdrop:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// One scale, many bars; scales without data; unusable references
// ---------------------------------------------------------------------------------------------------------------

// Why: a bar holds no colours, limits or ticks of its own, so two bars of one scale, in a panel and inside an axes,
// must agree on every tick and every colour; otherwise one of them lies about the plots. The inside bar takes the
// full edge, so that both bars are nearly equally long (the inside one shorter by the padding of its box) and
// automatic ticks, which depend on the length, are comparable.
#[test]
fn two_bars_of_one_scale_agree_on_their_ticks_and_colours() {
    let (mut fx, axes, scale, panel) = column_beside_stack(1, TrackSize::Fit);
    fx.ax(axes[0]).colorbar = Some(InsideColorbar {
        scale,
        side: InsideColorbarSide::West,
        length: 1.0,
        ..InsideColorbar::default()
    });
    let scene = compile_figure(&fx.build());
    let in_panel = bar_of(&scene, ColorbarOwner::Panel(panel));
    let inside = bar_of(&scene, ColorbarOwner::Axes(axes[0]));
    assert!(
        length(&inside) < length(&in_panel)
            && length(&in_panel) - length(&inside) < 2.0 * FONT_SIZE,
        "{} and {}",
        length(&inside),
        length(&in_panel)
    );
    let plot = axes_hit(&scene, axes[0]).plot_rect;
    let panel_drawn = owned(&scene, ColorbarOwner::Panel(panel));
    let axes_drawn = owned(&scene, ColorbarOwner::Axes(axes[0]));
    let texts = |ticks: Vec<(f64, Rect, String)>, within: Option<Rect>| {
        let mut t: Vec<String> = ticks
            .into_iter()
            .filter(|(_, b, _)| {
                within.is_none_or(|p| {
                    b.x >= p.x && b.right() <= p.right() && b.y >= p.y && b.bottom() <= p.bottom()
                })
            })
            .map(|t| t.2)
            .collect();
        t.sort();
        t
    };
    assert_eq!(
        texts(numeric(&panel_drawn), None),
        texts(numeric(&axes_drawn), Some(plot))
    );
    for f in [0.1, 0.3, 0.7] {
        assert_eq!(
            bar_colour(&panel_drawn, &in_panel, f),
            bar_colour(&axes_drawn, &inside, f),
            "at {f}"
        );
    }
}

// Why: a hidden artist still contributes to its scale's automatic limits (ADR 0006), so the bar of a scale whose only
// artist is hidden still runs over that artist's values; hiding a plot must not relabel its key.
#[test]
fn the_bar_of_a_scale_whose_artist_is_hidden_still_spans_its_values() {
    let (mut fx, ax, _, panel) = lone_bar(viridis(Limits::Auto));
    let scale = fx.fig.color_scales[0].id;
    fx.scalar_image(
        ax,
        vec![1, 2],
        vec![0.0, 50.0],
        Some(scale),
        xy(None, None),
        |image| image.visible = false,
    );
    let scene = compile_figure(&fx.build());
    let values: Vec<f64> = numeric(&owned(&scene, ColorbarOwner::Panel(panel)))
        .into_iter()
        .map(|t| t.0)
        .collect();
    assert!(
        values.contains(&0.0) && values.contains(&50.0),
        "{values:?}"
    );
    assert!(values.iter().all(|v| (0.0..=50.0).contains(v)));
}

// Why: a custom colourbar shows a scale that no artist uses (#24): with manual limits it shows them, and with
// automatic limits over no value it shows the default range of stage 2, [0, 1], or [1, 10] under a logarithmic norm;
// either way it is a normal figure and raises no warning.
#[test]
fn a_bar_of_a_scale_that_no_artist_names_shows_its_limits_or_the_default_range() {
    for (kind, low, high) in [
        (viridis(manual(2.0, 7.0)), 2.0, 7.0),
        (viridis(Limits::Auto), 0.0, 1.0),
    ] {
        let (fx, _, scale, panel) = lone_bar(kind);
        let scene = compile_figure(&fx.build());
        let ticks = numeric(&owned(&scene, ColorbarOwner::Panel(panel)));
        let values: Vec<f64> = ticks.iter().map(|t| t.0).collect();
        assert!(
            values.contains(&low) && values.contains(&high),
            "{values:?}"
        );
        assert!(
            warnings_naming(&scene, panel).is_empty(),
            "{:?}",
            scene.warnings
        );
        assert!(
            warnings_naming(&scene, scale).is_empty(),
            "{:?}",
            scene.warnings
        );
    }
    let log = ColorScaleKind::Continuous(ContinuousColorScale {
        norm: ColorScaleNorm::Log,
        ..ContinuousColorScale::default()
    });
    let (fx, _, _, panel) = lone_bar(log);
    let scene = compile_figure(&fx.build());
    let decades = glyph_runs(&owned(&scene, ColorbarOwner::Panel(panel)))
        .into_iter()
        .filter(|(_, g)| g.font == FontId::Math && g.text == "10")
        .count();
    assert!(decades >= 2, "the decades 1 and 10 are labelled");
}

// Why: a figure read from a file may name a scale it does not have; validation refuses it, but the compiler must
// still draw the rest of the figure, leave the bar out and say why against the node that holds the reference.
#[test]
fn a_colourbar_naming_no_scale_is_left_out_with_a_warning_against_its_owner() {
    let (mut fx, axes, _, panel) = column_beside_stack(1, TrackSize::Fit);
    if let PanelContent::Colorbar(colorbar) = &mut fx.panel_mut(panel).content {
        colorbar.scale = NodeId(9_999);
    }
    fx.ax(axes[0]).colorbar = Some(InsideColorbar {
        scale: axes[0],
        side: InsideColorbarSide::South,
        length: 0.8,
        ..InsideColorbar::default()
    });
    let scene = compile_figure(&fx.build());
    assert!(
        scene.hit_map.colorbars.is_empty(),
        "{:?}",
        scene.hit_map.colorbars
    );
    assert!(
        !warnings_naming(&scene, panel).is_empty(),
        "{:?}",
        scene.warnings
    );
    assert!(
        !warnings_naming(&scene, axes[0]).is_empty(),
        "{:?}",
        scene.warnings
    );
    assert!(
        scene.hit_map.panels.iter().any(|p| p.id == panel),
        "the panel keeps its place in the layout"
    );
}
