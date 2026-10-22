use ironlab::prelude::*;

pub const TITLE: &str = "Markers over a surface";
pub const DESCRIPTION: &str = "A synthetic hill drawn as a surface coloured by its elevation, with the survey points \
     measured on it drawn as markers of two kinds: points measured by GNSS and points measured by total station. \
     One legend panel beside the plot keys both: the two kinds of marker as ordinary entries, and the colour scale of \
     the surface as a bar item, a vertical colourbar five entries long that lies in the column of the entries and is \
     titled and ticked as a colourbar of the scale is.";

/// The number of grid nodes along each side of the surface.
const NODES: usize = 41;

/// Returns the elevation, in metres, of the hill at `x`, `y` kilometres from its centre: a main summit and a lower
/// shoulder to the south-west, on a plain at 120 m.
fn elevation(x: f64, y: f64) -> f64 {
    let summit = 380.0 * (-(x * x + y * y) / 0.9).exp();
    let shoulder = 160.0 * (-((x + 1.1).powi(2) + (y + 0.8).powi(2)) / 0.35).exp();
    120.0 + summit + shoulder
}

/// Returns the points of a survey, `count` of them on a spiral of the given turns and radius, each on the surface.
fn survey(count: usize, turns: f64, radius: f64, phase: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut x = Vec::with_capacity(count);
    let mut y = Vec::with_capacity(count);
    let mut z = Vec::with_capacity(count);
    for k in 0..count {
        let t = (k as f64 + 0.5) / count as f64;
        let angle = phase + std::f64::consts::TAU * turns * t;
        let r = radius * t.sqrt();
        let (px, py) = (r * angle.cos(), r * angle.sin());
        x.push(px);
        y.push(py);
        // Slightly above the surface, so that the marker is not hidden by the face it stands on.
        z.push(elevation(px, py) + 6.0);
    }
    (x, y, z)
}

pub fn figure() -> Figure {
    let x = linspace(-2.0, 2.0, NODES);
    let y = linspace(-2.0, 2.0, NODES);
    let z = Matrix::from_fn(NODES, NODES, |row, col| elevation(x[col], y[row]));

    let mut fig = Figure::new()
        .size_mm(170.0, 120.0)
        .tiles(1, 2)
        .fit_col(1)
        .title("Survey of a hill")
        .tag("surface")
        .tag("scatter")
        .tag("colormap")
        .tag("legend")
        .tag("measurements")
        .parameter("dimensionality", "3D")
        .parameter("artists", 3)
        .parameter("data_points", (2 * NODES + NODES * NODES + 3 * (14 + 9)) as i64)
        .parameter("has_legend", true);
    let height = fig
        .color_scale()
        .colormap(Colormap::Viridis)
        .limits(100.0, 500.0)
        .tick_step(100.0)
        .title("Elevation (m)")
        .id();
    let key = fig.panel(0, 1).legend().boxed(false).id();

    let mut ax = fig.axes(0, 0);
    ax.surf(&x, &y, &z)
        .color_scale(height)
        .edge_color(Color::rgba(0.0, 0.0, 0.0, 0.25));
    let (gx, gy, gz) = survey(14, 2.0, 1.8, 0.0);
    let gnss = ax
        .scatter3(&gx, &gy, &gz)
        .label("GNSS")
        .marker(Marker::Circle)
        .marker_size(6.0)
        .marker_face(Color::WHITE)
        .marker_edge(Color::BLACK)
        .id();
    let (tx, ty, tz) = survey(9, 1.0, 1.4, 1.3);
    let station = ax
        .scatter3(&tx, &ty, &tz)
        .label("Total station")
        .marker(Marker::TriangleUp)
        .marker_size(7.0)
        .marker_face(Color::rgb(0.85, 0.25, 0.10))
        .marker_edge(Color::BLACK)
        .id();
    ax.xlabel("East (km)")
        .ylabel("North (km)")
        .zlabel("Elevation (m)")
        .view(-35.0, 35.0)
        .legend_in(key);

    // The key lists the two kinds of survey point and then the scale of the surface as a bar item.
    fig.panel(0, 1).legend().item(gnss).item(station).item(height);
    fig
}
