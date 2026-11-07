use ironlab::prelude::*;

pub const TITLE: &str = "Radial and polar bars";
pub const DESCRIPTION: &str = "Bars in polar axes, drawn by the same bar builder as in Cartesian axes. On the left, \
     horizontal bars make a radial bar chart: each category is a ring, and its bar sweeps clockwise from the north \
     through its value, a share of the turn. On the right, upright bars make a polar bar chart of the monthly \
     rainfall of a temperate city, one wedge per month, with a hole at the centre so that the short wedges of the \
     dry months stay readable. A bar is a shape that represents its value, so its top is an arc at the radius of its \
     end and its sides are radial.";

/// The categories of the radial bar chart and the share of the turn each has reached, in percent.
const TASKS: [(&str, f64); 5] = [
    ("Design", 72.0),
    ("Build", 60.0),
    ("Test", 46.0),
    ("Document", 31.0),
    ("Release", 18.0),
];

/// The mean rainfall of each month, in millimetres, from January.
const RAINFALL_MM: [f64; 12] = [
    84.0, 61.0, 64.0, 57.0, 58.0, 55.0, 49.0, 63.0, 68.0, 92.0, 95.0, 90.0,
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
        .parameter("data_points", 34)
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
        .radial_labels_at(94.0)
        .title("Progress by task");

    let months: Vec<f64> = (0..12).map(|k| k as f64).collect();
    let mut ax = fig.polar_axes(0, 1);
    ax.angle_units(AngleUnits::PerTurn(12.0))
        .theta_zero(ThetaZero::North)
        .clockwise(true)
        .hole(0.25);
    ax.bar(&months, RAINFALL_MM).width(0.7);
    ax.rlim(0.0, 100.0)
        .xticks(&months)
        .xtick_labels(MONTHS)
        .ytick_step(25.0)
        .radial_labels_at(0.5)
        .title("Monthly rainfall (mm)");
    fig
}
