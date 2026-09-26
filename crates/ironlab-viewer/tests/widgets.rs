//! Tests of the widgets the property editor is to be drawn from: the property row and the controls that stand in it.
//!
//! What a widget looks like is judged in the widgets gallery (`cargo run -p ironlab-viewer --example widgets`). What
//! can be tested is what a widget promises to the program around it and to a reader who cannot see it: which
//! controls a row offers and when, what a control is called in the accessibility tree, what a click or a typed value
//! leaves behind, and what a reader is told on hover. Every widget is driven through egui_kittest's accessibility
//! tree, so a widget that reaches the screen without reaching the tree fails here.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ironlab_viewer::widgets::{
    Number, Property, Role as TextRole, checkbox, choice, heading, hint, number, readout, swatch,
    text,
};

/// The width the property editor's panel is given, which is what a row's columns must share.
const PANEL: egui::Vec2 = egui::vec2(300.0, 400.0);

/// A harness drawing `ui` over `state` in a panel of [`PANEL`] size, run once so that the tree is built.
fn driven<State: 'static>(
    state: State,
    ui: impl FnMut(&mut egui::Ui, &mut State) + 'static,
) -> Harness<'static, State> {
    let mut harness = Harness::builder()
        .with_size(PANEL)
        .build_ui_state(ui, state);
    harness.run();
    harness
}

// Why: the restore column is reserved on every row so that no control shifts sideways when a property becomes
// changed, but the control in it must exist only while there is a change to take back. A restore control on an
// unchanged row would be a dead control; a changed row without one would leave no way back but "Revert all".
#[test]
fn a_property_row_offers_its_restore_control_only_while_the_property_is_changed() {
    #[derive(Default)]
    struct State {
        changed: bool,
        restored: bool,
    }
    let mut harness = driven(State::default(), |ui, state| {
        let response = Property::new("visible")
            .changed(state.changed)
            .show(ui, |ui| {
                let mut flag = true;
                checkbox(ui, &mut flag, "visible");
            });
        state.restored |= response.restore;
    });

    assert!(
        harness.query_by_label("restore visible").is_none(),
        "an unchanged property offers nothing to take back"
    );

    harness.state_mut().changed = true;
    harness.run();
    harness.get_by_label("restore visible").click();
    harness.run();
    assert!(
        harness.state().restored,
        "clicking the restore control of a changed property reports the restore"
    );
}

// Why: a property's name is the whole of how a reader finds it, and its documentation is the whole of how a reader
// learns what it does, so both must reach the accessibility tree: the name as a label, and the documentation when
// the name is hovered rather than always, because a panel of forty rows cannot carry forty sentences on screen.
#[test]
fn a_property_row_names_its_property_and_explains_it_on_hover() {
    const DOCS: &str = "Whether the plot is drawn.";
    let mut harness = driven((), |ui, ()| {
        Property::new("visible").docs(DOCS).show(ui, |_| ());
    });

    assert!(
        harness.query_by_label_contains(DOCS).is_none(),
        "the documentation is not on screen until it is asked for"
    );
    harness
        .get_by_role_and_label(Role::Label, "visible")
        .hover();
    harness.run();
    harness.get_by_label_contains(DOCS);
}

// Why: a checkbox in the column of controls has no caption of its own, because its row already names the property.
// It must still be found by that name where it is read rather than seen, and a click must leave the flag toggled,
// which is the whole of what the property editor asks of it.
#[test]
fn a_checkbox_is_named_by_what_it_changes_and_toggles_on_click() {
    let mut harness = driven(false, |ui, flag| {
        checkbox(ui, flag, "visible");
    });

    harness
        .get_by_role_and_label(Role::CheckBox, "visible")
        .click();
    harness.run();
    assert!(*harness.state(), "clicking an unticked checkbox ticks it");

    harness
        .get_by_role_and_label(Role::CheckBox, "visible")
        .click();
    harness.run();
    assert!(!*harness.state(), "clicking it again unticks it");
}

// Why: the property editor's numbers carry the constraints the IR states — a count of rows is a whole number of at
// least one, an elevation lies between -90 and 90 degrees — and a field that let a value outside them through would
// hand the IR a change to refuse. The field holds the value to its range and to whole numbers itself, so that what
// leaves it is what the IR will take.
#[test]
fn a_number_field_holds_a_typed_value_to_its_range_and_to_whole_numbers() {
    let mut harness = driven(3.0_f64, |ui, value| {
        number(
            ui,
            value,
            Number::integer().range(1.0, 8.0),
            egui::Id::new("rows"),
        );
    });

    // A click turns the field into a text edit, which is still the spin button to the accessibility tree.
    harness.get_by_role(Role::SpinButton).click();
    harness.run();
    harness.get_by_role(Role::SpinButton).type_text("12.7");
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(
        (*harness.state() - 8.0).abs() < f64::EPSILON,
        "a typed value beyond the range is held to it; the field holds {}",
        harness.state()
    );

    let mut harness = driven(3.0_f64, |ui, value| {
        number(ui, value, Number::integer(), egui::Id::new("count"));
    });
    harness.get_by_role(Role::SpinButton).click();
    harness.run();
    harness.get_by_role(Role::SpinButton).type_text("2.6");
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(
        (*harness.state() - 3.0).abs() < f64::EPSILON,
        "a fraction typed into a whole-number field is rounded; the field holds {}",
        harness.state()
    );
}

// Why: a colour is named by its hex value, which is what a scientist writes in code and what a reader can copy
// elsewhere, and the value must say when the colour is not opaque, because an alpha the swatch hides is an alpha
// the reader cannot know about. The picker is reached from the swatch, not drawn beside it, because the column of
// controls has no room for it.
#[test]
fn a_swatch_reads_as_the_hex_of_its_colour_and_opens_the_picker_when_clicked() {
    let mut harness = driven(egui::Color32::from_rgb(31, 119, 180), |ui, color| {
        swatch(ui, color, egui::Id::new("stroke"));
    });
    // The picker is egui's own, and what it puts in the accessibility tree is its spin buttons of the channels.
    assert!(
        harness.query_by_role(Role::SpinButton).is_none(),
        "the picker is not on screen until the swatch is clicked"
    );
    harness.get_by_label("swatch #1F77B4").click();
    harness.run();
    assert!(
        harness.query_all_by_role(Role::SpinButton).count() > 0,
        "clicking the swatch opens the picker beneath it"
    );

    // egui stores a colour premultiplied, so the channels of a translucent colour do not survive exactly; what is
    // promised is that the alpha is written, as two more digits.
    let harness = driven(
        egui::Color32::from_rgba_unmultiplied(31, 119, 180, 128),
        |ui, color| {
            swatch(ui, color, egui::Id::new("fill"));
        },
    );
    let label = harness
        .get_by_role(Role::Button)
        .accesskit_node()
        .label()
        .expect("the swatch is named");
    assert_eq!(
        label.len(),
        "swatch #".len() + 8,
        "a translucent colour is written with its alpha, as {label:?} is not"
    );
    assert!(
        label.ends_with("80"),
        "the alpha of {label:?} is 128, which is 80 in hex"
    );
}

// Why: a value that cannot be changed is still shown, and the reader's first question about it is "why can I not
// change this?" The answer is given on hover, from the readout itself, so that a read-only row never needs a second
// control to carry its explanation.
#[test]
fn a_readout_carries_its_reason_on_hover() {
    const REASON: &str = "The groups of linked axes are set by the program that builds the figure.";
    let mut harness = driven((), |ui, ()| {
        let response = readout(ui, text(TextRole::Body, "no linked axes"));
        hint(response, REASON);
    });

    assert!(harness.query_by_label_contains(REASON).is_none());
    harness
        .get_by_role_and_label(Role::Label, "no linked axes")
        .hover();
    harness.run();
    harness.get_by_label_contains(REASON);
}

// Why: a choice the IR would not act on is listed rather than hidden, so that the reader sees the value exists and
// reads what would make it available; but listing it means it must not be takeable, or a click would change nothing
// and explain nothing.
#[test]
fn an_unavailable_choice_cannot_be_taken_and_says_why_on_hover() {
    const REASON: &str = "A logarithmic scale needs limits above zero.";
    #[derive(Default)]
    struct State {
        chosen: Option<&'static str>,
    }
    let mut harness = driven(State::default(), |ui, state| {
        if choice(ui, "Linear", true, None).clicked() {
            state.chosen = Some("Linear");
        }
        if choice(ui, "Logarithmic", false, Some(REASON)).clicked() {
            state.chosen = Some("Logarithmic");
        }
    });

    let unavailable = harness.get_by_label("Logarithmic");
    assert!(
        unavailable.accesskit_node().is_disabled(),
        "a choice that cannot be taken is disabled where it is read"
    );
    unavailable.click();
    harness.run();
    assert_eq!(
        harness.state().chosen,
        None,
        "clicking an unavailable choice chooses nothing"
    );

    harness.get_by_label("Logarithmic").hover();
    harness.run();
    harness.get_by_label_contains(REASON);

    harness.get_by_label("Linear").click();
    harness.run();
    assert_eq!(harness.state().chosen, Some("Linear"));
}

// Why: a heading is spelled in capitals on screen, but it is read aloud as it was written, because "OBJECTS" read
// letter by letter is what a screen reader makes of a word in capitals. The datum beside it is read as a label of
// its own, so that a count or a name is found by its own words.
#[test]
fn a_heading_is_read_as_it_was_written_and_its_datum_is_read_by_itself() {
    let harness = driven((), |ui, ()| {
        heading(ui, "Objects", Some("3"));
    });

    harness.get_by_role_and_label(Role::Label, "Objects");
    harness.get_by_role_and_label(Role::Label, "3");
    assert!(
        harness.query_by_label("OBJECTS").is_none(),
        "a heading is not read in the capitals it is spelled in"
    );
}
