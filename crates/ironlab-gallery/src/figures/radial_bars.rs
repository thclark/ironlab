use ironlab::prelude::*;

pub const TITLE: &str = "Radial and polar bars";
pub const DESCRIPTION: &str = "Bars in polar axes, drawn by the same bar builder as in Cartesian axes. On the left, \
     horizontal bars make a radial bar chart: each category is a ring, and its bar sweeps clockwise from the north \
     through its value, a share of the turn. On the right, floating bars show the range of the monthly mean daily \
     temperature of a temperate city, one wedge per month from the mean minimum to the mean maximum. Each bar starts \
     at its own base, and the radius starts at −5 °C rather than zero, with a hole at the centre, so the bars stand \
     clear of the middle. A bar is a shape that represents its value, so its ends are arcs at the radii of its base \
     and top and its sides are radial.";

/// The categories of the radial bar chart and the share of the turn each has reached, in percent.
const TASKS: [(&str, f64); 5] = [
    ("Design", 72.0),
    ("Build", 60.0),
    ("Test", 46.0),
    ("Document", 31.0),
    ("Release", 18.0),
];

/// The mean daily minimum temperature of each month, in degrees Celsius, from January.
const LOW_C: [f64; 12] = [
    2.4, 2.2, 3.8, 5.6, 8.6, 11.6, 13.7, 13.5, 11.2, 8.3, 5.0, 2.9,
];

/// The mean daily maximum temperature of each month, in degrees Celsius, from January.
const HIGH_C: [f64; 12] = [
    8.1, 8.6, 11.3, 14.2, 17.6, 20.7, 23.0, 22.6, 19.6, 15.6, 11.3, 8.6,
];

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub fn figure() -> Figure {
    let mut fig = Figure::new()
        .size_mm(170.0, 90.0)
        .tiles(1, 2)
        .title("Bars in polar axes")
        .tag("polar")
        .tag("bar")
        .tag("subplots")
        .parameter("dimensionality", "2D")
        .parameter("artists", 2)
        .parameter("data_points", 46)
        .parameter("has_legend", false);

    let rings: Vec<f64> = (1..=TASKS.len()).map(|k| k as f64).collect();
    let shares: Vec<f64> = TASKS.iter().map(|(_, share)| *share).collect();
    let mut ax = fig.polar_axes(0, 0);
    ax.angle_units(AngleUnits::PerTurn(100.0))
        .theta_zero(ThetaZero::North)
        .clockwise(true);
    ax.barh(&rings, &shares).width(0.7);
    ax.rlim(0.0, TASKS.len() as f64 + 0.5)
        .yticks(&rings)
        .ytick_labels(TASKS.iter().map(|(name, _)| *name))
        .xticks([0.0, 25.0, 50.0, 75.0])
        .xtick_labels(["0%", "25%", "50%", "75%"])
        .radial_axis_angle(94.0)
        .title("Progress by task");

    let months: Vec<f64> = (0..12).map(|k| k as f64).collect();
    let mut ax = fig.polar_axes(0, 1);
    ax.angle_units(AngleUnits::PerTurn(12.0))
        .theta_zero(ThetaZero::North)
        .clockwise(true)
        .hole(0.25);
    let spans: Vec<f64> = HIGH_C.iter().zip(LOW_C).map(|(h, l)| h - l).collect();
    ax.bar(&months, spans).base_data(LOW_C.to_vec()).width(0.7);
    ax.rlim(-5.0, 25.0)
        .xticks(&months)
        .xtick_labels(MONTHS)
        .ytick_step(10.0)
        .title("Monthly temperature range (°C)");
    fig
}
