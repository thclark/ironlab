use ironlab::prelude::*;

pub const TITLE: &str = "Polar scatter";
pub const DESCRIPTION: &str = "The florets of a sunflower head, placed by the golden angle: floret k lies at k times \
     137.5° and at a radius proportional to the square root of k, so that the florets fill the disc evenly. Each \
     floret is a marker coloured by its age, the order in which it formed, through a continuous scale shown by the \
     colourbar beside the axes. The scatter is an ordinary series without a line, drawn in polar axes.";

/// The number of florets.
const FLORETS: usize = 400;

/// The golden angle in degrees, which places each floret as far as possible from those before it.
const GOLDEN_ANGLE_DEG: f64 = 137.507_764;

pub fn figure() -> Figure {
    let age: Vec<f64> = (0..FLORETS).map(|k| k as f64).collect();
    let theta: Vec<f64> = age
        .iter()
        .map(|k| (k * GOLDEN_ANGLE_DEG).rem_euclid(360.0))
        .collect();
    let r: Vec<f64> = age.iter().map(|k| k.sqrt()).collect();

    let mut fig = Figure::new()
        .size_mm(130.0, 105.0)
        .tiles(1, 2)
        .fit_col(1)
        .title("Florets of a sunflower head")
        .tag("polar")
        .tag("scatter")
        .tag("color-scale")
        .tag("colorbar")
        .parameter("dimensionality", "2D")
        .parameter("artists", 1)
        .parameter("data_points", 1200)
        .parameter("has_legend", false);
    let scale = fig
        .color_scale()
        .colormap(Colormap::Viridis)
        .title("Floret number")
        .id();
    let mut ax = fig.polar_axes(0, 0);
    ax.polarscatter(&theta, &r)
        .color_data(&age)
        .color_scale(scale)
        .marker(Marker::Circle)
        .marker_size(4.0)
        .filled();
    ax.rlim(0.0, 20.0);
    fig.panel(0, 1).colorbar(scale);
    fig
}
