//! Figure labels: free words that describe a figure, used to filter and group
//! collections of figures. These tests cover their persistence in both encodings, the
//! order in which they are written, their JSON form, their validation, their JSON Schema
//! and the edit that sets them.
//!
//! The stability of their field number is tested in `protobuf.rs`.

mod common;

use std::collections::BTreeMap;

use common::edits::{applied, path, set, tx};
use common::{raw_fields, single_line_figure};
use ironlab_ir::*;
use serde_json::{Value as JsonValue, json};

/// Returns a figure carrying the given labels and otherwise a single line.
fn figure_with(labels: &[&str]) -> Figure {
    let (mut fig, _, _) = single_line_figure();
    fig.labels = labels.iter().map(|label| (*label).to_owned()).collect();
    fig
}

/// Labels at the edges of what a label holds: ASCII words, words carrying whitespace and
/// punctuation, words that are not ASCII, and two words that differ only in case, in an
/// order that is neither alphabetical nor the reverse of it.
fn edge_labels() -> Vec<&'static str> {
    vec![
        "validated",
        "boundary layer",
        "k–ω SST",
        "数値",
        "\u{1F30A} \"quoted\"\n",
        "Validated",
        "3d",
    ]
}

/// Returns the labels of a figure (field 14 of `Figure`) in the order in which they
/// appear in the bytes.
fn raw_labels(bytes: &[u8]) -> Vec<String> {
    raw_fields(bytes, 14)
        .into_iter()
        .map(|payload| String::from_utf8(payload).expect("a label is UTF-8"))
        .collect()
}

// ---------------------------------------------------------------------------------
// Round trips
// ---------------------------------------------------------------------------------

// Why: labels are what collections of figures are filtered and grouped by, so a saved
// `.fig` must reload every label as exactly the word that was written, keeping its case,
// its whitespace and its non-ASCII characters, and keeping the labels in the order in
// which the figure gave them, because that order is part of the figure's value.
#[test]
fn labels_survive_a_protobuf_round_trip_in_the_order_given() {
    let fig = figure_with(&edge_labels());
    let restored = Figure::from_protobuf(&fig.to_protobuf()).unwrap();
    assert_eq!(restored, fig);
    assert_eq!(restored.labels, edge_labels());
}

// Why: JSON is the secondary format, used by other tools and hand-written files, and must
// describe the same figure as the binary one: the same labels, unaltered, in the same
// order, and saving the reloaded figure must change nothing in the text.
#[test]
fn labels_survive_a_json_round_trip_in_the_order_given() {
    let fig = figure_with(&edge_labels());
    let json = fig.to_json();
    let restored = Figure::from_json(&json).unwrap();
    assert_eq!(restored, fig);
    assert_eq!(restored.labels, edge_labels());
    assert_eq!(restored.to_json(), json, "saving again changes the text");
}

// Why: the two encodings must describe the same figure, so converting a `.fig` carrying
// labels to JSON and back must reproduce the same bytes.
#[test]
fn labels_convert_from_protobuf_to_json_and_back_to_the_same_bytes() {
    let bytes = figure_with(&edge_labels()).to_protobuf();
    let json = Figure::from_protobuf(&bytes).unwrap().to_json();
    assert_eq!(Figure::from_json(&json).unwrap().to_protobuf(), bytes);
}

// ---------------------------------------------------------------------------------
// Order and determinism
// ---------------------------------------------------------------------------------

// Why: this is where labels differ from parameters. Parameters are a map, which both
// encodings write in ascending order of name, so the order in which they were set cannot
// be seen in the file. Labels are a sequence: the order is the figure's, it carries
// meaning (the first label is the one a browser shows first), and it must reach the file
// unchanged. A writer that sorted or deduplicated the labels on the way out would pass
// every round-trip test above, because it would sort them the same way on the way back
// in, so the bytes themselves are read here.
#[test]
fn labels_are_written_in_the_order_given_and_are_not_sorted() {
    let fig = figure_with(&edge_labels());
    let written = raw_labels(&fig.to_protobuf());
    assert_eq!(written, edge_labels());
    let mut sorted = written.clone();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    assert_ne!(
        written, sorted,
        "the fixture must not already be in sorted order, or this proves nothing"
    );
    let text: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    assert_eq!(text["labels"], json!(edge_labels()));
}

// Why: equal figures must encode to the same bytes and the same text, so that
// version-controlled files do not churn and figures can be deduplicated by their
// encoding. Nothing but the labels themselves and their order may reach the encoding:
// building the same list by different routes, here by pushing rather than by collecting,
// must give byte-for-byte the same file.
#[test]
fn the_same_labels_in_the_same_order_always_encode_to_the_same_bytes() {
    let one = figure_with(&edge_labels());
    let mut other = figure_with(&[]);
    for label in edge_labels() {
        other.labels.push(label.to_owned());
    }
    assert_eq!(other.to_protobuf(), one.to_protobuf());
    assert_eq!(other.to_json(), one.to_json());
}

// Why: the counterpart of the test above. Because the order is part of the figure's
// value, two figures whose labels differ only in order are different figures and must
// not encode alike; were they to, a viewer could not put the labels back as the code
// wrote them.
#[test]
fn labels_in_a_different_order_encode_differently() {
    let forwards = figure_with(&edge_labels());
    let mut backwards = edge_labels();
    backwards.reverse();
    let backwards = figure_with(&backwards);
    assert_ne!(backwards, forwards);
    assert_ne!(backwards.to_protobuf(), forwards.to_protobuf());
    assert_ne!(backwards.to_json(), forwards.to_json());
}

// ---------------------------------------------------------------------------------
// Absence
// ---------------------------------------------------------------------------------

// Why: most figures have no labels, and a property that is always empty only adds noise
// to every file; a figure without labels must write no `labels` property, and a file
// without the property must load with no labels.
#[test]
fn a_figure_without_labels_writes_no_labels_property_and_reloads_without_them() {
    let fig = Figure::new();
    let value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    assert!(
        value.get("labels").is_none(),
        "an empty labels property was written"
    );
    let restored = Figure::from_json(&fig.to_json()).unwrap();
    assert!(restored.labels.is_empty());
}

// Why: see the previous test; in protobuf, an empty sequence must add no bytes at all, so
// that a figure without labels encodes exactly as it would if the schema had no labels
// field, apart from its schema version.
#[test]
fn a_figure_without_labels_writes_no_label_bytes() {
    let fig = figure_with(&[]);
    assert!(raw_labels(&fig.to_protobuf()).is_empty());
    let with_one = figure_with(&["surface"]);
    assert_eq!(raw_labels(&with_one.to_protobuf()), ["surface"]);
    assert!(
        Figure::from_protobuf(&fig.to_protobuf())
            .unwrap()
            .labels
            .is_empty()
    );
}

// ---------------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------------

/// Returns the kinds of the errors of a report, in the order reported.
fn error_kinds(report: &ValidationReport) -> Vec<IssueKind> {
    report.errors.iter().map(|issue| issue.kind).collect()
}

// Why: an empty label describes nothing, cannot be chosen in a viewer and cannot be typed
// into a filter, so it is a mistake of the code that built the figure rather than a
// choice it could have meant. It must be reported against the figure, because labels
// belong to no other node.
#[test]
fn an_empty_label_is_an_error_on_the_figure() {
    let fig = figure_with(&["surface", ""]);
    let report = fig.validate();
    assert_eq!(error_kinds(&report), vec![IssueKind::InvalidLabel]);
    assert_eq!(report.errors[0].node, Some(fig.id));
}

// Why: a label repeated says no more than it did the first time, and would count twice
// when a collection is grouped by label. The message must name the label, so that the
// user can find it among the many a figure may carry.
#[test]
fn a_repeated_label_is_an_error_that_names_the_label() {
    let fig = figure_with(&["surface", "k–ω SST", "surface"]);
    let report = fig.validate();
    assert_eq!(error_kinds(&report), vec![IssueKind::InvalidLabel]);
    assert_eq!(report.errors[0].node, Some(fig.id));
    assert!(
        report.errors[0].message.contains("surface"),
        "the message does not name the label: {}",
        report.errors[0].message
    );
}

// Why: labels are compared exactly, as the figure carries them, so "Surface" and
// "surface" are two labels and neither repeats the other. Folding case to decide would
// silently reject a figure whose labels distinguish, for example, a quantity from the
// name of the run that produced it.
#[test]
fn labels_differing_only_in_case_are_not_a_repeat() {
    let fig = figure_with(&["Surface", "surface", "SURFACE"]);
    assert_eq!(error_kinds(&fig.validate()), vec![]);
}

// Why: the check must not be over-eager. Every non-empty label is valid, including one
// that is only whitespace, one carrying punctuation or newlines, and one that is not
// ASCII; rejecting any of them would stop a user describing a real figure.
#[test]
fn every_non_empty_label_is_valid() {
    let fig = figure_with(&edge_labels());
    assert_eq!(error_kinds(&fig.validate()), vec![]);
    assert_eq!(error_kinds(&figure_with(&[" "]).validate()), vec![]);
}

// Why: a user fixing a figure should see every faulty label at once rather than one per
// run, so validation must report them all rather than stopping at the first.
#[test]
fn every_invalid_label_is_reported() {
    let fig = figure_with(&["", "surface", "surface", "surface"]);
    assert_eq!(
        error_kinds(&fig.validate()),
        vec![
            IssueKind::InvalidLabel,
            IssueKind::InvalidLabel,
            IssueKind::InvalidLabel
        ]
    );
}

// ---------------------------------------------------------------------------------
// JSON Schema
// ---------------------------------------------------------------------------------

// Why: the generated JSON Schema is the only place where the two rules of a label can be
// stated declaratively. Protocol Buffers cannot express either: a `repeated string` field
// admits an empty string and admits the same string twice. A tool that generates figures
// from the schema is therefore stopped at the schema rather than at our validator, and
// only if the schema declares that the items are at least one character long and that the
// array holds no duplicates. The property must also stay optional, because a figure
// without labels omits it.
#[test]
fn the_json_schema_declares_optional_non_empty_unique_labels_in_the_figure_module() {
    let files: BTreeMap<String, JsonValue> = json_schema_files()
        .into_iter()
        .map(|(path, text)| {
            (
                path.display().to_string(),
                serde_json::from_str(&text).unwrap(),
            )
        })
        .collect();
    let figure = &files["figure.schema.json"];
    let labels = figure["properties"]
        .get("labels")
        .expect("labels are not declared in figure.schema.json");
    assert_eq!(labels["type"], json!("array"));
    assert_eq!(labels["items"]["type"], json!("string"));
    assert_eq!(
        labels["items"]["minLength"],
        json!(1),
        "an empty label is not excluded by the schema"
    );
    assert_eq!(
        labels["uniqueItems"],
        json!(true),
        "a repeated label is not excluded by the schema"
    );
    let required = figure["required"]
        .as_array()
        .expect("a figure has required properties");
    assert!(
        !required.contains(&json!("labels")),
        "labels must not be required"
    );
}

// ---------------------------------------------------------------------------------
// The edit protocol
// ---------------------------------------------------------------------------------

// Why: a label is arbitrary Unicode text that may contain dots, and the labels are a
// list, so no single label can be addressed by a path segment; a client changes them by
// setting the whole property, which must keep every label exactly as given and in the
// order given.
#[test]
fn labels_are_set_whole_and_not_addressed_by_segments() {
    let fig = figure_with(&["surface"]);
    for at in ["labels.0", "labels.surface", "labels.k–ω SST"] {
        assert!(
            matches!(
                fig.get(fig.id, &path(at)),
                Err(EditError::UnknownPath { edit: None, .. })
            ),
            "get {at}"
        );
    }
    let labels: Vec<String> = edge_labels().iter().map(|l| (*l).to_owned()).collect();
    let (edited, _) = applied(
        &fig,
        &tx([set(fig.id, "labels", Value::Strings(labels.clone()))]),
    )
    .unwrap();
    assert_eq!(edited.labels, labels);
    assert_eq!(
        edited.get(fig.id, &path("labels")).unwrap(),
        Value::Strings(labels)
    );
}

// Why: a viewer and a session exchange edits as Protocol Buffers, so an edit that sets
// the labels must arrive as the list that was sent, in the same order, and must encode
// again to the same bytes; a value type that were lost or reordered on the wire would
// silently relabel the figure at the other end.
#[test]
fn an_edit_setting_the_labels_round_trips_through_protobuf() {
    let labels: Vec<String> = edge_labels().iter().map(|l| (*l).to_owned()).collect();
    let original = tx([set(NodeId(1), "labels", Value::Strings(labels.clone()))]);
    let decoded = Transaction::from_protobuf(&original.to_protobuf())
        .unwrap_or_else(|e| panic!("the encoding does not decode: {e:?}"));
    assert_eq!(decoded, original);
    assert_eq!(decoded.to_protobuf(), original.to_protobuf());
    let Edit::Set { value, .. } = &decoded.edits[0] else {
        panic!("the transaction holds a set");
    };
    assert_eq!(*value, Value::Strings(labels));
}

// Why: undo is built from the inverse that an edit returns, so setting the labels must
// give back the labels that the figure carried, in the order it carried them, including
// the empty list of a figure that had none.
#[test]
fn the_inverse_of_setting_the_labels_restores_the_previous_labels() {
    for before in [vec![], vec!["surface", "3d"]] {
        let fig = figure_with(&before);
        let (edited, inverse) = applied(
            &fig,
            &tx([set(
                fig.id,
                "labels",
                Value::Strings(vec!["validated".to_owned()]),
            )]),
        )
        .unwrap();
        assert_eq!(edited.labels, ["validated"]);
        let (restored, _) = applied(&edited, &inverse).unwrap();
        assert_eq!(restored.labels, before);
    }
}
