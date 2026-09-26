use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Filled contour";
pub const DESCRIPTION: &str = "Bands between explicitly chosen levels of the Julia field, filled with colours from \
     the magma colormap.";

pub fn figure() -> Figure {
    let (x, y, z) = julia_grid(121);
    let levels = linspace(0.0, 6.5, 14);

    let mut fig = Figure::new()
        .size_mm(120.0, 100.0)
        .title(r"Filled contours of $\ln(1 + |z_3|)$")
        .label("contour")
        .label("colormap")
        .label("levels")
        .parameter("dimensionality", "2D")
        .parameter("artists", 1)
        .parameter("data_points", 14_883)
        .parameter("has_legend", false);
    let mut ax = fig.axes(0, 0);
    ax.contourf(&x, &y, &z).level_values(&levels);
    ax.colormap(Colormap::Magma).xlabel("$x$").ylabel("$y$");
    fig
}
