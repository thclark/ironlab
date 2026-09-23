use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Surface";
pub const DESCRIPTION: &str = "The Julia field drawn as a surface whose faces are coloured by height and outlined \
     with thin black edges. In the viewer, drag in rotate mode to change the azimuth and elevation of the view.";

pub fn figure() -> Figure {
    let (x, y, z) = julia_grid(41);

    let mut fig = Figure::new()
        .size_mm(160.0, 100.0)
        .title(r"Surface of $\ln(1 + |z_3|)$")
        .label("3d")
        .label("surface")
        .label("basics")
        .label("interaction")
        .parameter("kind", "surface")
        .parameter("dimensionality", "3D")
        .parameter("artists", 1)
        .parameter("data_points", 1763)
        .parameter("has_legend", false);
    let mut ax = fig.axes3(0, 0);
    ax.surf(&x, &y, &z).edge_width(0.25);
    ax.xlabel("$x$")
        .ylabel("$y$")
        .zlabel(r"$\ln(1 + |z_3|)$")
        .view(-37.5, 30.0);
    fig
}
