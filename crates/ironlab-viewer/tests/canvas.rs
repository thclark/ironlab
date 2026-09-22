//! Tessellation of display lists into draw lists.
//!
//! Every list is in figure space: a vertex position is a position in figure points whatever the resolution the list
//! is prepared for, because the mapping from the figure to the screen is a uniform of the painter. The tests
//! therefore measure the triangles of fills, glyphs and images in figure points (triangle area, bounding boxes and
//! the ink at a point) rather than comparing vertex lists, so that they hold for any correct triangulation and fail
//! only when the drawn shape is wrong. A stroke is not triangles: the canvas hands the painter one segment per edge
//! of the flattened path, in item space, and the constants of the stroke (its transform, colour, width, cap, join
//! and dashes) as one set of params per draw, from which the stroke pipeline expands the segments on the GPU. The
//! tests read the segments and the params directly, because they are the contract with the shaders; what the
//! shaders make of them (caps, joins, dashes and the hairline, in pixels) is tested in `offscreen.rs`. An image
//! becomes draws that each name a tile of the item's samples for the painter to upload; what the painter makes of a
//! tile, and the cache it keeps of them, are tested in `offscreen.rs` too.

mod common;

use std::ops::Range;
use std::sync::Arc;

use common::{TEXT, assert_close, depth_group, figure_with_surface, glyph_h, scale_then_translate};
use egui::Color32;
use ironlab_ir::NodeId;
use ironlab_scene::display::{
    Depth, DepthPlane, DisplayList, Fill, FillRule, GlyphsItem, ImageItem, Item, ItemKind, LineCap,
    LineJoin, PathItem, PathSegment, PlacedGlyph, Point, Rect, Rgba, Stroke, Transform,
};
use ironlab_text::TextItem;
use ironlab_viewer::canvas::{MAX_TILE_SIDE, Resolution, SCREEN_TOLERANCE, tessellate};
use ironlab_viewer::gpu::{
    Draw, DrawKind, DrawList, JOIN_AT_END, JOIN_AT_START, MAX_DASH_ENTRIES, Segment, StrokeParams,
    TileKey, Vertex,
};

// ---------------------------------------------------------------------------------------------------------------
// Display-list fixtures
// ---------------------------------------------------------------------------------------------------------------

/// A background that paints nothing.
const TRANSPARENT: Rgba = Rgba::new(0.0, 0.0, 0.0, 0.0);

/// A display list of 400 × 300 points holding `items` over a white background, which the renderer clears its target
/// to rather than the list drawing it, so that every draw of the draw list is an item's.
fn list(items: Vec<Item>) -> DisplayList {
    with_background(Rgba::WHITE, items)
}

/// A display list of 400 × 300 points holding `items` over `background`.
fn with_background(background: Rgba, items: Vec<Item>) -> DisplayList {
    DisplayList {
        width_pt: 400.0,
        height_pt: 300.0,
        background,
        items,
    }
}

/// The draw list of `display` at `resolution`, checked against what every draw list must satisfy.
#[track_caller]
fn draw_list_at(display: &DisplayList, resolution: Resolution) -> DrawList {
    let drawn = tessellate(display, &TEXT, resolution);
    assert_well_formed(&drawn);
    drawn
}

/// The draw list of `display` at the default resolution.
#[track_caller]
fn draw_list(display: &DisplayList) -> DrawList {
    draw_list_at(display, Resolution::default())
}

/// A resolution of `scale` screen units per figure point with the largest tiles.
fn at_scale(scale: f32) -> Resolution {
    Resolution {
        scale,
        ..Resolution::default()
    }
}

/// A resolution of one screen unit per figure point with tiles of at most `max_tile_side` pixels a side.
fn with_tiles(max_tile_side: u32) -> Resolution {
    Resolution {
        scale: 1.0,
        max_tile_side,
    }
}

fn move_to(x: f64, y: f64) -> PathSegment {
    PathSegment::MoveTo(Point::new(x, y))
}

fn line_to(x: f64, y: f64) -> PathSegment {
    PathSegment::LineTo(Point::new(x, y))
}

/// An open polyline through `points`.
fn polyline(points: &[(f64, f64)]) -> Vec<PathSegment> {
    points
        .iter()
        .enumerate()
        .map(|(k, &(x, y))| if k == 0 { move_to(x, y) } else { line_to(x, y) })
        .collect()
}

/// The polyline through `points`, closed back to its first point.
fn closed_polyline(points: &[(f64, f64)]) -> Vec<PathSegment> {
    let mut segments = polyline(points);
    segments.push(PathSegment::Close);
    segments
}

fn rect_segments(x: f64, y: f64, w: f64, h: f64) -> Vec<PathSegment> {
    closed_polyline(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
}

/// A circle of radius `r` about `(cx, cy)` as the four cubic Béziers a marker or a pie wedge is drawn with.
fn circle_segments(cx: f64, cy: f64, r: f64) -> Vec<PathSegment> {
    // The control-point distance at which a cubic Bézier approximates a quarter circle.
    let k = 0.5523 * r;
    vec![
        PathSegment::MoveTo(Point::new(cx + r, cy)),
        PathSegment::CubicTo(
            Point::new(cx + r, cy + k),
            Point::new(cx + k, cy + r),
            Point::new(cx, cy + r),
        ),
        PathSegment::CubicTo(
            Point::new(cx - k, cy + r),
            Point::new(cx - r, cy + k),
            Point::new(cx - r, cy),
        ),
        PathSegment::CubicTo(
            Point::new(cx - r, cy - k),
            Point::new(cx - k, cy - r),
            Point::new(cx, cy - r),
        ),
        PathSegment::CubicTo(
            Point::new(cx + k, cy - r),
            Point::new(cx + r, cy - k),
            Point::new(cx + r, cy),
        ),
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

/// A solid black stroke of `width` with butt caps and miter joins.
fn black_stroke(width: f64) -> Stroke {
    Stroke {
        color: Rgba::BLACK,
        width,
        dash: Vec::new(),
        dash_offset: 0.0,
        cap: LineCap::Butt,
        join: LineJoin::Miter,
    }
}

/// A stroke of `width` in `color` along `segments`, dashed by `dash` from `dash_offset`, with `cap` and `join`.
fn stroked(
    segments: Vec<PathSegment>,
    color: Rgba,
    width: f64,
    dash: Vec<f64>,
    dash_offset: f64,
    cap: LineCap,
    join: LineJoin,
) -> Item {
    Item {
        source: None,
        kind: ItemKind::Path(PathItem {
            segments,
            fill: None,
            stroke: Some(Stroke {
                color,
                width,
                dash,
                dash_offset,
                cap,
                join,
            }),
            depth: None,
        }),
    }
}

/// `segments` stroked solid black, 4 wide, with butt caps and miter joins.
fn solid(segments: Vec<PathSegment>) -> Item {
    stroked(
        segments,
        Rgba::BLACK,
        4.0,
        Vec::new(),
        0.0,
        LineCap::Butt,
        LineJoin::Miter,
    )
}

/// A path filled in `color` with the non-zero rule and stroked by `stroke`.
fn filled_and_stroked(segments: Vec<PathSegment>, color: Rgba, stroke: Stroke) -> Item {
    Item {
        source: None,
        kind: ItemKind::Path(PathItem {
            segments,
            fill: Some(Fill {
                color,
                rule: FillRule::NonZero,
            }),
            stroke: Some(stroke),
            depth: None,
        }),
    }
}

/// The line along y = 50 from x = 0 to x = 100.
fn line_segments() -> Vec<PathSegment> {
    polyline(&[(0.0, 50.0), (100.0, 50.0)])
}

/// The line of [`line_segments`] stroked 4 wide in black with miter joins.
fn stroked_line(dash: Vec<f64>, dash_offset: f64, cap: LineCap) -> Item {
    stroked(
        line_segments(),
        Rgba::BLACK,
        4.0,
        dash,
        dash_offset,
        cap,
        LineJoin::Miter,
    )
}

/// The line of [`line_segments`] stroked with a width of zero: the thinnest line the device can draw.
fn hairline() -> Item {
    stroked(
        line_segments(),
        Rgba::BLACK,
        0.0,
        Vec::new(),
        0.0,
        LineCap::Butt,
        LineJoin::Miter,
    )
}

/// A run of the glyphs of `text` at `size` points in `color`, laid out by the text engine so that the glyph ids,
/// the font and the advances are real, with the pen origin of the layout at `origin`.
fn glyph_run(text: &str, origin: Point, size: f64, color: Rgba) -> Item {
    let layout = TEXT.layout(text, false, size);
    let run = layout
        .items
        .iter()
        .find_map(|item| match item {
            TextItem::Glyphs(run) => Some(run),
            TextItem::Rule { .. } => None,
        })
        .unwrap_or_else(|| panic!("{text:?} lays out as a glyph run"));
    Item {
        source: None,
        kind: ItemKind::Glyphs(GlyphsItem {
            font: run.font,
            size_pt: run.size_pt,
            color,
            text: run.text.clone(),
            glyphs: run
                .glyphs
                .iter()
                .map(|glyph| PlacedGlyph {
                    id: glyph.id,
                    x: origin.x + glyph.x,
                    y: origin.y + glyph.y,
                    text_range: glyph.text_range.clone(),
                })
                .collect(),
        }),
    }
}

/// `item`, a glyph run, after `edit`.
fn with_run(mut item: Item, edit: impl FnOnce(&mut GlyphsItem)) -> Item {
    let ItemKind::Glyphs(run) = &mut item.kind else {
        panic!("only a glyph run can be edited")
    };
    edit(run);
    item
}

/// `item`, a path, given `depth`.
fn with_depth(mut item: Item, depth: Depth) -> Item {
    let ItemKind::Path(path) = &mut item.kind else {
        panic!("only a path item carries a `Depth`")
    };
    path.depth = Some(depth);
    item
}

/// `item` attributed to the IR node `id`.
fn sourced(mut item: Item, id: u64) -> Item {
    item.source = Some(NodeId(id));
    item
}

/// A filled square of side `size` with its top-left corner at `(x, y)`, lying at `plane`.
fn square_at(x: f64, y: f64, size: f64, color: Rgba, plane: DepthPlane) -> Item {
    with_depth(
        filled(rect_segments(x, y, size, size), color, FillRule::NonZero),
        Depth::Plane(plane),
    )
}

/// Two unit squares at the constant depths 0 and 1, side by side from `(x, y)`, which pin the depth range of the
/// depth group they lie in to [0, 1], so that the z of every other vertex and segment end of the group is
/// `1 − depth` whatever vertices the tessellation happens to emit.
fn range_pins(x: f64, y: f64) -> [Item; 2] {
    [
        square_at(x, y, 1.0, Rgba::BLACK, DepthPlane::constant(0.0)),
        square_at(x + 2.0, y, 1.0, Rgba::BLACK, DepthPlane::constant(1.0)),
    ]
}

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

/// An image item drawn into `rect` sharing `samples`, as a display list shares them.
fn image_of(rect: Rect, width: u32, height: u32, channels: u8, samples: Arc<[u8]>) -> Item {
    Item {
        source: None,
        kind: ItemKind::Image(ImageItem {
            rect,
            width,
            height,
            channels,
            samples,
            depth: None,
        }),
    }
}

fn image(rect: Rect, width: u32, height: u32, channels: u8, samples: Vec<u8>) -> Item {
    image_of(rect, width, height, channels, Arc::from(samples))
}

/// The tile of `columns` and `rows` of an image of `width` columns of `channels` bytes sharing `samples`.
fn tile_key(
    samples: &Arc<[u8]>,
    width: u32,
    channels: u8,
    columns: Range<u32>,
    rows: Range<u32>,
) -> TileKey {
    TileKey {
        samples: Arc::clone(samples),
        width,
        channels,
        columns,
        rows,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Measurements over draw lists
// ---------------------------------------------------------------------------------------------------------------

/// The index range of a triangle draw: a fill, a glyph run or an image tile. Panics for a stroke draw, which holds
/// segments rather than triangles.
#[track_caller]
fn triangles_of(draw: &Draw) -> Range<u32> {
    match &draw.kind {
        DrawKind::Triangles(range) => range.clone(),
        DrawKind::Stroke { .. } => {
            panic!("a triangle draw was expected, but the draw is a stroke: {draw:?}")
        }
    }
}

/// The segments of a stroke draw, in item space. Panics for a triangle draw.
#[track_caller]
fn segments_of<'a>(list: &'a DrawList, draw: &Draw) -> &'a [Segment] {
    match &draw.kind {
        DrawKind::Stroke { segments, .. } => {
            &list.segments[segments.start as usize..segments.end as usize]
        }
        DrawKind::Triangles(_) => {
            panic!("a stroke draw was expected, but the draw is triangles: {draw:?}")
        }
    }
}

/// The params of a stroke draw. Panics for a triangle draw.
#[track_caller]
fn params_of<'a>(list: &'a DrawList, draw: &Draw) -> &'a StrokeParams {
    match &draw.kind {
        DrawKind::Stroke { params, .. } => &list.stroke_params[*params as usize],
        DrawKind::Triangles(_) => {
            panic!("a stroke draw was expected, but the draw is triangles: {draw:?}")
        }
    }
}

/// The one draw of a list of one stroked path, which must be a stroke.
#[track_caller]
fn only_stroke(list: &DrawList) -> &Draw {
    assert_eq!(
        list.draws.len(),
        1,
        "one stroked path is one draw: {:?}",
        list.draws
    );
    let draw = &list.draws[0];
    assert!(
        matches!(draw.kind, DrawKind::Stroke { .. }),
        "the one draw of a stroked path is a stroke: {draw:?}"
    );
    draw
}

/// The z of everything a draw refers to: its vertices' z for triangles, or both ends of every segment for a stroke.
fn depths_of(list: &DrawList, draw: &Draw) -> Vec<f32> {
    match &draw.kind {
        DrawKind::Triangles(_) => vertices_of_draw(list, draw).iter().map(|v| v.z).collect(),
        DrawKind::Stroke { .. } => segments_of(list, draw).iter().flat_map(|s| s.z).collect(),
    }
}

/// The triangles of one draw, each as its three vertices.
fn triangles_in<'a>(list: &'a DrawList, draw: &'a Draw) -> impl Iterator<Item = [Vertex; 3]> + 'a {
    let range = triangles_of(draw);
    list.indices[range.start as usize..range.end as usize]
        .as_chunks::<3>()
        .0
        .iter()
        .map(move |t| {
            [
                list.vertices[t[0] as usize],
                list.vertices[t[1] as usize],
                list.vertices[t[2] as usize],
            ]
        })
}

/// The triangles of every draw of a list, in paint order; the list must hold no stroke draw.
fn triangles(list: &DrawList) -> impl Iterator<Item = [Vertex; 3]> + '_ {
    list.draws
        .iter()
        .flat_map(move |draw| triangles_in(list, draw))
}

/// Total unsigned area of `triangles` in figure points squared.
///
/// Triangles that overlap are counted twice, so this measures a fill, whose triangles tile the shape.
fn area_of(triangles: impl Iterator<Item = [Vertex; 3]>) -> f64 {
    triangles
        .map(|[a, b, c]| {
            let (ax, ay) = (f64::from(a.pos[0]), f64::from(a.pos[1]));
            let (bx, by) = (f64::from(b.pos[0]), f64::from(b.pos[1]));
            let (cx, cy) = (f64::from(c.pos[0]), f64::from(c.pos[1]));
            ((bx - ax) * (cy - ay) - (cx - ax) * (by - ay)).abs() / 2.0
        })
        .sum()
}

/// Total unsigned triangle area of a list in figure points squared, with the caveat of [`area_of`].
fn area(list: &DrawList) -> f64 {
    area_of(triangles(list))
}

/// Total unsigned triangle area of one draw in figure points squared, with the caveat of [`area_of`].
fn draw_area(list: &DrawList, draw: &Draw) -> f64 {
    area_of(triangles_in(list, draw))
}

/// The bounding box, in figure points, of every vertex referenced by `triangles`.
fn bounds_of(triangles: impl Iterator<Item = [Vertex; 3]>) -> egui::Rect {
    let mut rect = egui::Rect::NOTHING;
    for triangle in triangles {
        for v in triangle {
            rect.extend_with(egui::pos2(v.pos[0], v.pos[1]));
        }
    }
    rect
}

/// The bounding box of every vertex referenced by a triangle of a list.
fn bbox(list: &DrawList) -> egui::Rect {
    bounds_of(triangles(list))
}

/// The bounding box of every vertex referenced by a triangle of one draw.
fn draw_bbox(list: &DrawList, draw: &Draw) -> egui::Rect {
    bounds_of(triangles_in(list, draw))
}

/// The rectangle from `(x0, y0)` to `(x1, y1)` in figure points, as an expected bounding box.
fn bounds(x0: f32, y0: f32, x1: f32, y1: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
}

/// Asserts that every edge of `actual` lies within `tolerance` of the same edge of `expected`, naming `what` was
/// measured when one does not.
#[track_caller]
fn assert_rect_close(actual: egui::Rect, expected: egui::Rect, tolerance: f32, what: &str) {
    let ok = (actual.min.x - expected.min.x).abs() <= tolerance
        && (actual.min.y - expected.min.y).abs() <= tolerance
        && (actual.max.x - expected.max.x).abs() <= tolerance
        && (actual.max.y - expected.max.y).abs() <= tolerance;
    assert!(ok, "{what}: expected the bbox {expected:?}, got {actual:?}");
}

/// Whether any of `triangles` covers the figure-space point `p`. A triangle collapsed onto a line covers nothing,
/// even a point on that line.
fn covered(mut triangles: impl Iterator<Item = [Vertex; 3]>, p: Point) -> bool {
    triangles.any(|[a, b, c]| {
        let edge = |u: Vertex, v: Vertex| {
            (f64::from(v.pos[0]) - f64::from(u.pos[0])) * (p.y - f64::from(u.pos[1]))
                - (f64::from(v.pos[1]) - f64::from(u.pos[1])) * (p.x - f64::from(u.pos[0]))
        };
        let (d0, d1, d2) = (edge(a, b), edge(b, c), edge(c, a));
        if d0 == 0.0 && d1 == 0.0 && d2 == 0.0 {
            return false;
        }
        (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0) || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
    })
}

/// Whether any triangle of a list covers the figure-space point `p`.
fn ink_at(list: &DrawList, p: Point) -> bool {
    covered(triangles(list), p)
}

/// Whether any triangle of one draw covers the figure-space point `p`.
fn draw_ink_at(list: &DrawList, draw: &Draw, p: Point) -> bool {
    covered(triangles_in(list, draw), p)
}

/// The number of indices a triangle draw uses.
fn index_count(draw: &Draw) -> u32 {
    let range = triangles_of(draw);
    range.end - range.start
}

/// The vertices a triangle draw refers to, one per index in index order.
fn vertices_of_draw<'a>(list: &'a DrawList, draw: &Draw) -> Vec<&'a Vertex> {
    let range = triangles_of(draw);
    list.indices[range.start as usize..range.end as usize]
        .iter()
        .map(|&i| &list.vertices[i as usize])
        .collect()
}

/// The draw of `list` attributed to node `id`.
#[track_caller]
fn draw_of(list: &DrawList, id: u64) -> &Draw {
    list.draws
        .iter()
        .find(|draw| draw.source == Some(NodeId(id)))
        .unwrap_or_else(|| panic!("a draw is attributed to node {id}: {:?}", list.draws))
}

/// Whether every one of `vertices` lies within 1e-6 of `z`.
fn all_at_z(vertices: &[&Vertex], z: f32) -> bool {
    vertices.iter().all(|v| (v.z - z).abs() <= 1e-6)
}

/// The smallest and largest value of `pick` over `vertices`.
fn span(vertices: &[&Vertex], pick: impl Fn(&Vertex) -> f32) -> (f32, f32) {
    vertices
        .iter()
        .map(|&v| pick(v))
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        })
}

#[track_caller]
fn assert_span(actual: (f32, f32), expected: (f32, f32), what: &str) {
    assert!(
        (actual.0 - expected.0).abs() <= 1e-3 && (actual.1 - expected.1).abs() <= 1e-3,
        "{what}: expected the span {expected:?}, got {actual:?}"
    );
}

/// The premultiplied bytes egui makes of a straight-alpha colour, which is what the painter's blend state expects.
fn premultiplied(r: u8, g: u8, b: u8, a: u8) -> [u8; 4] {
    Color32::from_rgba_unmultiplied(r, g, b, a).to_array()
}

/// The premultiplied colour of [`premultiplied`] as the fractions a stroke's params carry.
fn premultiplied_fractions(r: u8, g: u8, b: u8, a: u8) -> [f32; 4] {
    premultiplied(r, g, b, a).map(|c| f32::from(c) / 255.0)
}

/// Asserts that every channel of `actual` lies within one byte step of `expected`.
#[track_caller]
fn assert_color_close(actual: [f32; 4], expected: [f32; 4], what: &str) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&got, want)| (got - want).abs() <= 1.0 / 255.0 + 1e-6),
        "{what}: expected the premultiplied fractions {expected:?}, got {actual:?}"
    );
}

/// The code of a cap in a stroke's params: 0 butt, 1 round, 2 square.
fn cap_code(cap: LineCap) -> u32 {
    match cap {
        LineCap::Butt => 0,
        LineCap::Round => 1,
        LineCap::Square => 2,
    }
}

/// The code of a join in a stroke's params: 0 miter (with a limit of 4), 1 round, 2 bevel.
fn join_code(join: LineJoin) -> u32 {
    match join {
        LineJoin::Miter => 0,
        LineJoin::Round => 1,
        LineJoin::Bevel => 2,
    }
}

/// What one segment of a stroke must hold: its four points and its arc lengths in item units, and whether a join
/// is drawn at `p0` and at `p1`. Its depths are the business of the depth-group tests.
#[derive(Clone, Copy, Debug)]
struct ExpectedSegment {
    prev: (f64, f64),
    p0: (f64, f64),
    p1: (f64, f64),
    next: (f64, f64),
    joins: (bool, bool),
    arc: (f64, f64),
}

/// The segment from `p0` to `p1` with the neighbours `prev` and `next`, a join at each end where `joins` says so
/// (and a cap otherwise), running from the first of `arc` to the second along its subpath.
fn segment(
    prev: (f64, f64),
    p0: (f64, f64),
    p1: (f64, f64),
    next: (f64, f64),
    joins: (bool, bool),
    arc: (f64, f64),
) -> ExpectedSegment {
    ExpectedSegment {
        prev,
        p0,
        p1,
        next,
        joins,
        arc,
    }
}

/// Asserts that `actual` holds exactly the segments of `expected`, in order, every point and arc length within
/// `tolerance` item units and the join bits as expected.
#[track_caller]
fn assert_segments(actual: &[Segment], expected: &[ExpectedSegment], tolerance: f64, what: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{what}: {} segments are expected, got {actual:#?}",
        expected.len()
    );
    for (k, (got, want)) in actual.iter().zip(expected).enumerate() {
        let points = [
            ("prev", got.prev, want.prev),
            ("p0", got.p0, want.p0),
            ("p1", got.p1, want.p1),
            ("next", got.next, want.next),
        ];
        for (name, point, (x, y)) in points {
            assert!(
                (f64::from(point[0]) - x).abs() <= tolerance
                    && (f64::from(point[1]) - y).abs() <= tolerance,
                "{what}: segment {k} has {name} = ({x}, {y}), got {point:?} in {got:?}"
            );
        }
        let joins = (got.flags & JOIN_AT_START != 0, got.flags & JOIN_AT_END != 0);
        assert_eq!(
            joins, want.joins,
            "{what}: segment {k} draws a join at (p0, p1) = {:?} and a cap at the other end: {got:?}",
            want.joins
        );
        assert!(
            (f64::from(got.arc[0]) - want.arc.0).abs() <= tolerance
                && (f64::from(got.arc[1]) - want.arc.1).abs() <= tolerance,
            "{what}: segment {k} runs from arc length {} to {}, got {:?} in {got:?}",
            want.arc.0,
            want.arc.1,
            got.arc
        );
    }
}

/// Asserts that the ends of `segments` lie at the depths of `expected`, segment by segment, within `tolerance`.
#[track_caller]
fn assert_z_pairs(segments: &[Segment], expected: &[[f64; 2]], tolerance: f64, what: &str) {
    let actual: Vec<[f32; 2]> = segments.iter().map(|s| s.z).collect();
    assert_eq!(
        actual.len(),
        expected.len(),
        "{what}: {} segments are expected, got {actual:?}",
        expected.len()
    );
    for (k, (got, want)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (f64::from(got[0]) - want[0]).abs() <= tolerance
                && (f64::from(got[1]) - want[1]).abs() <= tolerance,
            "{what}: the ends of segment {k} lie at z = {want:?}, got {got:?} (all: {actual:?})"
        );
    }
}

/// Asserts what every draw list must satisfy. A triangle draw has a non-empty index range within the index buffer,
/// the ranges of the triangle draws follow the paint order without overlap, and every one is whole triangles; a
/// stroke draw has a non-empty segment range, the ranges of the stroke draws partition the segments in paint order
/// (every segment is owned by exactly one draw), every params slot is owned by exactly one draw, a stroke draw
/// samples no texture, and it takes its depth from its segments (`vertex_z` set, `z` zero) exactly when it lies in
/// a depth group; every index names a vertex; every vertex, every segment and every params is finite; every z, of a
/// vertex, of a segment's ends and of a params, lies in [0, 1]; every segment has distinct ends, a neighbour equal
/// to the end it belongs to exactly when that end is capped, and no flag bit beyond the two join bits; every params
/// holds an even dash count of at most [`MAX_DASH_ENTRIES`] and, when dashed, a positive period, entries that are
/// not negative and a phase in [0, period), a width that is not negative and cap and join codes of at most 2; and
/// the draws of a depth group are contiguous with the groups ascending in paint order (the numbering need not start
/// at 0 or be consecutive: a group that yields no draws leaves a gap).
#[track_caller]
fn assert_well_formed(list: &DrawList) {
    let mut index_end = 0;
    let mut segment_end = 0;
    let mut owners_of_params: Vec<u32> = Vec::new();
    let mut previous: Option<u32> = None;
    let mut highest: Option<u32> = None;
    for (k, draw) in list.draws.iter().enumerate() {
        match &draw.kind {
            DrawKind::Triangles(range) => {
                assert!(
                    range.start >= index_end && range.end > range.start,
                    "triangle draw {k} is non-empty and follows the triangle draws before it without overlap: {:?}",
                    list.draws
                );
                assert!(
                    range.end as usize <= list.indices.len(),
                    "draw {k} lies within the {} indices: {draw:?}",
                    list.indices.len()
                );
                assert!(
                    (range.end - range.start).is_multiple_of(3),
                    "draw {k} is whole triangles: {draw:?}"
                );
                index_end = range.end;
            }
            DrawKind::Stroke { segments, params } => {
                assert!(
                    segments.start == segment_end && segments.end > segments.start,
                    "stroke draw {k} is non-empty and starts at the segment where the stroke draw before it ended \
                     ({segment_end}), so that every segment is owned by exactly one draw: {:?}",
                    list.draws
                );
                assert!(
                    segments.end as usize <= list.segments.len(),
                    "draw {k} lies within the {} segments: {draw:?}",
                    list.segments.len()
                );
                assert!(
                    (*params as usize) < list.stroke_params.len(),
                    "draw {k} names one of the {} stroke params: {draw:?}",
                    list.stroke_params.len()
                );
                assert_eq!(
                    draw.texture, None,
                    "a stroke draw samples no texture: {draw:?}"
                );
                let p = &list.stroke_params[*params as usize];
                assert_eq!(
                    p.vertex_z,
                    u32::from(draw.depth_group.is_some()),
                    "stroke draw {k} takes its depth from its segments exactly when it lies in a depth group \
                     ({:?}): {p:?}",
                    draw.depth_group
                );
                assert!(
                    p.vertex_z == 0 || p.z == 0.0,
                    "stroke draw {k}, inside a depth group, carries no depth in its params: {p:?}"
                );
                owners_of_params.push(*params);
                segment_end = segments.end;
            }
        }
        if let Some(group) = draw.depth_group
            && previous != Some(group)
        {
            assert!(
                highest.is_none_or(|h| group > h),
                "draw {k} starts depth group {group} after the groups up to {highest:?}, but the draws of a group \
                 are contiguous and the groups ascend in paint order: {:?}",
                list.draws.iter().map(|d| d.depth_group).collect::<Vec<_>>()
            );
            highest = Some(group);
        }
        previous = draw.depth_group;
    }
    assert_eq!(
        segment_end as usize,
        list.segments.len(),
        "the stroke draws own every one of the segments: {:?}",
        list.draws
    );
    owners_of_params.sort_unstable();
    assert!(
        owners_of_params
            .iter()
            .copied()
            .eq(0..list.stroke_params.len() as u32),
        "every one of the {} params slots is owned by exactly one stroke draw, but the draws name the slots \
         {owners_of_params:?}",
        list.stroke_params.len()
    );
    assert!(
        list.indices
            .iter()
            .all(|&i| (i as usize) < list.vertices.len()),
        "every index names one of the {} vertices",
        list.vertices.len()
    );
    for v in &list.vertices {
        assert!(
            v.pos[0].is_finite()
                && v.pos[1].is_finite()
                && v.z.is_finite()
                && v.uv[0].is_finite()
                && v.uv[1].is_finite(),
            "every vertex is finite: {v:?}"
        );
        assert!((0.0..=1.0).contains(&v.z), "every z lies in [0, 1]: {v:?}");
    }
    for (k, s) in list.segments.iter().enumerate() {
        assert!(
            [s.prev, s.p0, s.p1, s.next, s.z, s.arc, s.grad]
                .iter()
                .flatten()
                .all(|v| v.is_finite()),
            "segment {k} is finite: {s:?}"
        );
        assert!(
            s.z.iter().all(|z| (0.0..=1.0).contains(z)),
            "the z at both ends of segment {k} lies in [0, 1]: {s:?}"
        );
        assert_ne!(
            s.p0, s.p1,
            "segment {k} has distinct ends, so that it has a direction to expand along: {s:?}"
        );
        assert!(
            s.flags <= JOIN_AT_START | JOIN_AT_END,
            "segment {k} sets no flag bit beyond the two join bits: {s:?}"
        );
        assert_eq!(
            s.prev == s.p0,
            s.flags & JOIN_AT_START == 0,
            "segment {k} has prev equal to p0 exactly when p0 is capped rather than joined: {s:?}"
        );
        assert_eq!(
            s.next == s.p1,
            s.flags & JOIN_AT_END == 0,
            "segment {k} has next equal to p1 exactly when p1 is capped rather than joined: {s:?}"
        );
    }
    for (k, p) in list.stroke_params.iter().enumerate() {
        let count = p.dash_count as usize;
        assert!(
            p.dash_count.is_multiple_of(2) && count <= MAX_DASH_ENTRIES,
            "params {k} hold an even dash count of at most {MAX_DASH_ENTRIES}: {p:?}"
        );
        if count > 0 {
            assert!(
                p.period > 0.0 && p.dashes[..count].iter().all(|&d| d >= 0.0),
                "dashed params {k} hold a positive period and no negative entry: {p:?}"
            );
            assert!(
                (0.0..p.period).contains(&p.dash_offset),
                "dashed params {k} hold a phase reduced into [0, period): {p:?}"
            );
        }
        assert!(
            p.linear
                .iter()
                .chain(&p.offset)
                .chain(&p.color)
                .chain([&p.width, &p.dash_offset, &p.period, &p.z])
                .chain(&p.dashes[..count])
                .all(|v| v.is_finite()),
            "params {k} are finite: {p:?}"
        );
        assert!(
            (0.0..=1.0).contains(&p.z),
            "the z of params {k} lies in [0, 1]: {p:?}"
        );
        assert!(
            p.width >= 0.0,
            "params {k} hold a width that is not negative: {p:?}"
        );
        assert!(
            p.cap <= 2 && p.join <= 2,
            "params {k} code the cap and the join as 0, 1 or 2: {p:?}"
        );
    }
}

/// Asserts that no draw of `list` lies in a depth group and that every vertex and every segment end lies at z = 0,
/// as for a figure without a three-dimensional axes.
#[track_caller]
fn assert_outside_depth_groups(list: &DrawList) {
    assert!(
        list.draws.iter().all(|draw| draw.depth_group.is_none()),
        "no draw lies in a depth group: {:?}",
        list.draws
    );
    assert!(
        list.vertices.iter().all(|v| v.z == 0.0),
        "every vertex outside a depth group lies at z = 0: {:?}",
        list.vertices.iter().map(|v| v.z).collect::<Vec<_>>()
    );
    assert!(
        list.segments.iter().all(|s| s.z == [0.0, 0.0]),
        "every segment end outside a depth group lies at z = 0: {:?}",
        list.segments.iter().map(|s| s.z).collect::<Vec<_>>()
    );
    assert!(
        list.segments.iter().all(|s| s.grad == [0.0, 0.0]),
        "no segment outside a depth group carries a gradient of z: {:?}",
        list.segments.iter().map(|s| s.grad).collect::<Vec<_>>()
    );
}

/// Asserts that `draw` is one textured quad for the item-space rectangle `rect` beneath `transform`, sampling the
/// tile `key`: two triangles indexed `[0, 1, 2, 2, 1, 3]` over four white vertices at the figure-space positions of
/// the rectangle's corners, carrying the texture coordinates (0, 0), (1, 0), (0, 1) and (1, 1) at its top-left,
/// top-right, bottom-left and bottom-right corners, and covering the whole quad.
#[track_caller]
fn assert_textured_quad(
    list: &DrawList,
    draw: &Draw,
    transform: Transform,
    rect: Rect,
    key: &TileKey,
) {
    let texture = draw
        .texture
        .as_ref()
        .unwrap_or_else(|| panic!("the draw samples a tile: {draw:?}"));
    assert_eq!(
        texture, key,
        "the draw names the tile of the item's samples"
    );
    assert!(
        Arc::ptr_eq(&texture.samples, &key.samples),
        "the key shares the item's sample buffer rather than copying it, because the painter's tile cache keys by \
         the buffer's address"
    );
    let range = triangles_of(draw);
    let indices = &list.indices[range.start as usize..range.end as usize];
    let base = *indices.iter().min().expect("the draw has indices");
    let relative: Vec<u32> = indices.iter().map(|i| i - base).collect();
    assert_eq!(
        relative,
        [0, 1, 2, 2, 1, 3],
        "one quad is two triangles over four vertices: {indices:?}"
    );
    let corners = [
        ((0.0, 0.0), (rect.x, rect.y)),
        ((1.0, 0.0), (rect.right(), rect.y)),
        ((0.0, 1.0), (rect.x, rect.bottom())),
        ((1.0, 1.0), (rect.right(), rect.bottom())),
    ];
    for (k, ((u, v), (x, y))) in corners.into_iter().enumerate() {
        let vertex = list.vertices[base as usize + k];
        let expected = transform.apply(Point::new(x, y));
        assert!(
            (vertex.uv[0] - u).abs() <= 1e-6 && (vertex.uv[1] - v).abs() <= 1e-6,
            "vertex {k} of the quad carries the texture coordinate ({u}, {v}): {vertex:?}"
        );
        assert!(
            (f64::from(vertex.pos[0]) - expected.x).abs() <= 1e-3
                && (f64::from(vertex.pos[1]) - expected.y).abs() <= 1e-3,
            "vertex {k}, with texture coordinate ({u}, {v}), lies at {expected:?}: {vertex:?}"
        );
        assert_eq!(
            vertex.color,
            [255, 255, 255, 255],
            "the texture is drawn unmodulated: {vertex:?}"
        );
    }
    let parallelogram =
        (transform.a * transform.d - transform.b * transform.c).abs() * rect.width * rect.height;
    assert_close(
        draw_area(list, draw),
        parallelogram,
        1e-2,
        "the two triangles cover the quad",
    );
    let centre = transform.apply(Point::new(
        rect.x + rect.width / 2.0,
        rect.y + rect.height / 2.0,
    ));
    assert!(
        draw_ink_at(list, draw, centre),
        "the centre of the quad is covered"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------------------------------------------

// Why: the vertices of a list are in figure points and the mapping to the screen is a uniform of the painter, so a
// tessellation that scaled or moved the geometry (as the mesh path did) would draw every figure displaced or
// magnified twice over; and a draw must name the node that produced it, or picking could not attribute what is
// drawn.
#[test]
fn a_filled_rectangle_covers_its_area_at_its_position_in_figure_points() {
    let drawn = draw_list(&list(vec![sourced(
        filled(
            rect_segments(30.0, 40.0, 50.0, 20.0),
            Rgba::BLACK,
            FillRule::NonZero,
        ),
        7,
    )]));

    assert_eq!(
        drawn.draws.len(),
        1,
        "one path is one draw: {:?}",
        drawn.draws
    );
    let draw = &drawn.draws[0];
    assert_close(
        draw_area(&drawn, draw),
        1000.0,
        1e-3,
        "area in figure points squared",
    );
    assert_rect_close(
        draw_bbox(&drawn, draw),
        bounds(30.0, 40.0, 80.0, 60.0),
        1e-3,
        "the rectangle lies at its position in figure points",
    );
    assert_eq!(
        draw.source,
        Some(NodeId(7)),
        "the draw names the item's node: {draw:?}"
    );
    assert_eq!(
        draw.texture, None,
        "solid geometry samples no texture: {draw:?}"
    );
    assert_eq!(
        draw.clip, None,
        "an unclipped leaf records no clip: {draw:?}"
    );
    assert_outside_depth_groups(&drawn);
}

// Why: holes in filled regions (contour bands around a peak, glyph counters) depend on the fill rule; with even-odd a
// nested contour is a hole, with non-zero and the same winding it is not.
#[test]
fn the_fill_rule_decides_whether_a_nested_contour_is_a_hole() {
    let mut segments = rect_segments(0.0, 0.0, 100.0, 100.0);
    segments.extend(rect_segments(25.0, 25.0, 50.0, 50.0));

    for (rule, expected, what) in [
        (FillRule::EvenOdd, 7500.0, "even-odd leaves a hole"),
        (
            FillRule::NonZero,
            10_000.0,
            "non-zero with equal winding fills the hole",
        ),
    ] {
        let drawn = draw_list(&list(vec![filled(segments.clone(), Rgba::BLACK, rule)]));
        assert_close(area(&drawn), expected, 1e-2, what);
    }
}

// Why: rotated y-axis labels are drawn as groups with a rotation; if the group transform were ignored or applied in
// the wrong order, labels would appear horizontal or in the wrong place.
#[test]
fn a_group_rotation_rotates_its_items_before_translating_them() {
    let drawn = draw_list(&list(vec![group(
        None,
        Some(Transform::rotate(90.0).then(Transform::translate(100.0, 100.0))),
        vec![filled(
            rect_segments(0.0, 0.0, 40.0, 10.0),
            Rgba::BLACK,
            FillRule::NonZero,
        )],
    )]));

    // The rotation maps (x, y) to (−y, x), so the 40 × 10 rectangle becomes x ∈ [−10, 0], y ∈ [0, 40] before the
    // translation to (100, 100).
    assert_rect_close(
        bbox(&drawn),
        bounds(90.0, 100.0, 100.0, 140.0),
        1e-3,
        "the rectangle is rotated and then translated",
    );
    assert_close(area(&drawn), 400.0, 0.1, "rotation preserves area");
}

// Why: data outside the axes limits must not paint over tick labels and neighbouring subplots, and it is the
// painter's scissor, set from the draw's clip in figure points, that cuts it: the tessellation keeps the geometry
// whole, so that a pan within the plot box changes the mapping and nothing else. A draw that lost its clip would
// paint over its neighbours, a clip in other units or without the enclosing clips intersected would cut the wrong
// region, and geometry cut at tessellation would have to be rebuilt for every gesture.
#[test]
fn a_clipped_leaf_records_its_effective_clip_and_keeps_its_geometry_whole() {
    let square = || {
        filled(
            rect_segments(0.0, 0.0, 100.0, 100.0),
            Rgba::BLACK,
            FillRule::NonZero,
        )
    };
    let half = Rect::new(50.0, 0.0, 100.0, 100.0);
    let corner = Rect::new(0.0, 0.0, 80.0, 80.0);
    let whole = bounds(0.0, 0.0, 100.0, 100.0);
    // The outer group's clip and transform, the inner group's clip, the clip expected on the draw, and the area and
    // bounds of the square in figure points.
    let cases = [
        (None, None, Some(half), half, 10_000.0, whole),
        (
            Some(corner),
            None,
            Some(half),
            Rect::new(50.0, 0.0, 30.0, 80.0),
            10_000.0,
            whole,
        ),
        (Some(corner), None, None, corner, 10_000.0, whole),
        // The inner clip is expressed in the outer group's space, which the outer transform doubles.
        (
            None,
            Some(scale_then_translate(2.0, 2.0, 0.0, 0.0)),
            Some(Rect::new(10.0, 10.0, 20.0, 20.0)),
            Rect::new(20.0, 20.0, 40.0, 40.0),
            40_000.0,
            bounds(0.0, 0.0, 200.0, 200.0),
        ),
    ];
    for (outer_clip, outer_transform, inner_clip, expected_clip, expected_area, expected_bounds) in
        cases
    {
        let drawn = draw_list(&list(vec![group(
            outer_clip,
            outer_transform,
            vec![group(inner_clip, None, vec![square()])],
        )]));

        assert_eq!(
            drawn.draws.len(),
            1,
            "the square is one draw: {:?}",
            drawn.draws
        );
        let draw = &drawn.draws[0];
        assert_eq!(
            draw.clip,
            Some(expected_clip),
            "the draw records the effective clip in figure points: {draw:?}"
        );
        assert_close(
            draw_area(&drawn, draw),
            expected_area,
            1e-2,
            "the square is tessellated whole, clip or no clip",
        );
        assert_rect_close(
            draw_bbox(&drawn, draw),
            expected_bounds,
            1e-3,
            "the square keeps its whole extent, clip or no clip",
        );
    }
}

// Why: a leaf whose clip has no area, or whose transform collapses it, has nothing to show; the display-list contract
// has a backend skip it rather than hand the painter a scissor or a quad of no size, and the items around it must
// still draw.
#[test]
fn a_leaf_beneath_an_empty_clip_or_a_degenerate_transform_draws_nothing() {
    let square = || {
        filled(
            rect_segments(0.0, 0.0, 10.0, 10.0),
            Rgba::BLACK,
            FillRule::NonZero,
        )
    };
    let flat = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 0.0,
        e: 0.0,
        f: 0.0,
    };
    let cases = [
        (
            "a clip of zero width",
            group(Some(Rect::new(0.0, 0.0, 0.0, 10.0)), None, vec![square()]),
        ),
        (
            "nested clips that do not overlap",
            group(
                Some(Rect::new(0.0, 0.0, 50.0, 50.0)),
                None,
                vec![group(
                    Some(Rect::new(60.0, 0.0, 50.0, 50.0)),
                    None,
                    vec![square()],
                )],
            ),
        ),
        (
            "a non-finite clip",
            group(
                Some(Rect::new(f64::NAN, 0.0, 10.0, 10.0)),
                None,
                vec![square()],
            ),
        ),
        (
            "a transform that collapses the plane onto a line",
            group(None, Some(flat), vec![square()]),
        ),
    ];
    for (what, item) in cases {
        let drawn = draw_list(&list(vec![
            item,
            filled(
                rect_segments(300.0, 250.0, 20.0, 10.0),
                Rgba::BLACK,
                FillRule::NonZero,
            ),
        ]));

        assert_eq!(
            drawn.draws.len(),
            1,
            "beneath {what} the leaf draws nothing and only the rectangle after it draws: {:?}",
            drawn.draws
        );
        assert!(
            ink_at(&drawn, Point::new(310.0, 255.0)),
            "beneath {what} the rectangle after the leaf still draws"
        );
    }
}

// Why: the painter blends with egui's premultiplied blend state; passing straight alpha would draw translucent fills
// (legend boxes, alpha surfaces) too bright, and premultiplying differently from egui would change the pixels of
// every translucent item that the mesh path used to draw. A stroke's params carry the colour as fractions for the
// stroke pipeline, which blends the same way, so a translucent line must premultiply exactly as the fill beside it
// does, or the two would differ in brightness.
#[test]
fn colours_are_premultiplied_as_egui_premultiplies_them_for_fills_and_strokes_alike() {
    for (color, expected) in [
        (Rgba::new(1.0, 0.0, 0.0, 0.5), premultiplied(255, 0, 0, 128)),
        (
            Rgba::new(0.2, 0.4, 0.6, 0.4),
            premultiplied(51, 102, 153, 102),
        ),
        (Rgba::new(0.0, 0.0, 1.0, 1.0), [0, 0, 255, 255]),
    ] {
        let drawn = draw_list(&list(vec![
            filled(
                rect_segments(0.0, 0.0, 10.0, 10.0),
                color,
                FillRule::NonZero,
            ),
            stroked(
                line_segments(),
                color,
                4.0,
                Vec::new(),
                0.0,
                LineCap::Butt,
                LineJoin::Miter,
            ),
        ]));

        assert_eq!(
            drawn.draws.len(),
            2,
            "a fill and a stroke: {:?}",
            drawn.draws
        );
        let vertices = vertices_of_draw(&drawn, &drawn.draws[0]);
        assert!(!vertices.is_empty());
        for v in vertices {
            assert!(
                v.color
                    .iter()
                    .zip(expected)
                    .all(|(&got, want)| got.abs_diff(want) <= 1),
                "expected premultiplied {expected:?} for {color:?}, got {:?}",
                v.color
            );
        }
        assert_color_close(
            params_of(&drawn, &drawn.draws[1]).color,
            expected.map(|c| f32::from(c) / 255.0),
            &format!("the stroke's params carry the same premultiplied colour for {color:?}"),
        );
    }
}

// Why: the display list is ordered back to front (for example 3D faces after depth sorting); the draws must keep that
// order, starting with the first item because the background is not a draw, or the painter would disagree with the
// PDF about what is in front.
#[test]
fn draws_follow_the_paint_order_of_the_leaves() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    let drawn = draw_list(&with_background(
        Rgba::WHITE,
        vec![
            sourced(
                filled(rect_segments(0.0, 0.0, 50.0, 50.0), red, FillRule::NonZero),
                1,
            ),
            sourced(
                filled(
                    rect_segments(25.0, 25.0, 50.0, 50.0),
                    blue,
                    FillRule::NonZero,
                ),
                2,
            ),
            sourced(
                filled(
                    rect_segments(40.0, 40.0, 10.0, 10.0),
                    red,
                    FillRule::NonZero,
                ),
                3,
            ),
        ],
    ));

    let colours: Vec<[u8; 4]> = drawn
        .draws
        .iter()
        .map(|draw| vertices_of_draw(&drawn, draw)[0].color)
        .collect();
    assert_eq!(
        colours,
        [[255, 0, 0, 255], [0, 0, 255, 255], [255, 0, 0, 255]],
        "one draw per leaf, in item order, with no draw for the background"
    );
    let sources: Vec<Option<NodeId>> = drawn.draws.iter().map(|draw| draw.source).collect();
    assert_eq!(
        sources,
        [Some(NodeId(1)), Some(NodeId(2)), Some(NodeId(3))],
        "each draw names its leaf's node"
    );
    for draw in &drawn.draws {
        let vertices = vertices_of_draw(&drawn, draw);
        assert!(
            vertices.iter().all(|v| v.color == vertices[0].color),
            "a draw holds one leaf's colour: {vertices:?}"
        );
    }
}

// Why: display lists can be built by hand, and the display-list contract requires backends to skip invalid items
// rather than panic. Non-finite coordinates in particular must never reach the GPU, and one bad item must not stop
// the rest of the figure from drawing.
#[test]
fn invalid_items_are_skipped_without_panicking_and_valid_items_still_draw() {
    let along = || polyline(&[(0.0, 200.0), (100.0, 200.0)]);
    let bad_stroke = |color: Rgba, width: f64, dash: Vec<f64>| {
        stroked(
            along(),
            color,
            width,
            dash,
            0.0,
            LineCap::Butt,
            LineJoin::Miter,
        )
    };
    let nan = f64::NAN;
    let nan_colour = Rgba::new(f32::NAN, 0.0, 0.0, 1.0);
    // Every item is attributed to a node, so that the draws name the survivors: a non-finite coordinate, a path
    // that does not start with a `MoveTo`, a non-finite control point, a fill and a stroke with a non-finite colour,
    // a non-finite and a negative width, a dash pattern that is all zero and one that is not finite, a non-finite
    // group transform, a glyph run with a non-finite size and one with a non-finite colour, and then the valid
    // rectangle.
    let items = vec![
        sourced(
            filled(
                rect_segments(nan, 0.0, 10.0, 10.0),
                Rgba::BLACK,
                FillRule::NonZero,
            ),
            1,
        ),
        sourced(
            filled(
                vec![line_to(0.0, 0.0), line_to(10.0, 0.0), line_to(10.0, 10.0)],
                Rgba::BLACK,
                FillRule::NonZero,
            ),
            2,
        ),
        sourced(
            filled(
                vec![
                    move_to(0.0, 0.0),
                    PathSegment::CubicTo(
                        Point::new(nan, 0.0),
                        Point::new(10.0, 10.0),
                        Point::new(10.0, 0.0),
                    ),
                    PathSegment::Close,
                ],
                Rgba::BLACK,
                FillRule::NonZero,
            ),
            3,
        ),
        sourced(
            filled(
                rect_segments(0.0, 0.0, 10.0, 10.0),
                nan_colour,
                FillRule::NonZero,
            ),
            4,
        ),
        sourced(bad_stroke(nan_colour, 1.0, vec![]), 5),
        sourced(bad_stroke(Rgba::BLACK, f64::INFINITY, vec![]), 6),
        sourced(bad_stroke(Rgba::BLACK, -1.0, vec![]), 7),
        sourced(bad_stroke(Rgba::BLACK, 1.0, vec![0.0, 0.0]), 8),
        sourced(bad_stroke(Rgba::BLACK, 1.0, vec![nan, 2.0]), 9),
        group(
            None,
            Some(Transform {
                a: nan,
                ..Transform::IDENTITY
            }),
            vec![sourced(
                filled(
                    rect_segments(0.0, 0.0, 10.0, 10.0),
                    Rgba::BLACK,
                    FillRule::NonZero,
                ),
                10,
            )],
        ),
        with_run(
            sourced(glyph_h(Point::new(0.0, 0.0), 10.0, Rgba::BLACK), 11),
            |run| run.size_pt = nan,
        ),
        with_run(
            sourced(glyph_h(Point::new(0.0, 0.0), 10.0, Rgba::BLACK), 12),
            |run| run.color = nan_colour,
        ),
        sourced(
            filled(
                rect_segments(300.0, 250.0, 20.0, 10.0),
                Rgba::BLACK,
                FillRule::NonZero,
            ),
            13,
        ),
    ];

    let drawn = draw_list(&list(items));

    assert_eq!(
        drawn.draws.iter().map(|d| d.source).collect::<Vec<_>>(),
        [Some(NodeId(13))],
        "only the valid rectangle after the invalid items is drawn"
    );
    assert!(
        ink_at(&drawn, Point::new(310.0, 255.0)),
        "the valid rectangle after the invalid items is drawn"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Strokes
// ---------------------------------------------------------------------------------------------------------------

// Why: the stroke pipeline expands every segment on the GPU from its own instance, so the canvas must hand it, for
// each edge of the polyline, the edge's ends, the vertices either side of them (from which the vertex shader
// builds the joins), which ends get a join and which a cap, and the arc length at each end (from which the
// fragment shader dashes). A neighbour that is not the real vertex, a join bit at a free end or an arc length that
// is not the cumulative item-space length would draw a spike, a cap where a join belongs, or dashes that do not
// line up from one edge to the next.
#[test]
fn an_open_polyline_is_one_segment_per_edge_with_caps_at_its_ends_and_joins_between() {
    let drawn = draw_list(&list(vec![solid(polyline(&[
        (0.0, 0.0),
        (30.0, 0.0),
        (30.0, 40.0),
        (60.0, 80.0),
    ]))]));

    let draw = only_stroke(&drawn);
    // The edges are 30, 40 and 50 long, so the arc lengths run 0, 30, 70, 120: the diagonal edge measures its own
    // length, not the sum of its projections.
    assert_segments(
        segments_of(&drawn, draw),
        &[
            segment(
                (0.0, 0.0),
                (0.0, 0.0),
                (30.0, 0.0),
                (30.0, 40.0),
                (false, true),
                (0.0, 30.0),
            ),
            segment(
                (0.0, 0.0),
                (30.0, 0.0),
                (30.0, 40.0),
                (60.0, 80.0),
                (true, true),
                (30.0, 70.0),
            ),
            segment(
                (30.0, 0.0),
                (30.0, 40.0),
                (60.0, 80.0),
                (60.0, 80.0),
                (true, false),
                (70.0, 120.0),
            ),
        ],
        1e-4,
        "an open polyline of three edges",
    );
    assert_eq!(draw.texture, None, "a stroke samples no texture: {draw:?}");
    assert_eq!(
        draw.clip, None,
        "an unclipped stroke records no clip: {draw:?}"
    );
    assert_outside_depth_groups(&drawn);
}

// Why: a closed outline (a box, a marker, the edge of a face) has no free end: the edge back to the start is a
// segment of its own and the join at the start point joins the last edge to the first, so the neighbour before the
// first segment is the last point and the neighbour after the last is the first. A closed path drawn with caps at
// its start, or without its closing edge, would show a notch at the seam of every marker.
#[test]
fn a_closed_polyline_has_a_closing_segment_and_joins_that_wrap_around_its_start() {
    let drawn = draw_list(&list(vec![solid(closed_polyline(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
        (0.0, 10.0),
    ]))]));

    assert_segments(
        segments_of(&drawn, only_stroke(&drawn)),
        &[
            segment(
                (0.0, 10.0),
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (true, true),
                (0.0, 10.0),
            ),
            segment(
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (true, true),
                (10.0, 20.0),
            ),
            segment(
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (0.0, 0.0),
                (true, true),
                (20.0, 30.0),
            ),
            segment(
                (10.0, 10.0),
                (0.0, 10.0),
                (0.0, 0.0),
                (10.0, 0.0),
                (true, true),
                (30.0, 40.0),
            ),
        ],
        1e-4,
        "a closed square",
    );
}

// Why: the joint at the seam of a closed subpath is the start of its first segment, at arc length 0, and the end of
// its closing segment, at the perimeter, and the two measure the dash pattern differently unless the perimeter is a
// multiple of its period; the shader decides whether the seam lies in a dash on both sides, and places the caps of
// the closing segment's dashes, by the arc length of the segment before the joint, so every segment carries that
// length: the perimeter for the first segment of a closed subpath, and its own start otherwise (on open subpaths
// too). A first segment that carried 0 would draw the seam of a dashed marker outline with the phase of its start
// rather than of its end: a join where a gap belongs, or none where a dash runs through.
#[test]
fn the_first_segment_of_a_closed_subpath_measures_its_start_at_the_perimeter_and_every_other_at_its_own_arc()
 {
    let triangle = closed_polyline(&[(0.0, 0.0), (30.0, 0.0), (30.0, 40.0)]);
    let open = polyline(&[(100.0, 0.0), (130.0, 0.0), (130.0, 40.0)]);
    // The path, and the arc length expected at the start of each segment as the segment before it measures it.
    let cases: [(&str, Vec<PathSegment>, Vec<f64>); 4] = [
        (
            "a closed square of perimeter 40",
            rect_segments(0.0, 0.0, 10.0, 10.0),
            vec![40.0, 10.0, 20.0, 30.0],
        ),
        (
            "a closed triangle of perimeter 120",
            triangle.clone(),
            vec![120.0, 30.0, 70.0],
        ),
        (
            "an open polyline of two edges",
            open.clone(),
            vec![0.0, 30.0],
        ),
        (
            "the closed triangle and then the open polyline in one path",
            [triangle, open].concat(),
            vec![120.0, 30.0, 70.0, 0.0, 30.0],
        ),
    ];
    for (what, segments, expected) in cases {
        let drawn = draw_list(&list(vec![solid(segments)]));
        let segments = segments_of(&drawn, only_stroke(&drawn));
        let actual: Vec<f32> = segments.iter().map(|s| s.prev_arc).collect();
        assert!(
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(&expected)
                    .all(|(&got, &want)| (f64::from(got) - want).abs() <= 1e-4),
            "{what}: the segments carry the arc lengths {expected:?} at their starts as the segments before them \
             measure them, got {actual:?} in {segments:#?}"
        );
    }

    // A flattened closed curve is a closed subpath of many segments; only its first carries the perimeter.
    let drawn = draw_list(&list(vec![solid(circle_segments(50.0, 50.0, 20.0))]));
    let chords = segments_of(&drawn, only_stroke(&drawn));
    let perimeter = chords[chords.len() - 1].arc[1];
    assert_eq!(
        chords[0].prev_arc, perimeter,
        "the first chord of the circle measures its start at the perimeter, the end of the last chord: {:?}",
        chords[0]
    );
    for (k, chord) in chords.iter().enumerate().skip(1) {
        assert_eq!(
            chord.prev_arc, chord.arc[0],
            "chord {k} of the circle measures its start at its own arc length: {chord:?}"
        );
    }
}

// Why: the dash phase restarts at the start of every subpath and a subpath's ends are capped, as in PDF, so the arc
// length must restart at 0 with every subpath and no join may cross the gap between two, or the markers of a
// scatter drawn as one path of many subpaths would be linked by spikes and dashed unevenly; and a segment after
// `Close` without a `MoveTo` starts a new subpath at the closed one's start, as PDF has it.
#[test]
fn every_subpath_restarts_the_arc_length_and_no_join_crosses_the_gap_between_subpaths() {
    let cases = [
        (
            "two subpaths each begun by a MoveTo",
            vec![
                move_to(0.0, 0.0),
                line_to(30.0, 0.0),
                line_to(30.0, 40.0),
                move_to(100.0, 0.0),
                line_to(130.0, 0.0),
            ],
            vec![
                segment(
                    (0.0, 0.0),
                    (0.0, 0.0),
                    (30.0, 0.0),
                    (30.0, 40.0),
                    (false, true),
                    (0.0, 30.0),
                ),
                segment(
                    (0.0, 0.0),
                    (30.0, 0.0),
                    (30.0, 40.0),
                    (30.0, 40.0),
                    (true, false),
                    (30.0, 70.0),
                ),
                segment(
                    (100.0, 0.0),
                    (100.0, 0.0),
                    (130.0, 0.0),
                    (130.0, 0.0),
                    (false, false),
                    (0.0, 30.0),
                ),
            ],
        ),
        (
            "a closed triangle and then a segment without a MoveTo, which starts at the triangle's start",
            vec![
                move_to(0.0, 0.0),
                line_to(30.0, 0.0),
                line_to(30.0, 40.0),
                PathSegment::Close,
                line_to(60.0, 80.0),
            ],
            vec![
                segment(
                    (30.0, 40.0),
                    (0.0, 0.0),
                    (30.0, 0.0),
                    (30.0, 40.0),
                    (true, true),
                    (0.0, 30.0),
                ),
                segment(
                    (0.0, 0.0),
                    (30.0, 0.0),
                    (30.0, 40.0),
                    (0.0, 0.0),
                    (true, true),
                    (30.0, 70.0),
                ),
                segment(
                    (30.0, 0.0),
                    (30.0, 40.0),
                    (0.0, 0.0),
                    (30.0, 0.0),
                    (true, true),
                    (70.0, 120.0),
                ),
                segment(
                    (0.0, 0.0),
                    (0.0, 0.0),
                    (60.0, 80.0),
                    (60.0, 80.0),
                    (false, false),
                    (0.0, 100.0),
                ),
            ],
        ),
    ];
    for (what, segments, expected) in cases {
        let drawn = draw_list(&list(vec![solid(segments)]));
        assert_segments(
            segments_of(&drawn, only_stroke(&drawn)),
            &expected,
            1e-4,
            what,
        );
    }
}

// Why: a curve reaches the shader as the chords of its flattening, so the ends of every chord must lie on the curve
// and the chords must be short enough that no gap between chord and curve shows at the resolution the list is
// built for, which takes more chords when the canvas is zoomed in; the chords must chain around a closed curve
// with a join at each of their ends, or a marker outline would show notches, and their arc lengths must accumulate
// the chord lengths, or the dashes of a curved line would drift along it.
#[test]
fn a_stroked_circle_is_flattened_into_chords_on_the_circle_more_finely_at_a_higher_scale() {
    let (cx, cy, r) = (50.0, 50.0, 20.0);
    let circle = list(vec![solid(circle_segments(cx, cy, r))]);
    // The four cubic Béziers of `circle_segments` lie within 0.03 % of the radius of the true circle, which at this
    // radius is under a hundredth of a point.
    let bezier_error = 0.01;
    let radial = |p: [f32; 2]| ((f64::from(p[0]) - cx).hypot(f64::from(p[1]) - cy) - r).abs();
    let mut counts = Vec::new();
    for scale in [1.0, 10.0] {
        let drawn = draw_list_at(&circle, at_scale(scale));
        let chords = segments_of(&drawn, only_stroke(&drawn));
        let tolerance = SCREEN_TOLERANCE / f64::from(scale);
        assert!(
            chords.len() > 4,
            "at scale {scale} the circle is flattened into more chords than its four cubics: {}",
            chords.len()
        );
        for (k, chord) in chords.iter().enumerate() {
            assert!(
                radial(chord.p0) <= tolerance + bezier_error
                    && radial(chord.p1) <= tolerance + bezier_error,
                "at scale {scale} the ends of chord {k} lie on the circle: {chord:?}"
            );
            let middle = [
                (chord.p0[0] + chord.p1[0]) / 2.0,
                (chord.p0[1] + chord.p1[1]) / 2.0,
            ];
            assert!(
                radial(middle) <= 1.5 * tolerance + bezier_error,
                "at scale {scale} the middle of chord {k} lies within the flattening tolerance of {tolerance} item \
                 units of the circle: {chord:?}"
            );
            assert_eq!(
                chord.flags,
                JOIN_AT_START | JOIN_AT_END,
                "at scale {scale} chord {k} of the closed circle is joined at both ends: {chord:?}"
            );
            let length =
                f64::from(chord.p1[0] - chord.p0[0]).hypot(f64::from(chord.p1[1] - chord.p0[1]));
            assert_close(
                f64::from(chord.arc[1] - chord.arc[0]),
                length,
                1e-3,
                &format!("at scale {scale} the arc length across chord {k} is its length"),
            );
            let following = &chords[(k + 1) % chords.len()];
            assert!(
                chord.p1 == following.p0
                    && chord.next == following.p1
                    && following.prev == chord.p0,
                "at scale {scale} chord {k} chains into the chord after it, around to the first: {chord:?} then \
                 {following:?}"
            );
        }
        assert_eq!(
            chords[0].arc[0], 0.0,
            "the arc length starts at 0: {:?}",
            chords[0]
        );
        for pair in chords.windows(2) {
            assert_eq!(
                pair[0].arc[1], pair[1].arc[0],
                "the arc length accumulates from chord to chord: {pair:?}"
            );
        }
        let circumference = 2.0 * std::f64::consts::PI * r;
        assert_close(
            f64::from(chords[chords.len() - 1].arc[1]),
            circumference,
            0.01 * circumference,
            &format!(
                "at scale {scale} the arc length at the end of the last chord is the circumference"
            ),
        );
        counts.push(chords.len());
    }
    assert!(
        counts[1] > counts[0],
        "the circle is flattened more finely at scale 10 ({} chords) than at scale 1 ({} chords)",
        counts[1],
        counts[0]
    );
}

// Why: a zero-length edge has no direction, so the shader could not expand it (its normal is undefined) and would
// draw a spike or nothing at its joins; polylines from data repeat points (the risers of a step plot, an outline
// that returns to its start before closing), so the canvas merges consecutive coincident points and drops the
// duplicate start of a closed subpath rather than emitting an edge of no length with a join at both ends.
#[test]
fn consecutive_coincident_points_are_merged_and_a_closed_subpath_drops_a_last_point_equal_to_its_first()
 {
    let open = vec![
        segment(
            (0.0, 0.0),
            (0.0, 0.0),
            (30.0, 0.0),
            (30.0, 40.0),
            (false, true),
            (0.0, 30.0),
        ),
        segment(
            (0.0, 0.0),
            (30.0, 0.0),
            (30.0, 40.0),
            (30.0, 40.0),
            (true, false),
            (30.0, 70.0),
        ),
    ];
    let closed = vec![
        segment(
            (30.0, 40.0),
            (0.0, 0.0),
            (30.0, 0.0),
            (30.0, 40.0),
            (true, true),
            (0.0, 30.0),
        ),
        segment(
            (0.0, 0.0),
            (30.0, 0.0),
            (30.0, 40.0),
            (0.0, 0.0),
            (true, true),
            (30.0, 70.0),
        ),
        segment(
            (30.0, 0.0),
            (30.0, 40.0),
            (0.0, 0.0),
            (30.0, 0.0),
            (true, true),
            (70.0, 120.0),
        ),
    ];
    let cases = [
        (
            "repeated points along an open polyline",
            vec![
                move_to(0.0, 0.0),
                line_to(0.0, 0.0),
                line_to(30.0, 0.0),
                line_to(30.0, 0.0),
                line_to(30.0, 0.0),
                line_to(30.0, 40.0),
            ],
            open,
        ),
        (
            "a closed triangle that returns to its start before closing",
            vec![
                move_to(0.0, 0.0),
                line_to(30.0, 0.0),
                line_to(30.0, 40.0),
                line_to(0.0, 0.0),
                PathSegment::Close,
            ],
            closed.clone(),
        ),
        (
            "a closed triangle whose start is repeated",
            vec![
                move_to(0.0, 0.0),
                line_to(0.0, 0.0),
                line_to(30.0, 0.0),
                line_to(30.0, 40.0),
                PathSegment::Close,
            ],
            closed,
        ),
    ];
    for (what, segments, expected) in cases {
        let drawn = draw_list(&list(vec![solid(segments)]));
        assert_segments(
            segments_of(&drawn, only_stroke(&drawn)),
            &expected,
            1e-4,
            what,
        );
    }
}

// Why: a line of one data point, or of points that project to one place, has no direction to expand along, so a
// subpath with fewer than two distinct points yields no segments and, on its own, no draw, rather than a segment
// of no length or an empty draw for the painter; the subpaths beside it are unaffected.
#[test]
fn a_subpath_with_fewer_than_two_distinct_points_yields_no_segments() {
    for (what, segments) in [
        ("a lone MoveTo", vec![move_to(5.0, 5.0)]),
        (
            "a lone MoveTo that is closed",
            vec![move_to(5.0, 5.0), PathSegment::Close],
        ),
        (
            "a subpath of coincident points",
            vec![move_to(5.0, 5.0), line_to(5.0, 5.0), line_to(5.0, 5.0)],
        ),
        (
            "a closed subpath of coincident points",
            vec![move_to(5.0, 5.0), line_to(5.0, 5.0), PathSegment::Close],
        ),
    ] {
        let drawn = draw_list(&list(vec![solid(segments)]));
        assert!(
            drawn.draws.is_empty(),
            "{what} yields no draw, and so no segments: {drawn:?}"
        );
    }

    let drawn = draw_list(&list(vec![solid(vec![
        move_to(5.0, 5.0),
        move_to(0.0, 0.0),
        line_to(30.0, 0.0),
    ])]));
    assert_segments(
        segments_of(&drawn, only_stroke(&drawn)),
        &[segment(
            (0.0, 0.0),
            (0.0, 0.0),
            (30.0, 0.0),
            (30.0, 0.0),
            (false, false),
            (0.0, 30.0),
        )],
        1e-4,
        "a lone point followed by a subpath of two points",
    );
}

// Why: the shader transforms the pen with the geometry, as PDF does, so the segments stay in item space and the
// params carry the leaf's whole item-to-figure transform, in the display list's `[a, b, c, d]` and `[e, f]` order,
// together with everything else the pipelines need per draw: the premultiplied colour, the width in item units,
// and the cap and join as the codes the shader switches on. A transform composed in the wrong order, a colour
// with straight alpha, or a cap and join code swapped would draw every line displaced, too bright, or with the
// wrong ends.
#[test]
fn a_stroke_draws_params_carry_the_leafs_transform_colour_width_cap_and_join() {
    let outer = Transform {
        a: 2.0,
        b: 0.5,
        c: -0.25,
        d: 3.0,
        e: 100.0,
        f: 200.0,
    };
    let color = Rgba::new(0.2, 0.4, 0.6, 0.4);
    for (cap, join) in [
        (LineCap::Butt, LineJoin::Miter),
        (LineCap::Round, LineJoin::Round),
        (LineCap::Square, LineJoin::Bevel),
    ] {
        let drawn = draw_list(&list(vec![group(
            None,
            Some(outer),
            vec![group(
                None,
                Some(Transform::translate(5.0, 5.0)),
                vec![sourced(
                    stroked(
                        polyline(&[(0.0, 0.0), (30.0, 0.0), (30.0, 40.0)]),
                        color,
                        3.5,
                        Vec::new(),
                        0.0,
                        cap,
                        join,
                    ),
                    7,
                )],
            )],
        )]));

        let draw = only_stroke(&drawn);
        assert_eq!(
            draw.source,
            Some(NodeId(7)),
            "the draw names the path's node: {draw:?}"
        );
        let params = params_of(&drawn, draw);
        assert_eq!(
            params.linear,
            [2.0, 0.5, -0.25, 3.0],
            "the linear part of the item-to-figure transform is [a, b, c, d]: {params:?}"
        );
        // The inner translation is applied first and then the outer transform, which maps (5, 5) to
        // (2·5 − 0.25·5 + 100, 0.5·5 + 3·5 + 200).
        assert_eq!(
            params.offset,
            [108.75, 217.5, 0.0, 0.0],
            "the translation is [e, f, 0, 0] of the composed transform: {params:?}"
        );
        assert_color_close(
            params.color,
            premultiplied_fractions(51, 102, 153, 102),
            "the colour is premultiplied as a fill's vertices are",
        );
        assert_eq!(
            params.width, 3.5,
            "the width is in item units, as given: {params:?}"
        );
        assert_eq!(
            params.cap,
            cap_code(cap),
            "{cap:?} caps are coded {}: {params:?}",
            cap_code(cap)
        );
        assert_eq!(
            params.join,
            join_code(join),
            "{join:?} joins are coded {}: {params:?}",
            join_code(join)
        );
        assert_eq!(
            params.dash_count, 0,
            "a solid stroke has no dash entries: {params:?}"
        );
        assert_segments(
            segments_of(&drawn, draw),
            &[
                segment(
                    (0.0, 0.0),
                    (0.0, 0.0),
                    (30.0, 0.0),
                    (30.0, 40.0),
                    (false, true),
                    (0.0, 30.0),
                ),
                segment(
                    (0.0, 0.0),
                    (30.0, 0.0),
                    (30.0, 40.0),
                    (30.0, 40.0),
                    (true, false),
                    (30.0, 70.0),
                ),
            ],
            1e-4,
            "beneath the transforms the segments stay in item space",
        );
    }
}

// Why: the fragment shader dashes by the arc length of the segments, in item units, against the pattern in the
// params, so the pattern must reach it in item units too (a group's scale reaches the shader in the transform,
// which scales the dashes with the geometry as PDF does), an odd pattern must be doubled as PDF doubles it, its
// period must be the sum of the entries the shader sees, the phase must be the given one reduced into [0, period),
// which is the same phase and spares the shader the reduction, and a pattern the params cannot hold must be drawn
// solid rather than truncated into some other pattern; and a dashed line must stay one draw with the segments of
// the solid one, or it would cost a draw or a segment per dash.
#[test]
fn a_dash_pattern_is_carried_in_item_units_doubled_when_odd_and_solid_when_longer_than_the_params_hold()
 {
    /// A pattern and its phase as given, the scale of the group the line lies beneath, and the dash entries and
    /// phase expected in the params.
    struct Case {
        what: &'static str,
        dash: Vec<f64>,
        dash_offset: f64,
        group_scale: f64,
        dashes: Vec<f32>,
        offset: f32,
    }
    let case = |what, dash, dash_offset, group_scale, dashes, offset| Case {
        what,
        dash,
        dash_offset,
        group_scale,
        dashes,
        offset,
    };
    let sixteen: Vec<f64> = (1..=16).map(f64::from).collect();
    let sixteen_as_f32: Vec<f32> = sixteen.iter().map(|&d| d as f32).collect();
    let cases = vec![
        case("a solid stroke", vec![], 0.0, 1.0, vec![], 0.0),
        case("6 on, 4 off", vec![6.0, 4.0], 0.0, 1.0, vec![6.0, 4.0], 0.0),
        case(
            "6 on, 4 off from an offset of 6",
            vec![6.0, 4.0],
            6.0,
            1.0,
            vec![6.0, 4.0],
            6.0,
        ),
        case(
            "6 on, 4 off from an offset of 26, which the period of 10 reduces to 6",
            vec![6.0, 4.0],
            26.0,
            1.0,
            vec![6.0, 4.0],
            6.0,
        ),
        case(
            "6 on, 4 off from an offset of -4, which the period of 10 reduces to 6",
            vec![6.0, 4.0],
            -4.0,
            1.0,
            vec![6.0, 4.0],
            6.0,
        ),
        case(
            "the odd pattern [3], doubled to 3 on, 3 off",
            vec![3.0],
            0.0,
            1.0,
            vec![3.0, 3.0],
            0.0,
        ),
        case(
            "the odd pattern [1, 2, 3], doubled",
            vec![1.0, 2.0, 3.0],
            0.0,
            1.0,
            vec![1.0, 2.0, 3.0, 1.0, 2.0, 3.0],
            0.0,
        ),
        case(
            "sixteen entries, the most the params hold",
            sixteen,
            0.0,
            1.0,
            sixteen_as_f32,
            0.0,
        ),
        case(
            "nine entries, eighteen when doubled, drawn solid",
            vec![1.0; 9],
            0.0,
            1.0,
            vec![],
            0.0,
        ),
        case(
            "eighteen entries, drawn solid",
            vec![1.0; 18],
            0.0,
            1.0,
            vec![],
            0.0,
        ),
        case(
            "6 on, 4 off beneath a group scale of 2, still in item units",
            vec![6.0, 4.0],
            6.0,
            2.0,
            vec![6.0, 4.0],
            6.0,
        ),
    ];
    for Case {
        what,
        dash,
        dash_offset,
        group_scale,
        dashes,
        offset,
    } in cases
    {
        let drawn = draw_list(&list(vec![group(
            None,
            Some(scale_then_translate(group_scale, group_scale, 0.0, 0.0)),
            vec![stroked_line(dash, dash_offset, LineCap::Butt)],
        )]));

        let draw = only_stroke(&drawn);
        let params = params_of(&drawn, draw);
        assert_eq!(
            params.dash_count as usize,
            dashes.len(),
            "with {what} the params hold {} dash entries: {params:?}",
            dashes.len()
        );
        assert_eq!(
            &params.dashes[..dashes.len()],
            dashes.as_slice(),
            "with {what} the dash entries are in item units, in order: {params:?}"
        );
        if !dashes.is_empty() {
            assert_close(
                f64::from(params.period),
                dashes.iter().map(|&d| f64::from(d)).sum(),
                1e-4,
                &format!("with {what} the period is the sum of the entries"),
            );
            assert_eq!(
                params.dash_offset, offset,
                "with {what} the phase is the given one reduced into [0, period), in item units: {params:?}"
            );
        }
        assert_eq!(
            params.width, 4.0,
            "with {what} the width stays in item units: {params:?}"
        );
        assert_eq!(
            params.linear,
            [group_scale as f32, 0.0, 0.0, group_scale as f32],
            "with {what} the group's scale reaches the shader in the transform: {params:?}"
        );
        assert_segments(
            segments_of(&drawn, draw),
            &[segment(
                (0.0, 50.0),
                (0.0, 50.0),
                (100.0, 50.0),
                (100.0, 50.0),
                (false, false),
                (0.0, 100.0),
            )],
            1e-4,
            what,
        );
    }
}

// Why: the display-list contract has a backend skip what it cannot draw rather than panic, and a stroke it cannot
// draw is the stroke alone: a width or a dash entry that is negative or not finite, a phase that is not finite, a
// pattern with no positive period (which would dash forever) or a colour that is not a number gives no stroke
// draw and no segments for it, while the fill of the same path still draws, or one bad line style would blank a
// filled marker.
#[test]
fn an_invalid_stroke_gives_no_stroke_draw_while_the_fill_of_the_same_path_still_draws() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let nan = f64::NAN;
    // The stroke's colour, width, dash pattern and phase.
    let cases = [
        (
            "a width that is not a number",
            Rgba::BLACK,
            nan,
            vec![],
            0.0,
        ),
        ("an infinite width", Rgba::BLACK, f64::INFINITY, vec![], 0.0),
        ("a negative width", Rgba::BLACK, -1.0, vec![], 0.0),
        (
            "a dash entry that is not a number",
            Rgba::BLACK,
            1.0,
            vec![nan, 2.0],
            0.0,
        ),
        (
            "a negative dash entry",
            Rgba::BLACK,
            1.0,
            vec![-1.0, 2.0],
            0.0,
        ),
        (
            "a dash pattern of zeros, whose period is not positive",
            Rgba::BLACK,
            1.0,
            vec![0.0, 0.0],
            0.0,
        ),
        (
            "an odd dash pattern of one zero, whose period is not positive",
            Rgba::BLACK,
            1.0,
            vec![0.0],
            0.0,
        ),
        (
            "a phase that is not a number",
            Rgba::BLACK,
            1.0,
            vec![6.0, 4.0],
            nan,
        ),
        (
            "an infinite phase",
            Rgba::BLACK,
            1.0,
            vec![6.0, 4.0],
            f64::INFINITY,
        ),
        (
            "a colour that is not a number",
            Rgba::new(f32::NAN, 0.0, 0.0, 1.0),
            1.0,
            vec![],
            0.0,
        ),
    ];
    for (what, color, width, dash, dash_offset) in cases {
        let drawn = draw_list(&list(vec![sourced(
            filled_and_stroked(
                rect_segments(30.0, 40.0, 50.0, 20.0),
                red,
                Stroke {
                    color,
                    width,
                    dash,
                    dash_offset,
                    cap: LineCap::Butt,
                    join: LineJoin::Miter,
                },
            ),
            7,
        )]));

        assert_eq!(
            drawn.draws.len(),
            1,
            "with {what} the fill is the only draw: {:?}",
            drawn.draws
        );
        let draw = &drawn.draws[0];
        assert!(
            matches!(draw.kind, DrawKind::Triangles(_)) && draw.source == Some(NodeId(7)),
            "with {what} the one draw is the path's fill: {draw:?}"
        );
        assert_close(
            draw_area(&drawn, draw),
            1000.0,
            1e-3,
            &format!("with {what} the fill covers its rectangle"),
        );
    }
}

// Why: PDF paints a path's fill and then its stroke over it, and the painter draws a list in order, so a path with
// both must be its fill's triangles followed by its stroke as two consecutive draws, and both must carry what the
// painter cuts, tests and attributes by: the leaf's clip in figure points, its depth group and its node. A stroke
// that lost the clip would paint over the neighbouring subplot, one outside the group would not be tested against
// the faces it outlines, and one without the node could not be picked.
#[test]
fn a_filled_and_stroked_path_draws_its_fill_then_its_stroke_with_the_leafs_clip_source_and_depth_group()
 {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let clip = Rect::new(0.0, 0.0, 60.0, 60.0);
    let rectangle = || {
        sourced(
            filled_and_stroked(rect_segments(0.0, 0.0, 50.0, 20.0), red, black_stroke(2.0)),
            7,
        )
    };
    let [pin0, pin1] = range_pins(-10.0, -10.0);
    // The items beneath the clipped, translated group, the depth group expected on the draws, and the z of the
    // fill's vertices and of the segments' ends: 0 outside every depth group, and 0.75 inside a group whose range
    // the pins fix to [0, 1] with the path at a constant depth of 0.25.
    let cases: [(&str, Vec<Item>, Option<u32>, f32); 2] = [
        ("outside every depth group", vec![rectangle()], None, 0.0),
        (
            "inside a depth group",
            vec![depth_group(vec![
                pin0,
                pin1,
                with_depth(rectangle(), Depth::Plane(DepthPlane::constant(0.25))),
            ])],
            Some(0),
            0.75,
        ),
    ];
    for (what, items, group_number, z) in cases {
        let drawn = draw_list(&list(vec![group(
            Some(clip),
            Some(Transform::translate(10.0, 20.0)),
            items,
        )]));

        let of_path: Vec<&Draw> = drawn
            .draws
            .iter()
            .filter(|d| d.source == Some(NodeId(7)))
            .collect();
        assert_eq!(
            of_path.len(),
            2,
            "{what}, the path is two draws: {:?}",
            drawn.draws
        );
        let first = drawn
            .draws
            .iter()
            .position(|d| d.source == Some(NodeId(7)))
            .expect("the path is drawn");
        let (fill, stroke) = (&drawn.draws[first], &drawn.draws[first + 1]);
        assert!(
            matches!(fill.kind, DrawKind::Triangles(_))
                && matches!(stroke.kind, DrawKind::Stroke { .. })
                && stroke.source == Some(NodeId(7)),
            "{what}, the fill's triangles are drawn and then, as the next draw, the stroke: {:?}",
            drawn.draws
        );
        for draw in [fill, stroke] {
            assert_eq!(
                draw.clip,
                Some(clip),
                "{what}, both draws record the leaf's clip in figure points: {draw:?}"
            );
            assert_eq!(
                draw.depth_group, group_number,
                "{what}, both draws lie in the leaf's depth group: {draw:?}"
            );
            assert_eq!(
                draw.texture, None,
                "{what}, neither draw samples a texture: {draw:?}"
            );
        }
        assert_close(
            draw_area(&drawn, fill),
            1000.0,
            1e-3,
            &format!("{what}, the fill covers the rectangle"),
        );
        assert_rect_close(
            draw_bbox(&drawn, fill),
            bounds(10.0, 20.0, 60.0, 40.0),
            1e-3,
            &format!("{what}, the fill lies at the rectangle translated by the group"),
        );
        assert!(
            all_at_z(&vertices_of_draw(&drawn, fill), z),
            "{what}, the fill's vertices lie at z = {z}: {:?}",
            vertices_of_draw(&drawn, fill)
        );
        let segments = segments_of(&drawn, stroke);
        assert_eq!(
            segments.len(),
            4,
            "{what}, the stroke is the four edges of the rectangle: {segments:?}"
        );
        assert_z_pairs(
            segments,
            &[[f64::from(z); 2]; 4],
            1e-6,
            &format!(
                "{what}, every end of the stroke's segments lies at the path's constant depth"
            ),
        );
        assert!(
            segments.iter().all(|s| s.grad == [0.0, 0.0]),
            "{what}, a constant depth has no gradient: {segments:?}"
        );
        assert_eq!(
            params_of(&drawn, stroke).offset,
            [10.0, 20.0, 0.0, 0.0],
            "{what}, the params carry the leaf's translation: {:?}",
            params_of(&drawn, stroke)
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Glyph runs
// ---------------------------------------------------------------------------------------------------------------

// Why: text is drawn from font outlines; an outline that is not scaled by the run size, not placed at the glyph
// origin, or left in the font's y-up space would render labels at the wrong size, place or upside down, and a run
// that lost its node could not be picked.
#[test]
fn a_glyph_is_drawn_at_its_size_above_its_baseline_origin() {
    let size = 20.0;
    let origin = Point::new(10.0, 50.0);
    let item = sourced(glyph_h(origin, size, Rgba::new(0.0, 0.0, 1.0, 1.0)), 8);

    let drawn = draw_list(&list(vec![item]));
    assert_eq!(
        drawn.draws.len(),
        1,
        "one run is one draw: {:?}",
        drawn.draws
    );
    let draw = &drawn.draws[0];
    assert!(draw_area(&drawn, draw) > 0.0, "the glyph produces ink");
    assert_eq!(
        draw.source,
        Some(NodeId(8)),
        "the draw names the run's node: {draw:?}"
    );
    assert_eq!(draw.texture, None, "glyphs are solid geometry: {draw:?}");
    assert!(
        vertices_of_draw(&drawn, draw)
            .iter()
            .all(|v| v.color == [0, 0, 255, 255]),
        "the glyph is drawn in the run colour"
    );
    assert_outside_depth_groups(&drawn);

    let extent = draw_bbox(&drawn, draw);
    let (min, max) = (extent.min, extent.max);
    assert!(
        f64::from(min.x) >= origin.x - 0.5 && f64::from(max.x) <= origin.x + size,
        "ink starts at the pen position: {min:?}–{max:?}"
    );
    assert!(
        f64::from(max.y) <= origin.y + 0.5,
        "a capital H sits on the baseline, not below it: {min:?}–{max:?}"
    );
    assert!(
        f64::from(min.y) >= origin.y - size,
        "the ink is no taller than one em: {min:?}–{max:?}"
    );
    assert!(
        f64::from(max.y - min.y) >= 0.5 * size,
        "the cap height is a substantial part of the em: {min:?}–{max:?}"
    );
    assert!(
        f64::from(max.x - min.x) >= 0.4 * size,
        "the glyph is scaled to the run size: {min:?}–{max:?}"
    );
}

// Why: a label is one run of many glyphs and must reach the painter as one draw, or a tick label would cost a draw
// per character; and a glyph the layout could not place (a non-finite position) must be left out without taking the
// rest of the run with it, or one bad glyph would blank a whole label.
#[test]
fn a_run_of_several_glyphs_is_one_draw_and_a_glyph_with_a_non_finite_position_is_skipped() {
    let origin = Point::new(10.0, 50.0);
    let size = 20.0;
    let both = draw_list(&list(vec![glyph_run("HH", origin, size, Rgba::BLACK)]));
    assert_eq!(
        both.draws.len(),
        1,
        "a run of two glyphs is one draw: {:?}",
        both.draws
    );
    let extent = bbox(&both);
    assert!(
        f64::from(extent.max.x) > origin.x + size,
        "the second glyph's ink lies beyond one em from the origin: {extent:?}"
    );

    let one = draw_list(&list(vec![with_run(
        glyph_run("HH", origin, size, Rgba::BLACK),
        |run| run.glyphs[1].x = f64::NAN,
    )]));
    let single = draw_list(&list(vec![glyph_h(origin, size, Rgba::BLACK)]));
    assert_eq!(one.draws.len(), 1, "the run still draws: {:?}", one.draws);
    assert_rect_close(
        bbox(&one),
        bbox(&single),
        1e-3,
        "the run of two glyphs, one skipped, covers what the one glyph covers",
    );
}

// ---------------------------------------------------------------------------------------------------------------
// The background
// ---------------------------------------------------------------------------------------------------------------

// Why: the background is not part of the list: the offscreen renderer clears its target to it and the interactive
// canvas fills the figure's rectangle with egui, because a quad in figure points left the last pixel row uncovered
// when the image size rounded up from the page size. A background draw would therefore paint it twice (visibly, when
// it is translucent), take the first place in the paint order, and make a figure without items a non-empty list that
// the painter uploads for nothing.
#[test]
fn the_background_is_never_a_draw() {
    let square = || {
        filled(
            rect_segments(30.0, 40.0, 50.0, 20.0),
            Rgba::BLACK,
            FillRule::NonZero,
        )
    };
    let backgrounds = [
        ("opaque", Rgba::WHITE),
        ("translucent", Rgba::new(0.0, 0.0, 1.0, 0.5)),
        ("fully transparent", TRANSPARENT),
        ("non-finite", Rgba::new(f32::NAN, 0.0, 0.0, 1.0)),
    ];
    for (what, background) in backgrounds {
        let drawn = draw_list(&with_background(background, vec![square()]));
        assert_eq!(
            drawn.draws.len(),
            1,
            "over a background that is {what} the only draw is the item's: {:?}",
            drawn.draws
        );
        assert_close(
            draw_area(&drawn, &drawn.draws[0]),
            1000.0,
            1e-3,
            "the item is drawn and nothing else",
        );

        let alone = draw_list(&with_background(background, Vec::new()));
        assert!(
            alone.is_empty()
                && alone.draws.is_empty()
                && alone.vertices.is_empty()
                && alone.indices.is_empty(),
            "a list holding only a background that is {what} is an empty draw list: {alone:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------------------------------------------

// Why: the list is reused across a range of canvas scales and the mapping is a uniform, so the resolution can change
// only how finely curves are flattened, never where or how large the geometry is, what an image's quad is, or what
// clip a leaf records: a list whose positions followed the scale would draw a magnified figure twice magnified, and
// one flattened for the default resolution would show polygonal circles and jagged glyphs when the canvas is
// zoomed in.
#[test]
fn the_resolution_scale_refines_the_flattening_without_moving_or_scaling_the_geometry() {
    let circle = list(vec![filled(
        circle_segments(50.0, 50.0, 50.0),
        Rgba::BLACK,
        FillRule::NonZero,
    )]);
    let coarse = draw_list_at(&circle, at_scale(1.0));
    let fine = draw_list_at(&circle, at_scale(10.0));

    let disc = std::f64::consts::PI * 50.0 * 50.0;
    for (what, drawn) in [("scale 1", &coarse), ("scale 10", &fine)] {
        assert_close(
            area(drawn),
            disc,
            0.01 * disc,
            &format!("the circle at {what} covers its area in figure points"),
        );
        assert_rect_close(
            bbox(drawn),
            bounds(0.0, 0.0, 100.0, 100.0),
            1e-3,
            &format!("the circle at {what} keeps its extent in figure points"),
        );
    }
    assert!(
        fine.vertices.len() > coarse.vertices.len(),
        "the circle is flattened more finely at scale 10 ({} vertices) than at scale 1 ({} vertices)",
        fine.vertices.len(),
        coarse.vertices.len()
    );

    let o = list(vec![glyph_run(
        "O",
        Point::new(10.0, 50.0),
        20.0,
        Rgba::BLACK,
    )]);
    let (coarse_o, fine_o) = (
        draw_list_at(&o, at_scale(1.0)),
        draw_list_at(&o, at_scale(10.0)),
    );
    assert!(
        fine_o.vertices.len() > coarse_o.vertices.len(),
        "a curved glyph takes a finer tessellation bucket at scale 10 ({} vertices) than at scale 1 ({} vertices)",
        fine_o.vertices.len(),
        coarse_o.vertices.len()
    );
    assert_rect_close(
        bbox(&fine_o),
        bbox(&coarse_o),
        0.1,
        "the glyph keeps its extent at scale 10",
    );

    // An image's quad and the clip a leaf records do not depend on the resolution either.
    let clip = Rect::new(10.0, 10.0, 50.0, 50.0);
    let transform = scale_then_translate(10.0, 5.0, 20.0, 20.0);
    let rect = Rect::new(0.0, 0.0, 3.0, 2.0);
    let samples: Arc<[u8]> = Arc::from(tagged_rgb(3, 2));
    let placed = list(vec![group(
        Some(clip),
        Some(transform),
        vec![
            image_of(rect, 3, 2, ImageItem::RGB, Arc::clone(&samples)),
            filled(
                rect_segments(0.0, 0.0, 3.0, 2.0),
                Rgba::BLACK,
                FillRule::NonZero,
            ),
        ],
    )]);
    let coarse_placed = draw_list_at(&placed, at_scale(1.0));
    let fine_placed = draw_list_at(&placed, at_scale(10.0));
    for drawn in [&coarse_placed, &fine_placed] {
        assert_eq!(
            drawn.draws.len(),
            2,
            "the image and the path are two draws: {:?}",
            drawn.draws
        );
        assert_textured_quad(
            drawn,
            &drawn.draws[0],
            transform,
            rect,
            &tile_key(&samples, 3, ImageItem::RGB, 0..3, 0..2),
        );
        assert!(
            drawn.draws.iter().all(|d| d.clip == Some(clip)),
            "both leaves record the clip {clip:?} at any scale: {:?}",
            drawn.draws
        );
    }
    assert_eq!(
        vertices_of_draw(&coarse_placed, &coarse_placed.draws[0]),
        vertices_of_draw(&fine_placed, &fine_placed.draws[0]),
        "the image's quad is identical at scale 1 and scale 10"
    );
}

// Why: a stroke's segments are in item space and its width, dashes and transform are in its params, none of which
// depends on the resolution: only the flattening of its curves does (which the circle test shows the resolution
// scale refining), to within a screen tolerance that the stretch of the leaf's transform tightens in item units as
// the scale does. A stroke whose segments or params followed the scale (as the tessellated hairline and dashes
// once had to) would draw a magnified figure twice magnified, and one flattened for the default resolution would
// show polygonal circles when zoomed in.
#[test]
fn the_resolution_scale_and_the_group_stretch_change_only_the_flattening_of_a_stroke() {
    let dashed = list(vec![sourced(
        stroked(
            polyline(&[(0.0, 0.0), (30.0, 0.0), (30.0, 40.0)]),
            Rgba::new(0.0, 0.0, 1.0, 0.5),
            3.0,
            vec![6.0, 4.0],
            2.0,
            LineCap::Round,
            LineJoin::Bevel,
        ),
        1,
    )]);
    let coarse = draw_list_at(&dashed, at_scale(1.0));
    let fine = draw_list_at(&dashed, at_scale(10.0));
    assert_eq!(
        segments_of(&coarse, only_stroke(&coarse)),
        segments_of(&fine, only_stroke(&fine)),
        "the segments of straight edges are the same at scale 1 and at scale 10"
    );
    assert_eq!(
        params_of(&coarse, only_stroke(&coarse)),
        params_of(&fine, only_stroke(&fine)),
        "the params (transform, colour, width, cap, join, dashes and phase) are the same at scale 1 and at scale 10"
    );

    let circle = |transform: Option<Transform>| {
        list(vec![group(
            None,
            transform,
            vec![solid(circle_segments(50.0, 50.0, 20.0))],
        )])
    };
    let chords = |display: &DisplayList, scale: f32| {
        let drawn = draw_list_at(display, at_scale(scale));
        segments_of(&drawn, only_stroke(&drawn)).len()
    };
    let plain = circle(None);
    let stretched = circle(Some(scale_then_translate(4.0, 1.0, 0.0, 0.0)));
    assert!(
        chords(&stretched, 1.0) > chords(&plain, 1.0),
        "beneath a stretch of 4 the tolerance in item units is a quarter, so the circle takes more chords ({}) \
         than without it ({})",
        chords(&stretched, 1.0),
        chords(&plain, 1.0)
    );
    let drawn = draw_list_at(&stretched, at_scale(1.0));
    let params = params_of(&drawn, only_stroke(&drawn));
    assert!(
        params.linear == [4.0, 0.0, 0.0, 1.0] && params.width == 4.0,
        "the stretch reaches the shader in the transform and leaves the width in item units: {params:?}"
    );
}

// Why: a zero-width stroke is the thinnest line the device can draw, as in PDF: the shader makes a hairline one
// screen point wide in every direction, whatever the resolution and whatever the leaf's transform stretches (which
// `offscreen.rs` checks in pixels beneath a non-uniform scale). The canvas must therefore leave the width at 0 and
// change nothing else in the params: a width baked in from the resolution (as the tessellated hairline's was)
// would thicken when the same list is drawn at a larger scale, and one made from the transform's stretch would be
// wrong beneath a non-uniform scale, where only the shader knows the direction of the line on screen.
#[test]
fn a_zero_width_stroke_keeps_a_width_of_zero_in_its_params_beneath_any_scale_and_at_any_resolution()
{
    for (sx, sy, scale) in [
        (1.0, 1.0, 1.0),
        (1.0, 1.0, 4.0),
        (1.0, 1.0, 0.5),
        (2.0, 2.0, 2.0),
        (4.0, 1.0, 1.0),
    ] {
        let drawn = draw_list_at(
            &list(vec![group(
                None,
                Some(scale_then_translate(sx, sy, 0.0, 0.0)),
                vec![hairline()],
            )]),
            at_scale(scale),
        );

        let params = params_of(&drawn, only_stroke(&drawn));
        assert_eq!(
            params.width, 0.0,
            "beneath a scale of ({sx}, {sy}) at resolution scale {scale} the hairline's width stays 0 for the \
             shader to make one screen point of: {params:?}"
        );
        assert_eq!(
            params.linear,
            [sx as f32, 0.0, 0.0, sy as f32],
            "beneath a scale of ({sx}, {sy}) the params carry the scale: {params:?}"
        );
    }
}

// Why: a resolution that is not a positive finite number has no flattening tolerance; the canvas asks for one only
// by mistake, and the answer must be an empty list rather than a panic, a list of non-finite vertices, or geometry
// flattened to nothing.
#[test]
fn an_invalid_resolution_scale_gives_an_empty_list() {
    let display = with_background(
        Rgba::WHITE,
        vec![
            filled(
                rect_segments(0.0, 0.0, 10.0, 10.0),
                Rgba::BLACK,
                FillRule::NonZero,
            ),
            solid(line_segments()),
        ],
    );
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let drawn = draw_list_at(&display, at_scale(scale));
        assert!(
            drawn.is_empty(),
            "at a resolution scale of {scale} the list is empty: {:?}",
            drawn.draws
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------------------------------------------

// Why: an image is drawn as a textured quad whose corners are the corners of its rectangle under the same transforms
// as every other leaf, and the compiler places a mirrored pixel range with a negative scale, so a translating, scaling
// and mirroring transform must land the corners exactly where the leaf's transform maps them. A quad placed by the
// untransformed rectangle or with its texture coordinates at the wrong corners would draw the raster elsewhere, at the
// wrong size, or upside down, and a draw that did not name the item's own samples would show the painter another
// image.
#[test]
fn an_image_becomes_one_textured_quad_at_its_transformed_corners() {
    // Pixel space [0, 3] × [0, 2], scaled by 10 and −5 (mirroring the rows) and moved to (20, 60), covers the figure
    // rectangle [20, 50] × [50, 60].
    let transform = scale_then_translate(10.0, -5.0, 20.0, 60.0);
    let rect = Rect::new(0.0, 0.0, 3.0, 2.0);
    let samples: Arc<[u8]> = Arc::from(tagged_rgb(3, 2));
    let drawn = draw_list(&list(vec![group(
        None,
        Some(transform),
        vec![sourced(
            image_of(rect, 3, 2, ImageItem::RGB, Arc::clone(&samples)),
            4,
        )],
    )]));

    assert_eq!(
        drawn.draws.len(),
        1,
        "an image within one tile is one draw: {:?}",
        drawn.draws
    );
    let draw = &drawn.draws[0];
    assert_textured_quad(
        &drawn,
        draw,
        transform,
        rect,
        &tile_key(&samples, 3, ImageItem::RGB, 0..3, 0..2),
    );
    assert_eq!(
        draw.source,
        Some(NodeId(4)),
        "the draw names the image's node: {draw:?}"
    );
    assert_eq!(
        draw.clip, None,
        "an unclipped image records no clip: {draw:?}"
    );
    assert_rect_close(
        bbox(&drawn),
        bounds(20.0, 50.0, 50.0, 60.0),
        1e-3,
        "the quad lies at the image's transformed corners",
    );
    assert_outside_depth_groups(&drawn);
}

// Why: on the floor or a wall of a three-dimensional axes the placement has shear, so the image is a parallelogram in
// figure space. A quad built from an axis-aligned rectangle would draw the bounding box of the parallelogram with the
// raster stretched to fill it, so every corner must land where the transform sends it and the covered area must be
// the parallelogram's, not its bounding box's.
#[test]
fn a_skewed_transform_maps_an_image_to_a_parallelogram() {
    // The corners of pixel space [0, 6] × [0, 4] land at (30, 40), (90, 52), (46, 64) and (106, 76): a bounding box
    // of 76 × 36 = 2736 around a parallelogram of |10 · 6 − 2 · 4| · 6 · 4 = 1248, which the quad must cover.
    let transform = Transform {
        a: 10.0,
        b: 2.0,
        c: 4.0,
        d: 6.0,
        e: 30.0,
        f: 40.0,
    };
    let rect = Rect::new(0.0, 0.0, 6.0, 4.0);
    let samples: Arc<[u8]> = Arc::from(tagged_rgb(6, 4));
    let drawn = draw_list(&list(vec![group(
        None,
        Some(transform),
        vec![image_of(rect, 6, 4, ImageItem::RGB, Arc::clone(&samples))],
    )]));

    assert_eq!(drawn.draws.len(), 1);
    assert_textured_quad(
        &drawn,
        &drawn.draws[0],
        transform,
        rect,
        &tile_key(&samples, 6, ImageItem::RGB, 0..6, 0..4),
    );
}

/// The placement of the tiling tests: the item's pixel space scaled by 6 and moved to (40, 30), so that a 5 × 3 image
/// drawn into the rectangle of its pixel space with tiles of at most 2 pixels a side has tile column c covering
/// figure x ∈ [40 + 12c, 52 + 12c] (the last column only to 70) and tile row r covering y ∈ [30 + 12r, 42 + 12r]
/// (the last row only to 48).
const TILING: Transform = Transform {
    a: 6.0,
    b: 0.0,
    c: 0.0,
    d: 6.0,
    e: 40.0,
    f: 30.0,
};

/// A 5 × 3 image sharing `samples`, drawn into the rectangle of its pixel space beneath [`TILING`] inside a group
/// with `clip`.
fn tiled_image(clip: Option<Rect>, samples: &Arc<[u8]>) -> DisplayList {
    list(vec![group(
        clip,
        Some(TILING),
        vec![image_of(
            Rect::new(0.0, 0.0, 5.0, 3.0),
            5,
            3,
            ImageItem::RGB,
            Arc::clone(samples),
        )],
    )])
}

// Why: a GPU texture has a largest side, and a data image can be far wider than it (a spectrogram of a long record),
// so the raster is cut into tiles of at most that side, each a draw with a key of its own. The key of every tile must
// name the tile's sub-rectangle of the samples, in row-major order so that the painter uploads them as the samples
// are laid out; and the quads must share their edges exactly, because a tile a pixel out shows a seam of background
// through the data. The image is 7 pixels over 10 points cut into tiles of 3, so that the boundaries at 30/7 and
// 60/7 are not exactly representable: an implementation that placed a tile's far edge at its near edge plus the
// tile's width, rather than at the boundary's own pixel index, could differ from its neighbour's near edge in the
// last bit and fail the bit-equality below.
#[test]
fn an_image_wider_than_the_largest_tile_is_cut_into_abutting_tiles_in_row_major_order() {
    let samples: Arc<[u8]> = Arc::from(tagged_rgb(7, 7));
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    let drawn = draw_list_at(
        &list(vec![group(
            None,
            Some(TILING),
            vec![image_of(rect, 7, 7, ImageItem::RGB, Arc::clone(&samples))],
        )]),
        with_tiles(3),
    );

    assert_eq!(
        drawn.draws.len(),
        9,
        "a 7 × 7 image at a largest side of 3 is 3 × 3 tiles: {:?}",
        drawn.draws
    );
    let edges = [(0u32, 3u32), (3, 6), (6, 7)];
    let at = |pixel: u32| 10.0 * f64::from(pixel) / 7.0;
    for (k, draw) in drawn.draws.iter().enumerate() {
        let (r, c) = (k / 3, k % 3);
        let (x0, x1) = edges[c];
        let (y0, y1) = edges[r];
        let sub = Rect::new(at(x0), at(y0), at(x1) - at(x0), at(y1) - at(y0));
        assert_textured_quad(
            &drawn,
            draw,
            TILING,
            sub,
            &tile_key(&samples, 7, ImageItem::RGB, x0..x1, y0..y1),
        );
    }
    let extent =
        |draw: &Draw, pick: fn(&Vertex) -> f32| span(&vertices_of_draw(&drawn, draw), pick);
    for (r, row) in drawn.draws.chunks(3).enumerate() {
        for (c, pair) in row.windows(2).enumerate() {
            assert_eq!(
                extent(&pair[0], |v| v.pos[0]).1,
                extent(&pair[1], |v| v.pos[0]).0,
                "tiles ({r}, {c}) and ({r}, {}) share their vertical edge exactly",
                c + 1
            );
        }
    }
    for (k, (upper, lower)) in drawn.draws[..6].iter().zip(&drawn.draws[3..]).enumerate() {
        let (r, c) = (k / 3, k % 3);
        assert_eq!(
            extent(upper, |v| v.pos[1]).1,
            extent(lower, |v| v.pos[1]).0,
            "tiles ({r}, {c}) and ({}, {c}) share their horizontal edge exactly",
            r + 1
        );
    }
}

// Why: an image panned so that most of it lies outside the plot box must not be uploaded whole: a tile whose quad
// lies wholly outside the group's clip can show nothing, so a draw for it would make the painter upload a texture
// that no pixel samples. Only the tiles whose quads overlap the clip by some area are drawn (touching it along an
// edge shows nothing), in their row-major order, each with its whole quad, its whole texture range and the leaf's
// clip for the scissor to cut it by: a tessellation that trimmed a quad at the clip would have to cut its texture
// range in proportion and rebuild the list for every pan.
#[test]
fn tiles_wholly_outside_the_clip_are_not_drawn_and_the_others_keep_their_whole_quads() {
    let samples: Arc<[u8]> = Arc::from(tagged_rgb(5, 3));
    let columns = [(0u32, 2u32), (2, 4), (4, 5)];
    let rows = [(0u32, 2u32), (2, 3)];
    // Tile column 1 covers x ∈ [52, 64]: the first clip ends short of it, the second touches it along its left
    // edge, and the third cuts through it.
    for (what, clip, visible) in [
        (
            "ending before column 1",
            Rect::new(40.0, 30.0, 10.0, 18.0),
            [0usize].as_slice(),
        ),
        (
            "touching column 1 along its edge",
            Rect::new(40.0, 30.0, 12.0, 18.0),
            [0].as_slice(),
        ),
        (
            "cutting through column 1",
            Rect::new(40.0, 30.0, 15.0, 18.0),
            [0, 1].as_slice(),
        ),
    ] {
        let drawn = draw_list_at(&tiled_image(Some(clip), &samples), with_tiles(2));

        let expected: Vec<(usize, usize)> = (0..rows.len())
            .flat_map(|r| visible.iter().map(move |&c| (r, c)))
            .collect();
        assert_eq!(
            drawn.draws.len(),
            expected.len(),
            "with the clip {what}, the tiles {expected:?} are drawn in row-major order: {:?}",
            drawn.draws
        );
        for (draw, (r, c)) in drawn.draws.iter().zip(expected) {
            let (x0, x1) = columns[c];
            let (y0, y1) = rows[r];
            let sub = Rect::new(
                f64::from(x0),
                f64::from(y0),
                f64::from(x1 - x0),
                f64::from(y1 - y0),
            );
            assert_textured_quad(
                &drawn,
                draw,
                TILING,
                sub,
                &tile_key(&samples, 5, ImageItem::RGB, x0..x1, y0..y1),
            );
            assert_eq!(
                draw.clip,
                Some(clip),
                "tile ({r}, {c}) records the leaf's clip in figure points: {draw:?}"
            );
        }
    }
}

// Why: a tile must fit the device the list is drawn on, whose largest texture side the renderer passes as the
// resolution's tile side, and must not exceed what the offscreen renderer and the PDF exporter tile by, so the side
// is capped at `MAX_TILE_SIDE`; a tile side of zero is how a caller without textures asks for no images, and an image
// must then draw nothing rather than a white quad over the plot, while the other items still draw.
#[test]
fn the_tile_side_is_capped_at_max_tile_side_and_a_zero_side_draws_no_image() {
    assert_eq!(
        Resolution::default(),
        Resolution {
            scale: 1.0,
            max_tile_side: MAX_TILE_SIDE
        },
        "the default resolution is one screen unit per point with the largest tiles"
    );

    // A strip one pixel wider than the cap. An image with more tiles than a `u32` can number, which the canvas
    // declines to draw, cannot be built here: `ImageItem::is_valid` requires every one of its sample bytes to be
    // present.
    let width = MAX_TILE_SIDE + 1;
    let samples: Arc<[u8]> = Arc::from(vec![7u8; width as usize * 3]);
    let strip = list(vec![
        image_of(
            Rect::new(0.0, 0.0, f64::from(width), 1.0),
            width,
            1,
            ImageItem::RGB,
            Arc::clone(&samples),
        ),
        filled(
            rect_segments(300.0, 250.0, 20.0, 10.0),
            Rgba::BLACK,
            FillRule::NonZero,
        ),
    ]);
    let half = MAX_TILE_SIDE / 2;
    let capped = vec![0..MAX_TILE_SIDE, MAX_TILE_SIDE..width];
    for (max_tile_side, expected) in [
        (u32::MAX, capped.clone()),
        (MAX_TILE_SIDE + 1, capped.clone()),
        (MAX_TILE_SIDE, capped),
        (
            half,
            vec![0..half, half..MAX_TILE_SIDE, MAX_TILE_SIDE..width],
        ),
        (0, Vec::new()),
    ] {
        let drawn = draw_list_at(&strip, with_tiles(max_tile_side));

        let columns: Vec<Range<u32>> = drawn
            .draws
            .iter()
            .filter_map(|draw| draw.texture.as_ref().map(|tile| tile.columns.clone()))
            .collect();
        assert_eq!(
            columns, expected,
            "with a largest side of {max_tile_side} the strip is cut at min(max_tile_side, MAX_TILE_SIDE) pixels"
        );
        assert!(
            drawn
                .draws
                .last()
                .is_some_and(|draw| draw.texture.is_none()),
            "the rectangle after the image draws whatever the tile side: {:?}",
            drawn.draws
        );
        assert_eq!(
            ink_at(&drawn, Point::new(4000.0, 0.5)),
            max_tile_side != 0,
            "with a largest side of {max_tile_side} the image is drawn unless the side is zero"
        );
    }
}

// Why: display lists can be built by hand, and the display-list contract requires backends to skip invalid items
// rather than panic. An image with a channel count the painter cannot upload, with no pixels, with fewer samples than
// its size claims (which would read past the buffer), with alpha it does not carry, or with a non-finite or empty
// rectangle (which would put a NaN on the GPU or a degenerate quad) must be skipped without a draw, and the items
// around it must still draw.
#[test]
fn invalid_image_items_are_skipped_and_the_other_items_still_draw() {
    let samples = tagged_rgb(3, 2);
    let nan = f64::NAN;
    let rect = Rect::new(0.0, 0.0, 3.0, 2.0);
    // Every item is attributed to a node, so that the draws name the survivors: two channels, no columns, no rows,
    // too few samples, alpha the samples do not carry, a non-finite x, an infinite width, a zero width, and then the
    // valid image and the rectangle.
    let items = vec![
        sourced(image(rect, 3, 2, 2, samples[..12].to_vec()), 1),
        sourced(image(rect, 0, 2, ImageItem::RGB, Vec::new()), 2),
        sourced(image(rect, 3, 0, ImageItem::RGB, Vec::new()), 3),
        sourced(image(rect, 3, 2, ImageItem::RGB, samples[..17].to_vec()), 4),
        sourced(image(rect, 3, 2, ImageItem::RGBA, samples.clone()), 5),
        sourced(
            image(
                Rect::new(nan, 0.0, 3.0, 2.0),
                3,
                2,
                ImageItem::RGB,
                samples.clone(),
            ),
            6,
        ),
        sourced(
            image(
                Rect::new(0.0, 0.0, f64::INFINITY, 2.0),
                3,
                2,
                ImageItem::RGB,
                samples.clone(),
            ),
            7,
        ),
        sourced(
            image(
                Rect::new(0.0, 0.0, 0.0, 2.0),
                3,
                2,
                ImageItem::RGB,
                samples.clone(),
            ),
            8,
        ),
        sourced(
            image(
                Rect::new(100.0, 100.0, 30.0, 20.0),
                3,
                2,
                ImageItem::RGB,
                samples.clone(),
            ),
            9,
        ),
        sourced(
            filled(
                rect_segments(300.0, 250.0, 20.0, 10.0),
                Rgba::BLACK,
                FillRule::NonZero,
            ),
            10,
        ),
    ];

    let drawn = draw_list(&list(items));

    assert_eq!(
        drawn.draws.iter().map(|d| d.source).collect::<Vec<_>>(),
        [Some(NodeId(9)), Some(NodeId(10))],
        "only the valid image and the rectangle are drawn"
    );
    assert!(
        drawn.draws[0].texture.is_some() && drawn.draws[1].texture.is_none(),
        "the image's draw is textured and the rectangle's is not: {:?}",
        drawn.draws
    );
    assert!(
        ink_at(&drawn, Point::new(115.0, 110.0)),
        "the valid image after the invalid ones is drawn"
    );
    assert!(
        ink_at(&drawn, Point::new(310.0, 255.0)),
        "the rectangle after the invalid items is drawn"
    );
}

// Why: the display list is ordered back to front, and an image between two paths (a floor image under a surface,
// isolines drawn on top of a mapped field) must be drawn between them; batching textured draws apart from plain ones
// would put the image over the isolines drawn on it.
#[test]
fn an_image_keeps_its_place_in_the_paint_order() {
    let drawn = draw_list(&list(vec![
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
    ]));

    let textured: Vec<bool> = drawn
        .draws
        .iter()
        .map(|draw| draw.texture.is_some())
        .collect();
    assert_eq!(
        textured,
        [false, true, false],
        "draws are emitted in item order: {:?}",
        drawn.draws
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Depth groups and depth-carrying leaves
// ---------------------------------------------------------------------------------------------------------------

// Why: the leaves of one depth group must reach the painter as one depth-tested drawing, a draw per leaf in the
// painter's order, numbered as the group so that the painter clears the depth buffer once before them, attributed to
// the leaf's node, and with z normalised over the group so that the nearest depth is 0 and the farthest 1. Draws
// without a group number would be drawn without the test and give the painter's picture, and an inverted z the
// back-to-front one.
#[test]
fn a_depth_group_gives_depth_tested_draws_in_paint_order_with_z_normalised_over_the_group() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    let near = sourced(square_at(0.0, 0.0, 1.0, red, DepthPlane::constant(1.0)), 1);
    let far = sourced(square_at(2.0, 0.0, 1.0, blue, DepthPlane::constant(0.0)), 2);
    let drawn = draw_list(&list(vec![depth_group(vec![near, far])]));

    assert_eq!(
        drawn.draws.len(),
        2,
        "one draw per path leaf: {:?}",
        drawn.draws
    );
    for (draw, id) in drawn.draws.iter().zip([1, 2]) {
        assert_eq!(
            draw.source,
            Some(NodeId(id)),
            "the draws follow the paint order and name their node: {:?}",
            drawn.draws
        );
        assert_eq!(
            draw.depth_group,
            Some(0),
            "every draw of the first depth group is numbered 0: {draw:?}"
        );
        assert_eq!(
            draw.texture, None,
            "solid geometry samples no texture: {draw:?}"
        );
        assert_eq!(draw.clip, None, "no group clips these leaves: {draw:?}");
        assert!(
            index_count(draw) >= 6,
            "a square is at least two triangles: {draw:?}"
        );
    }

    let near_vertices = vertices_of_draw(&drawn, &drawn.draws[0]);
    assert!(
        all_at_z(&near_vertices, 0.0),
        "the nearer square lies at z = 0: {near_vertices:?}"
    );
    assert!(
        near_vertices.iter().all(|v| v.color == [255, 0, 0, 255]),
        "colours are premultiplied sRGB bytes: {near_vertices:?}"
    );
    assert_span(
        span(&near_vertices, |v| v.pos[0]),
        (0.0, 1.0),
        "positions are figure points, so the near square spans x = 0..1",
    );
    let far_vertices = vertices_of_draw(&drawn, &drawn.draws[1]);
    assert!(
        all_at_z(&far_vertices, 1.0),
        "the farther square lies at z = 1: {far_vertices:?}"
    );
    assert_span(
        span(&far_vertices, |v| v.pos[0]),
        (2.0, 3.0),
        "the far square spans x = 2..3",
    );
}

// Why: the exporter renders the content of a three-dimensional axes through the same pipeline but must get the
// painter's picture, so a depth-carrying leaf outside every depth group is drawn without a depth group and without a
// depth: its depth is ignored, its z is 0 like every other loose leaf's, and it keeps its place among the depthless
// leaves around it, or the exporter's picture would be depth tested against nothing and reordered.
#[test]
fn depth_carrying_leaves_outside_every_group_draw_in_order_without_a_group_at_z_zero() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    let drawn = draw_list(&list(vec![
        sourced(square_at(0.0, 0.0, 1.0, red, DepthPlane::constant(1.0)), 1),
        sourced(square_at(2.0, 0.0, 1.0, blue, DepthPlane::constant(0.0)), 2),
        sourced(
            filled(rect_segments(4.0, 0.0, 1.0, 1.0), red, FillRule::NonZero),
            3,
        ),
        sourced(square_at(6.0, 0.0, 1.0, blue, DepthPlane::constant(7.0)), 4),
    ]));

    assert_eq!(
        drawn.draws.iter().map(|d| d.source).collect::<Vec<_>>(),
        [
            Some(NodeId(1)),
            Some(NodeId(2)),
            Some(NodeId(3)),
            Some(NodeId(4))
        ],
        "every leaf is one draw, in paint order"
    );
    assert_outside_depth_groups(&drawn);
}

// Why: a figure with two three-dimensional axes puts two depth groups side by side in the paint order; each must be
// numbered as its own group, so that the painter clears the depth buffer between them, and each axes' depths must be
// normalised over that axes alone, rather than over both, in which case the nearer axes would hide the other; and a
// group whose depths all coincide has no range to normalise over, so its z must land in the middle rather than at
// an end or outside [0, 1].
#[test]
fn two_adjacent_depth_groups_are_numbered_in_paint_order_and_normalised_apart() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    let drawn = draw_list(&list(vec![
        depth_group(vec![sourced(
            square_at(0.0, 0.0, 1.0, red, DepthPlane::constant(0.0)),
            1,
        )]),
        depth_group(vec![sourced(
            square_at(2.0, 0.0, 1.0, blue, DepthPlane::constant(5.0)),
            2,
        )]),
    ]));

    assert_eq!(
        drawn
            .draws
            .iter()
            .map(|d| (d.source, d.depth_group))
            .collect::<Vec<_>>(),
        [(Some(NodeId(1)), Some(0)), (Some(NodeId(2)), Some(1))],
        "each group's draw carries the group's number, counted from 0 in paint order"
    );
    for draw in &drawn.draws {
        let vertices = vertices_of_draw(&drawn, draw);
        assert!(
            all_at_z(&vertices, 0.5),
            "each group normalises its one depth to the middle on its own, not over both groups: {vertices:?}"
        );
    }
}

// Why: the draws of a group are depth tested against one another after one clear of the depth buffer; a
// depth-carrying leaf straight after a depth group must therefore lie outside it, drawn without the test, rather
// than join the group and be tested against the group's depths.
#[test]
fn a_loose_depth_carrying_leaf_after_a_depth_group_lies_outside_it() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let blue = Rgba::new(0.0, 0.0, 1.0, 1.0);
    let drawn = draw_list(&list(vec![
        depth_group(vec![sourced(
            square_at(0.0, 0.0, 1.0, red, DepthPlane::constant(0.0)),
            1,
        )]),
        sourced(square_at(2.0, 0.0, 1.0, blue, DepthPlane::constant(0.0)), 2),
    ]));

    assert_eq!(
        drawn
            .draws
            .iter()
            .map(|d| (d.source, d.depth_group))
            .collect::<Vec<_>>(),
        [(Some(NodeId(1)), Some(0)), (Some(NodeId(2)), None)],
        "the group's draw is numbered and the loose leaf's is not"
    );
    assert!(
        all_at_z(&vertices_of_draw(&drawn, &drawn.draws[0]), 0.5),
        "the group normalises its one depth to the middle"
    );
    assert!(
        all_at_z(&vertices_of_draw(&drawn, &drawn.draws[1]), 0.0),
        "the loose leaf's depth is ignored and its z is 0"
    );
}

// Why: a face's depth varies across it and the depth test compares depths point by point, so z must be the plane
// at each vertex; larger depth is nearer, so the vertices at the greatest depth take z = 0 and those at the least
// z = 1. One z per item, or a plane read with the wrong sign, would let a tilted face cut wrongly through its
// neighbours.
#[test]
fn a_tilted_plane_gives_z_zero_where_the_depth_is_greatest_and_one_where_it_is_least() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let drawn = draw_list(&list(vec![depth_group(vec![square_at(
        0.0,
        0.0,
        10.0,
        red,
        DepthPlane {
            a: 0.1,
            b: 0.0,
            c: 0.0,
        },
    )])]));

    let vertices = vertices_of_draw(&drawn, &drawn.draws[0]);
    assert_span(
        span(&vertices, |v| v.pos[0]),
        (0.0, 10.0),
        "the square spans x = 0..10",
    );
    for v in &vertices {
        let expected = 1.0 - v.pos[0] / 10.0;
        assert!(
            (v.z - expected).abs() <= 1e-5,
            "z at x = {} is {expected}, the plane's depth 0.1·x normalised with the nearest at 0: got {}",
            v.pos[0],
            v.z
        );
    }
}

// Why: the stroke of a line lying on a plane (the edge of a face, a grid line on the floor of the box) is expanded
// by the shader from the depth at each end of every segment, so each end must take the plane at its own point, in
// item space (the compiler fits planes in item space beneath the group transform of a 3D axes), normalised over the
// group with the fills; and because the shader offsets the stroke's vertices from the polyline by half the width,
// the segment must also carry the gradient of z per item unit, the plane's tilt mapped through the group's
// normalisation, so that an offset vertex takes the plane at its own position rather than at the polyline's: a
// stroke on a steep plane would otherwise leave it by the tilt times half its width and cut into the face it
// outlines. The params leave the depth to the segments. Outside every depth group the plane is ignored, every end
// lies at z = 0 like every loose vertex, and there is no gradient.
#[test]
fn segments_take_the_plane_at_each_end_in_item_space_with_its_gradient_inside_a_depth_group_and_zero_outside()
 {
    // The polyline runs along y = 50 from x = 0 through x = 50 to x = 100 in item space; the plane rises by 0.008
    // per unit of x and by 0.002 per unit of y, so along the line its depth is 0.1, 0.5 and 0.9.
    let plane = DepthPlane {
        a: 0.008,
        b: 0.002,
        c: 0.0,
    };
    let line = || {
        sourced(
            with_depth(
                solid(polyline(&[(0.0, 50.0), (50.0, 50.0), (100.0, 50.0)])),
                Depth::Plane(plane),
            ),
            1,
        )
    };
    let transform = scale_then_translate(2.0, 2.0, 100.0, 200.0);
    let expected_segments = [
        segment(
            (0.0, 50.0),
            (0.0, 50.0),
            (50.0, 50.0),
            (100.0, 50.0),
            (false, true),
            (0.0, 50.0),
        ),
        segment(
            (0.0, 50.0),
            (50.0, 50.0),
            (100.0, 50.0),
            (100.0, 50.0),
            (true, false),
            (50.0, 100.0),
        ),
    ];
    // The depth range of the group, fixed by two pins at the depths 0 and `range`, over which z is
    // (range − depth) / range and the gradient of z is −(a, b) / range.
    for range in [1.0, 2.0] {
        let grouped = draw_list(&list(vec![group(
            None,
            Some(transform),
            vec![depth_group(vec![
                square_at(-4.0, -4.0, 1.0, Rgba::BLACK, DepthPlane::constant(0.0)),
                square_at(-2.0, -4.0, 1.0, Rgba::BLACK, DepthPlane::constant(range)),
                line(),
            ])],
        )]));

        let draw = draw_of(&grouped, 1);
        assert_eq!(
            draw.depth_group,
            Some(0),
            "the stroke lies in the depth group: {draw:?}"
        );
        let segments = segments_of(&grouped, draw);
        assert_segments(
            segments,
            &expected_segments,
            1e-4,
            "beneath the group transform the segments stay in item space",
        );
        let z = |depth: f64| (range - depth) / range;
        assert_z_pairs(
            segments,
            &[[z(0.1), z(0.5)], [z(0.5), z(0.9)]],
            1e-5,
            &format!(
                "with a depth range of {range}, each end takes the plane read at its own item-space point, 0 the \
                 nearest (a plane read at the figure position (100 + 2x, 200 + 2y) would give other values)"
            ),
        );
        let gradient = [-plane.a / range, -plane.b / range];
        for (k, s) in segments.iter().enumerate() {
            assert!(
                (f64::from(s.grad[0]) - gradient[0]).abs() <= 1e-6
                    && (f64::from(s.grad[1]) - gradient[1]).abs() <= 1e-6,
                "with a depth range of {range}, segment {k} carries the gradient of z per item unit of x and of \
                 y, −(a, b) / range = {gradient:?}: {s:?}"
            );
        }
        let params = params_of(&grouped, draw);
        assert_eq!(
            params.z, 0.0,
            "inside a depth group the params carry no depth of their own, the segments do: {params:?}"
        );
        assert_eq!(
            params.vertex_z, 1,
            "inside a depth group the params tell the shader to take the depth from the segments: {params:?}"
        );
        assert_eq!(
            params.offset,
            [100.0, 200.0, 0.0, 0.0],
            "the params carry the group's transform: {params:?}"
        );
    }

    let loose = draw_list(&list(vec![group(None, Some(transform), vec![line()])]));
    let draw = draw_of(&loose, 1);
    assert_eq!(
        draw.depth_group, None,
        "outside every depth group the stroke has no group: {draw:?}"
    );
    assert_z_pairs(
        segments_of(&loose, draw),
        &[[0.0, 0.0], [0.0, 0.0]],
        0.0,
        "outside every depth group the plane is ignored and every end lies at z = 0",
    );
    assert!(
        segments_of(&loose, draw)
            .iter()
            .all(|s| s.grad == [0.0, 0.0]),
        "outside every depth group no segment carries a gradient: {:?}",
        segments_of(&loose, draw)
    );
    assert_eq!(
        params_of(&loose, draw).vertex_z,
        0,
        "outside every depth group the params tell the shader to take their own z: {:?}",
        params_of(&loose, draw)
    );
}

// Why: the polylines of plot3, contour3 and quiver3 carry one depth per point, and a curve among them is flattened
// into chords whose interior points have no depth of their own; each segment end must take the depth of the point
// that made it, and a flattened point the depth interpolated between the curve's ends by its fraction of the
// flattened length, or a line would fight the surface it crosses wherever the depth fell at the wrong point along
// it. The range is pinned to [0, 1], so z is 1 − depth.
#[test]
fn segments_take_the_depth_of_the_vertex_at_each_end_and_interpolate_it_along_a_flattened_cubic() {
    // A polyline whose three points lie at the depths 0, 1 and 0.5, and an arch from a point at depth 0.2 to one
    // at 0.8.
    let polyline_item = sourced(
        with_depth(
            solid(polyline(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)])),
            Depth::Vertices(vec![0.0, 1.0, 0.5]),
        ),
        1,
    );
    let arch = sourced(
        with_depth(
            solid(vec![
                move_to(0.0, 150.0),
                PathSegment::CubicTo(
                    Point::new(30.0, 190.0),
                    Point::new(70.0, 190.0),
                    Point::new(100.0, 150.0),
                ),
            ]),
            Depth::Vertices(vec![0.2, 0.8]),
        ),
        2,
    );
    let [pin0, pin1] = range_pins(200.0, 0.0);
    let drawn = draw_list(&list(vec![depth_group(vec![
        pin0,
        pin1,
        polyline_item,
        arch,
    ])]));

    assert_z_pairs(
        segments_of(&drawn, draw_of(&drawn, 1)),
        &[[1.0, 0.0], [0.0, 0.5]],
        1e-5,
        "each end of a segment takes the depth of the vertex that made it",
    );

    let chords = segments_of(&drawn, draw_of(&drawn, 2));
    assert!(
        chords.len() >= 4,
        "the arch is flattened into several chords: {}",
        chords.len()
    );
    let length = f64::from(chords[chords.len() - 1].arc[1]);
    for (k, chord) in chords.iter().enumerate() {
        for (end, z, arc) in [
            ("start", chord.z[0], chord.arc[0]),
            ("end", chord.z[1], chord.arc[1]),
        ] {
            let depth = 0.2 + 0.6 * f64::from(arc) / length;
            let expected = 1.0 - depth;
            assert!(
                (f64::from(z) - expected).abs() <= 1e-3,
                "the {end} of chord {k}, at arc length {arc} of {length}, takes the depth {depth} interpolated \
                 between 0.2 and 0.8 by its fraction of the flattened length, z = {expected}: got {z}"
            );
        }
    }
    assert!(
        (chords[0].z[0] - 0.8).abs() <= 1e-5 && (chords[chords.len() - 1].z[1] - 0.2).abs() <= 1e-5,
        "the ends of the arch take the depths of its endpoints: {chords:?}"
    );
    let interior: Vec<f32> = chords.iter().skip(1).map(|chord| chord.z[0]).collect();
    assert!(
        !interior.is_empty() && interior.iter().all(|&z| z > 0.2 && z < 0.8),
        "every point made by the flattening lies strictly between the end depths: {interior:?}"
    );
    assert!(
        drawn.segments.iter().all(|s| s.grad == [0.0, 0.0]),
        "per-vertex depths vary along the line only, so no segment carries a gradient across it: {:?}",
        drawn.segments.iter().map(|s| s.grad).collect::<Vec<_>>()
    );
}

// Why: outside every depth group the stroke pipeline depth-tests with `Less` and writes the draw's own z, so that
// a translucent stroke never blends with itself where its parts overlap (as PDF paints a stroke once); consecutive
// stroke draws must therefore take strictly decreasing z, or a later stroke would fail the test against an earlier
// one and vanish where they cross, and the count restarts whenever the depth group changes, because the painter
// clears the depth buffer there, so that the values never run out. A fill between two strokes does not restart it.
// Inside a group the segments carry the depth and the params' z is 0 and unused.
#[test]
fn consecutive_stroke_draws_outside_depth_groups_take_strictly_decreasing_params_z_restarting_after_a_group()
 {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let line = |id: u64| sourced(solid(line_segments()), id);
    let [pin0, pin1] = range_pins(200.0, 0.0);
    let drawn = draw_list(&list(vec![
        line(1),
        filled(rect_segments(0.0, 0.0, 5.0, 5.0), red, FillRule::NonZero),
        line(2),
        depth_group(vec![
            pin0,
            pin1,
            sourced(
                with_depth(
                    solid(line_segments()),
                    Depth::Plane(DepthPlane::constant(0.5)),
                ),
                3,
            ),
        ]),
        line(4),
        line(5),
    ]));

    for (id, group_number) in [(1, None), (2, None), (3, Some(0)), (4, None), (5, None)] {
        let draw = draw_of(&drawn, id);
        assert_eq!(
            draw.depth_group, group_number,
            "the stroke of node {id} lies in the depth group {group_number:?}: {draw:?}"
        );
    }
    let z = |id: u64| params_of(&drawn, draw_of(&drawn, id)).z;
    assert_eq!(
        z(3),
        0.0,
        "inside the depth group the params carry no depth of their own: {:?}",
        params_of(&drawn, draw_of(&drawn, 3))
    );
    for (first, second) in [(1, 2), (4, 5)] {
        let (a, b) = (z(first), z(second));
        assert!(
            a > b && b > 0.0 && a < 1.0,
            "the strokes of nodes {first} and {second}, consecutive outside every depth group with only a fill \
             between them, take strictly decreasing z in (0, 1): {a} then {b}"
        );
    }
    assert_eq!(
        (z(4), z(5)),
        (z(1), z(2)),
        "the count restarts after the depth group: the strokes after it take the z of the strokes at the start of \
         the list"
    );
}

// Why: a filled polygon with one depth per vertex (a filled contour band of contour3, a patch of fill3) must give
// each corner its own depth and every other vertex the depth interpolated between the corners, so that the fill
// lies on the plane of its outline; a fill that took its first vertex's depth throughout, or lost the per-vertex
// depths in the tessellation, would lie flat at one depth and cut wrongly through what it meets.
#[test]
fn a_filled_triangle_takes_its_three_vertex_depths_at_its_corners() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let corners: [(f32, f32, f64); 3] = [(0.0, 0.0, 0.2), (10.0, 0.0, 0.9), (0.0, 10.0, 0.4)];
    let triangle = sourced(
        with_depth(
            filled(
                closed_polyline(&[(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)]),
                red,
                FillRule::NonZero,
            ),
            Depth::Vertices(corners.iter().map(|&(_, _, depth)| depth).collect()),
        ),
        1,
    );
    let [pin0, pin1] = range_pins(40.0, 40.0);
    let drawn = draw_list(&list(vec![depth_group(vec![pin0, pin1, triangle])]));

    let vertices = vertices_of_draw(&drawn, draw_of(&drawn, 1));
    assert!(
        vertices.len() >= 3,
        "the triangle is at least one triangle: {vertices:?}"
    );
    for (x, y, depth) in corners {
        let vertex = vertices
            .iter()
            .find(|v| (v.pos[0] - x).abs() <= 1e-3 && (v.pos[1] - y).abs() <= 1e-3)
            .unwrap_or_else(|| panic!("a vertex lies at the corner ({x}, {y}): {vertices:?}"));
        let expected = 1.0 - depth;
        assert!(
            (f64::from(vertex.z) - expected).abs() <= 1e-5,
            "the corner ({x}, {y}) of depth {depth} lies at z = {expected}, with the range pinned to [0, 1]: \
             {vertex:?}"
        );
    }
    for v in &vertices {
        // The depth is affine over the triangle: 0.2 at the origin, rising by 0.07 per unit of x and by 0.02 per
        // unit of y, which reaches 0.9 and 0.4 at the other two corners.
        let depth = 0.2 + 0.07 * f64::from(v.pos[0]) + 0.02 * f64::from(v.pos[1]);
        let expected = 1.0 - depth;
        assert!(
            (f64::from(v.z) - expected).abs() <= 1e-5,
            "z at ({}, {}) is {expected}, the depth interpolated between the corners: got {}",
            v.pos[0],
            v.pos[1],
            v.z
        );
    }
}

// Why: the compiler places the items of a 3D axes beneath a group transform but fits their planes in item space;
// a plane evaluated at the transformed position would tilt every face wrongly, which the pins expose because they
// fix the depth range the square's z is normalised over. A group's clip must reach the painter in figure points,
// as every other draw's does, or the scissor would cut the wrong region.
#[test]
fn a_plane_is_read_in_item_space_beneath_a_group_transform_and_the_clip_is_recorded_in_figure_points()
 {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let transform = scale_then_translate(2.0, 2.0, 100.0, 200.0);
    // In the parent (figure) space, around the square, which the transform places at (100..120, 200..220).
    let clip = Rect::new(90.0, 190.0, 40.0, 40.0);
    // In item space, at figure (92..94, 192..194) and (96..98, 192..194): inside the clip.
    let [pin0, pin1] = range_pins(-4.0, -4.0);
    let square = sourced(
        square_at(
            0.0,
            0.0,
            10.0,
            red,
            DepthPlane {
                a: 0.1,
                b: 0.0,
                c: 0.0,
            },
        ),
        5,
    );
    let drawn = draw_list(&list(vec![group(
        Some(clip),
        Some(transform),
        vec![depth_group(vec![pin0, pin1, square])],
    )]));

    for draw in &drawn.draws {
        assert_eq!(
            draw.clip,
            Some(clip),
            "the group's clip is recorded on the draw in figure points: {draw:?}"
        );
        assert_eq!(
            draw.depth_group,
            Some(0),
            "the leaves lie in the one depth group: {draw:?}"
        );
    }
    let vertices = vertices_of_draw(&drawn, draw_of(&drawn, 5));
    assert_span(
        span(&vertices, |v| v.pos[0]),
        (100.0, 120.0),
        "the square's corners land where the group transform puts them",
    );
    assert_span(
        span(&vertices, |v| v.pos[1]),
        (200.0, 220.0),
        "the square's corners land where the group transform puts them",
    );
    for v in &vertices {
        let item_x = (f64::from(v.pos[0]) - 100.0) / 2.0;
        let expected = 1.0 - item_x / 10.0;
        assert!(
            (f64::from(v.z) - expected).abs() <= 1e-4,
            "z at item x = {item_x} (figure x = {}) is {expected}, the plane read in item space: got {}",
            v.pos[0],
            v.z
        );
    }
}

// Why: an image inside the box of a 3D axes is a floor or a wall that faces cut through; it must reach the painter
// as a textured tile keyed by its own samples, in the group, with the depth of every corner read from its plane over
// pixel space, where the compiler fitted it. The pins fix the range to [0, 1], so a plane read over item space would
// put the corners at the wrong z.
#[test]
fn an_image_in_a_depth_group_is_one_textured_tile_whose_corners_take_the_plane_over_pixel_space() {
    let rect = Rect::new(3.0, 4.0, 20.0, 10.0);
    let samples: Arc<[u8]> = Arc::from(tagged_rgb(2, 1));
    let floor = Item {
        source: Some(NodeId(9)),
        kind: ItemKind::Image(ImageItem {
            rect,
            width: 2,
            height: 1,
            channels: ImageItem::RGB,
            samples: Arc::clone(&samples),
            depth: Some(DepthPlane {
                a: 0.5,
                b: 0.0,
                c: 0.0,
            }),
        }),
    };
    let [pin0, pin1] = range_pins(40.0, 40.0);
    let drawn = draw_list(&list(vec![depth_group(vec![pin0, pin1, floor])]));

    let draw = draw_of(&drawn, 9);
    assert_eq!(
        draw.depth_group,
        Some(0),
        "the tile lies in the depth group: {draw:?}"
    );
    assert_textured_quad(
        &drawn,
        draw,
        Transform::IDENTITY,
        rect,
        &tile_key(&samples, 2, ImageItem::RGB, 0..2, 0..1),
    );
    let vertices = vertices_of_draw(&drawn, draw);
    // A corner of the tile: its figure-space position, its z and its name.
    for ((x, y), z, corner) in [
        ((3.0, 4.0), 1.0, "top-left"),
        ((23.0, 4.0), 0.0, "top-right"),
        ((3.0, 14.0), 1.0, "bottom-left"),
        ((23.0, 14.0), 0.0, "bottom-right"),
    ] {
        let vertex = vertices
            .iter()
            .find(|vertex| (vertex.pos[0] - x).abs() <= 1e-3 && (vertex.pos[1] - y).abs() <= 1e-3)
            .unwrap_or_else(|| {
                panic!("the triangles refer to a vertex at the {corner} corner ({x}, {y}): {vertices:?}")
            });
        assert!(
            (vertex.z - z).abs() <= 1e-6,
            "the {corner} corner lies at z = {z}, the plane 0.5·x over the pixel columns 0..2 normalised with \
             the nearest at 0: {vertex:?}"
        );
    }
}

// Why: the display-list contract has a backend skip what it cannot draw rather than panic; a path whose per-vertex
// depths do not match its endpoints or whose plane is not finite has no depth to give its vertices or segment ends,
// a path or an image in a depth group without a depth has nothing to test, and a glyph run has no depth at all, so
// each must vanish without a draw of its own while its neighbours still draw.
#[test]
fn leaves_with_an_invalid_or_missing_depth_and_glyph_runs_are_skipped_inside_a_depth_group() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let short = sourced(
        with_depth(
            stroked_line(vec![], 0.0, LineCap::Butt),
            Depth::Vertices(vec![0.0]),
        ),
        2,
    );
    let unbounded = sourced(
        square_at(
            20.0,
            20.0,
            5.0,
            red,
            DepthPlane {
                a: f64::NAN,
                b: 0.0,
                c: 0.0,
            },
        ),
        3,
    );
    let depthless = sourced(
        image(
            Rect::new(30.0, 30.0, 4.0, 2.0),
            2,
            1,
            ImageItem::RGB,
            tagged_rgb(2, 1),
        ),
        4,
    );
    let depthless_path = sourced(
        filled(rect_segments(40.0, 40.0, 5.0, 5.0), red, FillRule::NonZero),
        6,
    );
    let depthless_stroke = sourced(solid(line_segments()), 8);
    let glyphs = sourced(glyph_h(Point::new(60.0, 60.0), 20.0, red), 7);
    let drawn = draw_list(&list(vec![depth_group(vec![
        sourced(square_at(0.0, 0.0, 5.0, red, DepthPlane::constant(0.0)), 1),
        short,
        unbounded,
        depthless,
        depthless_path,
        depthless_stroke,
        glyphs,
        sourced(square_at(10.0, 0.0, 5.0, red, DepthPlane::constant(1.0)), 5),
    ])]));

    assert_eq!(
        drawn.draws.iter().map(|d| d.source).collect::<Vec<_>>(),
        [Some(NodeId(1)), Some(NodeId(5))],
        "only the leaves with a usable depth are drawn"
    );
    assert!(
        drawn.draws.iter().all(|d| d.texture.is_none()),
        "no tile is drawn for the image without a depth: {:?}",
        drawn.draws
    );
}

// Why: the painter draws the list in sequence, so a depth group must sit between the draws of the items before and
// after it: the compiler draws the back of the box before the artists and the box's front edges after them.
#[test]
fn draws_keep_the_paint_order_around_a_depth_group() {
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    let drawn = draw_list(&list(vec![
        filled(rect_segments(0.0, 0.0, 5.0, 5.0), red, FillRule::NonZero),
        depth_group(vec![square_at(
            10.0,
            0.0,
            5.0,
            red,
            DepthPlane::constant(0.0),
        )]),
        filled(rect_segments(20.0, 0.0, 5.0, 5.0), red, FillRule::NonZero),
    ]));

    assert_eq!(
        drawn
            .draws
            .iter()
            .map(|d| d.depth_group)
            .collect::<Vec<_>>(),
        [None, Some(0), None],
        "the depth group sits between the draws of the items before and after it"
    );
    assert_rect_close(
        draw_bbox(&drawn, &drawn.draws[0]),
        bounds(0.0, 0.0, 5.0, 5.0),
        1e-3,
        "the first draw is the square before the group",
    );
    assert_rect_close(
        draw_bbox(&drawn, &drawn.draws[1]),
        bounds(10.0, 0.0, 15.0, 5.0),
        1e-3,
        "the second draw is the square of the group",
    );
    assert_rect_close(
        draw_bbox(&drawn, &drawn.draws[2]),
        bounds(20.0, 0.0, 25.0, 5.0),
        1e-3,
        "the third draw is the square after the group",
    );
}

// Why: what the tests above build by hand, the compiler emits for a three-dimensional axes; the faces of a surface
// must arrive as depth-tested draws of one group attributed to the surface, and everything else (the box, whose
// edges are strokes, the ticks and the labels) as draws outside any group, or a real figure would be drawn through
// the wrong path.
#[test]
fn a_compiled_three_dimensional_figure_gives_draws_in_a_depth_group_beside_draws_outside_it() {
    let scene = ironlab_scene::compile(&figure_with_surface(true), &TEXT);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let drawn = draw_list(&scene.display_list);

    let (grouped, loose): (Vec<&Draw>, Vec<&Draw>) = drawn
        .draws
        .iter()
        .partition(|draw| draw.depth_group.is_some());
    assert!(!grouped.is_empty(), "the surface's faces are drawn");
    assert!(
        grouped
            .iter()
            .all(|d| d.depth_group == Some(0) && d.source == Some(NodeId(3))),
        "every draw in a group is the surface's, in the one group: {grouped:?}"
    );
    let zs: Vec<f32> = grouped.iter().flat_map(|d| depths_of(&drawn, d)).collect();
    assert!(
        zs.iter().any(|z| z.abs() <= 1e-6) && zs.iter().any(|z| (z - 1.0).abs() <= 1e-6),
        "the nearest vertex or segment end of the group lies at z = 0 and the farthest at z = 1: {zs:?}"
    );
    assert!(
        !loose.is_empty(),
        "the box, the ticks and the labels are drawn outside the group"
    );
    assert!(
        loose
            .iter()
            .any(|d| matches!(d.kind, DrawKind::Stroke { .. })),
        "the box's edges are strokes outside the group: {loose:?}"
    );
    assert!(
        loose
            .iter()
            .flat_map(|d| depths_of(&drawn, d))
            .all(|z| z == 0.0),
        "every vertex and segment end outside the group lies at z = 0"
    );
}
