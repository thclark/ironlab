use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Mapped image";
pub const DESCRIPTION: &str = "The Julia field drawn as a colour-mapped image with the magma colormap, beneath black \
     isolines of the same field. The values inside a disc are set to NaN, and a pixel whose value is not finite is \
     transparent by default, so the axes background shows through the disc.";

/// The centre of the disc within which the field is undefined.
const HOLE_CENTRE: (f64, f64) = (-0.6, 0.6);

/// The radius of the disc within which the field is undefined.
const HOLE_RADIUS: f64 = 0.35;

pub fn figure() -> Figure {
    let (x, y, z) = julia_grid(241);
    // The values inside the disc are undefined, as the readings under a dead region of a sensor would be.
    let masked = Matrix::from_fn(y.len(), x.len(), |row, col| {
        let (dx, dy) = (x[col] - HOLE_CENTRE.0, y[row] - HOLE_CENTRE.1);
        if dx.hypot(dy) < HOLE_RADIUS {
            f64::NAN
        } else {
            z[(row, col)]
        }
    });

    let mut fig = Figure::new()
        .size_mm(120.0, 100.0)
        .title(r"$\ln(1 + |z_3|)$ as a mapped image");
    let mut ax = fig.axes(0, 0);
    ax.mapped_image(&masked)
        .pixel_columns(DOMAIN_MIN, DOMAIN_MAX)
        .pixel_rows(DOMAIN_MIN, DOMAIN_MAX);
    ax.contour(&x, &y, &z)
        .levels(12)
        .color(Color::BLACK)
        .line_width(0.5);
    ax.colormap(Colormap::Magma).xlabel("$x$").ylabel("$y$");
    fig
}
