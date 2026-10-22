use ironlab::prelude::*;

pub const TITLE: &str = "Equal aspect in three dimensions";
pub const DESCRIPTION: &str = "A unit sphere shown with limits of unequal span, x and z from −1 to 1 and y from \
     −2.5 to 2.5. With an automatic aspect the box stays a cube, so the sphere is squashed along y; at equal aspect \
     kept by the box, the limits are kept and the sides of the box take the lengths of the ranges, so that one unit \
     has the same length along x, y and z and the sphere is round from every view, however the figure is rotated. \
     The third panel draws a low hill at a ratio of z of 4, the vertical exaggeration of a terrain map, so that its \
     box is four times as tall as the hill's height alone would make it.";

/// The number of rows and columns of the grids of the sphere and of the hill.
const N: usize = 33;

pub fn figure() -> Figure {
    let mut fig = Figure::new()
        .size_mm(180.0, 80.0)
        .tiles(1, 3)
        .title("A unit sphere and an exaggerated hill")
        .tag("surface")
        .tag("color-scale")
        .tag("subplots")
        .tag("aspect")
        .parameter("dimensionality", "3D")
        .parameter("artists", 3)
        .parameter("data_points", (2 * 3 * N * N + 2 * N + N * N) as i64)
        .parameter("has_legend", false);

    // The sphere, parametrised by the polar angle along the rows and the azimuth along the columns.
    let polar = |row: usize| std::f64::consts::PI * row as f64 / (N - 1) as f64;
    let azimuth = |col: usize| std::f64::consts::TAU * col as f64 / (N - 1) as f64;
    let x = Matrix::from_fn(N, N, |row, col| polar(row).sin() * azimuth(col).cos());
    let y = Matrix::from_fn(N, N, |row, col| polar(row).sin() * azimuth(col).sin());
    let z = Matrix::from_fn(N, N, |row, _| polar(row).cos());
    for (col, (aspect, title)) in [
        (Aspect::Auto, "Automatic aspect"),
        (Aspect::Equal, "Equal aspect"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut ax = fig.axes(0, col as u32);
        ax.surf(x.clone(), y.clone(), &z);
        ax.title(title)
            .aspect(aspect)
            .xlim(-1.0, 1.0)
            .ylim(-2.5, 2.5)
            .zlim(-1.0, 1.0)
            .xlabel("$x$")
            .ylabel("$y$")
            .zlabel("$z$");
    }

    // A hill 0.5 high over a square 10 wide, drawn at four times its height.
    let ground = linspace(-5.0, 5.0, N);
    let hill = Matrix::from_fn(N, N, |row, col| {
        let (x, y) = (ground[col], ground[row]);
        0.5 * (-(x * x + y * y) / 8.0).exp()
    });
    let mut ax = fig.axes(0, 2);
    ax.surf(ground.clone(), ground, &hill);
    ax.title("Height drawn 4 times as long")
        .aspect(Aspect::Ratio { y: 1.0, z: 4.0 })
        .xlabel("$x$")
        .ylabel("$y$")
        .zlabel("$h$");
    fig
}
