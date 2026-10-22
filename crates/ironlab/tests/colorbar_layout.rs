//! The layout of colourbars, colour keys and panels built through the user API: where a
//! scale's title is drawn beside a bar, how long a panel colourbar is, how many rows a bar
//! item fills, and the title of a panel.
//!
//! The API these tests assume, beyond the IR of `crates/ironlab-ir/tests/colorbar_layout.rs`:
//!
//! ```text
//! impl PanelColorbarMut<'_> {
//!     /// Sets the length of the bar, keys included, as a fraction of the span of the plot
//!     /// boxes beside it; the default is automatic (the whole span standing, half lying).
//!     pub fn length(&mut self, fraction: f64) -> &mut Self;
//!     /// Sets where the scale's title is drawn beside the bar.
//!     pub fn title_placement(&mut self, placement: ColorbarTitlePlacement) -> &mut Self;
//! }
//! impl InsideColorbarMut<'_> {
//!     pub fn title_placement(&mut self, placement: ColorbarTitlePlacement) -> &mut Self;
//! }
//! impl PanelMut<'_> {
//!     /// Sets the heading drawn at the top of the panel's content.
//!     pub fn title(&mut self, title: impl Into<Text>) -> &mut Self;
//!     /// Sets how that heading is aligned with the content; the default is `Left`.
//!     pub fn title_align(&mut self, align: TitleAlign) -> &mut Self;
//! }
//! // A bar item of other than five rows is `LegendItem::color_scale(id).with_rows(n)`,
//! // passed to `PanelLegendMut::item` or listed in `AxesMut::legend_entries` as any item
//! // is; no setter of its own is added, as none was added for a scale item.
//! // The prelude exports `ColorbarTitlePlacement` beside `ColorbarOrientation`; `TitleAlign` is
//! // already exported.
//! ```
//!
//! The rules these tests pin:
//!
//! - A panel's title and its alignment are the panel's, not its content's, so they survive a
//!   switch between a legend and a colourbar, as the panel's alignment does.
//! - Pointing a panel's colourbar or an axes' colourbar at another scale keeps the length and
//!   the title placement the bar has, as it already keeps the orientation and the length.
//! - The facade sets what it is given; validation reports a length or a row count that
//!   cannot be drawn, against the panel.

mod common;

use common::has_error_at;
use ironlab::IssueKind;
use ironlab::ir::{InsideColorbar, Panel, PanelColorbar, PanelContent};
use ironlab::prelude::*;

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
fn inside_colorbar(fig: &Figure, id: NodeId) -> InsideColorbar {
    fig.ir()
        .axes(id)
        .unwrap_or_else(|| panic!("{id} is not an axes"))
        .colorbar
        .unwrap_or_else(|| panic!("{id} shows no colourbar"))
}

/// Returns the items of the explicit list of the legend of a panel.
#[track_caller]
fn items(fig: &Figure, id: NodeId) -> Vec<LegendItem> {
    match &panel(fig, id).content {
        PanelContent::Legend(legend) => match &legend.entries {
            LegendEntries::Explicit(items) => items.clone(),
            LegendEntries::Auto => panic!("panel {id} lists its entries automatically"),
        },
        other => panic!("panel {id} holds {other:?}, not a legend"),
    }
}

/// A field over a small grid, for images that name a scale.
fn field() -> Matrix {
    Matrix::from_fn(3, 4, |row, col| (row * 4 + col) as f64)
}

// PROMISE: a bar built through the facade has the settled defaults: an automatic length and
// the scale's title along the bar, in a panel and inside an axes; a panel has no title, and
// a title given to it is aligned with the left edge of its content unless the author says
// otherwise.
#[test]
fn bars_and_panels_built_through_the_facade_have_the_settled_defaults() {
    let mut fig = Figure::new().tiles(1, 2).fit_col(1);
    let s = fig.color_scale().title("Speed (m/s)").id();
    fig.axes(0, 0).scalar_image(field()).color_scale(s);
    let ax = fig.axes(0, 0).id();
    fig.axes(0, 0).colorbar(s, InsideColorbarSide::South);
    let bar = fig.panel(0, 1).colorbar(s).id();
    assert_eq!(panel_colorbar(&fig, bar).length, None);
    assert_eq!(
        panel_colorbar(&fig, bar).title_placement,
        ColorbarTitlePlacement::Along
    );
    assert_eq!(
        inside_colorbar(&fig, ax).title_placement,
        ColorbarTitlePlacement::Along
    );
    assert_eq!(panel(&fig, bar).title, None);
    assert_eq!(panel(&fig, bar).title_align, TitleAlign::Left);
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: the length and the title placement of a panel bar are set on the handle the call
// returns, chained like the orientation; a stated length is the fraction given.
#[test]
fn a_panel_bar_s_length_and_title_placement_are_set_on_its_handle() {
    let mut fig = Figure::new().tiles(2, 3).fit_row(1);
    let s = fig.color_scale().limits(0.0, 1.0).title("Speed (m/s)").id();
    for col in 0..3 {
        fig.axes(0, col).scalar_image(field()).color_scale(s);
    }
    let bar = fig
        .panel_span(1, 0, 1, 3)
        .colorbar(s)
        .orientation(ColorbarOrientation::Horizontal)
        .length(0.6)
        .title_placement(ColorbarTitlePlacement::Above)
        .id();
    assert_eq!(
        panel_colorbar(&fig, bar),
        PanelColorbar {
            scale: s,
            orientation: ColorbarOrientation::Horizontal,
            length: Some(0.6),
            title_placement: ColorbarTitlePlacement::Above,
        }
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: pointing a bar at another scale changes only its scale: a panel bar keeps its
// length and title placement, and an inside bar keeps its title placement, as each already
// keeps its orientation or its length.
#[test]
fn pointing_a_bar_at_another_scale_keeps_its_length_and_title_placement() {
    let mut fig = Figure::new().tiles(1, 2).fit_col(1);
    let first = fig.color_scale().limits(0.0, 1.0).id();
    let second = fig.color_scale().limits(0.0, 2.0).id();
    let ax = fig.axes(0, 0).id();
    fig.axes(0, 0)
        .colorbar(first, InsideColorbarSide::East)
        .title_placement(ColorbarTitlePlacement::Above);
    let bar = fig
        .panel(0, 1)
        .colorbar(first)
        .length(0.5)
        .title_placement(ColorbarTitlePlacement::Above)
        .id();
    fig.panel(0, 1).colorbar(second);
    fig.axes(0, 0).colorbar(second, InsideColorbarSide::West);
    let kept = panel_colorbar(&fig, bar);
    assert_eq!(kept.scale, second);
    assert_eq!(kept.length, Some(0.5));
    assert_eq!(kept.title_placement, ColorbarTitlePlacement::Above);
    let inside = inside_colorbar(&fig, ax);
    assert_eq!(inside.scale, second);
    assert_eq!(inside.side, InsideColorbarSide::West);
    assert_eq!(inside.title_placement, ColorbarTitlePlacement::Above);
}

// PROMISE: a panel's title heads whatever the panel holds, so it is set on the panel, and it
// and its alignment stay when the content is switched between a legend and a colourbar.
#[test]
fn a_panel_title_is_the_panel_s_and_survives_a_change_of_content() {
    let mut fig = Figure::new().tiles(1, 2);
    let s = fig.color_scale().id();
    let id = fig
        .panel(0, 1)
        .title("Key")
        .title_align(TitleAlign::Right)
        .legend()
        .id();
    assert_eq!(panel(&fig, id).title, Some(Text::from("Key")));
    assert_eq!(panel(&fig, id).title_align, TitleAlign::Right);
    fig.panel(0, 1).colorbar(s);
    assert_eq!(panel(&fig, id).title, Some(Text::from("Key")));
    assert_eq!(panel(&fig, id).title_align, TitleAlign::Right);
    fig.panel(0, 1).legend();
    assert_eq!(panel(&fig, id).title, Some(Text::from("Key")));
    assert_eq!(panel(&fig, id).title_align, TitleAlign::Right);
}

// PROMISE: a bar item of other than five rows is an item like any other, built with
// `with_rows` and appended by `item` in a panel or listed by `legend_entries` inside an
// axes, and the rows reach the figure unchanged.
#[test]
fn a_bar_item_of_other_rows_is_listed_as_any_item_is() {
    let mut fig = Figure::new().tiles(2, 1);
    let s = fig.color_scale().limits(0.0, 1.0).title("Speed").id();
    let key = fig
        .panel(1, 0)
        .legend()
        .item(LegendItem::color_scale(s).with_rows(8))
        .item(s)
        .id();
    assert_eq!(
        items(&fig, key),
        vec![
            LegendItem::color_scale(s).with_rows(8),
            LegendItem::color_scale(s),
        ]
    );
    let ax = fig.axes(0, 0).id();
    fig.axes(0, 0).legend_entries(LegendEntries::Explicit(vec![
        LegendItem::color_scale(s).with_rows(3),
    ]));
    match &fig.ir().axes(ax).unwrap().legend {
        Some(ironlab::ir::Legend::Inside(inside)) => assert_eq!(
            inside.entries,
            LegendEntries::Explicit(vec![LegendItem::color_scale(s).with_rows(3)])
        ),
        other => panic!("the axes shows an inside legend, not {other:?}"),
    }
}

// PROMISE: the facade never panics on a value the IR refuses: a panel bar given no length
// or a bar item given no rows reaches validation, which names the panel.
#[test]
fn an_unusable_length_or_row_count_is_reported_by_validate_against_the_panel() {
    let mut fig = Figure::new().tiles(1, 3);
    let s = fig.color_scale().id();
    let bar = fig.panel(0, 1).colorbar(s).length(0.0).id();
    let key = fig
        .panel(0, 2)
        .legend()
        .item(LegendItem::color_scale(s).with_rows(0))
        .id();
    let report = fig.validate();
    assert!(
        has_error_at(&report, IssueKind::InvalidColorbarLength, bar),
        "{report:?}"
    );
    assert!(
        has_error_at(&report, IssueKind::InvalidLegendRows, key),
        "{report:?}"
    );
}

// PROMISE: the layout choices are part of the figure, so a figure saved and reloaded through
// the facade's own formats keeps them.
#[test]
fn the_layout_choices_survive_a_save_and_reload() {
    let mut fig = Figure::new().tiles(1, 3);
    let s = fig.color_scale().title("Speed").id();
    fig.axes(0, 0).scalar_image(field()).color_scale(s);
    fig.axes(0, 0)
        .colorbar(s, InsideColorbarSide::East)
        .title_placement(ColorbarTitlePlacement::Above);
    fig.panel(0, 1)
        .title("Bar")
        .title_align(TitleAlign::Center)
        .colorbar(s)
        .length(0.4);
    fig.panel(0, 2)
        .title("Key")
        .legend()
        .item(LegendItem::color_scale(s).with_rows(7));
    let reloaded = Figure::from_protobuf(&fig.to_protobuf()).unwrap();
    assert_eq!(reloaded.ir(), fig.ir());
}
