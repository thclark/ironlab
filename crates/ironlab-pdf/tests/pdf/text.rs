//! Glyph runs: real (extractable) text, positioning and font embedding.

use ironlab_scene::display::{GlyphsItem, ItemKind, PlacedGlyph, Point, Rect, Transform};
use ironlab_text::{FontId, TextItem};

use crate::common::*;
use crate::require_tools;

/// Returns the glyph identifier and font of the single glyph that `content` lays out to.
fn single_glyph(text: &ironlab_text::TextEngine, content: &str, parse_math: bool) -> (FontId, u16) {
    let layout = text.layout(content, parse_math, 12.0);
    let glyphs: Vec<_> = layout
        .items
        .iter()
        .filter_map(|item| match item {
            TextItem::Glyphs(run) => Some(run.glyphs.iter().map(move |g| (run.font, g.id))),
            TextItem::Rule { .. } => None,
        })
        .flatten()
        .collect();
    assert_eq!(
        glyphs.len(),
        1,
        "{content:?} did not lay out to one glyph: {layout:?}"
    );
    glyphs[0]
}

#[test]
fn plain_glyph_runs_are_extractable_text() {
    // WHY: axis labels must be real text so that readers can search and copy them and screen readers can read them;
    // text drawn as outlines would look identical but fail this. The second label contains multi-byte characters, which
    // shift every later glyph's byte range (treating ranges as character indices garbles the text after them), and the
    // "ffi" ligature, one glyph that must still copy as three letters.
    require_tools!("pdftotext");
    let ws = Workspace::new("plain-text");
    let text = engine();
    let mut list = page(300.0, 80.0);
    list.items = label(&text, "Time (s)", false, 12.0, Point::new(20.0, 30.0));
    list.items.extend(label(
        &text,
        "Température (°C) coefficient",
        false,
        12.0,
        Point::new(20.0, 60.0),
    ));

    let extracted = pdftotext(&ws.write_pdf("figure", &render(&list, &text)));
    for expected in ["Time (s)", "Température (°C) coefficient"] {
        assert!(
            extracted.contains(expected),
            "{expected:?} missing from pdftotext output {extracted:?}"
        );
    }
}

#[test]
fn math_glyph_runs_are_extractable_text() {
    // WHY: math glyphs come from a different font with glyph identifiers that have no standard Unicode mapping, so the
    // run's text must be carried into the PDF for "αβ" to be copyable rather than extracted as garbage. The text engine
    // sets math letters in the Mathematical Italic block (U+1D6FC "𝛼" for \alpha), so the extracted text must equal
    // the characters that the layout's runs claim, whichever block they are in. Those characters take several bytes
    // each, which makes a byte-range versus character-index confusion visible.
    require_tools!("pdftotext");
    let ws = Workspace::new("math-text");
    let text = engine();
    let layout = text.layout("$\\alpha\\beta$", true, 12.0);
    let expected: String = layout
        .items
        .iter()
        .filter_map(|item| match item {
            TextItem::Glyphs(run) => Some(run.text.as_str()),
            TextItem::Rule { .. } => None,
        })
        .collect();
    let letters: Vec<char> = expected
        .chars()
        .map(|ch| match ch {
            '\u{1D6FC}' => 'α',
            '\u{1D6FD}' => 'β',
            other => other,
        })
        .collect();
    assert_eq!(
        letters,
        ['α', 'β'],
        "the math layout's run text {expected:?} does not represent alpha and beta"
    );
    let mut list = page(100.0, 60.0);
    list.items = text_items(
        &layout,
        Point::new(20.0, 30.0),
        ironlab_scene::display::Rgba::BLACK,
    );

    let extracted = pdftotext(&ws.write_pdf("figure", &render(&list, &text)));
    // Whitespace is ignored because poppler may infer a space from the italic spacing of math glyphs.
    let extracted_letters: String = extracted.split_whitespace().collect();
    assert!(
        extracted_letters == expected,
        "pdftotext extracted {extracted:?}, expected exactly {expected:?}"
    );
}

#[test]
fn text_occupies_the_laid_out_extent() {
    // WHY: the PDF pen advances by font advances while the layout gives absolute positions; a wrong conversion
    // between the two shifts glyphs so that exported labels overlap ticks that the scene compiler kept clear.
    require_tools!("pdftotext");
    let ws = Workspace::new("text-extent");
    let text = engine();
    let origin = Point::new(20.0, 30.0);
    let layout = text.layout("Time (s)", false, 12.0);
    let mut list = page(200.0, 60.0);
    list.items = text_items(&layout, origin, ironlab_scene::display::Rgba::BLACK);

    let words = pdftotext_words(&ws.write_pdf("figure", &render(&list, &text)));
    let first = words
        .iter()
        .find(|w| w.text == "Time")
        .unwrap_or_else(|| panic!("no word \"Time\" in {words:?}"));
    let last = words
        .iter()
        .find(|w| w.text == "(s)")
        .unwrap_or_else(|| panic!("no word \"(s)\" in {words:?}"));
    assert!(
        (first.x_min - origin.x).abs() < 0.5,
        "\"Time\" starts at {}, expected {}",
        first.x_min,
        origin.x
    );
    let end = origin.x + layout.width;
    assert!(
        (last.x_max - end).abs() < 1.0,
        "\"(s)\" ends at {}, expected {end}",
        last.x_max
    );
    for word in [first, last] {
        assert!(
            word.y_min < origin.y && origin.y < word.y_max,
            "baseline {} lies outside the vertical extent of {word:?}",
            origin.y
        );
    }
}

#[test]
fn fonts_are_embedded_subset_and_mapped_to_unicode() {
    // WHY: a non-embedded font is substituted by the viewer (and rejected by journals), a non-subset font bloats every
    // figure by megabytes, and a missing ToUnicode map makes text uncopyable.
    require_tools!("pdffonts");
    let ws = Workspace::new("fonts");
    let text = engine();
    let mut list = page(200.0, 80.0);
    list.items = label(&text, "Time (s)", false, 12.0, Point::new(20.0, 30.0));
    list.items.extend(label(
        &text,
        "$\\alpha^{2}$",
        true,
        12.0,
        Point::new(20.0, 60.0),
    ));

    let fonts = pdffonts(&ws.write_pdf("figure", &render(&list, &text)));
    assert!(
        fonts.len() >= 2,
        "expected the text and math fonts, got {fonts:?}"
    );
    for font in &fonts {
        assert!(font.embedded, "font not embedded: {font:?}");
        assert!(font.subset, "font not subset: {font:?}");
        assert!(font.unicode, "font has no ToUnicode map: {font:?}");
    }
    for expected in ["STIXTwoText-Regular", "STIXTwoMath"] {
        assert!(
            fonts.iter().any(|f| f.name.contains(expected)),
            "no {expected} font in {fonts:?}"
        );
    }
}

#[test]
fn each_font_id_embeds_its_own_face() {
    // WHY: the display list names faces by `FontId`, so a mix-up between identifiers and font bytes would silently set
    // italic or bold text in the wrong face.
    require_tools!("pdffonts");
    let ws = Workspace::new("font-faces");
    let text = engine();
    let (_, text_glyph) = single_glyph(&text, "a", false);
    let (math_font, math_glyph) = single_glyph(&text, "$a$", true);
    assert_eq!(math_font, FontId::Math);

    let mut list = page(200.0, 120.0);
    for (i, (font, id)) in [
        (FontId::TextRegular, text_glyph),
        (FontId::TextItalic, text_glyph),
        (FontId::TextBold, text_glyph),
        (FontId::Math, math_glyph),
    ]
    .into_iter()
    .enumerate()
    {
        list.items.push(item(ItemKind::Glyphs(GlyphsItem {
            font,
            size_pt: 12.0,
            color: ironlab_scene::display::Rgba::BLACK,
            text: "a".to_owned(),
            glyphs: vec![PlacedGlyph {
                id,
                x: 20.0,
                y: 25.0 + 25.0 * i as f64,
                text_range: 0..1,
            }],
        })));
    }

    let fonts = pdffonts(&ws.write_pdf("figure", &render(&list, &text)));
    for expected in [
        "STIXTwoText-Regular",
        "STIXTwoText-Italic",
        "STIXTwoText-Bold",
        "STIXTwoMath",
    ] {
        assert!(
            fonts.iter().any(|f| f.name.contains(expected)),
            "no {expected} font in {fonts:?}"
        );
    }
}

#[test]
fn glyphs_are_drawn_at_their_placed_positions_in_their_colour() {
    // WHY: math layout places glyphs of one run at arbitrary x and y (superscripts, kerning), so a backend that lets
    // the font's advances or a shared baseline decide positions draws different math from the one on screen.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("glyph-positions");
    let text = engine();
    let (font, id) = single_glyph(&text, "H", false);
    let size = 36.0;
    let mut list = page(200.0, 120.0);
    list.items.push(item(ItemKind::Glyphs(GlyphsItem {
        font,
        size_pt: size,
        color: RED,
        text: "HH".to_owned(),
        glyphs: vec![
            PlacedGlyph {
                id,
                x: 20.0,
                y: 50.0,
                text_range: 0..1,
            },
            PlacedGlyph {
                id,
                x: 120.0,
                y: 100.0,
                text_range: 1..2,
            },
        ],
    })));

    let is_red = |px: [u8; 3]| px[0] > 200 && px[1] < 90 && px[2] < 90;
    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        let first = Rect::new(20.0, 50.0 - size, size, size);
        let second = Rect::new(120.0, 100.0 - size, size, size);
        assert!(
            count_pixels(&image, first, is_red) > 20,
            "{engine:?}: no red ink for the glyph placed at (20, 50)"
        );
        assert!(
            count_pixels(&image, second, is_red) > 20,
            "{engine:?}: no red ink for the glyph placed at (120, 100)"
        );
        assert_eq!(
            count_pixels(&image, Rect::new(60.0, 0.0, 55.0, 120.0), is_ink),
            0,
            "{engine:?}: ink between the glyphs, so the second glyph followed the font advance instead of its x"
        );
        assert_eq!(
            count_pixels(&image, Rect::new(118.0, 0.0, 82.0, 60.0), is_ink),
            0,
            "{engine:?}: ink above the second glyph, so it was drawn on the first glyph's baseline instead of its y"
        );
    }
}

#[test]
fn rotated_text_is_drawn_vertically_and_remains_extractable() {
    // WHY: every 2D axes has a y label rotated by −90°, drawn as glyphs inside a transformed group; it must appear
    // vertical in the PDF and still be real text.
    require_tools!("pdftotext", RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("rotated-text");
    let text = engine();
    let mut list = page(100.0, 200.0);
    list.items.push(group(
        None,
        Some(Transform::rotate(-90.0).then(Transform::translate(40.0, 170.0))),
        label(&text, "Amplitude", false, 14.0, Point::new(0.0, 0.0)),
    ));

    let bytes = render(&list, &text);
    let pdf = ws.write_pdf("figure", &bytes);
    let extracted = pdftotext(&pdf);
    assert!(
        extracted.contains("Amplitude"),
        "pdftotext extracted {extracted:?}"
    );
    for engine in ENGINES {
        let image = rasterise(&pdf, engine);
        let columns: Vec<u32> = (0..image.width())
            .filter(|&x| (0..image.height()).any(|y| is_ink(image.get_pixel(x, y).0)))
            .collect();
        let rows: Vec<u32> = (0..image.height())
            .filter(|&y| (0..image.width()).any(|x| is_ink(image.get_pixel(x, y).0)))
            .collect();
        assert!(
            !columns.is_empty(),
            "{engine:?}: the rotated label left no ink"
        );
        let ink_width = columns[columns.len() - 1] - columns[0] + 1;
        let ink_height = rows[rows.len() - 1] - rows[0] + 1;
        assert!(
            ink_height > 3 * ink_width,
            "{engine:?}: label ink is {ink_width} wide and {ink_height} tall, so it is not vertical"
        );
        // Rotating by −90° about the baseline origin turns the text to read upwards, so its ink ends at the origin's
        // height and extends to the right of the origin only by the depth of descenders.
        let (left, right) = (columns[0], columns[columns.len() - 1]);
        let (top, bottom) = (rows[0], rows[rows.len() - 1]);
        assert!(
            bottom <= 171 && right <= 46 && left >= 25 && top >= 60,
            "{engine:?}: label ink spans columns {left}..={right} and rows {top}..={bottom}, which does not read \
             upwards from (40, 170)"
        );
    }
}
