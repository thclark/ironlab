use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Three-dimensional contour";
pub const DESCRIPTION: &str = "Isolines of the Julia field drawn in three dimensions, each at the height of its own \
     level, so that they trace the shape of the surface.";

pub fn figure() -> Figure {
    let (x, y, z) = julia_grid(81);

    let mut fig = Figure::new()
        .size_mm(160.0, 100.0)
        .title(r"Isolines of $\ln(1 + |z_3|)$ at their levels")
        .label("3d")
        .label("contour")
        .label("colormap")
        .label("levels");
    let mut ax = fig.axes3(0, 0);
    ax.contour3(&x, &y, &z).levels(20).line_width(1.0);
    ax.colormap(Colormap::Cividis)
        .xlabel("$x$")
        .ylabel("$y$")
        .zlabel(r"$\ln(1 + |z_3|)$")
        .grid(true);
    fig
}
