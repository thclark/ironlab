//! The layout of colourbars, colour keys and panels: where a colourbar's title is drawn, how
//! long a panel colourbar is along its span, how many entry rows a bar item in a legend fills,
//! and the title of a panel. These tests own the model of those choices: the defaults, the
//! documented JSON forms, persistence in both formats, validation, and the properties that the
//! edit registry offers. What each choice draws belongs to the scene tests
//! (`crates/ironlab-scene/tests/compile/colorbar_layout.rs`) and how the facade sets it to the
//! facade tests (`crates/ironlab/tests/colorbar_layout.rs`).
//!
//! The IR these tests assume:
//!
//! ```text
//! // crates/ironlab-ir/src/panel.rs
//! /// Where the title of a colour scale is drawn beside a colourbar of the scale.
//! #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
//! /// The field is named `title_placement`, not `title`, because a `title` is drawn text
//! /// (ADR 0023) and this is where the scale's title is drawn.
//! #[serde(rename_all = "snake_case")]
//! pub enum ColorbarTitlePlacement {
//!     /// Beyond the tick labels, on their side of the bar, centred on its length: on a
//!     /// standing bar rotated a quarter turn counter-clockwise, reading from bottom to top.
//!     #[default]
//!     Along,
//!     /// As a heading beside the bar: above a standing bar (above a standing panel bar, in
//!     /// the margin above the first row the panel spans) and above a lying panel bar; on a
//!     /// lying bar inside an axes, whose tick labels face the interior of the plot area, on
//!     /// the side of the bar away from them, so below a bar along the south edge and above
//!     /// a bar along the north edge.
//!     Above,
//! }
//! #[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
//! pub struct PanelColorbar {
//!     pub scale: NodeId,
//!     pub orientation: ColorbarOrientation,
//!     /// The length of the bar, its keys included, as a fraction of the span of the drawn
//!     /// plot boxes beside it, or `None` for the automatic length: the whole span for a
//!     /// standing bar and half of it for a lying one. A fraction is a finite number greater
//!     /// than 0 and at most 1. The bar is placed along the span by the panel's `align`.
//!     pub length: Option<f64>,                       // new; JSON null when automatic
//!     pub title_placement: ColorbarTitlePlacement,            // new
//! }
//! pub struct Panel {
//!     pub id: NodeId,
//!     pub cell: Cell,
//!     pub align: PanelLocation,
//!     /// A heading at the top of the panel's content, or `None`.
//!     pub title: Option<Text>,                       // new; JSON null when absent
//!     /// How the title is aligned with the content; a panel built by the facade or a
//!     /// fixture aligns it `Left`.
//!     pub title_align: TitleAlign,                   // new (the enum of `axes.rs`)
//!     pub content: PanelContent,
//! }
//!
//! // crates/ironlab-ir/src/axes.rs
//! pub struct InsideColorbar {
//!     pub scale: NodeId,
//!     pub side: InsideColorbarSide,
//!     pub length: f64,
//!     pub title_placement: ColorbarTitlePlacement,            // new; default Along
//! }
//!
//! // crates/ironlab-ir/src/legend.rs
//! pub enum LegendItemContent {
//!     Artist(NodeId),
//!     Custom { label: Text, sample: LegendSample },
//!     /// `rows`: the number of entry rows along which a standing bar of a continuous,
//!     /// discrete or indexed scale runs; at least 1. It is ignored by a categorical scale,
//!     /// which lists one entry per category, and by an item on a row of its own, whose bar
//!     /// lies across the legend; it is validated whatever the kind or the row.
//!     ColorScale { scale: NodeId, rows: u32 },       // was ColorScale(NodeId)
//! }
//! // JSON: {"type": "color_scale", "value": {"scale": <id>, "rows": 5}}, the form of the
//! // struct variant `Custom`.
//! impl LegendItem {
//!     pub fn color_scale(scale: NodeId) -> Self;     // rows 5, as before
//!     /// Returns the item with the bar of a colour-scale item `rows` entry rows long; an
//!     /// item of any other content is returned unchanged.
//!     pub fn with_rows(self, rows: u32) -> Self;     // new
//! }
//! pub const DEFAULT_BAR_ROWS: u32 = 5;               // not required; tests use 5
//!
//! // crates/ironlab-ir/src/validate.rs
//! IssueKind::InvalidColorbarLength  // now also: the length of a panel colourbar is not
//!                                   // `None` or a fraction; reported against the panel
//! IssueKind::InvalidLegendRows      // new: a colour-scale item of a legend has `rows` of 0;
//!                                   // reported against the legend's owner (the panel, or the
//!                                   // axes of an inside legend), as InvalidLegendColumns is
//!
//! // crates/ironlab-ir/src/edit: Value and ValueType gain ColorbarTitlePlacement; choices offer
//! // Along and Above. Registry paths:
//! //   Panel: "title" (Text, optional), "title_align" (TitleAlign),
//! //          "content.colorbar.length" (Double, optional), "content.colorbar.title_placement"
//! //          (ColorbarTitlePlacement)
//! //   Axes:  "colorbar.title_placement" (ColorbarTitlePlacement)
//!
//! // crates/ironlab-ir/src/lib.rs re-exports ColorbarTitlePlacement at the crate root.
//! ```
//!
//! The rules these tests pin, beyond those stated above:
//!
//! - **One rule for two lengths.** A panel colourbar's length is a fraction of the span beside
//!   it exactly as an inside colourbar's is a fraction of its edge, so the same values are
//!   accepted and refused, with the same issue kind, reported against the node that holds the
//!   bar.
//! - **Rows.** No existing kind covers a count of rows (`InvalidLegendColumns` names columns),
//!   so a new kind is added beside it, reported against the owner as columns are. A repeated
//!   scale item is still `DuplicateLegendItem` when the two items ask for different rows,
//!   because the legend then shows one scale twice.

mod common;

use common::edits::{applied, path, set, tx};
use common::{
    FigureBuilder, categorical, continuous, kitchen_sink_figure, legend_panel, named,
    panel_legend_mut,
};
use ironlab_ir::*;
use serde_json::{Value as JsonValue, json};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);

/// Returns whether a list of issues holds one of the given kind against the given node.
fn has(issues: &[ValidationIssue], kind: IssueKind, node: NodeId) -> bool {
    issues
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

/// A figure of one 2D axes in the first cell of a 1 × 3 layout holding an image through a
/// continuous scale, a vertical colourbar panel of that scale in the second cell, titled and
/// with an inside colourbar of the scale on the axes, and a legend panel in the third cell that
/// lists the scale as a bar item and a categorical scale that no artist names.
struct Layout {
    fig: Figure,
    axes: NodeId,
    speed: NodeId,
    status: NodeId,
    bar: NodeId,
    key: NodeId,
}

fn layout() -> Layout {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(1, 3);
    let axes = b.axes2d(0, 0);
    let speed = b.color_scale(continuous(named(ColormapName::Viridis), Limits::Auto));
    let status = b.color_scale(categorical(vec![RED, GREEN], OutOfRange::Strict));
    let data = b.matrix(2, 2, |j, i| (j * 2 + i) as f64);
    b.scalar_image(axes, data, Some(speed));
    b.axes(axes).colorbar = Some(InsideColorbar {
        scale: speed,
        ..InsideColorbar::default()
    });
    let bar = b.node();
    b.fig.panels.push(Panel {
        id: bar,
        cell: Cell {
            row: 0,
            col: 1,
            row_span: 1,
            col_span: 1,
        },
        align: PanelLocation::Center,
        title: None,
        title_align: TitleAlign::Left,
        content: PanelContent::Colorbar(PanelColorbar {
            scale: speed,
            ..PanelColorbar::default()
        }),
    });
    let key = b.node();
    let mut panel = legend_panel(
        key,
        Cell {
            row: 0,
            col: 2,
            row_span: 1,
            col_span: 1,
        },
    );
    panel_legend_mut(&mut panel).entries = LegendEntries::Explicit(vec![
        LegendItem::color_scale(speed),
        LegendItem::color_scale(status),
    ]);
    b.fig.panels.push(panel);
    Layout {
        fig: b.build(),
        axes,
        speed,
        status,
        bar,
        key,
    }
}

/// Returns the colourbar of the panel with the given identifier, found without the API under
/// test.
fn panel_colorbar_mut(fig: &mut Figure, id: NodeId) -> &mut PanelColorbar {
    match &mut fig
        .panels
        .iter_mut()
        .find(|p| p.id == id)
        .expect("the fixture panel exists")
        .content
    {
        PanelContent::Colorbar(colorbar) => colorbar,
        PanelContent::Legend(_) => panic!("panel {id} holds a legend"),
    }
}

/// Returns the panel with the given identifier, found without the API under test.
fn panel_mut(fig: &mut Figure, id: NodeId) -> &mut Panel {
    fig.panels
        .iter_mut()
        .find(|p| p.id == id)
        .expect("the fixture panel exists")
}

/// Returns the explicit list of items of the legend panel with the given identifier.
fn items_mut(fig: &mut Figure, id: NodeId) -> &mut Vec<LegendItem> {
    match &mut panel_legend_mut(panel_mut(fig, id)).entries {
        LegendEntries::Explicit(items) => items,
        LegendEntries::Auto => panic!("panel {id} lists its entries automatically"),
    }
}

/// Returns the property that the registry lists at a path for a kind of node.
#[track_caller]
fn property(kind: NodeKind, at: &str) -> Property {
    properties(kind)
        .into_iter()
        .find(|p| p.path.to_string() == at)
        .unwrap_or_else(|| panic!("{kind:?} lists no {at}"))
}

// ---------------------------------------------------------------------------------
// Defaults and the documented forms
// ---------------------------------------------------------------------------------

// Why: the user's review of the gallery settled that a scale's title reads along its bar by
// default, that a lying panel bar is half the span and a standing one the whole span unless
// the author says otherwise, and that a bar item runs five rows as it always has; a bar built
// with defaults must be that bar.
#[test]
fn the_layout_choices_have_the_settled_defaults() {
    assert_eq!(
        ColorbarTitlePlacement::default(),
        ColorbarTitlePlacement::Along
    );
    let panel = PanelColorbar::default();
    assert_eq!(panel.length, None, "a panel bar's length is automatic");
    assert_eq!(panel.title_placement, ColorbarTitlePlacement::Along);
    assert_eq!(
        InsideColorbar::default().title_placement,
        ColorbarTitlePlacement::Along
    );
    let item = LegendItem::color_scale(NodeId(7));
    assert_eq!(
        item.content,
        LegendItemContent::ColorScale {
            scale: NodeId(7),
            rows: 5,
        }
    );
    assert!(!item.full_row);
}

// Why: `with_rows` is how code asks for a longer or shorter bar item without spelling out the
// content; it must change only the rows of a scale item, and must not turn another kind of
// item into something else.
#[test]
fn with_rows_sets_the_rows_of_a_scale_item_and_leaves_other_items_alone() {
    let scale = NodeId(7);
    assert_eq!(
        LegendItem::color_scale(scale).with_rows(9),
        LegendItem {
            content: LegendItemContent::ColorScale { scale, rows: 9 },
            full_row: false,
        }
    );
    let full = LegendItem {
        full_row: true,
        ..LegendItem::color_scale(scale)
    }
    .with_rows(2);
    assert!(full.full_row, "the row of the item is kept");
    let artist = LegendItem::artist(scale);
    assert_eq!(artist.clone().with_rows(9), artist);
    let custom = LegendItem::custom(LegendSample::swatch(RED), "Hand-written");
    assert_eq!(custom.clone().with_rows(9), custom);
}

// Why: clients in other languages write figures as JSON, so each new choice must be written
// in a documented, snake-case form: the title placement as a string beside the other fields
// of a bar, an automatic length as null, the panel's title and alignment beside its cell, and
// a scale item's rows inside its tagged value, as the fields of a custom item are.
#[test]
fn the_json_forms_of_the_layout_choices_are_documented_snake_case() {
    let l = layout();
    let value: JsonValue = serde_json::from_str(&l.fig.to_json()).unwrap();
    assert_eq!(
        value["panels"][0]["content"],
        json!({
            "type": "colorbar",
            "value": {
                "scale": l.speed.0,
                "orientation": "vertical",
                "length": null,
                "title_placement": "along"
            }
        })
    );
    assert!(value["panels"][0]["title"].is_null(), "an untitled panel");
    assert_eq!(value["panels"][0]["title_align"], json!("left"));
    assert_eq!(
        value["axes"][0]["colorbar"],
        json!({ "scale": l.speed.0, "side": "south", "length": 0.8, "title_placement": "along" })
    );
    assert_eq!(
        value["panels"][1]["content"]["value"]["entries"]["value"][0],
        json!({
            "content": { "type": "color_scale", "value": { "scale": l.speed.0, "rows": 5 } },
            "full_row": false
        })
    );
    assert_eq!(
        serde_json::to_value(ColorbarTitlePlacement::Above).unwrap(),
        json!("above")
    );

    let mut fig = l.fig.clone();
    panel_colorbar_mut(&mut fig, l.bar).length = Some(0.5);
    let value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    assert_eq!(value["panels"][0]["content"]["value"]["length"], json!(0.5));
    // The documented forms are read back.
    assert_eq!(Figure::from_json(&fig.to_json()).unwrap(), fig);
}

// Why: a saved figure must reload with its titles where they were, its bars as long as they
// were and its bar items as many rows long, from both formats, for every value of every new
// field; otherwise a figure would come back rearranged.
#[test]
fn the_layout_choices_round_trip_through_json_and_protobuf() {
    let l = layout();
    let mut variants = Vec::new();
    for placement in [ColorbarTitlePlacement::Along, ColorbarTitlePlacement::Above] {
        for length in [None, Some(1.0), Some(0.375)] {
            for (align, panel_title) in [
                (TitleAlign::Left, None),
                (TitleAlign::Center, Some(Text::plain("Speed"))),
                (TitleAlign::Right, Some(Text::new(r"$\alpha$ (rad)"))),
            ] {
                let mut fig = l.fig.clone();
                let bar = panel_colorbar_mut(&mut fig, l.bar);
                bar.title_placement = placement;
                bar.length = length;
                bar.orientation = ColorbarOrientation::Horizontal;
                fig.axes[0].colorbar.as_mut().unwrap().title_placement = placement;
                for id in [l.bar, l.key] {
                    let panel = panel_mut(&mut fig, id);
                    panel.title = panel_title.clone();
                    panel.title_align = align;
                }
                items_mut(&mut fig, l.key)[0] = LegendItem::color_scale(l.speed).with_rows(12);
                variants.push(fig);
            }
        }
    }
    for fig in variants {
        assert_eq!(fig.validate().errors, vec![]);
        let json = Figure::from_json(&fig.to_json()).expect("the JSON form reloads");
        assert_eq!(json, fig, "JSON round trip");
        let binary = Figure::from_protobuf(&fig.to_protobuf()).expect("the binary form reloads");
        assert_eq!(binary, fig, "protobuf round trip");
    }
}

// Why: the kitchen-sink figure is what the encoding tests rely on to reach every field of the
// schema, so it must hold a titled panel, a panel colourbar of a stated length, a bar titled
// above and a bar item of other than five rows, and still reload exactly.
#[test]
fn the_kitchen_sink_figure_holds_every_layout_choice_and_round_trips() {
    let fig = kitchen_sink_figure();
    assert_eq!(fig.validate().errors, vec![]);
    assert!(
        fig.panels.iter().any(|p| p.title.is_some()),
        "a titled panel"
    );
    assert!(
        fig.panels.iter().any(|p| p.title_align != TitleAlign::Left),
        "a panel title aligned otherwise than the default"
    );
    let bars: Vec<PanelColorbar> = fig
        .panels
        .iter()
        .filter_map(|p| match &p.content {
            PanelContent::Colorbar(c) => Some(*c),
            PanelContent::Legend(_) => None,
        })
        .collect();
    assert!(bars.iter().any(|c| c.length.is_some()), "a stated length");
    assert!(
        bars.iter().any(|c| c.length.is_none()),
        "an automatic length"
    );
    let placements: Vec<ColorbarTitlePlacement> = bars
        .iter()
        .map(|c| c.title_placement)
        .chain(
            fig.axes
                .iter()
                .filter_map(|a| a.colorbar.map(|c| c.title_placement)),
        )
        .collect();
    assert!(placements.contains(&ColorbarTitlePlacement::Along));
    assert!(placements.contains(&ColorbarTitlePlacement::Above));
    let rows: Vec<u32> = fig
        .panels
        .iter()
        .filter_map(|p| match &p.content {
            PanelContent::Legend(legend) => Some(&legend.entries),
            PanelContent::Colorbar(_) => None,
        })
        .filter_map(|e| match e {
            LegendEntries::Explicit(items) => Some(items),
            LegendEntries::Auto => None,
        })
        .flatten()
        .filter_map(|item| match item.content {
            LegendItemContent::ColorScale { rows, .. } => Some(rows),
            _ => None,
        })
        .collect();
    assert!(
        rows.iter().any(|&r| r != 5),
        "a bar item of other than five rows: {rows:?}"
    );
    assert_eq!(Figure::from_json(&fig.to_json()).unwrap(), fig);
    assert_eq!(Figure::from_protobuf(&fig.to_protobuf()).unwrap(), fig);
}

// ---------------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------------

// Why: a panel bar's length is a fraction of the span beside it, so a bar of no length, of
// negative length or longer than the span cannot be drawn along it, and a length that is not
// a number places nothing; the whole span, a tiny fraction and the automatic length are all
// drawable. The rule is that of an inside bar, with the same issue kind, against the panel.
#[test]
fn the_length_of_a_panel_colourbar_is_automatic_or_a_fraction_of_its_span() {
    let l = layout();
    for length in [None, Some(1.0), Some(0.5), Some(1e-3)] {
        let mut fig = l.fig.clone();
        panel_colorbar_mut(&mut fig, l.bar).length = length;
        let report = fig.validate();
        assert_eq!(report.errors, vec![], "{length:?}");
    }
    for length in [
        0.0,
        -0.5,
        1.0 + 1e-9,
        2.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        let mut fig = l.fig.clone();
        panel_colorbar_mut(&mut fig, l.bar).length = Some(length);
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::InvalidColorbarLength, l.bar),
            "{length}: {report:?}"
        );
    }
}

// Why: a bar item of no rows has no length to draw its scale along; it is refused against
// the legend that lists it, a panel or the axes of an inside legend, as a legend of no
// columns is. One row is short but drawable.
#[test]
fn a_bar_item_of_no_rows_is_refused_against_its_legend() {
    let l = layout();
    let mut fig = l.fig.clone();
    items_mut(&mut fig, l.key)[0] = LegendItem::color_scale(l.speed).with_rows(1);
    assert_eq!(fig.validate().errors, vec![], "one row is drawable");

    let mut fig = l.fig.clone();
    items_mut(&mut fig, l.key)[0] = LegendItem::color_scale(l.speed).with_rows(0);
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::InvalidLegendRows, l.key),
        "{report:?}"
    );

    let mut fig = l.fig.clone();
    fig.axes[0].legend = Some(Legend::Inside(InsideLegend {
        entries: LegendEntries::Explicit(vec![LegendItem::color_scale(l.speed).with_rows(0)]),
        ..InsideLegend::default()
    }));
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::InvalidLegendRows, l.axes),
        "{report:?}"
    );
}

// Why: the rows are held by every scale item whatever it draws, and a later change of the
// scale's kind or of the item's row would make an ignored count count, so a count of zero is
// refused on a categorical item and on an item on a row of its own too.
#[test]
fn no_rows_is_refused_whatever_the_kind_of_scale_or_the_row_of_the_item() {
    let l = layout();
    for (what, item) in [
        (
            "a categorical item",
            LegendItem::color_scale(l.status).with_rows(0),
        ),
        (
            "an item on a row of its own",
            LegendItem {
                full_row: true,
                ..LegendItem::color_scale(l.speed)
            }
            .with_rows(0),
        ),
    ] {
        let mut fig = l.fig.clone();
        items_mut(&mut fig, l.key)[1] = item;
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::InvalidLegendRows, l.key),
            "{what}: {report:?}"
        );
    }
}

// Why: two items of one scale show it twice whatever their lengths, so asking for different
// rows does not make a repeated item a different item.
#[test]
fn two_items_of_one_scale_with_different_rows_are_still_a_repeated_item() {
    let l = layout();
    let mut fig = l.fig.clone();
    *items_mut(&mut fig, l.key) = vec![
        LegendItem::color_scale(l.speed),
        LegendItem::color_scale(l.speed).with_rows(8),
    ];
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(
        has(&report.warnings, IssueKind::DuplicateLegendItem, l.key),
        "{report:?}"
    );
}

// Why: a panel title is text drawn on the page like an axes title, so a titled panel of any
// content is valid and raises nothing; the title is not content, so it does not stop an empty
// legend panel from being reported as having nothing to draw.
#[test]
fn a_titled_panel_is_valid_and_a_title_alone_is_nothing_to_draw() {
    let l = layout();
    let mut fig = l.fig.clone();
    for id in [l.bar, l.key] {
        panel_mut(&mut fig, id).title = Some(Text::plain("Key"));
        panel_mut(&mut fig, id).title_align = TitleAlign::Right;
    }
    let report = fig.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![], "{report:?}");

    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(1, 2);
    b.axes2d(0, 0);
    let mut empty = legend_panel(
        NodeId(900),
        Cell {
            row: 0,
            col: 1,
            row_span: 1,
            col_span: 1,
        },
    );
    empty.title = Some(Text::plain("Nothing listed"));
    b.fig.panels.push(empty);
    let report = b.build().validate();
    assert!(
        has(&report.warnings, IssueKind::NothingToDraw, NodeId(900)),
        "{report:?}"
    );
}

// ---------------------------------------------------------------------------------
// Properties and edits
// ---------------------------------------------------------------------------------

// Why: the viewer's inspector edits a panel's title and alignment and a bar's title placement
// and length through the registry, so each must be listed at its path with its type; the
// panel title and the bar's length are optional, because no title and the automatic length
// are values an author chooses.
#[test]
fn the_registry_lists_the_layout_properties() {
    let title = property(NodeKind::Panel, "title");
    assert_eq!(title.value_type, ValueType::Text);
    assert!(title.optional);
    let align = property(NodeKind::Panel, "title_align");
    assert_eq!(align.value_type, ValueType::TitleAlign);
    assert!(!align.optional);
    let length = property(NodeKind::Panel, "content.colorbar.length");
    assert_eq!(length.value_type, ValueType::Double);
    assert!(length.optional, "the automatic length is no fraction");
    assert_eq!(
        property(NodeKind::Panel, "content.colorbar.title_placement").value_type,
        ValueType::ColorbarTitlePlacement
    );
    assert_eq!(
        property(NodeKind::Axes, "colorbar.title_placement").value_type,
        ValueType::ColorbarTitlePlacement
    );
}

// Why: titling a panel, aligning its title, moving a scale's title above its bar and fixing a
// bar's length are the edits a viewer makes; each is one set, read back by path, and undone
// exactly, and clearing the length restores the automatic one.
#[test]
fn the_layout_properties_are_set_by_path_and_undone() {
    let l = layout();
    let (edited, inverse) = applied(
        &l.fig,
        &tx([
            set(l.key, "title", Value::Text(Text::plain("Legend"))),
            set(l.key, "title_align", Value::TitleAlign(TitleAlign::Center)),
            set(
                l.bar,
                "content.colorbar.title_placement",
                Value::ColorbarTitlePlacement(ColorbarTitlePlacement::Above),
            ),
            set(l.bar, "content.colorbar.length", Value::Double(0.6)),
            set(
                l.axes,
                "colorbar.title_placement",
                Value::ColorbarTitlePlacement(ColorbarTitlePlacement::Above),
            ),
        ]),
    )
    .unwrap();
    assert_eq!(
        edited.get(l.key, &path("title")).unwrap(),
        Value::Text(Text::plain("Legend"))
    );
    assert_eq!(
        edited.get(l.key, &path("title_align")).unwrap(),
        Value::TitleAlign(TitleAlign::Center)
    );
    assert_eq!(
        edited.get(l.bar, &path("content.colorbar")).unwrap(),
        Value::PanelColorbar(PanelColorbar {
            scale: l.speed,
            orientation: ColorbarOrientation::Vertical,
            length: Some(0.6),
            title_placement: ColorbarTitlePlacement::Above,
        })
    );
    assert_eq!(
        edited.axes[0].colorbar.map(|c| c.title_placement),
        Some(ColorbarTitlePlacement::Above)
    );
    assert_eq!(applied(&edited, &inverse).unwrap().0, l.fig);

    let (cleared, _) = applied(
        &edited,
        &tx([set(l.bar, "content.colorbar.length", Value::Unset)]),
    )
    .unwrap();
    assert_eq!(
        cleared
            .get(l.bar, &path("content.colorbar.length"))
            .unwrap(),
        Value::Unset
    );
}

// Why: the property editor may type any number into a length, so an edit that makes a panel
// bar's length unusable is refused by validation, against the panel, and changes nothing.
#[test]
fn an_edit_setting_an_unusable_panel_bar_length_is_refused() {
    let l = layout();
    let mut edited = l.fig.clone();
    let result = edited.apply(&tx([set(
        l.bar,
        "content.colorbar.length",
        Value::Double(0.0),
    )]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues))
            if has(issues, IssueKind::InvalidColorbarLength, l.bar)),
        "{result:?}"
    );
    assert_eq!(edited, l.fig);
}

// Why: the title placement is offered as a combo box, which must offer both placements; and
// the panel's title alignment is the axes' enumeration, so it offers the same three.
#[test]
fn the_title_placement_offers_along_and_above() {
    let offered = choices(ValueType::ColorbarTitlePlacement, None);
    assert_eq!(offered.len(), 2, "{offered:?}");
    for placement in [ColorbarTitlePlacement::Along, ColorbarTitlePlacement::Above] {
        assert!(
            offered
                .iter()
                .any(|c| c.matches(&Value::ColorbarTitlePlacement(placement))),
            "{placement:?}"
        );
    }
    assert_eq!(choices(ValueType::TitleAlign, None).len(), 3);
}
