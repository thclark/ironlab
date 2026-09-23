//! Path construction helpers.

use ironlab_ir::NodeId;

use crate::display::{
    Depth, Fill, FillRule, Item, ItemKind, LineCap, LineJoin, PathItem, PathSegment, Point, Rect,
    Rgba, Stroke, Transform,
};

/// The distance of the Bézier control points from the ends of a quarter circle of unit radius.
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// Accumulates the segments of a path.
#[derive(Default)]
pub(super) struct PathBuilder {
    segments: Vec<PathSegment>,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_to(&mut self, p: Point) {
        self.segments.push(PathSegment::MoveTo(p));
    }

    pub fn line_to(&mut self, p: Point) {
        self.segments.push(PathSegment::LineTo(p));
    }

    pub fn close(&mut self) {
        self.segments.push(PathSegment::Close);
    }

    /// Adds one subpath through `points`, closed when `closed` is true. Fewer than two points add nothing.
    pub fn polyline(&mut self, points: &[Point], closed: bool) {
        let [first, rest @ ..] = points else {
            return;
        };
        if rest.is_empty() {
            return;
        }
        self.move_to(*first);
        for p in rest {
            self.line_to(*p);
        }
        if closed {
            self.close();
        }
    }

    /// Adds a closed axis-aligned rectangle.
    pub fn rect(&mut self, r: Rect) {
        self.polyline(
            &[
                Point::new(r.x, r.y),
                Point::new(r.right(), r.y),
                Point::new(r.right(), r.bottom()),
                Point::new(r.x, r.bottom()),
            ],
            true,
        );
    }

    /// Adds a closed circle of radius `r` about `c`, as four cubic Bézier arcs.
    pub fn circle(&mut self, c: Point, r: f64) {
        let k = KAPPA * r;
        self.move_to(Point::new(c.x + r, c.y));
        let arcs = [
            ((r, k), (k, r), (0.0, r)),
            ((-k, r), (-r, k), (-r, 0.0)),
            ((-r, -k), (-k, -r), (0.0, -r)),
            ((k, -r), (r, -k), (r, 0.0)),
        ];
        for ((ax, ay), (bx, by), (ex, ey)) in arcs {
            self.segments.push(PathSegment::CubicTo(
                Point::new(c.x + ax, c.y + ay),
                Point::new(c.x + bx, c.y + by),
                Point::new(c.x + ex, c.y + ey),
            ));
        }
        self.close();
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn finish(self) -> Vec<PathSegment> {
        self.segments
    }
}

/// Returns a nonzero fill of `color`.
pub(super) fn fill(color: Rgba) -> Fill {
    Fill {
        color,
        rule: FillRule::NonZero,
    }
}

/// Returns a stroke of `color` and `width` with the given dash array, butt caps and round joins.
pub(super) fn stroke(color: Rgba, width: f64, dash: Vec<f64>) -> Stroke {
    Stroke {
        color,
        width,
        dash,
        dash_offset: 0.0,
        cap: LineCap::Butt,
        join: LineJoin::Round,
    }
}

/// Returns a solid stroke of `color` and `width` with square joins, for axes decorations.
pub(super) fn solid(color: Rgba, width: f64) -> Stroke {
    Stroke {
        join: LineJoin::Miter,
        ..stroke(color, width, Vec::new())
    }
}

/// Wraps path segments in an item, or returns `None` when there is nothing to draw.
pub(super) fn item(
    source: NodeId,
    segments: Vec<PathSegment>,
    fill: Option<Fill>,
    stroke: Option<Stroke>,
) -> Option<Item> {
    if segments.is_empty() || (fill.is_none() && stroke.is_none()) {
        return None;
    }
    Some(Item {
        source: Some(source),
        kind: ItemKind::Path(PathItem {
            segments,
            fill,
            stroke,
            depth: None,
        }),
    })
}

/// Gives a path item the depth at which a 3D axes paints it; an item that is not a path is returned unchanged.
pub(super) fn with_depth(mut item: Item, depth: Depth) -> Item {
    if let ItemKind::Path(path) = &mut item.kind {
        path.depth = Some(depth);
    }
    item
}

/// Returns whether every coordinate of a point is finite.
pub(super) fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

/// Visits the end point of every segment of the path items among `items`, the four corners of every image item and
/// the position of every marker, including those inside groups, in the coordinate space of `items`: a vertex beneath
/// a group that carries a transform is mapped through it.
pub(super) fn for_each_vertex(items: &[Item], visit: &mut dyn FnMut(Point)) {
    fn walk(items: &[Item], transform: Transform, visit: &mut dyn FnMut(Point)) {
        for item in items {
            match &item.kind {
                ItemKind::Path(path) => {
                    for seg in &path.segments {
                        match *seg {
                            PathSegment::MoveTo(p)
                            | PathSegment::LineTo(p)
                            | PathSegment::CubicTo(_, _, p) => visit(transform.apply(p)),
                            PathSegment::Close => {}
                        }
                    }
                }
                ItemKind::Group {
                    transform: inner,
                    items,
                    ..
                } => {
                    let transform = match inner {
                        Some(inner) => inner.then(transform),
                        None => transform,
                    };
                    walk(items, transform, visit);
                }
                ItemKind::Dense { items, .. } | ItemKind::Depth { items } => {
                    walk(items, transform, visit);
                }
                ItemKind::Image(image) => {
                    let r = image.rect;
                    for corner in [
                        Point::new(r.x, r.y),
                        Point::new(r.right(), r.y),
                        Point::new(r.x, r.bottom()),
                        Point::new(r.right(), r.bottom()),
                    ] {
                        visit(transform.apply(corner));
                    }
                }
                ItemKind::Markers(markers) => {
                    for instance in &markers.instances {
                        visit(transform.apply(instance.position));
                    }
                }
                ItemKind::Glyphs(_) => {}
            }
        }
    }
    walk(items, Transform::IDENTITY, visit);
}
