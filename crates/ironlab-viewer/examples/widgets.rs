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
            });
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
            .with_inner_size([380.0, 900.0]),
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
