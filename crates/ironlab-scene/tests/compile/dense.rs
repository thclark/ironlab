//! Marking of content that is too dense to be worth drawing as vector geometry.
//!
//! The compiler does not decide whether to rasterise anything; it only records what a backend needs in order to
//! decide, and it must do so without changing the picture. These tests pin both halves of that contract.

use ironlab_ir::NodeId;
use ironlab_scene::display::{Item, ItemKind};

use crate::common::*;

/// The dense groups of the scene, in paint order, as `(source, cells, number of items)`.
fn dense_groups(scene: &ironlab_scene::Scene) -> Vec<(Option<NodeId>, u64, usize)> {
    fn walk(items: &[Item], out: &mut Vec<(Option<NodeId>, u64, usize)>) {
        for item in items {
            match &item.kind {
                ItemKind::Dense { cells, items } => out.push((item.source, *cells, items.len())),
                ItemKind::Group { items, .. } | ItemKind::Depth { items } => walk(items, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&scene.display_list.items, &mut out);
    out
}

// WHY: the PDF exporter chooses between vector and raster output from this count, so a compiler that counted grid
// nodes instead of cells, or counted faces it did not draw, would rasterise the wrong figures. A 9 by 7 grid has
// 8 × 6 = 48 cells, which is distinguishable from the 63 nodes and from either dimension.
#[test]
fn a_surface_marks_its_faces_dense_and_records_how_many_it_drew() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let surface = fx.surface(
        ax,
        &linspace(0.0, 1.0, 9),
        &linspace(0.0, 1.0, 7),
        |x, y| x + y,
        |_| {},
    );
    let scene = compile_figure(&fx.fig);

    assert_eq!(
        dense_groups(&scene),
        vec![(Some(surface), 48, 48)],
        "one dense group holding the 48 faces of the 9 by 7 grid"
    );
}

// WHY: a face whose corners cannot be placed is never drawn, so counting cells of the grid rather than faces emitted
// would tell the exporter to rasterise geometry that is not there, and the raster and the vector drawing would then
// cover different areas. One NaN in the field removes the four faces that touch it.
#[test]
fn a_surface_counts_only_the_faces_it_drew() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let x = linspace(0.0, 1.0, 5);
    let y = linspace(0.0, 1.0, 5);
    let surface = fx.surface(
        ax,
        &x,
        &y,
        |a, b| {
            if a == x[2] && b == y[2] {
                f64::NAN
            } else {
                a + b
            }
        },
        |_| {},
    );
    let scene = compile_figure(&fx.fig);

    assert_eq!(
        dense_groups(&scene),
        vec![(Some(surface), 12, 12)],
        "the 16 faces of the 5 by 5 grid, less the 4 that touch the NaN node"
    );
}

// WHY: the marking must be invisible to a backend that ignores it, otherwise the interactive canvas and a
// vector-only export would draw something different from what the compiler intended. A dense group carries no clip
// and no transform of its own, so its faces must reach a backend under exactly the clip and transform of the other
// geometry of the same axes.
#[test]
fn a_dense_group_changes_neither_the_clip_nor_the_transform_of_its_faces() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let surface = fx.surface(
        ax,
        &linspace(0.0, 1.0, 4),
        &linspace(0.0, 1.0, 4),
        |x, y| x + y,
        |_| {},
    );
    let scene = compile_figure(&fx.fig);
    let leaves = crate::probe::leaves(&scene);

    let faces = crate::probe::from_source(&leaves, surface);
    let lines = crate::probe::from_source(&leaves, line);
    assert_eq!(
        faces.len(),
        9,
        "the 3 by 3 faces of the grid are all leaves"
    );
    assert!(!lines.is_empty(), "the line is drawn");
    for face in &faces {
        assert_eq!(
            face.clip, lines[0].clip,
            "a face is clipped exactly as the line in the same axes is"
        );
        assert_eq!(
            face.transform, lines[0].transform,
            "a face is transformed exactly as the line in the same axes is"
        );
    }
}

// WHY: the threshold is applied per artist, so two surfaces in one axes must be marked separately with their own
// counts; one group covering both would rasterise a small surface because a large one shares its axes, and would
// also fuse two artists into one image that could no longer be replaced independently.
#[test]
fn each_surface_is_marked_separately_with_its_own_count() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let coarse = fx.surface(
        ax,
        &linspace(0.0, 1.0, 3),
        &linspace(0.0, 1.0, 3),
        |x, y| x + y,
        |_| {},
    );
    let fine = fx.surface(
        ax,
        &linspace(0.0, 1.0, 6),
        &linspace(0.0, 1.0, 6),
        |x, y| x - y,
        |_| {},
    );
    let scene = compile_figure(&fx.fig);

    assert_eq!(
        dense_groups(&scene),
        vec![(Some(coarse), 4, 4), (Some(fine), 25, 25)],
        "one group per surface, in artist order, each with its own face count"
    );
}

// WHY: in a 3D axes the depth sort interleaves the faces of a surface with the geometry of other artists, and a
// backend that replaced each run with an image must keep the back-to-front order. The runs are therefore split at
// every interruption, and each one records the artist's whole face count rather than the size of the run, because
// the decision to rasterise is about the artist, not about a fragment of it.
#[test]
fn depth_sorting_splits_a_3d_surface_into_runs_that_each_record_the_whole_count() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, ironlab_ir::View3d::default());
    let n = 7;
    let surface = fx.surface(
        ax,
        &linspace(0.0, 1.0, n),
        &linspace(0.0, 1.0, n),
        |_, _| 0.5,
        |_| {},
    );
    // Markers along the diagonal of the plane, half below it and half above it, so that the depth sort has to place
    // them among the faces rather than in front of or behind all of them.
    let t = linspace(0.0, 1.0, 9);
    let z: Vec<f64> = t.iter().map(|v| if v < &0.5 { 0.1 } else { 0.9 }).collect();
    fx.scatter(ax, &t, &t, Some(&z), |_, _| {});
    let scene = compile_figure(&fx.fig);

    let groups = dense_groups(&scene);
    let faces = (n - 1) * (n - 1);
    assert!(
        groups.len() > 1,
        "the markers interrupt the faces, so the surface is marked as several runs, not {groups:?}"
    );
    assert!(
        groups.iter().all(|g| g.0 == Some(surface)),
        "every dense group belongs to the surface: {groups:?}"
    );
    assert!(
        groups.iter().all(|g| g.1 as usize == faces),
        "every run records the surface's whole face count of {faces}: {groups:?}"
    );
    assert_eq!(
        groups.iter().map(|g| g.2).sum::<usize>(),
        faces,
        "the runs between them hold every face exactly once"
    );
}

// WHY: decimation and dense marking both restructure what the drawing functions emit, and they meet in the same
// stream of items. Two things must hold at that seam. A thinned artist lying between two stretches of a surface must
// break the dense runs rather than be swallowed into one, because a backend replaces a run with an image that would
// otherwise paint over the line. And the count each run records must still be the surface's whole face count, which
// decimation does not touch, so that thinning a line never changes whether the surface beside it is rasterised.
#[test]
fn a_decimated_line_breaks_the_dense_runs_of_a_surface_it_crosses() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, ironlab_ir::View3d::default());
    let n = 7;
    let surface = fx.surface(
        ax,
        &linspace(0.0, 1.0, n),
        &linspace(0.0, 1.0, n),
        |_, _| 0.5,
        |_| {},
    );
    // Far more points than any plot rectangle can resolve, running diagonally from beneath the surface to above it so
    // that the depth sort has to place the line among the faces.
    let count = 5_000;
    let t = linspace(0.0, 1.0, count);
    let z: Vec<f64> = t.iter().map(|v| 0.1 + 0.8 * v).collect();
    let line = fx.line(ax, &t, &t, Some(&z), |_| {});
    let scene = compile_figure(&fx.fig);

    let drawn: usize = crate::probe::from_source(&crate::probe::leaves(&scene), line)
        .iter()
        .flat_map(crate::probe::Leaf::subpaths)
        .map(|sub| sub.len())
        .sum();
    assert!(
        drawn < count,
        "the line is decimated, so this test exercises the seam: {drawn} of {count} points drawn"
    );

    let groups = dense_groups(&scene);
    let faces = (n - 1) * (n - 1);
    assert!(
        groups.len() > 1,
        "the thinned line interrupts the faces, so the surface is marked as several runs, not {groups:?}"
    );
    assert!(
        groups.iter().all(|g| g.0 == Some(surface)),
        "the thinned line is never inside a dense group: {groups:?}"
    );
    assert!(
        groups.iter().all(|g| g.1 as usize == faces),
        "decimating the line leaves the surface's face count at {faces}: {groups:?}"
    );
    assert_eq!(
        groups.iter().map(|g| g.2).sum::<usize>(),
        faces,
        "the runs between them still hold every face exactly once"
    );
}

/// Returns whether an item of `id` lies inside a dense group.
fn inside_dense(items: &[Item], id: NodeId, dense: bool) -> bool {
    items.iter().any(|item| match &item.kind {
        ItemKind::Dense { items, .. } => inside_dense(items, id, true),
        ItemKind::Group { items, .. } | ItemKind::Depth { items } => inside_dense(items, id, dense),
        _ => dense && item.source == Some(id),
    })
}

// WHY: an image is already a raster, so marking it dense would have the PDF exporter rasterise a raster and resample
// the pixels the user supplied; the marking exists for vector geometry too dense to keep as paths, and it must not
// spread to an image drawn beside such geometry in the same axes.
#[test]
fn an_image_is_never_marked_dense_even_beside_a_surface() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let surface = fx.surface(
        ax,
        &linspace(0.0, 1.0, 4),
        &linspace(0.0, 1.0, 4),
        |x, y| x + y,
        |_| {},
    );
    let image = fx.mapped_image(
        ax,
        vec![3, 3],
        (0..9).map(|v| v as f64).collect::<Vec<_>>(),
        xy(range(0.2, 0.8), range(0.2, 0.8)),
        |_| {},
    );
    let scene = compile_figure(&fx.build());

    assert_eq!(
        dense_groups(&scene),
        vec![(Some(surface), 9, 9)],
        "only the surface is marked dense"
    );
    assert!(
        !inside_dense(&scene.display_list.items, image, false),
        "the image item lies outside every dense group"
    );
    assert_eq!(
        crate::probe::from_source(&crate::probe::leaves(&scene), image).len(),
        1,
        "the image is drawn as one leaf"
    );
}
