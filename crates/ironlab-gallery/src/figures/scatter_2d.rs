use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Scatter";
pub const DESCRIPTION: &str = "Points of a sunflower spiral drawn as filled markers, each coloured by the value of \
     the Julia field at its position and sized by its distance from the centre.";

pub fn figure() -> Figure {
    let (x, y) = sunflower(400, 1.5);
    let field_value: Vec<f64> = x.iter().zip(&y).map(|(&x, &y)| julia_field(x, y)).collect();
    // Markers grow from 2 pt at the centre to 5 pt at the rim.
    let marker_size: Vec<f64> = x.iter().zip(&y).map(|(&x, &y)| 2.0 + 2.0 * x.hypot(y)).collect();

    let mut fig = Figure::new()
        .size_mm(120.0, 100.0)
        .title("Julia field sampled on a sunflower spiral")
        .label("2d")
        .label("scatter")
        .label("basics")
        .label("colormap")
        .label("markers");
    let mut ax = fig.axes(0, 0);
    ax.scatter(&x, &y)
        .colors(&field_value)
        .sizes(&marker_size)
        .marker(Marker::Circle)
        .filled();
    ax.colormap(Colormap::Plasma)
        .xlabel("$x$")
        .ylabel("$y$")
        .xlim(-1.6, 1.6)
        .ylim(-1.6, 1.6);
    fig
}
