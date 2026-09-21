//! Surfaces in two-dimensional axes: the pseudocolour plot (MATLAB's `pcolor`).
//!
//! A 2D axes shows a surface from directly above, so the grid alone places the faces and the field only colours them.
//! The marking of the faces as dense content, which a 2D axes shares with a 3D one, is pinned in `dense`.

use ironlab_ir::{Artist, ColorSpec, Grid, Limits, NodeId, Scale, Surface};
use ironlab_scene::Scene;
use ironlab_scene::maths::colormap::VIRIDIS;

use crate::common::{Fx, compile_figure, nearest_lut_index, rgb8};
use crate::probe::{Leaf, assert_close, axes_hit, axis_maps, from_source, inside, leaves};

/// Columns of the fixture grid, unevenly spaced so that a face placed by node index rather than by coordinate is
/// caught, and one more than the rows so that a transposed grid is caught.
const X: [f64; 4] = [-2.0, -1.0, 1.0, 4.0];
/// Rows of the fixture grid.
const Y: [f64; 3] = [10.0, 20.0, 50.0];

/// The field of the fixture surface. Its values are thousands of times larger than the grid coordinates, so a
/// compiler that let the field position anything, or widen a limit, would move the faces visibly.
fn field(x: f64, y: f64) -> f64 {
    1000.0 * (x + 3.0) * y
}

/// A face as the data it was drawn from: the corners of its quadrilateral mapped back through the axis maps of the
/// axes, sorted, and the index of the colormap entry nearest to its fill, when it is filled.
#[derive(Debug)]
struct Face {
    corners: Vec<[f64; 2]>,
    entry: Option<usize>,
}

impl Face {
    fn centroid(&self) -> [f64; 2] {
        let n = self.corners.len() as f64;
        [0, 1].map(|k| self.corners.iter().map(|c| c[k]).sum::<f64>() / n)
    }
}

fn sorted(mut corners: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    corners.sort_by(|a, b| a.partial_cmp(b).expect("corners are not NaN"));
    corners
}

/// Returns the faces a surface drew in a 2D axes, in paint order.
fn faces(scene: &Scene, ax: NodeId, surface: NodeId) -> Vec<Face> {
    let (xmap, ymap) = axis_maps(scene, ax);
    from_source(&leaves(scene), surface)
        .iter()
        .map(|leaf| {
            let subpaths = leaf.subpaths();
            assert_eq!(subpaths.len(), 1, "a face is one closed outline");
            Face {
                corners: sorted(
                    subpaths[0]
                        .iter()
                        .map(|p| [xmap.to_data(p.x), ymap.to_data(p.y)])
                        .collect(),
                ),
                entry: leaf
                    .path()
                    .and_then(|p| p.fill)
                    .map(|fill| nearest_lut_index(&VIRIDIS, fill.color)),
            }
        })
        .collect()
}

/// Returns the face whose centroid lies nearest to `(x, y)` in data space.
fn face_at(faces: &[Face], x: f64, y: f64) -> &Face {
    let distance = |f: &Face| {
        let [cx, cy] = f.centroid();
        (cx - x).powi(2) + (cy - y).powi(2)
    };
    faces
        .iter()
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
        .expect("the surface drew faces")
}

#[track_caller]
fn assert_corners(actual: &[[f64; 2]], expected: Vec<[f64; 2]>, tol: f64) {
    let expected = sorted(expected);
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    for (a, e) in actual.iter().zip(&expected) {
        assert!(
            (a[0] - e[0]).abs() <= tol && (a[1] - e[1]).abs() <= tol,
            "corners {actual:?} differ from the grid nodes {expected:?}"
        );
    }
}

/// The colormap entry a value takes between the colour limits `[cmin, cmax]`: the limits map linearly onto the 256
/// entries of the colormap. The arithmetic is written out here, rather than taken from the compiler's own
/// normalisation, so that the expected colour does not depend on the code under test.
fn entry(value: f64, cmin: f64, cmax: f64) -> usize {
    let t = (value - cmin) / (cmax - cmin);
    ((t * 256.0).floor() as usize).min(255)
}

fn range(values: impl IntoIterator<Item = f64>) -> (f64, f64) {
    values
        .into_iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        })
}

/// A 2D axes holding the fixture surface, customised by `edit`.
fn fixture(edit: impl FnOnce(&mut Surface)) -> (Fx, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let surface = fx.surface(ax, &X, &Y, field, edit);
    (fx, ax, surface)
}

/// Asserts that every cell of the rectilinear grid `x × y` is drawn as one face whose corners are the four nodes of
/// that cell.
#[track_caller]
fn assert_faces_on_nodes(faces: &[Face], x: &[f64], y: &[f64]) {
    assert_eq!(
        faces.len(),
        (x.len() - 1) * (y.len() - 1),
        "one face per cell"
    );
    for j in 0..y.len() - 1 {
        for i in 0..x.len() - 1 {
            let (x0, x1, y0, y1) = (x[i], x[i + 1], y[j], y[j + 1]);
            let face = face_at(faces, (x0 + x1) / 2.0, (y0 + y1) / 2.0);
            assert_corners(
                &face.corners,
                vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
                1e-9,
            );
        }
    }
}

/// Asserts that the face of every cell of the rectilinear grid `x × y` is filled with the colormap entry of the mean
/// of `value` over the four nodes of that cell, between the colour limits `[cmin, cmax]`, and returns the entries.
/// One entry of slack absorbs the rounding of the fill to 8-bit colour.
#[track_caller]
fn assert_faces_take_mean_colour(
    faces: &[Face],
    x: &[f64],
    y: &[f64],
    value: impl Fn(f64, f64) -> f64,
    (cmin, cmax): (f64, f64),
) -> Vec<usize> {
    let mut entries = Vec::new();
    for j in 0..y.len() - 1 {
        for i in 0..x.len() - 1 {
            let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            let mean = corners.iter().map(|&(i, j)| value(x[i], y[j])).sum::<f64>() / 4.0;
            let face = face_at(faces, (x[i] + x[i + 1]) / 2.0, (y[j] + y[j + 1]) / 2.0);
            let actual = face.entry.expect("faces are filled by default");
            let expected = entry(mean, cmin, cmax);
            assert!(
                actual.abs_diff(expected) <= 1,
                "cell ({i}, {j}) with mean {mean} has colormap entry {actual}, expected {expected}"
            );
            entries.push(actual);
        }
    }
    entries
}

// Why: a pseudocolour plot is read by where its cells are: each cell of the grid must appear as one quadrilateral
// whose corners are the four grid nodes of that cell, placed by the x and y axes of the 2D axes. The field takes no
// part in placing them, because a 2D axes has no z axis to place it along.
#[test]
fn a_surface_in_2d_axes_draws_each_grid_cell_as_a_quadrilateral_on_its_nodes() {
    let (fx, ax, surface) = fixture(|_| {});
    let scene = compile_figure(&fx.build());
    let faces = faces(&scene, ax, surface);
    assert_eq!(faces.len(), 3 * 2, "one face per cell of the 4 by 3 grid");
    assert_faces_on_nodes(&faces, &X, &Y);
}

// Why: as for a contour, a gridded field fills the plot area, so the automatic x and y limits are exactly the extent
// of the grid. The field must not reach any limit of a 2D axes: were its values (here up to a million) mixed into the
// x or y extent, or were they to position the vertices, the faces would collapse into a corner of the plot. Scaling
// the field must therefore leave the limits and every vertex where they were.
#[test]
fn the_field_influences_neither_the_limits_nor_the_geometry_of_a_2d_axes() {
    let build = |gain: f64| {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let surface = fx.surface(ax, &X, &Y, |x, y| gain * field(x, y), |_| {});
        let scene = compile_figure(&fx.build());
        let maps = axis_maps(&scene, ax);
        let vertices: Vec<_> = from_source(&leaves(&scene), surface)
            .iter()
            .flat_map(|l| l.subpaths().into_iter().flatten())
            .collect();
        (maps, vertices)
    };
    let ((xmap, ymap), vertices) = build(1.0);
    assert_eq!((xmap.min, xmap.max), (-2.0, 4.0), "x is tight to the grid");
    assert_eq!((ymap.min, ymap.max), (10.0, 50.0), "y is tight to the grid");

    let (maps, moved) = build(-1e3);
    assert_eq!(maps, (xmap, ymap), "the limits do not follow the field");
    assert!(!vertices.is_empty());
    assert_eq!(vertices, moved, "the vertices do not follow the field");
}

// Why: the colour of a cell is what a pseudocolour plot shows. A flat face takes the colormap sample of the mean of
// its four corner values, so that it represents the whole cell rather than one corner, scaled by automatic colour
// limits equal to the range of the field, so that the plot uses the whole colormap without the user setting limits.
#[test]
fn faces_in_2d_axes_take_the_colour_of_their_mean_field_value() {
    let (fx, ax, surface) = fixture(|_| {});
    let scene = compile_figure(&fx.build());
    let faces = faces(&scene, ax, surface);
    let limits = range(Y.iter().flat_map(|&y| X.iter().map(move |&x| field(x, y))));
    let entries = assert_faces_take_mean_colour(&faces, &X, &Y, field, limits);
    let (lo, hi) = (entries.iter().min().unwrap(), entries.iter().max().unwrap());
    assert!(hi - lo > 100, "the faces span the colormap: {entries:?}");
}

// Why: separate colour data lets a user lay one quantity out on the grid of another. When it is given it replaces the
// field entirely for colouring: both the face colours and the automatic colour limits come from it. The colour data
// here decreases where the field increases and spans a range a million times smaller, so colours or limits still
// taken from the field would reverse the order of the faces or paint them all in one colour.
#[test]
fn colour_data_replaces_the_field_for_face_colours_and_colour_limits() {
    let colour = |x: f64, y: f64| -1e-3 * (x + 3.0) * y;
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let values: Vec<f64> = Y
        .iter()
        .flat_map(|&y| X.iter().map(move |&x| colour(x, y)))
        .collect();
    let limits = range(values.iter().copied());
    let c = fx.matrix(Y.len(), X.len(), values);
    let surface = fx.surface(ax, &X, &Y, field, |s| s.c = Some(c));
    let scene = compile_figure(&fx.build());
    let faces = faces(&scene, ax, surface);
    assert_eq!(faces.len(), 6);
    assert_faces_take_mean_colour(&faces, &X, &Y, colour, limits);
}

// Why: NaN marks a node without data. A face cannot be coloured, or honestly outlined, from three corners, so a
// missing node removes exactly the faces that touch it and leaves a hole, whether the node is missing from the field
// or only from the colour data. The field positions nothing in a 2D axes, yet a node missing from it is still a
// missing node. Which faces go matters, not only how many: a compiler that dropped the right number of the wrong
// faces would hide valid data and paint invented data.
#[test]
fn a_missing_node_removes_exactly_the_faces_that_touch_it() {
    let nodes = [0.0, 1.0, 2.0, 3.0];
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    // The colour data lacks the last node (3, 3), a corner of the single cell (2, 2).
    let c = fx.matrix(
        4,
        4,
        (0..16)
            .map(|k| if k == 15 { f64::NAN } else { k as f64 })
            .collect(),
    );
    // The field lacks the node (1, 0) on the lower edge, a corner of the cells (0, 0) and (1, 0).
    let surface = fx.surface(
        ax,
        &nodes,
        &nodes,
        |x, y| {
            if (x, y) == (1.0, 0.0) {
                f64::NAN
            } else {
                x + y
            }
        },
        |s| s.c = Some(c),
    );
    let scene = compile_figure(&fx.build());
    let mut drawn: Vec<(usize, usize)> = faces(&scene, ax, surface)
        .iter()
        .map(|f| {
            let [cx, cy] = f.centroid();
            (cx.floor() as usize, cy.floor() as usize)
        })
        .collect();
    drawn.sort_unstable();
    let mut expected: Vec<(usize, usize)> = (0..3)
        .flat_map(|i| (0..3).map(move |j| (i, j)))
        .filter(|cell| ![(0, 0), (1, 0), (2, 2)].contains(cell))
        .collect();
    expected.sort_unstable();
    assert_eq!(drawn, expected, "cells drawn, as (column, row)");
}

// Why: coordinates that decrease along the grid are ordinary in a pseudocolour plot: depth below a surface, a
// frequency axis stored from high to low, a matrix whose first row is the top of the picture. The axes still runs
// from the smallest coordinate to the largest, every cell is still drawn on its own nodes, and every face still
// takes the colour of its own cell. A compiler that assumed increasing coordinates would produce reversed limits,
// lose the faces, or pair the colours of one end of the grid with the positions of the other.
#[test]
fn a_grid_with_descending_coordinates_is_drawn_on_its_nodes_in_its_own_colours() {
    let x = [4.0, 1.0, -1.0, -2.0];
    let y = [50.0, 20.0, 10.0];
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let surface = fx.surface(ax, &x, &y, field, |_| {});
    let scene = compile_figure(&fx.build());

    let (xmap, ymap) = axis_maps(&scene, ax);
    assert_eq!((xmap.min, xmap.max), (-2.0, 4.0));
    assert_eq!((ymap.min, ymap.max), (10.0, 50.0));
    let faces = faces(&scene, ax, surface);
    assert_faces_on_nodes(&faces, &x, &y);
    let limits = range(y.iter().flat_map(|&y| x.iter().map(move |&x| field(x, y))));
    assert_faces_take_mean_colour(&faces, &x, &y, field, limits);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
}

// Why: a pseudocolour plot draws cells between nodes, so a field with a single row of nodes has no cell to draw,
// which surprises a user who expects one coloured strip per value as an image would give. The surface must be left
// out with a warning that names it, so that the user learns why the axes is empty, and the rest of the axes must
// still be drawn.
#[test]
fn a_single_row_of_nodes_has_no_cells_and_is_skipped_with_a_warning() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let z = fx.matrix(1, 4, vec![1.0, 2.0, 3.0, 4.0]);
    let grid = Grid::Rectilinear {
        x: fx.vector(&X),
        y: fx.vector(&[10.0]),
    };
    let surface = fx.node();
    fx.ax(ax).artists.push(Artist::Surface(Surface {
        id: surface,
        grid,
        z,
        ..Surface::default()
    }));
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    assert!(from_source(&leaves, surface).is_empty());
    assert!(!from_source(&leaves, line).is_empty(), "the line is drawn");
    assert!(
        scene.warnings.iter().any(|w| w.node == Some(surface)),
        "a warning names the surface: {:?}",
        scene.warnings
    );
}

// Why: a curvilinear grid gives every node its own position, which is how a pseudocolour plot is drawn on a sheared,
// polar or otherwise mapped mesh. Each face must join the positions of its own four nodes, so faces are
// parallelograms here rather than axis-aligned rectangles, and the limits are tight to the extent of the node
// positions rather than to the node indices.
#[test]
fn a_curvilinear_grid_places_every_node_at_its_own_position() {
    let (ny, nx) = (3, 4);
    let node = |i: usize, j: usize| [i as f64 + 0.5 * j as f64, 2.0 * j as f64 + 0.25 * i as f64];
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let coordinates = |k: usize| -> Vec<f64> {
        (0..ny)
            .flat_map(|j| (0..nx).map(move |i| node(i, j)[k]))
            .collect()
    };
    let grid = Grid::Curvilinear {
        x: fx.matrix(ny, nx, coordinates(0)),
        y: fx.matrix(ny, nx, coordinates(1)),
    };
    let z = fx.matrix(ny, nx, (0..ny * nx).map(|k| k as f64).collect());
    let surface = fx.node();
    fx.ax(ax).artists.push(Artist::Surface(Surface {
        id: surface,
        grid,
        z,
        ..Surface::default()
    }));
    let scene = compile_figure(&fx.build());

    let (xmap, ymap) = axis_maps(&scene, ax);
    assert_eq!((xmap.min, xmap.max), (0.0, 4.0));
    assert_eq!((ymap.min, ymap.max), (0.0, 4.75));

    let faces = faces(&scene, ax, surface);
    assert_eq!(faces.len(), (nx - 1) * (ny - 1));
    for j in 0..ny - 1 {
        for i in 0..nx - 1 {
            let corners = vec![
                node(i, j),
                node(i + 1, j),
                node(i + 1, j + 1),
                node(i, j + 1),
            ];
            let centre = [0, 1].map(|k| corners.iter().map(|c| c[k]).sum::<f64>() / 4.0);
            let face = face_at(&faces, centre[0], centre[1]);
            assert_corners(&face.corners, corners, 1e-9);
        }
    }
}

/// The distinct horizontal figure-space positions of the vertices of a surface, in increasing order.
fn vertex_columns(leaves: &[Leaf], surface: NodeId) -> Vec<f64> {
    let mut xs: Vec<f64> = from_source(leaves, surface)
        .iter()
        .flat_map(|l| l.subpaths().into_iter().flatten())
        .map(|p| p.x)
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    xs
}

// Why: a surface is made of vertices, so, unlike an image, it can be drawn on a logarithmic axis: each vertex goes
// through the log map. Nodes one decade apart must therefore be evenly spaced across the plot, with the limits tight
// to the grid, which is the reason to choose a surface over an image for log-spaced data such as a spectrogram.
#[test]
fn a_logarithmic_x_axis_places_the_vertices_through_the_log_map() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let surface = fx.surface(
        ax,
        &[1.0, 10.0, 100.0, 1000.0],
        &[0.0, 1.0, 2.0],
        |x, y| x.log10() + y,
        |_| {},
    );
    fx.ax(ax).x.scale = Scale::Log;
    let scene = compile_figure(&fx.build());

    let (xmap, _) = axis_maps(&scene, ax);
    assert!(xmap.log);
    assert_eq!((xmap.min, xmap.max), (1.0, 1000.0));
    let plot = axes_hit(&scene, ax).plot_rect;
    let columns = vertex_columns(&leaves(&scene), surface);
    assert_eq!(columns.len(), 4, "{columns:?}");
    for (k, x) in columns.iter().enumerate() {
        assert_close(*x, plot.x + plot.width * k as f64 / 3.0, 1e-6);
    }
    assert!(
        scene.warnings.is_empty(),
        "positive coordinates lose nothing: {:?}",
        scene.warnings
    );
}

// Why: a node at a non-positive coordinate has no position on a logarithmic axis, so the faces that touch it cannot
// be drawn; they must be left out, with the user told which artist lost data, rather than emitted with non-finite
// geometry or stretched to the edge of the plot. The remaining faces are drawn, and the dropped column does not
// widen the limits.
#[test]
fn faces_touching_a_non_positive_coordinate_on_a_log_axis_are_dropped_with_a_warning() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let surface = fx.surface(
        ax,
        &[0.0, 1.0, 10.0, 100.0],
        &[0.0, 1.0, 2.0],
        |x, y| x + y,
        |_| {},
    );
    fx.ax(ax).x.scale = Scale::Log;
    let scene = compile_figure(&fx.build());

    let (xmap, _) = axis_maps(&scene, ax);
    assert_eq!((xmap.min, xmap.max), (1.0, 100.0));
    let faces = faces(&scene, ax, surface);
    assert_eq!(faces.len(), 2 * 2, "the column of cells from x = 0 is gone");
    assert!(
        faces
            .iter()
            .flat_map(|f| &f.corners)
            .all(|c| c[0].is_finite() && c[0] >= 1.0 - 1e-9),
        "{faces:?}"
    );
    assert!(
        scene.warnings.iter().any(|w| w.node == Some(surface)),
        "a warning names the surface: {:?}",
        scene.warnings
    );
}

// Why: the usual pseudocolour figure lays a line, markers or contours over the coloured field. In a 2D axes nothing
// is sorted by depth, so the faces must all be painted where the surface stands in the artist list: beneath a later
// line and over an earlier one. When the user zooms into the field, the faces that extend beyond the limits must be
// cut off at the plot rectangle, as every other 2D artist is, rather than spill over the tick labels.
#[test]
fn faces_are_painted_in_artist_order_and_clipped_to_the_plot_rectangle() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let under = fx.line(ax, &[-1.0, 1.0], &[20.0, 30.0], None, |_| {});
    let surface = fx.surface(ax, &X, &Y, field, |_| {});
    let over = fx.line(ax, &[-1.0, 1.0], &[30.0, 20.0], None, |_| {});
    let axes = fx.ax(ax);
    axes.x.limits = Limits::Manual {
        min: -1.5,
        max: 2.0,
    };
    axes.y.limits = Limits::Manual {
        min: 15.0,
        max: 40.0,
    };
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let positions = |id| -> Vec<usize> {
        (0..leaves.len())
            .filter(|i| leaves[*i].source == Some(id))
            .collect()
    };
    let (a, s, b) = (positions(under), positions(surface), positions(over));
    assert_eq!(s.len(), 6);
    assert!(
        a.iter().max() < s.iter().min() && s.iter().max() < b.iter().min(),
        "line at {a:?}, then faces at {s:?}, then line at {b:?}"
    );

    let plot = axes_hit(&scene, ax).plot_rect;
    for face in from_source(&leaves, surface) {
        assert_eq!(face.clip, Some(plot), "a face is clipped to the plot");
        let whole = face.bbox().expect("a face has geometry");
        assert!(
            !inside(whole, plot, 1e-6),
            "every cell of the fixture crosses the manual limits"
        );
    }
}

// Why: the faces of a fine grid are smaller than the edges drawn round them, so a pseudocolour plot of dense data is
// drawn without edges (MATLAB's `shading flat`). Switching the edges off must remove every stroke and keep every
// fill, and a fixed face colour with colormapped edges (the `mesh` look, seen from above) must do the reverse of the
// default: one fill colour throughout and edges that take the colours of their cells.
#[test]
fn face_and_edge_colours_are_honoured_in_2d_axes() {
    let (fx, _, flat) = fixture(|s| s.edge = ColorSpec::None);
    let scene = compile_figure(&fx.build());
    let drawn = from_source(&leaves(&scene), flat);
    assert_eq!(drawn.len(), 6);
    for leaf in &drawn {
        let path = leaf.path().expect("a face is a path");
        assert!(path.fill.is_some() && path.stroke.is_none(), "{path:?}");
    }

    let grey = ironlab_ir::Color::rgb(0.5, 0.5, 0.5);
    let (fx, _, wire) = fixture(|s| {
        s.face = ColorSpec::Rgba { color: grey };
        s.edge = ColorSpec::Colormapped;
        s.edge_width_pt = 2.0;
    });
    let scene = compile_figure(&fx.build());
    let drawn = from_source(&leaves(&scene), wire);
    assert_eq!(drawn.len(), 6);
    let mut edge_colours = Vec::new();
    for leaf in &drawn {
        let path = leaf.path().expect("a face is a path");
        assert_eq!(rgb8(path.fill.expect("filled").color), [128, 128, 128]);
        let stroke = path.stroke.as_ref().expect("stroked");
        assert_eq!(stroke.width, 2.0);
        edge_colours.push(nearest_lut_index(&VIRIDIS, stroke.color));
    }
    edge_colours.sort_unstable();
    edge_colours.dedup();
    assert!(
        edge_colours.len() > 1,
        "edges are coloured by their cells: {edge_colours:?}"
    );
}

// Why: a 2D axes keeps a z axis it never shows, and its scale may be left logarithmic (for example after the axes
// was switched back from 3D). The field of a surface is not placed along that axis, so zero and negative field
// values, which are ordinary in a pseudocolour plot, must neither remove faces nor raise the warning about values
// lost on a logarithmic axis.
#[test]
fn a_logarithmic_z_scale_does_not_affect_a_surface_in_2d_axes() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let surface = fx.surface(ax, &X, &Y, |x, y| x * (y - 20.0), |_| {});
    fx.ax(ax).z.scale = Scale::Log;
    let scene = compile_figure(&fx.build());
    assert_eq!(faces(&scene, ax, surface).len(), 6);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
}
