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

/// The control that opens and shuts the filter menu, which reads "Edit" beside a mark saying which it will do.
fn edit_filters<'a>(harness: &'a Harness<'_, ViewerApp>) -> egui_kittest::Node<'a> {
    harness.get_by_label_contains(" Edit")
}

/// The text that finds the row of a figure in the list, and nothing else.
fn row_of(title: &str) -> String {
    format!("{title}, ")
}

/// The text that finds the entry of a parameter in the filter menu, and nothing else.
///
/// An entry is labelled by the parameter's name, a comma, and then how many values it takes, which is what tells it
/// from the same name in a chip, where a colon follows the name, and from the details strip, where the name stands
/// alone.
fn parameter_of(name: &str) -> String {
    format!("{name}, ")
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

// Why: the browser is the only way to reach a figure other than the first, so it is open whenever there is a
// collection and nothing in the toolbar stands in for it. Shutting it is still possible, because room taken from
// the figure must be givable back, and shutting it must take the list with it.
#[test]
fn the_browser_is_open_for_a_collection_and_can_be_shut() {
    let mut harness = app(campaign());
    assert!(listed(&harness, "Run 9 lift"), "it starts open");
    assert!(
        harness.query_by_label("Figures").is_none(),
        "with no toggle in the toolbar, which the browser makes redundant"
    );

    harness.state_mut().browser_mut().open = false;
    harness.run();
    assert!(
        !listed(&harness, "Run 9 lift"),
        "shutting it takes the panel, and the list with it, off the screen"
    );

    harness.state_mut().browser_mut().open = true;
    harness.run();
    assert!(
        listed(&harness, "Run 9 lift"),
        "and opening it brings the list back"
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
    edit_filters(&harness).click();
    harness.run();

    for parameter in ["rig", "angle", "solver"] {
        assert!(
            harness
                .query_by_label_contains(&parameter_of(parameter))
                .is_some(),
            "the menu offers {parameter:?}, which divides the collection"
        );
    }
    assert!(
        harness.query_by_label_contains("axes").is_none(),
        "but not the number of axes, which is one for every figure and divides nothing"
    );
}

// Why: the design that was approved opens the filters as an accordion within the panel, pushing the order and
// grouping controls and the list down, rather than as a popup floating over them: a popup shuts as soon as the
// pointer strays outside it, and a reader choosing values one at a time loses it again and again. Whether the
// menu is in the panel or over it is a question of geometry, so the geometry is what is checked.
#[test]
fn the_filter_menu_opens_within_the_panel_and_pushes_the_controls_down() {
    let mut harness = app(campaign());
    let order_before = harness.get_by_label_contains("Ascending").rect();
    edit_filters(&harness).click();
    harness.run();

    let panel = panel_rect(&harness).expect("the browser is open");
    let row = harness.get_by_label_contains(&parameter_of("rig")).rect();
    assert!(
        panel.contains_rect(row),
        "the parameters are offered inside the panel ({panel:?}), not in a popup over it: {row:?}"
    );
    let order_after = harness.get_by_label_contains("Ascending").rect();
    assert!(
        order_after.top() > order_before.bottom(),
        "and the order and grouping controls move down below the menu: {order_before:?} then {order_after:?}"
    );
    assert!(
        row.bottom() <= order_after.top(),
        "the menu sits between the search field and the order controls"
    );

    edit_filters(&harness).click();
    harness.run();
    assert!(
        harness
            .query_by_label_contains(&parameter_of("rig"))
            .is_none(),
        "clicking Edit again shuts the menu"
    );
    assert_eq!(
        harness.get_by_label_contains("Ascending").rect(),
        order_before,
        "and the controls return to where they were"
    );
}

// Why: the chips are added and removed while the menu is open, and if they sat above it every tick would move the
// menu under the pointer. They sit beneath it instead, so the menu holds still while filters come and go.
#[test]
fn the_chips_sit_beneath_the_menu_so_that_it_holds_still_as_filters_change() {
    let mut harness = app(campaign());
    edit_filters(&harness).click();
    harness.run();
    harness.get_by_label_contains(&parameter_of("rig")).click();
    harness.run();
    let back_before = harness.get_by_label("Back to all parameters").rect();

    harness.get_by_label_contains("CFD, 2").click();
    harness.run();

    assert_eq!(
        harness.get_by_label("Back to all parameters").rect(),
        back_before,
        "ticking a value does not move the menu"
    );
    let done = harness.get_by_label("Done").rect();
    let chip = harness.get_by_label_contains("rig: CFD").rect();
    assert!(
        chip.top() >= done.bottom(),
        "the chip appears beneath the menu ({done:?}), not above it: {chip:?}"
    );
    let sort = harness.get_by_label("SORT").rect();
    assert!(
        sort.top() >= chip.bottom(),
        "and the order controls stay beneath the chips"
    );
}

// Why: the control that takes every filter away is a control like Edit, not an afterthought beside the chips, so it
// stands beside Edit, is drawn as Edit is and says what it does in full; the revert mark beside its words is the same mark that takes
// back a change in the property editor, so that taking back reads the same way everywhere.
#[test]
fn clear_all_takes_every_filter_away_and_carries_the_revert_mark() {
    let mut harness = app(campaign());
    assert!(
        harness.query_by_label_contains("Clear all").is_none(),
        "there is nothing to clear until a filter is chosen"
    );
    harness.state_mut().browser_mut().browse.toggle(
        &FacetKey::parameter("rig"),
        &FacetValue::Text("CFD".to_owned()),
    );
    harness.run();

    let clear = harness.get_by_label_contains("Clear all");
    assert!(
        clear
            .accesskit_node()
            .label()
            .is_some_and(|label| label.contains(ironlab_viewer::style::RESTORE)),
        "the control carries the revert mark beside its words"
    );
    let edit = edit_filters(&harness).rect();
    let rect = clear.rect();
    assert!(
        (rect.center().y - edit.center().y).abs() < 1.0 && rect.right() <= edit.left(),
        "and stands on the Filters row, beside Edit: {rect:?} and {edit:?}"
    );
    assert!(
        (rect.height() - edit.height()).abs() < 0.5,
        "drawn as Edit is drawn: {rect:?} and {edit:?}"
    );
    clear.click();
    harness.run();

    assert!(
        harness.state().browser().browse.filters.is_empty(),
        "and clicking it takes every filter away"
    );
    assert!(listed(&harness, "Run 9 lift"));
}

// Why: a control that grows by a point when the pointer reaches it jitters, and a row of chips jitters as the
// pointer crosses it. A chip's size is decided by its words, not by whether it is hovered.
#[test]
fn a_chip_keeps_its_size_under_the_pointer() {
    let mut harness = app(campaign());
    harness.state_mut().browser_mut().browse.toggle(
        &FacetKey::parameter("rig"),
        &FacetValue::Text("CFD".to_owned()),
    );
    harness.run();
    let before = harness.get_by_label_contains("rig: CFD").rect();

    harness.get_by_label_contains("rig: CFD").hover();
    harness.run();
    harness.run();

    assert_eq!(
        harness.get_by_label_contains("rig: CFD").rect(),
        before,
        "the chip is the same size with the pointer over it"
    );
}

// Why: the filters, the order and the grouping are three settings of the same list, and each is read the same
// way: a caption at the left of its row and its control at the right. Two settings sharing a row, or a caption
// above its control, would be read differently from the third for no reason.
#[test]
fn filters_sort_and_group_each_have_a_captioned_row_with_the_control_at_the_right() {
    let harness = app(campaign());
    let filters = harness.get_by_label("FILTERS").rect();
    let sort = harness.get_by_label("SORT").rect();
    let group = harness.get_by_label("GROUP").rect();
    assert!(
        filters.bottom() <= sort.top() && sort.bottom() <= group.top(),
        "the three captions come one beneath the other: {filters:?}, {sort:?}, {group:?}"
    );
    assert!(
        (filters.left() - sort.left()).abs() < 1.0 && (sort.left() - group.left()).abs() < 1.0,
        "and start at the same edge"
    );

    let edit = edit_filters(&harness).rect();
    let combos: Vec<egui::Rect> = harness
        .get_all_by_role(egui::accesskit::Role::ComboBox)
        .map(|node| node.rect())
        .collect();
    assert_eq!(
        combos.len(),
        2,
        "the order and the grouping are each a combo box"
    );
    let panel = panel_rect(&harness).expect("the browser is open");
    for (caption, control) in [(filters, edit), (sort, combos[0]), (group, combos[1])] {
        assert!(
            (control.center().y - caption.center().y).abs() < 2.0,
            "the control sits on the row of its caption: {caption:?} and {control:?}"
        );
        assert!(
            control.left() > caption.right(),
            "to the right of it: {caption:?} and {control:?}"
        );
    }
    assert!(
        (edit.right() - combos[1].right()).abs() < 1.0,
        "the controls end at one edge: {edit:?} and {:?}",
        combos[1]
    );
    assert!(
        (combos[0].right() - combos[1].right()).abs() < 1.0,
        "the two combo boxes too: {:?} and {:?}",
        combos[0],
        combos[1]
    );
    assert!(
        panel.right() - combos[1].right() < 40.0,
        "and that edge is the right of the panel, less its padding: {:?} in {panel:?}",
        combos[1]
    );
}

// Why: a menu that stays open until it is told to shut needs a control that shuts it, and the control has to be
// there on the page of values, which is where a reader is when they have finished choosing.
#[test]
fn done_shuts_the_menu_and_keeps_what_was_chosen() {
    let mut harness = app(campaign());
    edit_filters(&harness).click();
    harness.run();
    harness.get_by_label_contains(&parameter_of("rig")).click();
    harness.run();
    harness.get_by_label_contains("CFD, 2").click();
    harness.run();
    harness.get_by_label("Done").click();
    harness.run();

    assert!(harness.query_by_label("Done").is_none(), "the menu is shut");
    assert!(
        harness.query_by_label_contains("rig: CFD").is_some(),
        "and the value chosen in it is kept as a chip"
    );
    assert!(!listed(&harness, "Run 9 lift"), "which narrows the list");
}

// Why: a count beside a value is what makes the menu worth opening rather than guessing, and a count of zero must
// still be shown: a value that vanished as the reader reached for it reads as a fault, where a disabled one says
// plainly that the collection has nothing there.
#[test]
fn the_menu_counts_what_each_value_would_leave() {
    let mut harness = app(campaign());
    harness.state_mut().browser_mut().browse.query = "wake".to_owned();
    harness.run();
    edit_filters(&harness).click();
    harness.run();
    harness.get_by_label_contains(&parameter_of("rig")).click();
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
            .query_by_label_contains("Run 9 lift, angle = 4")
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
    painted_words_with(|browser| {
        if filtered {
            browser.browse.toggle(
                &FacetKey::parameter("rig"),
                &FacetValue::Text("CFD".to_owned()),
            );
            browser.browse.group = Some(FacetKey::parameter("rig"));
        }
    })
}

/// Every run of text the browser paints for the campaign, after `configure` has set it up.
fn painted_words_with(configure: impl FnOnce(&mut ironlab_viewer::FigureBrowser)) -> Vec<String> {
    let ctx = egui::Context::default();
    ironlab_viewer::style::apply(&ctx);
    ctx.set_theme(egui::Theme::Dark);
    let cards: Vec<ironlab_viewer::browse::FigureCard> = campaign()
        .into_iter()
        .map(|(title, figure)| ironlab_viewer::browse::FigureCard::of(title, &figure))
        .collect();
    let mut browser = ironlab_viewer::FigureBrowser::for_collection(cards.len());
    configure(&mut browser);
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

// Why: the control that reverses the order shares its row with a combo box, and a caption a size smaller than the
// text beside it reads as a different kind of control. The two are set at one size, which only what is painted can
// show.
#[test]
fn the_order_control_is_set_at_the_size_of_the_combo_box_beside_it() {
    let ctx = egui::Context::default();
    ironlab_viewer::style::apply(&ctx);
    ctx.set_theme(egui::Theme::Dark);
    let cards: Vec<ironlab_viewer::browse::FigureCard> = campaign()
        .into_iter()
        .map(|(title, figure)| ironlab_viewer::browse::FigureCard::of(title, &figure))
        .collect();
    let mut browser = ironlab_viewer::FigureBrowser::for_collection(cards.len());
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW)),
        ..egui::RawInput::default()
    };
    let mut sizes: std::collections::BTreeMap<String, f32> = std::collections::BTreeMap::new();
    for _ in 0..2 {
        let mut output = ctx.run_ui(input.clone(), |ui| {
            ironlab_viewer::figure_browser(ui, &mut browser, &cards, 0);
        });
        output.textures_delta.clear();
        sizes.clear();
        for clipped in &output.shapes {
            collect_text_sizes(&clipped.shape, &mut sizes);
        }
    }
    let order = sizes
        .get("Ascending ↑")
        .copied()
        .expect("the order control is painted");
    let combo = sizes
        .get("Title")
        .copied()
        .expect("the combo box is painted");
    assert!(
        (order - combo).abs() < 0.01,
        "the order control's words are set at {order} pt and the combo box's at {combo} pt"
    );
}

/// Adds the size of the first word of every run of text in a shape, and in the shapes it holds, to `sizes`.
fn collect_text_sizes(shape: &egui::Shape, sizes: &mut std::collections::BTreeMap<String, f32>) {
    match shape {
        egui::Shape::Text(text) => {
            if let Some(section) = text.galley.job.sections.first() {
                sizes.insert(text.galley.text().to_owned(), section.format.font_id.size);
            }
        }
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text_sizes(shape, sizes);
            }
        }
        _ => {}
    }
}

// Why: a mark that augments words has to actually be on screen beside them, or the decision to add it to the fonts
// bought nothing. Both marks are checked where they are drawn, because each is the only reason its character is in
// the style's list at all.
#[test]
fn a_chip_carries_the_remove_mark_and_the_order_carries_its_arrow() {
    let words = painted_words(true);
    assert!(
        words.iter().any(|word| word.contains("rig")
            && word.contains("CFD")
            && word.contains(ironlab_viewer::style::REMOVE)),
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

// ---------------------------------------------------------------------------------
// A parameter holding numbers
// ---------------------------------------------------------------------------------

// Why: a parameter holding numbers is narrowed by the two ends of a range rather than by picking from a list, and
// that is a different branch of the menu reached only when the values are numeric. Nothing else in the interface
// exercises it, so without this the first numeric parameter anyone writes would be the test.
#[test]
fn a_numeric_parameter_is_narrowed_by_a_range_rather_than_a_list_of_values() {
    let mut harness = app(campaign());
    edit_filters(&harness).click();
    harness.run();
    harness
        .get_by_label_contains(&parameter_of("angle"))
        .click();
    harness.run();

    assert_eq!(
        harness
            .get_all_by_role(egui::accesskit::Role::SpinButton)
            .count(),
        2,
        "the two ends of the range are offered, as number fields"
    );
    assert!(
        harness.query_by_label_contains("4, 3").is_none(),
        "and not the counted checklist a text parameter would get"
    );

    // Narrowing to the upper end keeps the figures at twelve degrees and drops those at four.
    harness
        .state_mut()
        .browser_mut()
        .browse
        .filters
        .push(ironlab_viewer::browse::Filter {
            key: FacetKey::parameter("angle"),
            constraint: ironlab_viewer::browse::Constraint::Between {
                low: 8.0,
                high: 12.0,
            },
        });
    harness.run();
    assert!(listed(&harness, "Run 10 lift"), "twelve degrees stays");
    assert!(!listed(&harness, "Run 9 lift"), "four degrees goes");
    assert!(
        harness.query_by_label_contains("angle: 8 to 12").is_some(),
        "and the chip says the range it was narrowed to"
    );
}

// Why: a range covering the whole parameter keeps every figure, so leaving it on the list would show a chip that
// narrows nothing and invite the reader to wonder what it is doing. There is no control that says "whole range":
// the ends of the range are the control, and dragging the lower end back to the least value the parameter takes
// has to remove the filter rather than keep a chip that narrows nothing.
#[test]
fn widening_a_range_to_the_whole_parameter_takes_the_filter_away() {
    let mut harness = app(campaign());
    harness
        .state_mut()
        .browser_mut()
        .browse
        .filters
        .push(ironlab_viewer::browse::Filter {
            key: FacetKey::parameter("angle"),
            constraint: ironlab_viewer::browse::Constraint::Between {
                low: 8.0,
                high: 12.0,
            },
        });
    harness.run();
    assert!(!listed(&harness, "Run 9 lift"));

    edit_filters(&harness).click();
    harness.run();
    harness
        .get_by_label_contains(&parameter_of("angle"))
        .click();
    harness.run();
    // The lower end is the first number field; clicking it opens it for typing, and what is typed replaces the
    // value as it is typed. Four degrees is the least angle in the campaign.
    fn lower_end<'a>(harness: &'a Harness<'_, ViewerApp>) -> egui_kittest::Node<'a> {
        harness
            .get_all_by_role(egui::accesskit::Role::SpinButton)
            .next()
            .expect("the lower end of the range")
    }
    lower_end(&harness).click();
    harness.run();
    assert!(
        lower_end(&harness).is_focused(),
        "clicking the lower end opens it for typing"
    );
    lower_end(&harness).type_text("4");
    harness.run();

    assert!(
        harness.state().browser().browse.filters.is_empty(),
        "the filter is gone rather than widened to the ends of the parameter"
    );
    assert!(listed(&harness, "Run 9 lift"), "and every figure is back");
}

// ---------------------------------------------------------------------------------
// The details below the canvas
// ---------------------------------------------------------------------------------

// Why: the labels and parameters are what the reader narrowed the collection by, so once a figure is on screen
// they want to see, without opening an editor, that it is the one they meant. The strip has to follow the figure
// shown, or it would describe a figure the reader is no longer looking at.
#[test]
fn the_details_below_the_canvas_say_what_the_shown_figure_carries() {
    let mut harness = app(campaign());
    assert!(
        harness.query_by_label("Tunnel A").is_some(),
        "the first figure's rig is shown beneath its canvas"
    );
    assert!(
        harness.query_by_label("angle").is_some(),
        "as is the name of its other parameter"
    );

    harness
        .get_by_label_contains(&row_of("Case B surface"))
        .click();
    harness.run();
    assert!(
        harness.query_by_label("LES").is_some(),
        "choosing another figure shows that figure's solver instead"
    );
    assert!(
        harness.query_by_label("Tunnel A").is_none(),
        "and the first figure's rig is gone with it"
    );
}

// Why: a figure carrying nothing has nothing to say beneath its canvas, and a strip with nothing in it would only
// take height from the figure.
#[test]
fn a_figure_carrying_nothing_has_no_details_strip() {
    let harness = app(vec![flat("Only", &[]), flat("Other", &[])]);
    assert!(
        egui::PanelState::load(&harness.ctx, egui::Id::new(ironlab_viewer::app::DETAILS_ID),)
            .is_none(),
        "no strip is drawn for a figure with no labels and no parameters"
    );
}

// Why: a row's second line is what makes the list worth reading rather than scanning, and it is the line that
// was lost when the row was a button, whose truncation ate the newline before it. The accessibility label carried
// the line all along, so every other test passed while the screen showed a title cut short. Only what is painted
// can say whether the line is there.
#[test]
fn the_second_line_of_a_row_is_actually_painted() {
    let words = painted_words_with(|browser| {
        browser.browse.sort.key = SortKey::Parameter("angle".to_owned());
    });
    assert!(
        words.iter().any(|word| word == "angle = 4"),
        "the value the list is ordered by is painted as a line of its own beneath the title: {words:?}"
    );
    assert!(
        words.iter().any(|word| word == "Run 9 lift"),
        "and the title is painted whole, not cut short to make room: {words:?}"
    );
}
