use ironlab::prelude::*;

pub const TITLE: &str = "Indexed palette";
pub const DESCRIPTION: &str = "A synthetic 8-bit elevation raster of a mountainous coast, stored as one byte per \
     pixel as many older terrain formats store it, and drawn through the 256-entry palette that travels with it. \
     Codes 0 to 63 are water, 64 to 191 are land from sea level to the snow line at 2000 m, and 192 to 255 are ice, \
     so each code is a step of about 16 m. The colourbar of an indexed scale draws one block per entry, so its 256 \
     blocks are the palette itself, and its ticks sit at the centres of the entries they name: here three entries in \
     the middle of the three ranges are labelled by name rather than by number.";

/// The number of samples east to west.
const COLUMNS: usize = 240;

/// The number of samples south to north.
const ROWS: usize = 160;

/// The palette code of sea level.
const SEA_LEVEL_CODE: f64 = 64.0;

/// The height of one palette code, in metres: the 128 land codes span sea level to the snow line at 2000 m.
const METRES_PER_CODE: f64 = 2000.0 / 128.0;

/// Returns the elevation, in metres, at a point `x` kilometres east and `y` kilometres north of the origin: a coast
/// that runs north to south with a ridge of three summits inland of it.
fn elevation(x: f64, y: f64) -> f64 {
    let coast = 900.0 * ((x - 40.0) / 12.0).tanh() - 300.0;
    let summits = [(95.0, 70.0, 2900.0, 14.0), (120.0, 40.0, 2500.0, 12.0), (70.0, 30.0, 1500.0, 10.0)];
    let peaks: f64 = summits
        .iter()
        .map(|&(cx, cy, h, w)| h * (-((x - cx).powi(2) + (y - cy).powi(2)) / (2.0 * w * w)).exp())
        .sum();
    coast + peaks
}

/// Returns the palette code of an elevation, clamped to the 256 codes.
fn code(elevation: f64) -> u8 {
    (SEA_LEVEL_CODE + (elevation / METRES_PER_CODE).floor()).clamp(0.0, 255.0) as u8
}

/// Returns `count` colours interpolated linearly between the given colours, which are evenly spaced along the ramp,
/// from the first colour to the last.
fn ramp(colors: &[Color], count: usize) -> Vec<Color> {
    let last = colors.len() - 1;
    (0..count)
        .map(|k| {
            let position = k as f32 / (count - 1) as f32 * last as f32;
            let i = (position.floor() as usize).min(last - 1);
            let (a, b, f) = (colors[i], colors[i + 1], position - i as f32);
            Color::rgb(a.r + (b.r - a.r) * f, a.g + (b.g - a.g) * f, a.b + (b.b - a.b) * f)
        })
        .collect()
}

pub fn figure() -> Figure {
    let x = linspace(0.0, 150.0, COLUMNS);
    let y = linspace(0.0, 100.0, ROWS);
    let codes = ByteMatrix::from_fn(ROWS, COLUMNS, |row, col| code(elevation(x[col], y[row])));

    // The palette: 64 blues for water, 128 greens and browns for land and 64 greys for ice.
    let mut palette = ramp(&[Color::rgb(0.05, 0.15, 0.40), Color::rgb(0.55, 0.78, 0.92)], 64);
    palette.extend(ramp(
        &[
            Color::rgb(0.25, 0.55, 0.25),
            Color::rgb(0.75, 0.75, 0.45),
            Color::rgb(0.55, 0.40, 0.25),
        ],
        128,
    ));
    palette.extend(ramp(&[Color::rgb(0.78, 0.80, 0.82), Color::WHITE], 64));

    let mut fig = Figure::new()
        .size_mm(160.0, 95.0)
        .tiles(1, 2)
        .fit_col(1)
        .title("An 8-bit elevation raster and its palette")
        .tag("image")
        .tag("color-scale")
        .tag("colorbar")
        .tag("aspect")
        .tag("map")
        .parameter("dimensionality", "2D")
        .parameter("artists", 1)
        .parameter("data_points", 38_400)
        .parameter("has_legend", false);
    let terrain = fig
        .indexed_scale(palette)
        .tick_labels([(32, "water"), (128, "land"), (224, "ice")])
        .title("Terrain code")
        .id();

    let mut ax = fig.axes(0, 0);
    ax.scalar_image(&codes)
        .pixel_columns(x[0], x[COLUMNS - 1])
        .pixel_rows(y[0], y[ROWS - 1])
        .color_scale(terrain);
    ax.xlabel("Distance east (km)")
        .ylabel("Distance north (km)")
        .aspect(Aspect::Equal);
    fig.panel(0, 1)
        .colorbar(terrain)
        .title_placement(ColorbarTitlePlacement::Above);
    fig
}
