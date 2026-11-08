use ironlab::prelude::*;

pub const TITLE: &str = "Axis sides";
pub const DESCRIPTION: &str = "Each axis drawn on its low side, through the middle of the plot, or on its high side, \
     set with `xaxis_side`, `yaxis_side` and `zaxis_side`. The top row moves the x and y axes of two-dimensional \
     axes to the top and right, through the middle, and back to the default bottom and left. The middle row moves \
     the axes of a three-dimensional box to its highest and rightmost edges, and stands the z axis on the centre of \
     the floor. The bottom row stands the z axis of cylindrical axes on the left wall (the default), up the axis of \
     the cylinder, and on the right wall. In every projection the side is a side of the page, chosen again as the \
     view turns.";

/// The three sides, with the words used for them in the titles of the tiles.
const SIDES: [(AxisSide, &str); 3] = [
    (AxisSide::High, "high"),
    (AxisSide::Centre, "centre"),
    (AxisSide::Low, "low"),
];

pub fn figure() -> Figure {
    let mut fig = Figure::new()
        .size_mm(180.0, 220.0)
        .tiles(3, 3)
        .title("Axis sides")
        .tag("line")
        .tag("subplots")
        .tag("cylindrical")
        .tag("axis-side")
        .parameter("dimensionality", "3D")
        .parameter("artists", 9)
        .parameter("data_points", 3120)
        .parameter("has_legend", false);

    // A damped sine about the origin, which reads naturally against axes through the middle of the plot.
    let x = linspace(-3.0, 3.0, 121);
    let y: Vec<f64> = x.iter().map(|x| (2.0 * x).sin() * (-0.2 * x * x).exp()).collect();
    for (col, (x_side, y_side)) in [
        (AxisSide::High, AxisSide::Low),
        (AxisSide::Centre, AxisSide::Centre),
        (AxisSide::Low, AxisSide::High),
    ]
    .into_iter()
    .enumerate()
    {
        let name = |side: AxisSide| SIDES.iter().find(|(s, _)| *s == side).map_or("", |(_, n)| n);
        let mut ax = fig.axes(0, col as u32);
        ax.plot(&x, &y);
        // Axes through the middle are drawn without the box, as a function is plotted about its origin in a
        // textbook, and with limits beyond the outer ticks, so that no tick label sits on the edge of the plot.
        let centred = x_side == AxisSide::Centre;
        ax.box_on(!centred)
            .xlim(-3.0, 3.0)
            .ylim(if centred { -1.25 } else { -1.0 }, if centred { 1.25 } else { 1.0 })
            .xlabel("$t$")
            .ylabel("$f(t)$")
            .xaxis_side(x_side)
            .yaxis_side(y_side)
            .title(format!("x {}, y {}", name(x_side), name(y_side)));
    }

    // A rising spiral inside a three-dimensional box.
    let t = linspace(0.0, 4.0 * std::f64::consts::PI, 121);
    let sx: Vec<f64> = t.iter().map(|t| t.cos()).collect();
    let sy: Vec<f64> = t.iter().map(|t| t.sin()).collect();
    let sz: Vec<f64> = t.iter().map(|t| t / 4.0).collect();
    let three_d: [(&str, [AxisSide; 3]); 3] = [
        ("x, y and z low", [AxisSide::Low; 3]),
        ("x, y and z high", [AxisSide::High; 3]),
        ("z centre", [AxisSide::Low, AxisSide::Low, AxisSide::Centre]),
    ];
    for (col, (title, [x_side, y_side, z_side])) in three_d.into_iter().enumerate() {
        let mut ax = fig.axes3(1, col as u32);
        ax.plot3(&sx, &sy, &sz);
        ax.xlabel("$x$")
            .ylabel("$y$")
            .zlabel("$z$")
            .xaxis_side(x_side)
            .yaxis_side(y_side)
            .zaxis_side(z_side)
            .title(title);
    }

    // A helix of radius 1 climbing one unit per turn, in cylindrical axes.
    let theta = linspace(0.0, 720.0, 145);
    let radius = vec![1.0; theta.len()];
    let height: Vec<f64> = theta.iter().map(|t| t / 360.0).collect();
    for (col, side) in [AxisSide::Low, AxisSide::Centre, AxisSide::High]
        .into_iter()
        .enumerate()
    {
        let label = SIDES.iter().find(|(s, _)| *s == side).map_or("", |(_, n)| n);
        let mut ax = fig.cylindrical_axes(2, col as u32);
        ax.plot3(&theta, &radius, &height);
        ax.view(-60.0, 20.0)
            .xtick_step(90.0)
            .ylim(0.0, 1.5)
            .ytick_step(0.5)
            .zlim(0.0, 2.0)
            .ztick_step(1.0)
            .zlabel("Height")
            .zaxis_side(side)
            .title(format!("z {label}"));
    }
    fig
}
