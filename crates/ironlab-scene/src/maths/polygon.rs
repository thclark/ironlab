//! Geometry of polygons in figure space.

use crate::display::Point;

/// Returns the polygon whose edges are those of a strictly convex polygon moved inwards by `distance`, each vertex of
/// the result the meeting point of the two moved edges at the vertex of the argument in the same position.
///
/// Returns `None` when `distance` is not finite and positive, when the polygon is not strictly convex (fewer than
/// three vertices, a repeated vertex, a straight or a reflex corner, or an outline that winds more than once), or
/// when the polygon is too small for the distance, so that the moved edges would cross and the inset fold over
/// itself.
#[must_use]
pub fn inset(points: &[Point], distance: f64) -> Option<Vec<Point>> {
    let n = points.len();
    if n < 3
        || !(distance.is_finite() && distance > 0.0)
        || !points.iter().all(|p| p.x.is_finite() && p.y.is_finite())
    {
        return None;
    }
    let edge = |i: usize| {
        let (a, b) = (points[i], points[(i + 1) % n]);
        (b.x - a.x, b.y - a.y)
    };
    let cross = |(ax, ay): (f64, f64), (bx, by): (f64, f64)| ax * by - ay * bx;
    let area = signed_area(points);
    if !area.is_finite() || area == 0.0 {
        return None;
    }
    let orientation = area.signum();
    // Every corner turns the same way, and the corners turn one full circle between them, or the outline is not
    // a simple convex polygon.
    let mut turning = 0.0;
    for i in 0..n {
        let (before, after) = (edge((i + n - 1) % n), edge(i));
        let turn = cross(before, after);
        if !turn.is_finite() || turn == 0.0 || turn.signum() != orientation {
            return None;
        }
        let dot = before.0 * after.0 + before.1 * after.1;
        turning += turn.atan2(dot);
    }
    if (turning.abs() - std::f64::consts::TAU).abs() > 1e-9 {
        return None;
    }
    // Each edge moved inwards by the distance: a point on the moved edge, its direction and its inward normal.
    let moved: Vec<MovedEdge> = (0..n)
        .map(|i| {
            let (dx, dy) = edge(i);
            let length = dx.hypot(dy);
            let normal = (-dy / length * orientation, dx / length * orientation);
            MovedEdge {
                origin: Point::new(
                    points[i].x + normal.0 * distance,
                    points[i].y + normal.1 * distance,
                ),
                direction: (dx, dy),
                normal,
            }
        })
        .collect();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (before, after) = (&moved[(i + n - 1) % n], &moved[i]);
        let denominator = cross(before.direction, after.direction);
        if !denominator.is_finite() || denominator == 0.0 {
            return None;
        }
        let (p, q) = (before.origin, after.origin);
        let t = cross((q.x - p.x, q.y - p.y), after.direction) / denominator;
        out.push(Point::new(
            p.x + t * before.direction.0,
            p.y + t * before.direction.1,
        ));
    }
    // The inset keeps the orientation and lies at least the distance inside every edge, or the edges crossed.
    let inset_area = signed_area(&out) * orientation;
    if inset_area.is_nan() || inset_area <= 0.0 {
        return None;
    }
    let least = distance * (1.0 - 1e-9);
    for (i, edge) in moved.iter().enumerate() {
        for q in &out {
            let inside = (q.x - points[i].x) * edge.normal.0 + (q.y - points[i].y) * edge.normal.1;
            if inside.is_nan() || inside < least {
                return None;
            }
        }
    }
    Some(out)
}

/// An edge of a polygon moved inwards: a point on the moved edge, the edge's direction and its inward unit normal.
struct MovedEdge {
    origin: Point,
    direction: (f64, f64),
    normal: (f64, f64),
}

/// The signed area of a polygon by the shoelace formula, whose sign is the polygon's orientation.
fn signed_area(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}
