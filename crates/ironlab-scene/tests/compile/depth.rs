//! Depth groups: the one group a three-dimensional axes wraps its artists in for a backend with a depth buffer, the
//! depth every path and image inside it carries, and the painter's order those depths induce.
//!
//! Expected depths are rebuilt from the public camera: with manual limits [`LO`] to [`HI`], a data point is
//! normalised by `normalise_box` and its depth is the `depth` that `Camera::project` reports at the default view,
//! larger nearer the viewer. The paths of a 3D axes are expressed in figure space, the axes group carrying no
//! transform, so a path's depth plane is evaluated at the path's own vertices; the plane of an image is over the
//! image's pixel space, the group above it carrying the placement.

use ironlab_ir::{
    Artist, ColorSpec, ContourPlacement, ImagePlane, Levels, Limits, MarkerShape, NodeId,
    QuiverScale, ScatterColor, Surface, View3d,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{
    Depth, DepthPlane, Item, ItemKind, MarkerInstance, PathSegment, Point,
};
use ironlab_scene::maths::camera::{
    Camera, EDGE_DEPTH_LIFT, FACE_DEPTH_BIAS, depth_plane, fit_to_rect, normalise_box,
};
use ironlab_scene::maths::decimate::Sample;
use ironlab_scene::maths::quiver::arrow;

use crate::common::{Fx, compile_figure, linspace, placement, range};
use crate::probe::{
    Leaf, assert_close, axes_hit, from_source, grouped_leaves, leaves, marker_instances,
    points_close,
};

const LO: [f64; 3] = [-1.0; 3];
const HI: [f64; 3] = [1.0; 3];

/// Sets manual limits of [−1, 1] on every axis of a 3D axes, so that depths can be rebuilt from the camera.
fn manual_unit_limits(fx: &mut Fx, ax: NodeId) {
    let axes = fx.ax(ax);
    for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
        axis.limits = Limits::Manual {
            min: -1.0,
            max: 1.0,
        };
    }
}

/// The depth at the default view of a data point of the box [`LO`] to [`HI`].
fn depth_at(p: [f64; 3]) -> f64 {
    Camera::default()
        .project(normalise_box(p, LO, HI, [false; 3]))
        .depth
}

/// The projection of a 3D axes at the default view with limits [`LO`] to [`HI`], rebuilt from the public camera and
/// the documented fit, giving the figure-space position and the depth of a data point.
fn projection(scene: &Scene, ax: NodeId) -> impl Fn([f64; 3]) -> (Point, f64) {
    let plot = axes_hit(scene, ax).plot_rect;
    let (scale, offset) = fit_to_rect(plot.width, plot.height);
    let origin = Point::new(plot.x + offset[0], plot.y + offset[1]);
    move |p: [f64; 3]| {
        let projected = Camera::default().project(normalise_box(p, LO, HI, [false; 3]));
        (
            Point::new(
                origin.x + scale * projected.screen[0],
                origin.y - scale * projected.screen[1],
            ),
            projected.depth,
        )
    }
}

/// The depth over figure space of the data plane `z = height`: the plane through three projected corners of the box
/// at that height, which `depth_plane` reproduces exactly, and which every point of the data plane obeys because
/// the projection is affine.
fn figure_plane_at_height(project: &impl Fn([f64; 3]) -> (Point, f64), height: f64) -> DepthPlane {
    let corners: Vec<(Point, f64)> = [
        [-1.0, -1.0, height],
        [1.0, -1.0, height],
        [-1.0, 1.0, height],
    ]
    .iter()
    .map(|p| project(*p))
    .collect();
    depth_plane(&corners).expect("three corners of a face of the box are not collinear")
}

/// Every depth group of the scene in paint order, as its source and the number of items it holds directly.
fn depth_groups(scene: &Scene) -> Vec<(Option<NodeId>, usize)> {
    fn walk(items: &[Item], out: &mut Vec<(Option<NodeId>, usize)>) {
        for item in items {
            match &item.kind {
                ItemKind::Depth { items } => {
                    out.push((item.source, items.len()));
                    walk(items, out);
                }
                ItemKind::Group { items, .. } | ItemKind::Dense { items, .. } => walk(items, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&scene.display_list.items, &mut out);
    out
}

/// Every group that holds a depth group directly, with the index of the depth group among its items.
fn groups_holding_a_depth_group(scene: &Scene) -> Vec<(Item, usize)> {
    fn walk(items: &[Item], out: &mut Vec<(Item, usize)>) {
        for item in items {
            match &item.kind {
                ItemKind::Group { items: inner, .. } => {
                    if let Some(at) = inner
                        .iter()
                        .position(|i| matches!(i.kind, ItemKind::Depth { .. }))
                    {
                        out.push((item.clone(), at));
                    }
                    walk(inner, out);
                }
                ItemKind::Dense { items, .. } | ItemKind::Depth { items } => walk(items, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&scene.display_list.items, &mut out);
    out
}

/// The drawn points of an artist, from the hit map.
fn samples_of(scene: &Scene, id: NodeId) -> &[Sample] {
    &scene
        .hit_map
        .artists
        .iter()
        .find(|a| a.artist == id)
        .unwrap_or_else(|| panic!("hit map has no drawn points for artist {id}"))
        .samples
}

/// The position in paint order of every leaf of `id`.
fn paint_positions(leaves: &[Leaf], id: NodeId) -> Vec<usize> {
    (0..leaves.len())
        .filter(|k| leaves[*k].source == Some(id))
        .collect()
}

/// The vertices of a `Depth::Vertices`, or a failure naming the leaf.
#[track_caller]
fn vertex_depths(leaf: &Leaf, what: &str) -> Vec<f64> {
    match leaf.depth() {
        Some(Depth::Vertices(depths)) => depths.clone(),
        other => panic!("{what} carries one depth per vertex, not {other:?}"),
    }
}

/// The plane of a leaf, or a failure naming the leaf.
#[track_caller]
fn plane_of(leaf: &Leaf, what: &str) -> DepthPlane {
    leaf.plane()
        .unwrap_or_else(|| panic!("{what} carries a depth plane, not {:?}", leaf.depth()))
}

/// The four data-space corners of every face of the surface of `f` over `x × y`, in the order the faces are
/// listed by the grid: rows of `y` outermost.
fn face_corners(x: &[f64], y: &[f64], f: impl Fn(f64, f64) -> f64) -> Vec<[[f64; 3]; 4]> {
    let mut faces = Vec::new();
    for j in 0..y.len() - 1 {
        for i in 0..x.len() - 1 {
            faces.push(
                [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)]
                    .map(|(ci, cj)| [x[ci], y[cj], f(x[ci], y[cj])]),
            );
        }
    }
    faces
}

/// The leaves of `surf` whose vertices are the projections of the four `corners`, in whatever order, each with its
/// position in paint order and the projected depth of the corner at each of its vertices, in vertex order.
fn leaves_of_face(
    leaves: &[Leaf],
    surf: NodeId,
    corners: &[[f64; 3]; 4],
    project: &impl Fn([f64; 3]) -> (Point, f64),
) -> Vec<(usize, Leaf, Vec<f64>)> {
    let projected: Vec<(Point, f64)> = corners.iter().map(|c| project(*c)).collect();
    let mut out = Vec::new();
    for (k, leaf) in leaves.iter().enumerate() {
        if leaf.source != Some(surf) {
            continue;
        }
        // A fill has the four corners as its endpoints; an edge ring has them first, then its inner outline.
        let vertices: Vec<Point> = leaf
            .endpoints()
            .iter()
            .take(4)
            .map(|p| leaf.transform.apply(*p))
            .collect();
        if vertices.len() != 4 {
            continue;
        }
        let depths: Option<Vec<f64>> = vertices
            .iter()
            .map(|v| {
                projected
                    .iter()
                    .find(|(p, _)| points_close(*v, *p, 1e-6))
                    .map(|(_, d)| *d)
            })
            .collect();
        let Some(depths) = depths else { continue };
        let every_corner = projected
            .iter()
            .all(|(p, _)| vertices.iter().any(|v| points_close(*v, *p, 1e-6)));
        if every_corner {
            out.push((k, leaf.clone(), depths));
        }
    }
    out
}

/// An edit of a fixture surface's style.
type SurfaceEdit = fn(&mut Surface);

/// A 3D axes at the default view with limits [−1, 1]³ holding one surface of `f` over `grid × grid`, edited by
/// `edit`: the scene, the axes and the surface.
fn surface_scene(
    grid: &[f64],
    f: impl Fn(f64, f64) -> f64 + Copy,
    edit: impl FnOnce(&mut Surface),
) -> (Scene, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let surf = fx.surface(ax, grid, grid, f, edit);
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    (scene, ax, surf)
}

/// Asserts that a plane gives `expected[k]` at `vertices[k]` for every vertex.
#[track_caller]
fn assert_plane_reproduces(plane: DepthPlane, vertices: &[Point], expected: &[f64], what: &str) {
    assert_eq!(
        vertices.len(),
        expected.len(),
        "{what}: one expected depth per vertex"
    );
    for (k, (v, d)) in vertices.iter().zip(expected).enumerate() {
        assert!(
            (plane.at(*v) - d).abs() <= 1e-9,
            "{what}: the plane gives {} at vertex {k} {v:?}, expected {d}",
            plane.at(*v)
        );
    }
}

/// The pixel edges along one axis of an image of `n` pixels whose first and last centres are given.
fn edges(first: f64, last: f64, n: usize) -> (f64, f64) {
    let pitch = (last - first) / (n - 1) as f64;
    (first - pitch / 2.0, last + pitch / 2.0)
}

/// An axes holding every kind of artist a 3D axes draws: a line with a gap and markers, a scatter, a surface, a
/// filled contour and a contour at level, a quiver, an image inside the box and one on its floor. Returns the
/// scene, the axes and the artists.
fn axes_with_every_artist(three_d: bool) -> (Scene, NodeId, Vec<NodeId>) {
    let mut fx = Fx::new();
    let ax = if three_d {
        fx.axes3d(0, 0, View3d::default())
    } else {
        fx.axes2d(0, 0)
    };
    let z = |values: &'static [f64]| three_d.then_some(values);
    let g = linspace(-1.0, 1.0, 5);
    let line = fx.line(
        ax,
        &[-0.8, -0.2, 0.3, 0.7, 0.9],
        &[0.5, -0.4, f64::NAN, 0.6, -0.7],
        z(&[-0.4, 0.2, 0.0, 0.6, 0.1]),
        |l| l.marker.shape = MarkerShape::Circle,
    );
    let scatter = fx.scatter(
        ax,
        &[-0.5, 0.0, 0.5],
        &[0.4, -0.6, 0.2],
        z(&[0.1, -0.3, 0.7]),
        |_, _| {},
    );
    let surface = fx.surface(ax, &g, &g, |x, y| 0.3 * x * y, |_| {});
    let bands = fx.contour(
        ax,
        &g,
        &g,
        |x, y| x + y,
        |c| {
            c.fill = true;
            c.levels = Levels::Explicit {
                values: vec![-0.5, 0.5],
            };
            c.placement = ContourPlacement::Plane { z: Some(0.25) };
        },
    );
    let isolines = fx.contour(
        ax,
        &g,
        &g,
        |x, y| x - y,
        |c| {
            c.levels = Levels::Explicit {
                values: vec![-0.5, 0.5],
            };
            c.placement = if three_d {
                ContourPlacement::AtLevel
            } else {
                ContourPlacement::Plane { z: None }
            };
        },
    );
    let quiver = fx.quiver(
        ax,
        &[-0.5, 0.3],
        &[0.2, -0.6],
        z(&[-0.4, 0.5]),
        &[0.2, -0.15],
        &[0.05, 0.1],
        z(&[0.15, -0.1]),
    );
    let interior = fx.mapped_image(
        ax,
        vec![2, 4],
        (0..8).map(|v| v as f64).collect::<Vec<_>>(),
        placement(
            ImagePlane::Xy { z: Some(0.5) },
            range(-0.6, 0.6),
            range(-0.5, 0.0),
        ),
        |_| {},
    );
    let floor = fx.mapped_image(
        ax,
        vec![2, 4],
        (0..8).map(|v| v as f64).collect::<Vec<_>>(),
        placement(
            ImagePlane::Xy { z: None },
            range(-0.75, 0.75),
            range(-0.5, 0.5),
        ),
        |_| {},
    );
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    (
        scene,
        ax,
        vec![
            line, scatter, surface, bands, isolines, quiver, interior, floor,
        ],
    )
}

// WHY: a backend with a depth buffer clears the buffer at a depth group and tests every item inside it against the
// others, so the artists of one axes must share exactly one group: two groups would let a later artist paint over a
// nearer earlier one, and none would leave the painter's order as the only ordering the backend has. The box, the
// grid and the labels of the axes carry no depth and must stay outside the group, where the backend draws them as
// it draws any two-dimensional leaf.
#[test]
fn a_three_dimensional_axes_holds_the_leaves_of_its_artists_in_one_depth_group() {
    let (scene, ax, artists) = axes_with_every_artist(true);
    let groups = depth_groups(&scene);
    assert_eq!(groups.len(), 1, "one depth group for the axes: {groups:?}");
    assert_eq!(
        groups[0].0,
        Some(ax),
        "the depth group is attributed to the axes"
    );

    let grouped = grouped_leaves(&scene);
    for id in &artists {
        let of_artist: Vec<&(Leaf, Option<usize>)> = grouped
            .iter()
            .filter(|(l, _)| l.source == Some(*id))
            .collect();
        assert!(!of_artist.is_empty(), "artist {id} is drawn");
        assert!(
            of_artist.iter().all(|(_, group)| *group == Some(0)),
            "every leaf of artist {id} lies in the first depth group: {:?}",
            of_artist.iter().map(|(_, g)| g).collect::<Vec<_>>()
        );
    }
    let of_axes: Vec<&(Leaf, Option<usize>)> = grouped
        .iter()
        .filter(|(l, _)| l.source == Some(ax))
        .collect();
    assert!(!of_axes.is_empty(), "the axes draws its box and labels");
    assert!(
        of_axes.iter().all(|(_, group)| group.is_none()),
        "no leaf of the axes itself lies in a depth group"
    );
    assert!(
        grouped
            .iter()
            .filter(|(l, _)| l.glyphs().is_some())
            .all(|(_, group)| group.is_none()),
        "no glyph run lies in a depth group"
    );
}

// WHY: the groups are numbered in paint order across the whole figure, so a backend can bundle the leaves of each
// group without knowing how axes nest; a compiler that reused one group for two axes, or an axes whose group was
// numbered from its own artists, would have the second axes depth-tested against the first.
#[test]
fn two_three_dimensional_axes_yield_two_depth_groups_numbered_in_paint_order() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let first = fx.axes3d(0, 0, View3d::default());
    let second = fx.axes3d(0, 1, View3d::default());
    let g = linspace(-1.0, 1.0, 3);
    let on_first = fx.surface(first, &g, &g, |x, y| x * y, |_| {});
    let on_second = fx.surface(second, &g, &g, |x, y| x + y, |_| {});
    let scene = compile_figure(&fx.build());

    let groups = depth_groups(&scene);
    assert_eq!(
        groups.iter().map(|g| g.0).collect::<Vec<_>>(),
        vec![Some(first), Some(second)],
        "one depth group per axes, in paint order"
    );
    let grouped = grouped_leaves(&scene);
    for (artist, expected) in [(on_first, Some(0)), (on_second, Some(1))] {
        let of_artist: Vec<Option<usize>> = grouped
            .iter()
            .filter(|(l, _)| l.source == Some(artist))
            .map(|(_, g)| *g)
            .collect();
        assert!(!of_artist.is_empty(), "artist {artist} has leaves");
        assert!(
            of_artist.iter().all(|g| *g == expected),
            "the leaves of artist {artist} report group {expected:?}: {of_artist:?}"
        );
    }
}

// WHY: the back edges of the box and the grid lie behind everything in the box and the front edges in front of it,
// so the depth group must sit between them in paint order for a backend without a depth buffer to draw the box
// around the data as the depth-buffer backend does; a group placed after the front edges would paint the data over
// the near edges of the box, and one placed before the back edges would draw the grid over the data.
#[test]
fn the_depth_group_sits_between_the_back_edges_and_the_front_edges_of_the_box() {
    let holding = |box_: bool| {
        let mut fx = Fx::new();
        let ax = fx.axes3d(0, 0, View3d::default());
        let g = linspace(-1.0, 1.0, 3);
        fx.surface(ax, &g, &g, |x, y| 0.5 * (x + y), |_| {});
        fx.ax(ax).box_ = box_;
        manual_unit_limits(&mut fx, ax);
        let scene = compile_figure(&fx.build());
        let holding = groups_holding_a_depth_group(&scene);
        assert_eq!(holding.len(), 1, "one group holds the depth group");
        let (group, at) = holding.into_iter().next().unwrap();
        assert_eq!(
            group.source,
            Some(ax),
            "the holding group is the axes' own group"
        );
        let ItemKind::Group { clip, items, .. } = group.kind else {
            unreachable!()
        };
        assert!(
            clip.is_some(),
            "the holding group is the clipped group of the axes"
        );
        let is_axes_path =
            move |item: &Item| item.source == Some(ax) && matches!(item.kind, ItemKind::Path(_));
        (items, at, is_axes_path)
    };

    let (items, at, is_axes_path) = holding(true);
    assert!(at >= 1, "the back edges of the box precede the depth group");
    assert!(
        items[..at].iter().all(is_axes_path),
        "everything before the depth group is a path of the axes: the grid and the back edges"
    );
    assert_eq!(
        items[at + 1..].iter().filter(|i| is_axes_path(i)).count(),
        1,
        "the front edges of the box follow the depth group"
    );
    assert_eq!(
        items.len(),
        at + 2,
        "nothing but the front edges follows the depth group"
    );

    let (items, at, _) = holding(false);
    assert_eq!(
        items.len(),
        at + 1,
        "without the box, the depth group is the last item of the axes' group"
    );
}

// WHY: a depth buffer is meaningless in two dimensions, where later artists simply cover earlier ones, so a 2D axes
// must emit no depth group and none of its paths or images may carry a depth; a backend that met a depth in 2D
// would test it against nothing, and one that met a depth group would clear its buffer for no reason.
#[test]
fn a_two_dimensional_axes_has_no_depth_group_and_its_leaves_carry_no_depth() {
    let (scene, ax, artists) = axes_with_every_artist(false);
    assert!(
        depth_groups(&scene).is_empty(),
        "a 2D axes emits no depth group: {:?}",
        depth_groups(&scene)
    );
    let grouped = grouped_leaves(&scene);
    assert!(
        grouped.iter().all(|(_, group)| group.is_none()),
        "no leaf of a 2D figure lies in a depth group"
    );
    for id in artists.iter().chain([&ax]) {
        for (leaf, _) in grouped.iter().filter(|(l, _)| l.source == Some(*id)) {
            if let Some(path) = leaf.path() {
                assert!(path.depth.is_none(), "a path of node {id} carries no depth");
            }
            if let Some(image) = leaf.image() {
                assert!(
                    image.depth.is_none(),
                    "an image of node {id} carries no depth"
                );
            }
        }
    }
}

// WHY: a depth group is never empty, so a backend can clear its depth buffer at the group without asking whether
// there is anything to test; an axes whose artists draw nothing, whether because it has none or because none of
// their points can be placed, must therefore emit no group at all while still drawing its box.
#[test]
fn a_three_dimensional_axes_whose_artists_draw_nothing_emits_no_depth_group() {
    let empty = {
        let mut fx = Fx::new();
        let ax = fx.axes3d(0, 0, View3d::default());
        manual_unit_limits(&mut fx, ax);
        (compile_figure(&fx.build()), ax)
    };
    let nothing_placeable = {
        let mut fx = Fx::new();
        let ax = fx.axes3d(0, 0, View3d::default());
        let nan = [f64::NAN; 3];
        fx.line(ax, &nan, &nan, Some(&nan), |_| {});
        manual_unit_limits(&mut fx, ax);
        (compile_figure(&fx.build()), ax)
    };
    for ((scene, ax), what) in [
        (empty, "an empty axes"),
        (nothing_placeable, "an axes whose line has no finite point"),
    ] {
        assert!(
            depth_groups(&scene).is_empty(),
            "{what} emits no depth group: {:?}",
            depth_groups(&scene)
        );
        assert!(
            !from_source(&leaves(&scene), ax).is_empty(),
            "{what} still draws its box"
        );
    }
}

// WHY: a backend with a depth buffer reads the depth of every leaf of the group without checking it, so every path
// inside must carry a depth a backend can use (a finite plane, or one finite depth per endpoint), every image a
// finite plane and every marker instance a finite depth of its own, whatever kind of artist drew it; a line, a band
// or an arrow whose depth count did not match its vertices would be drawn at garbage depths, and a glyph run inside
// the group would have no depth at all.
#[test]
fn every_leaf_inside_a_depth_group_carries_a_usable_depth() {
    let (scene, _, _) = axes_with_every_artist(true);
    let grouped = grouped_leaves(&scene);
    let inside: Vec<&(Leaf, Option<usize>)> = grouped
        .iter()
        .filter(|(_, group)| group.is_some())
        .collect();
    for (leaf, _) in inside {
        match &leaf.kind {
            ItemKind::Path(path) => {
                assert!(
                    path.depth.is_some(),
                    "a path of {:?} inside the depth group carries a depth",
                    leaf.source
                );
                assert!(
                    path.is_valid_depth(),
                    "the depth of a path of {:?} is usable: {:?} for {} endpoints",
                    leaf.source,
                    path.depth,
                    path.endpoint_count()
                );
            }
            ItemKind::Image(image) => assert!(
                image.depth.is_some_and(|plane| plane.is_finite()),
                "an image of {:?} inside the depth group carries a finite plane: {:?}",
                leaf.source,
                image.depth
            ),
            ItemKind::Markers(markers) => {
                assert!(
                    !markers.instances.is_empty(),
                    "a markers item of {:?} inside the depth group holds a marker",
                    leaf.source
                );
                for instance in &markers.instances {
                    assert!(
                        instance.depth.is_finite(),
                        "marker {} of {:?} inside the depth group carries a finite depth: {}",
                        instance.source_index,
                        leaf.source,
                        instance.depth
                    );
                }
            }
            other => {
                panic!("only paths, images and markers lie inside a depth group, not {other:?}")
            }
        }
    }
    for (leaf, _) in grouped.iter().filter(|(_, group)| group.is_none()) {
        assert!(
            leaf.path().is_none_or(|p| p.depth.is_none())
                && leaf.image().is_none_or(|i| i.depth.is_none()),
            "a leaf of {:?} outside every depth group carries no depth",
            leaf.source
        );
    }
}

// WHY: a line is depth-tested along its length, so each run must carry one depth per point, interpolated between
// them, and those depths must be the ones the hit map reports for the same points, otherwise picking would name a
// point at one depth while the buffer drew it at another. A gap in the data makes two runs, each with the depths of
// its own points in point order, and a run that borrowed the depths of the other would be drawn slanted.
#[test]
fn a_line_run_carries_one_depth_per_point_equal_to_the_depths_of_its_samples() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let (x, y, z) = (
        [-0.8, -0.2, 0.3, 0.7, 0.9],
        [0.5, -0.4, f64::NAN, 0.6, -0.7],
        [-0.4, 0.2, 0.0, 0.6, 0.1],
    );
    let line = fx.line(ax, &x, &y, Some(&z), |_| {});
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());

    let samples = samples_of(&scene, line);
    assert_eq!(
        samples.iter().map(|s| s.source_index).collect::<Vec<_>>(),
        vec![0, 1, 3, 4],
        "the four placeable points are drawn"
    );
    for s in samples {
        assert_close(
            s.depth,
            depth_at([x[s.source_index], y[s.source_index], z[s.source_index]]),
            1e-9,
        );
    }

    let runs = from_source(&leaves(&scene), line);
    assert_eq!(
        runs.len(),
        2,
        "the gap splits the line into two runs: {runs:?}"
    );
    let mut matched = Vec::new();
    for run in &runs {
        let depths = vertex_depths(run, "a line run");
        let vertices = run.endpoints();
        assert_eq!(
            depths.len(),
            vertices.len(),
            "one depth per point of the run"
        );
        let mut indices = Vec::new();
        for (v, d) in vertices.iter().zip(&depths) {
            let at = run.transform.apply(*v);
            let sample = samples
                .iter()
                .find(|s| points_close(s.position, at, 1e-6))
                .unwrap_or_else(|| panic!("the vertex at {at:?} is a drawn point of the line"));
            assert_eq!(
                *d, sample.depth,
                "the depth of point {} is its sample's",
                sample.source_index
            );
            indices.push(sample.source_index);
        }
        assert!(
            indices.windows(2).all(|w| w[0] < w[1]),
            "the depths follow the points in order: {indices:?}"
        );
        matched.extend(indices);
    }
    matched.sort();
    assert_eq!(
        matched,
        vec![0, 1, 3, 4],
        "between them the runs carry every drawn point once"
    );
}

// WHY: a marker is a small flat symbol at one point, so the whole of it lies at the depth of that point, whatever
// the shape; a marker given the plane of the line it decorates, or a depth per vertex of its outline, would sink
// half into whatever surface it sits on. Each instance carries that depth itself, since the run it lies in holds
// markers at many depths, and it is the depth the hit map records for the point, so that picking and drawing
// agree.
#[test]
fn markers_of_a_line_and_of_a_scatter_each_carry_the_depth_of_their_own_point() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let line = fx.line(
        ax,
        &[-0.8, 0.0, 0.7],
        &[0.5, -0.4, 0.9],
        Some(&[-0.3, 0.2, 0.6]),
        |l| l.marker.shape = MarkerShape::Circle,
    );
    let scatter = fx.scatter(
        ax,
        &[-0.5, 0.0, 0.5, 0.8],
        &[-0.5, 0.0, 0.5, 0.8],
        Some(&[-0.5, 0.0, 0.5, 0.8]),
        |_, _| {},
    );
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);

    for (id, count, what) in [(line, 3, "line"), (scatter, 4, "scatter")] {
        let samples = samples_of(&scene, id);
        assert_eq!(samples.len(), count, "the {what} draws every point");
        let drawn = marker_instances(&leaves, id);
        for s in samples {
            let markers: Vec<&MarkerInstance> = drawn
                .iter()
                .filter(|m| points_close(m.position, s.position, 1e-6))
                .collect();
            assert_eq!(
                markers.len(),
                1,
                "{what}: one marker is centred on point {}",
                s.source_index
            );
            assert_eq!(
                markers[0].depth, s.depth,
                "{what}: the marker of point {} lies at the depth of its sample",
                s.source_index
            );
            assert_eq!(
                markers[0].source_index, s.source_index,
                "{what}: the marker of point {} names its point",
                s.source_index
            );
        }
    }
}

// WHY: a face's edge must never spill over its neighbours, or the painter's order and the depth test would disagree
// along every fold of a surface and no surface with edges could be exported as vectors; so the edge is a ring filled
// with the edge colour between the face's outline and that outline moved half the edge width inwards, drawn after
// the fill on the fill's plane lifted by a hair (a depth buffer interpolates one plane differently over two
// triangulations, and the ring must not lose that tie), and the rings of two neighbours meet to make an edge of the
// full width. The whole
// face is pushed back by the bias so that lines and markers lying on the surface are painted, and depth-tested, in
// front of it; a face left as one leaf could not carry two colours.
#[test]
fn a_face_with_fill_and_edge_is_a_fill_leaf_then_an_edge_ring_on_its_plane_pushed_back_by_the_bias()
{
    let grid = linspace(-0.6, 0.6, 3);
    let height = |x: f64, y: f64| 0.5 * (x + y);
    let (scene, ax, surf) = surface_scene(&grid, height, |s| s.edge_width_pt = 0.5);
    let leaves = leaves(&scene);
    let project = projection(&scene, ax);
    assert_eq!(
        from_source(&leaves, surf).len(),
        8,
        "two leaves for each of the four faces"
    );

    for corners in face_corners(&grid, &grid, height) {
        let found = leaves_of_face(&leaves, surf, &corners, &project);
        assert_eq!(
            found.len(),
            2,
            "two leaves for the face at {corners:?}: {found:?}"
        );
        let (fill_at, fill, depths) = &found[0];
        let (ring_at, ring, _) = &found[1];
        assert!(
            ring_at > fill_at,
            "the fill at {fill_at} is painted before the edge at {ring_at}"
        );
        let (fill_path, ring_path) = (fill.path().unwrap(), ring.path().unwrap());
        assert!(
            fill_path.fill.is_some() && fill_path.stroke.is_none(),
            "the first leaf is the fill alone"
        );
        assert!(
            ring_path.fill.is_some() && ring_path.stroke.is_none(),
            "the edge is a filled ring, not a stroke"
        );
        assert_ne!(
            fill_path.fill.unwrap().color,
            ring_path.fill.unwrap().color,
            "the ring has the edge colour and the fill the face colour"
        );

        let outer: Vec<Point> = corners.iter().map(|c| project(*c).0).collect();
        let subpaths = ring.subpaths();
        assert_eq!(
            subpaths.len(),
            2,
            "the ring is the face's outline and an inner outline"
        );
        for (k, p) in subpaths[0].iter().enumerate() {
            assert!(
                points_close(*p, outer[k], 1e-9),
                "outer vertex {k} of the ring is the face's corner: {p:?} against {:?}",
                outer[k]
            );
        }
        let centre = Point::new(
            outer.iter().map(|p| p.x).sum::<f64>() / 4.0,
            outer.iter().map(|p| p.y).sum::<f64>() / 4.0,
        );
        assert_eq!(
            subpaths[1].len(),
            4,
            "the inner outline has one vertex per corner"
        );
        for q in &subpaths[1] {
            let insides: Vec<f64> = (0..4)
                .map(|i| distance_inside(*q, outer[i], outer[(i + 1) % 4], centre))
                .collect();
            let on_two_edges = insides.iter().filter(|d| (*d - 0.25).abs() <= 1e-9).count();
            assert!(
                on_two_edges == 2 && insides.iter().all(|d| *d >= 0.25 - 1e-9),
                "inner vertex {q:?} lies half the edge width inside two edges and at least that inside the rest: \
                 {insides:?}"
            );
        }

        let (fill_plane, ring_plane) = (plane_of(fill, "the fill"), plane_of(ring, "the ring"));
        assert_eq!(
            ring_plane,
            fill_plane.pushed_back(-EDGE_DEPTH_LIFT),
            "the ring lies on the fill's plane lifted by the hair that keeps it in front"
        );
        let pushed: Vec<f64> = depths.iter().map(|d| d - FACE_DEPTH_BIAS).collect();
        assert_plane_reproduces(fill_plane, &fill.endpoints(), &pushed, "the face's plane");
    }
}

/// The distance of `q` inside the edge from `a` to `b` of a polygon whose interior holds `inside`.
fn distance_inside(q: Point, a: Point, b: Point, inside: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx.hypot(dy);
    let (nx, ny) = (-dy / length, dx / length);
    let sign = ((inside.x - a.x) * nx + (inside.y - a.y) * ny).signum();
    sign * ((q.x - a.x) * nx + (q.y - a.y) * ny)
}

// WHY: the four corners of a face of a curved surface are not coplanar, and a backend can only test one plane per
// path, so the compiler must give the face the least-squares plane through its projected corners: the best single
// plane for a twisted face. A plane through three of the corners would leave the fourth hanging a visible distance
// off it, and a constant depth would let neighbouring faces cut through one another.
#[test]
fn a_twisted_face_takes_the_least_squares_plane_through_its_projected_corners() {
    let grid = linspace(-1.0, 1.0, 3);
    let height = |x: f64, y: f64| x * y;
    let (scene, ax, surf) = surface_scene(&grid, height, |_| {});
    let leaves = leaves(&scene);
    let project = projection(&scene, ax);

    for corners in face_corners(&grid, &grid, height) {
        let found = leaves_of_face(&leaves, surf, &corners, &project);
        assert_eq!(found.len(), 2, "two leaves for the face at {corners:?}");
        let (_, fill, depths) = &found[0];
        let vertices = fill.endpoints();
        let points: Vec<(Point, f64)> = vertices
            .iter()
            .copied()
            .zip(depths.iter().copied())
            .collect();
        let expected = depth_plane(&points).expect("the corners are finite");
        let residual = points
            .iter()
            .map(|(p, d)| (expected.at(*p) - d).abs())
            .fold(0.0, f64::max);
        assert!(
            residual > 1e-3,
            "the face is twisted, so no plane passes through all four corners: {residual}"
        );

        let fill_plane = plane_of(fill, "the fill");
        let expected = expected.pushed_back(FACE_DEPTH_BIAS);
        for (name, actual, wanted) in [
            ("a", fill_plane.a, expected.a),
            ("b", fill_plane.b, expected.b),
            ("c", fill_plane.c, expected.c),
        ] {
            assert!(
                (actual - wanted).abs() <= 1e-9,
                "coefficient {name} of the face's plane is {actual}, expected the least-squares plane pushed back \
                 by the bias, {wanted}"
            );
        }
        assert_eq!(
            plane_of(&found[1].1, "the ring"),
            fill_plane.pushed_back(-EDGE_DEPTH_LIFT),
            "the edge ring lies on the face's plane, lifted by the hair"
        );
    }
}

// WHY: a face with only a fill or only an edge is one leaf, and it must be pushed back by the bias like a face with
// both, otherwise `mesh` (edges alone) would tie with the markers and lines it should lie behind, and a surface
// without edges would hide them.
#[test]
fn a_face_with_only_a_fill_or_only_an_edge_is_one_leaf_pushed_back_by_the_bias() {
    let grid = linspace(-0.6, 0.6, 3);
    let height = |x: f64, y: f64| 0.5 * (x + y);
    let cases: [(SurfaceEdit, &str); 2] = [
        (|s| s.face = ColorSpec::None, "edge only"),
        (|s| s.edge = ColorSpec::None, "fill only"),
    ];
    for (edit, what) in cases {
        let (scene, ax, surf) = surface_scene(&grid, height, edit);
        let leaves = leaves(&scene);
        let project = projection(&scene, ax);
        assert_eq!(
            from_source(&leaves, surf).len(),
            4,
            "{what}: one leaf per face"
        );
        for corners in face_corners(&grid, &grid, height) {
            let found = leaves_of_face(&leaves, surf, &corners, &project);
            assert_eq!(
                found.len(),
                1,
                "{what}: one leaf for the face at {corners:?}"
            );
            let (_, leaf, depths) = &found[0];
            let path = leaf.path().unwrap();
            assert!(
                path.fill.is_some() && path.stroke.is_none(),
                "{what}: the leaf is a fill (the face, or the edge ring)"
            );
            assert_eq!(
                leaf.subpaths().len(),
                if what == "edge only" { 2 } else { 1 },
                "{what}: an edge ring has an inner outline and a fill has none"
            );
            let plane = plane_of(leaf, what);
            let lift = if what == "edge only" {
                EDGE_DEPTH_LIFT
            } else {
                0.0
            };
            let pushed: Vec<f64> = depths.iter().map(|d| d - FACE_DEPTH_BIAS + lift).collect();
            assert_plane_reproduces(plane, &leaf.endpoints()[..4], &pushed, what);
        }
    }
}

/// The four data-space corners of an image in the xy plane at `z` whose pixel centres span `columns` and `rows`,
/// paired with the pixel-space corner each one is drawn at.
fn image_corners(
    z: f64,
    columns: (f64, f64),
    rows: (f64, f64),
    nx: usize,
    ny: usize,
) -> Vec<(Point, [f64; 3])> {
    let (x0, x1) = edges(columns.0, columns.1, nx);
    let (y0, y1) = edges(rows.0, rows.1, ny);
    let (nx, ny) = (nx as f64, ny as f64);
    vec![
        (Point::new(0.0, 0.0), [x0, y0, z]),
        (Point::new(nx, 0.0), [x1, y0, z]),
        (Point::new(0.0, ny), [x0, y1, z]),
        (Point::new(nx, ny), [x1, y1, z]),
    ]
}

/// Compiles a 3D axes holding one image of 2 by 4 pixels in the xy plane at `z` with asymmetric pixel ranges, and
/// returns the depth plane of its one image leaf with the data corner at each pixel corner.
fn image_plane(plane: ImagePlane, z: f64) -> (DepthPlane, Vec<(Point, [f64; 3])>) {
    let (columns, rows) = ((-0.6, 0.6), (-0.5, 0.0));
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let image = fx.mapped_image(
        ax,
        vec![2, 4],
        (0..8).map(|v| v as f64).collect::<Vec<_>>(),
        placement(plane, range(columns.0, columns.1), range(rows.0, rows.1)),
        |_| {},
    );
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let drawn = from_source(&leaves(&scene), image);
    assert_eq!(drawn.len(), 1, "the image is one leaf");
    let item = drawn[0]
        .image()
        .expect("the leaf of an image is an image item");
    let plane = item
        .depth
        .unwrap_or_else(|| panic!("the image carries a depth plane: {item:?}"));
    (plane, image_corners(z, columns, rows, 4, 2))
}

// WHY: an image is depth-tested pixel by pixel from a plane over its own pixel space, so the plane must reproduce
// the projected depth of the data corner at each pixel corner, with the columns along the first axis of its plane
// and the rows along the second; a plane over figure space, or one with the axes exchanged, would tilt the image
// away from the surface it is meant to lie on. An image inside the box is pushed back by the bias like a face's
// fill, so that a line or marker drawn on it shows rather than flickers.
#[test]
fn an_image_inside_the_box_carries_its_plane_pushed_back_by_the_bias() {
    let (plane, corners) = image_plane(ImagePlane::Xy { z: Some(0.5) }, 0.5);
    for (pixel, data) in corners {
        assert_close(plane.at(pixel), depth_at(data) - FACE_DEPTH_BIAS, 1e-9);
    }
}

// WHY: an image on a face of the box is already sorted behind or in front of everything in the box, and there is
// nothing on the far side of the face for it to tie with, so its plane is the true one: pushing the floor back
// would open a gap between it and a surface resting on it, through which the background would show.
#[test]
fn an_image_on_a_face_of_the_box_carries_its_plane_unpushed() {
    for (plane, z, what) in [
        (
            ImagePlane::Xy { z: None },
            -1.0,
            "the floor, without an offset",
        ),
        (
            ImagePlane::Xy { z: Some(1.0) },
            1.0,
            "the ceiling, at the upper limit",
        ),
    ] {
        let (depth, corners) = image_plane(plane, z);
        for (pixel, data) in corners {
            assert!(
                (depth.at(pixel) - depth_at(data)).abs() <= 1e-9,
                "{what}: the plane gives {} at pixel corner {pixel:?}, expected {}",
                depth.at(pixel),
                depth_at(data)
            );
        }
    }
}

// WHY: a marker at the height of a flat face ties with the face in depth, so a backend without a depth buffer
// paints whichever the sort put last, and the stable sort puts the artist that came first underneath; the bias on
// the fill exists so that the marker wins whatever the artist order. Three scatters declared before the surface
// pin the relation rather than a lucky tie: a marker on the face and one a quarter of the bias behind it are both
// painted after the fill, and one two biases behind it before the fill. Without the bias the first would be
// decided by rounding and the second would vanish under the face.
#[test]
fn a_marker_on_or_just_behind_a_face_is_painted_after_its_fill() {
    // At the default view the depth changes by sin 30° per unit of the normalised box, and the z range of two
    // data units spans one unit of the box, so 1e-3 in z is a quarter of the bias and 8e-3 twice it.
    let grid = linspace(-1.0, 1.0, 3);
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let on_face = fx.scatter(ax, &[0.5], &[0.5], Some(&[0.5]), |_, _| {});
    let just_behind = fx.scatter(ax, &[0.5], &[0.5], Some(&[0.5 - 1e-3]), |_, _| {});
    let well_behind = fx.scatter(ax, &[0.5], &[0.5], Some(&[0.5 - 8e-3]), |_, _| {});
    let surf = fx.surface(ax, &grid, &grid, |_, _| 0.5, |_| {});
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let project = projection(&scene, ax);

    let corners = [
        [0.0, 0.0, 0.5],
        [1.0, 0.0, 0.5],
        [1.0, 1.0, 0.5],
        [0.0, 1.0, 0.5],
    ];
    let found = leaves_of_face(&leaves, surf, &corners, &project);
    assert_eq!(
        found.len(),
        2,
        "the face under the markers is a fill leaf and an edge leaf"
    );
    let (fill_at, fill, depths) = &found[0];
    assert!(
        fill.path().unwrap().fill.is_some(),
        "the first leaf of the face is its fill"
    );
    let face_depth = depths.iter().sum::<f64>() / 4.0;
    let marker_depth = depth_at([0.5, 0.5, 0.5]);
    assert!(
        (marker_depth - face_depth).abs() <= 1e-12,
        "the marker on the face shares its depth: {marker_depth} against {face_depth}"
    );
    let quarter_bias = marker_depth - depth_at([0.5, 0.5, 0.5 - 1e-3]);
    assert!(
        (quarter_bias - FACE_DEPTH_BIAS / 4.0).abs() <= 1e-12,
        "1e-3 in z is a quarter of the bias, not {quarter_bias}"
    );

    let position = |artist: NodeId| {
        let at = paint_positions(&leaves, artist);
        assert_eq!(at.len(), 1, "scatter {artist} is one markers item");
        assert_eq!(
            leaves[at[0]].instances().len(),
            1,
            "scatter {artist} holds one marker"
        );
        at[0]
    };
    assert!(
        position(on_face) > *fill_at,
        "the marker on the face is painted after the fill at {fill_at}"
    );
    assert!(
        position(just_behind) > *fill_at,
        "the marker a quarter of the bias behind the face is painted after the fill at {fill_at}"
    );
    assert!(
        position(well_behind) < *fill_at,
        "the marker two biases behind the face is painted before the fill at {fill_at}"
    );
}

// WHY: a filled contour band lies in one horizontal plane, and a depth buffer tests its whole polygon from one
// plane, so that plane must give every vertex its projected depth; a band keyed at a constant depth would cut
// diagonally through a surface it is drawn beneath, and one fitted in the wrong space would tilt.
#[test]
fn a_filled_contour_band_carries_the_plane_of_the_height_it_lies_at() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let g = linspace(-1.0, 1.0, 5);
    let contour = fx.contour(
        ax,
        &g,
        &g,
        |x, y| x + y,
        |c| {
            c.fill = true;
            c.levels = Levels::Explicit {
                values: vec![-0.5, 0.5],
            };
            c.placement = ContourPlacement::Plane { z: Some(0.25) };
        },
    );
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    let project = projection(&scene, ax);
    let expected = figure_plane_at_height(&project, 0.25);

    let bands: Vec<Leaf> = from_source(&leaves(&scene), contour)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.fill.is_some()))
        .collect();
    assert!(
        bands.len() >= 2,
        "the levels cut the field into several bands: {}",
        bands.len()
    );
    for band in &bands {
        let plane = plane_of(band, "a band");
        let vertices = band.endpoints();
        assert!(vertices.len() >= 3, "a band is a polygon");
        let depths: Vec<f64> = vertices.iter().map(|v| expected.at(*v)).collect();
        assert_plane_reproduces(plane, &vertices, &depths, "the band's plane");
    }
}

// WHY: `contour3` lifts each isoline to the height of its level, so each run must carry one depth per point at
// that height, and the runs of different levels must lie at different depths; a contour that reused one plane for
// every level would flatten the lifted lines back onto one another in the depth buffer.
#[test]
fn a_contour3_isoline_run_carries_one_depth_per_point_at_the_height_of_its_level() {
    let levels = [-0.5, 0.5];
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let g = linspace(-1.0, 1.0, 5);
    let contour = fx.contour(
        ax,
        &g,
        &g,
        |x, y| x + y,
        |c| {
            c.levels = Levels::Explicit {
                values: levels.to_vec(),
            };
            c.placement = ContourPlacement::AtLevel;
        },
    );
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    let project = projection(&scene, ax);
    let planes: Vec<DepthPlane> = levels
        .iter()
        .map(|z| figure_plane_at_height(&project, *z))
        .collect();

    let runs = from_source(&leaves(&scene), contour);
    assert!(
        runs.len() >= 2,
        "each level draws at least one run: {}",
        runs.len()
    );
    let mut levels_seen = Vec::new();
    for run in &runs {
        let depths = vertex_depths(run, "an isoline run");
        let vertices = run.endpoints();
        assert_eq!(
            depths.len(),
            vertices.len(),
            "one depth per point of the run"
        );
        assert!(vertices.len() >= 2, "a run has at least two points");
        let at_level = (0..levels.len()).find(|k| {
            vertices
                .iter()
                .zip(&depths)
                .all(|(v, d)| (planes[*k].at(*v) - d).abs() <= 1e-9)
        });
        let level = at_level.unwrap_or_else(|| {
            panic!("the run's depths {depths:?} lie in the plane of one of the levels {levels:?}")
        });
        levels_seen.push(level);
    }
    levels_seen.sort();
    levels_seen.dedup();
    assert_eq!(
        levels_seen,
        vec![0, 1],
        "both levels are drawn at their own heights"
    );
}

// WHY: an arrow is one path of two subpaths, the shaft and the open head, so a backend needs five depths in the
// order of its five endpoints; an arrow keyed at one depth would push its head into a surface it points at, and
// depths in another order would slant the head. The head is built in the normalised box, so its depths are those
// of the projected head points, not of the tip.
#[test]
fn a_quiver3_arrow_carries_five_depths_for_its_shaft_and_head() {
    let (x, y, z) = ([-0.5, 0.3], [0.2, -0.6], [-0.4, 0.5]);
    let (u, v, w) = ([0.4, -0.3], [0.1, 0.2], [0.3, -0.2]);
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let quiver = fx.quiver(ax, &x, &y, Some(&z), &u, &v, Some(&w));
    for artist in &mut fx.ax(ax).artists {
        if let Artist::Quiver(q) = artist
            && q.id == quiver
        {
            q.scale = QuiverScale::Off;
        }
    }
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    let project = projection(&scene, ax);
    let camera = Camera::default();

    let arrows = from_source(&leaves(&scene), quiver);
    assert_eq!(arrows.len(), 2, "one path per arrow");
    for i in 0..2 {
        let base = [x[i], y[i], z[i]];
        let tip = [x[i] + u[i], y[i] + v[i], z[i] + w[i]];
        let (nb, nt) = (
            normalise_box(base, LO, HI, [false; 3]),
            normalise_box(tip, LO, HI, [false; 3]),
        );
        let geometry = arrow(nb, [0, 1, 2].map(|k| nt[k] - nb[k]), 1.0, 0.3);
        let expected: Vec<f64> = [
            geometry.shaft[0],
            geometry.shaft[1],
            geometry.head[0],
            geometry.head[1],
            geometry.head[2],
        ]
        .iter()
        .map(|p| camera.project(*p).depth)
        .collect();

        let leaf = arrows
            .iter()
            .find(|l| {
                l.endpoints()
                    .first()
                    .is_some_and(|p| points_close(l.transform.apply(*p), project(base).0, 1e-6))
            })
            .unwrap_or_else(|| panic!("an arrow starts at the base {base:?}"));
        assert_eq!(
            leaf.path().unwrap().endpoint_count(),
            5,
            "shaft and head make five endpoints"
        );
        let depths = vertex_depths(leaf, "an arrow");
        assert_eq!(depths.len(), 5, "five depths for the five endpoints");
        for (k, (actual, wanted)) in depths.iter().zip(&expected).enumerate() {
            assert!(
                (actual - wanted).abs() <= 1e-9,
                "arrow {i}: depth {k} is {actual}, expected {wanted} (all {depths:?} against {expected:?})"
            );
        }
    }
}

/// The cell count recorded by every dense run of an artist, in paint order.
fn dense_cells_of(scene: &Scene, id: NodeId) -> Vec<u64> {
    fn walk(items: &[Item], id: NodeId, out: &mut Vec<u64>) {
        for item in items {
            match &item.kind {
                ItemKind::Dense { cells, items } => {
                    if item.source == Some(id) {
                        out.push(*cells);
                    }
                    walk(items, id, out);
                }
                ItemKind::Group { items, .. } | ItemKind::Depth { items } => walk(items, id, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&scene.display_list.items, id, &mut out);
    out
}

// WHY: a face with a corner the data cannot place is dropped, and the dense count must count the faces that were
// drawn, once each, whatever the number of leaves they became: a count that followed the leaves would double the
// count of a surface with edges and rasterise it too early, and one that counted the grid's cells would count faces
// that are not on the page.
#[test]
fn a_three_dimensional_surface_counts_each_drawn_face_once_whatever_its_leaves() {
    let grid = linspace(-1.0, 1.0, 4);
    let hole = grid[1];
    let field = move |x: f64, y: f64| {
        if (x - hole).abs() < 1e-9 && (y - hole).abs() < 1e-9 {
            f64::NAN
        } else {
            x * y
        }
    };
    let (scene, _, surf) = surface_scene(&grid, field, |_| {});

    let of_surface = from_source(&leaves(&scene), surf);
    assert_eq!(
        of_surface.len(),
        10,
        "the four faces at the hole are dropped and the five others are two leaves each"
    );
    let cells = dense_cells_of(&scene, surf);
    assert!(
        !cells.is_empty() && cells.iter().all(|c| *c == 5),
        "every dense run of the surface records the five drawn faces: {cells:?}"
    );
}

// WHY: a scatter coloured by data leaves out the points whose value the colour scale cannot map, so the markers
// that remain are a subset of the data; each must still carry the depth of its own point, found through its source
// index and not its position in the array, or every marker after a gap would be drawn at its neighbour's depth.
#[test]
fn a_scatter_coloured_by_data_gives_each_remaining_marker_the_depth_of_its_own_point() {
    let (x, y, z) = (
        [-0.8, -0.2, 0.3, 0.7],
        [0.5, -0.4, 0.1, 0.6],
        [-0.4, 0.2, 0.0, 0.6],
    );
    let values = [1.0, f64::NAN, 3.0, 4.0];
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let scatter = fx.scatter(ax, &x, &y, Some(&z), |s, fx| {
        s.color = ScatterColor::Data {
            data: fx.vector(&values),
        };
    });
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());

    let samples = samples_of(&scene, scatter);
    assert_eq!(
        samples.iter().map(|s| s.source_index).collect::<Vec<_>>(),
        vec![0, 2, 3],
        "the point without a colour is left out"
    );
    let markers = marker_instances(&leaves(&scene), scatter);
    assert_eq!(markers.len(), 3, "one marker per drawn point");
    for index in [0, 2, 3] {
        let expected = depth_at([x[index], y[index], z[index]]);
        let of_point: Vec<&MarkerInstance> =
            markers.iter().filter(|m| m.source_index == index).collect();
        assert_eq!(of_point.len(), 1, "one marker names point {index}");
        assert!(
            (of_point[0].depth - expected).abs() <= 1e-12,
            "the marker of point {index} carries the depth {expected} of its own point, not {}",
            of_point[0].depth
        );
    }
}

// WHY: a line longer than its plot resolves is thinned before it is drawn, so the depths a run carries must be
// those of the points that survived, as the hit map reports them, and not of the raw series at the same positions
// in the array; a run that took the depths of the raw series would drift along the curve as the view changed.
#[test]
fn a_thinned_line_run_carries_the_depths_of_the_points_that_survived() {
    let n = 5000;
    let t: Vec<f64> = (0..n).map(|i| i as f64 / (n - 1) as f64).collect();
    let x: Vec<f64> = t.iter().map(|t| 0.9 * (12.0 * t).cos()).collect();
    let y: Vec<f64> = t.iter().map(|t| 0.9 * (12.0 * t).sin()).collect();
    let z: Vec<f64> = t.iter().map(|t| 2.0 * t - 1.0).collect();
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let line = fx.line(ax, &x, &y, Some(&z), |_| {});
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());

    let samples = samples_of(&scene, line);
    assert!(
        samples.len() < n,
        "the series is thinned: {} of {n} points are drawn",
        samples.len()
    );
    let runs = from_source(&leaves(&scene), line);
    assert!(!runs.is_empty(), "the line is drawn");
    let mut drawn = 0;
    for run in &runs {
        let depths = vertex_depths(run, "a thinned run");
        let vertices = run.endpoints();
        assert_eq!(
            depths.len(),
            vertices.len(),
            "one depth per surviving point"
        );
        for (v, d) in vertices.iter().zip(&depths) {
            let at = run.transform.apply(*v);
            let sample = samples
                .iter()
                .find(|s| points_close(s.position, at, 1e-6))
                .unwrap_or_else(|| panic!("the vertex at {at:?} is a surviving point"));
            assert_eq!(
                *d, sample.depth,
                "the depth of point {} is its sample's",
                sample.source_index
            );
            drawn += 1;
        }
    }
    assert_eq!(
        drawn,
        samples.len(),
        "between them the runs carry every surviving point once"
    );
}

// WHY: an isoline that closes on itself is a closed subpath whose `Close` has no endpoint, so its depths must be one
// per point and not one per segment; a run that counted the closing segment would be one depth too long, fail
// validation and vanish from the depth buffer.
#[test]
fn a_closed_contour3_isoline_carries_one_depth_per_point_and_none_for_its_close() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let g = linspace(-1.0, 1.0, 9);
    let contour = fx.contour(
        ax,
        &g,
        &g,
        |x, y| x * x + y * y,
        |c| {
            c.levels = Levels::Explicit { values: vec![0.5] };
            c.placement = ContourPlacement::AtLevel;
        },
    );
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    let project = projection(&scene, ax);
    let plane = figure_plane_at_height(&project, 0.5);

    let runs = from_source(&leaves(&scene), contour);
    assert_eq!(
        runs.len(),
        1,
        "the level is one closed ring inside the grid"
    );
    let ring = &runs[0];
    let path = ring.path().expect("the ring is a path");
    assert!(
        matches!(path.segments.last(), Some(PathSegment::Close)),
        "the ring is closed"
    );
    let depths = vertex_depths(ring, "the ring");
    let vertices = ring.endpoints();
    assert_eq!(
        depths.len(),
        vertices.len(),
        "one depth per point and none for the close"
    );
    assert!(path.is_valid_depth(), "the ring's depth is usable");
    for (v, d) in vertices.iter().zip(&depths) {
        assert!(
            (plane.at(*v) - d).abs() <= 1e-9,
            "the ring lies at its level: the plane gives {} at {v:?}, the run {d}",
            plane.at(*v)
        );
    }
}
