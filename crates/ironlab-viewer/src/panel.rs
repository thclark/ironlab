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

use std::cell::Cell;
use std::collections::BTreeMap;

use ironlab_ir::{
    Axis, Cell as IrCell, Color, FigureSize, Legend, LineStyle, MarkerStyle, NodeId, Parameter,
    PropertyPath, Text, TileLayout, Value, ValueType, View3d, choices,
};

use crate::inspector::{
    Editor, ParameterKind, ParametersDraft, PropertyGroup, PropertyRow, TreeRow, commit, kind_name,
    property_groups, shape_label, tree_rows,
};
use crate::interaction::FigureState;

/// The width of the column of property names, in egui points.
const LABEL_WIDTH: f32 = 96.0;

/// The indent of each level of nesting, in egui points.
const INDENT: f32 = 10.0;

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
            egui::Panel::top("ironlab_object_tree")
                .resizable(true)
                .default_size(180.0)
                .show(ui, |ui| object_tree(ui, state));
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
    // container (an axis, a line style) is named by a heading of its own.
    if group.rows.first().is_none_or(|row| !row.label.is_empty()) {
        ui.label(egui::RichText::new(&group.name).strong().small());
    }
    let mut changed = false;
    for row in &group.rows {
        changed |= property_row(ui, panel, state, node, group, row);
    }
    changed
}

/// Draws one property, and returns whether the figure changed.
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
        let mut changed = ui
            .horizontal(|ui| {
                name_label(ui, row, &name, 0.0);
                row.overridden && revert_button(ui, state, node, &row.path, &name)
            })
            .inner;
        changed |= parameters_editor(ui, panel, state);
        return changed;
    }
    ui.horizontal(|ui| {
        let indent = INDENT * row.depth as f32;
        ui.add_space(indent);
        let mut changed = if row.value == Value::Unset {
            unset_row(ui, state, node, row, &name, indent)
        } else {
            match &row.editor {
                Editor::Bool => bool_row(ui, panel, state, node, row, &name),
                Editor::Number {
                    speed,
                    range,
                    integer,
                } => {
                    name_label(ui, row, &name, indent);
                    number_row(ui, panel, state, node, row, *speed, *range, *integer)
                }
                Editor::Choice => {
                    name_label(ui, row, &name, indent);
                    choice_row(ui, state, node, row)
                }
                Editor::Color => {
                    name_label(ui, row, &name, indent);
                    color_row(ui, panel, state, node, row)
                }
                Editor::Text | Editor::RichText => {
                    name_label(ui, row, &name, indent);
                    text_row(ui, panel, state, node, row)
                }
                Editor::Numbers => {
                    name_label(ui, row, &name, indent);
                    numbers_row(ui, panel, state, node, row)
                }
                Editor::Data { shape } => {
                    name_label(ui, row, &name, indent);
                    data_row(ui, row, shape.as_deref());
                    false
                }
                Editor::Group | Editor::ReadOnly | Editor::Parameters => {
                    name_label(ui, row, &name, indent);
                    false
                }
            }
        };
        if row.overridden {
            changed |= revert_button(ui, state, node, &row.path, &name);
        }
        changed
    })
    .inner
}

/// Draws the name of a property, in bold when the overlay overrides it, with its
/// documentation as a tooltip.
fn name_label(ui: &mut egui::Ui, row: &PropertyRow, name: &str, indent: f32) {
    let mut text = egui::RichText::new(name);
    if row.overridden {
        text = text.strong();
    }
    ui.add_sized(
        [
            (LABEL_WIDTH - indent).max(24.0),
            ui.spacing().interact_size.y,
        ],
        egui::Label::new(text).truncate(),
    )
    .on_hover_text(row.docs);
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
    let response = ui.small_button("↺");
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

fn bool_row(
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
    let mut text = egui::RichText::new(name);
    if row.overridden {
        text = text.strong();
    }
    let response = ui.checkbox(&mut flag, text).on_hover_text(row.docs);
    panel.hold(state, response.id, response.has_focus());
    response.changed() && set(state, node, &row.path, Value::Bool(flag))
}

#[allow(clippy::too_many_arguments)]
fn number_row(
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

fn choice_row(ui: &mut egui::Ui, state: &mut FigureState, node: NodeId, row: &PropertyRow) -> bool {
    let offered = choices(row.value_type, Some(&row.value));
    let current = offered
        .iter()
        .find(|choice| choice.matches(&row.value))
        .map_or("…", |choice| choice.label);
    let mut chosen: Option<Value> = None;
    egui::ComboBox::from_id_salt(("ironlab_choice", row.path.to_string()))
        .selected_text(current)
        .show_ui(ui, |ui| {
            for choice in &offered {
                if ui
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

fn color_row(
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

fn text_row(
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
    let response = ui
        .add(
            egui::TextEdit::singleline(&mut content)
                .desired_width(110.0)
                .hint_text("empty"),
        )
        .on_hover_text(row.docs);
    panel.hold(state, response.id, response.has_focus());
    let mut changed = false;
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
    if let Some(interpreter) = interpreter {
        let offered = choices(ValueType::Interpreter, None);
        let current = offered
            .iter()
            .find(|choice| choice.matches(&Value::Interpreter(interpreter)))
            .map_or("…", |choice| choice.label);
        let mut chosen = None;
        egui::ComboBox::from_id_salt(("ironlab_interpreter", row.path.to_string()))
            .selected_text(current)
            .width(72.0)
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
        if let Some(picked) = chosen
            && picked != interpreter
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
    }
    changed
}

fn numbers_row(
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
        .add(egui::TextEdit::singleline(&mut text).desired_width(130.0))
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

/// Draws a reference to a data array, read-only, with the shape of what it refers to.
fn data_row(ui: &mut egui::Ui, row: &PropertyRow, shape: Option<&[usize]>) {
    let text = match &row.value {
        Value::DataId(id) => format!("{id} {}", shape_label(shape)),
        _ => shape_label(shape),
    };
    ui.label(egui::RichText::new(text).weak()).on_hover_text(
        "Data is shown but not edited here: a plot that refers to an array of the wrong \
         shape cannot be drawn. Change it through the API that owns the figure.",
    );
}

/// Draws an optional value that is absent, with a control that gives it the default of
/// its type; a data reference has no default to give, so it is only reported.
fn unset_row(
    ui: &mut egui::Ui,
    state: &mut FigureState,
    node: NodeId,
    row: &PropertyRow,
    name: &str,
    indent: f32,
) -> bool {
    name_label(ui, row, name, indent);
    ui.label(egui::RichText::new("unset").weak());
    match default_value(row.value_type) {
        None => false,
        Some(value) => {
            ui.small_button("Set")
                .on_hover_text(format!("Give {name} a value."))
                .clicked()
                && set(state, node, &row.path, value)
        }
    }
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
                        .small_button("✖")
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
