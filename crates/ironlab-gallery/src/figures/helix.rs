use ironlab::prelude::*;

pub const TITLE: &str = "Helix in cylindrical axes";
pub const DESCRIPTION: &str = "A helix of radius 1 that climbs one unit for every turn, three turns in all, drawn \
     with `plot3` in cylindrical axes, where x is the angle about the z axis, y the distance from it and z the \
     height. A series joins the points it is given with straight lines in every axes, so the helix is drawn by \
     supplying a point every 5°. Arrows from every 45° show the tangent: each runs from its point to the point 30° \
     further round and one twelfth of a unit higher, and is the straight chord between them, so it points along the \
     helix rather than round the cylinder.";

/// The number of turns of the helix.
const TURNS: f64 = 3.0;

/// The angle, in degrees, over which each tangent arrow runs.
const ARROW_SWEEP: f64 = 30.0;

pub fn figure() -> Figure {
    let theta = linspace(0.0, 360.0 * TURNS, 217);
    let radius = vec![1.0; theta.len()];
    let height: Vec<f64> = theta.iter().map(|t| t / 360.0).collect();

    let bases = linspace(0.0, 360.0 * TURNS - 45.0, 24);
    let ones = vec![1.0; bases.len()];
    let base_height: Vec<f64> = bases.iter().map(|t| t / 360.0).collect();
    let sweep = vec![ARROW_SWEEP; bases.len()];
    let outward = vec![0.0; bases.len()];
    let climb = vec![ARROW_SWEEP / 360.0; bases.len()];

    let mut fig = Figure::new()
        .size_mm(130.0, 120.0)
        .title("A helix and its tangents")
        .tag("line")
        .tag("quiver")
        .tag("polar")
        .tag("cylindrical")
        .tag("helix")
        .parameter("dimensionality", "3D")
        .parameter("artists", 2)
        .parameter("data_points", 795)
        .parameter("has_legend", false);
    let mut ax = fig.cylindrical_axes(0, 0);
    ax.plot3(&theta, &radius, &height).line_width(1.25);
    ax.quiver3(&bases, &ones, &base_height, &sweep, &outward, &climb)
        .no_scale()
        .colour(Colour::BLACK)
        .line_width(0.75);
    ax.ylim(0.0, 1.5)
        .ytick_step(0.5)
        .zlim(0.0, 3.0)
        .ztick_step(1.0)
        .zlabel("Height")
        .view(-37.5, 25.0);
    fig
}
