//! The eframe viewer application.
//!
//! One figure is shown at a time, chosen in the figure browser of [`crate::sidebar`] when more than one is open.
//! The figure has a toolbar ([`toolbar`]) above a canvas that draws it at its physical aspect ratio, scaled to fit
//! and centred in the area below the toolbar on a neutral surround, and below the canvas a strip of details: the
//! labels and parameters the figure carries, which are what the browser narrows the collection by. The canvas compiles the scene when the figure is first shown and recompiles it after every change to
//! the figure, so that each gesture is hit-tested against the geometry that is on screen. It converts pointer input
//! into figure-space calls on [`FigureState`], and draws the figure as one draw list from
//! [`crate::canvas::tessellate`] through the viewer's own pipelines ([`crate::gpu`]) by a paint callback in the
//! figure's place among egui's shapes, relying on 4× MSAA for anti-aliasing. The list is in figure points and is
//! rebuilt only when the scene changes or the figure's scale on screen has changed enough for the flattening of
//! curves to show; a pan, a resize or a frame in which nothing moved uploads nothing but, at most, the mapping.
//!
//! Each figure also holds the property editor of [`crate::panel`], which the "Properties" button of the toolbar
//! opens into a side panel between the toolbar and the canvas. It is hidden when a figure is opened.
//!
//! Everything shown, exported and saved is the displayed figure of [`FigureState`]: the source figure with the user's
//! overlay applied. Pressing `R` resets the view of the active figure, and ⌘Z and ⌘⇧Z (Ctrl+Z and Ctrl+Shift+Z away
//! from macOS) undo and redo its gestures. "Export PDF…" writes the displayed figure with
//! [`crate::export::write_pdf`], which rasterises the dense parts of the figure through the viewer's own renderer,
//! and "Save figure…" writes it as a figure file with [`crate::files::write_figure`],
//! after which the overlay is folded into the source because the viewer owns it. Both open a native save dialog and
//! report the outcome in a notification.

use std::sync::Arc;

use ironlab_ir::Figure;
use ironlab_scene::Scene;
use ironlab_scene::display::Point;
use ironlab_scene::hit::ImageHit;
use ironlab_text::TextEngine;

use crate::browse::{FacetValue, FigureCard};
use crate::canvas::{MAX_TILE_SIDE, Resolution, ScreenTransform, premultiplied, tessellate};
use crate::gpu::{DEPTH_FORMAT, DrawList, GpuCallback, GpuConfig, GpuPainter};
use crate::interaction::{Datatip, FigureState, PixelDatatip, PixelValue, Tip, Tool};
use crate::panel::PropertyPanel;
use crate::problems::{Problem, indicator_label};
use crate::sidebar::{FigureBrowser, figure_browser};

/// The rate at which a wheel scroll zooms: a scroll of `d` points zooms by `exp(d · rate)`, so that one notch of a
/// typical mouse wheel (50 points) zooms by about 20 %.
const WHEEL_ZOOM_RATE: f64 = 0.0036;

/// The smallest gap, in egui points, between the figure and the edges of its canvas.
const CANVAS_MARGIN: f32 = 22.0;

/// The shadow the page of a figure casts on the surround, which is what makes it read as a sheet lying on the
/// canvas rather than as a white rectangle painted on it.
///
/// The design casts it from a rectangle a little smaller than the page, so that the shadow shows only beneath and
/// beside it; the page is drawn over the rest.
const PAGE_SHADOW: egui::Shadow = egui::Shadow {
    offset: [0, 8],
    blur: 26,
    spread: 0,
    color: egui::Color32::from_black_alpha(160),
};

/// How far inside the page's edge its shadow is cast from, in egui points.
const PAGE_SHADOW_INSET: f32 = 12.0;

/// The room between the edge of the toolbar and its controls, in egui points.
const TOOLBAR_PADDING: egui::Margin = egui::Margin {
    left: 9,
    right: 9,
    top: 6,
    bottom: 6,
};

/// The room between the edge of the strip of details and its contents, in egui points.
const DETAILS_PADDING: egui::Margin = egui::Margin {
    left: 12,
    right: 12,
    top: 8,
    bottom: 12,
};

/// The room between the tags of a figure and the table of its parameters, in egui points.
const DETAILS_GAP: f32 = 7.0;

/// The room between the columns and the rows of the table of a figure's parameters, in egui points.
const DETAILS_SPACING: [f32; 2] = [8.0, 4.0];

/// The number of samples per pixel of the window's multisample anti-aliasing, which the viewer's own pipelines must
/// match.
const MSAA_SAMPLES: u16 = 4;

/// The factor by which the scale of a figure on screen may change before its draw list is rebuilt for the new
/// scale: within it, curves flattened for the old scale stay within a tenth of a pixel of true, and a hairline
/// stays within a quarter of a pixel of one pixel wide.
const REBUILD_RATIO: f32 = 1.25;

/// The radius, in egui points, of the ring drawn around the data point under the pointer, and around the centre of
/// a pixel too small on screen to outline.
const DATATIP_RING_POINTS: f32 = 4.0;

/// The identifier of the strip of details below the canvas, which is also what a test loads its geometry by.
pub const DETAILS_ID: &str = "ironlab_figure_details";

/// The height beyond which the strip of details scrolls, in egui points, which is room for a handful of parameters
/// before the strip starts taking the canvas's room.
const DETAILS_MAX_HEIGHT: f32 = 150.0;

/// Draws one label of a figure as a tag: a small word in a frame of its own, so that a row of them reads as a set
/// of words rather than as a sentence.
fn tag(ui: &mut egui::Ui, label: &str) {
    egui::Frame::new()
        .fill(crate::style::TAG_FILL)
        .stroke(egui::Stroke::new(1.0, crate::style::TAG_STROKE))
        .corner_radius(2)
        .inner_margin(egui::Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(label)
                    .monospace()
                    .small()
                    .color(crate::style::TAG_TEXT),
            );
        });
}

/// How long a notification stays on screen, in seconds.
const NOTIFICATION_SECONDS: f64 = 5.0;

/// The width of the list of problems, in egui points, which is wide enough for a sentence
/// naming a property and its reason.
const PROBLEM_LIST_WIDTH: f32 = 380.0;

/// The height beyond which the list of problems scrolls, in egui points.
const PROBLEM_LIST_MAX_HEIGHT: f32 = 320.0;

/// Formats a data value for a datatip, with enough digits to tell neighbouring points apart and without the noise
/// that printing a binary fraction in full would add.
fn datatip_value(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    if value != 0.0 && !(1e-4..1e6).contains(&value.abs()) {
        return format!("{value:.4e}");
    }
    let text = format!("{value:.6}");
    match text.split_once('.') {
        Some(_) => text.trim_end_matches('0').trim_end_matches('.').to_owned(),
        None => text,
    }
}

/// The text of the tooltip that names the data point under the pointer.
///
/// The index shown is the one the point has in the artist's own data arrays, so it is what the user would use to
/// find the same point in the data they plotted.
fn datatip_text(tip: &Datatip) -> String {
    let mut lines = Vec::new();
    if let Some(name) = &tip.name {
        lines.push(name.clone());
    }
    lines.push(format!("x = {}", datatip_value(tip.x)));
    lines.push(format!("y = {}", datatip_value(tip.y)));
    if let Some(z) = tip.z {
        lines.push(format!("z = {}", datatip_value(z)));
    }
    lines.push(format!("index {}", tip.index));
    lines.join("\n")
}

/// The text of the tooltip that names the pixel under the pointer, one item per line: the name of the image when
/// it has one, the row and column of the pixel in the artist's own array, the coordinates of the pixel's centre,
/// and what the array holds there, every number formatted as the point datatip formats its coordinates, so that
/// the two callouts read as one.
///
/// The last line takes the words of the image's kind: `value = …` for a colour-mapped image, `index = …` for a
/// colour-indexed one, and for a true-colour image `rgb = …` or `rgba = …` listing the components in that order,
/// separated by commas.
#[must_use]
pub fn pixel_datatip_text(tip: &PixelDatatip) -> String {
    let mut lines = Vec::new();
    if let Some(name) = &tip.name {
        lines.push(name.clone());
    }
    lines.push(format!("row {}, column {}", tip.row, tip.column));
    lines.push(format!("x = {}", datatip_value(tip.x)));
    lines.push(format!("y = {}", datatip_value(tip.y)));
    lines.push(match &tip.value {
        PixelValue::Value(value) => format!("value = {}", datatip_value(*value)),
        PixelValue::Index(index) => format!("index = {}", datatip_value(*index)),
        PixelValue::Components(components) => {
            let label = match components.len() {
                3 => "rgb",
                4 => "rgba",
                _ => "components",
            };
            let listed: Vec<String> = components.iter().copied().map(datatip_value).collect();
            format!("{label} = {}", listed.join(", "))
        }
    });
    lines.join("\n")
}

/// The corners of the pixel in `row` and `column` of a drawn image, on screen and in the order they are joined, or
/// `None` when the pixel is smaller on screen than the ring drawn around a point, so that it is ringed instead and
/// the mark is never too small to see, or when its placement cannot be inverted.
fn pixel_outline(
    image: &ImageHit,
    row: usize,
    column: usize,
    to_screen: ScreenTransform,
) -> Option<[egui::Pos2; 4]> {
    let to_figure = image.to_pixel.inverse()?;
    let (c, r) = (column as f64, row as f64);
    let corners = [(c, r), (c + 1.0, r), (c + 1.0, r + 1.0), (c, r + 1.0)]
        .map(|(x, y)| to_screen.apply(to_figure.apply(Point::new(x, y))));
    let bounds = egui::Rect::from_points(&corners);
    let diameter = 2.0 * DATATIP_RING_POINTS;
    (bounds.width() >= diameter && bounds.height() >= diameter).then_some(corners)
}

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
            .on_hover_text("Undo the last change (Cmd+Z, Ctrl+Z).")
            .clicked()
        {
            response.changed |= state.undo();
        }
        if ui
            .add_enabled(state.can_redo(), egui::Button::new("Redo"))
            .on_hover_text("Redo the last undone change (Cmd+Shift+Z, Ctrl+Shift+Z).")
            .clicked()
        {
            response.changed |= state.redo();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Laid out from the right edge inwards, so the first widget added is the rightmost.
            if let Some(label) = indicator_label(problems) {
                problems_indicator(ui, state.figure(), problems, &label);
                ui.separator();
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
            if ui
                .button("Export PDF…")
                .on_hover_text("Save the figure, as currently shown, to a PDF file.")
                .clicked()
            {
                response.export_requested = true;
            }
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
        });
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

/// The draw list of a figure and the scale it was built for.
struct Built {
    list: Arc<DrawList>,
    scale: f32,
}

/// One figure tab.
struct FigurePane {
    title: String,
    state: FigureState,
    /// The property editor of this tab, hidden until the toolbar opens it.
    panel: PropertyPanel,
    /// The compilation of the displayed figure, or `None` when it must be recompiled.
    scene: Option<Scene>,
    /// The draw list of `scene`, or `None` when it must be rebuilt.
    built: Option<Built>,
}

impl FigurePane {
    fn new(title: String, figure: Figure) -> Self {
        Self {
            title,
            state: FigureState::new(figure),
            panel: PropertyPanel::default(),
            scene: None,
            built: None,
        }
    }

    /// Marks the scene as out of date after a change to the figure.
    fn invalidate(&mut self) {
        self.scene = None;
        self.built = None;
    }

    /// Compiles the scene if it is out of date and returns it.
    fn scene(&mut self, text: &TextEngine) -> &Scene {
        if self.scene.is_none() {
            self.built = None;
        }
        self.scene
            .get_or_insert_with(|| ironlab_scene::compile(self.state.figure(), text))
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        text: &TextEngine,
        gpu: Option<GpuConfig>,
        notification: &mut Option<Notification>,
    ) {
        self.scene(text);
        let mut problems: Vec<Problem> = self.scene.as_ref().map_or_else(Vec::new, |scene| {
            scene.warnings.iter().map(Problem::from_scene).collect()
        });
        problems.extend(self.state.problems().iter().cloned());
        let bar = egui::Frame::new()
            .inner_margin(TOOLBAR_PADDING)
            .show(ui, |ui| {
                toolbar(ui, &mut self.state, &problems, &mut self.panel.open)
            });
        // The rule beneath the toolbar, which parts it from the canvas as the browser's rule parts its controls
        // from its list.
        let rect = bar.response.rect;
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            egui::Stroke::new(1.0, crate::style::STROKE),
        );
        let response = bar.inner;
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
        let figure = self.state.figure();
        if figure.labels.is_empty() && figure.parameters.is_empty() {
            return;
        }
        let labels = figure.labels.clone();
        let parameters: Vec<(String, String)> = figure
            .parameters
            .iter()
            .map(|(name, value)| (name.clone(), FacetValue::from(value).text()))
            .collect();
        let frame = egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(DETAILS_PADDING);
        egui::Panel::bottom(egui::Id::new(DETAILS_ID))
            .resizable(false)
            .show_separator_line(true)
            .frame(frame)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(crate::style::ROW_GAP, 0.0);
                egui::ScrollArea::vertical()
                    .id_salt("ironlab_details_scroll")
                    .max_height(DETAILS_MAX_HEIGHT)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if !labels.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing =
                                    egui::Vec2::splat(crate::style::ROW_GAP);
                                for label in &labels {
                                    tag(ui, label);
                                }
                            });
                            ui.add_space(DETAILS_GAP);
                        }
                        if !parameters.is_empty() {
                            egui::Grid::new("ironlab_details_parameters")
                                .num_columns(2)
                                .spacing(DETAILS_SPACING)
                                .show(ui, |ui| {
                                    for (name, value) in &parameters {
                                        ui.label(
                                            egui::RichText::new(name)
                                                .monospace()
                                                .size(crate::style::DETAIL_SIZE_PT)
                                                .weak(),
                                        );
                                        ui.label(
                                            egui::RichText::new(value)
                                                .monospace()
                                                .size(crate::style::DETAIL_SIZE_PT),
                                        );
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
        let stem = crate::files::figure_stem(&self.title);
        let path = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(format!("{stem}.pdf"))
            .save_file()?;
        Some(
            match crate::export::write_pdf(
                self.state.figure(),
                text,
                &ironlab_pdf::PdfOptions::for_figure(self.state.figure()),
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
    ///
    /// The figure is drawn through the viewer's own pipelines by one paint callback built for `gpu`; without a
    /// graphics configuration (only the headless test harness lacks one) the list is built but not drawn.
    fn canvas(&mut self, ui: &mut egui::Ui, text: &TextEngine, gpu: Option<GpuConfig>) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, crate::style::SURROUND);

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
        if let Some(background) = premultiplied(scene.display_list.background)
            && background[3] > 0
        {
            let page = egui::Rect::from_min_size(to_screen.origin, size);
            painter.add(
                PAGE_SHADOW.as_shape(page.shrink(PAGE_SHADOW_INSET), egui::CornerRadius::ZERO),
            );
            painter.rect_filled(
                page,
                0.0,
                egui::Color32::from_rgba_premultiplied(
                    background[0],
                    background[1],
                    background[2],
                    background[3],
                ),
            );
        }
        let rebuild = self.built.as_ref().is_none_or(|built| {
            scale > built.scale * REBUILD_RATIO || scale < built.scale / REBUILD_RATIO
        });
        if rebuild {
            let scene = self.scene.as_ref().expect("compiled above");
            let max_tile_side = ui.ctx().input(|input| input.max_texture_side);
            let max_tile_side = u32::try_from(max_tile_side)
                .map_or(MAX_TILE_SIDE, |limit| limit.min(MAX_TILE_SIDE));
            let list = tessellate(
                &scene.display_list,
                text,
                Resolution {
                    scale,
                    max_tile_side,
                },
            );
            self.built = Some(Built {
                list: Arc::new(list),
                scale,
            });
        }
        if let (Some(built), Some(config)) = (&self.built, gpu) {
            // The callback covers the whole screen, so that its vertex mapping is the whole target's; every draw
            // of the list clips itself, within the painter's clip.
            painter.add(egui::Shape::Callback(
                egui_wgpu::Callback::new_paint_callback(
                    ui.ctx().viewport_rect(),
                    GpuCallback {
                        list: Arc::clone(&built.list),
                        config,
                        to_screen,
                    },
                ),
            ));
        }

        self.datatip(ui, &response, &painter, to_screen, text);

        if let Some(band) = self.state.rubber_band() {
            let band = egui::Rect::from_min_max(
                to_screen.apply(Point::new(band.x, band.y)),
                to_screen.apply(Point::new(band.right(), band.bottom())),
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

    /// Reads what lies under the pointer — a drawn data point or, where none is within reach, the pixel of an
    /// image — and shows it, marked on the canvas and named in a tooltip.
    ///
    /// Both come from the hit map of the current compilation, so a point names the index and the values of the
    /// user's own data even where the series was thinned to fit the view, and a pixel names the row and column of
    /// the user's own array. A point is ringed. A pixel is outlined, so that the reader sees the extent that was
    /// read, or ringed at its centre when it is smaller on screen than the ring, so that the mark is never too
    /// small to see.
    fn datatip(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        to_screen: ScreenTransform,
        text: &TextEngine,
    ) {
        if response.dragged() || !response.hovered() {
            return;
        }
        let Some(pointer) = response.hover_pos() else {
            return;
        };
        self.scene(text);
        let hit = &self.scene.as_ref().expect("compiled above").hit_map;
        let at = to_screen.invert(pointer);
        let Some(tip) = self.state.tip_at(hit, at) else {
            return;
        };
        let stroke = ui.visuals().selection.stroke;
        match tip {
            Tip::Point(tip) => {
                painter.circle_stroke(to_screen.apply(tip.position), DATATIP_RING_POINTS, stroke);
                response.clone().on_hover_text(datatip_text(&tip));
            }
            Tip::Pixel(tip) => {
                let outline = hit
                    .pixel_at(at)
                    .and_then(|(image, row, column)| pixel_outline(image, row, column, to_screen));
                match outline {
                    Some(corners) => {
                        painter.add(egui::Shape::closed_line(corners.to_vec(), stroke));
                    }
                    None => {
                        painter.circle_stroke(
                            to_screen.apply(tip.position),
                            DATATIP_RING_POINTS,
                            stroke,
                        );
                    }
                }
                response.clone().on_hover_text(pixel_datatip_text(&tip));
            }
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
            let at = response
                .interact_pointer_pos()
                .or(latest)
                .map_or(Point::new(f64::NAN, f64::NAN), to_figure);
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
        self.panes.get(index).map(|pane| &pane.state)
    }

    /// Returns the draw list last built for the figure at `index`, or `None` before its canvas has been drawn. The
    /// list is rebuilt when the figure changes or its scale on screen moves outside the band of the scale it was
    /// built for, and is otherwise the same `Arc` from frame to frame.
    #[must_use]
    pub fn draw_list(&self, index: usize) -> Option<Arc<DrawList>> {
        self.panes
            .get(index)
            .and_then(|pane| pane.built.as_ref())
            .map(|built| Arc::clone(&built.list))
    }

    /// Returns the interactive state of the figure at `index`, mutably.
    ///
    /// The scene of the figure is recompiled before it is next drawn, so that edits made through this reference are
    /// shown.
    pub fn figure_state_mut(&mut self, index: usize) -> Option<&mut FigureState> {
        let pane = self.panes.get_mut(index)?;
        pane.invalidate();
        Some(&mut pane.state)
    }

    /// Applies a change to the figure state of the figure shown, recompiling it if it changed.
    fn for_shown_pane(&mut self, change: impl Fn(&mut FigureState) -> bool) {
        if let Some(pane) = self.panes.get_mut(self.shown)
            && change(&mut pane.state)
        {
            pane.invalidate();
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
            self.for_shown_pane(FigureState::reset_view);
        }
        if redo {
            self.for_shown_pane(FigureState::redo);
        }
        if undo {
            self.for_shown_pane(FigureState::undo);
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
