//! A temporary window showing every widget of the proposed `widgets` module, to be hovered and clicked.
//!
//! Run with `cargo run -p ironlab-viewer --example widgets`. Nothing here is part of the viewer.

use ironlab_viewer::style;
use ironlab_viewer::widgets::*;

#[derive(Default)]
struct Gallery {
    search: String,
    typed: String,
    descending: bool,
    open: bool,
    ticked: [bool; 3],
    tool: usize,
    chips: Vec<(String, String, bool)>,
    selected_row: usize,
    closed: bool,
    editor: Editor,
}

/// The state of the property editor's section: a tree, the properties of the node chosen in it, and the
/// parameters of the figure.
struct Editor {
    node: usize,
    figure_open: bool,
    axes_open: bool,
    marker_open: bool,
    visible: bool,
    size_pt: f64,
    rows: f64,
    scale: usize,
    stroke: egui::Color32,
    title: String,
    interpreter: usize,
    labels: String,
    levels: String,
    legend_set: bool,
    restored: Vec<&'static str>,
    parameters: Vec<Parameter>,
}

/// One entry of the parameters table.
struct Parameter {
    name: String,
    kind: usize,
    value: String,
    flag: bool,
}

/// The kinds a parameter may take, named as a scientist names them.
const KINDS: [&str; 4] = ["string", "integer", "number", "boolean"];

impl Default for Editor {
    fn default() -> Self {
        Self {
            node: 2,
            figure_open: true,
            axes_open: true,
            marker_open: true,
            visible: true,
            size_pt: 6.0,
            rows: 2.0,
            scale: 0,
            stroke: egui::Color32::from_rgb(31, 119, 180),
            title: "Lift against angle".to_owned(),
            interpreter: 1,
            labels: "wake, piv".to_owned(),
            levels: "0.5, 1, 2, 4".to_owned(),
            legend_set: false,
            restored: Vec::new(),
            parameters: vec![
                Parameter {
                    name: "rig".to_owned(),
                    kind: 0,
                    value: "Tunnel A".to_owned(),
                    flag: false,
                },
                Parameter {
                    name: "angle".to_owned(),
                    kind: 2,
                    value: "12.0".to_owned(),
                    flag: false,
                },
                Parameter {
                    name: "stalled".to_owned(),
                    kind: 3,
                    value: String::new(),
                    flag: true,
                },
                Parameter {
                    name: "angle".to_owned(),
                    kind: 1,
                    value: "3".to_owned(),
                    flag: false,
                },
            ],
        }
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(12.0);
    label(ui, text(Role::Label, title));
    ui.add_space(4.0);
}

impl eframe::App for Gallery {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ui.visuals().panel_fill))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.allocate_ui(egui::vec2(356.0, 2000.0), |ui| self.gallery(ui));
                });
            });
    }
}

impl Gallery {
    fn gallery(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(Spacing::GAP, Spacing::GAP);
        ui.spacing_mut().button_padding = Spacing::CONTROL_PADDING;
        ui.spacing_mut().interact_size.y = Spacing::CONTROL_HEIGHT;
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Button, Role::Control.font());

        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                section(ui, "Type: four roles, three sizes");
                label(
                    ui,
                    text(Role::Body, "Body 14: a title in the list, a value"),
                );
                label(
                    ui,
                    text(Role::Control, "Control 12.5: buttons, combo boxes, notes"),
                );
                label(ui, text(Role::Label, "Label 11: sections and headings"));
                label(
                    ui,
                    text(Role::Data, "Data 11: counts, names, labels, values"),
                );

                section(ui, "Icons: one slot, one stroke");
                ui.horizontal(|ui| {
                    for icon in [
                        Icon::Plus,
                        Icon::Minus,
                        Icon::Cross,
                        Icon::Restore,
                        Icon::TriangleDown,
                        Icon::TriangleUp,
                        Icon::TriangleRight,
                        Icon::Tick,
                    ] {
                        Control::new(text(Role::Control, ""), Face::Quiet)
                            .before(icon)
                            .show(ui);
                    }
                });

                section(ui, "Controls and their states");
                ui.horizontal_wrapped(|ui| {
                    Control::button("Done").show(ui);
                    if Self::add_button(self.open).show(ui).clicked() {
                        self.open = !self.open;
                    }
                    if Control::button("Clear all")
                        .before(Icon::Restore)
                        .show(ui)
                        .clicked()
                    {
                        self.chips.clear();
                    }
                    Control::button("Back to all parameters").quiet().show(ui);
                    Control::button("12 more…").quiet().show(ui);
                });

                section(ui, "The toolbar: large controls");
                ui.horizontal_wrapped(|ui| {
                    for (index, tool) in ["Pan", "Zoom"].iter().enumerate() {
                        if Control::button(tool)
                            .large()
                            .selected(self.tool == index)
                            .show(ui)
                            .clicked()
                        {
                            self.tool = index;
                        }
                    }
                    Control::button("Rotate").large().enabled(false).show(ui);
                    ui.separator();
                    Control::button("Undo").large().enabled(false).show(ui);
                    Control::button("Redo").large().enabled(false).show(ui);
                    Control::button("Reset").large().show(ui);
                    Control::button("Export PDF…").large().show(ui);
                });

                section(ui, "Captioned rows");
                captioned_row(ui, "Sort", |ui| {
                    combo(ui, "gallery_sort", "Title", 150.0, |ui| {
                        ui.label("Title");
                        ui.label("rig");
                    });
                    let icon = if self.descending {
                        Icon::TriangleDown
                    } else {
                        Icon::TriangleUp
                    };
                    let word = if self.descending {
                        "Descending"
                    } else {
                        "Ascending"
                    };
                    if Control::button(word).after(icon).show(ui).clicked() {
                        self.descending = !self.descending;
                    }
                });
                captioned_row(ui, "Group", |ui| {
                    combo(ui, "gallery_group", "None", 150.0, |ui| {
                        ui.label("None");
                    });
                });
                captioned_row(ui, "Filters", |ui| {
                    if Self::add_button(self.open).show(ui).clicked() {
                        self.open = !self.open;
                    }
                    if !self.chips.is_empty()
                        && Control::button("Clear all")
                            .before(Icon::Restore)
                            .show(ui)
                            .clicked()
                    {
                        self.chips.clear();
                    }
                });

                section(ui, "Chips and tags: click a chip to remove it");
                ui.horizontal_wrapped(|ui| {
                    let mut remove = None;
                    for (index, (name, value, labels)) in self.chips.iter().enumerate() {
                        let tint = if *labels {
                            Tint::LABEL
                        } else {
                            Tint::PARAMETER
                        };
                        if Control::chip(name, value, tint).show(ui).clicked() {
                            remove = Some(index);
                        }
                    }
                    if let Some(index) = remove {
                        self.chips.remove(index);
                    }
                    if self.chips.is_empty()
                        && Control::button("Add the chips back")
                            .quiet()
                            .show(ui)
                            .clicked()
                    {
                        self.chips = Self::default_chips();
                    }
                    for tag in ["line", "legend", "basics"] {
                        Control::tag(tag).show(ui);
                    }
                });

                section(ui, "The field");
                field(
                    ui,
                    &mut self.search,
                    "Search, or rig:CFD angle>=8 -stalled",
                    egui::Id::new("g_search"),
                );
                field(ui, &mut self.typed, "Filter on…", egui::Id::new("g_filter"));

                section(ui, "The menu well");
            });
        well(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let one = Row::height(ui, false);
            for (index, (name, detail)) in [
                ("labels", "30 values · 100%"),
                ("rig", "3 values · 100%"),
                ("data_points", "25 values · 100%"),
            ]
            .iter()
            .enumerate()
            {
                let row =
                    Row::new(text(Role::Body, name), Detail::Trailing(detail)).state(RowState {
                        selected: self.selected_row == index,
                        ..Default::default()
                    });
                if row.show(ui, one).clicked() {
                    self.selected_row = index;
                }
            }
            ui.add_space(Spacing::GAP);
            for (index, (value, count)) in [("Tunnel A", "8"), ("Tunnel B", "6"), ("CFD", "0")]
                .iter()
                .enumerate()
            {
                let ticked = self.ticked[index];
                let row = Row::new(text(Role::Body, value), Detail::Trailing(count))
                    .leading(Leading::Check { ticked })
                    .state(RowState {
                        faded: !ticked && *count == "0",
                        ..Default::default()
                    });
                if row.show(ui, one).clicked() {
                    self.ticked[index] = !ticked;
                }
            }
            ui.add_space(Spacing::GAP);
            histogram(
                ui,
                &[
                    (0.0, 3),
                    (2.0, 5),
                    (4.0, 9),
                    (6.0, 4),
                    (8.0, 2),
                    (12.0, 6),
                    (16.0, 1),
                ],
                (0.0, 16.0),
                (3.0, 12.0),
            );
        });
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(10, 0))
                    .show(ui, |ui| section(ui, "The list: click a heading"));
                let two = Row::height(ui, true);
                if Row::new(text(Role::Label, "Tunnel A"), Detail::Trailing("3"))
                    .leading(Leading::Disclosure { open: !self.closed })
                    .state(RowState {
                        band: true,
                        ..Default::default()
                    })
                    .show(ui, two)
                    .clicked()
                {
                    self.closed = !self.closed;
                }
                if !self.closed {
                    Row::new(
                        text(Role::Body, "Contour"),
                        Detail::Beneath("contour, basics, levels"),
                    )
                    .show(ui, two);
                    Row::new(
                        text(Role::Body, "Correlation peak in 3D"),
                        Detail::Beneath("image, colormap, depth"),
                    )
                    .state(RowState {
                        striped: true,
                        ..Default::default()
                    })
                    .show(ui, two);
                    Row::new(
                        text(Role::Body, "Lines and markers"),
                        Detail::Beneath("line, legend, basics, dashes"),
                    )
                    .state(RowState {
                        selected: true,
                        ..Default::default()
                    })
                    .show(ui, two);
                }
                Row::new(text(Role::Label, "Tunnel B"), Detail::Trailing("2"))
                    .leading(Leading::Disclosure { open: false })
                    .state(RowState {
                        band: true,
                        ..Default::default()
                    })
                    .show(ui, two);
                note(
                    ui,
                    "No figure matches",
                    "Loosen a filter, or clear them all.",
                );
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(10, 0))
                    .show(ui, |ui| {
                        section(ui, "Details");
                        egui::Grid::new("g_details")
                            .num_columns(2)
                            .spacing([Spacing::GAP, 4.0])
                            .show(ui, |ui| {
                                label(ui, text(Role::Data, "artists"));
                                label(ui, text(Role::Body, "3"));
                                ui.end_row();
                                label(ui, text(Role::Data, "data_points"));
                                label(ui, text(Role::Body, "294"));
                                ui.end_row();
                            });
                        ui.add_space(20.0);
                    });
                self.editor(ui);
            });
    }

    /// The property editor: a tree of objects, the properties of the object chosen, the parameters of the figure
    /// and the foot that takes every change back.
    fn editor(&mut self, ui: &mut egui::Ui) {
        let editor = &mut self.editor;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| section(ui, "The property editor: the tree"));
        heading(ui, "Objects", None);
        let one = Row::height(ui, false);
        let tree = [
            (
                "Figure (Lift against angle)",
                0,
                Some(editor.figure_open),
                false,
            ),
            ("Axes (Speed)", 1, Some(editor.axes_open), false),
            ("Line (measured)", 2, None, false),
            ("Scatter (peaks)", 2, None, true),
            ("Axes (row 0, col 1)", 1, Some(false), false),
        ];
        for (index, (name, depth, open, hidden)) in tree.iter().enumerate() {
            let leading = match open {
                Some(open) => Leading::Disclosure { open: *open },
                None => Leading::None,
            };
            let row = Row::new(text(Role::Body, name), Detail::None)
                .leading(leading)
                .indent(*depth)
                .state(RowState {
                    selected: editor.node == index,
                    dimmed: *hidden,
                    ..RowState::default()
                })
                .show(ui, one);
            let row = if *hidden {
                hint(row, "This plot is hidden.")
            } else {
                row
            };
            if row.clicked() {
                editor.node = index;
                match index {
                    0 => editor.figure_open = !editor.figure_open,
                    1 => editor.axes_open = !editor.axes_open,
                    _ => {}
                }
            }
        }

        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| section(ui, "The property editor: the rows"));
        heading(ui, "Properties", Some("Line (measured)"));
        let marker = Property::new("marker")
            .disclosure(editor.marker_open)
            .show(ui, |_| ());
        if marker.name.clicked() {
            editor.marker_open = !editor.marker_open;
        }
        let mut stripe = 0;
        let mut striped = || {
            stripe += 1;
            stripe % 2 == 0
        };
        let changed = |editor: &Editor, name: &str| !editor.restored.contains(&name);
        let mut restore = None;
        // The restored list is what the reader has clicked back, so that a row's restore control is seen to
        // go away and its name to lose its colour.
        // The rows the marker gathers are drawn only while it is open.
        if editor.marker_open {
            const DOCS: &str =
                "Whether the plot is drawn. A hidden plot keeps its place in the legend, greyed.";
            let response = Property::new("visible")
                .depth(1)
                .docs(DOCS)
                .changed(changed(editor, "visible"))
                .striped(striped())
                .show(ui, |ui| {
                    checkbox(ui, &mut editor.visible, "visible");
                });
            if response.restore {
                restore = Some("visible");
            }
            let response = Property::new("size_pt")
                .depth(1)
                .docs("The size of a marker, in points.")
                .changed(changed(editor, "size_pt"))
                .striped(striped())
                .show(ui, |ui| {
                    number(
                        ui,
                        &mut editor.size_pt,
                        Number::real(0.1),
                        egui::Id::new("g_size_pt"),
                    );
                });
            if response.restore {
                restore = Some("size_pt");
            }
        }
        Property::new("rows")
            .docs("How many rows of tiles the figure is divided into.")
            .striped(striped())
            .show(ui, |ui| {
                number(
                    ui,
                    &mut editor.rows,
                    Number::integer().range(1.0, 64.0),
                    egui::Id::new("g_rows"),
                );
            });
        const SCALES: [&str; 2] = ["Linear", "Logarithmic"];
        Property::new("scale")
            .docs("How the axis maps values to distance.")
            .striped(striped())
            .show(ui, |ui| {
                let width = ui.available_width();
                combo(ui, "g_scale", SCALES[editor.scale], width, |ui| {
                    for (index, name) in SCALES.iter().enumerate() {
                        let unavailable =
                            (index == 1).then_some("A logarithmic scale needs limits above zero.");
                        if choice(ui, name, editor.scale == index, unavailable).clicked() {
                            editor.scale = index;
                        }
                    }
                });
            });
        let response = Property::new("stroke")
            .depth(1)
            .docs("The colour of the line.")
            .changed(changed(editor, "stroke"))
            .striped(striped())
            .show(ui, |ui| {
                swatch(ui, &mut editor.stroke, egui::Id::new("g_stroke"));
            });
        if response.restore {
            restore = Some("stroke");
        }
        const INTERPRETERS: [&str; 2] = ["plain", "LaTeX"];
        Property::new("title")
            .docs("The title above the axes.")
            .striped(striped())
            .show(ui, |ui| {
                field(ui, &mut editor.title, "empty", egui::Id::new("g_title"));
            });
        Property::new("interpreter")
            .depth(1)
            .docs("How the source of the title is read.")
            .striped(striped())
            .show(ui, |ui| {
                let width = ui.available_width();
                combo(
                    ui,
                    "g_interpreter",
                    INTERPRETERS[editor.interpreter],
                    width,
                    |ui| {
                        for (index, name) in INTERPRETERS.iter().enumerate() {
                            if choice(ui, name, editor.interpreter == index, None).clicked() {
                                editor.interpreter = index;
                            }
                        }
                    },
                );
            });
        Property::new("labels")
            .docs("The labels of the figure, separated by commas.")
            .striped(striped())
            .show(ui, |ui| {
                field(ui, &mut editor.labels, "empty", egui::Id::new("g_labels"));
            });
        Property::new("levels")
            .docs("The values the contours are drawn at, separated by commas.")
            .striped(striped())
            .show(ui, |ui| {
                field(ui, &mut editor.levels, "empty", egui::Id::new("g_levels"));
            });
        Property::new("x")
            .docs("The x data of the line.")
            .striped(striped())
            .show(ui, |ui| {
                let response = readout(ui, text(Role::Body, "x_measured [256 × 2]"));
                hint(
                    response,
                    "The data a plot draws comes from the program that builds the figure, which is where it \
                     is changed.",
                );
            });
        Property::new("links")
            .docs("The groups of axes whose limits are linked.")
            .striped(striped())
            .show(ui, |ui| {
                let response = readout(ui, text(Role::Body, "no linked axes"));
                hint(
                    response,
                    "The groups of linked axes are set by the program that builds the figure.",
                );
            });
        Property::new("legend")
            .docs("The legend of the axes, which it need not have.")
            .striped(striped())
            .show(ui, |ui| {
                // The control that gives an absent value one stands at the right of the column, and the word
                // that says there is none fills what is left.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if editor.legend_set {
                        if Control::button("Unset").quiet().show(ui).clicked() {
                            editor.legend_set = false;
                        }
                    } else {
                        if Control::button("Set").quiet().show(ui).clicked() {
                            editor.legend_set = true;
                        }
                        readout(ui, text(Role::Body, "unset"));
                    }
                });
            });
        if let Some(name) = restore {
            editor.restored.push(name);
        }

        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| section(ui, "The parameters, and a problem"));
        block(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(Spacing::GAP, Spacing::GAP);
            let mut remove = None;
            for (index, parameter) in editor.parameters.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.scope(|ui| {
                        ui.set_max_width(84.0);
                        field(
                            ui,
                            &mut parameter.name,
                            "name",
                            egui::Id::new(("g_parameter_name", index)),
                        );
                    });
                    combo(
                        ui,
                        ("g_parameter_kind", index),
                        KINDS[parameter.kind],
                        88.0,
                        |ui| {
                            for (kind, name) in KINDS.iter().enumerate() {
                                if choice(ui, name, parameter.kind == kind, None).clicked() {
                                    parameter.kind = kind;
                                }
                            }
                        },
                    );
                    let remove_width = Spacing::restore_column();
                    let room = ui.available_width() - remove_width - Spacing::GAP;
                    ui.scope(|ui| {
                        ui.set_max_width(room);
                        if parameter.kind == 3 {
                            checkbox(ui, &mut parameter.flag, &parameter.name);
                        } else {
                            field(
                                ui,
                                &mut parameter.value,
                                "value",
                                egui::Id::new(("g_parameter_value", index)),
                            );
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if Control::new(text(Role::Control, ""), Face::Quiet)
                            .before(Icon::Cross)
                            .spoken(format!("remove {}", parameter.name))
                            .show(ui)
                            .clicked()
                        {
                            remove = Some(index);
                        }
                    });
                });
            }
            if let Some(index) = remove {
                editor.parameters.remove(index);
            }
            if Control::button("Add parameter")
                .before(Icon::Plus)
                .show(ui)
                .clicked()
            {
                editor.parameters.push(Parameter {
                    name: format!("parameter {}", editor.parameters.len() + 1),
                    kind: 0,
                    value: String::new(),
                    flag: false,
                });
            }
            let mut names: Vec<&str> = editor
                .parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect();
            names.sort_unstable();
            if let Some(twice) = names.windows(2).find(|pair| pair[0] == pair[1]) {
                problem(ui, &format!("Two parameters are named {}.", twice[0]));
            }
        });

        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| section(ui, "The foot"));
        PanelKind::Foot.frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let changes = 3 - editor.restored.len();
                let words = format!("Revert all changes ({changes})");
                let response = Control::button(&words)
                    .before(Icon::Restore)
                    .enabled(changes > 0)
                    .show(ui);
                if hint(response, "Discard every change made to this figure.").clicked() {
                    editor.restored = vec!["visible", "size_pt", "stroke"];
                }
            });
        });
        ui.add_space(20.0);
    }

    /// The control that opens the filter menu, "Add", and shuts it again, "Close": adding is what the menu is for,
    /// and a filter once added is not edited but removed by its chip.
    fn add_button(open: bool) -> Control<'static> {
        if open {
            Control::button("Close").before(Icon::Minus)
        } else {
            Control::button("Add").before(Icon::Plus)
        }
    }

    fn default_chips() -> Vec<(String, String, bool)> {
        vec![
            ("rig".into(), "CFD".into(), false),
            ("data_points".into(), "294 to 271000".into(), false),
            ("labels".into(), "surface or 3d".into(), true),
        ]
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("IronLAB widgets")
            .with_inner_size([380.0, 1100.0]),
        ..Default::default()
    };
    eframe::run_native(
        "IronLAB widgets",
        options,
        Box::new(|cc| {
            style::apply(&cc.egui_ctx);
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            Ok(Box::new(Gallery {
                typed: "rig".to_owned(),
                chips: Gallery::default_chips(),
                ..Gallery::default()
            }))
        }),
    )
}
