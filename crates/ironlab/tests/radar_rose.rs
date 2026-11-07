//! Filled series, radar charts, polar histograms and wind roses in the facade (#123, #124).
//!
//! These tests assume this API:
//!
//! - `SeriesMut::base(y)`, `SeriesMut::fill(colour)` and `SeriesMut::fill_opacity(a)`, which set the base, the fill
//!   and the fill opacity of a series. A fill needs an explicit base; validation reports a fill without one.
//! - `AxesMut::spokes(labels)`, which converts the axes to polar axes for a radar chart of `n = labels.len()` spokes:
//!   the angle is measured in `AngleUnits::PerTurn(n)`, the angular limits are manual `[0, n]`, the angular ticks are
//!   the values `0, 1, …, n − 1` labelled with the labels, angle 0 points north, angles increase clockwise, and both
//!   grids are on.
//! - `AxesMut::radar(values) -> SeriesMut`, which adds a series through the points `(k, values[k])` for every spoke
//!   `k` and repeats the first value at `x = n`, so that the outline closes with a straight chord back to the first
//!   spoke; it draws a line and circle markers and is filled automatically (in the series colour) at an opacity of
//!   0.25 down to the manual lower radial limit, or to 0 when the radial limits are automatic. Values of the wrong
//!   number for the spokes are stored as given and reported by validation, as other length mismatches are.
//! - `AxesMut::polar_hist(angles) -> HistMut`, which converts the axes to polar axes, wraps every finite angle into
//!   `[0, turn)` of the axes' angle units, and bins the wrapped angles exactly as `hist` would with the range fixed to
//!   `[0, turn]` and 16 bins unless `bins` says otherwise.
//! - `AxesMut::wind_rose(directions, speeds, class_edges)`, which converts the axes to polar axes with north at the
//!   top and clockwise angles, bins the directions into 16 sectors centred on 0°, 22.5°, …, 337.5° (each spanning half
//!   a sector either side of its centre, so the sector of 0° spans north), and bins the speeds into the classes between
//!   consecutive edges, each class holding the speeds from its lower edge up to, but not including, its upper edge,
//!   and the last class open above its edge. A speed below the first edge is a calm: it counts towards the total but
//!   lies in no class. It adds one stacked bar series per class, at the sector centres, whose values are the
//!   percentage of all observations with a finite direction and speed that fall in each sector and class, labelled
//!   from the edges (`"0–2"`, `"2–4"`, and an open label for the last class). The handle it returns lists the series
//!   with `ids()`.

mod common;

use std::f64::consts::{PI, TAU};

use common::{
    BarExtent, artist, axes, bar, bar_extents, compile, data, has_error_at, series, stairs,
};
use ironlab::IssueKind;
use ironlab::ir::{
    Artist, Binning, ColorSpec, DashStyle, Limits, MarkerShape, PolarView, Projection, StairsData,
    TickLabels, Ticks,
};
use ironlab::prelude::*;

const CRITERIA: [&str; 5] = ["Mass", "Cost", "Efficiency", "Noise", "Range"];

/// Agreement expected between a number the facade computes and one computed here from its definition.
const TOL: f64 = 1e-9;

fn vector(fig: &Figure, id: NodeId) -> Vec<f64> {
    data(fig, id)
        .as_f64()
        .expect("the arrays are floating-point")
        .to_vec()
}

#[track_caller]
fn assert_all_close(actual: &[f64], expected: &[f64], what: &str) {
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

/// Returns the polar view of an axes, panicking if the axes is not polar.
fn polar_view(fig: &Figure, id: NodeId) -> PolarView {
    match axes(fig, id).projection {
        Projection::Polar { polar, .. } => polar,
        other => panic!("expected polar axes, found {other:?}"),
    }
}

/// The binning record and the values of a histogram drawn as bars or stairs, with the edges of its bins.
fn histogram_of(fig: &Figure, id: NodeId) -> (Binning, Vec<f64>, Vec<f64>) {
    let (record, values) = match artist(fig, id) {
        Artist::Bar(b) => (b.binning.clone(), vector(fig, b.values)),
        Artist::Stairs(s) => {
            let StairsData::Edges { values, .. } = s.data else {
                panic!("histogram stairs lie between edges, not {:?}", s.data);
            };
            (s.binning.clone(), vector(fig, values))
        }
        other => panic!("a histogram is drawn as bars or stairs, found {other:?}"),
    };
    let record = record.expect("a histogram of a sample carries its binning record");
    let edges = vector(fig, record.edges);
    (record, edges, values)
}

/// The edges of `n` equal bins over one turn of `turn` units.
fn turn_edges(turn: f64, n: usize) -> Vec<f64> {
    (0..=n).map(|k| turn * k as f64 / n as f64).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Filled series
// ---------------------------------------------------------------------------------------------------------------

// WHY: the setters are the user's only way to fill a series from code, so each must store exactly the IR property it
// names: the base as a value of y, the colour as given and the opacity as given; and a series nobody asked to fill
// must stay unfilled.
#[test]
fn the_fill_setters_store_the_base_the_fill_and_its_opacity() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let plain = ax.plot([0.0, 1.0, 2.0], [1.0, 3.0, 2.0]).id();
    let id = ax
        .plot([0.0, 1.0, 2.0], [1.0, 3.0, 2.0])
        .base(-1.0)
        .fill(Color::rgb(0.0, 0.0, 1.0))
        .fill_opacity(0.25)
        .id();
    let s = series(&fig, id);
    assert_eq!(s.base, Some(-1.0));
    assert_eq!(
        s.fill,
        ColorSpec::Rgba {
            color: Color::rgb(0.0, 0.0, 1.0)
        }
    );
    assert_eq!(s.fill_opacity, 0.25);
    let p = series(&fig, plain);
    assert_eq!(
        (p.base, p.fill, p.fill_opacity),
        (None, ColorSpec::None, 1.0)
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: the facade does not invent a base, because "fill to 0" is wrong for data far from 0 and on logarithmic axes; a
// fill set without a base is therefore a mistake the user must be told about by validation, against the series,
// rather than a silent fill to some guessed value.
#[test]
fn a_fill_without_a_base_is_reported_by_validate() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .plot([0.0, 1.0, 2.0], [1.0, 3.0, 2.0])
        .fill(ColorSpec::Auto)
        .id();
    assert_eq!(series(&fig, id).base, None);
    assert!(
        has_error_at(&fig.validate(), IssueKind::FillWithoutBase, id),
        "{:?}",
        fig.validate()
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Radar charts
// ---------------------------------------------------------------------------------------------------------------

// WHY: a radar chart is a polar axes in which each spoke is a criterion; measuring the angle in n units per turn puts
// spoke k at angle k, so the data needs no conversion to degrees. The manual limits [0, n] keep the full turn whatever
// the data, the ticks label exactly the n spokes (none at n, which is spoke 0 again), and north with clockwise
// angles is the convention every radar chart follows; without both grids there are no spokes and rings to read
// values against.
#[test]
fn spokes_make_polar_axes_of_one_unit_per_spoke_labelled_north_and_clockwise() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).spokes(&CRITERIA).id();
    let view = polar_view(&fig, id);
    assert_eq!(view.angle_units, AngleUnits::PerTurn(5.0));
    assert_eq!(view.zero_deg, 90.0, "spoke 0 points north");
    let a = axes(&fig, id);
    assert!(a.x.reverse, "the spokes run clockwise");
    assert_eq!(a.x.limits, Limits::Manual { min: 0.0, max: 5.0 });
    assert_eq!(a.x.ticks, Ticks::Values(vec![0.0, 1.0, 2.0, 3.0, 4.0]));
    let TickLabels::Text(labels) = &a.x.tick_labels else {
        panic!("the spokes are labelled with text: {:?}", a.x.tick_labels);
    };
    let labels: Vec<&str> = labels.iter().map(|t| t.content.as_str()).collect();
    assert_eq!(labels, CRITERIA);
    assert!(a.x.grid, "the spokes are drawn");
    assert!(a.y.grid, "the rings are drawn");
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: the outline of a radar chart is closed: the last spoke joins back to the first. A series does not close
// itself, so `radar` stores n + 1 points with the first value repeated at x = n, which lies on spoke 0 a turn later,
// so the closing edge is a straight chord like every other edge. The look is the radar chart's: a line, a marker on
// each spoke, and a translucent fill of the series colour down to the centre, so that overlaid candidates stay
// readable.
#[test]
fn radar_adds_a_closed_outline_with_markers_and_a_translucent_fill_to_the_centre() {
    let values = [0.8, 0.4, 0.9, 0.3, 0.6];
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.spokes(&CRITERIA);
    let id = ax.radar(values).id();
    let s = series(&fig, id);
    assert_eq!(vector(&fig, s.x), vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(vector(&fig, s.y), vec![0.8, 0.4, 0.9, 0.3, 0.6, 0.8]);
    assert_ne!(s.line.dash, DashStyle::None, "the outline is drawn");
    assert_ne!(s.line.color, ColorSpec::None, "the outline is drawn");
    assert_eq!(s.marker.shape, MarkerShape::Circle);
    assert_eq!(s.base, Some(0.0));
    assert_eq!(s.fill, ColorSpec::Auto, "the fill takes the series colour");
    assert_eq!(s.fill_opacity, 0.25);
    let report = fig.validate();
    assert!(
        report.errors.is_empty() && report.warnings.is_empty(),
        "{report:?}"
    );
}

// WHY: the fill of a radar outline goes down to the centre of the plot, which is the lower radial limit; when the user
// has fixed that limit (to spread values crowded near the rim, say), a base of 0 would lie below the centre and have no
// place, so the base follows the manual lower limit.
#[test]
fn radar_fills_down_to_a_manual_lower_radial_limit() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.spokes(&CRITERIA).rlim(0.2, 1.0);
    let id = ax.radar([0.8, 0.4, 0.9, 0.3, 0.6]).id();
    assert_eq!(series(&fig, id).base, Some(0.2));
}

// WHY: the closing chord depends on the angle units: in n units per turn, x = n must land exactly on spoke 0, or the
// outline would end short of where it began. The drawn line of a radar chart therefore starts and ends at the same
// point on the page, north of the centre.
#[test]
fn the_outline_of_a_radar_chart_ends_where_it_starts() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.spokes(&CRITERIA);
    let id = ax.radar([0.8, 0.4, 0.9, 0.3, 0.6]).id();
    let scene = compile(&fig);
    let mut points = Vec::new();
    scene.display_list.visit_leaves(|item, transform, _| {
        if item.source != Some(id) {
            return;
        }
        if let ironlab_scene::display::ItemKind::Path(path) = &item.kind
            && path.stroke.is_some()
            && path.fill.is_none()
        {
            for segment in &path.segments {
                if let ironlab_scene::display::PathSegment::MoveTo(p)
                | ironlab_scene::display::PathSegment::LineTo(p) = *segment
                {
                    points.push(transform.apply(p));
                }
            }
        }
    });
    assert_eq!(points.len(), 6, "six points, five chords: {points:?}");
    let (first, last) = (points[0], points[5]);
    assert!(
        (first.x - last.x).abs() < 1e-6 && (first.y - last.y).abs() < 1e-6,
        "the outline closes: {first:?} and {last:?}"
    );
    assert!(
        points[1].x > first.x,
        "spoke 1 lies clockwise of north, to the east"
    );
}

// WHY: a radar chart needs one value per spoke; a list of the wrong length is a mistake in the user's data, which
// must be reported by validation against the series, as other length mismatches are, rather than panic or draw a
// polygon whose vertices sit on the wrong spokes.
#[test]
fn radar_values_of_the_wrong_number_for_the_spokes_are_reported_by_validate() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.spokes(&CRITERIA);
    let id = ax.radar([0.8, 0.4, 0.9, 0.3]).id();
    assert!(
        has_error_at(&fig.validate(), IssueKind::ShapeMismatch, id),
        "{:?}",
        fig.validate()
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Polar histograms
// ---------------------------------------------------------------------------------------------------------------

// WHY: an angular histogram spans one turn whatever the data, so that bins are comparable between figures and the
// last bin adjoins the first; binning over the range of the data, as `hist` does, would leave most of the circle out
// for data in one sector. Sixteen bins, the compass points to a sixteenth, is the default.
#[test]
fn a_polar_histogram_spans_one_turn_in_sixteen_bins_whatever_the_data() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let id = ax.polar_hist(vec![10.0, 20.0, 30.0]).id();
    let axes_id = ax.id();
    assert!(matches!(
        axes(&fig, axes_id).projection,
        Projection::Polar { .. }
    ));
    let (record, edges, values) = histogram_of(&fig, id);
    assert_eq!(record.bins, Bins::Count(16));
    assert_eq!(record.range, Some([0.0, 360.0]));
    assert_all_close(&edges, &turn_edges(360.0, 16), "the bin edges");
    let mut expected = vec![0.0; 16];
    expected[0] = 2.0; // 10° and 20° in [0°, 22.5°)
    expected[1] = 1.0; // 30° in [22.5°, 45°)
    assert_all_close(&values, &expected, "the counts");
}

// WHY: an angle is the same direction a whole turn later or earlier, so a polar histogram must count each angle in
// the bin of its direction: −10° is the direction of 350°, 360° that of 0°, and 735° that of 15°. Counting by the raw
// value would drop such angles as out of range or put them in no bin at all. The record states the wrapped sample: all
// seven finite angles counted, the two non-finite ones excluded, none out of range.
#[test]
fn angles_are_wrapped_into_one_turn_before_they_are_binned() {
    let sample = vec![
        -10.0,
        350.0,
        355.0,
        360.0,
        0.0,
        5.0,
        735.0,
        f64::NAN,
        f64::INFINITY,
    ];
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let mut h = ax.polar_hist(sample);
    h.bins(Bins::Count(36));
    let summary = h.summary();
    let id = h.id();
    let (record, edges, values) = histogram_of(&fig, id);
    assert_all_close(&edges, &turn_edges(360.0, 36), "ten-degree bins");
    let mut expected = vec![0.0; 36];
    expected[0] = 3.0; // 360° → 0°, 0°, 5°
    expected[1] = 1.0; // 735° → 15°
    expected[35] = 3.0; // −10° → 350°, 350°, 355°
    assert_all_close(&values, &expected, "the counts");
    assert_eq!(record.total, 7.0);
    assert_eq!(record.excluded_non_finite, 2);
    assert_eq!(record.excluded_out_of_range, 0);
    assert_eq!(summary.excluded_non_finite, 2);
    assert_eq!(summary.excluded_out_of_range, 0);
}

// WHY: the angle a hair below 0 is a direction a hair before north; wrapping it in floating point can round to exactly
// one turn, which lies on the closed upper edge of the range. Either way it must be counted, never excluded as out of
// range, or the histogram would lose observations at north, the most common heading in many data sets.
#[test]
fn an_angle_just_below_zero_is_counted() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let h = ax.polar_hist(vec![-1e-14, -1e-300, 10.0]);
    assert_eq!(h.summary().excluded_out_of_range, 0);
    let id = h.id();
    let (record, _, values) = histogram_of(&fig, id);
    assert_eq!(record.total, 3.0);
    assert_eq!(values.iter().sum::<f64>(), 3.0);
}

// WHY: the turn is the turn of the axes' own angle units, so that a histogram of headings in radians, or of hours on
// a 24-hour clock, wraps at its own turn: in radians −0.1 is 2π − 0.1 and 2π + 0.2 is 0.2; on a 24-hour clock −1 is
// 23 and 25 is 1.
#[test]
fn angles_wrap_at_the_turn_of_the_axes_angle_units() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.angle_units(AngleUnits::Radians);
    let id = ax
        .polar_hist(vec![-0.1, 0.1, TAU + 0.2])
        .bins(Bins::Count(4))
        .id();
    let (record, edges, values) = histogram_of(&fig, id);
    assert_eq!(record.range, Some([0.0, TAU]));
    assert_all_close(
        &edges,
        &[0.0, PI / 2.0, PI, 3.0 * PI / 2.0, TAU],
        "quarter-turn bins in radians",
    );
    assert_all_close(&values, &[2.0, 0.0, 0.0, 1.0], "the counts in radians");

    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    ax.angle_units(AngleUnits::PerTurn(24.0));
    let id = ax
        .polar_hist(vec![25.0, -1.0, 12.5])
        .bins(Bins::Count(24))
        .id();
    let (record, _, values) = histogram_of(&fig, id);
    assert_eq!(record.range, Some([0.0, 24.0]));
    let mut expected = vec![0.0; 24];
    expected[1] = 1.0;
    expected[12] = 1.0;
    expected[23] = 1.0;
    assert_all_close(&values, &expected, "the counts on a 24-hour clock");
}

// WHY: a polar histogram is an ordinary histogram in polar axes, so its styles are those of `hist`: the step style
// stores stairs on the bin edges closed down to 0, whose runs the polar axes draw as arcs, and the record and counts
// are those of the bars it replaces.
#[test]
fn the_step_style_of_a_polar_histogram_stores_stairs_over_the_turn() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let mut h = ax.polar_hist(vec![-10.0, 10.0, 100.0, 190.0]);
    h.style(HistStyle::Step);
    let id = h.id();
    let s = stairs(&fig, id);
    assert_eq!(s.base, Some(0.0));
    let (record, edges, values) = histogram_of(&fig, id);
    assert_eq!(record.bins, Bins::Count(16));
    assert_all_close(&edges, &turn_edges(360.0, 16), "the stair edges");
    let mut expected = vec![0.0; 16];
    expected[0] = 1.0; // 10°
    expected[4] = 1.0; // 100° in [90°, 112.5°)
    expected[8] = 1.0; // 190° in [180°, 202.5°)
    expected[15] = 1.0; // −10° → 350°
    assert_all_close(&values, &expected, "the counts");
}

// ---------------------------------------------------------------------------------------------------------------
// Wind roses
// ---------------------------------------------------------------------------------------------------------------

/// Ten observations with a finite direction and speed, each therefore 10 % of the total, and two that are excluded
/// because their direction or speed is not finite.
///
/// | direction | speed | sector (centre)       | class |
/// |-----------|-------|-----------------------|-------|
/// | 355°      | 1.0   | 0 (0°)                | 0–2   |
/// | 5°        | 3.0   | 0 (0°)                | 2–4   |
/// | 0°        | 5.0   | 0 (0°)                | ≥ 4   |
/// | 360°      | 1.5   | 0 (0°)                | 0–2   |
/// | −10°      | 0.5   | 0 (0°), as 350°       | 0–2   |
/// | 90°       | 1.0   | 4 (90°)               | 0–2   |
/// | 95°       | 6.0   | 4 (90°)               | ≥ 4   |
/// | 200°      | 3.5   | 9 (202.5°)            | 2–4   |
/// | 225°      | 2.5   | 10 (225°)             | 2–4   |
/// | 270°      | 4.5   | 12 (270°)             | ≥ 4   |
fn observations() -> (Vec<f64>, Vec<f64>) {
    let rows = [
        (355.0, 1.0),
        (5.0, 3.0),
        (0.0, 5.0),
        (360.0, 1.5),
        (-10.0, 0.5),
        (90.0, 1.0),
        (95.0, 6.0),
        (200.0, 3.5),
        (225.0, 2.5),
        (270.0, 4.5),
        (f64::NAN, 1.0),
        (10.0, f64::NAN),
    ];
    rows.into_iter().unzip()
}

const SPEED_EDGES: [f64; 3] = [0.0, 2.0, 4.0];

/// The percentages expected of each class in each of the 16 sectors.
fn expected_percentages() -> [Vec<f64>; 3] {
    let mut calm = vec![0.0; 16];
    let mut moderate = vec![0.0; 16];
    let mut strong = vec![0.0; 16];
    calm[0] = 30.0;
    calm[4] = 10.0;
    moderate[0] = 10.0;
    moderate[9] = 10.0;
    moderate[10] = 10.0;
    strong[0] = 10.0;
    strong[4] = 10.0;
    strong[12] = 10.0;
    [calm, moderate, strong]
}

/// Builds the wind rose of [`observations`] and returns the figure, the axes and the bar series, one per class.
fn rose() -> (Figure, NodeId, Vec<NodeId>) {
    let (directions, speeds) = observations();
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let ids = ax.wind_rose(directions, speeds, SPEED_EDGES).ids();
    let axes_id = ax.id();
    (fig, axes_id, ids)
}

// WHY: a wind rose is read as a compass: north at the top and bearings increasing clockwise. Directions are bearings
// in degrees, so the axes keep degrees.
#[test]
fn a_wind_rose_is_drawn_in_polar_axes_with_north_up_and_clockwise_bearings() {
    let (fig, ax, _) = rose();
    let view = polar_view(&fig, ax);
    assert_eq!(view.zero_deg, 90.0);
    assert_eq!(view.angle_units, AngleUnits::Degrees);
    assert!(axes(&fig, ax).x.reverse, "bearings run clockwise");
    let report = fig.validate();
    assert!(
        report.errors.is_empty() && report.warnings.is_empty(),
        "{report:?}"
    );
}

// WHY: the sectors of a wind rose are centred on the compass points, so the sector of north spans from 348.75° to
// 11.25° across the wrap: winds from 355° and from 5° are both northerly and must be counted together, and the bars
// stand on the centres 0°, 22.5°, …, 337.5°. Binning from 0° as a histogram does would split the northerly winds
// between two sectors and leave the compass points on sector edges.
#[test]
fn sectors_are_centred_on_the_compass_points_so_north_spans_the_wrap() {
    let (fig, _, ids) = rose();
    assert_eq!(ids.len(), 3, "one series per speed class");
    let centres: Vec<f64> = (0..16).map(|k| 22.5 * k as f64).collect();
    for &id in &ids {
        assert_all_close(
            &vector(&fig, bar(&fig, id).positions),
            &centres,
            "the sector centres",
        );
    }
    let expected = expected_percentages();
    for (class, &id) in ids.iter().enumerate() {
        assert_all_close(
            &vector(&fig, bar(&fig, id).values),
            &expected[class],
            &format!("the percentages of class {class}"),
        );
    }
}

// WHY: the length of a petal is the share of the time the wind blows from that sector, so the values are percentages
// of every observation that has both a direction and a speed; observations missing either have no place in any
// sector and do not count towards the total. Over all classes and sectors they sum to 100.
#[test]
fn the_values_are_percentages_of_the_finite_observations_and_sum_to_one_hundred() {
    let (fig, _, ids) = rose();
    let total: f64 = ids
        .iter()
        .flat_map(|&id| vector(&fig, bar(&fig, id).values))
        .sum();
    assert!(
        (total - 100.0).abs() < 1e-9,
        "the percentages sum to {total}"
    );
}

// WHY: the classes of a wind rose are stacked so that each petal's full length is the share of its sector and each
// band the share of one speed class: the first class rests on the centre, and each later class on the sum of those
// before it, at every sector.
#[test]
fn the_speed_classes_are_stacked_in_order_from_the_centre() {
    let (fig, _, ids) = rose();
    let expected = expected_percentages();
    let mut below = vec![0.0; 16];
    for (class, &id) in ids.iter().enumerate() {
        let extents = bar_extents(&fig, id);
        let bases: Vec<f64> = extents.iter().map(|e: &BarExtent| e.base).collect();
        assert_all_close(&bases, &below, &format!("the bases of class {class}"));
        for (b, v) in below.iter_mut().zip(&expected[class]) {
            *b += v;
        }
    }
    assert_all_close(&below[..1], &[50.0], "the northerly petal reaches 50 %");
}

// WHY: each petal fills its sector with a small gap to its neighbours, so that adjacent petals read as separate
// sectors without leaving most of the circle empty; it is centred on its sector, so it covers the directions it counts.
#[test]
fn each_petal_is_centred_on_its_sector_and_nearly_fills_it() {
    let (fig, _, ids) = rose();
    for &id in &ids {
        for (k, e) in bar_extents(&fig, id).iter().enumerate() {
            let centre = 22.5 * k as f64;
            assert!(
                ((e.low + e.high) / 2.0 - centre).abs() < TOL,
                "petal {k} is centred on {centre}°: {e:?}"
            );
            let width = e.high - e.low;
            assert!(
                (0.75 * 22.5..22.5).contains(&width),
                "petal {k} nearly fills its 22.5° sector: {width}°"
            );
        }
    }
}

// WHY: the legend names each band by its range of speeds, built from the edges so that the user does not have to
// write them; the numbers carry no unit, which the user adds in a legend title or by relabelling. The last class has
// no upper edge, so its label must say it is open rather than invent a bound.
#[test]
fn each_class_is_labelled_by_its_range_of_speeds_and_the_last_as_open() {
    let (fig, _, ids) = rose();
    let labels: Vec<String> = ids
        .iter()
        .map(|&id| {
            bar(&fig, id)
                .label
                .as_ref()
                .expect("every class is labelled")
                .content
                .clone()
        })
        .collect();
    assert_eq!(labels[0], "0–2");
    assert_eq!(labels[1], "2–4");
    assert!(
        labels[2].contains('4') && !labels[2].contains('–'),
        "the last class is open above 4: {:?}",
        labels[2]
    );
}

// WHY: a calm has no meaningful direction, so a wind rose draws no petal for it; but it is still part of the record, so
// it counts towards the total and every petal is a share of all the observations. Leaving calms out of the total would
// overstate how often the wind blows from each sector. Here two of four observations are calm, so the two petals drawn
// are 25 % each and sum to 50.
#[test]
fn speeds_below_the_first_edge_are_calms_counted_in_the_total_but_drawn_in_no_class() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let ids = ax
        .wind_rose([0.0, 90.0, 180.0, 270.0], [0.5, 1.5, 3.0, 0.2], [1.0, 2.0])
        .ids();
    assert_eq!(ids.len(), 2, "one series per class, none for calms");
    let mut light = vec![0.0; 16];
    light[4] = 25.0; // 1.5 at 90°
    let mut strong = vec![0.0; 16];
    strong[8] = 25.0; // 3.0 at 180°
    assert_all_close(
        &vector(&fig, bar(&fig, ids[0]).values),
        &light,
        "the class 1–2",
    );
    assert_all_close(
        &vector(&fig, bar(&fig, ids[1]).values),
        &strong,
        "the class ≥ 2",
    );
}

// WHY: the classes are half-open, as the bins of a histogram are, so that every speed belongs to exactly one class:
// a speed exactly on an edge belongs to the class above it, and a speed exactly on the first edge is in the first
// class, not a calm. Counting an edge in both classes, or in neither, would make the petals sum to the wrong total.
#[test]
fn a_speed_on_an_edge_belongs_to_the_class_above_it() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let ids = ax
        .wind_rose([0.0, 90.0, 180.0, 270.0], [0.0, 2.0, 4.0, 1.0], SPEED_EDGES)
        .ids();
    let shares: Vec<Vec<f64>> = ids
        .iter()
        .map(|&id| vector(&fig, bar(&fig, id).values))
        .collect();
    let mut expected = [vec![0.0; 16], vec![0.0; 16], vec![0.0; 16]];
    expected[0][0] = 25.0; // 0.0, on the first edge
    expected[0][12] = 25.0; // 1.0
    expected[1][4] = 25.0; // 2.0, on the edge between the first two classes
    expected[2][8] = 25.0; // 4.0, on the edge of the open class
    for (class, (actual, expected)) in shares.iter().zip(&expected).enumerate() {
        assert_all_close(actual, expected, &format!("class {class}"));
    }
}
