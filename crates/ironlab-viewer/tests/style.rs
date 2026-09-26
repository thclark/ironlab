//! The text sizes and colours the viewer installs on its egui context.
//!
//! Two things about a style can be measured rather than admired: how large the text is, and how far the text stands
//! out from what is behind it. These tests hold the sizes to the scale the module names, hold every one of them above
//! egui's default, and hold the colours to the WCAG 2.1 contrast ratios for readable text, including the dimmed
//! colour egui actually paints when a widget is disabled.

use egui::{Color32, FontFamily, FontId, TextStyle, Theme};
use ironlab_viewer::style;

/// The contrast ratio WCAG 2.1 asks of ordinary body text.
const READABLE: f64 = 4.5;

/// A character the fonts certainly do not have, which shows what a real gap looks like: it is drawn as the
/// replacement box, and a character is covered exactly when it is drawn as something else.
const MISSING: char = '\u{2B6E}';

/// Whether `family` draws `character` as a glyph of its own rather than as the replacement box.
///
/// It is decided by laying the character out and comparing where in the glyph atlas it was drawn from with where
/// the box is drawn from, because that is what reaches the screen. egui's `has_glyph` answers a different
/// question — whether the face that owns the character differs from the face that owns the box — and in a family
/// whose first face owns both, which the monospaced family is, it says "no" for every character that face has.
fn draws(
    fonts: &mut egui::epaint::text::FontsView<'_>,
    family: &FontFamily,
    character: char,
) -> bool {
    let font = FontId::new(style::BODY_SIZE_PT, family.clone());
    let mut atlas_rect = |c: char| {
        let galley = fonts.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE);
        galley.rows[0].glyphs[0].uv_rect.min
    };
    atlas_rect(character) != atlas_rect(MISSING)
}

/// The contrast ratio WCAG 2.1 asks of large text, which is the least that text meant to be read but not
/// emphasised should have.
const SECONDARY: f64 = 3.0;

/// The WCAG 2.1 relative luminance of an opaque sRGB colour.
fn relative_luminance(color: Color32) -> f64 {
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

/// The WCAG 2.1 contrast ratio between two opaque colours, which runs from 1 (the same colour) to 21 (black on
/// white).
fn contrast_ratio(first: Color32, second: Color32) -> f64 {
    let (first, second) = (relative_luminance(first), relative_luminance(second));
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

/// The colour that reaches the screen when egui paints `color` at `opacity` over `background`.
///
/// `egui::Ui::disable` multiplies the opacity of everything a disabled widget paints, and egui composites its
/// premultiplied colours in gamma space, so the result is the straight weighted mean of the two byte values.
fn painted_at(color: Color32, opacity: f32, background: Color32) -> Color32 {
    let blend = |fore: u8, back: u8| {
        (opacity * f32::from(fore) + (1.0 - opacity) * f32::from(back)).round() as u8
    };
    Color32::from_rgb(
        blend(color.r(), background.r()),
        blend(color.g(), background.g()),
        blend(color.b(), background.b()),
    )
}

// Why: the request was to enlarge the text "across the board", so the guarantee worth testing is not that one style
// grew but that none was left behind: every style egui defines must be configured, and every one of them must clear
// egui's default for it by a point.
#[test]
fn every_text_style_is_at_least_a_point_larger_than_eguis_default() {
    let ours = style::text_styles();
    for (text_style, default) in egui::style::default_text_styles() {
        let configured = ours
            .get(&text_style)
            .unwrap_or_else(|| panic!("{text_style:?} is not configured"));
        assert!(
            configured.size >= default.size + 1.0,
            "{text_style:?} is {} pt, which is not at least a point above egui's {} pt",
            configured.size,
            default.size
        );
    }
}

// Why: the sizes are a scale the rest of the interface is measured against, so they must come from the module's
// named constants rather than from numbers written into the map, and each style must keep the family it is for: a
// monospaced style that is no longer monospaced would silently change what it is used to show.
#[test]
fn the_text_styles_are_the_named_sizes_in_their_own_families() {
    let styles = style::text_styles();
    assert_eq!(
        styles[&TextStyle::Small],
        FontId::new(style::SMALL_SIZE_PT, FontFamily::Proportional)
    );
    assert_eq!(
        styles[&TextStyle::Body],
        FontId::new(style::BODY_SIZE_PT, FontFamily::Proportional)
    );
    assert_eq!(
        styles[&TextStyle::Button],
        FontId::new(style::BUTTON_SIZE_PT, FontFamily::Proportional)
    );
    assert_eq!(
        styles[&TextStyle::Heading],
        FontId::new(style::HEADING_SIZE_PT, FontFamily::Proportional)
    );
    assert_eq!(
        styles[&TextStyle::Monospace],
        FontId::new(style::MONOSPACE_SIZE_PT, FontFamily::Monospace)
    );
}

// Why: this is the complaint the style answers. Every label, value and button caption in the viewer takes the
// ordinary text colour, so it is the one colour that has to clear the ratio asked of body text. egui's grey 140
// clears that ratio on paper and is still hard to read, so the test also asks that the viewer's colour be a real
// improvement on it rather than another shade that merely passes.
#[test]
fn ordinary_text_is_readable_against_the_panel() {
    let ratio = contrast_ratio(style::TEXT, style::BACKGROUND);
    assert!(
        ratio >= READABLE,
        "ordinary text has a contrast ratio of {ratio:.2}:1 against the panel, below the {READABLE}:1 asked of body \
         text"
    );
    let default = contrast_ratio(egui::Visuals::dark().text_color(), style::BACKGROUND);
    assert!(
        ratio > default,
        "ordinary text ({ratio:.2}:1) must be higher in contrast than egui's default ({default:.2}:1)"
    );
}

// Why: weak text carries the group headings, the tooltips and the reason a value cannot be edited. It is meant to
// be quieter than ordinary text, so it must stay quieter and stay distinguishable from it; but egui's default of
// 60 % opacity leaves it at 2.7:1 over the panel, which is where the reading stops being comfortable.
#[test]
fn weak_text_is_quieter_than_ordinary_text_yet_still_legible() {
    let weak = contrast_ratio(style::WEAK_TEXT, style::BACKGROUND);
    let ordinary = contrast_ratio(style::TEXT, style::BACKGROUND);
    assert!(
        weak >= SECONDARY,
        "weak text has a contrast ratio of {weak:.2}:1 against the panel, below the {SECONDARY}:1 asked of \
         secondary text"
    );
    assert!(
        weak < ordinary,
        "weak text ({weak:.2}:1) must stay quieter than ordinary text ({ordinary:.2}:1)"
    );
    assert!(
        contrast_ratio(style::WEAK_TEXT, style::TEXT) >= 1.25,
        "weak text must be visibly weaker than ordinary text, not the same grey by another name"
    );
}

// Why: a disabled control is still there to be read: the panel shows the marker size of a scatter, and the reason
// it cannot be edited, through disabled widgets. egui paints those at a fraction of their opacity, so the colour
// that reaches the screen is dimmer than the colour configured, and it is the painted colour that has to be legible.
#[test]
fn disabled_text_stays_legible_once_egui_has_dimmed_it() {
    for (name, color) in [("ordinary", style::TEXT), ("weak", style::WEAK_TEXT)] {
        let painted = painted_at(color, style::DISABLED_ALPHA, style::BACKGROUND);
        let ratio = contrast_ratio(painted, style::BACKGROUND);
        assert!(
            ratio >= SECONDARY,
            "disabled {name} text reaches the screen at a contrast ratio of {ratio:.2}:1, below the \
             {SECONDARY}:1 asked of secondary text"
        );
    }
}

// Why: the constants describe what the user sees only if the context is actually given them. The button colour is
// checked alongside the label colour because a caption on a button is ordinary text too, and was the one place egui
// coloured it differently.
#[test]
fn applying_the_style_gives_the_context_the_named_sizes_and_colours() {
    let ctx = egui::Context::default();
    style::apply(&ctx);

    let configured = ctx.style_of(Theme::Dark);
    assert_eq!(configured.text_styles, style::text_styles());
    assert_eq!(configured.visuals.panel_fill, style::BACKGROUND);
    assert_eq!(configured.visuals.text_color(), style::TEXT);
    assert_eq!(configured.visuals.weak_text_color(), style::WEAK_TEXT);
    assert_eq!(
        configured.visuals.widgets.inactive.fg_stroke.color,
        style::TEXT,
        "the caption of a button is ordinary text"
    );
    assert_eq!(configured.visuals.disabled_alpha, style::DISABLED_ALPHA);
    assert_eq!(
        configured.visuals.widgets.inactive.weak_bg_fill,
        style::WIDGET,
        "the face of a button is the named shade above the panel"
    );
    assert_eq!(
        configured.visuals.faint_bg_color,
        style::FAINT,
        "and the stripe of a list is the named one"
    );
    assert_eq!(
        configured.spacing.scroll.fade.strength, 0.0,
        "a scroll area fades none of its rows: the lists end at a rule, and a fade would darken the last row"
    );
}

// Why: the text sizes are a matter of legibility rather than of colour, and the viewer must not depend on the theme
// it happens to start in to get them, so both themes carry the same scale.
#[test]
fn applying_the_style_enlarges_the_text_of_both_themes() {
    let ctx = egui::Context::default();
    style::apply(&ctx);

    assert_eq!(ctx.style_of(Theme::Light).text_styles, style::text_styles());
    assert_eq!(ctx.style_of(Theme::Dark).text_styles, style::text_styles());
}

// Why: egui loads four fonts, and between them they cover far less than Unicode. A character they do not have is
// drawn as an empty box, which says nothing and reads as a fault in the program — which is exactly what the padlock
// once drawn on a read-only row did. The guard is to keep every character the interface draws in one list and to
// ask the fonts, through egui's own coverage check, whether they have each of them. A character added to the
// interface and not to the list is caught by the tests of what the panel paints; a character added to the list that
// the fonts cannot draw is caught here.
#[test]
fn the_fonts_have_every_character_the_interface_draws() {
    let ctx = egui::Context::default();
    style::apply(&ctx);
    // The fonts are not built until a pass has run, because the size of a point is not known until then.
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
    output.textures_delta.clear();

    // The interface draws proportional text everywhere and monospaced text where a figure's details are shown as
    // data, so both families are checked: a character is only safe when every family it might be set in has it.
    let mut families: Vec<FontFamily> = style::text_styles()
        .values()
        .map(|font| font.family.clone())
        .collect();
    families.sort_by_key(|family| format!("{family:?}"));
    families.dedup();
    assert!(
        !families.is_empty(),
        "the interface draws proportional text, so there is a family to check"
    );

    ctx.fonts_mut(|fonts| {
        for family in &families {
            // A character the fonts have and one they lack must come out differently, or the check proves nothing.
            assert!(
                draws(fonts, family, 'a') && !draws(fonts, family, MISSING),
                "the check must tell a character the fonts have from one they lack"
            );
            for character in style::INTERFACE_CHARACTERS {
                assert!(
                    draws(fonts, family, *character),
                    "the interface draws {character:?} (U+{:04X}), which the fonts of the {family:?} family \
                     cannot draw and would show as an empty box",
                    *character as u32
                );
            }
        }
    });
}

// Why: the list is what the code draws from and what the fonts are checked against, so a character used in the
// interface but left out of it would never be checked. Every mark the interface draws is named in `MARKS`, so the
// two lists can be held together by construction rather than by someone remembering to add to both.
#[test]
fn every_mark_the_interface_draws_is_one_of_the_listed_characters() {
    assert!(
        style::MARKS.contains(&style::RESTORE),
        "the revert control is a mark, and must be named among them"
    );
    for mark in style::MARKS {
        let characters: Vec<char> = mark.chars().collect();
        assert_eq!(
            characters.len(),
            1,
            "the mark {mark:?} is more than one character, so it is a word and does not belong here"
        );
        assert!(
            style::INTERFACE_CHARACTERS.contains(&characters[0]),
            "the mark {mark:?} is not in the list the fonts are checked against"
        );
    }
}

// Why: a problem is the one line of the interface that must be read before anything else on the panel, so it is
// drawn in a colour of its own; but a colour chosen for warmth rather than for contrast would be the least legible
// text on the panel exactly where legibility matters most. egui's own error colour is pure red, which clears the
// ratio and glares, so the viewer's colour is asked to clear the ratio asked of body text and to be plainly not the
// ordinary text colour.
#[test]
fn problem_text_is_readable_against_the_panel_and_distinct_from_ordinary_text() {
    let ratio = contrast_ratio(style::PROBLEM, style::BACKGROUND);
    assert!(
        ratio >= READABLE,
        "problem text has a contrast ratio of {ratio:.2}:1 against the panel, below the {READABLE}:1 asked of body \
         text"
    );
    assert_ne!(
        style::PROBLEM,
        style::TEXT,
        "a problem is not drawn in the colour of ordinary text"
    );
}
