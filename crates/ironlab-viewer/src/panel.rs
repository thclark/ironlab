//! The property editor: a side panel holding the object tree and the inspector.
//!
//! The panel is hidden until the toolbar's **Properties** button opens it. It shows the
//! object tree of the displayed figure at the top and the properties of the selected node
//! below, both built by [`crate::inspector`] from the figure that is already composed, so
//! that drawing the panel neither recomposes the overlay nor recompiles the scene.
//!
//! Everything here is drawn from [`crate::widgets`], as the figure browser is: the tree is
//! rows, the inspector is a heading and a property row for every property, every control
//! in a row is the widget of its kind, and the panel names no size, colour or padding of
//! its own beyond the share of its height each part takes.
//!
//! A change made in the panel is committed through [`FigureState::try_record`], which
//! records it in the overlay exactly as a gesture does, or refuses it and reports the
//! reason when the IR will not accept it. While a numeric field is dragged or a text
//! field is being typed into, one undo step is held open, so that the whole drag or the
//! whole edit is undone at once.
//!
//! The foot of the panel holds the control that discards every change at once, away from
//! the rows that edit one property each.
//!
//! The panel's height is divided before its parts are drawn: the foot takes a strip of
//! fixed height at the bottom, the object tree takes the top and can be dragged between a
//! floor and a ceiling, and the inspector fills what is left. No part takes its height
//! from what it holds, so neither a long tree nor a long label in the foot can cover the
//! properties.

use std::collections::BTreeMap;

use ironlab_ir::{
    Axis, Cell as IrCell, Choice, Color, FigureSize, ImagePlacement, Legend, LineStyle,
    MarkerStyle, NodeId, Parameter, PixelRange, PropertyPath, Text, TileLayout, Value, ValueType,
    View3d, choices,
};

use crate::inspector::{
    DATA_REASON, Editor, ParameterKind, ParametersDraft, PropertyGroup, PropertyRow, TreeRow,
    commit, kind_name, property_groups, read_only_label, shape_label, tree_rows,
};
use crate::interaction::FigureState;
use crate::widgets::{
    Control, Detail, Face, Icon, Leading, Number, PanelKind, Property, Role, Row, RowState,
    Spacing, checkbox, choice, combo, field, heading, hint, note, number, problem, readout, swatch,
    text,
};

/// The identifier egui lays the object tree out under. It is named here so that the
/// space the tree is given can be measured.
pub const OBJECT_TREE_ID: &str = "ironlab_object_tree";

/// The identifier egui lays the foot of the panel out under. It is named here so that the
/// strip the foot occupies can be measured.
pub const FOOTER_ID: &str = "ironlab_panel_footer";

/// The height the object tree is given when the panel is first opened, in egui points.
const TREE_HEIGHT: f32 = 180.0;

/// The least height the object tree keeps, in egui points: enough for a heading and a
/// few rows, below which the tree is of no use.
const TREE_MIN_HEIGHT: f32 = 64.0;

/// The least height the inspector keeps, in egui points: enough for its heading and
/// several property rows. The object tree is never given so much of the panel that the
/// inspector is left less than this, so that the properties of the node just selected in
/// the tree can always be read and edited.
const INSPECTOR_MIN_HEIGHT: f32 = 120.0;

/// What the row that says how the source of a text is read means.
const INTERPRETER_DOCS: &str = "How the source of the text is read: as LaTeX, in which \
     mathematics is set between dollar signs, or as literal text.";

/// The width of the kind of a parameter in the parameters table, in egui points: room for
/// the longest kind and the triangle. The name and the value share what the kind and the
/// control that removes the entry leave.
const PARAMETER_KIND_WIDTH: f32 = 88.0;

/// The state of the property editor that belongs to the panel rather than to the figure.
#[derive(Clone, Debug, Default)]
pub struct PropertyPanel {
    /// Whether the panel is shown. A figure opens with it hidden.
    pub open: bool,
    /// The parameters of the figure as they are being edited.
    parameters: Option<ParametersDraft>,
    /// The parameters of the displayed figure when the draft was last in step with it,
    /// so that a change from elsewhere (an undo, a revert) refreshes the draft.
    parameters_seen: BTreeMap<String, Parameter>,
    /// Why the draft of the parameters cannot be committed, if it cannot.
    parameters_problem: Option<String>,
    /// The widget whose undo step is open, while it is dragged or typed into.
    editing: Option<egui::Id>,
    /// Whether the widget whose step is open was drawn this frame.
    touched: bool,
}

impl PropertyPanel {
    /// Opens an undo step while a widget is being dragged or typed into, and closes it
    /// when the widget is left, so that the whole change is one step.
    fn hold(&mut self, state: &mut FigureState, id: egui::Id, active: bool) {
        if self.editing == Some(id) {
            self.touched = true;
        }
        if active {
            if self.editing != Some(id) {
                if self.editing.is_some() {
                    state.end_edit_step();
                }
                state.begin_edit_step();
                self.editing = Some(id);
                self.touched = true;
            }
        } else if self.editing == Some(id) {
            state.end_edit_step();
            self.editing = None;
        }
    }
}

/// Draws the property editor in the right-hand side of `ui`, and returns whether the
/// displayed figure changed, which tells the canvas to recompile its scene.
///
/// Nothing is drawn while the panel is closed.
pub fn property_panel(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
) -> bool {
    if !panel.open {
        return false;
    }
    panel.touched = false;
    let frame = PanelKind::Bare.frame(ui);
    let changed = egui::Panel::right("ironlab_property_editor")
        .default_size(300.0)
        .min_size(220.0)
        .frame(frame)
        .show(ui, |ui| {
            crate::style::compact(ui);
            let mut changed = false;
            // The panel is divided before anything is drawn in it: the foot takes a strip
            // of the bottom whose height comes from the widgets, the object tree takes the
            // top and is held between a floor and a ceiling that leave the inspector its
            // room, and the inspector fills what is left. Each part is given its height
            // rather than taking the height of what it holds, so that neither a long tree
            // nor a long label in the foot can squeeze another part out.
            let foot = footer_height(ui);
            let tree_max =
                (ui.available_height() - foot - INSPECTOR_MIN_HEIGHT).max(TREE_MIN_HEIGHT);
            egui::Panel::top(OBJECT_TREE_ID)
                .resizable(true)
                .default_size(TREE_HEIGHT)
                .size_range(TREE_MIN_HEIGHT.min(tree_max)..=tree_max)
                .frame(PanelKind::AboveRule.frame(ui))
                .show(ui, |ui| object_tree(ui, state));
            egui::Panel::bottom(FOOTER_ID)
                .exact_size(foot)
                .frame(PanelKind::Foot.frame(ui))
                .show(ui, |ui| {
                    changed |= footer(ui, state);
                });
            egui::CentralPanel::default()
                .frame(PanelKind::Bare.frame(ui))
                .show(ui, |ui| {
                    changed |= inspector(ui, panel, state);
                });
            changed
        })
        .inner;
    if !panel.touched && panel.editing.is_some() {
        state.end_edit_step();
        panel.editing = None;
    }
    changed
}

// ---------------------------------------------------------------------------------
// The foot of the panel
// ---------------------------------------------------------------------------------

/// The label of the control that discards every change the user has made, which names
/// how many changes that is so the user knows what is at stake before clicking.
#[must_use]
pub fn revert_all_label(changes: usize) -> String {
    match changes {
        0 => "Revert all changes".to_owned(),
        1 => "Revert all changes (1)".to_owned(),
        changes => format!("Revert all changes ({changes})"),
    }
}

/// What the control that discards every change says when there is something to discard.
const REVERT_ALL_HINT: &str = "Discard every change you have made to this figure — axis \
     limits, three-dimensional views, hidden plots and every property edited — and show \
     the figure as the program that built it defined it. The figure itself is not \
     touched. This cannot be undone, unlike Refit in the toolbar, which discards \
     only the limits and three-dimensional views and can be undone.";

/// What it says when there is nothing to discard.
const REVERT_ALL_EMPTY_HINT: &str =
    "You have made no changes to this figure, so there is nothing to discard.";

/// The height of the strip at the foot of the panel, in egui points.
///
/// It is the height of a control in the foot's own padding, read from the widgets so
/// that it follows the size of the text, and never from what the foot holds, so that
/// neither the number of changes the control names nor the width the label is given can
/// change how much of the panel is left for the inspector above it.
fn footer_height(ui: &egui::Ui) -> f32 {
    Control::height(ui) + 2.0 * Spacing::ROW_Y
}

/// Draws the foot of the panel: the control that discards every change the user has made.
///
/// It sits in the bottom-right corner of the panel, away from the property rows, because
/// it throws away every change at once rather than editing one of them. Its words are
/// cut short rather than wrapped, so that a narrow panel or a large number of changes
/// shortens what the foot says instead of making the foot taller. Returns whether the
/// displayed figure changed.
fn footer(ui: &mut egui::Ui, state: &mut FigureState) -> bool {
    let changes = state.change_count();
    let words = revert_all_label(changes);
    let clicked = ui
        .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = Control::button(&words)
                .before(Icon::Restore)
                .spoken(words.clone())
                .enabled(changes > 0)
                .show(ui);
            hint(
                response,
                if changes > 0 {
                    REVERT_ALL_HINT
                } else {
                    REVERT_ALL_EMPTY_HINT
                },
            )
            .clicked()
        })
        .inner;
    clicked && state.revert_all()
}

// ---------------------------------------------------------------------------------
// The object tree
// ---------------------------------------------------------------------------------

/// Draws the figure, its axes and their artists as a collapsible tree of rows, and
/// selects the node whose row is clicked.
///
/// A row that has rows beneath it carries a disclosure triangle; clicking the triangle
/// opens or closes it, and clicking anywhere else on the row selects the node. A hidden
/// plot is drawn dimmed and can still be selected, which is how it is shown again.
fn object_tree(ui: &mut egui::Ui, state: &mut FigureState) {
    heading(ui, "Objects", None);
    let rows = tree_rows(state.figure());
    let selection = state.selection();
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .id_salt("ironlab_object_tree_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let height = Row::height(ui, false);
            let mut index = 0;
            while index < rows.len() {
                index = subtree(ui, &rows, index, selection, height, &mut clicked);
            }
        });
    if let Some(node) = clicked {
        state.select(Some(node));
    }
}

/// Draws the row at `index` and the rows below it, and returns the index after them.
fn subtree(
    ui: &mut egui::Ui,
    rows: &[TreeRow],
    index: usize,
    selection: Option<NodeId>,
    height: f32,
    clicked: &mut Option<NodeId>,
) -> usize {
    let row = &rows[index];
    let end = rows[index + 1..]
        .iter()
        .position(|other| other.depth <= row.depth)
        .map_or(rows.len(), |offset| index + 1 + offset);
    let has_children = index + 1 < end;
    let id = ui.make_persistent_id(("ironlab_tree", row.node.0));
    let open = has_children && ui.ctx().data_mut(|data| *data.get_temp_mut_or(id, true));
    let leading = if has_children {
        Leading::Disclosure { open }
    } else {
        Leading::None
    };
    let response = Row::new(text(Role::Body, &row.label), Detail::None)
        .leading(leading)
        .indent(row.depth)
        .state(RowState {
            selected: selection == Some(row.node),
            dimmed: row.dimmed,
            ..RowState::default()
        })
        .show(ui, height);
    let response = if row.dimmed {
        hint(response, "This plot is hidden.")
    } else {
        response
    };
    if response.clicked() {
        // A click on the triangle opens or closes the rows beneath; a click anywhere else
        // on the row selects the node.
        #[allow(clippy::cast_precision_loss)]
        let triangle = response.rect.min.x
            + Spacing::INSET
            + row.depth as f32 * Spacing::INDENT
            + Icon::SLOT
            + Spacing::GAP;
        let on_triangle = has_children
            && response
                .interact_pointer_pos()
                .is_some_and(|pointer| pointer.x < triangle);
        if on_triangle {
            ui.ctx().data_mut(|data| data.insert_temp(id, !open));
        } else {
            *clicked = Some(row.node);
        }
    }
    if !open {
        return end;
    }
    let mut next = index + 1;
    while next < end {
        next = subtree(ui, rows, next, selection, height, clicked);
    }
    end
}

// ---------------------------------------------------------------------------------
// The inspector
// ---------------------------------------------------------------------------------

/// Draws the properties of the selected node, and returns whether the figure changed.
fn inspector(ui: &mut egui::Ui, panel: &mut PropertyPanel, state: &mut FigureState) -> bool {
    let Some(node) = state.selection() else {
        note(
            ui,
            "Nothing selected",
            "Select an object in the tree to see its properties.",
        );
        return false;
    };
    let Some(kind) = state.figure().node_kind(node) else {
        note(
            ui,
            "Nothing to show",
            "The selected object is no longer in the figure.",
        );
        return false;
    };
    heading(ui, kind_name(kind), Some(&node.to_string()));
    let groups = property_groups(state.figure(), state.overlay(), node);
    let mut changed = false;
    egui::ScrollArea::vertical()
        .id_salt("ironlab_inspector_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut stripe = Stripe::default();
            for group in &groups {
                changed |= property_group(ui, panel, state, node, group, &mut stripe);
            }
        });
    changed
}

/// Which rows of the inspector are drawn on the stripe: every second property row, so
/// that the eye can follow one row from its name to its control.
#[derive(Default)]
struct Stripe(usize);

impl Stripe {
    fn next(&mut self) -> bool {
        self.0 += 1;
        self.0.is_multiple_of(2)
    }
}

/// Draws one group of properties, and returns whether the figure changed.
fn property_group(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
    node: NodeId,
    group: &PropertyGroup,
    stripe: &mut Stripe,
) -> bool {
    // A group whose own value is a row is named by that row; one that is only a
    // container (an axis, a line style) is named by a heading of its own: a band across
    // the inspector, as the heading of a group in the browser's list is, beneath which
    // the properties it gathers are indented.
    if group.rows.first().is_none_or(|row| !row.label.is_empty()) {
        Row::new(text(Role::Label, &group.name), Detail::None)
            .state(RowState {
                band: true,
                ..RowState::default()
            })
            .passive()
            .show(ui, Row::height(ui, false));
        *stripe = Stripe::default();
    }
    let mut changed = false;
    for row in &group.rows {
        changed |= property_row(ui, panel, state, node, group, row, stripe);
    }
    changed
}

/// Draws one property, and returns whether the figure changed.
///
/// Every row is a [`Property`]: the name at the left, set in by how deeply the property
/// is nested and coloured when the reader has changed it; the control beside it, in the
/// column every control shares; and the control that takes the change back in the column
/// reserved at the right. A rich text takes two rows, its source and beneath it how the
/// source is read, because a field and a combo box side by side leave a panel of the
/// usual width too little of the field to read.
fn property_row(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
    node: NodeId,
    group: &PropertyGroup,
    row: &PropertyRow,
    stripe: &mut Stripe,
) -> bool {
    let name = if row.label.is_empty() {
        group.name.clone()
    } else {
        row.label.clone()
    };
    let property = Property::new(&name)
        .depth(row.depth)
        .changed(row.overridden)
        .striped(stripe.next())
        .docs(row.docs);
    if matches!(row.editor, Editor::Parameters) {
        // The parameters are edited in a table of their own below the row that names
        // them, which is too wide for the column of controls. The row itself keeps the
        // columns of every other row, so that the control that takes the parameters back
        // stands where every other revert control stands.
        let response = property.show(ui, |_| ());
        let mut changed = response.restore && state.revert(node, &row.path);
        changed |= parameters_editor(ui, panel, state);
        return changed;
    }
    let response = property.show(ui, |ui| {
        if row.value == Value::Unset {
            return unset_control(ui, state, node, row, &name);
        }
        match &row.editor {
            Editor::Bool => bool_control(ui, panel, state, node, row, &name),
            Editor::Number {
                speed,
                range,
                integer,
            } => number_control(ui, panel, state, node, row, *speed, *range, *integer),
            Editor::Choice { offered } => choice_control(ui, state, node, row, offered),
            Editor::Color => color_control(ui, panel, state, node, row),
            Editor::Text | Editor::RichText => text_control(ui, panel, state, node, row),
            Editor::Numbers => numbers_control(ui, panel, state, node, row),
            Editor::Words => words_control(ui, panel, state, node, row),
            Editor::Data { shape } => {
                let text = data_label(row, shape.as_deref());
                read_only_control(ui, row, &text, DATA_REASON);
                false
            }
            Editor::ReadOnly { reason } => {
                read_only_control(ui, row, &read_only_label(&row.value), reason);
                false
            }
            Editor::Group | Editor::Parameters => false,
        }
    });
    let mut changed = response.inner;
    if response.restore {
        changed |= state.revert(node, &row.path);
    }
    if row.value != Value::Unset && row.editor == Editor::RichText {
        changed |= Property::new("interpreter")
            .depth(row.depth + 1)
            .striped(stripe.next())
            .docs(INTERPRETER_DOCS)
            .show(ui, |ui| interpreter_control(ui, state, node, row))
            .inner;
    }
    changed
}

/// Commits a new value of a property, and returns whether the figure changed.
fn set(state: &mut FigureState, node: NodeId, path: &PropertyPath, value: Value) -> bool {
    let transaction = commit(state.figure(), node, path, value);
    state.try_record(&transaction)
}

/// The identifier of the control of a property, from its path, so that the control keeps
/// its state and its undo step whatever is drawn around it.
fn control_id(row: &PropertyRow) -> egui::Id {
    egui::Id::new(("ironlab_property", row.path.to_string()))
}

/// Draws the control of a boolean property: a checkbox with no caption of its own, so
/// that it stands in the column of controls with the numeric fields and combo boxes
/// rather than at the left of its row. The name of the property it changes is given to
/// the accessibility tree in its place, so that the control is still named where it is
/// read rather than seen.
fn bool_control(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
    name: &str,
) -> bool {
    let Value::Bool(current) = row.value else {
        return false;
    };
    let mut flag = current;
    let response = hint(checkbox(ui, &mut flag, name), row.docs);
    panel.hold(state, response.id, response.has_focus());
    response.changed() && set(state, node, &row.path, Value::Bool(flag))
}

#[allow(clippy::too_many_arguments)]
fn number_control(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
    speed: f64,
    range: Option<(f64, f64)>,
    integer: bool,
) -> bool {
    let current = match &row.value {
        Value::Double(number) => *number,
        Value::Float(number) => f64::from(*number),
        Value::UInt32(number) => f64::from(*number),
        _ => return false,
    };
    let mut value = current;
    let format = Number {
        speed,
        range,
        integer,
    };
    let response = hint(number(ui, &mut value, format, control_id(row)), row.docs);
    panel.hold(
        state,
        response.id,
        response.dragged() || response.has_focus(),
    );
    if !response.changed() {
        return false;
    }
    let value = match &row.value {
        Value::Float(_) => Value::Float(value as f32),
        Value::UInt32(_) => Value::UInt32(value.clamp(0.0, f64::from(u32::MAX)) as u32),
        _ if integer => Value::Double(value.round()),
        _ => Value::Double(value),
    };
    set(state, node, &row.path, value)
}

/// Draws the control of a property chosen from a list: a combo box of every choice the
/// IR offers, filling the column, whose menu greys a choice the IR would not act on here
/// with the reason on hover rather than leaving it out.
fn choice_control(
    ui: &mut egui::Ui,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
    offered: &[Choice],
) -> bool {
    let current = offered
        .iter()
        .find(|choice| choice.matches(&row.value))
        .map_or("…", |choice| choice.label);
    let mut chosen: Option<Value> = None;
    let width = ui.available_width();
    combo(ui, control_id(row), current, width, |ui| {
        for offer in offered {
            if choice(ui, offer.label, offer.label == current, offer.unavailable).clicked()
                && offer.label != current
            {
                chosen = Some(offer.value.clone());
            }
        }
    });
    match chosen {
        Some(value) => set(state, node, &row.path, value),
        None => false,
    }
}

/// Draws the control of a colour: a swatch of it and its hex, which opens the picker.
/// The undo step is held open while the picker is open, so that a colour chosen by
/// several movements of the picker is one step.
fn color_control(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
) -> bool {
    let Value::Color(current) = row.value else {
        return false;
    };
    let mut rgba = to_color32(current);
    let id = control_id(row);
    hint(swatch(ui, &mut rgba, id), row.docs);
    let open = egui::Popup::is_id_open(ui.ctx(), id.with("picker"));
    panel.hold(state, id, open);
    let chosen = from_color32(rgba);
    chosen != current && set(state, node, &row.path, Value::Color(chosen))
}

/// Draws the control of a text property: a field holding its source. A rich text keeps
/// the interpreter it has; that is changed in the row beneath.
fn text_control(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
) -> bool {
    let (mut content, interpreter) = match &row.value {
        Value::Text(text) => (text.content.clone(), Some(text.interpreter)),
        Value::String(text) => (text.clone(), None),
        _ => return false,
    };
    let response = hint(field(ui, &mut content, "empty", control_id(row)), row.docs);
    panel.hold(state, response.id, response.has_focus());
    if !response.changed() {
        return false;
    }
    let value = match interpreter {
        Some(interpreter) => Value::Text(Text {
            content,
            interpreter,
        }),
        None => Value::String(content),
    };
    set(state, node, &row.path, value)
}

/// Draws the control that says how the source of a rich text is read: a combo box of the
/// interpreters, filling the column as every choice does.
fn interpreter_control(
    ui: &mut egui::Ui,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
) -> bool {
    let Value::Text(text) = &row.value else {
        return false;
    };
    let offered = choices(ValueType::Interpreter, None);
    let current = offered
        .iter()
        .find(|choice| choice.matches(&Value::Interpreter(text.interpreter)))
        .map_or("…", |choice| choice.label);
    let mut chosen = None;
    let width = ui.available_width();
    combo(
        ui,
        control_id(row).with("interpreter"),
        current,
        width,
        |ui| {
            for offer in &offered {
                if choice(ui, offer.label, offer.label == current, None).clicked()
                    && let Value::Interpreter(picked) = offer.value
                    && picked != text.interpreter
                {
                    chosen = Some(picked);
                }
            }
        },
    );
    match chosen {
        Some(interpreter) => set(
            state,
            node,
            &row.path,
            Value::Text(Text {
                content: text.content.clone(),
                interpreter,
            }),
        ),
        None => false,
    }
}

fn numbers_control(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
) -> bool {
    let Value::Doubles(current) = &row.value else {
        return false;
    };
    let mut typed = current
        .iter()
        .map(f64::to_string)
        .collect::<Vec<String>>()
        .join(", ");
    let response = hint(
        field(ui, &mut typed, "empty", control_id(row)),
        &format!("{} Separate the numbers with commas.", row.docs),
    );
    panel.hold(state, response.id, response.has_focus());
    if !response.changed() {
        return false;
    }
    let parsed: Result<Vec<f64>, _> = typed
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::parse::<f64>)
        .collect();
    match parsed {
        Ok(numbers) if numbers != *current => set(state, node, &row.path, Value::Doubles(numbers)),
        _ => false,
    }
}

/// Draws the control of a list of words: a field holding them separated by commas.
///
/// The labels of a figure are short words rather than sentences, and there are a handful of
/// them, so one field holding the lot reads as what it is and is quicker to change than a
/// row of fields would be. A word is trimmed of the spaces around it, and an empty one is
/// dropped, so that a trailing comma while typing does not make a label of nothing; the
/// figure refuses a repeat, and says so in the problems list.
fn words_control(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
) -> bool {
    let Value::Strings(current) = &row.value else {
        return false;
    };
    let mut typed = current.join(", ");
    let response = hint(
        field(ui, &mut typed, "empty", control_id(row)),
        &format!("{} Separate the words with commas.", row.docs),
    );
    panel.hold(state, response.id, response.has_focus());
    if !response.changed() {
        return false;
    }
    let words: Vec<String> = typed
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    words != *current && set(state, node, &row.path, Value::Strings(words))
}

/// Draws a property that the panel shows but cannot change: its value as a readout, with
/// what the property means and the reason it cannot be changed here on hover.
///
/// Every read-only property is drawn here, whichever kind it is, so that the rule holds in
/// one place: the value dimmed, the reason on hover, and nothing else. Nothing is drawn
/// beside the value — no lock, no badge — because a mark that says only "this cannot be
/// changed" says less than the dimmed value does.
fn read_only_control(ui: &mut egui::Ui, row: &PropertyRow, value: &str, reason: &str) {
    hint(
        readout(ui, text(Role::Body, value)),
        &format!("{} {reason}", row.docs),
    );
}

/// The value of a reference to a data array, written as the array it names and that
/// array's shape.
fn data_label(row: &PropertyRow, shape: Option<&[usize]>) -> String {
    match &row.value {
        Value::DataId(id) => format!("{id} {}", shape_label(shape)),
        _ => shape_label(shape),
    }
}

/// Draws an optional value that is absent: the word that says there is none, and at the
/// right of the column a control that gives it the default of its type. A data reference
/// has no default to give, so it is only reported.
fn unset_control(
    ui: &mut egui::Ui,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
    name: &str,
) -> bool {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let changed = match default_value(row.value_type) {
            None => false,
            Some(value) => {
                hint(
                    Control::button("Set").quiet().show(ui),
                    &format!("Give {name} a value."),
                )
                .clicked()
                    && set(state, node, &row.path, value)
            }
        };
        hint(readout(ui, text(Role::Body, "unset")), row.docs);
        changed
    })
    .inner
}

/// The value that an absent optional property is given when the user asks for one, or
/// `None` for a reference to a data array, which the viewer never invents.
///
/// The match is exhaustive, so a value type added to the IR does not compile until it is
/// given a default here.
fn default_value(value_type: ValueType) -> Option<Value> {
    if let Some(choice) = choices(value_type, None).into_iter().next() {
        return Some(choice.value);
    }
    match value_type {
        ValueType::Bool => Some(Value::Bool(false)),
        ValueType::UInt32 => Some(Value::UInt32(0)),
        ValueType::Double => Some(Value::Double(0.0)),
        ValueType::Float => Some(Value::Float(0.0)),
        ValueType::String => Some(Value::String(String::new())),
        ValueType::Doubles => Some(Value::Doubles(Vec::new())),
        ValueType::Strings => Some(Value::Strings(Vec::new())),
        ValueType::Text => Some(Value::Text(Text::default())),
        ValueType::Color => Some(Value::Color(Color::BLACK)),
        ValueType::FigureSize => Some(Value::FigureSize(FigureSize::default())),
        ValueType::TileLayout => Some(Value::TileLayout(TileLayout::default())),
        ValueType::Links => Some(Value::Links(Vec::new())),
        ValueType::Parameters => Some(Value::Parameters(BTreeMap::new())),
        ValueType::Cell => Some(Value::Cell(IrCell::default())),
        ValueType::View3d => Some(Value::View3d(View3d::default())),
        ValueType::Axis => Some(Value::Axis(Axis::default())),
        ValueType::Legend => Some(Value::Legend(Legend::default())),
        ValueType::LineStyle => Some(Value::LineStyle(LineStyle::default())),
        ValueType::MarkerStyle => Some(Value::MarkerStyle(MarkerStyle::default())),
        ValueType::ImagePlacement => Some(Value::ImagePlacement(ImagePlacement::default())),
        ValueType::PixelRange => Some(Value::PixelRange(PixelRange::default())),
        ValueType::DataId => None,
        // Every remaining type has choices, and the first of them was returned above.
        ValueType::Projection
        | ValueType::Limits
        | ValueType::ColorSpec
        | ValueType::ScatterSize
        | ValueType::ScatterColor
        | ValueType::Grid
        | ValueType::Levels
        | ValueType::ContourPlacement
        | ValueType::QuiverScale
        | ValueType::ImagePlane
        | ValueType::OutOfRange
        | ValueType::Scale
        | ValueType::ColormapName
        | ValueType::LegendLocation
        | ValueType::MarkerShape
        | ValueType::DashStyle
        | ValueType::Interpreter
        | ValueType::FontSetId => None,
    }
}

fn to_color32(color: Color) -> egui::Color32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgba_unmultiplied(
        channel(color.r),
        channel(color.g),
        channel(color.b),
        channel(color.a),
    )
}

fn from_color32(color: egui::Color32) -> Color {
    let channel = |value: u8| f32::from(value) / 255.0;
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color::rgba(channel(r), channel(g), channel(b), channel(a))
}

// ---------------------------------------------------------------------------------
// The parameters of the figure
// ---------------------------------------------------------------------------------

/// The path of the figure's named parameters.
fn parameters_path() -> PropertyPath {
    PropertyPath::new(["parameters"]).expect("parameters names a property")
}

/// Draws the editor of the figure's named parameters, and returns whether the figure
/// changed.
///
/// Each entry is a row of the table: its name, its kind, its value, and the control that
/// removes it. The entries are edited one at a time but committed as one set of the whole
/// map, so that a change is one entry of the overlay and one step of the history. A
/// draft that cannot be committed (an entry with no name, two entries with one name, a
/// number that is not one) is kept and its reason is shown beneath the table, so that
/// typing is never interrupted.
fn parameters_editor(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
) -> bool {
    let node = state.figure().id;
    let current = state.figure().parameters.clone();
    if panel.parameters.is_none() || panel.parameters_seen != current {
        panel.parameters = Some(ParametersDraft::of(&current));
        panel.parameters_seen = current.clone();
        panel.parameters_problem = None;
    }
    let mut draft = panel.parameters.take().unwrap_or_default();

    let mut remove = None;
    let mut active: Option<egui::Id> = None;
    crate::widgets::block(ui, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(Spacing::GAP, Spacing::GAP);
        for (index, row) in draft.rows_mut().iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let shared = (ui.available_width()
                    - PARAMETER_KIND_WIDTH
                    - Spacing::restore_column()
                    - 3.0 * Spacing::GAP)
                    / 2.0;
                let name = ui
                    .scope(|ui| {
                        ui.set_max_width(shared);
                        field(
                            ui,
                            &mut row.name,
                            "name",
                            egui::Id::new(("ironlab_parameter_name", index)),
                        )
                    })
                    .inner;
                if name.has_focus() {
                    active = Some(name.id);
                }
                combo(
                    ui,
                    ("ironlab_parameter_kind", index),
                    row.kind.label(),
                    PARAMETER_KIND_WIDTH,
                    |ui| {
                        for kind in ParameterKind::ALL {
                            if choice(ui, kind.label(), row.kind == kind, None).clicked() {
                                row.kind = kind;
                            }
                        }
                    },
                );
                ui.scope(|ui| {
                    ui.set_max_width(shared);
                    if row.kind == ParameterKind::Bool {
                        checkbox(ui, &mut row.flag, &row.name);
                    } else {
                        let value = field(
                            ui,
                            &mut row.text,
                            "value",
                            egui::Id::new(("ironlab_parameter_value", index)),
                        );
                        if value.has_focus() {
                            active = Some(value.id);
                        }
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let response = Control::new(text(Role::Control, ""), Face::Quiet)
                        .before(Icon::Cross)
                        .spoken(format!("Remove {}", row.name))
                        .show(ui);
                    if hint(response, "Remove this parameter.").clicked() {
                        remove = Some(index);
                    }
                });
            });
        }
        let add = Control::button("Add parameter")
            .before(Icon::Plus)
            .spoken("Add parameter")
            .show(ui);
        if hint(add, "Add a named value that describes the figure.").clicked() {
            draft.add();
        }
        if let Some(words) = &panel.parameters_problem {
            problem(ui, words);
        }
    });
    if let Some(index) = remove {
        draft.remove(index);
    }

    if let Some(id) = active {
        panel.hold(state, id, true);
    }
    let mut changed = false;
    match draft.to_map() {
        Ok(map) => {
            panel.parameters_problem = None;
            if map != current {
                changed = set(
                    state,
                    node,
                    &parameters_path(),
                    Value::Parameters(map.clone()),
                );
                panel.parameters_seen = state.figure().parameters.clone();
            }
        }
        Err(words) => panel.parameters_problem = Some(words),
    }
    panel.parameters = Some(draft);
    changed
}
