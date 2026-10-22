//! Geometry that interactive front ends need to map pointer positions back onto figure elements.

use ironlab_ir::{ColorScaleKey, ColorbarOrientation, NodeId};

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
pub enum ColorbarOwner {
    /// The panel with this identifier.
    Panel(NodeId),
    /// The axes with this identifier.
    Axes(NodeId),
    /// A bar item of a legend: inside the plot area of an axes or in a legend panel. A legend may hold several bar
    /// items, which [`ColorbarHit::scale`] tells apart.
    Legend(LegendOwner),
}

/// Hit-testing geometry for one colourbar.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorbarHit {
    pub owner: ColorbarOwner,
    /// The colour scale the bar shows.
    pub scale: NodeId,
    /// The direction of the bar; an inside colourbar is horizontal on the south and north edges and vertical on the
    /// east and west edges, and a bar item of a legend is vertical, or horizontal on a row of its own.
    pub orientation: ColorbarOrientation,
    /// The bar, in figure space: for a continuous, discrete or indexed scale the coloured bar itself, without ticks,
    /// labels, title or keys; for a categorical scale the union of its category squares.
    pub bar: Rect,
    /// The squares of a categorical scale, one per category in category order; empty for any other kind.
    pub entries: Vec<Rect>,
    /// The out-of-range keys drawn, each with the category it shows.
    pub keys: Vec<ColorbarKeyHit>,
    /// The box drawn behind an inside colourbar, enclosing its title, bar, keys and labels, which lies `length` of
    /// the edge along it, centred, and is inset from the plot area by a margin; `None` for a panel colourbar and for a
    /// bar item of a legend, whose box (if any) is the legend's.
    pub backdrop: Option<Rect>,
}

/// Hit-testing geometry for one out-of-range key of a colourbar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorbarKeyHit {
    /// The out-of-range category the key shows.
    pub key: ColorScaleKey,
    /// The square of the key, in figure space.
    pub rect: Rect,
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
/// Only images drawn in two-dimensional axes have an entry: there the placement of an image is an affine map of
/// pixel space into figure space, so a pointer position maps back into pixel space by its inverse. A hidden image,
/// an image the compiler skipped, and an image drawn on the floor or a wall of a three-dimensional axes, which has
/// no such inverse, have none.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageHit {
    pub axes: NodeId,
    pub artist: NodeId,
    /// Maps figure space into the pixel space of the image, in which the pixel in row `j` and column `i` of the
    /// artist's array covers `[i, i + 1] × [j, j + 1]`; it is the inverse of the transform of the group that holds
    /// the image item.
    pub to_pixel: Transform,
    /// The number of columns of pixels.
    pub columns: usize,
    /// The number of rows of pixels.
    pub rows: usize,
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
    pub colorbars: Vec<ColorbarHit>,
}

impl HitMap {
    /// Returns the top-most axes whose plot rectangle contains `p`.
    pub fn axes_at(&self, p: Point) -> Option<&AxesHit> {
        self.axes.iter().rev().find(|a| a.plot_rect.contains(p))
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

    /// Returns the image drawn last under `p`, with the row and column of its pixel there.
    ///
    /// The pixel coordinates are floored, so the image is the half-open extent `[0, columns) × [0, rows)` of its
    /// pixel space and a boundary shared by two pixels belongs to the one with the higher index. Where images
    /// overlap, the one painted last is the one the reader sees on top, so it is the one returned.
    pub fn pixel_at(&self, p: Point) -> Option<(&ImageHit, usize, usize)> {
        self.images.iter().rev().find_map(|hit| {
            let q = hit.to_pixel.apply(p);
            let (column, row) = (q.x.floor(), q.y.floor());
            let inside =
                column >= 0.0 && row >= 0.0 && column < hit.columns as f64 && row < hit.rows as f64;
            // Inside the bounds the casts are exact; outside them (NaN included) they saturate and are discarded.
            inside.then_some((hit, row as usize, column as usize))
        })
    }
}
