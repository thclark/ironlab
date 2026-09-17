//! LaTeX math typesetting tests.

mod common;

use common::{
    EPS, METRIC_TOL, advance_pt, all_text, find_glyph, find_math_glyph, find_math_glyphs, glyphs,
    ink_pt, rules, runs,
};
use ironlab_text::{FontId, TextEngine};
use latex_rust::{MathFont, MathParams};

const MINUS_SIGN: char = '\u{2212}';

fn script_scale() -> f64 {
    let font = MathFont::stix_two_math().expect("math face loads");
    let params = MathParams::from_font(&font).expect("math constants");
    f64::from(params.script_percent_scale_down) / 100.0
}

/// Returns the fonts of a layout's runs with consecutive repeats removed, which
/// identifies the sequence of plain and math segments independently of how
/// many runs a math segment needs for its different sizes.
fn segment_fonts(layout: &ironlab_text::TextLayout) -> Vec<FontId> {
    let mut fonts: Vec<FontId> = runs(layout).iter().map(|r| r.font).collect();
    fonts.dedup();
    fonts
}

/// Concatenates the text of the plain (regular face) runs of a layout.
fn plain_text(layout: &ironlab_text::TextLayout) -> String {
    runs(layout)
        .iter()
        .filter(|r| r.font == FontId::TextRegular)
        .map(|r| r.text.as_str())
        .collect()
}

/// Returns the pen position of the first glyph set in the regular face.
fn first_plain_x(layout: &ironlab_text::TextLayout) -> f64 {
    glyphs(layout)
        .into_iter()
        .find(|(run, _)| run.font == FontId::TextRegular)
        .map(|(_, g)| g.x)
        .expect("layout has plain text")
}

fn math_glyph_id(ch: char) -> u16 {
    common::reference_face(FontId::Math)
        .glyph_index(ch)
        .unwrap_or_else(|| panic!("STIX Two Math has {ch:?}"))
        .0
}

// latex-rust does not size script glyphs itself; superscripts must be drawn at the font's script scale and raised, or exponents such as 10^3 would look like 103.
#[test]
fn superscript_is_smaller_and_raised() {
    let engine = TextEngine::new();
    let layout = engine.layout("$x^2$", true, 10.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    assert!(runs(&layout).iter().all(|r| r.font == FontId::Math));

    let (base_run, base) = find_math_glyph(&layout, 'x');
    let (sup_run, sup) = find_glyph(&layout, "2");
    assert!((base_run.size_pt - 10.0).abs() < EPS);
    assert!(
        (sup_run.size_pt - 10.0 * script_scale()).abs() < EPS,
        "superscript size {} should be the snapped script size",
        sup_run.size_pt
    );
    assert!(base.y.abs() < EPS, "base sits on the baseline");
    assert!(sup.y < 0.0, "superscript is raised (y down)");
    assert!(
        sup.x >= base.x + advance_pt(base_run, base) - METRIC_TOL,
        "superscript is placed after the full advance of its base"
    );
    assert!(layout.height > 0.0);
    common::assert_extents_cover_ink(&layout);
}

// Placing a script at reduced size must make the expression narrower than writing the same characters at full size; this catches the latex-rust full-size script bug leaking through.
#[test]
fn superscript_is_narrower_than_full_size_characters() {
    let engine = TextEngine::new();
    let sup = engine.layout("$x^2$", true, 10.0);
    let x = engine.layout("$x$", true, 10.0);
    let two = engine.layout("$2$", true, 10.0);
    assert!(sup.width < x.width + two.width);
}

// Subscripts label components and free-stream quantities (u_∞, σ_xx); they must be lowered below the baseline at script size, not raised and not at full size.
#[test]
fn subscript_is_smaller_and_lowered() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"$u_{\infty}$", true, 10.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);

    let (base_run, base) = find_math_glyph(&layout, 'u');
    let (sub_run, sub) = find_glyph(&layout, "\u{221E}");
    assert_eq!(sub.id, math_glyph_id('\u{221E}'));
    assert!((base_run.size_pt - 10.0).abs() < EPS);
    assert!((sub_run.size_pt - 10.0 * script_scale()).abs() < EPS);
    assert!(base.y.abs() < EPS);
    assert!(sub.y > 0.0, "subscript is lowered (y down): {}", sub.y);
    assert!(sub.x >= base.x + advance_pt(base_run, base) - METRIC_TOL);
    assert!(
        layout.depth > 0.0,
        "the lowered subscript gives the label depth"
    );
    common::assert_extents_cover_ink(&layout);
}

// Fractions are common in labels (for example units per second); the numerator must sit above, the denominator below, with a visible bar between them, and the layout must report both extents so nothing is clipped. In text style TeX sets both parts at script size.
#[test]
fn fraction_stacks_numerator_rule_denominator() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"$\frac{a}{b}$", true, 10.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);

    let (num_run, num) = find_math_glyph(&layout, 'a');
    let (den_run, den) = find_math_glyph(&layout, 'b');
    assert!(num.y < 0.0 && den.y > 0.0, "num {} den {}", num.y, den.y);
    let script = 10.0 * script_scale();
    assert!((num_run.size_pt - script).abs() < EPS && (den_run.size_pt - script).abs() < EPS);

    let bars = rules(&layout);
    assert_eq!(bars.len(), 1, "{layout:#?}");
    let (x, y, w, h) = bars[0];
    assert!(w > 0.0 && h > 0.0);
    assert!(x >= -EPS && x + w <= layout.width + EPS);
    assert!(y < 0.0, "the bar sits on the math axis, above the baseline");

    let num_ink = ink_pt(num_run, num).expect("a has ink");
    let den_ink = ink_pt(den_run, den).expect("b has ink");
    assert!(num_ink.y1 <= y, "numerator ink is above the bar");
    assert!(den_ink.y0 >= y + h, "denominator ink is below the bar");
    for ink in [num_ink, den_ink] {
        assert!(
            ink.x0 >= x - METRIC_TOL && ink.x1 <= x + w + METRIC_TOL,
            "numerator and denominator lie within the bar: {ink:?} vs {:?}",
            (x, w)
        );
    }

    assert!(layout.height > 0.0 && layout.depth > 0.0);
    common::assert_extents_cover_ink(&layout);
}

// Square roots appear in labels such as normalised quantities; the radical sign must precede the radicand and its overbar must be drawn above the radicand's ink and span it.
#[test]
fn square_root_draws_sign_and_overbar_above_radicand() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"$\sqrt{x}$", true, 10.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);

    let (surd_run, surd) = find_glyph(&layout, "\u{221A}");
    let (x_run, x) = find_math_glyph(&layout, 'x');
    assert_eq!(surd_run.font, FontId::Math);
    assert!(
        (x_run.size_pt - 10.0).abs() < EPS,
        "radicand stays at text size"
    );
    assert!(x.y.abs() < EPS);
    assert!(surd.x < x.x, "radical sign precedes the radicand");

    let bars = rules(&layout);
    assert_eq!(bars.len(), 1, "{layout:#?}");
    let (bar_x, bar_y, bar_w, bar_h) = bars[0];
    let x_ink = ink_pt(x_run, x).expect("x has ink");
    assert!(bar_h > 0.0);
    assert!(
        bar_y + bar_h <= x_ink.y0,
        "overbar is above the radicand ink"
    );
    assert!(
        bar_x <= x.x + METRIC_TOL && bar_x + bar_w >= x.x + advance_pt(x_run, x) - METRIC_TOL,
        "overbar spans the radicand's advance: bar {:?}, x at {}",
        (bar_x, bar_w),
        x.x
    );
    common::assert_extents_cover_ink(&layout);
}

// Greek letters are the most common math in scientific labels; they must resolve to the real STIX Two Math glyph, not a missing-glyph box.
#[test]
fn alpha_maps_to_the_math_face_glyph() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"$\alpha$", true, 10.0);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    assert_eq!(glyphs(&layout).len(), 1, "{layout:#?}");
    let (run, g) = find_math_glyph(&layout, '\u{03B1}');
    assert_eq!(run.font, FontId::Math);
    let ch = common::glyph_text(run, g)
        .chars()
        .next()
        .expect("glyph text is one character");
    let expected = math_glyph_id(ch);
    assert_ne!(expected, 0);
    assert_eq!(g.id, expected, "glyph id matches the character it claims");
}

// TeX sets math variables in italic, and published figures are expected to match the body text of the paper; an unstyled letter must therefore be drawn with the Mathematical Italic glyph of the math face (U+1D465 for x), not the upright letter.
#[test]
fn math_letters_are_mathematical_italic() {
    let engine = TextEngine::new();
    let layout = engine.layout("$x$", true, 10.0);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    let (run, g) = find_glyph(&layout, "\u{1D465}");
    assert_eq!(run.font, FontId::Math);
    assert_eq!(g.id, math_glyph_id('\u{1D465}'));
    assert_ne!(
        g.id,
        math_glyph_id('x'),
        "the italic glyph differs from upright x"
    );
}

// Operators and dimensionless groups such as Re must be set upright with \mathrm, whatever convention is used for math variables, and must map to the upright glyphs of the math face.
#[test]
fn mathrm_is_upright() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"$\mathrm{Re}_{\tau}$", true, 10.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    for ch in ['R', 'e'] {
        let (run, g) = find_glyph(&layout, &ch.to_string());
        assert_eq!(run.font, FontId::Math);
        assert_eq!(g.id, math_glyph_id(ch), "{ch:?} is the upright glyph");
        assert!((run.size_pt - 10.0).abs() < EPS && g.y.abs() < EPS);
    }
    let (tau_run, tau) = find_math_glyph(&layout, '\u{03C4}');
    assert!(tau.y > 0.0 && tau_run.size_pt < 10.0);
}

// Negative tick labels and exponents must use the typographic minus sign (U+2212) from the math face, as TeX does; a hyphen is visibly too short and too high. The replacement must happen before layout, so that the following digit is spaced for the wider minus rather than overlapping it.
#[test]
fn hyphen_in_math_is_typeset_as_minus_sign() {
    let engine = TextEngine::new();
    let layout = engine.layout("$-3$", true, 10.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    assert!(
        !all_text(&layout).contains('-'),
        "no hyphen-minus glyph remains: {layout:#?}"
    );

    let (minus_run, minus) = find_glyph(&layout, &MINUS_SIGN.to_string());
    let (three_run, three) = find_glyph(&layout, "3");
    assert_eq!(minus_run.font, FontId::Math);
    assert_eq!(minus.id, math_glyph_id(MINUS_SIGN));
    assert!(
        three.x >= minus.x + advance_pt(minus_run, minus) - METRIC_TOL,
        "the digit follows the full advance of the minus sign"
    );
    let minus_ink = ink_pt(minus_run, minus).expect("minus has ink");
    let three_ink = ink_pt(three_run, three).expect("3 has ink");
    assert!(
        minus_ink.x1 <= three_ink.x0,
        "minus and digit do not overlap"
    );
}

// Log-axis tick labels are generated as $10^{n}$ with negative exponents; these must typeset without warnings, raise the whole exponent at script size, and use a real minus sign.
#[test]
fn tick_label_exponent_is_raised_and_scaled() {
    let engine = TextEngine::new();
    let layout = engine.layout("$10^{-3}$", true, 9.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    let (one_run, one) = find_glyph(&layout, "1");
    let (_, zero) = find_glyph(&layout, "0");
    let (minus_run, minus) = find_glyph(&layout, &MINUS_SIGN.to_string());
    let (three_run, three) = find_glyph(&layout, "3");
    assert_eq!(minus.id, math_glyph_id(MINUS_SIGN));
    assert!(one.y.abs() < EPS && zero.y.abs() < EPS);
    assert!(
        minus.y < 0.0 && (three.y - minus.y).abs() < EPS,
        "exponent raised as a unit"
    );
    assert!((one_run.size_pt - 9.0).abs() < EPS);
    for run in [minus_run, three_run] {
        assert!((run.size_pt - 9.0 * script_scale()).abs() < EPS);
    }
    assert!(one.x < zero.x && zero.x < minus.x && minus.x < three.x);
    common::assert_extents_cover_ink(&layout);
}

// A typical stress-axis label: math followed by plain units. The subscript pair must be lowered at script size, and the plain text must follow the math on the shared baseline.
#[test]
fn subscripted_symbol_with_plain_units() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"$\sigma_{xx}$ (MPa)", true, 9.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);

    assert_eq!(segment_fonts(&layout), [FontId::Math, FontId::TextRegular]);
    assert_eq!(plain_text(&layout), " (MPa)");

    let (sigma_run, sigma) = find_math_glyph(&layout, '\u{03C3}');
    assert!(sigma.y.abs() < EPS && (sigma_run.size_pt - 9.0).abs() < EPS);
    let subs = find_math_glyphs(&layout, 'x');
    assert_eq!(subs.len(), 2, "{layout:#?}");
    for (run, g) in &subs {
        assert!(g.y > 0.0 && (run.size_pt - 9.0 * script_scale()).abs() < EPS);
    }
    assert!(subs[0].1.x < subs[1].1.x);

    let math_width = engine.layout(r"$\sigma_{xx}$", true, 9.0).width;
    assert!(first_plain_x(&layout) >= math_width - EPS);
}

// A typical mass-flow label: an accented symbol, plain units and a math exponent. The dot accent must sit above its base, and each math segment must typeset independently on one baseline.
#[test]
fn accent_and_exponent_in_unit_label() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"$\dot{m}$ / kg s$^{-1}$", true, 9.0);
    common::assert_well_formed(&layout);
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);

    assert_eq!(
        segment_fonts(&layout),
        [FontId::Math, FontId::TextRegular, FontId::Math]
    );
    assert_eq!(plain_text(&layout), " / kg s");

    let (m_run, m) = find_math_glyph(&layout, 'm');
    let plain_start = first_plain_x(&layout);
    let accented: Vec<_> = glyphs(&layout)
        .into_iter()
        .filter(|(run, g)| run.font == FontId::Math && g.x < plain_start)
        .collect();
    assert_eq!(accented.len(), 2, "m and its dot: {layout:#?}");
    let (dot_run, dot) = accented
        .into_iter()
        .find(|(_, g)| g.id != m.id)
        .expect("accent glyph");
    let m_ink = ink_pt(m_run, m).expect("m has ink");
    let dot_ink = ink_pt(dot_run, dot).expect("dot has ink");
    assert!(
        dot_ink.y1 <= m_ink.y0,
        "dot is above m: {dot_ink:?} vs {m_ink:?}"
    );
    assert!(
        dot_ink.x0 >= m_ink.x0 && dot_ink.x1 <= m_ink.x1,
        "dot is over m horizontally"
    );

    let (minus_run, minus) = find_glyph(&layout, &MINUS_SIGN.to_string());
    let (_, one) = find_glyph(&layout, "1");
    assert!(minus.y < 0.0 && one.y < 0.0);
    assert!(minus_run.size_pt < 9.0);
    let s_end = engine.layout(r"$\dot{m}$ / kg s", true, 9.0).width;
    assert!(minus.x >= s_end - EPS, "exponent follows the plain units");
}

// Consumers align labels on the baseline using height and depth. Math extents are those of the latex-rust box, which follow the ink, so a lone $t$ is shorter than the plain text strut; when mixed with plain text, the label takes the larger of each extent.
#[test]
fn math_extents_follow_the_box_and_mixed_labels_take_the_maximum() {
    let engine = TextEngine::new();
    let math = engine.layout("$t$", true, 10.0);
    let plain = engine.layout("t", false, 10.0);
    let (run, g) = find_math_glyph(&math, 't');
    let ink = ink_pt(run, g).expect("t has ink");
    assert!(
        (math.height - (-ink.y0).max(0.0)).abs() < METRIC_TOL,
        "height {} vs ink top {}",
        math.height,
        -ink.y0
    );
    assert!(
        (math.depth - ink.y1.max(0.0)).abs() < METRIC_TOL,
        "depth {} vs ink bottom {}",
        math.depth,
        ink.y1
    );
    assert!(math.height < plain.height && math.depth < plain.depth);

    let mixed = engine.layout("t $t$", true, 10.0);
    assert!((mixed.height - plain.height).abs() < METRIC_TOL);
    assert!((mixed.depth - plain.depth).abs() < METRIC_TOL);

    let tall = engine.layout(r"t $\frac{a}{b}$", true, 10.0);
    let frac = engine.layout(r"$\frac{a}{b}$", true, 10.0);
    assert!((tall.height - plain.height.max(frac.height)).abs() < METRIC_TOL);
    assert!((tall.depth - plain.depth.max(frac.depth)).abs() < METRIC_TOL);
}

// Label authors write unsupported or misspelled commands; the figure must still build, show the author the raw source, and surface a warning naming the problem.
#[test]
fn unsupported_command_falls_back_with_warning() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"$\notarealcommand{x}$", true, 9.0);
    common::assert_well_formed(&layout);
    assert_eq!(layout.warnings.len(), 1, "{:?}", layout.warnings);
    assert_eq!(layout.warnings[0].source, r"$\notarealcommand{x}$");
    assert!(!layout.warnings[0].message.is_empty());

    let all = runs(&layout);
    assert!(!all.is_empty() && all.iter().all(|r| !r.glyphs.is_empty()));
    assert!(all.iter().all(|r| r.font == FontId::TextRegular));
    assert_eq!(all_text(&layout), r"$\notarealcommand{x}$");
    assert!(layout.width > 0.0);
}

// Only the failing math segment should fall back; surrounding text and other valid math in the same label must still typeset normally, in source order.
#[test]
fn fallback_is_limited_to_the_failing_segment() {
    let engine = TextEngine::new();
    let layout = engine.layout(r"Rate $\bogus$ and $\alpha$", true, 9.0);
    common::assert_well_formed(&layout);
    assert_eq!(layout.warnings.len(), 1, "{:?}", layout.warnings);
    assert_eq!(layout.warnings[0].source, r"$\bogus$");

    assert_eq!(plain_text(&layout), r"Rate $\bogus$ and ");
    let last = runs(&layout).last().copied().expect("runs");
    assert_eq!(last.font, FontId::Math);
    find_math_glyph(&layout, '\u{03B1}');
}

// Arbitrary user input must never panic the renderer or produce non-finite geometry, whatever state the math parser is left in.
#[test]
fn malformed_input_never_panics() {
    let engine = TextEngine::new();
    for source in [
        "$",
        "$$",
        "$ $",
        "\\",
        "\\$",
        "$\\$",
        "$x^$",
        r"$\frac{a}$",
        "$}$",
        r"$\left($",
        "a $b$ $c",
        "$\u{1F600}$",
    ] {
        let layout = engine.layout(source, true, 9.0);
        common::assert_well_formed(&layout);
    }
}

// latex-rust parses and lays out recursively and overflows the stack of an ordinary thread at a few dozen levels of nesting, which aborts the whole process rather than panicking. Generated or malicious labels must fall back to raw text with a warning instead.
#[test]
fn deeply_nested_math_falls_back_without_overflowing() {
    let engine = TextEngine::new();
    let depth = 500;
    let sources = [
        format!("${}{}$", "x^{".repeat(depth), "}".repeat(depth)),
        format!("${}x{}$", "{".repeat(depth), "}".repeat(depth)),
        format!("${}x{}$", r"\frac{1}{".repeat(depth), "}".repeat(depth)),
    ];
    for source in sources {
        let layout = engine.layout(&source, true, 9.0);
        common::assert_well_formed(&layout);
        assert_eq!(layout.warnings.len(), 1, "one warning for {source:.20}");
        assert_eq!(all_text(&layout), source);
    }
}

// The engine is called from deep GUI stacks and from threads with small stacks. Math nested as deeply as the engine admits must still typeset there, because the recursive typesetting must not run on the caller's stack.
#[test]
fn admitted_deep_nesting_typesets_on_a_small_caller_stack() {
    let engine = TextEngine::new();
    let depth = 20;
    let source = format!("${}x{}$", r"\frac{1}{".repeat(depth), "}".repeat(depth));
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn_scoped(scope, || {
                let layout = engine.layout(&source, true, 9.0);
                common::assert_well_formed(&layout);
                assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
                assert_eq!(rules(&layout).len(), depth, "one bar per fraction");
            })
            .expect("thread starts")
            .join()
            .expect("layout on a small stack does not fail");
    });
}
