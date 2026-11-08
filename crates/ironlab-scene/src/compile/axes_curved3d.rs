//! Cylindrical and spherical axes: the frame of circles that replaces the edges of the box, its grid, tick marks and
//! labels, the depth-sorted artists and the legend.
//!
//! Both kinds of axes are three-dimensional: data points are placed in the normalised box by the coordinate stage of
//! the [`Projector`] and projected by the same camera, zoom and pan as Cartesian 3D axes, so that the view is navigated
//! alike; what the axes draws is fitted to the plot rectangle by its own circumscribing sphere. Every decoration is
//! built in the normalised box and projected through the same projector. A line of the outer frame (rims, equator,
//! meridian) is split where it passes the depth of the centre of the box: the part behind the centre is painted before
//! the artists and the part in front after them, as the edges of a Cartesian box are. A line that passes through the
//! drawn volume or lies on the floor of a cylinder is painted among the artists, with a depth at each vertex, so that
//! the artists in front of it hide it. Tick marks and tick labels are painted behind or in front by the depth of the
//! point they label.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use ironlab_ir::{AxisSide, PolarView, Projection, View3d};

use crate::display::{Depth, Item, ItemKind, Point, Rect};
use crate::hit::{AxesHit, AxesHitKind, HitMap};

use super::Ctx;
use super::artists::{
    AxesInput, Projector, Space, Stage, coalesce_markers, draw_artists, group_dense,
};
use super::axes3d::{LABEL_CLEARANCE, TICK_LENGTH, crowds, drawn_side, half_extent, unit};
use super::decor::AxisTicks;
use super::paths::{self, PathBuilder};
use super::style::{self, GRID, INK};
use super::text::TextBlock;
use super::{colourbar, legend};

/// A point or a vector of the normalised box.
type Vec3 = [f64; 3];

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scaled(a: Vec3, k: f64) -> Vec3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// The largest angle of one cubic Bézier piece of a decoration circle, whose distance from the circle is then a few
/// millionths of its radius.
const MAX_BEZIER_ARC: f64 = FRAC_PI_4;

/// The step, in normalised box units, by which a point is moved outward to find the page direction pointing away
/// from the circle it lies on.
const OUTWARD_STEP: f64 = 0.05;

/// The page segments of one kind of decoration line, split into the part painted behind the artists and the part
/// painted in front of them.
#[derive(Default)]
struct Layer {
    back: PathBuilder,
    front: PathBuilder,
}

impl Layer {
    fn side(&mut self, behind: bool) -> &mut PathBuilder {
        if behind {
            &mut self.back
        } else {
            &mut self.front
        }
    }
}

/// The kind of a decoration line, which sets its stroke.
#[derive(Clone, Copy)]
enum Kind {
    Grid,
    Lines,
}

/// Where a decoration line is painted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layering {
    /// A line of the outer frame, split at the depth of the centre of the box: the part behind it is painted before
    /// the artists and the part in front after them.
    Split,
    /// A line that passes through the drawn volume, or lies on its floor, painted among the artists with a depth at
    /// each vertex, so that the artists in front of it hide it.
    Interior,
}

/// The number of pieces into which an interior straight line is cut, each sorted at its own mean depth, so that the
/// painter's order of a backend without a depth buffer can put the parts of a long line on both sides of an artist.
const INTERIOR_PIECES: usize = 8;

/// The largest angle of the part of an interior circle drawn as one primitive.
const INTERIOR_ARC: f64 = std::f64::consts::FRAC_PI_8;

/// The outline on the page of the volume that the frame of a curved 3D axes encloses.
enum Silhouette {
    /// The disc of a sphere: its centre and radius.
    Disc(Point, f64),
    /// The convex hull of a cylinder, its vertices in order.
    Hull(Vec<Point>),
}

impl Silhouette {
    /// The silhouette of the sphere of unit diameter, or of the cylinder of the whole box, as the projector draws it.
    fn of(projector: &Projector) -> Self {
        let centre = projector
            .project([0.0; 3])
            .map_or(Point::new(0.0, 0.0), |(p, _)| p);
        if matches!(projector.stage, Stage::Sphere { .. }) {
            return Silhouette::Disc(centre, 0.5 * projector.scale);
        }
        let rims: Vec<Point> = (0..128)
            .flat_map(|k| {
                let a = TAU * f64::from(k) / 128.0;
                [-0.5, 0.5].map(|z| projector.project([0.5 * a.cos(), 0.5 * a.sin(), z]))
            })
            .flatten()
            .map(|(p, _)| p)
            .collect();
        Silhouette::Hull(convex_hull(rims))
    }

    /// Returns whether a rectangle lies wholly inside the silhouette.
    fn holds(&self, r: Rect) -> bool {
        let corners = [
            Point::new(r.x, r.y),
            Point::new(r.right(), r.y),
            Point::new(r.right(), r.bottom()),
            Point::new(r.x, r.bottom()),
        ];
        match self {
            Silhouette::Disc(c, radius) => corners
                .iter()
                .all(|p| (p.x - c.x).hypot(p.y - c.y) <= *radius),
            Silhouette::Hull(hull) => {
                hull.len() >= 3
                    && corners.iter().all(|p| {
                        hull.iter().zip(hull.iter().cycle().skip(1)).all(|(a, b)| {
                            (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x) >= 0.0
                        })
                    })
            }
        }
    }
}

/// Returns the convex hull of a set of points, its vertices in the order that turns positively (anticlockwise in a
/// y-up frame), by Andrew's monotone chain.
fn convex_hull(mut points: Vec<Point>) -> Vec<Point> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if points.len() < 3 {
        return points;
    }
    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut hull: Vec<Point> = Vec::with_capacity(2 * points.len());
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &Point>> = if pass == 0 {
            Box::new(points.iter())
        } else {
            Box::new(points.iter().rev())
        };
        for &p in ordered {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// The decorations of one curved 3D axes while they are built.
struct Frame<'a> {
    projector: &'a Projector,
    owner: ironlab_ir::NodeId,
    /// Grid lines.
    grid: Layer,
    /// The frame and the axis lines.
    lines: Layer,
    /// Tick marks.
    marks: Layer,
    /// Tick labels behind the centre of the box, painted before the artists.
    back_text: Vec<Item>,
    /// Tick labels in front of the centre of the box, painted after the artists.
    front_text: Vec<Item>,
    /// The outline of the frame on the page, inside which a label behind the centre of the box may be hidden.
    silhouette: Silhouette,
    /// Interior lines, each with the depth at which it is sorted among the artists.
    interior: Vec<(f64, Item)>,
    /// The boxes of the tick labels drawn so far, which later labels must not crowd.
    placed: Vec<Rect>,
    tick_length: f64,
    pad: f64,
    clearance: f64,
}

impl<'a> Frame<'a> {
    fn new(ctx: &Ctx, projector: &'a Projector, owner: ironlab_ir::NodeId) -> Self {
        let fs = ctx.font_size;
        let tick_length = TICK_LENGTH * fs;
        Self {
            projector,
            owner,
            grid: Layer::default(),
            lines: Layer::default(),
            marks: Layer::default(),
            back_text: Vec::new(),
            front_text: Vec::new(),
            interior: Vec::new(),
            silhouette: Silhouette::of(projector),
            placed: Vec::new(),
            tick_length,
            pad: tick_length + 0.3 * fs,
            clearance: LABEL_CLEARANCE * fs,
        }
    }

    fn layer(&mut self, kind: Kind) -> &mut Layer {
        match kind {
            Kind::Grid => &mut self.grid,
            Kind::Lines => &mut self.lines,
        }
    }

    fn page(&self, n: Vec3) -> Option<(Point, f64)> {
        self.projector.project(n)
    }

    /// Adds an interior polyline through the points of the box as one primitive, with the depth of each vertex.
    fn interior_polyline(&mut self, kind: Kind, points: &[Vec3]) {
        let Some(placed) = points
            .iter()
            .map(|n| self.page(*n))
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        if placed.len() < 2 {
            return;
        }
        let colour = match kind {
            Kind::Grid => GRID,
            Kind::Lines => INK,
        };
        let mut path = PathBuilder::new();
        let positions: Vec<Point> = placed.iter().map(|(q, _)| *q).collect();
        path.polyline(&positions, false);
        let depths: Vec<f64> = placed.iter().map(|(_, d)| *d).collect();
        let key = depths.iter().sum::<f64>() / depths.len() as f64;
        if let Some(item) = paths::item(
            self.owner,
            path.finish(),
            None,
            Some(paths::solid(colour, 0.5)),
        ) {
            self.interior
                .push((key, paths::with_depth(item, Depth::Vertices(depths))));
        }
    }

    /// Adds the straight line from `a` to `b`: to a layer, split where it passes the depth of the centre of the box,
    /// or, for an interior line, among the artists in [`INTERIOR_PIECES`] pieces.
    fn segment(&mut self, kind: Kind, a: Vec3, b: Vec3, how: Layering) {
        if how == Layering::Interior {
            let at = |t: f64| add(a, scaled(add(b, scaled(a, -1.0)), t));
            for k in 0..INTERIOR_PIECES {
                let (t0, t1) = (
                    k as f64 / INTERIOR_PIECES as f64,
                    (k + 1) as f64 / INTERIOR_PIECES as f64,
                );
                self.interior_polyline(kind, &[at(t0), at(t1)]);
            }
            return;
        }
        let (Some((pa, da)), Some((pb, db))) = (self.page(a), self.page(b)) else {
            return;
        };
        if how == Layering::Split && da * db < 0.0 {
            let t = da / (da - db);
            let m = add(a, scaled(add(b, scaled(a, -1.0)), t));
            let Some((pm, _)) = self.page(m) else { return };
            self.layer(kind).side(da < 0.0).polyline(&[pa, pm], false);
            self.layer(kind).side(db < 0.0).polyline(&[pm, pb], false);
        } else {
            let behind = da + db < 0.0;
            self.layer(kind).side(behind).polyline(&[pa, pb], false);
        }
    }

    /// Adds the arc `centre + u·cos t + v·sin t` for `t` from `t0` to `t1 > t0` to a layer, as cubic Bézier pieces of
    /// at most [`MAX_BEZIER_ARC`], split at the angles where it passes the depth of the centre of the box.
    ///
    /// The projection is affine, so the projection of the Bézier of an arc of the box is the Bézier of the projected
    /// arc, and the depth along the arc is `d_c + d_u·cos t + d_v·sin t`, whose zeros are found exactly.
    fn arc(
        &mut self,
        kind: Kind,
        centre: Vec3,
        u: Vec3,
        v: Vec3,
        (t0, t1): (f64, f64),
        how: Layering,
    ) {
        let depth = |n: Vec3| self.page(n).map(|(_, d)| d);
        let (Some(dc), Some(du), Some(dv)) =
            (depth(centre), depth(add(centre, u)), depth(add(centre, v)))
        else {
            return;
        };
        let (du, dv) = (du - dc, dv - dc);
        let mut cuts = vec![t0, t1];
        let reach = du.hypot(dv);
        if how == Layering::Split && reach > 1e-12 && dc.abs() < reach {
            let (alpha, beta) = (dv.atan2(du), (-dc / reach).acos());
            for t in [alpha + beta, alpha - beta] {
                let t = t0 + (t - t0).rem_euclid(TAU);
                if t > t0 + 1e-12 && t < t1 - 1e-12 {
                    cuts.push(t);
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        let at = |t: f64| add(centre, add(scaled(u, t.cos()), scaled(v, t.sin())));
        let tangent = |t: f64| add(scaled(u, -t.sin()), scaled(v, t.cos()));
        for pair in cuts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if b - a <= 1e-12 {
                continue;
            }
            let middle = (a + b) / 2.0;
            let behind = dc + du * middle.cos() + dv * middle.sin() < 0.0;
            let pieces = ((b - a) / MAX_BEZIER_ARC).ceil().max(1.0) as usize;
            let mut path = PathBuilder::new();
            let Some((start, _)) = self.page(at(a)) else {
                continue;
            };
            path.move_to(start);
            let mut whole = true;
            for k in 0..pieces {
                let ta = a + (b - a) * k as f64 / pieces as f64;
                let tb = a + (b - a) * (k + 1) as f64 / pieces as f64;
                let reach = 4.0 / 3.0 * ((tb - ta) / 4.0).tan();
                let (p0, p3) = (at(ta), at(tb));
                let p1 = add(p0, scaled(tangent(ta), reach));
                let p2 = add(p3, scaled(tangent(tb), -reach));
                match (self.page(p1), self.page(p2), self.page(p3)) {
                    (Some((c1, _)), Some((c2, _)), Some((end, _))) => path.cubic_to(c1, c2, end),
                    _ => whole = false,
                }
            }
            if whole {
                self.layer(kind).side(behind).extend(path);
            }
        }
    }

    /// Adds a whole circle, or the arc between two angles, of radius `radius` about `centre` in the plane spanned by
    /// the unit vectors `e1` and `e2`.
    fn circle(
        &mut self,
        kind: Kind,
        centre: Vec3,
        (e1, e2): (Vec3, Vec3),
        radius: f64,
        span: (f64, f64),
        how: Layering,
    ) {
        if radius > 1e-9 && how == Layering::Interior {
            self.interior_circle(kind, centre, (e1, e2), radius, span);
        } else if radius > 1e-9 {
            self.arc(
                kind,
                centre,
                scaled(e1, radius),
                scaled(e2, radius),
                span,
                how,
            );
        }
    }

    /// Adds an interior circle, or the arc between two angles, among the artists: chords that stray from the arc by at
    /// most [`super::plane::CHORD_ERROR_PT`] on the page, in parts of at most [`INTERIOR_ARC`], each a primitive with
    /// the depth of each vertex.
    fn interior_circle(
        &mut self,
        kind: Kind,
        centre: Vec3,
        (e1, e2): (Vec3, Vec3),
        radius: f64,
        (t0, t1): (f64, f64),
    ) {
        let sides = self.projector.sides();
        let reach = radius * self.projector.scale * sides.iter().copied().fold(0.0, f64::max);
        let sweep = t1 - t0;
        let parts = (sweep / INTERIOR_ARC).ceil().max(1.0) as usize;
        let chords = super::plane::arc_pieces(sweep / parts as f64, reach);
        let at = |t: f64| {
            add(
                centre,
                add(scaled(e1, radius * t.cos()), scaled(e2, radius * t.sin())),
            )
        };
        for part in 0..parts {
            let points: Vec<Vec3> = (0..=chords)
                .map(|k| t0 + sweep * (part * chords + k) as f64 / (parts * chords) as f64)
                .map(at)
                .collect();
            self.interior_polyline(kind, &points);
        }
    }

    /// Returns the unit page direction in which the point `at` of the box moves when it moves along `outward`, or away
    /// from the projected centre of the box when that direction is seen end-on.
    fn outward(&self, at: Vec3, outward: Vec3) -> Point {
        let (Some((p, _)), Some((q, _)), Some((c, _))) = (
            self.page(at),
            self.page(add(at, scaled(outward, OUTWARD_STEP))),
            self.page([0.0; 3]),
        ) else {
            return Point::new(0.0, 1.0);
        };
        let away = unit(Point::new(p.x - c.x, p.y - c.y), Point::new(0.0, 1.0));
        unit(Point::new(q.x - p.x, q.y - p.y), away)
    }

    /// Draws a tick label for the point `at` of the box: with a tick mark from its projection along the page direction
    /// `d` and the label beyond it when `mark` is true, or centred on the projection when `d` is zero. A label that
    /// would crowd one already drawn is left out. The tick mark is painted behind the artists when the point lies
    /// behind the centre of the box. The label is painted behind them only when, besides, its box lies wholly inside
    /// the silhouette of the frame, where an artist may cover all of it; a label that reaches outside the silhouette is
    /// painted in front, so that no artist cuts it in part. `always_in_front` paints both in front. Returns the box of
    /// the label when it is drawn.
    fn tick(
        &mut self,
        ctx: &mut Ctx,
        label: &TextBlock,
        at: Vec3,
        (d, mark): (Point, bool),
        always_in_front: bool,
    ) -> Option<Rect> {
        let (q, depth) = self.page(at)?;
        let behind = depth < 0.0 && !always_in_front;
        if mark {
            let end = Point::new(q.x + d.x * self.tick_length, q.y + d.y * self.tick_length);
            self.marks.side(behind).polyline(&[q, end], false);
        }
        let offset = if d.x == 0.0 && d.y == 0.0 {
            0.0
        } else if mark {
            self.pad + half_extent(d, label.width(), label.total_height())
        } else {
            self.pad - self.tick_length + half_extent(d, label.width(), label.total_height())
        };
        let centre = Point::new(q.x + d.x * offset, q.y + d.y * offset);
        let origin = label.origin_for_centre(centre);
        let bounds = label.bounds(origin);
        if self
            .placed
            .iter()
            .any(|p| crowds(*p, bounds, self.clearance))
        {
            return None;
        }
        let out = if behind && self.silhouette.holds(bounds) {
            &mut self.back_text
        } else {
            &mut self.front_text
        };
        label.draw_on_page(ctx, origin, INK, self.owner, out);
        self.placed.push(bounds);
        Some(bounds)
    }

    /// Draws the common exponent of an axis beyond the tick label at `centre` of `last`, along the page direction `a`.
    fn exponent(
        &mut self,
        ctx: &mut Ctx,
        ticks: &AxisTicks,
        last: Option<(Rect, Vec3)>,
        a: Point,
        gap: f64,
    ) {
        let (Some(e), Some((bounds, at))) = (&ticks.exponent, last) else {
            return;
        };
        let centre = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        );
        let shift = half_extent(a, bounds.width, bounds.height)
            + gap
            + half_extent(a, e.width(), e.total_height());
        let c = Point::new(centre.x + a.x * shift, centre.y + a.y * shift);
        let origin = e.origin_for_centre(c);
        let behind =
            self.page(at).is_some_and(|(_, d)| d < 0.0) && self.silhouette.holds(e.bounds(origin));
        let out = if behind {
            &mut self.back_text
        } else {
            &mut self.front_text
        };
        e.draw_on_page(ctx, origin, INK, self.owner, out);
    }

    /// The lowest page y reached by the tick labels drawn so far and by the given points.
    fn lowest(&self, points: impl Iterator<Item = Point>) -> f64 {
        points
            .map(|p| p.y)
            .chain(self.placed.iter().map(Rect::bottom))
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// The leftmost page x reached by the tick labels drawn so far and by the given points.
    fn leftmost(&self, points: impl Iterator<Item = Point>) -> f64 {
        points
            .map(|p| p.x)
            .chain(self.placed.iter().map(|r| r.x))
            .fold(f64::INFINITY, f64::min)
    }
}

const X_AXIS: Vec3 = [1.0, 0.0, 0.0];
const Y_AXIS: Vec3 = [0.0, 1.0, 0.0];
const Z_AXIS: Vec3 = [0.0, 0.0, 1.0];

/// Draws a cylindrical or spherical axes and records its hit geometry.
pub(super) fn emit(
    ctx: &mut Ctx,
    input: &AxesInput,
    view: View3d,
    out: &mut Vec<Item>,
    hits: &mut HitMap,
) {
    let axes = input.axes;
    let stage = Stage::of(axes);
    let projector = Projector::new(
        view,
        input.ranges,
        input.plot,
        super::aspect::box_sides(axes, input.ranges),
        stage,
        label_inset(ctx, input, stage),
    );
    let mut frame = Frame::new(ctx, &projector, axes.id);
    let mut axis_labels = Vec::new();
    match (axes.projection, stage) {
        (
            Projection::Cylindrical { polar, .. },
            Stage::Cylinder {
                zero,
                per_unit,
                hole,
            },
        ) => cylinder(
            ctx,
            input,
            &mut frame,
            Cylinder {
                polar,
                zero,
                per_unit,
                hole,
            },
            &mut axis_labels,
        ),
        (_, Stage::Sphere { .. }) => sphere(ctx, input, &mut frame, &mut axis_labels),
        _ => {}
    }

    let Frame {
        grid,
        lines,
        marks,
        back_text,
        front_text,
        interior,
        ..
    } = frame;
    let mut content = Vec::new();
    let stroke = |colour| Some(paths::solid(colour, 0.5));
    content.extend(paths::item(axes.id, grid.back.finish(), None, stroke(GRID)));
    content.extend(paths::item(axes.id, lines.back.finish(), None, stroke(INK)));
    content.extend(paths::item(axes.id, marks.back.finish(), None, stroke(INK)));
    content.extend(back_text);

    let primaries = style::primaries(ctx.figure, axes);
    let drawn = draw_artists(ctx, input, &primaries, &Space::ThreeD(&projector));
    hits.artists.extend(drawn.hits);
    hits.images.extend(drawn.images);
    let mut prims = drawn.prims;
    prims.extend(interior);
    crate::maths::camera::depth_order(&mut prims);
    let sorted: Vec<Item> = prims.into_iter().map(|(_, item)| item).collect();
    let artists = group_dense(coalesce_markers(sorted), &drawn.dense);
    if !artists.is_empty() {
        content.push(Item {
            source: Some(axes.id),
            kind: ItemKind::Depth { items: artists },
        });
    }

    content.extend(paths::item(
        axes.id,
        grid.front.finish(),
        None,
        stroke(GRID),
    ));
    content.extend(paths::item(
        axes.id,
        lines.front.finish(),
        None,
        stroke(INK),
    ));
    content.extend(paths::item(
        axes.id,
        marks.front.finish(),
        None,
        stroke(INK),
    ));
    content.extend(front_text);
    out.push(Item {
        source: Some(axes.id),
        kind: ItemKind::Group {
            clip: Some(input.outer),
            transform: None,
            items: content,
        },
    });
    out.extend(axis_labels);

    if let Some(title) = &input.decor.title {
        let plot = input.plot;
        let origin = Point::new(
            super::axes2d::title_left(axes.title_align, plot, title.width()),
            plot.y - 0.5 * ctx.font_size - title.depth(),
        );
        title.draw_on_page(ctx, origin, INK, axes.id, out);
    }
    colourbar::draw_inside(ctx, axes, input.plot, input.scales, out, hits);
    legend::draw_inside(ctx, input, &primaries, &[], out, hits);
    hits.axes.push(AxesHit {
        id: axes.id,
        plot_rect: input.plot,
        kind: AxesHitKind::ThreeD,
    });
}

/// Returns the room, across and down, that the tick labels placed outside the frame circles take beyond the
/// circumscribing sphere on each side: the pad between a circle and its labels and the widest or tallest of those
/// labels. They are the angle labels of a cylinder, and the azimuth and polar-angle labels of a sphere, and the z
/// labels of a cylinder whose z axis stands on a wall, beside it. The room depends on the labels alone, never on the
/// view, so the drawing keeps its size as the view turns.
fn label_inset(ctx: &Ctx, input: &AxesInput, stage: Stage) -> [f64; 2] {
    let fs = ctx.font_size;
    let pad = TICK_LENGTH * fs + 0.3 * fs;
    let on_wall = drawn_side(input.axes, 2) != AxisSide::Centre;
    let outside: &[usize] = match stage {
        Stage::Sphere { .. } => &[0, 1],
        _ if on_wall => &[0, 2],
        _ => &[0],
    };
    let ticks = &input.decor.ticks;
    let widest = outside
        .iter()
        .map(|&d| ticks[d].max_width())
        .fold(0.0, f64::max);
    let tallest = outside
        .iter()
        .map(|&d| ticks[d].max_height() + ticks[d].max_depth())
        .fold(0.0, f64::max);
    let room = |extent: f64| if extent > 0.0 { pad + extent } else { 0.0 };
    [room(widest), room(tallest)]
}

/// The polar view of a cylindrical axes as its stage applies it.
#[derive(Clone, Copy)]
struct Cylinder {
    polar: PolarView,
    zero: f64,
    per_unit: f64,
    hole: f64,
}

/// Draws the frame of a cylindrical axes.
///
/// On the floor, at the lower z limit: the rim circle (or the arc of a partial sector and its two radial edges), the
/// circle of the hole when there is one, spokes at the angular ticks when the x grid is on and circles at the radial
/// ticks when the y grid is on; angular tick labels outside the rim, and radial tick labels centred on the spoke that
/// the polar view names. On the wall: a ring at every z tick when the z grid is on. When the box is on: at the top, the
/// rim, the hole and the edges of a partial sector, joined to the floor by vertical edges at the edges of the sector,
/// and the two silhouette lines of the wall, from the floor rim to the top rim, that lie inside the sector.
///
/// The z axis, with its tick marks and labels, stands on the vertical line of that outline furthest left on the page
/// for the low side and furthest right for the high side, with its labels outside the cylinder; that line is drawn
/// even when the box is off. Through the middle it runs up the axis of the cylinder, among the artists, with its
/// labels to its left.
fn cylinder(
    ctx: &mut Ctx,
    input: &AxesInput,
    f: &mut Frame,
    c: Cylinder,
    axis_labels: &mut Vec<Item>,
) {
    let axes = input.axes;
    let [xr, _, zr] = *input.ranges;
    let [xt, yt, zt] = &input.decor.ticks;
    let p = f.projector;
    let phi = |theta: f64| c.zero + theta * c.per_unit;
    let rho = |r: f64| {
        let n = p.along_axes([xr.min, r, zr.min])[1];
        c.hole + (1.0 - c.hole) * (n + 0.5).max(0.0)
    };
    let height = |z: f64| p.along_axes([xr.min, input.ranges[1].min, z])[2];
    let floor = height(zr.min);
    let top = height(zr.max);
    let at = |phi: f64, rho: f64, z: f64| [0.5 * rho * phi.cos(), 0.5 * rho * phi.sin(), z];
    let radial = |phi: f64| [phi.cos(), phi.sin(), 0.0];

    // The sector, between its edge azimuths in increasing order.
    let (mut a0, mut a1) = (phi(xr.min), phi(xr.max));
    if (a1 - a0).abs() > TAU {
        a1 = a0 + TAU * (a1 - a0).signum();
    }
    if a1 < a0 {
        std::mem::swap(&mut a0, &mut a1);
    }
    let full = a1 - a0 >= TAU - 1e-9;
    let span = (a0, a1);
    let horizontal = (X_AXIS, Y_AXIS);
    // The floor is seen from inside the box when the viewer is on the side of it where the box lies.
    // The grid on the floor lies on the floor of the drawn volume, and every artist standing over it must hide it.
    let floor_layering = Layering::Interior;

    // The grid on the floor and on the wall.
    if axes.x.grid {
        for &theta in &xt.major {
            let a = phi(theta);
            let inside = full || (a - a0 > 1e-9 && a1 - a > 1e-9);
            if inside {
                f.segment(
                    Kind::Grid,
                    at(a, c.hole, floor),
                    at(a, 1.0, floor),
                    floor_layering,
                );
            }
        }
    }
    if axes.y.grid {
        for &r in &yt.major {
            let q = rho(r);
            if q > c.hole + 1e-6 && q < 1.0 - 1e-6 {
                f.circle(
                    Kind::Grid,
                    [0.0, 0.0, floor],
                    horizontal,
                    0.5 * q,
                    span,
                    floor_layering,
                );
            }
        }
    }
    if axes.z.grid {
        for &z in &zt.major {
            let h = height(z);
            if (h - floor).abs() > 1e-9 && (h - top).abs() > 1e-9 {
                f.circle(
                    Kind::Grid,
                    [0.0, 0.0, h],
                    horizontal,
                    0.5,
                    span,
                    Layering::Split,
                );
            }
        }
    }

    // The frame: the floor, the axis of the cylinder and, with the box, the top.
    let mut rims = vec![floor];
    if axes.box_ {
        rims.push(top);
    }
    for &z in &rims {
        f.circle(
            Kind::Lines,
            [0.0, 0.0, z],
            horizontal,
            0.5,
            span,
            Layering::Split,
        );
        f.circle(
            Kind::Lines,
            [0.0, 0.0, z],
            horizontal,
            0.5 * c.hole,
            span,
            Layering::Split,
        );
        if !full {
            for a in [a0, a1] {
                f.segment(
                    Kind::Lines,
                    at(a, c.hole, z),
                    at(a, 1.0, z),
                    Layering::Split,
                );
            }
        }
    }
    // The vertical lines of the outline, each an azimuth and a fraction of the rim radius: the edges of a partial
    // sector, and the silhouette lines of the wall that lie inside the sector.
    let mut uprights: Vec<(f64, f64)> = Vec::new();
    if !full {
        for a in [a0, a1] {
            for q in [c.hole, 1.0] {
                if q > 0.0 {
                    uprights.push((a, q));
                }
            }
        }
    }
    for a in wall_silhouette(p) {
        let a = a0 + (a - a0).rem_euclid(TAU);
        if full || (a - a0 > 1e-9 && a1 - a > 1e-9) {
            uprights.push((a, 1.0));
        }
    }
    // A z axis on the low side stands on the vertical line of the outline furthest left on the page, and one on the
    // high side on the line furthest right; one through the middle stands on the axis of the cylinder.
    let z_side = drawn_side(axes, 2);
    let page_x = |(a, q): (f64, f64)| f.page(at(a, q, floor)).map_or(0.0, |(point, _)| point.x);
    let z_wall = match z_side {
        AxisSide::Centre => None,
        AxisSide::Low => uprights
            .iter()
            .copied()
            .min_by(|u, v| page_x(*u).total_cmp(&page_x(*v))),
        AxisSide::High => uprights
            .iter()
            .copied()
            .max_by(|u, v| page_x(*u).total_cmp(&page_x(*v))),
    };
    // With the box, every vertical line of the outline is drawn; without it, only the one that carries the z axis.
    for &(a, q) in &uprights {
        if axes.box_ || z_wall == Some((a, q)) {
            f.segment(Kind::Lines, at(a, q, floor), at(a, q, top), Layering::Split);
        }
    }
    if z_wall.is_none() {
        f.segment(
            Kind::Lines,
            [0.0, 0.0, -0.5],
            [0.0, 0.0, 0.5],
            Layering::Interior,
        );
    }

    // Angular tick labels outside the rim.
    for (&theta, label) in xt.major.iter().zip(&xt.labels) {
        let a = phi(theta);
        let point = at(a, 1.0, floor);
        let d = f.outward(point, radial(a));
        f.tick(ctx, label, point, (d, true), false);
    }

    // The z axis: tick marks and labels on the line that carries it, to its left (on the high side, to its right).
    let base = |z: f64| match z_wall {
        Some((a, q)) => at(a, q, z),
        None => [0.0, 0.0, z],
    };
    let (bottom, upper) = (f.page(base(-0.5)), f.page(base(0.5)));
    let mut z_boxes = Vec::new();
    if let (Some((b, _)), Some((t, _))) = (bottom, upper) {
        let along = Point::new(t.x - b.x, t.y - b.y);
        if along.x.hypot(along.y) >= 1.0 {
            let along = unit(along, Point::new(0.0, -1.0));
            let normal = Point::new(-along.y, along.x);
            let leftward = if normal.x <= 0.0 {
                normal
            } else {
                Point::new(-normal.x, -normal.y)
            };
            let outward = if z_side == AxisSide::High {
                Point::new(-leftward.x, -leftward.y)
            } else {
                leftward
            };
            let mut last = None;
            for (&z, label) in zt.major.iter().zip(&zt.labels) {
                let point = base(height(z));
                if let Some(bounds) = f.tick(ctx, label, point, (outward, true), true) {
                    z_boxes.push(bounds);
                    last = Some((bounds, point));
                }
            }
            f.exponent(ctx, zt, last, along, 0.4 * ctx.font_size);
        }
    }

    // Radial tick labels centred on their spoke on the floor.
    let spoke = match (c.polar.radial_axis_angle, xt.major.as_slice()) {
        (Some(theta), _) if theta.is_finite() => theta,
        (_, [a, b, ..]) => (a + b) / 2.0,
        _ => xr.min + (xr.max - xr.min) / 16.0,
    };
    let psi = phi(spoke);
    let mut last = None;
    for (&r, label) in yt.major.iter().zip(&yt.labels) {
        let q = rho(r);
        if q < 1e-6 {
            continue;
        }
        let point = at(psi, q, floor);
        if let Some(bounds) = f.tick(ctx, label, point, (Point::new(0.0, 0.0), false), false) {
            last = Some((bounds, point));
        }
    }
    let spoke_direction = f.outward(at(psi, 0.5, floor), radial(psi));
    f.exponent(ctx, yt, last, spoke_direction, 0.2 * ctx.font_size);

    // Axis labels: the angle below the floor, the radius beyond the rim at the end of its spoke, and z upright beside
    // its tick labels.
    let fs = ctx.font_size;
    let gap = 0.4 * fs;
    let id = axes.id;
    let decor = input.decor;
    if let Some(label) = &decor.labels[0] {
        let samples = (0..64).filter_map(|k| {
            let a = TAU * f64::from(k) / 64.0;
            f.page(at(a, 1.0, floor)).map(|(q, _)| q)
        });
        let lowest = f.lowest(samples);
        if let Some((centre, _)) = f.page([0.0, 0.0, floor]) {
            let origin = Point::new(
                centre.x - label.width() / 2.0,
                lowest + gap + label.height(),
            );
            label.draw_on_page(ctx, origin, INK, id, axis_labels);
        }
    }
    if let Some(label) = &decor.labels[1] {
        let rim = at(psi, 1.0, floor);
        if let Some((q, _)) = f.page(rim) {
            let d = f.outward(rim, radial(psi));
            let extent = xt
                .labels
                .iter()
                .map(|l| 2.0 * half_extent(d, l.width(), l.total_height()))
                .fold(0.0, f64::max);
            let offset = f.pad + extent + gap + half_extent(d, label.width(), label.total_height());
            let centre = Point::new(q.x + d.x * offset, q.y + d.y * offset);
            label.draw_on_page(ctx, label.origin_for_centre(centre), INK, id, axis_labels);
        }
    }
    // The z label stands upright to the left of the cylinder (to its right for a z axis on the high side), clear of
    // the data inside it, as the z label of a Cartesian box stands beside its leftmost or rightmost edge.
    if let (Some(label), Some((b, _)), Some((t, _))) = (&decor.labels[2], bottom, upper) {
        let mid = Point::new((b.x + t.x) / 2.0, (b.y + t.y) / 2.0);
        let rims = (0..64).flat_map(|k| {
            let a = TAU * f64::from(k) / 64.0;
            [floor, top].map(|z| f.page(at(a, 1.0, z)).map(|(q, _)| q))
        });
        // Besides the z tick labels, the label clears every other tick label beside the span it stands over, such as an
        // angle label outside the rim below the wall.
        let (span_top, span_bottom) = (mid.y - label.width() / 2.0, mid.y + label.width() / 2.0);
        let beside: Vec<Rect> = f
            .placed
            .iter()
            .filter(|r| r.y < span_bottom && r.bottom() > span_top)
            .chain(&z_boxes)
            .copied()
            .collect();
        if z_side == AxisSide::High {
            let left = rims
                .flatten()
                .map(|q| q.x + f.pad)
                .chain(beside.iter().map(Rect::right))
                .fold(mid.x, f64::max)
                + gap;
            let origin = Point::new(left + label.height(), mid.y + label.width() / 2.0);
            label.draw_upwards_on_page(ctx, origin, INK, id, axis_labels);
            return;
        }
        let left = rims
            .flatten()
            .map(|q| q.x - f.pad)
            .chain(beside.iter().map(|r| r.x))
            .fold(mid.x, f64::min)
            - gap;
        let origin = Point::new(left - label.depth(), mid.y + label.width() / 2.0);
        label.draw_upwards_on_page(ctx, origin, INK, id, axis_labels);
    }
}

/// Returns the two azimuths at which the wall of the cylinder is tangent to the viewing direction, where its vertical
/// silhouette lines stand, or none when the cylinder is seen along its axis.
///
/// The depth of the wall point at azimuth `φ` changes with `φ` as `d_x·cos φ + d_y·sin φ`, where `d_x` and `d_y` are
/// the depths of the unit X and Y directions; the wall turns away from the viewer where that rate is zero.
fn wall_silhouette(p: &Projector) -> Vec<f64> {
    let depth = |n: Vec3| p.project(n).map(|(_, d)| d);
    let (Some(dc), Some(dx), Some(dy)) = (depth([0.0; 3]), depth(X_AXIS), depth(Y_AXIS)) else {
        return Vec::new();
    };
    let (dx, dy) = (dx - dc, dy - dc);
    if dx.hypot(dy) < 1e-9 {
        return Vec::new();
    }
    let facing = dy.atan2(dx);
    vec![facing + FRAC_PI_2, facing - FRAC_PI_2]
}

/// Draws the frame of a spherical axes.
///
/// The equator, a whole circle of the rim radius in the X–Y plane, carries the azimuth tick marks and labels and, when
/// the x grid is on, spokes at the azimuth ticks. The meridian of azimuth 0, a whole circle in the X–Z plane, carries
/// the polar-angle tick marks and labels all the way round: on its half at azimuth 0 and, mirrored with the same
/// values, on its half at azimuth 180°, with the labels at the poles, where the two halves meet, drawn once. When the y
/// grid is on, spokes at the polar-angle ticks are drawn on both halves. The radial axis runs along azimuth 0 in the equator plane with the radial tick marks and labels below it,
/// and when the z grid is on a circle at every radial tick is drawn in both planes.
fn sphere(ctx: &mut Ctx, input: &AxesInput, f: &mut Frame, axis_labels: &mut Vec<Item>) {
    let axes = input.axes;
    let p = f.projector;
    let [xr, yr, _] = *input.ranges;
    let [xt, yt, zt] = &input.decor.ticks;
    let per_unit = p.stage.radians_per_unit(0);
    let polar_per_unit = p.stage.radians_per_unit(1);
    let inclination = matches!(
        p.stage,
        Stage::Sphere {
            inclination: true,
            ..
        }
    );
    let phi = |x: f64| x * per_unit;
    let eps = |y: f64| {
        let angle = y * polar_per_unit;
        if inclination {
            FRAC_PI_2 - angle
        } else {
            angle
        }
    };
    let rho = |r: f64| (p.along_axes([xr.min, yr.min, r])[2] + 0.5).max(0.0);
    let equator = (X_AXIS, Y_AXIS);
    let meridian = (X_AXIS, Z_AXIS);
    let on_equator = |a: f64, q: f64| [0.5 * q * a.cos(), 0.5 * q * a.sin(), 0.0];
    let on_meridian = |e: f64, q: f64| [0.5 * q * e.cos(), 0.0, 0.5 * q * e.sin()];
    let whole = (0.0, TAU);

    if axes.x.grid {
        for &x in &xt.major {
            f.segment(
                Kind::Grid,
                [0.0; 3],
                on_equator(phi(x), 1.0),
                Layering::Interior,
            );
        }
    }
    // The point of the meridian at elevation `e` on the half at azimuth 180°, or `None` at a pole, where both halves
    // meet in the point of the half at azimuth 0.
    let mirrored = |e: f64| (e.cos().abs() > 1e-9).then(|| [-0.5 * e.cos(), 0.0, 0.5 * e.sin()]);
    if axes.y.grid {
        for &y in &yt.major {
            let e = eps(y);
            f.segment(
                Kind::Grid,
                [0.0; 3],
                on_meridian(e, 1.0),
                Layering::Interior,
            );
            if let Some(point) = mirrored(e) {
                f.segment(Kind::Grid, [0.0; 3], point, Layering::Interior);
            }
        }
    }
    if axes.z.grid {
        for &r in &zt.major {
            let q = rho(r);
            if q > 1e-6 && q < 1.0 - 1e-6 {
                f.circle(
                    Kind::Grid,
                    [0.0; 3],
                    equator,
                    0.5 * q,
                    whole,
                    Layering::Interior,
                );
                f.circle(
                    Kind::Grid,
                    [0.0; 3],
                    meridian,
                    0.5 * q,
                    whole,
                    Layering::Interior,
                );
            }
        }
    }
    f.circle(Kind::Lines, [0.0; 3], equator, 0.5, whole, Layering::Split);
    f.circle(Kind::Lines, [0.0; 3], meridian, 0.5, whole, Layering::Split);
    f.segment(Kind::Lines, [0.0; 3], [0.5, 0.0, 0.0], Layering::Interior);

    for (&x, label) in xt.major.iter().zip(&xt.labels) {
        let a = phi(x);
        let point = on_equator(a, 1.0);
        let d = f.outward(point, [a.cos(), a.sin(), 0.0]);
        f.tick(ctx, label, point, (d, true), false);
    }
    for (&y, label) in yt.major.iter().zip(&yt.labels) {
        let e = eps(y);
        let point = on_meridian(e, 1.0);
        let d = f.outward(point, [e.cos(), 0.0, e.sin()]);
        f.tick(ctx, label, point, (d, true), false);
    }
    // The same polar angles on the half at azimuth 180°, after those at azimuth 0, which keep their labels where the
    // two would crowd each other.
    for (&y, label) in yt.major.iter().zip(&yt.labels) {
        let e = eps(y);
        let Some(point) = mirrored(e) else { continue };
        let d = f.outward(point, [-e.cos(), 0.0, e.sin()]);
        f.tick(ctx, label, point, (d, true), false);
    }
    // The radial labels hang below their axis, on the side of it away from the top of the page.
    let (origin, end) = (f.page([0.0; 3]), f.page([0.5, 0.0, 0.0]));
    let mut down = Point::new(0.0, 1.0);
    let mut along = Point::new(1.0, 0.0);
    if let (Some((o, _)), Some((e, _))) = (origin, end) {
        along = unit(Point::new(e.x - o.x, e.y - o.y), Point::new(1.0, 0.0));
        down = if along.x >= 0.0 {
            Point::new(-along.y, along.x)
        } else {
            Point::new(along.y, -along.x)
        };
    }
    let mut last = None;
    let mut radial_boxes = Vec::new();
    for (&r, label) in zt.major.iter().zip(&zt.labels) {
        let q = rho(r);
        if q < 1e-6 {
            continue;
        }
        let point = [0.5 * q, 0.0, 0.0];
        if let Some(bounds) = f.tick(ctx, label, point, (down, true), false) {
            last = Some((bounds, point));
            radial_boxes.push(bounds);
        }
    }
    f.exponent(ctx, zt, last, along, 0.4 * ctx.font_size);

    // Axis labels: the azimuth below the sphere, the polar angle upright to its left and the radius below its axis.
    let fs = ctx.font_size;
    let gap = 0.4 * fs;
    let id = axes.id;
    let decor = input.decor;
    let Some((centre, _)) = origin else { return };
    let radius = 0.5 * p.scale;
    if let Some(label) = &decor.labels[0] {
        let lowest = f.lowest(std::iter::once(Point::new(centre.x, centre.y + radius)));
        let origin = Point::new(
            centre.x - label.width() / 2.0,
            lowest + gap + label.height(),
        );
        label.draw_on_page(ctx, origin, INK, id, axis_labels);
    }
    if let Some(label) = &decor.labels[1] {
        let left = f.leftmost(std::iter::once(Point::new(centre.x - radius, centre.y))) - gap;
        let origin = Point::new(left - label.depth(), centre.y + label.width() / 2.0);
        label.draw_upwards_on_page(ctx, origin, INK, id, axis_labels);
    }
    if let (Some(label), Some((e, _))) = (&decor.labels[2], end) {
        let mid = Point::new((centre.x + e.x) / 2.0, (centre.y + e.y) / 2.0);
        let extent = zt
            .labels
            .iter()
            .map(|l| 2.0 * half_extent(down, l.width(), l.total_height()))
            .fold(0.0, f64::max);
        let offset = f.pad + extent + gap + half_extent(down, label.width(), label.total_height());
        let c = Point::new(mid.x + down.x * offset, mid.y + down.y * offset);
        label.draw_on_page(ctx, label.origin_for_centre(c), INK, id, axis_labels);
    }
}
