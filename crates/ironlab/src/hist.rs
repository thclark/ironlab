//! Handles to histograms, groups of histograms, two-dimensional histograms and density
//! contours, which bin samples in the builder and store the binned values in the figure.
//!
//! The figure never holds the samples (ADR 0029): a histogram is stored as the bars or
//! stairs of its bins, a two-dimensional histogram as cells, and a density contour as a
//! contour of the binned field. Bars, stairs and cells also store a record of how they were
//! binned, from which a datatip reports the count and density of a bin. Each handle keeps
//! its own copy of the samples while it lives, so that a setter that changes the binning
//! can bin them again and replace the stored arrays; once the handle is dropped, the binned
//! values and their record are all that remains, and a viewer cannot rebin them.

use ironlab_ir::{
    Artist, Bar, BarOffset, BarWidth, Binning, Binning2d, CellEdges, CellGap, Cells, ColorSpec,
    Contour, Figure, Grid, Levels, LineStyle, NdArray, NodeId, Orientation, PerBar, Stairs,
    StairsData, Text,
};

use crate::artists::{BarColumns, BarMut, BarsColumn, data_ids, release_data};
use crate::binning::{
    BinRange, BinningError, Bins, HistNorm, HistStyle, Histogram, Histogram2d, HistogramSummary,
    bin_edges, histogram, histogram2d, normalise_1d,
};
use crate::color::IntoColorSpec;
use crate::color_scale;
use crate::data::{DataSource, matrix_array};

/// Returns the centre of each bin between neighbouring edges.
fn centres(edges: &[f64]) -> Vec<f64> {
    edges.windows(2).map(|e| (e[0] + e[1]) / 2.0).collect()
}

/// Returns the width of each bin between neighbouring edges.
fn widths(edges: &[f64]) -> Vec<f64> {
    edges.windows(2).map(|e| e[1] - e[0]).collect()
}

/// Stores a vector of values as a new, unnamed data array and returns its identifier.
fn store(fig: &mut Figure, values: Vec<f64>) -> NodeId {
    fig.add_data(NdArray::vector(values))
}

/// Converts an explicit range of the binning functions into the range of a record.
fn record_range(range: Option<BinRange>) -> Option<[f64; 2]> {
    range.map(|(min, max)| [min, max])
}

/// Returns whether a number of samples can be recorded: finite and not negative.
fn recordable_count(count: f64) -> bool {
    count.is_finite() && count >= 0.0
}

/// Returns whether a range can be recorded: absent, or finite and increasing. A range of
/// zero width, which the binning widens, is binned but not recorded.
fn recordable_range(range: Option<[f64; 2]>) -> bool {
    range.is_none_or(|[min, max]| min.is_finite() && max.is_finite() && min < max)
}

/// Returns the edges that a record or stairs store for the given bin edges: the edges, or,
/// for no bins, the one edge at 0 that one more edge than no bins requires.
fn stored_edges(edges: &[f64]) -> Vec<f64> {
    if edges.is_empty() {
        vec![0.0]
    } else {
        edges.to_vec()
    }
}

/// Returns the record of a histogram of one dimension binned as given, whose resolved edges
/// are the data array `edges`, or `None` when the record would describe no possible bins,
/// as a total made negative by negative weights would.
fn record(
    bins: &Bins,
    edges: NodeId,
    range: Option<BinRange>,
    norm: HistNorm,
    summary: HistogramSummary,
) -> Option<Binning> {
    let range = record_range(range);
    (recordable_count(summary.total) && recordable_range(range)).then(|| Binning {
        bins: bins.clone(),
        edges,
        range,
        normalization: norm,
        total: summary.total,
        excluded_non_finite: summary.excluded_non_finite as u64,
        excluded_out_of_range: summary.excluded_out_of_range as u64,
    })
}

/// Replaces the artist with the identifier of `artist` in place, at the same position among
/// the artists of its axes, and releases the arrays of the artist it replaces that nothing
/// else refers to and that have no name.
fn replace_artist(fig: &mut Figure, artist: Artist) {
    let id = artist.id();
    let old = fig
        .artist_mut(id)
        .map(|slot| std::mem::replace(slot, artist))
        .expect("a histogram handle always refers to an artist of its figure");
    for data in data_ids(&old) {
        release_data(fig, data);
    }
}

/// Returns the bars of the binned values `values` between the given edges: one bar at the
/// centre of each bin, as wide as the bin, not offset, from 0, so that the bars touch.
pub(crate) fn binned_bars(
    fig: &mut Figure,
    id: NodeId,
    edges: &[f64],
    values: NodeId,
    orientation: Orientation,
) -> Bar {
    Bar {
        id,
        orientation,
        positions: store(fig, centres(edges)),
        values,
        width: BarWidth::Data(store(fig, widths(edges))),
        offset: BarOffset::Widths(0.0),
        base: PerBar::Constant(0.0),
        ..Bar::default()
    }
}

/// A handle to a histogram created by [`hist`](crate::AxesMut::hist).
///
/// The histogram bins a sample, which the handle keeps, and stores the binned values in the
/// figure: as touching bars, one per bin ([`HistStyle::Bar`], the default), or as the stepped
/// outline of the bins, closed down to 0 at both ends ([`HistStyle::Step`]) and filled in
/// the colour of the outline ([`HistStyle::FilledStep`]). With them it stores a record of how
/// the bins were made: the bins and the range as given, the normalisation, the number of
/// samples counted and the numbers excluded (ADR 0029). The figure never holds the sample,
/// so a viewer of the figure cannot rebin it.
///
/// Every setter that changes the binning ([`bins`](Self::bins), [`range`](Self::range),
/// [`weights`](Self::weights) and [`normalize`](Self::normalize)) bins the sample again and
/// replaces the stored arrays and the record, releasing the arrays it replaces;
/// [`style`](Self::style) replaces the artist in place, keeping its identifier, its position
/// among the artists of its axes, its label, its colour, its orientation and its record. The
/// setters may therefore be chained in any order, and the figure ends as the final settings
/// alone would make it.
///
/// A specification that cannot be applied, such as zero bins, a width that is not positive,
/// a reversed range or weights of the wrong length, never panics: the histogram is stored
/// with no bins and no record, so that the figure stays valid and
/// [`Figure::validate`](crate::Figure::validate) warns that it has nothing to draw, and
/// [`error`](Self::error) returns the reason until a later setter fixes the specification.
/// [`summary`](Self::summary) reports how many samples the histogram includes and how many
/// it excludes, and why.
///
/// ```
/// use ironlab::prelude::*;
///
/// let sample = [0.5, 1.5, 1.5, 2.5, 2.5, 2.5, 3.5, f64::NAN];
/// let mut fig = Figure::new();
/// let mut ax = fig.axes(0, 0);
/// let mut h = ax.hist(sample);
/// h.bins(Bins::Count(4))
///     .normalize(HistNorm::Probability)
///     .style(HistStyle::Step)
///     .label("Sample");
/// assert_eq!(h.summary().excluded_non_finite, 1);
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct HistMut<'a> {
    fig: &'a mut Figure,
    id: NodeId,
    sample: Vec<f64>,
    weights: Option<Vec<f64>>,
    bins: Bins,
    range: Option<BinRange>,
    norm: HistNorm,
    style: HistStyle,
    /// The colour of the series: the face of the bars, or the outline of the stairs.
    color: ColorSpec,
    summary: HistogramSummary,
    error: Option<BinningError>,
}

impl<'a> HistMut<'a> {
    /// Bins the sample with the default bins and normalisation and appends the histogram,
    /// drawn as bars, to the artists of the axes.
    pub(crate) fn create(fig: &'a mut Figure, axes: NodeId, sample: DataSource) -> Self {
        let sample = sample.read(fig);
        let id = fig.alloc_node_id();
        let values = store(fig, Vec::new());
        let bar = binned_bars(fig, id, &[], values, Orientation::Vertical);
        fig.axes_mut(axes)
            .expect("an axes handle always refers to an axes of its figure")
            .artists
            .push(Artist::Bar(bar));
        let mut handle = Self {
            fig,
            id,
            sample,
            weights: None,
            bins: Bins::default(),
            range: None,
            norm: HistNorm::default(),
            style: HistStyle::Bar,
            color: ColorSpec::Auto,
            summary: HistogramSummary::default(),
            error: None,
        };
        handle.rebin();
        handle
    }
}

impl HistMut<'_> {
    /// Returns the node identifier of the histogram, which stays the same when the style
    /// changes.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns how many samples the histogram includes, and how many it excludes because
    /// they are not finite or lie outside the range or the edges, under the current
    /// settings. When the specification cannot be applied, every count is zero.
    #[must_use]
    pub fn summary(&self) -> HistogramSummary {
        self.summary
    }

    /// Returns why the sample cannot be binned as specified, or `None` when it can. While
    /// there is a reason, the histogram is stored with no bins and no record.
    #[must_use]
    pub fn error(&self) -> Option<&BinningError> {
        self.error.as_ref()
    }

    /// Sets how the range of the sample is divided into bins: a number of equal bins, a
    /// width, explicit edges or a rule. The default is numpy's automatic rule.
    pub fn bins(&mut self, bins: Bins) -> &mut Self {
        self.bins = bins;
        self.rebin()
    }

    /// Sets the range of values the bins span, in place of the range of the finite
    /// samples; samples outside it are excluded, and reported by
    /// [`summary`](Self::summary).
    pub fn range(&mut self, min: f64, max: f64) -> &mut Self {
        self.range = Some((min, max));
        self.rebin()
    }

    /// Weights the samples, one weight per sample, so that each bin holds the sum of the
    /// weights of its samples rather than their number. A sample whose weight is not
    /// finite is excluded. The weights are read, and are not stored in the figure.
    pub fn weights(&mut self, weights: impl Into<DataSource>) -> &mut Self {
        self.weights = Some(weights.into().read(self.fig));
        self.rebin()
    }

    /// Sets what the value of each bin represents: a count (the default), a probability,
    /// a density or a cumulative count, among others (see [`HistNorm`]).
    pub fn normalize(&mut self, norm: HistNorm) -> &mut Self {
        self.norm = norm;
        self.rebin()
    }

    /// Sets how the histogram is drawn: as touching bars (the default), or as the stepped
    /// outline of the bins, unfilled or filled. The histogram keeps its identifier, its
    /// position among the artists of its axes, its label, its colour, its orientation and
    /// its record.
    pub fn style(&mut self, style: HistStyle) -> &mut Self {
        self.style = style;
        let (label, visible, orientation) = self.kept();
        let artist = self.build(label, visible, orientation);
        replace_artist(self.fig, artist);
        self
    }

    /// Sets whether the bins lie along x, with the values along y (the default), or along
    /// y, with the values along x, as a marginal histogram beside the y axis of a scatter
    /// plot lies.
    pub fn orientation(&mut self, orientation: Orientation) -> &mut Self {
        match self.fig.artist_mut(self.id) {
            Some(Artist::Bar(b)) => b.orientation = orientation,
            Some(Artist::Stairs(s)) => s.orientation = orientation,
            _ => unreachable!("a histogram handle always refers to bars or stairs"),
        }
        self
    }

    /// Sets the label of the histogram, drawn beside its sample in a legend.
    pub fn label(&mut self, label: impl Into<Text>) -> &mut Self {
        let label = Some(label.into());
        match self.fig.artist_mut(self.id) {
            Some(Artist::Bar(b)) => b.label = label,
            Some(Artist::Stairs(s)) => s.label = label,
            _ => unreachable!("a histogram handle always refers to bars or stairs"),
        }
        self
    }

    /// Sets the colour of the histogram: the face of the bars, or the outline of the
    /// stairs, whose fill takes the colour of the outline. The default is the next colour
    /// of the colour order of the axes.
    pub fn color(&mut self, color: impl IntoColorSpec) -> &mut Self {
        self.color = color.into_color_spec();
        let (color, filled) = (self.color, self.style == HistStyle::FilledStep);
        match self.fig.artist_mut(self.id) {
            Some(Artist::Bar(b)) => b.face = color,
            Some(Artist::Stairs(s)) => {
                s.line.color = color;
                if filled {
                    s.fill = color;
                }
            }
            _ => unreachable!("a histogram handle always refers to bars or stairs"),
        }
        self
    }

    /// Returns the label, the visibility and the orientation of the stored artist, which a
    /// rebuilt artist keeps.
    fn kept(&self) -> (Option<Text>, bool, Orientation) {
        match self.fig.artist(self.id) {
            Some((_, Artist::Bar(b))) => (b.label.clone(), b.visible, b.orientation),
            Some((_, Artist::Stairs(s))) => (s.label.clone(), s.visible, s.orientation),
            _ => unreachable!("a histogram handle always refers to bars or stairs"),
        }
    }

    /// Bins the sample under the current settings and replaces the stored arrays and the
    /// record of the histogram, releasing the arrays it replaces. Every other property of
    /// the stored artist is kept.
    fn rebin(&mut self) -> &mut Self {
        let (label, visible, orientation) = self.kept();
        let artist = self.build(label, visible, orientation);
        let replaced: Vec<NodeId> = match (self.fig.artist_mut(self.id), artist) {
            (Some(Artist::Bar(old)), Artist::Bar(new)) => {
                let mut replaced = vec![
                    std::mem::replace(&mut old.positions, new.positions),
                    std::mem::replace(&mut old.values, new.values),
                ];
                if let BarWidth::Data(widths) = std::mem::replace(&mut old.width, new.width) {
                    replaced.push(widths);
                }
                replaced.extend(
                    std::mem::replace(&mut old.binning, new.binning).map(|record| record.edges),
                );
                replaced
            }
            (Some(Artist::Stairs(old)), Artist::Stairs(new)) => {
                let replaced = std::mem::replace(&mut old.data, new.data);
                // The record of stairs refers to their own edges, released with them.
                old.binning = new.binning;
                vec![replaced.positions(), replaced.values()]
            }
            _ => unreachable!("a rebinned histogram keeps its style"),
        };
        for data in replaced {
            release_data(self.fig, data);
        }
        self
    }

    /// Bins the sample under the current settings, stores the binned arrays, and returns
    /// the artist of the current style that draws them and records how they were binned,
    /// with the given label, visibility and orientation.
    fn build(&mut self, label: Option<Text>, visible: bool, orientation: Orientation) -> Artist {
        let binned = histogram(
            &self.sample,
            self.weights.as_deref(),
            &self.bins,
            self.range,
            self.norm,
        );
        let (edges, values, summary) = match binned {
            Ok(h) => {
                self.summary = h.summary;
                self.error = None;
                (h.edges, h.values, Some(h.summary))
            }
            Err(error) => {
                self.summary = HistogramSummary::default();
                self.error = Some(error);
                (Vec::new(), Vec::new(), None)
            }
        };
        let values = store(self.fig, values);
        // Stairs on edges hold one more edge than values, and so does a record, so that
        // stairs and records of no bins hold one edge, at 0, which draws nothing. The
        // record of stairs refers to the stairs' own edges, and that of bars to an array
        // of its own.
        let edges_data = summary.map(|_| store(self.fig, stored_edges(&edges)));
        let binning = summary.zip(edges_data).and_then(|(summary, edges)| {
            let binning = record(&self.bins, edges, self.range, self.norm, summary);
            if binning.is_none() {
                release_data(self.fig, edges);
            }
            binning
        });
        match self.style {
            HistStyle::Bar => Artist::Bar(Bar {
                label,
                visible,
                face: self.color,
                binning,
                ..binned_bars(self.fig, self.id, &edges, values, orientation)
            }),
            HistStyle::Step | HistStyle::FilledStep => Artist::Stairs(Stairs {
                id: self.id,
                label,
                visible,
                orientation,
                data: StairsData::Edges {
                    edges: match &binning {
                        Some(binning) => binning.edges,
                        None => store(self.fig, stored_edges(&edges)),
                    },
                    values,
                },
                base: Some(0.0),
                line: LineStyle {
                    color: self.color,
                    ..LineStyle::default()
                },
                fill: if self.style == HistStyle::FilledStep {
                    self.color
                } else {
                    ColorSpec::None
                },
                binning,
                ..Stairs::default()
            }),
        }
    }
}

/// The helper that draws stacked and grouped histograms, created by
/// [`hists`](crate::AxesMut::hists): the histogram counterpart of
/// [`BarsMut`](crate::BarsMut).
///
/// Every sample the helper holds is binned on one shared set of edges, so that the members
/// compare bin by bin: explicit edges are used as given, and any other specification is
/// applied to the pooled sample of all the members, as Matplotlib does when it is given
/// several samples. Each member is an ordinary bar series with its own label, its colour
/// from the colour order, its own values and its own binning record, whose exclusions are
/// those of its own sample. A member added by [`series`](Self::series)
/// stands in a column of its own, the columns splitting each bin into equal parts in the
/// order they first appear, the first at the low end; a member added by
/// [`stacked`](Self::stacked) spans its whole bin in the column of the name it is given and
/// rests on the values of the members of that column before it. The members are placed by
/// the code that places the series of [`BarsMut`](crate::BarsMut), with no gap between
/// clusters or columns, and with one width per bar, because bins may differ in width.
///
/// The normalisation and the range apply to every member. The members of a stacked column
/// are normalised by the pooled total of that column, as Matplotlib's
/// `hist(stacked=True, density=True)` and seaborn's default `common_norm=True` normalise
/// them, so that the top of the stack is the normalised histogram of the pooled sample and
/// each layer is the member's share of it: a stack of densities encloses unit area, and a
/// stack of probabilities, percentages or a CDF reaches 1, 100 or 1. A member standing in a
/// column of its own is normalised by its own total, because members side by side are
/// compared by shape. [`common_norm`](Self::common_norm) overrides either rule. Each
/// member's record holds the total its values were normalised by, so that a datatip
/// recovers the member's own count.
///
/// The settings may be given before or after the members are added; every call bins every
/// sample again and lays every member out again, releasing the arrays it replaces. A
/// specification that cannot be applied never panics: every member is stored with no bars
/// and no record, and [`error`](Self::error) returns the reason.
///
/// ```
/// use ironlab::prelude::*;
///
/// let control = [0.2, 0.8, 1.1, 1.4, 1.9, 2.3, 2.6, 3.1];
/// let treated = [1.2, 1.7, 2.1, 2.4, 2.8, 3.3, 3.6, 3.9];
/// let mut fig = Figure::new();
/// let mut ax = fig.axes(0, 0);
/// let mut hists = ax.hists(Bins::Count(8));
/// hists
///     .normalize(HistNorm::Density)
///     .stacked("all", "Control", control)
///     .stacked("all", "Treated", treated);
/// let ids = hists.ids();
/// // The stack, not each layer, encloses unit area.
/// let width = 3.7 / 8.0;
/// let area: f64 = ids
///     .iter()
///     .map(|id| match fig.ir().artist(*id) {
///         Some((_, ironlab::ir::Artist::Bar(bar))) => {
///             let values = fig.ir().data_array(bar.values).unwrap();
///             values.array.as_f64().unwrap().iter().sum::<f64>() * width
///         }
///         _ => unreachable!(),
///     })
///     .sum();
/// assert!((area - 1.0).abs() < 1e-9);
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct HistsMut<'a> {
    fig: &'a mut Figure,
    axes: NodeId,
    bins: Bins,
    range: Option<BinRange>,
    norm: HistNorm,
    /// Whether every member is normalised by the pooled total of all the members (`true`)
    /// or each by its own (`false`), or `None` for the default that follows the layout.
    common_norm: Option<bool>,
    /// The orientation of every member.
    orientation: Orientation,
    /// The members, in the order they were added, with the column each belongs to.
    columns: BarColumns,
    /// The sample of each member, in the order the members were added.
    samples: Vec<Vec<f64>>,
    error: Option<BinningError>,
}

impl<'a> HistsMut<'a> {
    /// Returns a helper for the axes `axes` holding no histogram yet, which bins its samples
    /// as `bins` specifies.
    pub(crate) fn new(fig: &'a mut Figure, axes: NodeId, bins: Bins) -> Self {
        let mut helper = Self {
            fig,
            axes,
            bins,
            range: None,
            norm: HistNorm::default(),
            common_norm: None,
            orientation: Orientation::Vertical,
            columns: BarColumns::default(),
            samples: Vec::new(),
            error: None,
        };
        helper.update();
        helper
    }
}

impl HistsMut<'_> {
    /// Adds a histogram of the given label and sample as a column of its own.
    pub fn series(&mut self, label: impl Into<Text>, sample: impl Into<DataSource>) -> &mut Self {
        let column = self.columns.own();
        self.add(column, label.into(), sample.into())
    }

    /// Adds a histogram of the given label and sample to the stacked column named
    /// `column`, which is created beside the columns before it when no histogram has named
    /// it yet. The histogram rests on the histograms of the column added before it.
    pub fn stacked(
        &mut self,
        column: impl Into<String>,
        label: impl Into<Text>,
        sample: impl Into<DataSource>,
    ) -> &mut Self {
        let column = BarsColumn::Stacked(column.into());
        self.add(column, label.into(), sample.into())
    }

    /// Sets what the value of each bin of every member represents (see [`HistNorm`]). The
    /// members of a stacked column are normalised by the pooled total of the column, so
    /// that the top of the stack is the normalised histogram of the pooled sample, and a
    /// member in a column of its own by its own total, unless
    /// [`common_norm`](Self::common_norm) says otherwise. Counts and count densities are
    /// not divided by a total, and are the same either way.
    pub fn normalize(&mut self, norm: HistNorm) -> &mut Self {
        self.norm = norm;
        self.update()
    }

    /// Normalises every member by the pooled total of all the members (`true`), or each
    /// member by its own total (`false`), in place of the default, under which the members
    /// of a stacked column share the total of their column and every other member has its
    /// own (seaborn's `common_norm`). The result is the same whether this is called before
    /// or after the members are added.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// let mut ax = fig.axes(0, 0);
    /// let mut hists = ax.hists(Bins::Edges(vec![0.0, 1.0, 2.0]));
    /// hists
    ///     .normalize(HistNorm::Probability)
    ///     .common_norm(false)
    ///     .stacked("all", "Small", [0.5])
    ///     .stacked("all", "Large", [0.5, 1.5, 1.5]);
    /// // Each layer is a distribution of its own, summing to 1, so the stack sums to 2.
    /// for id in hists.ids() {
    ///     let Some((_, ironlab::ir::Artist::Bar(bar))) = fig.ir().artist(id) else {
    ///         unreachable!()
    ///     };
    ///     let values = fig.ir().data_array(bar.values).unwrap();
    ///     let sum: f64 = values.array.as_f64().unwrap().iter().sum();
    ///     assert!((sum - 1.0).abs() < 1e-12);
    /// }
    /// ```
    pub fn common_norm(&mut self, common: bool) -> &mut Self {
        self.common_norm = Some(common);
        self.update()
    }

    /// Sets the range of values that the shared bins span, in place of the range of the
    /// pooled sample; the samples of each member outside it are excluded, and counted in
    /// that member's record.
    pub fn range(&mut self, min: f64, max: f64) -> &mut Self {
        self.range = Some((min, max));
        self.update()
    }

    /// Lays every histogram flat, with the bins along y and the values along x, as
    /// [`BarsMut::horizontal`](crate::BarsMut::horizontal) lays bar series flat.
    pub fn horizontal(&mut self) -> &mut Self {
        self.orientation = Orientation::Horizontal;
        self.update()
    }

    /// Returns the histograms of the helper, in the order they were added.
    #[must_use]
    pub fn ids(&self) -> Vec<NodeId> {
        self.columns.ids()
    }

    /// Returns why the samples cannot be binned as specified, or `None` when they can.
    /// While there is a reason, every member is stored with no bars and no record.
    #[must_use]
    pub fn error(&self) -> Option<&BinningError> {
        self.error.as_ref()
    }

    /// Returns the handle of the histogram added last, for styling.
    ///
    /// # Panics
    ///
    /// Panics when the helper holds no histogram.
    pub fn last(&mut self) -> BarMut<'_> {
        let id = *self
            .columns
            .ids()
            .last()
            .expect("the histograms helper holds a histogram");
        BarMut::new(self.fig, id, None)
    }

    /// Returns the handle of the histogram of the given index, in the order the histograms
    /// were added, for styling.
    ///
    /// # Panics
    ///
    /// Panics when the helper holds no histogram of that index.
    pub fn get(&mut self, index: usize) -> BarMut<'_> {
        let id = self.columns.ids()[index];
        BarMut::new(self.fig, id, None)
    }

    /// Adds a histogram of `sample` to `column`, and bins and lays out every member again.
    fn add(&mut self, column: BarsColumn, label: Text, sample: DataSource) -> &mut Self {
        let sample = sample.read(self.fig);
        let id = self.fig.alloc_node_id();
        // Empty arrays stand in until `update` stores the binned ones and releases these.
        let values = store(self.fig, Vec::new());
        let bar = Bar {
            label: Some(label),
            ..binned_bars(self.fig, id, &[], values, Orientation::Vertical)
        };
        self.fig
            .axes_mut(self.axes)
            .expect("the histograms helper refers to an axes of its figure")
            .artists
            .push(Artist::Bar(bar));
        self.columns.push(id, column);
        self.samples.push(sample);
        self.update()
    }

    /// Bins every sample on the shared edges under the current settings, stores the binned
    /// values and records, and lays out every member: the positions at the bin centres and
    /// the widths shared by every member, the offset of each column and the bases of each
    /// stacked column. The arrays it replaces are released.
    fn update(&mut self) -> &mut Self {
        let pooled = self.samples.concat();
        let (edges, binned): (Vec<f64>, Vec<Option<(Histogram, f64)>>) =
            match bin_edges(&pooled, &self.bins, self.range) {
                Ok(edges) => {
                    self.error = None;
                    // Without edges (no finite samples to estimate them from), each member
                    // is binned as specified, which records what each left out.
                    let shared = if edges.is_empty() {
                        self.bins.clone()
                    } else {
                        Bins::Edges(edges.clone())
                    };
                    let counts = self
                        .samples
                        .iter()
                        .map(|sample| {
                            histogram(sample, None, &shared, self.range, HistNorm::Count).ok()
                        })
                        .collect();
                    (edges, self.normalised(counts))
                }
                Err(error) => {
                    self.error = Some(error);
                    (Vec::new(), vec![None; self.samples.len()])
                }
            };

        let (fraction, placed) = self.columns.placement(0.0, 0.0);
        let positions = store(self.fig, centres(&edges));
        let member_widths = widths(&edges).iter().map(|w| w * fraction).collect();
        let member_widths = store(self.fig, member_widths);
        // Every member records the shared edges, which one array holds.
        let shared_edges = store(self.fig, stored_edges(&edges));
        let mut replaced = Vec::new();
        for ((id, offset), h) in placed.into_iter().zip(binned) {
            let (values, binning) = match h {
                Some((h, total)) => {
                    let summary = HistogramSummary { total, ..h.summary };
                    let binning = record(&self.bins, shared_edges, self.range, self.norm, summary);
                    (h.values, binning)
                }
                None => (Vec::new(), None),
            };
            let values = store(self.fig, values);
            let Some(Artist::Bar(bar)) = self.fig.artist_mut(id) else {
                unreachable!("a member of the histograms helper is a bar series");
            };
            replaced.push(std::mem::replace(&mut bar.positions, positions));
            replaced.push(std::mem::replace(&mut bar.values, values));
            if let BarWidth::Data(old) =
                std::mem::replace(&mut bar.width, BarWidth::Data(member_widths))
            {
                replaced.push(old);
            }
            bar.offset = BarOffset::Widths(offset);
            bar.orientation = self.orientation;
            replaced
                .extend(std::mem::replace(&mut bar.binning, binning).map(|record| record.edges));
        }
        // The shared arrays are released when no member holds them, which is the case
        // before the first member is added and when no member could be binned.
        replaced.extend([positions, member_widths, shared_edges]);
        for data in replaced {
            release_data(self.fig, data);
        }
        self.columns.stack(self.fig, edges.len().saturating_sub(1));
        self
    }

    /// Normalises the counts of every member, binned on the shared edges, and returns each
    /// member's histogram with the total it was normalised by: the pooled total of all the
    /// members under common normalisation, the pooled total of the member's stacked column
    /// by default, and otherwise the member's own total. A member that could not be binned
    /// stays `None` and adds nothing to a pooled total.
    fn normalised(&self, counts: Vec<Option<Histogram>>) -> Vec<Option<(Histogram, f64)>> {
        let own = |h: &Option<Histogram>| h.as_ref().map_or(0.0, |h| h.summary.total);
        let all: f64 = counts.iter().map(own).sum();
        let pooled = |column: &BarsColumn| -> f64 {
            self.columns
                .columns()
                .zip(&counts)
                .filter(|(c, _)| *c == column)
                .map(|(_, h)| own(h))
                .sum()
        };
        let totals: Vec<f64> = self
            .columns
            .columns()
            .zip(&counts)
            .map(|(column, h)| match (self.common_norm, column) {
                (Some(true), _) => all,
                (None, BarsColumn::Stacked(_)) => pooled(column),
                _ => own(h),
            })
            .collect();
        counts
            .into_iter()
            .zip(totals)
            .map(|(h, total)| {
                h.map(|h| {
                    let values = normalise_1d(&h.values, &widths(&h.edges), total, self.norm);
                    (Histogram { values, ..h }, total)
                })
            })
            .collect()
    }
}

/// The pairs of samples that a two-dimensional handle bins, with the settings of the
/// binning, shared by [`Hist2dMut`] and [`DensityContourMut`].
#[derive(Debug)]
struct Pairs {
    x: Vec<f64>,
    y: Vec<f64>,
    weights: Option<Vec<f64>>,
    bins: (Bins, Bins),
    range: (Option<BinRange>, Option<BinRange>),
    norm: HistNorm,
    min_count: Option<f64>,
    summary: HistogramSummary,
    error: Option<BinningError>,
}

impl Pairs {
    fn new(fig: &Figure, x: DataSource, y: DataSource) -> Self {
        Self {
            x: x.read(fig),
            y: y.read(fig),
            weights: None,
            bins: (Bins::default(), Bins::default()),
            range: (None, None),
            norm: HistNorm::default(),
            min_count: None,
            summary: HistogramSummary::default(),
            error: None,
        }
    }

    /// Bins the pairs under the current settings, recording the summary and the error, and
    /// returns the histogram, or `None` when the specification cannot be applied.
    fn bin(&mut self) -> Option<Histogram2d> {
        match histogram2d(
            &self.x,
            &self.y,
            self.weights.as_deref(),
            (&self.bins.0, &self.bins.1),
            self.range,
            self.norm,
            self.min_count,
        ) {
            Ok(h) => {
                self.summary = h.summary;
                self.error = None;
                Some(h)
            }
            Err(error) => {
                self.summary = HistogramSummary::default();
                self.error = Some(error);
                None
            }
        }
    }

    /// Returns the record of the binning that produced `summary`, whose resolved edges are
    /// the data arrays `x_edges` and `y_edges`, or `None` when the record would describe no
    /// possible bins.
    fn record(
        &self,
        summary: HistogramSummary,
        x_edges: NodeId,
        y_edges: NodeId,
    ) -> Option<Binning2d> {
        let (x_range, y_range) = (record_range(self.range.0), record_range(self.range.1));
        let valid = recordable_count(summary.total)
            && self.min_count.is_none_or(recordable_count)
            && recordable_range(x_range)
            && recordable_range(y_range);
        valid.then(|| Binning2d {
            x_bins: self.bins.0.clone(),
            y_bins: self.bins.1.clone(),
            x_edges,
            y_edges,
            x_range,
            y_range,
            normalization: self.norm,
            min_count: self.min_count,
            total: summary.total,
            excluded_non_finite: summary.excluded_non_finite as u64,
            excluded_out_of_range: summary.excluded_out_of_range as u64,
        })
    }
}

/// Returns a histogram of no bins, which a two-dimensional handle stores when its
/// specification cannot be applied.
fn no_bins() -> Histogram2d {
    Histogram2d {
        x_edges: Vec::new(),
        y_edges: Vec::new(),
        values: crate::Matrix::from_fn(0, 0, |_, _| 0.0),
        summary: HistogramSummary::default(),
    }
}

/// Defines the setters that change the binning of a two-dimensional handle, each of which
/// bins the pairs again through the handle's `rebin`.
macro_rules! binning_2d_setters {
    ($handle:ident) => {
        impl $handle<'_> {
            /// Returns the node identifier of the artist, which stays the same when the
            /// binning changes.
            #[must_use]
            pub fn id(&self) -> NodeId {
                self.id
            }

            /// Returns how many pairs are included, and how many are excluded because a
            /// value or a weight is not finite or a value lies outside its range or edges,
            /// under the current settings. When the specification cannot be applied, every
            /// count is zero.
            #[must_use]
            pub fn summary(&self) -> HistogramSummary {
                self.pairs.summary
            }

            /// Returns why the pairs cannot be binned as specified, or `None` when they
            /// can. While there is a reason, the artist is stored with no bins.
            #[must_use]
            pub fn error(&self) -> Option<&BinningError> {
                self.pairs.error.as_ref()
            }

            /// Sets how the x values and the y values are divided into bins, each as
            /// [`HistMut::bins`] divides a sample. The default is numpy's automatic rule
            /// along both.
            pub fn bins(&mut self, x: Bins, y: Bins) -> &mut Self {
                self.pairs.bins = (x, y);
                self.rebin()
            }

            /// Sets the ranges, `(min, max)`, that the bins of the x values and of the y
            /// values span; a pair with a value outside its range is excluded.
            pub fn range(&mut self, x: (f64, f64), y: (f64, f64)) -> &mut Self {
                self.pairs.range = (Some(x), Some(y));
                self.rebin()
            }

            /// Weights the pairs, one weight per pair, so that each bin holds the sum of
            /// the weights of its pairs. The weights are read, and are not stored in the
            /// figure.
            pub fn weights(&mut self, weights: impl Into<DataSource>) -> &mut Self {
                self.pairs.weights = Some(weights.into().read(self.fig));
                self.rebin()
            }

            /// Sets what the value of each bin represents (see [`HistNorm`]); a density is
            /// divided by the area of its bin.
            pub fn normalize(&mut self, norm: HistNorm) -> &mut Self {
                self.pairs.norm = norm;
                self.rebin()
            }

            /// Sets the label of the artist, drawn beside its sample in a legend.
            pub fn label(&mut self, label: impl Into<Text>) -> &mut Self {
                self.artist().label = Some(label.into());
                self
            }

            /// Maps the binned values to colours through the colour scale with the given
            /// identifier, in place of the default scale of its axes, which is deleted
            /// when this call created it and nothing else names it, as
            /// [`CellsMut::color_scale`](crate::CellsMut::color_scale) does.
            pub fn color_scale(&mut self, scale: NodeId) -> &mut Self {
                self.created_scale =
                    color_scale::replace_scale(self.fig, self.id, scale, self.created_scale);
                self
            }
        }
    };
}

/// A handle to a two-dimensional histogram created by [`hist2d`](crate::AxesMut::hist2d).
///
/// The histogram bins pairs of samples, which the handle keeps, and stores the binned
/// values as seamless [cells](crate::AxesMut::cells) between the bin edges, coloured by
/// value through the default colour scale of the axes, so that a colourbar of that scale
/// reads the values off the colours. With them it stores a record of how the cells were
/// binned: the bins and the ranges as given, the normalisation, the minimum count, the
/// number of pairs counted and the numbers excluded (ADR 0029). The figure never holds the
/// samples, so a viewer of the figure cannot rebin them.
///
/// The setters that change the binning ([`bins`](Self::bins), [`range`](Self::range),
/// [`weights`](Self::weights), [`normalize`](Self::normalize) and
/// [`min_count`](Self::min_count)) bin the pairs again and replace the stored arrays and the
/// record, releasing the arrays they replace. A specification that cannot be applied never
/// panics: the histogram is stored with no cells and no record, and [`error`](Self::error)
/// returns the reason, as for [`HistMut`].
///
/// ```
/// use ironlab::prelude::*;
///
/// let x = [0.1, 0.4, 0.6, 0.9, 0.7];
/// let y = [0.2, 0.3, 0.8, 0.9, 0.6];
/// let mut fig = Figure::new();
/// let mut ax = fig.axes(0, 0);
/// ax.hist2d(x, y)
///     .bins(Bins::Count(2), Bins::Count(2))
///     .min_count(1.0);
/// let scale = ax.color_scale().id();
/// ax.colorbar(scale, InsideColorbarSide::East);
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct Hist2dMut<'a> {
    fig: &'a mut Figure,
    id: NodeId,
    pairs: Pairs,
    /// The default colour scale of the axes that the plotting call created, if any.
    created_scale: Option<NodeId>,
}

binning_2d_setters!(Hist2dMut);

impl<'a> Hist2dMut<'a> {
    /// Bins the pairs with the default bins and normalisation and appends the histogram to
    /// the artists of the axes, mapped through the default colour scale of the axes.
    pub(crate) fn create(fig: &'a mut Figure, axes: NodeId, x: DataSource, y: DataSource) -> Self {
        let pairs = Pairs::new(fig, x, y);
        let (scale, created) = color_scale::default_or_new(fig, axes);
        let id = fig.alloc_node_id();
        let values = fig.add_data(NdArray::default());
        fig.axes_mut(axes)
            .expect("an axes handle always refers to an axes of its figure")
            .artists
            .push(Artist::Cells(Cells {
                id,
                values,
                gap: CellGap { x: 0.0, y: 0.0 },
                color_scale: Some(scale),
                ..Cells::default()
            }));
        let mut handle = Self {
            fig,
            id,
            pairs,
            created_scale: created.then_some(scale),
        };
        handle.rebin();
        handle
    }
}

impl Hist2dMut<'_> {
    /// Returns the identifier of the data array of the binned values, of one row per y bin
    /// and one column per x bin.
    #[must_use]
    pub fn values_data(&self) -> NodeId {
        match self.fig.artist(self.id) {
            Some((_, Artist::Cells(c))) => c.values,
            _ => unreachable!("a 2D histogram handle always refers to cells"),
        }
    }

    /// Makes every bin whose count, before normalisation, is below `count` NaN, so that
    /// the colour scale draws it as a value it cannot place, which is nothing by default
    /// (Matplotlib's `cmin`). The count is a number of pairs, or a sum of weights, whatever
    /// the normalisation.
    pub fn min_count(&mut self, count: f64) -> &mut Self {
        self.pairs.min_count = Some(count);
        self.rebin()
    }

    /// Sets the fractions of the width and the height of each cell left empty, as
    /// [`CellsMut::gap`](crate::CellsMut::gap) does; the default of 0 draws seamless cells.
    pub fn gap(&mut self, x: f64, y: f64) -> &mut Self {
        self.artist().gap = CellGap { x, y };
        self
    }

    fn artist(&mut self) -> &mut Cells {
        match self.fig.artist_mut(self.id) {
            Some(Artist::Cells(c)) => c,
            _ => unreachable!("a 2D histogram handle always refers to cells"),
        }
    }

    /// Bins the pairs under the current settings and replaces the stored values, edges and
    /// record, releasing the arrays it replaces. The cells always lie on explicit edges, to
    /// which the record refers; without bins, each dimension holds one edge, at 0, so that
    /// there is nothing to draw.
    fn rebin(&mut self) -> &mut Self {
        let binned = self.pairs.bin();
        let summary = binned.as_ref().map(|h| h.summary);
        let h = binned.unwrap_or_else(no_bins);
        let values = self.fig.add_data(matrix_array(&h.values));
        let x_edges = store(self.fig, stored_edges(&h.x_edges));
        let y_edges = store(self.fig, stored_edges(&h.y_edges));
        let binning = summary.and_then(|summary| self.pairs.record(summary, x_edges, y_edges));
        let (columns, rows) = (CellEdges::Edges(x_edges), CellEdges::Edges(y_edges));
        let cells = self.artist();
        cells.binning = binning;
        let replaced = [
            Some(std::mem::replace(&mut cells.values, values)),
            match std::mem::replace(&mut cells.columns, columns) {
                CellEdges::Edges(id) => Some(id),
                CellEdges::Unit => None,
            },
            match std::mem::replace(&mut cells.rows, rows) {
                CellEdges::Edges(id) => Some(id),
                CellEdges::Unit => None,
            },
        ];
        for data in replaced.into_iter().flatten() {
            release_data(self.fig, data);
        }
        self
    }
}

/// A handle to a density contour created by
/// [`density_contour`](crate::AxesMut::density_contour).
///
/// The contour bins pairs of samples as [`hist2d`](crate::AxesMut::hist2d) bins them, and
/// stores the binned values as a contour whose field is sampled at the centres of the bins,
/// on a rectilinear grid. The isolines are coloured by level through the default colour
/// scale of the axes, and [`filled`](Self::filled) fills the bands between them instead. The
/// figure never holds the samples, so a viewer of the figure cannot rebin them; a contour
/// stores no binning record, because its field is sampled at points rather than held in
/// bins.
///
/// The setters that change the binning bin the pairs again and replace the stored grid and
/// field, releasing those they replace. A specification that cannot be applied never
/// panics: the contour is stored with an empty field, and [`error`](Self::error) returns
/// the reason, as for [`HistMut`].
///
/// ```
/// use ironlab::prelude::*;
///
/// let x: Vec<f64> = (0..200).map(|k| (f64::from(k) * 0.37).sin()).collect();
/// let y: Vec<f64> = (0..200).map(|k| (f64::from(k) * 0.53).cos()).collect();
/// let mut fig = Figure::new();
/// fig.axes(0, 0)
///     .density_contour(&x, &y)
///     .bins(Bins::Count(12), Bins::Count(12))
///     .normalize(HistNorm::Density)
///     .levels(6)
///     .filled(true);
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct DensityContourMut<'a> {
    fig: &'a mut Figure,
    id: NodeId,
    pairs: Pairs,
    /// The default colour scale of the axes that the plotting call created, if any.
    created_scale: Option<NodeId>,
}

binning_2d_setters!(DensityContourMut);

impl<'a> DensityContourMut<'a> {
    /// Bins the pairs with the default bins and normalisation and appends the contour of the
    /// binned values to the artists of the axes, coloured through the default colour scale
    /// of the axes.
    pub(crate) fn create(fig: &'a mut Figure, axes: NodeId, x: DataSource, y: DataSource) -> Self {
        let pairs = Pairs::new(fig, x, y);
        let (scale, created) = color_scale::default_or_new(fig, axes);
        let id = fig.alloc_node_id();
        let grid = Grid::Rectilinear {
            x: store(fig, Vec::new()),
            y: store(fig, Vec::new()),
        };
        let z = fig.add_data(NdArray::default());
        fig.axes_mut(axes)
            .expect("an axes handle always refers to an axes of its figure")
            .artists
            .push(Artist::Contour(Contour {
                id,
                grid,
                z,
                color_scale: Some(scale),
                ..Contour::default()
            }));
        let mut handle = Self {
            fig,
            id,
            pairs,
            created_scale: created.then_some(scale),
        };
        handle.rebin();
        handle
    }
}

impl DensityContourMut<'_> {
    /// Chooses approximately `count` levels at nice values spanning the binned values, as
    /// [`ContourMut::levels`](crate::ContourMut::levels) does.
    pub fn levels(&mut self, count: u32) -> &mut Self {
        self.artist().levels = Some(Levels::Auto { count });
        self
    }

    /// Draws the contour at the given levels, which must be finite and increasing, as
    /// [`ContourMut::level_values`](crate::ContourMut::level_values) does.
    pub fn level_values(&mut self, levels: impl AsRef<[f64]>) -> &mut Self {
        self.artist().levels = Some(Levels::Explicit {
            values: levels.as_ref().to_vec(),
        });
        self
    }

    /// Fills the bands between the levels (`true`), as Plotly's `histogram2dcontour` draws
    /// them, or draws the isolines only (`false`, the default).
    pub fn filled(&mut self, filled: bool) -> &mut Self {
        self.artist().fill = filled;
        self
    }

    fn artist(&mut self) -> &mut Contour {
        match self.fig.artist_mut(self.id) {
            Some(Artist::Contour(c)) => c,
            _ => unreachable!("a density contour handle always refers to a contour"),
        }
    }

    /// Bins the pairs under the current settings and replaces the stored grid and field,
    /// releasing those it replaces.
    fn rebin(&mut self) -> &mut Self {
        let h = self.pairs.bin().unwrap_or_else(no_bins);
        let grid = Grid::Rectilinear {
            x: store(self.fig, centres(&h.x_edges)),
            y: store(self.fig, centres(&h.y_edges)),
        };
        let z = self.fig.add_data(matrix_array(&h.values));
        let contour = self.artist();
        let old_grid = std::mem::replace(&mut contour.grid, grid);
        let old_z = std::mem::replace(&mut contour.z, z);
        let (Grid::Rectilinear { x, y } | Grid::Curvilinear { x, y }) = old_grid;
        for data in [x, y, old_z] {
            release_data(self.fig, data);
        }
        self
    }
}
