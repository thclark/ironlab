//! The eframe viewer application.
//!
//! One figure is shown at a time, chosen in the figure browser of [`crate::sidebar`] when more than one is open.
//! The figure has a toolbar ([`toolbar`]) above a canvas that draws it at its physical aspect ratio, scaled to fit
//! and centred in the area below the toolbar on a neutral surround, and below the canvas a strip of details: the
//! labels and parameters the figure carries, which are what the browser narrows the collection by.
//!
//! The canvas is the egui host of a [`FigureCanvas`], which owns everything about showing the figure that does not
//! depend on egui: the compilation of the figure, its fit into the area, the draw list kept for that fit, and the
//! conversion of gestures into figure-space calls on [`FigureState`]. The pane forwards the gestures egui decides
//! from the canvas's response (a hover, a wheel notch, the start, moves and end of a drag, a click and a double
//! click), then draws what the controller reports: the page background inside the fit, the draw list through the
//! viewer's own pipelines ([`ironlab_canvas::gpu`]) by a paint callback ([`GpuCallback`]) in the figure's place among egui's
//! shapes, relying on 4× MSAA for anti-aliasing, and the cursor, callout and rubber band with egui's painter.
//!
//! Each figure also holds the property editor of [`crate::panel`], which the "Properties" button of the toolbar
//! opens into a side panel between the toolbar and the canvas. It is hidden when a figure is opened.
//!
//! Everything shown, exported and saved is the displayed figure of [`FigureState`]: the source figure with the user's
//! overlay applied. Pressing `R` resets the view of the active figure, and ⌘Z and ⌘⇧Z (Ctrl+Z and Ctrl+Shift+Z away
//! from macOS) undo and redo its gestures. "Export PDF…" writes the displayed figure with
//! [`ironlab_canvas::export::write_pdf`], which rasterises the dense parts of the figure through the viewer's own renderer,
//! and "Save figure…" writes it as a figure file with [`ironlab_canvas::files::write_figure`],
//! after which the overlay is folded into the source because the viewer owns it. Both open a native save dialog and
//! report the outcome in a notification.

use std::sync::Arc;

use ironlab_ir::Figure;
use ironlab_text::TextEngine;

use crate::browse::{FacetValue, FigureCard};
use crate::callback::GpuCallback;
use crate::panel::PropertyPanel;
use crate::sidebar::{FigureBrowser, figure_browser};
use crate::widgets::{Control, PanelKind, Role, Spacing, label, surround, text};
use ironlab_canvas::canvas::MAX_TILE_SIDE;
use ironlab_canvas::figure_canvas::{Callout, Cursor, FigureCanvas, Marker, wheel_factor};
use ironlab_canvas::gpu::{DEPTH_FORMAT, DrawList, GpuConfig, GpuPainter};
use ironlab_canvas::interaction::{FigureState, Tool};
use ironlab_canvas::problems::{Problem, indicator_label};

/// The room between the edge of the toolbar and its controls, in egui points.
const TOOLBAR_PADDING: egui::Margin = egui::Margin {
    left: 9,
    right: 9,
    top: 6,
    bottom: 6,
};

/// The number of samples per pixel of the window's multisample anti-aliasing, which the viewer's own pipelines must
/// match.
const MSAA_SAMPLES: u16 = 4;

/// The identifier of the strip of details below the canvas, which is also what a test loads its geometry by.
pub const DETAILS_ID: &str = "ironlab_figure_details";

/// The height beyond which the strip of details scrolls, in egui points, which is room for a handful of parameters
/// before the strip starts taking the canvas's room.
const DETAILS_MAX_HEIGHT: f32 = 150.0;

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
    /// Whether the toolbar changed the displayed figure (for example through "Refit" or "Undo").
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
/// are disabled when there is nothing to undo or redo, a "Refit" button that calls [`FigureState::reset_view`],
/// "Export PDF…" and "Save figure…" buttons, a "Properties" button that opens and closes the property editor through
/// `show_properties`, and, when `problems` is not empty, a problems indicator whose label contains the number of
/// problems (for example "2 problems") and which opens the list of [`problems_list`] when it is clicked.
///
/// The tools and the history sit at the left and everything that acts on the whole figure at the right, so that
/// the two groups read as what they are and the space between them is not filled with buttons.
pub fn toolbar(
    ui: &mut egui::Ui,
    state: &mut FigureState,
    problems: &[Problem],
    show_properties: &mut bool,
) -> ToolbarResponse {
    let mut response = ToolbarResponse::default();
    crate::style::compact(ui);
    let indicator = indicator_label(problems);

    // The file controls are laid out from the right edge inwards, which draws them over the tools when the row has
    // no room for both groups. They are given a row of their own instead, decided from the width of their captions
    // rather than from where last frame put them, so that the toolbar never draws one control over another.
    let history = buttons_width(ui, &["Pan", "Zoom", "Rotate", "Refit", "Undo", "Redo"], 1);
    let mut file_captions = vec!["Export PDF…", "Save figure…", "Properties"];
    if let Some(label) = &indicator {
        file_captions.push(label);
    }
    let file = buttons_width(ui, &file_captions, usize::from(indicator.is_some()));
    let two_rows = history + ui.spacing().item_spacing.x + file > ui.available_width();

    ui.horizontal(|ui| {
        history_controls(ui, state, &mut response);
        if !two_rows {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                file_controls(
                    ui,
                    state,
                    problems,
                    indicator.as_deref(),
                    show_properties,
                    &mut response,
                );
            });
        }
    });
    if two_rows {
        ui.add_space(Spacing::GAP);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                file_controls(
                    ui,
                    state,
                    problems,
                    indicator.as_deref(),
                    show_properties,
                    &mut response,
                );
            });
        });
    }
    response
}

/// The width a row of buttons with these captions takes, in egui points: each caption in the button font with the
/// button's padding, the spacing between them, and `separators` separators among them.
fn buttons_width(ui: &egui::Ui, captions: &[&str], separators: usize) -> f32 {
    let font_id = Role::Control.font();
    let spacing = ui.spacing();
    let buttons: f32 = captions
        .iter()
        .map(|caption| {
            ui.painter()
                .layout_no_wrap((*caption).to_owned(), font_id.clone(), egui::Color32::WHITE)
                .size()
                .x
                + 2.0 * Spacing::LARGE_CONTROL_PADDING.x
        })
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let gaps = (captions.len() + separators).saturating_sub(1) as f32;
    #[allow(clippy::cast_precision_loss)]
    let separators = separators as f32;
    buttons + gaps * spacing.item_spacing.x + separators * SEPARATOR_WIDTH
}

/// The width egui gives a separator drawn across a row, in egui points.
const SEPARATOR_WIDTH: f32 = 6.0;

/// Draws the tools and the controls that move through the history of the figure: Pan, Zoom, Rotate and Refit,
/// then Undo and Redo.
fn history_controls(ui: &mut egui::Ui, state: &mut FigureState, response: &mut ToolbarResponse) {
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
        if Control::button(label)
            .large()
            .selected(state.tool == tool)
            .enabled(enabled)
            .show(ui)
            .on_hover_text(hint)
            .clicked()
        {
            state.tool = tool;
        }
    }
    // Refit belongs with the tools: like them it acts on the view and nothing else, restoring the fit of every
    // axes that a pan, a zoom or a rotation moved.
    if Control::button("Refit")
        .large()
        .show(ui)
        .on_hover_text(
            "Restore the limits and 3D views of every axes (R), keeping hidden plots hidden and every property \
             you have edited. Double-click an axes to restore only that axes. To discard every change instead, \
             use Revert all changes at the foot of the property editor.",
        )
        .clicked()
    {
        response.changed |= state.reset_view();
    }
    ui.separator();
    if Control::button("Undo")
        .large()
        .enabled(state.can_undo())
        .show(ui)
        .on_hover_text("Undo the last change (Cmd+Z, Ctrl+Z).")
        .clicked()
    {
        response.changed |= state.undo();
    }
    if Control::button("Redo")
        .large()
        .enabled(state.can_redo())
        .show(ui)
        .on_hover_text("Redo the last undone change (Cmd+Shift+Z, Ctrl+Shift+Z).")
        .clicked()
    {
        response.changed |= state.redo();
    }
}

/// Draws the controls that concern the figure as a file, and the problems indicator when there is one, laid out
/// from the right edge inwards so that the first widget added is the rightmost.
fn file_controls(
    ui: &mut egui::Ui,
    state: &FigureState,
    problems: &[Problem],
    indicator: Option<&str>,
    show_properties: &mut bool,
    response: &mut ToolbarResponse,
) {
    if let Some(label) = indicator {
        problems_indicator(ui, state.figure(), problems, label);
        ui.separator();
    }
    if Control::button("Properties")
        .large()
        .selected(*show_properties)
        .show(ui)
        .on_hover_text("Show or hide the property editor, which lists the objects of the figure and their properties.")
        .clicked()
    {
        *show_properties = !*show_properties;
    }
    if Control::button("Save figure…")
        .large()
        .show(ui)
        .on_hover_text(
            "Save the figure, as currently shown, to a .fig (Protocol Buffers) or .json file.",
        )
        .clicked()
    {
        response.save_requested = true;
    }
    if Control::button("Export PDF…")
        .large()
        .show(ui)
        .on_hover_text("Save the figure, as currently shown, to a PDF file.")
        .clicked()
    {
        response.export_requested = true;
    }
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

/// One figure tab: the egui host of a [`FigureCanvas`].
struct FigurePane {
    title: String,
    /// The property editor of this tab, hidden until the toolbar opens it.
    panel: PropertyPanel,
    /// The figure, its interaction state, its compilation and its draw list.
    canvas: FigureCanvas,
}

impl FigurePane {
    fn new(title: String, figure: Figure) -> Self {
        Self {
            title,
            panel: PropertyPanel::default(),
            canvas: FigureCanvas::new(figure),
        }
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        text: &TextEngine,
        gpu: Option<GpuConfig>,
        notification: &mut Option<Notification>,
    ) {
        let problems = self.canvas.problems(text);
        // Nothing stands between the toolbar, the strip of details and the canvas: each ends where the next
        // begins, and the rule beneath the toolbar is its last row rather than a line in a gap.
        ui.spacing_mut().item_spacing.y = 0.0;
        let bar = egui::Frame::new()
            .inner_margin(TOOLBAR_PADDING)
            .show(ui, |ui| {
                let mut response = ToolbarResponse::default();
                self.canvas.edit(|state| {
                    response = toolbar(ui, state, &problems, &mut self.panel.open);
                    response.changed
                });
                response
            });
        // The rule beneath the toolbar, which parts it from the canvas as the browser's rule parts its controls
        // from its list.
        let rect = bar.response.rect;
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            egui::Stroke::new(1.0, crate::style::STROKE),
        );
        let response = bar.inner;
        self.canvas
            .edit(|state| crate::panel::property_panel(ui, &mut self.panel, state));
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
        self.details(ui);
        self.canvas(ui, text, gpu);
    }

    /// Draws the strip below the canvas that says what the figure carries: its labels as tags, then its parameters
    /// as a table, in ascending order of name.
    ///
    /// It is the same information the property editor edits, shown where it can be read at a glance beside the
    /// figure it describes, because it is what the browser narrows the collection by and a reader choosing between
    /// figures wants to see it without opening an editor. The strip is drawn only when there is something in it,
    /// so a figure that carries nothing keeps the whole height for its canvas.
    fn details(&mut self, ui: &mut egui::Ui) {
        let figure = self.canvas.state().figure();
        if figure.labels.is_empty() && figure.parameters.is_empty() {
            return;
        }
        let labels = figure.labels.clone();
        let parameters: Vec<(String, String)> = figure
            .parameters
            .iter()
            .map(|(name, value)| (name.clone(), FacetValue::from(value).text()))
            .collect();
        egui::Panel::bottom(egui::Id::new(DETAILS_ID))
            .resizable(false)
            .show_separator_line(true)
            .frame(PanelKind::Details.frame(ui))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(Spacing::GAP, 0.0);
                egui::ScrollArea::vertical()
                    .id_salt("ironlab_details_scroll")
                    .max_height(DETAILS_MAX_HEIGHT)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if !labels.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::Vec2::splat(Spacing::GAP);
                                for tag in &labels {
                                    Control::tag(tag).show(ui);
                                }
                            });
                            ui.add_space(Spacing::GAP);
                        }
                        if !parameters.is_empty() {
                            egui::Grid::new("ironlab_details_parameters")
                                .num_columns(2)
                                .spacing([Spacing::GAP, Spacing::LINE_GAP * 2.0])
                                .show(ui, |ui| {
                                    for (name, value) in &parameters {
                                        label(ui, text(Role::Data, name));
                                        label(ui, text(Role::Body, value));
                                        ui.end_row();
                                    }
                                });
                        }
                    });
            });
    }

    /// Asks for a destination and writes the displayed figure as a PDF there. Returns a notification of the outcome,
    /// or `None` when the user cancelled the dialog.
    fn export(&self, text: &TextEngine, now: f64) -> Option<Notification> {
        let stem = ironlab_canvas::files::figure_stem(&self.title);
        let path = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(format!("{stem}.pdf"))
            .save_file()?;
        let figure = self.canvas.state().figure();
        Some(
            match ironlab_canvas::export::write_pdf(
                figure,
                text,
                &ironlab_pdf::PdfOptions::for_figure(figure),
                &path,
            ) {
                // The warnings are those the problems indicator already shows for the open figure.
                Ok(_) => Notification {
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
        let stem = ironlab_canvas::files::figure_stem(&self.title);
        let path = rfd::FileDialog::new()
            .add_filter("Figure", &["fig", "json"])
            .set_file_name(format!("{stem}.fig"))
            .save_file()?;
        Some(
            match ironlab_canvas::files::write_figure(&path, self.canvas.state().figure()) {
                Ok(()) => {
                    // Folding the overlay leaves the displayed figure as it is, so the scene is kept.
                    self.canvas.edit(|state| {
                        state.fold_overlay();
                        false
                    });
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

    /// Draws the figure in the remaining space of the tab and forwards this frame's pointer gestures to it.
    ///
    /// egui decides the gestures from the response of the allocated rectangle; the controller converts them and
    /// says what to draw. The figure is drawn through the viewer's own pipelines by one paint callback built for
    /// `gpu`; without a graphics configuration (only the headless test harness lacks one) the list is built but not
    /// drawn.
    fn canvas(&mut self, ui: &mut egui::Ui, text: &TextEngine, gpu: Option<GpuConfig>) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        surround(&painter, rect, None);
        let max_tile_side = ui.ctx().input(|input| input.max_texture_side);
        self.canvas
            .resize(rect, u32::try_from(max_tile_side).unwrap_or(MAX_TILE_SIDE));

        let primary = egui::PointerButton::Primary;
        let latest = ui.input(|i| i.pointer.latest_pos());
        self.canvas.hover(response.hover_pos());
        if let Some(pos) = response.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            self.canvas.wheel(pos, wheel_factor(scroll.y, pinch), text);
        }
        if response.drag_started_by(primary)
            && let Some(origin) = ui
                .input(|i| i.pointer.press_origin())
                .or(response.interact_pointer_pos())
        {
            self.canvas.drag_start(origin, text);
        }
        if response.dragged_by(primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.canvas.drag_move(pos);
        }
        if response.drag_stopped_by(primary) {
            self.canvas
                .drag_end(response.interact_pointer_pos().or(latest));
        }
        if let Some(pos) = response.interact_pointer_pos().or(latest) {
            if response.double_clicked() {
                self.canvas.double_click(pos, text);
            } else if response.clicked() {
                self.canvas.click(pos, text);
            }
        }

        let Some(fit) = self.canvas.fit(text) else {
            return;
        };
        if response.hovered() {
            ui.ctx().set_cursor_icon(match self.canvas.cursor() {
                Cursor::Grab => egui::CursorIcon::Grab,
                Cursor::Grabbing => egui::CursorIcon::Grabbing,
                Cursor::Crosshair => egui::CursorIcon::Crosshair,
                Cursor::Move => egui::CursorIcon::Move,
            });
        }
        if let Some([r, g, b, a]) = self.canvas.background(text) {
            surround(&painter, rect, Some(fit.page));
            painter.rect_filled(
                fit.page,
                0.0,
                egui::Color32::from_rgba_premultiplied(r, g, b, a),
            );
        }
        let list = self.canvas.draw_list(text);
        if let (Some((list, to_screen)), Some(config)) = (list, gpu) {
            // The callback covers the whole screen, so that its vertex mapping is the whole target's; every draw
            // of the list clips itself, within the painter's clip.
            painter.add(egui::Shape::Callback(
                egui_wgpu::Callback::new_paint_callback(
                    ui.ctx().viewport_rect(),
                    GpuCallback {
                        list,
                        config,
                        to_screen,
                    },
                ),
            ));
        }
        let stroke = ui.visuals().selection.stroke;
        if let Some(Callout { marker, text }) = self.canvas.callout(text) {
            match marker {
                Marker::Ring { centre, radius } => {
                    painter.circle_stroke(centre, radius, stroke);
                }
                Marker::Outline(corners) => {
                    painter.add(egui::Shape::closed_line(corners.to_vec(), stroke));
                }
            }
            response.clone().on_hover_text(text);
        }
        if let Some(band) = self.canvas.rubber_band(text) {
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
}

/// A transient message about the outcome of an action, such as a PDF export.
struct Notification {
    message: String,
    is_error: bool,
    /// The egui time at which the notification was first shown.
    shown_at: f64,
}

/// The viewer application: the open figures, one of which is shown.
pub struct ViewerApp {
    /// The figures, in the order they were given.
    panes: Vec<FigurePane>,
    /// The index of the figure shown, into `panes`.
    shown: usize,
    /// What the figure browser shows of each figure, in the order the figures were given. It is read once, when
    /// the figures are opened, because counting the values of a figure's data on every frame would be felt.
    cards: Vec<FigureCard>,
    /// The figure browser, which chooses which figure is shown.
    browser: FigureBrowser,
    text: Arc<TextEngine>,
    notification: Option<Notification>,
    /// The render target the viewer's own pipelines draw into, or `None` when the application has no graphics
    /// device, which only the headless test harness lacks.
    gpu: Option<GpuConfig>,
}

impl ViewerApp {
    /// Creates an application holding the `(title, figure)` pairs in order, showing the first.
    #[must_use]
    pub fn new(figures: Vec<(String, Figure)>, text: Arc<TextEngine>) -> Self {
        let cards: Vec<FigureCard> = figures
            .iter()
            .map(|(title, figure)| FigureCard::of(title.clone(), figure))
            .collect();
        let panes: Vec<FigurePane> = figures
            .into_iter()
            .map(|(title, figure)| FigurePane::new(title, figure))
            .collect();
        // The browser opens with the collection: it is the only way to reach a figure other than the first. One
        // figure is not a collection, and opens as before.
        let browser = FigureBrowser::for_collection(panes.len());
        Self {
            panes,
            shown: 0,
            cards,
            browser,
            text,
            notification: None,
            gpu: None,
        }
    }

    /// The index, in the order the figures were given, of the figure shown.
    #[must_use]
    pub fn shown(&self) -> usize {
        self.shown
    }

    /// What the figure browser shows of each figure, in the order the figures were given.
    #[must_use]
    pub fn cards(&self) -> &[FigureCard] {
        &self.cards
    }

    /// The state of the figure browser, so that a test can drive the browsing without the interface.
    #[must_use]
    pub fn browser(&self) -> &FigureBrowser {
        &self.browser
    }

    /// The state of the figure browser, mutably.
    pub fn browser_mut(&mut self) -> &mut FigureBrowser {
        &mut self.browser
    }

    /// Shows the figure at `index` in the order the figures were given, returning whether the shown figure changed.
    pub fn show_figure(&mut self, index: usize) -> bool {
        if index >= self.panes.len() || index == self.shown {
            return false;
        }
        self.shown = index;
        true
    }

    /// Sets the render target that the viewer's own pipelines draw into, from the window's graphics state.
    #[must_use]
    pub fn with_gpu(mut self, gpu: GpuConfig) -> Self {
        self.gpu = Some(gpu);
        self
    }

    /// Returns the interactive state of the figure at `index` in the order the figures were given.
    #[must_use]
    pub fn figure_state(&self, index: usize) -> Option<&FigureState> {
        self.panes.get(index).map(|pane| pane.canvas.state())
    }

    /// Returns the draw list last built for the figure at `index`, or `None` before its canvas has been drawn. The
    /// list is rebuilt when the figure changes or its scale on screen moves outside the band of the scale it was
    /// built for, and is otherwise the same `Arc` from frame to frame.
    #[must_use]
    pub fn draw_list(&self, index: usize) -> Option<Arc<DrawList>> {
        self.panes
            .get(index)
            .and_then(|pane| pane.canvas.built_list())
    }

    /// Returns the interactive state of the figure at `index`, mutably.
    ///
    /// The scene of the figure is recompiled before it is next drawn, so that edits made through this reference are
    /// shown.
    pub fn figure_state_mut(&mut self, index: usize) -> Option<&mut FigureState> {
        self.panes
            .get_mut(index)
            .map(|pane| pane.canvas.state_mut())
    }

    /// Applies a change to the canvas of the figure shown, which recompiles the figure if it changed.
    fn for_shown_canvas(&mut self, change: impl FnOnce(&mut FigureCanvas) -> bool) {
        if let Some(pane) = self.panes.get_mut(self.shown) {
            change(&mut pane.canvas);
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
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // The buffers and textures the previous frame did not draw are freed before this frame draws.
        if let Some(state) = frame.wgpu_render_state()
            && let Some(painter) = state
                .renderer
                .write()
                .callback_resources
                .get_mut::<GpuPainter>()
        {
            painter.retain_used();
        }
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
            self.for_shown_canvas(FigureCanvas::reset_view);
        }
        if redo {
            self.for_shown_canvas(FigureCanvas::redo);
        }
        if undo {
            self.for_shown_canvas(FigureCanvas::undo);
        }
        // The browser is a panel, so it is added before the central panel that holds the figure; a panel takes
        // its room from what is left, and the central panel takes what remains.
        if self.panes.len() > 1 {
            let chosen = figure_browser(ui, &mut self.browser, &self.cards, self.shown).chosen;
            if let Some(index) = chosen {
                self.show_figure(index);
            }
        }
        egui::Frame::central_panel(ui.style())
            .inner_margin(0)
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                if let Some(pane) = self.panes.get_mut(self.shown) {
                    pane.ui(ui, &self.text, self.gpu, &mut self.notification);
                }
            });
        self.show_notification(ui.ctx());
    }
}

/// Opens a native window showing `figures`, with the browser beside them when there are several, and blocks until
/// it is closed.
///
/// The window is titled "IronLAB" and uses the wgpu backend with four-sample anti-aliasing and a depth buffer, which
/// the viewer's own pipelines draw the three-dimensional axes with. Its text sizes and colours come from
/// [`crate::style`], which is applied to the egui context as the window is created: this is the only place the
/// interface is styled, so that what is on screen matches what that module defines.
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
        multisampling: MSAA_SAMPLES,
        depth_buffer: 32,
        ..Default::default()
    };
    eframe::run_native(
        "IronLAB",
        options,
        Box::new(move |cc| {
            crate::style::apply(&cc.egui_ctx);
            let mut app = ViewerApp::new(figures, text);
            if let Some(state) = cc.wgpu_render_state.as_ref() {
                app = app.with_gpu(GpuConfig {
                    target_format: state.target_format,
                    samples: u32::from(MSAA_SAMPLES),
                    depth_format: DEPTH_FORMAT,
                });
            }
            Ok(Box::new(app))
        }),
    )
}
