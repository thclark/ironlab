//! The text sizes, colours and characters of the viewer's interface, defined in one place.
//!
//! egui's dark defaults are sized and coloured for a small tool window: 13 pt body text drawn in grey 140, and
//! secondary text at 60 % of that, which over the grey 27 of a panel comes out at grey 95 and a contrast ratio of
//! 2.7:1. Read for as long as a figure is worked on, that is both too small and too dim, and the dimness is worst
//! exactly where the property editor puts its explanations: group headings, tooltips, and the reason a value is
//! shown read-only.
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
//! covered, and the list is what the code draws from and what the test checks against the fonts.
//!
//! Nothing here reaches the figure. The text of a figure — its titles, axis labels, tick labels and legends — is
//! typeset by `ironlab-text` and drawn from the scene display list, in the sizes and colours the figure itself
//! carries. This style covers only the interface around it.

use std::collections::BTreeMap;

use egui::{Color32, Context, FontFamily, FontId, TextStyle, Theme};

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

/// The opacity at which egui paints a disabled widget.
///
/// egui's 0.5 would leave [`WEAK_TEXT`] in a disabled row at 2.4:1 over the panel; 0.7 leaves it at 3.5:1, and a
/// disabled control still reads as unmistakably disabled.
pub const DISABLED_ALPHA: f32 = 0.7;

/// The character that marks the control which takes back a change to one property.
///
/// The control is a column of its own, narrower than any word, so this one place in the interface is named by a
/// character rather than by what it does. The character is listed in [`INTERFACE_CHARACTERS`], which is checked
/// against the fonts, so it cannot become a character the viewer draws as an empty box.
pub const REVERT: &str = "↺";

/// Every character outside ASCII that the viewer's interface draws.
///
/// egui loads four fonts, and between them they cover far less than Unicode: a character they do not have is drawn
/// as an empty box, which tells the reader nothing and looks like a fault in the program. The interface therefore
/// draws only characters that are known to be covered, and this is the list of them, shared by the code that draws
/// them and by the test that checks the fonts have them.
///
/// The list is short on purpose. A symbol is used only where it says something a word cannot say in the room
/// available: the revert control, the arithmetic of an array's shape, and the punctuation of ordinary prose. Every
/// other mark the interface once carried — a padlock on a read-only row, a warning sign on the problems indicator,
/// a cross on a remove button, the Command and Shift keys in a shortcut — is written as a word instead, which reads
/// the same in every font.
pub const INTERFACE_CHARACTERS: &[char] = &[
    '—', // em dash, which separates the halves of a heading and the clauses of a sentence
    '…', // ellipsis, which ends the caption of a button that opens a dialogue
    '×', // multiplication sign, which joins the lengths of an array's shape
    '↺', // the revert control, `REVERT`
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
    });
}
