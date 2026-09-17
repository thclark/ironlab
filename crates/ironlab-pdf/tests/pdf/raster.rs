//! Raster checks of paths, strokes, clips, transforms, transparency and fill rules with two independent engines.
//!
//! Pages are rasterised at 72 dpi, so one pixel is one point and pixel coordinates equal display-list coordinates
//! (origin at the top-left, y down). Samples are taken at pixel centres well inside or outside shapes so that
//! anti-aliasing does not affect them.

use ironlab_scene::display::{
    DisplayList, FillRule, ItemKind, LineCap, LineJoin, PathItem, PathSegment, Point, Rect, Rgba,
    Transform,
};

use crate::common::*;
use crate::require_tools;

#[test]
fn filled_rectangle_is_drawn_at_its_position_without_flipping() {
    // WHY: PDF's native y axis points up while the display list's points down; this pins the rectangle to the
    // top-left origin convention, and an unflipped or doubly flipped implementation paints the mirror image.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("rect");
    let text = engine();
    let mut list = page(200.0, 100.0);
    list.items
        .push(filled_rect(Rect::new(20.0, 10.0, 40.0, 20.0), RED));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_eq!(image.dimensions(), (200, 100), "{engine:?}: raster size");
        assert_pixel(engine, &image, 40.5, 20.5, RED_PX, 3, "rectangle centre");
        assert_pixel(
            engine,
            &image,
            22.5,
            12.5,
            RED_PX,
            3,
            "inside the top-left corner",
        );
        assert_pixel(
            engine,
            &image,
            10.5,
            20.5,
            WHITE_PX,
            3,
            "left of the rectangle",
        );
        assert_pixel(
            engine,
            &image,
            70.5,
            20.5,
            WHITE_PX,
            3,
            "right of the rectangle",
        );
        assert_pixel(
            engine,
            &image,
            40.5,
            5.5,
            WHITE_PX,
            3,
            "above the rectangle",
        );
        assert_pixel(
            engine,
            &image,
            40.5,
            80.5,
            WHITE_PX,
            3,
            "where a vertically flipped rectangle would be",
        );
    }
}

#[test]
fn cubic_segments_are_drawn_as_curves() {
    // WHY: markers, arrow heads and glyph-free round shapes are built from cubic segments; dropping or straightening
    // them turns a circle into a polygon or nothing.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("cubic");
    let text = engine();
    // A closed shape whose top edge bulges upwards: a straight chord from (20, 60) to (180, 60) plus a cubic.
    let segments = vec![
        PathSegment::MoveTo(Point::new(20.0, 60.0)),
        PathSegment::CubicTo(
            Point::new(20.0, 0.0),
            Point::new(180.0, 0.0),
            Point::new(180.0, 60.0),
        ),
        PathSegment::Close,
    ];
    let mut list = page(200.0, 80.0);
    list.items
        .push(filled_path(segments, BLUE, FillRule::NonZero));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        // The curve's apex is at y = 15, so (100, 25) is inside the curve but above the straight chord.
        assert_pixel(
            engine,
            &image,
            100.5,
            25.5,
            [0, 0, 255],
            3,
            "inside the bulge of the curve",
        );
        assert_pixel(
            engine,
            &image,
            100.5,
            8.5,
            WHITE_PX,
            3,
            "above the apex of the curve",
        );
        assert_pixel(
            engine,
            &image,
            25.5,
            20.5,
            WHITE_PX,
            3,
            "outside the curve near its start",
        );
    }
}

#[test]
fn dashed_stroke_leaves_gaps_along_the_line() {
    // WHY: dashed lines distinguish series in monochrome print; a backend that ignores the dash array draws a solid
    // line that makes the legend lie.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("dash");
    let text = engine();
    let mut stroke = solid_stroke(Rgba::BLACK, 4.0);
    stroke.dash = vec![10.0, 10.0];
    let mut list = page(200.0, 100.0);
    list.items.push(stroked_line(
        Point::new(10.0, 50.0),
        Point::new(190.0, 50.0),
        stroke,
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        // Dashes cover x in [10, 20), [30, 40), …; gaps cover [20, 30), [40, 50), ….
        for k in 0..8 {
            let dash_centre = 15.5 + 20.0 * f64::from(k);
            assert_pixel(engine, &image, dash_centre, 50.5, BLACK_PX, 3, "dash");
            assert_pixel(engine, &image, dash_centre + 10.0, 50.5, WHITE_PX, 3, "gap");
        }
        assert_pixel(
            engine,
            &image,
            100.5,
            45.5,
            WHITE_PX,
            3,
            "beyond the half width of the stroke",
        );
    }
}

#[test]
fn dash_offset_shifts_the_pattern_along_the_line() {
    // WHY: the offset is how the viewer and the PDF start a dash pattern at the same phase; ignoring it or applying it
    // with the opposite sign makes on-screen and exported dashes disagree.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("dash-offset");
    let text = engine();
    let mut stroke = solid_stroke(Rgba::BLACK, 4.0);
    stroke.dash = vec![10.0, 10.0];
    stroke.dash_offset = 5.0;
    let mut list = page(200.0, 100.0);
    list.items.push(stroked_line(
        Point::new(10.0, 50.0),
        Point::new(190.0, 50.0),
        stroke,
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        // As in PDF and SVG, the pattern starts 5 pt into its first dash: dash [10, 15), gap [15, 25), dash [25, 35),
        // gap [35, 45). Without the offset the pattern would be dash [10, 20), gap [20, 30), dash [30, 40), and with
        // the offset's sign reversed it would be gap [10, 15), dash [15, 25), gap [25, 35); every sample below
        // differs from at least one of those.
        assert_pixel(
            engine,
            &image,
            12.5,
            50.5,
            BLACK_PX,
            3,
            "shortened first dash",
        );
        assert_pixel(engine, &image, 17.5, 50.5, WHITE_PX, 3, "first gap");
        assert_pixel(engine, &image, 27.5, 50.5, BLACK_PX, 3, "second dash");
        assert_pixel(engine, &image, 37.5, 50.5, WHITE_PX, 3, "second gap");
    }
}

#[test]
fn line_caps_extend_the_stroke_as_specified() {
    // WHY: square and round caps extend a stroke by half its width beyond its end points while butt caps do not;
    // error bars and tick marks depend on the difference, and the round cap's shape distinguishes it from a square cap.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("caps");
    let text = engine();
    let mut list = page(100.0, 150.0);
    let mut butt = solid_stroke(Rgba::BLACK, 10.0);
    butt.cap = LineCap::Butt;
    let mut square = butt.clone();
    square.cap = LineCap::Square;
    list.items.push(stroked_line(
        Point::new(30.0, 25.0),
        Point::new(70.0, 25.0),
        butt,
    ));
    list.items.push(stroked_line(
        Point::new(30.0, 75.0),
        Point::new(70.0, 75.0),
        square,
    ));
    let mut round = solid_stroke(Rgba::BLACK, 20.0);
    round.cap = LineCap::Round;
    list.items.push(stroked_line(
        Point::new(40.0, 125.0),
        Point::new(70.0, 125.0),
        round,
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            27.5,
            25.5,
            WHITE_PX,
            3,
            "beyond the end of a butt-capped stroke",
        );
        assert_pixel(
            engine,
            &image,
            27.5,
            75.5,
            BLACK_PX,
            3,
            "within the extension of a square-capped stroke",
        );
        // The round cap is a half disc of radius 10 centred on (40, 125).
        assert_pixel(
            engine,
            &image,
            32.5,
            125.5,
            BLACK_PX,
            3,
            "within the round cap on the line's axis",
        );
        assert_pixel(
            engine,
            &image,
            31.5,
            116.5,
            WHITE_PX,
            3,
            "outside the round cap but inside where a square cap would reach",
        );
    }
}

#[test]
fn miter_joins_are_limited_at_a_ratio_of_four() {
    // WHY: the display list fixes the miter limit at 4 so that the viewer and the PDF agree on which sharp corners of
    // a data line are bevelled; PDF's default limit (10) would draw long spikes at sharp peaks that the screen does not
    // show.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("miter-limit");
    let text = engine();
    let mut list = page(200.0, 130.0);
    let mut stroke = solid_stroke(Rgba::BLACK, 10.0);
    stroke.join = LineJoin::Miter;
    let polyline = |points: [Point; 3]| {
        item(ItemKind::Path(PathItem {
            segments: vec![
                PathSegment::MoveTo(points[0]),
                PathSegment::LineTo(points[1]),
                PathSegment::LineTo(points[2]),
            ],
            fill: None,
            stroke: Some(stroke.clone()),
        }))
    };
    // A 20° corner at (100, 50) opening downwards: its miter ratio 1/sin(10°) ≈ 5.8 exceeds 4, so it is bevelled. With
    // a limit of 10 its miter would reach up to y ≈ 21.
    let (sin, cos) = 10.0_f64.to_radians().sin_cos();
    list.items.push(polyline([
        Point::new(100.0 - 70.0 * sin, 50.0 + 70.0 * cos),
        Point::new(100.0, 50.0),
        Point::new(100.0 + 70.0 * sin, 50.0 + 70.0 * cos),
    ]));
    // A right-angled corner at (150, 50): its miter ratio √2 is below 4, so it keeps its miter, reaching up to y ≈ 43.
    list.items.push(polyline([
        Point::new(110.0, 90.0),
        Point::new(150.0, 50.0),
        Point::new(190.0, 90.0),
    ]));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            100.5,
            42.5,
            WHITE_PX,
            3,
            "where the miter of the sharp corner would be drawn without the limit",
        );
        assert_pixel(
            engine,
            &image,
            100.5,
            55.5,
            BLACK_PX,
            3,
            "inside the sharp corner's join",
        );
        assert_pixel(
            engine,
            &image,
            150.5,
            44.5,
            BLACK_PX,
            3,
            "the miter tip of the right-angled corner, which a bevel or round join would not reach",
        );
    }
}

#[test]
fn stroke_width_and_dashes_scale_with_the_group_transform() {
    // WHY: stroke widths and dash lengths are in the item's local space, as in PDF; an exporter that converts them to
    // figure space separately (or a viewer that does not) makes lines inside scaled groups differ in weight and rhythm
    // between the screen and the PDF.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("stroke-scale");
    let text = engine();
    let scale = Transform {
        a: 4.0,
        d: 4.0,
        ..Transform::IDENTITY
    };
    let mut dashed = solid_stroke(Rgba::BLACK, 1.0);
    dashed.dash = vec![5.0, 5.0];
    let mut list = page(200.0, 100.0);
    list.items.push(group(
        None,
        Some(scale),
        vec![
            // Figure space: from (20, 50) to (180, 50), 12 pt wide, covering y in [44, 56].
            stroked_line(
                Point::new(5.0, 12.5),
                Point::new(45.0, 12.5),
                solid_stroke(Rgba::BLACK, 3.0),
            ),
            // Figure space: from (20, 80) to (180, 80), 4 pt wide, dashes [20, 40), [60, 80), … and gaps between.
            stroked_line(Point::new(5.0, 20.0), Point::new(45.0, 20.0), dashed),
        ],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        for (y, expected, what) in [
            (42.5, WHITE_PX, "above the scaled solid stroke"),
            (45.5, BLACK_PX, "inside the top of the scaled solid stroke"),
            (
                54.5,
                BLACK_PX,
                "inside the bottom of the scaled solid stroke",
            ),
            (57.5, WHITE_PX, "below the scaled solid stroke"),
        ] {
            assert_pixel(engine, &image, 100.5, y, expected, 3, what);
        }
        assert_pixel(engine, &image, 30.5, 80.5, BLACK_PX, 3, "first scaled dash");
        assert_pixel(
            engine,
            &image,
            50.5,
            80.5,
            WHITE_PX,
            3,
            "first scaled gap, where an unscaled pattern would draw a dash",
        );
        assert_pixel(
            engine,
            &image,
            70.5,
            80.5,
            BLACK_PX,
            3,
            "second scaled dash",
        );
        assert_pixel(
            engine,
            &image,
            30.5,
            76.5,
            WHITE_PX,
            3,
            "above the scaled 4 pt dashed stroke",
        );
    }
}

#[test]
fn clipped_group_hides_content_outside_the_clip() {
    // WHY: axes clip their artists to the plot box; without clipping, data outside the limits draws over tick labels.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("clip");
    let text = engine();
    let mut list = page(200.0, 100.0);
    list.items.push(group(
        Some(Rect::new(0.0, 0.0, 100.0, 100.0)),
        None,
        vec![filled_rect(Rect::new(50.0, 20.0, 100.0, 40.0), RED)],
    ));
    // Items after the group must not be clipped: the clip is popped when the group ends.
    list.items
        .push(filled_rect(Rect::new(160.0, 70.0, 20.0, 20.0), BLUE));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(engine, &image, 75.5, 40.5, RED_PX, 3, "inside the clip");
        assert_pixel(engine, &image, 125.5, 40.5, WHITE_PX, 3, "outside the clip");
        assert_pixel(
            engine,
            &image,
            170.5,
            80.5,
            [0, 0, 255],
            3,
            "item after the clipped group",
        );
    }
}

#[test]
fn group_clip_is_expressed_in_the_parent_space() {
    // WHY: the display list defines a group's clip in its parent's coordinates, applied before its transform; applying
    // the transform to the clip as well moves the plot-box clip away from the plot box.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("clip-space");
    let text = engine();
    let mut list = page(200.0, 100.0);
    list.items.push(group(
        Some(Rect::new(0.0, 0.0, 100.0, 100.0)),
        Some(Transform::translate(50.0, 0.0)),
        vec![filled_rect(Rect::new(0.0, 20.0, 100.0, 40.0), RED)],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            25.5,
            40.5,
            WHITE_PX,
            3,
            "left of the translated rectangle",
        );
        assert_pixel(
            engine,
            &image,
            75.5,
            40.5,
            RED_PX,
            3,
            "translated rectangle inside the parent-space clip",
        );
        assert_pixel(
            engine,
            &image,
            125.5,
            40.5,
            WHITE_PX,
            3,
            "translated rectangle outside the parent-space clip",
        );
    }
}

#[test]
fn nested_clips_intersect() {
    // WHY: a legend or inset inside a clipped axes must be limited by both clips, not only the innermost one.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("nested-clip");
    let text = engine();
    let mut list = page(200.0, 100.0);
    list.items.push(group(
        Some(Rect::new(0.0, 0.0, 100.0, 100.0)),
        None,
        vec![group(
            Some(Rect::new(50.0, 0.0, 150.0, 100.0)),
            None,
            vec![filled_rect(Rect::new(0.0, 20.0, 200.0, 40.0), RED)],
        )],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            25.5,
            40.5,
            WHITE_PX,
            3,
            "outside the inner clip",
        );
        assert_pixel(engine, &image, 75.5, 40.5, RED_PX, 3, "inside both clips");
        assert_pixel(
            engine,
            &image,
            150.5,
            40.5,
            WHITE_PX,
            3,
            "outside the outer clip",
        );
    }
}

#[test]
fn group_rotated_by_ninety_degrees_turns_a_horizontal_bar_vertical() {
    // WHY: transforms are how rotated labels and 3D-projected content are placed; this pins both the composition order
    // (rotate, then translate) and the documented direction (positive angles turn clockwise on screen).
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("rotate");
    let text = engine();
    let mut list = page(200.0, 200.0);
    // A bar along local +x from 0 to 60, 4 pt thick; rotating clockwise by 90° points it down the page.
    list.items.push(group(
        None,
        Some(Transform::rotate(90.0).then(Transform::translate(100.0, 100.0))),
        vec![filled_rect(Rect::new(0.0, -2.0, 60.0, 4.0), Rgba::BLACK)],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            100.5,
            110.5,
            BLACK_PX,
            3,
            "near the start of the vertical bar",
        );
        assert_pixel(
            engine,
            &image,
            100.5,
            150.5,
            BLACK_PX,
            3,
            "near the end of the vertical bar",
        );
        assert_pixel(
            engine,
            &image,
            100.5,
            70.5,
            WHITE_PX,
            3,
            "where a counter-clockwise rotation would draw",
        );
        assert_pixel(
            engine,
            &image,
            130.5,
            100.5,
            WHITE_PX,
            3,
            "where an unrotated bar would draw",
        );
        assert_pixel(
            engine,
            &image,
            70.5,
            100.5,
            WHITE_PX,
            3,
            "where a bar rotated by 180° would draw",
        );
    }
}

#[test]
fn half_transparent_fill_blends_with_the_background() {
    // WHY: alpha is used for overlapping filled contours and confidence bands; an exporter that drops it hides the
    // data underneath.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("alpha-fill");
    let text = engine();
    let mut list = page(100.0, 100.0);
    list.items.push(filled_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        RED.with_alpha_factor(0.5),
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            50.5,
            50.5,
            [255, 128, 128],
            12,
            "half-transparent red over white",
        );
    }
}

#[test]
fn half_transparent_stroke_blends_with_what_is_underneath() {
    // WHY: stroke opacity is separate from fill opacity in PDF; setting only one of them leaves translucent lines
    // opaque.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("alpha-stroke");
    let text = engine();
    let mut list = page(100.0, 100.0);
    list.items
        .push(filled_rect(Rect::new(0.0, 40.0, 100.0, 20.0), BLUE));
    list.items.push(stroked_line(
        Point::new(50.0, 10.0),
        Point::new(50.0, 90.0),
        solid_stroke(Rgba::BLACK.with_alpha_factor(0.5), 10.0),
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            50.5,
            20.5,
            [128, 128, 128],
            12,
            "half-transparent black over white",
        );
        assert_pixel(
            engine,
            &image,
            50.5,
            50.5,
            [0, 0, 128],
            12,
            "half-transparent black over blue",
        );
    }
}

#[test]
fn even_odd_and_nonzero_rules_differ_for_nested_same_direction_contours() {
    // WHY: filled contour bands rely on nonzero winding while holes in shapes rely on even-odd; a backend that ignores
    // the rule fills or empties the centre of one of them.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("fill-rule");
    let text = engine();
    let nested = |dx: f64| {
        let mut segments = rect_segments(Rect::new(20.0 + dx, 20.0, 60.0, 60.0));
        segments.extend(rect_segments(Rect::new(35.0 + dx, 35.0, 30.0, 30.0)));
        segments
    };
    let mut list = page(200.0, 100.0);
    list.items
        .push(filled_path(nested(0.0), RED, FillRule::NonZero));
    list.items
        .push(filled_path(nested(100.0), RED, FillRule::EvenOdd));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(engine, &image, 25.5, 50.5, RED_PX, 3, "nonzero ring");
        assert_pixel(
            engine,
            &image,
            50.5,
            50.5,
            RED_PX,
            3,
            "nonzero centre (winding number 2)",
        );
        assert_pixel(engine, &image, 125.5, 50.5, RED_PX, 3, "even-odd ring");
        assert_pixel(
            engine,
            &image,
            150.5,
            50.5,
            WHITE_PX,
            3,
            "even-odd centre (hole)",
        );
    }
}

#[test]
fn path_with_both_fill_and_stroke_draws_the_stroke_over_the_fill() {
    // WHY: markers with distinct face and edge colours are a single path; if the fill is painted after the stroke it
    // covers the inner half of the edge.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("fill-stroke");
    let text = engine();
    let mut list = page(100.0, 100.0);
    list.items.push(item(ItemKind::Path(PathItem {
        segments: rect_segments(Rect::new(20.0, 20.0, 60.0, 60.0)),
        fill: Some(ironlab_scene::display::Fill {
            color: RED,
            rule: FillRule::NonZero,
        }),
        stroke: Some(solid_stroke(BLUE, 10.0)),
    })));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(engine, &image, 50.5, 50.5, RED_PX, 3, "fill");
        assert_pixel(
            engine,
            &image,
            22.5,
            50.5,
            [0, 0, 255],
            3,
            "inner half of the edge",
        );
        assert_pixel(
            engine,
            &image,
            17.5,
            50.5,
            [0, 0, 255],
            3,
            "outer half of the edge",
        );
    }
}

#[test]
fn later_items_paint_over_earlier_ones() {
    // WHY: the display list is ordered back to front (grid, then data, then legend), and reordering hides data.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("order");
    let text = engine();
    let mut list = page(100.0, 100.0);
    list.items
        .push(filled_rect(Rect::new(10.0, 10.0, 60.0, 60.0), RED));
    list.items
        .push(filled_rect(Rect::new(30.0, 30.0, 60.0, 60.0), BLUE));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(engine, &image, 20.5, 20.5, RED_PX, 3, "first item only");
        assert_pixel(
            engine,
            &image,
            50.5,
            50.5,
            [0, 0, 255],
            3,
            "overlap takes the later item",
        );
    }
}

/// A page mixing fills, a dashed stroke, a clipped and translucent group, a curve, rotated text and mathematics, whose
/// background is off-white so that a white page would be detected.
fn mixed_content_page(text: &ironlab_text::TextEngine) -> DisplayList {
    let mut list = page(300.0, 200.0);
    list.background = Rgba::from_u8([250, 248, 240]);
    list.items
        .push(filled_rect(Rect::new(20.0, 20.0, 80.0, 50.0), RED));
    let mut dashed = solid_stroke(Rgba::from_u8([30, 120, 60]), 2.0);
    dashed.dash = vec![6.0, 3.0];
    list.items.push(stroked_line(
        Point::new(20.0, 90.0),
        Point::new(280.0, 90.0),
        dashed,
    ));
    list.items.push(group(
        Some(Rect::new(150.0, 10.0, 120.0, 70.0)),
        Some(Transform::translate(150.0, 10.0)),
        vec![
            filled_rect(
                Rect::new(-20.0, -20.0, 80.0, 80.0),
                BLUE.with_alpha_factor(0.6),
            ),
            item(ItemKind::Path(PathItem {
                segments: vec![
                    PathSegment::MoveTo(Point::new(0.0, 60.0)),
                    PathSegment::CubicTo(
                        Point::new(40.0, -20.0),
                        Point::new(80.0, 100.0),
                        Point::new(120.0, 20.0),
                    ),
                ],
                fill: None,
                stroke: Some(solid_stroke(Rgba::BLACK, 1.5)),
            })),
        ],
    ));
    list.items.push(group(
        None,
        Some(Transform::rotate(-90.0).then(Transform::translate(30.0, 185.0))),
        label(text, "Amplitude", false, 11.0, Point::new(0.0, 0.0)),
    ));
    list.items.extend(label(
        text,
        "Time (s)",
        false,
        11.0,
        Point::new(120.0, 140.0),
    ));
    list.items.extend(label(
        text,
        "$\\frac{\\alpha}{2}$",
        true,
        14.0,
        Point::new(220.0, 170.0),
    ));
    list
}

#[test]
fn poppler_and_ghostscript_rasters_agree() {
    // WHY: a subtly invalid PDF (bad operator nesting, unbalanced graphics state, broken font program) is often
    // repaired silently by one viewer and not the other, so disagreement between independent engines exposes it.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("engines-agree");
    let text = engine();
    let rasters = render_and_rasterise(&ws, &mixed_content_page(&text), &text);
    let (_, poppler) = &rasters[0];
    let (_, ghostscript) = &rasters[1];
    assert_engines_agree(poppler, ghostscript);
}

#[test]
fn engine_comparison_detects_a_small_local_defect() {
    // WHY: the engine-agreement tests are only meaningful if the comparison tolerates the sub-pixel differences between
    // correct rasters yet still rejects a real defect; a page-wide mean alone dilutes a small one below its tolerance.
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("engines-defect");
    let text = engine();
    let rasters = render_and_rasterise(&ws, &mixed_content_page(&text), &text);
    let (_, poppler) = &rasters[0];
    let (_, ghostscript) = &rasters[1];
    assert!(
        raster_difference(poppler, ghostscript).agrees(),
        "clean rasters of the same page should agree"
    );

    // A 10 px red square painted over an empty part of the background, where neither engine draws anything.
    let square = Rect::new(240.0, 110.0, 10.0, 10.0);
    assert_eq!(
        count_pixels(poppler, square, |px| !close_to(px, [250, 248, 240], 3)),
        0,
        "the defect must be painted over bare background"
    );
    let mut defective = poppler.clone();
    for y in 110..120 {
        for x in 240..250 {
            defective.put_pixel(x, y, image::Rgb(RED_PX));
        }
    }
    let difference = raster_difference(&defective, ghostscript);
    assert!(
        !difference.agrees(),
        "a 10 px red square should make the rasters disagree, but the comparison measured {difference:?}"
    );
}
