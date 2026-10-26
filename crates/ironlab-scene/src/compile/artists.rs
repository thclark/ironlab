//! Drawing of artists in 2D and 3D axes.
//!
//! Every artist is drawn through a [`Space`], which maps a data point to figure space together with its depth
//! (zero in 2D). Each emitted item carries a depth, so 2D axes can keep the emission order while 3D axes sort all
//! items of all artists back to front.

use std::collections::BTreeMap;
use std::sync::Arc;

use ironlab_ir::{
    Artist, Axes, ColorScaleNorm, ColorSpec, Contour, ContourPlacement, Coords as ArtistCoords,
    DashStyle, Levels, NodeId, Quiver, Series, Surface, View3d,
};

use crate::display::{
    Depth, DepthPlane, ImageItem, Item, ItemKind, Point, Rect, Rgba, Stroke, Transform,
};
use crate::hit::{ArtistHit, AxisMap, BarHit, CellHit, ImageHit, StepHit};
use crate::maths::camera::{
    Camera, EDGE_DEPTH_LIFT, FACE_DEPTH_BIAS, Plane, back_planes, clamp_elevation, depth_plane,
    fit_to_rect, normalise_box, wrap_azimuth,
};
use crate::maths::contour::{self, Coords, GridRef};
use crate::maths::decimate::{self, Sample};
use crate::maths::{polygon, quiver};

use super::Ctx;
use super::data::{ArtistData, ImageData, Points, Prepared, SeriesColours};
use super::decor::Decor;
use super::image;
use super::limits::Range;
use super::paths::{self, PathBuilder};
use super::style::{self, ColourScales, Paint, ResolvedScale};

/// Everything needed to draw one axes.
pub(crate) struct AxesInput<'a> {
    pub axes: &'a Axes,
    pub prepared: &'a [Prepared<'a>],
    pub ranges: &'a [Range; 3],
    /// The colour scales of the figure, resolved once for the whole compilation.
    pub scales: &'a ColourScales<'a>,
    pub decor: &'a Decor,
    pub plot: Rect,
    pub outer: Rect,
    /// The whole page, which bounds what an artist that is not clipped to the plot area can paint.
    pub page: Rect,
}

/// An orthographic projection of the data box of a 3D axes into its plot rectangle.
pub(crate) struct Projector {
    pub camera: Camera,
    lo: [f64; 3],
    hi: [f64; 3],
    log: [bool; 3],
    /// Whether each axis is reversed, which mirrors the normalised box along it.
    reverse: [bool; 3],
    /// The relative lengths of the sides of the box along x, y and z, the longest being 1.
    sides: [f64; 3],
    scale: f64,
    centre: Point,
}

impl Projector {
    /// Builds the projection for a view, fitting the box to the plot rectangle independently of the view and then
    /// applying the view's zoom about the plot centre and its pan as fractions of the plot size (positive pan moves
    /// right and down).
    ///
    /// The box has sides of the relative lengths `sides` along x, y and z, the longest being 1: a cube for an
    /// automatic aspect, and the proportions of the aspect otherwise. It is fitted by the sphere that circumscribes it,
    /// whose projection is the same disc in every view, so that rotating the view never changes the size or the
    /// proportions of the box and every view of it fits the plot rectangle at a zoom of 1.
    pub fn new(view: View3d, ranges: &[Range; 3], plot: Rect, sides: [f64; 3]) -> Self {
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
        let usable = sides.iter().all(|s| s.is_finite() && *s > 0.0);
        let sides = if usable { sides } else { [1.0; 3] };
        // The unit cube has a diagonal of √3, for which `fit_to_rect` is computed; a box of other sides is fitted by
        // its own diagonal.
        let diagonal = sides.iter().map(|s| s * s).sum::<f64>().sqrt();
        let fit = fit * 3f64.sqrt() / diagonal;
        Self {
            camera,
            lo: ranges.map(|r| r.min),
            hi: ranges.map(|r| r.max),
            log: ranges.map(|r| r.log),
            reverse: ranges.map(|r| r.reverse),
            sides,
            scale: fit * zoom,
            centre: Point::new(
                plot.x + offset[0] + pan_x * plot.width,
                plot.y + offset[1] + pan_y * plot.height,
            ),
        }
    }

    /// Maps a data point into the normalised box `[-0.5, 0.5]³`, in which the lower limit of an axis is at −0.5 and
    /// its upper limit at 0.5, or the other way round when the axis is reversed, so that every artist, tick, grid
    /// line and label placed through the box is mirrored along a reversed axis alike.
    pub fn normalise(&self, p: [f64; 3]) -> [f64; 3] {
        let n = normalise_box(p, self.lo, self.hi, self.log);
        std::array::from_fn(|k| if self.reverse[k] { -n[k] } else { n[k] })
    }

    /// Projects a point of the normalised box to figure space, returning its depth as well.
    pub fn project(&self, n: [f64; 3]) -> Option<(Point, f64)> {
        if !n.iter().all(|c| c.is_finite()) {
            return None;
        }
        let projected = self
            .camera
            .project(std::array::from_fn(|k| n[k] * self.sides[k]));
        let p = Point::new(
            self.centre.x + self.scale * projected.screen[0],
            self.centre.y - self.scale * projected.screen[1],
        );
        (paths::finite(p) && projected.depth.is_finite()).then_some((p, projected.depth))
    }
}

/// The mapping from data space to figure space.
#[derive(Clone, Copy)]
pub(crate) enum Space<'a> {
    TwoD { x: AxisMap, y: AxisMap },
    ThreeD(&'a Projector),
}

impl Space<'_> {
    /// Maps the point at index `i` of a series to a sample, or returns `None` when it cannot be placed.
    fn sample(&self, source_index: usize, p: [f64; 3]) -> Option<Sample> {
        let (position, depth) = self.map(p)?;
        Some(Sample {
            source_index,
            position,
            depth,
        })
    }

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

    /// Returns this space with each dimension of a 2D axes whose values are fractions of the plot area mapped onto
    /// the plot area instead of through the axis.
    ///
    /// A fraction maps 0 to the left or bottom edge of `plot` and 1 to its right or top edge, linearly, whatever the
    /// limits, scale or direction of the axis, so it is an [`AxisMap`] over `[0, 1]` laid on the physical extent of
    /// the plot area. A data dimension keeps the axis's own map, which is the single place where the limits, scale
    /// and direction of the axis are applied. A 3D axes has no plot area to take a fraction of, so its space is
    /// returned unchanged (validation reports such an artist as an error).
    pub fn blended(self, coords: [ArtistCoords; 2], plot: Rect) -> Self {
        let Space::TwoD { x, y } = self else {
            return self;
        };
        let fraction = |start: f64, end: f64| AxisMap {
            min: 0.0,
            max: 1.0,
            log: false,
            start,
            end,
        };
        Space::TwoD {
            x: match coords[0] {
                ArtistCoords::Data => x,
                ArtistCoords::AxesFraction => fraction(plot.x, plot.right()),
            },
            y: match coords[1] {
                ArtistCoords::Data => y,
                ArtistCoords::AxesFraction => fraction(plot.bottom(), plot.y),
            },
        }
    }
}

/// A drawable item with the depth at which it is sorted in 3D.
pub(crate) type Prim = (f64, Item);

/// Everything the artists of one axes drew.
pub(crate) struct Drawn {
    /// The items, tagged with the depths at which a 3D axes sorts them.
    pub prims: Vec<Prim>,
    /// The points each series drew, for picking and datatips.
    pub hits: Vec<ArtistHit>,
    /// The placement of each image drawn in a 2D axes, for finding the pixel under a pointer.
    pub images: Vec<ImageHit>,
    /// Every bar drawn, for finding the bar under a pointer.
    pub bars: Vec<BarHit>,
    /// Every step of stairs drawn, for finding the step under a pointer.
    pub steps: Vec<StepHit>,
    /// Every cell drawn, for finding the cell under a pointer.
    pub cells: Vec<CellHit>,
    /// The number of data cells drawn by each artist whose geometry a backend may rasterise, by artist.
    ///
    /// The count is of the cells the artist actually drew, not of the cells its data holds. A backend thresholds on
    /// it to decide whether drawing the artist as vector geometry is worth the file size, so what matters is the
    /// geometry that would be drawn: a cell dropped for a NaN corner, or one removed by decimation, costs nothing
    /// and must not count towards rasterising the rest.
    pub dense: BTreeMap<NodeId, u64>,
}

/// State shared while drawing the artists of one axes.
pub(super) struct Draw<'a, 'f> {
    /// The compilation, to which an artist skipped while it is drawn is reported.
    ctx: &'a mut Ctx<'f>,
    /// The mapping of the artist being drawn: the axes' own, or a blend of it with the plot area for a series that
    /// places a dimension in axes-fraction coordinates.
    pub(super) space: Space<'a>,
    /// The colour scales of the figure, through which artists map their numbers to colours.
    pub(super) scales: &'a ColourScales<'a>,
    /// The lower limit of each axis: planar contours without an explicit height are drawn at the lower z limit,
    /// and an image whose plane has no offset lies at the lower limit of the third axis of its plane.
    low: [f64; 3],
    /// The axes being drawn, named by the hit-map entry of each artist.
    pub(super) axes: ironlab_ir::NodeId,
    /// The number of points a series is decimated to for this plot rectangle.
    target: usize,
    /// The rectangle the geometry of the artist being drawn is clipped to, outside which nothing is drawn: the plot
    /// area of a 2D axes, the tile of a 3D axes, or the page for an artist that is not clipped.
    pub(super) clip: Rect,
    out: Vec<Prim>,
    /// The points each artist drew, for picking and datatips.
    hits: Vec<ArtistHit>,
    /// The placement of each image drawn in a 2D axes, for finding the pixel under a pointer.
    images: Vec<ImageHit>,
    /// Every bar drawn, for finding the bar under a pointer.
    pub(super) bars: Vec<BarHit>,
    /// Every step of stairs drawn, for finding the step under a pointer.
    pub(super) steps: Vec<StepHit>,
    /// Every cell drawn, for finding the cell under a pointer.
    pub(super) cells: Vec<CellHit>,
    /// The number of data cells drawn by each artist whose geometry a backend may rasterise, by artist.
    pub(super) dense: BTreeMap<NodeId, u64>,
}

impl Draw<'_, '_> {
    /// Emits an item at a depth, and reports whether there was one to emit.
    pub(super) fn push(&mut self, depth: f64, item: Option<Item>) -> bool {
        match item {
            Some(item) => {
                self.out.push((depth, item));
                true
            }
            None => false,
        }
    }

    /// Emits an item at a sort key, giving it the depth `depth` builds when the axes is three-dimensional; the
    /// items of a 2D axes carry no depth. Reports whether there was an item to emit.
    fn push_at(&mut self, key: f64, item: Option<Item>, depth: impl FnOnce() -> Depth) -> bool {
        let item = if self.space.is_3d() {
            item.map(|item| paths::with_depth(item, depth()))
        } else {
            item
        };
        self.push(key, item)
    }

    /// Records the points an artist drew, merging the samples of its line and of its markers.
    fn record(&mut self, artist: ironlab_ir::NodeId, mut samples: Vec<Sample>) {
        samples.sort_by_key(|s| s.source_index);
        samples.dedup_by_key(|s| s.source_index);
        if !samples.is_empty() {
            self.hits.push(ArtistHit {
                axes: self.axes,
                artist,
                samples,
            });
        }
    }
}

/// Draws every visible artist of an axes in artist order.
///
/// Returns the items tagged with their depths, the drawn points of every series, each carrying the index
/// it has in the artist's data arrays, the placement of every image drawn in a 2D axes, and the cell count of every
/// artist a backend may rasterise. An artist that cannot be drawn after all (an image whose corners cannot be
/// placed) is reported to `ctx` with a warning naming it.
pub(super) fn draw_artists(
    ctx: &mut Ctx<'_>,
    input: &AxesInput,
    primaries: &[Paint],
    space: &Space,
) -> Drawn {
    let axes_clip = if space.is_3d() {
        input.outer
    } else {
        input.plot
    };
    let mut draw = Draw {
        ctx,
        space: *space,
        scales: input.scales,
        low: input.ranges.map(|r| r.min),
        axes: input.axes.id,
        target: decimate::target_points(input.plot.width),
        clip: axes_clip,
        out: Vec::new(),
        hits: Vec::new(),
        images: Vec::new(),
        bars: Vec::new(),
        steps: Vec::new(),
        cells: Vec::new(),
        dense: BTreeMap::new(),
    };
    for (prepared, primary) in input.prepared.iter().zip(primaries) {
        let Some(data) = &prepared.data else { continue };
        if !prepared.artist.visible() {
            continue;
        }
        let placement = Placement::of(prepared.artist);
        draw.space = space.blended(placement.coords, input.plot);
        draw.clip = if space.is_3d() {
            axes_clip
        } else {
            placement.clip_rect(input.plot, input.page)
        };
        match (prepared.artist, data) {
            (
                Artist::Series(series),
                ArtistData::Series {
                    points,
                    sizes,
                    colours,
                },
            ) => draw_series(&mut draw, series, *points, *sizes, *colours, *primary),
            (Artist::Contour(c), ArtistData::Contour(grid)) => draw_contour(&mut draw, c, grid),
            (
                Artist::Quiver(q),
                ArtistData::Quiver {
                    points,
                    vectors,
                    scale,
                },
            ) => draw_quiver(&mut draw, q, *points, *vectors, *scale, *primary),
            (
                Artist::Surface(s),
                ArtistData::Surface {
                    grid,
                    colours,
                    face_alpha,
                    edge_alpha,
                },
            ) => {
                let alpha = SurfaceAlpha {
                    face: *face_alpha,
                    edge: *edge_alpha,
                };
                draw_surface(&mut draw, s, grid, *colours, alpha)
            }
            (Artist::Image(_) | Artist::ScalarImage(_), ArtistData::Image(image)) => {
                let scale = draw.scales.of(prepared.artist);
                draw_image(&mut draw, prepared.artist.id(), image, scale)
            }
            (Artist::Bar(bar), ArtistData::Bars(bars)) => {
                super::bars::draw_bars(&mut draw, bar, bars, *primary);
            }
            (Artist::Stairs(stairs), ArtistData::Stairs(geometry)) => {
                super::bars::draw_stairs(&mut draw, stairs, geometry, *primary);
            }
            (Artist::Cells(cells), ArtistData::Cells(data)) => {
                let scale = draw.scales.of(prepared.artist);
                super::cells::draw_cells(&mut draw, cells, data, scale);
            }
            _ => {}
        }
    }
    Drawn {
        prims: draw.out,
        hits: draw.hits,
        images: draw.images,
        bars: draw.bars,
        steps: draw.steps,
        cells: draw.cells,
        dense: draw.dense,
    }
}

/// How a series is placed in a 2D axes: the coordinates of its x and y values and whether its clipping is
/// released along the dimensions it places as fractions of the plot area. Every other artist places data along every
/// dimension and is clipped.
#[derive(Clone, Copy)]
pub(crate) struct Placement {
    pub coords: [ArtistCoords; 2],
    pub clip: bool,
}

impl Placement {
    pub fn of(artist: &Artist) -> Self {
        match artist {
            Artist::Series(series) => Self {
                coords: [series.x_coords, series.y_coords],
                clip: series.clip,
            },
            _ => Self {
                coords: [ArtistCoords::Data; 2],
                clip: true,
            },
        }
    }

    /// Returns the rectangle the artist is clipped to in a 2D axes with the plot area `plot` on the page `page`.
    ///
    /// Releasing the clip (`clip` false) releases it only along a dimension placed as fractions of the plot area,
    /// where the clip extends to the page, so that a marker can overhang an edge of the axes. Along a dimension placed
    /// in data the artist is clipped to the plot area as it is with `clip` true, so that data outside the limits of
    /// that axis is never drawn, and an artist in data along both dimensions is clipped to the plot area whatever its
    /// flag.
    pub fn clip_rect(&self, plot: Rect, page: Rect) -> Rect {
        let released = |dim: usize| !self.clip && self.coords[dim] == ArtistCoords::AxesFraction;
        let (left, right) = if released(0) {
            (page.x.min(plot.x), page.right().max(plot.right()))
        } else {
            (plot.x, plot.right())
        };
        let (top, bottom) = if released(1) {
            (page.y.min(plot.y), page.bottom().max(plot.bottom()))
        } else {
            (plot.y, plot.bottom())
        };
        Rect::new(left, top, right - left, bottom - top)
    }
}

/// Wraps every maximal run of consecutive items drawn for the same dense artist in an [`ItemKind::Dense`] group.
///
/// `items` are in final paint order, so the runs are exactly the stretches of the picture that a backend can replace
/// with a raster image without changing what covers what. In a 2D axes each dense artist yields one run; in a 3D axes
/// depth sorting can interleave the faces of a surface with the geometry of other artists, and each run is then
/// marked separately so that the back-to-front order survives rasterisation. Every group of one artist records that
/// artist's total cell count, not the size of the run.
pub(super) fn group_dense(items: Vec<Item>, dense: &BTreeMap<NodeId, u64>) -> Vec<Item> {
    if dense.is_empty() {
        return items;
    }
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    let mut run: Vec<Item> = Vec::new();
    let mut run_source: Option<NodeId> = None;
    let flush = |out: &mut Vec<Item>, run: &mut Vec<Item>, source: Option<NodeId>| {
        if let Some(source) = source
            && !run.is_empty()
        {
            out.push(Item {
                source: Some(source),
                kind: ItemKind::Dense {
                    cells: dense[&source],
                    items: std::mem::take(run),
                },
            });
        }
    };
    for item in items {
        let source = item.source.filter(|id| dense.contains_key(id));
        if source != run_source {
            flush(&mut out, &mut run, run_source);
            run_source = source;
        }
        if run_source.is_some() {
            run.push(item);
        } else {
            out.push(item);
        }
    }
    flush(&mut out, &mut run, run_source);
    out
}

/// Merges every maximal run of consecutive marker items of one artist that share an outline and an edge width into
/// one item holding their instances in order.
///
/// The compiler pushes one marker per item so that the depth sort of a 3D axes can order every marker among the
/// faces around it; afterwards the markers that the sort left together become one item, and in a 2D axes, where
/// nothing is sorted, an artist's markers are one item. No merge crosses another kind of item, so the order of
/// everything else is untouched.
pub(super) fn coalesce_markers(items: Vec<Item>) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    for item in items {
        if let ItemKind::Markers(markers) = &item.kind
            && let Some(last) = out.last_mut()
            && last.source == item.source
            && let ItemKind::Markers(run) = &mut last.kind
            && Arc::ptr_eq(&run.outline, &markers.outline)
            && run.edge_width == markers.edge_width
        {
            run.instances.extend(markers.instances.iter().copied());
            continue;
        }
        out.push(item);
    }
    out
}

/// Returns the mean of the depths of mapped samples.
fn mean_depth(samples: &[Sample]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().map(|s| s.depth).sum::<f64>() / samples.len() as f64
}

/// Returns the depths of mapped samples, in order.
fn depths(samples: &[Sample]) -> Vec<f64> {
    samples.iter().map(|s| s.depth).collect()
}

/// Returns the plane fitted to the positions and depths of mapped samples: exact for the corners of a planar face
/// and for the vertices of a band at one height, the least-squares plane for a twisted face, and the constant plane
/// at the mean depth when the samples are collinear on the page.
fn face_plane(samples: &[Sample]) -> DepthPlane {
    let points: Vec<(Point, f64)> = samples.iter().map(|s| (s.position, s.depth)).collect();
    depth_plane(&points).unwrap_or_else(|| DepthPlane::constant(mean_depth(samples)))
}

/// Returns the positions of mapped samples, in order.
fn positions(samples: &[Sample]) -> Vec<Point> {
    samples.iter().map(|s| s.position).collect()
}

/// Splits a sequence of data points into runs of consecutive points that can be placed.
///
/// A point that cannot be placed — one that is not finite, or that falls off a logarithmic axis —
/// breaks the run, so the geometry drawn for one run never spans a break. Decimation is applied to
/// each run separately for the same reason.
fn mapped_runs(space: &Space, points: impl Iterator<Item = [f64; 3]>) -> Vec<Vec<Sample>> {
    let mut runs = vec![Vec::new()];
    for (i, p) in points.enumerate() {
        match space.sample(i, p) {
            Some(s) => runs.last_mut().expect("runs is never empty").push(s),
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

/// Maps every point of a series, dropping those that cannot be placed.
fn mapped_samples(space: &Space, points: impl Iterator<Item = [f64; 3]>) -> Vec<Sample> {
    points
        .enumerate()
        .filter_map(|(i, p)| space.sample(i, p))
        .collect()
}

/// The smallest square that markers are ever binned into, in points.
const MIN_MARKER_BIN_PT: f64 = 0.5;

/// Returns `rect` grown by `margin` on every side.
fn expand(rect: Rect, margin: f64) -> Rect {
    let margin = if margin.is_finite() && margin > 0.0 {
        margin
    } else {
        0.0
    };
    Rect::new(
        rect.x - margin,
        rect.y - margin,
        rect.width + 2.0 * margin,
        rect.height + 2.0 * margin,
    )
}

/// Returns whether a segment can paint inside `rect`, judged by its bounding box.
fn segment_meets(rect: Rect, a: Point, b: Point) -> bool {
    a.x.min(b.x) <= rect.right()
        && a.x.max(b.x) >= rect.x
        && a.y.min(b.y) <= rect.bottom()
        && a.y.max(b.y) >= rect.y
}

/// Splits a run into the stretches of it that can paint inside `clip`.
///
/// Artist geometry is drawn inside a group clipped to the axes, so a segment whose bounding box
/// misses the clip rectangle paints nothing. Dropping such segments therefore changes nothing on
/// the page, and it spends the whole decimation budget on the part of the series the reader can
/// see: zooming into a thousand points of a million draws those thousand in full.
fn visible_stretches(run: &[Sample], clip: Rect) -> Vec<Vec<Sample>> {
    let mut stretches: Vec<Vec<Sample>> = Vec::new();
    let mut current: Vec<Sample> = Vec::new();
    for pair in run.windows(2) {
        if segment_meets(clip, pair[0].position, pair[1].position) {
            if current.is_empty() {
                current.push(pair[0]);
            }
            current.push(pair[1]);
        } else if !current.is_empty() {
            stretches.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        stretches.push(current);
    }
    stretches
}

/// Thins the polyline of a run to the resolution of the view, leaving short runs untouched.
fn thin_polyline(run: Vec<Sample>, target: usize, clip: Rect) -> Vec<Vec<Sample>> {
    if run.len() <= target {
        return vec![run];
    }
    visible_stretches(&run, clip)
        .into_iter()
        .map(|stretch| decimate::largest_triangle_three_buckets(&stretch, target))
        .collect()
}

/// Thins a set of markers of width `size_pt` when there are more of them than the view can show.
///
/// Markers that cannot paint inside `clip` are dropped first, so that a zoomed-in view keeps every
/// marker it shows. The binning square is half the marker width, so a marker dropped from a square
/// is at least three-quarters covered by the marker kept there; the width used is the smallest a
/// marker of the artist takes, so no marker is dropped by a smaller one that fails to cover it.
fn thin_markers(samples: Vec<Sample>, target: usize, size_pt: f64, clip: Rect) -> Vec<Sample> {
    if samples.len() <= target {
        return samples;
    }
    let side = if size_pt.is_finite() && size_pt > 0.0 {
        (size_pt / 2.0).max(MIN_MARKER_BIN_PT)
    } else {
        MIN_MARKER_BIN_PT
    };
    let visible = expand(clip, if size_pt.is_finite() { size_pt } else { 0.0 });
    let samples: Vec<Sample> = samples
        .into_iter()
        .filter(|s| visible.contains(s.position))
        .collect();
    if samples.len() <= target {
        return samples;
    }
    decimate::bin(&samples, side)
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

/// The longest a piece of a gradient segment may be, in points, unless the segment has [`MAX_GRADIENT_PIECES`]
/// pieces: a few device pixels at print resolution, so that the seams between pieces cannot be seen.
const MAX_GRADIENT_PIECE_PT: f64 = 1.0;

/// The most pieces one gradient segment is cut into: no colormap holds more distinguishable colours than this along
/// one segment, and the bound keeps a very long segment from costing thousands of paths.
const MAX_GRADIENT_PIECES: usize = 256;

/// Returns the number of pieces of one colour into which a gradient segment `length` points long on the page is
/// cut, which depends on nothing else, so that a figure is reproducible and a larger figure is cut more finely.
///
/// The pieces are of equal length: a segment no longer than [`MAX_GRADIENT_PIECE_PT`] is one piece, and a longer
/// one is cut into the fewest pieces no longer than that, at most [`MAX_GRADIENT_PIECES`]. A segment of length
/// `L > 1` pt is then cut into `⌈L⌉` pieces of `L / ⌈L⌉ ≥ L / (L + 1) > 0.5` pt, so that no piece is shorter than
/// half a point unless its segment is a single piece.
fn gradient_pieces(length: f64) -> usize {
    if !(length.is_finite() && length > MAX_GRADIENT_PIECE_PT) {
        return 1;
    }
    // The length is finite and positive, and the cast saturates, so the clamp bounds any value.
    ((length / MAX_GRADIENT_PIECE_PT).ceil() as usize).clamp(1, MAX_GRADIENT_PIECES)
}

/// Returns the straight sRGB interpolation from `a` to `b` at `t`, alpha included.
fn mix(a: Rgba, b: Rgba, t: f64) -> Rgba {
    let t = t as f32;
    let at = |p: f32, q: f32| p + (q - p) * t;
    Rgba::new(at(a.r, b.r), at(a.g, b.g), at(a.b, b.b), at(a.a, b.a))
}

/// Returns whether a dash pattern, run from its start, is on (drawing) at `distance` along the line; a solid line,
/// whose pattern is empty, is on everywhere.
fn dash_on(dash: &[f64], distance: f64) -> bool {
    let period: f64 = dash.iter().sum();
    if !(period.is_finite() && period > 0.0) {
        return true;
    }
    let mut phase = distance.rem_euclid(period);
    for (k, &length) in dash.iter().enumerate() {
        if phase < length {
            return k % 2 == 0;
        }
        phase -= length;
    }
    true
}

/// The colours of one segment of a line drawn segment by segment.
#[derive(Clone, Copy)]
struct SegmentPaint {
    /// The colour at the segment's first point.
    from: Rgba,
    /// The colour at the segment's second point.
    to: Rgba,
    /// Whether the segment is a gradient between the two colours, cut into pieces of one colour by
    /// [`gradient_pieces`], rather than one stroke of the colour `from`.
    gradient: bool,
}

/// Draws the line of a series segment by segment: each segment of each run is one stroke, or a run of pieces when it
/// is a gradient, and in a 3D axes each piece is an item of its own, carrying the depths of its two ends and placed
/// in the painter's order by its own depth, so that a line passing through a surface is drawn behind it and in front
/// of it where it should be. `paint` gives the colours of the segment between two consecutive drawn points, or
/// `None` for a segment that is not drawn.
///
/// Each stroke keeps the cap, width and dash array of `template`, with a dash offset equal to its distance from the
/// start of its run, so that a dashed line keeps the rhythm of the same line drawn as one path. A corner between two
/// drawn segments, which one path would join with a round join, is filled with a disc of the line's width in the
/// colour at the corner, wherever the dash pattern is on there; the disc is painted before the segments in a 2D axes
/// and just behind them in a 3D axes, so that it shows only outside the two segments, where the join would be.
fn draw_segments(
    draw: &mut Draw,
    id: NodeId,
    runs: &[Vec<Sample>],
    template: &Stroke,
    paint: impl Fn(&Sample, &Sample) -> Option<SegmentPaint>,
) {
    let period: f64 = template.dash.iter().sum();
    let offset = |distance: f64| {
        if template.dash.is_empty() || !(period.is_finite() && period > 0.0) {
            0.0
        } else {
            distance.rem_euclid(period)
        }
    };
    let mut joins: Vec<(Sample, Rgba)> = Vec::new();
    let mut pieces: Vec<(Sample, Sample, Rgba, f64)> = Vec::new();
    for run in runs {
        let mut distance = 0.0;
        let mut previous: Option<Rgba> = None;
        for pair in run.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let length = (b.position.x - a.position.x).hypot(b.position.y - a.position.y);
            let segment = paint(&a, &b);
            if let (Some(corner), Some(_)) = (previous, segment)
                && dash_on(&template.dash, distance)
            {
                joins.push((a, corner));
            }
            if let Some(segment) = segment {
                let n = if segment.gradient {
                    gradient_pieces(length)
                } else {
                    1
                };
                // The ends of a piece are computed from the same parameter on either side, and the ends of the
                // segment are its points exactly, so that pieces meet without gap or overlap and a segment carries
                // exactly the depths of its points.
                let at = |j: usize| {
                    if j == 0 {
                        return a;
                    }
                    if j == n {
                        return b;
                    }
                    let t = j as f64 / n as f64;
                    Sample {
                        source_index: a.source_index,
                        position: Point::new(
                            a.position.x + (b.position.x - a.position.x) * t,
                            a.position.y + (b.position.y - a.position.y) * t,
                        ),
                        depth: a.depth + (b.depth - a.depth) * t,
                    }
                };
                for j in 0..n {
                    let colour = if segment.gradient {
                        mix(segment.from, segment.to, (j as f64 + 0.5) / n as f64)
                    } else {
                        segment.from
                    };
                    let start = distance + length * j as f64 / n as f64;
                    pieces.push((at(j), at(j + 1), colour, start));
                }
            }
            previous = segment.map(|segment| segment.to);
            distance += length;
        }
    }
    for (corner, colour) in joins {
        let mut b = PathBuilder::new();
        b.circle(corner.position, template.width / 2.0);
        let item = paths::item(id, b.finish(), Some(paths::fill(colour)), None);
        let depth = corner.depth - EDGE_DEPTH_LIFT;
        draw.push_at(depth, item, || Depth::Plane(DepthPlane::constant(depth)));
    }
    for (a, b, colour, start) in pieces {
        let mut builder = PathBuilder::new();
        builder.polyline(&[a.position, b.position], false);
        let stroke = Stroke {
            color: colour,
            dash_offset: offset(start),
            ..template.clone()
        };
        let item = paths::item(id, builder.finish(), None, Some(stroke));
        draw.push_at((a.depth + b.depth) / 2.0, item, || {
            Depth::Vertices(vec![a.depth, b.depth])
        });
    }
}

/// Draws a series: the line through its placeable points, when it draws one, followed by its markers.
///
/// The line of each run is thinned by [`decimate::largest_triangle_three_buckets`] and the markers by
/// [`decimate::bin`], so a series far denser than the plot rectangle can resolve costs only what the view can show;
/// a thinned segment joins two kept points, and every colour is read at the source index of its point or segment.
///
/// A line of one colour in a 2D axes is one stroked path with a subpath per run, exactly as the line that a series
/// replaced. Any other line is drawn segment by segment by [`draw_segments`]: a colour-mapped line with one colour
/// value per point strokes each segment as a gradient between the colours of its two points, one with a value per
/// segment strokes each segment in the colour of its value (a thinned segment in that of the first source segment it
/// replaces, the one leaving its first point), and every line of a 3D axes is depth-ordered segment by segment.
///
/// The colour of the series is its line colour (`primary`), which an automatic marker face or edge follows: with a
/// colour-mapped line and one value per point, the colour of each point. A colormapped marker face or edge takes the
/// colour of its point's value, and without colour data the colour of the series. A point whose colour value the
/// scale draws nothing for (a non-finite value under the default transparent policy) is not drawn: it has no marker,
/// and it breaks the line, so neither segment that meets it is drawn. A series that draws a line strokes its marker
/// edges at its line width held to 0.5–1.5 pt, and one that draws no line at 0.5 pt, as the line and the scatter that
/// a series replaced did.
fn draw_series(
    draw: &mut Draw,
    series: &Series,
    points: Points,
    sizes: Option<&[f64]>,
    colours: Option<SeriesColours>,
    primary: Paint,
) {
    // The data stage lets colour data through only with a colour scale to map it.
    let scale = series.color_scale.and_then(|id| draw.scales.get(id));
    let map_all = |values: &[f64]| -> Vec<Option<Rgba>> {
        values
            .iter()
            .map(|&value| scale.and_then(|scale| scale.colour(value)))
            .collect()
    };
    let (point_colours, segment_colours) = match colours {
        Some(SeriesColours::PerPoint(values)) => (Some(map_all(values)), None),
        Some(SeriesColours::PerSegment(values)) => (None, Some(map_all(values))),
        None => (None, None),
    };
    let own = primary.single(style::viridis_middle());
    let mapped_line = series.line.color == ColorSpec::Colormapped;
    // The colour of the series at a point, which an automatic marker colour follows.
    let series_colour = |i: usize| match &point_colours {
        Some(colours) if mapped_line => colours[i],
        _ => own,
    };
    let part = |spec: ColorSpec, i: usize| match spec {
        ColorSpec::Auto => series_colour(i),
        ColorSpec::Colormapped => match &point_colours {
            Some(colours) => colours[i],
            None => series_colour(i),
        },
        ColorSpec::Rgba { color } => Some(style::ir_colour(color)),
        ColorSpec::None => None,
    };
    // A point whose colour the scale draws nothing for cannot be placed, so it breaks the line as a gap does.
    let at = |i: usize| match &point_colours {
        Some(colours) if colours[i].is_none() => [f64::NAN; 3],
        _ => points.get(i),
    };

    let mut drawn: Vec<Sample> = Vec::new();
    let template = series
        .draws_line()
        .then(|| line_stroke(&series.line, Some(Rgba::BLACK)))
        .flatten();
    if let Some(template) = template {
        let clip = expand(draw.clip, template.width);
        let runs: Vec<Vec<Sample>> = mapped_runs(&draw.space, (0..points.len()).map(at))
            .into_iter()
            .flat_map(|run| thin_polyline(run, draw.target, clip))
            .collect();
        drawn.extend(runs.iter().flatten().copied());
        let solid = |colour: Option<Rgba>| {
            colour.map(|colour| SegmentPaint {
                from: colour,
                to: colour,
                gradient: false,
            })
        };
        match (&point_colours, &segment_colours) {
            (Some(colours), _) if mapped_line => {
                draw_segments(draw, series.id, &runs, &template, |a, b| {
                    Some(SegmentPaint {
                        from: colours[a.source_index]?,
                        to: colours[b.source_index]?,
                        gradient: true,
                    })
                });
            }
            (_, Some(colours)) if mapped_line => {
                draw_segments(draw, series.id, &runs, &template, |a, _| {
                    solid(colours[a.source_index])
                });
            }
            _ if draw.space.is_3d() => {
                draw_segments(draw, series.id, &runs, &template, |_, _| solid(own));
            }
            _ => {
                if let Some(colour) = own {
                    let mut b = PathBuilder::new();
                    for run in &runs {
                        b.polyline(&positions(run), false);
                    }
                    let stroke = Stroke {
                        color: colour,
                        ..template
                    };
                    draw.push(0.0, paths::item(series.id, b.finish(), None, Some(stroke)));
                }
            }
        }
    }
    if let Some(outline) = style::marker_outline(series.marker.shape) {
        let edge_width = if series.draws_line() {
            style::width_or(series.line.width_pt, 0.75).clamp(0.5, 1.5)
        } else {
            0.5
        };
        let size_at = |i: usize| sizes.map_or(series.marker.size_pt, |sizes| sizes[i]);
        let samples = mapped_samples(&draw.space, (0..points.len()).map(at));
        let smallest = samples
            .iter()
            .map(|s| size_at(s.source_index))
            .filter(|v| v.is_finite() && *v > 0.0)
            .fold(f64::INFINITY, f64::min);
        let samples = thin_markers(samples, draw.target, smallest, draw.clip);
        for s in &samples {
            let i = s.source_index;
            let instance = style::marker_instance_coloured(
                &outline,
                style::MarkerPlace {
                    position: s.position,
                    size: size_at(i),
                    depth: s.depth,
                    source_index: i,
                },
                part(series.marker.face, i),
                part(series.marker.edge, i),
                1.0,
            );
            draw.push(
                s.depth,
                instance
                    .map(|instance| style::marker_item(series.id, &outline, edge_width, instance)),
            );
        }
        drawn.extend(samples);
    }
    draw.record(series.id, drawn);
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

/// Returns the finite positive range of a field's values, over which a logarithmic norm places levels.
fn positive_field_range(values: &[f64]) -> Option<(f64, f64)> {
    let positive: Vec<f64> = values.iter().copied().filter(|v| *v > 0.0).collect();
    field_range(&positive)
}

/// Returns the contour levels of an artist for a field spanning `[zmin, zmax]`, sorted and without duplicates.
///
/// Automatic levels are spaced linearly, or, when `log_range` holds the positive range of the field under a
/// logarithmic norm, logarithmically within it (see [`contour::auto_log_levels`]).
pub(super) fn contour_levels(
    levels: &Levels,
    zmin: f64,
    zmax: f64,
    log_range: Option<(f64, f64)>,
) -> Vec<f64> {
    let mut out = match (levels, log_range) {
        (Levels::Auto { count }, None) => {
            contour::auto_levels(zmin, zmax, (*count).max(1) as usize)
        }
        (Levels::Auto { count }, Some((lo, hi))) => {
            contour::auto_log_levels(lo, hi, (*count).max(1) as usize)
        }
        (Levels::Explicit { values }, _) => {
            values.iter().copied().filter(|v| v.is_finite()).collect()
        }
    };
    out.sort_by(f64::total_cmp);
    out.dedup();
    out
}

/// The number of automatic levels of a contour without levels of its own whose colour scale offers none.
const AUTOMATIC_CONTOUR_LEVELS: u32 = 10;

/// Draws the filled bands and isolines of a contour, the bands faded by its fill opacity and the isolines by its
/// line opacity; an opacity of 0 draws nothing.
///
/// A contour without levels of its own takes them from its colour scale (see [`ResolvedScale::contour_levels`]), and
/// otherwise draws ten automatic levels, which a scale with a logarithmic norm spaces logarithmically over the positive
/// values of the field. A band is coloured by the middle of its range within the field, geometric under a logarithmic
/// norm when the range is positive, and an isoline by its level, except for the levels a contour takes from an indexed or categorical scale: those lie
/// between entries, so a band takes the entry of the whole number it holds and an isoline the entry of the band
/// below it. A colormapped colour of a contour that names no usable scale takes the middle of viridis.
fn draw_contour(draw: &mut Draw, c: &Contour, grid: &GridRef) {
    let Some((zmin, zmax)) = field_range(grid.z) else {
        return;
    };
    let fill_opacity = style::unit_opacity(c.fill_opacity);
    let line_opacity = style::unit_opacity(c.line_opacity);
    let scale: Option<&ResolvedScale> = c.color_scale.and_then(|id| draw.scales.get(id));
    let from_scale = match &c.levels {
        Some(_) => None,
        None => scale.and_then(ResolvedScale::contour_levels),
    };
    // Levels between the entries of an indexed or categorical scale, of which each band holds one whole number.
    let between_entries = from_scale.is_some() && scale.is_some_and(ResolvedScale::is_index_mapped);
    let log = scale.is_some_and(|scale| scale.norm() == ColorScaleNorm::Log);
    let log_range = if log {
        positive_field_range(grid.z)
    } else {
        None
    };
    let levels = match (&c.levels, from_scale) {
        (Some(levels), _) => contour_levels(levels, zmin, zmax, log_range),
        (None, Some(levels)) => {
            contour_levels(&Levels::Explicit { values: levels }, zmin, zmax, log_range)
        }
        (None, None) => contour_levels(
            &Levels::Auto {
                count: AUTOMATIC_CONTOUR_LEVELS,
            },
            zmin,
            zmax,
            log_range,
        ),
    };
    let mapped = |value: f64| match scale {
        Some(scale) => scale.colour(value),
        None => Some(style::viridis_middle()),
    };
    let plane = match c.placement {
        ContourPlacement::Plane { z: Some(z) } => z,
        _ => draw.low[2],
    };
    if c.fill && fill_opacity > 0.0 {
        for (lo, hi) in contour::band_edges(&levels, zmin, zmax) {
            let (low, high) = (lo.max(zmin), hi.min(zmax));
            // Under a logarithmic norm a band of positive values is coloured by its geometric middle, which is the
            // middle of its span on the scale.
            let mid = if log && low > 0.0 {
                (low * high).sqrt()
            } else {
                (low + high) / 2.0
            };
            let value = if between_entries { mid.round() } else { mid };
            let Some(colour) = mapped(value) else {
                continue;
            };
            let mut b = PathBuilder::new();
            let mut mapped_all: Vec<Sample> = Vec::new();
            for polygon in contour::isobands(grid, lo, hi) {
                let mapped: Option<Vec<Sample>> = polygon
                    .iter()
                    .enumerate()
                    .map(|(i, q)| draw.space.sample(i, [q[0], q[1], plane]))
                    .collect();
                let Some(mapped) = mapped else { continue };
                b.polyline(&positions(&mapped), true);
                mapped_all.extend(mapped);
            }
            let fill = paths::fill(colour.with_alpha_factor(fill_opacity as f32));
            let item = paths::item(c.id, b.finish(), Some(fill), None);
            draw.push_at(mean_depth(&mapped_all), item, || {
                Depth::Plane(face_plane(&mapped_all))
            });
        }
    }
    let line_colour = |level: f64| {
        let value = if between_entries { level - 0.5 } else { level };
        let colour = match c.line.color {
            ColorSpec::Rgba { color } => Some(style::ir_colour(color)),
            ColorSpec::None => None,
            ColorSpec::Auto | ColorSpec::Colormapped if !c.fill => mapped(value),
            ColorSpec::Auto | ColorSpec::Colormapped => None,
        };
        colour
            .filter(|_| line_opacity > 0.0)
            .map(|colour| colour.with_alpha_factor(line_opacity as f32))
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
            let runs = mapped_runs(&draw.space, points);
            let closed =
                polyline.closed && runs.len() == 1 && runs[0].len() == polyline.points.len();
            for run in runs {
                let pts = positions(&run);
                if draw.space.is_3d() {
                    let mut single = PathBuilder::new();
                    single.polyline(&pts, closed);
                    let item = paths::item(c.id, single.finish(), None, Some(stroke.clone()));
                    draw.push_at(mean_depth(&run), item, || Depth::Vertices(depths(&run)));
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
    let colour = primary.single(style::viridis_middle());
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
        let key = (arrow[0].1 + arrow[1].1) / 2.0;
        let item = paths::item(q.id, b.finish(), None, Some(stroke.clone()));
        draw.push_at(key, item, || {
            Depth::Vertices(arrow.iter().map(|(_, depth)| *depth).collect())
        });
    }
}

/// Returns the physical x and y coordinates of grid node `(i, j)`.
fn node_xy(grid: &GridRef, i: usize, j: usize) -> [f64; 2] {
    match grid.coords {
        Coords::Rectilinear { x, y } => [x[i], y[j]],
        Coords::Curvilinear { x, y } => [x[j * grid.nx + i], y[j * grid.nx + i]],
    }
}

/// The alpha arrays of a surface, one value per node of its field, for its faces and for their edges.
#[derive(Clone, Copy)]
struct SurfaceAlpha<'a> {
    face: Option<&'a [f64]>,
    edge: Option<&'a [f64]>,
}

/// Returns the alpha of a face from an alpha array at the nodes of its four corners: the mean of the four values,
/// each clamped to `[0, 1]`, or 0 when any of them is not finite, so that the face is transparent; a surface without
/// such an array is opaque, with an alpha of 1.
fn corner_alpha(alpha: Option<&[f64]>, corners: [usize; 4]) -> f64 {
    let Some(alpha) = alpha else {
        return 1.0;
    };
    let values = corners.map(|k| alpha[k]);
    if values.iter().any(|value| !value.is_finite()) {
        return 0.0;
    }
    values
        .iter()
        .map(|value| value.clamp(0.0, 1.0))
        .sum::<f64>()
        / 4.0
}

/// Draws one flat face per grid cell of a surface, and records how many faces it drew.
///
/// The alpha of the fill of a face is the alpha of its colour multiplied by its face alpha (see [`corner_alpha`])
/// and by the face opacity, and the alpha of its edge likewise by its edge alpha and the edge opacity. A fill or an
/// edge whose alpha and opacity multiply to 0 is not drawn, and a face with neither is not drawn at all; a surface
/// without alpha arrays and with both opacities 1 is drawn in exactly the colours it names.
///
/// In a 2D axes a face is one item carrying its fill and its edge. In a 3D axes the two are separate items, the
/// fill and then the edge, both on the plane fitted to the projected corners of the face pushed back by
/// [`FACE_DEPTH_BIAS`] and sorted at the mean corner depth less the bias, so that lines and markers lying on the
/// surface are painted, and depth-tested, in front of it. The edge is a ring filled with the edge colour between
/// the face's outline and the outline moved half the edge width inwards ([`polygon::inset`]), so that it never
/// spills over the neighbouring faces: the painter's order and the depth test then agree along every fold, and the
/// rings of two neighbours meet to make an edge of the full width. A face whose outline has no inset (one seen
/// edge-on, or too small for its edge) is stroked instead. A face counts once however many items it is.
fn draw_surface(
    draw: &mut Draw,
    s: &Surface,
    grid: &GridRef,
    colours: Option<&[f64]>,
    alpha: SurfaceAlpha,
) {
    let values = colours.unwrap_or(grid.z);
    let scale: Option<&ResolvedScale> = s.color_scale.and_then(|id| draw.scales.get(id));
    let first_corner = scale.is_some_and(ResolvedScale::is_index_mapped);
    let width = style::width_or(s.edge_width_pt, 0.5);
    let face_opacity = style::unit_opacity(s.face_opacity);
    let edge_opacity = style::unit_opacity(s.edge_opacity);
    let mut faces: u64 = 0;
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
            // The mean of indices is not an index, so through an indexed or categorical scale a face takes the
            // value of its first corner.
            let value = if first_corner {
                values[index(corners[0])]
            } else {
                corners.iter().map(|c| values[index(*c)]).sum::<f64>() / 4.0
            };
            let mapped: Option<Vec<Sample>> = corners
                .iter()
                .enumerate()
                .map(|(k, &(ci, cj))| {
                    let [x, y] = node_xy(grid, ci, cj);
                    draw.space.sample(k, [x, y, grid.z[index((ci, cj))]])
                })
                .collect();
            let Some(mapped) = mapped else { continue };
            let paint = |spec: ColorSpec| match style::resolve(spec, Paint::Colormapped) {
                Paint::Fixed(c) => c,
                Paint::Colormapped => match scale {
                    Some(scale) => scale.colour(value),
                    None => Some(style::viridis_middle()),
                },
            };
            let nodes = corners.map(index);
            let face_factor = corner_alpha(alpha.face, nodes) * face_opacity;
            let edge_factor = corner_alpha(alpha.edge, nodes) * edge_opacity;
            let fill = paint(s.face)
                .filter(|_| face_factor > 0.0)
                .map(|c| paths::fill(c.with_alpha_factor(face_factor as f32)));
            let stroke = paint(s.edge)
                .filter(|_| edge_factor > 0.0)
                .map(|c| paths::stroke(c.with_alpha_factor(edge_factor as f32), width, Vec::new()));
            let mut b = PathBuilder::new();
            b.polyline(&positions(&mapped), true);
            let segments = b.finish();
            let mean = mean_depth(&mapped);
            let drawn = if draw.space.is_3d() {
                let plane = face_plane(&mapped).pushed_back(FACE_DEPTH_BIAS);
                let key = mean - FACE_DEPTH_BIAS;
                let mut drawn = false;
                if let Some(fill) = fill {
                    let item = paths::item(s.id, segments.clone(), Some(fill), None);
                    drawn |= draw.push_at(key, item, || Depth::Plane(plane));
                }
                if let Some(stroke) = stroke {
                    let corners = positions(&mapped);
                    let lifted = plane.pushed_back(-EDGE_DEPTH_LIFT);
                    let item = match polygon::inset(&corners, stroke.width / 2.0) {
                        Some(inner) => {
                            let mut ring = PathBuilder::new();
                            ring.polyline(&corners, true);
                            let mut reversed = inner;
                            reversed.reverse();
                            ring.polyline(&reversed, true);
                            paths::item(s.id, ring.finish(), Some(paths::fill(stroke.color)), None)
                        }
                        None => paths::item(s.id, segments, None, Some(stroke)),
                    };
                    drawn |= draw.push_at(key + EDGE_DEPTH_LIFT, item, || Depth::Plane(lifted));
                }
                drawn
            } else {
                draw.push(mean, paths::item(s.id, segments, fill, stroke))
            };
            if drawn {
                faces += 1;
            }
        }
    }
    if faces > 0 {
        *draw.dense.entry(s.id).or_default() += faces;
    }
}

/// The largest distance, as a fraction of the extent of an axis, at which the plane of an image still counts as
/// lying on a face of the box of a 3D axes.
const FACE_TOLERANCE: f64 = 1e-9;

/// Returns the depth at which a 3D axes sorts an image whose plane lies on a face of its box, or `None` when the
/// plane lies inside or beyond the box and the image is sorted at the mean depth of its corners.
///
/// Everything inside the box is in front of a back plane of the view and behind a front face, so an image on a back
/// plane is keyed at `−∞`, before every other primitive of the axes, and one on a front face at `+∞`, after every
/// other primitive; every other depth is finite. The depth sort is stable, so several images on faces keep their
/// artist order among themselves. The plane lies on a face when its coordinate along the third axis, normalised
/// into the box, is within [`FACE_TOLERANCE`] of −0.5 or 0.5: an absent offset is the lower limit itself and an
/// explicit offset equal to a manual limit matches exactly, while an explicit offset to which automatic limits were
/// rounded can differ from the limit by floating-point noise, because the rounding computes the limit as a
/// multiple of the tick step, and the tolerance admits it.
fn face_depth(projector: &Projector, third: usize, offset: f64) -> Option<f64> {
    let mut p = projector.lo;
    p[third] = offset;
    let u = projector.normalise(p)[third];
    let on_min = (u + 0.5).abs() <= FACE_TOLERANCE;
    let on_max = (u - 0.5).abs() <= FACE_TOLERANCE;
    if !(on_min || on_max) {
        return None;
    }
    let back_is_min = matches!(
        back_planes(&projector.camera)[third],
        Plane::XMin | Plane::YMin | Plane::ZMin
    );
    Some(if on_min == back_is_min {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    })
}

/// Computes the transform of the pixel space `[0, nx] × [0, ny]` of an image into figure space, the key at which
/// the image is sorted in 3D, and the plane of its depth over pixel space in 3D.
///
/// The columns run along the first axis of the plane and the rows along the second, between the pixel edges of the
/// placement; the coordinate along the third axis is the offset of the plane, or the lower limit of that axis when
/// it has none, and a 2D axes ignores it. The four corners are mapped through the axes, which is affine on the
/// linear axes of the plane, so the transform follows from three of them, and so does the plane of the depth, which
/// reproduces the depth of every corner. An image whose plane lies on a face of the box is sorted at the sentinel
/// of [`face_depth`] and keeps that plane; an image inside the box is sorted at the mean depth of its corners less
/// [`FACE_DEPTH_BIAS`], with its plane pushed back by the same, so that markers and lines lying on it are painted,
/// and depth-tested, in front of it. Returns `None` when a corner cannot be placed.
fn place_image(draw: &Draw, image: &ImageData) -> Option<(Transform, f64, Option<DepthPlane>)> {
    let [columns, rows] = image.plane_dims();
    let third = image.offset_dim();
    let (x0, x1) = image::edges(image.placement.columns, image.nx);
    let (y0, y1) = image::edges(image.placement.rows, image.ny);
    let offset = image.placement.plane.offset().unwrap_or(draw.low[third]);
    let corner = |x: f64, y: f64| {
        let mut p = [0.0; 3];
        p[columns] = x;
        p[rows] = y;
        p[third] = offset;
        draw.space.map(p)
    };
    let (origin, d0) = corner(x0, y0)?;
    let (along_columns, d1) = corner(x1, y0)?;
    let (along_rows, d2) = corner(x0, y1)?;
    let (_, d3) = corner(x1, y1)?;
    let (nx, ny) = (image.nx as f64, image.ny as f64);
    let transform = Transform {
        a: (along_columns.x - origin.x) / nx,
        b: (along_columns.y - origin.y) / nx,
        c: (along_rows.x - origin.x) / ny,
        d: (along_rows.y - origin.y) / ny,
        e: origin.x,
        f: origin.y,
    };
    let mean = (d0 + d1 + d2 + d3) / 4.0;
    let (key, plane) = match draw.space {
        Space::ThreeD(projector) => {
            let plane = DepthPlane {
                a: (d1 - d0) / nx,
                b: (d2 - d0) / ny,
                c: d0,
            };
            match face_depth(projector, third, offset) {
                Some(sentinel) => (sentinel, Some(plane)),
                None => (
                    mean - FACE_DEPTH_BIAS,
                    Some(plane.pushed_back(FACE_DEPTH_BIAS)),
                ),
            }
        }
        Space::TwoD { .. } => (mean, None),
    };
    Some((transform, key, plane))
}

/// Draws an image of any kind as one image item in pixel space beneath one group whose transform places it in the
/// axes, pushed as one primitive at the key [`place_image`] gives and carrying the plane of its depth in 3D, and
/// records the placement in the hit map when the axes is two-dimensional.
///
/// The samples run in row order from row 0 and are never reordered: a range that runs backwards mirrors the image
/// through the transform alone, so the row and column that a hit resolves to index the artist's array. The image is
/// never wrapped in a dense group, since it is already a raster. A scalar image is coloured through `scale`, its
/// colour scale. An image whose corners cannot be placed is skipped with a warning naming it.
fn draw_image(draw: &mut Draw, id: NodeId, image: &ImageData, scale: Option<&ResolvedScale>) {
    let Some((transform, key, plane)) = place_image(draw, image) else {
        draw.ctx.warn(
            Some(id),
            "The artist is not drawn because the corners of the image cannot be placed in the axes.",
        );
        return;
    };
    let raster = image::resolve(image, scale);
    let (nx, ny) = (image.nx, image.ny);
    let item = Item {
        source: Some(id),
        kind: ItemKind::Image(ImageItem {
            rect: Rect::new(0.0, 0.0, nx as f64, ny as f64),
            // The data stage refuses an image with more pixels along an axis than fit in 32 bits.
            width: nx as u32,
            height: ny as u32,
            channels: raster.channels,
            samples: raster.samples,
            depth: plane,
        }),
    };
    draw.out.push((
        key,
        Item {
            source: Some(id),
            kind: ItemKind::Group {
                clip: None,
                transform: Some(transform),
                items: vec![item],
            },
        },
    ));
    if !draw.space.is_3d()
        && let Some(to_pixel) = transform.inverse()
    {
        draw.images.push(ImageHit {
            axes: draw.axes,
            artist: id,
            to_pixel,
            columns: nx,
            rows: ny,
        });
    }
}
