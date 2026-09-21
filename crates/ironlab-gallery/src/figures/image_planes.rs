use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Images in three dimensions";
pub const DESCRIPTION: &str = "The Julia field as a colour-mapped image on the floor of a three-dimensional axes, \
     beneath a surface of the same field, and the magnitude of the field's x derivative as a colour-mapped image on \
     the lower part of the far wall. An image is planar rather than two-dimensional: it lies in one of the three \
     planes of the axes, at a chosen offset along the third axis, and its pixels are placed along the two axes of \
     that plane.";

/// The bottom of the z axis, well below the field, so that the floor and the wall band lie beneath the surface.
const Z_BOTTOM: f64 = -4.0;

/// The top of the wall band, at the foot of the range of heights of the surface.
const BAND_TOP: f64 = 0.0;

/// The top of the z axis.
const Z_TOP: f64 = 8.0;

pub fn figure() -> Figure {
    let (x, y, z) = julia_grid(81);
    let (dzdx, _) = gradient(&x, &y, &z);
    let (first_x, last_x) = (x[0], x[x.len() - 1]);
    let (first_y, last_y) = (y[0], y[y.len() - 1]);
    let z_max = z.values().iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // The columns of an image in the yz plane run along y and its rows along z, so the slope is transposed, and the
    // x coordinate of each of its values is laid along the height of the band.
    let slope = Matrix::from_fn(x.len(), y.len(), |row, col| dzdx[(col, row)].abs());
    // Pixels extend half a pitch beyond their centres, so the centres of the first and last rows of the band are
    // placed half a pitch inside its edges, which then lie exactly at the bottom of the axes and the top of the band.
    let pitch = (BAND_TOP - Z_BOTTOM) / slope.rows() as f64;

    let mut fig = Figure::new()
        .size_mm(160.0, 100.0)
        .title(r"$\ln(1 + |z_3|)$ on the floor, $|\partial_x \ln(1 + |z_3|)|$ on the wall");
    let mut ax = fig.axes3(0, 0);
    ax.mapped_image(&z)
        .pixel_columns(first_x, last_x)
        .pixel_rows(first_y, last_y);
    ax.surf(&x, &y, &z).edge_color(None);
    // A plane without an offset lies at the low end of its third axis, so the band is on the wall at the left end of
    // the x axis, which the view from an azimuth of 37.5° puts behind the surface. One pair of colour limits serves
    // every artist of the axes, so the limits are fixed to the range of the field and the few slopes steeper than
    // its largest value take the last colour of the colormap.
    ax.mapped_image(&slope)
        .plane(ImagePlane::Yz { x: None })
        .pixel_columns(first_y, last_y)
        .pixel_rows(Z_BOTTOM + pitch / 2.0, BAND_TOP - pitch / 2.0)
        .above(OutOfRange::Clamp);
    ax.clim(0.0, z_max)
        .zlim(Z_BOTTOM, Z_TOP)
        .view(37.5, 30.0)
        .xlabel("$x$")
        .ylabel("$y$")
        .zlabel(r"$\ln(1 + |z_3|)$");
    fig
}
