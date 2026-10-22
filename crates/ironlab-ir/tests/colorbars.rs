//! Colourbars (#8, #88): a colour scale shown in a panel of its own, or inside an axes along
//! one edge of its plot area. These tests own the model: the defaults, the references from
//! a colourbar to its scale and their validation, the length of an inside colourbar, the
//! place of a colourbar panel in the tile layout, persistence in both formats, and the
//! properties that the edit registry offers.
//!
//! The IR these tests assume is the approved sketch of the colourbar epic plan, with these
//! details, which the sketch leaves open:
//!
//! ```text
//! // crates/ironlab-ir/src/panel.rs
//! pub enum PanelContent { Legend(PanelLegend), Colorbar(PanelColorbar) }
//! #[derive(Debug, Clone, Copy, Default, PartialEq, ...)]
//! pub struct PanelColorbar { pub scale: NodeId, pub orientation: ColorbarOrientation, ... }
//! // (`length` and `title`, and the panel's `title` and `title_align`, belong to the layout
//! // stage; see `colorbar_layout.rs`.)
//! #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, ...)]
//! #[serde(rename_all = "snake_case")]
//! pub enum ColorbarOrientation { #[default] Vertical, Horizontal }
//!
//! // crates/ironlab-ir/src/axes.rs
//! pub struct Axes { ..., pub colorbar: Option<InsideColorbar>, ... }   // None by default
//! #[derive(Debug, Clone, Copy, PartialEq, ...)]
//! pub struct InsideColorbar { pub scale: NodeId, pub side: InsideColorbarSide, pub length: f64, ... }
//! impl Default for InsideColorbar      // scale NodeId::default(), side South, length 0.8
//! #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, ...)]
//! #[serde(rename_all = "snake_case")]
//! pub enum InsideColorbarSide { #[default] South, North, East, West }
//!
//! // crates/ironlab-ir/src/color_scale.rs: not part of the data model, never serialised
//! #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
//! pub enum ColorScaleKey { Below, Above, Outside, NonFinite }
//! impl ColorScale {
//!     /// The out-of-range category that a key of this kind shows, or `None` when the kind of
//!     /// the scale has no such category (below or above on an indexed or categorical scale,
//!     /// outside on a continuous or discrete one).
//!     pub fn key(&self, key: ColorScaleKey) -> Option<&ColorScaleOutOfRange>;
//! }
//!
//! // crates/ironlab-ir/src/validate.rs
//! IssueKind::InvalidColorbarLength   // new: the length of an inside colourbar is not a
//!                                    // finite number greater than 0 and at most 1;
//!                                    // reported against the axes
//!
//! // crates/ironlab-ir/src/lib.rs re-exports PanelColorbar, ColorbarOrientation,
//! // InsideColorbar, InsideColorbarSide and ColorScaleKey at the crate root.
//!
//! // crates/ironlab-ir/src/edit: Value and ValueType gain PanelColorbar,
//! // ColorbarOrientation, InsideColorbar and InsideColorbarSide; the panel content offers a
//! // "Colourbar" choice beside "Legend", and both enumerations offer their variants.
//! ```
//!
//! The rules these tests pin:
//!
//! - **References.** A colourbar names its scale as an artist does: a reference that names no
//!   node is `UnknownColorScale`, and one that names a node of another kind is `WrongNodeKind`.
//!   Each is reported against the node that holds the reference: the panel for a panel
//!   colourbar, the axes for an inside colourbar.
//! - **Removal.** Removing a scale that a colourbar names is refused, as removing a scale that
//!   an artist names is (stage 2): validation reports the dangling reference against the
//!   panel or the axes, so the transaction is invalid unless the colourbar goes too.
//! - **Length.** The length of an inside colourbar is a fraction of the edge it lies along: a
//!   finite number greater than 0 and at most 1, otherwise `InvalidColorbarLength` against
//!   the axes. No existing issue kind covers a fraction of an edge (`InvalidOpacity` and
//!   `InvalidAspect` each name one property), so the kind is new, as each of those is.
//! - **Layout.** A colourbar panel is placed, fitted and checked against the layout exactly as
//!   a legend panel is, but it always has something to draw, so it never raises
//!   `NothingToDraw`, even for a scale that no artist names.
//! - **Edits.** A panel's colourbar is reached below its content, as its legend is
//!   (`content.colorbar.scale`, `content.colorbar.orientation`), and an inside colourbar below
//!   the axes (`colorbar`, `colorbar.scale`, `colorbar.side`, `colorbar.length`). The panel's
//!   `content` itself is settable with choices, as it already is for a legend.

mod common;

use common::edits::{applied, path, set, tx};
use common::{FigureBuilder, continuous, indexed, kitchen_sink_figure, legend_panel, named};
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

/// Returns the kinds of the issues of a list, in the order reported.
fn kinds(issues: &[ValidationIssue]) -> Vec<IssueKind> {
    issues.iter().map(|issue| issue.kind).collect()
}

/// A figure of two 2D axes stacked in the first column of a 2 × 2 layout whose second
/// column is fitted, an image in the upper axes mapped through one continuous scale, a
/// vertical colourbar panel of that scale spanning both rows of the fitted column (#88), and
/// an inside colourbar of the same scale along the south edge of the lower axes.
struct Bars {
    fig: Figure,
    axes: [NodeId; 2],
    image: NodeId,
    scale: NodeId,
    panel: NodeId,
    data: NodeId,
}

fn bars() -> Bars {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout {
        col_weights: vec![TrackSize::Weight(1.0), TrackSize::Fit],
        ..TileLayout::new(2, 2)
    };
    let axes = [b.axes2d(0, 0), b.axes2d(1, 0)];
    let scale = b.color_scale(continuous(named(ColormapName::Viridis), Limits::Auto));
    let data = b.matrix(2, 2, |j, i| (j * 2 + i) as f64);
    let image = b.scalar_image(axes[0], data, Some(scale));
    let panel = b.node();
    b.fig.panels.push(Panel {
        id: panel,
        cell: Cell {
            row: 0,
            col: 1,
            row_span: 2,
            col_span: 1,
        },
        align: PanelLocation::Center,
        title: None,
        title_align: TitleAlign::Left,
        content: PanelContent::Colorbar(PanelColorbar {
            scale,
            orientation: ColorbarOrientation::Vertical,
            ..PanelColorbar::default()
        }),
    });
    b.axes(axes[1]).colorbar = Some(InsideColorbar {
        scale,
        side: InsideColorbarSide::South,
        length: 0.8,
        title_placement: ColorbarTitlePlacement::Along,
    });
    Bars {
        fig: b.build(),
        axes,
        image,
        scale,
        panel,
        data,
    }
}

/// Returns the colourbar of the panel with the given identifier, found without the API
/// under test.
fn panel_colorbar_mut(fig: &mut Figure, id: NodeId) -> &mut PanelColorbar {
    let panel = fig
        .panels
        .iter_mut()
        .find(|p| p.id == id)
        .expect("the fixture panel exists");
    match &mut panel.content {
        PanelContent::Colorbar(colorbar) => colorbar,
        PanelContent::Legend(_) => panic!("panel {id} holds a legend"),
    }
}

/// Returns the inside colourbar of the axes with the given identifier, found without the
/// API under test.
fn inside_colorbar_mut(fig: &mut Figure, id: NodeId) -> &mut InsideColorbar {
    fig.axes
        .iter_mut()
        .find(|a| a.id == id)
        .and_then(|a| a.colorbar.as_mut())
        .expect("the fixture axes holds a colourbar")
}

// ---------------------------------------------------------------------------------
// The model: defaults and persistence
// ---------------------------------------------------------------------------------

// Why: a colourbar built with defaults must be the familiar one: a vertical bar in a panel,
// and inside an axes a bar along the bottom edge four fifths of its length; and an axes
// built with defaults shows no bar, because a scale is shown only where the author asks.
#[test]
fn colourbars_have_the_defaults_the_model_promises() {
    assert_eq!(
        ColorbarOrientation::default(),
        ColorbarOrientation::Vertical
    );
    assert_eq!(
        PanelColorbar::default().orientation,
        ColorbarOrientation::Vertical
    );
    assert_eq!(InsideColorbarSide::default(), InsideColorbarSide::South);
    let inside = InsideColorbar::default();
    assert_eq!(inside.side, InsideColorbarSide::South);
    assert_eq!(inside.length, 0.8);
    assert_eq!(Axes::default().colorbar, None);
}

// Why: a saved colourbar must reload as it was, from both formats, in a panel and inside an
// axes, or a figure would come back with its bar moved, turned or gone.
#[test]
fn colourbars_round_trip_through_json_and_protobuf() {
    let mut fig = bars().fig;
    // Every orientation and every side, so that no variant is lost on the way.
    let other = NodeId(600);
    fig.panels.push(Panel {
        id: other,
        cell: Cell {
            row: 1,
            col: 1,
            row_span: 1,
            col_span: 1,
        },
        align: PanelLocation::SouthWest,
        title: None,
        title_align: TitleAlign::Left,
        content: PanelContent::Colorbar(PanelColorbar {
            scale: fig.color_scales[0].id,
            orientation: ColorbarOrientation::Horizontal,
            ..PanelColorbar::default()
        }),
    });
    for side in [
        InsideColorbarSide::South,
        InsideColorbarSide::North,
        InsideColorbarSide::East,
        InsideColorbarSide::West,
    ] {
        let mut sided = fig.clone();
        sided.axes[1].colorbar.as_mut().unwrap().side = side;
        sided.axes[1].colorbar.as_mut().unwrap().length = 0.375;
        let json = Figure::from_json(&sided.to_json()).expect("the JSON form reloads");
        assert_eq!(json, sided, "JSON round trip, {side:?}");
        let binary = Figure::from_protobuf(&sided.to_protobuf()).expect("the binary form reloads");
        assert_eq!(binary, sided, "protobuf round trip, {side:?}");
    }
}

// Why: the kitchen-sink figure is what the encoding tests rely on to reach every field of
// the schema, so it must hold colourbars of every variant, and still reload exactly.
#[test]
fn the_kitchen_sink_figure_holds_colourbars_of_every_variant_and_round_trips() {
    let fig = kitchen_sink_figure();
    assert_eq!(fig.validate().errors, vec![]);
    let orientations: Vec<ColorbarOrientation> = fig
        .panels
        .iter()
        .filter_map(|p| match &p.content {
            PanelContent::Colorbar(c) => Some(c.orientation),
            PanelContent::Legend(_) => None,
        })
        .collect();
    assert!(orientations.contains(&ColorbarOrientation::Vertical));
    assert!(orientations.contains(&ColorbarOrientation::Horizontal));
    let sides: Vec<InsideColorbarSide> = fig
        .axes
        .iter()
        .filter_map(|a| a.colorbar.map(|c| c.side))
        .collect();
    for side in [
        InsideColorbarSide::South,
        InsideColorbarSide::North,
        InsideColorbarSide::East,
        InsideColorbarSide::West,
    ] {
        assert!(sides.contains(&side), "{side:?}");
    }
    assert_eq!(Figure::from_json(&fig.to_json()).unwrap(), fig);
    assert_eq!(Figure::from_protobuf(&fig.to_protobuf()).unwrap(), fig);
}

// Why: clients in other languages write figures as JSON, so a colourbar must be written in
// the documented form: the content of a panel tagged as a legend is, and an inside colourbar
// as a plain object on the axes, with snake-case names.
#[test]
fn the_json_form_of_a_colourbar_follows_the_tagged_form_of_panel_content() {
    let s = bars();
    let value: JsonValue = serde_json::from_str(&s.fig.to_json()).unwrap();
    assert_eq!(
        value["panels"][0]["content"],
        json!({
            "type": "colorbar",
            "value": {
                "scale": s.scale.0,
                "orientation": "vertical",
                "length": null,
                "title_placement": "along"
            }
        })
    );
    assert_eq!(
        value["axes"][1]["colorbar"],
        json!({ "scale": s.scale.0, "side": "south", "length": 0.8, "title_placement": "along" })
    );
    assert!(
        value["axes"][0]["colorbar"].is_null(),
        "an axes without a colourbar writes none"
    );
    for (side, name) in [
        (InsideColorbarSide::North, "north"),
        (InsideColorbarSide::East, "east"),
        (InsideColorbarSide::West, "west"),
    ] {
        assert_eq!(serde_json::to_value(side).unwrap(), json!(name));
    }
    assert_eq!(
        serde_json::to_value(ColorbarOrientation::Horizontal).unwrap(),
        json!("horizontal")
    );
}

// ---------------------------------------------------------------------------------
// The keys of a scale
// ---------------------------------------------------------------------------------

// Why: the scene, the facade and the viewer each ask a scale for the category behind one of
// its keys; a key that the kind of the scale does not have must be absent rather than
// silently standing in for another, or an indexed bar would draw a "below" key for its
// outside values twice.
#[test]
fn a_scale_has_exactly_the_keys_of_its_kind() {
    let continuous_scale = ColorScale {
        kind: continuous(named(ColormapName::Viridis), Limits::Auto),
        ..ColorScale::default()
    };
    for key in [
        ColorScaleKey::Below,
        ColorScaleKey::Above,
        ColorScaleKey::NonFinite,
    ] {
        assert!(continuous_scale.key(key).is_some(), "{key:?}");
    }
    assert!(continuous_scale.key(ColorScaleKey::Outside).is_none());

    let indexed_scale = ColorScale {
        kind: indexed(vec![RED, GREEN], OutOfRange::Strict),
        ..ColorScale::default()
    };
    for key in [ColorScaleKey::Outside, ColorScaleKey::NonFinite] {
        assert!(indexed_scale.key(key).is_some(), "{key:?}");
    }
    for key in [ColorScaleKey::Below, ColorScaleKey::Above] {
        assert!(indexed_scale.key(key).is_none(), "{key:?}");
    }

    // Each key is the category of that name, not a neighbour.
    let mut keyed = continuous_scale.clone();
    if let ColorScaleKind::Continuous(c) = &mut keyed.kind {
        c.above.show_key = true;
    }
    assert!(keyed.key(ColorScaleKey::Above).unwrap().show_key);
    assert!(!keyed.key(ColorScaleKey::Below).unwrap().show_key);
}

// ---------------------------------------------------------------------------------
// References from colourbars to scales
// ---------------------------------------------------------------------------------

// Why: a colourbar that names a scale is the normal state of a figure with a key, and must
// raise neither an error nor a warning; this is the baseline the refusals below depart from.
#[test]
fn colourbars_naming_a_scale_are_valid_without_warnings() {
    let report = bars().fig.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![]);
}

// Why: a custom colourbar (#24) shows a scale that no artist uses, so a bar of a scale named
// only by colourbars is valid, and its panel always has something to draw, unlike an empty
// legend panel, so it raises no `NothingToDraw`.
#[test]
fn a_colourbar_of_a_scale_that_no_artist_names_is_valid_without_warnings() {
    let mut s = bars();
    s.fig.axes[0].artists.clear();
    s.fig.data.retain(|d| d.id != s.data);
    let report = s.fig.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![], "{report:?}");
}

// Why: a scale reference that names no node cannot be drawn, so it is reported, and the
// error points at the node that holds the reference so that the viewer can select it.
#[test]
fn a_colourbar_naming_no_node_is_unknown_color_scale_against_its_holder() {
    let s = bars();
    let missing = NodeId(4_242);

    let mut fig = s.fig.clone();
    panel_colorbar_mut(&mut fig, s.panel).scale = missing;
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::UnknownColorScale, s.panel),
        "{report:?}"
    );

    let mut fig = s.fig.clone();
    inside_colorbar_mut(&mut fig, s.axes[1]).scale = missing;
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::UnknownColorScale, s.axes[1]),
        "{report:?}"
    );
}

// Why: in one identifier space a colourbar can name a node of another kind (an axes, a data
// array, a panel), which must be reported as the wrong kind, not as an unknown scale, so the
// message tells the author what is wrong.
#[test]
fn a_colourbar_naming_a_node_of_another_kind_is_wrong_node_kind() {
    let s = bars();
    for (what, target) in [
        ("an axes", s.axes[0]),
        ("a data array", s.data),
        ("a panel", s.panel),
        ("an artist", s.image),
    ] {
        let mut fig = s.fig.clone();
        panel_colorbar_mut(&mut fig, s.panel).scale = target;
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::WrongNodeKind, s.panel),
            "{what} in a panel: {report:?}"
        );
        assert!(
            !kinds(&report.errors).contains(&IssueKind::UnknownColorScale),
            "{what} in a panel: {report:?}"
        );

        let mut fig = s.fig.clone();
        inside_colorbar_mut(&mut fig, s.axes[1]).scale = target;
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::WrongNodeKind, s.axes[1]),
            "{what} inside an axes: {report:?}"
        );
    }
}

// Why: a property editor may point a colourbar at any identifier, so an edit that names the
// wrong kind of node is refused by validation like any edit that makes the figure invalid,
// and leaves the figure unchanged.
#[test]
fn an_edit_pointing_a_colourbar_at_an_axes_is_refused() {
    let s = bars();
    for (node, at) in [
        (s.panel, "content.colorbar.scale"),
        (s.axes[1], "colorbar.scale"),
    ] {
        let mut edited = s.fig.clone();
        let result = edited.apply(&tx([set(node, at, Value::NodeId(s.axes[0]))]));
        assert!(
            matches!(&result, Err(EditError::Invalid(issues))
                if has(issues, IssueKind::WrongNodeKind, node)),
            "{at}: {result:?}"
        );
        assert_eq!(edited, s.fig, "{at}: a refused edit changes nothing");
    }
}

// Why: removing a scale that a colourbar shows would leave a bar with nothing to show, so it
// is refused as removing a scale that an artist names is; removing the colourbar's holder
// with it is accepted, and undo restores both exactly.
#[test]
fn removing_a_scale_that_a_colourbar_names_is_refused_unless_the_colourbars_go_too() {
    let mut s = bars();
    // Only the colourbars name the scale, so that the refusal is theirs and not the image's.
    s.fig.axes[0].artists.clear();
    s.fig.data.retain(|d| d.id != s.data);

    let mut edited = s.fig.clone();
    let result = edited.apply(&tx([Edit::Remove { node: s.scale }]));
    let Err(EditError::Invalid(issues)) = &result else {
        panic!("removing the scale was not refused: {result:?}");
    };
    assert!(
        has(issues, IssueKind::UnknownColorScale, s.panel),
        "{issues:?}"
    );
    assert!(
        has(issues, IssueKind::UnknownColorScale, s.axes[1]),
        "{issues:?}"
    );
    assert_eq!(edited, s.fig, "a refused transaction changes nothing");

    // The panel goes in the same transaction, and the inside colourbar is set absent.
    let both = tx([
        Edit::Remove { node: s.panel },
        set(s.axes[1], "colorbar", Value::Unset),
        Edit::Remove { node: s.scale },
    ]);
    let (removed, inverse) = applied(&s.fig, &both).unwrap();
    assert!(removed.color_scales.is_empty());
    assert!(removed.panels.is_empty());
    assert_eq!(removed.axes[1].colorbar, None);
    assert_eq!(applied(&removed, &inverse).unwrap().0, s.fig);
}

// ---------------------------------------------------------------------------------
// The length of an inside colourbar
// ---------------------------------------------------------------------------------

// Why: the length is the fraction of the edge that the bar covers, so a bar of no length,
// of negative length or longer than its edge cannot be drawn inside the plot area, and a
// length that is not a number places nothing; the full edge and a tiny fraction are both
// drawable and must be accepted.
#[test]
fn the_length_of_an_inside_colourbar_is_a_fraction_of_its_edge() {
    let s = bars();
    for length in [1.0, 0.8, 0.5, 1e-3] {
        let mut fig = s.fig.clone();
        inside_colorbar_mut(&mut fig, s.axes[1]).length = length;
        let report = fig.validate();
        assert!(
            !has(&report.errors, IssueKind::InvalidColorbarLength, s.axes[1]),
            "{length}: {report:?}"
        );
        assert_eq!(report.errors, vec![], "{length}");
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
        let mut fig = s.fig.clone();
        inside_colorbar_mut(&mut fig, s.axes[1]).length = length;
        let report = fig.validate();
        assert!(
            has(&report.errors, IssueKind::InvalidColorbarLength, s.axes[1]),
            "{length}: {report:?}"
        );
    }
}

// Why: the property editor sets the length by path, so an unusable length must be refused
// there too, leaving the bar as it was.
#[test]
fn an_edit_setting_an_unusable_length_is_refused() {
    let s = bars();
    let mut edited = s.fig.clone();
    let result = edited.apply(&tx([set(s.axes[1], "colorbar.length", Value::Double(1.5))]));
    assert!(
        matches!(&result, Err(EditError::Invalid(issues))
            if has(issues, IssueKind::InvalidColorbarLength, s.axes[1])),
        "{result:?}"
    );
    assert_eq!(edited, s.fig);
}

// ---------------------------------------------------------------------------------
// Colourbar panels in the tile layout
// ---------------------------------------------------------------------------------

// Why: a panel is placed and checked against the layout in the same way whatever it holds,
// so a colourbar panel outside the layout is an error against it, and one over an axes is a
// warning against it, exactly as for a legend panel.
#[test]
fn the_cell_checks_of_the_tile_layout_apply_to_colourbar_panels() {
    let s = bars();

    let mut fig = s.fig.clone();
    fig.panels[0].cell.col = 2;
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::CellOutOfLayout, s.panel),
        "{report:?}"
    );

    let mut fig = s.fig.clone();
    fig.panels[0].cell.col = 0;
    let report = fig.validate();
    assert_eq!(report.errors, vec![]);
    assert!(
        has(&report.warnings, IssueKind::OverlappingCells, s.panel),
        "{report:?}"
    );
}

// Why: #88's bar beside a stack of plots sits alone in a fitted column, which takes its width
// from the bar; that is valid for a colourbar panel as for a legend panel, and the rule
// against an axes alone in fitted tracks still counts the colourbar panel as the content that
// sizes the track.
#[test]
fn a_colourbar_panel_alone_in_a_fitted_column_is_valid() {
    let s = bars();
    assert_eq!(
        s.fig.layout.col_weights,
        vec![TrackSize::Weight(1.0), TrackSize::Fit]
    );
    assert_eq!(s.fig.validate().errors, vec![]);

    // A horizontal bar under a row of axes sits alone in a fitted row.
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout {
        row_weights: vec![TrackSize::Weight(1.0), TrackSize::Fit],
        ..TileLayout::new(2, 3)
    };
    let scale = b.color_scale(continuous(
        named(ColormapName::Magma),
        Limits::Manual { min: 0.0, max: 1.0 },
    ));
    for col in 0..3 {
        b.axes2d(0, col);
    }
    let panel = b.node();
    b.fig.panels.push(Panel {
        id: panel,
        cell: Cell {
            row: 1,
            col: 0,
            row_span: 1,
            col_span: 3,
        },
        align: PanelLocation::Center,
        title: None,
        title_align: TitleAlign::Left,
        content: PanelContent::Colorbar(PanelColorbar {
            scale,
            orientation: ColorbarOrientation::Horizontal,
            ..PanelColorbar::default()
        }),
    });
    let report = b.build().validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![]);
}

// Why: a legend panel and a colourbar panel are one node kind, so a session can turn one into
// the other by setting the content, and undo turns it back; the panel keeps its identifier,
// cell and alignment through the change.
#[test]
fn a_panel_s_content_is_switched_between_a_legend_and_a_colourbar_by_one_set() {
    let s = bars();
    let (legend, inverse) = applied(
        &s.fig,
        &tx([set(
            s.panel,
            "content",
            Value::PanelContent(PanelContent::Legend(PanelLegend::default())),
        )]),
    )
    .unwrap();
    assert!(matches!(legend.panels[0].content, PanelContent::Legend(_)));
    assert_eq!(legend.panels[0].id, s.panel);
    assert_eq!(legend.panels[0].cell, s.fig.panels[0].cell);
    assert_eq!(applied(&legend, &inverse).unwrap().0, s.fig);

    let spare = legend_panel(
        NodeId(5_000),
        Cell {
            row: 1,
            col: 1,
            ..Cell::default()
        },
    );
    let mut fig = s.fig.clone();
    fig.panels.push(spare);
    let (switched, _) = applied(
        &fig,
        &tx([set(
            NodeId(5_000),
            "content",
            Value::PanelContent(PanelContent::Colorbar(PanelColorbar {
                scale: s.scale,
                orientation: ColorbarOrientation::Horizontal,
                ..PanelColorbar::default()
            })),
        )]),
    )
    .unwrap();
    assert!(matches!(
        switched.panels[1].content,
        PanelContent::Colorbar(PanelColorbar {
            orientation: ColorbarOrientation::Horizontal,
            ..
        })
    ));
}

// ---------------------------------------------------------------------------------
// Properties and edits
// ---------------------------------------------------------------------------------

/// Returns the property that the registry lists at a path for a kind of node.
#[track_caller]
fn property(kind: NodeKind, at: &str) -> Property {
    properties(kind)
        .into_iter()
        .find(|p| p.path.to_string() == at)
        .unwrap_or_else(|| panic!("{kind:?} lists no {at}"))
}

// Why: the property editor edits a colourbar through the registry, so its scale reference
// and orientation must be listed below the panel's content, as a panel's legend properties
// are, and an inside colourbar's scale, side and length below the axes, with their types.
#[test]
fn the_registry_lists_the_properties_of_colourbars() {
    let scale = property(NodeKind::Panel, "content.colorbar.scale");
    assert_eq!(scale.value_type, ValueType::NodeId);
    assert!(!scale.optional, "a colourbar always names a scale");
    assert_eq!(
        property(NodeKind::Panel, "content.colorbar.orientation").value_type,
        ValueType::ColorbarOrientation
    );

    let colorbar = property(NodeKind::Axes, "colorbar");
    assert_eq!(colorbar.value_type, ValueType::InsideColorbar);
    assert!(colorbar.optional, "an axes shows no bar unless asked");
    assert_eq!(
        property(NodeKind::Axes, "colorbar.scale").value_type,
        ValueType::NodeId
    );
    assert_eq!(
        property(NodeKind::Axes, "colorbar.side").value_type,
        ValueType::InsideColorbarSide
    );
    assert_eq!(
        property(NodeKind::Axes, "colorbar.length").value_type,
        ValueType::Double
    );
}

// Why: turning a bar, moving it to another edge and shortening it are the edits a viewer
// makes; each is one set, read back by path, and undone exactly.
#[test]
fn the_orientation_side_and_length_are_set_by_path_and_undone() {
    let s = bars();
    let (edited, inverse) = applied(
        &s.fig,
        &tx([
            set(
                s.panel,
                "content.colorbar.orientation",
                Value::ColorbarOrientation(ColorbarOrientation::Horizontal),
            ),
            set(
                s.axes[1],
                "colorbar.side",
                Value::InsideColorbarSide(InsideColorbarSide::East),
            ),
            set(s.axes[1], "colorbar.length", Value::Double(0.5)),
        ]),
    )
    .unwrap();
    assert_eq!(
        edited
            .get(s.panel, &path("content.colorbar.orientation"))
            .unwrap(),
        Value::ColorbarOrientation(ColorbarOrientation::Horizontal)
    );
    assert_eq!(
        edited.get(s.axes[1], &path("colorbar")).unwrap(),
        Value::InsideColorbar(InsideColorbar {
            scale: s.scale,
            side: InsideColorbarSide::East,
            length: 0.5,
            title_placement: ColorbarTitlePlacement::Along,
        })
    );
    assert_eq!(applied(&edited, &inverse).unwrap().0, s.fig);
}

// Why: an axes gains a bar by one set of the whole optional value, as it gains a legend, and
// a field below an absent bar has no value to read or change.
#[test]
fn an_inside_colourbar_is_added_by_setting_it_and_its_fields_are_absent_until_then() {
    let s = bars();
    let bare = s.axes[0];
    assert!(matches!(
        s.fig.get(bare, &path("colorbar.length")),
        Err(EditError::AbsentValue { .. })
    ));
    assert_eq!(s.fig.get(bare, &path("colorbar")).unwrap(), Value::Unset);
    let added = InsideColorbar {
        scale: s.scale,
        side: InsideColorbarSide::West,
        length: 0.6,
        title_placement: ColorbarTitlePlacement::Above,
    };
    let (edited, inverse) = applied(
        &s.fig,
        &tx([set(bare, "colorbar", Value::InsideColorbar(added))]),
    )
    .unwrap();
    assert_eq!(edited.axes[0].colorbar, Some(added));
    assert_eq!(edited.validate().errors, vec![]);
    assert_eq!(applied(&edited, &inverse).unwrap().0, s.fig);
}

// Why: a path into a legend's properties on a colourbar panel, or into a colourbar's on a
// legend panel, is a stale edit (the content was switched since the edit was made), and must
// be refused without change rather than reported as a typo.
#[test]
fn a_path_into_the_other_kind_of_content_is_an_inactive_variant() {
    let s = bars();
    let mut fig = s.fig.clone();
    fig.panels.push(legend_panel(
        NodeId(5_000),
        Cell {
            row: 1,
            col: 1,
            ..Cell::default()
        },
    ));
    for (node, at, value) in [
        (s.panel, "content.legend.boxed", Value::Bool(true)),
        (
            NodeId(5_000),
            "content.colorbar.orientation",
            Value::ColorbarOrientation(ColorbarOrientation::Horizontal),
        ),
    ] {
        let mut edited = fig.clone();
        let result = edited.apply(&tx([set(node, at, value)]));
        assert!(
            matches!(result, Err(EditError::InactiveVariant { .. })),
            "{at}: {result:?}"
        );
        assert_eq!(edited, fig, "{at}");
    }
}

// Why: the panel's content is offered as a combo box; switching a legend panel to a
// colourbar is a choice the editor must offer, and the choice of the current kind must keep
// what the panel holds rather than reset it.
#[test]
fn the_content_of_a_panel_offers_a_legend_and_a_colourbar() {
    let offered = choices(ValueType::PanelContent, None);
    assert_eq!(offered.len(), 2, "{:?}", offered);
    assert!(
        offered
            .iter()
            .any(|c| matches!(c.value, Value::PanelContent(PanelContent::Legend(_))))
    );
    assert!(
        offered
            .iter()
            .any(|c| matches!(c.value, Value::PanelContent(PanelContent::Colorbar(_))))
    );

    let current = Value::PanelContent(PanelContent::Colorbar(PanelColorbar {
        scale: NodeId(77),
        orientation: ColorbarOrientation::Horizontal,
        length: Some(0.5),
        title_placement: ColorbarTitlePlacement::Above,
    }));
    let kept = choices(ValueType::PanelContent, Some(&current))
        .into_iter()
        .find(|c| matches!(c.value, Value::PanelContent(PanelContent::Colorbar(_))))
        .expect("a colourbar is offered");
    assert_eq!(kept.value, current, "the current colourbar is kept");
}
