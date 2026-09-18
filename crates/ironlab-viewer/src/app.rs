//! The eframe viewer application.
//!
//! Each figure is shown in its own `egui_tiles` tab. A tab has a toolbar ([`toolbar`]) above a canvas that draws the
//! figure at its physical aspect ratio, scaled to fit and centred in the area below the toolbar on a neutral
//! surround. The canvas compiles the scene when the figure is first shown and recompiles it after every change to
//! the figure, so that each gesture is hit-tested against the geometry that is on screen. It converts pointer input
//! into figure-space calls on [`FigureState`], and draws the meshes from [`crate::canvas::tessellate`] with
//! `egui::Shape::mesh`, relying on 4× MSAA for anti-aliasing; the meshes are rebuilt only when the scene, the scale
//! or the position of the figure changes.
//!
//! A tab also holds the property editor of [`crate::panel`], which the "Properties" button of the toolbar opens
//! into a side panel between the toolbar and the canvas. It is hidden when a figure is opened.
//!
//! Everything shown, exported and saved is the displayed figure of [`FigureState`]: the source figure with the user's
//! overlay applied. Pressing `R` resets the view of the active figure, and ⌘Z and ⌘⇧Z (Ctrl+Z and Ctrl+Shift+Z away
//! from macOS) undo and redo its gestures. "Export PDF…" writes the displayed figure with
//! [`ironlab_pdf::write_pdf`], and "Save figure…" writes it as a figure file with [`crate::files::write_figure`],
//! after which the overlay is folded into the source because the viewer owns it. Both open a native save dialog and
//! report the outcome in a notification.

use std::sync::Arc;

use ironlab_ir::Figure;
use ironlab_scene::Scene;
use ironlab_text::TextEngine;

use crate::canvas::{ScreenTransform, color32, tessellate};
use crate::interaction::{FigureState, Tool};
use crate::panel::PropertyPanel;
use crate::problems::{Problem, indicator_label};

/// The rate at which a wheel scroll zooms: a scroll of `d` points zooms by `exp(d · rate)`, so that one notch of a
/// typical mouse wheel (50 points) zooms by about 20 %.
const WHEEL_ZOOM_RATE: f64 = 0.0036;

/// The smallest gap, in egui points, between the figure and the edges of its canvas.
const CANVAS_MARGIN: f32 = 12.0;

/// How long a notification stays on screen, in seconds.
const NOTIFICATION_SECONDS: f64 = 5.0;

/// The width of the list of problems, in egui points, which is wide enough for a sentence
/// naming a property and its reason.
const PROBLEM_LIST_WIDTH: f32 = 380.0;

/// The height beyond which the list of problems scrolls, in egui points.
const PROBLEM_LIST_MAX_HEIGHT: f32 = 320.0;

/// What the user asked for through the toolbar in one frame, beyond edits it applied to the figure state itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolbarResponse {
    /// Whether the toolbar changed the displayed figure (for example through "Reset view" or "Undo").
    pub changed: bool,
    /// Whether "Export PDF…" was clicked; the caller shows the save dialog and writes the file.
    pub export_requested: bool,
    /// Whether "Save figure…" was clicked; the caller shows the save dialog and writes the file.
    pub save_requested: bool,
}

/// Draws the toolbar of one figure tab.
///
/// The toolbar has selectable buttons labelled "Pan", "Zoom" and "Rotate" that set [`FigureState::tool`] ("Rotate" is
/// disabled when the figure has no 3D axes), "Undo" and "Redo" buttons that step through the overlay's history and
/// are disabled when there is nothing to undo or redo, a "Reset view" button that calls [`FigureState::reset_view`],
/// "Export PDF…" and "Save figure…" buttons, a "Properties" button that opens and closes the property editor through
/// `show_properties`, and, when `problems` is not empty, a problems indicator whose label contains the number of
/// problems (for example "2 problems") and which opens the list of [`problems_list`] when it is clicked.
pub fn toolbar(
    ui: &mut egui::Ui,
    state: &mut FigureState,
    problems: &[Problem],
    show_properties: &mut bool,
) -> ToolbarResponse {
    let mut response = ToolbarResponse::default();
    ui.horizontal(|ui| {
        let has_3d = state.has_3d();
        for (tool, label, hint, enabled) in [
            (
                Tool::Pan,
                "Pan",
                "Drag to pan 2D axes or move 3D axes. Scroll to zoom.",
                true,
            ),
            (
                Tool::Zoom,
                "Zoom",
                "Drag a rectangle to zoom 2D axes to it. Scroll to zoom.",
                true,
            ),
            (
                Tool::Rotate,
                "Rotate",
                "Drag to rotate 3D axes. Scroll to zoom.",
                has_3d,
            ),
        ] {
            let button = egui::Button::selectable(state.tool == tool, label);
            if ui
                .add_enabled(enabled, button)
                .on_hover_text(hint)
                .clicked()
            {
                state.tool = tool;
            }
        }
        ui.separator();
        if ui
            .add_enabled(state.can_undo(), egui::Button::new("Undo"))
            .on_hover_text("Undo the last change (⌘Z, Ctrl+Z).")
            .clicked()
        {
            response.changed |= state.undo();
        }
        if ui
            .add_enabled(state.can_redo(), egui::Button::new("Redo"))
            .on_hover_text("Redo the last undone change (⌘⇧Z, Ctrl+Shift+Z).")
            .clicked()
        {
            response.changed |= state.redo();
        }
        ui.separator();
        if ui
            .button("Reset view")
            .on_hover_text(
                "Restore the limits and 3D views of every axes (R), keeping hidden plots hidden and every property \
                 you have edited. Double-click an axes to restore only that axes. To discard every change instead, \
                 use Revert all changes at the foot of the property editor.",
            )
            .clicked()
        {
            response.changed |= state.reset_view();
        }
        if ui
            .button("Export PDF…")
            .on_hover_text("Save the figure, as currently shown, to a PDF file.")
            .clicked()
        {
            response.export_requested = true;
        }
        if ui
            .add(egui::Button::selectable(*show_properties, "Properties"))
            .on_hover_text("Show or hide the property editor, which lists the objects of the figure and their properties.")
            .clicked()
        {
            *show_properties = !*show_properties;
        }
        if ui
            .button("Save figure…")
            .on_hover_text(
                "Save the figure, as currently shown, to a .fig (Protocol Buffers) or .json file.",
            )
            .clicked()
        {
            response.save_requested = true;
        }
        if let Some(label) = indicator_label(problems) {
            ui.separator();
            problems_indicator(ui, state.figure(), problems, &label);
        }
    });
    response
}

/// Draws the problems indicator and, while it is open, the list of problems below it.
///
/// The indicator is a button rather than a label because clicking it is how the user
/// reads what is wrong: hovering gives only the invitation, and the list itself stays
/// open until the user clicks outside it or clicks the indicator again.
fn problems_indicator(ui: &mut egui::Ui, figure: &Figure, problems: &[Problem], label: &str) {
    let response = ui
        .add(
            egui::Button::new(egui::RichText::new(label).color(ui.visuals().warn_fg_color))
                .frame(false),
        )
        .on_hover_text("Show what is wrong with this figure.");
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(PROBLEM_LIST_WIDTH)
        .show(|ui| problems_list(ui, figure, problems));
}

/// Draws the list of problems: each one named in full, with the object and property it
/// concerns and how it arose.
pub fn problems_list(ui: &mut egui::Ui, figure: &Figure, problems: &[Problem]) {
    ui.set_max_width(PROBLEM_LIST_WIDTH);
    egui::ScrollArea::vertical()
        .id_salt("ironlab_problems_list")
        .max_height(PROBLEM_LIST_MAX_HEIGHT)
        .show(ui, |ui| {
            for (index, problem) in problems.iter().enumerate() {
                if index > 0 {
                    ui.separator();
                }
                ui.label(egui::RichText::new(problem.subject(figure)).strong());
                ui.label(&problem.detail);
                ui.label(
                    egui::RichText::new(problem.origin.explanation())
                        .weak()
                        .small(),
                );
            }
        });
}

/// Meshes tessellated for one placement of the figure on screen.
struct MeshCache {
    to_screen: ScreenTransform,
    meshes: Vec<Arc<egui::Mesh>>,
}

/// One figure tab.
struct FigurePane {
    title: String,
    state: FigureState,
    /// The property editor of this tab, hidden until the toolbar opens it.
    panel: PropertyPanel,
    /// The compilation of the displayed figure, or `None` when it must be recompiled.
    scene: Option<Scene>,
    /// The meshes of `scene`, or `None` when they must be rebuilt.
    meshes: Option<MeshCache>,
}

impl FigurePane {
    fn new(title: String, figure: Figure) -> Self {
        Self {
            title,
            state: FigureState::new(figure),
            panel: PropertyPanel::default(),
            scene: None,
            meshes: None,
        }
    }

    /// Marks the scene as out of date after a change to the figure.
    fn invalidate(&mut self) {
        self.scene = None;
        self.meshes = None;
    }

    /// Compiles the scene if it is out of date and returns it.
    fn scene(&mut self, text: &TextEngine) -> &Scene {
        if self.scene.is_none() {
            self.meshes = None;
        }
        self.scene
            .get_or_insert_with(|| ironlab_scene::compile(self.state.figure(), text))
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        text: &TextEngine,
        notification: &mut Option<Notification>,
    ) {
        self.scene(text);
        let mut problems: Vec<Problem> = self.scene.as_ref().map_or_else(Vec::new, |scene| {
            scene.warnings.iter().map(Problem::from_scene).collect()
        });
        problems.extend(self.state.problems().iter().cloned());
        let response = egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                toolbar(ui, &mut self.state, &problems, &mut self.panel.open)
            })
            .inner;
        if response.changed {
            self.invalidate();
        }
        if crate::panel::property_panel(ui, &mut self.panel, &mut self.state) {
            self.invalidate();
        }
        let now = ui.input(|i| i.time);
        if response.export_requested
            && let Some(outcome) = self.export(text, now)
        {
            *notification = Some(outcome);
        }
        if response.save_requested
            && let Some(outcome) = self.save(now)
        {
            *notification = Some(outcome);
        }
        self.canvas(ui, text);
    }

    /// Asks for a destination and writes the displayed figure as a PDF there. Returns a notification of the outcome,
    /// or `None` when the user cancelled the dialog.
    fn export(&self, text: &TextEngine, now: f64) -> Option<Notification> {
        let stem = crate::files::figure_stem(&self.title);
        let path = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(format!("{stem}.pdf"))
            .save_file()?;
        Some(
            match ironlab_pdf::write_pdf(self.state.figure(), text, &path) {
                Ok(()) => Notification {
                    message: format!("Exported {}", path.display()),
                    is_error: false,
                    shown_at: now,
                },
                Err(error) => Notification {
                    message: format!("Could not export {}: {error}", path.display()),
                    is_error: true,
                    shown_at: now,
                },
            },
        )
    }

    /// Asks for a destination and writes the displayed figure as a figure file there, in the format named by the
    /// extension the user gives.
    ///
    /// The figure written becomes the source of the tab and the overlay is emptied, because the viewer owns the source
    /// of the figures it opens. Returns a notification of the outcome, or `None` when the user cancelled the dialog.
    fn save(&mut self, now: f64) -> Option<Notification> {
        let stem = crate::files::figure_stem(&self.title);
        let path = rfd::FileDialog::new()
            .add_filter("Figure", &["fig", "json"])
            .set_file_name(format!("{stem}.fig"))
            .save_file()?;
        Some(
            match crate::files::write_figure(&path, self.state.figure()) {
                Ok(()) => {
                    self.state.fold_overlay();
                    Notification {
                        message: format!("Saved {}", path.display()),
                        is_error: false,
                        shown_at: now,
                    }
                }
                Err(error) => Notification {
                    message: format!("Could not save {}: {error}", path.display()),
                    is_error: true,
                    shown_at: now,
                },
            },
        )
    }

    /// Draws the figure in the remaining space of the tab and applies pointer gestures to it.
    fn canvas(&mut self, ui: &mut egui::Ui, text: &TextEngine) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

        let (width_pt, height_pt) = {
            let list = &self.scene(text).display_list;
            (list.width_pt as f32, list.height_pt as f32)
        };
        let area = rect.shrink(CANVAS_MARGIN);
        if !(width_pt > 0.0 && height_pt > 0.0 && area.width() > 0.0 && area.height() > 0.0) {
            return;
        }
        let scale = (area.width() / width_pt).min(area.height() / height_pt);
        let size = egui::vec2(width_pt * scale, height_pt * scale);
        let to_screen = ScreenTransform {
            scale,
            origin: rect.center() - size / 2.0,
        };

        self.handle_input(ui, &response, to_screen, text);

        let scene = self.scene(text);
        if let Some(background) = color32(scene.display_list.background)
            && background.a() > 0
        {
            painter.rect_filled(
                egui::Rect::from_min_size(to_screen.origin, size),
                0.0,
                background,
            );
        }
        if self
            .meshes
            .as_ref()
            .is_none_or(|cache| cache.to_screen != to_screen)
        {
            let scene = self.scene(text);
            let meshes = tessellate(&scene.display_list, text, to_screen)
                .into_iter()
                .map(Arc::new)
                .collect();
            self.meshes = Some(MeshCache { to_screen, meshes });
        }
        if let Some(cache) = &self.meshes {
            for mesh in &cache.meshes {
                painter.add(egui::Shape::Mesh(Arc::clone(mesh)));
            }
        }

        if let Some(band) = self.state.rubber_band() {
            let band = egui::Rect::from_min_max(
                to_screen.apply(ironlab_scene::display::Point::new(band.x, band.y)),
                to_screen.apply(ironlab_scene::display::Point::new(
                    band.right(),
                    band.bottom(),
                )),
            );
            let selection = ui.visuals().selection;
            painter.rect(
                band,
                0.0,
                selection.bg_fill.gamma_multiply(0.25),
                selection.stroke,
                egui::StrokeKind::Middle,
            );
        }
    }

    /// Converts this frame's pointer input on the canvas into edits of the figure, recompiling the scene after each
    /// edit so that the next gesture is hit-tested against up-to-date geometry.
    fn handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        to_screen: ScreenTransform,
        text: &TextEngine,
    ) {
        let primary = egui::PointerButton::Primary;
        let latest = ui.input(|i| i.pointer.latest_pos());
        let to_figure = |p: egui::Pos2| to_screen.invert(p);

        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            let factor = (f64::from(scroll.y) * WHEEL_ZOOM_RATE).exp() * f64::from(pinch);
            if factor != 1.0
                && let Some(pos) = response.hover_pos()
            {
                self.scene(text);
                let hit = &self.scene.as_ref().expect("compiled above").hit_map;
                if self.state.scroll(hit, to_figure(pos), factor) {
                    self.invalidate();
                }
            }
            let icon = match self.state.tool {
                Tool::Pan if response.dragged() => egui::CursorIcon::Grabbing,
                Tool::Pan => egui::CursorIcon::Grab,
                Tool::Zoom => egui::CursorIcon::Crosshair,
                Tool::Rotate => egui::CursorIcon::Move,
            };
            ui.ctx().set_cursor_icon(icon);
        }

        if response.drag_started_by(primary)
            && let Some(origin) = ui
                .input(|i| i.pointer.press_origin())
                .or(response.interact_pointer_pos())
        {
            self.scene(text);
            let hit = &self.scene.as_ref().expect("compiled above").hit_map;
            self.state.drag_start(hit, to_figure(origin));
        }
        if response.dragged_by(primary)
            && let Some(pos) = response.interact_pointer_pos()
            && self.state.drag_update(to_figure(pos))
        {
            self.invalidate();
        }
        if response.drag_stopped_by(primary) {
            let at = response.interact_pointer_pos().or(latest).map_or(
                ironlab_scene::display::Point::new(f64::NAN, f64::NAN),
                to_figure,
            );
            if self.state.drag_end(at) {
                self.invalidate();
            }
        }

        if let Some(pos) = response.interact_pointer_pos().or(latest) {
            let clicked = if response.double_clicked() {
                Some(true)
            } else if response.clicked() {
                Some(false)
            } else {
                None
            };
            if let Some(double) = clicked {
                self.scene(text);
                let hit = &self.scene.as_ref().expect("compiled above").hit_map;
                let changed = if double {
                    self.state.double_click(hit, to_figure(pos))
                } else {
                    self.state.click(hit, to_figure(pos))
                };
                if changed {
                    self.invalidate();
                }
            }
        }
    }
}

/// A transient message about the outcome of an action, such as a PDF export.
struct Notification {
    message: String,
    is_error: bool,
    /// The egui time at which the notification was first shown.
    shown_at: f64,
}

/// Connects the figure panes to `egui_tiles`.
struct TabBehavior<'a> {
    text: &'a TextEngine,
    notification: &'a mut Option<Notification>,
}

impl egui_tiles::Behavior<FigurePane> for TabBehavior<'_> {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut FigurePane,
    ) -> egui_tiles::UiResponse {
        pane.ui(ui, self.text, self.notification);
        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &FigurePane) -> egui::WidgetText {
        pane.title.clone().into()
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }
}

/// The viewer application: one tab per figure.
pub struct ViewerApp {
    tree: egui_tiles::Tree<FigurePane>,
    /// Tile identifiers of the figure panes, in the order the figures were given.
    panes: Vec<egui_tiles::TileId>,
    text: Arc<TextEngine>,
    notification: Option<Notification>,
}

impl ViewerApp {
    /// Creates an application with one tab per `(title, figure)` pair, in order.
    #[must_use]
    pub fn new(figures: Vec<(String, Figure)>, text: Arc<TextEngine>) -> Self {
        let mut tiles = egui_tiles::Tiles::default();
        let panes: Vec<egui_tiles::TileId> = figures
            .into_iter()
            .map(|(title, figure)| tiles.insert_pane(FigurePane::new(title, figure)))
            .collect();
        let root = tiles.insert_tab_tile(panes.clone());
        Self {
            tree: egui_tiles::Tree::new("ironlab_viewer_tabs", root, tiles),
            panes,
            text,
            notification: None,
        }
    }

    /// Returns the interactive state of the figure at `index` in the order the figures were given.
    #[must_use]
    pub fn figure_state(&self, index: usize) -> Option<&FigureState> {
        let id = *self.panes.get(index)?;
        self.tree.tiles.get_pane(&id).map(|pane| &pane.state)
    }

    /// Returns the interactive state of the figure at `index`, mutably.
    ///
    /// The scene of the figure is recompiled before it is next drawn, so that edits made through this reference are
    /// shown.
    pub fn figure_state_mut(&mut self, index: usize) -> Option<&mut FigureState> {
        let id = *self.panes.get(index)?;
        match self.tree.tiles.get_mut(id)? {
            egui_tiles::Tile::Pane(pane) => {
                pane.invalidate();
                Some(&mut pane.state)
            }
            egui_tiles::Tile::Container(_) => None,
        }
    }

    /// Applies a change to the figure state of every tab that is currently shown, recompiling the tabs it changes.
    fn for_active_panes(&mut self, change: impl Fn(&mut FigureState) -> bool) {
        for id in self.tree.active_tiles() {
            if let Some(egui_tiles::Tile::Pane(pane)) = self.tree.tiles.get_mut(id)
                && change(&mut pane.state)
            {
                pane.invalidate();
            }
        }
    }

    fn show_notification(&mut self, ctx: &egui::Context) {
        let Some(notification) = &self.notification else {
            return;
        };
        let now = ctx.input(|i| i.time);
        let elapsed = now - notification.shown_at;
        let duration = if notification.is_error {
            2.0 * NOTIFICATION_SECONDS
        } else {
            NOTIFICATION_SECONDS
        };
        if !(0.0..duration).contains(&elapsed) {
            self.notification = None;
            return;
        }
        egui::Area::new(egui::Id::new("ironlab_viewer_notification"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let color = if notification.is_error {
                        ui.visuals().error_fg_color
                    } else {
                        ui.visuals().text_color()
                    };
                    ui.label(egui::RichText::new(&notification.message).color(color));
                });
            });
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(duration - elapsed));
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // A shortcut is read only when no text field has the keyboard, so that typing never navigates a figure. The
        // redo shortcut is read before the undo shortcut, because egui matches a shortcut whose modifiers are held
        // alongside others: ⌘⇧Z would otherwise be taken as ⌘Z.
        let (reset, redo, undo) = if ui.ctx().egui_wants_keyboard_input() {
            (false, false, false)
        } else {
            ui.input_mut(|i| {
                (
                    i.key_pressed(egui::Key::R) && i.modifiers.is_none(),
                    i.consume_key(
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                        egui::Key::Z,
                    ),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                )
            })
        };
        if reset {
            self.for_active_panes(FigureState::reset_view);
        }
        if redo {
            self.for_active_panes(FigureState::redo);
        }
        if undo {
            self.for_active_panes(FigureState::undo);
        }
        egui::Frame::central_panel(ui.style())
            .inner_margin(0)
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                let mut behavior = TabBehavior {
                    text: &self.text,
                    notification: &mut self.notification,
                };
                self.tree.ui(&mut behavior, ui);
            });
        self.show_notification(ui.ctx());
    }
}

/// Opens a native window showing `figures` as tabs and blocks until it is closed.
///
/// The window is titled "IronLAB" and uses the wgpu backend with `NativeOptions { multisampling: 4, .. }`. Its text
/// sizes and colours come from [`crate::style`], which is applied to the egui context as the window is created: this
/// is the only place the interface is styled, so that what is on screen matches what that module defines.
///
/// # Errors
///
/// Returns the eframe error when the window or graphics context cannot be created.
pub fn run(figures: Vec<(String, Figure)>) -> eframe::Result<()> {
    let text = Arc::new(TextEngine::new());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("IronLAB")
            .with_inner_size([1100.0, 800.0]),
        renderer: eframe::Renderer::Wgpu,
        multisampling: 4,
        ..Default::default()
    };
    eframe::run_native(
        "IronLAB",
        options,
        Box::new(move |cc| {
            crate::style::apply(&cc.egui_ctx);
            Ok(Box::new(ViewerApp::new(figures, text)))
        }),
    )
}
