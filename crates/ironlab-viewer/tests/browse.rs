//! Choosing one figure from a collection of them: the facets a collection offers, the query
//! language, the refinements and the order and grouping of the result.
//!
//! The module holds no egui, so every test here drives it as a function of a collection and a
//! state. The tests are written from what the module's documentation promises rather than from
//! what its code does, so that they are an independent statement of the intended behaviour.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use ironlab_ir::{Artist, Axes, DataId, Figure, Line, NdArray, NodeId, Parameter, TileLayout};
use ironlab_viewer::browse::{
    Browse, Comparison, Constraint, Facet, FacetKey, FacetKind, FacetValue, FigureCard, Filter,
    NO_LABELS, NOT_SET, Query, Results, Sort, SortKey, Term, browsable_labels, describe_facets,
    has_nothing_to_browse_by, parameter_names,
};

// ---------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------

/// A card built directly, because the browser reads a figure once and then works only from the
/// card; the tests of everything but the deriving therefore state their cards explicitly.
fn card(title: &str, labels: &[&str], parameters: &[(&str, Parameter)]) -> FigureCard {
    FigureCard {
        title: title.to_owned(),
        labels: labels.iter().map(|label| (*label).to_owned()).collect(),
        parameters: parameters
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    }
}

/// A text parameter.
fn string(value: &str) -> Parameter {
    Parameter::String(value.to_owned())
}

/// A text facet value, which is also how a label is held.
fn text(value: &str) -> FacetValue {
    FacetValue::Text(value.to_owned())
}

/// Four figures of one campaign: two from a solver and two from a tunnel, one of which carries
/// neither an angle nor a label.
///
/// It is deliberately sparse, because a parameter that some figures of a collection carry and
/// others do not is the normal case as soon as two experiments are compared.
fn collection() -> Vec<FigureCard> {
    vec![
        card(
            "Run 1",
            &["piv"],
            &[
                ("rig", string("CFD")),
                ("solver", string("SST")),
                ("angle", Parameter::Number(4.0)),
            ],
        ),
        card(
            "Run 2",
            &["piv", "stalled"],
            &[
                ("rig", string("CFD")),
                ("solver", string("LES")),
                ("angle", Parameter::Number(8.0)),
            ],
        ),
        card(
            "Run 9",
            &["tunnel"],
            &[
                ("rig", string("Tunnel")),
                ("angle", Parameter::Number(12.0)),
            ],
        ),
        card("Run 10", &[], &[("rig", string("Tunnel"))]),
    ]
}

/// The facet of the given name, or a panic naming the facets there are.
fn facet<'a>(facets: &'a [Facet], name: &str) -> &'a Facet {
    facets
        .iter()
        .find(|facet| facet.key.name() == name)
        .unwrap_or_else(|| {
            let names: Vec<&str> = facets.iter().map(|facet| facet.key.name()).collect();
            panic!("there is no facet {name:?}; there are {names:?}")
        })
}

/// The titles of the figures a result lists, in the order shown and across every group.
fn titles(results: &Results, cards: &[FigureCard]) -> Vec<String> {
    results
        .members()
        .map(|index| cards[index].title.clone())
        .collect()
}

/// The result as its groups, each named and holding the titles of its figures in order.
fn groups(results: &Results, cards: &[FigureCard]) -> Vec<(String, Vec<String>)> {
    results
        .groups
        .iter()
        .map(|group| {
            (
                group.name.clone().unwrap_or_default(),
                group
                    .members
                    .iter()
                    .map(|index| cards[*index].title.clone())
                    .collect(),
            )
        })
        .collect()
}

/// The hash of a value under the default hasher.
fn hash_of(value: &FacetValue) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// An axes of the given artists, two-dimensional and in the first cell unless changed by the
/// caller.
fn axes(id: u64, artists: Vec<Artist>) -> Axes {
    Axes {
        id: NodeId(id),
        artists,
        ..Axes::default()
    }
}

/// A figure of the given axes and data arrays.
fn figure(axes: Vec<Axes>, data: &[(u64, NdArray)]) -> Figure {
    Figure {
        id: NodeId(1),
        layout: TileLayout {
            rows: 1,
            cols: axes.len().max(1) as u32,
        },
        axes,
        data: data
            .iter()
            .map(|(id, array)| (DataId(*id), array.clone()))
            .collect(),
        ..Figure::new()
    }
}

// ---------------------------------------------------------------------------------
// Facet values
// ---------------------------------------------------------------------------------

// Why: the counts beside a facet's values are kept in an ordered map, so the order of the forms
// decides the order the interface lists them in. A collection whose figures disagree about the
// form of a parameter — an angle written as a whole number by one script and as a decimal by
// another — must still list its values in one settled order rather than shuffling between frames.
#[test]
fn the_forms_of_a_value_are_ordered_booleans_integers_numbers_then_text() {
    let mut values = vec![
        text("beta"),
        FacetValue::Number(0.0),
        FacetValue::Integer(7),
        text("Alpha"),
        FacetValue::Bool(true),
        FacetValue::Number(-1.5),
        FacetValue::Integer(-2),
        FacetValue::Bool(false),
    ];
    values.sort();
    assert_eq!(
        values,
        [
            FacetValue::Bool(false),
            FacetValue::Bool(true),
            FacetValue::Integer(-2),
            FacetValue::Integer(7),
            FacetValue::Number(-1.5),
            FacetValue::Number(0.0),
            text("Alpha"),
            text("beta"),
        ],
        "the forms are ordered among themselves, and the values of one form among each other"
    );
    assert_ne!(
        FacetValue::Integer(1),
        FacetValue::Number(1.0),
        "a whole number written as an integer is a different value from the same number written \
         as a double, because the two forms are counted and listed separately"
    );
}

// Why: `total_cmp` is a total order over every double, including the two zeros and the non-finite
// values a figure should not carry but might. If `-0.0` and `0.0` compared equal while hashing
// differently, or the reverse, a facet counting its values in a map would either lose one of them
// or count one twice; pinning that they are distinct under both settles it in one direction.
#[test]
fn positive_and_negative_zero_are_distinct_values_that_order_and_hash_apart() {
    let (negative, positive) = (FacetValue::Number(-0.0), FacetValue::Number(0.0));
    assert_ne!(negative, positive, "the two zeros are different values");
    assert!(
        negative < positive,
        "negative zero sorts below positive zero"
    );
    assert_ne!(
        hash_of(&negative),
        hash_of(&positive),
        "hashing by the bits keeps the two zeros apart, as the ordering does"
    );

    let mut counts: BTreeMap<FacetValue, usize> = BTreeMap::new();
    *counts.entry(FacetValue::Number(-0.0)).or_default() += 1;
    *counts.entry(FacetValue::Number(0.0)).or_default() += 1;
    assert_eq!(
        counts.len(),
        2,
        "a facet holding both zeros offers both of them"
    );
}

// Why: the facet counts are a map keyed by the value, so a value that occurs in two figures must
// land in one entry. Values that compare equal but hash apart would count each figure separately
// and show a facet of duplicated values, which is the failure the documented hashing rule exists
// to prevent.
#[test]
fn values_that_are_equal_hash_alike() {
    let pairs = [
        (FacetValue::Bool(true), FacetValue::Bool(true)),
        (FacetValue::Integer(-9), FacetValue::Integer(-9)),
        (FacetValue::Number(2.5), FacetValue::Number(2.5)),
        (text("PIV"), FacetValue::Text("PIV".to_owned())),
    ];
    for (left, right) in pairs {
        assert_eq!(left, right, "{left:?} and {right:?} are the same value");
        assert_eq!(
            hash_of(&left),
            hash_of(&right),
            "{left:?} and {right:?} are equal, so they must hash alike"
        );
    }
}

// Why: the text of a value is what the reader clicks on in the sidebar and what a search term is
// compared against, so it must read as a statement about the world rather than as the contents of
// a file. "yes" and "no" say whether the run stalled; "true" and "false" say what is stored. A
// number must lose the noise of its binary fraction without losing the digits that tell one value
// of a facet from the next.
#[test]
fn the_text_of_a_value_is_what_the_interface_writes() {
    assert_eq!(FacetValue::Bool(true).text(), "yes");
    assert_eq!(FacetValue::Bool(false).text(), "no");
    assert_eq!(FacetValue::Integer(-3).text(), "-3");
    assert_eq!(FacetValue::Number(1.5).text(), "1.5");
    assert_eq!(
        FacetValue::Number(0.0).text(),
        "0",
        "a whole number keeps no empty fraction"
    );
    assert_eq!(
        FacetValue::Number(0.1 + 0.2).text(),
        "0.3",
        "the digits of the binary fraction are noise, and the reader is shown the number they meant"
    );
    assert_eq!(
        FacetValue::Number(1.0e-5).text(),
        "1.0000e-5",
        "a number too small to write plainly is written in exponent form rather than as zero"
    );
    assert_eq!(text("PIV").text(), "PIV");
}

// Why: a range narrows a facet by comparing numbers, and a query such as `angle>=8` does the same.
// Both ask a value for its number, and both must be told plainly that a label or a flag has none
// rather than being given a number that stands for one.
#[test]
fn only_the_numeric_forms_have_a_number() {
    assert_eq!(FacetValue::Integer(4).as_number(), Some(4.0));
    assert_eq!(FacetValue::Number(4.5).as_number(), Some(4.5));
    assert_eq!(FacetValue::Bool(true).as_number(), None);
    assert_eq!(text("4").as_number(), None);
}

// ---------------------------------------------------------------------------------
// What is read off a figure
// ---------------------------------------------------------------------------------

// Why: a free-text term is matched against one string, and what is in that string decides what
// typing a word can find. Parameter names belong in it so that someone who half remembers a
// parameter can type its name and see which figures have one; the values and the labels belong in
// it so that a word from any of them finds the figure. Folding to lower case is what lets the
// reader type as they would speak.
#[test]
fn the_free_text_of_a_card_holds_its_title_labels_and_parameter_names_and_values() {
    let card = card(
        "Wake survey",
        &["PIV"],
        &[
            ("rig", string("Tunnel")),
            ("stalled", Parameter::Bool(true)),
        ],
    );
    let haystack = card.haystack();
    for wanted in ["wake survey", "piv", "rig", "tunnel", "stalled", "yes"] {
        assert!(
            haystack.contains(wanted),
            "the searchable text {haystack:?} must contain {wanted:?}"
        );
    }
    assert!(
        !haystack.contains("true"),
        "a boolean is searched for as the interface writes it, not as the file stores it"
    );
}

// ---------------------------------------------------------------------------------
// Which facets are worth offering
// ---------------------------------------------------------------------------------

// Why: the labels are the facet the user wrote in order to browse by, so no arithmetic over the
// collection may push them below a parameter that happens to divide it more evenly. Putting them
// first is what makes the sidebar match the reason the labels exist.
#[test]
fn the_labels_are_the_first_facet_offered() {
    let facets = describe_facets(&collection());
    assert_eq!(
        facets.first().map(|facet| facet.key.clone()),
        Some(FacetKey::Labels)
    );
    let labels = facet(&facets, "labels");
    assert_eq!(labels.kind, FacetKind::Labels);
    assert_eq!(
        labels.present, 3,
        "three of the four figures carry a label at all"
    );
    assert_eq!(
        labels.values,
        [text("piv"), text("stalled"), text("tunnel")],
        "every label of the collection is offered, once, in ascending order"
    );
    assert_eq!(labels.range, None, "labels are not narrowed by a range");
}

// Why: a facet every figure agrees on divides nothing: ticking its one value leaves the collection
// exactly as it was. Offering it costs the reader a row of the sidebar and the moment it takes to
// find out it was useless, so its score must be zero, which is how the sidebar knows to leave it
// out.
#[test]
fn a_facet_with_one_value_scores_zero_because_it_divides_nothing() {
    let cards = vec![
        card(
            "A",
            &[],
            &[("rig", string("CFD")), ("solver", string("SST"))],
        ),
        card(
            "B",
            &[],
            &[("rig", string("CFD")), ("solver", string("LES"))],
        ),
    ];
    let facets = describe_facets(&cards);
    assert_eq!(
        facet(&facets, "rig").score,
        0.0,
        "every figure has the same rig, so the facet divides nothing and scores zero"
    );
    assert!(
        facet(&facets, "solver").score > 0.0,
        "a facet that does divide the collection is worth offering"
    );
    assert_eq!(
        facets.last().map(|facet| facet.key.name()),
        Some("rig"),
        "the facet that scores zero comes last, behind everything worth offering"
    );
}

// Why: the labels lead the sidebar because they are what the user wrote in order to browse by,
// but a single label shared by every figure divides the collection no more than a parameter of one
// value does. Exempting the labels from that rule would put a row at the top of the sidebar whose
// only value keeps everything, which is precisely what the score exists to keep out.
#[test]
fn a_single_label_shared_by_every_figure_scores_zero_like_any_other_facet() {
    let cards = [
        card("A", &["2d"], &[("rig", string("CFD"))]),
        card("B", &["2d"], &[("rig", string("Tunnel"))]),
    ];
    let facets = describe_facets(&cards);
    let labels = facet(&facets, "labels");
    assert_eq!(
        labels.values,
        [text("2d")],
        "there is one label in the collection"
    );
    assert_eq!(
        labels.score, 0.0,
        "the one label divides nothing, and leading the sidebar is not a reason to offer it"
    );
    assert_eq!(
        facets.first().map(|facet| facet.key.name()),
        Some("rig"),
        "the facet that does divide the collection leads instead"
    );
}

// Why: a run number or a note has a different value for nearly every figure. As a facet it is a
// list as long as the collection and every row of it leaves one figure, which is a search term
// wearing a facet's clothes. A column of distinct numbers is the opposite case: a range over it
// narrows the collection perfectly well, so the penalty must not reach it.
#[test]
fn a_nearly_unique_text_facet_is_penalised_and_a_numeric_one_is_not() {
    let cards: Vec<FigureCard> = (0..6)
        .map(|index| {
            card(
                &format!("Run {index}"),
                &[],
                &[
                    ("note", string(&format!("note {index}"))),
                    ("run", Parameter::Integer(index)),
                ],
            )
        })
        .collect();
    let facets = describe_facets(&cards);
    let note = facet(&facets, "note");
    let run = facet(&facets, "run");

    assert_eq!(note.cardinality(), 6);
    assert_eq!(run.cardinality(), 6);
    assert!(
        note.score < run.score,
        "the text facet names its figures ({}) where the numeric one can be ranged over ({})",
        note.score,
        run.score
    );
    assert!(
        note.score > 0.0,
        "the naming facet is penalised, not removed: it is still a facet"
    );
    assert_eq!(
        facets.first().map(|facet| facet.key.name()),
        Some("run"),
        "the facet worth using is offered before the one that is nearly a name"
    );
    assert_eq!(
        run.range,
        Some((0.0, 5.0)),
        "a numeric facet carries the ends of its range, so the interface can offer a slider"
    );
}

// Why: the sidebar is rebuilt on every frame. If two facets that divide the collection equally
// well could swap places between frames, a value would move out from under the reader's pointer.
// Ordering equal scores by name is what makes the list the same list every time it is built.
#[test]
fn facets_of_equal_score_are_ordered_by_name() {
    let cards = vec![
        card(
            "A",
            &[],
            &[("zulu", string("one")), ("alpha", string("one"))],
        ),
        card(
            "B",
            &[],
            &[("zulu", string("two")), ("alpha", string("two"))],
        ),
    ];
    let facets = describe_facets(&cards);
    let names: Vec<&str> = facets.iter().map(|facet| facet.key.name()).collect();
    assert_eq!(
        names,
        ["alpha", "zulu"],
        "the two facets divide the collection identically, so their names settle the order"
    );
    assert_eq!(
        facet(&facets, "alpha").score,
        facet(&facets, "zulu").score,
        "the two scores are the same, which is what makes the name the tie-breaker"
    );
}

// Why: one script writes an angle as 4 and another writes it as 4.5, which is a difference in how
// the number was recorded and not a disagreement about what the column holds. Calling such a
// column text would take the range away from exactly the parameters most worth having one, and
// would offer the reader a list of every distinct angle in place of a slider.
#[test]
fn a_facet_whose_values_are_numbers_of_both_forms_is_still_numeric() {
    let cards = [
        card("A", &[], &[("angle", Parameter::Integer(4))]),
        card("B", &[], &[("angle", Parameter::Number(4.5))]),
        card("C", &[], &[("angle", Parameter::Integer(12))]),
    ];
    let facets = describe_facets(&cards);
    let angle = facet(&facets, "angle");
    assert_eq!(angle.kind, FacetKind::Number);
    assert_eq!(
        angle.range,
        Some((4.0, 12.0)),
        "the range spans the values of both forms"
    );
    assert!(
        angle.score > 0.9,
        "a numeric facet is exempt from the penalty on a facet of nearly distinct values, so the \
         column of three distinct angles is not cut to a tenth of its score ({})",
        angle.score
    );
}

// Why: nothing stops two figures of a collection from writing the same parameter in different
// forms, because they were written by different scripts. The facet must still be offerable, and
// text is the only form every value can be written in; degrading to it keeps the facet usable
// instead of letting one figure's choice of form decide what the others may be narrowed by.
#[test]
fn a_facet_whose_values_disagree_about_their_form_is_offered_as_text() {
    let cards = vec![
        card("A", &[], &[("angle", Parameter::Integer(4))]),
        card("B", &[], &[("angle", string("eight"))]),
        card("C", &[], &[("angle", Parameter::Bool(true))]),
    ];
    let facets = describe_facets(&cards);
    let angle = facet(&facets, "angle");
    assert_eq!(angle.kind, FacetKind::Text);
    assert_eq!(
        angle.range, None,
        "a facet that is not numeric has no range, whatever forms its values take"
    );
    assert_eq!(
        angle.values,
        [
            FacetValue::Bool(true),
            FacetValue::Integer(4),
            text("eight")
        ],
        "every value is still offered, ordered by form so that the list is settled"
    );
}

// Why: the forms a facet offers decide how it is narrowed, and only the numeric forms are narrowed
// by a range. Getting this wrong would offer a slider over a set of solver names, or a list of
// every distinct Reynolds number in the collection.
#[test]
fn a_facet_takes_the_form_its_values_share() {
    let cards = vec![
        card(
            "A",
            &["piv"],
            &[
                ("stalled", Parameter::Bool(false)),
                ("cells", Parameter::Integer(1_000)),
                ("angle", Parameter::Number(4.0)),
                ("rig", string("CFD")),
            ],
        ),
        card(
            "B",
            &["piv"],
            &[
                ("stalled", Parameter::Bool(true)),
                ("cells", Parameter::Integer(2_000)),
                ("angle", Parameter::Number(8.0)),
                ("rig", string("Tunnel")),
            ],
        ),
    ];
    let facets = describe_facets(&cards);
    assert_eq!(facet(&facets, "labels").kind, FacetKind::Labels);
    assert_eq!(facet(&facets, "stalled").kind, FacetKind::Bool);
    assert_eq!(facet(&facets, "cells").kind, FacetKind::Integer);
    assert_eq!(facet(&facets, "angle").kind, FacetKind::Number);
    assert_eq!(facet(&facets, "rig").kind, FacetKind::Text);
    assert!(
        !FacetKind::Bool.is_numeric() && !FacetKind::Text.is_numeric(),
        "a flag and a name are chosen from, not ranged over"
    );
    assert!(
        FacetKind::Integer.is_numeric() && FacetKind::Number.is_numeric(),
        "both numeric forms are ranged over"
    );
    assert_eq!(facet(&facets, "cells").range, Some((1000.0, 2000.0)));
    assert_eq!(facet(&facets, "rig").range, None);
}

// Why: a facet only some figures carry is less use than one they all carry, however evenly it
// divides what it covers, because narrowing by it throws away every figure that is silent on it.
// Coverage is half of the score for that reason, and a facet must report how much of the
// collection it reaches so that the interface can say so.
#[test]
fn a_facet_that_covers_more_of_the_collection_scores_above_one_that_covers_less() {
    let cards = vec![
        card(
            "A",
            &[],
            &[("rig", string("CFD")), ("solver", string("SST"))],
        ),
        card("B", &[], &[("rig", string("Tunnel"))]),
        card("C", &[], &[("rig", string("CFD"))]),
        card("D", &[], &[("rig", string("Tunnel"))]),
    ];
    let facets = describe_facets(&cards);
    assert_eq!(facet(&facets, "rig").present, 4);
    assert_eq!(facet(&facets, "solver").present, 1);
    assert!(
        facet(&facets, "rig").score > facet(&facets, "solver").score,
        "the facet that reaches the whole collection is the more useful one"
    );
}

// ---------------------------------------------------------------------------------
// The query language
// ---------------------------------------------------------------------------------

/// An "anywhere" term.
fn anywhere(text: &str, negated: bool) -> Term {
    Term::Anywhere {
        text: text.to_owned(),
        negated,
    }
}

/// A named term.
fn named(key: &str, comparison: Comparison, value: &str, negated: bool) -> Term {
    Term::Named {
        key: key.to_owned(),
        comparison,
        value: value.to_owned(),
        negated,
    }
}

/// The parameter names a collection is taken to have, for the tests that parse text without
/// building the figures the text is asked about.
fn known(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

// Why: the table in the module's documentation is the whole of what a user is told about the
// search field, so every row of it must parse to the term it promises. A form that parsed to
// something else would be a lie told in the documentation and discovered only by a reader whose
// search quietly returned the wrong figures.
#[test]
fn every_documented_form_of_a_term_parses_as_the_table_says() {
    let names = known(&["rig", "angle"]);
    let query = Query::parse(
        "surface -stalled rig:CFD rig=CFD angle>=8 label:piv",
        &names,
    );
    assert_eq!(
        query.terms,
        [
            anywhere("surface", false),
            anywhere("stalled", true),
            named("rig", Comparison::Contains, "CFD", false),
            named("rig", Comparison::Equals, "CFD", false),
            named("angle", Comparison::AtLeast, "8", false),
            named("label", Comparison::Contains, "piv", false),
        ]
    );
    assert_eq!(
        Query::parse("angle>8 angle<8 angle<=8", &names).terms,
        [
            named("angle", Comparison::Above, "8", false),
            named("angle", Comparison::Below, "8", false),
            named("angle", Comparison::AtMost, "8", false),
        ],
        "the strict and inclusive comparisons are told apart, in both directions"
    );
    assert!(
        Query::parse("", &names).is_empty() && Query::parse("   ", &names).is_empty(),
        "an empty field asks nothing"
    );
}

// Why: the user types into the field one character at a time, so every half-finished thing they
// can type passes through the parser. A minus on its own is what a negation looks like before the
// word arrives; it must ask nothing rather than become a term that matches nothing and empties the
// list under the reader's hands.
#[test]
fn a_bare_minus_asks_nothing() {
    let names = known(&["rig"]);
    assert!(Query::parse("-", &names).is_empty());
    assert!(
        Query::parse("- -", &names).is_empty(),
        "several of them are still nothing"
    );
    assert_eq!(
        Query::parse("rig:CFD -", &names).terms.len(),
        1,
        "the finished term beside it still stands"
    );
    assert!(
        Query::parse("-", &names).matches(&card("Run 1", &[], &[])),
        "a query that asks nothing keeps every figure"
    );
}

// Why: `is_unfiltered` decides whether the control that takes back what the reader has done is
// offered, so it asks what has been entered rather than what came of it. A reader who has typed a
// stray character needs that control to be there, even though the list in front of them is the
// whole collection; without it the only way out of a field they did not mean to type in would be
// to work out which character to delete.
#[test]
fn text_that_asks_nothing_is_still_something_to_clear() {
    let cards = collection();
    let mut browse = Browse {
        query: "-".to_owned(),
        ..Browse::default()
    };
    assert_eq!(
        browse.results(&cards).matched,
        cards.len(),
        "a lone minus asks nothing, so every figure is still in the list"
    );
    assert!(
        !browse.is_unfiltered(),
        "there is something in the search field, so there is something to take back"
    );
    browse.clear();
    assert!(
        browse.is_unfiltered(),
        "taking it back leaves nothing entered, and the control goes away"
    );
}

// Why: a name followed by an operator and nothing at all is what the user has typed halfway
// through writing `rig=CFD`. The documentation promises that anything which is not one of the
// listed forms is free text, so this must fall back to free text rather than become a comparison
// against an empty string, which every value contains and which would therefore quietly do
// nothing.
#[test]
fn a_name_and_an_operator_with_nothing_after_it_is_free_text() {
    let names = known(&["rig", "angle"]);
    assert_eq!(
        Query::parse("rig=", &names).terms,
        [anywhere("rig=", false)],
        "half a comparison is text, not a comparison, even though the collection has the parameter"
    );
    assert_eq!(
        Query::parse("angle>=", &names).terms,
        [anywhere("angle>=", false)]
    );
    assert_eq!(
        Query::parse("-rig:", &names).terms,
        [anywhere("rig:", true)],
        "the negation survives the fall back to free text"
    );
}

// Why: a word with a colon in it is a question about a parameter only when the collection has one
// of that name; otherwise it is a web address, a file path or a colon in a title, and must narrow
// the list to the figures that mention it rather than silently emptying it. This is why the
// collection's own names are what the parser is given: the same word means different things in
// different collections, and only the figures can say which.
#[test]
fn a_word_with_a_colon_that_names_no_parameter_is_free_text() {
    let cards = [
        card("speed:high", &[], &[("rig", string("CFD"))]),
        card("speed:low", &[], &[("rig", string("Tunnel"))]),
    ];
    let names = parameter_names(&cards);
    assert_eq!(
        names,
        known(&["rig"]),
        "the collection offers one parameter, and it is not called \"speed\""
    );

    let query = Query::parse("speed:high", &names);
    assert_eq!(
        query.terms,
        [anywhere("speed:high", false)],
        "the word names no parameter of this collection, so the whole of it is text to search for"
    );
    assert!(
        query.matches(&cards[0]),
        "the figure whose title contains the text is what the reader was looking for"
    );
    assert!(
        !query.matches(&cards[1]),
        "the other figure does not contain the text"
    );
    assert_eq!(
        Query::parse("rig:CFD", &names).terms,
        [named("rig", Comparison::Contains, "CFD", false)],
        "a word naming a parameter the collection does have is a question about it"
    );
}

// Why: the labels are not a parameter and can never appear among a collection's parameter names,
// so a rule that only the collection's names make a named term would put them out of reach of the
// query language altogether. They are the one name that always asks what it looks like it asks.
#[test]
fn labels_are_asked_about_by_name_although_they_are_not_a_parameter() {
    let cards = [card("Run 1", &["piv"], &[("rig", string("CFD"))])];
    let names = parameter_names(&cards);
    assert!(
        !names.contains("label") && !names.contains("labels"),
        "no figure carries a parameter of either name"
    );
    assert_eq!(
        Query::parse("label:piv", &names).terms,
        [named("label", Comparison::Contains, "piv", false)]
    );
    assert!(Query::parse("labels=PIV", &names).matches(&cards[0]));
}

// Why: a number in a figure's parameters was written by the code that produced it, and a reader
// looking for a mesh of two million cells will type the number the way that code wrote it. Reading
// the underscores as separators is what lets the query read the same as the source.
#[test]
fn underscores_inside_a_typed_number_are_separators() {
    let cards = [
        card("Coarse", &[], &[("cells", Parameter::Integer(1_000_000))]),
        card("Fine", &[], &[("cells", Parameter::Integer(4_000_000))]),
    ];
    let names = parameter_names(&cards);
    let query = Query::parse("cells>=2_000_000", &names);
    assert!(!query.matches(&cards[0]));
    assert!(query.matches(&cards[1]));
    assert!(
        Query::parse("cells>=2000000", &names).matches(&cards[1]),
        "the same number without separators asks the same question"
    );
}

// Why: the reader types what they remember, not what the file stores, and they do not remember
// whether the solver was written "SST" or "sst". A comparison that was case-sensitive would return
// nothing and give no reason, which is the worst answer a search can give.
#[test]
fn a_comparison_against_text_ignores_case() {
    let cards = [card("Run 1", &["PIV"], &[("solver", string("SST"))])];
    let (names, card) = (parameter_names(&cards), &cards[0]);
    assert!(Query::parse("solver:sst", &names).matches(card));
    assert!(Query::parse("solver:SST", &names).matches(card));
    assert!(Query::parse("solver=sSt", &names).matches(card));
    assert!(
        !Query::parse("solver=ss", &names).matches(card),
        "an exact comparison is still exact: only its case is forgiven"
    );
    assert!(
        Query::parse("solver:ss", &names).matches(card),
        "a containing comparison finds part of the value"
    );
    assert!(
        Query::parse("label:piv", &names).matches(card)
            && Query::parse("labels:PIV", &names).matches(card),
        "labels are asked about by either name, in either case"
    );
    assert!(
        Query::parse("SST", &names).matches(card),
        "free text ignores case too"
    );
}

// Why: asking about a solver is asking for the figures that have one. A measured run has no solver
// at all, and treating its silence as a match would put it in the results of every question ever
// asked about a parameter it does not carry, which would make a named term useless for exactly the
// sparse collections the module exists to browse.
#[test]
fn a_named_term_does_not_match_a_figure_that_lacks_the_parameter() {
    let cards = [
        card(
            "Computed",
            &[],
            &[("solver", string("LES")), ("angle", Parameter::Number(4.0))],
        ),
        card("Measured", &[], &[("rig", string("Tunnel"))]),
    ];
    let names = parameter_names(&cards);
    let (computed, measured) = (&cards[0], &cards[1]);
    assert!(Query::parse("solver:LES", &names).matches(computed));
    assert!(!Query::parse("solver:LES", &names).matches(measured));
    assert!(
        Query::parse("angle>=0", &names).matches(computed)
            && !Query::parse("angle>=0", &names).matches(measured),
        "a comparison against a number holds for the figure that carries one and fails for the \
         figure that does not, rather than failing for both"
    );
    assert!(
        Query::parse("-solver:LES", &names).matches(measured),
        "the figure that is not what was asked for is what the negation asks for"
    );
    assert!(
        !Query::parse("label:piv", &names).matches(measured),
        "a figure with no labels is not found by a question about them"
    );
}

// Why: terms are cumulative, so typing another word always narrows the result. A reader refines a
// search by adding to it, and a field where a second word widened what was found would be
// unusable.
#[test]
fn every_term_of_a_query_must_hold() {
    let cards = [card(
        "Run 1",
        &["piv"],
        &[("rig", string("CFD")), ("angle", Parameter::Number(8.0))],
    )];
    let (names, card) = (parameter_names(&cards), &cards[0]);
    assert!(Query::parse("rig:CFD angle>=8 label:piv run", &names).matches(card));
    assert!(
        !Query::parse("rig:CFD angle>=12", &names).matches(card),
        "one term that does not hold rules the figure out"
    );
    assert!(
        !Query::parse("rig:CFD -piv", &names).matches(card),
        "a negated term is a term like any other"
    );
}

// ---------------------------------------------------------------------------------
// Refinements
// ---------------------------------------------------------------------------------

// Why: a refinement is a statement about a parameter, and a figure that does not carry the
// parameter cannot satisfy one. Letting it through would mean that narrowing by a solver returned
// the measured runs as well, which is the opposite of narrowing.
#[test]
fn a_figure_that_lacks_the_facet_never_passes_a_filter() {
    let measured = card("Measured", &[], &[("rig", string("Tunnel"))]);
    let chosen = Filter {
        key: FacetKey::parameter("solver"),
        constraint: Constraint::AnyOf(vec![text("LES")]),
    };
    let ranged = Filter {
        key: FacetKey::parameter("angle"),
        constraint: Constraint::Between {
            low: 0.0,
            high: 20.0,
        },
    };
    let labelled = Filter {
        key: FacetKey::Labels,
        constraint: Constraint::AnyOf(vec![text("piv")]),
    };
    assert!(!chosen.matches(&measured));
    assert!(!ranged.matches(&measured));
    assert!(
        !labelled.matches(&measured),
        "a figure with no labels is outside every refinement of the labels"
    );
}

// Why: a figure carries several labels, so a refinement of the labels asks whether any of them is
// one of the chosen ones, and a range over a set of labels is not a question that can be asked at
// all. Answering it with anything but "no figure" would invent an order for text that has none.
#[test]
fn a_label_filter_keeps_a_figure_that_carries_any_chosen_label() {
    let card = card("Run 2", &["piv", "stalled"], &[]);
    let either = Filter {
        key: FacetKey::Labels,
        constraint: Constraint::AnyOf(vec![text("tunnel"), text("stalled")]),
    };
    let neither = Filter {
        key: FacetKey::Labels,
        constraint: Constraint::AnyOf(vec![text("tunnel")]),
    };
    let ranged = Filter {
        key: FacetKey::Labels,
        constraint: Constraint::Between {
            low: 0.0,
            high: 1.0,
        },
    };
    assert!(either.matches(&card), "one of the two labels is chosen");
    assert!(!neither.matches(&card));
    assert!(
        !ranged.matches(&card),
        "labels cannot be narrowed by a range"
    );
}

// Why: a range is inclusive at both ends, because the reader who drags a slider to 8 means to keep
// the run at 8. It must also read the number out of whichever numeric form the figure wrote it in,
// and refuse a value that has no number at all rather than ordering text by its characters.
#[test]
fn a_range_keeps_the_numbers_from_its_low_end_to_its_high_end_inclusive() {
    let filter = Filter {
        key: FacetKey::parameter("angle"),
        constraint: Constraint::Between {
            low: 4.0,
            high: 8.0,
        },
    };
    let at = |value: Parameter| card("Run", &[], &[("angle", value)]);
    assert!(filter.matches(&at(Parameter::Number(4.0))), "the low end");
    assert!(filter.matches(&at(Parameter::Number(8.0))), "the high end");
    assert!(filter.matches(&at(Parameter::Integer(6))));
    assert!(!filter.matches(&at(Parameter::Number(3.9))));
    assert!(!filter.matches(&at(Parameter::Number(8.1))));
    assert!(
        !filter.matches(&at(string("6"))),
        "a number written as text has no number to compare"
    );
}

// Why: the values of one facet are alternatives, so choosing a second value must widen the result
// rather than narrow it to nothing. This is the rule that makes ticking another box safe, and it
// is the whole reason the constraint is a list rather than a single value.
#[test]
fn choosing_several_values_of_one_facet_keeps_a_figure_with_any_of_them() {
    let filter = Filter {
        key: FacetKey::parameter("rig"),
        constraint: Constraint::AnyOf(vec![text("CFD"), text("Tunnel")]),
    };
    assert!(filter.matches(&card("A", &[], &[("rig", string("CFD"))])));
    assert!(filter.matches(&card("B", &[], &[("rig", string("Tunnel"))])));
    assert!(!filter.matches(&card("C", &[], &[("rig", string("Water"))])));
}

// ---------------------------------------------------------------------------------
// Making and unmaking choices
// ---------------------------------------------------------------------------------

// Why: this is what clicking a value does, and clicking it again must undo exactly what the first
// click did. Leaving an empty list of alternatives behind when the last value is unticked would
// leave a refinement that matches nothing while the interface shows nothing ticked, which the
// reader would have no way to undo.
#[test]
fn toggling_a_value_adds_it_removes_it_and_drops_the_filter_with_the_last_one() {
    let mut browse = Browse::default();
    let rig = FacetKey::parameter("rig");

    browse.toggle(&rig, &text("CFD"));
    assert_eq!(
        browse.filter(&rig).map(|filter| filter.constraint.clone()),
        Some(Constraint::AnyOf(vec![text("CFD")]))
    );

    browse.toggle(&rig, &text("Tunnel"));
    assert_eq!(
        browse.filter(&rig).map(|filter| filter.constraint.clone()),
        Some(Constraint::AnyOf(vec![text("CFD"), text("Tunnel")])),
        "a second value is an alternative to the first, not a replacement"
    );
    assert_eq!(browse.filters.len(), 1, "one facet has one filter");

    browse.toggle(&rig, &text("CFD"));
    assert_eq!(
        browse.filter(&rig).map(|filter| filter.constraint.clone()),
        Some(Constraint::AnyOf(vec![text("Tunnel")])),
        "unticking one value leaves the others"
    );

    browse.toggle(&rig, &text("Tunnel"));
    assert_eq!(browse.filter(&rig), None, "the last value takes the filter");
    assert!(
        browse.filters.is_empty() && browse.is_unfiltered(),
        "nothing is chosen, so the result is the whole collection again"
    );
}

// Why: a facet can be narrowed by a range or by chosen values but not by both at once, because the
// two ask different questions of the same parameter. Clicking a value is the plainer statement, so
// it replaces the range rather than being refused or silently ignored.
#[test]
fn choosing_a_value_replaces_a_range_on_the_same_facet() {
    let facet = Facet {
        key: FacetKey::parameter("angle"),
        kind: FacetKind::Number,
        present: 3,
        values: vec![
            FacetValue::Number(4.0),
            FacetValue::Number(8.0),
            FacetValue::Number(12.0),
        ],
        range: Some((4.0, 12.0)),
        score: 1.0,
    };
    let mut browse = Browse::default();
    browse.set_range(&facet, 4.0, 8.0);
    assert_eq!(
        browse.filter(&facet.key).map(|f| f.constraint.clone()),
        Some(Constraint::Between {
            low: 4.0,
            high: 8.0
        })
    );

    browse.toggle(&facet.key, &FacetValue::Number(12.0));
    assert_eq!(
        browse.filter(&facet.key).map(|f| f.constraint.clone()),
        Some(Constraint::AnyOf(vec![FacetValue::Number(12.0)])),
        "the chosen value is now the whole of the refinement"
    );
    assert_eq!(
        browse.filters.len(),
        1,
        "the facet is still narrowed once, not twice"
    );
}

// Why: a range from one end of a facet to the other keeps every figure that has the parameter, so
// it is not a refinement. Showing a chip for it would tell the reader they have narrowed something
// when they have not, and would leave them a chip to clear that never did anything.
#[test]
fn a_range_that_covers_the_whole_facet_is_no_refinement_at_all() {
    let facet = Facet {
        key: FacetKey::parameter("angle"),
        kind: FacetKind::Number,
        present: 3,
        values: vec![FacetValue::Number(4.0), FacetValue::Number(12.0)],
        range: Some((4.0, 12.0)),
        score: 1.0,
    };
    let mut browse = Browse::default();

    browse.set_range(&facet, 4.0, 12.0);
    assert!(
        browse.filters.is_empty(),
        "a range that is exactly the facet's own range narrows nothing"
    );

    browse.set_range(&facet, 0.0, 100.0);
    assert!(
        browse.filters.is_empty(),
        "a range wider than the facet narrows nothing either"
    );

    browse.set_range(&facet, 5.0, 12.0);
    assert_eq!(
        browse.filter(&facet.key).map(|f| f.constraint.clone()),
        Some(Constraint::Between {
            low: 5.0,
            high: 12.0
        }),
        "a range that does leave something out is a refinement"
    );

    browse.set_range(&facet, 4.0, 12.0);
    assert!(
        browse.filters.is_empty(),
        "widening a range back to the whole facet takes the refinement away again"
    );
}

// ---------------------------------------------------------------------------------
// Counting what each choice would leave
// ---------------------------------------------------------------------------------

// Why: this is the rule that makes a faceted sidebar usable. A facet counted with its own
// refinement applied would show a count beside the chosen value and zero beside every alternative
// to it, so the reader could never see what choosing a second value would add and would have to
// untick before they could look. Counting a facet as though it were not refined is what makes
// ticking a second box a widening the reader can see the size of before they click.
#[test]
fn a_facets_own_refinement_is_left_out_of_its_own_counts() {
    let cards = collection();
    let facets = describe_facets(&cards);
    let rig = facet(&facets, "rig");

    let mut browse = Browse::default();
    browse.toggle(&rig.key, &text("CFD"));

    assert_eq!(
        browse.counts(&cards, rig),
        BTreeMap::from([(text("CFD"), 2), (text("Tunnel"), 2)]),
        "the alternative to the chosen rig still shows what choosing it would add"
    );
}

// Why: facets are cumulative, so the count beside a value must be how many figures that value
// would leave given everything else the reader has already chosen. Counting a facet without the
// other facets' refinements would promise figures that the other choices have already ruled out.
#[test]
fn the_other_facets_refinements_are_applied_to_a_facets_counts() {
    let cards = collection();
    let facets = describe_facets(&cards);
    let (rig, solver) = (facet(&facets, "rig"), facet(&facets, "solver"));

    let mut browse = Browse::default();
    browse.toggle(&rig.key, &text("CFD"));

    assert_eq!(
        browse.counts(&cards, solver),
        BTreeMap::from([(text("LES"), 1), (text("SST"), 1)]),
        "only the two figures from the chosen rig are counted"
    );

    browse.query = "stalled".to_owned();
    assert_eq!(
        browse.counts(&cards, solver),
        BTreeMap::from([(text("LES"), 1), (text("SST"), 0)]),
        "what was typed narrows the counts as a refinement does"
    );
}

// Why: a value that vanished as the reader reached for it would be a worse answer than a value
// shown as unavailable, and a facet whose rows appeared and disappeared as other facets were
// refined would be impossible to read. Every value the facet takes must be present, with the count
// that tells the reader whether it is worth clicking.
#[test]
fn a_value_that_nothing_would_leave_is_counted_as_zero_rather_than_dropped() {
    let cards = collection();
    let facets = describe_facets(&cards);
    let (rig, solver) = (facet(&facets, "rig"), facet(&facets, "solver"));

    let mut browse = Browse::default();
    browse.toggle(&solver.key, &text("SST"));

    let counts = browse.counts(&cards, rig);
    assert_eq!(
        counts,
        BTreeMap::from([(text("CFD"), 1), (text("Tunnel"), 0)]),
        "no figure from the tunnel has a solver, so choosing it would leave nothing, and it is \
         shown as such rather than removed"
    );
    assert_eq!(
        counts.keys().cloned().collect::<Vec<FacetValue>>(),
        rig.values,
        "the counts cover exactly the values the facet offers"
    );
}

// Why: a figure carries several labels and belongs to each of their counts, because the count
// beside a label is how many figures ticking it would show. Counting a figure once for the whole
// facet would understate every label it carries.
#[test]
fn a_figure_is_counted_under_each_of_its_labels() {
    let cards = collection();
    let facets = describe_facets(&cards);
    let labels = facet(&facets, "labels");
    let browse = Browse::default();
    assert_eq!(
        browse.counts(&cards, labels),
        BTreeMap::from([(text("piv"), 2), (text("stalled"), 1), (text("tunnel"), 1)]),
        "the figure carrying two labels is counted under both"
    );
}

// ---------------------------------------------------------------------------------
// The result
// ---------------------------------------------------------------------------------

// Why: figures of a campaign are numbered, and ordering their titles by character would put
// "Run 10" before "Run 9" and scatter a run of ten figures through the list. Reading the digits as
// a number is what makes a numbered collection read in the order it was produced.
#[test]
fn numbers_in_a_title_are_ordered_as_numbers() {
    let cards = collection();
    let browse = Browse::default();
    assert_eq!(
        titles(&browse.results(&cards), &cards),
        ["Run 1", "Run 2", "Run 9", "Run 10"],
        "nine comes before ten"
    );
}

// Why: a figure that does not carry the parameter has no place in an order taken from it, and
// burying it at whichever end the order happens to run to would hide it from a reader who reversed
// the order precisely in order to find it. Putting it last in both directions is the only
// arrangement in which reversing the order means what it says.
#[test]
fn a_figure_without_the_sort_parameter_comes_last_whichever_way_the_order_runs() {
    let cards = collection();
    let mut browse = Browse {
        sort: Sort {
            key: SortKey::Parameter("angle".to_owned()),
            descending: false,
        },
        ..Browse::default()
    };
    assert_eq!(
        titles(&browse.results(&cards), &cards),
        ["Run 1", "Run 2", "Run 9", "Run 10"],
        "ascending by angle, with the figure that has no angle at the end"
    );

    browse.sort.descending = true;
    assert_eq!(
        titles(&browse.results(&cards), &cards),
        ["Run 9", "Run 2", "Run 1", "Run 10"],
        "reversing the order reverses the figures that have an angle and leaves the one that does \
         not at the end"
    );
}

// Why: the list is rebuilt whenever anything changes, so figures the sort cannot tell apart must
// come out in the same order every time. Without a tie-breaker the order of equal figures would be
// whatever the sort happened to do, and a list the reader was halfway down would rearrange itself.
#[test]
fn titles_break_a_tie_in_the_sort() {
    let cards = vec![
        card("Beta", &[], &[("angle", Parameter::Number(4.0))]),
        card("Alpha", &[], &[("angle", Parameter::Number(4.0))]),
        card("Gamma", &[], &[("angle", Parameter::Number(4.0))]),
    ];
    let mut browse = Browse {
        sort: Sort {
            key: SortKey::Parameter("angle".to_owned()),
            descending: false,
        },
        ..Browse::default()
    };
    assert_eq!(
        titles(&browse.results(&cards), &cards),
        ["Alpha", "Beta", "Gamma"]
    );
    browse.sort.descending = true;
    assert_eq!(
        titles(&browse.results(&cards), &cards),
        ["Alpha", "Beta", "Gamma"],
        "the figures the sort cannot tell apart keep the one settled order in both directions"
    );
}

// Why: a parameter written as text is sorted by its text and one written as a number by its
// number, because sorting Reynolds numbers by their characters would be useless and sorting
// solver names by anything else is impossible.
#[test]
fn a_text_parameter_sorts_by_its_text_and_a_numeric_one_by_its_number() {
    let cards = vec![
        card("A", &[], &[("cells", Parameter::Integer(100))]),
        card("B", &[], &[("cells", Parameter::Integer(9))]),
        card("C", &[], &[("cells", Parameter::Integer(20))]),
    ];
    let numeric = Browse {
        sort: Sort {
            key: SortKey::Parameter("cells".to_owned()),
            descending: false,
        },
        ..Browse::default()
    };
    assert_eq!(titles(&numeric.results(&cards), &cards), ["B", "C", "A"]);

    let named = vec![
        card("A", &[], &[("solver", string("SST"))]),
        card("B", &[], &[("solver", string("LES"))]),
    ];
    let textual = Browse {
        sort: Sort {
            key: SortKey::Parameter("solver".to_owned()),
            descending: false,
        },
        ..Browse::default()
    };
    assert_eq!(titles(&textual.results(&named), &named), ["B", "A"]);
}

// Why: grouping by labels is how a reader browses a collection they did not label themselves, and
// a figure with two labels belongs under both: it is in the collection twice as far as the reader
// is concerned. The figures with no labels at all are what the grouping does not apply to, and
// they come last because they are the least likely to be what the reader chose the grouping to
// find.
#[test]
fn a_figure_is_grouped_under_each_of_its_labels_and_the_unlabelled_come_last() {
    let cards = collection();
    let browse = Browse {
        group: Some(FacetKey::Labels),
        ..Browse::default()
    };
    let results = browse.results(&cards);
    assert_eq!(
        groups(&results, &cards),
        [
            (
                "piv".to_owned(),
                vec!["Run 1".to_owned(), "Run 2".to_owned()]
            ),
            ("stalled".to_owned(), vec!["Run 2".to_owned()]),
            ("tunnel".to_owned(), vec!["Run 9".to_owned()]),
            (NO_LABELS.to_owned(), vec!["Run 10".to_owned()]),
        ]
    );
    assert_eq!(
        results.matched, 4,
        "the figure listed twice is still one figure that matched"
    );
    assert_eq!(
        titles(&results, &cards).len(),
        5,
        "the order shown is longer than the number matched, because one figure is shown twice"
    );
}

// Why: a collection is grouped by a parameter that some of its figures do not carry, which is the
// normal case. Those figures must be gathered under a heading that says so rather than dropped
// from the result or scattered through it, and that heading comes last because the reader chose
// the grouping to look at the figures the parameter applies to.
#[test]
fn figures_the_grouping_does_not_apply_to_are_gathered_last_under_not_set() {
    let cards = collection();
    let browse = Browse {
        group: Some(FacetKey::parameter("solver")),
        ..Browse::default()
    };
    let results = browse.results(&cards);
    assert_eq!(
        groups(&results, &cards),
        [
            ("LES".to_owned(), vec!["Run 2".to_owned()]),
            ("SST".to_owned(), vec!["Run 1".to_owned()]),
            (
                NOT_SET.to_owned(),
                vec!["Run 9".to_owned(), "Run 10".to_owned()]
            ),
        ],
        "the two figures without a solver keep the sorted order inside their own group"
    );
    assert_eq!(results.matched, 4);
    assert_eq!(results.total, 4);
}

// Why: a reader who groups a campaign by its run number is walking through it in order, and
// ordering the headings by their characters would read 10, 11, 9 and put the ninth run after the
// eleventh. The headings must read in the same order as the figures inside them, and the figures
// the grouping does not apply to must stay last however their heading would sort.
#[test]
fn groups_are_ordered_as_the_figures_inside_them_are() {
    let numbered = [
        card("Run 10", &[], &[("run", Parameter::Integer(10))]),
        card("Run 9", &[], &[("run", Parameter::Integer(9))]),
        card("Run 11", &[], &[("run", Parameter::Integer(11))]),
        card("Sweep", &[], &[]),
    ];
    let by_run = Browse {
        group: Some(FacetKey::parameter("run")),
        ..Browse::default()
    };
    let names: Vec<String> = groups(&by_run.results(&numbered), &numbered)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        names,
        ["9", "10", "11", NOT_SET],
        "nine comes before ten in the headings as it does in the titles"
    );

    let lettered = [
        card("A", &[], &[("stage", string("zeta"))]),
        card("B", &[], &[]),
    ];
    let by_stage = Browse {
        group: Some(FacetKey::parameter("stage")),
        ..Browse::default()
    };
    let names: Vec<String> = groups(&by_stage.results(&lettered), &lettered)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        names,
        ["zeta", NOT_SET],
        "the figures the grouping does not apply to are last although their heading would sort \
         before the one that is named"
    );
}

// Why: a group is named by the value its figures share as the interface writes it, so that the
// heading of a group reads the same as the value in the sidebar and the datatips. A group headed
// "true" under a facet whose value reads "yes" would be the same collection described two ways.
#[test]
fn a_group_is_named_by_the_value_as_the_interface_writes_it() {
    let cards = vec![
        card("A", &[], &[("stalled", Parameter::Bool(true))]),
        card("B", &[], &[("stalled", Parameter::Bool(false))]),
    ];
    let browse = Browse {
        group: Some(FacetKey::parameter("stalled")),
        ..Browse::default()
    };
    let names: Vec<String> = groups(&browse.results(&cards), &cards)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["no", "yes"]);
}

// Why: the counts under the list are how the reader knows whether their refinements have gone too
// far, so the number matched must be the number of figures kept and the total must be the size of
// the collection rather than the size of anything the refinements produced.
#[test]
fn the_result_reports_how_many_figures_matched_out_of_the_collection() {
    let cards = collection();
    let mut browse = Browse::default();
    browse.toggle(&FacetKey::parameter("rig"), &text("CFD"));
    let results = browse.results(&cards);
    assert_eq!(results.matched, 2);
    assert_eq!(results.total, 4);
    assert!(!results.is_empty());
    assert_eq!(titles(&results, &cards), ["Run 1", "Run 2"]);

    browse.query = "nothing at all".to_owned();
    let empty = browse.results(&cards);
    assert!(empty.is_empty(), "nothing matched");
    assert_eq!(empty.matched, 0);
    assert_eq!(
        empty.total, 4,
        "the collection is still the size it was, which is what tells the reader to widen"
    );
    assert_eq!(titles(&empty, &cards), Vec::<String>::new());
}

// Why: an ungrouped result is still a result, and the interface reads it the same way whether it
// is grouped or not. One unnamed group holding everything is what lets it do that without asking
// which case it is in.
#[test]
fn an_ungrouped_result_is_one_group_with_no_name() {
    let cards = collection();
    let results = Browse::default().results(&cards);
    assert_eq!(results.groups.len(), 1);
    assert_eq!(results.groups[0].name, None);
    assert_eq!(results.groups[0].members.len(), 4);
}

// Why: the query and the refinements are two ways of asking the same kind of question, and both
// must hold. A reader who has ticked a rig and then typed a word means both, and a result that
// honoured only one of them would show figures they had already ruled out.
#[test]
fn what_was_typed_and_what_was_ticked_both_narrow_the_result() {
    let cards = collection();
    let mut browse = Browse::default();
    browse.toggle(&FacetKey::Labels, &text("piv"));
    browse.query = "angle>=8".to_owned();
    assert_eq!(
        titles(&browse.results(&cards), &cards),
        ["Run 2"],
        "only the figure that is labelled piv and has an angle of at least eight"
    );

    browse.clear();
    assert!(
        browse.is_unfiltered(),
        "clearing takes back what was typed as well as what was ticked"
    );
    assert_eq!(browse.results(&cards).matched, 4);
}

// ---------------------------------------------------------------------------------
// The labels a figure is browsed by
// ---------------------------------------------------------------------------------

// Why: neither an empty label nor a repeated one can reach a figure through IronLAB — both are validation errors,
// and the JSON Schema refuses them outright. A figure written by another program can still carry them, because the
// Protocol Buffers schema cannot express either rule, and a blank entry in a menu or the same word offered twice
// is a fault in the interface whatever the file says. The figure itself is left alone, so that opening a file and
// saving it again does not quietly change it.
#[test]
fn an_empty_or_repeated_label_is_cleaned_up_for_browsing_without_altering_the_figure() {
    let mut source = figure(vec![axes(2, vec![Artist::Line(Line::default())])], &[]);
    source.labels = vec![
        "wake".to_owned(),
        String::new(),
        "wake".to_owned(),
        "piv".to_owned(),
    ];

    assert_eq!(
        browsable_labels(&source),
        ["wake", "piv"],
        "the empty label is dropped and the repeat is kept once"
    );
    assert_eq!(
        source.labels.len(),
        4,
        "and the figure still holds exactly what it was given"
    );
    assert!(
        !source.validate().is_valid(),
        "which validation reports, so the reader is told rather than left to wonder"
    );
}

// Why: a card is what the rest of the browser works from, and it must carry what the figure carries and nothing
// besides. A viewer that added facets of its own would be deciding what a collection can be narrowed by, which is
// the author's decision to make: they are the only one who knows which of a figure's properties matter.
#[test]
fn a_card_carries_the_figures_own_labels_and_nothing_else() {
    let mut source = figure(vec![axes(2, vec![Artist::Line(Line::default())])], &[]);
    source.labels = vec!["basics".to_owned()];
    let card = FigureCard::of("Lines", &source);
    assert_eq!(
        card.labels,
        ["basics"],
        "the card carries the figure's labels and nothing the viewer invented"
    );
    assert!(
        card.haystack().contains("basics"),
        "and a search for the author's word finds the figure"
    );
}

// Why: a collection nobody has described is not a fault and not an empty collection — the figures are all there and
// can be searched by title. It is the one state the interface has to explain rather than simply render, so telling
// it apart from a collection that merely has few descriptions is worth pinning: a single parameter on a single
// figure is something to browse by, and the explanation would be false.
#[test]
fn a_collection_is_without_anything_to_browse_by_only_when_nothing_is_described() {
    let bare = [card("One", &[], &[]), card("Two", &[], &[])];
    assert!(has_nothing_to_browse_by(&bare));
    assert!(
        has_nothing_to_browse_by(&[]),
        "an empty collection has nothing to browse by either, vacuously"
    );

    let labelled = [card("One", &["wake"], &[]), card("Two", &[], &[])];
    assert!(
        !has_nothing_to_browse_by(&labelled),
        "one label on one figure is something to browse by"
    );

    let measured = [
        card("One", &[], &[("rig", string("CFD"))]),
        card("Two", &[], &[]),
    ];
    assert!(
        !has_nothing_to_browse_by(&measured),
        "and so is one parameter on one figure"
    );
}
