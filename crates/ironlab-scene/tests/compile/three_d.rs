//! Three-dimensional axes: surfaces, painter's ordering, 3D artists and box decorations.

use ironlab_ir::{Color, ColorSpec, ContourPlacement, Levels, Limits, NodeId, View3d};
use ironlab_scene::Scene;
use ironlab_scene::display::Point;
use ironlab_scene::hit::AxesHitKind;
use ironlab_scene::maths::camera::{Camera, normalise_box};
use ironlab_scene::maths::colormap::{VIRIDIS, normalise};

use crate::common::{Fx, compile_figure, linspace, nearest_lut_index, rgb8, rgb8_close};
use crate::probe::{Leaf, axes_hit, from_source, glyph_runs, leaves, parse_number, runs_with_text};

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

fn filled(leaves: &[Leaf], id: NodeId) -> Vec<Leaf> {
    from_source(leaves, id)
        .into_iter()
        .filter(|l| l.path().is_some_and(|p| p.fill.is_some()))
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

    let strokes: Vec<_> = from_source(&leaves, surf)
        .iter()
        .filter_map(|l| l.path().and_then(|p| p.stroke.clone()))
        .collect();
    assert!(!strokes.is_empty(), "face edges are stroked");
    assert!(
        strokes.iter().all(|s| rgb8(s.color) == [0, 0, 0]),
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
    let mut edge_colours: Vec<[u8; 3]> = from_source(&leaves, surf)
        .iter()
        .filter_map(|l| {
            l.path()
                .and_then(|p| p.stroke.as_ref())
                .map(|s| rgb8(s.color))
        })
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
        from_source(&leaves, scatter).len(),
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
// distance: pan [0.1, 0] must move the whole projection right by a tenth of the plot width and
// nothing else.
#[test]
fn three_d_pan_shifts_the_projection_by_a_fraction_of_the_plot_rect() {
    let (fx_a, ax, surf_a) = surface_axes(View3d::default(), |_| {});
    let (fx_b, bx, surf_b) = surface_axes(
        View3d {
            pan: [0.1, 0.0],
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
        crate::probe::assert_close(q.y - p.y, 0.0, 1e-6);
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
