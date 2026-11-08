//! Geometry that interactive front ends need to map pointer positions back onto figure elements.

use ironlab_ir::{ColourScaleKey, ColourbarOrientation, HistNorm, NodeId};

use crate::display::{Point, Rect, Transform};
use crate::maths::decimate::Sample;

/// Maps one data axis onto a coordinate range in figure space.
///
/// `start` is the figure-space coordinate (points) of the `min` limit and `end` that of the `max` limit. For a
/// horizontal axis `start < end`; for a vertical axis (y increasing downwards in figure space) `start > end`. A
/// reversed axis swaps them, so that `start > end` for a reversed horizontal axis and `start < end` for a reversed
/// vertical one; `min < max` always, and every conversion below holds in either direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisMap {
    pub min: f64,
    pub max: f64,
    pub log: bool,
    pub start: f64,
    pub end: f64,
}

impl AxisMap {
    fn forward(&self, v: f64) -> f64 {
        if self.log { v.log10() } else { v }
    }

    fn inverse(&self, v: f64) -> f64 {
        if self.log { 10f64.powf(v) } else { v }
    }

    /// Converts a data value to a figure-space coordinate.
    pub fn to_figure(&self, value: f64) -> f64 {
        let (a, b) = (self.forward(self.min), self.forward(self.max));
        let t = (self.forward(value) - a) / (b - a);
        self.start + t * (self.end - self.start)
    }

    /// Converts a figure-space coordinate to a data value.
    pub fn to_data(&self, coord: f64) -> f64 {
        let (a, b) = (self.forward(self.min), self.forward(self.max));
        let t = (coord - self.start) / (self.end - self.start);
        self.inverse(a + t * (b - a))
    }
}

/// Hit-testing geometry for one axes.
#[derive(Clone, Debug, PartialEq)]
pub struct AxesHit {
    pub id: NodeId,
    /// The data region of the axes, in figure space.
    pub plot_rect: Rect,
    pub kind: AxesHitKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AxesHitKind {
    TwoD {
        x: AxisMap,
        y: AxisMap,
    },
    /// Three-dimensional axes are manipulated through their camera, not through a data mapping.
    ThreeD,
    /// A polar axes, in which x is the angle and y the radius.
    ///
    /// `x` maps an angle to its page angle in radians, anticlockwise from east (its `start` and `end` are the page
    /// angles of the lower and upper angular limits, so they encode the zero direction, the direction of the axis
    /// and the sector), and `y` maps a radius to its distance from `centre` in points (its `start` and `end` are the
    /// distances of the lower and upper radial limits: the edge of the hole and the rim, swapped for a reversed
    /// radius).
    Polar {
        centre: Point,
        x: AxisMap,
        y: AxisMap,
    },
}

impl AxesHitKind {
    /// Converts data coordinates `[x, y]` to a figure-space point, or returns `None` for a three-dimensional axes or
    /// coordinates that cannot be placed.
    pub fn to_figure(&self, data: [f64; 2]) -> Option<Point> {
        let p = match self {
            Self::TwoD { x, y } => Point::new(x.to_figure(data[0]), y.to_figure(data[1])),
            Self::ThreeD => return None,
            Self::Polar { centre, x, y } => {
                let (phi, rho) = (x.to_figure(data[0]), y.to_figure(data[1]));
                Point::new(centre.x + rho * phi.cos(), centre.y - rho * phi.sin())
            }
        };
        (p.x.is_finite() && p.y.is_finite()).then_some(p)
    }

    /// Converts a figure-space point to data coordinates `[x, y]`, or returns `None` for a three-dimensional axes, a
    /// point that no value maps to, and in polar axes a point outside the plot region (beyond the rim, in the hole or
    /// outside the sector). A polar angle is reported within the angular limits.
    pub fn to_data(&self, p: Point) -> Option<[f64; 2]> {
        let data = match self {
            Self::TwoD { x, y } => [x.to_data(p.x), y.to_data(p.y)],
            Self::ThreeD => return None,
            Self::Polar { centre, x, y } => {
                let (theta, inside) = polar_angle(*centre, x, p);
                let rho = (p.x - centre.x).hypot(p.y - centre.y);
                let (r0, r1) = (y.start.min(y.end), y.start.max(y.end));
                let tol = 1e-9 * r1.max(1.0);
                if !inside || rho < r0 - tol || rho > r1 + tol {
                    return None;
                }
                [theta, y.to_data(rho)]
            }
        };
        (data[0].is_finite() && data[1].is_finite()).then_some(data)
    }
}

/// Returns the angle that the page angle of `p` about `centre` represents through the angle map `x`, reduced by whole
/// turns to lie in the turn that starts at the lower angular limit and runs in the direction of the axis, and
/// whether it lies within the angular limits.
fn polar_angle(centre: Point, x: &AxisMap, p: Point) -> (f64, bool) {
    let phi = (centre.y - p.y).atan2(p.x - centre.x);
    let span = x.end - x.start;
    if !(span.is_finite() && span != 0.0) {
        return (f64::NAN, false);
    }
    // The fraction of the way from the lower to the upper limit, with one turn being `period` of it.
    let period = std::f64::consts::TAU / span.abs();
    let mut t = ((phi - x.start) / span).rem_euclid(period);
    // A point on the start edge, reached from the far side of the turn by rounding, lies at the lower limit.
    if period - t < 1e-12 {
        t = 0.0;
    }
    let inside = t <= 1.0 + 1e-12;
    (x.min + t * (x.max - x.min), inside)
}

/// The area of a bar or a cell as drawn, in figure space, from which the bar or cell under a pointer is found.
#[derive(Clone, Debug, PartialEq)]
pub enum HitArea {
    /// An axis-aligned rectangle: a bar or cell of a Cartesian axes.
    Rect(Rect),
    /// A closed polygon, without its first vertex repeated at its end: a bar or cell of a polar axes, whose outline
    /// follows the axes.
    Polygon(Vec<Point>),
}

impl HitArea {
    /// Returns whether `p` lies in the area. A point on the boundary of a rectangle lies in it; a point on the
    /// boundary of a polygon may or may not.
    pub fn contains(&self, p: Point) -> bool {
        match self {
            Self::Rect(rect) => rect.contains(p),
            Self::Polygon(points) => {
                let mut inside = false;
                let n = points.len();
                for k in 0..n {
                    let (a, b) = (points[k], points[(k + n - 1) % n]);
                    if (a.y > p.y) != (b.y > p.y) {
                        let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
                        if p.x < x {
                            inside = !inside;
                        }
                    }
                }
                inside
            }
        }
    }

    /// Returns the smallest axis-aligned rectangle that contains the area.
    pub fn bounds(&self) -> Rect {
        match self {
            Self::Rect(rect) => *rect,
            Self::Polygon(points) => {
                let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
                let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
                for p in points {
                    x0 = x0.min(p.x);
                    y0 = y0.min(p.y);
                    x1 = x1.max(p.x);
                    y1 = y1.max(p.y);
                }
                if x0 > x1 {
                    return Rect::new(0.0, 0.0, 0.0, 0.0);
                }
                Rect::new(x0, y0, x1 - x0, y1 - y0)
            }
        }
    }
}

/// The legend that draws an entry: the legend inside the plot area of an axes, or a legend panel of the figure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LegendOwner {
    /// The legend inside the plot area of the axes with this identifier.
    Axes(NodeId),
    /// The legend panel with this identifier.
    Panel(NodeId),
}

/// Hit-testing geometry for one legend entry.
#[derive(Clone, Debug, PartialEq)]
pub struct LegendHit {
    /// The legend that draws the entry.
    pub legend: LegendOwner,
    /// The `(axes, artist)` pairs of the artists the entry stands for, in the order a legend panel lists entries:
    /// by the reading order of the cells of their axes (row, then column, with the order of the axes in the figure
    /// breaking ties), then by the order of the artists in their axes. An entry of an inside legend stands for one
    /// artist; an entry of a legend panel stands for every artist whose label and sample it shares.
    pub targets: Vec<(NodeId, NodeId)>,
    pub rect: Rect,
}

/// Hit-testing geometry for one panel, whatever its content.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelHit {
    pub id: NodeId,
    /// The area of the panel, in figure space: the cells it occupies, inset by the aligned margins of the plot areas
    /// in its rows and columns.
    pub rect: Rect,
}

/// What holds a colourbar: a panel, an axes that draws it inside its plot area, or a legend that lists its scale as a
/// bar item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColourbarOwner {
    /// The panel with this identifier.
    Panel(NodeId),
    /// The axes with this identifier.
    Axes(NodeId),
    /// A bar item of a legend: inside the plot area of an axes or in a legend panel. A legend may hold several bar
    /// items, which [`ColourbarHit::scale`] tells apart.
    Legend(LegendOwner),
}

/// Hit-testing geometry for one colourbar.
#[derive(Clone, Debug, PartialEq)]
pub struct ColourbarHit {
    pub owner: ColourbarOwner,
    /// The colour scale the bar shows.
    pub scale: NodeId,
    /// The direction of the bar; an inside colourbar is horizontal on the south and north edges and vertical on the
    /// east and west edges, and a bar item of a legend is vertical, or horizontal on a row of its own.
    pub orientation: ColourbarOrientation,
    /// The bar, in figure space: for a continuous, discrete or indexed scale the coloured bar itself, without ticks,
    /// labels, title or keys; for a categorical scale the union of its category squares.
    pub bar: Rect,
    /// The squares of a categorical scale, one per category in category order; empty for any other kind.
    pub entries: Vec<Rect>,
    /// The out-of-range keys drawn, each with the category it shows.
    pub keys: Vec<ColourbarKeyHit>,
    /// The box drawn behind an inside colourbar, enclosing its title, bar, keys and labels, which lies `length` of
    /// the edge along it, centred, and is inset from the plot area by a margin; `None` for a panel colourbar and for a
    /// bar item of a legend, whose box (if any) is the legend's.
    pub backdrop: Option<Rect>,
    /// How scale values lie along the bar: `min` at `start` and `max` at `end`, which is the low end of the bar (its
    /// bottom when it stands, its left when it lies), in base-10 logarithms under a logarithmic norm. The limits are
    /// those the bar is drawn over: a continuous scale's limits, automatic ones resolved; a discrete scale's first and
    /// last edges; an indexed scale's −0.5 to n − 0.5, so that entry k lies at the centre of its block; and, for
    /// limits that coincide, the span of one unit (one decade under a logarithmic norm) about them that the bar is
    /// drawn over. `None` for a categorical scale, whose squares are its geometry.
    pub map: Option<AxisMap>,
    /// The band edges of a discrete bar as drawn, automatic ones as the compiler chose them, increasing; empty for
    /// every other kind.
    pub edges: Vec<f64>,
}

/// Hit-testing geometry for one out-of-range key of a colourbar.
#[derive(Clone, Debug, PartialEq)]
pub struct ColourbarKeyHit {
    /// The out-of-range category the key shows.
    pub key: ColourScaleKey,
    /// The square of the key, in figure space.
    pub rect: Rect,
    /// The source text of the label drawn beside the key: the author's label, or the default label as drawn.
    pub label: String,
}

/// The points one artist drew, in the order they were painted.
///
/// Only artists made of individual data points — series — have an entry. Each sample
/// names the index the point has in the artist's own data arrays, so a front end reports the index
/// and the values the user's data holds even when the series was decimated to fit the view; see
/// [`crate::maths::decimate`]. A point the artist did not draw, because it could not be placed or
/// because decimation removed it, has no sample.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtistHit {
    pub axes: NodeId,
    pub artist: NodeId,
    /// The drawn points, in ascending source index.
    pub samples: Vec<Sample>,
}

/// The placement of one image drawn in a two-dimensional axes, from which the pixel under a pointer is found.
///
/// Only images drawn in two-dimensional axes have an entry: there a pointer position maps back into pixel space
/// through [`ImageHit::to_pixel`]. A hidden image, an image the compiler skipped, and an image drawn on the floor or
/// a wall of a three-dimensional axes, which has no such inverse, have none.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageHit {
    pub axes: NodeId,
    pub artist: NodeId,
    /// Maps figure space into the pixel space of the image, in which the pixel in row `j` and column `i` of the
    /// artist's array covers `[i, i + 1] × [j, j + 1]`.
    pub to_pixel: PixelMap,
    /// The number of columns of pixels.
    pub columns: usize,
    /// The number of rows of pixels.
    pub rows: usize,
}

/// How figure space maps into the pixel space of an image drawn in a two-dimensional axes.
#[derive(Clone, Debug, PartialEq)]
pub enum PixelMap {
    /// The inverse of the affine placement of the image (the transform of its group, inverted).
    Affine(Transform),
    /// Through the axes: a figure-space point is mapped to data by the two axis maps, then linearly from the data
    /// coordinates of the first and last pixel edge (`columns` along x, `rows` along y) to pixel space, which spans
    /// `pixels` (the number of columns and of rows).
    Axes {
        x: AxisMap,
        y: AxisMap,
        columns: [f64; 2],
        rows: [f64; 2],
        pixels: [usize; 2],
    },
    /// Through a polar axes: a figure-space point is converted to its page angle and its distance from `centre`,
    /// mapped to data by the two axis maps (as [`AxesHitKind::Polar`] describes them), then linearly from the data
    /// coordinates of the first and last pixel edge to pixel space, as [`PixelMap::Axes`] does.
    Polar {
        centre: Point,
        x: AxisMap,
        y: AxisMap,
        columns: [f64; 2],
        rows: [f64; 2],
        pixels: [usize; 2],
    },
}

impl PixelMap {
    /// Maps a figure-space point into pixel space.
    pub fn to_pixel(&self, p: Point) -> Point {
        match self {
            Self::Affine(transform) => transform.apply(p),
            Self::Axes {
                x,
                y,
                columns,
                rows,
                pixels,
            } => {
                let along = |edges: [f64; 2], value: f64, n: usize| {
                    n as f64 * (value - edges[0]) / (edges[1] - edges[0])
                };
                Point::new(
                    along(*columns, x.to_data(p.x), pixels[0]),
                    along(*rows, y.to_data(p.y), pixels[1]),
                )
            }
            Self::Polar {
                centre,
                x,
                y,
                columns,
                rows,
                pixels,
            } => {
                let along = |edges: [f64; 2], value: f64, n: usize| {
                    n as f64 * (value - edges[0]) / (edges[1] - edges[0])
                };
                let (theta, _) = polar_angle(*centre, x, p);
                let rho = (p.x - centre.x).hypot(p.y - centre.y);
                Point::new(
                    along(*columns, theta, pixels[0]),
                    along(*rows, y.to_data(rho), pixels[1]),
                )
            }
        }
    }

    /// Maps a pixel-space point forward into figure space, or returns `None` when it has no place there (an affine
    /// map that cannot be inverted, or a data coordinate that a logarithmic axis cannot place).
    pub fn from_pixel(&self, p: Point) -> Option<Point> {
        let q = match self {
            Self::Affine(transform) => transform.inverse()?.apply(p),
            Self::Axes {
                x,
                y,
                columns,
                rows,
                pixels,
            } => {
                let data = |edges: [f64; 2], value: f64, n: usize| {
                    edges[0] + value / n as f64 * (edges[1] - edges[0])
                };
                Point::new(
                    x.to_figure(data(*columns, p.x, pixels[0])),
                    y.to_figure(data(*rows, p.y, pixels[1])),
                )
            }
            Self::Polar {
                centre,
                x,
                y,
                columns,
                rows,
                pixels,
            } => {
                let data = |edges: [f64; 2], value: f64, n: usize| {
                    edges[0] + value / n as f64 * (edges[1] - edges[0])
                };
                let phi = x.to_figure(data(*columns, p.x, pixels[0]));
                let rho = y.to_figure(data(*rows, p.y, pixels[1]));
                Point::new(centre.x + rho * phi.cos(), centre.y - rho * phi.sin())
            }
        };
        (q.x.is_finite() && q.y.is_finite()).then_some(q)
    }
}

/// One bar drawn in a two-dimensional axes, from which the bar under a pointer is found.
///
/// The numbers are those of the artist's own data at `index`, so that a front end reports what the user's data holds:
/// the position, the value and the base. Because a value is a length from the base, the hit also records the segment
/// the bar spans from its base to its end, which is what the reader sees. A bar that was not drawn, because its
/// position or value is not finite, its series is hidden or it paints nothing, has no entry.
#[derive(Clone, Debug, PartialEq)]
pub struct BarHit {
    pub axes: NodeId,
    pub artist: NodeId,
    /// The index of the bar in the arrays of its series.
    pub index: usize,
    /// The area of the bar as drawn, in figure space: in Cartesian axes the part of its rectangle inside the plot
    /// area, and in polar axes its outline, clamped to the plot region.
    pub area: HitArea,
    /// Where a callout is anchored, in figure space: the centre of the area in Cartesian axes, and in polar axes the
    /// point at the middle angle and the middle radius of the drawn wedge.
    pub anchor: Point,
    /// The position of the bar along the category axis, without its offset.
    pub position: f64,
    /// The value of the bar: its length from its base.
    pub value: f64,
    /// The base from which the bar starts.
    pub base: f64,
    /// The start and end of the bar along the value axis: its base and its base plus its value.
    pub segment: (f64, f64),
    /// The count and density of the bin the bar draws, when its series records the binning of a histogram.
    pub bin: Option<BinReading>,
}

/// What a bin of a histogram holds, recovered from the value drawn for it, the extent of the bin and the binning
/// record of its artist (ADR 0029).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinReading {
    /// The number of samples in the bin.
    pub count: f64,
    /// The probability density of the bin: its count divided by the total and by its width, or by its area for a
    /// bin of two dimensions.
    pub density: f64,
}

impl BinReading {
    /// Recovers the count and density of a bin from `value`, the value drawn for it under `normalisation`, `size`,
    /// the width or area of the bin, and `total`, the number of samples counted. A cumulative value is first
    /// differenced with `before`, the cumulative value of the bins before it (0 for the first bin), so that the count
    /// is that of the bin alone. Returns `None` when the value or the size is not finite.
    #[must_use]
    pub fn recover(
        normalisation: HistNorm,
        total: f64,
        value: f64,
        before: f64,
        size: f64,
    ) -> Option<BinReading> {
        if !(value.is_finite() && size.is_finite()) {
            return None;
        }
        let count = match normalisation {
            HistNorm::Count => value,
            HistNorm::Probability => value * total,
            HistNorm::Percentage => value * total / 100.0,
            HistNorm::CountDensity => value * size,
            HistNorm::Density => value * total * size,
            HistNorm::CumulativeCount => value - before,
            HistNorm::Cdf => (value - before) * total,
        };
        Some(BinReading {
            count,
            density: count / (total * size),
        })
    }
}

/// One cell drawn in a two-dimensional axes, from which the cell under a pointer is found.
///
/// The row and column index the artist's own array of values, and the edges and the value are those its data holds,
/// so that a front end reports what the user's data holds rather than anything read back from the page. A cell that
/// was not drawn, because its scale leaves it transparent (as the NaN padding of a ragged row is), its alpha is 0,
/// one of its edges cannot be placed on a logarithmic axis, it lies outside the plot area or its artist is hidden,
/// has no entry.
#[derive(Clone, Debug, PartialEq)]
pub struct CellHit {
    pub axes: NodeId,
    pub artist: NodeId,
    /// The row of the cell in the artist's array of values.
    pub row: usize,
    /// The column of the cell in the artist's array of values.
    pub column: usize,
    /// The area of the cell as drawn inside its gap, in figure space: in Cartesian axes the part of its rectangle
    /// that lies inside the plot area, and in polar axes its outline, clamped to the plot region.
    pub area: HitArea,
    /// Where a callout is anchored, in figure space: the centre of the area in Cartesian axes, and in polar axes the
    /// point at the middle angle and the middle radius of the drawn sector.
    pub anchor: Point,
    /// The edges of the cell's column, edges `column` and `column + 1`, in the order of the data.
    pub column_edges: (f64, f64),
    /// The edges of the cell's row, edges `row` and `row + 1`, in the order of the data.
    pub row_edges: (f64, f64),
    /// The count and density of the bin the cell draws, when the cells record the binning of a two-dimensional
    /// histogram.
    pub bin: Option<BinReading>,
}

/// One step of stairs drawn in a two-dimensional axes: the run of the outline at the level of one value, from which
/// the step under a pointer is found.
#[derive(Clone, Debug, PartialEq)]
pub struct StepHit {
    pub axes: NodeId,
    pub artist: NodeId,
    /// The index of the value in the artist's array of values.
    pub index: usize,
    /// The two edges between which the value holds, in the order of the data: the edges themselves for stairs
    /// between edges, and for stairs through samples the edges that `step` places.
    pub edges: (f64, f64),
    /// The value held across the step.
    pub value: f64,
    /// The run of the outline at the level of the value as drawn, in figure space, from the end at the first edge to
    /// the end at the second: its two ends in Cartesian axes, and in polar axes the vertices of the arc it follows.
    pub run: Vec<Point>,
    /// The count and density of the bin the step draws, when the stairs record the binning of a histogram.
    pub bin: Option<BinReading>,
}

impl StepHit {
    /// Returns the point of the step's run nearest `p`, in figure space.
    #[must_use]
    pub fn nearest(&self, p: Point) -> Point {
        let nearest_on = |from: Point, to: Point| {
            let (dx, dy) = (to.x - from.x, to.y - from.y);
            let length = dx * dx + dy * dy;
            let t = if length > 0.0 {
                (((p.x - from.x) * dx + (p.y - from.y) * dy) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            Point::new(from.x + t * dx, from.y + t * dy)
        };
        let distance = |q: Point| (q.x - p.x).powi(2) + (q.y - p.y).powi(2);
        match self.run.as_slice() {
            [] => p,
            [only] => *only,
            run => run
                .windows(2)
                .map(|pair| nearest_on(pair[0], pair[1]))
                .min_by(|a, b| distance(*a).total_cmp(&distance(*b)))
                .unwrap_or(run[0]),
        }
    }
}

/// All interactive geometry of a compiled figure.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HitMap {
    pub axes: Vec<AxesHit>,
    pub legend_entries: Vec<LegendHit>,
    /// The area of every panel of the figure, in the order the figure lists them.
    pub panels: Vec<PanelHit>,
    /// The drawn points of every series, in the order their axes were compiled.
    pub artists: Vec<ArtistHit>,
    /// The placement of every image drawn in a two-dimensional axes, in paint order.
    pub images: Vec<ImageHit>,
    /// Every colourbar drawn, in paint order: the inside colourbars and the bar items of the inside legends of the
    /// axes, then the colourbars and the bar items of the legends of the panels.
    pub colourbars: Vec<ColourbarHit>,
    /// Every bar drawn in a two-dimensional axes, in paint order.
    pub bars: Vec<BarHit>,
    /// Every step of stairs drawn in a two-dimensional axes, in paint order.
    pub steps: Vec<StepHit>,
    /// Every cell drawn in a two-dimensional axes, in paint order.
    pub cells: Vec<CellHit>,
}

/// Returns the item drawn last whose area contains `p`, which is the one the reader sees on top where areas
/// overlap.
fn last_containing<T>(items: &[T], p: Point, area: impl Fn(&T) -> &HitArea) -> Option<&T> {
    items.iter().rev().find(|item| area(item).contains(p))
}

impl AxesHit {
    /// Returns whether `p` lies in the plot region of the axes: its plot rectangle, and for a polar axes the disc or
    /// sector of the rim within it, the hole included.
    pub fn contains(&self, p: Point) -> bool {
        if !self.plot_rect.contains(p) {
            return false;
        }
        match &self.kind {
            AxesHitKind::Polar { centre, x, y } => {
                let rho = (p.x - centre.x).hypot(p.y - centre.y);
                let rim = y.start.max(y.end);
                rho <= rim + 1e-9 && (rho == 0.0 || polar_angle(*centre, x, p).1)
            }
            _ => true,
        }
    }
}

impl HitMap {
    /// Returns the top-most axes whose plot region contains `p`: its plot rectangle, and for a polar axes the disc or
    /// sector within it.
    pub fn axes_at(&self, p: Point) -> Option<&AxesHit> {
        self.axes.iter().rev().find(|a| a.contains(p))
    }

    /// Returns the legend entry containing `p`, if any.
    pub fn legend_entry_at(&self, p: Point) -> Option<&LegendHit> {
        self.legend_entries
            .iter()
            .rev()
            .find(|e| e.rect.contains(p))
    }

    /// Returns the drawn data point nearest to `p` within `radius` points, and the artist that drew it.
    ///
    /// Distance is measured in figure space rather than in data units, so the nearest point is the
    /// one nearest on the page whatever the scales and the aspect ratio of the axes are. Points at
    /// equal distance are resolved in favour of the one painted later, which is the one the reader
    /// sees on top.
    pub fn sample_at(&self, p: Point, radius: f64) -> Option<(&ArtistHit, &Sample)> {
        let squared = |s: &Sample| (s.position.x - p.x).powi(2) + (s.position.y - p.y).powi(2);
        let mut best: Option<(&ArtistHit, &Sample, f64)> = None;
        for artist in &self.artists {
            for sample in &artist.samples {
                let d = squared(sample);
                if d <= radius * radius && best.is_none_or(|(_, _, b)| d <= b) {
                    best = Some((artist, sample, d));
                }
            }
        }
        best.map(|(artist, sample, _)| (artist, sample))
    }

    /// Returns the bar drawn last whose area contains `p`, which is the one the reader sees on top where bars
    /// overlap.
    pub fn bar_at(&self, p: Point) -> Option<&BarHit> {
        last_containing(&self.bars, p, |bar| &bar.area)
    }

    /// Returns the cell drawn last whose area contains `p`, which is the one the reader sees on top where cells
    /// overlap. A point in the gap between two cells lies in neither.
    pub fn cell_at(&self, p: Point) -> Option<&CellHit> {
        last_containing(&self.cells, p, |cell| &cell.area)
    }

    /// Returns the step of stairs whose run lies nearest to `p` within `radius` points, measured in figure space;
    /// of steps at equal distance, the one painted later, which the reader sees on top.
    pub fn step_at(&self, p: Point, radius: f64) -> Option<&StepHit> {
        let mut best: Option<(&StepHit, f64)> = None;
        for step in &self.steps {
            let q = step.nearest(p);
            let d = (q.x - p.x).powi(2) + (q.y - p.y).powi(2);
            if d <= radius * radius && best.is_none_or(|(_, b)| d <= b) {
                best = Some((step, d));
            }
        }
        best.map(|(step, _)| step)
    }

    /// Returns the image drawn last under `p`, with the row and column of its pixel there.
    ///
    /// The pixel coordinates are floored, so the image is the half-open extent `[0, columns) × [0, rows)` of its
    /// pixel space and a boundary shared by two pixels belongs to the one with the higher index. Where images
    /// overlap, the one painted last is the one the reader sees on top, so it is the one returned.
    pub fn pixel_at(&self, p: Point) -> Option<(&ImageHit, usize, usize)> {
        self.images.iter().rev().find_map(|hit| {
            let q = hit.to_pixel.to_pixel(p);
            let (column, row) = (q.x.floor(), q.y.floor());
            let inside =
                column >= 0.0 && row >= 0.0 && column < hit.columns as f64 && row < hit.rows as f64;
            // Inside the bounds the casts are exact; outside them (NaN included) they saturate and are discarded.
            inside.then_some((hit, row as usize, column as usize))
        })
    }
}
