use ironlab::prelude::*;

pub const TITLE: &str = "Antenna directivity";
pub const DESCRIPTION: &str = "The radiation pattern of a broadside array of four patch elements half a wavelength \
     apart, in its two principal planes. The axes are polar, with angle 0 at the north and angles increasing \
     clockwise, as antenna patterns are drawn. The radius is the gain relative to the peak, in decibels, on a linear \
     axis from −30 dB at the centre to 0 dB at the rim, so that the main lobe, the side lobes and the nulls between \
     them can all be read. Each cut is a line through the gain sampled every degree, joined by straight chords.";

/// The number of elements of the array.
const ELEMENTS: f64 = 4.0;

/// The lowest gain drawn, in decibels: the radial lower limit at the centre.
const FLOOR_DB: f64 = -30.0;

/// The pattern of one patch element at `theta` radians from boresight: strongest forwards and falling to nothing
/// behind the ground plane.
fn element(theta: f64) -> f64 {
    ((1.0 + theta.cos()) / 2.0).powi(2)
}

/// The array factor of the elements, spaced half a wavelength apart across the cut, at `theta` radians from
/// boresight, normalised to 1 at its peak.
fn array_factor(theta: f64) -> f64 {
    let psi = std::f64::consts::PI * theta.sin();
    if psi.abs() < 1e-12 {
        return 1.0;
    }
    ((ELEMENTS * psi / 2.0).sin() / (ELEMENTS * (psi / 2.0).sin())).abs()
}

/// Returns a gain as a power ratio in decibels, held at the floor.
fn decibels(amplitude: f64) -> f64 {
    (20.0 * amplitude.max(1e-12).log10()).max(FLOOR_DB)
}

pub fn figure() -> Figure {
    let theta = linspace(0.0, 360.0, 361);
    let e_plane: Vec<f64> = theta
        .iter()
        .map(|t| decibels(element(t.to_radians()) * array_factor(t.to_radians())))
        .collect();
    let h_plane: Vec<f64> = theta
        .iter()
        .map(|t| decibels(element(t.to_radians()).powf(1.5)))
        .collect();

    let mut fig = Figure::new()
        .size_mm(120.0, 110.0)
        .title("Directivity of a four-element patch array")
        .tag("polar")
        .tag("line")
        .tag("legend")
        .tag("antenna")
        .parameter("dimensionality", "2D")
        .parameter("artists", 2)
        .parameter("data_points", 1444)
        .parameter("has_legend", true);
    let mut ax = fig.polar_axes(0, 0);
    ax.theta_zero(ThetaZero::North)
        .clockwise(true)
        .rlim(FLOOR_DB, 0.0)
        .ytick_step(10.0)
        .radial_labels_at(215.0);
    ax.polarplot(&theta, &e_plane)
        .line_width(1.25)
        .label("E-plane (array axis)");
    ax.polarplot(&theta, &h_plane)
        .line_width(1.25)
        .dash(Dash::Dashed)
        .label("H-plane");
    ax.legend(LegendLocation::SouthEast);
    fig
}
