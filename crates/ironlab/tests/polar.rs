//! Polar axes in the facade (#122): creating and converting polar axes, the view setters that
//! name the polar view in the vocabulary of polar plots, the readability aliases of the limit
//! and scale setters, `polarplot` and `polarscatter`, and the existing builders drawing into
//! polar axes unchanged.
//!
//! These tests assume this API:
//!
//! - `Figure::polar_axes(row, col) -> AxesMut`, which creates the tile's axes or converts it to
//!   `Projection::Polar` with the default `PolarView`, and turns on the grid of both axes; an
//!   existing polar axes keeps its view, as `axes3` keeps a 3D view.
//! - `AxesMut::make_polar()`, which converts a 2D axes to polar axes and keeps its artists.
//! - The view setters `theta_zero(ThetaZero)`, `clockwise(bool)`, `angle_units(AngleUnits)`,
//!   `hole(f64)` and `radial_labels_at(f64)`, which convert a 2D axes to polar axes and keep
//!   a cylindrical axes cylindrical; `thetalim`, `rlim` and `rscale`, aliases of `xlim`, `ylim`
//!   and `yscale`.
//! - `AxesMut::polarplot(theta, r)` and `AxesMut::polarscatter(theta, r)`, which make the axes
//!   polar and add the series that `plot` and `scatter` add.
//! - `ThetaZero`, `AngleUnits` and `PolarView` in the prelude.

mod common;

use common::{axes, data, series};
use ironlab::ir::{Artist, Limits, PolarView, Projection, Series, View3d};
use ironlab::prelude::*;

/// Returns the polar view of an axes, panicking if the axes is not polar.
fn polar_view(fig: &Figure, id: NodeId) -> PolarView {
    match axes(fig, id).projection {
        Projection::Polar { polar, .. } => polar,
        other => panic!("expected polar axes, found {other:?}"),
    }
}

/// Returns the series with its own identifier and data identifiers cleared, so that two series
/// built in different figures compare equal when they are built alike.
fn without_ids(series: &Series) -> Series {
    Series {
        id: NodeId::default(),
        x: NodeId::default(),
        y: NodeId::default(),
        ..series.clone()
    }
}

const THETA: [f64; 5] = [0.0, 90.0, 180.0, 270.0, 360.0];
const R: [f64; 5] = [1.0, 2.0, 3.0, 2.0, 1.0];

// WHY: `polar_axes` is the entry point of every polar figure. It must produce the IR projection
// that the compiler draws as polar axes, with the documented default view (angle 0 east,
// anticlockwise, no hole, degrees), and with both grids on, because polar axes without spokes
// and circles are unreadable; the IR default keeps grids off, so the facade must turn them on.
#[test]
fn polar_axes_creates_polar_axes_with_the_default_view_and_both_grids() {
    let mut fig = Figure::new();
    let id = fig.polar_axes(0, 0).id();
    assert_eq!(polar_view(&fig, id), PolarView::default());
    let a = axes(&fig, id);
    assert!(a.x.grid, "the angular grid (spokes) is on");
    assert!(a.y.grid, "the radial grid (circles) is on");
    assert!(!a.x.reverse, "the default direction is anticlockwise");
    assert_eq!(
        a.aspect,
        Aspect::Auto,
        "polar axes need no aspect to be round"
    );
    assert_eq!(
        a.x.limits,
        Limits::Auto,
        "the full turn is the automatic sector"
    );
}

// WHY: a polar plot often spans several tiles beside smaller Cartesian ones (a directivity pattern beside its cuts),
// so `polar_axes_span` must give a spanning tile the same polar axes and grids as `polar_axes` gives a single one.
#[test]
fn polar_axes_span_creates_polar_axes_over_several_tiles() {
    let mut fig = Figure::new().tiles(2, 2);
    let id = fig.polar_axes_span(0, 0, 2, 1).id();
    assert_eq!(polar_view(&fig, id), PolarView::default());
    let a = axes(&fig, id);
    assert!(a.x.grid && a.y.grid, "both grids are on");
    assert_eq!(
        fig.axes_span(0, 0, 2, 1).id(),
        id,
        "the spanning tile holds that axes"
    );
}

// WHY: the default view is what a user gets without asking; it is documented as angle 0 east in
// degrees with no hole and radial labels placed automatically, and the IR default is the single
// place that says so.
#[test]
fn the_default_polar_view_is_east_degrees_without_a_hole() {
    let view = PolarView::default();
    assert_eq!(view.zero_deg, 0.0);
    assert_eq!(view.hole, 0.0);
    assert_eq!(view.angle_units, AngleUnits::Degrees);
    assert_eq!(view.radial_labels_at, None);
}

// WHY: calling `polar_axes` again for the same tile returns the same axes, as `axes3` does; it
// must not reset a view that the user has already set, or the order of calls would matter.
#[test]
fn polar_axes_keeps_the_view_of_an_existing_polar_axes() {
    let mut fig = Figure::new();
    let id = fig.polar_axes(0, 0).theta_zero(ThetaZero::North).id();
    assert_eq!(fig.polar_axes(0, 0).id(), id);
    assert_eq!(polar_view(&fig, id).zero_deg, 90.0);
}

// WHY: a user who has already plotted into a tile and then asks for polar axes expects the data
// to be redrawn in polar coordinates, not discarded.
#[test]
fn polar_axes_and_make_polar_convert_a_cartesian_axes_keeping_its_artists() {
    let mut fig = Figure::new().tiles(1, 2);
    let a = fig.axes(0, 0).id();
    let line = fig.axes(0, 0).plot(THETA, R).id();
    assert_eq!(fig.polar_axes(0, 0).id(), a);
    assert!(matches!(axes(&fig, a).projection, Projection::Polar { .. }));
    assert_eq!(axes(&fig, a).artists.len(), 1);
    assert_eq!(axes(&fig, a).artists[0].id(), line);

    let b = fig.axes(0, 1).id();
    fig.axes(0, 1).scatter(THETA, R);
    fig.axes(0, 1).make_polar();
    assert_eq!(polar_view(&fig, b), PolarView::default());
    assert_eq!(axes(&fig, b).artists.len(), 1);
}

// WHY: compass names are how users of polar plots think about the zero direction (navigation and
// antenna patterns use north); each must map to its documented angle anticlockwise from east,
// and a mix-up (for example, west and east swapped) would rotate every figure by half a turn.
#[test]
fn theta_zero_maps_each_compass_point_to_degrees_anticlockwise_from_east() {
    let cases = [
        (ThetaZero::East, 0.0),
        (ThetaZero::North, 90.0),
        (ThetaZero::West, 180.0),
        (ThetaZero::South, 270.0),
        (ThetaZero::Degrees(37.5), 37.5),
    ];
    for (zero, expected) in cases {
        let mut fig = Figure::new();
        let id = fig.polar_axes(0, 0).theta_zero(zero).id();
        assert_eq!(polar_view(&fig, id).zero_deg, expected, "{zero:?}");
    }
}

// WHY: the direction of increasing angle is the x axis's `reverse` flag, not a separate
// property; `clockwise` must write that flag and be reversible.
#[test]
fn clockwise_sets_and_clears_the_reverse_flag_of_the_angle_axis() {
    let mut fig = Figure::new();
    let id = fig.polar_axes(0, 0).clockwise(true).id();
    assert!(axes(&fig, id).x.reverse);
    assert!(!axes(&fig, id).y.reverse, "the radius keeps its direction");
    fig.polar_axes(0, 0).clockwise(false);
    assert!(!axes(&fig, id).x.reverse);
}

// WHY: the remaining view setters each reach their own field of the polar view and leave the
// others alone; a setter that wrote the wrong field would be invisible in code review.
#[test]
fn view_setters_reach_their_own_fields() {
    let mut fig = Figure::new();
    let id = fig
        .polar_axes(0, 0)
        .angle_units(AngleUnits::Radians)
        .hole(0.3)
        .radial_labels_at(1.25)
        .id();
    let view = polar_view(&fig, id);
    assert_eq!(view.angle_units, AngleUnits::Radians);
    assert_eq!(view.hole, 0.3);
    assert_eq!(view.radial_labels_at, Some(1.25));
    assert_eq!(view.zero_deg, 0.0);

    fig.polar_axes(0, 0).angle_units(AngleUnits::PerTurn(12.0));
    assert_eq!(polar_view(&fig, id).angle_units, AngleUnits::PerTurn(12.0));
}

// WHY: setting a polar property on an ordinary axes is a request for polar axes; converting
// silently is friendlier than ignoring the call or leaving a 2D axes with no polar view to
// carry the setting.
#[test]
fn a_view_setter_on_a_cartesian_axes_converts_it_to_polar() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).hole(0.25).id();
    assert_eq!(polar_view(&fig, id).hole, 0.25);

    let mut fig = Figure::new();
    let id = fig.axes(0, 0).clockwise(true).id();
    assert!(matches!(
        axes(&fig, id).projection,
        Projection::Polar { .. }
    ));
    assert!(axes(&fig, id).x.reverse);
}

// WHY: cylindrical axes carry a polar view of their own; the view setters must change that view
// and must not demote the axes to 2D polar axes, which would discard the 3D camera. The facade
// has no cylindrical constructor yet, so the axes is made through the IR.
#[test]
fn view_setters_keep_a_cylindrical_axes_cylindrical() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).id();
    let view3d = View3d {
        azimuth_deg: 10.0,
        ..View3d::default()
    };
    fig.ir_mut().axes_mut(id).unwrap().projection = Projection::Cylindrical {
        view3d,
        polar: PolarView::default(),
    };

    fig.axes(0, 0).theta_zero(ThetaZero::North).hole(0.5);

    match axes(&fig, id).projection {
        Projection::Cylindrical {
            view3d: kept,
            polar,
        } => {
            assert_eq!(kept, view3d);
            assert_eq!(polar.zero_deg, 90.0);
            assert_eq!(polar.hole, 0.5);
        }
        other => panic!("expected cylindrical axes, found {other:?}"),
    }
}

// WHY: `thetalim`, `rlim` and `rscale` exist so that polar code reads in polar terms; they must
// be exact aliases of the x and y setters, because the IR stores the angle on x and the radius
// on y.
#[test]
fn thetalim_rlim_and_rscale_set_the_x_and_y_axes() {
    let mut fig = Figure::new();
    let id = fig
        .polar_axes(0, 0)
        .thetalim(-60.0, 60.0)
        .rlim(1.0, 100.0)
        .rscale(Scale::Log)
        .id();
    let a = axes(&fig, id);
    assert_eq!(
        a.x.limits,
        Limits::Manual {
            min: -60.0,
            max: 60.0
        }
    );
    assert_eq!(
        a.y.limits,
        Limits::Manual {
            min: 1.0,
            max: 100.0
        }
    );
    assert_eq!(a.y.scale, Scale::Log);
    assert_eq!(a.x.scale, Scale::Linear, "a logarithmic angle is never set");
    assert!(fig.validate().is_valid(), "a logarithmic radius is valid");
}

// WHY: `polarplot` is a convenience, not a new artist: it must add exactly the series `plot`
// adds (no interpolation, no closing point, the same defaults), so that every series setter
// and every rule about series apply unchanged in polar axes.
#[test]
fn polarplot_makes_the_axes_polar_and_adds_the_series_plot_adds() {
    let mut cartesian = Figure::new();
    let reference = cartesian.axes(0, 0).plot(THETA, R).id();

    let mut fig = Figure::new();
    let line = fig.axes(0, 0).polarplot(THETA, R).id();
    let id = fig.axes(0, 0).id();

    assert!(matches!(
        axes(&fig, id).projection,
        Projection::Polar { .. }
    ));
    let got = series(&fig, line);
    let want = series(&cartesian, reference);
    assert_eq!(without_ids(got), without_ids(want));
    assert_eq!(data(&fig, got.x), data(&cartesian, want.x));
    assert_eq!(data(&fig, got.y), data(&cartesian, want.y));
}

// WHY: as for `polarplot`, `polarscatter` must add the markers-only series `scatter` adds, so
// that colour and size by data work in polar axes through the existing series mechanisms.
#[test]
fn polarscatter_makes_the_axes_polar_and_adds_the_series_scatter_adds() {
    let mut cartesian = Figure::new();
    let reference = cartesian.axes(0, 0).scatter(THETA, R).id();

    let mut fig = Figure::new();
    let dots = fig.axes(0, 0).polarscatter(THETA, R).id();
    let id = fig.axes(0, 0).id();

    assert!(matches!(
        axes(&fig, id).projection,
        Projection::Polar { .. }
    ));
    let got = series(&fig, dots);
    let want = series(&cartesian, reference);
    assert_eq!(without_ids(got), without_ids(want));
    assert_eq!(data(&fig, got.x), data(&cartesian, want.x));
    assert_eq!(data(&fig, got.y), data(&cartesian, want.y));
}

// WHY: `polarplot` into an axes that is already polar must keep the view the user set, or a
// second call would reset a clockwise, north-up figure.
#[test]
fn polarplot_keeps_an_existing_polar_view() {
    let mut fig = Figure::new();
    let id = fig
        .polar_axes(0, 0)
        .theta_zero(ThetaZero::North)
        .clockwise(true)
        .id();
    fig.axes(0, 0).polarplot(THETA, R);
    fig.axes(0, 0).polarscatter(THETA, R);
    assert_eq!(polar_view(&fig, id).zero_deg, 90.0);
    assert!(axes(&fig, id).x.reverse);
    assert_eq!(axes(&fig, id).artists.len(), 2);
}

// WHY: the governing rule of polar axes is that no artist is duplicated: bars, histograms,
// stairs, cells, filled contours, surfaces and scalar images are built by the same builders in
// polar axes, and the resulting figure is valid without warnings (bars, stairs and cells are
// 2D artists, which polar axes accept; no aspect is set, so none is ignored).
#[test]
fn existing_builders_draw_into_polar_axes_and_validate_cleanly() {
    let theta: Vec<f64> = (0..=12).map(|i| 30.0 * i as f64).collect();
    let r: Vec<f64> = (0..5).map(|i| 0.25 * i as f64).collect();
    let field = Matrix::from_fn(r.len(), theta.len(), |row, col| {
        r[row] * (theta[col].to_radians()).cos()
    });

    let mut fig = Figure::new().tiles(2, 4);
    fig.polar_axes(0, 0)
        .bar([0.0, 90.0, 180.0, 270.0], [3.0, 1.0, 2.0, 4.0]);
    fig.polar_axes(0, 1)
        .barh([1.0, 2.0, 3.0], [120.0, 200.0, 300.0]);
    fig.polar_axes(0, 2)
        .hist([10.0, 20.0, 25.0, 100.0, 190.0, 200.0, 350.0]);
    fig.polar_axes(0, 3)
        .stairs([0.0, 90.0, 180.0, 270.0, 360.0], [1.0, 2.0, 3.0, 2.0])
        .base(0.0);
    fig.polar_axes(1, 0).contourf(&theta, &r, &field);
    let x = Matrix::from_fn(r.len(), theta.len(), |_, col| theta[col]);
    let y = Matrix::from_fn(r.len(), theta.len(), |row, _| r[row]);
    fig.polar_axes(1, 1).surface(x, y, &field);
    fig.polar_axes(1, 2)
        .scalar_image(Matrix::from_fn(4, 6, |row, col| (row * col) as f64))
        .pixel_columns(-50.0, 50.0)
        .pixel_rows(1.0, 4.0);
    fig.polar_axes(1, 2).thetalim(-60.0, 60.0).rlim(0.5, 4.5);
    fig.polar_axes(1, 3)
        .cells(Matrix::from_fn(3, 8, |row, col| (row + col) as f64));

    let report = fig.validate();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    for a in &fig.ir().axes {
        assert!(
            matches!(a.projection, Projection::Polar { .. }),
            "drawing into polar axes leaves them polar"
        );
        assert!(
            !a.artists
                .iter()
                .any(|artist| matches!(artist, Artist::Series(_))),
            "no builder converts its artist into a series to draw it in polar axes"
        );
    }
}

// WHY: users import `ironlab::prelude::*` and expect to name the polar vocabulary without
// reaching into `ironlab::ir`; this test fails to compile if a re-export is missing.
#[test]
fn the_prelude_names_the_polar_vocabulary() {
    let _zero: ThetaZero = ThetaZero::South;
    let _units: AngleUnits = AngleUnits::Degrees;
    let _view: ironlab::prelude::PolarView = PolarView::default();
}
