use ironlab::prelude::*;

pub const TITLE: &str = "Equal aspect kept by the box and by the limits";
pub const DESCRIPTION: &str = "A unit circle and the square around it, drawn at equal aspect in two tiles of \
     unequal weight. On the left the aspect is kept by the box: the manual limits are drawn exactly, and the plot \
     area shrinks to their shape and is centred in its tile. On the right it is kept by the limits: the plot area \
     fills its tile, and the manual x limits widen symmetrically about their centre. In both, the circle is round and the \
     square is square, as they stay while the figure is panned and zoomed in the viewer.";

pub fn figure() -> Figure {
    let t = linspace(0.0, std::f64::consts::TAU, 181);
    let x: Vec<f64> = t.iter().map(|t| t.cos()).collect();
    let y: Vec<f64> = t.iter().map(|t| t.sin()).collect();
    let square_x = [-1.0, 1.0, 1.0, -1.0, -1.0];
    let square_y = [-1.0, -1.0, 1.0, 1.0, -1.0];

    let mut fig = Figure::new()
        .size_mm(160.0, 70.0)
        .tiles(1, 2)
        .col_weights([1.0, 2.0])
        .title("A unit circle at equal aspect")
        .tag("line")
        .tag("subplots")
        .tag("aspect")
        .parameter("dimensionality", "2D")
        .parameter("artists", 4)
        .parameter("data_points", 2 * 2 * (181 + 5))
        .parameter("has_legend", false);
    for (col, (adjust, title)) in [
        (AspectAdjust::Box, "Kept by the box"),
        (AspectAdjust::Limits, "Kept by the limits"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut ax = fig.axes(0, col as u32);
        ax.plot(&x, &y).color(Color::BLACK);
        ax.plot(square_x, square_y)
            .color(Color::BLACK)
            .dash(Dash::LongDash);
        ax.title(title)
            .title_align(TitleAlign::Left)
            .aspect(Aspect::Equal)
            .aspect_adjust(adjust)
            .xlim(-1.25, 1.25)
            .ylim(-1.25, 1.25)
            .xtick_step(0.5)
            .ytick_step(0.5)
            .xlabel("$x$")
            .ylabel("$y$");
    }
    fig
}
