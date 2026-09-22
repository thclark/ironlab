//! Tessellation of display lists into egui meshes.
//!
//! The tests measure geometry (triangle area and bounding boxes) rather than comparing vertex lists, so that they hold
//! for any correct triangulation and fail only when the drawn shape is wrong. Image items are drawn through a texture
//! provider, which the canvas asks for one texture per tile of an image; a recording provider lets the quads and the
//! pixels rendered for them be checked without a GPU, and the cache the interactive canvas uses is checked against an
//! egui context of its own.

mod common;

use std::cell::Cell;
use std::sync::Arc;

use common::{TEXT, assert_close, scale_then_translate};
use egui::{Color32, Mesh, Pos2, TextureId};
use ironlab_scene::display::{
    DisplayList, Fill, FillRule, GlyphsItem, ImageItem, Item, ItemKind, LineCap, LineJoin,
    PathItem, PathSegment, PlacedGlyph, Point, Rect, Rgba, Stroke, Transform,
};
use ironlab_text::TextItem;
use ironlab_viewer::canvas::{TextureCache, TextureProvider, tessellate_with};
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
            depth: None,
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
            depth: None,
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
            depth: None,
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

// ---------------------------------------------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------------------------------------------

/// A distinct opaque colour for the pixel in row `j` and column `i` of an image of up to 25 rows and columns.
fn tag(j: u32, i: u32) -> [u8; 3] {
    [(10 * j + 1) as u8, (10 * i + 1) as u8, 200]
}

/// The RGB samples, in row order, of an image of `height` rows and `width` columns coloured by [`tag`].
fn tagged_rgb(width: u32, height: u32) -> Vec<u8> {
    (0..height)
        .flat_map(|j| (0..width).flat_map(move |i| tag(j, i)))
        .collect()
}

/// The opaque texel that [`tag`] gives the pixel in row `j` and column `i`.
fn tag_texel(j: u32, i: u32) -> Color32 {
    let [r, g, b] = tag(j, i);
    Color32::from_rgb(r, g, b)
}

fn image(rect: Rect, width: u32, height: u32, channels: u8, samples: Vec<u8>) -> Item {
    Item {
        source: None,
        kind: ItemKind::Image(ImageItem {
            rect,
            width,
            height,
            channels,
            samples: Arc::from(samples),
            depth: None,
        }),
    }
}

/// A texture provider that renders every tile it is asked for, keeps the result, and answers with a user texture
/// id numbered from one, so that a mesh carrying a tile can be told from one carrying egui's default texture.
struct Recorder {
    max_side: u32,
    /// The tiles rendered, in the order they were requested: the tile index and the pixels rendered for it.
    tiles: Vec<(u32, egui::ColorImage)>,
}

impl Recorder {
    fn new(max_side: u32) -> Self {
        Self {
            max_side,
            tiles: Vec::new(),
        }
    }
}

impl TextureProvider for Recorder {
    fn max_side(&self) -> u32 {
        self.max_side
    }

    fn texture(
        &mut self,
        _samples: &Arc<[u8]>,
        tile: u32,
        render: &mut dyn FnMut() -> egui::ColorImage,
    ) -> TextureId {
        self.tiles.push((tile, render()));
        TextureId::User(self.tiles.len() as u64)
    }
}

/// The screen position of the item-space point `(x, y)` of an item beneath a group carrying `transform`.
fn on_screen(transform: Transform, x: f64, y: f64) -> Pos2 {
    to_screen().apply(transform.apply(Point::new(x, y)))
}

/// The position and texture coordinate of every vertex of a mesh, for failure messages.
fn vertices_of(mesh: &Mesh) -> Vec<(Pos2, Pos2)> {
    mesh.vertices.iter().map(|v| (v.pos, v.uv)).collect()
}

/// The smallest and largest value of `pick` over the vertices of a mesh.
fn extent(mesh: &Mesh, pick: impl Fn(&egui::epaint::Vertex) -> f32) -> (f32, f32) {
    mesh.vertices
        .iter()
        .map(pick)
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        })
}

/// Asserts that `mesh` is one textured quad for the item-space rectangle `rect` beneath `transform`: four white
/// vertices at the screen positions of the rectangle's corners, carrying the texture coordinates (0, 0), (1, 0),
/// (0, 1) and (1, 1) at its top-left, top-right, bottom-left and bottom-right corners, two triangles that cover the
/// whole quad, and `texture`.
#[track_caller]
fn assert_textured_quad(mesh: &Mesh, transform: Transform, rect: Rect, texture: TextureId) {
    assert_eq!(
        mesh.texture_id, texture,
        "the mesh samples the tile's texture"
    );
    assert_ne!(
        mesh.texture_id,
        TextureId::default(),
        "an image never samples egui's default texture"
    );
    assert_eq!(
        mesh.vertices.len(),
        4,
        "one quad has four vertices: {:?}",
        vertices_of(mesh)
    );
    assert_eq!(mesh.indices.len(), 6, "one quad is two triangles");
    let corners: [((f32, f32), (f64, f64)); 4] = [
        ((0.0, 0.0), (rect.x, rect.y)),
        ((1.0, 0.0), (rect.right(), rect.y)),
        ((0.0, 1.0), (rect.x, rect.bottom())),
        ((1.0, 1.0), (rect.right(), rect.bottom())),
    ];
    for ((u, v), (x, y)) in corners {
        let expected = on_screen(transform, x, y);
        let vertex = mesh
            .vertices
            .iter()
            .find(|vertex| (vertex.uv.x - u).abs() <= 1e-6 && (vertex.uv.y - v).abs() <= 1e-6)
            .unwrap_or_else(|| {
                panic!(
                    "a vertex carries the texture coordinate ({u}, {v}): {:?}",
                    vertices_of(mesh)
                )
            });
        assert!(
            (vertex.pos - expected).length() <= 1e-3,
            "the corner with texture coordinate ({u}, {v}) lies at {expected:?}, not {:?}",
            vertex.pos
        );
        assert_eq!(
            vertex.color,
            Color32::WHITE,
            "the texture is drawn unmodulated"
        );
    }
    let parallelogram =
        (transform.a * transform.d - transform.b * transform.c).abs() * rect.width * rect.height;
    let quad = std::slice::from_ref(mesh);
    assert_close(
        area(quad),
        scaled_area(parallelogram),
        1e-2,
        "the two triangles cover the quad",
    );
    let centre = transform.apply(Point::new(
        rect.x + rect.width / 2.0,
        rect.y + rect.height / 2.0,
    ));
    assert!(ink_at(quad, centre), "the centre of the quad is covered");
}

// Why: an image is drawn as a textured quad whose corners are the corners of its rectangle under the same transforms
// as every other leaf, and the compiler places a mirrored pixel range with a negative scale, so a translating, scaling
// and mirroring transform must land the corners exactly where `LeafContext` maps them. A quad placed by the
// untransformed rectangle, by the group transform without the screen transform, or with its texture coordinates at
// the wrong corners would draw the raster elsewhere, at the wrong size, or upside down.
#[test]
fn an_image_becomes_one_textured_quad_at_its_transformed_corners() {
    // Pixel space [0, 3] × [0, 2], scaled by 10 and −5 (mirroring the rows) and moved to (20, 60), covers the figure
    // rectangle [20, 50] × [50, 60].
    let transform = scale_then_translate(10.0, -5.0, 20.0, 60.0);
    let rect = Rect::new(0.0, 0.0, 3.0, 2.0);
    let mut textures = Recorder::new(8192);
    let meshes = tessellate_with(
        &list(vec![group(
            None,
            Some(transform),
            vec![image(rect, 3, 2, ImageItem::RGB, tagged_rgb(3, 2))],
        )]),
        &TEXT,
        to_screen(),
        &mut textures,
    );

    assert_eq!(meshes.len(), 1, "an image within one tile is one mesh");
    assert_eq!(textures.tiles.len(), 1, "one tile is rendered");
    assert_textured_quad(&meshes[0], transform, rect, TextureId::User(1));
    assert_rect_close(
        bbox(&meshes),
        egui::Rect::from_min_max(
            to_screen().apply(Point::new(20.0, 50.0)),
            to_screen().apply(Point::new(50.0, 60.0)),
        ),
        1e-3,
    );
}

// Why: on the floor or a wall of a three-dimensional axes the placement has shear, so the image is a parallelogram on
// screen. A mesh built from an axis-aligned rectangle (egui's `add_rect_with_uv`) would draw the bounding box of the
// parallelogram with the raster stretched to fill it, so every corner must land where the transform sends it and the
// covered area must be the parallelogram's, not its bounding box's.
#[test]
fn a_skewed_transform_maps_an_image_to_a_parallelogram() {
    // The corners of pixel space [0, 6] × [0, 4] land at (30, 40), (90, 52), (46, 64) and (106, 76): a bounding box
    // of 76 × 36 = 2736 around a parallelogram of |10 · 6 − 2 · 4| · 6 · 4 = 1248.
    let transform = Transform {
        a: 10.0,
        b: 2.0,
        c: 4.0,
        d: 6.0,
        e: 30.0,
        f: 40.0,
    };
    let rect = Rect::new(0.0, 0.0, 6.0, 4.0);
    let mut textures = Recorder::new(8192);
    let meshes = tessellate_with(
        &list(vec![group(
            None,
            Some(transform),
            vec![image(rect, 6, 4, ImageItem::RGB, tagged_rgb(6, 4))],
        )]),
        &TEXT,
        to_screen(),
        &mut textures,
    );

    assert_eq!(meshes.len(), 1);
    assert_textured_quad(&meshes[0], transform, rect, TextureId::User(1));
    assert!(
        area(&meshes) < scaled_area(2000.0),
        "the quad is the parallelogram, not its bounding box"
    );
}

// Why: the texture is what the reader sees, so its texels must be the item's samples pixel for pixel and in row
// order: three-channel samples are opaque, and four-channel samples carry straight alpha that egui stores
// premultiplied, so the conversion must be the one egui applies to straight alpha (`from_rgba_unmultiplied`), or
// translucent pixels would be drawn too bright.
#[test]
fn a_tile_is_rendered_from_the_items_samples_with_opaque_rgb_and_straight_alpha_rgba() {
    let rgb = tagged_rgb(3, 2);
    let rgba: Vec<u8> = [
        [255u8, 0, 0, 255],
        [0, 255, 0, 128],
        [0, 0, 255, 0],
        [10, 20, 30, 64],
    ]
    .concat();
    let mut textures = Recorder::new(8192);
    tessellate_with(
        &list(vec![
            image(
                Rect::new(0.0, 0.0, 3.0, 2.0),
                3,
                2,
                ImageItem::RGB,
                rgb.clone(),
            ),
            image(
                Rect::new(10.0, 0.0, 2.0, 2.0),
                2,
                2,
                ImageItem::RGBA,
                rgba.clone(),
            ),
        ]),
        &TEXT,
        to_screen(),
        &mut textures,
    );

    assert_eq!(textures.tiles.len(), 2, "each image is one tile");
    let (tile, opaque) = &textures.tiles[0];
    assert_eq!(*tile, 0, "an image within one tile is tile 0");
    assert_eq!(opaque.size, [3, 2], "the texture is width by height");
    let expected: Vec<Color32> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| Color32::from_rgb(p[0], p[1], p[2]))
        .collect();
    assert_eq!(
        opaque.pixels, expected,
        "RGB samples become opaque texels in row order"
    );
    let (_, translucent) = &textures.tiles[1];
    assert_eq!(translucent.size, [2, 2]);
    let expected: Vec<Color32> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
        .collect();
    assert_eq!(
        translucent.pixels, expected,
        "RGBA samples are converted from straight alpha as `ColorImage::from_rgba_unmultiplied` converts them"
    );
}

/// The placement of the tiling tests: pixel space [0, 5] × [0, 3] scaled by 6 and moved to (40, 30), so that with
/// tiles of at most 2 pixels a side, tile column c covers figure x ∈ [40 + 12c, 52 + 12c] (the last column only to
/// 70) and tile row r covers y ∈ [30 + 12r, 42 + 12r] (the last row only to 48).
const TILING: Transform = Transform {
    a: 6.0,
    b: 0.0,
    c: 0.0,
    d: 6.0,
    e: 40.0,
    f: 30.0,
};

/// Tessellates a 5 × 3 image beneath [`TILING`] inside a group with `clip`, with tiles of at most 2 pixels a side.
fn tessellate_tiled(clip: Option<Rect>, textures: &mut Recorder) -> Vec<Mesh> {
    tessellate_with(
        &list(vec![group(
            clip,
            Some(TILING),
            vec![image(
                Rect::new(0.0, 0.0, 5.0, 3.0),
                5,
                3,
                ImageItem::RGB,
                tagged_rgb(5, 3),
            )],
        )]),
        &TEXT,
        to_screen(),
        textures,
    )
}

// Why: a GPU texture has a largest side, and a data image can be far wider than it (a spectrogram of a long record),
// so the raster is cut into tiles of at most that side, each a texture and a quad of its own. The pixels of every
// tile must be the tile's sub-rectangle of the source; the tile index is part of the provider's key, so its meaning
// must be defined, and row-major order matches the order of the samples; and the quads must share their edges
// exactly, because a tile a pixel out shows a seam of background through the data.
#[test]
fn an_image_wider_than_the_largest_texture_is_cut_into_abutting_tiles_in_row_major_order() {
    let transform = TILING;
    let mut textures = Recorder::new(2);
    let meshes = tessellate_tiled(None, &mut textures);

    assert_eq!(
        meshes.len(),
        6,
        "a 5 × 3 image at a largest side of 2 is 3 × 2 tiles"
    );
    assert_eq!(textures.tiles.len(), 6);
    let columns = [(0u32, 2u32), (2, 4), (4, 5)];
    let rows = [(0u32, 2u32), (2, 3)];
    for (k, (mesh, (tile, pixels))) in meshes.iter().zip(&textures.tiles).enumerate() {
        let (r, c) = (k / 3, k % 3);
        let (x0, x1) = columns[c];
        let (y0, y1) = rows[r];
        assert_eq!(
            *tile as usize, k,
            "tile {k} is requested in row-major order"
        );
        let sub = Rect::new(
            f64::from(x0),
            f64::from(y0),
            f64::from(x1 - x0),
            f64::from(y1 - y0),
        );
        assert_textured_quad(mesh, transform, sub, TextureId::User(k as u64 + 1));
        assert_eq!(
            pixels.size,
            [(x1 - x0) as usize, (y1 - y0) as usize],
            "tile {k} has the size of its sub-rectangle"
        );
        let expected: Vec<Color32> = (y0..y1)
            .flat_map(|j| (x0..x1).map(move |i| tag_texel(j, i)))
            .collect();
        assert_eq!(
            pixels.pixels, expected,
            "tile {k} holds the pixels of rows {y0}..{y1} and columns {x0}..{x1}"
        );
    }
    for (r, row) in meshes.chunks(3).enumerate() {
        for (c, pair) in row.windows(2).enumerate() {
            assert_eq!(
                extent(&pair[0], |v| v.pos.x).1,
                extent(&pair[1], |v| v.pos.x).0,
                "tiles ({r}, {c}) and ({r}, {}) share their vertical edge exactly",
                c + 1
            );
        }
    }
    for (c, (upper, lower)) in meshes[..3].iter().zip(&meshes[3..]).enumerate() {
        assert_eq!(
            extent(upper, |v| v.pos.y).1,
            extent(lower, |v| v.pos.y).0,
            "tiles (0, {c}) and (1, {c}) share their horizontal edge exactly"
        );
    }
}

// Why: an image panned so that most of it lies outside the plot box must not be uploaded whole: a tile whose quad
// lies wholly outside the group's clip draws nothing, so asking the provider for it would upload a texture that no
// pixel samples. Only the tiles the clip touches are requested, in their row-major order, and those are clipped like
// any other leaf.
#[test]
fn tiles_wholly_outside_the_clip_are_never_requested() {
    // The clip keeps x ∈ [40, 50] of the image's [40, 70]: part of tile column 0 and none of columns 1 and 2, which
    // begin at x = 52.
    let clip = Rect::new(40.0, 30.0, 10.0, 18.0);
    let mut textures = Recorder::new(2);
    let meshes = tessellate_tiled(Some(clip), &mut textures);

    let requested: Vec<u32> = textures.tiles.iter().map(|(tile, _)| *tile).collect();
    assert_eq!(
        requested,
        vec![0, 3],
        "only the two tiles of the first column are requested, in row-major order"
    );
    assert_eq!(
        meshes.iter().map(|m| m.texture_id).collect::<Vec<_>>(),
        vec![TextureId::User(1), TextureId::User(2)],
        "the two requested tiles are the two meshes"
    );
    assert_close(
        area(&meshes),
        scaled_area(10.0 * 18.0),
        1e-2,
        "the visible part of the first column is drawn",
    );
    assert_rect_close(
        bbox(&meshes),
        egui::Rect::from_min_max(
            to_screen().apply(Point::new(40.0, 30.0)),
            to_screen().apply(Point::new(50.0, 48.0)),
        ),
        1e-3,
    );
}

// Why: `tessellate` keeps its signature for the callers that draw no images, and without a provider there is no
// texture to draw with; an image item must then draw nothing rather than a white quad over the plot, while the other
// items of the list still draw.
#[test]
fn without_a_texture_provider_an_image_item_draws_nothing() {
    let meshes = tessellate(
        &list(vec![
            image(
                Rect::new(20.0, 0.0, 30.0, 20.0),
                3,
                2,
                ImageItem::RGB,
                tagged_rgb(3, 2),
            ),
            filled(
                rect_segments(0.0, 0.0, 10.0, 10.0),
                Rgba::BLACK,
                FillRule::NonZero,
            ),
        ]),
        &TEXT,
        to_screen(),
    );

    assert!(
        meshes.iter().all(|m| m.texture_id == TextureId::default()),
        "no mesh samples a texture"
    );
    assert_close(
        area(&meshes),
        scaled_area(100.0),
        1e-3,
        "only the square is drawn",
    );
    assert!(
        !ink_at(&meshes, Point::new(35.0, 10.0)),
        "nothing is drawn where the image lies"
    );
}

// Why: display lists can be built by hand, and the display-list contract requires backends to skip invalid items
// rather than panic. An image with a channel count egui cannot upload, with no pixels, with fewer samples than its
// size claims (which would read past the buffer), with alpha it does not carry, or with a non-finite or empty
// rectangle (which would put a NaN on the GPU or a degenerate quad) must be skipped without asking for a texture,
// and the items around it must still draw.
#[test]
fn invalid_image_items_are_skipped_without_a_texture_and_the_other_items_still_draw() {
    let samples = tagged_rgb(3, 2);
    let nan = f64::NAN;
    let items = vec![
        image(
            Rect::new(0.0, 0.0, 3.0, 2.0),
            3,
            2,
            2,
            samples[..12].to_vec(),
        ),
        image(
            Rect::new(0.0, 0.0, 3.0, 2.0),
            0,
            2,
            ImageItem::RGB,
            Vec::new(),
        ),
        image(
            Rect::new(0.0, 0.0, 3.0, 2.0),
            3,
            0,
            ImageItem::RGB,
            Vec::new(),
        ),
        image(
            Rect::new(0.0, 0.0, 3.0, 2.0),
            3,
            2,
            ImageItem::RGB,
            samples[..17].to_vec(),
        ),
        image(
            Rect::new(0.0, 0.0, 3.0, 2.0),
            3,
            2,
            ImageItem::RGBA,
            samples.clone(),
        ),
        image(
            Rect::new(nan, 0.0, 3.0, 2.0),
            3,
            2,
            ImageItem::RGB,
            samples.clone(),
        ),
        image(
            Rect::new(0.0, 0.0, f64::INFINITY, 2.0),
            3,
            2,
            ImageItem::RGB,
            samples.clone(),
        ),
        image(
            Rect::new(0.0, 0.0, 0.0, 2.0),
            3,
            2,
            ImageItem::RGB,
            samples.clone(),
        ),
        image(
            Rect::new(100.0, 100.0, 30.0, 20.0),
            3,
            2,
            ImageItem::RGB,
            samples.clone(),
        ),
        filled(
            rect_segments(300.0, 250.0, 20.0, 10.0),
            Rgba::BLACK,
            FillRule::NonZero,
        ),
    ];
    let mut textures = Recorder::new(8192);
    let meshes = tessellate_with(&list(items), &TEXT, to_screen(), &mut textures);

    assert_eq!(textures.tiles.len(), 1, "only the valid image is rendered");
    assert!(
        triangles(&meshes)
            .flatten()
            .all(|v| v.pos.x.is_finite() && v.pos.y.is_finite()),
        "no vertex has a non-finite position"
    );
    assert_eq!(
        meshes
            .iter()
            .filter(|m| m.texture_id != TextureId::default())
            .count(),
        1,
        "one mesh is textured"
    );
    assert!(
        ink_at(&meshes, Point::new(115.0, 110.0)),
        "the valid image after the invalid ones is drawn"
    );
    assert!(
        ink_at(&meshes, Point::new(310.0, 255.0)),
        "the rectangle after the invalid items is drawn"
    );
}

// Why: axes clip their artists to the plot box, and an image panned half out of it must show exactly the half that is
// left, with the texture range cut in proportion: a clip that trimmed the quad but kept the corner texture
// coordinates would squash the whole raster into the visible half, and one that reset the coordinates to egui's white
// pixel would draw a flat colour.
#[test]
fn a_clip_that_halves_an_image_keeps_half_its_texture_range() {
    // Pixel space [0, 4] × [0, 2] covers the figure rectangle [0, 100] × [0, 100]; the clip is in the parent space.
    let transform = scale_then_translate(25.0, 50.0, 0.0, 0.0);
    let rect = Rect::new(0.0, 0.0, 4.0, 2.0);
    for (clip, (u_min, u_max)) in [
        (Rect::new(0.0, 0.0, 50.0, 100.0), (0.0f32, 0.5f32)),
        (Rect::new(50.0, 0.0, 50.0, 100.0), (0.5, 1.0)),
    ] {
        let mut textures = Recorder::new(8192);
        let meshes = tessellate_with(
            &list(vec![group(
                Some(clip),
                Some(transform),
                vec![image(rect, 4, 2, ImageItem::RGB, tagged_rgb(4, 2))],
            )]),
            &TEXT,
            to_screen(),
            &mut textures,
        );

        assert_eq!(meshes.len(), 1);
        let mesh = &meshes[0];
        assert_eq!(
            mesh.texture_id,
            TextureId::User(1),
            "the clipped mesh keeps its texture"
        );
        assert_close(
            area(&meshes),
            scaled_area(50.0 * 100.0),
            1e-2,
            "half the image remains",
        );
        let origin = to_screen().apply(Point::new(0.0, 0.0));
        for v in &mesh.vertices {
            assert_eq!(v.color, Color32::WHITE);
            let u = (v.pos.x - origin.x) / (100.0 * SCALE);
            let w = (v.pos.y - origin.y) / (100.0 * SCALE);
            assert!(
                (v.uv.x - u).abs() <= 1e-4 && (v.uv.y - w).abs() <= 1e-4,
                "the texture coordinate of the vertex at {:?} is in proportion to its position: expected ({u}, {w}), got {:?}",
                v.pos,
                v.uv
            );
        }
        let (lo, hi) = extent(mesh, |v| v.uv.x);
        assert!(
            (lo - u_min).abs() <= 1e-4 && (hi - u_max).abs() <= 1e-4,
            "the clipped mesh spans the texture range [{u_min}, {u_max}], not [{lo}, {hi}]"
        );
    }
}

// Why: the display list is ordered back to front, and an image between two paths (a floor image under a surface,
// isolines drawn on top of a mapped field) must be drawn between them; batching textured meshes apart from plain ones
// would put the image over the isolines drawn on it.
#[test]
fn an_image_keeps_its_place_in_the_paint_order() {
    let meshes = tessellate_with(
        &list(vec![
            filled(
                rect_segments(0.0, 0.0, 50.0, 50.0),
                Rgba::new(1.0, 0.0, 0.0, 1.0),
                FillRule::NonZero,
            ),
            image(
                Rect::new(10.0, 10.0, 30.0, 30.0),
                3,
                2,
                ImageItem::RGB,
                tagged_rgb(3, 2),
            ),
            filled(
                rect_segments(20.0, 20.0, 10.0, 10.0),
                Rgba::new(0.0, 0.0, 1.0, 1.0),
                FillRule::NonZero,
            ),
        ]),
        &TEXT,
        to_screen(),
        &mut Recorder::new(8192),
    );

    let textures: Vec<TextureId> = meshes.iter().map(|m| m.texture_id).collect();
    assert_eq!(
        textures,
        vec![
            TextureId::default(),
            TextureId::User(1),
            TextureId::default()
        ],
        "meshes are emitted in item order"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// The texture cache of the interactive canvas
// ---------------------------------------------------------------------------------------------------------------

/// A sample buffer of `n` bytes all equal to `fill`, shared as a display list shares it.
fn buffer(n: usize, fill: u8) -> Arc<[u8]> {
    Arc::from(vec![fill; n])
}

/// A two-by-one image, as the canvas renders a tile.
fn two_texels() -> egui::ColorImage {
    egui::ColorImage::new([2, 1], vec![Color32::RED, Color32::BLUE])
}

// Why: the samples of an image do not change between frames, only the view does, so a pan must not re-upload every
// image every frame: the same sample buffer and tile must be served the texture uploaded before, without calling
// back for the pixels, while another tile of the same buffer, or another buffer, is a texture of its own. The
// texture must ask for nearest sampling, or the screen would blur the pixel edges that the offscreen renderer and
// the PDF draw hard.
#[test]
fn the_texture_cache_uploads_each_buffer_and_tile_once_with_nearest_sampling() {
    let ctx = egui::Context::default();
    let mut cache = TextureCache::new(ctx.clone());
    let renders = Cell::new(0u32);
    let mut render = || {
        renders.set(renders.get() + 1);
        two_texels()
    };
    let (a, b) = (buffer(6, 1), buffer(6, 2));

    let first = cache.texture(&a, 0, &mut render);
    let again = cache.texture(&a, 0, &mut render);
    assert_eq!(first, again, "the same buffer and tile share one texture");
    assert_eq!(renders.get(), 1, "the pixels are rendered once");
    assert_ne!(first, TextureId::default());
    let other_tile = cache.texture(&a, 1, &mut render);
    assert_ne!(
        other_tile, first,
        "another tile of the same buffer is another texture"
    );
    assert_eq!(renders.get(), 2);
    let other_buffer = cache.texture(&b, 0, &mut render);
    assert!(
        other_buffer != first && other_buffer != other_tile,
        "another buffer is another texture"
    );
    assert_eq!(renders.get(), 3, "another buffer is rendered for itself");

    let manager = ctx.tex_manager();
    let textures = manager.read();
    let meta = textures
        .meta(first)
        .expect("the texture is held by the context");
    assert_eq!(meta.size, [2, 1], "the texture is the rendered tile");
    assert_eq!(
        meta.options,
        egui::TextureOptions::NEAREST,
        "images are drawn with hard pixel edges on screen"
    );
}

// Why: the interactive canvas rebuilds its meshes whenever the scale or position of the figure changes, and the image
// textures must survive that rebuild: two tessellations of the same display list at different placements must sample
// the same texture, allocated once, which is what spares the GPU an upload per gesture.
#[test]
fn two_tessellations_of_the_same_display_list_share_the_cached_texture() {
    let ctx = egui::Context::default();
    let mut cache = TextureCache::new(ctx.clone());
    let display = list(vec![image(
        Rect::new(0.0, 0.0, 3.0, 2.0),
        3,
        2,
        ImageItem::RGB,
        tagged_rgb(3, 2),
    )]);
    let before = ctx.tex_manager().read().num_allocated();

    let first = tessellate_with(&display, &TEXT, to_screen(), &mut cache);
    let moved = ScreenTransform {
        scale: 3.0,
        origin: Pos2::new(0.0, 0.0),
    };
    let second = tessellate_with(&display, &TEXT, moved, &mut cache);

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_ne!(first[0].texture_id, TextureId::default());
    assert_eq!(
        first[0].texture_id, second[0].texture_id,
        "the rebuilt mesh samples the texture uploaded for the first"
    );
    assert_eq!(
        ctx.tex_manager().read().num_allocated(),
        before + 1,
        "one texture was allocated for the two tessellations"
    );
}

// Why: a figure edited to hold other data leaves its old textures behind, and a pan through a large figure must not
// let the GPU memory grow without bound: after a rebuild, the textures it did not use are freed and the ones it used
// stay, so that the next rebuild neither re-uploads what is on screen nor keeps what is not.
#[test]
fn retain_requested_frees_the_textures_not_used_by_the_latest_rebuild() {
    let ctx = egui::Context::default();
    let mut cache = TextureCache::new(ctx.clone());
    let renders = Cell::new(0u32);
    let mut render = || {
        renders.set(renders.get() + 1);
        two_texels()
    };
    let (a, b) = (buffer(3, 1), buffer(3, 2));
    let id_a = cache.texture(&a, 0, &mut render);
    let id_b = cache.texture(&b, 0, &mut render);

    cache.retain_requested();
    {
        let manager = ctx.tex_manager();
        let textures = manager.read();
        assert!(
            textures.meta(id_a).is_some() && textures.meta(id_b).is_some(),
            "both textures were requested since the cache was made, so both are kept"
        );
    }

    assert_eq!(cache.texture(&a, 0, &mut render), id_a);
    cache.retain_requested();
    {
        let manager = ctx.tex_manager();
        let textures = manager.read();
        assert!(
            textures.meta(id_a).is_some(),
            "the texture requested in the latest round survives"
        );
        assert!(
            textures.meta(id_b).is_none(),
            "the texture not requested in the latest round is freed"
        );
    }
    assert_eq!(renders.get(), 2, "nothing has been rendered again so far");

    let restored = cache.texture(&b, 0, &mut render);
    assert_eq!(
        renders.get(),
        3,
        "a freed texture is rendered again when its buffer is drawn again"
    );
    assert!(
        ctx.tex_manager().read().meta(restored).is_some(),
        "the rendered texture is held by the context again"
    );
    assert_eq!(
        cache.texture(&a, 0, &mut render),
        id_a,
        "a kept texture is still served from the cache"
    );
    assert_eq!(renders.get(), 3);
}

// Why: the cache keys a texture by the address of its sample buffer, and an allocator reuses an address as soon as
// the buffer at it is dropped: a figure whose image data is replaced by an array of the same size can land at the
// same address, and a cache that did not hold the buffer alive would draw the old pixels for the new data. The
// mechanism is pinned directly: the cache holds a clone of the buffer while its texture is cached, so that the
// address cannot be reused, and lets it go with the texture once a rebuild has not requested it; a buffer allocated
// after another was dropped is therefore always rendered afresh.
#[test]
fn the_cache_holds_a_buffer_while_its_texture_is_cached_so_its_address_cannot_be_reused() {
    let ctx = egui::Context::default();
    let mut cache = TextureCache::new(ctx.clone());
    let renders = Cell::new(0u32);
    let mut render = || {
        renders.set(renders.get() + 1);
        two_texels()
    };

    let samples = buffer(64, 1);
    assert_eq!(Arc::strong_count(&samples), 1);
    let id = cache.texture(&samples, 0, &mut render);
    assert_eq!(
        Arc::strong_count(&samples),
        2,
        "the cache holds the buffer while its texture is cached"
    );
    cache.retain_requested();
    assert_eq!(
        Arc::strong_count(&samples),
        2,
        "a rebuild that requested the buffer keeps it"
    );
    cache.retain_requested();
    assert_eq!(
        Arc::strong_count(&samples),
        1,
        "a rebuild that did not request the buffer lets it go"
    );
    assert!(
        ctx.tex_manager().read().meta(id).is_none(),
        "the texture goes with it"
    );
    drop(samples);

    // Two more buffers of the same size, allocated and dropped in turn without a rebuild between them, so that the
    // cache still holds the first when the second is allocated: each is rendered for itself.
    for round in 2..4u8 {
        let samples = buffer(64, round);
        cache.texture(&samples, 0, &mut render);
        assert_eq!(
            renders.get(),
            u32::from(round),
            "buffer {round} is rendered afresh, whatever address it was given"
        );
        drop(samples);
    }
}

// Why: a tile must fit the GPU the context runs on, whose limit the backend reports through egui's input, and must
// not be so large that one upload stalls a frame or exceeds what the offscreen renderer tiles by, so the largest side
// is the context's limit capped at 8192. The limit is set through a pass, as a backend sets it, before the cache is
// made, so the test holds whether the cache reads it when it is made or when it is asked.
#[test]
fn the_texture_cache_tiles_by_the_contexts_limit_capped_at_8192() {
    let max_side_with = |limit: usize| {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput {
            max_texture_side: Some(limit),
            ..Default::default()
        });
        // The pass allocates the font texture, and epaint asserts in debug builds that its delta is consumed.
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
        TextureCache::new(ctx).max_side()
    };

    assert_eq!(
        max_side_with(16_384),
        8192,
        "a limit above the cap is capped"
    );
    assert_eq!(
        max_side_with(4096),
        4096,
        "a limit below the cap is the limit"
    );
}
