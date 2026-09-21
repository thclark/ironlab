//! Geometry that interactive front ends need to map pointer positions back onto figure elements.

use ironlab_ir::NodeId;

use crate::display::{Point, Rect, Transform};
use crate::maths::decimate::Sample;

/// Maps one data axis onto a coordinate range in figure space.
///
/// `start` is the figure-space coordinate (points) of the `min` limit and `end` that of the `max` limit. For a
/// horizontal axis `start < end`; for a vertical axis (y increasing downwards in figure space) `start > end`.
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

/// Hit-testing geometry for one legend entry.
#[derive(Clone, Debug, PartialEq)]
pub struct LegendHit {
    pub axes: NodeId,
    pub artist: NodeId,
    pub rect: Rect,
}

/// The points one artist drew, in the order they were painted.
///
/// Only artists made of individual data points — lines and scatters — have an entry. Each sample
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
    /// The drawn points of every line and scatter, in the order their axes were compiled.
    pub artists: Vec<ArtistHit>,
    /// The placement of every image drawn in a two-dimensional axes, in paint order.
    pub images: Vec<ImageHit>,
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
