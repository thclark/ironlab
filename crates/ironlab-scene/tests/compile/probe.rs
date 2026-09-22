//! Inspection of compiled scenes: display list leaves in figure space, their bounding boxes, and
//! the axes geometry published in the hit map.

use ironlab_ir::NodeId;
use ironlab_scene::Scene;
use ironlab_scene::display::{
    GlyphsItem, ImageItem, ItemKind, PathItem, PathSegment, Point, Rect, Rgba, Transform,
};
use ironlab_scene::hit::{AxesHit, AxesHitKind, AxisMap};
use ironlab_text::FontId;
use kurbo::Shape;

use crate::common::engine;

/// A leaf of the display list together with where it ends up in figure space.
#[derive(Clone, Debug)]
pub struct Leaf {
    pub source: Option<NodeId>,
    pub kind: ItemKind,
    /// The accumulated transform from item space to figure space.
    pub transform: Transform,
    /// The intersection of all enclosing clips, in figure space.
    pub clip: Option<Rect>,
}

/// Collects every leaf of the scene's display list in paint order.
pub fn leaves(scene: &Scene) -> Vec<Leaf> {
    let mut out = Vec::new();
    scene.display_list.visit_leaves(|item, transform, clip| {
        out.push(Leaf {
            source: item.source,
            kind: item.kind.clone(),
            transform,
            clip,
        });
    });
    out
}

/// Returns the leaves produced by a node, in paint order.
pub fn from_source(leaves: &[Leaf], id: NodeId) -> Vec<Leaf> {
    leaves
        .iter()
        .filter(|l| l.source == Some(id))
        .cloned()
        .collect()
}

impl Leaf {
    pub fn path(&self) -> Option<&PathItem> {
        match &self.kind {
            ItemKind::Path(p) => Some(p),
            _ => None,
        }
    }

    pub fn glyphs(&self) -> Option<&GlyphsItem> {
        match &self.kind {
            ItemKind::Glyphs(g) => Some(g),
            _ => None,
        }
    }

    pub fn image(&self) -> Option<&ImageItem> {
        match &self.kind {
            ItemKind::Image(i) => Some(i),
            _ => None,
        }
    }

    fn to_figure(&self, p: Point) -> Point {
        self.transform.apply(p)
    }

    /// Returns the on-curve points of each subpath of a path leaf, in figure space. A closing
    /// vertex that repeats the first vertex is dropped.
    pub fn subpaths(&self) -> Vec<Vec<Point>> {
        let Some(path) = self.path() else {
            return Vec::new();
        };
        let mut out: Vec<Vec<Point>> = Vec::new();
        for seg in &path.segments {
            match *seg {
                PathSegment::MoveTo(p) => out.push(vec![self.to_figure(p)]),
                PathSegment::LineTo(p) | PathSegment::CubicTo(_, _, p) => {
                    if out.is_empty() {
                        out.push(Vec::new());
                    }
                    out.last_mut().unwrap().push(self.to_figure(p));
                }
                PathSegment::Close => {}
            }
        }
        for sub in &mut out {
            if sub.len() > 1 && points_close(sub[0], *sub.last().unwrap(), 1e-9) {
                sub.pop();
            }
        }
        out
    }

    /// Returns the number of subpaths started with `MoveTo`.
    pub fn move_to_count(&self) -> usize {
        self.path().map_or(0, |p| {
            p.segments
                .iter()
                .filter(|s| matches!(s, PathSegment::MoveTo(_)))
                .count()
        })
    }

    /// Returns every straight segment of a path leaf in figure space, including closing segments.
    pub fn line_segments(&self) -> Vec<(Point, Point)> {
        let Some(path) = self.path() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut start = None;
        let mut current = None;
        for seg in &path.segments {
            match *seg {
                PathSegment::MoveTo(p) => {
                    start = Some(p);
                    current = Some(p);
                }
                PathSegment::LineTo(p) => {
                    if let Some(c) = current {
                        out.push((self.to_figure(c), self.to_figure(p)));
                    }
                    current = Some(p);
                }
                PathSegment::CubicTo(_, _, p) => current = Some(p),
                PathSegment::Close => {
                    if let (Some(s), Some(c)) = (start, current)
                        && !points_close(s, c, 1e-12)
                    {
                        out.push((self.to_figure(c), self.to_figure(s)));
                    }
                    current = start;
                }
            }
        }
        out
    }

    /// Returns the bounding box of the leaf in figure space, ignoring clipping.
    ///
    /// For paths this is the box of all control points, which contains the curve. For glyph runs it
    /// is the union of the glyph outline boxes (from the text engine), or of the glyph origins for
    /// glyphs without outlines, so it follows the ink rather than a nominal line height. For an image
    /// it is the box of the four corners of its rectangle, which the transform of a 3D axes may turn
    /// into a parallelogram.
    pub fn bbox(&self) -> Option<Rect> {
        match &self.kind {
            ItemKind::Path(path) => {
                let points = path.segments.iter().flat_map(|s| match *s {
                    PathSegment::MoveTo(p) | PathSegment::LineTo(p) => vec![p],
                    PathSegment::CubicTo(a, b, c) => vec![a, b, c],
                    PathSegment::Close => vec![],
                });
                bbox_of(points.map(|p| self.to_figure(p)))
            }
            ItemKind::Glyphs(run) => {
                let mut corners = Vec::new();
                for g in &run.glyphs {
                    match engine().glyph_outline(run.font, g.id) {
                        Some(outline) => {
                            let b = outline.bounding_box();
                            for (qx, qy) in [(b.x0, b.y0), (b.x1, b.y0), (b.x0, b.y1), (b.x1, b.y1)]
                            {
                                let local =
                                    Point::new(g.x + run.size_pt * qx, g.y + run.size_pt * qy);
                                corners.push(self.to_figure(local));
                            }
                        }
                        None => corners.push(self.to_figure(Point::new(g.x, g.y))),
                    }
                }
                bbox_of(corners)
            }
            ItemKind::Image(image) => {
                let r = image.rect;
                let corners = [
                    Point::new(r.x, r.y),
                    Point::new(r.right(), r.y),
                    Point::new(r.x, r.bottom()),
                    Point::new(r.right(), r.bottom()),
                ];
                bbox_of(corners.map(|p| self.to_figure(p)))
            }
            // Groups, dense and depth ones included, are descended into by `visit_leaves` and never become
            // leaves.
            ItemKind::Group { .. } | ItemKind::Dense { .. } | ItemKind::Depth { .. } => None,
        }
    }

    /// Returns the part of the bounding box that survives clipping.
    pub fn visible_bbox(&self) -> Option<Rect> {
        let b = self.bbox()?;
        match self.clip {
            Some(c) => intersect(b, c),
            None => Some(b),
        }
    }

    /// Returns the colours this leaf paints with (fill, stroke or glyph colour).
    pub fn colors(&self) -> Vec<Rgba> {
        match &self.kind {
            ItemKind::Path(p) => p
                .fill
                .map(|f| f.color)
                .into_iter()
                .chain(p.stroke.as_ref().map(|s| s.color))
                .collect(),
            ItemKind::Glyphs(g) => vec![g.color],
            ItemKind::Image(_)
            | ItemKind::Group { .. }
            | ItemKind::Dense { .. }
            | ItemKind::Depth { .. } => Vec::new(),
        }
    }

    /// Returns whether the leaf's transform turns horizontal text to read upwards on the page, which
    /// is how a y label is set.
    pub fn reads_upwards(&self) -> bool {
        let origin = self.transform.apply(Point::new(0.0, 0.0));
        let ahead = self.transform.apply(Point::new(1.0, 0.0));
        (ahead.x - origin.x).abs() < 1e-6 && ahead.y - origin.y < -0.999
    }
}

/// Returns the bounding box of a set of points.
pub fn bbox_of(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
    let mut it = points.into_iter();
    let first = it.next()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for p in it {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
}

pub fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(
        x,
        y,
        a.right().max(b.right()) - x,
        a.bottom().max(b.bottom()) - y,
    )
}

/// Returns the intersection of two rectangles, or `None` if they do not overlap.
pub fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let r = a.right().min(b.right());
    let bt = a.bottom().min(b.bottom());
    (r >= x && bt >= y).then(|| Rect::new(x, y, r - x, bt - y))
}

/// Returns whether two rectangles have no interior in common.
pub fn disjoint(a: Rect, b: Rect) -> bool {
    a.right() <= b.x || b.right() <= a.x || a.bottom() <= b.y || b.bottom() <= a.y
}

/// Returns whether `inner` lies inside `outer`, allowing `tol` points of overhang.
pub fn inside(inner: Rect, outer: Rect, tol: f64) -> bool {
    inner.x >= outer.x - tol
        && inner.y >= outer.y - tol
        && inner.right() <= outer.right() + tol
        && inner.bottom() <= outer.bottom() + tol
}

pub fn centre(r: Rect) -> Point {
    Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
}

pub fn points_close(a: Point, b: Point, tol: f64) -> bool {
    (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol
}

/// Returns the signed area of a polygon (shoelace formula).
pub fn signed_area(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

/// Returns every glyph run leaf together with its run.
pub fn glyph_runs(leaves: &[Leaf]) -> Vec<(&Leaf, &GlyphsItem)> {
    leaves
        .iter()
        .filter_map(|l| l.glyphs().map(|g| (l, g)))
        .collect()
}

/// Returns the union of the bounding boxes of all glyph runs whose text is exactly `text`.
pub fn text_bbox(leaves: &[Leaf], text: &str) -> Option<Rect> {
    glyph_runs(leaves)
        .into_iter()
        .filter(|(_, g)| g.text == text)
        .filter_map(|(l, _)| l.bbox())
        .reduce(union)
}

/// Returns the glyph run leaves whose text is exactly `text`.
pub fn runs_with_text<'a>(leaves: &'a [Leaf], text: &str) -> Vec<&'a Leaf> {
    glyph_runs(leaves)
        .into_iter()
        .filter(|(_, g)| g.text == text)
        .map(|(l, _)| l)
        .collect()
}

/// Parses a tick label, accepting U+2212 MINUS SIGN as the sign.
pub fn parse_number(text: &str) -> Option<f64> {
    text.replace('\u{2212}', "-").parse().ok()
}

/// A plain numeric tick label: its text, value and bounding box in figure space.
#[derive(Clone, Debug)]
pub struct NumericLabel {
    pub text: String,
    pub value: f64,
    pub bbox: Rect,
}

/// Returns the plain-text numeric runs lying entirely below `plot` (x tick labels).
pub fn x_tick_labels(leaves: &[Leaf], plot: Rect) -> Vec<NumericLabel> {
    numeric_labels(leaves)
        .into_iter()
        .filter(|l| l.bbox.y >= plot.bottom() - 1e-6)
        .collect()
}

/// Returns the plain-text numeric runs lying entirely left of `plot` (y tick labels).
pub fn y_tick_labels(leaves: &[Leaf], plot: Rect) -> Vec<NumericLabel> {
    numeric_labels(leaves)
        .into_iter()
        .filter(|l| l.bbox.right() <= plot.x + 1e-6)
        .collect()
}

fn numeric_labels(leaves: &[Leaf]) -> Vec<NumericLabel> {
    glyph_runs(leaves)
        .into_iter()
        .filter(|(_, g)| g.font != FontId::Math)
        .filter_map(|(l, g)| {
            Some(NumericLabel {
                text: g.text.clone(),
                value: parse_number(&g.text)?,
                bbox: l.bbox()?,
            })
        })
        .collect()
}

/// Returns the hit geometry of an axes.
pub fn axes_hit(scene: &Scene, id: NodeId) -> &AxesHit {
    scene
        .hit_map
        .axes
        .iter()
        .find(|a| a.id == id)
        .unwrap_or_else(|| panic!("hit map has no entry for axes {id}"))
}

/// Returns the x and y axis maps of a 2D axes.
pub fn axis_maps(scene: &Scene, id: NodeId) -> (AxisMap, AxisMap) {
    match axes_hit(scene, id).kind {
        AxesHitKind::TwoD { x, y } => (x, y),
        AxesHitKind::ThreeD => panic!("axes {id} is three-dimensional"),
    }
}

#[track_caller]
pub fn assert_close(actual: f64, expected: f64, tol: f64) {
    assert!(
        (actual - expected).abs() <= tol,
        "expected {expected} ± {tol}, got {actual}"
    );
}

/// Returns the red, green, blue and alpha of the pixel in row `row` and column `column` of an image,
/// reading the alpha as 255 from an image without an alpha channel.
///
/// The samples of an image run row by row from row 0, so the pixel starts at index
/// `(row · width + column) · channels`.
#[track_caller]
pub fn pixel(image: &ImageItem, row: usize, column: usize) -> [u8; 4] {
    assert!(
        row < image.height as usize && column < image.width as usize,
        "pixel ({row}, {column}) lies inside an image of {} rows and {} columns",
        image.height,
        image.width
    );
    let channels = usize::from(image.channels);
    let start = (row * image.width as usize + column) * channels;
    let s = &image.samples[start..start + channels];
    [s[0], s[1], s[2], if channels == 4 { s[3] } else { 255 }]
}
