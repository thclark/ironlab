use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Images in three dimensions";
pub const DESCRIPTION: &str = "The Julia field as a colour-mapped image on the floor of a three-dimensional axes, \
     beneath a surface of the same field, and the magnitude of the field's x derivative as a colour-mapped image on \
     the wall behind the surface. An image is planar rather than two-dimensional: it lies in one of the three planes \
     of the axes, at a chosen offset along the third axis, and its pixels are placed along the two axes of that \
     plane.";

pub fn figure() -> Figure {
    let (x, y, z) = julia_grid(81);
    let (dzdx, _) = gradient(&x, &y, &z);
    let (first_x, last_x) = (x[0], x[x.len() - 1]);
    let (first_y, last_y) = (y[0], y[y.len() - 1]);
    let (z_min, z_max) = z
        .values()
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(*v), hi.max(*v))
        });
    // The columns of an image in the yz plane run along y and its rows along z, so the slope is transposed, and the
    // x coordinate of each of its values is laid along the height of the wall.
    let slope = Matrix::from_fn(x.len(), y.len(), |row, col| dzdx[(col, row)].abs());
    // Pixels extend half a pitch beyond their centres, so the centres of the first and last rows are placed half a
    // pitch inside the range of heights of the surface: the edges of the wall image then span exactly that range,
    // and the automatic z limits are those the surface alone would take.
    let pitch = (z_max - z_min) / slope.rows() as f64;

    let mut fig = Figure::new()
        .size_mm(160.0, 100.0)
        .title(r"$\ln(1 + |z_3|)$ on the floor, $|\partial_x \ln(1 + |z_3|)|$ on the wall");
    let mut ax = fig.axes3(0, 0);
    ax.mapped_image(&z)
        .pixel_columns(first_x, last_x)
        .pixel_rows(first_y, last_y);
    ax.surf(&x, &y, &z).edge_color(None);
    // A plane without an offset lies at the low end of its third axis, so the floor image is at the bottom of the z
    // axis and the wall image at the left end of the x axis. One pair of colour limits serves every artist of the
    // axes, so the limits are fixed to the range of the field and the few slopes steeper than its largest value take
    // the last colour of the colormap.
    ax.mapped_image(&slope)
        .plane(ImagePlane::Yz { x: None })
        .pixel_columns(first_y, last_y)
        .pixel_rows(z_min + pitch / 2.0, z_max - pitch / 2.0)
        .above(OutOfRange::Clamp);
    // An image on a face of the box is painted behind everything inside the box when the view puts that face at the
    // back, so the view is taken from an azimuth of 37.5°, which turns the x = min wall away from the viewer; from
    // the default azimuth of −37.5° that wall faces the viewer, and the wall image would cover the surface.
    ax.clim(0.0, z_max)
        .view(37.5, 30.0)
        .xlabel("$x$")
        .ylabel("$y$")
        .zlabel(r"$\ln(1 + |z_3|)$");
    fig
}
