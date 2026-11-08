//! The depth information the display list carries for three-dimensional artists, the validity of
//! a markers item, of an image drawn through a mesh and of the sector of a sector clip, the
//! piecewise affine placement of such an image, and how the traversals present depth groups and
//! sector clips to backends.
//!
//! Display lists are built by hand here, because the contract under test belongs to the display
//! list itself (which depths and markers are valid, and what a backend sees of a depth group), not
//! to the scene compiler that produces it.

use std::sync::Arc;

use ironlab_scene::display::{
    Depth, DepthPlane, DisplayList, Fill, FillRule, ImageGeometry, ImageItem, ImageMesh, Item,
    ItemKind, MarkerInstance, MarkersItem, PathItem, PathSegment, Point, Rect, Rgba, Sector,
    Transform,
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
        geometry: ImageGeometry::Rect(Rect::new(0.0, 0.0, 2.0, 1.0)),
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
        geometry: ImageGeometry::Rect(Rect::new(0.0, 0.0, 0.0, 1.0)),
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

// ---------------------------------------------------------------------------------------------------------------
// Images drawn through a mesh
// ---------------------------------------------------------------------------------------------------------------

/// An arbitrary smooth map of pixel space that is far from affine, so that a node placed at the wrong index, or a
/// position interpolated between the wrong nodes, lands visibly elsewhere.
fn warp(u: f64, v: f64) -> Point {
    Point::new(3.0 + 10.0 * u * u, 20.0 * v + u)
}

/// A lattice with nodes at every pair of `u` and `v`, each node drawn at [`warp`] of its pixel-space position.
fn warped_mesh(u: &[f64], v: &[f64]) -> ImageMesh {
    let points = v
        .iter()
        .flat_map(|vj| u.iter().map(move |ui| warp(*ui, *vj)))
        .collect();
    ImageMesh {
        u: u.to_vec(),
        v: v.to_vec(),
        points,
        depths: None,
    }
}

/// An opaque image of `width` by `height` pixels drawn through `mesh`, without a depth plane.
fn mesh_image(width: u32, height: u32, mesh: ImageMesh) -> ImageItem {
    ImageItem {
        geometry: ImageGeometry::Mesh(mesh),
        width,
        height,
        channels: ImageItem::RGB,
        samples: Arc::from(vec![0u8; (width * height * 3) as usize]),
        depth: None,
    }
}

/// One lattice cell over the pixel space [0, 2] × [0, 1] drawn as a quadrilateral that is not a parallelogram, so
/// that the two triangles of the cell, an interpolation across the other diagonal and a bilinear interpolation all
/// place an interior point differently.
fn one_cell() -> (ImageMesh, [Point; 4]) {
    let corners = [
        Point::new(0.0, 0.0),
        Point::new(4.0, 0.0),
        Point::new(0.0, 2.0),
        Point::new(6.0, 3.0),
    ];
    let mesh = ImageMesh {
        u: vec![0.0, 2.0],
        v: vec![0.0, 1.0],
        points: corners.to_vec(),
        depths: None,
    };
    (mesh, corners)
}

#[track_caller]
fn assert_point_close(actual: Point, expected: Point, what: &str) {
    assert!(
        (actual.x - expected.x).abs() <= 1e-9 && (actual.y - expected.y).abs() <= 1e-9,
        "{what}: expected {expected:?}, got {actual:?}"
    );
}

// WHY: every backend draws a mesh image by the same piecewise affine rule (each lattice cell split along its
// diagonal from (u[i], v[j]) to (u[i+1], v[j+1]) into two affinely mapped triangles), and `locate` is the reference
// that the PDF outlines and the hit tests are built on, so it must reproduce that rule exactly: the nodes are drawn
// where the lattice says, a point in either triangle is the barycentric mix of that triangle's three nodes, and a
// point outside pixel space has no position. The quadrilateral of `one_cell` is not a parallelogram, so a bilinear
// interpolation (which would give (3.375, 0.6875) for the first interior point) or a split along the other diagonal
// would fail here.
#[test]
fn locate_maps_pixel_space_piecewise_affinely_through_the_two_triangles_of_each_cell() {
    let (mesh, [p00, p10, p01, p11]) = one_cell();
    for ((u, v), expected) in [
        ((0.0, 0.0), p00),
        ((2.0, 0.0), p10),
        ((0.0, 1.0), p01),
        ((2.0, 1.0), p11),
    ] {
        let found = mesh
            .locate(u, v)
            .unwrap_or_else(|| panic!("the corner ({u}, {v}) of pixel space has a position"));
        assert_point_close(found, expected, &format!("the corner ({u}, {v})"));
    }
    // (1.5, 0.25) lies below the diagonal, in the triangle of (0, 0), (2, 0) and (2, 1), with barycentric weights
    // 0.25, 0.5 and 0.25.
    let lower = 0.25 * p00.x + 0.5 * p10.x + 0.25 * p11.x;
    let lower = Point::new(lower, 0.25 * p00.y + 0.5 * p10.y + 0.25 * p11.y);
    assert_point_close(
        mesh.locate(1.5, 0.25).expect("inside pixel space"),
        lower,
        "a point in the lower triangle",
    );
    assert_point_close(lower, Point::new(3.5, 0.75), "the expectation itself");
    // (0.5, 0.75) lies above the diagonal, in the triangle of (0, 0), (2, 1) and (0, 1), with weights 0.25, 0.25 and
    // 0.5.
    let upper = Point::new(
        0.25 * p00.x + 0.25 * p11.x + 0.5 * p01.x,
        0.25 * p00.y + 0.25 * p11.y + 0.5 * p01.y,
    );
    assert_point_close(
        mesh.locate(0.5, 0.75).expect("inside pixel space"),
        upper,
        "a point in the upper triangle",
    );
    // A point on the diagonal is the same from either side.
    assert_point_close(
        mesh.locate(1.0, 0.5).expect("inside pixel space"),
        Point::new(0.5 * (p00.x + p11.x), 0.5 * (p00.y + p11.y)),
        "the middle of the diagonal",
    );
    for (u, v) in [(-0.1, 0.5), (2.1, 0.5), (1.0, -0.1), (1.0, 1.1)] {
        assert_eq!(
            mesh.locate(u, v),
            None,
            "({u}, {v}) lies outside pixel space"
        );
    }
}

// WHY: in a lattice of several cells, `locate` must pick the cell holding the point by the lattice values, not by
// equal division of pixel space; on a logarithmic axis the lattice lines are the pixel edges, unevenly drawn, and a
// point looked up in the wrong cell would be drawn in the wrong pixel. Every node of a warped lattice is found at
// its own drawn position, including those in the interior and on the far edges.
#[test]
fn locate_finds_every_node_of_a_lattice_of_unequal_cells() {
    let (u, v) = ([0.0, 0.5, 1.0, 2.0], [0.0, 0.5, 1.0, 2.0]);
    let mesh = warped_mesh(&u, &v);
    for vj in v {
        for ui in u {
            let found = mesh
                .locate(ui, vj)
                .unwrap_or_else(|| panic!("the node ({ui}, {vj}) has a position"));
            assert_point_close(found, warp(ui, vj), &format!("the node ({ui}, {vj})"));
        }
    }
    // A node lies in every cell around it, so the nodes alone cannot tell a wrong cell from the right one; an interior
    // point can. (1.5, 1.25) lies in the last cell [1, 2] × [1, 2], below its diagonal, in the triangle of (1, 1),
    // (2, 1) and (2, 2) with barycentric weights 0.5, 0.25 and 0.25; extrapolating from any other cell, or an equal
    // division of pixel space into three cells, places it elsewhere under the warp.
    let (a, b, c) = (warp(1.0, 1.0), warp(2.0, 1.0), warp(2.0, 2.0));
    assert_point_close(
        mesh.locate(1.5, 1.25).expect("inside pixel space"),
        Point::new(
            0.5 * a.x + 0.25 * b.x + 0.25 * c.x,
            0.5 * a.y + 0.25 * b.y + 0.25 * c.y,
        ),
        "an interior point of the last cell",
    );
}

// WHY: the PDF exporter writes a mesh image as one filled path per pixel built from `pixel_outline`, so the outline
// must follow the lattice: a pixel that spans several lattice cells has a kink at every lattice node on its
// boundary, and an outline through its four corners alone would cut across the warped cells and leave gaps or
// overlaps between neighbouring pixels. The order is part of the contract, so that neighbouring outlines share
// their boundary points and a fill rule sees a consistent winding: it starts at the (column, row) corner, runs along
// increasing u first and is not closed by repeating its first point.
#[test]
fn a_pixel_outline_passes_through_every_lattice_node_on_its_boundary_in_order() {
    let mesh = warped_mesh(&[0.0, 0.5, 1.0, 2.0], &[0.0, 0.5, 1.0, 2.0]);
    let expected =
        |nodes: &[(f64, f64)]| -> Vec<Point> { nodes.iter().map(|(u, v)| warp(*u, *v)).collect() };
    /// A pixel by its column and row, and the pixel-space positions its outline passes through, in order.
    type Case = ((u32, u32), Vec<(f64, f64)>);
    let cases: [Case; 2] = [
        (
            (0, 0),
            vec![
                (0.0, 0.0),
                (0.5, 0.0),
                (1.0, 0.0),
                (1.0, 0.5),
                (1.0, 1.0),
                (0.5, 1.0),
                (0.0, 1.0),
                (0.0, 0.5),
            ],
        ),
        ((1, 1), vec![(1.0, 1.0), (2.0, 1.0), (2.0, 2.0), (1.0, 2.0)]),
    ];
    for ((column, row), nodes) in cases {
        let outline = mesh.pixel_outline(column, row);
        let wanted = expected(&nodes);
        assert_eq!(
            outline.len(),
            wanted.len(),
            "pixel ({column}, {row}): {outline:?}"
        );
        for (k, (found, want)) in outline.iter().zip(&wanted).enumerate() {
            assert_point_close(
                *found,
                *want,
                &format!("point {k} of the outline of pixel ({column}, {row})"),
            );
        }
    }
}

// WHY: when the compiler thins a lattice, several pixels share one lattice cell, and the outline of such a pixel has
// no node of its own: its corners must then be placed by `locate` inside the cell, or the pixels sharing it would
// all be drawn over the whole cell. The outline still starts at the (column, row) corner and visits the other
// three corners in order along increasing u first.
#[test]
fn a_pixel_inside_one_lattice_cell_takes_its_corners_from_locate() {
    let (mesh, _) = one_cell();
    // Pixel (1, 0) of the 2 by 1 image covers [1, 2] × [0, 1], the right half of the one cell.
    let outline = mesh.pixel_outline(1, 0);
    let corners = [(1.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0)]
        .map(|(u, v)| mesh.locate(u, v).expect("inside pixel space"));
    assert_point_close(
        outline[0],
        corners[0],
        "the outline starts at the (column, row) corner",
    );
    let mut next = 1;
    for (k, corner) in corners.iter().enumerate().skip(1) {
        let at = outline[next..]
            .iter()
            .position(|p| (p.x - corner.x).abs() <= 1e-9 && (p.y - corner.y).abs() <= 1e-9)
            .unwrap_or_else(|| panic!("corner {k} {corner:?} follows in order in {outline:?}"));
        next += at + 1;
    }
    assert!(
        (outline.last().unwrap().x - corners[0].x).abs() > 1e-9
            || (outline.last().unwrap().y - corners[0].y).abs() > 1e-9,
        "the outline is not closed by repeating its first point: {outline:?}"
    );
}

// WHY: backends trust a valid image item without checking it, and a mesh is only drawable when its lattice can be
// walked: `u` and `v` strictly increasing from 0 to the width and height (else cells would fold over or pixels fall
// outside), one finite point per node (else a triangle would index past the points, or be drawn at NaN), and depths,
// inside a depth group, of one finite value per node. An image carries its depth either as a plane over pixel space
// (a rectangle) or as node depths (a mesh), never both, so a mesh with a depth plane is invalid rather than
// ambiguous.
#[test]
fn a_mesh_image_is_valid_only_when_its_lattice_points_and_depths_are_usable() {
    let reference = || mesh_image(2, 1, warped_mesh(&[0.0, 1.0, 2.0], &[0.0, 1.0]));
    assert!(reference().is_valid(), "the reference mesh is valid");
    /// A description of the change, the edit that makes it to the reference item, and whether the item is still
    /// valid.
    type Case = (&'static str, fn(&mut ImageItem), bool);
    fn mesh(item: &mut ImageItem) -> &mut ImageMesh {
        match &mut item.geometry {
            ImageGeometry::Mesh(mesh) => mesh,
            ImageGeometry::Rect(_) => unreachable!("the reference is a mesh"),
        }
    }
    let cases: Vec<Case> = vec![
        (
            "one finite depth per node",
            |i| mesh(i).depths = Some(vec![0.0, 0.5, 1.0, 0.0, 0.5, 1.0]),
            true,
        ),
        (
            "a lattice of the outer edges only",
            |i| {
                *mesh(i) = warped_mesh(&[0.0, 2.0], &[0.0, 1.0]);
            },
            true,
        ),
        (
            "a u that repeats a value",
            |i| {
                *mesh(i) = warped_mesh(&[0.0, 1.0, 1.0, 2.0], &[0.0, 1.0]);
            },
            false,
        ),
        (
            "a u that runs backwards",
            |i| {
                let m = mesh(i);
                m.u = vec![0.0, 2.5, 2.0];
            },
            false,
        ),
        (
            "a u that stops short of the width",
            |i| {
                *mesh(i) = warped_mesh(&[0.0, 1.0, 1.5], &[0.0, 1.0]);
            },
            false,
        ),
        (
            "a v that does not start at 0",
            |i| {
                *mesh(i) = warped_mesh(&[0.0, 1.0, 2.0], &[0.25, 1.0]);
            },
            false,
        ),
        (
            "a single u",
            |i| {
                *mesh(i) = warped_mesh(&[0.0], &[0.0, 1.0]);
            },
            false,
        ),
        (
            "a missing point",
            |i| {
                mesh(i).points.pop();
            },
            false,
        ),
        (
            "an extra point",
            |i| mesh(i).points.push(Point::new(0.0, 0.0)),
            false,
        ),
        (
            "a NaN point",
            |i| mesh(i).points[4] = Point::new(f64::NAN, 1.0),
            false,
        ),
        (
            "an infinite point",
            |i| mesh(i).points[1] = Point::new(0.0, f64::INFINITY),
            false,
        ),
        (
            "depths one short",
            |i| mesh(i).depths = Some(vec![0.0; 5]),
            false,
        ),
        (
            "a NaN depth",
            |i| mesh(i).depths = Some(vec![0.0, 0.0, f64::NAN, 0.0, 0.0, 0.0]),
            false,
        ),
        (
            "a depth plane beside the mesh",
            |i| i.depth = Some(DepthPlane::constant(0.0)),
            false,
        ),
    ];
    for (what, edit, expected) in cases {
        let mut item = reference();
        edit(&mut item);
        assert_eq!(
            item.is_valid(),
            expected,
            "a mesh image with {what} is {}",
            if expected { "valid" } else { "invalid" }
        );
    }
}

// WHY: display lists are inspected through `Debug` in test failures and logs, and an image can hold millions of
// samples, so the geometry (which is what tells a reader where the image went) must be printed and the samples must
// not.
#[test]
fn the_debug_form_of_a_mesh_image_shows_its_geometry_and_not_its_samples() {
    let mut item = mesh_image(2, 1, warped_mesh(&[0.0, 1.0, 2.0], &[0.0, 1.0]));
    item.samples = Arc::from(vec![173u8; 6]);
    let printed = format!("{item:?}");
    assert!(printed.contains("Mesh"), "{printed}");
    assert!(
        !printed.contains("173"),
        "the samples are omitted: {printed}"
    );
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

/// A sector centred at (50, 50) with the given radii, start and sweep.
fn sector(inner: f64, outer: f64, start: f64, sweep: f64) -> Sector {
    Sector {
        centre: Point::new(50.0, 50.0),
        inner,
        outer,
        start,
        sweep,
    }
}

// WHY: every backend clips to a sector by building its outline (the PDF exporter as a clip path,
// the GPU as a test in the fragment shader), and both trust the compiler to emit only sectors that
// describe a region: a finite centre, radii with 0 ≤ inner < outer, and a sweep of more than
// nothing and at most a turn. A backend handed a sector with a NaN radius, an inner radius beyond
// the outer one or a sweep of several turns would draw nothing, everything, or garbage.
#[test]
fn a_sector_is_valid_only_with_a_finite_centre_ordered_radii_and_a_sweep_of_at_most_a_turn() {
    use std::f64::consts::{FRAC_PI_2, TAU};
    let valid = [
        ("a whole disc", sector(0.0, 40.0, 0.0, TAU)),
        ("an annulus", sector(10.0, 40.0, 1.0, TAU)),
        ("a quarter", sector(0.0, 40.0, FRAC_PI_2, FRAC_PI_2)),
        ("a start beyond a turn", sector(0.0, 40.0, 7.0, 1.0)),
        ("a negative start", sector(0.0, 40.0, -1.0, 1.0)),
    ];
    for (what, s) in valid {
        assert!(s.is_valid(), "{what} is valid: {s:?}");
    }
    let mut nan_centre = sector(0.0, 40.0, 0.0, TAU);
    nan_centre.centre = Point::new(f64::NAN, 50.0);
    let mut infinite_centre = sector(0.0, 40.0, 0.0, TAU);
    infinite_centre.centre = Point::new(50.0, f64::INFINITY);
    let invalid = [
        ("a centre that is not a number", nan_centre),
        ("an infinite centre", infinite_centre),
        ("a negative inner radius", sector(-1.0, 40.0, 0.0, TAU)),
        (
            "an inner radius equal to the outer",
            sector(40.0, 40.0, 0.0, TAU),
        ),
        (
            "an inner radius beyond the outer",
            sector(50.0, 40.0, 0.0, TAU),
        ),
        (
            "an infinite outer radius",
            sector(0.0, f64::INFINITY, 0.0, TAU),
        ),
        (
            "a radius that is not a number",
            sector(0.0, f64::NAN, 0.0, TAU),
        ),
        (
            "a start that is not finite",
            sector(0.0, 40.0, f64::NAN, 1.0),
        ),
        ("a zero sweep", sector(0.0, 40.0, 0.0, 0.0)),
        ("a negative sweep", sector(0.0, 40.0, 0.0, -1.0)),
        (
            "a sweep of more than a turn",
            sector(0.0, 40.0, 0.0, TAU + 1e-6),
        ),
        (
            "a sweep that is not a number",
            sector(0.0, 40.0, 0.0, f64::NAN),
        ),
    ];
    for (what, s) in invalid {
        assert!(!s.is_valid(), "{what} is invalid: {s:?}");
    }
}

// WHY: backends that draw through `visit_leaves` must still draw the artists of a polar axes, which
// lie inside a sector clip, in their place in the paint order and under the enclosing transform;
// a traversal that skipped the sector clip would leave every polar axes blank. Any rectangular
// clip it reports for them may be loosened to the sector's bounding box for culling, but never
// tightened inside it, or the backend would cut off part of the sector.
#[test]
fn visit_leaves_descends_into_a_sector_clip_without_cutting_into_the_sector() {
    use std::f64::consts::TAU;
    let s = sector(0.0, 30.0, 0.0, TAU);
    let list = display_list(vec![
        marker(1.0),
        item(ItemKind::SectorClip {
            sector: s,
            items: vec![marker(2.0), group(None, None, vec![marker(3.0)])],
        }),
        marker(4.0),
    ]);
    let seen = visited(&list);
    assert_eq!(
        seen.iter().map(|v| v.0).collect::<Vec<_>>(),
        [1.0, 2.0, 3.0, 4.0]
    );
    let bounds = Rect::new(20.0, 20.0, 60.0, 60.0);
    for (m, transform, clip) in &seen {
        assert_eq!(*transform, Transform::IDENTITY, "marker {m}");
        if let Some(c) = clip {
            assert!(
                c.x <= bounds.x + 1e-9
                    && c.y <= bounds.y + 1e-9
                    && c.right() >= bounds.right() - 1e-9
                    && c.bottom() >= bounds.bottom() - 1e-9,
                "marker {m}: the clip {c:?} contains the sector's bounding box {bounds:?}"
            );
        }
    }
}
