use ironlab::prelude::*;

pub const TITLE: &str = "Fields on an angle–radius grid";
pub const DESCRIPTION: &str = "The vertical displacement of a clamped circular membrane vibrating in a mode with two \
     nodal diameters and one nodal circle, sampled every 7.5° and every twentieth of the radius. On the left the \
     field is drawn as filled contours and on the right as a surface seen from above (a pseudocolour plot), both in \
     polar axes. Both artists draw their data directly: the corners of each face and the crossings of each contour \
     level are placed at their angles and radii and joined by straight chords, so the grid is fine enough to make \
     the chords follow the circles. The grid repeats its first angle at 360° so that the field closes on itself.";

/// The radius of the second zero of the Bessel function of order 2, at which the mode has its nodal circle, scaled
/// so that the membrane is clamped at radius 1.
const NODAL_CIRCLE: f64 = 5.135_622 / 8.417_244;

/// The displacement of the mode at angle `theta` degrees and radius `r`, a fraction of the radius of the membrane.
///
/// The radial shape is a polynomial with the zeros of the true Bessel shape at the nodal circle and at the clamped
/// edge, which is close enough to it for a picture.
fn displacement(theta: f64, r: f64) -> f64 {
    let radial = r * r * (r - NODAL_CIRCLE) * (1.0 - r) * 12.0;
    radial * (2.0 * theta.to_radians()).cos()
}

pub fn figure() -> Figure {
    let theta = linspace(0.0, 360.0, 49);
    let r = linspace(0.0, 1.0, 21);
    let field = Matrix::from_fn(r.len(), theta.len(), |row, col| displacement(theta[col], r[row]));

    let mut fig = Figure::new()
        .size_mm(170.0, 90.0)
        .tiles(1, 2)
        .title("Vibration mode (2, 2) of a circular membrane")
        .tag("polar")
        .tag("contour")
        .tag("surface")
        .tag("subplots")
        .parameter("dimensionality", "2D")
        .parameter("artists", 2)
        .parameter("data_points", 4186)
        .parameter("has_legend", false);
    let scale = fig
        .colour_scale()
        .colourmap(Colourmap::Coolwarm)
        .limits(-0.3, 0.3)
        .id();

    let mut ax = fig.polar_axes(0, 0);
    ax.contourf(&theta, &r, &field)
        .level_values(linspace(-0.3, 0.3, 13))
        .colour_scale(scale);
    ax.rlim(0.0, 1.0).title("Filled contours");

    let x = Matrix::from_fn(r.len(), theta.len(), |_, col| theta[col]);
    let y = Matrix::from_fn(r.len(), theta.len(), |row, _| r[row]);
    let mut ax = fig.polar_axes(0, 1);
    ax.surface(x, y, &field)
        .edge_colour(None)
        .colour_scale(scale);
    ax.rlim(0.0, 1.0).title("Surface seen from above");
    fig
}
