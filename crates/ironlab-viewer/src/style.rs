//! The text sizes, colours and characters of the viewer's interface, defined in one place.
//!
//! egui's dark defaults are sized and coloured for a small tool window: 13 pt body text drawn in grey 140, and
//! secondary text at 60 % of that, which over the grey 27 of a panel comes out at grey 95 and a contrast ratio of
//! 2.7:1. Read for as long as a figure is worked on, that is both too small and too dim, and the dimness is worst
//! exactly where the property editor puts its explanations: group headings, tooltips, and the reason a value is
//! shown read-only.
//!
//! It also holds every colour and size the interface is drawn in beyond egui's own: the face of a button, the
//! stripe of a list, the fills of the browser's menu and foot, the surround of the canvas, and the sizes of the
//! small text that sits beside ordinary text. They are the values of the design that was approved for the figure
//! browser, named here so that the panel, the toolbar and the strip of details are drawn from one set.
//!
//! This module raises every text style by 1.5 pt and lifts the text colours until ordinary text clears the WCAG 2.1
//! ratio of 4.5:1 against the panel and secondary text clears 3:1, including after egui has dimmed it for a disabled
//! widget. It does so in one place, as named constants, so that the interface moves as a whole rather than one
//! widget at a time, and so that the styling overhaul this is the first step of has a single definition to build on.
//!
//! The colours belong to the dark theme, because they are chosen against the grey 27 of a dark panel. The viewer
//! still follows the theme the host system asks for, and a light interface keeps egui's own colours, which are dark
//! text on a light panel and were never the complaint.
//!
//! It also holds [`INTERFACE_CHARACTERS`], every character outside ASCII that the interface draws. A character the
//! loaded fonts do not have is drawn as an empty box, so the interface draws only characters that are known to be
//! covered, and the list is what the code draws from and what the test checks against the fonts. egui's own fonts
//! carry every character on the list, so the viewer installs no font of its own.
//!
//! Nothing here reaches the figure. The text of a figure — its titles, axis labels, tick labels and legends — is
//! typeset by `ironlab-text` and drawn from the scene display list, in the sizes and colours the figure itself
//! carries. This style covers only the interface around it.

use egui::{Color32, Context, FontFamily, FontId, TextStyle, Theme};
use std::collections::BTreeMap;

/// The background the interface is drawn on, and the colour every contrast ratio here is measured against.
///
/// It is egui's own dark panel fill, kept as it is because the fault was the text rather than what is behind it.
pub const BACKGROUND: Color32 = Color32::from_gray(27);

/// Ordinary text: labels, values and the captions of buttons.
///
/// Grey 190 on [`BACKGROUND`] is a contrast ratio of 9.3:1, where egui's grey 140 gives 5.1:1.
pub const TEXT: Color32 = Color32::from_gray(190);

/// Secondary text: group headings, tooltips, and the reason a value cannot be edited.
///
/// Grey 150 on [`BACKGROUND`] is a contrast ratio of 5.8:1, comfortably readable while still plainly quieter than
/// [`TEXT`]. egui's default, which fades ordinary text to 60 % opacity, gives 2.7:1.
pub const WEAK_TEXT: Color32 = Color32::from_gray(150);

/// The face of a button, a combo box and an unticked checkbox, and the fill of a row of a list under the pointer.
///
/// Grey 45, a shade above [`BACKGROUND`], so that a control reads as raised from the panel without competing with
/// the text on it; egui's grey 60 is kept for the outline of a text field and the lines between panels, which
/// [`STROKE`] names.
pub const WIDGET: Color32 = Color32::from_gray(45);

/// The outline of a text field, the line between two panels, and the rule above and below a group heading.
pub const STROKE: Color32 = Color32::from_gray(60);

/// The fill of every second row of a list, so that the eye can follow one row across.
///
/// Grey 36 is faint enough to read as a stripe and not as a selection; egui's own faint fill, five above the
/// panel, is too faint to follow.
pub const FAINT: Color32 = Color32::from_gray(36);

/// The fill of a text field: egui's own, named here so that a field drawn by hand matches one egui draws.
pub const FIELD: Color32 = Color32::from_gray(10);

/// The fill of the filter menu, which opens within the browser: a shade below the panel, so that the menu reads
/// as a well the parameters sit in rather than as a card laid over the panel.
pub const MENU_FILL: Color32 = Color32::from_gray(20);

/// The fill of the strip at the foot of the browser that counts what is left of the collection.
pub const FOOT_FILL: Color32 = Color32::from_gray(25);

/// The fill of the heading of a group in the browser's list.
pub const GROUP_FILL: Color32 = Color32::from_rgb(32, 32, 34);

/// The surround the canvas draws a figure on: a neutral grey, lighter than the panel, so that the white page of a
/// figure and the dark panels beside it both read as objects against it.
pub const SURROUND: Color32 = Color32::from_rgb(43, 43, 46);

/// The fill of a chip that narrows a parameter: a wash of blue, the same construction as [`LABEL_CHIP_FILL`] in
/// another hue, so that the two kinds of chip read as one kind of thing.
pub const CHIP_FILL: Color32 = Color32::from_rgb(29, 43, 56);

/// The fill of a chip that narrows a parameter while the pointer is over it.
pub const CHIP_HOVER: Color32 = Color32::from_rgb(36, 56, 74);

/// The outline of a chip that narrows a parameter: solid, a shade lighter than its fill.
pub const CHIP_STROKE: Color32 = Color32::from_rgb(45, 90, 120);

/// The name of the parameter on a chip.
pub const CHIP_KEY: Color32 = Color32::from_rgb(143, 196, 230);

/// The value on a chip.
pub const CHIP_TEXT: Color32 = Color32::from_rgb(191, 227, 247);

/// The fill of a chip that narrows the labels, and of a tag beneath the canvas: a wash of green.
pub const LABEL_CHIP_FILL: Color32 = Color32::from_rgb(43, 58, 47);

/// The fill of a chip that narrows the labels while the pointer is over it.
pub const LABEL_CHIP_HOVER: Color32 = Color32::from_rgb(55, 74, 60);

/// The outline of a chip that narrows the labels, and of a tag.
pub const LABEL_CHIP_STROKE: Color32 = Color32::from_rgb(63, 90, 70);

/// The word "labels" on a chip that narrows the labels.
pub const LABEL_CHIP_KEY: Color32 = Color32::from_rgb(143, 191, 158);

/// The labels on a chip that narrows the labels, and the word of a tag.
pub const LABEL_CHIP_TEXT: Color32 = Color32::from_rgb(188, 217, 196);

/// A bar of the histogram above a numeric range, for the figures the range leaves out.
pub const HISTOGRAM_BAR: Color32 = Color32::from_gray(63);

/// A bar of the histogram above a numeric range, for the figures the range keeps.
pub const HISTOGRAM_KEPT: Color32 = Color32::from_rgb(47, 110, 140);

/// The second line of a selected row, and the count on it: pale enough to read on the selection fill.
pub const SELECTED_DETAIL: Color32 = Color32::from_rgb(201, 230, 245);

/// The brightest text, which the tick of a ticked checkbox is drawn in.
pub const BRIGHT: Color32 = Color32::from_gray(236);

/// The opacity at which egui paints a disabled widget.
///
/// egui's 0.5 would leave [`WEAK_TEXT`] in a disabled row at 2.4:1 over the panel; 0.7 leaves it at 3.5:1, and a
/// disabled control still reads as unmistakably disabled.
pub const DISABLED_ALPHA: f32 = 0.7;

/// The mark on the column of the property editor that takes back a change to one property.
///
/// The control is a column of its own, narrower than any word, so that one place in the interface is named by
/// the mark alone. Everywhere else a mark is painted by [`crate::widgets::Icon`] and comes from no font; this one
/// remains a character, listed in [`INTERFACE_CHARACTERS`] and checked against the fonts, until the property
/// editor is drawn from the widgets too.
pub const RESTORE: &str = "↺";

/// Every mark the interface draws beside or in place of words, which [`INTERFACE_CHARACTERS`] must hold.
///
/// It exists so that the check is by construction: a mark named here and left out of the list fails a test rather
/// than waiting to be noticed as an empty box on screen.
pub const MARKS: &[&str] = &[RESTORE];

/// Every character outside ASCII that the viewer's interface draws.
///
/// egui loads four fonts, and between them they cover far less than Unicode: a character they do not have is drawn
/// as an empty box, which tells the reader nothing and looks like a fault in the program. The interface therefore
/// draws only characters that are known to be covered, and this is the list of them, shared by the code that draws
/// them and by the test that checks the fonts have them.
///
/// The list is short on purpose, and a mark earns its place in one of two ways.
///
/// It may say something a word cannot say in the room available: the restore control of the property editor, which
/// is a column narrower than any word, the arithmetic of an array's shape, and the punctuation of ordinary prose.
/// Every other mark the interface draws is painted as a shape by [`crate::widgets::Icon`] and is no character at
/// all.
///
/// What a mark may not do is carry alone a meaning that has no words at all. That is what the padlock on a
/// read-only row and the warning sign on the problems indicator once did, and both are written as words now, as
/// are the Command and Shift keys of a shortcut.
pub const INTERFACE_CHARACTERS: &[char] = &[
    '—', // em dash, which separates the halves of a heading and the clauses of a sentence
    '…', // ellipsis, which ends the caption of a button that opens a dialogue
    '×', // the multiplication sign that joins the lengths of an array's shape
    '↺', // the mark of a control that puts things back, `RESTORE`
    '·', // the middle dot that separates two facts about a parameter in the filter menu, such as "8 values · 100%"
];

/// The size of small text, such as where a problem came from, in points: egui's 9 pt raised by 1.5 pt.
pub const SMALL_SIZE_PT: f32 = 10.5;

/// The size of body text, in points: egui's 13 pt raised by 1.5 pt.
pub const BODY_SIZE_PT: f32 = 14.5;

/// The size of the captions of buttons, in points: egui's 13 pt raised by 1.5 pt.
pub const BUTTON_SIZE_PT: f32 = 14.5;

/// The size of monospaced text, in points: egui's 13 pt raised by 1.5 pt.
pub const MONOSPACE_SIZE_PT: f32 = 14.5;

/// The size of headings, in points: egui's 18 pt raised by 1.5 pt.
pub const HEADING_SIZE_PT: f32 = 19.5;

/// The text styles the viewer installs: the five styles egui defines, each in the font family egui uses for it and
/// at the size named above.
#[must_use]
pub fn text_styles() -> BTreeMap<TextStyle, FontId> {
    [
        (
            TextStyle::Small,
            FontId::new(SMALL_SIZE_PT, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(BODY_SIZE_PT, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(BUTTON_SIZE_PT, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(HEADING_SIZE_PT, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(MONOSPACE_SIZE_PT, FontFamily::Monospace),
        ),
    ]
    .into()
}

/// Gives `ui` the spacing of the widgets: controls in their padding, set a gap apart on a row, and nothing between
/// one block and the next, because each block carries its own padding.
///
/// It is applied to the toolbar and to the figure browser, which are drawn from the same set of measurements and
/// must agree with each other; the property editor keeps egui's own spacing.
pub fn compact(ui: &mut egui::Ui) {
    use crate::widgets::{Role, Spacing};
    let style = ui.style_mut();
    style.spacing.button_padding = Spacing::CONTROL_PADDING;
    style.spacing.item_spacing = egui::vec2(Spacing::GAP, 0.0);
    style.spacing.interact_size.y = Spacing::CONTROL_HEIGHT;
    style
        .text_styles
        .insert(TextStyle::Button, Role::Control.font());
}

/// Installs the viewer's text sizes and colours on `ctx`.
///
/// The sizes are given to both themes, because how large text should be is a question of legibility rather than of
/// colour, and the viewer should not depend on which theme it starts in to be readable. The colours are given to the
/// dark theme alone, because they are chosen against [`BACKGROUND`]; the theme the viewer runs in is still the one
/// the host system asks for.
///
/// Weak text is given a colour of its own rather than egui's fade of the ordinary text colour, so that what the
/// constants promise is what is painted: a fade leaves the colour on screen depending on whatever happens to be
/// behind it.
///
pub fn apply(ctx: &Context) {
    ctx.all_styles_mut(|style| style.text_styles = text_styles());
    ctx.style_mut_of(Theme::Dark, |style| {
        let visuals = &mut style.visuals;
        visuals.panel_fill = BACKGROUND;
        visuals.window_fill = BACKGROUND;
        // The colour of ordinary text, and of the caption of a button, which egui draws from the inactive widget
        // visuals and therefore in a colour of its own.
        visuals.widgets.noninteractive.fg_stroke.color = TEXT;
        visuals.widgets.inactive.fg_stroke.color = TEXT;
        visuals.weak_text_color = Some(WEAK_TEXT);
        visuals.disabled_alpha = DISABLED_ALPHA;
        // The face of a button, and the fill of an unticked checkbox: a shade above the panel rather than egui's
        // grey 60, which is kept for outlines.
        visuals.widgets.inactive.weak_bg_fill = WIDGET;
        visuals.widgets.inactive.bg_fill = WIDGET;
        visuals.faint_bg_color = FAINT;
        visuals.extreme_bg_color = FIELD;
    });
}
