//! Tessellation of display lists into egui meshes.
//!
//! The tests measure geometry (triangle area and bounding boxes) rather than comparing vertex lists, so that they hold
//! for any correct triangulation and fail only when the drawn shape is wrong.

mod common;

use common::{TEXT, assert_close};
use egui::{Color32, Mesh, Pos2};
use ironlab_scene::display::{
    DisplayList, Fill, FillRule, GlyphsItem, Item, ItemKind, LineCap, LineJoin, PathItem,
    PathSegment, PlacedGlyph, Point, Rect, Rgba, Stroke, Transform,
};
use ironlab_text::TextItem;
use ironlab_viewer::{ScreenTransform, tessellate};

const SCALE: f32 = 2.0;

fn to_screen() -> ScreenTransform {
    ScreenTransform {
        scale: SCALE,
        origin: Pos2::new(10.0, 20.0),
    }
}

fn list(items: Vec<Item>) -> DisplayList {
    DisplayList {
        width_pt: 400.0,
        height_pt: 300.0,
        background: Rgba::WHITE,
        items,
    }
}

fn rect_segments(x: f64, y: f64, w: f64, h: f64) -> Vec<PathSegment> {
    vec![
        PathSegment::MoveTo(Point::new(x, y)),
        PathSegment::LineTo(Point::new(x + w, y)),
        PathSegment::LineTo(Point::new(x + w, y + h)),
        PathSegment::LineTo(Point::new(x, y + h)),
        PathSegment::Close,
    ]
}

fn filled(segments: Vec<PathSegment>, color: Rgba, rule: FillRule) -> Item {
    Item {
        source: None,
        kind: ItemKind::Path(PathItem {
            segments,
            fill: Some(Fill { color, rule }),
            stroke: None,
        }),
    }
}

fn group(clip: Option<Rect>, transform: Option<Transform>, items: Vec<Item>) -> Item {
    Item {
        source: None,
        kind: ItemKind::Group {
            clip,
            transform,
            items,
        },
    }
}

fn triangles(meshes: &[Mesh]) -> impl Iterator<Item = [egui::epaint::Vertex; 3]> + '_ {
    meshes.iter().flat_map(|mesh| {
        mesh.indices.as_chunks::<3>().0.iter().map(|t| {
            [
                mesh.vertices[t[0] as usize],
                mesh.vertices[t[1] as usize],
                mesh.vertices[t[2] as usize],
            ]
        })
    })
}

/// Total unsigned triangle area in screen units squared.
fn area(meshes: &[Mesh]) -> f64 {
    triangles(meshes)
        .map(|[a, b, c]| {
            let (ax, ay) = (f64::from(a.pos.x), f64::from(a.pos.y));
            let (bx, by) = (f64::from(b.pos.x), f64::from(b.pos.y));
            let (cx, cy) = (f64::from(c.pos.x), f64::from(c.pos.y));
            ((bx - ax) * (cy - ay) - (cx - ax) * (by - ay)).abs() / 2.0
        })
        .sum()
}

/// Bounding box of every vertex referenced by a triangle, in screen units.
fn bbox(meshes: &[Mesh]) -> egui::Rect {
    let mut rect = egui::Rect::NOTHING;
    for tri in triangles(meshes) {
        for v in tri {
            rect.extend_with(v.pos);
        }
    }
    rect
}

fn assert_rect_close(actual: egui::Rect, expected: egui::Rect, tolerance: f32) {
    let ok = (actual.min.x - expected.min.x).abs() <= tolerance
        && (actual.min.y - expected.min.y).abs() <= tolerance
        && (actual.max.x - expected.max.x).abs() <= tolerance
        && (actual.max.y - expected.max.y).abs() <= tolerance;
    assert!(ok, "expected bbox {expected:?}, got {actual:?}");
}

fn scaled_area(figure_area: f64) -> f64 {
    figure_area * f64::from(SCALE) * f64::from(SCALE)
}

/// Whether any triangle covers the screen position of the figure-space point `p`.
fn ink_at(meshes: &[Mesh], p: Point) -> bool {
    let q = to_screen().apply(p);
    let (px, py) = (f64::from(q.x), f64::from(q.y));
    triangles(meshes).any(|[a, b, c]| {
        let edge = |u: Pos2, v: Pos2| {
            (f64::from(v.x) - f64::from(u.x)) * (py - f64::from(u.y))
                - (f64::from(v.y) - f64::from(u.y)) * (px - f64::from(u.x))
        };
        let (d0, d1, d2) = (edge(a.pos, b.pos), edge(b.pos, c.pos), edge(c.pos, a.pos));
        (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0) || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
    })
}

// Why: the screen transform is the only thing mapping figure points to screen units; a wrong scale or origin would
// misplace every primitive and break hit-testing, which uses the inverse of the same transform.
#[test]
fn a_filled_rectangle_covers_its_scaled_area_at_its_transformed_position() {
    let meshes = tessellate(
        &list(vec![filled(
            rect_segments(30.0, 40.0, 50.0, 20.0),
            Rgba::BLACK,
            FillRule::NonZero,
        )]),
        &TEXT,
        to_screen(),
    );

    assert_close(area(&meshes), scaled_area(1000.0), 1e-3, "area");
    assert_rect_close(
        bbox(&meshes),
        egui::Rect::from_min_max(Pos2::new(70.0, 100.0), Pos2::new(170.0, 140.0)),
        1e-3,
    );
}

// Why: holes in filled regions (contour bands around a peak, glyph counters) depend on the fill rule; with even-odd a
// nested contour is a hole, with non-zero and the same winding it is not.
#[test]
fn the_fill_rule_decides_whether_a_nested_contour_is_a_hole() {
    let mut segments = rect_segments(0.0, 0.0, 100.0, 100.0);
    segments.extend(rect_segments(25.0, 25.0, 50.0, 50.0));

    let even_odd = tessellate(
        &list(vec![filled(
            segments.clone(),
            Rgba::BLACK,
            FillRule::EvenOdd,
        )]),
        &TEXT,
        to_screen(),
    );
    let non_zero = tessellate(
        &list(vec![filled(segments, Rgba::BLACK, FillRule::NonZero)]),
        &TEXT,
        to_screen(),
    );

    assert_close(
        area(&even_odd),
        scaled_area(7500.0),
        1e-2,
        "even-odd leaves a hole",
    );
    assert_close(
        area(&non_zero),
        scaled_area(10_000.0),
        1e-2,
        "non-zero with equal winding fills the hole",
    );
}

fn horizontal_line(dash: Vec<f64>) -> Item {
    stroked_line(dash, 0.0, LineCap::Butt)
}

fn stroked_line(dash: Vec<f64>, dash_offset: f64, cap: LineCap) -> Item {
    Item {
        source: None,
        kind: ItemKind::Path(PathItem {
            segments: vec![
                PathSegment::MoveTo(Point::new(0.0, 50.0)),
                PathSegment::LineTo(Point::new(100.0, 50.0)),
            ],
            fill: None,
            stroke: Some(Stroke {
                color: Rgba::BLACK,
                width: 4.0,
                dash,
                dash_offset,
                cap,
                join: LineJoin::Miter,
            }),
        }),
    }
}

// Why: lyon cannot dash, so the canvas splits dashed strokes itself; the drawn ink must be the dash duty fraction of a
// solid line of the same width, which also checks that the stroke width is scaled to the screen.
#[test]
fn a_dashed_stroke_covers_the_duty_fraction_of_a_solid_stroke() {
    let solid = tessellate(&list(vec![horizontal_line(vec![])]), &TEXT, to_screen());
    let dashed = tessellate(
        &list(vec![horizontal_line(vec![6.0, 4.0])]),
        &TEXT,
        to_screen(),
    );

    assert_close(
        area(&solid),
        scaled_area(100.0 * 4.0),
        scaled_area(1.0),
        "solid stroke area is length × width",
    );
    assert_close(
        area(&dashed),
        0.6 * area(&solid),
        scaled_area(4.0),
        "dashes cover 6 of every 10 points",
    );
    assert!(dashed.iter().map(|m| m.indices.len()).sum::<usize>() > 0);
}

// Why: the duty fraction alone does not show that dash lengths are in figure points (a pattern left in screen units
// has the same duty fraction) or that the dash phase is honoured; the PDF writes both natively, so the canvas must
// place its dashes where the PDF does. The line runs along y = 50 from x = 0 to x = 100 with the pattern 6 on, 4 off.
#[test]
fn dash_lengths_and_offset_are_in_figure_points() {
    let y = 50.0;
    let plain = tessellate(
        &list(vec![stroked_line(vec![6.0, 4.0], 0.0, LineCap::Butt)]),
        &TEXT,
        to_screen(),
    );
    for (x, inked) in [
        (1.0, true),
        (4.0, true),
        (8.0, false),
        (13.0, true),
        (18.0, false),
        (95.0, true),
    ] {
        assert_eq!(
            ink_at(&plain, Point::new(x, y)),
            inked,
            "without offset, ink at x = {x} is {inked}"
        );
    }

    // An offset of 6 starts the stroke at the beginning of the gap: off on [0, 4], on on [4, 10], off on [10, 14].
    let offset = tessellate(
        &list(vec![stroked_line(vec![6.0, 4.0], 6.0, LineCap::Butt)]),
        &TEXT,
        to_screen(),
    );
    for (x, inked) in [(2.0, false), (7.0, true), (12.0, false), (16.0, true)] {
        assert_eq!(
            ink_at(&offset, Point::new(x, y)),
            inked,
            "with offset 6, ink at x = {x} is {inked}"
        );
    }
}

// Why: caps change the drawn length of every stroke (tick marks, error bars, marker outlines); the canvas must map
// them as the PDF does, where a square cap extends each end by half the width and a round cap adds a half disc.
#[test]
fn line_caps_extend_the_stroke_as_in_pdf() {
    let area_of = |cap| {
        area(&tessellate(
            &list(vec![stroked_line(vec![], 0.0, cap)]),
            &TEXT,
            to_screen(),
        ))
    };

    assert_close(
        area_of(LineCap::Butt),
        scaled_area(100.0 * 4.0),
        scaled_area(0.1),
        "butt caps end at the end points",
    );
    assert_close(
        area_of(LineCap::Square),
        scaled_area(104.0 * 4.0),
        scaled_area(0.1),
        "square caps add half the width at each end",
    );
    assert_close(
        area_of(LineCap::Round),
        scaled_area(100.0 * 4.0 + std::f64::consts::PI * 4.0),
        scaled_area(1.0),
        "round caps add a disc of the stroke width in total",
    );
}

// Why: stroke widths are in the item's local space, so a scaling group transform thickens the line exactly as the PDF
// does; transforming only the geometry would draw hairlines where the PDF draws thick strokes.
#[test]
fn a_group_scale_scales_the_stroke_width() {
    let scale_two = Transform {
        a: 2.0,
        b: 0.0,
        c: 0.0,
        d: 2.0,
        e: 0.0,
        f: 0.0,
    };
    let meshes = tessellate(
        &list(vec![group(
            None,
            Some(scale_two),
            vec![horizontal_line(vec![])],
        )]),
        &TEXT,
        to_screen(),
    );

    // The 100 × 4 local stroke becomes 200 × 8 in figure space.
    assert_close(area(&meshes), scaled_area(1600.0), scaled_area(1.0), "area");
    assert_rect_close(
        bbox(&meshes),
        egui::Rect::from_min_max(
            to_screen().apply(Point::new(0.0, 96.0)),
            to_screen().apply(Point::new(200.0, 104.0)),
        ),
        1e-3,
    );
}

// Why: data outside the axes limits must not paint over tick labels and neighbouring subplots; the clip must remove
// exactly the part outside the clip rectangle.
#[test]
fn a_clip_removes_the_geometry_outside_the_clip_rectangle() {
    let meshes = tessellate(
        &list(vec![group(
            Some(Rect::new(50.0, 0.0, 100.0, 100.0)),
            None,
            vec![filled(
                rect_segments(0.0, 0.0, 100.0, 100.0),
                Rgba::BLACK,
                FillRule::NonZero,
            )],
        )]),
        &TEXT,
        to_screen(),
    );

    assert_close(
        area(&meshes),
        scaled_area(50.0 * 100.0),
        0.5,
        "half the rectangle remains",
    );
    let outside = tessellate(
        &list(vec![group(
            Some(Rect::new(200.0, 0.0, 50.0, 50.0)),
            None,
            vec![filled(
                rect_segments(0.0, 0.0, 100.0, 100.0),
                Rgba::BLACK,
                FillRule::NonZero,
            )],
        )]),
        &TEXT,
        to_screen(),
    );
    assert_eq!(
        area(&outside),
        0.0,
        "geometry entirely outside its clip draws nothing"
    );
    assert_rect_close(
        bbox(&meshes),
        egui::Rect::from_min_max(
            Pos2::new(10.0 + 100.0, 20.0),
            Pos2::new(10.0 + 200.0, 20.0 + 200.0),
        ),
        1e-3,
    );
}

// Why: rotated y-axis labels are drawn as groups with a rotation; if the group transform were ignored or applied in
// the wrong order, labels would appear horizontal or in the wrong place.
#[test]
fn a_group_rotation_rotates_its_items_before_translating_them() {
    let meshes = tessellate(
        &list(vec![group(
            None,
            Some(Transform::rotate(90.0).then(Transform::translate(100.0, 100.0))),
            vec![filled(
                rect_segments(0.0, 0.0, 40.0, 10.0),
                Rgba::BLACK,
                FillRule::NonZero,
            )],
        )]),
        &TEXT,
        to_screen(),
    );

    // The rotation maps (x, y) to (−y, x), so the 40 × 10 rectangle becomes x ∈ [−10, 0], y ∈ [0, 40] before the
    // translation to (100, 100).
    assert_rect_close(
        bbox(&meshes),
        egui::Rect::from_min_max(
            Pos2::new(10.0 + 180.0, 20.0 + 200.0),
            Pos2::new(10.0 + 200.0, 20.0 + 280.0),
        ),
        1e-3,
    );
    assert_close(
        area(&meshes),
        scaled_area(400.0),
        0.1,
        "rotation preserves area",
    );
}

// Why: text is drawn from font outlines; an outline that is not scaled by the run size, not placed at the glyph
// origin, or left in the font's y-up space would render labels at the wrong size, place or upside down.
#[test]
fn a_glyph_is_drawn_at_its_size_above_its_baseline_origin() {
    let size = 20.0;
    let layout = TEXT.layout("H", false, size);
    let run = layout
        .items
        .iter()
        .find_map(|item| match item {
            TextItem::Glyphs(run) => Some(run),
            TextItem::Rule { .. } => None,
        })
        .expect("\"H\" lays out as a glyph run");
    let origin = Point::new(10.0, 50.0);
    let item = Item {
        source: None,
        kind: ItemKind::Glyphs(GlyphsItem {
            font: run.font,
            size_pt: run.size_pt,
            color: Rgba::new(0.0, 0.0, 1.0, 1.0),
            text: "H".to_owned(),
            glyphs: vec![PlacedGlyph {
                id: run.glyphs[0].id,
                x: origin.x,
                y: origin.y,
                text_range: 0..1,
            }],
        }),
    };

    let meshes = tessellate(&list(vec![item]), &TEXT, to_screen());
    assert!(area(&meshes) > 0.0, "the glyph produces ink");
    assert!(
        triangles(&meshes)
            .flatten()
            .all(|v| v.color == Color32::BLUE),
        "the glyph is drawn in the run colour"
    );

    let screen = bbox(&meshes);
    let min = to_screen().invert(screen.min);
    let max = to_screen().invert(screen.max);
    assert!(
        min.x >= origin.x - 0.5 && max.x <= origin.x + size,
        "ink starts at the pen position: {min:?}–{max:?}"
    );
    assert!(
        max.y <= origin.y + 0.5,
        "a capital H sits on the baseline, not below it: {min:?}–{max:?}"
    );
    assert!(
        min.y >= origin.y - size,
        "the ink is no taller than one em: {min:?}–{max:?}"
    );
    assert!(
        max.y - min.y >= 0.5 * size,
        "the cap height is a substantial part of the em: {min:?}–{max:?}"
    );
    assert!(
        max.x - min.x >= 0.4 * size,
        "the glyph is scaled to the run size: {min:?}–{max:?}"
    );
}

// Why: egui blends premultiplied colours; passing straight alpha would draw translucent fills (legend boxes, alpha
// surfaces) too bright.
#[test]
fn colours_are_premultiplied() {
    let meshes = tessellate(
        &list(vec![filled(
            rect_segments(0.0, 0.0, 10.0, 10.0),
            Rgba::new(1.0, 0.0, 0.0, 0.5),
            FillRule::NonZero,
        )]),
        &TEXT,
        to_screen(),
    );

    let expected = Color32::from_rgba_unmultiplied(255, 0, 0, 128);
    let vertices: Vec<_> = triangles(&meshes).flatten().collect();
    assert!(!vertices.is_empty());
    for v in vertices {
        let [r, g, b, a] = v.color.to_array();
        assert!(
            r.abs_diff(expected.r()) <= 1 && g == 0 && b == 0 && a.abs_diff(expected.a()) <= 1,
            "expected premultiplied {expected:?}, got {:?}",
            v.color
        );
    }
}

// Why: the display list is ordered back to front (for example 3D faces after depth sorting); the meshes must preserve
// that order or the canvas would disagree with the PDF about what is in front.
#[test]
fn meshes_preserve_paint_order() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    let meshes = tessellate(
        &list(vec![
            filled(rect_segments(0.0, 0.0, 50.0, 50.0), red, FillRule::NonZero),
            filled(
                rect_segments(25.0, 25.0, 50.0, 50.0),
                blue,
                FillRule::NonZero,
            ),
            filled(
                rect_segments(40.0, 40.0, 10.0, 10.0),
                red,
                FillRule::NonZero,
            ),
        ]),
        &TEXT,
        to_screen(),
    );

    let colours: Vec<Color32> = triangles(&meshes).map(|[a, _, _]| a.color).collect();
    let mut runs: Vec<Color32> = Vec::new();
    for colour in colours {
        if runs.last() != Some(&colour) {
            runs.push(colour);
        }
    }
    assert_eq!(
        runs,
        vec![Color32::RED, Color32::BLUE, Color32::RED],
        "triangles are emitted in item order"
    );
}

// Why: display lists can be built by hand, and the display-list contract requires backends to skip invalid items
// rather than panic. Non-finite coordinates in particular must never reach the GPU, and one bad item must not stop
// the rest of the figure from drawing.
#[test]
fn invalid_items_are_skipped_without_panicking_and_valid_items_still_draw() {
    let bad_stroke = |width: f64, dash: Vec<f64>| Item {
        source: None,
        kind: ItemKind::Path(PathItem {
            segments: vec![
                PathSegment::MoveTo(Point::new(0.0, 200.0)),
                PathSegment::LineTo(Point::new(100.0, 200.0)),
            ],
            fill: None,
            stroke: Some(Stroke {
                color: Rgba::BLACK,
                width,
                dash,
                dash_offset: 0.0,
                cap: LineCap::Butt,
                join: LineJoin::Miter,
            }),
        }),
    };
    let nan = f64::NAN;
    let items = vec![
        filled(
            rect_segments(nan, 0.0, 10.0, 10.0),
            Rgba::BLACK,
            FillRule::NonZero,
        ),
        filled(
            vec![
                PathSegment::LineTo(Point::new(0.0, 0.0)),
                PathSegment::LineTo(Point::new(10.0, 0.0)),
                PathSegment::LineTo(Point::new(10.0, 10.0)),
            ],
            Rgba::BLACK,
            FillRule::NonZero,
        ),
        bad_stroke(f64::INFINITY, vec![]),
        bad_stroke(-1.0, vec![]),
        bad_stroke(1.0, vec![0.0, 0.0]),
        bad_stroke(1.0, vec![nan, 2.0]),
        group(
            None,
            Some(Transform {
                a: nan,
                ..Transform::IDENTITY
            }),
            vec![filled(
                rect_segments(0.0, 0.0, 10.0, 10.0),
                Rgba::BLACK,
                FillRule::NonZero,
            )],
        ),
        Item {
            source: None,
            kind: ItemKind::Glyphs(GlyphsItem {
                font: TEXT
                    .layout("H", false, 10.0)
                    .items
                    .iter()
                    .find_map(|item| match item {
                        TextItem::Glyphs(run) => Some(run.font),
                        TextItem::Rule { .. } => None,
                    })
                    .expect("a glyph run"),
                size_pt: nan,
                color: Rgba::BLACK,
                text: "H".to_owned(),
                glyphs: vec![PlacedGlyph {
                    id: 43,
                    x: 0.0,
                    y: 0.0,
                    text_range: 0..1,
                }],
            }),
        },
        filled(
            rect_segments(300.0, 250.0, 20.0, 10.0),
            Rgba::BLACK,
            FillRule::NonZero,
        ),
    ];

    let meshes = tessellate(&list(items), &TEXT, to_screen());

    assert!(
        triangles(&meshes)
            .flatten()
            .all(|v| v.pos.x.is_finite() && v.pos.y.is_finite()),
        "no vertex has a non-finite position"
    );
    assert!(
        ink_at(&meshes, Point::new(310.0, 255.0)),
        "the valid rectangle after the invalid items is drawn"
    );
}
