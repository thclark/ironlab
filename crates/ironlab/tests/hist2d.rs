//! Two-dimensional histograms and density contours in the facade (#94): `hist2d` and `density_contour`, which bin
//! pairs of samples in the facade and store the binned values as cells or as a contour.
//!
//! The binning itself is tested in `binning.rs`; these tests check that the facade stores what the binning computes,
//! in the form the colour scale, the colourbar and a reader rely on, and that the handles keep the figure clean as
//! their settings change. They assume this API:
//!
//! - `AxesMut::hist2d(x, y) -> Hist2dMut`, each sample `impl Into<DataSource>`, binned with `Bins::default()` along
//!   both dimensions and `HistNorm::Count`. It stores `Cells` with explicit column and row edges, the normalised
//!   counts as a data array of shape `[ny, nx]`, no gap, the default colour scale of the axes, created as `cells`
//!   creates it, and a `Binning2d` record of how the cells were binned (ADR 0029): the bins along x and y as given,
//!   the explicit ranges, the normalisation, the minimum count, the total counted and the numbers excluded.
//! - `Hist2dMut` setters `bins(Bins, Bins)`, `range((min, max), (min, max))`, `weights(impl Into<DataSource>)`,
//!   `normalize(HistNorm)`, `min_count(f64)`, `gap(x, y)`, `color_scale(NodeId)` and `label`; and the getters
//!   `id()`, `values_data()`, `summary() -> HistogramSummary` and `error() -> Option<&BinningError>`.
//! - `AxesMut::density_contour(x, y) -> DensityContourMut`, binned as `hist2d` bins, which stores a `Contour` of the
//!   normalised counts on the rectilinear grid of the bin centres, unfilled, through the default colour scale. The
//!   contour artist is unchanged and carries no binning record.
//! - `DensityContourMut` setters `bins`, `range`, `weights` and `normalize` as on `Hist2dMut`, and `levels(u32)`,
//!   `level_values`, `filled(bool)`, `color_scale(NodeId)` and `label`; and the getters `id()`, `summary()` and
//!   `error()`.

mod common;

use common::{artist, colorbar_span, compile, data, filled_paths, filled_shapes, has_warning};
use ironlab::IssueKind;
use ironlab::binning::{BinningError, histogram2d};
use ironlab::ir::{Artist, Binning2d, CellEdges, CellGap, Cells, Contour, Grid, Levels, NdArray};
use ironlab::prelude::*;

/// The edges of seven unit bins centred on the integers from −3 to 3.
fn unit_edges() -> Vec<f64> {
    (0..8).map(|k| f64::from(k) - 3.5).collect()
}

/// The number of samples a blob places at the integer point `(i, j)`: a rounded Gaussian of peak 40 inside the
/// circle of radius 3, and none outside it, so that the corners of the grid are empty.
fn blob_count(i: i32, j: i32) -> usize {
    let r2 = f64::from(i * i + j * j);
    if r2 > 9.0 {
        0
    } else {
        (40.0 * (-r2 / 4.0).exp()).round() as usize
    }
}

/// A sample of pairs that places `blob_count(i, j)` samples at each integer point of the 7 × 7 grid from −3 to 3,
/// and the counts that unit bins centred on those points hold: row `j + 3` and column `i + 3` count the point
/// `(i, j)`.
fn blob() -> (Vec<f64>, Vec<f64>, Matrix) {
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for j in -3..=3 {
        for i in -3..=3 {
            for _ in 0..blob_count(i, j) {
                x.push(f64::from(i));
                y.push(f64::from(j));
            }
        }
    }
    let counts = Matrix::from_fn(7, 7, |row, col| {
        blob_count(col as i32 - 3, row as i32 - 3) as f64
    });
    (x, y, counts)
}

#[track_caller]
fn cells(fig: &Figure, id: NodeId) -> &Cells {
    match artist(fig, id) {
        Artist::Cells(c) => c,
        other => panic!("a 2D histogram is drawn as cells, found {other:?}"),
    }
}

#[track_caller]
fn contour(fig: &Figure, id: NodeId) -> &Contour {
    match artist(fig, id) {
        Artist::Contour(c) => c,
        other => panic!("a density contour is a contour, found {other:?}"),
    }
}

/// The binning record of a 2D histogram with the identifiers of its arrays of edges
/// cleared, so that records of different figures compare by what they say; the edges
/// themselves are compared on their own.
#[track_caller]
fn record(fig: &Figure, id: NodeId) -> Option<Binning2d> {
    cells(fig, id).binning.as_ref().map(|r| Binning2d {
        x_edges: NodeId::default(),
        y_edges: NodeId::default(),
        ..r.clone()
    })
}

fn vector(fig: &Figure, id: NodeId) -> Vec<f64> {
    data(fig, id).as_f64().expect("floating-point").to_vec()
}

/// The values of an array, with each NaN written as `None` so that arrays holding NaN can be compared.
fn values_of(array: &NdArray) -> Vec<Option<f64>> {
    array
        .as_f64()
        .expect("the values are floating-point")
        .iter()
        .map(|v| (!v.is_nan()).then_some(*v))
        .collect()
}

/// The column edges, row edges and values of the cells of a 2D histogram, the edges as data.
#[track_caller]
fn binned_cells(fig: &Figure, id: NodeId) -> (Vec<f64>, Vec<f64>, &NdArray) {
    let c = cells(fig, id);
    let CellEdges::Edges(columns) = c.columns else {
        panic!(
            "the columns of a 2D histogram lie between its x edges, not {:?}",
            c.columns
        );
    };
    let CellEdges::Edges(rows) = c.rows else {
        panic!(
            "the rows of a 2D histogram lie between its y edges, not {:?}",
            c.rows
        );
    };
    (vector(fig, columns), vector(fig, rows), data(fig, c.values))
}

/// The x and y coordinates of the grid and the field of a density contour.
#[track_caller]
fn contoured(fig: &Figure, id: NodeId) -> (Vec<f64>, Vec<f64>, &NdArray) {
    let c = contour(fig, id);
    let Grid::Rectilinear { x, y } = c.grid else {
        panic!(
            "a density contour lies on the rectilinear grid of the bin centres, not {:?}",
            c.grid
        );
    };
    (vector(fig, x), vector(fig, y), data(fig, c.z))
}

fn centres(edges: &[f64]) -> Vec<f64> {
    edges.windows(2).map(|e| (e[0] + e[1]) / 2.0).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// hist2d: what it stores
// ---------------------------------------------------------------------------------------------------------------

// WHY: a 2D histogram is read cell by cell as the count of the pairs in a bin: each cell must lie exactly between
// its bin's x and y edges, with no gap, so that the cells tile the binned region as the bins do, and must be
// coloured by its count. The counts are stored as a data array mapped through the default colour scale of the axes,
// so that a colourbar of the axes describes them; ordinary data gives a figure without a warning.
#[test]
fn hist2d_stores_seamless_cells_on_the_bin_edges_coloured_by_their_counts() {
    let x = [0.5, 1.5, 1.5, 2.5];
    let y = [0.5, 0.5, 1.5, 1.5];
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .hist2d(x, y)
        .bins(
            Bins::Edges(vec![0.0, 1.0, 2.0, 3.0]),
            Bins::Edges(vec![0.0, 1.0, 2.0]),
        )
        .id();
    let default = fig.axes(0, 0).color_scale().id();
    let (columns, rows, values) = binned_cells(&fig, id);
    assert_eq!(columns, [0.0, 1.0, 2.0, 3.0]);
    assert_eq!(rows, [0.0, 1.0, 2.0]);
    assert_eq!(
        values.shape,
        vec![2, 3],
        "one row per y bin and one column per x bin"
    );
    assert_eq!(values.as_f64(), Some(&[1.0, 1.0, 0.0, 0.0, 1.0, 1.0][..]));
    let c = cells(&fig, id);
    assert_eq!(c.gap, CellGap { x: 0.0, y: 0.0 }, "the cells are seamless");
    assert_eq!(
        c.color_scale,
        Some(default),
        "the default scale of the axes"
    );
    assert_eq!(fig.ir().color_scales.len(), 1);
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: the facade must pass every setting to the binning exactly (both samples with their non-finite values, the
// weights, both ranges, both bin specifications, the normalisation and the minimum count), so that the cells are what
// numpy's `histogram2d` and matplotlib's `hist2d` would give for the same call. The expected values come from the
// binning functions, which `binning.rs` checks against numpy.
#[test]
fn hist2d_draws_the_binning_of_the_pairs_with_every_setting_applied() {
    let x = [0.1, 0.4, 0.4, 1.7, 2.2, 2.9, f64::NAN, 1.1, 0.8, 2.5, 5.0];
    let y = [
        1.0,
        0.2,
        0.9,
        1.4,
        0.3,
        1.9,
        0.5,
        f64::INFINITY,
        1.2,
        0.7,
        0.5,
    ];
    let w = [1.0, 2.0, 0.5, 1.5, 1.0, 3.0, 1.0, 1.0, 2.5, 0.5, 1.0];
    let (bx, by) = (Bins::Count(3), Bins::Edges(vec![0.0, 0.5, 1.0, 2.0]));
    for norm in [
        HistNorm::Count,
        HistNorm::Probability,
        HistNorm::Density,
        HistNorm::Cdf,
    ] {
        let expected = histogram2d(
            &x,
            &y,
            Some(&w),
            (&bx, &by),
            (Some((0.0, 3.0)), Some((0.0, 2.0))),
            norm,
            Some(1.0),
        )
        .unwrap();
        let mut fig = Figure::new();
        let id = fig
            .axes(0, 0)
            .hist2d(x, y)
            .weights(w)
            .bins(bx.clone(), by.clone())
            .range((0.0, 3.0), (0.0, 2.0))
            .normalize(norm)
            .min_count(1.0)
            .id();
        let (columns, rows, values) = binned_cells(&fig, id);
        assert_eq!(columns, expected.x_edges, "{norm:?}");
        assert_eq!(rows, expected.y_edges, "{norm:?}");
        assert_eq!(
            values.shape,
            vec![expected.values.rows(), expected.values.cols()]
        );
        assert_eq!(
            values_of(values),
            expected
                .values
                .values()
                .iter()
                .map(|v| (!v.is_nan()).then_some(*v))
                .collect::<Vec<_>>(),
            "{norm:?}"
        );
        assert!(fig.validate().is_valid(), "{norm:?}: {:?}", fig.validate());
    }
}

// WHY: a 2D histogram always carries a colourbar, which a reader uses to read counts off colours; its automatic
// limits must therefore span exactly the counts the cells hold, from the emptiest bin to the fullest. With a minimum
// count, the empty bins are not drawn and the bar must start at the smallest count drawn, which is also what lets a
// logarithmic scale show counts that span decades without a zero it cannot place.
#[test]
fn the_colourbar_of_a_2d_histogram_spans_its_counts() {
    let (x, y, counts) = blob();
    let fullest = counts.values().iter().copied().fold(0.0, f64::max);
    let emptiest_drawn = counts
        .values()
        .iter()
        .copied()
        .filter(|&c| c >= 1.0)
        .fold(f64::INFINITY, f64::min);
    assert_eq!(
        (fullest, emptiest_drawn),
        (40.0, 4.0),
        "the blob is as described"
    );

    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.hist2d(&x, &y)
        .bins(Bins::Edges(unit_edges()), Bins::Edges(unit_edges()));
    let scale = ax.color_scale().id();
    ax.colorbar(scale, InsideColorbarSide::East);
    assert_eq!(colorbar_span(&fig, scale), (0.0, fullest));

    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.hist2d(&x, &y)
        .bins(Bins::Edges(unit_edges()), Bins::Edges(unit_edges()))
        .min_count(1.0);
    let scale = ax.color_scale().norm(ColorScaleNorm::Log).id();
    ax.colorbar(scale, InsideColorbarSide::East);
    assert_eq!(colorbar_span(&fig, scale), (emptiest_drawn, fullest));
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: matplotlib's `cmin` hides bins too sparse to mean anything, leaving the background visible; the threshold is
// a number of samples (or a sum of weights), whatever the normalisation, because a reader thinks of sparseness in
// samples. A cell below it is NaN, which the default non-finite policy of the scale draws as nothing.
#[test]
fn min_count_makes_bins_with_fewer_samples_nan_whatever_the_normalisation() {
    // Counts [[1, 1, 0], [0, 1, 3]], of six samples in all.
    let x = [0.5, 1.5, 1.5, 2.5, 2.5, 2.5];
    let y = [0.5, 0.5, 1.5, 1.5, 1.5, 1.5];
    let bins = (
        Bins::Edges(vec![0.0, 1.0, 2.0, 3.0]),
        Bins::Edges(vec![0.0, 1.0, 2.0]),
    );
    let mut fig = Figure::new();
    let counted = fig
        .axes(0, 0)
        .hist2d(x, y)
        .bins(bins.0.clone(), bins.1.clone())
        .min_count(1.0)
        .id();
    assert_eq!(
        values_of(binned_cells(&fig, counted).2),
        [Some(1.0), Some(1.0), None, None, Some(1.0), Some(3.0)],
        "empty bins are NaN"
    );
    let probable = fig
        .axes(0, 0)
        .hist2d(x, y)
        .bins(bins.0, bins.1)
        .normalize(HistNorm::Probability)
        .min_count(2.0)
        .id();
    assert_eq!(
        values_of(binned_cells(&fig, probable).2),
        [None, None, None, None, None, Some(0.5)],
        "the bin of three samples is kept although its probability, 0.5, is below 2"
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// ---------------------------------------------------------------------------------------------------------------
// hist2d: the binning record
// ---------------------------------------------------------------------------------------------------------------

// WHY: the figure keeps the binned counts but not the pairs (ADR 0029), so the record on the cells is the only account
// of how they were binned: the bins along each dimension as the author specified them, the ranges, the normalisation,
// the minimum count below which a bin was left empty, how many pairs were counted and how many were left out, and
// why. A datatip reads it to report the count and density of a cell, so it must say exactly what the binning did.
#[test]
fn hist2d_records_the_bins_ranges_normalisation_minimum_count_total_and_exclusions() {
    let x = [0.5, f64::NAN, 1.5, 2.5, 9.0, 0.5, 1.2];
    let y = [0.5, 0.5, f64::INFINITY, 0.5, 0.5, -4.0, 0.7];
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .hist2d(x, y)
        .bins(Bins::Count(3), Bins::Edges(vec![0.0, 0.5, 1.0]))
        .range((0.0, 3.0), (0.0, 1.0))
        .normalize(HistNorm::Density)
        .min_count(1.0)
        .id();
    assert_eq!(
        record(&fig, id),
        Some(Binning2d {
            x_edges: NodeId::default(),
            y_edges: NodeId::default(),
            x_bins: Bins::Count(3),
            y_bins: Bins::Edges(vec![0.0, 0.5, 1.0]),
            x_range: Some([0.0, 3.0]),
            y_range: Some([0.0, 1.0]),
            normalization: HistNorm::Density,
            min_count: Some(1.0),
            total: 3.0,
            excluded_non_finite: 2,
            excluded_out_of_range: 2,
        })
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
}

// WHY: the record holds the edges the specification resolved to along x and along y, from which a datatip finds the
// area of a cell's bin. They are the edges the cells are drawn between, so the record refers to the cells' own arrays
// of column and row edges rather than storing them twice; and after rebinning, to the new ones.
#[test]
fn the_record_of_a_2d_histogram_refers_to_the_edges_of_its_cells() {
    let (x, y, _) = blob();
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let mut h = ax.hist2d(&x, &y);
    h.bins(Bins::Count(5), Bins::Count(4));
    h.bins(Bins::Edges(unit_edges()), Bins::Width(2.0));
    let id = h.id();
    let c = cells(&fig, id);
    let (CellEdges::Edges(columns), CellEdges::Edges(rows)) = (c.columns, c.rows) else {
        panic!("a 2D histogram lies on explicit edges");
    };
    let r = c
        .binning
        .as_ref()
        .expect("the histogram records its binning");
    assert_eq!((r.x_edges, r.y_edges), (columns, rows));
    assert_eq!(vector(&fig, r.x_edges), unit_edges());
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: the record says how the bins were chosen, not what they turned out to be, which the cells themselves show: a
// bare `hist2d` records numpy's automatic rule along both dimensions, no ranges, raw counts and no minimum count,
// rather than the edges or the ranges of the pairs that the rule found.
#[test]
fn a_bare_2d_histogram_records_the_automatic_rule_no_ranges_and_counts() {
    let (x, y, _) = blob();
    let n = x.len() as f64;
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).hist2d(&x, &y).id();
    assert_eq!(
        record(&fig, id),
        Some(Binning2d {
            x_edges: NodeId::default(),
            y_edges: NodeId::default(),
            x_bins: Bins::Rule(BinRule::Auto),
            y_bins: Bins::Rule(BinRule::Auto),
            x_range: None,
            y_range: None,
            normalization: HistNorm::Count,
            min_count: None,
            total: n,
            excluded_non_finite: 0,
            excluded_out_of_range: 0,
        })
    );
}

// WHY: the record exists so that a reader pointing at a cell learns its count and density without the pairs (ADR
// 0029). For a probability histogram of cells of unequal area the value drawn is neither, so the datatip of each
// cell must recover the count from the value and the total, and the density from the count, the total and the
// cell's area.
#[test]
fn the_datatip_of_each_cell_reports_its_count_and_density_from_the_record() {
    // Counts [[1, 1, 0], [0, 1, 3]] of six pairs, in columns of widths 1, 1 and 2 and rows of heights 1 and 2.
    let x = [0.5, 1.5, 1.5, 3.0, 3.0, 3.0];
    let y = [0.5, 0.5, 2.0, 2.0, 2.0, 2.0];
    let counts = [[1.0, 1.0, 0.0], [0.0, 1.0, 3.0]];
    let widths = [1.0, 1.0, 2.0];
    let heights = [1.0, 2.0];
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .hist2d(x, y)
        .bins(
            Bins::Edges(vec![0.0, 1.0, 2.0, 4.0]),
            Bins::Edges(vec![0.0, 1.0, 3.0]),
        )
        .normalize(HistNorm::Probability)
        .id();
    let scene = compile(&fig);
    let hits: Vec<_> = scene
        .hit_map
        .cells
        .iter()
        .filter(|c| c.artist == id)
        .collect();
    assert_eq!(hits.len(), 6, "one datatip per cell");
    for hit in hits {
        let (row, column) = (hit.row, hit.column);
        let reading = hit
            .bin
            .unwrap_or_else(|| panic!("cell ({row}, {column}) has a reading"));
        let count = counts[row][column];
        let density = count / (6.0 * widths[column] * heights[row]);
        assert!(
            (reading.count - count).abs() < 1e-9 && (reading.density - density).abs() < 1e-9,
            "cell ({row}, {column}) reads {reading:?}, not a count of {count} and a density of {density}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// hist2d: setters
// ---------------------------------------------------------------------------------------------------------------

// WHY: each setter that changes the binning rebins in place; the figure must end exactly as the final settings alone
// would make it, with the same cells and no array of an earlier binning left behind to bloat every saved file (the
// `UnusedData` warning), and the histogram must keep its identifier, so that the handle and a legend entry still
// name it.
#[test]
fn rebinning_a_2d_histogram_leaves_the_figure_as_the_final_settings_alone_would_make_it() {
    let (x, y, _) = blob();
    let w: Vec<f64> = (0..x.len()).map(|k| 1.0 + (k % 3) as f64).collect();
    let mut tried = Figure::new();
    let mut ax = tried.axes(0, 0);
    let mut h = ax.hist2d(&x, &y);
    let id = h.id();
    h.bins(Bins::Count(20), Bins::Count(5))
        .normalize(HistNorm::Probability)
        .range((-1.0, 1.0), (-1.0, 1.0))
        .min_count(3.0)
        .weights(&w)
        .bins(Bins::Count(6), Bins::Width(1.5))
        .range((-3.0, 3.0), (-3.0, 3.0))
        .normalize(HistNorm::Density)
        .min_count(2.0);
    assert_eq!(h.id(), id, "the histogram keeps its identifier");

    let mut direct = Figure::new();
    let reference = direct
        .axes(0, 0)
        .hist2d(&x, &y)
        .bins(Bins::Count(6), Bins::Width(1.5))
        .range((-3.0, 3.0), (-3.0, 3.0))
        .weights(&w)
        .normalize(HistNorm::Density)
        .min_count(2.0)
        .id();
    let (tc, tr, tv) = binned_cells(&tried, id);
    let (dc, dr, dv) = binned_cells(&direct, reference);
    assert_eq!((tc, tr), (dc, dr));
    assert_eq!(tv.shape, dv.shape);
    assert_eq!(values_of(tv), values_of(dv));
    assert_eq!(
        tried.ir().data.len(),
        direct.ir().data.len(),
        "no array of an earlier binning is kept"
    );
    assert_eq!(
        record(&tried, id),
        record(&direct, reference),
        "the record describes the final binning, not an earlier one"
    );
    assert_eq!(
        tried.ir().color_scales.len(),
        1,
        "rebinning creates no second scale"
    );
    let report = tried.validate();
    assert!(!has_warning(&report, IssueKind::UnusedData), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: counts that span decades need a logarithmic scale, which the author creates and names; naming it replaces the
// default scale the call created, which is deleted because nothing else names it (no stray colourbar), and rebinning
// afterwards must keep the author's scale rather than return to a default.
#[test]
fn a_scale_the_author_names_replaces_the_default_and_survives_rebinning() {
    let (x, y, _) = blob();
    let mut fig = Figure::new();
    let log = fig.color_scale().norm(ColorScaleNorm::Log).id();
    let id = fig
        .axes(0, 0)
        .hist2d(&x, &y)
        .color_scale(log)
        .bins(Bins::Count(5), Bins::Count(5))
        .min_count(1.0)
        .id();
    assert_eq!(cells(&fig, id).color_scale, Some(log));
    assert_eq!(
        fig.ir().color_scales.len(),
        1,
        "the default scale is deleted"
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());

    // A second histogram in the same axes shares the default scale, so that one colourbar describes both.
    let mut fig = Figure::new();
    let first = fig.axes(0, 0).hist2d(&x, &y).id();
    let second = fig.axes(0, 0).hist2d(&y, &x).id();
    assert_eq!(
        cells(&fig, first).color_scale,
        cells(&fig, second).color_scale
    );
    assert_eq!(fig.ir().color_scales.len(), 1);
}

// WHY: the gap and the label are the cells' own properties, written as `CellsMut` writes them: a gap sets bins apart
// when the cells are few, and the label names the histogram in a legend.
#[test]
fn gap_and_label_write_the_properties_of_the_cells() {
    let (x, y, _) = blob();
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .hist2d(&x, &y)
        .gap(0.1, 0.2)
        .label("Positions")
        .bins(Bins::Count(4), Bins::Count(4))
        .id();
    let c = cells(&fig, id);
    assert_eq!(c.gap, CellGap { x: 0.1, y: 0.2 }, "rebinning keeps the gap");
    assert_eq!(c.label, Some(Text::new("Positions")));
}

// WHY: the figure does not hold the samples, so the handle is where the author learns how many pairs the histogram
// counts and how many it left out: a pair is left out when either value is not finite, or when either lies outside
// its range.
#[test]
fn the_summary_reports_the_pairs_included_and_excluded() {
    let x = [0.5, f64::NAN, 1.5, 2.5, 9.0, 0.5];
    let y = [0.5, 0.5, f64::INFINITY, 0.5, 0.5, -4.0];
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let mut h = ax.hist2d(x, y);
    h.range((0.0, 3.0), (0.0, 1.0))
        .bins(Bins::Count(3), Bins::Count(1));
    assert_eq!(
        h.summary(),
        HistogramSummary {
            total: 2.0,
            excluded_non_finite: 2,
            excluded_out_of_range: 2,
        }
    );
    assert!(h.error().is_none());
}

// WHY: samples of different lengths, or a bin specification that cannot be applied, are mistakes the author must
// hear of without a panic in the middle of building the figure: the histogram is stored with no cells, so that the
// figure stays valid and validation warns that it has nothing to draw (ADR 0012), and the handle returns the reason
// until a later setter fixes the specification.
#[test]
fn a_bad_specification_draws_no_cells_and_the_handle_reports_why() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let h = ax.hist2d([0.0, 1.0, 2.0], [0.0, 1.0]);
    assert_eq!(
        h.error(),
        Some(&BinningError::LengthMismatch {
            expected: 3,
            actual: 2
        })
    );
    let id = h.id();
    assert_eq!(data(&fig, cells(&fig, id).values).len(), 0, "no cells");
    assert_eq!(
        cells(&fig, id).binning,
        None,
        "no record of bins that were not made, which would itself make the figure invalid"
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(has_warning(&report, IssueKind::NothingToDraw), "{report:?}");

    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let mut h = ax.hist2d([0.0, 1.0, 2.0], [0.0, 1.0, 2.0]);
    h.bins(Bins::Count(2), Bins::Count(0));
    assert_eq!(h.error(), Some(&BinningError::ZeroBins));
    h.bins(Bins::Count(2), Bins::Count(2));
    assert_eq!(h.error(), None, "a fixed specification clears the reason");
    let id = h.id();
    assert_eq!(data(&fig, cells(&fig, id).values).shape, vec![2, 2]);
    assert!(
        cells(&fig, id).binning.is_some(),
        "the fixed bins are recorded"
    );
    let report = fig.validate();
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: pairs that are all NaN, or no pairs at all, are the normal state of a figure built before its data arrives
// (ADR 0012): no error, no cells, and a warning that there is nothing to draw.
#[test]
fn no_pairs_draw_no_cells_without_an_error() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let h = ax.hist2d(Vec::<f64>::new(), Vec::<f64>::new());
    assert_eq!(h.error(), None);
    let id = h.id();
    assert_eq!(data(&fig, cells(&fig, id).values).len(), 0);
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(has_warning(&report, IssueKind::NothingToDraw), "{report:?}");
}

// ---------------------------------------------------------------------------------------------------------------
// density_contour
// ---------------------------------------------------------------------------------------------------------------

// WHY: a density contour shows the shape of a joint distribution as isolines of the same counts a 2D histogram
// shows; each count belongs to its bin, so the field is sampled at the bin centres, on a rectilinear grid, and the
// isolines are coloured by level through the default colour scale of the axes. Ordinary data gives a figure without
// a warning.
#[test]
fn density_contour_stores_the_counts_on_the_grid_of_the_bin_centres() {
    let (x, y, counts) = blob();
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .density_contour(&x, &y)
        .bins(Bins::Edges(unit_edges()), Bins::Edges(unit_edges()))
        .id();
    let default = fig.axes(0, 0).color_scale().id();
    let (gx, gy, z) = contoured(&fig, id);
    let expected_centres: Vec<f64> = (-3..=3).map(f64::from).collect();
    assert_eq!(gx, expected_centres);
    assert_eq!(gy, expected_centres);
    assert_eq!(gx, centres(&unit_edges()));
    assert_eq!(z.shape, vec![7, 7]);
    assert_eq!(z.as_f64(), Some(counts.values()));
    let c = contour(&fig, id);
    assert!(!c.fill, "isolines by default");
    assert_eq!(c.color_scale, Some(default));
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: the levels and the fill decide what a density contour shows (a few nested isolines, or filled bands as
// Plotly's `histogram2dcontour` draws them), and the normalisation decides what the levels mean, so each setter must
// write its property; rebinning replaces the grid and the field without leaving the old arrays behind.
#[test]
fn density_contour_setters_write_levels_fill_and_normalisation_and_rebin_cleanly() {
    let (x, y, _) = blob();
    let mut fig = Figure::new();
    let chosen = fig.color_scale().colormap(Colormap::Cividis).id();
    let id = fig
        .axes(0, 0)
        .density_contour(&x, &y)
        .bins(Bins::Count(20), Bins::Count(20))
        .normalize(HistNorm::Probability)
        .levels(6)
        .filled(true)
        .label("Joint density")
        .color_scale(chosen)
        .bins(Bins::Count(7), Bins::Count(7))
        .normalize(HistNorm::Density)
        .id();
    let c = contour(&fig, id).clone();
    assert!(c.fill);
    assert_eq!(c.levels, Some(Levels::Auto { count: 6 }));
    assert_eq!(c.label, Some(Text::new("Joint density")));
    assert_eq!(c.color_scale, Some(chosen));
    assert_eq!(
        fig.ir().color_scales.len(),
        1,
        "the default scale is deleted"
    );

    let expected = histogram2d(
        &x,
        &y,
        None,
        (&Bins::Count(7), &Bins::Count(7)),
        (None, None),
        HistNorm::Density,
        None,
    )
    .unwrap();
    let (gx, gy, z) = contoured(&fig, id);
    assert_eq!(gx, centres(&expected.x_edges));
    assert_eq!(gy, centres(&expected.y_edges));
    assert_eq!(z.as_f64(), Some(expected.values.values()));

    let mut direct = Figure::new();
    let chosen = direct.color_scale().colormap(Colormap::Cividis).id();
    direct
        .axes(0, 0)
        .density_contour(&x, &y)
        .bins(Bins::Count(7), Bins::Count(7))
        .normalize(HistNorm::Density)
        .color_scale(chosen);
    assert_eq!(
        fig.ir().data.len(),
        direct.ir().data.len(),
        "no array of an earlier binning is kept"
    );
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");

    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .density_contour(&x, &y)
        .level_values([5.0, 15.0, 30.0])
        .filled(true)
        .filled(false)
        .id();
    let c = contour(&fig, id);
    assert_eq!(
        c.levels,
        Some(Levels::Explicit {
            values: vec![5.0, 15.0, 30.0]
        })
    );
    assert!(!c.fill, "`filled(false)` returns to isolines");
}

// WHY: a bad specification must not panic and must leave a valid figure that draws nothing, with the reason on the
// handle, exactly as for `hist2d`.
#[test]
fn a_density_contour_of_a_bad_specification_draws_nothing_and_the_handle_reports_why() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let mut h = ax.density_contour([0.0, 1.0, 2.0], [0.0, 1.0, 2.0]);
    h.range((1.0, 0.0), (0.0, 1.0));
    assert_eq!(h.error(), Some(&BinningError::InvalidRange(1.0, 0.0)));
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(has_warning(&report, IssueKind::NothingToDraw), "{report:?}");
}

// ---------------------------------------------------------------------------------------------------------------
// The compiled figure
// ---------------------------------------------------------------------------------------------------------------

// WHY: the stored cells must reach the page: one filled cell per bin, and, with a minimum count of one, none for the
// empty bins, which leave the background visible (29 of the 49 bins of the blob hold samples).
#[test]
fn the_compiled_figure_draws_one_cell_per_bin_and_none_for_bins_below_the_minimum_count() {
    let (x, y, counts) = blob();
    let occupied = counts.values().iter().filter(|&&c| c >= 1.0).count();
    assert_eq!(occupied, 29, "the lattice points within radius 3");
    let mut fig = Figure::new().tiles(1, 2);
    let every = fig
        .axes(0, 0)
        .hist2d(&x, &y)
        .bins(Bins::Edges(unit_edges()), Bins::Edges(unit_edges()))
        .id();
    let sparse = fig
        .axes(0, 1)
        .hist2d(&x, &y)
        .bins(Bins::Edges(unit_edges()), Bins::Edges(unit_edges()))
        .min_count(1.0)
        .id();
    assert_eq!(filled_shapes(&fig, every), 49);
    assert_eq!(filled_shapes(&fig, sparse), occupied);
}

// WHY: the stored contour must reach the page: filled at three levels inside the range of the counts, a density
// contour draws four bands (below the first level, between each pair, and above the last), each in its own colour.
#[test]
fn the_compiled_figure_draws_one_filled_band_per_interval_of_the_levels() {
    let (x, y, _) = blob();
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .density_contour(&x, &y)
        .bins(Bins::Edges(unit_edges()), Bins::Edges(unit_edges()))
        .level_values([5.0, 15.0, 30.0])
        .filled(true)
        .id();
    let colours: std::collections::BTreeSet<[u8; 4]> = filled_paths(&fig, id)
        .into_iter()
        .map(|(colour, _)| colour)
        .collect();
    assert_eq!(colours.len(), 4, "one colour per band: {colours:?}");
}

// WHY: quantities that span decades (particle sizes against energies, frequencies against amplitudes) are binned on
// logarithmically spaced edges and drawn on logarithmic axes, where each decade is the same distance on the page.
// The cells must lie on exactly the edges given, so that every cell of a row is drawn the same width and every cell
// of a column the same height; edges altered into a linear spacing, or unit edges starting at −0.5 that a logarithmic
// axis cannot place, would distort the bins or leave them undrawn with a `NonPositiveOnLogAxis` warning.
#[test]
fn a_2d_histogram_on_logarithmic_edges_draws_cells_of_equal_size_on_logarithmic_axes() {
    let x_edges: Vec<f64> = vec![1.0, 10.0, 100.0, 1000.0];
    let y_edges: Vec<f64> = vec![0.1, 1.0, 10.0];
    // One pair at the geometric centre of each bin, and a second pair in the bin of the largest x and y.
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for j in 0..2 {
        for i in 0..3 {
            x.push((x_edges[i] * x_edges[i + 1]).sqrt());
            y.push((y_edges[j] * y_edges[j + 1]).sqrt());
        }
    }
    x.push(500.0);
    y.push(5.0);
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.xscale(Scale::Log).yscale(Scale::Log);
    let id = ax
        .hist2d(&x, &y)
        .bins(Bins::Edges(x_edges.clone()), Bins::Edges(y_edges.clone()))
        .id();
    let (columns, rows, values) = binned_cells(&fig, id);
    assert_eq!(columns, x_edges);
    assert_eq!(rows, y_edges);
    assert_eq!(values.as_f64(), Some(&[1.0, 1.0, 1.0, 1.0, 1.0, 2.0][..]));
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");

    let scene = common::compile(&fig);
    let drawn: Vec<_> = scene
        .hit_map
        .cells
        .iter()
        .filter(|c| c.artist == id)
        .collect();
    assert_eq!(drawn.len(), 6, "every bin is drawn");
    let (width, height) = (drawn[0].rect.width, drawn[0].rect.height);
    for cell in &drawn {
        assert!(
            (cell.rect.width - width).abs() < 0.5 && (cell.rect.height - height).abs() < 0.5,
            "each decade is the same size on the page: {cell:?} against {width} × {height}"
        );
    }
}

// WHY: densities span decades, so a density contour is often coloured through a logarithmic scale; the bins where
// the density is zero (the corners of the blob) cannot be placed on that scale, but they are an ordinary part of a
// sampled distribution, not a fault, so the figure must stay valid and clean, and the bands between log-spaced levels
// must each be drawn in their own colour of the author's scale.
#[test]
fn a_density_contour_through_a_logarithmic_scale_draws_its_bands_despite_empty_bins() {
    let (x, y, counts) = blob();
    assert!(counts.values().contains(&0.0), "the blob has empty bins");
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let log = ax.color_scale().norm(ColorScaleNorm::Log).id();
    let id = ax
        .density_contour(&x, &y)
        .bins(Bins::Edges(unit_edges()), Bins::Edges(unit_edges()))
        .level_values([5.0, 10.0, 20.0])
        .filled(true)
        .color_scale(log)
        .id();
    ax.colorbar(log, InsideColorbarSide::East);
    let c = contour(&fig, id);
    assert_eq!(c.color_scale, Some(log));
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
    let colours: std::collections::BTreeSet<[u8; 4]> = filled_paths(&fig, id)
        .into_iter()
        .map(|(colour, _)| colour)
        .collect();
    assert_eq!(colours.len(), 4, "one colour per band: {colours:?}");
}

// WHY: users write helpers that configure these handles, so both are nameable from the prelude, as every other
// handle is.
#[test]
fn the_handles_are_in_the_prelude() {
    fn configure(h: &mut Hist2dMut<'_>) {
        h.bins(Bins::Count(2), Bins::Count(2));
    }
    fn outline(h: &mut DensityContourMut<'_>) {
        h.levels(3);
    }
    let (x, y, _) = blob();
    let mut fig = Figure::new().tiles(1, 2);
    configure(&mut fig.axes(0, 0).hist2d(&x, &y));
    outline(&mut fig.axes(0, 1).density_contour(&x, &y));
    assert!(fig.validate().is_valid());
}
