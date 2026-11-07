use ironlab::prelude::*;

pub const TITLE: &str = "Logarithmic radius";
pub const DESCRIPTION: &str = "The intensity of light diffracted by a single slit eight wavelengths wide, against \
     the angle from the axis of the slit, drawn in polar axes with a logarithmic radius. The intensity falls by four \
     decades from the central maximum to the faintest side lobes drawn, so on a linear radius every side lobe would \
     be a speck at the centre; on a logarithmic radius each decade takes an equal step from the lower limit at the \
     centre to the rim. The radial axis is a ruler along the right half of the diameter, labelled below it, so that \
     its labels lie clear of the pattern.";

/// The width of the slit, in wavelengths.
const WIDTH: f64 = 8.0;

/// The lowest intensity drawn, relative to the central maximum: the radial lower limit.
const FLOOR: f64 = 1e-4;

/// The intensity at `theta` radians from the axis, relative to the central maximum.
fn intensity(theta: f64) -> f64 {
    let beta = std::f64::consts::PI * WIDTH * theta.sin();
    let sinc = if beta.abs() < 1e-12 {
        1.0
    } else {
        beta.sin() / beta
    };
    (sinc * sinc).max(FLOOR)
}

pub fn figure() -> Figure {
    let theta = linspace(-90.0, 90.0, 721);
    let relative: Vec<f64> = theta.iter().map(|t| intensity(t.to_radians())).collect();

    let mut fig = Figure::new()
        .size_mm(120.0, 80.0)
        .title("Single-slit diffraction")
        .tag("polar")
        .tag("line")
        .tag("log")
        .tag("physics")
        .parameter("dimensionality", "2D")
        .parameter("artists", 1)
        .parameter("data_points", 1442)
        .parameter("has_legend", false);
    let mut ax = fig.polar_axes(0, 0);
    ax.theta_zero(ThetaZero::North)
        .clockwise(true)
        .thetalim(-90.0, 90.0)
        .rscale(Scale::Log)
        .rlim(FLOOR, 1.0)
        .xtick_step(30.0)
        .radial_axis(RadialAxis::AlongSpoke {
            angle: Some(90.0),
            side: RadialAxisSpokeSide::Clockwise,
        })
        .ylabel("Relative intensity");
    ax.polarplot(&theta, &relative).line_width(1.0);
    fig
}
