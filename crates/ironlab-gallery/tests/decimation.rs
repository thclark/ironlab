//! Tests that the decimation entry still demonstrates what its page claims.
//!
//! WHY: this entry is the only one whose subject is a property of the renderer rather than of the API. Nothing in its
//! source says that the record is thinned, and the other gallery tests check only that a figure builds and validates,
//! so shrinking the record, widening the window or giving the two panels different data would leave the page claiming
//! something the figure no longer shows. These tests compile the figure and inspect the points that were actually
//! drawn, which is the only way to see the property at all.

use ironlab::ir::{Artist, Axes, Figure, Limits, NodeId};
use ironlab_gallery::find;
use ironlab_text::TextEngine;

const SLUG: &str = "decimated_timeseries";

fn built() -> Figure {
    (find(SLUG)
        .expect("the decimation entry is registered")
        .build)()
    .into_ir()
}

/// Returns the axes in the tile row `row`.
fn panel(figure: &Figure, row: u32) -> &Axes {
    figure
        .axes
        .iter()
        .find(|axes| axes.cell.row == row)
        .unwrap_or_else(|| panic!("{SLUG} has an axes in tile row {row}"))
}

/// Returns the x and y data identifiers of the single line of an axes.
fn line_data(axes: &Axes) -> (ironlab::ir::DataId, ironlab::ir::DataId) {
    match axes.artists.as_slice() {
        [Artist::Line(line)] => (line.x, line.y),
        artists => panic!(
            "expected one line in the axes, found {} artists",
            artists.len()
        ),
    }
}

/// Returns the values of a data array of the figure.
fn values(figure: &Figure, id: ironlab::ir::DataId) -> &[f64] {
    &figure
        .data
        .get(&id)
        .expect("the array is in the figure")
        .values
}

/// Returns the source indices of the points drawn for `axes`, which the hit map publishes for picking.
fn drawn_indices(figure: &Figure, axes: NodeId) -> Vec<usize> {
    let scene = ironlab_scene::compile(figure, &TextEngine::new());
    scene
        .hit_map
        .artists
        .iter()
        .filter(|artist| artist.axes == axes)
        .flat_map(|artist| artist.samples.iter().map(|sample| sample.source_index))
        .collect()
}

/// WHY: the page tells the reader that the lower panel shows the same record as the upper one, so what the window
/// adds must be detail rather than different data. The facade stores a fresh array for each call that plots one, so
/// the two panels hold separate copies and the check is on the values they hold.
#[test]
fn both_panels_plot_the_same_record() {
    let figure = built();
    let (overview_x, overview_y) = line_data(panel(&figure, 0));
    let (window_x, window_y) = line_data(panel(&figure, 1));
    assert_eq!(values(&figure, overview_x), values(&figure, window_x));
    assert_eq!(values(&figure, overview_y), values(&figure, window_y));
}

/// WHY: the entry demonstrates thinning, which only happens when a series holds more points than its plot can
/// resolve. A record shortened to a few thousand samples would be drawn in full, and the figure would silently become
/// an ordinary line plot while every other test kept passing.
#[test]
fn the_record_holds_far_more_samples_than_either_panel_draws() {
    let figure = built();
    let (x, _) = line_data(panel(&figure, 0));
    let samples = values(&figure, x).len();
    assert!(
        samples >= 50_000,
        "the record holds {samples} samples, too few to need thinning"
    );
    for row in 0..2 {
        let drawn = drawn_indices(&figure, panel(&figure, row).id).len();
        assert!(
            drawn * 10 < samples,
            "panel {row} drew {drawn} of {samples} samples, so the record is barely thinned"
        );
    }
}

/// WHY: this is the claim the page makes, and the reason the entry is a pair of panels rather than one. The window
/// must draw the samples inside it far more densely than the overview does, because that difference is the detail a
/// reader sees when they narrow the view. A change that stopped thinning from following the view would leave both
/// panels drawing the same handful of points in the window and the figure would show nothing.
#[test]
fn the_window_draws_the_samples_inside_it_far_more_densely_than_the_overview() {
    let figure = built();
    let window = panel(&figure, 1);
    let Limits::Manual { min, max } = window.x.limits else {
        panic!("the lower panel must fix its x limits to a window of the record");
    };
    let (x, _) = line_data(window);
    let times = values(&figure, x);
    let inside = |indices: Vec<usize>| {
        indices
            .into_iter()
            .filter(|&i| (min..=max).contains(&times[i]))
            .count()
    };

    let in_overview = inside(drawn_indices(&figure, panel(&figure, 0).id));
    let in_window = inside(drawn_indices(&figure, window.id));

    assert!(
        in_window > 20 * in_overview.max(1),
        "the window drew {in_window} of the samples it covers and the overview drew {in_overview}, so narrowing the \
         view reveals little"
    );
}

/// WHY: the window is worth showing only if the record has structure inside it that the overview cannot resolve. A
/// record smooth at the scale of the window would zoom into a straight line, demonstrating the opposite of the point,
/// so the values inside the window must change direction many times.
#[test]
fn the_record_has_structure_inside_the_window_that_the_overview_cannot_resolve() {
    let figure = built();
    let window = panel(&figure, 1);
    let Limits::Manual { min, max } = window.x.limits else {
        panic!("the lower panel must fix its x limits to a window of the record");
    };
    let (x, y) = line_data(window);
    let (times, values_in) = (values(&figure, x), values(&figure, y));
    let inside: Vec<f64> = (0..times.len())
        .filter(|&i| (min..=max).contains(&times[i]))
        .map(|i| values_in[i])
        .collect();

    let turning_points = inside
        .windows(3)
        .filter(|w| (w[1] - w[0]).signum() != (w[2] - w[1]).signum())
        .count();
    assert!(
        turning_points > 50,
        "the window holds {turning_points} turning points, too few for zooming into it to reveal anything"
    );
}
