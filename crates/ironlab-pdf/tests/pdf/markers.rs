//! Marker items: the instances of one outline, each written as a path of its own, scaled to its size, moved to its
//! position, filled with its face and stroked with its edge; what an invalid item leaves on the page; how a marker
//! counts towards the bounds of a raster; and how the exporter's walkers treat a markers item as the leaf it is.
//!
//! The scene compiler emits the markers of an artist as one item whose outline is the marker of width 1 centred on
//! the origin, and the tests build such items by hand. Pages are rasterised at 72 dpi, so one pixel is one point and
//! pixel coordinates equal display-list coordinates; samples are taken at pixel centres well inside or outside the
//! shapes so that anti-aliasing does not affect them, and every geometric check runs against both engines.

use std::sync::Arc;

use ironlab_ir::NodeId;
use ironlab_pdf::raster::{DepthPolicy, Need, needs_rasteriser, rasterises_any};
use ironlab_pdf::{
    ExportWarningKind, PdfOptions, RasterOptions, RasterPolicy, render_display_list,
};
use ironlab_scene::display::{
    DisplayList, Item, ItemKind, MarkerInstance, MarkersItem, PathSegment, Point, Rect, Rgba,
};

use crate::common::*;
use crate::require_tools;

const BLUE_PX: [u8; 3] = [0, 0, 255];

/// The identifier of the three-dimensional axes whose depth group holds a scatter of markers.
const AXES: NodeId = NodeId(11);

/// What the stub paints for a render that holds a depth group, standing for a depth-tested picture.
const TESTED: [u8; 4] = [255, 0, 255, 255];
const TESTED_PX: [u8; 3] = [255, 0, 255];
/// What the stub paints for a render without one, standing for a painter's-order picture.
const PAINTED: [u8; 4] = [0, 255, 0, 255];

/// The distance of the Bézier control points from the ends of a quarter circle of unit radius.
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// The outline of a square of side 1 centred on the origin, so that an instance of size `s` covers `s` points.
fn square_outline() -> Arc<[PathSegment]> {
    Arc::from(rect_segments(Rect::new(-0.5, -0.5, 1.0, 1.0)))
}

/// The outline of a circle of diameter 1 centred on the origin, as the four cubic arcs the scene compiler builds.
fn circle_outline() -> Arc<[PathSegment]> {
    let r = 0.5;
    let k = KAPPA * r;
    let mut segments = vec![PathSegment::MoveTo(Point::new(r, 0.0))];
    for ((ax, ay), (bx, by), (ex, ey)) in [
        ((r, k), (k, r), (0.0, r)),
        ((-k, r), (-r, k), (-r, 0.0)),
        ((-r, -k), (-k, -r), (0.0, -r)),
        ((k, -r), (r, -k), (r, 0.0)),
    ] {
        segments.push(PathSegment::CubicTo(
            Point::new(ax, ay),
            Point::new(bx, by),
            Point::new(ex, ey),
        ));
    }
    segments.push(PathSegment::Close);
    Arc::from(segments)
}

/// An instance of width `size_pt` centred on `(x, y)`, at depth 0, with the given face and edge colours.
fn instance(
    x: f64,
    y: f64,
    size_pt: f64,
    face: Option<Rgba>,
    edge: Option<Rgba>,
) -> MarkerInstance {
    MarkerInstance {
        position: Point::new(x, y),
        depth: 0.0,
        size_pt,
        face,
        edge,
        source_index: 0,
    }
}

/// A markers item of `outline`, with an edge `edge_width` wide, holding `instances`.
fn markers(outline: Arc<[PathSegment]>, edge_width: f64, instances: Vec<MarkerInstance>) -> Item {
    item(ItemKind::Markers(MarkersItem {
        outline,
        edge_width,
        instances,
    }))
}

/// What is wrong with a markers item, its outline, its edge width and the instance it holds beside a valid sibling.
type InvalidMarkers = (&'static str, Arc<[PathSegment]>, f64, MarkerInstance);

/// The depth group of the axes `AXES`, holding `items`.
fn depth_group(items: Vec<Item>) -> Item {
    Item {
        source: Some(AXES),
        kind: ItemKind::Depth { items },
    }
}

/// A 200 by 100 point page holding `items`.
fn holding(items: Vec<Item>) -> DisplayList {
    let mut list = page(200.0, 100.0);
    list.items = items;
    list
}

/// Options that rasterise whatever is marked dense, and verify depth groups, at 72 dots per inch.
fn rasterising(policy: RasterPolicy) -> PdfOptions {
    PdfOptions {
        raster: RasterOptions {
            policy,
            dpi: 72.0,
            depth: DepthPolicy::Auto,
        },
        ..PdfOptions::default()
    }
}

/// The rectangle of figure space that a planned raster covers, read back from the list handed to the rasteriser:
/// the list is the size of the rectangle, and its root group's transform moves the rectangle's corner to the
/// origin, so the corner is what that transform subtracts.
fn planned_rect(planned: &DisplayList) -> Rect {
    let [
        Item {
            kind:
                ItemKind::Group {
                    transform: Some(transform),
                    ..
                },
            ..
        },
    ] = planned.items.as_slice()
    else {
        panic!(
            "a planned list has one root group with a transform, not {:?}",
            planned.items
        );
    };
    Rect::new(
        -transform.e,
        -transform.f,
        planned.width_pt,
        planned.height_pt,
    )
}

/// Reports whether two rectangles agree to within a millionth of a point on every side.
fn same_rect(a: Rect, b: Rect) -> bool {
    [a.x - b.x, a.y - b.y, a.width - b.width, a.height - b.height]
        .iter()
        .all(|d| d.abs() < 1e-6)
}

// WHY: the scene compiler emits every marker of an artist as one instance of a shared unit outline, so a figure with
// markers is right only if the exporter scales the outline by the instance's size about its position and fills it
// with that instance's face colour. An exporter that wrote the unit outline unscaled and unmoved, scaled it by the
// size rather than the half size, or filled it with a default colour fails one of the samples across the square.
#[test]
fn a_filled_square_marker_covers_its_size_about_its_position_in_its_face_colour() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("markers-filled-square");
    let text = engine();
    let mut list = page(100.0, 100.0);
    // A square of side 10 centred on (50, 50) covers x and y in [45, 55].
    list.items.push(markers(
        square_outline(),
        1.0,
        vec![instance(50.0, 50.0, 10.0, Some(RED), None)],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            50.5,
            50.5,
            RED_PX,
            3,
            "the centre of the marker",
        );
        assert_pixel(
            engine,
            &image,
            46.5,
            46.5,
            RED_PX,
            3,
            "inside its top-left corner",
        );
        assert_pixel(
            engine,
            &image,
            53.5,
            53.5,
            RED_PX,
            3,
            "inside its bottom-right corner",
        );
        for (x, y, what) in [
            (42.5, 50.5, "8 points left of the centre"),
            (58.5, 50.5, "8 points right of the centre"),
            (50.5, 42.5, "8 points above the centre"),
            (50.5, 58.5, "8 points below the centre"),
            (
                0.5,
                0.5,
                "where the unit outline would lie unscaled and unmoved",
            ),
        ] {
            assert_pixel(engine, &image, x, y, WHITE_PX, 3, what);
        }
    }
}

// WHY: a marker with an edge and no face is a ring whose thickness is the item's edge width in points, not scaled
// by the instance's size: a size of 20 with an edge of 2 is a ring of radius 10 and thickness 2. An exporter that
// filled the outline whatever the face, scaled the edge with the size (which would ink the centre) or stroked the
// unit outline (a dot) fails one of the samples on and off the rim.
#[test]
fn an_edge_only_circle_marker_inks_its_rim_and_not_its_centre() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("markers-ring");
    let text = engine();
    let mut list = page(100.0, 100.0);
    // A circle of diameter 20 centred on (50, 50) with an edge 2 wide: the ring covers radii 9 to 11.
    list.items.push(markers(
        circle_outline(),
        2.0,
        vec![instance(50.0, 50.0, 20.0, None, Some(Rgba::BLACK))],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        // A pixel whose centre is 9.5 points from the marker's centre along an axis lies between radii 9 and 10.05.
        for (x, y, what) in [
            (40.5, 50.5, "the rim to the left of the centre"),
            (59.5, 50.5, "the rim to the right"),
            (50.5, 40.5, "the rim above"),
            (50.5, 59.5, "the rim below"),
        ] {
            assert_pixel(engine, &image, x, y, BLACK_PX, 3, what);
        }
        for (x, y, what) in [
            (50.5, 50.5, "the centre, which no face fills"),
            (50.5, 45.5, "inside the ring"),
            (50.5, 36.5, "outside the ring, 13 points from the centre"),
        ] {
            assert_pixel(engine, &image, x, y, WHITE_PX, 3, what);
        }
    }
}

// WHY: a scatter colours each point by its own data, so face and edge belong to the instance, not to the item: two
// markers of one item must ink their own colours, an instance without an edge gets none however wide the item's
// edge is, and where an instance has both, the edge is stroked over the face as it is for a path with both. An
// exporter that took the colours of the first instance for all, stroked every instance, or drew the face over the
// edge fails a sample.
#[test]
fn each_instance_inks_its_own_face_and_edge_colours() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("markers-colours");
    let text = engine();
    let mut list = page(100.0, 100.0);
    list.items.push(markers(
        square_outline(),
        4.0,
        vec![
            // Squares of side 10 covering [15, 25] and [45, 55] across.
            instance(20.0, 50.0, 10.0, Some(RED), None),
            instance(50.0, 50.0, 10.0, Some(BLUE), None),
            // A square of side 20 covering [70, 90] whose edge, 4 wide, covers [68, 72] and [88, 92] on each side.
            instance(80.0, 50.0, 20.0, Some(RED), Some(BLUE)),
        ],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        for (x, y, expected, what) in [
            (
                20.5,
                50.5,
                RED_PX,
                "the first instance in its own face colour",
            ),
            (
                50.5,
                50.5,
                BLUE_PX,
                "the second instance in its own face colour",
            ),
            (
                13.5,
                50.5,
                WHITE_PX,
                "beside the first instance, which has no edge to stroke",
            ),
            (80.5, 50.5, RED_PX, "the face of the third instance"),
            (
                70.5,
                50.5,
                BLUE_PX,
                "the left edge of the third instance, stroked over its face",
            ),
            (89.5, 50.5, BLUE_PX, "its right edge"),
            (80.5, 40.5, BLUE_PX, "its top edge"),
        ] {
            assert_pixel(engine, &image, x, y, expected, 3, what);
        }
    }
}

// WHY: an instance with neither a face nor an edge is a valid instance that asks for nothing to be drawn, so the
// exporter must write nothing for it, neither a path without paint nor a default black fill, and must draw the
// item's other instances as usual; an exporter that dropped the whole item on meeting it, or painted it anyway,
// fails one of the two checks. This is not the skipping of an invalid instance: an invalid instance makes its
// whole item invalid, and the item then draws nothing at all.
#[test]
fn a_valid_instance_that_asks_for_nothing_draws_nothing_while_its_sibling_is_drawn() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("markers-no-paint");
    let text = engine();
    let mut list = page(100.0, 100.0);
    list.items.push(markers(
        square_outline(),
        2.0,
        vec![
            instance(30.0, 50.0, 20.0, None, None),
            instance(70.0, 50.0, 20.0, Some(RED), None),
        ],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        let ink = count_pixels(&image, Rect::new(10.0, 30.0, 40.0, 40.0), is_ink);
        assert_eq!(
            ink, 0,
            "{engine:?}: the instance without paint left {ink} ink pixels around (30, 50)"
        );
        assert_pixel(
            engine,
            &image,
            70.5,
            50.5,
            RED_PX,
            3,
            "the instance beside it is drawn",
        );
    }
}

// WHY: the compiler strokes every marker edge with round joins, as it strokes every line, so that the viewer and the
// PDF agree on the corners of a square or diamond marker, and the display list carries no join for a markers item,
// so the exporter must supply it. On a thick-edged square a miter join spikes each corner beyond the round one and a
// bevel cuts it short, and two samples on the diagonal of one corner tell the three apart.
#[test]
fn the_edge_of_a_marker_is_stroked_with_round_joins() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("markers-joins");
    let text = engine();
    let mut list = page(100.0, 100.0);
    // A square of side 60 centred on (50, 50), so with its bottom-right vertex at (80, 80), and an edge 20 wide: a
    // round join fills the disc of radius 10 about the vertex, a miter join the square from (70, 70) to (90, 90),
    // and a bevel cuts the corner along the line from (90, 80) to (80, 90).
    list.items.push(markers(
        square_outline(),
        20.0,
        vec![instance(50.0, 50.0, 60.0, None, Some(Rgba::BLACK))],
    ));

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        assert_pixel(
            engine,
            &image,
            86.5,
            86.5,
            BLACK_PX,
            3,
            "within 9.9 points of the vertex on its diagonal: inside the round join and beyond a bevel",
        );
        assert_pixel(
            engine,
            &image,
            88.5,
            88.5,
            WHITE_PX,
            3,
            "over 11 points from the vertex on its diagonal: outside the round join and inside a miter",
        );
        assert_pixel(
            engine,
            &image,
            80.5,
            50.5,
            BLACK_PX,
            3,
            "the right side of the edge",
        );
        assert_pixel(
            engine,
            &image,
            50.5,
            50.5,
            WHITE_PX,
            3,
            "the centre, which no face fills",
        );
    }
}

// WHY: a NaN from user data or a list built by hand can make a markers item invalid, and the exporter must skip
// such an item whole rather than write a line without a current point, a negative line width or a non-finite
// coordinate into the content stream, which viewers reject or misdraw; it must neither fail the export nor stop
// drawing the items after it. What makes an item invalid is tabled against `MarkersItem::is_valid` in the scene
// tests, so one row of each kind is enough to show that the exporter honours it, and a valid sibling instance in
// every item shows that the item is skipped whole rather than instance by instance.
#[test]
fn an_invalid_markers_item_draws_nothing_and_the_rest_of_the_page_survives() {
    require_tools!("pdfinfo", RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("markers-invalid");
    let text = engine();
    let painted =
        |x: f64, size_pt: f64| instance(x, 50.0, size_pt, Some(Rgba::BLACK), Some(Rgba::BLACK));
    // What is wrong with the item, its outline, its edge width and the instance beside the valid sibling.
    let cases: [InvalidMarkers; 3] = [
        (
            "an outline that does not start with a move",
            Arc::from(vec![
                PathSegment::LineTo(Point::new(0.5, -0.5)),
                PathSegment::LineTo(Point::new(0.5, 0.5)),
                PathSegment::LineTo(Point::new(-0.5, 0.5)),
                PathSegment::Close,
            ]),
            1.0,
            painted(50.0, 20.0),
        ),
        (
            "a negative edge width",
            square_outline(),
            -1.0,
            painted(50.0, 20.0),
        ),
        (
            "an instance at a position that is not a number",
            square_outline(),
            1.0,
            painted(f64::NAN, 20.0),
        ),
    ];

    for (index, (name, outline, edge_width, bad)) in cases.into_iter().enumerate() {
        // The valid sibling: a square of side 20 about (80, 50), in the same item as the bad instance.
        let sibling = painted(80.0, 20.0);
        let mut list = page(200.0, 100.0);
        list.items
            .push(markers(outline, edge_width, vec![bad, sibling]));
        list.items
            .push(filled_rect(Rect::new(120.0, 20.0, 40.0, 40.0), BLUE));
        let rendered = render_display_list(&list, &text, &PdfOptions::default(), None)
            .unwrap_or_else(|error| panic!("{name}: the export failed: {error}"));
        let pdf = ws.write_pdf(&format!("figure-{index}"), &rendered.bytes);
        assert_eq!(pdfinfo(&pdf, false)["Pages"], "1", "{name}: page count");
        for engine in ENGINES {
            // `rasterise` fails on any error or warning either engine reports while reading the file.
            let image = rasterise(&pdf, engine);
            assert_pixel(
                engine,
                &image,
                80.5,
                50.5,
                WHITE_PX,
                3,
                &format!("{name}: the valid sibling, which the invalid item does not draw"),
            );
            let ink = count_pixels(&image, Rect::new(0.0, 0.0, 100.0, 100.0), is_ink);
            assert_eq!(
                ink, 0,
                "{engine:?}: {name}: the invalid item was drawn in part ({ink} ink pixels)"
            );
            assert_pixel(
                engine,
                &image,
                140.5,
                40.5,
                BLUE_PX,
                3,
                &format!("{name}: the rectangle after the invalid item"),
            );
        }
    }
}

// WHY: a raster must cover every marker of the dense artist it replaces, or the markers at the edge of a surface
// would be cut off; but a markers item is a list of centres, so the planner must grow each centre by half the
// instance's size times the outline's reach, control points included, and by the reach of the edge's joins when
// the instance has an edge. Comparing the planned rectangle with and without each marker, beside the square that
// anchors the group, pins each term of that growth and shows that a marker alone is enough to plan for.
#[test]
fn a_marker_inside_a_dense_group_grows_the_planned_raster_by_its_extent_and_edge_reach() {
    let anchor = || filled_rect(Rect::new(20.0, 10.0, 40.0, 20.0), RED);
    // An outline whose control points reach 1 from the origin while its end points reach 0.5.
    let bulge: Arc<[PathSegment]> = Arc::from(vec![
        PathSegment::MoveTo(Point::new(-0.5, 0.0)),
        PathSegment::CubicTo(
            Point::new(-0.5, -1.0),
            Point::new(0.5, -1.0),
            Point::new(0.5, 0.0),
        ),
        PathSegment::Close,
    ]);
    let filled = |x: f64, y: f64, size_pt: f64| instance(x, y, size_pt, Some(RED), None);
    let cases = [
        (
            "the anchoring square alone",
            vec![anchor()],
            Rect::new(20.0, 10.0, 40.0, 20.0),
        ),
        (
            "a filled square marker of size 20 alone, reaching 5 points from (100, 50)",
            vec![markers(
                square_outline(),
                2.0,
                vec![filled(100.0, 50.0, 20.0)],
            )],
            Rect::new(95.0, 45.0, 10.0, 10.0),
        ),
        (
            "the same marker beside the anchor, with no edge reach for an instance without an edge",
            vec![
                anchor(),
                markers(square_outline(), 2.0, vec![filled(100.0, 50.0, 20.0)]),
            ],
            Rect::new(20.0, 10.0, 85.0, 45.0),
        ),
        (
            "the same marker with an edge 2 wide, whose joins reach 4 points further",
            vec![
                anchor(),
                markers(
                    square_outline(),
                    2.0,
                    vec![instance(100.0, 50.0, 20.0, Some(RED), Some(RED))],
                ),
            ],
            Rect::new(20.0, 10.0, 89.0, 49.0),
        ),
        (
            "an outline whose control points reach 1, so that a size of 20 reaches 10 points",
            vec![
                anchor(),
                markers(bulge, 2.0, vec![filled(100.0, 50.0, 20.0)]),
            ],
            Rect::new(20.0, 10.0, 90.0, 50.0),
        ),
        (
            "two instances, each grown by its own size, snapped out to whole pixels",
            vec![
                anchor(),
                markers(
                    square_outline(),
                    2.0,
                    vec![filled(100.0, 50.0, 20.0), filled(150.0, 80.0, 10.0)],
                ),
            ],
            Rect::new(20.0, 10.0, 133.0, 73.0),
        ),
    ];

    for (name, items, expected) in cases {
        let list = holding(vec![dense(10_000, items)]);
        let (mut stub, calls) = Stub::flat([255, 0, 0, 255]);
        render_reporting(&list, &rasterising(RasterPolicy::Always), Some(&mut stub));
        let calls = calls.borrow();
        assert_eq!(calls.len(), 1, "{name}: one dense group, one render");
        let rect = planned_rect(&calls[0].0);
        assert!(
            same_rect(rect, expected),
            "{name}: the raster covers {rect:?}, expected {expected:?}"
        );
    }
}

// WHY: the markers of a three-dimensional scatter are all that its depth group holds, and the exporter decides
// whether a depth group shows anything from the bounds of its leaves. A planner that did not count markers would
// take such a group for one that shows nothing and draw it in order without asking, so a scatter whose markers
// interpenetrate would go to print unverified and unreported. The group must be rendered twice at the markers'
// bounds, the depth-tested render embedded when the two differ, and the finding reported, exactly as for faces.
#[test]
fn a_depth_group_holding_only_markers_is_verified_at_their_bounds_and_rasterised_when_the_renders_differ()
 {
    require_tools!(IMAGE_TOOL, RASTER_TOOLS[0]);
    let ws = Workspace::new("markers-depth");
    // A square of side 20 centred on (50, 50) with an edge 2 wide: its centre grown by 5 and by 4 more for the edge.
    let list = holding(vec![depth_group(vec![markers(
        square_outline(),
        2.0,
        vec![instance(50.0, 50.0, 20.0, Some(RED), Some(Rgba::BLACK))],
    )])]);
    let (mut stub, calls) = Stub::new(TESTED, PAINTED);
    let rendered = render_reporting(
        &list,
        &rasterising(RasterPolicy::default()),
        Some(&mut stub),
    );
    let pdf = ws.write_pdf("figure", &rendered.bytes);

    {
        let calls = calls.borrow();
        assert_eq!(
            calls.len(),
            2,
            "the group is rendered twice: with its depth test and without"
        );
        assert!(
            holds_depth(&calls[0].0) && !holds_depth(&calls[1].0),
            "the first render keeps the depth group and the second unwraps it: {calls:?}"
        );
        for (which, (planned, _)) in ["depth-tested", "painter's-order"]
            .into_iter()
            .zip(calls.iter())
        {
            let rect = planned_rect(planned);
            assert!(
                same_rect(rect, Rect::new(41.0, 41.0, 18.0, 18.0)),
                "the {which} render covers the marker grown by its reach and its edge's: {rect:?}"
            );
        }
    }
    assert_warnings(
        &rendered.warnings,
        &[(AXES, ExportWarningKind::RasterisedForDepth)],
        "under Auto with renders that differ",
    );
    let images = pdfimages(&pdf);
    assert_eq!(
        images.len(),
        1,
        "the depth-tested render is embedded: {images:?}"
    );
    let image = rasterise(&pdf, Engine::Poppler);
    assert_pixel(
        Engine::Poppler,
        &image,
        50.5,
        50.5,
        TESTED_PX,
        3,
        "the depth-tested render where the marker is",
    );
    assert_pixel(
        Engine::Poppler,
        &image,
        30.5,
        50.5,
        WHITE_PX,
        3,
        "the page beside the marker",
    );
}

// WHY: `rasterises_any` and `needs_rasteriser` tell a caller whether to look for a graphics adapter, and they
// answer by walking the list's groups down to its leaves. A markers item is a leaf of a new kind, and a walker that
// took it for a group, or stopped at it, would miss the dense group beside it or mistake the depth group of a
// scatter, which holds nothing but markers, for one with nothing to verify; the scatter must still be a check under
// `Auto` and a render under `Raster`, and a dense group of markers a render at the threshold.
#[test]
fn the_walkers_treat_a_markers_item_as_a_leaf_beneath_every_kind_of_group() {
    let scatter = || {
        markers(
            square_outline(),
            1.0,
            vec![instance(50.0, 50.0, 10.0, Some(RED), None)],
        )
    };
    let cases = [
        (
            "markers at the root",
            vec![scatter()],
            DepthPolicy::Auto,
            false,
            Need::No,
        ),
        (
            "markers inside a group",
            vec![group(None, None, vec![scatter()])],
            DepthPolicy::Auto,
            false,
            Need::No,
        ),
        (
            "a dense group of markers at the threshold",
            vec![dense(100, vec![scatter()])],
            DepthPolicy::Auto,
            true,
            Need::ToDraw,
        ),
        (
            "a dense group of markers below the threshold",
            vec![dense(99, vec![scatter()])],
            DepthPolicy::Auto,
            false,
            Need::No,
        ),
        (
            "a depth group holding only markers, left to the exporter",
            vec![depth_group(vec![scatter()])],
            DepthPolicy::Auto,
            false,
            Need::ToVerify,
        ),
        (
            "a depth group holding only markers, kept vector",
            vec![depth_group(vec![scatter()])],
            DepthPolicy::Vector,
            false,
            Need::No,
        ),
        (
            "a depth group holding only markers, rasterised by request",
            vec![depth_group(vec![scatter()])],
            DepthPolicy::Raster,
            false,
            Need::ToDraw,
        ),
        (
            "a depth group of markers inside a group",
            vec![group(None, None, vec![depth_group(vec![scatter()])])],
            DepthPolicy::Auto,
            false,
            Need::ToVerify,
        ),
        (
            "a dense group of markers at the threshold inside a depth group",
            vec![depth_group(vec![dense(100, vec![scatter()])])],
            DepthPolicy::Auto,
            true,
            Need::ToDraw,
        ),
        (
            "markers beside a dense group at the threshold",
            vec![
                scatter(),
                dense(
                    100,
                    vec![filled_rect(Rect::new(20.0, 10.0, 40.0, 20.0), RED)],
                ),
            ],
            DepthPolicy::Auto,
            true,
            Need::ToDraw,
        ),
    ];

    for (name, items, depth, rasterises, need) in cases {
        let list = holding(items);
        let options = RasterOptions {
            policy: RasterPolicy::Auto { cells: 100 },
            dpi: 72.0,
            depth,
        };
        assert_eq!(
            rasterises_any(&list, &options),
            rasterises,
            "{name}: whether any dense content becomes an image"
        );
        assert_eq!(
            needs_rasteriser(&list, &options),
            need,
            "{name}: what the export needs a rasteriser for"
        );
    }
}
