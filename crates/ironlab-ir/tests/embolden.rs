//! The figure's embolden step: its default, its persistence in both formats, and its
//! editing through the property editor.
//!
//! WHY: one saved figure must serve both paper and projection, so the step it is drawn at
//! is part of the figure. It must survive saving and loading in either format, must not
//! clutter the JSON of the many figures that never change it, and must be editable in the
//! viewer like any other property of the figure.

mod common;

use common::edits::{path, set, tx};
use common::single_line_figure;
use ironlab_ir::*;
use serde_json::Value as Json;

const STEPS: [Embolden; 3] = [Embolden::Regular, Embolden::Medium, Embolden::Semibold];

/// A small valid figure drawn at `step`.
fn figure_at(step: Embolden) -> Figure {
    let (mut fig, _, _) = single_line_figure();
    fig.embolden = step;
    fig
}

// Why: a figure built without asking for emboldening is meant for print, so it must be
// drawn as designed; and because nearly every figure keeps the default, writing it would
// add a line to every saved `.fig.json` and break the byte stability of files written by
// earlier builds for no information.
#[test]
fn a_figure_is_regular_by_default_and_the_default_is_omitted_from_json() {
    assert_eq!(Figure::default().embolden, Embolden::Regular);
    assert_eq!(Embolden::default(), Embolden::Regular);

    let json: Json = serde_json::from_str(&figure_at(Embolden::Regular).to_json()).unwrap();
    assert!(
        json.get("embolden").is_none(),
        "a regular figure writes no embolden field: {json}"
    );
}

// Why: hand-written files and web clients produce the documented JSON form rather than
// serde's output, so each step must be written under its documented name, and a document
// that omits the field, as every document of an earlier build does, must load as regular.
#[test]
fn each_step_round_trips_through_json_under_its_documented_name() {
    for (step, name) in [
        (Embolden::Medium, "medium"),
        (Embolden::Semibold, "semibold"),
    ] {
        let original = figure_at(step);
        let text = original.to_json();
        let json: Json = serde_json::from_str(&text).unwrap();
        assert_eq!(json["embolden"], name, "{step:?}");
        let restored = Figure::from_json(&text).expect("own output parses");
        assert_eq!(restored, original, "{step:?}");
    }

    let mut json: Json = serde_json::from_str(&figure_at(Embolden::Semibold).to_json()).unwrap();
    json.as_object_mut().unwrap().remove("embolden");
    let restored = Figure::from_json(&json.to_string()).unwrap();
    assert_eq!(restored.embolden, Embolden::Regular);
}

// Why: a step this build does not know (a misspelling, or "bold", which is not a step)
// must be refused rather than silently drawn as regular, or the author would export a
// slide figure that is as thin as the print figure without being told.
#[test]
fn an_unknown_step_in_json_is_refused() {
    for unknown in ["bold", "Semibold", "heavy", ""] {
        let mut json: Json =
            serde_json::from_str(&figure_at(Embolden::Semibold).to_json()).unwrap();
        json["embolden"] = Json::String(unknown.to_owned());
        let result = Figure::from_json(&json.to_string());
        assert!(
            matches!(result, Err(IrError::Json(_))),
            "{unknown:?} must be refused: {result:?}"
        );
    }
}

// Why: `.fig` is the default format, so a figure saved for slides must reload for slides;
// without the field on the wire it would reload as regular.
#[test]
fn each_step_round_trips_through_protobuf() {
    for step in STEPS {
        let original = figure_at(step);
        let restored = Figure::from_protobuf(&original.to_protobuf()).expect("own output decodes");
        assert_eq!(restored.embolden, step);
        assert_eq!(restored, original, "{step:?}");
    }
}

// Why: the viewer edits the step with a combo box built from the property's choices, so the
// figure must list the property, offer exactly the three steps, in order from lightest to
// heaviest, and mark none of them unavailable.
#[test]
fn the_editor_offers_the_three_steps_for_the_figure() {
    let fig = figure_at(Embolden::Regular);
    let listed = properties_of(&fig, fig.id);
    let property = listed
        .iter()
        .find(|property| property.path == path("embolden"))
        .expect("the figure lists its embolden property");
    assert!(!property.optional, "a figure always has a step");
    assert!(!property.docs.is_empty(), "the property is documented");

    let offered = property_choices(&fig, fig.id, &path("embolden"));
    assert_eq!(offered.len(), 3, "{offered:?}");
    assert!(offered.iter().all(Choice::available), "{offered:?}");
    let labels: Vec<&str> = offered.iter().map(|choice| choice.label).collect();
    let distinct: std::collections::BTreeSet<&str> = labels.iter().copied().collect();
    assert_eq!(distinct.len(), 3, "{labels:?}");

    let current = fig.get(fig.id, &path("embolden")).unwrap();
    let selected: Vec<&str> = offered
        .iter()
        .filter(|choice| choice.matches(&current))
        .map(|choice| choice.label)
        .collect();
    assert_eq!(
        selected,
        vec![labels[0]],
        "the combo box shows the first step"
    );
}

// Why: choosing a step in the combo box must change the figure to that step, in the order
// offered, and the inverse the edit returns must restore the step, so that the viewer can
// undo it.
#[test]
fn choosing_a_step_sets_it_and_undo_restores_the_previous_one() {
    let original = figure_at(Embolden::Regular);
    let offered = property_choices(&original, original.id, &path("embolden"));
    assert_eq!(offered.len(), 3);
    for (choice, expected) in offered.iter().zip(STEPS) {
        let mut fig = original.clone();
        let inverse = fig
            .apply(&tx([set(fig.id, "embolden", choice.value.clone())]))
            .unwrap_or_else(|e| panic!("{:?} applies: {e:?}", choice.label));
        assert_eq!(fig.embolden, expected, "{:?}", choice.label);
        assert_eq!(
            fig.get(fig.id, &path("embolden")).unwrap(),
            choice.value,
            "{:?}",
            choice.label
        );
        fig.apply(&inverse).unwrap();
        assert_eq!(fig, original, "undoing {:?}", choice.label);
    }
}

// Why: the web viewer and the Python bindings send edits as Protocol Buffers or JSON, so
// an edit that sets the step must survive both encodings of a transaction.
#[test]
fn an_edit_of_the_step_round_trips_through_both_transaction_encodings() {
    let fig = figure_at(Embolden::Regular);
    let semibold = property_choices(&fig, fig.id, &path("embolden"))
        .pop()
        .expect("three choices")
        .value;
    let transaction = tx([set(fig.id, "embolden", semibold)]);
    assert_eq!(
        Transaction::from_protobuf(&transaction.to_protobuf()).unwrap(),
        transaction
    );
    assert_eq!(
        Transaction::from_json(&transaction.to_json()).unwrap(),
        transaction
    );
}
