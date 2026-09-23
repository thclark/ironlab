//! Three-dimensional axes: surfaces, painter's ordering, 3D artists and box decorations.

use ironlab_ir::{Color, ColorSpec, ContourPlacement, Levels, Limits, NodeId, View3d};
use ironlab_scene::Scene;
use ironlab_scene::display::Point;
use ironlab_scene::hit::AxesHitKind;
use ironlab_scene::maths::camera::{Camera, normalise_box};
use ironlab_scene::maths::colormap::{VIRIDIS, normalise};

use crate::common::{Fx, compile_figure, linspace, nearest_lut_index, rgb8, rgb8_close};
use crate::probe::{
    Leaf, axes_hit, from_source, glyph_runs, leaves, marker_instances, parse_number, runs_with_text,
};

const LO: [f64; 3] = [-1.0, -1.0, -1.0];
const HI: [f64; 3] = [1.0, 1.0, 1.0];

fn manual_unit_limits(fx: &mut Fx, ax: NodeId) {
    let axes = fx.ax(ax);
    for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
        axis.limits = Limits::Manual {
            min: -1.0,
            max: 1.0,
        };
    }
}

fn height(x: f64, y: f64) -> f64 {
    0.6 * x * y
}

/// Grid of the fixture surface: six columns and five rows, so the face count distinguishes
/// `(nx − 1)(ny − 1)` from `nx · ny` and from a transposed grid.
fn surface_grid() -> (Vec<f64>, Vec<f64>) {
    (linspace(-1.0, 1.0, 6), linspace(-1.0, 1.0, 5))
}

/// A 3D axes with manual limits [−1, 1]³ holding one surface of `height`.
fn surface_axes(
    view3d: View3d,
    edit: impl FnOnce(&mut ironlab_ir::Surface),
) -> (Fx, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, view3d);
    let (x, y) = surface_grid();
    let surf = fx.surface(ax, &x, &y, height, edit);
    manual_unit_limits(&mut fx, ax);
    (fx, ax, surf)
}

/// The face fills of a surface: its filled leaves of one outline. The edge of a face is a second filled leaf, a
/// ring of two outlines, which [`edge_rings`] returns.
fn filled(leaves: &[Leaf], id: NodeId) -> Vec<Leaf> {
    from_source(leaves, id)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.fill.is_some()) && l.move_to_count() == 1)
        .collect()
}

/// The edge rings of a surface: its filled leaves of two outlines, the face and the face moved inwards.
fn edge_rings(leaves: &[Leaf], id: NodeId) -> Vec<Leaf> {
    from_source(leaves, id)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.fill.is_some()) && l.move_to_count() == 2)
        .collect()
}

/// A data face of the fixture surface: its centroid in data space.
fn data_faces() -> Vec<[f64; 3]> {
    let (x, y) = surface_grid();
    let mut faces = Vec::new();
    for j in 0..y.len() - 1 {
        for i in 0..x.len() - 1 {
            let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            let mut c = [0.0; 3];
            for (ci, cj) in corners {
                c[0] += x[ci] / 4.0;
                c[1] += y[cj] / 4.0;
                c[2] += height(x[ci], y[cj]) / 4.0;
            }
            faces.push(c);
        }
    }
    faces
}

/// Centres a point set and scales it to unit root-mean-square radius.
fn standardise(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let n = points.len() as f64;
    let mx = points.iter().map(|p| p[0]).sum::<f64>() / n;
    let my = points.iter().map(|p| p[1]).sum::<f64>() / n;
    let rms = (points
        .iter()
        .map(|p| (p[0] - mx).powi(2) + (p[1] - my).powi(2))
        .sum::<f64>()
        / n)
        .sqrt();
    points
        .iter()
        .map(|p| [(p[0] - mx) / rms, (p[1] - my) / rms])
        .collect()
}

/// Identifies each painted face of the surface with its data face, returning the data face indices
/// in paint order.
///
/// The projection from data to figure space is an orthographic view followed by a uniform scale, a
/// translation and a flip of y, so after standardising both point sets the projected data centroids
/// and the painted face centroids coincide, whatever scale and offset the layout chose.
fn painted_face_order(scene: &Scene, surf: NodeId, camera: Camera) -> Vec<usize> {
    let leaves = leaves(scene);
    let painted: Vec<[f64; 2]> = filled(&leaves, surf)
        .iter()
        .map(|l| {
            let vs = &l.subpaths()[0];
            let n = vs.len() as f64;
            let cx = vs.iter().map(|p| p.x).sum::<f64>() / n;
            let cy = vs.iter().map(|p| p.y).sum::<f64>() / n;
            [cx, -cy]
        })
        .collect();
    let projected: Vec<[f64; 2]> = data_faces()
        .iter()
        .map(|c| camera.project(normalise_box(*c, LO, HI, [false; 3])).screen)
        .collect();
    assert_eq!(
        painted.len(),
        projected.len(),
        "one painted face per data face"
    );
    let painted = standardise(&painted);
    let projected = standardise(&projected);
    let order: Vec<usize> = painted
        .iter()
        .map(|p| {
            let (k, d) = projected
                .iter()
                .enumerate()
                .map(|(k, q)| (k, ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            assert!(
                d < 0.02,
                "painted face matches a projected data face (distance {d})"
            );
            k
        })
        .collect();
    let mut sorted = order.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), order.len(), "each data face is painted once");
    order
}

fn depth(camera: Camera, face: usize) -> f64 {
    camera
        .project(normalise_box(data_faces()[face], LO, HI, [false; 3]))
        .depth
}

// Why: `surf` draws one flat face per grid cell, attributed to the surface, in axes whose hit
// geometry is manipulated through the camera rather than a 2D data mapping.
#[test]
fn surface_draws_one_face_per_grid_cell() {
    let (fx, ax, surf) = surface_axes(View3d::default(), |_| {});
    let scene = compile_figure(&fx.build());
    assert_eq!(filled(&leaves(&scene), surf).len(), 5 * 4);
    assert_eq!(axes_hit(&scene, ax).kind, AxesHitKind::ThreeD);
}

// Why: `surf` colours each flat face through the colormap by its height and outlines it in black
// (MATLAB `shading faceted`). The flat colour of a face is the colormap sample of the mean of its four
// corner heights, scaled by automatic colour limits equal to the height range, so a face's colour
// represents the whole face rather than one arbitrary corner.
#[test]
fn surface_faces_take_the_colour_of_their_mean_height_with_black_edges() {
    let camera = Camera::default();
    let (fx, _, surf) = surface_axes(View3d::default(), |_| {});
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let faces = filled(&leaves, surf);
    let (x, y) = surface_grid();
    let nodes: Vec<f64> = y
        .iter()
        .flat_map(|yv| x.iter().map(move |xv| height(*xv, *yv)))
        .collect();
    let cmin = nodes.iter().copied().fold(f64::INFINITY, f64::min);
    let cmax = nodes.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let order = painted_face_order(&scene, surf, camera);
    for (painted, data_face) in order.iter().enumerate() {
        let colour = faces[painted].path().unwrap().fill.unwrap().color;
        assert!(
            VIRIDIS.iter().any(|v| rgb8_close(*v, rgb8(colour))),
            "{colour:?} is a viridis colour"
        );
        let mean = data_faces()[*data_face][2];
        let t = normalise(mean, cmin, cmax);
        let expected = ((t * 256.0).floor() as usize).min(255);
        let actual = nearest_lut_index(&VIRIDIS, colour);
        assert!(
            actual.abs_diff(expected) <= 1,
            "face {data_face} with mean height {mean} has colormap entry {actual}, expected {expected}"
        );
    }

    // A face whose projected outline is not convex (a twisted face seen from the side) has no ring and is stroked.
    let rings = edge_rings(&leaves, surf);
    let strokes: Vec<_> = from_source(&leaves, surf)
        .iter()
        .filter_map(|l| l.path().and_then(|p| p.stroke.clone()))
        .collect();
    assert_eq!(
        rings.len() + strokes.len(),
        20,
        "each face has an edge, as a ring or as a stroke"
    );
    assert!(!rings.is_empty(), "the faces seen face-on have edge rings");
    assert!(
        rings
            .iter()
            .all(|r| rgb8(r.path().unwrap().fill.unwrap().color) == [0, 0, 0])
            && strokes.iter().all(|s| rgb8(s.color) == [0, 0, 0]),
        "edges are black"
    );
}

// Why: `mesh` is a wireframe: faces painted in the background colour (hiding what is behind them)
// with edges coloured through the colormap.
#[test]
fn mesh_faces_take_background_and_edges_are_colormapped() {
    let (fx, _, surf) = surface_axes(View3d::default(), |s| {
        s.face = ColorSpec::Rgba {
            color: Color::WHITE,
        };
        s.edge = ColorSpec::Colormapped;
    });
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let faces = filled(&leaves, surf);
    assert_eq!(faces.len(), 20);
    assert!(
        faces
            .iter()
            .all(|f| rgb8(f.path().unwrap().fill.unwrap().color) == [255, 255, 255])
    );
    let mut edge_colours: Vec<[u8; 3]> = edge_rings(&leaves, surf)
        .iter()
        .map(|l| rgb8(l.path().unwrap().fill.unwrap().color))
        .collect();
    assert!(
        edge_colours
            .iter()
            .all(|c| VIRIDIS.iter().any(|v| rgb8_close(*v, *c)))
    );
    edge_colours.sort();
    edge_colours.dedup();
    assert!(
        edge_colours.len() >= 2,
        "edge colours vary with height: {edge_colours:?}"
    );
}

// Why: with flat faces and no depth buffer, faces must be painted back to front. Faces whose depths
// are close may be ordered either way depending on the depth key chosen (centroid, nearest or
// farthest corner), so the test requires the farthest face to precede the nearest and only a small
// fraction of face pairs to be out of depth order; a reversed or unsorted order fails both.
#[test]
fn surface_faces_are_painted_back_to_front() {
    let camera = Camera::default();
    let (fx, _, surf) = surface_axes(View3d::default(), |_| {});
    let scene = compile_figure(&fx.build());
    let order = painted_face_order(&scene, surf, camera);
    let depths: Vec<f64> = order.iter().map(|k| depth(camera, *k)).collect();
    assert!(depths[0] < depths[depths.len() - 1], "{depths:?}");
    let farthest = (0..depths.len())
        .min_by(|a, b| depths[*a].total_cmp(&depths[*b]))
        .unwrap();
    let nearest = (0..depths.len())
        .max_by(|a, b| depths[*a].total_cmp(&depths[*b]))
        .unwrap();
    assert!(
        farthest < nearest,
        "farthest face at {farthest} painted before nearest at {nearest}"
    );
    let n = depths.len();
    let inversions = (0..n)
        .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
        .filter(|(i, j)| depths[*i] > depths[*j])
        .count();
    let pairs = n * (n - 1) / 2;
    assert!(
        (inversions as f64) < 0.1 * pairs as f64,
        "{inversions} of {pairs} face pairs are painted out of depth order"
    );
}

// Why: painter's order depends on the view; turning the camera half way round must re-sort the faces,
// otherwise rotation in the viewer shows hidden faces on top.
#[test]
fn painter_order_follows_the_camera() {
    let default = Camera::default();
    let turned = Camera {
        azimuth_deg: default.azimuth_deg + 180.0,
        elevation_deg: default.elevation_deg,
    };
    let (fx_a, _, surf_a) = surface_axes(View3d::default(), |_| {});
    let (fx_b, _, surf_b) = surface_axes(
        View3d {
            azimuth_deg: turned.azimuth_deg,
            ..View3d::default()
        },
        |_| {},
    );
    let order_a = painted_face_order(&compile_figure(&fx_a.build()), surf_a, default);
    let order_b = painted_face_order(&compile_figure(&fx_b.build()), surf_b, turned);
    assert_ne!(order_a, order_b);
    let first = depth(turned, order_b[0]);
    let last = depth(turned, *order_b.last().unwrap());
    assert!(first < last, "turned view is also painted back to front");
}

// Why: `scatter3`, `quiver3` and `contour3` are the 3D forms of their 2D artists and must all reach
// the display list in 3D axes.
#[test]
fn three_d_scatter_quiver_and_contour_produce_items() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let p = [-0.5, 0.0, 0.5, 0.8];
    let scatter = fx.scatter(ax, &p, &p, Some(&p), |_, _| {});
    let quiver = fx.quiver(ax, &p, &p, Some(&p), &[0.1; 4], &[0.1; 4], Some(&[0.1; 4]));
    let g = linspace(-1.0, 1.0, 9);
    let contour = fx.contour(ax, &g, &g, height, |c| {
        c.placement = ContourPlacement::AtLevel
    });
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    assert_eq!(
        marker_instances(&leaves, scatter).len(),
        4,
        "one marker per point"
    );
    assert_eq!(
        from_source(&leaves, quiver).len(),
        4,
        "one arrow per vector"
    );
    assert!(!from_source(&leaves, contour).is_empty());
}

/// Returns every vertex of every item of `id`, in figure space and paint order.
fn vertices(scene: &Scene, id: NodeId) -> Vec<Point> {
    from_source(&leaves(scene), id)
        .iter()
        .flat_map(|l| l.subpaths().into_iter().flatten())
        .collect()
}

fn same_geometry(a: &[Point], b: &[Point], tol: f64) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| (p.x - q.x).abs() <= tol && (p.y - q.y).abs() <= tol)
}

// Why: `surf` and `mesh` in MATLAB fit the box to the grid in x and y, so the surface reaches the
// edges of the box, while the vertical axis keeps nicely rounded limits so that the box ends on
// labelled heights. A surface compiled with automatic limits must therefore be drawn exactly as with
// manual limits equal to the grid extent in x and y and the rounded height range in z.
#[test]
fn surface_takes_tight_x_and_y_limits_and_nice_z_limits() {
    let build = |manual: bool| {
        let mut fx = Fx::new();
        let ax = fx.axes3d(0, 0, View3d::default());
        let (x, y) = ([-1.3, -0.3, 0.7, 1.7], [0.4, 1.0, 1.6, 2.2]);
        let surf = fx.surface(
            ax,
            &x,
            &y,
            |x, y| 0.13 + 0.74 * (x + 1.3) * (y - 0.4) / 5.4,
            |_| {},
        );
        if manual {
            let axes = fx.ax(ax);
            axes.x.limits = Limits::Manual {
                min: -1.3,
                max: 1.7,
            };
            axes.y.limits = Limits::Manual { min: 0.4, max: 2.2 };
            axes.z.limits = Limits::Manual { min: 0.0, max: 1.0 };
        }
        (compile_figure(&fx.build()), surf)
    };
    let (automatic, a) = build(false);
    let (manual, b) = build(true);
    let (va, vb) = (vertices(&automatic, a), vertices(&manual, b));
    assert!(!va.is_empty());
    assert!(
        same_geometry(&va, &vb, 1e-6),
        "automatic limits differ from the expected ones"
    );
}

// Why: `contour3` lifts each isoline to the height of its level, which must project differently from
// the same isolines laid flat in a plane.
#[test]
fn contour3_at_level_differs_from_planar_contour() {
    let build = |placement| {
        let mut fx = Fx::new();
        let ax = fx.axes3d(0, 0, View3d::default());
        let g = linspace(-1.0, 1.0, 9);
        let id = fx.contour(ax, &g, &g, height, |c| {
            c.placement = placement;
            c.levels = Levels::Explicit {
                values: vec![-0.3, 0.3],
            };
        });
        manual_unit_limits(&mut fx, ax);
        (compile_figure(&fx.build()), id)
    };
    let (at_level, a) = build(ContourPlacement::AtLevel);
    let (planar, b) = build(ContourPlacement::Plane { z: None });
    let va = vertices(&at_level, a);
    let vb = vertices(&planar, b);
    assert!(!va.is_empty() && !vb.is_empty());
    assert!(!same_geometry(&va, &vb, 1.0));
}

// Why: `plot` in 3D axes without z data is MATLAB's `plot3(x, y, zeros)`: the line lies in z = 0.
#[test]
fn line_without_z_in_3d_axes_is_drawn_at_z_zero() {
    let x = [-0.8, 0.0, 0.7];
    let y = [0.5, -0.4, 0.9];
    let build = |z: Option<&[f64]>| {
        let mut fx = Fx::new();
        let ax = fx.axes3d(0, 0, View3d::default());
        let id = fx.line(ax, &x, &y, z, |_| {});
        manual_unit_limits(&mut fx, ax);
        (compile_figure(&fx.build()), id)
    };
    let (implicit, a) = build(None);
    let (explicit, b) = build(Some(&[0.0, 0.0, 0.0]));
    let va = vertices(&implicit, a);
    assert_eq!(va.len(), 3);
    assert!(same_geometry(&va, &vertices(&explicit, b), 1e-6));
}

/// Which axes of the 3D decoration fixture have grid lines enabled.
#[derive(Clone, Copy)]
enum GridOn {
    None,
    XOnly,
}

/// A 3D axes whose x, y and z ranges produce tick labels that are told apart by their text: x
/// labels have decimals, y labels are hundreds and z labels are negative. No limit is a multiple of
/// any plausible tick step, so no grid line coincides with a box edge.
fn decorated_3d(grid: GridOn) -> (Scene, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    fx.line(
        ax,
        &[0.15, 0.85],
        &[150.0, 850.0],
        Some(&[-85.0, -15.0]),
        |_| {},
    );
    let axes = fx.ax(ax);
    axes.x.limits = Limits::Manual {
        min: 0.05,
        max: 0.95,
    };
    axes.y.limits = Limits::Manual {
        min: 110.0,
        max: 890.0,
    };
    axes.z.limits = Limits::Manual {
        min: -95.0,
        max: -15.0,
    };
    axes.x.label = crate::common::text("East");
    axes.y.label = crate::common::text("North");
    axes.z.label = crate::common::text("Height");
    axes.x.grid = matches!(grid, GridOn::XOnly);
    (compile_figure(&fx.build()), ax)
}

fn decoration_segments(scene: &Scene, ax: NodeId) -> usize {
    from_source(&leaves(scene), ax)
        .iter()
        .map(|l| l.line_segments().len())
        .sum()
}

// Why: a 3D axes is only readable with its box, all three axis labels and tick labels on every
// axis.
#[test]
fn three_d_axes_draw_box_axis_labels_and_tick_labels_on_every_axis() {
    let (scene, ax) = decorated_3d(GridOn::None);
    assert!(
        decoration_segments(&scene, ax) >= 9,
        "at least the visible box edges are drawn"
    );
    let leaves = leaves(&scene);
    for label in ["East", "North", "Height"] {
        assert!(
            !runs_with_text(&leaves, label).is_empty(),
            "{label} label is drawn"
        );
    }
    let values: Vec<(String, f64)> = glyph_runs(&leaves)
        .iter()
        .filter_map(|(_, g)| Some((g.text.clone(), parse_number(&g.text)?)))
        .collect();
    assert!(
        values
            .iter()
            .any(|(t, v)| t.contains('.') && (0.05..=0.95).contains(v)),
        "x tick labels: {values:?}"
    );
    assert!(
        values.iter().any(|(_, v)| (110.0..=890.0).contains(v)),
        "y tick labels: {values:?}"
    );
    assert!(
        values
            .iter()
            .any(|(t, v)| t.starts_with('\u{2212}') && (-95.0..=-15.0).contains(v)),
        "z tick labels: {values:?}"
    );
}

// Why: the z label reads from bottom to top beside the vertical axis, as a y label does in 2D, and it
// sits to the left of the z axis and its tick labels so that it covers neither.
#[test]
fn three_d_z_label_reads_upwards_left_of_the_z_tick_labels() {
    let (scene, _) = decorated_3d(GridOn::None);
    let leaves = leaves(&scene);
    let runs = runs_with_text(&leaves, "Height");
    assert!(!runs.is_empty(), "z label glyphs are drawn");
    assert!(
        runs.iter().all(|l| l.reads_upwards()),
        "the z label is rotated to read upwards"
    );
    let label = crate::probe::text_bbox(&leaves, "Height").unwrap();
    let z_ticks: Vec<_> = glyph_runs(&leaves)
        .into_iter()
        .filter(|(_, g)| g.text.starts_with('\u{2212}') && parse_number(&g.text).is_some())
        .filter_map(|(l, _)| l.bbox())
        .collect();
    assert!(!z_ticks.is_empty(), "z tick labels are drawn");
    let leftmost = z_ticks.iter().map(|b| b.x).fold(f64::INFINITY, f64::min);
    assert!(
        label.right() <= leftmost,
        "z label {label:?} left of z tick labels at {leftmost}"
    );
}

// Why: as in MATLAB, each major tick of a 3D axis with its grid on draws one grid line on each of the
// two back planes that contain that axis's direction, and never on the front planes, where grid
// lines would cover the data.
#[test]
fn three_d_x_grid_adds_two_back_plane_lines_per_tick() {
    let (plain, ax) = decorated_3d(GridOn::None);
    let (gridded, gax) = decorated_3d(GridOn::XOnly);
    let x_ticks = glyph_runs(&leaves(&gridded))
        .iter()
        .filter(|(_, g)| g.text.contains('.') && parse_number(&g.text).is_some())
        .count();
    assert!(x_ticks >= 2, "x tick labels are drawn");
    let added = decoration_segments(&gridded, gax) - decoration_segments(&plain, ax);
    assert_eq!(
        added,
        2 * x_ticks,
        "{added} grid segments for {x_ticks} x ticks"
    );
}

/// A 3D axes in the given view holding a surface over [−2, 2]², whose x and y limits end on labelled
/// ticks, so that the tick labels of x and y meet at the corners of the box.
fn corner_labels_3d(view3d: View3d) -> (Scene, NodeId, f64) {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, view3d);
    let grid = linspace(-2.0, 2.0, 9);
    fx.surface(ax, &grid, &grid, |x, y| x * x + y * y, |_| {});
    let font_size = fx.fig.font_size_pt;
    (compile_figure(&fx.build()), ax, font_size)
}

/// Returns the figure-space ink boxes and texts of the numeric tick labels of an axes.
fn tick_label_boxes(scene: &Scene, ax: NodeId) -> Vec<(String, ironlab_scene::display::Rect)> {
    let leaves = leaves(scene);
    glyph_runs(&from_source(&leaves, ax))
        .iter()
        .filter(|(_, g)| parse_number(&g.text).is_some())
        .filter_map(|(l, g)| Some((g.text.clone(), l.bbox()?)))
        .collect()
}

// Why: where the rows of x and y tick labels meet at a corner of the box, or where the lowest z label
// meets the end of a horizontal row, labels can land on top of each other and read as one number
// ("−2−2"). Every pair of tick labels must keep a clear gap, at the default view and at views that
// bring other corners to the front.
#[test]
fn three_d_tick_labels_never_overlap() {
    for (azimuth_deg, elevation_deg) in [(-37.5, 30.0), (45.0, 20.0), (-30.0, 40.0), (-50.0, 35.0)]
    {
        let view = View3d {
            azimuth_deg,
            elevation_deg,
            ..View3d::default()
        };
        let (scene, ax, font_size) = corner_labels_3d(view);
        let boxes = tick_label_boxes(&scene, ax);
        assert!(
            boxes.len() >= 6,
            "view ({azimuth_deg}, {elevation_deg}): {boxes:?}"
        );
        let clearance = 0.25 * font_size;
        for (i, (ta, a)) in boxes.iter().enumerate() {
            for (tb, b) in &boxes[i + 1..] {
                let apart = a.right() + clearance <= b.x
                    || b.right() + clearance <= a.x
                    || a.bottom() + clearance <= b.y
                    || b.bottom() + clearance <= a.y;
                assert!(
                    apart,
                    "view ({azimuth_deg}, {elevation_deg}): {ta} at {a:?} and {tb} at {b:?} collide"
                );
            }
        }
    }
}

/// Returns the length of a segment.
fn segment_length((p, q): (Point, Point)) -> f64 {
    (q.x - p.x).hypot(q.y - p.y)
}

/// Returns the distance from `p` to the segment `(a, b)`.
fn distance_to_segment(p: Point, (a, b): (Point, Point)) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

// Why: as in MATLAB, each labelled edge of a 3D box carries short tick marks at its major ticks, so the
// reader can see exactly which point along the edge a label refers to. A tick mark starts on the box
// edge and points away from the box, towards its label, so that it never cuts into the plotted data.
#[test]
fn three_d_labelled_edges_carry_outward_tick_marks() {
    let (scene, ax, font_size) = corner_labels_3d(View3d::default());
    let segments: Vec<(Point, Point)> = from_source(&leaves(&scene), ax)
        .iter()
        .flat_map(|l| l.line_segments())
        .collect();
    let edges: Vec<_> = segments
        .iter()
        .copied()
        .filter(|s| segment_length(*s) > 5.0 * font_size)
        .collect();
    let marks: Vec<_> = segments
        .iter()
        .copied()
        .filter(|s| {
            let len = segment_length(*s);
            len > 0.1 * font_size && len < font_size
        })
        .collect();
    let labels = tick_label_boxes(&scene, ax);
    assert!(!labels.is_empty());
    assert!(
        marks.len() >= labels.len(),
        "{} marks for {} labels",
        marks.len(),
        labels.len()
    );
    for (text, bbox) in &labels {
        let centre = Point::new(bbox.x + bbox.width / 2.0, bbox.y + bbox.height / 2.0);
        let on_edge = |a: Point| edges.iter().any(|e| distance_to_segment(a, *e) < 1e-6);
        let dist = |a: Point| (a.x - centre.x).hypot(a.y - centre.y);
        let found = marks.iter().any(|&(p, q)| {
            let (inner, outer) = if on_edge(p) { (p, q) } else { (q, p) };
            on_edge(inner) && dist(outer) < dist(inner) && dist(inner) < 4.0 * font_size
        });
        assert!(
            found,
            "label {text} at {bbox:?} has an outward tick mark on the box"
        );
    }
}

/// Returns the figure-space centroid of every filled face of `surf`, in paint order.
fn face_centroids(scene: &Scene, surf: NodeId) -> Vec<Point> {
    filled(&leaves(scene), surf)
        .iter()
        .map(|l| {
            let vs = &l.subpaths()[0];
            let n = vs.len() as f64;
            Point::new(
                vs.iter().map(|p| p.x).sum::<f64>() / n,
                vs.iter().map(|p| p.y).sum::<f64>() / n,
            )
        })
        .collect()
}

// Why: 3D pan is stored as fractions of the plot area, so the viewer can drag the box by a known
// distance: `pan_x` 0.1 must move the whole projection right by a tenth of the plot width, and
// `pan_y` 0.2 must move it down (figure-space y increases downwards) by a fifth of the plot height,
// without changing the layout. Distinct values on the two axes catch the fields being exchanged.
#[test]
fn three_d_pan_shifts_the_projection_by_a_fraction_of_the_plot_rect() {
    let (fx_a, ax, surf_a) = surface_axes(View3d::default(), |_| {});
    let (fx_b, bx, surf_b) = surface_axes(
        View3d {
            pan_x: 0.1,
            pan_y: 0.2,
            ..View3d::default()
        },
        |_| {},
    );
    let (a, b) = (compile_figure(&fx_a.build()), compile_figure(&fx_b.build()));
    let plot = axes_hit(&a, ax).plot_rect;
    assert_eq!(
        plot,
        axes_hit(&b, bx).plot_rect,
        "pan does not change the layout"
    );
    let (ca, cb) = (face_centroids(&a, surf_a), face_centroids(&b, surf_b));
    assert_eq!(ca.len(), cb.len());
    for (p, q) in ca.iter().zip(&cb) {
        crate::probe::assert_close(q.x - p.x, 0.1 * plot.width, 1e-6);
        crate::probe::assert_close(q.y - p.y, 0.2 * plot.height, 1e-6);
    }
}

// Why: 3D zoom magnifies the projection about the centre of the plot area, so zoom 2 doubles every
// face centroid's offset from that centre.
#[test]
fn three_d_zoom_scales_the_projection_about_the_plot_centre() {
    let (fx_a, ax, surf_a) = surface_axes(View3d::default(), |_| {});
    let (fx_b, _, surf_b) = surface_axes(
        View3d {
            zoom: 2.0,
            ..View3d::default()
        },
        |_| {},
    );
    let (a, b) = (compile_figure(&fx_a.build()), compile_figure(&fx_b.build()));
    let plot = axes_hit(&a, ax).plot_rect;
    let centre = crate::probe::centre(plot);
    let (ca, cb) = (face_centroids(&a, surf_a), face_centroids(&b, surf_b));
    assert_eq!(ca.len(), cb.len());
    for (p, q) in ca.iter().zip(&cb) {
        crate::probe::assert_close(q.x - centre.x, 2.0 * (p.x - centre.x), 1e-6);
        crate::probe::assert_close(q.y - centre.y, 2.0 * (p.y - centre.y), 1e-6);
    }
}
