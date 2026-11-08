//! The three image kinds: one image item each, placed by pixel centres in two and three dimensions, taking tight
//! limits, coloured on the CPU under out-of-range policies, with hit geometry, legend samples and warnings.
//!
//! Pixel space is `[0, nx] × [0, ny]`: pixel `(j, i)` covers `[i, i + 1] × [j, j + 1]` and its samples start at
//! index `(j · nx + i) · channels`. On linear axes the transform of the image leaf maps pixel space into figure space;
//! the tests read that transform through the probes and compare it with the axis maps of a 2D axes, or with a
//! projection rebuilt from the public camera for a 3D axes. On a logarithmic axis the image is drawn through a
//! lattice whose nodes are its pixel edges, and the tests compare each node with the position computed from the
//! definition of a logarithmic axis.

use ironlab_ir::{
    Colour, ColourScaleKind, ColourSpec, ColourmapName, Dimension, ImagePlane, InsideLegend,
    Legend, LegendLocation, Limits, NodeId, OutOfRange, Scale, View3d,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{
    ImageGeometry, ImageItem, ImageMesh, Item, ItemKind, Point, Rect, Transform,
};
use ironlab_scene::hit::{ImageHit, LegendOwner, PixelMap};
use ironlab_scene::maths::camera::{
    Camera, FACE_DEPTH_BIAS, Plane, back_planes, fit_to_rect, normalise_box,
};
use ironlab_scene::maths::colourmap::{Lut, MAGMA, VIRIDIS, normalise, sample};

use crate::common::{
    Fx, compile_figure, linspace, out_of_range, palette_colours, placement, range, rgb8, text, xy,
};
use crate::probe::{
    Leaf, assert_close, axes_hit, axis_maps, disjoint, entry_artist, from_source, inside, leaves,
    pixel, points_close, x_tick_labels,
};

/// A distinct opaque colour for each pixel of an image of up to 25 rows and columns, from its row and column.
fn tag(j: usize, i: usize) -> [u8; 3] {
    [(10 * j + 1) as u8, (10 * i + 1) as u8, 200]
}

/// The 8-bit RGB pixels of an image of `ny` rows and `nx` columns, each coloured by [`tag`].
fn tagged_pixels(ny: usize, nx: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(ny * nx * 3);
    for j in 0..ny {
        for i in 0..nx {
            out.extend(tag(j, i));
        }
    }
    out
}

/// The values 0, 1, …, n − 1 as floating-point values.
fn ramp(n: usize) -> Vec<f64> {
    (0..n).map(|v| v as f64).collect()
}

/// An opaque colour as the four channels the pixel probe reads.
fn opaque(rgb: [u8; 3]) -> [u8; 4] {
    [rgb[0], rgb[1], rgb[2], 255]
}

/// The opaque colour a finite value takes when mapped through the colour limits `cmin` to `cmax`.
fn mapped(lut: &Lut, value: f64, cmin: f64, cmax: f64) -> [u8; 4] {
    opaque(sample(lut, normalise(value, cmin, cmax)).expect("a finite value has a colour"))
}

/// Gives the continuous colour scale of an axes the limits 0 to 10 and the given policies below and above them and
/// for non-finite values.
fn set_policies(
    fx: &mut Fx,
    ax: NodeId,
    below: OutOfRange,
    above: OutOfRange,
    non_finite: OutOfRange,
) {
    let id = fx.axes_scale(ax);
    let scale = fx.scale_mut(id);
    scale.non_finite = out_of_range(non_finite);
    let ColourScaleKind::Continuous(continuous) = &mut scale.kind else {
        panic!("the axes scale is continuous");
    };
    continuous.limits = Limits::Manual {
        min: 0.0,
        max: 10.0,
    };
    continuous.below = out_of_range(below);
    continuous.above = out_of_range(above);
}

/// Returns the one leaf an image artist drew, which must be an image item.
#[track_caller]
fn image_leaf(scene: &Scene, id: NodeId) -> Leaf {
    let drawn = from_source(&leaves(scene), id);
    assert_eq!(drawn.len(), 1, "an image is exactly one leaf: {drawn:?}");
    let leaf = drawn.into_iter().next().unwrap();
    assert!(
        leaf.image().is_some(),
        "the leaf of an image is an image item: {:?}",
        leaf.kind
    );
    leaf
}

/// Returns the image item of the one leaf an image artist drew.
#[track_caller]
fn image_item(scene: &Scene, id: NodeId) -> ImageItem {
    image_leaf(scene, id)
        .image()
        .expect("an image item")
        .clone()
}

/// Returns the messages of the warnings that name `id`.
fn warnings_naming(scene: &Scene, id: NodeId) -> Vec<String> {
    scene
        .warnings
        .iter()
        .filter(|w| w.node == Some(id))
        .map(|w| w.message.clone())
        .collect()
}

/// Asserts that an artist drew nothing and that exactly one warning names it, and returns that warning.
#[track_caller]
fn assert_skipped_with_one_warning(scene: &Scene, id: NodeId, what: &str) -> String {
    assert!(
        from_source(&leaves(scene), id).is_empty(),
        "the {what} is not drawn"
    );
    let warnings = warnings_naming(scene, id);
    assert_eq!(
        warnings.len(),
        1,
        "one warning names the {what}: {:?}",
        scene.warnings
    );
    warnings.into_iter().next().unwrap()
}

/// Asserts that an image artist is drawn as one leaf and that no warning names it.
#[track_caller]
fn assert_drawn_without_warning(scene: &Scene, id: NodeId, what: &str) {
    assert_eq!(
        from_source(&leaves(scene), id).len(),
        1,
        "the {what} is drawn"
    );
    assert!(
        warnings_naming(scene, id).is_empty(),
        "no warning names the {what}: {:?}",
        scene.warnings
    );
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6
}

/// Asserts that the transform of an image leaf in a 2D axes maps the centre of pixel `(j, i)` to the figure
/// position of the data point `(column_centres[i], row_centres[j])`, and the pixel corners at `(0, 0)` and
/// `(nx, ny)` to the data points `(column_edges.0, row_edges.0)` and `(column_edges.1, row_edges.1)`.
#[track_caller]
fn assert_placed_2d(
    scene: &Scene,
    ax: NodeId,
    leaf: &Leaf,
    column_centres: &[f64],
    row_centres: &[f64],
    column_edges: (f64, f64),
    row_edges: (f64, f64),
) {
    let (xmap, ymap) = axis_maps(scene, ax);
    for (j, cy) in row_centres.iter().enumerate() {
        for (i, cx) in column_centres.iter().enumerate() {
            let p = leaf
                .transform
                .apply(Point::new(i as f64 + 0.5, j as f64 + 0.5));
            let expected = Point::new(xmap.to_figure(*cx), ymap.to_figure(*cy));
            assert!(
                points_close(p, expected, 1e-6),
                "the centre of pixel ({j}, {i}) is at {p:?}, expected {expected:?} for the data point ({cx}, {cy})"
            );
        }
    }
    let (nx, ny) = (column_centres.len() as f64, row_centres.len() as f64);
    let corners = [
        ((0.0, 0.0), (column_edges.0, row_edges.0)),
        ((nx, ny), (column_edges.1, row_edges.1)),
    ];
    for ((u, v), (x, y)) in corners {
        let p = leaf.transform.apply(Point::new(u, v));
        let expected = Point::new(xmap.to_figure(x), ymap.to_figure(y));
        assert!(
            points_close(p, expected, 1e-6),
            "the pixel corner ({u}, {v}) is at {p:?}, expected {expected:?} for the data point ({x}, {y})"
        );
    }
}

/// Finds the image item of `id` in the display list and returns the transform of the group directly holding it
/// (`None` when that group has no transform, or when the item lies at the top level) together with whether a
/// dense group encloses it.
fn enclosure(
    items: &[Item],
    id: NodeId,
    parent: Option<Transform>,
    dense: bool,
) -> Option<(Option<Transform>, bool)> {
    for item in items {
        match &item.kind {
            ItemKind::Image(_) if item.source == Some(id) => return Some((parent, dense)),
            ItemKind::Group {
                transform, items, ..
            } => {
                if let Some(found) = enclosure(items, id, *transform, dense) {
                    return Some(found);
                }
            }
            ItemKind::Depth { items } => {
                if let Some(found) = enclosure(items, id, None, dense) {
                    return Some(found);
                }
            }
            ItemKind::Dense { items, .. } => {
                if let Some(found) = enclosure(items, id, None, true) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

const LO: [f64; 3] = [-1.0; 3];
const HI: [f64; 3] = [1.0; 3];

/// Sets manual limits of [−1, 1] on every axis of a 3D axes.
fn manual_unit_limits(fx: &mut Fx, ax: NodeId) {
    let axes = fx.ax(ax);
    for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
        axis.limits = Limits::Manual {
            min: -1.0,
            max: 1.0,
        };
    }
}

/// The index of a dimension in a data point.
fn dim(dimension: Dimension) -> usize {
    match dimension {
        Dimension::X => 0,
        Dimension::Y => 1,
        Dimension::Z => 2,
    }
}

/// The projection of a 3D axes at the default view with limits `lo` to `hi`, rebuilt from the public camera and the
/// documented fit: the unit box is fitted to the plot rectangle by [`fit_to_rect`] about the rectangle's centre,
/// with the screen's y up turned into the page's y down.
fn projection(scene: &Scene, ax: NodeId, lo: [f64; 3], hi: [f64; 3]) -> impl Fn([f64; 3]) -> Point {
    let plot = axes_hit(scene, ax).plot_rect;
    let (scale, offset) = fit_to_rect(plot.width, plot.height);
    let centre = Point::new(plot.x + offset[0], plot.y + offset[1]);
    move |p: [f64; 3]| {
        let screen = Camera::default()
            .project(normalise_box(p, lo, hi, [false; 3]))
            .screen;
        Point::new(centre.x + scale * screen[0], centre.y - scale * screen[1])
    }
}

/// The depth at `camera` of a data point of a box with limits [`LO`] to [`HI`], larger nearer the viewer.
fn depth_at(camera: Camera, p: [f64; 3]) -> f64 {
    camera.project(normalise_box(p, LO, HI, [false; 3])).depth
}

/// The mean depth at `camera` of the four corners of an image in `plane` at `third` along its third axis whose
/// pixel edges span `columns` along the first axis of the plane and `rows` along the second: the key at which a sort
/// by mean depth would place the image.
fn image_mean_depth(
    camera: Camera,
    plane: ImagePlane,
    third: f64,
    columns: (f64, f64),
    rows: (f64, f64),
) -> f64 {
    let [column_axis, row_axis] = plane.axes().map(dim);
    let offset_axis = 3 - column_axis - row_axis;
    let corners = [
        (columns.0, rows.0),
        (columns.1, rows.0),
        (columns.0, rows.1),
        (columns.1, rows.1),
    ];
    corners
        .iter()
        .map(|(u, v)| {
            let mut p = [0.0; 3];
            p[column_axis] = *u;
            p[row_axis] = *v;
            p[offset_axis] = third;
            depth_at(camera, p)
        })
        .sum::<f64>()
        / 4.0
}

/// The grid of the flat surface of [`image_and_surface`]: its sixteen faces are centred at ±0.1125 and ±0.3375 of
/// the box along x and y, so at the default view and at its mirror image about the x axis their depths reach about
/// ±0.41 of the box, on both sides of the centre of every face of the box (whose depth is at most 0.35).
fn straddling_grid() -> Vec<f64> {
    linspace(-0.9, 0.9, 5)
}

/// The depth at `camera` of every face of a flat surface at height `z` over the grid `grid × grid`, as the mean of
/// the depths of its four corners, which is the key at which a 3D axes sorts the edge of the face; the fill of the
/// face is sorted a small bias behind its edge, which is far less than any margin the tests built on this rely on.
fn flat_face_depths(camera: Camera, grid: &[f64], z: f64) -> Vec<f64> {
    let mut depths = Vec::new();
    for xs in grid.windows(2) {
        for ys in grid.windows(2) {
            let corners = [
                (xs[0], ys[0]),
                (xs[1], ys[0]),
                (xs[1], ys[1]),
                (xs[0], ys[1]),
            ];
            depths.push(
                corners
                    .iter()
                    .map(|(x, y)| depth_at(camera, [*x, *y, z]))
                    .sum::<f64>()
                    / 4.0,
            );
        }
    }
    depths
}

/// Asserts that the faces of the flat surface at height `z` over [`straddling_grid`] lie on both sides of `depth`
/// at `camera`: a sort by depth alone would then paint some of them before, and some after, an image keyed at
/// `depth`, so a test built on the surface can tell the face rule from the mean-depth rule.
#[track_caller]
fn assert_faces_straddle(camera: Camera, z: f64, depth: f64) {
    let depths = flat_face_depths(camera, &straddling_grid(), z);
    assert!(
        depths.iter().any(|d| *d < depth) && depths.iter().any(|d| *d > depth),
        "the faces lie on both sides of the depth {depth}: {depths:?}"
    );
}

/// Compiles a 3D axes with manual limits of [−1, 1] on every axis at `view`, holding an image of 2 by 4 pixels
/// whose edges span [−1, 1] along both axes of `plane`, so that an image on a face of the box covers the whole face,
/// followed by a flat surface at height `z` over [`straddling_grid`]. Returns the scene, the image and the surface.
fn image_and_surface(view: View3d, plane: ImagePlane, z: f64) -> (Scene, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, view);
    let image = fx.imagesc(
        ax,
        vec![2, 4],
        ramp(8),
        placement(plane, range(-0.75, 0.75), range(-0.5, 0.5)),
        |_| {},
    );
    let grid = straddling_grid();
    let surface = fx.surface(ax, &grid, &grid, |_, _| z, |_| {});
    manual_unit_limits(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{plane:?}: {:?}", scene.warnings);
    (scene, image, surface)
}

/// Returns the position of every leaf of `id` in the paint order of the scene.
fn paint_positions(leaves: &[Leaf], id: NodeId) -> Vec<usize> {
    (0..leaves.len())
        .filter(|k| leaves[*k].source == Some(id))
        .collect()
}

/// Asserts that the one leaf of `image` is painted before every leaf of the sixteen faces of `surface` (a fill and
/// an edge each) when `before` is true, and after every one of them otherwise.
#[track_caller]
fn assert_image_painted(scene: &Scene, image: NodeId, surface: NodeId, before: bool, what: &str) {
    let leaves = leaves(scene);
    let image_at = paint_positions(&leaves, image);
    assert_eq!(
        image_at.len(),
        1,
        "{what}: the image is one primitive, not split by the depth sort"
    );
    let faces = paint_positions(&leaves, surface);
    assert_eq!(
        faces.len(),
        2 * 16,
        "{what}: a fill leaf and an edge leaf per cell of the 5 by 5 grid"
    );
    let (side, ordered) = if before {
        ("before", faces.iter().all(|k| *k > image_at[0]))
    } else {
        ("after", faces.iter().all(|k| *k < image_at[0]))
    };
    assert!(
        ordered,
        "{what}: the image at {} is painted {side} every face of the surface: {faces:?}",
        image_at[0]
    );
}

/// Returns the endpoints of every straight segment an axes drew, among which are the corners of its box.
fn segment_ends(scene: &Scene, ax: NodeId) -> Vec<Point> {
    from_source(&leaves(scene), ax)
        .iter()
        .flat_map(Leaf::line_segments)
        .flat_map(|(p, q)| [p, q])
        .collect()
}

fn near_any(points: &[Point], p: Point) -> bool {
    points.iter().any(|q| points_close(*q, p, 1e-6))
}

fn transforms_close(a: Transform, b: Transform) -> bool {
    close(a.a, b.a)
        && close(a.b, b.b)
        && close(a.c, b.c)
        && close(a.d, b.d)
        && close(a.e, b.e)
        && close(a.f, b.f)
}

/// Returns every vertex of every path item of `id`, in figure space and paint order.
fn vertices(scene: &Scene, id: NodeId) -> Vec<Point> {
    from_source(&leaves(scene), id)
        .iter()
        .flat_map(|l| l.subpaths().into_iter().flatten())
        .collect()
}

/// Returns the hit entry of an image.
fn image_hit(scene: &Scene, id: NodeId) -> &ImageHit {
    scene
        .hit_map
        .images
        .iter()
        .find(|h| h.artist == id)
        .unwrap_or_else(|| panic!("the hit map has an entry for image {id}"))
}

/// Returns the fill colour and alpha of every filled path the axes drew wholly inside `rect`.
fn patches_in(leaves: &[Leaf], ax: NodeId, rect: Rect) -> Vec<([u8; 3], f32)> {
    leaves
        .iter()
        .filter(|l| l.source == Some(ax) && l.bbox().is_some_and(|b| inside(b, rect, 0.5)))
        .filter_map(|l| l.path().and_then(|p| p.fill))
        .map(|f| (rgb8(f.colour), f.colour.a))
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Emission
// ---------------------------------------------------------------------------------------------------------------

// WHY: both backends draw an image as one raster placed by the transform of the group that holds it, so an artist
// must become exactly one image item in pixel space with its samples in row order, beneath a group carrying the
// placement, inside the clip of its axes: an image expressed in figure space would have to be rebuilt on every pan,
// an unclipped one would paint over the decorations once panned off the plot, a reordered raster would show the
// data upside down, and one inside a dense group would be rasterised a second time by the PDF exporter.
#[test]
fn a_visible_image_is_one_image_item_under_a_transform_group_inside_the_axes_clip() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let line = fx.line(ax, &[0.0, 2.0], &[0.0, 1.0], None, |_| {});
    let (ny, nx) = (2, 3);
    let image = fx.image(
        ax,
        vec![ny, nx, 3],
        tagged_pixels(ny, nx),
        xy(None, None),
        |_| {},
    );
    let scene = compile_figure(&fx.build());

    let leaf = image_leaf(&scene, image);
    let item = leaf.image().unwrap();
    assert_eq!(
        item.geometry,
        ImageGeometry::Rect(Rect::new(0.0, 0.0, 3.0, 2.0)),
        "on linear axes the image is stretched over a rectangle, which is pixel space"
    );
    assert_eq!((item.width, item.height), (3, 2));
    assert_eq!(item.channels, 3, "opaque pixels need no alpha channel");
    for j in 0..ny {
        for i in 0..nx {
            assert_eq!(
                pixel(item, j, i),
                opaque(tag(j, i)),
                "pixel ({j}, {i}) lies at index (j · nx + i) · channels"
            );
        }
    }

    let lines = from_source(&leaves(&scene), line);
    assert!(!lines.is_empty(), "the line is drawn");
    assert!(leaf.clip.is_some(), "the image is clipped");
    assert_eq!(
        leaf.clip, lines[0].clip,
        "the image is clipped exactly as a line in the same axes is"
    );

    let (parent, dense) = enclosure(&scene.display_list.items, image, None, false)
        .expect("the image item is in the display list");
    assert!(
        parent.is_some(),
        "the image lies directly beneath a group carrying its placement transform"
    );
    assert!(!dense, "an image is never wrapped in a dense group");
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
}

// WHY: hiding an artist from its legend entry must move nothing else, as the visibility convention of the compiler
// promises, so a hidden image draws nothing and records no hit geometry while its pixel edges still set the limits;
// otherwise a click in the legend would rescale the axes under the reader.
#[test]
fn a_hidden_image_draws_nothing_but_its_edges_still_set_the_limits() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let line = fx.line(ax, &[1.0, 2.0], &[1.0, 1.5], None, |_| {});
    // Six columns centred from 0.45 to 2.95 (pitch 0.5, edges 0.2 and 3.2) and three rows centred from 0.5 to 1.5
    // (edges 0.25 and 1.75), all reaching beyond the line; no tick rounding of the line's [1, 2] × [1, 1.5], nor of
    // the edges themselves, gives these values, so only the tight edges of the hidden image can.
    let hidden = fx.imagesc(
        ax,
        vec![3, 6],
        ramp(18),
        xy(range(0.45, 2.95), range(0.5, 1.5)),
        |m| m.visible = false,
    );
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    assert!(
        from_source(&leaves, hidden).is_empty(),
        "the hidden image draws nothing"
    );
    assert!(!from_source(&leaves, line).is_empty(), "the line is drawn");
    let (x, y) = axis_maps(&scene, ax);
    assert!(
        close(x.min, 0.2) && close(x.max, 3.2),
        "the edges of the hidden image set the x limits: [{}, {}]",
        x.min,
        x.max
    );
    assert!(
        close(y.min, 0.25) && close(y.max, 1.75),
        "the edges of the hidden image set the y limits: [{}, {}]",
        y.min,
        y.max
    );
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
}

// WHY: an array with no rows or no columns holds no pixels, so there is nothing to draw: such an image, which a
// computation that returned no data can produce, must not panic in the placement (whose pitch divides by n − 1),
// and having no edges it contributes nothing to the limits, which stay those of the other data. ADR 0012 decides
// that the compiler warns of every artist it leaves out, naming it, so that the viewer's problems indicator and
// `validate()` agree about which artists are absent; an empty image of each kind must therefore be reported by one
// warning that names it, and by nothing else.
#[test]
fn an_image_with_no_rows_or_no_columns_is_left_out_with_a_warning_naming_it() {
    let build = |with_empty_images: bool| {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
        let empty = if with_empty_images {
            vec![
                fx.image(ax, vec![0, 3, 3], Vec::<f64>::new(), xy(None, None), |_| {}),
                fx.imagesc(
                    ax,
                    vec![2, 0],
                    Vec::<u8>::new(),
                    xy(range(1.0, 2.0), None),
                    |_| {},
                ),
                fx.indexed_image(ax, vec![0, 0], Vec::<f64>::new(), xy(None, None), |_| {}),
            ]
        } else {
            Vec::new()
        };
        (compile_figure(&fx.build()), ax, line, empty)
    };
    let (scene, ax, line, empty) = build(true);
    assert_eq!(empty.len(), 3);
    for (id, what) in empty.iter().zip(["image", "mapped image", "indexed image"]) {
        assert_skipped_with_one_warning(&scene, *id, &format!("empty {what}"));
    }
    assert!(
        !from_source(&leaves(&scene), line).is_empty(),
        "the line is drawn"
    );
    assert_eq!(
        scene.warnings.len(),
        3,
        "nothing else is reported: {:?}",
        scene.warnings
    );
    // The line alone gives limits of [0, 1] on both axes, and the empty images must leave them so.
    let (alone, alone_ax, _, _) = build(false);
    assert_eq!(
        axis_maps(&scene, ax),
        axis_maps(&alone, alone_ax),
        "the empty images contribute nothing to the limits"
    );
    let (x, y) = axis_maps(&scene, ax);
    assert_eq!((x.min, x.max), (0.0, 1.0));
    assert_eq!((y.min, y.max), (0.0, 1.0));
}

// ---------------------------------------------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------------------------------------------

// WHY: without ranges, as with MATLAB's `image`, column i is centred on x = i and row j on y = j, so the raster
// covers −0.5 to n − 0.5 and its pixels sit on the integer ticks; a compiler that put the pixel edges on the
// integers would shift every image by half a pixel against its ticks, and one whose transform disagreed with its
// limits would draw the image out of register with its own axes.
#[test]
fn default_placement_centres_pixels_on_the_integers_and_fills_the_axes() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let (ny, nx) = (3, 4);
    let image = fx.imagesc(ax, vec![ny, nx], ramp(ny * nx), xy(None, None), |_| {});
    let scene = compile_figure(&fx.build());
    let (x, y) = axis_maps(&scene, ax);
    assert_eq!((x.min, x.max), (-0.5, 3.5));
    assert_eq!((y.min, y.max), (-0.5, 2.5));
    let leaf = image_leaf(&scene, image);
    assert_placed_2d(
        &scene,
        ax,
        &leaf,
        &ramp(nx),
        &ramp(ny),
        (-0.5, 3.5),
        (-0.5, 2.5),
    );
    let plot = axes_hit(&scene, ax).plot_rect;
    let covered = leaf.bbox().unwrap();
    assert!(
        inside(covered, plot, 1e-6) && inside(plot, covered, 1e-6),
        "the image {covered:?} fills the plot rectangle {plot:?}"
    );
}

// WHY: explicit ranges register an image with physical coordinates: the centre of the first column lands on
// `first`, that of the last on `last`, the pitch follows from the count, and the raster covers half a pitch beyond
// each end. Placing the edges on the range, or reading the ranges as pixel edges, would shift the image against data
// plotted over it; and the ranges must not touch the order of the samples.
#[test]
fn explicit_ranges_place_the_first_and_last_centres_with_the_pitch_between_them() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let (ny, nx) = (3, 4);
    // Columns centred on 1, 2, 3 and 4 (pitch 1, edges 0.5 and 4.5); rows centred on 10, 15 and 20 (pitch 5, edges
    // 7.5 and 22.5).
    let image = fx.image(
        ax,
        vec![ny, nx, 3],
        tagged_pixels(ny, nx),
        xy(range(1.0, 4.0), range(10.0, 20.0)),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    let leaf = image_leaf(&scene, image);
    assert_placed_2d(
        &scene,
        ax,
        &leaf,
        &[1.0, 2.0, 3.0, 4.0],
        &[10.0, 15.0, 20.0],
        (0.5, 4.5),
        (7.5, 22.5),
    );
    let item = leaf.image().unwrap();
    for j in 0..ny {
        for i in 0..nx {
            assert_eq!(
                pixel(item, j, i),
                opaque(tag(j, i)),
                "the ranges leave pixel ({j}, {i}) at its index"
            );
        }
    }
}

// WHY: a range running backwards mirrors the image along that axis by moving the pixels, not the samples: row 0
// still holds the first row of the array and lands on `rows.first`, which is now the higher y and so higher on the
// page, while the pixel corner at v = 0 maps to the edge beyond `first`. Reordering the samples instead would break
// the hit map's row and column, which index the user's array, and mirroring only the limits would leave the
// transform pointing the wrong way.
#[test]
fn a_mirrored_range_flips_the_transform_and_leaves_the_samples_in_row_order() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let (ny, nx) = (4, 3);
    // Columns centred on 5, 3 and 1 (pitch −2, edges 6 and 0); rows centred on 3, 2, 1 and 0 (pitch −1, edges 3.5
    // and −0.5).
    let image = fx.image(
        ax,
        vec![ny, nx, 3],
        tagged_pixels(ny, nx),
        xy(range(5.0, 1.0), range(3.0, 0.0)),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    let leaf = image_leaf(&scene, image);
    assert_placed_2d(
        &scene,
        ax,
        &leaf,
        &[5.0, 3.0, 1.0],
        &[3.0, 2.0, 1.0, 0.0],
        (6.0, 0.0),
        (3.5, -0.5),
    );
    let at = |u: f64, v: f64| leaf.transform.apply(Point::new(u, v));
    assert!(
        at(0.5, 0.5).y < at(0.5, 3.5).y,
        "row 0 lies higher on the page than row 3"
    );
    assert!(
        at(0.5, 0.5).x > at(2.5, 0.5).x,
        "column 0 lies to the right of column 2"
    );
    assert!(
        at(0.0, 0.0).y < at(0.0, 4.0).y,
        "the row axis of the transform points down the page"
    );
    let (x, y) = axis_maps(&scene, ax);
    assert_eq!(
        (x.min, x.max),
        (0.0, 6.0),
        "the limits are the pixel edges in ascending order"
    );
    assert_eq!((y.min, y.max), (-0.5, 3.5));
    let item = leaf.image().unwrap();
    for j in 0..ny {
        for i in 0..nx {
            assert_eq!(
                pixel(item, j, i),
                opaque(tag(j, i)),
                "mirroring leaves pixel ({j}, {i}) at its index"
            );
        }
    }
}

// WHY: one pixel along an axis has no pitch to derive from two centres, so it is one data unit wide about its first
// centre, and its `last` is ignored (it may even equal `first`, which validation allows for one pixel); a compiler
// that divided by n − 1 would place such an image at infinity, or refuse a single row that is perfectly drawable.
#[test]
fn a_single_column_or_row_is_one_data_unit_wide_whatever_its_range() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let tall = fx.axes2d(0, 0);
    // One column centred on 2 (its `last` of 7 is ignored) and three rows centred on 4, 5 and 6.
    let column = fx.imagesc(
        tall,
        vec![3, 1],
        ramp(3),
        xy(range(2.0, 7.0), range(4.0, 6.0)),
        |_| {},
    );
    let wide = fx.axes2d(0, 1);
    // Two columns centred on 0 and 3 and one row centred on 4, whose first and last centres coincide.
    let row = fx.imagesc(
        wide,
        vec![1, 2],
        ramp(2),
        xy(range(0.0, 3.0), range(4.0, 4.0)),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let leaf = image_leaf(&scene, column);
    assert_placed_2d(
        &scene,
        tall,
        &leaf,
        &[2.0],
        &[4.0, 5.0, 6.0],
        (1.5, 2.5),
        (3.5, 6.5),
    );
    let (x, y) = axis_maps(&scene, tall);
    assert_eq!((x.min, x.max), (1.5, 2.5));
    assert_eq!((y.min, y.max), (3.5, 6.5));

    let leaf = image_leaf(&scene, row);
    assert_placed_2d(
        &scene,
        wide,
        &leaf,
        &[0.0, 3.0],
        &[4.0],
        (-1.5, 4.5),
        (3.5, 4.5),
    );
    let (x, y) = axis_maps(&scene, wide);
    assert_eq!((x.min, x.max), (-1.5, 4.5));
    assert_eq!((y.min, y.max), (3.5, 4.5));
}

// WHY: the offset of the xy plane is the height of the image in a 3D axes and means nothing in a 2D one, as the
// height of a planar contour does; a 2D axes must therefore draw an image at any xy offset exactly as it draws one
// with none, without a warning, so a figure can be switched between projections without editing its images.
#[test]
fn an_xy_offset_is_ignored_by_a_two_dimensional_axes() {
    let build = |z: Option<f64>| {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let image = fx.imagesc(
            ax,
            vec![2, 3],
            ramp(6),
            placement(ImagePlane::Xy { z }, range(1.0, 3.0), None),
            |_| {},
        );
        let scene = compile_figure(&fx.build());
        (scene, ax, image)
    };
    let (plain, plain_ax, plain_image) = build(None);
    let (lifted, lifted_ax, lifted_image) = build(Some(7.5));
    assert!(lifted.warnings.is_empty(), "{:?}", lifted.warnings);
    assert_eq!(
        axis_maps(&plain, plain_ax),
        axis_maps(&lifted, lifted_ax),
        "the offset does not touch the limits"
    );
    let (a, b) = (
        image_leaf(&plain, plain_image),
        image_leaf(&lifted, lifted_image),
    );
    assert_eq!(
        a.transform, b.transform,
        "the offset does not move the image"
    );
    assert_eq!(a.image().unwrap().samples, b.image().unwrap().samples);
}

// ---------------------------------------------------------------------------------------------------------------
// Three dimensions
// ---------------------------------------------------------------------------------------------------------------

// WHY: in a 3D axes an image is a flat raster on the floor or a wall, so its transform must carry the projection of
// its plane: the columns run along the first axis the plane names and the rows along the second, the third
// coordinate is the offset or the low end of that axis, and the corners of a raster whose edges are the limits
// coincide with corners of the box. A transform built in the wrong plane, with the axes exchanged, or at the wrong
// offset would show the data transposed or standing on the wrong wall.
#[test]
fn in_three_dimensions_an_image_lies_on_its_plane_with_columns_along_its_first_axis() {
    let cases = [
        (ImagePlane::Xy { z: None }, -1.0),
        (ImagePlane::Xy { z: Some(1.0) }, 1.0),
        (ImagePlane::Xz { y: None }, -1.0),
        (ImagePlane::Xz { y: Some(1.0) }, 1.0),
        (ImagePlane::Yz { x: None }, -1.0),
        (ImagePlane::Yz { x: Some(1.0) }, 1.0),
    ];
    for (plane, third) in cases {
        let mut fx = Fx::new();
        let ax = fx.axes3d(0, 0, View3d::default());
        let (ny, nx) = (2, 4);
        // Pixel edges at −1 and 1 along both axes of the plane: four columns of pitch 0.5, two rows of pitch 1.
        let image = fx.imagesc(
            ax,
            vec![ny, nx],
            ramp(ny * nx),
            placement(plane, range(-0.75, 0.75), range(-0.5, 0.5)),
            |_| {},
        );
        manual_unit_limits(&mut fx, ax);
        fx.ax(ax).box_ = true;
        let scene = compile_figure(&fx.build());
        assert!(scene.warnings.is_empty(), "{plane:?}: {:?}", scene.warnings);
        let leaf = image_leaf(&scene, image);

        let corners = segment_ends(&scene, ax);
        for (u, v) in [(0.0, 0.0), (4.0, 0.0), (0.0, 2.0), (4.0, 2.0)] {
            let p = leaf.transform.apply(Point::new(u, v));
            assert!(
                near_any(&corners, p),
                "{plane:?}: the pixel corner ({u}, {v}) at {p:?} is a corner of the box"
            );
        }

        let project = projection(&scene, ax, LO, HI);
        let [columns, rows] = plane.axes().map(dim);
        let offset_axis = 3 - columns - rows;
        for j in 0..ny {
            for i in 0..nx {
                let mut data = [0.0; 3];
                data[columns] = -0.75 + 0.5 * i as f64;
                data[rows] = -0.5 + j as f64;
                data[offset_axis] = third;
                let expected = project(data);
                let actual = leaf
                    .transform
                    .apply(Point::new(i as f64 + 0.5, j as f64 + 0.5));
                assert!(
                    points_close(actual, expected, 1e-6),
                    "{plane:?}: the centre of pixel ({j}, {i}) is at {actual:?}, expected {expected:?} for {data:?}"
                );
            }
        }
    }
}

// WHY: a 3D axes paints back to front, and an image inside the box is one primitive whose key is the mean depth of
// its four corners, as a face's key is the mean of its corners (its fill a small bias behind its edge); the depth
// sort does not split it. So a surface just above an image at mid-height near the centre of the box, whose faces
// are all nearer than that mean, is painted after the image, while faces farther than the mean, though nearer than
// the image's farthest corner, are painted before it. An image keyed on its nearest or its farthest corner would
// order one of these the other way round, and one split into pieces would no longer be the single raster the
// backends draw. Each face is a fill leaf and an edge leaf, and both must fall on the right side of the image.
#[test]
fn an_image_at_an_interior_offset_is_one_primitive_sorted_by_the_mean_depth_of_its_corners() {
    let camera = Camera::default();
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    // The whole xy plane at mid-height, strictly inside the z limits of [−1, 1]: at the default view its corners
    // lie at depths from about −0.61 to 0.61 in the normalised box, with a mean of 0.
    let plane = ImagePlane::Xy { z: Some(0.0) };
    let image = fx.imagesc(
        ax,
        vec![2, 2],
        ramp(4),
        placement(plane, range(-0.5, 0.5), range(-0.5, 0.5)),
        |_| {},
    );
    // Centred and just above the image: every face lies between the mean and the nearest corner in depth.
    let centred_grid = linspace(-0.2, 0.2, 3);
    let centred = fx.surface(ax, &centred_grid, &centred_grid, |_, _| 0.4, |_| {});
    // Towards the far corner and below the image: every face lies between the farthest corner and the mean in depth.
    let far_grid = linspace(0.4, 0.8, 3);
    let far = fx.surface(ax, &far_grid, &far_grid, |_, _| -0.4, |_| {});
    manual_unit_limits(&mut fx, ax);

    let corner_depths: Vec<f64> = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .iter()
        .map(|(x, y)| depth_at(camera, [*x, *y, 0.0]))
        .collect();
    let (nearest, farthest) = (
        corner_depths
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max),
        corner_depths.iter().copied().fold(f64::INFINITY, f64::min),
    );
    let mean = image_mean_depth(camera, plane, 0.0, (-1.0, 1.0), (-1.0, 1.0));
    assert!(
        flat_face_depths(camera, &centred_grid, 0.4)
            .iter()
            .all(|d| mean < *d && *d < nearest),
        "the centred faces lie between the mean depth {mean} and the nearest corner {nearest}"
    );
    assert!(
        flat_face_depths(camera, &far_grid, -0.4)
            .iter()
            .all(|d| farthest < *d && *d < mean),
        "the far faces lie between the farthest corner {farthest} and the mean depth {mean}"
    );

    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let image = paint_positions(&leaves, image);
    assert_eq!(
        image.len(),
        1,
        "the image is one primitive, not split by the depth sort"
    );
    let (centred_faces, far_faces) = (
        paint_positions(&leaves, centred),
        paint_positions(&leaves, far),
    );
    assert_eq!(
        (centred_faces.len(), far_faces.len()),
        (2 * 4, 2 * 4),
        "a fill leaf and an edge leaf per cell of each 3 by 3 grid"
    );
    assert!(
        far_faces.iter().all(|k| *k < image[0]),
        "faces farther than the mean depth of the image are painted before it: {far_faces:?} before {}",
        image[0]
    );
    assert!(
        centred_faces.iter().all(|k| *k > image[0]),
        "faces nearer than the mean depth of the image are painted after it: {centred_faces:?} after {}",
        image[0]
    );
}

// WHY: a floor image lies on the z = min face of the box, which is a back plane of the default view, so the whole of
// a surface above it is nearer the viewer than the floor; but the mean depth of the floor lies in the middle of the
// box, so a sort by mean depth would paint the floor over the faces of the surface that are farther than the centre
// of the floor, cutting the back of the surface away. An image on a back face of the box must be painted before
// every other primitive of the axes, whatever the depths of its corners.
#[test]
fn a_floor_image_is_painted_before_every_face_of_a_surface_above_it() {
    let camera = Camera::default();
    assert_eq!(
        back_planes(&camera)[2],
        Plane::ZMin,
        "the floor is a back plane of the default view"
    );
    let plane = ImagePlane::Xy { z: None };
    assert_faces_straddle(
        camera,
        0.0,
        image_mean_depth(camera, plane, -1.0, (-1.0, 1.0), (-1.0, 1.0)),
    );
    let (scene, image, surface) = image_and_surface(View3d::default(), plane, 0.0);
    assert_image_painted(&scene, image, surface, true, "floor");
}

// WHY: whether an image on a wall is behind or in front of the data depends on the view alone. The x = min wall
// faces the viewer at the default view, so an image on it must be painted after every face of a surface, which it
// then covers; turning the camera to an azimuth of 37.5° puts the same wall at the back, where the image must be
// painted before every face. A key taken from the depths of the corners of the wall would interleave the faces with
// the image either way.
#[test]
fn an_image_on_a_front_wall_is_painted_after_every_face_and_on_a_back_wall_before_them() {
    let plane = ImagePlane::Yz { x: None };
    for (azimuth_deg, back_plane, before) in
        [(-37.5, Plane::XMax, false), (37.5, Plane::XMin, true)]
    {
        let camera = Camera {
            azimuth_deg,
            elevation_deg: 30.0,
        };
        assert_eq!(
            back_planes(&camera)[0],
            back_plane,
            "the back plane of the x axis at an azimuth of {azimuth_deg}°"
        );
        assert_faces_straddle(
            camera,
            0.0,
            image_mean_depth(camera, plane, -1.0, (-1.0, 1.0), (-1.0, 1.0)),
        );
        let view = View3d {
            azimuth_deg,
            ..View3d::default()
        };
        let (scene, image, surface) = image_and_surface(view, plane, 0.0);
        assert_image_painted(
            &scene,
            image,
            surface,
            before,
            &format!("azimuth {azimuth_deg}°"),
        );
    }
}

// WHY: an image whose explicit offset is a limit of its third axis lies on a face of the box exactly as an image
// without an offset does, whether the limit is the lower or the upper one, and the face is at the back or the front
// of the view as `back_planes` says; only an explicit offset can reach the ceiling and the two far walls. Treating
// only an absent offset as a face would leave those images sorted by mean depth, painted over the faces behind their
// centres or under the faces in front of them.
#[test]
fn an_image_whose_offset_is_a_limit_of_its_axis_lies_on_that_face_of_the_box() {
    let camera = Camera::default();
    let back = back_planes(&camera);
    let cases = [
        (ImagePlane::Xy { z: Some(-1.0) }, Plane::ZMin),
        (ImagePlane::Xy { z: Some(1.0) }, Plane::ZMax),
        (ImagePlane::Xz { y: Some(-1.0) }, Plane::YMin),
        (ImagePlane::Xz { y: Some(1.0) }, Plane::YMax),
        (ImagePlane::Yz { x: Some(-1.0) }, Plane::XMin),
        (ImagePlane::Yz { x: Some(1.0) }, Plane::XMax),
    ];
    assert_eq!(
        cases.iter().filter(|(_, face)| back.contains(face)).count(),
        3,
        "three of the faces are back planes and three are front faces"
    );
    for (plane, face) in cases {
        let third = plane.offset().expect("every case has an explicit offset");
        assert_faces_straddle(
            camera,
            0.0,
            image_mean_depth(camera, plane, third, (-1.0, 1.0), (-1.0, 1.0)),
        );
        let (scene, image, surface) = image_and_surface(View3d::default(), plane, 0.0);
        assert_image_painted(
            &scene,
            image,
            surface,
            back.contains(&face),
            &format!("{plane:?}"),
        );
    }
}

// WHY: images on the same face of the box are coplanar, so no depth can order them, and the reader expects the later
// artist to cover the earlier one as in a 2D axes. The depth sort is stable, so images keyed alike keep their artist
// order, on a back face and on a front face alike, even where a sort by mean depth would reverse them; a key that
// varied from image to image would let the covering image change with the view.
#[test]
fn images_on_the_same_face_of_the_box_keep_artist_order() {
    let camera = Camera::default();
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let floor = ImagePlane::Xy { z: None };
    let wall = ImagePlane::Yz { x: None };
    // The whole floor, then the far half of it, whose mean depth is smaller.
    let first_floor = fx.imagesc(
        ax,
        vec![2, 4],
        ramp(8),
        placement(floor, range(-0.75, 0.75), range(-0.5, 0.5)),
        |_| {},
    );
    let second_floor = fx.imagesc(
        ax,
        vec![2, 4],
        ramp(8),
        placement(floor, range(0.125, 0.875), range(-0.5, 0.5)),
        |_| {},
    );
    let grid = straddling_grid();
    let surface = fx.surface(ax, &grid, &grid, |_, _| 0.0, |_| {});
    // The whole x = min wall, then its lower far quarter, whose mean depth is smaller.
    let first_wall = fx.imagesc(
        ax,
        vec![2, 4],
        ramp(8),
        placement(wall, range(-0.75, 0.75), range(-0.5, 0.5)),
        |_| {},
    );
    let second_wall = fx.imagesc(
        ax,
        vec![2, 4],
        ramp(8),
        placement(wall, range(0.125, 0.875), range(-0.75, -0.25)),
        |_| {},
    );
    manual_unit_limits(&mut fx, ax);
    assert_eq!(
        back_planes(&camera),
        [Plane::XMax, Plane::YMax, Plane::ZMin]
    );
    assert!(
        image_mean_depth(camera, floor, -1.0, (0.0, 1.0), (-1.0, 1.0))
            < image_mean_depth(camera, floor, -1.0, (-1.0, 1.0), (-1.0, 1.0)),
        "a sort by mean depth would paint the second floor image first"
    );
    assert!(
        image_mean_depth(camera, wall, -1.0, (0.0, 1.0), (-1.0, 0.0))
            < image_mean_depth(camera, wall, -1.0, (-1.0, 1.0), (-1.0, 1.0)),
        "a sort by mean depth would paint the second wall image first"
    );

    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let leaves = leaves(&scene);
    let at = |id: NodeId| -> usize {
        let positions = paint_positions(&leaves, id);
        assert_eq!(positions.len(), 1, "image {id} is one primitive");
        positions[0]
    };
    let faces = paint_positions(&leaves, surface);
    assert_eq!(
        faces.len(),
        2 * 16,
        "a fill leaf and an edge leaf per cell of the 5 by 5 grid"
    );
    assert!(
        at(first_floor) < at(second_floor),
        "the floor images keep artist order on the back face"
    );
    assert!(
        faces.iter().all(|k| *k > at(second_floor)),
        "both floor images are painted before the surface"
    );
    assert!(
        faces.iter().all(|k| *k < at(first_wall)),
        "both wall images are painted after the surface"
    );
    assert!(
        at(first_wall) < at(second_wall),
        "the wall images keep artist order on the front face"
    );
}

// WHY: the xz and yz planes are the walls of a 3D axes and have no place in a 2D one, as `contour3` and `plot3` data
// have none, so such an image is skipped with one warning naming it while the rest of the axes is still drawn;
// projecting a wall onto the page, or panicking, would misrepresent or lose the figure.
#[test]
fn a_wall_image_in_a_two_dimensional_axes_is_not_drawn_with_a_warning_naming_it() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let xz = fx.imagesc(
        ax,
        vec![2, 2],
        ramp(4),
        placement(ImagePlane::Xz { y: None }, None, None),
        |_| {},
    );
    let yz = fx.image(
        ax,
        vec![2, 2, 3],
        tagged_pixels(2, 2),
        placement(ImagePlane::Yz { x: Some(1.0) }, None, None),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    assert_skipped_with_one_warning(&scene, xz, "xz image in a 2D axes");
    assert_skipped_with_one_warning(&scene, yz, "yz image in a 2D axes");
    assert!(
        !from_source(&leaves(&scene), line).is_empty(),
        "the line is still drawn"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------------------------------------------

// WHY: as with `contour` and `surf`, an image fills its axes: the automatic limits along its plane are exactly the
// pixel edges, not those edges rounded outward to ticks, so that no empty band appears between the raster and the
// box.
#[test]
fn an_axes_holding_only_an_image_takes_the_pixel_edges_as_its_limits() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    // Four columns of pitch 0.8 (edges −0.05 and 3.15) and three rows of pitch 0.3 (edges 0.95 and 1.85), none of
    // which is a tick value.
    fx.imagesc(
        ax,
        vec![3, 4],
        ramp(12),
        xy(range(0.35, 2.75), range(1.1, 1.7)),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    let (x, y) = axis_maps(&scene, ax);
    assert_close(x.min, -0.05, 1e-9);
    assert_close(x.max, 3.15, 1e-9);
    assert_close(y.min, 0.95, 1e-9);
    assert_close(y.max, 1.85, 1e-9);
}

// WHY: tight limits describe the image alone; a line reaching beyond it still widens that end of the axis, rounded
// outward to a labelled tick as for any other data, while the end the image alone reaches stays at the pixel edge.
#[test]
fn other_data_beyond_an_image_rounds_that_end_outward_to_a_tick() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    // Six columns of pitch 0.5 with edges 0.2 and 3.2 along x, which no tick rounding of the data gives, and rows
    // with edges 0.25 and 1.75 along y.
    fx.imagesc(
        ax,
        vec![3, 6],
        ramp(18),
        xy(range(0.45, 2.95), range(0.5, 1.5)),
        |_| {},
    );
    fx.line(ax, &[1.0, 4.3], &[1.0, 1.5], None, |_| {});
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let (x, y) = axis_maps(&scene, ax);
    assert!(
        close(x.min, 0.2),
        "the image alone sets the left limit, at its pixel edge rather than at a tick: {}",
        x.min
    );
    assert!(x.max > 4.3, "the line extends the right limit: {}", x.max);
    let labels = x_tick_labels(&leaves(&scene), plot);
    assert!(
        labels.iter().any(|l| (l.value - x.max).abs() < 1e-9),
        "the extended limit {} is a labelled tick",
        x.max
    );
    assert_eq!(
        (y.min, y.max),
        (0.25, 1.75),
        "the line lies inside the image in y"
    );
}

/// Compiles a 3D axes holding one image on the xz wall whose pixel edges lie at −1.3 and 1.7 along x and at 0.1 and
/// 2.9 along z, with automatic limits or with the given manual x, y and z limits, and returns the image leaf and the
/// endpoints of the segments the axes drew.
fn wall_image(limits: Option<[(f64, f64); 3]>) -> (Leaf, Vec<Point>) {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let image = fx.imagesc(
        ax,
        vec![4, 4],
        ramp(16),
        placement(
            ImagePlane::Xz { y: None },
            range(-0.925, 1.325),
            range(0.45, 2.55),
        ),
        |_| {},
    );
    if let Some(limits) = limits {
        let axes = fx.ax(ax);
        for (axis, (min, max)) in [&mut axes.x, &mut axes.y, &mut axes.z]
            .into_iter()
            .zip(limits)
        {
            axis.limits = Limits::Manual { min, max };
        }
    }
    let scene = compile_figure(&fx.build());
    (image_leaf(&scene, image), segment_ends(&scene, ax))
}

// WHY: along the two axes of its plane an image takes tight limits, as in 2D; along z, whose limits are always
// rounded outward so that the box ends on labelled heights, the edges of a wall image are rounded like every other
// z extent; and an absent offset contributes nothing along the third axis, which keeps the default limits of
// [0, 1]. An automatic wall image must therefore be drawn exactly as with those limits set by hand.
#[test]
fn a_wall_image_takes_tight_limits_along_x_and_rounded_limits_along_z() {
    let (automatic, automatic_box) = wall_image(None);
    let (expected, expected_box) = wall_image(Some([(-1.3, 1.7), (0.0, 1.0), (0.0, 3.0)]));
    assert!(
        transforms_close(automatic.transform, expected.transform),
        "automatic limits place the image as manual limits of x [−1.3, 1.7], y [0, 1] and z [0, 3] do: {:?} vs {:?}",
        automatic.transform,
        expected.transform
    );
    assert_eq!(
        automatic_box.len(),
        expected_box.len(),
        "the box and its ticks are drawn alike"
    );
    assert!(
        automatic_box
            .iter()
            .zip(&expected_box)
            .all(|(p, q)| points_close(*p, *q, 1e-6)),
        "the box is drawn where those manual limits put it"
    );
}

/// A 3D axes holding a surface of heights 0 to 1 over [−1, 1]² and an image at height 10 in the xy plane, with
/// automatic limits or with manual limits of x [−1, 1], y [−1, 1] and z [0, 10]; returns the scene and the two
/// artists.
fn lifted_image(manual: bool) -> (Scene, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let g = linspace(-1.0, 1.0, 3);
    let surface = fx.surface(ax, &g, &g, |x, y| (x + 1.0) * (y + 1.0) / 4.0, |_| {});
    let image = fx.imagesc(
        ax,
        vec![2, 4],
        ramp(8),
        placement(
            ImagePlane::Xy { z: Some(10.0) },
            range(-0.75, 0.75),
            range(-0.5, 0.5),
        ),
        |_| {},
    );
    if manual {
        let axes = fx.ax(ax);
        axes.x.limits = Limits::Manual {
            min: -1.0,
            max: 1.0,
        };
        axes.y.limits = Limits::Manual {
            min: -1.0,
            max: 1.0,
        };
        axes.z.limits = Limits::Manual {
            min: 0.0,
            max: 10.0,
        };
    }
    (compile_figure(&fx.build()), surface, image)
}

// WHY: an image lifted to an explicit height (or set on a wall at an explicit offset) occupies that coordinate, so
// the third axis must extend to it or the image would be drawn outside the box; a surface of heights 0 to 1 beneath
// an image at z = 10 therefore takes z limits of [0, 10], and both must be drawn exactly as with those limits set by
// hand.
#[test]
fn an_explicit_plane_offset_extends_the_limits_of_the_third_axis() {
    let (automatic, surface_a, image_a) = lifted_image(false);
    let (manual, surface_b, image_b) = lifted_image(true);
    let (va, vb) = (
        vertices(&automatic, surface_a),
        vertices(&manual, surface_b),
    );
    assert!(!va.is_empty(), "the surface is drawn");
    assert_eq!(va.len(), vb.len());
    assert!(
        va.iter().zip(&vb).all(|(p, q)| points_close(*p, *q, 1e-6)),
        "the surface is drawn as with z limits of [0, 10]"
    );
    assert!(
        transforms_close(
            image_leaf(&automatic, image_a).transform,
            image_leaf(&manual, image_b).transform
        ),
        "the image is drawn as with z limits of [0, 10]"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Colour of a mapped image
// ---------------------------------------------------------------------------------------------------------------

// WHY: `imagesc` scales every value through the limits of its continuous colour scale: a value at the lower limit
// takes the first colourmap entry and one at the upper limit the last, exactly as `colourmap::sample` of
// `colourmap::normalise` gives, so that an image agrees with a surface or scatter coloured beside it; under transparent
// `below` and `above` policies values outside the limits are transparent, and non-finite values are transparent by
// default, so a raster with gaps or outliers still reaches the page without colours invented for them.
#[test]
fn a_mapped_image_colours_finite_values_through_the_colour_limits_and_leaves_the_rest_transparent()
{
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let values = vec![
        -1.0,
        0.0,
        5.0,
        10.0,
        11.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        8.05,
    ];
    let image = fx.imagesc(ax, vec![3, 3], values, xy(None, None), |_| {});
    let scale = fx.axes_scale_mut(ax);
    scale.limits = Limits::Manual {
        min: 0.0,
        max: 10.0,
    };
    scale.below = out_of_range(OutOfRange::Transparent);
    scale.above = out_of_range(OutOfRange::Transparent);
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let item = image_item(&scene, image);
    assert_eq!(item.channels, 4, "transparent pixels need an alpha channel");
    assert_eq!(
        pixel(&item, 0, 1),
        opaque(VIRIDIS[0]),
        "the lower limit takes the first entry"
    );
    assert_eq!(
        pixel(&item, 0, 2),
        mapped(&VIRIDIS, 5.0, 0.0, 10.0),
        "a value inside the limits takes its sample"
    );
    assert_eq!(pixel(&item, 0, 2), opaque(VIRIDIS[128]));
    assert_eq!(
        pixel(&item, 1, 0),
        opaque(VIRIDIS[255]),
        "the upper limit takes the last entry"
    );
    // 8.05 normalises to 0.805, and `sample` takes entry floor(256 · 0.805) = 206, whereas rounding 255 · 0.805
    // would take entry 205.
    assert_eq!(
        pixel(&item, 2, 2),
        mapped(&VIRIDIS, 8.05, 0.0, 10.0),
        "a value inside the limits takes the entry `sample` gives"
    );
    assert_eq!(pixel(&item, 2, 2), opaque(VIRIDIS[206]));
    assert_ne!(pixel(&item, 2, 2), opaque(VIRIDIS[205]));
    for (j, i, what) in [
        (0, 0, "below the limits"),
        (1, 1, "above the limits"),
        (1, 2, "NaN"),
        (2, 0, "+∞"),
        (2, 1, "−∞"),
    ] {
        assert_eq!(pixel(&item, j, i)[3], 0, "a value {what} is transparent");
    }
}

// WHY: automatic colour limits are the range of all the colour data of the axes, so a mapped image and a scatter
// coloured by data share one scale, as a surface and a scatter do: alone, the smallest value of the image takes the
// first entry and its largest the last; beside a scatter coloured 0 to 3, its 1 maps a third of the way along.
#[test]
fn automatic_colour_limits_of_a_mapped_image_are_the_union_with_the_other_colour_data() {
    let build = |with_scatter: bool| {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let image = fx.imagesc(ax, vec![1, 3], vec![0.0, 0.5, 1.0], xy(None, None), |_| {});
        if with_scatter {
            fx.scatter(ax, &[0.0, 1.0, 2.0], &[0.0, 0.0, 0.0], None, |s, fx| {
                s.colour_data = Some(fx.vector(&[0.0, 1.5, 3.0]));
                s.line.colour = ColourSpec::Colourmapped;
            });
        }
        let scene = compile_figure(&fx.build());
        image_item(&scene, image)
    };
    let alone = build(false);
    assert_eq!(pixel(&alone, 0, 0), opaque(VIRIDIS[0]));
    assert_eq!(pixel(&alone, 0, 1), opaque(VIRIDIS[128]));
    assert_eq!(pixel(&alone, 0, 2), opaque(VIRIDIS[255]));
    let shared = build(true);
    assert_eq!(pixel(&shared, 0, 0), opaque(VIRIDIS[0]));
    assert_eq!(
        pixel(&shared, 0, 2),
        mapped(&VIRIDIS, 1.0, 0.0, 3.0),
        "the largest value of the image is a third of the way along the shared scale"
    );
    assert_ne!(pixel(&shared, 0, 2), opaque(VIRIDIS[255]));
}

// WHY: `clamp` paints an out-of-range pixel in the nearest end colour of the colourmap, which is what `imagesc`
// does, while a non-finite value has no nearest end and stays transparent; a fixed colour paints the pixel exactly
// in that colour, alpha included, so that missing data can be shown in a colour of the user's choosing.
#[test]
fn clamp_and_fixed_colour_policies_paint_the_out_of_range_pixels_of_a_mapped_image() {
    let values = vec![-1.0, 11.0, f64::NAN, 5.0];
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let clamped = fx.imagesc(left, vec![1, 4], values.clone(), xy(None, None), |_| {});
    set_policies(
        &mut fx,
        left,
        OutOfRange::Clamp,
        OutOfRange::Clamp,
        OutOfRange::Clamp,
    );
    let right = fx.axes2d(0, 1);
    let painted = fx.imagesc(right, vec![1, 4], values, xy(None, None), |_| {});
    set_policies(
        &mut fx,
        right,
        OutOfRange::Rgba {
            colour: Colour::rgba(1.0, 0.2, 0.6, 0.4),
        },
        OutOfRange::Rgba {
            colour: Colour::rgb(0.0, 0.0, 1.0),
        },
        OutOfRange::Rgba {
            colour: Colour::rgba(0.0, 1.0, 0.0, 0.8),
        },
    );
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let item = image_item(&scene, clamped);
    assert_eq!(
        pixel(&item, 0, 0),
        opaque(VIRIDIS[0]),
        "clamped below: the first entry"
    );
    assert_eq!(
        pixel(&item, 0, 1),
        opaque(VIRIDIS[255]),
        "clamped above: the last entry"
    );
    assert_eq!(
        pixel(&item, 0, 2)[3],
        0,
        "a clamped NaN has no nearest end and is transparent"
    );
    assert_eq!(pixel(&item, 0, 3), mapped(&VIRIDIS, 5.0, 0.0, 10.0));
    assert_eq!(item.channels, 4);

    let item = image_item(&scene, painted);
    assert_eq!(
        pixel(&item, 0, 0),
        [255, 51, 153, 102],
        "a fixed colour is quantised with its alpha"
    );
    assert_eq!(pixel(&item, 0, 1), [0, 0, 255, 255]);
    assert_eq!(pixel(&item, 0, 2), [0, 255, 0, 204]);
    assert_eq!(pixel(&item, 0, 3), mapped(&VIRIDIS, 5.0, 0.0, 10.0));
    assert_eq!(item.channels, 4);
}

// WHY: a backend uploads three-channel samples as an opaque texture and four-channel ones with blending, so the
// channel count must follow what the pixels are: opaque pixels, including out-of-range ones painted in an opaque
// fixed colour or clamped, need no alpha, while one transparent pixel, from a lenient policy, a translucent fixed
// colour or a clamped NaN, needs it for the whole image. Choosing the count from the presence of out-of-range values
// rather than from their opacity would either lose transparency or waste a channel.
#[test]
fn sample_channels_follow_the_opacity_of_the_pixels() {
    let translucent_red = OutOfRange::Rgba {
        colour: Colour::rgba(1.0, 0.0, 0.0, 0.5),
    };
    let opaque_red = OutOfRange::Rgba {
        colour: Colour::rgb(1.0, 0.0, 0.0),
    };
    let cases = [
        (
            "every value in range",
            vec![1.0, 5.0],
            OutOfRange::Transparent,
            OutOfRange::Transparent,
            3,
        ),
        (
            "a value below, transparent",
            vec![-1.0, 5.0],
            OutOfRange::Transparent,
            OutOfRange::Transparent,
            4,
        ),
        (
            "a value below, an opaque fixed colour",
            vec![-1.0, 5.0],
            opaque_red,
            OutOfRange::Transparent,
            3,
        ),
        (
            "a value below, clamped",
            vec![-1.0, 5.0],
            OutOfRange::Clamp,
            OutOfRange::Transparent,
            3,
        ),
        (
            "a value below, a translucent fixed colour",
            vec![-1.0, 5.0],
            translucent_red,
            OutOfRange::Transparent,
            4,
        ),
        (
            "a NaN, clamped",
            vec![f64::NAN, 5.0],
            OutOfRange::Transparent,
            OutOfRange::Clamp,
            4,
        ),
    ];
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 3;
    let mut images = Vec::new();
    for (k, (what, values, below, non_finite, channels)) in cases.into_iter().enumerate() {
        let ax = fx.axes2d((k / 3) as u32, (k % 3) as u32);
        let image = fx.imagesc(ax, vec![1, 2], values, xy(None, None), |_| {});
        set_policies(&mut fx, ax, below, OutOfRange::Transparent, non_finite);
        images.push((what, image, channels));
    }
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    for (what, image, channels) in images {
        let item = image_item(&scene, image);
        assert_eq!(item.channels, channels, "{what}");
        assert_eq!(
            item.samples.len(),
            2 * usize::from(channels),
            "{what}: width · height · channels bytes"
        );
    }
}

// WHY: a strict policy is the user's statement that the data must be good in that category, so an offending pixel
// must not be painted over quietly: the image is skipped with one warning that names it and the category, so the
// user knows which check failed, while the rest of the figure and a strict image with nothing to offend are drawn
// as usual, because compilation never fails.
#[test]
fn a_strict_category_with_an_offending_pixel_skips_the_image_with_a_warning_naming_the_category() {
    for (category, offending) in [("below", -1.0), ("above", 11.0), ("non-finite", f64::NAN)] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
        let image = fx.imagesc(
            ax,
            vec![1, 3],
            vec![2.0, offending, 8.0],
            xy(None, None),
            |_| {},
        );
        let strict = |name: &str| {
            if name == category {
                OutOfRange::Strict
            } else {
                OutOfRange::Transparent
            }
        };
        set_policies(
            &mut fx,
            ax,
            strict("below"),
            strict("above"),
            strict("non-finite"),
        );
        let scene = compile_figure(&fx.build());
        let warning = assert_skipped_with_one_warning(
            &scene,
            image,
            &format!("mapped image with a strict {category} pixel"),
        );
        let spellings = [category.to_string(), category.replace('-', "_")];
        assert!(
            spellings.iter().any(|s| warning.contains(s.as_str())),
            "the warning names the {category} category: {warning}"
        );
        assert!(
            !from_source(&leaves(&scene), line).is_empty(),
            "the line is still drawn"
        );
    }

    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    // Strict on every category with nothing to offend: the image is drawn as if the policies were lenient.
    let clean = fx.imagesc(ax, vec![1, 3], vec![0.0, 5.0, 10.0], xy(None, None), |_| {});
    set_policies(
        &mut fx,
        ax,
        OutOfRange::Strict,
        OutOfRange::Strict,
        OutOfRange::Strict,
    );
    // An indexed image gives the policies the same meaning: an index of −1 under a strict `outside` skips it.
    let indexed = fx.indexed_image(
        ax,
        vec![1, 2],
        vec![-1.0, 3.0],
        xy(range(4.0, 5.0), None),
        |_| {},
    );
    fx.palette_mut(ax).outside = out_of_range(OutOfRange::Strict);
    let scene = compile_figure(&fx.build());
    assert_drawn_without_warning(&scene, clean, "strict image with no offending pixel");
    assert_eq!(image_item(&scene, clean).channels, 3);
    let warning = assert_skipped_with_one_warning(
        &scene,
        indexed,
        "indexed image with a strict outside pixel",
    );
    assert!(warning.contains("outside"), "{warning}");
}

// WHY: an array of bytes is a compact way to hold a mapped image, and its bytes are numbers like any other, so a
// byte is mapped through the colour limits as the value it denotes: 128 under limits of [0, 255] takes the same
// entry as the floating-point value 128 would, and under automatic limits the smallest byte takes the first entry
// and the largest the last. Treating bytes as already-scaled fractions, or as indices, would recolour the image.
#[test]
fn eight_bit_values_of_a_mapped_image_are_mapped_as_the_numbers_they_denote() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let fixed = fx.axes2d(0, 0);
    let under_manual_limits = fx.imagesc(
        fixed,
        vec![1, 3],
        vec![0u8, 128, 255],
        xy(None, None),
        |_| {},
    );
    fx.axes_scale_mut(fixed).limits = Limits::Manual {
        min: 0.0,
        max: 255.0,
    };
    let automatic = fx.axes2d(0, 1);
    let under_automatic_limits = fx.imagesc(
        automatic,
        vec![1, 3],
        vec![0u8, 100, 200],
        xy(None, None),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let item = image_item(&scene, under_manual_limits);
    assert_eq!(item.channels, 3);
    for (i, v) in [0.0, 128.0, 255.0].into_iter().enumerate() {
        assert_eq!(
            pixel(&item, 0, i),
            mapped(&VIRIDIS, v, 0.0, 255.0),
            "the byte {v} is mapped as the number it denotes"
        );
    }

    let item = image_item(&scene, under_automatic_limits);
    assert_eq!(
        pixel(&item, 0, 0),
        opaque(VIRIDIS[0]),
        "the smallest byte takes the first entry under automatic limits"
    );
    assert_eq!(pixel(&item, 0, 1), opaque(VIRIDIS[128]));
    assert_eq!(
        pixel(&item, 0, 2),
        opaque(VIRIDIS[255]),
        "the largest byte takes the last entry under automatic limits"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Colour of an indexed image
// ---------------------------------------------------------------------------------------------------------------

// WHY: an indexed image is MATLAB's `image` with an indexed array: each index names an entry of the axes colourmap
// directly, so a byte i is drawn as entry i exactly, whatever the colour limits are, because a lookup that went
// through the limits would recolour a segmentation map whenever a surface was added beside it.
#[test]
fn an_indexed_image_takes_its_entries_directly_whatever_the_colour_limits() {
    let build = |clim: Limits| {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        fx.palette_mut(ax).colours = palette_colours(&MAGMA);
        fx.axes_scale_mut(ax).limits = clim;
        let image = fx.indexed_image(
            ax,
            vec![2, 2],
            vec![0u8, 1, 128, 255],
            xy(None, None),
            |_| {},
        );
        let scene = compile_figure(&fx.build());
        assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
        image_item(&scene, image)
    };
    let automatic = build(Limits::Auto);
    assert_eq!(automatic.channels, 3, "every entry is opaque");
    for (k, index) in [0usize, 1, 128, 255].into_iter().enumerate() {
        assert_eq!(
            pixel(&automatic, k / 2, k % 2),
            opaque(MAGMA[index]),
            "index {index} takes entry {index} of the axes colourmap"
        );
    }
    let manual = build(Limits::Manual {
        min: 2.0,
        max: 18.0,
    });
    assert_eq!(
        manual.samples, automatic.samples,
        "the colour limits play no part"
    );
}

// WHY: the indices of an indexed image and the components of a true-colour image are not colour data, so they must
// not stretch the automatic colour limits of the axes: a mapped image of values 0 to 1 beside an indexed image of
// indices 0 to 255 and a true-colour image of bytes 0 to 255 still maps its 0 to the first entry and its 1 to the
// last, rather than being squeezed into the bottom of the colourmap.
#[test]
fn indexed_and_true_colour_images_contribute_nothing_to_automatic_colour_limits() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let mapped_image = fx.imagesc(ax, vec![1, 3], vec![0.0, 0.5, 1.0], xy(None, None), |_| {});
    fx.indexed_image(
        ax,
        vec![16, 16],
        ramp(256),
        xy(range(4.0, 19.0), None),
        |_| {},
    );
    fx.image(
        ax,
        vec![1, 2, 3],
        vec![0u8, 128, 255, 255, 0, 64],
        xy(range(22.0, 23.0), None),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    let item = image_item(&scene, mapped_image);
    assert_eq!(pixel(&item, 0, 0), opaque(VIRIDIS[0]));
    assert_eq!(pixel(&item, 0, 1), opaque(VIRIDIS[128]));
    assert_eq!(pixel(&item, 0, 2), opaque(VIRIDIS[255]));
}

// WHY: a floating-point index names an entry only when it is a whole number, so 2 is entry 2, while a fractional
// index (2.9, −0.5, 255.9), an index below 0 or above 255 and a non-finite index cannot be placed and follow the
// policies of the indexed scale: transparent; a clamp, which truncates towards zero and then clamps to the ends, as
// MATLAB rounds indexed arrays down, and draws nothing for a non-finite index; or a fixed colour.
#[test]
fn floating_point_indices_name_entries_when_whole_and_take_their_policy_otherwise() {
    let indices = vec![
        2.9,
        -0.5,
        255.9,
        -1.0,
        256.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        2.0,
        0.0,
    ];
    let mut fx = Fx::new();
    fx.fig.layout.cols = 3;
    let policies = |fx: &mut Fx, ax: NodeId, outside: OutOfRange, non_finite: OutOfRange| {
        fx.palette_mut(ax).outside = out_of_range(outside);
        let id = fx.palette(ax);
        fx.scale_mut(id).non_finite = out_of_range(non_finite);
    };
    let lenient = fx.axes2d(0, 0);
    let transparent =
        fx.indexed_image(lenient, vec![2, 5], indices.clone(), xy(None, None), |_| {});
    let nearest = fx.axes2d(0, 1);
    let clamped = fx.indexed_image(nearest, vec![2, 5], indices.clone(), xy(None, None), |_| {});
    policies(&mut fx, nearest, OutOfRange::Clamp, OutOfRange::Clamp);
    let fixed = fx.axes2d(0, 2);
    let painted = fx.indexed_image(fixed, vec![2, 5], indices, xy(None, None), |_| {});
    policies(
        &mut fx,
        fixed,
        OutOfRange::Rgba {
            colour: Colour::rgb(1.0, 0.0, 0.0),
        },
        OutOfRange::Rgba {
            colour: Colour::rgba(0.0, 0.0, 1.0, 0.4),
        },
    );
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    for id in [transparent, clamped, painted] {
        let item = image_item(&scene, id);
        assert_eq!(item.channels, 4);
        assert_eq!(pixel(&item, 1, 3), opaque(VIRIDIS[2]), "2 is entry 2");
        assert_eq!(pixel(&item, 1, 4), opaque(VIRIDIS[0]), "0 is entry 0");
    }

    let item = image_item(&scene, transparent);
    for (j, i, what) in [
        (0, 0, "2.9"),
        (0, 1, "−0.5"),
        (0, 2, "255.9"),
        (0, 3, "−1"),
        (0, 4, "256"),
        (1, 0, "NaN"),
        (1, 1, "+∞"),
        (1, 2, "−∞"),
    ] {
        assert_eq!(pixel(&item, j, i)[3], 0, "the index {what} is transparent");
    }

    let item = image_item(&scene, clamped);
    assert_eq!(
        pixel(&item, 0, 0),
        opaque(VIRIDIS[2]),
        "2.9 truncates to entry 2"
    );
    assert_eq!(
        pixel(&item, 0, 1),
        opaque(VIRIDIS[0]),
        "−0.5 truncates to entry 0"
    );
    assert_eq!(
        pixel(&item, 0, 2),
        opaque(VIRIDIS[255]),
        "255.9 truncates to entry 255"
    );
    assert_eq!(
        pixel(&item, 0, 3),
        opaque(VIRIDIS[0]),
        "−1 clamps to the first entry"
    );
    assert_eq!(
        pixel(&item, 0, 4),
        opaque(VIRIDIS[255]),
        "256 clamps to the last entry"
    );
    for i in 0..3 {
        assert_eq!(
            pixel(&item, 1, i)[3],
            0,
            "a clamped non-finite index is transparent"
        );
    }

    let item = image_item(&scene, painted);
    for i in 0..5 {
        assert_eq!(
            pixel(&item, 0, i),
            [255, 0, 0, 255],
            "an index that names no entry takes the fixed colour"
        );
    }
    for i in 0..3 {
        assert_eq!(pixel(&item, 1, i), [0, 0, 255, 102]);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Colour of a true-colour image
// ---------------------------------------------------------------------------------------------------------------

// WHY: true-colour pixels reach the backends as 8-bit sRGB, so floating-point components from 0 to 1 are quantised
// by rounding (0.5 is 128, not the 127 of a truncation) after clamping the odd component that strays outside the
// range, while 8-bit components are the bytes the user supplied and are copied untouched; an image of opaque pixels
// carries three channels.
#[test]
fn true_colour_components_are_clamped_and_quantised_or_copied() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let floats = fx.axes2d(0, 0);
    let quantised = fx.image(
        floats,
        vec![1, 3, 3],
        vec![1.2, -0.3, 0.5, 0.2, 0.6, 1.0, 0.0, 1.0, 0.0],
        xy(None, None),
        |_| {},
    );
    let bytes = fx.axes2d(0, 1);
    let copied = fx.image(
        bytes,
        vec![1, 2, 3],
        vec![7u8, 8, 9, 250, 0, 1],
        xy(None, None),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let item = image_item(&scene, quantised);
    assert_eq!(item.channels, 3);
    assert_eq!(
        pixel(&item, 0, 0),
        [255, 0, 128, 255],
        "components are clamped to 0..1 and rounded to eight bits"
    );
    assert_eq!(pixel(&item, 0, 1), [51, 153, 255, 255]);
    assert_eq!(pixel(&item, 0, 2), [0, 255, 0, 255]);

    let item = image_item(&scene, copied);
    assert_eq!(item.channels, 3);
    assert_eq!(pixel(&item, 0, 0), [7, 8, 9, 255], "bytes are copied");
    assert_eq!(pixel(&item, 0, 1), [250, 0, 1, 255]);
}

// WHY: the fourth component of a pixel is its straight alpha, and a pixel with a non-finite component has no colour
// and is transparent, so either case needs four-channel samples for the whole image; RGBA data whose alphas are all
// 1 is opaque and needs only three, since the channel count follows the pixels and not the shape of the array.
#[test]
fn alpha_and_non_finite_components_make_pixels_transparent_and_add_an_alpha_channel() {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    let a = fx.axes2d(0, 0);
    let translucent = fx.image(
        a,
        vec![1, 2, 4],
        vec![1.0, 0.0, 0.0, 0.4, 0.0, 1.0, 0.0, 1.0],
        xy(None, None),
        |_| {},
    );
    let b = fx.axes2d(0, 1);
    let holed = fx.image(
        b,
        vec![1, 2, 3],
        vec![f64::NAN, 1.0, 1.0, 0.0, 0.0, 1.0],
        xy(None, None),
        |_| {},
    );
    let c = fx.axes2d(1, 0);
    let fully_opaque = fx.image(
        c,
        vec![1, 2, 4],
        vec![10u8, 20, 30, 255, 40, 50, 60, 255],
        xy(None, None),
        |_| {},
    );
    let d = fx.axes2d(1, 1);
    let byte_alpha = fx.image(
        d,
        vec![1, 2, 4],
        vec![10u8, 20, 30, 254, 40, 50, 60, 255],
        xy(None, None),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let item = image_item(&scene, translucent);
    assert_eq!(item.channels, 4);
    assert_eq!(
        pixel(&item, 0, 0),
        [255, 0, 0, 102],
        "the fourth component is the alpha, quantised"
    );
    assert_eq!(pixel(&item, 0, 1), [0, 255, 0, 255]);

    let item = image_item(&scene, holed);
    assert_eq!(
        item.channels, 4,
        "a transparent pixel in RGB data needs an alpha channel"
    );
    assert_eq!(
        pixel(&item, 0, 0)[3],
        0,
        "a pixel with a non-finite component is transparent"
    );
    assert_eq!(pixel(&item, 0, 1), [0, 0, 255, 255]);

    let item = image_item(&scene, fully_opaque);
    assert_eq!(
        item.channels, 3,
        "RGBA data whose pixels are all opaque needs no alpha channel"
    );
    assert_eq!(pixel(&item, 0, 1), [40, 50, 60, 255]);

    let item = image_item(&scene, byte_alpha);
    assert_eq!(item.channels, 4);
    assert_eq!(
        pixel(&item, 0, 0),
        [10, 20, 30, 254],
        "an 8-bit alpha is copied"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Logarithmic axes
// ---------------------------------------------------------------------------------------------------------------

/// Returns the lattice of an image item, which must be drawn through a mesh.
#[track_caller]
fn mesh_of(item: &ImageItem) -> &ImageMesh {
    match &item.geometry {
        ImageGeometry::Mesh(mesh) => mesh,
        ImageGeometry::Rect(rect) => {
            panic!("the image is drawn through a mesh, not stretched over {rect:?}")
        }
    }
}

/// The figure-space x of a data value on a logarithmic x axis with limits `min` to `max`, from the definition of such
/// an axis (equal ratios are equal distances, the limits at the sides of the plot rectangle), not from the compiler's
/// axis map.
fn log_x(plot: Rect, min: f64, max: f64, value: f64) -> f64 {
    plot.x + (value.log10() - min.log10()) / (max.log10() - min.log10()) * plot.width
}

/// The figure-space y of a data value on a logarithmic y axis with limits `min` to `max`; figure space runs down the
/// page, so the lower limit is at the bottom of the plot rectangle.
fn log_y(plot: Rect, min: f64, max: f64, value: f64) -> f64 {
    plot.bottom() - (value.log10() - min.log10()) / (max.log10() - min.log10()) * plot.height
}

/// The figure-space y of a data value on a linear y axis with limits `min` to `max`.
fn linear_y(plot: Rect, min: f64, max: f64, value: f64) -> f64 {
    plot.bottom() - (value - min) / (max - min) * plot.height
}

/// Sets manual limits on the x and y axes of an axes.
fn manual_xy_limits(fx: &mut Fx, ax: NodeId, x: (f64, f64), y: (f64, f64)) {
    let axes = fx.ax(ax);
    axes.x.limits = Limits::Manual { min: x.0, max: x.1 };
    axes.y.limits = Limits::Manual { min: y.0, max: y.1 };
}

/// Asserts that every node of a lattice is drawn, through the transform of its leaf, at `expected(u, v)`.
#[track_caller]
fn assert_nodes_at(leaf: &Leaf, mesh: &ImageMesh, expected: impl Fn(f64, f64) -> Point) {
    assert_eq!(
        mesh.points.len(),
        mesh.u.len() * mesh.v.len(),
        "one point per node of the lattice"
    );
    for (j, v) in mesh.v.iter().enumerate() {
        for (i, u) in mesh.u.iter().enumerate() {
            let drawn = leaf.transform.apply(mesh.points[j * mesh.u.len() + i]);
            let want = expected(*u, *v);
            assert!(
                points_close(drawn, want, 1e-6),
                "the node at pixel-space ({u}, {v}) is drawn at {drawn:?}, expected {want:?}"
            );
        }
    }
}

// WHY: on a logarithmic axis equal pitches in the data are unequal distances on the page, so an image there is drawn
// through a lattice whose nodes are its pixel edges mapped through the axis: every pixel then lies exactly between
// its own edges, which is what the reader reads the axis against. Here three columns centred on 10, 20 and 30 have
// edges at 5, 15, 25 and 35, which a logarithmic axis draws ever closer together; the linear y axis needs only the
// two outer row edges, because the map is affine along it. The samples are the user's pixels in row order, as for a
// rectangle, and the image is clipped to the plot as every other artist of the axes is.
#[test]
fn an_image_on_a_logarithmic_axis_is_drawn_through_a_lattice_of_its_pixel_edges() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let line = fx.line(ax, &[1.0, 1000.0], &[0.0, 4.0], None, |_| {});
    let (ny, nx) = (2, 3);
    let image = fx.image(
        ax,
        vec![ny, nx, 3],
        tagged_pixels(ny, nx),
        xy(range(10.0, 30.0), range(1.0, 3.0)),
        |_| {},
    );
    fx.ax(ax).x.scale = Scale::Log;
    manual_xy_limits(&mut fx, ax, (1.0, 1000.0), (0.0, 4.0));
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let leaf = image_leaf(&scene, image);
    let item = leaf.image().unwrap();
    let mesh = mesh_of(item);
    assert_eq!(
        mesh.u,
        vec![0.0, 1.0, 2.0, 3.0],
        "a node at every column edge along the logarithmic axis"
    );
    assert_eq!(
        mesh.v,
        vec![0.0, 2.0],
        "only the outer row edges along the linear axis"
    );
    assert!(
        mesh.depths.is_none() && item.depth.is_none(),
        "an image in a 2D axes carries no depth"
    );
    let plot = axes_hit(&scene, ax).plot_rect;
    assert_nodes_at(&leaf, mesh, |u, v| {
        Point::new(
            log_x(plot, 1.0, 1000.0, 5.0 + 10.0 * u),
            linear_y(plot, 0.0, 4.0, 2.0 * v),
        )
    });
    assert_eq!((item.width, item.height), (3, 2));
    for j in 0..ny {
        for i in 0..nx {
            assert_eq!(pixel(item, j, i), opaque(tag(j, i)), "pixel ({j}, {i})");
        }
    }
    let lines = from_source(&leaves(&scene), line);
    assert!(leaf.clip.is_some(), "the image is clipped");
    assert_eq!(
        leaf.clip, lines[0].clip,
        "clipped as the line in the same axes"
    );
}

// WHY: a backwards pixel range is how a user flips an image, and it must flip on logarithmic axes as on linear ones:
// pixel space still starts at the first pixel edge, so the lattice runs from the first edge to the last across the
// page in the opposite direction, while the samples stay in the row order of the user's array. On log-log axes both
// directions are lattices of pixel edges. Reordering the samples instead would show the right picture here but
// break every consumer that indexes pixels by row and column, such as datatips.
#[test]
fn a_mirrored_image_on_log_log_axes_runs_its_lattice_backwards_and_keeps_its_samples() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let (ny, nx) = (2, 3);
    // Columns centred on 30, 20 and 10 (edges 35, 25, 15 and 5); rows centred on 4 and 2 (edges 5, 3 and 1).
    let image = fx.image(
        ax,
        vec![ny, nx, 3],
        tagged_pixels(ny, nx),
        xy(range(30.0, 10.0), range(4.0, 2.0)),
        |_| {},
    );
    fx.ax(ax).x.scale = Scale::Log;
    fx.ax(ax).y.scale = Scale::Log;
    manual_xy_limits(&mut fx, ax, (1.0, 100.0), (0.5, 50.0));
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let leaf = image_leaf(&scene, image);
    let item = leaf.image().unwrap();
    let mesh = mesh_of(item);
    assert_eq!(mesh.u, vec![0.0, 1.0, 2.0, 3.0]);
    assert_eq!(mesh.v, vec![0.0, 1.0, 2.0]);
    let plot = axes_hit(&scene, ax).plot_rect;
    assert_nodes_at(&leaf, mesh, |u, v| {
        Point::new(
            log_x(plot, 1.0, 100.0, 35.0 - 10.0 * u),
            log_y(plot, 0.5, 50.0, 5.0 - 2.0 * v),
        )
    });
    for j in 0..ny {
        for i in 0..nx {
            assert_eq!(
                pixel(item, j, i),
                opaque(tag(j, i)),
                "pixel ({j}, {i}) keeps its place in the samples"
            );
        }
    }
}

// WHY: a lattice with a node at every pixel edge would grow with the image, and an image of thousands of columns on a
// logarithmic axis would become thousands of nodes squeezed into fractions of a point, which no reader can see and
// every backend must still draw. Edges closer than one point on the page are therefore dropped, keeping the first
// and last, so that the node count is bounded by the page. The test pins every half of that rule on 2000 columns
// whose edges 0.5, 1.5, ..., 2000.5 span four decades of a five-decade axis: no lattice cell is shorter than one
// point; the kept nodes are pixel edges, placed exactly; edges more than a point from both neighbours, at the
// low end, are all kept; and no cell that merges pixels is longer than the rule needs (all the merged pixels are
// narrower than a point, so a merged cell is shorter than three points even where the last edge is forced in). A
// reversed axis draws the edges leftwards, so the rule must measure distances on the page whatever their sign; a
// rule that compared signed differences would keep every edge, or none, there.
#[test]
fn a_lattice_drops_pixel_edges_closer_than_a_point_and_keeps_the_first_and_last() {
    let n = 2000;
    for reverse in [false, true] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let image = fx.imagesc(
            ax,
            vec![1, n],
            ramp(n),
            xy(range(1.0, n as f64), range(0.0, 0.0)),
            |_| {},
        );
        fx.ax(ax).x.scale = Scale::Log;
        fx.ax(ax).x.reverse = reverse;
        manual_xy_limits(&mut fx, ax, (0.1, 10000.0), (-1.0, 1.0));
        let scene = compile_figure(&fx.build());
        assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

        let at = if reverse { "reversed" } else { "forwards" };
        let leaf = image_leaf(&scene, image);
        let mesh = mesh_of(leaf.image().unwrap());
        let plot = axes_hit(&scene, ax).plot_rect;
        let page = |edge: f64| {
            let x = log_x(plot, 0.1, 10000.0, 0.5 + edge);
            if reverse {
                2.0 * plot.x + plot.width - x
            } else {
                x
            }
        };
        let apart = |a: f64, b: f64| (page(b) - page(a)).abs();
        assert_eq!(mesh.u.first(), Some(&0.0), "{at}: the first edge is kept");
        assert_eq!(
            mesh.u.last(),
            Some(&(n as f64)),
            "{at}: the last edge is kept"
        );
        assert!(
            mesh.u.iter().all(|u| u.fract() == 0.0),
            "{at}: every node is a pixel edge"
        );
        assert!(
            mesh.u.windows(2).all(|w| w[0] < w[1]),
            "{at}: the lattice increases"
        );
        assert!(
            mesh.u.len() < n / 2,
            "{at}: most of the {n} edges are dropped: {} kept",
            mesh.u.len()
        );
        assert!(
            (mesh.u.len() as f64) <= plot.width + 2.0,
            "{at}: the node count is bounded by the width of the plot in points"
        );
        for w in mesh.u.windows(2) {
            let length = apart(w[0], w[1]);
            assert!(
                length >= 1.0 - 1e-9,
                "{at}: the cell from edge {} to edge {} is {length} pt long, shorter than a point",
                w[0],
                w[1]
            );
            if w[1] - w[0] > 1.0 {
                assert!(
                    length < 3.0,
                    "{at}: the cell from edge {} to edge {} merges pixels over {length} pt, more than the rule \
                     needs",
                    w[0],
                    w[1]
                );
            }
        }
        for k in 1..n {
            let edge = k as f64;
            if apart(edge - 1.0, edge) >= 1.0 && apart(edge, edge + 1.0) >= 1.0 {
                assert!(
                    mesh.u.contains(&edge),
                    "{at}: edge {k} lies more than a point from both neighbours and is kept"
                );
            }
        }
        assert!(
            (0..=5).all(|k| mesh.u.contains(&(k as f64))),
            "{at}: the widely spaced low edges are kept: {:?}",
            &mesh.u[..mesh.u.len().min(8)]
        );
        assert_nodes_at(&leaf, mesh, |u, v| {
            Point::new(page(u), linear_y(plot, -1.0, 1.0, v - 0.5))
        });
    }
}

// WHY: the smallest lattices are where an off-by-one in the edges or the thinning shows. A single pixel has no pitch
// to take from its range, so it is one unit wide about its centre, and on log-log axes its lattice is exactly its
// four corners at the mapped edges 9.5 and 10.5. An image whose whole width is narrower than a point cannot satisfy
// the one-point rule at all; it keeps its first and last edge, so that it is still drawn, as a single cell, rather
// than vanishing or being refused.
#[test]
fn a_single_pixel_and_an_image_narrower_than_a_point_keep_exactly_their_outer_edges() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let single = fx.imagesc(
        ax,
        vec![1, 1],
        vec![1.0],
        xy(range(10.0, 10.0), range(10.0, 10.0)),
        |_| {},
    );
    // Fifty columns centred on 1000 to 1000.49: edges from 999.995 to 1000.495, some 0.01 pt in all.
    let narrow = fx.imagesc(
        ax,
        vec![1, 50],
        ramp(50),
        xy(range(1000.0, 1000.49), range(10.0, 10.0)),
        |_| {},
    );
    fx.ax(ax).x.scale = Scale::Log;
    fx.ax(ax).y.scale = Scale::Log;
    manual_xy_limits(&mut fx, ax, (1.0, 10000.0), (1.0, 100.0));
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let plot = axes_hit(&scene, ax).plot_rect;

    let leaf = image_leaf(&scene, single);
    let mesh = mesh_of(leaf.image().unwrap());
    assert_eq!(
        (mesh.u.clone(), mesh.v.clone()),
        (vec![0.0, 1.0], vec![0.0, 1.0])
    );
    assert_nodes_at(&leaf, mesh, |u, v| {
        Point::new(
            log_x(plot, 1.0, 10000.0, 9.5 + u),
            log_y(plot, 1.0, 100.0, 9.5 + v),
        )
    });

    let leaf = image_leaf(&scene, narrow);
    let mesh = mesh_of(leaf.image().unwrap());
    assert_eq!(
        mesh.u,
        vec![0.0, 50.0],
        "an image narrower than a point keeps only its outer edges"
    );
    assert!(leaf.image().unwrap().is_valid(), "and is still drawable");
}

// WHY: a logarithmic axis has no place for zero or a negative coordinate, so an image whose first or last pixel edge
// along a logarithmic axis of its plane is not positive cannot be placed. Validation refuses such a figure, but the
// compiler draws whatever it is given and never fails, so it must leave the image out with one warning naming it,
// draw the other artists of the axes, and let the skipped image contribute nothing to the limits, as any skipped
// artist does. The default placement centres pixels on 0, 1, ..., so its first edge is at -0.5. Only the axes of the
// plane impose this: the third axis, which only sets the offset, may be logarithmic with the image drawn as an
// ordinary rectangle, exactly as before images could lie on logarithmic axes, and so may the z axis of a 2D axes.
#[test]
fn an_image_with_a_pixel_edge_off_a_logarithmic_axis_is_left_out_with_a_warning() {
    // A 2D axes with a logarithmic x axis holding a line over [1, 100] × [1, 2] and, when asked, an image at the
    // default columns (edges -0.5 and 1.5, so the first has no place on the axis) whose rows are centred on 1 and 2
    // (edges 0.5 and 2.5, which would show as tight y limits against the line's [1, 2] if a skipped image still
    // counted).
    let flat_axes = |fx: &mut Fx, with_image: bool| -> (NodeId, NodeId, Option<NodeId>) {
        let flat = fx.axes2d(0, 0);
        let line = fx.line(flat, &[1.0, 100.0], &[1.0, 2.0], None, |_| {});
        let image = with_image
            .then(|| fx.imagesc(flat, vec![2, 2], ramp(4), xy(None, range(1.0, 2.0)), |_| {}));
        fx.ax(flat).x.scale = Scale::Log;
        (flat, line, image)
    };

    let mut fx = Fx::new();
    fx.fig.layout.cols = 4;
    let (flat, line, on_log_x) = flat_axes(&mut fx, true);
    let on_log_x = on_log_x.expect("the image is added");

    let log_z = fx.axes3d(0, 1, View3d::default());
    fx.line(log_z, &[0.0, 1.0], &[0.0, 1.0], Some(&[1.0, 100.0]), |_| {});
    // Rows along the logarithmic z axis at the default centres: edges -0.5 and 1.5.
    let wall_on_log_z = fx.imagesc(
        log_z,
        vec![2, 2],
        ramp(4),
        placement(ImagePlane::Xz { y: None }, None, None),
        |_| {},
    );
    let floor_under_log_z = fx.imagesc(
        log_z,
        vec![2, 2],
        ramp(4),
        placement(ImagePlane::Xy { z: Some(10.0) }, None, None),
        |_| {},
    );
    fx.ax(log_z).z.scale = Scale::Log;

    let log_y = fx.axes3d(0, 2, View3d::default());
    fx.line(log_y, &[0.0, 1.0], &[1.0, 100.0], Some(&[0.0, 1.0]), |_| {});
    let wall_beside_log_y = fx.imagesc(
        log_y,
        vec![2, 2],
        ramp(4),
        placement(ImagePlane::Xz { y: Some(10.0) }, None, None),
        |_| {},
    );
    fx.ax(log_y).y.scale = Scale::Log;

    let flat_log_z = fx.axes2d(0, 3);
    let under_unused_z = fx.imagesc(flat_log_z, vec![2, 2], ramp(4), xy(None, None), |_| {});
    fx.ax(flat_log_z).z.scale = Scale::Log;

    let scene = compile_figure(&fx.build());
    let warning =
        assert_skipped_with_one_warning(&scene, on_log_x, "image on a logarithmic x axis");
    assert!(
        warning.contains("logarithmic"),
        "the warning says why: {warning}"
    );
    assert_skipped_with_one_warning(&scene, wall_on_log_z, "xz image on a logarithmic z axis");
    for (id, what) in [
        (
            floor_under_log_z,
            "xy image whose third axis is logarithmic",
        ),
        (
            wall_beside_log_y,
            "xz image whose third axis is logarithmic",
        ),
        (under_unused_z, "2D image under a logarithmic z axis"),
    ] {
        assert_drawn_without_warning(&scene, id, what);
        assert!(
            matches!(image_item(&scene, id).geometry, ImageGeometry::Rect(_)),
            "the {what} has no logarithmic axis in its plane and is drawn as a rectangle"
        );
    }
    assert!(
        !from_source(&leaves(&scene), line).is_empty(),
        "the line beside the skipped image is still drawn"
    );

    // The same axes without the skipped image takes the same limits: the line's alone.
    let mut alone = Fx::new();
    alone.fig.layout.cols = 4;
    let (flat_alone, _, _) = flat_axes(&mut alone, false);
    let alone = compile_figure(&alone.build());
    let ((x, y), (xa, ya)) = (axis_maps(&scene, flat), axis_maps(&alone, flat_alone));
    assert_eq!(
        (x.min, x.max, y.min, y.max),
        (xa.min, xa.max, ya.min, ya.max),
        "the skipped image contributes nothing to the limits"
    );
}

// WHY: a datatip over an image on a logarithmic axis must report the pixel the reader sees under the pointer, and the
// reader sees each pixel between its edges mapped through the axis. The hit map therefore inverts the axes rather
// than an affine placement: a point maps to data through the axis maps, then linearly in the data to pixel space.
// Three columns with edges at 5, 15, 25 and 35 tell the two apart: x = 14 lies in column 0 and x = 16 in column 1,
// but on the page x = 14 is past the middle of the image (log 14/5 over log 7 is 0.53), so a page-linear inverse
// would put it in column 1. Points beyond the outer edges lie over nothing.
#[test]
fn the_hit_map_resolves_the_pixel_under_a_point_through_a_logarithmic_axis() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let image = fx.imagesc(
        ax,
        vec![2, 3],
        ramp(6),
        xy(range(10.0, 30.0), range(1.0, 3.0)),
        |_| {},
    );
    fx.ax(ax).x.scale = Scale::Log;
    manual_xy_limits(&mut fx, ax, (1.0, 1000.0), (-1.0, 5.0));
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let plot = axes_hit(&scene, ax).plot_rect;
    let at = |x: f64, y: f64| Point::new(log_x(plot, 1.0, 1000.0, x), linear_y(plot, -1.0, 5.0, y));
    assert_eq!(
        scene.hit_map.images.len(),
        1,
        "the drawn image has an entry"
    );
    let hit = image_hit(&scene, image);
    assert_eq!((hit.axes, hit.columns, hit.rows), (ax, 3, 2));
    assert!(
        matches!(hit.to_pixel, PixelMap::Axes { .. }),
        "an image drawn through the axes is inverted through them: {:?}",
        hit.to_pixel
    );
    for (x, column) in [
        (6.0, 0),
        (14.0, 0),
        (16.0, 1),
        (24.0, 1),
        (26.0, 2),
        (34.0, 2),
    ] {
        for (y, row) in [(0.5, 0), (1.9, 0), (2.1, 1), (3.5, 1)] {
            let (found, r, c) = scene
                .hit_map
                .pixel_at(at(x, y))
                .unwrap_or_else(|| panic!("({x}, {y}) lies over the image"));
            assert_eq!(
                (found.artist, r, c),
                (image, row, column),
                "the data point ({x}, {y})"
            );
        }
    }
    for (x, y) in [(4.0, 1.0), (36.0, 1.0), (20.0, -0.5), (20.0, 4.5)] {
        assert!(
            scene.hit_map.pixel_at(at(x, y)).is_none(),
            "({x}, {y}) lies beyond the pixel edges"
        );
    }
    for ((x, y), expected) in [
        ((5.0, 0.0), Point::new(0.0, 0.0)),
        ((20.0, 1.0), Point::new(1.5, 0.5)),
        ((35.0, 4.0), Point::new(3.0, 2.0)),
    ] {
        let found = hit.to_pixel.to_pixel(at(x, y));
        assert!(
            points_close(found, expected, 1e-6),
            "the data point ({x}, {y}) maps to pixel space at {found:?}, expected {expected:?}"
        );
        // The forward map, which the canvas uses to outline the pixel under a datatip, takes pixel space back to the
        // page through the axis, so that the outline is drawn between the pixel's mapped edges.
        let back = hit
            .to_pixel
            .from_pixel(expected)
            .unwrap_or_else(|| panic!("pixel space at {expected:?} has a place on the page"));
        assert!(
            points_close(back, at(x, y), 1e-6),
            "pixel space at {expected:?} maps forward to {back:?}, expected {:?}",
            at(x, y)
        );
    }
}

// WHY: a datatip must also be right for a flipped image on flipped axes, the combination in which a hit map that
// assumed increasing edges or an unreversed axis would report the mirror-image pixel. Columns centred on 30, 20 and
// 10 (edges 35 to 5) along a reversed logarithmic x axis and rows centred on 4 and 2 (edges 5 to 1) along a
// logarithmic y axis: column 0, the data from 25 to 35, must be found wherever the data lie there, whichever side
// of the page that is, and the forward map must invert the backward one.
#[test]
fn the_hit_map_resolves_a_mirrored_image_on_reversed_log_log_axes_by_its_data() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let image = fx.imagesc(
        ax,
        vec![2, 3],
        ramp(6),
        xy(range(30.0, 10.0), range(4.0, 2.0)),
        |_| {},
    );
    fx.ax(ax).x.scale = Scale::Log;
    fx.ax(ax).x.reverse = true;
    fx.ax(ax).y.scale = Scale::Log;
    manual_xy_limits(&mut fx, ax, (1.0, 100.0), (0.5, 50.0));
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let plot = axes_hit(&scene, ax).plot_rect;
    let at = |x: f64, y: f64| {
        Point::new(
            2.0 * plot.x + plot.width - log_x(plot, 1.0, 100.0, x),
            log_y(plot, 0.5, 50.0, y),
        )
    };
    for (x, column) in [
        (34.0, 0),
        (26.0, 0),
        (24.0, 1),
        (16.0, 1),
        (14.0, 2),
        (6.0, 2),
    ] {
        for (y, row) in [(4.9, 0), (3.1, 0), (2.9, 1), (1.1, 1)] {
            let (found, r, c) = scene
                .hit_map
                .pixel_at(at(x, y))
                .unwrap_or_else(|| panic!("({x}, {y}) lies over the image"));
            assert_eq!(
                (found.artist, r, c),
                (image, row, column),
                "the data point ({x}, {y})"
            );
        }
    }
    for (x, y) in [(4.0, 3.0), (36.0, 3.0), (20.0, 0.9), (20.0, 5.1)] {
        assert!(
            scene.hit_map.pixel_at(at(x, y)).is_none(),
            "({x}, {y}) lies beyond the pixel edges"
        );
    }
    let hit = image_hit(&scene, image);
    for local in [
        Point::new(0.0, 0.0),
        Point::new(1.5, 0.5),
        Point::new(3.0, 2.0),
    ] {
        let page = hit
            .to_pixel
            .from_pixel(local)
            .unwrap_or_else(|| panic!("pixel space at {local:?} has a place on the page"));
        let back = hit.to_pixel.to_pixel(page);
        assert!(
            points_close(back, local, 1e-6),
            "pixel space at {local:?} goes forward to {page:?} and back to {back:?}"
        );
    }
    let corner = hit.to_pixel.from_pixel(Point::new(0.0, 0.0)).unwrap();
    assert!(
        points_close(corner, at(35.0, 5.0), 1e-6),
        "the first corner of pixel space is drawn at the first edges, 35 and 5: {corner:?}"
    );
}

/// The projection of a 3D axes at the default view with limits `lo` to `hi` and the given logarithmic axes, rebuilt
/// from the public camera and the documented fit, giving the figure-space position and the depth of a data point.
fn log_projection(
    scene: &Scene,
    ax: NodeId,
    lo: [f64; 3],
    hi: [f64; 3],
    log: [bool; 3],
) -> impl Fn([f64; 3]) -> (Point, f64) {
    let plot = axes_hit(scene, ax).plot_rect;
    let (scale, offset) = fit_to_rect(plot.width, plot.height);
    let centre = Point::new(plot.x + offset[0], plot.y + offset[1]);
    move |p: [f64; 3]| {
        let projected = Camera::default().project(normalise_box(p, lo, hi, log));
        (
            Point::new(
                centre.x + scale * projected.screen[0],
                centre.y - scale * projected.screen[1],
            ),
            projected.depth,
        )
    }
}

// WHY: in a 3D axes an image on a logarithmic axis is drawn through the same lattice of pixel edges, each node
// projected by the camera from its data position, and a backend depth-tests every pixel from depths interpolated
// between the nodes as the points are, so each node must carry the projected depth of its own data point (pushed
// back by the face bias for an image inside the box, like a face's fill, so that lines on it show). A plane over
// pixel space cannot describe these depths, because pixel space no longer maps affinely to the page. 3D images keep
// no hit geometry.
#[test]
fn a_three_dimensional_image_on_a_logarithmic_axis_carries_a_projected_depth_at_every_node() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    // Columns centred on 2, 4, 6 and 8 (edges 1, 3, 5, 7 and 9) along the logarithmic x axis, rows centred on -0.5
    // and 0.5 (edges -1 and 1) along y, at z = 0.5 inside the box.
    let image = fx.imagesc(
        ax,
        vec![2, 4],
        ramp(8),
        placement(
            ImagePlane::Xy { z: Some(0.5) },
            range(2.0, 8.0),
            range(-0.5, 0.5),
        ),
        |_| {},
    );
    manual_unit_limits(&mut fx, ax);
    fx.ax(ax).x.scale = Scale::Log;
    fx.ax(ax).x.limits = Limits::Manual {
        min: 1.0,
        max: 100.0,
    };
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let leaf = image_leaf(&scene, image);
    let item = leaf.image().unwrap();
    let mesh = mesh_of(item);
    assert_eq!(mesh.u, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    assert_eq!(mesh.v, vec![0.0, 2.0]);
    assert!(
        item.depth.is_none(),
        "a mesh carries its depths at its nodes, not as a plane"
    );
    let depths = mesh
        .depths
        .as_ref()
        .expect("an image inside a depth group carries node depths");
    assert_eq!(depths.len(), mesh.points.len(), "one depth per node");
    let project = log_projection(
        &scene,
        ax,
        [1.0, -1.0, -1.0],
        [100.0, 1.0, 1.0],
        [true, false, false],
    );
    let data = |u: f64, v: f64| [1.0 + 2.0 * u, -1.0 + v, 0.5];
    assert_nodes_at(&leaf, mesh, |u, v| project(data(u, v)).0);
    for (j, v) in mesh.v.iter().enumerate() {
        for (i, u) in mesh.u.iter().enumerate() {
            assert_close(
                depths[j * mesh.u.len() + i],
                project(data(*u, *v)).1 - FACE_DEPTH_BIAS,
                1e-9,
            );
        }
    }
    assert!(
        scene.hit_map.images.is_empty(),
        "a 3D image records no hit geometry"
    );
}

// WHY: drawing an image through a mesh must not change where the depth sort places it. A floor image lies on a back
// face of the box, so it must be painted before every face of a surface above it, whatever the depths of its nodes;
// a key taken from its nodes or corners would interleave it with the faces of the surface behind its centre. The
// surface's grid is the straddling grid of the linear tests carried onto the logarithmic axis, so that its faces lie
// on both sides of the mean depth of the floor in exactly the same way, and only the face rule orders them all.
#[test]
fn a_floor_image_on_a_logarithmic_axis_is_still_painted_before_every_face_above_it() {
    let camera = Camera::default();
    let plane = ImagePlane::Xy { z: None };
    assert_faces_straddle(
        camera,
        0.0,
        image_mean_depth(camera, plane, -1.0, (-1.0, 1.0), (-1.0, 1.0)),
    );
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    // Two columns with edges at 1, 50.5 and 100, the limits of the logarithmic x axis, and two rows with edges at
    // -1 and 1, so that the floor covers the whole face of the box.
    let image = fx.imagesc(
        ax,
        vec![2, 2],
        ramp(4),
        placement(plane, range(25.75, 75.25), range(-0.5, 0.5)),
        |_| {},
    );
    // A normalised coordinate n of the box lies at x = 10^(2 (n + 0.5)) on an axis from 1 to 100.
    let xs: Vec<f64> = straddling_grid()
        .iter()
        .map(|n| 10f64.powf(2.0 * (n + 0.5)))
        .collect();
    let surface = fx.surface(ax, &xs, &straddling_grid(), |_, _| 0.0, |_| {});
    manual_unit_limits(&mut fx, ax);
    fx.ax(ax).x.scale = Scale::Log;
    fx.ax(ax).x.limits = Limits::Manual {
        min: 1.0,
        max: 100.0,
    };
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);

    let mesh = mesh_of(&image_item(&scene, image)).clone();
    assert_eq!(
        mesh.depths.as_ref().map(Vec::len),
        Some(mesh.points.len()),
        "one depth per node"
    );
    assert_image_painted(
        &scene,
        image,
        surface,
        true,
        "floor on a logarithmic x axis",
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Hit map
// ---------------------------------------------------------------------------------------------------------------

// WHY: a datatip over an image must report the row and column of the pixel under the pointer, and through them the
// value the user's array holds there, so the hit map records every drawn image of a 2D axes with the inverse of its
// placement, `to_pixel`, which maps figure space back into pixel space. `pixel_at` floors the pixel coordinates, so
// the image is the half-open extent [0, nx) × [0, ny) and a boundary shared by two pixels belongs to the higher
// index: the centre of pixel (j, i) resolves to (j, i), a point just inside the low corner to (0, 0), and a point
// just beyond the high corner, like one a quarter of a pixel outside the raster, to nothing.
#[test]
fn the_hit_map_records_a_drawn_two_dimensional_image_and_resolves_the_pixel_under_a_point() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 5.0], &[0.0, 30.0], None, |_| {});
    let (ny, nx) = (2, 3);
    let image = fx.image(
        ax,
        vec![ny, nx, 3],
        tagged_pixels(ny, nx),
        xy(range(1.0, 3.0), range(10.0, 20.0)),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    assert_eq!(scene.hit_map.images.len(), 1, "one entry per drawn image");
    let hit = image_hit(&scene, image);
    assert_eq!((hit.axes, hit.columns, hit.rows), (ax, nx, ny));
    let leaf = image_leaf(&scene, image);
    for j in 0..ny {
        for i in 0..nx {
            let local = Point::new(i as f64 + 0.5, j as f64 + 0.5);
            let centre = leaf.transform.apply(local);
            let (found, row, column) = scene.hit_map.pixel_at(centre).unwrap_or_else(|| {
                panic!("the centre of pixel ({j}, {i}) at {centre:?} lies over the image")
            });
            assert_eq!((found.artist, row, column), (image, j, i));
            let back = hit.to_pixel.to_pixel(centre);
            assert!(
                points_close(back, local, 1e-6),
                "to_pixel maps the centre of pixel ({j}, {i}) back to {local:?}, not {back:?}"
            );
            let forward = hit.to_pixel.from_pixel(local);
            assert!(
                forward.is_some_and(|p| points_close(p, centre, 1e-6)),
                "from_pixel maps the centre of pixel ({j}, {i}) forward to {centre:?}, not {forward:?}"
            );
        }
    }
    for (u, v) in [(-0.25, 1.0), (3.25, 1.0), (1.5, -0.25), (1.5, 2.25)] {
        let outside = leaf.transform.apply(Point::new(u, v));
        assert!(
            scene.hit_map.pixel_at(outside).is_none(),
            "the point ({u}, {v}) of pixel space lies outside the image"
        );
    }
    // The extent is half-open: a point just inside the low corner lies in pixel (0, 0), a point just beyond the
    // high corner lies outside, and the boundary between two columns belongs to the higher one.
    let eps = 1e-9;
    let (found, row, column) = scene
        .hit_map
        .pixel_at(leaf.transform.apply(Point::new(eps, eps)))
        .expect("a point just inside the low corner lies over the image");
    assert_eq!((found.artist, row, column), (image, 0, 0));
    let beyond = leaf
        .transform
        .apply(Point::new(nx as f64 + eps, ny as f64 + eps));
    assert!(
        scene.hit_map.pixel_at(beyond).is_none(),
        "a point just beyond the high corner lies outside the half-open extent"
    );
    let column_at = |u: f64| {
        scene
            .hit_map
            .pixel_at(leaf.transform.apply(Point::new(u, 0.5)))
            .expect("inside the image")
            .2
    };
    assert_eq!(
        column_at(1.0 + eps),
        1,
        "just past the boundary lies in column 1"
    );
    assert_eq!(
        column_at(1.0 - eps),
        0,
        "just short of the boundary lies in column 0"
    );
}

// WHY: where images overlap the reader sees the one painted last, so that is the one a pointer over the overlap
// must resolve to; and an image mirrored by a backwards range keeps its rows indexed as the array is, so its
// `to_pixel` must invert its mirrored placement exactly and the row found must be the row of the array under the
// pointer, not the row counted from the bottom of the raster.
#[test]
fn the_last_painted_image_wins_where_images_overlap_and_a_mirrored_image_resolves_its_rows() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let ax = fx.axes2d(0, 0);
    // Both 4 by 4 with a pitch of 1: the first covers −0.5 to 3.5 and the second 1.5 to 5.5, along both axes.
    let first = fx.imagesc(ax, vec![4, 4], ramp(16), xy(None, None), |_| {});
    let second = fx.imagesc(
        ax,
        vec![4, 4],
        ramp(16),
        xy(range(2.0, 5.0), range(2.0, 5.0)),
        |_| {},
    );
    let mirrored_axes = fx.axes2d(0, 1);
    // Rows centred on 3, 2, 1 and 0: row 0 of the array lies at y = 3.
    let mirrored = fx.imagesc(
        mirrored_axes,
        vec![4, 2],
        ramp(8),
        xy(None, range(3.0, 0.0)),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    assert_eq!(scene.hit_map.images.len(), 3);

    let (x, y) = axis_maps(&scene, ax);
    let at = |dx: f64, dy: f64| Point::new(x.to_figure(dx), y.to_figure(dy));
    let (hit, row, column) = scene
        .hit_map
        .pixel_at(at(2.7, 2.7))
        .expect("inside both images");
    assert_eq!(
        (hit.artist, row, column),
        (second, 1, 1),
        "the later image wins in the overlap"
    );
    let (hit, row, column) = scene
        .hit_map
        .pixel_at(at(0.6, 0.6))
        .expect("inside the first image only");
    assert_eq!((hit.artist, row, column), (first, 1, 1));

    let leaf = image_leaf(&scene, mirrored);
    let PixelMap::Affine(to_pixel) = &image_hit(&scene, mirrored).to_pixel else {
        panic!("an image on linear axes records the inverse of its affine placement");
    };
    let inverted = leaf.transform.then(*to_pixel);
    assert!(
        transforms_close(inverted, Transform::IDENTITY),
        "to_pixel inverts the placement of the mirrored image: {inverted:?}"
    );
    let (x, y) = axis_maps(&scene, mirrored_axes);
    let at = |dx: f64, dy: f64| Point::new(x.to_figure(dx), y.to_figure(dy));
    for (data_y, row) in [(2.7, 0), (1.2, 2), (0.4, 3)] {
        let (hit, found, column) = scene
            .hit_map
            .pixel_at(at(0.0, data_y))
            .expect("inside the mirrored image");
        assert_eq!(
            (hit.artist, found, column),
            (mirrored, row, 0),
            "y = {data_y} lies in row {row} of the array"
        );
    }
}

// WHY: the hit map serves datatips in 2D axes, where a pixel is found by inverting the placement of the image; a
// hidden image is not on the page, a 3D image has no such inverse (3D picking is future work), and an image the
// compiler skipped (for a strict category, or for a pixel edge with no place on a logarithmic axis) has nothing under
// the pointer, so none of them may record an entry that would point at pixels nobody sees.
#[test]
fn hidden_three_dimensional_and_undrawn_images_have_no_hit_entry() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 3;
    let flat = fx.axes2d(0, 0);
    fx.imagesc(flat, vec![2, 2], ramp(4), xy(None, None), |m| {
        m.visible = false
    });
    fx.imagesc(
        flat,
        vec![2, 2],
        ramp(4),
        placement(ImagePlane::Xz { y: None }, None, None),
        |_| {},
    );
    let strict = fx.imagesc(
        flat,
        vec![2, 2],
        vec![0.0, f64::NAN, 2.0, 3.0],
        xy(None, None),
        |_| {},
    );
    let flat_scale = fx.axes_scale(flat);
    fx.scale_mut(flat_scale).non_finite = out_of_range(OutOfRange::Strict);
    let log = fx.axes2d(0, 1);
    fx.line(log, &[1.0, 100.0], &[0.0, 1.0], None, |_| {});
    // At the default centres the first column edge is at -0.5, which has no place on the logarithmic x axis.
    let on_log = fx.imagesc(log, vec![2, 2], ramp(4), xy(None, None), |_| {});
    fx.ax(log).x.scale = Scale::Log;
    let solid = fx.axes3d(0, 2, View3d::default());
    let floor = fx.imagesc(solid, vec![2, 2], ramp(4), xy(None, None), |_| {});
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    assert_eq!(
        from_source(&leaves, floor).len(),
        1,
        "the 3D image is drawn"
    );
    for id in [strict, on_log] {
        assert!(from_source(&leaves, id).is_empty(), "{id} is not drawn");
    }
    assert!(
        scene.hit_map.images.is_empty(),
        "no image records hit geometry: {:?}",
        scene.hit_map.images
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Legend
// ---------------------------------------------------------------------------------------------------------------

// WHY: a legend entry must look like the artist it names, and a true-colour image has no line, marker or scale, so its
// sample is a filled patch of the mean colour of its finite pixels, which resembles the raster. The sample of a
// colour-mapped image is a sweep of its scale, which `legend_colour_keys.rs` pins; here it is enough that every named
// image has an entry, in artist order, whose label is drawn by the axes like every other part of the legend.
#[test]
fn a_named_image_has_a_legend_entry_and_a_true_colour_image_is_keyed_by_its_mean_colour() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.axes_scale_mut(ax).colourmap = ColourmapName::Magma.into();
    fx.palette_mut(ax).colours = palette_colours(&MAGMA);
    let mapped_image = fx.imagesc(ax, vec![2, 2], ramp(4), xy(None, None), |m| {
        m.label = text("Mapped")
    });
    let indexed = fx.indexed_image(
        ax,
        vec![2, 2],
        vec![0u8, 50, 100, 150],
        xy(range(3.0, 4.0), None),
        |m| m.label = text("Indexed"),
    );
    // Pure red, green and blue, and a pixel with a non-finite component that does not count: the mean is a third of
    // each component.
    let true_colour = fx.image(
        ax,
        vec![2, 2, 3],
        vec![
            1.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            1.0,
            f64::NAN,
            1.0,
            1.0,
        ],
        xy(range(6.0, 7.0), None),
        |m| m.label = text("Pixels"),
    );
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        location: LegendLocation::NorthEast,
        boxed: true,
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    let entries: Vec<_> = scene
        .hit_map
        .legend_entries
        .iter()
        .filter(|e| e.legend == LegendOwner::Axes(ax))
        .collect();
    assert_eq!(
        entries.iter().map(|e| entry_artist(e)).collect::<Vec<_>>(),
        vec![mapped_image, indexed, true_colour],
        "one entry per named image, in artist order"
    );
    let leaves = leaves(&scene);
    let entry = entries
        .iter()
        .find(|e| entry_artist(e) == true_colour)
        .unwrap();
    let patches = patches_in(&leaves, ax, entry.rect);
    assert_eq!(
        patches.len(),
        1,
        "one filled patch lies inside the entry of the true-colour image: {patches:?}"
    );
    assert_eq!(
        patches[0],
        ([85, 85, 85], 1.0),
        "the patch takes the mean colour of the finite pixels, opaque"
    );
    for artist in [mapped_image, indexed, true_colour] {
        let entry = entries.iter().find(|e| entry_artist(e) == artist).unwrap();
        assert!(
            leaves.iter().any(|l| l.source == Some(ax)
                && l.glyphs().is_some()
                && l.bbox().is_some_and(|b| inside(b, entry.rect, 0.5))),
            "the label lies inside the entry of {artist}"
        );
    }
}

// WHY: the `best` legend location is the corner that covers the least data, and an image is data too, so its
// corners count: a small image inside the top-right corner of the plot sends the legend away from that corner, and
// images inside both top corners send it to the bottom. Counting only path vertices would let the legend sit on the
// one thing the reader came to see.
#[test]
fn the_best_legend_location_keeps_clear_of_an_image() {
    // Every image is two by two pixels with y edges 9.3 to 9.6 under limits of [0, 10], that is 4 to 7 percent of
    // the plot height below its top, and x edges 6 to 12 percent of the plot width from a side. The sizing this
    // relies on: on the default figure the plot is about 400 by 230 points and the legend, inset 0.7 font sizes,
    // about 54 points wide and 19 points high per entry, so the corner candidate nearest an image contains the
    // image's corners and the candidate is passed over.
    let build = |columns: &[(f64, f64)]| {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let mut images = Vec::new();
        for &(first, last) in columns {
            images.push(fx.imagesc(
                ax,
                vec![2, 2],
                ramp(4),
                xy(range(first, last), range(9.375, 9.525)),
                |m| m.label = text("Patch"),
            ));
        }
        let axes = fx.ax(ax);
        axes.x.limits = Limits::Manual {
            min: 0.0,
            max: 10.0,
        };
        axes.y.limits = Limits::Manual {
            min: 0.0,
            max: 10.0,
        };
        axes.legend = Some(Legend::Inside(InsideLegend {
            location: LegendLocation::Best,
            boxed: true,
            ..InsideLegend::default()
        }));
        let scene = compile_figure(&fx.build());
        let entries: Vec<Rect> = scene
            .hit_map
            .legend_entries
            .iter()
            .filter(|e| e.legend == LegendOwner::Axes(ax))
            .map(|e| e.rect)
            .collect();
        assert_eq!(entries.len(), images.len(), "one entry per image");
        let plot = axes_hit(&scene, ax).plot_rect;
        let boxes: Vec<Rect> = images
            .iter()
            .map(|id| image_leaf(&scene, *id).bbox().unwrap())
            .collect();
        (entries, plot, boxes)
    };

    // One image with x edges 8.8 to 9.4, inside the north-east corner: the legend moves west.
    let (entries, plot, boxes) = build(&[(8.95, 9.25)]);
    for entry in &entries {
        assert!(
            boxes.iter().all(|b| disjoint(*entry, *b)),
            "the legend entry {entry:?} keeps clear of the image {boxes:?}"
        );
        assert!(
            entry.x + entry.width / 2.0 < plot.x + plot.width / 2.0,
            "the legend moves to the left half of the plot: {entry:?} in {plot:?}"
        );
    }

    // Images inside both top corners, with x edges 8.8 to 9.4 and 0.6 to 1.2: the legend moves to the bottom.
    let (entries, plot, boxes) = build(&[(8.95, 9.25), (0.75, 1.05)]);
    for entry in &entries {
        assert!(
            boxes.iter().all(|b| disjoint(*entry, *b)),
            "the legend entry {entry:?} keeps clear of the images {boxes:?}"
        );
        assert!(
            entry.y + entry.height / 2.0 > plot.y + plot.height / 2.0,
            "the legend moves to the lower half of the plot: {entry:?} in {plot:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Warnings and robustness
// ---------------------------------------------------------------------------------------------------------------

// WHY: compilation never fails, so an image whose data is missing or of the wrong shape, or whose placement cannot
// be drawn (a non-finite centre or offset, or centres that coincide across more than one pixel, which validation
// reports in the same terms), is skipped with one warning naming it while the rest of the figure is drawn; a
// compiler that indexed the array by a shape it did not check, or divided by a zero pitch, would panic and take the
// whole figure with it.
#[test]
fn an_image_with_unusable_data_or_placement_is_skipped_with_one_warning_naming_it() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let ax = fx.axes2d(0, 0);
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let cases = [
        (
            "image whose pixels are not in the data table",
            fx.image(ax, vec![1, 1, 3], vec![0u8; 3], xy(None, None), |i| {
                i.pixels = NodeId(999)
            }),
        ),
        (
            "image whose pixels are two-dimensional",
            fx.image(ax, vec![2, 3], vec![0.0; 6], xy(None, None), |_| {}),
        ),
        (
            "image with two components per pixel",
            fx.image(ax, vec![2, 3, 2], vec![0.0; 12], xy(None, None), |_| {}),
        ),
        (
            "image with five components per pixel",
            fx.image(ax, vec![1, 1, 5], vec![0u8; 5], xy(None, None), |_| {}),
        ),
        (
            "indexed image whose indices are three-dimensional",
            fx.indexed_image(ax, vec![2, 3, 1], vec![0u8; 6], xy(None, None), |_| {}),
        ),
        (
            "mapped image whose values are one-dimensional",
            fx.imagesc(ax, vec![6], vec![0.0; 6], xy(None, None), |_| {}),
        ),
        (
            "image with a NaN column centre",
            fx.imagesc(
                ax,
                vec![2, 3],
                ramp(6),
                xy(range(f64::NAN, 2.0), None),
                |_| {},
            ),
        ),
        (
            "image with an infinite row centre",
            fx.imagesc(
                ax,
                vec![2, 3],
                ramp(6),
                xy(None, range(0.0, f64::INFINITY)),
                |_| {},
            ),
        ),
        (
            "image with a NaN plane offset",
            fx.imagesc(
                ax,
                vec![2, 3],
                ramp(6),
                placement(ImagePlane::Xy { z: Some(f64::NAN) }, None, None),
                |_| {},
            ),
        ),
        (
            "image whose three column centres coincide",
            fx.imagesc(ax, vec![2, 3], ramp(6), xy(range(2.0, 2.0), None), |_| {}),
        ),
        (
            "indexed image whose two row centres coincide",
            fx.indexed_image(
                ax,
                vec![2, 3],
                vec![0u8; 6],
                xy(None, range(1.0, 1.0)),
                |_| {},
            ),
        ),
    ];
    let solid = fx.axes3d(0, 1, View3d::default());
    let wall_with_nan_offset = fx.imagesc(
        solid,
        vec![2, 2],
        ramp(4),
        placement(ImagePlane::Xz { y: Some(f64::NAN) }, None, None),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    for (what, id) in cases {
        assert_skipped_with_one_warning(&scene, id, what);
    }
    assert_skipped_with_one_warning(&scene, wall_with_nan_offset, "wall image with a NaN offset");
    assert!(
        !from_source(&leaves(&scene), line).is_empty(),
        "the line is still drawn"
    );
}
