//! Series in the facade (#19, #44): the MATLAB-flavoured entry points that remain, the
//! defaults each sets on the one `Series` artist, and the one handle that configures it.
//!
//! These tests assume that every point-plotting call (`plot`, `plot3`, `loglog`,
//! `semilogx`, `semilogy`, `scatter`, `scatter3`, `hline` and `vline`) returns one handle,
//! `SeriesMut`, exported in the prelude, and that `LineMut` and `ScatterMut` are gone. The
//! handle keeps the setters of both old handles that still have a meaning (`label`,
//! `color`, `line_width`, `dash`, `marker`, `marker_size`, `marker_face`, `marker_edge`,
//! `filled`, `x_coords`, `y_coords`, `clip`, and the getters `id`, `x_data`, `y_data` and
//! `z_data`), and adds:
//!
//! - `color_data(values)`, which stores the values (n per point, or n − 1 per segment),
//!   names the default colour scale of the axes when the series names none, creating it if
//!   the axes has none, and sets the line colour to `Colormapped`, so that the line, and the
//!   automatic marker colours that follow it, are coloured by the data;
//! - `size_data(values)`, which stores one marker diameter in points per point;
//! - `color_scale(id)`, which names another scale, as on every other colour-mapped handle.
//!
//! Replacing size or colour data releases the array it replaced when nothing else uses it.
//! `color` sets the line colour, which is the colour of the series: automatic marker
//! colours follow it, as they followed a line's colour before. Unlike the old scatter's
//! `color`, it does not release colour data, so that a series can have a fixed line with
//! colour-mapped markers. Granular setters remain; there is no whole-style setter such as
//! `line(LineStyle)`.

mod common;

use common::{data, is_3d, parent, series};
use ironlab::ir::{self, ColorSpec, DashStyle, LineStyle, MarkerShape, MarkerStyle, NdArray};
use ironlab::prelude::*;
use ironlab::{IssueKind, SeriesMut};

/// The series an entry point would make from its defaults alone, with the data and
/// identifier of `actual`, so that two series can be compared on their style.
fn restyled(actual: &ir::Series, line: LineStyle, marker: MarkerStyle) -> ir::Series {
    ir::Series {
        id: actual.id,
        x: actual.x,
        y: actual.y,
        z: actual.z,
        line,
        marker,
        ..ir::Series::default()
    }
}

/// The style that `scatter` sets: circles without a line.
fn scatter_style() -> (LineStyle, MarkerStyle) {
    (
        LineStyle {
            dash: DashStyle::None,
            ..LineStyle::default()
        },
        MarkerStyle {
            shape: MarkerShape::Circle,
            ..MarkerStyle::default()
        },
    )
}

// PROMISE: every plot-family entry point remains and makes a series with the defaults of `plot` (a solid automatic line without markers, data or scale), so existing code draws what it drew.
#[test]
fn the_plot_family_makes_series_with_the_defaults_of_plot() {
    let mut fig = Figure::new();
    let mut ids = Vec::new();
    {
        let mut ax = fig.axes(0, 0);
        ids.push(("plot", ax.plot([1.0, 2.0], [3.0, 4.0]).id()));
        ids.push(("loglog", ax.loglog([1.0, 2.0], [3.0, 4.0]).id()));
        ids.push(("semilogx", ax.semilogx([1.0, 2.0], [3.0, 4.0]).id()));
        ids.push(("semilogy", ax.semilogy([1.0, 2.0], [3.0, 4.0]).id()));
    }
    let deep = fig
        .axes(0, 0)
        .plot3([1.0, 2.0], [3.0, 4.0], [5.0, 6.0])
        .id();
    ids.push(("plot3", deep));
    for (name, id) in ids {
        let s = series(&fig, id);
        assert_eq!(
            *s,
            restyled(s, LineStyle::default(), MarkerStyle::default()),
            "{name}"
        );
        assert_eq!(data(&fig, s.x), &NdArray::vector(vec![1.0, 2.0]), "{name}");
        assert_eq!(data(&fig, s.y), &NdArray::vector(vec![3.0, 4.0]), "{name}");
    }
    let s = series(&fig, deep);
    assert_eq!(
        data(&fig, s.z.expect("plot3 stores z")),
        &NdArray::vector(vec![5.0, 6.0])
    );
    assert!(is_3d(parent(&fig, deep)));
    assert!(fig.validate().is_valid());
}

// PROMISE: `scatter` and `scatter3` remain and make a series of unfilled automatic circles without a line, which is MATLAB's scatter.
#[test]
fn the_scatter_family_makes_series_of_circles_without_a_line() {
    let mut fig = Figure::new();
    let flat = fig.axes(0, 0).scatter([1.0, 2.0], [3.0, 4.0]).id();
    let deep = fig
        .axes(0, 0)
        .scatter3([1.0, 2.0], [3.0, 4.0], [5.0, 6.0])
        .id();
    let (line, marker) = scatter_style();
    for (name, id) in [("scatter", flat), ("scatter3", deep)] {
        let s = series(&fig, id);
        assert_eq!(*s, restyled(s, line, marker), "{name}");
        assert_eq!(s.marker.face, ColorSpec::None, "{name}: unfilled");
        assert_eq!(s.marker.edge, ColorSpec::Auto, "{name}: automatic edge");
        assert_eq!(s.line.color, ColorSpec::Auto, "{name}: automatic colour");
    }
    assert!(is_3d(parent(&fig, deep)));
    assert!(fig.validate().is_valid());
}

// PROMISE: plot and scatter differ only in their defaults, so a scatter joined by a line and a plot given the same markers are the same series, and a user can move continuously between the two.
#[test]
fn a_scatter_joined_by_a_line_is_the_same_series_as_a_plot_with_markers() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let joined = ax.scatter([0.0, 1.0], [1.0, 0.0]).dash(Dash::Solid).id();
    let marked = ax.plot([0.0, 1.0], [1.0, 0.0]).marker(Marker::Circle).id();
    let (a, b) = (series(&fig, joined), series(&fig, marked));
    assert_eq!(a.line, b.line);
    assert_eq!(a.marker, b.marker);
    assert_eq!((a.size_data, a.color_data), (b.size_data, b.color_data));
}

// PROMISE: each setter of the one handle writes its own field of the series, so that the merged handle cannot silently write the wrong field (for example the marker face writing the edge).
#[test]
fn the_setters_of_a_series_map_to_its_fields() {
    let red = Color::rgb(0.8, 0.1, 0.1);
    let blue = Color::rgb(0.0, 0.2, 0.9);
    let mut fig = Figure::new();
    let (id, x_data, y_data) = {
        let mut ax = fig.axes(0, 0);
        let mut handle: SeriesMut<'_> = ax.plot([0.0, 1.0], [0.0, 1.0]);
        handle
            .label("measured")
            .color(red)
            .line_width(1.75)
            .dash(Dash::Dashed)
            .marker(Marker::TriangleUp)
            .marker_size(6.5)
            .marker_face(Color::WHITE)
            .marker_edge(blue)
            .x_coords(Coords::AxesFraction)
            .clip(false);
        assert_eq!(handle.z_data(), None);
        (handle.id(), handle.x_data(), handle.y_data())
    };
    let s = series(&fig, id);
    assert_eq!((s.x, s.y), (x_data, y_data));
    assert_eq!(s.label, Some(Text::new("measured")));
    assert_eq!(s.line.color, ColorSpec::Rgba { color: red });
    assert_eq!(s.line.width_pt, 1.75);
    assert_eq!(s.line.dash, DashStyle::Dashed);
    assert_eq!(s.marker.shape, MarkerShape::TriangleUp);
    assert_eq!(s.marker.size_pt, 6.5);
    assert_eq!(
        s.marker.face,
        ColorSpec::Rgba {
            color: Color::WHITE
        }
    );
    assert_eq!(s.marker.edge, ColorSpec::Rgba { color: blue });
    assert_eq!(s.x_coords, Coords::AxesFraction);
    assert_eq!(s.y_coords, Coords::Data);
    assert!(!s.clip);

    let filled = fig.axes(0, 0).scatter([0.0], [0.0]).filled().id();
    assert_eq!(series(&fig, filled).marker.face, ColorSpec::Auto);
}

// PROMISE: colour data is the one way to colour a series by a third quantity: it stores the values, names the default scale of the axes, and colours the line and the markers that follow it, giving a valid figure with no further call.
#[test]
fn colour_data_stores_the_values_names_the_default_scale_and_colours_the_series() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .scatter([0.0, 1.0, 2.0], [0.0, 1.0, 2.0])
        .color_data([-1.0, 0.0, 1.0])
        .id();
    let default = fig.axes(0, 0).color_scale().id();
    let s = series(&fig, id);
    let colours = s.color_data.expect("the series has colour data");
    assert_eq!(data(&fig, colours), &NdArray::vector(vec![-1.0, 0.0, 1.0]));
    assert_eq!(s.color_scale, Some(default));
    assert_eq!(s.line.color, ColorSpec::Colormapped);
    assert_eq!(
        s.marker.edge,
        ColorSpec::Auto,
        "the edge follows the colour of the series"
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: size data gives each marker its own diameter and leaves the marker size alone, because the marker size remains the size of the legend key.
#[test]
fn size_data_stores_one_diameter_per_point_and_keeps_the_marker_size() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .scatter([0.0, 1.0, 2.0], [0.0, 1.0, 2.0])
        .marker_size(5.0)
        .size_data([2.0, 4.0, 8.0])
        .id();
    let s = series(&fig, id);
    let sizes = s.size_data.expect("the series has size data");
    assert_eq!(data(&fig, sizes), &NdArray::vector(vec![2.0, 4.0, 8.0]));
    assert_eq!(s.marker.size_pt, 5.0);
    assert!(fig.validate().is_valid());
}

// PROMISE: a fixed colour set after colour data colours the line and keeps the colour data and its scale, so that a series can have a fixed line with colour-mapped markers; only `color_data` gives or replaces the values.
#[test]
fn a_fixed_colour_set_after_colour_data_keeps_the_colour_data() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .plot([0.0, 1.0, 2.0], [0.0, 1.0, 0.0])
        .marker(Marker::Circle)
        .color_data([0.0, 1.0, 2.0])
        .color(Color::BLACK)
        .marker_face(ColorSpec::Colormapped)
        .id();
    let default = fig.axes(0, 0).color_scale().id();
    let s = series(&fig, id);
    assert_eq!(
        s.line.color,
        ColorSpec::Rgba {
            color: Color::BLACK
        }
    );
    assert_eq!(s.marker.face, ColorSpec::Colormapped);
    let colours = s.color_data.expect("the colour data is kept");
    assert_eq!(data(&fig, colours), &NdArray::vector(vec![0.0, 1.0, 2.0]));
    assert_eq!(s.color_scale, Some(default), "the scale is kept");
    assert_eq!(fig.ir().color_scales.len(), 1, "no second scale is created");
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: replacing size or colour data leaves no orphaned array behind, so the figure carries no unused data and no warning about it.
#[test]
fn replacing_size_or_colour_data_releases_the_array_it_replaced() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .scatter([0.0, 1.0], [0.0, 1.0])
        .color_data([0.0, 1.0])
        .color_data([5.0, 6.0])
        .size_data([1.0, 2.0])
        .size_data([3.0, 4.0])
        .id();
    let s = series(&fig, id);
    assert_eq!(
        data(&fig, s.color_data.unwrap()),
        &NdArray::vector(vec![5.0, 6.0])
    );
    assert_eq!(
        data(&fig, s.size_data.unwrap()),
        &NdArray::vector(vec![3.0, 4.0])
    );
    assert_eq!(
        fig.ir().data.len(),
        4,
        "x, y, and the last colour and size data"
    );
    assert!(
        fig.validate()
            .warnings
            .iter()
            .all(|w| w.kind != IssueKind::UnusedData)
    );
}

// PROMISE: colour data given by identifier is shared with the series that holds it, as every other data argument is, so a series coloured by another's y values does not copy them.
#[test]
fn colour_data_given_by_identifier_shares_the_array() {
    let mut fig = Figure::new();
    let shared = fig
        .axes(0, 0)
        .plot([0.0, 1.0, 2.0], [3.0, 1.0, 2.0])
        .y_data();
    let id = fig
        .axes(0, 0)
        .scatter([0.0, 1.0, 2.0], [0.0, 0.0, 0.0])
        .color_data(shared)
        .id();
    assert_eq!(series(&fig, id).color_data, Some(shared));
    assert_eq!(
        fig.ir().data.len(),
        4,
        "x and y of each series, nothing more"
    );
}

// PROMISE: a series names any scale by `color_scale`, in place of the default its colour data named, and a default the call created is deleted when nothing else names it.
#[test]
fn a_series_names_another_scale_in_place_of_the_default() {
    let mut fig = Figure::new();
    let chosen = fig.indexed_scale([Color::BLACK, Color::WHITE]).id();
    let id = fig
        .axes(0, 0)
        .scatter([0.0, 1.0], [0.0, 1.0])
        .color_data([0.0, 1.0])
        .color_scale(chosen)
        .id();
    assert_eq!(series(&fig, id).color_scale, Some(chosen));
    assert_eq!(
        fig.ir().color_scales.len(),
        1,
        "the default the colour data created is deleted"
    );
    assert!(fig.validate().is_valid());
}

// PROMISE: reference lines remain series spanning the plot area in axes-fraction coordinates, in a fixed dark grey that takes no colour from the axes colour order.
#[test]
fn reference_lines_are_series_in_axes_fraction_coordinates_with_a_fixed_colour() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let h = ax.hline(2.0).id();
    let v = ax.vline(3.0).id();
    for (name, id, fraction) in [("hline", h, (true, false)), ("vline", v, (false, true))] {
        let s = series(&fig, id);
        assert_eq!(
            (
                s.x_coords == Coords::AxesFraction,
                s.y_coords == Coords::AxesFraction
            ),
            fraction,
            "{name}"
        );
        assert!(
            matches!(s.line.color, ColorSpec::Rgba { .. }),
            "{name}: a fixed colour"
        );
        assert_eq!(s.line.dash, DashStyle::Solid, "{name}");
        assert_eq!(s.marker.shape, MarkerShape::None, "{name}");
        assert_eq!(s.color_data, None, "{name}");
    }
    assert!(fig.validate().is_valid());
}

// PROMISE: a figure of series saved and reloaded, or wrapped with `from_ir`, is the same figure, and keeps adding colour-mapped series to the same default scale.
#[test]
fn series_survive_from_ir_and_a_round_trip_through_both_formats() {
    let mut fig = Figure::new();
    {
        let mut ax = fig.axes(0, 0);
        ax.plot([0.0, 1.0, 2.0], [0.0, 1.0, 0.0])
            .marker(Marker::Square)
            .color_data([0.0, 0.5])
            .label("per segment");
        ax.scatter([0.0, 1.0, 2.0], [1.0, 0.0, 1.0])
            .size_data([3.0, 6.0, 9.0])
            .color_data([2.0, 1.0, 0.0])
            .filled();
    }
    let scale = fig.axes(0, 0).color_scale().id();
    let saved = fig.into_ir();
    assert!(saved.validate().is_valid(), "{:?}", saved.validate());
    for (name, restored) in [
        ("from_ir", saved.clone()),
        ("JSON", ir::Figure::from_json(&saved.to_json()).unwrap()),
        (
            "protobuf",
            ir::Figure::from_protobuf(&saved.to_protobuf()).unwrap(),
        ),
    ] {
        assert_eq!(restored, saved, "{name}");
        let mut wrapped = Figure::from_ir(restored);
        let later = wrapped
            .axes(0, 0)
            .scatter([0.0], [0.0])
            .color_data([1.0])
            .id();
        assert_eq!(series(&wrapped, later).color_scale, Some(scale), "{name}");
        assert_eq!(wrapped.ir().color_scales.len(), 1, "{name}");
    }
}
