//! The alignment of axes titles with their plot areas, and the warning raised for text that extends beyond the page.

use ironlab_ir::{
    Aspect, AspectAdjust, FigureSize, NodeId, Text, TickLabels, Ticks, TileLayout, TitleAlign,
};
use ironlab_scene::{Scene, SceneWarningKind};

use crate::common::{Fx, compile_figure, linspace, text};
use crate::probe::{assert_close, axes_hit, centre, leaves, text_bbox};

/// How far, in points, the ink of a title may lie from the edge it is aligned with: the side bearing of its first or
/// last glyph.
const BEARING: f64 = 1.0;

/// A 1 × 2 layout whose left axes has a long title, wide y tick labels and a y label, so that its plot area starts
/// well to the right of its tile.
fn titled(align: TitleAlign) -> (Fx, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout = TileLayout::new(1, 2);
    let ax = fx.axes2d(0, 0);
    let x = linspace(0.0, 1.0, 11);
    let y: Vec<f64> = x.iter().map(|v| 12_345.0 * v).collect();
    fx.line(ax, &x, &y, None, |_| {});
    fx.ax(ax).y.label = text("Lift per unit span");
    fx.ax(ax).title = Some(Text::plain("Thickness t (m = 0.04)"));
    fx.ax(ax).title_align = align;
    let other = fx.axes2d(0, 1);
    fx.line(other, &x, &x, None, |_| {});
    (fx, ax)
}

fn title_box(scene: &Scene) -> ironlab_scene::display::Rect {
    text_bbox(&leaves(scene), "Thickness t (m = 0.04)").expect("the title is drawn")
}

// Why: a family sheet names the varied and the fixed parameters in long titles that read from the left edge of the
// plot, as matplotlib's `loc="left"` does; aligning with the tile instead would push the title over the tick labels.
#[test]
fn a_left_aligned_title_starts_at_the_left_edge_of_the_plot_area() {
    let (fx, ax) = titled(TitleAlign::Left);
    let scene = compile_figure(&fx.fig);
    let plot = axes_hit(&scene, ax).plot_rect;
    let title = title_box(&scene);
    assert_close(title.x, plot.x, BEARING);
    assert!(title.bottom() <= plot.y, "the title stays above the plot");
}

// Why: a right-aligned title ends at the right edge of the plot area, not of the tile.
#[test]
fn a_right_aligned_title_ends_at_the_right_edge_of_the_plot_area() {
    let (fx, ax) = titled(TitleAlign::Right);
    let scene = compile_figure(&fx.fig);
    let plot = axes_hit(&scene, ax).plot_rect;
    assert_close(title_box(&scene).right(), plot.right(), BEARING);
}

// Why: the default stays as it was, the title centred over the plot area, so existing figures do not move.
#[test]
fn a_centred_title_is_centred_over_the_plot_area() {
    let (fx, ax) = titled(TitleAlign::Center);
    let scene = compile_figure(&fx.fig);
    let plot = axes_hit(&scene, ax).plot_rect;
    assert_close(centre(title_box(&scene)).x, centre(plot).x, BEARING);
}

// Why: a plot area that the aspect shrinks is still the plot area; a left-aligned title must follow its left edge,
// which moves right as the box narrows, and sit just above its top edge.
#[test]
fn a_left_aligned_title_follows_a_plot_area_shrunk_by_the_aspect() {
    let (mut fx, ax) = titled(TitleAlign::Left);
    let free = axes_hit(&compile_figure(&fx.fig), ax).plot_rect;
    fx.ax(ax).aspect = Aspect::Equal;
    fx.ax(ax).aspect_adjust = AspectAdjust::Box;
    fx.ax(ax).y.limits = ironlab_ir::Limits::Manual { min: 0.0, max: 3.0 };
    let scene = compile_figure(&fx.fig);
    let plot = axes_hit(&scene, ax).plot_rect;
    assert!(
        plot.x > free.x + 10.0,
        "the box narrows: {plot:?} in {free:?}"
    );
    let title = title_box(&scene);
    assert_close(title.x, plot.x, BEARING);
    assert!(title.bottom() <= plot.y && title.bottom() > plot.y - 20.0);
}

// -------------------------------------------------------------------------------------------------------------------
// Text beyond the page
// -------------------------------------------------------------------------------------------------------------------

/// Returns the source texts that the scene reports as extending beyond the page, with the nodes named.
fn off_page(scene: &Scene) -> Vec<(Option<NodeId>, String)> {
    scene
        .warnings
        .iter()
        .filter_map(|w| match &w.kind {
            SceneWarningKind::TextOutsidePage { text } => Some((w.node, text.clone())),
            _ => None,
        })
        .collect()
}

/// A narrow figure of one axes.
fn narrow() -> (Fx, NodeId) {
    let mut fx = Fx::new();
    fx.fig.size = FigureSize {
        width_mm: 50.0,
        height_mm: 40.0,
    };
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    (fx, ax)
}

const LONG: &str = "Position of maximum camber p with m = 0.04 and t = 0.12 held fixed";

// Why: a title too long for its figure used to be clipped by the page edge without a word, and four titles of the
// yFoil family sheets were reworded only after a reader noticed; the scene must say which text left the page.
#[test]
fn an_axes_title_beyond_the_page_is_reported_against_its_axes() {
    let (mut fx, ax) = narrow();
    fx.ax(ax).title = Some(Text::plain(LONG));
    let scene = compile_figure(&fx.fig);
    assert_eq!(off_page(&scene), vec![(Some(ax), LONG.to_owned())]);
}

// Why: a left-aligned title runs only to the right, so it leaves the page at the right edge, and is reported there
// too.
#[test]
fn a_left_aligned_title_beyond_the_right_edge_is_reported() {
    let (mut fx, ax) = narrow();
    fx.ax(ax).title = Some(Text::plain(LONG));
    fx.ax(ax).title_align = TitleAlign::Left;
    let scene = compile_figure(&fx.fig);
    assert_eq!(off_page(&scene), vec![(Some(ax), LONG.to_owned())]);
}

// Why: the warning covers every title and label, not only axes titles: the figure title names the figure, and an
// axis label names its axes.
#[test]
fn a_figure_title_and_an_axis_label_beyond_the_page_are_reported() {
    let (mut fx, ax) = narrow();
    fx.fig.title = Some(Text::plain(LONG));
    fx.ax(ax).x.label = Some(Text::plain(
        "A horizontal axis label far longer than the page is wide",
    ));
    let scene = compile_figure(&fx.fig);
    let reported = off_page(&scene);
    assert!(
        reported.contains(&(Some(fx.fig.id), LONG.to_owned())),
        "{reported:?}"
    );
    assert!(
        reported.contains(&(
            Some(ax),
            "A horizontal axis label far longer than the page is wide".to_owned()
        )),
        "{reported:?}"
    );
}

// Why: explicit tick labels are author text like a title, and one too long for the page is lost the same way.
#[test]
fn a_tick_label_beyond_the_page_is_reported() {
    let (mut fx, ax) = narrow();
    let label = "an extremely long name for the last category of the strip chart";
    fx.ax(ax).x.ticks = Ticks::Values(vec![0.0, 1.0]);
    fx.ax(ax).x.tick_labels = TickLabels::Text(vec![Text::plain("first"), Text::plain(label)]);
    let scene = compile_figure(&fx.fig);
    assert!(
        off_page(&scene).contains(&(Some(ax), label.to_owned())),
        "{:?}",
        off_page(&scene)
    );
}

// Why: a centred title wider than its plot area but inside the page is drawn whole, so it must not be reported.
#[test]
fn a_title_wider_than_its_plot_but_inside_the_page_raises_no_warning() {
    let mut fx = Fx::new();
    fx.fig.layout = TileLayout::new(1, 3);
    let ax = fx.axes2d(0, 1);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    fx.ax(ax).title = Some(Text::plain(
        "A centred title rather wider than its own narrow plot area",
    ));
    let scene = compile_figure(&fx.fig);
    let plot = axes_hit(&scene, ax).plot_rect;
    assert!(
        text_bbox(
            &leaves(&scene),
            "A centred title rather wider than its own narrow plot area"
        )
        .unwrap()
        .width
            > plot.width
    );
    assert!(off_page(&scene).is_empty(), "{:?}", off_page(&scene));
}

// Why: a right-aligned title runs only to the left, so on the right-hand tile a title too long for the page leaves
// it at the left edge, and is reported there.
#[test]
fn a_right_aligned_title_beyond_the_left_edge_is_reported() {
    let mut fx = Fx::new();
    fx.fig.size = FigureSize {
        width_mm: 80.0,
        height_mm: 40.0,
    };
    fx.fig.layout = TileLayout::new(1, 2);
    let ax = fx.axes2d(0, 1);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    fx.ax(ax).title = Some(Text::plain(LONG));
    fx.ax(ax).title_align = TitleAlign::Right;
    let scene = compile_figure(&fx.fig);
    let title = text_bbox(&leaves(&scene), LONG).unwrap();
    assert!(title.x < 0.0, "the title runs off the left edge: {title:?}");
    assert_eq!(off_page(&scene), vec![(Some(ax), LONG.to_owned())]);
}

// Why: the warning must not cry wolf: an ordinary figure whose text fits reports nothing, or authors learn to ignore
// it.
#[test]
fn text_that_fits_the_page_raises_no_warning() {
    let (fx, _) = titled(TitleAlign::Left);
    let scene = compile_figure(&fx.fig);
    assert!(off_page(&scene).is_empty(), "{:?}", off_page(&scene));
}
