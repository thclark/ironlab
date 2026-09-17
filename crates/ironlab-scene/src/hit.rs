//! Geometry that interactive front ends need to map pointer positions back onto figure elements.

use ironlab_ir::NodeId;

use crate::display::{Point, Rect};

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
    TwoD { x: AxisMap, y: AxisMap },
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

/// All interactive geometry of a compiled figure.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HitMap {
    pub axes: Vec<AxesHit>,
    pub legend_entries: Vec<LegendHit>,
}

impl HitMap {
    /// Returns the top-most axes whose plot rectangle contains `p`.
    pub fn axes_at(&self, p: Point) -> Option<&AxesHit> {
        self.axes.iter().rev().find(|a| a.plot_rect.contains(p))
    }

    /// Returns the legend entry containing `p`, if any.
    pub fn legend_entry_at(&self, p: Point) -> Option<&LegendHit> {
        self.legend_entries.iter().rev().find(|e| e.rect.contains(p))
    }
}
