//! The mapping of a two-dimensional axes, Cartesian or polar, from data to the page.
//!
//! Both kinds of axes map a data point in two stages. Each axis map first takes its value to an *axis coordinate*:
//! in Cartesian axes the page x or page y itself, and in polar axes the page angle in radians (anticlockwise from
//! east) or the page radius in points. The plane then places the pair of axis coordinates on the page, which is the
//! identity in Cartesian axes and the polar placement about the centre in polar ones.
//!
//! Artists that draw data directly map each point and join consecutive points with straight lines on the page
//! ([`Plane::map`]). Artists that draw shapes representing data build their outlines in axis coordinates from edges
//! at constant coordinates, and [`Plane::outline`] places those outlines so that every edge follows the axes: in polar
//! axes an edge at constant radius becomes an arc. In Cartesian axes neither adds a vertex, so Cartesian geometry is
//! exactly what it would be without the plane.

use std::f64::consts::TAU;

use crate::display::{Point, Rect, Sector};
use crate::hit::AxisMap;

use super::rects::{CLAMP_MARGIN, expand};

/// The largest distance, in points, that a chord of a followed outline may stray from the arc it stands for.
pub(crate) const CHORD_ERROR_PT: f64 = 0.05;

/// The mapping of a two-dimensional axes from data to the page.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plane {
    /// The map of x to its axis coordinate: page x, or the page angle in polar axes.
    pub x: AxisMap,
    /// The map of y to its axis coordinate: page y, or the page radius in polar axes.
    pub y: AxisMap,
    /// How axis coordinates are placed on the page.
    pub warp: Warp,
}

/// How the axis coordinates of a [`Plane`] are placed on the page.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Warp {
    /// The axis coordinates are the page coordinates.
    None,
    /// The axis coordinates are a page angle and a page radius about `centre`.
    Polar {
        /// The centre of the polar axes on the page.
        centre: Point,
    },
}

impl Plane {
    /// Returns a Cartesian plane.
    pub fn cartesian(x: AxisMap, y: AxisMap) -> Self {
        Self {
            x,
            y,
            warp: Warp::None,
        }
    }

    /// Returns whether the plane is polar.
    pub fn is_polar(&self) -> bool {
        matches!(self.warp, Warp::Polar { .. })
    }

    /// Places a pair of axis coordinates on the page.
    pub fn place(&self, a: Point) -> Point {
        match self.warp {
            Warp::None => a,
            Warp::Polar { centre } => polar_point(centre, a.x, a.y),
        }
    }

    /// Returns the axis coordinates of a data point, or `None` when it cannot be placed: a value that is not finite or
    /// that a logarithmic axis cannot place, and in polar axes a radius beyond the centre side of the radial limits,
    /// which would otherwise be reflected through the centre.
    pub fn axis_coords(&self, p: [f64; 2]) -> Option<Point> {
        if !(p[0].is_finite() && p[1].is_finite()) {
            return None;
        }
        let a = Point::new(self.x.to_figure(p[0]), self.y.to_figure(p[1]));
        if !(a.x.is_finite() && a.y.is_finite()) {
            return None;
        }
        if self.is_polar() {
            let inner = self.y.start.min(self.y.end);
            let span = (self.y.end - self.y.start).abs();
            if a.y < inner - 1e-9 * span.max(1.0) {
                return None;
            }
        }
        Some(a)
    }

    /// Maps a data point to the page, or returns `None` when it cannot be placed.
    pub fn map(&self, p: [f64; 2]) -> Option<Point> {
        let q = self.place(self.axis_coords(p)?);
        (q.x.is_finite() && q.y.is_finite()).then_some(q)
    }

    /// Places an outline given in axis coordinates, closed when `closed` is true, so that its edges follow the axes.
    ///
    /// In Cartesian axes the points are returned as they are. In polar axes every edge along which the angle changes is
    /// divided, in axis coordinates, into pieces short enough that the chord of each piece strays from the arc it
    /// stands for by at most [`CHORD_ERROR_PT`], and the result is placed; an edge at constant angle is a straight
    /// radial line and is not divided. The closing edge of a closed outline is divided too, but its end, the first
    /// point, is not repeated.
    pub fn outline(&self, points: &[Point], closed: bool) -> Vec<Point> {
        let Warp::Polar { centre } = self.warp else {
            return points.to_vec();
        };
        let mut out = Vec::with_capacity(points.len());
        let n = points.len();
        let edges = if closed { n } else { n.saturating_sub(1) };
        if n == 0 {
            return out;
        }
        out.push(polar_point(centre, points[0].x, points[0].y));
        for k in 0..edges {
            let (a, b) = (points[k], points[(k + 1) % n]);
            let pieces = arc_pieces(b.x - a.x, a.y.abs().max(b.y.abs()));
            let last = if closed && k + 1 == n {
                pieces - 1
            } else {
                pieces
            };
            for j in 1..=last {
                let t = j as f64 / pieces as f64;
                out.push(polar_point(
                    centre,
                    a.x + (b.x - a.x) * t,
                    a.y + (b.y - a.y) * t,
                ));
            }
        }
        out
    }

    /// Returns the rectangle of axis coordinates to which shapes representing data are clamped before they are placed.
    ///
    /// In Cartesian axes this is the clip rectangle grown by [`CLAMP_MARGIN`], so that a shape reaching far beyond the
    /// limits is drawn at a reasonable size with its clamped edges out of sight. In polar axes it is the plot region
    /// itself, the angles of the sector by the radii from the edge of the hole to the rim, so that the outline of a
    /// clamped shape is the outline of what is seen; a full turn is not clamped in angle.
    pub fn bounds(&self, clip: Rect) -> Rect {
        match self.warp {
            Warp::None => expand(clip, CLAMP_MARGIN),
            Warp::Polar { .. } => {
                let (r0, r1) = (self.y.start.min(self.y.end), self.y.start.max(self.y.end));
                let sweep = (self.x.end - self.x.start).abs();
                let (a0, a1) = if sweep >= TAU - 1e-9 {
                    (-1e6, 1e6)
                } else {
                    (self.x.start.min(self.x.end), self.x.start.max(self.x.end))
                };
                Rect::new(a0, r0, a1 - a0, r1 - r0)
            }
        }
    }

    /// Returns the sector of the plot region of a polar plane, or `None` for a Cartesian one.
    pub fn sector(&self) -> Option<Sector> {
        let Warp::Polar { centre } = self.warp else {
            return None;
        };
        let sweep = (self.x.end - self.x.start).abs();
        let full = sweep >= TAU - 1e-9;
        Some(Sector {
            centre,
            inner: self.y.start.min(self.y.end),
            outer: self.y.start.max(self.y.end),
            start: if full {
                self.x.start.rem_euclid(TAU)
            } else {
                self.x.start.min(self.x.end)
            },
            sweep: if full { TAU } else { sweep },
        })
    }
}

/// Returns the page point at page angle `phi` (radians anticlockwise from east) and radius `rho` from `centre`; the
/// page y grows downwards.
pub(crate) fn polar_point(centre: Point, phi: f64, rho: f64) -> Point {
    Point::new(centre.x + rho * phi.cos(), centre.y - rho * phi.sin())
}

/// Returns the largest angle, in radians, of an arc of radius `rho` points whose chord strays from it by at most
/// [`CHORD_ERROR_PT`].
pub(crate) fn max_arc_step(rho: f64) -> f64 {
    if !(rho.is_finite() && rho > CHORD_ERROR_PT) {
        return TAU;
    }
    2.0 * (1.0 - CHORD_ERROR_PT / rho).acos()
}

/// Returns the number of pieces into which an arc sweeping `angle` radians at radius up to `rho` points is divided so
/// that no chord strays from it by more than [`CHORD_ERROR_PT`]; at least 1, and at most a bounded number.
pub(crate) fn arc_pieces(angle: f64, rho: f64) -> usize {
    let angle = angle.abs();
    if !(angle.is_finite() && angle > 0.0) {
        return 1;
    }
    let n = (angle / max_arc_step(rho)).ceil();
    if n.is_finite() && n >= 1.0 {
        (n as usize).min(100_000)
    } else {
        1
    }
}

/// Returns the bounding box, at unit radius about the origin with y growing upwards, of the sector between the page
/// angles `a0 < a1`, including the centre.
pub(crate) fn unit_sector_box(a0: f64, a1: f64) -> (f64, f64, f64, f64) {
    use std::f64::consts::FRAC_PI_2;
    let mut xs = vec![0.0, a0.cos(), a1.cos()];
    let mut ys = vec![0.0, a0.sin(), a1.sin()];
    if a1 - a0 >= TAU - 1e-9 {
        return (-1.0, 1.0, -1.0, 1.0);
    }
    let mut k = (a0 / FRAC_PI_2).ceil();
    while k * FRAC_PI_2 <= a1 + 1e-12 {
        let a = k * FRAC_PI_2;
        xs.push(a.cos());
        ys.push(a.sin());
        k += 1.0;
    }
    let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
    let max = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (min(&xs), max(&xs), min(&ys), max(&ys))
}
