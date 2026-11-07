use std::f64::consts::PI;

use ironlab::prelude::*;

pub const TITLE: &str = "Emboldened for slides";
pub const DESCRIPTION: &str = "The damped oscillator of [lines and markers](line_markers.md), with its decay on a \
     logarithmic axis beside it, drawn at the semibold step of emboldening for a projector. Plain text is set in \
     STIX Two Text SemiBold, the mathematics of the labels and of the logarithmic tick labels is thickened to match, \
     and every line, edge and tick is drawn about one and a half times as thick, while the markers keep their \
     sizes and the ticks their lengths. The same figure is exported for print by leaving `embolden` at its default, \
     or drawn at any step on export without being changed; see [figures for slides]\
     (../guides/getting-started.md#figures-for-slides).";

pub fn figure() -> Figure {
    let t = linspace(0.0, 4.0 * PI, 49);
    // A cosine whose amplitude decays within the envelope e^(-t/4).
    let envelope: Vec<f64> = t.iter().map(|t| (-t / 4.0).exp()).collect();
    let damped_cosine: Vec<f64> = t.iter().map(|t| (-t / 4.0).exp() * t.cos()).collect();
    // The energy of the oscillator decays twice as fast as its amplitude.
    let energy: Vec<f64> = t.iter().map(|t| (-t / 2.0).exp()).collect();

    let mut fig = Figure::new()
        .size_mm(160.0, 80.0)
        .tiles(1, 2)
        .embolden(Embolden::Semibold)
        .title("A damped oscillator")
        .tag("embolden")
        .tag("slides")
        .tag("subplots")
        .tag("line")
        .tag("legend")
        .tag("log")
        .tag("latex")
        .tag("markers")
        .parameter("dimensionality", "2D")
        .parameter("artists", 3)
        .parameter("data_points", 294)
        .parameter("has_legend", true);

    let mut ax = fig.axes(0, 0);
    ax.plot(&t, &damped_cosine)
        .label(r"$e^{-t/4} \cos t$")
        .marker(Marker::Circle)
        .marker_size(3.5);
    ax.plot(&t, &envelope)
        .label(r"$e^{-t/4}$")
        .color(Color::rgb(0.4, 0.4, 0.4))
        .dash(Dash::Dashed);
    ax.title(r"\textbf{Displacement}")
        .xlabel(r"Time $t$ (s)")
        .ylabel(r"$x(t)$ (m)")
        .grid(true)
        .legend(LegendLocation::NorthEast);

    let mut ax = fig.axes(0, 1);
    ax.semilogy(&t, &energy).label(r"$E / E_0$");
    ax.title(r"\textbf{Energy}")
        .xlabel(r"Time $t$ (s)")
        .ylabel(r"$E / E_0$")
        .grid(true);

    fig
}
