//! The hit geometry a colourbar publishes for the viewer (stage 6 of the colourbar epic): how scale values lie along
//! the bar, the band edges of a discrete bar, and the label of every out-of-range key as it is drawn.
//!
//! The viewer derives nothing. A datatip over a bar reports the value, the band or the key under the pointer, and a
//! gesture over a bar starts from the limits the reader sees, so everything either needs must come from the
//! compilation that drew the bar rather than be worked out again in the canvas: the limits a scale with automatic
//! limits resolved to, the span a scale of one value is drawn over, the edges the compiler chose for automatic bands,
//! and the default text of a key label, which is formatted as the bar's ticks are.
//!
//! The scene API these tests assume, beyond `crates/ironlab-scene/tests/compile/colorbars.rs`:
//!
//! ```text
//! // crates/ironlab-scene/src/hit.rs
//! pub struct ColorbarHit {
//!     ...,
//!     /// How scale values lie along the bar: `min` at `start` and `max` at `end`, which is the low end of the bar
//!     /// (its bottom when it stands, its left when it lies), in base-10 logarithms under a logarithmic norm. The
//!     /// limits are those the bar is drawn over: a continuous scale's limits, automatic ones resolved; a discrete
//!     /// scale's first and last edges; an indexed scale's −0.5 to n − 0.5, so that entry k lies at the centre of
//!     /// its block; and, for limits that coincide, the span of one unit (one decade) about them that the bar is
//!     /// drawn over. `None` for a categorical scale, whose squares are its geometry.
//!     pub map: Option<AxisMap>,
//!     /// The band edges of a discrete bar as drawn, automatic ones as the compiler chose them, increasing; empty for
//!     /// every other kind.
//!     pub edges: Vec<f64>,
//! }
//!
//! pub struct ColorbarKeyHit {
//!     pub key: ColorScaleKey,
//!     pub rect: Rect,
//!     /// The source text of the label drawn beside the key: the author's label, or the default label as drawn.
//!     pub label: String,
//! }
//! ```
//!
//! `ColorbarKeyHit` is no longer `Copy`, because it carries a `String`.

use ironlab_ir::{
    CategoricalColorScale, Cell, Color, ColorCategory, ColorScaleKey, ColorScaleKind,
    ColorScaleLevels, ColorScaleNorm, ColorScaleOutOfRange, ColorbarOrientation,
    ContinuousColorScale, DiscreteColorScale, IndexedColorScale, InsideColorbar,
    InsideColorbarSide, LegendEntries, LegendItem, Limits, NodeId, OutOfRange, Panel,
    PanelColorbar, PanelContent, PanelLocation, Text, TrackSize,
};
use ironlab_scene::Scene;
use ironlab_scene::hit::{AxisMap, ColorbarHit, ColorbarOwner, LegendOwner};

use crate::common::{Fx, cell, compile_figure, out_of_range, xy};

/// Geometry that the compiler computes exactly, such as the end of a bar.
const EXACT: f64 = 1e-6;

// ---------------------------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------------------------

fn manual(min: f64, max: f64) -> Limits {
    Limits::Manual { min, max }
}

fn continuous(limits: Limits, norm: ColorScaleNorm) -> ColorScaleKind {
    ColorScaleKind::Continuous(ContinuousColorScale {
        limits,
        norm,
        ..ContinuousColorScale::default()
    })
}

fn discrete(levels: ColorScaleLevels) -> ColorScaleKind {
    ColorScaleKind::Discrete(DiscreteColorScale {
        levels,
        ..DiscreteColorScale::default()
    })
}

fn indexed(n: usize) -> ColorScaleKind {
    ColorScaleKind::Indexed(IndexedColorScale {
        colors: (0..n)
            .map(|k| Color::rgb(k as f32 / n as f32, 0.5, 0.5))
            .collect(),
        ..IndexedColorScale::default()
    })
}

fn categorical(labels: &[&str]) -> ColorScaleKind {
    ColorScaleKind::Categorical(CategoricalColorScale {
        categories: labels
            .iter()
            .map(|label| ColorCategory {
                color: Color::rgb(0.2, 0.4, 0.6),
                label: Text::plain(*label),
            })
            .collect(),
        outside: out_of_range(OutOfRange::Strict),
    })
}

fn keyed(policy: OutOfRange, label: Option<&str>) -> ColorScaleOutOfRange {
    ColorScaleOutOfRange {
        policy,
        show_key: true,
        label: label.map(Text::plain),
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
        title_align: ironlab_ir::TitleAlign::Left,
        content: PanelContent::Colorbar(PanelColorbar {
            scale,
            orientation,
            ..PanelColorbar::default()
        }),
    });
    id
}

/// One 2D axes beside a fitted column holding a vertical colourbar panel of a scale of the given kind, with an image
/// of `values` (one row) through the scale when there are any. Returns the figure, the axes, the scale and the panel.
fn bar_beside(kind: ColorScaleKind, values: &[f64]) -> (Fx, NodeId, NodeId, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    fx.fig.layout.col_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit];
    let ax = fx.axes2d(0, 0);
    let scale = fx.color_scale(kind);
    if !values.is_empty() {
        fx.scalar_image(
            ax,
            vec![1, values.len()],
            values.to_vec(),
            Some(scale),
            xy(None, None),
            |_| {},
        );
    }
    let panel = colorbar_panel(
        &mut fx,
        cell(0, 1, 1, 1),
        scale,
        ColorbarOrientation::Vertical,
    );
    (fx, ax, scale, panel)
}

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

#[track_caller]
fn map_of(hit: &ColorbarHit) -> AxisMap {
    hit.map.unwrap_or_else(|| {
        panic!(
            "the bar of {:?} publishes how values lie along it",
            hit.owner
        )
    })
}

#[track_caller]
fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: expected {b}, got {a}");
}

// ---------------------------------------------------------------------------------------------------------------
// How values lie along the bar
// ---------------------------------------------------------------------------------------------------------------

// Why: a datatip over a bar reads the value under the pointer and a wheel over it zooms about that value, both by
// inverting the mapping the bar was drawn with. The mapping must therefore run from the low end of the bar, where the
// lower limit is drawn (the bottom of a standing bar, the left of a lying one), to its far end, exactly over the bar
// rectangle the hit map publishes, or a reading would be off by however far the two disagree.
#[test]
fn a_bar_publishes_its_limits_from_its_low_end_to_its_high_end_over_the_bar_itself() {
    let (fx, _, _, panel) = bar_beside(continuous(manual(2.0, 8.0), ColorScaleNorm::Linear), &[]);
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let map = map_of(&hit);
    assert_eq!((map.min, map.max, map.log), (2.0, 8.0, false));
    close(
        map.start,
        hit.bar.bottom(),
        EXACT,
        "the lower limit lies at the bottom",
    );
    close(map.end, hit.bar.y, EXACT, "the upper limit lies at the top");

    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.row_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit];
    fx.axes2d(0, 0);
    let scale = fx.color_scale(continuous(manual(2.0, 8.0), ColorScaleNorm::Linear));
    let lying = colorbar_panel(
        &mut fx,
        cell(1, 0, 1, 1),
        scale,
        ColorbarOrientation::Horizontal,
    );
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(lying));
    let map = map_of(&hit);
    close(
        map.start,
        hit.bar.x,
        EXACT,
        "the lower limit lies at the left",
    );
    close(
        map.end,
        hit.bar.right(),
        EXACT,
        "the upper limit lies at the right",
    );
}

// Why: a gesture on a scale with automatic limits must start from the limits the reader sees, as an axis gesture
// starts from the resolved limits of its axes; the viewer cannot resolve them itself without deriving what the
// compiler already derived, so the bar publishes them: the range of every value mapped through the scale.
#[test]
fn a_bar_of_automatic_limits_publishes_the_limits_they_resolved_to() {
    let (fx, _, scale, panel) = bar_beside(
        continuous(Limits::Auto, ColorScaleNorm::Linear),
        &[3.0, -1.0, 7.5],
    );
    let figure = fx.build();
    let scene = compile_figure(&figure);
    let map = map_of(&bar_of(&scene, ColorbarOwner::Panel(panel)));
    assert_eq!(
        Some((map.min, map.max)),
        figure.automatic_color_limits(scale),
        "the bar runs over the automatic limits the IR defines"
    );
    assert_eq!((map.min, map.max), (-1.0, 7.5));
}

// Why: a logarithmic scale places decades evenly along its bar, so the reading under the pointer and a zoom about it
// must be computed in logarithms; the bar says so, as an axis map does for a logarithmic axis.
#[test]
fn a_bar_of_a_logarithmic_scale_publishes_a_logarithmic_mapping() {
    let (fx, _, _, panel) = bar_beside(continuous(manual(1.0, 1000.0), ColorScaleNorm::Log), &[]);
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let map = map_of(&hit);
    assert!(map.log, "the mapping is logarithmic");
    assert_eq!((map.min, map.max), (1.0, 1000.0));
    let middle = hit.bar.y + hit.bar.height / 2.0;
    close(
        map.to_data(middle),
        10f64.powf(1.5),
        1e-9,
        "the middle of the bar is the middle decade",
    );
}

// Why: a scale whose values are all equal still draws a bar, over one unit about the value, and a gesture or a
// reading over it must use that span rather than the empty range of the limits, which no reading could invert.
#[test]
fn a_bar_of_one_value_publishes_the_span_it_is_drawn_over() {
    let (fx, _, _, panel) = bar_beside(
        continuous(Limits::Auto, ColorScaleNorm::Linear),
        &[5.0, 5.0],
    );
    let scene = compile_figure(&fx.build());
    let map = map_of(&bar_of(&scene, ColorbarOwner::Panel(panel)));
    assert!(
        map.min < 5.0 && 5.0 < map.max && map.max - map.min > 0.0,
        "the bar is drawn over a span about the one value: {map:?}"
    );
}

// Why: a discrete bar is read by its band, and the band edges of automatic levels are chosen by the compiler at nice
// values, which the viewer cannot know; the bar publishes the edges it drew, spanning its mapping from end to end,
// and explicit edges are published exactly as the author gave them.
#[test]
fn a_discrete_bar_publishes_the_edges_it_drew_from_one_end_of_the_bar_to_the_other() {
    let (fx, _, _, panel) = bar_beside(
        discrete(ColorScaleLevels::Auto {
            count: 5,
            limits: manual(0.0, 10.0),
        }),
        &[],
    );
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let map = map_of(&hit);
    assert!(
        hit.edges.len() >= 3,
        "automatic levels give several bands: {:?}",
        hit.edges
    );
    assert!(
        hit.edges.windows(2).all(|pair| pair[0] < pair[1]),
        "the edges increase: {:?}",
        hit.edges
    );
    assert_eq!(
        (hit.edges[0], *hit.edges.last().unwrap()),
        (map.min, map.max),
        "the first and last edges are the ends of the bar"
    );
    assert_eq!(
        (map.min, map.max),
        (0.0, 10.0),
        "automatic levels span their limits"
    );

    let explicit = [0.0, 0.5, 1.0, 2.0];
    let (fx, _, _, panel) = bar_beside(
        discrete(ColorScaleLevels::Explicit {
            values: explicit.to_vec(),
        }),
        &[],
    );
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    assert_eq!(hit.edges, explicit, "explicit edges are drawn as given");
    let map = map_of(&hit);
    assert_eq!((map.min, map.max), (0.0, 2.0));
}

// Why: an indexed bar is read by the entry under the pointer, and entry k is drawn as the k-th of n equal blocks;
// publishing the mapping from −0.5 to n − 0.5 puts every whole number at the centre of its block, so that the entry
// under the pointer is the nearest whole number to the value there.
#[test]
fn an_indexed_bar_maps_every_entry_to_the_centre_of_its_block() {
    let (fx, _, _, panel) = bar_beside(indexed(6), &[]);
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let map = map_of(&hit);
    assert_eq!((map.min, map.max, map.log), (-0.5, 5.5, false));
    assert!(hit.edges.is_empty(), "an indexed bar has no band edges");
}

// Why: a categorical scale is drawn as separate squares, which the hit map already lists one by one; a mapping along
// a bar would describe nothing that is drawn.
#[test]
fn a_categorical_key_publishes_no_mapping_and_no_edges() {
    let (fx, _, _, panel) = bar_beside(categorical(&["Water", "Forest", "Rock"]), &[]);
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    assert_eq!(hit.map, None);
    assert!(hit.edges.is_empty());
    assert_eq!(hit.entries.len(), 3);
}

// ---------------------------------------------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------------------------------------------

// Why: hovering a key reports what the key says, and the default label of a key is formatted as the bar's ticks are,
// which only the compiler knows; the key therefore publishes the text it drew, the author's label verbatim when there
// is one.
#[test]
fn every_key_publishes_the_label_it_drew() {
    let mut kind = continuous(manual(0.0, 10.0), ColorScaleNorm::Linear);
    if let ColorScaleKind::Continuous(scale) = &mut kind {
        scale.below = keyed(OutOfRange::Clamp, None);
        scale.above = keyed(OutOfRange::Clamp, Some("Saturated"));
    }
    let (mut fx, _, scale, panel) = bar_beside(kind, &[]);
    fx.scale_mut(scale).non_finite = keyed(OutOfRange::Transparent, None);
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    let label = |key: ColorScaleKey| -> String {
        hit.keys
            .iter()
            .find(|k| k.key == key)
            .unwrap_or_else(|| panic!("the {key:?} key is drawn: {:?}", hit.keys))
            .label
            .clone()
    };
    assert_eq!(
        label(ColorScaleKey::Below),
        "< 0",
        "the lower limit as its tick writes it"
    );
    assert_eq!(
        label(ColorScaleKey::Above),
        "Saturated",
        "the author's label verbatim"
    );
    assert_eq!(label(ColorScaleKey::NonFinite), "NaN / Inf");

    let mut kind = indexed(3);
    if let ColorScaleKind::Indexed(scale) = &mut kind {
        scale.outside = keyed(OutOfRange::Clamp, None);
    }
    let (fx, _, _, panel) = bar_beside(kind, &[]);
    let scene = compile_figure(&fx.build());
    let hit = bar_of(&scene, ColorbarOwner::Panel(panel));
    assert_eq!(
        hit.keys
            .iter()
            .map(|k| k.label.as_str())
            .collect::<Vec<_>>(),
        ["Out of range"]
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Every bar of one scale agrees
// ---------------------------------------------------------------------------------------------------------------

// Why: a scale may be shown at once by a panel colourbar, by an inside colourbar and by a bar item of a legend, and a
// gesture on any of them edits the one scale; all of them must therefore publish the same values, or the same edit
// would read differently depending on which bar the reader used.
#[test]
fn every_bar_of_one_scale_publishes_the_same_limits_and_edges() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 3;
    fx.fig.layout.col_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit, TrackSize::Fit];
    let ax = fx.axes2d(0, 0);
    let scale = fx.color_scale(discrete(ColorScaleLevels::Auto {
        count: 4,
        limits: Limits::Auto,
    }));
    fx.scalar_image(
        ax,
        vec![1, 3],
        vec![0.0, 3.0, 7.0],
        Some(scale),
        xy(None, None),
        |_| {},
    );
    fx.ax(ax).colorbar = Some(InsideColorbar {
        scale,
        side: InsideColorbarSide::East,
        length: 0.8,
        ..InsideColorbar::default()
    });
    let bar_panel = colorbar_panel(
        &mut fx,
        cell(0, 1, 1, 1),
        scale,
        ColorbarOrientation::Vertical,
    );
    let legend = fx.panel(cell(0, 2, 1, 1));
    fx.legend_mut(legend).entries = LegendEntries::Explicit(vec![LegendItem::color_scale(scale)]);
    let scene = compile_figure(&fx.build());

    let bars: Vec<ColorbarHit> = [
        ColorbarOwner::Panel(bar_panel),
        ColorbarOwner::Axes(ax),
        ColorbarOwner::Legend(LegendOwner::Panel(legend)),
    ]
    .into_iter()
    .map(|owner| bar_of(&scene, owner))
    .collect();
    for bar in &bars {
        let map = map_of(bar);
        let first = map_of(&bars[0]);
        assert_eq!(
            (map.min, map.max, map.log),
            (first.min, first.max, first.log),
            "the bar of {:?} agrees with the panel bar",
            bar.owner
        );
        assert_eq!(
            bar.edges, bars[0].edges,
            "the bar of {:?} draws the same bands",
            bar.owner
        );
        assert_eq!(bar.scale, scale);
    }
}
