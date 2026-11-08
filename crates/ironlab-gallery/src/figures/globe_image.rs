use ironlab::prelude::*;

pub const TITLE: &str = "Globe in spherical axes";
pub const DESCRIPTION: &str = "An imaginary planet, its surface generated from a few smooth bumps on the sphere \
     rather than taken from any map, stored as an equirectangular true-colour raster of 180 by 90 pixels, 2° each, \
     and drawn on the X–Y plane of spherical axes at radius 1, where the plane of azimuth and elevation wraps the \
     sphere. The image is drawn through a lattice whose nodes lie on the pixel edges and are refined along both \
     angles, each node carrying its own depth, so the far side is hidden by the near side. A great-circle route \
     between two points is a series at radius 1.01, sampled every degree of its arc and joined by straight chords \
     just above the surface.";

/// The width and height of the raster, in pixels.
const COLUMNS: usize = 180;
const ROWS: usize = 90;

/// The centres of the continents, as longitude and latitude in degrees, with their angular radius and height.
const CONTINENTS: [(f64, f64, f64, f64); 6] = [
    (20.0, 15.0, 32.0, 1.0),
    (75.0, 45.0, 28.0, 0.9),
    (130.0, -20.0, 22.0, 0.8),
    (-70.0, 25.0, 26.0, 1.0),
    (-50.0, -25.0, 20.0, 0.7),
    (170.0, 50.0, 14.0, 0.6),
];

/// The unit vector of a direction given by longitude and latitude in degrees.
fn direction(lon: f64, lat: f64) -> [f64; 3] {
    let (lon, lat) = (lon.to_radians(), lat.to_radians());
    [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()]
}

/// The height of the land above the sea at a longitude and latitude, negative under the sea.
fn height(lon: f64, lat: f64) -> f64 {
    let d = direction(lon, lat);
    let bumps: f64 = CONTINENTS
        .iter()
        .map(|&(clon, clat, radius, peak)| {
            let c = direction(clon, clat);
            let cosine = (d[0] * c[0] + d[1] * c[1] + d[2] * c[2]).clamp(-1.0, 1.0);
            let angle = cosine.acos().to_degrees();
            peak * (-(angle / radius).powi(2)).exp()
        })
        .sum();
    let ripple = 0.12
        * (3.0 * lon.to_radians() + 2.0 * lat.to_radians()).sin()
        * (5.0 * lat.to_radians() - lon.to_radians()).cos();
    bumps + ripple - 0.45
}

/// The colour of the surface at a longitude and latitude: ice near the poles, land from green to brown by height,
/// and sea from pale to deep blue by depth.
fn colour(lon: f64, lat: f64) -> Color {
    let h = height(lon, lat);
    if lat.abs() > 72.0 {
        return Color::rgb(0.93, 0.95, 0.97);
    }
    if h > 0.0 {
        let t = (h / 0.6).min(1.0) as f32;
        Color::rgb(0.25 + 0.4 * t, 0.55 - 0.15 * t, 0.2 + 0.05 * t)
    } else {
        let t = (-h / 0.45).min(1.0) as f32;
        Color::rgb(0.25 - 0.2 * t, 0.5 - 0.3 * t, 0.8 - 0.25 * t)
    }
}

/// The points of the great circle from one longitude and latitude to another, every degree of its arc, as unwrapped
/// longitudes and latitudes.
fn great_circle(from: (f64, f64), to: (f64, f64)) -> (Vec<f64>, Vec<f64>) {
    let (a, b) = (direction(from.0, from.1), direction(to.0, to.1));
    let angle = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0).acos();
    let steps = angle.to_degrees().ceil() as usize;
    let (mut lon, mut lat): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        let (wa, wb) = (
            ((1.0 - t) * angle).sin() / angle.sin(),
            (t * angle).sin() / angle.sin(),
        );
        let p: Vec<f64> = (0..3).map(|i| wa * a[i] + wb * b[i]).collect();
        let mut longitude = p[1].atan2(p[0]).to_degrees();
        if let Some(&last) = lon.last() {
            longitude += 360.0 * ((last - longitude) / 360.0).round();
        }
        lon.push(longitude);
        lat.push(p[2].clamp(-1.0, 1.0).asin().to_degrees());
    }
    (lon, lat)
}

pub fn figure() -> Figure {
    let pixel = 360.0 / COLUMNS as f64;
    let longitude = |col: usize| (col as f64 + 0.5) * pixel;
    let latitude = |row: usize| -90.0 + (row as f64 + 0.5) * pixel;
    let surface = Pixels::rgb_from_fn(ROWS, COLUMNS, |row, col| colour(longitude(col), latitude(row)));
    let (route_lon, route_lat) = great_circle((10.0, 20.0), (125.0, 40.0));
    let route_radius = vec![1.01; route_lon.len()];

    let mut fig = Figure::new()
        .size_mm(130.0, 120.0)
        .title("A route across an imaginary planet")
        .tag("image")
        .tag("line")
        .tag("polar")
        .tag("spherical")
        .tag("globe")
        .tag("depth")
        .parameter("dimensionality", "3D")
        .parameter("artists", 2)
        .parameter("data_points", 48888)
        .parameter("has_legend", false);
    let mut ax = fig.spherical_axes(0, 0);
    ax.image(&surface)
        .pixel_columns(longitude(0), longitude(COLUMNS - 1))
        .pixel_rows(latitude(0), latitude(ROWS - 1))
        .plane(ImagePlane::Xy { z: Some(1.0) });
    ax.plot3(&route_lon, &route_lat, &route_radius)
        .color(Color::rgb(0.85, 0.1, 0.1))
        .line_width(1.5);
    ax.zlim(0.0, 1.0)
        .ztick_labels_hidden()
        .xlabel("Longitude")
        .ylabel("Latitude")
        .view(160.0, 25.0);
    fig
}
