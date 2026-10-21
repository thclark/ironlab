use ironlab::prelude::*;

pub const TITLE: &str = "Pressure distribution on a reversed axis";
pub const DESCRIPTION: &str = "The pressure coefficient and the surface speed of the potential flow around a \
     Joukowski aerofoil at an angle of attack of 4°, computed exactly by conformal mapping. The pressure \
     coefficient is plotted on a reversed y axis, as aerodynamicists draw it, so that suction (negative $C_p$) is \
     upwards; the limits are still given lower bound first, and a datatip reports $C_p$ as stored rather than its \
     negation. The two panels share the chordwise axis through an x link, which shares limits only, so the speed \
     below keeps its usual direction. Only the bottom panel labels its x ticks, and the plot areas keep the 3 : 2 \
     ratio of the row weights all the same.";

/// The centre of the circle that the Joukowski map turns into the aerofoil, in the plane of the circle: its real
/// part sets the thickness and its imaginary part the camber.
const CENTRE: (f64, f64) = (-0.09, 0.06);
/// The angle of attack, in degrees.
const ALPHA_DEG: f64 = 4.0;

/// One point of the surface: its chordwise position from 0 at the leading edge to 1 at the trailing edge, its
/// pressure coefficient and its speed relative to the free stream.
struct SurfacePoint {
    x: f64,
    cp: f64,
    speed: f64,
}

/// Computes the flow around the aerofoil at `n` points of the circle and returns the upper and the lower surface,
/// each from the leading edge to the trailing edge.
fn surfaces(n: usize) -> (Vec<SurfacePoint>, Vec<SurfacePoint>) {
    let (mx, my) = CENTRE;
    let radius = ((1.0 - mx).powi(2) + my * my).sqrt();
    let alpha = ALPHA_DEG.to_radians();
    // The angle of the trailing edge on the circle, where the Kutta condition fixes the circulation.
    let beta = (my / (1.0 - mx)).atan();
    let circulation = 4.0 * std::f64::consts::PI * radius * (alpha + beta).sin();

    let mut points: Vec<(f64, f64, f64)> = (1..n)
        .map(|k| {
            let theta = -beta + 2.0 * std::f64::consts::PI * k as f64 / n as f64;
            // A point of the circle, relative to its centre and absolutely.
            let (rx, ry) = (radius * theta.cos(), radius * theta.sin());
            let (zx, zy) = (mx + rx, my + ry);
            // The complex velocity in the plane of the circle: free stream, doublet and vortex.
            let r2 = rx * rx + ry * ry;
            let (inv_x, inv_y) = (rx / r2, -ry / r2); // 1 / (ζ − μ)
            let (inv2_x, inv2_y) = (inv_x * inv_x - inv_y * inv_y, 2.0 * inv_x * inv_y);
            let (ex, ey) = (alpha.cos(), alpha.sin());
            let doublet = (radius * radius * (ex * inv2_x - ey * inv2_y), radius * radius * (ex * inv2_y + ey * inv2_x));
            let vortex = (-circulation / (2.0 * std::f64::consts::PI) * inv_y, circulation / (2.0 * std::f64::consts::PI) * inv_x);
            let w = (ex - doublet.0 + vortex.0, -ey - doublet.1 + vortex.1);
            // The derivative of the map z = ζ + 1/ζ, by which the velocity is divided.
            let d = zx * zx + zy * zy;
            let (iz2_x, iz2_y) = ((zx * zx - zy * zy) / (d * d), -2.0 * zx * zy / (d * d));
            let derivative = (1.0 - iz2_x, -iz2_y);
            let speed = (w.0.hypot(w.1)) / derivative.0.hypot(derivative.1);
            // The position in the plane of the aerofoil.
            let x = zx + zx / d;
            (x, 1.0 - speed * speed, speed)
        })
        .collect();
    let (lo, hi) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
    for p in &mut points {
        p.0 = (p.0 - lo) / (hi - lo);
    }
    // The circle is traversed from the trailing edge over the upper surface to the leading edge, then back along
    // the lower surface.
    let leading = points
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.0.total_cmp(&b.1.0))
        .map(|(k, _)| k)
        .expect("the surface has points");
    let point = |&(x, cp, speed): &(f64, f64, f64)| SurfacePoint { x, cp, speed };
    let upper = points[..=leading].iter().rev().map(point).collect();
    let lower = points[leading..].iter().map(point).collect();
    (upper, lower)
}

pub fn figure() -> Figure {
    let (upper, lower) = surfaces(240);
    let mut fig = Figure::new()
        .size_mm(140.0, 120.0)
        .tiles(2, 1)
        .row_weights([3.0, 2.0])
        .title(format!("Joukowski aerofoil at $\\alpha = {ALPHA_DEG}^\\circ$"))
        .tag("line")
        .tag("subplots")
        .tag("legend")
        .tag("linked-axes")
        .tag("ticks")
        .tag("reversed-axes")
        .parameter("dimensionality", "2D")
        .parameter("artists", 4)
        .parameter("data_points", 960)
        .parameter("has_legend", true);

    let column = |points: &[SurfacePoint], value: fn(&SurfacePoint) -> f64| -> Vec<f64> {
        points.iter().map(value).collect()
    };
    let surfaces = [("upper surface", &upper, Dash::Solid), ("lower surface", &lower, Dash::Dashed)];

    let mut cp = fig.axes(0, 0);
    for (name, points, dash) in surfaces {
        cp.plot(column(points, |p| p.x), column(points, |p| p.cp))
            .label(name)
            .color(Color::BLACK)
            .dash(dash);
    }
    cp.ylabel("$C_p$")
        .ylim(-2.0, 1.0)
        .yreverse(true)
        .ytick_step(0.5)
        .xlim(0.0, 1.0)
        .xtick_step(0.2)
        .xtick_labels_hidden()
        .grid(true)
        .legend(LegendLocation::SouthEast);
    let cp = cp.id();

    let mut speed = fig.axes(1, 0);
    for (name, points, dash) in surfaces {
        speed
            .plot(column(points, |p| p.x), column(points, |p| p.speed))
            .label(name)
            .color(Color::BLACK)
            .dash(dash);
    }
    speed
        .xlabel("Chordwise position $x/c$")
        .ylabel("$U_e/U_\\infty$")
        .ylim(0.0, 1.6)
        .xtick_step(0.2)
        .grid(true);
    let speed = speed.id();

    fig.link(Dim::X, &[cp, speed])
        .expect("both axes belong to this figure");
    fig
}
