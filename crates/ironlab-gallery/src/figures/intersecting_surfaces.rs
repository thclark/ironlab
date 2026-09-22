use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Intersecting surfaces";
pub const DESCRIPTION: &str = "A saddle and an inclined plane that cut through each other, a helix threading \
     through both, and scattered points on either side of the sheets. The viewer resolves every crossing with a \
     depth buffer. No back-to-front order of these faces, segments and markers can draw the picture, so the \
     exported PDF embeds the axes as an image and the export report says so.";

/// The number of turns the helix makes between the floor and the ceiling of the box.
const TURNS: f64 = 2.0;

pub fn figure() -> Figure {
    let grid = linspace(-1.0, 1.0, 25);
    let saddle = Matrix::from_fn(grid.len(), grid.len(), |row, col| {
        let (x, y) = (grid[col], grid[row]);
        x * x - y * y
    });
    let plane = Matrix::from_fn(grid.len(), grid.len(), |row, col| 0.4 * grid[col] + 0.15 * grid[row]);

    let t = linspace(0.0, TURNS * std::f64::consts::TAU, 400);
    let helix_x: Vec<f64> = t.iter().map(|t| 0.7 * t.cos()).collect();
    let helix_y: Vec<f64> = t.iter().map(|t| 0.7 * t.sin()).collect();
    let helix_z: Vec<f64> = t
        .iter()
        .map(|t| -1.0 + 2.0 * t / (TURNS * std::f64::consts::TAU))
        .collect();

    let (points_x, points_y) = sunflower(60, 0.95);
    let points_z: Vec<f64> = points_x
        .iter()
        .zip(&points_y)
        .map(|(&x, &y)| 0.6 * (3.0 * x).sin() * (2.0 * y).cos())
        .collect();

    let mut fig = Figure::new()
        .size_mm(160.0, 100.0)
        .title("A saddle, a plane, a helix and points that pass through one another");
    let mut ax = fig.axes3(0, 0);
    ax.surf(&grid, &grid, &saddle).edge_width(0.25);
    ax.surf(&grid, &grid, &plane)
        .face_color(Color::rgb(0.85, 0.85, 0.85))
        .edge_color(Color::rgb(0.55, 0.55, 0.55))
        .edge_width(0.25);
    ax.plot3(&helix_x, &helix_y, &helix_z)
        .color(Color::rgb(0.75, 0.1, 0.1))
        .line_width(1.2);
    ax.scatter3(&points_x, &points_y, &points_z)
        .color(Color::rgb(0.1, 0.1, 0.5))
        .size(3.0)
        .filled();
    ax.xlabel("$x$").ylabel("$y$").zlabel("$z$");
    fig
}
