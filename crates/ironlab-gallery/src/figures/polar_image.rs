use ironlab::prelude::*;

pub const TITLE: &str = "Sonar sweep in a polar sector";
pub const DESCRIPTION: &str = "The echo strength of a forward-looking sonar, recorded by 60 beams across a sector \
     from 60° to port to 60° to starboard and in 100 range bins out to 100 m, drawn as a scalar image in polar axes \
     whose angular limits are that sector, with its colour scale shown by the colourbar beside it. Each pixel is the annular sector between its beam edges and its range \
     edges, so the image fans out from the transducer at the centre; it is drawn through a mesh whose nodes lie on \
     the pixel edges and follow the arcs. Angle 0 points up the page, and angles increase clockwise, to starboard.";

/// The number of beams across the sector.
const BEAMS: usize = 60;

/// The number of range bins.
const BINS: usize = 100;

/// The echo strength, in decibels, of the beam at `bearing` degrees and the range bin at `range` metres: a sloping
/// seabed beyond 55 m, two targets, and the speckle of the water column.
fn echo_db(bearing: f64, range: f64) -> f64 {
    let seabed_range = 55.0 + 0.15 * bearing;
    let seabed = 30.0 * (-((range - seabed_range) / 4.0).powi(2)).exp()
        + if range > seabed_range { 18.0 } else { 0.0 };
    let target = |b: f64, r: f64, size: f64| {
        size * (-((bearing - b) / 2.5).powi(2) - ((range - r) / 1.5).powi(2)).exp()
    };
    let speckle = 4.0 * ((bearing * 1.7 + range * 3.1).sin() * (bearing * 0.9 - range * 2.3).cos());
    -40.0 + seabed + target(-20.0, 30.0, 32.0) + target(25.0, 42.0, 26.0) + speckle
}

pub fn figure() -> Figure {
    let beam_width = 120.0 / BEAMS as f64;
    let bin_length = 100.0 / BINS as f64;
    let bearing = |col: usize| -60.0 + (col as f64 + 0.5) * beam_width;
    let range = |row: usize| (row as f64 + 0.5) * bin_length;
    let echo = Matrix::from_fn(BINS, BEAMS, |row, col| echo_db(bearing(col), range(row)));

    let mut fig = Figure::new()
        .size_mm(150.0, 85.0)
        .tiles(1, 2)
        .fit_col(1)
        .title("Forward-looking sonar")
        .tag("polar")
        .tag("image")
        .tag("color-scale")
        .tag("colorbar")
        .parameter("dimensionality", "2D")
        .parameter("artists", 1)
        .parameter("data_points", 6000)
        .parameter("has_legend", false);
    let strength = fig
        .color_scale()
        .colormap(Colormap::Inferno)
        .limits(-45.0, 0.0)
        .title("Echo (dB)")
        .id();
    let mut ax = fig.polar_axes(0, 0);
    ax.scalar_image(&echo)
        .pixel_columns(bearing(0), bearing(BEAMS - 1))
        .pixel_rows(range(0), range(BINS - 1))
        .color_scale(strength);
    ax.theta_zero(ThetaZero::North)
        .clockwise(true)
        .thetalim(-60.0, 60.0)
        .rlim(0.0, 100.0)
        .xtick_step(30.0)
        .ylabel("Range (m)");
    fig.panel(0, 1).colorbar(strength);
    fig
}
