//! Colour keys in legends (#24): a colour scale listed as an item of a legend, inside a
//! plot area or in a legend panel. These tests own the model: the item, its persistence in
//! both formats, the validation of the reference from the item to its scale, and the
//! refusal to remove a scale that a legend item names. What the item draws belongs to the
//! scene tests, and how the facade builds it to the facade tests.
//!
//! The IR these tests assume is the approved sketch of the colourbar epic plan, with these
//! details, which the sketch leaves open:
//!
//! ```text
//! // crates/ironlab-ir/src/legend.rs
//! pub enum LegendItemContent {
//!     Artist(NodeId),
//!     Custom { label: Text, sample: LegendSample },
//!     /// The colour scale with this node identifier: one entry per category of a
//!     /// categorical scale, and a vertical bar for any other kind.
//!     ColorScale(NodeId),                                   // new
//! }
//! impl LegendItem {
//!     /// An item, not on a row of its own, that shows the colour scale with this node
//!     /// identifier.
//!     pub fn color_scale(scale: NodeId) -> Self;           // new
//! }
//! // JSON: {"type": "color_scale", "value": <scale id>}, as the other variants are tagged.
//! // `From<NodeId> for LegendItem` still makes an artist item.
//!
//! // crates/ironlab-ir/src/validate.rs: no new issue kind.
//! //   A `ColorScale` item naming no node      -> UnknownColorScale, against the legend's owner
//! //   A `ColorScale` item naming another kind -> WrongNodeKind,     against the legend's owner
//! //   (the owner is the panel for a legend panel and the axes for an inside legend,
//! //   as for every other legend item).
//! ```
//!
//! The rules these tests pin:
//!
//! - **References.** A scale item names its scale as an artist or a colourbar does, so the
//!   kinds of issue are those of every scale reference (`UnknownColorScale`,
//!   `WrongNodeKind`), not `DanglingLegendItem`, which is about artists. A scale is a node of
//!   the figure rather than of an axes, so a legend inside a plot area may list any scale of
//!   the figure, whichever axes its artists belong to.
//! - **Removal.** Removing a scale that a legend item names is refused, as removing a scale
//!   that a colourbar names is: validation reports the dangling reference against the legend's
//!   owner, so the transaction is invalid unless the legend goes too.
//! - **Warnings.** A scale item is never `UnnamedLegendItem`, because a scale draws its own
//!   labels (category labels, ticks, its title); an untitled bar is a normal key. A repeated
//!   scale item is `DuplicateLegendItem`, as any repeated item is. A panel whose explicit
//!   list holds a scale item has something to draw, so it raises no `NothingToDraw`, even
//!   when no axes names the panel and no artist names the scale.
//! - **Edits.** The explicit list of a legend stays hidden from the edit registry
//!   (ADR 0020), so a scale item is built by code and never by a property path; the
//!   existing test of hidden paths in `edit_paths.rs` already covers it, because the
//!   kitchen sink's explicit panel then holds a scale item.

mod common;

use common::edits::{applied, set, tx};
use common::{
    FigureBuilder, categorical, continuous, discrete, indexed, kitchen_sink_figure, legend_panel,
    named, panel_legend_mut,
};
use ironlab_ir::*;
use serde_json::{Value as JsonValue, json};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

/// Returns whether a list of issues holds one of the given kind against the given node.
fn has(issues: &[ValidationIssue], kind: IssueKind, node: NodeId) -> bool {
    issues
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

/// Returns the kinds of the issues of a list, in the order reported.
fn kinds(issues: &[ValidationIssue]) -> Vec<IssueKind> {
    issues.iter().map(|issue| issue.kind).collect()
}

/// Returns an explicit list of the given items.
fn explicit(items: impl IntoIterator<Item = LegendItem>) -> LegendEntries {
    LegendEntries::Explicit(items.into_iter().collect())
}

/// A figure of one 2D axes in the first cell of a 1 × 2 layout holding a labelled scalar
/// image through a continuous scale, a categorical scale that no artist names, and a legend
/// panel in the second cell that no axes names and that lists the categorical scale.
struct Keys {
    fig: Figure,
    axes: NodeId,
    image: NodeId,
    data: NodeId,
    /// The continuous scale of the image.
    speed: NodeId,
    /// The categorical scale that no artist names.
    status: NodeId,
    panel: NodeId,
}

fn keys() -> Keys {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(1, 2);
    let axes = b.axes2d(0, 0);
    let speed = b.color_scale(continuous(named(ColormapName::Viridis), Limits::Auto));
    let status = b.color_scale(categorical(vec![RED, GREEN, BLUE], OutOfRange::Strict));
    let data = b.matrix(2, 2, |j, i| (j * 2 + i) as f64);
    let image = b.scalar_image(axes, data, Some(speed));
    if let Some(Artist::ScalarImage(image)) = b.axes(axes).artists.last_mut() {
        image.label = Some(Text::plain("Speed"));
    }
    let panel = b.node();
    b.fig.panels.push(legend_panel(
        panel,
        Cell {
            row: 0,
            col: 1,
            row_span: 1,
            col_span: 1,
        },
    ));
    let mut fig = b.build();
    panel_legend_mut(&mut fig.panels[0]).entries = explicit([LegendItem::color_scale(status)]);
    Keys {
        fig,
        axes,
        image,
        data,
        speed,
        status,
        panel,
    }
}

/// Returns an inside legend that lists the given entries.
fn inside_listing(entries: LegendEntries) -> Option<Legend> {
    Some(Legend::Inside(InsideLegend {
        entries,
        ..InsideLegend::default()
    }))
}

// ---------------------------------------------------------------------------------
// The model: the item and its persistence
// ---------------------------------------------------------------------------------

// Why: the constructor is how code names a scale in a key, and it must make an ordinary
// item, not on a row of its own, as the constructors of the other kinds of item do; and a
// bare identifier must still mean an artist, so that no existing call changes meaning.
#[test]
fn a_scale_item_is_an_ordinary_item_and_a_bare_identifier_still_names_an_artist() {
    let scale = NodeId(7);
    assert_eq!(
        LegendItem::color_scale(scale),
        LegendItem {
            content: LegendItemContent::ColorScale(scale),
            full_row: false,
        }
    );
    assert_eq!(LegendItem::from(scale), LegendItem::artist(scale));
}

// Why: clients in other languages write figures as JSON, so a scale item must be written in
// the documented form, tagged as the other kinds of item are, with a snake-case name.
#[test]
fn the_json_form_of_a_scale_item_is_tagged_as_the_other_items_are() {
    let k = keys();
    let value: JsonValue = serde_json::from_str(&k.fig.to_json()).unwrap();
    assert_eq!(
        value["panels"][0]["content"]["value"]["entries"],
        json!({
            "type": "explicit",
            "value": [
                { "content": { "type": "color_scale", "value": k.status.0 }, "full_row": false }
            ]
        })
    );
    // The documented form is also read back.
    let written = k.fig.to_json();
    assert_eq!(Figure::from_json(&written).unwrap(), k.fig);
}

// Why: a saved key must reload as it was, from both formats, in a panel and inside an
// axes, on a row of its own or not, among items of the other kinds; otherwise a figure would
// come back with its colour key gone or turned into an artist entry.
#[test]
fn scale_items_round_trip_through_json_and_protobuf_in_both_kinds_of_legend() {
    let mut k = keys();
    panel_legend_mut(&mut k.fig.panels[0]).entries = explicit([
        LegendItem::artist(k.image),
        LegendItem::color_scale(k.status),
        LegendItem {
            full_row: true,
            ..LegendItem::color_scale(k.speed)
        },
        LegendItem::custom(LegendSample::swatch(RED), "Hand-written"),
    ]);
    k.fig.axes[0].legend = inside_listing(explicit([
        LegendItem::color_scale(k.speed),
        LegendItem::artist(k.image),
    ]));
    assert!(k.fig.validate().is_valid(), "{:?}", k.fig.validate());
    let json = Figure::from_json(&k.fig.to_json()).expect("the JSON form reloads");
    assert_eq!(json, k.fig, "JSON round trip");
    let binary = Figure::from_protobuf(&k.fig.to_protobuf()).expect("the binary form reloads");
    assert_eq!(binary, k.fig, "protobuf round trip");
}

// Why: the kitchen-sink figure is what the encoding tests rely on to reach every field of
// the schema, so it must hold a scale item, and still reload exactly.
#[test]
fn the_kitchen_sink_figure_holds_a_scale_item_and_round_trips() {
    let fig = kitchen_sink_figure();
    assert_eq!(fig.validate().errors, vec![]);
    let mut entries: Vec<&LegendEntries> = fig
        .panels
        .iter()
        .filter_map(|p| match &p.content {
            PanelContent::Legend(legend) => Some(&legend.entries),
            PanelContent::Colorbar(_) => None,
        })
        .collect();
    entries.extend(fig.axes.iter().filter_map(|a| match &a.legend {
        Some(Legend::Inside(inside)) => Some(&inside.entries),
        _ => None,
    }));
    let named_scales: Vec<NodeId> = entries
        .iter()
        .filter_map(|e| match e {
            LegendEntries::Explicit(items) => Some(items),
            LegendEntries::Auto => None,
        })
        .flatten()
        .filter_map(|item| match item.content {
            LegendItemContent::ColorScale(scale) => Some(scale),
            _ => None,
        })
        .collect();
    assert!(
        !named_scales.is_empty(),
        "some legend of the kitchen sink lists a scale"
    );
    for scale in named_scales {
        assert!(fig.color_scale(scale).is_some(), "{scale} is a scale");
    }
    assert_eq!(Figure::from_json(&fig.to_json()).unwrap(), fig);
    assert_eq!(Figure::from_protobuf(&fig.to_protobuf()).unwrap(), fig);
}

// ---------------------------------------------------------------------------------
// References from legend items to scales
// ---------------------------------------------------------------------------------

// Why: a custom colour key (#24) lists a scale that no artist names, in a panel that no
// axes names, which is the normal state of a figure with such a key: it must raise neither
// an error nor a warning. A scale draws its own labels, so an untitled scale is not an
// unnamed item, and the panel has something to draw. Every kind of scale may be listed.
#[test]
fn a_panel_listing_scales_that_no_artist_names_is_valid_without_warnings() {
    let mut k = keys();
    let report = k.fig.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![], "{report:?}");

    let bands = {
        let id = NodeId(900);
        k.fig.color_scales.push(ColorScale {
            id,
            title: None,
            kind: discrete(&[0.0, 1.0, 2.0]),
            non_finite: common::out_of_range(OutOfRange::Transparent),
        });
        id
    };
    let palette = {
        let id = NodeId(901);
        k.fig.color_scales.push(ColorScale {
            id,
            title: None,
            kind: indexed(vec![RED, GREEN], OutOfRange::Strict),
            non_finite: common::out_of_range(OutOfRange::Transparent),
        });
        id
    };
    panel_legend_mut(&mut k.fig.panels[0]).entries = explicit(
        [k.status, k.speed, bands, palette]
            .into_iter()
            .map(LegendItem::color_scale),
    );
    let report = k.fig.validate();
    assert_eq!(report.errors, vec![], "every kind of scale");
    assert_eq!(report.warnings, vec![], "every kind of scale: {report:?}");
}

// Why: a scale is a node of the figure, not of an axes, so the key inside one plot may show
// a scale whatever axes its artists belong to, even one that no artist of its own axes names;
// unlike an artist item, which must name an artist of its own axes.
#[test]
fn an_inside_legend_may_list_any_scale_of_the_figure() {
    let mut k = keys();
    k.fig.panels.clear();
    k.fig.axes[0].legend = inside_listing(explicit([
        LegendItem::color_scale(k.status),
        LegendItem::color_scale(k.speed),
    ]));
    let report = k.fig.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![], "{report:?}");
}

// Why: a scale reference that names no node cannot be drawn, so it is reported, as the
// reference of an artist or a colourbar is; the error points at the legend's owner, which
// holds the reference, so that the viewer can select it, and it is an unknown scale rather
// than a dangling artist item, so that the author looks for a missing scale.
#[test]
fn a_scale_item_naming_no_node_is_unknown_color_scale_against_the_legend_s_owner() {
    let k = keys();
    let missing = NodeId(4_242);

    let mut fig = k.fig.clone();
    panel_legend_mut(&mut fig.panels[0]).entries = explicit([LegendItem::color_scale(missing)]);
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::UnknownColorScale, k.panel),
        "{report:?}"
    );
    assert!(
        !kinds(&report.errors).contains(&IssueKind::DanglingLegendItem),
        "{report:?}"
    );

    let mut fig = k.fig.clone();
    fig.panels.clear();
    fig.axes[0].legend = inside_listing(explicit([LegendItem::color_scale(missing)]));
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::UnknownColorScale, k.axes),
        "{report:?}"
    );
    assert!(
        !kinds(&report.errors).contains(&IssueKind::DanglingLegendItem),
        "{report:?}"
    );
}

// Why: in one identifier space an item can name a node of another kind (an axes, an artist,
// a data array, a panel, the figure), which must be reported as the wrong kind, not as an
// unknown scale, so the message tells the author what is wrong. An artist in particular is
// the commonest slip, because an artist item and a scale item both take a bare identifier.
#[test]
fn a_scale_item_naming_a_node_of_another_kind_is_wrong_node_kind() {
    let k = keys();
    for (what, target) in [
        ("an axes", k.axes),
        ("an artist", k.image),
        ("a data array", k.data),
        ("a panel", k.panel),
        ("the figure", k.fig.id),
    ] {
        let mut fig = k.fig.clone();
        panel_legend_mut(&mut fig.panels[0]).entries = explicit([LegendItem::color_scale(target)]);
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::WrongNodeKind, k.panel),
            "{what} in a panel: {report:?}"
        );
        assert!(
            !kinds(&report.errors).contains(&IssueKind::UnknownColorScale),
            "{what} in a panel: {report:?}"
        );

        // The panel is kept, so that it stays a node of the figure for the inside legend to
        // name; its own list names a valid scale.
        let mut fig = k.fig.clone();
        fig.axes[0].legend = inside_listing(explicit([LegendItem::color_scale(target)]));
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::WrongNodeKind, k.axes),
            "{what} inside an axes: {report:?}"
        );
    }

    // The converse slip: an artist item naming a scale is the wrong kind of node too, as it
    // already is for any node that is not an artist.
    let mut fig = k.fig.clone();
    panel_legend_mut(&mut fig.panels[0]).entries = explicit([LegendItem::artist(k.status)]);
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::WrongNodeKind, k.panel),
        "an artist item naming a scale: {report:?}"
    );
}

// Why: listing a scale twice draws its key twice, which is almost always a mistake in the
// code that built the list, but the legend can still be drawn, so it is the warning that
// any repeated item is (ADR 0020), whether or not one of the two is on a row of its own;
// a scale item and an artist item of a scale's artist are different items.
#[test]
fn a_repeated_scale_item_is_a_warning_and_the_figure_stays_valid() {
    let k = keys();
    let mut fig = k.fig.clone();
    panel_legend_mut(&mut fig.panels[0]).entries = explicit([
        LegendItem::color_scale(k.status),
        LegendItem {
            full_row: true,
            ..LegendItem::color_scale(k.status)
        },
    ]);
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(
        has(&report.warnings, IssueKind::DuplicateLegendItem, k.panel),
        "{report:?}"
    );
    assert_eq!(report.warnings.len(), 1, "{report:?}");

    let mut fig = k.fig.clone();
    panel_legend_mut(&mut fig.panels[0]).entries = explicit([
        LegendItem::color_scale(k.speed),
        LegendItem::artist(k.image),
    ]);
    assert_eq!(
        fig.validate().warnings,
        vec![],
        "a scale and an artist it colours"
    );
}

// Why: a categorical scale with no categories is already refused against the scale
// (stage 2), whether or not anything shows it; listing it in a key must not add a second,
// misleading report against the legend, whose reference is sound.
#[test]
fn a_scale_item_naming_an_empty_categorical_scale_is_reported_against_the_scale_only() {
    let mut k = keys();
    if let ColorScaleKind::Categorical(c) = &mut k
        .fig
        .color_scales
        .iter_mut()
        .find(|s| s.id == k.status)
        .unwrap()
        .kind
    {
        c.categories.clear();
    }
    let report = k.fig.validate();
    assert!(
        has(&report.errors, IssueKind::InvalidColorScale, k.status),
        "{report:?}"
    );
    assert!(
        report
            .errors
            .iter()
            .chain(&report.warnings)
            .all(|issue| issue.node != Some(k.panel)),
        "nothing is reported against the legend: {report:?}"
    );
}

// ---------------------------------------------------------------------------------
// Removing a scale that a legend lists
// ---------------------------------------------------------------------------------

// Why: removing a scale that a key shows would leave an item with nothing to show, so it is
// refused as removing a scale that an artist or a colourbar names is; removing the legend
// with it in the same transaction is accepted, and undo restores both exactly. The explicit
// list is hidden from the registry, so the legend goes as a whole: the panel is removed,
// and an inside legend is set absent.
#[test]
fn removing_a_scale_that_a_legend_item_names_is_refused_unless_the_legend_goes_too() {
    let mut k = keys();
    k.fig.axes[0].legend = inside_listing(explicit([LegendItem::color_scale(k.status)]));
    assert!(k.fig.validate().is_valid(), "{:?}", k.fig.validate());

    let mut edited = k.fig.clone();
    let result = edited.apply(&tx([Edit::Remove { node: k.status }]));
    let Err(EditError::Invalid(issues)) = &result else {
        panic!("removing the scale was not refused: {result:?}");
    };
    assert!(
        has(issues, IssueKind::UnknownColorScale, k.panel),
        "{issues:?}"
    );
    assert!(
        has(issues, IssueKind::UnknownColorScale, k.axes),
        "{issues:?}"
    );
    assert_eq!(edited, k.fig, "a refused transaction changes nothing");

    let both = tx([
        Edit::Remove { node: k.panel },
        set(k.axes, "legend", Value::Unset),
        Edit::Remove { node: k.status },
    ]);
    let (removed, inverse) = applied(&k.fig, &both).unwrap();
    assert!(removed.color_scale(k.status).is_none());
    assert!(removed.panels.is_empty());
    assert_eq!(removed.axes[0].legend, None);
    assert_eq!(applied(&removed, &inverse).unwrap().0, k.fig);
}
