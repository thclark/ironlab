//! Drawing of artists in 2D and 3D axes.
//!
//! Every artist is drawn through a [`Space`], which maps a data point to figure space together with its depth
//! (zero in 2D). Each emitted item carries a depth, so 2D axes can keep the emission order while 3D axes sort all
//! items of all artists back to front.

use ironlab_ir::{
    Artist, Axes, ColorSpec, Contour, ContourPlacement, DashStyle, Levels, Line, Quiver, Scatter,
    ScatterSize, Surface, View3d,
};

use crate::display::{Item, Point, Rect, Rgba};
use crate::hit::AxisMap;
use crate::maths::camera::{Camera, clamp_elevation, fit_to_rect, normalise_box, wrap_azimuth};
use crate::maths::contour::{self, Coords, GridRef};
use crate::maths::quiver;

use super::data::{ArtistData, Points, Prepared};
use super::decor::Decor;
use super::limits::Range;
use super::paths::{self, PathBuilder};
use super::style::{self, ColourScale, Paint};

/// Everything needed to draw one axes.
pub(crate) struct AxesInput<'a> {
    pub axes: &'a Axes,
    pub prepared: &'a [Prepared<'a>],
    pub ranges: &'a [Range; 3],
    pub colours: ColourScale,
    pub decor: &'a Decor,
    pub plot: Rect,
    pub outer: Rect,
}

/// An orthographic projection of the data box of a 3D axes into its plot rectangle.
pub(crate) struct Projector {
    pub camera: Camera,
    lo: [f64; 3],
    hi: [f64; 3],
    log: [bool; 3],
    scale: f64,
    centre: Point,
}

impl Projector {
    /// Builds the projection for a view, fitting the unit box to the plot rectangle independently of the view and
    /// then applying the view's zoom about the plot centre and its pan as fractions of the plot size (positive pan
    /// moves right and down).
    pub fn new(view: View3d, ranges: &[Range; 3], plot: Rect) -> Self {
        let camera = Camera {
            azimuth_deg: wrap_azimuth(view.azimuth_deg),
            elevation_deg: clamp_elevation(view.elevation_deg),
        };
        let zoom = if view.zoom.is_finite() && view.zoom > 0.0 {
            view.zoom
        } else {
            1.0
        };
        let finite_or_zero = |p: f64| if p.is_finite() { p } else { 0.0 };
        let (pan_x, pan_y) = (finite_or_zero(view.pan_x), finite_or_zero(view.pan_y));
        let (fit, offset) = fit_to_rect(plot.width, plot.height);
        Self {
            camera,
            lo: ranges.map(|r| r.min),
            hi: ranges.map(|r| r.max),
            log: ranges.map(|r| r.log),
            scale: fit * zoom,
            centre: Point::new(
                plot.x + offset[0] + pan_x * plot.width,
                plot.y + offset[1] + pan_y * plot.height,
            ),
        }
    }

    /// Maps a data point into the normalised box `[-0.5, 0.5]³`.
    pub fn normalise(&self, p: [f64; 3]) -> [f64; 3] {
        normalise_box(p, self.lo, self.hi, self.log)
    }

    /// Projects a point of the normalised box to figure space, returning its depth as well.
    pub fn project(&self, n: [f64; 3]) -> Option<(Point, f64)> {
        if !n.iter().all(|c| c.is_finite()) {
            return None;
        }
        let projected = self.camera.project(n);
        let p = Point::new(
            self.centre.x + self.scale * projected.screen[0],
            self.centre.y - self.scale * projected.screen[1],
        );
        (paths::finite(p) && projected.depth.is_finite()).then_some((p, projected.depth))
    }
}

/// The mapping from data space to figure space.
pub(crate) enum Space<'a> {
    TwoD { x: AxisMap, y: AxisMap },
    ThreeD(&'a Projector),
}

impl Space<'_> {
    /// Maps a data point to figure space and depth, or returns `None` when it cannot be placed.
    pub fn map(&self, p: [f64; 3]) -> Option<(Point, f64)> {
        match self {
            Space::TwoD { x, y } => {
                if !(p[0].is_finite() && p[1].is_finite()) {
                    return None;
                }
                let q = Point::new(x.to_figure(p[0]), y.to_figure(p[1]));
                paths::finite(q).then_some((q, 0.0))
            }
            Space::ThreeD(projector) => projector.project(projector.normalise(p)),
        }
    }

    fn is_3d(&self) -> bool {
        matches!(self, Space::ThreeD(_))
    }
}

/// A drawable item with the depth at which it is sorted in 3D.
pub(crate) type Prim = (f64, Item);

/// State shared while drawing the artists of one axes.
struct Draw<'a> {
    space: &'a Space<'a>,
    scale: ColourScale,
    /// The lower z limit, at which planar contours without an explicit height are drawn.
    z_bottom: f64,
    out: Vec<Prim>,
}

impl Draw<'_> {
    fn push(&mut self, depth: f64, item: Option<Item>) {
        if let Some(item) = item {
            self.out.push((depth, item));
        }
    }
}

/// Draws every visible artist of an axes in artist order, returning items tagged with their depths.
pub(super) fn draw_artists(input: &AxesInput, primaries: &[Paint], space: &Space) -> Vec<Prim> {
    let mut draw = Draw {
        space,
        scale: input.colours,
        z_bottom: input.ranges[2].min,
        out: Vec::new(),
    };
    for (prepared, primary) in input.prepared.iter().zip(primaries) {
        let Some(data) = prepared.data else { continue };
        if !prepared.artist.visible() {
            continue;
        }
        match (prepared.artist, data) {
            (Artist::Line(line), ArtistData::Line(points)) => {
                draw_line(&mut draw, line, points, *primary)
            }
            (
                Artist::Scatter(scatter),
                ArtistData::Scatter {
                    points,
                    sizes,
                    colours,
                },
            ) => draw_scatter(&mut draw, scatter, points, sizes, colours, *primary),
            (Artist::Contour(c), ArtistData::Contour(grid)) => draw_contour(&mut draw, c, &grid),
            (
                Artist::Quiver(q),
                ArtistData::Quiver {
                    points,
                    vectors,
                    scale,
                },
            ) => draw_quiver(&mut draw, q, points, vectors, scale, *primary),
            (Artist::Surface(s), ArtistData::Surface { grid, colours }) => {
                draw_surface(&mut draw, s, &grid, colours)
            }
            _ => {}
        }
    }
    draw.out
}

/// Returns the mean of the depths of mapped points.
fn mean_depth(points: &[(Point, f64)]) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    points.iter().map(|(_, d)| d).sum::<f64>() / points.len() as f64
}

/// Splits a sequence of data points into runs of consecutive points that can be placed.
fn mapped_runs(space: &Space, points: impl Iterator<Item = [f64; 3]>) -> Vec<Vec<(Point, f64)>> {
    let mut runs = vec![Vec::new()];
    for p in points {
        match space.map(p) {
            Some(q) => runs.last_mut().expect("runs is never empty").push(q),
            None => {
                if !runs.last().expect("runs is never empty").is_empty() {
                    runs.push(Vec::new());
                }
            }
        }
    }
    runs.retain(|r| r.len() >= 2);
    runs
}

/// Returns the stroke for a line style and colour, or `None` when no line is drawn.
fn line_stroke(
    style: &ironlab_ir::LineStyle,
    colour: Option<Rgba>,
) -> Option<crate::display::Stroke> {
    let colour = colour?;
    let width = style::width_or(style.width_pt, 0.75);
    let dash = style::dash_array(style.dash, width)?;
    Some(paths::stroke(colour, width, dash))
}

/// Draws a polyline through the finite points of a line, followed by its markers.
fn draw_line(draw: &mut Draw, line: &Line, points: Points, primary: Paint) {
    let colour = primary.single(&draw.scale);
    if let Some(stroke) = line_stroke(&line.line, colour) {
        let runs = mapped_runs(draw.space, (0..points.len()).map(|i| points.get(i)));
        if draw.space.is_3d() {
            for run in runs {
                let mut b = PathBuilder::new();
                b.polyline(&run.iter().map(|(p, _)| *p).collect::<Vec<_>>(), false);
                let item = paths::item(line.id, b.finish(), None, Some(stroke.clone()));
                draw.push(mean_depth(&run), item);
            }
        } else {
            let mut b = PathBuilder::new();
            for run in &runs {
                b.polyline(&run.iter().map(|(p, _)| *p).collect::<Vec<_>>(), false);
            }
            draw.push(0.0, paths::item(line.id, b.finish(), None, Some(stroke)));
        }
    }
    let width = style::width_or(line.line.width_pt, 0.75).clamp(0.5, 1.5);
    for i in 0..points.len() {
        if let Some((p, depth)) = draw.space.map(points.get(i)) {
            let item = style::marker_item(
                line.id,
                &line.marker,
                p,
                line.marker.size_pt,
                colour,
                width,
                1.0,
            );
            draw.push(depth, item);
        }
    }
}

/// Draws one marker per placeable point of a scatter.
fn draw_scatter(
    draw: &mut Draw,
    scatter: &Scatter,
    points: Points,
    sizes: Option<&[f64]>,
    colours: Option<&[f64]>,
    primary: Paint,
) {
    let scalar_size = match scatter.size {
        ScatterSize::Scalar { value } => value,
        ScatterSize::Data { .. } => scatter.marker.size_pt,
    };
    for i in 0..points.len() {
        let colour = match colours {
            Some(values) => match draw.scale.colour(values[i]) {
                Some(c) => Some(c),
                None => continue,
            },
            None => primary.single(&draw.scale),
        };
        let size = sizes.map_or(scalar_size, |s| s[i]);
        if let Some((p, depth)) = draw.space.map(points.get(i)) {
            let item = style::marker_item(scatter.id, &scatter.marker, p, size, colour, 0.5, 1.0);
            draw.push(depth, item);
        }
    }
}

/// Returns the finite range of a field's values.
fn field_range(values: &[f64]) -> Option<(f64, f64)> {
    let (lo, hi) = values
        .iter()
        .filter(|v| v.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(*v), hi.max(*v))
        });
    (lo <= hi).then_some((lo, hi))
}

/// Returns the contour levels of an artist for a field spanning `[zmin, zmax]`, sorted and without duplicates.
pub(super) fn contour_levels(levels: &Levels, zmin: f64, zmax: f64) -> Vec<f64> {
    let mut out = match levels {
        Levels::Auto { count } => contour::auto_levels(zmin, zmax, (*count).max(1) as usize),
        Levels::Explicit { values } => values.iter().copied().filter(|v| v.is_finite()).collect(),
    };
    out.sort_by(f64::total_cmp);
    out.dedup();
    out
}

/// Draws the filled bands and isolines of a contour.
fn draw_contour(draw: &mut Draw, c: &Contour, grid: &GridRef) {
    let Some((zmin, zmax)) = field_range(grid.z) else {
        return;
    };
    let levels = contour_levels(&c.levels, zmin, zmax);
    let plane = match c.placement {
        ContourPlacement::Plane { z: Some(z) } => z,
        _ => draw.z_bottom,
    };
    if c.fill {
        for (lo, hi) in contour::band_edges(&levels, zmin, zmax) {
            let mid = (lo.max(zmin) + hi.min(zmax)) / 2.0;
            let Some(colour) = draw.scale.colour(mid) else {
                continue;
            };
            let mut b = PathBuilder::new();
            let mut mapped_all = Vec::new();
            for polygon in contour::isobands(grid, lo, hi) {
                let mapped: Option<Vec<(Point, f64)>> = polygon
                    .iter()
                    .map(|q| draw.space.map([q[0], q[1], plane]))
                    .collect();
                let Some(mapped) = mapped else { continue };
                b.polyline(&mapped.iter().map(|(p, _)| *p).collect::<Vec<_>>(), true);
                mapped_all.extend(mapped);
            }
            let item = paths::item(c.id, b.finish(), Some(paths::fill(colour)), None);
            draw.push(mean_depth(&mapped_all), item);
        }
    }
    let scale = draw.scale;
    let line_colour = |level: f64| match c.line.color {
        ColorSpec::Rgba { color } => Some(style::ir_colour(color)),
        ColorSpec::None => None,
        ColorSpec::Auto | ColorSpec::Colormapped if !c.fill => scale.colour(level),
        ColorSpec::Auto | ColorSpec::Colormapped => None,
    };
    for level in levels {
        let Some(stroke) = line_stroke(&c.line, line_colour(level)) else {
            continue;
        };
        let z = match c.placement {
            ContourPlacement::AtLevel => level,
            ContourPlacement::Plane { .. } => plane,
        };
        let mut b = PathBuilder::new();
        for polyline in contour::isolines(grid, level) {
            let points = polyline.points.iter().map(|q| [q[0], q[1], z]);
            let runs = mapped_runs(draw.space, points);
            let closed =
                polyline.closed && runs.len() == 1 && runs[0].len() == polyline.points.len();
            for run in runs {
                let pts: Vec<Point> = run.iter().map(|(p, _)| *p).collect();
                if draw.space.is_3d() {
                    let mut single = PathBuilder::new();
                    single.polyline(&pts, closed);
                    let item = paths::item(c.id, single.finish(), None, Some(stroke.clone()));
                    draw.push(mean_depth(&run), item);
                } else {
                    b.polyline(&pts, closed);
                }
            }
        }
        if !b.is_empty() {
            draw.push(0.0, paths::item(c.id, b.finish(), None, Some(stroke)));
        }
    }
}

/// Draws one arrow per quiver vector whose base and components are finite.
fn draw_quiver(
    draw: &mut Draw,
    q: &Quiver,
    points: Points,
    vectors: Points,
    scale: f64,
    primary: Paint,
) {
    let colour = primary.single(&draw.scale);
    let style = ironlab_ir::LineStyle {
        dash: match q.line.dash {
            DashStyle::None => DashStyle::None,
            _ => DashStyle::Solid,
        },
        ..q.line
    };
    let Some(stroke) = line_stroke(&style, colour) else {
        return;
    };
    let head = if q.head_size.is_finite() && q.head_size >= 0.0 {
        q.head_size
    } else {
        0.3
    };
    for i in 0..points.len() {
        let (base, v) = (points.get(i), vectors.get(i));
        if !base.iter().chain(&v).all(|c| c.is_finite()) {
            continue;
        }
        let tip = [0, 1, 2].map(|a| base[a] + scale * v[a]);
        let arrow = match draw.space {
            Space::TwoD { .. } => {
                let (Some((b, _)), Some((t, _))) = (draw.space.map(base), draw.space.map(tip))
                else {
                    continue;
                };
                let a = quiver::arrow([b.x, b.y, 0.0], [t.x - b.x, t.y - b.y, 0.0], 1.0, head);
                let at = |p: [f64; 3]| Some((Point::new(p[0], p[1]), 0.0));
                [
                    at(a.shaft[0]),
                    at(a.shaft[1]),
                    at(a.head[0]),
                    at(a.head[1]),
                    at(a.head[2]),
                ]
            }
            Space::ThreeD(projector) => {
                let (nb, nt) = (projector.normalise(base), projector.normalise(tip));
                let a = quiver::arrow(nb, [0, 1, 2].map(|k| nt[k] - nb[k]), 1.0, head);
                [a.shaft[0], a.shaft[1], a.head[0], a.head[1], a.head[2]]
                    .map(|p| projector.project(p))
            }
        };
        let Some(arrow) = arrow.into_iter().collect::<Option<Vec<_>>>() else {
            continue;
        };
        let mut b = PathBuilder::new();
        b.polyline(&[arrow[0].0, arrow[1].0], false);
        b.polyline(&[arrow[2].0, arrow[3].0, arrow[4].0], false);
        let depth = (arrow[0].1 + arrow[1].1) / 2.0;
        draw.push(
            depth,
            paths::item(q.id, b.finish(), None, Some(stroke.clone())),
        );
    }
}

/// Returns the physical x and y coordinates of grid node `(i, j)`.
fn node_xy(grid: &GridRef, i: usize, j: usize) -> [f64; 2] {
    match grid.coords {
        Coords::Rectilinear { x, y } => [x[i], y[j]],
        Coords::Curvilinear { x, y } => [x[j * grid.nx + i], y[j * grid.nx + i]],
    }
}

/// Draws one flat face per grid cell of a surface.
fn draw_surface(draw: &mut Draw, s: &Surface, grid: &GridRef, colours: Option<&[f64]>) {
    let values = colours.unwrap_or(grid.z);
    let width = style::width_or(s.edge_width_pt, 0.5);
    for j in 0..grid.ny - 1 {
        for i in 0..grid.nx - 1 {
            let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            let index = |(ci, cj): (usize, usize)| cj * grid.nx + ci;
            if corners
                .iter()
                .any(|c| grid.z[index(*c)].is_nan() || values[index(*c)].is_nan())
            {
                continue;
            }
            let mean = corners.iter().map(|c| values[index(*c)]).sum::<f64>() / 4.0;
            let mapped: Option<Vec<(Point, f64)>> = corners
                .iter()
                .map(|&(ci, cj)| {
                    let [x, y] = node_xy(grid, ci, cj);
                    draw.space.map([x, y, grid.z[index((ci, cj))]])
                })
                .collect();
            let Some(mapped) = mapped else { continue };
            let paint = |spec: ColorSpec| match style::resolve(spec, Paint::Colormapped) {
                Paint::Fixed(c) => c,
                Paint::Colormapped => draw.scale.colour(mean),
            };
            let fill = paint(s.face).map(paths::fill);
            let stroke = paint(s.edge).map(|c| paths::stroke(c, width, Vec::new()));
            let mut b = PathBuilder::new();
            b.polyline(&mapped.iter().map(|(p, _)| *p).collect::<Vec<_>>(), true);
            draw.push(
                mean_depth(&mapped),
                paths::item(s.id, b.finish(), fill, stroke),
            );
        }
    }
}
