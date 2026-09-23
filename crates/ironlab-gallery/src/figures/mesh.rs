use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Mesh";
pub const DESCRIPTION: &str = "The Julia field drawn as a wireframe whose edges are coloured by height. The faces are \
     filled with the background colour, so that they hide the parts of the mesh behind them.";

pub fn figure() -> Figure {
    let (x, y, z) = julia_grid(31);

    let mut fig = Figure::new()
        .size_mm(160.0, 100.0)
        .title(r"Mesh of $\ln(1 + |z_3|)$")
        .label("3d")
        .label("surface")
        .label("colormap")
        .label("depth")
        .label("wireframe")
        .parameter("kind", "surface")
        .parameter("dimensionality", "3D")
        .parameter("artists", 1)
        .parameter("data_points", 1023)
        .parameter("has_legend", false);
    let mut ax = fig.axes3(0, 0);
    ax.mesh(&x, &y, &z).edge_width(0.6);
    ax.colormap(Colormap::Plasma)
        .xlabel("$x$")
        .ylabel("$y$")
        .zlabel(r"$\ln(1 + |z_3|)$")
        .view(-50.0, 35.0);
    fig
}
