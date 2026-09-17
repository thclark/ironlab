use ironlab_scene::maths::contour::{
    Coords, GridRef, GridShapeError, Polyline, auto_levels, band_edges, isobands, isolines,
};
use proptest::prelude::*;

use crate::{assert_close, is_nice_step};

fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|k| a + (b - a) * k as f64 / (n - 1) as f64)
        .collect()
}

fn indices(n: usize) -> Vec<f64> {
    (0..n).map(|k| k as f64).collect()
}

/// Samples `f` on the rectilinear grid `x × y` in row-major order.
fn sample_field(x: &[f64], y: &[f64], f: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    y.iter()
        .flat_map(|&yj| x.iter().map(move |&xi| (xi, yj)))
        .map(|(xi, yj)| f(xi, yj))
        .collect()
}

/// Signed shoelace area; positive for counter-clockwise polygons.
fn signed_area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|k| {
            let [x0, y0] = points[k];
            let [x1, y1] = points[(k + 1) % n];
            x0 * y1 - x1 * y0
        })
        .sum::<f64>()
        / 2.0
}

/// A half-open band `[lo, hi)`.
type Band = (f64, f64);

type Polygon = Vec<[f64; 2]>;

fn z_range(z: &[f64]) -> (f64, f64) {
    z.iter()
        .filter(|v| v.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(*v), hi.max(*v))
        })
}

/// The smooth test field used by the band tests.
fn wavy(x: f64, y: f64) -> f64 {
    (1.3 * x).sin() * (2.1 * y).cos() + 0.3 * x
}

/// Evaluates the piecewise-linear interpolant that `isobands` contours, on a grid whose node
/// `(i, j)` sits at `(i, j)`.
fn triangle_interpolant(z: &[f64], nx: usize, ny: usize, p: [f64; 2]) -> f64 {
    let i = (p[0].floor() as usize).min(nx - 2);
    let j = (p[1].floor() as usize).min(ny - 2);
    let (u, v) = (p[0] - i as f64, p[1] - j as f64);
    let at = |di: usize, dj: usize| z[(j + dj) * nx + i + di];
    if u >= v {
        // Triangle (i, j), (i + 1, j), (i + 1, j + 1).
        at(0, 0) + u * (at(1, 0) - at(0, 0)) + v * (at(1, 1) - at(1, 0))
    } else {
        // Triangle (i, j), (i + 1, j + 1), (i, j + 1).
        at(0, 0) + v * (at(0, 1) - at(0, 0)) + u * (at(1, 1) - at(0, 1))
    }
}

/// Winding number of the closed polygon `polygon` around `p` (positive for counter-clockwise).
fn winding_number(polygon: &[[f64; 2]], p: [f64; 2]) -> i32 {
    let n = polygon.len();
    let mut winding = 0;
    for k in 0..n {
        let a = polygon[k];
        let b = polygon[(k + 1) % n];
        let side = (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1]);
        if a[1] <= p[1] {
            if b[1] > p[1] && side > 0.0 {
                winding += 1;
            }
        } else if b[1] <= p[1] && side < 0.0 {
            winding -= 1;
        }
    }
    winding
}

fn index_grid<'a>(nx: usize, ny: usize, x: &'a [f64], y: &'a [f64], z: &'a [f64]) -> GridRef<'a> {
    GridRef {
        nx,
        ny,
        coords: Coords::Rectilinear { x, y },
        z,
    }
}

fn segments_intersect(a: [[f64; 2]; 2], b: [[f64; 2]; 2]) -> bool {
    let orient = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let d1 = orient(a[0], a[1], b[0]);
    let d2 = orient(a[0], a[1], b[1]);
    let d3 = orient(b[0], b[1], a[0]);
    let d4 = orient(b[0], b[1], a[1]);
    d1 * d2 <= 0.0 && d3 * d4 <= 0.0
}

/// Whether `line` is the open two-point segment between `p` and `q` in either direction.
fn is_segment(line: &Polyline, p: [f64; 2], q: [f64; 2]) -> bool {
    let close =
        |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12;
    !line.closed
        && line.points.len() == 2
        && ((close(line.points[0], p) && close(line.points[1], q))
            || (close(line.points[0], q) && close(line.points[1], p)))
}

fn unit_cell_grid(z: &[f64]) -> GridRef<'_> {
    const UNIT: [f64; 2] = [0.0, 1.0];
    GridRef {
        nx: 2,
        ny: 2,
        coords: Coords::Rectilinear { x: &UNIT, y: &UNIT },
        z,
    }
}

// Why: validation lets the scene compiler turn malformed artist data into a warning instead of
// a panic or silently empty plot.
#[test]
fn validate_reports_shape_errors() {
    let x = [0.0, 1.0, 2.0];
    let y = [0.0, 1.0];
    let z = [0.0; 6];
    let good = GridRef {
        nx: 3,
        ny: 2,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    assert_eq!(good.validate(), Ok(()));
    assert_eq!(
        GridRef {
            nx: 1,
            ny: 2,
            ..good
        }
        .validate(),
        Err(GridShapeError::TooSmall { nx: 1, ny: 2 })
    );
    assert_eq!(
        GridRef { z: &z[..5], ..good }.validate(),
        Err(GridShapeError::ValuesLength {
            expected: 6,
            actual: 5
        })
    );
    assert_eq!(
        GridRef {
            coords: Coords::Rectilinear { x: &x[..2], y: &y },
            ..good
        }
        .validate(),
        Err(GridShapeError::CoordsLength {
            axis: 'x',
            expected: 3,
            actual: 2
        })
    );
    assert_eq!(
        GridRef {
            coords: Coords::Curvilinear { x: &z, y: &z[..4] },
            ..good
        }
        .validate(),
        Err(GridShapeError::CoordsLength {
            axis: 'y',
            expected: 6,
            actual: 4
        })
    );
    let bad = GridRef { z: &z[..5], ..good };
    assert!(isolines(&bad, 0.5).is_empty());
    assert!(isobands(&bad, 0.0, 1.0).is_empty());
}

// Why: contours are computed in index space and placed through `map`, so the mapping must be
// exact at nodes, linear per axis on non-uniform rectilinear grids, and bilinear inside the true
// quadrilateral on curvilinear grids.
#[test]
fn map_interpolates_node_positions() {
    let x = [0.0, 1.0, 3.0];
    let y = [0.0, 2.0];
    let z = [0.0; 6];
    let rect = GridRef {
        nx: 3,
        ny: 2,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    assert_eq!(rect.point(2, 1), [3.0, 2.0]);
    assert_eq!(rect.map(1.5, 0.5), [2.0, 1.0]);
    assert_eq!(rect.map(2.0, 1.0), [3.0, 2.0]);

    let cx = [0.0, 2.0, 0.0, 3.0];
    let cy = [0.0, 0.0, 1.0, 2.0];
    let quad = GridRef {
        nx: 2,
        ny: 2,
        coords: Coords::Curvilinear { x: &cx, y: &cy },
        z: &z[..4],
    };
    assert_eq!(quad.point(1, 1), [3.0, 2.0]);
    let centre = quad.map(0.5, 0.5);
    assert_close(centre[0], 1.25, 1e-12);
    assert_close(centre[1], 0.75, 1e-12);
}

// Why: the defining property of an isoline of x² + y² = 1 is that it is the unit circle; one
// closed polyline enclosing area π proves correct interpolation and complete stitching.
#[test]
fn isoline_of_paraboloid_is_a_closed_circle() {
    let x = linspace(-2.0, 2.0, 41);
    let y = linspace(-2.0, 2.0, 41);
    let z = sample_field(&x, &y, |x, y| x * x + y * y);
    let grid = GridRef {
        nx: 41,
        ny: 41,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    let lines = isolines(&grid, 1.0);
    assert_eq!(lines.len(), 1);
    let circle = &lines[0];
    assert!(circle.closed);
    assert!(circle.points.len() >= 20);
    for [px, py] in &circle.points {
        assert_close(px.hypot(*py), 1.0, 5e-3);
    }
    assert_close(
        signed_area(&circle.points).abs(),
        std::f64::consts::PI,
        0.03,
    );
}

// Why: levels that hit node values exactly are common with integer data; they must not create
// duplicate vertices or break the loop into pieces.
#[test]
fn isoline_through_nodes_has_no_duplicate_vertices() {
    let x = linspace(-3.0, 3.0, 13);
    let y = linspace(-3.0, 3.0, 13);
    let z = sample_field(&x, &y, |x, y| x * x + y * y);
    let grid = GridRef {
        nx: 13,
        ny: 13,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    let lines = isolines(&grid, 1.0);
    assert_eq!(lines.len(), 1);
    let loop_ = &lines[0];
    assert!(loop_.closed);
    let n = loop_.points.len();
    for k in 0..n {
        let [ax, ay] = loop_.points[k];
        let [bx, by] = loop_.points[(k + 1) % n];
        assert!((ax - bx).hypot(ay - by) > 1e-12, "duplicate vertex at {k}");
        assert_close(ax.hypot(ay), 1.0, 0.1);
    }
}

// Why: a 2 × 2 grid (one cell) is the smallest valid input; an off-by-one in the cell count must
// not make it validate as empty or contour as nothing.
#[test]
fn single_cell_grid_is_contoured() {
    // z = x + y on the unit cell.
    let z = [0.0, 1.0, 1.0, 2.0];
    let grid = unit_cell_grid(&z);
    assert_eq!(grid.validate(), Ok(()));
    let lines = isolines(&grid, 0.5);
    assert_eq!(lines.len(), 1);
    assert!(is_segment(&lines[0], [0.5, 0.0], [0.0, 0.5]), "{lines:?}");
    let bands = isobands(&grid, 0.5, 1.5);
    assert!(bands.iter().all(|p| signed_area(p) > 0.0));
    let area: f64 = bands.iter().map(|p| signed_area(p)).sum();
    assert_close(area, 0.75, 1e-12);
}

// Why: user-chosen levels often equal the data maximum. A peak exactly at the level touches it at
// a single point, which must produce nothing rather than zero-length polylines that stroke as
// stray dots; just below the peak the same field gives one small closed loop.
#[test]
fn level_equal_to_isolated_peak_gives_no_isoline() {
    let x = indices(3);
    let y = indices(3);
    let mut z = [0.0; 9];
    z[4] = 1.0;
    let grid = index_grid(3, 3, &x, &y, &z);
    assert_eq!(isolines(&grid, 1.0), vec![]);
    let below = isolines(&grid, 0.9);
    assert_eq!(below.len(), 1);
    assert!(below[0].closed);
    assert_eq!(below[0].points.len(), 4);
}

// Why: a ridge whose crest lies exactly on the level makes both cells beside the crest emit the
// same segment along their shared grid edge. It must be drawn once, as one open line along the
// crest: a duplicate would double-stroke (darkening translucent or dashed lines), and stitching
// the duplicates would fold the line back on itself into a degenerate loop.
#[test]
fn ridge_exactly_on_the_level_gives_one_line_along_the_crest() {
    let x = indices(3);
    let y = indices(3);
    let z = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0];
    let grid = index_grid(3, 3, &x, &y, &z);
    let lines = isolines(&grid, 1.0);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(!lines[0].closed);
    let mut points = lines[0].points.clone();
    if points[0][0] > points[points.len() - 1][0] {
        points.reverse();
    }
    assert_eq!(points, vec![[0.0, 1.0], [1.0, 1.0], [2.0, 1.0]]);
}

// Why: a contour line crossing many cells must be one polyline, not one segment per cell, so it
// strokes with proper joins and dashes run continuously.
#[test]
fn open_isoline_is_stitched_into_one_polyline() {
    let x = indices(5);
    let y = indices(5);
    let z = sample_field(&x, &y, |x, _| x);
    let grid = GridRef {
        nx: 5,
        ny: 5,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    let lines = isolines(&grid, 1.5);
    assert_eq!(lines.len(), 1);
    assert!(!lines[0].closed);
    assert_eq!(lines[0].points.len(), 5);
    let mut ys: Vec<f64> = lines[0].points.iter().map(|p| p[1]).collect();
    if ys[0] > ys[4] {
        ys.reverse();
    }
    assert_eq!(ys, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    assert!(lines[0].points.iter().all(|p| p[0] == 1.5));
}

// Why: a curvilinear grid with rectilinear node positions is the same grid; any difference
// would mean the curvilinear path maps index space incorrectly.
#[test]
fn curvilinear_copy_of_rectilinear_grid_gives_identical_isolines() {
    let x = linspace(-2.0, 2.0, 21);
    let y = linspace(-1.5, 2.5, 17);
    let z = sample_field(&x, &y, |x, y| x * x + 2.0 * y * y - x * y);
    let rect = GridRef {
        nx: 21,
        ny: 17,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    let cx = sample_field(&x, &y, |x, _| x);
    let cy = sample_field(&x, &y, |_, y| y);
    let curv = GridRef {
        coords: Coords::Curvilinear { x: &cx, y: &cy },
        ..rect
    };
    let a = isolines(&rect, 1.7);
    let b = isolines(&curv, 1.7);
    assert!(!a.is_empty());
    assert_eq!(a.len(), b.len());
    for (la, lb) in a.iter().zip(&b) {
        assert_eq!(la.closed, lb.closed);
        assert_eq!(la.points.len(), lb.points.len());
        for (pa, pb) in la.points.iter().zip(&lb.points) {
            assert_close(pa[0], pb[0], 1e-12);
            assert_close(pa[1], pb[1], 1e-12);
        }
    }
}

// Why: saddle cells are ambiguous; the documented centre-mean rule must pick the topology, and
// the two resulting segments must never cross (a crossing contour is visibly wrong).
#[test]
fn saddle_with_high_centre_isolates_low_corners() {
    // Corners: (0,0) = 1, (1,0) = 0, (0,1) = 0, (1,1) = 1; centre mean 0.5 is above 0.4.
    let z = [1.0, 0.0, 0.0, 1.0];
    let lines = isolines(&unit_cell_grid(&z), 0.4);
    assert_eq!(lines.len(), 2);
    let around_1_0 = |l: &Polyline| is_segment(l, [0.6, 0.0], [1.0, 0.4]);
    let around_0_1 = |l: &Polyline| is_segment(l, [0.0, 0.6], [0.4, 1.0]);
    assert!(lines.iter().any(around_1_0), "{lines:?}");
    assert!(lines.iter().any(around_0_1), "{lines:?}");
    let seg = |l: &Polyline| [l.points[0], l.points[1]];
    assert!(!segments_intersect(seg(&lines[0]), seg(&lines[1])));
}

// Why: the opposite side of the same saddle must produce the opposite topology, proving the
// rule depends on the centre value rather than on a fixed case table.
#[test]
fn saddle_with_low_centre_isolates_high_corners() {
    let z = [1.0, 0.0, 0.0, 1.0];
    let lines = isolines(&unit_cell_grid(&z), 0.6);
    assert_eq!(lines.len(), 2);
    let around_0_0 = |l: &Polyline| is_segment(l, [0.4, 0.0], [0.0, 0.4]);
    let around_1_1 = |l: &Polyline| is_segment(l, [1.0, 0.6], [0.6, 1.0]);
    assert!(lines.iter().any(around_0_0), "{lines:?}");
    assert!(lines.iter().any(around_1_1), "{lines:?}");
    let seg = |l: &Polyline| [l.points[0], l.points[1]];
    assert!(!segments_intersect(seg(&lines[0]), seg(&lines[1])));
}

// Why: missing data must leave a hole; drawing through it would invent a contour where nothing
// is known.
#[test]
fn nan_region_breaks_isolines() {
    let (nx, ny) = (21, 21);
    let x = indices(nx);
    let y = indices(ny);
    let mut z = sample_field(&x, &y, |x, _| x);
    for j in 8..=12 {
        for i in 8..=12 {
            z[j * nx + i] = f64::NAN;
        }
    }
    let grid = GridRef {
        nx,
        ny,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    let lines = isolines(&grid, 10.5);
    assert_eq!(lines.len(), 2, "the line is cut in two by the hole");
    for line in &lines {
        assert!(!line.closed);
        for [px, py] in &line.points {
            assert_eq!(*px, 10.5);
            assert!(
                !(7.0 < *py && *py < 13.0),
                "point {py} lies inside the hole"
            );
        }
    }
}

// Why: levels outside the data, NaN levels and all-missing data are routine and must simply
// produce nothing.
#[test]
fn isolines_empty_when_level_does_not_cross_data() {
    let x = indices(4);
    let y = indices(3);
    let z = sample_field(&x, &y, |x, y| x + y);
    let grid = GridRef {
        nx: 4,
        ny: 3,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    assert!(isolines(&grid, 25.0).is_empty());
    assert!(isolines(&grid, -1.0).is_empty());
    assert!(isolines(&grid, f64::NAN).is_empty());
    let nans = [f64::NAN; 12];
    assert!(isolines(&GridRef { z: &nans, ..grid }, 1.0).is_empty());
}

fn wavy_grid_data(x_decreasing: bool) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut x: Vec<f64> = (0..30)
        .map(|i| -1.0 + 3.0 * (i as f64 / 29.0).powf(1.5))
        .collect();
    if x_decreasing {
        x.reverse();
    }
    let y = linspace(-1.0, 1.0, 20);
    let z = sample_field(&x, &y, wavy);
    (x, y, z)
}

// Why: filled bands must tile the plot area exactly, with no gaps (unpainted slivers) and no
// overlaps (double-painted seams); the areas of all bands must therefore sum to the domain area.
#[test]
fn isobands_partition_the_domain() {
    let (x, y, z) = wavy_grid_data(false);
    let grid = GridRef {
        nx: 30,
        ny: 20,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    let (zmin, zmax) = z_range(&z);
    let levels = auto_levels(zmin, zmax, 8);
    let bands = band_edges(&levels, zmin, zmax);
    assert_eq!(bands.len(), levels.len() + 1);
    let total: f64 = bands
        .iter()
        .flat_map(|&(lo, hi)| isobands(&grid, lo, hi))
        .map(|polygon| signed_area(&polygon).abs())
        .sum();
    let domain = 3.0 * 2.0;
    assert!(
        (total - domain).abs() <= 1e-9 * domain,
        "bands cover {total}, domain is {domain}"
    );
}

// Why: bands are filled with the nonzero rule as one path each; that only unions correctly when
// every piece winds the same way (counter-clockwise for increasing x and y).
#[test]
fn isoband_polygons_are_counter_clockwise_on_increasing_grid() {
    let (x, y, z) = wavy_grid_data(false);
    let grid = GridRef {
        nx: 30,
        ny: 20,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    let (zmin, zmax) = z_range(&z);
    for (lo, hi) in band_edges(&auto_levels(zmin, zmax, 8), zmin, zmax) {
        for polygon in isobands(&grid, lo, hi) {
            assert!(polygon.len() >= 3);
            assert!(
                signed_area(&polygon) > 1e-15,
                "band [{lo}, {hi}) has a clockwise piece"
            );
        }
    }
}

// Why: winding follows index space, so a mirrored grid yields uniformly clockwise pieces; what
// matters for nonzero filling is that the winding is consistent, never mixed.
#[test]
fn isoband_winding_is_consistent_on_mirrored_grid() {
    let (x, y, z) = wavy_grid_data(true);
    let grid = GridRef {
        nx: 30,
        ny: 20,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    let polygons = isobands(&grid, -0.2, 0.4);
    assert!(!polygons.is_empty());
    assert!(polygons.iter().all(|p| signed_area(p) < -1e-15));
}

// Why: a band polygon must not reach into regions of another band; every vertex it has must carry
// an interpolated value inside the band.
#[test]
fn isoband_vertices_lie_within_band() {
    let (nx, ny) = (12, 10);
    let x = indices(nx);
    let y = indices(ny);
    let z = sample_field(&x, &y, |x, y| wavy(x / 4.0 - 1.0, y / 5.0 - 1.0));
    let grid = index_grid(nx, ny, &x, &y, &z);
    let (lo, hi) = (-0.25, 0.35);
    let polygons = isobands(&grid, lo, hi);
    assert!(!polygons.is_empty());
    for p in polygons.iter().flatten() {
        let v = triangle_interpolant(&z, nx, ny, *p);
        assert!(v >= lo - 1e-9 && v <= hi + 1e-9, "vertex value {v}");
    }
}

// Why: what a reader sees is which colour covers each point. Under the nonzero rule a band paints
// a point exactly when the point's total winding number over that band's polygons is non-zero, so
// every point must have winding one in the band that contains its interpolated value and zero in
// every other band. This checks colour assignment, gaps and overlaps together, independently of
// how a band is cut into polygons.
#[test]
fn every_point_is_painted_once_by_the_band_containing_its_value() {
    let (nx, ny) = (12, 10);
    let x = indices(nx);
    let y = indices(ny);
    let z = sample_field(&x, &y, |x, y| wavy(x / 4.0 - 1.0, y / 5.0 - 1.0));
    let grid = index_grid(nx, ny, &x, &y, &z);
    let (zmin, zmax) = z_range(&z);
    let bands: Vec<(Band, Vec<Polygon>)> = band_edges(&auto_levels(zmin, zmax, 6), zmin, zmax)
        .into_iter()
        .map(|(lo, hi)| ((lo, hi), isobands(&grid, lo, hi)))
        .collect();
    assert!(bands.len() >= 4);
    // Offsets avoid grid lines and the cell diagonal, where piece boundaries lie.
    let offsets = [
        (0.13, 0.71),
        (0.62, 0.29),
        (0.37, 0.44),
        (0.81, 0.93),
        (0.52, 0.07),
    ];
    let mut checked = 0;
    for j in 0..ny - 1 {
        for i in 0..nx - 1 {
            for (du, dv) in offsets {
                let p = [i as f64 + du, j as f64 + dv];
                let v = triangle_interpolant(&z, nx, ny, p);
                let on_boundary = bands
                    .iter()
                    .any(|((lo, hi), _)| (v - lo).abs() < 1e-9 || (v - hi).abs() < 1e-9);
                if on_boundary {
                    continue;
                }
                for ((lo, hi), polygons) in &bands {
                    let winding: i32 = polygons.iter().map(|poly| winding_number(poly, p)).sum();
                    let expected = i32::from(*lo <= v && v < *hi);
                    assert_eq!(
                        winding, expected,
                        "point {p:?} with value {v}, band [{lo}, {hi})"
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(
        checked > 450,
        "only {checked} points were away from band boundaries"
    );
}

// Why: integer or clipped data put whole regions exactly on a level. The half-open rule [lo, hi)
// must give such a plateau to the band that starts at the level and to no other, or two bands
// overpaint it and its colour depends on drawing order. Clipping must also not leave zero-area
// slivers along the plateau edge.
#[test]
fn plateau_on_a_level_belongs_only_to_the_band_starting_there() {
    // The left cell is a plateau at z = 1; across the right cell z = 2 − x falls from 1 to 0.
    let x = indices(3);
    let y = indices(2);
    let z = [1.0, 1.0, 0.0, 1.0, 1.0, 0.0];
    let grid = index_grid(3, 2, &x, &y, &z);
    let area = |lo: f64, hi: f64| -> f64 {
        let polygons = isobands(&grid, lo, hi);
        assert!(
            polygons.iter().all(|p| signed_area(p) > 1e-15),
            "degenerate piece in [{lo}, {hi})"
        );
        polygons.iter().map(|p| signed_area(p)).sum()
    };
    assert_close(area(1.0, 1.5), 1.0, 1e-12);
    assert_close(area(1.0, f64::INFINITY), 1.0, 1e-12);
    assert_close(area(0.5, 1.0), 0.5, 1e-12);
    assert_close(area(f64::NEG_INFINITY, 1.0), 1.0, 1e-12);
}

// Why: filled contour plots are often overlaid with the isolines of the same levels. Both place
// crossings on grid edges by linear interpolation along the edge, so wherever a level crosses a
// grid edge the line and the band boundary must meet exactly. Inside saddle cells the two are
// allowed to differ (bands follow the triangle split, lines the centre mean), so this test
// deliberately asserts nothing about points inside cells.
#[test]
fn isoline_meets_band_boundary_on_every_crossed_grid_edge() {
    let (nx, ny) = (16, 16);
    let x = indices(nx);
    let y = indices(ny);
    let z = sample_field(&x, &y, |x, y| (0.7 * x).sin() * (0.9 * y).cos());
    let grid = index_grid(nx, ny, &x, &y, &z);
    let (level, next) = (0.05, 0.3);
    let at = |i: usize, j: usize| z[j * nx + i];

    let saddles = (0..ny - 1)
        .flat_map(|j| (0..nx - 1).map(move |i| (i, j)))
        .filter(|&(i, j)| {
            let above =
                [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)].map(|v| v >= level);
            above[0] == above[2] && above[1] == above[3] && above[0] != above[1]
        })
        .count();
    assert!(saddles > 0, "the field must exercise saddle cells");

    let line_vertices: Vec<[f64; 2]> = isolines(&grid, level)
        .into_iter()
        .flat_map(|l| l.points)
        .collect();
    let band_vertices: Vec<[f64; 2]> = isobands(&grid, level, next).into_iter().flatten().collect();
    let has = |vertices: &[[f64; 2]], q: [f64; 2]| {
        vertices
            .iter()
            .any(|v| (v[0] - q[0]).abs() < 1e-12 && (v[1] - q[1]).abs() < 1e-12)
    };
    let mut crossings = 0;
    for j in 0..ny {
        for i in 0..nx {
            for (di, dj) in [(1, 0), (0, 1)] {
                if i + di >= nx || j + dj >= ny {
                    continue;
                }
                let (za, zb) = (at(i, j), at(i + di, j + dj));
                if (za - level) * (zb - level) >= 0.0 {
                    continue;
                }
                let t = (level - za) / (zb - za);
                let q = [i as f64 + t * di as f64, j as f64 + t * dj as f64];
                assert!(has(&line_vertices, q), "no isoline vertex at {q:?}");
                assert!(has(&band_vertices, q), "no band vertex at {q:?}");
                crossings += 1;
            }
        }
    }
    assert!(crossings > 100);
}

// Why: missing data must remove exactly the triangles that touch it, not whole neighbourhoods,
// so that filled contours reach as close to the hole as the data allow.
#[test]
fn isobands_skip_only_triangles_with_nan_corners() {
    let (nx, ny) = (5, 5);
    let x = indices(nx);
    let y = indices(ny);
    let mut z = sample_field(&x, &y, |x, y| x + y);
    z[2 * nx + 2] = f64::NAN;
    let grid = GridRef {
        nx,
        ny,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    let area: f64 = isobands(&grid, f64::NEG_INFINITY, f64::INFINITY)
        .iter()
        .map(|p| signed_area(p))
        .sum();
    // Six of the eight triangles around node (2, 2) contain it: 16 − 3 = 13.
    assert_close(area, 13.0, 1e-12);
}

// Why: an empty or inverted band is a caller error that must yield nothing rather than inverted
// geometry.
#[test]
fn isobands_empty_for_invalid_bounds() {
    let x = indices(3);
    let y = indices(3);
    let z = sample_field(&x, &y, |x, y| x + y);
    let grid = GridRef {
        nx: 3,
        ny: 3,
        coords: Coords::Rectilinear { x: &x, y: &y },
        z: &z,
    };
    assert!(isobands(&grid, 2.0, 2.0).is_empty());
    assert!(isobands(&grid, 3.0, 1.0).is_empty());
    assert!(isobands(&grid, f64::NAN, 1.0).is_empty());
    assert!(isobands(&grid, 10.0, 20.0).is_empty());
}

// Why: automatic contour levels must be nice, evenly spaced, and strictly inside the data so
// that no level produces a degenerate contour at the data extremes.
#[test]
fn auto_levels_are_nice_and_strictly_inside() {
    assert_eq!(auto_levels(0.0, 1.0, 5), vec![0.2, 0.4, 0.6, 0.8]);

    let levels = auto_levels(-1.3, 2.7, 8);
    assert!(levels.len() >= 2 && levels.len() <= 9);
    assert!(levels.iter().all(|l| -1.3 < *l && *l < 2.7));
    let step = levels[1] - levels[0];
    assert!(is_nice_step(step));
    for w in levels.windows(2) {
        assert_close(w[1] - w[0], step, 1e-12);
    }
}

// Why: computed data often reach a level by rounding residue only (sin(π) ≈ 1.2e−16 below zero);
// a level that close to the data extreme still draws a degenerate contour around the extreme
// nodes, so it must be excluded exactly as a level equal to the extreme is.
#[test]
fn auto_levels_exclude_levels_at_extremes_up_to_rounding() {
    assert_eq!(auto_levels(-1e-16, 1.0, 5), vec![0.2, 0.4, 0.6, 0.8]);
    assert_eq!(auto_levels(0.0, 1.0 + 1e-16, 5), vec![0.2, 0.4, 0.6, 0.8]);
}

// Why: constant or missing data have no contour levels.
#[test]
fn auto_levels_empty_for_degenerate_ranges() {
    assert!(auto_levels(1.0, 1.0, 5).is_empty());
    assert!(auto_levels(2.0, 1.0, 5).is_empty());
    assert!(auto_levels(f64::NAN, 1.0, 5).is_empty());
}

// Why: filled contours paint below the first level and above the last level too; infinite outer
// edges guarantee no data value falls outside every band.
#[test]
fn band_edges_include_open_outer_bands() {
    let inf = f64::INFINITY;
    assert_eq!(
        band_edges(&[0.0, 1.0, 2.0], -0.5, 2.5),
        vec![(-inf, 0.0), (0.0, 1.0), (1.0, 2.0), (2.0, inf)]
    );
    assert_eq!(band_edges(&[], -0.5, 2.5), vec![(-inf, inf)]);
}

// Why: bands that cannot contain data would add empty paths and phantom legend or colour bar
// entries; half-open membership decides the boundary cases.
#[test]
fn band_edges_drop_bands_outside_data_range() {
    let inf = f64::INFINITY;
    assert_eq!(
        band_edges(&[0.0, 1.0, 2.0], 0.2, 1.5),
        vec![(0.0, 1.0), (1.0, 2.0)]
    );
    assert_eq!(
        band_edges(&[0.0, 1.0, 2.0], 0.0, 2.0),
        vec![(0.0, 1.0), (1.0, 2.0), (2.0, inf)]
    );
}

// Why: user-supplied levels may be unsorted, repeated or contain NaN; bands must still be a
// clean ascending partition.
#[test]
fn band_edges_normalise_levels() {
    assert_eq!(
        band_edges(&[2.0, f64::NAN, 0.0, 1.0, 1.0], -0.5, 2.5),
        band_edges(&[0.0, 1.0, 2.0], -0.5, 2.5)
    );
}

proptest! {
    // Why: rough integer-valued data put many nodes exactly on the levels, the hardest case for
    // clipping; the bands must still tile the domain exactly and wind consistently.
    #[test]
    fn isobands_partition_rough_fields_with_values_on_levels(
        (nx, ny, z) in (2usize..7, 2usize..7).prop_flat_map(|(nx, ny)| {
            (Just(nx), Just(ny), proptest::collection::vec(0u8..4, nx * ny))
        })
    ) {
        let z: Vec<f64> = z.into_iter().map(f64::from).collect();
        let x = indices(nx);
        let y = indices(ny);
        let grid = GridRef { nx, ny, coords: Coords::Rectilinear { x: &x, y: &y }, z: &z };
        let (zmin, zmax) = z_range(&z);
        let mut total = 0.0;
        for (lo, hi) in band_edges(&[1.0, 2.0], zmin, zmax) {
            for polygon in isobands(&grid, lo, hi) {
                let area = signed_area(&polygon);
                prop_assert!(area > 0.0);
                total += area;
            }
        }
        let domain = ((nx - 1) * (ny - 1)) as f64;
        prop_assert!((total - domain).abs() <= 1e-9 * domain);
    }
}
