use ironlab::prelude::*;

pub const TITLE: &str = "Membrane mode in cylindrical axes";
pub const DESCRIPTION: &str = "The (2, 1) vibration mode of a clamped circular drum skin, whose displacement is \
     J₂(k r) cos 2θ with k chosen so that the rim, at radius 1, is a node. The surface is given on its natural grid \
     of angle and radius, 49 angles by 21 radii, and drawn in cylindrical axes, where x is the angle about the z \
     axis, y the distance from it and z the height. Each face is the quadrilateral of its four grid nodes, mapped \
     through the axes and joined by straight edges, so a finer grid follows the disc more closely. The floor of the \
     axes carries the polar frame of the disc, and the z axis stands up its centre.";

/// The first zero of the Bessel function J₂, which puts the clamped rim on a node of the mode.
const FIRST_ZERO_J2: f64 = 5.135_622_301_840_683;

/// The Bessel function of the first kind of order 2, summed from its power series, which converges quickly over
/// the range used here.
fn bessel_j2(x: f64) -> f64 {
    let half = x / 2.0;
    let mut term = half * half / 2.0;
    let mut sum = term;
    for k in 1..30 {
        let k = f64::from(k);
        term *= -half * half / (k * (k + 2.0));
        sum += term;
    }
    sum
}

pub fn figure() -> Figure {
    let theta = linspace(0.0, 360.0, 49);
    let radius = linspace(0.0, 1.0, 21);
    let displacement = Matrix::from_fn(radius.len(), theta.len(), |row, col| {
        bessel_j2(FIRST_ZERO_J2 * radius[row]) * (2.0 * theta[col].to_radians()).cos()
    });

    let mut fig = Figure::new()
        .size_mm(150.0, 110.0)
        .title("The (2, 1) mode of a circular membrane")
        .tag("surface")
        .tag("polar")
        .tag("cylindrical")
        .parameter("dimensionality", "3D")
        .parameter("artists", 1)
        .parameter("data_points", 1099)
        .parameter("has_legend", false);
    let mut ax = fig.cylindrical_axes(0, 0);
    ax.surf(&theta, &radius, &displacement).edge_width(0.25);
    ax.ylim(0.0, 1.0)
        .ytick_step(0.5)
        .zlim(-0.5, 0.5)
        .ztick_step(0.25)
        .xlabel(r"$\theta$")
        .ylabel("$r$")
        .zlabel("Displacement")
        .view(-30.0, 35.0);
    fig
}
