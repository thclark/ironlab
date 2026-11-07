use ironlab::prelude::*;

pub const TITLE: &str = "Antenna balloon in spherical axes";
pub const DESCRIPTION: &str = "The three-dimensional radiation pattern of a planar array of two by four patch \
     elements half a wavelength apart, facing +X, drawn as a balloon whose distance from the centre in each direction \
     is the gain in that direction, in decibels above a floor 30 dB below the peak. The surface is given on a grid of \
     azimuth and elevation every 5° and drawn with `surf` in spherical axes, where x is the azimuth, y the elevation \
     above the X–Y plane and z the distance from the centre, and its faces are coloured by the gain through the \
     colour scale shown by the colourbar. The equator and the meridian of azimuth 0 frame the sphere and carry the \
     angles.";

/// The lowest gain drawn, in decibels relative to the peak: the centre of the balloon.
const FLOOR_DB: f64 = -30.0;

/// The array factor of `n` elements half a wavelength apart along a direction whose cosine with the array axis is
/// `c`, normalised to 1 at its peak.
fn array_factor(n: f64, c: f64) -> f64 {
    let psi = std::f64::consts::PI * c;
    if (psi / 2.0).sin().abs() < 1e-12 {
        return 1.0;
    }
    ((n * psi / 2.0).sin() / (n * (psi / 2.0).sin())).abs()
}

/// The gain, in decibels relative to the peak and held at the floor, in the direction of azimuth `az` and elevation
/// `el` degrees.
fn gain_db(az: f64, el: f64) -> f64 {
    let (az, el) = (az.to_radians(), el.to_radians());
    let d = [el.cos() * az.cos(), el.cos() * az.sin(), el.sin()];
    let element = ((1.0 + d[0]) / 2.0).powi(2);
    let amplitude = element * array_factor(2.0, d[1]) * array_factor(4.0, d[2]);
    (20.0 * amplitude.max(1e-12).log10()).max(FLOOR_DB)
}

pub fn figure() -> Figure {
    let azimuth = linspace(0.0, 360.0, 73);
    let elevation = linspace(-90.0, 90.0, 37);
    let gain = Matrix::from_fn(elevation.len(), azimuth.len(), |row, col| {
        gain_db(azimuth[col], elevation[row])
    });
    let radius = Matrix::from_fn(elevation.len(), azimuth.len(), |row, col| {
        gain_db(azimuth[col], elevation[row]) - FLOOR_DB
    });

    let mut fig = Figure::new()
        .size_mm(160.0, 120.0)
        .tiles(1, 2)
        .fit_col(1)
        .title("Gain of a two-by-four patch array")
        .tag("surface")
        .tag("color-scale")
        .tag("colorbar")
        .tag("polar")
        .tag("spherical")
        .tag("antenna")
        .parameter("dimensionality", "3D")
        .parameter("artists", 1)
        .parameter("data_points", 5512)
        .parameter("has_legend", false);
    let scale = fig
        .color_scale()
        .colormap(Colormap::Viridis)
        .limits(FLOOR_DB, 0.0)
        .title("Gain (dB)")
        .id();
    let mut ax = fig.spherical_axes(0, 0);
    ax.surf(&azimuth, &elevation, &radius)
        .color_data(&gain)
        .color_scale(scale)
        .edge_width(0.2);
    ax.zlim(0.0, 30.0)
        .ztick_step(10.0)
        .xlabel("Azimuth")
        .ylabel("Elevation")
        .view(-50.0, 25.0);
    fig.panel(0, 1).colorbar(scale);
    fig
}
