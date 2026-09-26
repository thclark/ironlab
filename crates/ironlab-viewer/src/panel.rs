//! The property editor: a side panel holding the object tree and the inspector.
//!
//! The panel is hidden until the toolbar's **Properties** button opens it. It shows the
//! object tree of the displayed figure at the top and the properties of the selected node
//! below, both built by [`crate::inspector`] from the figure that is already composed, so
//! that drawing the panel neither recomposes the overlay nor recompiles the scene.
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

use std::cell::Cell;
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

/// The identifier egui lays the object tree out under. It is named here so that the
/// space the tree is given can be measured.
pub const OBJECT_TREE_ID: &str = "ironlab_object_tree";

/// The identifier egui lays the foot of the panel out under. It is named here so that the
/// strip the foot occupies can be measured.
pub const FOOTER_ID: &str = "ironlab_panel_footer";

/// The width of the column of property names, in egui points.
///
/// Every name is drawn within this column and every control begins where it ends, so that
/// the controls of a node form one column down the panel however deeply the properties
/// they belong to are nested.
const NAME_WIDTH: f32 = 104.0;

/// The width of the column at the right of every property row, in egui points, which
/// holds the control that takes back a change to that property.
///
/// The column is the same width on every row, whether or not the row is overridden, so
/// that a control never shifts sideways when the property it edits becomes overridden and
/// the revert controls stand in a column of their own.
const REVERT_WIDTH: f32 = 24.0;

/// The least width the column of controls keeps, in egui points, when the panel is too
/// narrow to give every column its share. The room a narrow panel needs is taken from the
/// controls, which are still the same controls in less room, rather than from the names,
/// which are what make a row findable at all.
const CONTROL_MIN_WIDTH: f32 = 56.0;

/// The least width a property name is drawn in, in egui points, however deeply it is
/// nested.
const NAME_MIN_WIDTH: f32 = 32.0;

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

/// The room left above and below the control in the foot of the panel, in egui points.
const FOOTER_PADDING: f32 = 4.0;

/// The indent of each level of nesting, in egui points. It is wide enough that the level
/// a property belongs to can be seen without comparing it with the row above.
const INDENT: f32 = 16.0;

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
    let changed = egui::Panel::right("ironlab_property_editor")
        .default_size(300.0)
        .min_size(220.0)
        .show(ui, |ui| {
            let mut changed = false;
            // The panel is divided before anything is drawn in it: the foot takes a strip
            // of the bottom whose height comes from the style, the object tree takes the
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
                .show(ui, |ui| object_tree(ui, state));
            egui::Panel::bottom(FOOTER_ID)
                .exact_size(foot)
                .show(ui, |ui| {
                    changed |= footer(ui, state);
                });
            egui::CentralPanel::default().show(ui, |ui| {
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
     touched. This cannot be undone, unlike Reset in the toolbar, which discards \
     only the limits and three-dimensional views and can be undone.";

/// What it says when there is nothing to discard.
const REVERT_ALL_EMPTY_HINT: &str =
    "You have made no changes to this figure, so there is nothing to discard.";

/// The height of the strip at the foot of the panel, in egui points.
///
/// It is read from the style, so that it follows the size of the text and of the controls
/// the user has chosen, and never from what the foot holds, so that neither the number of
/// changes the control names nor the width the label is given can change how much of the
/// panel is left for the inspector above it.
fn footer_height(ui: &egui::Ui) -> f32 {
    let spacing = ui.spacing();
    let button = spacing.interact_size.y + 2.0 * spacing.button_padding.y;
    let margin = egui::Frame::side_top_panel(ui.style())
        .total_margin()
        .sum()
        .y;
    button + 2.0 * FOOTER_PADDING + margin
}

/// Draws the foot of the panel: the control that discards every change the user has made.
///
/// It sits in the bottom-right corner of the panel, away from the property rows, because
/// it throws away every change at once rather than editing one of them. The control is
/// centred in the strip [`footer_height`] gives the foot, and its label is truncated
/// rather than wrapped, so that a narrow panel or a large number of changes shortens what
/// the foot says instead of making the foot taller. Returns whether the displayed figure
/// changed.
fn footer(ui: &mut egui::Ui, state: &mut FigureState) -> bool {
    let changes = state.change_count();
    let clicked = ui
        .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled(
                changes > 0,
                egui::Button::new(revert_all_label(changes)).truncate(),
            )
            .on_hover_text(REVERT_ALL_HINT)
            .on_disabled_hover_text(REVERT_ALL_EMPTY_HINT)
            .clicked()
        })
        .inner;
    clicked && state.revert_all()
}

// ---------------------------------------------------------------------------------
// The object tree
// ---------------------------------------------------------------------------------

/// Draws the figure, its axes and their artists as a collapsible tree, and selects the
/// node whose row is clicked.
fn object_tree(ui: &mut egui::Ui, state: &mut FigureState) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Objects").strong());
    let rows = tree_rows(state.figure());
    let selection = state.selection();
    let clicked: Cell<Option<NodeId>> = Cell::new(None);
    egui::ScrollArea::vertical()
        .id_salt("ironlab_object_tree_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut index = 0;
            while index < rows.len() {
                index = subtree(ui, &rows, index, selection, &clicked);
            }
        });
    if let Some(node) = clicked.get() {
        state.select(Some(node));
    }
}

/// Draws the row at `index` and the rows below it, and returns the index after them.
fn subtree(
    ui: &mut egui::Ui,
    rows: &[TreeRow],
    index: usize,
    selection: Option<NodeId>,
    clicked: &Cell<Option<NodeId>>,
) -> usize {
    let row = &rows[index];
    let end = rows[index + 1..]
        .iter()
        .position(|other| other.depth <= row.depth)
        .map_or(rows.len(), |offset| index + 1 + offset);
    if index + 1 == end {
        tree_label(ui, row, selection, clicked);
        return index + 1;
    }
    let id = ui.make_persistent_id(("ironlab_tree", row.node.0));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
        .show_header(ui, |ui| tree_label(ui, row, selection, clicked))
        .body(|ui| {
            let mut next = index + 1;
            while next < end {
                next = subtree(ui, rows, next, selection, clicked);
            }
        });
    end
}

/// Draws one selectable row of the tree.
fn tree_label(
    ui: &mut egui::Ui,
    row: &TreeRow,
    selection: Option<NodeId>,
    clicked: &Cell<Option<NodeId>>,
) {
    let mut text = egui::RichText::new(&row.label);
    if row.dimmed {
        text = text.color(ui.visuals().weak_text_color());
    }
    let response = ui.selectable_label(selection == Some(row.node), text);
    let response = if row.dimmed {
        response.on_hover_text("This plot is hidden.")
    } else {
        response
    };
    if response.clicked() {
        clicked.set(Some(row.node));
    }
}

// ---------------------------------------------------------------------------------
// The inspector
// ---------------------------------------------------------------------------------

/// Draws the properties of the selected node, and returns whether the figure changed.
fn inspector(ui: &mut egui::Ui, panel: &mut PropertyPanel, state: &mut FigureState) -> bool {
    let Some(node) = state.selection() else {
        ui.label("Select an object to see its properties.");
        return false;
    };
    let Some(kind) = state.figure().node_kind(node) else {
        ui.label("The selected object is no longer in the figure.");
        return false;
    };
    ui.add_space(4.0);
    ui.label(egui::RichText::new(format!("{} — {node}", kind_name(kind))).strong());
    let groups = property_groups(state.figure(), state.overlay(), node);
    let mut changed = false;
    egui::ScrollArea::vertical()
        .id_salt("ironlab_inspector_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for group in &groups {
                changed |= property_group(ui, panel, state, node, group);
            }
        });
    changed
}

/// Draws one group of properties, and returns whether the figure changed.
fn property_group(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
    node: NodeId,
    group: &PropertyGroup,
) -> bool {
    ui.add_space(4.0);
    // A group whose own value is a row is named by that row; one that is only a
    // container (an axis, a line style) is named by a heading of its own. The heading is
    // drawn at the size of the body text, because it is read as much as the rows are,
    // and at the left edge of the rows, where a property that belongs to no group is
    // drawn, so that the properties it gathers are visibly indented beneath it.
    if group.rows.first().is_none_or(|row| !row.label.is_empty()) {
        ui.label(egui::RichText::new(&group.name).strong());
    }
    let mut changed = false;
    for row in &group.rows {
        changed |= property_row(ui, panel, state, node, group, row);
    }
    changed
}

/// Draws one property, and returns whether the figure changed.
///
/// Every row is laid out in the same three columns, so that the panel can be read down
/// them: the name of the property at the left, indented by how deeply the property is
/// nested; the control at the right of the column between them, so that the controls of a
/// node line up with one another; and the control that takes back a change to the
/// property in a column of its own at the right, which is reserved whether or not this
/// row is overridden.
fn property_row(
    ui: &mut egui::Ui,
    panel: &mut PropertyPanel,
    state: &mut FigureState,
    node: NodeId,
    group: &PropertyGroup,
    row: &PropertyRow,
) -> bool {
    let name = if row.label.is_empty() {
        group.name.clone()
    } else {
        row.label.clone()
    };
    if matches!(row.editor, Editor::Parameters) {
        // The parameters are edited in a table of their own below the row that names
        // them, which is too wide for the column of controls. The row itself keeps the
        // columns of every other row, so that the control that takes the parameters back
        // stands where every other revert control stands.
        let mut changed = ui
            .horizontal(|ui| {
                let room = ui.available_width();
                name_column(ui, row, &name, 0.0);
                column(
                    ui,
                    control_width(ui, room),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |_| (),
                );
                revert_column(ui, state, node, row, &name)
            })
            .inner;
        changed |= parameters_editor(ui, panel, state);
        return changed;
    }
    ui.horizontal(|ui| {
        let indent = INDENT * row.depth as f32;
        let room = ui.available_width();
        name_column(ui, row, &name, indent);
        let mut changed = column(
            ui,
            control_width(ui, room),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
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
                        read_only_control(ui, row, text, DATA_REASON);
                        false
                    }
                    Editor::ReadOnly { reason } => {
                        read_only_control(ui, row, read_only_label(&row.value), reason);
                        false
                    }
                    Editor::Group | Editor::Parameters => false,
                }
            },
        );
        changed |= revert_column(ui, state, node, row, &name);
        changed
    })
    .inner
}

/// The width of the column of controls in a row that was given `room` points, which is
/// what the names and the revert control leave. A panel too narrow to give every column
/// its share takes the room from the controls rather than from the names.
fn control_width(ui: &egui::Ui, room: f32) -> f32 {
    let spacing = 2.0 * ui.spacing().item_spacing.x;
    (room - NAME_WIDTH - REVERT_WIDTH - spacing).max(CONTROL_MIN_WIDTH)
}

/// Draws one column of a property row, `width` points wide, and leaves the cursor at the
/// far edge of the column whatever the column holds, so that the next column of every row
/// begins at the same place.
///
/// A control too wide for its column overflows towards the edge the column's layout
/// starts from, which for the right-justified column of controls is its own right edge,
/// so that even an overflowing control leaves the column beyond it where it was.
fn column<R>(
    ui: &mut egui::Ui,
    width: f32,
    layout: egui::Layout,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let start = ui.cursor().left();
    let height = ui.spacing().interact_size.y;
    let inner = ui
        .allocate_ui_with_layout(egui::vec2(width, height), layout, add)
        .inner;
    let short = start + width + ui.spacing().item_spacing.x - ui.cursor().left();
    if short > 0.0 {
        ui.add_space(short);
    }
    inner
}

/// Draws the left column of a property row: the name of the property, justified to the
/// left edge of the level it belongs to, in bold when the overlay overrides it, with its
/// documentation as a tooltip.
///
/// The column is the same width whatever the indent, so that a nested property moves to
/// the right without moving the control beside it.
fn name_column(ui: &mut egui::Ui, row: &PropertyRow, name: &str, indent: f32) {
    column(
        ui,
        NAME_WIDTH,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add_space(indent.min(NAME_WIDTH - NAME_MIN_WIDTH));
            let mut text = egui::RichText::new(name);
            if row.overridden {
                text = text.strong();
            }
            ui.add(egui::Label::new(text).truncate())
                .on_hover_text(row.docs);
        },
    );
}

/// Draws the right-hand column of a property row, which holds the control that takes back
/// the user's change to the property when there is one to take back, and is left empty
/// but reserved when there is not.
fn revert_column(
    ui: &mut egui::Ui,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
    name: &str,
) -> bool {
    column(
        ui,
        REVERT_WIDTH,
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| row.overridden && revert_button(ui, state, node, &row.path, name),
    )
}

/// Draws the control that takes back the user's change to one property.
fn revert_button(
    ui: &mut egui::Ui,
    state: &mut FigureState,
    node: NodeId,
    path: &PropertyPath,
    name: &str,
) -> bool {
    let label = format!("Revert {name}");
    let enabled = ui.is_enabled();
    let response = ui.small_button(crate::style::RESTORE);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label.clone())
    });
    let response = response.on_hover_text(format!(
        "Take back your change to {name} and show the figure's own value again."
    ));
    response.clicked() && state.revert(node, path)
}

/// Commits a new value of a property, and returns whether the figure changed.
fn set(state: &mut FigureState, node: NodeId, path: &PropertyPath, value: Value) -> bool {
    let transaction = commit(state.figure(), node, path, value);
    state.try_record(&transaction)
}

/// Draws the control of a boolean property: a checkbox with no label of its own, so that
/// it stands in the column of controls with the numeric fields and combo boxes rather
/// than at the left of its row. The name of the property it changes is given to the
/// accessibility tree in its place, so that the control is still named where it is read
/// rather than seen.
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
    let enabled = ui.is_enabled();
    let response = ui.checkbox(&mut flag, "");
    let label = name.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, flag, label.clone())
    });
    let response = response.on_hover_text(row.docs);
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
    let mut number = current;
    let mut widget = egui::DragValue::new(&mut number).speed(speed);
    if let Some((low, high)) = range {
        widget = widget.range(low..=high);
    }
    let response = ui.add(widget).on_hover_text(row.docs);
    panel.hold(
        state,
        response.id,
        response.dragged() || response.has_focus(),
    );
    if !response.changed() {
        return false;
    }
    let value = match &row.value {
        Value::Float(_) => Value::Float(number as f32),
        Value::UInt32(_) => Value::UInt32(number.clamp(0.0, f64::from(u32::MAX)) as u32),
        _ if integer => Value::Double(number.round()),
        _ => Value::Double(number),
    };
    set(state, node, &row.path, value)
}

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
    egui::ComboBox::from_id_salt(("ironlab_choice", row.path.to_string()))
        .selected_text(current)
        .show_ui(ui, |ui| {
            for choice in offered {
                // A choice the IR would not act on here is shown disabled, with the
                // reason on hover, rather than left out: the user sees that the value
                // exists and reads what would make it available.
                if let Some(reason) = choice.unavailable {
                    ui.add_enabled(false, egui::Button::selectable(false, choice.label))
                        .on_disabled_hover_text(reason);
                } else if ui
                    .selectable_label(choice.label == current, choice.label)
                    .clicked()
                    && choice.label != current
                {
                    chosen = Some(choice.value.clone());
                }
            }
        });
    match chosen {
        Some(value) => set(state, node, &row.path, value),
        None => false,
    }
}

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
    let response = egui::color_picker::color_edit_button_srgba(
        ui,
        &mut rgba,
        egui::color_picker::Alpha::OnlyBlend,
    )
    .on_hover_text(row.docs);
    panel.hold(state, response.id, response.has_focus());
    let chosen = from_color32(rgba);
    chosen != current && set(state, node, &row.path, Value::Color(chosen))
}

/// Draws the control of a text property: the source, and for a rich text the interpreter
/// that reads it.
///
/// The column of controls is filled from its right edge, so the interpreter is drawn
/// before the field it belongs to and the field takes whatever room is left.
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
    let mut changed = false;
    let mut chosen = None;
    if let Some(interpreter) = interpreter {
        let offered = choices(ValueType::Interpreter, None);
        let current = offered
            .iter()
            .find(|choice| choice.matches(&Value::Interpreter(interpreter)))
            .map_or("…", |choice| choice.label);
        egui::ComboBox::from_id_salt(("ironlab_interpreter", row.path.to_string()))
            .selected_text(current)
            .width(64.0)
            .show_ui(ui, |ui| {
                for choice in &offered {
                    if ui
                        .selectable_label(choice.label == current, choice.label)
                        .clicked()
                        && let Value::Interpreter(picked) = choice.value
                    {
                        chosen = Some(picked);
                    }
                }
            });
    }
    let response = ui
        .add(
            egui::TextEdit::singleline(&mut content)
                .desired_width(ui.available_width())
                .hint_text("empty"),
        )
        .on_hover_text(row.docs);
    panel.hold(state, response.id, response.has_focus());
    if response.changed() {
        let value = match interpreter {
            Some(interpreter) => Value::Text(Text {
                content: content.clone(),
                interpreter,
            }),
            None => Value::String(content.clone()),
        };
        changed |= set(state, node, &row.path, value);
    }
    if let Some(picked) = chosen
        && Some(picked) != interpreter
    {
        changed |= set(
            state,
            node,
            &row.path,
            Value::Text(Text {
                content,
                interpreter: picked,
            }),
        );
    }
    changed
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
    let mut text = current
        .iter()
        .map(f64::to_string)
        .collect::<Vec<String>>()
        .join(", ");
    let response = ui
        .add(egui::TextEdit::singleline(&mut text).desired_width(ui.available_width()))
        .on_hover_text(format!("{} Separate the numbers with commas.", row.docs));
    panel.hold(state, response.id, response.has_focus());
    if !response.changed() {
        return false;
    }
    let parsed: Result<Vec<f64>, _> = text
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

/// Draws the control of a list of words: a text field holding them separated by commas.
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
    let mut text = current.join(", ");
    let response = ui
        .add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(ui.available_width())
                .hint_text("empty"),
        )
        .on_hover_text(format!("{} Separate the words with commas.", row.docs));
    panel.hold(state, response.id, response.has_focus());
    if !response.changed() {
        return false;
    }
    let words: Vec<String> = text
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    words != *current && set(state, node, &row.path, Value::Strings(words))
}

/// Draws a property that the panel shows but cannot change: its value, drawn dimmed, with
/// what the property means and the reason it cannot be changed here in its tooltip.
///
/// Every read-only property is drawn here, whichever kind it is, so that the rule holds in
/// one place: the value dimmed, the reason on hover, and nothing else. Nothing is drawn
/// beside the value — no lock, no badge — because a mark that says only "this cannot be
/// changed" says less than the dimmed value does and costs a glyph the fonts may not have.
fn read_only_control(ui: &mut egui::Ui, row: &PropertyRow, text: String, reason: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).weak()).truncate())
        .on_hover_text(format!("{} {reason}", row.docs));
}

/// The value of a reference to a data array, written as the array it names and that
/// array's shape.
fn data_label(row: &PropertyRow, shape: Option<&[usize]>) -> String {
    match &row.value {
        Value::DataId(id) => format!("{id} {}", shape_label(shape)),
        _ => shape_label(shape),
    }
}

/// Draws an optional value that is absent, with a control that gives it the default of
/// its type; a data reference has no default to give, so it is only reported.
///
/// The column of controls is filled from its right edge, so the control that gives the
/// value one is drawn before the word that says there is none.
fn unset_control(
    ui: &mut egui::Ui,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
    name: &str,
) -> bool {
    let changed = match default_value(row.value_type) {
        None => false,
        Some(value) => {
            ui.small_button("Set")
                .on_hover_text(format!("Give {name} a value."))
                .clicked()
                && set(state, node, &row.path, value)
        }
    };
    ui.label(egui::RichText::new("unset").weak());
    changed
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
/// The entries are edited one at a time but committed as one set of the whole map, so
/// that a change is one entry of the overlay and one step of the history. A draft that
/// cannot be committed (an entry with no name, two entries with one name, a number that
/// is not one) is kept and its reason is shown, so that typing is never interrupted.
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
    ui.vertical(|ui| {
        egui::Grid::new("ironlab_parameters")
            .num_columns(4)
            .spacing([6.0, 4.0])
            .show(ui, |ui| {
                for (index, row) in draft.rows_mut().iter_mut().enumerate() {
                    let name = ui.add(
                        egui::TextEdit::singleline(&mut row.name)
                            .desired_width(80.0)
                            .hint_text("name"),
                    );
                    if name.has_focus() {
                        active = Some(name.id);
                    }
                    egui::ComboBox::from_id_salt(("ironlab_parameter_kind", index))
                        .selected_text(row.kind.label())
                        .width(96.0)
                        .show_ui(ui, |ui| {
                            for kind in ParameterKind::ALL {
                                ui.selectable_value(&mut row.kind, kind, kind.label());
                            }
                        });
                    if row.kind == ParameterKind::Bool {
                        ui.checkbox(&mut row.flag, "");
                    } else {
                        let value = ui.add(
                            egui::TextEdit::singleline(&mut row.text)
                                .desired_width(80.0)
                                .hint_text("value"),
                        );
                        if value.has_focus() {
                            active = Some(value.id);
                        }
                    }
                    if ui
                        .small_button("Remove")
                        .on_hover_text("Remove this parameter.")
                        .clicked()
                    {
                        remove = Some(index);
                    }
                    ui.end_row();
                }
            });
        if ui
            .button("Add parameter")
            .on_hover_text("Add a named value that describes the figure.")
            .clicked()
        {
            draft.add();
        }
        if let Some(problem) = &panel.parameters_problem {
            ui.label(egui::RichText::new(problem).color(ui.visuals().error_fg_color));
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
        Err(problem) => panel.parameters_problem = Some(problem),
    }
    panel.parameters = Some(draft);
    changed
}
