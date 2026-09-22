use ironlab_scene::display::Point;
use ironlab_scene::maths::polygon::inset;

use crate::assert_close;

fn square(x0: f64, y0: f64, side: f64, clockwise: bool) -> Vec<Point> {
    let mut corners = vec![
        Point::new(x0, y0),
        Point::new(x0 + side, y0),
        Point::new(x0 + side, y0 + side),
        Point::new(x0, y0 + side),
    ];
    if clockwise {
        corners.reverse();
    }
    corners
}

/// The distance of `q` inside the edge from `a` to `b` of a polygon whose interior holds `inside`.
fn distance_inside(q: Point, a: Point, b: Point, inside: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx.hypot(dy);
    let (nx, ny) = (-dy / length, dx / length);
    let sign = ((inside.x - a.x) * nx + (inside.y - a.y) * ny).signum();
    sign * ((q.x - a.x) * nx + (q.y - a.y) * ny)
}

// WHY: the edge of a face is a ring between the face and its inset, so every inset vertex must sit exactly the
// distance inside the two edges that meet at its corner, in the corner's own position, whichever way round the
// polygon is listed; an inset that moved a vertex along an edge or swapped the order would draw a ring of uneven
// width or one that crossed itself.
#[test]
fn the_inset_of_a_square_is_the_square_shrunk_by_the_distance_on_every_side() {
    for clockwise in [false, true] {
        let outer = square(10.0, 20.0, 8.0, clockwise);
        let inner = inset(&outer, 1.5).expect("a square has an inset");
        let expected = square(11.5, 21.5, 5.0, clockwise);
        for (k, (q, e)) in inner.iter().zip(&expected).enumerate() {
            assert_close(q.x, e.x, 1e-12);
            assert_close(q.y, e.y, 1e-12);
            assert!(
                (q.x - e.x).abs() <= 1e-12 && (q.y - e.y).abs() <= 1e-12,
                "clockwise {clockwise}: inset vertex {k} is {q:?}, expected {e:?}"
            );
        }
    }
}

// WHY: the faces of a projected surface are sheared quadrilaterals, not squares, so the inset must move every edge
// along its own normal: each inset vertex lies exactly the distance inside its two edges and at least the distance
// inside the others.
#[test]
fn every_inset_vertex_of_a_sheared_quadrilateral_lies_the_distance_inside_its_edges() {
    let outer = [
        Point::new(0.0, 0.0),
        Point::new(10.0, 2.0),
        Point::new(13.0, 9.0),
        Point::new(2.0, 8.0),
    ];
    let centre = Point::new(6.0, 5.0);
    let inner = inset(&outer, 0.75).expect("a convex quadrilateral has an inset");
    assert_eq!(inner.len(), 4, "one inset vertex per corner");
    for (k, q) in inner.iter().enumerate() {
        for i in 0..4 {
            let (a, b) = (outer[i], outer[(i + 1) % 4]);
            let d = distance_inside(*q, a, b, centre);
            let adjacent = i == k || (i + 1) % 4 == k;
            if adjacent {
                assert!(
                    (d - 0.75).abs() <= 1e-9,
                    "inset vertex {k} lies {d} inside its own edge {i}, expected 0.75"
                );
            } else {
                assert!(
                    d >= 0.75 - 1e-9,
                    "inset vertex {k} lies {d} inside edge {i}, at least 0.75"
                );
            }
        }
    }
}

// WHY: a twisted face seen edge-on projects to a bow-tie, and a face too small for its edge width has no ring; both
// must be refused, so that the caller can fall back to a plain stroke rather than fill a crossed or folded ring, and
// so must degenerate or non-finite input.
#[test]
fn polygons_without_a_proper_inset_are_refused() {
    let bow_tie = [
        Point::new(0.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(10.0, 0.0),
        Point::new(0.0, 10.0),
    ];
    assert_eq!(inset(&bow_tie, 1.0), None, "a bow-tie has no inset");
    let straight_corner = [
        Point::new(0.0, 0.0),
        Point::new(5.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
    ];
    assert_eq!(
        inset(&straight_corner, 1.0),
        None,
        "a straight corner is not strictly convex"
    );
    let reflex = [
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(5.0, 5.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ];
    assert_eq!(inset(&reflex, 1.0), None, "a reflex corner is refused");
    let small = square(0.0, 0.0, 2.0, false);
    assert_eq!(
        inset(&small, 1.0),
        None,
        "a square folds over at half its side"
    );
    assert!(
        inset(&small, 0.99).is_some(),
        "a square just short of folding keeps an inset"
    );
    assert_eq!(inset(&small[..2], 0.1), None, "two points have no inset");
    assert_eq!(inset(&small, 0.0), None, "a zero distance is refused");
    assert_eq!(inset(&small, f64::NAN), None, "a NaN distance is refused");
    let with_nan = [
        Point::new(0.0, 0.0),
        Point::new(f64::NAN, 0.0),
        Point::new(1.0, 1.0),
    ];
    assert_eq!(
        inset(&with_nan, 0.1),
        None,
        "a non-finite vertex is refused"
    );
    let repeated = [
        Point::new(0.0, 0.0),
        Point::new(0.0, 0.0),
        Point::new(1.0, 1.0),
        Point::new(0.0, 1.0),
    ];
    assert_eq!(inset(&repeated, 0.1), None, "a repeated vertex is refused");
}
