use ironlab::prelude::*;

use crate::fields::*;

pub const TITLE: &str = "Indexed image";
pub const DESCRIPTION: &str = "The Julia field quantised into eight classes and drawn twice as a colour-indexed \
     image, whose pixels name entries of the colormap directly. On the left the classes are stored as bytes; on the \
     right they are floating-point indices with a frame of pixels at index 300, beyond the end of the colormap, \
     which the image's policy for indices above the colormap draws in a fixed colour.";

/// The number of classes into which the field is quantised.
const CLASSES: usize = 8;

/// The width, in pixels, of the frame of out-of-range indices around the second image.
const FRAME: usize = 6;

/// An index beyond the last entry of the colormap.
const BEYOND_THE_COLORMAP: f64 = 300.0;

pub fn figure() -> Figure {
    let (_, _, z) = julia_grid(121);
    let classes = quantise(&z, CLASSES);
    let (rows, cols) = (classes.rows(), classes.cols());
    let framed = Matrix::from_fn(rows, cols, |row, col| {
        let in_frame = row < FRAME || row >= rows - FRAME || col < FRAME || col >= cols - FRAME;
        if in_frame {
            BEYOND_THE_COLORMAP
        } else {
            f64::from(classes[(row, col)])
        }
    });

    let mut fig = Figure::new()
        .size_mm(160.0, 90.0)
        .tiles(1, 2)
        .title(r"Eight classes of $\ln(1 + |z_3|)$")
        .label("2d")
        .label("image")
        .label("subplots")
        .label("colormap")
        .label("comparison")
        .label("out-of-range")
        .parameter("kind", "image")
        .parameter("dimensionality", "2D")
        .parameter("artists", 2)
        .parameter("data_points", 29_282)
        .parameter("has_legend", false);
    let mut ax = fig.axes(0, 0);
    ax.indexed_image(&classes)
        .pixel_columns(DOMAIN_MIN, DOMAIN_MAX)
        .pixel_rows(DOMAIN_MIN, DOMAIN_MAX);
    ax.title("Byte indices").xlabel("$x$").ylabel("$y$");

    let mut ax = fig.axes(0, 1);
    ax.indexed_image(&framed)
        .pixel_columns(DOMAIN_MIN, DOMAIN_MAX)
        .pixel_rows(DOMAIN_MIN, DOMAIN_MAX)
        .above(Color::rgb(0.8, 0.8, 0.8));
    ax.title("Floating-point indices, framed at 300")
        .xlabel("$x$")
        .ylabel("$y$");
    fig
}
