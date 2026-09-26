//! The widgets the viewer's interface is drawn with.
//!
//! The interface is drawn from a small vocabulary, and every part of it is spelled from that vocabulary and
//! nothing else:
//!
//! - **Four roles of text at three sizes** ([`Role`]). Words are set in the proportional face, data in the
//!   monospaced one, and the scale has three steps: body, control, and label. Nothing names a size of its own.
//! - **One icon vocabulary** ([`Icon`]), painted rather than typed: a handful of shapes at one size and one
//!   stroke, in the colour of the text they stand beside. No mark depends on a font.
//! - **Three components** ([`Control`], [`Row`], [`field`]) and the frames they sit in. A button, a chip, a tag,
//!   a tool, a colour swatch and the control that reverses an order are one control; a row of the list, a
//!   heading, a menu entry, a choice and a checkbox are one row; a field of words and a field of a number are one
//!   field.
//! - **One row for every property** ([`Property`]): a name, a control and a place for the restore control, in
//!   three columns that every row shares, so that the property editor is read down its columns.
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
    /// A square of one colour: the colour a property holds, on the control that opens its picker.
    Swatch(Color32),
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
            Self::Swatch(_) => "swatch",
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
            Self::Swatch(fill) => {
                // The colour is shown as it is, with the outline every control has, so that a colour close to the
                // panel is still seen to be there. A colour with transparency is drawn over a chequer, because a
                // wash over the panel would read as a darker opaque colour.
                if fill.a() < 255 {
                    let half = slot.width() / 2.0;
                    let light = Color32::from_gray(90);
                    let dark = Color32::from_gray(50);
                    painter.rect_filled(slot, Spacing::RADIUS, dark);
                    painter.rect_filled(
                        Rect::from_min_size(slot.min, Vec2::splat(half)),
                        0.0,
                        light,
                    );
                    painter.rect_filled(
                        Rect::from_min_size(slot.center(), Vec2::splat(half)),
                        0.0,
                        light,
                    );
                }
                painter.rect(
                    slot,
                    Spacing::RADIUS,
                    fill,
                    Stroke::new(1.0, style::STROKE),
                    StrokeKind::Inside,
                );
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
    /// The width of the column of names in a property row. Every name is drawn within it and every control begins
    /// where it ends, so that the controls of a node form one column however deeply their properties are nested.
    pub const NAME_COLUMN: f32 = 116.0;
    /// The indent of one level of nesting: in a tree, and among the properties of a group. It is also the slot the
    /// disclosure triangle of a property that gathers others is drawn in, so that every name stands one indent in
    /// from the edge and a triangle takes no room from the names.
    pub const INDENT: f32 = 12.0;
    /// The least width the column of names keeps for its words, however deep the indent.
    pub const NAME_MIN: f32 = 32.0;
    /// The least width the column of controls keeps when the panel is too narrow to give every column its share.
    /// The room a narrow panel needs is taken from the controls, which are still the same controls in less room,
    /// rather than from the names, which are what make a row findable at all.
    pub const CONTROL_MIN: f32 = 56.0;

    /// The width of the column at the right of every property row that holds the restore control: the control
    /// itself, which is one icon in a control's padding.
    #[must_use]
    pub fn restore_column() -> f32 {
        Icon::SLOT + 2.0 * Self::CONTROL_PADDING.x
    }

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
    /// What the control is called in the accessibility tree when its words do not say enough: a control that is an
    /// icon alone.
    spoken: Option<String>,
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
            spoken: None,
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

    /// Names the control in the accessibility tree, for a control whose words do not say enough.
    #[must_use]
    pub fn spoken(mut self, spoken: impl Into<String>) -> Self {
        self.spoken = Some(spoken.into());
        self
    }

    /// The label of the control for the accessibility tree: what it was told to say, or else its datum, its words,
    /// and the name of its icons.
    #[must_use]
    pub fn label(&self) -> String {
        if let Some(spoken) = &self.spoken {
            return spoken.clone();
        }
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
        let shown = (kind == egui::WidgetType::ComboBox).then(|| self.words.words.to_owned());
        response.widget_info(|| egui::WidgetInfo {
            current_text_value: shown.clone(),
            ..egui::WidgetInfo::selected(kind, enabled, selected, label.clone())
        });
        if ui.is_rect_visible(rect) {
            self.paint(ui, &response, rect, words, datum);
        }
        response
    }

    /// The height of a regular control carrying an icon, which is the height of every control on a row of them.
    #[must_use]
    pub fn height(ui: &Ui) -> f32 {
        let words = ui.fonts_mut(|fonts| fonts.row_height(&Role::Control.font()));
        (words.max(Icon::SLOT) + 2.0 * Spacing::CONTROL_PADDING.y).max(Spacing::CONTROL_HEIGHT)
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
        .show(|ui| {
            // The menu is a list of rows, each carrying its own padding, so nothing stands between them.
            ui.spacing_mut().item_spacing.y = 0.0;
            add_contents(ui)
        })
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
    /// The row is greyed and cannot be clicked: a value the filters leave nothing of.
    pub faded: bool,
    /// The row is greyed and can still be clicked: a plot that is hidden, which is selected to be shown again.
    pub dimmed: bool,
}

/// A row of a list or a menu. The rows of the figure list, the headings of its groups, the entries of the menu
/// and its checkboxes are all this one row.
#[derive(Clone, Debug)]
pub struct Row<'a> {
    pub title: Text<'a>,
    pub detail: Detail<'a>,
    pub leading: Leading,
    pub state: RowState,
    /// How many levels of nesting the row stands in from the edge, each one [`Spacing::INDENT`].
    pub depth: usize,
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
            depth: 0,
            spoken: None,
        }
    }

    /// Sets the row in from the edge by `depth` levels of nesting.
    #[must_use]
    pub fn indent(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
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
        let (fill, mut title_color, data_color) = if self.state.selected {
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
        if self.state.dimmed && !self.state.selected {
            title_color = visuals.weak_text_color();
        }
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

        #[allow(clippy::cast_precision_loss)]
        let mut x = rect.min.x + Spacing::INSET + self.depth as f32 * Spacing::INDENT;
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
// The property editor
// =====================================================================================================================

/// A heading: words that name what follows, in spaced capitals, with a datum at the right of the same line when
/// there is one to give — the count of what follows, or the name of the node whose properties follow.
///
/// The words are read as they were written, not in the capitals they are spelled in, and the datum is read as a
/// label of its own.
pub fn heading(ui: &mut Ui, words: &str, datum: Option<&str>) -> Response {
    block(ui, |ui| {
        ui.horizontal(|ui| {
            let response = label(ui, text(Role::Label, words));
            if let Some(datum) = datum {
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    label(ui, text(Role::Data, datum));
                });
            }
            response
        })
        .inner
    })
}

/// A row of the property editor: the name of a property, a control that changes it, and a place for the control
/// that takes the change back.
///
/// Every row is laid out in the same three columns, so that a panel of them is read down the columns: the name at
/// the left, set in by how deeply the property is nested; the control in the column beside it, so that the controls
/// of a node line up with one another; and the restore control in a column of its own at the right, which is
/// reserved whether or not the row is changed, so that no control shifts sideways when a property becomes changed.
///
/// A property the reader has changed is marked by its name, drawn in the colour a chip draws the name of what it
/// narrows: the one colour the interface uses for what the reader has chosen.
///
/// A property that gathers others, such as the style of a line, is a row like any other, in the same text, with a
/// disclosure triangle before its name that opens and closes the rows beneath it; those rows are set in by one
/// indent, with a fine line down their left from the triangle, as the rows beneath a node of the object tree
/// are. Every name stands one indent in from the edge, so that a name with a triangle and a name without begin
/// at one place.
#[derive(Clone, Debug)]
pub struct Property<'a> {
    name: &'a str,
    depth: usize,
    changed: bool,
    striped: bool,
    docs: Option<&'a str>,
    /// Whether the property gathers the rows beneath it, and whether they are shown.
    disclosure: Option<bool>,
}

/// What a property row reports: what its control returned, whether its restore control was clicked, and the
/// response of its name, which a tooltip may be hung on.
#[derive(Debug)]
pub struct PropertyResponse<R> {
    pub inner: R,
    pub restore: bool,
    pub name: Response,
}

impl<'a> Property<'a> {
    #[must_use]
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            depth: 0,
            changed: false,
            striped: false,
            docs: None,
            disclosure: None,
        }
    }

    /// Makes the property one that gathers the rows beneath it, showing them while `open`. Clicking its name
    /// reports through [`PropertyResponse::name`], and what is drawn beneath it is the caller's to decide.
    #[must_use]
    pub fn disclosure(mut self, open: bool) -> Self {
        self.disclosure = Some(open);
        self
    }

    /// How deeply the property is nested beneath the group it belongs to.
    #[must_use]
    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    /// Whether the reader has changed the property, which marks its name and offers the restore control.
    #[must_use]
    pub fn changed(mut self, changed: bool) -> Self {
        self.changed = changed;
        self
    }

    /// Whether the row is one of the every-second rows drawn on the stripe.
    #[must_use]
    pub fn striped(mut self, striped: bool) -> Self {
        self.striped = striped;
        self
    }

    /// What the property means, shown when its name is hovered.
    #[must_use]
    pub fn docs(mut self, docs: &'a str) -> Self {
        self.docs = Some(docs);
        self
    }

    /// The height of every property row: a field, which is the tallest control a row holds, and a line's gap
    /// above and below it.
    #[must_use]
    pub fn height(ui: &Ui) -> f32 {
        field_height(ui) + 2.0 * Spacing::LINE_GAP
    }

    pub fn show<R>(self, ui: &mut Ui, control: impl FnOnce(&mut Ui) -> R) -> PropertyResponse<R> {
        let height = Self::height(ui);
        let width = ui.available_width();
        let (rect, allocated) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
        let visible = ui.is_rect_visible(rect);
        if visible && self.striped {
            ui.painter()
                .rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
        }

        // The columns: the name takes its width, the restore control takes its width, and the control takes what is
        // left, down to the least it keeps.
        let restore_width = Spacing::restore_column();
        let room = width - 2.0 * Spacing::INSET - 2.0 * Spacing::GAP;
        let control_width = (room - Spacing::NAME_COLUMN - restore_width).max(Spacing::CONTROL_MIN);
        let name_rect = Rect::from_min_size(
            egui::pos2(rect.min.x + Spacing::INSET, rect.min.y),
            egui::vec2(Spacing::NAME_COLUMN, height),
        );
        let control_rect = Rect::from_min_size(
            egui::pos2(
                name_rect.max.x + Spacing::GAP,
                rect.min.y + Spacing::LINE_GAP,
            ),
            egui::vec2(control_width, height - 2.0 * Spacing::LINE_GAP),
        );
        let restore_rect = Rect::from_min_size(
            egui::pos2(control_rect.max.x + Spacing::GAP, rect.min.y),
            egui::vec2(restore_width, height),
        );

        // The name: one indent in, and one more for every level of nesting, cut short to what is left of its
        // column, and a label in the accessibility tree so that it can be found and hovered. It takes its identity
        // from the row's place in the panel rather than from its words, because three axes each have a scale. A
        // name with a triangle before it is a button that takes the triangle's slot too.
        #[allow(clippy::cast_precision_loss)]
        let indent = ((self.depth + 1) as f32 * Spacing::INDENT)
            .min(Spacing::NAME_COLUMN - Spacing::NAME_MIN);
        let (name_left, sense, kind) = match self.disclosure {
            Some(_) => (
                name_rect.min.x + indent - Spacing::INDENT,
                Sense::click(),
                egui::WidgetType::CollapsingHeader,
            ),
            None => (
                name_rect.min.x + indent,
                Sense::hover(),
                egui::WidgetType::Label,
            ),
        };
        let name = ui.interact(
            Rect::from_min_max(egui::pos2(name_left, name_rect.min.y), name_rect.max),
            allocated.id.with("name"),
            sense,
        );
        let words = self.name.to_owned();
        name.widget_info(|| egui::WidgetInfo::labeled(kind, true, words.clone()));
        if visible {
            let painter = ui.painter();
            let color = if self.changed {
                style::CHIP_KEY
            } else if self.disclosure.is_some() && name.hovered() {
                ui.visuals().text_color()
            } else {
                ui.visuals().weak_text_color()
            };
            // The triangle sits in the slot before the name; the guide of a nested row runs down that slot's
            // middle, from the triangle of the row that gathers it.
            let slot_x = name_rect.min.x + indent - Spacing::INDENT / 2.0;
            if let Some(open) = self.disclosure {
                let icon = if open {
                    Icon::TriangleDown
                } else {
                    Icon::TriangleRight
                };
                // The triangle is drawn a little smaller than an icon, as the tree's is, so that it stands clear
                // of the name in a slot one indent wide.
                icon.paint(
                    painter,
                    Rect::from_center_size(
                        egui::pos2(slot_x, name_rect.center().y),
                        Vec2::splat(Icon::SLOT * 0.75),
                    ),
                    color,
                );
            }
            if self.depth > 0 {
                painter.vline(
                    name_rect.min.x + Spacing::INDENT / 2.0,
                    rect.y_range(),
                    Stroke::new(1.0, style::STROKE),
                );
            }
            let galley = text(Role::Data, self.name).galley(ui, Spacing::NAME_COLUMN - indent);
            painter.galley(
                egui::pos2(
                    name_rect.min.x + indent,
                    name_rect.center().y - galley.size().y / 2.0,
                ),
                galley,
                color,
            );
        }
        let name = match self.docs {
            Some(docs) => hint(name, docs),
            None => name,
        };

        // The columns are drawn in children that allocate nothing in the row, so that the row advances by its
        // own height whatever its columns hold: a control, a control that has been changed, or nothing at all.
        let mut column = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(allocated.id.with("control"))
                .max_rect(control_rect)
                .layout(egui::Layout::left_to_right(Align::Center)),
        );
        column.spacing_mut().item_spacing = egui::vec2(Spacing::GAP, 0.0);
        let inner = control(&mut column);

        let restore = self.changed && {
            let mut column = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(allocated.id.with("restore"))
                    .max_rect(restore_rect)
                    .layout(egui::Layout::centered_and_justified(
                        egui::Direction::LeftToRight,
                    )),
            );
            Control::new(text(Role::Control, ""), Face::Quiet)
                .before(Icon::Restore)
                .spoken(format!("Revert {}", self.name))
                .show(&mut column)
                .clicked()
        };

        PropertyResponse {
            inner,
            restore,
            name,
        }
    }
}

/// A checkbox on its own, in the column of controls: the box of a checked row, without a row. It is named in the
/// accessibility tree by `spoken`, the name of what it changes, because it carries no caption of its own.
pub fn checkbox(ui: &mut Ui, ticked: &mut bool, spoken: &str) -> Response {
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::splat(Spacing::CONTROL_HEIGHT), Sense::click());
    if response.clicked() {
        *ticked = !*ticked;
        response.mark_changed();
    }
    let (enabled, state, words) = (ui.is_enabled(), *ticked, spoken.to_owned());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, state, words.clone())
    });
    if ui.is_rect_visible(rect) {
        let slot = Rect::from_center_size(rect.center(), Vec2::splat(Icon::SLOT));
        let painter = ui.painter();
        Icon::Box { ticked: *ticked }.paint(painter, slot, ui.visuals().text_color());
        if response.hovered() && !*ticked {
            painter.rect_stroke(
                slot,
                Spacing::RADIUS,
                Stroke::new(1.0, ui.visuals().weak_text_color()),
                StrokeKind::Inside,
            );
        }
    }
    response
}

/// How a number field behaves: how far a point of pointer travel moves the value, the range the value is held to,
/// and whether it is held to whole numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Number {
    pub speed: f64,
    pub range: Option<(f64, f64)>,
    pub integer: bool,
}

impl Number {
    /// A real number, moved by `speed` per point of pointer travel.
    #[must_use]
    pub fn real(speed: f64) -> Self {
        Self {
            speed,
            range: None,
            integer: false,
        }
    }

    /// A whole number, moved by one per point of pointer travel.
    #[must_use]
    pub fn integer() -> Self {
        Self {
            speed: 1.0,
            range: None,
            integer: true,
        }
    }

    /// Holds the value between `low` and `high`, inclusive.
    #[must_use]
    pub fn range(mut self, low: f64, high: f64) -> Self {
        self.range = Some((low, high));
        self
    }
}

/// The height of a field: a line of body text in the field's padding.
fn field_height(ui: &Ui) -> f32 {
    ui.fonts_mut(|fonts| fonts.row_height(&Role::Body.font())) + 2.0 * Spacing::FIELD_PADDING.y
}

/// The outline of a field: the selection colour while it is being edited, the stroke colour otherwise.
fn field_stroke(ui: &Ui, editing: bool) -> Stroke {
    if editing {
        ui.visuals().selection.stroke
    } else {
        Stroke::new(1.0, style::STROKE)
    }
}

/// A number field as wide as the room it is given: a field that is dragged to change its value or clicked to type
/// one, drawn in the frame of every other field so that a row of numbers and a row of words read as one kind of
/// thing. A value typed beyond the range is held to it, and a fraction typed into a whole-number field is rounded.
pub fn number(ui: &mut Ui, value: &mut f64, format: Number, id: egui::Id) -> Response {
    let mut frame = egui::Frame::new()
        .fill(style::FIELD)
        .corner_radius(Spacing::RADIUS)
        .inner_margin(Spacing::margin(
            Spacing::FIELD_PADDING.x,
            Spacing::FIELD_PADDING.y,
        ))
        .begin(ui);
    let response = {
        let ui = &mut frame.content_ui;
        ui.set_width(ui.available_width());
        // The drag value is drawn as words in the field rather than as the button egui makes of it: no face, no
        // outline, no padding of its own, and the body font of every field.
        let widgets = &mut ui.visuals_mut().widgets;
        for state in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            state.weak_bg_fill = Color32::TRANSPARENT;
            state.bg_fill = Color32::TRANSPARENT;
            state.bg_stroke = Stroke::NONE;
            state.expansion = 0.0;
        }
        ui.style_mut().drag_value_text_style = egui::TextStyle::Body;
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Body, Role::Body.font());
        ui.spacing_mut().button_padding = Vec2::ZERO;
        let mut drag = egui::DragValue::new(value).speed(format.speed);
        if let Some((low, high)) = format.range {
            drag = drag.range(low..=high);
        }
        if format.integer {
            drag = drag.fixed_decimals(0);
        }
        // The drag value takes an id of its own from the ui, so it is drawn under `id` to keep that stable
        // whatever is drawn around it.
        let response = ui.push_id(id, |ui| ui.add(drag)).inner;
        if format.integer {
            *value = value.round();
        }
        response
    };
    frame.frame.stroke = field_stroke(ui, response.has_focus() || response.dragged());
    frame.end(ui);
    response
}

/// The hex of a colour as a scientist writes it: `#RRGGBB`, and `#RRGGBBAA` when the colour is not opaque, so that
/// an alpha is never hidden.
#[must_use]
pub fn hex(color: Color32) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

/// A colour: a control carrying a swatch of it and its hex, which opens egui's picker beneath it when clicked. The
/// response is marked changed when the picker changes the colour.
pub fn swatch(ui: &mut Ui, color: &mut Color32, id: egui::Id) -> Response {
    let words = hex(*color);
    let mut response = Control::new(text(Role::Data, &words), Face::Raised)
        .before(Icon::Swatch(*color))
        .show(ui);
    let picked = egui::Popup::menu(&response)
        .id(id.with("picker"))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            egui::color_picker::color_picker_color32(
                ui,
                color,
                egui::color_picker::Alpha::OnlyBlend,
            )
        })
        .is_some_and(|inner| inner.inner);
    if picked {
        response.mark_changed();
    }
    response
}

/// A value that is read and not changed: quiet text, cut short to the room it has, standing where a control would.
/// What it is and why it cannot be changed are hung on it with [`hint`].
pub fn readout(ui: &mut Ui, text: Text<'_>) -> Response {
    let galley = text.galley(ui, ui.available_width());
    let height = galley.size().y.max(Spacing::CONTROL_HEIGHT);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::hover());
    let words = text.words.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, words.clone()));
    if ui.is_rect_visible(rect) {
        ui.painter().galley(
            egui::pos2(rect.min.x, rect.center().y - galley.size().y / 2.0),
            galley,
            ui.visuals().weak_text_color(),
        );
    }
    response
}

/// One choice in the menu of a combo box: a row that is marked when it is the one chosen, and greyed with the
/// reason on hover when the figure would not act on it, so that a reader sees the value exists and reads what
/// would make it available.
pub fn choice(ui: &mut Ui, words: &str, selected: bool, unavailable: Option<&str>) -> Response {
    let height = Row::height(ui, false);
    let response = Row::new(text(Role::Body, words), Detail::None)
        .state(RowState {
            selected,
            faded: unavailable.is_some(),
            ..RowState::default()
        })
        .show(ui, height);
    match unavailable {
        Some(reason) => hint(response, reason),
        None => response,
    }
}

/// A problem: the sentence that says why something cannot be done, in the problem colour, wrapped to the room it
/// has because it must be read whole.
pub fn problem(ui: &mut Ui, words: &str) -> Response {
    let mut job = LayoutJob::default();
    job.append(words, 0.0, Role::Control.format());
    job.wrap = egui::text::TextWrapping::wrap_at_width(ui.available_width());
    let galley = ui.painter().layout_job(job);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    let spoken = words.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, spoken.clone()));
    if ui.is_rect_visible(rect) {
        ui.painter().galley(rect.min, galley, style::PROBLEM);
    }
    response
}

/// The width a tooltip wraps at: about sixty characters of control text, which is a sentence or two.
const HINT_WIDTH: f32 = 260.0;

/// Hangs `words` on a widget, to be shown while it is hovered: what a property means, why a value cannot be changed,
/// what a control does. It is the one tooltip of the interface, so every explanation is set the same way.
pub fn hint(response: Response, words: &str) -> Response {
    let words = words.to_owned();
    response.on_hover_ui(move |ui| {
        ui.set_max_width(HINT_WIDTH);
        ui.add(
            egui::Label::new(
                egui::RichText::new(words)
                    .font(Role::Control.font())
                    .color(ui.visuals().text_color()),
            )
            .wrap(),
        );
    })
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
