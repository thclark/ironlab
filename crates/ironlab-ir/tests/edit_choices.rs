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
