//! Tests for text laid out emboldened, for projection.
//!
//! WHY: a figure drawn for a projector must have heavier text than the same figure drawn for print, without the
//! author rewriting a single label. Plain text has heavier designed faces (STIX Two Text Medium and SemiBold), so it is
//! set in them. Bold, monospace and mathematics have no heavier face, so they keep their faces and are stroked along
//! their outlines by as much as the step thickens the stems of plain text, so that every kind of text in a label
//! thickens alike and bold still stands out from the text beside it.

mod common;

use std::sync::Arc;

use common::{METRIC_TOL, glyph_text, glyphs, rules, runs};
use ironlab_text::{Embolden, FontId, GlyphRun, TextEngine};
use ttf_parser::{GlyphId, OutlineBuilder};

const STEPS: [Embolden; 3] = [Embolden::Regular, Embolden::Medium, Embolden::Semibold];

/// The face in which a step sets plain text.
fn plain_face(step: Embolden) -> FontId {
    match step {
        Embolden::Regular => FontId::TextRegular,
        Embolden::Medium => FontId::TextMedium,
        Embolden::Semibold => FontId::TextSemibold,
    }
}

/// Returns the runs of a layout that are not plain text: bold, monospace and mathematics.
fn stroked_kinds<'a>(runs: &[&'a GlyphRun]) -> Vec<&'a GlyphRun> {
    runs.iter()
        .copied()
        .filter(|run| matches!(run.font, FontId::TextBold | FontId::Math))
        .collect()
}

// Plain text must be set in the designed face of the step, both when LaTeX is interpreted and when it is not, because
// a designed face is drawn better than a stroked one, and it must carry no stroke, which would thicken it twice.
#[test]
fn each_step_sets_plain_text_in_the_designed_face_of_its_weight() {
    let engine = TextEngine::new();
    for step in STEPS {
        for parse_math in [true, false] {
            let layout = engine.layout_emboldened("Pressure (kPa)", parse_math, 10.0, step);
            common::assert_well_formed(&layout);
            assert!(!runs(&layout).is_empty());
            for run in runs(&layout) {
                assert_eq!(
                    run.font,
                    plain_face(step),
                    "{step:?}, parse_math {parse_math}: {run:?}"
                );
                assert_eq!(
                    run.stroke_em, 0.0,
                    "{step:?}: plain text is not stroked: {run:?}"
                );
            }
        }
    }
}

// The glyphs of a heavier face must be the ones that face maps the characters to, and its own metrics must be used:
// the faces number their glyphs alike, so the identifiers alone could not show that the face was swapped, but the
// advances, read here from the font files, differ between weights. The word has no kerning pair in these faces, so its
// width is the sum of its advances; the regular face is checked the same way, so that a kerning pair added by a later
// font update shows up here as a broken premise rather than as a wrong face.
#[test]
fn plain_text_is_spaced_by_the_metrics_of_the_face_of_its_step() {
    let engine = TextEngine::new();
    let word = "minimum";
    let advances = |font: FontId| -> f64 {
        let face = common::reference_face(font);
        word.chars()
            .map(|ch| {
                let id = face.glyph_index(ch).expect("the face has the character");
                f64::from(face.glyph_hor_advance(id).expect("advance")) * 10.0
                    / f64::from(face.units_per_em())
            })
            .sum()
    };
    let regular_width = engine.layout(word, false, 10.0).width;
    assert!(
        (regular_width - advances(FontId::TextRegular)).abs() < METRIC_TOL,
        "{word:?} is not kerned in the regular face"
    );
    for step in [Embolden::Medium, Embolden::Semibold] {
        let layout = engine.layout_emboldened(word, false, 10.0, step);
        let expected = advances(plain_face(step));
        assert!(
            (expected - regular_width).abs() > 10.0 * METRIC_TOL,
            "{step:?}: the face's advances differ from the regular face's"
        );
        assert!(
            (layout.width - expected).abs() < METRIC_TOL,
            "{step:?}: width {} against {expected} from the font",
            layout.width
        );
        let face = common::reference_face(plain_face(step));
        for (run, glyph) in glyphs(&layout) {
            let ch = glyph_text(run, glyph).chars().next().unwrap();
            assert_eq!(
                Some(GlyphId(glyph.id)),
                face.glyph_index(ch),
                "{step:?}: {ch:?}"
            );
        }
    }
}

// Bold, monospace and mathematics keep their faces at every step, because there is no heavier face to set them in;
// at a heavier step they are stroked by the calibrated amount, which is what makes them as much heavier as the plain
// text beside them. Without the stroke a slide would show heavy body text beside thin mathematics.
#[test]
fn bold_monospace_and_mathematics_keep_their_faces_and_are_stroked_by_the_step() {
    let engine = TextEngine::new();
    for (label, face) in [
        (r"\textbf{xpanel.f}", FontId::TextBold),
        (r"\texttt{case_01}", FontId::Math),
        (r"$\alpha_0 + x^2$", FontId::Math),
    ] {
        for step in STEPS {
            let layout = engine.layout_emboldened(label, true, 10.0, step);
            common::assert_well_formed(&layout);
            assert!(layout.warnings.is_empty(), "{label}: {:?}", layout.warnings);
            let all = runs(&layout);
            assert!(!all.is_empty(), "{label}");
            for run in all {
                assert_eq!(run.font, face, "{label} at {step:?}: {run:?}");
                assert!(
                    (run.stroke_em - step.stroke_em()).abs() < 1e-12,
                    "{label} at {step:?}: stroke {} against {}",
                    run.stroke_em,
                    step.stroke_em()
                );
            }
        }
    }
}

// In a label that mixes kinds of text, each run is emboldened in its own way and in its own place: the plain text by
// its face alone, and the bold, monospace and mathematical runs by the stroke alone.
#[test]
fn a_mixed_label_emboldens_each_run_in_its_own_way() {
    let engine = TextEngine::new();
    let step = Embolden::Semibold;
    let layout = engine.layout_emboldened(
        r"in \textbf{xbl.f} at \texttt{n160}, $\Delta p$",
        true,
        10.0,
        step,
    );
    assert!(layout.warnings.is_empty(), "{:?}", layout.warnings);
    let all = runs(&layout);
    let plain: Vec<_> = all
        .iter()
        .filter(|run| run.font == FontId::TextSemibold)
        .collect();
    let stroked = stroked_kinds(&all);
    assert!(!plain.is_empty() && stroked.len() >= 3, "{all:?}");
    assert_eq!(
        plain.len() + stroked.len(),
        all.len(),
        "no run is in another face: {all:?}"
    );
    assert!(plain.iter().all(|run| run.stroke_em == 0.0), "{plain:?}");
    assert!(
        stroked
            .iter()
            .all(|run| (run.stroke_em - step.stroke_em()).abs() < 1e-12),
        "{stroked:?}"
    );
}

// A figure drawn regular must be drawn exactly as before emboldening existed, so the regular step must give the very
// layout that `layout` gives, with no stroke anywhere.
#[test]
fn the_regular_step_is_the_designed_layout() {
    let engine = TextEngine::new();
    for label in [
        r"Pressure $\frac{p}{p_0}$",
        r"\textbf{xbl.f} \texttt{n160}",
        "plain",
    ] {
        let designed = engine.layout(label, true, 9.0);
        let regular = engine.layout_emboldened(label, true, 9.0, Embolden::Regular);
        assert_eq!(*regular, *designed, "{label}");
        assert!(
            runs(&regular).iter().all(|run| run.stroke_em == 0.0),
            "{label}"
        );
    }
    assert_eq!(Embolden::Regular.stroke_em(), 0.0);
    assert_eq!(Embolden::Regular.line_scale(), 1.0);
}

// The memo key must include the step, or a label laid out for the print figure would be served to the slide figure
// (or the other way round) when both are drawn by one engine, as they are when one figure is exported at two steps.
#[test]
fn memo_distinguishes_steps() {
    let engine = TextEngine::new();
    let label = r"Time (s) $t$";
    let regular = engine.layout_emboldened(label, true, 9.0, Embolden::Regular);
    let medium = engine.layout_emboldened(label, true, 9.0, Embolden::Medium);
    let semibold = engine.layout_emboldened(label, true, 9.0, Embolden::Semibold);
    let fonts = |layout: &ironlab_text::TextLayout| {
        runs(layout).iter().map(|run| run.font).collect::<Vec<_>>()
    };
    assert!(
        fonts(&regular).contains(&FontId::TextRegular),
        "{regular:?}"
    );
    assert!(fonts(&medium).contains(&FontId::TextMedium), "{medium:?}");
    assert!(
        fonts(&semibold).contains(&FontId::TextSemibold),
        "{semibold:?}"
    );
    assert_ne!(*regular, *medium);
    assert_ne!(*medium, *semibold);
    // Asking again, in another order, is served from the memo, each step its own layout.
    assert!(Arc::ptr_eq(
        &semibold,
        &engine.layout_emboldened(label, true, 9.0, Embolden::Semibold)
    ));
    assert!(Arc::ptr_eq(&regular, &engine.layout(label, true, 9.0)));
    assert!(Arc::ptr_eq(
        &medium,
        &engine.layout_emboldened(label, true, 9.0, Embolden::Medium)
    ));
}

// A fraction bar or a radical bar is drawn as a rule beside the stroked glyphs of the mathematics; unless it is
// thickened by the same stroke, every edge moving out by half of it, the bars would look thin beside heavy symbols.
#[test]
fn math_rules_are_thickened_by_the_stroke_about_their_centre() {
    let engine = TextEngine::new();
    let size = 10.0;
    for label in [r"$\frac{a}{b}$", r"$\sqrt{x}$"] {
        let regular = rules(&engine.layout_emboldened(label, true, size, Embolden::Regular));
        assert!(!regular.is_empty(), "{label} draws a rule");
        for step in [Embolden::Medium, Embolden::Semibold] {
            let stroke = step.stroke_em() * size;
            let heavy = rules(&engine.layout_emboldened(label, true, size, step));
            assert_eq!(heavy.len(), regular.len(), "{label} at {step:?}");
            for (&(_, ry, rw, rh), &(_, hy, hw, hh)) in regular.iter().zip(&heavy) {
                assert!(
                    (hh - (rh + stroke)).abs() < METRIC_TOL,
                    "{label} at {step:?}: height {hh} against {rh} + {stroke}"
                );
                assert!(
                    (hw - (rw + stroke)).abs() < METRIC_TOL,
                    "{label} at {step:?}: width {hw} against {rw} + {stroke}"
                );
                assert!(
                    ((hy + hh / 2.0) - (ry + rh / 2.0)).abs() < METRIC_TOL,
                    "{label} at {step:?}: the bar stays on the math axis"
                );
            }
        }
    }
}

// The stroke draws ink half its width outside every outline, so the height and depth reported for a stroked label
// must enclose that ink too; otherwise the stroke of a tick label would overlap the axis, or a title's would be clipped
// at the edge of the page without the `TextOutsidePage` warning that measurement exists to give.
#[test]
fn extents_cover_the_ink_of_the_stroke() {
    let engine = TextEngine::new();
    for label in [
        r"$\frac{\int_0^1 f\,dx}{g_y}$",
        r"\textbf{Ag}",
        r"\texttt{jy}",
        "Ag",
    ] {
        let layout = engine.layout_emboldened(label, true, 10.0, Embolden::Semibold);
        for (run, glyph) in glyphs(&layout) {
            let half = run.stroke_em * run.size_pt / 2.0;
            if let Some(ink) = common::ink_pt(run, glyph) {
                assert!(
                    -(ink.y0 - half) <= layout.height + METRIC_TOL
                        && ink.y1 + half <= layout.depth + METRIC_TOL,
                    "{label}: stroked ink of {:?} exceeds height {} / depth {}",
                    glyph_text(run, glyph),
                    layout.height,
                    layout.depth
                );
            }
        }
        for (_, y, _, h) in rules(&layout) {
            assert!(
                -y <= layout.height + METRIC_TOL && y + h <= layout.depth + METRIC_TOL,
                "{label}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------
// Calibration against the bundled faces
// ---------------------------------------------------------------------------------

/// Collects the edges of a glyph outline as straight segments, flattening curves finely.
#[derive(Default)]
struct Segments {
    edges: Vec<((f64, f64), (f64, f64))>,
    start: (f64, f64),
    at: (f64, f64),
}

const FLATTEN_STEPS: usize = 64;

impl Segments {
    fn line(&mut self, to: (f64, f64)) {
        self.edges.push((self.at, to));
        self.at = to;
    }
}

impl OutlineBuilder for Segments {
    fn move_to(&mut self, x: f32, y: f32) {
        self.start = (f64::from(x), f64::from(y));
        self.at = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.line((f64::from(x), f64::from(y)));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p0 = self.at;
        let (p1, p2) = ((f64::from(x1), f64::from(y1)), (f64::from(x), f64::from(y)));
        for i in 1..=FLATTEN_STEPS {
            let t = i as f64 / FLATTEN_STEPS as f64;
            let u = 1.0 - t;
            self.line((
                u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
                u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
            ));
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p0 = self.at;
        let (p1, p2, p3) = (
            (f64::from(x1), f64::from(y1)),
            (f64::from(x2), f64::from(y2)),
            (f64::from(x), f64::from(y)),
        );
        for i in 1..=FLATTEN_STEPS {
            let t = i as f64 / FLATTEN_STEPS as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.line((
                a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
            ));
        }
    }

    fn close(&mut self) {
        if self.at != self.start {
            self.line(self.start);
        }
    }
}

/// Measures the vertical stem of the lowercase "l" of a face, in font units: the horizontal distance across its
/// outline at half the face's x-height, where the stem is clear of serifs at both ends.
fn stem_of_l(font: FontId) -> f64 {
    let face = common::reference_face(font);
    let glyph = face.glyph_index('l').expect("the face has an l");
    let mut outline = Segments::default();
    face.outline_glyph(glyph, &mut outline)
        .expect("the l has an outline");
    let y = f64::from(face.x_height().expect("the face records its x-height")) / 2.0;
    let mut crossings: Vec<f64> = outline
        .edges
        .iter()
        .filter(|((_, y0), (_, y1))| (y0 - y) * (y1 - y) < 0.0)
        .map(|&((x0, y0), (x1, y1))| x0 + (y - y0) * (x1 - x0) / (y1 - y0))
        .collect();
    crossings.sort_by(f64::total_cmp);
    assert_eq!(
        crossings.len(),
        2,
        "{font:?}: one stem crosses half the x-height: {crossings:?}"
    );
    (crossings[1] - crossings[0]) * 1000.0 / f64::from(face.units_per_em())
}

// The line scale and the stroke are constants calibrated from the stems of the bundled faces, so that lines thicken in
// proportion to the stems of plain text and the stroked kinds of text thicken by as much as plain text does. If the
// fonts are ever updated, these constants must be recalibrated with them, and this test says so.
#[test]
fn the_constants_of_each_step_follow_the_stems_of_the_bundled_faces() {
    let regular = stem_of_l(FontId::TextRegular);
    let bold = stem_of_l(FontId::TextBold);
    assert!(
        regular > 0.0 && bold > regular,
        "regular {regular}, bold {bold}"
    );
    let mut previous = regular;
    for step in [Embolden::Medium, Embolden::Semibold] {
        let stem = stem_of_l(plain_face(step));
        assert!(
            stem > previous && stem < bold,
            "{step:?}: stem {stem} lies between the lighter step and bold"
        );
        previous = stem;

        let line_scale = stem / regular;
        assert!(
            (step.line_scale() - line_scale).abs() < 0.02,
            "{step:?}: line scale {} against {line_scale} measured",
            step.line_scale()
        );
        // A stroke of `stroke_em` adds its whole width to a stem, half on each side.
        let stroke_em = (stem - regular) / 1000.0;
        assert!(
            (step.stroke_em() - stroke_em).abs() < 0.002,
            "{step:?}: stroke {} em against {stroke_em} measured",
            step.stroke_em()
        );
    }
}
