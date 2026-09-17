//! Robustness: display items that cannot be represented in a PDF are skipped, and the PDF stays valid.
//!
//! The scene compiler never intends to emit such items, but a single NaN from user data or a numerical corner case
//! must not cost the user the whole export or produce a file that some viewers reject.

use ironlab_scene::display::{
    Fill, FillRule, GlyphsItem, Item, ItemKind, PathItem, PathSegment, PlacedGlyph, Point, Rect,
    Rgba, Transform,
};
use ironlab_text::TextItem;

use crate::common::*;

/// The item drawn after the problematic one, which must still appear.
const GOOD: Rect = Rect::new(120.0, 20.0, 40.0, 40.0);

/// The region in which the problematic items would leave ink if they were drawn in part.
const BAD_REGION: Rect = Rect::new(0.0, 0.0, 100.0, 100.0);

/// Whether the problematic item must leave no ink at all, or may be drawn in a corrected form.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// The item has non-finite geometry and must be skipped entirely.
    Skipped,
    /// The item has an invalid attribute that may either be skipped or be corrected (for example by clamping).
    SkippedOrCorrected,
}

/// Renders `bad` followed by a good rectangle and checks that the PDF is valid and the good rectangle is drawn.
fn assert_survives(name: &str, bad: Item, expect: Expect) {
    let text = engine();
    let mut list = page(200.0, 100.0);
    list.items.push(bad);
    list.items.push(filled_rect(GOOD, BLUE));

    let bytes = render(&list, &text);
    assert!(bytes.starts_with(b"%PDF-"), "{name}: output is not a PDF");

    if !tools_available(&["pdfinfo", RASTER_TOOLS[0], RASTER_TOOLS[1]]) {
        return;
    }
    let ws = Workspace::new(name);
    let pdf = ws.write_pdf("figure", &bytes);
    assert_eq!(pdfinfo(&pdf, false)["Pages"], "1", "{name}: page count");
    for engine in ENGINES {
        // `rasterise` fails on any error or warning either engine reports while reading the file.
        let image = rasterise(&pdf, engine);
        assert_pixel(
            engine,
            &image,
            140.5,
            40.5,
            [0, 0, 255],
            3,
            &format!("{name}: item after the invalid one"),
        );
        if expect == Expect::Skipped {
            let ink = count_pixels(&image, BAD_REGION, is_ink);
            assert_eq!(
                ink, 0,
                "{engine:?}: {name}: the invalid item was partly drawn ({ink} ink pixels)"
            );
        }
    }
}

fn black_fill() -> Option<Fill> {
    Some(Fill {
        color: Rgba::BLACK,
        rule: FillRule::NonZero,
    })
}

/// A filled black quadrilateral in [`BAD_REGION`] whose third vertex is `third`.
fn quad_with_vertex(third: Point) -> Item {
    item(ItemKind::Path(PathItem {
        segments: vec![
            PathSegment::MoveTo(Point::new(10.0, 10.0)),
            PathSegment::LineTo(Point::new(90.0, 10.0)),
            PathSegment::LineTo(third),
            PathSegment::LineTo(Point::new(10.0, 90.0)),
            PathSegment::Close,
        ],
        fill: black_fill(),
        stroke: None,
    }))
}

#[test]
fn path_with_a_nan_coordinate_is_skipped() {
    // WHY: a NaN written into a content stream is a syntax error that viewers handle inconsistently; dropping just the
    // bad point would instead draw a shape the data does not describe.
    assert_survives(
        "nan-vertex",
        quad_with_vertex(Point::new(f64::NAN, 90.0)),
        Expect::Skipped,
    );
}

#[test]
fn path_with_an_infinite_coordinate_is_skipped() {
    // WHY: infinite coordinates arise from log scales of zero and divisions by zero, and cannot be written as PDF
    // numbers.
    assert_survives(
        "infinite-vertex",
        quad_with_vertex(Point::new(90.0, f64::INFINITY)),
        Expect::Skipped,
    );
}

#[test]
fn path_with_a_non_finite_control_point_is_skipped() {
    // WHY: control points are checked as well as end points, since a cubic with a bad control point is as invalid as a
    // line to a bad vertex.
    let bad = item(ItemKind::Path(PathItem {
        segments: vec![
            PathSegment::MoveTo(Point::new(10.0, 50.0)),
            PathSegment::CubicTo(
                Point::new(30.0, f64::NEG_INFINITY),
                Point::new(70.0, 10.0),
                Point::new(90.0, 50.0),
            ),
            PathSegment::Close,
        ],
        fill: black_fill(),
        stroke: None,
    }));
    assert_survives("infinite-control-point", bad, Expect::Skipped);
}

#[test]
fn path_that_does_not_start_with_a_move_is_not_written_as_invalid_operators() {
    // WHY: a PDF line operator without a current point is an error; an unguarded exporter writes it verbatim.
    let bad = item(ItemKind::Path(PathItem {
        segments: vec![
            PathSegment::LineTo(Point::new(90.0, 10.0)),
            PathSegment::LineTo(Point::new(90.0, 90.0)),
            PathSegment::Close,
        ],
        fill: black_fill(),
        stroke: None,
    }));
    assert_survives("line-without-move", bad, Expect::SkippedOrCorrected);
}

#[test]
fn stroke_with_a_non_finite_width_is_skipped() {
    // WHY: stroke widths are computed from marker and line sizes and can become NaN or infinite, which are not valid
    // line widths.
    for (i, width) in [f64::NAN, f64::INFINITY].into_iter().enumerate() {
        assert_survives(
            &format!("non-finite-stroke-width-{i}"),
            stroked_line(
                Point::new(10.0, 50.0),
                Point::new(90.0, 50.0),
                solid_stroke(Rgba::BLACK, width),
            ),
            Expect::Skipped,
        );
    }
}

#[test]
fn stroke_with_a_negative_width_does_not_produce_an_invalid_pdf() {
    // WHY: a negative line width is invalid in PDF; it must be dropped or corrected rather than written.
    assert_survives(
        "negative-stroke-width",
        stroked_line(
            Point::new(10.0, 50.0),
            Point::new(90.0, 50.0),
            solid_stroke(Rgba::BLACK, -2.0),
        ),
        Expect::SkippedOrCorrected,
    );
}

#[test]
fn invalid_dash_arrays_do_not_produce_an_invalid_pdf() {
    // WHY: a dash array that is all zeros or contains negative or non-finite lengths, or a non-finite dash phase, is
    // invalid in PDF and has hung or crashed some viewers; it must be dropped or corrected before writing.
    for (i, (dash, offset)) in [
        (vec![0.0, 0.0], 0.0),
        (vec![-4.0, 2.0], 0.0),
        (vec![f64::NAN, 3.0], 0.0),
        (vec![4.0, 2.0], f64::NAN),
    ]
    .into_iter()
    .enumerate()
    {
        let mut stroke = solid_stroke(Rgba::BLACK, 2.0);
        stroke.dash = dash;
        stroke.dash_offset = offset;
        assert_survives(
            &format!("invalid-dash-{i}"),
            stroked_line(Point::new(10.0, 50.0), Point::new(90.0, 50.0), stroke),
            Expect::SkippedOrCorrected,
        );
    }
}

#[test]
fn colour_components_outside_the_unit_interval_do_not_produce_an_invalid_pdf() {
    // WHY: colours are interpolated from colormaps and can overshoot [0, 1] numerically; PDF colour operands outside
    // their range are errors in strict consumers.
    let bad = filled_rect(
        Rect::new(10.0, 10.0, 80.0, 80.0),
        Rgba::new(1.5, -0.2, f32::NAN, 2.0),
    );
    assert_survives("colour-out-of-range", bad, Expect::SkippedOrCorrected);
}

/// A glyph run of "Time" placed in [`BAD_REGION`], with `edit` applied to it.
fn glyph_run(edit: impl FnOnce(&mut GlyphsItem)) -> Item {
    let text = engine();
    let layout = text.layout("Time", false, 12.0);
    let TextItem::Glyphs(run) = &layout.items[0] else {
        panic!("\"Time\" did not lay out to a glyph run: {layout:?}");
    };
    let mut glyphs = GlyphsItem {
        font: run.font,
        size_pt: run.size_pt,
        color: Rgba::BLACK,
        text: run.text.clone(),
        glyphs: run
            .glyphs
            .iter()
            .map(|g| PlacedGlyph {
                id: g.id,
                x: 20.0 + g.x,
                y: 50.0 + g.y,
                text_range: g.text_range.clone(),
            })
            .collect(),
    };
    edit(&mut glyphs);
    item(ItemKind::Glyphs(glyphs))
}

#[test]
fn glyph_run_with_a_non_finite_glyph_position_is_skipped() {
    // WHY: glyph positions are converted to advances by subtraction, so one NaN position poisons the whole run.
    let bad = glyph_run(|run| run.glyphs[2].x = f64::NAN);
    assert_survives("nan-glyph-position", bad, Expect::Skipped);
}

#[test]
fn glyph_run_with_a_non_positive_or_non_finite_size_is_skipped() {
    // WHY: the font size divides positions to form em advances, so a zero size produces infinities.
    for (i, size) in [0.0, -12.0, f64::NAN].into_iter().enumerate() {
        let bad = glyph_run(|run| run.size_pt = size);
        assert_survives(&format!("bad-glyph-size-{i}"), bad, Expect::Skipped);
    }
}

#[test]
fn glyph_run_with_text_ranges_outside_its_text_does_not_panic() {
    // WHY: krilla slices the run's text with each glyph's range, so an out-of-range or reversed range would panic
    // inside the exporter instead of losing only the copyable text of that run.
    let bad = glyph_run(|run| {
        run.glyphs[0].text_range = 3..40;
        #[allow(clippy::reversed_empty_ranges)]
        {
            run.glyphs[1].text_range = 2..1;
        }
    });
    assert_survives("bad-text-range", bad, Expect::SkippedOrCorrected);
}

#[test]
fn glyph_run_with_text_ranges_inside_multibyte_characters_does_not_panic() {
    // WHY: text ranges are byte ranges, and slicing a string at a byte that is not a character boundary panics; a
    // range that is within the text but splits a character must therefore be handled like any other invalid range.
    let bad = glyph_run(|run| {
        run.text = "αβγδ".to_owned();
        for (glyph, range) in run.glyphs.iter_mut().zip([0..1, 1..3, 3..5, 5..8]) {
            glyph.text_range = range;
        }
    });
    assert_survives("mid-character-text-range", bad, Expect::SkippedOrCorrected);
}

#[test]
fn glyph_identifier_beyond_the_font_does_not_panic() {
    // WHY: glyph identifiers index the font's glyph table during subsetting; an identifier the font does not contain
    // must not abort the export.
    let bad = glyph_run(|run| run.glyphs[0].id = u16::MAX);
    assert_survives("glyph-id-out-of-range", bad, Expect::SkippedOrCorrected);
}

#[test]
fn group_with_a_non_finite_transform_is_skipped() {
    // WHY: 3D projection builds transforms from camera parameters, and a degenerate view yields NaN matrices that
    // would otherwise be written into the content stream for every item in the group.
    let transform = Transform {
        a: f64::NAN,
        ..Transform::IDENTITY
    };
    let bad = group(
        None,
        Some(transform),
        vec![filled_rect(Rect::new(10.0, 10.0, 80.0, 80.0), Rgba::BLACK)],
    );
    assert_survives("nan-transform", bad, Expect::Skipped);
}

#[test]
fn group_with_a_singular_transform_does_not_produce_an_invalid_pdf() {
    // WHY: a 3D view seen exactly edge-on projects a plane with a finite but singular matrix; content under it is
    // invisible, but some consumers report a singular text or path matrix as an error, so the group must not make the
    // file fail to render.
    let text = engine();
    let mut items = vec![filled_rect(Rect::new(10.0, 10.0, 80.0, 80.0), Rgba::BLACK)];
    items.extend(label(&text, "Time", false, 12.0, Point::new(20.0, 50.0)));
    let bad = group(
        None,
        Some(Transform {
            a: 0.0,
            b: 0.0,
            c: 0.0,
            d: 0.0,
            e: 50.0,
            f: 50.0,
        }),
        items,
    );
    assert_survives("singular-transform", bad, Expect::SkippedOrCorrected);
}

#[test]
fn group_with_a_non_finite_clip_is_skipped() {
    // WHY: a clip rectangle derived from NaN axis limits cannot be written, and drawing the group unclipped would spill
    // data outside its plot box.
    let clip = Rect::new(10.0, 10.0, f64::NAN, 80.0);
    let bad = group(
        Some(clip),
        None,
        vec![filled_rect(Rect::new(10.0, 10.0, 80.0, 80.0), Rgba::BLACK)],
    );
    assert_survives("nan-clip", bad, Expect::Skipped);
}
