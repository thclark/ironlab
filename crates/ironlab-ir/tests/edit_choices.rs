//! The choices of a value type: the variants of a tagged value and the values of a plain
//! enumeration, as the property editor offers them in a combo box.

mod common;

use std::collections::BTreeSet;

use common::edits::{nodes, representative_figures, sample_values};
use ironlab_ir::*;

/// The value types whose values the property editor offers as a list: the tagged values,
/// whose variants a combo box selects between, and the plain enumerations.
///
/// The list is written by hand so that the tests check [`choices`] against the IR rather
/// than against itself.
const WITH_CHOICES: [ValueType; 16] = [
    ValueType::Projection,
    ValueType::Scale,
    ValueType::Limits,
    ValueType::ColormapName,
    ValueType::LegendLocation,
    ValueType::ColorSpec,
    ValueType::DashStyle,
    ValueType::MarkerShape,
    ValueType::ScatterSize,
    ValueType::ScatterColor,
    ValueType::Grid,
    ValueType::Levels,
    ValueType::ContourPlacement,
    ValueType::QuiverScale,
    ValueType::Interpreter,
    ValueType::FontSetId,
];

/// Every value type, taken from the sample values rather than from the API under test.
fn every_value_type() -> Vec<ValueType> {
    sample_values()
        .iter()
        .filter_map(Value::value_type)
        .collect()
}

fn labels(choices: &[Choice]) -> Vec<&'static str> {
    choices.iter().map(|choice| choice.label).collect()
}

// Why: the property editor writes a chosen value straight into an `Edit::Set` of the
// property it came from, so a choice whose value has another type would be refused as a
// type mismatch at every use.
#[test]
fn every_choice_has_the_type_whose_choices_it_is() {
    for value_type in every_value_type() {
        for choice in choices(value_type, None) {
            assert_eq!(
                choice.value.value_type(),
                Some(value_type),
                "the choice {:?} of {value_type:?} has the wrong type",
                choice.label
            );
        }
    }
}

// Why: a combo box with nothing in it cannot be used, so every type the editor shows as a
// list must offer its variants; and a type that has no variants must offer none, because
// an editor that showed a combo box for a number or a colour would be showing a lie.
#[test]
fn exactly_the_tagged_and_enumerated_types_offer_choices() {
    let expected: BTreeSet<ValueType> = WITH_CHOICES.into_iter().collect();
    for value_type in every_value_type() {
        assert_eq!(
            !choices(value_type, None).is_empty(),
            expected.contains(&value_type),
            "{value_type:?} offers the wrong number of choices"
        );
    }
}

// Why: the editor identifies a choice by its label, in the combo box and in the tests, so
// two choices of one type that share a label would be indistinguishable; and the order
// must not depend on the value being replaced, or the entries of a combo box would jump
// about as the user changes them.
#[test]
fn the_choices_of_a_type_are_distinctly_labelled_and_ordered_the_same_whatever_they_replace() {
    for value_type in WITH_CHOICES {
        let offered = choices(value_type, None);
        let distinct: BTreeSet<&str> = labels(&offered).into_iter().collect();
        assert_eq!(
            distinct.len(),
            offered.len(),
            "the labels of {value_type:?} are not distinct: {:?}",
            labels(&offered)
        );
        assert!(
            distinct.iter().all(|label| !label.is_empty()),
            "{value_type:?} has an empty label"
        );
        for choice in &offered {
            assert_eq!(
                labels(&choices(value_type, Some(&choice.value))),
                labels(&offered),
                "the choices of {value_type:?} are ordered differently when replacing {:?}",
                choice.label
            );
        }
    }
}

// Why: the combo box shows the choice the property currently holds, which it finds by
// asking each choice whether it is the one; a choice that matched another choice's value,
// or failed to match a value of its own variant that carries different contents, would
// show the wrong selection.
#[test]
fn a_choice_matches_every_value_of_its_own_variant_and_no_other() {
    for value_type in WITH_CHOICES {
        let offered = choices(value_type, None);
        for choice in &offered {
            assert!(
                choice.matches(&choice.value),
                "the choice {:?} of {value_type:?} does not match its own value",
                choice.label
            );
            for other in &offered {
                if other.label != choice.label {
                    assert!(
                        !choice.matches(&other.value),
                        "the choice {:?} of {value_type:?} also matches {:?}",
                        choice.label,
                        other.label
                    );
                }
            }
        }
    }
    // A variant whose contents differ from the default of its choice is still that
    // choice: selecting "Manual" while manual limits are shown must not look like a
    // change of variant.
    let manual = choices(ValueType::Limits, None)
        .into_iter()
        .find(|choice| choice.matches(&Value::Limits(Limits::Manual { min: 0.0, max: 1.0 })))
        .expect("manual limits are a choice");
    assert!(manual.matches(&Value::Limits(Limits::Manual {
        min: -50.0,
        max: 50.0
    })));
    assert!(!manual.matches(&Value::Limits(Limits::Auto)));
}

// Why: switching an axis from automatic to manual limits must start from the range the
// user is looking at rather than from an arbitrary one, or every such switch would throw
// the view away; and a grid must keep its coordinate arrays, because they are the only
// arrays that could possibly suit it.
#[test]
fn a_choice_takes_what_it_can_from_the_value_it_replaces() {
    let replacing = |value_type, replaced: Value, label: &str| {
        choices(value_type, Some(&replaced))
            .into_iter()
            .find(|choice| choice.label == label)
            .unwrap_or_else(|| panic!("{value_type:?} offers {label:?}"))
            .value
    };

    let from_manual = replacing(
        ValueType::Limits,
        Value::Limits(Limits::Manual {
            min: -3.0,
            max: 7.5,
        }),
        "Manual",
    );
    assert_eq!(
        from_manual,
        Value::Limits(Limits::Manual {
            min: -3.0,
            max: 7.5
        }),
        "manual limits keep the bounds they replace"
    );

    // Automatic limits hold no bounds, so the documented fallback of 0 to 1 is used.
    for replaced in [None, Some(Value::Limits(Limits::Auto))] {
        let fallback = choices(ValueType::Limits, replaced.as_ref())
            .into_iter()
            .find(|choice| choice.label == "Manual")
            .expect("limits offer manual")
            .value;
        assert_eq!(
            fallback,
            Value::Limits(Limits::Manual { min: 0.0, max: 1.0 })
        );
    }

    // A choice made afresh, with nothing to take from, gives the value that the Rust API
    // would build, so a variant chosen in the viewer and one written in a program start
    // the same.
    let fresh = |value_type, label: &str| {
        choices(value_type, None)
            .into_iter()
            .find(|choice| choice.label == label)
            .unwrap_or_else(|| panic!("{value_type:?} offers {label:?}"))
            .value
    };
    assert_eq!(
        fresh(ValueType::ScatterSize, "Single size"),
        Value::ScatterSize(ScatterSize::default())
    );
    assert_eq!(
        fresh(ValueType::ScatterColor, "Single colour"),
        Value::ScatterColor(ScatterColor::default())
    );
    assert_eq!(
        fresh(ValueType::Levels, "Automatic"),
        Value::Levels(Levels::default())
    );
    assert_eq!(
        fresh(ValueType::ContourPlacement, "In one plane"),
        Value::ContourPlacement(ContourPlacement::default())
    );
    assert_eq!(
        fresh(ValueType::Projection, "Three-dimensional"),
        Value::Projection(Projection::ThreeD {
            view3d: View3d::default()
        })
    );

    let curvilinear = replacing(
        ValueType::Grid,
        Value::Grid(Grid::Rectilinear {
            x: DataId(11),
            y: DataId(12),
        }),
        "Curvilinear",
    );
    assert_eq!(
        curvilinear,
        Value::Grid(Grid::Curvilinear {
            x: DataId(11),
            y: DataId(12)
        }),
        "a grid keeps the coordinate arrays it replaces"
    );
}

// Why: every choice is offered to the user as something to click, so each one must either
// be applicable or be refused for a reason the user can act on. The only refusal allowed
// is validation: a choice can be one the figure does not suit (contours at their levels
// need three-dimensional axes, a curvilinear grid needs coordinates of the field's shape,
// and a choice that needs a data array the replaced value does not name refers to data 0,
// which the figure need not have). A choice must never be refused as an unknown path, an
// inactive variant or a type mismatch, because those are faults in the choice itself.
#[test]
fn applying_every_choice_to_a_representative_figure_is_accepted_or_refused_only_by_validation() {
    let mut applied = 0usize;
    for figure in representative_figures() {
        for (node, kind) in nodes(&figure) {
            for property in properties(kind) {
                let Ok(current) = figure.get(node, &property.path) else {
                    continue;
                };
                for choice in choices(property.value_type, Some(&current)) {
                    let transaction = Transaction {
                        edits: vec![Edit::Set {
                            node,
                            path: property.path.clone(),
                            value: choice.value.clone(),
                        }],
                    };
                    applied += 1;
                    let mut copy = figure.clone();
                    match copy.apply(&transaction) {
                        Ok(_) | Err(EditError::Invalid(_)) => {}
                        Err(other) => panic!(
                            "the choice {:?} of {} on {node} was refused by {other:?}",
                            choice.label, property.path
                        ),
                    }
                }
            }
        }
    }
    assert!(
        applied > 100,
        "the representative figures exercised only {applied} choices"
    );
}

// Why: the choices of a tagged value are the only way the editor can switch variant, so
// every variant of every tagged value must be offered; a missing one would make a variant
// unreachable from the viewer.
#[test]
fn the_choices_of_a_tagged_value_cover_every_variant_it_has() {
    let covered = |value_type, values: Vec<Value>| {
        let offered = choices(value_type, None);
        for value in values {
            assert!(
                offered.iter().any(|choice| choice.matches(&value)),
                "no choice of {value_type:?} matches {value:?}"
            );
        }
    };
    covered(
        ValueType::Projection,
        vec![
            Value::Projection(Projection::TwoD),
            Value::Projection(Projection::ThreeD {
                view3d: View3d::default(),
            }),
        ],
    );
    covered(
        ValueType::Limits,
        vec![
            Value::Limits(Limits::Auto),
            Value::Limits(Limits::Manual { min: 0.0, max: 1.0 }),
        ],
    );
    covered(
        ValueType::ColorSpec,
        vec![
            Value::ColorSpec(ColorSpec::Auto),
            Value::ColorSpec(ColorSpec::Rgba {
                color: Color::BLACK,
            }),
            Value::ColorSpec(ColorSpec::None),
            Value::ColorSpec(ColorSpec::Colormapped),
        ],
    );
    covered(
        ValueType::ScatterSize,
        vec![
            Value::ScatterSize(ScatterSize::Scalar { value: 4.0 }),
            Value::ScatterSize(ScatterSize::Data { data: DataId(0) }),
        ],
    );
    covered(
        ValueType::ScatterColor,
        vec![
            Value::ScatterColor(ScatterColor::Spec {
                spec: ColorSpec::Auto,
            }),
            Value::ScatterColor(ScatterColor::Data { data: DataId(0) }),
        ],
    );
    covered(
        ValueType::Grid,
        vec![
            Value::Grid(Grid::Rectilinear {
                x: DataId(0),
                y: DataId(1),
            }),
            Value::Grid(Grid::Curvilinear {
                x: DataId(0),
                y: DataId(1),
            }),
        ],
    );
    covered(
        ValueType::Levels,
        vec![
            Value::Levels(Levels::Auto { count: 10 }),
            Value::Levels(Levels::Explicit {
                values: vec![1.0, 2.0],
            }),
        ],
    );
    covered(
        ValueType::ContourPlacement,
        vec![
            Value::ContourPlacement(ContourPlacement::Plane { z: None }),
            Value::ContourPlacement(ContourPlacement::AtLevel),
        ],
    );
    covered(
        ValueType::QuiverScale,
        vec![
            Value::QuiverScale(QuiverScale::Auto),
            Value::QuiverScale(QuiverScale::Factor { value: 1.0 }),
            Value::QuiverScale(QuiverScale::Off),
        ],
    );
    covered(
        ValueType::Scale,
        vec![Value::Scale(Scale::Linear), Value::Scale(Scale::Log)],
    );
    covered(
        ValueType::Interpreter,
        vec![
            Value::Interpreter(Interpreter::Latex),
            Value::Interpreter(Interpreter::None),
        ],
    );
    covered(
        ValueType::ColormapName,
        vec![
            Value::ColormapName(ColormapName::Viridis),
            Value::ColormapName(ColormapName::Cividis),
            Value::ColormapName(ColormapName::Magma),
            Value::ColormapName(ColormapName::Inferno),
            Value::ColormapName(ColormapName::Plasma),
            Value::ColormapName(ColormapName::Coolwarm),
            Value::ColormapName(ColormapName::Gray),
        ],
    );
    covered(
        ValueType::LegendLocation,
        vec![
            Value::LegendLocation(LegendLocation::NorthEast),
            Value::LegendLocation(LegendLocation::NorthWest),
            Value::LegendLocation(LegendLocation::SouthEast),
            Value::LegendLocation(LegendLocation::SouthWest),
            Value::LegendLocation(LegendLocation::North),
            Value::LegendLocation(LegendLocation::South),
            Value::LegendLocation(LegendLocation::East),
            Value::LegendLocation(LegendLocation::West),
            Value::LegendLocation(LegendLocation::Best),
        ],
    );
    covered(
        ValueType::MarkerShape,
        vec![
            Value::MarkerShape(MarkerShape::None),
            Value::MarkerShape(MarkerShape::Circle),
            Value::MarkerShape(MarkerShape::Square),
            Value::MarkerShape(MarkerShape::Diamond),
            Value::MarkerShape(MarkerShape::TriangleUp),
            Value::MarkerShape(MarkerShape::TriangleDown),
            Value::MarkerShape(MarkerShape::Plus),
            Value::MarkerShape(MarkerShape::Cross),
            Value::MarkerShape(MarkerShape::Point),
        ],
    );
    covered(
        ValueType::DashStyle,
        vec![
            Value::DashStyle(DashStyle::Solid),
            Value::DashStyle(DashStyle::Dashed),
            Value::DashStyle(DashStyle::Dotted),
            Value::DashStyle(DashStyle::DashDot),
            Value::DashStyle(DashStyle::None),
        ],
    );
    covered(
        ValueType::FontSetId,
        vec![Value::FontSetId(FontSetId::StixTwo)],
    );
}

// ---------------------------------------------------------------------------------
// The choices of one property of one node, and whether each is available there
// ---------------------------------------------------------------------------------

/// The paths at which the scene compiler looks a colour up in the colormap, written by
/// hand per kind of node so that the test checks [`property_choices`] against the
/// behaviour of the compiler rather than against itself.
///
/// Each isoline of a contour is coloured by its own level, and each face of a surface by
/// its colour data or by its height. Nowhere else does the IR hold a value to look a
/// colour up by: the compiler paints a colormapped line, quiver or single scatter colour
/// in the middle colour of the colormap, and draws a colormapped marker in the colour of
/// the plot it belongs to, exactly as an automatic one.
fn colormap_indexed(kind: NodeKind) -> Vec<&'static str> {
    match kind {
        NodeKind::Contour => vec!["line.color"],
        NodeKind::Surface => vec!["face", "edge"],
        _ => vec![],
    }
}

/// Every property of a kind of node whose value is a colour specification.
fn color_spec_paths(kind: NodeKind) -> Vec<String> {
    properties(kind)
        .into_iter()
        .filter(|property| property.value_type == ValueType::ColorSpec)
        .map(|property| property.path.to_string())
        .collect()
}

// Why: a choice that is simply removed from a combo box tells the user nothing, and a
// value the IR has is then invisible from the viewer. Every choice of the type must be
// listed at every property, in the order the type offers them, so that what is offered
// differs from what can be taken only by the mark each choice carries.
#[test]
fn every_choice_of_a_property_is_listed_whether_or_not_it_is_available() {
    let mut checked = 0usize;
    for figure in representative_figures() {
        for (node, kind) in nodes(&figure) {
            for property in properties(kind) {
                let Ok(value) = figure.get(node, &property.path) else {
                    continue;
                };
                let all = choices(property.value_type, Some(&value));
                let offered = property_choices(&figure, node, &property.path);
                assert_eq!(
                    labels(&offered),
                    labels(&all),
                    "{kind:?} lists the wrong choices at {}",
                    property.path
                );
                assert_eq!(
                    offered
                        .iter()
                        .map(|choice| choice.value.clone())
                        .collect::<Vec<Value>>(),
                    all.iter()
                        .map(|choice| choice.value.clone())
                        .collect::<Vec<Value>>(),
                    "{kind:?} lists the wrong values at {}",
                    property.path
                );
                checked += offered.len();
            }
        }
    }
    assert!(
        checked > 100,
        "the representative figures listed only {checked} choices"
    );
}

// Why: a colormapped colour promises that the plot is coloured by its data, and the scene
// compiler can keep that promise only where the IR gives it a value to look the colour up
// by. Marking it available where it is not would deliver a flat colour with no
// explanation; marking it unavailable where it works would make a surface uncolourable.
#[test]
fn a_colormapped_colour_is_available_exactly_where_the_ir_indexes_the_colormap() {
    for figure in representative_figures() {
        for (node, kind) in nodes(&figure) {
            let indexed = colormap_indexed(kind);
            let paths = color_spec_paths(kind);
            assert!(
                indexed
                    .iter()
                    .all(|path| paths.contains(&(*path).to_owned())),
                "{kind:?} has no colour specification at {indexed:?}, only {paths:?}"
            );
            for path in paths {
                let at = path.parse().expect("a registry path is a property path");
                let offered = property_choices(&figure, node, &at);
                if offered.is_empty() {
                    continue; // The property is not reachable in this figure.
                }
                let colormapped = offered
                    .iter()
                    .find(|choice| choice.label == "Colormapped")
                    .unwrap_or_else(|| panic!("{kind:?} lists no colormapped colour at {path}"));
                assert_eq!(
                    colormapped.available(),
                    indexed.contains(&path.as_str()),
                    "{kind:?} marks the colormapped colour at {path} wrongly: {:?}",
                    colormapped.unavailable
                );
            }
        }
    }
}

// Why: a disabled entry the user cannot act on is worse than no entry at all unless it
// says what would make it available, and a reason on a choice that can be taken would
// warn about nothing. Every unavailable choice must therefore carry a sentence, and every
// available one must carry none.
#[test]
fn an_unavailable_choice_carries_a_reason_and_an_available_one_carries_none() {
    let mut reasons = 0usize;
    for figure in representative_figures() {
        for (node, kind) in nodes(&figure) {
            for property in properties(kind) {
                for choice in property_choices(&figure, node, &property.path) {
                    match choice.unavailable {
                        None => assert!(
                            choice.available(),
                            "{kind:?} calls {:?} at {} unavailable with no reason",
                            choice.label,
                            property.path
                        ),
                        Some(reason) => {
                            reasons += 1;
                            assert!(
                                !choice.available(),
                                "{kind:?} gives {:?} at {} a reason it does not need",
                                choice.label,
                                property.path
                            );
                            assert!(
                                reason.len() > 40
                                    && reason.ends_with('.')
                                    && reason.starts_with(|first: char| first.is_uppercase()),
                                "the reason for {:?} at {} is not a sentence: {reason:?}",
                                choice.label,
                                property.path
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(
        reasons > 0,
        "the representative figures have unavailable choices to check"
    );
}

// Why: the reason is the only thing a user has to go on when a choice is shown disabled,
// so it must name what the property lacks and where the same choice does work. A reason
// that named neither would leave the user stuck in front of a value they can see but
// cannot pick.
#[test]
fn the_reason_names_what_the_property_lacks_and_where_the_colormap_can_be_used() {
    let figure = representative_figures().remove(0);
    let (line, kind) = nodes(&figure)
        .into_iter()
        .find(|(_, kind)| *kind == NodeKind::Line)
        .expect("a representative figure has a line");
    assert_eq!(kind, NodeKind::Line);

    let reason = property_choices(&figure, line, &"line.color".parse().unwrap())
        .into_iter()
        .find(|choice| choice.label == "Colormapped")
        .expect("a line lists the colormapped colour")
        .unavailable
        .expect("a line cannot be colormapped");
    assert!(
        reason.starts_with("A line is drawn in one colour and provides no value to look"),
        "the reason must say what a line lacks: {reason}"
    );
    for available in ["surface", "isolines of a contour", "colour comes from data"] {
        assert!(
            reason.contains(available),
            "the reason must send the user to {available:?}: {reason}"
        );
    }

    let marker = property_choices(&figure, line, &"marker.face".parse().unwrap())
        .into_iter()
        .find(|choice| choice.label == "Colormapped")
        .expect("a marker lists the colormapped colour")
        .unavailable
        .expect("a marker cannot be colormapped");
    assert!(
        marker.contains("takes the colour of the plot it belongs to"),
        "a marker's reason must say where its colour comes from: {marker}"
    );
}

// Why: a scatter coloured from data is the supported way to colour markers by value, and
// it names the array to look the colour up by; marking the colormapped colour
// specification unavailable must not touch it, or the scatter would appear to have lost
// data colouring altogether.
#[test]
fn a_scatter_still_offers_its_colour_and_size_from_data() {
    for figure in representative_figures() {
        for (node, kind) in nodes(&figure) {
            if kind != NodeKind::Scatter {
                continue;
            }
            for path in ["color", "size"] {
                let offered = property_choices(&figure, node, &path.parse().unwrap());
                assert_eq!(
                    labels(&offered),
                    match path {
                        "color" => ["Single colour", "From data"],
                        _ => ["Single size", "From data"],
                    }
                );
                assert!(
                    offered.iter().all(Choice::available),
                    "a scatter's {path} lost a choice it can take: {offered:?}"
                );
            }
        }
    }
}

// Why: the property editor asks for the choices of whatever it is showing, which a
// reconciliation or a stale frame can leave pointing at a node that has gone or a
// property that is no longer reachable; answering with a list would offer a change that
// cannot be made, so the answer must be no choices at all.
#[test]
fn a_node_or_a_path_that_the_figure_does_not_have_offers_no_choices() {
    let figure = representative_figures().remove(0);
    let axes = figure
        .axes
        .first()
        .expect("a representative figure has axes");
    let limits: PropertyPath = "x.limits".parse().unwrap();
    assert!(
        !property_choices(&figure, axes.id, &limits).is_empty(),
        "precondition: limits offer choices"
    );
    assert!(property_choices(&figure, NodeId(u64::MAX), &limits).is_empty());
    assert!(
        property_choices(&figure, axes.id, &"line.color".parse().unwrap()).is_empty(),
        "an axes has no line colour"
    );
    assert!(
        property_choices(&figure, figure.id, &"font_size_pt".parse().unwrap()).is_empty(),
        "a number is typed, not chosen"
    );
}

// Why: the choices of a type are asked for by callers with no figure (the control that
// gives an absent value one, the interpreter beside a text field), which have no property
// to judge availability at; a reason there would be a claim about a property that was
// never named.
#[test]
fn the_choices_of_a_type_alone_are_all_available() {
    for value_type in WITH_CHOICES {
        assert!(
            choices(value_type, None).iter().all(Choice::available),
            "{value_type:?} marks a choice unavailable with no property to judge it at"
        );
    }
}
