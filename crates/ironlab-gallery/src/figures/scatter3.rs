use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Three-dimensional scatter";
pub const DESCRIPTION: &str = "Points of a sunflower spiral lifted to the height of the Julia field and coloured by \
     that height, drawn as filled markers in three dimensions.";

pub fn figure() -> Figure {
    let (x, y) = sunflower(300, 1.5);
    let z: Vec<f64> = x.iter().zip(&y).map(|(&x, &y)| julia_field(x, y)).collect();

    let mut fig = Figure::new()
        .size_mm(160.0, 100.0)
        .title("Julia field sampled at scattered points");
    let mut ax = fig.axes3(0, 0);
    ax.scatter3(&x, &y, &z).colors(&z).size(3.0).filled();
    ax.xlabel("$x$")
        .ylabel("$y$")
        .zlabel(r"$\ln(1 + |z_3|)$")
        .grid(true);
    fig
}
