use ironlab::prelude::*;

use crate::fields::hsl_to_rgb;

pub const TITLE: &str = "Reference and variants";
pub const DESCRIPTION: &str = "The lift, drag and pitching-moment coefficients of a synthetic aerofoil polar at a \
     base case and at six levels of an input perturbation, in a stack of three plots that share the angle of attack. \
     The base case is drawn last, so that the family never hides it, but the key beneath the plots lists it first, on \
     a row of its own, followed by the perturbation levels in columns. The key sits in a row fitted to its entries, \
     so it neither clips them nor takes more height than they need.";

/// The perturbation levels, in per cent of the input.
const LEVELS: [f64; 6] = [0.5, 1.0, 2.0, 4.0, 8.0, 16.0];

/// Returns the lift coefficient at angle of attack `alpha` (in degrees) of a section whose lift slope is scaled by
/// `slope` and which stalls gradually above 12°.
fn lift(alpha: f64, slope: f64) -> f64 {
    let linear = 2.0 * std::f64::consts::PI * alpha.to_radians() * slope;
    let stall = 1.0 / (1.0 + ((alpha - 12.0) / 2.5).exp());
    0.25 * slope + linear * stall + (1.0 - stall) * 1.25
}

pub fn figure() -> Figure {
    let alpha = linspace(-4.0, 16.0, 41);

    let mut fig = Figure::new()
        .size_mm(160.0, 170.0)
        .tiles(4, 1)
        .fit_row(3)
        .title("Input sensitivity of an aerofoil polar")
        .label("line")
        .label("subplots")
        .label("legend")
        .label("markers")
        .label("simulations")
        .parameter("dimensionality", "2D")
        .parameter("artists", 21)
        .parameter("data_points", 1722)
        .parameter("has_legend", true);
    let key = fig.panel_legend(3, 0).id();

    // The levels are drawn first, from the smallest perturbation in pale violet to the largest in red, and the base
    // case last, in black with filled markers, so that it lies on top of the family.
    let ramp = |k: usize| hsl_to_rgb(0.75 * (1.0 - k as f64 / 5.0), 0.65, 0.5);
    let mut levels = Vec::new();
    let mut base = None;
    for (row, quantity) in ["lift", "drag", "moment"].into_iter().enumerate() {
        let curve = |slope: f64| -> Vec<f64> {
            alpha
                .iter()
                .map(|&a| {
                    let cl = lift(a, slope);
                    match quantity {
                        "lift" => cl,
                        "drag" => 0.006 + 0.008 * (slope - 1.0).abs() + 0.012 * cl * cl,
                        _ => -0.05 - 0.02 * (slope - 1.0) + 0.004 * a,
                    }
                })
                .collect()
        };
        let mut ax = fig.axes(row as u32, 0);
        for (k, level) in LEVELS.into_iter().enumerate() {
            let id = ax
                .plot(&alpha, curve(1.0 + level / 100.0))
                .display_name(format!("Perturbed by {level} %"))
                .color(ramp(k))
                .id();
            if row == 0 {
                levels.push(id);
            }
        }
        let id = ax
            .plot(&alpha, curve(1.0))
            .display_name("Base case (unperturbed inputs)")
            .color(Color::rgb(0.0, 0.0, 0.0))
            .line_width(1.5)
            .marker(Marker::Circle)
            .marker_size(3.5)
            .marker_face(Color::rgb(0.0, 0.0, 0.0))
            .id();
        base.get_or_insert(id);
        ax.grid(true).legend_in(key);
    }
    fig.axes(0, 0).ylabel("Lift $C_L$");
    fig.axes(1, 0).ylabel("Drag $C_D$");
    fig.axes(2, 0)
        .ylabel("Moment $C_m$")
        .xlabel(r"Angle of attack $\alpha$ (°)");
    fig.link_all_x()
        .expect("every axes of the figure can be linked");

    // The key names the artists of the top plot; each item also stands for the artists of the same name and look in
    // the other plots, so clicking it in the viewer hides or shows that case in all three.
    let mut panel = fig.panel_legend(3, 0);
    panel.full_row(base.expect("the figure has a base case"));
    for id in levels {
        panel.item(id);
    }
    fig
}
