//! Histograms in the facade (#93): `hist`, `hists`, `hist_counts` and their handles, which bin samples in the facade
//! and store the binned values as bars or stairs together with a record of how they were binned (ADR 0029).
//!
//! The binning itself (rules, numpy edge semantics, exclusions, weights and normalisations) is tested in
//! `binning.rs`; these tests check that the facade stores what the binning computes, in the form a reader of the
//! drawn figure relies on, and that the handles keep the figure clean as their settings change. They assume this API:
//!
//! - `AxesMut::hist(sample: impl Into<DataSource>) -> HistMut`, binned with `Bins::default()` and `HistNorm::Count`
//!   and drawn with `HistStyle::Bar`. A sample given by identifier is read, not referenced.
//! - `HistMut` setters `bins(Bins)`, `range(min, max)`, `weights(impl Into<DataSource>)`, `normalize(HistNorm)`,
//!   `style(HistStyle)`, `orientation(Orientation)`, `label` and `color(impl IntoColorSpec)` (the colour of the series:
//!   the face of the bars, or the outline of the stairs, whose fill takes the outline's colour); and the getters
//!   `id() -> NodeId`, `summary() -> HistogramSummary` and `error() -> Option<&BinningError>`. It has no `gap` and no
//!   `group`: histograms touch, and several histograms are placed together by `hists`.
//! - `AxesMut::hists(bins: Bins) -> HistsMut`, the histogram counterpart of `bars`, whose `series(label, sample)` adds
//!   a histogram as a column of its own and `stacked(column, label, sample)` adds one to the named stacked column.
//!   Every sample is binned on one shared set of edges: explicit edges as given, and any other specification applied
//!   to the pooled sample of all the members. The members of a stacked column are normalised by the pooled total of
//!   the column and every other member by its own total, unless `common_norm(bool)` normalises every member by the
//!   pooled total of all the members (`true`) or each by its own (`false`). The getters are `ids() -> Vec<NodeId>` and
//!   `error() -> Option<&BinningError>`.
//! - `AxesMut::hist_counts(edges, counts) -> BarMut`, which draws counts binned beforehand as touching bars.
//!
//! The style `Bar` stores a `Bar` artist at the bin centres, each bar as wide as its bin (`BarWidth::Data`), not
//! offset, from 0; the styles `Step` and `FilledStep` store `Stairs` on the bin edges, closed down to 0, the second
//! filled. Bars and stairs made from a sample carry a `Binning` record: the bins as given, the explicit range, the
//! normalisation, the total counted and the numbers excluded.

mod common;

use common::{
    BarExtent, artist, axes, bar, bar_extents, compile, data, filled_paths, filled_shapes,
    has_error_at, has_warning, stairs, stroke_colours,
};
use ironlab::IssueKind;
use ironlab::binning::{BinningError, bin_edges, histogram};
use ironlab::ir::{
    Artist, BarOffset, BarWidth, Binning, ColorSpec, NdArray, Orientation, PerBar, StairsData,
};
use ironlab::prelude::*;
use ironlab_scene::hit::{AxesHitKind, AxisMap};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);

/// A sample of ten values with 1, 2, 3 and 4 values in the unit bins from 0 to 4.
const STEPPED: [f64; 10] = [0.5, 1.5, 1.5, 2.5, 2.5, 2.5, 3.5, 3.5, 3.5, 3.5];

/// A sample of five values with 2, 1, 1 and 1 values in the bins between the edges 0, 1, 2, 3 and 4.5.
const OTHER: [f64; 5] = [0.2, 0.7, 1.1, 2.2, 3.9];

/// A sample of twelve finite values without ties, followed by a NaN and an infinity, for checks against the binning
/// functions.
const SPREAD: [f64; 14] = [
    2.1,
    -1.3,
    0.4,
    1.9,
    5.1,
    6.2,
    3.3,
    0.0,
    2.8,
    4.4,
    1.1,
    -0.7,
    f64::NAN,
    f64::INFINITY,
];

/// Agreement expected between an edge stored by the facade and one computed from it, which differ by the rounding
/// of a centre and a half width.
const TOL: f64 = 1e-12;

fn vector(fig: &Figure, id: NodeId) -> Vec<f64> {
    data(fig, id)
        .as_f64()
        .expect("histogram arrays are floating-point")
        .to_vec()
}

#[track_caller]
fn assert_close(actual: &[f64], expected: &[f64], what: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{what}: {actual:?} against {expected:?}"
    );
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (a - e).abs() <= TOL * e.abs().max(1.0),
            "{what}: {actual:?} against {expected:?}"
        );
    }
}

#[track_caller]
fn assert_extents_close(actual: &[BarExtent], expected: &[BarExtent], what: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{what}: {actual:?} against {expected:?}"
    );
    for (a, e) in actual.iter().zip(expected) {
        let near = |x: f64, y: f64| (x - y).abs() <= TOL * y.abs().max(1.0);
        assert!(
            near(a.low, e.low)
                && near(a.high, e.high)
                && near(a.base, e.base)
                && near(a.end, e.end),
            "{what}: {actual:?} against {expected:?}"
        );
    }
}

/// The bin edges and the value of each bin that a histogram artist draws.
///
/// For bars, each bar's extent along the category axis is resolved from its position, width and offset; the bars
/// must touch, so that the extents are one sequence of edges, and they must rise from 0. For stairs, the edges are
/// those stored, and the outline must be closed down to 0.
#[track_caller]
fn drawn_bins(fig: &Figure, id: NodeId) -> (Vec<f64>, Vec<f64>) {
    match artist(fig, id) {
        Artist::Bar(b) => {
            let values = vector(fig, b.values);
            let extents = bar_extents(fig, id);
            assert!(
                extents.iter().all(|e| e.base == 0.0),
                "the bars rise from 0: {extents:?}"
            );
            if extents.is_empty() {
                return (Vec::new(), values);
            }
            let lefts: Vec<f64> = extents.iter().map(|e| e.low).collect();
            let rights: Vec<f64> = extents.iter().map(|e| e.high).collect();
            assert_close(
                &lefts[1..],
                &rights[..rights.len() - 1],
                "each bar starts where the one before ends",
            );
            let mut edges = lefts;
            edges.push(*rights.last().unwrap());
            (edges, values)
        }
        Artist::Stairs(s) => {
            let StairsData::Edges { edges, values } = s.data else {
                panic!("histogram stairs lie between edges, not {:?}", s.data);
            };
            assert_eq!(s.base, Some(0.0), "the outline is closed down to 0");
            (vector(fig, edges), vector(fig, values))
        }
        other => panic!("a histogram is drawn as bars or stairs, found {other:?}"),
    }
}

/// The binning record of a histogram drawn as bars or stairs, as stored.
#[track_caller]
fn stored_record(fig: &Figure, id: NodeId) -> Option<&Binning> {
    match artist(fig, id) {
        Artist::Bar(b) => b.binning.as_ref(),
        Artist::Stairs(s) => s.binning.as_ref(),
        other => panic!("a histogram is drawn as bars or stairs, found {other:?}"),
    }
}

/// The binning record of a histogram drawn as bars or stairs, with the identifier of its
/// array of edges cleared, so that records of different artists or figures compare by what
/// they say; the edges themselves are compared by [`record_edges`].
#[track_caller]
fn record(fig: &Figure, id: NodeId) -> Option<Binning> {
    stored_record(fig, id).map(|r| Binning {
        edges: NodeId::default(),
        ..r.clone()
    })
}

/// The resolved bin edges that the record of a histogram holds.
#[track_caller]
fn record_edges(fig: &Figure, id: NodeId) -> Vec<f64> {
    let r = stored_record(fig, id).expect("the histogram records its binning");
    vector(fig, r.edges)
}

/// The number of bars of a bar artist.
fn bar_count(fig: &Figure, id: NodeId) -> usize {
    data(fig, bar(fig, id).positions).len()
}

/// Returns the identifiers of the artists of the axes, in order.
fn artist_ids(fig: &Figure, axes_id: NodeId) -> Vec<NodeId> {
    axes(fig, axes_id).artists.iter().map(Artist::id).collect()
}

/// The centre of each bin between the edges.
fn centres(edges: &[f64]) -> Vec<f64> {
    edges.windows(2).map(|e| (e[0] + e[1]) / 2.0).collect()
}

/// The mappings of the x and y axes of a two-dimensional axes of the compiled scene.
fn axis_maps(scene: &ironlab_scene::Scene, axes_id: NodeId) -> (AxisMap, AxisMap) {
    let hit = scene
        .hit_map
        .axes
        .iter()
        .find(|a| a.id == axes_id)
        .expect("the axes is drawn");
    match hit.kind {
        AxesHitKind::TwoD { x, y } => (x, y),
        AxesHitKind::ThreeD => panic!("a 2D axes"),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// What a histogram stores
// ---------------------------------------------------------------------------------------------------------------

// WHY: a reader reads a histogram's bars as its bins: each bar must span exactly its bin, so that the bars touch and
// an edge drawn is an edge of the binning, and rise from 0 to the bin's count. The bar primitive expresses this as
// positions at the bin centres, one width per bar equal to the bin's width (the only form that also draws bins of
// unequal width), no offset and a base of 0. A bare `hist` draws upright bars in the next colour of the colour order;
// it maps nothing through a colour scale, so it creates none, which would otherwise appear as a stray colourbar.
// Ordinary data gives a figure without a single warning.
#[test]
fn hist_stores_one_touching_bar_per_bin_from_zero_to_its_count() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .hist(STEPPED)
        .bins(Bins::Edges(vec![0.0, 1.0, 2.0, 3.0, 4.0]))
        .id();
    let b = bar(&fig, id);
    assert_eq!(b.orientation, Orientation::Vertical);
    assert_eq!(
        b.face,
        ColorSpec::Auto,
        "the next colour of the colour order"
    );
    assert_close(
        &vector(&fig, b.positions),
        &[0.5, 1.5, 2.5, 3.5],
        "bin centres",
    );
    let BarWidth::Data(widths) = b.width else {
        panic!(
            "each bar is as wide as its own bin, given per bar, not {:?}",
            b.width
        );
    };
    assert_close(&vector(&fig, widths), &[1.0; 4], "bin widths");
    assert!(
        b.offset == BarOffset::Widths(0.0) || b.offset == BarOffset::Absolute(0.0),
        "each bar is centred on its bin, not offset by {:?}",
        b.offset
    );
    assert_eq!(b.base, PerBar::Constant(0.0), "the bars rise from 0");
    let (edges, values) = drawn_bins(&fig, id);
    assert_close(&edges, &[0.0, 1.0, 2.0, 3.0, 4.0], "edges");
    assert_eq!(values, [1.0, 2.0, 3.0, 4.0]);
    assert!(
        fig.ir().color_scales.is_empty(),
        "no colour scale is created"
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: bins of unequal width are how a long tail is shown without a row of empty bins; each bar must be as wide as
// its own bin, or the area of the bars would misstate the distribution.
#[test]
fn bins_of_unequal_width_are_drawn_as_bars_of_unequal_width() {
    let edges = vec![0.0, 1.0, 3.0, 4.0, 8.0];
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .hist(STEPPED)
        .bins(Bins::Edges(edges.clone()))
        .id();
    let (drawn, values) = drawn_bins(&fig, id);
    assert_close(&drawn, &edges, "edges");
    assert_eq!(values, [1.0, 5.0, 4.0, 0.0]);
    let widths: Vec<f64> = bar_extents(&fig, id)
        .iter()
        .map(|e| e.high - e.low)
        .collect();
    assert_close(&widths, &[1.0, 2.0, 1.0, 4.0], "widths");
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: the facade must pass every setting to the binning exactly (the sample with its non-finite values, the
// weights, the range, the bins and the normalisation), so that what is drawn is what numpy and the binning functions
// would compute for the same call. The expected values are taken from the binning functions, which `binning.rs`
// checks against numpy, so this test checks the plumbing rather than the arithmetic.
#[test]
fn hist_draws_the_binning_of_the_sample_with_every_setting_applied() {
    let weights: Vec<f64> = (0..SPREAD.len()).map(|i| 0.5 + i as f64 / 4.0).collect();
    let bins = Bins::Rule(BinRule::Sturges);
    for norm in [
        HistNorm::Count,
        HistNorm::Probability,
        HistNorm::Percentage,
        HistNorm::CountDensity,
        HistNorm::Density,
        HistNorm::CumulativeCount,
        HistNorm::Cdf,
    ] {
        let expected = histogram(&SPREAD, Some(&weights), &bins, Some((-1.0, 5.0)), norm).unwrap();
        for style in [HistStyle::Bar, HistStyle::Step, HistStyle::FilledStep] {
            let mut fig = Figure::new();
            let id = fig
                .axes(0, 0)
                .hist(SPREAD)
                .weights(&weights)
                .range(-1.0, 5.0)
                .bins(bins.clone())
                .normalize(norm)
                .style(style)
                .id();
            let (edges, values) = drawn_bins(&fig, id);
            assert_close(
                &edges,
                &expected.edges,
                &format!("{norm:?} {style:?} edges"),
            );
            assert_close(
                &values,
                &expected.values,
                &format!("{norm:?} {style:?} values"),
            );
            let report = fig.validate();
            assert!(report.is_valid(), "{norm:?} {style:?}: {report:?}");
        }
    }
}

// WHY: a density histogram is drawn so that a probability density function can be laid over it; that works only if
// the bars, as drawn, enclose unit area, which must hold for bins of unequal width and when samples outside the range
// are excluded (they are left out of the total, as numpy leaves them out).
#[test]
fn the_bars_of_a_density_histogram_enclose_unit_area() {
    for (bins, range) in [
        (Bins::Edges(vec![-2.0, 0.0, 0.5, 3.0, 7.0]), None),
        (Bins::Count(5), Some((0.0, 4.0))),
        (Bins::Rule(BinRule::FreedmanDiaconis), None),
    ] {
        let mut fig = Figure::new();
        let mut ax = fig.axes(0, 0);
        let mut h = ax.hist(SPREAD);
        h.bins(bins.clone()).normalize(HistNorm::Density);
        if let Some((min, max)) = range {
            h.range(min, max);
        }
        let id = h.id();
        let area: f64 = bar_extents(&fig, id)
            .iter()
            .map(|e| (e.high - e.low) * (e.end - e.base))
            .sum();
        assert!((area - 1.0).abs() < 1e-12, "{bins:?}: the area is {area}");
    }
}

// WHY: a step outline is how several distributions are overlaid on one axes without hiding one another; it is the
// outline of the same bins, closed down to 0 at both ends as matplotlib's `histtype='step'` is, and unfilled. The
// filled step is the same outline filled in the colour of the series, so that it reads as one shape.
#[test]
fn the_step_styles_store_stairs_on_the_bin_edges_closed_down_to_zero() {
    let mut fig = Figure::new();
    let step = fig
        .axes(0, 0)
        .hist(STEPPED)
        .bins(Bins::Count(4))
        .style(HistStyle::Step)
        .id();
    let s = stairs(&fig, step);
    assert_eq!(s.fill, ColorSpec::None, "a step outline is not filled");
    assert_eq!(
        s.line.color,
        ColorSpec::Auto,
        "the next colour of the colour order"
    );
    assert_eq!(s.orientation, Orientation::Vertical);
    let (edges, values) = drawn_bins(&fig, step);
    assert_close(&edges, &[0.5, 1.25, 2.0, 2.75, 3.5], "edges");
    assert_eq!(values, [1.0, 2.0, 3.0, 4.0]);

    let filled = fig
        .axes(0, 0)
        .hist(STEPPED)
        .style(HistStyle::FilledStep)
        .color(RED)
        .id();
    assert_eq!(
        stairs(&fig, filled).line.color,
        ColorSpec::Rgba { color: RED }
    );
    drawn_bins(&fig, filled);
    let auto_filled = fig
        .axes(0, 0)
        .hist(STEPPED)
        .style(HistStyle::FilledStep)
        .id();
    // The fill is checked as drawn, because an automatic fill and an outline of a fixed colour could otherwise
    // resolve to different colours; it may be fainter than the outline, but never another colour.
    for (id, what) in [
        (filled, "a fixed colour"),
        (auto_filled, "the colour order"),
    ] {
        let outline: Vec<[u8; 3]> = stroke_colours(&fig, id)
            .into_iter()
            .map(|[r, g, b, _]| [r, g, b])
            .collect();
        let fills: Vec<[u8; 3]> = filled_paths(&fig, id)
            .into_iter()
            .map(|([r, g, b, _], _)| [r, g, b])
            .collect();
        assert_eq!(outline.len(), 1, "{what}: one outline colour, {outline:?}");
        assert!(!fills.is_empty(), "{what}: the step is filled");
        assert!(
            fills.iter().all(|fill| *fill == outline[0]),
            "{what}: the fill {fills:?} takes the colour of the outline {outline:?}"
        );
    }
    assert_eq!(
        stroke_colours(&fig, filled)
            .first()
            .map(|c| [c[0], c[1], c[2]]),
        Some([255, 0, 0])
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: a horizontal histogram is the marginal distribution drawn beside the y axis of a scatter plot (#118); it lies
// flat with the bins along y, and must differ from the upright histogram in its orientation only, in every style.
#[test]
fn a_horizontal_histogram_differs_from_an_upright_one_only_in_its_orientation() {
    for style in [HistStyle::Bar, HistStyle::Step, HistStyle::FilledStep] {
        let mut fig = Figure::new();
        let upright = fig.axes(0, 0).hist(STEPPED).style(style).id();
        let flat = fig
            .axes(0, 0)
            .hist(STEPPED)
            .style(style)
            .orientation(Orientation::Horizontal)
            .id();
        let orientation = match artist(&fig, flat) {
            Artist::Bar(b) => b.orientation,
            Artist::Stairs(s) => s.orientation,
            other => panic!("{other:?}"),
        };
        assert_eq!(orientation, Orientation::Horizontal, "{style:?}");
        assert_eq!(
            drawn_bins(&fig, upright),
            drawn_bins(&fig, flat),
            "{style:?}"
        );
        assert_eq!(record(&fig, upright), record(&fig, flat), "{style:?}");
        assert!(fig.validate().is_valid(), "{style:?}: {:?}", fig.validate());
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The binning record
// ---------------------------------------------------------------------------------------------------------------

// WHY: the figure keeps the binned values but not the sample (ADR 0029), so the record is the only account of how
// the bins were made: the bins as the author specified them, the range, the normalisation, how many samples were
// counted and how many were left out, and why. A datatip reads it to report the count and density of a bin, so it
// must be stored on whichever artist draws the histogram, in every style, and say exactly what the binning did.
#[test]
fn every_style_records_the_bins_range_normalisation_total_and_exclusions() {
    let sample = [0.5, 1.5, f64::NAN, 2.5, f64::INFINITY, -3.0, 9.0, 3.5];
    for style in [HistStyle::Bar, HistStyle::Step, HistStyle::FilledStep] {
        let mut fig = Figure::new();
        let id = fig
            .axes(0, 0)
            .hist(sample)
            .bins(Bins::Count(4))
            .range(0.0, 4.0)
            .normalize(HistNorm::Density)
            .style(style)
            .id();
        assert_eq!(
            record(&fig, id),
            Some(Binning {
                edges: NodeId::default(),
                bins: Bins::Count(4),
                range: Some([0.0, 4.0]),
                normalization: HistNorm::Density,
                total: 4.0,
                excluded_non_finite: 2,
                excluded_out_of_range: 2,
            }),
            "{style:?}"
        );
        let report = fig.validate();
        assert!(report.is_valid(), "{style:?}: {report:?}");
    }
}

// WHY: the record says how the bins were chosen, not what they turned out to be, which the bars themselves show: a
// rule is recorded as the rule, so that a reader knows the bins were chosen automatically, and a histogram given no
// range records none, rather than the range of the sample that the rule found. A bare `hist` therefore records
// numpy's automatic rule, no range, raw counts and every sample counted.
#[test]
fn a_bare_histogram_records_the_automatic_rule_no_range_and_counts() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).hist(STEPPED).id();
    assert_eq!(
        record(&fig, id),
        Some(Binning {
            edges: NodeId::default(),
            bins: Bins::Rule(BinRule::Auto),
            range: None,
            normalization: HistNorm::Count,
            total: 10.0,
            excluded_non_finite: 0,
            excluded_out_of_range: 0,
        })
    );
}

// WHY: with weights, each sample counts as its weight, so the total that the normalisations divide by, and that a
// datatip multiplies back by to recover a count, is the sum of the included weights rather than the number of
// samples.
#[test]
fn a_weighted_histogram_records_the_sum_of_the_included_weights_as_its_total() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .hist([0.5, 1.5, 2.5, f64::NAN, 9.0])
        .weights([1.0, 2.0, 0.5, 4.0, 8.0])
        .range(0.0, 3.0)
        .bins(Bins::Count(3))
        .id();
    let r = record(&fig, id).expect("a histogram records its binning");
    assert_eq!(r.total, 3.5);
    assert_eq!((r.excluded_non_finite, r.excluded_out_of_range), (1, 1));
}

// WHY: the record must describe the bins drawn, not the first ones tried: after any sequence of setters, and after
// switching the style back and forth, it must be the record the final settings alone would give, or a datatip would
// report counts and densities of a binning the figure no longer shows.
#[test]
fn rebinning_and_restyling_keep_the_record_true_to_the_final_settings() {
    let weights = [1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0];
    let mut tried = Figure::new();
    let id = tried
        .axes(0, 0)
        .hist(STEPPED)
        .bins(Bins::Count(10))
        .normalize(HistNorm::Probability)
        .range(0.0, 2.0)
        .style(HistStyle::Step)
        .bins(Bins::Width(0.5))
        .weights(weights)
        .range(1.0, 4.0)
        .normalize(HistNorm::Cdf)
        .style(HistStyle::Bar)
        .bins(Bins::Rule(BinRule::Sqrt))
        .id();
    assert_eq!(
        record(&tried, id),
        Some(Binning {
            edges: NodeId::default(),
            bins: Bins::Rule(BinRule::Sqrt),
            range: Some([1.0, 4.0]),
            normalization: HistNorm::Cdf,
            // Every weight but the first, whose sample, 0.5, lies below the range.
            total: 14.0,
            excluded_non_finite: 0,
            excluded_out_of_range: 1,
        })
    );
}

// WHY: counts binned elsewhere come with no sample, so the figure cannot know how many samples they hold, how they
// were normalised or what was left out; a record would claim what the figure does not know, so bars drawn from
// counts carry none and a datatip reports their values only.
#[test]
fn counts_binned_beforehand_carry_no_record() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .hist_counts([0.0, 1.0, 3.0, 4.0], [2.0, 5.0, 1.0])
        .id();
    assert_eq!(bar(&fig, id).binning, None);
}

// WHY: the record exists so that a reader pointing at a bin learns its count and density without the sample (ADR
// 0029). For a density histogram of unequal bins the value drawn is neither, so the datatip of each bar (and of each
// step of the step style) must recover the count and the density from the value, the width of its bin and the total
// that the record holds.
#[test]
fn the_datatip_of_each_bin_reports_its_count_and_density_from_the_record() {
    // Bins [0, 1), [1, 3) and [3, 4] hold 1, 5 and 4 of the 10 samples.
    let edges = vec![0.0, 1.0, 3.0, 4.0];
    let counts = [1.0, 5.0, 4.0];
    let densities = [0.1, 0.25, 0.4];
    for style in [HistStyle::Bar, HistStyle::Step] {
        let mut fig = Figure::new();
        let id = fig
            .axes(0, 0)
            .hist(STEPPED)
            .bins(Bins::Edges(edges.clone()))
            .normalize(HistNorm::Density)
            .style(style)
            .id();
        let scene = compile(&fig);
        let mut readings: Vec<(usize, Option<ironlab_scene::hit::BinReading>)> = match style {
            HistStyle::Bar => scene
                .hit_map
                .bars
                .iter()
                .filter(|b| b.artist == id)
                .map(|b| (b.index, b.bin))
                .collect(),
            _ => scene
                .hit_map
                .steps
                .iter()
                .filter(|s| s.artist == id)
                .map(|s| (s.index, s.bin))
                .collect(),
        };
        readings.sort_by_key(|(index, _)| *index);
        assert_eq!(readings.len(), 3, "{style:?}: one datatip per bin");
        for (k, (_, reading)) in readings.iter().enumerate() {
            let reading = reading.unwrap_or_else(|| panic!("{style:?}: bin {k} has a reading"));
            assert!(
                (reading.count - counts[k]).abs() < 1e-9
                    && (reading.density - densities[k]).abs() < 1e-9,
                "{style:?}: bin {k} reads {reading:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Setters that rebin or restyle
// ---------------------------------------------------------------------------------------------------------------

// WHY: an author tries several binnings before settling on one, and each setter that changes the binning rebins in
// place. The figure must then be exactly what the final settings alone would have made: the same bins, values and
// record, and no array left behind from an earlier binning, which would bloat every saved file and show in the
// browser as data of nothing (the `UnusedData` warning).
#[test]
fn rebinning_leaves_the_figure_as_the_final_settings_alone_would_make_it() {
    let weights = [1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0];
    for style in [HistStyle::Bar, HistStyle::Step] {
        let mut tried = Figure::new();
        let id = tried
            .axes(0, 0)
            .hist(STEPPED)
            .style(style)
            .bins(Bins::Count(10))
            .normalize(HistNorm::Probability)
            .range(0.0, 2.0)
            .bins(Bins::Width(0.5))
            .weights(weights)
            .range(0.0, 4.0)
            .normalize(HistNorm::Density)
            .bins(Bins::Count(4))
            .id();
        let mut direct = Figure::new();
        let reference = direct
            .axes(0, 0)
            .hist(STEPPED)
            .style(style)
            .bins(Bins::Count(4))
            .range(0.0, 4.0)
            .weights(weights)
            .normalize(HistNorm::Density)
            .id();
        assert_eq!(
            drawn_bins(&tried, id),
            drawn_bins(&direct, reference),
            "{style:?}"
        );
        assert_eq!(record(&tried, id), record(&direct, reference), "{style:?}");
        assert_eq!(
            tried.ir().data.len(),
            direct.ir().data.len(),
            "{style:?}: no array of an earlier binning is kept"
        );
        let report = tried.validate();
        assert!(
            !has_warning(&report, IssueKind::UnusedData),
            "{style:?}: {report:?}"
        );
        assert!(report.warnings.is_empty(), "{style:?}: {report:?}");
    }
}

// WHY: the setters of a handle are chained in whatever order the author writes them, so a style chosen before the
// bins and one chosen after must give the same figure; otherwise a rebin after a style switch could write to the
// artist of the old style.
#[test]
fn the_order_of_the_setters_does_not_change_the_histogram() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let style_first = ax
        .hist(STEPPED)
        .style(HistStyle::FilledStep)
        .bins(Bins::Count(3))
        .normalize(HistNorm::Cdf)
        .id();
    let style_last = ax
        .hist(STEPPED)
        .bins(Bins::Count(3))
        .normalize(HistNorm::Cdf)
        .style(HistStyle::FilledStep)
        .id();
    assert_eq!(drawn_bins(&fig, style_first), drawn_bins(&fig, style_last));
    assert_eq!(record(&fig, style_first), record(&fig, style_last));
    let (_, cdf) = drawn_bins(&fig, style_first);
    assert_eq!(cdf.last(), Some(&1.0), "a CDF ends at 1");
}

// WHY: the handle, and any legend entry or link that names the histogram, refer to it by its node identifier, and
// the order of the artists of an axes is the order in which they are painted and listed in a legend. Switching the
// style must therefore replace the artist in place: the same identifier, at the same position among its siblings,
// with its label, colour and record, and without leaving the arrays of the old artist behind.
#[test]
fn switching_the_style_keeps_the_identifier_position_label_colour_and_record() {
    let mut fig = Figure::new();
    let axes_id = fig.axes(0, 0).id();
    let mut ax = fig.axes(0, 0);
    let before = ax.hist(STEPPED).label("before").id();
    let mut h = ax.hist(STEPPED);
    h.label("switched")
        .color(RED)
        .normalize(HistNorm::Probability);
    let id = h.id();
    for style in [
        HistStyle::Step,
        HistStyle::FilledStep,
        HistStyle::Bar,
        HistStyle::Step,
    ] {
        h.style(style);
        assert_eq!(h.id(), id, "{style:?}: the handle keeps its identifier");
    }
    let after = ax.hist(STEPPED).label("after").id();
    assert_eq!(
        artist_ids(&fig, axes_id),
        [before, id, after],
        "the switched histogram keeps its place among its siblings"
    );
    let s = stairs(&fig, id);
    assert_eq!(s.label, Some(Text::new("switched")));
    assert_eq!(
        s.line.color,
        ColorSpec::Rgba { color: RED },
        "the colour survives the switches"
    );
    assert_eq!(
        s.fill,
        ColorSpec::None,
        "the last style, a step outline, is not filled"
    );
    assert_eq!(
        s.binning.as_ref().map(|r| r.normalization),
        Some(HistNorm::Probability),
        "the record survives the switches"
    );

    let mut direct = Figure::new();
    direct.axes(0, 0).hist(STEPPED);
    direct.axes(0, 0).hist(STEPPED).style(HistStyle::Step);
    direct.axes(0, 0).hist(STEPPED);
    assert_eq!(
        fig.ir().data.len(),
        direct.ir().data.len(),
        "no array of an earlier style is kept"
    );
    let report = fig.validate();
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: back in the bar style after a switch, the histogram must be the bars it was, with the colour of the series as
// the face of the bars, so that a style can be tried and abandoned without trace.
#[test]
fn a_style_switched_and_switched_back_is_the_histogram_it_was() {
    let mut fig = Figure::new();
    let original = fig.axes(0, 0).hist(STEPPED).color(RED).id();
    let round_trip = fig
        .axes(0, 0)
        .hist(STEPPED)
        .color(RED)
        .style(HistStyle::FilledStep)
        .style(HistStyle::Bar)
        .id();
    assert_eq!(drawn_bins(&fig, original), drawn_bins(&fig, round_trip));
    assert_eq!(
        bar_extents(&fig, original),
        bar_extents(&fig, round_trip),
        "the same widths, offsets and bases"
    );
    assert_eq!(bar(&fig, round_trip).face, ColorSpec::Rgba { color: RED });
    assert!(fig.validate().warnings.is_empty(), "{:?}", fig.validate());
}

// ---------------------------------------------------------------------------------------------------------------
// Several histograms
// ---------------------------------------------------------------------------------------------------------------

// WHY: overlaying the step outlines of two samples on shared bins is the most common way to compare two
// distributions (before and after a change, simulation and experiment); matplotlib's `histtype='step'` exists for
// it. Each outline must be drawn on the same edges, so that a difference in height is a difference in the samples
// rather than in the binning; normalised as densities, each must enclose unit area whatever its sample size; neither
// may be filled, or one would hide the other; and the two must be told apart by colour, the colour order advancing
// from one to the next. Two independent histograms are compared with nothing, so the figure is clean.
#[test]
fn overlaid_step_histograms_of_two_samples_share_their_bins_and_differ_in_colour() {
    let before = STEPPED;
    let after = [1.2, 1.4, 2.1, 2.2, 2.6, 2.9, 3.1, 3.3, 3.8, 4.6, 5.0, 5.5];
    let shared: Vec<f64> = before.iter().chain(&after).copied().collect();
    let edges = bin_edges(&shared, &Bins::Count(6), None).unwrap();
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let ids = [before.as_slice(), after.as_slice()].map(|sample| {
        ax.hist(sample.to_vec())
            .bins(Bins::Edges(edges.clone()))
            .normalize(HistNorm::Density)
            .style(HistStyle::Step)
            .id()
    });
    for id in ids {
        let s = stairs(&fig, id);
        assert_eq!(s.fill, ColorSpec::None, "an overlaid outline is not filled");
        let (drawn, values) = drawn_bins(&fig, id);
        assert_close(&drawn, &edges, "the shared edges");
        let area: f64 = drawn
            .windows(2)
            .zip(&values)
            .map(|(e, v)| (e[1] - e[0]) * v)
            .sum();
        assert!(
            (area - 1.0).abs() < 1e-12,
            "each outline encloses unit area, not {area}"
        );
        assert_eq!(
            filled_shapes(&fig, id),
            0,
            "nothing is filled in the drawn figure"
        );
    }
    let colours = ids.map(|id| stroke_colours(&fig, id));
    assert_eq!(colours[0].len(), 1, "{colours:?}");
    assert_eq!(colours[1].len(), 1, "{colours:?}");
    assert_ne!(
        colours[0], colours[1],
        "the two outlines are told apart by colour"
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: a cumulative histogram over a chosen range answers "how many samples (or what fraction) lie below this value,
// within the range". numpy and matplotlib exclude samples outside the range before accumulating, so the last bin
// holds the number of included samples, not the size of the sample, and a CDF ends at 1 however many samples the
// range or a NaN excluded; accumulating before excluding would end above the included count, and normalising by the
// whole sample would end below 1.
#[test]
fn a_cumulative_histogram_over_a_range_accumulates_only_the_samples_it_includes() {
    let sample: Vec<f64> = STEPPED
        .iter()
        .copied()
        .chain([-5.0, 10.0, f64::NAN])
        .collect();
    for style in [HistStyle::Bar, HistStyle::Step] {
        let mut fig = Figure::new();
        let mut ax = fig.axes(0, 0);
        let mut h = ax.hist(&sample);
        h.bins(Bins::Count(4))
            .range(0.0, 4.0)
            .style(style)
            .normalize(HistNorm::CumulativeCount);
        assert_eq!(h.summary().total, 10.0, "{style:?}");
        let id = h.id();
        let (edges, values) = drawn_bins(&fig, id);
        assert_close(&edges, &[0.0, 1.0, 2.0, 3.0, 4.0], "edges");
        assert_close(
            &values,
            &[1.0, 3.0, 6.0, 10.0],
            &format!("{style:?} cumulative counts"),
        );

        let mut ax = fig.axes(0, 0);
        let cdf = ax
            .hist(&sample)
            .bins(Bins::Count(4))
            .range(0.0, 4.0)
            .style(style)
            .normalize(HistNorm::Cdf)
            .id();
        let (_, values) = drawn_bins(&fig, cdf);
        assert_close(&values, &[0.1, 0.3, 0.6, 1.0], &format!("{style:?} CDF"));
        let report = fig.validate();
        assert!(report.is_valid(), "{style:?}: {report:?}");
        assert!(report.warnings.is_empty(), "{style:?}: {report:?}");
    }
}

// WHY: the marginal distribution of a scatter plot's y values is drawn as a horizontal histogram in the tile beside
// it, sharing the y axis (#118), so that each bin lines up with the points it counts. In the drawn figure each bar
// must therefore span its bin along y, on the same y mapping as the scatter, and extend from 0 along x to its count;
// bars stored with the right orientation flag but drawn along x would not line up with anything.
#[test]
fn a_horizontal_histogram_beside_a_scatter_lines_its_bins_up_with_the_shared_y_axis() {
    let x: Vec<f64> = (0..STEPPED.len()).map(|k| k as f64).collect();
    let mut fig = Figure::new().tiles(1, 2);
    let scatter_axes = fig.axes(0, 0).id();
    let y = fig.data("y", STEPPED).id();
    fig.axes(0, 0).scatter(&x, y);
    let marginal_axes = fig.axes(0, 1).id();
    let id = fig
        .axes(0, 1)
        .hist(y)
        .bins(Bins::Edges(vec![0.0, 1.0, 2.0, 3.0, 4.0]))
        .orientation(Orientation::Horizontal)
        .id();
    fig.link_all_y().unwrap();
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");

    let scene = compile(&fig);
    let (_, scatter_y) = axis_maps(&scene, scatter_axes);
    let (count_x, bin_y) = axis_maps(&scene, marginal_axes);
    assert_eq!(
        (scatter_y.min, scatter_y.max),
        (bin_y.min, bin_y.max),
        "the marginal shares the y limits of the scatter"
    );
    let mut bars: Vec<_> = scene
        .hit_map
        .bars
        .iter()
        .filter(|b| b.artist == id)
        .collect();
    bars.sort_by_key(|b| b.index);
    assert_eq!(bars.len(), 4, "one bar per bin");
    let near = |a: f64, b: f64| (a - b).abs() < 0.5;
    for (k, b) in bars.iter().enumerate() {
        let (low, high) = (bin_y.to_figure(k as f64), bin_y.to_figure(k as f64 + 1.0));
        let r = &b.rect;
        assert!(
            near(r.y, low.min(high)) && near(r.bottom(), low.max(high)),
            "bin {k}: the bar spans its bin along y, {r:?} against {low}..{high}"
        );
        let (zero, count) = (count_x.to_figure(0.0), count_x.to_figure(k as f64 + 1.0));
        assert!(
            near(r.x, zero.min(count)) && near(r.right(), zero.max(count)),
            "bin {k}: the bar extends along x from 0 to its count, {r:?} against {zero}..{count}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Stacked and grouped histograms: `hists`
// ---------------------------------------------------------------------------------------------------------------

// WHY: stacked and side-by-side histograms compare samples bin by bin, which is meaningful only when every sample is
// counted into the same bins. A rule or a count of bins must therefore be applied once, to the pooled sample of all
// the members, as matplotlib does when it is given several samples; binning each sample on its own would give each
// different edges. Explicit edges are used as given. Each member draws the counts of its own sample in those bins.
#[test]
fn hists_bin_every_sample_on_edges_shared_from_the_pooled_sample() {
    let a = [0.0, 0.5, 1.0, 1.5, 2.0];
    let b = [1.0, 2.5, 3.0, 4.0];
    let pooled: Vec<f64> = a.iter().chain(&b).copied().collect();
    for bins in [
        Bins::Count(4),
        Bins::Rule(BinRule::Sturges),
        Bins::Rule(BinRule::Auto),
        Bins::Edges(vec![-1.0, 0.5, 2.0, 5.0]),
    ] {
        let shared = bin_edges(&pooled, &bins, None).unwrap();
        if let Bins::Count(_) = bins {
            assert_ne!(
                bin_edges(&b, &bins, None).unwrap(),
                shared,
                "the second sample alone would be binned differently"
            );
        }
        let mut fig = Figure::new();
        let ids = fig
            .axes(0, 0)
            .hists(bins.clone())
            .stacked("all", "a", a)
            .stacked("all", "b", b)
            .ids();
        assert_eq!(ids.len(), 2);
        for (id, sample) in ids.iter().zip([a.as_slice(), b.as_slice()]) {
            let extents = bar_extents(&fig, *id);
            let mut drawn: Vec<f64> = extents.iter().map(|e| e.low).collect();
            drawn.extend(extents.last().map(|e| e.high));
            assert_close(&drawn, &shared, &format!("{bins:?}: the shared edges"));
            let expected = histogram(
                sample,
                None,
                &Bins::Edges(shared.clone()),
                None,
                HistNorm::Count,
            )
            .unwrap();
            assert_close(
                &vector(&fig, bar(&fig, *id).values),
                &expected.values,
                &format!("{bins:?}: the counts of the member's own sample"),
            );
        }
    }
}

// WHY: the helper places histograms exactly as `bars` places bar series (ADR 0026), so that a stacked or grouped
// histogram reads as the bar chart of its counts would: on bins of equal width, a group of histograms must give the
// same bars, with the same widths, offsets and bases, as `bars` at the bin centres with touching clusters given the
// same counts, whether the members stand side by side, stack, or mix the two.
#[test]
fn hists_on_equal_bins_have_the_geometry_of_bars_given_their_counts() {
    let samples: [&[f64]; 3] = [
        &[0.0, 0.5, 1.0, 1.5, 2.0],
        &[1.0, 2.5, 3.0, 4.0],
        &[0.2, 3.3, 3.6],
    ];
    let counts: [[f64; 4]; 3] = [
        [2.0, 2.0, 1.0, 0.0],
        [0.0, 1.0, 1.0, 2.0],
        [1.0, 0.0, 0.0, 2.0],
    ];
    let edges = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    // Each recipe gives, for each member, `None` for a column of its own or the name of its stacked column.
    let recipes: [[Option<&str>; 3]; 3] = [
        [None, None, None],
        [Some("s"), Some("s"), Some("s")],
        [None, Some("s"), Some("s")],
    ];
    for recipe in recipes {
        let mut fig = Figure::new().tiles(1, 2);
        let mut ax = fig.axes(0, 0);
        let mut hists = ax.hists(Bins::Edges(edges.clone()));
        for (k, column) in recipe.iter().enumerate() {
            match column {
                None => hists.series(format!("m{k}"), samples[k].to_vec()),
                Some(name) => hists.stacked(*name, format!("m{k}"), samples[k].to_vec()),
            };
        }
        let from_samples = hists.ids();

        let mut ax = fig.axes(0, 1);
        let mut bars = ax.bars(centres(&edges));
        bars.gap(0.0);
        for (k, column) in recipe.iter().enumerate() {
            match column {
                None => bars.series(format!("m{k}"), counts[k]),
                Some(name) => bars.stacked(*name, format!("m{k}"), counts[k]),
            };
        }
        let from_counts = bars.ids();

        for (k, (h, b)) in from_samples.iter().zip(&from_counts).enumerate() {
            assert_eq!(
                vector(&fig, bar(&fig, *h).values),
                counts[k],
                "{recipe:?}: member {k} draws its counts"
            );
            assert_extents_close(
                &bar_extents(&fig, *h),
                &bar_extents(&fig, *b),
                &format!("{recipe:?}: member {k}"),
            );
        }
        let report = fig.validate();
        assert!(report.is_valid(), "{recipe:?}: {report:?}");
        assert!(report.warnings.is_empty(), "{recipe:?}: {report:?}");
    }
}

// WHY: on bins of unequal width the bars of a group cannot share one width, which is why the helper writes widths
// per bar. Side by side, the members must split each bin into equal parts in the order they were added, the first at
// the low end, as grouped bars do, so that every bar stays inside its bin; stacked, every member must span its whole
// bin and rest on the counts of the members before it, so that the column's height is the combined count. Checked in
// the drawn figure too, because geometry stored correctly but laid out differently by the scene would still hide one
// sample behind another; and each member's datatip must report the count of its own sample, not the running total.
#[test]
fn hists_on_unequal_bins_split_or_stack_each_bin_in_the_drawn_figure() {
    let edges = vec![0.0, 1.0, 2.0, 3.0, 4.5];
    let first_counts = [1.0, 2.0, 3.0, 4.0];
    let second_counts = [2.0, 1.0, 1.0, 1.0];
    for stacked in [false, true] {
        let mut fig = Figure::new();
        let axes_id = fig.axes(0, 0).id();
        let mut ax = fig.axes(0, 0);
        let mut hists = ax.hists(Bins::Edges(edges.clone()));
        if stacked {
            hists
                .stacked("both", "first", STEPPED)
                .stacked("both", "second", OTHER);
        } else {
            hists.series("first", STEPPED).series("second", OTHER);
        }
        let ids = hists.ids();
        let (first, second) = (ids[0], ids[1]);
        assert_eq!(vector(&fig, bar(&fig, first).values), first_counts);
        assert_eq!(vector(&fig, bar(&fig, second).values), second_counts);

        let (lower, upper) = (bar_extents(&fig, first), bar_extents(&fig, second));
        for k in 0..4 {
            let (left, right) = (edges[k], edges[k + 1]);
            let half = (right - left) / 2.0;
            let expected = if stacked {
                [
                    BarExtent {
                        low: left,
                        high: right,
                        base: 0.0,
                        end: first_counts[k],
                    },
                    BarExtent {
                        low: left,
                        high: right,
                        base: first_counts[k],
                        end: first_counts[k] + second_counts[k],
                    },
                ]
            } else {
                [
                    BarExtent {
                        low: left,
                        high: left + half,
                        base: 0.0,
                        end: first_counts[k],
                    },
                    BarExtent {
                        low: left + half,
                        high: right,
                        base: 0.0,
                        end: second_counts[k],
                    },
                ]
            };
            assert_extents_close(
                &[lower[k], upper[k]],
                &expected,
                &format!("stacked {stacked}, bin {k}"),
            );
        }

        let scene = compile(&fig);
        let (x, _) = axis_maps(&scene, axes_id);
        let hits = |id: NodeId| {
            let mut hits: Vec<_> = scene
                .hit_map
                .bars
                .iter()
                .filter(|b| b.artist == id)
                .collect();
            hits.sort_by_key(|b| b.index);
            assert_eq!(hits.len(), 4, "stacked {stacked}: one bar per bin");
            hits
        };
        let (lower, upper) = (hits(first), hits(second));
        // Half a point of slack covers the rounding of figure-space coordinates.
        let near = |a: f64, b: f64| (a - b).abs() < 0.5;
        for k in 0..4 {
            let (left, right) = (x.to_figure(edges[k]), x.to_figure(edges[k + 1]));
            let (l, u) = (&lower[k].rect, &upper[k].rect);
            if stacked {
                assert_eq!(
                    upper[k].segment,
                    (first_counts[k], first_counts[k] + second_counts[k]),
                    "bin {k}: the second sample sits on the first"
                );
                for r in [l, u] {
                    assert!(
                        near(r.x, left) && near(r.right(), right),
                        "bin {k}: a stacked bar spans its bin, {r:?} against {left}..{right}"
                    );
                }
            } else {
                assert!(
                    near(l.x, left) && near(l.right(), u.x) && near(u.right(), right),
                    "bin {k}: the bars split the bin, first at the low end, {l:?} then {u:?} in {left}..{right}"
                );
            }
            for (hit, count) in [(lower[k], first_counts[k]), (upper[k], second_counts[k])] {
                let reading = hit
                    .bin
                    .expect("a member of a histogram group records its binning");
                assert!(
                    (reading.count - count).abs() < 1e-9,
                    "stacked {stacked}, bin {k}: the datatip reports the member's own count {count}, not {reading:?}"
                );
            }
        }
    }
}

// WHY: a member of a grouped histogram draws only its share of each bin, so the width of its bar is not the width of
// its bin. A datatip that divided by the drawn width would report densities, and for normalised histograms counts,
// wrong by the number of members. The record holds the edges the bins were resolved to, so the datatip of each bar
// must find the bar's bin among them and report the count of the member's own sample and the density over the whole
// bin, whatever the normalisation and however unequal the bins.
#[test]
fn the_datatip_of_a_grouped_histogram_reads_the_whole_bin_whatever_the_normalisation() {
    let edges = vec![0.0, 1.0, 2.0, 3.0, 4.5];
    let bin_widths = [1.0, 1.0, 1.0, 1.5];
    let members = [
        ("first", STEPPED.as_slice(), [1.0, 2.0, 3.0, 4.0], 10.0),
        ("second", OTHER.as_slice(), [2.0, 1.0, 1.0, 1.0], 5.0),
    ];
    for norm in [HistNorm::Count, HistNorm::Density, HistNorm::Probability] {
        let mut fig = Figure::new();
        let mut ax = fig.axes(0, 0);
        let mut hists = ax.hists(Bins::Edges(edges.clone()));
        hists.normalize(norm);
        for (label, sample, ..) in members {
            hists.series(label, sample.to_vec());
        }
        let ids = hists.ids();
        let scene = compile(&fig);
        for (id, (label, _, counts, total)) in ids.iter().zip(members) {
            let mut hits: Vec<_> = scene
                .hit_map
                .bars
                .iter()
                .filter(|b| b.artist == *id)
                .collect();
            hits.sort_by_key(|b| b.index);
            assert_eq!(hits.len(), 4, "{norm:?} {label}: one datatip per bin");
            for (k, hit) in hits.iter().enumerate() {
                let reading = hit
                    .bin
                    .unwrap_or_else(|| panic!("{norm:?} {label}: bin {k} has a reading"));
                let density = counts[k] / (total * bin_widths[k]);
                assert!(
                    (reading.count - counts[k]).abs() < 1e-9
                        && (reading.density - density).abs() < 1e-9,
                    "{norm:?} {label}, bin {k}: read {reading:?}, not a count of {} and a density of {density}",
                    counts[k]
                );
            }
        }
    }
}

// WHY: the record holds the edges the specification resolved to, so that a reader finds the bin of anything drawn
// without the sample. They must be the edges the histogram was binned on: for bars, the edges the bars span; for
// stairs, the stairs' own edges, shared rather than stored twice; and for every member of `hists`, the shared edges
// found from the pooled sample, not edges of its own sample.
#[test]
fn every_record_holds_the_edges_its_bins_were_resolved_to() {
    let mut fig = Figure::new();
    let bars = fig.axes(0, 0).hist(STEPPED).bins(Bins::Count(4)).id();
    assert_close(
        &record_edges(&fig, bars),
        &[0.5, 1.25, 2.0, 2.75, 3.5],
        "bars",
    );
    assert_close(
        &record_edges(&fig, bars),
        &drawn_bins(&fig, bars).0,
        "the edges the bars span",
    );

    let steps = fig
        .axes(0, 0)
        .hist(STEPPED)
        .bins(Bins::Count(4))
        .style(HistStyle::Step)
        .id();
    let StairsData::Edges { edges, .. } = stairs(&fig, steps).data else {
        panic!("histogram stairs lie between edges");
    };
    assert_eq!(
        stored_record(&fig, steps).map(|r| r.edges),
        Some(edges),
        "the stairs' own edges"
    );

    let (a, b) = ([0.0, 0.5, 1.0, 1.5, 2.0], [1.0, 2.5, 3.0, 4.0, 9.0]);
    let pooled: Vec<f64> = a.iter().chain(&b).copied().collect();
    let shared = bin_edges(&pooled, &Bins::Rule(BinRule::Sturges), None).unwrap();
    let ids = fig
        .axes(0, 0)
        .hists(Bins::Rule(BinRule::Sturges))
        .series("a", a)
        .stacked("s", "b", b)
        .ids();
    for id in ids {
        assert_close(&record_edges(&fig, id), &shared, "the shared edges");
    }
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: stacked and grouped histograms are drawn flat beside the y axis of a scatter plot as marginal distributions
// (#118), as `bars` draws grouped bars flat; `horizontal` must lay every member flat, whether called before or after
// the members are added, and change nothing else about where the bars lie.
#[test]
fn hists_horizontal_lays_every_member_flat_and_changes_nothing_else() {
    let edges = vec![0.0, 1.0, 2.0, 3.0, 4.5];
    let mut fig = Figure::new();
    let upright = fig
        .axes(0, 0)
        .hists(Bins::Edges(edges.clone()))
        .series("first", STEPPED)
        .stacked("s", "second", OTHER)
        .stacked("s", "third", STEPPED)
        .ids();
    for horizontal_first in [true, false] {
        let mut ax = fig.axes(0, 0);
        let mut hists = ax.hists(Bins::Edges(edges.clone()));
        if horizontal_first {
            hists.horizontal();
        }
        hists
            .series("first", STEPPED)
            .stacked("s", "second", OTHER)
            .stacked("s", "third", STEPPED);
        if !horizontal_first {
            hists.horizontal();
        }
        for (flat, upright) in hists.ids().into_iter().zip(&upright) {
            assert_eq!(bar(&fig, flat).orientation, Orientation::Horizontal);
            assert_eq!(
                bar_extents(&fig, flat),
                bar_extents(&fig, *upright),
                "horizontal first {horizontal_first}"
            );
        }
    }
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: the members share their bins but not their samples, so each must keep its own record (ADR 0029): its own
// total and its own exclusions, which differ when one sample has NaNs or values outside the bins and the other does
// not, while the bins and normalisation recorded are those given to the helper. A record pooled across the members
// would make every datatip report wrong densities.
#[test]
fn each_member_of_hists_records_its_own_total_and_exclusions() {
    let edges = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let clean = [0.0, 0.5, 1.0, 1.5, 2.0];
    let dirty = [1.0, 2.5, 3.0, 4.0, 9.0, f64::NAN];
    for rule in [Bins::Edges(edges.clone()), Bins::Rule(BinRule::Sturges)] {
        let mut fig = Figure::new();
        let ids = fig
            .axes(0, 0)
            .hists(rule.clone())
            .series("clean", clean)
            .series("dirty", dirty)
            .ids();
        let excluded_out_of_range = u64::from(matches!(rule, Bins::Edges(_)));
        assert_eq!(
            record(&fig, ids[0]),
            Some(Binning {
                edges: NodeId::default(),
                bins: rule.clone(),
                range: None,
                normalization: HistNorm::Count,
                total: 5.0,
                excluded_non_finite: 0,
                excluded_out_of_range: 0,
            }),
            "{rule:?}"
        );
        assert_eq!(
            record(&fig, ids[1]),
            Some(Binning {
                edges: NodeId::default(),
                bins: rule.clone(),
                range: None,
                normalization: HistNorm::Count,
                total: 5.0 - excluded_out_of_range as f64,
                excluded_non_finite: 1,
                excluded_out_of_range,
            }),
            "{rule:?}: 9.0 lies outside the explicit edges, but inside edges found from the pooled sample"
        );
    }
}

// WHY: each member is a histogram of its own sample and must be told apart from the others, so each carries its own
// label for the legend and takes the next colour of the colour order, as the series of `bars` do; and a group made of
// ordinary samples leaves a figure without a warning.
#[test]
fn the_members_of_hists_are_labelled_coloured_apart_and_clean() {
    let mut fig = Figure::new();
    let ids = fig
        .axes(0, 0)
        .hists(Bins::Count(4))
        .series("before", STEPPED)
        .series("after", OTHER)
        .ids();
    for (id, label) in ids.iter().zip(["before", "after"]) {
        let b = bar(&fig, *id);
        assert_eq!(b.label, Some(Text::new(label)));
        assert_eq!(
            b.face,
            ColorSpec::Auto,
            "the next colour of the colour order"
        );
    }
    let fills: Vec<Option<[u8; 4]>> = ids
        .iter()
        .map(|id| filled_paths(&fig, *id).first().map(|(colour, _)| *colour))
        .collect();
    assert!(fills[0].is_some() && fills[1].is_some(), "{fills:?}");
    assert_ne!(fills[0], fills[1], "the members are told apart by colour");
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: a bin specification that cannot be applied must not panic in the middle of building a figure, for a group of
// histograms any more than for one: every member is stored with no bars and no record, so that the figure stays
// valid and warns that there is nothing to draw (ADR 0012), and the helper returns the reason.
#[test]
fn hists_of_a_bad_specification_draw_nothing_and_the_helper_reports_why() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let mut hists = ax.hists(Bins::Count(0));
    hists.series("a", STEPPED).stacked("s", "b", OTHER);
    assert_eq!(hists.error(), Some(&BinningError::ZeroBins));
    let ids = hists.ids();
    for id in ids {
        assert_eq!(bar_count(&fig, id), 0, "no bars");
        assert_eq!(
            bar(&fig, id).binning,
            None,
            "no record of bins that were not made"
        );
    }
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(has_warning(&report, IssueKind::NothingToDraw), "{report:?}");
}

// WHY: the helper's normalisation and range must apply to every member, whether they are set before the members are
// added or after. Members side by side are compared by shape, so each is normalised by its own total and its own bins
// enclose unit area; members of one stack are normalised by the pooled total of the stack, so that the stack encloses
// unit area and each layer is its share of it. Either way each record holds the total its values were normalised by,
// which a datatip needs, and its own exclusions. The range bounds the shared edges, whatever the pooled sample spans,
// and excludes from each member the values outside it. A stacked member rests on the values of the member below it.
#[test]
fn hists_normalize_and_range_apply_to_every_member_whenever_they_are_set() {
    let first = [0.5, 1.5, 1.5, 2.5, -2.0, f64::NAN];
    let second = [0.2, 3.9, 3.1, 9.0];
    let edges = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    for settings_first in [true, false] {
        for stacked in [false, true] {
            let what = format!("settings first {settings_first}, stacked {stacked}");
            let mut fig = Figure::new();
            let mut ax = fig.axes(0, 0);
            let mut hists = ax.hists(Bins::Count(4));
            if settings_first {
                hists.range(0.0, 4.0).normalize(HistNorm::Density);
            }
            if stacked {
                hists
                    .stacked("both", "first", first)
                    .stacked("both", "second", second);
            } else {
                hists.series("first", first).series("second", second);
            }
            if !settings_first {
                hists.normalize(HistNorm::Density).range(0.0, 4.0);
            }
            assert_eq!(hists.error(), None, "{what}");
            let ids = hists.ids();
            let members = [
                (ids[0], first.as_slice(), 4.0, 1, 1),
                (ids[1], second.as_slice(), 3.0, 0, 1),
            ];
            let mut areas = Vec::new();
            for (id, sample, own_total, non_finite, out_of_range) in members {
                let total = if stacked { 7.0 } else { own_total };
                let counts = histogram(
                    sample,
                    None,
                    &Bins::Edges(edges.clone()),
                    Some((0.0, 4.0)),
                    HistNorm::Count,
                )
                .unwrap();
                let expected: Vec<f64> = counts.values.iter().map(|c| c / total).collect();
                let values = vector(&fig, bar(&fig, id).values);
                assert_close(&values, &expected, &format!("{what}: densities"));
                areas.push(values.iter().sum::<f64>());
                assert_eq!(
                    record(&fig, id),
                    Some(Binning {
                        edges: NodeId::default(),
                        bins: Bins::Count(4),
                        range: Some([0.0, 4.0]),
                        normalization: HistNorm::Density,
                        total,
                        excluded_non_finite: non_finite,
                        excluded_out_of_range: out_of_range,
                    }),
                    "{what}: the total normalised by, and the member's own exclusions"
                );
                let extents = bar_extents(&fig, id);
                for (k, e) in extents.iter().enumerate() {
                    assert!(
                        e.low >= edges[k] - TOL && e.high <= edges[k + 1] + TOL,
                        "{what}: bar {k} lies inside its bin, {e:?}"
                    );
                }
            }
            if stacked {
                let area: f64 = areas.iter().sum();
                assert!(
                    (area - 1.0).abs() < 1e-12,
                    "{what}: the stack encloses unit area, not {area}"
                );
            } else {
                for area in areas {
                    assert!(
                        (area - 1.0).abs() < 1e-12,
                        "{what}: each member encloses unit area, not {area}"
                    );
                }
            }
            let lower = vector(&fig, bar(&fig, ids[0]).values);
            let bases: Vec<f64> = bar_extents(&fig, ids[1]).iter().map(|e| e.base).collect();
            let expected_bases = if stacked { lower } else { vec![0.0; 4] };
            assert_close(&bases, &expected_bases, &format!("{what}: bases"));
            let report = fig.validate();
            assert!(report.is_valid(), "{what}: {report:?}");
            assert!(report.warnings.is_empty(), "{what}: {report:?}");
        }
    }
}

/// The value of each bin at the top of the stack of the given members: the sum of their values.
fn stack_top(fig: &Figure, ids: &[NodeId]) -> Vec<f64> {
    let mut top = vec![0.0; bar_count(fig, ids[0])];
    for id in ids {
        for (t, v) in top.iter_mut().zip(vector(fig, bar(fig, *id).values)) {
            *t += v;
        }
    }
    top
}

// WHY: a stacked histogram shows how the pooled sample divides among its members, so the top of the stack must be the
// histogram of the pooled sample under every normalisation, as Matplotlib's `hist(stacked=True, density=True)` and
// seaborn's `histplot(multiple="stack")` draw it: a density stack encloses unit area, a probability stack sums to 1,
// a percentage stack to 100, and a cumulative stack ends at the pooled total, or at 1 for a CDF. Each layer is the
// member's share of that: its own counts normalised by the pooled total. Counts and count densities need no total and
// are unchanged. Bins of unequal width check that a density is divided by the width of its own bin.
#[test]
fn the_top_of_a_stack_is_the_normalised_histogram_of_the_pooled_sample() {
    let edges = vec![0.0, 1.0, 2.0, 3.0, 4.5];
    let widths = [1.0, 1.0, 1.0, 1.5];
    let pooled: Vec<f64> = STEPPED.iter().chain(&OTHER).copied().collect();
    let pooled_total = 15.0;
    for norm in [
        HistNorm::Count,
        HistNorm::CountDensity,
        HistNorm::Density,
        HistNorm::Probability,
        HistNorm::Percentage,
        HistNorm::CumulativeCount,
        HistNorm::Cdf,
    ] {
        let mut fig = Figure::new();
        let ids = fig
            .axes(0, 0)
            .hists(Bins::Edges(edges.clone()))
            .normalize(norm)
            .stacked("both", "first", STEPPED)
            .stacked("both", "second", OTHER)
            .ids();
        let top = stack_top(&fig, &ids);
        let expected = histogram(&pooled, None, &Bins::Edges(edges.clone()), None, norm).unwrap();
        assert_close(
            &top,
            &expected.values,
            &format!("{norm:?}: the pooled histogram"),
        );
        let what = format!("{norm:?}: the top of the stack");
        match norm {
            HistNorm::Density => {
                let area: f64 = top.iter().zip(widths).map(|(v, w)| v * w).sum();
                assert!((area - 1.0).abs() < 1e-12, "{what} encloses {area}");
            }
            HistNorm::Probability => {
                assert!((top.iter().sum::<f64>() - 1.0).abs() < 1e-12, "{what}")
            }
            HistNorm::Percentage => {
                assert!((top.iter().sum::<f64>() - 100.0).abs() < 1e-10, "{what}")
            }
            HistNorm::CumulativeCount => assert_eq!(top.last(), Some(&pooled_total), "{what}"),
            HistNorm::Cdf => assert!((top[3] - 1.0).abs() < 1e-12, "{what} ends at {}", top[3]),
            _ => {}
        }
        for (id, sample) in ids.iter().zip([STEPPED.as_slice(), OTHER.as_slice()]) {
            let counts = histogram(
                sample,
                None,
                &Bins::Edges(edges.clone()),
                None,
                HistNorm::Count,
            )
            .unwrap()
            .values;
            let cumulative: Vec<f64> = counts
                .iter()
                .scan(0.0, |sum, c| {
                    *sum += c;
                    Some(*sum)
                })
                .collect();
            let share: Vec<f64> = match norm {
                HistNorm::Count => counts,
                HistNorm::CountDensity => counts.iter().zip(widths).map(|(c, w)| c / w).collect(),
                HistNorm::Density => counts
                    .iter()
                    .zip(widths)
                    .map(|(c, w)| c / (pooled_total * w))
                    .collect(),
                HistNorm::Probability => counts.iter().map(|c| c / pooled_total).collect(),
                HistNorm::Percentage => counts.iter().map(|c| 100.0 * c / pooled_total).collect(),
                HistNorm::CumulativeCount => cumulative,
                HistNorm::Cdf => cumulative.iter().map(|c| c / pooled_total).collect(),
            };
            assert_close(
                &vector(&fig, bar(&fig, *id).values),
                &share,
                &format!("{norm:?}: the layer is the member's share of the pooled histogram"),
            );
        }
    }
}

// WHY: the default follows the layout, because stacked members are read as parts of a whole and side-by-side members
// are compared by shape, but some comparisons want the other: members side by side whose heights show their shares of
// the pooled sample (seaborn's `common_norm=True` with `multiple="dodge"`), or a stack of shapes normalised each on
// its own (seaborn's `common_norm=False` with `multiple="stack"`). `common_norm(true)` must normalise every member by
// the pooled total of all the members, and `common_norm(false)` each by its own, in either layout; and because the
// helper may be configured before or after its members are added, the result must not depend on when it is called.
// The record of each member holds the total its values were normalised by.
#[test]
fn common_norm_overrides_the_layout_whenever_it_is_set() {
    let edges = vec![0.0, 1.0, 2.0, 3.0, 4.5];
    let samples = [STEPPED.as_slice(), OTHER.as_slice()];
    let own_totals = [10.0, 5.0];
    for stacked in [false, true] {
        for common in [None, Some(true), Some(false)] {
            for setter_first in [true, false] {
                let what =
                    format!("stacked {stacked}, common {common:?}, setter first {setter_first}");
                let mut fig = Figure::new();
                let mut ax = fig.axes(0, 0);
                let mut hists = ax.hists(Bins::Edges(edges.clone()));
                hists.normalize(HistNorm::Probability);
                if let (Some(common), true) = (common, setter_first) {
                    hists.common_norm(common);
                }
                for (k, sample) in samples.iter().enumerate() {
                    if stacked {
                        hists.stacked("both", format!("m{k}"), sample.to_vec());
                    } else {
                        hists.series(format!("m{k}"), sample.to_vec());
                    }
                }
                if let (Some(common), false) = (common, setter_first) {
                    hists.common_norm(common);
                }
                let pooled = common.unwrap_or(stacked);
                for ((id, sample), own_total) in hists.ids().iter().zip(samples).zip(own_totals) {
                    let total = if pooled { 15.0 } else { own_total };
                    let counts = histogram(
                        sample,
                        None,
                        &Bins::Edges(edges.clone()),
                        None,
                        HistNorm::Count,
                    )
                    .unwrap()
                    .values;
                    let expected: Vec<f64> = counts.iter().map(|c| c / total).collect();
                    assert_close(&vector(&fig, bar(&fig, *id).values), &expected, &what);
                    assert_eq!(
                        record(&fig, *id).map(|r| r.total),
                        Some(total),
                        "{what}: the record holds the total normalised by"
                    );
                }
            }
        }
    }
}

// WHY: each stacked column is a whole of its own, so the members of one stack are normalised by the pooled total of
// that stack alone, not of every member of the helper, and a member in a column of its own beside the stack keeps its
// own total. Each record holds the total its values were normalised by, so that a datatip recovers the member's own
// count, while the exclusions stay the member's own, because they describe what its own sample lost.
#[test]
fn the_members_of_a_stack_record_the_pooled_total_of_their_stack_and_their_own_exclusions() {
    let edges = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let alone = [0.5, 1.5, 2.5];
    let lower = [0.5, 1.5, 1.5, f64::NAN];
    let upper = [2.5, 3.5, 9.0];
    let mut fig = Figure::new();
    let ids = fig
        .axes(0, 0)
        .hists(Bins::Edges(edges.clone()))
        .normalize(HistNorm::Density)
        .series("alone", alone)
        .stacked("s", "lower", lower)
        .stacked("s", "upper", upper)
        .ids();
    let expected = [(3.0, 0, 0), (5.0, 1, 0), (5.0, 0, 1)];
    for (id, (total, non_finite, out_of_range)) in ids.iter().zip(expected) {
        assert_eq!(
            record(&fig, *id),
            Some(Binning {
                edges: NodeId::default(),
                bins: Bins::Edges(edges.clone()),
                range: None,
                normalization: HistNorm::Density,
                total,
                excluded_non_finite: non_finite,
                excluded_out_of_range: out_of_range,
            })
        );
    }
    let area: f64 = stack_top(&fig, &ids[1..]).iter().sum();
    assert!(
        (area - 1.0).abs() < 1e-12,
        "the stack encloses unit area, not {area}"
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: a layer of a stacked density histogram draws the member's share of the pooled density, but a reader pointing
// at it wants to know how many of the member's own samples lie in the bin. Because the record holds the pooled total,
// the datatip must recover the member's own count, and report as its density the member's contribution to the pooled
// density, the count divided by the pooled total and the width of the bin, so that the densities of the layers of a
// bin add up to the density of the stack there.
#[test]
fn the_datatip_of_a_stacked_member_reports_its_own_count_and_its_share_of_the_density() {
    let edges = vec![0.0, 1.0, 2.0, 3.0, 4.5];
    let widths = [1.0, 1.0, 1.0, 1.5];
    let members = [
        (STEPPED.as_slice(), [1.0, 2.0, 3.0, 4.0]),
        (OTHER.as_slice(), [2.0, 1.0, 1.0, 1.0]),
    ];
    for norm in [HistNorm::Density, HistNorm::Probability, HistNorm::Cdf] {
        let mut fig = Figure::new();
        let mut ax = fig.axes(0, 0);
        let mut hists = ax.hists(Bins::Edges(edges.clone()));
        hists.normalize(norm);
        for (k, (sample, _)) in members.iter().enumerate() {
            hists.stacked("both", format!("m{k}"), sample.to_vec());
        }
        let ids = hists.ids();
        let scene = compile(&fig);
        for (id, (_, counts)) in ids.iter().zip(members) {
            let mut hits: Vec<_> = scene
                .hit_map
                .bars
                .iter()
                .filter(|b| b.artist == *id)
                .collect();
            hits.sort_by_key(|b| b.index);
            assert_eq!(hits.len(), 4, "{norm:?}: one datatip per bin");
            for (k, hit) in hits.iter().enumerate() {
                let reading = hit.bin.expect("a stacked member records its binning");
                let density = counts[k] / (15.0 * widths[k]);
                assert!(
                    (reading.count - counts[k]).abs() < 1e-9
                        && (reading.density - density).abs() < 1e-9,
                    "{norm:?}, bin {k}: read {reading:?}, not a count of {} and a density of {density}",
                    counts[k]
                );
            }
        }
    }
}

// WHY: a range that cannot bound bins is a mistake in the helper's specification, shared by every member, so it
// behaves as a bad bin specification does: no panic, every member stored with no bars and no record, and the reason
// on the helper until a later range fixes it, after which every member is binned and recorded again.
#[test]
fn a_bad_range_of_hists_draws_nothing_until_a_good_one_replaces_it() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let mut hists = ax.hists(Bins::Count(4));
    hists
        .series("a", STEPPED)
        .series("b", OTHER)
        .range(4.0, 0.0);
    assert_eq!(hists.error(), Some(&BinningError::InvalidRange(4.0, 0.0)));
    let ids = hists.ids();
    for id in &ids {
        assert_eq!(bar_count(&fig, *id), 0, "no bars");
        assert_eq!(bar(&fig, *id).binning, None, "no record");
    }
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(has_warning(&report, IssueKind::NothingToDraw), "{report:?}");

    let mut ax = fig.axes(0, 0);
    let mut hists = ax.hists(Bins::Count(4));
    hists
        .range(4.0, 0.0)
        .series("a", STEPPED)
        .series("b", OTHER)
        .range(0.0, 4.0);
    assert_eq!(hists.error(), None, "a good range clears the reason");
    for id in hists.ids() {
        assert_eq!(bar_count(&fig, id), 4);
        assert_eq!(
            bar(&fig, id).binning.as_ref().map(|r| r.range),
            Some(Some([0.0, 4.0]))
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The sample
// ---------------------------------------------------------------------------------------------------------------

// WHY: the figure stores the binned values, not the sample (ADR 0029). A sample the author stored with `fig.data`
// is read by `hist` but not referenced, and it stays in the figure exactly as the author made it, because the author
// holds its identifier and may plot it again; the builder never deletes such an array. While nothing refers to it,
// the existing rule reports it with the `UnusedData` warning, which says truthfully that it costs space in every
// saved file and never prevents an export; the histogram adds no warning of its own.
#[test]
fn a_named_sample_is_read_but_not_referenced_and_is_kept_as_the_author_made_it() {
    let mut fig = Figure::new();
    let strain = fig.data("strain", STEPPED).id();
    let id = fig
        .axes(0, 0)
        .hist(strain)
        .bins(Bins::Count(2))
        .bins(Bins::Edges(vec![0.0, 1.0, 2.0, 3.0, 4.0]))
        .id();
    let (_, values) = drawn_bins(&fig, id);
    assert_eq!(values, [1.0, 2.0, 3.0, 4.0], "the named sample is binned");
    let stored = fig
        .ir()
        .data_array(strain)
        .expect("the named sample is kept");
    assert_eq!(stored.name.as_deref(), Some("strain"));
    assert_eq!(stored.array, NdArray::vector(STEPPED.to_vec()), "unchanged");
    let b = bar(&fig, id);
    let mut used = vec![b.positions, b.values];
    if let BarWidth::Data(widths) = b.width {
        used.push(widths);
    }
    if let PerBar::Data(bases) = b.base {
        used.push(bases);
    }
    assert!(
        !used.contains(&strain),
        "the histogram does not refer to the sample"
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert_eq!(
        report
            .warnings
            .iter()
            .map(|w| (w.kind, w.node))
            .collect::<Vec<_>>(),
        [(IssueKind::UnusedData, Some(strain))],
        "the unreferenced sample is reported once, by the existing rule"
    );
}

// WHY: a marginal histogram beside a scatter plot (#118) bins the same named array that the scatter plots; the
// scatter refers to it, so the figure is clean, and rebinning the histogram never deletes the array the scatter
// draws.
#[test]
fn a_named_sample_that_another_artist_plots_gives_no_warning() {
    let mut fig = Figure::new();
    let x = fig.data("x", STEPPED).id();
    let y: Vec<f64> = STEPPED.iter().map(|v| v * 2.0).collect();
    fig.axes(0, 0).scatter(x, &y);
    fig.axes(0, 0)
        .hist(x)
        .bins(Bins::Count(3))
        .bins(Bins::Count(4));
    assert!(
        fig.ir().data_array(x).is_some(),
        "the scattered array is kept"
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: the handle is where the author, while building, learns how many samples the histogram counts and how many it
// left out, and why: a NaN from a failed measurement and a value outside the chosen range are different problems.
// The summary must follow the current settings, since a new range changes what is excluded, and agree with the
// record stored in the figure.
#[test]
fn the_summary_reports_what_the_histogram_includes_and_excludes() {
    let sample = [0.5, 1.5, f64::NAN, 2.5, f64::INFINITY, -3.0, 9.0, 3.5];
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let mut h = ax.hist(sample);
    h.bins(Bins::Count(4)).range(0.0, 4.0);
    assert_eq!(
        h.summary(),
        HistogramSummary {
            total: 4.0,
            excluded_non_finite: 2,
            excluded_out_of_range: 2,
        }
    );
    h.range(-5.0, 10.0);
    assert_eq!(
        h.summary(),
        HistogramSummary {
            total: 6.0,
            excluded_non_finite: 2,
            excluded_out_of_range: 0,
        },
        "a wider range includes the values the narrower one excluded"
    );
    h.weights([1.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.5, 3.0]);
    // The included values are those at 0, 1, 3, 5, 6 and 7.
    assert_eq!(
        h.summary().total,
        7.0,
        "with weights the total is the sum of the included weights"
    );
    assert!(h.error().is_none());
    let summary = h.summary();
    let id = h.id();
    let r = record(&fig, id).expect("a histogram records its binning");
    assert_eq!(
        (r.total, r.excluded_non_finite, r.excluded_out_of_range),
        (
            summary.total,
            summary.excluded_non_finite as u64,
            summary.excluded_out_of_range as u64
        ),
        "the record agrees with the summary"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Bad specifications and empty samples
// ---------------------------------------------------------------------------------------------------------------

// WHY: a bin specification that cannot be applied (no bins, a width that is not positive, a reversed range, or
// weights that do not match the sample) is a mistake the author must hear of, but it must not panic in the middle
// of building a figure, nor leave a figure that cannot be exported. The histogram is stored with no bins and no
// record (a record of bins that could not be made would itself make the figure invalid), so that the figure stays
// valid and validation warns that it has nothing to draw (ADR 0012), and the handle returns the reason; a later
// setter that fixes the specification draws the bins, records them and clears the reason.
#[test]
fn a_bad_specification_draws_nothing_and_the_handle_reports_why() {
    type Spoil = fn(&mut HistMut<'_>);
    let cases: [(Spoil, BinningError); 4] = [
        (
            |h| {
                h.bins(Bins::Count(0));
            },
            BinningError::ZeroBins,
        ),
        (
            |h| {
                h.bins(Bins::Width(-1.0));
            },
            BinningError::InvalidWidth(-1.0),
        ),
        (
            |h| {
                h.range(4.0, 0.0);
            },
            BinningError::InvalidRange(4.0, 0.0),
        ),
        (
            |h| {
                h.weights([1.0, 2.0]);
            },
            BinningError::LengthMismatch {
                expected: 10,
                actual: 2,
            },
        ),
    ];
    for (spoil, reason) in cases {
        for style in [HistStyle::Bar, HistStyle::Step] {
            let mut fig = Figure::new();
            let mut ax = fig.axes(0, 0);
            let mut h = ax.hist(STEPPED);
            h.style(style);
            spoil(&mut h);
            let id = h.id();
            assert_eq!(h.error(), Some(&reason), "{style:?}");
            let (edges, values) = drawn_bins(&fig, id);
            // Stairs of no bins keep the one edge that the model requires of stairs on edges
            // (one more edge than values), which draws nothing.
            assert!(
                values.is_empty() && edges.len() <= 1,
                "{reason:?} {style:?}: no bins are drawn"
            );
            assert_eq!(record(&fig, id), None, "{reason:?} {style:?}: no record");
            let report = fig.validate();
            assert!(report.is_valid(), "{reason:?} {style:?}: {report:?}");
            assert!(
                has_warning(&report, IssueKind::NothingToDraw),
                "{reason:?} {style:?}: {report:?}"
            );
            assert!(
                !has_warning(&report, IssueKind::UnusedData),
                "{reason:?} {style:?}: {report:?}"
            );
        }
    }

    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let mut h = ax.hist(STEPPED);
    h.bins(Bins::Count(0)).bins(Bins::Count(4));
    assert_eq!(h.error(), None, "a fixed specification clears the reason");
    let id = h.id();
    assert_eq!(bar_count(&fig, id), 4);
    assert_eq!(
        record(&fig, id).map(|r| r.bins.clone()),
        Some(Bins::Count(4))
    );
    assert!(fig.validate().warnings.is_empty(), "{:?}", fig.validate());
}

// WHY: an empty sample, or one whose every value is NaN, is a normal state of a figure built before its data arrives
// (ADR 0012): it is no error, it draws nothing, and validation warns that there is nothing to draw. The binning
// succeeded, so the record is kept, and says that nothing was counted and why.
#[test]
fn an_empty_sample_draws_nothing_without_an_error() {
    for sample in [Vec::new(), vec![f64::NAN, f64::NAN]] {
        for style in [HistStyle::Bar, HistStyle::FilledStep] {
            let mut fig = Figure::new();
            let mut ax = fig.axes(0, 0);
            let mut h = ax.hist(&sample);
            h.style(style);
            assert_eq!(h.error(), None, "{sample:?} {style:?}");
            let id = h.id();
            assert!(drawn_bins(&fig, id).1.is_empty(), "{sample:?} {style:?}");
            let r = record(&fig, id).expect("an empty sample is binned and recorded");
            assert_eq!(
                (r.total, r.excluded_non_finite),
                (0.0, sample.len() as u64),
                "{sample:?} {style:?}"
            );
            let report = fig.validate();
            assert!(report.is_valid(), "{sample:?} {style:?}: {report:?}");
            assert!(has_warning(&report, IssueKind::NothingToDraw), "{report:?}");
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Counts binned beforehand
// ---------------------------------------------------------------------------------------------------------------

// WHY: counts binned elsewhere (by MATLAB's `histcounts`, a simulation or an instrument) must draw as the same
// histogram that `hist` would draw of the sample: touching bars spanning the given edges, from 0, which are ordinary
// bars whose handle sets their label and colours.
#[test]
fn hist_counts_draws_counts_binned_beforehand_as_touching_bars() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .hist_counts([0.0, 1.0, 3.0, 4.0], [2.0, 5.0, 1.0])
        .label("binned")
        .id();
    let (edges, values) = drawn_bins(&fig, id);
    assert_close(&edges, &[0.0, 1.0, 3.0, 4.0], "edges");
    assert_eq!(values, [2.0, 5.0, 1.0]);
    assert_eq!(bar(&fig, id).label, Some(Text::new("binned")));
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");

    let mut same = Figure::new();
    let from_sample = same
        .axes(0, 0)
        .hist(STEPPED)
        .bins(Bins::Edges(vec![0.0, 1.0, 2.0, 3.0, 4.0]))
        .id();
    let from_counts = same
        .axes(0, 0)
        .hist_counts([0.0, 1.0, 2.0, 3.0, 4.0], [1.0, 2.0, 3.0, 4.0])
        .id();
    assert_eq!(
        drawn_bins(&same, from_sample),
        drawn_bins(&same, from_counts)
    );
}

// WHY: edges that are not one more than the counts are a mistake in the author's data; like bars whose positions and
// values differ in length, they are reported by validation against the bars rather than panicking in the facade.
#[test]
fn hist_counts_with_mismatched_edges_is_reported_by_validate() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .hist_counts([0.0, 1.0, 2.0, 3.0], [1.0, 2.0])
        .id();
    assert!(
        has_error_at(&fig.validate(), IssueKind::ShapeMismatch, id),
        "{:?}",
        fig.validate()
    );
}

// ---------------------------------------------------------------------------------------------------------------
// The compiled figure
// ---------------------------------------------------------------------------------------------------------------

// WHY: the stored bars must reach the page: a histogram whose every bin holds samples is drawn as one filled
// rectangle per bin, whether it was binned by `hist`, given to `hist_counts` or placed by `hists`.
#[test]
fn the_compiled_figure_draws_one_bar_per_bin() {
    let mut fig = Figure::new().tiles(1, 2);
    let binned = fig.axes(0, 0).hist(STEPPED).bins(Bins::Count(4)).id();
    let counted = fig
        .axes(0, 0)
        .hist_counts([0.0, 1.0, 3.0, 4.0, 6.0], [2.0, 5.0, 1.0, 3.0])
        .id();
    let grouped = fig
        .axes(0, 1)
        .hists(Bins::Edges(vec![0.0, 1.0, 2.0, 3.0, 4.5]))
        .series("first", STEPPED)
        .series("second", OTHER)
        .ids();
    assert_eq!(filled_shapes(&fig, binned), 4);
    assert_eq!(filled_shapes(&fig, counted), 4);
    for id in grouped {
        assert_eq!(filled_shapes(&fig, id), 4);
    }
}

// WHY: users write helpers that configure histograms, so `HistMut` and `HistsMut` are nameable from the prelude, as
// every other handle is.
#[test]
fn the_handles_are_in_the_prelude() {
    fn configure(h: &mut HistMut<'_>) {
        h.bins(Bins::Count(2)).style(HistStyle::Step);
    }
    fn compare(h: &mut HistsMut<'_>) {
        h.series("first", STEPPED).series("second", OTHER);
    }
    let mut fig = Figure::new().tiles(1, 2);
    configure(&mut fig.axes(0, 0).hist(STEPPED));
    compare(&mut fig.axes(0, 1).hists(Bins::Count(3)));
    assert!(fig.validate().is_valid());
}
