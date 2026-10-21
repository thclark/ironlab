//! Figure tags: free words that describe a figure, used to filter and group
//! collections of figures. Tags were called labels before the vocabulary of titles,
//! labels, names and tags was settled: a label is text drawn beside what it identifies,
//! and a tag is never drawn. These tests cover their persistence in both encodings, the
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

/// Returns a figure carrying the given tags and otherwise a single line.
fn figure_with(tags: &[&str]) -> Figure {
    let (mut fig, _, _) = single_line_figure();
    fig.tags = tags.iter().map(|tag| (*tag).to_owned()).collect();
    fig
}

/// Tags at the edges of what a tag holds: ASCII words, words carrying whitespace and
/// punctuation, words that are not ASCII, and two words that differ only in case, in an
/// order that is neither alphabetical nor the reverse of it.
fn edge_tags() -> Vec<&'static str> {
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

/// Returns the tags of a figure (field 14 of `Figure`) in the order in which they
/// appear in the bytes.
fn raw_tags(bytes: &[u8]) -> Vec<String> {
    raw_fields(bytes, 14)
        .into_iter()
        .map(|payload| String::from_utf8(payload).expect("a tag is UTF-8"))
        .collect()
}

// ---------------------------------------------------------------------------------
// Round trips
// ---------------------------------------------------------------------------------

// Why: tags are what collections of figures are filtered and grouped by, so a saved
// `.fig` must reload every tag as exactly the word that was written, keeping its case,
// its whitespace and its non-ASCII characters, and keeping the tags in the order in
// which the figure gave them, because that order is part of the figure's value.
#[test]
fn tags_survive_a_protobuf_round_trip_in_the_order_given() {
    let fig = figure_with(&edge_tags());
    let restored = Figure::from_protobuf(&fig.to_protobuf()).unwrap();
    assert_eq!(restored, fig);
    assert_eq!(restored.tags, edge_tags());
}

// Why: JSON is the secondary format, used by other tools and hand-written files, and must
// describe the same figure as the binary one: the same tags, unaltered, in the same
// order, and saving the reloaded figure must change nothing in the text.
#[test]
fn tags_survive_a_json_round_trip_in_the_order_given() {
    let fig = figure_with(&edge_tags());
    let json = fig.to_json();
    let restored = Figure::from_json(&json).unwrap();
    assert_eq!(restored, fig);
    assert_eq!(restored.tags, edge_tags());
    assert_eq!(restored.to_json(), json, "saving again changes the text");
}

// Why: the two encodings must describe the same figure, so converting a `.fig` carrying
// tags to JSON and back must reproduce the same bytes.
#[test]
fn tags_convert_from_protobuf_to_json_and_back_to_the_same_bytes() {
    let bytes = figure_with(&edge_tags()).to_protobuf();
    let json = Figure::from_protobuf(&bytes).unwrap().to_json();
    assert_eq!(Figure::from_json(&json).unwrap().to_protobuf(), bytes);
}

// ---------------------------------------------------------------------------------
// Order and determinism
// ---------------------------------------------------------------------------------

// Why: this is where tags differ from parameters. Parameters are a map, which both
// encodings write in ascending order of name, so the order in which they were set cannot
// be seen in the file. Tags are a sequence: the order is the figure's, it carries
// meaning (the first tag is the one a browser shows first), and it must reach the file
// unchanged. A writer that sorted or deduplicated the tags on the way out would pass
// every round-trip test above, because it would sort them the same way on the way back
// in, so the bytes themselves are read here.
#[test]
fn tags_are_written_in_the_order_given_and_are_not_sorted() {
    let fig = figure_with(&edge_tags());
    let written = raw_tags(&fig.to_protobuf());
    assert_eq!(written, edge_tags());
    let mut sorted = written.clone();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    assert_ne!(
        written, sorted,
        "the fixture must not already be in sorted order, or this proves nothing"
    );
    let text: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    assert_eq!(text["tags"], json!(edge_tags()));
    assert!(
        text.get("labels").is_none(),
        "the old name is still written"
    );
}

// Why: equal figures must encode to the same bytes and the same text, so that
// version-controlled files do not churn and figures can be deduplicated by their
// encoding. Nothing but the tags themselves and their order may reach the encoding:
// building the same list by different routes, here by pushing rather than by collecting,
// must give byte-for-byte the same file.
#[test]
fn the_same_tags_in_the_same_order_always_encode_to_the_same_bytes() {
    let one = figure_with(&edge_tags());
    let mut other = figure_with(&[]);
    for tag in edge_tags() {
        other.tags.push(tag.to_owned());
    }
    assert_eq!(other.to_protobuf(), one.to_protobuf());
    assert_eq!(other.to_json(), one.to_json());
}

// Why: the counterpart of the test above. Because the order is part of the figure's
// value, two figures whose tags differ only in order are different figures and must
// not encode alike; were they to, a viewer could not put the tags back as the code
// wrote them.
#[test]
fn tags_in_a_different_order_encode_differently() {
    let forwards = figure_with(&edge_tags());
    let mut backwards = edge_tags();
    backwards.reverse();
    let backwards = figure_with(&backwards);
    assert_ne!(backwards, forwards);
    assert_ne!(backwards.to_protobuf(), forwards.to_protobuf());
    assert_ne!(backwards.to_json(), forwards.to_json());
}

// ---------------------------------------------------------------------------------
// Absence
// ---------------------------------------------------------------------------------

// Why: most figures have no tags, and a property that is always empty only adds noise
// to every file; a figure without tags must write no `tags` property, and a file
// without the property must load with no tags.
#[test]
fn a_figure_without_tags_writes_no_tags_property_and_reloads_without_them() {
    let fig = Figure::new();
    let value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    assert!(
        value.get("tags").is_none(),
        "an empty tags property was written"
    );
    let restored = Figure::from_json(&fig.to_json()).unwrap();
    assert!(restored.tags.is_empty());
}

// Why: see the previous test; in protobuf, an empty sequence must add no bytes at all, so
// that a figure without tags encodes exactly as it would if the schema had no tags
// field, apart from its schema version.
#[test]
fn a_figure_without_tags_writes_no_tag_bytes() {
    let fig = figure_with(&[]);
    assert!(raw_tags(&fig.to_protobuf()).is_empty());
    let with_one = figure_with(&["surface"]);
    assert_eq!(raw_tags(&with_one.to_protobuf()), ["surface"]);
    assert!(
        Figure::from_protobuf(&fig.to_protobuf())
            .unwrap()
            .tags
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

// Why: an empty tag describes nothing, cannot be chosen in a viewer and cannot be typed
// into a filter, so it is a mistake of the code that built the figure rather than a
// choice it could have meant. It must be reported against the figure, because tags
// belong to no other node.
#[test]
fn an_empty_tag_is_an_error_on_the_figure() {
    let fig = figure_with(&["surface", ""]);
    let report = fig.validate();
    assert_eq!(error_kinds(&report), vec![IssueKind::InvalidTag]);
    assert_eq!(report.errors[0].node, Some(fig.id));
}

// Why: a tag repeated says no more than it did the first time, and would count twice
// when a collection is grouped by tag. The message must name the tag, so that the
// user can find it among the many a figure may carry.
#[test]
fn a_repeated_tag_is_an_error_that_names_the_tag() {
    let fig = figure_with(&["surface", "k–ω SST", "surface"]);
    let report = fig.validate();
    assert_eq!(error_kinds(&report), vec![IssueKind::InvalidTag]);
    assert_eq!(report.errors[0].node, Some(fig.id));
    assert!(
        report.errors[0].message.contains("surface"),
        "the message does not name the tag: {}",
        report.errors[0].message
    );
}

// Why: tags are compared exactly, as the figure carries them, so "Surface" and
// "surface" are two tags and neither repeats the other. Folding case to decide would
// silently reject a figure whose tags distinguish, for example, a quantity from the
// name of the run that produced it.
#[test]
fn tags_differing_only_in_case_are_not_a_repeat() {
    let fig = figure_with(&["Surface", "surface", "SURFACE"]);
    assert_eq!(error_kinds(&fig.validate()), vec![]);
}

// Why: the check must not be over-eager. Every non-empty tag is valid, including one
// that is only whitespace, one carrying punctuation or newlines, and one that is not
// ASCII; rejecting any of them would stop a user describing a real figure.
#[test]
fn every_non_empty_tag_is_valid() {
    let fig = figure_with(&edge_tags());
    assert_eq!(error_kinds(&fig.validate()), vec![]);
    assert_eq!(error_kinds(&figure_with(&[" "]).validate()), vec![]);
}

// Why: a user fixing a figure should see every faulty tag at once rather than one per
// run, so validation must report them all rather than stopping at the first.
#[test]
fn every_invalid_tag_is_reported() {
    let fig = figure_with(&["", "surface", "surface", "surface"]);
    assert_eq!(
        error_kinds(&fig.validate()),
        vec![
            IssueKind::InvalidTag,
            IssueKind::InvalidTag,
            IssueKind::InvalidTag
        ]
    );
}

// ---------------------------------------------------------------------------------
// JSON Schema
// ---------------------------------------------------------------------------------

// Why: the generated JSON Schema is the only place where the two rules of a tag can be
// stated declaratively. Protocol Buffers cannot express either: a `repeated string` field
// admits an empty string and admits the same string twice. A tool that generates figures
// from the schema is therefore stopped at the schema rather than at our validator, and
// only if the schema declares that the items are at least one character long and that the
// array holds no duplicates. The property must also stay optional, because a figure
// without tags omits it.
#[test]
fn the_json_schema_declares_optional_non_empty_unique_tags_in_the_figure_module() {
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
    let tags = figure["properties"]
        .get("tags")
        .expect("tags are not declared in figure.schema.json");
    assert_eq!(tags["type"], json!("array"));
    assert_eq!(tags["items"]["type"], json!("string"));
    assert_eq!(
        tags["items"]["minLength"],
        json!(1),
        "an empty tag is not excluded by the schema"
    );
    assert_eq!(
        tags["uniqueItems"],
        json!(true),
        "a repeated tag is not excluded by the schema"
    );
    let required = figure["required"]
        .as_array()
        .expect("a figure has required properties");
    assert!(
        !required.contains(&json!("tags")),
        "tags must not be required"
    );
}

// ---------------------------------------------------------------------------------
// The edit protocol
// ---------------------------------------------------------------------------------

// Why: a tag is arbitrary Unicode text that may contain dots, and the tags are a
// list, so no single tag can be addressed by a path segment; a client changes them by
// setting the whole property, which must keep every tag exactly as given and in the
// order given.
#[test]
fn tags_are_set_whole_and_not_addressed_by_segments() {
    let fig = figure_with(&["surface"]);
    for at in ["tags.0", "tags.surface", "tags.k–ω SST"] {
        assert!(
            matches!(
                fig.get(fig.id, &path(at)),
                Err(EditError::UnknownPath { edit: None, .. })
            ),
            "get {at}"
        );
    }
    let tags: Vec<String> = edge_tags().iter().map(|l| (*l).to_owned()).collect();
    let (edited, _) = applied(
        &fig,
        &tx([set(fig.id, "tags", Value::Strings(tags.clone()))]),
    )
    .unwrap();
    assert_eq!(edited.tags, tags);
    assert_eq!(
        edited.get(fig.id, &path("tags")).unwrap(),
        Value::Strings(tags)
    );
}

// Why: a viewer and a session exchange edits as Protocol Buffers, so an edit that sets
// the tags must arrive as the list that was sent, in the same order, and must encode
// again to the same bytes; a value type that were lost or reordered on the wire would
// silently retag the figure at the other end.
#[test]
fn an_edit_setting_the_tags_round_trips_through_protobuf() {
    let tags: Vec<String> = edge_tags().iter().map(|l| (*l).to_owned()).collect();
    let original = tx([set(NodeId(1), "tags", Value::Strings(tags.clone()))]);
    let decoded = Transaction::from_protobuf(&original.to_protobuf())
        .unwrap_or_else(|e| panic!("the encoding does not decode: {e:?}"));
    assert_eq!(decoded, original);
    assert_eq!(decoded.to_protobuf(), original.to_protobuf());
    let Edit::Set { value, .. } = &decoded.edits[0] else {
        panic!("the transaction holds a set");
    };
    assert_eq!(*value, Value::Strings(tags));
}

// Why: undo is built from the inverse that an edit returns, so setting the tags must
// give back the tags that the figure carried, in the order it carried them, including
// the empty list of a figure that had none.
#[test]
fn the_inverse_of_setting_the_tags_restores_the_previous_tags() {
    for before in [vec![], vec!["surface", "3d"]] {
        let fig = figure_with(&before);
        let (edited, inverse) = applied(
            &fig,
            &tx([set(
                fig.id,
                "tags",
                Value::Strings(vec!["validated".to_owned()]),
            )]),
        )
        .unwrap();
        assert_eq!(edited.tags, ["validated"]);
        let (restored, _) = applied(&edited, &inverse).unwrap();
        assert_eq!(restored.tags, before);
    }
}

// Why: the rename of the figure's labels to tags is an intended breaking change, made
// while the schema has no users, so that `label` means only text that is drawn. The old
// path must therefore be unknown rather than kept as an alias, or two paths would set one
// property and the vocabulary would stay ambiguous.
#[test]
fn the_old_labels_path_is_unknown() {
    let fig = figure_with(&["surface"]);
    assert!(matches!(
        fig.get(fig.id, &path("labels")),
        Err(EditError::UnknownPath { edit: None, .. })
    ));
    let result = applied(
        &fig,
        &tx([set(fig.id, "labels", Value::Strings(vec!["3d".to_owned()]))]),
    );
    assert!(
        matches!(result, Err(EditError::UnknownPath { edit: Some(0), .. })),
        "{result:?}"
    );
}
