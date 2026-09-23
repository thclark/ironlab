//! Tests that the gallery is browsable.
//!
//! The gallery is the collection the figure browser was built for: two dozen figures that differ in what they draw,
//! opened together in one window, where finding the surface figures used to mean reading every tab. The viewer
//! works nothing out for itself, because the writer of a figure is the one who knows what matters about it, so
//! everything the gallery can be browsed by is a label an entry wrote: the structural words for what it draws and
//! how it is laid out, and the words naming the features of IronLAB it demonstrates.
//!
//! That makes these tests more valuable than they were, not less. The answers they check are no longer produced by
//! the same code that reads them: what a figure draws is read straight from the IR and compared with what the
//! entry claims, so a label that is missing, wrong or left behind by an edit is caught here.

use std::collections::{BTreeMap, BTreeSet};

use ironlab_gallery::all;
use ironlab_ir::{Artist, Figure, Projection, Scale};
use ironlab_viewer::browse::{
    Browse, FacetKey, FacetValue, FigureCard, Query, describe_facets, parameter_names,
};

/// Every gallery figure as the browser sees it.
fn cards() -> Vec<FigureCard> {
    all()
        .into_iter()
        .map(|entry| {
            let (figure, _warnings) = entry.build_validated().unwrap_or_else(|error| {
                panic!("the gallery entry {:?} is invalid: {error}", entry.slug)
            });
            FigureCard::of(entry.title, &figure.into_ir())
        })
        .collect()
}

/// The labels each gallery entry writes for itself, under the title of the entry.
///
/// These are the entries' own words, taken from the figure before the browser adds anything to it, so a test of
/// them is a test of what the gallery says rather than of what the viewer worked out.
fn authored() -> Vec<(&'static str, Vec<String>)> {
    all()
        .into_iter()
        .map(|entry| {
            let (figure, _warnings) = entry.build_validated().unwrap_or_else(|error| {
                panic!("the gallery entry {:?} is invalid: {error}", entry.slug)
            });
            (entry.title, figure.labels().to_vec())
        })
        .collect()
}

/// The titles a query leaves, in the order the browser lists them.
fn found(query: &str) -> Vec<String> {
    let cards = cards();
    let browse = Browse {
        query: query.to_owned(),
        ..Browse::default()
    };
    let results = browse.results(&cards);
    results
        .members()
        .map(|index| cards[index].title.clone())
        .collect()
}

/// The titles of the gallery entries whose figures draw at least one artist that `wanted` accepts.
///
/// It is read straight from the IR, so it is an answer arrived at independently of the browser rather than a
/// restatement of what the browser did.
fn drawing(wanted: fn(&Artist) -> bool) -> Vec<String> {
    let mut titles: Vec<String> = all()
        .into_iter()
        .filter_map(|entry| {
            let (figure, _) = entry.build_validated().expect("a valid gallery entry");
            figure
                .into_ir()
                .axes
                .iter()
                .flat_map(|axes| &axes.artists)
                .any(wanted)
                .then(|| entry.title.to_owned())
        })
        .collect();
    titles.sort();
    titles
}

/// The structural labels a figure ought to carry, read from the IR: whether it is flat or solid, what kinds of
/// artist it draws, whether it is tiled, whether any of its axes shows a legend and whether any of its axes is
/// logarithmic.
///
/// The viewer no longer works these out, so they are the entry's own words; reading them back off the figure here
/// is what keeps those words true. The three kinds of raster all count as an image, because how a pixel gets its
/// colour is a distinction inside the IR rather than one anyone browses by.
fn structural(figure: &Figure) -> BTreeSet<String> {
    let mut labels = BTreeSet::new();
    let solid = figure
        .axes
        .iter()
        .any(|axes| matches!(axes.projection, Projection::ThreeD { .. }));
    labels.insert(if solid { "3d" } else { "2d" }.to_owned());
    for axes in &figure.axes {
        for artist in &axes.artists {
            labels.insert(
                match artist {
                    Artist::Line(_) => "line",
                    Artist::Scatter(_) => "scatter",
                    Artist::Contour(_) => "contour",
                    Artist::Quiver(_) => "quiver",
                    Artist::Surface(_) => "surface",
                    Artist::Image(_) | Artist::IndexedImage(_) | Artist::MappedImage(_) => "image",
                }
                .to_owned(),
            );
        }
    }
    if figure.axes.len() > 1 {
        labels.insert("subplots".to_owned());
    }
    if figure.axes.iter().any(|axes| axes.legend.is_some()) {
        labels.insert("legend".to_owned());
    }
    if figure.axes.iter().any(|axes| {
        [&axes.x, &axes.y, &axes.z]
            .iter()
            .any(|axis| axis.scale == Scale::Log)
    }) {
        labels.insert("log".to_owned());
    }
    labels
}

// Why: this is the request the browser was built for, in the words it was asked in — "surface shows us all the
// surface figures". The answer is checked against the figures themselves, because a figure counts as a surface
// figure by drawing a surface, not by saying so in its title: the flow past a cylinder draws one, and a reader
// asking for surfaces should be shown it. Nothing reconciles the label with the drawing any more, so this test is
// where an entry that grows a surface and forgets to say so is caught.
#[test]
fn asking_for_the_surface_figures_finds_exactly_the_figures_that_draw_one() {
    let mut found = found("label:surface");
    found.sort();
    let expected = drawing(|artist| matches!(artist, Artist::Surface(_)));

    assert!(
        expected.len() >= 4,
        "the gallery draws several surfaces: {expected:?}"
    );
    assert_eq!(
        found, expected,
        "asking for surfaces finds every figure that draws one, and only those"
    );
}

// Why: the three kinds of raster differ only in how a pixel gets its colour, which is a distinction inside the IR
// rather than one anyone browses by. A reader looking for the pictures should find all of them with one word, so
// every entry that draws any of the three writes the same word.
#[test]
fn the_three_kinds_of_raster_are_all_found_by_asking_for_images() {
    let mut found = found("label:image");
    found.sort();
    let expected = drawing(|artist| {
        matches!(
            artist,
            Artist::Image(_) | Artist::IndexedImage(_) | Artist::MappedImage(_)
        )
    });
    assert_eq!(
        found, expected,
        "one word finds the true-colour, the indexed and the mapped rasters alike"
    );
}

// Why: the other half of the same request. Whether a figure is three-dimensional is the first thing anyone divides
// a gallery by, and it is a property of the figure rather than an opinion about it, so the two labels have to
// partition the gallery exactly: every entry solid or flat, none both and none neither, and each one agreeing with
// the projection of its axes.
#[test]
fn asking_for_the_three_dimensional_figures_finds_every_one_of_them() {
    let solid = found("label:3d");
    let flat = found("label:2d");
    let expected = drawing_in_three_dimensions();

    assert!(expected.len() >= 6, "the gallery has several: {expected:?}");
    let mut sorted = solid.clone();
    sorted.sort();
    assert_eq!(
        sorted, expected,
        "the entries labelled 3d are exactly the ones whose axes are three-dimensional"
    );
    assert_eq!(
        solid.len() + flat.len(),
        all().len(),
        "and every figure is one or the other, never both and never neither"
    );
}

/// The titles of the gallery entries with a three-dimensional axes, read straight from the IR.
fn drawing_in_three_dimensions() -> Vec<String> {
    let mut titles: Vec<String> = all()
        .into_iter()
        .filter_map(|entry| {
            let (figure, _) = entry.build_validated().expect("a valid gallery entry");
            figure
                .into_ir()
                .axes
                .iter()
                .any(|axes| matches!(axes.projection, Projection::ThreeD { .. }))
                .then(|| entry.title.to_owned())
        })
        .collect();
    titles.sort();
    titles
}

// Why: a reader who does not know the typed form types a word. It has to find what the word means, whether the word
// is in the title or in one of the labels the entry wrote.
#[test]
fn a_bare_word_searches_the_titles_and_the_labels_together() {
    let titles = found("image");
    assert!(
        titles.len() >= 4,
        "the four image entries, and the figures that draw one: {titles:?}"
    );
    assert!(
        titles
            .iter()
            .any(|title| title.to_lowercase().contains("surface")),
        "including the entry that draws an image and a surface together, whose title says surface \
         but which is labelled image because it draws one: {titles:?}"
    );
}

// Why: the panel offers what the collection gives it, and the gallery gives it labels and nothing else: no entry
// carries a parameter, and the viewer no longer invents any. If the labels came to nothing the panel would open on
// an empty menu and the reader would conclude the gallery cannot be filtered at all, which is the whole feature
// lost. The facet also has to be worth offering rather than merely present, which is what its score decides.
#[test]
fn the_gallery_offers_facets_worth_filtering_on() {
    let cards = cards();
    let facets = describe_facets(&cards);

    assert_eq!(
        facets
            .iter()
            .map(|facet| facet.key.clone())
            .collect::<Vec<FacetKey>>(),
        vec![FacetKey::Labels],
        "the labels are the whole of what the gallery offers, because they are all it wrote"
    );
    let labels = &facets[0];
    assert!(
        labels.cardinality() > 10 && labels.score > 0.0,
        "the labels divide the gallery many ways and are offered: {} values, score {}",
        labels.cardinality(),
        labels.score
    );
    assert_eq!(
        labels.present,
        cards.len(),
        "and every figure is in the menu, because every figure labelled itself"
    );
    assert!(
        parameter_names(&cards).is_empty(),
        "the gallery writes no parameters, and none is invented for it: {:?}",
        parameter_names(&cards)
    );
}

// Why: the labels are the menu the reader sees first, so they have to be the words they would look for. Two kinds
// of word have to be there. The structural ones say what a figure is made of, which is how a reader who knows the
// picture they want finds it; the feature ones say what the entry demonstrates, which is how a reader who knows
// the problem they have finds the entry that solves it. A gallery with only the first would be no better than the
// tab strip it replaces, and a gallery with only the second would answer "show me the surfaces" with nothing.
#[test]
fn the_labels_of_the_gallery_are_the_words_a_reader_would_look_for() {
    let cards = cards();
    let facets = describe_facets(&cards);
    let labels = facets
        .iter()
        .find(|facet| facet.key == FacetKey::Labels)
        .expect("the gallery has labels");
    let values: Vec<String> = labels.values.iter().map(FacetValue::text).collect();

    for wanted in [
        "2d", "3d", "contour", "image", "line", "quiver", "scatter", "surface", "subplots",
        "legend", "log",
    ] {
        assert!(
            values.iter().any(|value| value == wanted),
            "{wanted:?} says what a figure is made of and is one of the words the gallery can be \
             browsed by: {values:?}"
        );
    }
    for wanted in [
        "basics",
        "colormap",
        "decimation",
        "depth",
        "export",
        "interaction",
        "latex",
        "linked-axes",
        "markers",
        "placement",
        "transparency",
    ] {
        assert!(
            values.iter().any(|value| value == wanted),
            "{wanted:?} names something IronLAB does and is one of the words the gallery can be \
             browsed by: {values:?}"
        );
    }
}

// Why: the counts are what make the menu worth opening rather than guessing, and they have to come from the whole
// gallery rather than from whatever the panel has drawn.
#[test]
fn the_menu_counts_every_figure_behind_a_label() {
    let cards = cards();
    let facets = describe_facets(&cards);
    let labels = facets
        .iter()
        .find(|facet| facet.key == FacetKey::Labels)
        .expect("the gallery has labels");
    let counts = Browse::default().counts(&cards, labels);

    let solid = counts
        .get(&FacetValue::Text("3d".to_owned()))
        .copied()
        .unwrap_or_default();
    assert_eq!(
        solid,
        found("label:3d").len(),
        "the count beside 3d is how many figures asking for 3d would leave"
    );
    assert!(
        counts.values().sum::<usize>() > all().len(),
        "and a figure is counted under each of its labels, so the counts total more than the gallery holds"
    );
}

// Why: a figure title is text the user wrote, and one of the gallery's titles could as easily have held a colon as
// not. A word with a colon that names no parameter must search, not ask about a parameter that does not exist and
// find nothing.
#[test]
fn a_word_with_a_colon_searches_rather_than_asking_about_a_parameter() {
    let cards = cards();
    let names = parameter_names(&cards);
    let query = Query::parse("subplots:linked", &names);
    assert!(
        query
            .terms
            .iter()
            .all(|term| matches!(term, ironlab_viewer::browse::Term::Anywhere { .. })),
        "no gallery figure has a parameter named subplots, so the word is searched for"
    );
}

// Why: a figure reachable only by what it draws is reachable only by the part of the gallery a reader is least
// likely to know in advance. Every entry names at least one feature it demonstrates, so that every entry is
// findable by what it is for rather than by what artists happen to be in it.
#[test]
fn every_entry_carries_a_label_of_its_own() {
    for (title, labels) in authored() {
        assert!(
            !labels.is_empty(),
            "the gallery entry {title:?} carries no label of its own"
        );
    }
}

// Why: the menu of labels, with the count behind each, is how a reader learns what the gallery can be filtered on.
// A label carried by one figure is therefore not a label that divides nothing: it is the line in the menu that
// tells the reader decimation is demonstrated at all, and the 1 beside it says exactly how much of it there is.
// Requiring a label to gather two figures would delete that line, and with it the only trace of a feature the
// gallery shows once.
#[test]
fn a_label_carried_by_a_single_entry_is_still_offered_with_its_count() {
    let mut carriers: BTreeMap<String, usize> = BTreeMap::new();
    for (_title, labels) in authored() {
        for label in labels {
            *carriers.entry(label).or_default() += 1;
        }
    }
    let alone: Vec<&String> = carriers
        .iter()
        .filter(|(_, count)| **count == 1)
        .map(|(label, _)| label)
        .collect();
    assert!(
        alone.contains(&&"decimation".to_owned()),
        "decimation is demonstrated by one entry and labelled all the same: {alone:?}"
    );

    let cards = cards();
    let facets = describe_facets(&cards);
    let labels = facets
        .iter()
        .find(|facet| facet.key == FacetKey::Labels)
        .expect("the gallery has labels");
    let counts = Browse::default().counts(&cards, labels);
    for label in alone {
        let value = FacetValue::Text(label.clone());
        assert!(
            labels.values.contains(&value),
            "{label:?} is offered in the menu like any other label: {:?}",
            labels.values
        );
        assert_eq!(
            counts.get(&value).copied(),
            Some(1),
            "{label:?} is offered with the one figure that carries it behind it"
        );
    }
}

// Why: nothing reconciles a label with the figure it describes any more, so a label is only as true as the entry
// that wrote it. An entry that gains an artist, a tile, a legend or a logarithmic axis and does not say so
// disappears from the query that should find it, and one that keeps a word it no longer earns answers a query with
// a figure that does not belong. Both faults are invisible in the source and in the rendered gallery alike, which
// is why they are checked here against the figure itself.
#[test]
fn the_structural_labels_say_what_each_figure_actually_holds() {
    for entry in all() {
        let (figure, _warnings) = entry.build_validated().expect("a valid gallery entry");
        let labels = figure.labels().to_vec();
        let ir = figure.into_ir();
        let expected = structural(&ir);
        let written: BTreeSet<String> = labels
            .iter()
            .filter(|label| STRUCTURAL.contains(&label.as_str()))
            .cloned()
            .collect();
        assert_eq!(
            written, expected,
            "the gallery entry {:?} draws {expected:?} and says {written:?}",
            entry.slug
        );
    }
}

/// Every word that says what a figure is made of rather than what it demonstrates.
///
/// The list is written out because the test has to tell a structural word an entry left out from a feature word it
/// never claimed: without it, an entry that forgot "legend" would look the same as one that simply has no legend.
const STRUCTURAL: &[&str] = &[
    "2d", "3d", "contour", "image", "legend", "line", "log", "quiver", "scatter", "subplots",
    "surface",
];

// Why: the gallery is the code a reader copies, so an entry of it must be a figure IronLAB would accept. Labels are
// the newest thing the entries carry and the easiest to get wrong, because an empty label and a label written twice
// are both invisible in the source until the figure is validated.
#[test]
fn every_gallery_figure_is_valid_and_its_labels_are_neither_empty_nor_repeated() {
    for entry in all() {
        let figure = (entry.build)();
        let validation = figure.validate();
        assert!(
            validation.is_valid(),
            "the gallery entry {:?} is invalid: {:?}",
            entry.slug,
            validation
                .errors
                .iter()
                .map(|issue| issue.message.as_str())
                .collect::<Vec<&str>>()
        );

        // The same two rules again, read straight off the figure, so that the test says what it means rather than
        // deferring to whatever the validator currently checks.
        let labels = figure.labels();
        assert!(
            labels.iter().all(|label| !label.is_empty()),
            "the gallery entry {:?} carries an empty label: {labels:?}",
            entry.slug
        );
        let distinct: BTreeSet<&String> = labels.iter().collect();
        assert_eq!(
            distinct.len(),
            labels.len(),
            "the gallery entry {:?} carries a label twice: {labels:?}",
            entry.slug
        );
    }
}
