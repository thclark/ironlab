//! The widgets the viewer's interface is drawn with.
//!
//! The interface is drawn from a small vocabulary, and every part of it is spelled from that vocabulary and
//! nothing else:
//!
//! - **Four roles of text at three sizes** ([`Role`]). Words are set in the proportional face, data in the
//!   monospaced one, and the scale has three steps: body, control, and label. Nothing names a size of its own.
//! - **One icon vocabulary** ([`Icon`]), painted rather than typed: eight shapes at one size and one stroke, in
//!   the colour of the text they stand beside. No mark depends on a font.
//! - **Three components** ([`Control`], [`Row`], [`field`]) and the frames they sit in. A button, a chip, a tag,
//!   a tool and the control that reverses an order are one control; a row of the list, a heading, a menu entry
//!   and a checkbox are one row.
//! - **Colour applied when painting, never when laying out.** Every galley is laid out in the placeholder colour
//!   and painted with the colour of the widget's state, so hover, disabled and selected follow from one rule.
//! - **One spacing table** ([`Spacing`]). Blocks and rows share a horizontal inset, so text in a row and text in a
//!   control above it start at the same edge.

use egui::text::LayoutJob;
use egui::{Align, Align2, Color32, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::style;

// =====================================================================================================================
// Type
// =====================================================================================================================

/// The four kinds of text the interface draws, on a scale of three sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Words to read: a title in the list, a value in a checklist, what is typed into a field. 14 pt.
    Body,
    /// Words on a control: the caption of a button, the text of a combo box, a note beneath a list. 12.5 pt.
    Control,
    /// Words that name a section: the caption of a row of controls and the heading of a group, in spaced
    /// capitals. 11 pt.
    Label,
    /// Data beside words: a count, a parameter's name, the labels beneath a title, a value in a table. 11 pt,
    /// monospaced.
    Data,
}

impl Role {
    /// The scale: body, control, label.
    const BODY_PT: f32 = 14.0;
    const CONTROL_PT: f32 = 12.5;
    const LABEL_PT: f32 = 11.0;
    /// How far the letters of a label are spread, as a fraction of its size.
    const LABEL_TRACKING: f32 = 0.08;

    #[must_use]
    pub fn font(self) -> FontId {
        match self {
            Self::Body => FontId::proportional(Self::BODY_PT),
            Self::Control => FontId::proportional(Self::CONTROL_PT),
            Self::Label => FontId::proportional(Self::LABEL_PT),
            Self::Data => FontId::monospace(Self::LABEL_PT),
        }
    }

    /// Whether the role is quieter than ordinary text.
    fn weak(self) -> bool {
        matches!(self, Self::Label | Self::Data)
    }

    fn format(self) -> egui::TextFormat {
        egui::TextFormat {
            font_id: self.font(),
            color: Color32::PLACEHOLDER,
            extra_letter_spacing: if self == Self::Label {
                Self::LABEL_TRACKING * Self::LABEL_PT
            } else {
                0.0
            },
            valign: Align::Center,
            ..Default::default()
        }
    }

    fn spell(self, text: &str) -> String {
        if self == Self::Label {
            text.to_uppercase()
        } else {
            text.to_owned()
        }
    }
}

/// A run of text in one role, laid out on one line, cut short with an ellipsis where it would run past its room.
#[derive(Clone, Copy, Debug)]
pub struct Text<'a> {
    pub role: Role,
    pub words: &'a str,
}

#[must_use]
pub fn text(role: Role, words: &str) -> Text<'_> {
    Text { role, words }
}

impl Text<'_> {
    fn job(self, width: f32) -> LayoutJob {
        let mut job = LayoutJob::default();
        job.append(&self.role.spell(self.words), 0.0, self.role.format());
        job.wrap = egui::text::TextWrapping {
            max_width: width,
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('\u{2026}'),
        };
        job
    }

    fn galley(self, ui: &Ui, width: f32) -> std::sync::Arc<egui::Galley> {
        ui.painter().layout_job(self.job(width))
    }
}

// =====================================================================================================================
// Icons
// =====================================================================================================================

/// The shapes the interface draws beside words, all painted in one slot at one stroke. Nothing here comes from a
/// font, so nothing here can be an empty box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Opens something: the filter menu.
    Plus,
    /// Shuts what [`Icon::Plus`] opened.
    Minus,
    /// Takes something away: the filter a chip stands for.
    Cross,
    /// Puts things back as they were: every filter, a property.
    Restore,
    /// Points the way an order runs, and marks a combo box.
    TriangleDown,
    TriangleUp,
    /// Marks a group that is closed.
    TriangleRight,
    /// Marks a value that is chosen.
    Tick,
    /// The box of a checkbox, which [`Icon::Tick`] is drawn in when the value is chosen.
    Box {
        ticked: bool,
    },
}

impl Icon {
    /// The side of the slot every icon is drawn in.
    pub const SLOT: f32 = 14.0;
    /// The stroke of every icon: a little lighter than a letter's stem at the control size, so that an icon reads
    /// as a mark beside words and not as a bold word among them.
    const STROKE: f32 = 1.2;

    /// What the icon is called in the accessibility tree.
    pub fn name(self) -> &'static str {
        match self {
            Self::Plus => "plus",
            Self::Minus => "minus",
            Self::Cross => "cross",
            Self::Restore => "restore",
            Self::TriangleDown => "triangle down",
            Self::TriangleUp => "triangle up",
            Self::TriangleRight => "triangle right",
            Self::Tick => "tick",
            Self::Box { ticked: true } => "ticked",
            Self::Box { ticked: false } => "unticked",
        }
    }

    fn paint(self, painter: &egui::Painter, slot: Rect, color: Color32) {
        let c = slot.center();
        let r = Self::SLOT * 0.29;
        let stroke = Stroke::new(Self::STROKE, color);
        match self {
            Self::Plus => {
                painter.hline((c.x - r)..=(c.x + r), c.y, stroke);
                painter.vline(c.x, (c.y - r)..=(c.y + r), stroke);
            }
            Self::Minus => {
                painter.hline((c.x - r)..=(c.x + r), c.y, stroke);
            }
            Self::Cross => {
                let d = r * 0.8;
                painter.line_segment([c + egui::vec2(-d, -d), c + egui::vec2(d, d)], stroke);
                painter.line_segment([c + egui::vec2(-d, d), c + egui::vec2(d, -d)], stroke);
            }
            Self::Restore => {
                // An open ring travelling anticlockwise from the top, with its head where it stops at the right.
                let points: Vec<egui::Pos2> = (0..=24)
                    .map(|i| {
                        let t = 280.0_f32.to_radians() - (i as f32 / 24.0) * 270.0_f32.to_radians();
                        c + r * egui::vec2(t.cos(), t.sin())
                    })
                    .collect();
                let end = points[24];
                let t_end = 10.0_f32.to_radians();
                painter.add(egui::Shape::line(points, stroke));
                // The head: two short strokes back from the end, either side of the direction of travel.
                let back = egui::vec2(-t_end.sin(), t_end.cos()) * -1.0;
                let back = egui::vec2(-back.x, -back.y);
                let h = r * 0.7;
                let rotate = |v: Vec2, a: f32| {
                    egui::vec2(v.x * a.cos() - v.y * a.sin(), v.x * a.sin() + v.y * a.cos())
                };
                painter.add(egui::Shape::line(
                    vec![
                        end + h * rotate(back, 0.7),
                        end,
                        end + h * rotate(back, -0.7),
                    ],
                    stroke,
                ));
            }
            Self::TriangleDown | Self::TriangleUp | Self::TriangleRight => {
                let t =
                    Rect::from_center_size(c, egui::vec2(slot.width() * 0.7, slot.height() * 0.45));
                let points = match self {
                    Self::TriangleDown => vec![t.left_top(), t.right_top(), t.center_bottom()],
                    Self::TriangleUp => vec![t.left_bottom(), t.right_bottom(), t.center_top()],
                    _ => {
                        let t = Rect::from_center_size(
                            c,
                            egui::vec2(slot.width() * 0.45, slot.height() * 0.7),
                        );
                        vec![t.left_top(), t.left_bottom(), t.right_center()]
                    }
                };
                painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
            }
            Self::Tick => {
                painter.add(egui::Shape::line(
                    vec![
                        c + egui::vec2(-4.0, 0.0),
                        c + egui::vec2(-1.0, 3.0),
                        c + egui::vec2(4.0, -3.0),
                    ],
                    stroke,
                ));
            }
            Self::Box { ticked } => {
                // The box takes the selection colours when ticked and the widget colours when not; the tick on it
                // is drawn in the brightest text so that it reads on the selection fill.
                let (fill, outline) = if ticked {
                    (style::CHIP_FILL, Stroke::new(1.0, style::CHIP_STROKE))
                } else {
                    (style::WIDGET, Stroke::new(1.0, style::STROKE))
                };
                painter.rect(slot, Spacing::RADIUS, fill, outline, StrokeKind::Inside);
                if ticked {
                    Self::Tick.paint(painter, slot, style::BRIGHT);
                }
            }
        }
    }
}

// =====================================================================================================================
// Spacing and frames
// =====================================================================================================================

pub struct Spacing;

impl Spacing {
    /// The horizontal inset of everything: a block, a row, a well, a panel. One number, so that text lines up down
    /// the panel whatever it sits in.
    pub const INSET: f32 = 10.0;
    /// The room above and below a block of controls.
    pub const BLOCK_Y: f32 = 8.0;
    /// The room above and below the text of a row.
    pub const ROW_Y: f32 = 5.0;
    /// The room above and below a note in an empty list.
    pub const NOTE_Y: f32 = 22.0;
    /// The room between the caption of a control and its edge.
    pub const CONTROL_PADDING: Vec2 = egui::vec2(7.0, 2.0);
    /// The room between the caption of a large control, on the toolbar, and its edge.
    pub const LARGE_CONTROL_PADDING: Vec2 = egui::vec2(9.0, 4.0);
    /// The room between the text of a field and its edge.
    pub const FIELD_PADDING: Vec2 = egui::vec2(7.0, 4.0);
    /// The least height of a control.
    pub const CONTROL_HEIGHT: f32 = 18.0;
    /// The room between things on a row, and between an icon and its words.
    pub const GAP: f32 = 6.0;
    /// The room between the two lines of a row.
    pub const LINE_GAP: f32 = 2.0;
    /// The rounding of a control, a field and a tag.
    pub const RADIUS: u8 = 2;
    /// The rounding of a well.
    pub const WELL_RADIUS: u8 = 4;

    /// A margin of `x` sideways and `y` above and below, in the whole points egui frames take.
    #[allow(clippy::cast_possible_truncation)]
    const fn margin(x: f32, y: f32) -> egui::Margin {
        egui::Margin::symmetric(x as i8, y as i8)
    }

    /// The margin of a block: the inset sideways, the block room above and below.
    const fn block() -> egui::Margin {
        Self::margin(Self::INSET, Self::BLOCK_Y)
    }
}

pub fn block<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(Spacing::block())
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

/// A well: a frame a shade below the panel, which the filter menu sits in. It stands a block's inset from the
/// panel's sides, nothing from the row above it, a block's room from what follows, and holds its contents in a
/// block's margin.
pub fn well<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(style::MENU_FILL)
        .stroke(Stroke::new(1.0, style::STROKE))
        .corner_radius(Spacing::WELL_RADIUS)
        .outer_margin(egui::Margin {
            top: 0,
            ..Spacing::block()
        })
        .inner_margin(Spacing::block())
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

/// A row with a label at its left and controls at its right, the first control added being the rightmost.
pub fn captioned_row<R>(ui: &mut Ui, caption: &str, controls: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        label(ui, text(Role::Label, caption));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), controls)
            .inner
    })
    .inner
}

#[derive(Clone, Copy, Debug)]
pub enum PanelKind {
    Bare,
    AboveRule,
    BelowRule,
    Foot,
    Details,
}

impl PanelKind {
    pub fn frame(self, ui: &Ui) -> egui::Frame {
        let fill = ui.visuals().panel_fill;
        let rule = egui::Margin {
            top: 0,
            bottom: 0,
            ..Spacing::margin(0.0, Spacing::GAP)
        };
        match self {
            Self::Bare => egui::Frame::new().fill(fill),
            Self::AboveRule => egui::Frame::new().fill(fill).inner_margin(egui::Margin {
                bottom: Spacing::margin(0.0, Spacing::GAP).bottom,
                ..rule
            }),
            Self::BelowRule => egui::Frame::new().fill(fill).inner_margin(egui::Margin {
                top: Spacing::margin(0.0, Spacing::GAP).top,
                ..rule
            }),
            Self::Foot => egui::Frame::new()
                .fill(style::FOOT_FILL)
                .inner_margin(Spacing::margin(Spacing::INSET, Spacing::ROW_Y)),
            Self::Details => egui::Frame::new()
                .fill(fill)
                .inner_margin(Spacing::margin(Spacing::INSET, Spacing::BLOCK_Y)),
        }
    }
}

// =====================================================================================================================
// Controls
// =====================================================================================================================

/// How a control's frame is drawn.
#[derive(Clone, Copy, Debug)]
pub enum Face {
    /// A button: the widget fill at rest, egui's hovered and active fills under the pointer.
    Raised,
    /// A quiet button: no frame at rest, the widget fill under the pointer.
    Quiet,
    /// A tinted control, such as a chip or a tag: named colours that do not follow the state visuals.
    Tinted(Tint),
}

#[derive(Clone, Copy, Debug)]
pub struct Tint {
    pub fill: Color32,
    pub hover: Color32,
    pub stroke: Color32,
    pub text: Color32,
    pub data: Color32,
}

impl Tint {
    /// The tint of a chip that narrows a parameter.
    pub const PARAMETER: Self = Self {
        fill: style::CHIP_FILL,
        hover: style::CHIP_HOVER,
        stroke: style::CHIP_STROKE,
        text: style::CHIP_TEXT,
        data: style::CHIP_KEY,
    };
    pub const LABEL: Self = Self {
        fill: style::LABEL_CHIP_FILL,
        hover: style::LABEL_CHIP_HOVER,
        stroke: style::LABEL_CHIP_STROKE,
        text: style::LABEL_CHIP_TEXT,
        data: style::LABEL_CHIP_KEY,
    };
}

/// The size of a control: the regular size of the browser's controls, or the large size of the toolbar's, whose
/// buttons stand alone above the canvas and take more room around their words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Regular,
    Large,
}

impl Size {
    fn padding(self) -> Vec2 {
        match self {
            Self::Regular => Spacing::CONTROL_PADDING,
            Self::Large => Spacing::LARGE_CONTROL_PADDING,
        }
    }
}

/// A control: words, an optional datum before them, an optional icon before or after them, a face, a size and a
/// state. Every button, chip, tag and tool of the interface is one of these, so they are all one padding at their
/// size and colour themselves by one rule.
#[derive(Clone, Debug)]
pub struct Control<'a> {
    words: Text<'a>,
    datum: Option<&'a str>,
    leading: Option<Icon>,
    trailing: Option<Icon>,
    face: Face,
    size: Size,
    selected: bool,
    enabled: bool,
    clickable: bool,
    /// What the control is to the accessibility tree: a button, or the box of a combo.
    kind: egui::WidgetType,
    /// A width the control is given rather than takes from its words, as a combo box is.
    width: Option<f32>,
}

impl<'a> Control<'a> {
    /// A raised button.
    #[must_use]
    pub fn button(words: &'a str) -> Self {
        Self::new(text(Role::Control, words), Face::Raised)
    }

    #[must_use]
    pub fn new(words: Text<'a>, face: Face) -> Self {
        Self {
            words,
            datum: None,
            leading: None,
            trailing: None,
            face,
            size: Size::Regular,
            selected: false,
            enabled: true,
            clickable: true,
            kind: egui::WidgetType::Button,
            width: None,
        }
    }

    /// The large size, for the toolbar.
    #[must_use]
    pub fn large(mut self) -> Self {
        self.size = Size::Large;
        self
    }

    /// A chip: a tinted control naming what it narrows in data before its words, with a cross after them.
    #[must_use]
    pub fn chip(name: &'a str, value: &'a str, tint: Tint) -> Self {
        Self::new(text(Role::Control, value), Face::Tinted(tint))
            .datum(name)
            .after(Icon::Cross)
    }

    /// A tag: a tinted control that is read and not clicked.
    #[must_use]
    pub fn tag(words: &'a str) -> Self {
        let mut tag = Self::new(text(Role::Data, words), Face::Tinted(Tint::LABEL));
        tag.clickable = false;
        tag
    }

    #[must_use]
    pub fn quiet(mut self) -> Self {
        self.face = Face::Quiet;
        self
    }

    #[must_use]
    pub fn datum(mut self, datum: &'a str) -> Self {
        self.datum = Some(datum);
        self
    }

    #[must_use]
    pub fn before(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
        self
    }

    #[must_use]
    pub fn after(mut self, icon: Icon) -> Self {
        self.trailing = Some(icon);
        self
    }

    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The label of the control for the accessibility tree: its datum, its words, and the name of its icons.
    #[must_use]
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(icon) = self.leading {
            parts.push(icon.name().to_owned());
        }
        if let Some(datum) = self.datum {
            parts.push(format!("{datum}:"));
        }
        parts.push(self.words.words.to_owned());
        if let Some(icon) = self.trailing {
            parts.push(icon.name().to_owned());
        }
        parts.join(" ")
    }

    /// The one size rule: the words or the icon, whichever is taller, plus the padding, and never below the
    /// control height. A combo box comes out the same, so a control is level with the box beside it.
    fn size(
        words: &egui::Galley,
        datum: Option<&egui::Galley>,
        icons: usize,
        padding: Vec2,
    ) -> Vec2 {
        let icons_w = icons as f32 * (Icon::SLOT + Spacing::GAP);
        let datum_w = datum.map_or(0.0, |d| d.size().x + Spacing::GAP);
        let inner = egui::vec2(
            words.size().x + datum_w + icons_w,
            words.size().y.max(if icons > 0 { Icon::SLOT } else { 0.0 }),
        );
        let size = inner + 2.0 * padding;
        egui::vec2(size.x, size.y.max(Spacing::CONTROL_HEIGHT))
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let padding = self.size.padding();
        let room = ui.available_width() - 2.0 * padding.x;
        let datum = self.datum.map(|d| text(Role::Data, d).galley(ui, room));
        let words = self.words.galley(ui, room);
        let icons = usize::from(self.leading.is_some()) + usize::from(self.trailing.is_some());
        let mut size = Self::size(&words, datum.as_deref(), icons, padding);
        if let Some(width) = self.width {
            size.x = width;
        }
        let sense = if self.enabled && self.clickable {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(size, sense);
        let label = self.label();
        let (enabled, selected) = (self.enabled, self.selected);
        let kind = self.kind;
        response.widget_info(|| egui::WidgetInfo::selected(kind, enabled, selected, label.clone()));
        if ui.is_rect_visible(rect) {
            self.paint(ui, &response, rect, words, datum);
        }
        response
    }

    /// The one frame rule: the fill and outline of the state, on the rectangle exactly, and everything on it in
    /// the state's text colour. Nothing expands under the pointer.
    fn paint(
        &self,
        ui: &Ui,
        response: &Response,
        rect: Rect,
        words: std::sync::Arc<egui::Galley>,
        datum: Option<std::sync::Arc<egui::Galley>>,
    ) {
        let visuals = *ui.style().interact(response);
        let hovered = response.hovered() && self.clickable;
        let (fill, stroke, text_color, data_color) = match self.face {
            Face::Tinted(tint) => (
                if hovered { tint.hover } else { tint.fill },
                Stroke::new(1.0, tint.stroke),
                tint.text,
                tint.data,
            ),
            Face::Raised if self.selected => {
                let strong = ui.visuals().strong_text_color();
                (ui.visuals().selection.bg_fill, Stroke::NONE, strong, strong)
            }
            Face::Raised => (
                visuals.weak_bg_fill,
                visuals.bg_stroke,
                visuals.text_color(),
                ui.visuals().weak_text_color(),
            ),
            Face::Quiet => {
                let weak = ui.visuals().weak_text_color();
                (
                    if hovered {
                        visuals.weak_bg_fill
                    } else {
                        Color32::TRANSPARENT
                    },
                    Stroke::NONE,
                    weak,
                    weak,
                )
            }
        };
        let fade = if self.enabled {
            1.0
        } else {
            ui.visuals().disabled_alpha
        };
        let (text_color, data_color) = (
            text_color.gamma_multiply(fade),
            data_color.gamma_multiply(fade),
        );
        let painter = ui.painter();
        painter.rect(
            rect,
            Spacing::RADIUS,
            fill.gamma_multiply(fade),
            stroke,
            StrokeKind::Inside,
        );

        let inner = rect.shrink2(self.size.padding());
        let mut x = inner.min.x;
        if let Some(icon) = self.leading {
            icon.paint(
                painter,
                Rect::from_center_size(
                    egui::pos2(x + Icon::SLOT / 2.0, inner.center().y),
                    Vec2::splat(Icon::SLOT),
                ),
                text_color,
            );
            x += Icon::SLOT + Spacing::GAP;
        }
        if let Some(datum) = datum {
            painter.galley(
                egui::pos2(x, inner.center().y - datum.size().y / 2.0),
                datum.clone(),
                data_color,
            );
            x += datum.size().x + Spacing::GAP;
        }
        painter.galley(
            egui::pos2(x, inner.center().y - words.size().y / 2.0),
            words,
            text_color,
        );
        if let Some(icon) = self.trailing {
            let slot = Align2::RIGHT_CENTER.align_size_within_rect(Vec2::splat(Icon::SLOT), inner);
            icon.paint(painter, slot, text_color);
        }
    }
}

/// A combo box: a control showing what is chosen, with the down triangle at its right, which opens a menu of
/// `add_contents` beneath it. It is the same control as every button, so it is level with the control beside it
/// by construction rather than by matching egui's spacing to ours.
pub fn combo<R>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    selected: &str,
    width: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let mut control = Control::button(selected).after(Icon::TriangleDown);
    control.kind = egui::WidgetType::ComboBox;
    control.width = Some(width);
    let response = control.show(ui);
    let salt = egui::Id::new(id);
    egui::Popup::menu(&response)
        .id(salt)
        .width(width)
        .show(add_contents)
        .map(|inner| inner.inner)
}

/// A run of text on its own: ordinary text, or weak text for a label or a datum.
pub fn label(ui: &mut Ui, text: Text<'_>) -> Response {
    let color = if text.role.weak() {
        ui.visuals().weak_text_color()
    } else {
        ui.visuals().text_color()
    };
    // A label extends to its words, as egui's own does: it is what a grid measures its columns by, and a grid
    // starts a column at forty points, which a label that fitted itself to the room would never let grow. What
    // must be cut short to fit is a row, which is given its width.
    let galley = text.galley(ui, f32::INFINITY);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    let words = text.words.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, words.clone()));
    if ui.is_rect_visible(rect) {
        ui.painter().galley(rect.min, galley, color);
    }
    response
}

/// A single-line text field as wide as the room it is given, in body text, which takes the selection colour
/// while it has focus. Every field of the interface is this one.
pub fn field(ui: &mut Ui, value: &mut String, hint: &str, id: egui::Id) -> Response {
    let focused = ui.memory(|memory| memory.has_focus(id));
    let stroke = if focused {
        ui.visuals().selection.stroke
    } else {
        Stroke::new(1.0, style::STROKE)
    };
    egui::Frame::new()
        .fill(style::FIELD)
        .stroke(stroke)
        .corner_radius(Spacing::RADIUS)
        .inner_margin(Spacing::margin(
            Spacing::FIELD_PADDING.x,
            Spacing::FIELD_PADDING.y,
        ))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .id(id)
                    .hint_text(hint)
                    .font(Role::Body.font())
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .desired_width(f32::INFINITY),
            )
        })
        .inner
}

// =====================================================================================================================
// Rows
// =====================================================================================================================

/// What leads a row: nothing, a disclosure triangle, or a checkbox.
#[derive(Clone, Copy, Debug)]
pub enum Leading {
    None,
    Disclosure { open: bool },
    Check { ticked: bool },
}

/// The data beside a row's title: none, beneath it, or at the right edge. Data at the right edge lines up down a
/// list, which data set after the title, at whatever width the title takes, never does.
#[derive(Clone, Copy, Debug)]
pub enum Detail<'a> {
    None,
    Beneath(&'a str),
    Trailing(&'a str),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RowState {
    pub selected: bool,
    pub striped: bool,
    pub band: bool,
    pub faded: bool,
}

/// A row of a list or a menu. The rows of the figure list, the headings of its groups, the entries of the menu
/// and its checkboxes are all this one row.
#[derive(Clone, Debug)]
pub struct Row<'a> {
    pub title: Text<'a>,
    pub detail: Detail<'a>,
    pub leading: Leading,
    pub state: RowState,
    /// What the row is called in the accessibility tree when its title and detail do not say enough, such as a
    /// heading whose trailing count needs its unit.
    pub spoken: Option<String>,
}

impl<'a> Row<'a> {
    /// A row with nothing leading it and no special state.
    #[must_use]
    pub fn new(title: Text<'a>, detail: Detail<'a>) -> Self {
        Self {
            title,
            detail,
            leading: Leading::None,
            state: RowState::default(),
            spoken: None,
        }
    }

    #[must_use]
    pub fn leading(mut self, leading: Leading) -> Self {
        self.leading = leading;
        self
    }

    #[must_use]
    pub fn state(mut self, state: RowState) -> Self {
        self.state = state;
        self
    }

    #[must_use]
    pub fn spoken(mut self, spoken: String) -> Self {
        self.spoken = Some(spoken);
        self
    }

    /// The height of a row: a line of the title, a second line of data when the detail is beneath, and the
    /// row's padding. Rows of a list are all one height so that the list can find a row by multiplying.
    #[must_use]
    pub fn height(ui: &Ui, two_lines: bool) -> f32 {
        let title = ui.fonts_mut(|fonts| fonts.row_height(&Role::Body.font()));
        let second = if two_lines {
            Spacing::LINE_GAP + ui.fonts_mut(|fonts| fonts.row_height(&Role::Data.font()))
        } else {
            0.0
        };
        title + second + 2.0 * Spacing::ROW_Y
    }

    pub fn show(self, ui: &mut Ui, height: f32) -> Response {
        let sense = if self.state.faded {
            Sense::hover()
        } else {
            Sense::click()
        };
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), height), sense);
        let label = self.label();
        let (enabled, selected) = (!self.state.faded, self.state.selected);
        let kind = match self.leading {
            Leading::Check { .. } => egui::WidgetType::Checkbox,
            _ => egui::WidgetType::SelectableLabel,
        };
        response.widget_info(|| egui::WidgetInfo::selected(kind, enabled, selected, label.clone()));
        if ui.is_rect_visible(rect) {
            self.paint(ui, &response, rect);
        }
        response
    }

    fn label(&self) -> String {
        if let Some(spoken) = &self.spoken {
            return spoken.clone();
        }
        match self.detail {
            Detail::None => self.title.words.to_owned(),
            Detail::Beneath(d) | Detail::Trailing(d) => {
                format!("{}, {d}", self.title.words)
            }
        }
    }

    fn paint(&self, ui: &Ui, response: &Response, rect: Rect) {
        let visuals = ui.visuals();
        let fade = if self.state.faded {
            style::DISABLED_ALPHA
        } else {
            1.0
        };
        let hovered = response.hovered() && !self.state.faded;
        let (fill, title_color, data_color) = if self.state.selected {
            (
                visuals.selection.bg_fill,
                visuals.strong_text_color(),
                style::SELECTED_DETAIL,
            )
        } else if hovered {
            (
                style::WIDGET,
                visuals.text_color(),
                visuals.weak_text_color(),
            )
        } else if self.state.band {
            (
                style::GROUP_FILL,
                visuals.weak_text_color(),
                visuals.weak_text_color(),
            )
        } else if self.state.striped {
            (
                visuals.faint_bg_color,
                visuals.text_color(),
                visuals.weak_text_color(),
            )
        } else {
            (
                Color32::TRANSPARENT,
                visuals.text_color(),
                visuals.weak_text_color(),
            )
        };
        let (title_color, data_color) = (
            title_color.gamma_multiply(fade),
            data_color.gamma_multiply(fade),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, fill);
        if self.state.band {
            let rule = Stroke::new(1.0, style::STROKE);
            painter.hline(rect.x_range(), rect.top(), rule);
            painter.hline(rect.x_range(), rect.bottom(), rule);
        }
        if self.state.selected {
            painter.rect_filled(
                Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())),
                0.0,
                visuals.selection.stroke.color,
            );
        }

        let mut x = rect.min.x + Spacing::INSET;
        let slot = |x: f32| {
            Rect::from_center_size(
                egui::pos2(x + Icon::SLOT / 2.0, rect.center().y),
                Vec2::splat(Icon::SLOT),
            )
        };
        match self.leading {
            Leading::None => {}
            Leading::Disclosure { open } => {
                (if open {
                    Icon::TriangleDown
                } else {
                    Icon::TriangleRight
                })
                .paint(painter, slot(x), title_color);
                x += Icon::SLOT + Spacing::GAP;
            }
            Leading::Check { ticked } => {
                Icon::Box { ticked }.paint(painter, slot(x), title_color);
                x += Icon::SLOT + Spacing::GAP;
            }
        }

        let mut right = rect.max.x - Spacing::INSET;
        if let Detail::Trailing(count) = self.detail {
            let galley = text(Role::Data, count).galley(ui, f32::INFINITY);
            right -= galley.size().x;
            painter.galley(
                egui::pos2(right, rect.center().y - galley.size().y / 2.0),
                galley,
                data_color,
            );
            right -= Spacing::GAP;
        }

        match self.detail {
            Detail::Beneath(detail) => {
                let origin = egui::pos2(x, rect.min.y + Spacing::ROW_Y);
                let title = self.title.galley(ui, right - x);
                let title_height = title.size().y;
                painter.galley(origin, title, title_color);
                let detail = text(Role::Data, detail).galley(ui, right - x);
                painter.galley(
                    origin + egui::vec2(0.0, title_height + Spacing::LINE_GAP),
                    detail,
                    data_color,
                );
            }
            Detail::None | Detail::Trailing(_) => {
                let title = self.title.galley(ui, right - x);
                painter.galley(
                    egui::pos2(x, rect.center().y - title.size().y / 2.0),
                    title,
                    title_color,
                );
            }
        }
    }
}

// =====================================================================================================================
// The rest
// =====================================================================================================================

/// A histogram of `counts` across the room available, in the kept colour where a bin lies within the range.
pub fn histogram(
    ui: &mut Ui,
    counts: &[(f64, usize)],
    (least, most): (f64, f64),
    (low, high): (f64, f64),
) {
    const BINS: usize = 18;
    const HEIGHT: f32 = 26.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let span = if most > least { most - least } else { 1.0 };
    let mut tally = [0usize; BINS];
    for (value, count) in counts {
        let bin = ((value - least) / span * BINS as f64).floor();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bin = (bin.max(0.0) as usize).min(BINS - 1);
        tally[bin] += count;
    }
    let peak = tally.iter().copied().max().unwrap_or(0).max(1);
    #[allow(clippy::cast_precision_loss)]
    let bins = BINS as f32;
    let bar = (rect.width() - (bins - 1.0)) / bins;
    let painter = ui.painter();
    for (index, count) in tally.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let (index_f, count_f, peak_f) = (index as f32, *count as f32, peak as f32);
        let height = (count_f / peak_f * HEIGHT).round().max(1.0);
        let x = rect.min.x + index_f * (bar + 1.0);
        let centre = least + (f64::from(index_f) + 0.5) / f64::from(bins) * span;
        let kept = centre >= low && centre <= high;
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x, rect.max.y - height),
                egui::pos2(x + bar, rect.max.y),
            ),
            0.0,
            if kept {
                style::HISTOGRAM_KEPT
            } else {
                style::HISTOGRAM_BAR
            },
        );
    }
}

/// A note in the middle of an empty list: what is the case, and what to do about it.
pub fn note(ui: &mut Ui, title: &str, what_to_do: &str) {
    egui::Frame::new()
        .inner_margin(Spacing::margin(Spacing::INSET, Spacing::NOTE_Y))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                label(ui, text(Role::Body, title));
                ui.add_space(Spacing::LINE_GAP);
                label(ui, text(Role::Control, what_to_do));
            });
        });
}

/// The surround of the canvas and the shadow the page of a figure casts on it.
pub fn surround(painter: &egui::Painter, area: Rect, page: Option<Rect>) {
    painter.rect_filled(area, 0.0, style::SURROUND);
    if let Some(page) = page {
        let shadow = egui::Shadow {
            offset: [0, 8],
            blur: 26,
            spread: 0,
            color: Color32::from_black_alpha(160),
        };
        painter.add(shadow.as_shape(page.shrink(12.0), egui::CornerRadius::ZERO));
    }
}
