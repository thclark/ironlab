//! Tests of the figure browser: the left-hand panel that narrows a collection of figures down to the one shown.
//!
//! What the browser *works out* — which parameters are worth filtering on, what a query means, what a filter keeps
//! — is tested without a window in `browse.rs`. These tests are about the panel: that it is there when there is a
//! collection to browse and not when there is one figure, that choosing a figure shows it, that what the controls
//! do reaches the list, and that a list of hundreds of figures costs no more to draw than a list of ten.

mod common;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ironlab_ir::{Figure, Parameter};
use ironlab_viewer::ViewerApp;
use ironlab_viewer::browse::{FacetKey, FacetValue, SortKey};

use common::{TEXT, axes_2d, axes_3d, figure_with};

/// The window the browser is tested in: wide enough for the panel beside a figure, and tall enough for a dozen rows
/// of the list.
const WINDOW: egui::Vec2 = egui::vec2(1100.0, 700.0);

/// An application showing `figures`, drawn once so that its panels have been laid out.
fn app(figures: Vec<(String, Figure)>) -> Harness<'static, ViewerApp> {
    let mut harness = Harness::builder()
        .with_size(WINDOW)
        .build_eframe(|_cc| ViewerApp::new(figures, TEXT.clone()));
    harness.run();
    harness
}

/// A flat figure carrying the given parameters, listed under `title`.
fn flat(title: &str, parameters: &[(&str, Parameter)]) -> (String, Figure) {
    let mut figure = figure_with(vec![axes_2d(2)], Vec::new());
    for (name, value) in parameters {
        figure.parameters.insert((*name).to_owned(), value.clone());
    }
    (title.to_owned(), figure)
}

/// A three-dimensional figure carrying the given parameters, listed under `title`.
fn solid(title: &str, parameters: &[(&str, Parameter)]) -> (String, Figure) {
    let mut figure = figure_with(vec![axes_3d(2)], Vec::new());
    for (name, value) in parameters {
        figure.parameters.insert((*name).to_owned(), value.clone());
    }
    (title.to_owned(), figure)
}

/// A small campaign: four measured figures and two computed ones, of which only the computed ones carry a solver.
/// It is the shape every real collection has — a parameter that applies to some of the figures and not the rest.
fn campaign() -> Vec<(String, Figure)> {
    let text = |value: &str| Parameter::String(value.to_owned());
    vec![
        flat(
            "Run 9 lift",
            &[("rig", text("Tunnel A")), ("angle", Parameter::Number(4.0))],
        ),
        flat(
            "Run 10 lift",
            &[
                ("rig", text("Tunnel A")),
                ("angle", Parameter::Number(12.0)),
            ],
        ),
        flat(
            "Run 11 wake",
            &[("rig", text("Tunnel B")), ("angle", Parameter::Number(4.0))],
        ),
        flat(
            "Run 12 wake",
            &[
                ("rig", text("Tunnel B")),
                ("angle", Parameter::Number(12.0)),
            ],
        ),
        solid(
            "Case A surface",
            &[
                ("rig", text("CFD")),
                ("angle", Parameter::Number(4.0)),
                ("solver", text("k-omega SST")),
            ],
        ),
        solid(
            "Case B surface",
            &[
                ("rig", text("CFD")),
                ("angle", Parameter::Number(12.0)),
                ("solver", text("LES")),
            ],
        ),
    ]
}

/// The rectangle egui gave the browser's panel in the last frame, or `None` when it drew none.
fn panel_rect(harness: &Harness<'_, ViewerApp>) -> Option<egui::Rect> {
    egui::PanelState::load(
        &harness.ctx,
        egui::Id::new(ironlab_viewer::sidebar::PANEL_ID),
    )
    .map(|state| state.outer_rect)
}

/// Whether a figure of the given title is drawn as a row of the list.
///
/// A row's label is its title, a comma, and then its second line, which is what tells a row from the tab of the
/// same figure: a tab is labelled by the title alone.
fn listed(harness: &Harness<'_, ViewerApp>, title: &str) -> bool {
    harness.query_by_label_contains(&row_of(title)).is_some()
}

/// The text that finds the row of a figure in the list, and nothing else.
fn row_of(title: &str) -> String {
    format!("{title}, ")
}

/// Types `query` into the search field.
///
/// The field is found by its role: with the property editor closed and the filter menu shut, the browser's search
/// field is the only text input the application draws.
fn search(harness: &mut Harness<'_, ViewerApp>, query: &str) {
    harness
        .get_by_role(egui::accesskit::Role::TextInput)
        .focus();
    harness.run();
    harness
        .get_by_role(egui::accesskit::Role::TextInput)
        .type_text(query);
    harness.run();
}

// ---------------------------------------------------------------------------------
// Whether the panel is there at all
// ---------------------------------------------------------------------------------

// Why: the browser takes room from the figure, which is what the viewer is for. It has to earn that room, and it
// earns it only when there is a choice to make: one figure is not a collection, and the viewer must open on it
// exactly as it did before the browser existed.
#[test]
fn the_browser_opens_with_a_collection_and_stays_shut_for_one_figure() {
    let many = app(campaign());
    assert!(
        panel_rect(&many).is_some(),
        "a collection of six figures opens the browser"
    );

    let one = app(vec![flat("Only", &[])]);
    assert!(
        panel_rect(&one).is_none(),
        "a single figure draws no browser"
    );
    assert!(
        one.query_by_label("Figures").is_none(),
        "and offers no button to open one, because there is nothing to browse"
    );
}

// Why: room taken from the figure must be givable back. The toolbar button is the only way to do that which is
// visible without knowing the panel can be dragged, so it is the one that must work.
#[test]
fn the_figures_button_shows_and_hides_the_browser() {
    let mut harness = app(campaign());
    assert!(listed(&harness, "Run 9 lift"), "it starts open");

    harness.get_by_label("Figures").click();
    harness.run();
    assert!(
        !listed(&harness, "Run 9 lift"),
        "clicking it takes the panel, and the list with it, off the screen"
    );
    assert!(
        !harness.state().browser().open,
        "and the browser records that it is shut"
    );

    harness.get_by_label("Figures").click();
    harness.run();
    assert!(
        listed(&harness, "Run 9 lift"),
        "clicking it again brings the panel back"
    );
}

// Why: the panel is drawn before the tabs so that it takes its room from the window rather than overlapping the
// figure. A panel that overlapped would hide the very thing the reader chose.
#[test]
fn the_panel_takes_its_room_from_the_left_of_the_window() {
    let harness = app(campaign());
    let rect = panel_rect(&harness).expect("the browser is open");
    assert!(
        rect.left() <= 1.0,
        "the panel is against the left edge, not floating over the figure: {rect:?}"
    );
    assert!(
        rect.width() >= ironlab_viewer::sidebar::MIN_WIDTH,
        "and is at least as wide as a title needs: {rect:?}"
    );
    assert!(
        rect.width() < WINDOW.x / 2.0,
        "while leaving most of the window to the figure: {rect:?}"
    );
}

// ---------------------------------------------------------------------------------
// Choosing a figure
// ---------------------------------------------------------------------------------

// Why: the browser exists to replace scrolling a tab strip, so clicking a figure in it must show that figure. This
// is the whole point of the panel, and the only part of it the reader cannot work around.
#[test]
fn clicking_a_figure_in_the_list_shows_it() {
    let mut harness = app(campaign());
    assert_eq!(harness.state().shown(), 0, "the first figure opens");

    harness
        .get_by_label_contains(&row_of("Case B surface"))
        .click();
    harness.run();
    assert_eq!(
        harness.state().shown(),
        5,
        "clicking the last figure of the collection shows it"
    );

    harness
        .get_by_label_contains(&row_of("Run 11 wake"))
        .click();
    harness.run();
    assert_eq!(
        harness.state().shown(),
        2,
        "and clicking another shows that"
    );
}

// Why: a reader who has narrowed the list still needs to know which of the figures left is the one on screen.
// Without that the panel says what could be shown but not what is.
#[test]
fn the_figure_being_shown_is_marked_in_the_list() {
    let mut harness = app(campaign());
    harness
        .get_by_label_contains(&row_of("Run 11 wake"))
        .click();
    harness.run();

    // egui reports a selectable widget's state to the accessibility tree as "toggled".
    assert_eq!(
        harness
            .get_by_label_contains(&row_of("Run 11 wake"))
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True),
        "the row of the figure on screen is the selected one"
    );
    assert_eq!(
        harness
            .get_by_label_contains(&row_of("Run 9 lift"))
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False),
        "and no other row is"
    );
}

// ---------------------------------------------------------------------------------
// Narrowing
// ---------------------------------------------------------------------------------

// Why: typing is the fastest way to narrow a list, and the one that needs no menus to be discovered. What is typed
// has to reach the list on the keystroke, or the reader cannot tell whether it is working.
#[test]
fn typing_narrows_the_list_to_the_figures_that_match() {
    let mut harness = app(campaign());
    assert!(listed(&harness, "Run 9 lift"));
    assert!(listed(&harness, "Case A surface"));

    search(&mut harness, "wake");
    assert!(listed(&harness, "Run 11 wake"), "a match stays");
    assert!(listed(&harness, "Run 12 wake"), "as does the other");
    assert!(
        !listed(&harness, "Run 9 lift"),
        "and what does not match goes"
    );
    assert!(!listed(&harness, "Case A surface"));
}

// Why: a term about a parameter is a question about that parameter, and a figure that does not carry it cannot
// answer. Keeping such figures would make `solver:LES` mean "or has no solver at all", which is not what anyone
// types it for; this is the rule that makes sparse parameters usable.
#[test]
fn a_term_about_a_parameter_leaves_out_the_figures_without_it() {
    let mut harness = app(campaign());
    search(&mut harness, "solver:LES");

    assert!(
        listed(&harness, "Case B surface"),
        "the figure that matches"
    );
    assert!(
        !listed(&harness, "Case A surface"),
        "not the computed figure with another solver"
    );
    assert!(
        !listed(&harness, "Run 9 lift"),
        "and not the measured figures, which carry no solver at all"
    );
}

// Why: the reader needs to know how much of the collection they are looking at, and whether a filter they have
// forgotten is still on. A list with no count is a list you cannot trust.
#[test]
fn the_count_says_how_much_of_the_collection_is_left() {
    let mut harness = app(campaign());
    assert!(
        harness.query_by_label_contains("6 figures").is_some(),
        "with nothing chosen it says how many there are"
    );

    search(&mut harness, "wake");
    assert!(
        harness.query_by_label_contains("2 of 6 figures").is_some(),
        "and once narrowed it says how many of how many"
    );
}

// Why: a reader who has narrowed the list into a corner needs one move that gets them out, and it must take back
// what was typed as well as what was clicked, or the list stays narrowed for a reason they cannot see.
#[test]
fn showing_all_takes_back_everything_that_was_typed_and_chosen() {
    let mut harness = app(campaign());
    search(&mut harness, "wake");
    harness.state_mut().browser_mut().browse.toggle(
        &FacetKey::parameter("rig"),
        &FacetValue::Text("Tunnel B".to_owned()),
    );
    harness.run();
    assert!(!listed(&harness, "Run 9 lift"));

    harness.get_by_label("Show all").click();
    harness.run();
    assert!(listed(&harness, "Run 9 lift"), "every figure is back");
    assert!(
        harness.state().browser().browse.query.is_empty(),
        "and the search field is empty"
    );
    assert!(
        harness.state().browser().browse.filters.is_empty(),
        "and no filter is left"
    );
}

// ---------------------------------------------------------------------------------
// Filters and their chips
// ---------------------------------------------------------------------------------

// Why: the chip is how a filter reads back, and clicking it is how it is taken away. A filter that can be added but
// not seen or removed is a trap, and the panel's whole claim is that what is chosen is visible at a glance.
#[test]
fn a_chip_says_what_is_filtered_and_removes_it_when_clicked() {
    let mut harness = app(campaign());
    harness.state_mut().browser_mut().browse.toggle(
        &FacetKey::parameter("rig"),
        &FacetValue::Text("CFD".to_owned()),
    );
    harness.run();

    assert!(
        !listed(&harness, "Run 9 lift"),
        "the filter narrows the list"
    );
    let chip = harness
        .query_by_label_contains("rig: CFD")
        .expect("the filter reads back as a chip naming the parameter and the value");
    chip.click();
    harness.run();

    assert!(
        listed(&harness, "Run 9 lift"),
        "and clicking the chip takes the filter away"
    );
}

// Why: the menu is the only way to filter without knowing the typed form, so it has to offer the parameters the
// collection actually has, and it must not offer one that divides nothing, which would leave the list unchanged and
// teach the reader that the menu does not work.
#[test]
fn the_filter_menu_offers_the_parameters_that_divide_the_collection() {
    let mut harness = app(campaign());
    harness.get_by_label("Add filter").click();
    harness.run();

    for parameter in ["rig", "angle", "solver"] {
        assert!(
            harness.query_by_label_contains(parameter).is_some(),
            "the menu offers {parameter:?}, which divides the collection"
        );
    }
    assert!(
        harness.query_by_label_contains("axes").is_none(),
        "but not the number of axes, which is one for every figure and divides nothing"
    );
}

// Why: a count beside a value is what makes the menu worth opening rather than guessing, and a count of zero must
// still be shown: a value that vanished as the reader reached for it reads as a fault, where a disabled one says
// plainly that the collection has nothing there.
#[test]
fn the_menu_counts_what_each_value_would_leave() {
    let mut harness = app(campaign());
    harness.state_mut().browser_mut().browse.query = "wake".to_owned();
    harness.run();
    harness.get_by_label("Add filter").click();
    harness.run();
    harness.get_by_label_contains("rig").click();
    harness.run();

    assert!(
        harness.query_by_label_contains("Tunnel B, 2").is_some(),
        "the two wake figures are behind Tunnel B"
    );
    assert!(
        harness.query_by_label_contains("CFD, 0").is_some(),
        "and CFD is still offered, saying that it would leave nothing"
    );
}

// ---------------------------------------------------------------------------------
// Ordering and grouping
// ---------------------------------------------------------------------------------

// Why: a figure's number is a number. Ordering titles by their characters puts "Run 10" before "Run 9", which is
// wrong in the one collection the browser exists for: a campaign, whose figures are numbered.
#[test]
fn figures_numbered_in_their_titles_are_ordered_by_the_number() {
    let harness = app(campaign());
    let rows: Vec<String> = harness
        .state()
        .cards()
        .iter()
        .map(|card| card.title.clone())
        .collect();
    let results = harness
        .state()
        .browser()
        .browse
        .results(harness.state().cards());
    let order: Vec<&str> = results
        .members()
        .map(|index| rows[index].as_str())
        .collect();
    let nine = order.iter().position(|title| *title == "Run 9 lift");
    let ten = order.iter().position(|title| *title == "Run 10 lift");
    assert!(
        nine < ten,
        "Run 9 comes before Run 10, not after it: {order:?}"
    );
}

// Why: grouping is how a collection with a handful of distinct values is read, and the heading has to carry the
// count, because the size of a group is the first thing anyone wants from it.
#[test]
fn grouping_puts_a_counted_heading_above_each_run_of_figures() {
    let mut harness = app(campaign());
    harness.state_mut().browser_mut().browse.group = Some(FacetKey::parameter("rig"));
    harness.run();

    assert!(
        harness
            .query_by_label_contains("Tunnel A, 2 figures")
            .is_some(),
        "the heading names the group and counts it"
    );
    assert!(harness.query_by_label_contains("CFD, 2 figures").is_some());
    assert!(listed(&harness, "Run 9 lift"), "with the figures below it");
}

// Why: a heading that cannot be closed is only a label. Closing a group is how a reader puts aside the part of the
// collection they are not looking at, which is the reason to group in the first place.
#[test]
fn closing_a_group_hides_its_figures_and_leaves_the_others() {
    let mut harness = app(campaign());
    harness.state_mut().browser_mut().browse.group = Some(FacetKey::parameter("rig"));
    harness.run();
    assert!(listed(&harness, "Run 9 lift"));

    harness.get_by_label_contains("Tunnel A, 2 figures").click();
    harness.run();
    assert!(
        !listed(&harness, "Run 9 lift"),
        "the figures of the closed group are gone"
    );
    assert!(
        harness
            .query_by_label_contains("Tunnel A, 2 figures")
            .is_some(),
        "but its heading stays, so it can be opened again"
    );
    assert!(
        listed(&harness, "Run 11 wake"),
        "and the other groups are untouched"
    );
}

// Why: a figure that does not carry the parameter has to go somewhere, and it must be somewhere the reader can
// find. Dropping it would make grouping quietly lose figures, which is worse than showing an awkward group.
#[test]
fn figures_without_the_grouping_parameter_are_gathered_at_the_end() {
    let mut harness = app(campaign());
    harness.state_mut().browser_mut().browse.group = Some(FacetKey::parameter("solver"));
    harness.run();

    assert!(
        harness
            .query_by_label_contains(ironlab_viewer::browse::NOT_SET)
            .is_some(),
        "the four measured figures are grouped under a heading that says the parameter is not set"
    );
    assert!(
        listed(&harness, "Run 9 lift"),
        "and are still listed, rather than dropped"
    );
}

// Why: the second line of a row is the only room for anything beside the title, so it has to say whatever the
// reader is most likely to want. Ordering by a parameter is asking about that parameter, so that is what it shows.
#[test]
fn a_row_shows_the_value_the_list_is_ordered_by() {
    let mut harness = app(campaign());
    harness.state_mut().browser_mut().browse.sort.key = SortKey::Parameter("angle".to_owned());
    harness.run();

    assert!(
        harness
            .query_by_label_contains("Run 9 lift, angle: 4")
            .is_some(),
        "a figure that carries the parameter shows its value beside its title"
    );
}

// ---------------------------------------------------------------------------------
// A long list
// ---------------------------------------------------------------------------------

// Why: the browser is for collections too large to scroll through as tabs, so it must not pay for the figures that
// are not on screen. Laying out every row of a few hundred figures on every frame is exactly the cost the panel was
// built to avoid, and nothing else in the viewer would show it until the list got long.
#[test]
fn the_list_lays_out_only_the_rows_that_are_on_screen() {
    let figures: Vec<(String, Figure)> = (0..300)
        .map(|index| {
            flat(
                &format!("Figure {index:03}"),
                &[("run", Parameter::Integer(i64::from(index)))],
            )
        })
        .collect();
    let harness = app(figures);

    let drawn = (0..300)
        .filter(|index| listed(&harness, &format!("Figure {index:03}")))
        .count();
    assert!(
        drawn > 0,
        "the rows that fit in the panel are drawn: {drawn} of 300"
    );
    assert!(
        drawn < 60,
        "but not the three hundred rows of the whole collection: {drawn} were drawn"
    );
}

// Why: a collection of hundreds is the case the typed form exists for, and narrowing it has to reach the list
// however long the list is. A virtualised list that filtered only what it had already drawn would answer from the
// rows on screen rather than from the collection.
#[test]
fn a_long_list_is_narrowed_from_the_whole_collection_not_the_rows_on_screen() {
    let figures: Vec<(String, Figure)> = (0..300)
        .map(|index| {
            flat(
                &format!("Figure {index:03}"),
                &[("run", Parameter::Integer(i64::from(index)))],
            )
        })
        .collect();
    let mut harness = app(figures);

    search(&mut harness, "run>=295");
    assert!(
        harness
            .query_by_label_contains("5 of 300 figures")
            .is_some(),
        "the last five figures are found, though their rows were far below the panel"
    );
    assert!(listed(&harness, "Figure 299"));
}

// ---------------------------------------------------------------------------------
// What the panel paints
// ---------------------------------------------------------------------------------

/// Every run of text the browser paints, with its filter menu open on a parameter.
///
/// The panel is run through an egui context of its own rather than through the accessibility harness, because what
/// is asked of it here is what reaches the screen: the characters themselves, which the accessibility tree does not
/// carry.
fn painted_words(filtered: bool) -> Vec<String> {
    let ctx = egui::Context::default();
    ironlab_viewer::style::apply(&ctx);
    ctx.set_theme(egui::Theme::Dark);
    let cards: Vec<ironlab_viewer::browse::FigureCard> = campaign()
        .into_iter()
        .map(|(title, figure)| ironlab_viewer::browse::FigureCard::of(title, &figure))
        .collect();
    let mut browser = ironlab_viewer::FigureBrowser::for_collection(cards.len());
    if filtered {
        browser.browse.toggle(
            &FacetKey::parameter("rig"),
            &FacetValue::Text("CFD".to_owned()),
        );
        browser.browse.group = Some(FacetKey::parameter("rig"));
    }
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW)),
        ..egui::RawInput::default()
    };
    let mut words = Vec::new();
    // egui lays a panel out over two passes; the second draws where the first decided, so it is the pass whose
    // text is read.
    for _ in 0..2 {
        let mut output = ctx.run_ui(input.clone(), |ui| {
            ironlab_viewer::figure_browser(ui, &mut browser, &cards, 0);
        });
        output.textures_delta.clear();
        words.clear();
        for clipped in &output.shapes {
            collect_painted_text(&clipped.shape, &mut words);
        }
    }
    words
}

/// Adds every run of text in a shape, and in the shapes it holds, to `words`.
fn collect_painted_text(shape: &egui::Shape, words: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => words.push(text.galley.text().to_owned()),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_painted_text(shape, words);
            }
        }
        _ => {}
    }
}

// Why: the browser is the first part of the interface to draw a mark beside its words, and the last-resort face
// that gives it the arrows also draws four and a half thousand other characters. A character left out of the
// style's list therefore no longer announces itself as an empty box — it simply appears, unchecked, and will be an
// empty box for whoever builds without that face. This test is what the empty box used to be.
#[test]
fn the_browser_paints_no_character_outside_the_listed_ones() {
    let listed = ironlab_viewer::style::INTERFACE_CHARACTERS;
    for filtered in [false, true] {
        for word in painted_words(filtered) {
            for character in word.chars() {
                assert!(
                    character.is_ascii() || listed.contains(&character),
                    "the browser paints {character:?} in {word:?}, which is not in \
                     style::INTERFACE_CHARACTERS and is therefore not checked against the fonts"
                );
            }
        }
    }
}

// Why: a mark that augments words has to actually be on screen beside them, or the decision to add it to the fonts
// bought nothing. Both marks are checked where they are drawn, because each is the only reason its character is in
// the style's list at all.
#[test]
fn a_chip_carries_the_remove_mark_and_the_order_carries_its_arrow() {
    let words = painted_words(true);
    assert!(
        words
            .iter()
            .any(|word| word.contains("rig: CFD") && word.contains(ironlab_viewer::style::REMOVE)),
        "the chip says what it narrows and carries the mark that says clicking it takes that away: {words:?}"
    );
    assert!(
        words
            .iter()
            .any(|word| word == &format!("Ascending {}", ironlab_viewer::style::ASCENDING)),
        "the order says which way it runs and carries the arrow that shows it: {words:?}"
    );
}

// ---------------------------------------------------------------------------------
// A collection with nothing to browse by
// ---------------------------------------------------------------------------------

// Why: the viewer works out no properties of a figure for itself, so a collection whose author has not described it
// has nothing to filter by at all. Offering an empty menu would read as a panel that does not work, when what is
// actually missing is two lines in the program that built the figures. The note is the only way the reader learns
// that, and it has to name the remedy rather than merely state the problem.
#[test]
fn a_collection_with_no_labels_or_parameters_is_told_how_to_describe_itself() {
    let bare: Vec<(String, Figure)> = (0..4)
        .map(|index| flat(&format!("Figure {index}"), &[]))
        .collect();
    let harness = app(bare);

    assert!(
        harness
            .query_by_label_contains("nothing to narrow them by")
            .is_some(),
        "the note says what is missing"
    );
    assert!(
        harness
            .query_by_label_contains("How to describe figures")
            .is_some(),
        "and offers the page that says how to fix it"
    );
    assert!(
        listed(&harness, "Figure 0"),
        "while the figures are still listed, because nothing is wrong with them"
    );
}

// Why: the note is for a collection that cannot be browsed at all. A collection carrying even one description can be
// browsed, so the note would be false there, and it would take room from the list every time it was shown.
#[test]
fn a_collection_that_carries_a_description_is_not_offered_the_note() {
    let harness = app(campaign());
    assert!(
        harness
            .query_by_label_contains("nothing to narrow them by")
            .is_none(),
        "a described collection is left to get on with it"
    );
}

// Why: the note's whole value is the page it points at, and that page is in this repository while the link is an
// address on the published site. Nothing else connects the two, so renaming or moving the page would leave the
// viewer pointing at a page that no longer exists, and nobody would find out until a user clicked it.
#[test]
fn the_page_the_note_links_to_exists_in_the_documentation() {
    let url = ironlab_viewer::sidebar::DESCRIBING_FIGURES_URL;
    let path = url
        .strip_prefix("https://ironlab.org/")
        .unwrap_or_else(|| panic!("{url} is not an address on the documentation site"));
    let page = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs")
        .join(path.trim_end_matches('/'))
        .with_extension("md");
    assert!(
        page.exists(),
        "the note links to {url}, which is built from {}, and that file is not there",
        page.display()
    );
}
