//! Isolines and filled isobands of scalar fields sampled on structured grids.
//!
//! # Grid layout
//!
//! A grid has `nx` columns and `ny` rows. Node `(i, j)` has column index `i` in `0..nx` and row
//! index `j` in `0..ny`, and its value is `z[j * nx + i]` (row-major, with `j` the row, or y,
//! index). Cell `(i, j)` is the quadrilateral with corners `(i, j)`, `(i + 1, j)`,
//! `(i + 1, j + 1)` and `(i, j + 1)`, which is counter-clockwise in index space.
//!
//! # Method
//!
//! All extraction happens in index space, where every cell is a unit square, and the results
//! are mapped to physical coordinates through [`GridRef::map`] afterwards. Working in index
//! space makes the algorithms independent of whether the grid is rectilinear or curvilinear.
//!
//! A node is "above" a level when `z >= level` and "below" otherwise. Any cell with a NaN corner
//! is skipped, which leaves a hole in the output rather than inventing values.

use std::collections::{HashMap, HashSet};

use thiserror::Error;

/// Physical coordinates of the grid nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Coords<'a> {
    /// Axis-aligned grid: node `(i, j)` is at `(x[i], y[j])`. Requires `x.len() == nx` and
    /// `y.len() == ny`. The coordinates may be non-uniform and may decrease.
    Rectilinear { x: &'a [f64], y: &'a [f64] },
    /// General structured grid: node `(i, j)` is at `(x[j * nx + i], y[j * nx + i])`. Requires
    /// `x.len() == y.len() == nx * ny`.
    Curvilinear { x: &'a [f64], y: &'a [f64] },
}

/// A borrowed view of a scalar field on a structured grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridRef<'a> {
    /// Number of columns (nodes along the `i` direction).
    pub nx: usize,
    /// Number of rows (nodes along the `j` direction).
    pub ny: usize,
    /// Physical node coordinates.
    pub coords: Coords<'a>,
    /// Node values in row-major order, `z[j * nx + i]`. NaN marks a missing value.
    pub z: &'a [f64],
}

/// Reasons a [`GridRef`] cannot be contoured.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GridShapeError {
    /// The grid has fewer than two nodes along some direction, so it has no cells.
    #[error("grid of {nx} × {ny} nodes has no cells; both dimensions must be at least 2")]
    TooSmall { nx: usize, ny: usize },
    /// `z` does not hold `nx * ny` values.
    #[error("z holds {actual} values but the grid needs {expected}")]
    ValuesLength { expected: usize, actual: usize },
    /// A coordinate array has the wrong length for the grid and coordinate kind.
    #[error("{axis} coordinates hold {actual} values but the grid needs {expected}")]
    CoordsLength {
        axis: char,
        expected: usize,
        actual: usize,
    },
}

impl GridRef<'_> {
    /// Checks that the grid has at least one cell and that every array has the right length.
    ///
    /// The checks run in the order the [`GridShapeError`] variants are declared (cell count, then
    /// `z`, then `x`, then `y`), and the first failure is returned.
    ///
    /// [`isolines`] and [`isobands`] return no geometry for a grid that fails this check, so the
    /// scene compiler calls it first to report the problem as a warning.
    pub fn validate(&self) -> Result<(), GridShapeError> {
        let (nx, ny) = (self.nx, self.ny);
        if nx < 2 || ny < 2 {
            return Err(GridShapeError::TooSmall { nx, ny });
        }
        let nodes = nx.saturating_mul(ny);
        if self.z.len() != nodes {
            return Err(GridShapeError::ValuesLength {
                expected: nodes,
                actual: self.z.len(),
            });
        }
        let (x, y, x_expected, y_expected) = match self.coords {
            Coords::Rectilinear { x, y } => (x, y, nx, ny),
            Coords::Curvilinear { x, y } => (x, y, nodes, nodes),
        };
        for (axis, values, expected) in [('x', x, x_expected), ('y', y, y_expected)] {
            if values.len() != expected {
                return Err(GridShapeError::CoordsLength {
                    axis,
                    expected,
                    actual: values.len(),
                });
            }
        }
        Ok(())
    }

    /// Returns the physical position of node `(i, j)`.
    ///
    /// Panics if the node is outside the grid or the arrays are shorter than the grid requires.
    pub fn point(&self, i: usize, j: usize) -> [f64; 2] {
        assert!(
            i < self.nx && j < self.ny,
            "node ({i}, {j}) is outside the grid"
        );
        match self.coords {
            Coords::Rectilinear { x, y } => [x[i], y[j]],
            Coords::Curvilinear { x, y } => [x[j * self.nx + i], y[j * self.nx + i]],
        }
    }

    /// Maps a fractional index-space position to physical coordinates.
    ///
    /// The position lies in cell `(floor(fi), floor(fj))`, with the cell index clamped to the
    /// last cell along each direction (so the far boundary `fi == nx - 1` belongs to the last
    /// cell). Within the cell the physical position is the bilinear interpolation of the four
    /// corner positions; for a rectilinear grid this reduces to independent linear interpolation
    /// along each axis. Positions outside the grid are extrapolated from the nearest cell.
    ///
    /// Requires a grid that passes [`GridRef::validate`].
    pub fn map(&self, fi: f64, fj: f64) -> [f64; 2] {
        // The saturating cast sends NaN and negative positions to cell 0.
        let ci = (fi.floor() as usize).min(self.nx - 2);
        let cj = (fj.floor() as usize).min(self.ny - 2);
        let (u, v) = (fi - ci as f64, fj - cj as f64);
        let lerp = |a: f64, b: f64, t: f64| a + t * (b - a);
        match self.coords {
            Coords::Rectilinear { x, y } => [lerp(x[ci], x[ci + 1], u), lerp(y[cj], y[cj + 1], v)],
            Coords::Curvilinear { .. } => {
                let (p00, p10) = (self.point(ci, cj), self.point(ci + 1, cj));
                let (p01, p11) = (self.point(ci, cj + 1), self.point(ci + 1, cj + 1));
                std::array::from_fn(|axis| {
                    let bottom = lerp(p00[axis], p10[axis], u);
                    let top = lerp(p01[axis], p11[axis], u);
                    lerp(bottom, top, v)
                })
            }
        }
    }

    /// The value at node `(i, j)`.
    fn value(&self, i: usize, j: usize) -> f64 {
        self.z[j * self.nx + i]
    }
}

/// A connected piece of an isoline in physical coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Polyline {
    /// The vertices in order. Consecutive vertices are distinct, an open polyline has at least two
    /// vertices and a closed polyline at least three.
    pub points: Vec<[f64; 2]>,
    /// Whether the line returns to its start. A closed polyline does not repeat its first
    /// vertex at the end; the closing segment runs from the last vertex to the first.
    pub closed: bool,
}

/// Extracts the isoline `z == level` with marching squares.
///
/// Within each cell, crossings are placed on the cell edges by linear interpolation between the
/// two edge nodes. A saddle cell (diagonally opposite corners on the same side of the level) is
/// resolved using the mean of its four corner values as the value at the cell centre: if the
/// centre is above the level, the two above corners are connected through the cell and the
/// segments cut off the two below corners, and vice versa. Segments from all cells are stitched
/// through their shared edges into maximal polylines, so a line that runs through many cells is
/// one polyline, and a line that returns to its start is one closed polyline.
///
/// When a crossing falls exactly on a node, both edges that meet there give the same point, and
/// a segment whose two ends coincide is discarded. A level equal to an isolated peak value
/// therefore produces no polyline rather than a degenerate one.
///
/// Returns no polylines if the grid fails [`GridRef::validate`], if `level` is not finite, or if
/// the level does not cross the data.
pub fn isolines(grid: &GridRef, level: f64) -> Vec<Polyline> {
    if grid.validate().is_err() || !level.is_finite() {
        return Vec::new();
    }
    let segments = cell_segments(grid, level);
    stitch(&segments)
        .into_iter()
        .filter_map(|(ids, closed)| {
            let points = ids.iter().map(|id| id.position(grid, level)).collect();
            polyline(grid, points, closed)
        })
        .collect()
}

/// Identifies a vertex of an isoline so that cells sharing it agree without comparing floats.
///
/// A crossing that falls exactly on a node is identified by the node, because every edge that
/// meets at that node produces the same point; any other crossing is identified by its edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum VertexId {
    /// Node `(i, j)`.
    Node(usize, usize),
    /// The edge from node `(i, j)` to node `(i + 1, j)`.
    Horizontal(usize, usize),
    /// The edge from node `(i, j)` to node `(i, j + 1)`.
    Vertical(usize, usize),
}

impl VertexId {
    /// The crossing on the edge from `a` to `b`, where exactly one of the two nodes is above the
    /// level; `a` is the lower-index node of a horizontal or vertical edge.
    fn crossing(grid: &GridRef, a: (usize, usize), b: (usize, usize), level: f64) -> Self {
        if grid.value(a.0, a.1) == level {
            VertexId::Node(a.0, a.1)
        } else if grid.value(b.0, b.1) == level {
            VertexId::Node(b.0, b.1)
        } else if b.1 == a.1 {
            VertexId::Horizontal(a.0, a.1)
        } else {
            VertexId::Vertical(a.0, a.1)
        }
    }

    /// The index-space position of the vertex.
    fn position(self, grid: &GridRef, level: f64) -> [f64; 2] {
        match self {
            VertexId::Node(i, j) => [i as f64, j as f64],
            VertexId::Horizontal(i, j) => edge_crossing(grid, (i, j), (i + 1, j), level),
            VertexId::Vertical(i, j) => edge_crossing(grid, (i, j), (i, j + 1), level),
        }
    }
}

/// The index-space point where the linear interpolant along the edge from node `a` to node `b`
/// equals `level`.
///
/// Every caller passes the nodes in the same canonical order (the node with the smaller `i + j`
/// first), so isolines, and the band polygons of both triangles or cells sharing an edge, all
/// compute a crossing on that edge with bit-identical arithmetic.
fn edge_crossing(grid: &GridRef, a: (usize, usize), b: (usize, usize), level: f64) -> [f64; 2] {
    let (za, zb) = (grid.value(a.0, a.1), grid.value(b.0, b.1));
    if za == level {
        return [a.0 as f64, a.1 as f64];
    }
    if zb == level {
        return [b.0 as f64, b.1 as f64];
    }
    let t = (level - za) / (zb - za);
    [
        a.0 as f64 + t * (b.0 as f64 - a.0 as f64),
        a.1 as f64 + t * (b.1 as f64 - a.1 as f64),
    ]
}

/// Runs marching squares over every cell without a NaN corner and returns the non-degenerate
/// segments, each unordered pair of vertices at most once, in row-major cell order.
fn cell_segments(grid: &GridRef, level: f64) -> Vec<[VertexId; 2]> {
    let mut segments = Vec::new();
    let mut seen = HashSet::new();
    for j in 0..grid.ny - 1 {
        for i in 0..grid.nx - 1 {
            let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            let z = corners.map(|(ci, cj)| grid.value(ci, cj));
            if z.iter().any(|v| v.is_nan()) {
                continue;
            }
            let above = z.map(|v| v >= level);
            // Edge k joins corners k and k + 1, but crossings are always computed from the
            // canonical lower-index node: edges 2 and 3 run from corner 3 and corner 0.
            let edge = |k: usize| -> Option<VertexId> {
                let (a, b) = match k {
                    0 => (corners[0], corners[1]),
                    1 => (corners[1], corners[2]),
                    2 => (corners[3], corners[2]),
                    _ => (corners[0], corners[3]),
                };
                (above[k] != above[(k + 1) % 4]).then(|| VertexId::crossing(grid, a, b, level))
            };
            let crossed: Vec<(usize, VertexId)> =
                (0..4).filter_map(|k| edge(k).map(|id| (k, id))).collect();
            let mut emit = |p: VertexId, q: VertexId| {
                let key = if p <= q { (p, q) } else { (q, p) };
                if p != q && seen.insert(key) {
                    segments.push([p, q]);
                }
            };
            match crossed.as_slice() {
                [(_, p), (_, q)] => emit(*p, *q),
                [(_, e0), (_, e1), (_, e2), (_, e3)] => {
                    // A saddle: the centre mean decides which pair of opposite corners the
                    // segments cut off. Corner k lies between edges k − 1 and k.
                    let centre_above = z.iter().sum::<f64>() / 4.0 >= level;
                    if above[0] == centre_above {
                        emit(*e0, *e1);
                        emit(*e2, *e3);
                    } else {
                        emit(*e3, *e0);
                        emit(*e1, *e2);
                    }
                }
                _ => {}
            }
        }
    }
    segments
}

/// Joins segments that share vertices into maximal chains.
///
/// A chain passes through a vertex only when exactly two segments meet there; at any other vertex
/// (an end, or a node where several lines meet) chains stop. A chain that returns to its starting
/// vertex is closed and does not repeat that vertex. The output order depends only on the order
/// of `segments`.
fn stitch(segments: &[[VertexId; 2]]) -> Vec<(Vec<VertexId>, bool)> {
    let mut incident: HashMap<VertexId, Vec<usize>> = HashMap::new();
    for (s, ends) in segments.iter().enumerate() {
        for end in ends {
            incident.entry(*end).or_default().push(s);
        }
    }
    let mut used = vec![false; segments.len()];
    let mut chains = Vec::new();

    let walk = |start: VertexId, first: usize, used: &mut [bool]| {
        let mut ids = vec![start];
        let (mut at, mut via) = (start, first);
        loop {
            used[via] = true;
            let [p, q] = segments[via];
            at = if p == at { q } else { p };
            if at == start {
                return (ids, true);
            }
            ids.push(at);
            let next = match incident[&at].as_slice() {
                [a, b] => Some(if *a == via { *b } else { *a }),
                _ => None,
            };
            match next {
                Some(s) if !used[s] => via = s,
                _ => return (ids, false),
            }
        }
    };

    // Open chains start at vertices where the line cannot continue unambiguously.
    for s in 0..segments.len() {
        for end in segments[s] {
            if !used[s] && incident[&end].len() != 2 {
                chains.push(walk(end, s, &mut used));
            }
        }
    }
    // Every remaining segment lies on a cycle through vertices of degree two.
    for s in 0..segments.len() {
        if !used[s] {
            chains.push(walk(segments[s][0], s, &mut used));
        }
    }
    chains
}

/// Maps index-space vertices to physical coordinates, removes consecutive duplicates (which
/// arise when physical node positions coincide) and returns the polyline if it is not degenerate.
fn polyline(grid: &GridRef, points: Vec<[f64; 2]>, closed: bool) -> Option<Polyline> {
    let mut mapped: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for [fi, fj] in points {
        let p = grid.map(fi, fj);
        if mapped.last() != Some(&p) {
            mapped.push(p);
        }
    }
    if closed && mapped.len() > 1 && mapped.first() == mapped.last() {
        mapped.pop();
    }
    match (closed, mapped.len()) {
        (true, n) if n >= 3 => Some(Polyline {
            points: mapped,
            closed: true,
        }),
        (_, n) if n >= 2 => Some(Polyline {
            points: mapped,
            closed: false,
        }),
        _ => None,
    }
}

/// Extracts the filled region `lo <= z < hi` as a set of polygons.
///
/// Each cell is split along its `(i, j)`–`(i + 1, j + 1)` diagonal into the two index-space
/// counter-clockwise triangles `[(i, j), (i + 1, j), (i + 1, j + 1)]` and
/// `[(i, j), (i + 1, j + 1), (i, j + 1)]`. The field is taken to be linear on each triangle, and
/// each triangle is clipped against the half-spaces `z >= lo` and `z <= hi` with the
/// Sutherland–Hodgman algorithm, placing new vertices by linear interpolation of `z` along the
/// clipped edge. An infinite bound clips nothing, so `(-inf, inf)` returns the whole grid.
///
/// Clipping with `z <= hi` keeps the zero-area boundary `z == hi`, which is harmless, but it also
/// keeps a plateau: a triangle whose three corners all equal `hi` exactly. Such a triangle
/// belongs to the band that starts at `hi`, so it is excluded here; because the field is linear
/// on each triangle, this is the only way a region of non-zero area can have `z == hi`. A
/// triangle whose corners all equal `lo` is kept. Adjacent bands therefore never both paint a
/// plateau.
///
/// Where the boundary `z == lo` or `z == hi` crosses a grid edge (as opposed to a cell diagonal),
/// the crossing is a polygon vertex placed by the same linear interpolation along the edge that
/// [`isolines`] uses, so an isoline drawn over the filled bands meets the band boundaries exactly
/// on every grid edge. Inside a cell the two may differ slightly, because the bands follow the
/// triangle split while the isolines resolve saddles with the centre mean.
///
/// Every returned polygon has at least three vertices, a non-zero area, no repeated closing
/// vertex, and the same winding as the index-space counter-clockwise orientation. After mapping
/// to physical coordinates the polygons therefore all share one winding (counter-clockwise for a
/// grid whose x and y increase with `i` and `j`), so a renderer can fill all the polygons of one
/// band as a single path with the nonzero rule, and adjacent pieces join without seams.
///
/// Triangles with a NaN corner are skipped. Returns no polygons if the grid fails
/// [`GridRef::validate`], if either bound is NaN, or if `lo >= hi`.
pub fn isobands(grid: &GridRef, lo: f64, hi: f64) -> Vec<Vec<[f64; 2]>> {
    if grid.validate().is_err() || lo.is_nan() || hi.is_nan() || lo >= hi {
        return Vec::new();
    }
    let mut polygons = Vec::new();
    for j in 0..grid.ny - 1 {
        for i in 0..grid.nx - 1 {
            for triangle in [
                [(i, j), (i + 1, j), (i + 1, j + 1)],
                [(i, j), (i + 1, j + 1), (i, j + 1)],
            ] {
                let z = triangle.map(|(ti, tj)| grid.value(ti, tj));
                if z.iter().any(|v| v.is_nan()) || z.iter().all(|v| *v == hi) {
                    continue;
                }
                let mut polygon: Vec<ClipVertex> = (0..3)
                    .map(|k| ClipVertex {
                        position: [triangle[k].0 as f64, triangle[k].1 as f64],
                        z: z[k],
                        on: Location::Corner(k),
                    })
                    .collect();
                if lo > f64::NEG_INFINITY {
                    polygon = clip(grid, &triangle, &polygon, lo, |v| v >= lo);
                }
                if hi < f64::INFINITY {
                    polygon = clip(grid, &triangle, &polygon, hi, |v| v <= hi);
                }
                if let Some(piece) = band_piece(grid, &polygon) {
                    polygons.push(piece);
                }
            }
        }
    }
    polygons
}

/// Where a clipped polygon vertex lies on its triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Location {
    /// At triangle corner `k`.
    Corner(usize),
    /// Strictly inside the triangle edge joining corners `a` and `b`.
    Edge(usize, usize),
    /// Anywhere else; only chords cut by an earlier clip end at such points.
    Interior,
}

/// A vertex of a triangle being clipped, in index space.
#[derive(Debug, Clone, Copy)]
struct ClipVertex {
    position: [f64; 2],
    z: f64,
    on: Location,
}

/// Returns the triangle edge (as a pair of corner indices) that contains both vertices, if any.
fn common_edge(p: Location, q: Location) -> Option<(usize, usize)> {
    use Location::{Corner, Edge};
    let joins = |a: usize, b: usize, x: usize, y: usize| (a == x && b == y) || (a == y && b == x);
    match (p, q) {
        (Corner(a), Corner(b)) if a != b => Some((a, b)),
        (Corner(c), Edge(a, b)) | (Edge(a, b), Corner(c)) if c == a || c == b => Some((a, b)),
        (Edge(a, b), Edge(x, y)) if joins(a, b, x, y) => Some((a, b)),
        _ => None,
    }
}

/// Clips a convex polygon lying in `triangle` against the half-plane where `inside(z)` holds,
/// whose boundary is `z == level` (Sutherland–Hodgman).
///
/// A new vertex on a triangle edge is placed by interpolating along the whole edge from its
/// canonical first node, exactly as [`isolines`] does on grid edges. Because `z` is linear along
/// the edge, this is the same point as interpolating along the clipped sub-segment, but the
/// arithmetic is shared with every other polygon and isoline that meets the edge.
fn clip(
    grid: &GridRef,
    triangle: &[(usize, usize); 3],
    polygon: &[ClipVertex],
    level: f64,
    inside: impl Fn(f64) -> bool,
) -> Vec<ClipVertex> {
    let mut out = Vec::with_capacity(polygon.len() + 2);
    for k in 0..polygon.len() {
        let p = polygon[k];
        let q = polygon[(k + 1) % polygon.len()];
        let (p_in, q_in) = (inside(p.z), inside(q.z));
        if p_in != q_in {
            out.push(intersection(grid, triangle, p, q, level));
        }
        if q_in {
            out.push(q);
        }
    }
    out
}

/// The point on the polygon edge from `p` to `q` where `z == level`.
fn intersection(
    grid: &GridRef,
    triangle: &[(usize, usize); 3],
    p: ClipVertex,
    q: ClipVertex,
    level: f64,
) -> ClipVertex {
    let Some((a, b)) = common_edge(p.on, q.on) else {
        // A chord from an earlier clip has constant z, so a later clip cannot cross it in
        // exact arithmetic; interpolate locally in case rounding says otherwise.
        let t = (level - p.z) / (q.z - p.z);
        return ClipVertex {
            position: std::array::from_fn(|c| p.position[c] + t * (q.position[c] - p.position[c])),
            z: level,
            on: Location::Interior,
        };
    };
    // Canonical order: the node with the smaller i + j first.
    let (a, b) = if triangle[a].0 + triangle[a].1 <= triangle[b].0 + triangle[b].1 {
        (a, b)
    } else {
        (b, a)
    };
    let position = edge_crossing(grid, triangle[a], triangle[b], level);
    let on = if position == [triangle[a].0 as f64, triangle[a].1 as f64] {
        Location::Corner(a)
    } else if position == [triangle[b].0 as f64, triangle[b].1 as f64] {
        Location::Corner(b)
    } else {
        Location::Edge(a, b)
    };
    ClipVertex {
        position,
        z: level,
        on,
    }
}

/// Maps a clipped polygon to physical coordinates, dropping repeated vertices, and returns it if
/// it still has at least three vertices and a non-zero area.
fn band_piece(grid: &GridRef, polygon: &[ClipVertex]) -> Option<Vec<[f64; 2]>> {
    /// Pieces smaller than this fraction of a cell in index space are rounding artefacts of a
    /// boundary that touches a level, not regions to paint.
    const MIN_INDEX_AREA: f64 = 1e-12;

    let mut index_points: Vec<[f64; 2]> = Vec::with_capacity(polygon.len());
    for v in polygon {
        if index_points.last() != Some(&v.position) {
            index_points.push(v.position);
        }
    }
    while index_points.len() > 1 && index_points.first() == index_points.last() {
        index_points.pop();
    }
    if index_points.len() < 3 || shoelace(&index_points) <= MIN_INDEX_AREA {
        return None;
    }

    let mut mapped: Vec<[f64; 2]> = Vec::with_capacity(index_points.len());
    for [fi, fj] in index_points {
        let p = grid.map(fi, fj);
        if mapped.last() != Some(&p) {
            mapped.push(p);
        }
    }
    while mapped.len() > 1 && mapped.first() == mapped.last() {
        mapped.pop();
    }
    (mapped.len() >= 3 && shoelace(&mapped) != 0.0).then_some(mapped)
}

/// Signed area of a closed polygon, positive for counter-clockwise vertices.
fn shoelace(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|k| {
            let ([x0, y0], [x1, y1]) = (points[k], points[(k + 1) % n]);
            x0 * y1 - x1 * y0
        })
        .sum::<f64>()
        / 2.0
}

/// Chooses about `count` nice contour levels strictly inside `(zmin, zmax)`.
///
/// The levels are the major ticks of [`super::ticks::linear_ticks`] over `[zmin, zmax]` with
/// `count` as the target, excluding any tick equal to `zmin` or `zmax` (a level at the data
/// extreme would draw a degenerate contour). They are therefore equally spaced with a step of
/// 1, 2 or 5 times a power of ten. Returns no levels if either bound is not finite or
/// `zmin >= zmax`.
pub fn auto_levels(zmin: f64, zmax: f64, count: usize) -> Vec<f64> {
    if !(zmin.is_finite() && zmax.is_finite() && zmin < zmax) {
        return Vec::new();
    }
    let margin = 1e-9 * (zmax - zmin);
    super::ticks::linear_ticks(zmin, zmax, count)
        .major
        .into_iter()
        .filter(|level| *level > zmin + margin && *level < zmax - margin)
        .collect()
}

/// Builds the half-open bands `[lo, hi)` that a filled contour plot paints.
///
/// The non-finite values in `levels` are discarded and the rest are sorted and de-duplicated.
/// Each returned pair `(lo, hi)` denotes the half-open band `[lo, hi)`. For sorted levels
/// `l0 < l1 < … < ln`, the candidate bands are `[-inf, l0)`, `[l0, l1)`, …, `[ln, +inf)`. The outer edges are infinite so that the bands partition the whole real line:
/// every value falls in exactly one band, whatever rounding occurred when `zmin` and `zmax` were
/// computed. A candidate band is kept only if it can contain a value of the data range
/// `[zmin, zmax]`, that is, if `lo <= zmax` and `hi > zmin`. With no finite levels the result is
/// the single band `(-inf, +inf)`.
pub fn band_edges(levels: &[f64], zmin: f64, zmax: f64) -> Vec<(f64, f64)> {
    let mut finite: Vec<f64> = levels.iter().copied().filter(|l| l.is_finite()).collect();
    if finite.is_empty() {
        return vec![(f64::NEG_INFINITY, f64::INFINITY)];
    }
    finite.sort_by(f64::total_cmp);
    // Treat −0 and +0 as one level.
    finite.dedup_by(|a, b| a == b);
    std::iter::once(f64::NEG_INFINITY)
        .chain(finite.iter().copied())
        .zip(finite.iter().copied().chain(std::iter::once(f64::INFINITY)))
        .filter(|&(lo, hi)| lo <= zmax && hi > zmin)
        .collect()
}
