//! Colourbars built through the user API (#8, #88): a scale shown in a panel of its own or
//! inside an axes, and the ticks, tick labels and keys that a scale carries for every bar
//! that shows it.
//!
//! The API these tests assume, beyond the plan's "Facade" section:
//!
//! - `PanelMut::colorbar(&mut self, scale: NodeId) -> PanelColorbarMut<'_>` makes the
//!   content of the panel a colourbar of the scale. A panel that already holds a colourbar
//!   keeps its orientation and takes the new scale; content of another kind is replaced by a
//!   vertical colourbar. `PanelMut::legend` likewise replaces a colourbar by a default
//!   legend, and keeps a legend the panel already holds.
//! - `PanelColorbarMut` offers `orientation(&mut self, ColorbarOrientation) -> &mut Self`
//!   and `id(&self) -> NodeId`, which is the identifier of the panel.
//! - `AxesMut::colorbar(&mut self, scale: NodeId, side: InsideColorbarSide) ->
//!   InsideColorbarMut<'_>` shows the scale inside the plot area along that side. An axes
//!   that already shows a colourbar keeps its length and takes the new scale and side; a new
//!   one is 0.8 of the edge long. `InsideColorbarMut` offers `length(&mut self, f64) ->
//!   &mut Self`.
//! - `ColorScaleMut` gains tick setters that mirror those of an axis:
//!   `ticks(&mut self, impl AsRef<[f64]>)` (explicit values), `tick_step(&mut self, f64)`
//!   (a regular step from 0), `tick_labels(&mut self, impl IntoIterator<Item = (V, T)>)`
//!   with `V: Into<f64>` and `T: Into<Text>`, which sets the ticks to the values and labels
//!   each with its text in one call (because a label means nothing apart from its value),
//!   and `tick_labels_hidden(&mut self)`, which keeps the ticks. They apply to the
//!   continuous, discrete and indexed kinds, which have ticks, and leave a categorical scale
//!   as it is.
//! - `ColorScaleMut` gains key setters: `show_key(&mut self, ColorScaleKey) -> &mut Self`
//!   shows the key of one out-of-range category with its default label, and
//!   `key_label(&mut self, ColorScaleKey, impl Into<Text>) -> &mut Self` shows it with the
//!   given label. Neither changes the category's policy, which `below`, `above`, `outside`
//!   and `non_finite` set, and a key that the kind of the scale does not have (below or
//!   above on an index-mapped scale, outside on a continuous or discrete one) leaves the
//!   scale as it is, as the policy setters do.
//! - The prelude exports `ColorbarOrientation`, `InsideColorbarSide` and `ColorScaleKey`,
//!   and the crate exports `PanelColorbarMut` and `InsideColorbarMut`.
//!
//! What a bar draws belongs to the scene tests and what validation refuses to the IR tests;
//! these tests own the facade's promises.

mod common;

use common::has_error_at;
use ironlab::ir::{
    Cell, ColorScale, ColorScaleKind, InsideColorbar, IssueKind, OutOfRange, Panel, PanelColorbar,
    PanelContent, PanelLegend, TickLabels, Ticks,
};
use ironlab::prelude::*;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

/// Returns the panel with the given identifier.
#[track_caller]
fn panel(fig: &Figure, id: NodeId) -> &Panel {
    fig.ir()
        .panel(id)
        .unwrap_or_else(|| panic!("figure has no panel {id}"))
}

/// Returns the colourbar that the panel with the given identifier holds.
#[track_caller]
fn panel_colorbar(fig: &Figure, id: NodeId) -> PanelColorbar {
    match &panel(fig, id).content {
        PanelContent::Colorbar(colorbar) => *colorbar,
        other => panic!("panel {id} holds {other:?}, not a colourbar"),
    }
}

/// Returns the colourbar inside the axes with the given identifier.
#[track_caller]
fn inside_colorbar(fig: &Figure, id: NodeId) -> Option<InsideColorbar> {
    fig.ir()
        .axes(id)
        .unwrap_or_else(|| panic!("{id} is not an axes"))
        .colorbar
}

/// Returns the scale with the given identifier.
#[track_caller]
fn scale(fig: &Figure, id: NodeId) -> &ColorScale {
    fig.ir()
        .color_scale(id)
        .unwrap_or_else(|| panic!("{id} is not a colour scale"))
}

/// Returns the ticks and tick labels of a scale of a kind that has them.
#[track_caller]
fn ticks_of(fig: &Figure, id: NodeId) -> (Ticks, TickLabels) {
    match &scale(fig, id).kind {
        ColorScaleKind::Continuous(s) => (s.ticks.clone(), s.tick_labels.clone()),
        ColorScaleKind::Discrete(s) => (s.ticks.clone(), s.tick_labels.clone()),
        ColorScaleKind::Indexed(s) => (s.ticks.clone(), s.tick_labels.clone()),
        ColorScaleKind::Categorical(_) => panic!("a categorical scale has no ticks"),
    }
}

/// A field over a small grid, for images that name a scale.
fn field() -> Matrix {
    Matrix::from_fn(3, 4, |row, col| (row * 4 + col) as f64)
}

// ---------------------------------------------------------------------------------
// Panel colourbars
// ---------------------------------------------------------------------------------

// PROMISE: #88 in three lines: a scale shared by a row of plots is shown once, by a bar in a
// panel spanning the row, which is vertical unless the author turns it, and the figure is
// valid and lays out without a warning.
#[test]
fn a_spanning_panel_shows_a_shared_scale_as_one_bar() {
    let mut fig = Figure::new().tiles(2, 3).fit_row(1);
    let shared = fig
        .color_scale()
        .limits(0.0, 11.0)
        .title("Speed (m/s)")
        .id();
    for col in 0..3 {
        fig.axes(0, col).scalar_image(field()).color_scale(shared);
    }
    let bar = fig
        .panel_span(1, 0, 1, 3)
        .colorbar(shared)
        .orientation(ColorbarOrientation::Horizontal)
        .id();
    assert_eq!(
        panel_colorbar(&fig, bar),
        PanelColorbar {
            scale: shared,
            orientation: ColorbarOrientation::Horizontal,
            ..PanelColorbar::default()
        }
    );
    assert_eq!(
        panel(&fig, bar).cell,
        Cell {
            row: 1,
            col: 0,
            row_span: 1,
            col_span: 3
        }
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
    assert_eq!(fig.layout_warnings(), vec![]);
}

// PROMISE: a bar is vertical unless told otherwise, the identifier the colourbar handle
// gives is the panel's, and the panel keeps the alignment set before its content.
#[test]
fn a_panel_colourbar_is_vertical_by_default_and_its_id_is_the_panel_s() {
    let mut fig = Figure::new().tiles(1, 2).fit_col(1);
    let s = fig.color_scale().id();
    let id = fig.panel(0, 1).align(PanelLocation::West).colorbar(s).id();
    assert_eq!(fig.panel(0, 1).id(), id);
    assert_eq!(
        panel_colorbar(&fig, id).orientation,
        ColorbarOrientation::Vertical
    );
    assert_eq!(panel(&fig, id).align, PanelLocation::West);
}

// PROMISE: one panel holds one key, so asking a legend panel for a colourbar replaces its
// legend, and asking a colourbar panel for a legend gives the default legend; asking a
// colourbar panel for a colourbar again keeps how it was turned and shows the new scale, as
// asking for a legend again keeps the legend's settings.
#[test]
fn colorbar_and_legend_replace_content_of_the_other_kind_and_keep_their_own() {
    let mut fig = Figure::new().tiles(1, 2);
    let first = fig.color_scale().id();
    let second = fig.color_scale().id();
    let id = fig.panel(0, 1).legend().boxed(false).id();

    fig.panel(0, 1)
        .colorbar(first)
        .orientation(ColorbarOrientation::Horizontal);
    assert_eq!(
        panel_colorbar(&fig, id),
        PanelColorbar {
            scale: first,
            orientation: ColorbarOrientation::Horizontal,
            ..PanelColorbar::default()
        }
    );

    fig.panel(0, 1).colorbar(second);
    assert_eq!(
        panel_colorbar(&fig, id),
        PanelColorbar {
            scale: second,
            orientation: ColorbarOrientation::Horizontal,
            ..PanelColorbar::default()
        },
        "the orientation is kept"
    );

    fig.panel(0, 1).legend();
    assert_eq!(
        panel(&fig, id).content,
        PanelContent::Legend(PanelLegend::default()),
        "a colourbar is replaced by the default legend"
    );
    assert_eq!(fig.ir().panels.len(), 1);
}

// PROMISE: the facade never panics on a value the IR refuses; a colourbar given an
// identifier that is not a scale reaches validation, which names the panel.
#[test]
fn a_colourbar_of_something_that_is_not_a_scale_is_reported_by_validate() {
    let mut fig = Figure::new().tiles(1, 2);
    let ax = fig.axes(0, 0).id();
    let id = fig.panel(0, 1).colorbar(ax).id();
    assert!(has_error_at(&fig.validate(), IssueKind::WrongNodeKind, id));
}

// ---------------------------------------------------------------------------------
// Inside colourbars
// ---------------------------------------------------------------------------------

// PROMISE: an axes shows a scale inside its plot area by naming the scale and the edge;
// the bar is four fifths of that edge unless the author says otherwise.
#[test]
fn ax_colorbar_shows_a_scale_along_an_edge_of_the_plot_area() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.surf([0.0, 1.0, 2.0, 3.0], [0.0, 1.0, 2.0], field());
    let s = ax.color_scale().id();
    ax.colorbar(s, InsideColorbarSide::East);
    let id = ax.id();
    assert_eq!(
        inside_colorbar(&fig, id),
        Some(InsideColorbar {
            scale: s,
            side: InsideColorbarSide::East,
            length: 0.8,
            ..InsideColorbar::default()
        })
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: the length is set on the handle the call returns, chained like every setter, and
// a second call moves the bar to another edge or scale without forgetting its length.
#[test]
fn the_length_of_an_inside_colourbar_is_set_and_kept_when_the_bar_moves() {
    let mut fig = Figure::new();
    let first = fig.color_scale().id();
    let second = fig.indexed_scale([RED, GREEN, BLUE]).id();
    let id = fig.axes(0, 0).id();
    fig.axes(0, 0)
        .colorbar(first, InsideColorbarSide::South)
        .length(0.5);
    fig.axes(0, 0).colorbar(second, InsideColorbarSide::West);
    assert_eq!(
        inside_colorbar(&fig, id),
        Some(InsideColorbar {
            scale: second,
            side: InsideColorbarSide::West,
            length: 0.5,
            ..InsideColorbar::default()
        })
    );
}

// PROMISE: an unusable length is not a panic but a validation error naming the axes, as
// every value the IR refuses is.
#[test]
fn an_unusable_length_is_reported_by_validate() {
    let mut fig = Figure::new();
    let s = fig.color_scale().id();
    let id = fig.axes(0, 0).id();
    fig.axes(0, 0)
        .colorbar(s, InsideColorbarSide::North)
        .length(1.5);
    assert!(has_error_at(
        &fig.validate(),
        IssueKind::InvalidColorbarLength,
        id
    ));
}

// PROMISE: a program that reloads a saved figure keeps building on its colourbars: the
// handles of the reloaded figure address the same panel and the same inside colourbar, so
// a second call changes them in place, keeping how the bar was turned and how long it is,
// rather than adding a panel or forgetting a setting. (That the IR round-trips exactly is
// the IR tests' promise.)
#[test]
fn the_handles_of_a_reloaded_figure_change_its_colourbars_in_place() {
    let mut fig = Figure::new()
        .tiles(1, 2)
        .col_weights([TrackSize::Weight(1.0), TrackSize::Fit]);
    let first = fig.color_scale().limits(0.0, 1.0).id();
    let second = fig.color_scale().limits(0.0, 2.0).id();
    let ax = fig.axes(0, 0).id();
    fig.axes(0, 0)
        .colorbar(first, InsideColorbarSide::West)
        .length(0.25);
    let bar = fig
        .panel(0, 1)
        .colorbar(first)
        .orientation(ColorbarOrientation::Horizontal)
        .id();
    for (name, restored) in [
        (
            "JSON",
            ironlab::ir::Figure::from_json(&fig.ir().to_json()).unwrap(),
        ),
        (
            "protobuf",
            Figure::from_protobuf(&fig.to_protobuf())
                .unwrap()
                .ir()
                .clone(),
        ),
    ] {
        let mut reloaded = Figure::from_ir(restored);
        assert_eq!(reloaded.panel(0, 1).colorbar(second).id(), bar, "{name}");
        reloaded
            .axes(0, 0)
            .colorbar(second, InsideColorbarSide::North);
        assert_eq!(reloaded.ir().panels.len(), 1, "{name}");
        assert_eq!(
            panel_colorbar(&reloaded, bar),
            PanelColorbar {
                scale: second,
                orientation: ColorbarOrientation::Horizontal,
                ..PanelColorbar::default()
            },
            "{name}"
        );
        assert_eq!(
            inside_colorbar(&reloaded, ax),
            Some(InsideColorbar {
                scale: second,
                side: InsideColorbarSide::North,
                length: 0.25,
                ..InsideColorbar::default()
            }),
            "{name}"
        );
    }
}

// ---------------------------------------------------------------------------------
// The ticks of a scale
// ---------------------------------------------------------------------------------

// PROMISE: the ticks of every bar of a scale are the scale's, set like an axis's: explicit
// values, a regular step from zero, or hidden labels that keep the ticks; each reaches the
// kind of the scale it is called on.
#[test]
fn tick_setters_set_the_ticks_of_the_scale() {
    let mut fig = Figure::new();
    let c = fig.color_scale().ticks([0.0, 0.5, 1.0]).id();
    assert_eq!(
        ticks_of(&fig, c),
        (Ticks::Values(vec![0.0, 0.5, 1.0]), TickLabels::Auto)
    );

    let d = fig
        .color_scale()
        .discrete([0.0, 1.0, 5.0, 10.0])
        .tick_step(2.5)
        .id();
    assert_eq!(
        ticks_of(&fig, d).0,
        Ticks::Step {
            step: 2.5,
            origin: 0.0
        }
    );

    let i = fig
        .indexed_scale([RED, GREEN, BLUE])
        .ticks([0.0, 2.0])
        .tick_labels_hidden()
        .id();
    assert_eq!(
        ticks_of(&fig, i),
        (Ticks::Values(vec![0.0, 2.0]), TickLabels::Hidden)
    );
}

// PROMISE: the plan's example: a 256-entry palette labels a few of its entries by name with
// value-label pairs, which set the ticks at those entries and label each, in one call.
#[test]
fn tick_labels_take_value_label_pairs_and_set_ticks_and_labels_together() {
    let mut fig = Figure::new();
    let terrain = fig
        .indexed_scale(Colormap::Gray.sample(256))
        .tick_labels([(0, "water"), (128, "land"), (255, "ice")])
        .id();
    assert_eq!(
        ticks_of(&fig, terrain),
        (
            Ticks::Values(vec![0.0, 128.0, 255.0]),
            TickLabels::Text(vec![
                Text::from("water"),
                Text::from("land"),
                Text::from("ice")
            ])
        )
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());

    // Values given as floats work the same on a continuous scale.
    let c = fig
        .color_scale()
        .limits(0.0, 1.0)
        .tick_labels([(0.0, "cold"), (1.0, "hot")])
        .id();
    assert_eq!(ticks_of(&fig, c).0, Ticks::Values(vec![0.0, 1.0]));
}

// PROMISE: a categorical scale labels every category itself and has no ticks, so a tick
// setter leaves it as it is rather than failing, as a setter of another kind always does.
#[test]
fn tick_setters_leave_a_categorical_scale_as_it_is() {
    let mut plain = Figure::new();
    let untouched = plain.categorical_scale([(RED, "a"), (GREEN, "b")]).id();

    let mut fig = Figure::new();
    let ticked = fig
        .categorical_scale([(RED, "a"), (GREEN, "b")])
        .ticks([0.0, 1.0])
        .tick_step(1.0)
        .tick_labels([(0, "x"), (1, "y")])
        .tick_labels_hidden()
        .id();
    assert_eq!(scale(&fig, ticked).kind, scale(&plain, untouched).kind);
}

// ---------------------------------------------------------------------------------
// The keys of a scale
// ---------------------------------------------------------------------------------

// PROMISE: whether a bar shows a key for out-of-range values is the author's choice, off by
// default; showing one keeps the category's policy, and a label replaces the default text.
#[test]
fn key_setters_show_keys_with_default_or_given_labels_and_keep_the_policy() {
    let mut fig = Figure::new();
    let s = fig
        .color_scale()
        .limits(0.0, 1.0)
        .above(RED)
        .show_key(ColorScaleKey::Below)
        .key_label(ColorScaleKey::Above, "too hot")
        .non_finite(OutOfRange::Transparent)
        .key_label(ColorScaleKey::NonFinite, "not entered")
        .id();
    let sc = scale(&fig, s);
    let ColorScaleKind::Continuous(c) = &sc.kind else {
        panic!("the scale is continuous");
    };
    assert!(c.below.show_key);
    assert_eq!(c.below.label, None);
    assert_eq!(
        c.below.policy,
        OutOfRange::Clamp,
        "the default policy is kept"
    );
    assert!(c.above.show_key);
    assert_eq!(c.above.label, Some(Text::from("too hot")));
    assert_eq!(c.above.policy, OutOfRange::Rgba { color: RED });
    assert!(sc.non_finite.show_key);
    assert_eq!(sc.non_finite.label, Some(Text::from("not entered")));
    assert_eq!(sc.non_finite.policy, OutOfRange::Transparent);
}

// PROMISE: an indexed or categorical scale has an outside key and no below or above key; a
// key the kind does not have leaves the scale as it is, as the policy setters do, so that a
// change of kind never leaves a stray key behind.
#[test]
fn a_key_the_kind_does_not_have_leaves_the_scale_as_it_is() {
    let mut fig = Figure::new();
    let s = fig
        .categorical_scale([(RED, "taken"), (GREEN, "skipped")])
        .show_key(ColorScaleKey::Below)
        .show_key(ColorScaleKey::Above)
        .show_key(ColorScaleKey::Outside)
        .id();
    let ColorScaleKind::Categorical(k) = &scale(&fig, s).kind else {
        panic!("the scale is categorical");
    };
    assert!(k.outside.show_key);
    assert_eq!(k.outside.policy, OutOfRange::Strict);

    let c = fig.color_scale().show_key(ColorScaleKey::Outside).id();
    let ColorScaleKind::Continuous(kind) = &scale(&fig, c).kind else {
        panic!("the scale is continuous");
    };
    assert!(!kind.below.show_key && !kind.above.show_key);
}
