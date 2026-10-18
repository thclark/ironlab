use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Measured speed faded by its confidence";
pub const DESCRIPTION: &str = "The speed of the flow past a cylinder as a particle image velocimetry measurement \
     might report it, with a validation confidence for every vector: low next to the wall, where reflections spoil \
     the particle images, and lower still in the shadow that the cylinder casts in the laser sheet, which enters from \
     above. On the left the confidence is the alpha of a colour-mapped image, so that each pixel fades as far as its \
     vector is doubtful, and the inside of the cylinder, where nothing was measured, is transparent. On the right \
     the same speed is a surface over its own contours: the confidence is the alpha of the faces, and a face opacity \
     below one lets the contours on the floor show through the whole surface. Alpha has the shape of the data and \
     varies over it; opacity is one number for the whole artist, which the viewer's property editor can change \
     without regenerating any data.";

/// The number of pixels of the image along x and along y.
const NX: usize = 160;
const NY: usize = 100;

/// The extent of the measurement along x and along y, in cylinder radii.
const X_RANGE: (f64, f64) = (-3.0, 5.0);
const Y_RANGE: (f64, f64) = (-2.5, 2.5);

/// The number of rings and spokes of the polar mesh of the surface, and the radius of its outermost ring.
const RINGS: usize = 13;
const SPOKES: usize = 49;
const OUTER_RADIUS: f64 = 3.0;

/// The validation confidence of a vector measured at `(x, y)`, from 0 (worthless) to 1 (certain), or NaN inside
/// the cylinder, where there is no fluid and so no vector.
///
/// Reflections from the wall lower the confidence within a few tenths of a radius of it, and the cylinder shadows
/// the laser sheet beneath it, where the particles are lit only by scattered light.
fn confidence(x: f64, y: f64) -> f64 {
    let r = x.hypot(y);
    if r < 1.0 {
        return f64::NAN;
    }
    let reflection = 1.0 - 0.7 * (-(r - 1.0) / 0.25).exp();
    let across = ((1.05 - x.abs()) / 0.1).clamp(0.0, 1.0);
    let below = ((-y) / 0.3).clamp(0.0, 1.0);
    let shadow = 1.0 - 0.8 * across * below;
    reflection * shadow
}

/// The speed of the flow at `(x, y)` in units of the free-stream speed, or NaN inside the cylinder.
fn speed(x: f64, y: f64) -> f64 {
    cylinder_flow_speed(x.hypot(y), y.atan2(x))
}

pub fn figure() -> Figure {
    let pitch_x = (X_RANGE.1 - X_RANGE.0) / NX as f64;
    let pitch_y = (Y_RANGE.1 - Y_RANGE.0) / NY as f64;
    let xs = linspace(X_RANGE.0 + pitch_x / 2.0, X_RANGE.1 - pitch_x / 2.0, NX);
    let ys = linspace(Y_RANGE.0 + pitch_y / 2.0, Y_RANGE.1 - pitch_y / 2.0, NY);
    let measured = Matrix::from_fn(NY, NX, |row, col| speed(xs[col], ys[row]));
    let trust = Matrix::from_fn(NY, NX, |row, col| confidence(xs[col], ys[row]));

    let (mesh_x, mesh_y, mesh_speed) = cylinder_flow_mesh(RINGS, SPOKES, OUTER_RADIUS);
    let mesh_trust = Matrix::from_fn(RINGS, SPOKES, |row, col| {
        confidence(mesh_x[(row, col)], mesh_y[(row, col)])
    });

    let mut fig = Figure::new()
        .size_mm(170.0, 80.0)
        .tiles(1, 2)
        .title(r"Speed $|\mathbf{u}| / U_\infty$ faded by the validation confidence of each vector")
        .label("image")
        .label("surface")
        .label("contour")
        .label("line")
        .label("subplots")
        .label("transparency")
        .parameter("dimensionality", "3D")
        .parameter("artists", 4)
        .parameter("data_points", 36_701)
        .parameter("has_legend", false);

    let mut ax = fig.axes(0, 0);
    // The confidence is the alpha of every pixel: a pixel of confidence 0.3 is drawn at 30 % of its colour over the
    // white of the axes. The NaN confidence inside the cylinder makes those pixels transparent, as their NaN speed
    // already does.
    ax.mapped_image(&measured)
        .pixel_columns(xs[0], xs[NX - 1])
        .pixel_rows(ys[0], ys[NY - 1])
        .alpha(&trust);
    let circle: Vec<f64> = linspace(0.0, std::f64::consts::TAU, 121);
    ax.plot(
        circle.iter().map(|t| t.cos()).collect::<Vec<_>>(),
        circle.iter().map(|t| t.sin()).collect::<Vec<_>>(),
    )
    .color(Color::BLACK)
    .line_width(0.75);
    ax.aspect(Aspect::Equal)
        .clim(0.0, 2.0)
        .xlabel("$x / a$")
        .ylabel("$y / a$")
        .title("Alpha from the confidence");

    let mut ax = fig.axes3(0, 1);
    // The contours lie on the floor of the box, beneath the surface of the same speed.
    ax.contour(mesh_x.clone(), mesh_y.clone(), &mesh_speed)
        .levels(12)
        .line_width(0.75);
    // The faces take the confidence at their corners as their alpha, and the face opacity multiplies it, so that even
    // the most trusted faces let the contours beneath them show; the mesh lines are fainter still.
    ax.surf(mesh_x, mesh_y, &mesh_speed)
        .face_alpha(&mesh_trust)
        .face_opacity(0.8)
        .edge_opacity(0.25)
        .edge_width(0.25);
    ax.clim(0.0, 2.0)
        .zlim(0.0, 2.0)
        .xlabel("$x / a$")
        .ylabel("$y / a$")
        .zlabel(r"$|\mathbf{u}| / U_\infty$")
        .title("Alpha and opacity on a surface");
    fig
}
