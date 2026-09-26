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
//! covered, and the list is what the code draws from and what the test checks against the fonts. Because egui's own
//! fonts carry no arrow of any kind, [`font_definitions`] adds the mathematical face that `ironlab-text` already
//! compiles into the binary as the last resort of every family.
//!
//! Nothing here reaches the figure. The text of a figure — its titles, axis labels, tick labels and legends — is
//! typeset by `ironlab-text` and drawn from the scene display list, in the sizes and colours the figure itself
//! carries. This style covers only the interface around it.

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId, TextStyle, Theme};

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

/// The fill of a tag: the frame a label of a figure is drawn in, below the canvas.
///
/// A dark green, so that a row of tags reads as a set of words and not as a row of buttons, which the greys of the
/// interface are spent on. The tint is faint enough that a tag is quieter than the text beside it.
pub const TAG_FILL: Color32 = Color32::from_rgb(35, 48, 31);

/// The outline of a tag, a shade lighter than [`TAG_FILL`], which is what separates two tags side by side.
pub const TAG_STROKE: Color32 = Color32::from_rgb(51, 69, 44);

/// The text of a tag.
///
/// A pale green on [`TAG_FILL`] is a contrast ratio of 6.6:1, above the 4.5:1 asked of body text, so a label reads
/// as clearly as any other word in the interface.
pub const TAG_TEXT: Color32 = Color32::from_rgb(155, 191, 166);

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

/// The mark on a control that takes something away, such as the chip of a filter in the figure browser.
///
/// It augments the words of the control rather than standing in for them: the chip says which parameter it narrows
/// and to what, and the mark says that clicking it takes that away.
pub const REMOVE: &str = "×";

/// The mark on the control that puts a list in ascending order, which is captioned "Ascending".
pub const ASCENDING: &str = "↑";

/// The mark on the control that puts a list in descending order, which is captioned "Descending".
pub const DESCENDING: &str = "↓";

/// Every mark the interface draws beside or in place of words, which [`INTERFACE_CHARACTERS`] must hold.
///
/// It exists so that the check is by construction: a mark named here and left out of the list fails a test rather
/// than waiting to be noticed as an empty box on screen.
pub const MARKS: &[&str] = &[REVERT, REMOVE, ASCENDING, DESCENDING];

/// Every character outside ASCII that the viewer's interface draws.
///
/// egui loads four fonts, and between them they cover far less than Unicode: a character they do not have is drawn
/// as an empty box, which tells the reader nothing and looks like a fault in the program. The interface therefore
/// draws only characters that are known to be covered, and this is the list of them, shared by the code that draws
/// them and by the test that checks the fonts have them.
///
/// The list is short on purpose, and a mark earns its place in one of two ways.
///
/// It may say something a word cannot say in the room available: the revert control, which is a column narrower
/// than any word, the arithmetic of an array's shape, and the punctuation of ordinary prose. Or it may carry at a
/// glance what the words beside it already spell out: the cross on the chip that removes a filter, and the arrows
/// on the control that reverses an order. A mark of the second kind augments its words and never replaces them, so
/// the control reads correctly to someone who does not take the mark in.
///
/// What a mark may not do is carry alone a meaning that has no words at all. That is what the padlock on a
/// read-only row and the warning sign on the problems indicator once did, and both are written as words now, as
/// are the Command and Shift keys of a shortcut.
pub const INTERFACE_CHARACTERS: &[char] = &[
    '—', // em dash, which separates the halves of a heading and the clauses of a sentence
    '…', // ellipsis, which ends the caption of a button that opens a dialogue
    '×', // the mark of a control that takes something away, `REMOVE`, and the multiplication sign that joins the
    // lengths of an array's shape
    '↑', // the mark of ascending order, `ASCENDING`
    '↓', // the mark of descending order, `DESCENDING`
    '↺', // the revert control, `REVERT`
];

/// The name egui knows the interface's last-resort face by.
///
/// It is prefixed, because the name shares a namespace with egui's own four fonts.
pub const FALLBACK_FONT: &str = "ironlab_fallback";

/// The fonts the viewer installs: egui's own, with a last-resort face added to the end of every family.
///
/// egui's four fonts carry no arrow at all — not `↑`, not `▲`, not an arrowhead — so a control that shows the
/// direction of an order has nothing to draw it with, and neither has STIX Two Text, whose 1281 characters are the
/// text of a figure rather than its symbols. Rather than bundle a fifth font for two glyphs, the interface falls
/// back on STIX Two Math, which `ironlab-text` already compiles into the binary to typeset the mathematics of a
/// figure, and which carries every character of [`INTERFACE_CHARACTERS`].
///
/// The face is added last in each family, so it is reached only for a character the other fonts lack: nothing egui
/// draws today changes shape. It does mean a character left out of [`INTERFACE_CHARACTERS`] may now be drawn rather
/// than showing as an empty box, so the tests of what the interface paints, rather than the box, are what keep the
/// list honest.
///
/// Whether a family has a character is checked by laying the character out, not by asking egui's `has_glyph`.
/// That method compares the face that owns the character with the face that owns the replacement box, and in the
/// monospaced family those are one face, Hack, for every character Hack has — so it answers "no" for characters
/// that draw perfectly well.
#[must_use]
pub fn font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        FALLBACK_FONT.to_owned(),
        Arc::new(FontData::from_static(ironlab_text::font_bytes(
            ironlab_text::FontId::Math,
        ))),
    );
    for family in fonts.families.values_mut() {
        family.push(FALLBACK_FONT.to_owned());
    }
    fonts
}

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
///
/// The fonts of [`font_definitions`] are installed here too, so that everything the interface needs in order to
/// draw as this module describes arrives in one call.
pub fn apply(ctx: &Context) {
    ctx.set_fonts(font_definitions());
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
