//! The depth information the display list carries for three-dimensional artists, the validity of
//! a markers item, and how the traversals present depth groups to backends.
//!
//! Display lists are built by hand here, because the contract under test belongs to the display
//! list itself (which depths and markers are valid, and what a backend sees of a depth group), not
//! to the scene compiler that produces it.

use std::sync::Arc;

use ironlab_scene::display::{
    Depth, DepthPlane, DisplayList, Fill, FillRule, ImageItem, Item, ItemKind, MarkerInstance,
    MarkersItem, PathItem, PathSegment, Point, Rect, Rgba, Transform,
};

#[track_caller]
fn assert_close(actual: f64, expected: f64, tol: f64) {
    assert!(
        (actual - expected).abs() <= tol,
        "expected {expected} ± {tol}, got {actual}"
    );
}

fn plane(a: f64, b: f64, c: f64) -> DepthPlane {
    DepthPlane { a, b, c }
}

fn item(kind: ItemKind) -> Item {
    Item { source: None, kind }
}

/// A path with every segment kind: a straight side, a curved side and a close back to the start.
fn every_segment_kind() -> Vec<PathSegment> {
    vec![
        PathSegment::MoveTo(Point::new(0.0, 0.0)),
        PathSegment::LineTo(Point::new(1.0, 0.0)),
        PathSegment::CubicTo(
            Point::new(1.0, 0.5),
            Point::new(0.5, 1.0),
            Point::new(0.0, 1.0),
        ),
        PathSegment::Close,
    ]
}

/// A black filled path with the given depth.
fn path(segments: Vec<PathSegment>, depth: Option<Depth>) -> PathItem {
    PathItem {
        segments,
        fill: Some(Fill {
            color: Rgba::BLACK,
            rule: FillRule::NonZero,
        }),
        stroke: None,
        depth,
    }
}

/// The outline of a unit square centred on the origin.
fn unit_square() -> Arc<[PathSegment]> {
    Arc::from(vec![
        PathSegment::MoveTo(Point::new(-0.5, -0.5)),
        PathSegment::LineTo(Point::new(0.5, -0.5)),
        PathSegment::LineTo(Point::new(0.5, 0.5)),
        PathSegment::LineTo(Point::new(-0.5, 0.5)),
        PathSegment::Close,
    ])
}

/// An opaque black marker four points wide at `position`, at depth 0, from the first point of its
/// artist.
fn instance(position: Point) -> MarkerInstance {
    MarkerInstance {
        position,
        depth: 0.0,
        size_pt: 4.0,
        face: Some(Rgba::BLACK),
        edge: Some(Rgba::BLACK),
        source_index: 0,
    }
}

/// A markers item of one unit square instance centred at `(x, 0)`, so that `marker_of` can tell
/// the leaves of a list apart after a traversal.
fn marker(x: f64) -> Item {
    item(ItemKind::Markers(MarkersItem {
        outline: unit_square(),
        edge_width: 0.5,
        instances: vec![instance(Point::new(x, 0.0))],
    }))
}

/// The `x` that a leaf was built with by `marker`.
fn marker_of(leaf: &Item) -> f64 {
    match &leaf.kind {
        ItemKind::Markers(markers) => match markers.instances.as_slice() {
            [only] => only.position.x,
            other => panic!("a marker item holds {} instances, not one", other.len()),
        },
        other => panic!("a leaf that is not a markers item: {other:?}"),
    }
}

/// A two-by-one opaque image with the given depth.
fn image(depth: Option<DepthPlane>) -> ImageItem {
    ImageItem {
        rect: Rect::new(0.0, 0.0, 2.0, 1.0),
        width: 2,
        height: 1,
        channels: ImageItem::RGB,
        samples: Arc::from(vec![0u8; 6]),
        depth,
    }
}

fn group(clip: Option<Rect>, transform: Option<Transform>, items: Vec<Item>) -> Item {
    item(ItemKind::Group {
        clip,
        transform,
        items,
    })
}

fn dense(items: Vec<Item>) -> Item {
    item(ItemKind::Dense { cells: 1, items })
}

fn depth_group(items: Vec<Item>) -> Item {
    item(ItemKind::Depth { items })
}

fn display_list(items: Vec<Item>) -> DisplayList {
    DisplayList {
        width_pt: 100.0,
        height_pt: 100.0,
        background: Rgba::WHITE,
        items,
    }
}

/// Every leaf of a list as `(marker, transform, clip)`, in paint order.
fn visited(list: &DisplayList) -> Vec<(f64, Transform, Option<Rect>)> {
    let mut out = Vec::new();
    list.visit_leaves(|leaf, transform, clip| out.push((marker_of(leaf), transform, clip)));
    out
}

/// Every leaf of a list as `(marker, transform, clip, depth group)`, in paint order.
fn visited_grouped(list: &DisplayList) -> Vec<(f64, Transform, Option<Rect>, Option<usize>)> {
    let mut out = Vec::new();
    list.visit_leaves_grouped(|leaf, transform, clip, group| {
        out.push((marker_of(leaf), transform, clip, group));
    });
    out
}

// WHY: a backend places every pixel of a face or an image in depth by evaluating its plane, so a
// plane that mixed up its coefficients or dropped the offset would sort a whole three-dimensional
// axes wrongly while each depth still looked plausible on its own.
#[test]
fn a_depth_plane_evaluates_the_affine_depth_at_a_point() {
    let tilted = plane(2.0, -3.0, 0.5);
    assert_close(tilted.at(Point::new(0.0, 0.0)), 0.5, 1e-12);
    assert_close(tilted.at(Point::new(1.0, 0.0)), 2.5, 1e-12);
    assert_close(tilted.at(Point::new(0.0, 1.0)), -2.5, 1e-12);
    assert_close(tilted.at(Point::new(2.0, -1.0)), 7.5, 1e-12);
}

// WHY: a constant plane is what everything without a tilt (an edge-on face) is given, so it must
// give the same depth everywhere in the item's space, not only at the origin.
#[test]
fn a_constant_depth_plane_has_the_same_depth_everywhere() {
    let constant = DepthPlane::constant(4.0);
    assert_eq!((constant.a, constant.b), (0.0, 0.0));
    for p in [
        Point::new(0.0, 0.0),
        Point::new(-30.0, 12.0),
        Point::new(1e6, -1e6),
    ] {
        assert_eq!(constant.at(p), 4.0);
    }
}

// WHY: the scene compiler pushes the fill of a face, and an image inside the axes box, a fixed
// distance behind its geometry, so that the face's own edge and the lines and markers lying on it
// win the depth test where they coincide; the push must move the whole plane by exactly that
// distance and leave its tilt alone, or a tilted face would be pushed unevenly and its edge would
// fight it at one end.
#[test]
fn pushing_a_plane_back_lowers_every_depth_by_the_distance_and_keeps_the_tilt() {
    let original = plane(2.0, -3.0, 0.5);
    let pushed = original.pushed_back(0.75);
    assert_eq!((pushed.a, pushed.b), (original.a, original.b));
    for p in [
        Point::new(0.0, 0.0),
        Point::new(1.0, 1.0),
        Point::new(-4.0, 2.5),
    ] {
        assert_close(pushed.at(p), original.at(p) - 0.75, 1e-12);
    }
}

// WHY: backends skip items whose depths are not finite rather than feed NaN into a depth test,
// where one NaN comparison could hide or reveal every later face; the check must catch every
// coefficient and both signs of infinity, not only NaN in the offset.
#[test]
fn a_depth_plane_is_finite_only_when_every_coefficient_is() {
    assert!(plane(1.0, -2.0, 3.0).is_finite());
    assert!(DepthPlane::constant(0.0).is_finite());
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(!plane(bad, 0.0, 0.0).is_finite(), "a = {bad}");
        assert!(!plane(0.0, bad, 0.0).is_finite(), "b = {bad}");
        assert!(!plane(0.0, 0.0, bad).is_finite(), "c = {bad}");
    }
}

// WHY: a vertex depth list is paired with the end points of the segments by position, so the
// end point count is what makes the pairing line up; counting `Close` (which ends where its
// subpath began) or the control points of a curve would shift every depth after it onto the
// wrong vertex, and a second subpath must keep counting rather than start afresh.
#[test]
fn endpoint_count_is_one_per_move_line_and_cubic_and_none_for_close() {
    assert_eq!(path(every_segment_kind(), None).endpoint_count(), 3);
    let lone_move = vec![PathSegment::MoveTo(Point::new(0.0, 0.0))];
    assert_eq!(path(lone_move, None).endpoint_count(), 1);
    assert_eq!(path(Vec::new(), None).endpoint_count(), 0);
    assert_eq!(path(vec![PathSegment::Close], None).endpoint_count(), 0);
    let two_subpaths = vec![
        PathSegment::MoveTo(Point::new(0.0, 0.0)),
        PathSegment::LineTo(Point::new(1.0, 0.0)),
        PathSegment::Close,
        PathSegment::MoveTo(Point::new(2.0, 0.0)),
        PathSegment::LineTo(Point::new(3.0, 0.0)),
        PathSegment::Close,
    ];
    assert_eq!(path(two_subpaths, None).endpoint_count(), 4);
}

// WHY: every path outside a three-dimensional axes has no depth, so `None` must pass, or nothing
// in a two-dimensional axes could be drawn; and a face's plane is usable only when it can be
// evaluated everywhere on the face, so one non-finite coefficient must fail validation.
#[test]
fn a_plane_depth_is_valid_only_when_the_plane_is_finite() {
    assert!(path(every_segment_kind(), None).is_valid_depth());
    let with = |c: f64| {
        path(
            every_segment_kind(),
            Some(Depth::Plane(plane(0.5, -0.5, c))),
        )
    };
    assert!(with(1.0).is_valid_depth());
    assert!(!with(f64::NAN).is_valid_depth());
    assert!(!with(f64::INFINITY).is_valid_depth());
}

// WHY: a vertex depth list is interpolated along the segments by pairing each value with the
// end point at the same position, so a list of any other length was built for a different path
// and cannot be trusted: a short list leaves the last segment without a depth and a long list
// hides a vertex the path does not have. Both must be rejected, not only the short one, and a
// list of the right length is still invalid when one of its values would be interpolated into
// the depths of both segments that meet at its vertex.
#[test]
fn vertex_depths_are_valid_only_with_one_finite_value_per_endpoint() {
    let with = |depths: Vec<f64>| path(every_segment_kind(), Some(Depth::Vertices(depths)));
    assert!(with(vec![0.0, 1.0, 2.0]).is_valid_depth());
    assert!(!with(vec![0.0, 1.0]).is_valid_depth(), "one too few");
    assert!(
        !with(vec![0.0, 1.0, 2.0, 3.0]).is_valid_depth(),
        "one too many"
    );
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            !with(vec![0.0, bad, 2.0]).is_valid_depth(),
            "a depth of {bad}"
        );
    }
}

// WHY: an image outside a three-dimensional axes has no depth, and giving images a depth must
// not change what is drawable: a well-formed image stays valid, and the rules that already
// existed (here, the sample count matching the grid and a positive rectangle) still reject a
// malformed one.
#[test]
fn an_image_without_depth_is_valid_by_the_existing_rules() {
    assert!(image(None).is_valid());
    let short_of_samples = ImageItem {
        samples: Arc::from(vec![0u8; 5]),
        ..image(None)
    };
    assert!(!short_of_samples.is_valid());
    let flat = ImageItem {
        rect: Rect::new(0.0, 0.0, 0.0, 1.0),
        ..image(None)
    };
    assert!(!flat.is_valid());
}

// WHY: an image on a wall of a three-dimensional axes is placed in depth by its plane, so a
// finite plane leaves it drawable and a non-finite one must make the whole item invalid, rather
// than leave a backend to draw it at an undefined depth.
#[test]
fn an_image_with_a_depth_plane_is_valid_only_when_the_plane_is_finite() {
    assert!(image(Some(plane(0.1, 0.2, -3.0))).is_valid());
    assert!(!image(Some(plane(f64::NAN, 0.0, 0.0))).is_valid());
    assert!(!image(Some(plane(0.0, 0.0, f64::NEG_INFINITY))).is_valid());
}

// WHY: a backend draws every instance of a markers item without checking it, scaling one outline
// by each size and filling and stroking it with each colour, so one bad value anywhere would
// poison every marker of the run: the outline must be a path it can walk (starting with `MoveTo`,
// with something after it, and finite), the edge width a usable stroke width, and every instance
// a finite position, a positive size, a depth a depth buffer can take and colours whose channels
// are numbers. An item with no instances is valid and draws nothing, and an instance with neither
// face nor edge is valid too, since it asks for nothing to be drawn.
#[test]
fn a_markers_item_is_valid_only_when_its_outline_its_edge_width_and_every_instance_are_usable() {
    let reference = || MarkersItem {
        outline: unit_square(),
        edge_width: 0.5,
        instances: vec![
            instance(Point::new(1.0, 2.0)),
            instance(Point::new(3.0, 4.0)),
        ],
    };
    assert!(reference().is_valid(), "the reference item is valid");
    /// A description of the change, the edit that makes it to the reference item, and whether the
    /// item is still valid.
    type Case = (&'static str, fn(&mut MarkersItem), bool);
    let cases: Vec<Case> = vec![
        ("no instances", |m| m.instances.clear(), true),
        ("a zero edge width", |m| m.edge_width = 0.0, true),
        (
            "an instance with neither face nor edge",
            |m| {
                m.instances[1].face = None;
                m.instances[1].edge = None;
            },
            true,
        ),
        (
            "an outline that starts with LineTo",
            |m| {
                let mut segments = m.outline.to_vec();
                segments[0] = PathSegment::LineTo(Point::new(-0.5, -0.5));
                m.outline = Arc::from(segments);
            },
            false,
        ),
        (
            "an outline of a lone MoveTo",
            |m| m.outline = Arc::from(vec![PathSegment::MoveTo(Point::new(0.0, 0.0))]),
            false,
        ),
        (
            "an empty outline",
            |m| m.outline = Arc::from(Vec::new()),
            false,
        ),
        (
            "an outline with a NaN vertex",
            |m| {
                let mut segments = m.outline.to_vec();
                segments[2] = PathSegment::LineTo(Point::new(f64::NAN, 0.5));
                m.outline = Arc::from(segments);
            },
            false,
        ),
        (
            "an outline with an infinite control point",
            |m| {
                let mut segments = m.outline.to_vec();
                segments[2] = PathSegment::CubicTo(
                    Point::new(0.5, f64::INFINITY),
                    Point::new(0.5, 0.5),
                    Point::new(0.5, 0.5),
                );
                m.outline = Arc::from(segments);
            },
            false,
        ),
        ("a negative edge width", |m| m.edge_width = -0.5, false),
        ("a NaN edge width", |m| m.edge_width = f64::NAN, false),
        (
            "an instance at a NaN position",
            |m| m.instances[1].position = Point::new(f64::NAN, 0.0),
            false,
        ),
        (
            "an instance at an infinite position",
            |m| m.instances[0].position = Point::new(0.0, f64::NEG_INFINITY),
            false,
        ),
        (
            "an instance of zero size",
            |m| m.instances[1].size_pt = 0.0,
            false,
        ),
        (
            "an instance of negative size",
            |m| m.instances[1].size_pt = -4.0,
            false,
        ),
        (
            "an instance of infinite size",
            |m| m.instances[1].size_pt = f64::INFINITY,
            false,
        ),
        (
            "an instance at a NaN depth",
            |m| m.instances[0].depth = f64::NAN,
            false,
        ),
        (
            "an instance at an infinite depth",
            |m| m.instances[1].depth = f64::INFINITY,
            false,
        ),
        (
            "an instance with a NaN face channel",
            |m| m.instances[1].face = Some(Rgba::new(0.0, f32::NAN, 0.0, 1.0)),
            false,
        ),
        (
            "an instance with a NaN edge alpha",
            |m| m.instances[0].edge = Some(Rgba::new(0.0, 0.0, 0.0, f32::NAN)),
            false,
        ),
    ];
    for (what, edit, expected) in cases {
        let mut item = reference();
        edit(&mut item);
        assert_eq!(
            item.is_valid(),
            expected,
            "an item with {what} is {}",
            if expected { "valid" } else { "invalid" }
        );
    }
}

// WHY: both backends draw leaves through `visit_leaves`, and a backend without a depth buffer
// must still draw everything in a three-dimensional axes, in its place in the paint order and
// under the transform and clip of the group enclosing it; a depth group carries neither of its
// own, exactly like a dense group, so a traversal that skipped it, or reset either at it, would
// leave the axes blank, or draw its contents at the figure origin, or unclipped.
#[test]
fn visit_leaves_carries_the_enclosing_transform_and_clip_through_a_depth_group() {
    let clip = Rect::new(10.0, 10.0, 100.0, 50.0);
    let transform = Transform::translate(5.0, 7.0);
    let list = display_list(vec![group(
        Some(clip),
        Some(transform),
        vec![
            marker(1.0),
            depth_group(vec![marker(2.0), dense(vec![marker(3.0)])]),
            marker(4.0),
        ],
    )]);
    let expected: Vec<_> = [1.0, 2.0, 3.0, 4.0]
        .into_iter()
        .map(|m| (m, transform, Some(clip)))
        .collect();
    assert_eq!(visited(&list), expected);
}

// WHY: the promise to a backend that ignores depth is that it sees exactly what it would see if
// the depth group were a dense group; comparing the two traversals of the same nesting pins
// that equivalence for groups and dense groups inside the depth group too, which the case
// above spells out only for the outside.
#[test]
fn a_depth_group_is_traversed_exactly_like_a_dense_group() {
    let nested = |wrap: fn(Vec<Item>) -> Item| {
        display_list(vec![group(
            Some(Rect::new(0.0, 0.0, 40.0, 40.0)),
            Some(Transform::translate(3.0, -2.0)),
            vec![
                marker(1.0),
                wrap(vec![
                    marker(2.0),
                    group(
                        Some(Rect::new(5.0, 5.0, 10.0, 10.0)),
                        Some(Transform::translate(1.0, 1.0)),
                        vec![marker(3.0)],
                    ),
                    dense(vec![marker(4.0)]),
                ]),
                marker(5.0),
            ],
        )])
    };
    let through_depth = visited(&nested(depth_group));
    assert_eq!(through_depth.len(), 5);
    assert_eq!(through_depth, visited(&nested(dense)));
}

// WHY: each three-dimensional axes is one depth group and a backend keeps one depth buffer per
// group, so the group number is what tells it when to start afresh, and a leaf outside every
// depth group must report none, or a backend would depth-test two-dimensional axes. The groups
// must be numbered in paint order across the whole list however deeply each is nested (inside a
// plain group, or inside a dense group as the PDF exporter's rendering wraps one); a group or a
// dense group inside a depth group must pass the enclosing number on rather than start one of
// its own; and an empty depth group takes a number like any other, so that the numbering is a
// function of the list's structure alone.
#[test]
fn depth_groups_are_numbered_in_paint_order_across_nesting_levels() {
    let list = display_list(vec![
        marker(1.0),
        dense(vec![marker(2.0)]),
        depth_group(vec![
            marker(3.0),
            group(
                None,
                Some(Transform::translate(1.0, 1.0)),
                vec![marker(4.0)],
            ),
        ]),
        group(
            None,
            None,
            vec![
                marker(5.0),
                depth_group(vec![marker(6.0), dense(vec![marker(7.0)])]),
            ],
        ),
        depth_group(Vec::new()),
        dense(vec![depth_group(vec![marker(8.0)])]),
        marker(9.0),
    ]);
    let groups: Vec<(f64, Option<usize>)> = visited_grouped(&list)
        .into_iter()
        .map(|(m, _, _, g)| (m, g))
        .collect();
    assert_eq!(
        groups,
        [
            (1.0, None),
            (2.0, None),
            (3.0, Some(0)),
            (4.0, Some(0)),
            (5.0, None),
            (6.0, Some(1)),
            (7.0, Some(1)),
            (8.0, Some(3)),
            (9.0, None),
        ]
    );
}

// WHY: `visit_leaves_grouped` is `visit_leaves` with one more piece of information, so the two
// must agree on which leaves are visited, in what order, and with what transform and clip; a
// backend that moves from one to the other to gain depth must not see a different picture.
#[test]
fn visit_leaves_grouped_agrees_with_visit_leaves_on_order_transform_and_clip() {
    let list = display_list(vec![
        marker(1.0),
        group(
            Some(Rect::new(0.0, 0.0, 40.0, 40.0)),
            Some(Transform::translate(5.0, 7.0)),
            vec![
                marker(2.0),
                depth_group(vec![
                    marker(3.0),
                    dense(vec![marker(4.0)]),
                    group(
                        Some(Rect::new(2.0, 2.0, 10.0, 10.0)),
                        None,
                        vec![marker(5.0)],
                    ),
                ]),
                marker(6.0),
            ],
        ),
        depth_group(vec![marker(7.0)]),
    ]);
    let plain = visited(&list);
    let grouped: Vec<_> = visited_grouped(&list)
        .into_iter()
        .map(|(m, t, c, _)| (m, t, c))
        .collect();
    assert_eq!(plain.len(), 7);
    assert_eq!(grouped, plain);
}
