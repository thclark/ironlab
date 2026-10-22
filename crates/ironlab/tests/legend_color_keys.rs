//! Colour keys in legends built through the user API (#24): a colour scale listed as an
//! item of a legend panel or of a legend inside an axes.
//!
//! The API these tests assume, beyond the IR of `crates/ironlab-ir/tests/legend_color_keys.rs`
//! (`LegendItemContent::ColorScale { scale, rows }` and `LegendItem::color_scale(NodeId)`, which
//! gives five rows):
//!
//! - `PanelLegendMut::item` keeps its name and its argument, anything that converts into a
//!   `LegendItem` or a bare `NodeId`, as the plan's facade example
//!   `fig.panel(2, 0).legend().item(status).item(scatter_id)` requires. A bare identifier is
//!   resolved against the figure when `item` is called: an identifier that names a colour
//!   scale of the figure appends `LegendItem::color_scale(id)`, and any other identifier,
//!   one that names no node included, appends `LegendItem::artist(id)` as before, so that
//!   validation reports a wrong identifier rather than the facade guessing. A `LegendItem`
//!   passed whole is appended as it is. A dedicated `color_scale_item` setter is not added,
//!   because the existing `item` already takes every kind of identifier, and an author who
//!   holds a scale identifier and an artist identifier should not have to remember which
//!   setter each needs. (The implementation may change the argument's trait, for example
//!   to a facade-local `impl Into<LegendItemRef>`, as long as these calls compile.)
//! - `PanelLegendMut::full_row` resolves its identifier as `item` does, so a scale on a row
//!   of its own is `full_row(scale)`, as an artist on a row of its own is `full_row(artist)`.
//!   The argument of `item` is a facade-local `LegendItemRef`, which a `NodeId` and a
//!   `LegendItem` convert into.
//! - A legend inside an axes lists a scale through the existing
//!   `AxesMut::legend_entries(LegendEntries::Explicit(..))`, which takes the items as they
//!   are; no inside-legend item setter is added.
//!
//! What a scale item draws belongs to the scene tests and what validation refuses to the IR
//! tests; these tests own the facade's promises.

mod common;

use common::has_error_at;
use ironlab::ir::{Legend, PanelContent, PanelLegend};
use ironlab::prelude::*;
use ironlab::{IssueKind, LegendItemContent};

const GREEN: Color = Color::rgb(0.0, 0.6, 0.3);
const RED: Color = Color::rgb(0.8, 0.2, 0.1);
const GREY: Color = Color::rgb(0.5, 0.5, 0.5);

/// Returns the legend that the panel with the given identifier holds.
#[track_caller]
fn legend(fig: &Figure, id: NodeId) -> &PanelLegend {
    match &fig
        .ir()
        .panel(id)
        .unwrap_or_else(|| panic!("figure has no panel {id}"))
        .content
    {
        PanelContent::Legend(legend) => legend,
        other => panic!("panel {id} holds {other:?}, not a legend"),
    }
}

/// Returns the items of the explicit list of the legend of a panel.
#[track_caller]
fn items(fig: &Figure, id: NodeId) -> Vec<LegendItem> {
    match &legend(fig, id).entries {
        LegendEntries::Explicit(items) => items.clone(),
        LegendEntries::Auto => panic!("panel {id} lists its entries automatically"),
    }
}

// PROMISE: the plan's example in two lines: a key of a classification and of a plot, built
// with the same `item` setter, lists the scale's entries and the plot's marker entry in the
// order of the calls, and the figure is valid and lays out without a warning.
#[test]
fn item_lists_a_scale_and_an_artist_in_call_order() {
    let mut fig = Figure::new().tiles(3, 1);
    let status = fig
        .categorical_scale([(GREEN, "taken"), (RED, "skipped")])
        .id();
    let codes = ByteMatrix::from_rows(&[[0, 1, 1], [1, 0, 0]]);
    fig.axes(0, 0).scalar_image(&codes).color_scale(status);
    let x = linspace(0.0, 1.0, 5);
    let scatter_id = fig.axes(1, 0).scatter(&x, &x).label("Measured").id();
    let key = fig.panel(2, 0).legend().item(status).item(scatter_id).id();
    assert_eq!(
        items(&fig, key),
        vec![
            LegendItem::color_scale(status),
            LegendItem::artist(scatter_id)
        ]
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert_eq!(report.warnings, vec![], "{report:?}");
    assert_eq!(fig.layout_warnings(), vec![]);
}

// PROMISE: `item` recognises a scale of every kind by its identifier, whichever constructor
// made it, so that a bar of a continuous, discrete or indexed scale and the entries of a
// categorical one are all listed the same way.
#[test]
fn item_recognises_a_scale_of_every_kind() {
    let mut fig = Figure::new().tiles(2, 1);
    let continuous = fig.color_scale().limits(0.0, 1.0).title("Speed (m/s)").id();
    let discrete = fig.color_scale().discrete([0.0, 1.0, 2.0]).id();
    let indexed = fig.indexed_scale([GREEN, RED, GREY]).id();
    let categorical = fig.categorical_scale([(GREEN, "converged")]).id();
    let key = fig
        .panel(1, 0)
        .legend()
        .item(continuous)
        .item(discrete)
        .item(indexed)
        .item(categorical)
        .id();
    let contents: Vec<LegendItemContent> = items(&fig, key)
        .into_iter()
        .map(|item| item.content)
        .collect();
    assert_eq!(
        contents,
        [continuous, discrete, indexed, categorical]
            .map(|scale| LegendItemContent::ColorScale { scale, rows: 5 })
            .to_vec()
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: a custom colour key (#24) is a scale that no artist uses, listed in a panel that
// no axes names; it is a normal figure, valid, without a warning from validation or from
// the layout.
#[test]
fn a_key_of_a_scale_that_no_artist_uses_is_a_normal_figure() {
    let mut fig = Figure::new().tiles(1, 2).fit_col(1);
    let x = linspace(0.0, 1.0, 5);
    fig.axes(0, 0).plot(&x, &x).label("Curve");
    let terrain = fig
        .indexed_scale([GREEN, RED, GREY])
        .tick_labels([(0, "water"), (1, "land"), (2, "ice")])
        .title("Terrain")
        .id();
    fig.panel(0, 1).legend().item(terrain);
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert_eq!(report.warnings, vec![], "{report:?}");
    assert_eq!(fig.layout_warnings(), vec![]);
}

// PROMISE: the meaning of `item` for anything that is not a scale is unchanged: an artist's
// identifier lists the artist, a whole `LegendItem` is appended as given (a scale item on a
// row of its own included), and an identifier that names nothing is left for validation to
// report as a dangling artist item rather than guessed at.
#[test]
fn item_keeps_its_meaning_for_artists_whole_items_and_unknown_identifiers() {
    let mut fig = Figure::new().tiles(2, 1);
    let x = linspace(0.0, 1.0, 5);
    let line = fig.axes(0, 0).plot(&x, &x).label("Curve").id();
    let speed = fig.color_scale().id();
    let heading = LegendItem {
        full_row: true,
        ..LegendItem::color_scale(speed)
    };
    let missing = NodeId(9_999);
    let key = fig
        .panel(1, 0)
        .legend()
        .item(heading.clone())
        .item(line)
        .item(missing)
        .id();
    assert_eq!(
        items(&fig, key),
        vec![
            heading,
            LegendItem::artist(line),
            LegendItem::artist(missing)
        ]
    );
    let report = fig.validate();
    assert!(
        has_error_at(&report, IssueKind::DanglingLegendItem, key),
        "{report:?}"
    );
}

// PROMISE: a scale is resolved when `item` is called, so the key lists exactly what the
// author named at that moment; and once listed, a scale item is kept when the rest of the
// legend is changed by the other setters.
#[test]
fn a_scale_item_survives_the_other_legend_setters() {
    let mut fig = Figure::new().tiles(2, 1);
    let speed = fig.color_scale().id();
    let key = fig
        .panel(1, 0)
        .legend()
        .item(speed)
        .columns(LegendEntryColumns::Count(2))
        .order(LegendEntryOrder::RowMajor)
        .boxed(false)
        .id();
    assert_eq!(items(&fig, key), vec![LegendItem::color_scale(speed)]);
}

// PROMISE: the key inside one plot may show any scale of the figure, through the explicit
// entries it already takes, beside the plot's own artists.
#[test]
fn an_inside_legend_lists_a_scale_through_its_explicit_entries() {
    let mut fig = Figure::new();
    let x = linspace(0.0, 1.0, 5);
    let status = fig
        .categorical_scale([(GREEN, "taken"), (RED, "skipped")])
        .id();
    let mut ax = fig.axes(0, 0);
    let line = ax.plot(&x, &x).label("Curve").id();
    ax.legend(LegendLocation::NorthWest)
        .legend_entries(LegendEntries::Explicit(vec![
            LegendItem::artist(line),
            LegendItem::color_scale(status),
        ]));
    let axes = fig.axes(0, 0).id();
    match &fig.ir().axes(axes).unwrap().legend {
        Some(Legend::Inside(inside)) => assert_eq!(
            inside.entries,
            LegendEntries::Explicit(vec![
                LegendItem::artist(line),
                LegendItem::color_scale(status)
            ])
        ),
        other => panic!("the axes has an inside legend, not {other:?}"),
    }
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert_eq!(report.warnings, vec![], "{report:?}");
    assert_eq!(fig.layout_warnings(), vec![]);
}

// PROMISE: a figure with a colour key reloads with its key, so that `Figure::from_ir`, which
// the viewer and every saved file go through, keeps a scale item a scale item.
#[test]
fn a_scale_item_survives_a_round_trip_through_the_ir() {
    let mut fig = Figure::new().tiles(2, 1);
    let status = fig.categorical_scale([(GREEN, "taken")]).id();
    let key = fig.panel(1, 0).legend().item(status).id();
    let reloaded = Figure::from_ir(fig.clone().into_ir());
    assert_eq!(items(&reloaded, key), vec![LegendItem::color_scale(status)]);
    assert_eq!(reloaded.ir(), fig.ir());
}

// PROMISE: `full_row` resolves an identifier as `item` does, so that a scale on a row of its
// own is written as an artist on a row of its own is: a scale identifier lists the scale on
// a row of its own, and an artist identifier the artist, as before.
#[test]
fn full_row_resolves_a_scale_as_item_does() {
    let mut fig = Figure::new().tiles(2, 1);
    let x = linspace(0.0, 1.0, 5);
    let line = fig.axes(0, 0).plot(&x, &x).label("Curve").id();
    let speed = fig.color_scale().limits(0.0, 1.0).title("Speed (m/s)").id();
    let status = fig.categorical_scale([(GREEN, "taken")]).id();
    let key = fig
        .panel(1, 0)
        .legend()
        .full_row(speed)
        .full_row(line)
        .full_row(status)
        .id();
    let row = |item: LegendItem| LegendItem {
        full_row: true,
        ..item
    };
    assert_eq!(
        items(&fig, key),
        vec![
            row(LegendItem::color_scale(speed)),
            row(LegendItem::artist(line)),
            row(LegendItem::color_scale(status)),
        ]
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert_eq!(fig.layout_warnings(), vec![]);
}
