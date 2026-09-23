//! Tests that the gallery is browsable.
//!
//! The gallery is the collection the figure browser was built for: two dozen figures that differ in what they draw,
//! opened together in one window, where finding the surface figures used to mean reading every tab. None of these
//! figures carries a parameter, so what makes them browsable is entirely what the viewer reads off them. These
//! tests are the check that it reads enough.

use ironlab_gallery::all;
use ironlab_ir::Artist;
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

// Why: this is the request the browser was built for, in the words it was asked in — "surface shows us all the
// surface figures". It has to work on the gallery as the gallery is, with nobody having labelled anything, or the
// browser is a promise about figures that do not exist yet. The answer is checked against the figures themselves,
// because a figure counts as a surface figure by drawing a surface, not by saying so in its title: the flow past a
// cylinder draws one, and a reader asking for surfaces should be shown it.
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
// rather than one anyone browses by. A reader looking for the pictures should find all of them with one word.
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

// Why: the other half of the same request. Whether a figure is three-dimensional is the first thing anyone sorts a
// gallery by, and it is knowable from the figure without anyone saying so.
#[test]
fn asking_for_the_three_dimensional_figures_finds_every_one_of_them() {
    let solid = found("label:3d");
    let flat = found("label:2d");
    assert!(solid.len() >= 6, "the gallery has several: {solid:?}");
    assert_eq!(
        solid.len() + flat.len(),
        all().len(),
        "and every figure is one or the other, never both and never neither"
    );
}

// Why: a reader who does not know the typed form types a word. It has to find what the word means, whether the word
// is in the title, in a label the viewer worked out, or in a parameter.
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

// Why: the browser decides for itself which parameters are worth offering, and on the gallery it has only what it
// reads off the figures. If that came to nothing, the panel would open on an empty menu and the reader would
// conclude the gallery cannot be filtered at all.
#[test]
fn the_gallery_offers_facets_worth_filtering_on() {
    let cards = cards();
    let facets = describe_facets(&cards);
    let offered: Vec<&str> = facets
        .iter()
        .filter(|facet| facet.cardinality() > 1 && facet.score > 0.0)
        .map(|facet| facet.key.name())
        .collect();

    assert_eq!(
        facets.first().map(|facet| facet.key.clone()),
        Some(FacetKey::Labels),
        "the labels lead, because they are what the reader browses by"
    );
    for wanted in ["dimensionality", "artists", "data_values", "axes"] {
        assert!(
            offered.contains(&wanted),
            "{wanted:?} divides the gallery and is offered: {offered:?}"
        );
    }

    // The gallery's titles are all different, so a facet of them would name every figure rather than divide the
    // collection. Nothing derived from a figure behaves that way, which is why the ranking has to be measured
    // rather than assumed: the check is that the facets are ordered by how well they divide, not merely present.
    let scores: Vec<f64> = facets.iter().map(|facet| facet.score).collect();
    assert!(
        scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "the facets are offered in descending order of how well they divide the gallery: {scores:?}"
    );
}

// Why: the labels are the menu the reader sees first, so they have to be the words they would look for. A gallery
// labelled with the names of IR types would be no better than the tab strip it replaces.
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
    ] {
        assert!(
            values.iter().any(|value| value == wanted),
            "{wanted:?} is one of the words the gallery can be browsed by: {values:?}"
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
